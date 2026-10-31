---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-storage-upload-consolidation
spec_revision: 2
parent_task: TASK-001
requirements: [REQ-004, REQ-005, REQ-013, REQ-018, INV-001, AC-002, AC-006, AC-008]
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5]
depends_on: []
---

# Close inventory hydration and compensation gaps

## Subject and outcome

Approved specification: [revision 2](../../spec.md).
Original task: [TASK-001](../../tasks/TASK-001-object-inventory.md).
Original base: `3899b0a35`; reviewed candidate: `826fcf9c742a36bcae5e8f68307e76685d88244e`.
Independent diagnosis and correction evidence: [findings-validation.md](findings-validation.md).
Route this task directly to `$wyrd-implement`; record the new immutable candidate for closure review.

Hydrated Card reads must require their recorded envelope, and successful failed-declaration cleanup must record its deletion before cleanup is considered finished. For example, if file A completes and sibling B expires, cleaning up the pending Card must remove A's bytes and leave a deletion fact that dominates replayed completion. A failed deletion ACK must leave the Card durably Failed with cleanup pending. Complete the bounded repository-gate corrections in the same task.

## Diagnosis and selected corrections

### Envelope completion is currently optional to hydration — FIND-TASK-001-1

`components/cards/service.rs::hydrate_card` (candidate line 363; URI read at 371–390) receives `CardObjects` but ignores its envelope. `card_files` checks only completed file manifests. A metadata-only Active Card therefore hydrates from `card_blob_uri` and backend bytes even if the acknowledged envelope fact is not visible. File-bearing Cards behave the same when files are visible but the envelope is not. V1 observes healthy envelope presence; V3 hides a file, so neither proves the required dependency. This violates REQ-004/005/013: a pointer identifies an object but cannot replace missing completed-object inventory.

Use the already-resolved envelope at the existing hydration boundary. Preserve current missing-pointer/pending/not-active behavior, then require the expected completion fact before object IO. Absence returns `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE`. Use the recorded location consistently with the retained pointer and preserve existing exact tenant/Card, pointer identity, blob identity and integrity validation. All three hydrated getters share this owner; file listing and selected-file downloads keep their own required-fact behavior. This is a small use of an existing projection. Another query/resolver/cache would duplicate existing lookup; manifest-only checks cannot prove envelope completion.

### Initial successful compensation forgets deletion — FIND-TASK-001-2

`components/cards/service.rs::reconcile_card_claim` (line 1952) and `abort_card` (2295) pass physical cleanup success to `commit_card_failure` (2662). That transaction marks Failed and immediately clears reconciliation without `record_card_deletion`. A normal multi-file declaration can complete A, abandon/fail/expire B, and let the first reconciliation cleanup delete A. The inventory still advertises A as complete, while no cleanup retry remains. Failed Cards are not excluded by the SQL getter, so file listing and download initialization can also consume that stale completion. The same omission exists in initial declaration compensation. Explicit deletion and `reconcile_cleanup_claim` already record the correct tombstone. This violates REQ-018 and TASK-001's deletion-writer coverage; no new retention rule is needed.

Correct the existing compensation cleanup-completion ordering using `fail_card`, `record_card_deletion` and `mark_card_reconciliation_succeeded`. Preserve physical cleanup and the existing Pending-to-Failed transition. For these initial outcomes, durably persist Failed with cleanup reconciliation pending before waiting for deletion recording. Physical cleanup success alone cannot clear reconciliation. Once bytes have been cleaned and the deletion fact satisfies the selected ACK contract, clear cleanup through the existing lifecycle owner. Error or cancellation while recording must leave durable Failed/pending cleanup for the existing claimed cleanup path to retry.

Trace all four sibling physical-cleanup callers once: initial reconciliation, abort compensation, explicit deletion and claimed cleanup retry. Keep the latter two paths' existing lease/transition ownership and avoid assigning two owners to the tombstone in one attempt. Simply awaiting inventory before the existing failure commit can skip the Failed transition on error or cancellation; that is not an acceptable fix. Reader-side Failed filters hide the invalid authority without recording deletion. The correction is bounded lifecycle wiring, with no new durable state, worker, scanner, per-file tombstone scheme or concurrency contract.

### Complete the changed-item documentation gate — FIND-TASK-001-3

Root AGENTS and `agent_docs/rust.md` mandate rustdoc on changed items, including fields/test helpers, real Errors/Panics, and relevant durable partial-progress/retry behavior. The independently validated candidate omissions are:

| Owner | Items to document |
|---|---|
| `crates/wyrd/wyrd-storage/src/service.rs` | `download_url`, `persist_completed_upload`: missing docs/Errors; preserve caller-owned transaction explanation. |
| `crates/wyrd/wyrd-server/src/components/cards/service.rs` | `reconcile_cleanup_claim`: missing Errors; `finish_card_delete`: missing docs; `cleanup_card_blob`, `write_card_blob`: missing Errors and relevant changed durable-write/ACK behavior. |
| `crates/wyrd/wyrd-server/src/components/storage/routes.rs` | `complete`, `download_init`: missing Errors; completion must describe the actual inventory ACK boundary. |
| `crates/wyrd/wyrd-server/tests/integration/pg_card_declaration_route.rs` | Six `ArtifactPrompt` fields; Panics for `send_json`, `complete_request`, `list_artifacts_request`, `download_init_request`. |
| `crates/wyrd/wyrd-server/src/scribe_outbox.rs` | Panics for test `ObjectGate::ingest_frame`. |

Document these and any items materially changed by this remediation against their final behavior. This is bounded text, not an unrelated documentation sweep or a runtime refactor. Existing automated green lanes do not waive the explicit documentation gate.

### Make fresh-install SQL proof require setup — FIND-TASK-001-4

`crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs::fresh_install_has_no_postgres_object_inventory` (1494) returns success when `database_url()` yields None. The helper maps missing/invalid DSNs to None. No migration or table assertion executes, violating the nested SQL rule against silently skipped database proof. Provisioned green runs do not remove that branch.

Require setup explicitly in this new test, preserving the existing helper, migrations, repository-managed Postgres runner and three catalog assertions. A local expectation suffices; update the test's panic documentation. Do not convert the shared helper or rewrite neighboring untouched tests. Verify both provisioned success and missing-DSN failure without a new harness or permanent check.

### Restore the changed-handler tracing gate — FIND-TASK-001-5

`components/storage/routes.rs::{complete,download_init}` (218, 431) lack the root-required handler `tracing::instrument`. Inventory leaf spans cannot cover prior authorization, completion or lookup failures. Add conventional scrubbed instrumentation at these two handlers, excluding state, Caller/credentials, headers and bodies. Reuse safe structured operation/tenant fields and current handler conventions; no telemetry framework, new metric or unrelated route sweep. This requires source inspection and normal checks, not a new behavioral test solely for attributes.

## Preserved behavior and non-goals

Keep shared storage validation, existing tenant/Card authorization and path binding, scoped inventory reads, one Scribe outbox, upload-only ACK isolation, deletion dominance, and existing cleanup failure/lease semantics. Preserve pending-Card response behavior and explicit deletion ownership. Do not alter digest certification, public contracts, retention, physical deletion authority, deployment compatibility or later TASK-002/TASK-003 scope. No extra serving inventory, queue, repair scan, query checkpoint, helper framework, dependency or broad refactor is authorized.

## Focused implementation scenarios and proof

Use existing server/Postgres/outbox controlled seams. Start with `WYRD_LOG=info,wyrd_server=debug`. Run Cargo commands sequentially. For the two behavioral omissions, add/select one focused test at a time, observe the expected behavioral RED against the candidate (not a compile/setup failure), implement the bounded correction, then run GREEN and relevant focused regressions. Static docs/tracing changes require inspection rather than manufactured RED.

| Finding | Situation and required observation |
|---|---|
| FIND-TASK-001-1 | Otherwise valid metadata-only Active Card retains pointer and bytes while envelope fact is unavailable: exact retryable visibility error before envelope IO. Restore matching fact and hydrate successfully. Preserve file-bearing hydration and invalid-pointer binding checks. |
| FIND-TASK-001-2 | Pending multi-file Card has completed A and failed/expired B; initial cleanup removes A, retains Failed, records deletion, and stale A completion cannot restore files. Hold/refuse deletion recording and establish Failed plus cleanup pending are already durable; existing claimed cleanup retries and clears only after accepted deletion. Cover abort compensation's same rule if it does not share the corrected boundary. |
| FIND-TASK-001-3 | Every enumerated omission and newly changed item has meaningful required docs reflecting final error, panic and durable retry behavior. |
| FIND-TASK-001-4 | Exact SQL proof executes all assertions with repository Postgres; with required DSNs absent it fails explicitly. |
| FIND-TASK-001-5 | Both changed handlers have scrubbed instrumentation; no sensitive automatic argument recording. |

The following added selectors are **planned names**, not existing tests. Equivalent focused placement/names may be recorded with actual exact selectors; they do not prescribe fixture structure. Confirm each selector selects a test before recording RED/GREEN.

```bash
# Planned missing-envelope proof.
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_card_declaration_route::hydration_requires_recorded_envelope)'"

# Planned compensation/retry/replay proof; split only if independently necessary.
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_card_declaration_route::compensation_records_inventory_deletion)'"

# Existing SQL test, provisioned.
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_migration::pg_tests::fresh_install_has_no_postgres_object_inventory)'"

# Expected nonzero after correction: setup is absent, so proof must not pass.
mise exec -- env -u WYRD_DATABASE_URL -u WYRD_PLATFORM_DATABASE_URL cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_migration::pg_tests::fresh_install_has_no_postgres_object_inventory)'

# Focused existing Card regressions covering the affected owners.
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_card_declaration_route::completed_objects_use_one_inventory) | test(=pg_card_declaration_route::deleted_objects_survive_completion_replay) | test(=pg_card_declaration_route::inventory_reads_are_scoped_and_retryable) | test(=pg_card_declaration_route::card_reconciler_recovers_after_storage_retry) | test(=pg_card_declaration_route::delete_storage_failure_preserves_cleanup_state)'"

mise run fmt
mise run lints
git diff --check
```

These are focused remediation proofs, not a requirement to rerun full journey suites, language matrices or broad aggregates. Run any additional narrow owner check only when the actual remediation write set requires it. Inventory ACK machinery is not selected for behavioral modification; changing it materially requires reassessing affected scope rather than silently expanding this task.

## Closure handoff

Append observed RED/GREEN, focused regression/static results and material limits, then request `$wyrd-task-review` with prior candidate, new candidate and this task. Retain all original reports and FIND IDs. The next review checks these five closures, the incremental diff and affected callers/consumers; unchanged acceptance evidence carries forward unless the fixes invalidate it.

## Implementation evidence

Prior candidate `826fcf9c7`; remediation commits `c854d9c8e`, `e34b7a78c`, `e83c83296` and `a536ed2a8` on `change/storage-upload-consolidation` (review range `e6a34a010..HEAD`). Reuse: hydration consumes the already-resolved `CardObjects.envelope`; compensation reuses `fail_card`, `record_card_deletion` and `mark_card_reconciliation_succeeded` inside the existing `commit_card_failure`, shared by initial reconciliation and abort compensation. Fault seams are existing ones: the registry pointer, the server's WAL disk-full injection, and `with_limits_for_test`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-1: hydration requires the recorded envelope at the retained pointer, before IO | `cards/service.rs::hydrate_card` refuses with `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE` unless `inventory.envelope` names the pointer's path; missing-pointer, scheme, tenant/Card binding, identity and integrity checks unchanged | `pg_card_declaration_route::hydration_requires_recorded_envelope`: RED against candidate (`left: 200, right: 503`) with identical bytes at an unrecorded path; GREEN, and restoring the recorded pointer hydrates (200) | PASS |
| FIND-TASK-001-2: compensation records deletion before finishing; refusal leaves Failed/pending for the claimed retry | `commit_card_failure` commits `fail_card` (Failed, cleanup pending) first, then `record_card_deletion`, then `mark_card_reconciliation_succeeded` in its own transaction; callers pass the Card row | `pg_card_declaration_route::compensation_records_inventory_deletion`: RED against candidate (`reconcile_kind` `declaration`/idle after first cleanup). GREEN: `a.txt` bytes removed; Failed/cleanup/pending durable while ACK refused; trace shows the initial attempt's ACK `timed_out=true`, then the claimed `kind=cleanup` retry refused (`attempt=1`), still Failed/cleanup/pending; after admission the claimed retry clears to idle; replayed `a.txt` completion stays dominated (`deleted`, no files). Abort compensation shares the corrected boundary | PASS |
| FIND-TASK-001-3: changed-item rustdoc | `wyrd-storage/src/service.rs::{download_url, persist_completed_upload}`; `cards/service.rs::{reconcile_cleanup_claim, reconcile_card_claim, finish_card_delete, abort_card, commit_card_failure, cleanup_card_blob, write_card_blob, hydrate_card}`; `storage/routes.rs::{complete, download_init}` (completion now states the ACK boundary); `ArtifactPrompt` fields and helper Panics; `ObjectGate::ingest_frame` Panics; new test helpers documented | Source inspection; `mise run lints` exit 0 | PASS |
| FIND-TASK-001-4: fresh-install SQL proof requires setup | `pg_migration.rs::fresh_install_has_no_postgres_object_inventory` expects `database_url()`; panic docs updated | Provisioned: `pg_migration::pg_tests::fresh_install_has_no_postgres_object_inventory` PASS. Without `WYRD_DATABASE_URL`/`WYRD_PLATFORM_DATABASE_URL`: FAIL, `the fresh-install proof requires database DSNs`, nextest exit 100 | PASS |
| FIND-TASK-001-5: scrubbed handler tracing | `#[tracing::instrument(skip(state, caller, headers, body), fields(operation = "storage.upload.complete"))]` and `skip(state, caller, body), fields(operation = "storage.download.init")`; only the upload id path segment is recorded | Source inspection; route regressions below | PASS |

Focused regressions, sequentially through `scripts/postgres/with-test-postgres.sh` with migrations: `completed_objects_use_one_inventory`, `deleted_objects_survive_completion_replay`, `inventory_reads_are_scoped_and_retryable`, `card_reconciler_recovers_after_storage_retry`, `delete_storage_failure_preserves_cleanup_state`, `blob_storage_failure_leaves_durable_failure_state`, `card_reconciler_dead_letters_after_three_failures`, `heavy_declaration_initializes_upload_after_commit`, `card_reads_list_latest_and_delete_are_tenant_safe`, `flat_card_http_contract`, plus both new tests: all PASS (route subset rerun after instrumentation: 5/5). `mise run fmt`, `mise run lints` (exit 0), `mise run codegen:check` (exit 0), `git diff --check`: clean.

Non-goals stayed excluded: no new query, resolver, cache, durable state, worker, scan, per-file tombstone, reader-side Failed filter, public contract, ACK machinery change, dependency or later-task scope. Only the six listed source/test files and this task changed. Limit: the unleased success update after a held initial ACK may clear a concurrently claimed cleanup lease; that claim then records an idempotent duplicate tombstone and its lease-fenced success is a no-op.
