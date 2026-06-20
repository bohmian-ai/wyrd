# Iceberg And Bifrost

Load this reference when work touches Vala OLAP, Bifrost, Apache Iceberg,
Iceberg Rust crates, DataFusion query paths, object-store analytical storage,
Parquet warehouse layout, trace/eval/drift archival query, or
`.dev/plan/foundations/12-olap-warehouse`.

This is implementation guidance for `wyrd-rust-python`, not architecture review
output. Verify exact crate APIs and resolved dependency versions from local
`Cargo.toml`, `Cargo.lock`, and official docs before coding.

## Wyrd Placement

- `wyrd-spec::vala` owns pure primitive contracts only: table identifiers,
  schema fingerprints, system column names, table metadata, query classes,
  projection/cache/admission enums, and public Wyrd error types.
- Keep `wyrd-spec::vala` PyO3-free, async-free, IO-free, Arrow-free,
  DataFusion-free, Iceberg-free, SQL-free, and cloud-SDK-free.
- `vala-sql` owns Postgres migrations, row types, and queries for
  `iceberg_catalog` plus Wyrd control tables under `vala.*`.
- `vala-bifrost` owns the warehouse engine: Iceberg catalog adapter, schema
  conversion, writer, commit coordination, DataFusion `TableProvider`,
  `SessionContext` factory, batch builder, and benchmarks.
- `wyrd-storage` owns object-store construction. Vala engine code consumes
  `Arc<dyn ObjectStore>` from the storage handle; it does not construct S3,
  GCS, Azure, or local object-store clients except in local tests.
- Python-visible warehouse behavior belongs in the owning Vala crate behind a
  `python` feature. `python/py-wyrd` only registers the module and package
  exports.

## Iceberg Rust Crate Map

- `iceberg`: core Apache Iceberg types and APIs, including catalogs, table
  identifiers, table scans, schemas, expressions, IO, metadata, and Arrow
  conversion.
- `iceberg-catalog-sql`: SQL catalog implementation. Use it for Wyrd's
  Postgres-backed Iceberg catalog when the local plan calls for
  Postgres-as-catalog.
- `iceberg-catalog-loader`: dynamic catalog loader that can return
  `Arc<dyn Catalog>` from a catalog name and string properties. Use only when
  the implementation needs runtime catalog selection; prefer explicit builders
  for the locked Wyrd catalog path.
- `iceberg-datafusion`: Iceberg table-provider integration for DataFusion.
  Wrap or compose it with Wyrd's provider boundary when tenant predicates,
  projection enforcement, diagnostics, or Wyrd errors must be added.
- Treat official Iceberg docs as a live API source. Do not copy examples
  blindly; adapt them to Wyrd error handling, tracing, tenancy, and storage
  boundaries.

## Dependency Rules

- Keep the OLAP dependency cone aligned: `object_store`, `datafusion`,
  `iceberg`, `iceberg-catalog-sql`, `iceberg-catalog-loader` if used, and
  `iceberg-datafusion`.
- Use workspace dependencies for the Iceberg/DataFusion/object-store cone.
  Avoid per-crate version overrides that can split trait object types.
- Validate resolved versions with `cargo tree`, especially around
  `object_store`, Arrow, Parquet, and DataFusion. A docs.rs crate version may
  depend on a different DataFusion minor than the Wyrd plan intends.
- Do not keep `deltalake`, `_delta_log`, or Delta-specific transaction logic in
  new Vala warehouse implementation. Delta references are predecessor evidence
  only unless a later locked decision says otherwise.
- Pin deliberately when the plan requires lockstep versions. No wildcard
  versions and no casual caret bumps across the OLAP cone.

## Catalog And Storage

- Keep two Postgres responsibilities distinct: the Iceberg SQL catalog stores
  Iceberg table metadata pointers; `vala.*` tables store Wyrd control-plane
  state, idempotency, commit tracking, cache epochs, registry rows, and repair
  queues.
- Preserve tenant isolation in `vala.*` tables with the existing Wyrd SQL/RLS
  pattern. The Iceberg catalog schema may be global if the locked plan says the
  catalog contract requires it; tenant enforcement still happens in Wyrd table
  metadata, provider predicates, and service paths.
- Store analytical bytes in object-store-backed Iceberg-managed Parquet files,
  not high-churn Postgres rows.
- Register object stores once through Wyrd storage/runtime boundaries and reuse
  those handles in DataFusion and Iceberg paths.
- Make catalog names, namespaces, table identifiers, table UIDs, snapshot IDs,
  schema fingerprints, and tenant IDs typed at Wyrd boundaries.

## Write Path

- Prefer append-oriented writes with bounded batching. Avoid one-file-per-event
  or one-Iceberg-commit-per-record paths.
- Stamp reserved Bifrost system columns at the engine boundary, not in user
  payloads: event time, ingested-at time, batch ID, and tenant ID when the
  table scope requires it.
- Reject user writes to reserved system columns with a typed Wyrd error.
- Use deterministic batch IDs or idempotency keys so retries can be detected.
- Coordinate each table's writes through one clear concurrency boundary, such
  as a bounded per-table actor or an explicit commit coordinator. Avoid broad
  `Arc<Mutex<_>>` state around writer internals.
- When Postgres control rows and Iceberg commits must move together, implement
  explicit recovery semantics: pre-commit row, Iceberg snapshot commit,
  finalize row, and replay/repair for interrupted commits.
- Map Iceberg, SQL, object-store, schema, and concurrency failures into local
  `thiserror` enums first, then into public `wyrd_spec` error catalog types
  at public boundaries.

## Query Path

- Keep tenant, time, card/run IDs, service, status, profile, and other pruning
  dimensions as typed columns where queries filter, group, join, retain, or
  authorize on them.
- Wyrd's DataFusion provider boundary must honor projection pushdown. Do not
  read all columns and filter after collection.
- Report filter pushdown honestly. Tenant and partition predicates can be
  `Exact` only when the provider truly enforces them at scan planning; file or
  manifest pruning is often `Inexact`.
- For `SystemShared` tables, inject or require the tenant predicate before
  optimization and test that it cannot be removed by query rewrites.
- Avoid unbounded `collect()` in service paths. Stream Arrow `RecordBatch`
  results or page query results through a bounded API.
- Expose query diagnostics when useful: logical/physical plan, bytes scanned,
  files/manifests pruned, partitions touched, row groups pruned, elapsed time,
  and spill/memory behavior.
- Cache `SessionContext`, catalog, or table providers only behind explicit
  refresh epoch or invalidation rules. Do not assume readers see other pods'
  commits immediately without refresh semantics.

## Arrow And Schema

- Centralize Arrow schema construction for each Bifrost table family.
- Use compact binary representations for fixed-size IDs, tenant IDs, batch IDs,
  hashes, and schema fingerprints when equality and storage efficiency matter.
- Prefer typed columns, structs, lists, and maps over JSON for query-critical
  fields.
- Use microsecond UTC timestamps for event and ingest times unless a locked
  plan changes the physical contract.
- Test empty batches, nullability, schema mismatch, reserved columns, and round
  trips through Iceberg write plus DataFusion read.

## Python Boundary

- Prefer `pyarrow.RecordBatch`, `pyarrow.Table`, or Arrow streams for
  Python-visible analytical inputs and outputs.
- Convert Python inputs at the PyO3 boundary, release the GIL for blocking or
  long-running Rust work, and convert outputs at the edge.
- Do not hide network, object-store, or query execution work in Python
  properties, `__repr__`, or cheap-looking accessors.
- Expose explicit close, flush, shutdown, timeout, batch-size, and
  flush-interval controls for buffered writers once Python writer APIs exist.
- Add Python tests only when the Python surface exists. Until then, keep
  warehouse validation in Rust integration tests and benches.

## Verification

- For dependency work, run the local object-store/DataFusion/Iceberg pin check
  if present, plus targeted `cargo tree` diagnostics for split versions.
- For engine code, run targeted crate checks such as:

```bash
cargo check -p vala-bifrost --all-features
cargo test -p vala-bifrost --all-features -- --test-threads=1
```

- For SQL catalog/control changes, run the relevant `vala-sql` tests and tenant
  isolation checks.
- For public contracts or generated artifacts, run `mise run codegen:check`.
- For Python-visible warehouse work, run `mise run py:setup` and
  `mise run py:test:unit`.
- For foundation boundary changes, include `check:client-tier`,
  `check:pyo3-scope`, `check:mocks-scope`, `check:unwrap-audit`, and
  `check:wasm` when relevant.

## Reject These Patterns

- Iceberg or DataFusion dependencies in client-tier crates.
- Iceberg, Arrow, SQL, object-store, or PyO3 imports in `wyrd-spec`.
- Direct cloud SDK construction inside `vala-bifrost`.
- Stringly table, tenant, snapshot, or namespace identifiers at durable
  boundaries.
- Delta transaction-log logic in new Iceberg paths.
- Query filters applied only after DataFusion has collected broad results.
- Python wrappers that duplicate warehouse behavior instead of calling the Rust
  owner crate.
