---
id: TASK-002-R1
kind: remediation
status: blocked
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

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-002-1`: after `insert_claimed`, a shutdown never leaves the row fair-claimable, sends one pre-effect report, and removes bookkeeping once | `forge/worker.rs::release_claim_at_shutdown` removes the dispatch bookkeeping once. A dispatched claim closes through `close_dispatched(.., Shutdown)` (`cancelled`, attempt cleared). It sends exactly one `NotStarted` report, and only when the close owned the row. Fair claims still release to `retryable`. | `production_routes.rs::dispatch_shutdown_before_episode_closes_and_reports_not_started`. RED with the `e8d3cca13` worker.rs: `left: ("retryable", None) right: ("cancelled", None)`. GREEN: row `cancelled`, leader `in_flight == None`, `pending_commits` preserved because NotStarted keeps commits due. | PASS |
| `FIND-TASK-002-2`: every mandatory comparison row is filled and reconciled against `74bdc45`, `6773e19` and `ef97aea` | `tasks/TASK-002-pull-and-worker-results.md` § "Mandatory pinned nimtable and Wyrd fork review — completed (TASK-002-R1)" fills six rows: Full, SmallFiles/FilesWithDelete, Auto, noncommitting seam, governor/spill, and cancellation/loose outputs. `ef97aea` and `6773e19` differ only in the iceberg-rust repin in Cargo.toml/Cargo.lock. The Auto gate moved into `managed/policy.rs::auto_candidates` with no behavior change, so a test now fails if it is removed. | The focused test named in each matrix row, including `managed::policy::tests::auto_candidates_follow_upstream_thresholds_and_delete_first_order`. | PASS |
| `FIND-TASK-002-2`: every retained fork-only module has a production consumer and a test that fails if it is removed | Consumed and tested: context/`SpillLease`/`with_context`/`with_executor`, `NonCommittingCompaction`, JoinSet drain/`Cancelled`/bridge/observer, and `SelectionReport` with `plan_compaction_with_report`. **Unconsumed in the shipped fork `ef97aea`:** `PeakTrackingMemoryPool`, `PeakMemory`/`ScratchSpill`, `with_scratch_capacity_bytes`, `SpillLease::measure`, identity-aware selection (`IdentityAwareSelector`, `WyrdIdentityAware`, `MatchNoneFileFilter`, `identity.rs`, `identity_plan.rs`), and `dependency_universe.rs`. | Inventory recorded in the TASK-002 section. | **BLOCKED** |

### Blocker

Narrowing the fork means three steps. First, delete the unconsumed modules in `bohmian-ai/iceberg-compaction`. Second, push a new revision. Third, re-pin `Cargo.toml:243` and `Cargo.lock`. That external push is outside this task's reach. Once the re-pin lands, the only Wyrd-side follow-up is deleting the dead peak-memory fields in `forge/managed/observer.rs`. No production code was changed just to fill an evidence row.

### Verification

- Focused run, with Postgres via `scripts/postgres/with-test-postgres.sh` and `WYRD_LOG=info,vala_bifrost_redux=debug`: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --test integration -P journey --run-ignored=all -E '<union of the 18 tests named above and in the matrix>'` passed 18 of 18.
- `mise run test:bifrost:integration:redux` passed 892 of 892.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --test forge -P journey --run-ignored=all` passed 21 of 21.
- `mise run fmt` ran, `mise run lints` exited 0, and `git diff --check` was clean.
- The release-mode capacity benchmark was not re-run. The worker change only touches the shutdown edge; steady-state execution is unchanged.

### Non-goals preserved

Only the three approved Wyrd differences remain: Scribe hot-publication recovery, Oracle/hot-object deletion protection, and central-governor charging with governed spill placement. No new planner, strategy or default was added. No unrelated file changed.

