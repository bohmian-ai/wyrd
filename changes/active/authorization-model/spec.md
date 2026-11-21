---
id: SPEC-authorization-model
revision: 4
status: approved
---

# Scoped authorization and tenant-local social signup

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
a new workspace and becomes its administrator. Sam can explicitly connect
another sign-in method to that same User. A tenant can require company SSO.
Each invitation is a single-use link the administrator can copy; when an
operator configures SMTP, Wyrd also emails it. No particular email provider is
required.

Agreement: [design notes](design/notes.md), the original three system scenarios
and two-task split, [Rust responsibilities and token flow](design/rust-model.md),
and [finalized social-login design](design/social-login.md), locked 2026-11-21.
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

Provide Settings → Access for Users, Service accounts, Roles and optional Groups,
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
social signup is separately disabled by default; SaaS enables it through the
same setting. Rust returns and enforces allowed login methods from configured
providers and tenant policy, regardless of UI state. Absent/disabled providers
are not offered. Users remain tenant-local; no cross-tenant person account is added.

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
repository/organization or email API access. Drop provider access tokens after
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

### REQ-014 — Public signup reuses tenant creation and makes its creator admin

When public signup is enabled, a visitor proves Google/GitHub identity, enters
a workspace name with existing slug validation, and confirms creation. Wyrd
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

Recheck signup enablement and provider validity at completion. Apply shared
source/provider-identity rate limiting before expensive creation, so replicas
cannot bypass the limit; unavailable shared admission state refuses creation.
Bind retry/resumption to the original attempt and proven identity. Callback
replay creates no second tenant, another identity cannot adopt a failed claim,
and failure leaves no active ownerless tenant. A completed tenant remains
accessible through returning login if its session response is lost. The same
provider account can independently create or join another tenant without sharing
that tenant's User, grants or credentials.

### REQ-015 — Explicit linking preserves the User and its access

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

### REQ-016 — Company-SSO enforcement applies to existing human login paths

A tenant administrator freshly authenticated through its tested Active company
connection can require company SSO. Warn about Users without usable linked
company identities before enabling policy. Rust enforces it: password/social
starts and completions refuse, invitation acceptance uses company SSO, and
existing password/social browser sessions and CLI renewal become unusable under
the established freshness rules. Disabling policy does not resurrect revoked
sessions. Existing grants and token lifetime/revocation rules remain applicable;
no instantaneous global revocation guarantee is added.

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

### REQ-017 — Optional operator SMTP emails invitations with honest outcomes

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
| New platform application-mail/signup settings records | SMTP host/port/TLS/sender and sealed credentials, password availability and signup-enable setting; no tenant-visible secrets or implicit tenant administration rights |
| Existing tenant identity store and local User credentials | Reuse tenant/issuer/subject ownership uniqueness; explicit attachment/removal and lookup validity for anchors; no platform-person identity store |
| wyrd.auth_login_state | Purpose-bound returning login, invitation and linking context, exact provider revision, browser binding and linked User/invitation where needed; preserve company/test/device flows |
| New platform pre-tenant signup records and admission counters | Expiring browser/provider-bound signup proof and completion/retry ownership, reference to the provisioned tenant, and shared trusted-source/provider-identity rate-limit state; no provider token vault |
| platform.tenants and existing provisioning state | Associate signup-created tenant/claim with its proven creation attempt so another caller cannot adopt a failed claim; retain the directory lifecycle |
| wyrd.auth_browser_sessions, wyrd.auth_device_authorizations, wyrd.auth_refresh_tokens | Complete social provider/revision and linked-identity anchors, invalidation/renewal constraints; preserve password/company/recovery anchors and client binding |
| New tenant login-policy metadata | Require-company-SSO state and revision tied to current Active company connection; integrate existing freshness/invalidation owners |
| wyrd.auth_user_invitations | Recipient address, scoped intents, pending/revoke/expiry/consumption/resend and safe per-recipient send outcome; no raw invitation secret in storage |

Support constraints/indexes/policies are limited to provider/method validity,
unique identity ownership, bound one-use completion, shared admission accounting,
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
named revision-3 purposes above are permitted; splitting a purpose across
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
| AC-005: A visitor creates a workspace and becomes its administrator | With signup enabled, prove Google/GitHub identity, choose a new workspace and observe a usable tenant, creator's admin binding and ordinary session. Disabled signup, rate-limit refusal, unavailable admission state and invalidated provider create no active tenant. Exercise replay, concurrent/retried creation, failed initialization and lost session response: no duplicate/ownerless active tenant or takeover of another attempt. An existing tenant's slug/domain grants no entry; no platform principal/privilege or bootstrap key is returned. | REQ-011, REQ-012, REQ-014 |
| AC-006: A User connects a method and the tenant adopts company SSO | Freshly prove a password User's existing method and connect Google/GitHub/company SSO; principal/access/keys remain. Stale/recovery-only proof, conflicts and races cannot transfer identities. Add a password from a social-only User, remove a method and inspect anchor invalidation; last-method removal is refused. Enable company-SSO policy after admin proof and observe refusal of password/social starts, racing callbacks, existing browser sessions and CLI renewals. Provider replacement/removal and policy apply across replicas; API keys and operator recovery retain existing policy. | REQ-012, REQ-015, REQ-016; preserves REQ-002, REQ-004, REQ-005 |

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
merging, public password signup, generic operator OIDC/provider plugin frameworks,
continuous IdP offboarding, new mail queues, provider-token vaults, CAPTCHA,
operator-wide signup creation caps and provider-specific mail integrations/tests.
Email invitations do not add delivery/notifications to REQ-009's access-request
copy flow.

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
Specification approval does not authorize Git staging, commits or pushes.
