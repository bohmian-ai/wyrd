# SPEC-bifrost-variant revision 20: one per-file read core

Status: approved by the user on 2026-10-07 (substance recorded in
`tasks/TASK-003-decisions.md` §1).

This file holds the exact text of revision 20. It is kept apart from
`spec.md` because `spec.md` carries revision 19, edited outside this session
and not yet committed. When revision 19 is committed, this text is applied to
`spec.md` (frontmatter `revision: 20`) and this file is deleted.

## Why

The approved spec already says both Oracle readers call the one per-file
read-planning facade (`spec.md` "Semantic Variant access on shared
nested-field pushdown", REQ-023, INV-008). The code does not:

- Hot reads: `HotParquetExec` builds a `FileReadPlan`
  (`B/oracle/nested_pushdown.rs`), which calls `PerFileParquetReadPlanner`
  (`B/oracle/exec.rs` ~3080). Shredded leaves are projected, filtered while
  decoding, and pruned.
- Published reads: Oracle hands each Iceberg `FileScanTask` to the pinned
  `iceberg::arrow::ArrowReader` (`B/oracle/exec.rs` ~1001). It never builds a
  `FileReadPlan`, reads the whole Variant root, and does not use shredded
  leaves for projection, decoder filtering, or row-group/page pruning.

Forge promotes every Scribe object to Iceberg shortly after publication, so
almost every query takes the published path. Revision 20 states how the one
read core is reached without dropping Iceberg semantics.

## Changes to `spec.md`

### "Semantic Variant access on shared nested-field pushdown"

Replace the paragraph beginning "DataFusion owns that one generic physical
projection" through "or second reader is added." with:

> DataFusion owns that one generic physical projection, decoder-filter, and
> statistics-pruning path without pretending Variant is exact Struct
> extraction. Variant owns path interpretation, standard layout requirements,
> missing/null/conversion semantics, and residual fallback.
>
> Oracle reads every Wyrd-written Parquet data file through one per-file read
> core: `FileReadPlan`, built on the facade above. Two source adapters feed it:
>
> ```text
> Hot objects
>   └─ hot source adapter (tenant footer, owned row groups) ─┐
>                                                            ├─ FileReadPlan → Parquet decoder
> Published files                                            │
>   └─ Iceberg snapshot/manifest planning ───────────────────┘
>        └─ Iceberg task semantics: deletes, field IDs,
>           defaults, partition constants, lineage metadata
> ```
>
> Oracle owns orchestration. Iceberg owns published-table state and task
> semantics: catalog, snapshots, manifests, `TableScan::plan_files()`,
> `FileScanTask`, manifest/file statistics pruning, delete association and
> application, field-ID and task schema transformation, defaults, partition
> constants, and v3 row lineage. `FileReadPlan` owns per-file projection,
> nested decoder filtering, row-group/page pruning, and Variant
> shredded/residual reconstruction. The published adapter applies Iceberg task
> semantics around the shared core; Oracle does not reimplement them. The
> pinned `ArrowReader` may remain internally only while its task semantics wrap
> or feed the shared core; no published Parquet decoding duplicates it.
> Byte-range splitting, the tenant-proving footer loader, and the follower
> `assigned ⊆ planned` check are unchanged and shared. Interactive,
> Analytical, leader, and follower paths therefore cannot drift. No Wyrd
> Variant-specific projector, filter, pruner, general read-planning
> abstraction, or second per-file decoder is added.

### REQ-022 — Shredded files are read correctly everywhere

Append:

> Within one query, a path may be shredded in some files and residual in
> others, with any mix of hot, promoted, and rewritten files; results are
> identical to an all-residual read.

### REQ-023 — Leaf projection

Replace "The one DataFusion per-file read-planning facade supplies both
readers with" with:

> Both Oracle source adapters use the same per-file read plan
> (`FileReadPlan`), which supplies

and append:

> The published adapter additionally preserves Iceberg snapshot, task,
> delete, field-identity, partition, and metadata semantics.

### REQ-024 — Leaf filtering and row-group skipping

Replace "On both read paths," with "For hot and published files alike,".

### INV-008 — One nested-field pushdown pipeline

Replace "through the one TASK-003-owned per-file read-planning facade used by
both Oracle readers." with:

> through one per-file read core (`FileReadPlan` on the TASK-003-owned
> facade) used by both Oracle source adapters. The published adapter keeps
> Iceberg snapshot, task, delete, field-identity, partition, and metadata
> semantics around that core. The Iceberg reader is not removed, and
> published files with deletes are not refused.

### AC-014 — One read core over hot and published files (new)

> Tests prove, before TASK-003 completes:
>
> 1. A Variant key shredded in some files and residual in others, at any
>    depth, filters and projects identically to an all-residual read; a key
>    no file shredded reads as null and never errors.
> 2. A committed Forge/Scribe compatibility test: Scribe-written shredded
>    files rewritten by Forge read identically through the shared core.
> 3. The journey `published::struct_and_variant_share_physical_pushdown`:
>    hot, promoted, and rewritten files answer Struct and Variant filters
>    with identical rows, and published scans report leaf projection and
>    row-group pruning through existing Oracle metrics.
> 4. Oracle position-delete coverage and Forge position/equality-delete
>    journeys stay green on the shared core.
> 5. Tenant-footer, byte-range, LIMIT, and follower-assignment checks stay
>    green.
> 6. Variant values and v3 row lineage survive repeated Forge rewrites.

### Revision history

Prepend:

> - **Revision 20 (2026-10-07, approved):** By explicit human direction after
>   independent review, states how the existing one-facade requirement is
>   met. Iceberg plans and governs published data; one shared Wyrd Parquet
>   read core (`FileReadPlan`) decodes every Wyrd-written data file; the
>   published adapter keeps Iceberg task semantics (deletes, field IDs,
>   defaults, partition constants, lineage) around that core. Removing the
>   Iceberg reader or refusing files with deletes was rejected. REQ-022,
>   REQ-023, REQ-024, INV-008, and the semantic-access section are reworded;
>   AC-014 adds the required proofs.
