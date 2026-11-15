# R2 bounded closure discovery

**FAIL — FIND-TASK-001-2 remains open.** The new outer settlement handles cancellation before the Agent begins settling, but the same authored timeout can cancel the Agent's in-progress settlement after it has removed the only join future from engine-owned state. No new finding ID is proposed.

## Subject and method

Original base `5234961d51798581a026834cfffd5efaadc226b4`; prior candidate `f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18`; candidate/checked HEAD `9abeaf124585cd0d726d2554965b105fc0c0ef86` (runtime `66f4d54ca14d9da0257aa6a72239687e7abef004`). Read the actual prior-to-candidate diff, pinned revision 2 via Git, original TASK-001, R1 validated ledger/verdict, R2 diagnosis and appended evidence, scope map, task-review skill, repository instructions, workflow, Rust/maintainer guides, and owning Verifier authority. CodeGraph is absent; no nested AGENTS applies to the four source/test paths.

This is fresh discovery of sole remaining closure and affected paths. Nine source-backed R1 closures carry forward; ID 7 remains withdrawn. Local revision 3 and TASK-003 do not govern this review. Only this report was written. No tests/builds or source/index/HEAD mutations were performed.

## Acceptance matrix

| Obligation | Source/proof assessment | Result |
| --- | --- | --- |
| FIND-2; REQ-012/AC-009: started work settles before result/retry | Session registers its existing wait into ToolCalls at Agent open; engine awaits after timeout. This works only while the wait remains in ToolCalls. Inner Agent settlement removes it before awaiting, leaving cancellation unprotected. | FAIL |
| Old/new attempts do not overlap; cancelled/no-value records | New test genuinely uses real AgentExecutor/Engine, held blocking work, retry/no-retry cases and cancelled `(None, 0)` record. It covers timeout during tool execution, not timeout during the inner settlement wait. Residual path allows overlapping attempts despite preserved records. | FAIL under same FIND-2 |
| Mandatory changed-item rustdoc | ToolCalls/CallLog pending state, Settlement alias, settle/defer, ToolSession handoff, Agent invoke, engine attempt, HeldWork signal, extract_call, SettlingToolset fields/open and regression have applicable role/invariant/error/panic docs. No separate doc blocker found. | PASS |
| Existing owner reuse / scope | Extends existing engine-owned ToolCalls and ToolSession, preserving HostedTools TaskTracker; no replacement service, dependency, feature, wire/API/storage/SDK or broader lifecycle policy. | PASS |
| Prior 1: host-stop ends invocation | Existing typed host_stop → Halt::observe → invoke error path unchanged; affected code still returns halted error after normal settlement. Cleanup cancellation defect belongs to 2. | Carry closed |
| Prior 3: coordination deadline | Invocation::code writer/mount timeout and budget reservation unchanged. Started-work settlement remains consolidated under 2. | Carry closed |
| Prior 4–6: admitted records, actual delivery, no post-halt dispatch | Dispatch::call record-before-permit, Delivery publication, Halt-select/recheck and records settlement unchanged. New ToolCalls pending vector does not change request slots or arithmetic. | Carry closed |
| Prior 8: Rust docs | Prior repairs preserved; R2 items reviewed above. | Carry closed |
| Prior 9: recovery proof | FIFO consumption and successful CodeRun/FsRead assertions unchanged. | Carry closed |
| Prior 10: integer domain | Monty conversion and real u64 endpoint path unchanged. | Carry closed |
| Prior 11: exact Oracle QueueFull | Halt exact-code classification and ordinary-resource-error control unchanged; engine still maps Busy before retries when not itself timed out. | Carry closed |
| Other authority/tenant/catalog, caps/budget, workspace, result/SDK, portability and non-goal rows | R1 validated matrices carry; no changed owner invalidates those accepted contracts. Retained permit/writer/workspace guards still live with blocking closure. | PASS, carried |

## Proposed ledger

### FIND-TASK-001-2 / R2-CLOSURE-1 — settlement ownership is lost during a nested await

Classification: INCORRECT; retain prior ID and bounded REQ-012/AC-009 result/retry obligation.

**Location:** `crates/vala/vala-verify/src/blocks/tools.rs:170–180`, consumed by `blocks/agent.rs:420–423` under `blocks/engine.rs:675–686`.

**Producer-to-consumer proof:**

1. HostedTools::open (`hosted/tools.rs:173–179`) supplies the existing `work.close(); work.wait().await` future. Invocation::blocking (`:304–317`) tracks already-started bounded blocking work and retains writer/permit/workspace guards. Agent invoke hands that future to the invocation's ToolCalls immediately after open.
2. The Agent can finish its run or raise Halt while blocking work remains. A supported concrete path is generated `code.run` awaiting nested `fs.extract`: Sandbox::exec bounds its driver by its code-operation deadline (`hosted/sandbox.rs:227–244`), earlier than an authored attempt timeout; the cancelled bridge future does not stop an already-started tracked extraction. Invocation::code maps sandbox failure through host_stop; Dispatch::call observes it and Halt terminates the Agent loop. Another ordinary producer is a host-stop/Busy sibling beside a started extraction. Neither requires arbitrary caller Drop or a new shutdown contract.
3. Agent invoke then calls `calls.settle().await` **inside** the engine's timed actions.execute future. ToolCalls::settle `.pop()`s the only settlement out of shared state before `settlement.await`. While extraction is held, that inner wait stays pending.
4. The authored timeout now expires before extraction ends. `timeout_at` drops actions.execute, the inner settle future and its popped settlement. The engine's new `tool_calls.settle().await` sees an empty vector and returns immediately. Timeout becomes Retryable and starts another attempt, or produces the existing failed Attempted result, while prior work is active.

The consequence and materiality are exactly the retained finding: completed attempt cleanup/result or retry precedes already-started bounded host work, and a new session may admit work alongside the old attempt. Guard retention prevents deletion/release but does not join work. This is not a demand to await no-result whole-engine abandonment, lease loss, or shutdown.

**Smallest safe correction boundary:** keep the pending join owned/recoverable by the existing invocation/attempt lifecycle until it completes, including when the inner Agent settlement await is cancelled by its enclosing authored timeout. Reuse ToolCalls/ToolSession/TaskTracker and preserve checks' normal Agent settlement and the outer engine timeout/result/retry policy. Do not replace the lifecycle or add blocking Drop, a cleanup service, or consumer record guards. A local cancellation-safe ownership correction is sufficient; this requires no new public or concurrency policy. Merely deleting the Agent await would also need preserving the check consumer, which does not have the added engine attempt await.

**Focused proof:** extend the existing held-work engine test so an ordinary host-stop or run completion starts/polls the Agent's settlement **before** authored timeout; keep work held across that timeout, and assert no second session/result until release, for retry/no-retry. Use existing start/settlement/release signals; no new server harness or sleeps. Then retain current timeout-during-tool test and existing permit/workspace/deadline/budget/record regressions. This exercises the same common cause at the missing nested-await boundary.

## Test adequacy, affected callers and limits

The added regression has meaningful consumer-level assertions, deterministic work start/release, real engine retry counts, no-retry timeout classification and cancelled/no-delivery evidence. Its HeldWork never returns while held and no other tool stops the session, so Agent invoke cannot reach its own settlement before timeout: the only observed settlement poll is the outer engine one. The test therefore misses the destructive ownership transfer at the inner await. The orchestrator's verification.md records seven passing independent focused tests (five Vala and two hosted guards/deadlines); these prove their actual assertions but do not exercise the missing nested-await boundary. Reported wider GREEN owner lanes do not repair that source gap; no historical RED or execution by this reviewer is claimed here.

Check invocations await via Agent invoke and have no authored attempt timeout; normal completion still waits. Resolve failure after handoff is covered by the outer attempt settle for steps; session.resolve derives entries from the already resolved declaration/host registration. Engine::run whole-execution timeout and queued runner's abandon/lease-loss select (`runner.rs:891–896`) still drop the whole engine without a manufactured result; existing safety semantics and explicit exclusions remain. No new shutdown issue is proposed.

Convergence: this is incomplete closure of the original ownership cause, not expanding expectations. R1 preserved work-owned guards but left timeout-owned settlement; R2 transfers the wait into shared state but temporarily takes it back into the same cancellable Agent future. Next review should cover this one cancellation point plus affected checks/normal/timeout callers, carrying all other closures.

Skipped test/build execution and broad rediscovery as assigned. Risk: timed-out attempt progression still overlaps prior bounded work when the timeout interrupts Agent cleanup.
