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
`mise exec -- env WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER=production_ui_bff_journey mise run test:identity:journey`.

Then run: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check`,
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test`,
`mise run test:identity:journey` unfiltered (targets `server`, `ui`, `cli`,
`rust`, `client`, `python`, `typescript`), `mise run test:shared`,
`mise run test:wyrd-sdk`, `mise run test:cli:journey`,
`mise run test:principals:integration`, `mise run py:test:integration`,
`mise run ts:test:integration`, `mise run test:wyrd`,
`mise run codegen:check`, `mise run docs:check`, `mise run fmt`,
`mise run lints`, `mise run py:format`, `mise run py:lints`,
`mise run py:test:unit`, `mise run py:typecheck`, `mise run ts:test:unit`,
`mise run ts:typecheck`, `mise run ts:napi:check`, and the boundary checks
`mise run check:client-tier`, `mise run check:pyo3-scope`,
`mise run check:unwrap-audit`, and `mise run check:workspace-hack`.

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
