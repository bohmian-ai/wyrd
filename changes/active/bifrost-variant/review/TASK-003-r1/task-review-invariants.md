# TASK-003 r1 — invariant review (`invariant-rev`)

- Base `b145767dd`, candidate `861a0342f` (working-tree `spec.md` and
  `TASK-004` excluded from the candidate).
- Authority: `spec.md` (working tree) and `spec-revision-26-datafusion-variant.md`.
  On conflict, `tasks/TASK-003-native-datafusion-read-path-plan.md` wins.
- Lens: values and state traced from producer to sink, across shared owners,
  sibling consumers, lifecycle transitions, and failure paths.

## Flows traced

| Flow | Producer | Sink | Result |
|---|---|---|---|
| Tenant identity | admission → `egress.tenant(key)` (verified identity) in `analytical.rs` `WorkerSessionBuilder::build_session_state` | `TenantWarehouseReads::new` → `set_distributed_worker_plan_rewrite_handler` → `TenantParquetReaderFactory::get_metadata` footer proof | Identity flows intact. Pre-execution prefix refusal is missing (INV-4). |
| Pushed predicates (live) | `FilterPushdown` → `LiveTableRoutes::handle_child_pushdown_result` (`live.rs`) | `fragment()` encodes `FilterExec(pred, RemoteSourcePlaceholderExec)`, signed digest; follower `FilterPushdown` after decode (`follower.rs:774`) | PASS |
| Route selection by event time | per-route time-partition statistics | `LiveTableRoutes::selected_by` (`PruningPredicate`) | PASS |
| Scan statistics | `DataSourceExec` / `ParquetFileMetrics`, `files_ranges_pruned_statistics` | `OracleQueryScanStats::visit` / `finalize` → telemetry `finish` (`mod.rs` ~640-690) → terminal frame | Counts are wrong on Analytical plans (INV-2). |
| Memory reservations and metric fold | `retain_metric_fold`, grants | `AnalyticalGraphLifecycle::settle` (`analytical.rs` ~2585-2660): fold → join → `close_exchanges` → drop fold → drop grants → release | PASS |
| Query class | plan root | `query_class_for_root` (logic unchanged) | PASS |
| Overall scan statistics | `compute_all_files_statistics(.., true, true)` in `hot_parquet_scan` | aggregate planning | Inexact, so there is no `count(*)` statistics bypass. PASS |

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | PASS/FAIL |
|---|---|---|---|
| One read path: `DataSourceExec` over `ParquetSource`; predicates placed only by `FilterPushdown` | `exec.rs` `hot_parquet_scan` (~1000-1102); `published_files` → `DataSourceExec`; Wyrd pushdown rules deleted | `test:bifrost` green per the plan's step 4/5 evidence (commit `0eb07ce57`) | PASS |
| Deleted code must not return (revision 21: no persisted follower assignment) | `PublishedAssignment` and the `assigned` branch of `published_files` reintroduced (`exec.rs:1111-1208`) | none; the branch is unreachable | **FAIL** (INV-1) |
| Distributed execution is native datafusion-distributed with no Wyrd-invented logic | `analytical.rs` built on the `0b81b12` fork | Analytical journeys in `distributed.rs` | PASS |
| Tenant footer proof on the worker | `TenantParquetReaderFactory`, `TenantWarehouseReads::bind` (`exec.rs` ~793-990) | footer-proof journeys | PASS (open-time proof) |
| Worker refuses decoded files outside the tenant warehouse prefix before execution (`spec.md:340`) | `bind` checks only the source type and `ObjectStoreUrl` | no test | **FAIL** (INV-4) |
| Live fragment predicate is carried in the signed digest | `live.rs` `fragment()`; `follower.rs:774` | distributed predicate journey (inequality assertions) | PASS |
| Event-time file and route pruning | `set_event_time_bounds` (`exec.rs:1286`), `HotFileSource::statistics` (`exec.rs:1264`), `selected_by` | filtering journeys | PASS |
| Distributed scan metrics are exact (`OracleQueryScanStats`, telemetry, terminal frame) | `exec.rs:346` `visit`, `exec.rs:303` `finalize`, `exec.rs:464` `fold_distributed_scan_metrics` | Only Interactive plans have exact-count tests; the unit test sets fields directly | **FAIL** (INV-2) |
| Variant querying is DataFusion's `variant` feature; Wyrd Variant code deleted | fork `a4ff14550`; `variant_sql.rs`, `leaf_paths.rs`, and related files deleted | `test:bifrost` | PASS |
| No `parquet-variant*` dependency in `vala-bifrost-redux` (revision 26, material constraints) | `vala-bifrost-redux/Cargo.toml:60-61` keeps both crates | — | **FAIL** (INV-6) |
| Peer fingerprint binds the DataFusion fork revision; a mismatched worker refuses before decoding (revision 26, lines 137-138) | `ORACLE_VARIANT_SQL_VERSION` removed from `codec.rs:27` and `peer.rs:312`; nothing replaces it | none | **FAIL** (INV-3) |
| Wire/contract removal regenerates cleanly (AC-010: `codegen:check`, `check:proto-drift`) | `ScanPredicate` removed from `.proto`; `PersistedFileDescriptor` removed from `wyrd-spec/src/vala/api.rs` | The checked-in `wyrd-tonic/proto/wyrd.v1.bin` still contains `ScanPredicate`; `codegen:check` was not recorded | **FAIL** (INV-5) |
| Performance gates (revision 26 latency table) | native path | hot `select_wide` 10023 µs vs prior 9488-10106 µs; `published count_star` 621 µs (statistics inexact, not a bypass) | PASS (for this lens) |
| Test changes made to clear failures carry a diagnosis | `capacity.rs` `held_floor` now subtracts the idle `governed_memory_used_bytes` (commit `4559cff37`) | Plan step A4 records that the node-wide `FileMetadataCache` is charged up front to the Oracle root, which raises idle governed usage | PASS (design-caused, recorded) |
| Single-flight cache journey assertions removed | `storage/cache.rs` deleted (revision 20, plan A4) | — | PASS |

## Findings

### INV-1 — DRIFT (deleted concept returned as dead code)

- **Violated obligation:** revision 21 (one read path) deletes persisted
  follower assignment. Non-goal: deleted code must not return. AGENTS.md §15
  forbids scaffolding for hypothetical reuse.
- **Location:** `crates/vala/vala-bifrost-redux/src/oracle/exec.rs:1111-1208`
  (`PublishedAssignment`, its alias map, `location()`, the `assigned` and
  `assignment_drift` branches of `published_files`). The only caller is
  `exec.rs:1503`, which passes `published_files(published, None)`.
- **Evidence:** the candidate adds the type and the `Option<&PublishedAssignment>`
  parameter. No call site constructs `Some(..)`.
- **Observable consequence:** an unreachable, untested assignment/alias
  enforcement path exists that looks like a live security check. A maintainer
  could believe published reads are bound to an assignment when they are not.
- **Testable correction:** delete `PublishedAssignment` and the parameter;
  `published_files(scan)`. Check: `grep -n PublishedAssignment crates/` returns
  nothing, and `test:bifrost` stays green.

### INV-2 — INCORRECT (Analytical scan statistics double-count and mix units)

- **Violated obligation:** exact distributed scan metrics feed
  `OracleQueryScanStats`, the `oracle_query_*` telemetry counters, and the
  terminal frame.
- **Locations:**
  - `exec.rs:346-380`, `OracleQueryScanStats::visit`;
  - `exec.rs:464`, `fold_distributed_scan_metrics`;
  - `exec.rs:296-303`, `finalize` and its doc.
- **Evidence:**
  1. `visit` descends into `as_network_boundary().input_stage()` `Stage::Local`
     **and** into `plan.children()`. In the datafusion-distributed fork,
     `NetworkCoalesceExec` / `NetworkShuffleExec` / `NetworkBroadcastExec`
     `children()` return that same `Stage::Local` plan. `visit` keeps no `seen`
     set (unlike `PhysicalEvidenceWalk`, whose doc states that the boundary and
     its child are the same node). So each subtree below a boundary is counted
     twice, and nested boundaries compound it.
  2. `DistributedLeafExec` variants are per-task scans. Each variant is visited
     and its files counted, so one file assigned to several task variants is
     counted per variant.
  3. `fold_distributed_scan_metrics` follows the same double edge. The new
     `files_pruned` fold therefore also doubles.
  4. `finalize` subtracts `pruning.files_pruned`, which is in
     `PartitionedFile`-range units (`files_ranges_pruned_statistics`), from
     `distinct_files`, which is deduplicated by location. The doc ("counts once
     per range on both sides") contradicts the code. When a file is
     range-split (`file_scan_bytes_per_task`, set to 1 in
     `wyrd-testing/src/bifrost/cluster.rs:1829`), the subtraction undercounts
     and can saturate to zero.
  5. The unit test `scan_stats_exclude_files_distributed_tasks_pruned` sets
     fields directly and never exercises `visit` or the fold. The exact-count
     journeys (`distributed.rs` filtering, `published.rs`) run on a single
     Oracle node and are therefore Interactive. The predicate journey asserts
     only inequalities.
- **Observable consequence:**
  - files scanned, partitions, and files pruned reported to callers and to
    telemetry are inflated (×2 per boundary level, ×task variants) or
    undercounted on Analytical queries;
  - the pruning efficacy claims of AC evidence are unverified on the
    distributed path.
- **Testable correction:**
  - walk each node once: either skip `children()` on network boundaries, or
    keep a node-identity `seen` set as `PhysicalEvidenceWalk` does;
  - deduplicate file locations across `DistributedLeafExec` variants;
  - compute pruned and scanned in the same unit (both ranges, or both distinct
    locations) and fix the doc;
  - add an Analytical journey (two or more Oracle nodes,
    `file_scan_bytes_per_task = 1`) over a table with a known file count and a
    predicate that prunes a known number of files, asserting exact
    `files_scanned`, `files_pruned`, and partitions in the terminal frame.

### INV-3 — MISSING (DataFusion fork revision not bound into the peer contract)

- **Violated obligation:** revision 26 (`spec-revision-26-datafusion-variant.md:137-138`):
  "peer contract fingerprint includes the DataFusion fork revision, and a
  worker on a different revision refuses before decoding a plan."
- **Location:** `crates/vala/vala-bifrost-redux/src/oracle/codec.rs:27`
  (`physical_plan_fingerprint`) and `oracle/peer.rs:312` (`stage_body_digest`).
- **Evidence:**
  - `ORACLE_VARIANT_SQL_VERSION` was removed from both inputs;
  - no DataFusion revision constant replaces it (`grep` for `a4ff14550`,
    `datafusion_revision`, or `fork_revision` under `crates/` returns nothing);
  - `STAGE_PROTOCOL_VERSION = 2` and `ORACLE_PHYSICAL_CODEC_VERSION = 1` are
    unchanged.
- **Observable consequence:** during a rolling upgrade, a worker built against
  a different DataFusion fork accepts and decodes plans whose `variant`
  operator encoding or semantics may differ. The result is decode failures or
  silently divergent results, instead of a pre-decode refusal.
- **Testable correction:**
  - bind the resolved fork revision (for example, a build-time constant derived
    from the `Cargo.lock` git source of `datafusion`) into
    `physical_plan_fingerprint` and the peer capability/handshake;
  - add a unit test showing that a fingerprint computed under a different
    revision differs, and a peer test showing that a mismatched worker refuses
    before `try_decode`.

### INV-4 — MISSING (no pre-execution tenant-prefix refusal on the worker)

- **Violated obligation:** `spec.md:340` (revision 21): the worker
  `WorkerPlanRewriteHandler` "refuses any decoded file outside the
  authenticated tenant's warehouse prefix before execution."
- **Location:** `exec.rs:948`, `impl WorkerPlanRewriteHandler for TenantWarehouseReads::bind`.
- **Evidence:** `bind` rejects only:
  - a non-Parquet file source;
  - an `ObjectStoreUrl` other than `local_filesystem`.

  It reattaches the reader factory but never inspects `FileScanConfig` file
  groups against the tenant warehouse prefix. The tenant check happens only
  lazily in `TenantParquetReaderFactory::get_metadata` when a file is opened.
- **Observable consequence:**
  - a decoded plan naming another tenant's paths is not refused before
    execution;
  - the refusal surfaces mid-stream, after other partitions may have started
    or emitted rows;
  - files never opened (for example, after LIMIT short-circuit) are never
    checked;
  - the refusal is not the spec's pre-execution, whole-plan refusal.
- **Testable correction:**
  - in `bind`, walk every `DataSourceExec`/`FileScanConfig` file group and
    return a tenant-violation error if any `PartitionedFile` path is outside
    the tenant's warehouse prefix, before returning the plan;
  - add a worker unit test feeding `bind` a plan with one foreign-tenant file
    and asserting refusal with no reader opened.

### INV-5 — REGRESSION (stale protobuf descriptor; AC-010 gates unproven)

- **Violated obligation:** AC-010 requires `codegen:check` and the repository
  drift gates. AGENTS.md §12 requires that public contracts regenerate cleanly
  when touched.
- **Location:** `crates/wyrd/wyrd-tonic/proto/wyrd.v1.bin`, last changed in
  `47bdd88dc`, before this task. `ScanPredicate` was removed from the `.proto`.
  `PersistedFileDescriptor` and related schemars/utoipa types were removed from
  `crates/wyrd-spec/src/vala/api.rs`.
- **Evidence:**
  - `strings wyrd.v1.bin` still finds `ScanPredicate` (4 occurrences); the
    edited `.proto` has 0;
  - `scripts/checks/proto-drift.sh` (`mise.toml:1563`, `check:proto-drift`)
    regenerates the `.bin` and fails on any diff;
  - no `codegen:check` run is recorded in the plan evidence.
- **Observable consequence:**
  - `check:proto-drift` is red on the candidate;
  - gRPC reflection and descriptor consumers still advertise the removed
    message;
  - generated schema artifacts for the removed `wyrd-spec` types may be stale.
- **Testable correction:** regenerate `wyrd.v1.bin` and any codegen outputs,
  commit them, and record green `mise run check:proto-drift` and
  `mise run codegen:check`.

### INV-6 — DRIFT (spec constraint contradicts the implementation)

- **Violated obligation:** revision 26 material constraints
  (`spec-revision-26-datafusion-variant.md:234-235`): "no `parquet-variant*`
  dependency in `vala-bifrost-redux`".
- **Location:** `crates/vala/vala-bifrost-redux/Cargo.toml:60-61`
  (`parquet-variant`, `parquet-variant-compute`).
- **Evidence:**
  - the plan (item 11) keeps both because the Scribe writer shreds with them;
  - the approved spec text was not amended to scope the constraint to the read
    path;
  - the plan cannot override an approved material constraint without a spec
    revision.
- **Observable consequence:** the authoritative spec and the code disagree.
  Final change review cannot map this constraint to evidence, and a future
  implementor following the spec would break the writer.
- **Testable correction:** either
  - amend the spec (a revision note scoping the ban to Oracle read code, with
    human approval); or
  - move the writer's shredding dependency out of `vala-bifrost-redux`.

  Then check: `cargo tree -p vala-bifrost-redux -e normal | grep parquet-variant`
  matches the amended text.

## Withdrawn candidate

- `capacity.rs` `held_floor`: the assertion change (commit `4559cff37`) follows
  from the recorded plan step A4. That step charges the node-wide
  `FileMetadataCache` up front to the Oracle root, so idle governed usage is
  non-zero by design. It is not a failure-clearing change without a diagnosis.

## Overall verdict

**FAIL** — INV-1, INV-2, INV-3, INV-4, INV-5, INV-6.
