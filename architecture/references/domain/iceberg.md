# Apache Iceberg

Load for Bifrost snapshots, catalogs, schema and partition evolution, object
layout, promotion, compaction, retention, or cleanup.

## Iceberg's boundary in Bifrost

Iceberg is Bifrost's table format and snapshot protocol. Object storage holds
immutable Parquet data and Iceberg metadata; the Postgres-backed catalog owns
the authoritative metadata pointer and Wyrd control state. Readers pin a
snapshot. Writers create metadata and attempt an optimistic catalog
compare-and-swap. Iceberg snapshot atomicity does not make Postgres control
transactions and object-store writes atomic with each other.

Physical identity is the authenticated tenant plus logical `TableRef`.
Derive namespaces and prefixes from that identity; never authorize by path.
Retain tenant and managed row identity as required columns through every
snapshot and rewrite. Partition transforms and sort order follow measured
predicate, cardinality, skew, and file-count behavior. Event-time day
partitioning is a common option, not a universal rule.

## Schema, partition, and snapshot correctness

- Preserve Iceberg field IDs, names, logical types, nullability, and defaults
  across Arrow and Parquet. Map by field identity or name, never position.
- Prefer additive schema evolution. Require explicit compatibility for type,
  nullability, required-field, identifier-field, and partition-source changes.
- Evolve partition specs rather than rewriting field meaning in place. Readers
  and maintenance must understand files written under every retained spec.
- Bind cached scan state to table UUID, schema/spec identity, and an acquired
  stable snapshot cut. Refresh by authoritative catalog acquisition; a
  time-based cache or storage listing cannot establish freshness.
- Use branch/ref and snapshot identity explicitly. A commit based on one branch
  or snapshot may not silently publish against another.

## Promotion and rewrite are distinct

Forge first promotes each eligible Scribe hot object unchanged with a fast
append. The exact footer-validated `DataFile` projection accompanies the
published Scribe object; Forge revalidates it physically and does not fabricate
or recalculate it. Promotion writes no data object and commits one promotion
snapshot under tenant, table, attempt, operation, branch, and fence authority.

After promotion, managed compaction selects and rewrites eligible live files toward an
approximately 1 GiB target with an independent 128 MiB encoded-row-group
target. The managed writer tests its encoded-size estimate between completed
writes, rolls before accepting the following write once the target is crossed,
and preserves the final residue for each partition. Neither target is a
universal lower or upper bound on physical object bytes. The managed core owns selection, grouping, delete
application, sorting, partition fan-out, rolling, and produced `DataFile`
values. Forge owns leases, Wyrd resources, attempt identity, handoff validation,
catalog publication, reconciliation, audit, and SQL settlement.

The rewrite seam is exactly:

```text
base_snapshot_id
rewritten_data_files
applied_position_delete_files
applied_equality_delete_files
output_data_files
```

Resolve rewritten and applied-delete identities from the immutable base
snapshot manifests. Move output `DataFile` values unchanged from the managed
core. Applied deletes are executor evidence/read inputs, not automatically
removed catalog entries. Forge removes a position-delete file only when every
referenced live data file was rewritten and its positions were applied. It
removes an equality-delete file only when the delete's partition/spec scope and
data sequence number cannot apply to a surviving unselected data file. Output
sequence numbers prevent applied deletes from reapplying to replacement rows.
Reject duplicate identities or unproven delete scope before publication.

Physical writers may execute concurrently inside one attempt. Their physical
paths retain the managed core's recipe segment and use an attempt prefix,
per-writer canonical decimal ordinal, and writer UUIDv7 for uniqueness. Forge
assigns a separate attempt-global logical ordinal to each opened output for
observer and recovery evidence; that logical ordinal is not a filename field.
One fenced owner constructs the final rewrite transaction and calls
`commit_once`; the managed core never mutates the catalog.

## Optimistic commit and reconciliation

Before commit, validate that the freshly acquired table metadata still satisfies the
rewrite assumptions: branch head, base ancestry, selected input existence,
schema/spec/sort policy, delete attachments, lease, and fence. A definite
catalog conflict may make one further `commit_once` call only after reacquiring
the latest metadata and revalidating those assumptions. It reuses the same
attempt, operation ID, output generation, and objects. Another conflict,
deadline, or changed assumption ends that attempt as definitely uncommitted and
requires a new plan. Never hide revalidation inside a generic retry.

Use deterministic operation identity and snapshot properties to distinguish:

- definite pre-commit failure, where new work may retry safely;
- committed acceptance, where SQL and source authority can settle;
- uncertain acceptance, where Forge reconciles the operation against latest
  metadata before any fresh attempt.

Output objects are not proof of snapshot publication. Protect all possible
attempt outputs until the catalog result is terminal.

## Maintenance separation

Data-file compaction, manifest rewrite, snapshot expiration, expired-file
cleanup, and never-published orphan cleanup are separate protocols with
separate selection, commit, retention, and audit evidence.

- Expiration respects all retained refs, reader/task watermarks, minimum age,
  and minimum snapshot count.
- Cleanup derives candidates from catalog reachability before expiration and
  confirms they are unreachable from every retained head afterward.
- Expired-file cleanup persists per-object progress and treats already-missing
  objects idempotently only after safety validation.
- Orphan cleanup protects every live snapshot, staged or prepared operation,
  publication attempt, committed Scribe `file_list` object without exact
  promotion evidence, pinned Oracle cut, and configured age window.
  A v1 live-tail lease retains Scribe-local Arrow batches and staged resources for its lifetime but names no Forge-collectable object, so it contributes no independent Forge GC root.
  A storage listing alone can never prove an orphan.

## Rejected shapes

Reject shared mutable manifests, object-prefix authorization, one snapshot per
event, positional schema mapping, global physical serialization, catalog
mutation by the managed core, deletion based on a stale cache/listing, physical
file size as publication identity, and retries that change base assumptions
without replanning or revalidation.

## Stable Wyrd anchors

- Bifrost lifecycle: `architecture/bifrost-design.md`.
- Engine catalog and Forge: `crates/vala/vala-bifrost-redux/src/catalog/` and
  `crates/vala/vala-bifrost-redux/src/forge/`.
- Catalog control state: `crates/vala/vala-sql/`.
- Object-store boundary: `crates/wyrd/wyrd-storage/`.

## Primary grounding

- [Iceberg table specification](https://iceberg.apache.org/spec/)
- [Iceberg reliability](https://iceberg.apache.org/docs/latest/reliability/)
- [Iceberg maintenance](https://iceberg.apache.org/docs/latest/maintenance/)
- [Iceberg partitioning](https://iceberg.apache.org/docs/latest/partitioning/)
- [Apache Parquet documentation](https://parquet.apache.org/docs/)
