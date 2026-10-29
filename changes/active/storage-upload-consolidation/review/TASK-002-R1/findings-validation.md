# TASK-002 independent findings validation

Recommendation: **FIX_REQUIRED**, with three deduplicated findings. No specification revision is required for the bounded corrections below.

## Subject and evidence

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Base: `15f939fc917b22efa5e5e4d12fd1f97be807a0a7`.
Candidate: `5ef2981e66068e8ee5f96d3be15000eea9cbd94f`.
This is the first TASK-002 review; no prior TASK-002 findings or acceptance are carried forward. HEAD was checked at the candidate; implementation source remained immutable. There is no CodeGraph index.

Read the review skill, common scope, all five discovery reports, the standards addendum, `followup-review.md`, and `verification.md`. Compared the recorded Git range and independently read the proposed failing functions and their affected producers, callers, sibling consumers, SQL writers/indexes, and tests. Authority includes root/nested AGENTS, spec-driven development, Rust and maintainer guides, approved specification revision 2, original TASK-002 obligations before Implementation Evidence, original TASK-003 ownership, and the applicable Verifier, Bifrost inventory, and tenant-security sections. Implementation evidence is verification context, not authority.

The follow-up was warranted because BEH-001's initial coexistence suggestion did not resolve the active-path unique index or provider object-key coupling. Its final recommendation was independently checked against those sources. It discovered no additional blocker. No other follow-up was needed: remaining proposals have complete source-backed correction boundaries, and the REQ-014 scope dispute is resolved by the original task packet.

## Proposal decisions

| Discovery proposal | Decision | Final treatment |
|---|---|---|
| BEH-001, with focused follow-up | **REVISED** | Retain as FIND-TASK-002-1. Confirm the regression; replace the initial incomplete coexistence alternatives with one final location per existing upload identity, using the existing shared lifecycle and inventory. |
| BEH-002 | **CONFIRMED** | Retain as FIND-TASK-002-2. |
| DATA-001 | **CONFIRMED**, duplicate | Same newly admitted File rows and missed sweeper owner/path handling as BEH-002; consolidate into FIND-TASK-002-2, with its orphan-initialization and bounded-queue consequences. |
| RSTD-001 and focused addendum | **CONFIRMED** | Retain as FIND-TASK-002-3, one mandatory documentation correction including tests and durable partial effects. |
| MAINT-001 | **REJECTED** as a TASK-002 blocker | Source observation is true, but explicit TASK-003 work. No remediation item or optional advice is created from this rejected proposal. |
| Security report, empty proposal set | Validated | No material security proposal omitted. Existing tenant/operation authority remains the correction constraint. |

### Resolution of MAINT-001 and TASK-003 scope

The actual execution path carries SYSTEM identity: `VerificationControl::run_direct` takes `target.system_principal` and constructs `executed_by_system` at `components/verification/service.rs:526–534`; `HostedBlocks::engine` passes that caller into `TenantEvidence` at `verification/hosted/blocks.rs:137`. The new File reader calls `ObjectInventory::file_object`, which calls `rows`; `components/storage/inventory.rs:312–321` acquires a registry tenant connection, calls `system_principal_id`, then commits. `wyrd-sql/src/queries/auth/service_accounts.rs:305` is indeed an application SELECT against `wyrd.auth_service_accounts`. `SystemReadAuthority::resolve` is the existing table-scoped query authority and does not erase that added SELECT.

However, original TASK-002 explicitly delivers inventory-backed File locations immediately and assigns broader metadata reuse/readback to TASK-003; its requirement mapping omits REQ-014. Original TASK-003 includes REQ-014 in frontmatter, explicitly requires “A File inventory resolution adds no application-level Postgres work,” and assigns its Scenario 1/V1 to proving that property. Its implementation reference expressly identifies TASK-002's location replacement and the remaining reuse gap. This is a staged obligation, not a contradiction that invalidates the approved spec. Treating MAINT-001 as immediate TASK-002 remediation would pull the named downstream task forward. Reject it here; this rejection grants no exemption from REQ-014 at TASK-003 or integrated change review. No new identity cache, planning-identity decision, or expanded SQL instrumentation is authorized by this validation.

## Final deduplicated ledger

### FIND-TASK-002-1 — Independent File planning revokes a returned live upload

- Discovery: **BEH-001**, resolved correction evidence in `followup-review.md`.
- Status: **REVISED**. Classification: **REGRESSION**.
- Obligation: original TASK-002 preserves the File SDK operation and existing authoring behavior while consolidating it under REQ-001/REQ-016. Independent verifications of the same document are supported use; no approved authority introduces tenant-wide single-active-call admission per digest.
- Exact locations: `wyrd-storage/src/service.rs:202–226` creates the same tenant/digest path with no Card; `:269` calls the shared prior-replacement stage; `:812–834` finds and unconditionally aborts the prior pending row. `wyrd-sql/src/queries/storage/multipart_uploads.rs:392–436` matches NULL Card, relative path and digest. `20260601000002_storage.sql:48–50` independently enforces one initiating/pending row per storage path.

**Producer to consequence.** Both calls enter `VerificationControl::evidence` before a completed inventory match exists. A receives upload A. B then initializes the same File, finds A pending, marks it aborted, and best-effort aborts its backend handle. A's local transfer subsequently fails `load_pending_upload`; A's completion rejects `Aborted` with `WYRD_STORAGE_409_UPLOAD_NOT_PENDING`. The shared client `RunEvidence::pin` calls `upload_artifact`, which propagates the transfer/completion error, so the verification never runs. A-plan/B-plan/A-transfer is sufficient; no timing race within a function is needed. The base direct digest PUT did not revoke A. Parallel verification of common reference files is ordinary use, with a concrete failure and wasted multipart transfer, so this is material.

**Smallest safe correction.** Keep independent File sessions independent through their final object location. Use each session's existing `UploadId` to distinguish its location beneath the existing tenant/digest evidence namespace, persist it once with that same SQL session, and use that location unchanged through transfer, backend completion, inventory recording, and completion replay. Restrict destructive pending replacement to Card reinitialization. Extend the existing evidence path construction/strict validation to that canonical session-qualified shape and update its actual consumers/fixtures; keep verified tenant checks, canonical digest/session syntax, and Card/traversal/foreign-path refusal. Reuse the current planner, dispatcher, SQL coordinator, owner authorization, backend lifecycle and `ObjectInventory`; keep the active-path unique index unchanged.

**Why smaller alternatives fail.** Merely skipping prior abort still conflicts with the active-path index. Removing that index permits multiple writers at one mutable key and does not isolate Azure's fixed ordinal block IDs (`cloud.rs:298–315,373–392,682`). Replaying one live session couples explicit abort and concurrent backend completion: S3/GCS use the same backend handle, and Azure uses the same block set and terminal SQL row. Fixing those alternatives requires shared-session ownership/commit semantics absent from the task. Returning a conflict or adding downstream retries leaves an independent previously supported operation failing. No new locks, reference count, wait policy, cache, object-ID system, provider logic, promotion, or immutable-write protocol is necessary when existing sessions have separate locations.

**Authority and burden.** REQ-004 permits existing upload identity to associate retries and requires no global digest-deduplication service. REQ-010 selects digest/size through inventory and reads its recorded location; it does not require one physical key per digest. Architecture's File namespace remains the same. The exact no-suffix grammar is implementation text being replaced, not an approved invariant. This is the initialized session's final location from the start, preserving REQ-003's prohibition on promotion/new hashing, and it does not share Card/File objects or change deletion/retention ownership. Fresh-install scope creates no old-path migration or compatibility obligation. The correction is bounded path/session plumbing rather than a new concurrency or persistent-data model.

**Affected consumers and preservation.** Request continuation uses `service::row_path`; provider signing, multipart, commit and abort consume `ValidatedPath.location`; local writes use the persisted path. Inventory `file_fold` already chooses an undeleted matching path from multiple completions and validates it; `TenantEvidence::stored` reads that returned location and still checks size/digest. Preserve completion replay of one upload as the same logical object and all Card replacement/reconciliation behavior. Carry the same strict File shape into the sweeper correction in FIND-TASK-002-2. Update existing exact-path assertions and synthetic tombstones in `pg_verification_routes`, the tamper target in `pg_hosted_files` (currently constructs `tenant_path::evidence`), and the evidence path unit tests to use/validate actual recorded session locations. No second inventory or digest-derived reader fallback is needed.

**Focused closure proof.** In the existing real-Postgres server fixture, obtain A and B plans for the same tenant/digest before completion, then complete both and verify the File through inventory. Replay A completion and assert its location/logical object identity remains stable. In a separate pair, abort A and complete B. Assert Card reinitialization still retires its prior pending session. Extend only the relevant existing S3 multipart/Azure block backend fixture cases to exercise distinct paths while the two plans are live; Azure's block IDs must stay isolated by key. Retain strict foreign/malformed-path refusal and the existing hosted tamper proof against the actual completed path. These deterministic interleavings need no load test, sleep, queue, new harness, or all-surface suite.

### FIND-TASK-002-2 — File sessions cannot be reclaimed by the existing sweeper

- Discovery: **BEH-002 + DATA-001**, one cause and correction boundary.
- Status: **CONFIRMED**. Classification: **REGRESSION / INCORRECT**.
- Obligation: the newly supported File must use the existing full upload lifecycle while preserving existing recovery and Artifact behavior (TASK-002 outcome, REQ-001/REQ-016). This is coordination/provider upload reclamation, not new completed-payload GC, retention policy, or durable inventory repair.
- Exact producer: `wyrd-storage/src/service.rs:202–226,269–310` admits initiating/pending NULL-Card File rows; nullable owner migration `20261029000000_file_upload_sessions.sql:4`. Failing consumer: `wyrd-storage/src/sweeper.rs:297–298`. Selection: `wyrd-sql/src/queries/storage/admin/multipart_uploads.rs:43–78`.

**Producer to consequence.** Abandoned File requests, or process cancellation after the initiating-row commit but before backend/pending persistence, leave ordinary open upload rows. The existing expiration/grace query selects them in earliest-expiry order with a bounded LIMIT. `Sweeper::abort_one` applies Card-only `tenant_path::validate`, rejecting every File path before provider abort and before `mark_aborted_if_open`. `sweep_expired_uploads_batch` logs the error without retiring the row. Repeated ticks/restarts repeat it. Remote multipart resources remain unreclaimed; orphan initiating rows retain the active-path claim; enough oldest File rows fill every batch and starve later expired Card uploads across tenants. Existing `pg_sweeper` fixtures only seed Card paths. Normal upload abandonment and deterministic shared-queue starvation establish materiality.

**Smallest safe correction.** Extend existing `ExpiredUpload` and its existing discovery SELECT to carry persisted nullable Card ownership. The existing sweeper must select strict Card versus File path validation from that owner, matching request-side ownership selection in `row_path`; use the session-qualified File shape from FIND-TASK-002-1. Keep the same verified tenant validation, provider abort behavior, per-row error handling, tenant transaction, `mark_aborted_if_open` race protection, and commit/diagnostic owner. The additional owner projection is necessary because accepting either path shape without checking persisted ownership weakens that boundary. No new worker or broad path resolver is justified; skipping File rows, raising the batch limit, or only changing request-side abort cannot close the leak/starvation. `ExpiredUpload` is consumed by the existing sweeper and reclamation diagnostics; its discovered ownership is not a new caller-controlled permission.

**Focused closure proof.** Extend the existing real-Postgres `pg_sweeper` coverage with expired File pending and orphan initiating rows and prove each becomes aborted. With batch size one, place an older File before an expired Card and run bounded successive ticks; both must retire. Replan the reclaimed initiating File successfully. Preserve already-completed-row protection and strict malformed/foreign/mismatched-owner refusal. Use the existing relevant provider fixture/seam to prove accepted File multipart paths reach the same backend-abort operation; do not change provider cleanup semantics or require a cloud credential solely to test SQL/path dispatch.

### FIND-TASK-002-3 — Changed Rust items fail mandatory item documentation

- Discovery: **RSTD-001**, including its independently validated evidence-method addendum.
- Status: **CONFIRMED**. Classification: **VIOLATION**.
- Obligation: root AGENTS makes changed-item rustdoc a merge gate; `agent_docs/rust.md#rustdoc` includes private fields/helpers/tests, fallible errors, possible panics, and async/durable cancellation, partial progress, idempotency and retry. Green compiler/runtime checks do not waive it.

Independently checked each proposed item against the candidate and diff. The bounded inventory is:

| Candidate source | Validated missing contract |
|---|---|
| `wyrd-storage/src/service.rs:74–75` | Changed `PriorAbort` and `validated` field have no rustdoc; the field defers a validation error to best-effort abort. |
| Same file `:85,93` | Changed `PersistUploadInit` and optional `body_sha` lack docs for persistence role and paired key/digest invariant. |
| Same file `:164` | Changed `upload_init` retains only a name-restating summary; no Errors or committed initialization/cancellation/retry contract. |
| Same file `:202,237` | New `file_upload_init` and `init_upload` omit cancellation/partial-progress/retry across initiating commit, prior-session abort, backend initialization and pending persistence; File errors refer to the undocumented Card operation. |
| Same file `:329` | Changed `persist_upload_init` lacks purpose/invariants, Errors and persistence/commit partial-effect contract. |
| Same file `:361` | Changed `claim_upload_init` lacks purpose/invariants and Errors; its returned transaction owns the initialization lock/replay boundary. |
| Same file `:773` | Changed `try_replay_idempotent_init` lacks purpose and Errors for cached request/row parsing and owner-validated plan reconstruction. |
| Same file `:812` | Changed `find_and_mark_prior_pending` lacks purpose, Errors and transactional mutation/returned deferred abort contract. |
| Same file `:840` | Changed `abort_prior_best_effort` lacks docs for logging/suppression of validation/provider failures after the prior SQL transition. |
| Same file `:1013` | Changed `insert_initiating_upload` lacks purpose/invariants and Errors for checked conversions/SQL insertion; caller still owns commit. |
| Same file `:393,592,638` | Changed `upload_part_url`, `upload_abort`, `upload_local_blob` have only summaries, no required Errors; abort/backend-versus-SQL partial effects and validated local write/retry effects are undocumented. |
| `wyrd-client/src/verify/mod.rs:483` | Changed `RunEvidence::pin` omits committed earlier File completion/unfinished later session effects on failure/cancellation and retry behavior. |
| `wyrd-server/src/components/verification/service.rs:343–345` | Changed `VerificationControl::evidence` can commit earlier File sessions before a later loop lookup/init fails or the request is canceled, but omits partial-progress/retry documentation. |
| `wyrd-server/src/components/storage/routes.rs:154,326` | Changed `part_url` and `abort` omit Errors; utoipa responses do not supply item rustdoc. |
| `wyrd-server/src/http/middleware/body_limit.rs:96` | Changed `Service::call` lacks item rustdoc for delegating these routes to handler-owned bounds; module prose alone is insufficient. |
| `wyrd-sql/src/queries/storage/multipart_uploads.rs:134,137` | Changed `MultipartUploadRowDb` and nullable `card_uid` lack decoding/owner contract docs. |
| `wyrd-sql/tests/integration/pg_migration.rs:1372` | Changed `storage_queries_round_trip_under_tenant_conn` has no purpose rustdoc or Panics despite IO/setup expects and assertions. |
| `wyrd-storage/src/tenant_path.rs:238` | New `validate_evidence_accepts_only_own_tenant_digest_paths` lacks Panics for its assertions. |

The evidence-method occurrence is corroborated by the standards addendum and independently traced through HTTP `components/verification/routes.rs:145`, MCP `mcp/verification.rs:208`, and the full initialization body. It adds no new behavioral demand. The remaining documented new tests/helpers and richer completion/reconciliation/inventory methods were inspected as nearby patterns rather than grounds to document untouched code.

**Smallest safe correction and burden.** Add accurate, concise item rustdoc in place for this inventory; any items changed by the two behavioral fixes must document their final semantics. Explain actual errors/panics and meaningful partial effects, not boilerplate or new recovery guarantees. Preserve code behavior for documentation-only edits. No new script, lint suppression, helper extraction, broad doc rewrite, or runtime test is needed. This is one mandatory gate finding and belongs in the same remediation task as the lifecycle fixes, not a separate style round.

**Closure proof.** Inspect every listed item against the final diff and require appropriate purpose/invariant, Errors/Panics and durable partial-effect/retry contracts. Run the existing owning format/lint checks; do not invent a documentation checker.

## Coverage reconciliation and prospective proof scope

The behavior acceptance matrix, standards authority matrix, maintainer changed-owner coverage, and separate security/data traces cover the task's shared planning/transfer/completion, completed File-only reuse, inventory-backed byte reads, bounded validation, operation authority/tenant isolation/audit, thin language/HTTP/MCP/CLI projections, removed route/contracts and fresh-install ownership. Independently inspected the inventory reader/fold and hosted length/digest reader while validating their affected paths. No missing selected report, unresolved caller trace, changed candidate, unrelated new architecture, or unvalidated new blocker remains. REQ-014, complete once-per-execution reuse, broader queued readiness/readback and performance remain the named TASK-003 obligations. Existing pass rows stand within that task split; the two lifecycle rows and mandatory documentation row fail.

Package one cohesive remediation task for all three IDs. Keep proof bounded to the write set:

- Add a deterministic real-Postgres overlap/independent-abort case under existing `pg_verification_routes`; its exact new selector is planned and must be confirmed by the implementer. Rerun affected existing File lifecycle/reuse/permission selectors, and the hosted tamper selector if adapting its target.
- Use the existing `wyrd-storage --test integration` `pg_sweeper::pg_tests` owner for File expiry/orphan/starvation and completed-row protection. The current owner selectors include `pg_sweeper::pg_tests::tick_sweeps_expired_uploads_and_reaps_idempotency_rows` and `pg_sweeper::pg_tests::sweeper_skips_reclamation_when_upload_already_completed`. Add/confirm exact new selectors for the added gaps.
- For provider isolation, reuse only the affected `storage_e2e::s3_multipart_e2e` and `storage_e2e::azure_multipart_e2e` fixture cases or new exact cases in that owner. Setup comes from `storage:up` and the corresponding environment in `test:storage:e2e:inner`; use `wyrd-server --features storage-emulator --test integration --test-threads=1` with exact filters. Do not require the full `test:storage:e2e` aggregate. Existing GCS multipart has no credentialed proof; do not claim otherwise or create new credentials/harness work.
- Reuse repository-managed Postgres and migration setup from `verification.md` and `test:sql:inner`; run nextest through `mise exec -- cargo nextest run --locked ...` with exact nonzero selectors and tracing. SQL/path unit checks, existing format/lints and tenant/storage-boundary checks are appropriate if their write sets change. No full surface journey suite, release gate, broad backend matrix, benchmark, or codegen run is needed when no wire shape changes.

`verification.md` independently records fresh repository-Postgres V1/V2/V3 passing, including migration/idempotency setup. Standards independently records TypeScript format/lints passing. These prove the exercised serial paths, not either lifecycle gap. I did not rerun runtime, broad language/backend or credentialed cloud checks; validation here is independent source and correction-boundary analysis. No implementation was changed.
