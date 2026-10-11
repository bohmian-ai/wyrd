# Variant, Arrow, and JSON contract domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `99c5871ec5ca664b9f54baa379b437ee09d66e95`
- Approved authority: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation task: `changes/active/bifrost-variant/review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md`

The candidate was the checked-out `HEAD` before and after source inspection.
The repository has no `.codegraph/` directory, so navigation used the cumulative
Git diff, `rg`, and direct source/caller inspection.

## Reviewed boundary

This pass traced the Variant value from JSON or OTLP production through Arrow
admission and durable encoding, then back through Oracle and every result
terminal. It covered:

- exact integer, decimal, floating-point, null, object, array, and duplicate-key
  classification;
- fixed size/depth limits and Variant error precedence;
- top-level and nested `arrow.parquet.variant` identity;
- validation before Scribe dispatch, WAL mutation, and acknowledgement;
- logical Arrow/Iceberg/Parquet round trips and unchanged extension metadata;
- Rust, Python, TypeScript, CLI, MCP, and JSON result decoding; and
- closure of prior `FIND-TASK-001-1` and `FIND-TASK-001-2`.

The governing authorities were `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/bifrost-design.md`,
`architecture/references/domain/arrow-analytical-interop.md`,
`architecture/references/domain/iceberg.md`, the approved revision-10
specification, the original task, and the R1 remediation task. The standard
implementation comparison used the installed Arrow 59.3
`parquet-variant`/`parquet-variant-json` behavior and serde_json's installed
`raw_value` facility. The human decision rejecting workspace-wide
`arbitrary_precision` and approving local `RawValue` plus `i128` lexical
classification was treated as binding.

## Authority and source coverage

| Boundary | Source and caller evidence | Result |
|---|---|---|
| Exact JSON classification and duplicate keys | `crates/shared/wyrd-queue/src/variant.rs:142-164,580-695,870-921`; Oracle `parse_json` at `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:627-643,903-937` | PASS — integer tokens never enter `f64`; scale-zero Decimal16 and range errors retain their path; the final duplicate object key wins. |
| OTLP type and duplicate-key projection | `crates/vala/vala-bifrost-redux/src/tables/signal.rs:84-180,265-278,482-500` and signal projection tests | PASS — standard Variant builders preserve protocol types and reverse traversal keeps the final repeated key. |
| Size, encoding, and depth | `EncodedVariant::{from_json_text,from_bytes,sized}` and recursive depth walk in `wyrd-queue/src/variant.rs`; raw admission journey in `wyrd-testing/tests/bifrost/server/verification_runtime.rs:1186-1359` | PASS — already-encoded data checks size before decoding and depth; JSON text reports numeric/depth failures at the exact pointer. |
| Arrow extension identity, top level and nested | `BuiltinTableDefinition::validate_variants`, `holds_variant`, `variant_identity_matches`, and `validate_variant_values` in `vala-bifrost-redux/src/tables/mod.rs:195-344` | PASS — every TASK-001 built-in declaration is traversed through Struct/List nesting and supplied Variant markers must match before value decoding. |
| Validation before durability and typed refusal | `scribe/ingress.rs:497-518,520-609`; `scribe/preprocess.rs:384-406`; `scribe/execution_lanes.rs:518-583`; `contracts.rs:283-290,333-375`; `gate/error.rs:206-235,298-388` | PASS — all built-ins resolve a definition, validation occurs during pre-ACK preprocessing before dispatch/WAL ownership, and the catalogued `BifrostError` survives Scribe, Gate, and gRPC problem details. Genuine schema mismatch remains separate. |
| Arrow/Iceberg logical storage | canonical extension construction in `wyrd-queue/src/variant.rs:383-412`; table declarations/fingerprints; pinned iceberg-rust `variant_round_trips_unshredded` evidence | PASS — the logical extension and storage Struct remain standard and unchanged through the unshredded round trip. |
| Query result decoding | `wyrd-queue/src/variant.rs:222-335`; `wyrd-client/src/bifrost/facade.rs:893-927`; Python `bifrost/mod.rs:307-327` and `python/wyrd/bifrost/__init__.py:979-1040`; TypeScript native `lib.rs:423-445` and `wyrd/src/index.ts:638-677,900-945`; CLI/MCP encoder callers | FAIL (`VARIANT-ARROW-R2-001`). |

## Prior-finding closure

### `FIND-TASK-001-1` — closed

`EncodedVariant::from_json_text` validates the whole input with the installed
`RawValue`, recursively preserves child tokens, and sends integral lexical
forms through one `i128`/`integer_variant` rule. The focused encoder test covers
both signs, `u64::MAX + 1`, Decimal16 boundaries, out-of-range path/details,
fractions, exponents, and the final duplicate key. Oracle's focused test proves
the same rule through `parse_json`. This is the approved local correction and
does not enable serde_json `arbitrary_precision`, add a parser/dependency, or
create another numeric model.

### `FIND-TASK-001-2` — closed

Every built-in now resolves its `BuiltinTableDefinition` in ingress. The common
decoded-batch boundary first checks the registered schema fingerprint, then
recursively checks every declared Variant marker and stored value before card
scope stamping, registered field-id stamping, dispatch, WAL append, or ACK.
`ScribeError::ContractViolation(BifrostError)` preserves exact Variant identity
through Gate and the standard gRPC problem-detail carrier. The existing journey
proves missing/foreign top-level metadata, nested metadata, malformed bytes,
depth, size-before-decode precedence, type-before-value precedence, genuine
schema mismatch, no ACK, and no persisted refused row.

## Material proposed finding

### VARIANT-ARROW-R2-001 — INCORRECT — native and JSON result terminals narrow stored Decimal16 values through `f64`

**Violated obligation.** Revision-10 `REQ-004` says a Variant value reads back
with the same type and value, integers never pass through floating point, and a
JSON integer outside `i64` that fits Variant Decimal remains a decimal.
`INV-002` prohibits narrowing or retyping. TASK-001 explicitly includes the
Rust/Python/TypeScript terminals and HTTP/MCP JSON consumers in its affected
consumer closure.

**Exact locations.** The shared result decoder is
`crates/shared/wyrd-queue/src/variant.rs:246-249`. Its callers are
`variant_cell_to_json` and `VariantJsonEncoderFactory` at `:264-333`, Rust
`QueryResult::deserialize` at
`crates/shared/wyrd-client/src/bifrost/facade.rs:893-927`, Python
`variant_to_python` at `sdks/wyrd-sdk-python/src/bifrost/mod.rs:307-327`, and
TypeScript `variant_to_value` at
`sdks/wyrd-sdk-ts/native/src/lib.rs:423-445`. CLI and MCP JSON output also use
`VariantJsonEncoderFactory`.

**Evidence and reachability.** `variant_bytes_to_json` delegates to the
installed `parquet_variant_json::VariantToJson::to_json_value`. In
`parquet-variant-json-59.3.0/src/to_json.rs:285-305`, a `Decimal16` first tries
`i64`, then `u64`, and casts to `f64` when neither holds. The remediated producer
now intentionally stores an exact token such as `18446744073709551617` as
scale-zero Decimal16. Every typed/native terminal above then routes that stored
value through the lossy `serde_json::Value`: Rust observes a floating JSON
number, Python a float, TypeScript a JavaScript number rather than `bigint`, and
the JSON writer renders the rounded number. The existing consumer journeys use
an `i64` just above JavaScript's safe range, so they prove napi's `i64`/`bigint`
case but never enter the Decimal16 fallback.

**Observable consequence.** A value that R1 correctly admits and stores exactly
can be returned with another numeric type and, for most 20- to 38-digit values,
another value. Arrow terminals remain correct because they return the extension
unchanged; `to_json(parse_json(...))` also remains exact because the standard
Variant JSON writer writes directly to text. The defect is specifically the
shared `to_json_value` result path.

**Required outcome and decision boundary.** Preserve standard Arrow Variant as
the exact typed result and use `VariantToJson::to_json`'s direct standard text
writer for JSON-rendering surfaces; do not add a bespoke JSON-number parser,
second Variant model, option, or setting. The approved native-terminal contract
also needs one explicit representation for a Decimal16 integer beyond `u64`.
Python can represent it as `int` and TypeScript as `bigint`, but the currently
locked Rust `serde_json::Value` cannot represent it exactly without the rejected
workspace-wide `arbitrary_precision` feature. Selecting a different Rust/native
decimal representation changes a public result contract, so this portion
requires a specification decision rather than a remediation invented by the
reviewer.

**Focused closure proof.** Query a whole Variant containing
`18446744073709551617` and the positive/negative 38-digit boundaries. Assert the
Arrow terminal still carries Decimal16, every JSON-rendering terminal emits the
exact digits, and each language-native terminal returns the explicitly approved
non-floating representation. Include one value whose `f64` conversion changes
digits, not only `2^64`.

This finding is not `DRIFT`: the bad conversion is the installed dependency's
documented convenience conversion, but it is incompatible with Wyrd's stronger
exact-value contract. No additional bespoke mechanism is recommended. No other
candidate mechanism in this reviewed boundary lacked either repository
authority or standard-library/dependency precedent.

## Verification evidence and limits

- Re-ran the exact focused shared tests:
  `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=variant::tests::json_text_classifies_integers_from_their_tokens) | test(=variant::tests::json_converts_under_the_variant_contract)'` — PASS (2 tests).
- Re-ran
  `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=oracle::variant_sql::tests::parse_json_keeps_exact_integers_and_refuses_the_rest)'` — PASS.
- Reviewed the remediation record's successful Postgres-backed raw-IPC
  `builtin_variant_columns_are_refused_before_ack` journey and the original
  Rust/Python/TypeScript/MCP consumer journeys. I did not rerun those serialized
  environment-owning lanes in this independent pass.
- The recorded result-terminal evidence covers valid objects, arrays, nested
  Variants, null parents, and `i64`/JavaScript `bigint`, but not Decimal16 outside
  `u64`; that missing case is the reachable gap above.
- TASK-003 owns shredded physical layouts. This review covered the TASK-001
  logical unshredded extension and did not treat absent shredding evidence as a
  TASK-001 limit.

## Overall result

**FAIL**

R1 closes both prior Variant/Arrow findings, including the approved
`RawValue`/`i128` correction and common pre-durability built-in validation. One
reachable query-result path still violates exact Variant numeric meaning, and
the locked Rust terminal type prevents a decision-complete remediation without
clarifying the public Decimal16 result representation.
