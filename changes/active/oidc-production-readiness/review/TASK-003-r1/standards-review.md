# Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `4e0ca8d2ecc2940724861cf6884b68f9adf65464`
- Reviewed range: complete `base..candidate` diff (43 files)
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Approved authority: revision 5 of `changes/active/oidc-production-readiness/spec.md` from `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20`

## Authority coverage

| Changed surface | Applicable authority | Coverage and result |
|---|---|---|
| `wyrd-spec` callback contract and generated JSON schemas | `AGENTS.md` §§2, 4, 9, 11, 12, 16; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; generated-artifact rule in `architecture/agent-rules.md` | **PASS.** `CallbackQuery` remains a typed pure contract and the source change is reflected in both schema projections. The candidate records a passing focused Rust test and `mise run codegen:check`; no hand-authored contract fork or server dependency entered `wyrd-spec`. |
| Rust browser-session owner and login integration | `AGENTS.md` §§5, 6, 9, 16; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/wyrd-security-posture.md` | **FAIL (STD-001).** The stateful workflow is correctly owned by `BrowserSessions`, async methods await database/issuance IO, and secrets use `SecretString`/the sealing keyring. New Rust items do not all meet the mandatory rustdoc rule. |
| Private BFF Axum channel, auth state, boot, config, router | `AGENTS.md` §§5, 6, 9, 16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md` | **FAIL (STD-001).** The route is mounted only with configured key hashes, every handler is typed and trace-instrumented, and the common protective edge supplies request IDs, bounds, timeouts, and panic mapping. One new conversion method lacks required rustdoc, and the config insertion displaced another helper's rustdoc. |
| Postgres migration, RLS queries, tenant discovery, transaction ownership | `AGENTS.md` §§3, 9, 15; `architecture/agent-rules.md` SQL rules; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md` | **PASS.** Tenant data uses `&mut TenantConn<'_>` and callees never commit/rollback; cross-tenant tenant-directory access uses `OperatorPool`; session/completion discovery is narrowly exposed through revoked-from-public, `wyrd_app`-only `SECURITY DEFINER` functions before opening RLS transactions. No new production signature or field propagates `PgPool`, `PgConnection`, or caller-owned `Transaction`. |
| SvelteKit hooks, server auth/session helper, routes, settings actions, components | `AGENTS.md` §§2, 9, 11, 12; `architecture/references/languages/typescript-guide.md`; `.agents/skills/wyrd-ui/SKILL.md`; `wyrd-sveltekit-architecture.md`; `wyrd-testing-verification.md` | **PASS.** Credential-bearing work stays in server-only modules and server loads/actions. Cookies are host-only, Secure, HttpOnly, SameSite=Lax; tenant keys are validated; mutating actions enforce POST, exact origin, expiry, and constant-time CSRF comparison; the browser receives no Wyrd bearer. Exported operations have explicit return types and the UI projects server permissions rather than mapping roles. Styling uses existing tokens/patterns and preserves labeled controls and alert semantics. |
| Rust, Vitest, component, migration, and unit tests | `AGENTS.md` §§11; `architecture/agent-rules.md` test placement and exact-command rules; `architecture/references/languages/testing-workflows.md`; UI testing reference | **PASS.** The external Rust test earns its location by starting a bound real server and two production BFF processes. The real HTTP Vitest journey is excluded from the fast lane and is reached through the ignored/gated identity lane. The candidate evidence names exact Vitest selectors and reports both filtered journeys plus the unfiltered identity lane. The migration test extends the real-Postgres migration/RLS inventory. |
| `mise.toml` identity journey routing | `AGENTS.md` §§11, 12; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/spec-driven-development.md` | **PASS.** `WYRD_IDENTITY_TARGET=ui` selects the production BFF host, exact filters are counted before execution, and the unfiltered lane retains the prior server journey as well as the new UI journey. No gate was weakened or hidden; the integration file is excluded only from the credential-free unit lane and has an owning journey lane. |
| Generated-artifact provenance and permanent-code hygiene | `architecture/agent-rules.md`; `AGENTS.md` §§12, 15, 16 | **PASS except STD-001.** Schema files correspond to the changed source contract and the recorded codegen check is green. Production code contains no task/agent references, legacy compatibility aliases, new dependency, or new Cargo feature. `git diff --check base..candidate` is clean. |

## Applicable rule results

| Rule | Result | Source evidence |
|---|---|---|
| Server owns durable session identity, credentials, tenancy, and renewal | PASS | `crates/wyrd/wyrd-auth/src/browser_sessions.rs:119-130`; `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:57-66` |
| Receiving trust boundary authenticates internal calls; browser input cannot select effective tenant | PASS | `crates/wyrd/wyrd-server/src/components/auth/bff.rs:35-86`; `crates/wyrd/wyrd-auth/src/browser_sessions.rs:196-217,261-287,436-512` |
| Tenant SQL uses `TenantConn`; privileged cross-tenant access uses `OperatorPool`; callee does not own commit | PASS | `crates/wyrd/wyrd-sql/src/queries/auth/browser_sessions.rs:224-335`; `crates/wyrd/wyrd-sql/src/postgres.rs:230-320`; commits remain in `BrowserSessions` workflow methods |
| Typed server request/response bodies and structured Wyrd errors | PASS | `crates/wyrd/wyrd-server/src/components/auth/bff.rs:107-320`; handlers return `WyrdErrorResponse` |
| Trace instrumentation on changed write/read handlers with secret arguments scrubbed | PASS | all BFF handlers use `#[tracing::instrument(..., skip_all)]` at `bff.rs:121-320` |
| Core Rust is struct-centered; async must await real IO | PASS | `BrowserSessions` and `BffChannel` own dependencies; async workflow methods directly await SQL, issuance, or request execution |
| Secrets are redacted and encrypted at rest | PASS | `SecretString` boundaries in `browser_sessions.rs`; sealed columns and live/revoked constraints in `20261001000001_auth_browser_sessions.sql:11-56` |
| UI keeps backend credentials server-side and enforces session/CSRF/tenant binding | PASS | `server-sessions.ts:75-180,183-241,298-333`; `hooks.server.ts:10-31` |
| Every user-facing capability has a real journey, with integration work outside the fast lane | PASS | `identity_ui_e2e.rs:1-13,189-275`; `production-auth.integration.test.ts`; `vite.config.ts` integration exclusion; `mise.toml` UI target |
| New and materially modified Rust items have substantive rustdoc, including private and trait methods | **FAIL** | `browser_sessions.rs:132-135` and `bff.rs:167-174` add undocumented methods; `config.rs:3505-3531` attaches `env_opt`'s description to the new parser and leaves `env_opt` undocumented |
| Generated files are changed through source and verified for drift | PASS (evidence claim) | `oidc.rs` source change plus both `auth_callback_query.json` projections; task evidence records `mise run codegen:check` |
| No gate circumvention | PASS | The ignored Rust test is an explicitly gated real-server journey with an owning `mise` lane, consistent with the test taxonomy; no lint allow, weakened assertion, or removed coverage was found |

## Material findings

### STD-001 — Mandatory Rust documentation coverage is incomplete

- **Violated rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require substantive rustdoc for every new or materially modified Rust item, including private functions and methods; missing documentation is `BLOCK_BEFORE_MERGE`.
- **Locations:**
  - `crates/wyrd/wyrd-auth/src/browser_sessions.rs:132-135` — new `Debug::fmt` method has no rustdoc.
  - `crates/wyrd/wyrd-server/src/components/auth/bff.rs:167-174` — new `From::from` conversion has no rustdoc explaining that it is the one secret-bearing channel-to-wire projection.
  - `crates/wyrd/wyrd-server/src/config.rs:3505-3531` — the pre-existing `env_opt` rustdoc was left above the newly inserted parser, so `parse_bff_service_key_hashes` now starts with an incorrect environment-reader sentence and `env_opt` has no rustdoc at all.
- **Consequence:** The candidate violates a repository hard acceptance gate and leaves the security-sensitive conversion/config boundary inaccurately documented even if compilation and tests are green.
- **Testable correction:** Add intent-bearing rustdoc to the two trait methods, move the environment-variable description back onto `env_opt`, and leave `parse_bff_service_key_hashes` with only its own parsing/rotation contract. Run the repository rustdoc/lint gate that covers the touched crates plus `mise run fmt`; retain the existing focused config test.

## Verification reviewed

Candidate evidence reports all of the following green: both exact filtered UI identity journeys, the unfiltered identity journey (2 Vitest journeys and 28 Rust identity journeys), UI unit tests (175), UI check, Rust format, workspace lints, codegen drift check, `test:wyrd` (2338), the focused cards wildcard test, the exact `CallbackQuery` Rust test, and `git diff --check`.

This reviewer did not rerun Cargo-backed lanes, as instructed. I independently ran `git diff --check 63c5bffc93cd2f7b5ed558e610a213efcc34fd49..4e0ca8d2ecc2940724861cf6884b68f9adf65464`; it produced no output. The recorded green checks do not waive STD-001 because the repository rule makes documentation completeness independently mandatory, and the current `check:docs` task does not cover all touched server/auth crates.

## Overall result

**FAIL** — repository structure, tenancy, typed boundaries, UI boundary, and test-tier placement conform, but STD-001 is a hard repository-rule violation.
