# Iceberg And Bifrost

Load this reference when work touches Vala Bifrost, Apache Iceberg, the
Iceberg-Rust crates, DataFusion query paths, object-store analytical
storage, Parquet warehouse layout, or trace/eval/drift archival query.

This is implementation guidance, not architecture review output. Verify
exact crate APIs and resolved dependency versions from local `Cargo.toml`,
`Cargo.lock`, and official docs before coding.

## Current Source of Truth

The active Bifrost architecture (four-role rebuild: gate / scribe /
forge / oracle) is captured in maintainer-local planning at
`.dev/plan/not-started/06-bifrost-rebuild/` when present. That directory
is gitignored — other contributors will not see it in a fresh checkout.

If you have those local plan files, read them **before** editing under
`vala-bifrost` or `vala-bifrost-redux`. If you do not, ask the maintainer
driving the work, or inspect the current `vala-bifrost` crate plus
`architecture/wyrd-design.md` and this reference for the durable
contracts. Do not improvise architecture from scratch.

## Four Roles, One Binary

Bifrost is a single binary with four internal roles running per pod. No
per-role ownership; autoscaling is per-pod (HPA on CPU/memory). Optional
`WYRD_ROLES` env flag restricts a pod to a subset (narrow use case for
large SaaS).

| Role | Function |
|---|---|
| **Gate** | Auth; resolve `Principal` + `DataTenantId` + `TableRef`; dispatch writes to Scribe, reads to Oracle. Stateless per request. |
| **Scribe** | WAL append → fsync ack; memtable buffer; seal to Parquet + `file_list` + audit outbox in one tx. |
| **Forge** | Bin-pack small Parquet → big Parquet; Iceberg REPLACE; expire snapshots; orphan GC. Single-writer Iceberg committer. |
| **Oracle** | Fused scan (Iceberg + `file_list` + live Scribe WAL Phase 2); admission; distributed lane fan-out; read audit. |

`wyrd-server` remains the only serving surface — Gate registers handlers
under the existing `wyrd-server` router. `vala-*` crates are engines, not
serving crates.

## Crate Isolation Rule (Rebuild)

- `vala-bifrost-redux` **must not** depend on `vala-bifrost`. Enforced by
  `check:redux-isolation`.
- Redux imports from: `wyrd-spec`, `wyrd-runtime`, `wyrd-sql`,
  `wyrd-auth-verify`, `datafusion`, `iceberg`.
- Copy — do not depend on — patterns like `BifrostNamespace`,
  `SchemaFingerprint`, `TableRef`, system-column constants,
  `bifrost_writer_properties`, `attach_tenant_filter`.
- Migrations land in `crates/vala/vala-sql/migrations/` — `vala-sql`
  outlives both bifrost crates.
- Error codes land in `crates/wyrd-spec/src/error.rs` via
  `#[wyrd_error(...)]`.

## Wyrd Placement

- `wyrd-spec::vala` — pure primitive contracts only: table identifiers,
  schema fingerprints, system column names, table metadata, query
  classes, projection/cache/admission enums, public Wyrd error types.
  PyO3-free, async-free, IO-free, Arrow-free, DataFusion-free,
  Iceberg-free, SQL-free, cloud-SDK-free.
- `vala-sql` — Postgres migrations, row types, and queries for
  `iceberg_catalog`, `vala.file_list`, `vala.cluster_nodes`,
  `vala.audit_outbox`, `vala.maintenance_leases`, and Wyrd control
  tables under `vala.*`.
- `vala-bifrost` (current) / `vala-bifrost-redux` (rebuild) — warehouse
  engine: Iceberg catalog adapter, schema conversion, writer, commit
  coordination, DataFusion `TableProvider`, `SessionContext` factory,
  batch builder, benchmarks.
- `wyrd-storage` — artifact/blob storage, shared `BackendConfig`,
  signing, health checks, OpenDAL operator. Bifrost owns Iceberg-specific
  storage adaptation (`StorageFactory`, catalog properties, warehouse URI
  derivation) and may translate `BackendConfig` into Iceberg-specific
  config without moving the Iceberg dependency cone into `wyrd-storage`.
- Python-visible warehouse behavior lives in `vala-sdk` behind its
  `python` feature. `python/py-wyrd` only registers the module and package
  exports.

## Iceberg Rust Crate Map

- `iceberg` — core types: catalogs, table identifiers, table scans,
  schemas, expressions, IO, metadata, Arrow conversion.
- `iceberg-catalog-sql` — SQL catalog implementation for Postgres-backed
  Iceberg.
- `iceberg-catalog-loader` — dynamic catalog loader returning
  `Arc<dyn Catalog>`. Use only when runtime catalog selection is
  required; prefer explicit builders otherwise.
- `iceberg-datafusion` — Iceberg table-provider integration for
  DataFusion. Wrap or compose with Wyrd's provider boundary to add tenant
  predicates, projection enforcement, diagnostics, and Wyrd errors.
- **Iceberg fork pinned** for Bifrost rebuild:
  `https://github.com/mitari-ai/iceberg-rust`, branch
  `wyrd/rewrite-files-action` (Phase-0 prerequisite before Forge lands).

Treat official Iceberg docs as a live API source. Do not copy examples
blindly; adapt them to Wyrd error handling, tracing, tenancy, and storage
boundaries.

## Dependency Rules

- Keep the OLAP dependency cone aligned: `object_store`, `datafusion`,
  `iceberg`, `iceberg-catalog-sql`, `iceberg-catalog-loader` (if used),
  `iceberg-datafusion`.
- Use workspace dependencies for the Iceberg/DataFusion/object-store
  cone. Avoid per-crate version overrides that can split trait object
  types. Enforced by `check:object-store-pin`.
- Validate resolved versions with `cargo tree`, especially around
  `object_store`, Arrow, Parquet, and DataFusion.
- Do not keep `deltalake`, `_delta_log`, or Delta transaction-log logic
  in new Vala warehouse implementation. Delta references are predecessor
  evidence only.
- Pin deliberately when the plan requires lockstep versions. No wildcard
  versions, no casual caret bumps across the OLAP cone.

## Storage Format Contract

- **WAL** is Arrow IPC (local disk, ack path): `StreamWriter`,
  append-only, no encoding. Never queried.
- **Sealed staging files** are Parquet (object store, indexed in
  `file_list`): encoded with `bifrost_writer_properties` (bloom filters,
  page index, sort order). Queried directly by Oracle until Forge
  rewrites.
- **Compacted files** are Parquet (Iceberg snapshot): sorted by
  `(data_tenant_id, wyrd_event_time)`, Bifrost writer properties.
- **Partition spec**: `Day(wyrd_event_time)` for every organization-qualified
  physical table. The organization is encoded in the Iceberg namespace and
  object-store prefix, not in a shared-table bucket.
- **No Delta Lake.** Iceberg is the sole snapshot format.

## Object Store

- Parquet paths:
  `{s3_prefix}/tenants/{tenant_uuid}/{logical_namespace}/{table_name}/day=YYYY-MM-DD/{pod_id}-{ulid}.parquet`.
- Scribe writes staging; Forge writes compacted big files and deletes
  orphans (via ORPHAN_GC, 24h TTL safety window).
- All reads go through `WyrdObjectStore` (OpenDAL-backed), cached through
  the local disk tier (`foyer`).

## Catalog And Storage

- Keep two Postgres responsibilities distinct: the Iceberg SQL catalog
  stores Iceberg table metadata pointers; `vala.*` tables store Wyrd
  control-plane state, idempotency, commit tracking, cache epochs,
  registry rows, and repair queues.
- Preserve tenant isolation in `vala.*` tables with the existing Wyrd
  SQL/RLS pattern.
- Store analytical bytes in object-store-backed Iceberg-managed Parquet
  files, not high-churn Postgres rows.
- Register object stores once through Wyrd storage/runtime boundaries and
  reuse those handles in DataFusion and Iceberg paths.
- Type catalog names, namespaces, table identifiers, table UIDs, snapshot
  IDs, schema fingerprints, and tenant IDs at Wyrd boundaries.

## Write Path

- Prefer append-oriented writes with bounded batching. Avoid
  one-file-per-event or one-Iceberg-commit-per-record paths.
- Stamp reserved Bifrost system columns at the engine boundary, not in
  user payloads: event time, ingested-at time, batch ID, and the
  server-authenticated organization tenant ID on every physical table.
- Reject user writes to reserved system columns with a typed Wyrd error.
- Use deterministic batch IDs or idempotency keys so retries can be
  detected.
- Coordinate each table's writes through one clear concurrency boundary
  (bounded per-table actor, explicit commit coordinator). Avoid broad
  `Arc<Mutex<_>>` state around writer internals.
- When Postgres control rows and Iceberg commits must move together,
  implement explicit recovery semantics: pre-commit row, Iceberg
  snapshot commit, finalize row, replay/repair for interrupted commits.
- Map Iceberg / SQL / object-store / schema / concurrency failures into
  local `thiserror` enums first, then into `WyrdError` at public
  boundaries.

## Query Path

- Keep tenant, time, card/run IDs, service, status, profile, and other
  pruning dimensions as typed columns where queries filter, group, join,
  retain, or authorize on them.
- The Wyrd DataFusion provider boundary honors projection pushdown. Do
  not read all columns and filter after collection.
- Report filter pushdown honestly. Tenant and partition predicates can
  be `Exact` only when the provider truly enforces them at scan
  planning; file / manifest pruning is often `Inexact`.
- For every physical table, resolve the authenticated organization and logical
  table before optimization. Add the plan-root tenant tripwire and test that a
  wrong `data_tenant_id` cannot be returned or silently filtered.
- Avoid unbounded `collect()` in service paths. Stream Arrow
  `RecordBatch` or page through a bounded API.
- Expose query diagnostics when useful: logical/physical plan, bytes
  scanned, files/manifests pruned, partitions touched, row groups
  pruned, elapsed time, spill/memory behavior.
- Cache `SessionContext`, catalog, or table providers only behind
  explicit refresh-epoch or invalidation rules.

## Arrow And Schema

- Centralize Arrow schema construction for each Bifrost table family.
- Use compact binary representations for fixed-size IDs, tenant IDs,
  batch IDs, hashes, and schema fingerprints when equality and storage
  efficiency matter.
- Prefer typed columns, structs, lists, and maps over JSON for
  query-critical fields.
- Use microsecond UTC timestamps for event and ingest times unless a
  locked plan changes the physical contract.
- Test empty batches, nullability, schema mismatch, reserved columns,
  and round trips through Iceberg write + DataFusion read.

## Python Boundary

- Prefer `pyarrow.RecordBatch`, `pyarrow.Table`, or Arrow streams for
  Python-visible analytical inputs and outputs.
- Convert Python inputs at the PyO3 boundary, release the GIL for
  blocking or long-running Rust work, and convert outputs at the edge.
- Do not hide network, object-store, or query execution work in Python
  properties, `__repr__`, or cheap-looking accessors.
- Expose explicit close, flush, shutdown, timeout, batch-size, and
  flush-interval controls for buffered writers.

## Verification

- Run `mise run test:bifrost` and `mise run test:bifrost:journey` after
  Bifrost changes. If a local rebuild plan is in play, follow the gates
  it specifies as well.
- For dependency work, run `check:object-store-pin` plus targeted
  `cargo tree` diagnostics for split versions.
- For SQL catalog / control changes, run `mise run test:sql` and
  `check:tenant-isolation`.
- For public contracts or generated artifacts, run
  `mise run codegen:check`.
- For Python-visible warehouse work, run `mise run py:setup` and
  `mise run py:test:unit`.
- For foundation boundary changes, include `check:client-tier`,
  `check:pyo3-scope`, and `check:unwrap-audit` when relevant.

## Reject These Patterns

- Iceberg or DataFusion dependencies in client-tier crates.
- Iceberg, Arrow, SQL, object-store, or PyO3 imports in `wyrd-spec`.
- Direct cloud SDK construction inside `vala-bifrost*`.
- Stringly table, tenant, snapshot, or namespace identifiers at durable
  boundaries.
- Delta transaction-log logic in new Iceberg paths.
- Query filters applied only after DataFusion has collected broad
  results.
- Python wrappers that duplicate warehouse behavior instead of calling
  the Rust owner crate.
- HTTP/gRPC serving code in `vala-*` crates (belongs in `wyrd-server`).
