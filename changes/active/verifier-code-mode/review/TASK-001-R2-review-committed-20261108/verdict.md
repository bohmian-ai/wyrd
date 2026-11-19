# TASK-001 R2 re-review

**Verdict: FIX_REQUIRED.** **FIND-TASK-001-2** remains, narrowed to timeout
during an already-started cleanup wait. R2 fixes timeout while the Agent is
still executing, but a later timeout can discard the cleanup wait itself and
let retry/result proceed. No new finding; nine prior closures carry forward
and finding 7 remains withdrawn.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode`.
- Original base: `5234961d51798581a026834cfffd5efaadc226b4`.
- Prior candidate: `f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18`.
- Runtime candidate: `66f4d54ca14d9da0257aa6a72239687e7abef004`.
- Cumulative candidate/HEAD: `9abeaf124585cd0d726d2554965b105fc0c0ef86`;
  final commit records the prior review/R2 evidence, with identical runtime source.
- Delta: `git diff f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18 9abeaf124585cd0d726d2554965b105fc0c0ef86`.
  Original-base cumulative range remains acceptance context; no rebase.
- Approved revision 2 read from candidate Git blob, SHA-256
  `cc1b3c37d28201f3207769901776cc34b6a73c2810e9c42c53b50c2033cd4055`.
- Original TASK-001 SHA-256
  `494bca6fa15325ace56ffe116e45d7d4f745361c56567e5b8351af6cb9fb1521`.
- R2 with appended evidence SHA-256
  `36e712159220ff30fe403c854c6c000357c338b7492da2518483bc1edafbae5e`.

Local benchmark revision 3/snapshot/TASK-003 are pre-existing planning excluded
from this runtime review. Earlier uncommitted-subject BLOCKED report remains
intact and was not an unsuccessful remediation assessment. No source, prior
packet/evidence, index, HEAD or planning file changed during this review. New
reports and the R3 task are the only writes.

## Scope, selection and independent results

[scope.md](scope.md) records shared navigation, carry-forward and specialist
selection before discovery. Scope is FIND-2 closure, the R2 delta and affected
lifecycle consumers. The orchestrator routed inputs and tests; separate fresh
reviewers discovered and a further fresh reviewer validated findings.

| Role | Report | Result |
| --- | --- | --- |
| Remediation ponytail discovery | [Closure](task-review-closure.md) | FAIL; source-traced remaining FIND-2. |
| Lifecycle invariant specialist | [Invariants](task-review-invariants.md) | FAIL; same lost pending-settlement cause. |
| System resilience specialist | [System](system-review.md) | FAIL; supported earlier Python-timeout/Halt producer traced. |
| Fresh independent ponytail validation | [Ledger](findings-validation.md) | FIX_REQUIRED; one REVISED residual, no new finding. |

The validator independently verified selected coverage and omissions. R1's
concurrency review covers unchanged budget/admission/permit/writer boundaries;
new shared settlement ownership is freshly covered by invariant review.
Original/R1 security, persisted-record and standards evidence carries unchanged
contracts/dependencies/permission owners. New Rust docs and whole-workflow reuse
were freshly inspected. Existing owners are extended without replacement
runtime/service/abstraction, so a separate maintainer review is unnecessary.
No required report or sensitive-boundary coverage is missing.

Carry forward all nine prior closures and source-backed original/R1 accepted
rows for authority/tenancy/audit/catalog, delivery/integer/historical/custom-data/
privacy, request arithmetic/limits, workspace guards, public reduction/citations
and portable runtime/non-goals. Only settlement progression/consumers are
reopened by this fix. Source-backed matrices are in the prior and current
validated ledgers; no complete discovery restart occurred.

[comparison.md](comparison.md) consolidates three proposals into the same
finding. No follow-up was needed: no material unresolved conflict/path remained.
Agreement did not establish acceptance; the validator independently traced the
source and rejected expansion into arbitrary Drop/shutdown/no-result behavior.
No validator-discovered new blocker required separate validation.

## Reconciled acceptance

| Obligation | Reconciled source/proof | Result |
| --- | --- | --- |
| Timeout while Agent executes | Immediate session handoff to ToolCalls; outer engine settles after timeout; real engine retry/no-retry proof | PASS for this phase |
| Timeout while Agent already settles | Agent pops the wait into its timed future; timeout drops it; engine finds empty state | FAIL, FIND-TASK-001-2 |
| Normal/check/direct settlement | Existing Agent wait retained, successful when not interrupted; check consumers have no equivalent outer attempt wait | PASS except same residual |
| Guards, request/record state, failure/budget behavior | Unchanged producer owners plus focused Vala/hosted regressions; prior validated closure evidence | PASS, carried/affected paths inspected |
| Nine earlier findings; ID7 | R1 ledger closures unaffected; CodeTool withdrawal upheld | Closed; ID7 no work |
| Global no-result deadline/abandonment/lease-loss | Existing outer cancellation remains; no new result-after-abandonment or shutdown promise inferred | PASS scoped preservation |
| Mandatory docs/reuse/non-goals | Changed Rust items inspected; existing owners and standard dependency set retained | PASS |

## Validated finding and smallest correction

**FIND-TASK-001-2 — REVISED / INCORRECT.** REQ-012/AC-009 require cleanup
before result/retry. `ToolCalls::settle` (`blocks/tools.rs:170–180`) removes its
future before awaiting. AgentExecutor invokes it (`blocks/agent.rs:413–426`)
inside the engine's attempt timeout (`blocks/engine.rs:675–701`).

A supported earlier Python operation timeout can stop `code.run` while its
nested extraction still runs, raising the existing typed Halt. Agent cleanup
starts and removes the pending wait. The later authored step timeout drops
that wait; the engine's fallback sees no pending settlement and advances.
The existing green proof never begins inner cleanup before its step timeout.

The initial retained-future polling correction below that failure trace is
superseded by the [maintainer-directed root-cause amendment](recommendation-amendment.md).
Give the engine exclusive owning cleanup capability; executors can only register
work/account for calls/record evidence. Remove the Agent drain, and finish work
at both engine action and check completion boundaries before progression or
classification. Preserve explicit caller-owned completion in direct test seams.
The existing validator source-checked this bounded amendment; no new finding or
spec behavior is introduced. No second guard, polling fallback or framework is
selected. The validated failure trace and prior closures remain unchanged.

One self-contained [TASK-001-R3 remediation](TASK-001-R3-preserve-in-progress-settlement.md)
contains that bounded correction and the missing phase of the existing proof,
for direct wyrd-implement handoff. No spec revision or new policy is required.

## Convergence assessment

This is the second unsuccessful remediation review. R1 fixed resource guards
and ordinary joining but lost the join on outer timeout. R2 preserved the join
while executing but moved its sole ownership back into the timed future when
joining began. These are incomplete fixes to one ownership cause; the proof
missed timeout during joining. This is neither a new defect family nor expanded
expectations. Next review checks exclusive caller-owned cleanup capability and both timeout
cases plus action/check/direct-fixture consumers, carrying nine closures forward. Reject another
broad review restart or cleanup redesign.

## Verification and limits

[verification.md](verification.md) records seven independently rerun tests, all
passing, with exact commands/run IDs: five Vala lifecycle/permit/waiter/budget/
records and two hosted extraction/deadline tests. Cumulative whitespace passes.
The missing inner-join timeout path is independently source-proven, not claimed
as a new executed failing test. Implementation-reported RED/wider owner lanes/
format/lints/dependency checks remain reported evidence.

No broad gate, full SDK suite, platform matrix, benchmark, broken-source
experiment or source change was performed. Review does not implement, commit,
merge, push, deploy or complete TASK-001. Candidate remained unchanged.
