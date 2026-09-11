---
id: SPEC-tenant-oidc-federation
revision: 2
status: draft
---

# Tenant-owned OIDC federation

## Human intent and user value

Cloud SaaS customers must be able to use their own OIDC identity provider for
human single sign-on. Wyrd must not require every hosted tenant to use an
identity provider chosen by the Wyrd operator. One hosted tenant may use Okta,
another Entra ID, and another Keycloak while all use the same hosted Wyrd
service.

The same product model applies to self-hosted Wyrd. The difference is
operational ownership: the Wyrd SaaS operator runs hosted Wyrd, while a
self-hosted customer runs its deployment. In both cases the customer operates
its identity-provider organization and registers Wyrd as an OIDC application.

Users should enter through their tenant's Wyrd login URL, choose a generic
**Sign in with SSO** action, authenticate at that tenant's provider, and return
to Wyrd with tenant-bound roles. OIDC discovery and the provider's published
JWKS establish verification material; ordinary OIDC setup does not require a
manual certificate exchange.

## Current baseline

Wyrd already has tenant-scoped trusted-issuer storage, tenant-admin HTTP CRUD,
self-hosted TOML seeding, OIDC discovery and JWKS verification, PKCE login,
group-to-role mapping, and tenant-subdomain login resolution. The current UI
foundation deliberately implements local-development authentication only and
records production identity-provider integration as follow-up work.

This change defines the missing hosted customer workflow and aligns the UI,
headless administration, deployment guidance, and product documentation around
the existing tenant-owned federation model. Existing implementation is
evidence, not authority, and may require correction to satisfy this spec.

## Scope

- Human OIDC federation owned independently by each Wyrd tenant.
- Cloud SaaS and self-hosted configuration responsibilities.
- Tenant-scoped connection lifecycle, including discovery, credentials, claim
  mapping, group-to-role mapping, replacement, and removal.
- A generic tenant login experience backed only by that tenant's configured
  human OIDC connections.
- Headless administration and an optional tenant-settings projection, subject
  to `DEC-001`.
- Security, audit, failure, and documentation behavior for the configuration
  and login workflows.
- Production qualification of the complete hosted and self-hosted workflows
  against real OIDC providers and production-shaped deployments.

## Non-goals

- Requiring one Wyrd-selected identity provider for every SaaS tenant.
- SAML, manual signing-certificate exchange, LDAP, SCIM, or password
  authentication.
- Vendor-specific Okta, Entra ID, or Keycloak authentication implementations;
  supported providers interoperate through OIDC.
- Changing machine workload federation or workload bindings.
- Making the browser, CLI, or SDK the durable owner of identity-provider
  configuration, tenant identity, roles, or sessions.
- Defining custom-domain provisioning, billing, or tenant creation.
- Exercising qualification with customer production tenants, identities, or
  secrets; controlled qualification accounts and secret stores are required.

## Definitions

- **Tenant**: Wyrd's durable security and data-isolation boundary. Customer
  language may call this an organization, but this change does not introduce a
  second durable `Organization` noun.
- **Customer identity provider**: an OIDC provider operated or selected by the
  customer, such as its Okta organization, Entra ID tenant, or Keycloak realm.
- **Human OIDC connection**: one tenant-owned trusted-issuer configuration used
  for browser login and distinguished from a workload issuer by the existing
  human issuer policy.
- **Tenant login URL**: the public Wyrd URL that selects a candidate tenant
  before authentication. It is routing context, not trusted tenant identity.
- **Tenant administrator**: a human principal authorized by Wyrd to manage
  identity configuration for exactly one tenant.
- **Wyrd operator**: the party operating a Wyrd deployment: Wyrd for cloud
  SaaS, or the customer for self-hosted Wyrd.

## Required behavior

- **REQ-001**: Cloud SaaS MUST permit each tenant to configure and use its own
  human OIDC connection without changing or constraining another tenant's
  connection.
- **REQ-002**: Self-hosted Wyrd MUST permit the deployment operator to configure
  the customer's human OIDC connection without depending on a SaaS-operated
  identity provider or control plane.
- **REQ-003**: Every human OIDC connection MUST belong to exactly one Wyrd
  tenant. The same issuer URL configured by multiple tenants MUST produce
  independent client, audience, claim, role, credential, and lifecycle state.
- **REQ-004**: An authorized administrator MUST be able to create, inspect,
  replace or rotate, and remove its tenant's human OIDC connection through a
  headless Wyrd surface. Returned representations MUST redact secrets.
- **REQ-005**: Connection configuration MUST support the issuer URL, OIDC client
  ID, expected audience, supported client authentication, optional client
  secret, claim mapping, default roles, and group-to-role mappings required by
  Wyrd's existing human federation contract.
- **REQ-006**: Wyrd MUST provide the administrator with the exact callback URL
  to register at the customer identity provider. The customer registers Wyrd
  as an OIDC application and supplies the resulting connection values; Wyrd
  MUST NOT require manual vendor-certificate exchange for ordinary OIDC.
- **REQ-007**: Creating or replacing a connection MUST perform bounded OIDC
  discovery and obtain the provider's JWKS location without treating discovery
  alone as authorization to trust the issuer.
- **REQ-008**: A tenant's unauthenticated login entry MUST present generic Wyrd
  SSO language and MUST initiate login only through a human OIDC connection
  configured for that tenant. It MUST NOT expose another tenant's providers or
  configuration.
- **REQ-009**: Human login MUST use the authorization-code flow with PKCE,
  nonce validation, bounded single-use state, exact callback binding, ID-token
  signature and claim verification, and fail-closed issuer, audience,
  algorithm, key, and tenant checks.
- **REQ-010**: After successful callback verification, Wyrd MUST create or
  resolve the human principal within the connection's tenant, map verified
  provider groups and default roles to Wyrd roles, and issue only tenant-bound
  Wyrd credentials.
- **REQ-011**: Changes to connection trust, credentials, claim mapping, default
  roles, group-to-role mapping, or removal MUST affect only that tenant and MUST
  follow Wyrd's existing authorization-epoch and short-lived-token security
  rules where existing sessions or permissions are affected.
- **REQ-012**: Every security-significant connection mutation and human login
  outcome MUST emit the canonical redacted audit evidence at its authoritative
  transition. Configuration and login failures MUST fail closed if their
  required audit write cannot be accepted.
- **REQ-013**: Hosted and self-hosted documentation MUST state who operates
  Wyrd, who operates the customer identity provider, where the connection is
  configured, which redirect and credential values are exchanged, how groups
  map to Wyrd roles, and that signing keys are normally discovered through
  JWKS rather than manually exchanged certificates.
- **REQ-014**: Cloud SaaS configuration MUST be possible without a deployment
  restart. Self-hosted boot configuration MAY remain available alongside the
  same runtime management contract.
- **REQ-015**: Wyrd MUST NOT claim cloud SaaS or self-hosted human OIDC support
  from unit, mocked-provider, or in-process integration tests alone. Before the
  capability is release-ready, the complete workflow MUST pass in deployed,
  production-shaped environments using real external OIDC endpoints.
- **REQ-016**: Initial provider qualification MUST exercise controlled Okta,
  Entra ID, and Keycloak connections. Any additional provider named as
  supported by Wyrd MUST enter the same qualification matrix before that claim
  is published.
- **REQ-017**: Production qualification MUST exercise both hosted multi-tenant
  and self-hosted topologies over their real public TLS origin, callback
  routing, Postgres tenant boundary, secret protection, audit path, and Wyrd UI
  or headless surface included in the delivery. Loopback-only execution is not
  production qualification.
- **REQ-018**: Qualification MUST exercise the full connection lifecycle and
  user journey: provision the OIDC application, configure the tenant
  connection, discover metadata, initiate generic SSO, authenticate, process
  the callback, map groups to roles, use the resulting Wyrd session, change
  mappings, rotate applicable client and JWKS key material, revoke or remove
  the connection, and prove subsequent access fails closed as specified.
- **REQ-019**: Qualification evidence MUST be tied to the immutable Wyrd
  artifact, deployment and redacted configuration fingerprints, provider and
  topology matrix entry, execution time, and outcome. Skipped assertions,
  unresolved failures, secret-bearing artifacts, or evidence that cannot be
  tied to the tested artifact and configuration MUST fail qualification.
- **REQ-020**: A material change to federation contracts, verification,
  connection storage, tenant resolution, callback routing, secret handling,
  role mapping, session issuance, or deployment topology MUST re-run every
  affected production-qualification matrix entry before release.

## Invariants and prohibited outcomes

- **INV-001**: A tenant, principal, browser, callback, or identity-provider
  token can never select, read, mutate, or authenticate through another
  tenant's OIDC connection.
- **INV-002**: The tenant login URL selects only pre-authentication routing
  context. The server binds login state to the resolved tenant and connection,
  and successful Wyrd credentials derive tenant identity only from verified,
  server-owned state.
- **INV-003**: OIDC discovery makes provider metadata available; it never makes
  an issuer trusted. Only an authorized tenant-scoped configuration transition
  establishes trust.
- **INV-004**: Provider secrets never appear in a Card, browser payload,
  list/read response, log, trace, error, audit payload, generated artifact, or
  plaintext durable column.
- **INV-005**: Tenant-supplied discovery, token, and JWKS endpoints remain
  subject to Wyrd's production SSRF, DNS-pinning, TLS-verification, timeout,
  redirect, and response-bound controls.
- **INV-006**: Human and workload issuer policies remain distinct. A workload
  issuer cannot service browser login, and a human issuer cannot silently
  become a workload binding.
- **INV-007**: Group and default-role mapping can grant only Wyrd roles valid
  for the same tenant. Unknown, ambiguous, or unauthorized mappings fail
  closed and cannot widen cross-tenant authority.
- **INV-008**: The generic login UI is a projection of server-owned connection
  state. It does not persist trust, accept raw secret material, verify tokens,
  or become the only way to configure federation.
- **INV-009**: Absence, removal, outage, or invalidity of a tenant's connection
  never falls back to a platform-wide provider or another tenant's provider.
- **INV-010**: Public failure behavior does not reveal another tenant's
  existence, issuer inventory, client identifiers, claim mappings, or role
  mappings.
- **INV-011**: Production qualification never weakens TLS verification, SSRF
  controls, tenant isolation, audit coupling, token validation, or secret
  handling to make a provider or deployment pass.
- **INV-012**: Test-only providers and local emulators remain useful for fast,
  deterministic coverage but never substitute for live provider and deployed
  topology evidence.

## Externally observable behavior and failure modes

| Situation | Required result |
|---|---|
| Hosted tenant A configures Okta while tenant B configures Entra ID | Each tenant sees and uses only its own connection; both coexist in one hosted service. |
| Self-hosted customer configures Okta | The customer registers Wyrd at Okta, configures its deployment, and receives the same human login semantics. |
| User opens a tenant login URL with one usable human connection | Wyrd offers a generic **Sign in with SSO** action and begins that tenant's OIDC flow. |
| Callback succeeds | Wyrd maps verified identity and groups within the bound tenant, records the outcome, and establishes a Wyrd session or credentials for that tenant. |
| Tenant or issuer is unknown, inactive, mismatched, or removed | Authentication is denied without cross-tenant discovery or fallback. |
| State is missing, expired, replayed, or bound to a different callback | Authentication is denied and no Wyrd credential is issued. |
| Provider metadata, token endpoint, or JWKS is unavailable or unsafe | The affected operation fails closed with a stable, non-secret error; another tenant remains unaffected. |
| ID token has an invalid signature, nonce, issuer, audience, algorithm, key, or claims | Authentication is denied, audited, and no Wyrd credential is issued. |
| Administrator lacks the required permission | Connection data is not returned or changed; the denial is tenant-safe and audited as required. |
| Connection includes a client secret | The secret is accepted only at the server boundary, protected at rest, and redacted from every later representation. |
| A provider or affected Wyrd release has not passed its required live qualification | Wyrd does not advertise that provider/topology combination as supported or promote the affected release as production-ready. |

## Required system boundaries and cross-boundary flow

```text
Customer IdP administrator
  -> registers the tenant-specific Wyrd callback and client
  -> gives issuer/client credentials to an authorized Wyrd administrator

Authorized Wyrd administrator
  -> tenant-scoped HTTP/CLI administration or tenant settings
  -> wyrd-server validates discovery, protects secrets, commits trust, audits

User browser
  -> tenant login URL -> generic Sign in with SSO
  -> wyrd-server creates tenant-and-connection-bound PKCE state
  -> customer IdP authenticates the user
  -> wyrd-server callback verifies provider token through discovered JWKS
  -> wyrd-server maps tenant roles and issues tenant-bound Wyrd credentials
```

- Durable connection state, trust decisions, secret protection, tenant
  binding, role mapping, principal resolution, credential issuance, and audit
  remain server-owned Rust behavior.
- HTTP is the language-agnostic management and login contract. CLI and UI
  project it without defining a separate identity model.
- Self-hosted TOML remains deployment-owned seeding ergonomics, not an
  alternate durable contract.
- The Wyrd UI uses its BFF boundary and same public origin; the browser does
  not receive provider secrets or direct durable-management authority.

## Acceptance obligations

- **AC-001**: A real-server cloud-SaaS journey demonstrates two tenants using
  different controlled OIDC providers concurrently, including successful login
  and tenant-specific group-to-role mapping for each. Deterministic local
  providers MAY supply continuous coverage, but do not satisfy `AC-010`.
- **AC-002**: Cross-tenant negative evidence demonstrates that either tenant
  cannot list, select, mutate, remove, callback through, or receive roles from
  the other's connection, including when both configure the same issuer URL.
- **AC-003**: A real-server human-login journey proves tenant resolution,
  generic login initiation, PKCE and state binding, provider callback, JWKS
  verification, role mapping, credential issuance, and canonical audit output.
- **AC-004**: Negative login evidence covers untrusted or mismatched issuer,
  unsafe or unavailable discovery/JWKS, unknown key after bounded refresh,
  invalid audience or nonce, expired or replayed state, removed connection, and
  unauthorized role mapping without issuing credentials.
- **AC-005**: Management-surface evidence proves authorized tenant-scoped
  create, redacted read, replacement or rotation, and removal plus stable,
  audited denial for an under-privileged principal.
- **AC-006**: UI journey and accessibility evidence proves the generic SSO
  entry, tenant-safe unavailable/error states, callback completion, keyboard
  operation, visible focus, and equivalent light/dark behavior without making
  the UI the only management surface.
- **AC-007**: Self-hosted configuration evidence and documentation prove that
  the customer operates both its deployment and identity-provider setup while
  preserving the same login and verification contract.
- **AC-008**: Security review evidence proves secret redaction and protection,
  tenant isolation, fail-closed discovery/JWKS behavior, SSRF controls,
  authorization-epoch handling, and transactionally coupled audit for durable
  connection changes.
- **AC-009**: Contract, generated-artifact, docs, and public-surface drift
  checks prove that HTTP, CLI, UI, schemas, stable errors, and documentation
  describe one tenant-owned OIDC model.
- **AC-010**: Retained live-provider evidence proves successful end-to-end
  login against controlled Okta, Entra ID, and Keycloak tenants or realms over
  externally trusted TLS, including discovery, callback, JWKS verification,
  session use, and provider-specific group claim mapping.
- **AC-011**: Retained cloud-SaaS evidence proves two differently configured
  tenant connections coexist in the same deployed Wyrd service and remain
  isolated through configuration, login, callback, role mapping, session use,
  mutation, and removal.
- **AC-012**: Retained self-hosted evidence proves a customer-operated
  deployment can configure its own connection and complete the same end-to-end
  journey without any hidden SaaS identity dependency.
- **AC-013**: Lifecycle qualification proves client-secret rotation where
  applicable, provider signing-key/JWKS rotation, group-to-role mapping change,
  connection disablement or removal, authorization-epoch effects, and
  fail-closed use of superseded credentials and sessions.
- **AC-014**: Production-shaped fault evidence covers provider, discovery,
  token, JWKS, Postgres, secret-provider, and audit-path unavailability;
  callback replay; invalid claims; unsafe endpoint resolution; and tenant
  mismatch without a cross-tenant fallback or partially established session.
- **AC-015**: A qualification manifest ties every matrix result and redacted
  diagnostic artifact to the tested Wyrd artifact, deployment/configuration
  fingerprints, provider, topology, public origin, and execution. Review can
  determine exactly which support claims passed, failed, or were not run.

## Material constraints

- The server remains the sole durable identity and authorization authority.
- Full tenant separation applies in cloud SaaS across connection state,
  credentials, principals, roles, sessions, caches, logs, audit, and generated
  artifacts.
- Existing Wyrd principal, permission, trusted-issuer, authorization-code,
  JWKS, audit, stable-error, and deployment contracts remain authoritative
  unless a later approved revision explicitly changes one.
- Security uncertainty fails closed; availability never permits cross-tenant
  fallback or unverified login.
- Production support claims require both deterministic repository tests and
  the live, credentialed qualification defined here. Live qualification runs
  separately from the default credential-free test lanes and uses only
  controlled test identities and deployment-managed secrets.
- No new Card kind, provider abstraction, identity-provider-specific route, or
  client-owned durable auth logic is introduced by this change.

## Open material decisions

- **DEC-001 — Initial hosted management owner**: Decide whether the first
  delivery exposes connection management only through the existing authorized
  headless/admin workflow operated by Wyrd, or also includes tenant-admin UI
  settings. Both preserve tenant ownership; including settings adds the
  customer self-service and UI authorization journey to the first delivery.
- **DEC-002 — Multiple human connections for one tenant**: Decide whether v1 of
  the generic login experience permits exactly one enabled human connection
  per tenant or supports multiple connections with a tenant-safe chooser. The
  existing trust model can store multiple issuers, but the user-facing
  selection and non-enumeration contract must be explicit before approval.

## Revision history

- **Revision 2 — 2026-09-04 — draft**: Made production qualification a release
  obligation across live Okta, Entra ID, and Keycloak connections; hosted and
  self-hosted deployments; full lifecycle and rotation; production-shaped
  failure behavior; and artifact-bound redacted evidence. Remains unapproved.
- **Revision 1 — 2026-09-04 — draft**: Captured tenant-owned OIDC federation
  for cloud SaaS and self-hosted Wyrd, generic SSO login, discovery/JWKS
  behavior, group-to-role mapping, security constraints, and the two remaining
  delivery decisions. Not approved for planning or implementation.

## Material authority and evidence links

- [`AGENTS.md`](../../../AGENTS.md) — client/server ownership, tenant
  separation, public surfaces, audit, and journey-test requirements.
- [`architecture/wyrd-design.md`](../../../architecture/wyrd-design.md) —
  runtime identity, authentication plane, and language-agnostic surface
  authority.
- [`architecture/wyrd-security-posture.md`](../../../architecture/wyrd-security-posture.md)
  — normative federation, credential, tenant, SSRF, audit, and fail-closed
  requirements.
- [`architecture/wyrd-doctrine.mdx`](../../../architecture/wyrd-doctrine.mdx)
  — product and public-surface doctrine.
- [`architecture/v1/00-foundations/security.md`](../../../architecture/v1/00-foundations/security.md)
  — current Auth and Policy foundation map.
- [`architecture/v1/04-surfaces/deployment.md`](../../../architecture/v1/04-surfaces/deployment.md)
  — supported self-hosted, cloud SaaS, and enterprise topologies.
- [`architecture/operations/deployment-and-release.md`](../../../architecture/operations/deployment-and-release.md)
  and [`architecture/operations/reliability-and-recovery.md`](../../../architecture/operations/reliability-and-recovery.md)
  — production release evidence, real-dependency qualification, and
  fail-closed dependency behavior.
- [`docs/src/content/docs/concepts/cloud-identity.svx`](../../../docs/src/content/docs/concepts/cloud-identity.svx)
  and [`docs/src/content/docs/self-hosting/sso-and-oidc.svx`](../../../docs/src/content/docs/self-hosting/sso-and-oidc.svx)
  — current tenant-scoped issuer and self-hosted setup behavior.
- [`changes/active/wyrd-ui-foundation/spec.md`](../wyrd-ui-foundation/spec.md)
  — approved local-development authentication boundary and production OIDC
  follow-up seam.
