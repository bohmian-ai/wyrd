# Bifrost Design

**Version:** v1

Bifrost is Wyrd's OLAP warehouse: the public analytical surface and the storage
substrate `vala` uses to record Wyrd's own observations. This document is the
design authority for Bifrost internals — the table model, Scribe ingest, Oracle
query admission, Forge compaction, and the public query/ingest surface.

`architecture/wyrd-design.md` remains authority for Wyrd doctrine, the Card
catalog, and cross-kind contracts, including the rule that Bifrost is a storage
shape and never a Card kind. Where the two disagree on a doctrinal or Card
question, `wyrd-design.md` wins; where they disagree on Bifrost internals, this
file wins. Like `wyrd-design.md`, this document is **stateless** — it reflects
the shape as it stands now, and decision history lives in
`git log architecture/bifrost-design.md`.

---

## Table of contents

- [Table model](#table-model) — managed envelope, row identity, physical binding, substrate
- [Ingest — Scribe](#ingest--scribe) — shard lanes, execution runtime, pressure sealing, event time
- [Query — Oracle](#query--oracle) — read audit durability, slot admission, memory grants
- [Maintenance — Forge](#maintenance--forge) — compaction concurrency bounds
- [Public surface](#public-surface) — HTTP/gRPC/Python contracts, typed query service, payload permissions
- [Serving and platform invariants](#serving-and-platform-invariants) — single serving surface, audit sentinel

---

## Table model

**Everything is a Bifrost table.** One table shape underlies every internal
analytical table. Every physical schema has a server-owned managed envelope
with these required non-null columns:

- `wyrd_event_time`: the validated or server-derived event timestamp;
- `wyrd_ingested_at`: the server-stamped ingestion timestamp;
- `wyrd_batch_id`: the immutable 16-byte identity of one accepted logical
  batch;
- `wyrd_row_ordinal`: the zero-based position of a row in that complete
  logical batch, stored as an Iceberg `int` / Arrow `Int32`;
- `wyrd_request_id`: the server-minted or validated request-correlation ID;
- `data_tenant_id`: the authenticated tenant-isolation key.

Physical schemas also reserve nullable `run_id`, `card_uid`, and
`principal_id` correlation columns. Their values may be absent according to
the table's correlation policy and do not participate in physical row
identity.

Within one organization-qualified physical table, the immutable row identity
is:

```text
(wyrd_batch_id, wyrd_row_ordinal)
```

The globally qualified row identity is:

```text
(organization_id, logical_table, wyrd_batch_id, wyrd_row_ordinal)
```

`wyrd_row_ordinal` is contiguous across the logical request order and never
resets at an Arrow `RecordBatch`, WAL segment, Parquet file, or Forge rewrite
boundary. The accepted batch-row limit is below `i32::MAX`; negative or
non-contiguous ordinals are invalid. The client SDK generates one UUIDv7 batch
identity and preserves it unchanged across retries. Gate validates that
envelope value and assigns ordinals before Scribe admission; payload columns
cannot supply or override either identity field or other server-owned request,
ingestion, or tenant columns. Scribe, WAL, Parquet, Iceberg, and Forge preserve
the pair unchanged. Within the configured Scribe idempotency-retention window,
a replay may reuse an accepted batch identity only when schema fingerprint,
row count, row order, and payload digest match; otherwise it fails with the
stable Bifrost batch-identity conflict. Outside that window, callers must never
reuse a batch ID for a different logical batch. Oracle uses the pair to
reconcile live and sealed sources, with the fixed Iceberg snapshot winning when
both sources contain the same identity.

The physical table identity is always the authenticated organization plus the
logical table:

```text
(organization_id, logical_table) → one physical Iceberg table
```

The logical `TableRef` remains tenant-free and carries the table namespace and
local name. The Bifrost catalog derives the organization-qualified Iceberg
namespace, object-store prefix, and physical table from the authenticated
organization and `TableRef`. Every physical Parquet file carries the
server-stamped `data_tenant_id`. Gate and Scribe reject a binding whose tenant
does not match the authenticated organization; Oracle adds the plan-root
`TenantTripwireExec` and fails closed with `WYRD_VALA_500_TENANT_TRIPWIRE` on a
row mismatch. There is no shared physical table layout and no deployment-
specific storage mode.

Logical definition ownership is separate from physical identity. Wyrd ships
server-owned built-in definitions for tables such as spans, GenAI, eval, drift,
and audit, while users register dataset definitions. “Built-in” and “system”
describe who owns the immutable logical definition; they are not table scopes.
Every instantiated built-in or user-defined table uses the same
organization-qualified physical binding above.

The substrate is Apache Iceberg-managed Parquet in object storage, with Postgres
as the Iceberg catalog and control plane and DataFusion as the query engine —
consistent with Doctrine #4 (Postgres is control-plane only; analytical data
lives in object store). Runtime ownership stays in `vala`: the `vala-bifrost`
engine crate owns the Iceberg/DataFusion warehouse engine, `vala-ingest` owns
gRPC ingest _(under revision — serving ownership moving to wyrd-server, reconciled in a follow-up design pass)_, and `wyrd-spec::vala::api` owns the
public wire contracts. HTTP serving for these routes now belongs to `wyrd-server`:
the eval consolidation dissolved the former `vala-http` crate, per the principle
below. Python-visible Bifrost behavior lives in `vala-sdk` (the
approved Vala Python owner crate) behind its optional `python` feature.

## Ingest — Scribe

**Scribe write-path horizontal-scale contract.** The Scribe ingest engine inside
one pod partitions writes across sixteen fixed shard lanes. Each lane owns an
independent memtable, WAL segment directory, and `synced_not_inserted` dedup
index. This topology is stable: it is pod-local, requires no coordination
between pods, and has no migration cost.

The shard routing key is `(tenant_id, table, batch_id)`. Including `batch_id`
means distinct client batches for the same (tenant, table) spread across lanes
— removing the per-table serialization bottleneck — while a client retry
(same `batch_id`) always lands on the same lane that already holds its dedup
state. Exactly-once delivery therefore requires no cross-shard coordination:
the WAL record, `synced_not_inserted` set, and SQL unique constraint are all
shard-local.

Correctness is gateway-independent (D81): the fenced-publication
`AppendSliceId` uniqueness constraint is the sole authoritative cross-pod
exactly-once mechanism, and the Scribe storage identities
(`(node_id, writer_epoch, shard_id, seg_seq)` WAL segments and the
`vala.file_list` replay key) prevent cross-pod collisions. Gateway
batch-identity affinity is an optional efficiency optimization — it reduces
duplicate buffering and WAL work when a retry reaches the same pod — and must
not be treated as a correctness requirement. The tail-merge `AppendSliceId`
dedup named in D81 is the tracked prerequisite for any future multi-pod
ingest topology; until it ships, multi-pod ingest is out of scope.

**Scribe execution-lane topology.** The sixteen shard lanes, plus the Scribe
reconciliation and persistence loops, run on a dedicated Tokio runtime that is
separate from the request runtime, so a saturated API path cannot starve shard
progress and a shard lane blocked in Arrow memtable insertion or a Postgres
`COMMIT` cannot stall request serving. Its worker count derives from detected
parallelism, capped at the shard-lane count — a host cannot usefully run more
coordination threads than there are lanes to own — and floored at two so a
single-core deployment still makes progress while one lane blocks.

That runtime has exactly one owner. It is never reachable from `AppState`, the
composed Bifrost graph, or any other cloneable type: every consumer holds a
`Handle`. Dropping a Tokio runtime blocks to join its workers, which panics on
an async frame, and `wyrd-server` runs entirely under `#[tokio::main]` — a
runtime reachable from per-request-cloned state would therefore be torn down by
whichever clone happened to die last, on a stack where that teardown is illegal.
The single owner instead releases the executor without blocking, and is dropped
only after the bounded Bifrost drain reports that the Scribe role completed:
because a non-blocking release does not wait, that ordering — not the release
mechanism — is what guarantees no shard lane is abandoned while it still holds
WAL segments or post-`COMMIT` memtable state.

Control operations (seal-key freeze, live-tail snapshot, pressure flush, WAL
pressure flush) that were previously addressed to the one shard computed by
`shard_for(tenant, table)` are now broadcast to all sixteen shards. Shards that
hold no bucket for a given key no-op and return `None`; the caller collects the
merged union across every lane. Because `batch_id` spreads one seal-key's
buckets over multiple lanes, `freeze_key` collects **every** shard's frozen
memtable (not the first), pre-commits each within the caller's tenant
transaction, and rolls back any already-committed generations if a later one
fails — so no frozen Arrow data is orphaned. An empty aggregate (no lane held
the key) is a fail-closed `Internal` error, not a silent success. No new
cross-shard state is introduced.

**Scribe ingress pressure sealing (D83).** Admission is gated by ingress
*occupancy*, not effective child pressure: occupancy is
`scribe_total_bytes / ingress_limit_bytes`, where the ingress ceiling excludes
the persistence headroom (D75). `ScribePressureConfig` defines one 600-second
shard-generation age and a hysteresis band with a 75% high-water mark and 50%
low-water target (`low < high` is enforced at construction). Before appending
an incoming whole unit, automatic rotation ORs projected non-empty WAL
compressed bytes, WAL uncompressed bytes, aggregate shard JSON-equivalent
bytes, aggregate shard Arrow bytes, and shard-generation age. Any automatic
size or age trigger closes the shard WAL and atomically freezes every non-empty
`SealKey` into one cohort before the incoming unit enters a fresh generation;
age rotation does this even below high water. Separately, at or above the
high-water mark pressure sealing may selectively freeze younger largest
writable buckets across all sixteen lanes toward the low-water target without
closing the shard WAL or resetting shard-generation age. Forced and
tenant-scoped sealing are also selective paths and never masquerade as the
automatic whole-shard rotation contract.
Sealing is flush-first and drain-before-reject — a
memory-ceiling rejection returns `IngestBusy` only after a pressure seal is
requested and a single reservation retry still fails, deferring to D71 client
backoff for eventual admission. Freezing only recategorizes bytes; persistence
encode-and-retire frees them asynchronously, so there is no synchronous
busy-wait. The cgroup 90% tripwire remains an immediate `IngestBusy`: a
container-level limit that a Scribe-scoped seal cannot relieve is never routed
through the seal-and-retry path.

Crash recovery reads the shard lane from the WAL segment header (`shard_id`
field) and dispatches each replayed state directly to `shard_senders[shard_id]`
rather than recomputing the routing key. The client `batch_id` is not available
during replay, so re-deriving the lane would be incorrect. WAL segment paths are
organized by shard lane so the header value is authoritative for replay dispatch.

The pod-global LSN counter (`next_lsn`) is shared across all shard lanes via an
`Arc<AtomicU64>`; no per-shard LSN counter is needed.

**Bifrost event-time acceptance window (D85, T42).** D82/T38 opened the native
Arrow IPC path to an optional caller-supplied `wyrd_event_time` column. D85
narrows that opening before the contract ships: caller-supplied `wyrd_event_time`
values on both the native (D82/T38) and projected OTLP surfaces are validated
against a bounded acceptance window (default: 30 days past, 24 hours future,
relative to server receipt time). Values outside the window are **rejected** with
the stable error code `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE` carrying the
offending value and both bound instants. Rejection is never a clamp or
normalization — silently rewriting a caller timestamp is data corruption and
incoherent beside idempotent ingest. When the column is absent the server stamps
receipt time and no window check runs. The window is a single server-level knob
(`ScribeRuntimeConfig::event_time_past_window_secs` /
`event_time_future_window_secs`) defaulting to the D85 values; per-tenant
overrides are a future Policy concern, not in scope here.

## Query — Oracle

### Read-decision audit durability

Oracle query-read admission has one narrow durability exception to the normal
transactional audit rule: the serving process first fsyncs a versioned,
CRC-framed local WAL record (including its acceptance timestamp), then a single
bounded relay calls the canonical tenant hash-chain outbox writer at least once.
Postgres mutations and every other durable transition remain transactionally
audited at their own commit boundary. A crash after the outbox commit and
before the local checkpoint may replay one valid duplicate, but never loses an
accepted read decision.

### Oracle admission: slots admit, grants size

Oracle separates two decisions that a single constant used to conflate. Keep the
distinction in mind throughout: the **charge** is what a query pays to start and
is what bounds concurrency; the **grant** is the ceiling it may grow into once
running and denies nothing to anyone else.

**Admission is decided by slot capacity.** A query charges
`ORACLE_PARTITION_WORKING_MEMORY_BYTES` (32 MiB) per slot unit against the shared
elastic budget — an Interactive query occupies one unit, an Analytical query two.
That charge is the working set one execution partition needs to make progress,
not the largest envelope the query might grow into.

**Sizing is a separate, derived per-query ceiling.**
`ORACLE_PARTITION_MEMORY_BYTES` (256 MiB) is the cap on that ceiling and is never
reserved. The grant is

```
grant = oracle_budget * query_slots / sum(running_slots)
```

clamped to `[32 MiB, 256 MiB]`, computed once at admission and held for the
query's life.

Two properties of that formula are load-bearing. The numerator is the *fixed
configured budget*, never currently free memory: dividing free memory would make
two identical queries receive different ceilings depending on what Scribe and
Forge happened to be doing at that instant. And the live denominator makes the
ceiling converge on the charge as the node saturates — at full slot occupancy
`budget / slots` is approximately the 32 MiB a unit paid at the door, while an
idle node opens the ceiling to the cap. That coupling is what lets admission be
decided by slots alone, with no overcommit ratio and no kill-on-OOM backstop.

This is a self-limiting design, **not a proof**. Because the grant is fixed at
admission and held, a query admitted onto an idle node keeps its generous
ceiling while later arrivals compute smaller ones, so the sum of *held* ceilings
can exceed the budget during a ramp. What bounds real consumption is the charge,
which is deducted, plus the fact that a ceiling is an upper bound most queries
never reach. Do not restate this as a construction guarantee.

**What other engines do, and where we sit.** Divide-by-concurrency is the
minority approach, and the design record should say so plainly. Most engines
give every query the same flat, statically configured cap and control load with
a separate concurrency mechanism; Trino and ClickHouse both cap per query and
never divide. Apache Doris is the one clean open-source precedent for dividing,
and it ships both a fixed
(`mem_limit * slots / max_concurrency`) and a dynamic
(`mem_limit * slots / sum(running_slots)`) mode — the latter is what Wyrd
implements. The proprietary warehouses agree on the shape: Vertica
(`queuingThreshold / PLANNEDCONCURRENCY`) and Redshift manual WLM
(`total_mem * queue_pct / slot_count`). Vertica also confirms the
reservation/limit split directly: `MEMORYSIZE` reserves, `MAXMEMORYSIZE` caps the
borrow. Trino removed its reserved memory pool in favour of concurrency-based
admission but needed a low-memory killer, because a flat cap does not bound
aggregate use.

Wyrd chose the dividing form because the fixed form is degenerate at our
numbers — an Oracle budget divided by the resolved slot count falls below the
32 MiB floor, so every query would clamp to the floor permanently and
`prefer_hash_join` would never re-enable — and because a flat cap at this slot
count would overcommit badly enough to require the killer we decline to build.

**Memory admission is a stability mechanism, not a latency mechanism.** Apache
Pinot reaches sub-second queries at petabyte scale with essentially no memory
management in its default configuration: a no-op thread accountant, heap
throttling off, query killing off, no per-query memory budget at admission
anywhere in its source, and no spill path at all. It controls load purely on
threads. That is the honest calibration for this whole subsystem — latency is won
in partition pruning, vectorization, and I/O, and this machinery exists so a node
degrades gracefully instead of failing under concurrency. Size its complexity
accordingly.

**Why this replaced the previous design.** One constant served as both the
reservation debited at admission and the argument to the DataFusion pool, which
made node concurrency `budget / ceiling`. Raising the ceiling so one query could
use more memory silently reduced how many queries the node would accept. On a
3 GiB container a single Analytical distributed query reserved 1.25 GiB to scan a
handful of 5 KiB Parquet files, so a node shed load at the lowest production rung
and callers saw failed queries rather than backpressure.

**Node concurrency** defaults to `oracle_budget / ORACLE_PARTITION_WORKING_MEMORY_BYTES`
— how many admission units the budget holds — and is overridable per deployment
via `WYRD_BIFROST_ORACLE_QUERY_SLOT_LIMIT`. The resolved value is logged once at
startup. Core count deliberately does not clamp it. An earlier draft did clamp it
to a multiple of effective CPU; measurement refuted that, and the measurement is
the authority here. On the four-core R0 box a `cpu * 4` clamp resolved to 16
units and still shed peer work, while the unclamped divisor resolved to 78 and
completed the same workload with zero reservation refusals and zero read
retries. A slot unit is an admission unit, not a thread, and intra-query
parallelism is already bounded by `target_partitions`.

**One counter serves both roles, pending evidence that it should not.** The
resolved value sizes leader admission and the peer slot manager alike, even
though a node is normally both at once — leading its own queries while serving
fragments for queries other nodes lead. Splitting it was considered and rejected
for now: R0's refusals appeared on the peer side, but that is equally predicted
by a single ceiling set too low, since a leader admits before it asks peers and
therefore wins the race whenever slots are scarce. The unclamped default clears
those refusals entirely, leaving no failure for a split to fix. The discriminating
experiment, if the question returns, is to hold total slots constant and split
them; adopt two counters only if that beats one. Note that Pinot's runner/worker
thread pools are *not* precedent for this — they divide coordination from
execution within a single node, and Pinot's actual query leader is a separate
broker process.

**The grant sizes the whole session, not just the pool.** One
`OracleSessionShape` derives `target_partitions`, `batch_size`, and
`prefer_hash_join` from the same grant, at one site, so a query cannot end up
with a partition count sized for one ceiling and a batch size sized for another.
DataFusion's own tuning guidance is to reduce `target_partitions` and
`batch_size` together for memory-limited queries.

`prefer_hash_join = false` on floor grants is **not** an optimization. Verified
against DataFusion 53.1, `HashJoinExec` cannot spill: it holds five `try_grow`
reservations and no spill calls, and returns resource exhaustion when its
reservation cannot grow. `SortMergeJoinExec` and the non-grouped aggregate are
likewise unspillable; `SortExec` and the grouped hash aggregate do spill and
complete. Shrinking a ceiling under concurrency is therefore only safe *because*
a small grant also routes the plan away from an operator that would hard-fail
rather than spill; without it, the dynamic grant would convert a clean refusal
into a failed query. For the same reason the grant is never recomputed
mid-flight — shrinking a pool underneath a running operator is exactly how a
non-spillable consumer hard-fails.

**Why the cap is absolute, not proportional.** `ORACLE_PARTITION_MEMORY_BYTES` is
a fixed 256 MiB on every node rather than a fraction of detected memory. Node
capacity already scales without it: `detect_snapshot` reads the cgroup v2/v1
memory limit and takes the smaller of that and `/proc/meminfo` `MemTotal`, and
the Oracle budget, the elastic pool, and the default slot count all derive from
the result. Only the per-query ceiling is constant, and it stays constant for
two reasons.

The first is the non-spillable operator set described directly above. A larger
grant re-enables `prefer_hash_join`, and `HashJoinExec` cannot spill, so on a
large node a proportional cap would trade a graceful spill for a hard
`ResourcesExhausted` on precisely the queries most likely to be large. Raising
the cap is only safe once DataFusion can spill a hash join
(`apache/datafusion#12952`, proposal `#17267`, both open); until then the cap is
what keeps the planner on operators that degrade instead of failing.

The second is that the cap doubles as the interactive/analytical routing
threshold — an aggregate becomes analytical when its estimated hash state
exceeds the prospective grant. A constant cap makes that boundary
node-shape-invariant, so a published SLA measured on the reference node
describes the same routing behavior on every other node.

The objection this answers is that a large pod under-uses its memory. It does
not: the default slot count on that pod is `oracle_budget / 32 MiB`, so the
memory is consumed by concurrency rather than by one fat query. Nor can the cap
overcommit a small pod — it is never reserved, and `try_acquire_oracle` checks
the admission charge against the elastic budget independently of the ceiling.

For calibration against the comparators above: Trino caps a query at 30% of node
heap, ClickHouse at an absolute 10 GB beneath a 90%-of-RAM server limit, and
Apache Doris raised `exec_mem_limit` from 2 GB to 100 GB in 3.1 — but Doris
spills every operator, which is the precondition Wyrd does not yet have. Wyrd's
256 MiB is small by every comparator, deliberately.

Reopen this when the DataFusion spill gap closes, not before, and reopen it with
a measurement rather than a node size. There is deliberately no override knob:
the failure mode of a too-large ceiling is a crashed query rather than a slow
one, so a deployment that needs a different ceiling needs a design decision, not
an environment variable.

**Deviation from DataFusion's shared-pool guidance.** DataFusion recommends one
shared pool across concurrent queries; Wyrd grants each query its own bounded
pool, keyed by query identity. The reason is mechanical, not stylistic. In `FairSpillPool`, fairness covers only spillable reservations, which
receive `(pool_size - unspillable) / num_spillable`; unspillable reservations are
served first-come-first-served off the top with no protection, and the spillable
count spans every registered consumer pool-wide regardless of which query owns
it. So in a shared pool one query's hash join — unspillable — shrinks the fair
share of every other query, and the resulting `ResourcesExhausted` surfaces on
whichever *other* query next tries to grow. The failure lands on the innocent
query and names the wrong consumer. Per-query pools scope the failure boundary to
the query that caused it. The cost is real and accepted: an idle query's headroom
cannot be lent to a busy one.

Note that the isolation at stake here is noisy-neighbor *performance* isolation,
not data isolation. A shared pool never leaks tenant data; it leaks latency.

**Per-tenant fairness is not part of sizing.** It is owned by `OracleAdmission`
(per-tenant FIFO plus weighted round-robin over the durable queue). The grant
formula is deliberately tenant-blind; duplicating fairness in the sizing rule
would put two independent mechanisms in charge of the same outcome. Most engines
isolate at tenant or workload-group granularity rather than per query — Trino
resource groups, Doris workload groups, Pinot's primary/secondary split — so
Wyrd's per-query pool sits at the strict-isolation end of that spectrum by
choice.

Scratch is not sized this way. It remains reserved at the grant cap per slot
unit, because scratch is genuinely consumed disk rather than a ceiling: a query
that spills must have reserved the space it spills into.

**Grants are not sized from observed history, and deliberately so.** Learned
per-shape memory sizing (SQL Server's memory grant feedback, Redshift Auto WLM)
was evaluated and rejected. Both precedents exist because in those systems a
grant is a true *reservation* — SQL Server queues a query on `RESOURCE_SEMAPHORE`
until its grant can be satisfied, so an oversized grant literally blocks other
queries from starting, which is what Microsoft means by "inhibits parallelism."
Wyrd's ceiling reserves nothing and blocks nobody, so over-granting is already
free and the causal chain that makes the feature valuable is severed. The
remaining failure mode is under-granting, and there the formula works against a
fix: it shrinks the ceiling precisely when the node is busy and has no spare
memory to give. No open-source OLAP engine surveyed does learned per-shape
sizing. Per-query peak usage is instead emitted as telemetry, so the question can
be reopened against measurements rather than analogy.

## Maintenance — Forge

**Forge compaction concurrency bounds.** Forge maintenance work is scheduled
through durable Postgres claims and bounded by exactly three rules, with no
cluster-wide serialization (D78: a per-table in-flight track plus per-compactor
parallelism budgets). First, the partial unique index `forge_tasks_publication_active`
admits at most one active compaction per (tenant, catalog, namespace, table).
Second, a per-tenant active cap bounds a single tenant's fan-out. Third, an
oversized single-file compaction (`large_singleton` lane) is governed by a
per-owner one-active-large rule: a worker may hold at most one active large task,
so large compactions on distinct tables run concurrently across workers instead
of serializing through a former cluster-wide singleton lease. A crashed owner's
large task is freed for reclaim by the same `claim_expires_at` lease expiry as
any other claim; there is no separate large-lane reservation row.

## Public surface

Bifrost is a stable Wyrd public surface across HTTP, gRPC,
Python, generated schemas, MCP/agent documentation, and stable error codes.
The public contract includes:

- HTTP table management under `/v1/bifrost/tables`, served by `wyrd-server`.
- HTTP query surfaces served by `wyrd-server` under the `/v1` nest:
  `POST /v1/query` returns the terminal-safe, length-delimited Oracle frame
  stream with media type
  `application/vnd.wyrd.bifrost-query-stream` and only
  `x-wyrd-schema-fingerprint` as initial query metadata. There is no
  asynchronous query-job route family, status polling contract, result
  redirect, or initial row-count header. Future asynchronous jobs require a
  new architecture decision rather than a compatibility surface. The typed
  observation query routes under `/v1` (see `ValaQueryService` below) are the
  companion projection surface. `wyrd-spec::vala::api` owns all retained
  query/freshness wire types.
- gRPC ingest through `wyrd.v1.BifrostIngestService` _(under revision — serving ownership moving to wyrd-server, reconciled in a follow-up design pass)_.
- The `wyrd.bifrost` Python SDK submodule.
- Generated `wyrd-spec::vala::api` wire types such as `BifrostTableEntry`,
  register-table types, and query request/response types. Physical table
  identity is server-derived from the authenticated organization and logical
  table; it is not a caller-selected scope.
- The `WYRD_VALA_*_BIFROST_*` error catalog crossing HTTP, MCP, Python, and
  generated documentation boundaries.

Bifrost permissions are resource-scoped through `BifrostTable`, `BifrostRecord`,
and `BifrostQuery`. Generic record writes must not write reserved or
system-managed Bifrost tables. There is no `wyrd.warehouse` submodule and no
`WarehouseCard`.

**`ValaQueryService` — typed observability query surface (accepted, Stage 4).** `wyrd-server`
exposes `wyrd.v1.ValaQueryService` (gRPC-first) with an axum HTTP projection as the
**query-only** typed surface for the observability domain namespaces. There is no
`vala-http` crate — `wyrd-server` is the only serving surface. The gRPC service and
its HTTP projection are backed by `wyrd-spec::vala::api` request/response contracts with
cursor pagination, mandatory time windows, and stable `WYRD_VALA_*` error codes.

The accepted domain namespaces and tables:

| Namespace | Tables | Notes |
|---|---|---|
| `vala.traces` | `spans`, `events`, `links` | Raw OTel spans — source of truth |
| `vala.metrics` | `points` | OTel metric data points with exemplars |
| `vala.logs` | `records` | OTel LogRecord signal |
| `vala.genai` | `messages`, `embeddings`, `tool_calls`, `memory` | Derived from `vala.traces` spans carrying `gen_ai.*` attributes |
| `vala.eval` | `runs`, `assertions` | Agent/LLM evaluation records |
| `vala.drift` | `*` | Traditional ML drift records |
| `vala.dev` | `agent_traces` | High-fidelity coding-harness traces; carries the code axis |
| `vala.system` | `audit_log` | Transactional audit log (relay-written) |

All domain tables use the organization-qualified physical-table rule above.
Typed query routes build bound DataFusion `LogicalPlan`s (never `ctx.sql`); a
query-admission gate requires the authenticated organization/table binding and
a bounded time window before execution. Oracle's provider applies the
tenant-tripwire boundary before execution.

**Elevated payload-read permissions.** Four payload-bearing table families are
`PayloadClass::Sensitive` and gate their sensitive columns on an elevated read permission
beyond the base `BifrostQuery:Read`:

| Permission resource | Gates |
|---|---|
| `BifrostTracePayload` | `vala.traces.*` `attributes` column on `GetTrace` / `QueryTraces` |
| `BifrostLogPayload` | `vala.logs.records` `body` / `attributes` on log-query methods |
| `BifrostGenAiPayload` | `vala.genai.*` message and tool I/O columns |
| `BifrostAgentTracePayload` | `vala.dev.agent_traces` message/tool payload columns |

These four permissions extend the existing `Permission`/`Resource` model in
`crates/shared/wyrd-runtime/src/permission.rs`. The generic-SQL analyzer enforces the
same payload gate so `SELECT vala.traces.spans.attributes` without
`BifrostTracePayload:Read` is denied through both the typed and the generic-SQL path.

## Serving and platform invariants

**Principle — wyrd-server is the only serving surface.** `vala-*` crates are
engine and data-plane libraries; they are never HTTP or gRPC serving crates.
The Redux Gate may implement approved tonic protocol adapters and bearer-token
verification through `wyrd-auth-verify`, but it does not serve the network.
Only `wyrd-server` binds sockets, owns listeners and top-level routing,
terminates TLS, performs boot/readiness, and controls server lifecycle.
`wyrd-server` is the single process that binds ports and owns all HTTP/gRPC
serving. The eval consolidation (commits 01–05) is the first realization of
this principle; Bifrost/ingest serving reconciliation follows in a separate
design pass. The eval pull-protocol session-run (`/v1/eval/runs/{run_id}`) is an
ephemeral server-side session entry for concurrency and ownership tracking; it
is distinct from the doctrinal `RunRef` — the Card→Run→Observation run is a
client-side execution record (see the "There is no run registry" note under
_Observation identity — `Card → Run → Observation`_), never server-persisted.

**Platform audit sentinel.** `DataTenantId::SYSTEM_OWNER` is the established
durable platform tenant for security events that cannot safely be attributed
to caller-controlled tenant data, including peer tickets rejected before
verified claim decoding. Platform migrations must provision this sentinel
idempotently. Callers must never create or select an audit tenant from
unverified payload bytes.
Its canonical row is UUID `00000000-0000-0000-0000-000000000000`, slug
`wyrd-system`, display name `Wyrd System`, status `active`, and
`deleted_at IS NULL`. Provisioning and boot verification fail closed rather
than overwriting or accepting conflicting UUID/slug ownership or incompatible
attributes.

