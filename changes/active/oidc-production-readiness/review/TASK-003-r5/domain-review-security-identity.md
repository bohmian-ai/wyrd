# Security and identity domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediations:
  - `changes/active/oidc-production-readiness/review/TASK-003-r2/TASK-003-R2-production-ui-remediation.md`
  - `changes/active/oidc-production-readiness/review/TASK-003-r3/TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`
  - `changes/active/oidc-production-readiness/review/TASK-003-r4/TASK-003-R4-renewal-contract-boundaries.md`
- Human directions:
  - `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  - `changes/active/oidc-production-readiness/review/TASK-003-r2/human-direction-connection-test.md`

The checked-out candidate resolved to the requested immutable object before
source inspection and again immediately before this report was written. The
complete base-to-candidate range was reviewed. The latest locator range
`6aedcda5166509001db0cc851a5bc74502b4b043..989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
was used only to identify the R4 changes; it changes classifier visibility,
local documentation, and a direct unit proof, not runtime behavior.

The repository has no `.codegraph/` directory, so navigation used the
immutable Git range, `rg`, and direct source inspection.

## Reviewed boundary

This review traced the security, RBAC, and identity boundary through:

- tenant login and candidate-test initiation, persisted one-use state, common
  callback handling, conditional RFC 9207 issuer binding, PKCE, nonce,
  algorithm/audience/authorized-party validation, and tested-revision binding;
- connection administration authorization, tester re-authorization,
  recovery-key verification, tenant-local role mapping, canonical audit, and
  provider replacement;
- BFF service-key admission, server-owned session lookup, hashed opaque flow
  and session identifiers, cookie and CSRF controls, tenant chooser/switching,
  settings actions, and server-side Wyrd API calls;
- encrypted access, refresh, bootstrap-key, completion, provider-secret, and
  CSRF storage, including sealing-key inventory and rotation behavior;
- fixed-cost API-key exchange, access/refresh renewal, refresh-family replay
  containment, logout, principal/tenant/connection lifecycle refusal, and the
  internal-failure rollback boundary; and
- R4's crate-private refusal classifiers and direct proof that missing or
  unopenable renewal envelopes remain retryable internal failures.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| OIDC callback trust | Revision 7 REQ-003/006/007/008/017, INV-001/002/004; issuer-binding human direction; OIDC Core 1.0; RFC 7636; RFC 9207; RFC 8725 | `wyrd-auth/src/{login,callback,connections}.rs`; `wyrd-auth-oidc/src/provider.rs`; callback wire type and route; login-state SQL; callback and identity journeys | PASS |
| Candidate test sign-in | REQ-003, AC-006; real-sign-in human direction | `HumanConnections::{begin_test,tested_candidate,stamp_test_sign_in}`; `AuthorizationCodeExchange`; connection-test migration constraint; server handler and journey assertions | PASS |
| Connection administration and RBAC | REQ-002/003/014/017, INV-003/005/007; repository audit rules | `components/admin/identity.rs`; `HumanConnections`; tester and recovery-key authorization; tenant settings actions; canonical audit calls | PASS |
| Browser/BFF trust boundary | REQ-005/009/010/015/016; TASK-003 packet-local contract; security posture | `components/auth/bff.rs`; `BrowserSessions`; UI `server-sessions.ts`, hooks, login/completion/settings routes, upstream transport policy and tests | PASS |
| Tenant and principal binding | REQ-008/009/015/016, INV-001/002/003/005/007; RLS authority | browser-session and login-state migrations/queries; `TenantConn` paths; definer hash-to-tenant lookups; issuance and verifier calls; two-tenant journeys | PASS |
| Credential secrecy and lifecycle | REQ-005/009/016/017, AC-007; security-posture secret/replay rules | sealing/opening and canonical rewrap inventory; browser-session renewal/logout; `RefreshTokens`; `ExchangeApiKey`; leak, rotation, replay, refusal, and internal-failure proofs | PASS |
| R4 renewal contract boundary | R4-AC-01 through R4-AC-04 | `IssuanceError::is_refusal`, `RefreshError::is_refusal`, `ExchangeError::is_refusal`; `open_text`; `open_credential`; classification/unit and Postgres consequence tests | PASS |

Primary-standard comparison confirmed that the candidate keeps issuer values
case-sensitive, applies simple exact comparison to a present authorization
response `iss` before token-endpoint IO, binds the code exchange to the stored
redirect and PKCE verifier, checks the stored nonce, restricts the token
algorithm to the provider-advertised set before signature verification, and
enforces audience/`azp` semantics. The approved conditional behavior for an
absent `iss` from a provider that does not advertise RFC 9207 support, and its
documented residual mix-up exposure, are explicit human-owned scope decisions.

## Prior-finding and human-direction closure

- `FIND-TASK-003-1` remains closed under the human direction. Login state
  selects the tenant, connection, issuer, client, redirect, PKCE verifier, and
  nonce. A present `iss` must exactly equal that issuer; absence is refused
  only when fresh discovery advertises support. Both refusal cases consume the
  state before any token request and create no completion or session.
- `FIND-TASK-003-2` through `FIND-TASK-003-13` remain closed. The cumulative
  source preserves server-returned tenant authority, cookie and CSRF controls,
  TLS-only non-loopback BFF transport, fixed-cost credential refusal, complete
  sealing inventory, provider topology, issuer-mutation proof, and bounded
  chooser verification.
- `FIND-TASK-003-14` remains closed. Refresh replay maps to
  `Renewal::Contained`, so the existing family revocation and canonical audit
  commit. Only ordinary credential/lifecycle rejection reaches
  `Renewal::Refused`; store, audit, signing, role/corrupt-state,
  verification-task, and envelope-open failures reach `Renewal::Failed` and
  return without commit, session revocation, or stale-authority fallback.
- `FIND-TASK-003-15` remains documentation-only and closed.
- `FIND-TASK-003-16` is closed: all three renewal classifiers are
  `pub(crate)` and their behavior and callers are unchanged.
- `FIND-TASK-003-17` is closed: `open_text` assigns lifecycle policy to its
  caller, the classification prose excludes envelope-open failure from
  ordinary refusal, and
  `missing_or_unopenable_renewal_credential_is_retryable_failure` directly
  proves both absent and unheld-key envelopes become
  `Renewal::Failed(WyrdError::Internal)`.
- `HD-TASK-003-R2-1` remains closed. Candidate testing performs a real
  authorization-code sign-in through the ordinary provider verification
  boundary, re-checks the initiating tester's current stored permission,
  stamps only the exact candidate revision, and returns before User, role,
  credential, completion, refresh, or browser-session issuance.

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

None. The residual mix-up exposure for providers that neither advertise nor
send RFC 9207 `iss` is documented, explicitly accepted by the human owner, and
has a named per-connection-callback upgrade path. It is not a finding in this
review.

### Positive Controls

- Tenant and connection authority come from server-bound state and verified
  credentials, never route keys, cookie names, headers, email, or provider
  claims alone.
- Login and connection-test state is random, digest-indexed, expiring, and
  single-use. Callback processing consumes it before provider token IO.
- PKCE, nonce, exact redirect binding, issuer/audience/algorithm/key/time
  checks, `azp`, screened and pinned provider IO, and revision binding are
  applied at the shared callback owner.
- Test sign-in re-authorizes its initiating principal from current tenant
  state and transactionally audits the result; its branch issues no identity
  or credential authority.
- The BFF channel authenticates every call before store access. Its service
  key grants only session operations and never supplies tenant or Wyrd API
  authority.
- Browser cookies remain host-only, Secure, HttpOnly, SameSite=Lax, path `/`,
  and expiry bounded. Mutations require POST, exact same origin, and a
  constant-time session-CSRF comparison.
- Raw flow/session identifiers and all recoverable credentials are absent from
  durable plaintext. Session rows store only the session digest and sealed
  access, refresh, API-key, and CSRF material under the canonical deployment
  keyring.
- Tenant SQL uses `TenantConn` and forced RLS after narrow digest-to-tenant
  resolution. Cross-tenant browser state cannot be selected by a caller.
- API-key refusal performs one Argon2 verification and exposes one public
  refusal. Refresh rotation is single-use, replay revokes the complete family,
  and the canonical audit shares the containing transaction.
- Internal renewal failures fail the request, roll back tentative credential
  writes, retain the session for repair, and do not serve an old token as a
  fallback. R4 did not widen this policy into a public Rust contract.

## Verification limits

- This domain reviewer did not rerun Cargo, mise, pnpm, Postgres, Keycloak,
  Dex, browser, or TLS lanes. The R4 implementation record reports both exact
  unit selectors, all four exact Postgres renewal selectors, `test:wyrd`,
  `test:identity:journey`, `check:tenant-isolation`, `fmt`, `lints`, and
  `git diff --check` green. This review independently inspected the production
  paths and the asserted proof boundaries. The cumulative immutable range also
  passes `git diff --check` in this review.
- The non-public routing of `/internal/bff/v1/*` remains a deployment gateway
  obligation from the approved packet. Repository proof covers service-key
  admission and real trusted TLS transport; it cannot prove an operator's
  external ingress policy after deployment.
- No dependency manifest or lockfile changed in the cumulative range, so no
  new supply-chain surface required assessment.

## Overall result

**PASS**

The cumulative candidate satisfies the reviewed security, RBAC, identity,
tenant-binding, credential-secrecy, audit, provider-mix-up, browser-session,
and renewal-failure obligations. R4 closes its two bounded contract/proof gaps
without changing runtime behavior or reopening any prior finding.
