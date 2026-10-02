# Security and identity domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `6aedcda5166509001db0cc851a5bc74502b4b043`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediations:
  - `changes/active/oidc-production-readiness/review/TASK-003-r2/TASK-003-R2-production-ui-remediation.md`
  - `changes/active/oidc-production-readiness/review/TASK-003-r3/TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`
- Human directions:
  - `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  - `changes/active/oidc-production-readiness/review/TASK-003-r2/human-direction-connection-test.md`

The checked-out candidate resolved to the requested immutable object before
source inspection and immediately before this report was written. The complete
base-to-candidate range was reviewed; `8289fa298..6aedcda51` was used only to
locate the R3 remediation owners.

## Reviewed boundary

This pass traced the security and identity boundary through:

- browser cookie and CSRF handling, the SvelteKit BFF private channel, service-
  key admission, server-owned tenant/session resolution, and Wyrd API calls as
  the session principal;
- OIDC login and candidate-test state, issuer binding, PKCE, nonce, callback
  consumption, ID-token verification, tested-revision binding, and the branch
  that deliberately issues no User, credential, completion, or session;
- browser-session access/refresh/API-key/CSRF sealing, key rotation inventory,
  exact expiry, logout, tenant RLS, principal/connection lifecycle, and
  renewal under the durable row lock;
- refresh-family replay containment and canonical audit, fixed-cost API-key
  verification, refusal indistinguishability, and internal failure mapping;
  and
- the R3 behavior change: a missing or unopenable stored renewal credential is
  a retryable internal failure rather than a terminal session decision.

## Authority and source coverage

| Boundary | Authority | Source and proof inspected | Result |
|---|---|---|---|
| OIDC trust and issuer binding | Revision 7 REQ-003/006/007/017, INV-001/004; both human directions; OIDC Core, RFC 9207, RFC 9700, RFC 8725 | `wyrd-auth/src/{login,callback,connections}.rs`; `wyrd-auth-oidc/src/provider.rs`; callback contracts, SQL state, and identity journeys | PASS |
| Browser/BFF trust boundary | REQ-005/009/010/015; TASK-003 packet-local contract; security posture | `wyrd-server/src/components/auth/bff.rs`; UI `server-sessions.ts`, hooks, login/completion/settings routes, upstream policy and tests | PASS |
| Tenant, principal, and connection binding | REQ-002/008/014/016/017, INV-001/002/003/005/007; RLS and audit rules | `BrowserSessions`; `HumanConnections`; `TenantTokenIssuer`; browser/login SQL queries and migrations; provider-switch and two-tenant journeys | PASS |
| Refresh replay and failure semantics | REQ-009/016/017; R3 `FIND-TASK-003-14`; security-posture replay and fail-closed rules | `BrowserSessions::{current,renew}`; `Renewal`; `RefreshTokens::execute`; `RefreshError::is_refusal`; replay and internal-failure PG tests | PASS |
| API-key verification and renewal | REQ-010; fixed-cost credential-refusal authority; R3 `FIND-TASK-003-14` | `ExchangeApiKey::{execute,verify_api_key}`; `ExchangeError::is_refusal`; `map_exchange_error_to_wyrd`; API-key renewal/refusal tests | PASS |
| Secret lifecycle and exposure | REQ-005/009, AC-007; repository secret-handling rules | `BrowserSessions` seal/open paths; `SealedSecretRewrap`; browser-session inventory/CAS; redacted handlers and projections; cookie and page leak assertions | PASS |

## Prior-finding and human-direction closure

- `FIND-TASK-003-1` remains closed under its human direction. Callback state
  selects the issuer and connection; a present authorization-response `iss`
  is compared exactly before token-endpoint IO, and absence is refused only
  for a provider advertising RFC 9207 support. The approved residual risk for
  non-advertising providers remains documented.
- `FIND-TASK-003-2` through `FIND-TASK-003-13` remain closed. The R3 diff does
  not alter their callback, tenant, cookie, CSRF, TLS, fixed-cost verification,
  sealing-inventory, provider-topology, or UI trust boundaries.
- `FIND-TASK-003-14` is closed. `RefreshError::Reused` is preserved as
  `Renewal::Contained`, so family revocation and its canonical audit commit.
  Ordinary credential/lifecycle refusals alone use the expiry-bound terminal
  path. Store, audit, signing, corrupt-state, verification-task, and envelope-
  open failures return `Renewal::Failed`, causing the transaction to drop and
  roll back without serving the old token or revoking the browser row.
- `FIND-TASK-003-15` does not change runtime security behavior; its shared
  test-support documentation is corrected.
- `HD-TASK-003-R2-1` remains closed. Candidate testing continues to use the
  normal authorization-code and ID-token verification boundary, rechecks the
  tester's current authority, stamps only the exact tested revision, and
  creates no User or credential/session authority.

## Flagged behavior assessment

The unopenable-stored-credential change is correct and task-scoped.
`open_credential` maps a missing, non-UTF-8, or undecryptable stored refresh or
API-key credential to `Renewal::Failed(WyrdError::Internal)`
(`browser_sessions.rs:765-777`). `BrowserSessions::current` returns that error
without committing the tenant transaction, without serving the existing
access token, and without revoking or wiping the browser row
(`browser_sessions.rs:503-528`). The BFF clears a session cookie only for a
`401`; an internal response is surfaced as an upstream failure and retains the
cookie (`server-sessions.ts:192-205,317-333`). Restoring the held sealing key or
repairing the stored invariant therefore permits a later retry.

This boundary fails closed at the request, not at the whole session: no Wyrd
authority is returned while the credential cannot be opened, but a transient
keyring/configuration or recoverable durable-state fault does not destroy the
only session record needed for recovery. Treating this condition as an
ordinary credential refusal would incorrectly convert security uncertainty
into a durable lifecycle decision and would conflict with revision-7 key-
rotation recovery requirements.

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

None. The documented residual authorization-response mix-up exposure for a
provider that neither advertises nor sends RFC 9207 `iss` is an explicit
human-approved decision with a named per-connection-callback upgrade path, not
a finding in this review.

### Positive Controls

- Random, hashed flow and session identifiers remain the only browser lookup
  hints; verified server state selects tenant and connection authority.
- PKCE, nonce, exact issuer/audience/algorithm/key/time validation, screened
  provider IO, one-use state, and tested-revision binding remain shared by
  login and candidate testing.
- BFF calls require the deployment service key before store access; the key
  does not select tenant authority, and browser credentials travel only over
  the private server-side channel.
- Browser cookies remain host-only, Secure, HttpOnly, SameSite=Lax, path `/`,
  and expiry-bounded. Mutations require POST, exact same origin, and constant-
  time session CSRF verification.
- Refresh replay commits the existing complete-family revocation and canonical
  audit. No second containment or audit owner was introduced.
- Ordinary revoked/inactive credential outcomes remain indistinguishable at
  the public API-key boundary and end browser renewal only at exact access-
  token expiry.
- Internal renewal failures now fail the request, roll back tentative refresh
  consumption or API-key use, preserve the browser row for retry, and never
  serve stale authority as a fallback.
- Recoverable provider, access, refresh, bootstrap-key, and CSRF material
  remains encrypted at rest, absent from browser-visible metadata and logs,
  and included in the canonical sealing-key rewrap inventory.

## Verification limits

- Per assignment, this reviewer did not rerun Cargo, mise, pnpm, Postgres,
  Keycloak, Dex, browser, or TLS lanes. The R3 remediation records the three
  focused Postgres tests, the ordinary-refusal control, the classification
  unit test, `test:wyrd`, `test:identity:journey`, `check:tenant-isolation`,
  `fmt`, `lints`, and `git diff --check` as green. This pass independently
  inspected the implementation and the assertions in those tests.
- The focused runtime tests inject corrupt-role internal failures rather than
  corrupting ciphertext. The flagged envelope-open branch is a direct,
  side-effect-free classification through the same `Renewal::Failed` arm and
  was verified by source trace through the BFF cookie behavior; this is not a
  material proof gap.
- No dependency manifest or lockfile changed in the cumulative range, so no
  new supply-chain boundary required review.
- CodeGraph was unavailable because the repository has no `.codegraph/`
  directory; callers and sibling paths were traced with repository-native
  search and direct source inspection.

## Overall result

**PASS**

The cumulative candidate satisfies the reviewed security, identity, tenant-
binding, audit, replay-containment, secret-handling, and fail-closed browser-
renewal obligations. The flagged unopenable-credential behavior is a safe
retryable internal failure and introduces no material security regression.
