# TASK-003 r1 — follow-up discovery review (`followup-rev`)

Candidate `861a0342f` (base `b145767dd`). Authority: working-tree
`changes/active/bifrost-variant/spec.md` (revision 21) as amended by
`spec-revision-26-datafusion-variant.md`; governing plan
`tasks/TASK-003-native-datafusion-read-path-plan.md`. Forks read at their pins:
`datafusion-distributed` `0b81b12`, `datafusion` `a4ff14550`.

This review resolves three disputed points. It does not vote on other findings,
and it modified no source file.

---

## Uncertainty 1: does the worker tenant binding refuse foreign files before execution?

### Sources inspected

- `spec.md:336-340` (Analytical distribution, "what crosses the wire").
- `spec-revision-26-datafusion-variant.md:140-142, 218-222`. These amend
  REQ-025 and remove the Variant codec bullet. Only the tenant footer proof
  (`WorkerPlanRewriteHandler`) remains, "reattached before execution".
- `spec.md` INV-006 (`:1108-1111`).
- Plan Step 3 item 4 (`:55-56`) and design note (`:564-568`).
- `crates/vala/vala-bifrost-redux/src/oracle/exec.rs`:
  `TenantParquetReaderFactory` / `TenantParquetReader::get_metadata`
  (`:786-895`), `TenantWarehouseReads::{reader_factory, bind}` (`:897-988`), and
  the unit test `decoded_stage_scans_read_only_as_the_bound_tenant`
  (`:2606-2655`).
- `oracle/analytical.rs:536-555`, which installs the handler with the verified
  stage tenant.
- `catalog/tenant_table.rs:176-181`, where the tenant object prefix is
  `tenants/{tenant}/{segment}/{table}`.
- `catalog/iceberg_storage.rs:112` (`relativize`).

### Evidence

- **What the spec requires.** `spec.md:338-340` says: "the tenant footer
  proof, reattached by a `datafusion-distributed` `WorkerPlanRewriteHandler`
  that **refuses any decoded file outside the authenticated tenant's warehouse
  prefix before execution**." Revision 26 does not touch that sentence.
- **The two obligations.** The sentence imposes two things:
  1. The footer proof is reattached on the worker.
  2. The rewrite handler refuses foreign-prefix files at rewrite time, before
     any execution.

  The reviewers who read only (1) as "the spec's mechanism" left out the
  relative clause.
- **What the candidate does.** `bind` refuses only:
  - a non-Parquet file scan;
  - an `object_store_url` other than `local_filesystem`.

  Otherwise it swaps in `TenantParquetReaderFactory` and returns. It never
  reads `config.file_groups` or `PartitionedFile::object_meta.location`, so no
  path is compared with `tenants/{tenant}/`.
- **The test pins the opposite behavior.** The unit test asserts it directly:
  `.bind(decoded).expect("a foreign tenant binds")`. The refusal arrives only
  from `execute(...)` → `stream.next()`, after `get_metadata` has fetched the
  footer.
- **How much isolation still holds.** Rows remain isolated:
  `TenantParquetReader::get_metadata` proves the footer tenant before any row
  group is decoded. However:
  - A foreign tenant's object is still opened, and its footer bytes are read,
    on the worker under the authenticated tenant's query. The refusal comes
    after that I/O.
  - The refusal is classified as an execution-time `QueryTenantInvariant`, not
    a pre-execution plan refusal.
  - Exploiting this needs a leader that ships a foreign path inside a stage
    plan that passes signing and digest checks, for example a compromised or
    buggy coordinator. That is exactly the case defence-in-depth exists for.

### Proposed finding

**FUP-1 — MISSING**

- **Obligation:** `spec.md:338-340` (REQ-025's distributed read path), with
  INV-006 (tenant tripwires unchanged).
- **Location:** `crates/vala/vala-bifrost-redux/src/oracle/exec.rs:948-973`
  (`TenantWarehouseReads::bind`). The test that pins the opposite behavior is at
  `exec.rs:2639-2643`.
- **Evidence:** as above. No file-location check exists anywhere on the worker
  rewrite path.
- **Consequence:**
  - The pre-execution refusal the spec promises does not exist.
  - A foreign tenant's footer is fetched on a worker before the query fails.
  - The handler's documented contract is broader than the code.
- **Testable correction:**
  - In `bind`, walk every `PartitionedFile` of every file group. Refuse with
    `BifrostError::QueryTenantInvariant`, wrapped in `QueryCatalogError` so the
    code survives the worker hop, when its location is not under the
    authenticated tenant's prefix (`tenants/{tenant}/`, derived from the same
    owner as `TenantTableBinding`'s `object_prefix`).
  - Change the unit test to expect `bind` itself to fail for the foreign
    tenant. Write the fixture under `tenants/{FIXTURE_TENANT}/…`.
  - Keep the footer proof as the second tripwire.
  - Alternative: get a human-approved spec amendment that states refusal
    happens at footer open. That is the only other way to close this.

**Status: RESOLVED.** The spec requires a pre-execution prefix refusal, and the
candidate does not implement it. The footer proof alone satisfies only the
"reattached" half.

---

## Uncertainty 2: is "one published object is one task" the real reason one-object tables stayed Interactive?

### Sources inspected

- `datafusion-distributed@0b81b12`:
  - `src/events/defaults/file_scan_config.rs:13-34` (desired task count) and
    `:36-82` (leaf scale-up);
  - `src/distributed_planner/inject_network_boundaries.rs` (boundary
    triggers, `max_tasks`, `limit` / `merge`);
  - `src/distributed_planner/distributed_query_planner.rs:139-171`;
  - `src/distributed_planner/prepare_network_boundaries.rs:30-31`;
  - `src/distributed_planner/distributed_config.rs:15, 47`.
- `datafusion@a4ff14550`:
  `datafusion/physical-optimizer/src/ensure_requirements/enforce_distribution.rs:996-1050, 1530-1620`.
- Wyrd sources:
  - `oracle/analytical.rs:7023-7055` (`planning_session` sets
    `set_distributed_file_scan_config_bytes_per_partition` when
    `file_scan_bytes_per_task` is `Some`) and `:7179-7204` (worker URLs are
    the remote participants only);
  - `wyrd-testing/src/bifrost/cluster.rs:1823-1829` (the harness defaults the
    value to `1`; this was added in the same commit `029da306f` as the
    two-object fixtures);
  - `resources.rs:4103-4116` (`use_row_number_estimates_to_optimize_partitioning = true`);
  - `oracle/exec.rs:1040-1110` (`hot_parquet_scan` uses
    `FileGroup::split_files(target_partitions)` and an Inexact per-file row
    count);
  - `oracle/exec.rs:1555-1590` (provider `scan`).
- Plan evidence at `:612-625`.

### Evidence

1. **Task count is not the limiter.** The upstream task count is
   `ceil(ceil(total_bytes / bytes_per_partition) / target_partitions)`. With
   the harness value of `1`, a one-object, KB-sized table desires
   `ceil(size_bytes / target_partitions)` tasks. That is far above one for any
   real Parquet object. It is then clamped to `max_tasks`, which equals the
   number of remote participant URLs.

   So the reviewer is right: under the harness configuration the
   *desired task count* of a one-object scan is greater than one whenever there
   are two or more remote Oracles. "One published object is one task" is not
   what upstream computes.
2. **The real cause: a one-object plan has no network boundary.** A stage
   boundary is only created at:
   - a hash `RepartitionExec`;
   - a `BroadcastExec`;
   - a child of `CoalescePartitionsExec` / `SortPreservingMergeExec`.

   If no boundary survives, the planner returns `original_plan` and no
   `DistributedExec` is created (`distributed_query_planner.rs:160-162`). For a
   one-object table:
   - `hot_parquet_scan` builds `FileGroup::new(files).split_files(target_partitions)`,
     which gives one group and therefore one output partition.
   - The row count is `Inexact(12)`, and Wyrd sets
     `use_row_number_estimates_to_optimize_partitioning = true`. So
     `roundrobin_beneficial_stats = false` (`enforce_distribution.rs:1017`),
     which blocks both the round-robin repartition and
     `repartition_file_scans`.
   - `hash_necessary = is_hash && multi_partitions` is false for one
     partition (`:1035`).
   - The whole plan stays single-partition. The distributed planner adds no
     `CoalescePartitionsExec` (partition count is 1), and no
     boundary-triggering node exists. Root: not `DistributedExec`, so the class
     is Interactive.
   - With two objects there are two file groups and two partitions. A grouped
     statement then gets a hash repartition (shuffle boundary), and a scan gets
     a coalesce boundary. The bytes-based desired count (≥2 under the harness)
     keeps the boundary, and the root is `DistributedExec`.
3. **Verdict on the recorded cause.**
   - The symptom (no `DistributedExec` root for 12-row, one-object tables) and
     the fix site (fixtures publish two objects) are correct and explained.
   - The stated mechanism is wrong. The cause is that one object means one
     file group, which with row-estimate partitioning means a single-partition
     plan with no stage boundary. It is not that one object is one task.
   - `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs:640-641` states
     the correct mechanism ("a single-partition leaf is leader-executable").
     Several other journey docs state the wrong one.
   - One more consequence: if a one-object table's statistics were `Absent`,
     or its row estimate exceeded `batch_size`, it would distribute. "Class
     follows object count" is therefore not a general rule.

### Proposed finding

**FUP-2 — INCORRECT (low; documentation correctness under AGENTS.md §16)**

- **Obligation:** AGENTS.md §16 requires accurate rustdoc. The plan's evidence
  record is the governing artifact.
- **Location:**
  - `wyrd-testing/tests/bifrost/oracle/analytical_lifecycle.rs:105-109`
    ("the harness plans one task per published object");
  - `analytical_activation.rs:744-746`;
  - `capacity.rs:758-762, 824, 1819`;
  - plan `:618-621`.
- **Consequence:** Maintainers are told task count drives class. A future
  change, such as a non-`Inexact` row count, `Absent` statistics, or a disabled
  row-estimate option, would silently change journey classes against these
  comments.
- **Testable correction:** Reword these docs and the plan evidence to the
  single-partition / no-boundary mechanism. No behavior change is needed.
  Optional: a planner-level unit test that plans a one-object vs two-object
  scan under the analytical planning session with two worker URLs, and asserts
  `query_class_for_root` is Interactive vs Analytical. That pins the real
  mechanism.

**Status: RESOLVED.** The diagnosis's mechanism is wrong, but its symptom and
fix are valid. The reviewer's objection is correct about task count. It does
not show the fix is unproven.

---

## Uncertainty 3: are Analytical scan statistics wrong?

### Sources inspected

- `oracle/exec.rs`:
  - `parquet_pruning` (`:105-126`);
  - `RemoteScanMetrics` (`:167-260`);
  - `OracleQueryScanStats::{finalize, visit, open_distributed_scan}` (`:262-394`);
  - `record_distributed_scan_metrics` / `fold_distributed_scan_metrics`
    (`:433-484`);
  - `distinct_files` (`:1592-1604`);
  - unit test `scan_stats_exclude_files_distributed_tasks_pruned`
    (`:1969-1983`).
- `oracle/mod.rs:2322-2376`: `from_plan(root)` runs on the pre-execution
  `DistributedExec` root, and the fold is retained.
- `oracle/mod.rs:636-680`: terminal emission to the
  `oracle_query_{files,partitions,row_groups,bytes}_*_total{class}` counters.
  These counters are the only consumer; there is no terminal frame field.
- `oracle/analytical.rs:2540-2560, 7435-7500`: fold and settle.
- Upstream sources:
  - `NetworkShuffleExec` / `NetworkCoalesceExec` / `NetworkBroadcastExec::children()`
    (`network_shuffle.rs:189-194`, `network_coalesce.rs:210-215`,
    `network_broadcast.rs:213-218`), which return `input_stage.local_plan()`;
  - `DistributedLeafExec::children()` (`distributed_leaf.rs:159-161`), which
    returns `[]`;
  - `DistributedExec::children()` (`coordinator/distributed.rs:155-157`);
  - `rewrite_distributed_plan_with_metrics` (`task_metrics_rewriter.rs:44-96, 158-185`);
  - `MetricsWrapperExec::children()` (`metrics.rs:56-58`);
  - `as_network_boundary` (`network_boundary.rs:61-71`, concrete downcasts
    only).
- Base `exec.rs` `visit` at `b145767dd`.

### Evidence

**1. `visit` double-descends every `Stage::Local` boundary.** This is
definite and reachable on every Analytical query.

- `visit` descends `boundary.input_stage()` when it is `Stage::Local`, and then
  also walks `plan.children()`. Upstream's `children()` for all three network
  boundaries returns that same `Stage::Local` plan.
- `from_plan` runs on the planned root before execution. Every boundary there
  is still `Stage::Local`, because the planner builds `LocalStage`.
- Each leaf below *k* nested boundaries is therefore visited 2^k times.
  `files_scanned`, `partitions_scanned`, and the pushed `physical_metrics`
  handles are all multiplied:
  - plain scan → `NetworkCoalesce`: ×2;
  - `GROUP BY` → coalesce over shuffle: ×4.
- The candidate's own `PhysicalEvidenceWalk` documents this hazard and guards
  it with a `seen` set (`exec.rs:545-549`: "A `Stage::Local` boundary and its
  ordinary child are the same node … would count one sort or join twice").
  `visit` has no such guard.
- In base, `visit` had no boundary descent. Distributed file counts came from
  the follower-reported handles. So this is new in the candidate (plan Step 3
  item 7).

**2. The distributed fold does not double count.** After
`rewrite_distributed_plan_with_metrics`, every boundary is wrapped in
`MetricsWrapperExec`. `as_network_boundary` is a concrete downcast, so it
returns `None` on the wrapper, and the wrapper's `children()` is the stage plan.
`fold_distributed_scan_metrics` therefore reaches each stage once. Its bytes and
pruning are single-counted for remote stages.

**3. Files are counted once per task variant per file, while pruning is
counted per range.**

- `file_scan_config_scale_up_leaf_node` repartitions with
  `with_repartition_file_min_size(0)` into `partition_count × task_count`
  groups. That splits single objects into byte ranges spread across the
  variants.
- `distinct_files` deduplicates only within one variant. A file whose ranges
  land in *n* variants counts *n* times, even before the ×2^k from (1).
- `files_pruned` comes from `files_ranges_pruned_statistics`, which counts
  *ranges*.
- `finalize` then computes `files_scanned − files_pruned` with
  `saturating_sub` across mismatched units. A file pruned in *r* ranges but
  counted in *n* < *r* variants subtracts too much, which erases other files
  that really were opened.
- The doc comment "A file split into several ranges counts once per range on
  both sides" (`exec.rs:303-304`) is false for the plan side.

**4. Head-stage scans probably double count bytes and pruning.** Reachable but
narrow; not proven by execution.

- When a single-task producer stage sits directly under the single-task head,
  `prepare_network_boundaries.rs:30-31` collapses it into the head, and the
  coordinator runs that `DataSourceExec` itself. One example is the persisted
  sibling that `LiveUnionBoundary` wraps in `CoalescePartitionsExec`, for a
  one-object table joined with a distributed one.
- Its `file_source` metrics set is shared by `Arc` with the planned node.
  `finalize` sums it from `physical_metrics`, and
  `collect_plan_metrics(head_stage)` also feeds it into the fold, which records
  it into the same terminal through `RemoteScanMetrics`. The result is
  `bytes_scanned`, row-group pruning, and `files_pruned` all counted twice for
  that leaf.
- Confirm this with the test in the correction below before treating it as
  established.

**5. No test pins Analytical counts.**

- Exact `expect_measure(... "files" ...)` journeys
  (`distributed.rs:3237-3532`, `published.rs:1390`) use single-Oracle
  topologies. With `max_tasks` = 0 remote URLs → 1 there is no boundary, so they
  exercise only Interactive.
- Analytical file assertions are bounds that a multiplier passes:
  `distributed.rs:316` (`unfiltered_files < 3.0`) and the strict-less
  comparison at `:353`.
- The only unit test (`scan_stats_exclude_files_distributed_tasks_pruned`)
  builds a synthetic `OracleQueryScanStats` and never calls `visit` on a
  distributed plan.

**6. Spec obligation.**

- Revision 26 replaces AC-007's span from "while both produce …" through
  "expression.", which includes the sentence "Oracle metrics prove …,
  including in a distributed query". The surviving AC-007 asks only that
  DataFusion reads the declared leaves on distributed scans.
- No REQ, INV, or AC in the approved spec fixes the meaning of
  `oracle_query_files_scanned_total` / `partitions_scanned_total`.
- The violated obligations are:
  - the governing plan's Step 3 item 7: "Distributed scan metrics count files
    and partitions again";
  - the field contract at `exec.rs:274-279`: "Number of files executed file
    scan nodes opened: the files they were planned to read, less those
    `FilePruner` skipped";
  - base behavior, where follower-reported counts were per participant and
    not multiplied.
- No approved-spec REQ is violated.

### Proposed findings

**FUP-3 — REGRESSION**

- **Obligation:** plan Step 3 item 7, and the `OracleQueryScanStats` field
  contract.
- **Location:** `crates/vala/vala-bifrost-redux/src/oracle/exec.rs:345-366`
  (`visit`: the `as_network_boundary` descent plus the `children()` loop).
- **Evidence:** points (1) and (5) above.
- **Consequence:** Every Analytical query's `files_scanned` and
  `partitions_scanned` are multiplied by 2^(boundary depth). Operators reading
  `oracle_query_*_scanned_total{class="analytical"}` see inflated I/O. The
  `files_scanned − files_pruned` subtraction then mixes a multiplied count with
  a single-counted prune total.
- **Testable correction:**
  - Descend a `Stage::Local` boundary through exactly one edge. Either drop the
    explicit boundary branch, since `children()` already yields the local
    stage, or skip `children()` for boundaries.
  - Add a unit test that plans a two-object table under a planning session with
    two worker URLs, then asserts `from_plan(root).files_scanned == 2` and
    `partitions_scanned` equals the per-task group total.
  - Add an Analytical journey that asserts `oracle_query_files_scanned_total`
    equals the object count exactly.

**FUP-4 — INCORRECT**

- **Obligation:** the same field contract, and the doc at `exec.rs:301-304`.
- **Location:** `exec.rs:1592-1604` (`distinct_files` per variant) and
  `finalize` `:330`.
- **Evidence:** point (3) above.
- **Consequence:** Under task scale-up a file is counted once per task that
  holds a range of it. Range-granular prune counts are subtracted from
  variant-granular file counts, so `files_scanned` can be over-reported or
  zeroed.
- **Testable correction:**
  - Count distinct object locations across *all* variants of one
    `DistributedLeafExec`, not per variant.
  - Either report pruned files as distinct locations whose every range was
    pruned, or count both sides per range and document "file ranges".
  - Unit test: one file split across two variants, one range pruned. Assert the
    count is deterministic and documented.

**FUP-5 — INCORRECT (probable; confirm by test)**

- **Obligation:** the `RemoteScanMetrics` / `OracleQueryScanStats` contract
  that the leader is the single emitter, with no double count.
- **Location:** `exec.rs:309-331` (`finalize` sums `physical_metrics` and the
  fold sink) together with `exec.rs:433-452` (the fold includes head-stage
  nodes).
- **Evidence:** point (4) above.
- **Consequence:** A distributed query whose persisted scan collapsed into the
  coordinator's head stage reports doubled `bytes_scanned` and pruning. It also
  subtracts `files_pruned` twice.
- **Testable correction:**
  - In `record_distributed_scan_metrics`, fold only nodes below a boundary
    (skip the head stage). Alternatively, have `visit` not push
    `physical_metrics` for a `DistributedExec` root.
  - Test: an Analytical join of a two-object table with a one-object
    live+persisted table; assert `bytes_scanned` equals the sum of the objects'
    read bytes.

**Status: RESOLVED for FUP-3 and FUP-4. Narrowed for FUP-5,** which is
reachable by source reading but not executed. No approved-spec REQ or AC is
violated. The defects violate the governing plan's item 7 and the code's own
documented counter contract, and FUP-3 regresses base behavior.

---

## Not inspected

- Whether the telemetry `finish()` runs before the Analytical metric fold
  settles. If it does, the remote bytes and pruning recorded by the fold would
  miss the terminal counters entirely. That is a separate question and was not
  traced here.
