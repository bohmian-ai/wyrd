# Concurrent closeout

At a milestone, wait until every task in that milestone is accepted and on the
root branch. Run its declared integration commands from the root worktree using
the verification lane. Reopen the earliest responsible task when integrated
proof changes an invariant-bearing owner, contract, consumer, or cleanup path.

After the final task integrates:

1. Run all plan-required user journeys, integration checks, format, lint,
   typecheck, codegen, schema, docs, migration, and boundary gates.
2. Audit the complete root-branch diff and generated provenance.
3. Run a fresh terminal `review-and-plan` against the committed execution
   baseline with the approved plan as intent.
4. Validate confirmed findings, repair them through the responsible task
   worktree or a newly scoped task, and repeat terminal review after material
   or cross-task remediation.

Report completion only when each task has an acceptance checkpoint, all root
verification passes, terminal review is clean, and no unrelated changes remain.
Report the root branch/worktree, baseline, final commit, task commits,
verification, material decisions, and retained task worktrees.
