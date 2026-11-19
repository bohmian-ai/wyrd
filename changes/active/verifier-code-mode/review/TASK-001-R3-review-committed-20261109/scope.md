# TASK-001 R3 committed review scope

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode.
Original base 5234961d51798581a026834cfffd5efaadc226b4.
Prior candidate 9abeaf124585cd0d726d2554965b105fc0c0ef86.
Runtime candidate 6339391548ea31ac84382f4c6e7694f1405e0343.
Cumulative candidate/HEAD e5af2ce5caef2d00303e1c980b12c8a9bca4af0e.
Last commit adds R2 review/R3 evidence only; runtime source matches 633939154.
Review git diff <prior> <candidate>, original-base cumulative range available.
No rebase. Pinned approved rev2 via git show candidate:changes/active/verifier-code-mode/spec.md
(sha256 cc1b3c37d28201f3207769901776cc34b6a73c2810e9c42c53b50c2033cd4055).
Read original TASK001, first review, R1 ledger/verdict, R2 ledger/verdict,
recommendation-amendment.md and TASK-001-R3 with appended implementation evidence.
The maintainer explicitly rejected polling/second-guard correction and selected
one owning cleanup capability, executor registration-only handle, engine action
and check joining, caller-owned direct fixtures. That recorded amendment governs.
Local spec3/snapshot/TASK003 planning and previous subject-only BLOCKED reports
are excluded. Write only own reports here. No source/index/HEAD/old-doc mutations.

## Bounded scope and carry-forward

Check sole remaining FIND-TASK-001-2, R3 owner/handle API, engine action/check
result capture and settlement, typed Terminal/Busy/timeout outcomes, fixture
migrations and proof adequacy. Nine prior findings remain closed; ID7 withdrawn.
Carry original/R1/R2 source-backed accepted declaration/authority/tenant/audit,
record/delivery/integer, budget/admission/writer guards, citations/reduction,
historical/privacy/SDK/config/portable behavior and non-goals. Reopen only an
actual affected caller or new material source evidence, not full discovery.
Whole-run deadline/no-result abandonment/lease-loss/new shutdown guarantees stay
excluded; preserve their current behavior. No guard/framework/polling fallback.

## Shared navigation map

- blocks/tools.rs: new non-Clone ToolCallsOwner creates state, handle, settle,
  settled; ToolCalls now Clone only with private accounting/record/register
  methods. ToolSession::register_with transfers settlement once. Check whether
  executor handle can recover/drain owner, and whole-workflow reuse.
- blocks.rs: reexport and BlockCall handle contract, test no-session constructors.
- blocks/agent.rs invoke: immediate registration, removed child drain;
  actual result/Halt and error returns, session/workspace lifetime.
- blocks/engine.rs attempt: owner outside timed actions.execute, captures result,
  settles, then timer/retry/Busy/Attempted. judge: capture checks.execute,
  settle, then Busy/error/cited output/record handling. Inspect early exits.
- integration/blocks_agent.rs: owner-backed step_recording/check helpers;
  HeldWork/DropHeld/SettlingToolset; timeout test Direct and Stopped(Terminal/Busy)
  cases, agent_check_settles_before_result; fixture-authoring diagnoses in evidence.
- Server pg_hosted_artifacts direct call helper closures and HostedAgent call/
  step own state; pg_hosted_files and blocks.rs no-session examples use temporary
  owner handle. Trace actual constructors/callers and reason for retained owner.
- Unchanged HostedTools::open/Invocation::blocking/Workspace producer, TaskTracker
  and owned writer/permit, engine run and queue runner cancellation consumers.
- R2 review at ../TASK-001-R2-review-committed-20261108 carries last coverage,
  nine closures, failure trace, root-cause amendment and proof commands.

## Applicability and reviewers (before discovery)

| Contract/invariant and owner/consumer | Selection / omission evidence |
| --- | --- |
| FIND2 closure, changed public Rust type/callers/tests/docs, root-cause reuse and proof diagnoses | Fresh ponytail remediation discovery reviews entire small R3 delta and affected paths, validates no optional improvements. |
| Complex shared lifecycle and capability ownership: owner/handle -> session registration -> engine completion, record/budget state | Fresh invariant specialist required; check non-Clone owning state and executor's inability to take/drain its cleanup, all success/error paths and direct fixtures. |
| Material failure/recovery shift: Agent returns before join, action/check outcomes captured before cleanup, Busy/Terminal versus timeout progression | Fresh system specialist required; check actual producers/result consumers and preserved global/no-result cancellation. |
| Sensitive concurrency policy/permission/persisted boundaries | R1 concurrency and original security/records specialists plus R2 validated carry-forward cover unchanged Dispatch atomic allowance/semaphore, hold_permit/writer, grants/tenants and stored schema. R3 changes completion ownership, not these policies; fresh invariant/system coverage covers affected overlap and classification. No separate domain review unless new material boundary change is found. |
| Separate maintainer | New public type is one narrow capability split at existing ToolCalls state, expressly selected by maintainer. No replacement runtime/service/workflow/store, trait or dependency. Closure/invariant source review must inspect cost/reuse/export and all constructors; no substantial new orchestration abstraction requiring separate role. |
| Broad repository | Only existing Rust owners/test fixtures and report packet; no schemas/SDK/SQL/dependencies/generated/public-doc change. Prior standards evidence carries; changed-item rustdoc/exports/fixture docs audited freshly in closure review. |

Selected exact reports: task-review-closure.md, task-review-invariants.md,
system-review.md. Separate fresh ponytail validator after all discovery assesses
source, every proposal (including empty), closure/correction burden and role
applicability/carry-forward. New validator-discovered blockers need separate
focused source validation. Follow-up only for material conflict, untraced path
or diagnosis gap. No intended verdict or finding quota. If unsuccessful, include
convergence assessment, distinguish same cause/regression/new expectation and
bound next scope. Earlier missing-commit blocks do not count as assessments.
Orchestrator coordinates tests; no reviewer build/source experiments. Candidate
change or missing required report => BLOCKED.
