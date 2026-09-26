# TASK-002 Wave 1 Task Implementation Review

Immutable subject: base `c8bb490ad814c0c7770cac33ed7779897ff776e4`, candidate
`fbfc2591a985b288935180098f892aecdf3b8b49`, approved
`SPEC-verified-change-contract` revision 32.

## Review Findings

### Critical

None.

### Important

#### TASKREV-001 — VIOLATION — the configured Rust startup API does not match the locked interface

- **Violated obligation:** REQ-123 and TASK-002's acceptance criterion that all
  three public run APIs match `architecture/logic/run_api.md`.
- **Exact location:** `crates/shared/wyrd-client/src/state.rs:440-466`; locked
  interface at
  `changes/active/verified-change-contract/architecture/logic/run_api.md:158-162`.
- **Evidence:** The approved Rust example requires
  `start_bifrost_with_config(&client, Some(table), queue_config)`, mirroring
  `Bifrost::connect_with_config`. The candidate exposes only
  `start_bifrost_with(&client, table)` and hard-codes
  `QueueConfig::default()` at line 462. There is no
  `WyrdState::start_bifrost_with_config` in the candidate. The implementation
  record acknowledges that it replaced the approved method because
  `QueueConfig` was not re-exported, but a task record cannot revise the
  approved public contract.
- **Observable consequence:** The locked Rust example does not compile and a
  caller cannot pass the existing Bifrost producer configuration through the
  state-owned startup surface.
- **Required testable correction:** Restore the approved configured Rust method
  with the locked client/table/queue-config arguments, routing those arguments
  to `Bifrost::connect_with_config`, and add a public-surface compile or SDK test
  using the exact approved call. If the public method is intentionally being
  changed instead, that requires an approved specification/interface revision,
  not an implementation-only substitution.

#### TASKREV-002 — INCORRECT — startup accepts fixed tables with incompatible order, nullability, or extra columns

- **Violated obligation:** REQ-127, TASK-002 Scenario 1, and the fixed-table
  compatibility portion of AC-025 require an incompatible fixed table to fail
  startup; `table_schema.md` fixes exact authored column order, type, and
  nullability.
- **Exact location:** `crates/shared/wyrd-client/src/observe/mod.rs:305-331`,
  invoked from
  `crates/shared/wyrd-client/src/observe/lifecycle.rs:171-180`.
- **Evidence:** `require_projection` looks up expected fields by name and checks
  only `DataType`. It never compares field count, declaration order, or
  `Field::is_nullable()`. Consequently a Drift table with nullable
  `record_id`, reordered columns, or an added authored column passes startup.
  The existing incompatibility test mutates only a datatype, so it does not
  falsify these reachable cases.
- **Observable consequence:** `start_bifrost` reports success against a stale or
  incompatible system-table contract. An added non-nullable column then defers
  failure until batch sealing, while nullability/order drift is silently
  accepted despite the fixed physical contract.
- **Required testable correction:** Compare the complete described user schema
  for each fixed table against its exact expected fields, including count,
  order, datatype, and nullability. Add startup refusals for wrong nullability,
  reordered fields, and an extra field, alongside the existing wrong-type test.

#### TASKREV-003 — INCORRECT — Python mappings silently coerce non-string keys

- **Violated obligation:** REQ-124 and the locked Python boundary in
  `run_api.md` require mapping keys to be strings and require validation before
  strict JSON serialization.
- **Exact location:** `sdks/wyrd-sdk-python/src/observe/mod.rs:37-75`, especially
  the mapping branch at lines 58-60 and `json.dumps` at lines 66-75.
- **Evidence:** A `PyMapping` is passed directly to Python's `json.dumps` with
  `allow_nan=False`, but its keys are never inspected. Python JSON serialization
  accepts integer, float, boolean, and `None` keys by converting them to JSON
  object strings. For example, `{1: "value"}` reaches Rust as
  `{"1": "value"}` rather than being refused.
- **Observable consequence:** Drift feature identity or generic/Eval context can
  change at the SDK boundary instead of rejecting the caller's invalid mapping,
  contrary to the shared cross-language input contract.
- **Required testable correction:** Before `json.dumps`, validate that every
  top-level mapping key is a Python string (including the mapping produced by
  `dataclasses.asdict`) and raise the existing structured validation error on
  the first non-string key. Add Python boundary tests for a non-string mapping
  key and a valid string-key mapping.

#### TASKREV-004 — INCORRECT — TypeScript silently drops or coerces unsupported observation values

- **Violated obligation:** REQ-124 and the locked TypeScript contract require
  unsupported values, non-finite numbers, and unsafe numeric values to fail
  rather than be omitted or converted to `null` by `JSON.stringify`.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1105-1111`,
  `1123-1131`, and `1144-1146`.
- **Evidence:** All three methods call `JSON.stringify` directly. JavaScript
  omits object properties whose values are `undefined`, functions, or symbols,
  converts `NaN` and infinities to `null`, and cannot distinguish an unsafe
  integer that was already rounded as a `number`. The TypeScript unit tests at
  `sdks/wyrd-sdk-ts/wyrd/tests/unit/observe.test.ts:38-117` cover only valid
  serialization and projected native errors; they do not exercise the locked
  refusal cases.
- **Observable consequence:** A payload such as
  `{ score: 1, required: undefined }` is admitted as `{ score: 1 }`; Eval or a
  nullable generic-table row can likewise persist a value different from what
  the caller supplied. This is exactly the silent omission prohibited by the
  approved interface.
- **Required testable correction:** Use one small TypeScript boundary serializer
  shared by `drift`, `eval`, and `record` that recursively rejects unsupported
  values, non-finite numbers, and unsafe integers before calling native code,
  while preserving valid JSON values. Add unit tests proving omissions,
  non-finite values, and unsafe integers fail and the native method is not
  called.

#### TASKREV-005 — MISSING — Python and TypeScript cannot capture their runtime's active OpenTelemetry span

- **Violated obligation:** REQ-129, TASK-002 Scenario 4, and AC-026 require each
  first-class SDK to prefer explicit trace/span IDs and otherwise capture valid
  IDs from the active OpenTelemetry span when that runtime exposes one.
- **Exact location:** shared fallback at
  `crates/shared/wyrd-client/src/observe/eval.rs:123-141` and `198-214`; Python
  call boundary at `sdks/wyrd-sdk-python/src/observe/mod.rs:183-202`;
  TypeScript call boundary at
  `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1114-1132`.
- **Evidence:** The only implicit lookup is
  `tracing::Span::current().context()` inside Rust. Neither foreign boundary
  reads its owning runtime's active context. A Python OpenTelemetry context and
  a Node `@opentelemetry/api` context do not become a Rust `tracing` current span
  merely because a PyO3/N-API call occurs. The candidate contains no Python
  `get_current_span` or TypeScript `trace.getSpan(context.active())` call, and
  all journey calls omit IDs without installing/asserting an active span.
- **Observable consequence:** `observe.eval(...)` inside an active Python or
  Node span writes null `trace_id`/`span_id`, so the committed Eval observation
  cannot join to the trace unless every caller manually passes IDs.
- **Required testable correction:** When neither ID is explicit, have each
  foreign-runtime boundary read and validate its own active OpenTelemetry span
  context and pass those IDs through the existing Rust options path; retain the
  current Rust lookup for native Rust callers. Add Python- and Node-runtime tests
  with real active spans, plus precedence tests proving explicit IDs win.

#### TASKREV-006 — INCORRECT — concurrent first use performs duplicate table describes

- **Violated obligation:** REQ-128 and AC-025 require concurrent first uses of a
  dynamic table to converge and evidence one first-use describe per table.
- **Exact location:**
  `crates/shared/wyrd-client/src/bifrost/facade.rs:268-315`.
- **Evidence:** `writer_table` checks the cache under a mutex, releases it, awaits
  `TableConfig::describe`, and only then inserts. Two concurrent cache misses
  therefore both perform the remote describe; the source documentation at
  lines 279-282 explicitly says they may each describe. The only new cache test,
  `record_describes_a_dataset_table_once`, performs sequential calls and cannot
  detect this race.
- **Observable consequence:** A burst of first observations for one table makes
  duplicate metadata requests and duplicate `bifrost_table:read` authorization
  decisions/audit events instead of the required one cached first-use lookup.
- **Required testable correction:** Coalesce in-flight descriptions per table
  name so concurrent callers await one describe result and receive the same
  cached schema, without adding another producer cache. Add a concurrent test
  whose describe server observes exactly one request and whose writes share one
  producer.

### Suggestions

None. Optional refactors and preferences are excluded from this acceptance
audit.

## Acceptance Matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-075 — retain the two canonical observation records and remove obsolete refs/run IDs | `wyrd-spec/src/vala/drift/record.rs:29-48`; `wyrd-spec/src/vala/eval/record.rs:24-83` | generated-schema/codegen evidence recorded; source denies unknown fields | PASS |
| REQ-076 — use the existing queue, Arrow IPC, Gate, and Scribe path | `observe/mod.rs:166-173,207-212`; `bifrost/facade.rs:331-362`; `wyrd-queue/src/batch_builder.rs` | three real SDK observation journeys and `verify:bifrost` recorded green | PASS |
| REQ-118 — publisher/subject identity split and no raw Verifier/binding identity | `Run::correlation` at `observe/mod.rs:103-109`; fixed rows omit those identities | Rust/Python/TS journeys query exact subject UID and shared invocation ID | PASS |
| REQ-121 — managed identity semantics across all five tables | all five definitions use `CorrelationPolicy::Observation`; observation projections supply subject correlation | catalog schema test and observation journeys; result-row runtime population belongs to dependent tasks | PASS |
| REQ-122 — five daily tables and required Bloom declarations | table definitions under `vala-bifrost-redux/src/tables/{drift,eval,verification}` | `verification_tables_match_their_approved_schemas` asserts layouts and resolved Bloom union | PASS |
| REQ-123 — exact Rust/Python/TypeScript run APIs and state-owned writer | `Run`, Python, and TypeScript surfaces exist, but Rust configured startup differs at `state.rs:454-465` | no compile proof of the locked configured Rust example | FAIL (TASKREV-001) |
| REQ-124 — accepted input families and pre-admission refusals | shared Drift validation is present; Python and TypeScript boundary conversion is incomplete | valid-family tests exist; non-string Python keys and silent TypeScript omission are untested | FAIL (TASKREV-003, TASKREV-004) |
| REQ-125 — canonical record construction and fixed projection before generic insert | `observe/drift.rs`, `observe/eval.rs`, and explicit `insert_into` calls | server-free row tests plus all three journeys | PASS |
| REQ-126 — buffered enqueue, no per-call flush/verdict wait, shutdown drain | synchronous fixed emits and `WriterPool`-wide shutdown | lifecycle tests and real journey shutdowns recorded green | PASS |
| REQ-127 — startup describes/cache fixed tables and rejects incompatible schemas | lifecycle preflight exists, but compatibility checks only field presence/type | missing/wrong-type tests only | FAIL (TASKREV-002) |
| REQ-128 — dynamic explicit-table routing, cached first use, reserved refusal, one writer | explicit immutable `WriterTable` routing and namespace refusal exist; cache miss is not single-flight | sequential cache and two-table journeys exist; no concurrent describe proof | FAIL (TASKREV-006) |
| REQ-129 — Eval options, canonical record, explicit-first then active-span trace identity | canonical record/options and explicit IDs work; only Rust `tracing` context is consulted | explicit-ID Rust test and span-without-trace refusals; no Python/Node active-span proof | FAIL (TASKREV-005) |
| REQ-132 — generic lowercase-hex `FixedSizeBinary(16)/(8)` conversion | `wyrd-queue/src/batch_builder.rs:297-347` | focused round-trip and malformed/wrong-width tests recorded green | PASS |
| REQ-133 — single start, retryable ambiguous shutdown, permanently closed success | `observe/lifecycle.rs:34-225`; `WriterPool` retains ambiguous producers | lifecycle tests cover repeat start, failed shutdown retry, and closure | PASS |
| REQ-145 — reuse table-read/record-write permissions and reserved-table refusal | describe uses the existing client route; Gate remains the admission authority; `DATASETS_PREFIX` rejects system tables | real-server journeys cover valid admission and reserved refusal | PASS |
| INV-007 — tenant isolation and transactional auth audit remain intact | no alternate server write path; existing authenticated describe/Gate path reused | `verify:bifrost` and boundary evidence recorded green | PASS |
| INV-010 — Bifrost remains authoritative for observations/results | all observation writes stay on Bifrost; no Postgres observation store added | source inspection and journeys | PASS |
| INV-012 — reuse existing record/engine semantics | canonical record structs are modified only for approved identity removal/media extension; no replacement engine | codegen and existing family lanes recorded green | PASS |
| AC-017 (TASK-002 observation-authoring slice) — all three real SDK surfaces, scoping, correlation, queue path, shutdown | Rust/Python/TypeScript journey files exercise the real server and Postgres | journeys recorded green, but locked boundary-negative coverage is incomplete | FAIL (TASKREV-003, TASKREV-004, TASKREV-005) |
| AC-020 (TASK-002 supporting-test slice) — integration/unit coverage and required lanes | substantial unit/integration coverage and full recorded lane set | recorded lanes are green, but they omit the failing schema/concurrency/runtime-boundary cases above | FAIL (TASKREV-002 through TASKREV-006) |
| AC-024 (TASK-002 schema-definition slice) — exact columns/types/nullability/layout/Blooms | exact five contracts at `tables/mod.rs:898-1068` and table-owned definitions | exact catalog schema/layout/Bloom test recorded green | PASS |
| AC-025 — startup compatibility, dynamic describe/cache convergence, two tables, reserved refusal, drain | most lifecycle and two-table behavior exists; concurrent cache misses duplicate describe | journeys and sequential tests are green; no single-flight proof | FAIL (TASKREV-002, TASKREV-006) |
| AC-026 — Eval construction, options, trace/span validation, fixed binary round trip | fixed row and binary support exist; foreign-runtime active span capture does not | explicit trace/span and malformed binary tests exist; no Python/Node active-span journey | FAIL (TASKREV-005) |
| Task acceptance — exact registered schemas/order/nullability/partitions/Blooms | canonical Vala definitions match `table_schema.md` | exact catalog test | PASS |
| Task acceptance — all public run APIs match `run_api.md` | Python/TS and ordinary Rust shapes match; configured Rust method does not | no exact-example compile test | FAIL (TASKREV-001) |
| Task acceptance — graceful shutdown is the durability barrier; admission is not called an ACK | lifecycle and public docs consistently distinguish queue admission from drain/ACK | lifecycle and journey evidence | PASS |
| Task acceptance — shared generic queue/facade do not dispatch by Verifier kind | kind-specific projection ends before `Bifrost::insert_into`; queue/facade consume schema + row + correlation | source inspection and queue tests | PASS |
| Prohibited changes — no exposed `WriterPool`, second queue/transport/schema system, active-table mutation, caller schema, or per-observation flush | `WriterPool` remains private; one state-owned `Bifrost`; immutable explicit destinations | source inspection and boundary checks | PASS |
| Non-goals — no synchronous verdict/scoring, new durable observation type, new run registry, or retention policy | authoring methods only project/enqueue; canonical records reused; retention unchanged | source inspection | PASS |
| Unrelated diff drift | audit/Forge/test-harness fixes are tied in the task record to failures encountered in mandatory full verification, which AGENTS.md requires fixing; prior TASK-001 review records are workflow artifacts, not product behavior | complete base-to-candidate diff and commit history inspected | PASS |

## Open Questions

None. Each failed obligation has a correction boundary fixed by the approved
specification; no new product or architecture decision is needed.

## Verification Notes

- Reviewed the complete committed diff
  `c8bb490ad814c0c7770cac33ed7779897ff776e4..fbfc2591a985b288935180098f892aecdf3b8b49`,
  surrounding implementations, tests, manifests, and recorded task evidence.
- The task records green results for `verify:bifrost`, `test:shared`,
  `test:wyrd-sdk`, the exact queue test, Rust/Python/TypeScript journey and unit
  lanes, type checks, codegen/boundary checks, format, lints, and diff check.
- No test/build command was rerun in this read-only Wave 1 audit. Green aggregate
  evidence does not close the source-demonstrated gaps because the relevant
  wrong-nullability/order/extra-field, non-string-key, TypeScript omission,
  foreign-runtime active-span, concurrent-describe, and exact Rust API cases are
  absent from the cited tests.
- Candidate commit remained `fbfc2591a985b288935180098f892aecdf3b8b49`
  throughout source inspection before this report was written.

## Overall Result

**FAIL** — the implementation does not satisfy TASK-002 exactly. Six bounded
implementation corrections are required; none requires revising the approved
behavior.
