# Bifrost Design

**Version:** v1

Bifrost is Wyrd's high-throughput distributed OLAP warehouse. It is both the
public analytical surface and the storage substrate Vala uses for Wyrd
observations. This document is the authority for Bifrost internals: physical
table identity, Scribe ingestion, Oracle query execution, Forge maintenance,
resource ownership, durability, recovery, and public analytical behavior.

`architecture/wyrd-design.md` remains authority for Wyrd doctrine, Cards, and
cross-service contracts. Bifrost is infrastructure, never a Card kind. Decision
history lives in git; this document states one architecture.

## System boundary

`wyrd-server` is the only network-serving surface. It owns listeners, TLS,
authentication, authorization, request limits, boot readiness, and server
lifecycle for Bifrost HTTP, gRPC, and MCP operations. Vala owns the Bifrost
engine and its three cohesive subsystems:

- **Scribe** owns pod-local ingestion, WAL durability, active and immutable
  rows, durable local staging, hot-object publication, and bounded live-tail
  service behavior.
- **Oracle** owns immutable query cuts, admission, interactive and distributed
  execution, live-tail fusion, terminal-safe streaming, and read-audit
  acceptance.
- **Forge** owns Scribe-hot promotion, Iceberg maintenance scheduling,
  resource and lease fencing, compaction publication, reconciliation,
  retention, and garbage collection.

Apache Iceberg-managed Parquet in object storage is the analytical system of
record. Postgres is the tenant-isolated catalog and durable control plane.
DataFusion is the vectorized execution engine. Managed compaction and Iceberg
components perform bounded computation behind Vala-owned contracts; they do not
authenticate callers, serve the network, own tenant authority, or commit
durable Wyrd state independently.

## Table and row identity

Every Bifrost table has one server-owned managed envelope with these required,
non-null columns:

- `wyrd_event_time`: validated caller event time or server receipt time;
- `wyrd_ingested_at`: server-stamped ingestion time;
- `wyrd_batch_id`: immutable UUIDv7 identity of one accepted logical batch;
- `wyrd_row_ordinal`: zero-based `Int32` position in the complete logical batch;
- `wyrd_request_id`: server-minted or validated request correlation;
- `data_tenant_id`: authenticated tenant-isolation identity.

Nullable `run_id` and `card_uid` provide optional Card/Run correlation.
Required, non-null `principal_id` identifies the authenticated publisher. None
participates in row identity.

For OTLP records, table-owned projection reads correlation only from the final
record-level `wyrd.card_ref` and `wyrd.run_id` attributes, retaining all source
attributes losslessly. The values use the existing `CardRef` and `RunId` text
grammars. Any client Card UID is ignored; Scribe stamps only the UID from the
verified principal scope.

Within a tenant-qualified physical table, row identity is:

```text
(wyrd_batch_id, wyrd_row_ordinal)
```

Globally it is:

```text
(data_tenant_id, logical_table, wyrd_batch_id, wyrd_row_ordinal)
```

The ordinal is contiguous across request order and never resets at an Arrow
batch, WAL segment, shard, staged run, Parquet row group, object, snapshot, or
Forge rewrite. Gate validates the batch identity and routes the authenticated
write. Scribe assigns request-wide ordinals while table-owned validation
prevents payload columns from supplying server-owned fields. A retry preserves
the batch ID. Within the idempotency-retention window,
reusing an accepted ID requires the same schema fingerprint, row count, row
order, and payload digest; any mismatch is a stable batch-identity conflict.

One authenticated tenant and logical `TableRef` bind exactly one physical
Iceberg table, namespace, and object-store prefix. Callers never choose another
tenant's physical identity. Every physical file retains `data_tenant_id`.
Postgres RLS, object prefixes, Scribe ownership, Oracle source binding, and the
plan-root `TenantTripwireExec` enforce the same tenant. A mismatched row fails
closed with `WYRD_VALA_500_TENANT_TRIPWIRE`.

Built-in and user-defined tables share this physical model. "Built-in" names
definition ownership, not a weaker tenant scope or a separate storage mode.

## Durability and visibility

Bifrost uses explicit authority transitions:

```text
accepted append
  -> WAL-fsynced and batch-fenced active rows
  -> immutable rows owned by a closed shard cohort
  -> fsynced, validated, query-registered staged runs
  -> committed published Scribe hot objects
  -> Iceberg promotion snapshot
  -> Forge rewrite snapshot
```

An append acknowledgement means the WAL append and batch fence are durable and
the exact rows are authoritative in Scribe. It does not wait for local Parquet,
object publication, Iceberg promotion, or compaction.

Oracle binds every query to one immutable cut. The cut combines its pinned
Iceberg snapshot, committed Scribe hot objects not represented by that
snapshot, and a versioned Scribe live-tail lease. When the same row identity is
visible through more than one source, the most advanced authority in the list
above wins. A source is suppressed only when the cut contains exact publication
evidence for the complete corresponding WAL range or object identity. No query
uses mutable catalog state after pinning.

## Ingest: Scribe

### Fixed lanes and hierarchical ownership

Each Scribe pod owns exactly sixteen shard lanes. The routing key is:

```text
hash(data_tenant_id, canonical_table, wyrd_batch_id)
```

Distinct batches for one table use all lanes; retries use the recorded lane.
Each lane owns a bounded mailbox, WAL generation, memtable buckets, and cohort
state. Shards are a pod-local concurrency topology, not tenant partitions or
table reservations.

Shard owners, reconciliation, staging, assembly, and persistence run on one
dedicated Scribe Tokio runtime separated from request serving. One non-cloneable
owner controls that runtime; all engine consumers hold handles. Server shutdown
drains the bounded Scribe lifecycle before releasing the runtime, so request
state cannot accidentally destroy an executor or abandon durable ownership.

`ScribeAdmission` owns global resource accounting. A tenant ledger and table
ledger account every lifecycle category independently: admitted items and
bytes, active and immutable bytes, durable staging, merge scratch, stage and
upload claims, and persistence work. A table activates only when one checked
complete lifecycle vector fits. Idle capacity is work-conserving. Under
contention, equal tenant and table soft shares stop an over-share incumbent
from reacquiring capacity while acknowledged ownership drains. Bounded,
expiring identity-only demand determines which waiting table receives released
capacity; it reserves no bytes or slots.

Admission completes before WAL or mailbox mutation. It either accepts
immediately or returns typed retryable pressure. Already acknowledged ownership
is never revoked. Every category change is a checked move between owners;
temporary overlap is reserved explicitly.

Each shard schedules complete commands by tenant round-robin, table
round-robin within the tenant, and FIFO within the table. System and dynamic
tables have equal scheduling weight and admission rules.

The principal size boundaries are independent:

| Boundary | Rule |
|---|---|
| WAL segment | 512 MiB encoded default per shard WAL file |
| Active shard generation | `min(512 MiB, floor(active_generation_budget / 16))`, plus age and pressure |
| Parquet row group | 32 MiB logical or 131,072 rows |
| Scribe hot object | approximately 512 MiB encoded, with bounded residue |
| Forge rewrite output | approximately 1 GiB according to the table property |

The generation maximum age and staging maximum dwell are each 600 seconds by
default. Configuration validates checked arithmetic and proves that one maximum
ingress envelope, one complete table lifecycle vector, one immutable rotation,
and the independent merge and upload workspaces fit before serving. It never
derives tenant or table cardinality from shard count.

### Append, rotation, and staging

The Scribe write path is:

```text
validate and split by canonical physical partition
  -> global, tenant, and table admission
  -> route to one of sixteen shard mailboxes
  -> tenant/table/FIFO scheduling
  -> WAL append, fsync, and durable batch fence
  -> tenant/table/partition memtable insertion
  -> acknowledgement
```

Caller-supplied `wyrd_event_time` is accepted only within the server window,
defaulting to 30 days before through 24 hours after receipt. An out-of-window
value fails with `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE`; Scribe never clamps or
normalizes it. When the column is absent, the server stamps receipt time.

Each shard projects WAL bytes, Arrow and metadata ownership, age, and resource
pressure before append. Rotation closes the shard generation when any validated
limit requires it. The active memtable remains separated by exact
`SealKey { tenant, table, physical_partition }`. Rotation freezes each nonempty
bucket independently; no file or sort may mix tenants, tables, layouts, or
physical partitions.

Frozen members follow this monotonic lifecycle:

```text
ImmutableArrow
  -> PreparingLocalRun
  -> Ready
  -> Claimed
  -> Publishing
  -> Published
  -> CleanupPending
  -> Retired
```

`ScribeHotStage` bounded-sorts a member into immutable local Parquet runs,
fsyncs the file, manifest, and directory, verifies checksum/schema/layout/footer,
performs a bounded read preflight, and registers it as live-tail authority. The
shared cohort WAL retires only after every member reaches that boundary.
Definite pre-registration failure returns to immutable Arrow; uncertain state
keeps WAL and local evidence for reconciliation.

### Assembly and publication

`StagingAssembler` claims complete ready members under the exact key:

```text
(tenant, table, schema_fingerprint, physical_layout,
 physical_partition, node_id, writer_epoch)
```

Claims may combine compatible members across generations and shards on the same
node. Membership is deterministic and durable before merge; one member cannot
be split across claims and a later member cannot join an existing claim.

`ParquetBatchEncoder` performs a bounded external merge in canonical
`PhysicalLayout` order with `(wyrd_batch_id, wyrd_row_ordinal)` as the stable
tie-breaker. It writes 32 MiB logical or 131,072-row Parquet row groups and
closes immutable hot objects around 512 MiB encoded. A completed row group is
indivisible, and a smaller object is valid for dwell, partition close,
pressure, drain, or final residue. The 512 MiB target is independent of WAL,
active-memory, row-group, and Forge output geometry.

`ScribePersistence` derives deterministic object identities, uploads through a
bounded lane, and atomically commits `file_list` plus audit evidence under the
publication fence. Staged runs remain authoritative until that transaction is
committed or reconciled as identical. New cuts then use the published object;
existing staged-reader leases finish before local deletion.

### Live tail, recovery, and shutdown

Oracle never opens another node's local path and does not use WAL as its normal
query source. `FetchLiveTailService` serves a leased versioned cut of active,
immutable, or staged rows through bounded internal RPC. Projection, signed
predicate, physical partition, retained bytes, batch count, deadline, and
cancellation are enforced before returning Arrow batches.

Startup replays WAL using the recorded shard ID, validates staged files and
manifests, rebuilds source and claim indexes, and reconciles publishing
operation IDs against `file_list` before opening admission. Unknown versions,
checksum mismatch, contradictory lineage, or ambiguous authority fail closed.

Shutdown closes admission and mailboxes, rotates nonempty generations, stages
immutable ownership, publishes valid residue, and drains admitted work within
the server deadline. Unsettled work retains exact replay evidence. Shutdown
never deletes WAL or staged files merely to meet a deadline.

## Query: Oracle

### Immutable planning and source authority

Oracle authenticates and authorizes the request, validates read-only SQL,
acquires the read-audit durability boundary, pins one source cut, builds an
optimized logical plan, selects an execution path, admits resources, and
streams one terminal-safe result. DataFusion providers receive only the pinned
files and leased live-tail sources. Tenant authority is installed before plan
decode or source IO.

Oracle plans every query once through the pinned `datafusion-distributed`
planner and derives its admission and terminal path from the returned physical
root:

- **Interactive** is selected for a normal DataFusion physical root and keeps
  protected capacity for low-overhead, high-throughput reads.
- **Analytical** is selected for a
  `datafusion_distributed::DistributedExec` root and executes its streamed
  stage graph across authenticated Oracle peers.

Oracle maintains no operator allowlist, candidate heuristic, second physical
build, or pre-selection fallback. The pinned planner decides whether useful
network boundaries survive; its registered codecs and workers decide what the
integrated system can execute. Planning, codec, and worker incompatibilities
return a stable structured failure. Representative end-to-end queries prove
scan/filter/projection, grouped aggregation, join, sort/limit, exchange, and
spill behavior without claiming exhaustive operator coverage.

### Distributed analytical execution

The analytical path uses streamed exchanges parameterized by DataFusion
`Partitioning`. It adds no materialized shuffle service, independent scheduler,
or query-job subsystem. Every stage assignment is versioned, signed, replay
protected, and binds:

- tenant and permission digest;
- public query, DataFusion query, stage, task, and attempt identities;
- pinned snapshot and fragment digests;
- leader and worker fences and audience;
- reservation identity, request digest, nonce, and absolute deadline.

Peer TLS and workload authentication complete before the bounded first frame is
accepted. Claims and body digests are verified before lazy plan decode, task
cache lookup, provider creation, or source IO. Every worker replaces its
process runtime with the exact query-admitted `RuntimeEnv`, `MemoryPool`, spill
share, cancellation token, and deadline. Query-owned leases remain alive until
coordinator end-of-stream, cancellation, or cache invalidation and all
structured tasks have joined.

Exchange buffers draw from the same finite query-owned memory pool as the
operators; they are not precharged into a predicted child allocation. Before
dispatch, Oracle enforces its configured selected-worker limit, admitted
tasks/partitions, Wyrd-owned admission queue and slots, and scratch allocation
with checked count/range arithmetic. Dependency-owned exchange queues retain
their pinned byte backpressure without a Wyrd item-count guarantee. Oracle does
not claim to predict every dependency allocation or transient encoded-message
byte. Follower spill is charged to the same query-owned scratch allocation.
Memory-pool, transport, or scratch exhaustion is typed and releases all memory,
scratch, slot, task, cache, and transport owners exactly once after the graph
drains. Cleanup timeout or failure is never reported as a successful release:
the remaining graph stays observable to the owning supervisor, the node does
not claim a clean terminal state, and readiness or shutdown evidence surfaces
the failure.

One immutable cut, deadline, cancellation tree, and execution attempt bound
the complete stage graph. Head cancellation stops and joins every descendant.
Analytical selection binds the logical query to exactly one distributed
attempt: after selection, peer transport close, reset, availability timeout,
authentication, authorization, tenant, digest, protocol, resource, corruption,
cancellation, deadline, and execution failures are all terminal. Oracle does
not construct a successor attempt, does not rebuild the stage dependency
closure, and does not fall back to Interactive. A caller that receives the
failure terminal may submit a new logical query. Retry remains a property of
unrelated Scribe, catalog, and Forge protocols, not of a selected Analytical
query.

Result frames carry query and attempt identity and are followed by one explicit
success or failure terminal. Bounded transport buffers provide backpressure but
never spool the complete result. A caller may process frames incrementally, but
the result is successful only after the success terminal; frames preceding a
failure terminal are invalid as a complete query result. Frames that do not
match the owning attempt identity are rejected before egress, so a successful
stream cannot contain partial or duplicated rows.

### Admission and memory

Interactive and Analytical paths have separate queues and slot counters.
One atomic aggregate check prevents their combined occupancy from exceeding
Oracle capacity. A configured Interactive slot floor cannot be borrowed by
Analytical work; Interactive work may use idle unreserved capacity. Both paths
share one elastic memory and scratch root plus one leader/peer capacity counter.
`QueryClass` is derived from the one returned physical root and is never a
caller-controlled hint.

Local capacity is pod-local and is derived from this node's own CPU and memory,
never from a cluster-wide quota. Total slot units default to the smaller of two
units per effective CPU and the Oracle memory budget divided by the 32 MiB
working set a unit represents; an explicit operator limit replaces that
derivation outright. The resulting total splits once at boot into
`interactive_floor_units + analytical_max_units`. `analytical_max_units` of zero
is valid on a pod too small to run one two-unit Analytical query: its Analytical
admission is refused immediately rather than queued forever.

Within each path, queries are FIFO per tenant and tenants are selected by
weighted round robin. The arbiter first fills the protected Interactive floor,
then assigns unreserved capacity to the oldest eligible path head while
preserving tenant rotation. A continuously ready path cannot be skipped
indefinitely, and Analytical work never consumes the Interactive floor.

Slots admit; the actual grant sizes only the execution memory ceiling. A slot
unit represents the 32 MiB working set used to derive local capacity; it is not
itself charged as resident query memory. Concurrency is governed by slot units,
actual cooperative reservation by the shared memory root, and spill by the
separately leased scratch share. Interactive work charges one unit and
Analytical work two, with the selected physical plan owning the checked final
cost. A query-local memory ceiling is derived once at admission:

```text
grant = oracle_budget * query_slots / sum(running_slots)
grant = clamp(grant, 32 MiB, 256 MiB)
```

The denominator includes the candidate query's slots plus every running
query's admitted slots. The grant is a non-reserved per-query ceiling held for
the query lifetime and never recomputed under running operators. Every leader
and follower query receives a private view over one process-wide Oracle memory
root, never an independently sized pool: the view refuses growth past that
query's own ceiling, and the root's single tracked spill-fair pool, bounded by
the Oracle floor plus the elastic borrow, arbitrates what all live queries hold
together. Aggregate governed reservations therefore cannot sum above what the
pod owns.

Only fallible cooperative reservation is hard-limited. Growth DataFusion does
not let fail is still real memory, so it is charged to an explicit process
headroom counter that makes later fallible growth refuse sooner. The resource
plan retains that headroom alongside the unmanaged reserve for dependency
allocations outside cooperative reservation; the pool is the Oracle safety
boundary, not a guarantee against operating-system or cgroup OOM.

The guaranteed minimum successful grant fixes one `OracleSessionShape` before
physical planning: the 32 MiB memory floor, the minimum two execution
partitions narrowed by available cut work, and the resulting target partitions,
batch size, spill reservation, and join preference. Oracle retains that exact
`SessionConfig` with the single physical root. The actual grant chosen after
root-derived admission supplies only the query-owned `RuntimeEnv` and
`MemoryPool` ceiling; it does not reshape or rebuild the physical plan, and
capacity above the retained shape may remain unused.

Operators and exchange consumers share that issued query ceiling without
separate sublimits. Admission refuses before dispatch when checked graph counts
or scratch demand exceed their finite configured limits. Allocation or
transport refusal after admission is a typed query-resource failure and
cancels the full query; it never borrows from another query's ceiling. Scratch
space is separately reserved because spill consumes real disk.

Tenant fairness is owned separately by per-tenant FIFO and weighted
round-robin admission, scheduled pod-locally: tenant slot caps are local
scheduling caps rather than cluster quotas. Grants are tenant-blind.

An Analytical leader selects at most `max_workers_per_query` remote workers from
the pinned eligible cut, rotating the starting position by the attempt identity
so selection is deterministic, stable across re-projection of the same roster,
and spread across attempts. Only selected workers reserve resources, receive
requests, or affect the result; an unselected replica is absent from the cut
entirely. Memory governance protects
stability; pruning, vectorization, layout, and IO efficiency determine latency.

### Read audit and terminal contract

Oracle read admission is the narrow exception to transactional Postgres audit.
Before rows may be returned, the server fsyncs a versioned CRC-framed local WAL
acceptance containing its timestamp. One bounded relay appends it at least once
to the canonical tenant hash-chain outbox. A replayed relay may create one
valid duplicate but cannot lose an accepted read decision. One logical query
produces one read-audit event; distributed stages produce none.

Query streams are length-delimited, terminal-safe frames. Slot/queue refusal
before framing is `QueryAdmissionRejected`; governed memory, scratch, or
exchange exhaustion after framing is `QueryResourcesExhausted`. Cancellation,
deadline, peer loss, and execution failure have typed terminal outcomes. A
stream never represents partial rows as success.

A public distributed-plan or execution-path `EXPLAIN` surface is deferred and
is not part of this delivery. Selected-path evidence reaches callers only
through the success terminal of the one public query operation. The public
query deadline range is `1..=u32::MAX` milliseconds across Rust, HTTP, gRPC,
Python, TypeScript, and MCP.

## Maintenance: Forge

### Scheduling, admission, and fences

Forge schedules durable Postgres tasks with tenant-fair admission. At most one
durable task attempt owns the lease and fence for a tenant-qualified table.
Within that attempt, admitted ordinary compaction plans are independent child
operations: fitting siblings may rewrite and publish concurrently, while
bounded per-tenant and per-worker admission also permits independent tables to
progress concurrently. Each plan binds the owning tenant, table, task, plan
hash, base snapshot, target branch, attempt UUIDv7, operation ID, output
generation, lease, and fence. Loss of authority cancels and drains physical
work before settlement.

Forge owns runtime leases, pod-local plan admission, read streams, writer
fanout, upload buffers, close futures, SQL state, audit, reconciliation, and
garbage collection. Resource admission may defer a plan but never changes
table file geometry or creates a second grouping algorithm.

### Scribe hot promotion

Forge first promotes committed Scribe hot objects unchanged. The promotion
worker validates the exact footer-derived Iceberg `DataFile` projection against
the claimed `file_list` row, bound table, partition, path, checksum, and target
branch. It then performs one fenced duplicate-checking fast-append
`commit_once`. Promotion writes no data object and invokes no compaction core.

On a definite compare-and-swap conflict, Forge refreshes the branch head and
revalidates the exact `file_list` rows, object/footer evidence, absence of an
equivalent promoted entry, branch, lease, and fence. When all assumptions hold,
the same attempt and operation ID may make at most one additional `commit_once`
within the original deadline. Another conflict, changed assumption, or expired
deadline settles the attempt as definitely uncommitted and returns the rows to
durable promotion demand under a new attempt. No retry rewrites or reuploads
the Scribe object.

An ambiguous catalog result keeps the same attempt and operation ID. Forge
refreshes metadata and reconciles exact snapshot properties and manifest
entries without retrying the catalog call. A second attempt cannot begin while
the first is unresolved. After exact committed evidence exists, one fenced SQL
and audit transaction records the promotion snapshot on every claimed row and
switches Oracle authority. During the catalog-to-SQL interval, Oracle
suppresses a hot object only when its pinned snapshot contains the exact path,
checksum, and operation evidence.

### Managed compaction and publication

Iceberg maintenance rewrites eligible live files toward the table's
`write.target-file-size-bytes`, normally approximately 1 GiB. Row-group size is
independent and normally 128 MiB. A physical file rolls from the managed
writer's encoded estimate after a completed write; partition and end-of-stream
residue are valid. Neither target is a universal physical object-size
guarantee.

The managed compaction core is the sole owner of candidate selection, grouping,
bin packing, delete application, sorting, partition fanout, bounded concurrent
writing, rolling, and output `DataFile` production. After it produces real
`CompactionPlan` values, Forge estimates each plan's peak heap use from the
plan, table schema and format version, batch size, prefetch and sort settings,
delete files, and recommended parallelism.

Each Forge worker owns one strict FIFO queue of those plans. The queue starts
only its head when both the pod's aggregate estimated-memory budget and running
parallelism have room. Waiting plans do not consume the running-memory budget,
and a later smaller plan cannot bypass a blocked head; pending parallelism
bounds the queue itself. The memory budget is configured per worker or defaults
to 80 percent of that worker's declared memory. A plan larger than either total
budget is refused, while a plan that fits the totals waits for running plans to
release capacity. The budget is an admission estimate, not a `DataFusion`
allocation ceiling. `DataFusion` runs Forge plans with its default unbounded
memory pool and without disk spilling; Forge provisions no local scratch
storage. An underestimated plan can exhaust the pod, after which the durable
task, lease, and fence recovery path reclaims the lost work. One plan executes
within one worker; Forge does not split a compaction plan across pods.

The managed core consumes the attempt cancellation tree, output identity, and
closed physical observer. It never owns tenant authority, leases, SQL, audit,
reconciliation, object GC, or catalog commit.

The non-committing boundary returns exactly:

```text
RewriteHandoff {
  base_snapshot_id,
  rewritten_data_files,
  applied_position_delete_files,
  applied_equality_delete_files,
  output_data_files,
}
```

Forge validates every identity against the exact base snapshot. Rewritten data
identities and output identities are globally unique; duplicates fail rather
than being hidden. Applied delete vectors are evidence-only sets and may
deduplicate by their complete canonical identity. Reading a delete file does
not make it removable because it may still apply to unselected live data.
Forge does not reselect, regroup, reconstruct, edit, or rewrite handoff files.

The commit adapter derives delete-file disposition from the immutable base
snapshot and the applied-delete evidence; the handoff remains exactly five
fields. A position-delete file is removed only when every referenced live data
file is in `rewritten_data_files` and the managed core applied its positions.
An equality-delete file is removed only when its partition/spec scope and data
sequence number cannot apply to any surviving unselected data file. Otherwise
the delete remains live. Output data and file sequence numbers are assigned by
the Iceberg commit so an applied delete does not reapply to replacement rows.
Missing target evidence, mixed sequence semantics, or an unproven surviving
scope fails commit validation.

Attempt output paths are table-bound and retain the managed core's recipe
segment:

```text
{table}/data/forge/{recipe}/{attempt_uuidv7}-{writer_ordinal:05}-{writer_uuidv7}.parquet
```

The recipe segment is the managed core's canonical writer-recipe identity and
keeps completed Forge outputs recognizable as current on the next selection
pass. Each physical writer owns its filename counter and UUIDv7 suffix;
`writer_ordinal` is minimum-width five-digit canonical decimal and may restart
for another writer because `writer_uuidv7` provides cross-writer uniqueness.
Forge separately assigns each opened output one attempt-global
`OutputIdentity.logical_ordinal` for observer, drain, reconciliation, and
cleanup evidence. The logical ordinal is not encoded into or reconstructed
from the object path. Cancellation drains every writer and retains exact
produced-or-possible output evidence. Forge renews and verifies the lease and
fence immediately before the initial `commit_once`. The commit adapter removes
the exact rewritten data files, adds the exact outputs, preserves delete
correctness, and writes operation and lineage properties in the same snapshot.

Committed, definitely uncommitted, fence-refused, cancelled, and uncertain are
distinct per-plan outcomes. Concurrent siblings may optimistically race on the
same branch head and cause an expected compare-and-swap conflict. On a definite
conflict, Forge refreshes the branch head and revalidates the retained planning
snapshot, current schema identity, selected-input existence, lease, and fence.
When those assumptions remain true, the same plan operation, output generation,
and objects may make at most three further `commit_once` calls after fixed
1s/2s/4s delays within the original deadline. Exhausted retries, an expired
deadline, or a changed assumption settles only that plan as definitely
uncommitted. Successful sibling snapshots remain visible; the task succeeds
when any admitted plan publishes, and later discovery replans remaining debt
from the current head. No conflict retry creates new output objects.

An uncertain attempt protects its outputs and reconciles under the same
identity; it never retries the catalog call, starts a fresh attempt, or reports
terminal success until exact catalog evidence resolves it. Terminal SQL and
audit settlement occurs exactly once.

### Convergence, retention, and cleanup

Each rewrite snapshot records canonical versioned fingerprints for its base
live data set, resulting live data set, produced outputs, semantic debt shape,
and lineage snapshot. Before object IO, Forge reconstructs those exact sets and
refuses a rewrite when selected files are prior outputs, semantic debt is
unchanged, and no live-set delta affects the selected groups or delete scope.
Path churn alone never authorizes another rewrite. Missing, expired, partial,
or contradictory lineage evidence fails closed.

Data-file compaction, manifest rewriting, snapshot expiration, expired-object
cleanup, and never-published orphan cleanup are separate protocols. Snapshot
expiration preserves active refs, unresolved attempts, reconciliation evidence,
and the lineage snapshot referenced by the branch head. Orphan GC deletes only
objects proven unreferenced and outside every active or uncertain attempt.
Committed Scribe hot objects in `file_list` that lack exact promotion evidence,
and objects retained by a pinned Oracle cut, are hard GC roots even when no
Iceberg snapshot references them.
A v1 live-tail lease retains Scribe-local Arrow batches and staged resources for its lifetime but names no Forge-collectable object, so it contributes no independent Forge GC root.
No cleanup infers safety from age or path shape alone.

## Resource and failure invariants

- Every Wyrd-owned queue, mailbox, stream, fanout, task set, buffer, staged
  namespace, and object upload lane is bounded. Scribe scratch and Oracle
  memory pools, scratch roots, and spill paths remain hard-bounded. Forge uses
  bounded FIFO admission from estimated plan memory instead of a hard
  `DataFusion` pool or spill path. A pinned dependency-internal queue may
  instead provide finite byte backpressure when Wyrd cannot configure its item
  count; architecture must name that exception rather than claim ownership it
  does not have.
- Global resource owners account tenant and table attribution without creating
  an independent root pool per tenant.
- Cancellation is structured: stop admission, cancel descendants, join work,
  reconcile uncertain effects, and release ownership exactly once.
- Durable operations separate definite failure from uncertain completion.
  Uncertainty retains identity and evidence until reconciliation.
- Catalog compare-and-swap and lease fences own publication authority. An
  object-store PUT alone never makes data visible or safe to delete.
- All Postgres mutations and durable transitions append audit in their commit
  transaction, except Oracle's WAL-before-read acceptance.
- Operator-level Forge audit uses the tenant-bound fenced capability defined by
  repository SQL rules; it is not a generic cross-tenant executor.
- No retry or successor attempt in any Bifrost protocol widens tenant, table,
  snapshot, participant, deadline, permission, or resource authority.

## Telemetry

Scribe, Oracle, and Forge each own one closed telemetry registry used by
production behavior, verification, and operator documentation. Four surfaces
carry Bifrost's observability, and each fact belongs to exactly one of them.

The public Prometheus catalog answers what an operator must be able to graph
and alert on without reading code: demand, queue depth and age, active
ownership, durable results, latency, failure class, physical data flow, and
outstanding maintenance debt. It is deliberately small and closed; a subsystem
does not add a family because a value exists.

Protocol mechanics — lease and fence decisions, catalog calls, reconciliation,
cursor movement, scheduler passes, and resource envelopes — belong to
structured traces, where the identities that make them useful are legal.
Durable audit and task rows remain the authority for what actually happened,
and no metric is evidence of a durable fact. Unresolved authority or
in-progress recovery is a readiness signal, not a metric.

Every active gauge decrements on success, refusal, retry, uncertainty,
cancellation, and failure. Metric labels use only closed, bounded dimensions
such as stage, decision, outcome, close reason, query class, execution path,
route reason, fallback reason, and stage role. Tenant, table, SQL, object path,
query ID, task ID, snapshot digest, and other high-cardinality values are
scrubbed trace fields, never metric labels. Physical size, latency, throughput,
and SLA claims require measured evidence from the production path.

## Public surface

Bifrost is projected consistently through Rust, HTTP, gRPC, Python, TypeScript,
MCP, generated schemas, machine-readable documentation, and stable error codes.
`wyrd-spec::vala::api` owns public wire contracts; `vala-sdk` owns client
behavior; `wyrd-server` serves every route.

The surface includes:

- table management under `/v1/bifrost/tables`;
- `AppendReceipt { batch_id, accepted_rows, durability }`, where `durability`
  is the closed `AppendDurability::Acknowledged` value; the synchronous append
  response never reports staged, published, promoted, or rewritten state;
- terminal-safe query streaming at `POST /v1/query`;
- canonical SQL observation reads through the ordinary Bifrost query contract;
- gRPC ingestion through `wyrd.v1.BifrostIngestService`;
- gRPC query projection through `wyrd.v1.BifrostQueryService`;
- the `wyrd.bifrost` Python projection and matching TypeScript SDK;
- agent-facing read and write operations governed by explicit permissions.

Observation namespaces such as `vala.traces`, `vala.metrics`, `vala.logs`,
`vala.eval`, `vala.drift`, `vala.dev`, and `vala.system` remain
tenant-qualified Bifrost tables. Canonical SQL is their only read contract;
the namespace does not create another storage or authorization model.

Permissions are scoped through `BifrostTable`, `BifrostRecord`, and
`BifrostQuery`. Generic writes cannot target reserved or system-managed tables.
Sensitive trace, log, GenAI, and agent-trace payload columns require their
respective `BifrostTracePayload`, `BifrostLogPayload`,
`BifrostGenAiPayload`, or `BifrostAgentTracePayload` read permission through
the canonical SQL path.

Bifrost read permissions carry an object axis. A `bifrost_query:read` or
sensitive-payload grant is scoped either to every object (`all`) or to a named
Bifrost object: a `{ catalog, schema }` schema scope, or a
`{ catalog, schema, table_uid }` table scope keyed by the existing Bifrost
`TableUid`. A query names no tables until it is planned, so `POST /v1/query`
admits on the coarse operation capability only, and the authoritative object
decision runs inside the Oracle once `pin_cut` has resolved the complete
`PinnedSealedTable` set — before provider registration, physical planning,
admission charging, read-audit acceptance, peer dispatch, or any source read.
Every resolved table, including tables reached only through a join or an
expansion, must be covered; one uncovered table denies the whole query with
`WYRD_VALA_403_QUERY_FORBIDDEN` and streams no rows. Sensitive-payload
permissions are checked with the same resolved-table scope, and the payload
categories and payload-forbidden error are unchanged.

The permission digest bound into every stage assignment is derived from the
approved scoped permission together with the exact authorized table identities,
so a worker cannot execute against a wider object set than the leader approved.
This is object-scoped RBAC — a static role grant over named objects — and adds
no grant table, policy lookup, query-path database lookup, cache, or second
checker.

There is no `WarehouseCard`, `wyrd.warehouse` compatibility surface,
asynchronous query-job API, result polling/redirect protocol, or client-selected
physical tenant scope.

## Explicit non-goals

Bifrost does not provide:

- DML through Oracle, CTAS, distributed writes, or arbitrary code execution;
- a materialized shuffle service, independent distributed scheduler, or
  detached stage runtime;
- partial successful results, automatic retry of a selected analytical query,
  or silent cross-engine fallback after analytical selection;
- a public distributed-plan or execution-path `EXPLAIN` surface in this
  delivery;
- result caching as a correctness dependency;
- cross-region query execution or autoscaling semantics;
- cross-pod Scribe assembly or coordination of one append across ingest pods;
- a second compaction planner, a managed-core catalog commit, or global Forge
  serialization;
- external-system writes through `Source` or any Bifrost path;
- compatibility routes, legacy storage names, or alternate durable formats.

## Platform audit sentinel

`DataTenantId::SYSTEM_OWNER` is the durable platform tenant for security events
that cannot safely be attributed to caller-controlled tenant data, including
peer tickets rejected before verified claim decoding. Its canonical row is UUID
`00000000-0000-0000-0000-000000000000`, slug `wyrd-system`, display name
`Wyrd System`, status `active`, and `deleted_at IS NULL`. Provisioning and boot
verification fail closed on conflicting identity or attributes. Unverified
payload bytes can never select an audit tenant.
