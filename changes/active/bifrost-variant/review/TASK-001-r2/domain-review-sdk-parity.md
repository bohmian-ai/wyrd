# Domain Review: SDK parity and public query contracts

## Subject

- Domain: Rust, Python, TypeScript, HTTP, MCP, generated declarations, and stable-error parity for TASK-001 Variant query results.
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `99c5871ec5ca664b9f54baa379b437ee09d66e95`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10.
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`.
- Remediation task: `changes/active/bifrost-variant/review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md`.
- The candidate was the checked-out `HEAD` before and after this review.

## Authority and source coverage

| Boundary | Authority and source inspected | Assessment |
|---|---|---|
| Shared client and HTTP query path | `AGENTS.md` §§2–3, 8–11; `architecture/wyrd-design.md` client model; `architecture/bifrost-design.md` public surface and Variant SQL; `architecture/references/{architecture/patterns,languages/errors,domain/arrow-analytical-interop}.md`; `crates/shared/wyrd-client/src/bifrost/{facade,query}.rs`; `crates/shared/wyrd-client/src/error.rs`; `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs` | `wyrd_client::Bifrost` remains the one SDK-facing client and its Rust journey crosses the real HTTP query stream. Arrow results retain `arrow.parquet.variant`; typed rows install the shared Variant JSON encoder. HTTP/gRPC problem reconstruction now accepts the existing tagged `BifrostError`, closing prior `FIND-TASK-001-5` without a prose grammar. |
| Python projection | `architecture/references/languages/python-api-and-stubs.md`; `sdks/wyrd-sdk-python/src/bifrost/mod.rs`; `python/wyrd/bifrost/{__init__.py,__init__.pyi}`; `tests/integration/test_bifrost_query.py` | Public imports and generated stubs remain aligned. Raw Arrow returns the extension unchanged; the typed model terminal recursively walks Struct/List fields and delegates Variant bytes to the shared Rust decoder. The real-server journey covers whole/nested Variant values, exact `i64` values beyond IEEE-754 safety, Struct access, lenient parsing, and the stable invalid-JSON code. |
| TypeScript projection | `architecture/references/languages/typescript-guide.md`; `sdks/wyrd-sdk-ts/native/src/lib.rs`; `wyrd/src/{index,error-codes}.ts`; generated `index.d.ts`/`index.d.cts`; `tests/integration/oracle-query.test.ts` | The public wrapper recursively walks Struct/List fields, delegates Variant bytes through the thin napi boundary, and preserves the Arrow extension on `toArrow()`. The real-server journey proves `i64` values beyond JS safe integer range become `bigint`. Remediation reattached the existing `QueryResult` JSDoc, closing prior `FIND-TASK-001-7`. |
| MCP/JSON rendering | `architecture/references/languages/agent-harness.md`; `crates/wyrd/wyrd-server/src/mcp/bifrost.rs`; `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs` | MCP uses Arrow's installed encoder hook with the same `VariantJsonEncoderFactory`; the agent journey covers whole/nested Variant rendering, Struct access, an exact `i64` beyond 2^53, lenient parsing, and the stable invalid-JSON code. No second Variant model or MCP-only decoder was added. |
| Public contracts and generated parity | `crates/wyrd-spec/src/vala/{api,error}.rs`; generated Bifrost schemas; Python generated stubs; TypeScript generated native declarations and `error-codes.ts` | `DataTypeSpec::Variant`, extension documentation, and all six Variant error codes are present in their owning/generated surfaces. The implementation record reports `codegen:check`, Python lint/format, TypeScript typecheck, client-tier, and PyO3-scope checks passing. |
| Result-value fidelity after R1 | Spec REQ-004, REQ-018, INV-002; R1 `FIND-TASK-001-1`; `crates/shared/wyrd-queue/src/variant.rs:222-249,282-329,648-695`; installed `parquet-variant-json-59.3.0/src/to_json.rs:236-299`; all terminal consumers above | R1 correctly admits integral JSON tokens beyond `u64` as scale-zero Decimal16 without enabling rejected workspace `arbitrary_precision`. The result decoder still calls `Variant::to_json_value()`, whose Decimal16 branch falls back to `Value::from(integer as f64)` when the value fits neither `i64` nor `u64`. Rust `sql_as`, Python, TypeScript, MCP, and any Arrow-JSON HTTP result all share that lossy path. |

## Public behavior and journey assessment

| Obligation | Evidence | Result |
|---|---|---|
| Arrow terminals preserve the canonical extension. | Rust, Python, and TypeScript journeys inspect `arrow.parquet.variant`; all boundaries retain IPC/Arrow for raw results. | PASS |
| Typed and JSON terminals decode top-level and nested Variant values consistently. | One shared Rust decoder and Arrow encoder factory feed Rust, Python, TypeScript, MCP, and JSON rendering; journeys cover objects, arrays, null/absent values, Struct nesting, and signed 64-bit integers beyond 2^53. | PASS for covered values; FAIL for an accepted scale-zero Decimal16 beyond `u64` (`SDK-001`). |
| Invalid `parse_json` and lenient `try_parse_json` are stable across public surfaces. | Rust, Python, TypeScript, and MCP real-server journeys assert the stable code and null behavior; client reconstruction accepts tagged `BifrostError`. | PASS |
| Required Rust/HTTP, Python, TypeScript, and MCP evidence is at the user/agent-journey tier. | V5–V8 use the real SDK or MCP client against `WyrdTestServer`; V14–V15 use the approved stock-OTLP production path and native SDK reads. | PASS, subject to the uncovered numeric value class in `SDK-001`. |
| TASK-002 authoring work remains excluded. | No Python/TypeScript Variant Arrow authoring or JSON-text write normalization was introduced; V14/V15 retain the approved stock-OTLP deferral. | PASS |

## Material proposed finding

### SDK-001 — Accepted Decimal16 integers are narrowed through floating point at every native/JSON result terminal

- **Classification:** INCORRECT
- **Violated obligation:** Spec REQ-004 requires a written Variant value to read back with the same type and value and says integers are never converted through floating point; INV-002 forbids narrowing or retyping. REQ-018 requires the public typed and JSON terminals to render native Variant values consistently.
- **Exact location:** `crates/shared/wyrd-queue/src/variant.rs:222-249` (`EncodedVariant::to_json` and `variant_bytes_to_json`), `:282-329` (`VariantJsonEncoderFactory`); consumers at `crates/shared/wyrd-client/src/bifrost/facade.rs:892-923`, `sdks/wyrd-sdk-python/src/bifrost/mod.rs:307-327`, `sdks/wyrd-sdk-ts/native/src/lib.rs:423-444`, and `crates/wyrd/wyrd-server/src/mcp/bifrost.rs:633-675`. The installed standard helper's decisive branch is `parquet-variant-json-59.3.0/src/to_json.rs:285-299`.
- **Evidence:** R1's approved `RawValue` fix classifies an integral token such as `18446744073709551616` (`u64::MAX + 1`) as an exact scale-zero `VariantDecimal16`. `variant_bytes_to_json` immediately calls `Variant::to_json_value()`. That library narrows Decimal16 losslessly only to `i64` or `u64`, then explicitly executes `Value::from(integer as f64)`. Every typed/native terminal and the shared Arrow JSON encoder calls this function. Existing journeys use `9007199254740993` and `2^60`, both still inside `i64`/`u64`, so they cannot detect the fallback introduced into the reachable value space by the remediation.
- **Observable consequence:** Bifrost accepts and stores an exact integer beyond `u64`, but Rust `sql_as`, Python models, TypeScript row parsers, MCP, and JSON-rendering HTTP consumers can receive a rounded floating-point number. The same stored cell therefore changes value and type only at the public result boundary.
- **Required testable correction:** Preserve exact numeric semantics in the shared result-decoding owner for every accepted Variant numeric type, and keep all public projections on that one owner. Do **not** enable `serde_json/arbitrary_precision`; the human decision rejects its workspace-unification breakage. Reuse the installed Variant library's standard lexical JSON writer or another established Arrow/native decimal projection rather than adding a Wyrd-only numeric wrapper, option, or parallel Variant model. If the locked Rust `serde_json::Value` result contract cannot represent the accepted Decimal16 value under those constraints, this correction requires an explicit spec decision rather than silent rounding.
- **Focused closure proof:** Extend the existing Rust/Python/TypeScript/MCP Variant journeys with the same accepted scale-zero Decimal16 value just beyond `u64` and assert exact type/value after query. The raw Arrow assertion remains unchanged. A focused shared-decoder test must cover both positive and negative Decimal16 values outside the 64-bit range so every language journey exercises the same owner.

## Prior-finding closure

- `FIND-TASK-001-5`: closed for this domain. Distributed errors use the existing tagged serde `BifrostError`; the client reconstructs structured details without parsing `Display` prose.
- `FIND-TASK-001-7`: closed. The existing JSDoc is immediately attached to exported `QueryResult`.
- The R1 input-side correction for `FIND-TASK-001-1` is present and follows the human-approved `RawValue` plus local `i128` classification. `SDK-001` is the newly reachable output-side gap exposed by accepting those exact Decimal16 values.

## Verification limits

- I used the task's recorded successful V5–V8 and V14–V15 real-server journeys and inspected their complete public call paths and assertions. I did not rerun the Postgres-backed multi-language lanes.
- I ran `git diff --check` over the immutable base-to-candidate range and inspected `cargo tree` feature resolution; `serde_json/arbitrary_precision` is not enabled, consistent with the human decision.
- Direct Python and TypeScript Variant authoring remains TASK-002 scope and is not a finding.
- TASK-003's shredded physical layouts and leaf pruning remain outside this domain review.

## Novelty and drift audit

No separate `DRIFT` finding. The candidate's SDK adapters reuse Arrow IPC, standard extension metadata, Arrow's encoder factory, and the installed Variant crates. This review does not require a new option, error code, dependency, public wrapper, fixture codec, or Wyrd-specific decimal type. The one finding is loss of an explicitly accepted value at the existing shared result boundary.

## Overall result

**FAIL** — SDK and protocol parity is otherwise complete, but `SDK-001` leaves one accepted Variant numeric class observably narrowed across every typed/JSON public terminal.
