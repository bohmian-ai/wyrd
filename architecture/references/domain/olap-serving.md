# OLAP serving

Load for Bifrost table design, ingest and query paths, analytical API shape,
admission control, or tenant-safe serving decisions.

## One analytical substrate

Bifrost is Wyrd's internal OLAP surface. Every logical table resolves to one
organization-qualified physical Iceberg table backed by object storage. The
catalog and control state live in Postgres; analytical bytes do not. Built-in
observation tables and user-defined analytical tables use the same binding and
system-column rules.

The serving boundary belongs to `wyrd-server`. Vala crates provide engines and
adapters. A query request is typed, tenant-bound, projection-aware, and
time-bounded before DataFusion planning. A record write is schema-checked,
idempotent, and stamped with event time, ingest time, batch ID, row ordinal, and
authenticated tenant.

## Query path

1. Authenticate the principal and resolve `(organization, TableRef)`.
2. Admit only allowed query classes, bounded time windows, and safe projections.
3. Build a typed DataFusion logical plan; do not pass arbitrary SQL through a
   privileged context.
4. Apply tenant and time predicates at the provider boundary, push projection
   and filters into the scan, and verify plan-root tenant protection.
5. Stream bounded Arrow batches or length-delimited frames. Expose diagnostics
   such as files, partitions, row groups, bytes, spills, and elapsed time.

Avoid collecting a full result in a request path. A caller-facing page or
stream must have an explicit byte, row, or time budget. Sensitive payload
columns (trace, log, GenAI, and agent-trace data) require an elevated read
permission in both typed and generic query paths.

## Write path and consistency

Bounded buffering amortizes object-store and catalog overhead. A durable append
ack means the WAL or equivalent durable queue has accepted the immutable batch;
it does not imply that compaction is complete. Control rows and Iceberg
snapshots need explicit recovery markers when they cannot commit atomically.
Retries use batch identity and never append the same batch twice.

## Trade-offs and failures

Strict admission protects a multi-tenant service but can reject exploratory
queries; provide a safe diagnostic or explain surface rather than bypassing the
gate. Streaming lowers memory pressure but requires backpressure and clear
cancellation semantics. Caching table providers improves latency but requires
catalog-epoch invalidation; stale schemas must fail with a typed conflict.

Fail closed on tenant mismatch, schema fingerprint conflict, oversized query,
unregistered table, replayed batch, unknown sensitive projection, or missing
catalog metadata. Do not hide a partial stream as a successful result.

## Anti-patterns

Reject shared physical tables with caller-supplied tenant filters, one commit
per event, a global mutex around all tables, unbounded `collect()`, positional
schema mapping, generic SQL as the only authorization boundary, and network
listeners in Vala crates.

## Stable Wyrd anchors

- Bifrost contract and serving rule: `architecture/wyrd-design.md` §Bifrost.
- Query and table contracts: `crates/wyrd-spec/src/vala/api/`.
- Query admission and server routes: `crates/wyrd/wyrd-server/`.
- Engine implementation: `crates/vala/vala-bifrost-redux/`.

## Primary grounding

- [Apache Iceberg overview](https://iceberg.apache.org/docs/latest/)
- [Apache DataFusion configuration and pruning](https://datafusion.apache.org/user-guide/configs.html)
- [Apache Arrow columnar format](https://arrow.apache.org/docs/format/Columnar.html)
- [NIST AI RMF 1.0](https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.100-1.pdf)
- Wyrd anchors: `architecture/wyrd-design.md` §Bifrost;
  `crates/wyrd/wyrd-server/`; `crates/vala/vala-bifrost-redux/`.
