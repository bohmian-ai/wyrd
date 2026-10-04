---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-forge-concurrent-planning
requirements: [REQ-011, REQ-012, REQ-013]
depends_on: [TASK-003]
---

# Compaction on by default; tables choose their compaction type

## Scenario 1 — Every table compacts by default (REQ-011)

Flip `ForgeTableSettings::default().compaction_enabled` to `true`
(`crates/vala/vala-bifrost-redux/src/forge/settings.rs`). A table with no
Forge properties gets a compaction track on its first commit and is
dispatched by the 1-hour interval. Update the settings unit test, the
docs property table (`docs/src/content/docs/bifrost/forge.svx`) and every
test that relied on the old default by stating the property explicitly
where it needs compaction off. RED: a redux test that a property-less table
is dispatched after the interval. Journey: a table registered through the
public client with no options is compacted.

### Scenario 1 evidence

| Criterion | Implementation | Verification | Result |
|---|---|---|---|
| Absent `wyrd.forge.enable-compaction` means enabled; explicit `false` still disables | `ForgeTableSettings::default` (`forge/settings.rs`) `compaction_enabled: true`; module/constant/field rustdoc | lib `forge::settings::tests::settings_default_to_risingwave_and_parse_overrides` (pins defaults incl. 1 h interval, explicit `false`) | PASS |
| Property-less table gets a track on first commit and is dispatched at the 1 h boundary, not before | — | RED then GREEN: integration `forge::production_routes::property_less_table_is_compacted_after_the_default_interval` (RED failed at "the first commit opens a compaction track by default") | PASS |
| Public-client table with no options is compacted | — | journey `live_rewrite::property_less_public_table_is_compacted_by_default` (asserts no `wyrd.forge.*` property, no rewrite before the interval, committed rewrite after +1 h, exact public rows) | PASS |
| 512 MiB staging / 1 GiB compaction targets unchanged | not touched | diff audit | PASS |
| Docs state the default | `docs/src/content/docs/bifrost/forge.svx` property row `true` + compaction prose; `architecture/bifrost-design.md` states no compaction default (unchanged) | `mise run docs:check` | see final verification |

Suites: redux lib `forge::` 87/87; redux integration `-E 'test(/^forge::/)'` 57/57;
wyrd-testing `forge` journeys 20/20; wyrd-testing `oracle` journeys 40/42 (two
failures below, outside this task's cause); clippy `--all-features --tests -D
warnings` on `vala-bifrost-redux` and `wyrd-testing` clean.

Tests whose expectations or setup changed:

- `forge::leader::tests::due_rule_boundaries_match_risingwave` and
  `disabled_compaction_keeps_maintenance_membership` (lib): they model a
  *disabled* table, so they now state `compaction_enabled: false` instead of
  inheriting it from `default()`. Assertions unchanged.
- `forge::snapshot_expiration` manual-run step: the table has just been set
  `enable-compaction=false`; the manual request now carries those real
  settings, so its track is still temporary. Assertions unchanged.
- `live_rewrite::compaction_target_registers…` / `public_support::enable_compaction`:
  rustdoc only (compaction is no longer off by default; the helper still sets
  the count trigger so journeys do not wait out the hour).

Diagnoses (traced with `WYRD_LOG=info,vala_bifrost_redux::forge=debug`;
confirmed by an independent read-only diagnostician):

- `production_closeout::empty_maintenance_restart_protects_orphans` — Symptom:
  panic "the settled pass swept the rowless output". Evidence: commit notices
  recorded for `cold_restart_*`; final pass runs expiry + expired cleanup but
  no orphan sweep. Cause: the table now owes compaction after its rejoin
  commit and `run_maintenance` deliberately skips the orphan sweep for tables
  that owe compaction (`gc.rs` `owes_compaction`). Fix site: test setup — the
  scenario is about a cold, maintenance-only table, so it sets
  `enable-compaction=false`. Production unchanged.
- `live_rewrite::forge_promoted_files_rewrite_and_remain_exact_across_recovery`
  — Symptom: "an uncertain rewrite claims no outcome: []". Evidence: three
  compactions dispatched together (both neighbour tables and the owner); the
  one-shot `fail_after_next_commit` landed on the neighbour's rewrite. Cause:
  the neighbour tables now compact by default, contradicting the test's
  premise that the neighbour is never rewritten. Fix site: test setup — both
  neighbour tables set `enable-compaction=false`.
- `oracle distributed::pg_bifrost_selective_predicate_spans_hot_and_compacted_reads`
  — Symptom: selective scan bytes ≥ unfiltered. Evidence: one
  `Forge compaction dispatched table=oracle_two_tier_* pending_commits=20`
  during the measurement. Cause: the measured table now compacts after the
  journey closes its partition (+1 day), changing the cut between baseline and
  selective query. Fix site: test setup — the table opts out. Re-run: PASS, 0
  dispatches.

Blocker (not caused by this scenario; outside this task's write set):
`oracle distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads`
("hot-only projection proof requires a hot-only cut: hot=0 compacted=3") and
`oracle published::published_cache_pruning_and_shutdown_are_production_governed`
("decoded exactly once", 2 vs 1). Trace shows no compaction dispatch; each
flush is promoted to Iceberg by the leader's inline `scribe_promotion` within
~1 s, so these journeys' premise of a still-hot sealed object no longer holds
under the TASK-001 promotion route. Owner: the promotion/Oracle journey owner.

## Scenario 2 — Registration chooses the compaction type (REQ-012)

Follow every file on the `compaction_target_file_size_bytes` path
(`rg -l compaction_target_file_size_bytes`): wyrd-spec request/description
and generated schemas, `wyrd-client` table builder, server
`bifrost/service.rs` validation and re-register conflict with a new
`WyrdError` code, `BifrostCatalog` create-transaction property write, Python
SDK (PyO3, exports, stubs via codegen), TypeScript SDK (native, declarations,
src). Journeys in Rust, Python and TypeScript register a table with
`small-files`, read it back from the description, re-register with the same
value (accepted) and a different value (conflict), and show Forge dispatching
that type.

### Scenario 2 evidence

Wire spelling: `compaction_type` is the `snake_case` enum `CompactionTypeWire`
(`auto | full | small_files | files_with_delete`) in
`crates/wyrd-spec/src/vala/api.rs`. Every `wyrd-spec` wire enum is
`snake_case` (134 `rename_all = "snake_case"`, no kebab-case), so the wire
follows the contract convention; the catalog stores Forge's own hyphenated
property spelling (`ForgeCompactionType::as_str`, e.g. `small-files`) and maps
between the two with `From` impls in `forge/settings.rs`. The hyphenated
spelling is refused on the wire. New stable error:
`WYRD_VALA_409_BIFROST_COMPACTION_TYPE_MISMATCH`
(`BifrostError::CompactionTypeMismatch`). The catalog's positional
`compaction_target_file_size_bytes` parameter became one
`CompactionRegistration { target_file_size_bytes, compaction_type }` owner
(validate, assert-under-lock, write-properties) instead of a second threaded
parameter.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Request and description carry optional `compaction_type`; schemas regenerated | `wyrd-spec/src/vala/api.rs` (`CompactionTypeWire`, `RegisterTableRequest`, `BifrostTableDescription`), `wyrd-spec/schemas/*`, `tests/schemas/*` | `wyrd-spec vala::api::bifrost_wire_tests::*` (12 pass, incl. `bifrost_wire_register_request_carries_snake_case_compaction_type`); `mise run codegen:check` | PASS |
| Catalog writes `wyrd.forge.compaction.type` in the create transaction, reads it back; omitted stores nothing (Forge `full`) | `vala-bifrost-redux/src/catalog/bifrost_catalog.rs` (`CompactionRegistration`, `explicit_compaction_type`, `describe_table`) | `catalog::bifrost_catalog::tests::compaction_registration_writes_the_forge_type_property`; Rust journey asserts stored `small-files` / absent | PASS |
| Re-register omit/repeat accepted, different value 409 with stable code (server pre-check and catalog under-lock check) | `wyrd-server/src/bifrost/service.rs::register_table`, `CompactionRegistration::assert_matches`, `wyrd-spec/src/vala/error.rs` | `wyrd-server bifrost::service::pg_tests::bifrost_tables_register_compaction_type_is_stored_and_fenced` (+ target and idempotent tests, 3/3) | PASS |
| Rust client builder `with_compaction_type` / `compaction_type()`, problem-json mapping | `wyrd-client/src/bifrost/table.rs`, `wyrd-client/src/error.rs` | `wyrd-client error::tests::bifrost_grpc_codes_keep_their_wire_status` | PASS |
| Python SDK (PyO3, exports, stubs) | `sdks/wyrd-sdk-python/src/bifrost/mod.rs`, `python/wyrd/bifrost/__init__.py`, `stubs/bifrost.pyi` → generated `__init__.pyi` | `mise run py:test:unit` (530 pass), `test_table_config_carries_an_optional_compaction_type`, `tests/bifrost/test_public_typing.py`, `mise run py:typecheck` | PASS |
| TypeScript SDK (native, declarations, src) | `sdks/wyrd-sdk-ts/native/src/lib.rs`, `wyrd/src/index.ts` (`CompactionType`, `fromJsonSchema(..., compactionType)`, `compactionType`), generated `index.d.ts`/`index.d.cts`, `error-codes.ts` | `mise run ts:test:unit` (37 pass, 2 new), `mise run ts:typecheck` | PASS |
| Rust journey: register small-files, describe, same accepted, different 409, Forge dispatches that type, undeclared dispatches `full`, rows exact | `wyrd-testing/tests/bifrost/forge/live_rewrite.rs::compaction_type_registers_describes_and_steers_forge_dispatch` | with-test-postgres: `cargo nextest run -p wyrd-testing --test forge -P journey --run-ignored=all -E 'test(=live_rewrite::compaction_type_registers_describes_and_steers_forge_dispatch) \| test(=live_rewrite::compaction_target_registers_describes_and_steers_forge_rewrites)'` (2/2) | PASS |
| Python journey | `tests/integration/bifrost/test_bifrost_e2e.py::test_compaction_type_registers_describes_and_conflicts` | with-test-postgres pytest `-m integration` of that test and the target journey (2/2) | PASS |
| TypeScript journey | `tests/integration/bifrost-write.test.ts` "registers a compaction type…" | `mise run ts:test:integration` (30/30) | PASS |
| Served OpenAPI still valid | utoipa `ToSchema` on `CompactionTypeWire` | `mise run test:principals:integration` | PASS |
| Copy-on-write still compacts `full`; defaults, staging and compaction targets unchanged | `ForgeTableSettings::from_properties` untouched; only `From` impls added to `settings.rs` | `forge::settings::tests::settings_default_to_risingwave_and_parse_overrides` | PASS |

Also run: `mise run fmt`, `mise run py:format`, `mise run py:lints`,
`cargo clippy --locked --all-features --all-targets -- -D warnings` on
`wyrd-spec vala-bifrost-redux wyrd-client wyrd-server wyrd-testing
wyrd-sdk-python wyrd-sdk-ts`, `git diff --check`. Not run: the
workspace-wide `mise run lints` (touched crates linted instead) and the
whole `test:bifrost:journey:forge` lane (focused journeys run). Non-goals
held: no change to default enablement, staging or compaction targets, or
`docs/.../forge.svx` (the target option had no doc line either). No
unexpected failures occurred, so no diagnoses were needed.

## Scenario 3 — Merge staged files once; never revisit finished files (REQ-013)

Default `ForgeTableSettings::compaction_type` becomes `SmallFiles`. Replace
`ForgeConfig::small_file_threshold_bytes` (fixed 64 MiB,
`forge/compact.rs`) with a percentage of the resolved file target
(default 75); `ForgeTablePolicy::extract` (`forge/managed/policy.rs`)
derives the byte threshold from the table's resolved target. The
production-closeout geometry profiles already encode this ratio (768 MiB
of 1 GiB; the fast profile's 4 MiB stage / 8 MiB target scales the same
way), so they use the default instead of an explicit threshold. RED/GREEN
with real files at the scaled geometry: two staged-size files merge into
one target-size output; a target-size output is never selected again by a
later compaction; a lone staged-size file is left until a partner arrives (core group filter
`min_group_file_count = 2` on the SmallFiles config only; the core's
`min_size_per_partition` only sizes parallelism).
Unit tests pin 768 MiB at the 1 GiB default and 75% of a declared target.
Update docs (`forge.svx` property table and compaction prose) and
`architecture/bifrost-design.md` if it states the old default type or
threshold.

### Scenario 3 evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Default type is `small-files`; copy-on-write still forces `full`; declared types still parse | `forge/settings.rs` (`#[default] SmallFiles`, `ForgeTableSettings::default`, rustdoc) | lib `forge::settings::tests::settings_default_to_risingwave_and_parse_overrides` | PASS |
| Fixed 64 MiB threshold replaced by `small_file_threshold_percent` (default 75, `1..=99`, validated) | `forge/compact.rs` (`DEFAULT_SMALL_FILE_THRESHOLD_PERCENT`, `ForgeConfig::validate`) | lib `forge::compact::tests::forge_config_bounds_the_small_file_threshold_percent` | PASS |
| Threshold derived from the resolved target: 768 MiB at 1 GiB, 75% of a declared target; fingerprint hashes the derived bytes | `forge/managed/policy.rs` (`ForgeTablePolicy::extract`, `percent_of`); `fingerprint.rs` unchanged | lib `forge::managed::policy::tests::small_file_threshold_is_three_quarters_of_the_resolved_target` | PASS |
| `min_group_file_count = 2` on SmallFiles only; Full, FilesWithDelete, Auto keep upstream | `ForgeTablePolicy::planning` SmallFiles arm, `SMALL_FILES_MIN_GROUP_FILE_COUNT` | lib `forge::managed::policy::tests::only_small_files_requires_a_partner_file` | PASS |
| Two staged files merge into one target-size output that is never reselected; a lone file waits for a partner (real files, scaled geometry) | — | RED then GREEN: integration `forge::managed_rewrite::small_files_merges_staged_pairs_once_and_lone_files_wait` (RED refused the scaled target: "small-file threshold 67108864 must be below the target file size 181123") | PASS |
| Undeclared table dispatches `small-files` end to end; S2 declared type still steers dispatch | catalog docs (`CompactionRegistration`, `explicit_compaction_type`) | journeys `live_rewrite::compaction_type_registers_describes_and_steers_forge_dispatch`, `production_closeout::compactors_pull_oldest_due_with_capacity`; lib `catalog::bifrost_catalog::tests::compaction_registration_writes_the_forge_type_property` | PASS |
| Closeout geometry uses the default share | `production_closeout.rs` `GeometryProfile` (threshold field removed, `ForgeConfig::default()`) | closeout geometry journey | PASS |
| Docs | `forge.svx` (scheduler prose, `wyrd.forge.compaction.type` row, small-files selection prose); SDK docs (`sdks/wyrd-sdk-python/src/bifrost/mod.rs`, `python/wyrd/bifrost/__init__.py`, `stubs/bifrost.pyi`, `sdks/wyrd-sdk-ts/wyrd/src/index.ts`); `architecture/bifrost-design.md` states no type or threshold (unchanged) | `mise run docs:check`, `mise run codegen:check`, `mise run py:typecheck`, `mise run ts:typecheck` | PASS |

Physical-test diagnosis (GREEN first run) — Symptom: no merged output
(`merged.len() == 0`). Evidence: the merge produced two files, 115843 and
54548 bytes, under a target of 181123 (the staged pair's sum). Cause: the
iceberg rolling writer rolls on written bytes plus the open row group's
uncompressed estimate, so a target equal to the input sum rolls early. Fix
site: test geometry only — target = pair × 5/4, row group = target / 8
(production's 1/8 ratio), threshold 75% of that still sits between one
staged file and the merged pair (`scale_target_to_pair`).

Tests moved to `full` (the fixture seals one object per day, so each
partition holds a lone file the small-files group filter now leaves alone;
in each the compaction type is incidental):

- `PromotionIntegrationFixture` / `enable_compaction` declares
  `wyrd.forge.compaction.type=full`, covering every leader-dispatched fixture
  test:
  - `compaction_admission::*` (9): publication composition, partial
    progress, reconciliation, multi-plan volume, stale input, FIFO bound,
    released authority, acceptance-unknown recovery — admission semantics.
  - `managed_rewrite::compaction_publishes_replacements_without_deleting_inputs` — publication.
  - `orphan_cleanup::rowless_output_uses_canonical_identity_and_full_protection`,
    `bounded_retry_resumes_after_cursor_without_starvation` — orphan cleanup.
  - `production_routes::forge_metrics_describe_real_data_flow` — metrics.
  - `production_routes::coordinator_and_worker_delete_only_exact_never_published_generation` — cleanup.
  - `publication::*` (4) — publication, conflict and ambiguity recovery.
- `rewrite_support::run_attempt` plans `Full`, covering
  `managed_rewrite::managed_rewrite_plan_matches_core_report_on_promoted_snapshot`,
  `…_output_identity_is_unique_across_concurrent_writers`,
  `…_cancellation_drains_and_preserves_possible_outputs`,
  `…_failure_preserves_attempt_global_possible_outputs`,
  `…_produces_exact_handoff_without_catalog_commit`,
  `…_applies_position_and_equality_deletes_to_output_rows`,
  `…_scaled_geometry_has_no_legacy_file_or_group_ceiling`, and the
  shared-root rewrite in `production_routes::worker_selects_current_iceberg_files`
  — rewrite mechanics, deletes, cancellation, failure, output identity.

Tests given same-partition partners instead (they are about small-files
selection): `production_routes::worker_selects_current_iceberg_files`
(2-row partner sealed into days 0 and 1, promoted before selection).

Changed expectations:

- `production_routes::property_less_table_is_compacted_after_the_default_interval`:
  removes the fixture's enable/trigger/type properties and expects a
  `SmallFiles` dispatch (the new default).
- `production_closeout::compactors_pull_oldest_due_with_capacity`: an
  undeclared table now dispatches `SmallFiles`.
- `catalog::bifrost_catalog::tests::compaction_registration_writes_the_forge_type_property`:
  empty properties parse to `SmallFiles`.
- `live_rewrite::compaction_type_registers_describes_and_steers_forge_dispatch`
  (S2): the declared table now declares `full` (conflict probe uses
  `small_files`) so the declared type stays distinct from the default;
  dispatch expectation is declared `full`, undeclared `small-files`.
- `forge::managed::policy` tests: `limits()` is `ForgeConfig::default()`;
  the impossible-geometry case uses target 1 / row group 1.

Suites: redux lib `forge::` + `catalog::` 136/136 (with Postgres); redux
integration `forge::` 58/58 at 3d605c3ef and 57/58 after the S2 merge (the
one failure is the blocker below); wyrd-testing `forge` journeys 21/21
(`compactors_pull_oldest_due_with_capacity` re-run alone after its
expectation fix); clippy `--all-features --tests -D warnings` on
`vala-bifrost-redux` and `wyrd-testing` clean; `mise run codegen:check`,
`docs:check`, `py:typecheck`, `ts:typecheck`; `git diff --check`.

Blocker (not caused by this scenario; fix site outside this task's write
set): `forge::compaction_admission::multi_plan_success_counts_all_committed_volume_once`
fails intermittently (line 735 in the suite, line 654 alone; passed on
3d605c3ef). Diagnosis (traced with `WYRD_LOG=info,vala_bifrost_redux::forge=debug`;
independent read-only diagnostician):

- Symptom: line 735 — the stalled-commit counter equals `consumed`; line 654
  — "durable Scribe promotion demand diverged from the prepared plan", then
  every requested pass is "deferred behind the table's active attempt".
- Evidence: window 3's `promote_more_inputs` lands about 29–30 s after Forge
  start, on the third 10 s leader heartbeat; a promotion task is created by
  the heartbeat, not a requested pass, while `seal_more` is still sealing or
  aging.
- Cause: `Forge::supervise` (`forge/scheduler.rs`) delays only the first
  heartbeat when a test owns the trigger; every later `LEADER_HEARTBEAT`
  tick still runs `lead()` → `sweep_promotion_debt()`. That unrequested sweep
  either promotes a partial seal (the leftover promotion then consumes the
  one-shot commit stall meant for compaction → line 735), or plans before
  `age_files` rewrites `created_at` and revalidates after, so the
  `ORDER BY created_at, file_ordinal, id` order (id is a path hash) changes
  and the plan diverges; the task backs off on a manual clock that never
  advances (→ line 654). The fixture table is pinned to `full`, the old
  default, so this branch does not change the geometry.
- Fix site: `Forge::supervise` / `lead` — under a test-owned trigger the
  heartbeat should renew the term only and leave the promotion sweep to
  requested passes. Affected fixtures: redux `support.rs` (`SupervisedPromotion`),
  `rewrite_support.rs`, `wyrd-testing/tests/bifrost/forge/support.rs`;
  check `snapshot_expiration.rs` and `compaction_admission.rs` for any test
  that relies on the heartbeat sweep. Owner: Forge scheduler.

## Verification

Focused exact tests for each scenario, `mise run codegen:check`,
`mise run py:test:unit`, `mise run py:typecheck`, the TypeScript unit and
integration tasks, `mise run test:bifrost:journey:forge`, fmt, lints and
`git diff --check`.
