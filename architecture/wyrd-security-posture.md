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
| Database to audit anchor | Separately held signing key and an immutable external checkpoint destination |

## Principal and credential lifecycle

`Principal { id, kind, tenant_id, roles, effective_permissions }` is the only
runtime identity. `PrincipalKind` is the closed set `User`, `Service`, and
`Agent`; Service and Agent principals carry a server-verified `card_ref`.

Card-bound identities are provisioned idempotently by tenant, principal kind,
Card kind, and Card UID. Re-applying a Card preserves the principal identity.
Credential issuance is a separate privileged operation and is policy-gated.

### API keys

- API keys are bootstrap credentials for token exchange, not request-session
  credentials.
- Plaintext is returned once. The server stores only a memory-hard password
  hash and non-secret lookup metadata.
- API-key lookup enters a tenant-scoped transaction before credential
  verification and returns one indistinguishable public error for every
  invalid-key condition.
- API keys have an expiry, owner, creation audit event, use metadata, and
  revocation state. Rotation creates a new key, changes deployment secrets,
  verifies token exchange, and then revokes the old key.
- The deployment secret provider, never a Card, supplies `WYRD_API_KEY` to an
  SDK or workload.

### Access and refresh tokens

- `/auth/token` derives tenant and principal identity from a verified API key
  or trusted token-exchange subject. Client-supplied tenant identity is
  rejected.
- Access tokens expire no later than 15 minutes after issuance. Privileged
  operations may require a shorter configured lifetime, but never a longer
  one.
- Refresh tokens are stored by one-way digest, rotated on every successful
  use, and invalidated when replay is detected. Reuse of a rotated refresh
  token revokes its token family and emits a security audit event.
- Every access token carries issuer, audience, subject, issued-at, expiry,
  unique token identity, principal, tenant, roles, permissions, and credential
  or principal authorization epoch. Verification rejects a token issued before
  the effective revocation epoch.
- Revoking a principal, API key, refresh family, role binding, or credential
  advances the applicable authorization epoch transactionally. Verifier and
  permission caches cannot outlive the earlier of token expiry or that epoch.
- Bearer access tokens remain replayable until expiry or epoch revocation.
  TLS, short lifetime, token-family replay detection, authorization epochs,
  least privilege, and audit are the required replay controls. Logs and traces
  never record bearer material.

### Delegation and federation

- Cross-service delegation uses the RFC 8693 `act` chain in the signed Wyrd
  token. Both caller and callee are verified from that token; no caller-
  supplied identity header is accepted.
- Delegation depth is bounded, every hop is authorized, and the effective
  permission set can only narrow.
- External federation accepts tokens only from an explicitly configured
  issuer, audience, algorithm, and claim mapping. OIDC discovery does not make
  an issuer trusted.
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
- Emergency compromise disables signing immediately, advances affected
  authorization epochs, removes the key from accepted verification state, and
  invokes the
  [credential-compromise runbook](operations/runbooks.md#credential-or-signing-key-compromise).
  Availability never justifies accepting a token whose key state is unknown.
- Key inventories, rotation results, and JWKS publication are observable
  without exposing private material.

## Authorization and policy

Every route declares one typed `Permission { resource, action }`. Verification
constructs the runtime principal, and permission resolution is scoped to that
principal's verified tenant.

`POST /v1/authz/check` is the policy decision point for cross-service invokes.
It requires a valid delegated Wyrd token and denies when the policy engine,
policy inputs, Card state, or audit path is unavailable. An enforcement point
may cache a decision only when the cache key includes the complete verified
principal, delegation chain, target, action, policy revision, and relevant
Card revisions. A cached allow expires no later than the token, policy
revision, or authorization epoch and is invalidated on any of those changes.
There is no stale-allow mode.

Service-local policy may tighten organization policy and cannot override an
organization denial. Policy evaluation and the resulting allow or deny are
audited with request, principal, target, policy revision, and outcome identity.

### Production composition

Server construction injects the configured policy decision point, permission
resolver, canonical audit writer, Oracle audit-WAL relay, and external anchor
publisher as required capabilities. A production profile cannot substitute a
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

`SourceAuth` and Operator HTTP authentication carry secret-provider keys or
environment-variable names, never secret bytes. The server resolves credentials
only for the selected tenant, adapter, and operation. Secret values are
redacted in diagnostics and are not persisted in Card specs or observations.
Rotation changes the provider value without changing the Card contract.

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

## Audit integrity, anchoring, and privacy

Audit cardinality follows auditable domain operations and independently
durable transitions, not HTTP requests. Every auditable Postgres transition
appends its audit row in the same transaction. Forge may append tenant-owned
events only through its tenant-bound, fence-checked `OperatorAudit`
capability.

Oracle query admission is the single durability exception: the serving process
fsyncs a versioned, CRC-framed local acceptance record before returning rows,
then a bounded at-least-once relay appends the canonical tenant outbox entry.
Relay identity makes replay safe and observable.

The canonical audit log uses a per-tenant sequence and hash chain and is
append-only. Database tamper evidence is extended across the database trust
boundary as follows:

- A checkpoint signs tenant, sequence range, previous checkpoint, head hash,
  schema version, and timestamp with an audit-anchoring key held outside the
  database.
- Checkpoints are written to object-lock/WORM storage or an independent
  transparency service. Anchor publication lag is a monitored production SLI.
- Backup, restore, export, and incident procedures verify the chain through the
  latest external checkpoint and report any unanchored suffix explicitly.
- The anchor key has independent access policy, rotation, recovery, and
  destruction records; database administrators cannot use it.

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
peer-ticket rejection, tenant-tripwire failure, audit relay lag, audit-chain or
anchor failure, SSRF rejection, secret-resolution failure, and privileged
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
| Audit integrity | NIST SP 800-53 AU-9/AU-10; ISO 27001 A.8.15; RFC 9162 pattern |
| Delegation and federation | RFC 8693; RFC 7523; OpenID Connect |
| Policy decision/enforcement | XACML PDP/PEP; Envoy `ext_authz` pattern |
| Provenance | SLSA; in-toto; EU AI Act Article 12; NIST AI RMF |

Standards alignment does not substitute for deployment-specific threat
modeling, control evidence, or qualification.
