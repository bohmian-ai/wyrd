# Wyrd Security Posture

This document is the normative security architecture for Wyrd. It applies to
self-hosted, multi-tenant SaaS, and single-tenant enterprise deployments. The
protocol and product boundaries remain owned by `wyrd-design.md`; this document
defines the trust, identity, credential, authorization, audit, and data-access
controls required to operate those boundaries.

## Security principles

- The server owns identity, authorization, tenancy, audit, registry, policy,
  storage, and durable lifecycle decisions. Clients cannot assert durable
  identity or bypass a server decision.
- Every external and internal request is authenticated and authorized at its
  receiving boundary. Network location is not identity.
- Tenant identity comes only from verified credentials. Headers, paths,
  payloads, object names, and peer-supplied bytes never select the effective
  tenant.
- Tenant-scoped SQL runs through `TenantConn` under Postgres RLS. Cross-tenant
  work runs only through the explicitly privileged `OperatorPool` capability.
- Security uncertainty fails closed. Verification, authorization, policy,
  tenant binding, and fencing failures deny the operation. Audit is
  non-blocking: a decision that fails to commit is logged and counted and does
  not deny the operation.
- Secrets are resolved from a deployment secret provider at runtime. They are
  never Card fields, generated artifacts, logs, traces, errors, or audit
  payloads.
- Authentication establishes who may call a Wyrd route. Policy governs Card
  state (classify, gate). These are separate decision planes.

## Trust boundaries

| Boundary | Required control |
|---|---|
| Client or agent to gateway | TLS, verified Wyrd token, typed permission, request bounds |
| Gateway to `wyrd-server` | Authenticated transport; original credentials preserved; gateway metadata is never tenant authority |
| `wyrd-server` replica to replica | Mutual TLS from one dedicated cluster CA with the fixed `wyrd-peer` identity; the receiver checks every operation's context against its own state |
| Application to Postgres | Tenant traffic runs under RLS through `TenantConn` or the named single-statement owners; the operator pool is excluded from handlers by construction. Operators supply the logins; boot refuses superuser and `BYPASSRLS` logins. Production uses a separate tenant login that does not own Wyrd's objects, so RLS confines it even if compromised. One ordinary login (local and small self-hosted) keeps RLS per pool, but a process compromise could set the operator flag and reach every tenant |
| Application to object storage | Workload identity or short-lived credentials; tenant-qualified prefixes; encryption in transit and at rest |
| Server to a Source endpoint | Credential indirection, DNS resolution and SSRF screening, address pinning, bounded IO |

## Principal and credential lifecycle

Wyrd has two administration planes, and an identity belongs to exactly one.

The **platform plane** operates the deployment: tenant lifecycle and recovery
of a tenant's administration. Its principals live in `platform.principals`,
carry no tenant, and are never implicitly authorized over a tenant's
resources. The **tenant plane** operates one tenant's resources. Its
principals live under `wyrd.*` behind RLS and can never reach the platform
plane. Neither plane's credential or token is accepted by the other.

`PrincipalKindTag` is the closed wire set shared by both planes:
`global_admin`, `tenant_admin`, `user`, `service`, and `system`. A
platform principal is `global_admin` or `user`; a tenant principal is
`tenant_admin`, `user`, `service`, or the internal-only `system`. Agent is a
Card kind, not a principal kind. The runtime `Principal { id, kind,
tenant_id, roles, effective_permissions, credential_id, card_access,
workload_uid }` carries the tenant-plane `PrincipalKind`, and
`PlatformPrincipal { id, kind, effective_permissions, credential_id }` the
tenantless platform identity.

No principal binds a Card. A `service` principal is a stable account an
administrator creates explicitly; it serves Services, Agents, CI, and
automation and outlives every release. A Card root is request execution
context: the optional exchange or `x-wyrd-workload-uid` selection, signed as
`workload_uid` with a `card_access` limit. `tenant_admin`, `global_admin`, and
`user` hold no restriction; an administrative or human identity is not a
registered AI-system component.

Gateway capture is a server-internal write that holds no token, evaluates no
permission, and writes no audit decision. Its rows carry the verified
subject/actor pair the call ran under. Two boundaries protect that
attribution. First, public Gate ingestion derives every row's pair from the
verified token and refuses supplied SUB/ACT columns; only server-owned outbox
writes carry per-row pairs, and Scribe validates each as a principal id.
Second, outbox lanes and batches are tenant-scoped: Scribe compares the
authenticated tenant with the frame's principal tenant and binds the tenant's
physical location, so callers of one tenant may share a frame but tenants never
do. Outbox frames name `PLATFORM_AUDIT_PRINCIPAL`, the no-verified-caller
marker, as their transport principal; it grants nothing and never replaces a
row's verified pair or the invocation caller in audit. Gate refuses every
public write to `bifrost.gateway.calls` and `bifrost.storage.objects`. A pod without Scribe sends its outbox
writes over the peer plane's capture RPC, admitted by `wyrd-peer` mTLS alone.
Each request's rows carry the subject/actor pairs the local writer would
stamp, and the RPC accepts only the seven server-internal destinations: retained audit history, the two capture tables,
the three Verifier result tables, and the storage object inventory. Only audit history may name the reserved
system tenant; an audit row keeps its decision pair as content. Card file
reads consult that inventory only after the caller's Card decision, as the
tenant's `System` principal granted that one table, so they neither need nor
widen a caller's Bifrost query permission and never cross tenants. A compromised cluster
member, already out of scope, is the only caller that could misuse it.

Registering a Service or Agent Card creates Card versions only, never a
principal, role grant, or credential. `POST /v1/principals` creates a Service
account atomically with its roles, optional `workload_restriction`, and first
credential, returned once; failure creates nothing. A restriction is
`{"mode":"unrestricted"}` (the default) or
`{"mode":"family","kind":"Service","name":...}`. A family restriction
limits Card-attributed emission and verification to that family's roots and
makes the account the family's unique scheduled runtime account; a second live
account for the family conflicts with `WYRD_AUTH_409_WORKLOAD_BINDING_CONFLICT`.
Creating or replacing a family restriction
(`PUT /v1/principals/{id}/workload-restriction`) additionally requires the
tenant-administrative wildcard permission, audited as
`auth.principal.workload_restriction.set`; the replacement is idempotent,
atomic, Service-only, and changes no id, role, or credential. A rootless
User, tenant administrator, or unrestricted Service is `any_registered` and
may attribute evidence to any registered observation-target Card in its own
tenant; ingest resolves those references against the tenant registry, never
across tenants. A rootless family-restricted Service is `root_required`: its
generic authorized operations work, and Card-sensitive work is refused with
`workload_context_required` even under wildcard grants. A selected root is
required to be a Service and resolved under the account's restriction; an unknown and a foreign root are
refused identically. Agent Cards remain valid graph subjects, never workload roots or account families.
Configured Service/version/environment is retained as correlation on inputs/results;
environment and derived identity labels grant no authority. Roles are assigned to principals, not Cards. A tenant
administrator (`*`, not `service_accounts:write`) grants and revokes direct
roles with `PUT` and `DELETE /v1/principals/{id}/roles/{role}`
(`wyrd principal role grant|revoke`); each decision is staged on the Scribe
outbox as `auth.principal.role.grant` or `auth.principal.role.revoke`, and the
change reaches the principal's next token. A user's assignments record their
source: login replaces only `idp` assignments, so a direct grant survives the
next login, and tokens carry the union of both sources.
Every credential has one account owner and no version; releases never
require rotation.
The verification runtime also provisions one UUIDv7 `system` principal per
tenant, the server's own identity for continuous verification work that has no
direct principal. It is not publicly manageable, has no
credential, role grant, refresh, workload, or delegation path, and never
appears in a token: every issuer refuses it and every verifier rejects a
`system` claim set. It is only ever an actor, never a subject: work it
performs records the subject it serves as `sub_principal_id` and SYSTEM as
`act_principal_id`, and it adds no authority to that work. It has three
in-process uses. It performs Verifier result writes, which the server's
internal capture writer submits to Scribe like gateway capture, without a
token or Gate; Gate refuses every public write to the result tables. It
performs a Verifier's agent-check calls, which the gateway authorizes against
the execution's accepted authority, never SYSTEM's. And it carries a tokenless
`bifrost_query:read` authority scoped by table UID to exactly the input table
a scheduled window Verifier reads (`bifrost.drift.observations`), recorded for the
execution's subject. That read
authority is not a general Bifrost grant and is never inferred from the
subject, and Oracle authorizes and audits every read under it like any
caller's. The server never fabricates a user or other identity for its own
reads.

A Verifier run is accepted under one authority that execution never widens.
Queued work is activated only by a binding schedule or a caller's `submit`.
A submitted run keeps the caller's verified effective principal and
delegation chain; its inputs row is staged on the Scribe outbox, and only
after Scribe acknowledges that row is the submission handed to the queue
writer, so an unacknowledged row queues nothing. A scheduled run keeps its
binding owner's authority, recovered from current grants in the run's tenant
when the run is accepted; a missing, inactive, or foreign owner, or a tenant
without SYSTEM, accepts no run and nothing falls back to another identity or
SYSTEM. The accepted authority is stored server-side with no credential,
survives claim, retry, and restart, and is not refreshed by later grant
changes; every protected downstream action still checks its own permission
against it.

A tenant administrator is created once, during tenant provisioning, and is the
tenant's headless root of trust: it holds credentials and roles, federates no
identity by itself, and coexists with any human administrator the tenant later
adds. A platform administrator is created once, during deployment
initialization, and administration of the platform plane is available through
its credential whether or not a platform OIDC connection exists.

Every authorization decision records the deciding principal's stored kind and,
when the presented token was minted from a credential, that credential's
non-secret id. A principal may hold several credentials at once, so the
credential id is what makes a decision attributable across a rotation; a
federated session, a delegated token, and a refresh rotation name no
credential.

### API keys

- API keys are bootstrap credentials for token exchange, not request-session
  credentials.
- The server generates a 256-bit secret and returns the plaintext once, in the
  response that created it. It stores only the secret's SHA-256 verifier and
  non-secret lookup and lifecycle metadata. API keys default to a 365-day
  lifetime.
- A credential's non-secret prefix resolves its owning principal before
  verification — `wyrd_global_...` at platform scope, `wyrd_sk_<tenant>_...`
  within a tenant, the latter entering a tenant-scoped transaction first. Every
  invalid-credential condition performs exactly one verification and returns
  one indistinguishable public error.
- One principal may hold several simultaneously valid credentials. Revoking one
  retires that credential so it mints no new token; tokens it already minted
  lapse at their expiry, and the principal, its grants, and its other
  credentials stay intact.
- Losing every platform credential does not end administration of the
  deployment. An operator-only action beside initialization reissues one for the
  existing root; it is not an HTTP route, requires the deployment's
  platform-admin database access, creates no identity and writes no grant, and
  refuses on an uninitialized deployment. That database access is therefore
  equivalent in authority to holding the platform credential.
- A platform session names the credential that minted it and re-reads that
  credential on every request, so revoking a platform credential ends its live
  sessions on the next request. A platform session
  established by federated login names no credential; the principal itself is
  the anchor, and suspending it ends those sessions on the next request.
- API keys have an expiry, owner, creation audit event, use metadata, and
  revocation state. Rotation creates a new key, changes deployment secrets,
  verifies token exchange, and then revokes the old key.
- The deployment secret provider, never a Card, supplies `WYRD_API_KEY` to an
  SDK or workload.

### Access and refresh tokens

- `/auth/token` derives tenant and principal identity from a verified API key
  or trusted token-exchange subject. Client-supplied tenant identity is
  rejected.
- Tenant access tokens expire five minutes after issuance. Privileged
  operations may require a shorter configured lifetime, but never a longer
  one.
- Only a tenant human login receives a refresh token; platform sessions are
  access-only. Refresh tokens live 30 days, are stored by
  one-way digest, and are rotated on every use for public clients (`wyrd-cli`):
  reuse of a rotated refresh token revokes its rotation chain and emits a
  security audit event (RFC 9700 §4.14.2). The first-party web app uses
  Rust-owned browser sessions, not an OAuth client or refresh-token grant.
  Revoking an OAuth refresh token (RFC 7009) ends that login's renewal; it
  does not withdraw access tokens already issued.
- Every tenant access token carries issuer, audience `wyrd`, subject,
  issued-at, expiry, unique token identity, principal, tenant, Card scope,
  credential attribution, delegation chain, informational roles, and one
  `permissions` set. The `permissions` claim is the only tenant authority; one
  `TenantTokenIssuer` resolves it from current grants at issuance for API-key
  exchange, OIDC login, human refresh, workload `jwt-bearer`, and delegation.
  Verification is local and synchronous — signature, issuer, audience, expiry —
  and reads no store, holds no cache, and resolves no roles.
- Internal verification work mints no token. Results are staged on the
  process Scribe outbox under the operation's pair: a direct request's
  verified caller, or a queued run's subject with SYSTEM as actor. Trace
  input reads are ordinary Bifrost queries of `bifrost.traces.spans` under the
  execution's accepted caller, so Oracle authorizes and audits them like any
  query that caller makes. Scheduled drift window reads use the credentialless
  SYSTEM read authority described above, granted `bifrost_query:read` only on
  the registered `bifrost.drift.observations` table UID; a missing SYSTEM
  principal or unreadable table identity fails closed before any row is
  returned.
- Revoking a credential, suspending or deleting a principal, suspending a
  tenant, or changing grants refuses the next issuance immediately. A tenant
  token already issued keeps its snapshot authority until its five-minute
  expiry; there is no revocation list or authorization epoch. The platform
  plane instead revalidates current state on every request.
- Bearer access tokens remain replayable until expiry. TLS, short lifetime,
  token-family replay detection, least privilege, and audit are the required
  replay controls. Logs and traces never record bearer material.

### OAuth authorization server

- Wyrd is the OAuth 2.0 authorization server for its headless clients. Its
  device authorization grant (RFC 8628), refresh grant (RFC 6749 §6),
  revocation (RFC 7009), token exchange (RFC 8693), JWT bearer assertions
  (RFC 7523), and RFC 8414 metadata at
  `/.well-known/oauth-authorization-server` retain their machine-client
  contracts. First-party browser login is the session boundary below, not an
  additional OAuth authorization-code client. UI-only authorization/refresh
  routes and advertised grants with no remaining consumer are retired; no
  compatibility aliases or replacement client registration are introduced.
- `wyrd-cli` is the public client used by CLI/SDK login. The first-party web
  app is not a separate OAuth client: Rust owns its browser-session boundary.
  A Node client secret or UI cookie-encryption key is not provisioned. Public
  OAuth grant, client identification, redirect and refresh rules remain
  explicit for their actual machine-client consumers; browser login does not
  introduce a replacement OAuth client or relax exact provider callbacks.
- Device approval records only the approving principal, tenant and
  connection; tokens are minted on device-code redemption, never stored
  awaiting pickup. Existing authorization-code records retain their history
  and retention rules; retiring UI issuance does not delete those records.
  Provider-side authorization codes are exchanged only through the existing
  bound relying-party callback and never become browser session credentials.
- The token, platform token, revocation, and device authorization endpoints
  take form-encoded bodies and answer with `Cache-Control: no-store`. Token
  success is the RFC 6749 §5.1 token response, device authorization success
  is the RFC 8628 §3.2 response, and revocation success is an empty `200`
  (RFC 7009 §2.2). Every refusal is the RFC 6749 §5.2 error JSON, with
  `400`, `401` for `invalid_client`, `500` for `server_error`, or `503` for
  `temporarily_unavailable`. They are
  the one exception to `WyrdError` problem+json: a refusal converted from a
  `WyrdError` is logged under its Wyrd code, while a malformed form or
  client-identification refusal is answered directly with no Wyrd code.
- Interactive CLI login rate limiting is an ingress concern: deployments
  rate-limit `POST /auth/device`, the user-code entry form, per client address
  (RFC 8628 §5.1).
- The deployment sealing keyring protects only stored provider and
  workload-issuer client secrets and gateway `managed_secret` provider keys.
  A gateway managed secret is sealed with its tenant, name, and provider
  inside the authenticated payload, so an envelope copied into another
  tenant's or credential's row does not open. Wyrd stores no recoverable access token,
  refresh token, authorization code, device code, or API key.
- `wyrd init` keeps local database URLs and the tenant key only in the
  protected `credentials.toml`; `config.toml` `[dev]` holds non-secret
  settings. Gateway documents name provider keys as `local_env` sources the
  CLI resolves at apply time, and the server applies a composite document
  atomically, so a refused document commits nothing.

### First-party browser sessions

- Rust owns the complete browser-session lifecycle in the existing auth and
  SQL owners. Node renders pages and forwards the selected tenant's cookie
  and trusted public-origin context; it owns no durable session, OAuth code
  exchange, refresh token, API key or credential encryption. The existing
  database is the shared replica authority, with no additional secret/env
  configuration, Redis, session service or sticky routing.
- `POST /auth/browser/login` starts a tenant provider login and
  `POST /auth/browser/api-key` verifies a recovery key before creating a
  browser session. The existing provider callback finishes the exact bound
  login transaction. State, nonce, PKCE, issuer, connection revision, public
  origin and one-use completion remain validated by the same Rust owners as
  device and connection-test login. Browser transaction cookies are opaque
  bindings; no PKCE verifier is exposed to Node or browser storage. Login
  transactions expire within five minutes. Post-login redirection is to the
  resolved tenant's fixed signed-in path, never an arbitrary return URL.
- Session identifiers use cryptographic randomness and are regenerated at
  successful login. A host-only HttpOnly, SameSite=Lax cookie carries only
  that identifier, not OAuth tokens, API keys or identity/permission claims.
  Cookies are Secure in production; only the existing loopback-development
  HTTP exception is permitted. Raw identifiers never enter database rows,
  metadata, URLs, logs or traces. The record stores a one-way digest, tenant,
  principal, twelve-hour absolute expiry, revocation and either the exact
  human-connection revision or originating credential reference. There is
  no sliding renewal and no recoverable credential vault. Database-owned
  timestamps and tenant RLS govern the record; existing auth maintenance owns
  expiry cleanup.
- `GET /auth/browser/session` returns only safe tenant/principal/permission/
  expiry metadata for presented live sessions; an optional tenant route key
  filters those sessions, not their authority. Enumeration clears invalid or
  expired cookies and returns only live per-tenant contexts, without merging
  principals/permissions across tenants. A refused selected tenant never falls
  back to another session. Store failure still refuses the whole request.
  Cookie names and readable labels are never trusted as membership. No-session and invalid/expired/
  revoked session is an authentication refusal, distinct from store
  unavailability. `POST /auth/browser/logout` revokes only the selected tenant
  session and clears its cookie. Recovery logout never revokes the API key or
  other browser sessions; logout does not end the IdP session.
- Every browser-session request checks current tenant/principal authority.
  Human sessions require the exact Active connection revision. API-key-backed
  sessions require a still-valid originating credential and preserve all its
  effective-permission restrictions. Existing grant resolution, credential
  attribution, permission checks and non-blocking audit are reused; no Node
  role mapping, durable credential cache or fallback authority exists. Tenant
  bearer verification remains local and synchronous until token expiry; this
  session boundary does not change that contract.
- The existing protected tenant `/v1` HTTP APIs explicitly accept cookie
  sessions through their normal principal/permission boundary. Platform,
  MCP, OTLP, gateway and gRPC routes do not inherit cookie authentication.
  Tenant API cookie authentication requires exactly one selected session;
  ambiguous multiple session cookies are refused, never first-cookie-wins.
  Only the session-metadata enumeration accepts multiple tenant sessions.
  When `X-Wyrd-Access-Token` is present, existing bearer rules always apply;
  refusal never falls back to a cookie. Every cookie-authenticated mutation,
  including login, recovery and logout, requires exact trusted public Origin;
  missing or mismatched Origin is refused. The provider callback instead
  requires its bound one-use transaction. Node validates incoming form/action
  origin before forwarding and cannot replace a hostile or missing Origin
  with trusted authority. SameSite and SvelteKit checks supplement, rather
  than replace, Rust enforcement. Read-only GETs never mutate settings or log
  out a session.
- Session metadata and credential-bearing responses use
  `Cache-Control: no-store`. Existing Wyrd error codes distinguish refused authority from
  unavailable storage; a database outage cannot become anonymous, cached or
  mock success. Cookie authentication is documented alongside bearer
  authentication in served OpenAPI. UI-only OAuth issuance/renewal is retired
  when its consumers move, without changing CLI/device, SDK, platform,
  workload or provider-test behavior. Existing credential-bearing UI cookies
  are cleared and require sign-in; there is no plaintext compatibility path.

### Delegation and federation

- Cross-service delegation uses RFC 8693 token exchange: the actor presents
  the subject's token as `subject_token` and its own as `actor_token`; the
  actor must be a direct Service token with a selected root, and the signed
  token names the subject as its principal and the actor, with its
  `workload_uid` and optional credential `cid`, as its outer `act`. The
  effective `card_access` is the intersection of the subject's signed limit
  and the actor's resolved root graph; it may be empty, and a `root_required`
  subject cannot delegate Card authority it has not established.
  Both are verified from that token; no caller-supplied identity header is
  accepted, `act` confers no authority, and exchange consults no policy.
- A delegated token is bound to the `wyrd` or `bifrost` audience; a `bifrost`
  token is refused outside the Bifrost ingest and query surfaces.
- Token exchange adds no permission check of its own. `MAX_DELEGATION_DEPTH`
  is five: issuance refuses a deeper chain and verification rejects one, so a
  non-conforming token never enters request context. Every later request runs
  the ordinary permission check against the delegated principal and its
  intersected set; the `act` chain is attribution for audit only.
- Delegation depth is bounded, every hop is authorized, and the effective
  permission set is the exact intersection of the actor's current permissions
  and the subject's, so it can only narrow. If the complete intersection cannot
  be represented by existing token scopes, refuse exchange before signing and
  issue no token. Do not silently discard common authority or widen a grant.
  Preserve explicit deployment/model grants beside `all`: only those explicit
  grants admit matching `granted` deployments. Representable mixed-grant
  intersections remain supported. Refusal stages its authorization decision on
  the existing nonblocking audit outbox.
- External federation accepts tokens only from an explicitly configured
  issuer, audience, algorithm, and claim mapping. OIDC discovery does not make
  an issuer trusted. A workload binding maps a verified issuer, subject, and
  audience to an active Service account (`principal_id`) and an optional
  default `workload_uid`; the write refuses any other target. At jwt-bearer
  exchange an explicit `workload_uid` wins, then the binding's default, then
  rootless semantics, all under the account's restriction. Assertion claims
  never select a principal and the `tenant` selector only routes.
- A connection is selected by the login entry point, never by a token, header,
  hostname, or post-login chooser. A tenant entry resolves only that tenant's
  connection; the deployment's single platform-scope connection resolves only
  platform principals pre-registered against an expected issuer and claim and
  pinned on `(issuer, subject)` at first login. An unknown platform subject is
  denied and never provisioned just in time, and the platform connection's
  absence or outage never blocks administration through the global
  administrative credential.
- Unknown issuers, unknown keys after one bounded refresh, unavailable JWKS,
  invalid claims, and ambiguous claim mappings fail authentication.
- Every tenant shares one human-login callback, so authorization responses are
  bound to their issuer (RFC 9207) after the login state is consumed and
  before any token-endpoint request: a present `iss` must equal the issuer the
  login state recorded, byte for byte, and an absent `iss` is refused when the
  provider's discovery advertises
  `authorization_response_iss_parameter_supported`. Either refusal is audited
  as a denied exchange and issues no authorization code or token. Support is not
  required to test or activate a connection, so providers that neither send
  nor advertise `iss` (Microsoft Entra ID, Okta, Auth0) still work; for them
  server-bound state, PKCE, and ID-token issuer validation are the controls.
- Residual mix-up exposure: an authorization response without `iss` cannot
  be attributed to an issuer before the code is redeemed. Exploiting that
  requires a malicious tenant connection on the shared callback: a person
  begins login through the attacker's connection, its provider bounces them
  to an honest provider that sends no `iss`, and Wyrd redeems the honest
  provider's code, with the login's PKCE verifier, at the attacker's token
  endpoint. That can disclose a code the attacker may redeem at the honest
  provider for a public client; it never yields a Wyrd session for the
  victim, because the ID token must still verify under the attacker
  connection's own issuer and tenant. Honest providers that send `iss` are
  not exposed. Per-connection callback URLs would close the gap and are the
  upgrade path if it ever needs closing.

## Wyrd signing keys and JWKS

Wyrd-issued JWTs use asymmetric signing with an explicit algorithm allowlist.
Symmetric JWT algorithms, `alg=none`, algorithm substitution, and header-
selected key URLs are rejected.

- Every signing key has a stable `kid`, activation time, retirement time, and
  destruction record.
- Private signing material is held in a KMS, HSM, or deployment secret system
  outside Postgres and is available only to token-issuing processes.
- The public JWKS contains active verification keys and retired keys whose
  issued tokens may still be valid.
- Rotation publishes the new public key before it is used for signing. The
  overlap window is at least the maximum token lifetime plus permitted clock
  skew. A key is removed only after that window and after all issuers have
  stopped using it.
- Emergency compromise disables signing immediately, revokes affected
  credentials, removes the key from accepted verification state, and
  invokes the
  [credential-compromise runbook](operations/runbooks.md#credential-or-signing-key-compromise).
  Availability never justifies accepting a token whose key state is unknown.
- Key inventories, rotation results, and JWKS publication are observable
  without exposing private material.

## Authorization and policy

Every route declares one typed `Permission { resource, action, scope }`.
Issuance resolves permissions within the principal's tenant and signs them into
the token; verification constructs the runtime principal from that claim. Scope never selects or widens
tenancy: tenant identity comes only from the verified principal.

A route whose objects are not known until the request is planned — a Bifrost
SQL query — admits on the coarse operation capability and then takes the
authoritative object decision inside the owning service against the resolved
objects, before admission, audit acceptance, peer dispatch, or source IO. One
uncovered object denies the whole request without returning rows, and the
approved scoped decision is bound into the distributed permission digest so a
worker cannot widen it.

There is no runtime cross-service invoke policy decision point. Wyrd
authorizes every protected API request with its own RBAC permission check on the
verified principal, recorded to canonical audit; there is no separate
policy engine, policy cache, or stale-allow mode to reason about.

### Request principal

HTTP handlers receive identity through two extractors. `AuthenticatedPrincipal`
verifies the signed Wyrd access token from `X-Wyrd-Access-Token`; `Caller`
projects that verified principal with the request ID. `Caller.data_tenant_id`
is copied from `principal.tenant_id`; no request header is tenant authority.

An optional `x-wyrd-workload-uid` header (HTTP, MCP, gateway, and OTLP
metadata) selects request context, never identity or tenant. On a token
that already signs a root, an equal selector is idempotent and a different
one is refused as `signed_workload_mismatch`. On a rootless bearer the
server resolves the root under the token's signed `card_access` and the
account restriction, with no broader live-grant lookup and no activity
recorded; an unknown and a foreign root are refused identically. A
repeated or malformed selector is refused. Native Bifrost and OTLP gRPC
apply the same server-owned selection before Gate admission, so a User bearer
driving a loaded state writes natively under its validated root; signature
verification stays store-free, and Gate itself, without that server-resolved
caller, accepts only a signed root and refuses a selector it would have to
resolve.

### Permission model

`Permission { resource: Resource, action: Action, scope: PermissionScope }` is
the one authorization vocabulary. `resource` and `action` name the operation;
`scope` names the objects it may reach. A static role-derived grant over a
named object is RBAC, not ABAC, and never enters the Policy plane. Handlers
require a typed `Permission`, never a flat string; `resource:action` names an
operation only and the object axis has no string spelling.

`Resource` and `Action` are closed enums in `wyrd_runtime::permission`, each
with `AnyOf(..)` and `Wildcard`. Adding a variant is a reviewed Rust contract
change; there are no custom runtime permission strings and no
`Other(String)` escape hatch.

`PermissionScope` is a closed, domain-tagged union:

- `All` covers every object of the permission's own resource and action. It
  never widens the resource or the action.
- `Bifrost(BifrostPermissionScope)` is valid only on `bifrost_query` `read`.
  `Schema { catalog, schema }` covers every table currently or later resolved
  beneath that logical catalog and schema. `Table { catalog, schema,
  table_uid }` covers exactly that registered table UID and does not follow a
  dropped-and-recreated table; catalog and schema ride with the UID so the
  synchronous check needs no catalog lookup. Catalog and schema are the logical
  SQL names (catalog `bifrost`, schema `logs`), validated as identifier segments.

A future domain that needs object scope adds its own typed variant rather than
reusing another domain's identity spelling.

Coverage is per axis, and a `Permission` covers a requirement only when all
three cover. `Wildcard` covers every resource or action and `AnyOf` covers its
members; otherwise only the identical variant covers. `All` covers every scope;
a schema scope covers itself and every table scope in that exact
catalog/schema; a table scope covers only its UID; no Bifrost scope covers
`All`. A wildcard or multi-resource grant cannot carry a Bifrost scope: decode
rejects it. Grants are additive and absence is denial; there is no explicit
deny, ownership, or grant option.

`PermissionSet` keeps itself minimal on insert: an insert already covered is a
no-op, and entries the insert covers are removed. Every authorization decision
is `PermissionSet::contains`. `PermissionSet::covers_operation` ignores scope
and answers only whether the principal holds the operation under some scope; it
is route admission for requests whose objects are unknown until planning, never
authorization. Built-in administrative authority is ordinary wildcard
permissions, never a role-name bypass.

Roles persist permissions as a JSONB array in `wyrd.auth_roles.permissions`,
the direct serde projection of `Permission`:

```json
[
  {"resource": "cards", "action": "write", "scope": "all"},
  {"resource": {"any_of": ["operators", "triggers"]}, "action": "invoke", "scope": "all"},
  {"resource": "bifrost_query", "action": "read",
   "scope": {"bifrost": {"schema": {"catalog": "bifrost", "schema": "logs"}}}},
  {"resource": "bifrost_query", "action": "read",
   "scope": {"bifrost": {"table": {"catalog": "bifrost", "schema": "traces",
                                   "table_uid": "0199f0c1-2f7e-7c2a-9a6f-1c9b2f4d5e60"}}}}
]
```

`scope` is required; a two-field object is rejected, not promoted to `all`.
Malformed identities, Bifrost scope on another resource, and Bifrost scope on
any action other than `read` are rejected at decode, so a corrupt role row
surfaces by name instead of resolving into authority. Role permissions are the
only static grant model: there is no second grant table, policy lookup, or SQL
`GRANT` language. The shape may add closed variants or optional fields; existing
field names and variant meanings do not change.

### Permission check

`PermissionCheck` is the one runtime authorization chokepoint:

```rust
fn check(&self, principal: &Principal, permission: &Permission) -> PermissionVerdict;
```

It is synchronous and infallible. Denial is `PermissionVerdict::Deny`, not a
Rust error, and the handler boundary maps it into the public catalog
(`WYRD_PERMISSION_403_DENIED_RBAC`). The signature takes no target parameter:
the object is inside `Permission` as its scope. `RbacCheck`, the implementation,
is stateless and allows exactly when
`principal.effective_permissions.contains(permission)`, with no query-path
database or network lookup. Handlers call
`state.permission_check.check(&principal, &permission).into_result()?` and add
no shortcut helpers, so audit, tracing, and policy stay at one call site. CEL
and ABAC never sit behind this trait, and `PermissionDenyReason` has no ABAC
variant.

### Auth crate boundaries

```text
wyrd-runtime      Principal, Permission, PermissionSet, PermissionCheck, builtin roles
wyrd-auth-issue   issuing keys, JWT claims, API-key hashing, delegation depth
wyrd-auth-verify  synchronous JWT verifier, key map, principal from the permissions claim
wyrd-sql          tenant-bound role, principal, and credential rows
wyrd-server       HTTP handlers, TenantConn acquisition, credential writes, audit staging, error mapping
```

Shared auth crates do not depend on SQL. `wyrd-auth`'s `TenantTokenIssuer`
resolves tenant permissions once, at issuance, over `TenantConn`.

### Production composition

Server construction injects the configured permission resolver and canonical
audit writer as required capabilities. A production profile cannot substitute
an in-memory credential store, test key, or best-effort background audit
emitter. Missing or unhealthy capabilities prevent the affected role from
becoming ready. Dependency injection exists to make the boundary testable, not
to make a security control optional.

## Peer identity and distributed Oracle

Internal Oracle and Scribe RPCs run only in peer mode, on a private listener
that uses mutual TLS alone for cluster-member authentication. The default
one-process `all` target makes its Scribe, Oracle, and Forge calls in-process
and opens no private listener.

- Each peer-mode process reads one dedicated Wyrd cluster CA certificate and
  one shared peer leaf certificate and private key from `WYRD_PEER_TLS_DIR`
  (`ca.crt`, `tls.crt`, `tls.key`). The CA private key stays outside every
  Wyrd process and pod.
- Both directions validate the CA chain, validity window, client/server key
  usage, and the fixed `wyrd-peer` DNS identity. The listener refuses a
  same-CA leaf with any other identity before polling a request body. Missing
  or invalid peer inputs keep the process out of ready membership.
- Possession of the shared leaf confers only trusted-cluster-process identity.
  It never confers tenant identity, a specific replica identity, a user
  credential, a database role, or cross-tenant capability. The model does not
  protect against a compromised cluster member that holds the shared key.
- The receiver owns authorization context. For query forwarding,
  reservations, stages and fragments, shuffle reads, Scribe active-stream
  listing, and Scribe live fragments, it checks the applicable tenant,
  table/object/snapshot, query or
  assignment, target node and fence, deadline, protocol version, request body
  digest, and resource bounds against its own trusted state before decoding
  an executable plan or touching tenant storage. A mismatched or unresolvable
  context fails closed with no cross-tenant result.
- A peer refusal is audited on the system-owner chain
  (`DataTenantId::SYSTEM_OWNER`) until the receiver's own query, reservation,
  or stage state binds a tenant. Only a refusal after that binding is
  attributed to the bound tenant. Unverified request bytes never select an
  audit tenant.
- Public client authentication and authorization are unchanged by peer mode;
  peer TLS is not public TLS and grants nothing on public listeners.

## Source credentials and SSRF defense

`SourceAuth` carries secret-provider keys or environment-variable names, never
secret bytes. Operator Cards instead carry a tenant Operator-connection name;
the server resolves its envelope-encrypted Postgres credential under the exact
tenant, provider, HTTP origin, and auth authority immediately before delivery.
Multi-tenant Operator credentials never use process environment selectors.
Secret values are redacted in diagnostics and are not persisted in Card specs
or observations. Rotation changes the connection secret without changing the
Card contract.

Before fetching any user- or tenant-supplied URL, the server must:

1. Parse and normalize the effective URL; allow only adapter-approved schemes,
   ports, redirect behavior, and response limits.
2. Resolve DNS once with a bounded resolver.
3. Reject the request if any resolved address violates the deployment network
   policy. Cloud-metadata and link-local ranges are always rejected;
   loopback, private, carrier-grade NAT, and unique-local ranges are rejected
   in production profiles.
4. Connect to the screened address without re-resolution while preserving the
   intended TLS server name and certificate verification.
5. Re-run the full procedure for each permitted redirect. Redirects cannot
   change to a disallowed scheme, credential authority, or network class.
6. Apply connection, read, body-size, decompression, and total-operation
   bounds, then release all resources on cancellation.

A string allowlist without resolved-address validation and connection pinning
is not an SSRF control.

## Tenant and data isolation

- Tenant Postgres operations use `TenantConn` and transaction-local RLS state,
  except the exact single-statement owners documented below. Both paths derive
  tenant context from verified authority; a caller cannot select or widen it.
- `OperatorPool` is restricted to named platform operations whose owner checks
  tenant equality, lease generation, and fence before mutation. It is not a
  generic query escape hatch.
- Bifrost object keys, Iceberg namespaces, WAL, staged runs, Scribe objects,
  Oracle spill, caches, and telemetry are tenant-qualified.
- Encryption keys are scoped so compromise or erasure of one tenant does not
  require decrypting another tenant's data. Deployments that cannot provide
  per-tenant keys use independently encrypted storage domains and retain the
  same logical isolation contract.
- Tenant identity is repeated in durable metadata and checked at each storage
  transition. Oracle's row tripwire fails the entire query on mismatch; it
  never filters mismatched rows and returns a partial result.

### Postgres tenant binding

Wyrd uses one PostgreSQL control-plane database. `wyrd-sql` owns the
`platform` and `wyrd` schemas, `vala-sql` owns `vala`, and Skald owns none.
Analytical payload bytes stay in Bifrost object storage.

Every tenant-scoped row leads with
`data_tenant_id UUID NOT NULL REFERENCES platform.tenants(data_tenant_id)`,
typed in Rust as `wyrd_spec::ids::DataTenantId`. Human-facing `TenantSlug`
values resolve to a `DataTenantId` at the authenticated boundary and never
enter tenant rows. `space` is a registry namespace inside a tenant, never a
tenancy boundary. Tenant-scoped parents expose composite foreign-key targets
such as `(data_tenant_id, id)`, and children, including cross-schema
references, carry the tenant key in their foreign keys. `platform.*` is the
privileged plane above tenant scope.

Row-level security is the authoritative tenant boundary, and the pool decides
scope. Every table enables and forces RLS. Tenant tables carry a
`tenant_isolation` policy with both predicates:

```sql
CREATE POLICY tenant_isolation ON wyrd.example
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
```

Every table also carries an `operator_access` policy that targets only the
login that ran the migrations and admits sessions with `app.operator=on`.
Platform and catalog pools set that flag; tenant pools never do, and a separate
tenant login gains nothing by setting it because the policy does not apply to
it.

`TenantConn` binds the verified tenant and opens the transaction in one round
trip, and is the only code that builds this statement or issues raw
transaction control:

```sql
SELECT set_config('app.current_tenant', '<tenant uuid>', true); BEGIN
```

The binding precedes `BEGIN` deliberately. Postgres runs both statements as one
implicit transaction that `BEGIN` makes explicit, so the binding survives into
it; if the binding fails, `BEGIN` never runs and Postgres rolls back, leaving
the pooled connection idle. `BEGIN` first would leave a failed binding inside
an aborted transaction that SQLx does not roll back, poisoning the next
borrower. The simple-query form takes no parameters, so the tenant is inlined
from the typed `DataTenantId`, whose UUID rendering cannot contain a quote.
Elsewhere tenant identity is parameter-bound. A missing binding fails
(`wyrd.current_tenant()` is strict) rather than returning an empty or
cross-tenant result; dropping the wrapper rolls back and clears the binding.

Three existing hot-path owners avoid separate transaction round trips:
`OracleTableCuts`, `GatewayAdmissions` and `VerificationTargets`, enumerated by
`STATEMENT_TENANT_OWNERS` in `scripts/check_tenant_isolation.py`. Each executes
one request-role SQL statement whose function binds the verified tenant as
transaction-local RLS state within the statement's implicit transaction. RLS and
caller privileges still apply; an existing transaction bound to another tenant
must be refused, never rebound. On a matching transaction, the caller retains
commit/rollback ownership. The exception is confined to those owner methods,
not arbitrary raw connections, extra statements or new bypasses. Other tenant
work continues through TenantConn.

Postgres enforces only tenant isolation and table-local invariants that hold
for every login. Wyrd's RBAC is the single permission model: migrations add no
per-command, per-column, or per-login permissions, and Wyrd creates no role and
grants nothing to a named role. `SECURITY DEFINER` functions are narrow,
reviewed bridges with a pinned `search_path` that raise the operator flag only
for their own body, such as slug resolution, and never become a general RLS
bypass. Logins, boot validation, and migration order are in
[deployment and release](operations/deployment-and-release.md#database-logins-and-connection-budget).

## Audit integrity and privacy

Audit cardinality follows authorization decisions, not HTTP requests and not
engine mechanics. Permissions are blocking; audits are non-blocking. Every
audited surface stages its decision in memory on the one process Scribe outbox
once the decision is known, outside the operation's transaction, and never
waits for the write. The outbox admits a finite process capacity, by writes
(`scribe.outbox_capacity`, default 1,048,576, so bytes normally bind) and by accounted bytes
(`scribe.outbox_bytes`, default 1 GiB, counting at least every captured
payload and object a write holds and the Arrow memory its staged rows own,
with an immutable column every write shares resident once for the process),
covering queued, waiting, retrying, and in-flight writes. An audit decision or gateway capture staged past its share
of either is shed, not queued: it is counted in
`outbox_events_lost_total{outbox="scribe",reason="shed"}` and logged at most
once per tenant per second with the count shed, and the operation is neither
refused nor delayed. Its one background writer spreads each tenant's writes
over a few producer lanes, keeping one request's decisions and capture in one
lane and one Verifier execution's inputs and result in one lane, so an
execution's inputs commit no later than its result while different requests
and executions are written concurrently; queued submissions sharing an
idempotency key share a lane, so they are acknowledged in arrival order. As a
Kafka producer lingers, a lane's slice waits until its oldest write is 50 ms old
or it holds 4,096 writes or 32 MiB before it is submitted, and takes at most
32 MiB of the lane's backlog, so writes staged one per
request share one Scribe commit; producers never wait for it. It groups each lane's slice by
destination, builds each group's server-projected rows once into frames whose
batch ids derive from the slice's idempotency key, which every retry of the
slice keeps, and submits them to the local Scribe or over the mutually
authenticated peer plane to a Scribe pod, encoding Arrow frames only for the
peer plane. Only the local
Scribe takes those rows in-process as server-built canonical batches, whose
values are not revalidated; the trust follows from the in-process call, never
from a flag a caller can set. Peer and native Arrow IPC is a trust boundary
and is validated in full, and both paths keep tenant, schema, reserved-column,
managed-envelope, and admission checks. Producer lanes are not Scribe shards: Scribe routes each admitted
frame to its own shard and durability owner whichever lane submitted it. A retryable failure is
logged with its tenant, counted in `outbox_write_failures_total{outbox="scribe"}`,
and resubmitted as the identical slice, which Scribe's durable batch-id fence
absorbs when the earlier acknowledgement was lost. A terminal rejection is
logged, counted in `outbox_events_lost_total{outbox="scribe",reason="rejected"}`,
and consumed; a process that reaches no Scribe counts its writes with
`reason="unreachable"`. Writes Scribe has not acknowledged are lost on abrupt
process death or when graceful shutdown reaches its deadline; the shutdown loss
is counted with `reason="shutdown"`, and loss
never changes the originating permission decision. Scribe batch commits and
Forge maintenance transitions evaluate no permission: they are recorded as
lineage in `vala.scribe_batch_commits` and `vala.forge_operations` and emit no
audit event.

Oracle query admission, gateway invocation and administration, and direct
verification execution follow the same rule with no surface-specific
exception: each evaluates its permission synchronously before protected work
and stages its decision on the process outbox. No audit staging table,
publisher, surface-specific WAL, disk spool, durable queue, relay, or sink
exists.

Retained audit history lives in the tenant-qualified Bifrost
`bifrost.system.audit_log` table, written only through Scribe, never through Gate.
Audit frames are submitted under the platform audit principal, the one
principal Scribe admits for the reserved system owner's audit history. A row
carries only decision content (request, trace, operation, resource, audit Card,
the decision's verified subject and its kind, its actor, permission, outcome,
detail, and credential id) with
`created_at` set to the decision time; there is no chain, sequence, or
publication progress, and readers order by decision time. Because the outbox
writer evaluates no new permission, it appends no audit event and retained
history cannot feed itself. A legacy direct-Iceberg relay and
`platform.audit_log` are not alternate historical authorities.

Audit schemas minimize personal data. They store typed identities and decisions,
not credentials, raw prompts, request bodies, Source payloads, or unnecessary
personal attributes. Classification determines field retention and export
controls. Where a legal erasure obligation applies, encrypted supplemental
personal fields use tenant- or subject-scoped envelope keys that can be
destroyed while the minimal immutable decision event remains. Erasure is
itself audited and never rewrites the retained event.

## Cryptography and secret handling

- Approved algorithms, key sizes, libraries, and protocol versions are a
  closed deployment policy. Unknown or deprecated choices fail startup.
- TLS certificate verification is mandatory. Production does not expose a
  skip-verification switch.
- Secret-bearing Rust types use `SecretString` or an equivalent redacted
  wrapper and a redacted `Debug` implementation.
- Credentials are not accepted through command arguments or checked-in files.
  File-mounted secrets require restrictive permissions and atomic replacement.
- Cryptographic randomness comes from the operating system. Identifiers and
  nonces are never derived from timestamps alone.

## Security operations

Security events include credential issuance and revocation, token replay,
unknown signing keys, policy unavailability, repeated authorization denial,
peer context refusal, tenant-tripwire failure, Scribe outbox write failure or
counted loss, SSRF rejection, secret-resolution failure, and privileged
operator use.

Each event has a bounded-cardinality metric, structured redacted log, trace
correlation, and audit record when an authoritative tenant is available.
Alerting and response ownership are configured before a deployment becomes
ready. The incident procedure in `operations/reliability-and-recovery.md`
governs containment, evidence preservation, credential rotation, recovery, and
post-incident verification.

## Standards alignment

| Area | Standard or pattern |
|---|---|
| Zero trust and identity | NIST SP 800-207 |
| Access control | NIST SP 800-53 AC family; OWASP ASVS V4 |
| Audit integrity | NIST SP 800-53 AU-9/AU-10; ISO 27001 A.8.15 |
| Delegation and federation | RFC 8693; RFC 7523; OpenID Connect |
| Provenance | SLSA; in-toto; EU AI Act Article 12; NIST AI RMF |

Standards alignment does not substitute for deployment-specific threat
modeling, control evidence, or qualification.
