# Discovery claim comparison

All eight required discovery reports are complete. Behavior BEH-R5-1, invariants INV-R5-001 and standards RSTD-R5-1 identify one common issue: active shutdown-publication descriptions left after removal of the forced residue sweep. They propose documentation-only correction. Maintainer/system/data/concurrency reviews find these stale descriptions but no executable correctness defect and differ on their blocking significance within the user's correctness-regression-only scope.

All reviewers find the R5 prior corrections intact; no material runtime regression is proposed. Data reviewer investigated a possible failed-claim tick retry gap, but established it is unchanged adjacent behavior, not an item-2 removal-induced regression; startup resumes interrupted claims. It is not proposed for remediation.

A fresh focused follow-up is required to resolve the documentation conflict from current user authority and source/callers. It must establish which active descriptions specifically promise the deleted shutdown sweep, versus accurate admitted-work drain or explicit flush. It receives all reports, cumulative subject and fixed decisions, without an intended verdict. Independent Ponytail validation follows regardless of its result.
