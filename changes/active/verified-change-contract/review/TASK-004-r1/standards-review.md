# TASK-004 R1 Repository Standards Review

**Immutable base:** `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
**Immutable candidate:** `49ad24707de47378b9df51764034c49a129c6b8b`
**Scope:** complete cumulative repository-standards audit only; task acceptance and Ponytail validation are excluded.
**Overall result:** **FAIL**

## Material Source-Local Findings

### Critical

None.

### Important

- **STD-004-R1-001 — The new verification runtime propagates raw `PgPool` through production and library structs and constructors.**
  **Violated authority:** `architecture/agent-rules.md` bans raw `sqlx::PgPool` from library code and permits only `&mut TenantConn<'_>` for tenant-scoped work and `&OperatorPool` for authorized cross-tenant work in function signatures and struct fields. `AGENTS.md` requires tenant work to remain behind the repository SQL boundary and keeps pool construction/acquisition in `WyrdPostgres`/approved owners.
  **Exact locations:** `crates/wyrd/wyrd-server/src/verification/publisher.rs:17,65-68,86-96`; `crates/wyrd/wyrd-server/src/verification/runner.rs:18,55-61`; `crates/wyrd/wyrd-server/src/verification/scheduler.rs:14,32-58`; `crates/wyrd/wyrd-server/src/verification/mod.rs:343-366`; `crates/wyrd/wyrd-testing/src/verification.rs:10,63-89`.
  **Validated evidence:** `ResultPublisher`, `RunnerPools`, and `VerificationScheduler` store cloned application `PgPool` values, their constructors accept those pools, and the runtime builder obtains them through `state.postgres.app_pool().clone()`. The new reusable verification fixture likewise stores and accepts a `PgPool`. `mise run check:from-pools-allowlist` exits zero because that check only governs pool-construction allowlisting; it does not make pool propagation compliant with the explicit signature/field ban.
  **Consequence:** the runtime bypasses the required connection-owner API at a tenant-sensitive boundary, makes raw pool access available to every future method on these services, and defeats the repository rule that keeps tenant transaction setup and role/RLS selection centralized.
  **Testable correction:** remove `PgPool` from the listed fields and signatures and route each tenant transaction through the existing Wyrd Postgres/application-state connection owner, continuing to pass `&mut TenantConn<'_>` into tenant SQL and `&OperatorPool` only into the three authorized cross-tenant queue reads. Keep connection construction at the existing boundary. Prove the affected runtime/fixture journeys and add a source check or equivalent review evidence showing no raw pool remains in the changed library fields or signatures.

- **STD-004-R1-002 — Result payload construction relies on positional Arrow alignment instead of mapping columns by name.**
  **Violated authority:** `architecture/agent-rules.md` requires columns/fields to be mapped by name whenever schemas can diverge; `architecture/references/domain/arrow-analytical-interop.md` requires stable field identity/name mapping across analytical projections and fail-closed behavior for missing or incompatible fields.
  **Exact locations:** `crates/wyrd/wyrd-server/src/verification/results.rs:245-262,285-309,336-394`.
  **Validated evidence:** `summary`, `drift_features`, and `eval_items` build anonymous `Vec<ArrayRef>` values in assumed table order. `finish<T>` independently obtains `T::arrow_fields()` and gives `RecordBatch::try_new` the two positional vectors. No name joins each produced array to its table field. Existing tests read the resulting current schema with `column_by_name`, but they do not prevent an authored table-field reorder or same-typed insertion from silently pairing values with the wrong names.
  **Consequence:** a legitimate evolution or reorder of a result table schema can silently mislabel same-typed result data (or fail only incidentally on differing types), corrupting verification evidence before Scribe admission.
  **Testable correction:** assemble the user columns as named values and order/validate them against `T::arrow_fields()` by field name, rejecting missing, duplicate, or unexpected names before creating the batch; append managed correlation columns by their names. Add a focused test that changes/provides schema order independently of construction order and proves values remain attached to the intended names, plus a negative test for a missing/extra field.

- **STD-004-R1-003 — New Rust fields, signatures, return types, and trait bounds use fully qualified paths instead of the mandatory top-level import manifest and bare names.**
  **Violated authority:** `architecture/agent-rules.md` requires dependencies to be imported at module top and bare type names in struct fields, enum fields, function parameters/returns, trait bounds, and `where` clauses. The rule applies to production, feature-gated, fixture, and test code; only test-module import blocks and the narrow `use Trait as _` case are exceptions.
  **Exact locations:** `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:305,719,747,796,827,931,979,1008,1035,1077,1124,1165,1191,1214,1257,1296,1321,1340,1399,1459,1546,1578`; `crates/wyrd/wyrd-server/src/verification/clock.rs:77`; `crates/wyrd/wyrd-server/src/verification/mod.rs:98`; `crates/wyrd/wyrd-server/src/verification/publisher.rs:211-217,319,326,338-339,363`; `crates/wyrd/wyrd-server/src/verification/results.rs:55,505`; `crates/wyrd/wyrd-server/src/verification/runner.rs:533,584,588`; `crates/wyrd/wyrd-testing/src/verification.rs:36,39`; `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:91`; `crates/wyrd/wyrd-sql/tests/pg_verifier_runs.rs:77`; `sdks/wyrd-sdk-ts/native/src/verification.rs:23`.
  **Validated evidence:** the new SQL owner repeatedly returns `sqlx::Error` without importing an error alias; runtime fields and signatures spell `std::sync::*`, `tokio::task::JoinError`, `serde_json::Error`, `wyrd_queue::*`, and `chrono::Duration`; fixture/test signatures spell `wyrd_spec::*`; the N-API decoder spells `std::result::Result`. These are newly added items. `mise run lints` does not enforce this repository-specific source-shape rule. Ordinary imports inside the new `#[cfg(test)] mod tests` blocks are permitted and are not part of this finding.
  **Consequence:** the candidate violates a mandatory repository source-shape boundary and hides the true dependency manifest across long implementation signatures, even though the code compiles.
  **Testable correction:** import each named type or a collision-resolving alias in the owning module's top-level `use` block and use the bare name throughout every changed field, signature, bound, and `where` clause. Preserve behavior and rerun formatting, lints, and the affected Rust/SDK tests.

### Suggestions

None.

## Authority Coverage

The review confirmed `.codegraph/` is absent, then read the repository router and every authority routed by the changed surfaces:

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/operations/README.md`
- `architecture/operations/deployment-and-release.md`
- `architecture/operations/reliability-and-recovery.md`
- `architecture/references/README.md`
- `architecture/references/doctrine/positioning-and-vocabulary.md`
- `architecture/references/doctrine/architecture-constraints.md`
- `architecture/references/architecture/patterns.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/rust-core.md`
- `architecture/references/languages/pyo3-boundaries.md`
- `architecture/references/languages/python-api-and-stubs.md`
- `architecture/references/languages/typescript-guide.md`
- `architecture/references/languages/testing-workflows.md`
- `architecture/references/languages/agent-harness.md`
- `architecture/references/languages/errors.md`
- `architecture/references/domain/vala-architecture.md`
- `architecture/references/domain/evaluation.md`
- `architecture/references/domain/telemetry-observations.md`
- `architecture/references/domain/olap-serving.md`
- `architecture/references/domain/arrow-analytical-interop.md`
- `architecture/references/domain/analytical-operations-reliability.md`

`domain/datafusion.md` and `domain/iceberg.md` are not applicable: the candidate does not change planning/provider behavior, Iceberg publication, catalog commits, partitioning, compaction, or object-store layout. `domain/drift-monitoring.md` is deferred to TASK-005's production Drift implementation; this task only adds the generic engine slot and result projection. `operations/runbooks.md` is not applicable because no incident procedure or operator mutation runbook changes.

| Changed surface | Applicable repository authority | Result | Exact evidence |
|---|---|---|---|
| Principal kind, token issue/verify, provisioning, revocation exclusions | `AGENTS.md`; Wyrd design/doctrine; security posture; architecture constraints/patterns; errors/testing references | PASS | `PrincipalKindTag::System`, tenant issuer/verifier paths, keyless stable provisioning migration, public lookup exclusions, and principal/route tests preserve the one internal tenant identity path. |
| Verification wire types, IDs, errors, generated schemas | Wyrd design/doctrine; Rust core; errors; testing workflows | PASS | `wyrd-spec/src/verification.rs`, UUIDv7 newtypes, derive-backed `WyrdError` variants, schema generator registrations, matching schema/golden output, and served OpenAPI coverage align. |
| Run/dispatch migrations and SQL queue | `AGENTS.md`; agent rules; security posture; patterns; Rust core; testing workflows | **FAIL** | Tables force RLS, operator reads are explicit, queue methods leave caller transactions open, and SQL tests are extensive; however raw pool propagation and qualified return types violate `STD-004-R1-001` and `STD-004-R1-003`. |
| Supervised scheduler/runner, permits, health, metrics, shutdown | Rust core; telemetry observations; analytical reliability; operations deployment/recovery | **FAIL** | Bounded 16/4 permits, leases, timeouts, restart supervision, bounded labels, and 30-second drain are present and tested; connection ownership and source shape fail under `STD-004-R1-001`/`003`. |
| Bifrost Gate/Scribe result publication and Arrow batches | Bifrost design; security posture; Vala/OLAP/Arrow/reliability references | **FAIL** | SYSTEM-only closed table matrix, remote `wyrd_client::Bifrost` write, ACK ordering, sealed replay, and Scribe journeys exist; payload columns are positionally paired with independently obtained schemas (`STD-004-R1-002`). |
| HTTP routes and served OpenAPI | Server/contract rules; patterns; errors; testing workflows | PASS | Typed bodies/responses, catalog errors, scrubbed tracing instrumentation, route registration, and `pg_openapi_contract` cover all three endpoints. |
| MCP tools | Agent harness; architecture constraints; errors; testing workflows | PASS | Read descriptors remain visible, the write descriptor is scope-gated, tool execution reuses `VerificationControl`, and the real discover-act-observe journey covers permission denial. |
| Shared Rust client and Rust SDK | Client pattern; Rust core; errors; testing workflows | PASS | One `Verification` handle owns `WyrdClient`; the Rust SDK re-exports it and drives a real server journey without duplicating durable state. |
| Python/PyO3 package and stubs | PyO3 boundaries; Python API/stubs; errors; testing workflows | PASS | Wrapper lives in the Python SDK, uses `__new__` with explicit signature, detaches the GIL around shared runtime IO, registers/exports publicly, and has unit plus integration coverage. `check:pyo3-scope` passed independently. |
| TypeScript/N-API package and declarations | TypeScript guide; errors; testing workflows | PASS | Thin N-API methods delegate to the shared handle, ergonomic TS types retain wire names, integration/typecheck evidence is recorded, and `ts:napi:check` independently reproduced the generated declaration. |
| Deployment/configuration/docs | Operations deployment/release and reliability/recovery; `AGENTS.md` verification rules | PASS | Default server drain is 35 seconds, Kubernetes grace is 45 seconds, role behavior and ingest endpoint are documented, and recorded `docs:check` passed. |
| Test/gate changes | `AGENTS.md`; agent rules; testing workflows; implementation execution | PASS | Real Rust/Python/TypeScript/MCP journeys and SQL/Bifrost integration suites exist; Postgres tests remain in gated `pg_*` targets; no test or gate was disabled. |

## Applicable Rule Results

| Applicable rule | Result | Source evidence |
|---|---|---|
| Durable contracts live in `wyrd-spec`, durable orchestration stays server-owned, and all first-class clients consume `wyrd-client` | PASS | Contract, server, shared handle, and thin Rust/Python/TypeScript/MCP projections follow the prescribed ownership chain. |
| `wyrd-spec` remains IO-, async-, SQL-, Arrow-, and PyO3-free | PASS | New contract code is synchronous validation/schema material only; `check:pyo3-scope` passes. |
| Public durable identifiers use domain newtypes and validate UUIDv7 | PASS | `VerificationRunId`, `VerificationResultId`, `BindingId`, and `OperatorDispatchId` are used at public/control boundaries and storage decode rejects malformed versions. |
| Tenant SQL uses `TenantConn`; cross-tenant discovery uses `OperatorPool`; callees do not commit/rollback | PASS | Queue tenant methods accept caller-owned `TenantConn`; only runnable/due/depth reads accept `OperatorPool`; commits occur in the acquiring service/runtime owner, not queue callees. |
| Raw SQL pools do not appear in library fields or signatures | **FAIL** | `STD-004-R1-001`. |
| Authorization decisions use the canonical audit path transactionally; engine mechanics do not audit | PASS | HTTP/MCP share `VerificationControl`; allowed/denied manual decisions append before commit, Gate owns the result-write decision, and scheduler/claim/publish/settlement paths add no auth audit. |
| Result publication uses the tenant SYSTEM principal, Gate/Scribe, and acknowledged remote Bifrost path | PASS | Issuance, Gate matrix, publisher, ACK/replay tests, and no-local-Scribe journey all use the existing identity and client path. |
| Analytical columns map by stable name rather than positional coincidence | **FAIL** | `STD-004-R1-002`. |
| Stateful Rust workflows have cohesive concrete owners and async is limited to IO/composition | PASS | `VerificationRuntime`, `VerificationScheduler`, `VerifierRunner`, `ResultPublisher`, `VerificationControl`, `VerifierRunQueue`, and client `Verification` own their dependencies/workflows; pure validation and payload mapping remain synchronous. |
| New/materially changed Rust items have substantive rustdoc and required error/panic/cancellation sections | PASS | The new modules document private/public items, fallible functions, panic invariants, and cancellation/partial-progress behavior; no placeholder docs were found. |
| Imports form the top-of-module dependency manifest and signatures use bare names | **FAIL** | `STD-004-R1-003`. |
| Public handlers use typed contracts, stable derive-backed errors, and scrubbed trace instrumentation | PASS | Verification routes and shared MCP service satisfy all three requirements; no parallel problem mapper or surface-only error code was added. |
| Python and N-API boundaries stay thin and do not hold foreign-runtime values across await | PASS | Python converts to Rust types before `py.detach`; N-API accepts owned strings, decodes them, and awaits only shared Rust handles. |
| Generated JSON schemas, Python stubs, and N-API declarations come from their owners and are drift checked | PASS | Recorded `codegen:check` passes; independent `ts:napi:check` passes; source annotations/registration changes account for generated deltas. |
| Every shipped user/agent surface has a real client-server-client journey | PASS | Rust, Python, TypeScript, and MCP verification journeys are present; HTTP negative/tenant/idempotency behavior is covered by the real-router Postgres target. |
| Metrics have bounded labels and runtime work is bounded, timed out, supervised, and drained | PASS | Labels are closed implementation/outcome/capability/status sets; permits, tenant rounds, execution/publication timeouts, restart backoff, and shutdown grace are finite. |
| No new legacy vocabulary, compatibility route, public SYSTEM lifecycle, new permission, direct Scribe write, lint suppression, or gate circumvention appears | PASS | Complete cumulative diff contains none of the prohibited mechanisms; `git diff --check` passes. |

## Open Questions

None. Each correction is determined by existing repository authority and requires no product, security, persistence, or public-contract decision.

## Verification Notes

- Confirmed `.codegraph/` is absent before source discovery.
- Inspected the complete 116-file base-to-candidate range, changed manifests/lockfile, migrations/RLS/grants, generated artifacts, HTTP/MCP registration, all language projections, deployment files, and surrounding owners/consumers.
- Confirmed `HEAD` was `49ad24707de47378b9df51764034c49a129c6b8b` at review start and after inspection. The untracked review directory contains only concurrent review artifacts and is outside the immutable candidate.
- Independently passed:
  - `mise run check:pyo3-scope`
  - `mise run ts:napi:check`
  - `mise run check:from-pools-allowlist` (not sufficient to enforce the raw-pool propagation rule)
  - `git diff --check 9431906eeb1c7b67a0efcec09487fd1d848f70a8 49ad24707de47378b9df51764034c49a129c6b8b`
- The task's immutable implementation evidence records passing principals, SQL, shared/Wyrd/Vala/Bifrost, Rust/Python/TypeScript/MCP journey, codegen, tenant-isolation, client-tier, unwrap-audit, docs, format, lint, and whitespace gates. Those broad suites were cross-checked against current task definitions and tests but not all rerun in this standards slice.
- Existing automated gates do not enforce `STD-004-R1-001`, `STD-004-R1-002`, or `STD-004-R1-003`; their green results do not close these source/architecture violations.

## Overall Verdict

**FAIL** — the candidate broadly aligns its contracts, security, tenancy, audit, SDKs, deployment, and verification evidence, but three mandatory repository rules remain violated: raw pool propagation in the runtime/fixture, positional analytical column assembly, and fully qualified types in newly added Rust fields/signatures. All three are bounded corrections under existing authority, but repository compliance requires them before approval.
