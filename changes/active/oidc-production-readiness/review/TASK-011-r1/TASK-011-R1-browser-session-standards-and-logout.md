---
id: TASK-011-R1
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
requirements: [REQ-009, REQ-016, REQ-021, AC-002, AC-003, AC-007]
depends_on: []
parent_task: TASK-011
remediates: [FIND-TASK-011-1, FIND-TASK-011-2]
---

# Keep refresh credentials opaque and make failed logout retryable

## Subject and authority

- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Review: `changes/active/oidc-production-readiness/review/TASK-011-r1/verdict.md`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Reviewed candidate: `0b8919fff090d4b0109a506711cb119214d231f2`

Implement this remediation against the cumulative TASK-011 candidate and route
the result directly back through `$wyrd-task-review`.

## Issue diagnosis

### `FIND-TASK-011-1` — refresh-token representation coupling

TASK-011 requires the BFF to use the documented `openid-client` refresh path
and prohibits custom OAuth protocol coupling. RFC 6749 section 1.5 treats a
refresh credential as client-opaque, and `openid-client.refreshTokenGrant`
accepts and forwards a string without inspecting its representation.

The current `BrowserSessions.establish` implementation at
`crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:253`
decodes a refresh credential as a JWT and derives the encrypted-cookie lifetime
from its private `exp`. A valid opaque refresh token therefore fails after
authorization-code redemption, before the BFF writes the portable session
cookie. No later consumer needs the token payload; cache misses pass the
original string directly to `refreshTokenGrant`.

The current tests use Wyrd's JWT-shaped refresh token and cannot detect this
private-representation dependency. The API-key session's bounded twelve-hour
application lifetime is not part of this finding: bounded stateless browser
sessions are conventional and that existing bound remains the local
application-session policy.

### `FIND-TASK-011-2` — failed revocation loses the retry credential

TASK-011 and REQ-009 define logout as RFC 7009 revocation of this login's
refresh token plus cookie clearing. RFC 7009 section 2.2.1 requires a client
receiving 503 to assume the token still exists and permits retry.

The current `BrowserSessions.logout` implementation at
`browser-sessions.ts:339` schedules cookie deletion and removes the process
cache entry before awaiting `openid-client.tokenRevocation`. Wyrd can return
503, or transport can fail, without committing revocation. The route then
returns a failed action, but the live browser has already lost the encrypted
cookie containing its sole retry credential while the server may still accept
that refresh token. The successful-revocation journey does not exercise this
failure path.

## Required outcome and recommendation

Keep renewal credentials opaque and use the existing `BrowserSessions` owner,
`jose` encrypted cookie, `openid-client` operations, application-session
lifetime, access cache, and route action. Make only these two corrections:

1. Delete refresh-token payload inspection from session establishment. Apply
   the existing conventional twelve-hour application-session bound to both
   refresh-token and API-key browser sessions. Store and later forward the
   credential unchanged. Wyrd's existing refresh refusal and revocation remain
   the credential authority.
2. For a refresh-token logout, await the existing
   `openid-client.tokenRevocation` call before deleting that session cookie and
   cache entry. Clear both after RFC 7009 success. If revocation fails, leave
   both untouched so the unchanged logout action can be retried. API-key logout
   remains local cookie/cache clearing because signing out must not revoke the
   operator API key.

These corrections sit at the two sources of invalid behavior. They preserve
the portable cookie and existing standard library calls rather than adding
downstream guards or a second lifecycle mechanism.

## Constraints and preserved behavior

- Keep `openid-client` 6.8.8 and `jose`; do not hand-write OAuth calls or token
  cryptography.
- Add no server endpoint, token claim, introspection call, token parser,
  server-side session store, retry loop, durable logout state, cross-replica
  coordination, new lifetime option, configuration setting, or custom OAuth
  error classifier.
- Keep SSO as the primary action and API-key sign-in on its separate recovery
  page.
- Keep API-key session authority bound to the exchanged token's tenant; do not
  add a route-tenant mapping endpoint or claim.
- Preserve Secure, HttpOnly, SameSite=Lax encrypted cookies, PKCE/state,
  per-tenant cookie isolation, access-token caching, refresh on cache miss,
  terminal-refusal cookie clearing, and tenant switch behavior.
- Preserve the expected OAuth rule that a self-contained access token already
  issued before logout or connection replacement remains valid until expiry.
- Preserve per-login revocation: one login's successful logout must not end a
  sibling login.
- Do not change the accepted twelve-hour application-session bound itself.
- Do not run full identity or every-language journeys in this remediation;
  those remain change-review proof.

## Acceptance criteria

1. `FIND-TASK-011-1` closes when session establishment accepts a non-JWT
   refresh-token string, writes the existing encrypted cookie under the
   application-session bound, and later passes that exact opaque string to
   `refreshTokenGrant` without interpreting it.
2. A terminal refresh refusal still clears only the selected tenant session;
   non-terminal upstream behavior is unchanged.
3. `FIND-TASK-011-2` closes when an RFC 7009 revocation failure leaves the
   selected encrypted cookie and cache entry available for a later logout
   retry.
4. A subsequent successful retry revokes the refresh token and clears that
   cookie and cache entry; the existing successful logout and sibling-login
   behavior remain intact.
5. API-key logout still clears local browser/cache state without revoking the
   operator API key.
6. No prohibited mechanism, setting, API, claim, store, coordination path, or
   broad protocol classifier enters the cumulative diff.

## Focused proof and narrow verification

Add focused tests under the owning browser-session module with these exact
outcomes and names:

- `accepts an opaque refresh token and forwards it unchanged`
- `failed refresh-token revocation preserves the session for retry`

Run each exact test:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run \
  src/lib/server/auth/browser-sessions.test.ts \
  -t 'accepts an opaque refresh token and forwards it unchanged'

mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run \
  src/lib/server/auth/browser-sessions.test.ts \
  -t 'failed refresh-token revocation preserves the session for retry'
```

Then run only the lanes covering the remediation write set:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
git diff --check
```

The focused tests must prove the two diagnosed gaps directly; the broad UI
lane is regression evidence, not a substitute. Full identity journeys run once
at change review.

## Remediation Evidence

The lead direction `lead-direction-FIND-TASK-011-2.md` reverses FIND-TASK-011-2. Logout follows standard practice:
- the cookie and the cached access token are always cleared;
- RFC 7009 revocation is best-effort;
- a failure is logged with no token values and is not shown as a failed logout.

So acceptance criteria 3–4 are replaced, and the second focused test is named for the reversed behavior.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1. FIND-TASK-011-1: an opaque refresh token is sealed under the application-session bound and forwarded unchanged | `browser-sessions.ts`: `establish` no longer decodes the refresh token, and `sessionLifetimeSeconds` (12 h) bounds both credential kinds | `browser-sessions.test.ts` `accepts an opaque refresh token and forwards it unchanged`: a non-JWT refresh token yields the cookie with `maxAge` 12 h and reaches `refreshTokenGrant` unchanged | PASS |
| 2. A terminal refresh refusal clears only this session | `access` returns `null` on `ResponseBodyError`; `read` clears that tenant's cookie | Same test: `invalid_grant` clears the refused session's cookie, and another browser's cookie stays | PASS |
| 3–4 (as reversed by lead direction): logout always signs out; revocation is best-effort and logged without secrets | `BrowserSessions.logout` clears the cookie and cache, then calls `tokenRevocation` in a try/catch with a `console.warn` carrying the tenant and the error name only | `browser-sessions.test.ts` `failed refresh-token revocation still signs out`: logout resolves, the cookie is cleared, one warning contains no token, and a replayed cookie renews through the server (the cache entry is gone) and is refused | PASS |
| 5. API-key logout clears local state without revoking the key | `logout` returns before revocation for `kind === 'api_key'` | Journey `OIDC-off credential UI`: the key still exchanges after logout | PASS |
| 6. No prohibited mechanism enters the diff | No retry, store, setting, endpoint, claim or parser added; the JWT decode is deleted | Diff review | PASS |
| Lead: the mix-up helper asserts the server's one response | `expectMixedCallbackRefused` requires 303 to `/login/callback` with `error=access_denied` and no `code` | Filtered UI journey: 4/4 tests passed | PASS |

Commands (all exited 0):

- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/browser-sessions.test.ts -t 'accepts an opaque refresh token and forwards it unchanged'`
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/browser-sessions.test.ts -t 'failed refresh-token revocation still signs out'`
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` (0 errors)
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test` (33 files, 179 tests)
- `CARGO_TARGET_DIR=… mise exec -- env WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER=production_ui_bff_journey mise run test:identity:journey` (4/4 UI journey tests passed; the host test passed)
- `CARGO_TARGET_DIR=… mise run fmt`
- `CARGO_TARGET_DIR=… mise run lints`
- `git diff --check`
