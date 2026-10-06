# Variant, Arrow, and persisted-schema domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `3cf911fce699bbfe197f8b95e72b13e2f551f766`
- Approved authority: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`

The candidate commit was checked before and after review and remained the
repository `HEAD`.

## Reviewed boundary

This pass covered the canonical Variant type from public contract to durable
storage and back:

- `DataTypeSpec::Variant`, stable error details, and generated schemas;
- the `arrow.parquet.variant` extension and unshredded
  `Struct<metadata: Binary, value: Binary>` storage;
- JSON and OTLP conversion, duplicate-key handling, size/depth validation,
  null placeholders, native JSON result rendering, and stable fingerprints;
- fixed persisted Struct layouts and every REQ-006 through REQ-010 built-in;
- canonical Arrow ingest validation and Scribe's path from decoded IPC to the
  registered physical schema;
- Arrow/Iceberg conversion, Parquet field-id verification, and the pinned
  Iceberg round-trip revision.

The governing authorities were `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/bifrost-design.md`,
`architecture/references/domain/arrow-analytical-interop.md`,
`architecture/references/domain/iceberg.md`, the revision-10 specification,
and TASK-001. The standard-layout comparison used the Apache Arrow canonical
extension contract and Apache Parquet Variant encoding linked by those
authorities. I found no candidate-only mechanism that lacked both a Wyrd
authority and an established comparable implementation, so this report has no
`DRIFT` proposal.

## Source coverage and results

| Boundary | Evidence inspected | Result |
|---|---|---|
| Public type and failures | `crates/wyrd-spec/src/vala/{api,error}.rs`; generated `bifrost_*` schemas | PASS |
| Arrow declaration and logical fingerprint | `wyrd-queue/src/{schema,variant}.rs`; `vala-bifrost-redux/src/tables/{fields,mod}.rs` | PASS |
| JSON numeric/type preservation | `wyrd-queue/src/variant.rs`; workspace `serde_json` feature selection; Oracle `parse_json` | FAIL (`VARIANT-ARROW-002`) |
| OTLP conversion and duplicate keys | `tables/signal.rs` plus trace/log/metric projections and tests | PASS |
| Fixed Struct and built-in schemas/producers | verification, eval, gateway, agent-trace, audit, trace, log, and metric table/projection owners; server and client producers | PASS for declared shape and ordinary produced values |
| Server Arrow trust boundary | `scribe/execution_lanes.rs`, `tables/signal.rs`, `tables/mod.rs`, `contracts.rs`, `gate/error.rs` and all validator registrations | FAIL (`VARIANT-ARROW-001`) |
| Arrow/Iceberg/Parquet round trip | catalog conversion, `parquet/promoted_object.rs`, pinned `iceberg-rust` revision `e999331f...`, and its `variant_round_trips_unshredded` source/test | PASS for valid values |
| Result terminals | Rust JSON encoder, Python/TypeScript native conversion, CLI/HTTP/MCP rendering | PASS for valid JSON-representable Variant values |

## Material proposed findings

### VARIANT-ARROW-001 — INCORRECT — the server does not enforce the Variant wire/value contract for every built-in write

**Violated obligation.** Revision 10 requires the server trust boundary to
accept only the Variant extension and repeat size, encoding, numeric, and depth
validation in row/field order. TASK-001 requires exact Variant limits and
stable error codes for every built-in persisted contract, with no malformed or
partial value acknowledged.

**Exact locations.** `crates/vala/vala-bifrost-redux/src/tables/mod.rs:225-230`,
`crates/vala/vala-bifrost-redux/src/tables/signal.rs:983-1009`,
`crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:621-645`,
`crates/vala/vala-bifrost-redux/src/contracts.rs:270-271`, and
`crates/vala/vala-bifrost-redux/src/gate/error.rs:309-311`.

**Evidence.** `DomainTable::CANONICAL_VALIDATOR` defaults to `None`, and only
the trace, log, and metric tables register one. The REQ-009/REQ-010 tables
(`verification.results`, eval observations/result items, gateway calls, agent
traces, and audit log) therefore rely on Arrow type shape alone. Variant's
extension marker is field metadata, while Scribe's `shape_matches` and the
ordinary schema fingerprint compare only `DataType`; a plain
`Struct<metadata,value>` consequently has the same accepted shape. No
`EncodedVariant::from_bytes` walk runs for those tables, so malformed,
oversized, or over-depth bytes can reach stamping and durable write.

The three signal validators do call `EncodedVariant::from_bytes`, but
`validate_field_identity` checks the extension only when the Variant is the
top-level declared field, not for Variant children inside event/link/exemplar
lists. More importantly, every value-level refusal is converted at
`execution_lanes.rs:636-645` to `ScribeError::FingerprintMismatch`; Gate then
publishes the fingerprint-mismatch catalog error rather than the required
`WYRD_VALA_400_VARIANT_INVALID_JSON`, `...TOO_DEEP`, or
`WYRD_VALA_413_VARIANT_TOO_LARGE` detail.

**Observable consequence.** A raw Arrow IPC writer can submit a fixed built-in
Variant as an unmarked storage Struct or submit invalid Variant bytes to a
REQ-009/REQ-010 table and have them admitted. The bad value can later fail
Parquet/Oracle decoding after acknowledgement. A malformed signal Variant is
refused, but callers receive the wrong stable error identity and details.

**Required testable correction.** Reuse one schema-driven Variant validation
walk at the common resolved-table ingest boundary for every table. Before WAL
admission, recursively require the extension on every declared Variant and run
the existing `EncodedVariant::from_bytes` validation in input row and logical
field order. Preserve the resulting catalogued `BifrostError` through
Scribe/Gate instead of collapsing it into fingerprint mismatch. Do not add a
second Variant model or per-table validators. Prove closure with raw Arrow IPC
writes to one REQ-010 table and one nested signal Variant covering a missing
extension, invalid bytes, too-deep/too-large values, exact public codes/details,
no acknowledgement, and no persisted row.

### VARIANT-ARROW-002 — INCORRECT — valid decimal-range JSON integers are silently converted through `f64`

**Violated obligation.** REQ-004 requires integers outside signed 64-bit range
that fit a Variant decimal to be stored as decimals, integers never to convert
through floating point, and an out-of-range integer to receive
`WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`. INV-002 prohibits narrowing or
retyping.

**Exact locations.** `Cargo.toml:101`,
`crates/shared/wyrd-queue/src/variant.rs:142-147`, and
`crates/shared/wyrd-queue/src/variant.rs:597-619`.

**Evidence.** `from_json_text` first parses into the workspace's ordinary
`serde_json::Value`. The dependency enables `float_roundtrip`/`preserve_order`
but not `arbitrary_precision`. Once an integer exceeds `u64`, serde_json's
standard parser represents the long integer as `f64` (or rejects an infinite
result); `number_variant` then takes `as_f64()` and emits a Variant Double.
For example, `18446744073709551617` fits `VariantDecimal16` but is rounded by
`f64` before Variant sees it. An even larger integer can be reported as
`VARIANT_INVALID_JSON` instead of the required numeric-range error. The focused
contract test exercises `u64::MAX`, which still takes the decimal branch, but
does not cross the `u64` boundary.

**Observable consequence.** SQL `parse_json`, gateway/audit JSON text, and any
other `from_json_text` caller can silently change a valid integer's type and
value. This defeats exact persisted round trips and can also return the wrong
stable failure code.

**Required testable correction.** Preserve JSON number lexemes until integer
classification, using serde_json's established arbitrary-precision support and
the already-installed `VariantDecimal16`; parse integral lexemes into the
decimal representation before considering `f64`, and return
`NumericOutOfRange` only when the integral token exceeds the Variant decimal
range. Keep non-integral JSON numbers as Double. Add one shared encoder test
and one `parse_json` test for an exact integer just above `u64::MAX`, the maximum
accepted decimal integer, and the first refused integer, asserting Variant
type/value and stable error code.

## Verification assessment and limits

- Re-ran
  `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=tables::tests::variant_contract_and_builtin_schemas_are_stable)'`:
  PASS (1 test, 836 skipped).
- Reviewed the task's recorded V1-V17 successful evidence, including the
  cross-language built-in journeys and the two pinned-fork tests. That evidence
  proves ordinary valid values and declared layouts, but it does not exercise a
  malformed raw Arrow Variant against a non-signal built-in, preservation of a
  validator's public error through Scribe/Gate, a missing nested extension, or
  an integer above `u64::MAX`.
- The external fork revisions in `Cargo.lock` match the recorded tested
  revisions. Their source covers unshredded Variant schema/value round trips;
  TASK-003 owns shredded physical schemas.

## Overall result

**FAIL**

The valid-value schemas and round trips are present, but the server trust
boundary can persist unvalidated Variant storage on several required built-in
tables, and JSON text can silently retype/round decimal-range integers.
