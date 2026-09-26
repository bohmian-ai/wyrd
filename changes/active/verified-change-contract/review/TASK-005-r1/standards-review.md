# Repository Standards Review — TASK-005

## Review Findings

### Critical

- None.

### Important

- **REPO-001 — [`crates/wyrd/wyrd-server/src/verification/drift.rs:583`](../../../../../crates/wyrd/wyrd-server/src/verification/drift.rs#L583): the Drift engine expands the reserved SYSTEM principal into a Bifrost reader.** `DriftEngine::context` loads the tenant's persisted SYSTEM identity, constructs `PrincipalKind::System`, and supplies a newly manufactured `PermissionSet` containing `bifrost_query:read` at lines 588–603. This conflicts with `architecture/wyrd-design.md:171-175`, which reserves SYSTEM exclusively for canonical verification-result publication with a fixed result-write capability, and `architecture/wyrd-security-posture.md:71-75,142-146`, which limits that principal to a token containing exactly `bifrost_record:write`. It also bypasses the security rule that every internal request is authenticated and authorized at its receiving boundary (`wyrd-security-posture.md:14-15`): the code creates the authority it then presents to Oracle instead of deriving it from a sanctioned grant/capability. The observable consequence is that the repository's least-privilege SYSTEM identity now has an undocumented read authority that cannot be revoked or bounded by its approved principal contract. **Correction:** remove the synthesized SYSTEM query permission and route fixed Drift plans through an architecture-approved, tenant-bound internal query capability or an actually authorized principal. Add a focused negative proof that the SYSTEM principal still cannot hold or exercise `bifrost_query:read`, plus a production Drift journey proving the sanctioned internal path still reads only the exact tenant/table/window.

- **REPO-002 — [`mise.toml:426`](../../../../../mise.toml#L426): the first-class TypeScript surface has no production Drift verifier journey.** The candidate exposes fitted-baseline state in `sdks/wyrd-sdk-ts/wyrd/src/index.ts:941-950`, but its TypeScript Bifrost journey lane runs only Oracle query, Bifrost write, OTEL export, and observation tests (`mise.toml:426-431`). The existing `verification-run.test.ts` is not in that lane and, even if run elsewhere, starts the default test server at line 89; the TypeScript testing binding only accepts `auditPublication` (`sdks/wyrd-sdk-ts/testing/index.d.ts:192`) and cannot enable the production verification runtime. It therefore proves request enqueue/readback only, not baseline fitting, production Drift execution, result persistence, or the newly exposed baseline status. This violates `AGENTS.md:67-70,378-405`, `architecture/agent-rules.md:18`, and `architecture/references/languages/testing-workflows.md:15-26`, all of which require a real client → server → client journey for every first-class SDK surface. **Correction:** reuse the existing TypeScript testing/server and `verification-run.test.ts` path, add the minimum test-only runtime toggle needed to compose the real verification runtime, and cover at least one fitted PSI/SPC lifecycle plus one scored production Drift result and the applicable permission denial. Include that target in the canonical TypeScript journey lane.

- **REPO-003 — [`crates/wyrd/wyrd-server/src/verification/drift.rs:672`](../../../../../crates/wyrd/wyrd-server/src/verification/drift.rs#L672): the new Rust test module has no rustdoc.** The candidate adds `#[cfg(test)] mod tests` without either an outer `///` or inner `//!` description. `AGENTS.md:705-718` and `architecture/agent-rules.md:35` require rustdoc on every new or materially modified Rust item, explicitly including modules and tests, and classify missing rustdoc as a hard merge blocker. **Correction:** add one concise module-level rustdoc comment explaining that the module proves fixed plan shape and aggregate decoding; no new test or abstraction is needed.

### Suggestions

- None. Optional refactors and unrelated improvements were excluded.

## Open Questions

- None. The SYSTEM-principal conflict is explicit in current authority; resolving it requires choosing an already approved authorization path, not interpreting the existing code as authority.

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `9cfe9b69c5990603e07458aa4402f6750131bfbc`
- Candidate remained `HEAD` throughout this review.
- Scope reviewed: complete `base..candidate` diff (53 files).

## Authority Coverage

| Changed surface | Governing authority read | Coverage result |
|---|---|---|
| Drift Card contract, validation, status, schemas | `AGENTS.md` §§2–3, 8–10, 16; `architecture/wyrd-design.md` Doctrine and Verifier; `architecture/wyrd-doctrine.mdx`; `doctrine/positioning-and-vocabulary.md`; `doctrine/architecture-constraints.md`; `languages/errors.md` via repository error rules | Covered; contract stays under `wyrd-spec`, remains typed/IO-free, and generated schemas align. |
| PSI/SPC/Custom fitting and aggregate scorers | `AGENTS.md` §§4–6, 10, 16; `languages/rust-core.md`; `domain/drift-monitoring.md`; `domain/vala-architecture.md`; `architecture/patterns.md` | Covered; Vala retains pure fit/score ownership and aggregate entry points reuse current scorers. |
| Oracle typed plans and source replacement | `architecture/bifrost-design.md` §§Query/Public surface; `domain/olap-serving.md`; `domain/analytical-operations-reliability.md`; `domain/vala-architecture.md`; `architecture/wyrd-security-posture.md` | Covered; plan ownership and tenant-qualified Oracle execution reviewed. Authorization failed under REPO-001. |
| Baseline queue, migration, RLS, leases, retries | `AGENTS.md` §§3, 9, 15; `architecture/agent-rules.md` SQL rules; `architecture/patterns.md` Storage/Registry; `architecture/wyrd-security-posture.md` Tenant/data isolation; `languages/rust-core.md` | Covered; `TenantConn`, `OperatorPool`, PostgreSQL clocks, RLS, and caller-owned transactions conform. |
| Server registration/status/fitter/runtime composition | `AGENTS.md` §§3, 5–6, 9, 16; `architecture/patterns.md`; `doctrine/architecture-constraints.md`; `domain/vala-architecture.md`; `languages/rust-core.md` | Covered; concrete owners and async/blocking boundaries conform except REPO-001 and REPO-003. |
| Rust/Python/TypeScript SDK and test surfaces | `AGENTS.md` §§2–3, 7–8, 11; `languages/python-api-and-stubs.md`; `languages/typescript-guide.md`; `languages/testing-workflows.md`; `architecture/wyrd-doctrine.mdx` Public surfaces | Covered; Python and Rust journeys exist. TypeScript production-runtime journey fails under REPO-002. |
| CLI fixtures and Parquet artifacts | `AGENTS.md` §§10–12; `languages/testing-workflows.md`; `domain/arrow-analytical-interop.md` principles routed through the Parquet/Arrow boundary | Covered; fixtures are repository-local, tenant-neutral test data and exercised by CLI journeys. |
| Generated JSON schemas and Python stubs | `AGENTS.md` §8 and §11; `architecture/agent-rules.md` generated-artifact rule; `languages/python-api-and-stubs.md` | Covered; candidate records `mise run codegen:check` PASS and generated outputs match the typed source. |
| Test lanes, skill mirrors, and failure-diagnosis workflow text | `AGENTS.md` §§11–16; `languages/spec-driven-development.md`; `languages/implementation-execution.md`; `languages/testing-workflows.md` | Covered; canonical/mirror skills are synchronized (`mise run check:skills-sync` PASS). |

## Rule-by-Rule Results

| Applicable repository rule | Evidence | Result |
|---|---|---|
| Durable contracts belong in `wyrd-spec`; it stays IO/async/PyO3/SQL free. | `card/drift.rs`, `card/verifier.rs`, and generated schemas add only typed validation/status data; no forbidden dependency entered the manifest. | PASS |
| Vala owns Drift computation; `wyrd-server` owns durable orchestration and remains the only serving surface. | `vala-drift` owns fitted/scoring types; `BaselineFitter` and `DriftEngine` compose storage/SQL/Oracle; no Vala listener was added. | PASS |
| Stateful workflows use concrete owning structs; pure deterministic transforms may remain free functions. | `BaselineFitter`, `DriftEngine`, `ObservationWindow`, and `DriftBaselineQueue` own dependencies/invariants; aggregate decoders and scoring transforms are pure helpers. | PASS |
| Async is restricted to actual IO; CPU/blocking Parquet fitting leaves the async worker. | `BaselineFitter::fit` performs storage IO asynchronously and uses `tokio::task::spawn_blocking` for Parquet decode/fit. Logical-plan and scoring work is synchronous. | PASS |
| Tenant SQL uses `TenantConn`; cross-tenant discovery uses `OperatorPool`; callees do not commit. | `drift_baselines.rs` tenant methods accept `&mut TenantConn`; `tenants_with_due_fits` alone accepts `&OperatorPool`; commits remain in `BaselineFitter::fit_next`, the caller. | PASS |
| PostgreSQL owns coordination clocks. | Migration/query SQL uses `statement_timestamp()` for due time, lease expiry, retry, and updates; callers bind durations only. | PASS |
| Persistent tenant data has RLS and tenant-qualified keys/FKs. | `20260601000032_drift_baselines.sql` uses `(data_tenant_id, verifier_uid)` PK, tenant-qualified Card FKs, forced RLS, and least-privilege grants. | PASS |
| Internal identities and permissions follow the fixed security contract. | `DriftEngine::context` synthesizes query permission for the result-writer-only SYSTEM principal. | **FAIL — REPO-001** |
| Public surfaces project the same typed status contract. | Rust status types, JSON schemas, and TypeScript `VerificationStatus.baseline` use the same `state/data/error` fields; Python consumes the wire Card status. | PASS |
| Every first-class SDK surface gets a real client → server → client journey for new user-visible behavior. | Rust and Python production-runtime journeys exist; TypeScript cannot enable the runtime and its canonical journey lane contains no Drift verifier execution. | **FAIL — REPO-002** |
| Python interpreter behavior is tested in Python; Python tests are top-level functions. | `test_drift_journey.py` uses one top-level `def test_*` and the public `wyrd` package. | PASS |
| External Rust tests earn their test binaries. | `pg_drift_baselines.rs` drives real Postgres; `drift_verification.rs` drives the real SDK/server/Bifrost journey. | PASS |
| Every new/materially modified Rust item, including modules/tests, has intent-bearing rustdoc. | New production types/functions and test helpers are documented, but `verification/drift.rs`'s `mod tests` is undocumented. | **FAIL — REPO-003** |
| Public/generated artifacts are regenerated, not allowed to drift. | Recorded `mise run codegen:check` PASS; schema and `.pyi` projections match source. | PASS |
| No gate was weakened or disabled. | No added `#[ignore]` outside the required gated journey, no test deletion to evade behavior, and the changed Forge wait retains its equality assertion; failure diagnoses are recorded in the task evidence. | PASS |
| New dependencies/features are minimal and correctly scoped. | `serde_json` is test-only in `vala-drift`; Arrow/Chrono/Parquet are Rust SDK dev-dependencies for the real Parquet journey; no Cargo feature was added. | PASS |
| No legacy vocabulary, compatibility route, second Drift scheduler, or client-side aggregation entered the diff. | Complete diff inspection found none; server-owned typed plans return aggregates to the existing Vala scorers. | PASS |
| Skill canonical source and Claude mirror remain identical. | Reviewer ran `mise run check:skills-sync` successfully. | PASS |

## Verification Notes

Implementation evidence records successful runs of `mise run test:vala`, `test:sql`, `test:wyrd`, `test:bifrost`, `test:bifrost:journey:sdk`, `test:wyrdstate:journey`, `test:storage:matrix`, `codegen:check`, `check:tenant-isolation`, `fmt`, `lints`, `ts:typecheck`, focused Oracle planning and Python Drift commands, and `git diff --check`.

This reviewer additionally ran, all passing:

- `mise run check:skills-sync`
- `mise run py:format`
- `mise run py:lints`
- `mise run py:typecheck`

The recorded green `test:bifrost` and `ts:typecheck` do not close REPO-002: the TypeScript lane explicitly omits a production-runtime Drift journey, and type-checking cannot prove server execution. No long-running verification lane was repeated.

## Overall Result

**FAIL**

The candidate violates the fixed SYSTEM-principal authorization contract, lacks required first-class TypeScript journey proof for the shipped Drift behavior, and contains one hard-blocking rustdoc omission. All three are bounded repository-compliance findings; no authority or source was missing.
