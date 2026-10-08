---
id: TASK-004
kind: implementation
status: review
spec: SPEC-forge-concurrent-planning
spec_revision: 6
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

### Red-gate remediation evidence

#### Failure 1 — `crates/vala/vala-bifrost-redux/tests/integration/forge/compaction_admission.rs::multi_plan_success_counts_all_committed_volume_once`

- Symptom: the intermittent line 735 (stalled-commit counter equals
  `consumed`) or line 654 ("durable Scribe promotion demand diverged from the
  prepared plan") failure recorded in the blocker above.
- Evidence: the traced runs and independent diagnostician report above; a
  scheduler heartbeat (`bifrost.forge.scheduler.pass`) creates a promotion
  task while `seal_more` / `age_files` is still running. Not reproduced
  locally in 11 traced pre-fix runs plus a 58/58 pre-fix `forge::` suite.
- Cause: `Forge::supervise` delayed only the first heartbeat under a
  test-owned trigger; every later tick ran `lead()` →
  `sweep_promotion_debt()`, an unrequested promotion pass the fixture does
  not own.
- Fix site: `forge/scheduler.rs` — `run_pass`/`lead` take `sweep`; the
  heartbeat arm passes `!quiet`, so under a test-owned trigger the heartbeat
  renews the leader term only and the sweep runs on requested passes.
  Production never sets `quiet` (only `ForgeSchedulerTrigger::with_owner_for_test`
  does), so the production heartbeat still renews and sweeps every tick.
  Callers checked: every `with_owner_for_test` fixture (redux `support.rs`,
  `rewrite_support.rs`, wyrd-testing Forge `support.rs`) drives promotion
  through requested passes or Scribe hints.

#### Failure 2 — Oracle hot-tier journeys

Traced RED (`WYRD_LOG=info,vala_bifrost_redux=debug`):

- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads`
  panicked at `distributed.rs:87` with
  `local pruning journey: "hot-only projection proof requires a hot-only cut: hot=0 compacted=3"`.
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs::published_cache_pruning_and_shutdown_are_production_governed`
  panicked at `published.rs:177`:
  `assertion left == right failed: one immutable identity is decoded exactly once, however many callers ask; left: 2.0 right: 1.0`.

- Symptom: the hot-tier proofs observe Iceberg-pinned files.
- Evidence: `file_list.compacted = true` is written only by
  `FileList::settle_promoted` (`vala-sql` `queries/file_list.rs`), i.e.
  promotion settlement; no `small_files` rewrite ran in either trace. Every
  `scribe_promotion` execution (18/18 in the distributed trace) came from the
  supervisor's hint arm, none from a `bifrost.forge.scheduler.pass`. In the
  published trace query 1 pinned `hot_files=1` at 13.979 s, the promotion
  committed 13.988–14.006 s, and query 2 pinned `hot_files=0
  iceberg_files=2` at 14.016 s; the Oracle cache keys `ObjectPin::Hot` and
  `ObjectPin::Published` differently, so the same object decoded twice.
- Cause: hint-driven promotion (REQ-002) promotes each flushed object within
  ~20–150 ms, which is correct product behaviour; the fixtures assumed the
  object stays hot. The test-owned trigger does not gate the hint arm, and no
  `WyrdTestCluster` option supplies one, so it cannot hold promotion.
- Fix site: the two journey fixtures. Both start through the existing
  `start_spec_with_forge_config_and_completion_observer(.., inject_uncertainty
  = true)` commit seam and park the first promotion commit before writing;
  the parked attempt is the table's active attempt, so every other promotion
  defers behind it. Flushes stay: every distributed proof and published
  phases 2, 3, 5 and 6 need sealed hot Parquet (row groups, footers,
  `HotParquetExec` projection mask, footer tenant proof); the live tail has
  none. `published.rs` releases the parked commit for phase 4 and parks again
  for phase 5. `CommitUncertaintyCatalog::release_paused_before_commit` was
  added to the harness so phase 4 commits instead of refusing; no product
  knob was added and product behaviour is unchanged. The diagnostician
  concurred on cause and fix site.

Findings reported, not fixed (outside this write set):

- Refusing a parked promotion (`reject_paused_before_commit`) left the task
  retrying about every 30 s with "Reset Forge generation cannot be reopened;
  retry requires a new operation and output generation"
  (`vala-sql` `queries/forge_operations.rs`) for about 100 s, until a new
  task id. That looks like a promotion-retry defect after a definite refusal.
- `distributed.rs::prove_hot_and_compacted_pruning` carries the same latent
  `hot > 0` race.
- A promoted object is decoded once per pin kind (Hot, then Published): a
  cache-efficiency note.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Heartbeat under a test-owned trigger renews the term only; production unchanged | `forge/scheduler.rs` (`supervise`, `run_pass`, `lead`) | `multi_plan_success_counts_all_committed_volume_once` 5/5 (35.6–37.9 s); redux integration `forge::` 58/58 | PASS |
| Distributed hot-tier proof reads a hot cut | `oracle/distributed.rs` commit-seam hold | `distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads` 5/5 (6.2–7.6 s) | PASS |
| Published governance journey observes hot, then promoted, then hot | `oracle/published.rs`; `forge_harness.rs` `release_paused_before_commit` | `published::published_cache_pruning_and_shutdown_are_production_governed` 5/5 (19.8–22.1 s) | PASS |
| No regression in the Oracle lane | — | `mise run test:bifrost:journey:oracle` 42/42 | PASS |

Commands: `mise exec -- scripts/postgres/with-test-postgres.sh -- <nextest -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E '<expr>'>`
and the same for `-p wyrd-testing --test oracle` after `mise run db:migrate:inner`,
both with `WYRD_LOG=info,vala_bifrost_redux=debug`; `mise run test:bifrost:journey:oracle`;
`mise exec -- cargo fmt --all --check`; `mise exec -- cargo clippy --locked -p vala-bifrost-redux -p wyrd-testing --all-features --tests -- -D warnings`;
`git diff --check`. All green. No product knob or behaviour change; no test
weakened, skipped or slept.

### Follow-up remediation evidence (red-gate findings)

#### Finding 1 — retry loop after a refused promotion commit (product defect, fixed)

- Symptom: after two definite conflicts reset a promotion operation, the
  task went `retryable`; every retry failed with
  `Forge SQL operation failed: sql operation conflict: Reset Forge generation
  cannot be reopened; retry requires a new operation and output generation`
  (`crates/vala/vala-sql/src/queries/forge_operations.rs:157-160`) until the
  attempt bound, while hint and sweep promotions logged "Forge promotion
  deferred behind the table's active attempt"
  (`forge/scribe_promotion.rs`, `promote_table`).
- Evidence (RED, `WYRD_LOG=info,vala_bifrost_redux=debug`, new test
  `forge::promotion::scribe_promotion_integration_reset_operation_retries_under_fresh_operation`):
  without the backoff release the test panicked at
  `tests/integration/forge/support.rs:2016` ("production Forge worker
  attempt bound: Elapsed") with two "deferred behind the table's active
  attempt" lines; with it, the retry of the same `task_id` under a new
  `attempt_id` failed at `support.rs:2017` with the reopen refusal
  (`failure_class="transient_coordination"`).
- Cause: `ForgeWorker::promotion_operation_id` returned `claim.task_id`, so
  the operation identity was fixed per task. `dispatch_scribe_promotion`
  closes that operation as `Reset` after a surviving definite conflict and
  returns `ForgeError::Catalog` (`TransientObjectStore`), so the task is
  retried in place and `ForgeOperations::append_prepared` refuses the same
  Reset identity on every later attempt. `insert_claimed`'s `NOT EXISTS
  ready/retryable` guard keeps the hint/sweep path from planning around it,
  and the all-state `forge_tasks_idempotency` index would refuse a fresh task
  for the same head and plan. `architecture/bifrost-design.md` ("Scribe hot
  promotion") requires the later retry to run under a new attempt and
  operation, so this is not intended behaviour.
- Diagnostician (fresh, read-only; command, trace and diff only): concurred
  on the cause; named `worker.rs` dispatch/settlement as the fix site; flagged
  the idempotency index as blocking a "terminalize and replan" fix and the
  recovery paths (`settle_promotion_evidence`, `forge.task_id` /
  `forge.operation_id` snapshot properties, `settle_promoted`) that must keep
  one identity per attempt.
- Fix site: `Forge::promotion_operation_id` (`forge/scribe_promotion.rs`)
  resolves the task's current operation generation from
  `vala.forge_operation_state`: generation zero is the task id; each `Reset`
  generation advances to a deterministic successor
  (`promotion_generation_operation_id`, domain-separated SHA-256 of task id
  and generation, UUID v8). The walk is bounded by `ATTEMPT_BOUND`. Both
  callers (`dispatch_scribe_promotion`, `settle_promotion_evidence`) use it,
  so Prepared resume, committed recovery and settlement keep one identity per
  attempt. No schema, plan-shape, failure-class or retry-policy change; the
  in-place retry keeps the production backoff.
- Harness gap (reported, not changed): the redux `integration` test binary
  installs no tracing subscriber unless a scenario installs
  `ForgeTelemetryCheckpoint`; the RED trace used a temporary
  `wyrd_telemetry::init` inside the new test, removed before commit.

#### Finding 2 — latent `hot > 0` race in `prove_hot_and_compacted_pruning` (fixed)

- Cause: same as the sibling hot-only journey; hint-driven promotion can
  settle the second batch before the precondition reads it.
- Fix: the journey starts through
  `start_spec_with_forge_config_and_completion_observer(.., inject_uncertainty
  = true)`, lets the first batch promote with the seam inert, then
  `pause_before_commit()` before writing the hot batch and awaits
  `wait_for_before_commit()`. Later hot-batch promotions logged "deferred
  behind the table's active attempt" (1–2 per run); shutdown drains the
  parked commit unsettled. Pre-fix runs passed 3/3 (latent, not reproduced).

#### Finding 3 — stale `compact_sealed_batch` rustdoc (fixed)

Now states that it waits on promotion settlement (`file_list.compacted`), that
promotion has no partition/age gate, what the one-day clock advance does
affect, the 10-second heartbeat sweep versus requested passes, backoff
release, and that a parked promotion commit must be released first.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| After a refused (reset) commit the next attempt succeeds under a fresh generation, no repeated reopen failures | `forge/scribe_promotion.rs` `promotion_operation_id`, `promotion_generation_operation_id`; `forge/worker.rs` callers | `forge::promotion::scribe_promotion_integration_reset_operation_retries_under_fresh_operation` (phases `reset, committed`; one task `succeeded` at attempt 1; no reopen error); unit `forge::scribe_promotion::tests::promotion_generations_are_deterministic_and_distinct` | PASS |
| Two-tier journey holds its hot batch | `oracle/distributed.rs` commit-seam hold | `distributed::pg_bifrost_selective_predicate_spans_hot_and_compacted_reads` 5/5 (8.6–11.0 s) | PASS |
| `compact_sealed_batch` rustdoc describes current behaviour | `oracle/support.rs` | review | PASS |
| No regression | — | `mise run test:bifrost:journey:oracle` 42/42; redux integration `forge::` 59/59; redux lib `forge::` 91/91; `mise run test:bifrost:journey:forge` 21/21 | PASS |

Commands: `mise exec -- scripts/postgres/with-test-postgres.sh -- <nextest -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E '<expr>'>`
(and `--lib`), the same for `-p wyrd-testing --test oracle` after
`mise run db:migrate:inner`; `mise run test:bifrost:journey:oracle`;
`mise run test:bifrost:journey:forge`; `mise exec -- cargo fmt --all --check`;
`mise exec -- cargo clippy --locked -p vala-bifrost-redux -p wyrd-testing --all-features --tests -- -D warnings`;
`git diff --check`. vala-sql was not modified. No test weakened, skipped or
slept; `clear_task_backoff` is the fixture's existing backoff release.

## Status

Status: IMPLEMENTED for Scenarios 1–3 and the red-gate fixes recorded above.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Integrated `verify:bifrost` | integration branch 51c64fb29 (tree merged unchanged) | `mise run verify:bifrost` exit 0, 9/9 lanes (TASK-002 evidence) | PASS |
