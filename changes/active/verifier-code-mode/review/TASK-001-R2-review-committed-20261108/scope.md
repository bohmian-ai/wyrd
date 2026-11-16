# TASK-001 R2 committed review scope

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode.
Original base: 5234961d51798581a026834cfffd5efaadc226b4.
Prior candidate: f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18.
Runtime candidate: 66f4d54ca14d9da0257aa6a72239687e7abef004.
Cumulative candidate/HEAD: 9abeaf124585cd0d726d2554965b105fc0c0ef86.
Last commit adds prior review/R2 evidence only. Review git diff <prior> <candidate>;
original-base cumulative range remains acceptance context. No rebasing.
Pinned authority: approved revision 2 via git show 9abeaf124:changes/active/verifier-code-mode/spec.md;
sha256 cc1b3c37d28201f3207769901776cc34b6a73c2810e9c42c53b50c2033cd4055.
Read original TASK-001, original review, R1 review/validated ledger and
TASK-001-R2-settle-before-retry.md with its appended evidence. They are committed.
Local spec revision3/snapshot/TASK003 are unrelated pre-existing planning;
untracked earlier R2-review directory records only a missing-commit block.
Do not alter those or any source/index/HEAD. New reports only here.

## Bounded acceptance scope and carry-forward

Remaining FIND-TASK-001-2: result-producing authored attempt timeout must settle
started bounded work before retry/Attempted result. Check R2 delta and affected
callers. Nine other findings closed by prior independently validated ledger
remain closed unless this fix affects accepted behavior. ID7 remains withdrawn.
Carry authority/tenancy/catalog, records/delivery/integer, budget/cap guards,
portable limits/workspaces, SDK/public evidence and non-goals from original and
R1 source-backed matrices. The whole-execution deadline, no-result abandonment,
lease-loss and new shutdown contracts were explicitly excluded as new blockers
by R1 validation; preserve existing semantics without reopening scope.
The earlier uncommitted subject block was not a remediation assessment.

## Shared navigation map (starting points, not exhaustive coverage)

- Vala blocks/tools.rs: ToolCalls/CallLog settlements, settle/defer;
  ToolSession with_settlement/settle_with; prior close removed. Inspect all
  consumers of settlement and cancellation around awaits.
- blocks/agent.rs AgentExecutor::invoke: handoff at open, normal completion
  settlement before dropping session; resolve/error/Halt paths.
- blocks/engine.rs VerificationEngine::attempt: engine-owned ToolCalls across
  retry, timeout_at actions.execute, added settlement before retry/result.
  run/check and server queued runner are relevant unchanged consumers.
- integration/blocks_agent.rs: HeldWork finished signal, SettlingToolset,
  timed_out_attempt_settles_before_retry_or_result; real engine/AgentExecutor,
  retry/no-retry cases with a held operation. Inspect proof boundary adequacy.
- Server HostedTools::open/Invocation::blocking (unchanged) produce tracker
  settlement; owned writer/workspace/permit and TaskTracker wait prove work ends.
- Prior R1 reports in ../TASK-001-R1-review-20261108/: findings-validation.md,
  verdict.md, task-review-closure.md, task-review-invariants.md,
  domain-review-concurrency.md, system-review.md and verification.md.

## Specialist applicability before discovery

| Contract; producer/consumer | Selection and evidence |
| --- | --- |
| Sole finding closure, timeout proof, changed Rust docs/owner reuse, affected records/budget/result | Fresh remediation ponytail discovery; inspect source and actual tests, carrying nine prior closures from R1 final matrix. |
| Material shared lifecycle: settlement ownership transferred session -> ToolCalls -> Agent/engine, cancellation of nested awaits and exactly completed work | Fresh invariant specialist: new pending settlement state/consumers require producer-to-sink trace. |
| Material recovery path: attempt timeout progression, retry/result and normal Agent completion/Halt | Fresh system specialist, bounded to these changed paths and preserved failure/queue consumers. |
| Sensitive concurrency semaphore/budget/writer boundaries | R1 domain-review-concurrency plus validation carry exact permit/budget/guard source evidence. No semaphore/admission policy, mutation owner or task-local permit transfer changes in R2; new settlement ownership covered by invariant specialist and closure reviewer. Separate concurrency domain omitted unless new material race/policy boundary is established. |
| Security/tenancy/persistence and broad repo | Unchanged producers/wire/storage/dependencies; original security/records/standards plus R1 validated coverage carry. No fresh broad specialists; closure reviewer audits every newly changed Rust item/doc/test and affected record consumer. |
| Substantial replacement ownership/abstraction | Existing ToolCalls/ToolSession/engine extended with pending settlement; no replacement runtime/service. Closure/invariant reviewers compare complete existing-owner workflow and smallest correction. Separate maintainer omitted unless new evidence shows substantial unearned replacement. |

Selected own reports: task-review-closure.md, task-review-invariants.md,
system-review.md. After discovery, separate fresh ponytail validator must
validate source/closures/selection and every proposal, even empty ledger.
No intended verdict, finding quota or automatic scope expansion. Any new
validator-discovered blocker needs separate focused source validation.
Orchestrator coordinates focused test runs; reviewers must not run builds or
edit source. Candidate changes => BLOCKED. If a second unsuccessful remediation
assessment results, verdict includes convergence assessment per skill.
