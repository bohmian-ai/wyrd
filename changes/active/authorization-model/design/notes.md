# Wyrd authentication and scoped access

Status: finalized

The maintainer resumed this packet on 2026-11-21 to design social sign-in and
signup as task 003, after reporting tasks 001 and 002 complete. The original
finalized agreement below remains the basis of approved specification revision 2;
its two-task limit applies to that original scope. The agreed addition is in
[social sign-in and signup](social-login.md), finalized by
the maintainer on 2026-11-21. It covers social signup/login, explicit linking,
operator UI onboarding, addressed bulk invitations and generic SMTP delivery.
It is recorded in [approved specification revision 3](../spec.md), with authority
reconciliation and explicit maintainer approval recorded on 2026-11-21.
[Task 003](../tasks/TASK-003-social-login.md) now proposes the implementation
and ordered acceptance scenarios for that approved addition. The maintainer approved
the PostgreSQL platform-storage distinction in the
supplement on 2026-11-21; authority/spec reconciliation is recorded on that date.
Locked product decisions remain intact.

The maintainer has locked the authorization model and preservation of existing
identity, ownership and attribution semantics, the three scenarios and two-task
split, and requested specification. The original agreement was carried into
specification revision 2, approved by the maintainer on 2026-11-19; its scope
remains preserved in the current draft.

## Problem

A person should be able to sign into Wyrd, use the CLI or an SDK, and see the
same authorized resources. Automation needs its own stable account and narrower
credentials. None of these journeys should require an external identity provider.

The immediate failure is Repository isolation: tenant-wide `cards:read` and
Verifier inspection cannot express “Alice can read Repository A and its review
sets, but not Repository B.” Table-level Bifrost grants can expose both
Repositories' retained data even if a UI hides one of them.

This is a separate change from `verification-workflow-ui`, informed by its
[authorization investigation](../../verification-workflow-ui/design/authorization-model.md).
That document's opening review and Codex appendix are research inputs, not
decisions or an implementation contract. This design makes its own recommendation;
the maintainer settles the design before specification.

## Desired outcome

Use one understandable rule across UI, HTTP, MCP, CLI, Rust, Python and
TypeScript: **a role says what an account can do; a binding says where it can
do it; a credential can restrict that authority but cannot increase it.**

An administrator can share a Repository with a person, service account or local
group. Its versions, review sets, executions and evidence follow that boundary,
including lists, totals, downloads and SQL. Someone who can read the selected
Verifier does not thereby gain access to the Repository's evidence.

## Constraints and non-goals

- Local users, invitations, password recovery, service accounts and keys work
  without SSO. OIDC and workload federation remain optional.
- Preserve separate platform and tenant administration, existing principal kinds,
  tenant RLS, server ownership, stable errors and nonblocking audit.
- Keep Card identity and envelopes unchanged. Groups do not own Cards. Labels,
  names of teams and arbitrary Card relationships confer no access.
- Use additive scoped RBAC, default deny and fixed parent inheritance. No policy
  language, explicit deny rules, external authorization service or graph engine.
- No SCIM, IdP group synchronization, nested groups, custom row-policy editor,
  mandatory team, ownership transfer or new telemetry collection concept.
- Keep implementation to at most two outcome-complete tasks, each with its own
  proof. Do not create a spec or task files until design is settled.

## Proposed implementation

The maintainer has locked the model and struct responsibilities recorded under
Decisions and in the Rust walkthrough. Private mechanics remain implementation-
owned. Nothing here is approved for implementation.

The [Rust model and token walkthrough](rust-model.md) records proposed structs,
scope matching, token claims and the complete request path. Its representations
are design sketches, not additional production types already present in Wyrd.

### Identity and login

The managed principals in this design are **User** and **Service**. Admin is
permission authority, not a new account type in the Access UI. Existing platform
bootstrap and internal SYSTEM mechanics remain their existing separate concerns;
they are not additional assignable tenant principals introduced by this change.
An SDK, CLI, OAuth client,
browser session, API key, workload token or Agent Card is not another person or
account. Registering a Card never creates an account or grants authority.

#### Preserve Service family identity and exact-version attribution

The maintainer explicitly requires preserving the existing flow. A runtime
Service account is created once with a stable principal ID, credential and
`workload_restriction = Family(Service, card_name)`. There is no version or exact
Card UID in that family association. An application loads an exact Service release;
the SDK supplies its UID as `workload_uid` through the existing token exchange or
validated request selector. The server validates that root against the account's
family and resolves its exact attribution graph. The same account and credential
serve later versions; declaring a release creates no account and rotates no key.

Keep these distinct existing responsibilities:

| Responsibility | Value/source |
|---|---|
| Who is calling | User or Service principal; stable account identity |
| Which workload family a runtime Service may act as | Existing WorkloadRestriction, e.g. Service/checkout |
| Which exact release is running | Existing workload_uid supplied by the loaded application and validated by the server |
| What operations/resources it may access | Role grants, revised into explicit scoped bindings |

The family restriction does not itself grant read/write/run authority and does
not mean that every dependency read must target that same Service Card. Explicit
role bindings can grant the Service access to required Models, Verifiers,
Repositories, tables or deployments. Such a grant cannot permit it to impersonate
another workload family through workload_uid. The existing one-live-runtime-
account-per-family constraint and signed-root mismatch checks remain intact.

Preserving attribution is a mandatory invariant of this revision: owning workload
Card, subject Card, exact Verifier, workload_uid, stable principal and actor chain
retain their existing meanings. A Repository parent used to filter evidence
access never replaces the owning workload root or rewrites evidence attribution.
The revised authorization structure must fit those existing semantics.

Reuse `WorkloadFamily` and `WorkloadRestriction`; do not replace them with the
general PermissionScope, add another workload identity, require a version at
account creation, or invent another token exchange. Cached current grants revise
authorization only. This design does not remove the existing unrestricted
automation capability or reopen that unrelated account policy. The ordinary
runtime-account UI journey below selects the named Service family.

Evidence: [existing family types](../../../../crates/wyrd-spec/src/auth/workload.rs),
[creation](../../../../crates/wyrd/wyrd-server/src/components/principals/routes.rs),
[root validation](../../../../crates/wyrd/wyrd-auth/src/card_scope.rs),
[issuance and selectors](../../../../crates/wyrd/wyrd-auth/src/issuance.rs),
[SDK service context](../../../../crates/shared/wyrd-client/src/auth.rs),
[configured Service authority](../../../../architecture/wyrd-design.md#configured-service-context).

| Caller | Authentication journey | Result |
|---|---|---|
| Human in the UI | Local password; optionally configured OIDC | Existing Rust-owned opaque browser session |
| Human using CLI or SDK interactively | Existing device authorization; approve the code after local password login or optional SSO | Short-lived access token and rotating human refresh token |
| Human script | Personal API key with explicit expiry and ceiling, exchanged at the token endpoint | Short-lived access token attributed to that User and key |
| CI, deployed service or autonomous agent | Explicit Service account and API-key exchange | Short-lived access token attributed to that account and key |
| Federated workload | Optional trusted issuer plus explicit binding to a Service account | Same Wyrd account and authorization path |
| Agent acting for a person | Existing RFC 8693 subject/actor exchange | Delegated token with intersected authority and actor attribution |

Reuse browser cookies, password handling, provider validation, key hashing,
five-minute tenant access tokens, refresh rotation and device-code protections.
Node renders and forwards; Rust authenticates and issues credentials. Browser
JavaScript does not receive CLI tokens or hold API credentials for ordinary UI
requests. Keep exact-origin checks on browser mutations and explicit confirmation
of the device's user code. A recovery-key session is not fresh human proof for
issuing personal keys or approving a new human login.

There is a verified gap: `CliLogins::authorize` requires an active OIDC connection,
and `approve` begins provider login. Extend this owner so the same device flow
can be approved by a locally authenticated human. Persist the local password
revision or provider connection revision as the login's actual authentication
anchor; never fabricate a provider connection for local users. Redemption,
refresh, password reset and connection invalidation check the appropriate anchor.
Keep one-use redemption, expiry, polling, denial and refresh replay detection.

### Roles, groups and bindings

Reuse the existing role store and closed operation catalog. Replace assignments
that conflate reusable roles and object scope with bindings:

```text
subject: User Alice | Service account ci | Group reviewers
role: viewer | editor | workload | approver | auditor | admin | custom role
scope: Tenant | Card family | exact Card version | supported domain object
```

Groups are optional tenant-local rosters of Users and Service accounts. A small
installation binds directly to accounts and never creates a group. No nesting,
ownership or implicit default membership. Only tenant administrators manage
groups, roles and bindings in this first version; roster editing is itself access
administration. Do not introduce a group-manager permission model.

| Scope | Meaning |
|---|---|
| Tenant | All tenant objects for the granted operations |
| Card family | Tenant + kind + name; every version, including future versions |
| Exact Card version | One exact `CardUid` |
| Bifrost schema/table | Existing schema boundary or immutable table UID |
| Gateway deployment/model | Preserve existing deployment/model scope and explicit-grant deployment rules |
| GitHub connection | One connection's supported administrative/use operations |
| Existing exact tool | Preserve the current built-in tool scope; no Tool Card or new tool authorization system |

Family is the default for ongoing work; exact version is explicit limited sharing.
Use the existing family identity tuple rather than require a new public identity
or mandatory owning group. Keep family grants across versions and retained
history. Deletion does not erase the authorization identity needed for evidence;
reusing the same kind/name remains the same access family. Exact-version grants
never follow a recreated version with a different UID.

Scope applicability is fixed by the server's operation catalog. A binding grants
only role operations applicable to that scope and its defined descendants. For
example, `viewer @ Repository family A` grants Repository/evidence reads and
secured queries for A; it grants no gateway, connection or arbitrary user-table
access. Reject unknown scopes, cross-tenant references, bindings with no applicable
operations, and administrative roles at resource scope. Expose the effective
operations in validation and access summaries so this rule is visible.

Agreed built-ins:

| Role | Intended authority within its scope |
|---|---|
| `viewer` | Read resources and their derived records; query permitted data; no audit history or gateway captured payloads |
| `editor` | Viewer plus authoring, verification binding management, execution and cancellation; no access administration or approval decisions |
| `workload` | Runtime context reads, verification execution, authorized ingestion and gateway invocation; no authoring, approval, audit or captured payload reads |
| `approver` | Read the relevant execution and submit approval decisions; does not grant execution or authoring |
| `auditor` | Tenant audit-history read; deliberately tenant-wide and sensitive; no implicit ordinary data or payload access |
| `admin` | All tenant operations, including access administration; tenant scope only |

Roles are bundles, not an ordered ladder. `workload` must still support a real
running Service; reducing it to append-only would break normal verification and
gateway journeys. Custom roles reuse the existing closed operation vocabulary
for narrower cases, such as deployment invocation or payload access. Do not add
specialist built-ins unless a demonstrated journey needs them.

No automatic viewer fallback for a newly configured trusted workload issuer.
Authentication alone grants nothing. Existing explicitly configured issuer grants
must be inventoried rather than silently removed. Existing optional OIDC
group-to-role mapping may project source-qualified bindings at login; it is not
local group membership, provisioning or continuous offboarding. Preserve direct
bindings when replacing provider-derived ones. Document that provider-only
changes remain unknown until Wyrd receives another verified login/update; local
suspension is the available immediate administrative control.

Creating a new Card family requires tenant-scoped authoring authority; an editor
bound to an existing family can register its later versions, but cannot create
another family. Do not silently grant resource editors tenant creation.
Protect the existing bootstrap/recovery path and the last usable tenant admin.

### Effective authority and freshness

For a request, authenticate its tenant and account, then compute:

```text
current direct/group/provider bindings
INTERSECT credential ceilings
INTERSECT every delegation participant's current authority and signed ceilings
INTERSECT accepted execution authority, when acting for an execution
AND audience, account state and workload-attribution restrictions
```

Binding grants add together. Required workflow permissions are checked together.
A narrower binding never subtracts a broader grant. `card_access` remains an
attribution restriction, not a read grant. Neither SYSTEM nor an actor identity
adds authority to the subject it serves. Preserve delegation depth, subject/actor
validation and representable scope intersections; refuse an exchange that would
widen access. Personal key ceilings are typed, explicit and bounded by authority
at creation; later account grants cannot silently enlarge the key.

Current server state replaces the token's permission claim as continuing
authority. Signature verification stays synchronous and store-free; the server
auth owner acquires authorization state before calling the synchronous checker.
Do not add SQL or network access inside `PermissionCheck` or shared auth crates.

**Request-cost decision:** cache already-resolved account permissions, not just
raw roles and bindings. A warm request verifies its token, looks up that account
view in process memory and checks the requested operation/resource against it
and any credential ceilings. It never asks the user to sign in again, reloads
roles from PostgreSQL or expands group membership merely because another request
arrived. Build the account view on first use and rebuild when its authorization
inputs change. Background confirmation of an unchanged revision renews freshness
without recomputing permissions. Miss/invalidation handling is exceptional, not
the normal per-request derivation path. Existing database-backed browser-session
validation remains a distinct authentication cost; this choice does not remove it.

Use one immutable authorization snapshot per active tenant, shared by requests.
It contains current bindings, roles, memberships, relevant principal/credential
state and indexes needed for scope checks. Reuse existing cache/listener patterns.
One tenant authorization revision advances atomically with every access-affecting
mutation, including existing role, key, principal and provider-mapping mutations.
Notifications accelerate invalidation; periodic background revision confirmation
catches lost notifications. No second revocation revision or client consistency
token is needed for the first version.

Healthy replicas refresh within five seconds of committed changes. A replica
that observes a newer revision must refresh before authorizing; it cannot use a
known-obsolete snapshot. The mutation-serving replica applies that rule before
returning a success that subsequent local reads could contradict. Other replicas
have the documented propagation window.

**Maintainer-approved outage rule:** when freshness cannot be confirmed, an
already-loaded snapshot can authorize for at most five minutes since its last
successful database confirmation. Failed refreshes, requests and token renewals
do not reset that clock. After expiry, or with no snapshot or a known newer
revision, refuse with an unavailable-state error. Existing known revocations
remain refused. This permits unseen removals to remain effective for up to five
minutes during a partition; it does not promise five-second global revocation
during an outage. Browser-session authentication and other operations that
already require PostgreSQL may fail sooner; this is not a whole-product outage
availability guarantee.

Authorize each new read/action against this state. Long streams revalidate before
releasing subsequent batches and stop when authority or freshness is lost; a
permission change does not retract bytes already delivered. In-flight computation
does not gain new permissions. Preserve existing cancellation on principal
revocation and accepted-authority ceilings across queued work, retries and restart.
Rechecking protected downstream actions must never substitute broader live grants
for the authority under which execution was accepted.

### GitHub connection versus Repository Card — locked

The maintainer settled these as distinct access boundaries on 2026-11-19:

- GitHub connection configuration pages, integration inventory and corresponding
  UI/HTTP operations require tenant admin or explicit GitHub-configuration
  permission. Repository Card access alone grants none of those operations.
- Read access to a Repository Card includes its code-review results, review
  sets, member executions and retained inputs/outputs/evidence, consistently on
  UI, HTTP, MCP, SDK and SQL surfaces. Do not require GitHub configuration access
  or independent read grants on each referenced Verifier to inspect that data.
  Existing explicit sensitive-content restrictions still apply.
- Reading the Repository does not grant starting a fresh review, changing policy,
  registering versions, configuring the connection, approval or arbitrary GitHub
  API/source access. Those remain their own operation checks.

The actual GitHub repository is provider identity: verified host plus stable
repository ID. The installation association and current provider selection govern
what Wyrd can reach. Connecting a repository creates no Card, account or grant.
Before Card registration it appears only in the restricted integration inventory;
no additional provider-repository permission scope is needed for that inventory.

The Repository Card supplies Wyrd's versioned identity and review policy. A binding
on `Card Family { kind: Repository, name: payments }` grants the corresponding
read behavior across its versions. An exact-version binding grants only that
version and its derived records. The server uses the recorded Repository parent
for every derived read. A caller supplying only provider ID/PR number must still
be resolved to that trusted parent before anything is disclosed.

Registration verifies provider identity through the connection. Preserve the
existing refusal of a second family for the same provider repository and the
existing family through a GitHub rename. A family grant is not a grant on a
mutable GitHub owner/name string.

An authorized Repository workflow may use the connection internally without
exposing connection administration to the caller. New provider work still needs
current installation access and its Wyrd action permissions. PR work retains the
selected declaration's accountable authority; webhooks use signature validation
and existing admission, never a new user identity or SYSTEM authority fallback.
Retained review reads need no GitHub call. Disconnect fences new provider work
under the existing contract but does not erase retained reviews or otherwise
valid Wyrd read grants. References expanded outside retained review content,
such as opening another Card's specification, require their own access check.

The current GitHub configuration owner uses `identity_connections:write`.
The final closed operation/scope mapping must express GitHub-specific configure
authority without accidentally giving a GitHub-only configurator OIDC management
or tenant data access. Keep the existing association owner and provider identity;
do not add another credential or repository store.

Authorities: [Repository](../../../../architecture/wyrd-design.md#repository),
[GitHub association](../../../../architecture/wyrd-security-posture.md#github-app-association).
These locked design decisions are inputs to later authority/specification work,
not approval to change production behavior now.

### Derived data and SQL

Record a trusted authorization parent when accepting work:

| Data | Parent used for access |
|---|---|
| Ordinary Card verification | Exact subject Card |
| Direct verification with no subject | Exact Verifier Card |
| Repository review set and every member execution | Exact Repository Card selected at admission |
| Inputs, outputs, results, evidence, approvals, publication bytes | Same parent as the execution |
| Gateway call records | Existing gateway deployment boundary, plus payload-read permission when content is accessed |
| Audit history | Explicit tenant audit-read authority |
| Ordinary user tables and unattributed logs/traces | Explicit schema/table authority; no inferred Card ownership |

The parent is server-derived from accepted identities, survives coordination-row
cleanup, and remains usable after Card deletion. Never infer it from a shared
Verifier, a caller-supplied column, mutable label or nullable correlation field.
Cross-Card references confer no authority; expanding a referenced Card, trace or
file requires its own applicable check. Reading an execution need not reveal the
specification or secrets of its referenced Verifier. Approval needs both its
Verifier approve operation and visibility of the execution's parent.

Dedicated reads check the parent and operation. Lists, totals, search and
pagination apply the same visibility predicate before counting or selecting a
page. Inaccessible exact objects use ordinary not-found behavior. No API or UI
may fetch a broad page and hide unauthorized rows afterwards.

Oracle must secure managed verification/evidence scans before aggregation or
user expressions. A family-scoped viewer can query matching verification rows;
a schema/table query grant alone cannot expose other families' rows. In mixed
queries, every source still requires its own applicable grant. Ordinary user
tables remain table-scoped: granting one deliberately exposes its rows. We do
not claim arbitrary user-table row isolation or private per-Card telemetry from
nullable attribution columns.

Enforce the managed-row boundary in all providers: memtables, Parquet, views,
joins, subqueries and peer execution. Do not trust SQL text rewriting or a
planner predicate that an optimizer can move after a user expression. Internal
fixed SYSTEM reads returning caller-visible data enforce the same caller parent
boundary. Gateway payload restrictions also apply to predicates and other uses,
not only selected columns. Audit read remains a separate tenant operation;
Card viewer never implies it. Protect metadata, statistics and EXPLAIN from
revealing hidden row counts or storage access paths.

Bind distributed plans, reusable query state and pagination to tenant, effective
authority and authorization revision. Never reuse another caller's visibility;
on relevant changes restart pagination or cancel a stream rather than widen it.
Missing or unprovable parents are unavailable to ordinary readers, including raw
SQL. Administrative repair must establish the parent; admin permission alone
does not turn an unknown parent into a normal readable row.

### Administration and the connected user journey

#### Agreed Access UI and requesting access

Agreed by the maintainer on 2026-11-19. The admin can create a User or Service
through the UI and assign role bindings with explicit Card-family or other
supported scopes. Reuse the existing HTTP/CLI-backed server owners. This does
not add an access-request service or a different UI-only authentication flow.

Use one tenant Settings → Access area with Users, Service accounts, Roles and
optional Groups sections. Restrict account/role/group/binding administration to
tenant administrators in this first version, as already proposed. A separate
GitHub-configuration grant does not grant Access administration. Rust authorizes
every read/mutation; navigation and disabled buttons reflect the same server
authority. Reuse the existing UI patterns when implementation reaches this tree.

- Users: invite/manage supported account fields, suspend/resume and inspect
  effective grants. Do not add a second password or identity-management flow.
- Service accounts: create/manage/suspend accounts, inspect grants and use the
  existing credential issuance/rotation/revocation flow. Secrets remain one-time
  values; permissions and key restrictions are displayed separately. For a runtime
  Service, the form selects a Service Card family name and stores the existing
  workload_restriction; it never asks for a release version/UID. Display “Runtime
  family” separately from role/scope grants. The application supplies workload_uid.
- Roles: show built-in operation bundles read-only; create/edit/delete custom
  roles using the closed operation list, without policy text or JSON editing.
- Groups: optional local rosters and their bindings. Show that changing a roster
  changes inherited access; no group ownership or nested groups.

Use one grant form: choose account/group → role → explicit scope → Save. Require
the scope instead of silently defaulting to tenant-wide. Card family is the normal
Card choice, with exact-version access available explicitly. The form shows the
effective operations at that scope and the scope includes future versions when
family is selected. Deleting a binding removes that binding; inherited or broader
grants remain visible and effective.

An account's Access table shows role, scope and source (direct, group, provider).
Edit/remove only locally administered bindings at their owner; a group/provider
grant links to its source rather than suggesting that deleting a display row
overrides it. A compact effective-access view explains combined grants without
building a policy simulator. Custom-role edits show affected bindings/accounts;
prevent dangling references and preserve last-admin protection. No self-service
role/group edits or self-approval. All changes use existing audited server APIs
and invalidation, so UI, CLI, MCP and SDK remain equivalent.

For a user seeking access, start with an informational path: “Ask your Wyrd
administrator for access” and a Copy access request action. Copy the signed-in
account identity, operation, tenant context and the user's requested link/identifier;
allow the user to add their reason in their existing communication channel. This
does not send a message, persist a request, grant access or promise notification.
It requires no configured IdP, email, Slack, contact directory or new database table.

Preserve ordinary not-found behavior for inaccessible/unknown resource links.
The help action is generic for both, never a resource-existence probe. Do not
look up or disclose a hidden resource's name, members, owner, administrator roster
or repository inventory merely to assemble a request. When a resource is already
visible but an action is forbidden, explain that missing action without revealing
private grants. API errors provide corresponding safe remediation; the UI does
not become the only way to understand or administer access.

The administrator receives the user's message through their existing channel,
reviews the requested scope and creates a normal binding in Access. The user
retries after normal cache propagation, without signing in again. This is the
whole initial request journey. A durable in-app request inbox, approval states,
notifications, expiry and escalation are excluded unless maintainers select a
later demonstrated requirement.

This applies least-privilege administration and explicit scoped grants from
[Kubernetes RBAC good practices](https://kubernetes.io/docs/concepts/security/rbac-good-practices/)
and server-enforced checks/non-disclosure from
[OWASP authorization guidance](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html).
The specific tab layout and manual request path are Wyrd simplicity choices,
not external standards mandates. Journey proof must show restricted management
routes, direct versus inherited grants, no hidden-resource disclosure, safe role
edits, and access becoming usable after a normal grant without a new login.

Use existing auth/role/principal owners for typed group, membership, binding and
current-access operations, with thin SDK/CLI/MCP projections. Extend UI Access
settings as a consumer of those APIs. Reuse the central decision result for a
safe self-access summary; tenant admins can inspect another account's bindings.
Do not build a general policy simulator or a second UI authorization engine.

The three agreed system scenarios below define the connected user journeys;
HTTP, CLI and UI are consumers of the same backend authorization decisions.

Each actual permission decision stages audit on the existing Scribe outbox.
Record account/actor, credential ID where applicable, operation, resource,
decision and authorization revision. Audit storage failure never fails the
operation. A query records its secured-source decisions, not one audit per row.

### Agreed system scenarios

These three connected scenarios were agreed by the maintainer on 2026-11-19.
They exercise changes together; individual checks and failure variants belong
inside them rather than becoming a scenario per feature. No execution result
is claimed. Use the real Rust owners, WyrdTestServer and repository PostgreSQL;
production UI actions and real CLI/SDK calls must consume the same created
accounts, bindings, credentials and retained records. Local journeys configure
no IdP. Extend existing browser infrastructure rather than introduce UI mocks.

#### 1. An administrator gives a user access, and every surface respects it

An admin creates Alice through Access and assigns read access to Repository
family payments, directly or through a local group. Alice completes local sign-in
and approves CLI login without an IdP. Alice inspects payments and its code
reviews, review sets, member executions and evidence through the UI and HTTP/CLI.
Raw Bifrost queries return the same authorized data. A second Repository's objects,
rows and counts remain absent. Alice cannot configure GitHub or administer access.

Observe the persisted binding and the actual returned/listed/aggregated data,
not only the form's success notification. Family and exact-version grants,
role/group edits and cross-tenant requests are cases inside this journey. Existing
explicit payload restrictions and mutation permissions remain separate from read.
The HTTP/CLI-only variant creates and grants the account programmatically; the
browser variant must prove UI creation against those same backend contracts.

#### 2. An administrator provisions a Service that operates across releases with correct attribution

An admin creates checkout-runtime through HTTP/CLI, associates it with the
existing Service/checkout runtime family and grants its required operations on
actual dependencies. Access displays that same account, runtime family and grants.
Run checkout v1 and v2 with the same principal and credential through the real
SDK/execution/ingest paths. Retained evidence must identify each exact workload_uid
and preserve owning workload Card, subject Card, exact Verifier and subject/actor
attribution. Another family's UID and an ungranted operation are refused.

Observe unchanged account/credential IDs, distinct release attribution and absence
of unauthorized work. No release creates an account or rotates a key. Rootless
restricted work, signed-root mismatch and duplicate runtime-family claims remain
negative cases under the existing rules. Include the UI-created Service-account
variant, and use its returned-once credential in the same two-release journey;
UI must show runtime family separately from operation grants and never require
an exact release UID during account creation. Keep secrets out of logs/captures.

#### 3. Access changes affect existing sessions and credentials predictably

Alice encounters an inaccessible resource, copies an access request and uses the
team's existing communication channel. The admin grants access through HTTP/CLI;
Alice's existing session gains access after normal propagation. The admin changes
or removes the grant through Access; existing browser and bearer requests reflect
that change without re-login. A restricted key or delegated token remains within
its frozen ceiling even if the account receives broader grants.

Observe the same access decisions across callers, cached permission reuse between
changes, and refresh after changes rather than role derivation on every request.
Exercise healthy replicas and interrupted database connectivity within this
journey: known-invalidated state cannot allow, and unconfirmed state stops
allowing five minutes after its last database confirmation. Use bearer requests
to isolate that bound from database-backed browser authentication, which can fail
sooner. Preserve generic inaccessible/unknown responses when copying a request;
no message delivery, persistent approval queue or hidden-resource disclosure is
introduced. Audit uses the existing nonblocking outbox.

### Logical task grouping

The maintainer locked this two-task split on 2026-11-19 because
complete server enforcement is independently verifiable before the new admin UI
consumes it. These are design groupings, not ready task files or deployment phases.

| Group | Complete outcome and necessary consumers | Proof ownership |
|---|---|---|
| Revised authorization and resource enforcement | Typed roles/bindings/scopes and persistence; cached current authority and token ceilings; local device approval; HTTP/CLI/SDK/MCP administration and every ingress; Card/Repository/derived-evidence/Bifrost enforcement; preserved workload attribution and accepted execution authority; required contract/schema updates | Programmatic variants of all three scenarios, including actual retained data and refusal paths; narrow owner tests and hot-path wait checks |
| Admin UI and connected user journeys | Access account creation and management, roles/groups/scoped grants, accurate current-access/source displays, manual request guidance and local device-approval presentation; consume the first group's contracts rather than introduce another auth path | Full browser-to-server-to-CLI/SDK variants of the same three scenarios, including the UI-created Service across two releases and changes to existing sessions |

The UI group depends on the first group's working administration, authorization
and filtered-read contracts. The backend group includes any minimal adaptation
needed to keep existing consumers correct; it must not leave an alternate ingress
or raw query path permissive pending UI work. Each task owns its tests; the UI task
is not a deferred backend testing phase. Both tasks implement the revised contracts directly; there is no shipped-system
cutover or mixed-version support. Neither may leave an alternate authorization
path permissive. Rust/Python/TypeScript and MCP coverage exercise these same behaviors at
shipped boundaries. Final integrated review runs the complete journeys and
required verification without adding a third acceptance-only task.

### Owners and schema definitions

| Owner | Change it must carry |
|---|---|
| `wyrd-spec` | Typed bindings/scopes/ceilings, auth requests/responses and stable errors |
| `wyrd-runtime` | Existing operation vocabulary, role bundles, scope coverage and pure checker |
| `wyrd-auth`, shared issue/verify crates | Existing login/exchange flows; local device approval; token contract and server snapshot acquisition at the proper tier |
| `wyrd-sql` | Existing roles/principals/credentials; local groups/memberships, scoped bindings and authorization revision; local device/refresh authentication anchors |
| Server Card/verification/GitHub/storage owners | Parent capture, current checks, filtered listings and authenticated file delivery |
| Bifrost Gate/Scribe/Oracle and peer owners | Server-controlled retained parents and secured scans across storage/peer paths |
| `wyrd-client`, three SDKs, CLI, MCP, UI | One contract, scoped credentials/access administration, accurate visible counts |
| Tests, fixtures, examples, served schemas and docs | Replace broad-role assumptions and exercise local and optional-federated journeys |

Reuse existing assignment tables if they can represent bindings cleanly; replace
superseded assignments rather than maintain competing grant stores. The approved
spec must name concrete new/altered tables, constraints, indexes, RLS and login
anchor changes before schema implementation. New tenant tables lead with
`data_tenant_id`, force RLS and use tenant-qualified references. PostgreSQL stores
control metadata only; Bifrost owns retained evidence, inputs and audit.

The maintainer clarified that nothing has shipped. Implement the revised schema,
serialization, callers and fixtures directly. Do not add existing-grant conversion,
historical-parent backfill, credential reissue, rollout/cutover, rollback or dual
contract support. This does not authorize editing an applied migration or resetting
a database. Normal credential rotation, retained evidence and execution recovery
remain product behavior, not upgrade work.

The local CLI decision is to extend device approval using exactly one local
password-revision or provider connection-revision anchor, retained through refresh.
No synthetic OIDC connection or new password OAuth grant is introduced.

The invitation decision is to store explicit role-and-scope assignment intents,
then revalidate issuer authority and every role/scope at one-use acceptance.
User creation and binding creation succeed together; invalid or scope-less
invitations create neither. A missing scope never means tenant-wide access.

## Evidence and rationale

Verified on the working tree on 2026-11-19, after consulting CodeGraph:

- [Permission scopes](../../../../crates/wyrd-spec/src/auth/permission_scope.rs)
  include Bifrost, gateway, exact Verifier and tool scopes, but no general Card
  family/version scope. Reuse the typed-domain approach.
- [RbacCheck](../../../../crates/shared/wyrd-runtime/src/permission_check.rs)
  checks the effective permission set synchronously. Keep that single checker.
- [Built-in roles](../../../../crates/shared/wyrd-runtime/src/builtin_roles.rs)
  currently include audit/payload reads in viewer and workload, and default
  trusted issuers to viewer. Those are deliberate privilege-change proposals.
- [CLI login owner](../../../../crates/wyrd/wyrd-auth/src/cli_logins.rs)
  explicitly requires an active connection and provider-bound approval. Local
  password device login is missing; it cannot be claimed as existing support.
- [Card routes](../../../../crates/wyrd/wyrd-server/src/components/cards/routes.rs)
  authorize a broad Card read before listing. [Execution inspection](../../../../crates/wyrd/wyrd-server/src/components/verification/service/inspection.rs)
  checks exact Verifier access and a generic Card read for PR records.
- [Oracle](../../../../crates/vala/vala-bifrost-redux/src/oracle/mod.rs)
  authorizes every resolved table and separately checks gateway payload columns.
  That reusable admission seam does not itself implement Card-row isolation.

Primary sources support the chosen building blocks, not Wyrd-specific timing
numbers or an assertion that other systems share our outage behavior:

- [Kubernetes RBAC](https://kubernetes.io/docs/reference/access-authn-authz/rbac/)
  separates roles and bindings and uses additive grants without deny rules.
- [GitHub teams](https://docs.github.com/en/organizations/organizing-members-into-teams/about-teams)
  illustrate group-based repository sharing with optional identity-provider sync.
  This design adopts local grouping, not GitHub's nesting or ownership machinery.
- [OWASP authorization guidance](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html)
  supports least privilege, default deny and checking every request. RBAC plus
  Wyrd's fixed parent boundary is our fit decision, not an OWASP RBAC mandate.
- [Kubernetes RBAC good practices](https://kubernetes.io/docs/concepts/security/rbac-good-practices/)
  explains why role/binding administration itself needs protection from escalation.
- [RFC 8628](https://www.rfc-editor.org/rfc/rfc8628.html)
  supplies the device grant; using Wyrd's local password instead of requiring an
  upstream IdP is a Wyrd integration choice.
- [RFC 9700](https://www.rfc-editor.org/rfc/rfc9700.html)
  supplies OAuth security guidance, including public-client refresh replay
  protection. [RFC 8693](https://www.rfc-editor.org/rfc/rfc8693.html) supplies token
  exchange vocabulary; the exact current-authority intersection is Wyrd policy.
- [OWASP session guidance](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
  supports preserving opaque protected browser sessions.
- [PostgreSQL row security](https://www.postgresql.org/docs/current/ddl-rowsecurity.html)
  demonstrates filtering before user expressions. Bifrost needs its own equivalent
  enforcement in its existing providers; PostgreSQL RLS cannot protect Parquet.

The five-second healthy propagation and five-minute outage bound are product
choices requiring executable proof, not universal standards guarantees. The
source supplement's blanket claims about other products serving stale authority
were not established and are not relied on here.

## Decisions and alternatives

Maintainer decisions in this conversation:

- 2026-11-19: allow already-loaded permissions for at most five minutes since
  database confirmation during outage; known-invalidated state cannot allow.
- 2026-11-19: agree to extending existing device login so local password users
  can approve CLI login, with optional SSO and no UI OAuth client.
- 2026-11-19: aligned with the smaller scoped-RBAC revision described here:
  optional local groups, family-scoped sharing, inherited verification-data
  protection, current server grants and ordinary table-scoped user data. This
  includes the agreed role separation. The later no-shipped-system clarification
  removes the earlier assumed deployment-transition work.

- 2026-11-19: aligned with the proposed Rust responsibilities and replacing the
  authoritative JWT permissions claim with optional restrictive ceilings;
  requested concrete binding examples and analysis of existing dependencies.
- 2026-11-19: locked the token-field decision: remove authoritative `permissions`
  and informational `roles`; retain identity, validity, credential/actor and
  workload-context fields; add an explicit contract version and optional signed
  `permission_ceiling`. Ordinary tokens omit the ceiling; restricted and
  delegated tokens preserve their limits. This locks that design decision,
  not implementation approval or finalization of the remaining persistence work.
- 2026-11-19: locked the Repository access boundary: GitHub setup/inventory and
  UI/HTTP configuration routes require admin or explicit GitHub-configuration
  permission; Repository Card read covers its code reviews, review sets, member
  executions and evidence without GitHub configuration or separate Verifier-read
  grants. Fresh execution, policy changes and other mutations remain separate.
- 2026-11-19: agreed the simple Access UI and copyable/manual access-request path;
  required a real UI-driven creation/grant journey for Users and Services.
- 2026-11-19: preserve User/Service as managed principal kinds and the existing
  Service family restriction plus exact workload_uid attribution. No per-version
  account, new principal kind, new workload identity or new authentication flow.
- 2026-11-19: locked the decision-complete distinction: retain WorkloadRestriction
  and WorkloadFamily for permitted runtime identity; use CardPermissionScope for
  operation target scope, with direct kind/name fields rather than a new generic
  CardFamily wrapper. User and Service share one role/binding/permission checker.
  Preserve existing owning Cards, subject/Verifier context, principal/actor
  attribution, tenant isolation, delegation narrowing and nonblocking audit.
  A trusted access parent never substitutes for runtime ownership or attribution.
- 2026-11-19: agreed three connected system scenarios spanning UI and HTTP/CLI:
  user access across surfaces; Service operation across releases with preserved
  attribution; access changes under existing sessions and restricted credentials.
  Permitted splitting into two tasks where needed. The logical grouping above
  recommends complete backend enforcement, then its admin UI consumer.
- 2026-11-19: locked the two-task split: complete backend authorization and
  enforcement first; admin UI consuming those contracts second. Both tasks own
  their proof against the same three scenarios. No further product decisions
  are currently identified; bounded technical closure below remains required.

[rust-model.md](rust-model.md) now maps account/role/family examples to binding
records and explains the current claim's purpose, affected consumers, and the
required preservation of accepted execution ceilings. Overall model alignment
is not specification or implementation approval.

Compared with the source proposals, remove mandatory team ownership, SCIM,
source-expiring synchronized groups, team telemetry collections, arbitrary
user-table row modes, two authorization revision types, configurable policy
engines and invented capacity quotas. None is needed for the stated journey.
Keep raw SQL enforcement: dropping it would leave a demonstrated access bypass.

Keeping signed permission snapshots as sole authority is smaller, but leaves
grant changes effective only on issuance/expiry and makes resource-scale grants
and shared current filtering harder. Current-state caching costs invalidation and
outage handling; it is recommended because all clients need one resource model.
Strict five-second expiry would bound outage revocation more tightly at a direct
availability cost; the maintainer selected the five-minute outage bound.

The maintainer's latest correction supersedes the earlier upgrade assumption:
no shipped-system migration exists. Specification revision 2 records direct local
CLI anchors and scoped invitation acceptance as explicit decisions, while retaining
the locked identity, attribution and permission model.

## Architecture alignment

The maintainer authorized reconciliation of the agreed model on 2026-11-19.
The owning architecture now states these replacements directly:

- [Security posture](../../../../architecture/wyrd-security-posture.md#authorization-and-policy):
  operation bundles/scoped bindings, cached current authority and freshness,
  restrictive token/accepted ceilings, device anchors, scoped invitations,
  administration and distinct GitHub configuration authority.
- [Bifrost public surface](../../../../architecture/bifrost-design.md#public-surface):
  source authority plus managed-row visibility, retained parents and distributed
  context, including streaming freshness and payload/audit restrictions.
- [Runtime identity](../../../../architecture/wyrd-design.md#runtime-identity) and
  [Repository](../../../../architecture/wyrd-design.md#repository): preserved
  Service-family restriction and exact workload attribution, retained parent reads,
  approval boundaries and separation from provider configuration.

This satisfies the architecture prerequisite for planning. It makes no claim
that implementation already follows the revised authority; no exception or
alternate authorization path is granted.

## Open questions

The social-login addition has no remaining material product decisions; its
finalized supplement records the agreement and implementation-owned details.
The settled-scope statement below describes specification revision 2 only.

No further product decisions are currently identified. The model, principal and
attribution semantics, token responsibilities, roles, access boundaries, UI,
three system scenarios and two-task split are settled. Do not reopen them to
choose ordinary private coding mechanics.

The specification names the bounded persistence changes and preservation rules.
Inspection established that device approvals and human refresh rows currently
bind provider connections, invitations currently store role IDs without scopes,
personal keys already have JSONB permission ceilings constrained to User owners,
and accepted execution authority is already serialized server-side. The six
Bifrost verification tables have existing schema/writer owners; nullable workload
and subject correlation columns do not by themselves establish the new read parent.

Sources: [device approval schema](../../../../crates/wyrd/wyrd-sql/migrations/20261002000000_auth_device_authorizations.sql),
[human refresh binding](../../../../crates/wyrd/wyrd-sql/src/row_types/auth/human_connections.rs),
[invitations and key ceilings](../../../../crates/wyrd/wyrd-sql/migrations/20261102000000_local_users_personal_keys.sql),
[execution authority](../../../../crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs),
[verification tables](../../../../crates/vala/vala-bifrost-redux/src/tables/verification/mod.rs),
[correlation columns](../../../../crates/vala/vala-bifrost-redux/src/tables/managed_columns.rs).

Per-callsite integration, physical names/layout and final scope-compiler wiring
remain implementation-owned within the specification's boundaries. Repository
inspection is not runtime verification; no schema implementation was executed.
Known unprovable retained parents stay hidden under the agreed rule. If concrete
evidence shows that agreed behavior cannot be preserved or that an additional
persistence change is needed, report that conflict for specification revision;
do not guess an owner, widen a grant or silently redesign.

## Handoff

Finalized design is handed to approved specification revision 2. No task,
implementation, schema change, commit or runtime-review PASS is produced. Intended
planning shape is the two logical groups above, with connected proof in each and
no task files until specification approval. Specification revision 2 is approved
and architecture reconciliation is recorded; planning may proceed.

Proof must exercise the real caller path: local signup/login → device approval
or scoped key exchange → SDK/CLI/MCP/UI operation → server check → retained
evidence → direct/list/raw-SQL read with identical visibility. Use two Repositories,
a shared Verifier, a human, a Service account, group removal and cross-tenant IDs.
Prove unauthorized rows contribute nothing to aggregates and cannot be reached
through peers, joins, alternate providers, SYSTEM reads or file downloads.

Cover exact versus future-family versions, personal-key narrowing, delegation,
approval separation, local password-reset invalidation, optional OIDC/workload
flows, no implicit issuer grant, lost notifications, known invalidation, snapshot
expiry, database recovery and revocation while streaming. The five-minute outage
test must not accidentally depend on a fresh database-backed browser session.

Use repository PostgreSQL and `WyrdTestServer` journeys on shipped surfaces.
Existing runners include `test:identity:journey` for optional providers and
`test:bifrost`; local-auth proof must also run with no IdP configured. Discover
the narrow owner lanes and exact named-test commands during implementation.
Regenerate served contracts and update SDK exports/stubs, fixtures and docs;
run appropriate format, lints, codegen and tenant-boundary checks.

Hot-path wait inventory for later review:

| Added/moved wait | Sanctioned boundary |
|---|---|
| Snapshot miss or known invalidation reload | Permission check; one batched PostgreSQL reload before the pure checker |
| Warm direct verification/query/append | No added wait; process memory |
| Row filtering and batch revalidation | Process-memory authorization; no per-row SQL |
| Audit | Stage on existing Scribe outbox; never await audit durability |
| Revision reconciliation | Background control-plane work |
| Group/binding changes and local device approval | Existing non-hot authentication/administration transactions |

Do not add object-storage, Vault or Scribe waits for authorization. Preserve
existing sanctioned IO and execution admission durability. Later hot-path review
must enumerate actual added/moved waits.
