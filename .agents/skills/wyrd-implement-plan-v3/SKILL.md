---
name: wyrd-implement-plan-v3
description: Execute a Wyrd plan produced by wyrd-plan-v3 through its detailed task packets, focused verification, immutable candidate commits, independent review, and serial integration. Use when Codex must implement, continue, finish, or test a complete approved plan without creating manifests, cold rehearsals, or proof-attempt ledgers.
---

# Wyrd Implement Plan v3

Execute the plan's task packets; do not turn the controller into another
planner. The root owns task order, integration, user communication, and the
minimal execution record. It does not implement delegated source changes.
Never invoke a v1 or v2 workflow. V3 roots and delegated roles run as
`gpt-5.6-sol` at low reasoning effort.

## Establish execution

Read `plan.md` and every task packet, then read `AGENTS.md`,
`architecture/agent-rules.md`, and the authorities named by the affected
tasks. Confirm that each task has bounded scope, concrete acceptance criteria,
focused verification, and only necessary dependencies. Resolve the current
integration SHA and preserve a small execution record outside source
worktrees: task ID, base SHA, candidate SHA, verification result, review
verdict, and integrated SHA. A repository that supplies the plan is a planning
source only: never place source worktrees, Cargo targets, virtual environments,
or execution records inside that repository. Put execution records under the
target repository's ignored `.dev/executions/<plan-id>/` tree (or another
caller-declared external evidence root), and put source worktrees under a
dedicated sibling such as `<target-repository>-worktrees/<plan-id>/`.

The plan is an allowlist of outcomes and ownership, not a demand that every
local implementation consequence be anticipated in a packet. Reject
speculative work or material contract changes outside a task. Before freezing a
task, use the packet's intended outcome, repository context, diagnostics, and
the implementor's evidence to distinguish a real authority gap from routine
implementation judgment. Treat compiler, formatter, lint, rustdoc, codegen,
fixture, and focused-test failures as normal development feedback. The
implementor may make the task-local completion changes permitted by its
implementation skill; do not replan for those changes.

Stop for user or plan-authority direction only when evidence, after reasonable
task-local investigation, exposes an undecided product, public or durable
contract, ownership, dependency, security, tenancy, migration, or acceptance
decision. An omitted path, unspecified local implementation detail, or routine
repair is not itself such evidence. Record the exact conflict and freeze only
the affected task and its dependent tasks.

## Execute tasks

Execute tasks in dependency order, serially by default. Run tasks in parallel
only when their packets explicitly permit it and their write scopes and semantic
owners are disjoint. Give every worker its complete task packet, the current
base SHA, a dedicated clean worktree, its allowed/prohibited scope, the
surface-appropriate implementation skill, and its focused verification command.

For each task:

1. Dispatch one implementation worker. It produces one normal immutable
   candidate commit; it does not integrate, amend, rebase, or change the plan.
2. Run the task packet's focused `mise` verification command against that
   candidate in an isolated worktree. Treat worker checks as diagnostic only.
3. Build the compact review bundle required by `$wyrd-review-v3` from the task
   packet, candidate/parent/diff identity, implementation report, current
   controller snapshot, and the focused-check result as its proof summary.
   Dispatch one independent read-only reviewer.
4. For a reversible verification or review defect, send the exact finding or
   diagnostic to a fresh worker generation and repeat from a new descendant
   candidate. Do not revise the packet merely because a repair is inconvenient.
5. For an approved candidate, cherry-pick it into the root worktree, run any
   task-declared post-integration check, and record the resulting integrated
   SHA before starting a dependent task.

Never reuse verification or review after a candidate changes. If an integrated
predecessor changes a remaining task's real assumptions, refresh that task from
the current integration SHA before it is implemented. A clean cherry-pick is
not proof of semantic compatibility.

## Close out

After all tasks integrate, run the plan's declared integrated checks. Run
`$wyrd-review-and-plan-v3` only when the plan or user requests terminal review;
otherwise report that task-level independent review and focused verification
were completed. Required terminal-review remediation returns to
`$wyrd-plan-v3` as new or revised task packets.

Report integrated commits, focused verification, review verdicts, unresolved
material decisions, and any checks intentionally not run. Preserve the minimal
execution record. After a candidate is reviewed and integrated, reclaim its
clean implementation and verification worktrees and their task-specific Cargo
targets, virtual environments, and caches. Never delete a dirty worktree:
archive it under the dedicated sibling worktree root and report why it was
retained. Keep only the current integration worktree and worktrees needed by
active or dependent tasks; reclaim those during final closeout. If the user
explicitly requests preserved worktrees, retain the requested set instead.
