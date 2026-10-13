# TASK-001 r2 maintainer review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `99c5871ec5ca664b9f54baa379b437ee09d66e95`
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation: `changes/active/bifrost-variant/review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md`
- Authorities: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/references/languages/maintainer-style.md`, `architecture/references/languages/rust-core.md`, `architecture/references/languages/python-api-and-stubs.md`, `architecture/references/languages/typescript-guide.md`, and the task-applicable Bifrost authority.

The candidate was still `99c5871ec5ca664b9f54baa379b437ee09d66e95` after inspection. CodeGraph was unavailable because the repository has no `.codegraph/` index, so source navigation used the cumulative Git diff and repository search as directed by `AGENTS.md`.

## Changed-surface coverage

| Surface | Symbols and consumers inspected | Maintainer assessment |
|---|---|---|
| Shared Variant owner | `EncodedVariant`, `VariantViolation`, `VariantColumnBuilder`, raw/text encoding, Arrow cell rendering, builders, and focused tests in `crates/shared/wyrd-queue/src/variant.rs`; queue schema and batch-builder callers | PASS. Construction and validation remain on the invariant-bearing `EncodedVariant` owner. `from_json_text` uses the approved local `RawValue` plus lexical `i128` classification (`variant.rs:142-163,580-695`) without enabling workspace-wide `arbitrary_precision`. The formerly invalid public size-only constructor is private and the dead emptiness API is gone (`variant.rs:185-219`). Names, error contracts, and rustdoc make the numeric and storage invariants discoverable. |
| Built-in declarations and recursive trust-boundary validation | `BuiltinTableDefinition::validate_variants`, `holds_variant`, `variant_identity_matches`, `validate_variant_values`, all built-in field declarations, Scribe `decode_rows`, and the raw-IPC server journey | PASS. The recursive pass is a cohesive method on the table definition that owns the declared schema (`tables/mod.rs:195-238`); narrow deterministic recursion stays in adjacent private helpers (`tables/mod.rs:241-340`). Scribe invokes it once before canonical validation, stamping, ACK, or WAL mutation (`scribe/execution_lanes.rs:575-650`). This avoids per-table validators and reuses `EncodedVariant::from_bytes`. |
| Error projection | `ScribeError::ContractViolation`, Gate `IngestError::ContractViolation`, problem-detail gRPC transport, shared-client reconstruction, and tests | PASS. The typed catalog error remains visible through the existing Scribe/Gate/client owners. The names distinguish contract violations from schema mismatch, and the transport documentation describes the single projection rather than adding a parallel error model. |
| Oracle Variant SQL and distributed recovery | `OracleVariantSql`, operator planner, Variant UDF implementations, `ParseJson`, `VariantQueryError`, `variant_query_error`, session installers, CLI/MCP/SDK row terminals, and local/distributed tests | PASS. Stateful registration is owned by `OracleVariantSql`; pure decoding/planning helpers remain local. Distributed recovery serializes and recognizes the existing tagged `BifrostError` (`variant_sql.rs:650-695`; `oracle/mod.rs:4271-4295`) instead of maintaining a prose grammar. Trait methods, fallible helpers, and tests now carry substantive item documentation. |
| Iceberg v3 lineage and Forge rewrite | v3 creation, Forge publication/handoff, pinned compaction behavior, rewrite/GC journeys, and removed duplicate-lineage fixture | PASS. The remediation deletes the optional-metrics prerequisite and rewrite-wide duplicate-ID mechanism. The remaining implementation follows the human-approved standard boundary: field-ID projection, per-batch presence/type/null validation, and unchanged copy. The obsolete duplicate-copy test helpers were deleted rather than replaced with another mechanism. |
| Parquet Bloom configuration | shared writer recipe, Scribe and Forge callers, and resolved-properties test | PASS. The writer sets only enablement and FPP and lets parquet-rs derive NDV from row-group geometry (`writer_properties.rs:115-150`). The existing focused test reads the resolved native value (`writer_properties.rs:303-333`); no duplicate constant or setting remains. |
| Rust repository shape | Cumulative added/materially changed Rust items, module imports, signature types, owner placement, tests, and rustdoc | PASS. Changed workflows have concrete owners; free functions are deterministic helpers. Added imports are at module or test-module scope, apart from the expressly permitted `Trait as _` pattern. The inspected cumulative added-item inventory supplies intent and `# Errors`/`# Panics` sections where applicable. No new one-implementation trait, utility object, compatibility layer, checker, or configuration surface was introduced. |
| TypeScript terminal | Variant-native conversion, `QueryResult`, N-API boundary/declarations, and query journeys | PASS. The existing JSDoc is attached directly to `QueryResult` (`sdks/wyrd-sdk-ts/wyrd/src/index.ts:679-686`), public return types remain explicit, and Variant conversion stays a projection of the shared Rust decoder rather than durable TypeScript logic. |
| Python and Rust SDK terminals | Python public `wyrd.bifrost` projection, PyO3 module, Rust client query facade/errors, and language journeys | PASS. Public package exports and native wrappers continue to project `wyrd-client::Bifrost`; no language-specific Variant authoring or durable validation path was added. |
| Architecture and user documentation | `architecture/bifrost-design.md` and `docs/src/content/docs/bifrost/schema.svx` against owning declarations | PASS. The existing documents now describe Iceberg v3 lineage, Variant wire/storage rules, SQL functions, error behavior, promoted built-in layouts, and native terminal decoding in place. No generator, bespoke doc check, setting, or speculative example was added. |
| Generated and manifest parity | Workspace/crate manifests, lockfile, `wyrd-spec` schemas and fixtures, TypeScript declarations/error codes | PASS. The `serde_json` change uses the already-established `raw_value` feature rather than the rejected `arbitrary_precision`; schema/declaration changes correspond to the public contract and were recorded as regenerated evidence. |

## Verification assessment

The remediation record reports PASS for the focused Variant encoder and Oracle exact-integer tests, the raw-IPC built-in admission journey, local and distributed error identity, repeated Forge lineage rewrite, pinned Iceberg/compaction tests, Bloom resolved-properties test, Rust/Python/TypeScript/MCP journeys, code generation, docs, formatting, lints, type checks, client-tier/PyO3/unwrap boundaries, and `git diff --check`. I independently ran `git diff --check` and inspected the cited source/test owners and cumulative added-item/import surface. Those proofs exercise the maintainer-sensitive contracts above; no new harness or repository check is required.

## Material findings

None.

No personal-preference or calibration-only concerns were retained. In particular, the approved local `RawValue` implementation and standard Iceberg v3 lineage handling are not findings, and no duplicate-ID scan, metrics gate, `arbitrary_precision` feature, or replacement mechanism is required.

## Result

**PASS**

The cumulative candidate is findable and maintainable at its existing owners, closes the prior maintainer findings, preserves generated/public declaration parity, and adds no nonstandard maintenance mechanism within the reviewed task.
