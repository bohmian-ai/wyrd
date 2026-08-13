# Root Task Cycle

The root loads and applies `$wyrd-implement` directly, plus `$wyrd-ui` for UI
work. It establishes the task contract, implements the complete task, runs
focused verification sequentially, audits the full diff, and records exact
evidence.

Every implementation-finished task must invoke `$wyrd-review`; a generic
review or terminal review does not satisfy this gate. Spawn a fresh
`wyrd-reviewer` for the first pass and provide the approved plan, active `Ready`
task, last accepted commit, complete tracked and untracked delta, canonical
completion evidence, exact verification evidence, and applicable authority.

The reviewer is read-only. Require scope derivation before evidence inspection,
delta plus cumulative-owner analysis, the five-axis cited matrix, inspected
surfaces, and adversarial probes. Reject a verdict-only or uncited approval.

Validate findings against source. For `RESUME_IMPLEMENTATION`, fix confirmed
bounded issues in the root, rerun proof, and resume the same reviewer. For
`ROOT_DECISION_REQUIRED`, revise canonical authority and obtain plan review
when material before implementation. Accept only `APPROVE`, then mark the task
`Complete` and create the acceptance commit.
