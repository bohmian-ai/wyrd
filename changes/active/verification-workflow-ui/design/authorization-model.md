# Authorization model

Status: historical exploration — superseded by
[authorization-model's approved specification](../../authorization-model/spec.md)
and this packet's [revision-8 resumption agreement](resume-005-007.md).
The findings/questions below describe the 2026-11-19 stopping point, not current
permissions or unresolved decisions for tasks 005–007. Preserve them as research
provenance; use the current owning security/Bifrost contracts for implementation.

## Why this exists

TASK-005A delivered PR review sets: one round of checks on one pull request,
admitted under one Repository Card's bindings. TASK-005 shows them in the UI.
A user who has no access to that Repository must not see its review sets,
their member executions, counts, or the Bifrost rows behind them.

Wyrd cannot express that today:

- `cards:read` has only tenant-wide scope
  (`crates/wyrd-spec/src/auth/permission_scope.rs:45`). A user reads every
  Card in the tenant or none.
- Bifrost checks one table grant per query, not each row's owner
  (`crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3820`). Anyone who can
  query `bifrost.verification.execution_inputs` reads every review set.
- Execution inspection checks Verifier access, never the owning Repository
  (`crates/wyrd/wyrd-server/src/components/verification/service/inspection.rs:521`).

Fixing this properly means choosing Wyrd's scoped-access model for every
domain (Cards, verification, Bifrost, gateway, GitHub, admin), not adding one
check.

## The proposal in plain terms

A read-only Codex review produced the full proposal in the appendix. In
short:

- **Model:** role-based access with typed resource scopes, plus fixed parent
  inheritance. Not attribute-based (ABAC), not a policy language, not an
  external authorization service.
- **Roles** are bundles of operations. A **binding** gives a role to a
  subject at a scope: the tenant, one Card family (every version), one exact
  Card version, a Bifrost table, a gateway deployment, a GitHub connection.
- **Inheritance:** executions, review sets, results, evidence and their
  Bifrost rows record the Card version they belong to. Reading them requires
  access to that Card. A Verifier grant never unlocks a Repository's data.
- **Lists, counts and SQL** are filtered before results are built, including
  raw Bifrost SQL (row-level security, as in PostgreSQL RLS, Snowflake row
  access policies, BigQuery row-level policies).
- **Allow-only, default deny.** No deny rules. An "explain access" operation
  shows which binding allowed or why a request was denied.

The proposal also assumes two things that do not hold for Wyrd and must be
corrected (questions 1 and 2): that every Card has a mandatory owning team,
and that an identity provider exists.

## Open questions

Each question gives the context, what comparable systems do, and a
recommendation.

### 1. Do we have groups at all, and what are they?

Teams were removed from Wyrd because they created too much overhead. Today
Cards carry no team; `labels.team` in `wyrd.toml` is free metadata with no
authorization meaning. The proposal reintroduces a mandatory owning team on
every Card family.

Options:

- **a. No groups.** Bind roles directly to users and service accounts.
  Simplest; every grant names a person.
- **b. Groups as binding subjects only.** A group is a name and a list of
  users and service accounts. Cards never reference it and none is
  required. A small install never creates one.
- **c. Groups that own Cards** (the proposal). Every Card family has an
  owning group.

Standard: GitHub teams and Grafana teams are local objects that grant
access (option b), with optional sync from SSO. Backstage requires an owner
group on every catalog entity (option c).

Recommendation: **b**. It gives "grant this to the platform team" without
putting teams back on Cards.

### 2. Is "read any Card belonging to team X" required?

That rule needs a field on the Card saying which group it belongs to. With
option 1b alone, a group gets access per Card family (as GitHub grants a team
access per repository), not to "everything the team owns".

Options:

- **a. Per-family grants only.** No ownership field.
- **b. Optional owning group on a Card family.** Unset by default; unset
  means only tenant-wide and per-family grants apply.

Labels must never grant access (standard in Kubernetes and the proposal),
because a Card author could change them to gain or remove access.

Recommendation: decide whether the belongs-to-team journey is real. If it is,
**b**; otherwise **a**.

### 3. Wyrd has no assumed identity provider. How do groups get members?

Wyrd runs with local password users, invitations, service accounts and API
keys, and SSO is optional (`architecture/wyrd-security-posture.md:401`). Any
grouping must be Wyrd's own. The proposal's SCIM and OIDC group sync can only
be an optional add-on for tenants that configure SSO.

Recommendation: Wyrd-managed groups first; SSO group sync is a separate,
later decision.

### 4. What happens when PostgreSQL is unreachable?

The proposal moves permissions out of tokens into server memory, refreshed
from PostgreSQL, and **denies every request** once permissions are more than
five seconds old without confirmation. A 30-second database failover would
fail every SDK call, UI page and verification, including warm hot paths.

Standard: AWS IAM, GCP IAM (propagation up to about seven minutes),
Kubernetes, Zanzibar and SpiceDB keep serving the last known permissions
during an outage. Revocation is bounded by short-lived tokens plus an
emergency suspend.

Recommendation: refresh within seconds when healthy; during an outage keep
the last snapshot up to a maximum age (the access-token lifetime), then deny.

### 5. Do tokens keep carrying permissions?

Today a token's permission snapshot is the authority until it expires. The
proposal makes the server's current state the authority, so a removed grant
stops working within seconds instead of at token expiry. The cost is a
server-side permission cache per tenant and a coordinated cutover that
reissues every token and API key.

Recommendation: adopt the server-side authority, together with question 4's
outage rule.

### 6. Which built-in role changes are acceptable?

The proposal removes audit-log and captured gateway payload access from
`viewer` and `editor`, adds `auditor`, `gateway_operator`, `access_admin` and
(if groups exist) `team_manager`, and drops the automatic `viewer` grant for
trusted workload issuers (`crates/shared/wyrd-runtime/src/builtin_roles.rs:23`,
`:110`). Existing users lose access they have today unless re-granted.

Recommendation: accept; least-privilege defaults are standard. Confirm the
role list.

### 7. Version or family as the default grant target?

An exact `CardUid` pins one version, so every new Repository version would
need new grants. The proposal offers both family and version scopes.

Recommendation: family is the normal grant; version scope exists for sharing
one exact version.

### 8. How much of Bifrost is protected by rows?

Row filtering applies to every verification, evidence, approval, PR and
audit table, and to user tables that opt into Card-rooted mode. Existing
rows need their owning Card backfilled; rows that cannot be proven are
hidden from ordinary reads. This is the largest engineering piece and needs
its own benchmarks (`bench:l1:bifrost`, `bench:l1:verification`).

Recommendation: protect all verification and audit tables from the start;
decide whether user tables get Card-rooted mode now or later.

### 9. Delivery and TASK-005

The proposal is one breaking cutover across security posture, Bifrost and
Card design. It is its own change, not part of TASK-005A.

Recommendation: run it through `$wyrd-design` as its own change packet.
TASK-005's review-set read waits on it, or ships behind the current
tenant-wide `cards:read` and is narrowed when the new model lands.

## Appendix: Codex proposal (verbatim)

Produced by a read-only review on 2026-11-19. Not reviewed or approved.
Questions 1–4 above supersede its assumptions on teams, identity providers
and outage behavior.

### Wyrd authorization model proposal

**Adopt bounded, relationship-aware RBAC: reusable roles, explicit resource-scoped bindings, first-class teams, and fixed inheritance from resources to their derived data. Evaluate those rules synchronously against server-resident authorization snapshots. Apply the same rules before pagination, aggregation, and Bifrost query execution.**

Teams are subjects and ownership relationships, not namespaces. Card identity remains tenant, kind, name, and version. A Repository grant controls its review sets, member executions, results, evidence, and corresponding Bifrost rows.

This is a proposed replacement for parts of the current authorization contract. It requires approved revisions to the security and Bifrost authorities before implementation. It preserves one server implementation, portable deployments, tenant separation, synchronous permission evaluation, and nonblocking audit recording.

#### 1. Model decision

##### Comparison with established systems

| Model/system | Relevant practice | Wyrd decision |
|---|---|---|
| GitHub organizations, teams, and repository roles | Teams receive repository roles; organization administration and repository access are distinct capabilities. | Adopt team subjects and resource-scoped roles. Keep access administration distinct from data access. Do not adopt nested teams. [GitHub repository roles](https://docs.github.com/en/organizations/managing-user-access-to-your-organizations-repositories/managing-repository-roles/repository-roles-for-an-organization) |
| GCP IAM and resource hierarchy | Bindings apply at a resource and inherit to descendants. IAM Conditions add CEL expressions. | Adopt explicit inheritance and additive bindings. Reject CEL and configurable conditions. [GCP IAM](https://docs.cloud.google.com/iam/docs/overview), [IAM Conditions](https://docs.cloud.google.com/iam/docs/conditions-overview) |
| AWS IAM tags/ABAC | Principal attributes and resource tags can express access across matching resources. | Support the specific relationship “owned by team X,” using validated, server-managed ownership. Labels and arbitrary attributes never grant access. [AWS ABAC](https://docs.aws.amazon.com/IAM/latest/UserGuide/introduction_attribute-based-access-control.html) |
| Kubernetes RBAC | Reusable roles contain additive permissions; bindings determine subjects and scope. There are no deny rules. | Adopt this separation and allow-only semantics. Add Wyrd’s resource inheritance and analytical filtering. [Kubernetes RBAC](https://kubernetes.io/docs/reference/access-authn-authz/rbac/) |
| Zanzibar, SpiceDB, OpenFGA | Relationship models support shared authorization, reverse queries, and explicit consistency mechanisms. | Adopt fixed ownership/member/parent relationships and authorization revisions. Reject a general relationship schema and an external graph service. [Zanzibar](https://research.google/pubs/zanzibar-googles-consistent-global-authorization-system/), [SpiceDB consistency](https://authzed.com/docs/spicedb/concepts/consistency), [OpenFGA queries](https://openfga.dev/docs/interacting/relationship-queries) |
| Cedar / AWS Verified Permissions | Policies provide default deny and expressive permit/forbid evaluation. | Adopt default deny. Reject user-authored policy expressions, explicit forbid rules, and a request-time authorization service dependency. [Cedar semantics](https://docs.cedarpolicy.com/other/security.html), [Verified Permissions](https://docs.aws.amazon.com/verifiedpermissions/latest/userguide/authorization.html) |
| OPA | Rego can evaluate decisions and derive data filters through partial evaluation within a supported fragment. | Reject Rego and a second policy compiler. Wyrd will compile its fixed scope model directly into point checks and secured query scans. [OPA filtering](https://www.openpolicyagent.org/docs/filtering) |

This is a hybrid of RBAC and **bounded resource relationships**. It is not general ABAC and does not expose a Zanzibar-style modeling language.

The important relationship rules are fixed in Wyrd:

- A principal belongs to a team.
- A resource has one owning team.
- A binding grants a role to a principal or team at a scope.
- A derived object inherits its authorization root.
- A delegated credential restricts its ancestors’ authority.

Ownership alone grants nothing. For example, team X receives access to its Cards through an explicit binding:

```text
subject: team X
role: viewer
scope: resources owned by team X
```

##### Explicitly rejected

- **Tenant-only RBAC:** cannot express the required Repository or team boundaries.
- **Labels as ACLs:** author-controlled metadata must not silently change access.
- **Arbitrary conditions:** introduce policy-language operations, error semantics, and filtering equivalence problems without a demonstrated requirement.
- **General relationship graphs:** Wyrd does not need configurable relation definitions, recursive groups, or arbitrary graph traversal.
- **External authorization services:** add deployment machinery and a network dependency to permission decisions.
- **Token permissions as continuing authority:** prevent timely local offboarding and ownership-transfer enforcement.
- **Client-side filtering:** leaks counts, pagination, aggregates, and direct object reads.
- **Deny rules:** add precedence and explanation complexity. Credential ceilings and mandatory resource prerequisites provide restrictions without deny policies.

SpiceDB, OpenFGA, and OPA have relevant listing and filtering capabilities. Their rejection is about Wyrd’s operational fit, not an inability to solve authorization.

#### 2. Vocabulary, ownership, and hierarchy

##### Principals

Persist only the existing durable identity categories:

- **User:** a human identity.
- **Service account:** automation, CI, deployed Services, and agents.

The following are credential or execution forms, not additional durable principal kinds:

- **Delegated agent:** a user or service-account subject acting through an authenticated service-account actor chain.
- **Workload:** a service account using a signed workload credential.
- **Agent Card:** a versioned component being verified; it is not an identity.

Platform and tenant identities remain separate. Platform authority never implies tenant-data access.

##### Teams and IdP groups

A team is a tenant-local access-management object with an immutable `TeamId`.

Each team has exactly one membership source:

1. **Local:** Wyrd manages membership. Users and service accounts may be members.
2. **IdP-synced:** one configured issuer/provisioning source owns membership. Only linked users may be members.

Decisions:

- No nested teams.
- No team-to-team membership.
- No mixed local and synchronized membership in one team.
- IdP identity uses immutable issuer and external group IDs, never display names.
- Service accounts use local teams. They do not impersonate human IdP group membership.
- Renaming a team or external group does not change grants.
- Deleting a team is prohibited while it owns resources or has bindings.
- Team managers can manage a local roster, not role bindings, ownership, or custom roles.
- Assigning team-manager authority is an access-administration operation.

Managing a roster can grant access to everything shared with that team. The UI and audit record must state that consequence.

##### Synchronization and offboarding

**SCIM is the target provisioning integration.** Support Users and Groups, PATCH membership changes, pagination, deactivation, and version checks. Synchronization is tenant- and issuer-qualified.

OIDC group claims remain supported for installations without SCIM:

- Verified claims replace that source’s memberships atomically.
- OIDC-derived memberships expire at the earlier of the assertion expiry and five minutes after acceptance.
- Token refresh using Wyrd’s stored state does not renew that membership validity.
- Renewing membership requires a new verified IdP assertion.
- Unknown group mappings grant nothing.
- Direct local memberships and bindings remain independent.

Offboarding guarantees are measured from **Wyrd observing the change**:

- Local suspension: enforced within five seconds.
- SCIM deactivation or group removal: enforced within five seconds of commit.
- OIDC-only removal: enforced when a new assertion removes it, or when the previously accepted membership expires.

Wyrd cannot promise immediate detection of a change an IdP has not delivered. Emergency local suspension overrides every source and credential.

SCIM deactivation suspends the provisioned user, including direct local bindings and their API keys. Re-enabling requires an explicit provisioning or administrative action.

##### Resource hierarchy

```text
Tenant
├── Card family: kind + name
│   └── Exact Card version: CardUid
│       └── Derived objects rooted at that version
├── Bifrost schema/table
├── Gateway deployment
├── GitHub connection
├── Team telemetry collection
└── Identity and access-management objects
```

A team is not a parent namespace. `OwnedByTeam(X)` selects resources through their current ownership relationship.

Introduce an internal immutable `CardFamilyId` for a tenant/kind/name identity. This does not change `CardRef`, routes, names, or version identity.

##### Card ownership

Every registered Card family has exactly one owning team.

- First registration supplies `metadata.owningTeam`.
- The server validates the team and registration authority.
- Later versions inherit the family’s owner.
- A later version cannot select a different owner.
- The envelope projects the current owner; it is server-managed governance metadata.
- Ownership is outside the versioned specification and does not change content hashes.
- Labels and annotations have no authorization meaning.
- Ordinary relationships between Cards confer no access.

Ownership transfer is a dedicated operation available only to a tenant access administrator.

A transfer atomically:

1. Changes the family owner for all versions.
2. Removes explicit bindings scoped to that family or its versions.
3. Advances authorization revisions.
4. Records the removed bindings and old/new owners in audit.

Tenant-wide bindings remain effective. There is no “preserve shares” switch. Sharing after transfer requires explicit new bindings.

A deleted family retains an authorization tombstone while retained evidence references it. Re-registering the same kind/name reuses that family identity and its governance state.

##### Other ownable resources

Bifrost user tables, gateway deployments, and GitHub connections each have one owning team. Their ownership is independently governed; owning a Card does not automatically grant provider, connection, or table administration.

Schema ownership provides the default owner for newly created user tables. It does not override an existing table’s owner.

#### 3. Permissions, roles, and bindings

Separate the operation from where it is granted.

```rust
/// A registered operation; contains no subject or scope.
pub struct Permission {
    pub resource: PermissionResource,
    pub action: PermissionAction,
}

/// A fixed selector understood by both checks and data filtering.
pub enum PermissionScope {
    Tenant,
    Team(TeamId),
    OwnedByTeam(TeamId),
    CardFamily(CardFamilyId),
    CardVersion(CardUid),
    BifrostSchema(SchemaId),
    BifrostTable(TableId),
    GatewayDeployment(DeploymentId),
    GithubConnection(ConnectionId),
    McpTool {
        server: CardUid,
        name: String,
    },
}

/// A reusable collection of explicit operations.
pub struct Role {
    pub id: RoleId,
    pub name: String,
    pub permissions: Vec<Permission>,
    pub builtin: bool,
}

/// An additive grant within one tenant.
pub struct RoleBinding {
    pub id: RoleBindingId,
    pub subject: BindingSubject,
    pub role: RoleId,
    pub scope: PermissionScope,
}

/// Durable subjects eligible for bindings.
pub enum BindingSubject {
    User(UserId),
    ServiceAccount(ServiceAccountId),
    Team(TeamId),
}

/// A tenant-local membership boundary.
pub struct Team {
    pub id: TeamId,
    pub name: String,
    pub source: TeamMembershipSource,
}
```

All persisted records and references are tenant-qualified. These are contract sketches; implementation must provide the repository’s required rustdoc and validation.

`Team(X)` manages the team object and roster. `OwnedByTeam(X)` selects its resources. They are deliberately different scopes.

`McpTool` identifies a tool exposed by an exact Mcp Card. It does not introduce a Tool Card kind. Wyrd’s own MCP endpoints always authorize their underlying domain operations.

##### Operation catalog

The spec owns an exhaustive operation registry and a scope-compatibility matrix. Unknown operations, wildcard strings, and incompatible bindings are rejected.

| Resource | Operations |
|---|---|
| Cards | read, create-family, register-version, delete |
| Ownership | transfer |
| Verification bindings | read, manage |
| Verification executions | read, execute, cancel |
| Evidence | read, append |
| Approvals | read, submit |
| Repository review sets | read, execute |
| Bifrost | query, append, manage-table, manage-schema |
| Gateway | read, invoke, manage, manage-limits, manage-budgets, read-payload |
| GitHub connections | read, manage, use |
| Audit | read |
| Teams | read, manage, manage-members |
| Access administration | manage-roles, manage-bindings, explain-other |
| Principals/credentials/OIDC | read, manage |
| MCP-connected tools | invoke |

`create-family` applies to Tenant or OwnedByTeam scopes. Version registration applies to the existing family or owning-team selection.

Ownership transfer, role administration, arbitrary binding administration, OIDC configuration, and principal suspension require tenant scope.

No operation implicitly includes another. Server workflows explicitly request every prerequisite.

##### Built-in roles

Built-ins are immutable permission bundles. Their binding supplies the scope.

| Role | Granted behavior |
|---|---|
| `viewer` | Read Cards, bindings, executions, evidence, approvals, review sets, nonsecret gateway/connection metadata; query authorized Bifrost data |
| `workload` | Append evidence and telemetry through authorized ingest; no read, invocation, execution, or administration |
| `editor` | Viewer plus Card registration/deletion, binding management, execution/cancellation, evidence append, approval submission, and review-set execution |
| `auditor` | Viewer plus audit read; no writes and no captured gateway payload access |
| `team_manager` | Read a team and manage its local roster |
| `gateway_operator` | Read/manage deployments, limits, and budgets; invoke; no captured payload read |
| `access_admin` | Manage teams, ownership, roles, bindings, principals, credentials, and OIDC; explain another principal’s access |
| `admin` | Every registered tenant operation |

Important consequences:

- `viewer @ OwnedByTeam(X)` reads X’s resources and derived data.
- `viewer @ CardFamily(C)` shares all versions and their derived data.
- `viewer @ CardVersion(V)` shares only that version and its derived data.
- `access_admin` does not automatically read evidence or payloads. It can grant itself access, but that is an explicit audited mutation.
- `admin @ Tenant` is deliberately unrestricted within the tenant.
- Custom roles contain explicit operations and do not acquire future operations automatically.
- No automatic workload role or viewer fallback is assigned to trusted issuers.

The last active tenant-admin principal cannot be removed, suspended, or stripped of that binding through ordinary APIs.

#### 4. Evaluation and freshness

##### Exact authorization semantics

Authorization is default-deny and allow-only.

For an operation on a resource:

```text
ALLOW =
    valid tenant credential
    AND active subject and credential
    AND active delegation ancestors
    AND required audience
    AND a current matching role binding
    AND every credential/delegation ceiling
    AND every mandatory workflow prerequisite
```

For each requested permission:

1. Resolve the authenticated tenant.
2. Acquire a fresh immutable authorization snapshot before entering the synchronous checker.
3. Resolve the resource’s trusted authorization identity.
4. Expand the subject into itself and its current team memberships.
5. Find bindings whose role contains the operation and whose scope contains the resource.
6. Apply all credential ceilings and workload restrictions.
7. Return a structured allow/deny decision with matched binding IDs and policy revision.
8. Stage that decision on the process Scribe outbox.

A permission is allowed if any eligible binding grants it. Multiple required permissions are conjunctive.

There are no “more specific wins” rules. A Card-version grant adds access; it does not restrict a tenant grant. To narrow a principal, remove the broader binding or issue a narrower credential.

##### Scope inheritance

- Tenant scope includes tenant resources.
- OwnedByTeam scope matches the current owner of an ownable resource.
- Card-family scope includes every version and derived object rooted at those versions.
- Card-version scope includes that version and its derived objects.
- Table/schema scopes control Bifrost query or administration capabilities; they never replace protected-row permissions.
- Gateway and connection scopes include their own operational children.
- Generic Card relationships do not inherit authorization.
- No grant crosses a tenant boundary.

##### Server snapshots replace token authority

Tokens identify the principal, credential, audience, expiry, delegation chain, and signed ceilings. A token’s permission snapshot is no longer continuing authority.

The server maintains immutable authorization snapshots containing:

- Active principal and credential state.
- Current memberships and their validity deadlines.
- Roles and role bindings.
- Resource ownership and Card-family identities.
- Authorization tombstones.
- Policy and revocation revisions.
- Compiled scope indexes.

Checks remain synchronous and memory-only. `PermissionCheck` does not perform IO or execute a policy language.

Snapshot acquisition belongs to the server/auth owner. A miss, invalidation, or expired freshness lease can perform one batched PostgreSQL permission-state reload before checking. Warm requests perform no PostgreSQL statement.

Runtime owns evaluation. Auth owns acquisition and credential semantics. SQL owns persistence. Oracle consumes the compiled authorization view; it does not invent another policy model.

##### Revisions and revocation

Maintain two monotonically increasing tenant revisions:

- **Policy revision:** every authorization-affecting mutation.
- **Revocation revision:** any mutation that can remove access.

Pure additions advance only policy revision. Removals, suspension, ownership transfers, credential revocation, and edits whose effect is not provably additive advance both.

Every mutation commits its data and revisions atomically and emits a PostgreSQL notification. Notifications accelerate refresh; they are not the correctness mechanism.

Each process:

- Watches notifications.
- Reconciles revisions for active tenants in one batched background operation every second.
- Uses a freshness lease lasting at most five seconds.
- Renews a lease only after PostgreSQL confirms the relevant revocation revision.
- Fails closed when the lease expires and freshness cannot be established.

PostgreSQL derives coordination timestamps with `statement_timestamp()`. Processes use the existing anchored-clock approach to enforce elapsed validity without inventing independent coordination time.

A mutation response returns its policy revision and revocation deadline. Clients carry the revision as a minimum-freshness requirement on subsequent requests.

**The revocation guarantee is five seconds after commit, not instantaneous global revocation.** A mutation caller gets read-after-write behavior through its returned revision.

##### In-flight work

- Streaming reads and queries revalidate before emitting batches.
- Outputs are buffered behind a validation point; no batch is released under an expired freshness lease.
- A changed revocation revision cancels work using the older authorization snapshot.
- Pure additions do not cancel existing queries. Their original view remains a safe subset.
- Cached results are keyed by tenant, effective authorization fingerprint, credential ceilings, and revision.
- Exports and downloads pass through authenticated server delivery. No durable bearer URL bypasses subsequent revocation checks.

Execution already admitted may continue computing. Revocation prevents further caller reads and credential-driven actions; it does not erase evidence or automatically cancel background engine work.

##### Size and capacity rules

Do not put team lists or expanded Card IDs into tokens.

Target limits:

- Encoded tenant token: 16 KiB.
- Delegation depth: five.
- Signed ceiling terms across a credential chain: 256.
- Teams per principal: 2,000.
- Teams per tenant: 10,000.
- Role bindings per tenant: 100,000.
- Permissions per role: 256.
- Card families in an active tenant authorization directory: 1,000,000.

Use shared indexed snapshots and compressed family/version sets, not copied permissions per request.

Default authorization-cache budgets are 256 MiB per tenant and 1 GiB per process. Operators may raise capacity budgets; doing so does not change policy semantics. Evict inactive tenant snapshots. If a required snapshot cannot be loaded within its budget, return an authorization-state-unavailable error; never fall back to token grants.

Control-plane mutations validate structural limits before commit. Capacity and refresh failures are observable through metrics and administrative health endpoints.

##### Delegated agents and API keys

Every delegated token requires an explicit requested ceiling. No unrestricted default exists.

Effective authority is the intersection of:

- Current subject authority.
- Current authority of every actor in the chain.
- Original signed ceiling restrictions.
- Requested restrictions at each exchange.
- Audience and workload restrictions.

Ceilings use the same typed scopes. Preserve intersections as bounded conjunctive layers of allow sets; do not flatten an owning-team restriction into a permanent Card grant. A Card moving out of team X must stop satisfying an ancestor’s `OwnedByTeam(X)` ceiling.

This is a fixed credential representation, not a user-authored expression language.

Exchange rejects an unrepresentable or oversized result before signing. Child expiry cannot exceed any ancestor expiry. Preserve the existing direct-Service actor requirement, audience restrictions, and maximum depth unless separately revised.

User API keys receive typed scoped ceilings. Operation strings parsed as tenant-wide permissions are removed.

##### Workload tokens and `card_access`

Keep two independent restrictions:

1. Operation authority.
2. Signed attribution authority through `card_access`.

`card_access` does not grant reads.

A workload appending evidence must have:

- Evidence/ingest permission.
- An authorized destination.
- Attribution covered by `card_access`.
- A valid execution or workload context establishing the authorization root.

The server never trusts a caller-supplied authorization-root column.

Generic telemetry without Card attribution belongs to an explicit team telemetry collection. Its token must name that authorized collection. There is no unowned tenant-wide telemetry fallback.

#### 5. Derived objects and Bifrost data

##### Authorization roots

Every protected object and row has a trusted authorization root.

For Card-derived data, record:

```text
CardFamilyId
exact root CardUid
root authorization revision
```

Other root types are:

- Bifrost table.
- Gateway deployment.
- GitHub connection.
- Team telemetry collection.
- Tenant control plane.

A root is separate from correlation fields such as `subject_card_uid`, workload UID, run ID, or GitHub repository ID.

These correlation fields are currently nullable and cannot establish complete authorization. See [managed_columns.rs:62](../../../../crates/vala/vala-bifrost-redux/src/tables/managed_columns.rs:62).

##### Inheritance rules

| Object/data | Root and mandatory read requirements |
|---|---|
| Card version | Its family/version; Card read |
| Verification binding | Target Card family/version; Card read and binding read |
| Ordinary execution | Exact subject Card version; Card read and execution read |
| Direct verification without a subject Card | Exact Verifier Card version |
| Repository review set | Exact Repository Card version; Card read and review-set read |
| Review-set member execution | The review set’s Repository root; Card read, review-set read, execution read |
| Results, inputs, outputs, evidence, approvals | Same root as their execution; corresponding read operation |
| PR evidence/history | Exact Repository root |
| Gateway call metadata | Gateway deployment; gateway read |
| Gateway captured payload | Same root plus read-payload |
| Generic telemetry | Explicit team telemetry collection |
| User-table row | Table root, or validated Card root for a protected-row table |
| Resource-specific audit decision | Resource checked by that decision; audit read plus resource visibility |
| Tenant-control audit decision | Tenant control-plane root; tenant audit read |

A verifier grant never substitutes for a Repository grant when reading a Repository-owned execution.

##### Review-set execution rules

A review-set execution is created under the Repository root. An existing execution from another root cannot be attached as a member.

Execution admission checks:

- Repository visibility and review-set execution.
- Read/invoke authority for selected Verifiers.
- Access to required target/configuration resources.
- Any connection, gateway, and approval prerequisites.

Results are published into that Repository’s evidence boundary. This does not grant access to referenced Card specifications, provider secrets, or connection credentials.

References to another protected resource do not confer access. Expanding or downloading the referenced object performs its own check.

##### Server lists, searches, counts, and pagination

Authorization filtering occurs in the owner query before:

- Sorting.
- Cursor application.
- Limit.
- Count.
- Faceting or grouping.

For PostgreSQL Card listings, compiled visible family/version sets become SQL predicates or typed array relations. Never fetch a page and remove unauthorized rows afterward.

For derived-object listings, filter on their recorded root using the same compiled authorization view.

Pagination cursors bind:

- Tenant.
- Query and sort.
- Authorization fingerprint.
- Policy snapshot identity.

A relevant revocation invalidates the cursor. Return a stable cursor-invalidated error and require the client to restart. Do not continue from an offset that leaks hidden membership or produces misleading counts.

Search suggestions, autocomplete, dashboards, export totals, and empty-state counts obey the same rule.

##### Oracle and raw SQL

Oracle secures every scan, including:

- Memtables.
- Iceberg/Parquet.
- Views and expanded subqueries.
- Joins, CTEs, unions, and nested queries.
- Peer execution and cached results.

For each protected source:

```text
Visible(row) =
    tenant isolation
    AND query grant covers the table or the row's root
    AND domain read permissions cover the row's root
    AND row-class requirements are satisfied
```

Query-grant scope interpretation is fixed:

- Tenant/schema/table grants cover their corresponding table.
- Card-family/version and owning-team grants cover matching roots in protected tables.
- For ordinary user tables, owning-team scope matches the table’s owner.

A table grant alone does not expose verification or audit rows.

Examples:

- `bifrost.query @ system.verification_results` without Card/execution read returns no protected result rows.
- `viewer @ OwnedByTeam(X)` can query X’s authorized verification rows.
- `audit.read` without visibility of Repository R cannot expose R’s decision rows.
- Gateway payload access remains an additional requirement, including when referenced in a predicate rather than projection.

The secured table provider applies authorization before user expressions. It is an optimizer boundary: user UDFs, projections, and unsafe expression pushdown cannot execute against hidden rows.

Authorization membership uses compiled sets/bitmaps inside the provider, not enormous generated `IN` strings. Partition/file pruning may accelerate this, but the final scan filter is mandatory.

Do not expose unfiltered statistics, physical file paths, hidden-root counts, or raw underlying storage through `EXPLAIN`, catalog functions, or alternate scan interfaces. Timing isolation is not promised; data, cardinality, and error-message isolation are.

Peer requests bind tenant, secured plan, authorization fingerprint, revisions, and a short-lived internal proof. Peers validate freshness and cannot substitute unrestricted SYSTEM authority.

Internal SYSTEM reads may bypass the caller’s table-query capability for a specific server workflow. They must still apply the caller’s domain/root visibility before returning data.

##### User tables

Each table declares one immutable access mode:

- **Table:** every row inherits the table root.
- **Card-rooted:** every row requires a validated exact Card root.

Changing the access mode requires creating a replacement table and an explicit data migration. It is not an in-place authorization toggle.

For Card-rooted tables, append requires both table append authority and authority to attribute to the supplied root. Missing or forged roots are rejected.

System verification and audit tables remain server-write-only.

##### Fail-closed handling

- Unknown root: deny direct read; omit from lists and scans.
- Missing protected-row root: quarantine from ordinary reads.
- Stale snapshot: refresh or return unavailable.
- Cross-tenant reference: reject before evaluating grants.
- Missing ownership: resource cannot enter normal registered/servable state.
- Old retained rows without provable roots: administrator-only migration quarantine, not tenant-wide visibility.
- Revocation during streaming: stop output and return the established cancellation/error result.

#### 6. Surfaces and user journeys

##### One server API across clients

Expose typed HTTP operations, matching MCP tools, and thin SDK/CLI projections for:

| Capability | HTTP collection/operation |
|---|---|
| Teams | `/v1/teams` |
| Local memberships | `/v1/teams/{team_id}/memberships` |
| Roles | `/v1/roles` |
| Role bindings | `/v1/role-bindings` |
| Ownership transfer | `/v1/cards/{card_uid}/ownership` |
| Explain access | `/v1/access/explain` |
| Current access summary | `/v1/access/self` |
| Provisioning | `/scim/v2/Users`, `/scim/v2/Groups` |

Use existing repository conventions for CRUD verbs and pagination. Mutations require revision preconditions; stale updates return conflict. Creates and transfers accept idempotency keys.

Bindings expose subject, role, scope, creator, revision, and source. There is no separate UI-only authorization API.

Example SDK flow:

```text
teams.create(...)
teams.add_member(...)
role_bindings.create(subject=team_x, role=viewer, scope=owned_by_team_x)
cards.register(..., owning_team=team_x)
access.explain(permission=..., resource=...)
```

MCP read tools remain available. Calling them still applies tenant/resource checks. MCP write exposure requires explicit credential scopes and the underlying operation permission.

##### Tenant administrator journey

1. Open Access settings.
2. Create a local team or map an immutable IdP group to a synchronized team.
3. Assign its manager.
4. Add users/service accounts, or inspect synchronized membership.
5. Bind a role to that team and a resource scope.
6. Preview the resulting access using the server’s explain operation.
7. Save with a revision precondition.
8. Receive the committed revision and revocation deadline.

The UI distinguishes:

- Team membership.
- Resource ownership.
- Role bindings.
- Credential restrictions.

It never presents ownership as an implicit grant.

##### Engineer and data scientist journey

A user registering a Card chooses an authorized owning team on first registration. Subsequent versions display the inherited owner.

Lists show only visible Cards. Their totals and cursors come from already-filtered server queries. Opening an inaccessible direct link returns the resource’s ordinary not-found response.

SDKs behave identically. There is no client-side ownership convention.

##### Platform automation and CI

Create a service account, place it in a local team, and bind only its required operations.

A CI account might receive:

- Register-version on a Model family.
- Execute on a Repository family.
- Workload append on exact execution contexts.

It receives no viewer access merely because it is trusted to authenticate.

Federated CI identity maps to that service account. Repository or branch assertions may constrain credential issuance through the issuer’s fixed authentication rules; they do not become an arbitrary runtime ABAC language.

##### Cross-team sharing

Share a Card family with another team through a normal binding:

```text
subject: team Y
role: viewer
scope: CardFamily C
```

This shares C’s versions and derived evidence. It does not transfer ownership or share unrelated dependencies.

Share one version using CardVersion scope. Share an entire owning team’s collection using OwnedByTeam scope.

##### Explain access

The request contains permission, resource reference, and optionally a subject.

- Any caller may explain its own access.
- Explaining another principal requires tenant `explain-other`.
- The operation never impersonates the subject or executes the requested action.

An allowed explanation returns:

- Decision ID and revisions.
- Required operations.
- Matched binding and role.
- Membership path.
- Scope/inheritance path.
- Effective credential ceilings.

A denied self-explanation returns the failed prerequisite and safe remediation. It does not reveal a hidden resource’s owner, existence, or other principals’ grants. Access administrators receive the full administrative explanation.

##### Errors

Retain existing authentication and RBAC denial codes, including `WYRD_PERMISSION_403_DENIED_RBAC`. Update its title/remediation to current tenant vocabulary.

Add:

| Code | Status | Meaning |
|---|---:|---|
| `WYRD_PERMISSION_503_STATE_UNAVAILABLE` | 503 | Fresh authorization state cannot be established |
| `WYRD_PERMISSION_422_INVALID_SCOPE` | 422 | Scope is incompatible, malformed, or cross-tenant |
| `WYRD_PERMISSION_422_LIMIT_EXCEEDED` | 422 | Structural policy/credential limit exceeded |
| `WYRD_PERMISSION_409_REVISION_CONFLICT` | 409 | Administrative update used stale state |
| `WYRD_PERMISSION_409_CURSOR_INVALIDATED` | 409 | Relevant authorization changed during pagination |
| `WYRD_PERMISSION_409_MEMBERSHIP_SOURCE` | 409 | Attempt to edit a synchronized roster locally |

Use existing domain not-found errors for inaccessible object lookups. Use 403 when the caller knows the administrative operation/resource but lacks authority. Preserve the existing unrepresentable-delegation error.

Errors include a decision ID. They never include hidden grants or secrets.

##### Audit

Every actual permission check stages one decision on the process Scribe outbox.

Record:

- Tenant, subject, actor chain, credential ID.
- Permission and root.
- Allow/deny outcome.
- Policy/revocation revision.
- Matched binding IDs or safe denial reason.
- Request and decision IDs.

A workflow checking multiple permissions records each check at its boundary. A filtered query records its source authorization decisions and secured-view fingerprint, not an event per row.

Audit staging never waits on durable audit storage and never changes the operation outcome. Audit history remains in `bifrost.system.audit_log`.

#### 7. Worked examples

##### “Read any Card belonging to team X”

Configuration:

```text
Alice is a member of team X.
Binding B grants viewer to team X at OwnedByTeam(X).
Model M and Repository R are owned by X.
Model N is owned by Y.
```

When Alice lists Cards:

1. The server resolves Alice’s current membership.
2. B supplies read operations over resources currently owned by X.
3. The Card query applies the authorized-family predicate before pagination/count.
4. M and R appear. N does not.
5. Alice can read derived evidence rooted at M or R through viewer’s corresponding operations.
6. A Bifrost aggregate includes only the same authorized roots.

Removing Alice from X invalidates B’s subject relationship within five seconds. Moving M to Y stops B matching M, including historical evidence. No retained Bifrost row requires rewriting.

##### Repository review-set read

Assume:

- Repository version R is owned by team Payments.
- Review set S is rooted at R.
- Member executions E1 and E2 were created with R as their immutable root.
- Their selected Verifiers are shared across several teams.

Bob can read the Verifiers but has no access to R.

**Direct read**

1. Bob requests S.
2. The server resolves S’s recorded R root.
3. It checks Repository Card read and review-set read at R.
4. Bob fails.
5. The response is the ordinary not-found result.
6. The denied decision is staged asynchronously.

**Member execution**

Bob requests E1 directly. The server checks E1’s R root and the execution/review-set prerequisites. Verifier read does not satisfy them. The result remains not found.

**Lists and counts**

S, E1, and E2 are absent from Bob’s lists. They do not contribute to totals, search facets, or pagination.

**Raw SQL**

Bob runs:

```sql
SELECT count(*) FROM system.verification_results;
```

Oracle secures the source. E1/E2 rows fail R visibility before aggregation. A table-query grant does not restore them.

**Sharing**

An administrator grants Bob’s team viewer on R’s family. Bob can now read S, both executions, results, evidence, and their rows. This does not reveal the specifications or secrets of otherwise inaccessible referenced components.

**Offboarding**

Removing the binding advances revocation revision. Existing streams stop releasing batches under the old revision within the five-second bound.

#### 8. Decision register

| Decision point | Chosen answer |
|---|---|
| Base model | Relationship-aware RBAC with fixed semantics |
| General ABAC/policy language | Rejected |
| External authorization service | Rejected |
| Grant behavior | Additive allow-only; default deny |
| Restriction mechanism | Mandatory prerequisites and intersecting credential ceilings |
| Team nesting | None |
| Team membership sources | One local or one synchronized source per team |
| Machine membership | Local teams |
| Ownership cardinality | Exactly one team per ownable resource |
| Card ownership granularity | Family; inherited by every version |
| Card identity | Unchanged |
| Ownership implies access | No |
| Cross-team sharing | Explicit scoped bindings |
| Transfer behavior | Remove family/version shares atomically |
| Derived data | Immutable root; current root grants |
| Review-set member root | Repository version |
| Arbitrary execution attachment | Prohibited across roots |
| Token grants | Ceilings/context, not continuing authority |
| Runtime check | Synchronous, memory-only |
| Freshness | Notification plus revision reconciliation |
| Revocation bound | Five seconds after Wyrd commit |
| IdP-only freshness | Membership assertion expires within five minutes |
| List/count filtering | Before pagination and aggregation |
| Raw SQL | Mandatory secured providers |
| Missing roots/state | Fail closed |
| Audit | Every decision staged; never blocks |
| Access-admin data visibility | No automatic data grant |
| Compatibility | Coordinated breaking cutover; no aliases or fallback |

#### 9. Required authority and schema changes

##### Architecture revisions

Approval must explicitly replace the conflicting current contracts:

| Authority | Required revision |
|---|---|
| Security posture identity sections | Teams, membership sources, machine membership, suspension |
| Security posture token authority, [275](../../../../architecture/wyrd-security-posture.md:275) | Replace sole token authority with current server snapshots |
| Security posture revocation, [313](../../../../architecture/wyrd-security-posture.md:313) | Add revisions, leases, five-second enforcement |
| Security posture delegation, [466](../../../../architecture/wyrd-security-posture.md:466) | Typed scoped ceilings and current ancestor authority |
| Security posture permission model, [650](../../../../architecture/wyrd-security-posture.md:650) | Roles separated from resource-scoped bindings; ownership relationships |
| Security posture checker/boundaries, [692](../../../../architecture/wyrd-security-posture.md:692) | Preserve pure checker; assign snapshot acquisition explicitly |
| Card metadata/identity, [3222](../../../../architecture/wyrd-design.md:3222) | Add owning-team projection and internal family ID; preserve rejection of spaces |
| Execution authorization, [2014](../../../../architecture/wyrd-design.md:2014), [2232](../../../../architecture/wyrd-design.md:2232) | Mandatory recorded roots and current visibility |
| PR verification, [2679](../../../../architecture/wyrd-design.md:2679) | Repository-rooted review-set/membership contract |
| Bifrost authorization, [2182](../../../../architecture/bifrost-design.md:2182) | Replace table-only visibility with query-plus-root filtering |
| Bifrost query constraints, [2187](../../../../architecture/bifrost-design.md:2187) | Define shared snapshot consumption and secured peer execution |
| Audit sections | Add revisions/binding evidence and protected-root reads; retain nonblocking writes |
| Operations | Cutover, SCIM health, snapshot capacity, stale-state failure and recovery |
| UI authority | Access administration and filtered-list behavior through server APIs |

Implementation against today’s conflicting text is **`SPEC_REVISION_REQUIRED`**. The proposal does not authorize editing architecture to justify already-written code.

##### PostgreSQL changes

The approved specification must name these changes before migrations are written.

Reuse `wyrd.auth_roles`; do not create a second role store.

New tables:

| Table | Purpose |
|---|---|
| `wyrd.auth_teams` | Team identity, source, external source/group ID |
| `wyrd.auth_team_memberships` | User/service-account membership, source, validity |
| `wyrd.auth_role_bindings` | Subject, role, typed scope, provenance |
| `wyrd.auth_policy_revisions` | Policy/revocation revisions |
| `wyrd.card_families` | Tenant/kind/name identity, owner, tombstone, root revision |
| `wyrd.auth_team_telemetry_collections` | Explicit ownership boundary for unattributed telemetry |

Every new table:

- Leads with `data_tenant_id`.
- Enables and forces RLS.
- Uses tenant-qualified keys and foreign keys.
- Uses PostgreSQL-owned coordination timestamps.

Required constraints/indexes:

- Unique tenant/team name.
- Unique synchronized issuer/external-group identity.
- Unique membership per team/principal.
- Exactly one user or service-account member reference.
- Exactly one binding subject reference.
- Scope-specific columns, CHECK constraints, and tenant-qualified foreign keys.
- Unique tenant/kind/name family identity.
- Indexes by binding subject, role, scope, team member, owner, and external source.
- Idempotency-key uniqueness for relevant mutations.

Alter:

- `auth_roles`: operation-only documents; role revisions.
- Principals/credentials: active/revoked state and credential-ceiling contract version where absent.
- Cards: family foreign key.
- Bifrost catalog tables, gateway deployments, GitHub connections: owning team and resource revision.
- Durable execution/admission/review-set coordination metadata: exact authorization root.
- User-table definition metadata: access mode.
- Existing role-assignment storage: migrate into role bindings and remove the superseded assignment tables.

Do not put evidence, analytical rows, inputs, or audit history in these tables.

Bifrost managed schemas separately gain server-controlled root fields and root revision. Keep existing tenant-footer isolation; do not replace it with a caller-supplied tenant column.

##### Control-plane consistency

Administrative mutations use one transaction to validate references, change governance state, increment revisions, and publish invalidation notification.

Audit remains outside that transaction on Scribe’s outbox.

Use revision preconditions to serialize conflicting roster, ownership, and binding edits. Normal authorization requests never mutate these tables.

#### 10. Migration and compatibility

Use a coordinated breaking cutover. Do not run old token-authority servers alongside new snapshot-authority servers.

1. **Inventory and preflight**
   - Enumerate roles, assignments, keys, issuers, Cards, derived roots, and tables.
   - Produce a reviewable ownership and binding manifest.
   - Require an owning team for every family and ownable resource.
   - Reject ambiguous Repository roots rather than guessing from Verifier UID or GitHub numeric ID.

2. **Translate identity and grants**
   - Convert direct user/service-account assignments into bindings.
   - Convert IdP group mappings into synchronized teams plus explicit bindings.
   - Map `All` to Tenant scope, Bifrost/Gateway/Verifier scopes to their typed equivalents.
   - Map named connected-tool permissions to McpTool scope.
   - Reject mappings that cannot be represented.

3. **Apply built-in role changes**
   - `viewer` and `editor` lose implicit audit and captured-payload access.
   - Existing recipients retain Tenant scope unless the approved manifest narrows it.
   - Audit/payload access requires explicit approved grants.
   - Workload assignments receive explicit append scopes; no broad viewer fallback.
   - Custom roles translate to explicit operation bundles and bindings.

   These reductions are intentional behavior changes and appear in preflight output. No hidden privilege-preserving fallback exists.

4. **Backfill roots**
   - Establish family ownership.
   - Root retained execution data using immutable admission identities.
   - Rewrite/backfill Bifrost data through the owning migration/maintenance path.
   - Quarantine unprovable rows from ordinary reads.

5. **Cut over**
   - Drain old-version traffic.
   - Deploy the new server and first-class client contracts.
   - Reject old token contract versions.
   - Reauthenticate sessions and reissue workload/delegated tokens.
   - Reissue API keys with typed ceilings.
   - Remove old scope parsing and obsolete assignment code.

6. **Rollback rule**
   - Roll back deployment before reopening traffic, or restore the pre-cutover backup.
   - Never reopen with old authorization code against newly governed data and credentials.

No compatibility routes, aliases, dual checkers, or token-snapshot fallback remain.

#### 11. Hot-path and verification requirements

##### Added or moved waits

| Path | Proposed wait | Classification |
|---|---|---|
| Warm permission check | None | Memory only |
| Permission snapshot miss/invalidation/expiry | One batched PostgreSQL permission-state reload | Sanctioned permission check |
| Oracle secured scan | None for authorization with warm directory | In-memory membership/filtering |
| Workload/evidence root validation | None with admitted execution/workload context | Memory only |
| Cold target/definition resolution | Existing permitted cache load | Preserve existing owner and bound |
| Audit recording | None | Stage on Scribe outbox |
| Revision reconciliation | Background PostgreSQL operation | Outside request hot path |
| IAM mutations | Control-plane transaction | Outside defined execution/query/append hot paths |

No request waits for object storage, Scribe audit persistence, or Vault to decide access. No per-request client/runtime construction and no lock held across an await.

##### Journey tests

Use real SDK → `WyrdTestServer` + repository PostgreSQL → SDK journeys for Rust, Python, TypeScript, and HTTP/MCP. CLI and UI must exercise those same contracts.

Required scenarios:

- Local team creation, membership, binding, registration, and list/read.
- OIDC group mapping, assertion expiry, unknown groups.
- SCIM add/remove/deactivate and source-edit rejection.
- Service-account CI registration and workload emission.
- Agent delegation, every ancestor ceiling, suspension, depth/size refusal.
- Cross-team family and exact-version sharing.
- Ownership transfer, removed shares, historical evidence visibility.
- Last-admin protection.
- Explain-access information disclosure.
- Cross-tenant IDs in every administration/resource operation.
- Stale revisions, idempotent retries, concurrent membership/transfer edits.
- Cold cache, lost notifications, disconnected listener, PostgreSQL outage.
- Revocation during query, download, pagination, and streaming.

The Repository example must pass on every read surface:

- No review set.
- No member execution.
- No result/evidence/approval.
- No count or facet contribution.
- No raw SQL row or aggregate contribution.
- No bypass through Verifier access, internal SYSTEM reads, views, peers, or exports.

Oracle integration tests must cover every scan implementation and SQL expression position, including unauthorized values referenced only in filters/UDFs, unions, joins, CTEs, `COUNT(*)`, statistics, and cached results.

##### Verification lanes and performance

Run the owning targeted lanes, generated-contract checks, format, and lints through mise. Name and run exact nextest tests in implementation evidence.

Hot-path acceptance additionally requires full clean runs:

- `mise run bench:l1:verification`
- `mise run bench:l1:gateway`
- `mise run bench:l1:bifrost`

The Bifrost aggregate includes writes, hot queries, and Parquet queries. Compare each with the newest matching experiment/host record under `benchmarks/history/`; verdict must be `VALID`.

Measure authorization latency, directory memory, large-team compilation, secured aggregate throughput, invalidation convergence, and cancellation latency. Smoke, partial-step, and dirty runs do not satisfy acceptance.

#### 12. Current-model defects and limitations

These findings are static source observations.

| Finding | Evidence |
|---|---|
| Scopes cannot represent teams, arbitrary Card families/versions, or Repository ownership. | [permission_scope.rs:45](../../../../crates/wyrd-spec/src/auth/permission_scope.rs:45) |
| The checker only tests token-snapshot permissions; it has no current membership/ownership context. | [permission_check.rs:65](../../../../crates/shared/wyrd-runtime/src/permission_check.rs:65) |
| Viewer includes broad Card, Verifier, audit, captured gateway payload, and Bifrost access. Workload repeats broad read authority. | [builtin_roles.rs:23](../../../../crates/shared/wyrd-runtime/src/builtin_roles.rs:23), [builtin_roles.rs:38](../../../../crates/shared/wyrd-runtime/src/builtin_roles.rs:38) |
| Trusted workload issuers default to viewer. Human OIDC login does not use this fallback. | [builtin_roles.rs:110](../../../../crates/shared/wyrd-runtime/src/builtin_roles.rs:110) |
| `card_access` is attribution authority, not a read ACL; using it to solve Repository visibility would be incorrect. | [workload.rs:216](../../../../crates/wyrd-spec/src/auth/workload.rs:216) |
| Personal API-key ceiling strings parse into All scope, preventing typed team/object narrowing. | [issuance.rs:845](../../../../crates/wyrd/wyrd-auth/src/issuance.rs:845), [permission.rs:923](../../../../crates/shared/wyrd-runtime/src/permission.rs:923) |
| IdP groups project directly into roles; provider removal reaches persisted grants on next login. | [callback.rs:647](../../../../crates/wyrd/wyrd-auth/src/callback.rs:647), `role_assignments.rs:151` (removed historical source) |
| PR retained-execution reads check Verifier authority plus generic Card read, without the owning Repository identity. | [inspection.rs:521](../../../../crates/wyrd/wyrd-server/src/components/verification/service/inspection.rs:521) |
| Card lists authorize generic Card read; the SQL list filters metadata and paginates without a visibility predicate. | [routes.rs:381](../../../../crates/wyrd/wyrd-server/src/components/cards/routes.rs:381), [list.rs:108](../../../../crates/wyrd/wyrd-sql/src/queries/cards/list.rs:108) |
| Oracle authorizes resolved tables without constructing Card/Repository row predicates. Its extra payload check is gateway-specific. | [oracle/mod.rs:3820](../../../../crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3820), [oracle/mod.rs:3856](../../../../crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3856) |
| Correlation columns are nullable; GitHub delivery events can omit both subject and workload identities. | [managed_columns.rs:62](../../../../crates/vala/vala-bifrost-redux/src/tables/managed_columns.rs:62), [delivery.rs:675](../../../../crates/wyrd/wyrd-server/src/components/github/delivery.rs:675) |
| Card metadata has no owner. Existing exact Card UIDs do not provide a family governance identity. | [envelope.rs:162](../../../../crates/wyrd-spec/src/envelope.rs:162), [20260601000006_cards.sql:12](../../../../crates/wyrd/wyrd-sql/migrations/20260601000006_cards.sql:12) |
| Refused delegation is recorded with an allowed audit outcome. | [exchange_api_key.rs:317](../../../../crates/wyrd/wyrd-auth/src/exchange_api_key.rs:317), [exchange_api_key.rs:342](../../../../crates/wyrd/wyrd-auth/src/exchange_api_key.rs:342) |
| Current UI settings expose IdP group-to-role mapping, not team/resource binding management. | [+page.server.ts:80](../../../../crates/wyrd/wyrd-server/wyrd-ui/src/routes/t/[tenantKey]/settings/+page.server.ts:80) |
| The denial remediation still uses “workspace admin.” | [error.rs:1193](../../../../crates/wyrd-spec/src/error.rs:1193) |

A dedicated production ReviewSet API/type was not found; current production behavior uses PR generation/admission records. The proposed review-set contract must therefore be implemented against that owner rather than assumed to exist.

Repository unchanged. No tests, running-server checks, migrations, or benchmarks were executed; correctness and performance remain implementation acceptance requirements.
