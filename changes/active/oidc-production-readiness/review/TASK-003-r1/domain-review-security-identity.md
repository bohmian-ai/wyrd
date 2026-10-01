# Security and identity domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `4e0ca8d2ecc2940724861cf6884b68f9adf65464`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 5 as recorded at `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20`
- Task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md` at the candidate

The candidate commit resolved to the same immutable object before this report was written.

## Reviewed boundary

This pass traced the TASK-003 security and identity boundary end to end:

- tenant login entry, the common provider callback, callback query parsing, server-owned state, browser-flow binding, sealed completion, and one-use redemption;
- the deployment BFF service-key configuration, rotation overlap, middleware admission, internal route mounting, and all browser-session operations;
- session-id generation and hashing, tenant lookup and RLS re-entry, encrypted access/refresh/API-key/CSRF storage, renewal serialization, logout, cookies, CSRF/origin checks, and tenant switching;
- OIDC-off API-key exchange, tenant binding, permission projection including wildcard grants, and tenant-settings mutations;
- public-origin use, error/log redaction, page/URL exposure, and sealing-key rotation through the existing canonical rewrap owner;
- the changed callback and UI journeys, focused unit tests, migration constraints, and claimed verification evidence in the task.

## Authority and source coverage

| Boundary | Governing authority | Source inspected |
|---|---|---|
| Tenant and principal authority | Approved spec REQ-006/007/009/010/015/016, INV-001/003/005; `AGENTS.md` tenant isolation and server ownership; `architecture/wyrd-security-posture.md` | `wyrd-auth/src/login.rs`, `callback.rs`, `browser_sessions.rs`; `wyrd-sql/src/postgres.rs`, `queries/auth/login_state.rs`, `browser_sessions.rs`; browser-session migration |
| OAuth/OIDC callback | Approved spec REQ-006/007 and INV-001/004; RFC 9207 §§2.4, 4; RFC 9700 §§2.1, 4.4.2; OIDC Core authorization-code flow | `wyrd-spec/src/auth/oidc.rs`; `wyrd-server/src/components/auth/routes.rs`; `wyrd-auth/src/callback.rs`; generated callback schemas and callback test |
| BFF credential and session channel | TASK-003 packet-local contract; approved spec REQ-005/009/010; `architecture/wyrd-security-posture.md` credential lifecycle | `wyrd-server/src/components/auth/bff.rs`, `http/router.rs`, `config.rs`, `boot/mod.rs`; UI `server-sessions.ts`, hook, completion/login/settings routes |
| Browser protection and RBAC | Approved spec REQ-009/015/016, INV-003/005; task CSRF, tenant, permission, and logout contract | UI `server-sessions.ts`, `hooks.server.ts`, root and tenant layouts/actions, settings actions and views, `wyrd.ts`; Rust browser-session owner and SQL queries |
| Credential secrecy and key rotation | Approved spec REQ-005 and AC-007; task rotation contract; existing sealing rotation authority | `wyrd-auth/src/sealing.rs`, `browser_sessions.rs`; `wyrd-sql` sealed-secret queries and browser-session table/query code; config/boot keyring wiring |
| Proof | TASK-003 named scenarios and verification requirements | `identity_ui_e2e.rs`, `production-auth.integration.test.ts`, `session.test.ts`, callback unit test, task evidence |

## Material findings

### SEC-ID-001 — INCORRECT (High): the shared callback discards the authorization-response issuer instead of validating it

- **Violated obligation:** Approved spec REQ-007 and INV-004 require a fail-closed, issuer-bound OIDC flow. The approved authority includes RFC 9700. Wyrd interacts with multiple independent authorization servers through one callback, so RFC 9700 §4.4.2 requires a mix-up defense. Under the issuer-identification defense, RFC 9207 §2.4 requires a present `iss` response parameter to be compared by exact string equality with the issuer to which the authorization request was sent and requires rejection on mismatch.
- **Location:** `crates/wyrd-spec/src/auth/oidc.rs:400-414`, especially the statement that RFC 9207 `iss` is ignored; `crates/wyrd/wyrd-server/src/components/auth/routes.rs:327-345`, where only `code` and `state` reach the exchange; `crates/wyrd-spec/src/auth/oidc.rs:634-655`, whose regression test positively requires discarding `iss`.
- **Evidence:** The login producer already persists the exact issuer selected for the flow in `crates/wyrd/wyrd-auth/src/login.rs:114-120`, and the callback later reloads that state. The candidate nevertheless deserializes `iss` as an unknown field and drops it before `AuthorizationCodeExchange` can compare it. The deployment intentionally uses one common redirect URI for different tenant issuers, so it does not use RFC 9700's distinct-redirect alternative. See [RFC 9207 §2.4](https://www.rfc-editor.org/rfc/rfc9207.html#section-2.4) and [RFC 9700 §4.4.2](https://www.rfc-editor.org/rfc/rfc9700.html#section-4.4.2).
- **Plausible exploit path:** In a multi-provider deployment where one authorization server or its metadata/endpoints are attacker-controlled, a mix-up response can carry a valid victim flow state and an `iss` identifying the attacker's server. The candidate ignores the one response value intended to detect that mismatch and proceeds to handle the code under the state-selected endpoint. Depending on the malicious endpoint arrangement, this can disclose a code or client credential to the wrong server or bind the flow to the wrong authorization server. Later ID-token verification is not a substitute for validating the authorization response before sending the code and client authentication to a token endpoint.
- **Observable consequence:** A callback that explicitly says it came from a different issuer is not rejected at the callback boundary. The current test makes this standards violation a required behavior.
- **Testable correction:** Keep tolerance for unrelated provider parameters such as Keycloak `session_state`, but model optional `iss` explicitly. Carry it into the existing `AuthorizationCodeExchange` owner and, before code exchange, compare it exactly with the issuer stored in the consumed login state; reject and audit a mismatch without calling any token endpoint. Preserve provider metadata support where available so a provider advertising `authorization_response_iss_parameter_supported=true` also cannot omit `iss`. Replace the ignore assertion with focused mismatched-issuer rejection and matching-issuer acceptance tests, plus a callback journey proving a mismatch performs no token exchange and establishes no completion/session.

### SEC-ID-002 — INCORRECT (Medium): canonical sealing-key rotation omits all new durable browser-session ciphertext

- **Violated obligation:** Approved spec REQ-005 requires operators to rotate the deployment sealing keyring without making existing sessions permanently unusable, and AC-007 requires rotated-key evidence. TASK-003 stores recoverable browser-session credentials under that same keyring.
- **Location:** `crates/wyrd/wyrd-auth/src/browser_sessions.rs:230-243` and `292-305` create encrypted access tokens, refresh tokens or API keys, and CSRF tokens; `crates/wyrd/wyrd-sql/migrations/20261001000001_auth_browser_sessions.sql:28-35` persists those four ciphertext classes. The canonical rotation owner at `crates/wyrd/wyrd-auth/src/sealing.rs:73-118` walks only human connections, workload issuers, and the platform connection. No browser-session row or column is listed or compare-and-swap rewrapped anywhere in the candidate.
- **Evidence:** The documented rotation procedure permits retiring K1 after a post-rollout rewrap reports `remaining == 0` (`wyrd-auth/src/sealing.rs:3-11`). Because browser sessions are invisible to that report, it can report zero while live access, refresh, API-key, and CSRF ciphertext remains under K1. Ordinary renewal rewrites an access token and, for SSO, its refresh token, but never rewrites the CSRF token; API-key renewal also never rewrites the stored API key. The task's claimed UI journey starts with one key and contains no K1-to-K2 retirement exercise.
- **Observable consequence:** Following the repository's own successful rotation signal and removing K1 makes live sessions fail to open on their next read. OIDC-off sessions lose their stored bootstrap credential, SSO sessions lose at least their CSRF credential, and logout can silently skip refresh revocation when `open_text` cannot open the old ciphertext (`browser_sessions.rs:406-415`). This is precisely the permanent session breakage REQ-005 prohibits.
- **Testable correction:** Extend the existing `SealedSecretRewrap` workflow—do not create a second rotation path—to enumerate and compare-and-swap rewrap every non-null live `auth_browser_sessions` sealed column under the operator capability, counting unreadable or concurrently changed values in the same `remaining` signal. Preserve the existing late-writer/CAS rule. Add a rotation integration check that creates both SSO and API-key browser sessions under K1, switches to K2 with K1 retained, runs the canonical proof pass, removes K1, and proves both sessions still read, renew, pass CSRF, and log out correctly. The pass must not report zero while any live browser-session ciphertext still needs K1.

## Security audit summary

### Critical

None.

### High

- `SEC-ID-001`: authorization-response issuer validation is explicitly discarded on the shared multi-provider callback.

### Medium

- `SEC-ID-002`: key rotation's proof and repair owner does not cover durable browser sessions.

### Low / defense in depth

None. Optional hardening outside the approved task was not recorded as a finding.

### Positive controls

- Random 256-bit browser flow, session, and CSRF values; only session and flow digests enter tenant lookup/storage.
- Login state and completion are single-use, server-owned, connection-bound records, with tenant recovered from a narrow hash lookup before RLS re-entry.
- Browser cookies are Secure, HttpOnly, SameSite=Lax, host-only, path `/`, and bounded; tokens and API keys remain server-side.
- The BFF service key is checked before store access, supports a bounded two-hash overlap, and does not itself select tenant authority.
- Session renewal is serialized by a row lock and reuses the ordinary audited token issuers; tenant and connection lifecycle checks remain server-owned.
- Mutating UI actions require POST, exact same origin, a live tenant-bound session, and constant-time CSRF comparison; the Wyrd API independently enforces the session principal's permissions.
- Cross-tenant API-key sign-in returns an indistinguishable credential refusal, and provider/client/session secrets are skipped in request tracing and safe UI error projections.

## Verification limits

- Per assignment, no Cargo, mise, pnpm, or live-provider command was run. The task's recorded green commands are implementation claims, not independently reproduced evidence in this pass.
- Source inspection was performed against the immutable candidate, including the real-server/two-BFF journey source. Those journeys provide credible coverage for flow-cookie mismatch/replay, service-key refusal, cross-replica use, cookie attributes, CSRF/origin, cross-tenant refusal, permission denial, logout, and token/page secrecy.
- The existing proof does not exercise a mismatched RFC 9207 `iss` or a K1-to-K2 browser-session rotation followed by K1 removal; those are the focused proof gaps attached to the findings, not generic verification limits.

## Overall result

**FAIL**

Two material, bounded security/identity defects remain. The candidate does not satisfy the approved multi-issuer callback security authority or the required browser-session sealing-key rotation behavior.
