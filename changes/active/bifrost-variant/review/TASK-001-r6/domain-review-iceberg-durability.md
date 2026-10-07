# Iceberg / Parquet durability domain review

## Result

**PASS — no material findings.**

The cumulative candidate keeps Bifrost fresh-v3-only, persists canonical
unshredded Variant columns through the pinned Iceberg/Parquet stack, validates
stored Variant cells before Oracle decoding, preserves nullable-Struct meaning
across Parquet, and carries hidden row lineage through Forge rewrite and v3
maintenance. The round-5 admission changes do not introduce a second durable
representation or an implicit migration path.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Round-5 remediation and evidence:
  `changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md`
- Pinned Iceberg fork: `e999331f280b698bcd026550812b5047e8789df6`
- Pinned compaction fork: `2b65fa189f2d05002acc6e59515a071a63777970`

The candidate commit and tree were reconfirmed before and after review. Other
round-6 reports were not read.

## Reviewed boundary and evidence

| Boundary | Evidence | Result |
|---|---|---|
| Iceberg v3 creation and refusal | `catalog/bifrost_catalog.rs:1047-1058` creates every shared physical table with `FormatVersion::V3`; `:1077-1106` rejects a loaded table whose format or physical schema differs. Built-ins and dynamic tables converge on this owner. | **PASS.** Non-v3 state is refused, not migrated or accepted as mixed-version state. |
| Variant physical type and Parquet promotion | `tables/fields.rs` and `wyrd-queue::variant_field` carry `arrow.parquet.variant` over the canonical `metadata`/`value` binary Struct; `parquet/promoted_object.rs:146-190` treats the Parquet Variant group as one Iceberg field while retaining the registered top-level field ID; pinned `iceberg-rust` maps that extension to `Type::Variant` and back to the unshredded extension layout. | **PASS.** Variant remains an atomic logical Iceberg field; its encoding children do not become logical schema fields. |
| Variant admission and stored-read validation | `EncodedVariant::from_bytes` in `wyrd-queue/src/variant.rs:230-246` applies bounded complete validation and the revision-13 numeric domain before an `EncodedVariant` can reach a writer. `mask_placeholders` at `:405-440` restores nulls hidden by nullable parent Structs and runs upstream full validation on every present stored cell before Oracle Variant functions decode it. The final addendum also bounds aliased-object traversal by encoded value length. | **PASS.** Acknowledged raw values are storage-safe; malformed persisted bytes fail a query instead of panicking or being normalized into another value. |
| Nullable Struct Parquet behavior | Revision 12 keeps children physically nullable where the parent is nullable. Round 5 adds `refuse_partial_structs` at `tables/mod.rs:296-334`, invoked only after exact schema and Variant checks at `:273-293`; Results, metric buckets, and optional gateway `resolved_model` wire into this seam. | **PASS.** Valid rows remain either complete-present or null-absent. Parquet retains child nulls for absent parents, while partial values are refused before durable work. No read-side value synthesis was added. |
| Pre-ACK / pre-WAL placement | `scribe/execution_lanes.rs:533-577` invokes the built-in owner before fingerprint comparison, correlation stamping, and field-ID stamping; `enforce_builtin_source_contract` calls the table validator at `:601-634`. Both IPC and native persistence preprocessing use this `decode_rows` transition. | **PASS.** Round-5 Struct and Variant refusals cannot leave a WAL record, staged Parquet row, promotion candidate, or ACK. |
| Gateway required/optional model distinction | `tables/gateway/calls.rs:94-120` declares non-null children for required `requested_model` and nullable children only for optional `resolved_model`; `WHOLE_STRUCTS` enforces all-or-none values for the latter. | **PASS.** The r5 correction restores the required durable shape without weakening the optional Parquet representation. |
| Row lineage through compaction | The pinned compaction fork projects Iceberg's reserved row-lineage field IDs into only the internal v3 scan/writer schema, decodes run-end constants, requires non-null `Int64` values per batch, and writes the same batch. Wyrd publishes only a successful existing five-field handoff. `managed_rewrite.rs` observes both hidden values after successive physical rewrites. | **PASS.** `_row_id` and `_last_updated_sequence_number` remain absent from logical schemas/fingerprints and unchanged for surviving rows; a failed rewrite produces no commit-capable handoff. |
| V3 manifest rewrite, snapshot expiry, and GC | `forge/gc.rs:223-270` uses the pinned Iceberg manifest rewrite for v3 rather than skipping it. The dependency preserves explicit per-file `first_row_id`; the Wyrd journey then expires replaced snapshots and verifies existing lineage before a fresh append. | **PASS.** Maintenance does not reassign surviving rows and the table's monotonic row-ID allocation remains owned by Iceberg. |
| Bloom sizing | `parquet/writer_properties.rs:114-150` enables only the resolved columns and sets the unchanged `0.01` FPP. parquet-rs 59.3 resolves unset NDV from `max_row_group_row_count` and folds the completed filter to the actual inserted cardinality. Scribe and Forge use this shared recipe. | **PASS.** There is no opening-batch estimate or duplicate Wyrd NDV policy; both writers use native row-group geometry. |
| Dependency identity | Root `Cargo.toml`/`Cargo.lock` resolve all Iceberg crates to `e999331f...`, compaction to `2b65fa18...`, and Variant crates to the Arrow/Parquet 59.3 line. Local pinned checkouts matched those revisions. | **PASS.** No moving branch, path override, or second storage implementation entered the candidate. |

## Round-5 persisted compatibility and fingerprint assessment

The round-5 value validators are admission policy, not schema. Adding
`WHOLE_STRUCTS` and `refuse_partial_structs`, and tightening raw Variant numeric
and encoding validation, changes neither accepted-row bytes nor any logical or
physical fingerprint for values that were already valid.

The gateway declaration correction is intentionally schema-significant:
restoring non-null children under required `requested_model` changes that
built-in's nested Arrow/Iceberg shape and its catalog fingerprint relative to
the defective round-4/round-5-reviewed candidate. Physical-table reconciliation
also compares nested nullability and therefore refuses that stale table. This
is correct for the approved unreleased, no-migration contract: the candidate
does not silently reuse incompatible persisted files, invent a compatibility
schema, or normalize old rows. Optional `resolved_model` retains nullable
children and its valid persisted representation is unchanged.

Variant extension metadata and the unshredded `metadata`/`value` storage
children remain excluded from the canonical logical Variant fingerprint; the
canonical built-in encoding commits the single `0x0d` Variant tag. Hidden
Iceberg lineage columns remain outside both catalog and canonical physical
fingerprints.

## Verification evidence and limits

Source was audited cumulatively from the base through the candidate, including
the two pinned fork diffs and the recorded round-5 implementation evidence.
Relevant recorded green proof includes:

- `variant::tests::raw_shared_field_values_are_refused` and the raw-IPC
  refusal journey;
- `tables::tests::partial_nullable_structs_are_refused` and
  `partial_metric_buckets_are_refused`;
- `verification_runtime::typed_builtin_payloads_are_queryable` and
  `builtin_variant_columns_are_refused_before_ack`;
- `arrow::schema::tests::variant_round_trips_unshredded` at the pinned
  Iceberg revision;
- `executor::datafusion::tests::row_lineage_is_complete` and
  `compaction::tests::rewrite_preserves_v3_row_lineage` at the pinned
  compaction revision;
- `forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite`; and
- `parquet::writer_properties::tests::bloom_capacity_uses_row_group_limit_for_scribe_and_forge`.

No Cargo, mise, codegen, PostgreSQL, or object-store command was rerun in this
parallel review, per orchestrator coordination. The conclusion therefore
depends on source inspection and the immutable task's recorded execution
evidence; no source-proven contradiction was found.

## Findings

No material findings.

## Overall status

**PASS**
