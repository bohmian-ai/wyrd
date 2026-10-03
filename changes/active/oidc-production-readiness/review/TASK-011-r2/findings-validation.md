# TASK-011 round 2 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `4d468b33e49de4dd9df30c5dd046a334569465bf`
- Candidate tree: `87acdce15e3ca6ea2b6016969695db398ab2d598`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Prior review: `changes/active/oidc-production-readiness/review/TASK-011-r1/`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-011-r1/TASK-011-R1-browser-session-standards-and-logout.md`
- Binding reversal: `changes/active/oidc-production-readiness/review/TASK-011-r1/lead-direction-FIND-TASK-011-2.md`
- Cumulative diff: `7c48ac7c99f018d3993922e63875839f3695c503..4d468b33e49de4dd9df30c5dd046a334569465bf`
- Remediation localization: `0b8919fff090d4b0109a506711cb119214d231f2..4d468b33e49de4dd9df30c5dd046a334569465bf`

The candidate and tree matched these identities before and after validation.
The repository has no `.codegraph/` directory, so I used the repository's
existing Git and source-search tools. I reviewed the complete cumulative diff,
the remediation diff, all eight required round-2 discovery reports, the prior
verdict and finding ledger, the original task, the remediation task, and the
applicable repository, architecture, security, UI, TypeScript, testing, and
standards authorities. No required report or source path was missing.

The binding lead direction is applied without reopening the original
`FIND-TASK-011-2`: browser logout always clears the local cookie and current
replica cache, and RFC 7009 revocation is best-effort. The lead-approved SSO
primary/recovery-page split and API-key token-tenant authority are likewise
settled inputs.

## Discovery proposal validation

The discovery union is explicitly empty:

| Report | Proposed material findings | Validation |
|---|---:|---|
| `task-review-behavior.md` | None | Independently checked against the cumulative source, callers, tests, and task obligations; empty proposal retained. |
| `task-review-invariants.md` | None | Independently traced credential, tenant, session, cache, and logout state producer-to-sink; empty proposal retained. |
| `standards-review.md` | None | Applicable standards/library ownership and deleted custom mechanisms were checked in source and manifests; empty material proposal retained. |
| `maintainer-review.md` | None | Changed owners, routes, consumers, tests, and declarations were checked; empty material proposal retained. |
| `system-review.md` | None | Restart, cold replica, dependency failure, terminal refusal, replacement, and logout failure paths were checked; empty proposal retained. |
| `domain-review-oidc-security.md` | None | Code/PKCE, confidential-client, cookie, refresh, revocation, CSRF, and secret boundaries were checked; empty proposal retained. |
| `domain-review-tenancy.md` | None | Route context, sealed-cookie identity, token tenant, settings calls, and switching were traced; empty proposal retained. |
| `domain-review-session-lifecycle.md` | None | Establishment, cache miss, refresh, refusal, eviction, logout, replay, and cross-replica paths were traced; empty proposal retained. |

There was no material conflict, unreviewed reachable path, or repeated-remediation
uncertainty in those reports, so the orchestrator's decision not to request a
follow-up report is supported. Agreement was not treated as proof; the empty
union was validated directly below.

## Independent source and caller tracing

### Credential producers and consumers

`BrowserSessions.complete` is the sole refresh-session producer. It obtains the
refresh credential from `openid-client.authorizationCodeGrant` and passes it
unchanged to `establish`. `BrowserSessions.signInWithApiKey` is the API-key
session producer; it uses `openid-client.genericGrantRequest` and passes the
operator credential to the same owner. `establish` caches only the returned
access token and seals the renewal credential without parsing it.

The complete credential-consumer set is:

- `access`, which hashes the credential for the process-local cache and either
  passes it unchanged to `openid-client.refreshTokenGrant` or reuses it in the
  existing RFC 8693 API-key exchange;
- `logout`, which removes that credential's cache entry and, only for refresh
  sessions, passes it unchanged to `openid-client.tokenRevocation`;
- `sealed`, `read`, `switch`, and `metadata`, which decrypt or select the
  tenant-bound cookie but do not inspect credential representation.

Repository search finds no other refresh-token decoder or OAuth call. The only
remaining `decodeJwt` reads the server-issued access token needed for its
expiry and safe UI projection; it does not read the refresh credential. Thus
the invalid state behind prior `FIND-TASK-011-1` is no longer produced, and no
downstream compatibility guard or second parser was added.

### Caller and sibling-consumer closure

The production callers are complete and cohesive:

- tenant login calls `begin`, and the common callback calls `complete`;
- the separate recovery route calls `signInWithApiKey`;
- the server hook and tenant login load call `read`;
- root actions call `switch` and `logout`;
- the tenant layout calls `metadata`;
- authenticated settings and change actions call `BrowserSession.api`, which
  sends only the private access token to Wyrd.

`BrowserSession.context` obtains tenant UUID, principal, and permissions from
the server-issued access token. The route tenant key remains navigation and
cookie-slot context; protected Wyrd operations carry the token and no
route-derived tenant authority. This preserves the lead-approved API-key rule:
a foreign tenant's key used on a recovery route receives only its own
server-scoped authority. Adding a mapping endpoint, route-tenant claim, second
role mapper, or route/token equality check would contradict that decision and
would be `DRIFT`.

### Logout and failure propagation

The root logout action awaits `BrowserSessions.logout` and then redirects to
the tenant login page. `logout` decrypts only the selected tenant cookie,
deletes that cookie, removes only the matching credential-hash cache entry,
and returns immediately for API-key sessions. For refresh sessions it uses the
installed `openid-client.tokenRevocation`; failure is caught at this request
boundary and emits only the route tenant and error class. It neither retains
the browser session nor turns local logout into a failed action.

The focused test exercises a transport failure, proves local cookie removal,
proves the credential value is absent from the warning, then replays the prior
cookie and proves the cache entry was removed because renewal is attempted.
This is exactly the binding local-clear/best-effort direction. Retry cookies,
durable revocation work, replica broadcasts, tombstones, a server-side session
store, or a logout option would be additional nonstandard mechanisms and are
not valid corrections.

### Cumulative replacement and sibling behavior

The cumulative candidate deletes `server-sessions.ts`, `/login/complete`, the
private BFF protocol callers, the flow cookie, and custom CSRF fields. It uses
the specified `openid-client` operations, `jose` authenticated encryption,
SvelteKit's native origin checking, SameSite=Lax cookies, and the existing Wyrd
token exchange and API authorization. The new Rust harness method composes the
existing `WyrdTestServer::start_replica` and `bind` owners solely to run the
required two-Wyrd/two-BFF journey; it does not create a second server lifecycle.

The process-local access cache is reconstructible from the encrypted cookie,
bounded without a new setting, and not an authority. A cold replica or cache
miss uses the standard refresh/exchange operation. The accepted bearer-token
limit remains visible: another replica may serve an already issued access token
until its ordinary expiry, but no new invalidation or coordination mechanism is
promised.

## Prior-finding closure

### `FIND-TASK-011-1` — **CLOSED**

- **Prior classification:** `DRIFT`
- **Prior defect:** `establish` decoded the refresh credential as a JWT to
  derive cookie expiry, coupling a standard OAuth client to a private token
  representation.
- **Source closure:** `browser-sessions.ts:249-261` now applies the existing
  fixed application-session bound and seals `credential` unchanged.
  `browser-sessions.ts:293-306` forwards that exact string to
  `refreshTokenGrant`. The only credential siblings are the hash key and
  best-effort revocation use; neither interprets its representation.
- **Focused proof:** `browser-sessions.test.ts` supplies
  `opaque-refresh-token-not-a-jwt`, observes the encrypted 12-hour cookie,
  verifies unchanged forwarding to `refreshTokenGrant`, and verifies a
  terminal refusal clears only the selected session.
- **Ponytail result:** the remediation deletes the nonstandard inspection and
  reuses the existing cookie lifetime and installed client library. No parser,
  endpoint, setting, store, or compatibility path replaces it. Closure is
  independently confirmed.

### `FIND-TASK-011-2` — **REVERSED; LEAD DIRECTION SATISFIED**

- **Binding direction:** end the local browser session unconditionally and
  attempt refresh-token revocation best-effort; do not retain a retry cookie or
  surface revocation failure as failed logout.
- **Source evidence:** `browser-sessions.ts:341-357` clears the selected cookie
  and matching cache entry first, skips revocation for API keys, calls
  `openid-client.tokenRevocation` only for refresh sessions, and contains a
  failure in a token-free warning.
- **Caller evidence:** `routes/+page.server.ts:170-187` redirects normally
  after the resolved logout operation. No sibling caller exposes a retry or
  alternate logout behavior.
- **Focused proof:** `failed refresh-token revocation still signs out` proves
  the operation resolves, the cookie and cache state are gone, and the warning
  excludes the refresh token.
- **Ponytail result:** the candidate follows the explicit conventional
  direction with the already-installed library and no added mechanism. The
  original retry-preservation finding is not reopened.

## Ponytail ladder assessment

| Changed mechanism or surface | Delete? | Existing/native/standard owner | Result |
|---|---|---|---|
| Refresh-credential parsing | Yes | OAuth treats the credential as opaque; `openid-client` forwards it | Deleted by remediation; no replacement mechanism. |
| Code, PKCE, discovery, refresh, exchange, revocation | Custom implementations must be deleted | `openid-client` 6.8.8 | Standard library path used throughout. |
| Cookie confidentiality/integrity | Required by TASK-011 | Installed `jose` JWE primitives | Minimum specified mechanism; no home-grown cryptography. |
| CSRF token/check | Yes | SvelteKit `csrf.checkOrigin` plus SameSite=Lax | Custom mechanism deleted. |
| Server-side BFF session/protocol | Yes | Encrypted cookie plus standard nonrotating confidential-client refresh | Deleted; no compatibility path remains. |
| Logout retry state or durable revocation machinery | Yes | Conventional local sign-out plus best-effort RFC 7009 call | Correctly absent under lead direction. |
| API-key route-to-tenant mapper/check | Yes | Wyrd-issued token and existing server authorization | Correctly absent under lead decision. |
| Process-local access cache | No; task requires it | Native `Map`, credential hash, standard refresh/exchange on miss | Small bounded reconstructible cache; no option, shared store, or coordination layer. |
| Two-replica journey seam | No; required proof needs an out-of-process bound replica | Existing `WyrdTestServer::start_replica` plus `bind` | Narrow inherent method, no new trait, store, or runtime. |

No changed abstraction has an unearned second implementation, generic layer,
factory, compatibility surface, public option, or speculative extension. The
candidate deletes the prior custom protocol and uses the standard, framework,
native-platform, or already-installed mechanism at every reviewed boundary.

## Rejected and non-blocking notes

- The prior proposal to retain the session after revocation failure is rejected
  by binding lead direction; it is not a live finding or optional suggestion.
- Requiring immediate cross-replica invalidation of already issued access
  tokens is rejected by REQ-016 and the task's approved bearer-token limit.
- A route/token tenant equality check or tenant-mapping endpoint for API-key
  recovery is rejected because the approved authority is the exchanged token's
  tenant and Wyrd enforces it on every protected call.
- A nonce, duplicate JWT verifier, additional CSRF token, refresh-token parser,
  server-side browser-session store, revocation retry state, shared cache,
  single-flight lock, new lifetime setting, or custom OAuth error taxonomy
  would add mechanisms not required by the applicable standards, framework,
  task, or comparable conventional behavior. None is required.
- `login/api-key/+page.server.ts:15` includes the requirement identifier
  `REQ-010` in JSDoc. This is a wording-only repository-rule note with no
  behavioral, security, tenancy, durability, public-contract, or documentation
  completeness consequence. Per explicit review direction it is non-blocking
  and is not retained in the finding ledger.
- File placement, naming, structure, and wording proposals are excluded from
  the material ledger as directed. No missing Rust documentation was found.

## Verification

I independently ran the narrow owner proof at the immutable candidate:

```text
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run \
  src/lib/server/auth/browser-sessions.test.ts

Test Files  1 passed (1)
Tests       2 passed (2)
```

`git diff --check 7c48ac7c99f018d3993922e63875839f3695c503..4d468b33e49de4dd9df30c5dd046a334569465bf`
also passed. The task evidence records both exact focused tests, UI `check`, the
full UI unit lane, the filtered production UI identity journey, Rust format and
lints, and diff checking as green. Under the supplied direction, full journeys
belong to change review; their absence from this rerun is not converted into a
finding or verification limit.

## Final deduplicated finding ledger

**Empty.**

No discovery proposal required confirmation or revision, and independent
source validation found no reachable `MISSING`, `INCORRECT`, `DRIFT`,
`VIOLATION`, or `REGRESSION` finding. `FIND-TASK-011-1` is closed by source and
focused proof. `FIND-TASK-011-2` remains reversed and the candidate satisfies
the binding replacement direction. No new `FIND-TASK-011-*` ID is assigned.

No decision-complete remediation recommendation is required.
