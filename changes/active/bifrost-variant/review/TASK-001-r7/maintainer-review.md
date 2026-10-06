# TASK-001 r7 maintainer review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `6147cc617d81f2c03464043be698ab2565e9d745`
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`
- Approved contract: `changes/active/bifrost-variant/spec.md`, revision 13
- Task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Authorities read: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/references/languages/maintainer-style.md`, `architecture/wyrd-design.md`, and `architecture/bifrost-design.md`.

## Review findings

### Critical

None.

### Important

None.

### Suggestions

None. The manual raw-Variant scanner and the bounded repeated JSON subtree scans are necessary for the approved trust-boundary contract, are concentrated in the existing Variant owner, and are documented with their limits and focused proofs; moving or abstracting them would add maintenance surface without removing an existing one.

## Changed-surface coverage

| Surface | Symbols and consumers traced | Maintainer assessment |
|---|---|---|
| Variant construction and admission | `VariantViolation`, `VariantViolations`, `EncodedVariant::{from_json,from_json_text,from_bytes,validate}`, `VariantColumnBuilder`, batch-builder callers, OTLP projection callers, and Scribe's `validate_declared_variants` | One invariant-bearing owner defines the JSON and raw-byte contract. Error selection, depth handling, numeric rules, and canonicality checks are named and documented where they are enforced; callers supply only field/row context. |
| Raw Variant scanning and rendering | `scan_encoded`, `object_field_slots`, `mask_placeholders`, `variant_bytes_to_json`, `variant_cell_to_json`, `VariantJsonEncoderFactory`, and Rust/CLI/MCP/HTTP result consumers | The explicit-stack validation gate is reused before every upstream recursive renderer. The dense wire work remains private beside its owner, documents encoding assumptions and panic containment, and has adversarial tests for depth, overlap, repeated names, decimals, and non-finite numbers. |
| Built-in schema and validation | `BuiltinTableDefinition`, `validate_predeclared`, `refuse_undeclared`, `refuse_partial_structs`, `WHOLE_STRUCTS`, verification, gateway, metrics, audit, agent-trace, and canonical OTLP table definitions/projections | Schema identity, whole-or-absent Struct rules, and Variant validation stay table-owned and are invoked through the common validator rather than copied into producers. Field names and nullability remain declared at the table owner. |
| Scribe trust boundary | Built-in-source enforcement in `scribe/execution_lanes.rs`, fixed IPC null validation, ingress propagation, and peer capture ingest | Validation occurs before acknowledgement through existing Scribe owners. The code preserves catalogued errors instead of introducing another carrier, and focused peer/server journeys exercise the real boundary. |
| Oracle SQL and late errors | `OracleVariantSql`, `VariantOperatorPlanner`, `VariantGet`, `VariantAsText`, `ParseJson`, `ToJson`, production session builders/codecs, `QueryCatalogError`, query terminals, HTTP/gRPC conversion, and SDK collection/settlement | One concrete registry owns all functions and operator lowering; Struct access remains DataFusion-owned. One external error envelope preserves catalog identity through local and distributed terminals, and client collection discards partial rows through the existing stream owner. |
| Persisted storage | v3 catalog creation/validation, Forge rewrite and GC call paths, promoted-object validation, row-lineage fork boundary, and shared writer-property construction | The candidate preserves existing catalog/Forge handoffs and keeps hidden lineage physical. Bloom sizing is one calculation shared by Scribe and Forge; no task-specific facade or compatibility path was added. |
| Public contracts and language terminals | `DataTypeSpec::Variant`, catalog errors, protobuf terminal problem field/conversion, Rust query decoding, Python wrapper/export, TypeScript native binding/declarations/error codes, CLI and MCP renderers | Durable behavior remains server/Rust-owned. Language surfaces project the shared wire and native JSON result rather than implementing separate Variant models or validators; generated schemas and protocol artifacts agree with the source contract. |
| Documentation and tests | Bifrost architecture/schema docs; Variant unit tests; table, Oracle, gateway, Scribe, Forge, server, OTLP, Rust, Python, TypeScript, CLI, and MCP journeys | New and materially modified Rust items inspected have intent-level rustdoc, fallible items name error conditions, and test helpers explain their fixture role. Tests are located with their owners or in justified cross-boundary journey targets and assert caller-visible outcomes. |
| Dependencies and abstraction shape | Workspace/crate manifests, Arrow 59.3 Variant dependencies, concrete `OracleVariantSql`, concrete builders and error carriers | Dependencies remain in the narrow owners approved by the task. No single-implementation trait, speculative configuration, cache, second Variant model, or parallel registry was introduced. |

## Open questions

None that affect correctness or maintenance risk.

## Verification notes

- Reviewed the complete base-to-candidate diff and current source, using CodeGraph before targeted source inspection to trace the primary owners and callers.
- The supplied candidate evidence reports green Variant, Vala/Postgres, Oracle, gateway, server and peer journeys plus formatting, lint, codegen, skills-sync, docs, and diff checks; this review did not rerun those commands.
- The change from a NaN log-body fixture to `-0.0` is accompanied by a separate test proving a non-finite body rejects only its own OTLP record, so the fixture correction does not erase the changed behavior.
- Omitting `stacker` is maintainable: the candidate establishes that raw-text syntax validation is non-recursive and performs the bounded depth check before the recursive `serde_json::Value` serialization path, avoiding a dependency and keeping the rule in the existing owner.

## Overall result

**PASS** — the materially changed workflows have clear concrete owners, shared invariants are enforced once and reused by their consumers, documentation explains the non-obvious wire and failure rules, generated/public surfaces remain aligned, and no material maintainability finding remains.

## Identity check

At report completion, `HEAD^{commit}` was `6147cc617d81f2c03464043be698ab2565e9d745` and `HEAD^{tree}` was `7b7bb069ecbe8600e09cc8b6ea4e938709614718`; the immutable subject did not change during this review.
