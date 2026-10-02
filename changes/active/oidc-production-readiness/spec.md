---
id: SPEC-oidc-production-readiness
revision: 9
status: approved
---

# Enterprise identity integration for self-hosted and hosted Wyrd

## Objective and user value

A person can sign in to a self-hosted or hosted Wyrd tenant through that
tenant's existing OIDC or SAML provider, then use the UI, CLI, and SDKs without
handling provider tokens. An enterprise may use SCIM to provision the same
tenant Users and role mappings. A deployed application keeps its own Wyrd
identity and needs no interactive employee login. Human federation remains
optional: a Wyrd deployment and tenant work without an identity provider.

One hosted tenant may use Okta while another uses Keycloak in the same Wyrd
service. Hosted signup and automatic tenant creation belong to the separate
commercial distribution, which builds on the open-source server.

This change supersedes the unapproved
[`SPEC-tenant-oidc-federation`](../tenant-oidc-federation/spec.md) draft. It
retains its tenant federation obligations and
closes its connection-selection and delivery-owner decisions. The approved UI
foundation's local mock session is development scaffolding, not production
authentication.

## Scope and ownership

- The open-source Wyrd server owns tenant OIDC/SAML trust and SCIM
  provisioning, principal resolution, role mapping, Wyrd credential issuance,
  and audit.
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
  Testing a candidate connection completes one real interactive sign-in
  through that exact provider, client, and callback, verified exactly as a
  production login, after discovery and signing-key checks. A test sign-in
  only marks that candidate revision tested; it issues no session, Wyrd
  credential, or `User`. Wyrd never infers connection health from
  side-effect probes such as non-interactive `prompt=none` redirects or
  fabricated authorization codes, so any provider that can complete a
  standard login can be tested and activated.
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
  logs, traces, errors, audit, and generated artifacts. Recoverable Wyrd
  login and browser-session credentials, including a pending access/refresh
  token pair or an operator bootstrap API key, are likewise encrypted at rest
  and never returned to an unauthorized client. One deployment sealing
  keyring, held separately from the encrypted data and shared by serving
  replicas, protects these values. It is required whenever provider secrets
  are stored or human login or browser sessions persist recoverable
  credentials, including when the provider uses no client secret; otherwise
  it is optional. Missing key material refuses activation or use of the
  affected human login or session flow without disabling independent machine
  authentication. Operators can rotate the keyring without making existing
  connections or sessions permanently unusable; the rotation procedure is
  documented and tested. No per-tenant OIDC secret is injected into every
  serving replica as an environment variable.

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
- **REQ-008**: Activating a tenant's human connection authorizes successful
  authentication by that exact provider to establish membership in that
  tenant, except when that tenant has enabled SCIM-managed membership under
  REQ-019. A successful callback resolves or creates a tenant-bound `User`
  principal for the verified `(issuer, subject)` and issues tenant-bound Wyrd
  credentials. Outside SCIM-managed membership, Wyrd maps verified provider
  groups only to roles valid in that tenant. `User` is a principal kind, not a
  role; a user with no applicable
  role mapping receives no privileged grant. No provider claim directly names
  Wyrd permissions, and unknown or ambiguous privileged mappings cannot grant
  authority.
- **REQ-009**: The production BFF completes login and maintains a session
  usable across serving replicas. The browser gets a Secure, HttpOnly,
  SameSite cookie and safe session metadata, never a Wyrd bearer or refresh
  token in page data, URL, JavaScript storage, or a JSON callback page. BFF
  completion redeems a short-lived, one-time pending credential only when
  bound to the initiating browser flow and authorized server-side BFF caller;
  missing, expired, replayed, or mismatched claims return no credential or
  session. BFF actions enforce CSRF, expiry, tenant binding, and Wyrd
  permissions. Logout ends the Wyrd browser session; it does not claim to end
  every IdP session.
- **REQ-010**: With OIDC absent, a self-hosted operator can sign in to the UI
  through an existing authorized Wyrd credential. This uses Wyrd's existing
  exchange and session authority; no new local password store is introduced.
  When tenant SSO is active, routine human UI login uses SSO. Explicit
  operator recovery credentials remain available outside the routine UI flow.

### Laptop clients and workloads

- **REQ-011**: `wyrd auth login` for a human opens the system browser and
  completes the tenant's SSO flow without pasting a callback URL or printing
  access or refresh tokens. The CLI uses the OAuth 2.0 Device Authorization
  Grant (RFC 8628) with Wyrd as the authorization server: it shows a user
  code and opens the verification URL, the person signs in through the
  tenant's normal browser login and approves that code, and the CLI polls
  Wyrd's token endpoint for the Wyrd user credential. A wrong, expired,
  denied, or already-redeemed device code returns no credential. Neither the
  provider authorization code nor a Wyrd token is put in a redirect URL. A
  second IdP application registration is not required solely for CLI use.
- **REQ-012**: The CLI stores the renewable Wyrd user credential in a
  user-protected credential store with tenant and server identity. Rust,
  Python, and TypeScript clients resolve it through the shared client and
  renew short-lived Wyrd access tokens automatically without contacting the
  IdP on routine API calls. An explicitly supplied credential overrides the
  saved user credential. With multiple saved logins for one server, the
  `tenant` key selects one; without it, the most recent login for that server
  is used. Local clients sharing a saved login refresh under an exclusive file
  lock and reread the file first, so they reuse a token another client just
  saved instead of replaying its predecessor. A refused refresh asks the user
  to log in again; a client that crashes between the server's rotation and
  the local save replays the old token on retry, which the server's reuse
  detection treats as theft. Logout deletes the saved login locally, then
  revokes it on the server best-effort and warns if revocation fails.
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
  replacement. A person authenticated by the replacement may receive a new
  tenant `User` principal under REQ-008, but that identity does not inherit
  the prior principal's roles, ownership, or history. Linking it to a prior
  identity requires an explicit authorized transition; matching email
  addresses alone never links identities. The owner must have a tested route
  back in before the prior connection retires.
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
  blocked by an existing stronger guard. A BFF session established through the
  old connection cannot renew its Wyrd authority; once its current access
  token expires, it requires login through the active connection. Documentation
  and UI must not promise instantaneous revocation of such tokens or sessions.
- **REQ-017**: Security-significant connection mutations, login outcomes,
  and role changes produce redacted canonical audit evidence under their
  owning authority. A required audit failure cannot silently establish a
  connection or session.
- **REQ-018**: Self-hosted and hosted documentation state who operates Wyrd
  and the IdP, the exact callback and configuration inputs, the one-active-
  connection rule, role mapping, secret rotation, login failures, operator
  recovery, local CLI login and SDK credential selection, and the separate
  human and workload paths. Product surfaces must not advertise production
  SSO while only mock UI authentication works.
- **REQ-019**: A tenant may opt into SCIM 2.0 provisioning from its existing
  identity system. A tenant-bound provisioning credential can manage only that
  tenant's Users and Groups through the standard SCIM resource operations.
  The credential reuses Wyrd's existing tenant API-key verification and is
  authorized only for provisioning; a SCIM client need not perform Wyrd's
  JWT exchange, and no second secret or principal store is introduced.
  The provider's stable `externalId` must equal the verified sign-in subject
  for a provisioned user; email never links identities. In this mode, an
  unprovisioned or inactive user cannot gain tenant membership through login.
  SCIM Groups mapped by an authorized tenant administrator to existing tenant
  roles are the authority for that user's mapped grants; sign-in claims do not
  independently rewrite them. Suspension or removal revokes renewable Wyrd
  authority using the existing User revocation path. Already issued access
  tokens retain only their existing bounded lifetime.
- **REQ-020**: A tenant may configure SAML 2.0 browser SSO as an alternative
  to OIDC for its one active human login connection. Wyrd acts as a service
  provider and supports SP-initiated login through tenant-selected,
  server-bound state. An authenticated SAML response establishes only the
  existing tenant User and Wyrd credential/session authority after signature,
  issuer, audience, recipient, destination, time, request binding, and replay
  validation. SAML metadata and signing-key rotation are tenant-scoped and
  fail closed. A persistent verified NameID is the sign-in subject; transient
  identifiers are refused. SAML assertions and IdP sessions are never Wyrd
  API authority.

## Invariants and non-goals

- **INV-001**: Paths, hosts, headers, browser state, email, and unverified
  provider tokens cannot select effective tenant identity or a connection
  after login starts. Only verified, server-bound state can do so.
- **INV-002**: The stable external user identity is `(issuer, subject)` within
  its tenant. Tenant membership through OIDC requires successful
  authentication by that tenant's active configured provider; a SCIM-managed
  tenant additionally requires active provisioned membership. Email and email
  domain are display or invitation data, never automatic account linking or
  membership authority.
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
- **INV-007**: OIDC, SAML, and SCIM use the same tenant User, role, revocation,
  Wyrd credential/session, and canonical audit authorities. Each protocol has
  its own standards-compliant validation at the trust boundary; no protocol
  introduces a second principal store, role mapper, session issuer, raw SQL
  pool path, or email-based account link. Tenant-scoped SQL uses `TenantConn`;
  cross-tenant operator work uses `OperatorPool`.

LDAP, password authentication, simultaneous Okta and Keycloak
login within one tenant, direct acceptance of arbitrary IdP access tokens as
Wyrd API authority, public SaaS signup, social login, automatic tenant
creation, and commercial-edition scaffolding in the open-source server are
outside this change.

The initial SAML delivery excludes IdP-initiated unsolicited login, Single
Logout, and assertion encryption. A provider requiring one of these features
cannot be claimed as supported until a later approved revision adds it.
The initial SCIM delivery excludes using SCIM as an authentication mechanism.

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
5. Provider switching never links users by email. A replacement-provider
   login may create a separate `User`; linking it to a prior identity requires
   an explicit authorized transition.
6. A local interactive login gives all first-class SDKs renewable Wyrd user
   authority for one server and tenant. The browser session and provider
   tokens are not SDK credentials; deployed workloads use their own identity.
7. A completed OIDC callback hands Wyrd credentials to its initiating browser
   through one server-owned, encrypted, expiring, single-use handoff. The CLI
   receives its credential only through the RFC 8628 device-code grant.
   The deployment keyring protects recoverable provider and Wyrd session
   credentials in both self-hosted and hosted deployments; the number of
   tenants does not select a different secret-storage or login path.
8. SCIM is optional per tenant. Its client-provided `externalId` is the
   provisioned counterpart of the immutable subject asserted at sign-in;
   enabling SCIM makes provisioned active membership authoritative for human
   login and mapped roles. There is no email fallback or silent migration of
   existing Users. Tenant administrators must resolve incompatible subject
   formats or existing accounts before enabling SCIM.
9. OIDC and SAML are mutually exclusive choices for the one active tenant
   human connection. Both feed the existing tenant User and Wyrd credential
   authority. A SAML response selects no tenant from unverified assertion
   data; server-bound request state chooses the tenant and connection.

## Acceptance criteria and evidence

- **AC-001**: An OIDC-off, self-hosted deployment starts and serves an actual
  UI and SDK journey with an existing Wyrd credential; no IdP, OIDC secret,
  or mock-auth flag is present.
- **AC-002**: A real-server self-hosted journey configures a standard OIDC
  provider (Keycloak or Dex), follows the exact shown callback, signs in through the UI, maps
  a role, makes an authorized Wyrd call, and proves a denied call.
- **AC-003**: A real-server hosted journey runs two tenants with different
  OIDC providers (Keycloak and Dex) concurrently. It proves separate configuration, login, roles,
  session use, mutation, and removal, plus wrong-tenant callback and same-
  issuer cross-tenant refusal.
- **AC-004**: Rust, Python, and TypeScript client journeys use a credential
  established by the interactive CLI path without manual token paste, renew
  it without a repeat IdP visit, and reject an expired or revoked credential.
  Journeys prove explicit credential precedence, `tenant` selection and the
  most-recent-login default among saved logins on one server, and that
  concurrent local clients sharing one login do not replay a rotated token. The browser receives no provider
  secret or Wyrd token in page data or redirect URLs.
- **AC-005**: Machine journeys prove an API-key Service or Agent continues
  operating with human SSO enabled and that an opted-in workload assertion
  maps only to its exact bound principal; rotated assertions renew, while a
  wrong issuer, subject, audience, or tenant fails closed.
- **AC-006**: A provider-switch journey proves an owner can test, by
  completing a real sign-in, and activate a replacement, including a provider
  that does not honor `prompt=none`; a test sign-in creates no session,
  credential, or `User`. The journey also proves the owner can preserve an
  authorized route back in, and provision a new `User` through the
  replacement without inheriting the prior principal's authority or linking
  identities through an email match.
- **AC-007**: Fault and security evidence covers IdP outage, unsafe discovery
  and JWKS URL, invalid token and nonce, replayed or expired state, wrong
  callback origin, inactive connection, unsupported client auth, audit
  failure, mapping changes, provider key and secret rotation, absent or
  rotated sealing keys, and BFF session behavior across two serving replicas.
  A missing or wrong browser-flow binding, unauthorized BFF caller, missing or
  wrong, expired, denied, or already-redeemed device code yields no Wyrd
  credential or browser session. An unmapped but valid provider subject
  receives a tenant `User` without privileged grants, and an old-connection
  BFF session cannot renew after replacement or removal.
- **AC-008**: Wyrd is provider agnostic: it uses only standard OIDC
  (discovery, authorization code with PKCE, `client_secret_basic` or
  `client_secret_post`, ID-token validation against JWKS, standard claims) and
  has no provider-specific branch. The automated journeys prove this against
  two independent implementations, Keycloak and Dex. Documentation gives the
  generic setup plus short examples for common providers (such as Okta,
  Microsoft Entra ID, Google, Auth0, and Keycloak). There is no per-provider
  live qualification record or certified-provider list.
- **AC-009**: Public contracts, CLI help, UI, self-hosted and SaaS docs, and
  generated schemas agree on optional OIDC, setup inputs, callback, one active
  connection, credential ownership, and failure behavior.
- **AC-010**: A real-server SCIM journey provisions, updates, suspends, and
  removes users and groups for one tenant while another tenant remains
  unchanged. A provisioned user signs in through its verified immutable
  subject, receives only mapped current roles, and loses renewable authority
  after suspension. Wrong-tenant, inactive, unprovisioned, replayed, and
  email-match attempts grant no membership or role.
- **AC-011**: A real-server SAML journey signs a tenant user in through a
  standard SAML 2.0 IdP (Keycloak) and existing Wyrd browser/CLI credential path, including a
  hosted two-tenant case. Invalid signature, issuer, audience, recipient,
  destination, request binding, time, replay, inactive connection, or
  transient NameID fails without issuing Wyrd authority. OIDC and machine
  paths continue to work for their configured tenants and principals.

## Open material decisions

Revision 6 needs human approval of its two durable choices: SCIM-managed
tenants require provisioned active membership with `externalId` equal to the
verified sign-in subject, and SAML is an alternative to OIDC for the tenant's
one active human connection. Revision 5 remains the approved authority for
TASK-001–005 until this draft is approved; TASK-006/007 are proposed only.

## Revision history

- **Revision 9 — 2026-10-02 — approved**: Approved by Steven Forrester:
  follow conventional OAuth 2.0 and OIDC for UI and programmatic access, and
  stay provider agnostic like comparable open-source servers. Replaced the
  live Okta/Keycloak/Entra ID qualification matrix and its evidence record
  with standards-only behavior proven by automated journeys against Keycloak
  and Dex, plus generic and per-provider setup docs.
- **Revision 8 — 2026-10-02 — approved**: Approved by Steven Forrester under
  his standing direction that Wyrd does what comparable CLIs and SDKs do. CLI
  login uses the RFC 8628 device-code grant (as gh and aws sso do), replacing
  the custom polled browser/CLI handoff. Saved-login renewal and logout match
  gh, gcloud, and aws: refresh under a file lock and save the result, ask for
  a new login on refusal, use the most recent login when no `tenant` is
  given, and on logout delete locally then revoke best-effort. Removed the
  refresh-pending marker, per-request generation revalidation, logged-out
  tombstone, custom lock deadline, and per-login format version. Accepted
  consequence: a crash between rotation and save makes the retry look like
  refresh-token reuse, which revokes that User's refresh tokens.
- **Revision 7 — 2026-10-01 — approved**: Approved by Steven Forrester.
  Connection testing completes one real interactive sign-in and never infers
  provider health from side-effect probes (`prompt=none` redirect or
  fabricated-code token-endpoint probes). Evidence: Dex v2.38–v2.45 ignores
  `prompt` (dexidp/dex#4560) and classic Amazon Cognito hosted UI ignores
  `prompt=none`, so the probe locked out providers that complete standard
  logins; `invalid_grant` ordering is not mandated by RFC 6749 §5.2. Also
  approves revision 6's SCIM and SAML additions unchanged.
- **Revision 6 — 2026-10-01 — draft**: Proposed two additional enterprise
  integration tasks: SCIM provisioning and SAML SSO, both sharing existing
  Wyrd identity and security authorities. Keeps revision 5 tasks intact
  pending approval of the new identity-matching and active-connection rules.
- **Revision 5 — 2026-09-26 — approved**: Approved the existing encrypted,
  short-lived, one-time browser/CLI credential handoff. Clarified that the
  deployment sealing keyring also protects recoverable login and browser
  session credentials, including secretless-provider and OIDC-off browser
  sessions; required initiator-bound, single-use redemption and key-failure
  evidence. This supersedes revision 4's provider-secrets-only key condition.
- **Revision 4 — 2026-09-25 — approved**: Clarified the CLI-to-SDK local user
  credential flow, explicit precedence, tenant selection, and safe concurrent
  renewal of a shared saved login.
- **Revision 3 — 2026-09-25 — draft**: Made successful login through the
  tenant's active provider sufficient for tenant `User` provisioning without
  a privileged default. Clarified that provider replacement never transfers
  an old identity's authority and that old-connection BFF sessions stop at
  their current access token's expiry.
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
- [SCIM 2.0 protocol](https://www.rfc-editor.org/rfc/rfc7644.html),
  [SCIM core schema](https://www.rfc-editor.org/rfc/rfc7643.html),
  [SAML browser SSO profile](https://docs.oasis-open.org/security/saml/v2.0/saml-profiles-2.0-os.pdf),
  and [SAML metadata](https://docs.oasis-open.org/security/saml/v2.0/saml-metadata-2.0-os.pdf):
  the separate enterprise provisioning and federation trust boundaries.
- [OWASP Cryptographic Storage](https://cheatsheetseries.owasp.org/cheatsheets/Cryptographic_Storage_Cheat_Sheet.html)
  and [Secrets Management](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html):
  protection and management of stored credentials and deployment keys.
