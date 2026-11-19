# TASK-001 maintainer review

Result: **PASS within maintainer scope after reassessment.** MNT-001 is withdrawn. No replacement dispatcher, worker transport, or tool service was found. Other reviewers’ runtime findings remain separate.

## Subject and authority

Reviewed immutable base `5234961d51798581a026834cfffd5efaadc226b4` through candidate `0e51af2a2332f362ab44b7c5b88b51c3cb4f8011`. HEAD still identifies that candidate at closeout. This is the first cumulative review; no prior acceptance evidence is carried forward.

Read the review scope, approved revision-2 specification, original TASK-001 including its reported evidence, root AGENTS.md, the task-review skill, changes/spec-driven-development.md, agent_docs/index.md, agent_docs/maintainer-style.md, agent_docs/rust.md, the Verifier tooling/workspace/record authority in architecture/wyrd-design.md, and the TypeScript directory rules. CodeGraph is absent. No source, task, specification, or generated files were changed. No Cargo commands or subagents were used.

## Changed-surface coverage

| Changed surface | Source, callers and proof inspected | Maintainer assessment |
| --- | --- | --- |
| ToolSession, Dispatch, RecordedTool, ToolBridge, Halt | Complete blocks/tools.rs; base RecordedTool/Refusal implementation; AgentExecutor::invoke; hosted HostedTools::open and Invocation; Vala bridge/budget/Busy tests | The existing recorded-call workflow is extended into one common owner, rather than copied for Python. Halt extends the old refusal signal. Shared Dispatch is necessary because direct and nested calls need one declaration set, admission rule, record path and semaphore. |
| ToolCalls and engine settlement | blocks.rs BlockCall/ActionOutput; action retry loop, Attempted::settle, check execution and Judged; failed-step/inconclusive-check and retry-budget tests | Moving records to engine-owned invocation state closes failure/retry retention that returning ActionOutput alone cannot provide. It does not introduce a durable journal or a second result owner. |
| CodeTool and hosted Invocation | Complete new trait, both holders, with_code, invocation branches and all implementations/callers | The trait provides the required cross-tier injection without a server dependency in Vala. Invocation and the substantive Script integration-test implementation use that seam. No correctness defect or benefit from callback replacement was established. Hosted protected execution remains Invocation::call, including code.run. |
| Sandbox future/event driver | Complete Sandbox::exec, drive, request, answer; base exec; installed monty-pool 1.1.0 checkout feed/resume/name lookup/future/mount APIs | The existing checkout and suspension loop is extended. Installed checkout APIs supply transport and mount execution, not Wyrd's declared-tool bridge, safe-error projection or concurrent host-call driver. drive keeps pending host work polled during interpreter and workspace waits; it operates on caller-owned temporary state. No competing worker lifecycle is introduced. |
| JSON conversion | Estimate::json, quoted_len, python_from_json/to_python; installed monty-types object accessors and serialization; conversion test | Existing authored-code JSON text conversion does not meet the bridge boundary: it cannot independently reject Python non-string keys and other unsupported values before JSON coercion. Bounded graph conversion has an earned purpose, uses existing Monty objects, and adds no serializer dependency. |
| Workspace callbacks and protected IO | Invocation::call/code, Workspace::writer/mounts/work_room; Sandbox OS-call branch; workspace and host-wait tests | Mutation coordination retains Workspace as owner. Per-write locking is needed to permit awaited extraction; copying the previous whole-program lock would deadlock. The parent still delegates mounted IO to resume_from_mounts and extraction to Workspace. Query cancellation uses the existing query owner and token/drop guard. |
| Test/provider scaffolding | Vala Fixed/Budget/Busy/Script toolsets and Recording provider; server Scripted/CodeAgent; Rust PolicyDesk, script_code/script_reduction, reduction/readback helpers and public failure story | New helpers retain explicit state and local scope. Scripted delegates response production to existing MockProvider and adds request capture required by visibility assertions. CodeAgent shares actual hosted construction across several boundary tests. PolicyDesk extends its existing server/provider story. None replaces a production workflow. |
| Generated and result consumers | ToolCallRecord/ToolRecipient, RequirementResult and decode_data; generated TypeScript declarations/error additions; Rust SDK exports and construction-site diff | Wire additions and decode_data preservation match the Rust owner. New records explicitly emit recipient; Rust historical decoding defaults it to model, and generated TypeScript describes the canonical emitted result. Focused language journeys/stubs are separately owned by TASK-002. No independent handwritten wire model was introduced here. |

## Whole-workflow reuse audit

| Existing alternative | Missing behavior and extension decision |
| --- | --- |
| Skald run_loop direct tool batch, callbacks, conversation/journal and buffer_unordered cap | Reusing this whole loop for each Python child would entail model/session bookkeeping and result conversation insertion that the bridge must avoid. Candidate correctly reuses its ordinary Agent loop for direct calls and the underlying declared AgentTool operations for children. Its shared non-code semaphore supplies the invocation-wide bound that the direct-batch cap cannot enforce inside one suspended code.run. |
| Base ToolSession/RecordedTool/Refusal | Base code already owns result bounds, recording and Busy propagation, but its records are attempt-local and only finished records survive successful invocation return. Candidate extends that workflow into Dispatch/ToolCalls/Halt instead of preserving two implementations. |
| Hosted Builtin/Invocation capability dispatch | Candidate still invokes the same accepted Invocation and capability owners. The special bridge-bearing code entry returns to Invocation::call before parsing/running code, preserving authorization. No additional authorization or tool implementation is warranted. |
| Sandbox::run and CodeExecutor authored entrypoint workflow | Authored code remains computation with explicit JSON inputs and no bridge. Passing a bridge into the existing exec owner supplies the different approved Agent contract without replacing authored-code execution. |
| Monty checkout and mounts | These remain the execution/IPC/file mechanism. A Wyrd-specific bridge driver is needed because the installed API intentionally returns host events to its caller; it cannot select accepted Wyrd authority or record child calls itself. |

## Reassessment and withdrawn finding

MNT-001, previously retained as FIND-TASK-001-7, is withdrawn at the
maintainer's request following a bounded source reassessment. The original
review interpreted “two real implementations” as “two production
implementations”; neither root AGENTS.md nor agent_docs/rust.md explicitly
makes that restriction.

`CodeTool` is a legitimate cross-tier seam: Vala owns dispatch, while the
server's `Invocation` performs protected execution through `Invocation::call`.
The trait keeps the concrete server type out of Vala. `Script` is a substantive
integration-test implementation exercising concurrent children and boundary
rejection, not an empty implementation invented to satisfy a count. The
holders do not establish an owning-reference cycle.

Replacing it with a callback would still require dynamic dispatch, captured
invocation state and an asynchronous future. The cited `BeforeModelFn`
precedent is synchronous; it did not establish that the proposed asynchronous
replacement was simpler. No trait-caused correctness defect or concrete
maintenance improvement from removal was demonstrated. Keep `CodeTool`; no
callback replacement or artificial second production implementation is required.

Source references: `crates/vala/vala-verify/src/blocks/tools.rs:88,191,521`,
`crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:482`,
`crates/vala/vala-verify/tests/integration/blocks_agent.rs:1704`, and
`crates/skald/skald-agent/src/callbacks.rs:61`. The reassessment changes this
finding's disposition only; no runtime finding from another scope is cleared.

## Verification limits

This is static specialist review. Reported successful focused tests and checks were read as evidence claims, not rerun. Runtime cleanup, concurrency, security and record-integrity acceptance remain with the selected independent specialists; the coverage above establishes the ownership/reuse assessment, not an independent execution of their test lanes.

No Cargo or language tests were rerun for this reassessment. No source changed; other runtime and evidence-integrity findings remain unresolved.
