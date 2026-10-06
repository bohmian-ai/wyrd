# TASK-001 R6 reuse review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- Authority: approved `SPEC-bifrost-variant` revision 13, `TASK-001`, and remediation packets R1-R5. The R5 implementation-evidence section and its shared-object addendum were treated as claims and checked against source.
- Scope: the complete cumulative base-to-candidate diff, including added and materially changed types, functions, validators, conversions, renderers, schema mappings, error carriers, configuration, and test helpers. Review artifacts were not treated as implementation evidence. No other R6 report was read.

## Search and dependency coverage

I began with CodeGraph for the Variant owners, table validators, callers, and tests, then searched the base with `git grep` and the candidate with symbol/caller searches. I inspected the public APIs of the exact resolved dependencies rather than relying on similarly named repository code:

- `parquet-variant 59.3.0`: `Variant`, `VariantMetadata`, `VariantBuilder`, shallow/full validation, object/list offset iteration, and JSON rendering hooks.
- `parquet-variant-compute 59.3.0`: `VariantArray`, `VariantArrayBuilder`, `json_to_variant`, `variant_get`, `unshred_variant`, and `VariantType`.
- `parquet-variant-json 59.3.0`: `JsonToVariant`, `append_json`, and `VariantToJson`.
- Arrow/Parquet 59.3: extension metadata, JSON `EncoderFactory`, Arrow builders, and `WriterProperties` dynamic Bloom NDV resolution from `max_row_group_row_count`.
- DataFusion 55: `ScalarUDFImpl`, `ExprPlanner`, `SessionStateBuilder`, and the absence of a built-in semantic Parquet Variant SQL surface.
- iceberg-rust fork `e999331f280b698bcd026550812b5047e8789df6`: Variant schema conversion, format-v3 creation/validation, metadata columns, `first_row_id`, and v3 manifest behavior.
- iceberg-compaction fork `2b65fa189f2d05002acc6e59515a071a63777970`: native preservation of `_row_id` and `_last_updated_sequence_number` during v3 rewrites.

The latest `scan_encoded` change was checked specifically against the pinned Variant implementation. Upstream full validation recursively validates every object field starting at its offset and does not reject two fields that begin at the same child bytes. It therefore does not supply the candidate's linear-work admission rule. The candidate continues to use upstream shallow accessors and full validation for accepted values; its visited-node bound is the missing Wyrd trust-boundary constraint, not a second Variant decoder.

## Confirmed duplicates

None.

No added or materially changed candidate mechanism was found to be semantically equivalent to an existing repository owner or a pinned dependency API while satisfying the revision-13 Wyrd contract. The cumulative change removes several base-tree duplicates instead: the three Arrow/wire schema mappings converge on `wyrd_queue`, hand-maintained terminal-error projections are replaced by the shared problem reconstruction, audit projection reuses the staging hash owner, and the custom Bloom NDV heuristic is deleted in favor of Parquet's native sizing.

## Checked surfaces found not duplicated

- `DataTypeSpec::Variant`, Variant limits, stable error variants, schema artifacts, and terminal problem fields — searched `wyrd-spec`, generated schemas, proto, base terminal enums, and error catalogs; these extend the existing contract owners rather than create parallel contracts.
- `arrow_schema_to_fieldspec`, `field_to_spec`, `spec_to_field`, `is_extension_key`, and recursive Variant schema mapping — searched the base client mapper, server `bifrost/convert.rs`, and catalog `wire.rs`; the candidate deletes the two server-side twins and leaves `wyrd_queue` as the single mapping owner.
- `VariantViolation` and `VariantViolations` — searched `BifrostError`, queue errors, and dependency errors; the former is the table-independent local fact and the latter only retains first facts until the locked precedence decision, with no second public error envelope.
- `EncodedVariant::{from_json,from_json_text,from_bytes}` and `sized` — searched repository JSON encoders and the three Variant crates; upstream lacks Wyrd's size/depth/error-precedence/exact-i64-or-u64-decimal contract, while accepted bytes still finish through upstream validation.
- `append_raw`, `scan_numbers`, `raw_number_variant`, `narrow_integer`, and `JsonPointer` extensions — searched `serde_json::RawValue`, `parquet_variant_json::{JsonToVariant,append_json}`, and base JSON helpers; the dependency parser loses the required large-integer token semantics and has no Wyrd depth/error-priority rule.
- Iterative `scan_encoded`, `enter_container`, object-name check, Decimal16 domain check, and visited-node bound — searched `Variant::{try_new,with_full_validation}`, `VariantMetadata`, object/list fallible iterators, and the old recursive `check_depth`; the candidate replaces the old walker and wraps upstream validation only for Wyrd-specific limits, precedence, stack safety, exact numeric domain, and aliased-offset work bounding.
- `variant_bytes_to_json`, `variant_cell_to_json`, `mask_placeholders`, and `VariantJsonEncoderFactory` — searched Arrow JSON encoders and Variant JSON/compute APIs; each adapter delegates value rendering to `VariantToJson`/`VariantArray`, adding only Arrow-cell placeholder masking or the Arrow JSON extension hook that upstream does not install.
- `variant_storage_type`, `variant_field`, and `is_variant` — searched Arrow's `ExtensionType`, `VariantType`, Iceberg Variant conversion, and prior storage declarations; these centralize the exact Wyrd wire declaration and extension-identity rule.
- `VariantColumnBuilder` and `variant_column` — searched Arrow `BinaryBuilder` and dependency `VariantArrayBuilder`; the installed builder emits `BinaryView` children and does not stamp the required extension, whereas Wyrd needs canonical `Binary` storage from already validated `EncodedVariant` values.
- `VariantFailure`, OTLP `attributes_variant`/`any_value_variant`, last-key-wins object projection, `ResourceEnvelope`, `ScopeEnvelope`, `ExceptionPromotions`, and promoted-value helpers — searched the removed protobuf encoders and the trace/log/metric projectors; shared signal behavior is consolidated in `tables/signal.rs`, with signal-specific row assembly left in its owning projector.
- Entity-reference declarations and `entity_refs_column`, correlation extraction, and shared signal Arrow-column helpers — searched all three canonical signal implementations and Arrow builders; candidate code is the common owner used by traces, logs, and metrics rather than three copies.
- Built-in Variant/Struct declarations and projection changes for audit, agent traces, eval, verification, gateway, traces, logs, and metrics — searched every named producer and consumer; each table retains one schema owner and uses shared Variant/signal builders instead of a parallel storage model.
- `DomainTable::CANONICAL_VALIDATOR`, `validate_predeclared`, `validate_declared_variants`, and nested Variant walking — searched Gate/Scribe validation, canonical signal validation, fingerprints, and Arrow validation; the checks sit at the existing table-owned pre-ACK validator seam and reuse `EncodedVariant` rather than recreate byte validation.
- `DomainTable::WHOLE_STRUCTS` and `refuse_partial_structs` — searched Arrow `StructArray` validity, table-specific validators, gateway capture, verification result projection, and metric bucket validation; Arrow permits nullable children under nullable parents, so no native validator enforces the domain's whole-present/whole-absent rule. Results and gateway declare the shared check; metrics reuses the same helper from its existing custom validator.
- Separate `requested_model` and `RESOLVED_MODEL` declarations — searched the prior `model_ref_type`; the prior helper incorrectly forced one child-nullability shape on two distinct persisted contracts, so separate declarations remove rather than add semantic duplication.
- `OracleVariantSql`, `VariantOperatorPlanner`, `VariantGet`, `VariantAsText`, `ParseJson`, and `ToJson` — searched every production session constructor, DataFusion built-ins, and the Variant compute/JSON crates; the single session owner wraps installed kernels and Wyrd parsing semantics, and no second Oracle registry remains.
- `per_row_get`, `canonical_storage`, and `decode_rows` — searched `variant_get`, `VariantArrayBuilder`, and `unshred_variant`; these cover dynamic row paths and normalize kernel output to Wyrd's declared Binary storage, cases not provided by the literal-path kernel alone.
- `ORACLE_VARIANT_SQL_VERSION` propagation — searched plan/stage fingerprints, codecs, peers, followers, and workers; it extends the existing distributed compatibility digests instead of adding a parallel negotiation mechanism.
- `QueryCatalogError::{external,find}` and the changed DataFusion error sites — searched existing error-chain walkers and remote DataFusion transport; this is the sole local/distributed catalog carrier, replacing generic/string-only branches rather than competing with another typed carrier.
- `failed_terminal`, terminal `WyrdProblem`, tonic conversion, client `from_problem`, and server `terminal_error` — searched base `QueryTerminalErrorCode` mappings across Oracle, client, tonic, and server; the candidate deletes those parallel per-code tables and routes all surfaces through the existing `WyrdProblem`/`code_to_wyrd_error` owner.
- Queue-to-catalog conversion — searched the deleted client `queue_catalog_error`; conversion moves to `impl From<&WyrdQueueError> for WyrdError`, so callers share one owner.
- Rust/HTTP/MCP/CLI JSON result rendering — searched hand-written row renderers; the changed consumers install `VariantJsonEncoderFactory` or call the shared Variant decoder rather than introduce surface-local Variant semantics.
- Python `variant_to_python` and TypeScript native `variant_to_value` — searched SDK and native bindings; both are necessary foreign-runtime adapters and delegate decoding to `wyrd_queue::variant_bytes_to_json`, with no Python or TypeScript Variant implementation.
- Python/TypeScript declarations and error-code additions — searched generated/public SDK surfaces; they project the existing wire and native binding contracts and add no transport, validation, or storage owner.
- `bifrost_writer_properties*` and Bloom configuration — searched the removed `bloom_filter_ndv` helper and Parquet 59.3 properties; the candidate deletes the local sizing algorithm and relies on Parquet's native row-group-derived NDV for both Scribe and Forge.
- Iceberg format-v3 creation/validation and Forge/GC changes — searched the pinned Iceberg and compaction forks; candidate orchestration selects and verifies their native v3/lineage behavior instead of implementing row-lineage rewriting locally.
- Audit `entry_hash` visibility and audit Variant projection — searched staging and projection hash construction; the projection now calls the staging writer's existing hash owner, removing an independently restated preimage.
- Test fixtures for exact integers, malformed/deep/aliased Variant bytes, whole Structs, OTLP signals, terminal problems, distributed sessions, and repeated Forge rewrites — searched nearby test support and base fixtures; existing helpers were extended where shapes matched, while runtime- or encoding-specific helpers cover distinct shapes and do not leak a second production mechanism.
- Latest `AB_METADATA` and `shared_objects` journey helper plus the unit fixture — searched `nested_lists`, `EMPTY_METADATA`, `list`, and `nest`; none can express an object whose two field offsets alias the same child. The repetition across a crate-private unit test and the cross-crate server journey avoids exposing test-only encoding machinery as production API.
- Dependency and configuration changes — searched the workspace graph and lockfile; only the three already-authoritative Arrow Variant crates and the two required fork repins are added/changed, with no alternative JSON, Variant, SQL, or storage library.

## Proof limits

No Cargo, mise, codegen, or test command was started because the shared-checkout coordinator explicitly prohibited parallel Cargo work. This review used source, immutable diffs, exact resolved dependency/fork source, CodeGraph, base/candidate searches, and the command/results recorded in the R5 evidence. `git diff --check` for the immutable range was clean.

## Result

**PASS**

The cumulative candidate reuses or consolidates the applicable repository, standard-library, native-platform, and installed-dependency owners. No blocking semantic duplicate or parallel owner was found, and the inspected surface set is complete for the task's added and materially changed mechanisms.
