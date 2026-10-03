# TASK-011 structured findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `0b8919fff090d4b0109a506711cb119214d231f2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Prior closure direction: `changes/active/oidc-production-readiness/review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md`

The candidate remained the stated commit throughout validation. I read the
complete cumulative diff, every required discovery report and the focused
follow-up report, then checked each proposal against the actual candidate,
callers, sibling consumers, server error producers, installed dependency
source, approved authority, and applicable standards. The repository has no
`.codegraph/` directory.

## Proposal validation

### `OIDC-SEC-001`, `SESSION-1`, and `FOLLOWUP-011-1`

**REVISED.** The common source is retained only as refresh-token
representation coupling. The proposed API-key-lifetime half is rejected.

#### Producer-to-consumer trace

1. `BrowserSessions.complete` receives the `refresh_token` string from
   `openid-client.authorizationCodeGrant` and is the only refresh-session
   caller of `BrowserSessions.establish`
   (`crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:213-229`).
2. `establish` calls `jose.decodeJwt` on that string, derives the encrypted
   JWE and HTTP-cookie lifetime from its unverified private `exp`, and refuses
   the otherwise successful login if the token is opaque or has no positive
   `exp` (`browser-sessions.ts:246-263`).
3. The resulting cookie is the sole cross-replica refresh authority. Its JWE
   expiry is enforced by `jwtDecrypt` in `unseal`; its browser lifetime is
   enforced by `Max-Age`; `read`, `switch`, `metadata`, and `logout` are the
   sibling consumers (`browser-sessions.ts:177-184,315-377`).
4. On a cache miss, `access` passes the original credential string unchanged
   to `openid-client.refreshTokenGrant`; neither that library operation nor
   any other UI caller needs its JWT payload (`browser-sessions.ts:295-313`).
   The only two cookie writers are the authorization-code completion path and
   API-key recovery path.

RFC 6749 section 1.5 defines a refresh token as a string usually opaque to the
client. The installed `openid-client` 6.8.8 declaration and implementation
accept a `string` and forward it to the refresh grant without decoding it.
Wyrd currently issues JWT-shaped refresh credentials and the server privately
uses that shape to locate tenant state, but revision 11 does not expose the
shape as a BFF contract; REQ-021 instead requires a standard OAuth client to
work unchanged. `jose.decodeJwt('opaque-refresh-token')` throws, making the
path reachable immediately after a successful standard code grant.

The deletion ladder therefore stops at the already-installed standard client:
delete the refresh-token inspection and keep passing the credential opaquely.
No parser, claim, endpoint, introspection call, durable store, or coordination
mechanism is needed. The existing twelve-hour application-session bound used
for API-key recovery is also the server's existing `wyrd-ui` absolute session
bound and is a conventional encrypted-cookie session lifetime. Comparable
projects such as OAuth2 Proxy and `express-openid-connect` have independent
absolute cookie/application-session lifetimes. Its presence is not the
standards-or-conventional-project absence required for `DRIFT`, so that half
of both discovery findings is rejected. The minimum correction can apply the
existing local application-session bound to refresh sessions without
inspecting either credential kind.

### `SYSTEM-011-1` and `FOLLOWUP-011-2`

**REVISED.** The refresh/API-key exchange outage branch is rejected. The
logout ordering on failed RFC 7009 revocation is retained.

#### Refresh and API-key exchange branch

The installed `oauth4webapi` 3.8.8 implementation constructs
`ResponseBodyError` only for OAuth error bodies on HTTP 4xx responses. A 500
or 503 response becomes `OperationProcessingError` with
`OAUTH_RESPONSE_IS_NOT_CONFORM`; `openid-client` forwards that error from both
`refreshTokenGrant` and `genericGrantRequest`. Wyrd emits `server_error` with
500 and `temporarily_unavailable` with 503
(`crates/wyrd/wyrd-server/src/auth/oauth.rs:60-70,100-120` and
`crates/wyrd/wyrd-server/src/components/auth/routes.rs:81-105`). Those failures
therefore take `reject('upstream')`, not the `null` branch in `exchange` or
`access`, and `read` does not delete the cookie. The proposed rolling-outage
session-loss path is not reachable with the reviewed server and installed
dependency. No other non-terminal 4xx producer was established. This portion
is rejected rather than converted into a speculative error classifier.

#### Logout branch

1. The root production logout action is the only caller. It awaits
   `BrowserSessions.logout`; on an upstream error it returns a failed action
   rather than redirecting (`crates/wyrd/wyrd-server/wyrd-ui/src/routes/+page.server.ts:170-186`).
2. `logout` decrypts the selected tenant cookie, immediately schedules its
   deletion, immediately deletes the credential-hash cache entry, and only
   then awaits `openid-client.tokenRevocation`
   (`browser-sessions.ts:338-350`). API-key logout takes only the local-clear
   branch; refresh logout reaches revocation.
3. Wyrd's `/auth/revoke` owner can return 503 when its store or transactional
   audit path fails without committing revocation
   (`crates/wyrd/wyrd-server/src/auth/cli_login.rs:265-307`). The installed
   `tokenRevocation` operation rejects for that non-200 response.
4. RFC 7009 section 2.2.1 requires the client to assume the token still exists
   after 503 and permits retry. The encrypted cookie is the only portable
   holder of that token. Because SvelteKit retains cookie mutations on a
   failed action response, the current ordering removes the browser's retry
   authority even though the server left the token renewable. The cache
   deletion does not repair that loss, and another BFF or a copied old cookie
   can still refresh until successful revocation or expiry.
5. The replaced owner awaited its server logout operation and cleared the
   cookie only after success or the idempotent already-ended response
   (`server-sessions.ts` at the base commit, lines 247-263). The current
   success journey proves only the 200 path and has no revocation-failure
   assertion.

The minimum correction is ordinary sequencing, not a new recovery mechanism:
for a refresh session, await the existing `openid-client.tokenRevocation`
operation before deleting that cookie and cache entry. Its RFC 7009 200 result
already covers an unknown or previously revoked token. A failed operation
leaves the current cookie untouched so the existing action can be retried;
there is no retry loop, new state, endpoint, error type, option, or setting.
API-key logout remains a local clear. This correction does not affect the
approved rule that already issued self-contained access tokens remain valid
until expiry.

## Rejected or accepted discovery claims

| Discovery claim | Validation | Source result |
|---|---|---|
| Decode a refresh credential as a JWT to obtain cookie expiry | **REVISED and retained** | Reachable standards-interoperability failure; consolidated as `FIND-TASK-011-1` |
| Fixed twelve-hour API-key application-session lifetime | **REJECTED** | Bounded cookie/application-session lifetimes are conventional; no standards-or-comparable-project absence was proved |
| Wyrd 500/503 refresh or API-key exchange response becomes `ResponseBodyError`, then cookie deletion | **REJECTED** | Installed `oauth4webapi` classifies those statuses as `OperationProcessingError`; the alleged deletion branch is unreachable |
| Clear refresh cookie/cache before an uncommitted RFC 7009 revocation | **REVISED and retained** | Reachable on Wyrd 503/transport failure; consolidated as `FIND-TASK-011-2` |
| Immediate invalidation of cached self-contained access tokens after logout | **REJECTED / approved limit** | Explicitly excluded by human direction and REQ-016; no coordination or revocation mechanism may be added |
| New API-key tenant endpoint or claim | **REJECTED / approved non-goal** | Token tenant remains authoritative through the existing exchange and server authorization path |
| Placement, naming, structure, and wording notes | **NON-BLOCKING** | They have no behavioral, security, tenancy, durability, or public-contract consequence |

## Final deduplicated finding ledger

### `FIND-TASK-011-1` — refresh-token representation coupling

- **Discovery sources:** `OIDC-SEC-001`, `SESSION-1`, `FOLLOWUP-011-1`
- **Validation status:** **REVISED**
- **Classification:** `DRIFT`
- **Violated obligation:** TASK-011 requires the documented `openid-client`
  refresh path and prohibits custom OAuth protocol coupling; revision-11
  REQ-021 requires a standard OAuth client to work unchanged. RFC 6749 section
  1.5 treats the refresh credential as client-opaque.
- **Exact location:**
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:253-257`.
- **Evidence:** `establish` calls `decodeJwt(credential).exp` only for refresh
  sessions. `openid-client.refreshTokenGrant` accepts and forwards a string;
  no consumer needs the payload. A conforming opaque string throws before the
  encrypted cookie is written even though code redemption succeeded.
- **Observable consequence:** a private change from JWT-shaped to opaque Wyrd
  refresh tokens, with no OAuth wire-contract change, breaks production BFF
  sign-in before the standard client can use the valid credential.
- **Decision-complete minimum correction:** delete refresh-token payload
  inspection. Keep the credential opaque in the existing encrypted cookie and
  use the already-present conventional twelve-hour application-session bound
  for both session credential kinds. Keep Wyrd refresh refusal and revocation
  as credential authority. Do not add a server endpoint, token claim,
  introspection call, parser, session store, coordination mechanism, new
  lifetime option, or setting.
- **Focused closure proof:** a focused UI/Vitest case supplies a non-JWT
  refresh-token string from the standard grant result, establishes the
  encrypted cookie, and later passes the same opaque string unchanged to
  `refreshTokenGrant`; a terminal server refusal still clears only that
  session. Run that exact test plus the narrow UI `check` and `test` lanes.
  Full identity journeys remain change-review proof.

### `FIND-TASK-011-2` — failed revocation loses the only retry credential

- **Discovery sources:** `SYSTEM-011-1`, `FOLLOWUP-011-2`
- **Validation status:** **REVISED**
- **Classification:** `REGRESSION`
- **Violated obligation:** TASK-011 and REQ-009 define logout as RFC 7009
  revocation of this login's refresh token plus cookie clearing. RFC 7009
  section 2.2.1 says a client receiving 503 must assume the token still exists
  and may retry.
- **Exact location:**
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:339-350`,
  reached from
  `crates/wyrd/wyrd-server/wyrd-ui/src/routes/+page.server.ts:170-186`.
- **Evidence:** cookie and cache deletion precede `tokenRevocation`; Wyrd can
  return 503 without committing revocation; the route reports failure after
  the deletion. The base owner cleared only after its server logout operation
  completed. The existing journey covers successful revocation only.
- **Observable consequence:** after a transient failed logout, the refresh
  token remains renewable on the server while the live browser has lost the
  sole credential needed to retry and complete logout; a copied prior cookie
  remains usable subject to the approved access/refresh lifetime boundaries.
- **Decision-complete minimum correction:** for refresh sessions, await the
  existing `openid-client.tokenRevocation` call before deleting the selected
  cookie and its local cache entry. Clear both after its RFC 7009 success;
  preserve them when it fails so the unchanged action can be retried. Keep
  API-key logout as local clearing only. Add no retry loop, durable state,
  endpoint, setting, custom error classifier, or other mechanism.
- **Focused closure proof:** a focused UI/Vitest case drives refresh logout
  through a 503 and asserts the failed action retains the existing cookie,
  then retries against RFC 7009 200 and asserts that the cookie is cleared and
  the existing successful-revocation behavior remains. Run that exact test
  plus the narrow UI `check` and `test` lanes. Full identity journeys remain
  change-review proof.

## Validation outcome

The validated ledger contains two bounded implementation findings:
`FIND-TASK-011-1` and `FIND-TASK-011-2`. They share the `BrowserSessions`
owner but not a root cause and therefore remain separate correction
boundaries. Neither correction requires a new product, public API,
architecture, security, compatibility, concurrency, resource-ownership, or
persistent-data decision. The remaining proposals were rejected from the
ledger rather than preserved as optional advice.
