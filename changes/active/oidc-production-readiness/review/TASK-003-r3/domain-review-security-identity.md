# Security and identity domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `8289fa298ed33d21f2568558bc0a02905fd0b218`
- Approved authority: `SPEC-oidc-production-readiness` revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation: `changes/active/oidc-production-readiness/review/TASK-003-r2/TASK-003-R2-production-ui-remediation.md`
- Human directions:
  - `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  - `changes/active/oidc-production-readiness/review/TASK-003-r2/human-direction-connection-test.md`

The checked-out candidate resolved to the requested immutable object before
source inspection and immediately before this report was written. The complete
base-to-candidate range was reviewed; `622a77028..8289fa2` was used only to
locate the latest remediation owners.

## Reviewed boundary

This pass traced the security and identity boundary through:

- candidate-test authorization, canonical audit, screened discovery and JWKS
  checks, PKCE/state/nonce construction, tester and candidate-revision binding,
  common callback consumption, RFC 9207 handling, code redemption, ID-token
  verification, permission re-check, tested-stamp commit, and refusal paths;
- ordinary browser login and candidate-test sibling paths through the shared
  `AuthorizationCodeExchange`, including active/candidate trust selection and
  the point where User, role, refresh-family, session, and completion creation
  are deliberately skipped for a test;
- login-state persistence, tenant lookup, RLS re-entry, one-use/expiry
  semantics, initiation-shape constraints, and the prohibition on a test
  completion;
- private BFF service-key admission, upstream-origin validation, native TLS
  verification, session-id/tenant binding, exact-origin and constant-time CSRF
  checks, safe metadata, authority retrieval, and secret-bearing requests;
- renewal after connection/key refusal, browser-session ciphertext inventory,
  keyless boot, and secret wiping; and
- real Keycloak/Dex setup, provider-produced callback parameter preservation,
  cross-provider and same-issuer mixed callbacks, cross-tenant/expired/replayed
  test state, wrong callback/secret, and browser-visible secret checks.

## Authority and source coverage

| Boundary | Authority | Source and proof inspected | Result |
|---|---|---|---|
| Real candidate test | Revision 7 REQ-003/007/017, AC-006; human real-sign-in direction | `wyrd-server/src/components/admin/identity.rs:198-265`; `wyrd-auth/src/connections.rs:316-520,765-820,856-898`; `wyrd-auth/src/callback.rs:76-374`; callback PG tests and `tenant_connection_test_sign_in_journey` | PASS |
| Issuer and token binding | Human issuer direction; REQ-006/007; `wyrd-security-posture.md:198-232`; OIDC Core and RFC 9207 conditional policy | `wyrd-auth/src/callback.rs:149-215,260-289,615-640,693-756`; `wyrd-auth-oidc/src/provider.rs`; callback contract and issuer-binding journey | PASS |
| Tenant and caller binding | REQ-002/003/006/015, INV-001/005; RLS and audit rules | `auth_login_state` migration and queries; `HumanConnections::{tested_candidate,stamp_test_sign_in}`; `tester_authorized`; tenant-definer lookup; callback tests for withdrawn authority and audit failure | PASS |
| No identity or credential issuance during test | REQ-003; human real-sign-in direction | Test branch returns at `wyrd-auth/src/callback.rs:279-290`, before `ensure_user_identity`, role replacement, issuance, or completion; database constraint forbids test completion; real journey counts Users, credentials, browser sessions, and completions | PASS |
| BFF channel, TLS, and upstream origin | TASK-003 private-channel contract; REQ-005/009 | `components/auth/bff.rs`; `wyrd-ui/src/lib/server/{upstream.ts,auth/server-sessions.ts}`; trusted TLS terminator in `identity_ui_e2e.rs`; upstream policy tests | PASS |
| Browser session lifecycle and secret storage | REQ-005/009/016; R2 findings 4/10/12/13 | `BrowserSessions::current`; browser-session SQL/migration; canonical sealed-secret inventory; keyless-boot and proactive-refusal tests; sequential cookie-hint resolution | PASS |
| Multi-provider and mixed callbacks | REQ-007/015, AC-003; R2 finding 6 | Keycloak and Dex fixture setup; `production multi-provider tenant switch`; helper that preserves all provider parameters and replaces only `state` | PASS |
| Secret and sensitive-data exposure | REQ-005/009; repository secret-handling authority | secret wrappers and redacted `Debug`; `skip_all` handlers; fixed callback pages/redirect; safe page metadata; `expectNoSecrets`; no manifest or lockfile change | PASS |

## Prior-finding and human-direction closure

- `FIND-TASK-003-1` remains closed under the human issuer direction. State is
  consumed before provider IO; present `iss` is byte-for-byte compared with the
  recorded issuer, absence is refused only when fresh metadata advertises RFC
  9207 support, and mismatches reach no token endpoint. Non-advertising
  providers remain usable under the explicitly approved residual-risk policy.
- `FIND-TASK-003-4` is closed. All four browser-session envelope queries select
  every non-null stored value without an expiry filter; expired ciphertext is
  therefore counted, rewrapped, and blocks keyless boot.
- `FIND-TASK-003-5` is closed. A production-built BFF performs an authenticated
  session operation through the repository-CA trusted TLS terminator using the
  native fetch path. The same origin owner still refuses non-loopback plaintext
  and an explicitly empty value before fetch.
- `FIND-TASK-003-6` is closed. The second active provider is Dex, not another
  Keycloak realm. Mixed callbacks preserve the provider's complete query,
  including genuine `iss`, while replacing only victim `state`; both the
  different-provider and same-issuer paths create no completion or session.
- `FIND-TASK-003-10` is closed. A refused proactive renewal rolls back its
  transaction and serves only the still-valid stored access token. The relock
  uses PostgreSQL expiry authority; the first post-expiry use revokes and
  refuses without trying another credential.
- `FIND-TASK-003-11` is closed by substantive documentation of the raw RFC 9207
  flag and the fallible discovery projection.
- `FIND-TASK-003-12` is closed by sequential resolution of distinct cookie
  hints through the existing `read` owner, retaining server verification and
  invalid-cookie clearing with a native concurrency bound of one.
- `FIND-TASK-003-13` is closed. Only an absent `WYRD_SERVER_URL` receives the
  loopback default; an explicit empty string reaches the URL parser and fails.
- `HD-TASK-003-R2-1` is closed. The old side-effect probes are absent. Testing
  now starts a real authorization-code flow bound to the exact candidate
  revision and initiating principal, uses the normal token verification path,
  re-checks current stored permission, transactionally records the canonical
  tested decision, stamps only that revision, and issues no User, credential,
  browser session, or redeemable completion. Cross-tenant, expired, replayed,
  wrong-callback, wrong-secret, authority-withdrawal, and audit-failure paths
  remain fail closed.

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
human-approved decision with a per-connection-callback upgrade path, not a
finding in this review.

### Positive Controls

- Candidate testing and ordinary login share one authorization-code exchange,
  screened provider client, PKCE, nonce, issuer/audience/algorithm/key/time
  checks, and typed callback contract; no probe or provider-specific bypass
  remains.
- State alone resolves tenant and connection. It is random, stored only as a
  hash, consumed and committed before provider IO, expiry-bound by PostgreSQL,
  and tenant-isolated when the callback re-enters through `TenantConn`.
- Candidate trust is tied to connection id, revision, issuer, and client id.
  The callback re-reads that candidate and later stamps under the connection
  slot lock, so replacement during the browser round trip cannot test a new
  revision.
- The initiating principal's current durable roles are re-resolved before the
  stamp. Allowed and denied decisions use the canonical transactional audit
  path; audit failure leaves the candidate untested.
- The connection-test branch returns before User resolution, role writes,
  refresh issuance, browser-session creation, or login completion, with a
  database constraint as a second guard against a test completion.
- BFF session operations require the deployment service key before store
  access, cannot derive tenant from that key, and carry secrets over native
  certificate-verified TLS except for the explicit literal-loopback allowance.
- Browser cookies remain host-only, Secure, HttpOnly, SameSite=Lax, path `/`,
  and expiry-bounded. The server-returned tenant is authoritative; mutating UI
  actions require POST, exact same origin, and constant-time session CSRF.
- Recoverable provider, access, refresh, bootstrap-key, and CSRF material is
  encrypted at rest, omitted from browser data/logs/errors/audit, covered by
  the canonical rewrap inventory, and wiped on browser-session revocation.
- Refused renewal cannot manufacture successor authority or commit tentative
  refresh/key-use writes; issued snapshot authority ends at its stored expiry.

## Verification limits

- Per assignment, this reviewer ran no Cargo, mise, pnpm, Postgres, Keycloak,
  Dex, browser, or TLS lane. The only available execution evidence is the R2
  remediation's recorded green focused and broader commands. This pass
  inspected the named tests and assertions but did not reproduce them.
- This static pass did not inspect live deployment gateway configuration. The
  task fixes `/internal/` non-public routing as a deployment responsibility;
  source enforces the independent service-key and TLS controls.
- No dependency manifest or lockfile changed in the cumulative range, so no new
  supply-chain boundary required review.
- CodeGraph was unavailable because the repository has no `.codegraph/`
  directory; callers and sibling paths were traced with repository-native
  search and direct source inspection.

## Overall result

**PASS**

The cumulative candidate satisfies the reviewed security, identity, TLS,
tenant-binding, audit, and secret-handling obligations. The latest remediation
closes the prior domain gaps and the human-directed real connection-test flow
without introducing a material task-scoped security defect.
