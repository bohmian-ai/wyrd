# TASK-001 Behavior Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `99c5871ec5ca664b9f54baa379b437ee09d66e95`
- Candidate tree: `80e6f539ff6f23a0b6e59d1e8ac317df5d58f1b0`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Prior review: `changes/active/bifrost-variant/review/TASK-001-r1/verdict.md`
- Remediation task: `changes/active/bifrost-variant/review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md`

The review covers the complete base-to-candidate range. The checked-out
candidate remained `99c5871ec5ca664b9f54baa379b437ee09d66e95` while this
report was prepared. No reviewed source was changed.

## Navigation and caller-to-result coverage

I traced the changed behavior through these owners and consumers:

- JSON text enters `EncodedVariant::from_json_text`, is validated as
  `RawValue`, and is recursively classified by `append_raw`,
  `raw_number_variant`, and the shared `integer_variant` rule before the
  encoded value reaches queue or query consumers.
- Raw Arrow built-in writes reach `decode_rows`; fingerprint/type precedence is
  followed by `BuiltinTableDefinition::validate_variants`, which checks nested
  extension identity and calls `EncodedVariant::from_bytes` before stamping,
  ACK, or WAL mutation. `ScribeError::ContractViolation`,
  `IngestError::ContractViolation`, and the existing `wyrd-error-bin` carrier
  preserve the catalogued refusal.
- Oracle `parse_json` raises `VariantQueryError(BifrostError)`. Local execution
  retains the typed source; distributed execution serializes the existing
  tagged `BifrostError` into the dependency's string carrier and the
  coordinator decodes only that representation.
- Forge publication no longer gates on optional `DataFile` metric maps. The
  pinned compaction revision `fb3a594b0a93d9f99b62a77084e94be96fb7fba7`
  projects the standard v3 lineage fields by field ID, validates their
  presence/type/null state per batch, and writes the batch unchanged. The
  production rewrite-wide row-ID collection/sort is absent.
- Parquet writer recipes enable Bloom filters and set FPP but leave NDV unset,
  so parquet-rs derives it from the native row-group row maximum.
- The active Bifrost design and schema guide describe the v3, Variant, SQL,
  promoted-column, Struct, and removed-`details` contracts. Rust, Python,
  TypeScript, MCP, and row-as-JSON consumers decode Variant values natively;
  the existing `QueryResult` documentation is attached to the exported class.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001, REQ-002, INV-003, AC-002: all Bifrost tables are Iceberg v3 and repeated Forge rewrites preserve hidden row lineage while v3 GC works | v3 catalog creation/validation and GC are in the cumulative diff; `forge/publication.rs` accepts standard Iceberg descriptors without requiring optional lineage metrics; pinned compaction `fb3a594` validates both hidden `Int64` columns per batch and copies the batch unchanged | Recorded V10 `forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite`, V13 `compaction::tests::rewrite_preserves_v3_row_lineage`, and `executor::datafusion::tests::row_lineage_is_complete` pass | PASS |
| Human decision for FIND-TASK-001-3: no production duplicate-ID scan or metrics gate | `ensure_row_lineage_evidence` and its optional metric-map prerequisite are deleted; the pinned fork deletes its rewrite-wide row-ID vector, merge, sort, and uniqueness check; no replacement setting or gate was added | Cumulative and pinned-fork diffs inspected; repeated-rewrite equality remains the required outcome proof | PASS |
| REQ-003, REQ-004, REQ-019, INV-001, INV-002: one exact Variant representation, limits, errors, fingerprint, and value meaning | `wyrd-queue::variant` owns encoding/decoding; tag/layout/constants and catalog errors are shared; integer tokens in `from_json_text` are parsed lexically as `i128`, narrowed to integer or Decimal16, or refused before any `f64` conversion | `variant::tests::json_text_classifies_integers_from_their_tokens` and `json_converts_under_the_variant_contract` rerun and pass; recorded V1 and Oracle exact-integer tests pass | PASS |
| Human decision for FIND-TASK-001-1: use local `RawValue` plus `i128`, not workspace `arbitrary_precision` | `from_json_text` uses `serde_json::value::RawValue`; only the already-used `raw_value` feature is requested; no `arbitrary_precision`, second parser, or second numeric model is present | Source inspection and focused exact-boundary tests | PASS |
| REQ-006 through REQ-011, INV-004, INV-005, AC-001, AC-003: every named built-in stores the required Variant/Struct shapes, promotions, and sensitivity and exposes native results | Canonical table ledgers, projections, verification/eval/gateway/agent-trace/audit producers, SDK terminals, HTTP/MCP rendering, and generated contracts are changed together | Recorded V1-V8 and V14-V16 pass, including typed built-in, three OTLP, Rust, Python, TypeScript, MCP, and codegen journeys | PASS |
| Server trust boundary repeats built-in Variant validation before admission and preserves exact stable errors | `decode_rows` validates the source fingerprint, then recursively validates every declared built-in Variant before stamping; missing/foreign extension identity maps to unsupported type and malformed/large/deep bytes retain their `BifrostError`; Gate carries problem details over its existing metadata header | Recorded `builtin_variant_columns_are_refused_before_ack` proves missing/foreign markers, invalid bytes, depth, size, precedence, no ACK, and no persisted row; focused table contract test rerun and passes | PASS |
| REQ-017, REQ-019, INV-004, INV-006, AC-005, AC-008: every Oracle session shares the Variant SQL surface, Struct remains `get_field`, sensitivity is checked before IO, and local/distributed failures have identical stable identity | `OracleVariantSql` is the single registration owner; `VariantQueryError` retains a typed local source and uses tagged `BifrostError` serde text only where the dependency exposes a string carrier; malformed/unrelated strings remain generic | `oracle::tests::variant_errors_keep_their_catalog_identity_locally_and_remotely` and `parse_json_keeps_exact_integers_and_refuses_the_rest` rerun and pass; recorded V9 session journey passes | PASS |
| REQ-005 and AC-009: both writers size Bloom filters from row-group geometry while preserving enablement, FPP, and folding | `writer_properties::recipe_builder` sets Bloom enablement and FPP only; no Wyrd NDV constant or setter remains | `bloom_capacity_uses_row_group_limit_for_scribe_and_forge` rerun and passes; recorded V11 passes | PASS |
| FIND-TASK-001-4: active architecture and supported-schema documentation match shipped behavior | `architecture/bifrost-design.md` documents v3 lineage, Variant storage/admission and SQL; `docs/.../bifrost/schema.svx` lists the actual built-in Variant/Struct/promoted shapes and states that verification `details` is absent | Recorded `mise run docs:check` and `mise run check:docs` pass; source cross-check against owning declarations | PASS |
| FIND-TASK-001-6: invalid `EncodedVariant` state is not publicly constructible | `sized` is private, `is_empty` is deleted, and the required size accessor is named `encoded_bytes`; public construction remains limited to `from_json`, `from_json_text`, and `from_bytes` | Source/caller inspection and focused `wyrd-queue` tests | PASS |
| FIND-TASK-001-7: public `QueryResult` documentation remains attached to its class | The existing JSDoc immediately precedes exported `QueryResult`; generated declarations remain generated | Recorded `mise run ts:typecheck` passes | PASS |
| Repository behavior constraints: no client-tier durable implementation, PyO3 leakage, generated-contract drift, or unstable public error split | Durable validation/query/storage behavior remains in Rust owners; client surfaces project it; `wyrd-spec` stays IO/PyO3-free; stable errors use the shared catalog | Recorded `codegen:check`, `check:client-tier`, `check:pyo3-scope`, `check:unwrap-audit`, format, lint, Python, and TypeScript checks pass | PASS |
| Preserved security and durability boundaries: permission before IO, tenant tripwires, audit hash inputs, fixed trace IDs, Forge five-field handoff, recovery identity | The cumulative diff routes Variant behavior through existing owners without changing these boundaries; task journeys cover authorization and retained data | Recorded Oracle, built-in, OTLP, Forge, and SDK journeys pass | PASS |
| Non-goals: no shredding policy, user-model inference, second reader/model, DataFusion repin, migration, compatibility alias, signing, dynamic-table authoring, or new public option/protocol | Diff and manifests contain none of these; Task 2 remains the owner of dynamic-table and Python/TypeScript authoring behavior | Cumulative diff and dependency inspection | PASS |
| Standing DRIFT direction: no bespoke mechanism/check/file/setting/option lacking both repository and widely used project precedent | Remediation deletes the optional Iceberg metrics gate, duplicate-ID production scan, prose error grammar, public invalid constructor, and local Bloom NDV duplication; retained mechanisms are serde `RawValue`, standard tagged serde, standard Iceberg v3 projection, and parquet-rs defaults | Cumulative remediation diff and pinned dependency diff inspected | PASS |

## Prior-finding closure

| Stable finding | Closure |
|---|---|
| `FIND-TASK-001-1` | Closed by exact lexical integer classification in `EncodedVariant::from_json_text` using the approved `RawValue` plus `i128` approach. |
| `FIND-TASK-001-2` | Closed by recursive built-in admission validation and typed Scribe/Gate propagation before ACK/WAL. |
| `FIND-TASK-001-3` | Closed by deleting the optional-metrics gate and production rewrite-wide duplicate scan while retaining standard v3 projection, per-batch validation, unchanged copy, and equality proof. |
| `FIND-TASK-001-4` | Closed by updates to the two existing authoritative documents. |
| `FIND-TASK-001-5` | Closed by tagged serialization of the existing `BifrostError` over the existing carrier; prose parsing is absent. |
| `FIND-TASK-001-6` | Closed by private size-only construction and deletion of the impossible emptiness API. |
| `FIND-TASK-001-7` | Closed by reattaching the existing JSDoc to `QueryResult`. |
| `FIND-TASK-001-8` | Closed by native parquet-rs NDV derivation. |
| `FIND-TASK-001-9` | Behavior-relevant changed Rust items inspected carry substantive owner-local documentation; recorded lint audit passes. |
| `FIND-TASK-001-10` | Changed behavior paths use module-level imports and bare signature types; recorded lint/check evidence passes. |

## Proposed findings

None. I found no reachable task behavior that is missing, incorrect, drifted,
in violation, or regressed after the remediation.

## Verification performed in this review

- `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..99c5871ec5ca664b9f54baa379b437ee09d66e95` — PASS.
- Focused `wyrd-queue` exact-number and Variant contract tests — 2 PASS.
- Focused `vala-bifrost-redux` table contract, Oracle exact-number, distributed
  error identity, and Bloom recipe tests — 4 PASS.
- The candidate's recorded implementation evidence supplies the Postgres,
  cross-language, MCP, Forge, fork, docs, codegen, format, lint, and boundary
  results listed in the matrix; no contradictory result was found.

## Overall result

**PASS**
