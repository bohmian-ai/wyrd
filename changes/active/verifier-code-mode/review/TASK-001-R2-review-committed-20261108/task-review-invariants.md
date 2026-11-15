# TASK-001 R2 invariant review

Result: **FAIL**. **FIND-TASK-001-2 remains open**, narrowed to cancellation of the newly shared settlement await. No new finding ID or broader lifecycle requirement is proposed.

## Immutable subject and scope

Original base `5234961d51798581a026834cfffd5efaadc226b4`; prior candidate `f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18`; candidate/checked HEAD `9abeaf124585cd0d726d2554965b105fc0c0ef86` (runtime commit `66f4d54ca14d9da0257aa6a72239687e7abef004`). Inspected `git diff f8b45a720 9abeaf124`, actual changed bodies and affected producers/consumers, original TASK-001, R1 validated ledger/invariant report and R2 remediation/evidence. Approved authority is revision 2 from `git show 9abeaf124:changes/active/verifier-code-mode/spec.md`; local revision 3/TASK-003 is excluded.

Applied root AGENTS, task-review skill, specification workflow, agent-docs index/Rust/maintainer guides, architecture authority map and owning Agent/tool/workspace section (`architecture/wyrd-design.md:1842–1923`), and testing tier guidance. No `.codegraph/` directory or applicable nested AGENTS exists in the affected runtime trees. No builds/tests, source experiments, index/HEAD changes, or writes outside this report were performed. Runtime execution evidence is implementation/orchestrator evidence, not this reviewer's execution.

Scope is the existing result-producing authored-attempt timeout/retry obligation and its changed settlement owner. Nine R1 closures carry forward; FIND-7 remains withdrawn. Whole-execution deadline, no-result abandonment, lease loss, arbitrary Drop and new shutdown promises remain excluded.

## Producer-to-sink trace

1. `HostedTools::open` (`crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:139–179`) creates the existing Invocation/TaskTracker and installs `work.close(); work.wait().await` with `ToolSession::with_settlement`. It remains the sole production settlement producer. `Invocation::blocking` (`:303–317`) moves the owned workspace writer and Dispatch permit into tracked blocking work; its caller dropping cannot cancel an already-started extraction. Existing Workspace/extractor ownership is reused.
2. `AgentExecutor::invoke` (`crates/vala/vala-verify/src/blocks/agent.rs:384–385`) synchronously hands the future from ToolSession into the engine-owned ToolCalls after open. There is no await between receiving the session and this handoff. Resolve errors occur after handoff; the engine's new post-attempt settle covers them. If the timed Agent is dropped while still running, the pending future remains in ToolCalls and the new engine wait works.
3. Normal Agent completion and Halt both converge at `agent.rs:413–422`. The executor still invokes `calls.settle().await` **inside** the `actions.execute` future controlled by the authored attempt timeout. This preserves normal checks and direct executor calls, but creates an earlier competing consumption point.
4. `ToolCalls::settle` (`blocks/tools.rs:170–180`) pops the only settlement out of `CallLog.settlements` before awaiting it. Pending settlement then belongs solely to that settle call, not to the engine-owned shared state. Cancelling that await drops it permanently. The mutex protects pop, not the wait's lifetime.
5. `VerificationEngine::attempt` (`blocks/engine.rs:675–686`) drops `actions.execute` at the authored timeout and then invokes `tool_calls.settle()`. If the Agent was already awaiting settlement, the list is now empty, so this second call returns immediately. Retry/Attempted progression (`:689–701`) bypasses still-running tracked work.

Sibling consumers were checked: these are the only two ToolCalls::settle call sites; ToolSession::settle_with has one caller; HostedTools is the sole production with_settlement producer. Check execution relies on the Agent's normal settle and has no separate authored-attempt timeout. No change is needed in extraction, delivery, records, or the server's TaskTracker producer to explain this failure.

The installation/handoff is an appropriate extension of existing owners rather than a replacement lifecycle framework. The defect is that ownership survives cancellation only until the first wait begins.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-012 / AC-009: settle work when authored timeout interrupts an Agent still running tools | Handoff at open plus engine post-attempt wait | New `blocks_agent::timed_out_attempt_settles_before_retry_or_result` holds direct HeldWork until engine begins settlement, checks retry/no-retry, records cancellation/no digest | PASS for this phase |
| Same obligation when authored timeout interrupts an Agent already performing cleanup after return/error/Halt | Agent consumes settlement inside timed future; ToolCalls pops before await; engine fallback sees empty list | Actual await/drop trace above; new test never makes the inner Agent enter settle before timeout | **FAIL, FIND-TASK-001-2** |
| Normal completion/Halt without a later attempt timeout waits for work | Agent `calls.settle().await` before dropping session/returning halt | Existing owner settlement and interrupted-permit evidence carried; source supports ordinary completion | PASS, subject to preceding cancellation exception |
| REQ-011/013: retain owned permit/writer/workspace and existing serialized/no-overwrite scratch behavior | Unchanged Invocation::blocking, Workspace and Dispatch ownership | R1 concurrency/owner proof carried | PASS within session; cross-attempt overlap remains same FIND-2 consequence |
| Preserve retry classification/count, record allowance across retries, no late returned value/digest | Timeout remains Retryable; same ToolCalls state; Dispatch/Delivery unchanged | R1 records/budget proof carried; new test checks cancelled extraction bytes/digest and retry count | PASS except premature progression above |
| Nine R1 closures: FIND-1 host stop identity; FIND-3 coordination deadline; FIND-4 admitted records; FIND-5 delivery; FIND-6 stopped waiters; FIND-8 docs; FIND-9 recovery proof; FIND-10 exact integers; FIND-11 exact QueueFull Busy | R1 `findings-validation.md` reconciled matrix and cited source; R2 changes none of those producers/contracts. Changed normal settle placement preserves error projection absent the shared cancellation gap | R1 independent verification and original authority/security/records/standards evidence carried; R2 appended evidence supplies narrow reruns | PASS, carried forward; no new issue IDs |
| FIND-7 withdrawal; no new service/runtime/store/transport, no public/schema/dependency change, no new abandonment/shutdown contract | Bounded diff uses ToolCalls, ToolSession, AgentExecutor and engine | Static diff and caller inventory | PASS |

## Retained material proposal

### INV-R2-001 — FIND-TASK-001-2: timeout during inner settlement discards the engine's pending cleanup

Classification: **INCORRECT**, incomplete closure of the same prior cleanup cause. Obligation: approved revision-2 REQ-012, AC-009, original task cleanup requirement, and R2's explicit settlement-before-retry/result outcome. Changed source: `blocks/tools.rs:170–180`, with cancellation consumer `blocks/agent.rs:420–422` and timeout owner `blocks/engine.rs:675–686`.

A supported reachable scenario is a bounded extraction already running when the Agent loop ends early due to a sibling host stop, such as budget exhaustion, an uncaught program failure, or Busy. Shared Dispatch explicitly supports sibling direct/nested operations and Halt; it raises host stop in `tools.rs:648–652`, and Agent's select drops the other calls at `agent.rs:413–417`. The real sandbox similarly polls child calls alongside interpreter turns (`hosted/sandbox.rs:438–458`), drops unresolved calls with the operation, and projects Python failure through `Invocation::code` into the same Halt. The extraction itself continues in TaskTracker with its guards. These are accepted failure paths, not arbitrary external destruction.

If that stop occurs before the authored step timeout, Agent starts settlement, pops its pending wait, and waits for extraction. If bounded extraction remains active through the authored timeout, timeout_at drops the inner wait. The subsequent engine wait returns empty and starts a retry or produces the failed timeout step while extraction remains active. This requires no change in host limits and no guarantee that bounded extraction always finishes before an authored one-second timeout exists. With retries, the next session has a fresh semaphore/workspace while old work retains the previous session's permit/workspace; without retries the reported attempt completes before required cleanup. Guards prevent premature deletion, but do not establish settlement-before-progression.

This is the same supported result-producing boundary retained by R1 validation. The added engine fallback fixes timeout during execution but misses timeout during cleanup. No separate shutdown/no-result guarantee is needed to retain it.

**Smallest correction boundary:** retain cancellation-safe access to unfinished settlement in the existing ToolCalls owner until it completes, so cancellation of the Agent's inner wait leaves the engine's post-attempt wait able to await that same work. Keep TaskTracker, writer/permit guards, and existing result/error/budget owners. This is a local ownership repair; no new framework, runtime, detached cleanup task, async Drop or durable journal is justified. Merely moving the join out of Agent is not a complete standalone correction: checks and direct executor consumers currently depend on that normal wait. Moving joins to existing outer owners is viable only if those consumers retain equivalent settlement; retaining unfinished settlement at the shared owner addresses the cause without redistributing their lifecycle responsibilities. A new downstream guard or longer timeout cannot repair lost ownership.

**Focused proof:** extend the existing engine/AgentExecutor/HeldWork/SettlingToolset seam with an inner-completion or Halt case that starts blocking work, cancels its awaiting tool call, and enters Agent settlement before authored timeout. Synchronize on the settlement being polled; keep work held past the timeout, and assert no retry/new session and no final result until release. Then assert existing retry/final-failure classification and cancelled/no-digest records. Reuse the same retry/no-retry cases and existing start/release events; no server harness or broad journey rerun is required. Existing new test covers the complementary timeout-before-inner-cleanup path and should remain.

The recorded green tests establish their asserted phases but do not falsify this counterexample: in the new test the only model tool call awaits HeldWork until timeout, so Agent never reaches `calls.settle()` before engine timeout. The first settlement poll therefore happens in the safe outer engine owner. The test releases work as soon as that poll is observed and does not test a previously polled inner settlement being cancelled.

## Conclusion

One source-proven residual proposal, preserving **FIND-TASK-001-2**; no other proposal. Carry nine closures and FIND-7 withdrawal unchanged. Overall **FAIL** within the assigned invariant scope.

Skipped builds/tests and broad rediscovery as assigned. Remaining risk: an authored timeout during cleanup can discard the pending wait and report/retry while prior bounded work still runs.
