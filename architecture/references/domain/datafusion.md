# Apache DataFusion

Load for logical or physical planning, providers, pruning, interactive and
distributed execution, managed compaction, memory, spill, or diagnostics.

## DataFusion is execution, not authority

DataFusion receives an already authenticated, tenant-qualified, admitted
operation. Wyrd owns authorization, query-class routing, resource grants,
deadlines, retry, audit, and terminal semantics. A `SessionContext`, SQL parser,
optimizer, or `TableProvider` is never an authorization boundary.

A provider owns schema, exact snapshot-bound file facts, statistics, scan
construction, and truthful pushdown claims. Advertise `Exact` filtering only
when the scan enforces the expression for every row; manifest, file, or
row-group pruning alone is normally `Inexact`. Retain the plan-root tenant
predicate and terminal `TenantTripwireExec`. Project only requested columns and
map fields by name or stable field identity.

## Oracle execution paths

The interactive engine is the default. Keep work interactive when it needs no
network exchange or when cardinality/working-state estimates are missing or
invalid. Use the streamed distributed path for exchange-requiring joins,
high-cardinality aggregation, partitioned windows, subqueries, and
deduplicating set operations. The analytical candidate must contain a real
network exchange.

Distributed stages use one partitioned streamed exchange and no materialized
shuffle service. Bind tenant, pinned-snapshot digest, fragment digest, and fence
before decoding the physical plan or performing IO. Install the admitted
query-owned `RuntimeEnv` and dynamic `MemoryPool` on leader and workers; never
fall back to a worker's process-global runtime for Wyrd query work.

Reserve exchange buffers as a child of the query grant:

```text
fan_out * (worker_connection_buffer_budget_bytes + max_message_bytes)
```

Refuse before dispatch if the checked reservation does not fit or leaves too
little operator working state. Followers use the same query-owned scratch
allocation for spill. One immutable deadline covers the entire stage tree;
head cancellation cancels and joins every descendant. One authenticated
availability loss may re-execute deterministically against the same pinned cut
only before Oracle emits a result-data frame. Protocol, auth, tenant, resource,
corruption, cancellation, deadline, and post-egress failures never retry.
DataFusion and the transport do not decide that policy.

Disable or constrain execution features such as file-stream work stealing when
they violate stage partition ownership or retry identity. Validate every
configuration key and default against Wyrd's pinned DataFusion version rather
than copying examples from a newer upstream release.

## Forge managed compaction

Forge uses DataFusion through the managed compaction core for manifest-backed
selection, delete application, optional sort execution, partition fan-out, and
rolling Parquet production. Wyrd injects the admitted runtime, memory/spill
resources, cancellation token, attempt output identity, immutable table policy,
and non-semantic observer. It does not reimplement planning or physical rewrite.

The managed core may expose narrow seams for runtime injection, structured
cancellation/drain, output identity, selection evidence, and observations. It
must not own tenant SQL, leases, audit, final Iceberg commit, commit retry, or
uncertain-outcome reconciliation.

## Memory, spill, and concurrency

- Make each query or Forge attempt own its memory-pool lifetime. A process pool
  is an aggregate capacity root, not an operation-local grant.
- Account scan buffers, repartition buffers, hash state, exchange buffers,
  output builders, object-store writer buffers, Parquet row groups, footer
  state, and close/upload concurrency before admission.
- Keep spill allocation tenant- and operation-bound. Configure capacity,
  compression, file rotation, cleanup, and failure behavior. Disk exhaustion
  is a typed operation failure, not a reason to borrow another tenant's space.
- Bound target partitions, file-read concurrency, exchange fan-out, and open
  writers from measured resources. More partitions can increase retained
  buffers and memory even when each operator is individually bounded.
- Stream `RecordBatch` output. A user-sized `collect()` is forbidden.

Accurate statistics drive pruning, join choice, repartitioning, and routing.
Use exact Parquet footer/provider statistics over the pinned post-pruning file
set. File size and sort/time layout are not substitutes for row count,
cardinality, null count, or value distribution.

## Metrics and dependency boundary

Collect DataFusion plan and operator metrics for rows, batches, elapsed work,
spills, and partition behavior. Wyrd separately owns admission waits, queue
age, snapshot acquisition, exchange reservation, peer retry, WAL/catalog age,
object-store errors, and successful-terminal accounting. Never infer resource
or durability success from a metric descriptor alone.

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
