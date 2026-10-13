# Iceberg v3 durability domain review

## Review findings

No material findings.

The r4 dependency change is correctly placed at the existing compaction scan
projection boundary. It replaces the fork's private row-lineage name matcher
with `iceberg::metadata_columns::get_metadata_field_id`, then filters the result
back to exactly `RESERVED_FIELD_ID_ROW_ID` and
`RESERVED_FIELD_ID_LAST_UPDATED_SEQUENCE_NUMBER`. It does not widen the
projection to other metadata columns and does not add another lineage,
duplicate-detection, or metrics mechanism.

## Subject and reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `a6429060fb011aafa4335f2f736c70adab231739`
- Candidate tree: `64177d67141993ec18e63b43dc227dbc31d70950`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Pinned Iceberg fork: `e999331f280b698bcd026550812b5047e8789df6`
- Pinned compaction fork: `2b65fa189f2d05002acc6e59515a071a63777970`

The checkout `HEAD` was not the immutable candidate, so every repository source
claim was read from `git show a642906...` or the base-to-candidate diff. Both
dependency checkouts resolved exactly to the candidate pins above.

Reviewed boundary: v3 table creation and refusal; native `first_row_id`
allocation; `_row_id` and `_last_updated_sequence_number` resolution, internal
transport, validation, and unchanged rewrite; five-field handoff; atomic commit
and recovery identity; v3 manifest/snapshot/orphan maintenance; logical schema
and fingerprint separation; unshredded Variant Arrow/Iceberg mapping; and
row-group Bloom sizing/folding.

## Authority and source coverage

| Boundary | Authority and source evidence | Result |
|---|---|---|
| V3-only creation and refusal | Spec REQ-001/AC-002; `architecture/bifrost-design.md:124-142`; candidate `catalog/bifrost_catalog.rs:996-1060,1062-1163` | **PASS.** The shared physical-table owner creates `FormatVersion::V3`; registration and reconciliation refuse any existing non-v3 table. Built-in and user tables share this owner, with no migration or capability switch. |
| Append row identity | Spec REQ-002; candidate Forge Scribe promotion; pinned Iceberg transaction, snapshot, manifest-list, manifest inheritance, and reader code | **PASS.** Promotion uses the native Iceberg append transaction. The pinned library owns `next-row-id`, snapshot/manifest `first_row_id`, per-file inheritance, and retry reassignment. Wyrd adds no allocator or repair mechanism. |
| R4 metadata-column lookup | Pinned compaction `core/src/executor/datafusion/iceberg_file_task_scan.rs:188-223`; pinned Iceberg `crates/iceberg/src/metadata_columns.rs:426-460` | **PASS.** Logical fields resolve from the table schema first. Only a missing logical field may resolve through the Iceberg metadata owner, and the subsequent reserved-ID filter admits only the two v3 lineage fields. The change deletes the parallel string matcher without changing projected IDs. |
| Hidden lineage through repeated rewrites | Spec REQ-002/INV-003/AC-002; pinned compaction `datafusion_processor.rs:870-929`, `iceberg_file_task_scan.rs:188-223,657-695`, and `executor/datafusion/mod.rs:404-438`; pinned Iceberg row-lineage reader pipeline | **PASS.** V3 planning adds the two reserved fields only to internal scan/writer schemas. The Iceberg reader synthesizes inherited values or reads the physical values; run-end constants are decoded; every output batch must contain non-null `Int64` lineage; the same batch is then written unchanged. Hidden fields never enter the table schema. |
| Validation, publication, and recovery atomicity | Candidate `forge/managed/executor.rs:239-336`, `forge/managed/handoff.rs:13-108`, `forge/publication.rs:353-462,1291-1320`, and worker publication/reconciliation paths | **PASS.** The managed core has no catalog authority. Any scan, lineage validation, or writer failure returns no successful handoff; possible objects remain named by the attempt output ledger. Only a successful five-field handoff can derive a fenced replacement request. Removal and addition occur in one Iceberg transaction; definite conflicts revalidate the same identity, while ambiguous submissions reconcile without recommit. |
| Five-field handoff | Spec locked handoff and candidate `forge/managed/handoff.rs:13-108,133-186` | **PASS.** The handoff remains exactly `base_snapshot_id`, three consumed-file vectors, and `output_data_files`. Row lineage stays in output Parquet/DataFile evidence and does not create a sixth public or recovery field. |
| No duplicate/metrics mechanism | Binding review decision; cumulative Wyrd source and both pinned forks | **PASS.** Production and required proofs contain no rewrite-wide row-ID collection/sort/uniqueness gate and no requirement that optional lineage entries exist in `value_counts`, `null_value_counts`, bounds, or another metrics map. Existing path-identity duplicate checks remain correctly scoped to files, not row lineage. |
| Repeated rewrite and v3 maintenance proof | Candidate `tests/integration/forge/managed_rewrite.rs:1102-1270,1420-1562`; pinned compaction `core/src/compaction/mod.rs:3289-3405`; candidate `forge/gc.rs:223-341` and expiry/cleanup paths | **PASS.** The repository journey follows logical row plus both lineage values through two rewrite outputs, manifest rewrite, snapshot expiry, and a later append. The fork test mixes inherited append lineage with physically copied rewrite lineage. V3 manifest rewrite is enabled; existing active-read, attempt, handoff, lease, and orphan roots remain in force. |
| Schema/fingerprint separation and Variant | Spec REQ-003/INV-003; candidate `tables/mod.rs:414-540,787-883`; pinned Iceberg `arrow/schema.rs` Variant conversions and `arrow::schema::tests::variant_round_trips_unshredded` | **PASS.** Metadata columns never enter logical Arrow/Iceberg schema identity. Variant fingerprints use the logical `0x0d` tag and exclude extension/storage children; the pinned mapping converts the canonical Arrow extension to Iceberg Variant and returns the unshredded `metadata`/`value` extension layout. |
| Bloom row-group geometry | Spec REQ-005/AC-009; candidate `parquet/writer_properties.rs:10-151,303-333`; parquet 59.3 `file/properties.rs:1756-1770` and `column/writer/encoder.rs:249` / `arrow/arrow_writer/byte_array.rs:439` | **PASS.** Scribe and Forge set only enablement and unchanged FPP. Parquet derives the missing NDV from `max_row_group_row_count`, and its native writer folds the completed filter to the target FPP after observing actual values. No Wyrd NDV estimate or duplicate folding implementation remains. |
| Dependency identity | Candidate `Cargo.toml:225-247` and `Cargo.lock` | **PASS.** Every Iceberg crate resolves to immutable revision `e999331f...`; compaction resolves to immutable revision `2b65fa1...`; no path patch or moving branch is present. |

## Verification notes and limits

Independently run against the exact candidate pins:

```text
mise exec -- cargo nextest run --locked \
  --manifest-path /home/thorrester/Documents/GitHub/iceberg-compaction/Cargo.toml \
  -p iceberg-compaction-core --lib \
  -E 'test(=executor::datafusion::tests::row_lineage_is_complete) | test(=compaction::tests::rewrite_preserves_v3_row_lineage)'
```

Result: **PASS**, 2 run, 151 skipped.

```text
mise exec -- cargo nextest run --locked \
  --manifest-path /home/thorrester/Documents/GitHub/iceberg-rust/Cargo.toml \
  -p iceberg --lib \
  -E 'test(=arrow::schema::tests::variant_round_trips_unshredded)'
```

Result: **PASS**, 1 run, 1,779 skipped. The fork emitted existing compiler
warnings unrelated to this boundary.

```text
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  --features test-support \
  -E 'test(=parquet::writer_properties::tests::bloom_capacity_uses_row_group_limit_for_scribe_and_forge)'
```

Result: **PASS**, 1 run, 840 skipped.

`git diff --check 80b33286... a642906...` also passed.

I did not rerun the Postgres/object-store journey
`forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite` in this
domain pass. The candidate's recorded task evidence says it passed before the
r4 repin, and source inspection plus the independently rerun pinned-core test
cover the only behavior changed by that repin. The task evidence table at
`TASK-001-variant-storage-and-query.md:256` still names the older compaction
revision `94db7b94...`; that record is stale, but the manifest/lock and the
independent command above establish the reviewed revision as `2b65fa1...`.
This is an evidence-bookkeeping limit, not a durability finding.

## Overall result

**PASS**

The candidate satisfies TASK-001's Iceberg v3 creation, standard lineage,
Forge publication/recovery, v3 maintenance, logical-schema separation,
unshredded Variant, and native Bloom-sizing obligations. The r4 metadata lookup
removes a duplicate mapping and preserves the exact prior lineage behavior.
