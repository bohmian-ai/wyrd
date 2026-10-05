---
id: TASK-002-R1
kind: remediation
status: ready
spec: SPEC-forge-concurrent-planning
spec_revision: 11
parent_task: TASK-002
remediates: [FIND-TASK-002-1, FIND-TASK-002-2]
---

# Preserve dispatch ownership and complete the shipped-fork proof

## Contract and candidates

- Approved spec: `changes/active/forge-concurrent-planning/spec.md`
- Original task: `changes/active/forge-concurrent-planning/tasks/TASK-002-pull-and-worker-results.md`
- Task index: `changes/active/forge-concurrent-planning/tasks/README.md`
- Reviewed candidate/base: `7ac45dec99535c881b7a936c66623044f15d8823` / `c1508b375`

## Diagnosis

After `insert_claimed` commits and dispatch bookkeeping exists, shutdown before
episode start calls the generic retry release. That changes accepted leader
work to `retryable`, so `claim_fair` may assign it independently while the
leader still considers the table in flight. The existing `close_dispatched`
transition is bypassed. Separately, the mandatory physical-planner/fork matrix
compares upstream `74bdc45` with old pin `6773e19` but does not reconcile the
shipped `ef97aea` fork or all required rows/modules. Thus attempt ownership and
the three-difference constraint are both unproved.

## Intended correction outcome

An accepted dispatch closes only through the pull/report protocol, including
shutdown before episode start. The packet also contains a complete, source-
checked comparison of the shipped fork with no empty row.

## Decision-complete recommendation

At the post-insert/pre-episode shutdown edge, reuse `close_dispatched` and the
existing report path: close the exact attempt without making it fair-claimable,
remove bookkeeping once, and report the established pre-effect/NotStarted
outcome once. Add no state or queue. Update the existing fork-review evidence,
not a new artifact, to compare `74bdc45`, `6773e19`, and shipped `ef97aea` for
Full, SmallFiles/FilesWithDelete, Auto, noncommitting seam, governor/spill, and
cancellation/loose outputs; every retained fork-only module must name a live
consumer and a failure-sensitive test.

## Preserved behavior and non-goals

- Preserve capacity calculation, pull cadence, oldest-due selection, worker-
  side planning, stale-result handling, and capacity thresholds.
- Preserve only the three approved Wyrd fork differences.
- Do not change production code solely to fill an evidence row.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-002-1` | Shutdown after durable dispatch insertion never makes the row fair-claimable and produces exactly one leader-visible pre-effect report. |
| `FIND-TASK-002-2` | Every mandatory comparison row is non-empty and reconciles the shipped fork; every retained fork-only module has a production consumer and removal-sensitive test. |

## Focused proof and broader verification

Pause after `insert_claimed`, trigger shutdown, and prove closure, non-claimability,
and one report. Execute every focused comparison test recorded in the completed
matrix. Re-run the release-mode Forge capacity benchmark only if production
worker behavior changes, then run the owning Bifrost lane, format, lints, and
diff check.

