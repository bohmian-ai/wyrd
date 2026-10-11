# SPEC-bifrost-variant revision 25: hot files read through DataFusion's ParquetSource

Status: approved by the user on 2026-10-07 ("Approve"), with
`tasks/TASK-003-revision-query-latency.md`.

Kept apart from `spec.md` for the same reason as revisions 20–24. It is
applied after them (frontmatter `revision: 25`).

## Why

Oracle's own hot-file scan, `HotParquetExec`, re-implements what DataFusion's
`DataSourceExec` + `ParquetSource` already do: partitioning, row-group and
Bloom pruning, and projection. It is also slower. The rule for Oracle is new
code only for Wyrd-specific implementation and for extending DataFusion,
such as Variant shredding. So hot files move to DataFusion's scan, with
Oracle's memory accounting, tenant proof and staged lease in a custom reader
factory.

## Changes to `spec.md`

### One read core

Replace "`HotParquetExec` and the pinned Iceberg `ArrowReader` call the exact
same facade" with:

> DataFusion's `ParquetSource` for hot files and the pinned Iceberg
> `ArrowReader` for published files call the exact same facade

The non-goal "Moving the published read path to DataFusion's `ParquetSource`"
is unchanged.

### Revision history

Prepend:

> - **Revision 25 (2026-10-07, approved):** Hot files are read by DataFusion's
>   `DataSourceExec` + `ParquetSource` with an Oracle reader factory for
>   memory accounting, tenant proof and the staged lease. Published files stay
>   on the pinned Iceberg `ArrowReader`.
