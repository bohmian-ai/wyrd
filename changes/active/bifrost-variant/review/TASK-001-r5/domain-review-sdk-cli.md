# TASK-001 r5 — SDK, CLI, MCP, and late-error domain review

## Review Findings

### Critical

None.

### Important

None.

### Suggestions

None.

## Open Questions

None.

## Immutable subject

- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`
- Candidate tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 12
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation: `changes/active/bifrost-variant/review/TASK-001-r4/TASK-001-R4-close-final-variant-contract-gaps.md`
- Review scope: cumulative base-to-candidate behavior, with the round-4 CLI gap and prior exact-integer/late-error findings rechecked against the final source.

The candidate commit resolves to the supplied tree. `HEAD` remained the supplied
candidate and tree at the end of this review. Other reviewers' untracked files
under `TASK-001-r5/` do not alter the reviewed source.

## Reviewed boundary

This review traced the public Variant and query-error path from the derive-backed
wire contracts through HTTP and gRPC frames, the shared Rust `Bifrost` stream and
collector, Python and TypeScript projections, MCP collection, and compiled CLI
JSONL output. It specifically rechecked exact JSON shape and numeric spelling,
complete late-failure problem preservation, collected-result discard, and the
documented split between single-pod language journeys and the Rust multi-pod
distributed journey.

## Authority and source coverage

| Boundary | Source and authority evidence | Result |
| --- | --- | --- |
| Public parity and ownership | `AGENTS.md` §§2–3, 8–11; `architecture/wyrd-design.md` client model; `architecture/bifrost-design.md` Variant SQL, terminal contract, public surface, and non-goals; spec REQ-003, REQ-004, REQ-017–REQ-019 and AC-001, AC-003–AC-005; language error, testing, Python, TypeScript, and agent-harness references | PASS — `wyrd_client::Bifrost` remains the one client owner. No SDK-specific transport, Variant model, late-error code list, or compatibility path was added. |
| Compiled CLI Variant JSON | `crates/wyrd/wyrd-cli/src/query/mod.rs:148-170`; `crates/wyrd/wyrd-cli/tests/query_server_journey.rs:63-161` | PASS — the CLI uses the shared `VariantJsonEncoderFactory`. The existing compiled-binary journey now queries a native Variant object and compares the raw JSONL line, proving the object is not the physical storage Struct, `u64::MAX` keeps all digits, and a double remains spelled `3.0`. This closes prior `FIND-TASK-001-19`. |
| Rust collection and partial-result handling | `crates/shared/wyrd-client/src/bifrost/query.rs:750-825`; `crates/shared/wyrd-client/src/error.rs:143-231`; focused client tests at `query.rs:2053-2084,2538-2604` | PASS — a failed terminal is accepted only after clean EOF, retains the complete terminal, and becomes an error. `collect_bounded` owns batches privately and returns only the error on any late failure, so no collected partial `QueryResult` escapes. The common mapper reconstructs typed `BifrostError` details and the full catalog before using the unknown-code fallback. |
| HTTP/gRPC envelope parity | `crates/wyrd/wyrd-tonic/src/query_conversion.rs:456-501,576-587`; `crates/wyrd/wyrd-server/src/query/routes.rs:807-856`; `crates/wyrd/wyrd-server/src/grpc/query.rs:570-623` | PASS — failed protobuf terminals carry serialized `WyrdProblem`, reject malformed terminal combinations, and round-trip code and details. HTTP and gRPC use the same domain frames; focused parity proof includes a batch followed by a failed terminal. |
| Python projection | `sdks/wyrd-sdk-python/src/bifrost/mod.rs:307-327`; `sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.py:653-701,739-813`; `sdks/wyrd-sdk-python/tests/integration/test_bifrost_query.py:455-606` | PASS — raw Arrow retains the extension; typed rows recursively produce native `dict`, `list`, scalar, and exact integer values. The real-server journey compares early and post-8192-row failures by `code`, `status`, `detail`, and `details`; collected `sql()` raises instead of returning retained rows. |
| TypeScript projection | `sdks/wyrd-sdk-ts/native/src/lib.rs:930-1018`; `sdks/wyrd-sdk-ts/wyrd/src/index.ts:893-940`; `sdks/wyrd-sdk-ts/wyrd/tests/integration/oracle-query.test.ts:472-610` | PASS — napi projects the shared Rust problem fields; the wrapper's collector returns no `QueryResult` when iteration raises. The real-server journey proves native values, `bigint` at and beyond the JavaScript safe-integer range, full late-error identity, and no collected partial result. |
| MCP JSON and failure collection | `crates/wyrd/wyrd-server/src/mcp/bifrost.rs:520-625,628-655`; `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:720-803` | PASS — MCP retains rows privately, converts a failed terminal to its originating catalog problem before producing a tool result, and uses the same Variant JSON encoder. Its agent journey proves native nested JSON and exact `u64::MAX`. |
| Interactive and distributed late failures | `crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs:1213-1247,1290-1374`; Python and TypeScript journeys above | PASS — the Rust multi-pod journey forces both Interactive and Analytical late Variant failures after at least one emitted batch and compares the complete problem while returning no collected result. Python and TypeScript exercise the same shared client reconstruction on their accepted single-pod Interactive harnesses. No requirement makes each language harness independently provision a distributed cluster. |
| Python RBAC assertion | `sdks/wyrd-sdk-python/tests/integration/bifrost/test_bifrost_e2e.py:350-366`; `architecture/references/languages/errors.md` | PASS — the integration test now branches on `WyrdError.code` instead of matching a catalog code in human message text, and still proves the denied batch creates no row. This is the contract-preserving correction recorded in the remediation evidence. |

## Verification Notes

- PASS — compiled CLI real-server journey:
  `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E "test(=query_server_journey::query_command_reads_seeded_table)"'`
  (1 passed). An initial attempt used the source filename as the Cargo test
  target; Cargo correctly reported that the actual target is `cli`, after which
  `cargo nextest list` confirmed the exact selector above.
- PASS — shared-client late-terminal reconstruction and partial-result tests:
  `failed_terminal_problem_rebuilds_its_catalog_error`,
  `resource_terminal_rejects_partial_rows`, and
  `bifrost_query_failed_terminal_is_retained_and_rejected` (3 passed).
- PASS — tonic failed-problem round trip:
  `query_conversion::tests::failed_terminal_problem_round_trips` (1 passed).
- PASS — server HTTP/gRPC late-terminal parity:
  `http_query_stream_preserves_late_failed_terminal` and
  `http_and_grpc_query_frame_parity_covers_degraded_and_late_failed` (2 passed).
- The review did not rerun the heavier Python, TypeScript, MCP, or multi-pod
  journeys. Their final-candidate executions are recorded in the task and R4
  remediation evidence; this pass inspected their complete assertions and the
  shared owners they reach. The independently rerun Rust transport/collector
  slices cover the common error envelope those projections consume.
- Streaming APIs may deliver batches before a late failure by design; those
  batches are not a complete successful result. The no-partial-result obligation
  was verified for the collecting Rust/Python/TypeScript and MCP surfaces.

## Overall result

**PASS.** No material SDK, CLI, MCP, JSON-rendering, or late-error finding
remains. The round-4 CLI journey gap is closed, exact integer and `3.0`
rendering are proved at the compiled binary, full catalog problems survive
interactive and distributed late failures over the shared HTTP/gRPC contract,
and collecting public surfaces return no partial successful result.
