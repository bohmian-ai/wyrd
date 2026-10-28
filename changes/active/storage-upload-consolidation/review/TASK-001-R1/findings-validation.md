# TASK-001 R1 — independent findings validation

Recommendation: **FIX_REQUIRED**. Five deduplicated material findings remain. No specification revision is needed. No additional discovery blocker was introduced by this validation.

## Subject and method

Repository `/Users/stevenforrester/Documents/GitHub/wyrd`; first-round full subject `3899b0a35...826fcf9c742a36bcae5e8f68307e76685d88244e`. HEAD independently matched the candidate. No earlier acceptance or finding IDs exist. Source remained immutable; this report is the only file written by this reviewer. No Cargo command was run.

Inputs: complete task-review skill; `scope.md`, `verification.md`, `claim-comparison.md`; all six selected discovery reports; approved specification revision 2; original TASK-001 and its execution evidence; root and SQL-directory instructions, spec-driven-development authority order, maintainer guide, Rust diagnostics/rustdoc rules, and the applicable Card, inventory/outbox and scoped-read/peer security authorities. No `.codegraph/` directory exists.

This is source validation and acceptance reconciliation, not another comprehensive discovery pass. I inspected the actual range and full functions at the proposed correction boundaries, traced production callers and sibling writers, and compared the existing completion, query, delivery, cleanup and test-environment owners. Report agreement was not treated as proof. No follow-up discovery is necessary: the failed-Card wording conflict resolves directly from SQL and Card source, and none of the retained corrections requires a new public, persisted, concurrency or resource-ownership decision.

## Disposition of every proposed claim

| Discovery source ID | Disposition | Independent determination |
|---|---|---|
| BEH-001 | CONFIRMED | Hydration ignores its already-returned envelope fact and reads the SQL pointer. Consolidated as FIND-TASK-001-1. |
| DATA-1 | CONFIRMED | Same cause and correction as BEH-001; no separate finding. |
| BEH-002 | REVISED | Missing compensation tombstone is real. Correction must durably preserve Failed plus cleanup pending before introducing an ACK wait; see FIND-TASK-001-2. |
| DATA-2 | REVISED | Same missing writer. Reject the sentence claiming normal getters reject Failed Cards: the SQL getter excludes only Deleted. Keep the canonical-inventory defect and narrower source-backed listing/download consequence. |
| SYS-001 | REVISED | Same missing writer and recovery boundary as BEH-002. The selected correction makes the Failed/pending ordering explicit and does not move physical deletion into readers or terminate shared service work. |
| REPO-001 | CONFIRMED | Every enumerated rustdoc omission is in a new or materially changed item. Mandatory changed-item documentation gate; FIND-TASK-001-3. |
| REPO-002 | CONFIRMED | The new named migration proof can return green before any assertion when DSNs are missing/invalid. SQL directory rule explicitly forbids it; FIND-TASK-001-4. |
| REPO-003 | CONFIRMED | Both changed storage handlers lack handler-level scrubbed instrumentation. Root rule covers server handlers, including this read handler; FIND-TASK-001-5. |

No proposal is rejected wholesale. Rejected wording and unsafe correction variants create no remediation work. The maintainer and security reports propose no claims; their scoped PASS results do not waive the behavioral or mandatory-gate failures above.

## Final stable finding ledger

### FIND-TASK-001-1 — require the completed envelope during hydration

**Status:** CONFIRMED. **Discovery:** BEH-001, DATA-1. **Classification:** INCORRECT.

**Obligation:** REQ-004/005/013 and INV-001; TASK-001 V1/V3 explicitly include hydration and envelopes. Retained SQL pointers can identify an object but cannot replace missing Bifrost completion facts.

**Location:** `crates/wyrd/wyrd-server/src/components/cards/service.rs:363` (`hydrate_card`, particularly the pointer read at 371–390); related `card_files:332` and `components/storage/inventory.rs:239,282,331`.

**Producer → consumer evidence:** `write_card_blob:2775` writes the deterministic envelope, records `ObjectOrigin::CardEnvelope` through `ObjectInventory::record`, then returns its URI. The inventory's ACK is explicitly not an unconditional read-availability guarantee. `card_objects` uses the scoped query and `fold`; no envelope row returns `CardObjects.envelope=None`. `card_files` tests only completed manifest files. All three hydrated getters (`get_card_by_uid`, `get_card_by_ref`, `get_latest_card`) resolve through that function and then call `hydrate_card`. Hydration validates and reads `row.card_blob_uri` but never consumes `inventory.envelope`; its remaining integrity work consumes files. A metadata-only Active Card has no manifest to trip file readiness. A file-bearing Card also passes when its file rows are visible and its envelope row is absent. The sole production `card_objects` consumer is `card_files`; tests inspect envelopes, but healthy envelope assertions do not make hydration depend on them.

**Consequence and materiality:** A supported delayed-visibility read succeeds from a Postgres pointer and backend bytes while Bifrost does not establish the envelope's completion. This directly violates the approved sole-authority and missing-inventory obligations; it is not malformed-input hardening or a demand for immediate availability after ACK.

**Decision-complete minimal correction:** At the existing hydration boundary, after preserving the current missing-pointer/not-active behavior, require the already-resolved completed envelope. An absent required fact returns `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE`. Use its recorded location consistently with the retained pointer, enforcing the existing tenant/Card and pointer identity validation before object IO. Preserve URI, blob identity and integrity validation and all file-only readers' behavior. No extra query, envelope resolver, cache, new error family, hash pass or session is needed. This consumer guard belongs in hydration because that operation consumes the envelope; artifact listing and selected-file downloads do not.

**Burden and alternatives:** Small local use of an existing projection plus focused proof. Deleting envelope recording violates the task. Tightening only manifests cannot establish envelope completion. Adding a second resolver duplicates existing query/folding work.

**Focused closure proof:** Read an otherwise valid metadata-only Active Card with its SQL pointer and backend envelope intact but no visible envelope fact; assert the exact retryable code. Make the matching fact visible and read successfully. Preserve the file-bearing round trip and existing invalid pointer/tenant/Card checks. Reuse the current server/controlled-read fixtures; do not add a general query harness or broad fault framework.

### FIND-TASK-001-2 — record compensation deletion before clearing cleanup

**Status:** REVISED. **Discovery:** BEH-002, DATA-2, SYS-001. **Classification:** MISSING.

**Obligation:** REQ-018 and TASK-001's replacement of all existing deletion writers; owning registry/Bifrost authority requires completed artifacts, envelopes and existing lifecycle deletion facts in one inventory.

**Location:** `crates/wyrd/wyrd-server/src/components/cards/service.rs:1952` (`reconcile_card_claim` initial abandonment), `:2295` (`abort_card`), and `:2662` (`commit_card_failure`). Existing reusable deletion owner is `record_card_deletion:2576`.

**Producer → consumer evidence:** Public storage completion records file A. In a supported two-file declaration, sibling B can fail, abort or expire before activation. `manifest_needs_cleanup:2347` excludes completed/live uploads but recognizes unfinished expired/failed/aborted ones. `reconcile_card_claim` then invokes `cleanup_card_artifacts:2512`; `cleanup_manifest_artifact` physically deletes A in its completed branch. `abort_pending_declaration_cards` also reaches `abort_card` for declaration compensation. Both initial callers pass physical-cleanup success to `commit_card_failure`, without a deletion fact. `fail_card` (`wyrd-sql/src/queries/cards/lifecycle.rs:555`) writes Failed plus cleanup pending, but `commit_card_failure` immediately clears that reconciliation in the same transaction when cleanup succeeded. The successful first attempt therefore has no later cleanup retry to record deletion.

I enumerated all four `cleanup_card_artifacts` callers and both `commit_card_failure` callers. The sibling explicit deletion (`finish_card_delete`) and claimed cleanup retry (`reconcile_cleanup_claim`) correctly record the same tombstone after physical cleanup and before success. Their physical deletion authority, lease checks and retry ownership must remain intact. `ObjectInventory::fold` already gives an existing tombstone dominance over stale completion regardless of order; it cannot invent a missing tombstone.

**Failed-Card discrepancy resolved:** `wyrd-sql/src/queries/cards/get.rs:17` excludes only `status='deleted'`; Failed rows remain readable. `card_manifests` uses that getter, and listing/download go through `card_files`, which has no Failed-status refusal and can find A's still-completed manifest and inventory entry. Thus listing can advertise removed A and download initialization can issue its local/signed plan. Do not claim successful hydrated Failed-Card reads generally: an absent envelope pointer or removed bytes can fail hydration independently. DATA-2's blanket getter-refusal sentence is rejected, not a basis for remediation. Direct authorized Bifrost inventory queries also retain the stale completed fact.

**Consequence and materiality:** Canonical inventory reports a completed available object after supported authorized cleanup removed it. Cleanup is durably marked finished, so a process restart does not repair this omission; stale completion has no deletion to dominate it. Although the old SQL branches also omitted a delete, this finding rests on the explicit new inventory/deletion obligation, not unrelated historical debt.

**Decision-complete minimal correction:** Retain existing physical cleanup and the Pending→Failed transition. For the two initial compensation outcomes, persist Failed with existing cleanup reconciliation pending before waiting on the reused `record_card_deletion` operation. Physical cleanup success alone must no longer clear reconciliation. Once physical cleanup has succeeded and the deletion fact meets the existing ACK contract, mark cleanup succeeded through the existing SQL lifecycle owner. If inventory recording fails or is cancelled, the durable Failed/pending cleanup must survive so the existing `reconcile_cleanup_claim` retries. Retain explicit-delete and claimed-retry paths' current transition/lease ownership and avoid duplicate tombstone ownership within each attempt. Group this as one compensation cleanup-completion correction; no per-reader status filter, per-file tombstones, extra queue or repair scan.

**Correction boundary and burden:** A bounded change to the existing compensation completion ordering, using `fail_card`, `record_card_deletion`, and `mark_card_reconciliation_succeeded`. Simply inserting a fallible inventory call before `commit_card_failure` is insufficient: its error or cancellation could skip the previously durable Failed transition. Merely clearing success later without the pending state also loses retry ownership. Adding reader guards hides invalid inventory rather than repairing its producer. No new durable state, retention rule, service or expensive architecture decision is required.

**Focused closure proof:** Complete A on a pending multi-file Card, expire/fail B through the existing lifecycle, run the initial reconciler cleanup, and assert A bytes removed, Failed retained, inventory deleted/empty, and stale A completion still dominated. Hold/refuse deletion recording at this boundary: Failed and cleanup pending must already be durable; retry through the existing cleanup claim and confirm success clears only after accepted deletion. Exercise the abort sibling's same completion rule with focused coverage if it does not share the corrected boundary. Preserve existing physical-cleanup failures, explicit deletion and lease-fenced retry tests. Use existing Postgres/server/outbox seams, without new load or fault infrastructure.

### FIND-TASK-001-3 — complete mandatory changed-item rustdoc

**Status:** CONFIRMED. **Discovery:** REPO-001. **Classification:** VIOLATION.

**Obligation:** Root Code rule explicitly blocks merge for missing changed-item rustdoc; `agent_docs/rust.md:233` includes private/test helpers and fields, Errors, Panics, and durable partial-progress/retry contracts.

**Validated locations:**

- `crates/wyrd/wyrd-storage/src/service.rs:861` `download_url` and `:1298` `persist_completed_upload`: changed signature/body, no rustdoc or Errors. The sole callers are `download_init` and shared `finish_upload`; caller-owned transaction semantics remain material.
- `crates/wyrd/wyrd-server/src/components/cards/service.rs:1974` `reconcile_cleanup_claim`: newly fallible deletion recording without Errors; `:2227` `finish_card_delete`: no rustdoc; `:2592` `cleanup_card_blob` and `:2775` `write_card_blob`: fallible changed boundaries with no Errors. The latter now writes bytes before awaiting inventory ACK.
- `crates/wyrd/wyrd-server/src/components/storage/routes.rs:218,431` `complete` and `download_init`: no Errors; completion's prose omits the new inventory ACK boundary.
- `crates/wyrd/wyrd-server/tests/integration/pg_card_declaration_route.rs:4978` `ArtifactPrompt`: six new undocumented fields; `send_json:5074`, `complete_request:5088`, `list_artifacts_request:5104`, `download_init_request:5113`: new helpers with `expect` and no Panics.
- `crates/wyrd/wyrd-server/src/scribe_outbox.rs:2488` `ObjectGate::ingest_frame`: new test implementation can panic on a closed release semaphore and has no Panics.

**Evidence/materiality:** These are actual changed items, independently checked against the diff and full bodies, not an unrelated old-documentation sweep. A successful compiler/clippy run cannot waive the repository's explicit merge gate. Missing durable failure/partial-progress docs also obscure what is committed before ACK and which owner retries.

**Minimal correction and burden:** Complete meaningful docs on this bounded set, including real Errors/Panics and the applicable cancellation/partial-progress/idempotency/retry semantics. Batch the docs for behavioral remediation's changed items in the same pass. No runtime refactor, new documentation tool, annotation suppression, test or unrelated file sweep. Burden is bounded text.

**Closure proof:** Inspect the listed items against the mandatory rule and the final changed behavior; run the normal format/lint checks for the resulting write set. Avoid placeholder Errors text or expanding docs on untouched helpers.

### FIND-TASK-001-4 — migration proof must fail without its database setup

**Status:** CONFIRMED. **Discovery:** REPO-002. **Classification:** VIOLATION.

**Obligation/location:** SQL-directory AGENTS Tests forbids SQL proof silently skipping for absent environment. New `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:1494` `fresh_install_has_no_postgres_object_inventory` does exactly that.

**Source path and consequence:** `database_url:2346` maps `ResolvedDsns::from_env().ok()` to Option. Missing or invalid DSNs yield None; the new test's `let Some(url) ... else { return; }` returns success before migrations or absent/retained-table assertions. Properly provisioned recorded runs do execute the assertions, but do not make this forbidden green-without-proof branch unreachable. Existing neighboring optional tests do not override the directory rule for this new replacement proof.

**Minimal correction and burden:** Require database setup explicitly in this new test, preserving the existing DSN helper, repository Postgres runner, migration owner and assertions. Missing/invalid setup must fail. No global helper conversion or neighboring test rewrite is necessary; a local expectation is sufficient and its panic docs must reflect it.

**Focused closure proof:** Run its exact selector through the existing repository-managed Postgres task/wrapper. Also execute the same test with all required DSNs absent and observe failure, not green. Reuse the built test/runner and existing process environment mechanism; no new harness or permanent check is required.

### FIND-TASK-001-5 — add scrubbed tracing to changed storage handlers

**Status:** CONFIRMED. **Discovery:** REPO-003. **Classification:** VIOLATION.

**Obligation/location:** Root server-handler rule and Rust diagnostics rule. `crates/wyrd/wyrd-server/src/components/storage/routes.rs:218` `complete` and `:431` `download_init` have utoipa attributes but no `tracing::instrument`.

**Source path and consequence:** The upload route performs idempotency extraction, authorization, shared completion and fact construction before `ObjectInventory::record`. Download authorizes and reads Card/manifest/inventory before the storage plan. The inventory spans cover only leaf operations and cannot cover handler errors outside those calls. The mandatory handler-level diagnostic boundary is absent in both materially changed operations. This is an explicit repository gate, not an optional request for more metrics.

**Minimal correction and burden:** Add conventional scrubbed handler instrumentation, excluding state, Caller/credentials, headers and request bodies. Reuse existing safe operation/tenant fields and tracing conventions (for example the adjacent Card lifecycle handlers and instrumented storage service). No framework, extra telemetry fields or unrelated route sweep.

**Closure proof:** Inspect both handler attributes and their argument scrubbing; use existing focused route regression checks and normal format/lints. No new behavioral test is needed solely for attributes.

## Reconciled acceptance and no-finding coverage

The selected reports cover every applicable TASK-001 row, including no-finding surfaces. Their source evidence remains useful where the validated failures do not invalidate it. The matrix below supersedes overbroad PASS wording in discovery/task summaries; it does not extend TASK-001 into later upload/evidence tasks.

| Obligation / affected surface | Reconciled source and proof | Result |
|---|---|---|
| REQ-002 shared normal/reconstructed completion; activation validation | `upload_complete` and `reconcile_upload` converge on `finish_upload`; Card manifest binding remains before recording; envelope write/ACK precedes activation. Existing storage/Card regressions and V1/V2. | PASS |
| REQ-003 / INV-004 validation preserved, no new digest certification | Existing backend commit/HEAD/size/encryption and Card manifest/path checks retained; digest conversion only normalizes representation. No new hash pass. | PASS |
| REQ-004 schema/catalog, attribution, daily layout and Blooms | ObjectsTable extends existing built-ins and `StagedRow::Cell::Int`; attributed daily layout includes subject Card plus storage/digest lookup. Schema/catalog/attribution proof recorded. | PASS |
| Replay uniqueness and recorded deletion dominance | `fold` maps validated relative paths and clears all objects when any Card deletion exists. V2 independently rerun; omission of a deletion is separately FIND-2. | PASS for recorded facts |
| REQ-005/013 hydration and envelope authority | SQL pointer bypass survives when envelope row is absent. Healthy V1 does not falsify it. | FAIL — FIND-1 |
| REQ-005 file listing/download/fitter/hosted lookup | Shared `card_files`/`card_file`; manifest completeness creates readiness, never metadata fallback. Fitter preserves Card/size bounds. V1/V3 rerun, fitter/hosted/CLI regressions recorded. | PASS apart from FIND-2 producer state |
| REQ-006 existing delivery and peer route | Closed Object variant/destination extends existing projection/submit/tally and mTLS capture. No second delivery worker/outbox. | PASS |
| REQ-007 genuine bounded upload-only ACK | `record` stages row-specific oneshots with half timeout; `tally` only sends for successful owning Object streams; refusal/loss cannot send success. V4 rerun and healthy public completion journey. | PASS with proof limit below |
| REQ-008 / INV-003 sibling producer behavior | Object-only send is non-awaiting; existing detail/summary submission, group scheduling and queued reserved handoff remain. Telemetry change affects emitted metrics, not queue/result transitions. | PASS |
| REQ-013 missing file/query failures | Bounded scoped reader propagates stream failure; absent required file returns existing visibility code. V3 rerun checks exact 502/503 and successful retry. | PASS; envelope separately FAIL |
| REQ-015 / INV-002 tenant/authorization/public-write boundary | Table-scoped tenant SystemReadAuthority reuses existing owner; Card authorization stays upstream; file fold validates tenant/Card/path; Gate reserves Objects before admission; peer keeps system-tenant refusal and existing authenticated listener. Security reviewer additionally traces narrow Card-read hosted consumer proof. | PASS |
| REQ-017 removal/fresh installation; retained coordination | Old query module/exports removed; new immutable drop migration; sessions/manifests retained. No remaining serving SQL inventory caller. V1 and provisioned migration runs pass, but new test can silently skip. | Implementation PASS; mandatory proof FAIL — FIND-4 |
| REQ-018 all existing physical cleanup reflected | Explicit delete/retry record correctly; successful initial compensation does not. | FAIL — FIND-2 |
| Existing-owner reuse / maintainer scope | Table/catalog, query/authority, shared completion, Scribe and Card cleanup remain their current owners. Extra pre-completion HEAD is visible but neither a proven material regression nor an independent required refactor. | PASS |
| Changed-item rustdoc / handler diagnostics | Actual listed omissions validated, independent of otherwise passing automated lanes. | FAIL — FIND-3, FIND-5 |
| Bounded verification fixes and gate integrity | Recorded independent diagnoses align with changed runner/telemetry and fixture outbox settlement; no weakened assertion/skip was introduced to fix those two failures. | PASS; SQL new-test skip separately FAIL |
| Non-goals / cross-tier scope | No new dependency/feature, OS-specific path, public API, SQL serving copy, queue, backfill, repair scan, hashing/promotion, retention or permission semantics. SDK projections/integrated language/performance closure remain allocated to subsequent tasks/change review. | PASS |

## Verification limits and final recommendation

Review-time `verification.md` records exact V1–V3 runs against fresh repository-managed Postgres and V4 passing, plus diff whitespace. Broader format/lint, catalog/schema, SQL, hosted/fitter/CLI, producer, dependency, tenancy and codegen checks remain recorded task evidence; this reviewer did not rerun them or create a fault reproduction. Passing journeys do not cover missing envelope visibility or successful initial compensation tombstones.

V4 experimentally controls the outbox receiver; the public API proof is healthy-path completion followed by inventory read, and queued handoff isolation is structural. Source directly traces the endpoint's awaited recorder and the unchanged handoff; retain those limits without inventing an additional blocker or a new public test framework. No cloud backend, load, full language suite or deployment/backfill test was run here; deployment migration/backfill is explicitly excluded.

Recommend **FIX_REQUIRED**, one bounded remediation task containing FIND-TASK-001-1 through FIND-TASK-001-5. Use focused missing-envelope and compensation/retry proofs, the exact SQL selector in provisioned and missing-DSN contexts, the existing affected Card/storage/cleanup tests, and format/lints for the final write set. Reuse current ACK and server/Postgres fixtures; no whole journey suite or broad aggregate is required for remediation. All retained findings fit approved behavior. No rejected claim, optional simplification, unrelated cleanup or new architectural decision should enter remediation.
