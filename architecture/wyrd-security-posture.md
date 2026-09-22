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
  tenant binding, fencing, and audit failures deny the operation.
- Secrets are resolved from a deployment secret provider at runtime. They are
  never Card fields, generated artifacts, logs, traces, errors, or audit
  payloads.
- Authentication establishes who may call a Wyrd route. Policy governs Card
  state and cross-service invocation. These are separate decision planes.

## Trust boundaries

| Boundary | Required control |
|---|---|
| Client or agent to gateway | TLS, verified Wyrd token, typed permission, request bounds |
| Gateway to `wyrd-server` | Authenticated transport; original credentials preserved; gateway metadata is never tenant authority |
| `wyrd-server` replica to replica | Mutually authenticated TLS plus a purpose-bound, signed, expiring peer ticket |
| Application to Postgres | Role-separated DSNs; RLS for tenant traffic; privileged pools excluded from handlers by construction |
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
`global_admin`, `tenant_admin`, `user`, `service`, and `agent`. A platform
principal is `global_admin` or `user`; a tenant principal is `tenant_admin`,
`user`, `service`, or `agent`. The runtime `Principal { id, kind, tenant_id,
roles, effective_permissions, credential_id }` carries the tenant-plane
`PrincipalKind`, and `PlatformPrincipal { id, kind, effective_permissions,
credential_id }` the tenantless platform identity.

Card binding is a property of some machine principals, not a precondition for
holding a credential. An `agent` principal always binds an Agent Card. A
`service` principal binds a Service Card when it is a deployed workload and
binds none when it is tenant automation created by a tenant administrator; a
Card-free machine principal therefore carries no emit scope. `tenant_admin`,
`global_admin`, and `user` never bind a Card — an administrative or human
identity is not a registered AI-system component.

Card-bound identities are provisioned idempotently by tenant, principal kind,
Card kind, and Card UID. Re-applying a Card preserves the principal identity.
Credential issuance is a separate privileged operation and is policy-gated.

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
- Plaintext is returned once. The server stores only a memory-hard password
  hash and non-secret lookup metadata.
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
- Refresh tokens are stored by one-way digest, rotated on every successful
  use, and invalidated when replay is detected. Reuse of a rotated refresh
  token revokes its token family and emits a security audit event.
- Every tenant access token carries issuer, audience `wyrd`, subject,
  issued-at, expiry, unique token identity, principal, tenant, Card scope,
  credential attribution, delegation chain, informational roles, and one
  `permissions` set. The `permissions` claim is the only tenant authority; one
  `TenantTokenIssuer` resolves it from current grants at issuance for API-key
  exchange, OIDC login, human refresh, workload `jwt-bearer`, and delegation.
  Verification is local and synchronous — signature, issuer, audience, expiry —
  and reads no store, holds no cache, and resolves no roles.
- Revoking a credential, suspending or deleting a principal, suspending a
  tenant, or changing grants refuses the next issuance immediately. A tenant
  token already issued keeps its snapshot authority until its five-minute
  expiry; there is no revocation list or authorization epoch. The platform
  plane instead revalidates current state on every request.
- Bearer access tokens remain replayable until expiry. TLS, short lifetime,
  token-family replay detection, least privilege, and audit are the required
  replay controls. Logs and traces never record bearer material.

### Delegation and federation

- Cross-service delegation uses RFC 8693 token exchange: the actor presents
  the subject's token as `subject_token` and its own as `actor_token`, the
  invoke policy must allow the pair, and the signed token names the subject as
  its principal and the actor as its outer `act`. Both are verified from that
  token; no caller-supplied identity header is accepted, and `act` confers no
  authority.
- A delegated token is bound to the `wyrd` or `bifrost` audience; a `bifrost`
  token is refused outside the Bifrost ingest and query surfaces.
- Delegation depth is bounded, every hop is authorized, and the effective
  permission set is the intersection of the actor's current permissions and
  the subject's, so it can only narrow.
- External federation accepts tokens only from an explicitly configured
  issuer, audience, algorithm, and claim mapping. OIDC discovery does not make
  an issuer trusted.
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

`POST /v1/authz/check` is the policy decision point for cross-service invokes.
It requires a valid delegated Wyrd token and denies when the policy engine,
policy inputs, Card state, or audit path is unavailable. An enforcement point
may cache a decision only when the cache key includes the complete verified
principal, delegation chain, target, action, policy revision, and relevant
Card revisions. A cached allow expires no later than the token or policy
revision and is invalidated on either change.
There is no stale-allow mode.

Service-local policy may tighten organization policy and cannot override an
organization denial. Policy evaluation and the resulting allow or deny are
audited with request, principal, target, policy revision, and outcome identity.

### Production composition

Server construction injects the configured policy decision point, permission
resolver, and canonical audit writer as required
capabilities. A production profile cannot substitute a
permit-all policy evaluator, no-op audit sink, in-memory credential store,
test key, or best-effort background audit emitter. Missing or unhealthy
capabilities prevent the affected role from becoming ready. Dependency
injection exists to make the boundary testable, not to make a security control
optional.

## Peer identity and distributed Oracle

Internal Oracle and Scribe RPCs use mutually authenticated TLS. Certificate
identity admits the peer transport; a signed peer ticket authorizes one exact
engine operation.

- Peer tickets are domain-separated from user JWTs and use independently
  managed keys.
- A ticket binds tenant, query, snapshot digest, fragment or request digest,
  retry epoch, source node, destination node, deadline, and fence.
- The receiver verifies signature, `kid`, audience, destination, expiry,
  replay identity, tenant, snapshot, fragment, and fence before decoding a
  physical plan or touching storage.
- Rotation follows publish-before-use and bounded-overlap semantics. A ticket
  never remains valid beyond its query deadline, so retired verification keys
  need only cover the maximum ticket lifetime and clock skew.
- Tickets are single-purpose and cannot be promoted into user credentials,
  database roles, generic internal authorization, or cross-tenant capability.
- A rejected ticket is audited under the verified tenant when safe; rejection
  before trusted tenant decoding uses `DataTenantId::SYSTEM_OWNER`. Unverified
  bytes never select an audit tenant.

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

- Tenant Postgres operations use `TenantConn` and transaction-local RLS state.
  A tenant caller cannot select or widen that state.
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

## Audit integrity and privacy

Audit cardinality follows authorization decisions, not HTTP requests and not
engine mechanics. Every decision that evaluates a principal's permission
appends its audit row in the same transaction that made it. Scribe batch
commits and Forge maintenance transitions evaluate no permission: they are
recorded as lineage in `vala.scribe_batch_commits` and `vala.forge_operations`
and emit no audit event.

Oracle query admission is the single durability exception: the serving process
fsyncs a versioned, CRC-framed local acceptance record before returning rows,
then a bounded at-least-once relay appends the canonical tenant
`vala.audit_staging` entry. Relay identity makes replay safe and observable.

`vala.audit_staging` is transient transactional write-ahead state with no
external consumer. Retained audit history lives in the
tenant-qualified Bifrost `vala.system.audit_log` table. A bounded publisher in a
process owning a local Scribe moves events idempotently into that table through
that Scribe, never through Gate. Progress is the monotonic per-tenant watermark
plus at most one frozen in-flight upper bound, which every competing or
restarted publisher reuses so the replayed range and its batch identity are
identical; rows appended above the bound wait for the next batch. One tenant
transaction advances the watermark, clears the matching bound, and
garbage-collects through the watermark; a replayed range is absorbed by Scribe's
durable batch-id dedup fence.
Because publication evaluates no new permission, it appends no audit event and
retained history cannot feed itself. A legacy direct-Iceberg relay and
`platform.audit_log` are not alternate historical authorities.

Audit schemas minimize personal data. They store typed identities and decisions,
not credentials, raw prompts, request bodies, Source payloads, or unnecessary
personal attributes. Classification determines field retention and export
controls. Where a legal erasure obligation applies, encrypted supplemental
personal fields use tenant- or subject-scoped envelope keys that can be
destroyed while the minimal immutable event, sequence, and hash-chain evidence
remain. Erasure is itself audited and never rewrites or silently breaks the
chain.

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
peer-ticket rejection, tenant-tripwire failure, Oracle audit commit failure, audit-chain or
publication failure, SSRF rejection, secret-resolution failure, and privileged
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
| Policy decision/enforcement | XACML PDP/PEP; Envoy `ext_authz` pattern |
| Provenance | SLSA; in-toto; EU AI Act Article 12; NIST AI RMF |

Standards alignment does not substitute for deployment-specific threat
modeling, control evidence, or qualification.
