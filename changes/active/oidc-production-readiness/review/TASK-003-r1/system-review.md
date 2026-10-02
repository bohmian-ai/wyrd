# TASK-003 system-resilience review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `4e0ca8d2ecc2940724861cf6884b68f9adf65464`
- Approved authority: `SPEC-oidc-production-readiness`, revision 5, at `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20:changes/active/oidc-production-readiness/spec.md`
- Task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md` at the candidate

The candidate commit remained available under the same immutable object id throughout this review.

## Deployed-path coverage

| Changed runtime path | Owner and dependent topology | Source evidence | Assessment |
|---|---|---|---|
| Tenant login discovery and start | Browser -> either SvelteKit BFF -> public `/auth/login` -> `HumanConnections` -> Postgres and, after redirect, the tenant IdP | `server-sessions.ts:96-139`; `browser_sessions.rs:158-177`; `production-auth.integration.test.ts:125-164` | A BFF process holds no login authority. The random flow remains in an `HttpOnly` browser cookie and its binding is durable login state, so another BFF replica can complete the flow. An IdP outage stops new login only; established Wyrd browser sessions do not call the IdP during routine renewal. |
| SSO completion | Browser -> either BFF `/login/complete` -> service-key-authenticated `/internal/bff/v1/sessions/complete` -> one tenant transaction consuming completion and creating session | `server-sessions.ts:154-169`; `bff.rs:177-195`; `browser_sessions.rs:179-244`; `login_state.rs` redemption query | The delete of the completion and insert of the browser session share one tenant transaction. A server crash before commit leaves the completion redeemable; a crash or timeout after commit but before the response can orphan the created session and requires a fresh login, but cannot disclose or double-redeem credentials. That is a bounded fail-closed outcome. |
| Protected page and API use | Browser cookie -> BFF hook `read` -> internal session read -> Postgres row lock/optional renewal -> BFF authority call -> normal public Wyrd route with the session principal's access token | `hooks.server.ts:10-31`; `server-sessions.ts:183-241,298-333`; `browser_sessions.rs:308-378,422-555`; `browser_sessions.rs` SQL lock/rotate statements | Postgres is the cross-replica authority. A BFF restart loses no session state. A Wyrd replica restart loses no committed session state provided replicas retain the same signing and sealing keyrings. Postgres or Wyrd unavailability refuses the request as an upstream failure without falling back to mock identity or another tenant. |
| Concurrent refresh, connection cutoff, and logout | Each session use locks its browser-session row; refresh additionally uses the existing refresh-family and human-connection slot locks; logout locks the same browser row before revocation | `browser_sessions.rs:389-505`; `refresh.rs:84-227`; `issuance.rs:615-699`; `human_connections.rs:168-181,337-357`; `production-auth.integration.test.ts:229-232,281-309` | Concurrent BFF replicas serialize on the session row. Refresh rotation, audit, and session-row update share the caller-owned transaction, so interruption rolls back the combined change. Connection replacement/deactivation serializes with issuance, so a committed cutoff prevents a later renewal. Logout and refresh cannot both update the browser row concurrently. |
| OIDC-off session | Browser -> BFF -> internal API-key session creation -> ordinary audited API-key exchange; later renewal re-exchanges the sealed key under the session lock | `server-sessions.ts:171-181`; `browser_sessions.rs:246-306,541-552`; `production-auth.integration.test.ts:312-375` | A revoked/expired key ends only that browser session; no alternate credential or provider is attempted. A database or server outage refuses the request. The bootstrap key remains server-side and sealed. |
| BFF service-key rollout | BFF sends one raw key; Wyrd accepts one hash or two hashes during overlap | `server-sessions.ts:75-88`; `config.rs:2176-2186,3507-3529`; `bff.rs:35-87` | The source supports the intended overlap ordering: add old+new hashes on Wyrd replicas, roll BFF replicas to the new key, then remove the old hash. The supplied HTTP journey uses one key on both BFFs and one Wyrd server; it does not exercise a mixed-key rolling replacement. This is a verification limit, not a demonstrated implementation defect. |
| Sealing-key rollout | Wyrd stores access, refresh/API key, and CSRF envelopes in `auth_browser_sessions`; the pre-existing boot rewrap scans provider-secret stores only | `browser_sessions.rs:230-243,292-305,479-505`; migration `20261001000001_auth_browser_sessions.sql:8-18`; `sealing.rs:73-119`; `self-hosting/authentication.svx:63-88` | **Fail.** The operational procedure introduced before browser sessions now makes a false retirement decision for the candidate's new durable ciphertext. See `SYSTEM-001`. |

## Failure and recovery assessment

- **Single BFF failure or rolling BFF replacement:** another BFF replica can use the same browser cookie and Postgres-backed state. The candidate's real HTTP journey proves begin/complete and page use can cross its two BFF processes.
- **Wyrd process failure or cancellation:** open tenant transactions roll back and release row/advisory locks. Committed sessions survive in Postgres. An uncertain completion response may require a new login, but does not replay a completion or expose a token.
- **Postgres outage:** session creation, read, renewal, settings mutation, and logout fail at their request boundary. Existing rows are not replaced by local state and recover when Postgres returns. The candidate provides no explicit outage/recovery journey.
- **OIDC provider outage:** new authorization/callback work fails closed. Existing browser renewal is Wyrd refresh/API-key issuance and does not contact the provider, so unrelated established sessions and machine authentication remain available.
- **Public/API load or timeout:** internal BFF calls share the server's protected edge timeout and concurrency controls. They may return an upstream error under overload; there is no unbounded retry loop in the BFF. The browser retains its session cookie for non-`401` failures, allowing recovery on a later request.
- **Concurrent refresh/logout:** the browser-session row lock is the common serialization point. A committed logout wipes sealed values; a refresh interrupted before commit cannot leave a half-rotated row.
- **Connection deactivation/replacement:** the existing human-connection slot lock orders lifecycle mutation against renewal. Already issued access authority remains bounded; the next browser read/authority renewal ends the old-connection session.
- **Service-key rotation:** two server-side accepted hashes are implemented, but the candidate supplies no mixed-old/new rolling proof or operator procedure.
- **Sealing-key rotation:** retaining the old key keeps old sessions usable, but the published runbook instructs operators to remove it after a provider-only rewrap reports zero plus two minutes. That is unsafe for sessions created by this task.

## Affected capabilities and blast radius

The changed path affects every production UI page because `hooks.server.ts` resolves a server-owned session before tenant route resolution. A BFF or internal-channel outage removes UI access but does not remove public HTTP/SDK or machine authentication. A Postgres outage removes every session operation and the ordinary server operations that share Postgres. An IdP outage removes new SSO login but not established Wyrd sessions or OIDC-off/machine exchange. Premature sealing-key retirement affects every still-live browser session written under the retired key across all tenants, while independently authenticated SDK and machine traffic continues.

## Material finding

### SYSTEM-001 — The documented sealing-key retirement gate ignores durable browser-session ciphertext

- **Classification:** `INCORRECT`
- **Violated obligation:** SPEC revision 5 `REQ-005` requires keyring rotation without making existing sessions permanently unusable and requires the rotation procedure to be documented and tested. `REQ-018` requires self-hosted/hosted secret-rotation and failure documentation. TASK-003 makes recoverable browser-session credentials part of that same keyring boundary.
- **Locations:**
  - `crates/wyrd/wyrd-server/wyrd-ui` consumes the production session boundary, while `crates/wyrd/wyrd-auth/src/browser_sessions.rs:230-243,292-305,321-328,453-505` writes and later opens session access, refresh/API-key, and CSRF envelopes.
  - `crates/wyrd/wyrd-auth/src/sealing.rs:73-119` rewraps only human-connection, workload-issuer, and platform provider secrets.
  - `docs/src/content/docs/self-hosting/authentication.svx:65,76,82-88` omits browser-session credentials and says `remaining = 0` plus the two-minute login-completion wait means no stored secret needs the old key.
  - `docs/src/content/docs/self-hosting/sso-and-oidc.svx:21` likewise describes the key as protecting only the pre-redemption completion and connection secret; `docs/src/content/docs/concepts/cloud-identity.svx:119-122` still claims live key rotation is unsupported.
- **Evidence and reachable path:** Create any SSO or OIDC-off browser session while K1 is the write key. Roll replicas to K2 with K1 retained. The provider-secret rewrap can report `remaining = 0` because it never visits `wyrd.auth_browser_sessions`. Follow the published step 5 after two minutes and remove K1. The next protected page calls `BrowserSessions::read`; even if access/refresh were renewed under K2, `csrf_token_sealed` was never rotated, so `open_text(K2, csrf_token_sealed)` returns `InvalidToken`. A fresh access token also remains under K1 until renewal. Every affected browser is forced out despite following the documented successful rotation gate.
- **Observable system consequence:** A routine operator rotation can simultaneously end all pre-roll production UI sessions across tenants. The boot log positively tells the operator retirement is safe even though the new table still depends on K1. The headless API and machine paths remain available, so this is a browser-session blast-radius regression rather than a whole-service failure.
- **Smallest testable correction:** Reuse the existing retained-key mechanism and correct the operator contract rather than adding a new rewrap subsystem: document that the provider rewrap count does not cover browser sessions; retain K1 until every browser session that an old-key replica could have created has reached its absolute expiry (the candidate's maximum is 12 hours), in addition to the short login-completion window; then remove K1. Update the production UI deployment documentation with the BFF/session credential inventory and remove contradictory single-key guidance. Add a production-shaped rotation test that creates a session under K1, rolls to K2 with K1 retained, and proves both BFF replicas can still read, renew, act, and log out; the proof must also establish that retirement is allowed only after the old-session expiry boundary. If immediate old-key retirement is required instead, that is a larger alternative: extend the existing rewrap owner and its compare-and-swap inventory to every live browser-session envelope and prove the same mixed-replica race safety before changing the runbook gate.

## Verification assessment

The candidate records successful focused UI journeys, the unfiltered identity journey, UI tests/checks, Rust family tests, formatting, lints, and code generation in the task packet. Per assignment, this review did not run Cargo-backed lanes. Static inspection confirms that `production-auth.integration.test.ts` uses real HTTP, two production Node BFF processes, a real Wyrd test server, Postgres, and Keycloak, and covers concurrent renewal, cross-replica completion/use, deactivation cutoff, forged/cross-tenant cookies, CSRF, logout, settings authorization, and OIDC-off entry.

Residual proof gaps are server/BFF restart during an in-flight completion or renewal, Postgres outage and recovery, mixed old/new BFF service-key rollout, and sealing-key rollover with a pre-existing browser session. The last gap accompanies `SYSTEM-001`; the others have source-supported bounded behavior but no direct recovery journey in this candidate.

## Overall result

**FAIL**

The runtime session coordination is bounded and recovers safely for ordinary replica failure, dependency outage, and concurrent refresh/logout. The production rotation procedure, however, became incorrect when the candidate added long-lived sealed browser-session fields without adding them to the retirement gate or extending the documented retention window.
