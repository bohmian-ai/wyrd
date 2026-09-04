---
id: BIFROST-R6-T04-REMEDIATION-SINGLE-PLANNER-QUERY-RELIABILITY
title: Replace two-gate routing and close query terminal reliability gaps
kind: remediation
mode: REMEDIATE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 6
depends_on: [BIFROST-R5-T04A-PRODUCTION-ACTIVATION]
requirements: [REQ-001, REQ-002, REQ-003, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
acceptance: [AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008]
parent_task: BIFROST-R5-T04A-PRODUCTION-ACTIVATION
reviewed_candidate: 47310e6f61ccbb2a1bf22886ca8beed53ccb0c59
remediates: [FIND-04A-1, FIND-04A-2, FIND-04A-3, FIND-04A-4, FIND-04A-5]
---

# Single-plan query routing and reliability remediation

## Outcome and value

Oracle builds one physical root from one immutable source cut through the pinned
`datafusion-distributed` planner. A normal root is Interactive; a
`DistributedExec` root is Analytical. Oracle admits that root's class and
executes the same root with the admitted query runtime. Stale source evidence,
planning failure, peer failure, and execution failure are terminal; Oracle
never repins, rebuilds, or falls back.

Interactive and Analytical keep separate admission accounting so stage graphs
cannot consume the protected Interactive floor. They no longer maintain
separate query engines.

The same task makes scheduled callers reject invalid terminals, makes SDK
settlement preserve body failures within the original deadline, replaces the
weakened negative journeys with their real conditions, and repairs the Task 04A
whitespace defect.

Required execution skill: `$wyrd-implement`.

## Owners, scope, and non-goals

Primary owners:

- `vala-bifrost-redux::oracle::Oracle`: one cut, physical build, root-derived
  class, admission, execution, and settlement.
- `oracle/planner.rs`: validation and immutable cut preparation.
- `oracle/exec.rs`, `analytical_scan.rs`, and `follower.rs`: planning-neutral
  leaves that acquire execution governance from the admitted `TaskContext`.
- `oracle/analytical.rs`: the pinned planner, destination routing, and selected
  `DistributedExec` lifecycle.
- `wyrd-server::oracle::forwarding`: class-neutral leader forwarding.
- `wyrd-server::query::scheduled`: scheduled terminal validation.
- `vala-sdk::query::QueryResultStream`: external Rust stream settlement.
- `wyrd-testing`: public and real-process routing, pressure, failure, and
  cleanup evidence.

Expected write set includes `architecture/bifrost-design.md` plus
`crates/vala/vala-bifrost-redux/src/oracle/{mod.rs,planner.rs,exec.rs,analytical.rs,analytical_scan.rs,analytical_transport.rs,follower.rs,tail_fence.rs,codec.rs,participant_cut.rs,dispatcher.rs,splitter.rs}`,
`crates/wyrd/wyrd-server/src/oracle/{forwarding.rs,peer_authority.rs}`,
`crates/wyrd/wyrd-server/src/query/{scheduled.rs,service.rs}`,
`crates/vala/vala-sdk/src/query.rs`,
the private assignment contract under `wyrd-spec`, and the existing Oracle
process journeys and support. `ForwardQueryClaims` remains a server-private
signed JSON value; this task changes no protobuf contract or descriptor.

After removing the final production and test callers of the custom Interactive
split path, delete `oracle/splitter.rs` and remove `mod splitter`. Do not
preserve unused validators, rewrite helpers, plan-substitution types, fixtures,
or tests from that architecture. If a concrete helper still has a live caller,
move only that helper to its actual owner. Preserve reservation, assignment
resolution, stage transport, and graph lifecycle used by `DistributedExec`.

Do not add a caller path hint, candidate heuristic, operator allowlist,
provisional admission class, second plan, automatic retry, custom network
boundary, dependency patch, or runtime capacity RPC.

## Ordered implementation scenarios

### Scenario 1 — One cut and one physical build decide the path

**Behavior.** The selected leader freezes one class-neutral membership/source
cut, builds one physical root, derives the class from that root, admits it, and
executes it once. Any planning or stale-source failure is terminal. Maps
REQ-001, REQ-002, REQ-003, REQ-005, REQ-006, REQ-009; INV-001, INV-002,
INV-006, INV-007, INV-008; AC-002, AC-003, AC-005, AC-008.

**RED.** Add the pure unit test
`oracle::exec::tests::physical_root_alone_selects_query_class`. Build an
ordinary root and a root returned by the pinned planner. Assert only the exact
`DistributedExec` root is Analytical. Keep cut, attempt, build, terminal, and
ownership assertions out of this unit test.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::exec::tests::physical_root_alone_selects_query_class)'
```

Refactor the existing Oracle journey to
`analytical_activation::single_planner_root_selects_path_and_capacity`. Through
the public server route run one normal-root scan and one grouped join whose root
is `DistributedExec`; assert exact results, matching terminal paths, positive
remote-stage evidence for Analytical, and zero retained ownership. Reuse the
journey's local object-store fixture to publish a valid Iceberg snapshot, then
delete only one referenced Parquet data object while leaving its metadata and
manifests intact. Query it through the same ordinary `PublishedOnly` normal-root
scan—not the grouped `DistributedExec` query—so the real public entry reaches
the existing Interactive pre-output stale-replacement branch and observes a
second ordinal/build as the pre-change RED. After remediation assert one
structured failed terminal, the original single cut, exactly one physical build
and attempt, no second ordinal, and zero retained query/graph ownership. A one-shot
planning failure must make the same one-build/no-fallback assertions. Add no
fault injector or separate stale harness. Add the one missing narrow operation,
`ProcessCluster::remove_storage_object(relative_object_key)`, to the existing
process-cluster support; it rejects absolute/parent-traversing keys and calls
`std::fs::remove_file` under the already-owned `storage_root`.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_activation::single_planner_root_selects_path_and_capacity)' --run-ignored=all"
```

**GREEN.** Replace `run_sql_query_attempt_loop` with one `run_sql_query` flow:

```text
authenticate and validate
  -> select one ready leader that advertises both existing classes
  -> leader freezes one class-neutral membership roster and immutable source cut
  -> build one physical root through the pinned distributed planner
  -> derive QueryClass from the exact root
  -> finalize and sign the participant cut with that class and frozen roster
  -> admit the class
  -> fsync audit acceptance and activate/drain the pinned sources
  -> bind the execution grant and drained sources once
  -> create the query-owned execution TaskContext
  -> execute the retained root once
     -> OracleRouteTasks validates and routes each remote stage before submission
  -> emit one matching terminal
  -> require clean EOF before a consumer reports success
  -> join cleanup and release ownership
```

Make these changes:

1. Delete `StaleReplacementGate`, the two-ordinal loop, ordinal fields/metrics,
   and every repin/replan branch. A stale Iceberg, hot, or live-tail source
   error before or after output maps to the existing stable failure and settles
   the sole attempt. The caller may submit a new logical query.
2. Delete `OracleClassification`, `classify`, complexity and scan-time
   heuristics/constants/reasons, `select_analytical`,
   `plan_analytical_candidate`, and all candidate/fallback telemetry and test
   controls. Remove `query_class` from `PlannedSqlCut`.
3. In server forwarding, choose the leader before class exists. Remove
   `query_class`, `participant_cut`, and `participant_cut_fingerprint` from
   `ForwardingAttempt` and `ForwardQueryClaims`. Keep the final private claim at
   `protocol_version = 1`; nothing has shipped, so add no compatibility decoder
   or migration path. The signed JSON claims retain audience, leader fence,
   expiry/nonce replay protection, authenticated context (including request
   identity), the unchanged request body, and the absolute deadline. Only the
   selected leader pins and plans.
4. Split participant-cut construction into a class-neutral frozen roster/source
   cut and final signing that consumes that same value plus the root-derived
   class. No topology or source refresh is permitted between those calls.
5. Add one pure
   `query_class_for_root(&dyn ExecutionPlan) -> QueryClass`: exact root downcast
   to `DistributedExec` is Analytical; every other root is Interactive. Move
   admission, running-query registration, class telemetry, query runtime, and
   final cut signing after this call.
6. Execute the retained `Arc<dyn ExecutionPlan>` with the admitted execution
   `TaskContext`. A normal root is graphless. A `DistributedExec` root transfers
   the admitted envelope once into the existing graph supervisor and executes
   through the existing stage lifecycle. Never rebuild either root.
7. Delete `execute_distributed_session`, `plan_distributed_split`,
   `RemoteScanExec`, and the production fragment path used only by the retired
   Interactive engine. After their final production and test callers are gone,
   delete `oracle/splitter.rs` and `mod splitter` in full, including
   `RemoteScanRule`, `PostRemoteOptimizerRule`, supported-operator validation,
   exchange rewriting, plan substitution, result wrappers, fixtures, and tests.
   Move a helper only if the new architecture leaves it with a concrete live
   caller and owner; do not retain `splitter` as a utility module.

**REFACTOR.** Leave one root-class function and one execution entry. Delete
superseded classifier, allowlist, fallback, ordinal, and custom Interactive
tests rather than renaming them.

### Scenario 2 — Pre-admission planning retains one minimum-grant shape

**Behavior.** Providers and physical leaves created before admission retain
only frozen source identity, schema/projection/predicate closure, destination,
and the exact `SessionConfig` used to build the root. That config comes from the
minimum grant with which admission can succeed. After admission and the audited
source drain, the retained `OnceLock` receives every concrete local
batch, reservation, degraded-state fact, and follower assignment exactly once.
At execution leaves resolve those bindings plus the admitted runtime, memory
pool, cancellation, deadline, and root-derived class from the execution
`TaskContext`; admission does not reshape the retained plan. The process
planning runtime performs no row IO and retains no query memory. Maps REQ-002,
REQ-005, REQ-007; INV-002, INV-003, INV-006; AC-002, AC-004, AC-008.

**RED.** Refactor the nearest existing plan-execution test
`oracle::exec::tests::hot_parquet_decodes_at_the_admitted_batch_size_in_both_modes`
into `oracle::exec::tests::retained_plan_uses_admitted_task_context_only`; do
not add a parallel test. Plan its hot-file scan, one assignment-backed scan,
and one Fused local live-tail source with a sentinel planning `RuntimeEnv`, then
admit a distinct query runtime, bind the existing fixture batches through the
lock, and execute the retained roots. Assert the exact result includes the
Fused rows and that the first row IO and every decoded-byte reservation use the
query runtime's pool. Its cancellation/deadline/class values must be observed,
the planning pool's reserved bytes and row-IO counter must remain zero, and both
pools must return to zero after settlement. Exercise an unbound key, a duplicate
key, and a follower assignment whose source identity or destination differs
from the planned leaf; each must fail before the row-IO counter changes. Assert
the planning and execution `SessionConfig` carry the same target partitions,
batch size, hash-join preference, and sort-spill reservation from the retained
`OracleSessionShape`, even when admission grants more memory.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::exec::tests::retained_plan_uses_admitted_task_context_only)'
```

**GREEN.** After freezing the cut and computing its `work_units`, derive exactly
one planning shape with
`OracleSessionShape::for_grant(ORACLE_PARTITION_WORKING_MEMORY_BYTES,
ORACLE_MIN_TARGET_PARTITIONS, work_units)`. These are the existing lower bounds
of every successful Oracle memory grant and partition allocation. Retain the
shape and its exact `SessionConfig` with the physical root; do not recompute or
replace either value after class derivation or admission. Install one
`Arc<std::sync::OnceLock<OracleExecutionBindings>>` directly through
`SessionConfig::with_extension` before planning. The empty lock owns no runtime
or memory and is not provisional admission. Keep validation and source
resolution methods on `OracleExecutionBindings`; add no slot wrapper.

Give `AnalyticalExecutionHandle` one process-owned, planning-only
`Arc<RuntimeEnv>`. Its per-query planning `SessionState` uses that retained
config and registers immutable cut providers, the pinned planner, codec,
task-count/scale/routing handlers, and frozen worker resolver on the planning
runtime. Planning may resolve catalog and manifest metadata but cannot open a
row source, reserve a peer, spill, or retain query data.
Drop the planning `SessionContext` immediately after retaining the physical
root, config, and lock; no second planning-session owner may keep the later
bound sources or reservations alive past query settlement.

Remove `query_pool`, `query_class`, `distributed_iceberg_batches`,
`distributed_hot_batches`, and `live_batches` from `OracleTableProvider` and
every planning-time leaf/governance constructor. A planning leaf retains one
private closed `OracleSourceKey`: `LocalDrained { table }` for the single
per-table Fused batch set, or `Follower { scan_id, role, destination }` for a
remote source. It also retains the frozen schema fingerprint, required columns,
predicate closure, tenant tripwire context, and advertised partition shape. It
must not retain a future `FollowerScanAssignment`.

After admission, run the existing `audit_and_drain_cut` against the admitted
pool before creating a `TaskContext`. Build one `OracleExecutionBindings` value
containing the `OracleExecutionGrant` (root-derived class, query cancellation,
absolute deadline, and existing leader accounting/telemetry handles), the
existing `DrainedTails` batches/reservations/degraded flag, and the completed
follower-source assignments keyed by the exact planned `OracleSourceKey`.
Validate that every planned key has exactly one binding, there are no extra or
duplicate keys, and each follower role, destination, tenant, schema
fingerprint, projection/predicate closure, and source identity matches the
planned leaf and frozen cut. On validation failure, drop the unbound
`DrainedTails` so its existing reservations release, then settle the admitted
query with the existing execution error before row IO.

Set that `OnceLock<OracleExecutionBindings>` once. A failed `OnceLock::set`
drops the rejected bindings and terminates as an internal
execution error; it never replaces live bindings. Then create the sole execution
`SessionState`/`TaskContext` with the query-owned `RuntimeEnv` and the retained
planning `SessionConfig`. Admission supplies the actual runtime and memory pool
only; it must not change optimizer choices, target partitions, batch size, or
spill reservation. Capacity above the retained minimum-grant shape may remain
unused. Do not put a memory pool in the extension: leaves use
`TaskContext::memory_pool()` and validate it is the admitted runtime's pool.
Planning and execution therefore share the same config and extension identity;
leaves retrieve it with
`task.session_config().get_extension::<std::sync::OnceLock<OracleExecutionBindings>>()`;
planning observes only an unbound lock and performs no execution IO.

Generalize the existing `AnalyticalScanExec` owner rather than adding another
deferred-plan type. Its local-drained variant looks up `LocalDrained { table }`,
projects the bound shallow batches through the existing
`projected_memory_source`, after the execution context has been built from the
same admitted pool that already owns their `DrainedTails` reservations; do not
double-reserve the shallow batches. `OracleExecutionBindings` retains those
reservations for the memory source's lifetime. Its follower-assignment variant looks up
the exact follower key and passes only the bound assignment to the existing
`FollowerSourceResolver`. In both variants, `resolve` receives the executing
`Arc<TaskContext>`, rejects a missing or mismatched binding before constructing
a memory source or calling a resolver, memoizes the resulting physical plan in
the existing `tokio::sync::OnceCell`, and executes it with the original task.

Build the follower resolver's temporary `SessionState` from
`task.session_config().clone()`, `task.runtime_env()`, and the task's registered
scalar, higher-order, aggregate, and window functions; do not call
`SessionStateBuilder::new().with_default_features()` with a new runtime. The
existing physical codec serializes a follower assignment only by looking up the
planned key through the now-bound lock shared by the retained leaf and execution
config. The follower decoder constructs the existing follower-assignment
variant from those authenticated bytes; at execute it uses the worker
`TaskContext` and existing `GraphLease` governance for runtime, pool, deadline,
and cancellation. This is the existing plan transport, not a new assignment
side channel, and it does not rewrite or rebuild the root.

Change `HotParquetExec` and equivalent local leaves to create their existing
governance reservation lazily from `TaskContext::memory_pool()` plus
`OracleExecutionGrant` inside `execute`. Missing/mismatched grant, elapsed
deadline, or cancellation is a DataFusion execution error before IO. Worker
task contexts receive the same execution-governance projection from their
admitted `GraphLease`; they do not share the leader's process-local lock or
accept a process-global runtime.

The leader's `Arc<OnceLock<OracleExecutionBindings>>` travels with the retained
plan and its derived leader task contexts. Its `DrainedTails` reservations
remain live through execution and are dropped by joined query/graph settlement
before the admitted parent pool and `GraphLease` are released. Record its
degraded flag through the existing terminal path; do not copy reservations back
onto `AdmittedQueryGuard` or create a second source registry.

**REFACTOR.** Keep the existing lazy assignment-backed scan owner. Do not add a
second deferred-plan abstraction or duplicate execution-resource struct.

### Scenario 3 — Every remote source has one forced, destination-bound stage

**Behavior.** A remote-only source always makes the returned root
`DistributedExec`; a normal root is entirely leader-executable. Each isolated
remote stage routes only to the destination frozen for its leaf, and conflicting
destinations fail planning. A multi-task stage partitions each bound source
across its task variants so every row is read exactly once. Maps REQ-002,
REQ-003, REQ-005, REQ-009; INV-002, INV-006, INV-007; AC-002, AC-003, AC-005.

**RED.** Add
`oracle::analytical::tests::single_partition_remote_leaf_keeps_destination_bound_stage`.
Start with one single-partition remote placeholder and one frozen non-leader
destination. Assert the pinned planner returns `DistributedExec`, the isolated
producer stage survives, every routed task URL equals that destination, and
execution resolves the assignment only after the admitted task exists. For the
forced two-task stage, cover an Oracle assignment with distinguishable files
and a Scribe assignment: assert the combined output contains the exact expected
rows once, each Oracle file belongs to exactly one task variant, the Scribe
source resolves exactly once, and the other Scribe task is a native `EmptyExec`.
Put two different destinations in one stage and assert planning fails before
dispatch.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::single_partition_remote_leaf_keeps_destination_bound_stage)'
```

**GREEN.** Extend the existing remote placeholder with its exact
`OracleSourceKey` and destination from the frozen roster: peer URL, node ID,
role, and fence. This is private stage authority, not a public query input. The
planning placeholder carries no `FollowerScanAssignment`; Scenario 2 binds that
value after admission and audited drain. During physical scan construction wrap
each remote-only leaf in DataFusion's native
`CoalescePartitionsExec`. The pinned planner converts that marker into its
native `NetworkCoalesceExec`, isolating the leaf as a producer stage while
remaining free to inject its normal join/aggregate/shuffle boundaries elsewhere.

The pinned revision elides a boundary when producer and consumer each have one
task. Therefore the existing desired-task-count and leaf-scale handlers must
assign every forced remote producer stage exactly two tasks when its original
leaf has one partition; multi-partition leaves use the retained planning count
from `OracleSessionShape::target_partitions`, bounded by their available work.
Both tasks preserve distinct partition execution but may route to the same
frozen destination. This is an internal correctness minimum, not a public
capacity setting.

Retain `AnalyticalLeafSplit` as the installed leaf-scale handler. It replaces
the source placeholder with one variant per final stage task. Each placeholder
variant carries private `task_index` and `task_count` fields in addition to the
same planned `OracleSourceKey`; it still carries no assignment before Scenario
2 binds the lock. When the codec encodes an Oracle variant, it retrieves the
one bound assignment from the retained `OnceLock<OracleExecutionBindings>`, clones its authority and
closure fields unchanged, and narrows only `persisted.files` with the existing
`skip(task_index).step_by(task_count)` partitioning. For a Scribe key, task
index zero alone retains the placeholder and resolves the source; every other
variant is DataFusion's native schema-compatible `EmptyExec`. The split never
copies a complete Oracle assignment to multiple tasks and never resolves a
live-tail source more than once.

Install one `OracleRouteTasks` handler after the dependency defaults. It walks
the isolated stage plan, reads destination-bearing remote leaves, and:

- defers when the stage has no remote leaf;
- returns the one frozen destination repeated to `event.task_count` when every
  remote leaf agrees; and
- returns a planning/execution error before submission if the stage contains
  zero authorized destinations, a destination outside the frozen roster, or
  more than one distinct destination.

`OracleRouteTasks` is the sole destination validator and router. Its traversal
runs at the actual routing boundary while `DistributedExec` prepares worker
placement, before task submission or source IO. Do not invoke the same traversal
after physical planning or at any other call site; if named
`validate_remote_stage_destinations`, it remains a private helper called only by
`OracleRouteTasks`. The RED conflict case must fail at this pre-dispatch routing
boundary.

Do not use `AnalyticalWorkerResolver`'s undifferentiated roster to route these
stages. Keep it only as the bounded pool for ordinary planner-created stages.
Delete the custom Interactive splitter only after this focused test and the
representative process journey pass.

**REFACTOR.** Retain `AnalyticalLeafSplit` for task-local source partitioning.
One native marker, the dependency's native boundary, and one route handler
replace only the custom Interactive splitter. No custom `NetworkBoundary` type
or second leaf-split abstraction.

### Scenario 4 — Public journeys drive real distribution, pressure, and loss

**Behavior.** Tier-1 evidence covers representative query styles, real
Analytical saturation with Interactive floor service, real HTTP/gRPC caller
drop, and terminal peer failure. Maps REQ-003, REQ-006, REQ-007, REQ-011;
INV-003, INV-006; AC-003, AC-004, AC-007, AC-008.

**RED.** Refactor the existing journeys, without adding parallel replacements:

1. `peer_network::analytical::stage_graph_executes_representative_query_styles`
   covers partitioned scan/filter/projection, grouped aggregation, left
   equi-join, and sort/limit across real Oracle processes with exact results,
   exchange/spill evidence, and cleanup.
2. `analytical_activation::public_query_selects_both_paths_and_preserves_interactive_floor`
   starts with typed `analytical_slots = 2`, holds one two-unit Analytical
   graph, proves a second Analytical query waits/refuses, and proves a normal
   Interactive root completes from the protected floor.
3. `analytical_activation::transport_drop_retains_running_status_until_cleanup_joins`
   consumes schema plus one batch from the HTTP `QueryResultStream` and drops it
   without `settle`; separately consumes one gRPC message and drops both the
   stream and channel. A fresh client/channel checks RUNNING while
   `AnalyticalCleanupPause` holds release, then retirement and zero ownership
   after release. Neither original consumer drains to terminal.
4. `analytical_activation::selected_peer_failure_is_terminal` drives real
   follower loss after Analytical selection. Assert one build, the original cut
   fingerprint, one structured failed terminal, and zero ownership. Remove the
   old repin-success, unsupported-fallback, no-exchange-fallback, and
   planner-refusal-fallback cases; retain the existing real under-privileged
   refusal assertion. The missing-object stale case remains in Scenario 1's
   existing routing journey; do not duplicate it here.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=peer_network::analytical::stage_graph_executes_representative_query_styles)' --run-ignored=all"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_activation::public_query_selects_both_paths_and_preserves_interactive_floor)' --run-ignored=all"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_activation::transport_drop_retains_running_status_until_cleanup_joins)' --run-ignored=all"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_activation::selected_peer_failure_is_terminal)' --run-ignored=all"
```

**GREEN.** Reuse the existing typed startup configuration, cleanup pause,
public status surface, peer-kill control, and ownership inspection. Test support
cannot fabricate a terminal, retry, or bypass a scan.

**REFACTOR.** Keep one journey per behavior and one representative operator
matrix. Remove invalidated fixtures and assertions.

### Scenario 5 — Scheduled callers require a valid successful terminal

**Behavior.** `ScheduledQueryCaller` succeeds only after one successful
terminal whose emitted-row count and Arrow EOS match the consumed stream.
Failed, missing, duplicate, malformed, post-terminal, or overflowing streams
return an existing stable query error and no `ScheduledQueryOutcome`. Maps
REQ-007, REQ-008, REQ-010; INV-003, INV-004; AC-004, AC-006, AC-008.

**RED.** Add one table-driven scheduled-consumer test,
`query::scheduled::tests::scheduled_terminal_requires_clean_eof`. Feed the
production consumption helper a failed terminal, success with the wrong
emitted-row count, malformed EOS, checked-row overflow, a second terminal, and
schema/batch data after a valid terminal. Assert each existing typed mapping and
no outcome; the valid terminal followed by clean EOF is the only success.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::scheduled::tests::scheduled_terminal_requires_clean_eof)'
```

Extend, rather than duplicate,
`query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup`.
Run a root-selected Analytical scheduled query, kill its real peer after stage
activation, and assert the mapped failure, no outcome, one audit decision, and
zero graph/query ownership after joined cleanup.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey -E 'test(=query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup)' --run-ignored=all"
```

**GREEN.** Keep one `QueryIpcDecoder`, checked `u64` row count, and at most one
provisional successful terminal in `ScheduledQueryCaller::run`. On the first
terminal, in order:

1. map `QueryTerminalOutcome::Failed` through the existing
   `query::service::terminal_error_to_bifrost`;
2. call `terminal.validate_emitted_rows(rows)` and map mismatch to the existing
   stream-protocol error; and
3. call `decoder.accept_eos(&terminal.arrow_ipc_eos)` and map the Arrow error
   through the existing service mapping.

Before taking `stream.frames`, convert its existing absolute `deadline_ms` once
to a `tokio::time::Instant` using the nonnegative remaining duration; do not
start a new timeout after a terminal. Retain a validated success provisionally
and continue polling the same stream in one `tokio::select!` over the original
cancellation token, that fixed deadline, and `frames.next()`. A second terminal,
schema, batch, stream error, or any other frame after the terminal is
`QueryStreamProtocol`. Construct `ScheduledQueryOutcome` only when the next
observation is clean EOF. EOF before a terminal remains incomplete; deadline or
cancellation keeps the existing incomplete-stream mapping.
Make the existing mapping helpers `pub(super)` instead of copying them. Replace
`saturating_add` with `checked_add`; overflow is `QueryStreamProtocol`. Extend
this one consumer test for duplicate and post-terminal cases; add no separate
harness. Reuse Scenario 4's peer-loss control and Oracle lifecycle; add no
scheduled execution branch.

**REFACTOR.** Scheduled execution remains a thin consumer of
`AppState::query_sql` and buffers no result batches.

### Scenario 6 — SDK success requires clean EOF and failures settle once

**Behavior.** The first public decode, protocol, incomplete-body, or transport
error marks `QueryResultStream` broken before return. Settlement never repolls
that body, and cancel, healthy drain, and retirement polling share the original
server-provided absolute deadline. A validated successful terminal remains
provisional until the same body reaches clean EOF; duplicate or post-terminal
frames can never become a successful SDK result. Maps REQ-007, REQ-008,
REQ-010; INV-003, INV-004; AC-004, AC-006.

**RED.** Extend the existing exact test
`query::tests::query_result_stream_settles_every_incomplete_exit_once` with:

- direct `next_batch()` transport, decode, protocol, and incomplete-body errors
  followed by `settle()`, proving the body poll count does not increase;
- a successful terminal followed by a duplicate terminal, schema, or batch,
  proving each is rejected and marked broken, while the same terminal followed
  by clean EOF alone settles successfully;
- a cancellation endpoint that never responds, proving settlement returns by
  the stream deadline; and
- an already-expired deadline, proving cancel, drain, and status polling do not
  receive fresh budgets.

```bash
mise exec -- cargo nextest run --locked -p vala-sdk --lib -E 'test(=query::tests::query_result_stream_settles_every_incomplete_exit_once)'
```

**GREEN.** In `QueryResultStream::next_batch`, replace the `?` on
`raw.next_decoded_frame()` with one match. Before returning any `Protocol`,
`Arrow`, `Transport`, or incomplete-stream error, invoke the same private broken
marker used by `settle_with`; return the original error unchanged.

On a successful terminal, validate its emitted-row count and the Arrow EOS
already accepted by `RawQueryStream`, but keep the terminal local and leave
`StreamSettlement` unchanged. Poll `RawQueryStream::next_decoded_frame()` once
more inside `tokio::time::timeout(self.remaining(), ...)`, using only the
original absolute deadline. `Ok(None)` is clean EOF: retain the terminal, set
`StreamSettlement::Settled`, and return `Ok(None)`. Any returned frame is the
existing post-terminal protocol error; a raw stream error is returned unchanged;
and timeout maps to the existing incomplete-stream error. Mark each non-EOF
case broken before returning. A failed terminal may retain its existing
immediate `FailedTerminal` return and settled state because it cannot become a
successful result.

In `settle`, compute `remaining()` before cancellation. If zero, record the
existing unconfirmed-settlement telemetry and return. Otherwise wrap
`client.cancel(&request_id)` in `tokio::time::timeout(remaining, ...)`.
Recompute the remainder before healthy body drain and before each status poll;
pass only that remainder. Broken streams skip drain. Keep the existing
at-most-once settlement transition and preserve the originating caller error.

**REFACTOR.** `QueryResultStream` remains the sole SDK settlement owner. `Drop`
stays nonblocking and spawns no task. Add no second state enum or client retry.
Extend only the named existing test; add no shared consumer abstraction or
harness. This scenario consumes only the unchanged public stream/deadline
contract and must not depend on or duplicate Oracle implementation details.

### Scenario 7 — Final v1 forwarding claims and task artifact are exact

**Behavior.** The server-private signed JSON claim binds the final forwarding
authority at protocol version 1, and Task 04A has no whitespace defect. Maps
REQ-001, REQ-009; INV-007; AC-005, AC-008.

**RED/GREEN.** Minimally extend
`oracle::peer_authority::tests::oracle_peer_authority_rejects_tamper_replay_and_restart_fence`
with the final `ForwardQueryClaims`. Assert a valid claim round-trips with
`protocol_version == 1` and preserves the audience, worker fence, expiry/nonce,
authenticated context and request ID, complete request body, and absolute
deadline. Mutating each signed binding or replaying the ticket must fail through
the existing authority checks. Do not add a v2 type, compatibility decoder,
migration test, protobuf field, or descriptor regeneration. Remove the trailing
blank line at EOF from `tasks/04a-production-query-activation-successor.md`.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=oracle::peer_authority::tests::oracle_peer_authority_rejects_tamper_replay_and_restart_fence)'
git diff --check 5d3cc09f75d0d3583164baeb481179ed92358808
```

## Retired tests and invariants

Delete tests for the v1 operator allowlist, logical complexity/scan-time
classification, class-change rejection, stale replacement, second ordinals,
custom Interactive splitting, and Interactive fallback after unsupported,
no-exchange, or planner refusal. Preserve tenant tripwire, immutable cut,
assignment integrity, codec, reservation, graph ownership, admission fairness,
terminal, cancellation, spill, audit, readiness, and shutdown tests.

Invariants: one cut, one build, one root, one directly stored, once-bound
`OnceLock<OracleExecutionBindings>`,
one admitted `TaskContext`, and one terminal; a successful consumer finalizes
that terminal only after clean EOF; root type alone derives class;
normal roots are leader-executable;
remote leaves are destination-bound stage inputs; planning owns no row memory;
each bound remote source contributes rows exactly once across its stage tasks;
Analytical never borrows the Interactive floor; failed or malformed streams
never become successful scheduled or SDK outcomes; SDK settlement uses one
absolute deadline.

## Broader verification

Run each named command scenario-by-scenario, then:

```bash
mise run fmt
mise run lints
mise run test:vala
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:server
mise run test:bifrost:journey:sdk
mise run test:e2e
mise run codegen:check
mise run check:client-tier
mise run check:pyo3-scope
mise run check:bifrost-resource-governance
git diff --check
```

Run `mise run check:unwrap-audit` and record its result without widening this
task to unrelated baseline findings.

## Completion evidence and stop conditions

Evidence must show one cut/build/root, an unchanged minimum-grant planning and
execution config, admitted-runtime-only row IO, forced and correct destination
routing for a single-partition remote source,
representative real stage graphs, protected Interactive capacity, real caller
drop and follower loss, a real absent pinned Parquet object producing one
terminal failure with no second build/ordinal, Fused rows bound after admission,
zero retained ownership, and exact final v1 forwarding bindings. Scheduled evidence must
reject failed terminals, row mismatches, malformed EOS, duplicate/post-terminal
frames, and peer loss. SDK evidence must prove successful terminals require
clean EOF, duplicate/post-terminal frames fail, broken bodies are not repolled,
and settlement cannot outlive the original deadline.

Return `SPEC_REVISION_REQUIRED` if implementation requires a caller path hint,
operator allowlist, cost heuristic, second plan, automatic retry, fallback,
dependency patch/fork, changed tenant/audit semantics, or removal of the
Interactive floor. Also return `SPEC_REVISION_REQUIRED` if the actual
admission-time grant must continue sizing `OracleSessionShape`; that requires a
post-admission physical rebuild and cannot coexist with revision 6's one-build
contract. Return `PLAN_BLOCKED` if the pinned planner cannot preserve
the selected native remote-source marker/boundary or execute the retained root
with the admitted `TaskContext`; record the exact failing API and plan.

## Material authority

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/operations/reliability-and-recovery.md`
- `architecture/references/languages/{spec-driven-development,implementation-execution,testing-workflows}.md`
- `architecture/references/domain/{olap-serving,datafusion,analytical-operations-reliability}.md`
- `tasks/04a-production-query-activation-successor.md`

## Execution evidence

### Status: PARTIAL — Scenario 1 items 1 and 5 landed; items 2, 3, 4, 6, 7 and Scenarios 2–7 not started

Two commits on `oracle-distributed`, both compiling and green:

- `52b572502` — Scenario 1 item 5 and its RED/GREEN cycle.
- `4afc69731` — Scenario 1 item 1.

No spec or task revision is required. Nothing found so far justifies
`SPEC_REVISION_REQUIRED` or `PLAN_BLOCKED`; the pinned dependency exposes every
API the remaining scenarios need (verified against the pinned rev, see
"Dependency findings" below).

### Scenario 1 — RED

`oracle::exec::tests::physical_root_alone_selects_query_class` was added at the
end of `oracle::exec`'s `mod tests`. First run failed to compile on the missing
`query_class_for_root`, which is the specified RED.

A second, real RED followed: with only a `WorkerResolver` installed, the pinned
planner returned a **non**-distributed root for a two-partition `MemTable`
grouped aggregation, so the fixture's `DistributedExec` assertion failed. The
planner's built-in task estimator derives task count from Parquet bytes scanned
and elides every boundary for a two-row in-memory table. The fixture now also
installs a `DesiredTaskCountHandler` (`RootClassTaskCount`) answering `2` for
leaf nodes, exactly as production's `AnalyticalCutTaskCount` answers from the
frozen cut rather than from bytes. This is a fixture correction, not a weakened
assertion: the test still asserts the planner itself produced the
`DistributedExec` root. `#[async_trait::async_trait]` is required on the impl;
the upstream trait is `#[async_trait]` and a bare `async fn` fails with E0195.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::exec::tests::physical_root_alone_selects_query_class)'
# 1 passed
```

The Scenario 1 journey (`analytical_activation::single_planner_root_selects_path_and_capacity`)
and `ProcessCluster::remove_storage_object` were **not** written.

### Scenario 1 item 5 — GREEN (`52b572502`)

`exec::is_distributed_plan(&dyn ExecutionPlan) -> bool` is replaced by
`exec::query_class_for_root(&dyn ExecutionPlan) -> QueryClass`: exact root
downcast to `DistributedExec` is `Analytical`, every other root is
`Interactive`. Its one production caller, `Oracle::plan_analytical_candidate`
(`oracle/mod.rs`), was adapted in place. The **reordering** half of item 5 —
moving admission, running-query registration, class telemetry, query runtime,
and final cut signing after this call — is NOT done.

### Scenario 1 item 1 — GREEN (`4afc69731`)

Deleted: `StaleReplacementGate` (struct, impl, and its unit test
`stale_replacement_gate_settles_once_and_rejects_second_or_post_output_attempts`),
the `for retry_ordinal in 0_u8..=1` loop, `record_stale_replan` and its
`oracle_query_stale_replans_total` counter, and the `stale_replanned` plumbing
through `oracle/query_stream.rs`.

- `run_sql_query_attempt_loop` → `run_sql_query`, one `run_sql_attempt` call.
- `run_sql_attempt` returns `Result<OracleQueryStream>`, no longer `Option`.
- `settle_attempt_output` returns `Result<OracleQueryStream>`; a typed stale
  first batch cancels, joins distributed children, and returns
  `release_error(.., QueryExecutionFailed, "stale first batch")`.
- `retry_ordinal` removed from `SqlAttemptInput`, `CutAuditInput`,
  `AttemptSettlement`, and the `read_decision` signature.

Two deliberate boundary decisions, both inside the task's stated write set:

1. `AuditDetail::BifrostQueryReadDecision.retry_ordinal` is **kept** in
   `wyrd-spec` and written as the constant `0`. `crates/wyrd-spec/src/vala/audit_detail.rs`
   and `crates/wyrd-spec/schemas/bifrost_audit_event.json` are outside this
   task's declared write set, and removing the field is a tenant-visible audit
   schema change plus codegen churn. Confirm this is what the reviewer wants.
2. `QueryWarning::StaleCutReplanned` remains in `wyrd-spec` and
   `wyrd-tonic/src/query_conversion.rs` but is now never emitted. Same reason.

### Not started

- Scenario 1 items 2, 3, 4, 6, 7 (classifier deletion, class-neutral
  forwarding, participant-cut split, retained-root execution, split-path
  deletion).
- Scenarios 2, 3, 4, 5, 6, 7 in full.
- Every broader verification lane, `git diff --check`, and the whitespace repair
  of `tasks/04a-production-query-activation-successor.md`.

### The one thing that shapes the remaining work

**Scenario 1 and Scenario 2 are a single atomic commit.** They cannot be split.

Removing `PlannedSqlCut::query_class` (item 2) has no compiling intermediate.
Today's `run_sql_attempt` order is: pin cut → telemetry(class) →
`analytical_candidate` → `admit_and_lease_attempt` → `audit_and_drain_cut` →
`execute_sql_cut` (registers providers, builds physical, executes) → settle.
Admission runs **before** the physical build, so the classifier is the only
class source at that point. Deriving the class from the root requires building
the root first, and the root can only be built pre-admission once providers stop
capturing post-admission state at construction — which is exactly Scenario 2's
`Arc<OnceLock<OracleExecutionBindings>>` mechanism.

An attempt was made to land item 2 alone; it produced ~12 `no field query_class
on PlannedSqlCut` errors with no correct way to satisfy them (a provisional
admission class is explicitly forbidden). It was reverted. Do items 1–7 and
Scenario 2 as one change.

### Concrete findings for the next implementor

Coupling that Scenario 2 must break, exhaustively located:

- `OracleTableInputs` / `OracleTableProvider` (`oracle/exec.rs:1725` and
  `:1755`) carry `query_pool`, `query_class`, `live_batches`,
  `distributed_iceberg_batches`, `distributed_hot_batches`. Only three are read
  in `TableProvider::scan` (`oracle/exec.rs:2141`): the `live_batches` branch,
  the two `distributed_*` branches, and `HotParquetGovernance::Leader { memory,
  memory_pool, telemetry, query_class }`. Everything else in `scan()` is already
  binding-free. The provider surgery is genuinely contained.
- `HotParquetGovernance::Leader` (`oracle/exec.rs:2519`) must lose `memory_pool`
  and `query_class`; `reserve_range` takes them from the execution
  `TaskContext`. ~8 test construction sites in `exec.rs` follow.
- `AnalyticalScanExec::resolve` (`oracle/analytical_scan.rs`) currently builds
  `SessionStateBuilder::new().with_default_features().build()`. Scenario 2
  requires it to build from `task.session_config().clone()`,
  `task.runtime_env()`, and the task's registered functions. That is the file's
  only structural change besides generalizing the key.
- `select_analytical` (`oracle/mod.rs`) **already** plans the distributed root on
  one session (`handle.planning_session(...)`) and executes it on a different
  one (`leader.task_ctx()`). The "execute the retained root with the admitted
  `TaskContext`" contract is therefore an existing, working pattern to
  generalize — not a new capability.
- `OracleQueryAttemptCut::try_from_snapshot` (`oracle/participant_cut.rs`)
  consumes `query_class` only to filter `capabilities.supported_classes` and
  stores no class field. The item-4 roster/finalize split is therefore small.
- `ReadyOracleForwarder::forward` (`wyrd-server/src/oracle/forwarding.rs:161`)
  calls `classify_for_forwarding` then `planned.query_class()` then
  `eligible_oracle_candidates(&snapshot, query_class)`. `validate_claims`
  (same file) checks `participant_cut` and `participant_cut_fingerprint`; both
  checks go with item 3.
- Deletion surface for item 7, all call sites mapped: `execute_distributed_session`
  and `build_remote_scan` (`oracle/mod.rs:4205`, `:4246`), `plan_distributed_split`
  (`:5551`), `RemoteScanExec` + `RemoteScanConfig` (`oracle/exec.rs:144`),
  `RemoteScanBuildContext` / `DistributedScanAssignments` / `fan_participant_partitions`
  (`oracle/mod.rs:1613`, `:1639`, `:5310`), `mod splitter` (`oracle/mod.rs:87`)
  and `oracle/splitter.rs` (1169 lines). Six `splitter::validate_supported` call
  sites in `exec.rs` tests (`:6879`, `:6941`, `:7004`, `:7053`, `:7091`, `:7105`)
  die with it. **Keep** `prune_assignments_by_event_time` — Scenario 3 still
  narrows assignments. `register_cut_provider`'s `let distributed =
  self.fragment_dispatcher.is_some()` gate (`oracle/mod.rs:3922`, `:3967`) must
  become unconditional: after item 7 the planner alone decides distribution, so
  scan ids are always produced.
- Scenario 5 needs no new machinery: `terminal_error_to_bifrost`
  (`wyrd-server/src/query/service.rs:478`, make `pub(super)`),
  `QueryTerminalFrame::validate_emitted_rows`, and `QueryIpcDecoder::accept_eos`
  all already exist; `service.rs:400-460` is the pattern to mirror.

### Dependency findings (pinned rev `4cfa166`, no fork or patch needed)

Confirmed present and public: `DistributedExec::new`, `DistributedExt::set_distributed_{worker_resolver,channel_resolver,user_codec,desired_task_count_handler,scale_up_leaf_node_handler,route_tasks_handler}`,
`SessionStateBuilderExt::with_distributed_planner`, `RouteTasksHandler` with
`RouteTasksEvent { task_ctx, plan, task_count }` and
`RouteTasksEventResponse::new(urls)`, `DistributedLeafExec`, `NetworkCoalesceExec`,
`NetworkShuffleExec`, `NetworkBroadcastExec`, `rewrite_distributed_plan_with_metrics`.
The planner wraps a >1-partition plan in `CoalescePartitionsExec`, injects
boundaries, elides unnecessary ones, and returns the original non-distributed
plan when no boundary survives — which is what makes root-type classification
exact. Scenario 3's forced single-partition remote stage and `OracleRouteTasks`
handler are both reachable with these APIs.

### Baseline noise

Four `vala-bifrost-redux` lib tests fail at `47310e6f6` and are unrelated to
this task: `catalog::bifrost_catalog::production_pin_tests::pinned_snapshot_decodes_manifest_event_time_by_writer_field_id`,
`catalog::bifrost_catalog::production_pin_tests::pinned_provider_refuses_a_snapshot_the_table_does_not_publish`,
`scribe::persistence::tests::persist_once_emits_compression_telemetry`,
`scribe::persistence::tests::persistence_runtime_registers_idle_queue_gauges`.

### Tooling note

`git diff` in this repository is rewritten by the `rtk` hook into a diffstat.
Use `rtk proxy git diff` for a real patch. A work-in-progress diff was lost this
way; nothing committed was affected.
