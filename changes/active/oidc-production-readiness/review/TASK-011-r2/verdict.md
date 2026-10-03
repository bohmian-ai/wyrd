# TASK-011 review verdict — round 2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `4d468b33e49de4dd9df30c5dd046a334569465bf`
- Candidate tree: `87acdce15e3ca6ea2b6016969695db398ab2d598`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Prior review: `changes/active/oidc-production-readiness/review/TASK-011-r1/`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-011-r1/TASK-011-R1-browser-session-standards-and-logout.md`
- Binding reversal: `changes/active/oidc-production-readiness/review/TASK-011-r1/lead-direction-FIND-TASK-011-2.md`

The candidate and tree remained unchanged through discovery, validation, and
verdict preparation. The complete base-to-candidate range was reviewed. The
latest remediation diff was used to locate changed owners and prove prior
finding closure without narrowing the cumulative acceptance audit.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Use `openid-client` 6.8.8 for RFC 8414 discovery, authorization code plus S256 PKCE, refresh, RFC 8693 API-key exchange, and RFC 7009 revocation as confidential client `wyrd-ui` | `browser-sessions.ts` delegates those operations to the pinned library and uses `ClientSecretBasic`; package and lock files pin the required dependency | Cumulative source/dependency inspection; recorded production journey; focused owner tests | PASS |
| Use one `jose`-encrypted Secure, HttpOnly, SameSite=Lax portable cookie, with no server-side BFF session store | `BrowserSessions.seal`/`unseal`, per-tenant cookie names, and reconstructible process-local access cache | Recorded cross-replica journey; UI test lane; independent source validation | PASS |
| Two BFF replicas in front of two Wyrd replicas serve the same session | Encrypted cookie carries renewal authority; cache miss uses the standard refresh/exchange path; `identity_ui_e2e.rs` and `WyrdTestServer::start_bound_replica` host the topology | Recorded filtered production UI journey; system and lifecycle source review | PASS |
| No token appears in page data, URLs, or JavaScript | `BrowserSession.#token` remains private; safe metadata is projected centrally; credentials remain server-only or inside authenticated encryption | Recorded secret-scanning journey assertions; route and source inspection | PASS |
| `FIND-TASK-011-1`: refresh credentials remain opaque | `BrowserSessions.establish` seals the credential under the existing application-session bound without parsing it; `access` forwards it unchanged to `refreshTokenGrant` | Exact test `accepts an opaque refresh token and forwards it unchanged` passed; full owner test file passed 2/2 | PASS — CLOSED |
| Terminal renewal refusal clears only the selected session, while transient upstream failure preserves retry through the existing cookie | `access` distinguishes the installed library's terminal OAuth refusal; `read` deletes only the selected tenant cookie | Focused opaque-token test and lifecycle source trace | PASS |
| Binding reversal of `FIND-TASK-011-2`: logout always clears local cookie/cache and attempts RFC 7009 revocation best-effort without exposing failure to the user | `BrowserSessions.logout` clears the selected local state, skips API-key revocation, catches refresh-token revocation failure, and logs only tenant plus error class | Exact test `failed refresh-token revocation still signs out` passed; full owner test file passed 2/2 | PASS — REVERSED DIRECTION SATISFIED |
| Logout revokes only the selected login when revocation succeeds; sibling login and already issued bounded bearer authority follow the approved limits | Per-login refresh credential, selected cache key, no global user invalidation or cross-replica coordination | Recorded two-login production journey; cumulative source review | PASS |
| Routine login is SSO and operator API-key recovery is a separate page | Tenant login route exposes SSO as the primary action; `/t/{tenant}/login/api-key` owns recovery | Recorded OIDC-off and SSO journey evidence; route inspection | PASS |
| An API-key session's authority is the exchanged token's tenant, not the route key | `BrowserSession.context` projects the server-issued token's tenant and permissions; protected calls forward that token; no mapper, claim, or route/token equality mechanism was added | Tenancy reviewer traced recovery, hooks, settings, switching, and cross-tenant refusal paths | PASS |
| Tenant switch revalidates the target tenant's independent session | `BrowserSessions.switch` calls `read` for the target's own authenticated cookie | Recorded multi-provider switch journey; tenancy and lifecycle reviews | PASS |
| An old-connection session cannot renew after replacement or removal | Existing cached access remains bounded; the next refresh-on-miss receives terminal refusal and clears the selected cookie | Recorded replacement journey; lifecycle review | PASS |
| Delete `server-sessions.ts`, the flow cookie, custom CSRF, private BFF protocol, and `/login/complete` | Deleted cumulative sources and callers; SvelteKit `csrf.checkOrigin` plus SameSite=Lax replace custom CSRF | Cumulative diff and repository search; UI tests | PASS |
| Preserve non-goals: no server session store, second role mapper, `@auth/sveltekit`, handwritten OAuth calls, retry state, shared cache, new setting, tenant-mapping endpoint, or compatibility surface | Complete cumulative diff and manifest/source inspection | All discovery reviews and structured Ponytail validation | PASS |

## Independent review results

| Review | Result | Material result |
|---|---|---|
| Behavior implementation review | PASS | No proposed findings; complete acceptance matrix passes |
| Invariant implementation review | PASS | No proposed findings; credential, tenant, cache, renewal, replacement, and logout invariants pass |
| Repository standards review | PASS | No material findings; one wording-only non-blocking note |
| Maintainer review | PASS | No material findings; wording/comment notes remain non-blocking |
| System-resilience review | PASS | No material process, outage, restart, or recovery finding |
| OIDC/browser-security domain review | PASS | No material protocol or trust-boundary finding |
| Tenancy/authority domain review | PASS | No material tenancy or authorization finding |
| Session-lifecycle domain review | PASS | No material lifecycle, concurrency, durability, or recovery finding |
| Structured Ponytail validation | COMPLETE | Independently validated empty final ledger |

Every required report is present. No required reviewer was unavailable.

## Follow-up decision

No focused follow-up was needed. The eight discovery reports proposed no
material findings, did not materially conflict, revealed no unreviewed
reachable path, and did not leave a repeated-remediation common source
untraced. Structured Ponytail validation independently checked that empty
union against the cumulative source, callers, sibling consumers, prior
findings, installed dependencies, and approved authority rather than treating
reviewer agreement as proof.

## Validated finding ledger

**Empty.**

No reachable `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION`
finding remains. The wording-only `REQ-010` identifier in recovery-route JSDoc
has no behavioral, security, tenancy, durability, public-contract, or
documentation-completeness consequence. Under the binding review direction it
is non-blocking and is not retained in the ledger. No new
`FIND-TASK-011-*` identifier is assigned.

## Prior-finding closure

### `FIND-TASK-011-1` — closed

The prior refresh-token representation coupling is deleted at its source.
`BrowserSessions.establish` no longer decodes the refresh credential or
derives cookie lifetime from a private token claim. It applies the existing
conventional twelve-hour application-session bound and preserves the credential
unchanged for the installed standard refresh and revocation operations. The
focused opaque-token test proves establishment, encrypted-cookie creation,
unchanged refresh forwarding, and selected-session clearing on terminal
refusal. No parser, introspection call, endpoint, claim, setting, store, or
compatibility path replaced the deleted mechanism.

### `FIND-TASK-011-2` — reversed; binding direction satisfied

The lead direction supersedes the original retry-preservation finding. The
candidate ends the selected local browser session unconditionally, removes the
current replica's matching cache entry, attempts refresh-token revocation
best-effort through `openid-client`, logs no token value, and does not surface
revocation failure as failed logout. API-key logout remains local only. The
focused failure test proves these effects. No retry cookie, durable revocation
state, server endpoint, option, setting, or cross-replica mechanism was added.

## Verification and limits

The task and remediation evidence record the two exact focused Vitest tests,
UI `check`, the full UI unit lane, the filtered production UI identity journey,
Rust format and lints, and cumulative diff checking as green. During this
review the orchestrator reran:

- both exact remediation Vitest selectors — one selected test passed in each;
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test` — 33 files,
  179 tests passed;
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` in isolation
  — 0 errors and 0 warnings; and
- `git diff --check 7c48ac7c99f018d3993922e63875839f3695c503..4d468b33e49de4dd9df30c5dd046a334569465bf`
  — passed.

An initial concurrent UI check/test invocation transiently lost generated
SvelteKit route types in their shared `.svelte-kit` directory. The isolated
rerun passed, and independent reviewers also reran the isolated check and owner
tests successfully; this was a shared generated-directory race, not a source
failure.

Per the supplied review direction, full identity and every-language journeys
are change-review proof and were not rerun as task-review gates. That scoped
execution does not leave an unproved remediation behavior: both changed
outcomes have exact focused owner tests, while the cumulative journey evidence
and source paths were independently audited.

## Verdict

**PASS**

All required reports are complete, the independently validated ledger is
empty, every original TASK-011 obligation passes under the approved spec and
binding lead decisions, both prior findings are resolved under current
authority, non-goals remain excluded, verification is credible at the required
task scope, and no unrelated implementation change entered the cumulative
candidate.
