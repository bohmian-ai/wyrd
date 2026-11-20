---
id: SPEC-authorization-model
revision: 2
status: approved
---

# Scoped authorization with preserved workload attribution

## Problem and outcome

An administrator must be able to give a User or Service access to one Repository,
Model or other supported resource without granting access to the whole tenant.
The UI, HTTP, CLI, MCP, SDKs and raw Bifrost queries must agree about that access.
Today, broad Card grants and table-only queries cannot express that boundary.

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

Agreement: [design notes](design/notes.md), including the three system scenarios
and two-task split; [Rust responsibilities and token flow](design/rust-model.md).
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

Local users, existing invitations/password recovery, browser sessions, API-key
exchange and Service accounts work without an IdP. Preserve existing optional
OIDC, workload federation and delegation, their trust validation and the separate
platform plane. Registering or declaring a Card creates no principal or grant.

Decision: extend the existing device flow, not introduce a password OAuth grant.
A human signs into Wyrd using a local password or optional configured OIDC and
explicitly approves the CLI's user code. The CLI receives the existing short-lived
access and rotating refresh tokens through one-use device-code redemption.

Store exactly one human authentication anchor in the approved device record and
its refresh family: local User/password revision, or OIDC connection ID/revision.
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
Keep existing one-use, expiry and password-setup behavior.

User creation uses existing local account/invitation flows. Runtime Service creation
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

## Persistence and authority changes

This revision names the schema definitions the revised system needs. These are
implementation changes, not an upgrade/data-migration project. This document
authorizes no editing of an applied migration or resetting an existing database.
New tables lead with data_tenant_id, force RLS and use tenant-qualified references. Coordination time
remains PostgreSQL-owned. No new family-ownership or principal store is introduced.

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

Within those named changes, support columns, constraints, indexes and policies
are limited to the described subject/role/scope records, membership uniqueness,
tenant isolation and references, revision confirmation, login-anchor validity,
invitation scopes, credential ceilings and their lookup paths. Physical names,
JSON layout and cache representation remain implementation-owned. Naming a table
does not authorize an unrelated change to it. Any additional table, persistence
purpose, index/policy outside those properties, or new SQL function requires a
specification revision before schema implementation. No new SQL function is
proposed.

The owning [security posture](../../../architecture/wyrd-security-posture.md#authorization-and-policy),
[runtime identity](../../../architecture/wyrd-design.md#runtime-identity),
[Repository](../../../architecture/wyrd-design.md#repository) and
[Bifrost public authorization](../../../architecture/bifrost-design.md#public-surface)
sections are reconciled with these agreed replacements. Platform separation,
tenant isolation, nonblocking audit, workload ownership and credential security
remain required. This records the authority prerequisite as satisfied, not
implementation completion or runtime verification.

## Acceptance: three connected system scenarios

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

Existing safeguards remain required as cases/regressions within these journeys:
cross-tenant refusal, exact versus family scopes, direct/provider/group provenance,
root mismatch, key/delegation narrowing, optional federation, accepted-work recovery,
safe role changes and retained-source/peer reads. They do not create additional
top-level system scenarios. Rust/Python/TypeScript and MCP exercise their shipped
boundaries, with language-lifetime behavior tested in its owning language.

Hot-path verification must list every added/moved PostgreSQL, Scribe, object-store
or Vault wait and match it to a sanctioned boundary. Warm authorization and row
checks add no wait; cold/invalidation acquisition is a permission-state reload;
audit stages on the existing Scribe outbox and never blocks the operation.
Run full clean mise run bench:l1:verification, bench:l1:gateway and bench:l1:bifrost
with VALID verdicts, comparing the newest same-experiment/host history records.
Smoke, partial-step and dirty runs are insufficient. Apply owner format, lint,
codegen, tenant-boundary and journey lanes; no measured result is claimed here.

## Scope and delivery

Two implementation tasks are agreed: complete backend authorization/enforcement
first, then admin UI and its connected journeys consuming those contracts.
Each owns proof against the same three scenarios; testing is not deferred to the
UI task. Existing consumers remain correct after backend integration, and the
split does not permit an alternate authorization path or bypass.

Excluded: mandatory teams/ownership metadata, SCIM/group synchronization, new
principal kinds, a policy language/service, general user-table row policies,
per-release accounts/credentials, a UI-only workflow and an access-request inbox.

Revision 2 makes local device-login anchors and scoped invitation consumption
explicit, and removes shipped-system transition requirements because nothing has
shipped. The three acceptance scenarios and two-task split are unchanged.
The maintainer explicitly approved revision 2 on 2026-11-19. Architecture reconciliation was recorded on 2026-11-19; planning may proceed.
Recording approval and authority alignment creates no tasks, schema implementation,
production changes or Git actions.
