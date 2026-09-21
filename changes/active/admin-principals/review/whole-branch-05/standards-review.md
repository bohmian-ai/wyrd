# Repository Standards Review

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `c9e1092bbdb4df3781eb91b0eb33150e00df7623`
- Scope: repository-rule compliance only. Task acceptance, Ponytail-only
  simplification, optional improvements, and unrelated baseline debt are
  excluded.

## Authority Coverage

| Changed surface | Applicable authority read | Coverage evidence |
|---|---|---|
| Repository workflow, manifests, checks, and active change artifacts | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/references/README.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md`; `mise.toml`; affected `Cargo.toml` files and `Cargo.lock` | Complete base-to-candidate name/status and diff statistics were inspected; dependency/feature changes and the recorded final-candidate verification evidence were checked against the canonical task definitions. |
| Rust contracts and generated schemas | `AGENTS.md` §§4, 5, 9, 12; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md` | Inspected `wyrd-spec` auth/audit changes, schema sources and generated JSON/golden removals/updates, their server/client consumers, and the recorded `codegen:check` result. |
| Identity, credentials, OIDC, RBAC, tenancy, audit, and secrets | `architecture/wyrd-security-posture.md`; `architecture/v1/00-foundations/service-identity.md`; `architecture/v1/00-foundations/tenancy.md`; `architecture/agent-rules.md` SQL/audit/SSRF rules; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md` | Inspected the runtime/auth/auth-verify/auth-issue/auth-oidc changes, platform and tenant extractors, authorization owners, SQL query modules/migrations, canonical audit staging changes, secret-bearing types/debug implementations, and negative/injected-failure tests. |
| Server boot, HTTP, OpenAPI, routes, CLI, MCP, and shared client | `AGENTS.md` §§3, 5, 6, 9, 11; `architecture/wyrd-design.md`; `architecture/operations/deployment-and-release.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/errors.md`; `architecture/references/architecture/patterns.md` | Inspected initialization, router and handler registration, runtime OpenAPI generation/contract tests, shared `wyrd-client` handles and transport, CLI callers/journeys, MCP client/tools/catalog tests, and the thin Rust SDK re-export. The approved task explicitly excludes new Python/TypeScript administrative bindings, so their untouched runtime packages are not treated as drift. |
| Vala/Bifrost audit publication and analytical consumers | `architecture/bifrost-design.md`; `architecture/references/domain/vala-architecture.md`; `architecture/references/domain/olap-serving.md`; `architecture/references/domain/analytical-operations-reliability.md`; `architecture/agent-rules.md` canonical-audit rules | Inspected audit-event projection, staging schema/query/migration changes, table registration/fingerprint behavior, publication/integration tests, and related catalog/engine consumers. |
| Tests, fixtures, docs, scripts, and generated public material | `AGENTS.md` §§8, 11, 12; `architecture/agent-rules.md` test/generated-artifact rules; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/agent-harness.md`; docs/build tasks in `mise.toml` | Inspected new unit, Postgres integration, real-server CLI/identity/platform/MCP journeys, test-harness changes, OpenAPI/schema/docs generators, generated LLM indexes, boundary scripts, and the literal verification evidence in `TASK-001-008-R4-close-cumulative-findings.md`. |

No applicable authority was unavailable.

## Rule-by-Rule Results

| Repository rule | Result | Exact evidence |
|---|---|---|
| Candidate is immutable during review | PASS | `git rev-parse HEAD` was `c9e1092bbdb4df3781eb91b0eb33150e00df7623` at review start. Final identity is recorded below. |
| Ownership and dependency direction: contracts in `wyrd-spec`, durable behavior in server/service owners, shared clients in `wyrd-client`, no server/storage dependencies in client tier | PASS | Contract additions are under `crates/wyrd-spec/src/auth` and `src/vala`; durable SQL/server behavior remains under `wyrd-sql`, `wyrd-auth`, and `wyrd-server`; client projections are under `crates/shared/wyrd-client`; independently rerun `mise run check:client-tier` passed. |
| Cargo features and dependencies must be earned and kept in the narrowest owner | PASS | The diff removes `utoipa/yaml` and server `serde_yaml`; the new production `tokio/net` capability is confined to `wyrd-auth-oidc`'s DNS resolution, while new `wiremock`/`tracing-subscriber` uses are test-support dependencies. No new Cargo feature was introduced. |
| SQL signatures use only `TenantConn` or `OperatorPool`; a `TenantConn` callee does not commit/rollback; RLS is not duplicated by tenant predicates | PASS | Production SQL changes accept `&mut TenantConn<'_>` or `&OperatorPool`; commits shown in changed product paths remain caller-owned transaction boundaries. Independently rerun `python3 scripts/check_tenant_isolation.py` passed. Raw `PgPool` occurrences added by the diff are confined to external integration-test helpers. |
| Cross-tier imports use owning re-exports | PASS | Vala SQL consumers import the Vala-owned surfaces, while Wyrd-owned crates consume `wyrd_sql` through their owning tier. No new Vala-to-`wyrd_sql` reach-through was found. |
| Types in fields, parameters, returns, bounds, and `where` clauses are imported and written as bare names | **FAIL** | Diff-added items use qualified paths in exactly the locations summarized by `STD-R5-001`, including `screening.rs:96`, `auth.rs:118,248`, `api.rs:2735,2783`, `main.rs:137`, and `server.rs:2188,2239`. The same pattern occurs in additional changed production and test signatures/bounds. |
| All imports are at module top, except test-module imports and the documented single-function trait exception | PASS | No diff-added production function/impl-scoped `use` was found. Added nested imports are under `#[cfg(test)] mod ...`, which is the explicit exception. |
| Struct-centered Rust style and synchronous-by-default async boundary | PASS | Stateful workflows are owned by concrete handles/services (`TokenExchange`, `Platform`, `Principals`, platform authorization/session/credential services, provisioning/recovery owners). New async operations await HTTP, DNS, database, process, or server IO; pure parsing/mapping helpers remain synchronous. |
| Rustdoc on new/materially modified Rust, including `# Errors`/panic and side-effect contracts | PASS | Changed public and private workflow items carry intent and error contracts; the candidate records a diff-based all-item audit and strict `wyrd-sql` rustdoc pass. Spot checks covered the newly added OIDC screening, boot initialization, shared-client handles, platform/principal services, SQL modules, and test-harness helpers. |
| No unsafe unwrap/expect use on fallible production boundaries; Clippy allowances require an immediate specific justification | PASS | Independently rerun `python3 scripts/check_unwrap_audit.py` and `python3 scripts/check_clippy_allow.py`; both passed. The new `too_many_arguments` allowance at `wyrd-sql/src/queries/platform/identity.rs:88` has the required immediately preceding site-specific justification. |
| SSRF screening resolves, rejects any forbidden address, pins the screened addresses, and refuses redirects | PASS | `crates/shared/wyrd-auth-oidc/src/screening.rs:96-133` screens literal addresses, resolves domains once, rejects any blocked result, supplies the accepted addresses to `resolve_to_addrs`, and disables redirects. OIDC discovery, token exchange, and JWKS callers consume this owner; negative metadata/loopback/hostless tests are present. |
| Public errors use the derive-backed Wyrd catalog and do not leak internal source text | PASS | Public HTTP/CLI/MCP paths project `WyrdError`; `wyrd-server/src/http/error.rs:64-78` logs internal causes and returns stable redacted detail. Changed SQL/provider/auth failures are converted at boundaries rather than serialized raw. |
| Authentication planes, tenant isolation, credential secrecy, and revocation fail closed | PASS | Tenant and platform extractors remain distinct while using the canonical Wyrd header; verifier cache/backend-unavailable paths refuse; credential-bearing structs use secrecy/redacted debug behavior; cross-plane, revocation, suspension, replay, and injected-store-failure tests are present in the affected integration/journey targets. |
| Each authorization decision uses the one canonical audit path, in the decision/effect transaction, and failure fails closed | PASS | Platform/tenant authorization owners append to `vala.audit_staging`; platform mutations use the audited operator transaction and tenant mutations use caller-owned `TenantConn` transactions. Added failure-injection tests exercise refusal/rollback when the audit append cannot commit; audit publication remains owned by the existing publisher. |
| Generated artifacts are generator-owned; runtime OpenAPI has one authority | PASS | The YAML artifact/generator is deleted; `/openapi.json` is assembled from `utoipa` route/error declarations and checked through `pg_openapi_contract.rs`. JSON schemas/goldens track source changes, and recorded `mise run codegen:check` passed. |
| Every user/agent-facing surface has the required real client→server→client proof | PASS | Real-server platform, identity, CLI, principal, and MCP journeys are added/extended. The evidence packet records two consecutive platform runs plus identity, CLI, MCP, SQL, shared, and Bifrost lanes with nonzero selections; exact focused commands are recorded for the named regressions. |
| Tests use the correct runtime/tier, external test binaries are earned, and repository-managed setup is used | PASS | External additions exercise Postgres, real HTTP servers, compiled CLI processes, OIDC fixtures, or MCP. Pure checks remain inline. Postgres/identity commands are recorded through repository setup wrappers and exact `nextest` selectors. |
| Required verification and diff hygiene | PASS | Recorded evidence includes format, lints, client boundary, unwrap/clippy/tenant checks, scoped unit/integration/journey lanes, SQL/Bifrost lanes, codegen/examples/docs, strict rustdoc, and focused exact selectors. Independently rerun boundary/unwrap/clippy/tenant checks passed; `git diff --check base..candidate` was silent. |
| Architecture, public docs, and generated indexes agree with the shipped identity/admin model | PASS | The diff updates `wyrd-design`, doctrine, security posture, service identity, tenancy, deployment/release, authentication/authorization/self-hosting/CLI docs, and both generated LLM indexes together; recorded `docs:check` passed. |

## Material Findings

### STD-R5-001 — Qualified type paths remain in newly added Rust signatures and fields

- **Classification:** `VIOLATION` / `BLOCK_BEFORE_MERGE`
- **Violated rule:** `architecture/agent-rules.md`: "Bring types in with
  `use` and use bare names in signatures"; it explicitly applies to struct
  fields, parameters, return types, trait bounds, and `where` clauses.
- **Locations:** At minimum:
  - `crates/shared/wyrd-auth-oidc/src/screening.rs:96`
  - `crates/shared/wyrd-client/src/auth.rs:118,248`
  - `crates/shared/wyrd-client/src/eval/handle.rs:162-163`
  - `crates/wyrd-spec/src/vala/api.rs:2735,2783`
  - `crates/wyrd/wyrd-auth/src/revoke.rs:87`
  - `crates/wyrd/wyrd-cli/src/auth/trusted_issuer.rs:267`
  - `crates/wyrd/wyrd-mcp/src/client.rs:52,106,131,163,166`
  - `crates/wyrd/wyrd-server/src/boot/init.rs:186`
  - `crates/wyrd/wyrd-server/src/components/admin/routes.rs:184,806,951,961`
  - `crates/wyrd/wyrd-server/src/http/openapi.rs:30,63,99`
  - `crates/wyrd/wyrd-server/src/main.rs:137`
  - `crates/wyrd/wyrd-server/src/mcp/principals.rs:180`
  - `crates/wyrd/wyrd-sql/src/queries/auth/api_keys.rs:35-110`,
    `auth/revocation.rs:87-122`, `auth/role_assignments.rs:119-155`, and
    `platform/tenant_resolver.rs:36-59`
  - New external/test-support signatures in
    `crates/wyrd/wyrd-cli/tests/operator_journey.rs:20,31,36`,
    `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:176`,
    `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:3533-3541,3723-3734,3822-3864`,
    and `crates/wyrd/wyrd-testing/src/server.rs:2188,2239`.
- **Evidence:** These are candidate-added declarations, not merely pre-existing
  qualified expressions. Examples include
  `Result<reqwest::Client, ScreenError>`, a `reqwest::Client` field,
  `Option<uuid::Uuid>` fields/parameters, `impl std::fmt::Display`,
  `Result<wyrd_sql::OperatorPool, BootExit>`, `T:
  serde::de::DeserializeOwned`, and `&sqlx::PgPool` test parameters. Several
  affected modules already import adjacent types, confirming that importing
  these names at the top and using their bare forms is directly available.
- **Consequence:** The candidate violates an explicit repository-wide hard
  acceptance rule and leaves dependency ownership obscured at the declaration
  sites. The passing compiler, Clippy, boundary, and rustdoc lanes do not enforce
  this source-shape rule, so their success does not close it.
- **Required testable correction:** For every qualified type path introduced in
  a changed field, parameter, return type, generic bound, and `where` clause,
  add or extend the module-top `use` block and use the bare imported name. Keep
  qualified paths in expressions where they improve disambiguation; the rule
  does not prohibit those. Re-run `mise run fmt:check`, `mise run lints`, the
  affected scoped tests, and a base-to-candidate diff scan proving no newly
  introduced declaration still contains a qualified type path.

## Verification Limits

- This review did not rerun the complete expensive Postgres, identity-provider,
  Bifrost, docs, or codegen matrix. It inspected the candidate's literal
  nonzero results and commands in the supplied implementation evidence and
  independently reran the client-tier, tenant-isolation, unwrap-audit,
  clippy-allow-audit, and diff-check gates.
- Repository standards that require end-to-end behavioral judgment were checked
  against source, consumers, and the supplied focused/journey evidence; task
  acceptance remains outside this report.

## Overall Result

**FAIL**

`STD-R5-001` is a direct, material violation of a mandatory repository source
rule. No missing authority or unavailable evidence blocks the review.

Final immutable-subject check: `HEAD` must remain
`c9e1092bbdb4df3781eb91b0eb33150e00df7623`; any different value supersedes
this result with `BLOCKED`.
