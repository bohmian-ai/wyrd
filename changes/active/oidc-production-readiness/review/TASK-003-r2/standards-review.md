# Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `05ff68fb47572e7d8e5fa34037042559bcfeac83`
- Remediation locator: `4e0ca8d2ecc2940724861cf6884b68f9adf65464..05ff68fb47572e7d8e5fa34037042559bcfeac83`
- Approved specification: revision 5 of `changes/active/oidc-production-readiness/spec.md` at `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation: `changes/active/oidc-production-readiness/review/TASK-003-r1/TASK-003-R1-production-ui-remediation.md`
- Human direction: `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`, which replaces FIND-TASK-003-1 and R1-AC-01
- CodeGraph: not available; the repository has no `.codegraph/` directory.

This pass reviews repository standards only. It does not decide task acceptance or validate the prior finding ledger.

## Authority coverage

| Changed surface | Applicable authority read | Coverage and result |
|---|---|---|
| Change packet, remediation evidence, and explicit human amendment | `AGENTS.md` §§12, 14; `architecture/references/languages/spec-driven-development.md` | **PASS.** The cumulative candidate retains the original task, prior review, remediation, and explicit human decision. The evidence records the immutable candidate and named focused/broader lanes without changing production authority through a test or summary. |
| OIDC provider discovery and callback contract (`wyrd-auth-oidc`, `wyrd-spec`, generated callback schemas) | `AGENTS.md` §§3-6, 9, 10, 12, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` runtime identity; `architecture/wyrd-security-posture.md` federation; `architecture/references/architecture/patterns.md`; `rust-core.md`; `errors.md` | **FAIL (STD-R2-001).** The contract remains typed and IO-free in `wyrd-spec`; provider IO remains in the provider owner; `CallbackQuery.iss` is reflected identically in both schema snapshots. The new raw discovery field and materially modified fallible parser lack mandatory private-item rustdoc. |
| Browser-session, API-key exchange, callback, sealing, and BFF Rust owners | `AGENTS.md` §§4-6, 9, 15, 16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `rust-core.md`; `errors.md`; `maintainer-style.md` | **PASS.** Stateful workflows remain on `BrowserSessions`, `AuthorizationCodeExchange`, `ExchangeApiKey`, `SealedSecretRewrap`, and `BffChannel`; async methods await SQL/network/issuance IO; secret-bearing values use `SecretString` or sealed bytes; changed handlers use typed bodies, structured Wyrd errors, and scrubbed trace instrumentation. The prior cited rustdoc sites are corrected. |
| SQL migration, tenant queries, browser-session persistence, rewrap CAS, and boot inventory | `AGENTS.md` §§3, 9, 15, 16; `architecture/agent-rules.md` SQL capability and transaction rules; `architecture/wyrd-security-posture.md` tenant/data isolation and cryptography; `architecture/references/architecture/patterns.md` | **PASS.** Production tenant functions accept `&mut TenantConn<'_>` and do not commit or roll back. Cross-tenant rewrap and directory work use `&OperatorPool`; raw `PgPool`, naked connections, and caller-provided SQL transactions do not enter production signatures or fields. `SealedSecretTable` keeps CAS work in `wyrd-sql`, and transaction lifecycle stays with the Rust workflow owner. |
| SvelteKit server sessions, upstream origin, hooks, routes, settings, chooser, and components | `AGENTS.md` §§2, 9, 11, 12; `architecture/references/languages/typescript-guide.md`; `.agents/skills/wyrd-ui/SKILL.md`; `wyrd-sveltekit-architecture.md`; `wyrd-testing-verification.md` | **FAIL (STD-R2-002, STD-R2-003).** Credentials remain server-only, cookies keep Secure/HttpOnly/SameSite attributes, routes use server loads/actions, and tenant authority comes from server-returned session state. The repaired chooser verifies cookie hints, but fans them out through an unbounded `Promise.all`. The upstream default uses `||`, so an explicitly empty security-sensitive URL bypasses validation and silently becomes loopback. |
| Rust/TypeScript unit, Postgres integration, and real browser journeys; `mise.toml` lane routing | `AGENTS.md` §11; `architecture/agent-rules.md` test placement/gate rules; `architecture/references/languages/testing-workflows.md`; UI testing reference | **PASS.** External Rust targets earn their placement by starting Postgres, a bound Wyrd server, provider services, and production BFF processes. Ignored tests are registered in the explicit identity journey lane; the fast UI lane excludes the real integration file, while `test:identity:journey` verifies exact required Rust and Vitest names before running them. No `#[allow]`, weakened test, hidden selector, or new all-features test task entered the diff. |
| Public docs, security authority, generated `llms-full.txt`, schema docs, fixtures, and generated artifacts | `AGENTS.md` §§1, 2, 12, 16; `architecture/agent-rules.md` generated-artifact rule; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md` | **PASS.** The lasting RFC 9207 decision is recorded in the security authority and self-hosting docs; source and generated callback schemas agree; generated docs are covered by the recorded docs/codegen lanes. Permanent production code contains no task IDs, agent references, legacy compatibility aliases, or commercial-edition scaffolding. |
| Dependencies, features, manifests, and build/test configuration | `AGENTS.md` §§1, 4, 11, 12; `architecture/agent-rules.md`; `testing-workflows.md` | **PASS.** No dependency, Cargo feature, per-crate profile, or client-tier dependency was added. The `mise.toml` edit extends the existing identity journey rather than creating a competing gate or broad feature-union build. |

## Applicable rule results

| Repository rule | Result | Source evidence |
|---|---|---|
| Durable identity/session behavior remains server-owned; UI is a projection | PASS | `crates/wyrd/wyrd-auth/src/browser_sessions.rs:112-163`; `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:60-70` |
| Tenant identity comes from verified/server-bound state, not route or cookie hints | PASS | `browser_sessions.rs:203-250,273-317,327-369`; `server-sessions.ts:186-217,283-306` |
| Tenant SQL uses `TenantConn`; cross-tenant work uses `OperatorPool`; callees do not end caller transactions | PASS | `crates/wyrd/wyrd-sql/src/queries/auth/browser_sessions.rs:205-320`; `human_connections.rs:381-424`; `crates/wyrd/wyrd-sql/src/postgres.rs:180-328` |
| Rust workflows are struct-centered and async only at real IO boundaries | PASS | `BrowserSessions`, `AuthorizationCodeExchange`, `SealedSecretRewrap`, and `BffChannel` own the dependencies and workflows they use; pure issuer comparison remains synchronous at `callback.rs:559-585` |
| Public/private HTTP bodies are typed and public failures use structured Wyrd errors | PASS | `crates/wyrd/wyrd-server/src/components/auth/bff.rs:108-327`; `crates/wyrd/wyrd-auth/src/callback.rs:337-349` |
| Secret-bearing state is redacted, sealed, and absent from browser page data | PASS | `browser_sessions.rs:56-97,237-249,304-316`; `bff.rs:231-279`; `server-sessions.ts:14-37,299-306` |
| Every new or materially modified Rust item, including private fields/helpers/tests, has substantive rustdoc; every fallible function has `# Errors` | **FAIL** | `crates/shared/wyrd-auth-oidc/src/provider.rs:21-30,53-89` lacks docs for the added raw field and for the materially modified fallible parser |
| TypeScript concurrent work has an explicit bound; bare `Promise.all` over an unbounded collection is forbidden | **FAIL** | `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:290-306` fans out one internal request per cookie-derived hint with bare `Promise.all` |
| TypeScript defaults use nullish semantics so valid falsey values are not silently reinterpreted | **FAIL** | `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.ts:14-24` uses `||` on deployment configuration; an explicitly empty value becomes the loopback default instead of failing validation |
| SvelteKit keeps auth/backend access in hooks, server loads/actions, and server-only helpers | PASS | `hooks.server.ts`; `src/lib/server/auth/server-sessions.ts`; `routes/t/[tenantKey]/+layout.server.ts`; tenant login/settings server routes |
| Every user-facing UI capability has a real server journey and runtime-specific tests stay in their owning runtime | PASS | `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:1-16,246-350`; `production-auth.integration.test.ts`; `mise.toml:690-736` |
| Generated artifacts are source-derived and drift-checked | PASS (recorded evidence) | `crates/wyrd-spec/src/auth/oidc.rs:389-410`; both `auth_callback_query.json` files are byte-identical; implementation evidence records `mise run codegen:check` and `docs:check` green |
| No gate circumvention | PASS | Only real environment journeys carry `#[ignore]`, and each is selected by `test:identity:journey`; cumulative diff adds no production lint allow or disabled assertion |

## Material findings

### STD-R2-001 — New provider discovery state and its parser lack mandatory rustdoc

- **Violated rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require substantive rustdoc for every new or materially modified Rust item, including private fields and helpers, and an `# Errors` section on every fallible function.
- **Location:** `crates/shared/wyrd-auth-oidc/src/provider.rs:21-30,53-89`.
- **Evidence:** `authorization_response_iss_parameter_supported` is a new field on private `RawProviderMetadata` with only `#[serde(default)]`. `parse_raw_metadata` was materially modified to project that field but has no rustdoc and no `# Errors`, despite returning `Result<ProviderMetadata, OidcError>` from three URL parses.
- **Consequence:** The candidate violates the repository's hard documentation gate at the OIDC trust boundary. A maintainer cannot learn from the owning item why an absent discovery member becomes `false`, or which malformed endpoint fields the parser rejects.
- **Testable correction:** Document the new raw field's absent-is-false wire meaning and add intent/operation rustdoc plus an `# Errors` section to `parse_raw_metadata`. Do not document unrelated untouched fields or add a wrapper. Retain the existing provider discovery tests and run the owning format/lint/doc coverage.

### STD-R2-002 — Cookie hints create unbounded concurrent internal session reads

- **Violated rule:** `architecture/references/languages/typescript-guide.md` requires independent async work to run only inside an explicit bound and explicitly rejects bare `Promise.all` over an unbounded collection.
- **Location:** `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:290-306`.
- **Evidence:** `metadata` derives `hints` from every matching request cookie and executes `Promise.all([...hints].map((key) => this.read(key, cookies)))`. The request supplies the collection and the method sets no concurrency bound; every element becomes an authenticated private-channel request and database-backed session read.
- **Consequence:** A request with many syntactically valid forged session-cookie names fans out concurrent BFF-to-server calls before the invalid hints are cleared, amplifying one browser request into avoidable internal load.
- **Testable correction:** Resolve the existing distinct hints with an explicit concurrency bound using repository/native behavior (sequential resolution is sufficient and needs no dependency), while preserving invalid-cookie clearing and the server-verified output. Extend the focused metadata test with multiple forged hints and assert the bounded request behavior as well as the rendered result.

### STD-R2-003 — An explicitly empty upstream URL silently becomes the loopback default

- **Violated rule:** `architecture/references/languages/typescript-guide.md` requires nullish coalescing rather than `||` for defaults; security input validation must validate the effective configured value.
- **Location:** `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.ts:14-24`.
- **Evidence:** `const configured = env.WYRD_SERVER_URL || 'http://127.0.0.1:8080'` treats an explicitly configured empty string as if the setting were absent. The subsequent URL/TLS validation therefore checks the fallback, not the invalid deployment value.
- **Consequence:** A malformed security-sensitive production configuration does not fail closed; the BFF silently targets a loopback listener, which may be unavailable or may be a different local process.
- **Testable correction:** Default only when the value is absent (`??`), leaving an explicitly empty value to the existing URL parser's refusal. Add one focused upstream test that sets the variable to an empty string and proves refusal before the fetcher observes a request.

## Verification reviewed

Implementation evidence records all of these green after the last code commit: UI `pnpm test` and `pnpm check`; `mise run test:identity:journey` (four UI journeys and thirty Rust identity journeys); `mise run test:wyrd`; `mise run test:sql`; `mise run codegen:check`; `mise run check:tenant-isolation`; `mise run docs:check`; `mise run fmt`; `mise run lints`; and `git diff --check`.

This reviewer did not start Cargo- or mise-backed work because the shared-checkout review explicitly prohibited overlapping builds. Static checks performed here confirmed that the candidate remained `05ff68fb47572e7d8e5fa34037042559bcfeac83` during source inspection, the two callback schema snapshots are byte-identical, and `git diff --check base..candidate` is clean. Green recorded lanes do not waive STD-R2-001 because the private-item documentation rule is broader than compiler missing-docs coverage, nor STD-R2-002/003 because those are source-shape and effective-value rules not established by the existing happy-path tests.

## Overall result

**FAIL** — SQL capabilities, transaction ownership, generated artifacts, server/UI ownership, and journey-lane integrity conform, but three bounded repository-rule violations remain: incomplete Rust documentation at the OIDC discovery boundary, unbounded TypeScript fan-out over cookie hints, and falsey fallback that bypasses validation of an explicitly empty upstream URL.
