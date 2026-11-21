# Rust model and token flow

Status: finalized design supplement. Not specification or implementation approval.

This supports the [current design](notes.md). The maintainer locked the unified
authorization model and preservation of existing attribution semantics on
2026-11-19. The representations below record that agreement.
Rust sketches omit derives, imports, timestamps, routine pagination and CRUD.
Named IDs stand for validated domain newtypes; this is not compilable source or
a file-by-file prescription. No production types or schema definitions have changed.

## What stays and what changes

| Type/concept | Proposed shape |
|---|---|
| Role | Remains a reusable operation bundle; object scope moves to its binding |
| RoleRef | Keep the existing validated role name for authoring/display |
| Resource / Action | Keep the closed operation enums; no runtime strings |
| Permission | Keep the existing resource/action/scope triple for resolved grants, ceilings and requirements |
| PermissionScope | Keep and extend the typed enum for general Card family/version access |
| PermissionSet | Keep an additive collection of scoped permissions |
| RoleAssignment | Replace unscoped assignment with RoleBinding; retain direct/provider provenance |
| Principal | Keep stable identity and attribution; effective permissions come from current server state |
| AccessTokenClaims | Remove authoritative grant snapshot and informational role list; identify the caller and carry restrictions when needed |
| New machinery | Local groups, scoped bindings, shared authorization snapshots and an effective-permission view |

There is no existing single public `Role` struct to preserve verbatim: current
roles have a SQL `RoleRow`, seeded `BuiltinRole` and runtime `RoleRef`. The domain
concept stays. A typed Role projection replaces raw JSON at public boundaries;
it reuses `wyrd.auth_roles`, not a second role catalog.

## Role operations versus scoped permissions

A role answers what; the binding supplies where. Preserve the request-side
Permission triple rather than force every handler to adopt another spelling.

```rust
/// An operation a reusable role can grant, without an object selection.
pub struct Operation {
    pub resource: Resource,
    pub action: Action,
}

/// An existing tenant role, projected as a typed operation bundle.
pub struct Role {
    pub id: RoleId,
    pub name: RoleRef,
    pub operations: Vec<Operation>,
    pub builtin: bool,
}

/// A scoped grant, credential restriction, or concrete check requirement.
pub struct Permission {
    pub resource: Resource,
    pub action: Action,
    pub scope: PermissionScope,
}

/// Additive grants: any member covering a requirement is sufficient.
pub struct PermissionSet(Vec<Permission>);
```

`Operation` is the one small new value needed to avoid putting fake `All` scopes
inside every role or allowing role scope and binding scope to conflict. It reuses
Resource and Action rather than define a second operation enum. A role-to-scope
compiler produces the same Permission values the checker already understands.

Only admin retains the deliberate tenant wildcard. Custom roles name explicit
operations and do not silently acquire future operations. Existing gateway rules
still require explicit deployment/model grants for a `granted` deployment;
normalization must preserve them beside `All`.

Pure wire/domain values belong in `wyrd-spec`. Resource, Action, Permission,
PermissionSet and RoleRef currently live partly in runtime: move the required
pure definitions to their contract owner and re-export through runtime, rather
than duplicate them in request DTOs. Runtime retains checking/indexing behavior;
SQL retains rows; no auth contract gains IO, async or database dependencies.

## PermissionScope remains

```rust
/// A family grant or one exact version with its trusted family context.
pub enum CardPermissionScope {
    Family {
        kind: CardKind,
        name: CardName,
    },
    Version {
        kind: CardKind,
        name: CardName,
        uid: CardUid,
    },
}

/// Supported object scopes; tenant authority is always supplied separately.
pub enum PermissionScope {
    All,
    Card(CardPermissionScope),
    Bifrost(BifrostPermissionScope),
    Gateway(GatewayAccess),
    GithubConnection(GithubConnectionId),
    Tool(BuiltinTool),
}
```

Keep `All`'s existing name: it means all applicable objects for that operation
inside the verified tenant, not all tenants or platform authority. Reuse the
existing Bifrost, gateway and built-in tool types. The GitHub variant uses the
owning connection's identifier; no parallel connection identity is introduced.

Replace the special `Verifier(CardUid)` scope with general exact-Card scope.
Resolve and validate the Verifier's family through the existing Card owner; no
compatibility alias is introduced. A registered Verifier is a Card and needs no second identity rule.

An exact scope contains its family so family coverage is a pure comparison:

```text
All covers an applicable Card family or version.
Family(Repository, payments) covers versions whose trusted family is payments.
Version(payments, uid1) covers only that exact version.
Version(payments, uid1) does not cover uid2 or the whole payments family.
```

The server obtains version/family association from its Card/target cache or
retained admission metadata. It never trusts a caller's assertion that uid1
belongs to payments. The family context avoids a new directory lookup for every
permission comparison and does not require expanding a family into all version
UIDs. Validation excludes nonregistrable Card kinds and foreign-tenant objects.

Scope containment does not itself decide operation applicability. The fixed
catalog rejects nonsensical operation/scope pairs. A gateway scope cannot become
Card access; a Card scope cannot authorize arbitrary table contents.

### WorkloadRestriction is retained separately — locked

```rust
/// Existing limit on which Service family an account may run as.
pub enum WorkloadRestriction {
    Unrestricted,
    Family(WorkloadFamily),
}

/// Existing runtime family, independent of release version.
pub struct WorkloadFamily {
    pub kind: WorkloadKind, // Service
    pub name: CardName,
}
```

Keep these existing specialized types and semantics. WorkloadRestriction limits
valid runtime context; CardPermissionScope supplies the object axis of an
operation permission. They are independent mandatory checks, not two competing
grant mechanisms. Neither supplies authority the other lacks. The earlier
suggested general CardFamily wrapper is removed; no new family identity/store
or ownership object is needed.

The account, credential and runtime-family association survive release changes.
The loaded application supplies the exact workload_uid, which the existing owner
validates against the restriction and resolves into Card attribution. Preserve
signed-root mismatch refusal, the exact attribution graph and existing
unrestricted automation behavior. Broad operation permissions never bypass a
configured runtime-family restriction. User and Service operation checks use the
same roles, bindings, scopes, cached grants and PermissionCheck.

## RoleBinding and optional local groups

```rust
/// An existing tenant account or an optional local access group.
pub enum BindingSubject {
    Principal {
        id: PrincipalId,
        kind: PrincipalKindTag,
    },
    Group(GroupId),
}

/// A scoped assignment of one existing role within one tenant.
pub struct RoleBinding {
    pub id: RoleBindingId,
    pub data_tenant_id: DataTenantId,
    pub subject: BindingSubject,
    pub role_id: RoleId,
    pub scope: PermissionScope,
    pub source: RoleSource,
}

/// An optional Wyrd-managed roster, independent of resource ownership.
pub struct Group {
    pub id: GroupId,
    pub data_tenant_id: DataTenantId,
    pub name: String,
}
```

The principal reference includes kind because existing principal IDs are resolved
with their account table/kind; do not assume a bare UUID identifies every account
category uniquely. The managed subject kinds are User and Service; validate
those at this administration boundary. Existing bootstrap/internal mechanics
are not additional account choices in the UI or this binding API. A group is
not another principal kind and never logs in.

Membership is a tenant-qualified group/account relationship with the same
validated account reference, not an embedded roster copied into each binding.
Reuse existing `RoleSource` direct/provider distinctions. Local group bindings
are direct; optional IdP role projection remains account-specific and replaces
only that provider's assignments.

Example stored relationship:

```text
group reviewers contains User Alice
binding: reviewers → role viewer → Card Family(Repository, payments)
```

For Alice, the role compiler applies viewer's operations to that scope and its
fixed derived-data descendants. It skips operations that have no meaning there
and rejects bindings yielding no authority. The returned access summary shows
the applicable operations. No operation is promoted to `All` to make a binding fit.

## Concrete mapping: Alice and CI

The earlier arrows abbreviate actual role and binding records. Here is the
mapping to the proposed structs. This is illustrative application code, not
existing SDK method names. Assume tenant, account and role records already exist;
`new_binding_id()` supplies a typed ID and `card_name()` validates a CardName.

```rust
let viewer: Role = roles.get_by_name("viewer")?;
let editor: Role = roles.get_by_name("editor")?;

let alice_binding = RoleBinding {
    id: new_binding_id(),
    data_tenant_id: tenant_id,
    subject: BindingSubject::Principal {
        id: alice_id,
        kind: PrincipalKindTag::User,
    },
    role_id: viewer.id,
    scope: PermissionScope::Card(CardPermissionScope::Family {
        kind: CardKind::Repository,
        name: card_name("payments")?,
    }),
    source: RoleSource::Direct,
};

let ci_binding = RoleBinding {
    id: new_binding_id(),
    data_tenant_id: tenant_id,
    subject: BindingSubject::Principal {
        id: ci_id,
        kind: PrincipalKindTag::Service,
    },
    role_id: editor.id,
    scope: PermissionScope::Card(CardPermissionScope::Family {
        kind: CardKind::Model,
        name: card_name("fraud-model")?,
    }),
    source: RoleSource::Direct,
};
```

`viewer` is the existing tenant role record: its ID goes in the binding, while
its operations stay in the one role store. Alice's binding does not embed a copy
of viewer or change that role. The editor role works the same way.

For the Card-read part of viewer, resolution is:

```text
Role viewer:
  Operation { resource: Cards, action: Read }

Alice's binding:
  role_id = viewer.id
  scope = Card Family(Repository, payments)

Resolved grant for Alice:
  Permission {
    resource: Cards,
    action: Read,
    scope: Card Family(Repository, payments)
  }

GET a payments version:
  required = Cards + Read + Card Version(payments, exact UID)
  family covers exact version → allowed

GET another Repository:
  required = Cards + Read + Card Version(other family, exact UID)
  family does not cover it → denied
```

Viewer's derived execution/evidence operations are compiled using that same
parent scope. Its unrelated gateway/connection operations yield no grant from
this binding. This is one binding producing several applicable permissions,
not a binding granting every operation on every resource in the role.

The CI binding gives the Model-family authoring permissions present in editor;
it does not make CI a tenant editor. It cannot create another Model family,
administer accounts or invoke an unrelated gateway. It also does not by itself
authorize a selected external Verifier: invoking that Verifier needs a separate
applicable binding. For example, a narrow custom role can contain:

```rust
let runner_operations = vec![Operation {
    resource: Resource::Verifier,
    action: Action::Run,
}];
```

Bind that role to CI at the selected Verifier family/version. Add read or gateway
operations only when the actual workflow requires them. The original Model
editor arrow was a scoped authoring example, not a complete CI credential setup.

If this CI account is the runtime account for Service/checkout, that association
is its existing `WorkloadRestriction::Family(WorkloadFamily { kind: Service,
name: checkout })`, separately from the Model editor binding above. The SDK's
loaded Service supplies the exact release UID through workload_uid. Neither
binding nor restriction stores an account-per-release identity. Preserve this
existing workflow; the revised permission model changes what it may do, not
how the Service proves its account or attributes its running release.

To use a group instead, keep Alice's role and scope but set the subject to
`BindingSubject::Group(reviewers_id)`. A separate membership record links the
tenant-qualified Alice account to reviewers. Group expansion finds the same
binding and produces the same permission; no group ID goes in Alice's token.

## Principal and effective permissions

### Recommended practice and request cost

For Wyrd, use locally verified tokens plus cached, already-resolved account
permissions. Do not authenticate the human again or derive permissions from roles
and groups on each request. The normal cost is token validation, an in-memory
view lookup, and a permission/scope test. Identity verification and checking a
requested permission remain necessary even when the permission set is cached.

| Work | When it happens |
|---|---|
| Human sign-in | Login or required session renewal, not each API operation |
| JWT signature, issuer, audience and expiry validation | Each bearer request, locally |
| Resolve direct/provider/group role bindings into account permissions | First cache population and relevant authorization changes |
| Background database revision confirmation | Batched refresh; unchanged revision does not rebuild account permissions |
| Retrieve resolved permissions | Process-memory lookup on a warm request |
| Decide whether the requested action/resource is covered | Each protected operation, against the cached view and restrictive layers |
| Permission-state database load | Cache miss, invalidation or required recovery, never the normal warm path |

Keep the existing permission resolver as the compilation owner; cache its result
under tenant, account kind/ID and the validated authorization revision. Reuse the
result rather than rerun compilation when constructing each request Principal.
The snapshot supplies current credential/account state too. Signed ceilings are
restrictions evaluated with that view, not a reason to re-expand role bindings.
The cache stores grants and indexes, not an unconditional allow decision for an
account: different requested resources and actions still receive different checks.

An existing browser session can require its existing database validation; that is
separate from deriving grants, and changing that lifecycle is not implied here.

This is the recommendation for Wyrd, not a universal rule that JWT permission
claims are wrong. Self-contained short-lived grant tokens are simpler when
expiry-based permission changes and store-free operation on a cold process are
acceptable. The selected cached model adds invalidation/recovery to make healthy
grant changes govern existing sessions without rebuilding permissions on every
request. Cold starts still need state; caching cannot make that dependency vanish.

The signature verifier must no longer manufacture an authorized Principal from
the JWT's grants. It returns verified claims. The existing server authentication
owner then validates current account/credential state, acquires a snapshot and
constructs the request Principal.

```rust
/// Authority available to one request, without copying a tenant's grants.
pub struct EffectivePermissions {
    grants: Arc<PermissionSet>,
    ceilings: Vec<Arc<PermissionSet>>,
}

impl EffectivePermissions {
    /// Each layer is additive internally; all independent layers must allow.
    pub fn contains(&self, required: &Permission) -> bool {
        self.grants.contains(required)
            && self.ceilings.iter().all(|set| set.contains(required))
    }
}

/// The authenticated tenant account and this request's resolved authority.
pub struct Principal {
    pub id: PrincipalId,
    pub kind: PrincipalKind,
    pub tenant_id: DataTenantId,
    pub workload_restriction: WorkloadRestriction,
    pub effective_permissions: EffectivePermissions,
    pub credential_id: Option<CredentialId>,
    pub card_access: CardAccess,
    pub workload_uid: Option<CardUid>,
}
```

This is a suggested internal representation, not a required new public interface.
The workload restriction is projected from existing account state; showing it
here does not require a new persisted User field or a different login mechanism.
Ordinary Users retain their existing absence of a runtime-family restriction.
`EffectivePermissions` earns its place by keeping current grants separate from
mandatory restrictions without flattening/copying large intersections per request.
It adds no policy language or second checker. The existing checker can still call
`principal.effective_permissions.contains(required)`. Snapshot freshness and
active-account checks occur before that pure decision.

`grants` is a cached account view of applicable direct, provider and group
bindings at the current tenant revision. Mandatory layers include the signed
token ceiling, current credential ceiling, current grants of delegation actors,
and accepted execution ceilings when applicable. They are conjunctive; they
cannot supply authority absent from the subject's current grants. Actor identity
alone remains attribution, not a grant. No global group expansion is performed
for every row or request.

Drop Principal's copied token-role list from authorization. Current role/binding
information is available through the access-summary API; a bare `viewer` name
without its scope does not describe access accurately. Tenant and platform
Principal projections remain separate. SYSTEM remains credentialless and cannot
authenticate through public token verification.

## Tokens: identity plus optional permission ceiling

Token-field decision: locked by the maintainer on 2026-11-19. Preserve the
agreed field responsibilities below; remaining design work does not reopen them.

In plain language, a ceiling is the maximum access allowed through this token.
It can reduce the account's current access, never add to it. For example, Alice
may read and write Repository payments, but issues a script key limited to read
payments. That script's token cannot write, even though Alice can. If Alice later
loses read access, the token cannot read either. Ordinary UI/CLI login needs no
additional token ceiling and uses Alice's current grants.

The concept already exists for personal keys: the current server reads the
stored `permission_ceiling` and intersects it with the account's permissions in
`TenantTokenIssuer::resolve_grant`. Today the resulting restriction is carried
inside the JWT's authoritative permissions snapshot. Delegation similarly puts
its narrowed intersection in that snapshot. Removing the snapshot must not
discard those restrictions. The new claim makes the remaining restrictive
information explicit; it does not create another grant system or imply
permission derivation on every request. Extending typed restrictions to Service
credentials and giving the JWT field a distinct name are changes to the existing
representation, not existing shipped behavior.

**Do not keep the current `permissions` claim as a grant snapshot.** Keep the
existing signature/issuer/audience/expiry checks, identity, actor attribution and
workload context. A restricted token carries scoped permissions only as an upper
bound, with a distinct name and meaning.

```rust
/// Revised tenant JWT claims; selected fields shown, encoding still validated.
pub struct AccessTokenClaims {
    pub version: u16,
    pub iss: String,
    pub aud: TokenAudience,
    pub sub: PrincipalId,
    pub principal: TokenPrincipalRef,
    pub iat: u64,
    pub exp: u64,
    pub jti: String,
    pub cid: Option<CredentialId>,
    pub permission_ceiling: Option<PermissionSet>,
    pub card_access: CardAccess,
    pub workload_uid: Option<CardUid>,
    pub act: Option<Box<ActClaim>>,
}
```

Keep the agreed explicit contract version and the existing sub/principal match,
actor-chain checks and audience validation. Implement the revised token contract
directly; there is no old-token decoder, conversion or credential-reissue campaign.
Numeric field types and serde spelling are illustrative; avoid unrelated encoding
changes. Nothing has shipped.

| Token origin | Permission information carried |
|---|---|
| Normal human CLI login/refresh | No grant list; `permission_ceiling` absent |
| Ordinary unrestricted Service key exchange | No grant list; identity and credential reference remain |
| Restricted personal or Service key exchange | Explicit signed ceiling, bounded by the key's allowed scope |
| Delegation | Required signed ceiling bounded by effective authority of subject and actors at exchange and all inherited restrictions |
| Browser session | No JWT in the cookie; opaque session ID resolves identity and current authority server-side |

`None` means no additional token-specific restriction; current account authority
is still required. `Some(empty)` means no operations are allowed. Invalid claims
are refused, never converted to `None`. Roles and group memberships are not
token claims and resource version lists are not expanded into tokens.

This choice has observable behavior: granting Alice a new family makes it usable
with her existing normal token once server state refreshes. A restricted key or
delegated token cannot acquire authority beyond its frozen ceiling. Refreshing a
normal human login obtains current authority without freezing obsolete grants;
refresh does not silently remove restrictions from a restricted login.

For personal keys, creation freezes a ceiling covered by the creator's authority;
exchange and requests also consult the key's current state/ceiling. New account
grants outside that frozen limit cannot enlarge the key. Removing a grant takes
effect through current server state even while an old signed ceiling includes it.
No token is a substitute for active credential/account state.

Delegation computes the signed ceiling from both participants' effective
authority, not merely their role names or the subject's supplied JWT. Preserve
prior ceilings and the existing actor chain, maximum depth, root restrictions,
and child-expiry limits. Runtime checks also require actors' current authority.
Reject an unrepresentable intersection rather than widening it; family/version
scope inclusion remains symbolic. Encoded-token limits still apply; do not add
an opaque restriction database or invented capacity quota to bypass them.

## One request from token to decision

Alice asks to read an exact Repository Card:

```text
JWT verification
  → Alice's verified tenant/account/credential and signed restrictions
  → server auth owner acquires a usable current authorization snapshot
  → resolve Alice's direct/provider/group bindings from that snapshot
  → attach mandatory credential/delegation ceiling layers
  → construct Principal
  → resolve the Card's trusted family and UID
  → check Cards + Read + Card Version(family, uid)
  → stage decision on the Scribe outbox
  → return the authorized Card
```

The checker stays synchronous. Acquiring authorization state can wait on the
sanctioned permission reload for a cache miss/invalidation; warm checks do not
query PostgreSQL. The snapshot's server-side revision and database-confirmed
freshness enforce the agreed outage bound. Neither a fresh token nor an arbitrary
request renews snapshot freshness.

Every public ingress must acquire this same server-resolved authority, including
HTTP, MCP, gateway, OTLP and native Bifrost. Passing a signature-verified identity
straight to Gate is no longer enough. Long-running streams reacquire a usable
view before later output; keeping an old Arc does not extend its freshness.
Peer execution binds the same caller authority and cannot replace it with SYSTEM.

To show what the check looks like, using the existing Permission shape:

```rust
let required = Permission {
    resource: Resource::Cards,
    action: Action::Read,
    scope: PermissionScope::Card(CardPermissionScope::Version {
        kind: resolved.kind,
        name: resolved.name,
        uid: resolved.uid,
    }),
};

state.permission_check.check(&caller.principal, &required)
```

The example omits the existing audited boundary/error mapping, which production
must retain. `resolved` is trusted server context, never an unchecked request DTO.

## Execution parents and secured queries

Locked Repository rule (2026-11-19): Repository Card read includes its code-review
results, review sets, member executions and retained evidence. This applies to a
custom role granting Repository Card read as well as viewer; it is fixed parent
inheritance, not a role-name bypass. Where the internal catalog separates
execution/evidence reads, compilation must include those reads at the Repository
parent scope. No separate referenced-Verifier read or GitHub-configuration grant
is required to inspect that content. Opening a referenced Verifier definition
still checks that Card independently. Starting reviews, changing policy and
approval are separate actions. GitHub setup/inventory pages and their UI/HTTP
routes require admin or explicit GitHub-configuration permission, enforced by
Rust. See the [locked boundary in the main design](notes.md#github-connection-versus-repository-card--locked).

```rust
/// Trusted retained Card parent of verification data.
pub struct AuthorizationParent {
    pub kind: CardKind,
    pub name: CardName,
    pub card_uid: CardUid,
}
```

Tenant identity stays with the execution/row's existing tenant envelope. The
parent is recorded at admission. For PR work it is the Repository, not the
shared Verifier. Family and exact-version comparisons use this same retained
parent after Card or queue-row deletion. If existing retained fields already
prove it, reuse those fields rather than duplicate a serialized struct.

This access parent must not overwrite or reinterpret the owning workload Card,
subject Card, exact Verifier, workload_uid or subject/actor attribution. Those
retain their existing independent meanings. Access filtering consumes a trusted
parent; it does not change which component ran or who performed the operation.

Distinguish definition operations from derived execution operations in the
closed catalog. Reading a Verifier definition and reading an execution that
used it are different checks. Add/clarify the execution-read operation where the
existing `verifier:read` conflates them; the concrete variant name is
implementation-owned, but the access boundary is material. Starting work still
checks selected Verifier run authority as well as its subject/Repository
prerequisites. An approval checks Verifier approve authority plus parent
visibility; it borrows neither the submitter's nor the approver's broader rights.

For each secured verification source, Oracle compiles the same grants and
ceiling layers into a visibility predicate over trusted parents. It cannot use a
boolean “has table read” as a substitute for parent checks. Apply the predicate
before aggregation and user expressions on every storage and peer path. This
is a projection of the one access model, not another role resolver.

The example Alice-with-payments access must produce identical membership for
Card lists, review-set lists, execution lookup, publication download and raw SQL
counts. A ceiling restricting her to one exact version narrows every one of
those surfaces consistently.

## Why the current permissions claim exists

It is not an accidental field or inherently incorrect JWT practice. The current
architecture deliberately chooses a short-lived, self-contained authorization
snapshot:

```text
issuance: current roles → resolved PermissionSet → signed JWT permissions
request: verify signature/expiry → Principal with that set → pure check
```

This provides store-free authorization on every bearer ingress, avoids
permission-cache coherence, gives each token stable authority for its lifetime,
and encodes delegation's attenuated intersection. An already-issued token does
not need PostgreSQL merely to prove its permissions. The current five-minute
expiry bounds how long removed grants remain effective, although other IO may
still fail independently during an outage.

Object scopes could be added while retaining that pattern. Scoped RBAC does not
technically require removing JWT grants. The reason for the selected revision
is to make current server grants govern all surfaces, reduce healthy offboarding
delay, and avoid repeatedly embedding account-wide scoped grants in tokens.
Its cost is authorization state, invalidation and recovery at every ingress.
An ordinary token also gains newly assigned permissions before expiry; that
intentional change is why restricted/delegated tokens retain frozen ceilings.

### What removing the claim affects

| Current dependency | What breaks if only the field is removed | Required replacement |
|---|---|---|
| Shared issuer `AccessGrant` / claim construction | Still requires and signs the resolved permission snapshot | Issue identity plus the appropriate ceiling; preserve claim validation |
| `AccessTokenClaims::into_verified` | Directly copies `self.permissions` into Principal today | Return verified identity/restrictions; server resolves current authority before constructing an authorized Principal |
| HTTP authenticated-principal and Caller extractors | Expect the verified token already to contain usable authority | Use the shared server resolver before the handler |
| Gate's native ingest authentication | A synchronous verifier currently constructs its authorized context with no server lookup | Host resolves authority before Gate; standalone signature verification alone cannot authorize ingest |
| gRPC/OTLP, query and gateway ingress | Any path consuming the old authorized Principal can deny everything or bypass the intended live-grant checks if adapted inconsistently | Inventory each path and route it through the same authority acquisition; no token-snapshot fallback |
| Delegation exchange | Intersects the subject's signed set with the actor's resolved set today | Resolve current subject/actor authority and preserve every inherited ceiling before issuing the new one |
| Personal key exchange | Currently bakes the account/key intersection into token permissions | Preserve frozen key limits in the signed ceiling and current credential checks |
| `AcceptedAuthority` and execution recovery | Persist/copy a Principal with a concrete permission snapshot today | Persist a serializable accepted ceiling and identity/actor context; reconstruct current views on recovery without widening accepted authority |
| Principal/`AuthContext::effective_permissions` and peer/query envelopes | Code expects a concrete PermissionSet in memory and serialization | Adapt shared accessors and distributed proof/digest handling to the effective view; never serialize cache Arcs as an execution contract |
| Current token consumers and fixtures | Their source still expects the existing field shape | Update producers, consumers and fixtures together to the revised contract; no deployed-version support or reissue workflow |

The standalone Gate path is a concrete integration constraint: its present
`authenticate` function verifies a JWT and immediately constructs AuthContext.
Adding a resolver only to HTTP would leave native ingest outside the revised
model. Keep the interceptor's cryptographic validation synchronous; acquire any
needed state in the async hosting boundary, then hand Gate resolved authority.
If a deployment has no usable current-authority snapshot, it refuses rather than
treating verified identity as permission. No database dependency is added to the
shared signature verifier or an ad hoc runtime inside the interceptor.

For queued work, the accepted ceiling is a durable plain PermissionSet, not the
request's Arc-backed EffectivePermissions. The ceiling preserves the authority
under which work was admitted, including all then-effective restrictions. A retry
or restart combines it with current subject/actor authority. A later grant must
not expand already-accepted work, and unrepresentable accepted authority must be
refused rather than serialized as an unrestricted ceiling. Existing cancellation
and attribution remain intact. The persisted representation must support normal runtime retry/recovery. No
historical-state conversion is required.

### Observable behavior changes and limits

- Healthy grant removal, account suspension and key revocation reach existing
  tokens through server state, rather than waiting only for token expiry.
- Ordinary tokens can use new grants after refresh of server state. Restricted
  tokens and accepted work remain bounded by their original ceilings.
- A cold process or known-invalidated snapshot cannot authorize from a token
  alone. An unknown removal during a partition may remain allowed for the
  maintainer-approved five-minute maximum since database confirmation.
- Verification remains local for signatures, but complete request authorization
  is no longer independently offline. Warm requests remain memory-only; cold
  permission state may require the sanctioned PostgreSQL reload.
- SDKs should continue treating access-token strings as opaque. That preserves
  token transport, not every auth API: role/binding DTOs and typed key ceilings
  still require SDK changes. External software decoding the old claims breaks;
  no compatibility alias or dual token-authority mode is proposed.
- Browser cookie format need not change for this reason; permission metadata and
  authorization acquisition do. Browser database availability is still separate.

The change is therefore a revision of an existing authorization architecture,
not a field rename. Do not remove the old claim before its responsibilities have
replacement owners and journey proof.

Verified source references for this dependency analysis:

- [JWT-to-Principal conversion](../../../../crates/shared/wyrd-auth-verify/src/lib.rs)
  (`AccessTokenClaims::into_verified` copies the permission set).
- [Shared signing](../../../../crates/shared/wyrd-auth-issue/src/lib.rs)
  (`build_access_claims` consumes `AccessGrant.permissions`).
- [Token issuance and delegation](../../../../crates/wyrd/wyrd-auth/src/issuance.rs)
  (the grant compiler intersects subject permissions and actor authority).
- [Native Gate authentication](../../../../crates/vala/vala-bifrost-redux/src/gate/auth.rs)
  (`authenticate` produces authorized context directly from verified token).
- [HTTP extraction](../../../../crates/wyrd/wyrd-server/src/components/auth/principal_extractor.rs)
  and [gRPC hosting](../../../../crates/wyrd/wyrd-server/src/grpc/mod.rs).
- [Accepted execution authority](../../../../crates/shared/wyrd-runtime/src/request_context.rs)
  (`AcceptedAuthority` retains the concrete Principal and delegation chain).
- [Current token authority contract](../../../../architecture/wyrd-security-posture.md#access-and-refresh-tokens).

## Implementation-owned details

These sketches record the settled responsibilities without fixing map types,
cache implementation, public DTO packaging or private helper signatures.
[Specification revision 2](../spec.md#persistence-and-authority-changes) names the
bounded persistence changes. Its behavioral and schema rules govern the
implementation; no additional schema purpose or public authority is implied by
an illustrative field or helper here.

Validate the selected semantics through the journey/proof plan in notes.md,
including grant addition under a normal token, grant removal under an old
ceiling, empty versus absent ceilings, family/exact intersection, unrelated
Verifier access, snapshot outage expiry and delegation actor revocation. These
sketches have not been compiled.

## Current source references

- [Runtime Permission and PermissionSet](../../../../crates/shared/wyrd-runtime/src/permission.rs)
- [Current Principal, RoleRef and delegation participant](../../../../crates/shared/wyrd-runtime/src/principal.rs)
- [Current PermissionScope](../../../../crates/wyrd-spec/src/auth/permission_scope.rs)
- [Assignment provenance](../../../../crates/wyrd-spec/src/auth/tenant_principals.rs)
- [Role persistence](../../../../crates/wyrd/wyrd-sql/src/row_types/auth/roles.rs)
- [Current JWT claims](../../../../crates/shared/wyrd-auth-verify/src/lib.rs)
- [Grant issuance](../../../../crates/wyrd/wyrd-auth/src/issuance.rs)
- [Existing token request contracts](../../../../crates/wyrd-spec/src/auth/token.rs)
