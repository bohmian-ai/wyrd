# Apache DataFusion

Load for logical or physical planning, providers, pruning, interactive and
distributed execution, managed compaction, memory, spill, or diagnostics.

## DataFusion is execution, not authority

DataFusion planning receives an authenticated, tenant-qualified immutable cut
before path-specific admission. It uses the exact `OracleSessionShape` derived
from Oracle's guaranteed minimum successful grant and performs no row IO or
query-memory retention. After the returned root selects its class, execution
receives the admitted query-owned runtime and memory pool through `TaskContext`
while retaining the planning `SessionConfig` unchanged; extra granted capacity
may remain unused. Wyrd owns authorization, query-class routing, resource
grants, deadlines, failure policy, audit, and terminal semantics. A
`SessionContext`, SQL parser, optimizer, or `TableProvider` is never an
authorization boundary.

A provider owns schema, exact snapshot-bound file facts, statistics, scan
construction, and truthful pushdown claims. Advertise `Exact` filtering only
when the scan enforces the expression for every row; manifest, file, or
row-group pruning alone is normally `Inexact`. Retain the plan-root tenant
predicate and terminal `TenantTripwireExec`. Project only requested columns and
map fields by name or stable field identity.

## Oracle execution paths

Run every query through the pinned `datafusion-distributed` planner once. A
normal DataFusion physical root selects Interactive; a
`datafusion_distributed::DistributedExec` root selects Analytical. Retain and
execute that exact returned root after path-specific admission. Do not add a
candidate classifier, operator allowlist, second physical build, or fallback
planner. Representative end-to-end stage-graph queries prove the integrated
planner, codec, worker, and result path without promising exhaustive operator
coverage.

Distributed stages use one partitioned streamed exchange and no materialized
shuffle service. Bind tenant, pinned-snapshot digest, fragment digest, and fence
before decoding the physical plan or performing IO. Install the admitted
query-owned `RuntimeEnv` and dynamic `MemoryPool` on leader and workers; never
fall back to a worker's process-global runtime for Wyrd query work.

Operators and streamed exchanges use the same finite query-owned memory pool;
do not create a predicted exchange child or separate operator/exchange
sublimits. Before dispatch, enforce the configured selected-worker limit,
admitted tasks/partitions, Wyrd-owned admission queue and slots, and scratch
demand with checked count/range arithmetic. Dependency-owned exchange queues
retain their pinned byte backpressure without a Wyrd item-count guarantee. Do
not claim these controls predict every dependency allocation or transient
encoded-message byte. Followers use the same query-owned scratch
allocation for spill. One immutable deadline covers the entire stage tree;
head cancellation cancels and joins every descendant. A selected analytical
query owns exactly one execution attempt: peer, transport, protocol, auth,
tenant, resource, corruption, cancellation, deadline, and execution failures
after selection are terminal and are never retried or rerun interactively.
DataFusion and the transport do not decide that policy.

Disable or constrain execution features such as file-stream work stealing when
they violate stage partition ownership or attempt identity. Validate every
configuration key and default against Wyrd's pinned DataFusion version rather
than copying examples from a newer upstream release.

## Forge managed compaction

Forge uses DataFusion through the managed compaction core for manifest-backed
selection, delete application, optional sort execution, partition fan-out, and
rolling Parquet production. The core first produces real `CompactionPlan`
values. Forge estimates each plan's peak heap use, then admits plans through a
strict pod-local FIFO constrained by aggregate estimated memory and
parallelism. Waiting plans do not count against running memory and cannot
bypass a blocked head. A plan that fits the worker totals waits for running
capacity; a plan larger than the worker's total estimated-memory or parallelism
budget is refused.

The estimate is scheduler accounting, not a hard allocation limit. Forge uses
DataFusion's default unbounded memory pool, configures no disk spilling, and
provisions no local scratch storage. Estimator undershoot may OOM the worker;
durable task, lease, and fence recovery handles that process loss. Wyrd still
supplies cancellation, attempt output identity, immutable table policy, and a
non-semantic observer. It does not reimplement planning or physical rewrite.

The managed core may expose narrow seams for runtime injection, structured
cancellation/drain, output identity, selection evidence, and observations. It
must not own tenant SQL, leases, audit, final Iceberg commit, commit retry, or
uncertain-outcome reconciliation.

## Memory, spill, and concurrency

- Each Oracle query owns its memory-pool and spill lifetime. The process pool
  is an aggregate capacity root, not an operation-local grant.
- Oracle accounts scan, repartition, hash, and exchange buffers before
  admission and keeps spill tenant- and operation-bound.
- Forge accounts the real plan's scan and prefetch buffers, decoded Arrow
  batches, sort workspace, writer buffers, delete joins, and fixed headroom in
  its scheduler estimate. The pod-local FIFO tracks only running estimates.
- Bound target partitions, file-read concurrency, exchange fan-out, and open
  writers from measured resources. More partitions can increase retained
  buffers and memory.
- Stream `RecordBatch` output. A user-sized `collect()` is forbidden.

Accurate statistics drive pruning, join choice, and repartitioning. Prefer
Parquet footer/provider statistics over the pinned post-pruning file set. File
size and sort/time layout are not substitutes for row count, cardinality, null
count, or value distribution.

## Metrics and dependency boundary

Collect DataFusion plan and operator metrics for rows, batches, elapsed work,
Oracle spills, and partition behavior. Wyrd separately owns admission waits,
queue age, snapshot acquisition, exchange reservation, terminal peer failure,
WAL/catalog age, object-store errors, and successful-terminal accounting. Never
infer resource or durability success from a metric descriptor alone.

`datafusion-distributed` is a pinned `datafusion-contrib` dependency, not part
of Apache DataFusion core. Wyrd must qualify its planner/codec compatibility,
stream backpressure, worker lifecycle, authentication seam, cancellation,
buffer bounds, and failure behavior against the exact DataFusion/Arrow version
cone. Cargo resolving two compatible-looking versions does not make their
native plans, arrays, sessions, or protobuf codecs interchangeable.

The analytical dependency cone is DataFusion 55.0.0, Arrow/Parquet 59.2.0,
and `datafusion-distributed` 4.0.0 at its pinned revision. Managed Iceberg and
the managed compaction core must resolve that same native universe. Change the
cone as one verified dependency decision; never bridge duplicate universes
with JSON, IPC, FFI, trait erasure, or a sidecar process.

## Rejected shapes

Reject privileged global contexts, arbitrary SQL as the only control,
post-collection tenant filtering, positional mappings, false pushdown claims,
unbounded repartition or spill, detached writer/stage tasks, implicit runtime
replacement, retry on a changed snapshot, and custom execution where
DataFusion's owned abstractions already satisfy the required boundary.

## Stable Wyrd anchors

- Bifrost query and Forge authority: `architecture/bifrost-design.md`.
- Oracle and Forge execution: `crates/vala/vala-bifrost-redux/src/oracle/` and
  `crates/vala/vala-bifrost-redux/src/forge/`.
- Public query types: `crates/wyrd-spec/src/vala/api.rs`.

## Primary grounding

- [DataFusion features](https://datafusion.apache.org/user-guide/features.html)
- [DataFusion configuration](https://datafusion.apache.org/user-guide/configs.html)
- [DataFusion configuration and memory-limited queries](https://datafusion.apache.org/user-guide/configs.html#memory-limited-queries)
- [DataFusion operator metrics](https://datafusion.apache.org/user-guide/metrics.html)
- [DataFusion 55 `TableProvider`](https://docs.rs/datafusion/55.0.0/datafusion/catalog/trait.TableProvider.html)
- [datafusion-distributed](https://github.com/datafusion-contrib/datafusion-distributed)
redacted
redacted
redacted
