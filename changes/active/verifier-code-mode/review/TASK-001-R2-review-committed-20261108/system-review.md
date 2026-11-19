# TASK-001 R2 system resilience review

Result: **FAIL**. Proposed source-local **SYS-R2-001** is an incomplete closure of existing **FIND-TASK-001-2**, not a new finding or policy.

## Immutable subject and scope

Reviewed `f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18` → `9abeaf124585cd0d726d2554965b105fc0c0ef86`, runtime candidate `66f4d54ca14d9da0257aa6a72239687e7abef004`. HEAD remains the cumulative candidate. Approved revision 2 was read from `git show 9abeaf124:changes/active/verifier-code-mode/spec.md`; local revision 3 is excluded. Read the original task, R1 validated ledger/verdict/system evidence, R2 task and appended implementation evidence, shared scope, task-review skill, repository rules, maintainer/Rust guides, spec-driven workflow, owning Verifier tool authority, Oracle queue and deployment shutdown sections. CodeGraph is absent; no nested AGENTS.md applies to the changed runtime/test directories.

The scope is settlement ownership, timeout → retry/result, normal completion/Halt, and affected global-deadline and queue callers. Carry forward the nine R1 validated closures and other unchanged acceptance recorded in `../TASK-001-R1-review-20261108/findings-validation.md`; finding 7 remains withdrawn. No tests/builds or source/index/HEAD changes were performed. This report is the only write.

## Deployed path and failure propagation

Hosted Agent actions still enter the existing Vala engine and AgentExecutor. HostedTools::open creates the per-attempt Invocation and TaskTracker and supplies `work.close(); work.wait().await` through ToolSession (`crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:139–179`). `Invocation::blocking` retains the owned writer, active-operation permit and work closure inside actual tracked blocking execution (`:306–320`); dropping its async waiter cannot end that execution. The server owns IO; Monty remains a subprocess without server credentials or capability ownership. R2 introduces no replacement runtime, service or shared-server shutdown path.

R2 hands the existing settlement into engine-owned ToolCalls immediately after session open (`blocks/agent.rs:384–386`). Engine::attempt awaits ToolCalls after its timeout wrapper (`blocks/engine.rs:676–686`). This correctly repairs timeouts that occur while the Agent is still running. However, the Agent also consumes that settlement inside the same timeout wrapper on completion/Halt (`blocks/agent.rs:413–426`). ToolCalls removes the settlement from shared state before awaiting it (`blocks/tools.rs:170–180`). A timeout during this second phase therefore loses the only join and still advances.

## Scoped acceptance matrix

| Obligation / reachable path | Implementation evidence | Verification evidence and limit | Result |
| --- | --- | --- | --- |
| Authored attempt timeout while Agent/tool call is running must settle before retry or failed result | Session → ToolCalls transfer at Agent open; engine settlement after `timeout_at` | New `blocks_agent::timed_out_attempt_settles_before_retry_or_result` holds extraction across timeout and tests retry/no-retry, no-value cancellation and reopen-after-finish. The held direct call prevents the Agent reaching its own settlement first. Implementation reports GREEN; orchestrator reports focused tests pass. | PASS for this phase |
| Timeout while Agent is already settling started work must also settle before progression | Agent awaits ToolCalls inside timed action; ToolCalls pops before await; outer engine then finds empty settlement list | New test releases held work immediately when first settlement poll is observed (`blocks_agent.rs:2715–2724`); it never holds an Agent-owned settlement across the authored timeout. Source establishes the missing failure below. | FAIL |
| Normal Agent completion/Halt with settlement finishing before authored timeout retains resources and original result/error | Agent settles before dropping session and accepting run/error; hosted TaskTracker and owned work guards unchanged | Prior hosted extraction/writer and Vala permit proofs carry; R2 evidence reports retained regressions PASS. Halt/error is returned only after settlement when uncancelled. | PASS |
| Global execution deadline and no-result abandonment retain their existing bounds and lease safety | Engine::run still wraps drive in deadline (`engine.rs:359–373`); runner wraps dispatch (`runner.rs:391–398`) and selects abandonment/lease loss (`:892–899`) | Actual unchanged consumers inspected. New await stays inside existing outer cancellation owners. No new result-after-abandonment or async-Drop promise inferred. | PASS, scoped preservation |
| Exact Busy still defers queued work without subject judgment; healthy sibling service survives | Engine Busy mapping unchanged after settlement (`engine.rs:689`); runner Busy → Deferred (`runner.rs:1049–1052`), existing queue lease/defer owner unchanged | R1 source-backed exact-QueueFull and queue recovery composition carries. No process abort or admission-policy change added. The residual settlement loss can still overlap work or produce unrelated capacity refusal, covered by the same finding. | PASS for unchanged classification; residual below |
| Existing owner reuse and non-goals | ToolCalls, ToolSession, AgentExecutor and Engine::attempt extended; hosted TaskTracker retained | Diff contains no new dependencies/features/runtime/journal, shutdown service, arbitrary-Drop blocking, or public contract. | PASS |

## Proposed residual: SYS-R2-001 — interrupted settlement is removed from its surviving owner

**Classification:** INCORRECT / incomplete FIND-TASK-001-2 closure. **Obligation:** approved revision-2 REQ-012 / AC-009, R2 result-producing timeout/retry settlement guarantee; shared non-code cap preservation across retries is affected by the same cause.

**Exact changed location:** `crates/vala/vala-verify/src/blocks/tools.rs:170–180`, consumed at `blocks/agent.rs:422` inside `blocks/engine.rs:677` and retried at `blocks/engine.rs:686–698`.

**Supported producer-to-consumer trace:**

1. A generated `code.run` awaits nested `fs.extract`; hosted Invocation::blocking starts bounded tracked work and retains its actual writer/permit. The server's configured Python operation limit may be earlier than the authored step timeout (`config.rs:1817–1822`; `hosted/tools.rs:337–347`). This is an existing supported setting; extraction duration is not guaranteed shorter than it.
2. At that earlier Python limit, Sandbox::exec's `timeout_at` drops its pending bridge calls and returns a terminal Python timeout (`hosted/sandbox.rs:244–405`). Blocking extraction continues under TaskTracker. Invocation::code preserves that BlockError via host_stop (`hosted/tools.rs:361–372`). Halt::observe raises the typed session stop (`blocks/tools.rs:487–507`). No further tool/model work is needed to reach cleanup.
3. AgentExecutor's Halt select ends the Agent loop and calls `calls.settle()` (`blocks/agent.rs:413–422`). ToolCalls pops the sole settlement, then awaits `work.wait()`; work remains held.
4. The later authored attempt timeout now expires while this Agent-owned settlement is pending. It drops actions.execute, including the popped settlement. Shared ToolCalls still exists, but its list is empty. Engine's added `tool_calls.settle()` returns immediately and the existing retry or failed Attempted result proceeds (`blocks/engine.rs:676–698`).
5. Old extraction can remain active when a new session opens with its own Dispatch semaphore, or while a no-retry failed result has already been produced. Guard retention prevents early directory deletion, but cannot satisfy settlement-before-progression.

**Consequence and materiality:** a supported result-producing timeout still claims cleanup/progression before started bounded work has settled. Retried non-code work can overlap an old attempt despite cap 1; workspace capacity may remain occupied after reported completion. This is the explicit original/R2 obligation, not speculative arbitrary cancellation hardening. The same source can lose settlement after normal Agent completion with outstanding cancelled child work; the earlier Python-timeout/Halt producer already proves material reachability.

**Smallest correction boundary:** retain the existing session settlement in the surviving ToolCalls owner until it actually completes, including cancellation of a current waiter. The next engine await must resume/join that same work after the timed Agent future is dropped. Keep normal/check completion settlement and existing hosted TaskTracker/guards; do not merely remove the Agent await (checks/direct executor consumers depend on it), extend deadlines, spawn unowned cleanup, or introduce a shutdown/Drop policy. This is a local cancellation-safety correction to the new owner; no replacement machinery or public decision is needed.

**Focused closure proof:** extend the existing held-work/real AgentExecutor/Engine regression with a phase that raises a typed host stop while extraction is held, observes the Agent beginning settlement *before* its authored timeout, then keeps work held across that timeout. Assert no second session, retry, or failed result until release; cover existing retry/no-retry outcomes and cancelled/no-value records. This directly targets the lost-owner phase using the same fixture; no server harness, broad suite, synthetic load or new lifecycle policy is needed.

## Recovery and proof assessment

R2 fixes the initial running-to-cleanup transition but not cancellation while cleanup is in progress. This is the same finding and selected correction boundary; nine previous closures remain accepted. The source evidence does not justify reopening global deadline, lease-loss, process restart or shutdown behavior as independent blockers. Independent validation should reconcile SYS-R2-001 against FIND-TASK-001-2.

Skipped tests/builds and broad rediscovery as assigned. Risk: an authored timeout can discard an in-progress settlement and advance while tracked host work still runs.
