# TASK-011 behavior review, remediation round 2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `4d468b33e49de4dd9df30c5dd046a334569465bf`
- Candidate tree: `87acdce15e3ca6ea2b6016969695db398ab2d598`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Prior review: `changes/active/oidc-production-readiness/review/TASK-011-r1/`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-011-r1/TASK-011-R1-browser-session-standards-and-logout.md`
- Lead direction: `changes/active/oidc-production-readiness/review/TASK-011-r1/lead-direction-FIND-TASK-011-2.md`

`HEAD` matched the candidate and its tree before and after this review. The
repository has no `.codegraph/` directory. I reviewed the complete
base-to-candidate range and used `0b8919fff090d4b0109a506711cb119214d231f2..4d468b33e49de4dd9df30c5dd046a334569465bf`
only to locate the remediation changes.

`FIND-TASK-011-2` was not reopened. The governing lead direction reverses its
prior retry-on-failure outcome: browser logout always clears local session
state and attempts refresh-token revocation best-effort. The original task's
lead-approved SSO/recovery-page and API-key tenant-authority decisions were
also treated as fixed authority.

## Navigation and caller map

| Owner or boundary | Caller and consumer paths inspected |
|---|---|
| Browser session owner | `wyrd-ui/src/lib/server/auth/browser-sessions.ts`: `BrowserSessions.begin`, `complete`, `establish`, `access`, `read`, `logout`, `switch`, and `metadata`; `BrowserSession.context` and `api` |
| Request boundary | `hooks.server.ts`, the common `/login/callback`, tenant SSO and API-key recovery routes, root switch/logout actions, and tenant layout metadata |
| Server API consumers | Settings loads/mutations and change actions reached through the token-holding `BrowserSession.api` boundary |
| Browser-visible UI | SSO-first login page, separate API-key recovery page, Shell logout, TenantChooser switch, and page-data projections |
| Replaced behavior | Deleted `server-sessions.ts`, `/login/complete`, flow cookie, private BFF protocol, and custom CSRF token/checks |
| Proof paths | `browser-sessions.test.ts`, `production-auth.integration.test.ts`, `identity_ui_e2e.rs`, `WyrdTestServer::start_bound_replica`, and the identity lane wiring in `mise.toml` |
| Dependency boundary | `openid-client` 6.8.8 and `jose` 6.2.12 in `package.json` and `pnpm-lock.yaml` |

The realistic paths followed were code grant plus PKCE, callback redemption,
cookie transfer across replicas, cached and uncached renewal, terminal renewal
refusal, operator-key exchange, token-derived authorization, tenant switch,
connection replacement, successful logout, failed best-effort revocation, and
replay of a copied pre-logout cookie.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-009: `wyrd-ui` is a confidential OAuth client using authorization code plus S256 PKCE | `browser-sessions.ts:128-164,189-232` uses `openid-client` discovery, `ClientSecretBasic`, library PKCE/state generation, `buildAuthorizationUrl`, and `authorizationCodeGrant`; `/login/callback` delegates redemption to that owner | `production SSO crosses replicas` asserts the standard authorization request, exact callback, redemption, state/PKCE separation, code replay refusal, and cross-replica completion; the original task records the filtered production journey passing | PASS |
| Required library ownership: RFC 8414 discovery, refresh, RFC 7009 revocation, and existing RFC 8693 exchange use `openid-client`, with no handwritten OAuth call | `BrowserSessions.configuration`, `access`, `logout`, and `exchange` call the installed library operations directly; package and lock pin 6.8.8 | Source and dependency inspection; recorded production journey exercises each operation; focused tests mock only the library seam | PASS |
| The browser session is one encrypted Secure, HttpOnly, SameSite=Lax cookie readable by every BFF replica, with no server-side BFF session store | `browser-sessions.ts:23-34,98-187,249-262` seals the renewal credential through `jose` `dir`/`A256GCM`; only configurations and access tokens are kept in bounded process-local maps | Journey helpers assert cookie flags, host-only scope and JWE shape; the production journey redeems on one BFF and reads/renews on another backed by another Wyrd replica | PASS |
| FIND-TASK-011-1: a refresh token remains opaque and the conventional application-session bound applies without inspecting the credential | `establish` at `browser-sessions.ts:249-262` caches and seals the credential unchanged under `sessionLifetimeSeconds`; the only remaining `decodeJwt` decodes the server-issued access token in `cache`, not the refresh token | Independently rerun exact test `accepts an opaque refresh token and forwards it unchanged`: passed; it supplies a non-JWT refresh token, observes the 12-hour cookie bound, and proves the same string reaches `refreshTokenGrant` | PASS |
| A terminal refresh refusal still clears only the selected tenant session; non-terminal upstream failure behavior is unchanged | `access` maps the standard library's `ResponseBodyError` refusal to `null`; `read` deletes only `cookieName(tenantKey)`; other failures remain the existing upstream error path | The same focused opaque-token test forces `invalid_grant`, proves that cookie is cleared, and proves another browser's cookie remains; exact test and full UI test lane passed | PASS |
| Lead reversal of FIND-TASK-011-2: logout always ends the local browser session; refresh-token revocation is best-effort and its failure is not a failed logout | `logout` at `browser-sessions.ts:336-358` deletes the selected cookie and credential-hash cache entry before revocation, catches failure, and returns normally | Independently rerun exact test `failed refresh-token revocation still signs out`: passed; it proves cookie deletion, successful action completion, cache eviction through forced renewal on replay, and a swallowed revocation failure | PASS |
| Failed best-effort revocation logs no credential or token value | The warning at `browser-sessions.ts:352-356` contains the tenant key and error name only | Focused logout test serializes the warning arguments and proves the refresh token is absent | PASS |
| Successful logout remains per-login; already issued bearer access remains valid only for its ordinary bounded lifetime | `logout` revokes only the selected sealed refresh credential and clears only its local cache entry; there is no global user revocation or cross-replica coordination mechanism | Recorded two-login production journey proves the selected refresh cannot renew and the sibling login still can; the accepted material limit is stated in the original task evidence | PASS |
| API-key logout is local only and never revokes the operator key | `logout` returns after local clearing when `sealed.kind !== 'refresh'` | Recorded OIDC-off production journey logs out and then exchanges the same operator key successfully | PASS |
| Lead-approved REQ-010 behavior: routine sign-in is SSO and API-key recovery is a separate page | `/t/[tenantKey]/login` exposes the SSO action and a recovery link; `/login/api-key` owns the API-key form and `genericGrantRequest` exchange | Recorded OIDC-off and SSO production journeys prove the primary/recovery split, inactive-SSO problem path, bad-key refusal, cross-origin refusal, and recovery while SSO is active | PASS |
| Lead-approved API-key authority: the exchanged token's tenant governs and no mapping endpoint or claim is introduced | `BrowserSession.context` projects tenant id, principal and permissions from the server-issued access token; downstream server API calls carry only that token. The route key controls navigation/cookie selection, not server authorization | Recorded cross-tenant recovery case proves another tenant's key cannot see or remove the route tenant's staged connection | PASS |
| No Wyrd/provider token or API key reaches page data, URL, or JavaScript | The access token is `BrowserSession.#token`; `metadata` returns only safe subject, expiry, and tenant names; renewal credentials remain inside the encrypted HttpOnly cookie; caught errors do not project library diagnostics | `expectNoSecrets` inspects rendered pages, Svelte data responses, redirects and cookie history on both replicas; API-key journey separately checks the plaintext key; UI lanes passed | PASS |
| Access-token cache is per-process, keyed by renewal-credential hash, and a miss renews without replica coordination | `hash`, `cache`, and `access` at `browser-sessions.ts:264-311` own the bounded map and standard refresh/exchange path | Production cross-replica journey forces renewal on a BFF that did not redeem the code; focused logout proof demonstrates cache eviction by requiring renewal after replay | PASS |
| REQ-015: tenant switch revalidates the target tenant's own session and prompts for target login when absent | `switch` calls `read(target)`; `sealed` requires payload tenant equality; no source-tenant credential is reused | Recorded multi-provider journey holds independent Keycloak/Dex sessions, switches across replicas, prompts for a missing target session, and rejects forged or crossed cookies | PASS |
| REQ-016: replaced/deactivated connections cannot renew, while a previously issued access token keeps only its approved short lifetime | `access` refreshes at cache miss/expiry and `read` clears the session on terminal refusal; no BFF revocation list or connection-specific state was added | Recorded replacement/deactivation journeys prove the bounded existing-token window, eventual renewal refusal, replay refusal on both replicas, and replacement-session continuity | PASS |
| CSRF uses SvelteKit `csrf.checkOrigin` plus SameSite=Lax; custom CSRF behavior is removed | Custom fields and checks are absent from changed action consumers; cookie options retain SameSite=Lax; no alternative CSRF mechanism or option was added | Built production journey records 403 for cross-origin settings and API-key recovery form posts; UI tests passed | PASS |
| AC-001: OIDC-off real UI works through an existing Wyrd credential | Separate recovery route uses existing token exchange and the same cookie/session owner, without password storage or mock authentication | Recorded journey covers invalid, reader and admin keys, authorized and denied actions, logout, and tenant-authority isolation | PASS |
| AC-002: real-provider sign-in supports an authorized operation and a denied operation | Token-derived context reaches existing settings API through `BrowserSession.api` | Recorded Keycloak journey permits the admin mutation and refuses the reader mutation | PASS |
| AC-003: concurrent tenants/providers remain isolated | Per-tenant sealed cookies and target-specific reads keep sessions separate; there is no provider-specific branch | Recorded Keycloak/Dex journey covers two simultaneous tenant sessions, wrong-provider and same-issuer cross-tenant mix-up refusal, forged/crossed cookies, and target switching | PASS |
| Applicable AC-007: the session works across two BFF/two Wyrd replicas and failed/replayed login paths produce no session | `identity_ui_e2e.rs` starts two BFFs with the shared secret and separate bound Wyrd replicas; common callback and `complete` set no session on failed redemption | Recorded four-case production UI journey covers missing/forged state, state/PKCE mismatch, code replay, cookie forgery/crossing, revocation, and connection replacement. Full journeys remain change-review proof and were not required for this task review | PASS |
| Delete `server-sessions.ts`, the flow cookie, custom CSRF token, and `/login/complete` | Cumulative diff deletes both files and removes their callers, custom fields and checks; repository search found no live private-BFF/flow-cookie references | Cumulative source inspection, successful UI check/test lanes, and diff check | PASS |
| Prohibited scope: no token-bearing page data, server session store, second role mapper, browser-selected effective tenant, `@auth/sveltekit`, `tower-sessions`, handwritten OAuth, or new remediation mechanism/setting/API | `BrowserSessions` projects server claims through installed dependencies and adds no durable state, role mapper, endpoint, token claim, retry, classifier, coordination path, or configuration option | Complete cumulative diff, callers, manifests, lockfile and proof inspection | PASS |
| Standards-first/no unearned mechanism | The candidate uses `openid-client`, `jose`, native SvelteKit origin protection, ordinary bounded cookie lifetime, best-effort local-first logout, and existing server authorization. The remediation deletes refresh-token representation coupling rather than replacing it | Diff and source inspection found no nonstandard compatibility path, duplicated validator, revocation state, logout retry state, provider branch, or extra public option | PASS |
| Write-set and regression closure | Runtime changes remain within the UI owner/consumers, UI journey host/lane, and one narrow testing helper; remediation changes are confined to the existing session owner and its focused proof | Independently rerun UI `check` (0 errors/warnings), UI `test` (33 files/179 tests), both exact remediation tests, and cumulative `git diff --check`; all exited 0 | PASS |

## Prior-finding closure

- `FIND-TASK-011-1` is closed by source and focused proof: refresh credentials
  are not parsed, their string is sealed unchanged, and the same string reaches
  the standard refresh operation.
- `FIND-TASK-011-2` is reversed by lead direction and therefore is not an open
  retry-preservation obligation. The directed replacement behavior is closed:
  cookie and local cache are always cleared, revocation is attempted
  best-effort, failure is redacted and does not fail logout, and no retry/store/
  setting/endpoint mechanism was added.

## Proposed findings

None.

No reachable `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION`
remains. I found no separate caller-specific symptom whose source survived the
remediation. Placement, naming, structure, and wording observations were not
treated as blocking behavior findings.

## Verification assessment

I independently ran the two exact remediation tests, the UI `check` lane, the
complete UI test lane, and cumulative `git diff --check`; all passed. I did not
rerun the production identity journey or a broad repository aggregate. Under
the governing direction those belong to change review, while this task review
uses the narrowest lanes for the remediation write set. The original task's
recorded journey evidence remains credible after source inspection because the
remediation preserves its public success paths and adds focused proof for the
two changed behaviors.

## Overall result

**PASS**
