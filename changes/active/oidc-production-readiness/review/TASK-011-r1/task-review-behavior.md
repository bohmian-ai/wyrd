# TASK-011 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `0b8919fff090d4b0109a506711cb119214d231f2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Prior closure direction: `changes/active/oidc-production-readiness/review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md`

`HEAD` was the candidate above when this review began and remained that candidate
after source inspection. The cumulative range contains three commits and 46
changed paths: the BFF session replacement, its production journey and
two-replica harness support, and the task evidence record.

## Navigation and caller map

| Owner or boundary | Changed entry points and consumers traced |
|---|---|
| OAuth client and cookie owner | `wyrd-ui/src/lib/server/auth/browser-sessions.ts`: `BrowserSessions.begin`, `complete`, `signInWithApiKey`, `read`, `switch`, `logout`, and `metadata`; `BrowserSession.context` and `api` |
| Request authentication | `wyrd-ui/src/hooks.server.ts` reads the tenant cookie, establishes token-derived tenant/principal/permission context, and supplies the production `WyrdClient` boundary |
| Login and recovery | `routes/t/[tenantKey]/login/+page.server.ts`, `login/+page@.svelte`, `login/api-key/+page.server.ts`, `login/api-key/+page@.svelte`, and `routes/login/callback/+server.ts` |
| Session consumers | Tenant layout metadata, root tenant switch/logout actions, Shell/TenantChooser forms, settings loads and mutations, and the existing UI route/action callers |
| Replaced owners | Deleted `lib/server/auth/server-sessions.ts`, deleted `/login/complete`, removed `wyrd_flow` and custom CSRF fields/checks |
| Deployed proof | `production-auth.integration.test.ts`; `identity_ui_e2e.rs`; `WyrdTestServer::start_bound_replica`; the `test:identity:journey` UI selection wiring in `mise.toml` |
| Dependency surface | Direct `openid-client` 6.8.8 and `jose` 6.2.12 entries in `package.json` and the frozen lockfile |

The realistic paths inspected were: tenant login to Wyrd authorization to
provider callback to Wyrd code to BFF redemption; a session cookie moving
between BFF/server replicas; cached and uncached renewal; authorized and denied
settings calls; tenant switching; OIDC-off recovery exchange; replacement
cutoff; and logout/replay behavior.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-009: the BFF is the confidential `wyrd-ui` OAuth client and uses authorization code plus S256 PKCE | `browser-sessions.ts:130-160,186-229` uses `openid-client` discovery, `ClientSecretBasic`, `buildAuthorizationUrl`, library PKCE/state generation, and `authorizationCodeGrant`; `routes/login/callback/+server.ts:5-12` is the registered callback | `production SSO crosses replicas` asserts `response_type=code`, `client_id=wyrd-ui`, exact redirect URI, S256 challenge, state, code redemption, replay refusal, and cross-replica completion; the recorded filtered identity journey passed | PASS |
| RFC 8414 discovery, refresh, and RFC 7009 revocation use the required library rather than handwritten OAuth calls | `BrowserSessions.configuration` uses `client.discovery`; `access` uses `client.refreshTokenGrant`; `logout` uses `client.tokenRevocation`; API-key recovery uses the installed library's `genericGrantRequest` for the existing RFC 8693 extension | Production journeys exercise discovery and code redemption, renewal on a replica with no cached token, API-key exchange, and revocation; package and lock pin `openid-client` 6.8.8 | PASS |
| Encrypted, Secure, HttpOnly, SameSite=Lax cookie is the only browser session; no server-side session store | `browser-sessions.ts:20-31,95-180,246-264` seals the renewal credential with `jose` `dir`/`A256GCM`; `BrowserSessions` owns only process-local configuration/access-token maps | Journey `expectSafeCookie` and `expectEncrypted` assert flags, host-only scope, compact JWE shape and no clear token; the same cookie is read by both BFF replicas | PASS |
| Access tokens are process-local and keyed by the renewal credential hash; a miss renews with no replica coordination | `browser-sessions.ts:106-108,266-313` hashes the refresh token/API key, keeps a bounded per-process map, reuses a fresh token, and renews on a miss | `production SSO crosses replicas` redeems on one BFF and serves through the other, forcing the second BFF to renew; `identity_ui_e2e.rs:389-486` runs the BFFs against two independently built Wyrd replica states over shared durable storage | PASS |
| No Wyrd or provider token reaches page data, JavaScript, or redirect URLs | `BrowserSession.#token` is private; `metadata` projects only principal id, expiry, and tenant keys; routes return safe metadata/problems; the refresh token/API key exists only inside the encrypted HttpOnly cookie | `expectNoSecrets` inspects rendered pages, Svelte data responses, redirect history and cookie behavior on both replicas; API-key journeys separately check the plaintext key never appears | PASS |
| Lead-approved REQ-010 behavior: SSO is the routine primary action and API-key sign-in is a separate recovery page | Login UI exposes one primary `?/sso` action and links to `/login/api-key`; the separate recovery route owns the API-key form and exchange | `OIDC-off credential UI` proves the SSO-first page, separate recovery form, inactive-SSO problem path linking to recovery, bad-key and cross-origin refusal; `production SSO crosses replicas` proves recovery remains usable with active SSO | PASS |
| Lead-approved API-key tenant authority: the exchanged token's tenant governs; no new endpoint or claim | `signInWithApiKey` uses the existing token-exchange grant; `BrowserSession.context` takes `tenantId`, principal and permissions from the server-issued access-token claims; settings calls send that token to the existing server API. The route slug is navigation context and does not become request authority | `OIDC-off credential UI` signs in on one recovery route with another tenant's key, observes none of the route tenant's staged state, fails removal, and confirms the owning tenant's record remains | PASS |
| REQ-015: switching uses the target tenant's separately established authority and otherwise prompts for that tenant login | `BrowserSessions.switch` calls `read` for the target's own encrypted cookie; `sealed` binds each cookie payload to its cookie tenant; no source-session credential is reused for the target | `production multi-provider tenant switch` holds independent Keycloak and Dex sessions, switches on both replicas, sends an absent target session to login, and refuses crossed/forged cookie values | PASS |
| REQ-016: replacement/deactivation stops renewal through the old connection while an already issued access token remains valid to expiry | `BrowserSessions.read/access` continues using a fresh cached access token, then clears the cookie when the server refuses the next refresh; no BFF-side connection override or revocation list was added | `production provider replacement settings` and `production SSO crosses replicas` prove the bounded issued-token window, eventual renewal refusal on both replicas, replay refusal, and replacement-session continuity | PASS |
| Logout revokes only this login's refresh token and clears its cookie | `browser-sessions.ts:338-351` deletes the selected cookie and its local cache entry, revokes only that cookie's refresh token with the standard hint, and deliberately does not revoke an operator API key | The two-login journey logs out one login, proves its refresh cannot renew, and proves the sibling login still renews; OIDC-off journey proves logout leaves the underlying API key exchangeable | PASS |
| Expected standard bearer behavior: logout does not retroactively invalidate an access token already cached on another replica | Access tokens remain ordinary self-contained bearer tokens until `exp`; logout revokes renewal authority and local state only | Task evidence records this material limit; the source and security authority agree that bearer access tokens remain usable until bounded expiry. No nonstandard revocation mechanism was introduced | PASS |
| CSRF is SvelteKit origin checking plus SameSite=Lax, with the custom token deleted | All affected POST forms have removed custom CSRF fields and server checks; SvelteKit's default `csrf.checkOrigin` remains enabled; session/login cookies are SameSite=Lax | Real built-server journeys get 403 for cross-origin settings and API-key recovery posts; UI check/test lanes passed | PASS |
| AC-001: an OIDC-off deployment serves a real UI path using an existing Wyrd credential | The separate recovery route uses the existing API-key token exchange and cookie/session owner; no password store, IdP secret, or mock-auth bypass was added | `OIDC-off credential UI` drives bad, reader, admin, logout, authorized staging, denied mutation, and cross-tenant authority cases against the real server | PASS |
| AC-002: real-provider UI sign-in produces authorized behavior and a denied call | The BFF uses the real Wyrd authorization server and token-derived permissions; settings mutations call the existing server boundary through `BrowserSession.api` | `production SSO crosses replicas` signs an admin and reader in through Keycloak, permits the admin mutation, and refuses the reader mutation | PASS |
| AC-003: two tenants/providers remain isolated and usable concurrently | Per-tenant encrypted cookies and target-specific `read` keep the sessions independent; no provider-specific branch exists in the BFF | `production multi-provider tenant switch` uses Keycloak and Dex concurrently, and exercises wrong-provider, wrong-tenant, same-issuer cross-tenant, forged-cookie, switching, and separate-session paths | PASS |
| Applicable AC-007 BFF proof: two BFF replicas and two Wyrd replicas, replay/failure refusal, old-connection cutoff, and no session on failed code completion | `start_bound_replica` binds a separately composed replica over the shared fixture; the BFF catches failed grants without setting a session; replacement refusal is owned by refresh | The four production UI journeys cover two-by-two replica routing, missing/forged login state, code replay, state/PKCE mismatch, forged/crossed session cookies, CSRF refusal, revoked refresh, and replacement cutoff. Broader provider/server fault cases remain in the existing identity journeys and are not reimplemented here | PASS |
| FIND-TASK-010-1 closure: one production `openid-client` driver proves code + PKCE, refresh, and revoke against the real server | The production owner is `BrowserSessions`; no second test-only OAuth driver was added | `production SSO crosses replicas` performs all three operations across real BFF and server replicas; this matches the lead direction exactly | PASS |
| Delete the replaced protocol, flow-cookie implementation, custom CSRF token, and BFF `/login/complete` side | `server-sessions.ts` and `routes/login/complete/+server.ts` are deleted; custom CSRF code and form fields are removed; the old BFF service-key/private-session callers are gone | Cumulative diff and repository search support the task's recorded absence checks; built UI and journey lanes passed | PASS |
| Prohibited scope: no tokens in browser-visible data, server-side session store, second role mapper, untrusted route tenant authority, `@auth/sveltekit`, `tower-sessions`, or handwritten OAuth call | `BrowserSession` projects server claims, uses the installed standard clients, and adds neither durable state nor role mapping; manifests contain only the approved dependencies | Source/caller trace plus production negative journeys; dependency and cumulative-diff inspection | PASS |
| Standards-first/no unearned mechanism | Code uses `openid-client` for the standard client operations, `jose` for the encrypted cookie, SvelteKit origin protection, and existing Wyrd exchange/API owners. The bounded access cache and temporary state cookie are direct consequences of the approved cookie-session and PKCE design | Source trace found no provider branch, compatibility path, extra public option, duplicated OAuth validator, or proposed custom revocation/coordination mechanism | PASS |
| Write-set closure and regression boundary | Changes stay within the UI/session consumers, package lock, UI journey host/lane, one narrow test-harness method, and the task evidence record | Recorded UI check: 0 errors/warnings; UI test: 32 files/177 tests; focused routing/changes tests, filtered production journey, Rust fmt/lints, and `git diff --check` all exited 0 | PASS |

## Proposed findings

None.

The acceptance audit found no reachable missing, incorrect, drifting,
constraint-violating, or regressing behavior. In particular, it does not treat
the bounded lifetime of a previously issued bearer access token after logout
as a defect, and it does not require a new tenant-mapping endpoint or token
claim for API-key recovery.

## Verification assessment

The review used the task-recorded narrow commands and inspected the production
journey assertions and their actual owners. It did not rerun an unfiltered
identity sweep or a repository aggregate; those are change-review work under
the standing direction. The recorded evidence is credible for this write set:
the named host command selects the real production UI journey, the journey
runs four browser-facing cases over Keycloak/Dex, two BFF processes, two Wyrd
replicas and repository Postgres, and the UI check/unit lanes plus Rust
format/lints cover the remaining changed surfaces.

## Overall result

**PASS**
