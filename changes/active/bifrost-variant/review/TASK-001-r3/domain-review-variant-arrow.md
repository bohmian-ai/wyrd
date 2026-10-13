# Variant and Arrow Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `555308ba14058ddc56102d2f925298ef43858175`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediations: `TASK-001-R1-close-variant-contract-gaps.md` and `TASK-001-R2-exact-integers-and-late-errors.md`

The candidate commit remained unchanged throughout this review.

## Reviewed boundary

This review traced Variant data from JSON and built-in producers through the
canonical unshredded Arrow extension, raw Arrow IPC decode, recursive built-in
validation, Scribe preprocessing and WAL preparation, Oracle JSON parsing and
Variant operators, and the Rust/Python/TypeScript/JSON result consumers. It
also inspected the nested Variant layouts in spans and verification results,
the exact-integer cases added by revision 11, and the admission journey that
bypasses client-side validation.

## Authority and source coverage

| Area | Authority and source inspected | Result |
|---|---|---|
| Variant representation and limits | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/bifrost-design.md` storage/Variant and query contracts; `architecture/references/domain/{olap-serving,arrow-analytical-interop}.md`; spec REQ-003, REQ-004, REQ-019, INV-001, INV-002, INV-007; `wyrd-queue/src/variant.rs` | PASS — one installed Arrow/Parquet Variant model owns encoding, full byte validation, depth/size limits, and native JSON rendering. `serde_json` `arbitrary_precision` is not enabled. Integral JSON tokens are classified lexically as `i64`, then `u64`, and all other integral tokens are refused. |
| Arrow declaration and schema projection | Spec locked representation and fingerprint; `wyrd-queue/src/{schema,batch_builder}.rs`; `vala-bifrost-redux/src/tables/fields.rs`; `wyrd-server/src/bifrost/convert.rs`; generated `wyrd-spec` schemas | PASS — `DataTypeSpec::Variant` projects to the standard `arrow.parquet.variant` marker over canonical unshredded storage; nested Struct/List declarations retain their Variant identity and no parallel model was introduced. |
| Built-in producer closure | Spec REQ-006 through REQ-011 and persisted Struct layouts; producers and schemas under `vala-bifrost-redux/src/tables/{traces,logs,metrics,verification,eval,gateway,dev,audit}`; `wyrd-client/src/observe/eval.rs`; `wyrd-server/src/{verification/results,components/gateway/capture}.rs` | PASS — open values use `EncodedVariant`/`VariantColumnBuilder`, fixed verification values remain Struct, and the registry exposes all eleven built-ins through the same definition owner. No reviewed producer retained the replaced JSON-text or protobuf-binary form. |
| Raw Arrow trust boundary and error precedence | Spec locked write precedence, REQ-019, INV-007; R1 FIND-2; `tables/mod.rs:195-344`; `scribe/ingress.rs:497-592`; `scribe/execution_lanes.rs:518-583`; `scribe/preprocess.rs:564-715`; `contracts.rs:283-290`; `gate/error.rs` | PASS — every built-in resolves a `BuiltinTableDefinition`; after schema fingerprinting, top-level and nested Variant markers and bytes are checked in deterministic field/row order. Size precedes byte validity and depth. Typed `BifrostError` survives Scribe and Gate. Native sources are completely decoded, validated, stamped, split, and serialized before any WAL append or durable ACK; an error drops earlier prepared slices. |
| Nested built-ins | Spec persisted Struct layouts plus REQ-006/REQ-009; recursive `holds_variant`, `variant_identity_matches`, and `validate_variant_values`; span event/link schemas and verification `drift_report.features`; `verification_runtime::builtin_variant_columns_are_refused_before_ack` | PASS — Struct and List traversal validates the actual child row/range, skips null parents, names the top-level field in failures, and the raw-IPC journey covers both a non-signal top-level Variant and a signal Variant nested under a list/struct. |
| Oracle parse and native decode | Spec REQ-004, REQ-017 through REQ-019; `oracle/variant_sql.rs`; shared client `bifrost/facade.rs`; Python `bifrost/mod.rs`; TypeScript native `lib.rs`; MCP query rendering | PASS — `parse_json` reuses `EncodedVariant::from_json_text`; `try_parse_json` maps invalid JSON to null; `variant_get` uses Arrow-rs and returns the extension; Struct remains `get_field`. `u64::MAX` remains exact through the shared decoder and language-native projections. |
| Standards/drift check | Human standing direction; installed Arrow 59.3 `VariantType`/`VariantArray`; Parquet Variant crates; comparable Arrow extension and Problem Details patterns | PASS — no extra Variant encoding, duplicate validator model, capability option, metrics gate, repository check, or configuration surface was added. The fixed limits are contract constants, not knobs. The server-side validation is the required trust-boundary repetition of the standard encoding, not a competing format. |
| Binding decisions encountered | Revision 11 and caller direction | PASS — no duplicate-row-id or optional-metrics gate is required here; `arbitrary_precision` remains off; integers beyond `i64`/`u64` are refused; late failures use the full catalog problem outside this domain's encoding owner; the accepted Interactive-only Python/TypeScript late-failure limit does not weaken Variant encoding or admission proof. |

## Material proposed findings

None. I found no reachable Variant/Arrow encoding or built-in admission defect,
no unmet obligation in this domain, and no nonstandard mechanism that should be
removed under the standing direction.

## Verification evidence and limits

Independently rerun on candidate `555308ba14058ddc56102d2f925298ef43858175`:

- `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=variant::tests::json_text_classifies_integers_from_their_tokens) | test(=variant::tests::json_converts_under_the_variant_contract)'` — 2 passed.
- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=oracle::variant_sql::tests::parse_json_keeps_exact_integers_and_refuses_the_rest) | test(=oracle::variant_sql::tests::variant_operators_and_functions_follow_the_contract)'` — 2 passed.
- `cargo tree -e features -p wyrd-queue --locked` exposed no `serde_json/arbitrary_precision` feature.

The Postgres-backed raw-IPC refusal journey, Rust/Python/TypeScript/MCP
built-in journeys, and full task lanes were not rerun in this independent
domain pass. Their recorded candidate evidence was checked against the current
test source and the complete producer-to-WAL/result paths. The accepted
Python/TypeScript Interactive-only late-failure limit is outside the admission
finding boundary; distributed reconstruction is shared Rust code and is
covered by the task's Rust multi-pod evidence.

## Overall result

**PASS**
