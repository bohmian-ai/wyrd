# Domain Review: Tenancy and Canonical Audit

## Immutable subject

- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Overall result: **FAIL**

## Reviewed boundary

This review traced the tenant and principal authority used by every changed
authorization-server grant from its externally supplied routing material to the
server-owned authority, SQL capability, transaction owner, canonical audit
append, and commit or rollback boundary. It covered authorization initiation
and callback state; authorization-code and device-code redemption; refresh,
revocation, API-key exchange, delegation, and workload JWT bearer; tenant token
issuance; platform credential exchange; the changed auth SQL queries and
migrations; and the focused tests that claim connection-lifecycle and audit
closure.

The lead-directed API-key shape is present: the tenant and platform endpoints
accept RFC 8693 token exchange with
`subject_token_type=urn:wyrd:oauth:token-type:api_key`, and
`wyrd_api_key` is rejected rather than retained as an alias
(`crates/wyrd-spec/src/auth/token.rs:75-85, 288-318`,
`crates/wyrd/wyrd-server/src/components/auth/routes.rs:168-189`,
`crates/wyrd/wyrd-server/src/components/platform/routes.rs:98-112`). The
deleted `/internal/bff/v1` consumers were not treated as a TASK-010 defect.

## Authority and source coverage

| Boundary | External or unverified input | Server-bound authority and isolation | Transaction and audit result |
|---|---|---|---|
| Authorization initiation and provider callback | Tenant route slug, OAuth client parameters, provider `state`, code, and optional `iss` | The slug is pre-login routing only. `OidcLogin::begin_bound` stores the canonical tenant, exact connection revision, provider issuer/client/callback/PKCE/nonce, and OAuth-client redirect/PKCE binding under `TenantConn`. The callback hashes random state, uses the narrow `auth_login_state_tenant` definer lookup only to route, then consumes the row under tenant RLS before provider IO (`login.rs`, `callback.rs`, `queries/auth/login_state.rs`, migration `20260925000001`). | State consumption commits before provider IO, making replay terminal. Final principal/role/code or device-approval writes use one later `TenantConn`, but findings TA-001 and TA-002 show that this transaction is not fully fenced or audited. |
| Authorization-code redemption | Tenant UUID embedded in the opaque code, authenticated client, redirect URI, verifier | The prefix only selects a `TenantConn`; deletion by the stored SHA-256 under forced RLS is authority. The returned row supplies principal, exact client, exact redirect, S256 challenge, expiry, and connection revision (`callback.rs:445-543`, `login_state.rs:89-101`). | The row is spent on refusal. Success rechecks the connection through `TenantTokenIssuer::issue_human_session`, appends canonical token-exchange audit, and commits the refresh row, audit, and code deletion together. |
| Device authorization and redemption | Tenant route slug, user code, tenant-prefixed device code | Creation resolves the canonical tenant and active connection; approval and denial use `TenantConn`; redemption treats the prefix as routing only and authorizes by the hashed row visible under forced RLS (`cli_logins.rs`, `queries/auth/device_authorizations.rs`). | Pending/slow-down state commits only poll bookkeeping; terminal refusal deletes and commits; approved redemption deletes, rechecks the exact active connection, mints, appends token and device-grant audit, and commits atomically. Callback-side approval remains affected by TA-001 and TA-002. |
| API-key exchange | RFC 8693 API-key `subject_token`; tenant UUID and prefix embedded in the key | The parsed tenant only opens `TenantConn`; the row lookup and Argon2 verification under RLS establish the credential and principal. Malformed inputs receive the dummy constant-cost verification (`routes.rs:240-286`, `exchange_api_key.rs`, `credential_verify.rs`). | Last-use metadata, activity, tenant issuance, and canonical audit commit on the same tenant transaction; errors drop the transaction and use the canonical best-effort refusal path when a tenant is routable. |
| Refresh and revocation | Unverified tenant claim in refresh JWT, authenticated OAuth client | The claim only selects `TenantConn`; the refresh digest row under RLS supplies tenant, principal, client, family, and connection binding. The principal-family lock and connection-slot lock serialize rotation/reuse and exact-connection revalidation (`routes.rs:332-385`, `refresh.rs`, `cli_logins.rs:387-470`, `queries/auth/refresh_tokens.rs`). | Success, CLI rotation, refresh-row mutation, token audit, and commit share the tenant transaction. Detected reuse commits family containment and its audit before refusal. Revocation is idempotent and commits its audit with the mutation. |
| Delegation | Tenant claim in unverified subject JWT, subject token, actor token, audience | The subject claim only routes to `TenantConn`; `DelegateToken` verifies both signed tokens against that tenant and computes authority from current tenant principals and grants. A cross-tenant actor fails verification (`routes.rs:289-329`, `delegate_token.rs`). | The grant and allowed or denied policy decision use the existing issuer and canonical tenant audit transaction. |
| Workload JWT bearer | Host or fallback tenant slug and external assertion | Host/payload selects only a candidate tenant. The external verifier validates the assertion for that tenant and token policy; the exact `(tenant, issuer, subject, audience)` workload binding then resolves a Card, and `TenantConn` resolves the active Service/Agent principal (`wyrd-server/src/auth/jwt_bearer.rs:26-88`, `wyrd-auth/src/jwt_bearer.rs:51-139`). | Tenant issuance and canonical audit commit together. A verified or resolved refusal is recorded best-effort on the same canonical path. No provider assertion is used directly as Wyrd authority. |
| Tenant token issuance | Principal UUID selected by a verified grant | `TenantTokenIssuer::issue` derives tenant from `TenantConn`, re-reads tenant admission, active principal, roles, permissions, and Card scope, then signs (`issuance.rs:389-517`). | `append_auth_audit` writes through the sole `vala.audit_staging` owner before the caller commits. An append failure returns no token. |
| Platform exchange and plane separation | RFC 8693 API-key subject token | `/auth/platform/token` constructs `PlatformSessions` over `OperatorPool`; tenant `TokenGrants` never accepts platform sessions, while platform routes use `PlatformCaller` and reject tenant tokens (`components/platform/routes.rs:1-12, 54-142`, `platform_sessions.rs:145-184`). | Platform authentication, mint, canonical audited-operator append, and commit are one transaction. No tenant `TenantConn` or tenant principal is projected into the platform session. |
| SQL isolation and rollback | Tenant IDs used to acquire database capability | `TenantConn` binds typed `DataTenantId` to transaction-local `app.current_tenant`; forced RLS owns row visibility, and dropping without commit rolls back (`tenant_conn.rs:15-81`). The login/device/refresh query modules accept `TenantConn`, not a raw pool. | Changed durable grant rows and `vala.audit_staging` entries share their owning tenant transaction except the documented best-effort refusal records. |

Applicable authority read for this boundary included `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/wyrd-design.md` runtime identity,
`architecture/wyrd-security-posture.md`, and the v1 security, tenancy, SQL
foundation, and Postgres-layout authorities, plus the approved specification,
task, and TASK-004 r2 lead routing.

## Verification limits

This was a source and diff audit of the immutable cumulative range. I did not
rerun the implementation evidence commands. The task records passing focused
identity filters, `test:principals:integration`, `test:sql`, platform and CLI
journeys, code generation, boundary checks, format, and lints. Those results do
not exercise the two callback invariants below: the existing cross-replica
journey deliberately accepts a code after a concurrent deactivation and checks
only that later redemption mints nothing, while the audit-failure test injects
failure only for a role-sync event on a login that changes roles. Per task
direction, complete unfiltered journeys remain change-review evidence; closure
here needs only focused Postgres callback tests and the narrow owner lane.

## Security Audit

### Critical

- None.

### High

#### TA-001 — INCORRECT: callback completion is not fenced against connection replacement

- **Violated obligation:** REQ-016 requires deactivation, deletion, or
  replacement to block new login through the old connection immediately;
  INV-001 requires the connection after login starts to remain selected only by
  verified server-bound authority; INV-007 requires the shared tenant User and
  role authority to remain fail closed. The task also prohibits changing role
  mapping or tenancy semantics.
- **Location:** `crates/wyrd/wyrd-auth/src/callback.rs:342-441`, especially the
  committed `active_connection` read at line 363 followed by a separate tenant
  transaction at lines 365-441; the lifecycle serialization owner is
  `crates/wyrd/wyrd-auth/src/connections.rs:703-719`; the established issuance
  ordering and exact in-transaction recheck are
  `crates/wyrd/wyrd-auth/src/issuance.rs:700-759`.
- **Evidence:** `bound_connection` calls `active_connection`, whose read commits
  its own transaction at `connections.rs:642-666`. The callback then opens a
  new `TenantConn`, may create the User, acquires only the principal-family
  lock, replaces durable roles, appends role audit, and stores a code or device
  approval before commit. It never acquires `lock_human_connection_slot` and
  never rechecks the exact binding in that transaction. Activation,
  deactivation, and removal do acquire that slot lock. The reachable race is
  already constructed by
  `crates/wyrd/wyrd-server/tests/identity_e2e.rs:3422-3565`: the callback passes
  its connection check, waits during role replacement, another replica commits
  deactivation, and the callback still returns a newly issued code.
- **Plausible exploit scenario:** during a same-issuer connection replacement
  that removes or changes a privileged group mapping, a subject starts a login
  through the old revision and delays completion. After the replacement has
  committed, the stale callback can win this window and overwrite the shared
  `(issuer, subject)` User's durable roles from the retired revision. The stale
  code itself is refused at redemption, but a session established through the
  replacement reads the now-stale shared roles on its next refresh and can
  receive authority the replacement mapping removed.
- **Observable consequence:** an inactive connection can still create a User,
  mutate current durable role grants, append an allowed role-sync record, and
  issue a code or approve a device grant after the lifecycle mutation has
  committed. Later redemption's connection check prevents that particular old
  grant from minting, but it does not roll back the already committed identity
  and role changes.
- **Required testable correction:** reuse the existing human-connection slot
  lock and exact active-binding check in the callback's final `TenantConn`, in
  the established principal-family-then-connection-slot lock order, before any
  role, code, or approval can commit. A focused Postgres concurrency test must
  park completion, commit replacement/deactivation on another connection, then
  prove completion is refused and leaves no User/role mutation, code, device
  approval, or allowed role-sync audit. Preserve state consumption before
  provider IO and redemption's existing defense in depth.

### Medium

#### TA-002 — REGRESSION: successful provider login outcomes can commit without canonical audit

- **Violated obligation:** REQ-017 requires security-significant login outcomes
  and role changes to produce redacted canonical audit evidence under their
  owning authority. INV-007 requires browser and CLI login to share the
  canonical audit authority. TASK-010 prohibits changing audit ownership or
  semantics.
- **Location:** `crates/wyrd/wyrd-auth/src/callback.rs:385-441` and
  `crates/wyrd/wyrd-auth/src/audit.rs:19-30, 117-160`.
- **Evidence:** callback completion appends `auth.user.roles.sync` only when
  `replace_user_roles` reports a change. With unchanged roles, the same
  transaction can commit a newly issued authorization code or device approval
  without any `append_auth_audit` call. If the grant expires or is abandoned,
  the later redemption audit never exists, so the successful provider login
  outcome has no canonical record. The focused failure test at
  `crates/wyrd/wyrd-server/src/auth/callback.rs:571-620` covers only a changed
  role set and fails the role-sync operation; it does not exercise a successful
  unchanged-role callback or the login-outcome audit itself.
- **Impact:** a tenant's successful OIDC authentication and authorization-code
  issuance or device approval can be absent from retained security history.
  Audit unavailability is not observed on an unchanged-role completion because
  the callback attempts no append, yet it returns the code or approval.
- **Required testable correction:** record every successful non-test OIDC login
  outcome through the existing canonical append in the same callback
  `TenantConn` that commits the authorization code or device approval,
  independently of whether roles changed. Keep the distinct role-change record
  when a mutation occurs and keep token/grant audit at redemption. Focused
  Postgres tests for authorization-code and device completion with unchanged
  roles must prove the allowed login outcome exists, and an injected failure of
  that append must prove no code or approval commits.

### Low / Defense In Depth

- None. No optional hardening or nonstandard check is proposed.

### Positive Controls

- Externally supplied tenant identifiers consistently route to a typed
  capability; forced RLS rows or verified server bindings, not the routing
  value, establish effective tenant and principal authority.
- Authorization and device codes are stored only by SHA-256 digest, and the
  SQL delete/lock transitions provide single-use behavior in the owning tenant
  transaction.
- `TenantTokenIssuer` is the common token authority and stages successful
  tenant token decisions through `vala.audit_staging` before a token is
  returned.
- Refresh-family and connection-slot locks give refresh, rotation, reuse
  containment, and token issuance a coherent transaction order.
- Platform credential exchange stays on `OperatorPool` and the platform token
  profile; tenant grants stay on `TenantConn`, preserving the two-plane
  boundary.
- No compatibility alias for `grant_type=wyrd_api_key` was introduced, and no
  custom replacement for the RFC 8693 API-key exchange is present.

## Overall result

**FAIL.** TA-001 is a reachable authorization and tenancy race that lets a
retired connection mutate the shared User/role authority after lifecycle commit.
TA-002 leaves successful callback outcomes unaudited when the grant is not
redeemed. Both corrections are bounded to the callback's existing tenant
transaction, connection lock, and canonical audit owners; neither requires a
new mechanism, option, compatibility path, or specification decision.
