---
id: TASK-002
kind: implementation
status: implemented
spec: SPEC-bifrost-variant
spec_revision: 13
requirements: [REQ-003, REQ-004, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-018, REQ-019, INV-002, INV-003, INV-004, INV-006, INV-007, AC-004, AC-005, AC-008]
depends_on: [TASK-001]
parent_task:
remediates: [BVR-FRESH-004, BVR-FRESH-005]
---

## Outcome and Value

Rust, Python, and TypeScript users declare ordinary nested/open models, insert
rows or Arrow batches, query them, and receive native values. Every invalid row
or batch is rejected by the shared prepared-input boundary before queue or
budget mutation; multi-row calls are all-or-none.

## Owners, Scope, Consumers, and Prohibited Changes

- `wyrd-spec` owns `DataTypeSpec::Variant` and exact errors from TASK-001.
- `wyrd-queue` owns JSON Schema mapping, supported-type validation, nested
  builders, Variant conversion, and `RowPreflight -> PreparedRows`.
- `wyrd-client::Bifrost::write_batch(table, batch)` calls the existing
  authoritative `describe(table)` at the start of the async operation and
  normalizes Arrow batches before direct-send admission. No signature change,
  schema cache, overload, or caller-supplied schema is added.
- The server accepts only the Variant extension and repeats schema/value checks
  at the trust boundary. SDKs only project native schemas and values.
- Rust/Python/TypeScript public types, generated contracts/stubs, MCP/HTTP JSON,
  and supported-type documentation are in consumer scope.
- Preserve the existing stable-batch retry, cancellation, acknowledgement,
  sensitivity, and tenant rules.
- Do not add Map, infer JSON from row strings, add TypeScript `fromArrow`, add
  dependencies, duplicate validation per language, or introduce a user-table
  Variant query path outside TASK-001's semantic `variant_get` registration.

## Approach

1. Map revision-10 declaration forms to existing canonical Variant, Struct,
   List, scalar, and nullability shapes; reject unsupported forms on client and
   server.
2. Add the single prepared-input boundary. Validate/normalize the complete
   input, compute the exact charge, reserve once, and enqueue once.
3. Extend the existing nested builder. Strings supplied through row insertion
   remain Variant strings.
4. For `write_batch`, call `describe(table)` before admission and normalize
   only destination-declared Variant fields from extension or Utf8/LargeUtf8
   JSON.
5. Project query values through existing language runtimes and regenerate
   contracts/stubs/docs.

## Ordered Implementation Scenarios

### 1. Schema declarations are deterministic

**Behavior.** Every REQ-012 form maps exactly; fixed objects become Struct,
open/mixed shapes Variant, typed arrays List, and unsupported types fail in the
SDK before a request and again at the server. Open extras beside fixed fields
return `WYRD_VALA_400_SCHEMA_PARSE`. This proves REQ-003, REQ-012, REQ-015,
REQ-016, INV-002, INV-006, and AC-005.

**RED.** Add
`schema::schema_tests::open_nested_and_unsupported_schemas_map_exactly`. Assert every
table row in REQ-012, field/nullability output, and exact client errors. The SDK
journeys in scenario 4 assert the repeated server refusal creates no table. Run:
`mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=schema::schema_tests::open_nested_and_unsupported_schemas_map_exactly)'`.

**GREEN.** Extend the current mapper and server validator only.

**REFACTOR.** Keep one shared decision table; foreign-runtime acquisition stays
at its SDK edge.

### 2. Rows are fully prepared before queue admission

**Behavior.** Nested Struct/List/Variant values preserve types, missing/null,
and integer precision. The complete row set is validated in revision-10 order;
one reservation and handoff follow. Any row failure, cancellation before
handoff, or conversion failure leaves queue length, budget, counters, and
acknowledgements unchanged. This proves REQ-004, REQ-013, REQ-015, REQ-019,
INV-002, INV-007, AC-004, and AC-005.

**RED.** Add
`producer::tests::prepared_rows_reject_atomically_before_reservation`.
Use a multi-row input whose final row has an undeclared field, too-deep value,
oversize value, and out-of-range number in separate cases; assert exact first
error and unchanged queue/budget/counters. Cover cancellation before and after
handoff against existing retry semantics. Run:
`mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=producer::tests::prepared_rows_reject_atomically_before_reservation)'`.

**GREEN.** Move all fallible conversion into `RowPreflight::prepare`; make the
producer accept only `PreparedRows`.

**REFACTOR.** Delete the old post-admission validation path. Add no builder
hierarchy or second queue.

### 3. Arrow normalization uses the destination schema

**Behavior.** `write_batch` performs exactly one authoritative `describe(table)`
before admission, then conforms the batch to the declared columns (match by
name, declared order, omitted nullable columns as nulls, omitted required
column `SCHEMA_PARSE`, undeclared column `UNDECLARED_FIELD`) and converts
extension and Utf8/LargeUtf8 JSON only for declared Variant fields. Describe failure, invalid JSON, and wrong wire types
leave queue, budget, and direct-send state unchanged; server input must be the
extension. Row strings are not JSON-parsed. This proves REQ-014, REQ-019,
INV-002, INV-007, AC-004, and AC-005.

**RED.** Add `bifrost::facade::tests::variant_batch_describes_before_admission`.
Register a table, exercise both accepted inputs plus invalid JSON and a same-
named non-Variant text field, and assert one describe precedes admission,
describe failure changes no state, and the server wire type is Variant. Run:
`mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::facade::tests::variant_batch_describes_before_admission)'`.

**GREEN.** Describe, normalize, then admit and send; repeat validation on the
server.

**REFACTOR.** Reuse the same Variant conversion as row preparation; no cache.

### 4. First-class SDK journeys agree

**Behavior.** Each SDK registers a free-form field, union, and nested model;
writes rows and both Arrow forms; flushes; queries with operators and Struct
access; returns native values (`bigint` for TypeScript 64-bit integers); and
proves Struct uses `get_field`, Variant uses semantic `variant_get`, and a
refusal has no durable row. Arrow terminals retain the extension. This
proves REQ-013, REQ-014, REQ-018, INV-003, INV-004, AC-004, AC-005, AC-008.

**RED.** Add `variant_tables_round_trip_and_refuse_atomically` in each existing
runtime journey owner. Each must fail on the missing runtime projection rather
than setup. Run:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-client --test pg_bifrost_e2e -P journey --run-ignored=all -E "test(=pg_tests::variant_tables_round_trip_and_refuse_atomically)"'`,
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/bifrost/test_bifrost_e2e.py::test_variant_tables_round_trip_and_refuse_atomically'`,
and
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/bifrost-write.test.ts -t "variant tables round trip and refuse atomically"'`.

**GREEN.** Add only the thin runtime projections required to make the same
observable matrix pass in all three SDKs, then rerun scenarios 1–3.

**REFACTOR.** Remove language-local durable conversion or validation.

### 5. Python and TypeScript canonical-signal Arrow journeys write Variant columns

**Behavior.** TASK-001 made the built-in signal `attributes`,
`resource_attributes`, and log `body` columns Variant and temporarily routed the
Python and TypeScript canonical-signal journeys through the stock OTLP
exporters, because neither SDK could author Variant columns. With scenario 3 in
place, both journeys again build canonical signal Arrow batches directly: the
Variant columns are Utf8 JSON text, `write_batch` normalizes them to the Variant
extension, and the journey flushes, reads the rows back through SQL, and asserts
native values plus semantic `variant_get` access into `attributes`,
`resource_attributes`, and log `body`. The Arrow terminal retains the
extension. No per-fixture Variant encoder exists. This restores Python and
TypeScript canonical-Arrow write coverage and proves REQ-014, AC-004, and
AC-005 on the built-in signal tables.

**RED.** Restore the direct canonical-Arrow write in
`sdks/wyrd-sdk-python/tests/integration/test_bifrost_query.py::test_canonical_signal_arrow_write_and_sql_read_round_trip`
and the TypeScript `oracle-query.test.ts` test
"canonical signal Arrow write and SQL read round-trip", passing JSON text for
every Variant column. Each must fail on the missing normalization, not setup.
Run:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_bifrost_query.py::test_canonical_signal_arrow_write_and_sql_read_round_trip'`
and
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/oracle-query.test.ts -t "canonical signal Arrow write and SQL read round-trip"'`.

**GREEN.** No new code beyond scenario 3; the shared `write_batch`
normalization makes both journeys pass.

**REFACTOR.** Delete the OTLP-exporter detour TASK-001 added to these two
journeys.

## Acceptance Criteria

- Every declaration in REQ-012 has the exact type/nullability or exact refusal
  on client and server.
- Complete row sets and Arrow batches are normalized before one reservation;
  any failure/cancellation before handoff leaves all queue state unchanged.
- One authoritative describe before admission—not inference or a cache—controls
  Arrow conversion, and the server receives only the Variant extension.
- Rust, Python, and TypeScript journeys round-trip native and Arrow values and
  prove refusals create no durable row.
- Python and TypeScript canonical-signal journeys write built-in signal
  Variant columns (`attributes`, `resource_attributes`, log `body`) directly as
  Arrow JSON text through `write_batch` and read them back natively through SQL
  and `variant_get`; the TASK-001 OTLP-exporter detour is gone.

## Expected Write Set and Consumer Closure

- `crates/wyrd-spec/src/vala/{api,error}.rs` and generated contracts.
- `crates/shared/wyrd-queue/src/{schema,batch_builder,error}.rs` and producer.
- `crates/shared/wyrd-client/src/bifrost/` plus server register/write validation.
- `sdks/wyrd-sdk-{rust,python,ts}` public types, conversions, tests, stubs/docs.
- MCP/HTTP JSON and supported-type documentation consumers.

## Verification and Evidence

Run only these task-local proofs; do **not** run `mise run verify:bifrost`.
The `db:migrate:*` setup below prepares only the test control-plane database;
it is not a Bifrost data or Iceberg migration.

1. `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=schema::schema_tests::open_nested_and_unsupported_schemas_map_exactly)'`
2. `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=producer::tests::prepared_rows_reject_atomically_before_reservation)'`
3. `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::facade::tests::variant_batch_describes_before_admission)'`
4. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-client --features test-support --test pg_bifrost_e2e -P journey --run-ignored=all -E "test(/^variant_tables::pg_tests::/)"'`
5. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/bifrost/test_variant_tables.py'`
6. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/variant-tables.test.ts'`

Scenario 4's single `variant_tables_round_trip_and_refuse_atomically` journey
per SDK is split into one focused, user-shaped file per SDK
(`pg_bifrost_e2e/variant_tables.rs`, `test_variant_tables.py`,
`variant-tables.test.ts`) with the same eight cases each; commands 4–6 run
those files.
7. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_bifrost_query.py::test_canonical_signal_arrow_write_and_sql_read_round_trip'`
8. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/oracle-query.test.ts -t "canonical signal Arrow write and SQL read round-trip"'`
9. `mise run py:typecheck`
10. `mise run ts:typecheck`
11. `mise run codegen:check`
12. `git diff --check`

## Material Stop Conditions

- A declaration needs a new durable public type beyond revision 10.
- Pre-admission rejection cannot be achieved without language-local queues or
  durable validation outside Rust.
- `write_batch` cannot use the existing authoritative `describe(table)` before
  admission without changing its public signature or adding a cache.

## Cold rehearsal evidence — 2026-10-05

Inputs: revision-10 queue and Arrow contracts, current `Producer::enqueue_rows`,
`BatchBuilder`, `WriterPool`, `WriterTable`, and three SDK write/query paths.
First slice: make the queue-state RED test fail on the current post-admission
validation, then introduce `PreparedRows`. Owners, consumers, error order, and
schema acquisition are fixed above; only local symbol placement remains.

## Authority Links

- `changes/active/bifrost-variant/spec.md` revision 10
- `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- `AGENTS.md`
- `architecture/{agent-rules,wyrd-design,wyrd-doctrine,bifrost-design}.md`
- `architecture/references/domain/arrow-analytical-interop.md`

## Implementation evidence — 2026-10-06

| Criterion | Implementation | Verification | Result |
| --- | --- | --- | --- |
| REQ-012 declarations map exactly; client and server refuse unsupported forms | `wyrd-queue` `schema.rs` shared decision table; server register validation | Cmd 1; Rust `unsupported_type_is_refused_by_sdk_and_server` (SDK + raw server, no table created); Python `test_unsupported_type_is_refused`; TS `refuses open extras beside fixed fields` | pass |
| Rows fully prepared before one reservation; failures leave queue state unchanged | `RowPreflight::prepare -> PreparedRows`; producer accepts only `PreparedRows` | Cmd 2; Rust/Python/TS undeclared-field cases read back zero rows | pass |
| `write_batch` describes once, then conforms by the same rules as `insert` (revision 13) | `RowPreflight::prepare_batch`, sharing the `undeclared`, `missing_required`, and `encode_variant` helpers with `prepare`; facade calls it after `describe` | Cmd 3; `wyrd-queue` batch conformance unit tests (57/57); Arrow JSON-text cases in all three SDKs | pass |
| Server accepts only the extension and repeats checks; struct-masked nulls in required children are accepted | `scribe/fixed_ipc.rs`, `scribe/material_plan.rs` masked-null handling | `masked_required_struct_child_null_roundtrips`; `vala-bifrost-redux` lib 347/347 | pass |
| Rust, Python, TypeScript journeys round-trip native and Arrow values and refuse atomically | Focused files per SDK; TS typed rows project Struct to objects and List to arrays (`nativeValue`) | Cmds 4–6: Rust 8/8 (full `pg_bifrost_e2e` 26/26), Python 8/8, TS 8/8 | pass |
| Canonical-signal Arrow journeys write Variant columns as JSON text; OTLP detour removed | Python `test_bifrost_query.py`, TS `oracle-query.test.ts` build batches from the described schema | Cmds 7–8 (Python file 9/9, TS file 9/9) | pass |
| Contracts, stubs, typing, format, lints | — | Cmds 9–12; `mise run fmt`, `lints`, `py:format`, `py:lints`, `ts:test:unit` (38/38), `py:test:unit` (539) | pass |

Open findings:

- DataFusion 55 `get_field` returns a Struct's child column without the
  parent's nulls, so `point['x']` on a row whose `point` is null reads the
  child's placeholder (`0`) instead of `null`. Whole-Struct reads are correct.
  This predates TASK-002: it is the same Parquet-reader and `get_field`
  limitation `wyrd_queue::variant::mask_placeholders` documents, which can
  only repair Variant children because they have a recognizable empty
  placeholder. A fix means replacing Struct field-access planning in the
  Oracle (losing DataFusion's nested projection pushdown and changing the
  plan contract peers verify), so it is a follow-up for the Oracle owner, not
  part of this task.
- TypeScript `TableConfig.fromJsonSchema` refusals carry the message but no
  stable `code` (pre-existing for every construction-time TS error). Rust and
  Python carry the code.
- Nested Variant fields inside List/Struct columns (e.g. span `events[].attributes`)
  cannot be written from Arrow JSON text; revision 13 conforms top-level
  declared Variant columns only. The OTLP path still covers them.

Status: IMPLEMENTED. Every acceptance criterion above is met; the open
findings are follow-ups outside this task's write set.
