# TASK-001 r6 — SDK, HTTP/gRPC, MCP, and CLI parity review

## Scope

- Immutable base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`.
- Reviewed candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`.
- Reviewed tree: `e3c59991994125dac41187f17de12935e2b45424`.
- Approved authority: `changes/active/bifrost-variant/spec.md`, revision 13;
  original `TASK-001`; remediation packets r1 through r5; `AGENTS.md`;
  `architecture/bifrost-design.md`; and the Arrow, errors, and testing
  references.
- Domain boundary: exact Variant JSON/native-value projection, especially
  `u64::MAX` and a double spelled `3.0`; full late catalog-problem identity;
  and treatment of batches preceding a failed terminal across Rust, Python,
  TypeScript, HTTP, gRPC, MCP, and the compiled CLI.
- Latest-fix boundary: commit `ef92074f0`, which refuses raw Variants whose
  object fields alias the same bytes.

The candidate and tree matched at the start and end of this review. I did not
read another r6 report or modify reviewed source.

## Findings

No material findings.

### Cross-surface trace

| Surface | Source trace and observable result | Assessment |
| --- | --- | --- |
| Shared exact-value owner | `wyrd-queue/src/variant.rs` renders stored cells through upstream `Variant::to_json_value`; `VariantJsonEncoderFactory` feeds that value back as JSON text. This is the single row-as-JSON owner used by Rust typed rows, MCP, and CLI. It preserves `u64` rather than narrowing through `f64`, and its focused tests pin both `u64::MAX` and `3.0`. | PASS |
| Rust SDK | `Bifrost::sql` collects through the terminal-validating shared stream; `QueryResult::deserialize` uses `VariantJsonEncoderFactory` before `serde_json` deserialization. `collect_bounded` owns its batch vector locally and returns only the error when `next_batch` observes a failed terminal, so no partial `QueryResult` escapes. The Rust real-server journey pins the extension, native JSON, exact `u64::MAX`, a batch preceding a catalogued late failure, the complete problem, and whole-result refusal. | PASS |
| Python SDK | The Arrow terminal crosses once as IPC. Typed rows recursively replace every top-level or nested Variant with `variant_bytes_to_json` projected by `json_to_pyobject`, yielding native `dict`, `list`, scalar, and arbitrary-size Python `int` values. Synchronous and asynchronous `sql(..., model)` begin only after the native Rust collector completes, and model validation builds a local list before return. The journey pins `u64::MAX`, exact early/late problem fields, 8,192 streamed rows before failure, and rejection rather than a partial collected result. | PASS |
| TypeScript SDK | N-API `variantToValue` calls the same Rust decoder; the generated JS projection preserves integers beyond the IEEE-754 safe range as `bigint`. `nativeValue` recursively handles Variant values nested in Structs and Lists. `sql` retains batches only in a local array; a failed iterator rejects before a `QueryResult` or typed row array is returned. The journey pins `u64::MAX` as `bigint`, exact early/late problem fields, streamed-prefix visibility, and collected-result rejection. | PASS |
| HTTP and gRPC | Both transports project the same `QueryStreamFrame` sequence. The protobuf terminal carries serialized `WyrdProblem` bytes; `wyrd-tonic` reconstructs and validates it, including emitted-row count and the failed-terminal matrix. A late failure intentionally follows already transmitted batches: the failed terminal makes those frames invalid as a complete result, while collectors above discard their private accumulation. HTTP/gRPC parity tests pin the identical batch-plus-failed-terminal sequence. | PASS |
| MCP | `ResultCollector` keeps rows private, uses `VariantJsonEncoderFactory`, and returns `terminal_error` immediately for a failed terminal; no structured success containing the retained rows is produced. `terminal_error` uses the same shared problem reconstruction as the client. The MCP real-server journey pins native nested JSON and exact `u64::MAX`; the collector test pins a failed terminal's catalog code and no successful partial projection. | PASS |
| CLI | JSONL uses `VariantJsonEncoderFactory`; the compiled-binary journey compares the raw output line and pins `u64::MAX` digits plus `3.0`. A late terminal becomes `CliBoundaryError::Remote` through the shared client, preserving the originating catalog metadata and a non-success exit. The CLI is deliberately a streaming sink, like direct SDK streams and raw HTTP/gRPC: bytes already written to stdout cannot be retracted. It therefore does not claim transactional partial-row discard; it guarantees that a truncated stream is never reported as success. This does not violate REQ-019, whose discard requirement applies to collecting SDK results. | PASS |

### Catalog error identity

The server-side `QueryCatalogError` is the one local/distributed carrier. A
failed terminal contains the complete derive-backed `WyrdProblem`; HTTP and
gRPC preserve it; `wyrd-client::error::from_problem` reconstructs the typed
catalog error from its serialized `BifrostError` details; Python, TypeScript,
CLI, and MCP all project that same result. There is no Variant-only list,
human-message parser, or language-specific error code. An uncatalogued failure
alone falls back to `WYRD_VALA_500_QUERY_EXECUTION_FAILED`.

### Latest raw-write fix and journey scope

`ef92074f0` changes only `EncodedVariant::from_bytes` admission. The iterative
scanner now stops after more visited nodes than value bytes, records the
aliased object graph as malformed, and skips upstream recursive validation once
malformation is established. The correction cannot alter an accepted value:
an honestly encoded node owns at least one byte, and valid inputs still pass
the prior upstream validation and all existing rendering/query paths.

The added `raw_shared_field_values_are_refused` unit test pins the bounded
classification. More importantly, the existing real-server
`verification_runtime::builtin_variant_columns_are_refused_before_ack` journey
now sends the aliased raw IPC shape through the gRPC ingest trust boundary,
asserts `WYRD_VALA_400_VARIANT_INVALID_JSON`, continues through accepted raw
`u64::MAX` data, flushes, and proves only accepted sessions are retained. The
r5 addendum records that exact journey and the queue/Oracle focused lanes as
passing after the final fix.

No new public journey and no rerun of the Rust/Python/TypeScript/MCP query
journeys is required for this final correction. It adds no capability and
changes no accepted encoding, result projection, query terminal, or language
boundary. Duplicating the hostile raw frame in every language would not add a
new path: TASK-001 explicitly defers direct Python/TypeScript Variant Arrow
authoring, while the server must independently reject a client that bypasses
preparation. The real-server raw-IPC journey tests that precise owner. The
previous r5 evidence already records the public V5-V8 journeys green for the
accepted `u64::MAX` and native-value contract.

### Verification evidence and limit

I performed source review only, per review coordination; no Cargo, mise,
codegen, or journey command was started in the shared checkout. I relied on
the r5 packet's recorded final-candidate evidence, including its addendum for
`ef92074f0`. This is a proof-execution limit, not a source-proven parity gap.

## Status

**PASS.** Revision 13's exact Variant domain, native-value rendering, complete
late catalog error, and collected-result discard remain consistent across the
shared Rust owner and all projected surfaces. The final raw-write correction
is covered at its real server trust boundary and does not justify redundant
language-journey work.
