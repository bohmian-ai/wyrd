# Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `8289fa298ed33d21f2568558bc0a02905fd0b218`
- Latest remediation locator: `622a77028..8289fa298ed33d21f2568558bc0a02905fd0b218`
- Approved authority: revision 7 of `changes/active/oidc-production-readiness/spec.md`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation: `changes/active/oidc-production-readiness/review/TASK-003-r2/TASK-003-R2-production-ui-remediation.md`
- Human directions: `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md` and `changes/active/oidc-production-readiness/review/TASK-003-r2/human-direction-connection-test.md`
- CodeGraph: unavailable; the repository has no `.codegraph/` directory.

This pass reviews repository standards only. It does not decide task acceptance or perform the structured Ponytail validation.

## Authority coverage

| Changed surface | Applicable authority | Coverage and result |
|---|---|---|
| Change packet, approved revision, remediation evidence, and human directions | `AGENTS.md` §§12, 14, 16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md` | **PASS.** The candidate retains the cumulative task packet, records revision 7 as approved, and makes the real-sign-in direction explicit rather than treating implementation or tests as authority. |
| OIDC discovery, typed callback and connection contracts, error catalog, and generated schemas | `AGENTS.md` §§2-6, 9, 10, 12, 16; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md` | **PASS.** `wyrd-spec` remains synchronous and IO-free; provider IO stays in `wyrd-auth-oidc`; contracts and public errors remain typed. The prior discovery rustdoc defect is closed at `provider.rs:28-68`. |
| Human-connection testing, callback exchange, browser sessions, API-key recovery, sealing, and server handlers | `AGENTS.md` §§4-6, 9, 15, 16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md`; `languages/maintainer-style.md` | **FAIL (STD-R3-001).** Runtime owners remain cohesive, async operations await real IO, provider calls remain screened, secrets stay typed/redacted, and handlers use typed payloads and structured errors. One materially changed test-support module and helper violate the mandatory rustdoc rule. |
| Tenant SQL, migrations, login-state initiation binding, browser-session expiry, and cross-tenant rewrap inventory | `AGENTS.md` §§3, 6, 9, 15, 16; `architecture/agent-rules.md` SQL capability and transaction rules; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md` | **PASS.** Production query signatures take `&mut TenantConn<'_>` or `&OperatorPool`; query functions do not commit or roll back caller transactions; cross-tenant sealed-secret inventory stays on `OperatorPool`; the migration preserves forced-RLS ownership and makes exactly one initiation binding representable. |
| SvelteKit BFF sessions, private channel, upstream origin, tenant routes, settings, and components | `AGENTS.md` §§2, 9, 11, 12, 16; `architecture/references/languages/typescript-guide.md`; `architecture/wyrd-security-posture.md` | **PASS.** Credentials remain server-only, session metadata is server-derived, mutations retain same-origin/CSRF checks, and cookies retain Secure/HttpOnly/SameSite behavior. The prior unbounded chooser fan-out is now sequential (`server-sessions.ts:283-309`), and an explicit empty upstream URL reaches validation instead of the loopback default (`upstream.ts:8-26`). |
| Rust, SQL, TypeScript, and production browser journeys; `mise.toml` lane registration | `AGENTS.md` §11; `architecture/agent-rules.md` test placement and gate rules; `architecture/references/languages/testing-workflows.md` | **PASS.** Postgres/live-server tests remain in earned external or `pg_tests` locations, the identity journey owns the production BFF/provider topology, and focused cases cover the corrected concurrency/configuration branches. No production lint suppression, weakened assertion, new all-features test task, or unregistered ignored journey entered the diff. |
| Public docs, security authority, schema/docs generation, fixtures, and build configuration | `AGENTS.md` §§1, 2, 11, 12, 16; `architecture/agent-rules.md` generated-artifact rule; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md` | **PASS.** The lasting issuer-binding decision is in security authority, setup docs describe interactive testing, generated artifacts have recorded drift checks, and the candidate adds no dependency, Cargo feature, compatibility route, or source-tree build generation. |

## Applicable rule results

| Repository rule | Result | Source evidence |
|---|---|---|
| Durable identity, connection, session, role, and credential behavior remains server-owned; the UI projects it | PASS | `crates/wyrd/wyrd-auth/src/connections.rs:94-150,319-509`; `crates/wyrd/wyrd-auth/src/browser_sessions.rs`; `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:69-370` |
| Tenant and connection selection comes from server-bound single-use state, not callback, route, cookie, or provider data | PASS | `crates/wyrd/wyrd-auth/src/callback.rs:76-219`; `crates/wyrd/wyrd-sql/src/queries/auth/login_state.rs:1-316` |
| Tenant SQL uses `TenantConn`; cross-tenant work uses `OperatorPool`; query callees do not end caller transactions | PASS | `crates/wyrd/wyrd-sql/src/queries/auth/login_state.rs:216-316`; `human_connections.rs:381-424`; `crates/wyrd/wyrd-sql/src/postgres.rs`; `tenant_conn.rs:42-82` |
| Authorization decisions use the canonical audit path and required mutations fail closed when their audit cannot commit | PASS | `crates/wyrd/wyrd-server/src/components/admin/identity.rs:65-100,210-266`; `crates/wyrd/wyrd-auth/src/connections.rs:461-509,856-947`; callback audit-failure coverage in `crates/wyrd/wyrd-server/src/auth/callback.rs` |
| User/tenant provider URLs use screened, bounded provider IO; connection-test discovery/JWKS and callback reuse that capability | PASS | `crates/wyrd/wyrd-auth/src/connections.rs:94-110,344-410,784-827`; `crates/shared/wyrd-auth-oidc/src/provider.rs`; `architecture/wyrd-security-posture.md` SSRF boundary |
| Secret-bearing values are sealed/redacted and absent from browser page data and generated responses | PASS | `crates/wyrd/wyrd-auth/src/browser_sessions.rs`; `crates/wyrd/wyrd-server/src/components/auth/bff.rs`; `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:95-217,312-347` |
| Rust workflows are struct-centered; async is limited to database, provider, server, and process IO | PASS | `HumanConnections`, `AuthorizationCodeExchange`, `BrowserSessions`, `SealedSecretRewrap`, and `ServerSessions` own their dependencies and workflows; pure binding conversion remains synchronous in `login_state.rs:94-157` |
| Every new or materially modified Rust item, including modules and test helpers, has accurate substantive rustdoc; fallible and panicking operations document their failure modes | **FAIL** | `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:540-541,866-886` |
| TypeScript concurrent work has an explicit bound | PASS | `server-sessions.ts:283-309` verifies request-derived cookie hints sequentially; the focused test is `session.test.ts:265-306` |
| TypeScript configuration defaults use nullish semantics and validate the effective configured value | PASS | `upstream.ts:8-26`; `upstream.test.ts:70-80` |
| Every user-facing browser/auth capability has a real server journey; runtime-dependent behavior stays in its owning runtime | PASS | `crates/wyrd/wyrd-server/tests/identity_e2e.rs`; `identity_ui_e2e.rs`; `wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts`; `mise.toml:576-617` |
| Generated artifacts are source-derived and drift-checked; no generated file is treated as source authority | PASS (recorded evidence) | `wyrd-spec` source contracts and schema snapshots; implementation evidence records `mise run codegen:check` and `mise run docs:check` green |
| No gate circumvention | PASS | Cumulative diff adds no production `#[allow]`, deleted/ignored assertion, widened boundary allowlist, or test-task feature union; ignored live journeys remain explicitly registered in the identity lane |

## Material findings

### STD-R3-001 — Newly shared API-key test support has missing and incorrect rustdoc

- **Violated rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require substantive, accurate rustdoc for every new or materially modified Rust item, including modules and test helpers, plus `# Panics` for retained panic paths.
- **Location:** `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:540-541,866-886`.
- **Evidence:** The remediation changes `pg_tests` from a private module to `pub(crate)` so `browser_sessions::pg_tests` can reuse its fixtures, but the materially modified module has no rustdoc explaining that shared test-support role. It also changes `insert_live_api_key` to `pub(crate)` without correcting the comment attached to it: lines 866-876 describe a machine credential-exchange test and claim panic conditions involving fixture startup and assertions, while the helper at lines 881-904 only hashes and inserts one API key and can panic on hashing or SQL insertion. The actual helper description is appended after the unrelated `# Panics` prose, so its contract and failure behavior are not accurately documented.
- **Consequence:** The candidate violates the repository's hard documentation gate, and a maintainer following the newly exposed shared fixture cannot tell its actual panic boundary from the owning item. Green compiler/lint lanes do not establish this semantic documentation rule.
- **Testable correction:** Add concise module rustdoc stating that `pg_tests` owns API-key exchange tests and the crate-visible fixtures reused by sibling browser-session tests. Replace the stale machine-exchange paragraphs on `insert_live_api_key` with one cohesive helper contract and a `# Panics` section naming API-key hash or insert failure. Do not add a wrapper, new fixture module, or runtime test; formatting/lints plus source inspection are sufficient proof for this documentation-only correction.

## Verification reviewed

The R2 implementation evidence records these lanes green at candidate `8289fa298ed33d21f2568558bc0a02905fd0b218`: UI `pnpm test` (183 passed) and `pnpm check`; `mise run test:identity:journey`; `mise run test:wyrd`; `mise run test:sql`; `mise run codegen:check`; `mise run check:tenant-isolation`; `mise run docs:check`; `mise run fmt`; `mise run lints`; and `git diff --check`. It also names the focused Rust and Vitest cases for every R2 correction and the real interactive connection-test direction.

Per the review assignment, this reviewer ran no builds or tests. Read-only inspection confirmed the candidate identities, the complete cumulative and latest-remediation diffs, the routed authorities, relevant source/callers/tests/migration/mise configuration, and a clean `git diff --check base..candidate`. The recorded verification is broad enough for the touched runtime surfaces, but it cannot waive STD-R3-001 because the repository's private-item documentation rule is broader than compiler missing-doc coverage.

## Overall result

**FAIL** — the prior SQL, provider, browser-session, bounded-concurrency, upstream-configuration, generated-artifact, and journey-lane standards findings are closed, but `exchange_api_key::pg_tests` and its newly shared `insert_live_api_key` helper still violate the repository's hard Rust documentation rule.
