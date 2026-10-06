---
id: TASK-002-R1
kind: remediation
status: review
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
| `FIND-TASK-002-2`: every retained fork-only module has a production consumer and a test that fails if it is removed | Fork commit `380a4d0717e1786b95c4aa9f257579af496b3c8c` on the new `bohmian-ai/iceberg-compaction` branch `wyrd/narrow-managed-seam` (parent `ef97aea`, 13 files, +173/−2454). It deletes `PeakTrackingMemoryPool`, the `PeakMemory`/`ScratchSpill` events and the never-emitted `OperatorSpill`, the `with_memory_pool` capacity argument, `with_scratch_capacity_bytes`, `SpillLease::measure`, identity-aware selection (`IdentityAwareSelector`, `WyrdIdentityAware`, `identity.rs`, `identity_plan.rs`) and `dependency_universe.rs`. `MatchNoneFileFilter` is upstream (`d4b7c4f`), not fork-only. `core/src/file_selection` is identical to upstream `d4b7c4f`. Wyrd re-pins `Cargo.toml:243`/`Cargo.lock` and deletes the peak fields in `forge/managed/observer.rs`. `policy.rs`, `fingerprint.rs` and `executor.rs` follow the narrowed API. Fingerprints are byte-compatible: production reports always had `policy: None`. | Retained items map to the matrix tests (inventory in the TASK-002 section). Fork: `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt --check`, and `cargo test --workspace --lib` (151 passed, including the new `wyrd_selection_report_is_canonical_or_refused`, `wyrd_selection_report_is_canonical_and_matches_final_plans` and `managed_context_charges_leased_pool_and_spills_under_leased_root`). The Docker-backed fork integration tests were compile-checked, not run. | PASS |

### Fork narrowing (resolved blocker)

With human approval, the narrowed fork was pushed as a new branch. No existing ref was rewritten. Wyrd now pins `380a4d0`.

### Verification

Phase 1 (commit `042cae533`, shutdown fix): the focused run of 18 tests passed 18 of 18, the redux lane passed 892 of 892, and the forge journey passed 21 of 21.

Phase 2 (fork re-pin to `380a4d0`):

- Focused run, with Postgres via `scripts/postgres/with-test-postgres.sh` and `WYRD_LOG=info,vala_bifrost_redux=debug`: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::managed::observer::tests::forge_managed_observer_folds_outputs_without_semantics) | test(=forge::managed::fingerprint::tests::forge_selection_fingerprint_is_ordered_versioned_and_snapshot_bound) | test(=forge::managed::fingerprint::tests::forge_no_progress_refuses_unchanged_semantic_debt_before_io) | test(=forge::managed::executor::tests::rewrite_pool_charges_root_and_releases_on_cancel) | test(=forge::managed::policy::tests::auto_candidates_follow_upstream_thresholds_and_delete_first_order) | test(=forge::managed::policy::tests::forge_table_policy_plans_risingwave_task_types) | test(=forge::managed::policy::tests::only_small_files_requires_a_partner_file)'` passed 7 of 7.
- `mise run test:bifrost:integration:redux` passed 892 of 892.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --test forge -P journey --run-ignored=all` passed 21 of 21.
- `mise run fmt` ran, `mise run lints` exited 0, and `git diff --check` was clean.
- `mise run bench:bifrost:forge-capacity` (release, re-run because the fork change touches the rewrite path) passed all 16 gates: in-process and live-leader p99 commit/pull/report all below 65 µs at 1x and 10x, 2- and 3-worker drain ratios 2.11 and 3.22, no short pull under backlog, leader CPU 3.6%, 0 unsuccessful rewrites, knee at 64,000 pulls/s.

### Non-goals preserved

Only the three approved Wyrd differences remain: Scribe hot-publication recovery, Oracle/hot-object deletion protection, and central-governor charging with governed spill placement. No new planner, strategy or default was added. No unrelated file changed.
