# Dependency and fork-comparison domain review

## Subject and boundary

- Immutable Wyrd candidate: `7fcb45fc15ef2a43e8249af2a3dc7721fb55d517`.
- Original base: `c1508b375ba21a517f03ed6dd4d680dab4c3d12c`.
- Remediation base: `e8d3cca13ccb799ec6dc5c69d09da3de40bffba9`.
- RisingWave authority: `e23ddf952c3e6ebc03cc254789e84d1179cfacae`, which pins nimtable `iceberg-compaction-core` at `74bdc45cb17feaf0ec4eb351d4be271c99d3624c`.
- Candidate dependency: `Cargo.toml:243` and `Cargo.lock:4745-4748` both pin bohmian-ai `iceberg-compaction-core` at `380a4d0717e1786b95c4aa9f257579af496b3c8c`; the exact commit is present locally and on `origin/wyrd/narrow-managed-seam`.
- Reviewed authority: spec revision 11 REQ-005, REQ-007, REQ-010, REQ-013, INV-007, TASK-002, TASK-002-R1, and the task-index comparison rule.

## Coverage

| Required comparison | Source validation | Result |
| --- | --- | --- |
| Exact ancestry and shipped pin | `merge-base 74bdc45 380a4d0` is exactly `74bdc45`; `380a4d0` has parent `ef97aea`; `6773e19..ef97aea` changes only `Cargo.toml` and `Cargo.lock`; candidate manifest and lockfile agree on `380a4d0` | PASS |
| Full | Nimtable `74bdc45 core/src/file_selection/strategy.rs:820-831` selects all files. Fork `380a4d0:999-1015` adds only the later-upstream optional sequence filter, and candidate `forge/managed/policy.rs:278-294` always sets that bound to `None`; COW forces table-scoped Full | PASS |
| SmallFiles / FilesWithDelete | Nimtable `74bdc45:800-854` and fork `380a4d0:973-1044` use the same exclusive size and inclusive delete-count predicates when the sequence bound is unset. Candidate policy supplies the RisingWave runner terms. REQ-013's explicit SmallFiles-only two-file group and 75%-of-target settings are Wyrd table configuration, not a fork selector | PASS |
| Auto | Nimtable `74bdc45 core/src/compaction/auto.rs:131-202` gates on table-wide counts and tries delete-heavy work first. Candidate `forge/managed/policy.rs:409-522` ports the same one-file guard, 1-delete-heavy and 5-small floors and delete-first fallback while never invoking the fork's union selector | PASS |
| Noncommitting publication | Candidate `forge/managed/executor.rs:258-335` calls only `NonCommittingCompaction::rewrite`; fork `managed/boundary.rs:124-133` delegates to `rewrite_plan` and exposes no commit operation on that path. `compact_with_plan` has no candidate caller. The real integration proof asserts zero catalog mutation and unchanged live files | PASS, subject to the drift finding below |
| Central governor and spill | Candidate `forge/managed/executor.rs:407-426` installs `ForgeResources::rewrite_memory_pool()` and a `SpillLease`; fork `managed/context.rs:195-220` places those exact objects into the DataFusion runtime. Unit and real-rewrite proofs cover shared-root refusal, governed spill placement, and release | PASS; approved Wyrd difference |
| Cancellation and loose outputs | Fork `executor/datafusion/mod.rs` uses a `JoinSet`, checks cancellation before IO and during batch consumption, drains writer outcomes, and emits the attempt-global output ledger. Candidate `observer.rs` and `executor.rs:338-389` preserve those outputs as `RewriteUnsettled`; the integration proofs force cancellation and failed close after outputs open and assert every possible object remains named and no commit occurs | PASS; required deletion-protection behavior |
| Removed fork code | `ef97aea..380a4d0` is 13 files, `+173/-2454`. Candidate-wide symbol searches find no production consumer for the removed identity-aware selector/policy, peak-pool and spill-measurement surfaces, capacity argument, operator/peak events, dependency-universe verifier, or their deleted config types. Candidate Wyrd code was narrowed to the replacement API | PASS |
| Evidence completeness | TASK-002 lines 391-410 contain non-empty source, selected-owner, focused-proof and disposition cells for all six mandatory rows and the fork inventory. Every named Wyrd and fork test exists at the pinned commits | PASS |

## Material finding

### FORK-REV-001 — DRIFT — retained fork-only planning API has no Wyrd consumer

- **Violated obligation:** REQ-005 and REQ-010 require fork-only machinery without a proven remaining consumer to be deleted; TASK-002 also says the upstream noncommitting plan/rewrite methods mean a wrapper alone does not justify extra fork surface.
- **Exact location:** pinned fork `380a4d0`, `core/src/managed/boundary.rs:38-41,75-110` and `core/src/compaction/mod.rs:475-498`; the completed inventory at candidate `tasks/TASK-002-pull-and-worker-results.md:405,415-416` claims the retained boundary item has a production consumer without separating its unused planning half.
- **Evidence:** the only candidate production calls are `NonCommittingCompaction::new` and `rewrite` at `crates/vala/vala-bifrost-redux/src/forge/managed/executor.rs:279-305`. `NonCommittingCompaction::{attempt_id,context,load_table,plan_with_report}` have callers only in fork tests; its separately retained `context` field exists only for those accessors/debug output; `Compaction::plan_compaction_with_report` is called only by that unused boundary method and fork tests. Wyrd planning instead calls the necessary `CompactionPlanner::plan_compaction_with_report` directly from `forge/managed/policy.rs:423-439`.
- **Observable consequence:** the narrowed dependency still ships a second, unused public planning entry point and redundant retained state, so the claimed consumer-complete trim is not true and future changes have two fork planning surfaces to maintain.
- **Smallest testable correction:** in the fork, reduce `NonCommittingCompaction` to the production-used `new` and `rewrite` capability, remove its redundant `context` field/accessors/load/planning method and remove the now-test-only `Compaction::plan_compaction_with_report`; retain `CompactionPlanner::plan_compaction_with_report`, the injected executor, governor/spill context, cancellation/output ledger and Wyrd's direct planner path. Delete the tests that only assert the removed API and keep the noncommit, selection-report, governor, cancellation and loose-output proofs.

## Verification limits

- This pass inspected commit-scoped Wyrd, RisingWave and fork source and independently checked exact ancestry, diffs, symbols, consumers and named tests.
- I did not rerun the fork's recorded 151 unit tests or Docker-backed integration tests. The task records unit/clippy/fmt success; Docker-backed fork tests were only compile-checked. Candidate Wyrd integration and journey proofs exercise the retained runtime seams.

## Overall result

**FAIL.** Physical selection and the three required Wyrd safety seams are source-true, and the 2,454 deleted lines have no Wyrd consumer, but `FORK-REV-001` leaves bounded unconsumed fork-only API behind and prevents the consumer-complete narrowing claim from passing.
