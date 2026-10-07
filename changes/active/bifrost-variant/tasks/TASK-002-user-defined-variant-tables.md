---
id: TASK-002
kind: implementation
status: implemented
spec: SPEC-bifrost-variant
spec_revision: 16
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
- `wyrd-client::Bifrost::write_batch(table, batch)` sends the batch verbatim
  (revision 17); the client neither describes nor conforms it.
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

1. Map revision-16 declaration forms to existing canonical Variant, Struct,
   List, scalar, and nullability shapes; reject unsupported forms on client and
   server.
2. Add the single prepared-input boundary. Validate/normalize the complete
   input, compute the exact charge, reserve once, and enqueue once.
3. Extend the existing nested builder. Strings supplied through row insertion
   remain Variant strings.
4. Project query values through existing language runtimes and regenerate
   contracts/stubs/docs.

## Ordered Implementation Scenarios

### 1. Schema declarations are deterministic

**Behavior.** Every REQ-012 form maps exactly; fixed objects become Struct,
open/mixed shapes Variant, typed arrays List whose items keep their declared
nullability, and unsupported types fail at every SDK declaration boundary that
can represent them before a request and again at the server. Open extras beside fixed fields
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
and integer precision. The complete row set is validated in row, then
declared-field, order;
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

### 3. Withdrawn by revision 17

`write_batch` sends its batch verbatim; the server judges it.

### 4. First-class SDK journeys agree

**Behavior.** Each SDK registers a free-form field, union, and nested model;
writes rows and both Arrow forms; flushes; queries with operators and Struct
access; returns native values (`bigint` for TypeScript 64-bit integers); and
proves Struct uses `get_field`, Variant uses semantic `variant_get`, and a
refusal has no durable row. Arrow terminals retain the extension. Each SDK
also proves the AC-005 refusal matrix at its public boundary with exact
catalog codes: a model allowing extra keys (`SCHEMA_PARSE`) and an undeclared
write with no durable row (`BIFROST_UNDECLARED_FIELD`). Rust and Python also
declare and refuse each REQ-016 type (UInt64, Date64, Time32, second and
millisecond timestamps, a non-UTC zone) through their Arrow door
(`BIFROST_UNSUPPORTED_TYPE`). Per revision 16, TypeScript declares tables only
from JSON Schema/Zod, which cannot express those types; its Variant column
written as Int64 is REQ-014 evidence only. The Rust journey also sends Variant data
unchanged over authenticated gRPC to a dynamic table: a registered Variant
without its extension or with a foreign one, the Variant extension on the
ordinary `point` Struct and on its nested `label`, and malformed, depth-65, and
over-8-MiB bytes; the server refuses each with its catalog code and stores
nothing, while a valid batch on the same path is stored. This proves REQ-013, REQ-014, REQ-015, REQ-016,
REQ-018, REQ-019, INV-003, INV-004, INV-007, AC-004, AC-005, AC-008.

**RED.** Add the cases to one focused journey file per SDK:
`crates/shared/wyrd-client/tests/pg_bifrost_e2e/variant_tables.rs`,
`sdks/wyrd-sdk-python/tests/integration/bifrost/test_variant_tables.py`, and
`sdks/wyrd-sdk-ts/wyrd/tests/integration/variant-tables.test.ts`. Each must
fail on the missing runtime projection rather than setup. Run Verification
commands 4–6, which select every test in those files.

**GREEN.** Add only the thin runtime projections required to make the same
observable matrix pass in all three SDKs, then rerun scenarios 1–3.

**REFACTOR.** Remove language-local durable conversion or validation.

### 5. Withdrawn by revision 17

The canonical-signal journeys keep the OTLP-exporter path; there is no
client-side JSON-text conversion to drive them.

## Acceptance Criteria

- Every declaration in REQ-012 has the exact type/nullability or exact refusal
  on client and server.
- Complete row sets and Arrow batches are normalized before one reservation;
  any failure/cancellation before handoff leaves all queue state unchanged.
- `write_batch` sends its batch verbatim, and the server accepts only the
  Variant extension for a Variant column.
- Rust, Python, and TypeScript journeys round-trip native and Arrow values and
  prove refusals create no durable row.

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
4. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-client --features test-support --test pg_bifrost_e2e -P journey --run-ignored=all -E "test(/^variant_tables::pg_tests::/)"'`
5. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/bifrost/test_variant_tables.py'`
6. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/variant-tables.test.ts'`

9. `mise run py:typecheck`
10. `mise run ts:typecheck`
11. `mise run codegen:check`
12. `git diff --check`

## Material Stop Conditions

- A declaration needs a new durable public type beyond revision 16.
- Pre-admission rejection cannot be achieved without language-local queues or
  durable validation outside Rust.

## Cold rehearsal evidence — 2026-10-05

Inputs: revision-10 queue and Arrow contracts, current `Producer::enqueue_rows`,
`BatchBuilder`, `WriterPool`, `WriterTable`, and three SDK write/query paths.
First slice: make the queue-state RED test fail on the current post-admission
validation, then introduce `PreparedRows`. Owners, consumers, error order, and
schema acquisition are fixed above; only local symbol placement remains.

## Authority Links

- `changes/active/bifrost-variant/spec.md` revision 17
- `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- `AGENTS.md`
- `architecture/{agent-rules,wyrd-design,wyrd-doctrine,bifrost-design}.md`
- `architecture/references/domain/arrow-analytical-interop.md`

## Implementation evidence — 2026-10-06

Includes the TASK-002-r1 remediation (FIND-TASK-002-1 through 12).

| Criterion | Implementation | Verification | Result |
| --- | --- | --- | --- |
| REQ-012 supported declaration forms map exactly; List items keep their declared nullability | `wyrd-queue` `schema.rs` decision table; `map_type` returns item nullability for List | Cmd 1; `schema::schema_tests::array_becomes_list`; model-fields cases in Rust, Python, TS journeys assert non-null `list[str]` items | pass |
| REQ-016 unsupported types refused by client and server | `schema::check_supported`; server register validation | Rust `unsupported_type_is_refused_by_sdk_and_server` and `each_unsupported_type_is_refused_when_declared` (UInt64, Date64, Time32(s), Timestamp(s), Timestamp(ms, UTC), Timestamp(us, America/New_York)); Python `test_unsupported_type_is_refused` (same six, code + status + `for field moment`) | pass |
| Open-extras models refused with the catalogued code | `schema.rs` open-object refusal; TS `tableConfigFromJsonSchema` returns the structured `error`, raised as `WyrdError` | Rust `model_allowing_extra_keys_is_refused`; Python `test_model_allowing_extra_keys_is_refused`; TS `refuses a model allowing extra keys` (code, status 400, remediation) | pass |
| Rows fully prepared before one reservation; failures leave queue state unchanged | `RowPreflight::prepare -> PreparedRows`; producer accepts only `PreparedRows` | Cmd 2; journeys read back zero rows after every refusal | pass |
| Server repeats Variant checks for dynamic tables before stamping | `scribe/execution_lanes.rs` `decode_rows` → `enforce_dynamic_variants` → `tables::validate_declared_variants` | Rust `server_refuses_unstorable_variant_bytes_sent_directly`: invalid encoding, depth 65, 8 MiB + 1 sent raw through `enqueue_batch` each refused with its code, zero rows, valid control stored; RED with the check disabled; `vala-bifrost-redux` lib 843/843 | pass |
| Server accepts only the extension; struct-masked nulls in required children accepted | `scribe/fixed_ipc.rs`, `scribe/material_plan.rs` | `masked_required_struct_child_null_roundtrips` (in redux lib) | pass |
| Rust, Python, TypeScript journeys round-trip and refuse atomically | Focused journey file per SDK | Cmds 4–6: Rust 11/11, Python 14/14, TS 9/9 | pass |
| Contracts, stubs, typing, format, lints | — | Cmds 9–12; `mise run fmt`, `lints`, `py:format`, `py:lints`, `ts:test:unit` 38/38, `py:test:unit` 539 | pass |

| New item | Owners searched | Why new |
| --- | --- | --- |
| `execution_lanes::enforce_dynamic_variants` | `enforce_builtin_source_contract`, `tables::validate_declared_variants` | Dynamic-table counterpart that calls the existing validator; Iceberg-derived fields rename list items and would misrefuse |
| Test helpers `variant_cell`, `wrap_in_array` (Rust journey) | `wyrd_queue::variant`, journey fixtures | Build deliberately invalid storage bytes no production encoder emits |

No dependencies added. Non-goals stayed excluded; no public signature changed
except the TS native binding's return carrier, which the wrapper unwraps.

Open findings:

- DataFusion `get_field` returns a Struct's child without the parent's nulls,
  so `point['x']` on a null `point` reads the placeholder. DataFusion has fixed
  this upstream; TASK-003 picks it up with the upgrade.
- TypeScript declares tables only from JSON Schema, which cannot express the
  REQ-016 types (revision 16).
- Revision 17 removed `write_batch` describe-and-conform: `prepare_batch`,
  `conform_column`, `encode_text_column`, `first_duplicate`, the
  facade unit test, the JSON-text and Struct/List `write_batch` journeys, and
  the JSON-text canonical-signal journeys (restored to OTLP exporters).

Status: IMPLEMENTED.
