# TASK-001 Maintainer Review

Immutable subject:

- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `a6429060fb011aafa4335f2f736c70adab231739`
- Candidate tree: `64177d67141993ec18e63b43dc227dbc31d70950`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11

## Review Findings

### Critical

None.

### Important

- **MNT-001 — The durable completion evidence does not describe the reviewed candidate.**
  - Changed locations: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md:238`, `:256`, and `:285-287`.
  - Governing principle: maintainer-style documentation must explain the current contract and workflow accurately; spec-driven review evidence must identify the immutable subject and reproducible proof it claims.
  - Evidence:
    - The task header binds `spec_revision: 11`, and the candidate specification is revision 11, but the authority list still calls it revision 10.
    - The evidence table says the tested `iceberg-compaction` revision and pin are `94db7b94f72c48c75c937d36a59e80c237e8ca72`; the candidate manifest and lockfile pin `2b65fa189f2d05002acc6e59515a071a63777970`.
    - The recorded placeholder diagnosis says `mask_placeholders` uses `wyrd_queue::variant::is_placeholder`; no such symbol exists in the candidate. `mask_placeholders` contains the placeholder test itself in `crates/shared/wyrd-queue/src/variant.rs:352-388` and is shared directly by the Oracle and JSON encoder paths.
  - Concrete maintenance cost: a maintainer cannot use the task's claimed final evidence to reproduce the fork proof or navigate the implemented fix, and must rediscover which specification, dependency revision, and owner actually shipped. The assertion that all commands ran on the final candidate is not auditable while the recorded revision differs from the candidate pin.
  - Smallest testable correction: update the task authority and diagnosis to revision 11 and the actual `mask_placeholders` implementation; record V13 against `2b65fa189f2d05002acc6e59515a071a63777970` only after that exact fork command succeeds, then keep the tested SHA identical to `Cargo.toml` and `Cargo.lock`.

### Suggestions

None.

## Changed-Surface Coverage

| Surface | Owner and changed symbols inspected | Callers / proof inspected | Maintainer assessment |
|---|---|---|---|
| Variant contract and catalog errors | `wyrd-spec::vala::{api,error}`: `DataTypeSpec::Variant`, limits, `BifrostError` variants, terminal problem carrier; generated schema fixtures | Queue/schema conversion, tonic conversion, client reconstruction, contract/codegen evidence | Typed, derive-backed contract remains in the foundational owner; generated JSON schemas track the source shape. |
| Shared Arrow Variant owner | `wyrd-queue::{variant,schema,batch_builder,error}`: `EncodedVariant`, `VariantViolation`, `VariantColumnBuilder`, `VariantJsonEncoderFactory`, `mask_placeholders`, Arrow/wire conversion | JSON row builder, Vala projections and validators, CLI/MCP/server JSON writers, Python/TypeScript boundary decoders, focused unit tests | Cohesive module with one invariant-bearing value type and one column builder. Helper functions are local, deterministic algorithms; no speculative trait or factory layer was added. |
| Built-in table declarations and validation | `vala-bifrost-redux::tables` declarations, `CanonicalBatchValidator`, `validate_declared_variants`, verification Struct layouts, audit/gateway/eval/agent-trace payload declarations | Scribe ingress/decode validation seam, table-owned validators, stable-schema test, server journey coverage | Validation remains discoverable from each `DomainTable`; the function-pointer seam is a closed table capability rather than a one-implementation trait. |
| OTLP canonical projections | `tables::signal` Variant projection values and shared resource/scope rows; trace/log/metric projection owners and promotion types | Signal table tests and three OTLP journeys, rejection coverage, canonical query readers | Protocol-specific traversal stays beside the signal projection owner and reuses the shared encoded value/column types. Names distinguish source values, promotions, and persisted columns. |
| Verification and audit producers | `ResultPayloadBuilder`, `ResultRun`, `EvalItem`, typed `drift_report`/`eval_summary`; eval observation, gateway capture, audit projection | Table declarations, result unit tests, typed built-in server journey, audit hash recomputation through the staging writer | Stateful/multi-step mapping is owned by concrete builders or existing projection modules. Named-column assembly prevents positional drift and documents null-parent placeholders. |
| Oracle Variant SQL and errors | `OracleVariantSql`, operator planner, UDF implementations, canonical storage helpers, `QueryCatalogError`, terminal problem reconstruction | Leader, analytical, worker/resource session construction; peer/codec paths; local SQL tests and all-session Oracle journey | One concrete registration owner is installed at session construction sites. Stateless UDF helpers remain private and adjacent. Distributed and client errors use the shared catalog rather than parallel message parsing. |
| Scribe, Parquet, catalog, and Forge | built-in admission validator routing, row-group Bloom recipe, v3 catalog creation/validation, promoted Variant group handling, Forge GC/rewrite call sites | Bloom unit proof; repeated-rewrite/GC journey; fork pins and lockfile | Changes extend existing owners and retain the five-field handoff. No second writer, compaction planner, or lineage abstraction entered the repository. The stale fork evidence is the blocking documentation issue above. |
| Public transport and rendering | tonic `error_problem_json`, query conversion, HTTP/gRPC/server query edges, CLI/MCP/HTTP Arrow JSON rendering | Conversion tests, HTTP/gRPC parity tests, MCP and Rust query journeys | Transport adapters remain narrow projections. Variant JSON rendering is reused through `VariantJsonEncoderFactory`; terminal problems reconstruct through the catalog owner. |
| First-class SDKs | Rust query terminal, Python `_validated_rows`/PyO3 decoder, TypeScript `nativeValue`/N-API decoder and generated declarations | Rust, Python, TypeScript built-in and canonical-signal journeys; typecheck/codegen evidence | Foreign-runtime code is limited to walking its Arrow runtime's row representation and invoking the shared Rust decoder. Public typed-row APIs retain explicit result types; generated N-API declarations match the native symbol. |
| Tests, docs, manifests, generated artifacts | Variant unit/contract tests, Oracle and Forge journeys, OTLP and server journeys, SDK/MCP journeys, Bifrost schema documentation, workspace dependencies and lockfile | Task-local V1-V17 evidence and `git diff --check` | Test names describe caller-observable outcomes and helpers own repeated fixture setup. Documentation describes the shipped logical shapes. The task evidence record itself is stale as described in MNT-001. |

## Open Questions

None.

## Calibration Notes

- `crates/shared/wyrd-queue/src/variant.rs` and `oracle/variant_sql.rs` are large, but each has one cohesive responsibility and a clear public owner; splitting them by line count would make navigation worse without reducing coupled invariants.
- The Python private decoder callback and generated N-API `any` return are foreign-runtime boundaries that narrow immediately into public typed-row validation; they do not justify a blocking typing finding.
- Repeated fixture helpers across Rust, Python, TypeScript, and MCP are runtime-specific journey setup, not duplicated durable behavior.

## Verification Notes

- Confirmed the supplied candidate tree exactly matches `64177d67141993ec18e63b43dc227dbc31d70950`; current checkout `HEAD` is later, so source references were read from the candidate commit explicitly.
- Inspected the full cumulative base-to-candidate name/stat diff, all material owner groups above, their production call sites, and their relevant unit/journey tests. CodeGraph was used first for the shared Variant owner, Oracle registry, table validator, and signal projection call paths.
- Independently ran `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..a6429060fb011aafa4335f2f736c70adab231739`; it passed.
- The task records all V1-V17 commands plus format, lint, TypeScript typecheck, consumer journeys, and codegen as passing. This review did not rerun the environment-backed suites. V13 is not accepted as reproducible evidence for the candidate until its recorded tested SHA matches the candidate pin, per MNT-001.

## Overall Result

**FAIL** — production structure and source maintainability pass, but the immutable task's final evidence is materially stale and cannot substantiate the candidate's pinned compaction revision or accurately direct maintainers to the implemented placeholder fix.
