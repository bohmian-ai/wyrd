# SPEC-bifrost-variant revision 24: nested-field benchmark shape

Status: approved by the user on 2026-10-07 ("I approve. Continue"). The user chose "Match
fork bench" when asked.

Kept apart from `spec.md` for the same reason as revisions 20–23. It is
applied after them (frontmatter `revision: 24`).

## Why

The spec says the benchmark follows the published Logfire/DataFusion shape
with `262_144` rows and `8_192`-byte strings. The published benchmark in the
pinned DataFusion fork, `datafusion/core/benches/parquet_struct_projection.rs`,
uses 512 rows (two 256-row batches, 256-row row groups) and 16 KiB strings.
The reference medians in the spec (about 400–700 µs) belong to that 512-row
workload. At 262,144 rows each string sibling is about 2 GiB and every case
runs about 500 times longer, so the reference table would not be comparable.

## Changes to `spec.md`

### Nested-field performance evidence

Replace "`262_144` rows, `8_192`-byte string siblings" with:

> 512 rows written as two 256-row batches into 256-row row groups, and
> 16 KiB string siblings, exactly as the pinned DataFusion fork's
> `parquet_struct_projection` benchmark,

The rest of the section is unchanged: values still vary per row, and
`select_small_field` and `select_inner_small_field` are the projection-only
cases revision 22 requires.

### AC wording that names the row count or string size

Replace `262,144 rows, 8 KiB string` with `512 rows, 16 KiB string` wherever
the revision history or acceptance text names the benchmark workload.

### Revision history

Prepend:

> - **Revision 24 (2026-10-07, approved):** The nested-field benchmark uses
>   the pinned DataFusion fork's published workload (512 rows, 16 KiB
>   strings) so the reference medians are comparable.
