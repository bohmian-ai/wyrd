# TASK-011 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Prior reviewed candidate: `0b8919fff090d4b0109a506711cb119214d231f2`
- Candidate: `4d468b33e49de4dd9df30c5dd046a334569465bf`
- Candidate tree: `87acdce15e3ca6ea2b6016969695db398ab2d598`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-011-r1/TASK-011-R1-browser-session-standards-and-logout.md`
- Lead direction: `changes/active/oidc-production-readiness/review/TASK-011-r1/lead-direction-FIND-TASK-011-2.md`

The candidate remained at the stated commit and tree throughout this review. I
reviewed the complete base-to-candidate range and used the remediation diff
`0b8919fff090d4b0109a506711cb119214d231f2..4d468b33e49de4dd9df30c5dd046a334569465bf`
to locate the corrected owner and verify prior-finding closure. The lead
direction reverses the original remedy for `FIND-TASK-011-2`: browser logout
must always clear local state and RFC 7009 revocation is best-effort. The
original preserve-for-retry behavior is therefore not an acceptance
obligation and was not reopened.

## Navigation and invariant trace

| Producer or owner | State or value | Sinks and sibling consumers traced | Invariant result |
|---|---|---|---|
| `BrowserSessions.begin` (`browser-sessions.ts:189-209`) | `openid-client` PKCE verifier and state plus the route tenant, sealed in the five-minute `wyrd_login` cookie | Wyrd `/auth/authorize`, the common callback, `BrowserSessions.complete`, and failure redirects | The library generates PKCE/state. The BFF cookie binds the client transaction, while Wyrd's consumed server login state remains the connection and tenant authority. |
| `BrowserSessions.complete` (`216-233`) | The `authorizationCodeGrant` result; its refresh token is passed unchanged to `establish` | Encrypted session cookie, access cache, later refresh and revocation | A session is produced only after the standard confidential-client code grant succeeds and returns a refresh token. Missing, crossed, replayed, or invalid login state produces no session. |
| `BrowserSessions.signInWithApiKey` (`235-247`) | Existing operator API key exchanged through `genericGrantRequest` using the RFC 8693 grant | `establish`, `access`, route loads/actions, protected Wyrd calls | Per the approved lead decision, the route key is presentation/routing context and the exchanged token's tenant and permissions are authority. No mapping endpoint, tenant claim, role mapper, or alternate authority was added. |
| `BrowserSessions.establish`, `seal`, and `unseal` (`172-187`, `249-262`) | JWE `dir`/`A256GCM` cookie holding `{tenant, kind, credential}` under a twelve-hour application-session bound | Both BFF replicas, `read`, `switch`, `metadata`, and `logout` | The renewal credential remains opaque. The browser receives only a Secure, HttpOnly, SameSite=Lax encrypted cookie, and replicas sharing the client secret require no server-side BFF session store. |
| Wyrd token endpoint | Wyrd access token containing expiry, principal tenant/id, and permissions | `cache`, `BrowserSession.context`, server-side UI gates, and `BrowserSession.api` | The UI projects server-issued claims and sends the bearer back to Wyrd for authorization. It maps no role and cannot widen token authority. |
| Refresh token or API key | SHA-256 cache key | `#access`, `access`, `logout`, sibling requests, and each replica-local cache | Raw renewal credentials are neither map keys nor browser-visible metadata. Cache misses use the corresponding installed-library grant; the cache is bounded and expired entries are removed. |
| `BrowserSessions.access` and `read` (`293-334`) | Cached access token until its expiry margin, otherwise refresh/exchange result | Hooks on every protected route; replacement/deactivation/removal; tenant switch; replayed cookies | A terminal OAuth refusal clears only that tenant cookie. Transport/dependency failures remain upstream failures. Current self-contained access-token authority lasts only until its signed `exp`, matching REQ-016. |
| `hooks.server.ts:10-31` | `BrowserSession` and token-derived `TenantContext` | Every protected tenant route, `WyrdClient`, settings loads/actions, and tenant layout | Production routes do not use the development identity. An absent, unreadable, crossed, expired, or non-renewable session is refused before route handling. |
| `BrowserSessions.switch` and `metadata` (`360-384`) | Target tenant's separately sealed cookie and a safe metadata projection | Root switch action, tenant chooser, tenant layout | Switching calls `read` for the target cookie and therefore renews that target authority or prompts for login. Metadata contains no credential, access token, API key, or permission set. |
| `BrowserSessions.logout` (`336-358`) | Selected cookie, its credential-hash cache entry, and—only for refresh sessions—the RFC 7009 operation | Root logout action, local browser state, one replica's cache, Wyrd revocation endpoint, and sibling sessions | Cookie and cache deletion are unconditional. API keys are never revoked. Refresh-token revocation is best-effort; failure logs only tenant plus error class and does not undo logout. No retry/store/setting/endpoint was introduced. |
| SvelteKit `csrf.checkOrigin` plus SameSite=Lax | Framework origin enforcement for form actions | Login, recovery, switch, logout, settings, and changed actions after custom CSRF deletion | The native framework mechanism replaces the deleted custom CSRF protocol; the cumulative diff does not add a parallel check. |
| `browser-sessions.test.ts` | Opaque refresh credential, access-cache renewal boundary, local logout state, and failed revocation | `authorizationCodeGrant`, `refreshTokenGrant`, `tokenRevocation`, cookie jar, warning sink | The focused tests directly exercise both remediation sources: opaque refresh forwarding and local logout/cache clearing despite revocation failure. |
| Production journey plus Rust host | Real Keycloak/Dex, two BFF processes, two Wyrd replicas, shared Postgres, and one BFF-to-Wyrd TLS hop | Code redemption, other-replica refresh, tenant switching, API-key recovery, settings authorization, replacement, logout, and token-leak assertions | The source provides the required user-journey proof boundary. Per standing direction, its full execution belongs to change review; it is not a remediation-round gate. |

Sibling writers and consumers were enumerated. The SSO login route and
separate API-key recovery route are the only production session-cookie
creators. The callback is the sole code-redemption entry. Hooks is the common
protected-route reader. Root switch/logout actions and the tenant layout are
the lifecycle and metadata consumers. Settings is the current production route
that sends the bearer to Wyrd. Development `LocalSessions` stays isolated
behind `localAuthEnabled`. No second production credential, tenant, role,
permission, refresh, or logout authority entered the cumulative diff.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006: canonical tenant login, generic SSO primary action, common callback, and route key not trusted as authority | Tenant login and common callback routes; `BrowserSessions.begin`/`complete`; separate recovery route | Production journey source covers SSO entry, inactive/OIDC-off refusal, callback errors, and recovery link | PASS |
| REQ-009: confidential `wyrd-ui`, code grant with S256 PKCE, encrypted cookie, no token in page/URL/JavaScript, refresh-token logout | `configuration` uses `ClientSecretBasic`; `begin`/`complete`/`seal`/`logout`; exact `openid-client` 6.8.8 and `jose` 6.2.12 pins | Focused session tests pass; journey source contains encrypted-cookie and no-secret assertions | PASS |
| REQ-010 approved decision: SSO is routine login and API-key sign-in is a separate recovery page | `/t/[tenantKey]/login` and `/t/[tenantKey]/login/api-key`; the routine page contains the SSO action and recovery link | `OIDC-off credential UI` source covers problem-to-recovery, bad key, reader/admin behavior, and recovery while SSO is active | PASS |
| REQ-010 approved tenant authority: API-key session authority is the token's tenant | Existing RFC 8693 exchange; `BrowserSession.context` uses access-token tenant/id/permissions; `BrowserSession.api` sends that bearer | Cross-tenant recovery journey source proves the key cannot read or mutate the route tenant's connection | PASS |
| REQ-015: tenant switch revalidates target membership or prompts for target login | Per-tenant encrypted cookies; `switch` delegates to `read` for the target | Multi-provider journey source covers absent/valid/forged/crossed target cookies and two providers | PASS |
| REQ-016: old connection cannot renew; issued access authority may remain only through bounded expiry | `access` returns only a still-live cached token and otherwise refreshes; refused refresh causes `read` to clear the cookie | Replacement/deactivation journey source covers the issued-token window and renewal refusal | PASS |
| REQ-021 and remediation `FIND-TASK-011-1`: refresh credentials are client-opaque and standard library paths own OAuth mechanics | `establish` stores the string without decoding it; `access` forwards it unchanged to `refreshTokenGrant` | `accepts an opaque refresh token and forwards it unchanged` passes | PASS |
| Human reversal of `FIND-TASK-011-2`: local logout always completes; revocation is best-effort and secret-free | `logout` clears cookie/cache before revocation, returns for API keys, and catches refresh revocation failure with tenant/error-name fields only | `failed refresh-token revocation still signs out` passes and proves cache eviction by forcing replay through `refreshTokenGrant` | PASS |
| INV-001: paths, browser state, and headers cannot select effective tenant or connection authority | Wyrd login state selects the federated connection; Wyrd access-token claims govern protected calls; cookie tenant only selects a local slot | Forged/crossed cookies and wrong-provider/same-issuer cross-tenant journey cases create no widened authority | PASS |
| INV-005: UI projects server-owned identity and permissions and owns no durable role mapping | `Claims`, `permissionNames`, `BrowserSession.context`, and bearer-backed `api` | Real-server journey source distinguishes reader denial from admin success | PASS |
| AC-001: OIDC-off real UI path uses an existing Wyrd credential with no local password store | Separate recovery action uses the existing token-exchange endpoint and common encrypted-cookie owner | OIDC-off journey source covers bad key, authorized and denied calls, logout, and tenant confinement | PASS |
| AC-002: real-provider UI login makes one authorized and one denied Wyrd call | Standard BFF code/refresh flow and server-backed settings calls | `production SSO crosses replicas` source signs in admin and reader through Keycloak | PASS |
| AC-003: two tenants/providers remain isolated and usable concurrently | Independent per-tenant cookies, server-bound login state, and target-specific renewal | Multi-provider journey source uses Keycloak and Dex and covers configuration, session use, mutation/removal, and callback mix-up refusal | PASS |
| AC-004 browser clause: no provider secret or Wyrd token in page data or redirect URLs | Private `#token`; `metadata` is the sole safe browser projection; JWE cookies | Journey `expectNoSecrets`/`expectEncrypted` source checks pages, data responses, redirects, and cookies | PASS |
| AC-007: one cookie works across two BFF/two Wyrd replicas; malformed/replayed OAuth inputs yield no session | Shared cookie key, no BFF store, replica-local caches, real second Wyrd replica and bound harness seam | Journey source redeems on one BFF and refreshes on the other and covers forged/missing login state, code replay, crossed cookies, replacement, and CSRF | PASS |
| Delete `server-sessions.ts`, flow cookie, custom CSRF token, and `/login/complete` | Deleted legacy files/fields/routes; built-in SvelteKit origin protection remains | Cumulative source/diff inspection | PASS |
| Prohibited mechanisms remain absent: no server-side session store, second role mapper, `@auth/sveltekit`, `tower-sessions`, hand-written OAuth call, new endpoint/claim, retry, durable logout state, or new setting | One encrypted-cookie owner, process-local access cache, installed `openid-client`/`jose`, and existing Wyrd endpoints only | Dependency, source, and cumulative diff inspection | PASS |
| Verification is narrowest-lane at task review; full journeys wait for change review | Remediation write set is limited to `browser-sessions.ts`, its focused test, and one assertion tightening in the existing journey | Reviewer rerun: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/browser-sessions.test.ts` — 1 file, 2 tests passed | PASS |

## Prior-finding closure

### `FIND-TASK-011-1` — closed

The invalid state was produced by `BrowserSessions.establish` interpreting a
refresh credential as a JWT to derive cookie expiry. Candidate lines 249-262
delete that inspection and apply the already-existing conventional twelve-hour
application-session bound to both credential kinds. The same opaque string is
sealed and later forwarded by `access` to `openid-client.refreshTokenGrant`.
The focused test uses `opaque-refresh-token-not-a-jwt`, observes the encrypted
cookie and twelve-hour bound, and verifies exact unchanged forwarding. No
parser, introspection call, option, setting, endpoint, or alternate mechanism
replaces the deleted coupling.

### `FIND-TASK-011-2` — original finding reversed; lead direction satisfied

The prior preserve-the-cookie-for-retry recommendation is not current
authority. Candidate lines 341-357 clear the selected cookie and cache entry
unconditionally, skip revocation for API-key sessions, and catch a failed
refresh-token revocation so logout still completes. The warning includes only
the route tenant and the error class; no token value, exception message,
request body, or response body is logged. The focused test proves deletion,
best-effort failure handling, token-free logging, and cache eviction. This is
the standard local-logout outcome selected by the human direction and spec
revision history, so the original finding must not be reopened.

## Failure and lifecycle assessment

- Discovery, code redemption, refresh, token exchange, and revocation remain
  installed-library operations. A failed discovery is removed from the
  configuration cache so a later request can retry without a custom retry
  subsystem.
- Malformed, expired, crossed, or undecryptable cookies are cleared. A token
  endpoint OAuth refusal ends that tenant session; dependency/transport failure
  is an upstream error rather than an authentication success.
- Connection deactivation, replacement, or removal refuses the next refresh at
  Wyrd. A previously issued access token is neither extended nor reinterpreted
  and expires at its own signed expiry.
- Logout always ends the live browser's local session. Failed best-effort
  revocation can leave a copied credential usable within existing server token
  bounds, which is the explicitly accepted conventional tradeoff, not a gap to
  fill with retry state or coordination.
- Another BFF reconstructs renewal from the encrypted cookie alone, and the
  second Wyrd replica uses the same durable authority. Process restart and
  rolling replacement therefore do not require BFF session persistence.

## Proposed findings

None.

The API-key recovery route retaining its entered route key for navigation is
not a finding because the approved decision expressly makes the exchanged
token's tenant authoritative and forbids a new mapping endpoint or claim. A
cached self-contained access token remaining usable until its own expiry after
logout or connection replacement is likewise an approved OAuth bearer-token
window.

Placement, naming, structure, and wording observations were not promoted to
findings, as directed. The remediation deletes the non-standard refresh-token
inspection and adds no mechanism, check, file, setting, or option absent from
the applicable standards or comparable conventional browser logout behavior.

## Verification assessment

The remediation evidence records both exact focused tests, the UI `check` and
UI test lanes, and `git diff --check` as successful. This reviewer independently
reran the owning test file through the repository toolchain; both focused tests
passed. These are the narrowest credible lanes for the remediation write set.
Full identity journeys and every-language sweeps remain change-review work and
their non-execution in this review is not a verification limit or acceptance
gap.

## Overall result

**PASS**

The cumulative candidate satisfies the original TASK-011 obligations.
`FIND-TASK-011-1` is closed at its producer, the human reversal of
`FIND-TASK-011-2` is implemented without extra lifecycle machinery, and the
traced credential, tenant, permission, cache, renewal, replacement, and logout
paths reveal no material `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or
`REGRESSION` finding.
