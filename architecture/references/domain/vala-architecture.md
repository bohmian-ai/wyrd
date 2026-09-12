# Vala architecture

Load this reference for broad Wyrd/Vala ownership or a choice that crosses
observations, evaluation, drift, and analytical storage. Load the narrower
domain references for implementation mechanics.

## Product and serving boundary

`wyrd-server` is Wyrd's only external serving surface. It owns authentication,
authorization, public HTTP and gRPC routes, MCP, request validation, tenancy,
and public audit behavior. Vala owns the engines behind observation storage,
telemetry projections, evaluation, drift computation, and Bifrost. Skald owns
model, prompt, tool, and agent-runtime primitives. SDKs project the typed wire
contract and never become durable registries, query engines, or data stores.

```text
client / agent
  -> wyrd-server
     -> Vala
        -> Scribe: pod-local ingest, WAL, staged runs, hot publication
        -> Oracle: interactive and streamed distributed reads
        -> Forge: Iceberg promotion, rewrite, and maintenance
        -> Source: read-only adapters for external data
```

Internal Scribe live-tail and Oracle peer RPCs are authenticated engine seams,
not independent public services. Vala crates may construct their service
implementations, but `wyrd-server` owns listener lifecycle and the external
network contract.

`Source` is a Card describing how Vala reads an external system. Wyrd never
writes external warehouses, metric backends, or user object stores. Bifrost is
Wyrd-owned analytical storage and is not a Card kind.

## Bifrost ownership

- **Scribe** owns schema-checked append admission, sixteen fixed pod-local
  shards, WAL/fsync/fence-before-acknowledgement, active and immutable rows,
  durable staged-run authority, approximately 512 MiB immutable hot objects,
  exact replay, and bounded live-tail service behavior. One logical append is
  owned by one pod-local Scribe; ingest is not a distributed write protocol.
- **Oracle** owns typed query admission, interactive execution, streamed
  distributed analytical execution, pinned read cuts, tenant tripwires,
  query-local memory and spill, cancellation, terminal streaming, and query
  diagnostics.
- **Forge** owns unchanged promotion of eligible Scribe hot objects into
  Iceberg, managed-core rewrites toward approximately 1 GiB files, catalog
  publication, leases and fences, reconciliation, retention, cleanup, and
  maintenance telemetry.
- **Postgres** owns catalog pointers, tenant-scoped control state, leases,
  operation identities, and transactional audit rows. It does not store
  analytical payload bytes.
- **Object storage** owns immutable Parquet and Iceberg metadata objects. A
  path or prefix is never an authorization boundary.

The managed compaction core may plan and physically rewrite files, but it never
owns Wyrd tenancy, leases, audit, or catalog commit. DataFusion executes plans;
it never decides Wyrd authorization, admission, retry, or successful-terminal
semantics.

## Durable invariants

- Physical analytical identity is `(tenant, logical table)`. Shared
  physical tables with caller-supplied tenant predicates are forbidden.
- Server-owned system columns and `(wyrd_batch_id, wyrd_row_ordinal)` survive
  WAL, staging, Parquet, Iceberg promotion, Forge rewrite, and query unchanged.
- A Card is an optional declared subject. A Run is one client execution. Every
  Observation retains authenticated tenant and publisher `principal_id`; when
  supplied it also retains authorized `card_ref`, resolved `card_uid`, opaque
  `run_id`, and propagated `Wyrd-Request-Id` at the contract grain defined by
  its table.
- `wyrd-spec` remains IO-free and owns wire types. Durable behavior belongs to
  server and Vala owner crates; language bindings remain projections.
- Every queue, reservation, retry, and cleanup path is bounded. Tenant
  mismatch, contradictory durable evidence, or ambiguous publication fails
  closed without discarding the last valid authority.
- Audit cardinality follows evaluated permission decisions: one event per
  received verdict, appended before the result or refusal. Oracle read admission
  uses the one local-WAL audit exception. Engine-internal transitions record
  lineage in their own operational tables instead of audit.

## Rejected shapes

Reject listeners owned outside `wyrd-server`, Python-side durable logic,
external write adapters, stringly identity, shared physical tenant tables,
process-global analytical memory as query admission, a second warehouse noun,
and compatibility routes. A UI may explain Vala state but can never be the only
way to ingest, query, evaluate, monitor, or operate it.

## Stable Wyrd anchors

- Protocol and Card doctrine: `architecture/wyrd-design.md`.
- Bifrost internals: `architecture/bifrost-design.md`.
- Public Vala contracts: `crates/wyrd-spec/src/vala/`.
- Engines: `crates/vala/`.
- External serving owner: `crates/wyrd/wyrd-server/`.

## Primary grounding

- [Apache Iceberg specification](https://iceberg.apache.org/spec/)
- [Apache DataFusion architecture](https://datafusion.apache.org/library-user-guide/building-logical-plans.html)
- [Apache Arrow columnar format](https://arrow.apache.org/docs/format/Columnar.html)
- [OpenTelemetry signals](https://opentelemetry.io/docs/concepts/signals/)
