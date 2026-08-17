---
name: wyrd-implement-plan-v3
description: Execute or resume an approved, decision-complete Wyrd v3 implementation plan through bounded parallel Sol-low implementors, immutable candidate commits, isolated verification lanes, independent task review, and serial root integration. Use when Codex must implement, continue, finish, or recover a complete Wyrd plan while preserving source candidates and canonical evidence across restarts.
---

# Wyrd Implement Plan V3

Run as the root controller with Sol reasoning effort `low`. The controller owns
scheduling, authority, state, integration, and user communication; it does not
implement delegated tasks. Never invoke a v1 or v2 implementation, review, or
planning workflow.

## Load authority

Read the approved plan and every task completely. Read repository `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`, the plan's named authorities, and
`references/controller-protocol.md`. Read the external plan repository's
canonical task files without copying them into an execution cache.

Require an approved, decision-complete plan. A defect that invalidates the
overall V1 objective, task DAG, or cross-task ownership must return through
`$wyrd-plan-v3`; do not silently repair that plan-wide intent inside this
controller. A bounded unforeseen decision inside one proven impact cone follows
the root advisory-revision protocol below. Invocation authorizes local branches, isolated worktrees, focused
and integrated verification, task evidence updates, and local commits. It does
not authorize push, PR creation, merge, history rewriting, identity changes,
or destructive cleanup.

The approved plan is an allowlist of intent as well as files. A candidate that
adds speculative abstractions, upstream machinery, optional infrastructure, or
behavior outside the task contract has failed even when it compiles or appears
beneficial. Return reversible deviation to implementation; treat any deviation
that requires a new product, durable, public, ownership, dependency, security,
or acceptance decision as material.

## Resolve material uncertainty

Only the root controller may commission advice. When implementation, proof, or
review exposes an unforeseen issue requiring a material decision, freeze its
proven impact cone and spawn one fresh read-only agent using `$wyrd-advise`.
The advisory request must name the exact accepted SHA, affected plan/task
digests, issue, current evidence, applicable Wyrd authorities, and requested
recommendation. The advisor investigates and recommends; it does not edit,
implement, mutate controller state, or invoke another workflow. Run the advisor
as `gpt-5.6-sol` with `low` reasoning.

The root validates the recommendation against source and authorities and owns
the decision. Reject and rerun an advisory result that does not select exactly
one recommendation, merely enumerates alternatives, asks planning to decide,
or returns the decision to the controller. Validate the result with
`scripts/validate_advisory_result.py` before any workflow transition. Record
escalation phases distinctly as `requested`, `recommended`, `root_validated`,
and `root_accepted` or `root_rejected`; neither planning nor task revision may
start before the root decision is recorded.

If the root agrees, record the evidence and rationale and author
one digest-bound successor task revision for only the frozen cone, with exact
contract, write-set, lock, dependency, proof, and escalation changes. Invalidate
affected candidates, proofs, and reviews; validate the revised task request and
resume through a fresh immutable candidate and independent review. Do not rerun
plan-wide cold rehearsal for this bounded execution-time revision. Unaffected
work may continue. If the decision changes the overall V1 objective, task DAG,
or cross-task semantic ownership, stop and route the plan-wide change through
`$wyrd-plan-v3`, including its normal rehearsal. If the root rejects the
recommendation, record the source-backed reason and continue only when the
existing approved contract unambiguously decides the issue. Advice never
authorizes an implementor or reviewer to deviate from the current task.

## Establish durable state

Create controller state outside source worktrees. Record its path in the root
session. Use `scripts/validate_controller_state.py` before first dispatch,
after every state transition, before integration, and when resuming. State is
a projection of its append-only event log; resume by reconstructing from that
log and comparing the resulting snapshot. Never use a context capsule or
context cache.

Initialize the complete scheduling identity of every task before first
dispatch: canonical digest and revision, dependencies, semantic locks, declared
write set, proof requirements, and proof prerequisites. Never populate these
fields lazily. An empty write set is invalid because it makes overlap checks
non-conservative.

The root alone writes canonical task status and evidence in the external plan
repository. Proof and review evidence first records the SHA-256 content digest
of each immutable artifact. After those digests and the source candidate SHA
are committed to the external plan repository, record that resulting Git
commit SHA separately as the canonical evidence commit. Never use a predicted
or self-referential evidence commit SHA. Worker reports, chat memory, branch
names, and worktree contents are not canonical task evidence.

## Schedule bounded work

Run a work-conserving scheduling pass after initialization and every state
transition, including dispatch, candidate publication, proof or review result,
failure, lane release, freeze, advisory transition, revision acceptance, and
integration. Each pass must:

1. recompute all dependency-ready tasks;
2. exclude tasks conflicting with active or accepted candidates;
3. honor the approved concurrency schedule and lane limits;
4. dispatch every selected task for which an implementor slot exists; and
5. record a concrete lane, conflict, dependency, approved-serialization, or
   agent-capacity reason for every ready task left idle.

Validate the resulting scheduling report with
`scripts/validate_schedule_pass.py` before dispatching or waiting. An eligible
task absent from both implementors and selected-but-idle reasons invalidates the
pass.

Do not focus the controller on the first active task. Freezing one cone releases
its implementor and lane allocations immediately and triggers another
scheduling pass; unaffected work continues. A task waiting for a proof lane
does not consume an implementor allocation.

Dispatch only dependency-ready tasks whose declared write sets do not overlap
any active or accepted-but-not-integrated candidate. Use at most three fresh
Sol-low implementors. Each implementor receives one task, its complete
contract, exact base SHA, owned write set, authorities, required implementation
skills, focused proof command, and isolated source worktree. Tell every worker
that it is not alone and must not modify canonical plan state, integrate,
rebase, merge, or change another task's files.

Use up to two isolated Cargo lanes. Each configured lane has a distinct worktree and
`CARGO_TARGET_DIR`; no task may occupy both. All commands that mutate shared
external state, including Postgres fixtures, generators, migrations, package
build caches, or repository services, use one exclusive stateful lane. A task
may wait for a lane without blocking unrelated static or implementation work.

Workers produce committed candidate SHAs. A candidate is immutable: any
remediation produces a new descendant candidate and invalidates review and
proof for the old SHA. Never amend, force-update, or continue editing a
candidate worktree after publication.

Report allocations by class after each scheduling pass: implementors, proof
lanes, reviewers, advisors, frozen tasks, dependency-blocked tasks, and every
selected-but-idle task with its reason. Reviewers do not count as implementors
and cannot be allocated before an immutable candidate exists.

## Prove and review candidates

Run every task-declared proof stream against the candidate SHA in its assigned
isolated lane. Keep stable proof IDs, exact proof-specific integrated
prerequisites, separate ordered attempts per proof ID, and exactly one accepted
authoritative PASS per required proof. A proof attempt is eligible only after
all prerequisites for that proof are integrated. Proof prerequisites never
gate implementation dispatch; only implementation dependencies do.

If a candidate predates integration of any proof prerequisite, return it from
`candidate` to `active` and produce a fresh descendant candidate from the
current integration head. Increment generation, replay only the scoped patch,
and clear every proof attempt, accepted proof, and review before publishing the
successor. Never merely rebase, re-prove, or reuse review for the stale
candidate. Record every attempt's full argv, lane, result, selected-test count,
classification, and artifact digest. Only classified infrastructure/setup
failure may be retried in the same proof stream on the same candidate; source
or test failure requires a successor candidate. Implementor test runs are
diagnostic only. Static review may overlap eligible focused verification.

Dispatch at most two simultaneous fresh Sol-low task reviewers. Reviewers are
read-only and receive the approved task, authorities, exact base and candidate
SHAs, complete diff, and available proof. They inspect correctness, contract
closure, ownership, callers, consumers, tests, generated surfaces, and
evidence semantics without running project verification or modifying source.
The root validates every finding against source.

A material finding freezes only its proven impact cone: the affected task and
tasks whose contracts, write sets, dependencies, or acceptance proof could
change. Independent work continues. Use the root-only material-uncertainty
protocol above for an unforeseen decision. Use `$wyrd-plan-v3` only when the
decision crosses the bounded-revision threshold; do not invoke an older planner. Reversible implementation
findings return to a fresh Sol-low implementor as a bounded successor
candidate.

## Integrate serially

Before integration require: candidate SHA unchanged, one accepted PASS for
every required proof ID on that SHA, every proof-specific prerequisite
integrated before its attempt, task review approved for that SHA, root
validation complete, dependencies integrated, and no active write-set overlap.
Perform a semantic-staleness check against every sibling candidate produced
from an older integration head. Re-dispatch or re-prove a sibling when
integrated changes alter its assumptions, owners, consumers, contracts, or
acceptance proof; a clean cherry-pick is not semantic proof.

Cherry-pick exactly one accepted candidate into the root worktree. Run any
declared post-integration check, record root-only canonical task evidence in
the external plan repository, mark the task complete, and commit the evidence
update. Then reschedule from the new integration head.

## Close out

After every task is integrated, run plan-defined integrated gates from the
root worktree. Invoke `$wyrd-review-and-plan-v3` against committed base and
target SHAs with the approved plan as intent. Validate its findings against
source. Required remediation is planned only through `$wyrd-plan-v3`, then
executed as a newly approved plan or task cone under this protocol.

Report completion only when all tasks and dependencies are integrated,
focused and integrated proof is recorded, terminal review is clean, controller
state validates, and the root history plus external canonical evidence are
inspectable. Preserve worktrees and state at handoff unless the user explicitly
authorizes cleanup.
