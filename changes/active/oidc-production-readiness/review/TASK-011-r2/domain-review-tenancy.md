# TASK-011 tenancy and authority-separation domain review

## Immutable subject

- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `4d468b33e49de4dd9df30c5dd046a334569465bf`
- Candidate tree: `87acdce15e3ca6ea2b6016969695db398ab2d598`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Prior review: `changes/active/oidc-production-readiness/review/TASK-011-r1/`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-011-r1/TASK-011-R1-browser-session-standards-and-logout.md`
- Binding reversal: `changes/active/oidc-production-readiness/review/TASK-011-r1/lead-direction-FIND-TASK-011-2.md`

The candidate and tree resolved to the stated objects before this report was
written. The complete base-to-candidate diff, the latest remediation diff, the
original task, the round-one reports and verdict, the remediation task, and the
lead reversal were inspected. `FIND-TASK-011-2` is reversed and was not
reopened.

## Reviewed boundary

This review traced tenant identity and authorization through the production
SvelteKit BFF: route tenant input, tenant-derived cookie selection, the sealed
cookie's `tenant`, API-key exchange, authorization-code completion, access-token
claims, the process-local access cache, `BrowserSession.context()`, hooks,
tenant switching and metadata, settings reads and mutations, logout, sibling
sessions, and the real-server multi-tenant assertions.

The following approved decisions were treated as authority rather than review
questions:

- Routine sign-in is SSO, with API-key sign-in on a separate recovery page.
- An API-key browser session's effective authority is the exchanged token's
  tenant, not the recovery route's tenant key.
- Browser logout always ends the local session. Refresh-token revocation is
  best-effort; a failed revocation does not retain the local session for retry.
- An already issued bearer access token keeps its bounded snapshot authority
  until expiry; no cross-replica invalidation mechanism is required.

## Authority and source coverage

| Boundary | Governing authority | Source and consumer evidence | Result |
|---|---|---|---|
| Effective tenant is selected by verified Wyrd authority, not a path, cookie label, header, or browser field | Spec `INV-001`, `INV-005`; `architecture/wyrd-security-posture.md` security principles and access-token rules | `BrowserSessions.complete` and `signInWithApiKey` obtain access tokens through `openid-client`; `BrowserSession` projects `principal.tenant_id` from that token; `BrowserSession.api` forwards the bearer to Wyrd, whose verified principal remains the authorization boundary (`browser-sessions.ts:49-92,216-246,268-306`). | PASS |
| API-key recovery preserves the exchanged token's tenant authority | Lead-approved TASK-011 decision; spec `REQ-010`; security posture `/auth/token` derivation | The recovery action supplies no route tenant to token exchange (`browser-sessions.ts:235-246,279-290`). The route key selects the encrypted cookie and redirect only. Settings calls carry the private access token and no tenant header or route-derived authority (`settings/+page.server.ts:27-64`). | PASS |
| SSO sessions remain bound to server-selected tenant login state | Spec `REQ-006`, `REQ-009`, `INV-001` | `begin` seals the route key with PKCE verifier and state; Wyrd's authorize endpoint selects the tenant connection; `complete` accepts only the standard grant result for that sealed state (`browser-sessions.ts:189-232`). The journey covers wrong-provider and same-issuer cross-tenant callback mix-up refusal (`production-auth.integration.test.ts:574-591`). | PASS |
| Per-tenant cookie selection cannot substitute another session | Spec `REQ-009`, `REQ-015`, `INV-001` | `cookieName` accepts only tenant-slug syntax, each cookie seals its own tenant, and `sealed` requires the payload tenant to equal the requested cookie key (`browser-sessions.ts:166-186,313-315`). Forged and crossed cookies are cleared and refused (`production-auth.integration.test.ts:387-400,599-605`). | PASS |
| Hooks and protected route consumers preserve server authority | Spec `INV-005`; `AGENTS.md` server/client boundary | `hooks.server.ts:13-30` reads only the selected cookie and installs a `BrowserSession`. The token's tenant UUID and permissions populate request context; protected settings operations call Wyrd with the bearer (`settings/+page.server.ts:27-64,94-150`). UI projection cannot make a refused server operation succeed. | PASS |
| Tenant switching revalidates the target's independent authority | Spec `REQ-015` | `BrowserSessions.switch` calls `read(target, ...)`, so the target cookie must decrypt, bind to the target key, and yield a current token (`browser-sessions.ts:318-364`). The journey covers independent Keycloak/Dex sessions, a missing target session, forged cookies, and cross-tenant cookie copying (`production-auth.integration.test.ts:421-425,542-605`). | PASS |
| Settings reads and mutations cannot operate with route-derived tenant authority | Spec `REQ-003`, `REQ-015`, `INV-001`, `INV-005` | Every production connection read or mutation uses `locals.browserSession.api`; no tenant key is sent as authority. Reader denial and foreign-key isolation are exercised against the real server (`production-auth.integration.test.ts:407-419,489-539`). | PASS |
| Sibling tenant and login sessions remain independent | Spec `REQ-009`, `REQ-015`; TASK-011 logout scenario | Session cookies are per tenant and refresh credentials are per login. Logout opens only the named encrypted cookie, removes only that credential hash from the current replica, and revokes only that refresh credential (`browser-sessions.ts:341-357`). The journey proves a sibling login still renews after the first login logs out (`production-auth.integration.test.ts:427-443`). | PASS |
| Remediation keeps refresh credentials opaque without changing tenant derivation | `FIND-TASK-011-1`; remediation acceptance criteria 1-2 | `establish` no longer decodes the refresh credential and stores it unchanged; `access` later forwards it unchanged to `refreshTokenGrant` (`browser-sessions.ts:249-261,293-310`). Tenant and permissions still come from the resulting access-token claims. The focused test covers opaque forwarding and selected-session clearing on terminal refusal. | PASS |
| Lead-reversed logout behavior remains tenant-local | Lead direction reversing `FIND-TASK-011-2`; spec revision-8 conventional logout direction incorporated by the lead | `logout` clears the selected tenant cookie and matching cache entry before best-effort revocation, logs only route tenant and error class, and never revokes an API key (`browser-sessions.ts:336-357`). The focused test proves the cookie/cache clear and absence of token material in the warning. No retry state, endpoint, setting, or cross-tenant mechanism was added. | PASS |

## End-to-end trust-boundary trace

For SSO, a route key begins only navigation and is sealed into the short-lived
login cookie alongside PKCE verifier and state. The authorization server owns
connection selection and issues tenant-bound Wyrd credentials. The BFF stores
the returned refresh credential opaquely, obtains or refreshes an access token
through `openid-client`, and forwards that access token to protected Wyrd APIs.
The server, not the route key or browser metadata, authorizes the request.

For API-key recovery, the BFF sends the API key and standard token type without
a client-supplied tenant. Wyrd derives tenant and principal from the verified
key. The recovery route key names the cookie and post-login URL, but
`BrowserSession.tenantId`, permissions, and every protected server call come
from the exchanged access token. Consequently, another tenant's key entered on
this tenant's recovery page operates only with the key's tenant authority. The
real-server assertion proves it cannot read or remove the route tenant's staged
connection (`production-auth.integration.test.ts:529-539`). This is the
approved behavior and does not require route/token equality, a mapping
endpoint, or a new claim.

For switching and sibling sessions, the target key selects only that target's
cookie. `read` verifies the encrypted tenant binding and obtains current Wyrd
authority from the credential in that cookie. A source tenant's credential is
never reused to enter a target tenant. Logout similarly selects and clears only
one sealed session; best-effort refresh revocation cannot broaden or transfer
authority to another tenant or login.

## Prior-finding closure in this domain

- `FIND-TASK-011-1` is closed in the cumulative candidate. Refresh credentials
  are opaque to the BFF, while access-token claims remain the sole projected
  source of effective tenant UUID and permissions. The correction introduces
  no alternate tenant resolver or browser-selected authority.
- `FIND-TASK-011-2` was reversed by binding lead direction. The candidate
  follows that direction: local cookie/cache state is always cleared and
  revocation failure is best-effort and redacted. This review does not revive
  the prior retry requirement.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required by the approved task. Adding route/token tenant equality, a
tenant-lookup endpoint or claim, another downstream tenant guard, durable
logout state, or cross-replica cache coordination would contradict approved
direction or add a mechanism not used by the governing standards and
comparable projects.

### Positive Controls

- Wyrd derives API-key tenant authority from the verified credential; the BFF
  supplies no tenant parameter to token exchange.
- The BFF sends protected API calls with the private Wyrd bearer token and no
  route-derived tenant header.
- Per-tenant session cookies are encrypted, host-only, Secure, HttpOnly, and
  SameSite=Lax; the encrypted payload is checked against the selected cookie
  key.
- `openid-client` owns authorization-code redemption, PKCE/state validation,
  refresh, token exchange, and revocation operations.
- Access tokens remain private to `BrowserSession`; browser-visible metadata
  contains only safe navigation and display fields.
- Forged, undecryptable, expired, refused, and crossed session cookies are
  cleared and do not establish a request session.
- Real-server tests exercise reader denial, other-tenant API-key confinement,
  independent multi-provider sessions, crossed cookies, and sibling-login
  logout isolation.

## Material proposed findings

None.

No `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION` finding was
identified in the tenancy and authority-separation boundary. In particular,
the cumulative candidate adds no nonstandard tenant mechanism, setting, check,
file, or option that should be removed from this boundary.

## Verification assessment and limits

The remediation record reports both exact focused browser-session tests, the
UI `check` and `test` lanes, formatting, lints, and `git diff --check` as green.
The focused opaque-token test directly covers the remediation's only tenant-
adjacent change: the access token returned by standard refresh remains the
source of projected tenant authority. The focused logout test covers selected
cookie/cache clearing and redacted failure logging.

This reviewer did not rerun a full identity journey. Per standing direction,
task remediation uses its narrowest owning tests and full journey proof belongs
at change review. The cumulative source was nevertheless checked against the
existing real-server assertions for other-tenant API-key behavior, crossed and
forged cookies, independent multi-provider sessions, settings authorization,
tenant switching, and sibling-login logout isolation.

## Overall result

**PASS**

The cumulative candidate keeps durable tenant and authorization authority at
the Wyrd token/server boundary, preserves the approved API-key recovery
semantics, and closes the opaque-refresh remediation without creating another
tenant resolver or cross-tenant path. The lead-directed local-first logout
change is confined to the selected browser session and introduces no tenancy
regression.
