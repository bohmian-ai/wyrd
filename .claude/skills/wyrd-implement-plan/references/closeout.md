# Integrated Closeout

At plan milestones, run the declared integrated commands sequentially and
record exact results. Reopen the earliest responsible accepted task when later
work changes an invariant-bearing surface or integration fails; repair it in
the root and require another `$wyrd-review` approval.

After all tasks are accepted, run required journeys and affected format, lint,
typecheck, codegen, schema, docs, migration, and boundary gates. Audit and
commit the complete verified branch.

Spawn a fresh Opus/low subagent and explicitly invoke full `$review-and-plan`
against the committed execution baseline with the approved plan as reference.
This is mandatory and never replaces per-task `$wyrd-review`. The terminal
review remains read-only. The root validates and implements confirmed findings,
reruns proof, and creates fix-forward commits. Repeat full review after material
or cross-task changes; focused `$wyrd-review` may close only bounded fixes.

Complete only when every task is accepted, all proof passes, terminal review
has no required finding, generated outputs are current, and no unrelated change
remains.
