# TASK-011 review verdict — round 1

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `0b8919fff090d4b0109a506711cb119214d231f2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Prior closure direction: `changes/active/oidc-production-readiness/review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md`

The candidate remained the stated commit through discovery, follow-up, and
structured validation. The complete base-to-candidate range was reviewed.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Use `openid-client` 6.8.8 for discovery, code + S256 PKCE, refresh, and RFC 7009 revocation as `wyrd-ui` | `browser-sessions.ts`; locked `package.json` and `pnpm-lock.yaml` | Recorded production BFF journey; source and dependency inspection | **FAIL** — the protocol operations use the library, but `FIND-TASK-011-1` adds non-standard refresh-token representation coupling and `FIND-TASK-011-2` loses retry authority before revocation succeeds |
| Use a `jose`-encrypted Secure, HttpOnly, SameSite=Lax portable cookie and no server-side session store | `BrowserSessions.seal`/`unseal`; per-tenant cookies; process-local access cache only | Cross-replica recorded journey; UI tests | PASS |
| Two BFF replicas and two Wyrd replicas serve the same session | `identity_ui_e2e.rs`; `WyrdTestServer::start_bound_replica`; cache-miss refresh | Recorded `production_ui_bff_journey`; source inspection | PASS |
| No token reaches page data, URLs, or JavaScript | private `BrowserSession.#token`; centralized safe metadata projection | Recorded secret-scanning journey assertions; UI tests | PASS |
| Tenant switch revalidates through the target tenant's own session | `BrowserSessions.switch` and token-derived context | Recorded multi-provider switch journey | PASS |
| SSO is the primary login action and API-key sign-in is a separate recovery page | `/t/[tenantKey]/login` and `/login/api-key` routes | Recorded OIDC-off and SSO UI journeys; route tests | PASS |
| An API-key session uses the exchanged token's tenant authority, with no endpoint or claim added | token-derived `BrowserSession.tenantId`; server-authorized API calls | Recorded cross-tenant recovery assertion; tenancy review | PASS |
| Replaced or deactivated connection cannot refresh after the access-token window | shared refresh-on-miss path and terminal-refusal cookie clearing | Recorded replacement journey | PASS |
| Logout revokes only this login; an already issued self-contained access token may remain valid until expiry | per-login refresh token revocation; local cookie/cache clearing; no sibling-login invalidation | Recorded two-login journey; approved OAuth limit | **FAIL** — successful logout behavior passes, but `FIND-TASK-011-2` prevents retry after an uncommitted revocation failure |
| Delete the custom BFF protocol, flow cookie, custom CSRF token, and `/login/complete` | deleted `server-sessions.ts` and route; SvelteKit `csrf.checkOrigin`; form cleanup | Cumulative diff, route tests, recorded journey | PASS |
| Close `FIND-TASK-010-1` with the production `openid-client` driver against the real server | `BrowserSessions.begin`, `complete`, `access`, and `logout` | Recorded production journey exercises code + PKCE, refresh, and revoke | PASS |
| Preserve non-goals: no server session store, second role mapper, `@auth/sveltekit`, hand-written OAuth calls, or new API-key tenant contract | cumulative diff and dependency graph | All independent reviews | PASS |

## Independent review results

| Review | Result | Material result |
|---|---|---|
| Behavior implementation review | PASS | No proposed findings |
| Invariant implementation review | PASS | No proposed findings |
| Repository standards review | PASS | No material findings; one non-blocking wording note |
| Maintainer review | PASS | No material findings; non-blocking notes only |
| System-resilience review | FAIL | Proposed `SYSTEM-011-1`, narrowed during follow-up and validation |
| OIDC/browser-security domain review | FAIL | Proposed `OIDC-SEC-001`, revised during follow-up and validation |
| Tenancy/authorization domain review | PASS | No material findings |
| Session-lifecycle domain review | FAIL | Proposed `SESSION-1`, revised during follow-up and validation |
| Structured Ponytail validation | COMPLETE | Retained two revised, deduplicated findings |

## Follow-up decision

A focused follow-up was required because the discovery reports materially
conflicted on `BrowserSessions` credential lifetime and outage behavior. It
resolved the uncertainty from source and installed dependency behavior:

- it retained only refresh-token JWT decoding from the two lifecycle-drift
  proposals and rejected the API-key twelve-hour application-session bound as
  conventional;
- it rejected the proposed refresh/API-key-exchange 500/503 cookie-loss path
  because installed `oauth4webapi` classifies those responses outside the
  terminal `ResponseBodyError` branch; and
- it retained the logout clear-before-revoke path when RFC 7009 revocation
  fails without committing.

The follow-up result was `RESOLVED`. Structured validation independently
confirmed those narrowed results.

## Validated finding ledger

### `FIND-TASK-011-1` — refresh-token representation coupling

**REVISED · DRIFT.** `BrowserSessions.establish` decodes a refresh credential
as a JWT and derives cookie lifetime from its private `exp`. RFC 6749 and the
installed `openid-client` treat the refresh token as an opaque string; no
consumer needs its payload. A conforming opaque token therefore breaks after a
successful code grant. Delete the payload inspection, keep the credential
opaque in the existing encrypted cookie, and apply the existing conventional
application-session bound without adding any endpoint, claim, parser,
introspection call, setting, store, or coordination mechanism.

### `FIND-TASK-011-2` — failed revocation loses the retry credential

**REVISED · REGRESSION.** `BrowserSessions.logout` deletes the cookie and cache
entry before awaiting RFC 7009 revocation. When Wyrd returns 503 or transport
fails before revocation commits, the action reports failure but the browser has
already lost its only credential for retry. For refresh sessions, await the
existing `openid-client.tokenRevocation` operation before clearing cookie and
cache; preserve them on failure. API-key logout remains local clearing. Add no
retry loop, durable state, endpoint, setting, or custom error classifier.

## Prior-finding closure

`FIND-TASK-010-1` is closed by source evidence: the candidate's production BFF
uses `openid-client` for authorization code + PKCE, refresh, and revocation,
and the recorded real-server journey drives all three operations. The two new
TASK-011 findings do not invalidate that driver proof; they concern credential
representation coupling and failed-revocation sequencing around it.

## Verification and limits

The task records successful focused production identity journey execution,
Rust formatting and lints, UI checks and tests, and exact changed UI tests. In
this review round the orchestrator reran:

- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` — 0 errors,
  0 warnings;
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test` — 32 files,
  177 tests passed;
- both exact Vitest commands named by TASK-011 — one selected test passed in
  each command; and
- `git diff --check 7c48ac7c9..0b8919fff090d4b0109a506711cb119214d231f2` — passed.

Per the governing direction, the environment-owning full identity journeys
were not rerun during task review; they run at change review. That limit does
not prevent a verdict because both retained findings are established from
reachable source and dependency behavior, and their focused proofs are defined
in the remediation task.

## Verdict

**FIX_REQUIRED**

The candidate satisfies the remainder of TASK-011, including the approved
SSO/API-key decisions, expected bearer-token lifetime after logout, and
`FIND-TASK-010-1` closure. `FIND-TASK-011-1` and `FIND-TASK-011-2` are bounded
implementation corrections within the approved behavior and require no spec
revision.
