# TASK-001 Findings Validation

## Immutable subject

- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `3cf911fce699bbfe197f8b95e72b13e2f551f766`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Candidate stability: confirmed at the start and end of this validation; `HEAD` remained the candidate commit.
- CodeGraph: unavailable because this repository has no `.codegraph/` directory. Source and caller tracing used the cumulative Git diff, `rg`, Cargo metadata, and the pinned dependency checkouts.

## Validation coverage

This validation read the complete cumulative diff and every discovery or follow-up report in this review directory: behavior, invariants, standards, maintainer, system, all five routed domain reports, and the follow-up review. It also checked the repository authorities, the applicable Wyrd and Bifrost design authorities, the task and approved specification, the affected source and tests, sibling consumers, and the pinned DataFusion-distributed and Iceberg-compaction revisions.

The standing direction, “Wyrd does what everyone else does,” was applied as a binding authority. A mechanism is retained as `DRIFT` when it duplicates or departs from an established project/native mechanism without comparable-project precedent. Remediation below never requires a novel mechanism, check, file, setting, or option.

## Proposed-finding disposition

| Proposed source ID | Disposition | Stable finding | Validation result |
|---|---|---|---|
| `BVR-BEH-001`, `VARIANT-ARROW-002` | CONFIRMED | `FIND-TASK-001-1` | Both reports identify the same reachable integer-loss root and are deduplicated. |
| `VARIANT-ARROW-001` | REVISED | `FIND-TASK-001-2` | The trust-boundary defect is real, but remediation is limited to the TASK-001 built-in-table path and must preserve typed Variant errors rather than create parallel validators. |
| `BVR-BEH-002` | CONFIRMED | `FIND-TASK-001-3` | Both the publication metrics prerequisite and rewrite-wide uniqueness collection are nonstandard duplicate mechanisms. |
| `BVR-BEH-003`, `INV-REV-001` | REVISED | `FIND-TASK-001-4` | Architecture and the existing Bifrost schema guide are stale; no concrete stale example justifies inventing or changing an example. |
| `MR-001` | REVISED | `FIND-TASK-001-5` | Prose parsing is real drift, but simply deleting it would lose distributed Variant identity; the existing serialized error representation and existing dependency carrier are sufficient. |
| `MR-002` | CONFIRMED | `FIND-TASK-001-6` | The public constructor and emptiness branch expose an invalid state with no external caller. |
| `MR-003` | CONFIRMED | `FIND-TASK-001-7` | The inserted helper block displaced the public `QueryResult` documentation. |
| `ICE-DUR-001` | CONFIRMED | `FIND-TASK-001-8` | The local NDV default duplicates parquet-rs's native default. |
| `REPO-001` | CONFIRMED | `FIND-TASK-001-9` | The cumulative Rust diff violates the repository's mandatory documentation rule. |
| `REPO-002` | CONFIRMED | `FIND-TASK-001-10` | The cumulative Rust diff violates the repository's import and signature-name rule. |

No proposed finding was rejected. The two conflicts described in `followup-review.md` were independently resolved by source, caller, and dependency tracing as recorded in findings 3 and 5; no disagreement remains.

## Final validated ledger

### FIND-TASK-001-1 — JSON integers outside `u64` can silently become imprecise doubles

- **Source proposals:** `BVR-BEH-001`, `VARIANT-ARROW-002`
- **Disposition:** CONFIRMED
- **Classification:** INCORRECT
- **Obligation:** The approved Variant contract requires deterministic numeric classification and exact preservation or `NumericOutOfRange`; it does not permit an integral token to be rounded into a `Double`.
- **Location:** `crates/shared/wyrd-queue/src/variant.rs:142-147,597-619`
- **Evidence and reachability:** `EncodedVariant::from_json_text` first parses with ordinary `serde_json::Value`. The workspace's `serde_json` feature set does not enable arbitrary precision, and serde_json 1.0.151's parser converts an integer that exceeds `u64` to `f64`. `number_variant` then observes only `as_f64()` and emits a Variant `Double`. The existing boundary test stops at `u64::MAX`. This owner is reached by Oracle `parse_json` and by JSON-producing audit and gateway paths, so the defect is not isolated to a helper.
- **Observable consequence:** Values such as `18446744073709551616` can be accepted with a different numeric value/type instead of being encoded exactly as Decimal16 or rejected with the stable range error.
- **Smallest safe correction:** At the shared `EncodedVariant::from_json_text` owner, use serde_json's already-installed arbitrary-precision token preservation and classify the integral lexical form before floating-point conversion: representable `i64` stays integer, other exact integral values that fit Decimal16 become scale-zero Decimal16, and values beyond the supported exact range return `NumericOutOfRange`. Do not add a parser, dependency, numeric model, option, or second classification path.
- **Focused closure proof:** Add focused shared-encoder and Oracle `parse_json` cases for `u64::MAX + 1`, a negative integer below `i64::MIN` that fits Decimal16, the largest supported exact integral value, and the first out-of-range value. Assert exact Variant type/value or the exact stable error code and details.

### FIND-TASK-001-2 — Built-in write admission can accept unvalidated Variant metadata and bytes

- **Source proposal:** `VARIANT-ARROW-001`
- **Disposition:** REVISED
- **Classification:** INCORRECT
- **Obligation:** The approved server trust boundary requires the server to repeat Variant extension identity, size, JSON, numeric, and depth validation in stable precedence before acknowledging or persisting a built-in-table write.
- **Location:** `crates/vala/vala-bifrost-redux/src/tables/mod.rs:225-230`; `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:556-568,621-645`; `crates/vala/vala-bifrost-redux/src/schema/fingerprint.rs`; built-in declarations under `crates/vala/vala-bifrost-redux/src/tables/`
- **Evidence and reachability:** `DomainTable::CANONICAL_VALIDATOR` defaults to `None`; only signal-family tables opt in. The verification, evaluation, gateway, agent-trace, and audit built-ins declare Variant fields without an admission validator. Schema fingerprints are derived from Arrow data types and omit extension metadata, while the common shape check compares data types rather than the Variant extension identity. Signal validation traverses values, but its top-level extension check is not a general recursive built-in trust boundary. Any validator failure is then collapsed to `ScribeError::FingerprintMismatch`, which Gate exposes as `SchemaMismatch`, losing the required Variant error identity and precedence.
- **Observable consequence:** A raw Arrow IPC client can submit a structurally matching field with missing/wrong Variant extension metadata or malformed/oversized/deep bytes to a TASK-001 built-in; the server may accept it or report the wrong stable error.
- **Smallest safe correction:** Reuse `EncodedVariant::from_bytes` from one schema-driven recursive validation step at the common decoded built-in admission boundary, before WAL/admission. Check Variant extension identity for top-level and nested fields, and carry the resulting typed `BifrostError` through Scribe and Gate instead of converting it to fingerprint prose; genuine schema mismatches remain `SchemaMismatch`. Scope this correction to the TASK-001 built-ins. Do not add per-table duplicate validators, a second Variant decoder, a new public error model, or TASK-002 dynamic-table behavior.
- **Focused closure proof:** Send raw Arrow IPC to one REQ-010 non-signal built-in and to a nested signal Variant field with missing/wrong extension metadata, invalid bytes, excessive depth, and excessive size. Assert stable code/details in the specified precedence and prove no ACK or persistence. Retain a genuine schema mismatch case to prove it still maps to `SchemaMismatch`.

### FIND-TASK-001-3 — Row-lineage durability adds nonstandard metrics and rewrite-wide uniqueness mechanisms

- **Source proposal:** `BVR-BEH-002`
- **Disposition:** CONFIRMED
- **Classification:** DRIFT
- **Obligation:** Standing direction requires the established standard and comparable-project mechanism. Apache Iceberg row lineage is preserved by projecting and copying the reserved row-lineage fields; optional file metrics are not proof of row identity, and an exact one-to-one rewrite does not require a second global uniqueness fence.
- **Location:** `crates/vala/vala-bifrost-redux/src/forge/publication.rs:455,1456-1492`; pinned `iceberg-compaction` revision `94db7...`, `core/src/executor/datafusion/mod.rs` in the rewrite writer/lineage collection path
- **Evidence and reachability:** Wyrd publication rejects a file unless optional `value_counts` and `null_value_counts` entries exist for both lineage fields and match the record count. The [Apache Iceberg table specification](https://iceberg.apache.org/spec/) defines missing metrics entries as unknown; the metrics are optional and do not establish that exact lineage values were copied. The pinned compaction fork additionally retains every `_row_id` in memory, merges them, sorts them, and scans for duplicates after already validating and copying the reserved fields. Apache Iceberg's comparable implementation preserves lineage through projection/copying, as reflected in [Apache Iceberg PR #14149](https://github.com/apache/iceberg/pull/14149/files), without either Wyrd mechanism. The existing repeated-rewrite journey already compares exact before/after lineage.
- **Observable consequence:** Standards-valid Iceberg v3 publication can fail solely because optional metrics are absent, while large rewrites pay an unnecessary one-`i64`-per-row allocation plus global sort.
- **Smallest safe correction:** Delete the Wyrd optional-metrics prerequisite and delete the fork's rewrite-wide ID collection, merge, sort, and duplicate scan. Retain the established field-ID projection, per-batch missing/type/null validation before write, exact unchanged lineage write, and existing repeated before/after equality journey. Add no replacement check, metadata, configuration, or option.
- **Focused closure proof:** Run the existing repeated-rewrite lineage journey and pinned-fork lineage tests, including a valid input whose optional lineage metrics maps are absent. Assert exact lineage equality and successful publication; keep the existing missing/type/null rejection cases.

### FIND-TASK-001-4 — Required architecture authority and existing schema guide remain stale

- **Source proposals:** `BVR-BEH-003`, `INV-REV-001`
- **Disposition:** REVISED
- **Classification:** MISSING
- **Obligation:** The approved spec and task explicitly require Bifrost architecture and supported consumer documentation to describe the shipped contract.
- **Location:** `architecture/bifrost-design.md`; `docs/src/content/docs/bifrost/schema.svx`; no architecture, docs, or examples file appears in the cumulative diff
- **Evidence and reachability:** `architecture/bifrost-design.md` contains no Variant, Iceberg v3 row-lineage, changed query-function, or duplicate-key contract. The existing schema guide still documents multiple now-promoted JSON-bearing built-in columns as `Binary`, `Utf8`, or `List<Binary>`, still lists removed `details`, and omits Variant from its type mapping. These are direct public and maintainer consumers of the table-owned declarations. Discovery did not identify a concrete stale example, so requiring an invented example would be speculative.
- **Observable consequence:** Maintainers and users are directed to obsolete physical schemas and do not receive the approved Variant/query/lineage contract from the repository's designated authorities.
- **Smallest safe correction:** Update `architecture/bifrost-design.md` and the existing Bifrost schema guide in place from the table-owned declarations: document Iceberg v3 and hidden lineage, Variant representation and duplicate-key behavior, the shipped query surface, the actual promoted built-in shapes, and removal of `details`. Do not add a generator, documentation checker, file, setting, or example absent a concrete affected journey.
- **Focused closure proof:** Reconcile every documented built-in field against its owning declaration, then run the repository's existing `mise run docs:check` and `mise run check:docs` lanes. The task's existing example verification remains applicable, but no new example mechanism is required.

### FIND-TASK-001-5 — Distributed Variant identity is recovered by parsing human-readable error prose

- **Source proposal:** `MR-001`
- **Disposition:** REVISED
- **Classification:** DRIFT
- **Obligation:** Stable machine error identity must not depend on `Display` wording. Standing direction favors structured serialization through an existing transport capability over a parallel prose grammar.
- **Location:** `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4277-4350`; pinned `datafusion-distributed` revision `4cfa166`, `src/protocol/grpc/errors/datafusion_error.rs`
- **Evidence and reachability:** Oracle traverses every error source, calls `to_string()`, and splits English delimiters to reconstruct a Variant error. The pinned dependency's gRPC error model carries `External(String)`, serializes DataFusion external errors with `to_string()`, and reconstructs only a generic remote error, so typed source downcasting alone cannot cross the coordinator boundary. `BifrostError` already has a tagged serde representation. The current parser is therefore reachable for remote `parse_json` failures, but it duplicates the display format as an undocumented grammar.
- **Observable consequence:** A harmless wording or punctuation change can silently turn a stable remote Variant error into a generic analytical failure or corrupt its structured details.
- **Smallest safe correction:** At the `parse_json` external-error producer, put the existing serialized `BifrostError` representation into the dependency's existing string carrier, while retaining the typed source locally; at the coordinator, deserialize only the recognized structured payload. Delete the prose parser. Do not repin or extend the dependency, add a protobuf/enum/error format, or expose a new option.
- **Focused closure proof:** Add a focused local/remote test demonstrating that a `Display` wording change has no effect on stable code/details, plus a malformed or unrelated external string case that remains generic. Retain the real analytical journey assertion for exact Variant code and details.

### FIND-TASK-001-6 — `EncodedVariant` publicly exposes an invalid-state constructor and dead emptiness branch

- **Source proposal:** `MR-002`
- **Disposition:** CONFIRMED
- **Classification:** DRIFT
- **Obligation:** The type's documented invariant says an `EncodedVariant` is storable, but its public surface must not manufacture values that bypass format validation. Public API without a real caller is also contrary to the repository's concrete-minimum rule.
- **Location:** `crates/shared/wyrd-queue/src/variant.rs:108,169-180,200-204`
- **Evidence and reachability:** Public `EncodedVariant::sized` checks only byte length and accepts empty or malformed Variant bytes. Public `is_empty` then exposes that impossible state. Caller tracing found `sized` only at internal construction sites and no caller of `is_empty`; real external construction already goes through `from_json`, `from_json_text`, or `from_bytes`.
- **Observable consequence:** Downstream code can construct a value whose type promises validated/storable Variant bytes but whose contents cannot be decoded, forcing unrelated code to account for an invalid empty state.
- **Smallest safe correction:** Make the size-only helper private and delete `is_empty`. Keep the existing three validated public constructors. Add no replacement constructor, trait, flag, or compatibility alias.
- **Focused closure proof:** Compile all callers and retain the existing invalid-byte and round-trip tests; add a compile/API assertion only if the existing public-surface checks already have an established home.

### FIND-TASK-001-7 — `QueryResult` lost its attached public documentation

- **Source proposal:** `MR-003`
- **Disposition:** CONFIRMED
- **Classification:** REGRESSION
- **Obligation:** Existing public TypeScript API documentation must remain attached to the symbol it describes.
- **Location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:638-686`
- **Evidence and reachability:** The pre-existing `QueryResult` JSDoc is now immediately followed by `VARIANT_EXTENSION` and the inserted Variant helpers. TypeScript therefore associates it with the constant, while `export class QueryResult` begins without its documentation.
- **Observable consequence:** Generated/editor API documentation describes the wrong symbol and no longer explains the public query result.
- **Smallest safe correction:** Move the existing JSDoc block immediately above `QueryResult`; do not create a new documentation mechanism or hand-edit generated declarations.
- **Focused closure proof:** Run the existing TypeScript typecheck/documentation build lane that consumes this source and inspect the emitted/editor symbol association.

### FIND-TASK-001-8 — Wyrd duplicates parquet-rs's native Bloom-filter NDV default

- **Source proposal:** `ICE-DUR-001`
- **Disposition:** CONFIRMED
- **Classification:** DRIFT
- **Obligation:** Standing direction requires using the established library mechanism when it already supplies the same default.
- **Location:** `crates/vala/vala-bifrost-redux/src/parquet/writer_properties.rs:34-41,151`
- **Evidence and reachability:** Wyrd defines `BLOOM_NDV` as `DEFAULT_MAX_ROW_GROUP_ROW_COUNT` and explicitly sets it for Variant columns. parquet-rs 59.3 already resolves enabled Bloom filters with no per-column NDV to the writer's `max_row_group_row_count`; see the native [BloomFilterProperties](https://arrow.apache.org/rust/parquet/file/properties/struct.BloomFilterProperties.html) and [WriterPropertiesBuilder](https://arrow.apache.org/rust/parquet/file/properties/struct.WriterPropertiesBuilder.html) APIs. Both local writer recipes retain that native row-group default, so the Wyrd constant merely copies it and can diverge later.
- **Observable consequence:** A future row-group-size change can leave Bloom sizing pinned to a stale duplicated constant, and maintainers must reason about two defaults for one property.
- **Smallest safe correction:** Delete `BLOOM_NDV`, its import, and the explicit `.set_column_bloom_filter_max_ndv(...)` calls. Keep Bloom enablement and the false-positive probability; let parquet-rs derive NDV from the existing row-group setting. Add no setting or wrapper.
- **Focused closure proof:** Keep the focused writer-properties test asserting that both recipes resolve Variant Bloom NDV to the configured maximum row-group row count and preserve the required false-positive probability.

### FIND-TASK-001-9 — Added and materially changed Rust items lack mandatory rustdoc

- **Source proposal:** `REPO-001`
- **Disposition:** CONFIRMED
- **Classification:** VIOLATION
- **Obligation:** `AGENTS.md` and `architecture/agent-rules.md` require substantive documentation for every added or materially modified Rust item, including private items and tests, with `# Errors`, `# Panics`, and cancellation behavior where applicable.
- **Location:** Cumulative Rust diff; representative confirmed examples include `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:203-212,416-425,478-487,552-569` and `crates/shared/wyrd-client/src/error.rs:421`
- **Evidence and reachability:** New trait implementation methods in `variant_sql.rs` have no substantive item documentation, and the new client error test that panics through assertions lacks the required `# Panics`. These are examples of a cumulative-diff obligation, not an exhaustive waiver for the remaining changed items.
- **Observable consequence:** The candidate fails an explicit repository completion rule and leaves invariant/error behavior undiscoverable at the owning symbols.
- **Smallest safe correction:** Audit every added or materially changed Rust item in the cumulative diff and add concise substantive rustdoc, including the required error, panic, and cancellation sections. Use existing module/symbol documentation style. Do not add a new doc checker, lint exception, generated file, or allow attribute.
- **Focused closure proof:** Reinspect the cumulative diff item-by-item and run the existing `mise run lints` lane; verify no new or materially changed Rust item remains undocumented.

### FIND-TASK-001-10 — Added Rust code uses function-local imports and qualified signature names

- **Source proposal:** `REPO-002`
- **Disposition:** CONFIRMED
- **Classification:** VIOLATION
- **Obligation:** `architecture/agent-rules.md` requires imports at module scope and bare imported names in signatures and bounds, except the documented `Trait as _` and test-module-scope cases.
- **Location:** Representative confirmed examples: `crates/shared/wyrd-queue/src/variant.rs:351,570,763-765`; `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs:2732`; `crates/vala/vala-bifrost-redux/src/tables/mod.rs:2024,2297-2298`; `crates/wyrd/wyrd-mcp/tests/pg_mcp_bifrost_query.rs:741`; `crates/vala/vala-bifrost-redux/src/observe/eval.rs:188`; `crates/vala/vala-bifrost-redux/src/parquet/writer_properties.rs:130`; `crates/wyrd/wyrd-server/src/verification_runtime.rs:1178`
- **Evidence and reachability:** The cited functions contain ordinary local `use` declarations, including inside individual test functions, and signatures spell types/bounds such as `serde::Serialize`, `parquet_variant::BuilderSpecificState`, `parquet::file::properties::WriterPropertiesBuilder`, and `std::fmt::Debug` instead of importing them once at the relevant module scope.
- **Observable consequence:** The candidate violates the repository's single import convention and scatters dependency names through signatures, increasing duplication and making the changed modules harder to scan.
- **Smallest safe correction:** Move ordinary imports to the relevant module top (the enclosing test module counts as module scope) and import signature types/bounds under bare names. Preserve the documented `Trait as _` exception. Add no import checker or allow rule.
- **Focused closure proof:** Reinspect the cumulative Rust diff for local `use` declarations and qualified signature types, then run existing formatting and lint lanes.

## Rejected proposals

None.

## Validation result

**VALIDATED — 10 retained findings.** The ledger is non-empty. Source and caller tracing is complete for the retained roots, the required authorities were available, and no unresolved reviewer disagreement blocks orchestration.

The orchestrator-recorded successful checks (`git diff --check`, `codegen:check`, the Variant contract test, the Bloom test, and both exact pinned-fork tests) remain valid evidence for the behavior they exercise. They do not close the retained roots above; each finding names the focused proof required for closure.
