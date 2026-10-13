# Domain Review: SDK parity and public query contracts

## Subject

- Domain: Rust, Python, TypeScript, MCP, and generated public-contract parity for Variant results and failures.
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `3cf911fce699bbfe197f8b95e72b13e2f551f766`
- Task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Specification: `changes/active/bifrost-variant/spec.md` revision 10.
- Candidate identity was rechecked after review and remained unchanged.

## Authority and source coverage

| Boundary | Authority and source inspected | Assessment |
|---|---|---|
| Public logical type and generated contracts | `AGENTS.md` §§2, 3, 8, 9, 11; spec REQ-003, REQ-018, REQ-019; `wyrd-spec/src/vala/{api,error}.rs`; generated `wyrd-spec/{schemas,tests/schemas}`; TypeScript generated `error-codes.ts` | `DataTypeSpec::Variant`, the canonical extension name and fixed limits are owned by `wyrd-spec`. All six locked catalog codes are derive-backed; generated schemas contain `Variant`, and the TypeScript generated code union contains every new code. `mise run codegen:check` regenerated all schemas/stubs and passed without drift. |
| Server Arrow/wire conversion | `wyrd-server/src/bifrost/convert.rs:82-181` and its callers | `field_to_arrow` attaches the canonical extension metadata, and `field_from_arrow` recognizes the extension before interpreting its Struct storage, strips only extension keys, and returns the logical `DataTypeSpec::Variant`. The wire description therefore does not leak the physical `metadata`/`value` Struct. |
| Rust typed and Arrow terminals | `wyrd-client/src/bifrost/facade.rs:890-923`; `wyrd-client/tests/pg_bifrost_e2e.rs:2731-2807`; Rust SDK drift journey | `QueryResult` retains Arrow batches and extension metadata, while `sql_as` uses Arrow's encoder with the shared Variant encoder. The journey writes through the real shared SDK to a booted server, publishes, reads the unchanged Arrow extension, decodes whole Variant values including an integer beyond 2^53, exercises Struct and nested-Variant access, and checks invalid/lenient JSON behavior. The drift journey additionally consumes the whole `drift_report` Struct with its nested Variant `features`. |
| Python terminal and public exports | `wyrd-sdk-python/src/bifrost/mod.rs:307-327`; `python/wyrd/bifrost/__init__.py:979-1040`; generated `__init__.pyi`; `test_bifrost_query.py:456-550`; drift/observe/OTEL consumer journeys | The public model terminal walks top-level, Struct, List, and large-list fields, delegates Variant byte decoding to the shared Rust owner, and preserves arbitrary-width Python integers. The raw Arrow terminal remains unchanged. No new public helper or duplicate Variant model was added; the native decoder stays behind the package facade. Public exports and the generated stub remain aligned. |
| TypeScript terminal and declarations | `wyrd-sdk-ts/native/src/lib.rs:422-443`; `wyrd/src/index.ts:645-683,900-945`; generated `index.d.ts`/`index.d.cts`; `oracle-query.test.ts:472-539`; drift and OTEL consumer journeys | The facade detects the standard extension metadata, recursively walks Struct/List values, and delegates bytes to the existing Rust Variant owner. napi's installed `serde-json` projection returns unsafe-range integers as `bigint`; the journey proves `9007199254740993n` and `2n ** 60n`. Raw `toArrow()` retains the extension. Native declarations and public TypeScript source agree. |
| MCP JSON result and stable failure | `wyrd-server/src/mcp/bifrost.rs:639-674`; `wyrd-mcp/tests/bifrost/mcp/query.rs:740-805` | MCP installs the same encoder on Arrow's JSON path. Its real MCP-to-server journey observes a whole Variant object as JSON, preserves an integer beyond 2^53 in the JSON value, exercises Struct/nested access, and receives the exact invalid-JSON catalog code. |
| Error transport parity | `wyrd-client/src/error.rs:185-223,415-431`; Python/TypeScript `WyrdError` projection; Rust/Python/TypeScript/MCP query journeys | HTTP and gRPC reconstruction deserialize the derive-backed `BifrostError` payload and verify its code before exposing it. The focused exact round-trip test passed locally, and every language journey checks `WYRD_VALA_400_VARIANT_INVALID_JSON`. Other locked Variant error codes and detail fields are pinned by `variant_contract_and_builtin_schemas_are_stable`; language boundaries consume the same generic catalog projection rather than maintaining per-language copies. |
| V14/V15 documented deferral | Task verification items 14-15 and implementation evidence; Python canonical journey `test_bifrost_query.py:299-453`; TypeScript canonical journey `oracle-query.test.ts:338-469`; Rust canonical Arrow journey `pg_bifrost_e2e.rs:2464-2685` | The deferral is accurately implemented: Python and TypeScript use stock OTLP exporters to produce spans, logs, and metrics, then query native Variant values through their real SDKs. They do not claim or implement direct Variant Arrow authoring. Canonical Arrow write equivalence remains exercised through the Rust shared-client journey, and TASK-002 owns restoring direct Python/TypeScript Arrow writes. No fixture-only encoder or premature JSON-text normalization was introduced. |

## Public behavior and journey assessment

| Obligation | Evidence | Result |
|---|---|---|
| Arrow terminals retain `arrow.parquet.variant`; typed terminals return language-native values without integer narrowing. | Rust, Python, and TypeScript Variant journeys inspect the extension and decode objects, arrays, nulls, absent keys, and unsafe-range integers. MCP checks native JSON rendering. | PASS |
| Struct access remains exact while Variant access stays semantic, including Variant nested in persisted Struct/List shapes. | Query journeys exercise `events[1]['name']`, nested event attributes, whole top-level Variant columns, and parsed Variant values; the drift consumer journeys read the whole `drift_report.features` nested Variant. | PASS |
| Invalid `parse_json` is the exact stable Wyrd error and `try_parse_json` is null on every public result surface in scope. | Rust, Python, TypeScript, and MCP real-server journeys assert the exact code and lenient null; the client catalog round-trip preserves fields and metadata. | PASS |
| Generated schemas, declarations, exports, and error-code unions match the Rust contract. | `mise run codegen:check` passed; inspected generated schemas, Python stubs, native TS declarations, and generated TS error-code union. | PASS |
| Required tests are real user/agent journeys, not in-process substitutes. | Each named language/MCP test boots or receives `WyrdTestServer`, writes through the canonical SDK or approved stock OTLP producer, flushes/publishes, queries through Oracle, and decodes at the public client boundary. | PASS |

## Verification

Independently run during this review:

- `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=error::tests::bifrost_problem_details_reconstruct_exact_variant)'` — PASS (1 test).
- `mise run codegen:check` — PASS; schemas and Python stubs regenerated cleanly.
- `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..3cf911fce699bbfe197f8b95e72b13e2f551f766` — PASS.

The task's implementation evidence records successful execution on the final candidate of the exact Rust, Python, TypeScript, and MCP journeys (V5-V8), the two approved canonical-signal deferral journeys (V14-V15), and the affected drift, observe, and OTEL consumer journeys. I did not rerun those Postgres-backed multi-language journeys; this review traced their setup, public call path, assertions, and ownership against source and used the recorded successful results as the available execution evidence.

## Novelty and drift audit

No `DRIFT` finding. The candidate adds no SDK-specific Variant model, compatibility alias, alternate wire contract, fixture encoder, cache, configuration switch, or new testing mechanism. The only language-boundary adapters use the repository's existing PyO3/napi projections, Apache Arrow extension metadata and IPC, Arrow's installed encoder hook, and the already-approved `parquet-variant` conversion owner. Those are required by the approved contract and follow established native-extension patterns; no novel mechanism absent both repository practice and comparable Arrow SDK practice is required for remediation.

## Findings

None.

## Verification limits

- Direct Python and TypeScript authoring of Variant Arrow columns is intentionally not proof supplied by TASK-001. V14/V15 use stock OTLP production and native SQL reads; the task explicitly assigns direct language Arrow authoring to TASK-002 scenario 5. This is an approved scope boundary, not a substitute claim or a finding.
- TASK-003 owns shredded physical schemas and nested-leaf projection/pruning. This review assessed only TASK-001's logical unshredded extension and public result contract.

## Overall result

**PASS** — the candidate's cross-language public Variant contract, native result decoding, stable error projection, generated artifacts, exports/types, and required user/agent journey evidence satisfy TASK-001 within its documented V14/V15 deferral. No material SDK-parity finding remains.
