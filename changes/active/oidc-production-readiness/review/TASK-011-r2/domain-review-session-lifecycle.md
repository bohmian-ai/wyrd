# TASK-011 browser-session lifecycle domain review — round 2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `4d468b33e49de4dd9df30c5dd046a334569465bf`
- Candidate tree: `87acdce15e3ca6ea2b6016969695db398ab2d598`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Prior review: `changes/active/oidc-production-readiness/review/TASK-011-r1/`
- Remediation: `changes/active/oidc-production-readiness/review/TASK-011-r1/TASK-011-R1-browser-session-standards-and-logout.md`
- Lead direction: `changes/active/oidc-production-readiness/review/TASK-011-r1/lead-direction-FIND-TASK-011-2.md`

`HEAD` and its tree matched the candidate before and after inspection and focused verification. This review treats `FIND-TASK-011-2` as reversed: logout always clears local browser and cache state, and RFC 7009 revocation is best-effort. It does not reopen the rejected retry-cookie behavior. The recorded lead decisions also stand: routine login is SSO with API-key recovery on its separate page, and an API-key session takes its effective tenant authority from the exchanged token.

## Reviewed boundary and authority

This domain review covers the production browser-session lifecycle, concurrency, and recovery behavior:

- the sealed login transaction and per-tenant encrypted session cookies;
- authorization completion and API-key recovery establishment;
- process-local, credential-hash-keyed access-token caching, expiry, and eviction;
- cache-miss refresh or API-key exchange across tabs, BFF replicas, and process replacement;
- request reads, tenant switching, metadata projection, terminal renewal refusal, and logout;
- provider or authorization-server outage, connection replacement, revocation failure, cookie replay, and service recovery.

The governing authorities were `AGENTS.md`, `architecture/agent-rules.md`, `architecture/references/languages/spec-driven-development.md`, the revision-11 specification (REQ-006, REQ-009, REQ-010, REQ-015, REQ-016, INV-001, INV-005, AC-001, AC-002, AC-003, AC-007), TASK-011, the standards recommendation at `changes/active/oidc-production-readiness/research/auth-standards-recommendation.md` section 3.2, RFC 6749 refresh-token semantics, RFC 7009 revocation, the lead reversal, and the prior review/remediation record. The standing human direction was applied: no nonstandard lock, retry protocol, durable logout state, shared cache, server-side session store, setting, option, or other bespoke mechanism is required.

## Source coverage

| Boundary | Source and caller evidence | Assessment |
|---|---|---|
| Login state and completion | `browser-sessions.ts:10-17,172-230`; `routes/t/[tenantKey]/login/+page.server.ts:25-36`; `routes/login/callback/+server.ts:5-12` | The five-minute encrypted login cookie carries tenant, PKCE verifier, and state, is readable by either replica sharing the deployment secret, and is deleted before callback completion returns. Code redemption and state/PKCE validation remain owned by `openid-client`. |
| Session establishment | `browser-sessions.ts:235-262`; `browser-sessions.test.ts:56-92` | Both credential kinds are sealed into a twelve-hour stateless application session. The refresh credential is no longer decoded or otherwise interpreted, and the exact opaque string is later forwarded to `refreshTokenGrant`. |
| Access-cache lifecycle | `browser-sessions.ts:264-310` | The cache is process-local, keyed by SHA-256 of the renewal credential, opportunistically removes expired entries, and has a fixed 10,000-entry cap. A stale, evicted, or absent entry triggers the standard refresh or exchange path; it is not durable session authority. |
| Request read and routing | `browser-sessions.ts:313-334`; `hooks.server.ts:10-31`; `routes/t/[tenantKey]/login/+page.server.ts:8-22` | A valid tenant-bound cookie plus cached or renewed access token creates the request session. Missing, undecryptable, crossed, expired, or terminally refused sessions are cleared for that tenant. An upstream failure propagates without deleting the cookie. |
| Tenant switch and metadata | `browser-sessions.ts:360-383`; `routes/+page.server.ts:137-153`; `routes/t/[tenantKey]/+layout.server.ts:5-13` | Metadata lists only valid encrypted per-tenant session cookies; it does not grant authority. Switching invokes `read` for the target cookie and therefore revalidates through the server before entry. |
| Logout | `browser-sessions.ts:336-358`; `routes/+page.server.ts:170-187`; `browser-sessions.test.ts:94-113` | Local cookie and matching cache state are always removed first. Refresh-token revocation uses `openid-client` and failures are reduced to a token-free warning; API-key logout never revokes the operator key. |
| Cross-replica and old-connection proof | `production-auth.integration.test.ts:341-455,542-606,608-711`; `identity_ui_e2e.rs`; `wyrd-testing/src/server.rs` | The recorded journey uses two BFF processes and two independently bound Wyrd application states over shared storage, proves cache-miss renewal on the other replica, concurrent reads, tenant-separated cookies, per-login revocation, and renewal refusal after deactivation or replacement. |
| Library failure classification | `openid-client` 6.8.8 `refreshTokenGrant`; installed `oauth4webapi` 3.8.8 `checkOAuthBodyError`; Wyrd `auth/oauth.rs:60-71,100-123` | Wyrd's `400 invalid_grant` is a `ResponseBodyError` and ends the unusable session. Wyrd's `500 server_error`, `503 temporarily_unavailable`, transport failures, and discovery failures are not classified as terminal by the BFF, so the request fails upstream while the cookie remains available for a later retry. No extra client error taxonomy is needed. |

## Lifecycle and failure-path assessment

| Lifecycle or failure path | Source evidence and observable result | Result |
|---|---|---|
| Login begins on one BFF and completes on another | `begin` seals all transient client state with the shared key; `complete` decrypts it and delegates code processing to `openid-client`. The production journey begins and finishes on different replicas. | PASS |
| Process restart, rolling replacement, or cache eviction | The cookie holds the renewal credential; cache contents are reconstructible. `access` refreshes or exchanges on a miss. A fresh replica that never saw establishment serves the same cookie in the journey. | PASS |
| Concurrent tabs or replicas read one session | Each process may independently refresh the same confidential-client credential. TASK-010 intentionally makes `wyrd-ui` refresh tokens nonrotating, so concurrent successful uses do not invalidate one another. The journey issues concurrent reads to both replicas. No single-flight lock or shared session state is warranted. | PASS |
| Access-cache expiry | Entries inside the five-second renewal margin are not served. Refresh or exchange overwrites the keyed entry; expired entries are also swept whenever a token is cached. Eviction changes only latency and upstream work. | PASS |
| Authorization server, discovery, or transport outage during read | Discovery failures are removed from the configuration map for a later retry. Network, nonconforming, `500 server_error`, and `503 temporarily_unavailable` failures propagate as upstream errors and do not reach `read`'s cookie-deletion branch. Cached access remains usable until its own bounded expiry; the sealed session can retry after recovery. | PASS |
| Terminal refresh or API-key refusal | Wyrd returns the OAuth `400` refusal that `openid-client` exposes as `ResponseBodyError`; `access` returns `null`, and `read` deletes only the selected tenant's cookie. The focused opaque-token test also proves another browser's cookie is unaffected. | PASS |
| Connection deactivation, replacement, or removal | Each BFF may use an already issued access token until its bounded expiry. Its next cache-miss renewal is `invalid_grant`, which clears that tenant session. The recorded journey proves refusal on both replicas and preservation of the replacement session. | PASS |
| Tenant switch while target authority is live, absent, refused, or temporarily unavailable | `switch` uses the target's own cookie and `read`. A live target opens; absence or terminal refusal goes to its login; a transient upstream failure returns an upstream error without destroying the cookie. | PASS |
| Logout and refresh race | Local logout wins for the ordinary browser because the cookie and this replica's cache entry are removed before revocation. A refresh already completed elsewhere may leave a self-contained access token live only until its normal expiry; a refresh after successful revocation is refused. This is the approved bearer-token limit. | PASS |
| Revocation failure | Per the lead reversal, local sign-out still completes, token-free failure context is logged, and no retry state or failure page is introduced. A replayed cookie must miss this replica's cache and return to the authorization server; if revocation did not commit, its remaining authority is the approved bearer/refresh-credential limit rather than a hidden local session. | PASS |
| Replayed cookie on another replica | Another replica may retain an already issued access token until expiry, exactly as the task's recorded material limit states. Without cached access it must refresh, and successful revocation or connection retirement refuses it. Cross-replica invalidation machinery would be unapproved drift. | PASS |
| Logout of one of two logins | Refresh tokens are per login. The journey revokes and replays the first login while the second login renews successfully on the other replica. | PASS |
| API-key recovery and logout | Recovery exchanges the key through the server; the returned access token remains the effective tenant authority. Logout deletes only local cookie/cache state and does not revoke the durable operator credential. The journey reuses the key after logout. | PASS |

The one global encrypted login-transaction cookie means a later interactive sign-in attempt in the same browser supersedes an earlier pending attempt. That is conventional transient OAuth client state, not durable session coordination, and the candidate adds no custom multi-transaction store or cookie scheme. Existing mix-up proof shows that one transaction's state/verifier cannot redeem another transaction's response. This is not a finding.

## Prior-finding closure

### `FIND-TASK-011-1` — closed

Candidate `browser-sessions.ts:249-262` no longer calls `decodeJwt` on the renewal credential. It applies the accepted twelve-hour application-session bound to the encrypted cookie and preserves the credential string unchanged. `browser-sessions.test.ts:69-92` establishes a session with `opaque-refresh-token-not-a-jwt`, verifies encrypted cookie creation under that bound, observes the same string passed to `refreshTokenGrant`, and proves a later `invalid_grant` clears only the selected session. The producer, cookie, cache-miss consumer, and terminal-refusal consumer all agree on the opaque representation.

### `FIND-TASK-011-2` — reversed by lead direction; not reopened

Candidate `browser-sessions.ts:341-357` implements the approved standard behavior: it always clears the selected cookie and cache entry, then attempts RFC 7009 revocation best-effort. `browser-sessions.test.ts:94-113` proves logout resolves after transport failure, clears local state, logs no credential, and forces a replayed cookie through renewal rather than the removed cache entry. Requiring retry-preserved browser state would contradict the controlling lead direction and introduce the rejected logout mechanism.

## Verification evidence and limits

Fresh narrow verification run for this review:

- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/browser-sessions.test.ts` — 1 file, 2 tests passed.
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` — 0 errors and 0 warnings.
- `git diff --check 7c48ac7c99f018d3993922e63875839f3695c503..4d468b33e49de4dd9df30c5dd046a334569465bf` — passed.

The remediation record also reports both exact focused tests, the full UI Vitest lane, the filtered production UI identity journey, format, and lints as passing. Per current task-review direction, this domain reviewer did not rerun the environment-owning full journey; full journeys belong to change review. The focused test directly covers both remediation changes. The previously recorded two-BFF/two-server journey remains supporting cumulative evidence for replica and connection-lifecycle paths, not a substitute for source inspection.

There is no focused fault-injection test for a `500` or `503` refresh response. That does not limit acceptance: the installed library's status-based error construction and Wyrd's explicit OAuth status mapping make the nonterminal path directly inspectable, and this remediation did not change that path. Requiring a new outage harness or custom classifier would exceed the task and the human standards direction.

## Material proposed findings

None.

No placement, naming, structure, wording, speculative hardening, or optional availability proposal is elevated into a finding. No unearned lifecycle mechanism was found in the remediation or the cumulative task boundary.

## Overall result

**PASS**

The cumulative candidate provides the required stateless encrypted browser session, reconstructible per-replica access cache, nonrotating concurrent refresh behavior, tenant-specific renewal and switch semantics, bounded old-connection authority, and locally reliable best-effort logout. `FIND-TASK-011-1` is closed at its source, `FIND-TASK-011-2` follows the controlling reversal, and no material browser-session lifecycle, concurrency, durability, or recovery defect remains.
