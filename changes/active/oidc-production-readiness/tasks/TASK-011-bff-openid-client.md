---
id: TASK-011
kind: implementation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
requirements: [REQ-006, REQ-009, REQ-010, REQ-015, REQ-016, INV-001, INV-005, AC-001, AC-002, AC-003, AC-007]
depends_on: [TASK-010]
---

# BFF on `openid-client` with an encrypted cookie session

## Outcome and Value

The SvelteKit BFF signs people in as a standard confidential OAuth client of
Wyrd, keeps the session in one encrypted HttpOnly cookie that any BFF replica
can read, and logs out by revoking only that login's refresh token. The
custom session protocol, flow cookie, and CSRF token are deleted. This is
report item T3 in
[`research/auth-standards-recommendation.md`](../research/auth-standards-recommendation.md).

## Owners, Scope, Consumers, and Prohibited Changes

Owner: the BFF under `crates/wyrd/wyrd-server/wyrd-ui` (server hooks, login,
callback, logout, tenant switch, OIDC-off login). Consumer: every UI route
that calls Wyrd as the signed-in principal.

Libraries and standards, exactly:

- `openid-client` 6.8.8 for discovery from Wyrd's RFC 8414 metadata, the
  authorization code grant with PKCE (RFC 6749 §4.1, RFC 7636 S256), refresh
  (RFC 6749 §6), and revocation (RFC 7009 §2), authenticating as `wyrd-ui`
  with `client_secret_basic` (RFC 6749 §2.3.1).
- `jose` (an `openid-client` dependency) to encrypt the Secure, HttpOnly,
  SameSite=Lax session cookie. The cookie holds the Wyrd refresh token (or,
  OIDC-off, the operator API key; REQ-010) plus safe metadata.
- Access tokens are cached in BFF memory keyed by the refresh-token hash; a
  cache miss runs a refresh. Refresh tokens for `wyrd-ui` do not rotate
  (TASK-010), so replicas and tabs need no coordination.
- CSRF uses SvelteKit's built-in `csrf.checkOrigin` plus `SameSite=Lax`.

Delete: the protocol code in `server-sessions.ts`, the flow-cookie code, the
custom CSRF token, and the BFF side of `/login/complete`.

Prohibited: tokens in page data, URLs, or JavaScript; a server-side session
store; a second role mapper; tenant selection from untrusted browser data;
`@auth/sveltekit`, `tower-sessions`, or any hand-written OAuth call.

## Approach

1. Configure `openid-client` from Wyrd's metadata as the `wyrd-ui` client.
2. Implement login, callback, and the encrypted cookie; cache access tokens
   in memory and refresh on miss.
3. Implement tenant switch, OIDC-off API-key login, and logout = RFC 7009
   revoke + cookie clear.
4. Delete the replaced session, flow-cookie, and CSRF code.

## Ordered Implementation Scenarios

### Scenario 1 — Sign in and use the UI across replicas

**Behavior.** A tenant user signs in through the UI, makes an authorized call
and is refused a denied one; two BFF replicas in front of two Wyrd replicas
serve the same session from the cookie (REQ-009, AC-002, AC-003, AC-007).

**RED.** `production_ui_bff_journey` fails because the private BFF channel it
used was deleted in TASK-010.

**GREEN.** Implement the `openid-client` login, callback, encrypted cookie,
and in-memory access-token cache until the journey passes on both replicas.

**REFACTOR.** Delete `server-sessions.ts` protocol code and the flow cookie.

### Scenario 2 — No token reaches the browser

**Behavior.** Page data, URLs, and JavaScript contain no Wyrd or provider
token; only the encrypted cookie and safe metadata reach the browser
(REQ-009, AC-004).

**RED.** A journey assertion over rendered page data, redirect URLs, and the
cookie value fails if any token appears in clear form.

**GREEN.** Expose only safe metadata to load functions. Rerun Scenario 1.

**REFACTOR.** One place builds browser-visible session metadata.

### Scenario 3 — Tenant switch, OIDC-off login, and old-connection refusal

**Behavior.** Tenant switch revalidates membership and prompts for that
tenant's login (REQ-015); an OIDC-off deployment signs in with an operator
API key (REQ-010, AC-001); a session from a replaced connection cannot
refresh once its access token expires (REQ-016, AC-007).

**RED.** Journeys for each case fail against the Scenario 1 build.

**GREEN.** Implement each through the same cookie and client. Rerun
Scenarios 1–2.

**REFACTOR.** Reuse one refresh-on-miss path.

### Scenario 4 — Logout revokes only this login

**Behavior.** Logout revokes this cookie's refresh token through RFC 7009 and
clears the cookie; another login for the same User keeps working
(FIND-TASK-003-18; REQ-009).

**RED.** A journey with two sessions for one User shows the second session
fails after the first logs out, or the first can still refresh.

**GREEN.** Revoke through `openid-client` and clear the cookie. Rerun
Scenarios 1–3.

**REFACTOR.** Remove the custom CSRF token in favor of `csrf.checkOrigin`.

## Acceptance Criteria

- UI journeys AC-002 and AC-003 pass.
- Two BFF replicas share a session through the cookie.
- No token appears in page data or URLs.
- An old-connection session cannot refresh after replacement (REQ-016).
- Logout revokes only this login (FIND-TASK-003-18 proof).
- OIDC-off API-key login works (AC-001).
- `server-sessions.ts` protocol code, flow-cookie code, custom CSRF, and
  `/login/complete` are gone.

## Expected Write Set and Consumer Closure

`crates/wyrd/wyrd-server/wyrd-ui` server code, routes, `package.json`, the
lockfile, UI tests, `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs`, and
its identity-lane wiring.

## Verification and Evidence

Run every new or changed named test with its exact focused command. The UI
journey runs with
`mise exec -- env WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER=production_ui_bff_journey mise run test:identity:journey`;
UI Vitest tests run by exact file and name with
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run <file> -t '<name>'`.

Then run only the lanes covering this write set:
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check`,
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test`, and, for
`identity_ui_e2e.rs` and its lane wiring, `mise run fmt` and `mise run lints`.
Unfiltered identity journeys and every-language sweeps run once at change
review.

## Material Stop Conditions

Stop and report if a needed behavior has no vetted library and is not covered
by a named RFC section; never write custom protocol logic. Also stop if the
session cannot work across replicas without server-side session state.

## Authority Links

[Approved spec](../spec.md) REQ-009;
[research report](../research/auth-standards-recommendation.md) §3.2 and T3;
[AGENTS.md](../../../../AGENTS.md);
[`wyrd-ui` skill](../../../../.agents/skills/wyrd-ui/SKILL.md);
[RFC 6749](https://www.rfc-editor.org/rfc/rfc6749);
[RFC 7636](https://www.rfc-editor.org/rfc/rfc7636);
[RFC 7009](https://www.rfc-editor.org/rfc/rfc7009);
[RFC 8414](https://www.rfc-editor.org/rfc/rfc8414).

## Implementation Evidence

UI paths are relative to `crates/wyrd/wyrd-server/wyrd-ui/src`. The journey is
`lib/server/auth/production-auth.integration.test.ts`, hosted by
`crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| UI journeys AC-002 and AC-003 pass | `lib/server/auth/browser-sessions.ts` (`BrowserSessions.begin`/`complete`/`read`, openid-client 6.8.8 `discovery`, `buildAuthorizationUrl` + S256 PKCE + state, `authorizationCodeGrant`); `routes/login/callback/+server.ts`; `hooks.server.ts` | Journey tests `production SSO crosses replicas` (sign-in, an authorized admin action, a reader denied an admin action) and `production multi-provider tenant switch` (including mix-up callback refusals) | PASS |
| Two BFF replicas share a session through the cookie, in front of two Wyrd replicas (AC-007) | `identity_ui_e2e.rs` runs BFF 0 against replica A and BFF 1 against replica B (TLS hop); `WyrdTestServer::start_bound_replica` in `crates/wyrd/wyrd-testing/src/server.rs`; jose `dir`/`A256GCM` cookie keyed by HKDF of the client secret | Journey: the code is redeemed at one BFF, and the other BFF serves the session by renewing it with `refreshTokenGrant` | PASS |
| No token appears in page data or URLs | `BrowserSession` keeps the access token in a private `#token` field; `BrowserSessions.metadata` is the single source of browser-visible metadata | Journey `expectNoSecrets` checks pages, `__data.json`, redirects and Set-Cookie. `expectEncrypted` asserts every session and login cookie is a JWE containing no clear JWT. The operator key never appears in responses. | PASS |
| An old-connection session cannot refresh after replacement (REQ-016) | `BrowserSessions.access` renews on cache miss; a refused refresh clears the cookie | Journey test `production provider replacement settings`: after activation the session ends on its next renewal, and replaying the old cookie at either BFF goes to sign-in | PASS |
| Logout revokes only this login (FIND-TASK-003-18) | `BrowserSessions.logout` calls `tokenRevocation(..., {token_type_hint: 'refresh_token'})` and clears the cookie; an operator API key is never revoked | Journey: two logins for one user. The first logs out; replaying its cookie cannot refresh; the second still renews on a BFF that had not cached it. | PASS |
| OIDC-off API-key login works (AC-001) | `routes/t/[tenantKey]/login/api-key/+page.server.ts` uses `genericGrantRequest` with RFC 8693 token exchange | Journey test `OIDC-off credential UI`: a bad key gets 401, a cross-site post gets 403, a reader is denied admin, logout works, an admin stages a candidate | PASS |
| Tenant switch revalidates membership (REQ-015) | `BrowserSessions.switch` reads the target tenant's own cookie and renews it with the server | Journey: switching to a tenant with no session goes to its login; a tenant with a session opens; forged and crossed cookies are cleared | PASS |
| `server-sessions.ts`, the flow cookie, the custom CSRF token and `/login/complete` are gone | `git rm` of `lib/server/auth/server-sessions.ts` and `routes/login/complete/+server.ts`; CSRF fields removed from every form; SvelteKit `csrf.checkOrigin` plus `SameSite=Lax` | `git grep` finds no `WYRD_BFF_SERVICE_KEY`, `internal/bff`, `x-wyrd-bff-key`, `login/complete` or `wyrd_flow` outside `changes/`. The journey gets 403 for cross-site posts from the built server. | PASS |
| FIND-TASK-010-1 closure: openid-client completes code + PKCE, refresh and revoke against the real server | `browser-sessions.ts` | All three steps run in `production SSO crosses replicas` | PASS |

Decisions (team lead, recorded as approved):

1. **REQ-010 recovery sign-in.** Routine sign-in is SSO; "Sign in with SSO" is the login page's only primary action. The operator API-key form is on its own recovery page, `/t/{tenant}/login/api-key`. Both the login page and the sign-in problem page link to it. A tenant without an Active SSO connection gets `access_denied` from `/auth/authorize`, and the BFF renders the problem page.
   - Journey: the OIDC-off tenant's SSO attempt returns to the problem page, which links to recovery.
   - Journey: the SSO-active tenant shows SSO, and its recovery page still signs in with an operator key.
2. **An API-key session's tenant is the key's own tenant.** The server scopes every call by the exchanged token. No server change, mapping endpoint or claim was added.
   - Two earlier assertions were dropped: "an SSO tenant refuses an API key" and "another tenant's key is refused at this page".
   - Replacement assertion: another tenant's key signed in at this tenant's recovery page does not see this tenant's staged connection, and the server refuses to remove it.

Material limit: each BFF keeps a session's cached access token until it expires (access-token TTL). Logout clears the browser cookie at once and revokes the refresh token. A replayed cookie on a replica that still caches the token keeps working until the token expires, the same window any bearer access token has.

Non-goals: no server-side session store, no second role mapper, no `@auth/sveltekit`, and no hand-written OAuth calls. The only change outside the UI, journey host and lane is a test-harness method in `wyrd-testing`.

Commands (all exited 0):

- `CARGO_TARGET_DIR=… mise exec -- env WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER=production_ui_bff_journey mise run test:identity:journey` (4 UI journey tests passed; the host test passed)
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/routing/journey.test.ts -t 'real browser-facing loads and actions contain credentials and isolate tenant tabs'`
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/changes/ChangesJourney.test.ts -t 'incomplete draft saves, resumes, and accepts multiple subjects and requirements'`
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` (0 errors, 0 warnings)
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test` (32 files, 177 tests passed)
- `CARGO_TARGET_DIR=… mise run fmt`
- `CARGO_TARGET_DIR=… mise run lints`
- `git diff --check`
