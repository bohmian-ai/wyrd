# TASK-001 r5 Reuse Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`
- Candidate tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`
- Latest remediation range: `bb6ae8070e276c20011e675ba1a804f0356e51ea..0e37748f3a27d3bcec4713e6210e97328e045886`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 12
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Prior review rounds read: `TASK-001-r1` through `TASK-001-r4`, including every verdict, validated ledger, and remediation task; r4's reuse report was treated as navigation, not as a conclusion.
- Pinned forks inspected: `iceberg-rust@e999331f280b698bcd026550812b5047e8789df6` and `iceberg-compaction@2b65fa189f2d05002acc6e59515a071a63777970`.

CodeGraph was used first. The full cumulative diff, the remediation diff, base-tree owners, current candidate callers, sibling TASK-002 source, installed Arrow/Parquet Variant/DataFusion APIs, and both pinned fork deltas were then compared. Candidate identity and tree were resolved before and after the review.

## Confirmed duplicates

### REUSE-R5-001 — TASK-001 adds a temporary raw-row conversion path beside TASK-002's existing prepared-input owner

- New location: `crates/shared/wyrd-queue/src/batch_builder.rs:43-48,117-139,205-259,369-385` (`BuiltRow` raw-field storage, `append_json_row`, `collect_raw`, and `build_variant_column`).
- Existing owner: sibling TASK-002 commit `0fcb8a7ac443e7f89bc538bb1a23bd007bb9221d`, `crates/shared/wyrd-queue/src/batch_builder.rs:48-74,174-208,359-380,428-460,635-655` (`RowPreflight`, `Cell`, `build_column`, and `build_variant`).
- Evidence that both do the same job: both retain each JSON field as `serde_json::value::RawValue`, normalize JSON `null` to an absent cell, preserve the original numeric token, and route a present Variant through `EncodedVariant::from_json_text` plus `VariantColumnBuilder`. The TASK-002 owner is the already implemented complete-input boundary for this exact row-authoring concern; TASK-001's smaller version is replaced wholesale when that sibling is integrated. The r4 remediation explicitly names TASK-002 authoring behavior as a non-goal.
- Reachability and unnecessary scope: the only new TASK-001 need is the pre-ACK numeric-versus-depth proof in `verification_runtime.rs:1652-1698`. That same fixture already owns `trace_frame` and raw-IPC `refusal` helpers at `:1714-1751` and `:1762+`, so it can exercise the server boundary without expanding the production row authoring path.
- Smallest consolidation: delete TASK-001's `RawValue`/`collect_raw` production changes and drive the compound refusal through the existing raw Arrow fixture. Leave exact-token row preparation to TASK-002's `RowPreflight`, its declared owner.
- Violated rule: `AGENTS.md` §15 requires reuse of the current owner and forbids adding a parallel mechanism for speculative or sibling-owned behavior.
- Classification: `VIOLATION` (blocking).

### REUSE-R5-002 — TASK-001 copies TASK-002's Scribe masked-null implementation after revision 12 removes its TASK-001 caller

- New location: `crates/vala/vala-bifrost-redux/src/scribe/fixed_ipc.rs:294-360` and `crates/vala/vala-bifrost-redux/src/scribe/material_plan.rs:368-395,432-442`, plus `fixed_ipc.rs:1774-1791`.
- Existing owner: sibling TASK-002 commit `535367c94dfad322ad36d950565a09b1d6ff7d2e`, in the same two Scribe files.
- Evidence that both do the same job: the patches from `535367c94^..535367c94` and `bb6ae8070..7c2c60f3e` for these files are byte-identical (same SHA-256), and the candidate, TASK-002 head, and `535367c94` resolve to the same final blobs: `b3df0c4d83f30223e73da95d6eab81cc3d5f26a1` for `fixed_ipc.rs` and `4262f0f3f01b28905d4f7ea17eed48f3d88acf4f` for `material_plan.rs`.
- Why TASK-001 no longer needs the copy: approved revision 12 changes every affected TASK-001 built-in Struct child to nullable, and the verification, metric-bucket, and gateway producers now emit nullable child slots. None requires a non-nullable child to carry a parent-masked null. The copied allowance remains needed by TASK-002's general nested-row authoring contract, where the implementation already lives.
- Smallest consolidation: delete the two Scribe production changes and their copied focused test from TASK-001; retain them only in TASK-002. TASK-001's nullable-child journeys continue to exercise the actual revision-12 contract.
- Violated rule: `AGENTS.md` §15 requires deletion of a mechanism whose current task has no caller and reuse of the sibling owner that already implements it.
- Classification: `VIOLATION` (blocking).

## Checked added or materially changed surfaces found not duplicated

| Surface checked | Existing owner / dependency searched | Result |
|---|---|---|
| Workspace dependency additions and pins | Base manifests/lockfile, Arrow/Parquet 59.3 graph, pinned fork manifests | Dependencies remain in the narrow consuming crates; no second Variant, Iceberg, or compaction dependency was added. |
| `DataTypeSpec::Variant`, Variant limits, schema artifacts, protobuf terminal problem, and catalog errors | Base `wyrd-spec` contract/error owners and generated schema/protobuf owners | Existing exhaustive contract owners were extended; no parallel public type or hand-written generated model remains. |
| Arrow/wire schema conversion | Base server `bifrost/convert.rs`, Redux `catalog/wire.rs`, current `wyrd_queue::schema` | Candidate consolidates conversion in `wyrd-queue`; the two former tables delegate to it. |
| `VariantViolation` and queue-to-catalog conversion | Base queue errors, client-local conversion removed in remediation, `BifrostError` catalog | One context-free violation becomes one catalog error only when field/row context exists. |
| `EncodedVariant::{from_json,from_json_text,from_bytes,sized}` | Base encoders; `parquet_variant_json::{JsonToVariant,append_json}`; `parquet_variant::Variant::{new,try_new}` | Installed APIs do not implement Wyrd's i64/u64-only integer contract, locked error precedence, duplicate-key-last pointer details, pre-validation depth cap, or size limit. Candidate delegates byte validity to upstream after its bounded policy checks. |
| Raw JSON token walker, numeric narrowing, and depth walk | Base-tree JSON walkers; `serde_json::RawValue`; Parquet Variant builder/accessors | One conversion owner serves JSON values, row input, OTLP values, and Oracle `parse_json`; no sibling numeric classifier or depth walker exists in the candidate. `check_depth` and `append_raw` act on encoded bytes versus input tokens and are not interchangeable. |
| Variant byte/cell rendering and Arrow JSON integration | `VariantToJson`, `VariantArray`, `variant_to_json`, Arrow JSON `EncoderFactory` | `variant_bytes_to_json`, `variant_cell_to_json`, and `VariantJsonEncoderFactory` cover distinct borrowed-byte, Arrow-cell, and row-writer boundaries and all delegate to upstream decoding. The dead per-instance renderer found in r4 is gone. |
| `mask_placeholders` | Arrow `StructArray::flatten`, `NullBuffer`, installed Variant kernels | `flatten` propagates a known parent validity bitmap; it cannot identify the legacy empty-byte placeholder after `get_field`/Parquet has discarded that parent. The candidate has one placeholder validator/masker shared by Oracle and JSON rendering. |
| Variant extension/schema helpers | Arrow `ExtensionType`/`VariantType`, base schema/fingerprint helpers | Small adapters centralize the exact Binary storage and exact empty extension metadata; no second extension predicate remains. |
| `VariantColumnBuilder` and `variant_column` | `parquet_variant_compute::VariantArrayBuilder`, all candidate producers | Upstream emits a different physical view and does not attach the exact Bifrost extension/schema. Candidate has one canonical column builder used across producers and SQL. |
| `BatchBuilder` non-Variant scalar paths | Base builder and standard Arrow arrays | Existing scalar/date/timestamp/fixed-binary construction remains the same owner. Only the raw Variant detour in `REUSE-R5-001` duplicates sibling work. |
| Predeclared table schema refusal and Variant validation | Base `DomainTable` validation, canonical signal validator, fingerprint fence | `refuse_undeclared` is shared by both built-in validators; value recursion reaches `EncodedVariant::from_bytes`. No table-name switch or per-table Variant validator remains. |
| Nullable verification Struct declarations and producers | `ResultsTable` schema owner, Arrow `StructArray::try_new`, `VariantColumnBuilder`, primitive Option arrays | Field-specific producers directly author their children. No reusable repository helper already owns mixed Variant/string/numeric summary construction; a new generic helper would add indirection. |
| Nullable metric bucket Struct declarations and producer | `CanonicalField`, shared `signal::{list_column,struct_column,i32_opt_column}`, Arrow `StructArray::flatten` | The projection reuses the existing shared column assemblers. `flatten` is a read/view operation and does not replace the persisted nullable leaf declarations or producer values. No duplicate helper was added. |
| Nullable gateway `resolved_model` Struct | Existing gateway schema owner and installed Arrow JSON `StructArrayDecoder` | Candidate changes only the declaration and reuses Arrow JSON's native nullable-parent decoding; it adds no second producer mechanism. |
| Other nullable-Struct candidates | All built-in `DataType::Struct`/`CanonicalType::Struct` declarations and their producers | List-element Structs are non-null elements and unaffected. Verification, metric buckets, and gateway model are the reachable nullable top-level Structs; no fourth copied producer branch was found. |
| OTLP `AnyValue`, attributes, resource/scope envelopes, entity refs, and semantic promotions | Base signal encoders and trace/log/metric projections | `tables::signal` remains the sole OTLP-to-Variant/common-envelope owner; per-signal modules contain only signal-specific row layout. |
| Verification, Eval, gateway, trace, log, metric, audit, and agent-trace Variant projections | Their existing table/service producers and shared Variant builder | Each durable producer uses the shared encoding owner; no legacy JSON-text/protobuf alternative remains in the cumulative candidate. |
| Audit retained-row hash proof | `vala-sql::audit_staging::entry_hash` | Journey uses the staging writer's exported hash owner; the prior hand-written preimage duplicate is gone. |
| Oracle registry, planner, UDFs, and every session installation | Base Oracle session constructors/codecs; DataFusion `ExprPlanner`/`ScalarUDF`; installed Variant kernels | One `OracleVariantSql` owner installs the same functions/planner everywhere. Dynamic paths are the required residual case, not a second registry. |
| Oracle `to_json`, `variant_as_text`, and `parse_json` | Shared queue encoder/renderer and installed compute/json APIs | `parse_json` calls `EncodedVariant::from_json_text`; rendering uses the shared upstream-backed value path. String-special `variant_as_text` is distinct public SQL behavior. |
| Distributed query error carrier and terminal problem reconstruction | Base error mapping; datafusion-distributed string carrier; `WyrdProblem`/client reconstruction | One tagged serialized catalog envelope covers all remote errors; no per-error parser/table remains. |
| Gate ingest error projection | Shared `wyrd_tonic::error::wyrd_error_to_status` | Gate keeps ingest-specific retry/class behavior but reuses the canonical problem producer. |
| Rust/Python/TypeScript query terminals | Shared Rust query result, raw-byte decoder, PyArrow and Arrow JS runtime boundaries | Rust owns durable decoding; Python and TypeScript only traverse runtime-native row shapes. No language-local Variant validator or transport implementation was added. |
| HTTP, MCP, CLI, and server row-as-JSON rendering | Arrow JSON writer plus `VariantJsonEncoderFactory` | All changed production JSON surfaces install the same encoder. CLI journey adds proof, not an encoder. |
| CLI Variant journey fixture | Existing compiled-binary journey/server helpers and Oracle `to_json` | Extended the existing harness; no second CLI client or fixture framework. |
| Python permission assertion | Existing `WyrdError.code` contract and sibling Python assertions | Changing message regex matching to `denied.value.code` reuses the public error attribute; no helper or error mapping was added. |
| Iceberg v3 create/validate/GC and Forge handoff | Existing `BifrostCatalog`, Forge worker/GC, Iceberg `FormatVersion` APIs | Existing owners were extended; no compatibility path, lineage DTO, or second handoff was introduced. |
| Bloom sizing | Existing writer-properties owner and Parquet native NDV folding | One row-group-capacity input feeds Scribe/Forge while Parquet owns folding; the earlier local NDV duplicate is gone. |
| `iceberg-rust` delta `97c32f6..e999331f` | Existing Arrow/Iceberg schema conversion and reader/writer APIs | Delta adds only the end-to-end unshredded Variant proof; no production mechanism duplicates Wyrd. |
| `iceberg-compaction` delta `380a4d0..2b65fa1` | Iceberg metadata-column constants/lookup, Arrow cast, compaction scan/writer | Final code reuses Iceberg's metadata-field lookup and existing compaction pipeline. Per-batch lineage validation and run-end decoding guard distinct dependency boundaries; the rewrite-wide duplicate scan was removed. |
| Cumulative Rust journey fixtures | Existing client/MCP/Oracle/OTLP/server/Forge test modules, `WyrdTestServer`, shared Variant helpers | Tests remain colocated and reuse production encoding/rendering. Apart from the raw-row route in `REUSE-R5-001`, no standalone harness or second server fixture was added. |
| Python/TypeScript journeys and declarations | Existing SDK integration suites, generated/native declarations, stock exporters | Runtime-specific traversal remains thin; durable conversion stays in Rust. |
| Architecture, task evidence, and schema documentation | Existing `architecture/bifrost-design.md`, active task packet, Bifrost schema guide | Existing authorities were updated in place; no competing design or compatibility document was added. |

## Overall result

**FAIL** — `REUSE-R5-001` and `REUSE-R5-002` are confirmed parallel mechanisms and therefore blocking `AGENTS.md` §15 violations. All other added or materially changed surfaces in the cumulative TASK-001 candidate, including the nullable-Struct producers, installed dependency APIs, candidate siblings, and both pinned fork deltas, were covered and found not duplicated.
