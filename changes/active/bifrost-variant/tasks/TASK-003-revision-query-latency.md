# TASK-003 revision: query latency, scan simplification, and close-out

Status: approved by the user on 2026-10-07 ("Approve"), together with spec
revision 25.

This revision applies to `TASK-003-shredding-and-leaf-pruning.md`. It lists
every remaining TASK-003 item: the code changes (Part A) and the close-out
(Part B). TASK-003 is complete when both parts are done.

## Outcome

1. The nested-field benchmark meets current DataFusion performance: every
   Struct case is at or below plain DataFusion reading the same file with
   the same query.
2. Shredding makes querying faster: every shredded Variant case is faster
   than the same query on unshredded Variant, and at or below plain
   DataFusion's Struct time for the same query.

## How this is proven

No new scenario tests are written. The code is revised, simplified and made
faster. Existing tests, journeys and benchmarks prove it.

- A test that only exercised deleted code is deleted with it.
- A test that pinned an old internal shape (for example a partition count) is
  updated to the new shape.
- Nothing else in a test changes.

Proof lanes, run after every step unless the step says otherwise:

| Lane | Command |
|---|---|
| L1 format | `cargo +nightly-2026-03-05 fmt` |
| L2 lint | `CARGO_TARGET_DIR=target/clippy cargo +nightly-2026-03-05 clippy --locked -p vala-bifrost-redux -p wyrd-testing -p wyrd-client --all-features --all-targets -- -D warnings` |
| L3 redux lib | `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support'` |
| L4 redux journeys | `mise run test:bifrost:integration:redux` |
| L5 Oracle journeys | `mise run test:bifrost:journey:oracle` |
| L6 nested-field bench | `mise run bench:bifrost:nested-field-pushdown` |
| L7 Forge journeys (deletes, lineage, rewrites) | `mise run test:bifrost:journey:forge` |
| L8 full gate, once at the end | `mise run verify:bifrost` |

Steps A1–A3 run L1–L3 and L6. Steps A4–A7 run L1–L7. Every L6 result goes
into the benchmark log at the end of this file.

## The rule

We only write new code for:

1. **Wyrd-specific implementation**: memory accounting, tenant proof on file
   footers, the staged-run lease, signed follower assignments, scan evidence
   and metrics, admission; and
2. **extending DataFusion** for what it lacks, such as Variant shredding and
   Variant querying (`variant_sql.rs`, the shredded-leaf read in
   `nested_pushdown.rs` and `leaf_paths.rs`, and the iceberg reader hooks
   they need).

Everything else uses DataFusion, arrow/parquet or iceberg-rust as-is. Every
ADD in the ledger names which of the two it is.

## Why

The benchmark runs in-process: SQL → plan → hot scan → collect, with no
server. It was far slower than plain DataFusion on the same workload. The
reference is the pinned fork's `parquet_struct_projection` bench, run on the
same machine.

| Case | Before (Struct / shredded) | After A1 (Struct / shredded) | Fork bench (LTO build, other data) |
|---|---:|---:|---:|
| narrow select_small_field | 764 / 986 µs | 366 / 628 µs | 208 µs |
| wide select_small_field | 788 / 1035 µs | 345 / 693 µs | 217 µs |
| nested select_inner_small_field | 793 / 1085 µs | 453 / 717 µs | 226 µs |
| wide sum_small_field | 1138 / 1271 µs | 680 / 809 µs | 340 µs |

## Findings

### F1. Scans split across every CPU (measured)

- **Symptom:** a 512-row, two-row-group read planned 64 partitions on a
  32-CPU host.
- **Evidence:**
  - the profile was dominated by `parking_lot` lock_slow (~16%), malloc
    (~11%), and `ExecutionPlanMetricsSet::register` /
    `BaselineMetrics::new` (~6%);
  - `ProjectionExec::execute` and `UnionExec::execute` were called once per
    partition (33×).
- **Cause:**
  - `HotParquetExec` and `OracleIcebergScanExec` split by
    `target_partitions` without statistics;
  - the empty Iceberg leaf (the bench has no snapshot) also took 32.
  - DataFusion's own rule does not split a scan whose exact row count fits in
    one batch (`enforce_distribution.rs:1549`). The fork's
    `repartition_file_min_size` default is 1 MiB (`config.rs:1675`), not
    10 MiB.
- **What A1 actually changed:** the empty Iceberg leaf now takes 1 partition,
  and that is the measured gain. The 3.3 MB hot files are over 1 MiB, so the
  hot scan still splits 32 ways; A1's hot rule never fires on this bench.
- **Fix site:** A4. The hot scan is DataFusion's `DataSourceExec` with exact
  row-count statistics from the footer, so DataFusion's own rule decides. A
  probe with the hot scan forced to 1 partition measured Struct at
  210–226 µs, against plain DataFusion's 270 µs on the same file.

### F2. Oracle re-implements library features (read from source)

The read path is about 7,200 production lines:
- about 3,000 are Wyrd-specific implementation;
- about 2,350 extend DataFusion (`variant_sql.rs`, `nested_pushdown.rs`,
  `leaf_paths.rs`, the iceberg hooks);
- about 900 duplicate a library feature.

The published scan stays on iceberg-rust's `ArrowReader`. DataFusion's
`ParquetSource` has no delete files, field-id mapping, partition constants or
row lineage. `iceberg-datafusion` has its own scan for the same reason, and
`spec.md` keeps "Moving the published read path to `ParquetSource`" as a
non-goal.

### F3. Per-query overhead (read from source; A4–A6 remove it)

1. **Serial hot IO:** `IcebergParquetReader` (`exec.rs:2741`) implements no
   `get_byte_ranges`. parquet's default therefore makes one storage request
   per column chunk, one after another.
2. **Filters run twice:** `OracleTableProvider::scan` adds a `FilterExec`
   (`exec.rs:2405-2420`). DataFusion adds its own residual filter above the
   provider anyway, because pushdown is `Inexact`.
3. **Per-partition and per-file rework:**
   - every partition compiles the predicate conjunction and the leaf
     projection;
   - every file rebuilds `ArrowReaderMetadata` from the cached footer and
     registers a `ParquetFileMetrics` that is discarded.
4. **Per-column settings:** `ConfigOptions::default()` is allocated per
   trimmed column and per typed comparison (`variant_sql.rs`).
5. **Unneeded Iceberg leaf:** it is always planned (`exec.rs:2120`), even
   when the cut has no published files.
6. **Session rebuilds:** up to four full DataFusion session builds per query
   (planning, execution, each follower fragment, each analytical leaf).
   Measured by F4 before anything is changed for it.
7. **Extra repartition on analytical leaves:** since A1, a small analytical
   leaf resolves to fewer partitions than it advertised, so
   `AnalyticalScanExec::resolve` adds a round-robin `RepartitionExec`. This
   is correct but costs a node.

### F4. Profile of the remaining gap (measured)

- **Method:**
  - frame-pointer `perf -e task-clock` on the narrow cases;
  - a scratch probe timing parse/plan/execute separately over the same
    Oracle tables and plain DataFusion on the identical file.
  - The probe and the folded profiles are in the session scratchpad
    (`profile/`).

Wall time per narrow small-field query, as parse+logical / optimize+physical
/ execute = total, in µs:

| Configuration | µs |
|---|---|
| Fork bench, fork build (full LTO, 1 codegen unit) | 182 |
| Same fork bench, Wyrd release settings | 242–250 |
| Plain DataFusion on Oracle's Struct file | 45 / 138 / 85 = 270 |
| Oracle Struct, as benched (hot scan split 32 ways) | 49 / 134 / 145 = 330 |
| Oracle Struct, hot scan in 1 partition | 38 / 111 / 59 = 210–226 |
| Oracle shredded Variant, as benched | 63 / 173 / 383 = 622 |
| Oracle shredded Variant, 1 partition, no round-robin | 63 / 180 / 480–506 = 738 |
| Oracle shredded Variant, 1 partition, round-robin left on | 74 / 188 / 698 = 965 |

Causes, ranked:

1. **Variant row path, ~390 µs, serial (DataFusion extension).**
   `CAST(s ->> 'small_int' AS INT)` on a shredded file:
   - `FileRead` (`variant_sql.rs:804`) unshreds the typed column back into
     binary Variant (~100 µs);
   - `VariantGet` (`variant_sql.rs:414`) re-validates it and runs
     `variant_get` on the binary form (186 µs);
   - `VariantAsText` (`variant_sql.rs:644`) validates again and renders text
     (78 µs);
   - arrow then parses the text back into Int32 (18 µs).

   Validation is `wyrd-queue/src/variant.rs` `mask_placeholders` (:513) and
   `validate` (:284): a full metadata validation, a scan, a `catch_unwind`,
   and another full validation, per row and per call. That is 166 µs inside
   the steps above.
2. **Hot scan fan-out, ~70–120 µs on Struct.** See F1.
3. **Round-robin above the Variant projection, ~230 µs.** DataFusion adds a
   32-way `RoundRobinBatch` because the scan reports no statistics. It must
   be fixed with cause 2: with only cause 2 fixed, Variant goes from 622 to
   965 µs.
4. **Build settings, ~60 µs, not Oracle.** The fork's 182–208 µs uses full
   LTO. Built like Wyrd, the same bench runs at ~245 µs. The A2
   `_datafusion` form removes this from the comparison.
5. **Per-execute fixed costs, 5–25 µs each**, multiplied by the partition
   count:
   - `ConfigOptions::default()` per `FileRead::expr` (8.7 µs);
   - `ArrowReaderMetadata::try_new` from the cached footer on every query
     (`exec.rs:3298`);
   - the footer-cache hit path building a loader future and telemetry
     (`exec.rs:3212`);
   - throwaway `ParquetFileMetrics` in `FileReadPlan::new`
     (`nested_pushdown.rs:67`);
   - the empty Iceberg leaf (`exec.rs:2129`, `available_parallelism`
     syscalls plus `Table::scan`), about 23 µs.
6. **Variant planning, about +117 µs over Struct.** Parse +14, optimize and
   physical +39, mostly `schema_name` formatting of a three-deep expression.
   INFERRED to be generic DataFusion cost; it shrinks once A7 folds
   `CAST(->>)` into one call.

## Change ledger

Every code item this revision touches. **DELETE** removes it. **REVISE**
changes it in place. **ADD** is new, and only under the rule above. Anything not
listed is unchanged.

### `crates/vala/vala-bifrost-redux/src/oracle/exec.rs`

| Item | Action | Step | Replaced by / change |
|---|---|---|---|
| `scan_partitions` | REVISE (added in A1) | A1, A4 | Iceberg scan only after A4 |
| `OracleIcebergScanExec::with_partitions` | REVISE (done) | A1 | takes `&ConfigOptions`, sizes from snapshot summary |
| `HotParquetExec` + `ExecutionPlan`/`DisplayAs` impls | DELETE | A4 | `DataSourceExec` + `ParquetSource` inside `HotScanExec` |
| `HotParquetExec::with_partitions` | DELETE (revised in A1) | A4 | `FileGroupPartitioner` |
| `HotReaderFactory` | DELETE | A4 | `OracleParquetReaderFactory` |
| `hot_stream`, `hot_piece_metadata` | DELETE | A4 | `ParquetSource` opener |
| `HotBloomProbes`, `BloomProbe` | DELETE | A4 | `ParquetSource` Bloom pruning (`bloom_filter_on_read`) |
| `partition_byte_ranges`, `row_groups_in_byte_range` | DELETE | A4 | `FileGroupPartitioner`, `RowGroupAccessPlanFilter::prune_by_range` |
| `RowGroupSelection`, `parquet_column_index` | DELETE | A4 | `ParquetSource` row-group plan |
| `OracleScanMetricsHandle::record_hot_range`, `record_hot_file`, `record_row_groups`, `record_bloom_pruned`, `record_page_pruned_rows` | DELETE | A4 | DataFusion `ParquetFileMetrics` |
| `OracleScanMetricsHandle` (struct, Iceberg half) | REVISE | A4 | Iceberg counters only |
| `OracleQueryScanStats::visit` | REVISE | A4 | folds `ParquetFileMetrics` into `PruningEvidence` |
| `OracleParquetReaderFactory` | ADD (Wyrd-specific: governed ranges, tenant footer, lease) | A4 | implements `ParquetFileReaderFactory` |
| `HotScanExec` | ADD (Wyrd-specific: grant/pool resolution, per-batch charge, leaf paths) | A4 | thin wrapper over `DataSourceExec` |
| `HotParquetPlan`, `HotParquetGovernance`, range/decoded reservations | KEEP | — | reused by the factory and wrapper |
| `IcebergParquetReader` | REVISE | A5 | wraps iceberg `ArrowFileReader`; adds `get_byte_ranges` with one reservation |
| `HotObjectSource` | DELETE | A5 | iceberg `ArrowFileReader` |
| `OracleIcebergScanExec::start_stream` limit stream (lines 1707-1724) | DELETE | A5 | iceberg's limit stream |
| `OracleIcebergScanExec` predicate / leaf projection / planner built per partition | REVISE | A6 | built once, shared by `Arc` |
| `OracleTableProvider::scan` provider `FilterExec` (lines 2405-2420) | DELETE | A6 | DataFusion residual filter |
| `OracleTableProvider::persisted_inputs` | REVISE | A4, A6 | builds `HotScanExec`; skips the empty Iceberg leaf and a one-input `UnionExec` |
| test `hot_parquet_split_partitions_read_every_row_once` | REVISE (A1 done) | A1, A4 | runs on `HotScanExec` |
| test `scan_partitions_split_only_large_scans` | DELETE | A1 | covered by the split test and L6 |
| `HotParquetExec` unit tests of governance, tenant refusal, lease, retry/drop | REVISE | A4 | rebuilt on `HotScanExec` |
| unit tests of Bloom probes, byte-range split, row-group selection | DELETE | A4 | outcome proven by L4/L5 |

### `crates/vala/vala-bifrost-redux/src/oracle/follower.rs`

| Item | Action | Step | Replaced by / change |
|---|---|---|---|
| hot leaf (line ~344), `live_leaf` staged scan | REVISE (A1 done) | A1, A4 | `HotScanExec` under session options |
| `restrict_plan_to_assigned_files` | REVISE | A1, A3 | takes `&ConfigOptions`; keeps only the `IcebergTableScan` branch |
| `OracleIcebergScanExec` branch and `DataSourceExec`/`FileScanConfig` branch of `restrict_plan_to_assigned_files` | DELETE | A3 | unreachable: the catalog builds only `IcebergStaticTableProvider` |
| `physical_file_location` | DELETE | A3 | only the deleted branch used it |
| test `scribe_live_sources_keep_the_session_partition_count` | REVISE (done) | A1 | renamed `…_read_every_row_once_at_their_partition_count`; staged expects 1 |
| test `oracle_projection_is_exactly_the_authenticated_assignment` | REVISE | A3 | keeps only the duplicate-assignment assertion; the rest tested the deleted branch (live path proven by L5 distributed journeys) |
| test helpers `file_plan`, `plan_files` | DELETE | A3 | — |

### Other files

| File: item | Action | Step | Change |
|---|---|---|---|
| `oracle/leaf_paths.rs`: `LeafPathPushdown` | REVISE | A4 | targets `HotScanExec` instead of `HotParquetExec` |
| `oracle/variant_sql.rs`: `ConfigOptions::default()` per column and per comparison | REVISE | A6 | one per adapter |
| `oracle/variant_sql.rs`: `OracleVariantSql` planner rule | REVISE | A7 | folds `CAST(->> / variant_get)` into typed `variant_get` |
| `oracle/variant_sql.rs`: `FileRead` unconditional unshred | DELETE | A7 | passes shredded input through to `variant_get` |
| `oracle/variant_sql.rs`: per-call validation in `VariantGet::invoke`, `VariantAsText::invoke` | DELETE | A7 | validated once where bytes enter |
| `wyrd-queue/src/variant.rs`: `mask_placeholders` | DELETE if no caller remains | A7 | — |
| `oracle/nested_pushdown.rs`: throwaway metrics set in `FileReadPlan::new` | DELETE | A6 | the scan's own metrics set |
| `oracle/exec.rs`: footer-cache hit path (line 3212) | REVISE | A6 | no loader future on a hit |
| node metadata cache: cached `ArrowReaderMetadata` | REVISE | A4 | stored beside the footer |
| `bench_support.rs`: `hot_leaf_paths`, `bytes_scanned` | REVISE | A4 | read `HotScanExec` / `ParquetFileMetrics` |
| `benches/nested_field_pushdown.rs`: `write_shape`, `main`, `check`, `smallest_large_leaf` | REVISE | A2 | four forms: plain DataFusion, Struct, unshredded, shredded |
| `architecture/bifrost-design.md` | REVISE | B3 | scan architecture and partition rule |
| `changes/active/bifrost-variant/spec-revision-25-hot-parquet-source.md` | ADD | A4 | one-sentence spec change |
| `TASK-003-shredding-and-leaf-pruning.md` | REVISE | B4 | stale notes, frontmatter, evidence |

## Part A — code changes, in order

### A1. Small scans read one partition per file (done, uncommitted)

- **`oracle/exec.rs`:**
  - new `scan_partitions(files, bytes, &ConfigOptions)` applies DataFusion's
    listing rule;
  - `HotParquetExec::with_partitions` and
    `OracleIcebergScanExec::with_partitions` take `&ConfigOptions`.
    `OracleIcebergScanExec` sizes from the snapshot summary
    (`total-data-files`, `total-files-size`) or the assigned file count;
  - the leader provider passes `state.config_options()`.
- **`oracle/follower.rs`:**
  - the follower hot leaf, `live_leaf`'s staged scan and
    `restrict_plan_to_assigned_files` pass `session.config().options()`;
  - `live_leaf` memtable grouping is unchanged.
- **Tests updated to the new shape:**
  - `hot_parquet_split_partitions_read_every_row_once` sets the threshold to
    0;
  - `scribe_live_sources_keep_the_session_partition_count` is renamed
    `scribe_live_sources_read_every_row_once_at_their_partition_count`, and
    the staged source now expects 1 partition.
- **Remove** the unit test `scan_partitions_split_only_large_scans` that was
  added during diagnosis; the split test and L6 cover the rule.
- **Proof:** L1–L3 already pass (873/873); L6 is recorded.
- **Correction (F1):** the hot half of A1 never fires on this bench, and A4
  deletes it. The `OracleIcebergScanExec` half stays.

### A2. Benchmark compares plain DataFusion, Struct, unshredded Variant and shredded Variant

All changes are in `benches/nested_field_pushdown.rs`:

- **`write_shape`** writes a third file, `<shape>_unshredded.parquet`. It
  holds `cast_to_variant` of the Struct column with no `shred_variant`, under
  the same Variant field metadata. The return type becomes three
  `(schema, path)` pairs.
- **`main`:**
  - registers the table `<shape>_unshredded`;
  - runs each case's `variant_sql` against it, with the table name replaced;
  - reports the ids `<case>_datafusion`, `<case>_struct`,
    `<case>_variant_unshredded` and `<case>_variant_shredded`.
- **`check`** gets a `byte_bound: bool` parameter:
  - true for Struct and shredded, unchanged;
  - false for unshredded, which must read the whole value. The function
    still checks the plan, the rows and the leaf paths. If the leaf paths of
    an unshredded read differ, the expected value is recorded from the first
    run and stated here.
- **Plain DataFusion reference.** `main` also builds a plain
  `SessionContext` (default config, no Oracle rules) with
  `register_parquet("<shape>", <struct file>)`. It runs each case's
  `struct_sql` as `<case>_datafusion`, checked for the same rows. This is the
  reference for the outcome on identical data. The fork's
  `parquet_struct_projection` numbers stay as context only, because its
  strings repeat and compress while ours vary per row.
- `smallest_large_leaf` ignores the unshredded file.
- The median table prints the four forms side by side.
- **Proof:** L6, every assertion passes.

### A3. Delete dead follower branches

In `oracle/follower.rs`, `restrict_plan_to_assigned_files` keeps only the
`IcebergTableScan` branch. That is the only plan
`IcebergStaticTableProvider::scan` returns, and it is the only provider the
catalog builds (`catalog/bifrost_catalog.rs:1378,1493,1531`).

- **Delete:**
  - the `OracleIcebergScanExec` branch;
  - the `DataSourceExec` / `FileScanConfig` branch;
  - `physical_file_location`;
  - their now-unused imports.
- **Stays fail-closed:** any other leaf leaves `file_leaves == 0`, which
  still returns the error.
- **Test:** `oracle_projection_is_exactly_the_authenticated_assignment`:
  - Its "restricted to the assigned file", "missing file" and "extra file"
    assertions run only through the deleted `DataSourceExec` branch, so they
    are deleted.
  - The live path's file-set checks (`OracleIcebergScanExec::plan_tasks`) are
    proven by the distributed journeys in L5 (`oracle/distributed.rs`, the
    follower `with_assigned_files` cases).
  - The duplicate-assignment assertion tests live code that runs before the
    plan is examined, so it is kept, on an `EmptyExec` plan.
  - Delete `file_plan` and `plan_files`.

### A4. Hot reads use DataFusion's `DataSourceExec` + `ParquetSource`

- **Delete from `oracle/exec.rs`:**
  - `HotParquetExec` and its `ExecutionPlan` and `DisplayAs` impls;
  - `HotReaderFactory`, `hot_stream`, `hot_piece_metadata`;
  - `HotBloomProbes`, `BloomProbe`;
  - `row_groups_in_byte_range`, `partition_byte_ranges`;
  - `RowGroupSelection`, `parquet_column_index`;
  - the hot counters `record_hot_range`, `record_hot_file`,
    `record_row_groups`, `record_bloom_pruned` and `record_page_pruned_rows`
    on `OracleScanMetricsHandle`.
- **Add, Wyrd-specific only:**
  - `OracleParquetReaderFactory`, implementing DataFusion's
    `ParquetFileReaderFactory`. Its `create_reader` returns the governed
    `IcebergParquetReader` (A5 shape), whose `get_metadata` loads the footer
    through `BifrostStorage::object_metadata` and verifies the tenant key
    before any data range is read. It also owns the staged lease, an
    `Option<Arc<StagedSourceLease>>`, so the lease lives as long as the scan
    and its streams.
  - `HotScanExec`: one thin wrapper over the `DataSourceExec`. At `execute`
    it resolves the admitted grant and memory pool through the existing
    `HotParquetPlan::resolve`, hands the governance to the factory for that
    partition, and charges each yielded batch through the existing
    `HotParquetGovernance::reserve_decoded`. It carries `leaf_paths` for
    `LeafPathPushdown`. Every other `ExecutionPlan` method delegates to the
    inner exec.
- **The hot `FileScanConfig`:**
  - one `PartitionedFile` per `HotFileSource`, with size and location;
  - the closure schema;
  - projection from the leaf paths;
  - `ParquetSource` with the pushed predicate, `pushdown_filters`,
    `bloom_filter_on_read`, page index, and the custom factory;
  - expression adapter `OracleVariantSql::file_adapter()`;
  - each `PartitionedFile` carries exact statistics from the cached footer
    (`num_rows`, and byte size), so DataFusion's own distribution rule
    decides whether to split (F1, F4 causes 2 and 3). Oracle does not split
    eagerly; `DataSourceExec::repartitioned` does it when DataFusion asks.

  `scan_partitions` then serves only `OracleIcebergScanExec`.
- **Cache `ArrowReaderMetadata`** beside the footer in the node metadata
  cache, so a query does not rebuild the Arrow schema from the footer
  (F4 cause 5).
- **Object store:** register `file://` and the hot store URL on the session
  runtime env where the provider is built. Hot reads still go through the
  factory's reader, never through the store.
- **Callers moved to `HotScanExec`:**
  - `OracleTableProvider::persisted_inputs`;
  - follower `hot_leaf` / `signed_hot_source`;
  - follower `live_leaf` (staged runs);
  - `LeafPathPushdown`;
  - `OracleQueryScanStats::visit`, which now reads DataFusion's
    `ParquetFileMetrics` (`bytes_scanned`, `row_groups_pruned_statistics`,
    `row_groups_pruned_bloom_filter`, `page_index_rows_pruned`, files) into
    the same `PruningEvidence` fields;
  - `bench_support::{hot_leaf_paths, bytes_scanned}`.
- **Check before writing code, and stop with a finding if one fails:**
  1. Remote placeholders never serialize their local hot plan. A
     `FileScanConfig` with a custom adapter does not serialize
     (`datafusion-datasource-parquet` `file_scan_config/proto.rs:95`).
  2. DataFusion's Bloom lookup on a nested leaf. The spec non-goal already
     excludes Bloom on shredded Variant leaves. Losing Struct-leaf Bloom is
     recorded here, not patched.
- **Spec change (revision 25, `spec-revision-25-hot-parquet-source.md`):** in
  the read-core paragraph of `spec.md`, replace "`HotParquetExec` and the
  pinned Iceberg `ArrowReader` call the exact same facade" with "DataFusion's
  `ParquetSource` for hot files and the pinned Iceberg `ArrowReader` for
  published files call the exact same facade". It needs the user's approval
  together with this revision.
- **Tests:**
  - `HotParquetExec` unit tests in `oracle/exec.rs` that assert behaviour
    still owned by Oracle (governance, tenant refusal, lease lifetime,
    retry/drop paths) are rebuilt on `HotScanExec`;
  - tests of deleted internals (Bloom probe, byte-range split, row-group
    selection) are deleted, and the journeys in L4/L5 prove the outcome;
  - `hot_parquet_split_partitions_read_every_row_once` keeps its assertion
    (every row once across N partitions) on `HotScanExec`.

### A5. Published and hot reads fetch byte ranges in batches

- In `oracle/exec.rs`, `IcebergParquetReader` wraps iceberg's
  `ArrowFileReader` (`iceberg::arrow` `file_reader.rs:59`) instead of
  `HotObjectSource` plus its own bounds and short-read code.
- `get_byte_ranges` reserves the summed bytes through
  `HotParquetGovernance::reserve_range` before IO. It then delegates, so
  ranges are merged and fetched concurrently.
- Delete `HotObjectSource` and its bounds and short-read checks.
- `OracleIcebergScanExec::start_stream`: delete the copied limit stream
  (`exec.rs:1707-1724`) and use iceberg's.

### A6. Remove duplicate and repeated work

- **`OracleTableProvider::scan`:** delete the provider `FilterExec` and
  project the union directly. Before deleting, confirm that no distributed
  stage boundary relies on it; follower leaves apply their own signed filter.
- **`OracleTableProvider::persisted_inputs`:** skip the Iceberg leaf when the
  snapshot has no files in the cut, and drop the `UnionExec` when only one
  input remains.
- **`OracleIcebergScanExec`:** compile the predicate conjunction, the leaf
  projection and the file read planner once in its constructor, and share
  them through `Arc` with every partition.
- **`variant_sql.rs`:** build one `ConfigOptions` per adapter instead of per
  column and per comparison.
- **`nested_pushdown.rs` `FileReadPlan::new`:** pass the scan's real
  `ExecutionPlanMetricsSet` instead of registering a throwaway one.
- **Footer cache hit path (`exec.rs:3212`):** return the cached entry without
  building a loader future on a hit.

### A7. Variant reads the shredded typed column directly (DataFusion extension)

This is F4 cause 1. All changes are in `oracle/variant_sql.rs` unless noted.

- **Fold casts into the read.** `CAST(variant_get(s, path) AS T)` and
  `CAST(s ->> k AS T)`, where T is a primitive Arrow type, become one
  `variant_get` with `as_type = T`. Arrow's `variant_get` with a typed
  `as_type` reads a shredded `typed_value` leaf directly and falls back to
  the residual only for rows that need it. The planner rule doing this sits
  beside the existing `->>` rewrite in `OracleVariantSql`. `->>` without a
  cast keeps its text semantics.
- **Stop unshredding in `FileRead`.** When every consumer of a trimmed
  shredded column is a `VariantGet` with a path, `FileRead` passes the
  shredded `StructArray` through; arrow's `variant_get` accepts shredded
  input. It unshreds only for a consumer that needs the whole binary value
  (`VariantAsText` of the root, output of the column itself).
- **Validate once, at the trust boundary.** `VariantGet` and `VariantAsText`
  stop calling `wyrd-queue` `mask_placeholders` / `validate` on arrays Oracle
  itself produced from a file it already verified. Validation stays where
  bytes enter: ingest, and the footer/tenant-verified file read.
- **Delete** the code paths these remove: the per-call validation in
  `VariantGet::invoke` and `VariantAsText::invoke`, and `FileRead`'s
  unconditional unshred.
- **Ledger impact:** the `variant_sql.rs` A7 rows are REVISE. If
  `mask_placeholders` loses its last caller, it is DELETE in
  `wyrd-queue/src/variant.rs`.
- **Proof:**
  - L3, including `oracle::nested_pushdown` and the `variant_sql` tests;
  - L4/L5, including `published::struct_and_variant_share_physical_pushdown`
    and the residual / unshredded-key journeys;
  - L6, where `_variant_shredded` must beat `_variant_unshredded` and meet
    `_datafusion`.

Not doing: rewriting `LeafPathPushdown` onto DataFusion's projection pushdown
(high risk, no measured gain).

## Part B — close-out

- **B1. Commit the uncommitted finished work** with the step it belongs to:
  - A2 carries the benchmark: `benches/nested_field_pushdown.rs`,
    `bench_support.rs`, the bench session in `bindings.rs`, `oracle/mod.rs`
    visibility, the `Cargo.toml` criterion dev-dependency and `[[bench]]`,
    `Cargo.lock`, the `mise.toml` task, and
    `spec-revision-24-benchmark-shape.md`;
  - B3 carries `architecture/bifrost-design.md`.

  `spec.md` and `TASK-004-forge-parallelism.md` are never committed.
- **B2. Diagnose `wide/select_one_string_field` on Variant**, which takes
  about twice `nested/select_extra_string` although both return one 16 KiB
  string per row.
  - INFERRED from F4: `FileRead` unshreds the trimmed column, and
    `VariantGet` / `VariantAsText` validate and copy each 16 KiB value
    several times.
  - After A7, rerun L6. If the 2× remains, profile that case and record
    Symptom / Evidence / Cause / Fix site here.
- **B3. Architecture doc (`architecture/bifrost-design.md`):** keep this
  session's additions (Variant shredding, Nested-field reads, the
  `bifrost_variant_residual_share` metric), and add:
  - small scans read one partition per file, by DataFusion's rule;
  - hot files are read by `DataSourceExec` + `ParquetSource` with Oracle's
    reader factory and the `HotScanExec` wrapper;
  - published files are read by iceberg's `ArrowReader` with batched ranges;
  - the memory charge is taken per merged range before IO.
- **B4. Task file (`TASK-003-shredding-and-leaf-pruning.md`):**
  - Frontmatter `spec_revision: 25`.
  - Correct the stale notes:
    - "Added A … uncommitted" and "Added B … Uncommitted": both are
      committed;
    - Scenario 2 "Implemented, not pushed": the pins are pushed;
    - "Known gap … REQ-023's output leaf projection is still open": closed by
      `fdc6fd5ff`.
  - Replace the benchmark evidence table with the final three-way table.
  - Add the new-items table: `OracleParquetReaderFactory`, `HotScanExec`,
    `scan_partitions`, the A2 bench changes, each with the owners searched.
- **B5. Not in TASK-003:** the merge's open follow-up, moving JSON-row →
  Arrow conversion from `wyrd-queue/src/batch_builder.rs` to `wyrd-types`.
  Record it as a follow-up in the task file. It is unrelated to shredding or
  latency and touches the ingest path. The user decides.
- **B6. Verify:** L1–L7, then L8 once as the last command. Record exit codes
  and test counts in the task file.
- **B7. Review:** a fresh `$wyrd-task-review` of the cumulative candidate
  against TASK-003, spec revisions 18–25 and this revision. Fix-required
  findings are fixed before B8.
- **B8. Commit and push** to `wyrd/bifrost-variant/TASK-003`:
  - one commit per A step, B1 folded into A2, and B3/B4 as the last commit;
  - local identity; each message ends with the session line;
  - push with the scratchpad `lfs` PATH prefix.

## Done when

- Every lane L1–L8 passes.
- The Outcome holds on the final L6 run, for every case:
  - `_struct` ≤ `_datafusion`;
  - `_variant_shredded` < `_variant_unshredded`;
  - `_variant_shredded` ≤ `_datafusion`.

  Within Criterion's reported noise, the interval overlaps.
- A missed case is reported with its profile and is not done; it is never
  hidden or excluded.
- Oracle-owned scan code is only Wyrd-specific implementation and DataFusion
  extensions (the rule above).

## Benchmark log

Columns are DataFusion / Struct / unshredded / shredded.

| After | narrow small | wide small | nested inner small | wide sum |
|---|---|---|---|---|
| A1 | – / 366 / – / 628 µs | – / 345 / – / 693 µs | – / 453 / – / 717 µs | – / 680 / – / 809 µs |
| A2 (noisy: a clippy build overlapped part of the run) | 304 / 446 / 3646 / 689 µs | 287 / 333 / 14944 / 618 µs | 292 / 414 / 6464 / 709 µs | 456 / 599 / 16676 / 769 µs |
