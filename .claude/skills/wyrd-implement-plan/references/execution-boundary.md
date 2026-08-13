# Execution Boundary

The root is the only writer. It validates the plan, inspects Git state and
identity, and creates one uniquely named local branch and clean dedicated
worktree from the caller's `HEAD`. Never delegate those mutations.

Never stash, reset, clean, commit, copy back, or absorb caller-owned changes.
Import only caller-selected plan artifacts absent from the baseline. Respect
ignore rules, stage exact paths, and use append-only fix-forward history. Never
force-add, broadly stage, push, merge, amend, rebase, squash, reset, rewrite
history, or alter Git identity.

Use the plan, task packets, completion evidence, Git history, source, and
recorded verification as durable state. A task remains `Ready` until proof
passes and `$wyrd-review` approves it.

Repair stale private mechanics from repository evidence. For material changes
to contracts, migrations, dependencies, features, security, tenancy, audit,
ownership, requirements, or acceptance outcomes, revise all affected authority
cohesively and obtain read-only `wyrd-plan-reviewer` approval.

Use `Blocked` only for genuinely unavailable external credentials,
permissions, infrastructure, irreversible authority, or an artifact with no
discernible outcome. Failures, findings, plan defects, compaction, and elapsed
time are not external blockers.
