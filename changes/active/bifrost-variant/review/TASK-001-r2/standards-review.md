# Repository standards review — TASK-001 r2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `99c5871ec5ca664b9f54baa379b437ee09d66e95`
- Cumulative range inspected: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..99c5871ec5ca664b9f54baa379b437ee09d66e95`
- Candidate was still `99c5871ec5ca664b9f54baa379b437ee09d66e95` after the static review.
- CodeGraph was not used because this checkout has no `.codegraph/` directory.

This review audits repository-rule compliance only. It does not decide whether
the implementation satisfies the task's product acceptance criteria.

## Authority coverage

| Changed surface | Applicable authority read | Source coverage | Result |
|---|---|---|---|
| Workspace and crate manifests, lockfile, pinned Iceberg/compaction revisions, Arrow Variant dependencies and dependency features | `AGENTS.md` §§1–6, 11–12; `architecture/agent-rules.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/testing-workflows.md` | Root `Cargo.toml`, `Cargo.lock`, `wyrd-queue`, `vala-bifrost-redux`, `wyrd-cli`, and TypeScript native manifests | PASS |
| Public Variant contract, stable errors, generated JSON schemas, HTTP/gRPC/MCP/CLI projections | `AGENTS.md` §§2–4, 9, 12; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/errors.md` | `crates/wyrd-spec/src/vala/{api,error}.rs`, generated schemas, `wyrd-client/src/error.rs`, Gate, server conversion, CLI and MCP query paths | PASS |
| Shared Variant encoding, Arrow extension handling, JSON rendering and admission validation | `AGENTS.md` §§3–6, 9–10; `architecture/bifrost-design.md`; `architecture/references/languages/rust-core.md`; `architecture/references/domain/{vala-architecture,arrow-analytical-interop,olap-serving}.md` | `wyrd-queue/src/variant.rs`, batch/schema owners, Vala table definitions and `DomainDefinition::validate_variants` | PASS |
| Built-in persisted schemas and producers: verification, eval, gateway, audit, traces, logs, metrics and agent traces | `AGENTS.md` §§2–6, 9–10; `architecture/wyrd-design.md`; `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `architecture/references/domain/{vala-architecture,telemetry-observations,arrow-analytical-interop}.md` | Vala table/projection modules, client observation producer, server gateway and verification producers, canonical signal fixtures | PASS |
| Scribe and Gate admission, typed refusal propagation and gRPC problem details | `AGENTS.md` §§4, 6, 9–12; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `architecture/references/languages/errors.md`; `architecture/references/domain/{olap-serving,analytical-operations-reliability}.md` | `scribe/execution_lanes.rs`, `contracts.rs`, `gate/error.rs`, `wyrd-client/src/error.rs`, server journeys | PASS |
| Oracle SQL registration, DataFusion planning/execution, distributed codec/error transport and result rendering | `AGENTS.md` §§2–6, 9–12; `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `architecture/references/domain/{datafusion,olap-serving,arrow-analytical-interop,analytical-operations-reliability}.md`; `architecture/references/languages/errors.md` | `oracle/{variant_sql,mod,codec,analytical,exec,peer}.rs`, client facade/query, server/MCP/CLI terminals | PASS |
| Iceberg v3 creation, hidden row lineage, Forge rewrites/GC, Parquet writer properties and Bloom configuration | `AGENTS.md` §§2–6, 10–12; `architecture/bifrost-design.md`; `architecture/references/domain/{iceberg,datafusion,analytical-operations-reliability,olap-serving}.md` | Catalog, Forge, promoted-object, writer-properties, managed-rewrite and production-route changes plus immutable fork pins | PASS |
| Python SDK/PyO3 projection and Python journeys | `AGENTS.md` §§7–8, 11–12; `architecture/references/languages/{pyo3-boundaries,python-api-and-stubs,testing-workflows,errors}.md`; `architecture/references/domain/arrow-analytical-interop.md` | `sdks/wyrd-sdk-python/src/bifrost/mod.rs`, public `wyrd.bifrost`, integration journeys | PASS |
| TypeScript/N-API projection, declarations, error-code union and journeys | `AGENTS.md` §§2–4, 8–12; `architecture/references/languages/{typescript-guide,testing-workflows,errors}.md`; `architecture/references/domain/arrow-analytical-interop.md` | TypeScript native binding, public wrapper, generated `.d.ts`/`.d.cts`, integration journeys | PASS |
| Architecture and user documentation | `AGENTS.md` §§1–2, 12; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md`; `architecture/references/languages/agent-harness.md` | Updated Bifrost authority and `docs/src/content/docs/bifrost/schema.svx` | PASS |
| Test placement, journey coverage, exact selectors, generated-artifact checks and boundary gates | `AGENTS.md` §11–12; `architecture/agent-rules.md`; `architecture/references/languages/{spec-driven-development,implementation-execution,testing-workflows}.md` | Rust unit/integration/journey targets, Python and TypeScript journeys, MCP journey, task/remediation evidence, `mise.toml` task definitions | PASS |

Authority coverage is complete for the changed Rust, Python, TypeScript,
contract, documentation, manifest, generated-artifact, and test surfaces.

## Rule results

| Governing rule | Evidence | Result |
|---|---|---|
| Durable contracts stay in `wyrd-spec`; durable analytical behavior stays in Vala/server owners; SDKs remain projections over `wyrd-client`. | `DataTypeSpec::Variant` and limits are in `crates/wyrd-spec/src/vala/api.rs:273-294`; catalogued errors are in `crates/wyrd-spec/src/vala/error.rs:237-346`. Encoding is owned by `wyrd-queue::variant`; admission/query/storage behavior remains under `vala-bifrost-redux`; Python and TypeScript bindings call the shared Rust decoder rather than introducing a second Variant model. | PASS |
| `wyrd-spec` remains foundational, IO-free, async-free and PyO3-free; client-tier crates do not gain DataFusion or Iceberg. | The `wyrd-spec` changes are pure enums/constants/errors. DataFusion, Iceberg and Parquet remain in `vala-bifrost-redux`; `wyrd-queue` receives only the approved Arrow 59.3 Variant crates. Recorded `check:client-tier` and `check:pyo3-scope` results are PASS. | PASS |
| Cargo features and dependencies are narrow and earned; no wildcard or unrelated dependency is added. | Root pins add the installed `parquet-variant*` 59.3 family; `wyrd-queue` enables serde_json's dependency-native `raw_value` feature for `EncodedVariant::from_json_text`; TypeScript native enables napi's dependency-native `serde-json` projection. The human decision explicitly rejects workspace `arbitrary_precision`; no such feature appears in the candidate. | PASS |
| Rust uses cohesive owners, inherent methods and pure helpers; no speculative single-implementation trait or utility owner is introduced. | `EncodedVariant` owns validated construction (`wyrd-queue/src/variant.rs:116-217`); `VariantColumnBuilder` owns Arrow column assembly; `OracleVariantSql` owns immutable UDFs and the expression planner (`oracle/variant_sql.rs:85-139`); `DomainDefinition::validate_variants` owns schema-driven built-in validation (`tables/mod.rs:215`). `VariantJsonEncoderFactory` is the concrete adapter required by Arrow's installed `EncoderFactory` extension point, not a speculative Wyrd abstraction. | PASS |
| Synchronous work stays synchronous; async remains at real server, storage, database and journey IO boundaries. | Variant parsing, validation, SQL lowering helpers and Arrow conversion are synchronous. Changed async functions are query, ingest, server, database, object-store, distributed-peer or user-journey operations that await IO. No ad-hoc runtime was added. | PASS |
| Imports live at module tops; signatures use imported bare types; allowed traits-in-function imports are the documented exception. | Cumulative diff inspection found ordinary imports at module or nested test-module tops. The only nested functional import is `use std::fmt::Write as _`, the explicit trait-enablement exception. Recorded `fmt` and `lints` are PASS. | PASS |
| New and materially changed Rust items have substantive rustdoc, including errors, panics and lifecycle behavior where applicable. | The candidate documents the domain types and helpers in `wyrd-queue/src/variant.rs`, every `ScalarUDFImpl` operation and the distributed error carrier in `oracle/variant_sql.rs`, admission/error additions, projections and changed test helpers. Fallible public and private operations name `# Errors`; panicking tests/helpers name `# Panics`. The cumulative static declaration audit found no material undocumented item. | PASS |
| Production unwrap/expect and Clippy allowances follow repository policy. | New fallible production paths return typed errors or use checked fallbacks; added `expect` calls are test code or named compile-time invariants. The one added production allowance at `sdks/wyrd-sdk-ts/native/src/lib.rs:439` is immediately preceded by the required site-specific napi ownership justification. Recorded `check:unwrap-audit` and `lints` are PASS. | PASS |
| Public errors use derive-backed Wyrd metadata and preserve structured identity across boundaries. | All added public codes are `#[wyrd_error]` variants in `BifrostError` (`wyrd-spec/src/vala/error.rs:237-346`). Gate carries the canonical problem document in the existing gRPC metadata carrier (`gate/error.rs:365-393`); the client reconstructs the exact tagged `BifrostError` before legacy fallback mapping (`wyrd-client/src/error.rs:194-211`). Oracle's dependency string carrier contains tagged serde through `VariantQueryError` (`oracle/variant_sql.rs:650-694`), not a prose grammar. | PASS |
| Tenant identity, sensitivity and authorization boundaries are not weakened. | Variant validation runs after table resolution but before ACK/WAL mutation (`scribe/execution_lanes.rs:530-570`). Table query authorization and sensitive-column planning remain in the existing Oracle path; the Oracle journey checks denial before follower work. No row tenant column/filter, raw SQL pool, caller-selected tenant, alternate audit path or public gateway write is introduced. | PASS |
| Arrow schemas are mapped by identity/name, extension identity is validated, RecordBatch work stays bounded, and native result values retain the shared encoding contract. | `DomainDefinition::validate_variants` recursively compares declared/supplied fields and validates encoded cells; `wyrd-queue::variant` accepts the standard `arrow.parquet.variant` metadata/value storage and bounded 64-depth/8 MiB values. Python `_native_value` and TypeScript `nativeValue` walk returned schema fields and invoke the same Rust byte decoder. No positional cross-schema mapping or result-wide custom transport was added. | PASS |
| Iceberg v3 lineage uses the approved standard mechanism only. | `architecture/bifrost-design.md:124-157,895-903` names v3 hidden metadata and per-batch missing/type/null refusal. The cumulative diff removes Forge's optional-metrics prerequisite and pins the compaction fork that retains field-ID projection, per-batch validation and unchanged copy. No duplicate-ID scan, rewrite-wide collection/sort, metric gate, option or setting remains. This follows the binding human decision and the standard Iceberg v3 boundary. | PASS |
| Bloom sizing uses native parquet-rs behavior rather than a Wyrd duplicate. | `parquet/writer_properties.rs:122-157` enables Bloom filters and FPP only; it does not set a Wyrd NDV. The focused test reads the resolved native `DEFAULT_MAX_ROW_GROUP_ROW_COUNT`. | PASS |
| Generated artifacts derive from source and public surfaces stay aligned across Rust, HTTP/MCP/CLI, Python and TypeScript. | `wyrd-spec` source changes accompany regenerated schema fixtures and TypeScript error-code output. Python exposes native decoded values through public `wyrd.bifrost`; TypeScript wraps the private napi binding and retains generated native declarations. Recorded `codegen:check`, Python formatting/lints, and TypeScript typecheck are PASS. | PASS |
| User/agent-facing behavior has journey coverage in every shipped surface; external tests earn their placement. | The candidate contains real-server Rust client, server, OTLP, Oracle, Forge, Python, TypeScript and MCP journeys. External Rust targets bring up Postgres/server/peer/object-store boundaries, satisfying the exception to inline-test placement. Named Rust tests use exact nextest expressions through `mise`; Python/TypeScript tests use exact repository runners. | PASS |
| No gate was bypassed or nonstandard permanent check/configuration mechanism introduced. | The lineage test step that required the rejected rewrite-wide duplicate scan was removed with the mechanism under the explicit human decision; no assertion was weakened to preserve an approved failure. No `#[ignore]` was added outside the established gated journey pattern, no check/allowlist was broadened, and no new repository check, config option, metrics gate, documentation generator or validation framework was added. | PASS |
| Documentation states the shipped architecture and public schema without compatibility vocabulary. | `architecture/bifrost-design.md:124-166,439-466,895-903` covers v3, hidden lineage, Variant and SQL. `docs/src/content/docs/bifrost/schema.svx:10-39,400` documents the same user surface and updated built-in shapes. No legacy route, alias, storage name or parallel vocabulary was introduced. | PASS |

## Verification evidence

The candidate's implementation records report PASS for all original V1–V17
proofs and the remediation-focused Variant admission, exact-number,
distributed-error, lineage and Bloom tests. They also report PASS for:

- `mise run fmt`
- `mise run lints`
- `mise run py:format`
- `mise run py:lints`
- `mise run ts:typecheck`
- `mise run docs:check`
- `mise run check:docs`
- `mise run codegen:check`
- `mise run check:client-tier`
- `mise run check:pyo3-scope`
- `mise run check:unwrap-audit`

This reviewer independently ran
`git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..99c5871ec5ca664b9f54baa379b437ee09d66e95`;
it passed. Cargo-backed lanes were not rerun because the review topology shares
one checkout/target and repository authority forbids overlapping Cargo work;
the immutable candidate contains the required successful command evidence.

## Material repository-rule findings

None.

The standing human direction was applied: no candidate mechanism, check, file,
setting or option was retained merely as Wyrd-specific hardening where the
established dependency or widely used standard already owns the behavior.
Notably, serde_json `RawValue` plus local `i128` lexical classification uses an
installed standard mechanism; native parquet-rs owns Bloom NDV; and standard
Iceberg v3 field-ID projection/per-batch validation/unchanged copy is the sole
row-lineage mechanism.

## Overall result

**PASS**
