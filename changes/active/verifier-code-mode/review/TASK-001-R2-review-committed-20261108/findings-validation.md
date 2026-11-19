# Independent R2 findings validation

Recommendation: **FIX_REQUIRED**. Retain **FIND-TASK-001-2** as one incomplete closure, narrowed to cancellation while the Agent is already awaiting settlement. Nine prior findings remain closed; FIND-TASK-001-7 remains withdrawn. No new finding is introduced.

## Immutable subject and method

Original base `5234961d51798581a026834cfffd5efaadc226b4`; prior candidate `f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18`; runtime candidate `66f4d54ca14d9da0257aa6a72239687e7abef004`; cumulative candidate/checked HEAD `9abeaf124585cd0d726d2554965b105fc0c0ef86`. Reviewed the actual prior-to-candidate source/test diff with cumulative acceptance context. Authority is approved revision 2 read from `git show 9abeaf124:changes/active/verifier-code-mode/spec.md`, original TASK-001, R1 validated ledger/verdict and TASK-001-R2 diagnosis/evidence. Local revision 3, its snapshot and TASK-003 are excluded.

Read the task-review skill, supplied scope/comparison/verification, all three selected discovery reports, R1 validation/concurrency/verdict evidence, original standards/security/records coverage, applicable workflow and maintainer/Rust rules, and owning Agents/tools/workspaces authority. CodeGraph is absent; no nested AGENTS applies to the four changed source/test paths. Source inspection covered full ToolCalls settlement/defer and ToolSession handoff bodies, full Agent invocation and engine attempt, check/run consumers, HostedTools session producer, Invocation blocking/code, Sandbox execution/driver/timeout projection, Halt/Dispatch stop projection, queued abandonment/lease-loss consumer, and the new held-work test. Existing in-place future polling in `vala-verify/src/walk.rs:127–145` supplies a standard-library reuse example.

No builds/tests, experiments, source/index/HEAD changes or writes outside this report were performed. The seven independent passing tests are orchestrator executions in `verification.md`, not executions by this validator. No historical RED is independently claimed.

## Specialist applicability and coverage validation

All selected reports exist and cover their assigned boundaries. Independent source evidence below, rather than their agreement, supports the finding.

| Role/boundary | Validation |
| --- | --- |
| Fresh remediation ponytail discovery | Required and complete: prior closure, actual R2 diff, changed Rust docs/test proof and affected results/budget paths. |
| Lifecycle invariant specialist | Required and complete: new shared settlement crosses session, Agent await and engine timeout/retry ownership. It traces producer and both settlement consumers. |
| System resilience specialist | Required and complete: result-producing authored timeout, earlier host stop, retry/final result and unchanged whole-execution/queue cancellation owners. No component failure is escalated into shared-server shutdown. |
| Separate concurrency domain omitted | Valid for this delta. R1 `domain-review-concurrency.md` plus its validated ledger cover admission/waiter races, owned permit/writer transfer, atomic request ceiling and scratch serialization. R2 changes none of these owners or policies. The materially new shared settlement lifetime is explicitly covered by the fresh invariant specialist; retry overlap is a consequence of that same lost settlement, not a distinct semaphore/admission policy defect. |
| Separate maintainer omitted | Valid: ToolCalls, ToolSession and engine are existing owners; no replacement runtime/workflow or substantial new abstraction. Fresh closure/invariant coverage and independent full-body inspection compare the actual workflow and correction burden. |
| Broad repository specialist omitted | Valid: no public/schema/SDK/dependency/generated/SQL change in this delta. Original `standards-review.md`, resolved through prior validated ledgers, carries unchanged cross-tier coverage. Fresh changed-item inspection covers the bounded Rust documentation and fixture edits; no new mandatory rustdoc blocker established. |
| Security/tenancy and persistent evidence carried | Original `domain-review-security.md` and `domain-review-records.md`, reconciled in R1 validation, cover accepted authority, protected tool/capability dispatch, tenant selectors, sensitive result storage/readback, additive historical contracts and SDK projection. Those producers/consumers are unchanged. R2 touches the time at which the existing ToolCalls records are consumed, reviewed freshly here; it adds no permission or persistence boundary. |

No required report is missing. No follow-up is required: the proposals share one traced source and scope, with no material unresolved conflict. No independently discovered new blocker requires separate validation.

## Reconciled acceptance and closure matrix

| Obligation/prior ID | Implementation and verification evidence | Result/disposition |
| --- | --- | --- |
| FIND-2, REQ-012/AC-009: authored timeout while Agent is still running must settle started work before retry/result | Immediate session handoff to ToolCalls; engine await outside attempt timeout. New real engine/Agent held-work test covers retry/no-retry and cancelled `(None, 0)` evidence. Independent focused execution passes. | PASS for this phase only |
| Same FIND-2 obligation when Agent has already begun settling after earlier host stop | ToolCalls pops before await; timeout drops the Agent's popped future; engine then sees empty list. Earlier Python operation deadline is a supported concrete producer of this phase. | FAIL; sole retained residual |
| Normal/check/direct Agent settlement | Agent awaits settlement before session drop/error/result; check consumer has no substitute outer attempt settlement. Uncancelled behavior remains intact. | PASS, with preceding cancellation exception |
| FIND-1: typed host stop ends invocation | Invocation::code → host_stop → Halt::observe → Agent halt selection/error retained. R2 does not change projection or later-dispatch guards. R1 closure/proof carries. | CONFIRMED closed |
| FIND-3: coordination obeys operation time/budget | Writer/mount preparation timeout and Reservation unchanged; independent held-writer/deadline test passes. Started-work settlement stays consolidated in FIND-2. | CONFIRMED closed |
| FIND-4: admitted waiter records | Dispatch starts slot before permit wait; ToolCalls records unchanged. Independent waiter regression passes. | CONFIRMED closed |
| FIND-5: actual delivery evidence | Delivery/Sandbox publication boundary unchanged; R1 deterministic completed-unpublished and actual-worker evidence carries. | CONFIRMED closed |
| FIND-6: no dispatch after stop | Halt-select and permit recheck unchanged; independent admitted-waiter regression passes. | CONFIRMED closed |
| FIND-7 | Prior rejection/withdrawal remains; no new abstraction evidence reopens it. | REJECTED; reserved ID, no work |
| FIND-8: mandatory Rust docs | Prior repairs remain. New settlement state/methods/handoff, changed Agent/engine behavior, HeldWork signal and new fixture/test document their purpose/invariants. No separate missing-rustdoc finding established. | CONFIRMED closed |
| FIND-9: recovery proof executes intended program | Prior FIFO fix and successful CodeRun/FsRead assertions untouched. No dependency change invalidates R1 proof. | CONFIRMED closed |
| FIND-10: exact JSON integer domain | Monty conversion and actual u64 endpoint proof untouched. | CONFIRMED closed |
| FIND-11: exact QueueFull Busy | Exact catalog identity and ordinary-resource-error control unchanged; engine Busy classification stays after settlement. R1 producer/consumer/queue composition carries. | CONFIRMED closed |
| Request budget/records across retries and failed/inconclusive results | Same engine-owned ToolCalls outside retry loop, same Dispatch/Delivery writers and check result consumer. Independent budget and failed-record regressions pass. Lost settlement affects progression, not a new record-field defect. | PASS except shared FIND-2 progression |
| Owned permit/writer/workspace; shared scratch/cap-1 semantics | Invocation::blocking retains work-owned guards; existing Workspace owner unchanged. Independent permit and hosted extraction guard tests pass. Joining remains separately incomplete. | PASS for retention; FIND-2 controls attempt overlap |
| Remaining original accepted obligations/non-goals | Source-backed original/R1 matrices carry authority/tenant/audit/catalog, public reduction/citations, wire/historical/custom-data/privacy, budgets/limits/portability and owner reuse. Delta adds no dependency, service, runtime, transport, journal, public contract or TASK-002/003 behavior. | PASS, carried |
| Whole-execution/no-result/queue cancellation preservation | Engine::run outer deadline and runner abandon/lease-loss select remain. They can drop the whole engine without producing a partial result. R2 adds no new drain/shutdown or result-after-abandonment guarantee. | PASS, scoped preservation |

## Final deduplicated ledger

### FIND-TASK-001-2 — An authored timeout can discard settlement already being awaited by the Agent

**Discovery sources:** R2-CLOSURE-1, INV-R2-001, SYS-R2-001, all continuing prior FIND-TASK-001-2. **Validation:** REVISED, same stable ID. **Classification:** INCORRECT / incomplete cleanup correction.

**Obligation:** approved revision-2 REQ-012/AC-009, original task cleanup requirement and R2's explicit settlement-before-result/retry requirement. Started bounded extraction may complete during cleanup but cannot outlive completion of that cleanup. Retaining its resources is necessary but does not prove joining it before progression.

**Exact location:** `crates/vala/vala-verify/src/blocks/tools.rs:170–180` (remove-before-await), `blocks/agent.rs:413–426` (inner consumer), `blocks/engine.rs:675–701` (timeout/drop and outer consumer).

**Independent producer-to-consumer proof:**

1. `HostedTools::open` (`hosted/tools.rs:139–179`) is the production settlement producer. Its existing TaskTracker future closes/waits the session's started work. `Invocation::blocking` (`:303–317`) runs extraction in tracked blocking work while retaining actual writer/permit; the extraction closure retains Workspace. Dropping the asynchronous call does not stop already-started blocking work.
2. Generated `code.run` can await that extraction. `Invocation::code` (`:337–372`) uses the earlier operation/Python-budget deadline and passes it to Sandbox. `Sandbox::exec` (`sandbox.rs:227–244,395–405`) times its driver, drops pending bridge calls and maps expiry to terminal Python time-limit failure. The configurable code-operation timeout (`config.rs:1817–1822`) may be earlier than the authored attempt timeout. Bounded extraction is not guaranteed to complete before either timeout.
3. That earlier failure keeps its typed host_stop cause, is observed by Dispatch/Halt (`blocks/tools.rs:487–507,648–651`), and ends the Agent loop. Agent invocation then enters `calls.settle().await` before the authored timeout. No arbitrary external Drop, shutdown or unsupported consumer is needed to reach this state.
4. ToolCalls removes the sole boxed settlement from its surviving state before awaiting it. While it is Pending, only the Agent's inner settle future owns it. The authored engine timeout then drops `actions.execute`, including this popped pending future.
5. The engine's newly added `tool_calls.settle().await` still has ToolCalls, but its settlement vector is empty. It returns immediately; existing retryable timeout policy starts another session or produces Attempted while old extraction remains active. Both `settle` callers and the sole `settle_with` caller were traced; no other production settlement owner repairs this loss.

**Consequence/materiality:** a supported timed-out attempt can report completion before bounded work ends or open a new session with fresh semaphore/workspace admission while old work retains the prior permit/workspace. Guards prevent early deletion/release but cannot establish required settlement-before-progression. This is the same explicit result-producing lifecycle obligation retained by R1, not a newly broadened cancellation policy.

**Original correction, superseded by the maintainer-directed amendment below:** keep each unfinished boxed settlement in the existing ToolCalls pending state until it resolves, and poll/await it without transferring sole ownership into the cancellable waiter. An interrupted Agent waiter must leave the same future resumable by the existing outer engine waiter. Reuse standard-library in-place future polling and the existing state mutex; `vala-verify/src/walk.rs:127–145` already polls retained boxed futures and removes them only when Ready. Private helper/control-flow shape stays implementation-owned. No new dependency or framework is needed; the mutex must not be held across an asynchronous wait or while running blocking host work.

Preserve synchronous session handoff, normal Agent settlement and check/direct consumers, outer engine timeout classification/retry count/final failed-step/inconclusive-check behavior, engine-owned request/record state, hosted retry-spanning Python budget, exact Busy deferral, ordinary catchable domain errors, actual work-owned guards, no late delivered value/digest, scratch limits/no-overwrite, query cancel-on-drop and sibling service availability. Removing only the Agent wait would abandon normal check/direct settlement; moving waits into every consumer duplicates ownership. Deletion of the shared pending owner reintroduces the original timeout defect. Longer deadlines, downstream result guards and detached cleanup do not fix lost ownership.

**Burden:** one local pending-future ownership correction plus extending the existing deterministic engine fixture. Existing TaskTracker, ToolCalls state and test start/release/settlement signals suffice. This requires no new public, persistent, concurrency-policy or architecture decision, so SPEC_REVISION_REQUIRED is unwarranted.

**Focused closure proof:** use the existing real engine/Agent/HeldWork seam with an ordinary typed host stop or completion that cancels the child waiter and causes Agent settlement before authored timeout. Observe that inner settlement is polled; hold work through the later authored timeout and assert no retry/new session/result until release. Then retain retry/no-retry outcomes, cancellation/no-value records and recovered admission assertions. Keep the current timeout-during-execution phase. Synchronization should establish phase order; no added harness, synthetic load, test quota or broad journey restart is justified. Reuse the seven focused selectors already recorded in `verification.md`, adding only a directly affected selector if the actual correction changes another consumer.

**Rejected expansion:** no separate arbitrary-Drop, whole-engine no-result timeout, lease-loss, abandonment or shutdown-drain blocker. These unchanged owners retain their existing semantics. No async Drop, blocking runtime-thread join, cleanup service, journal, process termination, rollback or new resource policy is authorized by this finding.

## Verification, convergence and recommendation

Seven independently rerun focused tests pass: five Vala lifecycle/permit/waiter/budget/records tests and two hosted extraction/deadline owner tests, as recorded with exact selectors/run IDs in `verification.md`. The new timeout test waits on HeldWork as its only tool operation, preventing Agent invocation from reaching its own settlement before timeout. Its first observed settlement poll is therefore the safe outer engine await, and it releases work immediately then. Its passing assertions do not exercise cancellation of a previously polled inner settlement. This is a precise missing phase in the existing required proof, not an exhaustive-input demand. Broader implementation GREEN/RED and format/lint evidence remains reported evidence.

This is the second unsuccessful remediation assessment. R1 repaired work-owned guards and ordinary joining but left the attempt timeout able to discard the join. R2 correctly transfers pending settlement into ToolCalls while execution is active, then temporarily transfers it back into the same cancellable Agent future when joining starts. The failure is incomplete correction of one ownership cause; the missed inner-await phase explains the still-green regression. It is neither a new defect family nor expanding expectations. The next bounded round should check retained-until-complete settlement and both cancellation phases, normal/check/direct consumers and preserved timeout/result/budget behavior, carrying the nine closed findings and other accepted evidence forward.

Recommend **FIX_REQUIRED**, one bounded remediation for **FIND-TASK-001-2**. No further discovery or new specialist role is needed for this diagnosis.

Skipped builds/tests and broad rediscovery as assigned. Risk: authored timeout during Agent cleanup can still advance while prior bounded host work runs.

## Maintainer-directed recommendation supersession

The finding/source trace and closures above stand. The selected retained-future
polling recommendation is superseded by [recommendation-amendment.md](recommendation-amendment.md)
and revised TASK-001-R3 after the maintainer rejected duplicate draining
capability. A bounded source reassessment by this validator confirmed exclusive
engine/caller cleanup ownership with registration-only executor handles,
including engine checks and direct fixture consumers. This introduces no new
finding and changes no approved behavior. No implementation/test result is claimed.
