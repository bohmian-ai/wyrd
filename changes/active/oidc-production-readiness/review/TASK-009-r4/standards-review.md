# TASK-009 round-4 repository standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `0bd3686e8bb763b07376f84661946aadc6200bfb`
- Candidate tree: `86fb8e11681b4d2e60bba2aa7ee3bf2c4f134153`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Current remediation: `changes/active/oidc-production-readiness/review/TASK-009-r3/TASK-009-R3-platform-login-boundary-and-contract.md`
- Binding withdrawals: `FIND-TASK-009-5`, `FIND-TASK-009-14`, and `FIND-TASK-009-15`; none is reopened directly or indirectly.

The candidate object and tree remained the stated immutable subject throughout
this review. `.codegraph/` is absent, so navigation used commit-qualified Git
diffs, repository search, and direct source/caller inspection. This report
reviews repository-rule compliance only, not task acceptance or another
reviewer's conclusions.

## Authority coverage

| Changed surface | Applicable authority | Coverage |
|---|---|---|
| Workspace dependency, lockfile, and generated Hakari feature union | `AGENTS.md` §§1, 4, 11–12, 15–16; `architecture/agent-rules.md` Cargo-feature rule; `architecture/references/languages/testing-workflows.md`; TASK-009's approved `openidconnect = 4.0.1` decision | Reviewed workspace and crate manifests, `Cargo.lock`, generated `workspace-hack`, dependency placement, and the resolved feature graph. |
| `wyrd-auth-oidc` relying party, screened transport, typed provider metadata, Moka cache, token verification, and crate-local errors | `AGENTS.md` §§3–6, 9–10, 12, 16; `architecture/agent-rules.md` SSRF, async, owner, error, and rustdoc rules; `architecture/wyrd-security-posture.md` federation, credential, and SSRF boundaries; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md` | Reviewed the complete production module, exports, all production callers, verification configuration, cache paths, transport routing, error projection, and in-module tests. |
| Tenant human login, callback, connection testing, role mapping, issuance, and audit | `AGENTS.md` §§3, 5–6, 9–12, 16; `architecture/agent-rules.md` tenancy, audit, SQL, SSRF, async, and documentation rules; `architecture/wyrd-design.md` Runtime identity; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md` | Reviewed `HumanConnections`, login/callback owners, connection lifecycle, SQL capabilities, server handlers, error mapping, and relevant tests. |
| Platform login, process composition, configuration, begin/callback routes, and OpenAPI declaration | Same server/security/identity authorities; `architecture/wyrd-design.md` Runtime identity; `architecture/references/languages/errors.md`; `architecture/references/languages/testing-workflows.md` | Reviewed `PlatformLogin`, `ServerAuth`, boot construction, configuration and login handlers, full discovery before persistence, served `503` declaration, and exact contract tests. |
| Workload issuer boot and administration, plus `wyrd-auth-verify` | `AGENTS.md` §§3–6, 9–12, 16; `architecture/agent-rules.md` SQL, SSRF, async, and documentation rules; approved non-goal retaining RFC 7523 on `ExternalVerifier` | Confirmed workload setup remains metadata-only, workload verification remains on `ExternalVerifier`, and human-verifier prose and fixture wiring are absent. |
| Platform wire contract, durable migration, and SQL query layer | `AGENTS.md` §§3–6, 9, 11–12, 15–16; `architecture/agent-rules.md` SQL capability and transaction rules; `architecture/references/architecture/patterns.md` storage/server patterns | Reviewed request/view/row parity, audience derivation from `client_id`, migration and query callers, actual production field/signature types, and SQL tests. The direct unreleased-column removal remains binding human direction under withdrawn `FIND-TASK-009-14`. |
| Rust unit tests, Postgres integration tests, real-server journeys, fixtures, and nextest configuration | `AGENTS.md` §11 and §16; `architecture/agent-rules.md` test placement, exact selectors, managed environment, and host-load rules; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/implementation-execution.md` | Reviewed changed tests and helpers, their documentation, exact-selector evidence, repository-managed Postgres setup, and reuse of the native `postgres-fixtures` nextest group. |
| Public/generated surfaces and workflow records | `AGENTS.md` §§8–9, 11–12, 14–16; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/errors.md`; binding narrowest-task-lane direction | Reviewed the Rust wire owner, served OpenAPI route, task/remediation evidence, and all four changed canonical/mirror skill pairs. No Python, TypeScript, UI, stub, declaration, or schema-golden source is changed in the cumulative range. |

`architecture/bifrost-design.md` and the analytical-domain references do not
apply because the candidate changes no Bifrost ingest, query, storage,
compaction, or maintenance behavior.

## Applicable rule results

| Rule | Source evidence | Result |
|---|---|---|
| Use the approved vetted relying-party library in its narrow owner | `Cargo.toml` pins `openidconnect = "=4.0.1"`; only `wyrd-auth-oidc` consumes it; `RelyingParty` owns discovery, library state/nonce/S256 generation, code redemption, and human ID-token verification. | PASS |
| Do not enable `oauth2`'s `reqwest` feature or add a second relying-party transport | `openidconnect` has `default-features = false`; `cargo tree --locked -e features -i oauth2` shows the sole edge through `openidconnect` and no enabled `oauth2` feature. All relying-party HTTP uses `ScreenedHttp`. | PASS |
| Screen, resolve, pin, bound, and refuse redirects for every provider request | `ScreenedHttp::send` routes discovery, JWKS, and token calls through the existing screened, DNS-pinned, proxy-free, bounded, redirect-disabled client. The cumulative tests cover blocked destinations and an unreached redirect target. | PASS |
| Keep stateful runtime behavior and caches on concrete process-owned services | `HumanConnections` owns the tenant relying party; one boot-built `PlatformLogin` owns the platform relying party shared by configure, begin, and callback. No second cache, invalidation service, or transport was added. The placement-only accessor concern is withdrawn by binding lead direction and cannot be reopened as a standards finding. | PASS |
| Tenant SQL uses `TenantConn`; cross-tenant platform work uses `OperatorPool`; callees do not end caller transactions | Production signature inspection found no raw `PgPool`, naked connection, or caller-supplied transaction in the changed auth/server/query paths. Platform reads use `OperatorPool`; audited writes use the authorization-owned `TenantConn`; changed callees do not commit or roll back it. | PASS |
| Preserve server ownership, platform/tenant separation, and canonical audit | Tenant selection remains server-state-derived; platform login is tenantless and cannot fall back to tenant trust; existing authorization decisions and transactional audit owners remain in place. No client-side durable behavior, second identity store, or audit writer was added. | PASS |
| Keep secrets out of public contracts and diagnostic formatting | Client secrets use `SecretBearer`/`SecretString`, stored secrets remain sealed, `PlatformLogin` and `RelyingParty` debug implementations omit sensitive state, and codes, PKCE verifiers, tokens, and sessions do not enter responses or traces. Conventional provider error diagnostics remain allowed by withdrawn `FIND-TASK-009-5`. | PASS |
| Public errors use the stable derive-backed catalog; local errors remain crate-local | `RelyingPartyError` and `ProviderHttpError` remain `thiserror` crate-local types. Server boundaries map them to existing `WyrdError` variants. Platform configuration now declares and proves the reachable existing `503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`; no parallel code or response shape was added. | PASS |
| Public request/response contracts are typed and generated declarations match runtime behavior | Platform request/view/callback types remain in `wyrd-spec` with schema derives. `configure_connection` declares its typed body and stable responses, including the corrected `503`; the served OpenAPI test inspects that assembled route. | PASS |
| Async is restricted to real IO or intentional composition | Changed async methods await HTTP, cache, SQL, or handler workflows. Issuer checks, claim checks, URL conversion, error classification, and mapping remain synchronous. | PASS |
| New and materially modified Rust items, fields, variants, helpers, and tests have substantive rustdoc | Reviewed production owners and materially changed helpers/tests across the cumulative range. Error and panic sections are present where required; stale human-verifier ownership prose and the displaced JWKS-helper comment from earlier rounds are corrected. | PASS |
| Tests live at the owning tier and use repository-managed dependencies | Pure relying-party behavior remains in Rust unit tests; SQL and assembled-server behavior use existing Postgres fixtures and `WyrdTestServer`. The nextest override reuses the established `postgres-fixtures` group rather than adding a custom harness, sleep, retry, or synthetic load. | PASS |
| Verification is narrowest-lane for a task review, with exact selectors for named tests | The remediation record identifies the exact platform journey and served OpenAPI tests. Current repository-wrapped executions selected and passed one test in each target after migrations. `fmt` and `lints` are recorded green on the final candidate; broader `test:wyrd` and principals evidence exists for the same behavioral change before the placement-only revert. Full journeys remain change-review work by binding direction. | PASS |
| Generated and mirrored artifacts remain owner-controlled | The Hakari manifest changes match the dependency feature union; all four changed `.agents` skill files are byte-identical to their `.claude` mirrors. No generated language artifact was hand-edited. | PASS |
| No provider branch, compatibility alias, speculative setting, permanent check, or nonstandard mechanism entered the change | The cumulative implementation uses the selected library, existing `ScreenedHttp`, existing Moka cache, existing stable errors, typed SQL capabilities, existing nextest group, and existing tests. No custom protocol, probe, retry system, audience option, second cache, migration preflight, overlap column, dual write, or provider-specific path was added or required. | PASS |

## Material findings

None.

## Prior standards-finding closure

| Prior standards issue | Current source evidence | Result |
|---|---|---|
| Process lifetime and cache ownership | `ServerAuth::platform_login` is built once and `login_service` returns that owner to configuration, begin, and callback; `RelyingParty::cached` uses Moka `try_get_with`. | CLOSED |
| Inaccurate or missing changed-helper rustdoc | `connections.rs` keeps distinct documentation on JWKS usability and stable test-reason helpers; workload verifier and auth-state docs describe workload ownership only. | CLOSED |
| Missing exact named-test evidence | The remediation records exact selectors; current repository-wrapped runs selected and passed the two round-three changed tests. | CLOSED |
| Platform discovery `503` omitted from public contract | `components/platform/identity.rs:173-205` declares the stable `503`; `pg_openapi_contract.rs:108-176` checks the served declaration; `platform_admin_e2e.rs:1615-1642` pins runtime status and code. | CLOSED |

`FIND-TASK-009-5`, `FIND-TASK-009-14`, and `FIND-TASK-009-15` remain
**WITHDRAWN — MUST NOT REOPEN**. No equivalent redaction mechanism, schema
overlap mechanism, or placement-only owner change is required here.

## Verification assessment

Current repository-wrapped exact executions passed
`platform_admin_e2e::federated_platform_sign_in_runs_through_the_served_callback`
and `pg_openapi_contract::the_served_document_describes_the_composed_surface`,
one selected test each, after the repository-managed migrations completed.
Direct invocations without the repository wrapper lacked
`WYRD_TEST_DATABASE_ADMIN_URL`; that environment omission is not a product or
test failure and the required wrapped executions supersede them.

The remediation record also reports final-candidate `mise run fmt` and
`mise run lints` passing. The same `FIND-TASK-009-16` implementation passed
`mise run test:wyrd` and `mise run test:principals:integration` before the
subsequent commit reverted only the withdrawn, behavior-free
`FIND-TASK-009-15` placement edit. Static review additionally confirmed a clean
cumulative `git diff --check`, synchronized skill mirrors, no raw SQL-pool
propagation in the changed production paths, and no Python/TypeScript/UI or
generated-language diff.

Under the binding narrowest-lane direction, no full identity journey or
every-language sweep is required for task review. Those run once at change
review and are not recorded as a verification limit or standards failure here.

## Overall result

**PASS**

Repository authority coverage is complete, the round-three public-contract
gap is source-closed and directly proven, all prior standards issues are
closed or bindingly withdrawn, and no material repository-rule violation
remains.
