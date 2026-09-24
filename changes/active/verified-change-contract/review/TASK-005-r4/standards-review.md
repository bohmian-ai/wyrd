# TASK-005 round 4 repository standards review

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`; cumulative base `f8811ac5035c3aa165d34c38992f9889b3c9081f`; candidate `6d93b75825926ac27c9fd7de90879b19d67e39b3`.
- Approved change authority: `changes/active/verified-change-contract/spec.md` revision 36 and the original `tasks/TASK-005-production-drift-verifier.md`. Prior r1–r3 reviews and remediation tasks provide closure history, not replacement authority.
- Governing rules read: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`. The reference router selected `references/doctrine/{positioning-and-vocabulary,architecture-constraints}.md`, `references/architecture/patterns.md`, `references/languages/{spec-driven-development,implementation-execution,rust-core,python-api-and-stubs,typescript-guide,testing-workflows,errors,agent-harness}.md`, and `references/domain/{drift-monitoring,olap-serving,iceberg,arrow-analytical-interop,analytical-operations-reliability}.md`. No `.codegraph/` directory exists.
- Reviewed the full changed-file inventory and cumulative base-to-candidate diff, with focused source inspection of the new fitter shutdown code, SQL and token owners, language journeys, task packet, and prior finding closures. `git diff --check` passes and HEAD still matches the candidate. Recorded verification results were not rerun in this review.

## Authority coverage and rule results

| Changed surface | Applicable authority | Result and source evidence |
|---|---|---|
| Approved spec, active task, architecture/security/Drift text and review evidence | `AGENTS.md` §§1–2, 12, 14–16; spec-driven and implementation-execution references; design, doctrine, security posture | **PASS**. Revision 36 remains approved; TASK-005 states the SYSTEM read token, fixed SQL and query-service path; r3 remediation/evidence is in the active packet. The added r3 record and r4 evidence do not change product authority. |
| `wyrd-spec` Card behavior, JSON schemas, TypeScript projection | `AGENTS.md` §§2–4, 8–9, 16; design/doctrine; positioning, patterns, Rust, error and TypeScript references | **PASS** for ownership and contract shape. Drift remains a typed Verifier; spec stays IO- and PyO3-free. Generated schemas and TS projection match the added baseline status contract; recorded `codegen:check` is green. |
| SYSTEM token, query service and scheduled query consumer | `AGENTS.md` §§2–6, 9–10; agent-rules tenant/audit boundaries; security posture, Bifrost design; patterns, agent-harness and OLAP references | **PASS** for repository boundaries. Issuance grants the one observation-table permission; verifier accepts a closed SYSTEM purpose; Drift reads via the query service and shared scheduled consumer, leaving Gate/Oracle authorization and auditing in place. |
| Vala baseline, PSI, SPC and Custom scoring | `AGENTS.md` §§3–6, 10, 16; agent-rules imports/rustdoc/struct rules; Rust, Drift, analytical-interop and reliability references | **FAIL** on two explicit code rules: see `REPO-R4-001` and `REPO-R4-002`. Scoring remains synchronous and Vala-owned; the cooperative fit API and bounded scoring mechanisms otherwise fit the ownership rules. |
| Server baseline fitter, shared verification runtime and Postgres shutdown test | `AGENTS.md` §§3–6, 11, 16; agent-rules imports, async, documentation and test-tier rules; Rust, testing and reliability references | **FAIL** on the same import rule at the new test helper (`REPO-R4-001`). The fitter otherwise uses `RuntimeLimits`, the shared permits, PostgreSQL claim/settlement and a `test-support`-only gate. Its new lifecycle methods document cancellation and settlement behavior. |
| Baseline SQL, Forge namespace, migrations and SQL tests | `AGENTS.md` §§2–3, 9, 11, 15–16; agent-rules `TenantConn`/`OperatorPool`/RLS rules; Bifrost design, Iceberg, OLAP and reliability references | **PASS** for tier/transaction structure. The baseline queue uses tenant connections for tenant rows and operator access for due-tenant discovery; the migration has tenant-qualified keys and RLS; Forge's Rust list and SQL constraints include the verification namespace. |
| CLI fixtures, Rust/Python/TypeScript journeys, test harness, mise/script and lockfile | `AGENTS.md` §§3, 7–8, 11–12, 16; agent-rules runtime ownership and gate integrity; PyO3, Python, TypeScript, testing and implementation references | **PASS** except the new Rust test helper imports in `REPO-R4-001`. Each language's journey uses its own runtime against the real server. The TS test harness starts the verification runtime; named tests have exact recorded commands. The Oracle cleanup test still checks the same assertion within its prior bound. |

## Applicable cross-cutting checks

| Rule | Result | Evidence |
|---|---|---|
| Language-agnostic server ownership and tier direction (`AGENTS.md` §§2–3, 7–9) | **PASS** | Vala Rust owns scoring, server owns durable orchestration, SQL owns persistent queue, SDK additions are typed projection and journeys. |
| Tenant isolation, authorization and audit (`AGENTS.md` §§2, 9; agent-rules; security posture) | **PASS** for standards shape | Scoped SYSTEM token and query service preserve the registered-table decision and audited Gate/Oracle path. Domain reviewer owns deeper behavioral validation. |
| Struct-centered design, async boundary and shutdown ownership (`AGENTS.md` §§5–6; agent-rules) | **PASS** | `BaselineFitter` retains its cohesive lifecycle; `fit_within_drain` composes actual asynchronous work; the blocking computation remains synchronous and awaited before lease settlement. |
| Import placement (`architecture/agent-rules.md`, “All `use` statements live at the top of the module”) | **FAIL** | `REPO-R4-001`. |
| Rustdoc for materially modified fallible Rust (`AGENTS.md` §16; agent-rules rustdoc requirement) | **FAIL** | `REPO-R4-002`. |
| Test tiers, exact named commands, generated artifacts, gate integrity (`AGENTS.md` §§8, 11–12; agent-rules) | **PASS** | r3 evidence records a focused Postgres-wrapped exact test, `fmt`, `lints`, Wyrd, Bifrost server integration and Drift journey green after the last production change; earlier evidence records Vala/SQL/codegen/client-tier lanes. No gate was disabled. |

## Material repository-rule findings

### REPO-R4-001 — imports inside changed functions

- **Rule:** `architecture/agent-rules.md` requires all `use` statements at the module top. Its narrow function exception is `Trait as _` in a single generic function where module scope is unsuitable.
- **Location/evidence:** New `score_psi_counts` contains `use wyrd_spec::card::drift::DriftMethod;` at `crates/vala/vala-drift/src/psi/mod.rs:171`. The new Postgres test helper `baseline_artifact` contains five ordinary function-scoped imports at `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:1762-1766` and two trait imports in a local block at lines 1784–1785; the helper is not generic.
- **Consequence:** The changed modules hide dependencies from their top import blocks, contrary to a mandatory repository structural rule. This is independent of runtime test success.
- **Testable correction:** Move these imports to their module tops, remove redundant imports already present there, and rerun `mise run fmt`, `mise run lints`, and the affected owning tests. No behavior change is needed.

### REPO-R4-002 — materially modified fallible PSI fit entry point lacks rustdoc

- **Rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` make rustdoc mandatory for every materially modified Rust item and require `# Errors` for fallible functions; missing documentation is an explicit hard blocker.
- **Location/evidence:** The cumulative diff turns `crates/vala/vala-drift/src/psi/mod.rs:67-73` `pub fn fit_psi_baseline` into a wrapper over the new cancellable entry point. The function has no rustdoc or `# Errors`; the adjacent new `fit_psi_baseline_until` has both.
- **Consequence:** The public entry point's behavior and error contract remain undocumented for callers despite a material change to its implementation relationship.
- **Testable correction:** Add rustdoc explaining that this entry point performs an uncancelled fit through the same fitting path, and document its fitting error conditions under `# Errors`; run `mise run fmt` and the Vala lane. No implementation change is needed.

## Overall result

**FAIL** — the candidate violates two explicit repository rules. The r3 shutdown fix itself presents no additional repository-standards concern from this review. The recorded lanes establish compile/test outcomes but cannot waive the import or rustdoc requirements.
