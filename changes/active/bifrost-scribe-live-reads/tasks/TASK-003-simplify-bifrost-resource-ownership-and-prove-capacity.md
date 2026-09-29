---
id: TASK-003
title: Simplify Bifrost resource ownership and prove read/write capacity
kind: implementation
status: proposed
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 11
requirements: [REQ-001, REQ-003, REQ-005, REQ-008, REQ-009]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006]
acceptance: [AC-004, AC-009, AC-010, AC-011]
depends_on: [TASK-001]
replaces: TASK-002
---

## Outcome and Value

One 4-CPU/8-GiB Bifrost node durably acknowledges writes, retains and stages
their WAL without ambiguous owners, serves published and live data through the
existing single query plan, survives query cancellation, and meets the read,
write, mixed-workload, and scan targets in approved REQ-008. A passing gate
without valid workload measurements is not completion.

The current implementation has overlapping disk and Scribe-memory admission,
selective WAL seals with several retirement owners, upfront scratch charges
for queries that never spill, and a process-fatal resource mismatch when
cancelled work releases memory after its parent. TASK-003 removes those mechanisms instead
of extending them. Keep the public query contract, authenticated peer boundary,
Iceberg snapshot protection, bounded queue, and durable write ACK.

**Design rule:** Make invalid ownership transitions unrepresentable. A retained
WAL generation cannot become retired before every staged replacement is
durable; a query cannot return its slot while a child still runs or owns query
memory; each file has exactly one accounting owner.
Ownership moves when the required operation succeeds, not when a timeout,
poll, reconciliation pass, or process-wide poison notices an earlier mistake.
Deadlines for user queries and validation at trust boundaries remain part of
their actual contracts; neither is a substitute for resource ownership.

## Starting State and Earlier-Task Disposition

- TASK-002 is superseded. Work from it already committed on this branch is the
  baseline, not a separate task to finish. Retain working benchmark harness,
  queue, transport reuse, pruning, and focused fixes where they satisfy this
  task; delete obsolete paths and tests as their owner is removed. Do not reset
  the branch or reimplement working public contracts.
- TASK-001-R3 is marked `blocked`, but its committed ticket, replay, transport,
  telemetry, and benchmark changes are the current implementation baseline.
  Do not rerun or undo that implementation as a separate task. TASK-003 owns
  preservation of the authenticated published/live read in Scenario 1, its
  queue/overload behavior in Scenario 8, and its remaining capacity proof in
  Scenario 9; its final
  evidence feeds the cumulative TASK-001 review. `depends_on: [TASK-001]`
  names the existing implemented query path, not a claim that TASK-001 has a
  PASS verdict. TASK-001 review closure follows TASK-003; no overlapping R3
  execution remains.
- The WAL-only disk ledger, its directory walk and breaker, the Oracle volume
  class, and the volume governor are already deleted on this branch. Disk is
  provisioned, not tracked; do not reintroduce any of them.
- The uncommitted `oracle/admission.rs` change moves a five-second child-drain
  poll into a detached task. Do not commit that as the solution. Preserve it
  while working, then replace it with query-owned child lifetime and remove
  the poll, detached release, and timing-dependent test assumptions.
- Keep an exact before/after diff and the existing benchmark report. Change
  only these known uncommitted hunks when removing them; preserve any later or
  unrelated edits by another contributor.

**Execution order within this one task:** resolve the read-path cost in §5,
then the WAL group sync in §1, run one standard benchmark checkpoint, then
implement the ownership changes in §§2–4 and the queue contract in §6. Run
the final standard and heavy benchmarks after all scenarios. The checkpoint
is diagnostic evidence, not completion; if the read target needs a new
published-cut ownership protocol, stop before the ownership rewrite and
obtain that material decision.

## Owners, Scope, Consumers, and Prohibited Changes

- Gate validates and authorizes the public write/query request. Scribe alone
  owns WAL append, fsync, durable batch fence, memtable, generation rotation,
  staging, and WAL retirement. No Wyrd component tracks WAL, staged, or
  assembly disk bytes: the operator provisions the device, each shard's WAL is
  bounded by its own size trigger, and an out-of-space write fails before ACK
  as retryable `507`. DataFusion's existing `DiskManager` alone owns Oracle
  temporary spill files and enforces their per-query byte limit. Do not add a
  Wyrd spill-file implementation, a disk ledger, or a second charge for those
  files.
- `ScribeResources` is the only Scribe-wide memory capacity authority;
  `ScribeOwnership` carries its move-only active/immutable leases.
  Admission retains the in-flight item bound and per-table fairness, not a
  second global byte ceiling. Transport's body-byte admission remains the
  distinct pre-decode bound for requests held before Scribe sees them.
- Oracle alone owns the leader's query deadline, bounded tenant-fair queue,
  DataFusion graph, cancellation, result terminal, and every child lifetime.
  The pod governor is the one capacity authority for Oracle execution slots,
  including local leaders and remote followers. Admission owns queue order
  and pod-local tenant fairness, not a second class/slot capacity ledger.
  Oracle's memory pool remains the one process-wide memory authority. The
  benchmark uses the existing public client,
  process-cluster children, telemetry, and `bench:bifrost:query-capacity`.
- Keep WAL fsync and the durable batch fence **before** ACK, including batch-id
  retry/dedup behavior. Staging, publication, and compaction remain after ACK.
  A stage failure keeps the acknowledged WAL and its live source; it never
  converts an acknowledged batch into a lost or silently skipped batch.
- Keep per-tenant/table/partition files, tenant authority, Iceberg snapshot
  protection, published/live union, bounded streaming, query terminals,
  leader deadline, and the existing one-hour queue/two-hour default total
  timeout policy. Do not replace them with a published-only fast path, a second
  query endpoint, a benchmark-only server path, or a cache without an explicit
  invalidation proof.
- Do not add a custom spill factory/writer, second disk meter, WAL breaker,
  admission queue, polling guard, query retry, or compatibility machinery for
  code that has never shipped.
  Existing public errors may be mapped to the correct reason; do not classify
  shared-device pressure as WAL corruption or a replay/security violation.
- Do not leave a cleanup timer, eventual counter repair, or health-triggered
  process kill as the normal path for a cancelled query or retired WAL file.
  A child lease retains its parent capacity by construction; a generation
  retains its WAL paths until stage and unlink success by construction.

## Decisions and Implementation Approach

### 1. Preserve the write ACK and make pre-ACK admission truthful

Keep the current durability order while removing repeated fsync within one
shard group. `append_and_sync_batch_commits` currently syncs the growing
`state.touched` set once per batch, up to 64 times; the cost is code-proven but
not yet measured. Implement this order, including the already-existing SLICE
sync and individual PostgreSQL dedup fence:

```text
validate and reserve before WAL mutation
append all group SLICE records; fsync their distinct touched WAL segments once
append all complete-batch COMMIT records; fsync distinct touched segments once
for each batch in group order:
    commit its existing PostgreSQL batch-identity fence (serial, unchanged)
insert the complete committed group into the authoritative memtable once
ACK each batch only after its own fence and the group's insertion succeed
```

No batch is ACKed merely because another batch in the group was fenced. If
the process crashes after group COMMIT fsync but before all fences, replay and
retry of the same batch IDs must produce every accepted row exactly once.
Keep the per-batch PostgreSQL fences serial; changing them is a separate
durability decision. Reject a request before WAL mutation if its validated
size or WAL growth cannot be admitted. After ACK, staging and publication
remain asynchronous. Transient stage IO retries; a permanent invalid member
follows the explicit fail-closed policy in §3. Never move stage or publication
into the ACK path.

### 2. Give each resource one owner: provisioned disk, governed memory

**Shards.** Scribe runs `WYRD_MEM_TABLE_BUCKET_NUM` shards (default 1, at most
256). Each shard owns its own WAL stream and memtable. Routing is
`hash(tenant, table, batch_id) mod shard_count`; replay maps a recorded shard
onto the running count, so the count may change across restarts.

**Triggers.** Each shard rotates its WAL and memtable together when either
reaches its size or the generation reaches its age:

| Setting | Default | Counter it reads |
| --- | --- | --- |
| `WYRD_MAX_FILE_SIZE_ON_DISK` (MiB) | 512 | the shard's WAL bytes |
| `WYRD_MAX_FILE_SIZE_IN_MEMORY` (MiB) | 512 | the shard's memtable bytes |
| `WYRD_MAX_FILE_RETENTION_TIME` (s) | 600 | the generation's age |

Each trigger reads its own counter; no byte is charged to two triggers. The
staging target is `min(WYRD_MAX_FILE_SIZE_ON_DISK, Forge target file size)`,
and staged members drain by that size or by the same age. There is no separate
staging-target override and no active-generation budget.

**Disk is provisioned, not tracked.** There is no volume governor, WAL disk
ledger, free-space probe, percentage threshold, or sticky breaker. The
worst-case WAL footprint is `shard_count × WYRD_MAX_FILE_SIZE_ON_DISK` plus
retained generations awaiting stage; operators size the device for it. An
actual ENOSPC maps to `ScribeError::WalDiskFull` and the public retryable
`507`, before ACK. If the existing WAL rollback restores and syncs the
pre-append length, a later request retries once space returns; failed
rollback or ambiguous fsync closes **Scribe** admission until restart replays
the WAL.

**Memory stays governed.** Scribe memory competes with queries, so the global
memory governor remains. `ScribeResources` is the one Scribe-wide capacity
decision; ingress and generation leases charge it. Remove
`AdmissionController`'s separate 90-percent `memory_breaker_bytes` refusal
and duplicate active/immutable byte counters. Keep the in-flight **item**
bound and per-table contention policy. An admitted request must not be refused
by a second estimate after its root lease was granted.

**Spill.** Oracle spill is bounded per query by DataFusion's `DiskManager`
limit; §4 owns it.

**Integrity failure is role-local.** Scribe's WAL health signal is separate
from the process memory governor's. An uncertain WAL makes
`ScribeImpl::is_ready()` false immediately, so Gate refuses new writes without
ACK. The retained Scribe role uses its existing `start_draining()` transition
to close local readiness and heartbeat advertisement, then deactivates its
cluster role fence. Do not call `ScribeImpl::begin_shutdown()` on this fault;
it closes shard lanes needed to settle accepted work. The fault does not stop
Oracle, Gate queries, or Forge. Keep WAL and staged files untouched and do not
attempt in-process replay. A later restart runs stage recovery and WAL replay
**before** Scribe advertises readiness; if replay fails, boot keeps Scribe
unready, preserves its files, and continues independent roles. A WAL that
cannot be validated or repaired needs operator repair; do not truncate a
complete corrupt record or report missing data as recovered.

Published reads through a healthy Oracle remain available. If Oracle froze a
roster containing the Scribe before it became unavailable, the existing
terminal rules apply: `Degraded` with `LiveTailUnavailable` only for an
eligible loss before live rows, and `Failed` after live rows. Once the failed
Scribe is absent from the ready roster, a published-only result may be
`Success` under the approved best-effort live contract. State that limitation
plainly in the task evidence; do not add an owner inventory or new query
status.

On a combined Scribe/Oracle target, a failed Scribe alone leaves `/readyz`
and gRPC health ready **only while** every other required role and shared
dependency is healthy; the body still reports Scribe unready. On a
Scribe-only target, `/readyz` stays unready. This is one conditional on the
existing readiness snapshot, not a second endpoint.

At startup, stage recovery reattaches existing staged files without charging
them again; a restart produces the same Scribe memory totals as a clean start
over the same surviving files.

### 3. Pair WAL retirement with whole-shard generation rotation

One shard generation owns its closed WAL segment paths and every frozen
tenant/table/partition member produced by that rotation. The trigger contract
is:

| Trigger | Shards rotated | Selection |
| --- | --- | --- |
| WAL bytes reach `WYRD_MAX_FILE_SIZE_ON_DISK`, or memtable bytes reach `WYRD_MAX_FILE_SIZE_IN_MEMORY`, on append | The receiving shard only | Existing projected size rule; rotate before that append. |
| Periodic age tick past `WYRD_MAX_FILE_RETENTION_TIME` | Every nonempty shard past its generation age | Rotate even if that shard is idle. |
| Scribe root memory pressure | One whole shard per pressure check | Choose the largest active-byte shard; wait for staging to release root bytes, then re-evaluate against the existing low-water target. |
| Disk capacity | None | Disk is provisioned. An actual ENOSPC fails the append before ACK as `507`. |

With more than one shard, routing by batch ID spreads one table across
shards, so later checks may rotate other shards. Do not infer memory relief
merely from freezing: immutable batches still hold memory until staging
releases it. No new pressure API is needed.
A rotation drains existing Arrow batches without duplicating them, then
stages each member separately under its tenant/table/partition key. Bounded
staging workers remain; do not create a cross-tenant file. Measure whether
removing the disk trigger causes WAL-full refusals; that is a failed benchmark,
not permission to add a disk policy.

Retire the generation's WAL only after **all** members have durable,
validated, live-query-registered staged replacements. The shard generation
is the sole retirement owner; the WAL worker performs idempotent unlink IO.
Keep retirement intent with the generation until the worker confirms deletion.
If submitting or executing unlink fails, retain the paths and retry on the
next maintenance tick and on startup recovery. A crash between stage commit
and unlink replays the stage/WAL evidence and safely retries; unlink never
precedes stage authority. No new persistent retirement table is needed.

Once this lifecycle passes its journeys, remove selective member seals,
last-selective-member detection, `referenced_wal_paths`, rotation cohorts,
the WAL-writer **generation-retirement** refcounts, and the deferred-delete
set used to arbitrate several retirement owners. `ReplaySegmentPin` also uses
today's `retirement_refs`: keep its reader protection, replacing those shared
refs with a replay-reader lease checked by the generation's unlink operation.
No segment may be unlinked while a replay cursor can read it; a failed or
cancelled replay conservatively retains the segment until restart. Retain
staged-file reader leases separately. Do not delete either reader guard before
its replacement proves the same protection.

Stage errors have two outcomes. Transient IO, capacity, or worker faults retry
while the generation retains its WAL. A deterministic schema, layout,
validation, or corruption failure marks that member unhealthy, retains its
WAL, stops repeated futile stage submission, and emits the existing
Scribe error/health telemetry with tenant, table, shard, generation, and
reason. Keep this outcome on the owning pending generation; do not add a
separate health registry or repair service. The affected shard/key stops
accepting new writes **before WAL append and ACK**; use the existing
`WriterUnavailable` public error, not the retryable `IngestBusy` buffer-full
error. Generations already accepted for that key remain queued in FIFO order
behind the unhealthy member. Its immutable rows and those successors retain
their Scribe memory leases and remain live-readable; do not evict acknowledged
data, skip the member, or publish successors out of order. Stopping later
acceptance bounds further memory growth from this failure. Sibling keys and
tenants may stage, publish, and write while the shared Scribe root and device
have capacity; none can cause the shared generation's WAL to unlink early.
If retained immutable memory fills the root, other writes on that pod also
fail before ACK. This is a real per-pod availability limit, not tenant-data
exposure or silent loss. An unhealthy member remains visible to operators and
is retried only after repair or restart validation. Shared-device exhaustion
can likewise refuse writes for other tenants before ACK. Document both limits in
`architecture/operations/reliability-and-recovery.md`. Do not add an attempt
cap, quarantine store, or automatic deletion of acknowledged data.

Do not claim that retrying against another Scribe makes the original ACKed
batch available. `wyrd-client` retains the same batch ID and bytes on its own
retries, and PostgreSQL fences that logical identity across nodes, but the
first successful Scribe's WAL remains its durable source until staging. The
same-ID fence prevents a second insertion; it does not replicate or replace
the first pod's WAL. Keep this distinction in tests, operator docs, and the
task evidence.

Whole-shard rotation can stage younger buckets alongside one aged bucket.
Accept that simpler behavior; do not add selective sealing back based on a
theoretical small-file concern. Measure stage jobs, staged files and average
size, published file size, WAL bytes, time from ACK to WAL retirement, ingest
rows/s, and read latency in the standard workload. If the measured workload
misses a required target because of file fanout, optimize within the single
generation owner; do not reintroduce competing WAL owners.

### 4. Use DataFusion's spill files and own cancellation structurally

Keep Oracle's slots, tenant-fair queue, shared memory pool, and the existing
DataFusion local-directory `DiskManager`. Set its per-query spill ceiling to
`min(class spill quantum, resolved scratch_limit_bytes)`: the existing class
quanta are 256 MiB interactive and 512 MiB analytical. The existing
`bifrost.resources.scratch_limit_bytes` environment/config value remains an
upper bound for this DataFusion limit, not a precharge or shared byte ledger.
Delete the matching 256/512-MiB **physical reservation at query
admission**, the Wyrd root scratch counter, nested `try_split_scratch` ledger,
and Oracle volume lease. An unspilled query owns zero physical scratch;
DataFusion's own `used_disk_space()` reports bytes actually written. Do not
implement `TempFileFactory` or `SpillWriter`. If a query reaches its
DataFusion ceiling or the filesystem returns ENOSPC, fail that query and let
DataFusion remove its temporary files; do not poison or stop the pod. Scribe
checks current physical free space on its next WAL/stage growth and refuses a
new write **before ACK** if needed. Already ACKed WAL stays durable and
readable. This design does not promise that concurrent spilling can never
temporarily reduce Scribe's free space; report such interference in the
mixed-workload benchmark rather than hiding it behind a speculative second
reservation system.

Delete `LocalPermit`; do not replace it with `QueryCapacityOwner` or another
permit wrapper. `OracleQueryResources` already owns the governor's pod-local
slot charge, including remote follower charges. Its release is the single
capacity return. Admission keeps only its tenant FIFO, per-tenant active count,
and queue policy. Remove its duplicate class/total slot counts, `spill_used`,
the copied `RetainedQuerySessionShape`, and the independent capacity wakeup.
Use the governor's existing `oracle_capacity_changed` signal to reconsider
queued grants when any local or remote slot returns. Preserve the current
Interactive borrowing and Analytical maximum in the governor; grant and
release under its existing lock. The admission scheduler may choose a
tenant/class, but only a successful governor grant consumes a queue place and
tenant count. A remote follower has no leader-local tenant charge or queue
place. A local leader's tenant charge follows the same final resource release,
so a freed slot cannot wake a tenant queue before its fairness count is free.

The query's resource owner must remain alive for every DataFusion memory
reservation and Wyrd execution child. Use the existing types in this
cycle-free ownership shape:

```rust
// Existing resource owner holds the governor slot and, for a leader,
// the scheduler's tenant charge. It does not own the query pool or runtime.
Arc<OracleQueryResources>

// Existing per-query MemoryPool view retains that owner. Every DataFusion
// reservation retains its pool, so a live child retains its slot.
OracleQueryMemoryView { capacity: Arc<OracleQueryResources>, /* existing fields */ }
```

Move `OracleQueryResources`' pool field out of its release-only state before
the view retains it; the owner must not hold an `Arc` back to that view. The
query stream or graph retains the pool while running, and the pool retains the
resource owner while any reservation survives. The final drop returns the
governor slot and leader-local tenant count once, then wakes the queue. No
timeout, detached task, or second counter repairs an early release. Ordinary
late child release must not poison the process; genuine accounting corruption
still fails closed with a structured error. At completion, failure, deadline,
or disconnect: cancel the graph, drop its response stream and
`RuntimeEnv`/`TaskContext`, join Wyrd-owned descendants, and let the last
DataFusion pool view return capacity. No process-level cache may retain a
query runtime or pool after terminal; clear the current analytical task cache
on every terminal path.

One lifetime policy applies to **every remote query child**, regardless of
Interactive or Analytical class and regardless of whether the peer is Oracle
or Scribe: the leader's query stream owns the child RPC stream; the peer's
stream-bound execution owner retains its resource lease. On success, the
leader drains each child through its protocol end marker. On failure,
deadline, or client disconnect, it cancels the query and drops every child
RPC stream. The peer then cancels and settles that child, returning its lease
only after its work has stopped. The one
leader-signed absolute query deadline is the fallback when a broken connection
does not report closure. Do not add a peer heartbeat, another
query deadline, a shared lease registry, or a new common wrapper merely to
make the three paths look identical in type names.

The existing `ExecuteFragment` peer service already serves Interactive Oracle
worker fragments and Scribe live fragments through the same gRPC response
stream. Its `WorkerExecution` stream owns an Oracle running reservation and
worker resources on the Oracle branch, or the Scribe follower/staged-reader
leases on the Scribe branch. Keep those leases **inside** that response stream;
the leader's `LiveFragmentRead`/physical batch stream owns the receiving
stream. Dropping either side must drop its child future and lease. Apply the
signed deadline while opening and polling both branches, as specified below.
An Analytical query can also read a Scribe live fragment, so this rule is
based on the peer stream, not on the query class.

Across pods, **retain** the follower's existing settlement driver and the
signed absolute deadline supplied by the leader. The follower does not set a
new query deadline; the carried deadline is the final bound if the leader
disappears. An Analytical graph spans several task/data streams, so its one
long-lived `SetPlan` coordinator stream owns the graph rather than any one
unary task request. Fix the one gap before that connection exists: upstream can send
`ExecuteTask` before its asynchronously opened `SetPlan` coordinator channel.
Today either message activates a graph, so leader death in that interval can
leave a remote slot charged until the signed deadline. Make `SetPlan` the
only graph activator and connection owner. In the existing
`AnalyticalWorkerChannel`, share a query-local, per-`TaskKey` plan-ready
result across channels from `AnalyticalChannelResolver`:

1. `coordinator_channel` publishes ready only after the follower accepts and
   installs `SetPlan`; publish its error if installation fails. Closing the
   channel after installation remains the graph-departure signal.
2. `execute_task` waits for that matching result before forwarding its unary
   request. Use one query-local map keyed by upstream's existing `TaskKey`,
   with a single pending/ready/failed notification per task and no spawned
   waiter. The wait is dropped with the leader query's existing upstream
   `JoinSet` and is also bounded by its absolute deadline; an installation
   error ends it immediately. The map lives only as long as this query's
   resolver/channels, with no global cache or second timeout.
3. On the follower, `authorize_stage_message` accepts `ExecuteTask` only for
   an already active graph; it never activates one. Before `SetPlan`, only the
   existing pending reservation can exist, and its existing two-second
   `PENDING_TTL` releases it after leader loss. After `SetPlan`, the existing
   authenticated coordinator request stream holds `AnalyticalConnectionLease`;
   its drop starts settlement when the last connection closes.

Do not treat a unary `ExecuteTask` body or an individual data exchange closing
as graph death, and do not add a heartbeat. Make the **graph map** the only
settlement inventory. The last `AnalyticalConnectionLease` drop changes its
graph from Active to DrainingPending under that map's lock and signals the
existing settlement driver with `tokio::sync::Notify`. The driver scans the
map before sleeping, takes each pending graph exactly once into
DrainingRunning, and joins its settlement; the same transition handles signed
deadline expiry and shutdown. `Notify` cannot refuse a transition, and a
wake that arrives before the driver waits remains visible through the map.
On successful settlement remove the graph; on failure retain its owner and
failure record for readiness/shutdown inspection. During shutdown, stop new
activation, mark remaining active graphs pending, wake and join the driver,
then inspect retained failures. Delete `GraphSettlementCommand`, the bounded
settlement channel, `SettlementQueueRefusal`, `offer_settlement`, and their
capacity arithmetic/test. No second queue, heartbeat, cleanup task, or retry
owner remains between connection loss and the graph's settlement owner.

Last-drop release also requires child work to end. The leader's catalog pin
already races the query deadline, and its PostgreSQL and Iceberg calls are
awaited inside that bound. `BifrostStorage` already bounds object-store
attempts and total retries. Keep those policies. Close only these concrete
gaps:

1. In `oracle/dispatcher.rs::execute_with_capacity`, apply the already
   computed signed `follower_deadline` while awaiting
   `PhysicalPlanFollower::execute`, which resolves catalog providers before
   it returns a stream. Expiry drops that future and its reservation/worker
   resource owners and returns the existing `DispatchError::Unavailable`.
   Its batch stream already enforces the same deadline in
   `encode_attempt_frames`.
2. In `wyrd-server/src/oracle/peer_service.rs::execute_scribe_fragment`,
   derive one absolute deadline from the verified leader-signed peer claim.
   Enforce it both while awaiting `fragment_follower().execute` and while
   polling each `batches.next()`. Expiry drops the follower stream and its
   lease, and emits a terminal fragment error so a query whose own deadline
   expired cannot be reported as a successful degraded live read. Ordinary
   tonic response-stream drop remains the immediate cancellation path for
   detected disconnects. Both local and remote Scribe fragments use this
   peer path.
3. In `catalog/iceberg_storage.rs`, race each awaited
   `EpochGatedIcebergStorage`/`EpochGatedFileRead` inner `exists`, `metadata`,
   `read`, `reader`, `list`, and range-read call against the `ReaderIoPermit`'s
   **existing** query and epoch cancellation tokens and no-IO deadline. Keep
   the existing checks before IO and before exposing a result. Iceberg's
   `plan_files` starts detached
   manifest tasks; dropping its output stream alone does not stop an in-flight
   manifest read. `ReaderQueryGuard` already cancels the permit on query drop,
   so this race drops the inner `BifrostStorage` future and returns its storage
   permit promptly. The detached planner then sees its closed result channel;
   it does not own an Oracle slot. Do not replace Iceberg planning or add a
   second cancellation owner.
4. In `oracle/analytical_transport.rs`, replace the query-local peer channel
   cache's `endpoint.connect().await` **under its mutex** with tonic's
   `Endpoint::connect_lazy()` inserted under a plain short
   `std::sync::Mutex` lock. TLS and peer workload
   authentication remain on the channel and every RPC. Bound the first
   `coordinator_channel` and `execute_task` network awaits by the already
   signed absolute leader deadline; query cancellation drops those call
   futures. This removes a serial, unbounded dial from the query path without
   adding a transport timeout setting or a second pool. The returned streams
   remain owned by the existing graph and query lifetime.

Use the existing leader deadline only; add no per-file timer, second
cancellation owner, or generic reaper. A query that has reached a
terminal but still holds capacity must be observable: reuse the current
running-query/resource gauge and graph-settlement failure record where they
already cover it; for local child residue, emit one structured terminal-held
event and one retained-count gauge, decremented by the same final release.
Record query identity and retention age when it eventually releases. No
periodic scan or age-based forced release is authorized.

DataFusion `RefCountedTempFile` owns `Arc<DiskManager>`, **not** the memory
pool or slot. Do not claim spill-file drop releases a slot or add a custom
spill-file wrapper. DataFusion owns spill cleanup; the query owns its
execution graph until it has stopped. Test the release with a controlled
reservation whose final drop is explicit, not a sleep or a five-second poll.
Remove the detached drain/poll and process-fatal poison for ordinary late
child release. Cancellation of one query cannot alter another query's
resources or terminal.

### 5. Cut measured read work before the ownership rewrite

The current selective read is 29.9 ms p50 at one client; the complete
`snapshot_pin` phase averages 7.9 ms, including `table_lookup` 2.6 ms,
`metadata_load` 1.9 ms, `hot_cut` 1.7 ms, and `revalidation` 1.6 ms. These
subphases overlap the enclosing `snapshot_pin` timer and must **not** be added
to it. Three Wyrd table-reference parses take microseconds and are not the
main lever. Current SQL-catalog `load_table` does an existence SELECT, a
metadata-pointer SELECT, and an object-store metadata JSON read **twice**
per table: before and after the reader guard.

First streamline this existing protected-cut path, without a new cache:

1. Keep tenant registration/authorization before reader protection. Resolve
   a registered table and its authoritative Iceberg metadata pointer with
   the fewest existing tenant-scoped SQL reads; load that immutable metadata
   document once. Do not add an unscoped catalog shortcut.
2. Acquire the existing guard for the complete multi-table cut before any
   manifest or hot-file source IO. After the guard, re-read the authoritative
   **pointer/version only**. If it changed, discard the whole attempt and
   use the existing single promotion retry. Do not reload the same JSON just
   to compare it with itself. This read belongs to the existing Iceberg SQL
   catalog connection and catalog-owner credential. The request-role
   `TenantConn` is intentionally denied `iceberg_catalog` access by
   `20260619000000_iceberg_catalog.sql`; do not grant it access or query the
   catalog table from that role. Add a narrow pointer-only method to the
   pinned `iceberg-catalog-sql` owner, then call it from `BifrostCatalog`;
   use the catalog owner's existing authoritative **primary** connection,
   never a lagging read replica, and retain the exact catalog/table/namespace
   predicates. Do not create another pool or catalog implementation.
   Pin the dependency revision in workspace `Cargo.toml`/`Cargo.lock` to the
   reviewed method. Do not create a second catalog pool or Wyrd copy of the
   Iceberg SQL catalog.
3. This pointer check is sound only if every supported catalog commit writes
   a new immutable metadata location and never changes a document in place.
   Prove that invariant against the pinned Iceberg SQL catalog, Wyrd's
   publication path, and a promotion-race test before replacing document
   equality. If an in-place change is supported, keep full revalidation and
   stop for a material consistency decision **before** §1 or §§2–4 work. If
   the catalog-owned pointer API cannot be added to the pinned dependency,
   stop at the same point; do not improvise a less-privileged SQL shortcut.
4. Reuse the validated metadata/manifest projection within this one attempt;
   keep the current tenant-scoped hot-file cut and exact published/hot
   overlap subtraction. Parse Wyrd SQL table references once as minor cleanup.
   Keep DataFusion's own SQL parsing, the one published/live plan, and peer
   transport reuse.

After this and §1, run the standard benchmark checkpoint. If selective
latency or QPS still misses its target, report the remaining registration,
pointer, object metadata, manifest, hot-cut, row-scan, and client costs before
changing anything else. A versioned in-memory published cut would require a
new promotion/invalidation and file-retirement contract across Oracle nodes;
no time-based cache or stale pointer is authorized by this task. If that is
the only credible way to meet the target, seek the user's explicit decision
**before** the ownership rewrite. Keep the target and failed evidence intact.

For diagnosis, retain end-to-end client latency and the existing Oracle phase
timers, and add narrow wall-time spans around actual PostgreSQL statements
and object-store metadata reads so wait time is distinguishable from CPU time.
Use PostgreSQL statement timing where available; do not infer database time
from phase names. At one client and saturation concurrency, run Linux `perf`
against the **server child PID** in a separate diagnostic window for on-CPU
stacks. Reuse `BifrostProcessCluster::nodes()` and `ProcessNode::pid()`; save
PID, cgroup, SQL/object timing, `perf.data`, leading symbols/sample shares,
and cgroup CPU beside phase timings. `perf` is not installed on this host;
install/authorize it before claiming CPU attribution. If unavailable, report
CPU owner unknown, while still reporting off-CPU wall/statement evidence.
Do not write a Wyrd profiler or a second benchmark driver.

### 6. Keep the queue and benchmark as the truth

Preserve the one 1,000-waiter queue and the shared timeout policy. The 1,000th
waiter fits; the 1,001st **already gets an immediate refusal**, but today it
shares `WYRD_VALA_429_QUERY_ADMISSION_REJECTED` with other failures. Add one
public `BifrostError::QueryQueueFull` variant with code
`WYRD_VALA_429_QUERY_QUEUE_FULL`, HTTP 429, retryable overload detail and
remediation "Retry after capacity becomes available." Reuse the existing
transient-capacity headers (`Retry-After: 1` on HTTP and
`retry-after-ms: 1000` on gRPC); keep the canonical problem code in both.
Map only the existing
`OracleAdmissionReason::QueueFull` branch to it; shutdown, unavailable class,
membership loss, genuine resource fault, and timeout keep their distinct
semantics. Propagate the stable code through HTTP/gRPC problem mapping,
`wyrd-client` Rust error decoding, generated TypeScript error codes, Python
and TypeScript SDK projections, and served documentation. No compatibility
alias is needed because nothing has shipped. Saturation below the queue limit
waits under the leader deadline. A
cancelled waiter leaves promptly. A full queue must stay within the 8-GiB
node, and a cancelled running query must return capacity only after its child
work ends.

Reuse the existing standard and separate heavy benchmark commands. Run the
diagnostic standard checkpoint after Scenarios 1–2, then run the final
standard benchmark **first after all remaining changed scenarios are
implemented and their focused tests pass**, followed by the heavy
qualification. Neither a
correctness gate nor a favorable p95 from only accepted queries is a
performance verdict. Keep the standard suite human-readable and approximately
10–15 minutes after build; report actual duration. Do not add a cold/hot,
file-layout, or parameter cross-product suite.

## Ordered Implementation Scenarios

### Scenario 1 — Preserve the snapshot race while cutting catalog work

**Behavior.** A selective read identifies the tenant table and immutable Iceberg metadata once, protects the complete multi-table cut, rechecks the authoritative metadata pointer after protection, and then reads the matching manifests and hot-file cut. A promotion between preparation and protection restarts the whole attempt once; in-place metadata mutation is forbidden by the supported writer contract. No query observes a partially updated cut.

**RED.** Add a focused catalog/planner test that counts metadata JSON loads and catalog pointer lookups for one stable read, plus a promotion-race test that changes the pointer between preparation and guard acquisition and asserts the whole attempt restarts. Prove the pinned Iceberg SQL catalog and Wyrd publication never overwrite an existing metadata location; the catalog-owner credential reads the authoritative primary pointer while the request-role credential is still denied it. Run the Postgres-backed catalog test with `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::bifrost_catalog::tests::protected_cut_reuses_immutable_metadata)'` and the planner test with `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::planner::tests::promotion_between_prepare_and_guard_restarts_whole_cut)'`; confirm exact selectors with `mise exec -- cargo nextest list --locked -p vala-bifrost-redux --lib` after adding them. Run `mise run test:bifrost:journey:oracle` for the real-server published/live and promotion paths.

**GREEN.** Replace the second full table load with an authoritative pointer/version read under the existing guard, reuse the first immutable metadata document, retain tenant-scoped registration and hot-file lookup, and keep the one whole-attempt promotion retry. If the immutable-location premise fails, stop before Scenario 2; do not weaken the revalidation. Parse Wyrd SQL table references once, without treating that microsecond cleanup as the main capacity fix.

**REFACTOR.** Delete only redundant catalog existence/metadata loads and repeat Wyrd parses. Retain the reader guard, permit-gated object IO, exact published/hot subtraction, and DataFusion SQL plan.

### Scenario 2 — Sync one WAL COMMIT group once

**Behavior.** A shard group writes all complete-batch COMMIT records, syncs its distinct touched segments once, then commits each batch's existing PostgreSQL fence serially. Every ACK remains after its own fence and the complete group's memtable insertion. A crash after group fsync but before all fences recovers and deduplicates retries by exact batch ID.

**RED.** Add a focused WAL group test that counts sync submissions for a multi-batch group. In a Postgres-backed **in-process integration test**, stop the Scribe owner immediately after the one group COMMIT sync and before the last batch fence, without graceful closeout; reopen the same WAL, retry the same IDs, and assert each fenced batch appears once and no unfenced batch was ACKed. This tier owns the precise crash window because `process_cluster` can kill a pod but cannot target an internal instruction. Use `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::group_commit_syncs_once)'` and `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::pg_tests::group_commit_crash_between_sync_and_fence_replays)'` after confirming the new selectors. Keep `mise run test:bifrost:journey:scribe` for a real client write, ungraceful whole-pod kill/restart, and read-back. No child-process failpoint or production feature is added.

**GREEN.** Follow the §1 ordering; keep the already-existing SLICE sync, individual serial PostgreSQL fences, retained ambiguous group owner, memtable insertion, and retry/dedup fence. Remove only the cumulative per-COMMIT sync.

**REFACTOR.** Delete the per-batch cumulative sync submission and its now-obsolete assertions without changing ACK, fencing, or replay semantics.

## Mid-task benchmark checkpoint

After Scenarios 1 and 2 and their focused/journey tests pass, run `mise run bench:bifrost:query-capacity` once on this Linux host. It may exit nonzero because later scenarios remain red; save the complete report, raw samples, logs, child cgroup evidence, phase means, PostgreSQL/object-store wait timing, and any CPU profile. Compare selective p50/p95/p99 and QPS, snapshot subphases, ingest ACK rate/p95, and WAL sync count with the saved baseline. Attribute improvement only to these two changes. If the selective target still misses, identify the measured remaining owner and apply the material-stop rule in §5 before starting Scenario 3. This is a diagnostic checkpoint, not an acceptance gate or a reason to run the broad repository gate.

### Scenario 3 — Durable ACK and single-charge recovery

**Behavior.** A successfully ACKed batch survives an immediate crash/restart,
is read before and after staging, and is never ACKed if WAL fsync or durable
batch commit fails. Recovering staged files charges their bytes exactly once.

**RED.** In the existing Scribe journey and stage recovery owner, drive the
ACK/failure/restart path and compare Scribe memory totals before and after restart;
the present readmission path should overcount existing staged files. Use
`mise run test:bifrost:journey:scribe` and
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::staging_runtime::tests::recovered_staged_bytes_are_charged_once)'`.

**GREEN.** Keep the ACK order and make startup scan the sole charge for
recovered staged files. A stage failure leaves the WAL/live rows recoverable.

**REFACTOR.** Delete the recovery recharge and tests that assert duplicate
accounting, without weakening checksum or stage validation.

### Scenario 4 — Provisioned disk, governed memory, configurable shards

**Behavior.** The four `WYRD_*` settings configure shard count and the three
per-shard triggers; zero or overflowing values are refused at boot by config
field name. No Wyrd component tracks disk bytes. A full device refuses the
affected append before ACK with retryable `507`; after space returns and the
WAL rollback succeeded, the next write succeeds without restart. A request
with a valid Scribe root memory lease is not refused merely because a second
90-percent estimate was crossed; item and table fairness remain. If WAL
rollback or fsync leaves integrity uncertain, only Scribe becomes unready as
§2 specifies. A pod with several shards spreads one table across them and
publishes each row exactly once; a restart with a different shard count
replays every ACKed row exactly once.

**RED.** Config tests cover the env overrides, defaults, zero and megabyte
overflow refusals, and `min(on-disk size, Forge target)` staging derivation.
Inject a WAL `StorageFull` whose rollback succeeds and prove the write is
refused with `507` before ACK and a later write succeeds. Add an admission
test in which the Scribe root grant succeeds above the removed 90-percent
cutoff while item and table rules still apply. Inject a failed rollback or
ambiguous fsync and prove the §2 readiness outcome on combined and
Scribe-only targets. Journeys run the default single-shard pod; one
dedicated journey runs a small multi-shard pod. Use
`mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^config::tests::/)'`,
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::admission::tests::root_memory_grant_has_no_second_global_ceiling)'`,
and `mise run test:bifrost:journey:scribe` (including
`cross_shard::scribe_cross_shard_generations_publish_packed_objects_once`).

**GREEN.** Configure shards and triggers through `ScribeGeometry`. Map
ENOSPC to `507`. Make the Scribe root the only RAM capacity decision.
Separate Scribe WAL health from process memory health and deactivate only
Scribe's cluster fence on WAL integrity failure.

**REFACTOR.** Delete the separate Scribe 90-percent breaker, mirrored RAM
counters, and their obsolete tests.

### Scenario 5 — One WAL generation owner, including failed retirement

**Behavior.** WAL size, memtable size, periodic age, and memory pressure
rotate the whole selected shard as specified in §3; nothing rotates or refuses
on a disk percentage. Each tenant/table/partition member stages separately. A generation
retires no WAL until every member is durable and every replay reader is done.
Failed retirement submission or unlink leaves the generation retryable. A
permanently invalid member stays unhealthy and observable without an
unbounded retry loop; sibling tenants can continue while disk has capacity.
Restart never loses an ACKed row or deletes its last durable source.

**RED.** In the existing shard test owner, inject a failed retirement submit
and unlink, hold a `ReplaySegmentPin` during an otherwise eligible unlink,
and inject a deterministic stage validation failure in one tenant's member.
Assert no early unlink or futile resubmit and an observable unhealthy reason.
A later generation already accepted for the same shard/key waits in FIFO order;
a newly offered write to that key gets `WriterUnavailable` before WAL append
or ACK, while another tenant stages and writes until the shared device or
Scribe memory root fills. A root-full refusal is before ACK. Use
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::generation_retirement_survives_submit_and_unlink_failure)'`.
For the exact stage-commit-before-unlink window, use a Postgres-backed
in-process integration test that stops the owner without graceful closeout,
reopens the same files, and proves all ACKed rows return once; run
`scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::pg_tests::stage_commit_before_wal_unlink_replays)'` after confirming the selector. In `mise run test:bifrost:journey:scribe`, write multiple keys on one shard, force idle age rotation, kill/restart the pod without targeting an internal instruction, and read all rows. No cross-process failpoint is added.

**GREEN.** Pair rotation, member stage completion, replay-reader pins, and
WAL retirement under one shard generation; retain retry intent through
submit and IO failures. Classify transient versus deterministic stage errors
per §3, keep the WAL for both, and reject later writes to an unhealthy key
before they enter WAL while allowing sibling keys to proceed.

**REFACTOR.** Delete selective seals and separate generation-retirement
refcounts/cohorts/deferred-delete machinery only after replay pins and failed
members remain protected by the replacement.

### Scenario 6 — Actual spill bytes, no speculative query scratch

**Behavior.** Many point reads consume no Oracle scratch reservation. A
spilling query stays within DataFusion's existing per-query spill limit. If
that limit or the filesystem is exhausted, the query fails and DataFusion
removes its temporary files; another query and the pod stay healthy. Scribe
refuses an unadmittable new WAL append before ACK.

**RED.** In Oracle/resource tests, prove an unspilled query charges zero and
exercise DataFusion's ordinary spilling query at its existing per-query
ceiling and at an exhausted test filesystem. The current upfront reservation
should fail the zero-charge assertion. Run
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=resources::tests::unspilled_query_holds_no_scratch)'` and
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::spill::tests::native_spill_limit_fails_one_query_and_cleans_up)'`.

**GREEN.** Keep the existing DataFusion `DiskManager` and its per-query limit;
remove only Wyrd's speculative reservation and verify its standard failure
and cleanup behavior through the query stream.

**REFACTOR.** Delete admission-time scratch leases, nested scratch split
accounting, and Oracle's governor charge. Keep DataFusion's own spill limit.

### Scenario 7 — One slot owner through cancellation and leader loss

**Behavior.** A client drops a streaming scan while a DataFusion child still
holds query memory. That query cancels, its capacity stays owned
until the child exits, other queries continue, and the pod remains healthy.
The same holds for deadline and failed-terminal teardown. Local leaders and
remote followers consume the same pod governor slots; tenant-fair queue
selection does not keep a second class/slot count. A terminal query holding
capacity is visible until final release. If a distributed leader dies,
the follower's existing coordinator-channel lease closes its graph and
returns remote slots promptly; the leader-supplied deadline remains the
last-resort bound if the connection does not close. `ExecuteTask` arriving
before `SetPlan` waits at the leader, so it cannot create a remote graph with
no connection owner. Interactive Oracle fragments and Scribe live fragments
use the same `ExecuteFragment` response-stream lifetime: ending the leader
stream ends the peer execution and its lease after child work stops. An
Analytical query's Scribe fragments obey that same rule. Cancellation drops an
in-flight published manifest read, remote follower setup, remote peer call, or
Scribe live read and returns its owned permits without affecting another query.

**RED.** Add a controlled child-release test in Oracle and a process-cluster
Oracle journey that disconnects mid-scan and then completes an independent
query. The controlled test holds one reservation after terminal, checks the
governor slot and retained-capacity signal stay charged, drops it, then
checks both clear and a queued tenant wakes. Prove the governor still allows
Interactive borrowing while limiting Analytical work, and that one leader
grant charges slots only once. Include a remote follower slot return in that
wakeup proof; the old local-only wakeup should fail it. In the
multi-node Oracle journey, kill the leader during a real analytical query,
observe that the follower's coordinator channel closes and its remote slots
return before the query deadline, and complete another query on that follower.
In that existing process-cluster journey, also cancel an Interactive query
mid-stream while a remote Oracle worker is reading. With Scribe on another
pod, drop the client during an Interactive live read and kill the leader
during an Analytical live read after its first batch. For each case,
observe the peer response stream close, its running/follower lease return,
and another query on that peer succeed before the original deadline. A
completed query must deliver its required footer/end marker and leave no
peer lease behind. Use the existing transport and resource observations;
add no benchmark-only path or new control RPC.
In a focused follower test, drop the last coordinator leases for several
graphs in one scheduler turn and prove each graph-map transition settles once,
including a wake before the driver waits. Shutdown must join the driver and
report a deliberately failed settlement without losing its owner.
Force `ExecuteTask` before `SetPlan` and prove it holds no active follower
graph/slot; accept the plan and prove execution starts, then repeat with plan
failure and leader-side cancellation before installation, proving only the
existing pending reservation remains and it expires by `PENDING_TTL`. The
process-cluster leader-kill journey covers death after installation. Hold a
published manifest read, a remote follower provider resolution, and a Scribe live
producer in focused tests; cancel or pass the already signed deadline and
prove each owned storage/follower permit returns, no further read starts, and
the subsequent query succeeds. A slow TLS peer must not hold the channel-cache
lock or keep a stage call alive past the signed deadline.
Use bounded event/condition waits, not arbitrary sleeps. The current immediate
release poisons or the uncommitted five-second poll relies on timing. Run
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::admission::tests::cancelled_query_retains_capacity_until_last_child)'` and
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::analytical::tests::last_connection_drop_settles_without_queue)'` and
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::analytical_transport::tests::execute_waits_for_set_plan_owner)'` and
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::analytical_transport::tests::peer_dial_does_not_serialize_or_outlive_deadline)'` and
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::dispatcher::tests::follower_setup_deadline_returns_slot)'` and
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::iceberg_storage::tests::cancelled_manifest_read_returns_storage_permit)'` and
`mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=oracle::peer_service::tests::scribe_fragment_deadline_releases_lease)'` and
`mise run test:bifrost:journey:oracle`.

**GREEN.** Delete `LocalPermit` as specified in §4. Keep the governor's one
slot charge and attach leader-local tenant fairness release to the same final
query-resource lifetime. Make the pool view retain that owner without an
`Arc` cycle; join Wyrd children on cancellation and clear terminal
runtime/task references. Preserve the existing coordinator connection lease
and follower deadline settlement, but replace the fallible settlement command
queue with the graph-map transition and `Notify` in §4. Make `SetPlan` the sole
graph activator, share only its query-local per-task readiness result with
`ExecuteTask`, and
keep remote peer dial and stage calls inside the signed deadline as specified
in §4. Keep `OraclePeerService::execute_fragment`'s `WorkerExecution` inside
its returned gRPC stream for both Oracle and Scribe roles, and keep the leader
batch stream as the owner of that response stream for both query classes.
Cancel/drop these streams from the one leader query cancellation and terminal
path; use no class-specific cancellation service. Bound the identified
follower setup and Scribe stream gaps; make the existing reader permit cancel
detached Iceberg manifest IO.
Expose local terminal-held capacity. The controlled test releases its last reservation
explicitly, without scheduler timing.

**REFACTOR.** Delete the duplicate leader-only class/slot counters,
`LocalPermit`, `RetainedQuerySessionShape`, its queue wake, the detached drain
and polling schedule, the graph-settlement command queue and its capacity
test, and any process-fatal path reached solely by ordinary
delayed child teardown. Keep the tenant FIFO/count, governor capacity wake,
coordinator-channel lease, and signed graph deadline.

### Scenario 8 — Queue-full is distinct from timeout

**Behavior.** With slots occupied, 1,000 waiters retain bounded places; the
1,001st receives the new typed, retryable queue-full code instead of today's
generic admission code. A cancelled waiter drops
its place, and a new waiter can enter. The running query's cleanup does not
start a wake/retry loop while physical resources are unchanged.

**RED.** Extend the existing queue-capacity test and Oracle journey to assert
the precise error code and bounded queue memory. The current server refuses
place 1,001 immediately under the shared admission code; the current
benchmark records overflow `None`, which must be diagnosed as a harness
measurement gap. Assert shutdown and unavailable class are not mislabeled
queue-full. Run
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::admission::tests::queue_full_is_distinct_from_timeout)'` and
`mise run test:bifrost:journey:oracle`.

**GREEN.** Map the existing queue-full branch to the new catalog variant,
project that code through all client surfaces, fix the benchmark overflow
classification, and wake the existing queue only when slots or genuinely
usable resources return.

**REFACTOR.** Remove duplicate wake paths and any second admission refusal
that bypasses this queue.

### Scenario 9 — Valid workload and capacity proof

**Behavior.** The existing standard suite and heavy qualification return
exact results and the REQ-008 targets on the specified 4-CPU/8-GiB local
process. Concurrent writes meet the durable-ACK target without unexpected
WAL_DISK_FULL; read-back works; disconnects and the full queue do not kill
the pod. Each result includes the resource and file-shape evidence below.

**RED.** Retain the current report as baseline and run no new baseline load.
The latest reported standard run misses Q1, some aggregates, memory, and
mixed-workload acceptance; those remain red until a valid new run proves
otherwise. Benchmark code must mark wrong results, failed held streams,
unexpected refusals, missing physical scan bytes, and missing child cgroup
proof invalid.

**GREEN.** After Scenarios 1–8 pass their focused tests, run the final standard
benchmark, diagnose every failed target from saved phase, CPU, resource, and
file-shape evidence, make the smallest root-cause change, rerun affected
windows, then run the complete standard and heavy modes.

**REFACTOR.** Remove temporary probes and duplicate fixture/report logic;
retain one readable report and reusable production metrics.

## Acceptance Criteria

1. Durable ACK remains after WAL fsync and batch fencing. Every ACKed batch
   survives restart and reads from WAL/memtable or its staged replacement;
   staging and publication may occur later. A pre-ACK disk or WAL error does
   not ACK. A group submits one distinct-segment SLICE sync and one
   distinct-segment COMMIT sync, keeps serial per-batch PostgreSQL fences,
   and ACKs only after its own fence and group insertion. Crash between sync
   and the last fence does not lose or duplicate a retried batch. The precise
   sync/fence window is proved in-process against PostgreSQL and WAL replay;
   the process journey proves client ACK and whole-pod restart/read-back.
2. Scribe runs `WYRD_MEM_TABLE_BUCKET_NUM` shards (default 1), each with its
   own WAL and memtable, rotated by `WYRD_MAX_FILE_SIZE_ON_DISK`,
   `WYRD_MAX_FILE_SIZE_IN_MEMORY`, or `WYRD_MAX_FILE_RETENTION_TIME`, each
   reading its own counter. The staging target is
   `min(on-disk size, Forge target)`; the separate staging-target override
   and active-generation budget are gone. A multi-shard pod publishes every
   row once, and replay after a shard-count change loses and duplicates none.
   No Wyrd component tracks disk bytes: the volume governor, WAL-only ledger,
   walk, thresholds, and breaker are gone, and DataFusion alone bounds Oracle
   spill. ENOSPC fails the affected write before ACK with retryable `507`;
   with a successful WAL rollback the next write succeeds without restart.
   The Scribe root alone decides global RAM capacity; a second 90-percent
   refusal and mirrored active/immutable byte counters are gone. In-flight
   item bounds and per-table fairness still work. An uncertain WAL stops
   Scribe readiness and all writes assigned to it without ACK, but does not
   terminate the server or close Oracle/Forge; startup replay is required
   before Scribe is ready again. A failed startup replay leaves only Scribe
   unready and retains its files; complete-record corruption needs operator
   repair. A combined target's `/readyz` and gRPC health remain ready if every
   other required role and shared dependency is healthy; a Scribe-only target
   is unready. Published reads remain available; a known source loss after
   roster freeze obeys the existing degraded/failed terminal rules, while a
   later ready-only roster cannot identify rows on the failed Scribe.
3. Whole-shard size, idle-age, and one-shard-at-a-time memory-pressure
   rotation follow §3; nothing rotates or refuses on a disk percentage. Each rotated shard generation owns its WAL until all
   per-key stage replacements are durable and registered **and replay readers
   are done**. Failed submit/unlink and crash recovery keep retirement
   retryable. A deterministic invalid member retains WAL, stops futile
   resubmission, reports an unhealthy reason, and lets siblings progress while
   capacity remains. Already accepted same-key successors wait in order;
   later offers to the unhealthy shard/key receive existing
   `WriterUnavailable` before WAL/ACK. Its ACKed rows retain WAL and live
   memory. If their retained memory fills the Scribe root, other tenants on
   that pod are refused before ACK; no task claim treats another replica as a
   replacement for the original WAL. The exact stage/unlink crash window is
   proved in-process against PostgreSQL and replay, with a separate real-pod
   kill/restart journey. No ACKed row is lost. Selective retirement machinery
   is gone; replay protection remains.
4. Unspilled queries hold zero scratch bytes. DataFusion bounds each spilling
   query with its existing limit and cleans up its own files; filesystem
   exhaustion fails that query, and a new unadmittable Scribe write fails
   before ACK. The governor alone counts local and remote Oracle slots;
   admission retains the bounded tenant-fair queue and leader-local tenant
   counts but no duplicate class/slot counter or `LocalPermit`. Cancellation
   releases the governor slot and tenant count only after the last memory
   reservation and Wyrd child end; a DataFusion spill file is not falsely
   treated as the slot owner. Terminal-held capacity is observable. A
   follower keeps its leader-supplied absolute deadline as final bound, while
   closing the existing coordinator channel after leader death releases its
   graph and slots promptly. `ExecuteTask` cannot activate a graph before
   `SetPlan` owns that channel. A last connection drop records settlement in
   the graph map and wakes the one driver without a fallible command queue;
   several simultaneous drops settle once each, and shutdown joins them.
   Interactive Oracle peer fragments, Scribe live fragments under either
   query class, and Analytical coordinator graphs all follow the same leader
   stream lifetime policy in §4. On observed leader-stream closure, the peer
   stops work and returns its lease after descendants stop; normal completion
   validates the protocol end marker. Remote follower setup, peer calls, and
   Scribe live reads use that same absolute deadline; query cancellation ends
   detached Iceberg manifest IO through its existing reader permit. No
   ordinary delayed child can poison or terminate a healthy pod.
5. One Wyrd table-reference parse remains. The protected-cut path loads one
   immutable metadata document, rechecks the catalog pointer after reader
   protection, and restarts the whole attempt if it changed. The catalog-owner
   credential reads the authoritative primary pointer; no read replica or
   request-role query is substituted. Iceberg snapshot protection,
   authenticated published/live plan, distributed workers, and terminal
   behavior remain intact. The 1,000-place queue and leader deadlines work;
   place 1,001 returns `WYRD_VALA_429_QUERY_QUEUE_FULL` immediately while
   other admission failures keep their own reason.
6. The after-Scenarios-1-and-2 standard checkpoint reports selective QPS and
   latency, pin subphases, PostgreSQL/object wait, WAL sync count, write ACK
   rate, and the remaining bottleneck, with no broad gate run yet. A valid
   final standard run reports every REQ-008 workload and concurrency,
   client p50/p95/p99, successful QPS, offered/refused/wrong results, physical
   scan bytes/s, CPU, peak cgroup memory, ACKed rows/s and input bytes/s,
   batch p95, staged jobs and files/s, average staged and published file
   sizes, WAL bytes and ACK-to-retirement time, actual Oracle spill bytes,
   read-back, queue outcome, and mixed read degradation. Report setup and
   measurement time and retain raw samples, pod logs, CPU profile for missed
   selective targets, telemetry,
   cgroup proof, SQL/fixture checksum and geometry. Count page cache in memory.
7. Numeric REQ-008 targets pass on valid full runs: selective p50 <2 ms,
   p95 <5 ms, p99 <10 ms and >1,000 successful QPS at stated concurrency;
   small aggregate >=100 QPS and p95 <100 ms; medium >=20 QPS and p95 <300
   ms; 10M-row aggregate p95 <300 ms; heavy queries <2 s and full-scan
   physical throughput >=500 MB/s; ingest and mixed ingest >=100,000 durable
   rows/s; mixed analytical >=100 QPS with <20% read p95 degradation; peak
   memory <7 GiB and no OOM. The separate 100M-row qualification has its
   actual seed time stated. A miss is a FAIL, never relabelled complete.
8. The diff removes redundant state and paths. Report before/after production
   lines and the removed owners; a net increase requires a concrete reason
   tied to a retained guarantee. Update `architecture/bifrost-design.md`,
   `architecture/references/domain/datafusion.md`,
   `architecture/references/domain/olap-serving.md`,
   `architecture/references/domain/analytical-operations-reliability.md`, and
   `architecture/operations/reliability-and-recovery.md` to the implemented single-owner
   lifecycle and resource rules; no obsolete rule may remain authoritative.
   A cancellation or retirement correctness claim may not depend on sleeping
   for a fixed time, polling for eventual cleanup, or poisoning the whole pod.

## Expected Write Set and Consumer Closure

The paths below are the implementation map, including deletion. Resolve every
named caller and test when deleting its owner; do not leave a dormant parallel
path behind. The file names refer to `crates/vala/vala-bifrost-redux/src/`
unless a longer prefix is shown.

| Owner/file | Required work and removal |
| --- | --- |
| `oracle/planner.rs`, `catalog/bifrost_catalog.rs`, pinned `iceberg-catalog-sql` dependency, workspace `Cargo.toml` and `Cargo.lock` | Resolve registration before protection; add one catalog-owner pointer-only recheck after the complete reader guard through its existing authoritative primary connection, keep the one whole-attempt promotion retry and immutable metadata/permit-gated IO. Do not grant `wyrd_app` access to `iceberg_catalog` or read through a replica. Remove the second full `load_table`, redundant catalog existence queries where the owning catalog can combine them, and repeated Wyrd SQL-reference parses. Verify the pinned dependency's metadata-location immutability before relying on pointer equality. The pinned dependency method is a narrow addition to its existing catalog owner, not a second pool or catalog implementation. |
| `resources.rs` | Done: the volume governor and every disk-byte class are deleted. Keep Scribe WAL health separate from the process memory governor's health and local to the Scribe role. Remove upfront query scratch charge, `OracleQueryScratchReservation`, `try_split_scratch`, and related nested counters/metrics/tests. Keep the existing process memory root. Retain `OracleResourceRequest::for_class`'s 256/512-MiB class quantum as a **limit only**, and expose `min(class quantum, plan.scratch_limit_bytes)` to Oracle's `DiskManager` without debiting a root counter. Make `OracleQueryResources` the one slot owner retained by the query memory view, with no reverse pool reference; preserve the governor's Interactive borrowing/Analytical cap and capacity-change wake for both leader and follower releases. |
| `scribe/wal.rs` and `scribe/shards.rs` | Keep segment append, fsync, durable replay, batch-fence behavior, and the existing rollback-or-fail-closed response to ambiguous WAL mutation; signal unsafe WAL through Scribe WAL health only; map ENOSPC to `WalDiskFull` (`507`). In `append_and_sync_batch_commits`, append all COMMITs, submit one distinct-segment sync, then run serial PostgreSQL fences; preserve post-commit ambiguity ownership. Keep replay-reader pins while replacing generation-retirement refs. Done: `WalDiskState`, its breaker, thresholds, and walk are deleted. Per-shard WAL state is sized to the u8 shard id so any recorded shard replays after the count changes. |
| `scribe/admission.rs` and `scribe/mod.rs` | Remove the WAL breaker, `memory_breaker_bytes`, duplicate active/immutable byte accounting, and `wal.disk_pressure()` trigger. Admit through the existing Scribe root lease; keep item bound and per-table fairness. Drive only WAL-size, memtable-size, age-tick, and root-memory rotation per §3; do not invent a disk threshold. Size shard tasks and mailboxes from `ScribeGeometry::shard_count`. |
| `crates/wyrd/wyrd-server/src/boot/mod.rs`, `state.rs`, `app/server.rs`, `components/health/mod.rs`, `crates/vala/vala-bifrost-redux/src/gate/mod.rs` | Reuse Scribe's existing readiness and cluster fence: a WAL-health failure closes Scribe's local ready/heartbeat advertisement, deactivates only its registered role, and makes Gate refuse new writes without ACK. Keep the process-wide resource-health watcher for actual shared-memory/runtime failure; do not route Scribe WAL poison through it. On Scribe replay failure at boot, retain its files and unready role while continuing independent selected roles; do not activate or briefly advertise the failed role. In the existing readiness snapshot, permit a combined Scribe/Oracle target to report ready when only Scribe is down and every other required check passes; keep Scribe-only unready and expose the Scribe failure in the body. Gate query admission and Oracle/Forge remain available. |
| `scribe/staging_runtime.rs` and `scribe/member_stager.rs` | Remove recovery's call to `readmit_staged_bytes` and the helper after startup scan owns that charge. Retain staged-file digest, validation, registration, and reader leases. |
| `scribe/memtable.rs`, `scribe/shards.rs`, `scribe/execution_lanes.rs`, `scribe/persistence.rs`, `contracts.rs`, `gate/error.rs` | Reuse `freeze_all_nonempty()` to rotate the selected whole shard while staging each key separately. Make `RetainedGeneration` keep WAL paths and pending retire intent through submit and unlink acknowledgement; `RetireWal` returns success/failure to that owner and retries failed intent on tick/recovery. Classify transient versus deterministic invalid stage outcomes, retain WAL for both, stop futile resubmission for the latter, and report unhealthy identity/reason. After the unhealthy transition, refuse new appends for the affected shard/key before WAL/ACK and project the existing public `WriterUnavailable` with table identity; keep already accepted successors in FIFO and live memory, and allow siblings to progress. Prove the exact stage/unlink crash window in-process against PostgreSQL. Only after this passes, delete selective seals, `rotation_cohorts`, `referenced_wal_paths`, generation-retirement refcounts, and the deferred-delete set. Keep replay-reader pins and staged reader leases until safe release. |
| `oracle/spill.rs` | Keep the existing DataFusion `DiskManagerBuilder` local directory and per-query `with_max_temp_directory_size` limit; map its full-disk/limit error to query failure and verify cleanup. Add no `TempFileFactory`, `SpillWriter`, Wyrd disk meter, or second spill path. |
| `oracle/admission.rs`, `oracle/analytical.rs`, `oracle/analytical_transport.rs`, `oracle/analytical_supervisor.rs`, `oracle/mod.rs`, `oracle/query_stream.rs`, `oracle/exec.rs`, `oracle/telemetry.rs`, `oracle/dispatcher.rs`, `crates/wyrd/wyrd-server/src/oracle/tail_discovery.rs` | Remove query scratch reservation/splitting and `state.spill_used` from grants and execution; retain the queue, per-tenant fairness, and leader deadline. In `oracle/analytical_supervisor.rs`, delete attempt `scratch` and `AnalyticalAttemptRelease::scratch_bytes` once no owner remains; pass the limit-only quantum to `build_query_runtime`. Delete `LocalPermit`, its duplicate class/slot counters, copied query pool, and private capacity wake; use the governor's slot charge and release event for both local and remote work. Attach leader tenant-count release to the final query-resource lifetime; let DataFusion pool views and graph children retain that owner without a cycle. Clear terminal `RuntimeEnv`/`TaskContext` cache holders. Make `SetPlan` the sole follower graph activator; in the query-local `AnalyticalChannelResolver` share a per-`TaskKey` plan-ready result with `AnalyticalWorkerChannel::execute_task`, so an early task waits and a failed plan returns an error. Preserve `ReplayBody`/`AnalyticalConnectionLease` and the follower's leader-signed deadline settlement. Replace the peer cache's serialized `endpoint.connect().await` with tonic `connect_lazy()` and bound stage RPC awaits by that deadline. Apply that existing deadline around `PhysicalPlanFollower::execute` before stream creation. Remove `GRAPH_DRAIN_INTERVAL`, `GRAPH_DRAIN_POLLS`, the detached five-second release poll, and process poisoning caused solely by ordinary late child release. Reuse existing telemetry and add only the one local retained-count gauge needed to expose terminal-held capacity. Map only the full-queue branch to the new error. |
| `oracle/analytical.rs` settlement owner | Use the existing graph map as the only inventory for Active, DrainingPending, DrainingRunning, and retained failure states. The last coordinator lease drop, signed deadline, and shutdown all record pending settlement under that map's lock and signal the one driver with `Notify`; the driver takes each pending graph once and joins it. Delete `GraphSettlementCommand`, `SettlementQueueRefusal`, the bounded command channel, `offer_settlement`, its capacity calculation, and `settlement_capacity` test. Retain the existing graph failure inspection and signed deadline, with no heartbeat or second cleanup owner. |
| `oracle/dispatcher.rs`, `oracle/live.rs`, `oracle/query_stream.rs` | Preserve the existing `ExecuteFragment` response stream as the lifetime of both Interactive Oracle worker fragments and Scribe live fragments, including Scribe leaves inside an Analytical query. The leader query stream owns and drops each child transport stream on cancellation/disconnect; Oracle's `WorkerExecution` retains `RunningReservation` and worker resources until its response stream drops. Preserve the signed leader deadline while opening and polling, and prove remote lease release through the real client-to-server path. Add no class-specific cancellation path. |
| `catalog/iceberg_storage.rs`, `oracle/reader_pins.rs` | Keep `ReaderIoPermit` as the one query/epoch IO authority. Expose its existing cancellation/deadline inputs to the gated adapter and race awaited inner `exists`, `metadata`, `read`, `reader`, `list`, and range-read calls against them, so Iceberg's detached manifest planner cannot keep a cancelled BifrostStorage request permit until its retry timeout. Keep before/after permit checks and read-only adapter behavior. Do not add a new token, cache, planner, or storage timeout. |
| `crates/wyrd/wyrd-server/src/oracle/peer_service.rs` | Keep both Oracle and Scribe `WorkerExecution` streams inside the one returned `ExecuteFragment` gRPC response stream; transport drop must drop the running/follower lease and its batch stream. Enforce the verified leader-signed execution deadline around Scribe fragment setup and every live batch poll, matching Oracle worker enforcement. Drop its follower stream and lease on expiry and emit terminal failure. One peer service covers local and remote Scribe. |
| `crates/wyrd-spec/src/vala/error.rs`, `crates/shared/wyrd-client/src/error.rs`, `crates/wyrd/wyrd-server/src/query/routes.rs`, `crates/wyrd/wyrd-server/src/grpc/query.rs`, `crates/wyrd/wyrd-server/src/http/error.rs`, `sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts`, and existing Rust/Python/TypeScript query journeys | Add and project `WYRD_VALA_429_QUERY_QUEUE_FULL` as the one retryable 429 for a full 1,000-place Oracle queue. Generic SDK error carriers remain unchanged if they already preserve stable code; update only consumers that enumerate it. Keep timeout, shutdown, unavailable class, and resource errors distinguishable. Regenerate, never hand-edit, derived schemas/error-code surfaces. |
| `crates/wyrd/wyrd-server/src/config.rs`, `oracle/mod.rs` | Done: `scribe.wal_disk_limit_bytes`, the staging-target override, and the active-generation budget are deleted; `WYRD_MEM_TABLE_BUCKET_NUM`, `WYRD_MAX_FILE_SIZE_ON_DISK`, `WYRD_MAX_FILE_SIZE_IN_MEMORY`, and `WYRD_MAX_FILE_RETENTION_TIME` configure `ScribeGeometry`, with the staging target derived as `min(on-disk size, Forge target)`. Keep existing `bifrost.resources.scratch_limit_bytes` and its environment parsing as the ceiling used in `min(class quantum, resolved scratch_limit_bytes)` for DataFusion. Remove `analytical_scratch_bytes` once its precharge owner is gone. Do not add another configuration knob. |
| `crates/wyrd/wyrd-testing/src/load/capacity/run.rs` and `crates/wyrd/wyrd-testing/src/bifrost/process_cluster.rs` | Reuse the current child and cgroup harness; include `ProcessNode::pid()` and cgroup path in report metadata so Linux `perf` can attach to the server. Add only missing report evidence (stage jobs/file sizes, WAL residence, actual spill, wrong/refused results); do not build a second benchmark driver or profiler. Keep scored workloads within the existing standard/heavy commands. |
| `architecture/bifrost-design.md`, `architecture/references/domain/datafusion.md`, `architecture/references/domain/olap-serving.md`, `architecture/references/domain/analytical-operations-reliability.md`, `architecture/operations/reliability-and-recovery.md` | Replace obsolete selective-seal, WAL-breaker, upfront scratch, and cancellation-poll rules with the implemented ownership and standard DataFusion spill rules. Record measured small-file and spill/write interference instead of promising unproven isolation. |

Scribe and Oracle journeys remain in their existing test files/lanes. Query
request and terminal shapes and the storage schema remain unchanged; the
queue-full public error code is added as specified above.

## Verification and Evidence

1. Implement Scenarios 1–2 in RED/GREEN/REFACTOR order. Run their exact
   focused tests and Oracle/Scribe journeys. **Then run the mid-task standard
   benchmark checkpoint** described above, before changing the governor,
   WAL retirement, scratch, cancellation, or queue code. Present its
   selective-read and write-sync findings; honor §5's material stop rule.
2. Implement Scenarios 3–8 in order with RED/GREEN/REFACTOR. Run each planned
   focused Rust test with the exact command in its scenario after confirming
   its selector with `mise exec -- cargo nextest list --locked -p
   vala-bifrost-redux --lib` or the corresponding `wyrd-server --lib`
   listing. Use `mise run test:bifrost:journey:scribe` and
   `mise run test:bifrost:journey:oracle` for the required real-server flows;
   do not substitute unit tests. Tests needing Postgres use the
   repository-managed wrapper/lane. These focused checks may run during work.
3. **After all changed scenarios are implemented**, run the final
   `mise run bench:bifrost:query-capacity` first. Then run
   `WYRD_BENCH_HEAVY_SCAN=1 mise run bench:bifrost:query-capacity` as the
   separate 100M-row qualification. Run one at a time on this Linux host:
   Wyrd children are local `process_cluster` processes in the existing
   systemd 4-CPU/8-GiB cgroup; Docker is only for repository-managed
   PostgreSQL. The public client and PostgreSQL stay outside the child
   limit. Save complete reports, raw samples, logs, and sampled CPU stacks.
4. Present the valid benchmark results, capacity misses, root-cause profile,
   and deletion ledger to the user. Revise and rerun affected benchmark
   windows until the user is satisfied with the bottleneck remediation. No
   broad gate runs before this review.
5. Then run `mise run fmt`, `mise run lints`, `mise run codegen:check`, and
   `mise run gate` one at a
   time. `gate` already runs Bifrost verification; do not duplicate
   `verify:bifrost`. Resolve failures, append an AC-by-AC evidence table,
   and close TASK-003 only when AC-010's numeric targets pass.

## Material Stop Conditions

- A proposed simplification would ACK before durable WAL/batch fencing,
  delete the last readable source, weaken tenant or snapshot protection, or
  release capacity while child work still holds it: stop that change and
  preserve the guarantee.
- A query spill causes the process to die or loses ACKed WAL: diagnose that
  concrete failure before revising the standard DataFusion spill path. Do not
  add a custom writer or return to speculative upfront reservations.
- Profiling proves a required target needs a new persisted index, a different
  consistency contract, or another expensive-to-reverse change: retain the
  failed benchmark and request a specific decision. Do not weaken the target,
  write a benchmark-only bypass, or declare a correctness gate sufficient.
- The early read-path proof cannot establish immutable metadata locations or
  cannot obtain an authoritative pointer through the existing catalog-owner
  credential, or the checkpoint shows only a new multi-node published-cut
  protocol can reach the selective target: stop before the WAL/governor/Oracle
  ownership rewrite and present the measured options. Do not query the Iceberg
  pointer with `wyrd_app`, add a TTL cache, or change reader-GC protection
  under implementation authority.

## Authority Links

- [Approved spec revision 11](../spec.md): REQ-001, REQ-003,
  REQ-005, REQ-008–009, INV-001–006, AC-004, AC-009–011.
- [Repository standards](../../../../AGENTS.md),
  [Bifrost design](../../../../architecture/bifrost-design.md),
  [testing map](../../../../TESTING.md).
- [Superseded TASK-002](TASK-002-restore-and-measure-read-capacity.md) and
  [TASK-001 review evidence](../review/task-001-codex-rereview-20260927/TASK-001-R2-bound-staged-cancellation-and-close-source-rules.md).

## Implementation Evidence — one Oracle query capacity path (2026-09-29)

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Reader guard keeps snapshot protection, imposes no capacity gate | `oracle/reader_pins.rs`: semaphore, `release_permits`, `release_capacity`, `try_acquire_owned` removed; `oracle/mod.rs` slot+queue sizing removed | `--test integration -E 'test(/oracle::/) \| test(/forge::/)'` 48/48 (Postgres) | PASS |
| Shutdown tracked without a limit; token taken under the lifecycle lock; fail-closed join | `TaskTracker` token in `protect`; `select_loss` closes tracker; `join_descendants` waits under the join deadline | `oracle::reader_authority::*` integration | PASS |
| Guard release infallible while running | `mpsc::unbounded_channel` narrowing; token rides in `NarrowingCommand`; failed send logged as authority failure | reader_authority, forge `expired_cleanup`, `reader_expiry_ordering` | PASS |
| Admission has no duplicate capacity accounting; governor grants slots; final release returns slot + tenant charge and wakes once | `oracle/admission.rs`: `LocalPermit`/class slot counts removed; `TenantCharge` attached to `OracleQueryResources`; single `wake_on_capacity` (epoch captured before spawn) | `--lib -E 'test(/oracle::/) \| test(/resources::/)'` 253/253; `local_admission_is_fair_and_work_conserving`, `tenant_charge_returns_with_its_resources`, `follower_release_wakes_waiting_leader_without_reordering` | PASS |
| 1,000-place tenant-fair queue, one-hour queue limit, total deadline preserved | unchanged `AdmissionState`, `wait_for_grant` | `queued_queries_share_one_thousand_places`, `queued_queries_obey_queue_and_total_deadlines` | PASS |
| Queue overflow is `WYRD_VALA_429_QUERY_QUEUE_FULL`, only from `QueueFull`; other refusals distinct | `wyrd-spec` `BifrostError::QueryQueueFull`; `enqueue_waiter` QueueFull branch; Gate reason `oracle_queue_full` | `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --all-features --lib -E 'test(=oracle::admission::tests::queue_full_is_distinct_from_timeout)'` | PASS |
| Projection through HTTP (Retry-After), gRPC, Rust client, TS, OpenAPI docs | `http/error.rs`, `grpc/query.rs` tests, `wyrd-client/src/error.rs`, generated `error-codes.ts`, `query/routes.rs` | wyrd-server/wyrd-client/wyrd-spec error + route tests 117/117; `mise run codegen:check` | PASS |
| User journey: full queue refuses the next query immediately with the queue-full code | `wyrd-testing/tests/bifrost/oracle/capacity.rs` overflow asserts `QUERY_QUEUE_FULL_CODE`; load harness maps the code | `mise run test:bifrost:journey:oracle` 36/36 | PASS |
| Fenced reader cannot expose a cut served from the node-wide manifest cache | `catalog/bifrost_catalog.rs::materialize_reader_cut` checks `permit.expose_result()` | `reader_authority::leader_and_followers_protect_before_io_and_join_before_release` | PASS |

Checks: `mise run fmt`, `mise run lints` (clean), `git diff --check` (clean),
`vala-bifrost-redux --lib` 910/910 with Postgres. Python projects error codes
generically (no per-code surface to change). Non-goals untouched; write set
limited to the files above plus the approved gauge-test edits in `oracle/mod.rs`.
