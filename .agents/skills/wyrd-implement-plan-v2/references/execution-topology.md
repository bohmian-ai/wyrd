# Execution topology

## Root and task worktrees

Resolve the caller's `HEAD`, verify Git identity, then create a clean execution
branch and root worktree. Retain the caller worktree unchanged. Name the root
branch `wyrd/execute/<plan-slug>` and task branches
`wyrd/execute/<plan-slug>/t<NN>`. Use sibling worktrees named for the plan and
task. Do not reuse a branch name for a different packet.

The root worktree is the only canonical integration surface. A task worktree
contains one worker's candidate changes. Each task branch starts from the root
branch's current accepted commit; record that base SHA in the worker capsule.
Do not rebase, merge worker branches, or use a peer's task branch as a base.

## Eligibility

Dispatch a ready task only when all dependencies are complete on the root
branch and all of the following are disjoint from every active task:

- allowed source owners and paths;
- manifests, lockfiles, migrations, generated artifacts, plan/task artifacts,
  and shared configuration;
- required verification cone, including a shared Cargo target or integration
  fixture.

Tasks that merely look independent because they modify different files still
serialize when they share a contract, owner invariant, generator, migration
chain, or integration fixture. If the packet does not prove disjointness, run
it after the current wave integrates.

## Verification queue

Workers may inspect, edit, and run non-conflicting lightweight checks in
parallel. The root grants one verification token for any Cargo or `mise`
command. A worker releases the token after recording the exact command and
result. This preserves cache locality and avoids concurrent mutation of shared
build, fixture, and service state.

## Resume

Reconstruct execution from the root branch, canonical packets, worktree list,
task branches, candidate commits, and task evidence. A task is integrated only
when its acceptance commit is reachable from the root branch. A candidate task
branch without that commit remains pending, rejected, or stale; compare its
recorded base SHA with the current root head before reuse. Discard no worktree
or candidate commit automatically.
