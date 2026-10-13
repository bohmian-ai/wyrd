# Merge research: server / SQL group

Group: wyrd-server, wyrd-tonic, wyrd-storage, wyrd-mcp, wyrd-cli, vala-sql.
OURS = HEAD `cfe21dc90`; THEIRS = origin/main `8a8813a74`; base `5ab92b003`; merge-tree `574f4069d`.
Research was read-only. Nothing was compiled or run. Every "proof" below is a test or lane to run after the merge.

## Summary

### Counts by resolution (30 conflicted files)

| Resolution | Count | Files |
|---|---|---|
| Take main / delete | 9 | audit/mod.rs, bifrost/gate_audit.rs, verification/publisher.rs, verification/runner.rs (carry the ours semantics noted in its row), wyrd.v1.bin, vala-sql queries/audit_staging.rs, vala-sql tests/pg_audit_staging.rs (move one test out first), pg_verification_routes.rs, pg_verification_runtime.rs |
| Take ours | 6 | oracle_reader_authority.rs, query/service.rs, query_conversion.rs, wyrd-storage local.rs, wyrd-mcp tests query.rs, mcp/bifrost.rs (take main's deletion and port ours into query/collect.rs) |
| Combine | 14 | wyrd-cli Cargo.toml, wyrd-server Cargo.toml, app/server.rs, bifrost/service.rs, boot/mod.rs, gateway/capture.rs, http/otlp.rs, state.rs, drift.rs, eval.rs, observations.rs, results.rs, pg_grpc_ingest_smoke.rs, pg_oracle_membership.rs, wyrd.v1.proto |
| Recompute | 1 | vala-sql lib.rs (olap_minimal checksum) |

### Counts by risk

| Risk | Count |
|---|---|
| High | 9 |
| Medium | 13 |
| Low | 8 |

### Top 5 hazards

1. **Our new migrations fail under main's role model, and they merge cleanly, so nothing flags them.**
   - Files: `20261009000000_bifrost_snapshot_retention.sql`, `20261010000000_bifrost_cut_notify.sql`, `20261011000000_scribe_wal_fenced_lsn.sql`.
   - They GRANT to `wyrd_app`, which no longer exists, so migrate fails.
   - `forge_snapshot_supersessions` has no `operator_access` policy, so `verify_row_security` fails.
   - The `scribe_wal_newest_fence` SECURITY DEFINER never raises `app.operator`, so `current_tenant()` errors.
2. **Audit outbox swap across boot, state, bifrost/service and capture.**
   - Main retired staging, the publisher, `append_audit` and `AuditUnavailable`. `ScribeOutbox` is now the `GateAudit`.
   - Ours still threads `OracleQueryAudit` / `audit_outbox`. Ours' redux `register_dataset` (`bifrost_catalog.rs:1672-1686`) still calls `audit_staging::append_audit`.
3. **Terminal-error wire change.**
   - Ours replaced `QueryTerminalErrorCode` with `error_problem_json` / `Option<Box<WyrdProblem>>`.
   - Main's `query/collect.rs` merges clean but still calls `terminal_error_to_bifrost` (lines 496, 670, 812).
   - The client, redux oracle and query_stream still reference the deleted enum.
4. **Verification observation and result schemas.**
   - Ours uses a declared per-table time column (`created_at` / `ended_at`) and typed Variant columns.
   - Main uses `card_uid` correlation, its `wyrd_event_time` assumption (ledger L36), and a generic `details` column for the task Verifier kind (`284e4bc92`).
5. **Snapshot-expiration claim refusal.**
   - Main `0e1968570` makes the reader refuse tables with unresolved expiration claims (55000 → Conflict).
   - Ours' read-only `oracle_read_table_cut` plus the retention window drops that guard. That may be correct, but nobody has decided it.

## Resolution table

| file | main intent (commits) | ours intent (commits) | resolution | risk | proof |
|---|---|---|---|---|---|
| vala-sql/src/lib.rs | olap_minimal edited for the role model (`6c23a6f45`) | olap_minimal edited for the time-column change (`cfe21dc90`) | Recompute the digest from the merged `20260619000001_olap_minimal.sql`. Computed: `b09fc1af94501b55380fd5d4a9ba64d7071e119e4037642e9c4b4a919edcb537` | low | `vala-sql` lib `forge_and_oracle_migrations_have_unique_forward_owners`; `mise run test:bifrost:integration:sql` |
| vala-sql/src/queries/audit_staging.rs | deleted; staging retired (`70a6f4a4e`) | `entry_hash` made pub (`5a30d46f5`) | delete. The wyrd-testing `verification_runtime.rs:39,1291` retained-hash test breaks silently; the testing group owns it | medium | `cargo check` of wyrd-testing tests |
| vala-sql/tests/pg_audit_staging.rs | deleted (`515f16c97`, `70a6f4a4e`) | added `newest_wal_fence_spans_tenants_on_one_stream` | Delete the file. Move that one test to `tests/integration/pg_scribe_batch_commits.rs` (plus `mod` in `tests/integration/main.rs`), adapted to sync `superuser_pool()` (L19) and the `wyrd_tenant` login | medium | the exact nextest for the moved test via `test:bifrost:integration:sql` |
| vala-sql/src/queries/oracle_reader_authority.rs | `acquire` gains ActiveReadOwner, deadline and expiration-claim refusal (`0e1968570`) | replaced by read-only `read(&[CutTableRef]) -> Vec<TableCut>` and `cut_error` (`ce3474fa7`, `1190c539a`) | take ours; decision (a) below | high | `pg_oracle_membership` |
| vala-sql/tests/integration/pg_oracle_membership.rs | ActiveReads harness, `has_function_privilege('wyrd_tenant',…)`, owned_by_platform | `grants()` helper asserting `wyrd_app` grants and `!public_exec` | Keep ours' cut tests. Drop `grants()`, the wyrd_app and PUBLIC assertions, and main's active-read and claim tests. Use the `wyrd_tenant` role | medium | `test:bifrost:integration:sql` |
| wyrd-cli/Cargo.toml | `webbrowser`, `wyrd-client` features `["internal"]` | `wyrd-queue` (Variant rendering) | union | low | `cargo check -p wyrd-cli` |
| wyrd-server/Cargo.toml | `num-traits` | `opendal` moved to dev-deps, `wyrd-types` added, `cloud` feature edited | union; `opendal` in dev-deps only (merged boot has 0 uses; eval.rs uses it only in tests) | low | `cargo check -p wyrd-server --all-features` |
| wyrd-server/src/app/server.rs | `audit_publisher` block removed | `oracle_cut_cache` `watch_cuts` worker | Keep the `watch_cuts` worker and drop the publisher block. Merged lines 937 and 947 already call sync `bifrost.abort()` | medium | `test:bifrost:journey:server` |
| wyrd-server/src/audit/mod.rs | no `publication` module, no `append_audit` (`70a6f4a4e`) | small touch-ups | take main | low | compile |
| wyrd-server/src/bifrost/gate_audit.rs | deleted; `ScribeOutbox` implements `GateAudit` (`d2af088b9`) | `OutboxGateAudit` over `OracleQueryAudit` | delete | low | compile |
| wyrd-server/src/bifrost/service.rs | outbox staging first, describe-then-register, 5-argument `register_dataset` (L13) | all outcomes decided under the registration lock; `check_supported`; `spec_to_field(spec,true)`; `Some(allowed.clone())` | Combine: main's up-front staging with no `record_allowed`/`AuditUnavailable`, plus ours' `check_supported` and catalog-decided outcome. Redux must drop the audit parameter from `register_dataset` | high | `test:bifrost:integration:server`, `test:bifrost:journey:sdk` |
| wyrd-server/src/boot/mod.rs | `map_or(..., Duration::from_secs)` (L17); `event_time_window`; `scribe_outbox`; `membership_poller`; `OracleMemoryResources.audit`; test `from_hours(1)` | `snapshot_retention`; `EventTimeWindow` removed; `OracleQueryAudit` `audit_outbox`; `reconciliation_limit_bytes` removed (`f5bbac499`) | Per hunk:<br>• l.280: main's map_or plus `snapshot_retention`<br>• l.651: drop `event_time_window`<br>• l.886/1010/1034: main's `scribe_outbox`/`membership_poller`/`observation_runs`, with `.with_audit(Arc::clone(&scribe_outbox))`<br>• l.1793: ours' `OracleMemoryResources{resources}` plus main's `audit`<br>• l.2392: assert `from_hours(1)` and `snapshot_retention == from_hours(12)` | high | boot unit tests; `test:wyrd` |
| wyrd-server/src/components/gateway/capture.rs | routing and CaptureDrop moved into scribe_outbox (`f7b962d9e`); `card_uid`/`run_id` subject columns (`c8febbd49`) | Variant `request_payload`/`response_payload` via `VariantColumnBuilder` (`2227ba577`, `0545aa97c`) | Main's structure and subject stamping plus ours' Variant payload projection. Test l.1076 becomes `["card_uid","run_id","request_payload","response_payload"]`. Depends on the redux `CallsTable` schema | high | capture unit tests; gateway journey |
| wyrd-server/src/http/otlp.rs | 401 text (x-wyrd-api-key, `WYRD_AUTH_401_API_KEY_INVALID`); 400 includes `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE` | 400 text "touched more than 32 time partitions…", `WYRD_VALA_400_OTLP_REQUEST_MALFORMED` | ours' 400 plus main's 401. Drop EVENT_TIME_OUT_OF_RANGE, which exists only on main (`wyrd-spec/src/vala/error.rs`); the spec group must decide whether to delete it | medium | `test:principals:integration` (`pg_openapi_contract`) |
| wyrd-server/src/mcp/bifrost.rs | ResultCollector and tests moved to `query/collect.rs` (`db9fda55c`) | WyrdJsonEncoderFactory, problem-bearing terminal | Take main's move, then port ours into `query/collect.rs` (see silent conflicts) | high | `wyrd-mcp` query tests; collect.rs unit tests |
| wyrd-server/src/query/service.rs | `terminal_error_to_bifrost` | `terminal_error(Option<&WyrdProblem>)` via `wyrd_client::error::from_problem` | take ours | medium | query unit tests |
| wyrd-server/src/state.rs (15 hunks) | `ServerGate=Gate<ScribeOutbox>`, `process_drain`, `observation_runs`/`scribe_outbox`/`membership_poller`, storage close/abort at shutdown | `audit_outbox`, `judge_providers`, sync `abort()`/`abort_selected_owners()`, `poller.abort()`, no storage close/abort (`4559cff37`) | Main's fields, names and outbox construction. Drop `audit_outbox` and `judge_providers`. In shutdown keep main's poller stop but delete `storage.close(deadline)` and `storage.abort().await`. Keep ours' sync aborts. Remove the storage claims from the shutdown doc | high | `test:bifrost:journey:server`; shutdown tests |
| wyrd-server/src/verification/drift.rs | `.collect::<Float64Array>()` style; `wyrd_event_time` | `created_at` time column | main's style plus `created_at` | medium | `test:bifrost:journey:drift` |
| wyrd-server/src/verification/eval.rs | `PrincipalId` imports; doc mentions `run_id`/`principal_id` | `variant_cell_to_json` (used at merged l.685, 922); `created_at` | Union the imports. Doc l.622: `created_at` plus `run_id`/`principal_id` | medium | eval unit tests |
| wyrd-server/src/verification/observations.rs | `ObservationRunOutbox` staging (`9b560d005`, `990f922a9`, `dadaa3ab9`) | `created_at` event time | Main's outbox plus `created_at`. Redux `EvalObservationsTable::acknowledged` signature differs: main `(frame, receipt_micros)`, ours `(frame, principal)` | high | `test:bifrost:journey:observe` |
| wyrd-server/src/verification/publisher.rs | deleted (`8b7218a92`) | boxed-error change | delete | low | compile |
| wyrd-server/src/verification/results.rs | correlation `[CARD_UID, RUN_ID]` (`ad27e71f3`); `WYRD_EVENT_TIME`; task-kind `details` (`284e4bc92`) | typed `drift_report`/`eval_summary` Variant, `UTC_TIME_ZONE`, `ended_at` | `[CARD_UID, RUN_ID]` with no `WYRD_EVENT_TIME`; ours' typed columns; main's collect style. Decision (b) below | high | results unit tests; verification journeys |
| wyrd-server/src/verification/runner.rs | result staged on the outbox before completion; `publish` removed (`37156ea9c`, `eaf088b5e`) | publication-failure logging (`1777ae7d1`); `ended_at` | Take main. Carry `ended_at` semantics into main's builder. Port the logging only if an equivalent failure point exists (unknown) | medium | `pg_verification_runtime` |
| wyrd-server/tests/integration/pg_grpc_ingest_smoke.rs (main relocated) | `await_audit_retained`/`retained_audit_records`; `card_uid`; hand-listed Utf8 `details` schema | `ResultsTable::arrow_fields()`-driven `verification_result_ipc` | Main's audit assertions and `card_uid`, plus ours' arrow_fields-driven IPC (L23: apply at the integration path) | medium | `test:bifrost:integration:server` |
| wyrd-server/tests/integration/pg_verification_routes.rs | relocated | `wait_audit_staged` drains only | take main | low | `test:wyrd` |
| wyrd-server/tests/integration/pg_verification_runtime.rs | relocated | `wait_audit_staged` drains only | take main | low | `test:wyrd` |
| wyrd-storage/src/local.rs | async with `#[allow]` | sync, no `#[allow]` (AGENTS §6) | Take ours. cloud.rs awaits only S3/Gcs/Azure | low | `test:storage:matrix` |
| wyrd-tonic/proto/wyrd.v1.bin | deleted (`515f16c97`, L08) | regenerated | delete | low | `codegen:check` |
| wyrd-tonic/proto/wyrd.v1.proto | `QueryParam`, `params = 5` | no `QueryTerminalErrorCode`/`QueryTerminalError`; `error_problem_json = 9`; reserved 2/6 | Ours plus main's params (the merged tree already has both). Fix the l.252 comment `card_ref` → `card_uid` | medium | `codegen:check`; tonic tests |
| wyrd-tonic/src/query_conversion.rs | `proto_terminal_error` | `terminal_error(bytes)` | ours; keep main's params conversion (merges clean) | medium | `query_conversion::tests::failed_terminal_problem_round_trips` |
| wyrd-mcp/tests/bifrost/mcp/query.rs | hex Variant rendering | Variant rendered as a JSON value | take ours | low | wyrd-mcp tests |

## Silent conflicts (they merge cleanly or sit outside hunks, and are still broken)

### Migrations under main's role model

Evidence: `6c23a6f45`; `vala_sql::verify_schema` → `verify_row_security(RowScope::Tenant)`; the definer pattern in `platform.resolve_tenant_by_slug`.

- **Roles.** `20261009000000_bifrost_snapshot_retention.sql`, `20261010000000_bifrost_cut_notify.sql` and `20261011000000_scribe_wal_fenced_lsn.sql` contain `GRANT … TO wyrd_app` and `REVOKE … FROM PUBLIC`. Main has no `wyrd_app`, so migrate fails. Fix: delete the GRANT/REVOKE lines.
- **RLS policy.** `vala.forge_snapshot_supersessions` (20261009) has `tenant_isolation` but no `CREATE POLICY operator_access … TO CURRENT_USER USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session())`. `verify_row_security` rejects it.
- **Definer.** `scribe_wal_newest_fence` (20261011) is SECURITY DEFINER over a FORCE-RLS table and does not `PERFORM set_config('app.operator','on',true)` and restore. Under main, `wyrd.current_tenant()` raises in a tenant session with no binding. Fix: copy main's definer pattern, or drop the definer and call through `OperatorPool`. This is decision (d).
- **Caller.** The caller is redux `scribe/replay.rs:244` (`newest_wal_fence(&mut PgConnection)`). Whether it trips main's `scripts/check_tenant_isolation.py` is unknown.
- **Ordering.**
  - Ours' 20261009–20261011 sort after main's newest vala migration `20261008000000_drop_audit_staging.sql`, so versions do not collide.
  - `vala._sqlx_migrations` and `wyrd._sqlx_migrations` are separate ledgers, so there is no cross-crate collision either.
  - 20261009 drops `oracle_acquire_table_cut(uuid,uuid,bigint,bigint,jsonb)`; main kept that signature, so the DROP still matches.
  - 20261009 does not drop main's `0e1968570` ActiveRead additions. Whether objects that main added to that function/table set are left orphaned is unknown; the SQL group should diff the post-migrate schema.

### Leftover symbols after the merge (they compile-break or behave wrong)

- **`wyrd-server/src/query/collect.rs`** (main only, merges clean) still uses `terminal_error_to_bifrost` (l.496, 670, 812) and `EncoderOptions::default()` (l.543). Port:
  - ours' `WyrdJsonEncoderFactory`
  - `terminal_error(frame.error.as_deref())`
  - the test fixture `error: Some(Box::new(WyrdError::from(BifrostError::QueryTimeout).problem()))`
- **`QueryTerminalErrorCode`** leftovers (other groups): `wyrd-client/src/bifrost/query.rs:205+`, `vala-bifrost-redux/src/oracle/mod.rs:52,3585`, `oracle/query_stream.rs:1312`.
- **`audit_publication_disabled` / `wait_audit_staged`** leftovers: `wyrd-testing/src/server.rs`, `wyrd-client/tests/pg_bifrost_e2e.rs`.
- **`bifrost.abort().await`** on a now-sync `abort`: `wyrd-testing/src/server.rs:957`, `wyrd-testing/tests/bifrost/server/owner_inspection.rs:560`.
- **Active-read symbols** still referenced: redux `oracle/mod.rs`, redux `tests/integration/forge/snapshot_expiration.rs` (3 places), wyrd-testing `tests/bifrost/oracle/distributed.rs`, and main's `pg_forge_operations` / `pg_forge_tasks` / `reader_expiry_ordering.rs`.
- **`entry_hash`**: the test at wyrd-testing `verification_runtime.rs:39,1291` needs the deleted `audit_staging.rs`.
- **`wyrd_event_time` / `EventTimeWindow`**: in my group every occurrence sits inside a conflict hunk (boot:653, drift:1081, observations:14, results:16/565). The wyrd-testing journeys belong to the testing group.
- **`card_ref`** as a correlation column: only in hunks (capture.rs:1077, results.rs:561), plus a doc comment at `wyrd.v1.proto:252`.

### Cross-group seams that my files depend on

- **Redux `register_dataset`:** must drop the audit parameter and `BifrostCatalogError::AuditUnavailable` (`bifrost_catalog.rs:1672-1686`).
- **`GateAudit`:** ours' `stage_write_decision` is sync. Check main's `ScribeOutbox` impl against ours' trait shape. Unknown which signature survives.
- **`CallsTable`:** Variant `request_payload`/`response_payload` (ours) vs Utf8 `*_json` (main). capture.rs follows whichever wins.
- **`ResultsTable`:** typed Variant columns (ours) vs `details` (main).
- **`EvalObservationsTable::acknowledged`:** `(frame, principal)` (ours) vs `(frame, receipt_micros)` (main).
- **`BifrostStorage`:** ours removed close/abort (`4559cff37`), but main's state.rs shutdown calls them.
- **`OracleMemoryResources.reconciliation_limit_bytes`:** removed by ours, still set by main's boot.

### Design-document conflict

- Ours' `architecture/bifrost-design.md` (HEAD:64): "Every table has exactly one time column…".
- Main's same file (main:46/278/691) still describes a managed `wyrd_event_time`.
- Main's ledger L36 has observation journeys client-stamp `wyrd_event_time`.
- The design authority must be reconciled before the code is.

## Open decisions for the human

- **(a) Expiration-claim refusal.** Should `oracle_read_table_cut` port main's refusal of tables with unresolved snapshot-expiration claims (55000 → `QueryVisibilityUnavailable`)? Under ours' retention window, expiry never changes the current snapshot, so it is probably unnecessary. Needs agreement with the redux group (`forge/expire.rs`).
- **(b) Task-kind Verifier results.** Main's task kind (`284e4bc92`) writes `AssertionResult` into a generic `details` column. Ours' typed result schema has no such column. Options:
  - add a typed `assertion_result` Variant column
  - keep `details`
  - unknown which the spec intends
- **(c) Event time.** Ours declares one time column per table (`created_at` / `ended_at`). Main's verification code and ledger L36 assume a client-stamped `wyrd_event_time`. This also decides whether `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE` survives.
- **(d) WAL fence lookup.** Keep the SECURITY DEFINER with main's operator-flag raise and restore, or drop the definer and query through `OperatorPool`?
