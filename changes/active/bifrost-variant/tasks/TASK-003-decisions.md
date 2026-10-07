# TASK-003 supplement: decisions and next work

Companion to `TASK-003-shredding-and-leaf-pruning.md`. It records the user's
decisions after the hot-reader fix (`b807fcf72`) so the work can resume from
here. Each item below still needs a spec revision in
`changes/active/bifrost-variant/spec.md` and user approval before it is built.

## Where we are

- Fixed (`b807fcf72`): a Variant filter or projection on a key the file did not
  shred errored (`requires StructArray as input`) when every row's residual
  `value` was null. Every Variant key lookup, at any depth, goes through
  `VariantGet::invoke_with_args` (`oracle/variant_sql.rs`), which now requests
  Variant output from Arrow. A query either uses the shredded leaf or falls
  back to the residual; it never errors.
- Gap that matters most: published (Iceberg) reads never use shredded leaves.
  Forge promotes each Scribe object to Iceberg about a second after it is
  published (`forge/scheduler.rs` `promote_hinted`), and the Iceberg reader
  unshreds before filtering. Almost every query takes the slow, correct path.
- There is one shredding implementation: the iceberg fork's
  `crates/iceberg/src/writer/file_writer/variant_shredding.rs`. Scribe
  (`scribe/parquet_writer.rs`) and Forge (`forge/managed/policy.rs`) both call
  it.
- Shredded today: every Variant that is a table column. That includes the OTel
  `attributes`, `resource_attributes` and `scope_attributes` of spans, logs and
  metrics; logs `body`; metrics `metadata` and `filtered_attributes`.
- Not shredded today: a Variant inside a list column (spans `events[].attributes`,
  `links[].attributes`), and JSON arrays inside any Variant value.

## Agreed order

1. One read core (below).
2. One sampling algorithm (below).
3. Nested shredding (below).

## 1. One read core

Decision, from the independent review the user approved:

> Iceberg plans and governs published data; one shared Wyrd Parquet read core
> decodes all Wyrd-written data files. Published reads retain Iceberg task
> semantics around that core.

```
Hot files
  └─ Wyrd source adapter ───────────────┐
                                        ├─ FileReadPlan → Parquet decoder
Published files                         │
  └─ Iceberg snapshot/manifest planner ─┘
       └─ Iceberg task semantics
          deletes, field IDs, constants,
          sequence/lineage metadata
```

Oracle owns orchestration. Iceberg owns published-table state and task
semantics. `FileReadPlan` owns physical Parquet projection, nested filtering,
row-group/page pruning, and Variant reconstruction.

Not approved: removing the Iceberg reader and failing closed on deletes. That
cuts an existing, tested contract.

| Concern | Decision |
|---|---|
| Iceberg catalog, snapshots and manifests | Keep |
| `TableScan::plan_files()` and `FileScanTask` (`oracle/exec.rs` ~1422) | Keep |
| Manifest/file statistics pruning | Keep |
| Delete association and application (`oracle/exec.rs` ~5296; `tests/integration/forge/managed_rewrite.rs` ~555) | Keep |
| Field-ID and task schema transformations, defaults, partition constants | Keep |
| Forge v3 lineage preservation (`architecture/bifrost-design.md` ~973) | Keep |
| Tenant-proving footer loader | Keep |
| Follower `assigned ⊆ planned` verification | Keep |
| Byte-range splitting | Keep, shared |
| Per-file projection and decoder filters | One shared implementation |
| Row-group/page/nested pruning | One shared implementation |
| Variant shredded/residual reconstruction | One shared implementation |
| Duplicate published Parquet decoding logic | Delete |

Whether `iceberg::arrow::ArrowReader` remains internally is secondary. Remove
it only after its task semantics wrap or feed the shared decoder. Do not
reimplement those semantics in Oracle.

Spec wording change (REQ-023 `spec.md` ~1005, INV-008 ~1098): "both readers"
becomes "Both Oracle source adapters use the same per-file Parquet read plan.
The published adapter additionally preserves Iceberg snapshot, task, delete,
field-identity, partition, and metadata semantics."

Required proof before completion:

1. Unshredded-key fallback fixed (done, `b807fcf72`). Still owed: mixed files
   where a key is shredded in some files and residual in others.
2. A committed Forge/Scribe compatibility test. The earlier dump test was
   evidence only and was reverted.
3. The published journey TASK-003 requires (`published::struct_and_variant_share_physical_pushdown`):
   hot, promoted and rewritten files with Struct and Variant filters.
4. Oracle position-delete coverage and Forge position/equality-delete journeys
   stay green.
5. Tenant-footer, byte-range, LIMIT and follower-assignment checks stay green.
6. Variant and v3 lineage survive repeated Forge rewrites.

## 2. Sampling

Decision: one common stratified sampling algorithm for every Variant layout,
following standard practice.

- Seeded and reproducible. A re-run claim must write the same objects, so the
  seed derives from the claim or object identity, never from time or a global
  RNG.
- Stratified by the system-injected `principal_id` (`tables/managed_columns.rs`
  `MANAGED_COLUMNS`). It is the only injected column that is server-set, never
  null, and present on every table that carries injected columns: user tables,
  spans, logs, metrics, gateway, verification, and `dev.agent_traces`. It is
  already a managed Bloom column. A table without it (`audit_log`, written only
  by the `AuditPublisher`) is one stratum. A table with one writer is also one
  stratum, which is plain seeded random sampling. One rule, no per-table
  configuration.
  - Rejected: partition columns (every signal table partitions by hour of
    `wyrd_event_time`, so rows in one file barely differ there);
    `service_name` (caller-supplied, nullable, only on the signal tables);
    `card_uid` alone (nullable, and one stratum per Card version).
  - A principal is created per registered service and is not shared across
    Card versions, but one principal can write for many Cards. Their rows form
    one stratum. A key is still shredded when it is common within that
    stratum; only a key rare across the principal but common within one of its
    Cards is missed. Upgrade path if that shows up: stratify by
    `(principal_id, card_uid)`, falling back to `principal_id` when `card_uid`
    is null, with a stratum-count cap.
- Sample size follows the algorithm (it scales with the input row count),
  bounded only by a memory cap. The fixed `max_bytes: 64 MiB` and
  `max_rows: 4096` in `BIFROST_VARIANT_SHREDDING`
  (`parquet/writer_properties.rs` ~95) are arbitrary and go away. The memory
  bound comes from TASK-004's shared-pool prefix reservation instead.
- Sample the whole input, not its first rows. Scribe claims and Forge rewrites
  read finished inputs, so a pre-pass over only the Variant columns picks the
  sample before the writer opens. Cost: one extra read of those columns.

Constraint: a Parquet file has one schema, so each Variant column has one
layout per file. Per-writer layouts inside one file are impossible. The
stratified result is one combined layout: a key is shredded when it is common
(at or above the frequency threshold) in any stratum, and keys compete for the
emitted-children cap by frequency. Other writers' rows hold nulls in those
columns, which cost almost nothing in Parquet.

Deferred: sorting system-table files by `principal_id`, so each file is mostly
one writer and files or row groups prune by writer. Add it only if the
combined layout proves too wide.

Open, to settle in the spec revision: the sample-size formula, the
stratum allocation (proportional versus a per-stratum floor so small writers
are seen), and the frequency threshold.

## 3. Nested shredding

Decision: agreed.

- A Variant inside a list or struct column (spans `events[].attributes`,
  `links[].attributes`) is shredded by the same shared shredder. It finds every
  Variant field in the schema, not only table columns. Parquet allows a
  shredded Variant inside a list. Reads need an "any element matches" filter
  plus row-group statistics on the nested leaf.
- Nested object keys (`doc->'x'->>'y'`) are already shredded and read through
  the shared path. Unit test
  `oracle::nested_pushdown::tests::nested_variant_keys_filter_through_shredded_and_residual_leaves`
  covers a shredded nested key (`o.x`), an unshredded one read from `o`'s
  residual (`o.y`), and a missing one when `o`'s residual is all null
  (`o.z`). Without the `b807fcf72` fix the last case fails with the
  production error.
- JSON arrays inside a Variant value: Arrow's `shred_variant` can store a typed
  list, but its `ShreddedSchemaBuilder` does not support list paths yet
  (`parquet-variant-compute` 60 `shred_variant.rs` ~543, ~669). Skipped until a
  real query needs it.
