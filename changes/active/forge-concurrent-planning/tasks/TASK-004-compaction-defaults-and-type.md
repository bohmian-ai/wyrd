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

## Verification

Focused exact tests for each scenario, `mise run codegen:check`,
`mise run py:test:unit`, `mise run py:typecheck`, the TypeScript unit and
integration tasks, `mise run test:bifrost:journey:forge`, fmt, lints and
`git diff --check`.
