# TASK-011 focused follow-up review

## Immutable subject and uncertainty reviewed

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `0b8919fff090d4b0109a506711cb119214d231f2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Prior closure direction: `changes/active/oidc-production-readiness/review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md`

`HEAD` was the candidate above before and after this investigation. This pass
investigated only the two assigned `BrowserSessions` lifecycle uncertainties.
It is a discovery pass, not a vote on the task verdict.

## Source and authority inspected

| Boundary | Source inspected |
|---|---|
| Candidate session owner | Complete `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts` |
| Callers and error projection | `src/hooks.server.ts`; `src/routes/+page.server.ts`; `src/routes/login/callback/+server.ts`; `src/routes/t/[tenantKey]/login/+page.server.ts`; `src/routes/t/[tenantKey]/login/api-key/+page.server.ts`; `src/lib/server/auth/session.ts`; `src/lib/server/problem.ts` |
| Candidate proof | Relevant complete paths in `src/lib/server/auth/production-auth.integration.test.ts`; cumulative base-to-candidate diff; TASK-011 implementation evidence |
| Server error producers | `crates/wyrd/wyrd-server/src/components/auth/routes.rs`; `src/auth/oauth.rs`; `src/auth/cli_login.rs` |
| Replaced logout behavior | Base `server-sessions.ts` at `7c48ac7c99f018d3993922e63875839f3695c503` |
| Dependency behavior | Installed `openid-client` 6.8.8 `build/index.js` and declarations; installed `oauth4webapi` 3.8.8 `build/index.js` and declarations; installed `jose` 6.2.12 behavior |
| Approved/local authority | Revision-11 spec, original TASK-011, auth standards recommendation, FIND-TASK-010-1 lead direction, and the five assigned discovery reports |
| Standards and comparable practice | RFC 6749 §§1.5, 5.2, 6; RFC 7009 §§2.1-2.2.1; OAuth2 Proxy's official cookie-session documentation; Auth0 `express-openid-connect` session configuration documentation |

The local dependency source was decisive for the error-class question. In
`oauth4webapi` 3.8.8, `parseOAuthResponseErrorBody` recognizes OAuth bodies
only for HTTP 4xx responses. `checkOAuthBodyError` therefore raises
`ResponseBodyError` for a conforming 4xx OAuth refusal, but raises
`OperationProcessingError` for HTTP 500 or 503. `openid-client` forwards these
classes through `refreshTokenGrant`, `genericGrantRequest`, and
`tokenRevocation`. A direct check against the installed version produced:

```text
400 ResponseBodyError true server_error OAUTH_RESPONSE_BODY_ERROR
500 OperationProcessingError false undefined OAUTH_RESPONSE_IS_NOT_CONFORM
503 OperationProcessingError false undefined OAUTH_RESPONSE_IS_NOT_CONFORM
```

The installed `jose.decodeJwt` likewise throws `JWTInvalid` for an opaque
refresh credential such as `opaque-refresh-token`.

## Claim A — refresh-token decoding and the API-key cookie lifetime

### Resolution

**NARROWED.** The refresh-token half of `SESSION-1` / `OIDC-SEC-001` is
supported; the fixed API-key cookie-lifetime half is not supported as `DRIFT`
under the standing comparable-project direction.

RFC 6749 §1.5 defines a refresh token as a string that is usually opaque to
the client. The documented `openid-client.refreshTokenGrant` contract accepts
that string and sends it to the token endpoint without interpreting its
representation. Candidate `BrowserSessions.establish` instead calls
`decodeJwt(credential)` for every refresh session and uses an unverified
private `exp` claim to construct the JWE expiry and HTTP `Max-Age`
(`browser-sessions.ts:246-263`). A conforming opaque token therefore completes
the standard authorization-code grant and then fails in Wyrd-only session
construction. No approved requirement makes the server's current JWT refresh
encoding a BFF contract. This is a reachable interoperability failure and an
extra mechanism beyond the documented `openid-client` refresh path.

The same conclusion does not extend to `apiKeyLifetimeSeconds`. A bounded
application-session lifetime is conventional in comparable projects:
OAuth2 Proxy's stateless encrypted-cookie session has a configurable cookie
expiry (seven days by default, with zero selecting a browser-session cookie),
and Auth0 `express-openid-connect` has a default absolute session duration.
Those projects keep application-session expiry separate from parsing an OAuth
credential. A fixed local bound can end a browser session before the durable
credential expires without changing that credential's server authority. The
candidate's twelve-hour API-key bound may be a product-policy choice, but it
is not a mechanism absent from standards and comparable projects, creates no
OAuth representation dependency, and must not be retained as a blocking
`DRIFT` finding under the human direction.

### Revised proposed finding `FOLLOWUP-011-1`

- **Discovery sources:** `SESSION-1`, `OIDC-SEC-001` (revised).
- **Classification:** `DRIFT`.
- **Violated obligation:** TASK-011 requires the documented
  `openid-client` refresh path and forbids custom protocol coupling; RFC 6749
  treats the refresh credential as client-opaque.
- **Exact location:**
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:253-256`.
- **Evidence:** `decodeJwt(credential).exp` is neither an `openid-client`
  operation nor a standard refresh-token lifetime field. It throws for a
  standards-conforming opaque token after a successful code grant.
- **Observable consequence:** changing Wyrd's private refresh-token encoding
  without changing its OAuth contract breaks production BFF login even though
  `openid-client` accepted the token and could refresh with it.
- **Smallest testable correction:** remove BFF inspection of the refresh-token
  payload and keep the credential opaque in the existing encrypted cookie.
  Use ordinary application-cookie/session expiry behavior that does not depend
  on a refresh-token claim, while leaving the server's existing refresh refusal
  and revocation as credential authority. Do not add an endpoint, claim,
  introspection call, parser, store, coordination mechanism, or new setting.
  Focused proof should establish a session from a non-JWT refresh-token string
  and show that a later server `invalid_grant` still clears only that session.
  The API-key session's existing conventional local lifetime is outside this
  correction.

## Claim B — OAuth error classification and logout failure ordering

### Refresh and API-key exchange portion

**REJECTED AS REPORTED.** Wyrd's token endpoint emits `server_error` with HTTP
500 and `temporarily_unavailable` with HTTP 503
(`components/auth/routes.rs:98-105`; `auth/oauth.rs:60-70,115-120`). The
installed library does not expose either response as `ResponseBodyError`.
Consequently `exchange` and `access` do not return `null` for those outages:
their non-`ResponseBodyError` branch calls `reject('upstream')`, `read` never
reaches its cookie-deletion branch, and the recovery credential remains in the
cookie for a later request. Transport failures behave the same way. The claimed
outage-to-permanent-sign-out path is therefore not reachable against the
reviewed Wyrd server and installed dependency.

The broad `instanceof ResponseBodyError` checks do include other conforming
4xx OAuth refusals, but the reviewed Wyrd producers give the refresh path
`invalid_grant` for an unusable refresh token and the RFC 8693 API-key path
`invalid_request` for an unusable subject token. No distinct, reachable
non-terminal Wyrd 4xx producer was established by this follow-up, so it would
be speculative to retain a broader error-classification finding.

### Logout portion

**CONFIRMED AND NARROWED.** Candidate `BrowserSessions.logout` deletes the
cookie and cache before awaiting `openid-client.tokenRevocation`
(`browser-sessions.ts:338-350`). Wyrd's revocation endpoint can return HTTP 503
when its store or audit transaction is unavailable
(`auth/cli_login.rs:265-307`). RFC 7009 §2.2.1 says that after a 503 the client
must assume the token still exists and may retry. Here the encrypted cookie is
the approved sole portable holder of that refresh credential. Deleting it
first removes the only in-scope retry authority even though Wyrd did not commit
revocation.

This is also a concrete regression from the replaced owner. Base
`server-sessions.ts:247-263` awaited the server logout, treated only success or
the idempotent already-ended response as completion, and deleted the cookie
afterwards. The original TASK-011 scenario requires both RFC 7009 revocation
and cookie clearing; the candidate-appended sentence that the cookie clears
"at once" is implementation evidence, not higher authority, and cannot
silently redefine failure semantics. Reordering the two existing operations
uses `openid-client` and RFC 7009 exactly as documented and introduces no retry
loop, server state, endpoint, setting, or custom protocol.

### Revised proposed finding `FOLLOWUP-011-2`

- **Discovery source:** `SYSTEM-011-1` (revised to the proven logout path).
- **Classification:** `REGRESSION`.
- **Violated obligation:** TASK-011 logout revokes this login's refresh token
  through RFC 7009 and clears its cookie; RFC 7009 preserves retryability after
  HTTP 503.
- **Exact location:**
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:339-350`.
- **Evidence:** cookie/cache deletion precedes revocation; Wyrd can return 503
  without committing revocation; the former owner cleared only after the
  server operation completed.
- **Observable consequence:** a transient revocation outage reports logout
  failure while leaving the refresh token renewable, but the browser has lost
  its only credential with which to retry. A copied old cookie can still renew
  even though the user cannot complete revocation from the live browser.
- **Smallest testable correction:** for refresh sessions, complete the existing
  `openid-client.tokenRevocation` call before clearing that cookie and local
  cache. Preserve RFC 7009's idempotent 200 behavior and keep API-key logout as
  local clearing only. Add focused BFF proof that a 503 retains the existing
  refresh cookie for retry and that a later 200 clears it; add no retry loop,
  durable store, server endpoint, state, setting, or error mechanism.

This finding does not challenge the approved bearer-token limit: an access
token already cached on another replica remains valid until its own expiry
after successful refresh-token revocation.

## Outcome

**RESOLVED**

The conflicting claims can be decided from source and authority. Claim A
retains only the refresh-token representation coupling; its API-key lifetime
half is rejected because bounded application-session lifetimes are
conventional. Claim B rejects the claimed refresh/exchange outage path because
the installed library does not classify Wyrd's 500/503 responses as
`ResponseBodyError`, while retaining the independently reachable logout-order
regression under RFC 7009.
