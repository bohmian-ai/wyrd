# TASK-002 R4 Task Implementation Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Original base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Cumulative candidate: `b56560e511918efbdd84d8756b13100fc381eda0`
- Reviewed range: `c8bb490ad814c0c7770cac33ed7779897ff776e4..b56560e511918efbdd84d8756b13100fc381eda0`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior reviews and remediation: `review/TASK-002-r1/`, `review/TASK-002-r2/`, and `review/TASK-002-r3/`
- Locked logic authorities: `changes/active/verified-change-contract/architecture/logic/run_api.md` and `table_schema.md`

The complete cumulative range and all prior remediation were reviewed. Current
user authority explicitly permits the range's AI co-author trailers, so they
are not a violation or proposed finding in this review. The candidate remained
exact during source inspection, the focused TypeScript run, and the final
identity check.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-002 outcome: one state-owned Bifrost writer, one invocation, immutable Card scopes, and Drift/Eval/generic enqueue | `crates/shared/wyrd-client/src/state.rs` and `src/observe/{mod,lifecycle,drift,eval}.rs`; every emit reuses the state-held `Bifrost` and its existing `WriterPool` | Shared lifecycle/projection tests and all three real SDK journeys recorded in TASK-002 | PASS |
| REQ-075 / INV-012: reuse the canonical Drift/Eval records and remove authored Verifier/run identity | `wyrd-spec` retains the sole canonical records; `observe/drift.rs` and `observe/eval.rs` project them without `drift_ref`, `eval_ref`, or authored `run_id` | Shared projection/schema tests and `codegen:check` recorded in TASK-002 | PASS |
| REQ-076 / INV-010: reuse the bounded queue, Gate, Scribe, and Bifrost with no second ingest path | `Observe::{drift_value,eval_value,record_value}` route through `Bifrost::insert_into`; `WriterPool` remains the only producer pool | `verify:bifrost` 9/9 and the Rust/Python/TypeScript journeys recorded on the code-bearing candidate | PASS |
| REQ-118 / REQ-121: authenticated writer and observed subject remain distinct | `Run::correlation` supplies the scoped `CardRef` and invocation `RunId`; publisher and managed subject columns remain server-owned | Each journey reads rows by exact subject UID and the shared invocation ID | PASS |
| REQ-122 / AC-024 task slice: five exact schemas, daily layouts, Bloom declarations, and sensitivity | `crates/vala/vala-bifrost-redux/src/tables/{drift,eval,verification}` and `tables/mod.rs` match `table_schema.md` | Catalog/schema tiers recorded in the task; no R3 remediation altered these contracts | PASS |
| REQ-123: locked Rust, Python, and TypeScript run APIs, including configured Rust startup | `WyrdState::start_bifrost_with_config` delegates to `Bifrost::connect_with_config`; Python and TypeScript remain thin projections of shared Rust | Rust journey compiles the configured call; language type, unit, and integration lanes are recorded | PASS |
| Scenario 1 / REQ-127: startup describes both fixed tables and refuses unavailable or incompatible declarations | `StartClaim::complete` describes Drift and Eval; `require_projection` compares the full ordered name/type/nullability sequence | Focused incompatible-schema tests and each SDK journey's per-table real-server startup refusal | PASS |
| Scenario 1 / REQ-133: one start, retry after ambiguous drain, and terminal successful shutdown | `BifrostLifecycle::{claim,shutdown}` and fenced `StartClaim::{complete,drop}` retain the same writer after drain failure and close permanently after success | Five focused lifecycle/cache tests and state-level same-batch retry proof recorded in TASK-002 | PASS |
| Scenario 2: one UUIDv7 invocation with immutable root/Model/Agent views and local invalid-alias refusal | `Run::{new,for_card,correlation}` reuses the hydrated graph without mutable active scope or network IO | Shared unit tests and all three SDK journeys | PASS |
| Scenario 3 / REQ-124 / REQ-125: native inputs converge on canonical tall Drift rows and invalid values fail before admission | Shared Rust owns `FeatureName`/`FeatureValue` validation and projection; Python validates mapping keys and supports mapping/dataclass/Pydantic; TypeScript owns one strict boundary serializer | Rust/Python boundary tests and journeys cover their input families; the current TypeScript suite passes, but its double-read gap is `TASKREV-R4-001` | **FAIL — `TASKREV-R4-001`** |
| R3 `FIND-TASK-002-4` exact remediation cases: reject symbol/non-enumerable object keys and symbol/non-index array keys | `strictJson` checks `Reflect.ownKeys` for the specified object and array cases before traversal | Root and nested cases exercise Drift, Eval, and record and assert zero native calls; `mise run ts:test:unit` passed 20/20 | PASS |
| Scenario 4 / REQ-129 / AC-026: Eval session/media, explicit or active trace identity, fixed-width persistence, and negative refusal in every SDK | The canonical `EvalRecordObservation` projection remains shared; the Rust, Python, and TypeScript journeys supply session/media, read exact trace/span bytes, and leave only accepted rows | All three journeys cover invalid trace pair and malformed media; TypeScript uses a real active OpenTelemetry span through N-API | PASS |
| Scenario 5 / REQ-132: generic fixed-size-binary lowercase-hex decode with exact widths | `wyrd-queue/src/batch_builder.rs` handles `FixedSizeBinary` generically and rejects uppercase, malformed, prefixed, or wrong-width text | The three exact queue regressions are recorded passing | PASS |
| Scenario 6 / REQ-128 / AC-025: explicit dynamic routing, cached/converged describe, stale fingerprint fence, and unknown/denied/reserved refusal | `Observe::record_value` enforces `vala.datasets`; `Bifrost::writer_table` uses the owner miss gate/cache; `assert_stale_writer_is_fenced` crosses the real server fence | Every SDK journey observes cached reuse; shared owner concurrency, denied describe, and real stale-writer tests are recorded passing | PASS |
| Scenario 7 / AC-017 and AC-020 task slice: each first-class SDK crosses client, queue/IPC, Gate, Scribe, shutdown, and query readback | Shared Rust owns lifecycle/projection; Python/TypeScript contain only runtime-earned conversion and tracing boundaries | Three gated journeys, `verify:bifrost`, shared/SDK tests, and language integration/type lanes are recorded | PASS |
| REQ-126: observation calls do not wait for a verdict or flush per record; shutdown is the durability barrier | Drift/Eval synchronously project and enqueue; generic record awaits only a cache-miss describe; shutdown drains all producers | Source inspection and lifecycle/SDK journeys | PASS |
| REQ-145 / INV-007 task slice: existing permission, Card-scope, tenancy, and transactional-audit owners remain authoritative | No new authorization model or admission path; describes and inserts retain the existing authenticated server paths | Real denied-describe proof asserts stable denial, one audit row, no cached table, and zero producers | PASS |
| R3 `FIND-TASK-002-11`: changed Rust dependencies use module-top imports and bare declaration types | `Range`, `OnceLock`, `AtomicU32`, `SocketAddr`, `Duration`, and `service_account_by_card_ref` now use the existing top import groups and bare declaration names | `fmt`, `lints`, and `check:clippy-allow-audit` recorded passing | PASS |
| R3 `FIND-TASK-002-16`: closed TypeScript Eval option shapes do not use mergeable interfaces | `EvalMediaRef` and `EvalOptions` are exported readonly object type aliases with unchanged fields and documentation | `ts:typecheck`, unit, and integration lanes recorded passing | PASS |
| R3 `FIND-TASK-002-17`: committed evidence names the implemented stable error | The R2 evidence row, `invalidObservationInput`, and the unit assertion all name `WYRD_SPEC_400_VALIDATION` | Source inspection and `git diff --check` | PASS |
| Non-goals: no second serializer, dependency, queue, cache, producer pool, transport, schema authority, lifecycle state, authorization model, run registry, durable observation type, retention policy, verdict wait, or atomic multi-row API | Existing owners and types are reused; R3 extends `strictJson` rather than introducing another layer | Complete cumulative diff inspection | PASS |
| Verification-forced adjacent fixes preserve their owners and do not broaden product behavior | SQL clock fixes remain in `vala-sql`; harness and Oracle changes remain test/support concerns | Recorded focused SQL/Oracle tests and full Bifrost lanes | PASS |

## Prior-finding closure

| Stable finding | R4 status | Closure evidence |
|---|---|---|
| `FIND-TASK-002-1` | CLOSED | Configured Rust startup and the public `QueueConfig` projection remain present and exercised. |
| `FIND-TASK-002-2` | CLOSED | Fixed-schema comparison remains exact for count, order, type, and nullability. |
| `FIND-TASK-002-3` | CLOSED | Python rejects coercible non-string mapping/dataclass keys before serialization. |
| `FIND-TASK-002-4` | **REOPENED** | The exact R3 symbol/non-enumerable/non-index cases are fixed, but the same serializer still validates accessor-backed properties and then rereads them during `JSON.stringify`, allowing a different or omitted value to reach native. |
| `FIND-TASK-002-5` | CLOSED | Python and Node capture their own active spans with explicit identity precedence. |
| `FIND-TASK-002-6` | CLOSED | The Bifrost owner serializes cache misses, rechecks, and describes once. |
| `FIND-TASK-002-7` | CLOSED | Successful shutdown is terminal across never-started and in-flight-start states. |
| `FIND-TASK-002-8` | CLOSED | Ambiguous drain retry retains the same writer and batch. |
| `FIND-TASK-002-9` | CLOSED | PostgreSQL lock timeout propagates as an honest transaction error. |
| `FIND-TASK-002-10` | CLOSED | The cumulative changed-item rustdoc remediation remains present. |
| `FIND-TASK-002-11` | CLOSED | The R3 import and bare-declaration corrections are present at every cited site. |
| `FIND-TASK-002-12` | CLOSED | Real unknown and denied describes fail before admission with canonical audit proof. |
| `FIND-TASK-002-13` | CLOSED | Every SDK Eval journey persists session/media and exact trace/span identity and proves both required refusals add no row. |
| `FIND-TASK-002-14` | CLOSED | SDK journeys cover both fixed-table failures and cached reuse; the shared real-server path proves stale-fingerprint fencing. |
| `FIND-TASK-002-16` | CLOSED | Both new Eval data shapes are readonly object type aliases. |
| `FIND-TASK-002-17` | CLOSED | The task evidence now names the error the helper and test actually use. |

## Proposed findings

### `TASKREV-R4-001` — TypeScript validation and serialization can observe different property values

- **Classification:** INCORRECT
- **Violated obligation:** REQ-124, `run_api.md`, and the still-applicable intent of `FIND-TASK-002-4` require TypeScript observation data to reach Rust unchanged and require values that JavaScript would silently omit or coerce to fail before native admission.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1100-1151`, especially the first property reads in `Object.entries(node)` / `node[index]` at lines 1128-1145 followed by the second read of the original value in `JSON.stringify(value)` at line 1151; callers are `mediaJson` and `Observe::{drift,eval,record}` at lines 1184-1268. The existing refused-input test at `sdks/wyrd-sdk-ts/wyrd/tests/unit/observe.test.ts:134-196` has no accessor-backed case.
- **Evidence:** A public built-SDK probe passed `Observe.eval` a plain object whose enumerable `answer` getter returned `"yes"` on its first read and `undefined` on its second. `strictJson` read the getter twice, reported no error, and invoked native with `"{}"`; the observed result was `{"reads":2,"emits":[["{}",null,null,null,null]]}`. This uses an ordinary plain object and the production `Observe.eval` caller, not a private or dormant surface.
- **Observable consequence:** A getter-backed Drift feature, Eval context value, generic row field, or array element can change between validation and serialization, so the SDK can report successful admission after silently dropping or replacing data that passed its own validation.
- **Required testable correction:** Keep `strictJson` as the sole TypeScript boundary serializer, but make it serialize the exact validated snapshot so each accepted object property and array element is read once; do not validate one read and stringify the caller object again. Preserve the current accepted JSON values and all symbol/non-enumerable/non-index, prototype, cycle, scalar, and safe-number refusals. Add no dependency or second validation layer.
- **Focused closure proof:** Extend the existing `observe.test.ts` test with root/nested getter-backed object fields and accessor-backed array elements whose later reads differ, exercise the applicable Drift/Eval/record callers, and assert each getter is read once and native receives exactly the validated first value (or the existing structured refusal when that first value is unsupported). Run the exact Vitest target, `mise run ts:test:unit`, `mise run ts:typecheck`, and the existing TypeScript integration lane.

## Verification notes and limits

The task records passing full capability closure on the code-bearing R3
candidate: `verify:bifrost` 9/9, all three SDK journeys, the focused queue and
observe suites, TypeScript unit/type/integration lanes, formatting, lints,
Clippy-allow audit, and `git diff --check`. The later commits contain the R3
corrections and evidence only.

This reviewer additionally ran `mise run ts:test:unit` at `b56560e5`; all 20
tests passed. The passing suite does not exercise accessor-backed values. The
public built-SDK probe above directly falsified the exact-value boundary. Broad
Postgres journeys and aggregate lanes were not rerun in this Wave 1 review;
their recorded commands and results were inspected against the current source.

## Overall result

**FAIL**

The cumulative candidate closes the four exact R3 remediation items and all
other prior findings, but the shared TypeScript boundary can still silently
alter validated caller data through a reachable double-read path. This is one
bounded correction in the existing serializer and does not require a
specification revision.
