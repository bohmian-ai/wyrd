# TASK-011 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `0b8919fff090d4b0109a506711cb119214d231f2`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Prior closure direction: `changes/active/oidc-production-readiness/review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md`

The candidate remained at the stated commit throughout this review. I reviewed the complete base-to-candidate range and did not read another TASK-011 review report.

## Navigation and invariant trace

| Producer or owner | State or value | Sinks and sibling consumers traced | Invariant result |
|---|---|---|---|
| `BrowserSessions.begin` | `openid-client` PKCE verifier and state, sealed in the five-minute `wyrd_login` cookie; tenant route key supplied as OAuth routing context | Wyrd `/auth/authorize`; `BrowserSessions.complete`; common `/login/callback`; failure redirects | The library generates and checks PKCE and state. The Wyrd authorization server's single-use login state remains the connection and tenant authority; the BFF cookie only binds its own OAuth transaction and post-grant route. |
| `BrowserSessions.complete` | Standard token response from `authorizationCodeGrant`; refresh token becomes the encrypted session credential | `establish`, cookie, access cache, later refresh and revoke | A session is created only after the standard confidential-client code grant succeeds and returns a refresh token. Callback errors, missing/invalid login state, mismatched state/PKCE, and replay create no session. |
| `BrowserSessions.signInWithApiKey` | Existing API key exchanged by `genericGrantRequest` using the existing RFC 8693 grant | `establish`, `access`, protected `/v1` requests | The route key chooses the cookie/navigation slot, but the exchanged token's `principal.tenant_id` and signed permissions are the effective authority. This matches the lead-approved REQ-010 decision and adds no endpoint, claim, role mapper, or client-side tenant authority. |
| `BrowserSessions.establish`, `seal`, and `unseal` | JWE `dir`/`A256GCM` cookie containing `{tenant, kind, credential}`; key derived with HKDF from the shared confidential-client secret | Both BFF replicas; `read`, `logout`, `switch`, `metadata` | The browser receives only an encrypted Secure, HttpOnly, SameSite=Lax, host-only cookie. Replicas sharing the configured client secret can read it without a server-side BFF session store. |
| Wyrd token endpoint | Access token claims: expiry, principal tenant/id, and permissions | `cache`; `BrowserSession.context`; `WyrdClient` UI gates; `BrowserSession.api` bearer call | The UI decodes only the access token returned directly by Wyrd's authenticated token endpoint. It maps no roles and cannot widen signed permissions. Protected operations are still authorized by Wyrd from the bearer token. |
| Refresh token or API key | SHA-256 cache key | `#access`, `access`, `logout`; sibling requests and both replica-local caches | Raw credentials are not cache keys or browser-visible metadata. A cache miss uses the appropriate standard library grant. Expired entries are removed and the process-local cache is bounded. |
| `BrowserSessions.access` | Cached access token until its own expiry margin; otherwise refreshed/exchanged | Hooks on every protected tenant request; replacement, deactivation, removal, logout replay | Refresh refusal clears the cookie through `read`. A cached access token retaining snapshot authority until expiry after connection replacement or refresh-token revocation is the explicitly approved and documented OAuth bearer-token behavior, not a gap. |
| `hooks.server.ts` | `BrowserSession` and token-derived `TenantContext` | Every tenant route, server load/action, `WyrdClient`, settings API | Production routes do not use the development identity. Missing, unreadable, crossed, expired, or non-renewable sessions are refused before tenant route handling. |
| `BrowserSessions.switch` and `metadata` | Target tenant's independently sealed cookie; safe subject/tenant labels and expiry | Root switch action, tenant chooser, tenant layout | Switching calls `read` on the target session and therefore renews/revalidates it or prompts for that tenant's login. Metadata does not contain a credential, access token, refresh token, API key, or permission set. |
| `BrowserSessions.logout` | This cookie's refresh token and its local cache entry | RFC 7009 revocation endpoint; cookie deletion; other sessions and replica caches | Only the selected login's refresh token is revoked. API keys are not revoked. Other logins remain independent. A self-contained access token already cached on another replica remains valid only until its bounded expiry, as explicitly approved. |
| SvelteKit default `csrf.checkOrigin` plus SameSite=Lax | Origin enforcement for form actions | Login, recovery, switch, logout, settings, and change actions after custom CSRF-field deletion | The generated SvelteKit server configuration has origin checking enabled. Cross-site form posts are refused by the framework; no parallel custom CSRF protocol remains. |
| `production-auth.integration.test.ts` hosted by `identity_ui_e2e.rs` | Real Keycloak/Dex, two BFF processes, two Wyrd replicas over shared Postgres, one BFF-to-Wyrd TLS hop | Code redemption, refresh on the other replica, revocation, API-key exchange, provider replacement, settings authorization, browser/data/cookie assertions | The journey exercises the real deployed boundaries and directly closes FIND-TASK-010-1 without a second test-only OAuth driver. |

Sibling writers and consumers of session authority were also enumerated: the login and recovery routes are the only cookie creators; hooks is the protected-route reader; the root switch/logout actions and tenant layout are the lifecycle/metadata consumers; settings is the only current production route that sends the bearer token to the server; development `LocalSessions` remains isolated behind `localAuthEnabled`. No second production credential, tenant, role, or permission authority was introduced.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006: tenant login entry, generic SSO primary action, common callback, route key not tenant authority | `routes/t/[tenantKey]/login/+page.server.ts`, `+page@.svelte`; `routes/login/callback/+server.ts`; `BrowserSessions.begin`/`complete` | `OIDC-off credential UI` proves SSO remains primary and recovery is separate; SSO and multi-provider journeys drive the common callback and mix-up refusals | PASS |
| REQ-009: confidential `wyrd-ui`, code grant + S256 PKCE, encrypted cookie, tokens absent from page/URL/JS, refresh-token logout | `BrowserSessions.configuration`, `begin`, `complete`, `seal`, `logout`; `ClientSecretBasic`; `openid-client` and `jose` pinned in package and lock files | `production SSO crosses replicas`; `expectSafeCookie`, `expectEncrypted`, and `expectNoSecrets`; task records focused journey exit 0 | PASS |
| Lead-approved REQ-010: SSO is routine login; API-key sign-in is a separate recovery page | `routes/t/[tenantKey]/login/api-key/*`; routine login contains one primary SSO action and a recovery link | `OIDC-off credential UI` covers SSO refusal/problem-to-recovery, bad key, reader/admin outcomes, and recovery while SSO is active | PASS |
| Lead-approved REQ-010 tenant authority: an API-key session is scoped by the token's tenant, with no new endpoint or claim | `genericGrantRequest` uses the existing token-exchange endpoint; `BrowserSession.context` takes tenant UUID and permissions from access-token claims; `BrowserSession.api` sends that bearer to Wyrd | Cross-tenant-key journey proves the foreign key cannot see or remove the route tenant's staged connection while its own token remains the server authority | PASS |
| REQ-015: tenant switch revalidates target membership and otherwise prompts for target login | Per-tenant encrypted cookies; `BrowserSessions.switch` delegates to `read`, which renews through Wyrd | `production multi-provider tenant switch` covers two providers, both replicas, absent target session, valid target session, forged cookie, wrong-provider, same-issuer cross-tenant, and crossed-cookie refusal | PASS |
| REQ-016: old connection cannot renew; current access-token snapshot may live until bounded expiry | `BrowserSessions.access` returns a non-expired cached token and refreshes on expiry/miss; refused refresh causes `read` to clear the cookie | Deactivation and provider-replacement paths prove the immediate cached-token window, renewal refusal after expiry, and replay refusal on either replica | PASS |
| INV-001: paths, browser state, and headers cannot select effective tenant or connection authority | Route/cookie tenant is routing metadata; signed access-token tenant and permissions govern API calls; Wyrd's consumed login state governs the federated connection | Forged/crossed cookies, wrong-provider callback, same-issuer cross-tenant callback, foreign API-key route, and cross-site POST cases are refused or confined to token authority | PASS |
| INV-005: UI projects server-owned identity and permissions; no durable role mapper | `Claims` projects Wyrd-issued principal/permissions; `permissionNames` is a shape projection; bearer remains server-authorized | Reader denied/admin allowed settings mutations through the real server; replacement-provider user inherits no prior authority | PASS |
| AC-001: OIDC-off real UI journey with an existing Wyrd credential | Recovery route uses the existing RFC 8693 exchange and the same encrypted cookie/access path; no IdP or local password store | `OIDC-off credential UI` exercises bad key, CSRF refusal, reader denial, logout without key revocation, admin staging, and foreign-tenant confinement | PASS |
| AC-002: real-server self-hosted provider login, mapped authorization and denial | Standard BFF flow and settings bearer calls | `production SSO crosses replicas` signs in Keycloak admin and reader, permits the admin action, and refuses the reader action | PASS |
| AC-003: concurrent tenants/providers, isolated sessions, mutation/removal, callback refusal | Independent per-tenant cookies; Wyrd-bound login; target-session renewal | Multi-provider switch and provider-replacement journeys use Keycloak's two realms plus Dex and cover mutation, removal, wrong-provider and same-issuer cross-tenant refusal | PASS |
| AC-004 browser clause: no provider secret or Wyrd token in page data or redirect URLs | Private `#token`; single `metadata` safe projection; JWE cookies | `expectNoSecrets` checks pages, `__data.json`, settings, and redirects on both replicas; clear API keys and JWTs are also checked | PASS |
| AC-007: cookie works across two BFF/two Wyrd replicas; old session cannot renew; OAuth negatives yield no session | Shared JWE key, no BFF store, replica-local access caches; real second Wyrd replica and `start_bound_replica` seam | Journey redeems on one BFF and refreshes on the other; forged/missing login cookies, code replay, callback mix-up, crossed session cookie, provider replacement/deactivation, and CSRF refusal are covered | PASS |
| FIND-TASK-010-1 closure: off-the-shelf `openid-client` drives code + PKCE, refresh, and revoke against the real server | `BrowserSessions.begin`/`complete`/`access`/`logout` use `openid-client` 6.8.8 operations directly | The filtered production BFF journey drives authorization-code redemption, cross-replica refresh, and RFC 7009 revocation against the real Wyrd replicas | PASS |
| Delete `server-sessions.ts`, flow cookie, custom CSRF, and BFF `/login/complete` | Deleted files and all form CSRF fields; `SessionMetadata.csrf` removed; no old channel identifier remains in the touched runtime | Source search over UI, journey host, harness and lane finds only the intentional `csrf.checkOrigin` test comment | PASS |
| No handwritten OAuth call; use `openid-client` and `jose` conventionally | OAuth discovery, authorize URL, code grant, refresh, generic grant, and revocation all use documented library calls; direct fetch remains only the library transport adapter and protected-resource call | Package/lock pin 6.8.8 and its dependency versions; UI check/tests and real journey recorded green | PASS |
| No server-side session store, second role mapper, `@auth/sveltekit`, `tower-sessions`, new server endpoint/claim, or unrelated server behavior | One encrypted-cookie owner and one process-local access-token cache; server write set is limited to the test harness seam | Full cumulative diff and dependency/source search | PASS |
| Verification is the task's narrowest lanes; unfiltered journeys remain change-review work | No broad aggregate or every-language sweep is claimed for TASK-011 | Task evidence records the filtered UI identity journey, exact focused Vitest tests, UI check/test, format, lints, and `git diff --check`, all exiting 0 | PASS |

## Failure and lifecycle assessment

- Discovery, code redemption, refresh, token exchange, and revocation have the library's bounded request timeout. Discovery failure is not cached, so a later request retries without a custom retry protocol.
- A malformed, expired, crossed, or undecryptable session cookie is cleared. A standards error response during refresh/exchange is treated as refused authority; transport and dependency failures remain upstream failures rather than becoming authentication success.
- Deactivation, replacement, removal, and logout cut off future refresh at the server authority. Cached bearer authority is neither extended nor reinterpreted and expires at the token's `exp` value.
- Logout deletes the selected cookie and its local cache entry, then uses standard refresh-token revocation. It does not revoke an operator API key or another login's refresh token.
- The second BFF process can reconstruct session renewal from the encrypted cookie alone. The second Wyrd replica uses the same persistent authority, establishing the intended process-restart/rolling-replica boundary without introducing coordination state.

## Proposed findings

None.

The route-key presentation retained for an API-key recovery session is not a finding: the approved decision expressly makes the exchanged token's tenant authoritative without a new tenant-slug claim or mapping endpoint, and the real-server proof shows the route cannot widen that token. Likewise, a self-contained access token remaining usable on a replica until its own expiry after refresh-token revocation is the explicitly approved standard OAuth bearer-token window, not an incomplete logout.

Placement, naming, structure, and wording observations were not promoted to findings, as directed. The candidate adds no non-standard OAuth mechanism, check, file, setting, or option that would warrant `DRIFT`; the only protocol operations are the approved `openid-client`/`jose` paths and native SvelteKit origin protection.

## Verification assessment

Available task evidence records successful execution of:

- the filtered `production_ui_bff_journey` identity lane (four production UI journey tests and the Rust host test);
- the exact changed routing and Changes journey Vitest cases;
- the complete UI `check` and UI test lanes (32 files, 177 tests);
- Rust formatting and lints; and
- `git diff --check`.

That is the narrowest task-level proof required by TASK-011. Per standing direction, unfiltered identity journeys and every-language sweeps belong to change review and are not a TASK-011 verification deficiency.

## Overall result

**PASS**

The cumulative candidate satisfies the original task and closes FIND-TASK-010-1. The traced credential, tenant, permission, cache, renewal, replacement, and logout invariants have a single server-owned authority, and no material `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION` finding remains.
