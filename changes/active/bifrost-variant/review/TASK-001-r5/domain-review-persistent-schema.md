# Persistent schema, Parquet, and Iceberg durability domain review

## Subject and reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`
- Candidate tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`
- Remediation range: `bb6ae8070..0e37748f3a27d3bcec4713e6210e97328e045886`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 12
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Pinned forks: `iceberg-rust@e999331f280b698bcd026550812b5047e8789df6` and `iceberg-compaction@2b65fa189f2d05002acc6e59515a071a63777970`

This review covers persisted Arrow/Parquet/Iceberg schemas, nullable Struct
children and their producers, Scribe IPC validation, hot and published query
semantics, Iceberg v3 creation and validation, hidden row lineage, Forge
rewrite/maintenance/GC, and restart/recovery compatibility.

The candidate and checked-out tree matched throughout this review.

## Authority and source coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| No-migration premise and reachable deployment state | `spec.md:32,148-154,600-605`; `architecture/operations/deployment-and-release.md:179-216`; `catalog/bifrost_catalog.rs:996-1107` | **PASS.** The approved spec says Bifrost has not shipped and migrates no existing table, file, or data. Repository deployment authority independently says no Wyrd image has been published. Fresh registration creates v3, while any existing non-v3 or schema-mismatched local/dev table fails closed. A migration or mixed-schema reader is therefore neither required nor permitted for this task. |
| Revision-12 verification schema and producers | `spec.md:228-267`; `tables/verification/results.rs:38-109`; `wyrd-server/src/verification/results.rs:691-746`; `verification_runtime.rs:1045-1137` | **PASS.** Every `drift_report` and `eval_summary` child is nullable; absent summaries emit null child slots, while present summaries fill every child. The real-server journey queries every child through `get_field` before and after flush/publication, including Variant JSON null behavior. |
| Metrics bucket schema and projection | `tables/metrics/points.rs:24-62,98-125`; `tables/metrics/projection.rs:1080-1192,1209-1255`; `metrics_export.rs:834-855` | **PASS.** Optional positive/negative bucket Structs have optional `offset` and `bucket_counts` leaves, and the projector writes child nulls when the Struct is absent. The published metrics journey queries all four child fields and requires SQL null. |
| Gateway model schema and producer | `tables/gateway/calls.rs:32-46,99-132`; `wyrd-server/src/components/gateway/capture.rs:1426-1447`; remediation authority `TASK-001-R4-close-final-variant-contract-gaps.md:300-312` | **FAIL.** The requested correction targets optional `resolved_model`, but the shared `model_ref_type()` also makes the required `requested_model` children optional. See `PERSIST-R5-001`. |
| Native IPC acceptance and recovery encoding | `scribe/material_plan.rs:156-195,353-445`; `scribe/fixed_ipc.rs:270-360`; Arrow 59.3 validation behavior referenced by the remediation diagnosis | **PASS.** Raw IPC preflight permits a required child null count only when a nullable Struct ancestor can mask rows and no List changes the position space. Arrow decode remains the exact bitmap-containment validator before durable materialization, and `FixedIpcPlan` independently rejects an unmasked required-child null while preserving masked rows during WAL/recovery encoding. |
| Variant JSON rendering of masked/null storage | `wyrd-queue/src/variant.rs:280-345`; CLI/MCP/client encoder installation sites | **PASS.** `VariantJsonEncoderFactory` passes the placeholder-masked storage array's own logical nulls to `NullableEncoder`; it no longer renders an empty placeholder from under a null parent as a present Variant. The same encoder owner serves the JSON surfaces. |
| Arrow -> Parquet -> Iceberg schema durability | `catalog/bifrost_catalog.rs:996-1107,1509-1528`; `tables::iceberg_schema_for`; pinned Iceberg Arrow schema conversion and `arrow::schema::tests::variant_round_trips_unshredded` | **PASS except for the gateway contract above.** Catalog creation derives Iceberg field identity and optionality from the canonical Arrow schema, and reconciliation compares nested names/types/nullability. Optional children therefore become OPTIONAL Parquet/Iceberg fields and retain their own nulls on published reads. |
| Iceberg v3 lineage, Forge rewrite, and recovery | `forge` promotion/publication/worker paths; `tests/integration/forge/managed_rewrite.rs:1102-1561`; pinned compaction `executor/datafusion/mod.rs:251-438`; pinned fork row-lineage and manifest-list owners | **PASS.** Fresh tables are v3, native Iceberg append assigns row ranges, the compaction fork projects and validates both reserved lineage columns before writing the unchanged batch, and Forge receives only the established five-field handoff. The integration scenario follows built-in and user rows through two rewrites, manifest rewrite, snapshot expiry, and a post-GC append. |
| v3 maintenance and garbage collection | `forge/gc.rs:213-341`; pinned `iceberg-rust` manifest rewrite/snapshot expiry implementation | **PASS.** Forge no longer skips v3; existing leases, cancellation, catalog timeout, reconciliation, and object-root rules remain in force. Manifest rewrite preserves assigned `first_row_id` evidence, and failures stay on the existing per-table maintenance error path. |

## End-to-end durability trace

1. Table declarations produce the canonical Arrow schema. Fresh registration
   converts it to Iceberg with stable field identities and exact nullability;
   format v3 is mandatory.
2. Producers create nullable child arrays for absent verification summaries and
   metric bucket Structs. Gateway capture also creates null children for an
   absent `resolved_model`.
3. Gate/Scribe validate the incoming schema and values before ACK/WAL. Native
   preflight performs bounded structural checks; Arrow decoding proves exact
   ancestor-mask containment; `FixedIpcPlan` repeats the invariant when writing
   recovery IPC.
4. Scribe writes Parquet using the registered logical schema. Optional leaves
   retain their child validity through Parquet, avoiding the required-leaf
   padding that caused the prior published-row leak.
5. Promotion appends the exact Scribe `DataFile` evidence to the v3 table.
   Iceberg allocates row lineage. Forge scans the hidden lineage columns,
   validates them per batch, writes them unchanged, and publishes through the
   existing fenced handoff and catalog transaction.
6. Oracle reads hot files and snapshot-pinned Iceberg files. DataFusion
   `get_field` exposes the child array directly, so correctness depends on the
   child leaf carrying its own null; the revised verification and metrics
   declarations/producers do so. Variant JSON rendering uses the masked
   storage nulls.
7. On interruption, Scribe restores exact-schema staged runs from WAL; Forge
   has no local durable state and reconciles attempts/publication from its
   durable task, object, and catalog evidence. Schema or lineage mismatch fails
   the affected table/attempt closed rather than fabricating recovery state.

## Material finding

### PERSIST-R5-001 — REGRESSION: fixing optional `resolved_model` also makes required `requested_model` children nullable

- **Violated obligation:** The approved remediation changes
  `vala.gateway.calls.resolved_model` so its children survive Parquet as SQL
  null when the optional Struct is absent. It also requires preserved public
  logical schemas and no unrelated schema drift. The required
  `requested_model` has no absent-parent case and its provider/model members
  remain required durable facts.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/tables/gateway/calls.rs:32-46,111-112`.
- **Evidence:** `model_ref_type()` now returns nullable `provider` and `model`
  children and is reused by both `requested_model` (non-null parent) and
  `resolved_model` (nullable parent). The remediation authority at
  `TASK-001-R4-close-final-variant-contract-gaps.md:308-312` names only
  `resolved_model`; the null-padding defect exists only beneath that nullable
  parent. `calls_table_is_the_payload_contract_plus_the_managed_envelope`
  asserts only top-level nullability, and
  `unresolved_call_nulls_resolved_model_children` proves the optional side but
  does not detect the required side's weakening.
- **Observable consequence:** The registered Iceberg schema now declares
  `requested_model.provider` and `requested_model.model` OPTIONAL, so the
  durable contract no longer rejects a partial requested model at the schema
  boundary. Such a row can persist and later query with a null provider or
  model even though capture treats the requested model as a complete required
  `ModelRef`. The same shared helper also changes the logical fingerprint and
  Parquet leaf repetition for an unrelated required Struct.
- **Required testable correction:** Give `requested_model` a Struct whose two
  children remain non-null, while `resolved_model` alone uses nullable
  children. Keep the existing producer and Iceberg conversion owners; add no
  normalization layer or compatibility path. Extend the existing gateway table
  contract test to assert nested child nullability for both fields, and retain
  `unresolved_call_nulls_resolved_model_children` as proof of the optional
  producer behavior.

## Verification assessment

Independently run on the immutable candidate:

- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=tables::tests::variant_contract_and_builtin_schemas_are_stable) | test(=scribe::fixed_ipc::tests::masked_required_struct_child_null_roundtrips)'` — **PASS**, 2 tests.
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::results::tests::unscored_drift_writes_only_the_summary) | test(=components::gateway::capture::tests::unresolved_call_nulls_resolved_model_children)'` — **PASS**, 2 tests.
- `mise exec -- cargo nextest run --locked --manifest-path /home/thorrester/Documents/GitHub/iceberg-rust/Cargo.toml -p iceberg --lib -E 'test(=arrow::schema::tests::variant_round_trips_unshredded)'` at `e999331f280b698bcd026550812b5047e8789df6` — **PASS**, 1 test.
- `mise exec -- cargo nextest run --locked --manifest-path /home/thorrester/Documents/GitHub/iceberg-compaction/Cargo.toml -p iceberg-compaction-core --lib -E 'test(=compaction::tests::rewrite_preserves_v3_row_lineage)'` at `2b65fa189f2d05002acc6e59515a071a63777970` — **PASS**, 1 test.
- `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..0e37748f3a27d3bcec4713e6210e97328e045886` — **PASS**.

I did not rerun the PostgreSQL/object-store `typed_builtin_payloads_are_queryable`,
metrics, or repeated-Forge-rewrite journeys in this parallel domain pass. Their
source assertions and the task's final-candidate evidence were inspected. The
focused tests above do not close `PERSIST-R5-001` because none asserts the
nested nullability of `requested_model`.

## Overall result

**FAIL**

Revision-12 verification summaries, metric buckets, Scribe masked-null
handling, Parquet/Iceberg optional-leaf durability, v3 lineage, Forge recovery,
and GC are coherent. One bounded persisted-schema regression remains: the
`resolved_model` fix weakens the unrelated required `requested_model` child
contract.
