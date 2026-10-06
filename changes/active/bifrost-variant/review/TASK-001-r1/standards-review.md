# Repository Standards Review — TASK-001 r1

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `3cf911fce699bbfe197f8b95e72b13e2f551f766`
- Candidate remained `3cf911fce699bbfe197f8b95e72b13e2f551f766` throughout this review.
- Reviewed range: complete `base..candidate` diff (107 files; Rust contracts,
  shared queue/client code, Vala/Bifrost, server/CLI/MCP, Python and TypeScript
  SDKs, generated artifacts, manifests, and user journeys).

This report audits repository-rule compliance only. It does not decide task
acceptance or repeat the behavior reviewers' work.

## Authority coverage

| Changed surface | Applicable authority | Coverage |
|---|---|---|
| Workspace dependencies, lockfile, Iceberg/compaction pins | `AGENTS.md` §§1, 4, 15–16; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md`; `architecture/references/domain/{iceberg,datafusion,analytical-operations-reliability}.md`; `architecture/operations/{deployment-and-release,reliability-and-recovery}.md` | Dependency placement, feature additions, version pinning, native-version unity, and deployment/recovery boundaries inspected. |
| `wyrd-spec` Variant contracts, stable errors, and JSON schemas | `AGENTS.md` §§2–4, 9, 12, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/{architecture/patterns,doctrine/architecture-constraints,languages/errors,languages/agent-harness}.md` | Foundational purity, derive-backed errors, typed wire shapes, schema generation, and public-surface alignment inspected. |
| `wyrd-queue` canonical Variant encoding and Arrow storage | `AGENTS.md` §§3–6, 15–16; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; `architecture/references/{languages/rust-core,domain/arrow-analytical-interop}.md` | Owner placement, dependency cost, struct-centered shape, Arrow extension/storage shape, imports, signatures, docs, and tests inspected. |
| Vala built-in tables, signal projection, Scribe lanes, Parquet writer properties | `AGENTS.md` §§3–6, 9–12, 15–16; `architecture/bifrost-design.md`; `architecture/references/domain/{vala-architecture,olap-serving,arrow-analytical-interop,analytical-operations-reliability}.md` | Durable owner, schema mapping, sensitivity, bounded paths, row-group geometry, async boundary, documentation, and tests inspected. |
| Oracle SQL/session/codec/distributed surfaces | `AGENTS.md` §§3–6, 9–12, 15–16; `architecture/wyrd-security-posture.md`; `architecture/bifrost-design.md`; `architecture/references/domain/{olap-serving,datafusion,arrow-analytical-interop,analytical-operations-reliability}.md` | Single registry owner, session composition, structured failures, authorization/tenant boundaries, DataFusion ownership, docs, and tests inspected. |
| Forge publication, v3 catalog/GC, lineage validation, pinned forks | `AGENTS.md` §§3–6, 9–12, 15–16; `architecture/bifrost-design.md`; `architecture/references/domain/{iceberg,datafusion,analytical-operations-reliability}.md`; operations authorities | Owner/commit boundary, hidden lineage handling, GC/recovery, fork pins, test placement, imports, and docs inspected. |
| Server, shared client, CLI, HTTP/MCP rendering | `AGENTS.md` §§2–6, 9, 11–12, 15–16; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/references/{architecture/patterns,doctrine/architecture-constraints,languages/agent-harness,languages/errors}.md` | Server/client ownership, shared facade, typed errors, audit/security invariants, MCP parity, imports, docs, and journeys inspected. |
| Python SDK/PyO3 projection | `AGENTS.md` §§7–8, 11–12, 16; `architecture/references/languages/{pyo3-boundaries,python-api-and-stubs,testing-workflows}.md`; Arrow interop reference | Boundary placement, GIL/lifetime shape, public package projection, typing, formatting/linting, and Python journey coverage inspected. |
| TypeScript/N-API projection and declarations | `AGENTS.md` §§2–3, 11–12, 16; `architecture/references/languages/{typescript-guide,testing-workflows,errors}.md`; Arrow interop reference | Thin native boundary, shared-client reuse, generated declarations, native-value projection, typing, and TypeScript journey coverage inspected. |
| Rust/Python/TypeScript/MCP/OTLP/server/Forge tests | `AGENTS.md` §11; `architecture/agent-rules.md`; `architecture/references/languages/{testing-workflows,spec-driven-development,implementation-execution}.md` | Tier placement, runtime ownership, exact selectors, repository-managed environments, ignored journey gating, and cross-surface coverage inspected. |
| Active task packet edits and implementation evidence | `AGENTS.md` §14; `architecture/references/languages/{spec-driven-development,implementation-execution}.md` | Tracked active-packet placement, exact command recording, and evidence format inspected. |

## Rule results

| Repository rule | Evidence | Result |
|---|---|---|
| Foundational and client-tier dependency boundaries | `wyrd-spec` adds only pure contract constants/types/errors; Variant encoding is in `wyrd-queue`; DataFusion/Iceberg remain in Vala. Independent `check:client-tier`, `check:cli-client-tier`, `check:sdk-client-tier`, and `check:pyo3-scope` passed. | PASS |
| Cargo features/dependencies must be earned and narrow | No new crate feature was introduced. The new Arrow Variant crates are existing release-line dependencies placed only in the direct owners; N-API's `serde-json` feature is used by the new native boundary conversion. `check:object-store-pin` passed with one Arrow/Parquet/DataFusion universe. | PASS |
| Durable behavior stays server/Rust-owned; SDKs remain projections | Canonical Variant encoding/decoding is in `wyrd-queue`; Oracle/Forge behavior remains in Vala; Python and TypeScript only walk foreign-runtime values and call the Rust decoder. No second transport or Bifrost client appears. | PASS |
| Public errors use the derive-backed stable catalog | New Variant/field/type errors are variants of `BifrostError` with `#[wyrd_error(...)]`; transport clients reconstruct the typed payload rather than creating surface-only codes. Generated error-code artifacts are updated. | PASS |
| Generated artifacts come from owners and remain in sync | Source contract changes accompany the schema and declaration changes. Candidate evidence records `codegen:check`; independent `ts:napi:check` passed. No generated-only contract was introduced. | PASS |
| Struct-centered Rust style | Stateful responsibilities have concrete owners (`EncodedVariant`, `VariantColumnBuilder`, `OracleVariantSql`, existing Scribe/Forge/table owners). Added free functions are conversions or deterministic algorithms without dependency-bearing workflow state. | PASS |
| Async is limited to IO/composition | Added async production and journey functions await query, server, storage, or lifecycle IO. Variant parsing, schema construction, projection, and SQL expression transformation remain synchronous. | PASS |
| Import declarations are the module dependency manifest; signatures use imported bare names | Function-local imports remain at `wyrd-queue/src/variant.rs:351,763-765`, `wyrd-client/tests/pg_bifrost_e2e.rs:2732`, `vala-bifrost-redux/src/tables/mod.rs:2024,2297-2298`, and `wyrd-mcp/tests/bifrost/mcp/query.rs:741`. Added signatures also use fully-qualified types, including `wyrd-client/src/observe/eval.rs:188`, `wyrd-queue/src/variant.rs:570`, `vala-bifrost-redux/src/parquet/writer_properties.rs:130`, and `wyrd-testing/tests/bifrost/server/verification_runtime.rs:1178`. These are directly forbidden by `architecture/agent-rules.md` lines 9–10. | **FAIL (REPO-002)** |
| Every new/materially modified Rust item has substantive rustdoc, with `# Errors`/`# Panics` where applicable | The new `ScalarUDFImpl` methods at `vala-bifrost-redux/src/oracle/variant_sql.rs:204-212`, `417-425`, `479-487`, and `553-569` have no item rustdoc; fallible methods among them also omit `# Errors`. The added test at `wyrd-client/src/error.rs:421` asserts/panics but its rustdoc has no `# Panics`. `AGENTS.md` lines 720–733 and `architecture/agent-rules.md` line 35 make this a hard pre-merge blocker, including private and test items. | **FAIL (REPO-001)** |
| Tests use the owning runtime and required journey tier | The diff adds real Rust, Python, TypeScript, MCP, OTLP, Oracle, server, and Forge journeys. Python/Node lifetime behavior stays in its runtime; Postgres/live-server tests remain gated. No new external test binary is used for a unit-only concern. | PASS |
| Tenant, authorization, audit, and engine ownership boundaries | No raw production pools, caller-selected tenant path, alternate audit sink, or Vala-owned listener was added. Oracle continues through its established authorization/admission owners; Forge transitions remain lineage rather than audit. Relevant boundary checks passed. | PASS |
| Repository verification discipline | Candidate evidence records all 17 exact task-local proofs, including codegen and fork revisions. Independently run here: `fmt:check`, workspace `lints`, Python format/lints/typecheck, TypeScript typecheck/N-API drift, client/PyO3/CLI/SDK boundaries, unwrap and Clippy-allow audits, object-store pin, and `git diff --check`; all passed. | PASS |
| No unsupported bespoke mechanism/check/file/setting/option | The new limits, protocol version, pinned forks, and Variant dependency features are required contract/dependency mechanisms and align with the repository's existing versioned distributed-protocol and immutable-pin practices. No novel standalone check, configuration option, compatibility path, or speculative scaffold entered the implementation. | PASS; no human-direction DRIFT finding |

## Material findings

### REPO-001 — Missing required rustdoc on added Rust items

- **Violated rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require
  substantive rustdoc on every new or materially modified Rust item, including
  private methods and tests; every fallible item needs `# Errors`, and panicking
  items need `# Panics`. Missing documentation is explicitly
  `BLOCK_BEFORE_MERGE`.
- **Locations:**
  - `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:204-212`
    (`VariantGet::{name,signature,return_type}`)
  - `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:417-425`
    (`VariantAsText::{name,signature,return_type}`)
  - `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:479-487`
    (`ToJson::{name,signature,return_type}`)
  - `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:553-569`
    (`ParseJson::{name,signature,return_type,return_field_from_args}`)
  - `crates/shared/wyrd-client/src/error.rs:421`
    (`bifrost_problem_details_reconstruct_exact_variant` lacks `# Panics`)
- **Consequence:** the candidate violates a hard repository acceptance rule even
  though the compiler, Clippy, and public-doc build do not diagnose private or
  trait-implementation documentation omissions.
- **Smallest testable correction:** document every added/materially modified
  Rust item in the cumulative diff, not only the examples above. Explain its
  workflow role and invariants; add `# Errors` and `# Panics` where the body can
  fail or panic. Reinspect the cumulative Rust diff and rerun `mise run lints`.
  Do not add a new documentation check: the standing rule and ordinary review
  already cover this property.

### REPO-002 — Imports and signature types violate the mandatory module style

- **Violated rule:** `architecture/agent-rules.md` lines 9–10 require all
  imports at the top of their module and bare imported names in signatures,
  including bounds and return types. The only relevant function-local exception
  is `use Trait as _`; these imports are not that exception.
- **Locations/evidence:**
  - Function-local imports at
    `crates/shared/wyrd-queue/src/variant.rs:351,763-765`,
    `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs:2732`,
    `crates/vala/vala-bifrost-redux/src/tables/mod.rs:2024,2297-2298`, and
    `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:741`.
  - Fully-qualified signature names at
    `crates/shared/wyrd-client/src/observe/eval.rs:188`
    (`serde::Serialize`),
    `crates/shared/wyrd-queue/src/variant.rs:570`
    (`parquet_variant::BuilderSpecificState`),
    `crates/vala/vala-bifrost-redux/src/parquet/writer_properties.rs:130`
    (`parquet::file::properties::WriterPropertiesBuilder`), and
    `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:1178`
    (`std::fmt::Debug`), with further test-only examples in the same diff.
- **Consequence:** module dependency surfaces are hidden inside functions and
  signatures obscure the owning crate/import manifest, contrary to a mandatory
  repository-wide readability rule.
- **Smallest testable correction:** move each ordinary `use` to the nearest
  module-level import block and import every signature type/bound under a bare
  name. Preserve the valid narrow `use std::fmt::Write as _` trait-method
  exception. Reinspect all added Rust signatures/imports and rerun
  `mise run fmt:check` and `mise run lints`.

## Verification notes

Independently executed against the candidate, all successful:

- `mise run fmt:check`
- `mise run lints`
- `mise run py:format:check`
- `mise run py:lints`
- `mise run py:typecheck`
- `mise run ts:typecheck`
- `mise run ts:napi:check`
- `mise run check:client-tier`
- `mise run check:cli-client-tier`
- `mise run check:sdk-client-tier`
- `mise run check:pyo3-scope`
- `mise run check:unwrap-audit`
- `mise run check:clippy-allow-audit`
- `mise run check:object-store-pin`
- `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..3cf911fce699bbfe197f8b95e72b13e2f551f766`

The task's recorded implementation evidence reports all 17 task-local commands
successful on the final candidate. This review did not rerun the Postgres,
fork-repository, or complete cross-language journey commands; their recorded
evidence is the available verification result. That limit does not affect the
two source-proven repository-rule failures above.

## Overall result

**FAIL**

`REPO-001` and `REPO-002` are explicit repository-rule violations. No optional
improvement or unsupported bespoke mechanism is included as a finding.
