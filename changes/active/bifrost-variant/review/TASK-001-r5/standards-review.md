# TASK-001 Repository Standards Review — r5

## Review Findings

### Critical

None.

### Important

- **STD-R5-001 — Unapproved repository-wide implementation-policy changes remain bundled into TASK-001.** `[.agents/skills/wyrd-implement/SKILL.md:38]` and `[.agents/skills/wyrd-implement/SKILL.md:102]` add mandatory repository/dependency reuse searches, a new blocking relationship to `reuse-rev`, and a new evidence table for every future Wyrd implementation; the generated `.claude/skills/wyrd-implement/SKILL.md` mirror carries the same change. These are global workflow semantics, not Variant implementation or proof. TASK-001's scope is Bifrost contracts, storage/query behavior, producers, clients, generated contracts, examples, and documentation (`tasks/TASK-001-variant-storage-and-query.md:22-48`), while the governing r4 remediation expressly excludes an unrelated workflow-policy refactor (`TASK-001-R4-close-final-variant-contract-gaps.md:177-186`). The prior approved exception covers the mirrored **task-review** skill edits only (`TASK-001-r4/findings-validation.md:23-28`); that validator explicitly states the then-current branch's two `wyrd-implement` files were later than its immutable candidate and did not include them (`:12-15`). Accepting this candidate would therefore change the cost and contract of every future implementation as part of a storage task. Remove the two `wyrd-implement` skill-file changes from this cumulative candidate and land them under separately approved workflow scope; retain the explicitly approved task-review skill exception. Prove closure with a base-to-candidate changed-path audit and `mise run check:skills-sync`.

- **STD-R5-002 — The active task packet still identifies the wrong authority and lifecycle state.** `[changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md:4]` says `status: proposed` after implementation and four completed review/remediation rounds, while `[...:52]` still directs implementation of the "revision-10" Variant and persisted Struct contract even though the front matter, scenario, authority link, evidence, and approved specification all bind revision 12 (`:6`, `:69-73`, `:238`, `:254`; `spec.md:1-5,228-258`). Revision 12 materially changed nullable Struct-child persistence, so the stale approach is not harmless history: unlike the clearly dated cold-rehearsal note at `:227-234`, it remains the operative task instruction. This violates the spec-driven task contract that one ready/review task bind one approved revision and move through its lifecycle state. Update the operative approach to revision 12 and make the front-matter status reflect the review-stage packet; leave the dated revision-10 rehearsal history intact. Closure is a static packet check showing one operative revision and the correct lifecycle state.

### Suggestions

None.

## Open Questions

None. The task-review skill edits have a recorded owner-approved scope exception; no equivalent authority was found for the later `wyrd-implement` edits.

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`
- Candidate tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`
- Latest remediation range: `bb6ae8070..0e37748f3a27d3bcec4713e6210e97328e045886`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 12
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`

The candidate commit and tree matched the requested immutable subject at review start. The final identity check is recorded below.

## Authority Coverage

| Changed surface | Governing authority inspected | Result |
|---|---|---|
| Change packet, TDD/evidence, remediation history, workflow-skill edits | `AGENTS.md` §§11–16; `architecture/agent-rules.md`; `architecture/references/languages/{spec-driven-development,implementation-execution,maintainer-style,testing-workflows}.md`; original TASK-001; spec revision 12; all r1–r4 review/remediation artifacts | **FAIL** — `STD-R5-001`, `STD-R5-002` |
| Workspace manifests, lockfile, Arrow Variant dependencies, pinned Iceberg/compaction forks | `AGENTS.md` §§1, 3–6, 11–12, 15–16; `architecture/bifrost-design.md`; `architecture/references/languages/rust-core.md`; `domain/{iceberg,datafusion,analytical-operations-reliability}.md`; operations authority | PASS |
| `wyrd-spec` Variant wire types, stable errors, terminal contracts, generated schemas/protobuf | `AGENTS.md` §§2–4, 8–9, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/{doctrine/architecture-constraints,architecture/patterns,languages/errors,languages/agent-harness}.md` | PASS |
| `wyrd-queue` exact JSON tokens, canonical extension identity, bounded raw decoding, rendering, schema conversion, batch building | `AGENTS.md` §§3–6, 15–16; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; `languages/rust-core.md`; `domain/arrow-analytical-interop.md` | PASS |
| Vala built-in schemas/producers, Scribe native preflight, Gate errors, Parquet, Oracle SQL/distributed execution | `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `domain/{vala-architecture,olap-serving,datafusion,arrow-analytical-interop,analytical-operations-reliability}.md`; `languages/errors.md` | PASS |
| Verification, Eval, gateway, audit, OTLP traces/logs/metrics and nullable Struct producers | `AGENTS.md` §§2, 9–10; `architecture/bifrost-design.md`; `domain/{telemetry-observations,evaluation,drift-monitoring}.md`; schema guide | PASS |
| Forge/Iceberg v3 lineage, GC, repeated rewrites and final fork pins | `architecture/bifrost-design.md`; `domain/{iceberg,datafusion,analytical-operations-reliability}.md`; deployment/recovery authority | PASS |
| Shared Rust client, CLI/MCP/HTTP/gRPC, Python/PyO3, TypeScript/N-API and generated declarations | `AGENTS.md` §§2–9, 11–12, 16; `architecture/patterns.md`; `languages/{pyo3-boundaries,python-api-and-stubs,typescript-guide,agent-harness,errors}.md`; Arrow interop authority | PASS |
| Rust, Python, TypeScript, MCP, CLI, OTLP, server, Oracle and Forge tests | `AGENTS.md` §§11–12, 16; `architecture/agent-rules.md`; `languages/{testing-workflows,spec-driven-development,implementation-execution}.md`; `TESTING.md`; `mise.toml` | PASS |
| Architecture and public schema documentation | `AGENTS.md` §§1–2, 12; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md`; `docs/src/content/docs/bifrost/schema.svx` | PASS |

## Applicable Rule Results

| Repository rule | Source and verification evidence | Result |
|---|---|---|
| Durable contracts and behavior stay with their owning layers; SDKs remain projections over `wyrd-client`. | Pure Variant constants/errors remain in `wyrd-spec`; canonical conversion is in `wyrd-queue`; durable Scribe/Oracle/Forge behavior remains in Vala/server owners; Python and TypeScript decode through shared Rust owners. Reviewer-run `check:client-tier` and `check:pyo3-scope` passed. | PASS |
| `wyrd-spec` stays IO-, async-, PyO3-, DataFusion-, Parquet-, and Iceberg-free. | Contract changes are pure typed API/error/schema declarations. Analytical dependencies remain in their narrow owners. | PASS |
| Cargo features/dependencies are earned and version-aligned. | No new task-local feature or unrelated dependency was added in the final remediation. Arrow/Parquet/DataFusion stay on one release line; forks remain immutable pins recorded in the task evidence. | PASS |
| Struct-centered Rust and synchronous-by-default rules. | `EncodedVariant`, `VariantColumnBuilder`, `OracleVariantSql`, table owners, Scribe owners, and existing service handles retain cohesive responsibility. Added free functions are deterministic conversions/validation. Pure Variant/schema work is synchronous; async changes await actual query/server/storage/journey IO. | PASS |
| Imports live at module/test-module tops and signatures use imported bare names. | The earlier cumulative violations were removed. The only changed nested import remaining is the sanctioned `use Trait as _` form. | PASS |
| Every new or materially modified Rust item has substantive rustdoc, including errors/panics and relevant invariants. | The cumulative Variant/Oracle/table/test additions document owner role, behavior, and error/panic conditions. The latest remediation documents `collect_raw`, bounded raw-depth traversal, masked-null semantics, nullable persisted children, validators, and every added test/helper. No material undocumented item was found. | PASS |
| Public errors use the derive-backed catalog and preserve structured identity across boundaries. | Variant and Bifrost failures are catalog variants; Gate, Oracle terminals, HTTP/gRPC/client reconstruction, Python, TypeScript, CLI and MCP retain structured codes/details. Late-error and distributed journeys are recorded green. | PASS |
| Tenant, authorization, sensitivity, and audit ownership remain intact. | No raw pool, caller-chosen tenant, manual tenant filter, alternate audit sink, or pre-authorization protected IO was added. Sensitive Variant/Struct roots remain authorized by Oracle; Python's RBAC test now asserts the documented `code` attribute rather than unstable message text. | PASS |
| Arrow schema identity and untrusted Variant work are validated and bounded before durable admission. | Shared `is_variant` owns name/empty-metadata identity; table-owned validators order schema refusal before Variant values; `EncodedVariant::from_bytes` bounds size/depth before upstream recursive validation; Scribe repeats validation before ACK/WAL. | PASS |
| Nullable Struct storage is consistent across Arrow, Parquet, hot and published reads. | Revision 12 makes verification summary children nullable; sibling metrics buckets and gateway resolved-model children follow the same Parquet requirement under explicit owner direction. Producers null absent children, tests cover present/absent verification rows and published metric rows, and docs state the shapes. | PASS |
| First-class and agent-facing surfaces have real journeys. | Recorded evidence covers Rust, Python, TypeScript, MCP, CLI, OTLP, server and distributed Oracle paths. The r4 remediation adds the compiled CLI Variant JSONL case and real-server negative admission cases; Python/Node lifetime behavior remains in its owning runtime. | PASS |
| Generated artifacts are owner-derived and synchronized. | Schema/protobuf/N-API/error-code changes accompany source owners. Recorded `codegen:check` passed; reviewer-run `check:skills-sync` passed without rewriting the tree. | PASS |
| Verification gates are not bypassed. | No new weakening `#[allow]`, deleted/ignored failing test, sleep, retry, or widened boundary glob was found. Recorded diagnoses explain the Scribe masked-null, published Parquet, and Python RBAC failures before their corrections. Reviewer-run unwrap and Clippy-allow audits passed. | PASS |
| Final cumulative scope contains only approved task or explicitly excepted work. | The task-review skill changes have an explicit owner exception. The later global `wyrd-implement` policy changes have neither TASK-001 authority nor that exception and contradict the r4 non-goal. | **FAIL — `STD-R5-001`** |
| The active task packet names one approved revision and accurate lifecycle/evidence state. | Most references were corrected to revision 12, but the operative Approach still says revision 10 and front matter remains `proposed`. | **FAIL — `STD-R5-002`** |

## Verification Notes

Reviewer-run against the requested candidate:

- `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..0e37748f3a27d3bcec4713e6210e97328e045886` — PASS
- `git diff --check bb6ae8070..0e37748f3a27d3bcec4713e6210e97328e045886` — PASS
- `mise run check:skills-sync` — PASS
- `mise run fmt:check` — PASS
- `mise run check:client-tier` — PASS
- `mise run check:pyo3-scope` — PASS
- `mise run check:unwrap-audit` — PASS
- `mise run check:clippy-allow-audit` — PASS

The task and r4 remediation evidence record successful focused unit, real-server, CLI, Rust/Python/TypeScript/MCP, distributed Oracle, OTLP, Forge, final-pin fork, codegen, format, lint, and documentation lanes. This reviewer did not rerun the expensive Postgres, cross-language, fork, full Clippy, or code-generation suites. That residual runtime uncertainty does not affect the two source-proven standards findings.

## Overall Result

**FAIL**

Finding IDs: `STD-R5-001`, `STD-R5-002`.
