# TASK-003 concurrency and admission review

Result: **FAIL**. Two proposed material findings require independent validation.

## Subject and scope

Reviewed the complete first-review range `e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355...384a0def73ed6eac4041dc1e428dc30e981514fe`, with no carried-forward review conclusions. HEAD remains the candidate; the inspected implementation has no working-tree changes, and every entry in `authority-sha256.txt` verified successfully.

This review owns concurrency/admission only: shared resource ownership, execution/fanout ceilings, permit waits, readiness, invocation accounting, Busy deferral, cancellation and speculative tasks. I read no other discovery report and ran no tests, Cargo or source mutations. CodeGraph was used before locating implementation symbols.

Authorities: supplied root `AGENTS.md`, SQL directory rules for the cancellation migration, `changes/spec-driven-development.md`, `agent_docs/index.md`, `agent_docs/maintainer-style.md`, the task-review skill, `architecture/wyrd-design.md:1844-1870`, approved revision-22 TASK-003, and spec REQ-008 (`spec.md:313-330`), REQ-051 (`:975-994`), AC-006 (`:1231`). The revision-22 correction expressly permits competing tenants to wait while shared capacity is occupied. No per-tenant Agent reservation/quota or cluster quota is required or recommended.

## Owner and consumer tracing

* `HostedBlocks::engine` builds the per-execution engine with `STEP_CONCURRENCY` and the process's shared workspace/sandbox owners (`hosted/blocks.rs:185-203`). Engine cloning shares its semaphore; each `run` adds its own lowering semaphore and cumulative Agent budget (`engine.rs:700-742`).
* `drive` releases ready nodes from the ledger (`:881-976`); `step` freezes/prepares expansion (`:1133-1218`); `members` owns fanout futures and its optional lowering semaphore (`:1229-1276`); `member` sequences action, instance requirements, then repeat control (`:1287-1406`).
* Actions use `attempt -> admit -> spend -> begin -> executor -> commit`; requirements use `requirement -> spend -> begin -> judge -> admit -> executor -> commit`. The differing order is material below. `agent_attempts` restores recorded highest attempts into the budget (`:458-484`). Both whole-Verifier requirements and each fanout/repeat member use the same requirement producer.
* `Workspaces::open` uses nonblocking process-wide admission before mounting; the permit follows the workspace through invocation drop (`hosted/workspace.rs:79-96`, `:174-189`, `:303-317`). `AgentExecutor::invoke` keeps the session across the entire model/tool loop, races its refusal signal against the loop and drops the session before returning typed Busy (`blocks/agent.rs:335-425`). No replacement Agent loop is introduced.
* All three `Tasks` consumers are `drive`, `members`, and `instance_requirements`. The first two call `settled`, which drains on Busy; the third propagates `result?`. `Tasks::cancel` drops futures and `Tasks::drain` polls the retained futures (`walk.rs:119-165`). Nested ownership means outer draining cannot recover futures already dropped by the inner consumer.
* The queued runner maps Busy to `Deferred`, not a charged queue retry (`runner.rs:1683-1714`). Recovery reconstructs recorded invocations; therefore a dropped or prematurely recorded check affects both later dispatch eligibility and cumulative budgets.
* Committed running-to-cancelled SQL transitions emit a notification in migration `20261108000100_verifier_execution_cancel_notify.sql:15-27`. `LeaseRenewal::run` renews on the notification (`leases.rs:170-195`); missing fencing tokens cancel held work (`:216-234`); the runner's biased select drops execution on lost lease (`runner.rs:1363-1373`). The ordinary timer remains the listener-outage fallback; this review makes no new outage-latency guarantee.

## Acceptance and proof matrix

Evidence below is inspected source/assertions, not a claim that I reran the recorded tests.

| Obligation | Implementation evidence | Verification assertions inspected | Result |
|---|---|---|---|
| O2 readiness: dependents release without unrelated stage barrier; any-success waits for full predecessor settlement | `drive`, `ready`, `complete` (`engine.rs:881-976`, `:1920-1965`) | Readiness implementation consumes individual settlements; pending speculation is explicitly incomplete | PASS, static |
| O9.1 shared process and execution ceilings, fanout only lowers | `admit` (`:493-505`), per-run semaphore (`:729-731`), member semaphore (`:1239-1260`); shared workspace `try_acquire_owned` | `run_and_fanout_bounds_only_lower_engine_capacity` (`engine/tests.rs:3419-3501`) asserts overlap only when every bound admits both; Rust admission journey (`verifier_orchestration.rs:2170-2185`) asserts neither deferred item nor tenant B starts beside two holders | PASS for active ceilings |
| O-L35 a run-capacity waiter holds no shared engine permit | `admit` takes run capacity before shared capacity | `waiting_blocks_release_unrelated_permits` (`engine/tests.rs:3510-3539`) joins two runs and asserts both finish despite one run's blocked second block | PASS |
| O9.2 actions/checks/retries spend a cumulative Agent budget; recovery does not replenish actual attempts | Atomic budget; restored Agent attempts; action retry loop (`engine.rs:305-321`, `:458-484`, `:1563-1624`) | `agent_budget_counts_retries_and_is_never_replenished` (`engine/tests.rs:3559-3683`) checks retry spend, Agent checks, code independence and restored spend | FAIL: waiting checks are recorded/charged before admission (CONC-002) |
| Busy admission forgets the refused attempt and preserves already-dispatched sibling work for recovery | Action/requirement Busy paths forget; `settled` drains; **instance requirement consumer omits it** | `capacity_refusal_stops_the_execution_without_judging` (`:1674-1746`) covers one check; `capacity_refusal_lets_dispatched_work_settle` (`:1757-1810`) asserts action/member settlements and reclaim-only-refused-item, but not multiple checks of one member | FAIL: CONC-001 |
| O9.3 original timeout/deadline, cancellation releases computation capacity | Runner computes original remaining deadline and subtracts execution age (`runner.rs:1348-1360`); notification/lease-loss path above | Rust admission journey (`verifier_orchestration.rs:2239-2291`) asserts CLI cancelled status, tenant B completes after cancellation, and stalled limited execution is timed out before deployment deadline | PASS within inspected path |
| O9.4 competing tenant progresses after shared capacity release, with no reservation | Shared workspace admission and existing deferred queue transition | Rust admission journey (`:2182-2218`, `:2260-2271`) proves post-release progress and attempt 1 for refused items; cross-tenant read refusal also asserted | PASS |
| O9.5 acceptance-time limits snapshot | Revision 22 explicitly retires this obligation | N/A | RETIRED |
| Pending speculative work obeys ordinary capacity; false/blocked disposition drops unfinished calls and prevents dependent release | `drive` guard loop drops the keyed future before disposition (`engine.rs:893-936`); `complete` refuses Pending; `member` reuses the ordinary attempt/check path | `speculative_steps_withhold_content_until_admitted` (`engine/tests.rs:3236-3389`) inspects excluded outcomes, recorded dispositions and no resumed action calls | PASS for cancellation/eligibility path; check admission defect still applies |
| Agent tool Busy halts the invocation and drops pending tool calls | `AgentExecutor::invoke` refusal race/session drop (`blocks/agent.rs:403-425`); existing hosted code tool propagates typed Busy (`hosted/tools.rs:383-411`) | Static producer-to-loop-to-engine tracing | PASS for inspected shared-loop seam |

## Proposed findings

### CONC-001 — A Busy requirement drops its already-dispatched sibling checks

Classification: **INCORRECT**. Location: `crates/vala/vala-verify/src/blocks/engine.rs:1441-1444` (`instance_requirements`).

Obligation: REQ-008 capacity refusal defers without consuming attempts; TASK-003 O9.1/O9.2 and the already-implemented Busy preservation behavior let dispatched work settle and checkpoint before deferring. AC-006 requires accepted work to resume with correct attempt/budget identity. The explicit implementation contract of `drive` also says running blocks settle on Busy (`engine.rs:873-877`).

Reachable supported path: a normal step succeeds and has two independent Agent/code requirements. The first check enters the hosted executor and awaits a provider or code result. The second is refused workspace/sandbox capacity and returns Busy. `requirement` forgets the refused check (`:1503-1505`), but `instance_requirements` immediately returns through `result?`, dropping its local `running` set and the first check's future. Its begun attempt remains unsettled. The Busy result subsequently reaches `members` and `drive`; their drains cannot access this dropped inner set. Queued recovery reuses the step action but sees the first check as an interrupted recorded attempt and dispatches it again, spending another cumulative Agent invocation. A constrained budget can therefore prevent a previously dispatched sibling from completing even after capacity frees, or produce an inconclusive result solely because unrelated capacity admission failed.

Sibling audit: `drive` (`:970-971`) and `members` (`:1268-1270`) already call the shared `settled` owner; this third Tasks consumer is the omission. Single-check Busy tests and action-fanout drain tests do not cover this path.

Smallest correction: apply the existing Busy-aware `settled` behavior at this inner requirement result-consumption boundary so active sibling check futures checkpoint before propagation. Preserve ordinary non-Busy error/cancellation behavior and all outer ownership. No new scheduler, store, resource quota or retry policy is needed.

Focused closure proof: use one settled step with a delayed first requirement and a second Busy requirement; assert the first requirement settlement is retained before Busy returns, the refused requirement has no retained attempt, and a reclaimed execution invokes only the refused check. Check the cumulative Agent budget is not charged again for the retained sibling. A focused engine test at the existing checkpoint seam suffices; no new journey harness is needed.

### CONC-002 — A check spends and checkpoints its attempt while still waiting for an execution permit

Classification: **INCORRECT**. Location: `crates/vala/vala-verify/src/blocks/engine.rs:1489-1499`, with the admission sink at `:1659` (`requirement` / `judge`).

Obligation: REQ-008 resource/retry accounting and TASK-003 scenario 1/O-L65 require no pre-dispatch spend during capacity waits; original cumulative budgets must represent attempts rather than queue waiters. The existing action path establishes the available ordering: `attempt` obtains capacity at `:1563` before `spend`/`begin` at `:1567-1577`.

Reachable supported path: a queued Verifier has `limits.concurrency: 1`, `max_agent_invocations: 1`, and two whole-Verifier requirements declared in order: a slow code check and an Agent check. There need be no step action. The code check acquires the only run permit. The Agent requirement then spends its sole Agent budget and commits `begin` before `judge` waits for the occupied permit. No Agent executor or model call has run. Interrupt/restart while the code check holds capacity. `with_checkpoints` restores the never-dispatched Agent check as attempt 1 (`:380-392`), `agent_attempts` initializes spent to 1 (`:458-484`, `:725-727`), and the recovered requirement immediately reports budget spent (`:1491-1492`). Thus this Agent check can permanently become inconclusive without ever running once, even after capacity is free. This is not the unavoidable narrow checkpoint-before-dispatch crash window: the candidate deliberately records before an arbitrarily long capacity wait.

Sibling audit: the same requirement closure serves whole-Verifier and per-member/per-iteration requirements; fixing either caller alone leaves the other affected. Action admission already has the correct order. `judge` has this one production caller, so the correction belongs at the shared check invocation/admission boundary rather than in recovery's cumulative accounting.

Smallest correction: admit the check before spending its Agent budget or recording `begin`, reusing `admit` and retaining the existing shared/run permit order. Preserve the necessary checkpoint-before-external-dispatch rule, settled-check reuse, Busy forgetting, and cumulative accounting of genuinely started calls. Do not refund all interrupted recorded attempts or add a second budget owner.

Focused closure proof: hold a code check in a single-permit execution, let a sibling Agent check reach its capacity wait, and inspect checkpoint/call records: the waiter has neither a begun invocation nor an Agent call. Interrupt and recover that state with an invocation ceiling of one; assert the Agent check can run exactly once after capacity is available, rather than returning budget spent. This uses the existing engine checkpoint fixture and controlled synchronization.

## Calibration and limits

I considered eager count/member future allocation and polling pending futures during Busy drain. I did not retain a separate blocker: schema acceptance or an unusually large authored count alone does not establish the approved deployment/item-bound correction, and imposing an invented quota would strengthen revision 22. No fairness or tenant reservation is prescribed. The two retained findings have ordinary multi-check/recovery paths and reuse existing local owners.

Static review only; no tests or Cargo were run. Recorded assertions cover active ceilings and healthy cancellation, but do not close the two check-specific admission/recovery gaps above.
