---
id: TASK-007
title: Read staged runs with the published Parquet scan
kind: implementation
status: implemented
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 20
requirements: [REQ-014]
acceptance: [AC-016]
depends_on: []
blocks: [TASK-008]
---

## Outcome and Value

Live reads stop decoding every staged row. A selective query over 10M staged
rows drops from ~150 ms of CPU to a few ms, lifting the 4-core plateau from
~25 qps toward the 1,000 qps target.

## Evidence that drives this task

Profile (release server, 4 CPU/8 GiB, 10M rows in 102 staged runs, one row
group each): 82% of CPU is `ParquetRecordBatchStream` decode in
`scribe/staged_tail.rs` (`open_next_group`, `with_row_groups(vec![next_group])`
at :230) with no row-group or page pruning; zero-match queries cost the same
as hits. Offline replay on the same runs: 135 ms → 1.3 ms (row-group prune) →
0.6 ms (row-group + page prune). Every query runs on one core:
`oracle/live.rs:205` plans one partition per route and
`wyrd-server/src/oracle/peer_service.rs:336-345` runs the follower with a
literal partition count of 1. The in-process transport encodes Arrow IPC and
SHA-256 hashes every frame (`oracle/dispatcher.rs:781-818`, hash :807) and the
leader decodes and re-hashes it (`oracle/live.rs:735-757`, hash :740): 35% of
a 1-minute aggregate, 86% of `COUNT(*)`.

## Requirements

1. Staged runs are read by the same Parquet scan Oracle uses for published
   and hot files (`oracle/exec.rs`: `HotParquetExec`, `IcebergParquetReader`,
   `select_row_groups_for_predicates` :4003, `select_pages_for_predicates`
   :3899, page-index loading, node metadata cache). The custom staged decoder
   and the per-batch predicate compile in `scribe/tail_rpc.rs` (~:763) are
   deleted, not wrapped.
2. In-memory Scribe rows are read through DataFusion's in-memory source with
   the same pushdown.
3. The live leaf and the Scribe follower take their partition count from the
   session (REQ-013); no route-count or literal `1` partitioning remains.
4. When Oracle and the Scribe share a process, live batches pass as Arrow in
   memory: no IPC encode/decode and no hashing. Row/byte counters and the
   terminal footer rules stay. Remote transport is unchanged.
5. INV-004 holds: the scan executes on the Scribe pod; Oracle never opens
   another pod's staged files. Memory stays charged to the query grant.

## Verification

- A focused test proving a selective predicate skips non-matching staged row
  groups (observable through an existing scan metric or pruning counter; add a
  production metric if none exists, not a test hook).
- Run only the tests affected by the change: the focused test above, the
  vala-bifrost-redux unit tests for the touched modules, and the live-read
  journeys (the narrowest `test:bifrost:journey:<capability>` leaves or exact
  nextest expressions). After a fix, re-run only the tests that failed plus
  the focused ones. Do not run the full `mise run test:bifrost` lane; the
  caller runs it once after TASK-008.
- `mise run fmt`, `mise run lints`, `git diff --check`.
- Do not run the capacity benchmark; the caller runs it after TASK-008.

## Evidence

| Requirement | Implementation | Verification | Result |
|---|---|---|---|
| 1. Staged runs use Oracle's Parquet scan; custom decoder and per-batch predicate compile deleted | `scribe/staged_tail.rs` deleted; `ScanPredicateFilter`/`retain_signed` deleted; `oracle/follower.rs` `ScribeTailResolver::live_leaf` builds `HotParquetExec` over staged runs (`ObjectPin::Staged` metadata key in `storage/cache.rs`, lease held via `HotParquetExec::with_staged_lease`) | `oracle::follower::tests::scribe_staged_scan_prunes_non_matching_row_groups` (Eq value=2 → files_scanned 1, row_groups_scanned 1, row_groups_pruned 2); `oracle::follower::tests::a_dropped_staged_scan_releases_its_lease_immediately`; `scribe::tail_rpc::tests::an_open_live_read_keeps_staged_runs_across_publication` | PASS |
| 2. In-memory rows via DataFusion in-memory source with the same pushdown | `live_leaf`: memtable batches → `MemorySourceConfig`, unioned with the staged scan, one `FilterExec` from `exec::scan_predicate_conjunction` | `oracle::follower::tests::scribe_provider_projects_and_filters_a_nonzero_ordinal` and module suite (`-E 'test(/^oracle::follower::/)'`) | PASS |
| 3. Session partition count for live leaf and Scribe follower | `OracleTableProvider::scan` passes `target_partitions()` to `LiveScribeExec::new`; routes dealt `partition, partition+N, …`; `OracleResources::follower_execution` derives local partitions (no literal `1`); `peer_service.rs` call updated | `resources::tests::scribe_follower_execution_shape_contract`; oracle live-read journeys in `mise run test:bifrost` | PASS |
| 4. In-process live batches pass as Arrow, no IPC/hash; footer rules kept; remote unchanged | `dispatcher::LiveFrame::{Wire,Batch,Complete}`; directory routes this node's own Scribe to `ScribeFragmentExecutor` in every mode; `LiveFrameDecoder` counts rows, requires completion, refuses frames after it; only gRPC `execute_fragment` encodes (`AttemptEncoder`) | `oracle::live::tests::live_frames_release_batches_incrementally_and_validate_the_footer`; `wyrd-server` `oracle::peer_service::tests::*` (3 pass) | PASS |
| 5. Scan runs on the Scribe pod; memory charged to the query grant | Staged scan is built inside the Scribe follower's resolver under its follower session pool (`OracleScanGovernance::Follower`); leader only dispatches fragments | follower suite; live journeys | PASS |
| Format, lints, whitespace | — | `mise run fmt`, `mise run lints` (clippy `--all-features -D warnings`), `git diff --check` | PASS |
| Staged reads do not block the runtime | `oracle/follower.rs` `staged_run_io` uses `iceberg_storage_opendal::OpenDalStorageFactory::fs()` (async OpenDAL filesystem service, already in the graph through `iceberg-compaction-core`) instead of iceberg's `std::fs` `LocalFsStorageFactory`; workspace and crate manifests declare it, and the lockfile adds one edge | staged-scan focused tests above | PASS |
| Bifrost lane | — | Full `mise run test:bifrost`: 1425 passed, 3 failed (diagnosed and fixed below). After the fixes, the three were re-run with their exact expressions through the Postgres wrapper: 3 passed. Per the caller, the full lane is not re-run. | PASS (3 re-run) |

### Diagnosis: three Oracle journey failures

Each test was re-run with `WYRD_LOG=info,vala_bifrost_redux=debug` and an exact `-E 'test(=…)'` through the Postgres wrapper. All three were migrated to the in-process `PeerCluster`, so every pod shares one process.

- **`analytical_activation::selected_peer_failure_is_terminal`**
  - **Symptom:** "settled 5 attempts".
  - **Cause:** `attempt_totals` read the process-wide `bifrost_oracle_analytical_attempts_total`, which sums the attempts of the leader and of every follower.
  - **Fix site:** `AnalyticalSupervisor` now counts its own admitted and successful attempts (`attempt_counts`, test-support only). `PeerCluster::attempt_counts(index)` exposes them, and the journey reads the coordinator's counts.
- **`peer_network::transport::peer_transport_uses_immutable_fenced_destinations`**
  - **Symptom:** the `restart(1)` shutdown inspection failed with "Oracle shutdown retained ... state", `peer_running=1`.
  - **Cause:** the transport probe's `ReserveSlots` left a pending reservation that no leader would ever activate. `Oracle::shutdown` left that reservation to expire, so the capacity report counted it.
  - **Fix site:** `ReservationRegistry::drain_pending`, which `Oracle::shutdown` calls before the admission report. The `Oracle.reservations` field is no longer test-support-only.
- **`distributed::published_workers_and_live_scribes_share_one_plan`**
  - **Symptom:** "published worker 2 admitted no peer work".
  - **Cause:** the test's claim was wrong. This was confirmed by an independent read-only diagnostician. `register_cut_providers` freezes each table's cut to one destination (`destinations[index % len]`), and `OracleRouteTasks` routes every task of that stage there. `AnalyticalCutTaskCount` caps `LiveScribeExec` at one task, and that cap merges up to the hash repartition, so the shuffle boundary is removed and the aggregate stays in the leader stage. This is the design documented on `LiveUnionBoundary`. For a single table with live rows, exactly one worker leases a graph. The old multi-process check used process-wide body polls, which cannot attribute work to a worker.
  - **Fix site:** the test in `distributed.rs`. It now asserts that exactly one published worker leased a graph, and its docs explain why. There is no production change.

### Exact recipes and attribution (TASK-007-R1)

All recipes run from the repository root. Where tasks run concurrently, set a worktree-local `CARGO_TARGET_DIR` (and `MISE_STATE_DIR` if the default is read-only). Attribution:

- **Historical:** the original TASK-007 run recorded above. No exact command line was kept except where stated.
- **R1:** executed on 2026-09-30 against the TASK-007-R1 working tree.
- **Deferred:** needs the Postgres wrapper or a full lane, which is owned by the caller after TASK-008.

Unit checks for `vala-bifrost-redux` with the `test-support` lib and no Postgres:

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=oracle::follower::tests::scribe_staged_scan_prunes_non_matching_row_groups)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=oracle::follower::tests::a_dropped_staged_scan_releases_its_lease_immediately)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=scribe::tail_rpc::tests::an_open_live_read_keeps_staged_runs_across_publication)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=oracle::follower::tests::scribe_provider_projects_and_filters_a_nonzero_ordinal)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=resources::tests::scribe_follower_execution_shape_contract)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=oracle::live::tests::live_frames_release_batches_incrementally_and_validate_the_footer)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(/^oracle::follower::tests::/)'
```

- **Historical:** PASS was recorded, but no command line was kept.
- **Review:** `review/task-007-review/verification.md` records 18/18 passing for a combined selection of these tests.
- **R1:** each single-test recipe above: 1 passed. The follower, live and dispatcher module selection: 30 passed.

Server unit checks for `wyrd-server` peer service, no Postgres:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(/^oracle::peer_service::tests::/)'
```

- **Historical:** 3 passed.
- **R1:** 3 passed: `stale_object_dispatch_status_is_not_found`, `scribe_fragment_failure_classes_survive_to_dispatch` and `empty_scribe_attempt_emits_schema_and_complete_footer`.

Three diagnosed journeys: Postgres and the `wyrd-testing` `oracle` target, ignored journey tests:

```sh
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=only -E "test(=analytical_activation::selected_peer_failure_is_terminal) | test(=peer_network::transport::peer_transport_uses_immutable_fenced_destinations) | test(=distributed::published_workers_and_live_scribes_share_one_plan)"'
```

- **Historical:** the session log (2026-09-30 17:19) shows the Postgres wrapper, `db:migrate:all:inner`, then these three tests: "3 tests run: 3 passed, 37 skipped". The exact selector text was not logged, so the command above is the reconstructed current recipe.
- **R1:** `cargo nextest list --locked -p wyrd-testing --test oracle --run-ignored=only -E '<the same expression>'` confirmed that the expression selects exactly these three tests. Execution is deferred because TASK-008 forbids Postgres runs here.

Aggregate lanes and gates:

```sh
mise run test:bifrost
mise run fmt
mise run lints
git diff --check
```

- **`mise run test:bifrost`, historical:** 1425 passed and 3 failed, as recorded in the table above. Those 3 failures are the journeys diagnosed above.
- **`mise run test:bifrost`, R1:** deferred to the caller after TASK-008.
- **`mise run fmt` and `mise run lints`, historical:** PASS.
- **Gates, R1:** only the scoped equivalents in the R1 evidence (`review/task-007-review/TASK-007-R1-close-live-scan-gaps.md`).

The "oracle live-read journeys" and "live journeys" in rows 3 and 5 of the table refer to that aggregate lane. They name no specific test.

### FIND-007-3 disposition (lead decision, 2026-09-30)

Not changed. The shared `hot_stream` owner (published leader, published
follower, and staged scans) holds a decoded batch's charge until the next
pull, which is the engine's operator convention; batches queued in DataFusion
coalesce/repartition channels are item-bounded on every path. Base
`a7582db58` already behaves this way for published scans, so this is not a
TASK-007 regression. Exact in-flight accounting (rewrapping Arrow buffers
with a charge-carrying owner) would be a separate, owner-wide change covering
published and staged scans together; it is not part of revision 20.

**Status (2026-10-01):** implemented in `83ccbc634` ("serve live reads from
one Parquet scan with per-file tenant proof"), already on `origin/main`.
