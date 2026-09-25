---
id: TASK-003
kind: implementation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-003, REQ-006, REQ-009, REQ-010, REQ-015, REQ-016, REQ-018, INV-001, INV-003, INV-005, AC-001, AC-002, AC-003, AC-006, AC-007, AC-009]
depends_on: [TASK-002]
---

# Production UI login and tenant session

## Outcome and Value

The SvelteKit BFF replaces development-only identity in production: people sign in through tenant SSO, use authorized UI actions across replicas, switch tenants safely, and log out without browser-visible Wyrd tokens.

## Owners, Scope, Consumers, and Prohibited Changes

The existing Wyrd SvelteKit BFF owns cookies, CSRF, browser session lifecycle, and server-side calls as the signed-in tenant principal. The Rust server remains the identity and permission authority. Project the TASK-001 management contract in tenant settings. Preserve the development mock only behind its existing local boundary. Do not introduce a second role mapper, local password database, client-side bearer storage, or a tenant selector based on untrusted browser data.

## Approach

1. Connect the tenant login route and provider callback to the server-owned state and authority.
2. Establish a Secure, HttpOnly, SameSite browser session available across serving replicas; expose only safe metadata to pages.
3. Bind BFF actions to current tenant, CSRF, expiry, and Wyrd permissions; implement logout and tenant switching.
4. Project connection administration in tenant settings and a non-SSO authorized-credential entry for OIDC-off self-hosted deployments.

## Packet-local browser session contract

The BFF owns cookie issuance, CSRF, route guards, logout, and server-side
Wyrd calls. The Rust server owns one Postgres-backed browser-session record
and the Wyrd credential it references; this is storage for the BFF session,
not a new identity issuer. Internal session operations use a deployment-wide
BFF service key over TLS on a private, non-public server route. Deployments
provision it only when production UI is enabled, inject the same secret into
BFF replicas and the server, and rotate it with two accepted key hashes during
a bounded overlap; the old key is removed after all replicas switch. It
authorizes only browser-session operations, never tenant API authority.
Those operations derive tenant from verified flow/session state, never from
the service key or a caller-supplied tenant. Their typed semantics are:

```text
CreateBrowserSession { flow_id_from_http_only_cookie, csrf_binding }
BrowserSession { id_hash, tenant_id, principal_id, connection_id?,
  mode: OidcRefresh|ApiKeyExchange, access_expires_at,
  refresh_expires_at?, absolute_expires_at, csrf_hash, csrf_token_enc,
  revoked_at }
SessionRead { tenant_id, principal_id, safe_role_summary, expires_at,
  csrf_token: Secret }
SessionAuthority { access_token: Secret, access_expires_at } // BFF server only
```

Before leaving for the IdP, the BFF sets a short-lived random HttpOnly flow
cookie and sends only its hash to TASK-002's `BeginLogin`; TASK-002 binds that
hash into state. After callback verification
and Wyrd issuance, the server stores the token pair against that flow and
redirects to a fixed BFF completion route without query data. That route
redeems the flow once using its cookie and BFF service credential. In the
same transaction the server consumes the flow, creates the session, hashes
its own random 256-bit session ID for storage, and returns the raw ID once
over the private channel to the BFF. The BFF sets exactly that ID as an
opaque session cookie (`Secure`, `HttpOnly`, `SameSite=Lax`, host-only, path `/`, bounded
expiry), clears the flow cookie, and redirects to the tenant page without
carrying a grant or tokens.
The session ID is stored only as a hash in Postgres; token pair is encrypted
at rest. Invalid/expired/replayed flow sets no session cookie. Both BFF replicas
resolve the same record through the server, with no process-local production
session authority. On creation the BFF sends a random session CSRF token only
over the private channel; the server stores its hash and encrypted value.
Every replica retrieves the raw token through `SessionRead` to render existing
form fields; the BFF checks submitted token against the stored hash in
constant time. The CSRF token may appear in form data but is never a Wyrd
credential and cannot select a tenant.

On each protected request, the BFF reads the session record, checks path
tenant against the server-returned tenant, and obtains short-lived Wyrd
authority through its authenticated server channel; it then calls normal
Wyrd APIs as that User. The BFF never returns the token in page data, logs,
URL, or browser storage. Mutating actions require a session-bound CSRF token
and same-origin check before the API call; the Wyrd server still enforces
permissions. One transaction locks the session for refresh and updates its
rotated credential; a concurrent replica rereads the winner's record. A
failed/revoked/old-connection refresh ends the session at access expiry,
never falls back to a different tenant or provider. Logout revokes that
browser session and its refresh family, clears the cookie, and is idempotent.
Tenant switch resolves an independent session for the target tenant or starts
its login; it cannot rebind the current session ID. For OIDC-off self-hosted
entry, the user POSTs an existing tenant API key to the BFF over TLS. The BFF
passes it once over the private channel; the server validates its tenant and
authority through existing API-key exchange, then stores the bootstrap key
encrypted in `ApiKeyExchange` mode with an absolute session limit of eight
hours. API-key exchange issues no refresh token: at each access expiry the
server re-exchanges the stored key under the session lock, rechecks tenant
and current grants, and audits issuance. A revoked/expired key ends this UI
session; no alternate credential is tried. Logout deletes the stored key
and session, but does not revoke the underlying operator API key. OIDC mode
stores a refresh pair and revokes that family on logout. No password or mock
flag is accepted in production.

## Ordered Implementation Scenarios

### Scenario 1 — Production SSO session

**Behavior.** The tenant login page offers SSO only when active; callback establishes a session on the correct tenant. A second replica can use it. Browser URL, page data, JavaScript storage, and callback response expose no Wyrd bearer or refresh token.

**RED.** Add a BFF route/action test and real-server browser journey; observe the current production mock-auth refusal or token/session mismatch.

**GREEN.** Connect login and shared session authority; rerun both tests.

**REFACTOR.** Reuse the existing session and Wyrd request boundaries; keep mock setup local only.

### Scenario 2 — Action, switch, and logout safety

**Behavior.** Forged/expired session, missing CSRF, wrong tenant, missing permission, and inactive provider renewal are refused. Switching to another tenant revalidates membership and requires its own login when needed; logout ends this browser session.

**RED.** Extend BFF action and two-tenant browser journeys with negative cases; observe a session or action accepted under the wrong tenant.

**GREEN.** Enforce server authority and BFF session rules; rerun prior scenarios.

**REFACTOR.** Keep one browser session boundary and one tenant binding path.

### Scenario 3 — OIDC-off and tenant settings

**Behavior.** Without OIDC, an authorized Wyrd credential reaches a real self-hosted UI; no IdP or mock flag is needed. Tenant administrators can view redacted settings and perform permitted connection actions; other tenants cannot.

**RED.** Add an OIDC-off UI journey and tenant settings permission tests; observe missing entry or settings projection.

**GREEN.** Add only the credential exchange/session path and settings projection; rerun all scenarios.

**REFACTOR.** Keep the BFF as a projection of server contracts.

## Acceptance Criteria

AC-001/002/003/006/007 browser journeys pass. Session authority survives replica routing but never enters browser-visible data. Deactivated connection sessions stop renewing, with existing access bounded by five minutes. No production path depends on mock auth.

## Expected Write Set and Consumer Closure

Likely wyrd-ui routes, hooks, server auth/session and Wyrd client modules, components, UI tests, and the identity journey lane in mise.toml. TASK-001 owns the management API and TASK-002 owns login authority; this task consumes them.

## Verification and Evidence

Run each newly named UI test through its exact focused mise exec -- pnpm command after naming it. Extend the repository-managed mise run test:identity:journey lane to exercise the BFF over HTTP against the real server and provider, then run that lane. Also run mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test and mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check. If server or contracts change, run mise run codegen:check, mise run fmt, and mise run lints. Document safe browser data, CSRF, replica, OIDC-off, and tenant-switch evidence.

Add `src/lib/server/auth/session.test.ts` selectors `production session
rejects cross-tenant and missing CSRF` and `production session expires and
logs out` (Vitest unit, mocked server response) and run each with
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run
src/lib/server/auth/session.test.ts -t 'production session rejects
cross-tenant and missing CSRF'` (replace `-t` for the second). Add real HTTP
BFF journeys `production SSO crosses replicas` and `OIDC-off credential UI`
in `src/lib/server/auth/production-auth.integration.test.ts`, asserting
cookie attributes, safe page/URL
data, allowed/denied actions, expired/forged/CSRF rejection, switch/logout,
and tenant settings authority (AC-001/002/003/007/009). Extend
`test:identity:journey:inner` to start two BFF server processes against the
same Wyrd server/Postgres and run
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run
src/lib/server/auth/production-auth.integration.test.ts`; assert this target's tests are
listed before relying on the lane. Retain existing Rust identity selectors.
The lane's Postgres/IdP setup and public-origin config must be shared by both
BFF processes; a Vitest mock cannot substitute for this HTTP journey.
Extend the focused identity wrapper with `WYRD_IDENTITY_TARGET=ui` and
`WYRD_IDENTITY_FILTER=<exact Vitest test name>`; it must start the same real
server, two BFF processes, Postgres, and providers, check one selected test,
then run the inner Vitest command. From a clean checkout, run
`mise exec -- env WYRD_IDENTITY_TARGET=ui
WYRD_IDENTITY_FILTER='production SSO crosses replicas' mise run
test:identity:journey` and repeat with `OIDC-off credential UI`. The
unfiltered lane executes both and all earlier server journeys.

## Material Stop Conditions

Stop if UI correctness requires browser-held Wyrd tokens, UI-owned role mapping, a new local password authority, or relaxed tenant/CSRF/session checks.

## Authority Links

[Approved spec](../spec.md); [AGENTS.md](../../../../AGENTS.md); [Wyrd doctrine](../../../../architecture/wyrd-doctrine.mdx); [security posture](../../../../architecture/wyrd-security-posture.md).
