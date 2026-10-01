---
id: SPEC-oidc-production-readiness
revision: 2
status: draft
---

# Production OIDC for self-hosted and hosted Wyrd

## Objective and user value

A person can sign in to a self-hosted or hosted Wyrd tenant through that
tenant's chosen OIDC provider, then use the UI, CLI, and SDKs without handling
provider tokens. A deployed application keeps its own Wyrd identity and needs
no interactive employee login. OIDC is optional: a Wyrd deployment and tenant
remain usable without an identity provider.

One hosted tenant may use Okta while another uses Keycloak in the same Wyrd
service. Hosted signup and automatic tenant creation belong to the separate
commercial distribution, which builds on the open-source server.

This change supersedes the unapproved
[`SPEC-tenant-oidc-federation`](../tenant-oidc-federation/spec.md) draft. It
retains its tenant federation and production qualification obligations and
closes its connection-selection and delivery-owner decisions. The approved UI
foundation's local mock session is development scaffolding, not production
authentication.

## Scope and ownership

- The open-source Wyrd server owns tenant OIDC trust, callback verification,
  principal resolution, role mapping, Wyrd credential issuance, and audit.
- The existing SvelteKit BFF owns browser session handling and calls Wyrd as
  the signed-in tenant principal. The browser never becomes the identity or
  token authority.
- The shared Rust client owns credential resolution and renewal for the Rust,
  Python, and TypeScript SDKs. The CLI initiates interactive login and writes
  the resulting user credential for that shared client to consume.
- The separate commercial distribution may add public signup and automatic
  tenant creation by using the open-source server's ordinary tenant and
  identity contracts. This change defines no signup behavior or extension
  stub in the open-source server.
- The deployment-wide platform-administrator OIDC connection remains separate.
  It cannot authenticate a tenant user or substitute for a tenant connection.

## Required behavior

### Tenant connection and operator setup

- **REQ-001**: Human OIDC is optional per tenant. Absent a connection, startup,
  tenant provisioning, machine authentication, and authorized Wyrd credential
  use work without an IdP or OIDC-specific server environment variable.
- **REQ-002**: Each tenant has at most one **active human login connection**.
  A tenant cannot offer Okta and Keycloak simultaneously in this delivery;
  different tenants may select different providers. One tenant's connection,
  client credentials, roles, and lifecycle never affect another's, even when
  the issuer URL is shared.
- **REQ-003**: An authorized tenant administrator can create, inspect in
  redacted form, test, replace or rotate, activate, deactivate, and remove its
  connection through a headless Wyrd API. The UI projects this contract as
  tenant settings; neither surface can administer another tenant. Hosted
  changes take effect without restarting Wyrd replicas. Self-hosted boot
  seeding may use the same durable contract but is not a second trust model.
- **REQ-004**: Setup shows the exact public callback URL the customer must
  register for its Wyrd Web application. Required customer inputs are the
  issuer URL, client ID, a client secret only if the selected supported Web
  client authentication method needs one, and optional tenant role mappings.
  Wyrd discovers provider endpoints and JWKS, validates the configured issuer,
  and never asks for a signing certificate. The human ID-token audience is
  derived from the configured client ID rather than entered twice. Only
  implemented client authentication methods may be offered or accepted;
  `private_key_jwt` is refused until it is implemented end to end.
- **REQ-005**: A provider secret is accepted only at an authorized server
  boundary, encrypted at rest, and absent from read responses, browser data,
  logs, traces, errors, audit, and generated artifacts. A deployment sealing
  secret is required only while provider secrets are stored. Operators can
  rotate it without making existing connections permanently unusable; the
  rotation procedure is documented and tested. No per-tenant OIDC secret is
  injected into every serving replica as an environment variable.

### Browser login and session

- **REQ-006**: The canonical unauthenticated entry for a tenant is
  `/t/{tenantKey}/login` on the deployment's public origin. It presents one
  generic **Sign in with SSO** action only when that tenant has an active
  human connection. The route key is pre-login routing context, never trusted
  tenant authority. The common provider callback is on the same public origin;
  it derives tenant and connection solely from server-owned, single-use login
  state. The public origin and allowed callback are deployment-controlled,
  never assembled from arbitrary `Host` or forwarded headers.
- **REQ-007**: Human login uses authorization code with PKCE, state, and nonce;
  exact registered redirect binding; bounded one-time state; verified ID-token
  signature, issuer, audience, algorithm, key, time, and claims; and screened,
  bounded provider network calls. An unavailable, unsafe, untrusted, inactive,
  or mismatched provider fails closed without trying another tenant or the
  platform connection.
- **REQ-008**: Successful callback resolves or creates only a user in the
  bound tenant. Wyrd maps verified provider groups to roles valid in that
  tenant and issues tenant-bound Wyrd credentials. No provider claim directly
  names Wyrd permissions; unknown or ambiguous privileged mappings cannot
  grant authority. A user with no applicable grant receives no privileged
  default.
- **REQ-009**: The production BFF completes login and maintains a session
  usable across serving replicas. The browser gets a Secure, HttpOnly,
  SameSite cookie and safe session metadata, never a Wyrd bearer or refresh
  token in page data, URL, JavaScript storage, or a JSON callback page. BFF
  actions enforce CSRF, expiry, tenant binding, and Wyrd permissions. Logout
  ends the Wyrd browser session; it does not claim to end every IdP session.
- **REQ-010**: With OIDC absent, a self-hosted operator can sign in to the UI
  through an existing authorized Wyrd credential. This uses Wyrd's existing
  exchange and session authority; no new local password store is introduced.
  When tenant SSO is active, routine human UI login uses SSO. Explicit
  operator recovery credentials remain available outside the routine UI flow.

### Laptop clients and workloads

- **REQ-011**: `wyrd auth login` for a human opens the system browser and
  completes the tenant's SSO flow without pasting a callback URL or printing
  access or refresh tokens. The CLI obtains the Wyrd user credential through
  a short-lived, one-time handoff bound to the initiated login; neither the
  provider authorization code nor a Wyrd token is put in a redirect URL.
  A second IdP application registration is not required solely for CLI use.
- **REQ-012**: The CLI stores the renewable Wyrd user credential in a
  user-protected credential store with tenant and server identity. Rust,
  Python, and TypeScript clients resolve it through the shared client and
  renew access tokens automatically. Explicit credentials still override
  ambient user credentials. A user can log out or revoke the saved session.
- **REQ-013**: A deployed Service or Agent uses its own scoped Wyrd API key by
  default. Where the deployment chooses workload federation, Wyrd accepts a
  verified, audience-bound platform assertion only through the existing
  trusted-workload-issuer and exact workload-binding contract, then issues
  that workload's Wyrd token. Rotating platform assertions are reread on
  renewal. Neither machine path requires a human IdP login or inherits a
  person's roles. Enabling human SSO does not disable these machine paths.

### Provider replacement and multi-tenant use

- **REQ-014**: A tenant owner can replace one human provider with another by
  configuring and successfully testing the replacement before activation.
  Activation is one tenant-scoped transition: new logins use only the
  replacement. Existing people must be invited or explicitly link their new
  provider identity through an authorized transition; matching email
  addresses alone never links identities or grants tenant membership. The
  owner must have a tested route back in before the prior connection retires.
- **REQ-015**: A person who belongs to more than one tenant authenticates
  separately under each tenant's configured provider when required. A Wyrd
  account or provider assertion in one tenant never grants access to another.
  The BFF tenant switch revalidates membership and prompts for that tenant's
  login when needed.

### Lifecycle and failures

- **REQ-016**: Connection deletion, deactivation, or replacement blocks new
  login and renewal through the old connection immediately. Changes to group
  mappings affect the next Wyrd token issuance. Already issued tenant access
  tokens retain their bounded snapshot authority for no more than the existing
  five-minute lifetime unless their principal or tenant is independently
  blocked by an existing stronger guard. Documentation and UI must not promise
  instantaneous revocation of such tokens.
- **REQ-017**: Security-significant connection mutations, login outcomes,
  and role changes produce redacted canonical audit evidence under their
  owning authority. A required audit failure cannot silently establish a
  connection or session.
- **REQ-018**: Self-hosted and hosted documentation state who operates Wyrd
  and the IdP, the exact callback and configuration inputs, the one-active-
  connection rule, role mapping, secret rotation, login failures, operator
  recovery, and the separate human and workload paths. Product surfaces
  must not advertise production SSO while only mock UI authentication works.

## Invariants and non-goals

- **INV-001**: Paths, hosts, headers, browser state, email, and unverified
  provider tokens cannot select effective tenant identity or a connection
  after login starts. Only verified, server-bound state can do so.
- **INV-002**: The stable external user identity is `(issuer, subject)` within
  its tenant. Email and email domain are display or invitation data, never
  automatic account linking or membership authority.
- **INV-003**: Platform administrators, tenant users, and workloads remain
  distinct principal types and authorization planes. An IdP group cannot
  create a platform administrator or bypass tenant RBAC.
- **INV-004**: OIDC discovery provides metadata, not trust. TLS, SSRF and DNS
  pinning, JWKS verification, key rotation, replay protection, audit, and
  Postgres tenant isolation remain fail closed.
- **INV-005**: All first-class SDKs and the UI project server-owned identity
  and permissions; none performs durable role mapping or trusts provider
  tokens as Wyrd authorization.
- **INV-006**: The open-source server has no hosted-signup, social-login,
  licensing, edition-detection, or commercial onboarding stub. A commercial
  distribution adds its own behavior through ordinary Wyrd contracts.

SAML, LDAP, SCIM, password authentication, simultaneous Okta and Keycloak
login within one tenant, direct acceptance of arbitrary IdP access tokens as
Wyrd API authority, public SaaS signup, social login, automatic tenant
creation, and commercial-edition scaffolding in the open-source server are
outside this change.

## Expensive-to-reverse decisions and boundaries

1. The tenant remains the sole durable customer security boundary. A human
   connection is tenant-owned, and exactly one may be active at a time.
2. Human login, CLI login, and the UI yield Wyrd-issued tenant authority;
   workloads retain independent Wyrd principals and credentials.
3. The public tenant login route uses the existing `/t/{tenantKey}` UI
   namespace. The provider callback is a deployment-controlled public route
   with tenant identity carried in one-time server state, so one public origin
   serves self-hosted and multi-tenant SaaS deployments.
4. Hosted signup is owned entirely by a separate commercial distribution.
   The open-source server supplies its existing tenant and identity contracts
   without a stub, mock, placeholder, or edition gate for that product.
5. Provider switching never links users by email; an explicit authorized
   identity transition or invitation is required.

## Acceptance criteria and evidence

- **AC-001**: An OIDC-off, self-hosted deployment starts and serves an actual
  UI and SDK journey with an existing Wyrd credential; no IdP, OIDC secret,
  or mock-auth flag is present.
- **AC-002**: A deployed self-hosted journey configures a controlled real
  provider, follows the exact shown callback, signs in through the UI, maps
  a role, makes an authorized Wyrd call, and proves a denied call.
- **AC-003**: A deployed hosted journey runs two tenants with different real
  providers concurrently. It proves separate configuration, login, roles,
  session use, mutation, and removal, plus wrong-tenant callback and same-
  issuer cross-tenant refusal.
- **AC-004**: Rust, Python, and TypeScript client journeys use a credential
  established by the interactive CLI path without manual token paste, renew
  it, and reject an expired or revoked credential. The browser receives no
  provider secret or Wyrd token in page data or redirect URLs.
- **AC-005**: Machine journeys prove an API-key Service or Agent continues
  operating with human SSO enabled and that an opted-in workload assertion
  maps only to its exact bound principal; rotated assertions renew, while a
  wrong issuer, subject, audience, or tenant fails closed.
- **AC-006**: A provider-switch journey proves an owner can test and activate
  a replacement, preserve an authorized route back in, and cannot gain
  another identity's membership through an email match.
- **AC-007**: Fault and security evidence covers IdP outage, unsafe discovery
  and JWKS URL, invalid token and nonce, replayed or expired state, wrong
  callback origin, inactive connection, unsupported client auth, audit
  failure, mapping changes, provider key and secret rotation, and BFF session
  behavior across two serving replicas.
- **AC-008**: Provider qualification uses controlled Okta, Keycloak, and
  Entra ID accounts over externally trusted TLS for each combination publicly
  claimed as supported. Redacted results identify the immutable Wyrd artifact,
  topology, public origin, provider configuration fingerprint, execution time,
  and outcome. Mock providers remain useful for continuous tests but cannot
  alone justify a production support claim.
- **AC-009**: Public contracts, CLI help, UI, self-hosted and SaaS docs, and
  generated schemas agree on optional OIDC, setup inputs, callback, one active
  connection, credential ownership, and failure behavior.

## Open material decisions

None. Implementation and review may reveal a conflict requiring a new draft
revision and human approval.

## Revision history

- **Revision 2 — 2026-09-24 — draft**: Removed hosted signup and automatic
  tenant creation from this open-source change. The commercial
  distribution owns that separate work; no open-source stub is required.
- **Revision 1 — 2026-09-24 — draft**: Consolidated optional tenant OIDC,
  one active human provider, production UI and laptop login, independent
  workload authentication, hosted onboarding, provider switching, and
  production qualification. Superseded the unapproved tenant federation draft.

## Authority and standards

- [`AGENTS.md`](../../../AGENTS.md) and
  [`architecture/agent-rules.md`](../../../architecture/agent-rules.md):
  tenant isolation, server ownership, audit, SSRF, and journey tests.
- [`architecture/wyrd-design.md`](../../../architecture/wyrd-design.md),
  [`architecture/wyrd-security-posture.md`](../../../architecture/wyrd-security-posture.md),
  and [`architecture/wyrd-doctrine.mdx`](../../../architecture/wyrd-doctrine.mdx):
  principal planes, Wyrd credentials, and public surfaces. Approved changes
  to this spec must be reflected there where an older statement conflicts.
- [`SPEC-wyrd-ui-foundation`](../wyrd-ui-foundation/spec.md): BFF boundary,
  canonical tenant route, and the current local-only mock authentication.
- [`SPEC-tenant-oidc-federation`](../tenant-oidc-federation/spec.md): superseded
  draft and source of tenant federation qualification requirements.
- [OIDC Core](https://openid.net/specs/openid-connect-core-1_0.html),
  [OIDC Discovery](https://openid.net/specs/openid-connect-discovery-1_0-22.html),
  [RFC 8252](https://www.rfc-editor.org/rfc/rfc8252.html),
  [RFC 9700](https://www.rfc-editor.org/rfc/rfc9700.html), and
  [RFC 10017](https://www.rfc-editor.org/rfc/rfc10017.html): protocol and
  browser/native-client security guidance.
