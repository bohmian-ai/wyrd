# TASK-001 R2 — independent findings validation

Recommendation: **PASS**. FIND-TASK-001-1 through FIND-TASK-001-5 are closed. The independently validated remaining finding ledger is **empty**. No specification revision or additional remediation is required.

## Immutable subject and scope

Repository `/Users/stevenforrester/Documents/GitHub/wyrd`; original base `3899b0a35`; prior candidate `826fcf9c742a36bcae5e8f68307e76685d88244e`; candidate `5e7dec69732ba6d4d05399aeffd6f4d9c683745d`. HEAD independently matches the candidate. Reviewed the actual incremental diff `826fcf9c7..5e7dec697`, with the cumulative original-base range available. The requested `e6a34a010..5e7dec697` contains the same implementation: the intermediate commit adds only R1 review artifacts. No CodeGraph directory exists. Only this report was written by this validator; source stayed immutable.

Read the complete task-review skill, R2 scope, selected `domain-review-concurrency.md`, and fresh `verification.md`; retained the approved revision 2 specification, original TASK-001, full R1 verdict and findings validation, R1 scope/verification, and remediation diagnosis/evidence. Applicable authority is root AGENTS, nested SQL AGENTS, spec-driven-development, maintainer style, changed-item Rust documentation rules, and owning Card-envelope, Bifrost inventory/deletion/ACK and tenant security boundaries. Authority did not change in this round.

This is independent closure validation, not repeat whole-task discovery. I inspected every incremental source/test change and full correction bodies, traced the three hydrated getters, both compensation callers, all four physical-cleanup callers and the shared recorder, and followed the affected SQL/worker transitions. Accepted unchanged schema/catalog/layout/Blooms, shared completion and validation, scoped query/security/public-write boundary, delivery/ACK isolation, removed SQL consumers, fitter/hosted/CLI and bounded verification-runtime corrections carry forward from the R1 source-backed matrix. The changed persistence boundary specifically reopens cleanup concurrency; no other unchanged area needed reopening. The selected domain report is complete and proposes no finding. Its agreement is not treated as proof.

## Stable prior-finding closure

### FIND-TASK-001-1 — CLOSED

Prior classification **INCORRECT**, discovery BEH-001/DATA-1; obligations REQ-004/005/013 and INV-001.

`components/cards/service.rs::hydrate_card:366` now consumes the `CardObjects.envelope` already returned through `card_files`. After the existing missing-URI, URI-scheme, tenant-path and exact Card binding checks, `:389–398` requires the recorded path to equal the retained pointer and returns `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE` otherwise. Storage IO starts only afterward. URI-backed bytes cannot substitute for a missing envelope; equality also makes the validated URI location the recorded location. Blob parsing, exact identity and integrity checks remain afterward. All three getters (`get_card_by_uid:107`, `get_card_by_ref:132`, `get_latest_card:163`) share this boundary. Listing, selected-file download, fitter and hosted lookup still use the existing file owner without acquiring an envelope dependency.

`pg_card_declaration_route::hydration_requires_recorded_envelope:5427` creates a metadata-only Active Card, copies identical envelope bytes to a valid same-Card path without a matching inventory fact, changes the retained pointer, observes the exact 503 code, then restores the recorded pointer and gets 200. This directly proves pointer/bytes alone are insufficient. It tests a missing fact at the required location rather than deleting the entire envelope projection; the same source predicate explicitly handles `None`. Fresh review-time execution passed, along with the file-bearing round trip and scoped-read regression. No second query, resolver, fallback or new error family was introduced. The prior correction is satisfied at its existing consumer owner.

### FIND-TASK-001-2 — CLOSED

Prior classification **MISSING**, revised discovery BEH-002/DATA-2/SYS-001; obligation REQ-018 and original deletion-writer coverage.

Both initial abandonment (`reconcile_card_claim:1953`) and declaration compensation (`abort_card:2337`, reached by `abort_pending_declaration_cards`) pass their loaded Card to the same `commit_card_failure:2745`. The shared function commits `fail_card` first (`:2753–2754`). SQL `queries/cards/lifecycle.rs::fail_card:555` changes Pending to Failed with cleanup/pending, zero attempts, due retry and no lease. Only after this durable handoff, and only for successful physical cleanup, does `:2758` await the existing `record_card_deletion:2640`; only after its ACK does a separate tenant transaction call the existing success update. Physical failure leaves pending cleanup. Refusal, timeout, cancellation or transaction failure after the Failed commit cannot roll it back. A lost/unknown ACK leaves the existing cleanup claim as the recovery owner. No second tombstone owner, queue, durable state or reader-side Failed filter was added.

I checked sibling `finish_card_delete:2276` and `reconcile_cleanup_claim:2012`: their physical cleanup → recorder → success behavior is preserved; the claimed path still locks its live lease and uses `Some(owner)`. Worker `cards/reconciler.rs::process_claim` still selects cleanup by its durable kind and records errors through the fenced SQL failure owner. The declaration claim's old lease is intentionally cleared by `fail_card`, so its later error recording cannot overwrite the newly scheduled cleanup. The next claim uses that durable cleanup state. The initial transition returning false does not clear another owner's state.

The new `compensation_records_inventory_deletion:5543` exercises completed A plus expired B, refuses inventory writes through the existing WAL seam, observes A removed and Failed/cleanup/non-idle, then observes a refused claimed retry with Failed/cleanup/pending retained. After admission it observes idle, records stale A completion, and confirms the inventory is deleted with no files. Fresh review-time execution and existing storage-recovery/explicit-delete-failure/replay tests passed. Abort compensation shares the exact corrected function and needs no duplicate fault harness. Cancellation persistence follows the committed SQL-before-await ordering; it is not claimed as an independently scheduled cancellation experiment.

### FIND-TASK-001-3 — CLOSED

Prior classification **VIOLATION**, discovery REPO-001; mandatory root/`agent_docs/rust.md` changed-item rustdoc gate.

Independently inspected each enumerated omission and newly changed item against its final source:

| Owner | Closed documentation obligation |
|---|---|
| `wyrd-storage/src/service.rs` | `download_url:866` explains local versus signed URL and presign Errors; `persist_completed_upload:1311` explains caller-owned transaction, partial progress/retry and SQL Errors. |
| `cards/service.rs` | `reconcile_cleanup_claim:2012` documents repeated cleanup, deletion ACK, fenced success and Errors; `finish_card_delete:2276` documents deletion ownership and pending recovery; `cleanup_card_blob:2662` documents absent-object idempotency and Errors; `write_card_blob:2875` documents bytes-before-ACK, unknown outcome, safe rewrite and Errors. |
| Same changed behavior | `hydrate_card`, `reconcile_card_claim`, `abort_card` and `commit_card_failure` document their new visibility, failure and committed retry boundaries with Errors; no changed function is left without rustdoc. |
| `components/storage/routes.rs` | `complete` describes committed completion before inventory ACK and safe retry, with Errors; `download_init` describes authorized inventory resolution and Errors. |
| Card integration tests | All six `ArtifactPrompt` fields are documented; `send_json`, `complete_request`, `list_artifacts_request`, `download_init_request` have actual Panics descriptions. New pointer/state/poll helpers and both tests have intent and Panics documentation. |
| `scribe_outbox.rs` / SQL test | `ObjectGate::ingest_frame` documents closed-semaphore panic; the migration proof documents absent/invalid DSN panic. |

The docs describe the bounded final behavior rather than introducing a new contract. No suppression or unrelated documentation sweep entered the diff. Recorded remediation format/lints passed; source inspection, rather than green lints alone, establishes closure of the mandatory gate.

### FIND-TASK-001-4 — CLOSED

Prior classification **VIOLATION**, discovery REPO-002; nested SQL rule forbidding silent setup skips.

`pg_migration.rs::fresh_install_has_no_postgres_object_inventory:1494` now calls `database_url().expect("the fresh-install proof requires database DSNs")`. Missing/invalid setup cannot return success before assertions. The helper, migration owner and all three absent/retained table assertions remain unchanged. The new panic is documented. Fresh review-time provisioned execution passed through repository-managed Postgres; the exact selector with both DSNs removed failed at the expected setup expectation with nextest exit 100. This is the intended negative proof, not a waived failing gate. The correction remains local to the new test.

### FIND-TASK-001-5 — CLOSED

Prior classification **VIOLATION**, discovery REPO-003; root handler instrumentation rule.

`components/storage/routes.rs::complete:233` has handler-level `tracing::instrument` skipping state, caller, headers and body; its constant operation is `storage.upload.complete`. Only the route upload-id argument remains automatically recorded. `download_init:455` skips every argument and records constant `storage.download.init`. Credentials, request bodies and state are scrubbed. Both handler runtime bodies are unchanged by this correction, and fresh affected route regressions passed. No additional telemetry framework or unnecessary tracing-only test was added.

## Independent concurrency reconciliation

The domain report's success-overlap observation is reachable: A commits Failed/pending after successful physical cleanup and waits for deletion ACK; B claims cleanup; A receives ACK and the unleased success branch (`lifecycle.rs::mark_card_reconciliation_succeeded:250`, `None` branch) can clear B's lease. B's later success/failure is fenced and becomes a no-op. I verified these SQL predicates, worker dispatch, physical cleanup and recorder bodies directly.

This does not establish a material defect in the approved boundary. Before A can clear cleanup it has successfully removed all work its cleanup owner enumerates and received a real Card-wide deletion ACK. B repeats that same cleanup; absent-object deletion is accepted, and `upload_abort` accepts already terminal uploads. B's failure after that completed result does not create missing work. Failed cannot activate (`commit_card_activation` locks Pending), and explicit deletion requires Active, so the overlap cannot erase a new lifecycle category on this Failed Card. The inventory fold independently makes any deletion dominate completions regardless of order and clears files/envelope. Exactly one physical attempt or exactly one tombstone is not an approved obligation. Errors/cancellation before A's ACK leave durable cleanup; errors after ACK but before success produce safe extra retry. The existing explicit-delete owner already uses the same unleased-success convention.

I therefore accept the domain conclusion from source, without retaining the implementation evidence's overlap note as a new finding. A new claim protocol or consumer guard would impose correction burden without a source-proven missing required effect. There is no material conflict or unresolved failing path requiring a follow-up reviewer; no new validator-originated blocker was discovered.

## Reconciled acceptance matrix

| Obligation / boundary | Reconciled implementation and verification evidence | Result |
|---|---|---|
| REQ-002/003, INV-004 shared completion and preserved checks | Carry forward R1 shared `finish_upload`, backend/size/encryption/Card binding and no new digest certification evidence; runtime owner unchanged. Fresh Card round trip/recovery passed. | PASS |
| REQ-004 catalog/schema/daily layout/Blooms and replay | Carry forward R1 built-in/catalog/attribution evidence; schema and recorder/fold runtime unchanged. Fresh replay and compensation proof passed. | PASS |
| REQ-005/013, INV-001 envelope authority | Already-resolved envelope now required at validated pointer before IO; focused missing-location and restored read plus round trip passed. FIND-1 closed. | PASS |
| REQ-005 file consumers and readiness | Carry forward R1 listing/download/fitter/hosted evidence; shared file owner unchanged. Fresh scoped-read/retry and round trip passed; compensation producer corrected. | PASS |
| REQ-006/007/008, INV-003 delivery and sibling producers | Carry forward R1 genuine row-specific ACK, local/peer owner, bounded wait and unaffected producer/queued-handoff evidence. Incremental outbox change is panic docs only. | PASS, with carried proof limits |
| REQ-015, INV-002 authorization/tenancy/public writes | Carry forward R1 scoped SystemReadAuthority, upstream Card authorization, path binding, Gate reservation and peer boundary. Hydration preserves and precedes IO with tenant/Card checks; fresh scoped-read proof passed. | PASS |
| REQ-017 fresh-install SQL removal and coordination | Carry forward absence/removal source evidence. Required setup now fails explicitly and all three table assertions execute provisioned. FIND-4 closed. | PASS |
| REQ-018 existing physical deletion reflected and recoverable | Both initial callers reuse recorder after durable Failed/pending; existing retry and explicit deletion remain owners. Fresh compensation/refusal/retry/replay and cleanup regressions passed. FIND-2 closed. | PASS |
| Mandatory changed-item docs and diagnostics | Enumerated omissions and new items inspected against actual behavior; safe handler attributes added. FIND-3/5 closed. | PASS |
| Reuse, maintainability, gate integrity and excluded work | Existing hydration projection, compensation owner, SQL lifecycle and test fault seams extended locally. No new dependency, serving inventory, state/queue, scanner, public contract, hashing, retention, permission or deployment change. Prior bounded telemetry/fixture fixes unchanged. | PASS |

The unchanged R1 no-finding rows retain their original source/verification references; none relies on a now-invalid authority or dependency. Later upload projections/evidence tasks and integrated language/performance closure remain outside this task review.

## Remaining ledger and recommendation

**Independently validated remaining ledger: empty.** Stable IDs FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4 and FIND-TASK-001-5 are all CLOSED. Selected discovery proposed no new findings; independent incremental source validation found none. No optional improvement is carried as remediation.

Recommend **PASS**, without a remediation task or specification revision. This is the first remediation review; the two-unsuccessful-round convergence assessment does not apply.

## Verification limits

R2 `verification.md` records fresh repository-managed Postgres migrations, seven selected Card tests, SQL absence proof, expected missing-DSN failure, and clean incremental whitespace check. This validator ran no Cargo jobs and does not claim independent reruns of those recorded commands. Remediation format/lints/codegen and additional Card regressions remain recorded implementation evidence; unaffected catalog, producer/outbox, fitter/CLI/hosted and broader boundary checks carry forward from R1/task evidence.

The hydration proof controls an unmatched required envelope location; the compensation proof controls refusal/recovery, not deterministic concurrent success overlap. Concurrency and cancellation conclusions above are source-derived. Controlled public ACK delay/drop and queued-handoff isolation retain R1's stated experimental-versus-structural limits. No cloud, load, full language matrix or deployment/backfill suite was run; fresh-install scope excludes migration/backfill. These limits introduce no source-proven remaining blocker in this bounded remediation.
