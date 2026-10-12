# Merge research: vala-bifrost-redux

OURS = HEAD `cfe21dc90` (wyrd/bifrost-variant/TASK-003), THEIRS = `origin/main` `8a8813a74`, base `5ab92b003`.
Conflict hunks were taken from `git merge-tree` result tree `574f4069dcb10f1f3dc50478d0419486ff77fb2c` (extracted under `merge-research/redux-work/`).
Paths are relative to `crates/vala/vala-bifrost-redux/` unless shown otherwise. No builds or tests were run, so every "proof" below is a lane to run after the merge.

## Summary

**31 conflicting files**, by resolution:
- take-ours: 3
- take-theirs: 5, including 1 modify/delete
- combine: 23

By risk:
- HIGH: 10
- MEDIUM: 12
- LOW: 9

**Top 5 hazards**
1. **Expiry model collision.** OURS replaced active-read registration with a Forge snapshot-retention window (migration `20261009000000_bifrost_snapshot_retention.sql` drops `oracle_acquire_table_cut` and `oracle_active_table_reads`). THEIRS 0e1968570 builds lease-bounded expiry plus an `UnresolvedExpiry` refusal on top of `acquire_active_cut` and active reads. Several files depend on this choice: `catalog/bifrost_catalog.rs`, `forge/expire.rs`, `oracle/mod.rs`, `catalog/error.rs`, the `forge/snapshot_expiration.rs` test, and `vala-sql/src/queries/oracle_reader_authority.rs`. A human has to pick the model (decision D1).
2. **card_ref to card_uid in the decode path** (`scribe/execution_lanes.rs` with 20 hunks, `tables/signal.rs`, three projections, `gate/*`). The change is mechanical but wide. It interacts with OURS' `CorrelationPolicy`, `ConformPlan`, Variant columns and promoted OTLP columns, and it leaves stale `CardRef`/`CARD_REF`/`card_ref` uses outside the conflict markers that will fail to compile.
3. **Oracle drain and governed memory** (`resources.rs` with 16 hunks, `oracle/analytical.rs`, `oracle/admission.rs`, `analytical_supervisor.rs`).
   - THEIRS c63e2ff46 deleted the drain loop; a late child keeps the view alive.
   - OURS made memory lock-free (2f9e2889f) and deleted the exchange registry (63b6df251).
   - The correct result is OURS' lock-free core plus THEIRS' no-drain semantics, `released: Notify` and infallible headroom. It must not reintroduce THEIRS' `close_exchanges` or OURS' `LiveConsumers` drain.
4. **Audit projection and table.** THEIRS (outbox) removed `seq`, `entry_hash`, `prev_hash` and `AuditUnavailable`. OURS removed `wyrd_event_time` and made `created_at` the time column with Variant `detail`. The combined `project_audit_event(event, decided_at)` must write `created_at = decided_at`, and the layout must not sort on `seq`.
5. **Eval observation hook signature.** THEIRS `acknowledged(auth, frame, receipt_micros)` reads `wyrd_event_time` or the receipt time. OURS has no managed event time, so the key time must come from `created_at`. `wyrd-server/src/verification/observations.rs` (another group) consumes `key.event_time` and `receipt_micros`, so it silently breaks.

## card_ref to card_uid: how main's attribution interacts with our work

- Git auto-merged the `card_scope` threading correctly:
  - `IngressDecodeInputs.card_scope`, `DecodeContext.card_scope` and `NativeAdmittedRows.card_scope`
  - `ScribeIngressFrame.attributed_cards`
  - `gate/mod.rs` `dispatch_canonical`, which now carries both `attributed_cards` and OURS' `LogicalIdentity`
  - `dispatch_native_frame`
  - `with_card_registry`

  `gate/attribution.rs` is clean (main only). It resolves `attributed_cards` from the registry for unbound principals (`MAX_ATTRIBUTED_CARDS=32`, `span/log/metric_card_uids`).
- **LogicalIdentity and otlp_batch_id.** `card_uid` is a managed correlation column.
  - It stays excluded from `logical_data_identity` and `LogicalIdentity::for_rows`.
  - It stays included in `correlation_data_identity` (via `is_correlation_column`).
  - `otlp_batch_id(tenant, principal, table, &LogicalIdentity)` therefore still binds attribution.
  - The only rename needed in the identity code is the `CARD_REF` to `CARD_UID` const.
  - One behavior note: registry-resolved `attributed_cards` can differ between retries if the registry changes in between. The retry then yields a different batch id, which is consistent with "attribution is part of identity". Treat this as acceptable rather than as a conflict.
- **Conform plans.** OURS' `ConformPlan`/`ConformPlans` replaced `enforce_canonical_source_contract`, so THEIRS' edits to that function are dropped. `card_uid` must be listed as server-owned (`is_server_owned(name, policy)`) and accepted as a caller-reserved column, the same way `run_id` is (`is_reserved_column(name) && ![RUN_ID, CARD_UID].contains(&name)`).
- **Time columns.** THEIRS' window and `wyrd_event_time` code in the H6/H7 stamp path is dropped entirely. Only `let card_uid = authorized_card_uids(rows, context.card_scope)?` is kept. `authorized_card_uids` returns `CardScopeDenied`, and `resolve_card_uids`/`CardUnresolved` are deleted as THEIRS deleted them.
- **CodeAxis.** THEIRS 1090580f2 deleted `agent_traces`, which leaves OURS' `CorrelationPolicy::CodeAxis` with no user (`tables/managed_columns.rs:118-123`, `scribe/execution_lanes.rs:1410` and `2519-2549`). See decision D2.

## Per-file table

| file | main intent (commits) | ours intent (commits) | resolution | risk | proof |
|---|---|---|---|---|---|
| src/scribe/execution_lanes.rs (20 hunks) | card_ref→card_uid via `authorized_card_uids`/`card_scope`; delete `resolve_card_uids`, `CARD_REF`; event-time window tests | ConformPlans, CorrelationPolicy, LogicalIdentity, no managed event time | combine: imports theirs minus `WYRD_EVENT_TIME`; H3/H4/H5/H8 ours (no CARD_REF); H6/H7 ours stamp + theirs `card_uid` line; H9 theirs (delete); tests ours + `card_scope: principal.card_ref_scope().map(CardRefScope::uids).as_deref()`; keep main's `optional_and_scoped_card_correlations_stamp_signed_uids`, `stamp_card_uid`, `uid_text` adapted (no window, `conform: None`); drop H19's event-time tests. Fix outside-marker leftovers: merged :2395 CardRef import, :2533 agent_traces CodeAxis test, :3327-4828 CardRef helpers, :4951-4978 `columns_are_put_in_declared_order_by_name` uses CARD_REF | HIGH | nextest `stamp_card_uid`, `optional_and_scoped_card_correlations_stamp_signed_uids`, `remembered_conform_plan_matches_a_fresh_plan_and_is_reused`; `mise run test:bifrost` |
| src/gate/mod.rs (5) | outbox `impl<T: AuditStage> GateAudit`, no SYSTEM writer, no AuditUnavailable; card_uid docs | `LogicalIdentity` + `correlation_data_identity`; `acknowledged(auth, frame)` | combine: H1 `use crate::scribe::preprocess::{LogicalIdentity, correlation_data_identity}` (drop `require_card_scope`, `logical_data_identity`); H2/H4/H5 theirs; H3 ours signature (no `receipt_micros`) + main card_uid doc. Stale :54 `CardRefScope` import | HIGH | `otlp_batch_id_tests`; test:bifrost journey |
| src/gate/error.rs (3) | drop AuditUnavailable; fieldless `CardScopeDenied`; drop CardUnresolved mapping | `ContractViolation(BifrostError)`; `EventTimeOutOfRange` removed | combine: ours ContractViolation, theirs for the rest | MEDIUM | lints; test:bifrost |
| src/tables/eval/observations.rs | read stored `card_uid` (no principal, `InvalidFrame` on parse); key `event_time` from `wyrd_event_time`/receipt | time from `created_at` column | combine: main card_uid read + ours `created_at`; keep key field name `event_time` to minimize server change; no `receipt_micros` | HIGH | nextest `acknowledged_keys_*` |
| src/resources.rs (16) | `released: Notify`, infallible headroom (`oracle_infallible_headroom_bytes`, `record_oracle_memory`), no drain (51dbfc1fd, 71258cc8c, 6aaccebf9, c63e2ff46) | lock-free governed memory (2f9e2889f), `LiveConsumers` (ed9f3bf9a), `retain_root`/`release_root` | combine: ours lock-free core + main Notify/headroom; drop `LiveConsumers`, `nested_idle`, `nested_memory_consumers`, release poison check; H16 main late-child test | HIGH | test:bifrost oracle tier; resources unit tests |
| src/oracle/analytical.rs (6) | no-drain `reclaim()`, `close_exchanges` (c63e2ff46) | exchange registry deleted (63b6df251), drain tests | combine: main `reclaim()` without `AnalyticalGraphRelease`; do NOT add `close_exchanges`; keep `take_metric_fold` drop; main tests over ours drain tests | HIGH | test:bifrost oracle tier |
| src/oracle/admission.rs (1) | drop drain | `retain_root`/`release_root`, `drain_children` | combine: ours retain/release, drop `drain_children` | HIGH | test:bifrost |
| src/oracle/mod.rs (12) | `QueryParam`/`bind_values`/`with_param_values`; AuditStage; `QueryTerminalErrorCode`; `acquire_active_cut` | `plan_logical(session, context, sql)` split; `QueryCatalogError` envelope (929ce7771); `read_cut` | combine: ours split + `params: &[QueryParam]` → `.and_then(\|f\| f.with_param_values(bind_values(params)))`; main imports minus `ActiveReadOwner` (if D1=ours); tests `plan_logical(..., &[])`; take `terminal_error_label` only if referenced; drop QueryAuditUnavailable docs | HIGH | test:bifrost query journeys; lints |
| src/forge/expire.rs (4) | lease-bounded `tokio::time::timeout(lease_ttl)` on complete+settle, `PreparedExpiry`, keep claim on unknown outcome (0e1968570) | non-Option `exclusive()`, retention roots | combine: ours structure + port main lease bound and unknown-outcome claim keeping; depends on D1 | HIGH | snapshot_expiration + reader_expiry_ordering integration tests |
| src/catalog/bifrost_catalog.rs (9) | no audit param; `acquire_active_cut`+`UnresolvedExpiry`; returns `TableUid`; eager builtin test | `read_cut`; `register_dataset` → `RegisterTableResponse` (2afc97da2) | combine: drop AuditEvent import + audit param; H2 per D1; return type per D4; H8/H9 main `every_builtin_is_created_through_ensure_builtin`; ours registration-cache test optional (its premise is gone) | HIGH | nextest `every_builtin_is_created_through_ensure_builtin`; test:bifrost |
| src/tables/audit/projection.rs | `project_audit_event(event, decided_at)`, no seq/hash | Variant `detail`, `created_at` time column | combine: main fn shape, `created_at = decided_at`, detail via `VariantColumnBuilder` | HIGH | nextest `projects_canonical_content_with_the_decision_time` |
| src/tables/audit/audit_log.rs | drop `seq`/`entry_hash`/`prev_hash` | `credential_id`, Variant detail, `created_at` layout | combine: main columns + ours detail/credential_id; `daily_layout(CREATED_AT, [sort_desc(CREATED_AT)], bloom)`; no `sort_asc("seq")` | MEDIUM | `every_builtin_partitions_and_sorts_on_its_time_column` |
| src/tables/signal.rs | `CARD_UID_ATTRIBUTE`, `card_uid_text`; `wyrd_tonic::prost` | Variant imports/structure | combine: ours structure; CardRef/CardRefScope→CardUid, card_ref→card_uid, prost→`wyrd_tonic::prost`; main consts; update correlation_fixture | MEDIUM | test:bifrost OTLP journeys |
| src/tables/logs/projection.rs | card_uid fields, `card_scope: Option<&[CardUid]>` | promoted/Variant columns, `Cow` reasons + row ordinal | combine: ours + card_uid rename | MEDIUM | test:bifrost |
| src/tables/traces/projection.rs | as logs + `service_name` param (H2/H3) | promoted `service_name` already | combine: ours + rename; do not duplicate service_name; fix merged :445 `self.card_ref.push` | MEDIUM | test:bifrost |
| src/tables/metrics/projection.rs | as logs | as logs | combine; fix merged :786 `Option<&CardRefScope>` | MEDIUM | test:bifrost |
| src/catalog/wire.rs (4) | correlation_fields: stored `card_uid`/`run_id`, else synthesized nullable Utf8 | 2-class DescribedFields, `wyrd_types::field_to_spec`, no `managed_candidates` | combine: ours DescribedFields + main correlation_fields; imports `{CARD_UID, RUN_ID}`; drop INPUT_CLASS consts, TimeUnit | MEDIUM | `mise run codegen:check`; describe journeys |
| src/tables/mod.rs | eager provisioning; agent_traces removed | time-column tests | combine; drop agent_traces from ours tests (:1379, :3206); drop CodeAxis test if D2 | MEDIUM | `every_builtin_partitions_and_sorts_on_its_time_column` |
| tests/integration/forge/compaction_admission.rs | `catalog_request_timeout` | `CountingObjectStore::new(&promoted.fixture.storage)` | combine | MEDIUM | forge compaction_admission tests |
| tests/integration/forge/promotion.rs | deleted deadline test | storage move | take-theirs (deletion) + ours storage elsewhere | MEDIUM | forge promotion tests |
| tests/integration/forge/publication.rs | `assert_unanswered_call_ends_at_the_request_timeout` | object_store storage | combine; also merged :563 `fixture.staging` (silent) | MEDIUM | forge publication tests |
| src/storage/mod.rs | `#[allow(..., reason=...)]` | object_store test (opendal removed) | take-ours + main's reason form if an allow stays; merged :382 still `opendal::Error` | LOW | test:bifrost storage |
| src/tables/managed_columns.rs | drop `CARD_REF`, docs | time-column docs, CodeAxis | combine docs; CodeAxis per D2 | LOW | lints |
| src/tables/verification/results.rs | task Verifier `details` doc | time-column docs | combine docs | LOW | lints |
| src/oracle/follower.rs | `wyrd_tonic::prost::Message` | other imports | combine imports | LOW | lints |
| src/oracle/analytical_transport.rs | `wyrd_tonic::prost` | other imports | take-ours + `wyrd_tonic::prost::Message` | LOW | lints |
| src/oracle/query_stream.rs | removed audit branch | deleted `terminal_error_code` (envelope) | take-ours | LOW | test:bifrost query |
| src/scribe/tests/scribe_persistence_path.rs | `attributed_cards` field | time-column shift test | take-ours + `attributed_cards: None` (merged :609 and H1/H2 frames) | LOW | redux lib tests |
| src/scribe/ingress.rs (1) | eager provisioning (no lazy `ensure_builtin`) | lazy `ensure_builtin` on frame | take-theirs; merged :994 needs `attributed_cards: None` | LOW | test:bifrost ingest journeys |
| src/oracle/analytical_supervisor.rs | drop idle refusal | `graph_children_idle`/`debt` | take-theirs | MEDIUM | test:bifrost oracle tier |
| src/tables/dev/agent_traces.rs | deleted (1090580f2) | modified | take-theirs (delete) | LOW | lints (no dangling refs) |

## Silent conflicts (clean-merged, will break)

These are in files outside the conflict list, or outside the markers in conflicting files.
- **`fixture.staging` removed by OURS but used by THEIRS-added code.** `tests/integration/forge/production_routes.rs:1079`, `:2527` and `tests/integration/forge/publication.rs:563`.
- **THEIRS' expiry test depends on things OURS removed.** `tests/integration/forge/snapshot_expiration.rs:25` and `:1451-1520` use `acquire_active_cut`, `ActiveReadOwner`, `oracle_active_table_reads` and `UnresolvedExpiry`. OURS' migration drops that table and function. Resolve per D1.
- `src/catalog/error.rs:49` has `UnresolvedExpiry` (THEIRS). Keep it only if D1 retains the refusal.
- `crates/vala/vala-sql/src/queries/oracle_reader_authority.rs:436` uses `ActiveReadOwner` (server-sql group).
- **Removed direct `prost` dependency.** `src/oracle/live.rs:962` uses `prost::Message`, and THEIRS removed the direct `prost` dep from redux `Cargo.toml`. Switch to `wyrd_tonic::prost`.
- **`wyrd_app` grants.** OURS' migrations `20261009000000` (lines 142, 172) and `20261011000000` (line 29) GRANT to `wyrd_app`. THEIRS rewrote its migrations to `operator_access` and `wyrd.operator_session()` and has zero `wyrd_app` references. The role may not exist, so this must be rewritten to the operator pattern (server-sql group).
- **Server consumer of the observation hook.** `crates/wyrd/wyrd-server/src/verification/observations.rs` (THEIRS) passes `receipt_micros` and reads `key.event_time`. Adapt it to OURS' `acknowledged(auth, frame)`.
- `crates/shared/wyrd-testing/tests/bifrost/server/verification_runtime.rs` references `AgentTracesTable`/agent_traces (testing group).
- **CodeAxis is dead** after the agent_traces deletion: `tables/managed_columns.rs:118-123`, `scribe/execution_lanes.rs:1410`, `:2519-2549`.
- **Merged `src/forge/compact.rs` uses both timeouts.** It has `iceberg_total_retry_timeout` (OURS) and `catalog_request_timeout` (THEIRS); confirm both still exist on the config struct. Status: unknown until compiled.
- **`vala-sql` `audit_staging.rs`.** THEIRS deleted it and OURS modified it (server-sql group): take the deletion.
- **Design doc drift.** OURS `architecture/bifrost-design.md:84` still says `wyrd.card_ref`, while THEIRS line 70 says `wyrd.card_uid`. THEIRS' design doc still describes `wyrd_event_time` (:46, :278, :691) and active reads (:602, :880). OURS describes `snapshot_retention_secs` (:1186). The design authority has to be reconciled together with D1 and D3.

## Open decisions for the human

- **D1. Snapshot expiry vs live readers.**
  - Choice: OURS' retention window (`snapshot_retention`, `retained_supersessions`, `read_cut`/`read_cut_cached` + LISTEN/NOTIFY cut cache), or THEIRS' active-read registration with an `UnresolvedExpiry` refusal on `acquire_active_cut`.
  - Recommendation: keep OURS' retention, port THEIRS' lease-bounded expiry (timeout around complete plus settle, keep the claim on an unknown outcome), and drop active reads.
  - Sub-question: should `read_cut` refuse a table with an unresolved expiry claim (main's safety property), or does the retention window cover it?
- **D2. Delete `CorrelationPolicy::CodeAxis`** now that agent_traces is gone. Recommendation: yes, since nothing uses it.
- **D3. Eval observation key time.** The choice is OURS' stored `created_at` or THEIRS' `wyrd_event_time` falling back to receipt time. The managed column is gone on OURS, so the recommendation is `created_at`. Confirm the Verifier semantics: event time vs receipt time.
- **D4. `register_dataset` return type.** The choice is OURS' `RegisterTableResponse`/`RegisterOutcome` (2afc97da2) or THEIRS' bare `TableUid`. The answer determines the server caller and wire contract.

## Proof lanes after resolution

- `mise run lints` (all-features)
- `mise run test:bifrost`
- `mise run test:vala`
- `mise run codegen:check` (wire.rs)
- The named nextest tests in the table, each run with exact `-E 'test(=...)'` expressions
- The forge integration tests through their repository-managed Postgres wrapper
