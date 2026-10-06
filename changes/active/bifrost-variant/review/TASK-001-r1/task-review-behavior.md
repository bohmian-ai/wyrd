# TASK-001 Behavior Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `3cf911fce699bbfe197f8b95e72b13e2f551f766`
- Approved authority: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Candidate and checked-out `HEAD` both resolved to the candidate commit at the start and end of this review. No `.codegraph/` directory exists.

## Caller-to-result coverage

I traced the cumulative diff through the shared Variant encoder/decoder, built-in signal and verification projections, server/client result terminals, Oracle SQL registration and error mapping, catalog creation, Forge rewrite publication, the pinned Iceberg forks, and the Rust/Python/TypeScript/MCP journeys. The implementation evidence in the task reports all V1-V17 commands green; this review did not rerun those expensive lanes. The orchestrator independently reported `git diff --check` green.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: every Bifrost table is created and validated as Iceberg v3 | `catalog/bifrost_catalog.rs` creates `FormatVersion::V3` and refuses other versions | V10 plus catalog coverage reported in task evidence | PASS |
| REQ-002, INV-003, AC-002: exact row lineage survives repeated Forge rewrites and v3 GC | Pinned `iceberg-compaction` revision projects and writes both hidden lineage columns; Forge v3 creation/GC changes are cumulative | V10 and V13 reportedly compare lineage across repeated rewrites | PASS |
| REQ-003: Variant is a stable logical type across schema, Arrow, Iceberg, Parquet, fingerprints, and SDK wire surfaces | `DataTypeSpec::Variant`, `wyrd_queue::variant`, `CanonicalType::Variant`, generated schemas, Python/TS unions and result decoders | V1, V5-V8, V12, V16 reported green | PASS |
| REQ-004, INV-002: integers keep their type and are never converted through floating point; any integer fitting Variant decimal becomes decimal | `from_json_text` parses into the default `serde_json::Value`; `number_variant` handles only `i64` and `u64`, then treats every remaining number as `f64` | The focused test covers `i64` and `u64::MAX`, but no integer outside `u64` or below `i64` | **FAIL — BVR-BEH-001** |
| REQ-005, AC-009: Scribe and Forge Bloom capacity uses row-group geometry and keeps folding/FPP | `parquet/writer_properties.rs` uses the existing row-group limit for both writer paths | V11 reported green | PASS |
| REQ-006-REQ-011, INV-001, INV-004, INV-005, AC-001, AC-003: built-in persisted schemas, producers, permissions, promotions, audit hash inputs, and query consumers use Variant/Struct with no old stored form | Built-in ledgers and producers/consumers were converted; the public schema guide still documents Binary/Utf8/List\<Binary\>/`details` legacy forms | V1-V8 and V14-V15 prove runtime paths, but no proof can make the shipped stale consumer documentation correct | **FAIL — BVR-BEH-003** |
| REQ-017: all production Oracle sessions expose the same semantic Variant operators/functions while Struct remains `get_field` | `OracleVariantSql::install` is routed through interactive, admission/follower, analytical, and worker builders; plan-shape test keeps `get_field` distinct | V9 and `variant_operators_and_functions_follow_the_contract` reported green | PASS |
| REQ-019, AC-005: invalid/unrepresentable numbers fail with the exact stable catalog code rather than being narrowed | Numbers outside the default `serde_json::Number` integer domain reach the `as_f64` arm before `VariantDecimal16::try_new` | Existing evidence covers malformed JSON but not the required decimal-range boundary | **FAIL — BVR-BEH-001** |
| INV-006, AC-008: tenant and sensitivity checks remain before provider/file IO for whole and nested values | Existing authorization resolves logical roots; session journey asserts refused reads do not advance follower leases | V9 reports the whole/nested sensitive matrix green | PASS |
| INV-007: Variant value depth and encoded size remain fixed and bounded | Shared encoder enforces depth 64 and 8,388,608 encoded bytes; Arrow extension validation repeats at the server boundary | V1 plus OTLP oversize journeys reported green | PASS |
| Task acceptance: all tables are v3 only after lineage-safe repeated rewrite and GC; hidden columns remain internal and the five-field handoff is unchanged | Runtime implementation and fork preserve the handoff, but candidate adds two non-standard proof mechanisms that are neither required by Iceberg nor capable of proving exact preservation | V10/V13 already supply the direct before/after equality proof | **FAIL — BVR-BEH-002 (DRIFT)** |
| Task consumer closure: generated contracts, examples, and supported-type documentation consume the new shapes | Generated schemas changed; `docs/src/content/docs/bifrost/schema.svx` was not changed and still publishes removed layouts | No docs lane is listed as run in task evidence; the stale source is direct evidence | **FAIL — BVR-BEH-003** |
| Non-goals: no shredding policy, user-model inference, second reader/model, `datafusion-variant`, signing, migration, DataFusion repin, compatibility alias, or unrelated dependency | Cumulative diff and manifests retain the current DataFusion source and add only the locked Variant crates/fork revisions | `Cargo.lock`, V16, and diff inspection | PASS |
| Human standing direction: use established standard/comparable-project behavior; flag novel mechanisms as drift | Candidate requires optional Iceberg metrics as lineage proof and the pinned fork retains and sorts every rewritten row id as an extra uniqueness check | Apache Iceberg v3 specifies copy/inheritance semantics and makes the metric maps optional; V10/V13 already test the required result directly | **FAIL — BVR-BEH-002 (DRIFT)** |

## Proposed findings

### BVR-BEH-001 — INCORRECT — JSON integers outside `u64` are silently converted through `f64`

- **Violated obligation:** REQ-004, REQ-019, INV-002, scenario 1, and the first task acceptance criterion require every integral JSON token that fits Variant decimal to become a decimal, an integer that fits no Variant numeric type to receive `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`, and no integer to pass through floating point.
- **Exact location:** `crates/shared/wyrd-queue/src/variant.rs:142-147` and `:591-619`.
- **Producer-to-consumer evidence:** `EncodedVariant::from_json_text` first parses with the workspace's default `serde_json = "1"` (no `arbitrary_precision`). Serde JSON parses integer tokens beyond its `u64`/`i64` representation as `f64`. `number_variant` then checks only `as_i64` and `as_u64`; every remaining finite number is stored as `Variant::Double`. Thus `parse_json('18446744073709551616')`, gateway/audit JSON text, and any other text caller reaching this shared owner silently round the integer rather than storing the exact scale-zero `Decimal16`. A still larger integral token that fits no Variant decimal can likewise become a finite double instead of the required stable refusal. The only boundary test in `json_converts_under_the_variant_contract` stops at `u64::MAX`, so all reported journeys miss the reachable branch.
- **Observable consequence:** SQL/native/HTTP/MCP consumers receive a changed numeric value and type for valid large integral JSON, violating losslessness; out-of-range input can be acknowledged instead of refused.
- **Required testable correction:** keep conversion in the shared `EncodedVariant` owner, but preserve the JSON number token using the installed parser's established arbitrary-precision facility and classify integral syntax before the floating-point branch. Convert signed/unsigned values fitting `i64` normally, scale-zero integers fitting `VariantDecimal16` exactly, and return the catalogued numeric-range error otherwise. Add one focused check covering `u64::MAX + 1`, a negative integer below `i64::MIN` that fits Decimal16, and one integer beyond Decimal16; prove exact round-trip/type for the first two and the stable code/path for the last through `parse_json` or the shared encoder.

### BVR-BEH-002 — DRIFT — the lineage implementation adds non-standard checks that do not prove preservation

- **Violated obligation:** the human standing direction requires Wyrd to follow established/common mechanisms and classifies a mechanism or check absent from the standard and comparable projects as drift. The task requires exact lineage preservation, not a new lineage-verification protocol.
- **Exact locations:** `crates/vala/vala-bifrost-redux/src/forge/publication.rs:455,1456-1492`; pinned dependency `iceberg-compaction@94db7b94f72c48c75c937d36a59e80c237e8ca72:core/src/executor/datafusion/mod.rs:278-280,307-309,337-360,415-471`.
- **Evidence:** Forge refuses an output unless both reserved field IDs appear in `DataFile.value_counts` and `null_value_counts`. Apache Iceberg defines both maps/entries as optional and says a missing map/id is equivalent to a missing metric; lineage correctness is defined by copying the existing `_row_id` and `_last_updated_sequence_number` values when an existing row moves, not by publishing metrics ([Iceberg table spec](https://iceberg.apache.org/spec/)). Counts show only presence/non-nullness, so they cannot distinguish copied lineage from regenerated non-null values. The fork adds a second check that accumulates every `_row_id` in `Vec<i64>`, merges all vectors, sorts them, and checks uniqueness; exact preservation already follows from projecting/copying the two input columns and is directly proven by V10/V13. No Iceberg lineage rule requires a rewrite-wide in-memory uniqueness scan.
- **Observable consequence:** a standards-compliant writer configuration that omits optional count metrics is rejected even when it preserves lineage, while the metric gate can still accept rewritten values that are non-null but wrong. The fork also adds rewrite-size-proportional retained memory and an `O(n log n)` sort solely for this check.
- **Required testable correction:** delete the publication-time metric gate and the fork's rewrite-wide row-id accumulation/sort. Keep the standard mechanism already present: project the inherited lineage columns, require the bounded input batch to carry non-null `Int64` lineage, and write those exact columns unchanged. Keep the existing repeated-rewrite before/after equality journey as closure proof; it directly tests the required outcome without inventing persistent metadata, settings, or another verifier.

### BVR-BEH-003 — MISSING — shipped Bifrost documentation still publishes the replaced schemas

- **Violated obligation:** the task explicitly includes supported-type documentation and docs/examples in consumer closure (`TASK-001` lines 38-43 and 172-180); scenario 1 requires every consumer to move with the old stored forms, and the first acceptance criterion permits no legacy stored form.
- **Exact location:** `docs/src/content/docs/bifrost/schema.svx:71-85,107-130,156-190,213-252,255-264` (the file is absent from the cumulative diff).
- **Evidence:** the guide still calls span attributes/resource/scope `Binary`, entity references `List<Binary>`, log body `Utf8`, Eval `context`/`media` JSON `Utf8`, verification output `details: Utf8`, Eval result `actual`/`expected: Utf8`, and agent trace payloads `Utf8`. It omits the new Variant type, exact `drift_report`/`eval_summary` Structs, and the newly promoted signal fields. These contradict the candidate's current ledgers and public query behavior.
- **Observable consequence:** users author queries and integrations against columns that no longer exist or the wrong logical types, including selecting `details` instead of the two replacement Structs and treating Variant columns as text/binary.
- **Required testable correction:** update the existing Bifrost schema guide in place from the table-owned ledgers: describe Variant once in the type mapping, replace every affected built-in column and exact Struct layout, remove `details`, and list the promoted columns introduced by REQ-006-REQ-008. Run the existing docs checks named by repository policy; do not add a documentation generator or a new schema-check mechanism.

## Verification assessment

- Credible reported evidence: all 17 task-local commands are recorded as exit 0, including the exact Rust, Python, TypeScript, MCP, Oracle, Forge, fork, and codegen selectors. The candidate pins match the locally checked-out tested fork revisions.
- Independently available in this review: cumulative source/diff inspection and the orchestrator's green `git diff --check`.
- Closure gaps: no reported test exercises a JSON integer outside `u64`; V10/V13 prove exact lineage more directly than the two drift checks; the task evidence does not report `docs:check`/`check:docs`, and the docs source is observably stale regardless of a build result.

## Overall result

**FAIL**

The candidate does not yet satisfy the original task exactly. It has one reachable loss-of-precision defect, two unnecessary non-standard lineage checks, and an unconverted public documentation consumer.
