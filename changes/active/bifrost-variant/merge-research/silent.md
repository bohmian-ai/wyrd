# Silent conflicts: merge origin/main (8a8813a74) into wyrd/bifrost-variant/TASK-003 (cfe21dc90)

Merge base: 5ab92b003. Merged tree used for evidence: `git merge-tree --write-tree HEAD origin/main` = `574f4069dcb10f1f3dc50478d0419486ff77fb2c` (written as `M:` below).
Scope: these files auto-merge cleanly or are new on one side, but break after the merge. None of them is in `../conflict-files.txt`. Nothing was compiled or run. Every claim comes from source evidence. Anything that needs compile proof is marked **unknown**.

## Summary

| Kind | Count | High | Medium | Low |
|---|---|---|---|---|
| Migration/boot (roles, RLS) | 3 | 2 | 1 | 0 |
| Compile break (removed symbol used by auto-merged or new file) | 7 | 5 | 2 | 0 |
| Runtime/semantic break (wyrd_event_time, active reads) | 5 | 2 | 2 | 1 |
| Orphaned test modules / targets / CI paths | 4 | 1 | 2 | 1 |
| Bench bin consolidation | 3 | 1 | 1 | 1 |
| Generated artifacts / docs prose | 3 | 0 | 2 | 1 |
| **Total** | **25** | **11** | **10** | **4** |

Top 5 hazards:
1. **Ours' new vala-sql migrations grant to `wyrd_app`, a role that main no longer creates.** The migration fails with "role does not exist". Even with the grants removed, `forge_snapshot_supersessions` lacks the `operator_access` policy, so main's `verify_row_security` refuses serving boot.
2. **Main-only files compile against APIs that ours removed.** The affected files are `scribe_outbox.rs` (`ObjectStorePutFailed`, `project_audit_event`), `query/collect.rs` (`QueryTerminalError`), `forge/snapshot_expiration.rs` (`ActiveReadOwner`), `owner_inspection.rs` (`StorageLifecycle`), `observe/tests.rs` (`WYRD_EVENT_TIME`) and `scribe/describe.rs` (`BatchBuilder`, `wyrd_queue::schema`).
3. **The event-time models split.** Main's REQ-187 has the client stamp `wyrd_event_time`. Ours removes that column and uses each table's own time column. The auto-merged `bifrost/handle.rs`, main-new `observe_ingest.rs` and `capacity/load.rs`, and four docs pages still use main's model.
4. **wyrd-client e2e test layout.** Main folded tests into `tests/integration/main.rs`. Ours' new `tests/bifrost/*.rs` and `tests/pg_bifrost_e2e/variant_tables.rs` become orphaned, so they compile into nothing and pass silently. Ours' mise `--test pg_bifrost_e2e` and the CI selector regex also go stale.
5. **Capacity benches.** The merged `bifrost_query_capacity` bin is missing `workload.rs`, because main deleted it cleanly. `profile.rs` now exists twice: main has `capacity/profile.rs` and ours has `wyrd-testing/src/profile.rs`.

## Table

| File (merged tree) | Main intent | Ours intent | Resolution | Risk | Proof |
|---|---|---|---|---|---|
| `crates/vala/vala-sql/migrations/20261009000000_bifrost_snapshot_retention.sql` (ours-new) | No Wyrd roles or grants. Every RLS table has `operator_access` plus `tenant_isolation`. | Retention, supersessions table, `oracle_read_table_cut`. Grants to `wyrd_app`. | Drop the GRANTs (L142, L172) and the `REVOKE ... FROM PUBLIC`. Add an `operator_access` policy to `vala.forge_snapshot_supersessions`. | High | `mise run test:sql`, `test:bifrost` |
| `crates/vala/vala-sql/migrations/20261011000000_scribe_wal_fenced_lsn.sql` (ours-new) | Same as above | Fenced-LSN function, SECURITY DEFINER, grant to `wyrd_app` (L29) | Drop the GRANT and REVOKE FROM PUBLIC. Re-check whether SECURITY DEFINER is still needed when there are no roles (**unknown**). | High | `mise run test:sql` |
| `crates/vala/vala-sql/migrations/20261010000000_bifrost_cut_notify.sql` (ours-new) | Same as above | REVOKE on a trigger function | Harmless (trigger EXECUTE is checked at CREATE). For consistency, drop it. | Low | `test:sql` |
| `crates/vala/vala-sql/tests/integration/pg_oracle_membership.rs:856` (auto-merged) | — | Asserts grants to `wyrd_app` | Delete the grant assertions | Medium | `test:sql` |
| `crates/wyrd/wyrd-server/src/scribe_outbox.rs` (main-new) | Audit outbox to Scribe. Maps `ScribeError` and calls `project_audit_event(event, decided_at)`. | Removed `ScribeError::ObjectStorePutFailed`, `EventTimeOutOfRange`. Added `ContractViolation`. Projection is `project_audit_rows`, with audit `created_at`. | Drop the L357 arm (`Internal` already maps to Unavailable; `_ => Rejected` covers ContractViolation). Adapt to the resolved projection.rs signature. Main's `decided_at` feeds ours' `created_at`. | High | `mise run lints`, `test:wyrd` |
| `crates/wyrd/wyrd-server/src/query/collect.rs` (main-new) | Collects query frames. Uses `terminal_error_to_bifrost(error.code)` (L496), `QueryTerminalError` / `QueryTerminalErrorCode` (L670, L810). | Terminal frame carries `error: Option<Box<WyrdProblem>>`. Uses `service::terminal_error(Option<&WyrdProblem>)` (HEAD service.rs:372). | Rewrite as `terminal_error(frame.error.as_deref())`. Tests build a `WyrdProblem`. | High | `lints`, `test:bifrost` |
| `crates/vala/vala-bifrost-redux/tests/integration/forge/snapshot_expiration.rs` (auto-merged) | 0e1968570 adds `cut_pointer` helper using `oracle_reader_authority::ActiveReadOwner` (L25), `acquire_active_cut`, `release_active_reads` (M: L1446-1470), and counts `vala.oracle_active_table_reads` (L1517). | ce3474fa7 removed active reads in favour of retention and supersessions. | Rewrite the helper on `oracle_read_table_cut`, or drop main's added cases. | High | `test:bifrost` |
| `crates/wyrd/wyrd-testing/tests/bifrost/server/owner_inspection.rs` (auto-merged) | Asserts `storage.inspect().lifecycle == StorageLifecycle::Closed` (M: L428-431, L482-485) | 4559cff37 deleted `storage/cache.rs`, `StorageLifecycle`, `StorageInspection` | Remove those assertions or replace them with ours' close proof (**unknown**: equivalent API) | High | `test:bifrost` |
| `crates/shared/wyrd-client/src/observe/tests.rs` (auto-merged) | Tests at L1319-1485 use `managed_columns::WYRD_EVENT_TIME` and keep the caller-supplied event time | Constant removed. Drift/eval use client `created_at`. | Delete the tests, or rewrite them on `created_at` | High | `test:shared` |
| `crates/wyrd/wyrd-testing/tests/bifrost/scribe/describe.rs` (main-new, `mod describe;` in scribe/main.rs) | Uses `wyrd_queue::batch_builder::BatchBuilder::from_description`, `append_json_row`, `finish_ipc` (L153-172), `tables::arrow_type_shape_matches` (L99), `wyrd_queue::schema`, `wyrd_event_time` (L87-90, L124) | `RowPreflight::from_description` / `prepare` / `PreparedRows::to_ipc`. `wyrd_queue/src/schema.rs` deleted. | Port to `RowPreflight` and drop the event-time assertions. If `arrow_type_shape_matches` is gone, drop that check (**unknown**: whether it still exists). | Medium | `test:bifrost` |
| `crates/shared/wyrd-client/src/bifrost/handle.rs` (auto-merged) | `insert_rows(..., event_time_micros)`, `enqueue_rows(rows, card_uid, run_id, event_time_micros)` (L231). Docs at L208/212 mention wyrd_event_time. | `enqueue_rows(rows, card_ref, run_id)` | Keep `card_uid` (main) and drop `event_time_micros` (ours). Fix callers (`insert_observation` path) and the docs. | High | `lints`, `test:shared`, `test:bifrost` |
| `crates/wyrd/wyrd-testing/tests/bifrost/server/observe_ingest.rs` (main-new) | Writes `"wyrd_event_time"` (L282) and selects `CAST(wyrd_event_time AS BIGINT)` from drift/eval observations (L289-302) | Column removed. `wyrd_*` input refused. | Rewrite on `created_at` | Medium (runtime, compiles) | `test:bifrost` |
| `crates/wyrd/wyrd-testing/src/bin/capacity/load.rs` (main-new) | Queries `wyrd_event_time` on a dataset table (L489-497) | Table partitions on its declared time column or `wyrd_ingested_at` | Use the dataset's declared time column | Medium (runtime) | Run the bench manually |
| `crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity/*` (ours-kept) | Deleted the whole bin; `workload.rs` deleted cleanly | Kept and extended (`logs.rs`, `telemetry.rs`) | Either delete the bin (and its mise tasks) or restore `workload.rs` from HEAD (`logs.rs:22`, `run.rs` and `report.rs` use `crate::workload`) | High | `lints` |
| `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:30` + `capacity/profile.rs` (main) vs `wyrd-testing/src/profile.rs` (ours, +314 lines off-CPU) | `use crate::profile::{Capture, Profile}` | Profile moved to the lib | Delete `capacity/profile.rs` and point `step.rs` at `wyrd_testing::profile` (**unknown**: whether the API matches) | Medium | `lints` |
| `crates/wyrd/wyrd-testing/src/bin/verification_capacity/` | Renamed to `capacity` (merged tree keeps only `main.rs`, `step.rs`) | Moved `profile.rs` out | Delete the leftover `verification_capacity/` dir and its `[[bin]]`, if any | Low | `lints` |
| `crates/shared/wyrd-client/tests/bifrost/{every_iceberg_column_type,register_a_table_from_a_model,table_time_column,three_timestamp_types}.rs`, `tests/pg_bifrost_e2e/variant_tables.rs` (ours-new) | `tests/integration/main.rs` with `mod pg_bifrost_e2e;`. No `[[test]] pg_bifrost_e2e` (515f16c97). | Included via `#[path]` from `tests/pg_bifrost_e2e.rs` (modify/delete conflict) | Move under `tests/integration/pg_bifrost_e2e/` and add `mod` lines. Otherwise they silently never compile or run. | High | `mise exec -- cargo nextest list -p wyrd-client --test integration` shows them |
| `mise.toml` tasks (conflict file; affects silent targets) | `--test integration -E 'test(/^pg_bifrost_e2e::/)'` | `--test pg_bifrost_e2e` (HEAD mise.toml L308, L446-447) | Use main's selector | Medium | `test:bifrost` |
| `.github/scripts/select-ci.py:51` (auto-merged) | BIFROST_ONLY regex `tests/integration/pg_bifrost_e2e\.rs` | New files in a directory | Widen to `tests/integration/pg_bifrost_e2e(/|\.rs)` | Medium | Read the script; `TESTING.md:13` |
| `sdks/wyrd-sdk-ts/wyrd/tests/support/test-server.ts` (ours-new) | `support/server.ts` `serverTest`/`deploy`, with WYRD_CONFIG_HOME isolation and token/tenant env unset | `useTestServer`, `columnValues` without isolation | Port ours' 5 TS integration tests to `serverTest` and delete `test-server.ts`. Otherwise saved logins leak into the tests. | Medium | TS integration task |
| `sdks/wyrd-sdk-python/tests/**/test_variant_tables.py` (ours-new) | Session fixtures `wyrd_server`, `bifrost`, `cards` | Local `bifrost` fixture | Works (the local fixture overrides). Optionally reuse main's fixture. | Low | `py:test:unit` / integration marker |
| `sdks/wyrd-sdk-python/tests/unit/bifrost/test_reserved_column.py` (main) | Expects `card_uid` reserved | Reserves `card_ref` (batch_builder is a conflict file) | Follows the batch_builder resolution (card_uid) | Medium | `py:test:unit` |
| `crates/wyrd-spec/schemas/bifrost_*.json` (auto-merged, main's location) | Schemas moved from `tests/schemas/` | Ours edited `tests/schemas/` (deleted on main) | Regenerate | Medium | `mise run codegen:check` |
| Docs prose | Describes `wyrd_event_time`: `architecture/wyrd-security-posture.md:495`, `docs/src/content/docs/bifrost/architecture.svx:337`, `reading-data.svx:94,125`, `schema.svx:82`, `architecture/wyrd-design.md:842-844` | No managed event time | Rewrite these to the declared time column model | Medium | `mise run docs:check` |
| `crates/wyrd/wyrd-testing/src/gateway_capture.rs` (ours-new) | — | Uses `with_gateway_provider_root_for_test`, `start_bound`, `data_tenant_id`, `exchange_api_key`, `bootstrap_service` | None; all of them exist on main | Low | `lints` |

## Silent conflicts (evidence)

### 1. Roles and RLS (migration, then boot)
- Main 6c23a6f45: Wyrd creates no roles. All `wyrd_app` / `wyrd_platform_admin` grants are gone, and no vala-sql migration on main contains GRANT/REVOKE.
- At serving boot, `crates/wyrd/wyrd-sql/src/schema_check.rs::verify_row_security` (called from `crates/vala/vala-sql/src/lib.rs:67`) requires `operator_access` (TO CURRENT_USER, USING `wyrd.operator_session()`) plus `tenant_isolation` on every RLS table. Otherwise it returns SchemaNotReady.
- Ours (M) `20261009000000_bifrost_snapshot_retention.sql`:
  - L142 `GRANT EXECUTE ON FUNCTION vala.oracle_read_table_cut(uuid, jsonb) TO wyrd_app;`
  - L172 `GRANT SELECT, INSERT, DELETE ON vala.forge_snapshot_supersessions TO wyrd_app;`
  - The new table has only `tenant_isolation`.
- Ours `20261011000000_scribe_wal_fenced_lsn.sql`:
  - L29 grants to `wyrd_app`.
  - `REVOKE ALL ON FUNCTION vala.scribe_wal_newest_fence(...) FROM PUBLIC`, and the function is SECURITY DEFINER.
  - Under main's model the tenant login is not the owner. If someone keeps the REVOKE, a separate tenant login cannot EXECUTE.
- The DROP of `oracle_acquire_table_cut(uuid, uuid, bigint, bigint, jsonb)` still matches main's 20260910000025 (L186-191), so that part is safe.
- Main edited old migrations in place (20260910000025, and 20260910000021, which added `operator_access` on `scribe_batch_commits`). Ours' in-place comment edit to the olap_minimal migration only changes its checksum. That is low risk, because Bifrost is unshipped.
- Main's old tests that touch `oracle_active_table_reads` are `forge/reader_expiry_ordering.rs`, `forge/support.rs` and `vala-sql/tests/integration/pg_forge_tasks.rs`. They lose that reference in M, and `oracle/distributed.rs:4616` is in the conflict list. **Only `snapshot_expiration.rs` remains silent.**

### 2. Removed APIs used by main-new or auto-merged files
- `scribe_outbox.rs:357`: `ScribeError::ObjectStorePutFailed(_)` no longer exists (ours removed it in `contracts.rs`). The file also calls `project_audit_event(event, decided_at)`, while ours has `project_audit_rows(staging rows)`. `projection.rs` is a conflict file, but this caller is not.
- `query/collect.rs`:
  - L496 calls `super::service::terminal_error_to_bifrost(error.code)`.
  - L670 and L810 use `QueryTerminalError` / `QueryTerminalErrorCode`.
  - Ours (70142ba09) replaced these with `WyrdProblem` frames.
- `forge/snapshot_expiration.rs`:
  - M:L25 imports `vala_sql::queries::oracle_reader_authority::ActiveReadOwner`.
  - M:L1446-1470 call `acquire_active_cut` and `release_active_reads`.
  - M:L1517 does `SELECT count(*) FROM vala.oracle_active_table_reads`, and ours dropped that table.
- `server/owner_inspection.rs` M:L428-431 and L482-485 use `vala_bifrost_redux::storage::StorageLifecycle::Closed` (from main 5eff7ec29 and a14896e34). Ours deleted it.
- `observe/tests.rs` M:L1319-1485 use `wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME`.
- `scribe/describe.rs` uses:
  - L153-172: `BatchBuilder`
  - L99: `arrow_type_shape_matches`
  - `wyrd_queue::schema`
  - L87-90 and L124: `wyrd_event_time`

### 3. Event-time semantics (compiles, but behaves wrongly)
- Main REQ-187 (6e84727c9): `insert_observation` and `WriterPool::insert_rows(..., event_time_micros: Option<i64>)` stamp a client clock into `wyrd_event_time`. Auto-merged `wyrd-client/src/bifrost/handle.rs:231` calls `enqueue_rows(rows, card_uid, run_id, event_time_micros)`, which does not exist on ours.
- Ours TASK-005:
  - Partitions on the declared `time_column`, with `wyrd_ingested_at` as the fallback.
  - Removes `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE`.
  - Drift and eval already stamp a client `created_at` (`observe/drift.rs:104`).
  - `observe.record` into user tables and verify observations lose the client event time. See Decisions.
- `observe_ingest.rs` L282-302 and `capacity/load.rs` L489-497 read `wyrd_event_time` at runtime.

### 4. Orphans and stale targets
- Merged `crates/shared/wyrd-client/Cargo.toml` has no `[[test]] pg_bifrost_e2e`. `tests/integration/main.rs` has `mod pg_bifrost_e2e;`.
- Ours' new `tests/bifrost/*.rs` and `tests/pg_bifrost_e2e/variant_tables.rs` are reached only through `#[path]` from the deleted-on-main `tests/pg_bifrost_e2e.rs`.
- `.github/scripts/select-ci.py:51` matches only the old `pg_bifrost_e2e\.rs`. `TESTING.md:13` documents it.
- `bifrost_query_capacity/logs.rs:22` has `use crate::workload::{DAY_HOURS, DAY_START_MS, HOUR_MS, mix};`, and `workload.rs` is absent from M.

### Checked, no silent issue
- card_ref vs card_uid: every remaining delta is in conflict-list files.
- `docs/.../api/errors.md` is not generated from the catalog.
- The `lru` dependency is still used on main (`verification/cache.rs`).
- Deleted crates (skald-observer, wyrd-test-contract-macros) appear only in `scripts/test-families.sh`, which is a conflict file.
- `wyrd-types` (main-new crate): the client-tier deny lists are unaffected. It needs workspace-hack regeneration (a conflict file).
- `ambiguous_retry.rs` types exist on both sides. **Unknown**: whether the `IngestTransport` trait signatures agree (needs a compile).

## Open decisions for the human
1. **Client event time.** Main's REQ-187 lets the client stamp the observation time. Ours drops the managed column. Should `observe.record` and verify writes into user tables carry a client time in the table's declared time column, or rely on `wyrd_ingested_at`? Drift and eval are already covered by `created_at`.
2. **Snapshot-expiry safety model.** Main bars expiry with active-read rows (`active_read_refusal` in `expire.rs` and the snapshot_expiration tests). Ours uses retention plus supersessions. Pick ours and rewrite main's added tests, or carry main's unknown-outcome guarantee into the retention model.
3. **Benches.** Should ours' `bifrost_query_capacity` and `bifrost_client_queue` stay beside main's unified `capacity` bin, or fold into it? This decides whether `workload.rs` comes back and which mise tasks survive.
4. **Audit time.** Should ours' audit `created_at` ("PostgreSQL decision time") take main's outbox `decided_at` (server `Utc::now()`)? That would break the AGENTS.md §15 "PostgreSQL owns coordination timestamps" reading for audit, unless `created_at` is treated as a domain event time.

Unknown until compiled after the merge: the `IngestTransport` signatures, `forge_operations.rs` merge consistency, and the `wyrd_testing::profile` API match. Proof lanes: `mise run lints`, `test:sql`, `test:bifrost`, `test:shared`, `codegen:check`, `py:test:unit`, `docs:check`, and the TS integration task.
