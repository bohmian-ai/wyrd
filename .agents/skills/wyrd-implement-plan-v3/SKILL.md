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

Require an approved, decision-complete plan. Route a material plan defect only
through `$wyrd-plan-v3`; do not repair product or contract intent inside this
controller. Invocation authorizes local branches, isolated worktrees, focused
and integrated verification, task evidence updates, and local commits. It does
not authorize push, PR creation, merge, history rewriting, identity changes,
or destructive cleanup.

## Establish durable state

Create controller state outside source worktrees. Record its path in the root
session. Use `scripts/validate_controller_state.py` before first dispatch,
after every state transition, before integration, and when resuming. State is
a projection of its append-only event log; resume by reconstructing from that
log and comparing the resulting snapshot. Never use a context capsule or
context cache.

The root alone writes canonical task status and evidence in the external plan
repository. Proof and review evidence first records the SHA-256 content digest
of each immutable artifact. After those digests and the source candidate SHA
are committed to the external plan repository, record that resulting Git
commit SHA separately as the canonical evidence commit. Never use a predicted
or self-referential evidence commit SHA. Worker reports, chat memory, branch
names, and worktree contents are not canonical task evidence.

## Schedule bounded work

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
change. Independent work continues. Route the frozen cone through
`$wyrd-plan-v3`; do not invoke an older planner. Reversible implementation
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
