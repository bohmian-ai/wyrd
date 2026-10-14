# Discovery claim comparison

All seven discovery reports are present. They independently reviewed the
cumulative Wyrd and pinned-fork ranges in `subject.md`.

Behavior initially reported PASS, then falsified its singleton-bound premise
and revised its final report to FAIL. Only the final written report governs.
Behavior BEHAVIOR-1, invariant INV-1 and DataFusion DF-R3-1 identify the same
cause: the new scalar rewrite skips `single_value` when the existing native
`max_rows` producer falsely classifies an input as at most one row. The
invariant review additionally identifies grouping-set aggregation as a sibling
writer of the same false bound. The domain reviewer ran one focused traced
existing-binary reproduction outside the candidate; provenance limits are
recorded rather than claiming a fresh rebuild.

Standards, maintainer, system and tenancy discovery report PASS for their
scopes, with no material proposals. No final reports materially disagree on
the scalar shortcut's shared source. No separate follow-up is required: the
producer, sibling consumer and realistic execution path have been traced in
discovery, and the additional grouping-set proposal can receive ordinary
independent validation. A fresh validator will decide every claim; agreement
does not establish validity.

Owner steering during review: code should prevent edge cases from being
expressible. Apply that to the owning upper-bound/singleton proof, not to
per-query-shape consumer guards or an unrelated framework. The proposed
correction must preserve existing native aggregate, codec, global-count,
runtime-filter and route owners.

Prior FIND-1 and FIND-2 remain closed in discovery; FIND-4 documentation is
corrected; FIND-3 requires validation of incomplete cardinality closure.
No new finding IDs are assigned here.
