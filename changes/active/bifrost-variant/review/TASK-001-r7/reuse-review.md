# TASK-001 R7 reuse review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `6147cc617d81f2c03464043be698ab2565e9d745`
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`
- Approved authority: `changes/active/bifrost-variant/spec.md` revision 13 and `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Dependency revisions inspected: `iceberg-rust@e999331f280b698bcd026550812b5047e8789df6` and `iceberg-compaction@2b65fa189f2d05002acc6e59515a071a63777970`; both local checkouts matched these commits and were clean.

I reviewed the complete cumulative base-to-candidate diff, not only the R6 remediation commits. I began with CodeGraph for the Variant owners and callers, searched the base tree with `git grep`, searched the candidate for same-purpose mechanisms, and inspected the exact installed Arrow/Parquet Variant 59.3 APIs and pinned Iceberg fork deltas.

## Confirmed duplicates

### REUSE-R7-001 — accepted raw bytes are fully validated twice before byte rendering

- **New location:** `crates/shared/wyrd-queue/src/variant.rs:347-350`, `variant_bytes_to_json`.
- **Existing owner:** `crates/shared/wyrd-queue/src/variant.rs:269-284`, `EncodedVariant::validate`.
- **Evidence:** `variant_bytes_to_json` first calls `EncodedVariant::validate`; that gate calls `Variant::try_new(metadata, value)` for every accepted value, and the renderer immediately calls the identical `Variant::try_new(metadata, value)` again before `to_json_value`. In `parquet-variant 59.3.0`, `Variant::try_new` performs full recursive validation, so this is the same dependency validation twice in one hot SDK/terminal render path, not two different boundary checks. The TypeScript and Python SDK converters both call this renderer, as do shared signal tests.
- **Violation:** `AGENTS.md` section 15 requires reuse of the existing owner and one root-cause implementation; the R6 correction centralized the raw gate but discards the validated dependency value and recreates it in one consumer.
- **Smallest consolidation:** make the borrowing gate return the fully validated `Variant<'m, 'v>` it already constructs after the Wyrd scan, have `from_bytes` and `mask_placeholders` ignore that returned value, and have `variant_bytes_to_json` render it directly; while doing so, parse `VariantMetadata` once and pass the validated metadata through `scan_encoded` and `Variant::try_new_with_metadata` instead of validating the same metadata in both calls.
- **Deletion:** remove the second `Variant::try_new` from `variant_bytes_to_json`; do not add another renderer or public type.
- **Focused proof:** an accepted nested object rendered through `variant_bytes_to_json` remains exact, while the existing hostile-depth, aliased-offset, duplicate-name, non-finite, decimal-domain, and malformed-byte cases retain their current typed refusals.

## Checked surfaces found not duplicated

- `DataTypeSpec::Variant`, limits, error variants, proto/schema projections, and generated SDK declarations — the candidate extends the existing `wyrd-spec` and generated-contract owners; no second durable contract exists.
- Arrow/wire conversion (`arrow_schema_to_fieldspec`, `field_to_spec`, `spec_to_field`) — the cumulative change removes server/catalog copies and leaves `wyrd-queue` as the single recursive mapper.
- `VariantViolation` and private `VariantViolations` — the first is the table-independent failure fact and the second only retains error classes until the revision-13 precedence decision; neither duplicates the public error catalog.
- `EncodedVariant::{from_json,from_json_text,from_bytes}`, `append_raw`, `scan_numbers`, `raw_number_variant`, `JsonPointer`, and `narrow_integer` — `serde_json` and `parquet-variant-json` do not preserve all required integer-token, duplicate-key, depth, size, and error-priority semantics, while the candidate still delegates accepted encoding to `parquet-variant` builders.
- R6 `within_size_limit` and `sized` — the former consolidates the one size rule used by built and borrowed values; the latter only attaches owned bytes after that rule and is not a second check implementation.
- R6 `nests_past_limit` and `within_depth_limit` — these are the bounded pre-serialization guard and pruned copy needed because `serde_json::Value::to_string` recurses; the authoritative JSON walk remains `from_json_text`, so no second encoder was added.
- R6 `scan_encoded`, `object_field_slots`, object-name ordering, and raw numeric-domain checks — installed `parquet-variant` exposes shallow nodes but no public child byte ranges and permits aliased/overlapping field starts and duplicate resolved names in its unsorted-metadata path; the Wyrd scanner adds the missing canonicality and bounded-work rules while retaining upstream validation for accepted values.
- `variant_cell_to_json`, `mask_placeholders`, and `VariantJsonEncoderFactory` — these are distinct Arrow-cell and Arrow-JSON adapters over the same raw gate and upstream `VariantArray`/`VariantToJson` APIs, not surface-local Variant decoders.
- `variant_storage_type`, `variant_field`, `is_variant`, and `VariantColumnBuilder` — they centralize Wyrd's canonical Binary-child extension shape; the installed `VariantArrayBuilder` emits a different physical child type and does not stamp the required field contract.
- OTLP `attributes_variant`, `any_value_variant`, `finish_variant`, and last-key-wins object projection — traces, logs, and metrics share `tables/signal.rs`; R6 adds no signal-local NaN/Infinity normalization and lets the shared `EncodedVariant::from_bytes` gate reject the offending record.
- Built-in table schemas, projections, `validate_declared_variants`, and `refuse_partial_structs` — every table declares its schema once and uses the existing pre-ACK validator seam; whole-or-absent Struct validation is shared rather than copied into verification, gateway, and metrics producers.
- Oracle `OracleVariantSql`, planner, UDFs, storage normalization, and R6 deep/oversize tests — production sessions install one owner over DataFusion and the installed Variant kernels; the R6 change adds proof only, not another parser or registry.
- `QueryCatalogError`, final-frame problem transport, tonic conversion, and client problem reconstruction — the cumulative candidate replaces fragment matching and per-surface code tables with the existing `WyrdProblem`/catalog mapping path.
- Rust, HTTP, MCP, CLI, Python, and TypeScript Variant projection — Rust surfaces use the shared Arrow encoder or byte renderer; Python and TypeScript contain only foreign-runtime conversion adapters over `wyrd_queue`, not independent Variant implementations.
- Scribe Variant validation, Iceberg v3 creation, Forge rewrite/GC, row-lineage projection, and Bloom sizing — Wyrd delegates schema/lineage behavior to the pinned Iceberg forks and Parquet's row-group capacity rule; no Wyrd-side row-ID allocator, repair path, or Bloom heuristic remains.
- `iceberg-rust` fork delta `97c32f6..e999331f2` — it adds the end-to-end unshredded Variant round-trip test only and no production mechanism parallel to Wyrd.
- `iceberg-compaction` fork delta `380a4d0..2b65fa1` — it reuses Iceberg metadata-field constants/lookup and the existing scan/writer pipeline; run-end decoding and per-batch lineage completeness checks protect distinct dependency boundaries, and no Wyrd duplicate implements them.
- R6 unit helpers `metadata`, `object`, `nested_text`, `nested_value`, and `drop_nested` versus journey helpers `variant_metadata`, `variant_object`, `nested_lists`, and `primitive` — the small repetition is confined to two crates with incompatible error types and dependency directions; exposing malformed-encoding fixture builders as production API or adding a shared harness would create more mechanism than it deletes.
- R6 gateway `submit_partial_resolved_model` — the public client cannot produce the invalid Arrow Struct, so the direct peer fixture is the smallest proof of the existing Scribe trust boundary and does not create a production ingest path.
- Test setup for terminal errors, distributed sessions, canonical signals, hostile Variant bytes, gateway capture, and repeated rewrites — existing local helpers are extended where their boundary matches; no new general fixture framework or production test hook was added.
- Dependencies and configuration — the cumulative change uses the already-selected Arrow/Parquet Variant crates and immutable Iceberg revisions; no second JSON, Variant, SQL, storage, or lineage dependency entered the graph.

## Verification limits

This was a source and dependency reuse audit; I did not rerun Cargo or journey lanes. `git diff --check` on the immutable cumulative range reports the pre-existing trailing blank line in `TASK-001-r6/standards-review.md`, which is unrelated to the confirmed production duplicate.

## Result

**FAIL**

`REUSE-R7-001` is one confirmed same-path duplication in the shared renderer and blocks acceptance under the required reuse rule; no other semantic duplicate or parallel owner was found in the cumulative candidate.
