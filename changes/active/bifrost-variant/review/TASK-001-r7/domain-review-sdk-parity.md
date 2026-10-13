# TASK-001 r7 — SDK, CLI, MCP, and user-journey parity review

## Reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`.
- Immutable base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`.
- Immutable candidate: `6147cc617d81f2c03464043be698ab2565e9d745`.
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`.
- Authority: approved `changes/active/bifrost-variant/spec.md` revision 13,
  original TASK-001, `AGENTS.md` testing rules, `architecture/wyrd-design.md`,
  `architecture/wyrd-doctrine.mdx`, and `architecture/bifrost-design.md`.
- Scope: Rust, Python, TypeScript, HTTP/gRPC, MCP, and CLI Variant projection;
  exact `u64::MAX` and `3.0`; full late catalog errors; collected-result
  discard; generated declarations; public journeys; and the R6 OTLP
  non-finite behavior change.

The candidate and tree matched the assigned identity at the start of review.
I reviewed the cumulative base-to-candidate behavior and the R6 remediation
evidence; I did not modify production or test source and did not consume
another r7 reviewer's conclusions.

## Boundary trace and current coverage

| Public contract | Source and proof | Result |
| --- | --- | --- |
| One shared client owner | Rust `Bifrost` owns HTTP query collection and gRPC ingest; Python and TypeScript wrap it at their FFI boundaries rather than duplicating transport or terminal handling. | PASS |
| Native Variant values | `VariantJsonEncoderFactory`/`variant_bytes_to_json` are the shared decoding owners; Rust typed rows produce `serde_json::Value`, Python recursively produces native values, TypeScript recursively produces values with wide integers as `bigint`, and MCP/CLI reuse the same rendering path. | PASS |
| Exact integers | The Rust, Python, TypeScript, and MCP real-server Variant journeys assert `u64::MAX` exactly; TypeScript asserts `2n ** 64n - 1n`; the compiled CLI compares the literal decimal digits. | PASS |
| Floating rendering | The compiled CLI journey compares `3.0` byte-for-byte, and the shared renderer covers every JSON-producing surface. | PASS for accepted finite doubles |
| Arrow terminal | Rust, Python, and TypeScript journeys assert the `arrow.parquet.variant` extension remains present rather than being replaced by JSON text. | PASS |
| Late catalog identity and partial-result handling | The shared collector validates the terminal after clean EOF and returns an error without its accumulated batches; Rust, Python, and TypeScript journeys force one 8,192-row batch before catalogued and uncatalogued failures, compare the exact error/fallback, and prove collecting APIs reject instead of returning partial results. | PASS |
| HTTP/gRPC parity | Server tests project the same domain `QueryStreamFrame` through both transports and round-trip the terminal `WyrdProblem`; the Rust multi-pod Oracle journey owns distributed proof while Python/TypeScript intentionally exercise Interactive in their single-pod harnesses. | PASS; accepted coverage split |
| MCP | The real MCP journey proves native Variant JSON and exact `u64::MAX`; collector tests prove a failed terminal becomes a tool error without a partial success payload. | PASS |
| CLI | The compiled CLI journey proves native JSON, `u64::MAX`, and `3.0`; a streamed CLI cannot retract stdout already emitted, but a failed terminal remains a non-success with the catalog problem and is never reported as a complete result. | PASS |
| Generated surfaces | Generated TypeScript declarations expose the native Variant decoder, Python exports the `DataTypeSpec` projection, generated schemas contain `DataTypeSpec::Variant`, and recorded `codegen:check` is green. | PASS |

## Finding

### SDK-R7-001 — R6 silently changed OTLP non-finite doubles from preserved values to rejected records

- **Classification:** REGRESSION / unresolved product-contract conflict.
- **Violated obligation:** REQ-004 says a written floating-point value keeps its
  IEEE meaning (`spec.md:634-643`); revision 13 explicitly narrows raw decimal
  forms but never says NaN or infinities are refused (`spec.md:660-665`).
- **Exact source:** `crates/shared/wyrd-queue/src/variant.rs:946-949,1011-1016`
  rejects every non-finite raw Float/Double; all OTLP attributes and bodies
  enter that gate through `tables/signal.rs:84-140`.
- **Reachability and user effect:** OTLP protobuf represents doubles directly.
  `project_resource_logs` turns a Variant failure into a rejected record and
  standard OTLP partial success (`tables/logs/projection.rs:40-105`,
  `otlp_contract.rs:57-78`); the same shared resource/scope/attribute builders
  affect spans and metric metadata/exemplar attributes. The R6 record itself
  acknowledges that spans, logs, and metrics containing NaN or infinity are
  now rejected per record.
- **Evidence that this is a behavior change:** the pre-existing maximal log
  fixture deliberately used a NaN with payload to prove bit-exact preservation;
  R6 replaced it with `-0.0` after the new validator rejected it
  (`tables/logs/mod.rs:43-66`) and added a unit test that blesses rejection
  (`:170-203`). Changing the proof does not authorize changing REQ-004.
- **Observable consequence:** a conforming OTLP producer can receive partial
  success and lose an entire span, log record, or metric point solely because
  an attribute or body contains a non-finite IEEE double that the approved
  contract says keeps its meaning.
- **Decision required:** this is not safely remediable as a local SDK/test
  choice. Either preserve non-finite Float/Double on Arrow/storage paths and
  explicitly define their JSON-terminal representation, or revise the approved
  spec to declare them outside the Variant domain and authorize OTLP per-record
  refusal. The current candidate and current spec cannot both be accepted.
- **Focused closure proof after that decision:** a real OTLP HTTP or gRPC
  journey for each affected signal must send a non-finite value, assert the
  approved ACK/partial-success behavior, flush, and assert the approved SQL and
  JSON/Arrow terminal behavior; retain the existing finite `3.0`, `-0.0`, and
  sibling-record isolation cases.

## Test Coverage Analysis

### Current Coverage

- Rust, Python, TypeScript, MCP, and CLI journeys cover native finite Variant
  projection, exact wide integers, early/late errors, and collected-result
  refusal.
- Rust multi-pod Oracle coverage proves distributed error identity; the
  single-pod language journeys validly prove the same shared Interactive
  projection.
- The R6 unit test proves only that the current log projector rejects a NaN
  body while retaining a finite sibling; it does not establish that rejection
  is the approved product behavior.

### Gaps

- `tables/logs/mod.rs:170-203` — unit-only NaN rejection replaces the old
  preservation fixture; no client → OTLP edge → Scribe → query journey proves
  the user-visible contract or partial-success response.
- Span attributes/resource/scope attributes and metric metadata/exemplar
  attributes use the same gate, but no focused non-finite journey covers those
  sibling consumers.
- No proof defines what a preserved non-finite value should do on HTTP, MCP,
  CLI, Python, or TypeScript JSON/native terminals; that absence is exactly why
  the product decision must precede implementation remediation.

### Recommended Verification

- `mise run test:bifrost:journey:otlp` (or the narrowest existing OTLP journey
  leaves selected through `mise`) — prove the approved non-finite behavior at
  the real protocol and publication boundary for spans, logs, and metrics.
- Retain the task's exact Rust/Python/TypeScript/MCP Variant journey selectors
  and compiled CLI journey after the decision — prove finite `3.0`, exact
  `u64::MAX`, error identity, and terminal behavior did not regress.
- `mise run codegen:check` — required only if the resolved public contract
  changes generated declarations or error documentation.

### Residual Risk

Until the non-finite contract is decided, green unit and repository lanes only
prove that the new rejection is internally consistent; they do not prove that
silently dropping OTLP records is the intended user behavior.

## Verification limits

I performed source and recorded-evidence review only. I did not rerun the
serialized Postgres, multi-pod, Python, TypeScript, MCP, CLI, or code-generation
lanes. Their existing evidence is credible for all retained finite-value and
terminal contracts, but cannot resolve the authority conflict above.

## Overall result

**FAIL.** SDK/CLI/MCP parity, exact finite rendering, and late-error behavior
remain sound, but the cumulative candidate introduces one user-visible OTLP
rejection that is not authorized by the approved specification and needs an
explicit product-contract decision before TASK-001 can pass.
