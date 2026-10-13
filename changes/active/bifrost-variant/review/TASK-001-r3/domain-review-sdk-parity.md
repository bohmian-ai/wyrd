# Domain Review: SDK and transport parity

## Subject

- Domain: first-class Rust, Python, and TypeScript SDKs; HTTP, gRPC, and MCP query projections; generated contracts and stable errors.
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`.
- Candidate: `555308ba14058ddc56102d2f925298ef43858175`.
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11.
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`.
- Remediation inputs: `TASK-001-R1-close-variant-contract-gaps.md` and `TASK-001-R2-exact-integers-and-late-errors.md`.
- The candidate was the checked-out `HEAD` before and after this review.

## Reviewed boundary

The review traced the cumulative public path from the `wyrd-spec` query and
problem contracts through Oracle terminal construction, protobuf conversion,
HTTP and gRPC serving, `wyrd-client` decoding and error reconstruction, the
Python and TypeScript bindings and packages, MCP/scheduled collection, and the
real Rust, Python, TypeScript, distributed, and MCP journeys. It also checked
the revision-11 integer decision and the generated error/declaration surfaces.

## Authority and source coverage

| Boundary | Authority and source inspected | Assessment |
|---|---|---|
| Contract and error ownership | `AGENTS.md` §§2–3, 8–12; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/bifrost-design.md` public-surface, Variant-SQL, distributed-terminal, and non-goal sections; `architecture/references/{architecture/patterns,languages/errors,languages/maintainer-style,languages/spec-driven-development}.md`; `crates/wyrd-spec/src/{error,vala/api,vala/error}.rs` | `QueryTerminalFrame` owns the complete boxed RFC 9457 `WyrdProblem`; its validation requires an error exactly for failed outcomes and excludes an Arrow EOS from failures. The six Variant failures and `DataTypeSpec::Variant` remain derive-backed catalog/contracts, with no PyO3 or runtime dependency in `wyrd-spec`. |
| Oracle producer and distributed reconstruction | `crates/vala/vala-bifrost-redux/src/oracle/{mod,query_stream,variant_sql,analytical,codec,peer}.rs`; `architecture/bifrost-design.md` | Pre-stream and late DataFusion failures use the same `map_datafusion_error`; late terminals call `WyrdError::problem()` and retain the selected query class. Local typed errors and the existing distributed external-error carrier reconstruct the same tagged `BifrostError`; unknown execution failures remain `WYRD_VALA_500_QUERY_EXECUTION_FAILED`. No retry, fallback, second query engine, or partial-success terminal was introduced. |
| Protobuf and gRPC | `crates/wyrd/wyrd-tonic/proto/{wyrd.v1.proto,wyrd.v1.bin}`; `crates/wyrd/wyrd-tonic/src/query_conversion.rs`; `crates/wyrd/wyrd-server/src/grpc/query.rs` | Field 9 carries the serialized full problem while removed fields/names remain reserved. Conversion rejects malformed problem bytes and invalid terminal combinations, round-trips the problem unchanged, and the server projects the same logical frames over gRPC and HTTP. The regenerated descriptor is present. |
| HTTP and shared Rust client | `crates/wyrd/wyrd-server/src/query/{routes,service,scheduled}.rs`; `crates/shared/wyrd-client/src/bifrost/{facade,query}.rs`; `crates/shared/wyrd-client/src/error.rs`; `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs` | `wyrd_client::Bifrost` remains the sole SDK-facing owner. The converter validates the full stream and row count, retains a failed terminal only after clean EOF, rebuilds its problem through the same catalog mapper as pre-stream HTTP/gRPC errors, and `collect_bounded` drops retained batches on error. Raw Arrow terminals retain the Variant extension; typed/JSON terminals use the shared decoder. |
| Python projection | `architecture/references/languages/{pyo3-boundaries,python-api-and-stubs}.md`; `sdks/wyrd-sdk-python/src/bifrost/mod.rs`; `sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.py`; `sdks/wyrd-sdk-python/tests/integration/test_bifrost_query.py` | Both sync and async facades call the same native `wyrd-client` owner. A late terminal is retained for diagnostics and raised through the single `WyrdError` projector; collected `sql()` returns no partial result. The real-server journey proves exact `u64::MAX`, the complete early/late Variant problem, an unrelated generic late failure, and no collected partial result. Interactive-only Python proof is the approved harness limit. |
| TypeScript projection and declarations | `architecture/references/languages/typescript-guide.md`; `sdks/wyrd-sdk-ts/native/src/lib.rs`; `sdks/wyrd-sdk-ts/wyrd/src/{index,error-codes}.ts`; generated `index.d.ts` and `index.d.cts`; integration and unit query tests | The napi boundary projects the shared Rust error metadata without message parsing; the public `WyrdError` preserves code, status, title, detail, remediation, and details. Collected `sql()` returns nothing on a late failure, while raw Arrow results keep the extension. `u64::MAX` becomes exact `bigint`. Generated native declarations and the generated catalog-code union contain the changed surface. Interactive-only TypeScript proof is the approved harness limit. |
| MCP and server-side collected consumers | `architecture/references/languages/agent-harness.md`; `crates/wyrd/wyrd-server/src/mcp/bifrost.rs`; `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs`; scheduled query consumer | MCP validates the same terminal, maps the same full problem, and abandons locally retained rows on failure. Its real agent journey covers native Variant/Struct JSON, exact `u64::MAX`, and stable Variant refusal. Scheduled consumption uses the same terminal mapper and yields no outcome on failure. |
| Journey and generation evidence | TASK-001 V5–V9 and V16; R2 implementation evidence; Rust `pg_bifrost_e2e`, Python `test_bifrost_query.py`, TypeScript `oracle-query.test.ts`, distributed `published.rs`, MCP query journey, tonic/client unit tests | The evidence crosses real SDK/server boundaries. Rust V9 proves both Interactive and Analytical late failures through shared `wyrd-client` reconstruction; Python and TypeScript prove the same projection on their single-pod Interactive harnesses. `codegen:check`, format/lint/typecheck lanes, and `git diff --check` are recorded passing. |

## Obligation assessment

| Obligation | Evidence | Result |
|---|---|---|
| Variant and error contracts are identical across public surfaces. | Pure `wyrd-spec` contract, generated schemas/codes/descriptors, one shared Rust client owner, thin Python/napi boundaries, MCP using the same catalog mapper. | PASS |
| Every accepted integer reads back exactly without `serde_json/arbitrary_precision`; wider integers are refused. | Shared lexical `RawValue` classification accepts through `u64::MAX`, journeys assert `u64::MAX` in Rust, Python, TypeScript, and MCP, and focused encoder/Oracle tests refuse both out-of-range sides. Cargo features keep `arbitrary_precision` off. | PASS |
| A late failure carries the full catalog problem over HTTP and gRPC in Interactive and distributed execution. | `QueryTerminalFrame.error: Option<Box<WyrdProblem>>`; protobuf field 9; tonic round trip; Oracle local/peer terminal tests; HTTP/gRPC frame parity; Rust Interactive and Analytical real-server journeys. | PASS |
| Every SDK raises the same catalog error and does not return a collected partial result. | `wyrd-client::error::from_problem`; bounded collection error path; Python native `client_error`; TypeScript `projectedError`; MCP/scheduled terminal mapping; late-failure journeys compare early and late code/status/detail/details and exercise unrelated generic failure. | PASS |
| Raw Arrow and native typed values retain their required representation. | Shared Arrow IPC decoder and Variant decoder; Rust/Python/TypeScript/MCP journeys cover extension metadata, nested values, exact integers, null, Struct, and JSON functions. | PASS |
| No unrelated or bespoke public surface entered the task. | No new client, Variant model, public option, compatibility alias, error code, or alternate transport. The terminal reuses the existing RFC 9457 catalog problem and protobuf/Arrow transport mechanisms. | PASS |

## Material proposed findings

None.

## Prior-finding closure

- `FIND-TASK-001-5`: closed for this boundary. Distributed Variant failures
  recover the tagged catalog value rather than parsing human display prose,
  and every public terminal is subsequently built from the ordinary
  derive-backed problem.
- `FIND-TASK-001-7`: closed. The `QueryResult` documentation is attached to
  the exported TypeScript class.
- `FIND-TASK-001-11`: closed. Only `i64`/`u64` integer values are accepted;
  `u64::MAX` has exact multi-language journey coverage and
  `arbitrary_precision` remains disabled.
- `FIND-TASK-001-12`: closed. The terminal carries the complete problem and
  shared reconstruction retains its catalog identity after partial batches.

## Verification limits

- I inspected the complete cumulative source and recorded evidence but did not
  rerun the Postgres-backed multi-language or multi-pod journeys. Their exact
  final-candidate runs are recorded in the task and R2 evidence.
- Python and TypeScript late-failure journeys are Interactive-only because
  their accepted harnesses start one Oracle pod. This is the binding stated
  limit: Rust V9 proves Analytical execution on the real multi-pod path, and
  all three languages use the same `wyrd-client` reconstruction.
- Streaming APIs necessarily may yield batches before learning of a late
  failure; architecture permits incremental processing but declares those
  frames unusable as a complete result. The collected Rust/Python/TypeScript,
  MCP, and scheduled terminals return no partial successful result.
- Direct Python/TypeScript Variant Arrow authoring remains TASK-002 scope;
  TASK-003 owns shredded physical layouts and leaf pruning.

## Drift audit

No material `DRIFT` was found. The changed boundary uses standard Arrow IPC,
protobuf framing, RFC 9457 problem documents, ordinary language exceptions,
and the existing shared Wyrd catalog/client mechanisms. It adds no custom
check, setting, option, side channel, compatibility file, or parallel SDK
transport. No remediation should introduce one.

## Overall result

**PASS** — the cumulative candidate satisfies SDK, HTTP/gRPC, MCP, generated
contract, exact-integer, and late-terminal parity for this domain, with the
approved Python/TypeScript Interactive-only proof limit.
