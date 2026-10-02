# TASK-003 round-6 repository standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
- Latest remediation locator: `989d0734b0a9b04f314ef4b52aa7d8510f26fe11..ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority: TASK-003 R2, explicitly authorized R3 and R4, and explicitly authorized R5
- Human directions: R1 FIND-1, R2 connection testing, and R5 FIND-18. The R5 direction replaces FIND-18's earlier principal-family correction with session-chain-only revocation.

The candidate resolved to the stated commit before and after this review. The repository has no `.codegraph/` directory, so source navigation used the immutable Git ranges, repository search, and direct source inspection.

## Authority coverage

| Changed surface | Applicable authority | Coverage and result |
|---|---|---|
| Pure auth contracts, callback query, stable errors, and generated schema snapshots | `AGENTS.md` §§2-4, 8-9, 11-12, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `languages/errors.md` | **PASS.** Contract types remain synchronous and IO/PyO3-free, cross-boundary errors remain catalog-backed, and the source and test callback schema snapshots are byte-identical. No R5 change adds a public contract or generated artifact. |
| OIDC provider exchange, human connection management, login callback, token issuance/refresh, sealing, and browser-session lifecycle | `AGENTS.md` §§3-7, 9, 12, 15-16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/v1/00-foundations/security.md`; `architecture/references/languages/rust-core.md`; approved spec revision 7 | **PASS.** `BrowserSessions` remains the dependency-owning lifecycle owner. R5 stores the server-derived initial refresh-row ID during completion (`browser_sessions.rs:235-260`) and logout uses the locked row, the existing family advisory lock, and the SQL chain revoker in one caller-owned transaction (`browser_sessions.rs:404-447`). Secrets remain sealed and logout no longer opens the refresh envelope. |
| Private Axum BFF channel, callback, auth state, routing, configuration, and boot | `AGENTS.md` §§5-6, 9, 12, 16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `languages/errors.md` | **PASS.** The cumulative private channel remains typed, authenticated before handler execution, excluded from public OpenAPI, and server-derived for tenant/session authority. R5 changes only two test fixture initializers in boot to supply the new mode-consistent `refresh_chain_id: None`; it adds no route or alternate authority. |
| Postgres schema, auth queries, RLS capabilities, refresh-chain concurrency, and transaction lifecycle | `AGENTS.md` §§3, 5-6, 9, 12, 16; `architecture/agent-rules.md`; `architecture/v1/00-foundations/postgres-layout.md`; `architecture/operations/deployment-and-release.md`; `architecture/references/architecture/patterns.md`; `languages/rust-core.md` | **PASS.** The new column is non-secret, mode-constrained, and tenant-qualified by a composite foreign key (`20261001000003_auth_browser_session_refresh_chain.sql:1-14`). Production auth queries take `&mut TenantConn<'_>` and neither commit nor roll back it. `revoke_refresh_chain` runs under RLS and follows the existing `rotated_from` authority (`refresh_tokens.rs:173-208`); its caller first takes the existing principal-family advisory lock. No changed production signature or field propagates `PgPool`, `Pool<Postgres>`, a naked connection, or a caller-owned transaction. The browser-session table and this additive constraint are introduced in the same cumulative unreleased task range, so there is no supported old browser-session writer requiring an expand-and-contract overlap representation. |
| SvelteKit hooks, server-only session boundary, upstream calls, pages/actions, settings, chooser, and components | `AGENTS.md` §§2, 9, 11-12, 16; `architecture/wyrd-security-posture.md`; `architecture/references/languages/typescript-guide.md`; applicable SvelteKit/UI repository patterns | **PASS.** The cumulative UI keeps credentials in server-only code, uses Secure/HttpOnly/SameSite cookies, validates origin/CSRF/tenant bindings, and projects server-owned permissions. Exported operations remain typed and bounded; R5 does not alter TypeScript or Svelte code. |
| Rust unit and Postgres tests, server/provider/browser journeys, fixtures, and `WyrdTestServer` shutdown | `AGENTS.md` §11 and §16; `architecture/agent-rules.md` test placement, diagnosis, and no-circumvention rules; `architecture/references/languages/testing-workflows.md`; `languages/spec-driven-development.md` | **PASS.** Pure tests remain inline and Postgres tests stay under `pg_tests`; real browser/provider journeys remain registered in `test:identity:journey`. The R5 closure test directly exercises a committed successor, unreadable envelope, same-user independent login, browser-row wipe, and replay refusal (`browser_sessions.rs:1478-1567`). The harness change has a recorded trace-based diagnosis in the R5 evidence, changes the shared `WyrdTestServer::shutdown` owner, and follows the production Bifrost abort-on-drain-failure behavior (`wyrd-testing/src/server.rs:748-787`; `wyrd-server/src/state.rs:1970-2001`). It does not add an ignore, retry, sleep, timeout relaxation, or weakened assertion to clear the failure. |
| Public documentation, security authority, fixtures, and permanent-code hygiene | `AGENTS.md` §§1-2, 9, 11-12, 16; `architecture/agent-rules.md`; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md` | **PASS.** Lasting production identity and deployment behavior remains recorded in owning docs. Production code contains no task IDs, agent notes, legacy compatibility aliases, or commercial-edition scaffolding. R5 task/history references remain confined to the active change packet. |
| Dependencies, features, build configuration, and repository verification routing | `AGENTS.md` §§1, 3-4, 11-12; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md`; `mise.toml` | **PASS.** The cumulative range adds no dependency, Cargo feature, per-crate profile, client-tier database dependency, or source-tree build generation. R5 adds no gate or allowlist. The existing identity journey, SQL, codegen, documentation, formatting, lint, and tenant-isolation lanes cover the touched surfaces. |

## Applicable rule results

| Repository rule | Exact source evidence | Result |
|---|---|---|
| Stateful Rust workflows have a cohesive concrete owner; no utility struct or single-implementation trait is introduced. | `BrowserSessions` owns store, keyring, issuer, verifier, and connection dependencies (`browser_sessions.rs:114-165`); completion and logout remain inherent methods. `WyrdTestServer::shutdown` remains on the harness owner (`wyrd-testing/src/server.rs:748-787`). | PASS |
| Async is limited to real IO or intentional composition of IO. | R5 async paths await tenant lookup, SQL, refresh locking/revocation, commit, or Bifrost shutdown. The added schema mapping and mode values remain synchronous. | PASS |
| Every new or materially modified Rust item, including private/test items, has substantive rustdoc and fallible operations document errors. | New `refresh_chain_id` fields are documented (`browser_sessions.rs` query module lines 140-142, 171-173, 203-204); `revoke_refresh_chain` documents scope, lock precondition, result, and errors (`refresh_tokens.rs:173-208`); changed completion/logout and the R5 Postgres test document workflow and failure behavior (`browser_sessions.rs:189-260,404-447,1478-1567`); changed harness shutdown documents ordering and error behavior (`server.rs:748-787`). | PASS |
| Imports stay at module scope and signatures use imported bare types. | Direct inspection of the changed auth, SQL, boot, and testing modules found no new function-local production import or fully qualified signature. Test-module imports remain within the allowed `#[cfg(test)]` scope. | PASS |
| Public/cross-boundary failures use typed Wyrd errors and do not expose SQL, cryptographic, or provider internals. | Browser completion/logout map SQL through the existing `store_error` boundary and expose stable `WyrdError` variants (`browser_sessions.rs:219-260,419-447`). No new wire error or stringly error contract was added. | PASS |
| Secret-bearing state is typed, sealed at rest, redacted, and absent from browser-visible data. | The new `refresh_chain_id` is a non-secret UUID foreign key; token bytes remain sealed. Logout derives retirement from durable non-secret identity and never opens the stored refresh envelope (`browser_sessions.rs:404-447`; migration lines 1-14). | PASS |
| Tenant SQL uses `TenantConn`; cross-tenant work uses `OperatorPool`; query callees do not own commit/rollback. | Production auth query signatures in `queries/auth/browser_sessions.rs` and `refresh_tokens.rs` accept `&mut TenantConn<'_>`. Repository search found no `commit`/`rollback` in `queries/auth`. Raw pool types remain confined to the `TenantConn` implementation/construction boundary or test-only modules, not the changed production auth/query signatures. | PASS |
| RLS remains the tenant boundary and tenant-aware references include tenant identity. | The chain column uses `(data_tenant_id, refresh_chain_id)` against `(data_tenant_id, id)` (`20261001000003...sql:8-14`); chain traversal carries no competing manual tenant selector and executes through the caller's RLS transaction (`refresh_tokens.rs:183-208`). | PASS |
| Concurrency uses the existing shared authority and one fixed lock order. | Logout holds the browser row, then takes `lock_refresh_family("user", row.principal_id)` before recursive revocation and browser wipe (`browser_sessions.rs:429-447`). The existing lock documentation fixes family-before-connection ordering (`refresh_tokens.rs:96-119`). | PASS |
| Schema invariants make mode/state mismatches unrepresentable. | The migration requires `refresh_chain_id` exactly for `oidc_refresh` mode and prohibits it for `api_key_exchange`, while the composite FK prevents a cross-tenant root (`20261001000003...sql:8-14`). Both production constructors provide the matching value (`browser_sessions.rs:246-258,314-326`). | PASS |
| Tests use the owning tier and prove user-facing behavior at journey level. | The focused R5 Postgres test lives in the existing auth `pg_tests` module and uses real issuance, rotation, SQL, logout, and replay paths (`browser_sessions.rs:1478-1567`). Cumulative identity journeys remain registered under `mise.toml`'s `test:identity:journey` task. | PASS |
| No check was weakened or bypassed. | The cumulative diff adds no production `#[allow]`, no unregistered `#[ignore]`, no removed safety assertion, and no broadened boundary allowlist. The R5 harness fix changes the lifecycle owner after a recorded trace diagnosis rather than masking the failing tests. | PASS |
| Generated artifacts remain generator-owned and drift-checked. | The callback source and test JSON schema snapshots are byte-identical; R5 changes no generated artifact. The implementation evidence records `mise run codegen:check` green. | PASS |
| No unrequested dependency, feature, compatibility path, or alternate lifecycle owner entered the change. | No manifest changed in R5; the cumulative manifest diff adds no new dependency/feature. The refresh-chain correction reuses `rotated_from`, `lock_refresh_family`, `TenantConn`, and the existing browser-session owner. | PASS |

## Material findings

None.

The R5 direction is represented as a narrow durable field and one SQL operation on the existing refresh-token authority. The test-harness shutdown correction is source-local to the shared harness owner, carries the required failure diagnosis, and retains the production abort behavior. No repository-rule defect remains in the cumulative candidate.

## Verification notes

Read-only checks performed for this review confirmed:

- `git diff --check 63c5bffc93cd2f7b5ed558e610a213efcc34fd49..ad3b92ad0f326917c731383fd3b57cb7ac6a8c82` is clean;
- `crates/wyrd-spec/schemas/auth_callback_query.json` and `crates/wyrd-spec/tests/schemas/auth_callback_query.json` are byte-identical;
- changed production auth/query signatures contain no raw SQL pool or caller-owned transaction;
- auth query callees contain no `TenantConn` commit or rollback; and
- the candidate stayed at `ad3b92ad0f326917c731383fd3b57cb7ac6a8c82` during the review.

The R5 implementation record reports green results for the exact new Postgres selector, every `browser_sessions::` test, `mise run fmt`, `mise run lints`, `mise run codegen:check`, `mise run check:tenant-isolation`, `mise run test:sql`, `mise run test:wyrd`, `mise run test:identity:journey`, and `git diff --check`. After the harness correction, `test:wyrd` ran 2,348 tests successfully, and formatting, lints, the identity journey, and diff check were rerun.

This reviewer did not rerun Cargo, Postgres, provider, browser, or documentation tasks in the shared checkout; source, task definitions, failure diagnosis, and recorded command results were inspected directly. This is a verification limit, not a missing authority or missing required report.

## Overall result

**PASS** — the cumulative Rust, SQL, SvelteKit/TypeScript, migration, generated, documentation, fixture, and verification surfaces conform to the applicable repository authorities. No material repository-standards finding remains.
