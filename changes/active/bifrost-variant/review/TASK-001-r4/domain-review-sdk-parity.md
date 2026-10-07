# TASK-001 r4 — SDK and transport parity domain review

## Review Findings

### Critical

None.

### Important

#### SDK-PARITY-R4-001 — The compiled CLI has no user-journey proof for its new Variant JSON behavior

- **Classification:** MISSING (required user-facing verification)
- **Violated obligation:** REQ-018 requires every JSON-rendering surface to emit a
  Variant as its JSON value (`changes/active/bifrost-variant/spec.md:819-828`).
  The repository test authority says every new user/agent-facing capability needs
  a journey test and that lower-tier proof cannot substitute for it
  (`architecture/references/languages/testing-workflows.md:11-38`).
- **Location:** `crates/wyrd/wyrd-cli/src/query/mod.rs:148-170` changes the compiled
  CLI's JSONL writer to install `VariantJsonEncoderFactory`, while
  `crates/wyrd/wyrd-cli/tests/query_server_journey.rs:63-122` queries only the
  primitive `id` and `value` columns.
- **Evidence:** shared-encoder unit coverage and the Rust/Python/TypeScript/MCP
  journeys prove adjacent consumers, but no CLI test executes the compiled binary
  against a Variant result. The existing CLI journey parses stdout as JSON but
  cannot distinguish a correct native Variant value from a regression to Arrow's
  storage representation, and it does not exercise the `3.0` double-rendering
  requirement.
- **Observable consequence:** a regression in CLI field-metadata propagation,
  encoder installation, binary wiring, or JSONL serialization could pass all
  current TASK-001 proof while producing the wrong public CLI output.
- **Testable correction:** extend the existing compiled-CLI server journey in
  `query_server_journey.rs` (reusing its server and command helpers) with a query
  returning a Variant object containing both `3.0` and
  `18446744073709551615`. Assert that stdout contains the native object rather
  than the Variant storage struct, that the integer is exact, and that the raw
  JSON text retains `3.0`. Run the existing repository-managed CLI journey lane
  or its exact `mise exec -- cargo nextest` selection with the required Postgres
  setup.

### Suggestions

None.

## Immutable subject

- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `a6429060fb011aafa4335f2f736c70adab231739`
- Candidate tree: `64177d67141993ec18e63b43dc227dbc31d70950`
- Specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Review scope: the full cumulative base-to-candidate diff

The supplied candidate tree was independently matched to the candidate commit. The
checked-out source differs from that candidate only in review-skill material outside
this domain.

## Reviewed boundary

This review traced the shared Rust Bifrost client as the sole SDK-facing owner;
HTTP and gRPC query frames and their `wyrd-tonic` conversions; local and
distributed terminal-error reconstruction; late-failure settlement; Rust,
Python, TypeScript, MCP, and CLI Variant rendering; generated/public declarations;
and the available unit, integration, and user-journey proof for those surfaces.

## Authority and source coverage

| Boundary | Evidence reviewed | Result |
| --- | --- | --- |
| Shared Rust client ownership and settlement | `crates/shared/wyrd-client/src/bifrost/query.rs`; `crates/shared/wyrd-client/src/error.rs`; SDK wrappers and imports | PASS — collection remains private until a successful terminal; a late failed terminal returns an error rather than a partial `CollectedQueryResult`. Streaming callers may already have consumed earlier batches, as intended by the streaming contract. |
| Exact catalog reconstruction and fallback | `crates/shared/wyrd-client/src/error.rs`; `crates/vala/vala-bifrost-redux/src/oracle/mod.rs`; `crates/vala/vala-bifrost-redux/src/query_stream.rs` | PASS — known Variant/Bifrost problems reconstruct the exact catalog variant from structured details, generic catalog errors remain generic, and unknown upstream codes retain the original code/details in `UpstreamFailure`. The distributed path prefers typed/source-chain recovery before its generic execution fallback. |
| HTTP/gRPC query frames | `crates/wyrd/wyrd-tonic/src/query_conversion.rs`; `crates/wyrd/wyrd-server/src/query/routes.rs`; `crates/wyrd/wyrd-server/src/grpc/query.rs` | PASS — both transports use the same frame conversion and carry the complete `WyrdProblem` in failed terminals. Parity tests cover degraded and late-failed sequences. |
| Rust native JSON | `crates/shared/wyrd-queue/src/variant.rs`; `crates/shared/wyrd-client/src/bifrost/query.rs` | PASS — the shared encoder emits the Variant's JSON value rather than its Arrow storage struct and retains semantically significant double rendering such as `3.0`. |
| Python runtime/public API | `sdks/wyrd-sdk-python/src/bifrost/mod.rs`; `sdks/wyrd-sdk-python/python/wyrd`; Python Bifrost integration tests and generated stubs | PASS — sync and async surfaces delegate to the shared Rust owner, recursively decode nested Variant values, retain exact large integers, reconstruct exact late failures, and reject collected partial results. Public imports and stubs are present. |
| TypeScript runtime/public API | `sdks/wyrd-sdk-ts/src`; `sdks/wyrd-sdk-ts/index.d.ts`; `sdks/wyrd-sdk-ts/index.d.cts`; TypeScript integration tests | PASS — wrappers delegate to the shared Rust owner, recursively produce native values, use `bigint` for 64-bit integers, preserve exact late failures, and reject collected partial results. ESM/CJS declarations match. |
| MCP | `crates/wyrd/wyrd-server/src/mcp/bifrost.rs`; MCP Bifrost journey coverage | PASS — rows are collected privately, a failed terminal returns the originating problem rather than a success payload, and the journey proves native Variant JSON including exact large integers. |
| CLI | `crates/wyrd/wyrd-cli/src/query/mod.rs`; `crates/wyrd/wyrd-cli/tests/query_server_journey.rs` | FAIL — the implementation installs the shared Variant encoder, but the compiled-CLI journey exercises only primitive rows and does not prove Variant JSON or double rendering. See `SDK-PARITY-R4-001`. |
| Generated/public declarations | generated JSON schemas, Python stubs, and TypeScript declarations/error-code union | PASS — Variant and the new stable error codes are projected across the reviewed generated/public surfaces; paired declarations are in sync apart from generated-file headers where applicable. |

## Open Questions

None.

## Verification Notes

- Focused Rust test execution was attempted through the repository toolchain, but
  the commands remained blocked on a Cargo build-directory lock held by concurrent
  review work and were interrupted without producing a test result. This is a
  verification limit, not an observed test failure.
- The review therefore inspected the implementation and tests directly and used
  the task packet's recorded final-candidate V1–V17 results as historical evidence;
  those results were not independently reproduced in this pass.
- Python and TypeScript real-server Variant journeys exercise the interactive
  path. The distributed-path split is covered by Rust's multi-pod journey and by
  focused distributed error-identity tests; no claim is made that every language
  journey independently provisions a distributed cluster.
- Streaming APIs can expose batches consumed before a late terminal failure by
  design. The no-partial-success obligation was reviewed for collected SDK calls,
  MCP, and other aggregating paths.
- Direct Python and TypeScript Variant authoring remains outside TASK-001 and is
  deferred to TASK-002; this review does not treat that approved split as a gap.

## Overall result

**FAIL.** The shared implementation, transport framing, catalog reconstruction,
late-failure settlement, public declarations, and Rust/Python/TypeScript/MCP
behavior are coherent within the reviewed boundary. The result remains failing
because the changed compiled CLI JSON surface lacks the repository-required
user-journey proof for native Variant rendering and double preservation.
