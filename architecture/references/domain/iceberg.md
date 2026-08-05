# Apache Iceberg

Load for Iceberg table metadata, snapshots, partitioning, schema evolution,
catalog behavior, object-store layout, or compaction choices in Bifrost.

## What Iceberg gives Wyrd

Iceberg is the table format and snapshot protocol for Bifrost's analytical
Parquet files. A table metadata record names schemas, partition specs, data
files, manifests, and snapshots. Readers select a consistent snapshot; writers
commit a new snapshot with optimistic concurrency. The Postgres-backed catalog
stores table metadata pointers and Wyrd control state while object storage holds
the data and metadata files.

Use a stable logical `TableRef`; derive the physical namespace and prefix from
the authenticated organization. Partition by event time (typically a day
transform) and retain tenant identity as a required system column. Partitioning
is a pruning aid, not an authorization boundary.

## Evolution and compaction

Prefer additive schema changes with explicit compatibility checks. Field IDs and
logical types must survive Arrow/Parquet conversion; never rely on positional
column order. Evolve partition specs when query patterns or volume justify it,
and verify that old and new snapshots remain readable. Keep compaction as a
single-writer, bounded operation that produces a new snapshot and records its
input/output set. Object-store cleanup waits for a safety window and only
deletes files proven unreachable from retained snapshots.

Catalog caches need an epoch or refresh token. A cached table object is valid
only until the catalog says otherwise. Snapshot IDs, table UUIDs, schema
fingerprints, and tenant IDs are typed at Wyrd boundaries.

## Failure modes

Expect conflicts between concurrent commits, orphaned files after a process
crash, stale catalog reads, incomplete cleanup, and schema mismatches at
append. Recovery must distinguish an acknowledged durable batch from a
committed snapshot and provide repair visibility. Never delete a file merely
because it is absent from a local cache or a pre-commit list.

## Anti-patterns

Reject treating object-store prefixes as authorization, embedding tenant IDs
only in paths, using one mutable manifest without snapshots, changing field
meaning in place, writing one table commit per event, and compacting without
retention or concurrency fencing. Do not expose Iceberg implementation details
as a new public Card kind or client-owned catalog contract.

## Stable Wyrd anchors

- Physical identity and Bifrost rules: `architecture/wyrd-design.md` §Bifrost.
- Catalog and table adapters: `crates/vala/vala-bifrost/src/catalog/`.
- Postgres catalog/control boundary: `crates/vala/vala-sql/`.
- Object-store integration: `crates/wyrd/wyrd-storage/` and Vala adapters.

## Primary grounding

- [Apache Iceberg documentation](https://iceberg.apache.org/docs/latest/)
- [Iceberg table specification](https://iceberg.apache.org/spec/)
- [Iceberg API overview](https://iceberg.apache.org/docs/latest/api/)
- [Apache Parquet documentation](https://parquet.apache.org/docs/)
- Wyrd anchors: `architecture/wyrd-design.md` §Bifrost;
  `crates/vala/vala-bifrost/src/catalog/`; `crates/vala/vala-sql/`.
