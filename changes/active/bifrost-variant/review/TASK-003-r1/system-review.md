# TASK-003 r1 — System Resilience Review

- Reviewer: `system-rev`, an independent reviewer working under `$wyrd-task-review`.
- Subject: base `b145767dd` to candidate `861a0342f`.
- Authority:
  - the working-tree `spec.md` revision 26, with `spec-revision-26-datafusion-variant.md`;
  - `tasks/TASK-003-native-datafusion-read-path-plan.md`.
- Method: read-only, using `git show` and `git diff`. No source file was modified, and verification was not re-run.

**Verdict: FAIL**

## 1. Deployed Topology

This is derived from `architecture/bifrost-design.md`, `architecture/wyrd-design.md`, AGENTS.md §15 and `wyrd-server` boot.

- **Shapes:** self-hosted, SaaS multi-tenant (one logical serving surface) and enterprise single-tenant.
- **Replicas:** `wyrd-server` may run as several replicas or targeted pods (`WYRD_TARGET`) behind one gateway. Each replica activates a subset of these components:
  - Gate (ingest admission);
  - Scribe (WAL, live fragments, staging, commit);
  - Oracle (query leader and follower);
  - Forge (compaction and maintenance);
  - the audit publisher.
- **Oracle:** any Oracle replica can lead a query. Followers come from a participant cut that is frozen at plan time. A published Analytical stage runs on peers through native `datafusion-distributed`, and peers authenticate through the peer security digest and protocol versions. Scribe followers serve live fragments over the peer wire.
- **Storage:** after `BifrostStorage::new` (`crates/wyrd/wyrd-server/src/boot/mod.rs`), each node has one `object_store`. Its `LimitStore` permits are node-wide and shared by Scribe, Oracle, Forge and Iceberg (REQ-032/033/034). The Oracle footer cache (`with_metadata_cache`) is per node.
- **Durable state:**
  - Postgres holds the catalog, admission, audit staging and watermarks;
  - object storage holds Iceberg data and staged Parquet;
  - the local Scribe WAL holds unflushed rows.
- **In-memory state:** query sessions, footer caches, exchange channels and metric folds. All of these are lost on a crash by design.

## 2. Changed Runtime Paths and Failure-Path Evidence

### 2.1 Native datafusion-distributed Analytical execution and worker routing

Path:

1. `analytical.rs::planning_session` installs `AnalyticalWorkerResolver{urls}` over the frozen participants, with `LiveUnionBoundary`, `children_isolator_unions(false)` and an optional `set_distributed_file_scan_config_bytes_per_partition`.
2. In the fork `query_coordinator.rs`, `routed_urls` is computed once per stage (`prepare_static_plan`/`prepare_dynamic_plan`), through `single_task_coordinator_routing`, then `single_task_child_url_routing`, then `random_routing` (`events/defaults/routing.rs`).
3. Task counts come from `file_scan_config_desired_task_count`, bounded to 1..=budget by `TaskCountAnnotation::limit` (fork `8647526`).

Failure paths:

- **Follower crash mid-query.**
  - What stops: the stage stream to that worker errors. The coordinator surfaces the error, and the query fails visibly.
  - What stays available: other queries continue, and so does every other subsystem on surviving nodes.
  - Retries: nothing re-routes tasks mid-query, so a crash cannot loop. The leader's `settle` closes exchanges and drops the metric fold (`drop(self.supervisor.take_metric_fold(self.graph))` after `close_exchanges()`), so no supervisor state leaks. The client retries a whole new query, which re-freezes a new participant cut that excludes the dead peer once membership updates.
- **Peer unavailable at plan time.** The cut is frozen from the current roster. If a peer is listed but unreachable, the first dispatch fails and the query fails. This is no amplification: the fault costs one attempt per query.
- **Random routing.** Because routing is chosen once per stage, a stage cannot spray tasks across different URL sets on retry. This is not a finding.
- **Ceiling refusal.** Admission refuses before execution. See §2.6 for the IO that now happens before that refusal (SYS-2).
- **Rolling restart.** Peer protocol versions are unchanged: `PEER_PROTOCOL_VERSION = 5` (`oracle/dispatcher.rs:46`) and `STAGE_PROTOCOL_VERSION = 2` (`oracle/peer.rs:689`). The assignment-authority domain moved to `wyrd.oracle.assignment-authority.v9`. See SYS-1.

### 2.2 Leader graph settle, release and readiness

The `settle` fix (`analytical.rs`) releases the metric fold after `close_exchanges()`. That fold was the root cause the plan diagnosed for the hang in the MCP `max_rows: 1` journey.

- Effect: early-terminating consumers (limit, cancellation, dropped stream) now release the graph instead of pinning it.
- Proof: the 53/53 journeys in the recorded evidence cover it.
- Recovery: a cancelled query releases its exchanges and fold, and readiness is not held.

Not a finding.

### 2.3 Live Scribe fragment dispatch with pushed predicates

`live.rs`:

- `LiveScribeExec::handle_child_pushdown_result` takes non-dynamic parent filters in the Pre phase and reports every one as unsupported, so the parent `FilterExec` stays authoritative.
- `LiveTableRoutes::selected_by` prunes routes with `PruningPredicate`. If pruning cannot be evaluated, it keeps all routes (fail-open toward completeness, which is the correct direction).
- `fragment()` encodes `FilterExec(pred, RemoteSourcePlaceholderExec)`. The Scribe follower runs `FilterPushdown` after resolving the staged leaf.

Failure paths:

- **Scribe follower down.** Fragment dispatch fails and the query errors. Live rows are never silently omitted, because a route is only dropped by a proven prune.
- **Predicate decode failure on the follower.** It refuses visibly. A mixed-revision decode that does *not* fail is covered by SYS-1.
- **Fork gap, already known to the plan.** When a single-task `LiveScribeExec` stage sits under a multi-task consumer, it fails visibly; it does not return wrong rows. This is a recorded limitation, not a regression.

### 2.4 Worker plan rewrite for tenant reads

`analytical.rs::AnalyticalSessionBuilder::build_session_state` installs `TenantWarehouseReads` (`exec.rs` around 907–990), and `TenantWarehouseReads::bind` handles every decoded plan:

- it rewrites each decoded `DataSourceExec` to the `TenantParquetReaderFactory`;
- it refuses non-Parquet sources and any `ObjectStoreUrl` that is not local;
- it takes the tenant from `AnalyticalStageEgress::tenant(key)`, and a stage with no recorded identity is refused;
- it verifies the footer tenant on every `get_metadata`, including cache hits.

This is fail-closed at the stage boundary, and the refusal is per query: no node-wide state is poisoned, and later queries are unaffected. The path-prefix versus footer-proof question belongs to the security/tenancy domain reviewer. Not a resilience finding.

### 2.5 Storage and cache deletions

Deleted: the storage cache and its telemetry, the OpenDAL Forge store, and the storage close/abort at shutdown (`state.rs`).

- Scribe now uses `Arc<dyn object_store::ObjectStore>` (`scribe/{mod,persistence,staging_runtime,replay}.rs`). The change is mechanical. `ScribeStageMover` and recovery share one `Arc` instead of a cloned `Operator`, and WAL replay semantics are unchanged.
- Without the shutdown close/abort, an in-flight multipart upload on SIGTERM may be left partial. `parquet/object_uploader.rs` aborts on a failed write (around line 289) and documents the cancellation case (line 216).
  - Durable state is unaffected: the WAL still holds the rows, Scribe replay re-stages them, and the batch-id dedup fence absorbs the replay.
  - Orphan multipart parts are a cost and lifecycle-policy concern, not an integrity one.
  - Acceptable.

### 2.6 Planning-time footer IO (new in candidate `4559cff37`)

`exec.rs::persisted_input` (1489–1516) now awaits `cache_footers` (1435–1447) *during planning*. `oracle/mod.rs::run_sql_attempt` (2191+) runs `classify_one_build` (planning, which calls `build_physical_root` around 2859) **before** `admit_built_attempt` and `audit_and_bind`.

The base has no equivalent: `git show b145767dd:.../exec.rs` contains no footer prefetch. This is SYS-2.

### 2.7 Forge

`forge/compact.rs` (276–385), `ForgeObjects`:

- on non-Local backends, it lists in order with `list`/`list_with_offset` in chunks of `FORGE_OBJECT_LIST_PAGE_ENTRIES`;
- on Local, `unordered_pages` walks the whole tree for each page. That is O(n²) on large local trees, but self-hosted Local is a dev/single-node shape and the cost is bounded.

A listing failure fails the maintenance run visibly. Forge state is durable in its own operational tables and the next run resumes. Not a finding.

### 2.8 Server boot

`boot/mod.rs` builds one `BifrostStorage` (with the Oracle metadata cache) and hands out Forge `ForgeObjects::new(&bifrost_storage)` and Scribe `bifrost_storage.objects()`. In `config.rs`, `file_scan_bytes_per_task: Option<usize>` rejects `Some(0)` at startup, so the config fails fast rather than producing zero-sized tasks. Not a finding.

## 3. Affected Capabilities

- Bifrost SQL query: Interactive and Analytical, over published, hot and live data.
- Distributed Analytical execution across Oracle peers.
- Live Scribe fragment serving.
- Admission and fairness under load (Oracle, Scribe, Forge and Iceberg share the node-wide `LimitStore`).
- Rolling upgrades of multi-replica deployments.
- Scribe staging and recovery, and Forge maintenance (mechanical storage change only).

## 4. Recovery and Proof Assessment

- **Crash, cancellation and peer loss:** recovery is correct and per query. No durable state is introduced on the query path. Exchanges and metric folds are released on settle, and the journeys (53/53 recorded) prove the early-termination path.
- **Durability:** unchanged. The WAL, the Scribe dedup fence and Iceberg commits are untouched beyond the type swap.
- **Upgrade:** not proven. Nothing in the peer contract binds the DataFusion or datafusion-distributed fork revision (SYS-1). The plan's claim at line 164, "(the peer fingerprint carries the fork revision)", is not true of the candidate, and no test exercises a mixed-revision refusal.
- **Load and refusal:** not proven. No test asserts that a refused or queued query performs zero object-store reads (SYS-2).

## 5. Findings

### SYS-1 — MISSING: the peer contract carries no fork revision

- **Obligation:** spec revision 26 ("Oracle registration and distributed wire"): "the peer contract fingerprint includes the DataFusion fork revision, and a worker on a different revision refuses before decoding a plan". The plan restates it at line 164.
- **Location:**
  - `crates/vala/vala-bifrost-redux/src/oracle/peer.rs:312` `stage_body_digest`, which no longer hashes a variant or SQL version and has nothing to replace it;
  - `STAGE_PROTOCOL_VERSION = 2` (`peer.rs:689`) and `PEER_PROTOCOL_VERSION = 5` (`dispatcher.rs:46`), both unchanged;
  - the live fragment plan fingerprint;
  - `ORACLE_VARIANT_SQL_VERSION`, which was deleted.

  No digest, version or registration field carries the fork revision.
- **System consequence:** during a rolling restart that bumps only the DataFusion or datafusion-distributed pin, peers on different revisions accept and decode each other's stage and fragment plans. Protobuf drops unknown fields silently (for example `return_field_metadata`, field 7), so a worker can execute a plan with different types or semantics and return wrong results instead of refusing.

  The base-to-candidate transition itself refuses only by accident, because the digest domains changed. That refusal surfaces as a security or digest mismatch rather than a clean version refusal, and it protects nothing on the next fork bump.
- **Testable correction:**
  1. Bind a build-time constant for the fork revisions (for example `DATAFUSION_FORK_REV`, derived from the Cargo lock or source or set in `build.rs`) into the peer registration fingerprint and into the stage-body and fragment digests.
  2. Refuse a mismatched revision with a typed version error before `PhysicalPlanNode` decode.
  3. Add a unit test: a stage body produced under revision A and offered to a verifier configured for revision B is refused before decode, with the stable error code.

### SYS-2 — REGRESSION: object-store IO before admission, with no deadline

- **Location:**
  - `crates/vala/vala-bifrost-redux/src/oracle/exec.rs:1489` `persisted_input` awaits `cache_footers` (`exec.rs:1435`) during planning;
  - `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:2191` `run_sql_attempt` plans (`classify_one_build` at 2111, then `build_physical_root` at 2859) before `admit_built_attempt` (2064) and `audit_and_bind`;
  - `build_physical_root` has no deadline wrapper; the deadline is applied only in `discover_live_routes`.

  This was introduced in `4559cff37`, and the base has no planning-time fetch.
- **Regression boundary:** admission is the load-shedding boundary. Before this change, a refused or queued query did no warehouse IO. Now every planned attempt GETs a footer and page index for every published file Iceberg kept, `meta_fetch_concurrency` at a time, through the node-wide `LimitStore` that Scribe staging and Forge also use.
- **System consequence:**
  - Under overload, clients retry the retryable `QueryQueueFull`. Each retry repeats the footer reads on a cold or evicted cache before it is refused again, so the retries amplify object-store pressure exactly when the node is saturated.
  - That pressure starves Scribe flush and Forge permits on the same node, which can stall ingest durability progress.
  - With no deadline, a slow store holds planning open past the request deadline before admission is even consulted.
  - The errors are discarded (`let _ = ... try_collect`), so a degraded store is not visible at this layer.
  - The `scan()` rustdoc ("Row IO remains lazy ... first accessed only when DataFusion executes the already audited plan", `exec.rs` around 1547) is now false.
- **Testable correction:** move footer warming after admission (for example into `audit_and_bind` or execution start), or bound it by the request deadline and skip it when admission would refuse. Test with a counting `ObjectStore` wrapper: a query refused at admission (ceiling or queue full) performs zero GETs. Correct the `scan()` rustdoc.

## 6. Residual Risks (not findings)

- **Local Forge listing:** O(n²) page walk on large local trees. This applies to the dev/single-node shape only.
- **Orphan multipart parts:** cancellation at shutdown can leave them. The cost belongs to bucket lifecycle policy, and integrity is preserved by WAL replay and the dedup fence.
- **Single-task live stage under a multi-task consumer:** this fails visibly. It is a known fork gap recorded in the plan.
- **Tenant binding by path prefix versus footer proof:** deferred to the security/tenancy reviewer.

## 7. Verdict

**FAIL.** SYS-1 leaves a spec revision 26 obligation unmet, on a path that becomes wrong-results-instead-of-refusal at the next rolling upgrade. SYS-2 is a reachable regression in overload behavior: it moves object-store IO ahead of the admission boundary and outside the deadline. Both are bounded and testable corrections inside the approved task scope.
