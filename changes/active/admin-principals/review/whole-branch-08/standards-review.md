# Repository Standards Review

## Review Findings

### Critical

None.

### Important

- **`REPO-R8-1` — new CLI administration surfaces expose credentials as ordinary command-line strings.** `crates/wyrd/wyrd-cli/src/platform/credential.rs:32-39` derives `Debug` for `PlatformEndpoint`, stores the platform credential as `String`, and accepts it through `--credential`; `crates/wyrd/wyrd-cli/src/principal/credential.rs:30-37` does the same for a tenant bearer through `--token`; and `crates/wyrd/wyrd-cli/src/client.rs:9-15,29-36` explicitly carries that plaintext as `&str` before wrapping it. This violates `AGENTS.md` §4's requirement to use `SecretString` plus redacted `Debug` for secret-bearing structs and `architecture/wyrd-security-posture.md` “Cryptography and secret handling,” which forbids accepting credentials through command arguments. A supplied credential can be retained in shell history or observed in the process argument vector, and the derived `Debug` representation can print it if these argument structures are logged or included in a diagnostic. Reuse the existing ambient `ClientConfig` credential resolution for tenant commands and an environment-only `WYRD_PLATFORM_CREDENTIAL` resolution for platform commands, carry the resolved value as `SecretString`, and ensure every containing argument/command type has redacted rather than derived secret-bearing `Debug`; prove both command families authenticate from their documented environment variables and reject a missing credential without accepting a secret-valued CLI argument.

- **`REPO-R8-2` — a new function-scoped import violates the repository's module dependency rule.** `crates/vala/vala-sql/tests/pg_migration.rs:134` imports `sha2::Digest` inside `shipped_audit_staging_migration_is_immutable`. `architecture/agent-rules.md` requires every `use` statement at module scope; the test-module exception permits imports at the top of `mod pg_tests`, not inside an individual test, and the narrow local `Trait as _` generic-function exception does not apply. Move `sha2::Digest` into the existing import block at `pg_migration.rs:7-8`; the existing migration checksum test is sufficient closure proof.

### Suggestions

None; optional cleanup and untouched pre-existing drift are excluded.

## Open Questions

None. Both findings are resolved by current repository authority and require no new product or architecture decision.

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Cumulative base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `eb9b2f69cb883fa508ed168f21cb868451e61b82`
- Candidate was unchanged at report completion.

## Authority Coverage

| Changed surface | Applicable authority read | Coverage/result |
|---|---|---|
| Active specification, tasks, remediation/review records, and verification evidence | `AGENTS.md` §§14, 16; `architecture/agent-rules.md`; `architecture/references/README.md`; `languages/spec-driven-development.md`; `languages/implementation-execution.md` | Tracked packet, approved revision, immutable candidate, and evidence mapping are present; PASS. |
| Tenant JWT issuance and verification, OIDC federation, refresh, delegation, API-key exchange, principal/credential lifecycle | `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; `architecture/v1/00-foundations/security.md`; `service-identity.md`; `permission-check.md`; `AGENTS.md` §§2, 4-6, 9 | One concrete issuance owner and local verifier use typed claims and no request-time database/cache path; PASS. |
| Platform authentication, current-state authorization, provisioning, recovery, and operator SQL | `architecture/wyrd-security-posture.md`; `architecture/agent-rules.md`; `architecture/references/architecture/patterns.md`; `AGENTS.md` §§2-6, 9 | Platform state is revalidated through `OperatorPool`; audited effects share the existing transaction boundary; tenant/platform planes remain disjoint; PASS. |
| Tenant and Vala SQL, migrations, RLS, audit staging, publication, and concurrency | `architecture/agent-rules.md`; `architecture/bifrost-design.md`; `architecture/references/domain/vala-architecture.md`; `domain/olap-serving.md`; `domain/analytical-operations-reliability.md`; operations deployment/reliability/runbook authorities | Tenant SQL remains behind `TenantConn`, platform/cross-tenant access begins at `OperatorPool`, audit uses the canonical staging/publisher path, and publication contention is bounded per tenant; FAIL only `REPO-R8-2` for import placement in a migration test. |
| HTTP routes, stable errors, served OpenAPI, and local storage transfer | `AGENTS.md` §9; `architecture/references/architecture/patterns.md`; `languages/errors.md`; `languages/testing-workflows.md` | Typed co-registered routes use the canonical problem mapper; served-document and malformed-extractor coverage are recorded; PASS. |
| MCP principal tools and shared wire contracts | `AGENTS.md` §§2-3, 9; `architecture/wyrd-doctrine.mdx`; `languages/agent-harness.md`; `languages/errors.md` | Shared typed DTOs, scope-gated writes, UUID schema validation, and runtime MCP proof are present; PASS. |
| Shared Rust client, Rust SDK, CLI projections, and credential handling | `AGENTS.md` §§2-6, 9; `architecture/wyrd-security-posture.md`; `architecture/references/languages/rust-core.md`; `languages/agent-harness.md`; `languages/errors.md` | Client/server ownership and shared transport projection conform; FAIL `REPO-R8-1` for new plaintext command-argument credentials. |
| Bifrost Gate/Oracle authorization and stream admission | `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `domain/olap-serving.md`; `domain/analytical-operations-reliability.md` | JWT verification is local at admission, stable-table permissions remain service-owned, and admitted work remains deadline-bounded; PASS. |
| Rust structure, async boundaries, dependencies, manifests, documentation, and generated contracts | `AGENTS.md` §§4-6, 11-12, 16; `architecture/agent-rules.md`; `languages/rust-core.md`; `languages/testing-workflows.md` | Concrete owners, IO-earned async, dependency direction, codegen evidence, and branch-introduced rustdoc repairs conform; FAIL only the explicit import-location rule in `REPO-R8-2`. |
| Repository docs, examples, and operational guidance | `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; applicable `architecture/operations/*`; `AGENTS.md` §§11-12 | Auth flow and runtime OpenAPI guidance match the implementation; reported docs/examples lanes pass; PASS. |

## Rule-by-Rule Results

| Repository rule | Exact source evidence | Result |
|---|---|---|
| Tenant access tokens are five-minute self-contained permission snapshots verified locally | `crates/wyrd/wyrd-auth/src/issuance.rs` (`TenantTokenIssuer`); `crates/shared/wyrd-auth-verify/src/lib.rs:211-352` (`TokenVerifier`) | PASS |
| Platform requests revalidate current credential, principal, and grant state through the operator boundary | `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs`; `crates/wyrd/wyrd-auth/src/platform_authz.rs:100-194` | PASS |
| Tenant/cross-tenant SQL uses only the approved capabilities and preserves caller-owned transaction lifecycle | `crates/wyrd/wyrd-sql/src/operator_pool.rs:28-47`; changed query functions accept `TenantConn`/`OperatorPool`; reported tenant/pool checks | PASS |
| Every evaluated permission is recorded through canonical staging before allow/refuse | `crates/wyrd/wyrd-auth/src/platform_authz.rs:168-193`; tenant route owners and `PostgresGateAudit`; audit publication uses `vala.audit_staging` | PASS |
| Audit publication is bounded and one tenant's lock does not stall the sweep | `crates/vala/vala-sql/src/queries/audit_staging.rs:199-246` uses `FOR UPDATE NOWAIT`; focused publication journey recorded | PASS |
| Public errors use the derive-backed catalog and single problem projection | local transfer extractors and `crates/wyrd/wyrd-server/src/http/error.rs`; served-route contract evidence | PASS |
| Agent-facing routes and MCP schemas are typed and runtime-tested | `crates/wyrd/wyrd-server/src/mcp/principals.rs`; `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/principals.rs`; `pg_openapi_contract.rs` | PASS |
| Secrets use redacted types/debug and credentials never enter command arguments | `PlatformEndpoint` and `TenantEndpoint` locations in `REPO-R8-1` | **FAIL** |
| `use` statements stay at module scope | function-scoped `sha2::Digest` at `pg_migration.rs:134` | **FAIL** |
| Changed Rust follows concrete-owner, synchronous-default, and rustdoc rules | `TenantTokenIssuer`, `TokenVerifier`, `PlatformAuthorization`, client handles, and reported strict rustdoc attribution | PASS |
| No gate was weakened or bypassed | Added platform capability scan strengthens the tenant-isolation gate; clippy allowances retain documented justification; generated OpenAPI snapshot is replaced by served-router proof | PASS |
| Generated JSON/schema artifacts derive from source and remain drift-free | reported `codegen:check`; source/golden pairs in the cumulative diff | PASS |
| User-facing principal/platform/MCP/Bifrost behavior has journey coverage | recorded platform ×2, identity, CLI, MCP, and Bifrost server journeys with nonzero selectors | PASS |

## Verification Notes

The appended evidence reports passing format, workspace lints, client-tier, unwrap/clippy-allow/tenant-isolation/from-pools checks, principal/shared/SQL suites, platform/identity/CLI/MCP/Bifrost journeys, codegen, docs, strict rustdoc attribution, and cumulative whitespace checks on the code candidate `a9706766`; candidate `eb9b2f69` adds only the approved specification commit.

I independently re-ran `scripts/check_tenant_isolation.py` under `python3`, the from-pools and client-tier shell checks, `scripts/check_clippy_allow.py` under `python3`, and `git diff --check`; they passed. The canonical `mise run check:tenant-isolation` could not start because this host has no `python` executable, matching the recorded environment limitation. The client-tier shell emitted its existing missing-`python/` `rg` warning and Cargo profile warnings but exited successfully.

No broad test lane was rerun for this standards pass; the candidate's final commit changes only the approved spec, and the implementation evidence supplies the code-candidate lane results. Neither green compilation nor the recorded checks enforce the two failed source-shape/security rules above.

## Overall Result

**FAIL** — the cumulative candidate retains one security-authority violation in its new CLI credential surfaces and one explicit Rust import-placement violation.
