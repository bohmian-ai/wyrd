# OAuth/OIDC and Browser Security Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `0b8919fff090d4b0109a506711cb119214d231f2`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Prior-finding direction: `changes/active/oidc-production-readiness/review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md`

The candidate resolved to the supplied immutable identity before and after
source inspection. This review did not inspect another reviewer's report and
did not modify the subject.

## Reviewed boundary

This pass is limited to the BFF's OAuth/OIDC and browser-security boundary:
authorization-code initiation and callback, state and PKCE, confidential-client
authentication, refresh and revocation, API-key recovery exchange, encrypted
browser cookies, access-token caching, CSRF, tenant binding, browser-visible
errors and data, dependency selection, and the real production journey.

| Boundary | Authority and source coverage | Assessment |
|---|---|---|
| OAuth client and grant processing | RFC 6749 §§1.5, 2.3.1, 4.1, 6; RFC 7636 §§4.3–4.6; RFC 7009 §2; `openid-client` 6.8.8 documentation/source; `browser-sessions.ts:130-161,186-230,281-313,338-351` | Library-owned discovery, client authentication, authorization response processing, state/PKCE validation, refresh, and revocation are used directly. One token-opacity drift is recorded below. |
| Browser cookie confidentiality and integrity | REQ-009; `jose` 6.2.12 documented `EncryptJWT`/`jwtDecrypt`; `browser-sessions.ts:31,117-123,169-184,246-264,315-336` | HKDF-SHA256 gives domain separation from the confidential-client secret; `dir` plus `A256GCM` provides authenticated encryption; cookies are host-only, Secure, HttpOnly, SameSite=Lax, and path `/`. Forged or crossed cookies fail closed. |
| Login CSRF and callback correlation | REQ-006, REQ-009, INV-001; SvelteKit built-in origin checking; `browser-sessions.ts:186-230`; `login/+page.server.ts:25-36`; `login/callback/+server.ts:5-12`; `svelte.config.js:5-10` | Random library state and S256 PKCE are sealed into the short-lived login cookie and passed back to `authorizationCodeGrant`. SameSite=Lax plus SvelteKit's enabled CSRF protection covers state-changing form actions. No duplicate nonce or ID-token/JWKS check belongs in this OAuth client: Wyrd, not the upstream IdP, is this BFF's authorization server. |
| Tenant and authorization authority | REQ-010 lead decision, REQ-015, INV-001, INV-005; `browser-sessions.ts:232-264,315-377`; `hooks.server.ts:10-31`; `settings/+page.server.ts:27-64`; `production-auth.integration.test.ts:393-417,419-460,535-545,548-612` | Cookie tenant equality prevents moving an SSO session between tenant cookie names. For API-key recovery, the route tenant remains presentation context while the exchanged access token supplies the server-enforced tenant and permission authority. The journey proves a different tenant's key cannot read or mutate the route tenant. No new endpoint or claim is warranted. |
| Token exposure and error handling | REQ-005, REQ-009, AC-004; `BrowserSession.#token` and `api` at `browser-sessions.ts:40-93`; generic callback failure at `browser-sessions.ts:213-229`; `hooks.server.ts:53-56`; `problem.ts`; `production-auth.integration.test.ts:100-115,329-345,347-417,481-529` | Access material remains in server-only private state or authenticated encrypted cookies. Callback errors are reduced to a generic sign-in failure. The journey checks pages, SvelteKit data responses, redirects, cookie attributes, clear JWT patterns, and recovery-key sentinels. No secret logging was found in the cumulative diff. |
| Logout and connection lifecycle | REQ-009, REQ-016, approved expected bearer behavior; `browser-sessions.ts:295-313,338-351`; `production-auth.integration.test.ts:433-460,687-717` | Logout removes this replica's cache entry, revokes only the session refresh token through `tokenRevocation`, and clears the cookie. Another login remains renewable. Already issued self-contained access tokens keep their ordinary bounded validity; that approved behavior is not a gap. Old-connection refresh fails after the access-token window. |
| Dependency and transport behavior | `package.json:32-38`; `pnpm-lock.yaml` entries for `openid-client` 6.8.8, `oauth4webapi` 3.8.8, and `jose` 6.2.12; `browser-sessions.ts:130-161`; `upstream.ts:7-27` | Exact runtime versions and integrity hashes are locked. `openid-client` supplies `redirect: manual` and timeout signals to the custom fetch; the adapter preserves them while routing only the configured public-origin endpoints to the deployment-controlled internal server. HTTPS is required except for literal loopback development/test targets. The added dependency cone produced no audit advisory; current `pnpm audit --prod` findings are on the unchanged pre-existing `devalue` dependency, not the added packages. |
| Deployment-controlled public origin | REQ-006; SvelteKit adapter-node `ORIGIN`; `identity_ui_e2e.rs:106-152`; every `url.origin` use in `browser-sessions.ts` | The production adapter's conventional `ORIGIN` setting fixes `event.url.origin`; the real journey starts both replicas with that setting. No extra origin setting or duplicate host check is required. A deployment that exposes adapter-node without its documented `ORIGIN` or trusted-proxy configuration is an operator deployment error, not a candidate-specific mechanism to add. |

## Verification evidence and limits

The task records successful execution of the focused production UI identity
journey, UI type check, full UI Vitest lane, Rust format, and Rust lints. Source
inspection confirms that the real journey starts two production `node build`
BFF replicas with one deployment `ORIGIN` and shared client secret, routes them
to two real Wyrd replicas, and exercises Keycloak and Dex over HTTP rather than
mocking the BFF or server. Its reachable security assertions cover:

- S256 PKCE, state, exact redirect URI, one-use authorization code, and a
  mismatched login cookie;
- successful cross-replica refresh and refusal of forged and cross-tenant
  cookies;
- same-origin enforcement for SSO-adjacent mutations and API-key recovery;
- denied tenant administration for a reader;
- encrypted host-only Secure/HttpOnly/SameSite=Lax cookies and absence of
  clear tokens or recovery API keys in pages, data, redirects, and ordinary
  response headers;
- per-login revocation, preservation of a sibling login, and the approved
  access-token-until-expiry behavior;
- provider replacement and refusal to renew a retired connection; and
- SSO as the routine action with API-key sign-in on the separate recovery
  page, including server-enforced authority when the key belongs to another
  tenant.

The task correctly reserves the unfiltered identity sweep for change review.
This domain review did not rerun that broad journey. `pnpm audit --prod` was
run as a read-only dependency check; it reported advisories only through the
unchanged base dependency `devalue@5.8.1`, so those advisories are not a
TASK-011 finding. There is no browser automation proving user-agent cookie
storage limits; the fixed Wyrd credentials exercised by the real journey are
well within those limits.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

No optional defense-in-depth recommendation is proposed. Requiring a second
JWT/JWKS verifier, nonce, token profile, CSRF token, origin allowlist, or
server-side browser session would duplicate library/platform behavior or
contradict the approved task.

### Positive Controls

- `openid-client` owns the protocol exchanges and response checks; the BFF
  does not hand-write token requests.
- State and the PKCE verifier are generated by the library and carried only
  in an authenticated, short-lived, HttpOnly cookie.
- Session credentials use authenticated encryption and never enter page data,
  URLs, JavaScript, telemetry, or user-facing error detail in the inspected
  paths.
- Authorization stays at Wyrd: decoded claims gate presentation only, while
  the server verifies every bearer and enforces its tenant and permission
  authority.
- SvelteKit's conventional CSRF control and SameSite=Lax cookies are both
  active and have a real cross-site POST refusal test.
- Logout revokes one refresh token and does not pretend that RFC 7009 revokes
  an already issued self-contained access token.

## Material proposed findings

### OIDC-SEC-001 — DRIFT: the BFF interprets OAuth credentials to invent cookie lifetimes

- **Violated obligation:** TASK-011 requires the BFF to use `openid-client`
  as documented and avoid custom protocol mechanics. The standing direction
  requires deletion of mechanisms, settings, and options that neither the
  applicable standards nor conventional implementations require. RFC 6749
  §1.5 defines the refresh token as a credential string that is usually opaque
  to the client; `openid-client.refreshTokenGrant` likewise accepts and
  forwards the string without interpreting it.
- **Exact location:**
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:13-14,246-263`.
- **Evidence:** `establish` calls `decodeJwt(credential)` when the credential
  is a refresh token and treats an `exp` claim as a client contract. For an
  API key it instead applies the locally invented `apiKeyLifetimeSeconds = 12
  * 60 * 60`. Both values are then duplicated into a JWE `exp` and cookie
  `Max-Age`. Neither lifetime comes from `openid-client`, an OAuth response
  field governing refresh-token expiry, or approved Wyrd authority. The server
  already owns both credentials' validity and refuses refresh or exchange
  after expiry or revocation; `read` already clears the cookie on refusal.
- **Observable consequence:** The current JWT-shaped Wyrd refresh token works,
  but the BFF is not behaving as a standard OAuth client at this boundary. A
  standards-conforming opaque refresh token would make otherwise successful
  login fail inside `decodeJwt`; independently, the recovery browser session
  is shortened by an unexplained 12-hour client policy even when the server's
  credential remains valid. The extra client-side expiry can therefore reject
  valid server authority and makes a private Wyrd refresh-token encoding part
  of the browser contract.
- **Required testable correction:** Delete client inspection of the refresh
  token and the arbitrary API-key lifetime. Treat both credentials as opaque
  values and use an ordinary host-only encrypted browser session cookie; let
  Wyrd's existing refresh and token-exchange endpoints remain the sole expiry
  and revocation authority. Preserve authenticated encryption, HttpOnly,
  Secure, SameSite=Lax, state/PKCE, per-login revocation, and cache behavior.
  Focused proof must show that a sealed session no longer depends on a JWT
  refresh-token payload or a local 12-hour setting, and that server refusal
  still clears it. Do not add a new endpoint, claim, configuration setting,
  token parser, introspection call, or server-side session store.

This is blocking under the task-review rule that deletion of unearned code is
material even when current happy-path behavior remains green.

## Open questions

None. The approved decisions resolve the SSO/recovery presentation and
API-key tenant authority. The approved access-token-after-logout window is
ordinary bearer-token behavior and is not a finding.

## Overall result

**FAIL**

The OAuth/OIDC and browser-security boundary is otherwise coherent and has
credible real-system proof, but `OIDC-SEC-001` is standards drift in the
credential-lifetime owner and must be removed before this domain can pass.
