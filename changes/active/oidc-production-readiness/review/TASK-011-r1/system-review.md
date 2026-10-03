# System-resilience review — TASK-011

## Immutable subject

- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `0b8919fff090d4b0109a506711cb119214d231f2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Prior closure obligation: `FIND-TASK-010-1`

The candidate remained the repository `HEAD` throughout this review. I reviewed the complete base-to-candidate diff and the runtime owners and callers described below. I did not read another reviewer's report.

## Deployment and runtime path

The deployed path is:

`browser -> load balancer -> any SvelteKit BFF replica -> that replica's configured Wyrd server endpoint -> any Wyrd server replica / Postgres`, with the Wyrd authorization server federating to the tenant IdP only during interactive human authorization.

The candidate implements the BFF portion as follows:

- `BrowserSessions.begin` discovers Wyrd's RFC 8414 metadata through `openid-client`, creates standard state and S256 PKCE material, seals it in the short-lived `wyrd_login` JWE cookie, and sends the browser to Wyrd's authorization endpoint.
- `BrowserSessions.complete` redeems the returned code through `authorizationCodeGrant` and seals the returned non-rotating refresh token in a tenant session JWE.
- Every BFF derives the JWE key from the shared `WYRD_UI_CLIENT_SECRET`, so a cookie minted by one replica is readable by another. There is no BFF-side durable session store.
- Each BFF caches a Wyrd access token only in process memory under a SHA-256 digest of the refresh token or API key. A process restart, another replica, expiry, or eviction produces a cache miss and runs `refreshTokenGrant` or the RFC 8693 `genericGrantRequest` again.
- `BrowserSession.api` sends the access token only on the BFF-to-Wyrd request. The browser-visible projection contains principal and tenant metadata, not the token.
- OIDC-off recovery uses the separate API-key page and the standard token-exchange grant. The access token, not the route tenant key, remains the server-enforced tenant authority.
- Tenant switching reads the target tenant's independently sealed cookie and renews it before opening that tenant; absence or definite refusal returns to that tenant's login.
- Logout uses `openid-client.tokenRevocation` for refresh-token sessions. API-key logout clears only the browser session and never revokes the underlying API key.
- `identity_ui_e2e.rs` starts two BFF processes, each connected to a different Wyrd server replica over the shared database and public origin. Its filtered production UI journey covers code redemption on one BFF followed by renewal and use on the other.

## Failure and recovery assessment

| Failure or transition | Source evidence | System behavior and recovery assessment |
|---|---|---|
| BFF process restart, rolling BFF replacement, or request routed to a cold replica | `browser-sessions.ts:100-103, 270-278, 295-312`; `identity_ui_e2e.rs:106-152`; `production-auth.integration.test.ts:347-451` | The JWE survives the process and the process-local cache does not need to. A cold replica renews from the cookie's credential. Non-rotating confidential-client refresh tokens avoid a cross-replica rotation race. This path is correctly isolated to the affected request and does not require replica coordination. |
| Wyrd replica replacement | `browser-sessions.ts:125-160`; `identity_ui_e2e.rs:1-17` | Discovery and grant requests are routed from the public issuer to the configured internal Wyrd endpoint. Auth state is in Postgres/server signing authority, not BFF memory. The two-replica journey is credible evidence for replica interchangeability, although it does not kill and restart a replica mid-request. |
| Wyrd network timeout or connection failure during discovery, refresh, exchange, or revoke | `browser-sessions.ts:130-160, 281-312, 338-350`; installed `openid-client` 6.8.8 default 30-second request signal | `openid-client` supplies a bounded request signal. Network exceptions surface as the canonical upstream failure. Refresh/exchange network exceptions leave the cookie intact, but revocation currently clears the cookie first. The latter loses the only retry authority; see `SYSTEM-011-1`. |
| Wyrd returns an OAuth `500 server_error` or `503 temporarily_unavailable` | Wyrd `components/auth/routes.rs:80-105`, `auth/oauth.rs:60-70, 100-120`, and `auth/cli_login.rs:265-307`; BFF `browser-sessions.ts:281-312, 326-333` | Wyrd deliberately distinguishes transient server/store/audit failures from an unusable grant. The BFF collapses every `ResponseBodyError` into a terminal credential refusal and clears the cookie. A cold replica or an access-token expiry during a brief outage therefore turns a recoverable outage into permanent sign-out. See `SYSTEM-011-1`. |
| IdP outage | `BrowserSessions.begin` and `complete`; the server-owned authorization flow; task journey evidence for SSO and provider replacement | New interactive login fails closed and must be restarted. Existing BFF refresh is against Wyrd and does not revisit the IdP, so an established session continues while its Wyrd refresh authority is valid. No BFF process failure or unrelated tenant outage follows. |
| Malformed, expired, replayed, or cross-tenant cookie | `browser-sessions.ts:169-184, 315-335, 359-376`; `production-auth.integration.test.ts:347-403, 556-610` | JOSE authentication/expiry failure returns no session and clears only the affected cookie. The embedded tenant must equal the selected cookie tenant. Authorization-code replay yields no session. This fails closed without taking the process down. |
| Tenant connection replacement/deactivation/removal | `browser-sessions.ts:295-312`; `production-auth.integration.test.ts:614-715`; REQ-016 | A cached access token retains its bounded snapshot authority until expiry, as approved. A cold replica may attempt renewal earlier and be refused. A definite refresh refusal clears that tenant's cookie. The replacement session and other tenants remain live. |
| Logout and cookie replay | `browser-sessions.ts:338-350`; `production-auth.integration.test.ts:433-451` | Successful revocation removes that refresh authority and clears the local cache/cookie; the other login remains usable. A replay on another replica can continue only under an already cached self-contained access token until its normal expiry, which is the explicitly approved standard bearer-token behavior. Revocation failure is not safely recoverable because of the current ordering; see `SYSTEM-011-1`. |
| Direct API call outage | `BrowserSession.api`, `browser-sessions.ts:77-92`; route callers such as `settings/+page.server.ts:27-47` | The call has a 30-second bound and maps transport failure to the canonical upstream problem. It does not clear the renewable cookie or crash the BFF, so a later request can recover. |
| Missing BFF secret or invalid internal server URL | `browser-sessions.ts:110-123`; `upstream.ts:7-26` | The affected auth request fails closed before sending a credential. The process itself starts lazily and the current TCP startup probe does not validate this configuration. The approved task does not require a new readiness protocol, and adding one would be outside this task; this is therefore a verification/deployment limit, not a finding. |

## Affected capabilities

`SYSTEM-011-1` affects production SSO sessions and OIDC-off API-key recovery sessions whenever a replica must renew or exchange during a Wyrd store/audit outage. Its logout portion affects refresh-token sessions when the revocation endpoint cannot commit or cannot be reached. It does not affect local development identity, provider connection persistence, CLI/SDK credential handling, unrelated Wyrd APIs, or the approved validity of a self-contained access token until expiry.

## Material proposed finding

### SYSTEM-011-1 — Transient OAuth failures destroy the browser's only recovery authority

- **Classification:** `REGRESSION`
- **Violated obligation / boundary:** REQ-009 makes the encrypted cookie the BFF's portable renewable session authority; REQ-016 makes a definite refusal of an old connection terminal but does not make a server/store outage terminal. The deployed service must recover after an interrupted Wyrd dependency without converting that interruption into permanent session loss. RFC-style `server_error` and `temporarily_unavailable` are explicitly produced by Wyrd for this distinction.
- **Locations:**
  - `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:281-312`
  - `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:326-333`
  - `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:338-350`
  - Producer evidence: `crates/wyrd/wyrd-server/src/components/auth/routes.rs:80-105`, `crates/wyrd/wyrd-server/src/auth/oauth.rs:60-70,100-120`, and `crates/wyrd/wyrd-server/src/auth/cli_login.rs:265-307`.
- **Evidence:** `exchange` returns `null` for every `client.ResponseBodyError`; `access` does the same for every refresh `ResponseBodyError`; `read` interprets that `null` as a dead session and deletes the cookie. The Wyrd token endpoint can return `500 server_error` or `503 temporarily_unavailable` when issuance, storage, or audit fails, so both refresh and API-key exchange take this deletion path for a transient dependency failure. Separately, `logout` deletes the cookie and cache before awaiting RFC 7009 revocation. The revocation endpoint can return `503 temporarily_unavailable` when its store or audit transaction fails; after the action reports failure, the browser no longer holds the credential needed to retry the revocation.
- **Observable system consequence:** During a brief Wyrd outage, sessions on warm BFF replicas continue only until their cached access token expires, while sessions on a restarted/cold replica are immediately and permanently signed out even though their refresh token or API key is still valid. During a failed logout, the UI loses the cookie while the refresh token may remain renewable, so the user cannot retry revocation but a copied/replayed cookie can. A rolling BFF replacement during the outage amplifies the loss because every new process has an empty cache.
- **Smallest testable correction:** Keep `openid-client` as the sole protocol implementation and use the standard OAuth error already exposed by `ResponseBodyError`. For refresh, only a terminal unusable-grant response such as `invalid_grant` should return no session; for Wyrd's RFC 8693 API-key exchange, only the server's terminal unusable-subject response should do so. `server_error`, `temporarily_unavailable`, client/configuration failures, timeouts, and transport failures must surface as the canonical upstream failure while leaving the existing cookie intact. For a refresh-token logout, complete `tokenRevocation` successfully before deleting that cookie and cache; RFC 7009 already makes an unknown or previously revoked token a successful idempotent response, so no new retry store, protocol, setting, or custom mechanism is needed. API-key logout may continue to clear locally without revocation.
- **Focused proof:** In the narrow BFF test lane, drive an existing sealed refresh session and an existing sealed API-key session through a token endpoint response of `503 {"error":"temporarily_unavailable"}`. Assert that the request reports upstream unavailability, sends no replacement/clearing session cookie, and succeeds with the same cookie when the endpoint recovers. Drive refresh-token logout through the same 503 and assert the cookie remains; retry against a successful RFC 7009 response, assert the cookie is then cleared, and prove the old refresh token no longer renews. Existing terminal-refusal tests must continue to prove that `invalid_grant`/the unusable API-key response clears only the affected session.

This correction reuses standard `openid-client` errors and RFC 7009 behavior. It requires no retry loop, shared cache, server-side session state, health mechanism, new setting, or custom OAuth behavior.

## Verification and proof assessment

The task records successful execution of the exact filtered `production_ui_bff_journey` (four UI journey tests through the real BFF/server topology), the two named focused Vitest tests, the UI `check` and full UI test lane, Rust format and lints, and `git diff --check`. The journey materially proves successful cross-replica cookie use, cold-replica refresh, provider replacement, tenant switching, API-key recovery, normal logout, and the expected access-token-after-logout window.

Per the task-review direction, I did not require or rerun the unfiltered identity journeys or broader change-level sweeps; those belong to change review. The recorded narrow evidence does not exercise token-endpoint `server_error`/`temporarily_unavailable`, a transport failure during logout, or recovery with the same cookie after the dependency returns. Healthy-path and terminal-refusal evidence therefore cannot close `SYSTEM-011-1`.

## Overall result

**FAIL**

The normal two-BFF/two-Wyrd topology and the approved access-token/logout semantics are implemented coherently, but one reachable Wyrd outage converts transient failure into irreversible browser-session loss and can discard the user's only means to retry an uncommitted revocation.
