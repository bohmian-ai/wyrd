# TASK-002 invariant review

Overall result: **FAIL**.

Final subject check: HEAD and all four packet hashes listed below remained unchanged at completion.

Immutable subject: base `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f`, candidate `75c9bfa9e07163099eac40d5a556c4447c6a519d`. HEAD matched candidate during inspection. This is the first review; no TASK-002 acceptance evidence is carried forward.

## Scope and authority

Reviewed provider-call identity through Skald dispatch, recording wrappers, hosted invocation binding, producer attempts, shared publication acceptance/receipts, successor consumption, and queued re-entry. Reviewed the full cumulative diff for these owners and unchanged dependencies needed to establish their invariants. No source was changed.

Authority: root AGENTS.md; `.agents/skills/wyrd-task-review/SKILL.md`; `changes/spec-driven-development.md`; `agent_docs/{index,rust,maintainer-style}.md`; `TESTING.md`; `architecture/README.md`; `architecture/wyrd-design.md` hosted-tool contract at 1981–2022; original TASK-002; specification REQ-046/047, INV-017–019 and AC-039/040; publication supplement naming/replay/workspace/ownership sections; durability supplement common execution/recovery contract. Revision-18 baseline and explicitly approved durability corrections govern; unrelated revision-19–21 wait/approval additions do not govern this review. No nested AGENTS.md exists under the inspected Skald, Vala, or server owner directories.

CodeGraph was used before source navigation (`mise exec -- codegraph explore` for AgentExecutor/RecordedTool/HostedTools/Publications/BlockCall and subsequently RunState and runner/sandbox). `mise tasks ls --hidden` established available lanes. This is a static acceptance audit: implementation's test-success claims were not independently rerun.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Preserve original provider call identity through existing loop and recording wrapper | `skald-agent/src/loop_runtime.rs:632` invokes `invoke_call(&call.id, args)`; `skald-tool/src/trait_.rs:32` preserves existing tools' default behavior; `vala-verify/src/blocks/tools.rs:272` forwards identity to inner tool inside the existing recording workflow; hosted `Builtin::invoke_call` forwards to Invocation | `hosted_agent_tools::agent_publish_retries_use_host_identity` checks duplicate ID returns identical receipt; static source tracing | PASS |
| Host supplies producer identity and fresh retries have separate publication scope | Engine `attempt` sets BlockCall.attempt from recorded attempts; `AgentExecutor::execute` constructs Producer from call.id/attempt; no producer arguments accepted; `HostedTools::open` binds producer to session | Replay journey checks attempts 1/2 and forged fields; busy-after-publication path below reuses a producer identity | FAIL |
| No independent Agent/publication verification state, snapshot/store/replay workflow | HostedBlocks passes clones of the same RunState Arc into TenantEvidence and HostedTools; Invocation constructs existing stateless borrowed Publications adapter; existing Publications::path/json and RunState acceptance remain sole publication workflow | Static comparison with base and existing Publications/read/upload consumers | PASS |
| Durable receipt contains only name/kind and replays without mutable scratch or inventory | Invocation::publish awaits existing Publications acceptance, then constructs receipt; Publications::path probes receipt before freeze/upload; RunState::accept_publication waits through receipt_once_accepted | Replay journey checks JSON replay; foundational backend tests cover file receipt replay, exact retained descriptors, and snapshot immutability; not rerun | PASS |
| Successors consume exact accepted references; missing named publication blocks before model invocation | TenantEvidence::publications reads shared RunState; TenantEvidence::published uses retained StoredObjectRef, not inventory rediscovery; existing scope/workspace publication resolution remains owner | `agent_publishes_file_directory_and_json` reads immutable file/tree/JSON/null through consumer tools; retry journey checks missing publication; lifecycle journey checks restored consumer reads without drafter rerun | PASS |
| Queued recovery preserves attempts and accepted publication attribution, including process-capacity failures | Existing engine forget and RunState invocation deletion were not adapted for a session that can already have accepted publications; runner derives resumed attempt count from surviving invocation records | Lifecycle journey interrupts consumer after drafter settlement; limits journey does not exercise publish followed by sandbox Busy | FAIL |
| Existing tools/wrappers remain supported; standalone code gets no publication capability | ToolDef and AgentDelegateTool retain invoke behavior through trait default; only RecordedTool and hosted Builtin need/implement forwarding; checks pass no Producer; Blocks still routes standalone code to CodeExecutor | Static full caller inspection; selection journey rejects publish on requirement Agent | PASS |

Items/iterations are currently absent from the plain-step engine; both existing checkpoint producer and new Agent producer consistently use None. This task does not implement future foreach/repeat execution, so their implementation is not demanded here.

## Proposed finding INV-002-1 — an accepted publisher attempt is forgotten after a later capacity refusal

Classification: **INCORRECT**. Severity: material integrity/recovery defect.

Violated obligation: TASK-002 requires host-owned attempt attribution, separate fresh-attempt publication names, conserved bounds, and real direct/queued integration with shared state. REQ-047 requires fresh Agent retries to have separate attempt attribution and not merge prior content. The publication supplement explicitly supports an Agent combining `code.run` and `publish`; resource refusal/recovery is a supported operational path.

Changed integration location: `crates/vala/vala-verify/src/blocks/agent.rs:451–472` supplies the producer used by new hosted publishing; `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:341–374` now accepts publications for that producer. The affected unchanged lifecycle owners are `crates/vala/vala-verify/src/blocks/engine.rs:873–875` and `crates/wyrd/wyrd-server/src/verification/state.rs:1013–1030` (`Checkpoints::forget`).

Reachable producer-to-consumer trace:

1. A queued Agent selects `publish` and `code.run`. It publishes `report` during attempt 1. Invocation derives Producer(step, None, None, 1); Publications/RunState durably accept the publication and return success.
2. A later model turn calls `code.run` while the process sandbox admission is full. `Sandbox::exec` at 202–205 returns BlockError::Busy. `Invocation::run` preserves that typed cause, RecordedTool's Refusal observes it, and AgentExecutor::invoke returns BlockError::Busy instead of the run (`blocks/agent.rs:422–426`). This can occur after earlier calls have succeeded; session refusal is not restricted to initial admission.
3. Engine::attempt treats Busy as if no work ran and calls `forget(step, 1)`. RunState::forget deletes the unsettled InvocationRecord but leaves accepted `state.publications` intact. The resulting checkpoint contains report/attempt 1 without the invocation that consumed attempt 1.
4. Runner maps Busy to Deferred (`runner.rs:1707–1710`); next dispatch builds checkpointed attempts solely from `state.snapshot().await.invocations` (`runner.rs:832–843`). Engine's restored count is therefore zero and it starts another model run as attempt 1.
5. Republishing the logical report with a fresh provider ID is refused by `state.rs:960–978` as a same-attempt name conflict. If a provider reuses the original ID and arguments, the new run instead receives the prior receipt/content as its own. The queued retry cannot use the fresh-attempt scope promised by the contract; accepted history no longer accurately identifies separate Agent invocations. Repeated Busy deferrals can repeat model work without consuming its recorded block attempt bound.

Sibling owners/writers checked: both engine action and check Busy paths call forget; checks cannot publish. Workspace admission can produce Busy before a session starts and should retain its existing no-attempt-cost behavior. `code.run` is the in-session Busy source. `RunState::begin/settle/forget`, publication acceptance/receipt, and runner checkpoint restoration share this authority. Publications must not be removed to repair attribution: acknowledged snapshots remain retained. Ordinary Agent retryable failures already preserve attempt history and advance the attempt; no new replay store or scheduler is needed.

Smallest testable correction boundary: extend the existing invocation/forget lifecycle so an attempt with retained publication effects cannot be forgotten and then reused for a fresh Agent run. Preserve its original invocation/attempt and let existing recovery count it as spent; preserve current deferral/no-spend behavior for admission refusal before retained work. Use the existing RunState publication/invocation authority rather than downstream name guards, changing provider IDs, deleting accepted content, or introducing a separate session ledger. Reconcile the action/check forget callers against this invariant. This is a bounded lifecycle adjustment; it does not require new public APIs, storage, or retry machinery.

Focused closure proof: a deterministic existing-host test should accept a publication, force a subsequent sandbox-capacity refusal, recover the same queued execution, and demonstrate that a fresh permitted Agent attempt has a distinct Producer.attempt and can publish the same logical name while the old publication remains inspectible. With no retry budget, the earlier attempt must remain counted instead of being restarted as attempt 1. Retain a control case showing capacity refusal before retained work still defers without consuming an attempt. The current seven journey tests do not cover this ordering.

## Reuse and verification limits

The new identity hook and recording helper extend the existing loop/registry/session; no duplicate lifecycle or storage workflow was found in this boundary. The finding concerns an existing admission assumption exposed by newly durable in-session side effects, not a request to redesign that workflow.

Reviewed tests statically: hosted_agent_tools replay/publication/limits/lifecycle journeys, vala blocks-agent callers, and existing publication receipt/state readers. Runtime tests, broad aggregates, SDK projection correctness, security policy details, and workspace concurrency internals were not rerun or independently audited here; other selected specialists own those boundaries. No optional hardening findings are proposed.

Packet hashes observed during review (SHA-256): spec `5af4975a59d8aaba3702e5c2fde40d34b074760456a293cf61f30cd907a5e683`; publication supplement `a02ceaf9c6f40f07e5995925161c64dc38fb7563471e993d3212810533b773e9`; durability supplement `08c4bf050e2f73cf5bb4eabcd91d412c3e73cf727b1a415165d504e9cb804d9f`; TASK-002 `7c1773159a2d348d097a46fd5876f748bb32d1569447b102b664657117690bbe`.
