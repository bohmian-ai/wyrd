# Concurrency/resource boundary review

**Result: FAIL.** Independent static review of base `5234961d51798581a026834cfffd5efaadc226b4` → candidate `0e51af2a2332f362ab44b7c5b88b51c3cb4f8011`. HEAD remained candidate when checked. No source/spec/task/test edits, Cargo commands, or subagents.

## Boundary, authority and coverage

Reviewed shared request accounting, non-code admission, Python time reservations, pending futures, workspace mutation and blocking-work cancellation. Authority: root AGENTS.md, task-review skill, spec-driven-development and maintainer guides, approved revision-2 spec (admitted-call definition, REQ-010–013/015), TASK-001 scenarios 3–4, architecture/wyrd-design.md Agents/tools/workspaces. No .codegraph directory or nested AGENTS.md exists in inspected owner trees.

Inspected full Vala blocks/tools.rs, AgentExecutor invocation, engine attempt/check settlement; full hosted tools.rs/sandbox.rs; Workspace admission/writer/mounts/room/extraction/publication; Skald tool buffering/result ordering; relevant Vala/hosted integration tests. Existing owners are ToolSession/registry, hosted Invocation, Workspace writer, Sandbox.

| Obligation | Implementation evidence | Verification evidence inspected | Result |
| --- | --- | --- | --- |
| Atomic shared ceiling across retries; no refund; final allowance succeeds | ToolCalls::spend atomic try_update; RecordedTool/ToolBridge spend; engine log outside retry loop | agent_tool_budget_spans_attempts_without_refund; agent_tool_budget_spans_origins_and_retries | PASS ceiling/retry ownership |
| No later tool starts after stop | Halt checked only before possible permit wait | Sequential exhaustion and pending-query Busy tests lack post-stop permit release | FAIL CON-002 |
| Shared default-8 cap, cap 1 completes; code holds no child permit | One Dispatch semaphore, code excluded; Skald buffer_unordered(cap) | code_bridge_shares_the_recorded_path observes nested peak 1/2; actual-worker code_calls_share_non_code_concurrency | PASS normally; CON-001 exception |
| Future association and flat ordering | call_id retained through resume_futures; locked append/fixed-slot finish | Hosted read/query gather checks association and order | PASS executed calls; waiter omission CON-004 |
| Retry-spanning time never overbooked; waits bounded | Reservation subtracts before await, refunds unused elapsed | reservations_never_overbook_the_budget; hosted query-wait/retry test | PASS math; deadline FAIL CON-003 |
| Shared workspace, no recursive lock, no-clobber, shared scratch | Per-extract/per-Python-write writer; fresh work_room; persist_noclobber | code_calls_share_workspace_mutations; workspace_writes_share_one_budget | PASS normally; interruption FAIL CON-001 |
| Cleanup settles extraction, prevents late publication | Async caller owns writer; closure owns Workspace Arc only | Required extraction interruption test absent; task substitutes Arc claim | FAIL CON-001 |
| Admitted cancellation retained | Record starts after permit acquire | Cancellation test covers active query only | FAIL CON-004 |
| Query cancellation/worker shutdown | CancellationToken drop guard; sandbox owns futures; scoped checkout/admission | code_host_waits_obey_deadlines_and_cancellation; code_shutdown_settles_pending_host_work | PASS inspected paths; not extraction proof |

## Findings

### CON-001 — INCORRECT: interrupted extraction detaches and loses mutation/admission guards

**Obligation:** REQ-011–013/TASK-001 require started extraction settlement before cleanup returns, no late publication, serialized writes and bounded active non-code operations.

**Location:** `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:263–277`; related `crates/vala/vala-verify/src/blocks/tools.rs:502–535`, hosted `sandbox.rs:242–388`, `workspace.rs:675–765`.

The writer guard is local to async Invocation::call; the blocking closure captures Workspace/arguments but not the guard. Sandbox timeout or Agent cancellation drops the nested call/JoinHandle. A started spawn_blocking closure continues while writer and Dispatch permit release. Workspace::extract can subsequently measure work_room, stage output and persist_noclobber into /work after its caller ends. The Arc retains directory/workspace admission; it neither awaits cleanup nor retains writer/non-code admission.

Reachable case: interrupt nested extraction after its closure starts. Per-code timeout currently returns an ordinary tool error (Invocation::code:328–334), so later direct extraction or code in the same Agent session can acquire the released writer while detached extraction continues. Concurrent room checks/publication with different paths can exceed scratch/write ceilings. Whole-invocation cancellation also returns while blocking work/workspace capacity remain held, contrary to explicit cleanup requirements.

**Minimum correction:** extend existing hosted blocking-operation/session cleanup ownership so started extraction retains mutation and active-operation reservations through completion, cleanup awaits completion, and cancellation prevents late publication/delivery. Moving only writer into closure fixes early unlock but not awaited cleanup; aborting a started JoinHandle is insufficient. No second workspace, rollback or new extraction behavior.

**Proof:** already-required `code_extraction_settles_before_workspace_release`, with deterministic start/settlement barriers. Interrupt after start; cleanup must wait, competing writer cannot enter early, no publication follows cleanup, capacity returns after settlement, next invocation succeeds. Cover per-code timeout/same-session continuation or equivalent ownership observations. Normal extraction proof does not establish this.

### CON-002 — INCORRECT: pre-spent permit waiter can start after stop

**Obligation:** REQ-009–010/TASK-001 prohibit new tool work after exhaustion refusal.

**Location:** `crates/vala/vala-verify/src/blocks/tools.rs:453–464,502–534`; producer/driver hosted `sandbox.rs:435–477`; observer `blocks/agent.rs:410–420`.

spend checks Halt before acquisition. A spent permit waiter never rechecks Halt before recording/invoking. Outer Agent select is cooperative: it cannot interrupt an in-progress poll of its run future. Sandbox drive can poll multiple children inside that poll.

Reachable ordering at cap 1/allowance 3: outer code spends 1; A spends 2/occupies permit; B spends 3/waits. Worker emits malformed fourth bridge request. request immediately calls bridge.refuse while constructing its ready future (475–477), raising exhaustion synchronously. While next worker IPC turn is pending, drive polls children: A completes/releases permit, then drive loops and polls B, which acquires/invokes despite Halt. Only when run future yields can outer Agent select observe stop. Ceiling remains numerically correct, but another protected query/read/extraction and audit can start after refusal. This is within one cooperative poll, not dependent on parallel threads.

**Minimum correction:** enforce existing Halt at shared execution boundary after permit acquisition and before actual invocation, without another budget charge. Stop waiting on Halt as appropriate; preserve final-allowance success/Busy identity. Biasing outer select cannot interrupt current inner poll. Preserve already-admitted cancellation records under CON-004.

**Proof:** deterministic cap-1 active A/waiting B; raise exhaustion through fourth malformed request, then release A before yielding to outer selection. Assert B never invokes, no further model turn, spending capped. Existing sequential exhaustion/pending-query Busy tests miss this ordering.

### CON-003 — MISSING: initial workspace wait is outside Python deadline

**Obligation:** REQ-012/TASK-001 include workspace coordination in operation/reserved cumulative wall-clock bounds.

**Location:** `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:302–327`.

Reservation establishes deadline before coordination, but writer().await and mount construction precede Sandbox::exec, where timeout_at first enforces deadline. Direct extraction holding writer can keep concurrent direct code.run waiting beyond reserved deadline; expiration is discovered only after lock release/mount construction. A positive code timeout shorter than bounded extraction duration and a later Agent deadline are supported. Charging elapsed time does not bound the operation/resources.

**Minimum correction:** apply computed deadline to initial workspace wait/setup with existing timeout mechanism. An unstarted waiter must expire on time; reconcile started blocking setup with CON-001 settlement rather than detaching it. No new setting.

**Proof:** hold Workspace writer; start code.run with short operation budget/later Agent deadline; pass operation deadline; it must end without releasing held writer or entering worker. Retry must not restore elapsed time. Current host-wait test begins inside exec.

### CON-004 — MISSING: admitted permit waiters disappear on cancellation

**Obligation:** spec lines 69–71 define admission by successful local name/argument checks and common path entry; REQ-015 retains every admitted cancellation when a result is produced.

**Location:** `crates/vala/vala-verify/src/blocks/tools.rs:491–519`; consumers ToolCalls::settled and engine action/check settlement.

A valid request spends budget, resolves declaration, passes object/byte checks, then waits semaphore. Record slot exists only after acquisition. At cap 1 gather two valid nested calls while first stalls, then timeout or raise Busy/exhaustion. Second passed admission boundary but drops without slot. Produced result omits this admitted cancellation; settled only synthesizes cancellation for existing slots. This loses explicitly required accountability for supported concurrent gathers.

**Minimum correction:** create bounded record at admission after target/argument validation and before permit wait; unfinished slot remains cancellation if no value delivered. Preserve no-record pre-admission rejection and flat admission-start ordering. Reuse ToolCalls, no new journal; coordinate with CON-002 post-wait stop check.

**Proof:** cap-1 active-call barrier and second valid request confirmed waiting; interrupt/produce result. Both children plus outer code appear once in start order; waiter cancellation/no digest/zero bytes. Verify failed-step/inconclusive-check projections where applicable.

## Verification limits

No tests executed per assignment. Inspected test bodies/task command evidence are distinct from independent execution. Actual-worker gathers establish normal association/nested caps, not mixed-origin observed peaks, interrupted extraction or post-stop permit release. Reservation unit proof directly tests arithmetic. Findings require independent validation, especially exact cooperative-poll ordering in CON-002.

Skipped unrelated contracts and execution. Risk: extraction lifetime, stop dispatch and cancellation records remain proposed source-backed gaps pending independent validation.
