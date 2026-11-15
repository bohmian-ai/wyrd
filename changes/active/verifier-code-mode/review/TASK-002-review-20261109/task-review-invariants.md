# TASK-002 invariant review

Result: **PASS**. No material findings in the assigned invariant scope.

## Immutable subject and authority

- Base: `e5af2ce5caef2d00303e1c980b12c8a9bca4af0e`.
- Candidate: `ff78b51b61fea8f215a2139b17840044f65b17e4`.
- First review of the full base-to-candidate range, focused on recipient/default/call-record/custom-data preservation through Python, TypeScript, MCP, HTTP schemas and retained output. No prior TASK-002 acceptance evidence is carried forward.
- Approved authority: revision 2 of `changes/active/verifier-code-mode/spec.md` read from candidate; original `tasks/TASK-002-public-code-mode-workflow.md` read from base. Working revision 3 was excluded.
- Read the task-review skill, supplied root rules, Python and TypeScript directory rules, `changes/spec-driven-development.md`, `agent_docs/index.md`, maintainer style and agent-surface guide, and owning verification/result/tool sections of `architecture/wyrd-design.md`.
- No CodeGraph directory exists. Reviewed tracked runtime source is unchanged in the working tree; source inspection therefore describes the recorded candidate. Only this report was written.

## Producer-to-consumer trace

`vala-verify/src/blocks/tools.rs::Dispatch::call` (612 onward) creates one record with its intended recipient, argument object, zero bytes and no digest. Direct dispatch passes `ToolRecipient::Model`; `ToolBridge::call` passes `Code`. Successful bounded serialization computes the delivered JSON digest/size. `Delivery::deliver` commits them only on delivery; refusal and unfinished settlement produce zero bytes/no digest. The record has no raw result field.

`blocks/engine.rs::attempt` keeps one `ToolCallsOwner` across attempts and settles it before returning records. `Attempted::settle` includes the same records on both successful and failed steps (1136–1162). Agent/code checks settle before constructing `Judged`, then attach records after successful, invalid-output and inconclusive branches (773–799). `Judged::report` copies them into `RequirementResult` (1209–1222); the non-Agent paths use an empty vector. Citation invalidation does not discard records because record attachment follows `Judged::cited`.

The shared wire owner is `wyrd-spec/src/verification/execution.rs`: `RequirementResult.tool_calls` and `StepResult.tool_calls` default empty and omit empty values; `ToolCallRecord.recipient` defaults to `Model` and is always serialized. `RequirementResult::decode_data` copies every non-data field including `tool_calls`; whole-result decoding retains steps/evidence. Thus historical normalization happens in the existing Rust deserializer before native SDK projection.

Python's `src/verify.rs::PyVerificationResult::{plain,typed,requirements,steps}` serializes these Rust objects generically. Its `decode_data` replaces only each present dictionary's `data`. Execution status uses the same serialization; typed status transforms only its requirements; wait and typed wait use the same result class. The authored stub now describes the fields without introducing runtime classes or field filters.

TypeScript's native execution status/wait use `NativeLifecycleResult::outcome` (`native/src/lib.rs:204`), serializing the shared Rust result generically; the wrapper parses that JSON. `src/index.ts::RequirementResult` omits only wire `data`, and `typedResult` (301) spreads the remaining requirement fields plus the entire result/outcome. The generated `ToolCallRecord` recipient union and optional requirement calls therefore remain intact through immediate and queued typed APIs.

The persistence producer `wyrd-server/src/verification/results.rs::ResultPayloadBuilder::build` (289) serializes borrowed whole requirement/step slices through `StoredOutput`; it does not reconstruct individual records. The retained reader `verification/inputs.rs::InputResolver::result` (396) adds summary verdict and resolved evidence to the stored output and decodes the shared outcome. It does not rewrite historical payloads or their bytes/digests. MCP execute/get-execution output descriptors use `VerificationResult`/`VerificationExecutionState` directly (`src/mcp/verification.rs:86,114`); their handlers use existing control/read paths and generic structured serialization.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-015/016: honest recipients, bounded records, no raw results | `Dispatch::call`, `Delivery::{deliver,refuse}`, `ToolCallsOwner::settled`; shared `ToolCallRecord` | Python `assert_reduced` and denial assertions; TS `assertReduced` and denial assertions; MCP `records`/`assert_reduced` assert outer model and nested code, digest/bytes and zero-byte denials | PASS |
| REQ-017/INV-006: Agent check calls survive valid custom data, failed/inconclusive checks and failed steps; non-Agent calls empty | Engine check settlement attaches calls after judgment validation; failed/successful step branches both copy calls; `Judged::bare` and non-Agent paths empty | New public journeys exercise a failed custom-data Agent judgment; existing `blocks_agent.rs::agent_records_survive_failed_steps_and_inconclusive_checks` directly asserts invalid-output retention. Source confirms unchanged producer branches | PASS |
| AC-013: historical absent recipient defaults model; absent calls default empty; new records explicit; historical payload unchanged | Shared serde defaults at execution.rs:291,380,413; generic native projections; stored-output reader decodes rather than updates stored payload | Existing shared `verification_result_round_trips_documented_outcome` (903) removes historical fields, verifies defaults and explicit reserialization. No need for per-SDK duplicate decoder | PASS |
| Typed decoding changes only `data` | Rust `RequirementResult::decode_data` (557); Python dictionary replacement; TS `typedResult` spread and derived type | Python immediate typed and wait-typed assertions; TS verifyTyped and waitTyped assertions; Python/TS static fixtures | PASS |
| Immediate, queued wait/status and retained output preserve calls and custom data on public APIs | Shared `Execution::{status,wait,wait_typed,status_typed}`; native adapters; server stored output and reader | Python journey at test_verifier_building_blocks.py:808, TS journey at verifier-building-blocks.test.ts:906, MCP journey at verification.rs:3125 compare requirements/steps across result/readback and inspect custom data | PASS |
| Runtime served contracts agree with shared Rust fields and compatibility optionality | Rust derives own HTTP and MCP result schemas; authored Python stub uses optional call lists and normalized recipient literal union; TS generated type remains owner-derived | Served OpenAPI `verification_tool_records_match_served_contract` (1644) asserts both arrays share ToolCallRecord, optional historical fields, exact recipient enum and no added raw-output field. MCP journey inspects advertised output schema and actual records | PASS |
| INV-001 and task non-goals: SDKs project rather than own lifecycle/authorization; no new transport or retained store | Actual diff adds authored fields, public/type/schema tests and shared program fixtures; no production runtime/client engine/store change | Supplied codegen/native/stub checks and source inspection; program extraction preserves the existing Rust fixture program | PASS |

## Verification assessment and limits

The candidate task's Implementation Evidence records passing focused Rust SDK, Python (five selected tests), TypeScript, real MCP and served OpenAPI commands, plus format/lint/type/native/package/codegen/docs checks. These are supplied execution results, not independent reruns. Source inspection independently establishes the invariant paths; no Cargo/build/test command was run or source changed in this review.

No material MISSING, INCORRECT, DRIFT, VIOLATION or REGRESSION finding survived the scoped checks. In particular, making recipient required in the Python normalized-output TypedDict does not break historical result decoding: the shared Rust decoder inserts `model` before the generic Python projection. Optional requirement calls accurately describe omitted empty vectors.

The full TASK-001 concurrency, capacity, cancellation, security and worker fault matrix was not restarted. Sensitive capture/access-control acceptance belongs to the separately selected security reviewer; this review establishes that no new client projection or record payload field bypasses the existing owner. No new risk was identified within this scope.
