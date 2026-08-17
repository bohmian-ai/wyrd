---
name: wyrd-implement-plan-v2
description: Execute or resume an approved complete Wyrd implementation plan through bounded concurrent task workers, isolated Git worktrees, independent review, and serial root integration. Use when Codex is asked to implement, continue, finish, or test a complete approved Wyrd plan while retaining task isolation and reducing repeated repository discovery. This is the opt-in v2 experiment; use wyrd-implement-plan for the production serial controller.
---

# Wyrd Implement Plan v2

Run one persistent root orchestrator with a warm, bounded worker pool. The root
owns canonical plan truth, material decisions, scheduling, acceptance,
integration, commits on the execution branch, and the user-facing outcome.
Workers own only candidate changes in isolated task worktrees. Review bundles
are read-only and independent: each bundle is one `$wyrd-review` parent and
its nested review-core code-quality specialist.

Do not trade correctness for concurrency. Dispatch only tasks that are proved
independent by their approved packets. Continue until the plan is `COMPLETE` or
external authority makes correct completion impossible.

## Load authority

Before dispatching, read the approved plan and every task, `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/wyrd-design.md`, and
`architecture/wyrd-doctrine.mdx`. Read `architecture/references/README.md` and
the affected-surface references. Use CodeGraph before grep when `.codegraph/`
exists.

Load these references completely when their stage begins:

| Reference | Read when |
|---|---|
| `references/execution-topology.md` | Establishing, scheduling, resuming, or integrating worktrees |
| `references/context-capsule.md` | Assigning or reassigning a worker |
| `references/task-cycle.md` | Implementing, verifying, reviewing, or remediating a task |
| `references/closeout.md` | Running a milestone, terminal review, or completion |

Use cold rehearsal as a planning-readiness signal, not as a second
implementation controller. Before dispatching the first task in a cohesive
milestone, verify that the approved milestone and its material predecessor
contracts have current rehearsal evidence. Do not rerun rehearsal merely
because an accepted predecessor changed private names, paths, helpers, test
fixtures, or other reversible mechanics.

A rehearsal finding blocks dispatch only when it identifies a material gap:
an unresolved public or persisted contract, dependency or feature decision,
security or tenancy behavior, data-loss/recovery behavior, cross-owner
authority, acceptance outcome, or an allocation whose pre-allocation bound or
enforcing owner is genuinely unknown. Missing private helpers, structs,
fixtures, adapters, exact filters, or local split/transfer plumbing are normal
implementation work when the approved owner and invariant are already fixed.
Record those items in the worker capsule and continue under
`$wyrd-implement`'s bounded-correction authority.

Readiness, qualification, sealing, and promotion are limited to the explicitly
named language/runtime surfaces and journeys. Evidence from one runtime cannot
be promoted into readiness for an omitted wrapper or language surface.

Do not require another cold rehearsal after implementation begins. Repository
discoveries then route through implementation and review. Return to planning
only when no in-scope implementation can preserve the approved material
contract.

Use the existing `wyrd-implement` skill for every non-UI worker assignment and
also `wyrd-ui` when its write set enters the Wyrd UI tree. The root invokes
`wyrd-review` for every task and `review-and-plan` at terminal closeout.

## Schedule bounded waves

Keep up to three warm implementation workers and one simultaneous read-only
review bundle. A bundle contains the `$wyrd-review` parent and its required
code-quality child, keeping the root, worker, and reviewer total within the
seven-agent concurrency budget. Keep exactly one Cargo/mise verification lane.
These are defaults, not a reason to manufacture parallel tasks.

Build each wave from `Depends on`, `Allowed scope`, and `Target paths and
symbols`. A task is eligible only when every dependency is accepted and
integrated, its cohesive milestone's current cold rehearsal passed, and it has
no overlap with an active task's owner, source path,
manifest, lockfile, migration chain, generated artifact, plan artifact, shared
configuration, or Cargo verification cone. Ambiguity means serialize.

The plan may include a concise advisory table in `Execution handoff` with task
wave, write boundary, shared-artifact locks, Cargo cone, and worker affinity.
Do not require a new canonical plan or task schema, controller ledger, run
directory, or completion sentinel.

## Maintain isolated execution

Follow `references/execution-topology.md`. Create one clean execution branch
and root worktree from the caller's resolved `HEAD`. Create each task worktree
from the current accepted integration commit. A worker never begins from a
peer's unintegrated branch and never writes the root worktree or canonical
plan status.

Reuse a warm worker when its accepted prior task shares a useful owner or
verification cone, but reissue an exact task capsule and require it to confirm
the current baseline. Agent memory is an optimization; the approved packet and
repository state are authority.

## Integrate one task at a time

Use the state machine in `references/task-cycle.md`. Workers may make
candidate commits only in their task branches, leaving their canonical task
status `Ready`. The root verifies the candidate commit/tree, reviews it,
cherry-picks approved commits in dependency order, records acceptance evidence,
sets the task to `Complete`, and creates the acceptance checkpoint.

For `RESUME_IMPLEMENTATION`, return the same task to its original worker and
worktree. Permit one or more bounded remediation commits while findings remain
private, local, and reversible. Review must evaluate conformance and safety;
it must not prescribe private type names or redesign an approved task when
multiple in-owner implementations satisfy the contract.

For genuine material decisions, stale baselines that change meaning, or owner
overlap discovered after dispatch, stop only the affected task and route it
through the material-authority repair flow in `references/task-cycle.md`.
Independent tasks may continue when the missing decision cannot change their
contracts, owners, write sets, or acceptance proof.

The root first verifies that the finding is material and that no in-scope
implementation preserves the approved contract. When existing user intent,
Wyrd authority, and repository evidence can determine the answer, obtain one
bounded recommendation from a fresh Sol-low agent using `$wyrd-advise`, then
validate the recommendation independently. Apply a localized canonical
authority correction directly only when it changes one affected task or
decision without changing the impact graph, dependency order, task
decomposition, or another task's contract. Otherwise give the accepted
recommendation to a fresh Sol-low `$wyrd-plan` agent for a scoped revision.
Materially revised milestones must pass a fresh `$wyrd-cold-rehearsal` before
redispatch from the newly accepted integration head.

A rehearsal `FAIL` is advisory evidence until the root validates its material
classification. The root must reject findings that only demand private DTOs,
helpers, adapters, signatures, fixtures, containers, or local call shapes. It
must not strengthen acceptance criteria or redesign an owner boundary unless it
can cite the exact approved material requirement that forces that change and
show that no in-scope private implementation preserves it.

Do not recurse through advisory and planning agents or ask them to approve one
another. The root owns the decision and may make at most one advisory pass per
blocker before choosing a route. Return to the user when resolution requires
new product intent, incompatible public semantics, destructive approval,
external authority, or a preference not derivable from accepted authority.
Never rebase, merge a worker branch, or let a worker solve a material conflict
independently.

## Close the plan

Follow `references/closeout.md`. Run milestone and final verification from the
root worktree, preserve the accepted branch and worker worktrees at handoff,
and perform the mandatory terminal `review-and-plan` pass. Complete only when
every task is accepted, required proof passes, terminal review has no required
finding, and the root branch contains only inspectable append-only work.
