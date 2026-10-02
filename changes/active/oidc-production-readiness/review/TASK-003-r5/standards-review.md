# TASK-003 R5 repository standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority: the supplied R2, R3, and explicitly human-authorized R4 remediation tasks
- Human directions: `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and `TASK-003-r2/human-direction-connection-test.md`

The candidate resolved to the stated commit before and after inspection. This
repository has no `.codegraph/` directory, so navigation used the immutable Git
range, repository search, and direct source inspection.

## Authority coverage

| Changed surface | Applicable authority read and applied | Result and source evidence |
|---|---|---|
| Change packet, approved specification, task, remediation rounds, and human directions | `AGENTS.md` §§1, 2, 11, 12, 14, 16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md` | **PASS.** The cumulative range retains the approved revision-7 specification, original task, prior review records, all supplied remediation tasks, and both human directions. R3 and R4 are explicitly authorized inputs rather than implementation-created product authority. |
| OIDC discovery and callback contracts in `wyrd-auth-oidc` and `wyrd-spec` | `AGENTS.md` §§2–6, 9, 10, 12, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md` | **PASS.** Provider IO remains in `wyrd-auth-oidc`; `wyrd-spec` remains synchronous, IO-free, and PyO3-free. The RFC 9207 discovery field and parser have substantive documentation and `# Errors` (`provider.rs:18-68`). `CallbackQuery.iss` remains a typed optional contract, and the two callback schema snapshots are byte-identical. |
| Human-connection management, connection testing, authorization-code callback, token issuance/refresh, API-key exchange, sealing, and browser-session lifecycle | `AGENTS.md` §§3–6, 9, 10, 12, 15, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` runtime identity; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md`; `languages/maintainer-style.md` | **PASS.** Stateful workflows are owned by concrete dependency-bearing types, including `HumanConnections`, `AuthorizationCodeExchange`, `TenantTokenIssuer`, `RefreshTokens`, `ExchangeApiKey`, `SealedSecretRewrap`, and `BrowserSessions`. Async methods await SQL/provider/issuance IO; pure classification and conversion remain synchronous. Secret-bearing values use `SecretString` or sealed envelopes, and `BrowserSessions::Debug` discloses no fields (`browser_sessions.rs:114-145`). |
| R4 renewal contract correction | `AGENTS.md` §§4–6, 12, 16; `architecture/agent-rules.md` rustdoc and gate rules; `architecture/references/languages/rust-core.md` visibility rule; `languages/maintainer-style.md` documentation/testing guidance; R4 remediation authority | **PASS.** `IssuanceError::is_refusal`, `RefreshError::is_refusal`, and `ExchangeError::is_refusal` are now `pub(crate)` (`issuance.rs:285-299`, `refresh.rs:55-70`, `exchange_api_key.rs:88-107`), and repository-wide caller search finds no external consumer. `open_text` now assigns lifecycle policy to callers and documents its raw error (`browser_sessions.rs:753-768`); the classification prose distinguishes producer refusal from envelope-open failure (`browser_sessions.rs:872-880`). The focused unit test directly proves missing and unopenable envelopes map to retryable `Renewal::Failed(Internal)` (`browser_sessions.rs:848-870`). |
| Private Axum BFF channel, auth state, boot/configuration, callback handler, and router | `AGENTS.md` §§5, 6, 9, 12, 15, 16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `languages/errors.md` | **PASS.** The private router is mounted as one typed channel, protected before handler execution by configured service-key hashes, and excluded from public OpenAPI (`bff.rs:1-10,36-88`). Typed handlers use structured `WyrdErrorResponse`, scrubbed tracing, server-derived tenant/session state, and expose access authority only to the server-side BFF (`bff.rs:181-327`). |
| Postgres migrations, tenant connection binding, login state, human connections, browser-session storage, directory lookup, and sealing inventory | `AGENTS.md` §§3, 6, 9, 15, 16; `architecture/agent-rules.md` SQL capability and transaction rules; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md` | **PASS.** Production tenant query functions take `&mut TenantConn<'_>` and do not commit or roll back caller transactions; cross-tenant directory/inventory work uses `OperatorPool`. No changed production workflow signature or field propagates `PgPool`, a naked connection, or a caller-owned SQL transaction. Browser rows enforce forced RLS and mode/credential integrity, and the two narrow `SECURITY DEFINER` tenant locators revoke public execution and grant only `wyrd_app` (`20261001000001_auth_browser_sessions.sql:19-103`). Connection-test state makes exactly one initiation binding representable and forbids completions (`20261001000002_auth_connection_test_state.sql:10-22`). |
| SvelteKit hooks, server-only session boundary, upstream origin, routes/actions, settings, chooser, and components | `AGENTS.md` §§2, 9, 11, 12, 16; `architecture/wyrd-security-posture.md`; `architecture/references/languages/typescript-guide.md`; applicable UI architecture/testing authority | **PASS.** Credentials remain in server-only code; cookies are Secure, HttpOnly, SameSite=Lax; actions enforce POST, exact origin, expiry, and constant-time CSRF comparison (`server-sessions.ts:134-153,233-245`). Tenant cookie names remain hints and are resolved sequentially through server state, bounding authenticated work to one request at a time (`server-sessions.ts:283-309`). An absent upstream setting alone receives the loopback default; an explicit empty or unsafe value is rejected (`upstream.ts:7-26`). Exported operations have explicit return types and UI state projects server-owned permissions rather than mapping roles locally. |
| Rust unit/Postgres tests, TypeScript/component tests, production browser journey, provider fixtures, and `mise.toml` routing | `AGENTS.md` §11; `architecture/agent-rules.md` test placement and exact-command rules; `architecture/references/languages/testing-workflows.md`; `languages/spec-driven-development.md` | **PASS.** Fast pure checks remain inline; Postgres cases live in `pg_tests`; external Rust tests earn separate binaries by starting real Postgres/server/provider/BFF processes. The production journey starts two built BFF replicas and a trusted TLS hop (`identity_ui_e2e.rs:1-22,452-508`), while the HTTP Vitest journey drives Keycloak and Dex without mocking the BFF/server (`production-auth.integration.test.ts:1-37`). Ignored identity tests are registered in the explicit `test:identity:journey` lane rather than silently skipped. |
| Public docs, security authority, generated schemas/docs, and permanent-code hygiene | `AGENTS.md` §§1, 2, 9, 11, 12, 16; `architecture/agent-rules.md` generated-artifact rule; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; `languages/errors.md` | **PASS.** Lasting issuer-binding and deployment behavior are recorded in the owning security/docs authority. Source and callback schema snapshots agree, and recorded `codegen:check`/`docs:check` evidence covers generated parity. Production source contains no task IDs, agent notes, legacy compatibility aliases, or commercial-edition scaffolding. |
| Dependencies, Cargo features, build configuration, and boundary gates | `AGENTS.md` §§1, 3, 4, 11, 12; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md` | **PASS.** The cumulative range adds no dependency, Cargo feature, per-crate profile, client-tier database dependency, or source-tree build generation. `mise.toml` extends the existing identity journey and SQL coverage rather than adding a competing gate or an all-features test lane. |

## Applicable rule results

| Repository rule | Evidence | Result |
|---|---|---|
| Stateful Rust workflows have a cohesive concrete owner; no single-implementation trait or zero-state utility owner is introduced. | `BrowserSessions` owns store/keyring/issuer/verifier/connections (`browser_sessions.rs:114-165`); `BffChannel` owns session and admission state (`bff.rs:36-51`). | PASS |
| Async is limited to real IO or intentional composition of IO. | Browser-session public/private async methods await SQL, issuance, or provider operations; `is_refusal`, identifier validation, sealing conversions, and response-issuer comparison remain synchronous. | PASS |
| Rust visibility is no wider than real callers require. | All three renewal classifiers are `pub(crate)` and every call remains within `wyrd-auth`. | PASS |
| Every new or materially modified Rust item, including private helpers/tests, has substantive rustdoc; fallible operations document errors and test panics are stated where applicable. | Direct inspection of changed auth, server, SQL, testing, and latest R4 items; corrected sites include `provider.rs:18-68`, `bff.rs:168-178`, `browser_sessions.rs:753-800,848-880`, and the shared API-key test support. | PASS |
| Public/cross-boundary errors use typed Wyrd errors and do not expose provider, SQL, or cryptographic internals. | BFF handlers return `WyrdErrorResponse`; browser-session helpers map store/sealing failures to stable `WyrdError` variants with fixed messages (`browser_sessions.rs:743-833`). | PASS |
| Secrets are typed, sealed at rest, redacted from diagnostics, and kept out of browser page data. | `SecretString`, `SealingKeyring`, sealed SQL columns, redacted `Debug`, one-time internal session creation, and server-only authority projection (`browser_sessions.rs:58-145`; `bff.rs:157-178,282-312`; migration lines 8-16). | PASS |
| Tenant SQL uses `TenantConn`; cross-tenant work uses `OperatorPool`; tenant query callees do not own commit/rollback. | Production signature search and full auth-query inspection found only the allowed capabilities; `WyrdPostgres::tenant_conn` is the owning acquisition boundary (`postgres.rs:166-175`). | PASS |
| Tenant authority derives from verified credentials or server-bound state, not paths, hosts, headers, or cookie names. | Session tenant lookup starts from the hashed opaque id, then opens that tenant's RLS transaction (`browser_sessions.rs:470-488`); BFF rechecks returned tenant keys before rendering (`server-sessions.ts:186-216`). | PASS |
| Authorization/audit writes retain their owning transaction and failures fail closed. | Human-connection testing and browser renewal use caller-owned tenant transactions; replay containment is the only renewal outcome committed before serving the still-live issued token, while ordinary refusal rolls back and internal failure returns without commit (`browser_sessions.rs:503-528`). | PASS |
| TypeScript uses typed server-only boundaries, validates untrusted values, bounds concurrent work, and applies nullish defaults. | Explicit request/reply/session types, tenant-key validation, timeouts, sequential hint resolution, and `??` configuration fallback (`server-sessions.ts:9-108,283-347`; `upstream.ts:16-26`). | PASS |
| Public capability proof includes real user journeys; fast unit lanes remain dependency-free. | Production-built two-replica BFF/provider/TLS journey plus focused Rust/TypeScript/Postgres checks; `mise.toml:576-610` owns the ignored provider journey. | PASS |
| Generated artifacts are generator-owned and drift-checked. | Callback schema snapshots are byte-identical; recorded `mise run codegen:check` and `mise run docs:check` passed. | PASS |
| No gate is weakened or bypassed. | Cumulative diff adds no production `#[allow]`, no unregistered `#[ignore]`, no deleted safety assertion, and no broadened boundary allowlist. The identity ignores name their owning lane. | PASS |

## Material findings

None.

The prior standards defects are source-closed: provider and helper rustdoc is
present, cookie-hint verification is sequential, explicit empty upstream
configuration is refused, and the renewal classifiers are crate-private. The
latest R4 unit test directly pins the previously undocumented retryable
envelope-open boundary without adding a public API, dependency, fixture layer,
or alternate lifecycle owner.

## Verification notes

Read-only checks performed here confirmed:

- `git diff --check 63c5bffc93cd2f7b5ed558e610a213efcc34fd49..989d0734b0a9b04f314ef4b52aa7d8510f26fe11` is clean;
- `crates/wyrd-spec/schemas/auth_callback_query.json` and its test snapshot are byte-identical;
- production auth/server/query signatures do not introduce a raw SQL pool or caller-owned transaction; and
- repository-wide calls to the three renewal classifiers remain inside `wyrd-auth`.

The supplied implementation record reports green results for the new exact
unit selector, the existing classification selector, four focused
Postgres-backed renewal selectors, `mise run fmt`, `mise run lints`,
`mise run check:tenant-isolation`, `mise run test:wyrd`,
`mise run test:identity:journey`, and `git diff --check`. Earlier cumulative
evidence also records the owning SQL, UI, codegen, and documentation lanes.
This reviewer did not rerun Cargo, provider, browser, or database commands in
the shared checkout; source, lane definitions, and recorded results were
inspected directly.

## Overall result

**PASS** — all changed Rust, SQL, SvelteKit/TypeScript, migration, generated,
documentation, fixture, and verification surfaces covered by the cumulative
base-to-candidate range conform to the applicable repository authorities. No
material repository-standards finding remains.
