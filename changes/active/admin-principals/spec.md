---
id: SPEC-admin-principals
revision: 2
status: draft
---

# Global and tenant administrative principals

## Objective and user value

Wyrd has no way to create a tenant. `platform.tenants` rows exist only because a
migration seeds the system sentinel or a test fixture inserts one directly. There
is no deployment root of trust, no tenant provisioning operation, no
administrative identity that can configure a tenant, and no recovery path when a
credential is lost. Every Wyrd deployment — self-hosted, cloud SaaS, and
enterprise single-tenant — is therefore unusable without out-of-band SQL.

This change establishes the administrative identity model that makes a Wyrd
deployment operable:

```text
Global administrative principal        (platform control plane)
  ├── creates, inspects, suspends tenants
  └── recovers tenant administration
        │
        ▼
  Tenant administrative principal      (tenant control plane)
        ├── configures the tenant, including its OIDC federation
        ├── manages tenant principals and their roles
        └── issues, rotates, and revokes credentials
```

The user value is a deployment an operator can stand up and drive entirely
headlessly: install, initialize, `wyrd tenant create`, configure. A human
identity provider is a later convenience, never a provisioning prerequisite.

The load-bearing separation is **identity is durable; credentials are not**.
Credentials authenticate a principal. Roles authorize a principal. Rotating,
losing, or revoking a credential changes neither the principal nor its
authorization.

## Current baseline

Established repository behavior this change builds on or must correct:

- `Principal { id, kind, tenant_id, roles, effective_permissions }` is the only
  runtime identity (`crates/shared/wyrd-runtime/src/principal.rs`). `tenant_id`
  is **required**; there is no tenant-free principal.
- `PrincipalKind` is a doctrine-locked closed set: `User`,
  `Service { card_ref, card_ref_scope }`, `Agent { card_ref, card_ref_scope }`.
  Service and Agent are **card-bound** — provisioned by `wyrd apply` keyed on
  `(tenant_id, principal_kind, card_kind, card_uid)`.
- `wyrd.auth_api_keys` binds a credential to `sa_id` — a row in
  `wyrd.auth_service_accounts` — so only card-bound principals can hold a
  credential today. `wyrd.auth_refresh_tokens` is already principal-generic
  (`principal_kind`, `principal_id`) and is the precedent for generalizing.
- Argon2 key hashing, tenant-prefixed key material, expiry, revocation,
  `last_used_at`, one indistinguishable invalid-key error, and
  `wyrd.audit_credential_issuance` already exist.
- Roles are tenant-scoped rows in `wyrd.auth_roles`; `BUILTIN_ROLES` seeds
  `admin`, `writer`, `reader`, `agent`, `runtime_admin` per tenant.
  `seed_builtin_roles_for_tenant` is idempotent.
- `platform.tenants.status` is the closed set `active | suspended | deleted`.
  There is no provisioning state.
- `platform.users`, `platform.roles`, `platform.user_roles`, and
  `platform.api_keys` exist in the schema with query slots but no production
  caller. They are an email/password-shaped cross-tenant identity model that
  predates the principal model and is unreachable from any served surface.
- Two and only two connection abstractions exist: `&mut TenantConn<'_>` (RLS,
  `wyrd_app`) and `&OperatorPool` (BYPASSRLS, `wyrd_platform_admin`). The
  database tier already encodes the tenant/platform split this change needs at
  the identity tier.
- `crates/wyrd/wyrd-server/src/components/admin/routes.rs` states that it
  deliberately writes no audit row. That contradicts the current agent rule that
  every authorization decision is transactionally audited, and this change makes
  the administrative surfaces conform.
- Tenant-owned human OIDC federation is specified separately and is **not
  redefined here**; see `changes/active/tenant-oidc-federation/spec.md`.
- Typed object scope on `Permission` is approved separately; see
  `changes/active/object-scoped-rbac/spec.md`. This change adds no second grant
  model.

Existing implementation is evidence, not authority, and may require correction
to satisfy this specification.

## Scope

- A platform (global) administrative principal and its credentials, established
  once per deployment.
- Tenant provisioning as one authorized, atomic, resumable operation that yields
  a usable tenant.
- A tenant administrative principal that is the headless root of trust for its
  tenant and does not require a human identity or a Card.
- A principal-generic credential model supporting multiple concurrent
  credentials, independent revocation, and overlap-based rotation.
- One authentication pipeline that resolves any machine credential to a
  server-owned authenticated context, and one authorization boundary between the
  platform and tenant control planes.
- Global-administrator recovery of tenant administration.
- Tenant-administrator management of tenant principals, role grants, and
  credentials, including restricted-permission machine principals.
- HTTP, CLI, SDK, and MCP projections of the above, plus audit and
  documentation.

## Non-goals

- Defining, redesigning, or implementing human OIDC federation, the login flow,
  claim mapping, or group-to-role mapping. Owned by
  `SPEC-tenant-oidc-federation`. This change owns only the obligation that a
  federated human resolves into the same authenticated context (`REQ-016`).
- A new RBAC model, customizable role editor, permission-scope redesign,
  explicit deny, grant options, or ownership semantics. Existing roles and the
  approved object-scoped `Permission` remain the only static grant model.
- A `Principal`, `Credential`, `Tenant`, or administrative Card kind. Doctrine's
  16 registrable kinds are unchanged; administrative identity is server state,
  not a registered AI-system component.
- Global-administrator credential recovery. Losing every global credential is
  deployment-level recovery through operator access to the deployment's database
  and secret store, documented but not exposed as an application-level
  self-service path (`DEC-004`).
- Billing, plans, quotas, organizations as a second durable noun, custom domain
  provisioning, tenant deletion/data-destruction workflows, or tenant migration.
- Changing Card-bound Service/Agent principal provisioning, `wyrd apply`
  semantics, delegation chains, or the emit card scope.
- A SaaS customer-facing signup UI. The SaaS control plane is an ordinary holder
  of a global administrative credential.

## Definitions

- **Principal** — a durable server-owned identity that can hold role grants and
  be named in audit. It survives credential rotation, revocation, and loss.
- **Credential** — a secret that authenticates exactly one principal. Verified
  by a stored one-way verifier; the plaintext is never persisted.
- **Platform control plane** — operations over the tenant directory itself:
  tenant lifecycle and tenant-administration recovery.
- **Tenant control plane** — operations over one tenant's resources, including
  its configuration, principals, roles, and credentials.
- **Global administrative principal** — a principal in the platform control
  plane. It belongs to no tenant.
- **Tenant administrative principal** — a non-human, non-Card-bound principal
  created during tenant provisioning, holding that tenant's administrative
  role.
- **Initialization** — the one-time establishment of a deployment's global
  administrative principal and its first credential.

## Required behavior

### Principals and credentials

- **REQ-001**: A principal MUST exist independently of any credential. Creating,
  issuing, rotating, revoking, expiring, or losing credentials MUST NOT create,
  destroy, or alter a principal or its role grants.
- **REQ-002**: One principal MUST be able to hold multiple simultaneously valid
  credentials. Revoking or expiring one credential MUST NOT affect another
  credential of the same principal, the principal, or its authorization.
- **REQ-003**: Wyrd MUST support a non-human principal that is **not** bound to
  a Card. Tenant administrative principals and tenant-created machine principals
  are of this shape. Card-bound Service and Agent principals keep their existing
  provisioning contract and are not replaced.
- **REQ-004**: Credentials MUST be principal-generic: the durable credential
  record identifies its owning principal, and credential issuance, listing,
  revocation, and authentication MUST work uniformly for every principal that is
  permitted to hold one.
- **REQ-005**: Credential creation MUST generate the secret server-side from a
  cryptographically secure source, persist only a memory-hard verifier plus
  non-secret lookup and lifecycle metadata, and return the plaintext exactly
  once in the response that created it. No later read, list, log, trace, error,
  audit payload, generated artifact, Card, or UI surface MAY return it.
- **REQ-006**: Credential records MUST carry owning principal, non-secret lookup
  metadata, creation time, expiry, revocation state and time, and last-use
  metadata. Listing credentials MUST return this metadata and never the secret.
- **REQ-007**: Rotation MUST be expressible as overlap: issue a new credential,
  verify it, revoke the old one, with no window in which the principal holds no
  usable credential and no reconstruction of its authorization.

### Authentication and authorization

- **REQ-008**: Every machine credential MUST authenticate through one pipeline:
  extract → resolve credential record → verify secret → resolve principal →
  check principal and tenant status → construct the authenticated context. No
  served surface MAY authorize from credential material, key prefix, or
  credential record directly.
- **REQ-009**: The authenticated context MUST carry server-verified principal
  identity, principal kind, and control-plane scope — platform, or exactly one
  tenant. Tenant identity MUST derive only from the verified credential record,
  never from a request header, path, body, or hostname.
- **REQ-010**: Platform and tenant control planes MUST be distinct authorization
  boundaries. A tenant-scoped principal MUST NOT be able to invoke a
  platform-control-plane operation, and a global administrative principal MUST
  NOT implicitly gain authority over a tenant's resources. Any deliberate
  global-administrator access to tenant resources MUST be an explicit, named,
  separately authorized, and audited capability.
- **REQ-011**: Authorization MUST use the existing role-derived `Permission`
  vocabulary and the existing synchronous checker. Platform and tenant
  administrative capabilities are expressed as permissions on principals; no
  capability MAY be implied by principal kind alone.

### Deployment initialization

- **REQ-012**: On a deployment with no initialized administrative state, Wyrd
  MUST establish exactly one global administrative principal and exactly one
  initial credential for it, expose that credential's plaintext exactly once
  through the operator channel defined by `DEC-003`, and mark the deployment
  initialized — all transactionally.
- **REQ-013**: Initialization MUST be idempotent and safe under concurrent and
  repeated server starts, including multiple replicas starting simultaneously
  and a restart after a partial failure. An already-initialized deployment MUST
  NOT create an additional global administrative principal or credential, MUST
  NOT re-expose an existing credential, and MUST start normally.
- **REQ-014**: A failed initialization MUST leave the deployment uninitialized
  and retryable. It MUST NOT leave a global administrative principal with no
  usable credential, or a credential whose plaintext was never exposed, in a
  state the operator cannot recover from without database access.

### Tenant provisioning

- **REQ-015**: Tenant creation MUST be one authorized platform-control-plane
  operation that produces a usable tenant: the tenant directory row, its tenant
  administrative principal, that principal's administrative role grant, its
  initial credential, and the tenant's required initial state — including
  builtin role seeding — before the tenant is reported usable.
- **REQ-016**: `platform.tenants.status` MUST distinguish a tenant that is being
  provisioned from one that is usable. A tenant MUST NOT be reachable by any
  authenticated tenant operation, background sweeper, or directory consumer that
  services live tenants until provisioning has completed. A tenant whose
  provisioning failed MUST be observable as failed and MUST NOT be usable.
- **REQ-017**: Provisioning MUST be atomic where a single transaction suffices
  and otherwise resumable to the same outcome. A retried or concurrent creation
  for the same requested tenant identity MUST converge on one tenant with one
  tenant administrative principal, and MUST NOT produce duplicate tenants,
  duplicate administrative principals, or orphaned credentials. Retry behavior
  MUST be explicit in the contract and proven by test.
- **REQ-018**: The creation response MUST return the tenant's server-assigned
  identity, its status, its tenant administrative principal's identity, and that
  principal's initial credential plaintext exactly once.
- **REQ-019**: A global administrative principal MUST be able to list tenants,
  inspect one tenant's lifecycle state, and suspend and resume a tenant. A
  suspended tenant MUST refuse authentication and authorization for its
  principals without destroying tenant state, principals, or role grants.

### Tenant administration

- **REQ-020**: The tenant administrative principal MUST be able to complete its
  tenant's configuration with no further platform-control-plane involvement and
  no human identity: tenant configuration including its OIDC federation
  settings, tenant principal creation and role grants, and credential issuance,
  listing, rotation, and revocation.
- **REQ-021**: A tenant administrator MUST be able to create additional
  tenant-scoped machine principals and grant them a narrower set of existing
  roles, so tenant automation never requires sharing the tenant administrative
  credential.
- **REQ-022**: Every tenant-scoped operation MUST verify that the target
  resource belongs to the authenticated principal's tenant. Tenant data access
  MUST continue to run under `TenantConn` RLS; no handler MAY substitute a
  hand-written tenant filter or widen a query to reach another tenant.
- **REQ-023**: A human principal that later gains tenant administrative
  authorization MUST NOT displace or require removal of the tenant
  administrative principal. Both are independent identities that may hold the
  same role, preserving a headless administrative path that does not depend on
  the tenant's identity provider.

### Recovery

- **REQ-024**: A global administrative principal MUST be able to issue a new
  credential for an existing tenant administrative principal whose credentials
  are all lost, expired, or revoked, restoring programmatic tenant
  administration. This operation MUST NOT create a second tenant administrative
  principal, alter role grants, or grant the global principal any further tenant
  access.
- **REQ-025**: Recovery MUST be an explicitly named, separately authorized, and
  audited capability distinguishable in audit from ordinary tenant
  administration.

### Surfaces, audit, and documentation

- **REQ-026**: Platform and tenant administrative operations MUST be available
  headlessly over the language-agnostic HTTP contract with typed request and
  response bodies, stable `WyrdError` codes, and generated artifacts. The CLI
  and SDKs project that contract; no surface introduces a second identity model
  or durable authority. A UI, if present, is a projection only.
- **REQ-027**: Every authorization decision made by these operations —
  initialization, tenant creation, inspection, suspension, recovery, principal
  creation, role grant and revocation, and credential issuance, rotation, and
  revocation — MUST append its audit row in the same transaction as the
  decision, naming the principal, the permission, the resource, the tenant where
  applicable, and the outcome, for allowed and denied alike. A decision whose
  audit cannot be recorded MUST fail closed.
- **REQ-028**: Audit MUST attribute a privileged operation to a principal, and
  MUST additionally record which credential authenticated the request, so an
  operator can answer "which principal, using which credential, performed which
  operation, against which tenant, when".
- **REQ-029**: Documentation MUST cover the self-hosted operator journey
  (install → initialize → capture the global credential → create tenant →
  capture the tenant credential → configure), the cloud SaaS operator model in
  which Wyrd operates the global principal and the customer never receives it,
  credential rotation, and credential-loss recovery.
- **REQ-030**: The unreachable `platform.users`, `platform.roles`,
  `platform.user_roles`, and `platform.api_keys` schema objects and their query
  slots MUST be resolved by this change under `DEC-002` — either becoming the
  platform principal and credential store or being removed. Leaving a second,
  unreachable cross-tenant identity model in the schema is not an acceptable
  outcome.

## Invariants and prohibited outcomes

- **INV-001**: A credential is never an identity. Authorization is never
  attached to, derived from, or cached against a credential record; it is
  resolved from the principal the credential authenticates.
- **INV-002**: Raw credential material is never persisted, recoverable,
  re-displayable, logged, traced, included in an error or audit payload, or
  placed in a Card or generated artifact.
- **INV-003**: A credential can authenticate only its own principal, and a
  tenant-scoped credential can establish only its own tenant. No credential,
  request, header, hostname, or path can select a different principal or tenant.
- **INV-004**: The platform and tenant control planes never silently collapse. A
  global administrative principal reaching tenant resources is impossible except
  through a named capability that is separately authorized and audited as such.
- **INV-005**: A deployment has at most one initialization. Restart, replica
  count, crash recovery, and concurrent boot never yield a second global
  administrative principal, a second initial credential, or a re-exposure of an
  existing secret.
- **INV-006**: A tenant never becomes usable without its tenant administrative
  principal, that principal's role grant, and its initial credential. A failed
  or partial provisioning never presents as a usable tenant.
- **INV-007**: One tenant's principals, credentials, roles, audit, and
  configuration are never readable, mutable, or authenticable from another
  tenant, including when both hold structurally similar credentials.
- **INV-008**: Losing every credential of a principal never destroys the
  principal, its tenant, its roles, or its data.
- **INV-009**: Every authorization outcome in this change is fail-closed:
  unknown, ambiguous, unverifiable, suspended, expired, revoked, or
  unauditable conditions deny, and denial never leaks another tenant's
  existence, names, principal inventory, or configuration.
- **INV-010**: Invalid-credential conditions remain publicly indistinguishable
  from one another, preserving the existing single-error contract, and remain
  resistant to timing and enumeration inference.
- **INV-011**: Revoking a credential, principal, or role grant advances the
  applicable existing authorization epoch transactionally; no token or cached
  permission set outlives the earlier of its expiry or that epoch.
- **INV-012**: Administrative identity remains server-owned durable state. No
  Card kind, client SDK, CLI, or UI becomes a durable source of truth for
  principals, credentials, tenants, or grants.

## Externally observable behavior and failure modes

| Situation | Required result |
|---|---|
| Fresh deployment starts | Exactly one global administrative principal and one credential; the plaintext is exposed once through the operator channel with explicit "cannot be retrieved again" guidance. |
| Initialized deployment restarts, or N replicas boot together | No new principal, no new credential, no re-exposure; the server starts normally. |
| Initialization fails partway | The deployment remains uninitialized and retryable; no half-initialized root of trust. |
| Global credential creates a tenant | One tenant, one tenant administrative principal with its administrative role, seeded builtin roles, and one returned initial credential; the tenant is usable immediately. |
| Tenant creation fails partway | No usable tenant; the outcome is observable as failed; retry converges on a single correct tenant. |
| Tenant creation retried or issued concurrently for the same identity | Exactly one tenant and one tenant administrative principal; no duplicates or orphans. |
| Tenant credential calls a platform operation | Denied with a stable error; the tenant directory is unchanged and unenumerated. |
| Global credential calls an ordinary tenant operation | Denied; global privilege does not imply tenant access. |
| Tenant A credential targets tenant B's principals, credentials, roles, or data | Denied; nothing about tenant B is revealed. |
| Tenant administrator rotates its credential (issue B, verify B, revoke A) | Administration is uninterrupted; A stops working; authorization is unchanged. |
| Every tenant administrative credential is lost | The global administrator issues a replacement for the **same** principal; administration resumes with unchanged roles. |
| A principal or its tenant is suspended | Its credentials stop authenticating and existing tokens stop being honored at the authorization epoch; state and grants survive. |
| A credential is expired or revoked | Authentication fails with the single indistinguishable invalid-credential error. |
| A required audit row cannot be written | The operation refuses and commits nothing. |
| A credential secret is requested after creation | No surface returns it; only metadata is available. |

## Required system boundaries and cross-boundary flow

```text
Operator (self-hosted) or SaaS control plane
  └── holds a global administrative credential
        │  HTTP / CLI, platform control plane
        ▼
wyrd-server
  ├── authenticate → resolve principal → authorize → audit → execute
  ├── tenant directory writes run on the operator (BYPASSRLS) boundary
  └── tenant-scoped writes run on the RLS tenant boundary
        │
        ▼
Tenant administrative principal
  └── HTTP / CLI / SDK / MCP, tenant control plane
        └── tenant configuration, principals, role grants, credentials
```

- Durable principal, credential, tenant, grant, status, and audit state is
  server-owned Rust behavior. HTTP is the language-agnostic contract; CLI, SDKs,
  MCP, and UI project it.
- Platform-control-plane persistence uses the existing `OperatorPool`
  cross-tenant boundary. Tenant-scoped persistence uses `TenantConn` RLS. This
  change does not introduce a third connection abstraction.
- The identity tier's platform/tenant split MUST correspond to that existing
  database split rather than contradicting it.
- Secret material crosses the boundary exactly once, outward, in the response or
  operator channel that created it.

## Expensive-to-reverse decisions fixed by this specification

- Identity and credentials are separate durable concepts; authorization binds to
  the principal (`REQ-001`, `INV-001`).
- Administrative capability is authorization, not principal kind (`REQ-011`).
  `GLOBAL_ADMIN` and `TENANT_ADMIN` are **not** introduced as principal kinds;
  doing so would re-collapse authorization into identity — the same error this
  change exists to prevent, one level up.
- Wyrd gains a non-Card-bound machine principal shape (`REQ-003`). This is the
  minimum structural addition required by both the tenant administrative
  principal and tenant-created automation identities, neither of which is a
  registered AI-system component.
- The platform control plane is a distinct authorization boundary, not a
  privileged tenant (`REQ-010`, `INV-004`).
- `platform.tenants.status` gains provisioning and failure states; this is a
  persisted-contract change (`REQ-016`).
- Credential ownership becomes principal-generic in the durable schema
  (`REQ-004`), following the existing principal-generic refresh-token precedent.
- Administrative surfaces are transactionally audited (`REQ-027`), superseding
  the current deliberate no-audit stance on the admin routes.
- The authenticated context is one closed two-variant type — platform scope or
  tenant scope — produced by one authentication pipeline (`DEC-001`, approved
  2026-09-18). `Principal` keeps its required `tenant_id` and `PrincipalKind`
  stays doctrine-locked; the platform variant carries its own identity type.
  This makes `INV-004` a type-level guarantee rather than a convention and
  mirrors the existing `OperatorPool` / `TenantConn` split at the identity
  tier.

## Acceptance obligations

- **AC-001**: A real-server journey proves first-boot initialization yields
  exactly one global administrative principal and one usable credential, and
  that restart and concurrent boot yield no second principal, no second
  credential, and no re-exposure.
- **AC-002**: A real-server journey drives the complete self-hosted operator
  path through the shipped SDK and CLI: initialize → create tenant → use the
  returned tenant administrative credential to configure the tenant and create a
  restricted machine principal, with no database access and no human identity at
  any step.
- **AC-003**: Cross-plane negative evidence proves a tenant credential cannot
  invoke any platform operation and a global credential cannot invoke ordinary
  tenant operations, with stable errors and no directory or tenant enumeration.
- **AC-004**: Cross-tenant negative evidence proves tenant A cannot read,
  mutate, authenticate against, or recover tenant B's principals, credentials,
  roles, configuration, or data.
- **AC-005**: Credential-lifecycle evidence proves multiple concurrent
  credentials per principal, overlapping rotation with uninterrupted
  administration, independent revocation, expiry, metadata-only listing, and
  that the plaintext is unobtainable after creation.
- **AC-006**: Recovery evidence proves that a tenant administrative principal
  with zero usable credentials is restored by the global administrator against
  the same principal id, with unchanged role grants and no second administrative
  principal.
- **AC-007**: Provisioning-failure and retry evidence proves that an injected
  failure at each provisioning stage produces no usable tenant, and that retry
  and concurrent creation converge on exactly one tenant with exactly one
  administrative principal.
- **AC-008**: Suspension evidence proves a suspended tenant's principals stop
  authenticating and their live tokens stop being honored at the authorization
  epoch, and that resuming restores access with state and grants intact.
- **AC-009**: Audit evidence proves each covered decision appends its row in the
  decision's transaction for allowed and denied outcomes, names principal,
  credential, permission, resource, tenant, and outcome, and that an
  unrecordable audit refuses the operation. Redaction evidence proves no secret
  reaches an audit payload.
- **AC-010**: Security evidence proves the single indistinguishable
  invalid-credential error, verifier-only persistence, absence of plaintext in
  every durable and diagnostic surface, epoch coupling on revocation, and RLS as
  the tenant boundary with no hand-written tenant filters added.
- **AC-011**: Contract and generated-artifact evidence proves HTTP, CLI, SDK,
  MCP, schemas, stable errors, and documentation describe one administrative
  identity model, and that no unreachable second identity model remains in the
  schema (`REQ-030`).
- **AC-012**: Cross-language evidence proves the administrative HTTP contract is
  usable from the Rust, Python, and TypeScript SDKs for the operations each is
  intended to expose.
- **AC-013**: Seam evidence proves a federated human principal and a machine
  credential produce the same authenticated context shape, so downstream
  authorization contains no authentication-mechanism branch. Implementing OIDC
  itself remains out of scope.

## Material constraints

- The server remains the sole durable authority for identity, credentials,
  tenancy, and authorization.
- Full tenant separation applies across principals, credentials, roles,
  configuration, caches, logs, audit, and generated artifacts.
- Existing Wyrd principal, permission, API-key, token, revocation-epoch, audit,
  stable-error, RLS, and `OperatorPool` contracts remain authoritative unless a
  later approved revision changes one explicitly.
- Security or availability uncertainty fails closed; no fallback ever crosses a
  tenant or control-plane boundary.
- No compatibility route, alias, legacy name, or migration shim is introduced;
  none of these contracts has shipped to users.
- The change must not scaffold a general RBAC or identity framework beyond what
  these requirements need.

## Open material decisions

Each must be resolved by explicit human approval before this specification is
authoritative.

- **DEC-001 — Shape of the platform/tenant split in the authenticated context.**
  **Resolved 2026-09-18**: one authentication pipeline produces a closed
  two-variant authenticated context — platform scope or tenant scope. A
  platform identity is not representable as a `Principal` and cannot be handed
  to a tenant handler. Recorded above as a fixed decision; `REQ-008` through
  `REQ-010` and `INV-004` are read under it.

- **DEC-002 — Fate of the unreachable `platform.*` identity tables.** Options:
  (a) delete `platform.users`, `platform.roles`, `platform.user_roles`, and
  `platform.api_keys` with their query slots, and introduce
  principal/credential tables shaped by this specification; (b) repurpose them
  in place. The existing shape is email/password-oriented with no
  principal/credential separation. **Recommendation: (a).**

- **DEC-003 — Operator channel for the once-exposed initial global credential.**
  Options: (a) generate and print to the server's operator output only, which is
  poor UX under multiple replicas and log aggregation; (b) accept an
  operator-supplied initial secret from the deployment secret provider as the
  production path, with generate-and-print retained for self-hosted and
  development; (c) a dedicated one-shot initialization command separate from
  server start. The choice affects the deployment contract for cloud SaaS and
  Kubernetes. **Recommendation: (b) plus (c), with (a) as the single-node
  self-hosted default.**

- **DEC-004 — Global-administrator recovery.** Confirm that losing every global
  credential is handled by documented operator access to the deployment database
  and secret store rather than by any application-level self-service path, and
  confirm whether a supported operator command exists for it.

- **DEC-005 — Delivery decomposition.** This specification is large. Confirm
  whether it is approved as one change (planned into sequenced tasks) or split —
  the natural seam being principals, credentials, authenticated context, and
  initialization first; then tenant provisioning, tenant administration, and
  recovery. Splitting changes nothing in this specification's content, only how
  `$wyrd-plan` decomposes it.

## Revision history

- **Revision 2 — 2026-09-18 — draft**: Resolved `DEC-001` to the closed
  two-variant authenticated context and recorded it as a fixed
  expensive-to-reverse decision. `DEC-002` through `DEC-005` remain open.
- **Revision 1 — 2026-09-18 — draft**: Captured the global and tenant
  administrative principal model, credential lifecycle, deployment
  initialization, tenant provisioning, tenant administration, recovery, audit,
  and surface obligations against the current repository baseline. Scoped OIDC
  federation out to `SPEC-tenant-oidc-federation`. Five material decisions
  remain open. Not approved for planning or implementation.

## Material authority and evidence links

- [`AGENTS.md`](../../../AGENTS.md) — client/server ownership, tenant
  separation, audit foundations, journey-test requirements, Ponytail minimalism.
- [`architecture/agent-rules.md`](../../../architecture/agent-rules.md) —
  `TenantConn`/`OperatorPool` boundary, transactional authorization audit,
  fail-closed rules.
- [`architecture/wyrd-design.md`](../../../architecture/wyrd-design.md) —
  runtime identity, `PrincipalKind`, Auth vs Policy planes, API-key and token
  exchange contract.
- [`architecture/wyrd-doctrine.mdx`](../../../architecture/wyrd-doctrine.mdx) —
  registrable Card kinds and public-surface doctrine.
- [`architecture/wyrd-security-posture.md`](../../../architecture/wyrd-security-posture.md)
  — principal and credential lifecycle, API-key rules, token and revocation
  epochs, federation constraints.
- [`architecture/v1/00-foundations/security.md`](../../../architecture/v1/00-foundations/security.md)
  — Auth and Policy foundation map.
- [`architecture/v1/04-surfaces/deployment.md`](../../../architecture/v1/04-surfaces/deployment.md)
  — self-hosted, cloud SaaS, and enterprise topologies.
- [`changes/active/tenant-oidc-federation/spec.md`](../tenant-oidc-federation/spec.md)
  — owner of human OIDC federation; consumer of this change's authenticated
  context.
- [`changes/active/object-scoped-rbac/spec.md`](../object-scoped-rbac/spec.md)
  — approved permission/object model this change authorizes against.
