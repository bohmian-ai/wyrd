# Security, RBAC, authentication, federation, refresh, and audit domain review

**Result: FAIL**

**Immutable subject:** base `c5c20754a167e8f4d74a555a720bd51df6179a6f`, candidate
`96a1bd81b1d028fa81d6e0f385e3cd9b62080e30`.

**Approved authority:** `changes/active/admin-principals/spec.md`, revision 10,
SHA-256 `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`,
plus all eight original task packets, `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`, and the
v1 security, service-identity, permission-check, permission-model, and tenancy
authorities.

## Reviewed boundary

This review traced the cumulative candidate through both identity planes: Wyrd
credential and OIDC entry, token/session issuance, tenant token verification,
platform session confirmation, role resolution, authorization epochs, human
refresh rotation and replay, platform and tenant authorization, canonical audit
staging, public error mapping, and their served callers. It also rechecked the
security-relevant whole-branch-03 findings against source; appended remediation
evidence was treated only as a pointer to validate.

| Boundary | Authority | Source and reachable-caller coverage | Result |
|---|---|---|---|
| Tenant token trust and revocation | `INV-011`, `INV-013`; security foundation fail-closed rule | `wyrd-auth-verify::TokenVerifier::verify` -> production `boot::auth::build_auth_handles` -> `SqlRevocationCheck` | **FAIL — `SEC-R4-1`** |
| OIDC-derived human roles | `REQ-034`, `INV-013`; security-posture role-binding epoch rule | tenant callback -> `replace_user_roles` -> signed role claims -> permission resolver and token cache | **FAIL — `SEC-R4-2`** |
| Human refresh and replay | `REQ-048`, `AC-009`; refresh rotation/reuse posture | `/auth/token` refresh branch -> `RefreshTokens::execute` -> refresh-family revoke and canonical append | **FAIL — `SEC-R4-3`** |
| Tenant machine exchange | service-identity token-exchange audit rule; `REQ-037`, `AC-009` | `/auth/token` API-key branch -> `ExchangeApiKey::execute` -> card-free/card-bound issuance | **FAIL — `SEC-R4-4`** |
| Platform credential/OIDC sessions | `REQ-041`-`REQ-046`; service-identity token-exchange audit rule | `/auth/platform/token`, `/auth/platform/callback` -> `PlatformSessions::{exchange,issue_federated}` | **FAIL — `SEC-R4-5`** |
| Plane separation and authorization | `REQ-018`, `INV-015`; permission-check authority | tenant bearer extractor, platform extractor/session confirmation, `PlatformAuthorization`, tenant route decisions | PASS for inspected paths |
| Credential and provider-secret handling | `REQ-008`, `REQ-012`, `REQ-043`; security posture | Argon2 verifier persistence, once-returned secrets, sealed OIDC secrets, redacted boundary types and diagnostics | PASS for inspected paths |
| Tenant isolation | `INV-007`; tenancy authority | credential-derived tenant selection, `TenantConn`, RLS-backed principal/role/issuer reads | PASS for inspected paths |
| Public auth errors | `INV-011`, `INV-012`; stable error catalog | credential rejection, verifier/store failures, admin SQL-conflict mapping | PASS for the prior disclosed-constraint defect and inspected invalid-credential paths |

## Security Audit

### Critical

- None.

### High

- **`SEC-R4-1` — INCORRECT — [`crates/shared/wyrd-auth-verify/src/lib.rs:485`](../../../../../crates/shared/wyrd-auth-verify/src/lib.rs) revocation uncertainty explicitly fails open.**
  **Violated obligation:** approved-spec `INV-011` requires unknown or
  unverifiable state to deny, `INV-013` forbids a token or cached permission set
  from outliving revocation, and `architecture/v1/00-foundations/security.md`
  says uncertainty in revocation state fails closed. **Evidence:** both the
  positive-cache path at lines 487-505 and the fresh-verification path at lines
  527-545 log resolver failure and continue to return the token. The unit test at
  lines 1710-1725 makes this behavior explicit. Production installs
  `SqlRevocationCheck` at
  `crates/wyrd/wyrd-server/src/boot/auth.rs:77-84`; its every-request tenant
  admission and epoch reads convert database errors to `ResolveError::Unavailable`
  at `crates/wyrd/wyrd-auth/src/revocation_resolver.rs:106-156`.
  **Exploit path and consequence:** a holder of a stolen or revoked tenant-admin,
  Service, Agent, or User access token can continue privileged requests during a
  Postgres/revocation-resolver outage. A cache hit also retains the previously
  resolved permission set. An attacker need not forge a token; inducing or
  waiting for pool exhaustion/backend unavailability bypasses suspension and
  epoch revocation for otherwise-valid bearer material. **Testable correction:**
  make revocation-resolution unavailability invalidate/withhold any positive
  result and return the existing verifier-unavailable error, which the served
  boundary maps to the stable retryable 503. Add focused cache-hit and
  cache-miss tests plus a real-route test with an unavailable revocation store;
  all must refuse the token, while known-current and known-revoked epochs retain
  their existing behavior.

- **`SEC-R4-2` — INCORRECT — [`crates/wyrd/wyrd-auth/src/callback.rs:187`](../../../../../crates/wyrd/wyrd-auth/src/callback.rs) persists a changed OIDC role set without advancing the User authorization epoch.**
  **Violated obligation:** approved-spec `INV-013` requires role-grant
  revocation to take effect by the next request; the security posture at lines
  127-133 requires a role-binding revocation to advance the applicable epoch
  transactionally. **Evidence:** callback lines 197-215 replace all stored roles
  and issue a successor session in one transaction, but never advance
  `auth_users.tokens_not_before`. `REPLACE_USER_ROLES_SQL` at
  `crates/wyrd/wyrd-sql/src/queries/auth/role_assignments.rs:33-48` deletes
  omitted bindings and inserts new ones only. Existing access tokens retain role
  names in their signed claims, and
  `crates/shared/wyrd-auth-verify/src/lib.rs:934-957` resolves permissions from
  those claim roles; `SqlPermissionResolver` at
  `crates/wyrd/wyrd-auth/src/permission_resolver.rs:26-50` reads role definitions,
  not the User's current bindings. **Exploit path and consequence:** after an
  administrator removes `runtime_admin` (or another privileged group) at the
  IdP and a subsequent Wyrd login persists that reduced role set, a stolen
  pre-change access token still carries and exercises the removed role until
  its 15-minute expiry. A cached verification can preserve the same authority.
  This violates next-request revocation and gives a recently deprivileged human
  an actionable privilege window. **Testable correction:** in the transaction
  that changes the persisted role set, advance the User authorization epoch and
  ensure the newly issued reduced-authority token is admitted at or after that
  epoch. Preserve no-op login behavior when the set is unchanged. A served OIDC
  test must capture an initial admin token, log in after removing the IdP role,
  prove the old token is refused on its next request, and prove the successor
  token has only the new roles.

### Medium

- **`SEC-R4-3` — INCORRECT — [`crates/wyrd/wyrd-auth/src/refresh.rs:162`](../../../../../crates/wyrd/wyrd-auth/src/refresh.rs) omits the consumed refresh credential from the replay audit.**
  **Violated obligation:** `REQ-048` requires human refresh continuation to name
  the consumed refresh credential in audit, while `REQ-037`/`AC-009` require
  exact credential attribution. **Evidence:** the replay branch has the stale
  row and its `stale.id`, revokes the family, and appends
  `auth.refresh.revoke_family`, but lines 176-189 never attach
  `.with_credential_id(Some(stale.id))`; `auth_event` defaults that field to
  `None` at `crates/wyrd/wyrd-auth/src/audit.rs:47-70`. By contrast, successful
  rotation attaches `rotated_from` at
  `crates/wyrd/wyrd-auth/src/callback.rs:447-462`. The committed-state replay
  test at `refresh.rs:772-787` asserts only the operation and principal count.
  **Consequence:** the newly durable theft-response event cannot identify which
  consumed refresh row triggered family revocation, weakening exact incident
  attribution and failing the approved acceptance criterion. **Testable
  correction:** attach the already-known stale row id to the same canonical
  event. Extend the separate-transaction replay test and served human OIDC
  journey to assert that the committed denial names that UUID and still leaves
  the successor unusable.

- **`SEC-R4-4` — MISSING — [`crates/wyrd/wyrd-auth/src/exchange_api_key.rs:401`](../../../../../crates/wyrd/wyrd-auth/src/exchange_api_key.rs) does not append the required tenant token-exchange audit.**
  **Violated obligation:** the v1 service-identity authority requires token
  exchange to stage a durable row through the canonical append in the same
  transaction; `crates/wyrd/wyrd-auth/src/audit.rs:20-22` defines
  `auth.token.exchange` for every issued access token, and `AC-009` requires
  credential attribution. **Evidence:** the served route commits the transaction
  at `crates/wyrd/wyrd-server/src/components/auth/routes.rs:130-162`.
  Card-free issuance at `exchange_api_key.rs:401-434` only signs and returns, so
  a tenant-admin exchange commits no audit row. Card-bound issuance at lines
  444-504 writes only `auth.card_scope.mint`; its builder at
  `crates/wyrd/wyrd-auth/src/card_scope.rs:172-205` does not attach the API-key
  credential id and is not the required token-exchange event. **Consequence:** a
  stolen tenant administrative or machine credential can mint a privileged
  access token without a durable record of the credential-to-session grant;
  audit failure cannot fail the grant closed. **Testable correction:** stage one
  canonical `auth.token.exchange` event in the existing `TenantConn` grant
  transaction for both card-free and card-bound API-key exchange, naming the
  principal and `row.api_key_id`; keep the distinct card-scope event for its
  separate scope fact. Real-route tests must prove exact attribution after
  commit and prove an injected append failure returns no token and commits no
  grant-side effects.

- **`SEC-R4-5` — MISSING — [`crates/wyrd/wyrd-auth/src/platform_sessions.rs:142`](../../../../../crates/wyrd/wyrd-auth/src/platform_sessions.rs) issues privileged platform sessions outside the canonical audit transaction.**
  **Violated obligation:** the v1 service-identity authority requires every
  token exchange to durably audit in the same transaction, and the approved
  platform-human flow requires individually attributable administration.
  **Evidence:** credential exchange at lines 142-161 authenticates, independently
  updates `last_used_at` through
  `platform_credentials.rs:198-224`, signs, and returns without canonical audit.
  Federated issuance at `platform_sessions.rs:177-192` likewise signs after a
  principal read without audit. Their reachable routes are
  `components/platform/routes.rs:83-112` and
  `components/platform/identity.rs:705-743`; neither passes audit/request
  context. Platform OIDC completion at
  `wyrd-auth/src/platform_login.rs:214-263` can also pin an identity and return a
  session with no token-exchange row. **Consequence:** compromise or use of a
  global credential or federated platform identity can produce an administrative
  session with no durable issuance record, and a broken audit append cannot
  stop issuance. Later authorization rows show session use but cannot establish
  the credential or federated login event that created it. **Testable
  correction:** use the existing platform/system canonical-audit transaction
  for platform credential and federated session issuance; credential exchange
  must name its credential id, while federated issuance must name the resolved
  human principal and its credential-free identity context. Served tests must
  prove committed rows for both paths and fail-closed behavior when audit
  staging is injected to fail.

### Low / Defense In Depth

- None. Optional hardening was excluded from this acceptance review.

### Positive Controls

- Platform and tenant bearer tokens are separated by verified claims and
  extractor type; the platform extractor re-reads credential/principal/grant
  state on every request rather than accepting tenant authority or cached
  platform authority.
- Tenant and platform credential rejection performs one Argon2 verification
  against either the real verifier or a fixed dummy, preserving the single
  public invalid-credential response across malformed, unknown, revoked,
  expired, wrong-secret, and suspended cases.
- Credential plaintext is generated server-side, stored only as an Argon2
  verifier, and returned once. OIDC client secrets use redacted secret types and
  sealed persistence; inspected read, error, log, trace, and audit paths do not
  expose them.
- Human refresh secrets are one-way stored and rotate once; machine grants no
  longer receive refresh tokens. Replay now commits family revocation and its
  audit before returning the existing refusal, closing the whole-branch-03
  rollback defect.
- Platform OIDC uses discovery, PKCE, nonce verification, explicit issuer and
  audience checks, a verified-email first pin, and no just-in-time platform
  principal provisioning. The canonical issuer correction is present.
- Inspected platform and tenant administrative authorization decisions use the
  existing same-transaction canonical audit seams and fail closed when their
  decision row cannot be staged. Public admin conflicts now keep physical
  database constraint identifiers in structured server diagnostics only.

## Prior-finding closure

| Prior finding | Independent source result |
|---|---|
| `FIND-admin-principals-3` / `SEC-R3-3` | **CLOSED.** Admin duplicate/reference conflicts return static operation-specific messages; physical constraints are logged only by `trace_conflict`. |
| `FIND-admin-principals-R2-3` | **CLOSED for renewal-model separation.** API-key/workload grants carry no refresh token; User rotation re-reads persisted roles and attributes successful rotation to the consumed row. `SEC-R4-2` is a distinct live-access epoch defect. |
| `FIND-admin-principals-R2-4` / `SEC-R3-1` | **CLOSED for durability.** The served `Reused` branch commits family revocation and the canonical row, and the separate-transaction test observes the dead successor. `SEC-R4-3` narrows the remaining exact-attribution gap. |
| `FIND-admin-principals-R3-1` | **CLOSED.** Platform authorization callers supply the actual operation resource, including truthful provisioning retry identity. |
| `FIND-admin-principals-R3-2` / `SEC-R3-2` | **CLOSED.** Platform OIDC persists and reuses `IssuerUrl::as_str()`; the trailing-slash configuration no longer splits registration from callback identity. |
| `FIND-admin-principals-R3-3` | **CLOSED.** Tenant trusted-issuer input and CLI resolution use existing secret wrappers/redacted debug behavior and retain write-only schema projection. |
| `FIND-admin-principals-R3-5` | **CLOSED in this domain.** The canonical audit schema carries optional credential identity through staging/publication while preserving the historical null-credential hash shape. |

## Verification limits

This was a review-only static audit of the complete immutable range and
surrounding reachable production callers. No source was changed and no dynamic
test was rerun. Prior appended command results were used only to locate and
then inspect the claimed closure in source. Existing tests positively encode
the fail-open behavior in `SEC-R4-1`, do not exercise an old access token after
OIDC role replacement (`SEC-R4-2`), assert no credential on the replay event
(`SEC-R4-3`), and do not inject canonical token-exchange audit failure into the
tenant or platform issuance paths (`SEC-R4-4`/`SEC-R4-5`). These are bounded
implementation findings; none requires a new product or public-contract
decision.

## Overall

**FAIL.** Proposed finding IDs: `SEC-R4-1`, `SEC-R4-2`, `SEC-R4-3`,
`SEC-R4-4`, and `SEC-R4-5`.
