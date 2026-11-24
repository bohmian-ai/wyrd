---
id: SPEC-authorization-model
revision: 12
status: approved
---

# Scoped authorization and tenant-local onboarding

## Problem and outcome

An administrator must be able to give a User or Service access to one Repository,
Model or other supported resource without granting access to the whole tenant.
The UI, HTTP, CLI, MCP, SDKs and raw Bifrost queries must agree about that access.
The original change replaces broad Card grants and table-only queries that
cannot express that boundary. Tasks 001 and 002 implement that approved scope.

This revises Wyrd's authorization system while retaining existing account,
credential and workload-attribution flows. Roles describe operations; bindings
assign those operations at a scope. The server caches resolved current grants.
Credentials and delegation can restrict those grants, never increase them.

For example, Alice can read Repository family payments and its review evidence,
but another Repository contributes no objects, rows or counts. A Service account
can run checkout v1 and v2 with the same identity and credential, while each
request retains its exact workload_uid attribution.

Nothing has shipped. This specifies the revised system directly; there is no
upgrade, existing-data conversion, token-reissue campaign or mixed-version rollout.

Revision 3 adds social signup and sign-in. An operator enables Google/GitHub;
Sam accepts an addressed invitation and receives its scoped access, or creates
a new workspace and becomes its administrator. A tenant can require company SSO.
Explicitly connecting/removing another sign-in method is deferred from launch
under revision 11; different methods can resolve to separate Users.
Each invitation is a single-use link the administrator can copy; when an
operator configures SMTP, Wyrd also emails it. No particular email provider is
required.

Revision 6 adds provider-free verified-email workspace creation, named human
setup, verified-email workspace discovery and self-service password reset. Private
defaults remain unchanged: password enabled, signup disabled, no SMTP/providers.
Each workspace still owns independent Users, credentials, grants and sessions.
The maintainer approved all eight onboarding decisions on 2026-11-21.

Revision 7 adds a platform-only human console session and resolves the inherited
operator-UI credential blocker. Operators sign in to **Platform admin** using the
existing platform OIDC connection or a platform-user password without an IdP;
workspace administrators remain **workspace admin**. The maintainer explicitly
approved this behavior and the foundations-first, build-once order on 2026-11-21.
Revision 11 narrows the remaining launch scope: complete carryover cycles 1–16
and the retained SSO/verification work, excluding deferred cycles 17–18.

Revision 8 moves this change's secrets and keyed values onto the shared
stored-secret encryption of
[SPEC-stored-secret-encryption](../stored-secret-encryption/spec.md) (revision 2).
That change deletes the deployment sealing keyring this spec relied on. The
sign-in and signup behavior here does not change; what changes is how the server
protects its secrets, and that the owned-workspace cap now survives a
wrapping-key rotation (REQ-019).

Revision 9 bounds the unauthenticated sign-in starts the same way signup is
bounded: each start is admitted by a per-source window before it records a
pending-login row, so a flood of starts records nothing beyond bounded
counters (REQ-020).

Revision 12 replaces the tenant Settings and platform console presentation with
a conventional admin console (REQ-021): a visible section list, one page pattern,
a tenant secrets inventory, and the identity fields every comparable console
shows (your name and email; each person's sign-in method and roles). It adds a
provider-sourced display name to Users, amended into the unshipped auth
migration, and read projections; no new workflow.

Agreement: [finalized onboarding](design/onboarding.md), [design notes](design/notes.md), the original three system scenarios
and two-task split, [Rust responsibilities and token flow](design/rust-model.md),
and [finalized social-login design](design/social-login.md), locked 2026-11-21;
[admin console design](design/admin-console.md) and its [mock](design/admin-mock/README.md), finalized 2026-11-24.
The Rust snippets illustrate the agreed responsibilities, not required private
function signatures, cache containers or module layouts.

## Required behavior

### REQ-001 — One scoped grant model for User and Service

User and Service remain the managed tenant principals. A role contains typed
operations from Wyrd's closed resource/action vocabulary. A role binding names
the tenant, principal or optional local group, role, explicit scope and assignment
source. Direct and group grants are additive; absence of a covering grant denies
the operation. A narrower binding does not subtract a broader grant.

Supported scopes are tenant-wide, Card family, exact Card version, existing
Bifrost schema/table, gateway deployment/model, GitHub connection and existing
exact built-in tool. Family identity is tenant + Card kind + name; family access
includes existing and future versions. Exact-version access follows its UID,
not a replacement version. Preserve existing explicit gateway-grant semantics.
The server validates operation/scope applicability and tenant references; unknown
or inapplicable authority never becomes a tenant-wide grant.

The same permission checker serves both principal kinds. Optional groups are
local rosters, not principals, resource owners or namespaces. Local/provider
assignment provenance remains distinguishable; provider login replaces only its
own assignments. A group is not required for any ordinary installation.

### REQ-002 — Preserve authentication and enable local CLI login

Local users, invitations/password recovery, browser sessions, API-key
exchange and Service accounts work without an IdP. Preserve existing optional
OIDC, workload federation and delegation, their trust validation and the separate
platform plane. Registering or declaring a Card creates no principal or grant.

Decision: extend the existing device flow, not introduce a password OAuth grant.
A human signs into Wyrd using a local password or optional configured OIDC and
explicitly approves the CLI's user code. The CLI receives the existing short-lived
access and rotating refresh tokens through one-use device-code redemption.

Store exactly one human authentication anchor in the approved device record and
its refresh family: local User/password revision, company OIDC connection
ID/revision, or the social provider/identity anchor specified in REQ-012.
Both retain the CLI client binding. Local login requires no OIDC connection; do
not create a synthetic one. Refresh verifies the original anchor and current User
state, then rotates normally. A changed password revision or no-longer-valid
provider anchor refuses renewal rather than switching authentication methods.
Unapproved device records carry no authenticated principal/anchor. Preserve
expiry, polling, replay detection, explicit approval and browser-origin controls.
Rust retains browser-session and credential ownership; Node renders and forwards.

Setup-created and publicly enrolled humans use this same tenant-bound RFC 8628
flow. Discovery supplies a workspace login link, never CLI authority; local
password approval needs neither SMTP nor an IdP. No password grant, browser-token
handoff or new CLI identity is added.

### REQ-003 — Preserve runtime family, identity and attribution

Retain WorkloadRestriction and WorkloadFamily. A family-bound Service account
names one Service Card family, with no release version or UID. The existing SDK
supplies the exact running release through workload_uid; the server validates it
against the restriction and resolves its exact attribution graph. The account
and credential remain usable across releases without recreation or rotation.

The restriction grants no operation permission. Operation grants on dependencies
do not let the account impersonate another runtime family. Preserve root-required
behavior, signed-root mismatch refusal, the unique live runtime account per family
and existing unrestricted automation. Owning workload Card, subject Card, exact
Verifier, principal and actor chain retain their meanings. A parent used to filter
evidence access must not overwrite runtime ownership or attribution.

### REQ-004 — Resolve once, check current cached authority

A warm request validates its credential/token and checks the requested operation
against already-resolved process-memory authority. It does not reload roles,
expand group membership, derive permissions or query PostgreSQL merely because
another request arrived. Compile account grants at population or relevant change;
unchanged background revision confirmation does not recompile them. Signature
verification and the permission checker remain synchronous and store-free.

Access-affecting changes atomically advance tenant authorization state. Healthy
replicas enforce committed changes within five seconds. A process that observes
newer state must refresh before allowing; it cannot knowingly use obsolete grants.
Miss/invalidation acquisition may perform the sanctioned permission-state reload.

If database confirmation fails without known invalidation, an existing snapshot
may allow only until five minutes after its last successful confirmation.
Requests, failed refreshes and token renewal do not extend that bound. No usable
snapshot, known invalidation or expired freshness produces an unavailable-state
refusal, never anonymous or token-snapshot fallback. Existing database-dependent
browser authentication or operations can fail sooner. Subsequent stream output
must stop when its authority or permitted freshness is lost.

### REQ-005 — Tokens identify callers and preserve restrictive ceilings

Remove the authoritative permissions snapshot and informational roles from the
tenant token contract. Retain validated issuer, audience, subject/account/tenant,
issuance/expiry, token ID, credential provenance, delegation actors, card_access
and workload_uid, with the agreed explicit token-contract version. Implement the
revised claims directly; there is no old-token compatibility reader. Preserve
tenant access-token lifetime limits.

Ordinary human login and unrestricted Service-key tokens have no additional
permission ceiling. Restricted-key and delegated tokens carry a signed typed
ceiling: the maximum authority available through that token, never an independent
grant. An absent ceiling adds no restriction; an empty ceiling permits nothing;
invalid input is refused. Current subject authority, credential restrictions,
current delegation-participant authority, inherited ceilings, audience and workload
restrictions must all permit the requested operation.

A personal key remains bounded by its creation-time ceiling when its account
later gains access. Delegation preserves existing subject/actor validation, depth,
expiry and narrowing; an unrepresentable intersection is refused before signing.
Accepted execution authority is retained as a serializable ceiling plus identity
and attribution. Retry/recovery cannot expand it using newer grants or substitute
SYSTEM authority. Preserve existing principal-revocation cancellation behavior.

### REQ-006 — Resource access includes the correct retained data

Repository Card read access includes its code-review results, review sets, member
executions and retained evidence, subject to existing explicit sensitive-content
controls. This holds for custom roles as well as viewer; no GitHub configuration
or separate referenced-Verifier read grant is required. Opening another Card's
definition independently checks that Card. Starting work, changing policy and
approval remain separate operations; approval also requires parent visibility.

Capture a trusted exact authorization parent at acceptance: the subject Card for
ordinary verification, the Verifier for subjectless direct verification, and the
Repository for PR work and its members. Derived records retain that parent after
queue cleanup or Card deletion. Do not infer it from mutable labels, a shared
Verifier or caller-supplied identity. Family grants continue to cover their retained
history; inaccessible/unknown exact objects have ordinary not-found behavior.

Point reads, downloads, lists, searches, pagination and counts apply consistent
visibility before building results. Oracle enforces managed verification/evidence
row visibility before user expressions and aggregation across memtables, Parquet,
views, joins, subqueries and peers. A table grant alone cannot unlock another
Card's managed verification rows. Mixed queries require authority for every
source. Internal reads returning caller-visible data obey that caller boundary;
peers, pagination and reusable query state cannot widen it. Unprovable parents
are hidden from ordinary reads, not guessed or made readable by admin status.

Ordinary user tables and unattributed telemetry remain explicitly table-scoped.
Gateway records retain their deployment boundary and separate captured-payload
checks, including use in predicates. Audit history requires explicit tenant audit
read. This change does not promise arbitrary user-table row policies.

### REQ-007 — Separate GitHub configuration from Repository visibility

GitHub connection configuration pages, installation inventory and their UI/HTTP
operations require admin or explicit GitHub-configuration permission, enforced
by Rust. A Repository grant does not confer it. A GitHub-only configurator does
not thereby gain OIDC administration or tenant data access.

Preserve the association's verified host/stable provider repository identity,
selection and current provider-access checks. Connecting a repository creates no
Repository Card or user grant. Preserve family identity through provider rename
and reject duplicate families for the same provider repository. Authorized PR
work uses the existing selected declaration authority and webhook admission;
retained review reads need no GitHub request. Disconnect fences new provider
work without erasing retained evidence or otherwise valid Wyrd read grants.

### REQ-008 — Administer accounts and scoped grants through the UI

Provide tenant administration for Users, Service accounts, Roles and optional Groups
(presented as REQ-021's Settings sections),
using the same server contracts available through HTTP/CLI and thin first-class
SDK/MCP projections. Tenant admins create/manage accounts and scoped assignments;
non-admin direct requests cannot bypass the UI boundary. Preserve last-admin
protection and existing credential issuance, rotation, revocation and secret handling.

Decision: an invitation records explicit role-and-scope assignments, not bare
role IDs. The subject is the User created when it is accepted. At consumption,
revalidate the issuer's current authority to assign those grants, the referenced
roles, scopes and tenant. Create the User and those bindings atomically. An invalid
invitation creates neither; a missing scope is refused, never interpreted as All.
Enrollment creates the recorded assignments, not a tenant-wide role assignment.
Keep existing one-use, expiry and password-setup behavior. REQ-013 and REQ-017
add addressed links, bulk entry and optional email delivery.

Local User creation keeps its existing account/invitation flows; social admission
and public signup follow REQ-012 through REQ-014. Runtime Service creation
selects a Service family name, not an exact version; display runtime family and
operation grants separately. The grant form selects account/group, role and explicit
scope, showing applicable operations and family inclusion of future versions.
Access summaries identify direct/group/provider sources; removing one binding
does not imply removal of remaining inherited or broader authority.

Built-ins are read-only in administration; custom roles use the closed operation
list. Apply these agreed bundles within applicable scopes:

| Role | Authority |
|---|---|
| viewer | Resource/derived-record reads and authorized queries; no audit or captured gateway payload read |
| editor | Viewer plus authoring, verification binding management, execution/cancellation; no approvals or access administration |
| workload | Read the explicitly granted runtime resource's definition/artifacts and its execution context/evidence; execute verification, append authorized data and invoke granted gateways; no authoring, approval, audit or captured payload read |
| approver | Read/decide approvals on granted Verifiers only where the execution parent is also readable; no authoring/execution grant and no implicit grant on a different parent such as a Repository |
| auditor | Explicit tenant audit-history read; no implicit ordinary data/payload grants |
| admin | All tenant operations, bound at tenant scope |

Authentication alone grants nothing; new trusted workload issuers have no implicit
viewer fallback. Explicit configured issuer grants still use the common binding model.
Role names are not bypasses: a workload's dependency references supply no access
without applicable grants. An approver on a Verifier needs a separate Repository
read binding when approving a Repository-owned execution it otherwise cannot read.

### REQ-009 — Request access without an approval subsystem

An authenticated user can copy an access request containing their identity,
requested action and supplied link/identifier, then contact their administrator
through an existing channel. An admin responds by creating an ordinary binding;
the existing session gains access after normal propagation without re-login.
Unknown and inaccessible targets remain indistinguishable. The help flow reveals
no hidden resource name, owner, administrator roster or installation inventory.
Do not persist requests, send messages or add notifications/approval queues.

### REQ-010 — Revise existing owners without an upgrade subsystem

Reuse existing role, account, credential, execution and storage owners. Replace
superseded role-assignment code with the scoped binding model and update its
callers, schemas and fixtures together. Do not maintain parallel grant stores,
compatibility aliases, a second checker or token-snapshot fallback.

Nothing has shipped: historical grant conversion, retained-data backfill,
credential reissue, deployment cutover and rollback machinery are out of scope.
Exercise the revised system using fresh test state and its current contracts.
Normal execution retry/restart and retained evidence are still required runtime
behavior: serialize accepted ceilings and trusted read parents correctly from
acceptance, without changing owning Cards or principal/actor attribution.
Unprovable parent identity never authorizes ordinary disclosure.

### REQ-011 — Operators onboard shared providers; tenants manage company SSO

All deployment types use the same open-source implementation. An authenticated
operator administrator configures shared Google and GitHub.com sign-in through
platform UI and Rust management APIs. Setup shows provider registration
instructions and the exact callback URL, accepts client ID/secret, tests sign-in,
and enables the tested configuration. Provider-side registration remains the
operator's action. Store one authoritative platform configuration, with encrypted
secrets omitted from read responses; UI and programmatic management share it.

Social providers sit beside the tenant's existing single Active company OIDC
connection, preserving its Candidate/test/activate lifecycle. Tenant admins
manage only their own company SSO and login policy. Platform SSO authenticates
platform administrators; it is not inherited tenant SSO, and platform authority
does not implicitly authorize tenant settings changes.

Password is enabled by default and can be disabled by the operator. Public
signup is separately disabled by default; SaaS enables it through the same setting.
There are no bundled SMTP or provider credentials. Local public signup requires
password enabled, signup enabled and configured SMTP. Social acquisition requires
an enabled provider and verified contact proof; a provider-proven contact needs
no SMTP. Returning social login and invitation enrollment need no verified email. Rust returns and enforces allowed login methods from configured
providers and tenant policy, regardless of UI state. Absent/disabled providers
are not offered. The complete methods contract distinguishes returning/invitation
providers from usable acquisition providers, local signup, email discovery and
password-reset requests. `signup` means a creation method is usable now, not just
that its setting is enabled. Root, tenant login, invitation and device approval
render only advertised methods; Rust rechecks at start and completion, including
policy changes after page load. Root offers key/URL entry, live-session enumeration,
and Find my workspaces only when SMTP makes discovery usable. Show a root-page
“Create a workspace” link only when signup is usable; distinguish that journey
from “Join with an invitation”. Keep the tenant sign-in card narrow. At the
owned-workspace limit, disable or replace the creation form with the refusal and
recovery guidance. No unusable creation button is offered under private defaults.
Users remain tenant-local; no cross-tenant person account is added.

### REQ-012 — Provider proof authenticates a bound purpose, never tenant access

Rust owns provider exchange and completion for returning login, invitation
acceptance, new-tenant signup and linking. Before redirecting, bind the purpose,
provider identity/revision, browser, trusted origin, target tenant when present,
and invitation or current User where applicable. Callback parameters cannot
replace these. Retain five-minute transactions, one-use completion, server-held
state/PKCE and OIDC nonce, issuer/audience/signature/expiry validation, and fixed
post-login destinations. Use provider-specific social callbacks to separate
operator providers from tenant-configured provider endpoints.

Google reuses the OIDC relying party with `openid email profile`. GitHub.com
uses authorization code/S256 PKCE and an authenticated user lookup; its numeric
user ID is the subject under a fixed GitHub.com namespace. Do not use the mutable
username or treat its OAuth access token as an OIDC ID token. Request no
repository/organization access. For acquisition only, GitHub may request
`user:email` and read its authenticated email API for a verified address. Google
email verification is trusted only after validated OIDC `email_verified` proof.
Without provider-verified contact, bind a separate Wyrd email challenge to that
social attempt before creation. Email remains contact proof, never identity
lookup or automatic account linking. Drop provider access tokens after
identity resolution; retain no provider refresh-token vault.

Resolve returning social login only by tenant + issuer/provider namespace +
subject through the existing identity store. Missing identity refuses login
without creating a User; inactive tenant/User remains refused. Emails may repeat
and never select or merge accounts. Social login grants no role, replaces no
direct/local-group binding, and must not clear company-derived bindings. Existing
company-SSO first-login provisioning and group mapping remain distinct.

Browser sessions, device approvals and human refresh families retain the actual
social provider revision and linked-identity association as their original
authentication anchor. Disabled/replaced provider configuration or removal of
the identity refuses anchored session use/renewal without switching methods.
Recheck these bindings and tenant policy at completion; racing changes cannot
create a session from a no-longer-valid provider or method. All replicas enforce
the same provider revision. No synthetic company connection or merged
cross-tenant session is introduced.

### REQ-013 — Addressed individual and bulk invitations grant intended access

A tenant administrator invites one recipient email or pastes multiple addresses
and selects explicit role/scope assignments. Create a separate expiring,
single-use invitation per recipient and report per-recipient results. Ten or
more recipients require no different identity/provisioning model.

Each invitation is a link: a fixed tenant invitation page with its one-time
code in the URL fragment, so the code reaches no server, proxy log or referrer. Create and resend return each link once so the administrator can copy and
deliver it through any channel; when SMTP is configured, Wyrd also emails it
under REQ-017. Pending invitations can be listed (recipient, assignments,
expiry, last send outcome; never the code), resent and revoked. Resend issues a
fresh code and expiry and voids the previous link. Store only a verifier of the
code. Opening the link spends nothing; the code is consumed only when the
recipient completes enrollment.

Possession of the live link admits enrollment with any allowed password or
provider method, as in comparable products; the addressed email identifies whom
the administrator intended and where Wyrd sends it. It never selects, links or
identifies an existing User, and a differing provider email does not refuse
enrollment. There is no shared multi-use link: one invitation admits one User.
Whoever holds a forwarded live link can enroll; this is the accepted cost of the
conventional model, bounded by single use, the 24-hour expiry, revoke and the
administrator's review of pending invitations.

Preserve REQ-008's issuer-authority and scoped-assignment checks. In one tenant
transaction, revalidate the invitation, tenant policy and provider binding where
applicable; create the User/identity and intended bindings, consume the
invitation and create the session, or commit none of them. Concurrent
redemption cannot spend an invitation twice or create duplicate Users. Invalid,
expired, revoked or superseded links create no active User or grants. If the
provider identity already belongs to a User, refuse new-User enrollment and
direct them to sign in and request access; do not transfer the identity or
silently merge accounts.

Invitation possession never verifies the recipient address. After joining,
a User may separately verify a recovery contact under REQ-017; this never blocks
offline enrollment or creates another workspace. Password signup under
`/auth/browser/password/signup` stays invitation-only.

### REQ-014 — Public signup reuses tenant creation and makes its creator admin

When public signup is usable, a visitor proves a mailbox through Wyrd email,
or Google/GitHub identity plus verified contact, enters a workspace name with
existing slug validation, and confirms creation. Local email acquisition collects
no password before email proof; after proof require a tenant-local username and
password. Social acquisition forbids local credentials and creates no password. Wyrd
creates a new tenant with that tenant-local User bound to the tenant admin role,
then signs the User into it. Creating the tenant grants this role; being the
first visitor to an existing tenant does not. Email/domain/provider organization
or an existing slug grants no membership elsewhere. Operator-created tenants
and administrator invitation flows remain available.

Extend the existing provisioner with human-owner initialization. Reuse builtin
roles, SYSTEM, table initialization, provisioning states and activation/failure
handling; activate only a usable tenant with its administrator. Keep platform
and tenant transaction boundaries. Public signup confers only authority to create
this new tenant, never platform administrative authority, a platform principal,
or the existing administrative bootstrap key in the response.

Recheck signup enablement, method availability and provider validity where applicable at completion. Apply shared
source/proven-identity rate limiting before expensive creation, so replicas
cannot bypass the limit; unavailable shared admission state refuses creation.
Bind retry/resumption to the original attempt and proven identity. Callback
replay creates no second tenant, another identity cannot adopt a failed claim,
and failure leaves no active ownerless tenant. A completed tenant remains
accessible through returning login if its session response is lost. The same
proven identity can independently create or join another tenant without sharing
that tenant's User, grants or credentials.

Operator-authorized `wyrd-server setup --tenant acme --server URL` requires
`--admin-username` plus a hidden password prompt or `--admin-password-stdin`;
optional `--admin-email` is unverified. Never accept passwords in argv or an
environment dump. Missing non-interactive human inputs fail before tenant claim.
Extend the same provisioner's human-owner path for local credentials, retaining
its separate automation/recovery bootstrap credential result. Setup activation
requires a usable named human admin. Return login URL and username, never password.
Safe reruns return completed status without resetting passwords or duplicating
admins; interrupted setup resumes through existing operator-authorized recovery
and cannot activate ownerless. Existing machine-only administrative tenant
creation may omit the optional typed `human_admin:{username,password,email?}`.

Local creation sets a verified tenant-local recovery contact, admin binding and
ordinary password-anchored session only after promotion. Social creation stores
its verified contact and ordinary social-anchored session. A claimed slug conflicts;
choosing another before claim does not require a new email. Successful completion
retries return the same tenant and never create another. Generalize creator
accounting to a verified-email namespace for local creators and provider
namespace/subject for social creators. The cap is per proven identity, not per
human across methods; email never unifies social identities. Preserve every
revision-5 control below.

Revision 5 adds the following signup abuse controls. Each follows a published
convention: Cloudflare visitor-IP restoration and `/64` grouping, Auth.js sealed
OAuth-check cookies, RFC 6585 `Retry-After`, OWASP pseudonymization and
cheap-checks-first, Turnstile server-side validation and per-user resource
limits.

- **Trusted source.** The source of a request is the TCP peer, unless that peer is
  inside a trusted ingress IP/CIDR list named in deployment configuration (not
  a database record). In that case it is the client address the ingress supplies
  in the configured header, such as `CF-Connecting-IP`. A forwarding header from
  any other peer is ignored. IPv4-mapped addresses are normalized. For admission
  keys only, an IPv6 source counts as its `/64` prefix. Password-login source
  admission uses the same derivation, with its own windows, not a shared budget.
  When no source can be determined, signup start and completion are refused,
  never admitted without a source limit.
- **No pending row before proof.** Starting signup stores no pending signup/User/tenant PostgreSQL row;
  shared bounded admission counters are permitted. The provider round-trip state lives in a sealed, short-lived, path-scoped,
  HttpOnly cookie: state, PKCE verifier, nonce, provider and revision, redirect
  and browser binding. The callback still yields at most one proof per state:
  the provider redeems an authorization code once, and the proof record is
  unique on the state's hash, so of concurrent or replayed callbacks with one
  cookie exactly one records a proof and the others are refused. Only that
  verified proof is persisted, because completion and retry ownership require
  it.
- **Admission before work.** Starting signup is admitted in its own shared
  source window before any provider discovery or other work. No identity exists
  then, so no identity window applies. Completion keeps its separate source and
  identity windows. Unavailable admission state refuses start as it refuses
  completion. A throttled request answers `429`
  with `Retry-After` in whole seconds; the problem details carry the same value.
- **Pseudonymous, bounded counters.** Admission keys and the tenant's creator
  key are HMAC-SHA-256 values keyed from the platform derivation key (REQ-019), over the scope and
  value; no plain issuer, subject or address is stored there. Expired admission
  windows and expired or completed signup records are purged in bounded
  background batches. A public request never runs unbounded deletion. A
  completed attempt keeps no email.
- **Optional human challenge.** When the operator configures a challenge
  (Cloudflare Turnstile site and secret key), workspace creation requires a
  challenge token. Before claiming a tenant, the server sends the token and
  secret to the provider's server-side validation endpoint. It requires the
  provider's answer to report success, the deployment's own hostname and the
  `signup` action. A missing, invalid or expired token, or one the provider
  reports already used, creates no tenant and returns
  `WYRD_AUTH_403_SIGNUP_CHALLENGE`. If the
  challenge provider is unavailable, creation is refused. The challenge runs
  after admission, so refused traffic incurs no validation call. It applies only
  to the browser signup completion, never to SDK, CLI, MCP or API traffic.
  Without the configuration, no challenge is required.
- **Owned-workspace cap.** Each proven identity may have at most an
  operator-configured number (default 3) of signup-created tenants in any state other than
  retired, so a failed or provisioning claim counts until retired. A new claim
  at the cap is refused with `WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT`, not `429`,
  because waiting does not clear it. Resuming an attempt's own claim is not a
  new claim. The count is checked atomically with the claim, so concurrent
  completions on any replicas cannot exceed it. Operator-created tenants
  neither count nor are refused.
- **Crash recovery.** A creation interrupted by a process failure stays
  recoverable by its owner: the browser holding that proven attempt's
  completion cookie. Once the attempt and its claim have made no progress for
  two minutes, the signup staleness bound, that holder may restart it inside
  its five-minute completion window;
  nobody else can. After the proof expires, nobody can restart it; the claimed
  tenant stays in its non-active failed or provisioning state until operator
  retirement, never active and ownerless.

### REQ-015 — Explicit linking preserves the User and its access

**Revision-11 delivery status:** self-service linking, password addition to a
social-only User and method removal are deferred, not launch acceptance. The
behavior below remains the future contract if those capabilities are implemented;
do not build their UI, endpoints or proof orchestration in the remaining task.
Operator-wide invalidation required by shipped controls and REQ-012, and the
shared password-validation/reset safeguards below, remain required.

In account settings, a User freshly proves their existing allowed method within
five minutes, then proves the additional provider in a purpose-bound linking
transaction. A stale cookie or API-key recovery session alone is insufficient.
Attach the identity to that tenant's same User atomically. An identity owned by
another User refuses linking; concurrent claims have one winner. Linking an
identity already attached to the same User creates no duplicate.

Keep principal ID, direct grants, local memberships, keys and attribution intact.
Company-SSO linking applies only that connection's verified group mappings.
Linking prevents future duplicates; it does not merge existing Users. A
social-only User may add a password after fresh proof of an existing allowed
method, using existing username uniqueness/password validation; matching email
or a reset request alone cannot add a method.

Removing a method requires fresh proof through a remaining allowed method;
refuse removal of the last usable method. Removing an identity invalidates its
anchored sessions/refresh without revoking unrelated methods or changing the
existing API-key policy. Preserve last-usable-admin protection and operator recovery.

The last-method check applies to a User removing their method; it does not
prevent an operator disabling/replacing a shared provider or disabling password.
Those changes invalidate the affected login paths even if a User has no other
usable method. Operator recovery remains available; no automatic fallback or
new grant is created.

All local password operations (setup, direct creation, invited enrollment,
change, reset and adding a password) use one validator: 15–128 Unicode characters
after existing NFC normalization, existing blocklist and Argon2id. Reject shorter
or oversized values without truncation. No composition rules or periodic forced
rotation; allow paste and password managers. Reuse bounded blocking for hashing.
Email reset changes only an existing password; it cannot add one to a social-only
User. Verified contacts and reset never override required company SSO.

### REQ-016 — Company-SSO enforcement applies to existing human login paths

A tenant administrator freshly authenticated through its tested Active company
connection can require company SSO. Warn about Users without usable linked
company identities before enabling policy. Rust enforces it: password/social
starts and completions refuse, invitation acceptance uses company SSO, and
existing password/social browser sessions and CLI renewal become unusable under
the established freshness rules. Disabling policy does not resurrect revoked
sessions. Existing grants and token lifetime/revocation rules remain applicable;
no instantaneous global revocation guarantee is added.

Revision 11 fixes the remaining administration flow. Reuse tenant Authentication
settings beside the existing company connection. An admin-only read reports the
policy and its revision, current company connection ID/revision and readiness,
and a count of active Users without a usable identity for that connection;
disabled Users and Services are excluded. The count is an informational warning,
not a requirement that all Users enroll before enablement. Show explicit
confirmation. Enable submits the observed policy and connection revisions;
Rust rechecks current tenant-admin authority and fresh company proof through
that same current tested Active connection at the mutation boundary. A stale
revision refuses with conflict and requires reload; refused changes have no effect.
Ordinary Disable also requires fresh company-admin proof; unavailable SSO uses
the existing independent operator-assisted recovery path.

Fresh company proof retains the existing tenant five-minute successful-provider-
sign-in meaning, not a new universal upstream password/MFA re-entry guarantee.
If stale, sign in through the existing company flow and return to settings for
explicit confirmation; no pending mutation runs automatically after login.
Bearer/API-key possession, connection-test proof and an MCP write scope do not
substitute for this human proof. Existing ordinary connection administration and
independent recovery retain their current authority requirements.

Linking is not a prerequisite: existing company first-login provisioning and
verified group mapping, or existing explicit tenant-admin grants to that company
User, establish a company-authenticated administrator before enforcement. A
password/social User and a company User remain separate; their grants, keys and
principal IDs are not merged or transferred automatically. Disabling policy
permits new allowed-method logins, but does not revive invalidated credentials,
including credentials not presented while enforcement was enabled.

Preserve existing personal/Service API-key policy and independent operator-assisted
recovery. Ordinary password/social login cannot bypass tenant policy. Company
SSO is not continuous offboarding: upstream removal alone is not a Wyrd
revocation signal. Local suspension remains the existing administrative control,
subject to documented propagation/freshness.

Requiring company SSO remains enabled when its connection is replaced,
deactivated or unavailable. Human sign-in requires the current Active company
connection; without a usable one it refuses rather than falling back to
password/social. Replacement retains existing connection-revision invalidation
and issuer/subject identity rules; it never moves identities between Users.

### REQ-017 — Application mail, verified contacts, discovery and recovery

An operator administrator may configure one deployment SMTP service: host,
port, TLS mode, optional credentials and sender address, with a test-send
action. Store credentials through existing encrypted-secret handling; reads omit
them. Tenant administrators use delivery but cannot read/change its credentials.
Self-hosted and SaaS use the same path. Provider account/sender-domain setup
remain external operator work. Resend is an example, not a dedicated integration,
preset, SDK dependency or required test provider.

With SMTP configured, send each invitation link separately after its transaction
commits; do not hold a tenant transaction open during SMTP IO. Report relay
acceptance or failure per recipient, never claim inbox delivery from SMTP
acceptance. The create/resend response carries each link and its send outcome
after sending finishes. A send failure leaves the invitation pending, creates no
active User and permits explicit resend, which returns a new link to copy. Without SMTP, invitations
work through copied links and the UI says Wyrd will not email them. No new mail
queue, automatic retry subsystem or provider-specific API is required.


The same SMTP owner sends signup/contact proofs, workspace-discovery proof,
password-reset codes and successful-reset notifications. Without SMTP, hide
local creation, discovery and reset request; preserve password login, copied
invitations, setup, administrator-issued reset and device approval. Removing SMTP
prevents mail-dependent starts, not ordinary login or redemption of valid issued
reset codes. Existing signed proofs remain usable until their deadline if signup
and their method remain enabled.

A freshly authenticated allowed-method tenant User can request and verify a
recovery address. Keep verified contacts distinct from profile/invitation email;
duplicate addresses are allowed within and across tenants. Bind verification to
User, tenant and purpose. Atomically replace verified contact/revision and its
directory update intent, notify the old verified address, and invalidate reset
proofs bound to the old revision. No identity, password or grant changes. Setup
and invited accounts remain usable without contacts but cannot use email recovery
or discovery until separately verified.

Root discovery requests return the same generic `202` for known/unknown email.
Mail contains no workspace names. Only after browser-bound email proof may Rust
return bounded, deduplicated active workspace links for that exact verified
address, with no username/principal IDs. Links are fixed same-origin login paths;
the proof grants directory read only and creates no session, User, grant or link.
Each selected workspace requires its own normal authentication. No public email
or domain membership search exists. The minimal HMAC directory is the only new
cross-tenant lookup purpose. Revalidate live tenant, live User and current verified
contact/revision through their authorized owners before disclosure; stale, revoked,
disabled or unavailable validation never reveals a workspace. Eventual projection
updates may temporarily omit a workspace. Existing live-session enumeration stays
independent. Without SMTP, use saved URLs, invitations or an administrator.

Password reset requests take workspace plus username, not email as identity.
Return indistinguishable generic `202` bodies and comparable timing distributions
for known, unknown, unverified and ineligible accounts. Send only to an active
eligible password User's verified address. Use bounded process-owned transient
send execution on the same public response path; crash may lose a send and the
caller requests again. No durable delivery promise, plaintext spool, queue or
automatic retry. SMTP errors never disclose an account-specific outcome; a
deployment-wide outage may uniformly return `503`. Existing fifteen-minute,
one-use reset-code redemption invalidates local sessions and CLI refresh chains,
leaves keys alone, never signs in or adds a password, and cannot bypass company
SSO. The email contains workspace URL plus a code to paste; reset codes never
appear in URLs. Notify the verified address after success without the password.
Administrator-issued codes remain the offline recovery path.

Email proofs last fifteen minutes, signup completion proofs five minutes, and
discovery-read cookies five minutes. Before proof, sealed encrypted email envelopes
carry random identifier, canonical address, purpose, expiry and initiating-browser
binding, never a password; no pending User/tenant/signup row is written. After
proof, persist unique identifier digests for one-use consumption across replicas,
retaining spent identifiers until expiry. Lost verification response resumes only
in the same browser. Signup/discovery/contact/reset purposes cannot substitute
for one another. Persisted deadlines use PostgreSQL `statement_timestamp()` and
bound durations; pre-proof envelopes use the PostgreSQL-anchored server clock.
Keep original address for delivery; trim input whitespace, canonicalize domain,
preserve local-part case, and never collapse dots, plus tags or provider aliases.
Use this comparison consistently, accepting missed differently cased addresses
rather than merging accounts.

Apply separate shared source and address/account budgets, including resend, before
SMTP or hashing for signup/contact verification, discovery and reset. Use keyed
pseudonyms; recipient throttles and comparable timing never expose account
existence or delivery. Source throttles may return `429`. Reset requests never
lock an existing account. Retain production TLS, host-only Secure/HttpOnly/SameSite
cookies, trusted Origin, CSRF protections, fixed destinations and `no-store` for
proof/result pages and responses. Email links place proof in fragments, not query
strings; opening consumes nothing, page code POSTs proof then clears the fragment.
Never log addresses, passwords, codes or proofs. Sends occur after transactions,
outside locks; operator test-send/invitation delivery retain honest relay outcomes.

### REQ-018 — Platform console sign-in stays in the platform plane

An already registered platform operator opens `/platform`, with page heading
**Platform admin**. Use the existing platform principals, `PlatformLogin`,
`PlatformLoginRequest` and first-party Rust browser-session machinery; extend
those owners for the browser purpose rather than duplicate login orchestration.
The single platform OIDC connection authenticates pre-registered operators and
retains issuer/subject pinning and unknown-subject refusal. Operator MFA belongs
to the IdP. Installs without an IdP permit a registered platform `user` to sign
in with a platform-user password using the same 15–128 normalized-character,
blocklist and Argon2id policy as REQ-017. This is a platform credential, never a
tenant User or a cross-tenant person account. Provision/manage it through the
existing authorized platform principal/credential owners; there is no anonymous
operator enrollment or first-visitor administration.

Successful proof creates a regenerated opaque platform-only browser session,
not a browser API token. Rust owns issuance, validation, revocation and expiry;
Node only forwards the selected cookie and trusted origin and renders safe
metadata. The host-only Secure, HttpOnly, SameSite=Lax cookie has `Path=/platform`
and a distinct platform name. Tenant cookies are never accepted on platform
operations; platform cookies are never forwarded to or accepted on tenant routes,
tenant enumeration, MCP, CLI token exchange, gateway, OTLP or gRPC. Path scoping
supplements Rust's plane checks; it does not replace them. Reuse browser origin,
CSRF, one-use state/PKCE/nonce/issuer binding, fixed redirects, no-store, redaction,
logout and bounded cleanup controls. Store failure refuses access.

Sessions expire after 15 minutes idle or eight hours absolute, whichever comes
first; activity cannot extend the absolute deadline. PostgreSQL owns persisted
coordination timestamps. Check the current active platform principal and original
password/connection revision; suspension, credential invalidation or platform
connection replacement invalidates the anchored session. Logout revokes only the
selected session. No refresh-token or recovery-key browser login is added.

Before destructive operations, including workspace retirement and provider,
SMTP or signup-setting changes, require fresh proof of the same platform user's
allowed password or platform OIDC method. Reuse the existing five-minute recent
human-proof window; OIDC must request fresh IdP authentication, not silently
reuse a Wyrd cookie. A stale session alone, tenant proof, different operator or
bootstrap key cannot satisfy re-authentication. Refusal performs no mutation.
Ordinary CLI/API authorization remains its existing separate channel. The
bootstrap/global administrative key is CLI/API break-glass only: it is never
accepted by console login or placed in a browser, Node, page data or cookies.

**Same-origin routing resolution:** official nginx currently sends the whole
`/platform` prefix to Rust. Reserve exact `/platform` for the console entry and
`/platform/ui` plus its subtree for SvelteKit pages/actions (including sign-in,
callback landing and re-authentication). Use nginx exact matches for `/platform` and `/platform/ui`, and a `^~`
prefix location for `/platform/ui/` ahead of the existing API regex; all other `/platform/*` routes stay Rust
APIs, including Rust-owned browser-session operations under `/platform/browser/*`.
The BFF calls those owners server to server with the platform cookie only; OAuth
code exchange stays in Rust. Existing API paths, `platform` schema and `Platform*`
types retain their names; add no `/admin` or other alias and use no content-type
routing heuristic. The UI's `/platform/ui/*` namespace separates pages from
existing management resources such as `/platform/tenants` and `/platform/mail`.

### REQ-019 — Sign-in and signup secrets use the shared stored-secret encryption

Revision 8. The deployment sealing keyring is gone, so every secret and keyed
value this change introduced moves onto the shared `WyrdKms` facade of
[SPEC-stored-secret-encryption](../stored-secret-encryption/spec.md). No sign-in,
signup, mail or challenge behavior changes.

- **Stored secrets are ordinary stored secrets.** The shared social-provider
  client secrets, the SMTP relay password and the signup challenge secret are
  sealed and opened only through `WyrdKms`, each under its own purpose and bound
  to its owning record, as that spec's owners are. They take part in its
  encryption renewal and wrapping-key maintenance like every other store. A
  configured secret that cannot be opened refuses the operation that needs it,
  as an unavailable dependency does today: that provider's sign-in and signup,
  mail sending, or, for the challenge, signup completion. It is never treated
  as unconfigured, so an unreadable challenge secret never lets signup proceed
  without a challenge.
- **One platform derivation key for cookies and pseudonyms.** Sealed browser
  flow cookies, email-proof envelopes and every keyed pseudonym (admission
  keys, creator keys and the workspace discovery directory) use keys derived
  from one platform derivation key. Each use derives its own key under a
  distinct fixed label, so a value from one use never verifies or decrypts as
  another; the derivation function and labels are implementation choices. The server
  generates that random key once, wraps it with `WyrdKms` and stores the
  wrapped form durably; every replica unwraps the same stored key, and when
  several replicas start together exactly one generated key is kept. No raw key
  is ever stored or configured.
- **Rotation keeps pseudonyms stable.** Rotating the `WyrdKms` wrapping key
  re-wraps the derivation key through the shared maintenance and never changes
  it. A creator's tag is therefore the same before and after rotation, and the
  owned-workspace cap keeps counting workspaces created under the old wrapping
  key, including after that key is retired. This replaces any matching of tags
  across several keys.
- **Unavailable key refuses.** When the derivation key cannot be read or
  unwrapped, every flow that needs it (signup start and completion, social
  sign-in, invitation and linking round trips, email proof issue and
  verification, and workspace discovery) refuses with
  `WYRD_AUTH_503_VERIFY_UNAVAILABLE` and records nothing. Tenant and platform
  password sign-in, which uses no flow cookie or pseudonym, keeps working.

This change adds no operation that replaces the derivation key. If one is ever
needed (for example after the key itself is exposed), replacing it ends
in-flight flow cookies and proofs, resets admission windows, forgets creator
counts and requires the discovery directory to be rebuilt from tenant contacts;
that is outside this change.

### REQ-020 — Sign-in starts are admitted before they record anything

Revision 9. A sign-in start is an unauthenticated request that begins a
provider or device round trip and, today, writes one `wyrd.auth_login_state`
or `wyrd.auth_device_authorizations` row (a pending-login row) before the
person has proven anything: `POST /auth/browser/login` (company SSO, social
and invitation sign-in), `POST /auth/platform/login` and
`POST /auth/device_authorization`. Linking (REQ-015) starts from a signed-in
session and is not a sign-in start. In multi-tenant SaaS a client can send
thousands of starts, so the server bounds them itself rather than relying on
ingress configuration: the bundled per-container nginx would count per replica
and, behind a load balancer, would see the balancer's address.

- **Admission comes first.** Once the request body parses, and before tenant
  or provider resolution, any pending-login row, provider discovery or cookie,
  each start is counted in a sign-in window keyed by its trusted source. The
  source is derived exactly as REQ-014 derives it, including IPv6 /64 grouping.
  One window per source is shared by every replica, tenant and start route.
  It is separate from signup's windows, so neither flow's flood blocks the
  other. A window admits 300 starts per source per hour.
- **Over the limit, nothing is recorded.** A refused start writes no
  pending-login row, sets no cookie and contacts no provider. `Retry-After`
  gives whole seconds until the window ends. The browser and platform starts
  answer `429` problem+json `WYRD_AUTH_429_SIGN_IN_ATTEMPTS` with
  `details.retry_after_seconds`. The device authorization endpoint keeps its
  RFC 6749 §5.2 error form: `429` with `Retry-After` and
  `{"error":"temporarily_unavailable"}`.
- **Refuse when admission cannot run.** Window keys are keyed pseudonyms from
  the platform derivation key, as signup's are (REQ-019); no plain address is
  stored. When no trusted source can be derived, the key cannot be read, or the
  counters cannot be read or written, the start refuses with `503` and records
  nothing. The browser and platform starts use `WYRD_AUTH_503_VERIFY_UNAVAILABLE`;
  the device endpoint uses `{"error":"temporarily_unavailable"}`. The
  browser and platform refusals carry the server's usual retryable
  `Retry-After: 1`; the device refusal carries none. REQ-019's flows needing the derivation key therefore
  include these starts.

Unaffected: tenant and platform password sign-in, the platform root
credential (the operator's local break-glass sign-in), the pending-login row's
contents and five-minute lifetime, password sign-in's own failure windows,
signup's windows, and the device user-code entry form, whose code-guessing
limit remains an ingress concern (RFC 8628 §5.1). Excluded: a deployment-wide
aggregate ceiling and operator-configurable limits.

### REQ-021 — Admin console presentation and identity projections

Every tenant administration job is reachable from one visible Settings section
list, and the platform console uses the same shell. The UI presents existing
server behavior; it owns no workflow and re-derives no server-owned count,
threshold or aggregation. Where this requirement leaves a choice, follow
conventional admin-console practice (GitHub, Linear, Vercel, Slack, Google
Workspace). The [mock](design/admin-mock/README.md) is the look-and-wording
specification.

Tenant Settings (`/t/{tenant}/settings/*`) shows a section list beside one work
region:

| Section | Shows | Primary action |
|---|---|---|
| People | Status counts; Users with name, email, sign-in method, roles, status; pending invitations as a filter with delivery, expiry, resend and revoke; a person drawer with roles, API keys and suspend | Invite people (drawer) |
| Service accounts | Accounts, restriction, key count, status | New service account |
| Roles | One row per Role with its description and member count; the count opens People filtered to that Role; operations on demand | New role |
| Sign-in | Methods the tenant offers; company SSO setup (callback URL, client, test, activate); Require company SSO as a switch whose confirmation states the server's lock-out count; deactivate in a Danger zone | Set up company SSO |
| Secrets | One inventory of tenant-owned stored secrets: gateway provider credentials, Operator connections, the company SSO client secret and trusted-issuer secrets, with state and last rotation; Replace, Renew encryption and Revoke where the owner offers them | Add provider key |
| Integrations | GitHub, as today | — |

Your account stays at `/t/{tenant}/settings/account` and opens from the
sidebar name block for every User, not from the admin section list. The name
block and Your account show the caller's name and email, never a placeholder or
principal ID. A person is labelled by display name, else username, else email,
with email as the second line.

The platform console (`/platform/ui/*`) uses the same shell: Overview,
Workspaces, Sign-in providers, Mail, Admins and Stored secrets (maintenance
status and runs), with the light/dark control. Each action uses its existing
platform endpoint and fresh-proof rule.

Every page follows one pattern: title, one-line purpose, at most one primary
action, then status and content. Lists use the built table, status pill, empty
state and drawer components; settings use label/help/control rows; machine
values are read-only with Copy; destructive actions confirm and sit in a Danger
zone. Evidence Thread tokens and geometry remain; status carries a glyph or word.

`wyrd.auth_users` gains a nullable `display_name`. Nothing has shipped, so it is
added to the unshipped `20260601000001_auth.sql` in place rather than by a new
migration; this explicitly authorizes that one edit and the resulting local
database reset. Each successful sign-in sets it from the identity provider: the
Google ID token `name` claim, the GitHub profile `name` (its `login` when the
name is blank), or the company OIDC `name` claim. A provider that omits it leaves
the stored value unchanged. Password Users have none. Users cannot edit it.

`wyrd.auth_roles` gains a nullable `description`, added to the same unshipped
migration in place under the same authorization. The server sets the
description of each built-in Role; an administrator may set or clear a custom
Role's description through `PUT /v1/access/roles/{role}`. A Role without one
shows no summary.

Read projections:

| Endpoint | Adds |
|---|---|
| `GET /v1/principals/me` | `email` (the caller's own) and `display_name` |
| `GET /v1/principals` items | `display_name`, `authentication` (the same shape `/me` returns) and the role assignments with scope |
| `GET /v1/principals` page | counts by status for the requested filters |
| `GET /v1/principals` query | a `role` filter: principals holding that Role at any scope, directly or through a Group |
| `GET /v1/access/roles` items | `description` and `member_count`: distinct Users and service accounts holding the Role at any scope, directly or through a Group, each counted once |
| `PUT /v1/access/roles/{role}` | optional `description` for a custom Role |

The `/me` addition revises its "no email" rule for the caller's own address
only; it discloses nothing about another principal. `architecture/wyrd-design.md`
(the `GET /v1/principals/me` paragraph) must be reconciled before implementation. Its first
sentence becomes: "`GET /v1/principals/me` returns the calling User's safe
profile: principal, tenant, username, display name, the User's own email,
authentication method and effective permissions. It never returns another
principal's email."
The Secrets inventory is composed by the host from the owner endpoints; no
inventory endpoint is added. Rust, Python and TypeScript SDKs and MCP project the
new fields.

Google and GitHub sign-in buttons carry the provider's official mark, per
Google's Sign in with Google branding guidelines and GitHub's logo guidelines:
"Continue with Google/GitHub" on root/workspace signup, tenant sign-in and
"Join with" on invitation acceptance. The Google "G" keeps its official colors;
the GitHub mark is monochrome and follows the text color in both modes. The
same marks identify the method in People and on Sign-in. The marks are fixed
SVG assets, an explicit exception to Evidence Thread's tokens-only color rule.

Excluded: a user-editable display name, editable built-in Role descriptions, a rotation-age warning threshold, per-method
usage counts, Groups UI beyond today's, and any new administration capability.

### Revision-6 HTTP contracts (REQ-011–REQ-017)

| Route | Request → response and authority |
|---|---|
| `GET /auth/browser/methods?tenant=acme` (tenant optional) | Retain `{password, company_sso, providers, signup}`; add `{local_signup, email_discovery, password_reset, email_verification_required:true, signup_providers:[...]}`. `providers` means returning/invitation methods; `signup_providers` means eligible creation methods. Without tenant, password is not a global login promise; root uses discovery/key entry. `signup` means at least one creation method is usable now, not merely that its operator switch is on. No secrets or account counts. |
| `POST /auth/signup` | Tagged union `{method:"email", email}` or `{method:"social", provider}` replaces mandatory provider-only input. Email: `202 {status:"check_email"}`, sets sealed flow cookie. Social: `200 {authorization_url}`, sets existing sealed provider-check cookie. Local requires password enabled, signup enabled and SMTP; provider paths enforce configured enablement. |
| `POST /auth/signup/verify` | `{proof}` + initiating cookie → `200 {status:"proven"}` and opaque completion cookie. Proof comes from an email fragment link or pasted token, never a password-bearing URL. Social callback reaches the same proven-attempt state. A provider lacking verified contact uses the email branch bound to that provider attempt before completion. |
| `POST /auth/signup/complete` | `{slug, display_name, local_credentials?:{username,password}, challenge_token?}` + completion cookie → `200 {tenant}` + ordinary tenant session cookie. Credentials required for local proof and forbidden for social proof. Email, issuer, subject, role and tenant ID come from server-held proof. Repeat successful completion returns the same tenant; no second creation. |
| `POST /auth/browser/workspaces/request` | `{email}` → `202 {status:"check_email"}` and sealed browser binding; uniform for known/unknown addresses. SMTP required. Mail goes to the submitted address with no workspace names. |
| `POST /auth/browser/workspaces/verify` | `{proof}` + binding → `200 {workspaces:[{tenant,display_name,login_url}],next_cursor?}`. Bounded page, same short-lived discovery cookie for pagination via `GET /auth/browser/workspaces?cursor=...`. Result URLs are fixed same-origin workspace-login paths; proof grants directory read only. Empty result is normal after proof. |
| `POST /auth/browser/password/reset/request` | `{tenant,username}` → `202 {status:"check_email"}` for every syntactically valid admitted request. Send only for an active eligible password User with verified contact. SMTP failure is not exposed as an account-specific error. |
| Existing password routes | `/signup {tenant,invitation_code,username,password}` and `/login {tenant,username,password}` establish their existing session; `/change {tenant,current_password,new_password}` and `/reset {tenant,reset_code,new_password}` retain existing response contracts and invalidation. Reset never signs in. |
| `POST /auth/browser/email/request` and `/verify` | Fresh allowed-method tenant session: `{tenant,email}` → `202 {status:"check_email"}`; `{tenant,proof}` → `204`. Bind proof to that User and purpose. On verification atomically replace the verified contact/revision and directory-update intent; notify the old verified address. No grant, identity or password change. |
| Existing platform tenant creation and settings | Extend typed create-tenant input with optional `{human_admin:{username,password,email?}}`; setup requires it, existing machine-only administrative creation may omit it. Password never appears in response. Reuse existing platform login-settings and mail management operations, not a parallel setup API or configuration source. |

Failures enter the stable Wyrd error catalog: distinguish disabled method
(`403`), invalid/expired proof (`400`), unavailable dependency (`503`), claimed slug
(`409`) and admission throttle (`429`, `Retry-After` header and equal whole-second
problem field). Preserve `WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT`. Account-sensitive
request routes keep their generic `202`; a deployment-wide SMTP outage may return
uniform `503`, never condition that response on an account lookup.

## Persistence and authority changes

This revision names the schema definitions the revised system needs. These are
implementation changes, not an upgrade/data-migration project. This document
authorizes no editing of an applied migration or resetting an existing database.
Tenant-scoped PostgreSQL tables lead with data_tenant_id and tenant references
remain tenant-qualified. Genuine deployment-wide and pre-tenant platform tables
may omit that key under narrowly authorized platform operations. Every
PostgreSQL table enables and forces RLS, including platform tables. This is the
maintainer-approved platform distinction, not permission to fabricate a tenant
or grant a signup visitor platform authority. Coordination time remains
PostgreSQL-owned. No new family-ownership or principal store is introduced.

| Owner/table | Required change |
|---|---|
| wyrd.auth_roles | Retain the role store; encode typed operation bundles rather than role-owned object scopes |
| New wyrd.auth_groups and wyrd.auth_group_memberships | Local group identity/roster with unique tenant names/memberships and valid User/Service references |
| New wyrd.auth_role_bindings | Tenant-qualified role, subject, explicit typed scope and direct/provider provenance; validate subjects and index subject/role/scope access |
| wyrd.auth_user_roles and wyrd.auth_service_account_roles | Replace superseded assignment definitions and callers with scoped bindings; no stored-assignment conversion; retain bootstrap/internal authority through its existing owner |
| New wyrd.auth_authorization_revisions | One monotonically advancing revision per tenant, changed with authorization mutations; transactional notification and background confirmation |
| wyrd.auth_device_authorizations and wyrd.auth_refresh_tokens | Add local password-revision anchors and adjust approval/renewal constraints to accept exactly the appropriate local or complete provider anchor; preserve client binding and one-use/rotation rules |
| wyrd.auth_user_invitations | Replace role-only invitations with scoped assignment intents so enrollment cannot turn a narrow invitation into tenant-wide access; revalidate at consumption |
| wyrd.auth_api_keys | Retain existing JSONB permission_ceiling; encode validated scoped permissions and adjust owner/ceiling constraints for supported restricted Service keys while preserving required personal-key limits |
| Existing verifier execution authority JSON and Bifrost accepted records | Define serializable accepted ceilings and trusted parent metadata for newly accepted work and normal retry/recovery; do not add evidence or analytical payloads to PostgreSQL |
| Bifrost verification tables | Preserve a trusted family/exact parent in execution_inputs, execution_outputs, execution_events, results, evidence and approvals through their existing schema/writer owners; no historical-record conversion |

Revision 3 adds these bounded persistence purposes. Physical record/table names
remain implementation-owned; no applied migration may be edited. Platform-only
records belong to the existing platform control plane, not a fabricated tenant
or visitor principal. Their storage placement and policies follow the owning
[Postgres tenant-binding authority](../../../architecture/wyrd-security-posture.md#postgres-tenant-binding):
platform records may omit the tenant key; every table still forces RLS. No
additional isolation or authority exception is approved.

| Owner/store | Required change |
|---|---|
| New platform social-provider configuration records | Provider kind, client metadata, sealed secret, tested/enabled state and revision; existing platform management authority; shared by replicas |
| New platform application-mail/signup settings records | SMTP host/port/TLS/sender and sealed credentials, password availability and signup-enable setting; revision 5 adds the owned-workspace cap and the optional challenge site key with sealed secret; no tenant-visible secrets or implicit tenant administration rights |
| Existing tenant identity store and local User credentials | Reuse tenant/issuer/subject ownership uniqueness; explicit attachment/removal and lookup validity for anchors; no platform-person identity store |
| wyrd.auth_login_state | Purpose-bound returning login, invitation and linking context, exact provider revision, browser binding and linked User/invitation where needed; preserve company/test/device flows |
| New platform pre-tenant signup records and admission counters | Expiring verified signup proof and completion/retry ownership, reference to the provisioned tenant, and shared pseudonymous trusted-source/proven-identity rate-limit state; no pending signup record before email/provider proof, no plain identity or address in admission keys, no provider token vault (revision 5) |
| platform.tenants and existing provisioning state | Associate signup-created tenant/claim with its proven creation attempt so another caller cannot adopt a failed claim; record the creating identity's keyed pseudonym, so the owned-workspace cap survives attempt purge (revision 5); retain the directory lifecycle |
| New platform derivation-key record (revision 8) | One platform-scoped record holding the `WyrdKms` envelope of the platform derivation key (REQ-019), plus the owner identity and fence fields that SPEC-stored-secret-encryption maintenance uses for any other stored envelope; no raw key; forces RLS; permits only the existing platform owner. The provider, SMTP and challenge secret columns above hold `WyrdKms` envelopes instead of sealing-keyring values. |
| Platform admission counters (revision 9) | The signup admission counters above gain one sign-in scope for REQ-020's per-source start windows: the same pseudonymous keys, fixed windows and batched expiry; no new table and no plain address. Nothing has shipped, so the unshipped workspace-signup migration's scope check is amended in place rather than by a new migration; this explicitly authorizes that one edit and the resulting local database reset |
| Role description (revision 12) | `wyrd.auth_roles.description`, nullable text; server-set for built-in Roles, administrator-set for custom Roles (REQ-021); added to the unshipped `20260601000001_auth.sql` in place under the same authorization as the display name. Existing forced RLS applies; no new table or index. |
| User display name (revision 12) | `wyrd.auth_users.display_name`, nullable text set from the identity provider's name at each successful sign-in (REQ-021); added to the unshipped `20260601000001_auth.sql` in place, which this explicitly authorizes with the resulting local database reset. Existing forced RLS applies; no new table or index. |
| wyrd.auth_browser_sessions, wyrd.auth_device_authorizations, wyrd.auth_refresh_tokens | Complete social provider/revision and linked-identity anchors, invalidation/renewal constraints; preserve password/company/recovery anchors and client binding |
| New tenant login-policy metadata | Require-company-SSO state and revision tied to current Active company connection; integrate existing freshness/invalidation owners |
| wyrd.auth_user_invitations | Recipient address, scoped intents, pending/revoke/expiry/consumption/resend and safe per-recipient send outcome; no raw invitation secret in storage |

Revision 6 extends these approved stores as follows; this explicitly approves the
bounded discovery projection rather than deferring it. Physical names remain
implementation-owned. Required support columns, unique/lookup/expiry indexes,
foreign keys, checks and RLS policies implement only the named properties.

| Revision-6 persistence purpose | Boundary and reuse |
|---|---|
| Proven email signup/discovery/contact proof and one-use redemption | Genuine pre-tenant platform records may omit `data_tenant_id`, but enable and force RLS and permit only the existing platform owner. Extend proven signup state; no pre-proof pending row except shared abuse counters. Retain only bounded proof/consumption metadata, expiry, browser digest and necessary temporary address. No global person or global password store. |
| Tenant-local verified recovery contact | Every tenant table leads with `data_tenant_id`, forces RLS and uses tenant-qualified User references. Track address, verification time and replacement revision separately from unverified profile/invitation email. Duplicate emails remain allowed across and within tenants. Reuse User/auth owners rather than changing identity uniqueness. |
| Workspace discovery directory | Minimal platform control-plane projection: HMAC of canonical verified email → tenant ID and User reference/contact revision. Needed because tenant-RLS connections cannot scan every tenant at root; not an authorization or identity table. No passwords, grants, sessions or evidence. Tenant transaction updates a contact revision and notifies the existing reconciliation machinery; reconciliation reads through authorized tenant connections and updates/removes projection entries. Before revealing any result, revalidate current verified contact, live User and live tenant through those owners; stale or unavailable validation fails closed. Bound pages and cleanup. |
| Recovery and admission metadata | Reuse tenant password-reset verifiers/revisions and shared counters, adding email-purpose/source HMAC scopes and spent-proof retention where needed. No raw reset secret or mail body in PostgreSQL; support indexes/constraints are limited to the purposes and properties named here. |
| Provisioning and creator accounting | Existing tenant claim/proven-attempt ownership, initialization and activation. Generalize human owner to local credential or external identity. Keep creator HMAC after proof purge for cap enforcement; use verified email namespace for local creators and provider namespace/subject for social creators. Cap is per proven identity, not per human across methods. |

Revision 7 additionally authorizes only these platform persistence purposes.
Extend the existing platform identity and Rust browser-session/SQL owners; tenant
session rows cannot represent a tenantless operator without violating tenant RLS.
Physical names remain implementation-owned in the `platform` schema. Every table
enables and forces platform-owner-only RLS; no fabricated tenant is used.

| Revision-7 persistence purpose | Required data and permitted support properties |
|---|---|
| Platform-user local password credential | Existing platform `user` principal reference, unique principal ownership, Argon2id verifier, password revision and invalidation state. No plaintext/recoverable password, tenant credential, global person store or new principal kind. Reuse password validation and admission owners. |
| Platform browser session | One-way opaque session digest, platform principal reference, exact password or platform OIDC connection/revision anchor, creation/last-activity time, 15-minute idle and eight-hour absolute deadlines, revocation and last fresh human-authentication time. Lookup uniqueness, principal/anchor references and bounded expiry/revocation cleanup indexes/constraints only. Never store raw session identifiers, bootstrap keys or provider tokens. |
| Platform browser login and re-authentication binding | Extend existing platform login transaction metadata with browser binding, login versus same-operator re-authentication purpose, original session/principal and exact connection revision, fixed redirect and one-use expiry/consumption. Reuse existing state/PKCE/nonce owner and bounded cleanup; no second OAuth engine or SQL function. |

These purposes include the support columns, constraints, lookup/expiry indexes
and RLS policies needed for the named properties only. No migration is changed by
this documentation revision; implementation must cite this section for each
schema change and must never edit an applied migration.

The uncommitted `20261121000006_workspace_signup.sql` is unapplied anywhere;
revise it in place to the final revision-7 shape, rather than stacking a correction.
Never edit an applied migration. No schema work is performed by this revision.

Support constraints/indexes/policies are limited to provider/method validity,
unique identity ownership, bound one-use completion and email proof consumption/expiry, verified-contact
revision/tenant-qualified User ownership, HMAC directory lookup and revalidation,
reset-contact revision validity, shared admission accounting,
per-recipient lookup/state and tenant isolation. Invitation
contents/raw codes are not a PostgreSQL mail spool; construct message content
for sending, persisting only coordination metadata. Any additional persistence
purpose or SQL function needs a specification revision. Rate-limit windows/counts
and private representations remain implementation-owned within the bounded
shared-admission requirement.

Within those named changes, support columns, constraints, indexes and policies
are limited to the described subject/role/scope records, membership uniqueness,
tenant isolation and references, revision confirmation, login-anchor validity,
invitation scopes, credential ceilings and their lookup paths. Physical names,
JSON layout and cache representation remain implementation-owned. Naming a table
does not authorize an unrelated change to it. Tables/records implementing the
named revision-3 through revision-8 purposes above are permitted; splitting a purpose across
physical tables does not authorize another purpose. Tables, persistence purposes
or indexes/policies outside the named purposes/properties, and any new SQL
function, require a specification revision before schema implementation.
No new SQL function is proposed.

The owning [security posture](../../../architecture/wyrd-security-posture.md#authorization-and-policy),
[runtime identity](../../../architecture/wyrd-design.md#runtime-identity),
[Repository](../../../architecture/wyrd-design.md#repository) and
[Bifrost public authorization](../../../architecture/bifrost-design.md#public-surface)
sections were reconciled for revision 2. Platform separation,
tenant isolation, nonblocking audit, workload ownership and credential security
remain required. Revision 3's authority reconciliation is recorded on
2026-11-21 in the owning
[social admission, linking and mail](../../../architecture/wyrd-security-posture.md#social-admission-account-linking-and-application-mail),
[browser sessions](../../../architecture/wyrd-security-posture.md#first-party-browser-sessions),
[Postgres tenant binding](../../../architecture/wyrd-security-posture.md#postgres-tenant-binding)
and [human sign-in](../../../architecture/wyrd-design.md#human-sign-in) sections.
They now cover shared operator providers, public signup through the existing
provisioner, social/identity anchors, addressed invitations, SMTP, company-SSO
policy and the approved PostgreSQL platform-key distinction. This records the
approved design boundary; it does not claim implementation or verification.

Revision 7 reconciles the platform plane and first-party browser-session sections
of the security posture and human sign-in in the design, plus official same-origin
host routing in operations. The maintainer's explicit 2026-11-21 approval resolves
the operator-UI blocker; this is approved behavior, not implementation evidence.

## Acceptance: connected system scenarios

These are system observations, not one scenario per implementation feature.
Plans derive focused success/refusal cases inside them. Use production UI,
real CLI/SDK calls, WyrdTestServer and repository PostgreSQL with no IdP for the
local journeys. Assertions inspect actual authority, returned/retained data and
absence of forbidden side effects, not only UI success messages.

| Scenario | Connected action and observation | Obligations |
|---|---|---|
| AC-001: An administrator gives a user access, and every surface respects it | Create Alice and payments access through the UI, including direct/group variants. Alice completes local login and CLI approval. UI, HTTP/CLI, SDK/MCP and raw SQL expose the same Repository/review membership; another Repository contributes no data/counts. GitHub configuration and access administration remain denied. Programmatic creation/grants exercise the same contracts. | REQ-001, REQ-002, REQ-006, REQ-007, REQ-008 |
| AC-002: A provisioned Service operates across releases with correct attribution | Create checkout-runtime through HTTP/CLI and the UI variant; inspect the same family/grants. Run two checkout releases with the same account/key. Retained results preserve exact release, owning Card, subject, Verifier and actor attribution. Another runtime family's UID and an ungranted operation are refused without unauthorized work. | REQ-001, REQ-003, REQ-005, REQ-006, REQ-008 |
| AC-003: Access changes affect existing sessions and credentials predictably | Copy a safe request, grant through HTTP/CLI, use the existing session, then remove/change through the UI. Observe healthy propagation, warm cache reuse, restricted/delegated limits and agreement across callers. Interrupt confirmation to prove known-invalidated refusal and the five-minute maximum with bearer requests independent of browser-store availability. | REQ-001, REQ-004, REQ-005, REQ-008, REQ-009, REQ-010 |
| AC-004: An operator enables social login and an administrator invites a team | Set up/test/enable Google and GitHub through operator UI/APIs beside company SSO; tenant-admin changes to shared settings fail. Without SMTP, paste 10+ recipients with scoped access and copy each returned link; configure a local SMTP test server and observe separate emailed links, send outcomes and pending/resend/revoke state. Accept links using allowed password/provider methods; stranger login, expired/revoked/superseded/replayed links, an already-owned provider identity and concurrent redemption gain no unauthorized User/grant. SMTP failure is visible beside the returned link and creates no active User. Returning social login preserves access and company/direct bindings; another tenant remains separate. | REQ-011, REQ-012, REQ-013, REQ-017; preserves REQ-001, REQ-008 |
| AC-005: A visitor creates a workspace and becomes its administrator | With signup enabled, prove Google/GitHub identity, choose a new workspace and observe a usable tenant, creator's admin binding and ordinary session. Disabled signup, rate-limit refusal, unavailable admission state and invalidated provider create no active tenant. Exercise replay, concurrent/retried creation, failed initialization and lost session response: no duplicate/ownerless active tenant or takeover of another attempt. An existing tenant's slug/domain grants no entry; no platform principal/privilege or bootstrap key is returned. Revision 5: behind configured trusted ingress, distinct visitors get distinct source windows, a spoofed forwarding header from an untrusted peer is ignored, rotating within one IPv6 /64 shares a window and a missing source refuses start and completion; starting signup leaves no pending signup row (bounded admission counters are allowed) and its flood is throttled before provider work; every throttle carries `Retry-After`; stored admission keys contain no plain identity or address; with a challenge configured, missing/invalid/wrong-hostname/wrong-action/reused tokens and an unavailable provider create no tenant, and unconfigured deployments and non-browser surfaces need none; a replayed or concurrent callback records one proof; the owned-workspace cap refuses a new claim with its quota error across replicas, while resuming an own claim and operator creation are unaffected; a completed attempt keeps no email; an interrupted `creating` attempt is resumed by its owner only. | REQ-011, REQ-012, REQ-014 |
| AC-006: A tenant adopts company SSO without account linking | Establish a company-authenticated admin through existing company provisioning and explicit grants or verified group mapping. Read the admin-only active-User warning, enable after fresh company proof and observe refusal of password/social starts, racing callbacks, existing browser sessions and CLI renewals across replicas. Ordinary Disable requires fresh company-admin proof and never revives invalidated credentials, including credentials unused while enforcement was enabled. Connection outage/replacement/deactivation never enables fallback; API keys, independent operator recovery, other tenants and platform login retain their policy. Revision 11 defers self-service linking/password-add/removal and their preservation/race assertions. | REQ-012, REQ-016; preserves REQ-002, REQ-004, REQ-005 |
| AC-007: Local verified-email creation works with private defaults preserved | Defaults advertise no usable signup and anonymous creation writes no tenant. Enable signup/password/SMTP; email proof precedes credentials, then one workspace/admin/verified contact/password session is created. No pending signup row before proof except bounded counters, no bootstrap secret, no duplicate on replay/concurrency/lost response. Without SMTP local signup is absent; social acquisition needs a validated provider contact or bound Wyrd proof, while invitations remain offline-capable. | REQ-011, REQ-012, REQ-013, REQ-014, REQ-017 |
| AC-008: Setup yields a human admin and ordinary CLI access | With no SMTP/IdP, setup's named admin signs in and approves the real CLI device code. Missing noninteractive credentials claim nothing; rerun/crash does not duplicate/reset admins or activate unusable tenants. Automation/recovery output remains separate. Public signup User also approves the same tenant-bound device flow. | REQ-002, REQ-014 |
| AC-009: Discovery reveals verified workspace links only | Known/unknown requests disclose nothing before browser-bound proof. Proof returns bounded deduplicated active links, never User IDs or a login session. Separate tenant passwords/grants remain independent; revoked/replaced contacts, disabled Users, retired tenants, stale rows, cross-tenant references and unavailable validation reveal nothing. Projection lag may omit links. No SMTP hides discovery. | REQ-011, REQ-017 |
| AC-010: Verified contacts enable private self-service recovery | Verify/replace a contact after setup/invitation without blocking join. Known/unknown/unverified reset requests have indistinguishable bodies and comparable timing; no account lock or existence leak from recipient throttles/SMTP. Single-use/expired/replaced/replayed codes and old contact revisions refuse. Success invalidates local sessions/CLI renewal, leaves keys, requires login, notifies contact, and cannot add a password or bypass SSO. Offline admin reset still works. | REQ-002, REQ-013, REQ-015, REQ-016, REQ-017 |
| AC-011: Every auth page obeys Rust's advertised methods | Exercise root, login, invitation, signup and device pages with no provider, no SMTP, password disabled, SSO required/unavailable and no usable methods; render only current capabilities. Race settings changes with POST and observe Rust refusal. Root creation link appears only when usable; create and join wording differ; narrow tenant card and quota refusal replace/disable the form. Refused configured challenge uses WYRD_AUTH_403_SIGNUP_CHALLENGE. | REQ-011, REQ-012, REQ-014, REQ-016, REQ-017 |
| AC-012: Every password operation shares the minimum | Setup, direct creation, invitation, local signup, change, reset and password-add reject fourteen normalized characters and accept fifteen unless blocklisted; accept 128-character Unicode and paste, reject oversized without truncation. No password/code/proof reaches logs. | REQ-014, REQ-015, REQ-017 |
| AC-013: A platform operator signs in and manages the deployment safely | Through the production `/platform` console headed Platform admin, a pre-registered operator signs in through controlled platform OIDC (fresh IdP re-authentication/MFA policy) and, without an IdP, a platform-user password. Unknown subjects cannot enroll; fourteen-character passwords refuse and fifteen-character passphrases work unless blocklisted. Observe host-only Secure/HttpOnly/SameSite=Lax `/platform` cookies, safe metadata and no bootstrap key/API token in browser or Node. Tenant sessions refuse platform access; platform sessions refuse tenant routes even if manually replayed. Exercise 15-minute idle and eight-hour absolute expiry, logout, suspension, password/connection revision invalidation, storage failure and replica continuity. Stale, different-operator and tenant proof cannot retire a workspace or change providers/SMTP/signup; fresh same-operator proof permits the authorized action without granting tenant authority. Through the official host, `/platform` and `/platform/ui/*` render pages while existing `/platform/*` APIs retain typed responses; the BFF forwards only the platform cookie to Rust. CLI/API break-glass remains available separately. | REQ-018; preserves REQ-011, REQ-017 and platform/tenant separation |
| AC-014: Signup and sign-in secrets survive wrapping-key rotation | With a file-backed `[kms]` key, an operator configures a social provider, SMTP and a challenge; a visitor creates workspaces up to the cap. Two replicas starting together keep one derivation key. Activate a new wrapping key with the old one retained, restart and run routine maintenance, then retire the old key and restart: provider sign-in, mail and the challenge still work, an in-flight signup completes, and the same identity is still refused at the cap. Stored values contain no plaintext secret or raw key. With the derivation key unavailable, signup, social sign-in and email-proof verification refuse with `WYRD_AUTH_503_VERIFY_UNAVAILABLE` and record nothing, while password sign-in works. With the challenge secret unreadable, signup completion refuses and creates no tenant. | REQ-019; preserves REQ-011, REQ-014, REQ-017 |
| AC-015: A flood of sign-in starts records only counters | Through a configured trusted ingress, one source sends company-SSO, social, platform OIDC and device authorization starts, including starts for an unknown tenant, until refused at the 301st start in its window. Each refusal is `429` with whole-second `Retry-After`: problem+json `WYRD_AUTH_429_SIGN_IN_ATTEMPTS` with `details.retry_after_seconds`, or `temporarily_unavailable` on the device endpoint. From then on, no pending-login row, cookie or provider request is added. Another source, on the same tenant and on another tenant, still starts and completes a sign-in. The refused source can still start a signup, and a source refused by signup can still start a sign-in. Two replicas share one count. With no derivable source, the derivation key unavailable, or the counters unavailable, each start refuses with `503` and records nothing, while password and platform root sign-in still work. | REQ-020; preserves REQ-014, REQ-018, REQ-019 |
| AC-016: An administrator runs the workspace from Settings | In the production UI, an admin opens Settings and reaches People, Service accounts, Roles, Sign-in, Secrets and Integrations from the section list. From People, Invite people sends invitations in two actions; each person shows display name, email, sign-in method and roles from the server, and status counts match the server. Roles shows each Role's description and server member count, and the count opens People filtered to that Role. Sign-in's Require company SSO confirmation shows the server's lock-out count and keeps REQ-016's proof and revision rules. Secrets lists the tenant's provider credentials, Operator connections and SSO/issuer secrets; Replace, Renew encryption and Revoke call their owner endpoints and a non-admin is refused. Google and GitHub buttons show the official marks in light and dark. Google and GitHub sign-in record the provider's name, and a later sign-in refreshes it. The name block and Your account show the caller's name and email for password, Google and GitHub Users; `/me` never returns another principal's email. The platform console shows the same shell and mode control. Rust/Python/TypeScript clients read the new `/me` and principal-list fields; another tenant's principals never appear. | REQ-021; preserves REQ-008, REQ-016, REQ-018 |

Existing safeguards remain required as cases/regressions within these journeys:
cross-tenant refusal, exact versus family scopes, direct/provider/group provenance,
root mismatch, key/delegation narrowing, optional federation, accepted-work recovery,
safe role changes and retained-source/peer reads. They do not create additional
top-level system scenarios beyond those above. Rust/Python/TypeScript and MCP exercise their shipped
boundaries, with language-lifetime behavior tested in its owning language.

Hot-path verification must list every added/moved PostgreSQL, Scribe, object-store
or Vault wait and match it to a sanctioned boundary. Warm authorization and row
checks add no wait; cold/invalidation acquisition is a permission-state reload;
audit stages on the existing Scribe outbox and never blocks the operation.
Apply owner format, lint, codegen, tenant-boundary and journey lanes.

New provider/mail journeys use controlled OIDC/OAuth and SMTP servers with real
Rust endpoints, production UI, repository PostgreSQL and real CLI/SDK credentials.
Exercise explicit device-code approval under social login as well as existing
methods. Google/GitHub operator smoke tests establish actual registration,
consent and callback compatibility without replacing negative/concurrency
journeys. No hosted email provider is required for verification. Preserve the
finalized design's hot-path review conditions if new enforcement changes a hot
path; this draft claims no benchmark or runtime result.

## Scope and delivery

The original two tasks cover backend authorization/enforcement and its admin
UI/journeys against AC-001 through AC-003. Their completed work and evidence
remain under approved revision 2; do not rewrite their task records. Revision 3
adds the finalized social-login/invitation/SMTP outcome as intended task 003,
with AC-004 through AC-006 and applicable regression coverage. Planning follows
approval; this spec creates no task. Existing consumers and original
authorization/attribution guarantees remain required.

Excluded: mandatory teams/ownership metadata, SCIM/group synchronization, new
principal kinds, a policy language/service, general user-table row policies,
per-release accounts/credentials, a UI-only workflow and an access-request inbox.
The addition also excludes cross-tenant person accounts, automatic account
merging, OAuth password grant, generic operator OIDC/provider plugin frameworks,
continuous IdP offboarding, new mail queues, provider-token vaults, CAPTCHA
other than REQ-014's optional server-validated challenge, deployment-wide
total signup caps and provider-specific mail integrations/tests. Platform console sessions and
platform-user password verifiers are explicitly included under REQ-018; excluded
are browser/Node bootstrap-key login, tenant-to-platform session reuse, anonymous
platform enrollment, a new principal kind, an `admin` route/schema/type alias,
platform refresh tokens and a second authentication/session engine. Operator MFA
is enforced by the IdP; Wyrd-native MFA is outside this change.
The bounded verified-email discovery projection and email proof metadata are
explicitly included; neither creates a global identity account.
Email invitations do not add delivery/notifications to REQ-009's access-request
copy flow.

### Revision 12: admin console

Maintainer-approved 2026-11-24 ("follow all conventional standards", "i
approve"). [TASK-005](tasks/TASK-005-admin-console.md) owns REQ-021 and AC-016.
It revises existing UI, adds read projections and the provider-sourced
`display_name` and Role `description` columns. Test-first ordering and recorded
failing tests are not required anywhere in it; every wire addition and journey
still needs passing tests. It depends on TASK-004's code, not on TASK-004's
review. TASK-004's paused live Google/GitHub smoke (steps 3–5) remains
TASK-004 evidence and may run on the revised UI.

### Revision 11: remaining launch scope and SSO administration

Maintainer-approved 2026-11-23 in the explicit instruction to defer carryover
cycles 17–18 and create one task for cycles 19–20, accepting their design
recommendations. Self-service linking, password addition to a social-only User
and method removal are deferred; they are not launch blockers. Their future
REQ-015 contract remains documented, and existing provider-disablement,
credential-invalidation, password-validation and recovery safeguards remain
required for every shipped capability. No shipped feature is weakened.

[TASK-003-carryover](tasks/TASK-003-carryover.md) retains cycles 1–16 and their
evidence. [TASK-004](tasks/TASK-004-company-sso-and-launch-verification.md) owns
company-SSO enforcement and connected launch verification (former 19–20), with
no dependency on deferred linking/removal. REQ-016 above records the agreed
warning, proof, revision-conflict, no-linking adoption and no-resurrection
semantics. This delivery decision supersedes the earlier requirement to finish
all twenty carryover cycles before launch; it does not assert deployment
readiness or completion of unfinished retained work. The original TASK-001/002
contracts and evidence remain unchanged.

The existing approved tenant policy/identity/session/device/refresh/revision
stores support this flow; no new persistence purpose, dependency, credential
channel or recovery system is approved. Required safe methods/settings metadata
and existing authorized-management projections remain within REQ-011/017;
human-proof mutations use the Rust-owned browser HTTP channel, not unrestricted
bearer/CLI/MCP setters. Live Google/GitHub smoke remains required evidence for
shipped social flows; missing access is a disclosed unfinished verification item,
not a silent waiver. No self-service method-management API is added by planning.

### Revision 10 addendum: reserved signup slugs

Maintainer-approved 2026-11-23. A self-service signup completion cannot claim a reserved slug (`admin`, `administrator`, `api`, `app`, `auth`, `billing`, `help`, `login`, `mail`, `platform`, `root`, `security`, `signup`, `status`, `support`, `system`, `www`, `wyrd`). It is refused with the same `409` as a taken slug and creates nothing. Operator creation through the platform API is unrestricted. No migration. Extends REQ-011/AC-005.

## Revision history

Revision 2 makes local device-login anchors and scoped invitation consumption
explicit, and removes shipped-system transition requirements because nothing has
shipped. The maintainer approved revision 2 on 2026-11-19; its architecture
reconciliation was recorded that day. Revision 3 records the design locked on
2026-11-21 and the clarification that generic SMTP, not a Resend-specific
integration, is the contract. It includes the approved PostgreSQL platform-key
distinction and completed authority reconciliation. The maintainer explicitly
approved revision 3 on 2026-11-21. Revision 4 (approved by the maintainer, 2026-11-21) follows the
conventional invitation model at the maintainer's direction: each invitation is
a single-use link the administrator can copy, SMTP is optional delivery, and
mailbox-challenge recipient proof and its records are removed. Task 003 is
replanned against it.
Revision 5 (approved by the maintainer, 2026-11-21) adds REQ-014's signup abuse controls at the maintainer's direction, following standard practice: trusted ingress source and IPv6 /64 grouping, no pre-proof row, admission before initiation, `Retry-After`, pseudonymous bounded counters, an optional server-validated challenge, an owned-workspace cap and crash recovery. It adds the public error `WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT` and derives its HMAC key from the deployment sealing keyring. The owning [social admission](../../../architecture/wyrd-security-posture.md#social-admission-account-linking-and-application-mail), local-password source and sealing-keyring sections were reconciled on 2026-11-21 with the maintainer's agreement (default cap 3, `403` cap error, keyring-derived key). Task 003 cycle 6 is replanned against it.
Specification approval does not authorize Git staging, commits or pushes.

Revision 6 — status: approved; revision: 6; date: 2026-11-21. The maintainer said
“i approve the spec revision” and accepted all eight “Decisions for the maintainer”
in [onboarding](design/onboarding.md) exactly as recommended, including verified-email
workspace discovery and its bounded projection. This approval retains revision 5
(including REQ-014 abuse controls), adopts the six onboarding corrections, names
their persistence purposes and AC-007–AC-012, and reconciles human sign-in,
security posture and operations configuration. The previously approved UI direction
and `WYRD_AUTH_403_SIGNUP_CHALLENGE` are recorded in REQ-011/014 and AC-011.
TASK-003 completed-cycle evidence is retained; remaining work belongs solely to
TASK-003-carryover at revision 6. No code, migration or mock is changed here.


Revision 7 — status: approved; revision: 7; approval recorded: 2026-11-21.
The maintainer explicitly approved decisions A (platform console sign-in) and B
(nothing ships until complete; nothing built twice). REQ-018 and AC-013 reuse
existing platform identity/OIDC and first-party browser-session owners, name the
platform-only persistence purposes and resolve same-origin page/API routing.
The carryover replaces its inherited operator-UI blocker with executable proof,
orders final shared foundations before acquisition, then platform sessions, mock
extension/approval, pages and original cycles 7–10. Existing REQ/AC observations
and completed evidence remain intact. No implementation, migration, test, mock,
staging, commit or push is authorized by this documentation handoff.

Revision 8 — status: approved; revision: 8; approval recorded: 2026-11-23. The
maintainer said "i approve" and directed the architecture reconciliation and the
merge of SPEC-stored-secret-encryption's branch. Adopts SPEC-stored-secret-encryption revision 2,
which removes the deployment sealing keyring. REQ-019 and AC-014 move this
change's provider, SMTP and challenge secrets onto `WyrdKms` and key its cookies
and pseudonyms from one `WyrdKms`-wrapped platform derivation key, which
wrapping-key rotation re-wraps but never changes. This resolves
FIND-003-carryover-5 without matching tags under several keys. It authorizes one
new platform derivation-key record. Before implementation, the
[security posture](../../../architecture/wyrd-security-posture.md) social-admission,
application-mail and browser-session sections that name the sealing keyring
are reconciled with this revision at the maintainer's direction. Approval does not
authorize staging, commits or pushes.

Revision 9 — status: approved; revision: 9; approval recorded: 2026-11-23. At
the maintainer's direction, an enterprise SaaS requirement: no per-request
record before a visitor is validated, beyond bounded counters. REQ-020 and
AC-015 admit each unauthenticated sign-in start through a per-source window in
the server, not at ingress, before its pending-login row is written. The
maintainer agreed the public error `WYRD_AUTH_429_SIGN_IN_ATTEMPTS`, the
300-per-hour limit, admission on `POST /auth/device_authorization`, the
`503` refusals when admission cannot run, and one added sign-in scope on the
existing platform admission counters, amended in the unshipped migration
rather than by a new one. The security posture's sign-in start note is
reconciled to server admission. Approval does not authorize staging, commits
or pushes.

Revision 11 — status: approved; revision: 11; approval recorded: 2026-11-23.
The maintainer explicitly agreed to defer cycles 17–18, accepted recommendations
for cycles 19–20, and requested one new execution task plus deferral markers in
the current packet. This revision records that scope and the REQ-016 details;
frontmatter now includes approved revisions 10 and 11. It authorizes planning,
not production edits, deployment, staging, commits or pushes.

Revision 12 — status: approved; revision: 12; approval recorded: 2026-11-24.
The maintainer approved the admin console design and its conventional-standards
decisions, and asked for this spec update and one task, TASK-005, in this
packet. Adds REQ-021 and AC-016, and authorizes adding `auth_users.display_name`
to the unshipped `20260601000001_auth.sql` in place. Requires reconciling the
`GET /v1/principals/me` paragraph in `architecture/wyrd-design.md` before
implementation. It authorizes planning, not production edits, staging, commits
or pushes. Amended 2026-11-24 with maintainer agreement: adds Role
`description` (same in-place migration edit), Role `member_count`, the
principal-list `role` filter, and drops test-first ordering from TASK-005.
