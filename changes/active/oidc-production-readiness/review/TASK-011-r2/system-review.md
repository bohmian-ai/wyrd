# System-resilience review — TASK-011 remediation round 2

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

The candidate remained at the stated commit and tree throughout this review. I
reviewed the complete base-to-candidate range and used the latest remediation
diff to locate the changed failure and recovery paths. The lead reversal of
`FIND-TASK-011-2` is controlling authority: browser logout always clears local
cookie/cache state and RFC 7009 revocation is best-effort. I did not reopen the
superseded retryable-logout proposal. The recorded decisions that SSO is the
primary login, API-key recovery is a separate page, and the exchanged token is
the API-key session's tenant authority also stand.

## Deployed path and lifecycle ownership

The deployed path remains:

`browser -> gateway/load balancer -> either SvelteKit BFF replica -> that replica's Wyrd endpoint -> either Wyrd replica/Postgres`, with Wyrd federating to the tenant's provider during interactive authorization.

- `BrowserSessions.begin` and `complete` own browser login state, authorization
  code redemption, and creation of the encrypted tenant cookie. Login state is
  a five-minute JWE, so loss or restart of either BFF process does not lose the
  browser-held state.
- The session JWE contains the tenant, credential kind, and opaque renewal
  credential. All replicas derive its `A256GCM` key from the shared BFF client
  secret. The remediation no longer decodes a refresh credential and applies
  the existing twelve-hour application-session bound to either credential
  kind.
- Each process owns only a bounded access-token cache keyed by a credential
  digest. `BrowserSessions.read`, reached from the production hook and login
  route, uses a live cached token or renews through Wyrd on cache miss or near
  expiry. A cache is disposable process state rather than session authority.
- `BrowserSession.api` owns downstream calls and bounds them with a 30-second
  request timeout. `openid-client` owns discovery, code, refresh, exchange, and
  revocation protocol operations; no replacement protocol or recovery
  mechanism was added.
- `BrowserSessions.logout`, reached only from the root logout action, decrypts
  the selected tenant cookie, schedules its deletion, deletes that process's
  cached access token, skips revocation for API-key sessions, and attempts RFC
  7009 revocation for refresh sessions. A revocation exception is contained in
  the request, logged by error name and tenant without credential material, and
  does not turn local logout into a failed action.
- The journey host runs two BFF processes in front of separate Wyrd replicas
  sharing durable server state. Its production journey redeems on one BFF and
  renews/uses the same encrypted session on the other, demonstrating that no
  BFF-local durable state is required.

## Failure and recovery assessment

| Failure or transition | Source evidence | System behavior and recovery assessment |
|---|---|---|
| BFF crash, restart, cache eviction, or rolling replacement | `browser-sessions.ts:98-110,120-125,249-277,293-334`; `identity_ui_e2e.rs:377-520`; production journey cross-replica assertions | The browser-held JWE survives while the per-process cache may disappear. A cold replica renews with the unchanged opaque refresh credential or re-exchanges the API key. Confidential-client refresh credentials do not rotate, so concurrent replicas need no lock or shared cache. The remediation removes the private JWT-shape dependency that previously could prevent recovery with a valid opaque credential. |
| Wyrd replica restart/replacement or a request landing on the other replica | `browser-sessions.ts:128-164`; journey host's two server/BFF pairs | Discovery and OAuth operations route the public issuer to the configured internal Wyrd endpoint. Durable grant and connection authority remains server-owned. A failed cached discovery promise is removed, so the next request retries after dependency recovery rather than preserving a rejected promise. |
| Wyrd transport outage or timeout during refresh/API-key exchange | `browser-sessions.ts:279-311,324-334`; installed `openid-client` bounded request behavior | Transport and nonconforming 5xx failures surface as the canonical upstream error; they do not take the terminal `ResponseBodyError -> null` path and therefore do not clear the encrypted cookie. The failed request is isolated, and a later request can recover with the same credential. |
| Definite refresh or API-key refusal | `browser-sessions.ts:279-311,324-334`; `browser-sessions.test.ts:70-92` | A standard OAuth refusal returns no access authority and `read` clears only the selected tenant cookie. The focused opaque-token test proves terminal refresh refusal still ends that session while another browser session remains. This is the required fail-closed boundary, not a process or service crash. |
| IdP outage | `BrowserSessions.begin`/`complete` and server-owned authorization flow | A new interactive login fails closed and can be restarted. Existing sessions renew against Wyrd without revisiting the provider, so an IdP outage does not take established BFF sessions or unrelated tenants offline. |
| Provider connection replacement/deactivation/removal | `browser-sessions.ts:293-334`; `production-auth.integration.test.ts:608-710`; REQ-016 | A warm replica may use the already-issued access token only for its approved bounded lifetime. On renewal, the server refuses old-connection authority; the BFF clears that tenant cookie and requires login through the active connection. Other tenant cookies and replacement-provider sessions remain available. |
| Revocation endpoint outage, 5xx, timeout, or BFF-to-Wyrd refusal during logout | `browser-sessions.ts:336-358`; `browser-sessions.test.ts:94-113`; lead direction | Local cookie and process-cache deletion occur before the best-effort revocation attempt. The exception is caught, no credential is logged, and the action continues to its normal redirect. Thus a dependency failure does not keep a user visibly signed in or fail the shared BFF process. A copied cookie or another replica's cached self-contained access token can remain effective only within the already approved bearer-token/session bounds; this is the explicitly accepted standard consequence, not a missing recovery mechanism. |
| BFF crash while processing logout | `routes/+page.server.ts:170-186`; `BrowserSessions.logout` | Like ordinary browser logout in comparable systems, completion is acknowledged only when the HTTP response carrying cookie deletion reaches the browser. No extra durable logout transaction, retry store, replica broadcast, or tombstone is justified by the approved task or common practice. Server-side revocation remains best-effort. |
| API-key recovery when SSO is absent or unavailable | `login/api-key/+page.server.ts:14-32`; `browser-sessions.ts:235-247,279-300,341-358` | The recovery page is separate from routine SSO. Exchange failure affects that request only. Once established, route text does not become tenant authority: the server-issued access token scopes subsequent calls. Logout clears local state without revoking the operator API key, preserving the recovery capability. |
| Malformed, expired, wrong-key, or cross-tenant cookie | `browser-sessions.ts:166-187,313-334,366-383` | Authenticated decryption failure or tenant mismatch yields no session and clears only the selected invalid cookie. Metadata removes invalid sibling cookies. No malformed cookie can crash the process or become a fallback tenant identity. |
| Direct authenticated Wyrd API outage | `BrowserSession.api`, `browser-sessions.ts:80-95` | The request is bounded to 30 seconds and maps transport failure to the upstream problem. It does not delete the renewable browser credential, so subsequent requests recover when Wyrd does. |

## Affected capabilities and recovery proof

The remediation directly affects production SSO establishment and logout. It
also shares the application-session lifetime with API-key recovery but does not
change that recovery flow's authority or renewal behavior. The opaque refresh
credential remains usable after process restart, cross-replica routing, and
cache eviction. Failed RFC 7009 revocation now follows the lead-approved local
logout boundary: browser and current-process state end, while the outage is
contained and recorded without secrets.

Evidence available in the task records the exact opaque-refresh and failed-
revocation tests, the UI type/check lane, the full UI test lane, the filtered
production journey, Rust format/lints for the journey host, and
`git diff --check`, all passing. I independently ran the narrow owning test
file at the immutable candidate:

```text
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run \
  src/lib/server/auth/browser-sessions.test.ts

Test Files  1 passed (1)
Tests       2 passed (2)
```

This focused proof directly covers the remediation's failure boundaries. Per
the task-review direction, unfiltered identity and every-language journeys are
change-review evidence and are not required in this remediation round. The
existing filtered production journey remains the credible cumulative evidence
for the deployed two-BFF/two-Wyrd topology.

## Material proposed findings

None.

The prior system proposal's refresh/exchange outage branch remains rejected by
the r1 source validation: Wyrd 500/503 responses do not become the
`ResponseBodyError` branch that deletes a cookie. Its failed-revocation branch
is superseded by explicit lead direction and spec revision 8's standard local-
first, best-effort logout semantics. The candidate implements that direction
without adding a retry loop, durable logout state, server-side session store,
health mechanism, setting, cross-replica coordination, or custom OAuth error
classifier.

## Overall result

**PASS**

The cumulative candidate preserves the portable, stateless cross-replica
session model, restores recovery for standard opaque refresh credentials, and
contains logout revocation failures at the request boundary while always
ending local browser/process state as directed. No reachable process,
dependency-outage, restart, or rolling-replacement path requires a material
system-resilience finding.
