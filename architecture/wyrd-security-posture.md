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
  tenant binding, and fencing failures deny the operation, except for the explicit
  bounded use of previously confirmed authorization state below. Audit is
  non-blocking: a decision that fails to commit is logged and counted and does
  not deny the operation.
- Secrets are resolved from a deployment secret provider at runtime. They are
  never Card fields, generated artifacts, logs, traces, errors, or audit
  payloads.

## Trust boundaries

| Boundary | Required control |
|---|---|
| Client or agent to gateway | TLS, verified Wyrd token, typed permission, request bounds |
| Gateway to `wyrd-server` | Authenticated transport; original credentials preserved; gateway metadata is never tenant authority |
| `wyrd-server` replica to replica | Mutual TLS from one dedicated cluster CA with the fixed `wyrd-peer` identity; the receiver checks every operation's context against its own state |
| Application to Postgres | Tenant traffic runs under RLS through `TenantConn` or the named single-statement owners; the operator pool is excluded from handlers by construction. Operators supply the logins; boot refuses superuser and `BYPASSRLS` logins. Production uses a separate tenant login that does not own Wyrd's objects, so RLS confines it even if compromised. One ordinary login (local and small self-hosted) keeps RLS per pool, but a process compromise could set the operator flag and reach every tenant |
| Application to object storage | Workload identity or short-lived credentials; tenant-qualified prefixes; encryption in transit and at rest |
| GitHub to `wyrd-server` | Webhooks are accepted only with a valid `X-Hub-Signature-256` HMAC over the raw body under the deployment's webhook secret, and name a tenant only through the one SECURITY DEFINER installation-to-tenant lookup; setup and callback parameters are hints that never confer tenant rights until GitHub confirms the installation and the authorizing user's access |

## Principal and credential lifecycle

Wyrd has two administration planes, and an identity belongs to exactly one.

The **platform plane** operates the deployment: tenant lifecycle and recovery
of a tenant's administration. Its principals live in `platform.principals`,
carry no tenant, and are never implicitly authorized over a tenant's
resources. The **tenant plane** operates one tenant's resources. Its
principals live under `wyrd.*` behind RLS and can never reach the platform
plane. Neither plane's credential, token or browser session is accepted by the
other. The human platform console is headed **Platform admin**; tenant human
administrators remain **workspace admin**. Platform-user passwords and console
sessions belong to existing tenantless platform principals, never tenant Users.

`PrincipalKindTag` is the closed wire set shared by both planes:
`global_admin`, `tenant_admin`, `user`, `service`, and `system`. A
platform principal is `global_admin` or `user`; a tenant principal is
`tenant_admin`, `user`, `service`, or the internal-only `system`. Agent is a
Card kind, not a principal kind. The runtime `Principal { id, kind,
tenant_id, effective_permissions, credential_id, card_access,
workload_uid }` carries the tenant-plane `PrincipalKind`, and
`PlatformPrincipal { id, kind, effective_permissions, credential_id }` the
tenantless platform identity.

Managed tenant accounts are User and Service; administrative access is a role,
not another assignable account type. Existing bootstrap `tenant_admin` and internal
`system` mechanics remain separate from account management. No principal identity
contains a Card version. A `service` principal is a stable account an
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
account atomically with its scoped role bindings, optional `workload_restriction`, and first
credential, returned once; failure creates nothing. A restriction is
`{"mode":"unrestricted"}` (the default) or
`{"mode":"family","kind":"Service","name":...}`. A family restriction
limits Card-attributed emission to that family's roots and
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
environment and derived identity labels grant no authority. Roles contain typed
operations; bindings assign them to User or Service principals, or optional local
groups, at explicit scopes. Tenant admins manage these assignments through the
same server contracts used by HTTP/CLI, SDK/MCP and Settings → Access. Assignment
sources distinguish direct, group and provider authority; login replaces only
its provider's assignments. Changes affect existing sessions and tokens through
current cached grants, not only the next login or token issuance.
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

A PR-wide Wyrd fresh-review request requires the authenticated caller's current
`cards:read`, `verifier:read` and `verifier:run` for every selected PR binding,
plus an explicit
write scope on MCP. The caller supplies only provider host, repository id, PR
number and a request idempotency key, never tenant, member/policy/source pins or
execution authority. Admission retains the selected declaration's accountable
authority, including its refusal behavior, and never substitutes the requesting
caller or SYSTEM when that authority cannot act. Retry does not bypass current
read/action authorization or reactivate obsolete work; membership/counts apply
the retained Repository parent boundary and sensitive-content checks before disclosure.

A Verifier run is accepted under one authority that execution never widens.
Queued work is activated by a binding schedule, a caller's `submit`, or the
selected Repository's PR admission, including the explicit fresh-review request.
A submitted run keeps the caller's verified effective principal and
delegation chain; its inputs row is staged on the Scribe outbox, and only
after Scribe acknowledges that row is the submission handed to the queue
writer, so an unacknowledged row queues nothing. A scheduled run keeps its
binding owner's authority, recovered from current grants in the run's tenant
when the run is accepted; a missing, inactive, or foreign owner, or a tenant
without SYSTEM, accepts no run and nothing falls back to another identity or
SYSTEM. The accepted authority is stored server-side with no credential,
survives claim, retry, and restart as identity, attribution and a frozen permission
ceiling. Later grants cannot widen it; every protected downstream action checks
current account and delegation-participant grants within that accepted ceiling. A tool call that an Agent's `code.run` Python makes through
`call_tool` is such an action: it uses the same accepted authority and the
same exact `tool:invoke` and capability checks and non-blocking audit as the
model's direct call, and nothing in the program can select or widen the
identity, tenant, delegation, or credentials it runs under. The bridge
refuses `code.run`, `fs.write`, and `publish`, so a program cannot recurse
or publish, and writes only its private `/work` through the interpreter's
parent-mediated mount.

Publishing deferred evidence adds no permission and no authority to the
execution that consumes it. The publisher needs current `verifier:run` on
each subscribing Verifier and its subject; a subscriber it may not run is
skipped, and none refuses the publish before anything is retained. The
repository must be one the tenant's connected installation reaches, resolved
by the server from `owner/name`; the commit, branch, and any other Git
metadata grant nothing. A parked execution resumes under its own accepted
authority, never the publisher's, and losing repository access cancels it
`repository_access_lost`.

Deciding a pending approval requires current `verifier:approve` on the exact
Verifier and read access to the execution's retained authorization parent.
`verifier:run` and `verifier:read` do not confer approval. The `approver` bundle
allows read/decision at its applicable scope; it does not implicitly make another
parent, such as a Repository, readable. `admin` covers tenant approvals; `editor`,
`workload` and `viewer` do not. Each check is audited under
`verification.approval.decide` with the execution as resource. The decider is
recorded as decision actor and adds no authority to the resumed execution.

A tenant administrator is created once, during tenant provisioning, and is the
tenant's headless root of trust: it holds credentials and roles, federates no
identity by itself, and coexists with any human administrator the tenant later
adds. A platform administrator is created once, during deployment
initialization, and administration of the platform plane is available through
its credential whether or not a platform OIDC connection exists. This bootstrap/global
administrative key is CLI/API break-glass only and never enters a browser or Node.
Human console operators use pre-registered platform `user` principals with the
platform OIDC connection, or platform-user passwords for installs without an IdP.
Password verifiers/revisions belong to the platform credential owner and share
the tenant password validator (15–128 NFC-normalized Unicode characters,
blocklist, Argon2id and bounded blocking/admission); they confer no tenant rights.

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
- A platform API token session names the credential that minted it and re-reads that
  credential on every request, so revoking a platform credential ends its live
  sessions on the next request. A platform session
  established by federated login names no credential; the principal itself is
  the anchor, and suspending it ends those sessions on the next request.
- API keys have an expiry, owner, creation audit event, use metadata, and
  revocation state. Rotation creates a new key, changes deployment secrets,
  verifies token exchange, and then revokes the old key.
- The deployment secret provider, never a Card, supplies `WYRD_API_KEY` to an
  SDK or workload.
- A credential has exactly one tenant owner, a Service account or a User.
  Personal User keys carry a normalized name, a nonempty permission ceiling
  and an explicit expiry of at most 365 days. Exchange preserves the key's
  ceiling in the signed token; requests must satisfy both current grants and
  that ceiling, so a later grant never widens an old key. Restricted Service
  keys use the same typed ceiling model. `personal_credentials:read` lists and
  `personal_credentials:write` revokes only the caller's own keys, both
  derived from the verified caller rather than a submitted principal;
  Service principals hold neither. Creation additionally requires a browser
  or SSO session created by a human login within five minutes; API-key
  recovery is not such proof. A restricted key needs the matching ceiling to
  list or revoke. Credential lists page at 50 by default, at most 100, with an
  opaque cursor. Other-User issuance still needs tenant administration.

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
- Tenant access tokens carry `version`, `iss`, `aud`, `sub`, `principal`
  (account ID, kind and tenant), `iat`, `exp`, `jti`, required `card_access`, and
  conditional `cid`, `act` and `workload_uid` under their existing validation rules. They
  carry neither authoritative `permissions` nor informational `roles`.
  `TenantTokenIssuer` uses the same claim contract for local/OIDC human login,
  API-key exchange, refresh, workload federation and delegation.
- `permission_ceiling` is an optional signed typed PermissionSet, never a grant.
  Ordinary human login and unrestricted Service-key tokens have no additional
  ceiling. Restricted-key and delegated tokens retain their applicable ceilings.
  Absence adds no restriction; an empty set permits nothing; invalid claims are
  refused. Account grants remain server-owned and current.
- Cryptographic verification is local, synchronous and store-free: signature,
  issuer, audience, expiry and claim validation. Before permission checks, every
  tenant ingress attaches already-resolved current authority from the server's
  authorization snapshot. Successful signature verification alone grants no
  resource access. There is one token contract, not a legacy-claims fallback.
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
- Dedicated execution status/progress/publication reads authorize and audit current
  read access on the trusted retained parent, with sensitive-content checks.
  Repository-owned data needs no separate referenced-Verifier read grant.
  Execution reads use accepted/recovered Bifrost state through fixed
  tenant/execution-scoped statements under the same narrow SYSTEM read authority for
  registered `bifrost.verification.execution_inputs`, `results`, `execution_outputs` and
  `execution_events`. Retained identity and full metadata do not require a leftover
  execution row or fetching a deleted Card. Historical accepted authority is provenance,
  not a grant to the requesting caller. Ordinary public Bifrost SQL retains caller source and managed-row
  permissions; no caller SQL runs under internal authority.
  Accepted publication mounts/download/local-content select the exact private
  verified File association retained in run state from acceptance onward and
  validate its tenant, producer, path, digest and length through existing storage
  owners. Do not rediscover by global digest or accept a caller storage path.
  Local publication bytes remain under execution-parent authorization, never the
  cards:read blob route. Preserve external File completion/inventory acknowledgement,
  original evidence retention and independent delivery/replay authority. There
  is no active-Postgres descriptor custody or later reference transfer. Required
  unavailable state is not absence and cannot authorize duplicate execution.
  Checkpoint acceptance preserves current execution authority and rejects stale
  writers, including during WAL recovery; historical authority remains provenance.
- Revoking a credential, suspending or deleting a principal, suspending a
  tenant, or changing grants refuses the next issuance immediately. A tenant
  token already issued uses current cached account authority under the freshness
  bounds below; its signature is not a frozen grant. Credential revocation ends
  renewal; it does not itself withdraw an already-issued bearer. The platform
  plane revalidates current state on every request. Revoking a
  principal also cancels, in the same commit, every unfinished Verifier
  execution it submitted or accepted, so queued work never outlives the
  authority it was accepted under. A running claim observes that cancellation
  through its lease-renewal listener, stops further engine work and settles
  nothing after losing its claim; previously issued IO can still finish.
  Revocation itself never waits for Bifrost.
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
- Device approval accepts an allowed password, company OIDC or social login with
  explicit user-code approval. Approved device records and their refresh families
  retain exactly one anchor: local User/password revision, complete company OIDC
  connection ID/revision, or social provider ID/revision and linked-identity
  reference, together with the CLI client binding. Unapproved records
  carry no authenticated principal or anchor. Refresh checks the original anchor
  and current User, then rotates; an invalid anchor refuses renewal rather than
  switching methods. Local CLI login needs no IdP or synthetic connection. Tokens
  are minted on one-use device-code redemption, never stored awaiting pickup.
  Existing authorization-code records retain their history
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
- Sign-in start rate limiting is an ingress concern. Each unauthenticated
  start below records one short-lived pending-login row before any proof, so
  deployments rate-limit them per client address with an aggregate ceiling,
  and the server is not reachable around the ingress: `POST /auth/device`,
  the user-code entry form (RFC 8628 §5.1); `POST /auth/browser/login`, for
  company SSO, social and invitation sign-in; and `/auth/platform/login`,
  the platform OIDC sign-in start. Signup and password sign-in carry their own admission windows.
- Recoverable auth-provider and workload-issuer client secrets, shared
  social-provider client secrets, application SMTP credentials and the signup
  challenge secret follow [stored secrets](#stored-secrets). Signup and sign-in
  flow cookies, email proofs and keyed pseudonyms (admission and creator keys)
  derive per-use keys from one random
  platform derivation key, stored only as a `WyrdKms` envelope; wrapping-key
  rotation re-wraps it and never changes it. Wyrd stores no recoverable access token,
  refresh token, authorization code, device code, or API key.
- `wyrd init` keeps local database URLs and the tenant key only in the
  protected `credentials.toml`; `config.toml` `[dev]` holds non-secret
  settings. Gateway documents name provider keys as `local_env` sources the
  CLI resolves at apply time, and the server applies a composite document
  atomically, so a refused document commits nothing.

### Social admission, account linking and application mail

These capabilities ship in open source through one self-hosted/SaaS code path.
Users remain tenant-local: the same person in two tenants has two independent
Users. Platform administrators manage deployment-wide Google and GitHub.com
clients, password availability, public local/social signup and application SMTP through
Rust management APIs and their UI projections. Tenant administrators manage only
their own company SSO connection and tenant login policy. Shared social providers
sit beside the single Active company connection, not in its slot; platform SSO
continues to authenticate platform administrators only. Platform authority grants
no implicit tenant administration.

Operator onboarding shows provider registration instructions and the exact callback
URL, accepts sealed client credentials, tests sign-in and enables the tested
configuration. Stored secrets are omitted from read responses. Password is enabled
by default; public local/social signup is disabled by default. SaaS enables signup via
the same operator setting. Rust offers and enforces current allowed methods. Private defaults include no
SMTP/provider credentials. Local acquisition requires password/signup enabled and
SMTP; social acquisition requires an enabled provider and verified contact. The
methods response distinguishes returning/invitation providers from usable creation
providers, and advertises local signup, email discovery and password reset only
when usable. Every root/login/invitation/device UI uses that response; Rust rechecks
current policy at start and completion, including racing settings changes.

Rust binds each provider transaction to its purpose (returning login, invitation,
new-tenant signup or linking), exact provider revision, browser, trusted origin,
target tenant where present and relevant invitation/User. Retain five-minute
expiry, one-use completion, server-held state/PKCE, OIDC nonce and fixed completion
destinations. Social callbacks are provider-specific, separate from the shared
company callback. Disablement or revision change refuses in-flight completion
and invalidates credentials anchored to that provider revision.

Google uses OIDC with `openid email profile` and validated issuer, subject,
audience, signature, nonce and expiry. GitHub.com uses authorization code with
S256 PKCE and an authenticated user lookup, with numeric user ID under a fixed
GitHub.com namespace. It supplies no OIDC ID token. Request no repository or
organization access. For public acquisition only, GitHub may request `user:email`
and fetch authenticated email records to prove a verified contact. Accept Google's
`email_verified` only after OIDC validation; absent provider contact proof, require
a separate Wyrd email challenge bound to the social attempt. Returning login and
invitation acceptance have no added email-verification prerequisite. Provider tokens are transient identity-resolution
inputs; retain no provider refresh-token vault.

Returning social login resolves only tenant + issuer/provider namespace + subject.
An unknown identity, inactive User or inactive tenant refuses without User
creation. Email never selects, links or merges accounts and may repeat. Social
login grants no role and does not clear company, direct or local-group bindings.
Company SSO retains its existing provisioning and source-qualified group mapping.

Invitations name a recipient and explicit scoped assignments. Individual and bulk
entry create separate expiring, one-use invitations with per-recipient outcomes
and pending/resend/revoke operations. Each invitation is a link carrying its
one-time code in the URL fragment, returned once to the administrator and, with SMTP, emailed; Wyrd
stores only a verifier. Resend replaces the code. Possession of the live link
admits one enrollment by any allowed method; the address never selects, links or
identifies an existing User. Opening the link consumes nothing. A forwarded
live link admits its holder; single use, expiry and revoke bound that risk. Consumption revalidates issuer authority,
roles/scopes, tenant policy and provider revision, then atomically creates the
User, identity/credentials, bindings and session and consumes the invitation, or
commits none. Concurrent redemption has one winner. An identity already owned by
a User refuses new-User enrollment and directs the person to sign in and request
access through ordinary binding administration.

Enabled public local/social signup proves a verified contact and its creation identity, takes a new workspace name/slug
and invokes the existing tenant provisioner with a human-owner initialization
path. It creates a new tenant and a tenant-local User with tenant-wide admin
binding. It never joins an existing tenant by email, domain or slug, grants the
visitor platform authority, fabricates a platform caller or returns a bootstrap
administrative key. Preserve claim/initialize/promote lifecycle across platform
and tenant transactions; promote only when the tenant and administrator are usable.
Bind retry ownership to the creation attempt and proven identity; another caller
cannot adopt a failed claim, replay cannot create a second tenant and failure
leaves no active ownerless tenant. A lost session response permits ordinary login
to the completed tenant. Signup remains enabled at completion. Shared admission
limits trusted source and verified provider identity before expensive creation;
unavailable admission state refuses creation.

Signup stores no pending User/tenant/attempt before email/provider proof
(bounded shared admission counters remain permitted): the round-trip state is a sealed,
short-lived, HttpOnly, path-scoped cookie, and the recorded proof is unique per
state, so one callback yields at most one proof. Starting signup has its own
shared source window, checked before provider work. Throttles answer `429` with
`Retry-After` in whole seconds equal to the problem retry field. Admission keys and a signup tenant's creator key are HMAC-SHA-256
pseudonyms, keyed from the `WyrdKms`-wrapped platform derivation key; no plain issuer, subject
or address is stored there. Expired windows and attempts are purged in bounded
background batches; a completed attempt keeps no email. An operator may
configure a Turnstile challenge on browser signup completion, validated
server-side for success, hostname and `signup` action after admission and before
a tenant is claimed; refused tokens use `WYRD_AUTH_403_SIGNUP_CHALLENGE`. An
unreachable challenge provider refuses creation, and SDK, CLI, MCP and API
traffic is never challenged. One proven identity may hold at most an
operator-configured number (default 3) of non-retired signup-created tenants,
checked atomically with the claim and refused with
`WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT`. Only the browser holding a proven
attempt's completion cookie may restart its stale interrupted creation after the
existing claim-staleness bound, and only before proof expiry. A failed/provisioning
claim counts until retired; operator creation neither counts nor is refused.
The creator HMAC survives attempt purge: verified email namespace for local creation,
provider namespace/subject for social. This is per identity, not per human across
methods; never unify social identities through email.

A request's trusted source is its TCP peer, unless that peer is inside the
deployment-configured trusted ingress IP/CIDR list; then it is the client
address from the configured header (for example `CF-Connecting-IP`). Forwarding
headers from other peers are ignored, IPv4-mapped addresses are normalized and
IPv6 sources are admitted by `/64`. Signup and local password admission derive
the source this way, each with its own windows. Without a determinable source,
signup refuses.

Linking requires fresh proof of the User's current allowed method within five
minutes plus proof of the new identity in the same bound transaction. An old
cookie or recovery session alone is insufficient. Attach only to that same User;
refuse another User's identity, with one winner under concurrency. Same-User
repeat linking may succeed idempotently. Preserve principal ID, grants, groups
and keys; never merge existing duplicates. Linking company SSO applies only that
connection's validated group assignments. Adding a password to a social-only
User requires fresh current-method proof and existing password validation.
Removing a method requires fresh proof through a remaining allowed method,
refuses the last usable method and invalidates credentials anchored to the
removed identity. This does not prevent operator-wide provider/password disablement;
existing administrative recovery and last-admin safeguards remain applicable.

A freshly company-SSO-authenticated tenant administrator may require company SSO
only with a tested Active connection, after warning about unlinked accounts.
Then password/social starts, callbacks, existing browser sessions and CLI renewal
refuse under established freshness rules. Policy remains enabled across company
connection replacement, disablement or outage: no password/social fallback.
Replacement retains connection-revision invalidation and issuer/subject rules.
Disabling policy does not resurrect revoked sessions. Preserve personal/Service
API-key policy and independent operator-assisted recovery. This is not continuous
IdP offboarding; upstream removal alone does not revoke Wyrd grants or sessions.

One operator-configured SMTP relay supplies host, port, TLS mode, optional sealed
credentials and sender address, with test-send. Tenant administrators use it but
cannot read/change its credentials. Wyrd runs without SMTP; administrators
then copy invitation links and the UI says they are not emailed. Send separately after
each invitation commits, outside the tenant transaction. Report relay acceptance
or failure per recipient, never inbox delivery; failure leaves a pending invitation
and no active User, with explicit resend available. Raw invitation secrets are not
stored as a PostgreSQL mail spool. No new mail queue, automatic retry subsystem or
provider-specific integration is required; Resend is simply a compatible-provider
example. Provider account and sending-domain setup remain external operator work.


Local public creation proves email before collecting username/password. Use the
existing provisioner with local or external human-owner proof, activating only a
usable admin/workspace and issuing the appropriate ordinary tenant session.
Successful completion retry returns the same tenant, not another creation. Setup
requires a named local human admin before claim, using a hidden password prompt
or stdin; optional email remains unverified. Keep separate automation/recovery
credentials and safe reruns without password reset/duplicate admin. Machine-only
administrative tenant creation may omit human-admin input. Invitation links remain
bearer enrollment authority, never verified-contact proof.

Authenticated fresh allowed-method contact verification is tenant/User/purpose-bound;
it atomically replaces contact and revision plus directory-update intent, notifies
the old verified address and invalidates old-contact reset proofs. Duplicate
addresses remain permitted within/across tenants, distinct from profile or
invitation metadata. Normalize whitespace/domain, preserve local-part case and
original delivery address; never collapse dots, plus tags or aliases.

With SMTP, browser-bound verified-email discovery returns bounded deduplicated
active workspace login links only. Request responses are generic; proof mail names
no workspace. The platform HMAC email → tenant/User/contact-revision projection is
control-plane metadata, not a global person, authorization or identity store.
Tenant contact transactions advance revisions and notify existing reconciliation;
authorized tenant reads update/remove the projection. Before disclosure, revalidate
current verified contact, live User and live tenant through their owners. Stale,
revoked, disabled, retired or unavailable validation reveals nothing; eventual lag
may omit new links. No usernames/principal IDs, session, User, grant or account
link is returned/created. Selecting a link requires ordinary tenant authentication.
Without SMTP discovery is absent; saved URLs, invitations and live-session
metadata enumeration remain independent alternatives.

Email proofs use sealed authenticated encrypted envelopes with random identifier,
canonical address, purpose, deadline and initiating-browser binding, never password.
No pre-proof pending signup row is stored. After proof retain unique identifier
digests/spent-proof metadata until expiry for one-use consumption across replicas;
a lost response resumes only in the bound browser. Email proofs last fifteen
minutes; signup completion and discovery-read cookies five minutes. Purposes for
signup, discovery, contact and reset never substitute. Persisted deadlines use
PostgreSQL time; envelopes use the PostgreSQL-anchored clock. Links carry proof in
fragments, not query strings; opening consumes nothing, page code POSTs proof and
clears the fragment. Proof/result pages and responses are `no-store` and retain
trusted Origin, CSRF and host-only Secure/HttpOnly/SameSite protections.

Password reset request takes tenant + username and sends only to an active eligible
password User's verified contact. Known/unknown/unverified/ineligible requests have
generic `202` bodies and comparable timing distributions; recipient throttles and
SMTP outcomes cannot reveal account existence. Apply independent shared source and
address/account budgets, including resend, before SMTP/hashing for verification,
discovery and reset. Source throttles may return `429`; requests never lock accounts.
Reset uses existing fifteen-minute single-use code redemption with contact revision
binding, invalidates local sessions/CLI renewal, leaves keys alone, never signs in,
adds a password or bypasses SSO, and notifies the verified contact after success.
Codes remain manual input, never URLs. Admin codes preserve offline recovery.

ApplicationMail sends signup/contact/discovery/reset and reset-notification mail
after transactions, outside locks. Reset requests use bounded process-owned
transient sends on the same public response path for all accounts, with no durable
delivery guarantee; a crash may lose a send and users request again. Account-specific
relay failures are hidden; uniform deployment-wide `503` is permitted. Operator
test-send and invitations retain honest relay outcomes. No mail queue, raw-body
spool, automatic retry or provider-specific API is added. Removing SMTP disables
mail-dependent starts, not password login or valid issued reset redemption; signed
proofs retain their deadline if signup/method remain enabled. Never log addresses,
passwords, codes or proofs; use HMAC admission keys and existing bounded Argon2id
blocking strategy. These auth/control-plane operations add no verification,
gateway, Oracle or Scribe hot-path wait; audit delivery remains nonblocking.

### First-party browser sessions

- Rust owns the complete browser-session lifecycle in the existing auth and
  SQL owners. Node renders pages and forwards the selected plane's cookie
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
  bindings; signup pre-proof state may use an authenticated sealed HttpOnly cookie,
  never a plaintext PKCE verifier exposed to Node or script storage. Login
  transactions expire within five minutes. Post-login redirection is to the
  resolved tenant's fixed signed-in path, never an arbitrary return URL.
- Tenant session identifiers use cryptographic randomness and are regenerated at
  successful login. A host-only HttpOnly, SameSite=Lax cookie carries only
  that identifier, not OAuth tokens, API keys or identity/permission claims.
  Cookies are Secure in production; only the existing loopback-development
  HTTP exception is permitted. Raw identifiers never enter database rows,
  metadata, URLs, logs or traces. The record stores a one-way digest, tenant,
  principal, twelve-hour absolute expiry, revocation and exactly one anchor:
  the exact company-connection revision of an SSO session, the social
  provider ID/revision and linked-identity reference of a social session, the originating
  credential reference of a recovery session, or the local User's password
  revision of a password session. There is
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
- Local password login (`POST /auth/browser/password/login`), invitation
  signup, password change and code reset run in Rust. Passwords are 15–128
  Unicode characters after NFC normalization, blocklist-checked, stored only as an
  Argon2id verifier, and never logged, echoed or relayed through Node storage.
  One validator governs setup, direct/local creation, invitation, change, reset
  and password-add. No truncation, composition rules or periodic rotation; allow
  paste/password managers. Local signup requires completed email proof and creates
  a password-anchored session only after provisioning promotion; social signup
  creates its social anchor and no password. Email discovery creates no session.
  Ten failures per tenant and username, or one hundred per trusted source
  (see social admission), refuse
  further attempts with `429` and `Retry-After` until the window passes;
  unknown users and wrong passwords are indistinguishable. Invitation and
  reset codes are 256-bit, single-use, stored by SHA-256 verifier and bound to
  tenant and purpose. Change and reset advance the User's password revision,
  which ends every local session and invalidates local CLI renewal anchored to
  the old revision; API keys stay valid until revoked. Invitations hold explicit
  role-and-scope intents for the User created at acceptance, not bare role IDs.
  Consumption revalidates the issuer's current assignment authority, roles, scopes
  and tenant, and atomically creates the User and bindings or neither. Missing
  scope is refused, never promoted to All.
- Every browser-session request checks current tenant/principal authority.
  Company SSO sessions require the exact Active connection revision; social
  sessions require the enabled provider revision and live linked identity.
  Password sessions require the User's current password revision. These human
  method sessions obey current login policy and operator method availability. API-key-backed
  sessions require a still-valid originating credential and preserve all its
  effective-permission restrictions. Existing grant resolution, credential
  attribution, permission checks and non-blocking audit are reused; no Node
  role mapping, durable credential cache or fallback authority exists. Tenant
  signature verification remains local and synchronous; bearer and browser
  requests use the same cached current grants and restrictive ceilings.
- The existing protected tenant `/v1` HTTP APIs explicitly accept cookie
  sessions through their normal principal/permission boundary. Platform operations
  accept only their separately scoped platform browser session described below,
  never a tenant cookie. MCP, OTLP, gateway and gRPC routes do not inherit cookie
  authentication.
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
  `Cache-Control: no-store`. The one browser-visible secret is a personal key
  shown once from its authenticated no-store POST action result; it is never
  part of the initial page data, history, storage, URLs, logs, telemetry,
  captures or cookies. Existing Wyrd error codes distinguish refused authority from
  unavailable storage; a database outage cannot become anonymous, cached or
  mock success. Cookie authentication is documented alongside bearer
  authentication in served OpenAPI. UI-only OAuth issuance/renewal is retired
  when its consumers move, without changing CLI/device, SDK, platform API tokens,
  workload or provider-test behavior. Existing credential-bearing UI cookies
  are cleared and require sign-in; there is no plaintext compatibility path.

#### Platform console browser sessions

The `/platform` console and `/platform/ui/*` pages use the existing Rust
first-party browser-session machinery extended to the platform plane. Reuse
`PlatformLogin` and the existing platform login wire contracts for the one
platform OIDC connection, including pre-registration, issuer/subject pinning,
state, PKCE, nonce and one-use callback proof. Unknown operators are refused,
never enrolled. MFA is the IdP's policy. Without an IdP, an authorized registered
platform `user` may authenticate with its platform-local password. No bootstrap
key browser login, new principal kind or parallel authentication engine exists.

Rust creates a fresh opaque identifier in a distinct host-only Secure, HttpOnly,
SameSite=Lax cookie with `Path=/platform`. Rust's platform SQL owner stores only
its digest, principal, exact password or connection revision anchor, creation and
last activity, revocation and recent-authentication time. Records are tenantless
in `platform`, with forced platform-owner RLS, unique digest lookup and bounded
expiry cleanup through existing auth maintenance. Reuse existing platform login
transaction storage for browser/session/principal binding and one-use login or
re-authentication purpose. No provider token, raw cookie, password or bootstrap
key is retained. PostgreSQL derives idle/absolute deadlines: 15 minutes idle and
eight hours absolute; activity never extends the latter.

Every request checks the live platform principal and original anchor; suspension,
password invalidation or connection replacement refuses the session. Logout
revokes the selected session. Node forwards only the selected platform cookie to
Rust platform operations and safe metadata back to pages. It neither exchanges
codes nor stores credentials. Tenant sessions are refused by platform handlers;
platform cookies are never forwarded to or accepted on tenant routes/enumeration
or other protocols. Cookie path scoping is additional to these server checks.
Origin/CSRF, fixed redirects, no-store, secret redaction, fail-closed storage and
nonblocking audit follow the shared browser machinery.

Destructive console actions, including retiring a workspace or changing provider,
SMTP or signup settings, require same-operator fresh password or platform OIDC
re-authentication within the existing five-minute human-proof window. OIDC requests
fresh IdP authentication; an existing Wyrd cookie alone cannot satisfy it. Stale,
tenant, different-operator or break-glass proof cannot authorize the mutation.
CLI/API credentials remain their separate authorization channel; the global key
is never sent to Node or a browser. No platform refresh-token flow is added.

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
  the ordinary permission check against current subject and actor grants and every
  inherited/signed ceiling. The `act` chain retains audit attribution and identifies
  participants whose current authority constrains delegated access.
- Delegation depth is bounded, every hop is authorized, and the effective
  signed ceiling is the exact intersection of actor and subject authority and
  inherited restrictions at exchange. Later checks also require current authority
  of every participant, so delegation can only narrow. If the complete intersection cannot
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
  hostname, or post-login chooser. A company-SSO entry resolves only that tenant's
  connection; social entries select the operator provider through the bound
  transaction described in social admission above. The deployment's single
  platform-scope connection resolves only
  platform principals pre-registered against an expected issuer and claim and
  pinned on `(issuer, subject)` at first login. An unknown platform subject is
  denied and never provisioned just in time, and the platform connection's
  absence or outage never blocks administration through the global
  administrative credential.
- Unknown issuers, unknown keys after one bounded refresh, unavailable JWKS,
  invalid claims, and ambiguous claim mappings fail authentication.
- Tenant-configured company connections share one human-login callback, so authorization responses are
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

### GitHub App association

A tenant's GitHub connection is an association, not a principal or a
credential. The deployment owns one GitHub.com App, using GitHub.com’s documented
installation and OAuth flow, configured from environment
or files with its private key, OAuth client secret and webhook secret held as
`SecretString`; none is ever accepted from TOML, a tenant, a Card or a
request. `wyrd.github_installations` holds at most one installation per
tenant and one tenant per installation and host, under forced RLS; it stores
no key or token. An administrator or principal with explicit GitHub-configuration
permission begins a one-use transaction in `auth_login_state` (`flow = 'github'`), bound
to that principal and, at setup, to an HttpOnly browser binding cookie. Only
the matching principal and browser can consume it, once, before it expires,
and only against the slot revision it captured, so a stale callback cannot
restore a released or replaced connection. The callback redeems the OAuth
code with PKCE for a transient user token that is used for the
installation-access check and dropped. Disconnect fences the tenant's GitHub
work before it releases the installation. Every operation authorizes and
audits through the ordinary permission path. Connection configuration pages,
installation inventory and all associated UI/HTTP routes enforce that permission
in Rust. Repository Card access confers no connection access; GitHub configuration
authority alone confers neither OIDC administration nor tenant data read. Retained
review reads need no GitHub call and survive disconnect under their Wyrd grants.

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

Every route declares one typed `Permission { resource, action, scope }`. The
verified identity selects one tenant; scopes never select or widen tenancy.
Server-owned current grants and every applicable restrictive ceiling must cover
the operation and its target. No role name, Card reference or signed identity
alone grants access. There is no external policy service or second checker.

### Request principal

The shared verifier checks token identity and signed restrictions synchronously
without stores. Server ingress resolves current account authority before building
the authenticated runtime principal and `Caller`; `Caller.data_tenant_id` comes
only from the verified identity. HTTP, MCP, gateway, OTLP, native Bifrost and peer
execution obey this boundary. A signature-only Gate context cannot authorize data.
Browser sessions retain Rust-owned authentication and use the same grant checker.

An optional `x-wyrd-workload-uid` selector chooses context, never identity or tenant.
An equal signed root is idempotent; a different root is refused as
`signed_workload_mismatch`. Rootless selection resolves under signed `card_access`
and the account's WorkloadRestriction; malformed/repeated, unknown or foreign
roots are refused without activity, and unknown/foreign are indistinguishable.
Native Bifrost and OTLP gRPC perform the same server-owned selection before Gate.
WorkloadRestriction limits which Service family an account runs as; it grants no
operations. PermissionScope limits which target resources an operation reaches;
it never changes owning workload, subject, Verifier or principal attribution.

### Permission model

`Operation { resource, action }` uses closed typed Resource/Action enums, including
existing wildcard and AnyOf semantics. A Role stores operations, not object scopes.
A RoleBinding records its tenant, subject (User/Service ID and kind, or local
group), role, explicit PermissionScope and assignment source. The server compiles
bindings into `Permission { resource, action, scope }` values in PermissionSet.
Direct/group grants are additive and absence denies. There is no explicit deny,
policy language, mandatory group, ownership grant or nested/group-sync mechanism.
Provider mapping replaces only its own source-qualified bindings.

PermissionScope is a closed domain-tagged union:

- `All` covers the operation's objects within the verified tenant only.
- `Card(CardPermissionScope)` is `Family { kind, name }` or
  `Version { kind, name, uid }`, with tenant supplied by the verified context.
  Family covers existing/future versions and retained history; exact-version
  scope covers only its validated UID and never follows recreation. General Card
  version scope covers Verifiers; there is no separate Verifier-only scope.
- `Bifrost(BifrostPermissionScope)` retains logical catalog/schema or exact
  table UID scope for `bifrost_query:read`, including validated identifier segments.
- `Gateway(GatewayAccess)` retains deployment/model scope and explicit `granted`
  deployment semantics. Broad grants never erase required explicit grants.
- `GithubConnection(GithubConnectionId)` addresses connection configuration.
- `Tool(BuiltinTool)` addresses one exact built-in tool.

Bindings apply only to compatible operations at that scope or its applicable
descendants. Reject unknown, cross-tenant and wholly inapplicable assignments;
never promote missing or incompatible scope to All. Resource, action and scope
must each cover the requirement. Schema covers its tables, family covers its
versions, and an exact object covers itself, not a broader scope. Account-wide
All is tenant-limited. Malformed permissions/ceilings refuse authority.
`PermissionSet::covers_operation` is coarse route admission only when targets
are not yet known; it never substitutes for final target checks.

The existing `wyrd.auth_roles` owns typed operation bundles;
`wyrd.auth_role_bindings` is the common source of scoped assignments rather than
parallel User/Service grant stores. Optional `auth_groups` and
`auth_group_memberships` provide tenant-local rosters. These and their references
are tenant-qualified and protected by forced RLS. Bootstrap/internal authority
retains its existing owner. Authentication alone adds no grant; configured issuer
grants use the same binding model, without an implicit viewer fallback.

### Current authority and freshness

Each process holds an immutable tenant authorization snapshot and already-resolved
account views. Compile role/group bindings when first populated or meaningfully
changed, not on each request. Access-affecting mutations atomically advance the
tenant's `wyrd.auth_authorization_revisions` state and notify listeners. Background
confirmation checks revisions; unchanged revision renews freshness without
rebuilding grants. Healthy replicas enforce committed changes within five seconds.
A known newer revision must be refreshed before any allow.

A warm request performs local credential validation, a memory lookup and typed
containment checks, with no permission SQL or role/group expansion. A miss or
invalidation can acquire current permission state through the sanctioned reload.
Signature verification and the checker remain synchronous and store-free; server
hosting owns acquisition and refresh. Do not hold a lock across IO.

If database confirmation fails without known invalidation, a previously confirmed
snapshot may allow until at most five minutes after its last successful database
confirmation. Requests, failed refresh and token renewal never extend the bound.
Known invalidation, absent state or expired freshness refuses as unavailable,
without anonymous or token-snapshot fallback. Browser authentication and other
store-dependent operations may fail sooner. Long streams revalidate authority and
freshness before further output. No warm permission or managed-row check adds a
PostgreSQL, object-store, Vault or Scribe wait; audit stages on the process outbox.

### Permission check

PermissionCheck is the common synchronous, store-free authorization chokepoint
for User and Service. An allow requires that current grants contain the required
permission and every applicable credential, token, delegation and accepted-work
ceiling also contains it. Missing ceiling means no added restriction; an empty
ceiling permits nothing. An unavailable snapshot is refused before containment;
a valid snapshot lacking permission yields ordinary permission denial.
Audit, tracing and public error mapping remain at this shared boundary.

Delegation checks current subject and every actor's grants within all inherited
and signed ceilings. Accepted work serializes its frozen ceiling, identities and
attribution; retry/recovery cannot enlarge it with later grants or substitute
SYSTEM. Principal revocation retains its existing cancellation behavior.

### Resource and retained-data visibility

Capture a trusted exact authorization parent at acceptance: subject Card for
ordinary verification, Verifier for subjectless direct work, Repository for PR
work and its members. Retain exact/family identity with derived records after
queue cleanup or Card deletion. This is an access boundary, never a replacement
for the owning workload Card, workload_uid, subject, exact Verifier or actor chain.

Repository Card read covers its reviews, review sets, members and retained evidence
under sensitive-content controls, including for custom roles. Separate referenced
Verifier read or GitHub configuration is unnecessary. Opening another Card's
definition checks that Card independently. Starting work, authoring policy and
approval require their distinct operations; approval also requires parent read.

Point reads, downloads, lists, search, counts and pagination apply visibility
before returning or aggregating results. Unknown/inaccessible exact objects are
indistinguishable. Unprovable parents remain hidden even from admins. Public SQL
and internal reads exposing caller data obey the same managed-row boundary;
source, peer, query reuse or pagination cannot widen it. Ordinary user tables and
unattributed telemetry stay table-scoped; captured gateway payloads need separate
permission even in predicates, and audit history needs explicit tenant audit read.
See [Bifrost public surface](bifrost-design.md#public-surface) for source enforcement.

### Access administration

Settings → Access exposes Users, Service accounts, Roles and optional Groups
through existing Rust-owned account/credential workflows and common HTTP/CLI,
SDK/MCP contracts. Tenant admins administer access; preserve last-admin protection.
Service provisioning displays runtime family separately from operation grants;
select family name, never release version. Grant forms select subject, role and
explicit scope, show applicable operations and source, and explain family coverage
of future versions. Removing one binding does not remove inherited/broader grants.

Built-ins are read-only; custom roles select closed typed operations:

| Role | Operations within applicable scopes |
|---|---|
| viewer | Resource/derived reads and authorized queries; no audit or captured payload read |
| editor | Viewer plus authoring, binding management, execute/cancel; no approval/access administration |
| workload | Granted runtime definition/artifacts/context/evidence reads, execute, authorized ingest and gateway invocation; no authoring/approval/audit/payload read |
| approver | Read/decide on granted Verifiers where parent is readable; no authoring/execute or implicit different-parent read |
| auditor | Tenant audit-history read; no implicit ordinary data/payload access |
| admin | All tenant operations at tenant scope |

An authenticated user can copy their identity, requested action and supplied
link/identifier to request access through an existing communication channel. An
admin responds with an ordinary binding; current sessions benefit after normal
propagation. Help reveals no hidden target, owner, admin roster or installation
inventory. There is no persisted request queue, notification or message sending.

### Auth crate boundaries

```text
wyrd-spec         pure wire contracts for roles, bindings, scopes and permissions
wyrd-runtime      Principal, PermissionSet, PermissionCheck, builtin role behavior
wyrd-auth-issue   issuing keys, identity/restriction claims, API-key hashing, delegation depth
wyrd-auth-verify  synchronous store-free signature and claim verification
wyrd-sql          tenant-bound accounts, operations, bindings, groups, revisions and credentials
wyrd-auth         issuance, current authority acquisition and compiled account views
wyrd-server       ingress composition, credential writes, audit staging and error mapping
```

Shared auth shells remain SQL-free. Rust owns durable behavior; SDKs project the
wire contracts and Node renders/forwards. Existing owners are extended rather
than adding a policy service, parallel grant store or alternate checker.

### Production composition

Server construction injects the current-authority resolver and canonical audit
writer as required capabilities. A production profile cannot substitute an
in-memory credential store or test key. Missing capabilities prevent readiness;
subsequent authorization freshness follows the explicit bounds above. Audits use
the canonical nonblocking outbox. Dependency injection makes security testable,
not optional.

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
  table/object/snapshot, authorization revision and applicable row/ceiling boundary,
  query or assignment, target node and fence, deadline, protocol version, request body
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

## Outbound credentials and SSRF defense

Operator Cards carry a tenant Operator-connection name;
the server opens its Postgres credential through `WyrdKms` under the exact
tenant, provider, HTTP origin, and auth authority immediately before delivery.
Multi-tenant Operator credentials never use process environment selectors.
Secret values are redacted in diagnostics and are not persisted in Card specs
or observations. Rotation changes the connection secret without changing the
Card contract.

GitHub repository access uses installation tokens the server mints on demand
with an RS256 App JWT, narrowed to the one repository and the App's Contents
read, Pull requests write and Checks write permissions. A token is minted
only while the tenant's association is active, the repository is selected
and GitHub still grants it; it is never persisted, returned, or derived from
a user's personal token. Production uses `https://github.com` and
`https://api.github.com` through the screened HTTP client. Development and tests
may configure local protocol mocks.

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
- Recoverable stored secrets are isolated by independent, tenant-bound DEKs
  under the shared deployment KEK, as described under
  [stored secrets](#stored-secrets); platform secrets use a distinct platform
  scope, never an invented tenant. Disabling the shared KEK is deployment-wide containment,
  not tenant revocation.
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
scope. Every PostgreSQL table enables and forces RLS, including platform
tables. Genuine deployment-wide and pre-tenant records belong to the existing
platform plane and may omit `data_tenant_id`; they remain protected by narrowly
authorized platform operations and platform RLS policies. Do not fabricate a
tenant or give a signup visitor platform authority to store those records.
The only new cross-tenant lookup purpose is the bounded verified-email workspace
projection: HMAC canonical email, tenant ID, User reference and contact revision,
with no password, grants, sessions or evidence. It and pre-tenant proven-email /
one-use redemption metadata are platform-owner-only forced-RLS records. Tenant
verified contacts lead with `data_tenant_id`, force RLS and retain tenant-qualified
User references. Tenant mutations notify reconciliation; results always revalidate
current tenant/User/contact through authorized tenant owners before disclosure.
A stale projection cannot confer access or reveal a revoked association.
This exception is to the tenant-key requirement only, not forced RLS. Tenant
tables carry a
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
not credentials, raw prompts, request bodies, or unnecessary
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

### Stored secrets

- Every recoverable stored secret — tenant auth-provider and workload-issuer
  client secrets, the platform OIDC client secret, gateway `managed_secret`
  provider keys, Operator connection secrets, and any future owner's — is
  sealed and opened only through the shared `WyrdKms` facade (`wyrd-kms`),
  which server composition builds once. The facade generates a fresh
  OS-random 32-byte data-encryption key (DEK) per stored value, encrypts the
  value with AES-256-GCM under a fresh nonce, and has the configured
  deployment key-encryption key (KEK) wrap the DEK. Backends are file,
  explicit development memory, GCP KMS, AWS KMS, and Vault Transit; they differ
  only in how the DEK is wrapped and authenticated, never in the stored
  envelope. Remote providers retain the KEK and return plaintext DEKs only.
  Payload encryption and DEK wrapping both authenticate canonical context
  derived from verified authority and trusted row metadata: tenant or an
  explicit platform scope, purpose, owner identity, provider or credential
  authority, and secret version where the owner has one. An envelope copied
  into another tenant's, owner's, purpose's, or version's row does not open,
  and a refusal never tries another tenant or key source. The common envelope
  carries format and algorithm identifiers, a logical wrapping-key reference,
  the opaque backend-wrapped DEK, the payload nonce, and the ciphertext; the
  reference resolves only through the deployment's configured active and
  retained keys, so ciphertext cannot select an endpoint or key resource.
  Unknown formats, disallowed keys, wrong DEK lengths, and authentication or
  provider failures refuse; there is no legacy reader and no generated
  substitute key. Neither plaintext DEKs nor KEKs are stored in Postgres.
  Resource owners keep authorization, persistence, and domain lifecycle; no
  generic decryption API exists.
- One deployment KEK protects every tenant's DEKs. A compromise of that KEK
  with database ciphertext, or of a server identity authorized to use it, can
  expose every tenant's stored secrets; tenant binding prevents substitution,
  not use by a compromised server that supplies valid context. Independent
  tenant KMS revocation and tenant crypto-erasure of backup copies are not
  provided. File mode holds its KEK in process memory and offers no remote
  custody.
- Encryption renewal reseals an existing value under a fresh DEK; credential
  replacement seals a new value without opening the old one; routine
  wrapping-key rotation rewraps DEKs and keeps payload ciphertext; compromise
  recovery generates fresh DEKs and payload ciphertext. Retained keys stay
  readable until live, prepared, and backup dependencies are accounted for.
  Revocation uses the owner's existing disable or revoke lifecycle. None of
  these revokes a copied credential at its issuer or withdraws data an
  attacker already holds.

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
