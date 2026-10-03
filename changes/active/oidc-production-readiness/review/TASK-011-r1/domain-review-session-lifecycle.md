# Session Lifecycle, Concurrency, and Durability Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `0b8919fff090d4b0109a506711cb119214d231f2`
- Task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Prior-finding direction: `changes/active/oidc-production-readiness/review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md`

The candidate remained at the stated commit throughout this review. I reviewed the complete base-to-candidate range independently and did not use another reviewer's conclusions.

## Reviewed Boundary

This review is limited to session lifecycle, concurrency, and durability semantics:

- the encrypted login and per-tenant session cookies;
- the per-process access-token cache keyed by the refresh-token or API-key hash;
- nonrotating confidential-client refresh-token use by concurrent tabs and replicas;
- cache miss, access-token expiry, process-local cache loss, and renewal;
- logout, refresh/revoke races, and the accepted bounded access-token validity after refresh-token revocation;
- connection deactivation, replacement, and deletion cutoff at the next renewal;
- two BFF replicas in front of two Wyrd replicas; and
- the `WyrdTestServer::start_bound_replica` seam and the task's focused identity journey.

I did not audit authorization-server protocol validation, tenant/RBAC correctness, browser-origin trust, presentation, naming, placement, structure, or wording except where it directly changes this lifecycle boundary.

## Authority and Source Coverage

| Boundary | Authority | Source and caller coverage |
|---|---|---|
| Cookie-backed BFF session and no server-side session store | Spec REQ-009, task outcome/non-goals, research report §3.2 | `browser-sessions.ts` in full; `hooks.server.ts`; tenant layout; login, callback, recovery, switch, and logout actions; deletion of `server-sessions.ts` and `/login/complete` |
| Refresh-token concurrency and renewal | Spec REQ-012 and REQ-016; task scenarios 1, 3, and 4; RFC 6749 §6 through `openid-client` | `BrowserSessions.establish`, `cache`, `access`, `read`, `logout`, and `switch`; production journey helpers and assertions |
| Replica and restart behavior | AC-007; task scenario 1 | `identity_ui_e2e.rs::production_ui_bff_journey`; `WyrdTestServer::start_replica` and `start_bound_replica`; BFF-to-server routing in `production-auth.integration.test.ts` |
| Connection lifecycle cutoff | REQ-016 and AC-007 | deactivation assertions in `production SSO crosses replicas`; activation/replacement/removal assertions in `production provider replacement settings`; server renewal refusal through the shared refresh authority |
| Logout scope | REQ-009; FIND-TASK-003-18 scenario; approved expected bearer-token behavior | `BrowserSessions.logout`; two-login journey assertions; cache-hit and uncached-replica behavior |
| FIND-TASK-010-1 closure | Lead direction and TASK-011 acceptance evidence | `BrowserSessions.begin`, `complete`, `access`, and `logout`; real-server `production_ui_bff_journey` exercises code + PKCE, refresh, and revoke using `openid-client` 6.8.8 |
| API-key recovery lifecycle | REQ-010 and lead-approved tenant-authority decision | recovery action, `signInWithApiKey`, `exchange`, cache/renewal/logout paths, and `OIDC-off credential UI` |

Applicable repository authority read: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/wyrd-security-posture.md`, `architecture/references/languages/spec-driven-development.md`, `architecture/references/languages/typescript-guide.md`, and `architecture/references/languages/testing-workflows.md`. The task-specific revision-11 spec and standards recommendation control the nonrotating confidential-client refresh behavior.

## Lifecycle and Failure-Path Assessment

| Lifecycle or failure path | Evidence | Assessment |
|---|---|---|
| Login completes on a different BFF from the one that began it | The sealed `wyrd_login` cookie carries tenant, PKCE verifier, and state; `complete` decrypts it with the shared client-secret-derived key. The journey begins on BFF 1 and redeems on BFF 0. | PASS |
| A replica or restarted BFF has no cached access token | `read` decrypts the session cookie and `access` calls `refreshTokenGrant` on cache miss. The second BFF serves a cookie it never cached. Loss of the process-local map therefore loses only a performance cache, not the session authority. | PASS |
| Concurrent replicas or tabs renew the same session | Both may present the same confidential-client refresh token. TASK-010 deliberately leaves `wyrd-ui` refresh tokens nonrotating, so neither result invalidates the other and no client coordination or server session store is required. | PASS |
| Concurrent refresh and logout | Logout clears the browser cookie, deletes the local cache entry, and revokes the refresh token. A refresh completed before revocation may have minted a self-contained access token; it remains valid only until its normal expiry. A refresh after revocation is refused. This is the approved OAuth behavior and is not a finding. | PASS |
| A different BFF retains a cached token after logout | The browser no longer sends the cookie in the ordinary flow. If the old cookie is replayed to a BFF with a cached token, that access token may remain usable until expiry; no revocation list or coordination was approved. This is explicitly an expected limit, not a gap. | PASS |
| Logout affects one login | Refresh tokens are per login. Revoking the first cookie's token leaves the second login's refresh token renewable; the journey proves this on an uncached replica. | PASS |
| Process restart or rolling replacement | Cookie encryption material is derived from the deployment's shared BFF client secret, while access state is reconstructible through refresh. The fresh second BFF is the relevant restart-equivalent cache-loss proof. | PASS |
| Connection deactivation, replacement, or removal | Cached access keeps its bounded snapshot authority; after expiry/margin, refresh is refused and `read` clears the cookie. The journey proves deactivation, replacement, retired-connection removal, and renewed sessions on either replica. | PASS |
| API-key recovery logout | The cookie is cleared and the process-local cache entry is removed; logout deliberately does not revoke the durable operator key. Subsequent cache miss re-exchanges only while the server still accepts that credential. | PASS, except for the independently invented cookie lifetime in SESSION-1 |
| Cache resource lifetime | Expired entries are removed opportunistically and the per-process map has a fixed capacity. Eviction only causes a standard refresh or exchange; it does not destroy durable authority. | PASS |
| Two-Wyrd-replica seam | `start_bound_replica` shares the Postgres fixture and artifact storage but constructs and binds an independent application state. Each BFF routes `/auth/*` and API traffic to its paired server; the journey crosses both pairings. | PASS |

## Material Proposed Finding

### SESSION-1 — DRIFT: the BFF inspects an opaque refresh credential and invents a 12-hour API-key session lifetime

- **Violated obligation:** TASK-011 requires standard `openid-client` refresh behavior and a `jose`-encrypted cookie containing the renewal credential, under the standing direction to use those libraries as documented and add no mechanism or setting absent from the standards or comparable projects. OAuth refresh tokens are client-opaque credentials; their representation is not a BFF session-lifetime contract. REQ-010 likewise delegates API-key authority and expiry to Wyrd's existing credential/exchange authority and does not approve a second client-side 12-hour lifetime.
- **Exact location:** `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:13-14` and `:246-263`, especially `decodeJwt(credential).exp` for refresh sessions and `apiKeyLifetimeSeconds = 12 * 60 * 60` for recovery sessions.
- **Evidence:** `BrowserSessions.establish` treats a Wyrd refresh token as a JWT, reads its unverified `exp`, and makes that internal representation the JWE and HTTP-cookie lifetime. For API-key sessions, it instead creates a new twelve-hour cutoff that appears in neither the approved specification nor the cited OAuth sections. `openid-client` accepts a refresh-token string and does not require the client to decode it; the server already owns refresh-token expiry/revocation and API-key expiry/revocation. The real-server journey happens to issue JWT-shaped refresh tokens, so it cannot falsify this coupling.
- **Observable consequence:** a standards-conforming opaque refresh token makes an otherwise successful code grant fail while establishing the browser session. Independently, a valid operator recovery credential is forcibly signed out after twelve hours even when Wyrd still accepts it. These are public lifecycle behaviors introduced by BFF-only policy rather than by the server's credential authority.
- **Required testable correction:** delete both client-invented lifetime mechanisms. Keep refresh tokens and API keys opaque inside a conventional encrypted browser-session cookie, and let the existing server refresh/exchange/revocation and credential-expiry paths decide whether the session can continue; a refusal still clears the cookie through `read`. Add focused TypeScript proof that session establishment accepts a non-JWT refresh-token string and that API-key session continuity has no separate twelve-hour BFF cutoff, then run the task's narrow UI check/test lanes. Do not add a server endpoint, refresh-token claim, session store, coordination protocol, or replacement lifetime setting.

## Non-Findings and Accepted Limits

- A self-contained access token already minted before logout or connection retirement remains valid until expiry. Requiring immediate invalidation would contradict the approved behavior and introduce an unapproved coordination or revocation mechanism.
- Multiple simultaneous refreshes are acceptable because `wyrd-ui` refresh tokens do not rotate. A single-flight lock, lease, cross-replica cache, or server session table would be drift and is not required.
- Cache eviction, process restart, or replica routing can cause an additional refresh but no session loss while the server still accepts the renewal credential.
- The shared encrypted cookie, rather than cache contents, is the cross-replica session. No durable client-side session store is missing.
- FIND-TASK-010-1 has the requested closure path: the production BFF uses `openid-client` for code + PKCE, refresh, and revocation against the real Wyrd server. SESSION-1 does not invalidate that protocol-path proof; it is a separate client-lifecycle drift.
- No placement, naming, structure, wording, or style-only concern is proposed as blocking.

## Verification Evidence and Limits

The task records successful completion of the focused production UI identity journey, UI type check, full UI Vitest lane, exact changed UI tests, Rust format/lints, and `git diff --check`. Source inspection confirms that the journey host selects the four named production UI tests, starts two independently bound Wyrd server application states over shared durable storage, and starts two Node BFF processes with the same cookie/client secret.

This domain review did not rerun the environment-owning Keycloak/Dex/Postgres journey. Per task scope, unfiltered identity and every-language journeys belong to change review. The existing journey does not issue an opaque refresh token and therefore does not cover SESSION-1's standards-interoperability failure. No other verification limit prevents a lifecycle verdict.

## Overall Result

**FAIL**

The required cookie-based, cross-replica, nonrotating-refresh lifecycle is otherwise coherent and credibly exercised, including FIND-TASK-010-1 closure. SESSION-1 is material because it adds two unapproved public session-lifetime behaviors and couples the BFF to a refresh-token representation that standard OAuth clients treat as opaque.
