# Repository standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `6aedcda5166509001db0cc851a5bc74502b4b043`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority: `TASK-003-R2-production-ui-remediation.md`, `TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`, and both supplied human-direction records
- Candidate identity was rechecked at the end of review and remained unchanged.

## Authority coverage

| Changed surface | Applicable authority | Coverage and result |
|---|---|---|
| Shared OIDC provider discovery and callback issuer binding | `AGENTS.md` §§3, 4, 6, 9, 10, 16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md` federation rules; `architecture/references/{architecture/patterns,languages/rust-core,languages/errors}.md`; human issuer direction | PASS — provider-specific wire handling remains in `wyrd-auth-oidc`; issuer-response support is typed and documented; callback issuer validation occurs after server-owned state resolution and before token-endpoint IO; no provider-specific compatibility branch was added. |
| Shared wire contracts, callback schema, and public errors | `AGENTS.md` §§2–4, 8–9, 12, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/{architecture/patterns,languages/errors,languages/spec-driven-development}.md` | PASS — contracts remain synchronous and IO/PyO3-free in `wyrd-spec`; callback `iss` is typed; source and both schema snapshots agree; public error handling continues through the derive-backed catalog rather than a parallel problem format. |
| Tenant connection management, real interactive connection test, callback completion, issuance, refresh, API-key exchange, and browser-session lifecycle | `AGENTS.md` §§3–6, 9, 12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` runtime identity; `architecture/wyrd-security-posture.md`; `architecture/references/{architecture/patterns,languages/rust-core,languages/errors}.md`; revision-7 spec and remediation/human directions | **FAIL** only for the visibility finding below. Ownership, async IO boundaries, typed errors, secret redaction, audit transactionality, exact connection binding, replay containment, and retryable internal renewal failure otherwise conform. |
| Server handlers, boot/configuration, private BFF channel, and routing | `AGENTS.md` §§3, 5–6, 9, 12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/{architecture/patterns,languages/rust-core,languages/errors}.md` | PASS — durable behavior remains server-owned; BFF routes use typed bodies and structured Wyrd errors; BFF authentication is checked before store access; production state is injected through cohesive owners rather than constructed in handlers. |
| Tenant SQL queries, migrations, login state, connection-test state, and browser sessions | `AGENTS.md` §§3–6, 9, 11–12, 15–16; `architecture/agent-rules.md` SQL rules; `architecture/references/{architecture/patterns,languages/rust-core,languages/testing-workflows}.md`; security posture tenant-isolation rules | PASS — production query signatures use `&mut TenantConn<'_>`; no changed production signature or field propagates raw `PgPool`, `Pool<Postgres>`, `PgConnection`, or caller-owned transaction types; query functions do not commit or roll back; tenant RLS and caller-owned transaction boundaries remain intact. Raw `PgPool` additions are confined to external test helpers. |
| SvelteKit BFF, hooks, routes, tenant settings, cookies, CSRF, and upstream selection | `AGENTS.md` §§2, 9, 11–12, 15–16; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; `architecture/references/{architecture/patterns,languages/typescript-guide,languages/testing-workflows,languages/errors}.md` | PASS — the BFF projects server authority; browser-visible state contains no Wyrd credential; cookies and CSRF stay server-side/session-bound; request-derived session hints are resolved sequentially; explicit empty upstream configuration reaches URL validation instead of silently falling back. |
| Rust, SQL, and TypeScript tests plus identity journey harness | `AGENTS.md` §11; `architecture/agent-rules.md` test-placement/runtime rules; `architecture/references/{languages/testing-workflows,languages/spec-driven-development,languages/maintainer-style}.md` | PASS — pure cases remain unit tests; Postgres and provider behavior stays in gated PG/identity lanes; the real UI journeys drive two built BFF replicas, real Postgres, Keycloak and Dex, including real TLS and negative flows; ignored tests name their owning repository lane rather than weakening coverage. |
| Generated schemas, docs, fixtures, and `mise` tasks | `AGENTS.md` §§1, 11–12, 15–16; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md` | PASS — source/golden callback schemas are byte-identical, generated documentation is present, and the candidate records successful codegen, docs, formatting, lint, SQL, Wyrd-family, tenant-isolation, and identity-journey lanes. No gate was disabled or broadened. |
| Python, PyO3, first-class SDK declaration surfaces, MCP, and Bifrost | Corresponding `AGENTS.md` and routed references | N/A — the cumulative diff does not change these surfaces. |

## Rule results

| Rule | Evidence | Result |
|---|---|---|
| Raw SQL pools do not propagate through production domain signatures; tenant work uses `TenantConn` and callees do not end caller transactions. | `wyrd-sql/src/queries/auth/{browser_sessions,human_connections,login_state}.rs` and `wyrd-auth/src/browser_sessions.rs` use `&mut TenantConn<'_>`; commit/rollback is owned by the orchestrating `BrowserSessions`, not SQL query functions. | PASS |
| Stateful workflows have concrete owners and async is limited to IO composition. | `BrowserSessions`, `HumanConnections`, `AuthorizationCodeExchange`, `ExchangeApiKey`, `RefreshTokens`, `TenantTokenIssuer`, `BffChannel`, and `ServerSessions` own their dependencies and workflows. Added classification helpers are synchronous. | PASS |
| Public visibility is intentional; otherwise use `pub(crate)`. | `IssuanceError::is_refusal`, `RefreshError::is_refusal`, and `ExchangeError::is_refusal` are declared `pub`, but repository-wide caller search finds uses only within `wyrd-auth`. | FAIL (`STD-TASK-003-R4-1`) |
| Materially changed Rust items, including tests/private helpers, have substantive rustdoc and fallible items document errors. | Latest remediation documents `Renewal`, `open_credential`, all added test data/helpers/tests, `exchange_api_key::pg_tests`, and `insert_live_api_key`; fallible production helpers include `# Errors`. | PASS |
| Secrets remain redacted and outside browser/page/log/error/audit surfaces. | Secret-bearing values use `SecretString`/sealed byte envelopes; `BrowserSessions::Debug` is redacted; the BFF obtains access authority only server-side. | PASS |
| Authentication/tenant selection derives from verified or server-bound state, never route, host, or browser hints. | Browser session lookup uses the hashed opaque session id; login state binds tenant/connection; returned tenant is checked against route context; callback issuer is compared to stored issuer under the approved conditional RFC 9207 rule. | PASS |
| Internal renewal failures fail the request without committing tentative state, while replay containment preserves its required transaction. | `BrowserSessions::current` returns `Renewal::Failed` without commit; `Renewal::Contained` commits family revocation/audit; focused PG tests cover refresh and API-key internal failures before/after expiry plus replay containment. A stored credential that cannot be opened maps to the same retryable internal-failure path, which is fail-closed at the request boundary. | PASS |
| User-facing UI capability has real client/server journey proof; units do not substitute for journeys. | `production-auth.integration.test.ts`, `identity_ui_e2e.rs`, and `test:identity:journey` cover production-built BFF replicas, actual providers, TLS, tenant separation, permissions, callback/state failures, renewal expiry, and OIDC-off entry. | PASS |
| Generated artifacts are regenerated and public contract parity is checked. | The callback source and schema snapshots agree; implementation evidence records green `codegen:check` and `docs:check`. | PASS |
| No gate circumvention, legacy compatibility path, unrequested dependency, feature, or alternate authority was introduced. | Complete cumulative diff contains no new compatibility route, Cargo feature, auth/session owner, password store, provider-specific soft-pass path, or weakened/removed check. | PASS |

## Material finding

### STD-TASK-003-R4-1 — Renewal classification unnecessarily widens the public Rust API

- **Rule:** `architecture/references/languages/rust-core.md` requires `pub(crate)` by default and permits `pub` only for an intentional public surface; `AGENTS.md` §§4–5 and 15 require the smallest cohesive API.
- **Locations:** `crates/wyrd/wyrd-auth/src/issuance.rs:294`, `crates/wyrd/wyrd-auth/src/refresh.rs:64`, `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:97`.
- **Evidence:** the three new `is_refusal` methods are public. Repository-wide caller inspection finds every use in `wyrd-auth` (`browser_sessions.rs`, plus the error owners calling down to `IssuanceError`); no external crate consumes any method.
- **Consequence:** the remediation turns a private browser-renewal classification detail into three externally supported crate APIs, widening the compatibility and documentation contract without a task, specification, or existing consumer requiring it.
- **Testable correction:** change all three methods to `pub(crate) fn is_refusal`. Retain the existing unit classification test and the three focused Postgres renewal tests; run `mise run fmt`, `mise run lints`, and `mise run test:wyrd`.

## Verification notes

The candidate records successful focused Postgres selectors for replay containment, refresh internal-failure retry, API-key internal-failure retry, and ordinary refusal; the classification unit selector; and successful `mise run fmt`, `mise run lints`, `mise run check:tenant-isolation`, `mise run test:wyrd`, `mise run test:sql`, `mise run test:identity:journey`, `mise run codegen:check`, `mise run docs:check`, UI `pnpm test`, UI `pnpm check`, and `git diff --check`. This review inspected the source, complete cumulative diff, schema parity, task definitions, and recorded results; it did not rerun the expensive lanes.

## Overall result

**FAIL** — `STD-TASK-003-R4-1` is a bounded repository-standards violation. All other applicable repository rules reviewed here pass.
