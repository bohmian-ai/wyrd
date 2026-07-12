# OLAP, Iceberg, DataFusion, And Arrow Review

Use this reference when a Wyrd/Vala proposal touches analytical storage,
Bifrost, Apache Iceberg, Iceberg Rust crates, observations, traces,
eval/drift results, DataFusion, Parquet, object stores, Apache Arrow, pyarrow,
high-volume query, archival retention, or `.dev/plan/foundations/12-olap-warehouse`.

This is a review lens. Report only issues that affect Wyrd doctrine, public
contracts, tenant isolation, reliability, data correctness, cost, operability,
implementation feasibility, or verification strength.

## Wyrd Ownership

- Vala owns observability: observations, traces, drift, eval execution,
  archival query, OLAP warehouse behavior, and background data-plane work.
- `wyrd-spec::vala` may own pure primitive contracts and public error catalog
  types. It must stay PyO3-free, async-free, IO-free, SQL-free, Arrow-free,
  DataFusion-free, Iceberg-free, object-store-free, and cloud-SDK-free.
- `vala-sql` owns Postgres migrations, row types, and queries for Wyrd control
  tables and any locked Postgres-backed Iceberg catalog schema.
- The Vala warehouse engine owns Iceberg catalog adaptation, schema conversion,
  write coordination, DataFusion provider/session integration, batch building,
  query diagnostics, and benchmarks.
- `wyrd-storage` owns artifact/blob storage and shared backend configuration.
  `vala-bifrost` owns Iceberg-specific `StorageFactory`, catalog-property, and
  warehouse-URI adaptation. Warehouse plans should reuse `BackendConfig` and
  avoid direct cloud SDK clients without moving the Iceberg dependency cone
  into broadly consumed storage crates.
- DataFusion owns query planning/execution. Wyrd/Vala owns tenant enforcement,
  table/catalog boundaries, pruning metadata, admission, freshness, diagnostics,
  public query contracts, and stable Wyrd errors.
- Apache Arrow is the memory contract across Rust, DataFusion, Parquet, and
  Python-visible analytical APIs.

High-volume observations, traces, spans, eval records, drift facts, and
warehouse records should not be stored as ordinary high-churn Postgres rows
unless the proposal proves the volume is low and retention/query needs are
transactional.

## Iceberg And Catalog Checks

Review whether the design:

- Treats Iceberg as the analytical table format and object-store Parquet as the
  durable bytes layer, not as a new Wyrd public noun.
- Keeps Bifrost/table metadata as Vala-owned engine or control-plane state,
  not as a `WarehouseCard` or parallel card ontology unless a locked decision
  explicitly adds one.
- Separates Postgres responsibilities: Iceberg catalog pointers versus Wyrd
  control-plane rows, idempotency, commit tracking, cache epochs, repair state,
  and registry metadata.
- Preserves tenant isolation in Wyrd-controlled SQL, query predicates, cache
  keys, object-store paths where applicable, metrics, traces, and service
  routes.
- Defines snapshot refresh semantics: read-your-writes, bounded lag, refresh
  epoch, query watermark, table version, or explicit stale-read behavior.
- Defines schema evolution rules: additive nullable fields, schema
  fingerprints, migration jobs, or new table versions for breaking physical
  changes.
- Defines compaction, manifest maintenance, snapshot expiration, orphan-file
  cleanup, retention safety, leases/fencing, metrics, and reader-age policy
  before claiming production readiness.
- Validates the dependency cone for `object_store`, DataFusion, Arrow, Parquet,
  Iceberg, Iceberg catalog crates, and `iceberg-datafusion`.

Blocking patterns:

- `deltalake`, `_delta_log`, Delta transaction-log logic, or Delta coexistence
  in new Vala warehouse paths without a fresh locked decision.
- Iceberg/DataFusion/Arrow/object-store dependencies in `wyrd-spec` or
  client-tier crates.
- Direct cloud SDK or object-store client construction inside the warehouse
  engine instead of using Wyrd storage handles.
- Assuming Iceberg catalog updates, Wyrd control rows, object-store writes, and
  cache invalidation are atomically committed without explicit recovery design.
- Treating Iceberg metadata as a queue, scheduler, lock table, or Wyrd registry.
- Local filesystem paths in multi-pod or object-store production designs.

## Cross-Store Consistency Checks

Review whether the plan specifies:

- The ordering of object-store writes, Wyrd SQL rows, Iceberg snapshot commit,
  audit records, cache invalidation, and response visibility.
- Idempotency for retryable writes, duplicate batches, writer crashes, and
  partially visible commits.
- Recovery for pre-commit rows, committed Iceberg snapshots that were not
  finalized in Wyrd SQL, orphaned data files, failed finalization, and stale
  cache epochs.
- Conflict behavior for concurrent writers: optimistic commit conflict,
  retry/fail policy, fencing, or per-table coordinator.
- Metrics and repair views for pending, failed, orphaned, duplicated, stale, or
  partially visible warehouse work.

Blocking patterns:

- "Best effort" cross-store updates for durable observations or audit-relevant
  records.
- No recovery path for process death between object-store write, Iceberg commit,
  and Wyrd SQL finalization.
- One durable commit/file per observation or request on high-volume paths.
- Unbounded in-memory writer state as the only record of pending durable work.

## DataFusion Query Checks

Review whether the design:

- Keeps selective predicates visible to DataFusion instead of filtering after
  collection.
- Preserves time, tenant/space, service, status, profile, card/run IDs, and
  other pruning fields as typed columns.
- Enforces tenant predicates for shared tables before optimization and proves
  they cannot be removed by query rewrites.
- Honors projection pushdown: scans read only requested columns.
- Reports filter pushdown conservatively. Manifest/file/row-group pruning is
  usually `Inexact`; report `Exact` only when the provider truly enforces the
  predicate at scan planning.
- Avoids unbounded joins or scans without time/tenant filters.
- Streams large results instead of collecting unbounded batches into memory.
- Exposes diagnostics: plan shape, rows/bytes scanned, manifests/files/row
  groups pruned, partitions, execution time, cache behavior, and memory/spill
  behavior.
- Uses explicit `SessionContext` configuration and shared object-store/runtime
  registration instead of rebuilding expensive contexts per query.
- Defines admission control for broad scans, dashboard queries, compaction, and
  maintenance work so they cannot starve ingestion or management-plane writes.

Blocking patterns:

- JSON columns for fields used in filters, joins, grouping, retention, or
  authorization.
- `collect()` on broad queries in service paths.
- Custom `ExecutionPlan` where a `TableProvider`, logical plan, or existing
  operator would work.
- SQL string concatenation with user input.
- Tenant or authorization filtering after DataFusion already scanned broad
  cross-tenant data.

## Arrow Schema Checks

Review Arrow physical types for query/storage behavior:

- Fixed-size IDs and hashes use compact binary types where equality and storage
  efficiency matter.
- Repeated dimensions use dictionary encoding where supported.
- Query-critical structured fields use typed `Struct`, `List`, `Map`, or
  top-level columns rather than opaque JSON strings.
- Analytical timestamps use microsecond precision with UTC where required by
  the locked plan.
- Large analytical APIs exchange `RecordBatch`, `Table`, or Arrow streams, not
  row vectors or JSON payloads.
- Schema creation is centralized, versioned/fingerprinted, and tested for empty
  batches, nulls, reserved columns, schema mismatch, and round trip through
  Iceberg write plus DataFusion read.

## Python And PyArrow Boundary

For Python-visible analytical APIs:

- Prefer `pyarrow.RecordBatch`, `pyarrow.Table`, or Arrow streams for large
  inputs/outputs.
- Keep PyO3 wrappers thin: convert Python input to Rust/Arrow, run Rust work
  without holding the GIL, convert output at the edge.
- Do not return pandas by default for large results; offer conversions as
  explicit helpers when needed.
- Do not hide expensive network/query/object-store work in property access or
  `__repr__`.
- Expose close, flush, shutdown, timeout, batch-size, and flush-interval knobs
  for buffered writers.
- Require public Python import/stub tests only once the Python surface exists.

## Review Questions

- What query class is being optimized: selective lookup, dashboard, bounded
  exploration, broad scan, join, ingestion, compaction, repair, or Python
  interop?
- Which columns prune Iceberg manifests, files, row groups, partitions, and
  tenant scope?
- What happens when a reader has a stale snapshot?
- What metrics prove bytes/files/manifests were pruned?
- How does the design fail under object-store throttling, catalog conflict,
  compaction backlog, writer crash, duplicate retry, and broad query pressure?
