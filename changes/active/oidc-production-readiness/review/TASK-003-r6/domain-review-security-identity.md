# Security and identity domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority:
  - `review/TASK-003-r2/TASK-003-R2-production-ui-remediation.md`
  - `review/TASK-003-r3/TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`
  - `review/TASK-003-r4/TASK-003-R4-renewal-contract-boundaries.md`
  - `review/TASK-003-r5/TASK-003-R5-logout-refresh-family-retirement.md`
- Human directions:
  - `review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  - `review/TASK-003-r2/human-direction-connection-test.md`
  - `review/TASK-003-r5/human-direction-FIND-TASK-003-18.md`

The human owner explicitly authorized R3, R4, and R5. The R5 human direction
replaces FIND-18's earlier principal-wide logout correction: browser logout
retires only that session's refresh chain. The candidate resolved to the
stated object before inspection and again before this report was written. The
repository has no `.codegraph/` directory, so this review used the immutable
Git ranges, repository search, and direct source inspection. No other R6 report
was consulted.

## Reviewed boundary

This review traced the security and identity boundary through:

- tenant login and connection-test state, conditional authorization-response
  issuer binding, PKCE/nonce/token validation, candidate/tester binding, and
  the no-issuance connection-test result;
- connection administration RBAC, tenant binding, role mapping, audit, and
  replacement/deactivation behavior;
- BFF service-key admission and TLS policy, opaque flow/session cookies,
  server-owned tenant resolution, CSRF/origin checks, tenant switching, and
  server-side Wyrd API calls;
- encrypted provider, completion, access, refresh, bootstrap-key, and CSRF
  state, including canonical sealing-key inventory and rotation;
- browser renewal classification, refresh rotation/replay containment,
  logout, API-key preservation, principal/tenant/connection lifecycle, and
  failure rollback; and
- the R5 change specifically: durable refresh-chain identity, chain-local
  logout revocation, advisory-lock serialization, tenant RLS, and the added
  shutdown-harness correction's lack of production identity effect.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| OIDC callback and connection test | Revision-7 REQ-003/006/007/008/017, INV-001/002/004; issuer-binding and real-sign-in human directions; OIDC Core, RFC 9207, RFC 8725 | `wyrd-auth/src/{login,callback,connections}.rs`; `wyrd-auth-oidc/src/provider.rs`; callback wire types/routes; login-state SQL; callback and identity journeys | PASS |
| Browser/BFF trust boundary | REQ-005/009/010/015/016; TASK-003 packet-local contract; security posture | `components/auth/bff.rs`; UI `server-sessions.ts`, hooks, login/completion/settings routes, `upstream.ts`, and journey tests | PASS |
| Tenant, principal, RBAC, and audit | REQ-002/003/008/014/015/017, INV-001/002/003/005/007; forced-RLS and canonical-audit rules | `BrowserSessions`; `HumanConnections`; `TenantTokenIssuer`; tenant resolvers; browser/login SQL; settings actions and provider-switch journeys | PASS |
| Secret lifecycle and exposure | REQ-005/009/016, AC-007; repository secret rules | keyring seal/open paths; `SealedSecretRewrap`; browser-session inventory/CAS; redacted handlers and safe projections; cookie/page/URL leak assertions | PASS |
| Renewal and replay | REQ-009/016/017; RFC 9700 section 4.14.2; FIND-14/17 boundaries | `BrowserSessions::{current,renew}`; `Renewal`; `RefreshTokens::execute`; refusal classifiers; replay and internal-failure tests | PASS |
| Logout grant scope | R5 human direction; REQ-009; RFC 7009 section 2.1; RFC 9700 section 4.14.2 | `BrowserSessions::{complete,logout}`; browser-session migration/query; `lock_refresh_family`; `revoke_refresh_chain`; focused Postgres proof; API-key logout journey | PASS |

RFC 7009 permits revocation to cascade to tokens based on the same
authorization grant; it does not require revoking unrelated grants of the same
resource owner. The implemented root id plus `rotated_from` descendants is the
repository's concrete identity for that one login grant. RFC 9700 requires
retaining rotation relationships so replay can invalidate the active token;
the existing replay owner remains separate and unchanged.

## R5 security assessment

`BrowserSessions::complete` resolves the freshly issued refresh token inside
the flow's tenant transaction and persists its non-secret row id as
`refresh_chain_id` (`browser_sessions.rs:240-250`). The schema requires that
field exactly for `oidc_refresh` rows and binds it to a refresh row in the same
tenant with a composite foreign key
(`20261001000003_auth_browser_session_refresh_chain.sql:8-14`). API-key rows
must carry no chain id.

`BrowserSessions::logout` derives the tenant only from the hash of the opaque
session id, re-enters forced RLS, locks the live browser row, then uses the
locked row's principal and chain id. It takes the existing User-family advisory
lock, recursively revokes only the root and its descendants, wipes the browser
row, and commits once (`browser_sessions.rs:419-447`). The recursive query runs
through `TenantConn`; forced RLS confines both its anchor and recursive scan to
the bound tenant. The family lock serializes the update against ordinary
rotation, so a successor cannot be inserted outside logout's snapshot.

The correction no longer opens the stored refresh envelope. An unreadable
envelope therefore cannot skip revocation. A committed successor is included,
while another login of the same User has a different root and remains active.
This matches the replacing human direction and RFC 7009 grant scope. API-key
logout still performs only the browser-row wipe, leaving the underlying
operator key valid.

The R5 harness change in `wyrd-testing/src/server.rs:737-746` affects only
in-process test teardown: it cancels and drains Bifrost before dropping the
fixture database. It changes no production authentication, authorization,
tenant, token, cookie, or secret boundary.

## Prior-finding and human-direction closure

| Prior finding | Current-source security result |
|---|---|
| `FIND-TASK-003-1` | **CLOSED under the human replacement.** Typed optional callback `iss` reaches the common exchange owner; a present value is compared exactly before token-endpoint IO, and absence is refused only when fresh discovery advertises RFC 9207 support (`callback.rs:189-194,616-641`). |
| `FIND-TASK-003-2` | **CLOSED.** Browser API-key entry still routes known tenants through the shared verifier and makes an unknown route pay one dummy verification before the indistinguishable refusal (`browser_sessions.rs:263-312`). |
| `FIND-TASK-003-3` | **CLOSED.** Cookie suffixes are hints only; each is resolved through authenticated server state and mismatches are cleared (`server-sessions.ts:186-217,283-309`). |
| `FIND-TASK-003-4` | **CLOSED.** Canonical inventory and CAS rewrap cover all four browser envelopes, including expired stored rows (`human_connections.rs:437-540`). |
| `FIND-TASK-003-5` | **CLOSED.** Non-loopback plaintext is rejected before fetch, and the real UI harness retains a trusted TLS hop (`upstream.ts:16-27`; `identity_ui_e2e.rs:170-198`). |
| `FIND-TASK-003-6` | **CLOSED.** The production journey retains Keycloak/Dex switching plus wrong-provider and same-issuer cross-tenant callback refusal (`production-auth.integration.test.ts:481-558`). |
| `FIND-TASK-003-7` | **CLOSED.** The cited Rust items retain substantive workflow, invariant, and error documentation. |
| `FIND-TASK-003-8` | **CLOSED.** The typed tenant id comes from the server session and remains server-only (`bff.rs:231-279`; `server-sessions.ts:14-25,207-230`). |
| `FIND-TASK-003-9` | **CLOSED.** The speculative `SessionLifetime::Until` surface remains absent. |
| `FIND-TASK-003-10` | **CLOSED.** Ordinary proactive renewal refusal rolls back and serves the already-issued token only until its exact stored expiry (`browser_sessions.rs:450-541`). |
| `FIND-TASK-003-11` | **CLOSED.** The discovery support field and fallible projection retain accurate rustdoc and direct projection coverage (`provider.rs:28-55,84-107,261-287`). |
| `FIND-TASK-003-12` | **CLOSED.** Request-derived cookie hints are deduplicated and resolved sequentially, bounding private verification concurrency at one (`server-sessions.ts:292-302`). |
| `FIND-TASK-003-13` | **CLOSED.** Only an absent upstream value defaults; explicit empty or unsafe non-loopback HTTP values fail URL validation (`upstream.ts:16-27`). |
| `FIND-TASK-003-14` | **CLOSED.** Replay remains `Renewal::Contained` and commits containment; ordinary refusal rolls back before expiry; internal failure returns without commit or stale-authority fallback (`browser_sessions.rs:516-541,599-621`). |
| `FIND-TASK-003-15` | **CLOSED.** Shared API-key test support retains accurate ownership and panic documentation. |
| `FIND-TASK-003-16` | **CLOSED.** All three renewal classifiers remain crate-private (`issuance.rs:294`, `refresh.rs:64`, `exchange_api_key.rs:97`). |
| `FIND-TASK-003-17` | **CLOSED.** Missing or unopenable renewal credentials remain retryable internal failures and have a direct unit proof (`browser_sessions.rs:771-795,866-881`). |
| `FIND-TASK-003-18` | **CLOSED under the replacing R5 human direction.** Logout uses a durable chain root, the shared family lock, and descendant-only revocation without decrypting the envelope; the focused test proves a committed successor is revoked and a separate login remains active (`browser_sessions.rs:404-447,1478-1567`; `refresh_tokens.rs:173-209`). |

The real interactive connection-test direction also remains closed: test state
uses the ordinary authorization-code verification boundary, rechecks the
initiating tester's current authority, stamps only the exact candidate
revision, and returns without creating a User, credential, completion, or
browser session.

## Material findings

None.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. The documented residual mix-up exposure for providers that neither
advertise nor send RFC 9207 `iss` is an explicit human-approved decision with
a named per-connection-callback upgrade path, not a finding.

### Positive Controls

- Tenant and connection authority come from consumed server state or verified
  session/credential state, never route keys, cookie names, headers, email, or
  unverified provider claims.
- Login and connection-test state is random, digest-indexed, expiring, and
  single-use. Callback processing binds issuer, client, redirect, PKCE, nonce,
  connection revision, and intended result before issuing authority.
- The BFF key is checked before store access and grants only private session
  operations. Non-loopback traffic requires TLS; request tracing skips secret
  bodies and headers.
- Browser cookies remain host-only, Secure, HttpOnly, SameSite=Lax, path `/`,
  and expiry bounded. Mutations require POST, exact same origin, and
  constant-time session CSRF comparison.
- Raw flow/session ids and recoverable credentials are absent from durable
  plaintext. Session lookup stores only the session digest; keyring envelopes
  cover access, refresh, API-key, completion, provider-secret, and CSRF values.
- Browser SQL uses `TenantConn` after narrow digest-to-tenant resolution and
  forced RLS. The new refresh-chain foreign key includes `data_tenant_id`.
- Refresh rotation is single-use; detected replay retains the existing broader
  containment and canonical audit behavior. Ordinary logout now revokes only
  its own grant chain, preserving unrelated browser and CLI/SDK logins.
- Logout's chain revocation and browser-row wipe share one tenant transaction;
  lock, query, wipe, or commit failure cannot persist a half-logout.

## Verification limits

- This reviewer ran the exact R5 Postgres selector through the repository's
  Postgres lifecycle and migration wrapper. Migration idempotence passed, and
  `browser_sessions::pg_tests::oidc_logout_retires_only_its_refresh_chain_without_opening_it`
  passed. A first direct invocation without the required wrapper failed only
  at fixture startup because `WYRD_TEST_DATABASE_ADMIN_URL` was absent; it did
  not execute the test behavior.
- The R5 implementation record reports `fmt`, `lints`, `codegen:check`,
  `check:tenant-isolation`, `test:sql`, `test:wyrd`, and
  `test:identity:journey` green after the harness correction. This reviewer
  did not rerun those broad lanes, Keycloak/Dex, or the full browser/TLS
  journey.
- The non-public routing of `/internal/bff/v1/*` remains a deployment gateway
  obligation. Repository proof covers service-key admission and trusted TLS,
  but cannot prove an operator's external ingress configuration.
- No dependency manifest or lockfile changed in the cumulative range, so no
  new supply-chain surface required assessment.

## Overall result

**PASS**

The cumulative candidate satisfies the reviewed security, RBAC, identity,
tenant-isolation, secret-handling, callback, renewal, replay, and logout
obligations. R5 closes FIND-18 at the human-directed grant boundary without
revoking unrelated sessions or weakening the existing replay response.
