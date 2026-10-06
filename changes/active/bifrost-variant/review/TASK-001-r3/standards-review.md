# Repository Standards Review — TASK-001 r3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `555308ba14058ddc56102d2f925298ef43858175`
- Approved authority: `changes/active/bifrost-variant/spec.md`, revision 11
- Task packet: original TASK-001 plus TASK-001-R1 and TASK-001-R2
- Candidate stability: `git rev-parse HEAD` remained `555308ba14058ddc56102d2f925298ef43858175` during this review.

## Overall result

**FAIL**

The cumulative candidate is aligned with the repository boundaries, public-error model, tenancy model, generated-contract workflow, and multi-language journey rules. Two material repository-standard findings remain: the active Bifrost design authority still states the superseded integer contract, and materially changed Rust retains forbidden function-local and qualified-signature imports.

## Authority coverage

| Changed surface | Applicable authority read | Evidence inspected | Result |
|---|---|---|---|
| Repository process, Rust structure, async, imports, documentation, tests, generated files | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `implementation-execution.md`; `maintainer-style.md`; `rust-core.md`; `testing-workflows.md` | Complete base-to-candidate Rust/test/tooling diff; task verification record; focused source around every retained finding | **FAIL** — `REPO-R3-2` |
| Wyrd ownership and public client surfaces | `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md` | `wyrd-spec`, `wyrd-client`, server, CLI, MCP, Python and TypeScript changes | PASS |
| Bifrost ingest, query, terminal, durability and public behavior | `architecture/bifrost-design.md`; `domain/vala-architecture.md`; `domain/olap-serving.md`; `domain/analytical-operations-reliability.md` | queue preflight/Variant owner, Scribe validation, Oracle local/distributed terminal paths, Forge and journey changes | **FAIL** — `REPO-R3-1` |
| Iceberg v3, Parquet/Arrow Variant and Forge lineage | `domain/iceberg.md`; `domain/arrow-analytical-interop.md`; pinned dependency manifests and current Bifrost authority | v3 catalog validation, hidden lineage projection/copy, Forge journey, dependency pins, absence of duplicate-ID/optional-metrics mechanisms | PASS |
| DataFusion planning, session registration and distributed execution | `domain/datafusion.md`; `domain/olap-serving.md`; Bifrost Oracle authority | `OracleVariantSql`, every session builder, local/peer error mapping, distributed real-cluster proof | PASS |
| OTLP, built-in observations, evaluation and drift payloads | `domain/telemetry-observations.md`; `domain/evaluation.md`; `domain/drift-monitoring.md` | signal projections, built-in schemas/producers, verification/eval/drift writers and journeys | PASS |
| Security, tenancy, authorization and audit | `architecture/wyrd-security-posture.md`; `architecture/agent-rules.md`; `languages/agent-harness.md`; Bifrost public-surface authority | server-derived tenant paths, Oracle authority/audit path, MCP and SDK projections; no new URL or credential boundary | PASS |
| Stable public errors and wire projections | `languages/errors.md`; `languages/agent-harness.md`; Wyrd error catalog authority | `BifrostError`, boxed `WyrdProblem` terminal, HTTP/gRPC/MCP/client reconstruction, generated proto descriptor | PASS |
| Python/PyO3 surface | `languages/pyo3-boundaries.md`; `languages/python-api-and-stubs.md`; `domain/arrow-analytical-interop.md` | SDK-local wrapper/registration/export, public Python journeys, no PyO3 in `wyrd-spec`, recorded codegen/type/lint evidence | PASS |
| TypeScript/N-API surface | `languages/typescript-guide.md`; `languages/errors.md`; `domain/arrow-analytical-interop.md` | thin native decoder, public wrapper/declarations, exact-integer and late-terminal journeys, generated declaration evidence | PASS |

## Applicable-rule results

| Rule | Source evidence | Result |
|---|---|---|
| Active design authority must match approved durable behavior and implementation; implementation drift is not precedent. | `AGENTS.md` §§1–2; `architecture/references/README.md` authority hierarchy. `architecture/bifrost-design.md:149-152` still permits any integer fitting a Variant decimal, while revision 11, `docs/.../schema.svx:31-34`, and `wyrd-queue/src/variant.rs:665-672` accept only `i64` or `u64`. | **FAIL** — `REPO-R3-1` |
| All imports live at module top; signatures use imported bare names. | `architecture/agent-rules.md` import rules. `oracle/query_stream.rs:1654,1678` use function-local `DataFusionError`; `wyrd-server/src/query/service.rs:372` uses a qualified `wyrd_spec::error::WyrdProblem` signature. | **FAIL** — `REPO-R3-2` |
| `wyrd-spec` stays IO/async/PyO3/Arrow/DataFusion/Iceberg free and owns shared wire/error contracts. | The candidate adds only pure API/error shapes in `wyrd-spec`; analytical dependencies remain outside its manifest. | PASS |
| Stateful workflows have concrete owners; pure conversion helpers may remain free functions. | Queue conversion is owned by `EncodedVariant`/`VariantColumnBuilder`; session state by `OracleVariantSql`; service/engine workflows remain methods on existing owners. No new single-implementation trait or zero-sized workflow utility was introduced. | PASS |
| Async is restricted to real IO/composition; parsing and validation remain synchronous. | Variant token classification, Scribe Variant validation, terminal reconstruction and Arrow conversion are synchronous; async additions are journeys or existing query/write IO paths. | PASS |
| Public failures use the derive-backed Wyrd catalog and retain structured details across every boundary. | `wyrd-spec/src/vala/error.rs`; `QueryTerminalFrame.error`; proto `error_problem_json`; `wyrd-client::error::from_problem`; HTTP/gRPC/MCP/SDK tests. No prose parser or parallel error-code list remains. | PASS |
| Iceberg lineage uses standard v3 behavior without a non-standard duplicate scan or metrics gate. | Repository search finds no production/test duplicate-row-ID scan or lineage metrics prerequisite; Forge carries the two hidden columns and the r2 journey compares retained lineage. The pins are immutable revisions. | PASS |
| `serde_json` arbitrary precision remains disabled; oversized integers are rejected. | No `arbitrary_precision` occurrence exists in workspace manifests; `wyrd-queue` enables only `raw_value`; lexical classification at `variant.rs:665-672` accepts `i64` then `u64`, otherwise returns `NumericOutOfRange`. | PASS |
| Late query failures carry a full catalog problem; consumers discard partial rows. | `wyrd-spec/src/vala/api.rs:809-848`; `wyrd.v1.proto:31-46`; Oracle terminal construction; shared-client reconstruction. Recorded Rust, Python, TypeScript and MCP proofs cover the contract. The accepted Python/TypeScript Interactive-only harness limit is not a standards failure because Rust V9 proves the distributed server path through the same client reconstruction. | PASS |
| Server/client/SDK ownership remains singular. | All query/ingest transport stays in `wyrd-client::Bifrost`; bindings and CLI only project returned Arrow/Variant values and do not create a transport, registry, or durable state implementation. | PASS |
| Tenant and security boundaries are preserved. | No raw pool, manual `TenantConn` filter, caller-chosen tenant, new URL fetch, credential field, or alternate audit sink entered the diff. Oracle still resolves and authorizes the complete table set before provider construction/IO. | PASS |
| User-facing capability has real client→server→client journeys across public surfaces. | Recorded V5–V9 cover Rust, Python, TypeScript, MCP, Interactive and distributed Rust; OTLP/built-in journeys exercise server trust-boundary validation. Ignored Rust tests are in the documented repository-managed Postgres journey lane rather than disabled failures. | PASS |
| Generated artifacts derive from their owners and are checked for drift. | Proto descriptor, schemas, N-API declarations and public types changed with source owners; TASK-001 evidence records `mise run codegen:check`, `ts:typecheck`, `docs:check`, and `check:docs` passing. | PASS |
| Cargo features and dependency cost are narrow and version-aligned. | Variant crates use the Parquet 59.3 release line; Iceberg/compaction are immutable pins; `raw_value` is the only serde feature added; no new Cargo feature, compatibility surface, checker, setting, or option was added. | PASS |
| Human standing direction: reject mechanisms absent both established standards and comparable widely used projects. | The candidate relies on Iceberg v3 lineage, Arrow/Parquet Variant, DataFusion session/UDF machinery, serde raw tokens, RFC-style problem documents, and existing repository gates. No bespoke duplicate scanner, metrics gate, compatibility side channel, new checker, or configuration knob remains. | PASS |

## Material findings

### REPO-R3-1 — Active Bifrost authority retains the superseded wide-decimal integer contract

- **Rule:** `AGENTS.md` makes `architecture/bifrost-design.md` the active Bifrost authority and requires implementation/public documentation to align with it. Approved revision 11 and the binding human decision accept only signed 64-bit integers plus unsigned 64-bit values represented as scale-zero decimal; integers outside both ranges are refused.
- **Location:** `architecture/bifrost-design.md:149-152`.
- **Evidence:** The authority says “another integer that fits a Variant decimal” is accepted. That includes negative integers below `i64::MIN` and positive integers above `u64::MAX` that Decimal16 can hold. Candidate implementation `crates/shared/wyrd-queue/src/variant.rs:665-672` instead parses `i64`, then `u64`, then returns `NumericOutOfRange`; the public guide at `docs/src/content/docs/bifrost/schema.svx:31-34` describes that narrower behavior.
- **Consequence:** The repository has two conflicting durable contracts. A future contributor following the mandated architecture authority can reintroduce precisely the behavior revision 11 removed, while users reading the schema guide see a different range.
- **Testable correction:** Change only the numeric sentence in the active Bifrost authority to say that signed `i64` values remain integers, values above `i64::MAX` through `u64::MAX` become scale-zero decimals, and every other integral token is refused with `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`. Re-run `mise run docs:check` and `mise run check:docs`; no new checker, option, or numeric mechanism is warranted.

### REPO-R3-2 — Changed Rust retains forbidden local imports and a qualified signature type

- **Rule:** `architecture/agent-rules.md` requires every `use` at module top and bare imported names in function signatures. The only function-local exception is a narrowly scoped `use Trait as _`; tests are allowed their own module import block, not imports inside individual tests.
- **Locations:** `crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs:1654,1678`; `crates/wyrd/wyrd-server/src/query/service.rs:372`.
- **Evidence:** The materially changed `late_resource_exhaustion_is_typed_terminal` and new `late_catalog_error_keeps_its_identity` import `DataFusionError` inside their function bodies even though the test module already has a top-level dependency block. The new `terminal_error` signature spells `wyrd_spec::error::WyrdProblem` rather than importing and using `WyrdProblem`.
- **Consequence:** These sites defeat the repository's module-level dependency manifest and leave the exact style drift the r1 remediation required the cumulative candidate to remove.
- **Testable correction:** Import `datafusion::error::DataFusionError` once at the `query_stream.rs` test-module top and remove its local imports; import `WyrdProblem` in `query/service.rs` and use the bare name in the signature. Re-run `mise run fmt` and `mise run lints`. No new lint, script, allow, or checker is warranted.

## Verification notes

- Reviewed the complete cumulative diff and the candidate's recorded V1–V17 evidence, including Rust/Python/TypeScript/MCP journeys, Oracle distributed proof, Forge lineage proof, codegen, formatting, lint, docs, and boundary checks.
- Independently ran `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..555308ba14058ddc56102d2f925298ef43858175`; it passed.
- Did not rerun the expensive runtime lanes; this standards report treats their recorded results as available verification evidence and does not use them to override the two source-proven rule failures.
- No verification limitation changes either finding. The candidate and required authority were available and stable, so this review is not blocked.
