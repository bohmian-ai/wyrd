---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-bifrost-variant
spec_revision: 12
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-017, REQ-019, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, AC-001, AC-002, AC-003, AC-005, AC-008, AC-009]
depends_on: []
parent_task:
remediates: [BVR-FRESH-001, BVR-FRESH-003, BVR-FRESH-005, BVR-RR2-002, BVR-RR3-003]
---

## Outcome and Value

Bifrost has one stable Variant contract, every listed built-in producer writes
its typed Variant/Struct schema, and every Oracle session exposes the same
Arrow-backed semantic `variant_get` SQL surface while Struct remains exact
DataFusion `get_field`. Bloom filters use row-group capacity. Iceberg v3 creation,
lineage-preserving Forge rewrites, and v3 garbage collection land together;
there is no intermediate releasable state that can compact v3 rows incorrectly.

## Owners, Scope, Consumers, and Prohibited Changes

- `wyrd-spec` owns `DataTypeSpec::Variant`, exact catalog errors, generated
  schemas/OpenAPI, and peer contract versioning. It stays IO-free and PyO3-free.
- `vala-bifrost-redux` owns the canonical type/fingerprint, limits, built-in
  schemas/projections, v3 validation, Oracle registration, writer properties,
  and Forge orchestration.
- `wyrd-queue` and `vala-bifrost-redux` receive direct workspace dependencies
  on the existing Arrow 59.3 Variant crates only where revision 12 requires
  them. DataFusion, Parquet, and Iceberg stay out of client-tier crates.
- TASK-001 remains on the current workspace DataFusion source. It owns semantic
  `variant_get`, SQL lowering, session registration, codec round trips, and
  full-root/residual correctness only; TASK-003 owns all DataFusion fork,
  physical-field, facade, and repin work. Here the pinned Iceberg fork owns the
  logical unshredded Variant round-trip and the compaction fork owns v3
  metadata-column rewrites; TASK-003 owns shredded physical-schema support.
- Affected producers and consumers are in scope: `wyrd-client/src/observe/eval.rs`,
  `wyrd-server/src/verification/results.rs`, gateway capture, agent traces,
  audit publication, OTLP/canonical signal projection, Oracle
  `{mod,live,codec,bindings,dispatcher,peer,follower,analytical_scan}` paths,
  Rust/Python/TypeScript result terminals, HTTP/MCP JSON, generated contracts,
  examples, and supported-type documentation.
- Preserve audit hash inputs, sensitivity, tenant tripwires, public logical
  schemas, the five-field Forge handoff, and fixed trace identifiers.
- Do not add shredding policy, user-model inference, a second reader, a second
  Variant model, `datafusion-variant`, signing, migration, or unrelated
  dependencies. Do not rewrite Variant access to exact `get_field`.

## Approach

1. Implement the revision-10 Variant limits, errors, `0x0d` fingerprint tag,
   Arrow/Iceberg round-trip, and exact persisted Struct layouts.
2. Convert every named built-in producer and consumer; regenerate derived
   artifacts.
3. Add one Oracle session-registration owner and call it before planning,
   codec use, provider setup, or execution in every production session. Keep
   Struct on `get_field`; lower literal-key Variant `->`/`->>` to one semantic
   Arrow-backed `variant_get` UDF. Prove full-root and residual correctness here;
   defer physical field declarations to TASK-003.
4. Implement fresh v3 creation, hidden-lineage preservation through repeated
   rewrites, and v3 GC in the same change.
5. Size both writers' Bloom filters from existing row-group geometry.

## Ordered Implementation Scenarios

### 1. Variant and built-in persisted contracts

**Behavior.** Exact limits, error precedence/details, fingerprint tag, Struct
layouts, duplicate-key rule, promotions, sensitivity, and all built-in Variant
replacements match revision 12. Every old-format producer changes in the same
scenario. This proves REQ-003, REQ-004, REQ-006–REQ-011, REQ-019, INV-001,
INV-002, INV-004, INV-005, INV-007, AC-001, AC-003, and AC-005.

**RED.** Add `tables::tests::variant_contract_and_builtin_schemas_are_stable`
and `verification_runtime::typed_builtin_payloads_are_queryable`. The first
asserts tag, limits, error fields, field order/nullability, and logical
fingerprints. The journey writes Drift, Eval, gateway, agent-trace, audit, and
OTLP/canonical values, flushes, queries fields, and asserts equivalent native
values and unchanged audit hashes. Add
`trace_export::pg_tests::span_variant_fields_and_promotions_are_queryable`,
`logs_export::pg_tests::log_variant_body_attributes_and_promotions_are_queryable`,
and
`metrics_export::pg_tests::metric_variant_fields_and_promotions_are_queryable`
to the existing OTLP journey modules, plus
`builtin_variant_and_struct_payloads_are_queryable` to
the Rust, Python, TypeScript, and MCP query journeys. Run:
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=tables::tests::variant_contract_and_builtin_schemas_are_stable)'`
and Verification and Evidence items 2–8 exactly:
the focused `wyrd-testing` server, OTLP list/selectors, the focused
`pg_bifrost_e2e` selector, the Python node id, the TypeScript test title, and
the MCP query selector shown there. Those commands are part of this RED, not
optional broader evidence.

**GREEN.** Change shared contracts and every named producer/consumer until both
tests pass; regenerate artifacts.

**REFACTOR.** Reuse installed Variant crates and existing projection owners.
Remove replaced storage branches; add no compatibility alias.

### 2. One Variant SQL registry in every Oracle session

**Behavior.** Leader, admission, follower, analytical, and worker sessions
register revision-10 operators/functions before plan or codec use. Peer version
mismatch fails before decode. Arrow results retain the extension; typed,
HTTP, and MCP results emit native JSON values. This proves REQ-017, REQ-019,
INV-004, INV-006, AC-001, AC-003, AC-005, and AC-008.

**RED.** Add `published::variant_sql_registry_covers_every_session` to the
existing Oracle journey target.
Build each production session, execute the same operator/function matrix,
round-trip a distributed plan, assert invalid/try JSON behavior, and assert a
sensitive expression is refused before provider IO. Assert Struct access stays
exact `get_field`, Variant literal-key `->`/`->>` stays semantic `variant_get`,
full-root and residual evaluation preserve exact values, and `->>` text
conversion remains outside the UDF result. Run:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=published::variant_sql_registry_covers_every_session)"'`.

**GREEN.** Route every constructor and codec through the one registration
method and existing permission boundary.

**REFACTOR.** Delete duplicate registration; add no registry trait or parallel
planner.

### 3. Fresh v3 storage includes safe Forge rewrites

**Behavior.** Every table is created as v3, non-v3 input is unsupported,
append identity is
assigned, repeated Forge rewrites preserve both hidden lineage values, failed
lineage validation commits nothing, and GC operates on v3. This proves REQ-001,
REQ-002, INV-003, INV-006, and AC-002.

**RED.** Add
`forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite`. It creates
built-in and user tables, writes rows, records both hidden values, compacts
twice, compares every surviving row, injects missing lineage and asserts no
commit, then runs v3 GC. Run:
`scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite)'`.

**GREEN.** Carry metadata only in the fork/internal physical batch and validate
output `DataFile` evidence before the existing commit.

**REFACTOR.** Keep the current handoff and recovery identity; expose no public
row-lineage field or capability switch.

### 4. Bloom sizing uses existing geometry

**Behavior.** Scribe and Forge use maximum row-group rows and Parquet folds to
actual distinct values. This proves REQ-005 and AC-009.

**RED.** Add
`parquet::writer_properties::tests::bloom_capacity_uses_row_group_limit_for_scribe_and_forge`,
assert opening-batch size cannot change the capacity, and run:
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=parquet::writer_properties::tests::bloom_capacity_uses_row_group_limit_for_scribe_and_forge)'`.

**GREEN.** Replace opening-batch constants with the existing geometry value
and rerun the focused test.

**REFACTOR.** Keep one calculation and the current false-positive setting.

## Acceptance Criteria

- Variant limits, errors, fingerprint, persisted Structs, and every built-in
  producer/consumer match revision 12 with no legacy stored form.
- Every Oracle production session and codec supports the same SQL contract and
  enforces sensitivity before IO. Struct remains `get_field`; Variant remains
  semantic `variant_get` with correct full-root and residual evaluation.
- All tables are v3 only after repeated-rewrite lineage and v3 GC pass; hidden
  columns never enter the logical schema or changed handoff.
- Both writers use row-group Bloom capacity and retain folding/FPP behavior.

## Expected Write Set and Consumer Closure

- `crates/wyrd-spec/src/vala/` plus generated schemas/OpenAPI.
- Workspace manifests/lock and the narrow Variant dependency entries for
  `crates/shared/wyrd-queue` and `vala-bifrost-redux`.
- `crates/vala/vala-bifrost-redux/src/{tables,catalog,parquet,scribe,oracle,forge}/`.
- `crates/shared/wyrd-client/src/observe/eval.rs`, Bifrost result decoding, and
  `crates/wyrd/wyrd-server/src/verification/results.rs` plus gateway/audit owners.
- Rust/Python/TypeScript query terminals, MCP/HTTP rendering, docs/examples.
- Workspace manifests/lock plus pinned `iceberg-rust` and compaction forks;
  TASK-001 does not repin DataFusion.
- Existing Bifrost lib, integration, server, Oracle, and Forge test owners.

## Verification and Evidence

Run only these task-local proofs; do **not** run `mise run verify:bifrost`.
The `db:migrate:*` setup below prepares only the test control-plane database;
it is not a Bifrost data or Iceberg migration.

1. `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=tables::tests::variant_contract_and_builtin_schemas_are_stable)'`
2. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::typed_builtin_payloads_are_queryable)"'`
3. `mise exec -- cargo nextest list --locked -p wyrd-testing --test otlp -P journey --run-ignored=all`; record all three exact `pg_tests` names above before running them.
4. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=trace_export::pg_tests::span_variant_fields_and_promotions_are_queryable) | test(=logs_export::pg_tests::log_variant_body_attributes_and_promotions_are_queryable) | test(=metrics_export::pg_tests::metric_variant_fields_and_promotions_are_queryable)"'`
5. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-client --test pg_bifrost_e2e -P journey --run-ignored=all -E "test(=pg_tests::builtin_variant_and_struct_payloads_are_queryable)"'`
6. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_bifrost_query.py::test_builtin_variant_and_struct_payloads_are_queryable'`
7. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/oracle-query.test.ts -t "builtin Variant and Struct payloads are queryable"'`
8. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=query::pg_tests::builtin_variant_and_struct_payloads_are_queryable)"'`
9. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=published::variant_sql_registry_covers_every_session)"'`
10. `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite)'`
11. `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=parquet::writer_properties::tests::bloom_capacity_uses_row_group_limit_for_scribe_and_forge)'`
12. `mise exec -- cargo nextest run --locked --manifest-path /home/thorrester/Documents/GitHub/iceberg-rust/Cargo.toml -p iceberg --lib -E 'test(=arrow::schema::tests::variant_round_trips_unshredded)'`
13. `mise exec -- cargo nextest run --locked --manifest-path /home/thorrester/Documents/GitHub/iceberg-compaction/Cargo.toml -p iceberg-compaction-core --lib -E 'test(=compaction::tests::rewrite_preserves_v3_row_lineage)'`
   Record both tested revisions, then repin.
14. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_bifrost_query.py::test_canonical_signal_arrow_write_and_sql_read_round_trip'`
15. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/oracle-query.test.ts -t "canonical signal Arrow write and SQL read round-trip"'`
   Built-in signal `attributes`, `resource_attributes`, and log `body` are
   Variant here, and Python/TypeScript cannot author Variant Arrow columns until
   TASK-002 scenario 3. Commands 14–15 therefore produce their rows through the
   stock OTLP exporters and still read the Variant columns back natively through
   SQL; canonical-Arrow write equivalence stays proven in Rust. TASK-002
   scenario 5 restores the direct Python/TypeScript Arrow write. Add no
   per-fixture Variant encoder and do not pull `write_batch` JSON-text
   normalization into this task.
16. `mise run codegen:check`
17. `git diff --check`

## Material Stop Conditions

- A locked Variant or lineage value cannot be represented by the approved
  direct dependencies and narrow forks.
- v3 creation would be mergeable or deployable without this task's safe rewrite
  and GC behavior.
- A named producer retains an old stored shape or an Oracle constructor bypasses
  shared registration.

## Cold rehearsal evidence — 2026-10-05

Inputs: revision-10 spec, BVR-FRESH-001/003/005, BVR-RR2-002, and
BVR-RR3-003, current catalog, Forge
handoff, Oracle constructors/codecs, and named producer paths. First slice: add
the pure Variant contract test, then update its owner. Consumer closure is
explicit above. Remaining choices are local symbol placement only; no public,
persisted, security, or sequencing decision remains.

## Authority Links

- `changes/active/bifrost-variant/spec.md` revision 12
- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/operations/{deployment-and-release,reliability-and-recovery}.md`

## Implementation Evidence — 2026-10-05

All commands ran with
`CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd-bifrost-variant/target`
on the final candidate and exited 0.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Variant limits, errors, fingerprint, persisted Structs, and every built-in producer/consumer match revision 12 with no legacy stored form | `wyrd-spec/src/vala/error.rs`, `wyrd-queue/src/variant.rs`, `vala-bifrost-redux/src/tables/`, producers in `wyrd-client/src/observe/eval.rs`, `wyrd-server/src/verification/results.rs`, gateway/audit/OTLP owners; regenerated `wyrd-spec/schemas`, `tests/schemas`, `error-codes.ts` | V1 `tables::tests::variant_contract_and_builtin_schemas_are_stable`; V2 `verification_runtime::typed_builtin_payloads_are_queryable`; V3 lists the three OTLP names; V4 runs all three; V5 Rust, V6 Python, V7 TypeScript, V8 MCP `builtin_variant_and_struct_payloads_are_queryable`; V16 `codegen:check` | PASS |
| Every Oracle production session and codec supports the same SQL contract and enforces sensitivity before IO; Struct stays `get_field`; Variant stays semantic `variant_get` with correct full-root and residual evaluation | `oracle/variant_sql.rs` (one `OracleVariantSql` registration, `mask_placeholders` for Variants under null Structs), `oracle/mod.rs` (`remote_variant_error` keeps catalog identity across distributed hops), `wyrd-client/src/error.rs` (exact `BifrostError` reconstruction) | V9 `published::variant_sql_registry_covers_every_session` (interactive + analytical matrices, invalid JSON code on both paths, workload role refused before follower leases advance); plan shape (`get_field` vs `variant_as_text(variant_get(...))`) and null-parent Struct/Variant rows proven by `oracle::variant_sql::tests::variant_operators_and_functions_follow_the_contract` | PASS |
| All tables are v3 only after repeated-rewrite lineage and v3 GC pass; hidden columns stay out of the logical schema and handoff | Forge managed rewrite + `parquet/promoted_object.rs` Variant group arm; pinned forks `iceberg-rust` e999331f280b698bcd026550812b5047e8789df6, `iceberg-compaction` 2b65fa189f2d05002acc6e59515a071a63777970 (V12 and V13 ran at exactly these pinned revisions; V10 ran on the candidate whose lockfile pins them) | V10 `forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite`; V12 `arrow::schema::tests::variant_round_trips_unshredded`; V13 `compaction::tests::rewrite_preserves_v3_row_lineage` | PASS |
| Both writers use row-group Bloom capacity and retain folding/FPP behavior | `parquet/writer_properties.rs` | V11 `parquet::writer_properties::tests::bloom_capacity_uses_row_group_limit_for_scribe_and_forge` | PASS |
| Canonical signal round-trip in Python and TypeScript | `sdks/wyrd-sdk-python/tests/integration/test_bifrost_query.py`, `sdks/wyrd-sdk-ts/wyrd/tests/integration/oracle-query.test.ts` | V14 `test_canonical_signal_arrow_write_and_sql_read_round_trip`; V15 `canonical signal Arrow write and SQL read round-trip` | PASS |

Deferral (lead decision, option C): the Python and TypeScript canonical-signal
journeys (V14, V15) now produce their rows through the stock OTLP exporters
(traces, logs, metrics) and read the Variant columns back natively through
SQL. Neither SDK can author a Variant Arrow column until TASK-002, so
canonical-Arrow write equivalence stays proven in Rust through the shared
fixture. Direct Python/TypeScript Arrow writes return with TASK-002
scenario 5. No per-fixture Variant encoder was added, and `write_batch`
JSON-text normalization was not pulled forward.

Consumer journeys re-run because stored values are now native Variant: Rust
`sdks/wyrd-sdk-rust/tests/drift_verification.rs`, Python
`test_drift_journey.py` and `state/test_observe_journey.py`, the OTEL export
journeys in `bifrost/test_bifrost_e2e.py`, TypeScript
`drift-verification.test.ts` and `otel-export.test.ts`, and the MCP canonical
query journey: all pass.

Diagnosis — Python `test_drift_method_edges_score_through_oracle` 500:
- **Symptom:** an internal server error on
  `to_json(drift_report['features'])`.
- **Evidence:** the handler panicked at
  `parquet-variant-59.3.0/src/variant.rs:379` ("Received empty bytes") from
  `ToJson::invoke_with_args`.
- **Cause:** a null Struct leaves empty-bytes placeholders in its Variant
  child, and neither the Parquet reader nor DataFusion `get_field` pushes the
  parent null down.
- **Fix site:** every Variant argument now passes through `mask_placeholders`
  in `oracle/variant_sql.rs` before it is decoded. `mask_placeholders` marks
  cells whose `metadata` and `value` are both empty as null; every other
  present cell is fully validated.

Format and lint: `mise run fmt`, `mise run lints`, `mise run py:format`,
`mise run py:lints`, `mise run ts:typecheck`, and V17 `git diff --check` all
pass. Non-goals stayed excluded: no shredding, no DataFusion repin, no
compatibility alias, and no second Variant model.
