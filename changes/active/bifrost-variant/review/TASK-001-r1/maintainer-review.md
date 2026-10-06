# Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `3cf911fce699bbfe197f8b95e72b13e2f551f766`
- Task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Spec: `changes/active/bifrost-variant/spec.md` revision 10
- Result: **FAIL**

The candidate remained at the stated commit while this review was performed.

## Changed-surface coverage

| Surface | Changed owners, callers, and proof inspected | Maintainer result |
|---|---|---|
| Workspace and dependency shape | Root and crate manifests, lockfile, Arrow Variant dependencies, Iceberg/compaction pins, client-tier placement | PASS: dependencies are limited to the owners that encode, query, or project Variant values; no speculative feature or configuration switch was added. |
| Public contracts and generated artifacts | `wyrd-spec` Variant type/constants/errors, schema snapshots, client error reconstruction, generated TypeScript codes | PASS: the wire type and derive-backed errors have one contract owner and generated artifacts follow it. |
| Queue Variant owner | `wyrd-queue/src/variant.rs`, schema mapping, batch builder, error conversion, all real callers of `EncodedVariant` and its Arrow helpers | FAIL: MR-002 exposes a size-only constructor that bypasses the type's documented invariant and has no caller. |
| Built-in schemas and producers | Canonical field model; verification, gateway, audit, trace, log, metric, eval, and agent-trace projections; server/client producers | PASS: shared Variant/Struct builders replace rather than sit beside legacy encodings, and projection helpers stay with their table owner. |
| Oracle query surface | `OracleVariantSql`, session installation sites, codec and peer digests, planner/UDF helpers, result normalization, planning/execution error mapping, focused and journey tests | FAIL: MR-001 introduces a private text grammar to recover typed errors across the distributed boundary. The session owner itself is cohesive and has multiple production consumers. |
| Iceberg v3 and Forge | Catalog creation/validation, publication lineage validation, worker/compaction/GC callers, fork pins, repeated-rewrite integration fixture | PASS: lineage validation stays at the publication owner and the cumulative journey uses the existing Forge lifecycle instead of adding another rewrite path. |
| Scribe and Parquet | Execution-lane schema handling, promoted Variant layout, shared writer properties, Bloom sizing callers and test | PASS: the previous batch-dependent Bloom calculation was deleted and both writers reuse one recipe. |
| Rust client, server, CLI, HTTP, and MCP | Arrow result rendering, Bifrost error projection, server conversion, CLI and MCP JSON rendering, journey tests | PASS: all row-as-JSON callers reuse the Arrow encoder factory; no parallel terminal model was introduced. |
| Python SDK | PyO3 decoder, public typed-row projection, generated/public package surfaces, Python journeys | PASS: the foreign-runtime boundary calls the Rust decoder and owns only recursive Python value projection. |
| TypeScript SDK | N-API decoder, recursive typed-row projection, generated declarations, TypeScript journeys | FAIL: MR-003 leaves the exported `QueryResult` class detached from its JSDoc. |
| Tests and task evidence | Focused unit, integration, user-journey, codegen, Python, TypeScript, MCP, Oracle, Forge, and fork evidence recorded in the task; changed test owners and selectors | PASS for maintainability: tests live with existing owners and exercise public outcomes. This review did not rerun the recorded commands. |

## Material findings

### MR-001 — DRIFT — Distributed Variant errors are decoded from prose

- Location: `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4270`, especially `remote_variant_error` at line 4302 and its bespoke `located`/`bounded` grammar.
- Governing principle: `architecture/references/languages/errors.md` makes typed fields and derive-backed structured details the error source of truth; `architecture/references/languages/maintainer-style.md` requires signatures and boundaries to reveal stable behavior. The established cross-process convention in gRPC (`google.rpc.Status` details), HTTP Problem Details, and comparable distributed query systems is a code plus structured metadata, not reparsing localized display text.
- Evidence: the new code calls `current.to_string()` for every source in a `DataFusionError` chain and recognizes exact English fragments such as `"invalid Variant JSON in field "`, `" row "`, `", limit "`, and `" at "`. Those strings duplicate the fields already owned and serialized by `BifrostError`. The accompanying test models the wire as `DataFusionError::External(sent.to_string().into())`, thereby locking the duplicate prose protocol in place.
- Concrete maintenance cost: changing punctuation or wording in the error's human-facing `Display`, or adding a field value that overlaps a delimiter, silently turns the exact catalogued error into `QueryExecutionFailed`. A maintainer must update two independent grammars whenever a Variant error changes. The parser is also a Variant-only exception beside the repository's structured error path.
- Smallest testable correction: preserve the existing serialized `BifrostError` code/details at the internal distributed-stage failure boundary and deserialize them at the coordinator, then delete `remote_variant_error` and its prose-grammar test. Reuse the existing derive-backed `BifrostError` representation; do not add a second error enum, public API, setting, version knob, check, or message format. Prove closure by making the analytical journey return the same typed Variant error as the interactive path while allowing the human `Display` wording to vary independently.

### MR-002 — DRIFT — `EncodedVariant` publicly exposes an invalid-state constructor

- Location: `crates/shared/wyrd-queue/src/variant.rs:105-108`, `:150-180`, and `:200-204`.
- Governing principle: `AGENTS.md` §5 and `architecture/references/languages/rust-core.md` require invariant-bearing domain types to own construction and make invalid states difficult to represent. The Ponytail/YAGNI rule rejects public options with no current caller.
- Evidence: `EncodedVariant` documents that holding the type means the value is storable. `from_json` and `from_bytes` establish that invariant, but public `sized` checks only byte count and therefore accepts empty or malformed metadata/value bytes. Repository-wide caller inspection found no caller of `EncodedVariant::sized`; it is used only through `Self::sized` inside the same impl. Public `is_empty` exists solely to report a state valid construction says cannot occur and likewise has no caller.
- Concrete maintenance cost: downstream code cannot rely on the type-level promise without revalidating bytes, and future callers are offered two contradictory construction paths. `is_empty` advertises the contradiction as supported API rather than removing it.
- Smallest testable correction: make the size-only helper private and delete `is_empty`. Keep `from_json` and `from_bytes` as the public constructors; the existing round-trip and invalid-byte tests are the closure proof. Add no replacement trait, builder, option, or check.

### MR-003 — Exported `QueryResult` lost its class documentation

- Location: `sdks/wyrd-sdk-ts/wyrd/src/index.ts:638-686`.
- Governing principle: `architecture/references/languages/maintainer-style.md` and `architecture/references/languages/typescript-guide.md` require the public typed contract and its documentation to describe the same operation.
- Evidence: the JSDoc beginning `Arrow batches from one query, converted on demand` is immediately followed by `VARIANT_EXTENSION`, so it documents that constant instead of the exported `QueryResult` class now declared at line 686. The class has no replacement class-level documentation.
- Concrete maintenance cost: generated editor/API documentation attributes collection semantics to an internal string constant and gives maintainers and SDK users no class-level contract for `QueryResult`.
- Smallest testable correction: move that existing JSDoc directly above `export class QueryResult`, leaving the private Variant helpers with their own current comments. TypeScript typecheck/document generation is sufficient proof; add no new documentation mechanism or check.

## Calibration notes

- No finding requires a new check, config file, feature, compatibility path, abstraction, or option. The candidate's shared Arrow encoder factory, `OracleVariantSql` owner, fixed Variant limits, and row-group-derived Bloom capacity are all exercised by present callers and are conventional uses of the installed platform libraries.
- The review did not require duplicate prose on inherited trait boilerplate merely to satisfy a repository-specific documentation ritual; the human standing direction rules out novel checks and remediation that comparable projects do not use.
- The very large Forge journey remains readable because its local `LineageTable` owns the repeated state and its helpers correspond to distinct production phases required by the task; splitting it into additional fixtures would add indirection without reducing the scenario.

## Overall result

**FAIL** — MR-001 and MR-002 are material maintainer defects. MR-003 is a small but concrete public-documentation correction.
