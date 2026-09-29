# TASK-004 R2 Repository Standards Review

## Immutable subject

- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `2af4cc3ff95a609d1df682be4f633345f96934e1`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Prior remediation: `changes/active/verified-change-contract/review/TASK-004-r1/TASK-004-R1-close-validated-runtime-gaps.md`

The candidate remained the repository `HEAD` throughout this review. The complete
base-to-candidate range was reviewed. `.codegraph/` is absent, so source and
consumer tracing used repository search, `git diff`, `git blame`, and direct
source inspection. This reviewer did not read the R2 task-review conclusions.

## Authority coverage

| Changed surface | Governing authority read and applied | Source and proof inspected |
|---|---|---|
| Change packet, original task, prior review, remediation evidence | `AGENTS.md` §§11-16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md` | Approved spec/task, every R1 review artifact, remediation task and recorded commands, complete commit range |
| Verification wire contracts, IDs, public errors, schemas | `AGENTS.md` §§2-4, 9, 12, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md` | `wyrd-spec/src/{verification,ids,error,auth/principal_kind}.rs`, schema generator, generated schemas and schema tests |
| SYSTEM principal, JWT issuance/verification, public-route exclusion | `AGENTS.md` §§2, 9; `architecture/wyrd-security-posture.md`; `architecture/agent-rules.md` audit/tenancy rules; `languages/errors.md` | auth issue/verify/runtime/auth/server changes, principal migrations and SQL, principal and route tests |
| Tenant SQL, migrations, queue claims, leases, settlement, idempotency, dispatch | `AGENTS.md` §§3-6, 9-12, 16; `architecture/agent-rules.md` SQL/RLS/transaction rules; `architecture/references/architecture/patterns.md`; `languages/rust-core.md` | all three migrations, `queries/verifier_runs.rs`, auth/service-account queries, Postgres integration tests |
| Supervised runtime, scheduling, permits, cancellation, health, telemetry | `AGENTS.md` §§4-6, 9-12, 16; `architecture/wyrd-design.md`; `operations/deployment-and-release.md`; `operations/reliability-and-recovery.md`; `domain/analytical-operations-reliability.md` | `verification/{mod,clock,engines,health,permits,runner,scheduler}.rs`, server composition/config/metrics/readiness, runtime tests |
| Result mapping and Bifrost publication | `architecture/bifrost-design.md`; `architecture/agent-rules.md` name-mapping rule; `domain/vala-architecture.md`; `domain/olap-serving.md`; `domain/analytical-operations-reliability.md`; `domain/evaluation.md` | `verification/{publisher,results}.rs`, Bifrost facade use, result table declarations, mapping/publication tests and role-separated journey |
| Gate/Scribe authorization, scope, tenant identity, audit cardinality | `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `architecture/agent-rules.md`; `languages/agent-harness.md`; `domain/vala-architecture.md` | `gate/mod.rs`, `scribe/execution_lanes.rs`, audit projection/staging, authenticated gRPC and audit tests |
| HTTP/OpenAPI/MCP server surfaces | `AGENTS.md` §§2, 9, 11-12; `architecture/wyrd-design.md`; `languages/agent-harness.md`; `languages/errors.md` | typed routes/service, router/OpenAPI registration, MCP descriptors/handlers, route/OpenAPI/MCP journeys |
| Shared client and Rust SDK | `AGENTS.md` §§2-3, 9, 11; `architecture/wyrd-design.md` client model; `architecture/references/architecture/patterns.md`; `languages/rust-core.md` | `wyrd-client::Verification`, thin Rust SDK export, live-server Rust journey |
| Python SDK/PyO3/package/stubs | `AGENTS.md` §§7-8, 11; `languages/pyo3-boundaries.md`; `languages/python-api-and-stubs.md`; `languages/errors.md` | Python SDK Rust wrapper, public package exports, generated stubs, unit and integration journey |
| TypeScript/N-API/declarations | `AGENTS.md` §§2, 9, 11; `languages/typescript-guide.md`; `languages/errors.md` | native wrapper, public TS handle, generated declarations/error union, integration journey |
| Testing fixtures and user journeys | `AGENTS.md` §11; `architecture/agent-rules.md` test placement/journey rules; `languages/testing-workflows.md` | in-module pure tests, `pg_*` Postgres/live-server tests, shared fixture, Rust/Python/TS/MCP and role-separated journeys |
| Deployment, configuration, docs, and tooling | `AGENTS.md` §§11-12; `architecture/operations/README.md`; `operations/deployment-and-release.md`; `operations/reliability-and-recovery.md`; `languages/testing-workflows.md` | typed config, Docker/Kubernetes grace periods, self-hosting docs, schema inventory, `mise.toml`, recorded docs/codegen checks |

## Applicable-rule results

| Rule | Evidence | Result |
|---|---|---|
| Durable contracts stay in PyO3-free, IO-free `wyrd-spec`; durable runtime behavior stays server-owned | Wire types/errors/IDs are in `wyrd-spec`; SQL/runtime/publication are in `wyrd-sql`/`wyrd-server`; no PyO3 or IO was added to `wyrd-spec` | PASS |
| Shared client is the sole SDK-facing implementation; language SDKs remain projections | `wyrd-client/src/verification.rs` owns HTTP behavior; Rust re-exports it; Python and TS wrap it without duplicating durable queue or transport behavior | PASS |
| Public HTTP/MCP/SDK surfaces share typed contracts and stable Wyrd errors | Routes, MCP, and clients consume `StartVerificationRun*` and status types; new public errors use derive-backed `WyrdError`; OpenAPI and schema projections are registered | PASS |
| SYSTEM is tenant-local, credentialless, exact-Verifier-scoped, and excluded from public principal management | Principal/JWT contracts, provisioning migration/query exclusions, issuance path, real-route refusal tests, and Gate matrix agree with `wyrd-security-posture.md` | PASS |
| One canonical audit decision is recorded before Gate allows or denies a write; engine mechanics add no audit | `Gate::authorize_record_write` combines permission, reserved-table matrix, and mandatory SYSTEM scope into one outcome and one append; scheduler/runner/publisher transitions append none | PASS |
| Tenant SQL uses the sanctioned owners and caller-owned transaction lifecycle | Remediation removed runtime/fixture `PgPool` fields; scheduler, runner, publisher, and fixture use `WyrdPostgres::tenant_conn`; cross-tenant lists use `OperatorPool`; query methods do not commit caller connections | PASS |
| Result schemas bind values by name and fail closed on divergence | `ResultPayloadBuilder::assemble` indexes named arrays by `DomainTable::arrow_fields`, rejects missing/duplicate/unexpected columns, and has reordered/negative unit proof | PASS |
| Bifrost publication uses the shared facade and ACK boundary without a direct/local Scribe write | `ResultPublisher` mints SYSTEM authority and writes through `wyrd_client::Bifrost`; details precede summary; completion follows all ACKs; replay/crash and no-local-Scribe journeys cover the boundary | PASS |
| Queues, concurrency, deadlines, cancellation, restart, and shutdown are bounded | Concrete owners, permit ceilings, leases/fences, timeouts, supervised restart, cancellation-aware transaction admission, fenced late-claim release, and bounded drain are present and tested | PASS |
| Rust workflows are struct-centered and async is limited to IO/composition | `VerificationRuntime`, `VerificationScheduler`, `VerifierRunner`, `ResultPublisher`, `ResultPayloadBuilder`, `VerificationControl`, and `Verification` own their state; free helpers are pure conversions/labels | PASS |
| New and materially modified Rust items are documented, including errors and cancellation where applicable | Runtime, SQL, contracts, clients, helpers, fields, variants, and new tests carry intent-focused rustdoc and fallible APIs document errors; async lifecycle owners document cancellation/partial progress | PASS |
| Types in Rust fields, signatures, bounds, and impl headers are imported at module scope and named bare | Several newly added or materially rewritten items still use qualified paths; see `STD-004-R2-001` | **FAIL** |
| Test tier and placement match dependency/runtime ownership | Pure tests are in-module; Postgres/live HTTP/gRPC and multi-crate journeys use `pg_*`/external targets; Python and Node behavior run in their native runtimes | PASS |
| Every public surface has a real client-to-server journey | Rust, Python, TypeScript, and MCP manual-run journeys exist; the role-separated Bifrost journey proves remote result/detail publication and identity | PASS |
| Generated schemas/stubs/declarations and served OpenAPI have owning-source proof | Candidate includes source plus generated projections; implementation evidence records passing `codegen:check`, Python typing, TS typing, and served OpenAPI integration | PASS |
| Deployment and documentation expose the runtime's bounded shutdown/configuration behavior | Typed config, Docker/Kubernetes 45-second termination budget, server 35-second shutdown default, and self-hosting docs align with the runtime's 30-second drain | PASS |
| Required boundary and verification gates are credible | Recorded full lanes pass; this review reran `check:from-pools-allowlist`, `check:tenant-isolation`, `check:client-tier`, and `git diff --check`, all exit 0 | PASS |

## Material findings

### STD-004-R2-001 — Changed Rust items still hide dependency ownership behind qualified types

- **Violated rule:** `architecture/agent-rules.md` requires types to be imported in the module's top-level `use` block and used as bare names in fields, function parameters/returns, trait bounds, and `where` clauses. This applies to materially modified Rust and tests as well as public production items.
- **Exact locations:**
  - `crates/wyrd/wyrd-server/src/app/server.rs:513` returns `Option<crate::verification::VerificationRuntime>` and the same new method spells `RuntimeLimits`/`VerificationRuntime` through `crate::verification` in its body instead of declaring the dependency at module scope.
  - `crates/wyrd/wyrd-server/src/state.rs:2088` declares `Arc<crate::verification::health::VerificationHealth>`.
  - `crates/wyrd/wyrd-server/src/verification/publisher.rs:88` returns `fmt::Result` after the R1 mechanical import correction, leaving the return type qualified.
  - `crates/wyrd-spec/src/ids.rs:266` materially replaces the binding-ID implementations with `uuid7_id_type!`, whose generated `fmt` signature uses `fmt::Formatter` and `fmt::Result` rather than imported bare names.
  - `crates/vala/vala-bifrost-redux/src/gate/mod.rs:2493-2506` adds `AnyTableScribe` using `impl crate::contracts::Scribe`, `crate::contracts::ScribeIngressFrame`, and `Result<crate::contracts::FrameAdmission, crate::contracts::ScribeError>` despite the test module's top-level import manifest.
- **Evidence:** All cited lines are introduced or materially rewritten in the base-to-candidate diff and are reachable compiled production or test items. The R1 correction fixed its cited qualified signatures but did not exhaust the cumulative changed surface.
- **Consequence:** The candidate still violates the mandatory Rust source-shape rule and obscures the owning dependency at the exact composition, state, generated-identity, publication, and Gate-test seams touched by this task. Behavior is unaffected, but repository acceptance is not met.
- **Testable correction:** Add the exact types (using collision-resolving aliases such as `FmtResult` only where needed) to each module's existing top-level import block and use bare names in the cited fields, impl header, parameters, and return types. Do not reorganize modules or change behavior. Reinspect the complete cumulative Rust diff for qualified types in fields/signatures/bounds/impl headers, then run `mise run fmt`, `mise run lints`, and the affected crate lanes. No new test or permanent source check is required.

## Verification limits

- This review relied on the candidate's recorded successful full Rust, SQL, Vala, Bifrost, SDK, MCP, codegen, docs, format, lint, and boundary lanes rather than rerunning the multi-minute suites. It independently reran the four fast static checks listed above.
- `check:from-pools-allowlist` exited 0 but emitted its existing `rg: python/: No such file or directory` diagnostic; the inspected verification runtime/fixture sources independently contain no raw `PgPool`, `TenantConn::acquire`, or `RunnerPools` occurrence.
- No external credentials or cloud-storage lanes apply to this task. The task's nonexistent `test:e2e` name was replaced in the implementation evidence by the repository's actual `test:cards:integration` lane covering the Rust SDK and HTTP route journey; the other first-class surfaces have their own recorded integration lanes.

## Overall result

**FAIL**

The candidate satisfies the architecture, tenancy, security, audit, durability,
surface-alignment, testing, generated-artifact, and deployment rules reviewed
above, and it closes the seven R1 gaps. It still has one bounded mandatory
repository-style violation (`STD-004-R2-001`) in changed Rust type spellings.
