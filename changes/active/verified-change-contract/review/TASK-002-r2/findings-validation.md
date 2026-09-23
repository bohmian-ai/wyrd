# TASK-002 R2 Wave 2 Findings Validation

## Immutable subject

- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Cumulative candidate: `a000c201ae86f584fd5b80349f375e087902fd78`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Remediation task: `changes/active/verified-change-contract/review/TASK-002-r1/TASK-002-R1-close-scoped-observation-gaps.md`
- Locked logic authorities: `changes/active/verified-change-contract/architecture/logic/run_api.md` and `table_schema.md`

The complete base-to-candidate diff, all six R2 Wave 1 reports, all three named
R1 artifacts, the applicable authorities, and the current caller/test bodies
were inspected. `HEAD` resolved to the candidate before validation. The
candidate needs bounded remediation; no retained finding changes an approved
product, public API, architecture, security, concurrency-ownership,
resource-ownership, or persistent-data decision, so
`SPEC_REVISION_REQUIRED` does not apply.

## Wave 1 proposal validation

| Wave 1 proposal | Disposition | Independent validation and smallest sufficient correction |
|---|---|---|
| `SR2-001` | **CONFIRMED** | The hard rule is explicit: `AGENTS.md` §16 and `architecture/agent-rules.md` require rustdoc on every new Rust item, including private/test items, fields, associated constants, and methods. The four new `DomainTable` implementations still have undocumented associated constants and methods (`tables/drift/result_features.rs:25-49`, `tables/eval/observations.rs:25-46`, `tables/eval/result_items.rs:24-54`, `tables/verification/results.rs:28-60`), and the local `Features` type/field at `observe/tests.rs:752-755` is a directly demonstrated additional miss. This is incomplete closure of `FIND-TASK-002-10`, not a new finding. Add only the missing intent-bearing docs and required `# Errors`/`# Panics` sections; add no lint, suppression, or documentation abstraction. |
| `SR2-002` | **CONFIRMED** | `strictJson` traverses plain objects with `Object.entries` at `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1120-1127`; own symbol keys are absent from both that traversal and `JSON.stringify`. An independent candidate probe passed an enumerable `[Symbol("secret")]` property to `Observe.eval` and recorded the native call as `{"visible":true}`. The existing test covers a symbol value under a string key, not a symbol key. This remains `FIND-TASK-002-4`. Reject own symbol keys in the existing traversal and add root/nested cases to the existing table-driven unit test; no serializer or dependency is needed. |
| `DC-R2-001` | **CONFIRMED** | Duplicate of `SR2-002`; retained under `FIND-TASK-002-4`. The requested Drift/Eval/record coverage can stay in the one existing strict-serializer table because all three public calls reuse `strictJson`. |
| `DC-R2-002` | **REVISED** | The missing real-journey evidence is valid and task-scoped, but it is not a remaining active-span implementation defect and it is not limited to Rust/TypeScript. `FIND-TASK-002-5` is closed: Python and Node now read their own active contexts and precedence is covered at their owning runtime boundaries. Separately, TASK-002 Scenario 4 and AC-026 require each Rust, Python, and TypeScript Eval journey to cover optional session/media, valid explicit or active trace identity through persisted fixed-width columns, and invalid pair/media refusal before admission. Rust uses default options and reads no trace columns (`observe_run.rs:257-271,379-385`); TypeScript emits default Eval and reads only context/Card/run (`observe-run.test.ts:184-186,239-250`); Python proves active/explicit persisted IDs (`test_observe_journey.py:269-282,332-339`) but its journey still omits session/media and the required invalid pair/media refusals. Retained as genuinely new `FIND-TASK-002-13`. Extend the three existing journeys; add no harness. |
| `DC-R2-003` | **REVISED** | AC-025 is assigned directly to TASK-002 and expressly says the Rust, Python, and TypeScript SDK journeys must fail startup when either fixed table cannot be described and must emit without per-observation schema IO; repository testing rules make schema conflict a journey-level edge flow. The proposal overstates what must be repeated per language: owner-level concurrent miss convergence and producer uniqueness are already closed by the shared public-owner test and need not be triplicated through thin wrappers. The existing journeys do write two tables and prove unknown/reserved refusal, but none drives either fixed-table startup refusal, observes cached reuse/no repeat describe, or drives the stale-schema fingerprint refusal. This is a new acceptance-evidence gap, not a reopening of the corrected schema/cache implementations in `FIND-TASK-002-2` or `FIND-TASK-002-6`. Retained as `FIND-TASK-002-14`, narrowed to the missing real-boundary obligations. |

The task, data/durability, security/tenancy, and concurrency/lifecycle reports
proposed no additional findings. Their empty proposal sets are validated for
their assigned domains, but the task review's overall empty ledger is rejected
by the four retained findings above.

## Prior-finding reassessment

| Stable finding | R2 status | Independent source evidence |
|---|---|---|
| `FIND-TASK-002-1` | **CLOSED** | `WyrdState::start_bifrost_with_config` has the locked `(client, table, QueueConfig)` signature and delegates to `Bifrost::connect_with_config` under the existing lifecycle claim (`state.rs:463-487`); the Rust SDK journey compiles that call at `observe_run.rs:344-349`. |
| `FIND-TASK-002-2` | **CLOSED** | The shared fixed-table check compares the complete ordered field count/name/type/nullability sequence (`observe/mod.rs:305-345`), and the focused test covers reordered, extra, retyped, and renullabled schemas. AC-025's missing real-journey proof is distinct and retained as `FIND-TASK-002-14`. |
| `FIND-TASK-002-3` | **CLOSED** | Mapping/dataclass reduction calls `require_string_keys` before `json.dumps` (`python/src/observe/mod.rs:40-79`), while the direct Pydantic JSON path remains unchanged; focused Python cases cover coercible non-string keys. |
| `FIND-TASK-002-4` | **OPEN** | The common serializer closes the originally enumerated value/coercion cases, but still silently discards own symbol-keyed data because its only object traversal is `Object.entries` (`index.ts:1094-1132`). The omission was independently reproduced against the built candidate. |
| `FIND-TASK-002-5` | **CLOSED** | Python `active_span_ids` and TypeScript `activeSpanIds` read their owning runtime context only when both explicit IDs are absent; the Python real journey persists active and explicit IDs, and the Node runtime test proves active/explicit/absent precedence. Missing full AC-026 journey coverage is a separate proof obligation retained as `FIND-TASK-002-13`. |
| `FIND-TASK-002-6` | **CLOSED** | `Bifrost::writer_table` performs cache check, one owner-local async miss gate, cache recheck, one describe, and insertion into the existing map (`bifrost/facade.rs:316-330`); the barrier-controlled shared test proves one describe/producer per FQN. No per-key subsystem is warranted. |
| `FIND-TASK-002-7` | **CLOSED** | Shutdown moves `NotStarted`/`Starting` to `Closed`; completion and claim drop mutate only `Starting`; a failed actual drain retains `Started` (`observe/lifecycle.rs:121-218`). The terminal and controlled-race tests exercise those transitions. |
| `FIND-TASK-002-8` | **CLOSED** | `ambiguous_shutdown_retries_the_same_batch_on_the_same_state` drives the production state owner, retains the same batch identity across retry, then proves terminal write/restart refusal (`observe/tests.rs:528-597`). |
| `FIND-TASK-002-9` | **CLOSED** | `freeze_publication_range` now propagates the timed-out `FOR UPDATE` through `SqlError` rather than translating `55P03` into `Ok(None)` (`audit_staging.rs:205-286`); the Postgres test holds the lock, observes explicit failure, and retries the unchanged range. |
| `FIND-TASK-002-10` | **OPEN** | Module/table-struct documentation was added, but the required every-item audit was not completed. The undocumented `DomainTable` associated items and local `Features.score` are reachable source violations even though Clippy is green. |
| `FIND-TASK-002-11` | **CLOSED** | The remediated OpenTelemetry/table helper imports are module-scoped, and the changed production signatures use imported bare formatting/error types. No originally identified placement violation remains. |
| `FIND-TASK-002-12` | **CLOSED** | All three SDK journeys now reach the real unknown-table refusal. The server-backed denied-describe journey asserts the stable RBAC code, one canonical denied audit row, no cached destination, and zero producers (`pg_bifrost_e2e.rs:2580-2615`). Broader AC-030 duplication remains out of scope. |

## Deduplicated retained finding ledger

### `FIND-TASK-002-4`

- **Wave 1 source IDs:** R1 `TASKREV-004` / `DC-2`; R2 `SR2-002`, `DC-R2-001`
- **Status:** `CONFIRMED` / `OPEN`
- **Classification:** `INCORRECT`
- **Violated obligation:** REQ-124 and `run_api.md` require unsupported TypeScript input and silent JavaScript omission/coercion to fail before native queue admission.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1094-1132`; coverage gap at `sdks/wyrd-sdk-ts/wyrd/tests/unit/observe.test.ts:177-220`.
- **Reachability and evidence:** `Observe::{drift,eval,record}` all call `strictJson`; media uses the same helper. For a plain object with an enumerable own symbol key, `Object.entries` visits only the string key and `JSON.stringify` emits only that key. The independent probe reached the fake native Eval call with `{"visible":true}` and no error.
- **Observable consequence:** a caller receives success after part of its supplied observation evidence was discarded before Rust validation or admission.
- **Decision-complete correction:** Before iterating string entries in the existing plain-object branch, reject any own symbol key through `invalidObservationInput`. Keep one serializer and add no dependency or parallel validation layer.
- **Focused closure proof:** Add root and nested own-symbol-key cases to the existing table-driven Drift/Eval/record unit test, assert `WYRD_SPEC_400_VALIDATION`, and assert zero native calls; run `mise run ts:test:unit` and `mise run ts:typecheck`.

### `FIND-TASK-002-10`

- **Wave 1 source IDs:** R1 `RS-005`; R2 `SR2-001`
- **Status:** `CONFIRMED` / `OPEN`
- **Classification:** `VIOLATION`
- **Violated obligation:** `AGENTS.md` §16 and `architecture/agent-rules.md` make intent-bearing rustdoc on every new/materially modified Rust item, including private/test items, fields, associated constants, and methods, a hard pre-merge requirement.
- **Exact location:** At minimum `crates/vala/vala-bifrost-redux/src/tables/drift/result_features.rs:25-49`, `tables/eval/observations.rs:25-46`, `tables/eval/result_items.rs:24-54`, `tables/verification/results.rs:28-60`, and `crates/shared/wyrd-client/src/observe/tests.rs:752-755`.
- **Reachability and evidence:** The remediation documented modules and table structs, but each cited trait implementation still introduces undocumented associated constants/methods and the cited test-local struct/field remains undocumented. The rule applies regardless of public visibility or whether rustdoc/Clippy emits a warning.
- **Observable consequence:** The candidate fails an explicit `BLOCK_BEFORE_MERGE` repository completion rule; green lint output cannot establish compliance with the stronger source rule.
- **Decision-complete correction:** Audit the complete Rust additions in `base..candidate` and add concise docs only to missing items, including required `# Errors`, `# Panics`, and cancellation/partial-progress notes where applicable. Do not add a lint/check, suppression, wrapper, or structural refactor.
- **Focused closure proof:** Source-audit every added/materially changed Rust declaration against §16, then run `mise run fmt` and `mise run lints`; the audit must find no undocumented added item and no suppression.

### `FIND-TASK-002-13`

- **Wave 1 source IDs:** R2 `DC-R2-002`
- **Status:** `REVISED`
- **Classification:** `MISSING`
- **Violated obligation:** TASK-002 Scenario 4 and AC-026 require the Rust, Python, and TypeScript Eval journeys to prove optional session/media, valid explicit or active trace identity through the real SDK-to-server path and fixed-width persisted columns, plus invalid trace-pair and malformed-media refusal before admission.
- **Exact location:** `sdks/wyrd-sdk-rust/tests/observe_run.rs:257-271,379-385`; `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:269-282,332-339`; `sdks/wyrd-sdk-ts/wyrd/tests/integration/observe-run.test.ts:184-186,239-250`.
- **Reachability and evidence:** These are the three gated real SDK journeys named by TASK-002. Rust emits default Eval options and does not query trace/session/media. TypeScript does the same and its active-span proof stops at a fake native object. Python alone reads persisted active/explicit IDs, but its journey still omits optional session/media and both required negative admissions. Unit/projection tests cannot expose binding, fixed-binary queue, IPC, or persisted-readback regressions.
- **Observable consequence:** A language binding or queue regression can drop/corrupt trace/media identity, or admit an invalid pair/media descriptor, while every recorded journey remains green.
- **Decision-complete correction:** Extend each existing journey, reusing its server and Eval row query. Rust supplies a valid explicit pair; Python uses its existing active/explicit cases; TypeScript supplies a real active pair. Each emits session/media, reads exact non-null fixed-width IDs and authored fields back, and asserts span-without-trace plus malformed media is refused before enqueue. Add no new harness, record type, or serializer.
- **Focused closure proof:** Run the existing three capability journey lanes and assert exact persisted ID bytes/hex and zero added rows for both negative cases; then run `mise run verify:bifrost`.

### `FIND-TASK-002-14`

- **Wave 1 source IDs:** R2 `DC-R2-003`
- **Status:** `REVISED`
- **Classification:** `MISSING`
- **Violated obligation:** AC-025, TASK-002 Scenarios 1/6/7, and the repository testing taxonomy require real-SDK journey evidence for fixed-table startup refusal, cached no-repeat schema IO, and stale-schema fingerprint refusal.
- **Exact location:** successful-only startup and dynamic writes at `sdks/wyrd-sdk-rust/tests/observe_run.rs:344-395`, `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:322-341`, and `sdks/wyrd-sdk-ts/wyrd/tests/integration/observe-run.test.ts:167-187`; supporting-only owner tests at `crates/shared/wyrd-client/src/observe/tests.rs:361-439,840-921`.
- **Reachability and evidence:** Each public journey starts against canonical built-ins and writes each dynamic table once. None refuses startup for either missing/incompatible fixed table, repeats a dynamic write while proving no second describe, or drives a stale writer through the server fingerprint fence. The shared tests correctly prove exact comparison and concurrent owner convergence, but lower-tier proof cannot replace the journey cases AC-025 names. Conversely, those shared owner tests are sufficient for one-describe concurrency/producer uniqueness and should not be copied into every thin language wrapper.
- **Observable consequence:** SDK option/binding construction, real describe caching, or server fingerprint enforcement can regress while the task's claimed AC-025 journey matrix remains green.
- **Decision-complete correction:** Extend the three existing journeys with table-driven failure of each fixed-table preflight and one repeated dynamic write whose server-observed describe count remains one. Add the stale-schema fingerprint refusal to the narrowest existing real-server journey over the shared Rust owner and assert the public stable error before accepting a replacement schema. Reuse the current server/table fixtures; do not duplicate owner-level concurrency tests across languages or add cache/harness abstractions.
- **Focused closure proof:** The three SDK journey lanes prove both fixed-table startup refusals and cached reuse; one existing real-server capability journey proves stale-writer fingerprint refusal; the focused shared convergence test remains the sole concurrency proof. Run those lanes and `mise run verify:bifrost`.

## Validation result

Four bounded findings remain: open `FIND-TASK-002-4` and
`FIND-TASK-002-10`, plus new task-required journey gaps
`FIND-TASK-002-13` and `FIND-TASK-002-14`. All other R1 findings are closed.
The retained corrections reuse the existing serializer, documentation rule,
SDK journeys, test server, Bifrost owner, and verification lanes; no new
abstraction, dependency, cache, queue, transport, or harness is justified.

**Result: FIX_REQUIRED.**
