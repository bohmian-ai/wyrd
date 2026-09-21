---
id: SPEC-admin-principals
revision: 12
status: approved
approved_at: 2026-09-21
---

# Global and tenant administrative principals

## Objective and user value

Wyrd has no way to create a tenant. `platform.tenants` rows exist only because a
migration seeds the system sentinel or a test fixture inserts one directly. The
only administrative bootstrap that exists — `wyrd-server bootstrap-key` — mints
a credential for a tenant that must *already* exist, so every deployment is
unusable without out-of-band SQL.

This change implements one administrative identity model serving Wyrd SaaS,
self-hosted, and enterprise deployments, fully programmatic and headless, with
human OIDC authentication, credential rotation and recovery, and a foundation
for service accounts and finer-grained authorization.

```text
Global admin principal
    │
    ├── manages platform
    └── creates tenants
             │
             ▼
    Tenant admin principal
             │
             ├── configures tenant
             ├── configures OIDC
             ├── manages tenant identities
             ├── creates service principals
             └── manages credentials
```

Administrative identity is represented by durable principals, never by
credentials. **Credentials authenticate principals. Roles and permissions
authorize principals.**

The operator journey is three commands:

```text
1. start the server                       (no administrative state; platform routes refuse)
2. wyrd-server init                       -> wyrd_global_xxx, printed once to the operator terminal
3. wyrd tenant create --name acme         -> wyrd_tenant_xxx, printed once
```

## Current baseline

Established repository behavior this change builds on or must correct.

- `Principal { id, kind, tenant_id, roles, effective_permissions }` is the only
  runtime identity (`crates/shared/wyrd-runtime/src/principal.rs`). `tenant_id`
  is **required**; no tenant-free principal is representable.
- `PrincipalKind` is a doctrine-locked closed set: `User`,
  `Service { card_ref, card_ref_scope }`, `Agent { card_ref, card_ref_scope }`.
  Service and Agent are Card-bound, provisioned by `wyrd apply` keyed on
  `(tenant_id, principal_kind, card_kind, card_uid)`.
- `wyrd.auth_api_keys` binds a credential to `sa_id` in
  `wyrd.auth_service_accounts`, so only Card-bound principals can hold a
  credential. `wyrd.auth_refresh_tokens` is already principal-generic
  (`principal_kind`, `principal_id`) and is the precedent for generalizing.
- Argon2 hashing, tenant-prefixed key material, expiry, revocation,
  `last_used_at`, one indistinguishable invalid-key error, and
  `wyrd.audit_credential_issuance` already exist and are reused unchanged.
- `wyrd-server bootstrap-key --tenant <slug>`
  (`crates/wyrd/wyrd-server/src/boot/bootstrap.rs`, `main.rs:43`,
  `mise run cli:bootstrap-key`) is the current bootstrap. In one tenant
  transaction it resolves an **existing** tenant slug, seeds builtin roles,
  ensures a service account bound to a **fabricated** `CardRef`
  (`Service`, `system/bootstrap-admin@1.0.0`, no uid, never registered as a
  Card), grants `runtime_admin`, issues an API key through the production
  issuance seam, audits it, and returns the plaintext for one stdout print.
  `created_by` and the audit `issuer_principal_id` are `SYSTEM_OPERATOR_ID`
  (`bootstrap.rs:33`) — a fixed UUID projected as `PrincipalKind::User` with an
  empty `PermissionSet` that references no `auth_users` row and appears nowhere
  else in the codebase. Re-running reuses the service account but mints an
  additional credential.
- Roles are tenant-scoped rows in `wyrd.auth_roles`; `BUILTIN_ROLES` seeds
  `admin`, `writer`, `reader`, `agent`, `runtime_admin` per tenant.
  `seed_builtin_roles_for_tenant` is idempotent.
- `platform.tenants.status` is the closed set `active | suspended | deleted`.
  There is no provisioning state.
- `platform.users`, `platform.roles`, `platform.user_roles`, and
  `platform.api_keys` exist in the schema with query slots but no production
  caller — an email/password-shaped cross-tenant identity model predating the
  principal model, unreachable from any served surface.
- `wyrd.auth_user_identities` already keys federated human identity on
  `(data_tenant_id, issuer, subject)`.
- Migrations run on the `wyrd_migrator` DSN inside
  `WyrdPostgres::connect_from_dsns` (`wyrd-sql/src/postgres.rs:74`) before the
  `app` and `platform_admin` pools are built. No application credential gates
  database bootstrap or server start.
- Two and only two connection abstractions exist: `&mut TenantConn<'_>` (RLS,
  `wyrd_app`) and `&OperatorPool` (BYPASSRLS, `wyrd_platform_admin`).
- `components/admin/routes.rs` states it deliberately writes no audit row,
  contradicting the current agent rule that every authorization decision is
  transactionally audited.

Existing implementation is evidence, not authority, and may require correction
to satisfy this specification.

## Scope

- Durable principals with a type set covering global administration, tenant
  administration, humans, and machine identities, with tenancy present only
  where it applies.
- A principal-generic credential model: multiple concurrent credentials per
  principal, independent revocation, overlap-based rotation, verifier-only
  persistence.
- One authentication pipeline producing one authenticated context, and one
  authorization pipeline consuming it.
- Deployment initialization establishing the global administrative principal.
- Tenant provisioning as one authorized, transactional, resumable operation.
- Tenant administration: configuration, OIDC configuration, principal
  management, role grants, credential lifecycle.
- Global-administrator recovery of tenant administration.
- OIDC human identity resolution into the same authenticated context.
- Platform authority as a grant, held by the bootstrap global principal and by
  human platform principals.
- An optional deployment-owned platform-scope OIDC connection and the platform
  login entry it serves.
- Tenant-created service principals with restricted grants.
- Removal of `bootstrap-key` and its fabricated Card and operator identity.
- HTTP, OpenAPI, CLI, SDK, and MCP projections, audit, and documentation.

## Non-goals

- *Tenant* OIDC configuration: discovery, JWKS, client credentials, claim
  mapping, group-to-role mapping, PKCE login flow, and tenant login routing.
  Owned by `SPEC-tenant-oidc-federation`. This change owns the
  identity-resolution seam (`REQ-034`) and the platform-scope connection
  (`REQ-043`), and reuses that spec's verification mechanics rather than
  defining its own.
- A new RBAC engine, customizable role editor, permission-scope redesign,
  explicit deny, grant options, or ownership semantics. Existing roles and the
  approved object-scoped `Permission` remain the only static grant model, and
  administrative capabilities may remain internal rather than customer-editable.
- A `Principal`, `Credential`, or `Tenant` Card kind. Doctrine's 16 registrable
  kinds are unchanged; administrative identity is server state.
- Global-administrator credential recovery as an application-level path. Losing
  every global credential is deployment-level recovery through operator access
  to the database and secret store (`REQ-032`).
- Billing, plans, quotas, an `Organization` noun, custom domains, tenant
  deletion or data destruction, and tenant migration.
- Changing `wyrd apply` Card-bound principal provisioning, delegation chains, or
  the emit card scope.
- A SaaS customer-facing signup UI. The SaaS control plane is an ordinary holder
  of a global administrative credential.

## Definitions

- **Principal** — a durable server-owned identity that holds role grants and is
  named in audit. It survives credential rotation, revocation, and loss.
- **Credential** — a secret authenticating exactly one principal, verified
  against a stored one-way verifier.
- **Platform control plane** — operations over the tenant directory: tenant
  lifecycle and tenant-administration recovery.
- **Tenant control plane** — operations over one tenant's resources.
- **Initialization** — the one-time establishment of the deployment's global
  administrative principal and its first credential.

## Required behavior

### Principals

- **REQ-001**: A principal MUST exist independently of any credential. Creating,
  issuing, rotating, revoking, expiring, or losing credentials MUST NOT create,
  destroy, or alter a principal or its role grants.
- **REQ-002**: The durable principal record MUST carry id, type, optional
  tenant, name, status, and creation and update times.
- **REQ-003**: The principal type set MUST distinguish global administration,
  tenant administration, human identity, and machine identity. Tenancy
  constraints MUST be enforced durably: a global administrative principal MUST
  have no tenant; tenant administrative, human, and machine principals MUST have
  exactly one.
- **REQ-004**: Card binding MUST become a property of a machine principal rather
  than a precondition for being one. Existing Card-bound Service and Agent
  principals keep their `card_ref`, `card_ref_scope`, `wyrd apply` provisioning,
  and emit-scope behavior unchanged. A tenant administrative principal or
  tenant-created automation principal MUST be representable with no Card.
- **REQ-005**: Principal status MUST gate token issuance. A suspended or deleted
  principal MUST NOT receive a new token. An already-issued tenant token remains
  valid only until its five-minute expiry; a platform request MUST observe the
  current principal status. Suspension and deletion MUST NOT destroy the
  principal's grants or data.

### Credentials

- **REQ-006**: Credentials MUST be principal-generic: the durable record
  identifies its owning principal, and issuance, listing, revocation, and
  authentication work uniformly for every principal permitted to hold one.
- **REQ-007**: One principal MUST be able to hold multiple simultaneously valid
  credentials. Revoking or expiring one MUST NOT affect another, the principal,
  or its authorization.
- **REQ-008**: Credential creation MUST generate the secret server-side from a
  cryptographically secure source, persist only a memory-hard verifier plus
  non-secret lookup and lifecycle metadata, and return the plaintext exactly
  once in the response that created it.
- **REQ-009**: The credential record MUST carry id, owning principal, verifier,
  non-secret lookup prefix, creation time, optional expiry, optional revocation
  time, and last-use metadata. Listing MUST return this metadata and never the
  secret.
- **REQ-010**: Rotation MUST be expressible as overlap — issue B, verify B,
  revoke A — with no window in which the principal holds no usable credential
  and no reconstruction of its authorization.
- **REQ-011**: A credential's non-secret material MUST carry enough information
  to resolve its principal and, for tenant-scoped principals, its tenant scope,
  before secret verification.

### Authentication pipeline

- **REQ-012**: API-key exchange, OIDC authorization-code login, human refresh,
  workload `jwt-bearer`, and RFC 8693 delegation MUST all use one tenant-token
  issuance workflow after validating their grant-specific evidence. That
  workflow MUST load the current tenant and principal, reject inactive state,
  resolve current grants into one `PermissionSet`, and mint the same
  five-minute JWT shape. The JWT authority claim MUST be named `permissions`.
  Any retained `roles` claim is informational only and MUST NOT participate in
  request authorization. Refresh-token rotation and replay containment remain
  unchanged, but refresh MUST perform the same current-state checks and
  permission resolution as every other issuance path.
- **REQ-012a**: The per-request path MUST construct the authenticated context
  only from verified Wyrd token claims. Tenant-token verification MUST validate
  signature, issuer, audience, and expiry locally and MUST NOT read Postgres,
  resolve roles, introspect principal status, consult a revocation epoch, or use
  a positive verified-token cache. No served surface MAY authorize from raw
  credential material, a lookup prefix, or a provider token directly, and no
  request handler MAY branch on which entry path minted the token. Platform
  requests remain database-backed and MUST revalidate the current credential,
  principal, and grants without caching on every request.
- **REQ-012b**: Tenant access-token verification MUST be a concrete,
  synchronous, cryptographic operation that owns only local verification keys,
  issuer, audience, and clock-skew policy. External OIDC verification and its
  database-backed issuer or identity lookup remain on the issuance side. No
  common verifier trait, factory, checker, or database dependency may join
  those two responsibilities.
- **REQ-013**: The authenticated context MUST carry server-verified principal
  identity, principal type, and control-plane scope — platform, or exactly one
  tenant. It MUST be a closed two-variant type so a platform identity is not
  representable where a tenant identity is required, and the reverse.
- **REQ-014**: Tenant identity MUST derive only from the verified credential or
  verified federated identity, never from a request header, path, body, or
  hostname.
- **REQ-015**: All authenticated handlers MUST receive their principal through
  this context. No authorization logic keyed on API keys or secrets may remain.

### Authorization pipeline

- **REQ-016**: Every protected operation MUST authenticate, resolve the
  principal, authorize the action against the resource, and only then execute.
- **REQ-017**: Authorization MUST use the existing `Permission` vocabulary and
  synchronous checker. Tenant requests MUST check the `permissions`
  `PermissionSet` carried by the verified token; platform requests MUST check
  the current grants loaded for that request. The vocabulary MUST gain the
  administrative operations this change requires — at minimum tenant creation,
  listing, inspection, suspension, and administrative recovery on the platform
  plane, and tenant configuration, OIDC configuration, principal management, and
  credential management on the tenant plane.
- **REQ-018**: Platform and tenant control planes MUST be distinct authorization
  boundaries. A tenant-scoped principal MUST NOT invoke a platform operation, and
  a global administrative principal MUST NOT implicitly gain authority over
  tenant resources. Any deliberate global access to tenant resources MUST be an
  explicitly named, separately authorized, and audited capability.
- **REQ-019**: Administrative capability MUST follow from the principal's grants.
  Principal types MAY map to fixed internal capability sets in this delivery, but
  no capability may be implied by type in a way that bypasses the checker.

### Deployment initialization

- **REQ-020**: The deployment's global administrative principal MUST be
  established by an explicit operator-invoked initialization operation exposed
  as a `wyrd-server` subcommand, authorized by possession of the deployment's
  database credentials. Server start MUST NOT create administrative state and
  MUST NOT emit credential material.
- **REQ-021**: Initialization MUST, in one transaction: create the global
  administrative principal, generate its initial credential, persist only the
  verifier, grant its administrative authorization, mark the installation
  initialized, and return the plaintext for a single print to the invoking
  operator's terminal. The plaintext MUST NOT be logged or traced.
- **REQ-022**: Initialization MUST be idempotent and safe under concurrent and
  repeated invocation. An already-initialized deployment MUST refuse rather than
  create a second global administrative principal or initial credential, and
  MUST NOT re-expose an existing credential.
- **REQ-023**: A failed initialization MUST leave the deployment uninitialized
  and retryable, never a global administrative principal with no usable
  credential or a credential whose plaintext was never exposed.
- **REQ-024**: An uninitialized server MUST start and serve normally while
  refusing platform-control-plane operations with a stable error. Local
  development MUST remain a single command with no manual secret capture.

### Tenant provisioning

- **REQ-025**: Tenant creation MUST be one authorized platform-control-plane
  operation that authenticates the credential, resolves the global
  administrative principal, verifies tenant-creation permission, creates the
  tenant in a provisioning state, creates the tenant administrative principal,
  grants it tenant administration, generates its initial credential, initializes
  required tenant state including builtin role seeding, transitions the tenant to
  ready, and returns the tenant plus that credential's plaintext once.
- **REQ-026**: `platform.tenants.status` MUST distinguish provisioning, ready,
  failed, and suspended. A tenant MUST NOT be reachable by any authenticated
  tenant operation, background sweeper, or directory consumer servicing live
  tenants until it is ready. A tenant whose provisioning failed MUST be
  observable as failed and MUST NOT be usable.
- **REQ-027**: Provisioning MUST be transactional where one transaction
  suffices and otherwise resumable to the same outcome. Retried or concurrent
  creation for the same requested identity MUST converge on one tenant with one
  tenant administrative principal, with no duplicates and no orphaned
  credentials. Retry behavior MUST be explicit in the contract and tested.
- **REQ-028**: A global administrative principal MUST be able to list tenants,
  inspect one tenant's state, and suspend and resume a tenant. A suspended
  tenant MUST refuse authentication and authorization for its principals without
  destroying state, principals, or grants.

### Tenant administration

- **REQ-029**: The tenant administrative principal MUST be able to complete its
  tenant's configuration with no platform-plane involvement and no human
  identity: tenant configuration, OIDC configuration, principal creation and
  role grants, and credential issuance, listing, rotation, and revocation.
- **REQ-030**: A tenant administrator MUST be able to create tenant-scoped
  machine principals and grant them a narrower set of existing roles, so tenant
  automation never requires sharing the tenant administrative credential.
- **REQ-031**: Every tenant-scoped operation MUST verify that the target
  resource belongs to the authenticated principal's tenant. Tenant data access
  MUST continue to run under `TenantConn` RLS with no hand-written tenant
  filters and no widened queries.

### Recovery

- **REQ-032**: A global administrative principal MUST be able to issue a new
  credential for an existing tenant administrative principal whose credentials
  are all lost, expired, or revoked. This MUST NOT create a second tenant
  administrative principal, alter role grants, or grant the global principal any
  further tenant access. It MUST be explicitly named, separately authorized, and
  distinguishable in audit from ordinary tenant administration.
- **REQ-033**: Global-administrator credential loss MUST be recoverable only
  through documented deployment-level operator access to the database and secret
  store, using the same initialization-class authorization as `REQ-020`, never
  through an application-level principal.

### Human identity

- **REQ-034**: A verified federated human identity MUST resolve through its
  durable `(issuer, subject)` identity to a human principal in the bound tenant,
  and MUST mint a token producing the same authenticated context as a machine
  credential. Federated identity MUST be unique per tenant on
  `(tenant, issuer, subject)`, preserving the existing
  `wyrd.auth_user_identities` key, so one human may hold independent principals
  in multiple tenants. OIDC login is a human entry path only; it never
  authenticates a machine principal and never appears on the per-request path.
- **REQ-035**: Human principals MUST receive tenant roles independently of
  authentication, and a human principal holding tenant administration MUST NOT
  displace or require removal of the tenant administrative principal. The tenant
  retains a headless administrative path independent of its identity provider.

### Platform human administration

- **REQ-041**: Platform authority MUST be a grant held by a principal, not a
  property of a principal type. Both the bootstrap global administrative
  principal and human platform principals MUST be able to hold it. Platform
  grants MUST be stored at platform scope, outside any tenant's RLS-bound role
  tables.
- **REQ-042**: Human platform principals MUST live at platform scope with no
  tenant. A human who is also a user of a tenant holds a separate, independent
  tenant-scoped principal; the two are never merged into one identity.
- **REQ-043**: A deployment MAY have one platform-scope OIDC connection,
  configured, replaced, and removed by a principal holding platform authority.
  It is deployment-owned and distinct from every tenant-owned connection. Its
  absence, misconfiguration, or provider outage MUST NOT prevent platform
  administration through the global administrative credential.
- **REQ-044**: A human platform principal MUST be pre-registered before first
  login, against an expected issuer and a matching claim. The first successful
  login MUST pin `(issuer, subject)` durably, and later logins MUST match on
  that pinned identity alone. An unknown subject at the platform plane MUST be
  denied; just-in-time provisioning of platform principals is prohibited.
- **REQ-045**: The platform login entry MUST resolve only the platform
  connection, and a tenant login entry only that tenant's connection. The entry
  point selects the connection, the connection selects the principal, and the
  principal carries the scope. No scope is inferred from a token, header,
  hostname, or post-login chooser, and a platform session MUST carry platform
  scope only.
- **REQ-046**: Only a principal already holding platform authority may create a
  platform principal or grant platform authority. No tenant-plane operation —
  tenant administration, principal management, role grant, or OIDC group
  mapping — may create or elevate a platform principal.

- **REQ-047**: Every Wyrd-owned surface that calls a Wyrd HTTP route MUST do so
  through `crates/shared/wyrd-client`. No CLI command, SDK, or server-side tool
  constructs its own HTTP client, assembles its own authentication header, or
  maps its own status codes onto errors. A surface that cannot be expressed
  through the shared client is evidence the shared client is missing a
  capability, not licence to hand-roll one.
- **REQ-048**: Machine authentication and human-session continuation MUST use
  different renewal models. API-key and workload-identity grants MUST return a
  five-minute access token and no refresh token. `wyrd-client` MUST cache that
  access token, re-exchange the original durable credential before expiry, and
  after one authentication refusal re-exchange and retry the refused request at
  most once. Refresh tokens are reserved for human OIDC sessions: they MUST be
  stored as one-way verifiers, rotate on every use, revoke the active family on
  reuse, and name the consumed refresh credential in audit and successor access
  context. The eventual UI owns automatic human-session renewal and MUST keep
  refresh material in an HTTP-only server/BFF session rather than browser
  JavaScript storage; implementing that UI is not part of this change.
- **REQ-049**: `utoipa` is the single mandatory OpenAPI owner. The server MUST
  generate the canonical document from typed DTOs and route annotations and
  expose it at `/openapi.json` for Swagger-compatible tooling. The document
  MUST describe every served public route, authentication scheme, typed request
  and response body, problem media type, and reachable stable `WyrdError` code.
  Wyrd MUST NOT maintain a checked-in OpenAPI snapshot, a second YAML endpoint,
  an OpenAPI file generator or drift lane, a release-manifest OpenAPI digest, a
  hand-written route catalog, or an independent error list.

### Surfaces, audit, documentation, and replacement

- **REQ-036**: Platform and tenant administrative operations MUST be available
  headlessly over the language-agnostic HTTP contract with typed bodies, stable
  `WyrdError` codes, and generated OpenAPI. CLI, SDKs, MCP, and documentation
  project that contract; no surface introduces a second identity model or
  durable authority. The generated contract MUST declare the authentication
  scheme its administrative paths require, so an independent client can
  implement them from the served document alone.
- **REQ-037**: Every authorization decision made by these operations MUST append
  its audit row in the same transaction as the decision, for allowed and denied
  alike, naming the principal, the credential that authenticated the request,
  the permission, the resource, the tenant where applicable, and the outcome. A
  decision whose audit cannot be recorded MUST fail closed. Audit MUST be able to
  state which principal, using which credential, performed which operation,
  against which tenant, at what time.
- **REQ-038**: `wyrd-server bootstrap-key`, its fabricated
  `system/bootstrap-admin@1.0.0` `CardRef`, `SYSTEM_OPERATOR_ID`, and the
  `cli:bootstrap-key` task MUST be removed and replaced by `REQ-020` and
  `REQ-025`. Their transactional shape, single-print plaintext handling, and
  reuse of the production issuance and audit seams MUST be preserved in the
  replacement.
- **REQ-039**: The unreachable `platform.users`, `platform.roles`,
  `platform.user_roles`, and `platform.api_keys` objects and their query slots
  MUST become the platform principal, credential, and grant store required by
  `REQ-041`, or be removed where they do not fit it. Leaving a second
  unreachable cross-tenant identity model in the schema is not an acceptable
  outcome.
- **REQ-040**: Documentation MUST cover the three-command operator journey, the
  SaaS model in which Wyrd operates the global principal and the customer never
  receives it, credential rotation, and credential-loss recovery.

## Invariants and prohibited outcomes

- **INV-001**: A credential is never an identity. Authorization is never
  attached to, derived from, or cached against a credential record.
- **INV-002**: Raw credential material is never persisted, recoverable,
  re-displayable, logged, traced, placed in an error or audit payload, or
  written to a Card or generated artifact.
- **INV-003**: A credential authenticates only its own principal and establishes
  only its own tenant scope. No credential, header, hostname, or path selects a
  different principal or tenant.
- **INV-004**: The platform and tenant control planes never silently collapse.
  Global administrators manage tenant lifecycle; tenant administrators manage
  tenant resources.
- **INV-004a**: Platform authority never confers tenant data access. A platform
  principal may manage tenant lifecycle and recover tenant administration;
  reaching a tenant's resources requires the explicitly named, separately
  authorized, audited capability of `REQ-018`.
- **INV-004b**: Platform authority is reachable only from the platform plane.
  No tenant, tenant administrator, tenant OIDC connection, or tenant group
  mapping can create a platform principal or confer platform authority, so a
  tenant's compromise never escalates to platform control.
- **INV-005**: A deployment has at most one initialization. Restart, replica
  count, crash recovery, and concurrent invocation never yield a second global
  administrative principal, a second initial credential, or a re-exposure.
- **INV-006**: A tenant never becomes usable without its tenant administrative
  principal, its grant, and its initial credential. A failed or partial
  provisioning never presents as usable.
- **INV-007**: One tenant's principals, credentials, roles, audit, and
  configuration are never readable, mutable, or authenticable from another
  tenant.
- **INV-008**: Losing every credential of a principal never destroys the
  principal, its tenant, its roles, or its data.
- **INV-009**: Authorization belongs durably to the principal, never the
  credential. Tenant token issuance resolves the principal's current grants
  into one effective authority snapshot; credential rotation does not copy or
  mutate grants.
- **INV-010**: Every privileged operation is attributable to a real principal,
  never to a synthetic identity or to "an API key".
- **INV-011**: Every outcome is fail-closed: unknown, ambiguous, unverifiable,
  suspended, expired, revoked, or unauditable conditions deny, and denial never
  leaks another tenant's existence, names, inventory, or configuration.
- **INV-012**: Invalid-credential conditions remain publicly indistinguishable
  and resistant to timing and enumeration inference.
- **INV-013**: Revoking a tenant credential, suspending or deleting a tenant
  principal, or changing its grants MUST prevent new tokens immediately.
  Tenant tokens already issued remain valid for no more than their five-minute
  lifetime; their authority is immutable for that lifetime. Platform
  revocation and grant changes MUST take effect on the next request because the
  platform plane re-reads current credential, principal, and grant state without
  caching. Tenant authentication has no authorization epoch, revocation list,
  introspection read, or verified-token cache.
- **INV-014**: Administrative identity remains server-owned durable state. No
  Card kind, SDK, CLI, or UI becomes a durable source of truth.
- **INV-015**: Every Wyrd plane authenticates on `X-Wyrd-Access-Token`. The
  caller's own `Authorization` header belongs to the calling application and is
  never read by any Wyrd surface. Plane separation is carried by verified token
  claims and by the extractor type a route declares — never by which header
  carried the token, which any client can set.

## Externally observable behavior and failure modes

| Situation | Required result |
|---|---|
| Server starts on an uninitialized deployment | Serves normally; platform routes refuse with a stable error; nothing is printed or logged. |
| Operator runs initialization | Exactly one global administrative principal and one credential; plaintext printed once to that terminal, never to the server log. |
| Initialization is re-run, or run concurrently | Refused; no second principal, no second credential, no re-exposure. |
| Initialization fails partway | Deployment remains uninitialized and retryable. |
| Global credential creates a tenant | One tenant, one tenant administrative principal with its grant, seeded builtin roles, one returned credential; the tenant is immediately usable. |
| Tenant creation fails partway | No usable tenant; the outcome is observable as failed; retry converges on one correct tenant. |
| Tenant creation retried or concurrent for the same identity | Exactly one tenant and one administrative principal; no duplicates or orphans. |
| Tenant credential calls a platform operation | Denied; the tenant directory is unchanged and unenumerated. |
| Global credential calls an ordinary tenant operation | Denied; global privilege does not imply tenant access. |
| Tenant A targets tenant B's principals, credentials, roles, or data | Denied; nothing about tenant B is revealed. |
| Tenant administrator rotates (issue B, verify B, revoke A) | Administration is uninterrupted; A cannot mint another token, and an access token already minted through A remains valid only until its five-minute expiry. |
| Every tenant administrative credential is lost | The global administrator issues a replacement for the **same** principal; roles unchanged; no second principal. |
| Global administrative credential is lost | Recoverable only by an operator with database and secret-store access. |
| Tenant or principal is suspended | New tenant tokens are refused immediately; existing tenant tokens expire within five minutes; platform requests refuse on their next current-state check; state and grants survive. |
| Credential expired or revoked | The single indistinguishable invalid-credential error. |
| A required audit row cannot be written | The operation refuses and commits nothing. |
| A credential secret is requested after creation | No surface returns it; only metadata is available. |
| A federated human authenticates | Resolves through `(issuer, subject)` to a human principal producing the same authenticated context as a machine credential. |

## Required system boundaries and cross-boundary flow

```text
Operator (self-hosted) or SaaS control plane
  ├── wyrd-server init            (authorized by database-credential possession)
  └── holds the global administrative credential
        │  HTTP / CLI, platform control plane
        ▼
wyrd-server
  ├── authenticate -> resolve principal -> authorize -> audit -> execute
  ├── tenant directory writes on the operator (BYPASSRLS) boundary
  └── tenant-scoped writes on the RLS tenant boundary
        │
        ▼
Tenant administrative principal
  └── HTTP / CLI / SDK / MCP, tenant control plane
        └── configuration, OIDC, principals, role grants, credentials
```

- Durable principal, credential, tenant, grant, status, and audit state is
  server-owned Rust behavior. HTTP is the language-agnostic contract.
- Platform-plane persistence uses `OperatorPool`; tenant-scoped persistence uses
  `TenantConn` RLS. No third connection abstraction is introduced, and the
  identity tier's platform/tenant split corresponds to that existing split.
- Secret material crosses the boundary exactly once, outward, in the response or
  terminal output that created it.

## Expensive-to-reverse decisions

- Identity and credentials are separate durable concepts; authorization binds to
  the principal.
- The principal type set distinguishes global administration, tenant
  administration, humans, and machines, and tenancy is absent for global
  principals. This amends the doctrine-locked `PrincipalKind`.
- Card binding becomes a property of a machine principal, not a precondition for
  holding a credential.
- The authenticated context is one closed two-variant type — platform scope or
  tenant scope — produced by one authentication pipeline, making `INV-004` a
  type-level guarantee that mirrors the `OperatorPool` / `TenantConn` split.
- Tenant access tokens are standard five-minute self-contained JWTs. Issuance
  resolves current identity and grants once; requests verify signature, issuer,
  audience, and expiry locally and authorize from the `permissions` claim. A
  Bifrost request or stream must hold a valid token when admitted; admitted
  bounded work may finish under its existing deadline after token expiry, while
  any later request or stream establishment is refused. The lower-volume
  privileged platform plane remains database-backed on
  every request. No hybrid JWT cache, authorization epoch, revocation checker,
  or per-request tenant-auth introspection is permitted.
- Initialization is an operator-invoked subcommand authorized by database-
  credential possession, not a server-start side effect. Server boot never emits
  credential material, so the deployment root credential never enters the log
  pipeline.
- `platform.tenants.status` gains provisioning and failure states.
- Credential ownership becomes principal-generic in the durable schema.
- Administrative surfaces are transactionally audited, superseding the current
  no-audit stance on the admin routes.
- Machine credentials are re-exchanged; refresh-token families are reserved for
  human OIDC sessions and the eventual server-owned UI session boundary.
- OpenAPI generated through `utoipa` is a mandatory public contract served by
  the server at `/openapi.json`. It has no checked-in snapshot, duplicate YAML
  endpoint, file-generation lane, or speculative release digest. Typed wire
  DTOs, stable problem responses, first-class clients, MCP, CLI, JSON Schemas,
  required Python `.pyi` declarations, and required TypeScript `.d.ts`
  declarations remain separate required surfaces.
- `bootstrap-key`, its fabricated Card, and `SYSTEM_OPERATOR_ID` are deleted
  rather than retained alongside the new model.

## Required architecture amendments

This specification cannot be satisfied without amending current authority
documents. These amendments are part of the change.

- `architecture/wyrd-design.md` — the closed `PrincipalKind` set, the required
  `tenant_id`, and the statement that non-human principals are Card-bound.
- `architecture/wyrd-security-posture.md` — the principal and credential
  lifecycle section, which currently states the same closed set and Card
  binding, plus any tenant epoch, next-request revocation, permission-cache, or
  per-request introspection description superseded by revision 11.
- `architecture/v1/00-foundations/` — the Auth foundation and permission
  vocabulary pages affected by the new administrative permissions and the
  tenant JWT / platform current-state split.
- Every active architecture or operator document that describes tenant
  `tokens_not_before`, authorization epochs, revocation checking, verified-token
  caching, or request-time role resolution MUST be deleted or rewritten to the
  revision-11 flow; historical completed records remain historical evidence and
  are not rewritten.
- `components/admin/routes.rs` module documentation — the deliberate no-audit
  stance.
- `changes/active/tenant-oidc-federation/spec.md` — its assumption that every
  OIDC connection is tenant-owned, which `REQ-043` widens with one
  deployment-owned platform-scope connection.

## Acceptance obligations

- **AC-001**: A real-server journey proves initialization yields exactly one
  global administrative principal and one usable credential, that re-running and
  concurrent invocation refuse, that failure leaves the deployment retryable,
  and that no credential material appears in server logs or traces.
- **AC-002**: A real-server journey drives the three-command operator path
  through the shipped SDK and CLI — init, create tenant, configure the tenant and
  create a restricted machine principal with the returned credential — with no
  database access and no human identity at any step.
- **AC-003**: Cross-plane negative evidence proves a tenant credential cannot
  invoke any platform operation and a global credential cannot invoke ordinary
  tenant operations, with stable errors and no enumeration.
- **AC-004**: Cross-tenant negative evidence proves tenant A cannot read,
  mutate, authenticate against, or recover tenant B's principals, credentials,
  roles, configuration, or data.
- **AC-005**: Credential-lifecycle evidence proves multiple concurrent
  credentials per principal, overlapping rotation with uninterrupted
  administration, independent revocation, five-minute access-token expiry,
  metadata-only listing, immediate refusal of new exchange through a revoked
  credential, bounded validity of its already-issued token, and that the
  plaintext is unobtainable after creation.
- **AC-006**: Recovery evidence proves a tenant administrative principal with
  zero usable credentials is restored against the same principal id, with
  unchanged grants and no second administrative principal.
- **AC-007**: Provisioning-failure and retry evidence proves an injected failure
  at each stage produces no usable tenant, and that retry and concurrent
  creation converge on exactly one tenant and one administrative principal.
- **AC-008**: Suspension evidence proves a suspended tenant or principal cannot
  receive a new token, a previously issued tenant token remains bounded by its
  five-minute expiry, the platform plane observes suspension on its next
  request, and resuming restores issuance with state and grants intact.
- **AC-009**: Audit evidence proves each covered decision appends its row in the
  decision's transaction for allowed and denied outcomes, naming principal,
  credential, permission, resource, tenant, and outcome; that an unrecordable
  audit refuses the operation; and that no secret reaches an audit payload.
- **AC-010**: Security evidence proves the single indistinguishable
  invalid-credential error, verifier-only persistence, absence of plaintext in
  every durable and diagnostic surface, local tenant JWT verification with no
  authorization epoch, revocation checker, positive verifier cache, per-request
  admission read, or request-time permission resolution, current-state platform
  revalidation, and RLS as the tenant boundary with no hand-written tenant
  filters added.
- **AC-011**: Human-identity evidence proves a verified federated identity and a
  machine credential produce the same authenticated context, and that a human
  tenant administrator coexists with the tenant administrative principal without
  displacing it.
- **AC-012**: Service-principal evidence proves a tenant administrator can
  create a restricted machine principal whose credential performs its granted
  operations and is denied tenant administration. Bifrost evidence resolves a
  requested table to its stable identity and authorizes it against the exact,
  schema-wide, or global Bifrost permission carried in that principal's token;
  no authentication-store read occurs on the Bifrost request path.
- **AC-013**: Contract and generated-artifact evidence proves HTTP, OpenAPI,
  CLI, SDK, MCP, JSON Schemas, required Python `.pyi` declarations, required
  TypeScript `.d.ts` declarations, stable errors, and documentation describe
  one administrative identity model, that `bootstrap-key` and its fabricated
  identities are gone, and that no unreachable second identity model remains
  in the schema.
- **AC-015**: Bootstrap-chain evidence proves the global administrative
  credential configures the platform OIDC connection and pre-registers the first
  human platform administrator; that the first login pins `(issuer, subject)`
  and succeeds; that an unknown subject is denied; and that the credential still
  administers the platform when the connection is absent or its provider is
  failing.
- **AC-016**: Scope-separation evidence proves a platform session carries
  platform scope only, that the same human authenticating through a tenant
  connection receives an independent tenant-scoped principal and session, and
  that neither session reaches the other's plane.
- **AC-017**: Escalation evidence proves no tenant-plane operation — including
  tenant principal creation, role grant, and OIDC group mapping — can create a
  platform principal or confer platform authority.
- **AC-014**: Contract evidence proves the administrative HTTP surface is
  implementable by an independent client: the generated OpenAPI document and
  the served `/openapi.json` endpoint declare every administrative path, its
  typed bodies, reachable stable error codes, problem media, and authentication
  scheme. The CLI and MCP exercise those operations
  against a real server, and they are the administrative
  surfaces this change builds and proves: administration is an operator and
  agent act, and a Python or TypeScript binding for it has no user the CLI does
  not already serve. `wyrd-client` retains the shared implementation because
  `REQ-047` requires the CLI to call through it; the Rust SDK's existing
  re-export of that crate is unaffected and carries no separate administrative
  surface of its own.
- **AC-018**: Issuance evidence proves API-key exchange, OIDC login, human
  refresh, workload `jwt-bearer`, and RFC 8693 delegation use one workflow and
  mint the same five-minute tenant access-token shape. API-key and workload
  grants return no refresh token; `wyrd-client` re-exchanges before expiry and
  once after an authentication refusal; a human OIDC session rotates its
  refresh token, successfully uses the successor access token after the
  original expires, and durably revokes the successor family when the consumed
  token is replayed.
- **AC-019**: OpenAPI evidence proves `utoipa` remains the single generator,
  `/openapi.json` serves its runtime document, route/auth/body/problem/error
  coverage is exact without a parallel catalog, and no checked-in snapshot,
  YAML endpoint, file-generation lane, or release OpenAPI digest remains.

## Material constraints

- The server remains the sole durable authority for identity, credentials,
  tenancy, and authorization.
- Full tenant separation applies across principals, credentials, roles,
  configuration, caches, logs, audit, and generated artifacts.
- Existing Wyrd permission, API-key, token, audit, stable-error, RLS, and
  `OperatorPool` contracts remain authoritative except where this specification
  amends them explicitly. The tenant revocation-epoch and request-time
  permission-resolution contracts are superseded and MUST be removed.
- Security or availability uncertainty fails closed; no fallback crosses a
  tenant or control-plane boundary.
- No compatibility route, alias, legacy name, or migration shim is introduced;
  none of these contracts has shipped to users.
- RBAC is not overbuilt: administrative permissions may remain internal in this
  delivery rather than being exposed as customizable roles.
- Verification is deliberately narrow; see **Verification scope**. Concurrent
  unrelated work is expected to break tests outside these surfaces, and that
  breakage is not this change's responsibility.

## Verification scope

This change deliberately narrows the verification scope that `AGENTS.md` §11
and §12 would otherwise require. The narrowing is an author decision recorded
here, not drift, and this section is the authority for planning, implementation,
and review.

- **VER-001**: Verification proves only principal, credential, authenticated
  context, authorization-plane, initialization, tenant-provisioning,
  administrative-surface, and client-consolidation behavior introduced or
  changed by this specification. Client consolidation (`REQ-047`) covers every
  `wyrd-cli` command that calls a Wyrd route, including the authentication and
  admin commands that predate this change: they are in scope because this
  specification makes the canonical header normative and a command on the wrong
  header cannot authenticate at all.
  Every acceptance obligation is satisfied by focused tests, subsystem
  integration tests, and the user journeys named in this specification.
- **VER-002**: Every named Rust test runs through its exact focused expression,
  `mise exec -- cargo nextest run --locked -p <crate> <target> -E 'test(=...)'`,
  with the repository-managed setup wrapper where Postgres is required.
- **VER-003**: Broad test aggregates MUST NOT be run or required as evidence:
  `mise run gate`, `test:rust`, whole-crate and family lanes, the storage
  matrix, and ad hoc `--all-features` test lanes. The canonical `mise run lints`
  workspace Clippy lane remains required even though it uses `--all-features`;
  a reviewer MUST NOT treat the other aggregates' absence as missing
  verification.
- **VER-004**: Compilation and type checking are limited to the crates this
  change touches and their direct dependents. Workspace-wide compilation is not
  an acceptance obligation for this change.
- **VER-005**: Test failures outside the surfaces in `VER-001` are out of scope.
  They are not this change's obligation to diagnose, fix, skip, or report as
  regressions, and they do not block its acceptance. This does not license
  weakening, disabling, or deleting any test to produce a passing result.
- **VER-006**: Contract regeneration remains in scope for the error catalog,
  JSON Schemas, required Python `.pyi` declarations, and required TypeScript
  `.d.ts` declarations this change alters. Runtime OpenAPI correctness is
  verified by the owning server contract tests rather than a generated-file
  drift check.

## Delivery sequence

The specification is delivered in this order; each stage is usable evidence for
the next.

1. **Principal model** — principals and credentials as separate durable
   concepts, the type set, non-Card-bound machine principals, credential
   generation, verification, lookup, revocation, and rotation.
2. **Authentication context** — one pipeline, one two-variant authenticated
   context, removal of credential-keyed authorization.
3. **Global initialization** — the `wyrd-server` init subcommand, idempotent and
   concurrency-safe.
4. **Global authorization** — platform-plane permissions and the plane
   boundary.
5. **Tenant provisioning** — `POST /tenants`, lifecycle states, atomicity and
   retry, `bootstrap-key` removal.
6. **Tenant authorization** — tenant-plane permissions and tenant-boundary
   enforcement.
7. **Credential lifecycle** — create, list metadata, revoke, rotate.
8. **Administrative recovery** — global issuance against an existing tenant
   administrative principal.
9. **Human identity** — federated `(issuer, subject)` resolution into the same
   authenticated context.
10. **Service principals** — tenant-created machine principals with restricted
    grants.
11. **Platform human administration** — the platform-scope OIDC connection,
    pre-registered human platform principals, first-login identity pinning, and
    the platform login entry. Stage 4 carries the platform grant store this
    depends on.
12. **Surface consolidation** — one authentication pipeline on one header, one
    signing-key resolution, every Wyrd-owned caller moved onto `wyrd-client`,
    machine renewal by durable-credential re-exchange, human refresh rotation,
    and one accurate `utoipa` OpenAPI document served at `/openapi.json`. This
    stage removes duplicate renewal and OpenAPI artifact machinery and keeps one
    runtime HTTP contract.

## Open material decisions

None. Every decision raised during drafting has been resolved by the author.

## Revision history

- **Revision 12 — 2026-09-21 — approved**: Names the tenant JWT authority claim
  `permissions`; requires API-key exchange, OIDC login, human refresh, workload
  `jwt-bearer`, and RFC 8693 delegation to share one current-state issuance
  workflow; fixes the verifier as one concrete crypto-only boundary; and states
  that a Bifrost request or stream is authenticated at admission and may finish
  only under its existing bounded deadline. It also distinguishes the required
  all-features lint lane from prohibited broad test aggregates.
- **Revision 11 — 2026-09-21 — approved**: Replaces the rejected hybrid tenant
  authentication flow with standard five-minute self-contained JWT access
  tokens. Credential and OIDC exchange resolve current principal, tenant, and
  effective permissions once; each tenant request verifies signature, issuer,
  audience, and expiry locally and authorizes from the token's effective
  `PermissionSet`. Revocation, suspension, deletion, and grant changes prevent
  new tenant tokens immediately and affect existing tokens no later than
  expiry. The privileged lower-volume platform plane remains current-state and
  database-backed on every request. Tenant authorization epochs,
  `tokens_not_before`, revocation checks, positive verifier caching,
  per-request admission SQL, and request-time permission resolution are deleted,
  together with architecture prose that prescribes them.
- **Revision 10 — 2026-09-19 — approved**: Narrows revision 9 to the one
  OpenAPI surface users need. `utoipa`, its route/schema annotations, the
  runtime `WyrdApiDoc`, `/openapi.json`, contract tests, and user documentation
  remain required. The checked-in `openapi.yaml`, `/openapi.yaml`, its emitter,
  codegen/drift wiring, YAML-only dependency features, snapshot-parsing docs
  machinery, and the unimplemented release OpenAPI digest are deleted. Python
  `.pyi` and TypeScript `.d.ts` declarations remain required SDK surfaces.
- **Revision 9 — 2026-09-19 — approved**: Corrects revision 8's OpenAPI
  deletion. `utoipa`, checked-in generated OpenAPI, drift verification, docs,
  and runtime `/openapi.json` and `/openapi.yaml` endpoints are hard
  requirements for Swagger-compatible organizational tooling. The revision-8
  machine-versus-human renewal split remains approved and unchanged.
- **Revision 8 — 2026-09-19 — approved**: Owner-approved simplification.
  Machine API-key and workload grants no longer issue refresh tokens;
  `wyrd-client` re-exchanges their durable credential before expiry and once
  after an authentication refusal. Refresh rotation is reserved for human OIDC
  sessions and the eventual HTTP-only UI/BFF session boundary. OpenAPI is
  deleted as an unused duplicate contract: artifact, runtime endpoints,
  generator, `utoipa` machinery, docs, release references, tasks, and checks.
  Typed HTTP bodies, stable errors, first-class SDKs, CLI, MCP, JSON Schemas,
  required Python `.pyi` declarations, required TypeScript `.d.ts`
  declarations, behavioral tests, and generated docs remain authoritative.
- **Revision 7 — 2026-09-19 — approved**: Consolidation revision; adds no new
  product capability. `AC-014` drops the Python and TypeScript administrative
  bindings and is re-grounded on the generated contract plus the Rust SDK.
  `REQ-036` now requires the generated contract to declare its authentication
  scheme. `REQ-047` requires every Wyrd-owned caller to use `wyrd-client`.
  The CLI and MCP are the administrative client surfaces this change builds and
  proves; no Python or TypeScript administrative binding is added, and the unrun
  Rust SDK administrative journeys are deleted.
  `INV-015` makes `X-Wyrd-Access-Token` the one authentication header on every
  plane and states that plane separation is carried by token claims and
  extractor type, not by header choice. `INV-013` is restated so each plane's
  revocation mechanism follows from its own caching, recording that the platform
  plane satisfies it by caching nothing rather than by an epoch. `VER-001`
  widens to the consolidation surface, and stage 12 is added to the delivery
  sequence.
- **Revision 6 — 2026-09-18 — approved**: Added the **Verification scope** section
  narrowing proof to focused tests, subsystem integration tests, and the named
  user journeys for principal, credential, authenticated-context,
  authorization-plane, initialization, provisioning, and administrative
  surfaces. Broad aggregates and workspace-wide compilation are excluded, and
  failures outside these surfaces are out of scope.
- **Revision 5 — 2026-09-18 — draft**: Platform authority becomes a grant rather
  than a principal-type property, holdable by the bootstrap global principal and
  by human platform principals living at platform scope. Adds the optional
  deployment-owned platform-scope OIDC connection, pre-registration with
  first-login `(issuer, subject)` pinning, the platform login entry, and the
  prohibition on any tenant-plane path creating or elevating a platform
  principal. Repurposes the dormant `platform.*` tables as the platform
  principal, credential, and grant store.
- **Revision 4 — 2026-09-18 — draft**: Separated the two authentication entry
  paths (machine credential exchange, human OIDC login) from the per-request
  path, which derives the authenticated context only from verified Wyrd token
  claims. Pinned federated identity uniqueness to `(tenant, issuer, subject)`.
  Recorded that the platform control plane is credential-only.
- **Revision 3 — 2026-09-18 — draft**: Rewritten to the author's model.
  Principal types cover global administration, tenant administration, humans,
  and machines with tenancy absent for global principals; Card binding becomes a
  property rather than a precondition; human identity and service principals are
  in scope. Initialization is an operator-invoked `wyrd-server` subcommand rather
  than a server-start side effect, so the deployment root credential never
  reaches the log pipeline. `bootstrap-key`, its fabricated
  `system/bootstrap-admin@1.0.0` Card, and `SYSTEM_OPERATOR_ID` are deleted.
  Required architecture amendments and the ten-stage delivery sequence are
  recorded. No open decisions remain.
- **Revision 2 — 2026-09-18 — draft**: Resolved the authenticated-context shape
  to a closed two-variant type.
- **Revision 1 — 2026-09-18 — draft**: Initial capture against the repository
  baseline.

## Material authority and evidence links

- [`AGENTS.md`](../../../AGENTS.md) — client/server ownership, tenant
  separation, audit foundations, journey-test requirements, minimalism ladder.
- [`architecture/agent-rules.md`](../../../architecture/agent-rules.md) —
  `TenantConn`/`OperatorPool` boundary, transactional authorization audit,
  fail-closed rules.
- [`architecture/wyrd-design.md`](../../../architecture/wyrd-design.md) —
  runtime identity and Auth/Policy planes; amended by this change.
- [`architecture/wyrd-doctrine.mdx`](../../../architecture/wyrd-doctrine.mdx) —
  registrable Card kinds and public-surface doctrine.
- [`architecture/wyrd-security-posture.md`](../../../architecture/wyrd-security-posture.md)
  — principal and credential lifecycle; amended by this change.
- [`architecture/v1/04-surfaces/deployment.md`](../../../architecture/v1/04-surfaces/deployment.md)
  — self-hosted, cloud SaaS, and enterprise topologies.
- [`changes/active/tenant-oidc-federation/spec.md`](../tenant-oidc-federation/spec.md)
  — owner of OIDC configuration and login; consumer of this change's
  authenticated context.
- [`changes/active/object-scoped-rbac/spec.md`](../object-scoped-rbac/spec.md)
  — approved permission/object model this change authorizes against.
