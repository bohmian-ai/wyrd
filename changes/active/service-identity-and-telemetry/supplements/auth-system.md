---
spec: SPEC-service-identity-and-telemetry
spec_revision: 12
status: approved
---

# Supplemental authentication contracts and complete flows

This is a normative part of [specification revision 12](../spec.md), approved together with the [telemetry supplement](telemetry-schemas.md). It defines
the proposed system, not behavior already implemented. The main specification's
requirements and invariants apply throughout.

## The model in one example

An administrator creates account `checkout-runtime`. The server assigns
principal P, grants its roles, associates it with the Service family
`checkout`, and returns one account key K. Registration of checkout v1 and v2
creates Card UIDs R1 and R2, not authentication principals.

```text
K + R1 -> JWT for P, permissions from P, allowed Card UIDs from R1's graph
K + R2 -> JWT for P, permissions from P, allowed Card UIDs from R2's graph

v1 observation: actor P, owner R1, observed Card M1
v2 observation: actor P, owner R2, observed Card M2
```

The key determines the account. The selected UID determines execution context
and its graph limit. No selector changes the account, permissions, or tenant.

For checkout v2 using account `checkout-runtime` to observe Model M2:

| Stored field | Who or what it identifies |
| --- | --- |
| `sub_principal_id` | The `checkout-runtime` account; Alice's account instead when acting on her delegated request. |
| `act_principal_id` | The `checkout-runtime` account performing the action. |
| `workload_card_uid` | R2, the exact checkout v2 Card UID. |
| `subject_card_uid` | M2, the exact Model UID validated within R2's graph. |

These are stored evidence names. Token field `workload_uid` and header
`x-wyrd-workload-uid` still select the exact workload context; they are not
renamed. The stored subject column replaces existing `card_uid` without
changing its meaning or the canonical OTLP attribute `wyrd.card_uid`.

A User with an existing access token remains that User. Administrative grants
allow registered-Card operations without a root. If that User supplies a root
to claim Service ownership, the server checks that ownership independently;
admin authority does not make a false root/component relationship true.

## Current system and the precise changes

| Boundary | Current behavior | Proposed behavior |
| --- | --- | --- |
| Principal creation | Explicit unbound Service accounts exist; registration also creates version-bound Service/Agent principals. | Explicit stable accounts only; registration creates no auth principal or role grant. |
| Nonhuman kind | Service and Agent. | Service; Agent remains a Card kind. |
| Credentials | Principal-owned credentials exist; TASK-002 adds principal-free family deployment keys. | All keys retain one account owner; no deployment-key class. |
| Role authority | Issuer resolves the selected principal's roles. | Issuer always resolves the authenticated account's roles; root selection never selects another principal. |
| Root and graph | Card fields are embedded in the token principal and its runtime kind. | Account references contain identity only. Separate signed execution context and Card-access limit preserve the graph. |
| Missing root | Rootless User, TenantAdmin, and unbound Service mean any registered observation target, subject to operation permission. | Preserve that for unrestricted callers; represent a restricted account awaiting a root explicitly. |
| Delegation | Subject and actor tokens, permission intersection, account/actor attribution; actor graph context is reduced. | Same exchange, plus exact effective Card-limit intersection and preserved workload/provenance. |
| Background work | Some activity and authority recovery depend on version principals. | Explicit account plus exact root activity and accepted authority; no principal-to-version inference. |
| Evidence | Component Card UID plus version-principal publisher; verification separately has owner UID. | Stable account publisher plus first-class owner UID and component UID on signals and verification. |

The existing owners remain the principal administration service, shared
credential resolver/client, token exchange and issuer, graph resolver, verifier,
permission checker, verification admission/queue, and Bifrost admission. This
does not create another token endpoint, credential store, permission engine,
graph traversal, or SDK-owned durable workflow.

## Principal kinds and assignment

| Principal | How its identity is assigned | Result of this change |
| --- | --- | --- |
| Tenant User | Existing tenant human identity mapping and login. | Same principal, roles, login, and refresh; no workload prerequisite. |
| TenantAdmin | Existing tenant provisioning. | Same headless tenant administrator; wildcard grants remain ordinary permissions. |
| Service | Existing explicit account creation, server-assigned ID. | Stable nonhuman account for Services, Agents, CI, and automation. |
| Agent | Currently projected from an Agent Card version. | Remove the auth kind from the fresh baseline; Agent applications use explicit Service accounts. No old-account migration is required. |
| System | Existing tenant-local server provisioning. | Internal, credentialless actor only; no public token, account-management, or delegation path. |
| Platform GlobalAdmin/User | Existing tenantless platform provisioning/login. | Unchanged platform plane; never accepted as tenant authority. |

Runtime principal identity is account ID, kind, tenant, roles, effective
permissions, and optional credential provenance. A root is not a principal-kind
payload or part of durable account identity. Authenticated request context must
carry Card limits and per-participant workload UIDs alongside the account.
Private Rust struct layout remains implementation-owned.

## Account restrictions and administration

### One explicit restriction contract

The proposed `workload_restriction` is a closed JSON union:

```json
{"mode":"unrestricted"}
```

```json
{"mode":"family","kind":"Service","name":"checkout"}
```

Family kind is Service or Agent. The family is resolved in the account's
tenant. Unrestricted means no additional workload-family limit, not an
operation grant or cross-tenant access. The former migration-only exact-root
restriction is removed: the v0.3.0 baseline needs no legacy-account conversion.
Exact versions remain request/token workload context, not account identity.

The restriction applies to Card-attributed emission and verification. It does
not turn table-read permissions into a row policy or restrict unrelated
authorized registry operations. A restricted account with wildcard operation
grants is still explicitly restricted; a User or TenantAdmin is not implicitly
given that Service-account restriction.

A family binding establishes the unique runtime-account association used for
that family's automatic schedules. A second family-bound runtime account for
the same tenant/kind/name conflicts. Use multiple credentials on the same
account for rotation and concurrent releases. Other unrestricted automation and
Users can perform separately authorized direct work without becoming scheduled
authority. A caller does not become family runtime authority merely by
selecting a root.

### Requests and responses

Extend existing `POST /v1/principals` creation with this one optional field:

```text
CreateServicePrincipalRequest:
  name: string                         required, existing tenant-unique account name
  roles: string[]                      required, existing role names
  description: string | null           optional, existing
  workload_restriction: Restriction    optional; omitted means unrestricted

CreateServicePrincipalResponse:        unchanged
  principal_id: PrincipalId
  credential: secret string            returned once
```

The server resolves roles and validates the restriction before atomically
creating the account, grants, association, and first credential. Failure creates
no partial account and returns no secret. Keep the existing account-creation
permission. Creating or changing a family runtime association additionally
requires existing tenant-administrative wildcard permission; a role name alone
does not prove it.

Account listing remains `GET /v1/principals` with the existing kind/email/name
and cursor filters. Assignable kinds become User and Service. Its summary is:

```text
PrincipalSummary:
  principal_id: PrincipalId
  kind: user | service
  status: active | suspended
  email: string | null
  name: string | null
  workload_restriction: Restriction | null
```

`workload_restriction` is explicit for Service and null for User. Remove the
summary's old version `card_ref`; a stable account has no mutable "current
release." User creation/login remains under existing human identity owners.

Add `PUT /v1/principals/{principal_id}/workload-restriction` to the existing
principal administration owner. Its body is one complete Restriction value and
its response is PrincipalSummary. It requires tenant-administrative wildcard
permission, accepts Service only, is idempotent for an unchanged value, and
atomically checks/releases/acquires any unique family association. It grants no
roles, moves no credentials, changes no principal ID, and issues no token.
Conflict changes nothing. Changing scheduled association requires new
qualifying activity by the new account; old account activity is not inherited.

Existing direct-role administration remains:

```text
GET    /v1/principals/{principal_id}/roles
PUT    /v1/principals/{principal_id}/roles/{role}
DELETE /v1/principals/{principal_id}/roles/{role}
```

Keep assignment source semantics: human login updates IdP assignments only;
direct grants survive login. Restriction or role changes affect the next
issuance, not already-issued snapshot tokens. Do not add a live tenant-token
authorization epoch or a role-name bypass.

## Credentials, rotation, and revocation

Every stored API-key verifier has one principal owner. The server returns the
secret once and retains only its hash, lookup/lifecycle metadata, and owner.
Use existing secret generation, expiry policy, audit, and key verification;
there is no version field on the credential and no plaintext in Cards, state
bundles, logs, claims, or queued records.

The existing credential contracts remain:

```text
POST   /v1/principals/{principal_id}/credentials
  response: { id: CredentialId, credential: secret string }

GET    /v1/principals/{principal_id}/credentials
  response: { credentials: CredentialMetadata[] }
  metadata: { id, prefix, created_at, expires_at?, revoked_at?, last_used_at? }

DELETE /v1/principals/{principal_id}/credentials/{credential_id}
  response: { revoked: true, credential_id }
```

Ownership and tenant checks apply to both path IDs. Listing returns no secret.
Rotation is issue, verify the new key, deploy it, then revoke the old key;
overlap is permitted. Releases do not require rotation. Revocation prevents
fresh exchange/direct-key authentication. Existing tenant JWTs retain their
normal bounded lifetime; accepted queued work retains its accepted authority.
Neither token nor queue storage turns non-secret credential provenance into a
reusable credential.

## Token endpoint and grant contracts

`POST /auth/token` retains form encoding, the current OAuth envelopes, and its
existing grant types. This is not a new OAuth grant or a new endpoint.

| Grant | Complete request fields | Root behavior |
| --- | --- | --- |
| `refresh_token` | `refresh_token` | Existing human renewal; no machine/root prerequisite. |
| `urn:ietf:params:oauth:grant-type:device_code` | `device_code` | Existing approved human/device flow. |
| `urn:ietf:params:oauth:grant-type:token-exchange`, API-key subject | `subject_token`, `subject_token_type=urn:wyrd:oauth:token-type:api_key`, optional `workload_uid`; actor fields/audience absent | Key selects its owner; UID optionally selects context. |
| Same token-exchange grant, access-token subject and actor | `subject_token`, `subject_token_type=urn:ietf:params:oauth:token-type:access_token`, `actor_token`, matching `actor_token_type`, optional `audience=wyrd|bifrost`; `workload_uid` absent | Preserve subject; current Service actor supplies its already verified root. |
| `urn:ietf:params:oauth:grant-type:jwt-bearer` | `assertion`, optional existing `tenant` selector, **new optional `workload_uid`** | Configured verified issuer mapping selects account, not an arbitrary assertion-selected principal; optional UID selects context. |

Keep existing refusal of unsupported field/type combinations, duplicate
parameters, unsupported audiences, and missing required evidence. A raw UID is
not an RFC 8693 subject token and does not independently authenticate anyone.
Account root selection is Wyrd's authorized context extension.

TokenResponse is unchanged:

```text
access_token: secret string
token_type: "Bearer"
expires_in: unsigned seconds
refresh_token?: secret string
issued_token_type?: token-type URI
```

Keep tenant access lifetime at the existing five-minute maximum and human
refresh policy. Only the existing human CLI/saved-login flow receives refresh
tokens. API-key, external workload, and delegated tokens have no refresh token.
Platform and browser session policies remain under their existing owners.

### Direct account-key exchange

1. Verify the key once using its existing tenant-bound lookup and hash path.
   Reject unknown, expired, revoked, suspended, or mismatched credentials
   according to existing non-enumeration rules.
2. Resolve the owning active account and its current roles, restriction, and
   effective permissions. Never accept principal ID or tenant authority from
   the request.
3. With an explicit UID, resolve an active root in that tenant, check kind and
   restriction, and derive the existing bounded transitive observation-target
   graph. A family never supplies latest or a guessed release.
4. Issue the account token with the explicit Card-access state below. Without
   a root, a family-restricted account authenticates with `root_required`; an
   explicit unusable UID is refused rather than silently ignored.
5. Record credential/account use and, only when this is qualifying machine
   runtime use with a root, root/account activity. Stage normal permission audit
   without blocking on retained audit delivery.

## Complete proposed tenant JWT claims

Keep the registered claims, roles/permission encoding, and credential field.
Separate identity from Card context rather than continuing to encode execution
as a principal kind. The following is the full proposed claim set:

| Claim | Proposed type and meaning | Change |
| --- | --- | --- |
| `sub` | PrincipalId text, exactly equal to `principal.id` | Existing; require equality on verification. |
| `principal` | `{id, kind, tenant_id}` | Existing account fields; remove embedded `card_ref`/`card_ref_scope`. |
| `roles` | Existing RoleRef array, informational | Unchanged; never authorize from it. |
| `permissions` | Existing typed PermissionSet | Unchanged role-derived operation/object grants; delegation intersects them. |
| `workload_uid` | Optional exact root CardUid for the subject's selected execution context | New; replaces the root meaning of embedded `card_ref`, not account identity. |
| `card_access` | Required explicit Card-access state defined below | New explicit form of the existing graph limit. |
| `act` | Optional outer-current-actor claim, with earlier actors nested | Existing structure, extended below. |
| `aud` | Existing `wyrd` or `bifrost` audience | Unchanged. |
| `exp` | Existing expiry, Unix seconds | Unchanged lifetime policy. |
| `iat` | Existing issue instant, Unix seconds | Unchanged. |
| `iss` | Configured issuer | Unchanged. |
| `jti` | Existing token identity | Unchanged. |
| `cid` | Optional non-secret credential/refresh provenance ID under the existing grant rules | Unchanged direct provenance; not authorization. |

Token account kinds are `tenant_admin`, `user`, and `service`. Reject tenant
tokens naming `system`, `global_admin`, or removed `agent`. Platform claim
shapes and platform validation remain separate and unchanged.

The Card-access claim is a closed union:

```json
{"mode":"any_registered"}
```

```json
{"mode":"root_required","restriction":{"mode":"family","kind":"Service","name":"checkout"}}
```

```json
{"mode":"scoped","cards":["019a7c00-0000-7000-8000-000000000001","019a7c00-0000-7000-8000-000000000002"]}
```

`any_registered` retains tenant registry validation and operation checks; it
is not a wildcard grant. `root_required` contains a family restriction,
never unrestricted. It permits ordinary authorized non-Card/generic operations
but supplies no Card-attributed emit/verify authority until a root is validated.
`scoped.cards` is the exact authoritative CardUid set, produced by the existing
graph resolver, not authored names or strings requiring space resolution.
It may be empty after delegation and then authorizes no Card. No empty/missing
claim seeds root-only scope or becomes unrestricted.

At direct issuance:

| Account/context | Signed Card access |
| --- | --- |
| User/admin/unrestricted Service, no root | `any_registered` |
| Family-restricted Service, no usable selected root | `root_required` with that restriction snapshot |
| Any permitted direct root selection | `scoped` with the complete exact root graph; `workload_uid` identifies its root |

The scoped UID set avoids duplicating full graph CardRefs in claims. Direct
root graphs include the root; delegated intersections need not. Preserve the
current traversal depth 16 and membership bound 32. Issuance must also fit the
existing 8 KiB encoded bearer limit; otherwise refuse without truncating scope,
permissions, or actors. Do not mint tokens the existing verifier cannot admit.

This Card-access claim limits existing Card attribution/verification; it is
not a third permission system. `permissions` still supplies every operation
grant. The two dimensions are checked independently and cannot expand each
other. Do not derive operation authority from `act`.

### Actor claims

Each actor layer has the full proposed shape:

```text
ActClaim:
  sub: PrincipalId text
  principal: { id: PrincipalId, kind: service, tenant_id: DataTenantId }
  workload_uid: CardUid                 current actor must have a resolved root
  cid?: CredentialId text              new optional actor-credential provenance
  act?: ActClaim                       prior actors, current actor outermost
```

`sub` must equal that layer's account ID. The actor UID is context, not a new
principal. Preserve prior actors' workload/provenance when nesting. Their
historical claims confer no authority. The top-level effective `card_access`
is what authorizes Card-sensitive work; do not reconstruct authority from
missing actor graph fields or query current roles during normal token use.

A delegated token keeps the subject's top-level `workload_uid` if present.
The acting/owning root is the outer actor's UID, or the top-level UID when no
actor exists. Reject inconsistent tenant/kind/ID/context shapes rather than
silently dropping provenance. Top-level `cid` remains absent for delegation;
actor `cid` retains the verified key ID when present. No token contains a key
secret, external assertion, refresh secret, or server credential.

### Verification and request context

Verify the existing signature/EdDSA policy, key ID, issuer, audience, expiry,
clock-skew allowance, size, subject equality, tenant equality, canonical ID
grammars, allowed kinds, and delegation depth (maximum five). New tenant
tokens require a valid explicit `card_access`. Scoped UIDs are unique and
bounded. Direct scoped tokens require their workload root to be included;
delegated intersections may exclude both participant roots or be empty.
Direct `any_registered` and `root_required` tokens have no `workload_uid`;
direct `scoped` tokens require one. Delegated tokens require `scoped` access
and a current actor root. Reject other combinations rather than treating a
root-bearing token as unrestricted.

Reconstruct account, effective permissions, explicit Card limit, per-participant
root UIDs, and non-secret provenance. Do not conflate absent context with
unrestricted access. Do not preserve a runtime dependency on the removed
Service/Agent version payloads or a version-principal registry lookup.

Normal signed-token verification is synchronous and store-free. A signed
scope authorizes by UID in memory. Registry work occurs at issuance, when an
unresolved request root is explicitly selected, or for existing unrestricted
registered-Card validation, not as a new lookup for every scoped observation.

## Authorization, including privileged Users

Every protected operation still runs the existing typed permission check.
For Card-attributed emission or verification, also apply the effective Card
limit. For a claimed owner, verify the exact root/subject/binding relationship.
Unrelated registry and table-query operations are not filtered by graph scope.

| Caller | No root | Valid selected root | Out-of-graph subject |
| --- | --- | --- | --- |
| User/TenantAdmin with necessary grants | Registered tenant Card operations allowed; owner null unless an exact binding establishes it. | Owned operations allowed for the root's valid subjects/bindings. | Cannot falsely claim that owner; may perform a separately authorized unowned operation. |
| User lacking the operation grant | Denied. | Denied; context grants nothing. | Denied. |
| Unrestricted Service with necessary grants | Existing registered-Card behavior; generic owner null. | Scoped owned execution. | Selected token/context cannot claim the false owner; a separately issued unbound token keeps the account's intended unrestricted behavior. |
| Family-restricted Service | Generic authorized operations may work; Card-attributed work needs valid context. | Permission and selected graph must both cover the action. | Refused, even if the account has wildcard operation permissions. |
| Delegated caller | Subject/actor grants and Card limits must be resolved as below. | Current actor's root owns the work. | Effective Card limit and owner consistency both apply. |
| System | Existing accepted internal authority only. | Preserve accepted owner; System supplies no grants. | Cannot exceed accepted authority. |

For a rootless bearer, `x-wyrd-workload-uid` may select request context. Resolve
it under the signed Card-access snapshot and account restriction, never by
re-reading broader live grants. For `root_required`, validate the supplied
root against the signed restriction and derive its graph for that request.
For an unrestricted User, preserve grants while enforcing truthful owned
evidence. A previously signed root is immutable: an equal selector is
idempotent, a different selector is refused. Never silently drop a selector or
unpin a token to make the operation work.

## Complete caller flows

### User access token and WyrdState

1. The existing credential chain selects `WYRD_ACCESS_TOKEN` or saved human
   login. Use the User identity and existing renewal path; no machine key or
   workload variable is required.
2. Loading WyrdState from a bundle stays local/offline. A Service/Agent root
   is execution context, not a request to switch principal.
3. On owned IO, the immutable state supplies its pinned root with the existing
   bearer. The server validates it under the User's grants. A component Run
   view changes observed `subject_card_uid`, not account or root.
4. The server stamps User/User, validated owner, and authorized subject.
   A bare authorized User call without any root still works and has null owner
   unless a verified binding establishes an exact one.
5. Refresh/login keeps human identity; it does not fabricate a Service account.
   State request context remains independently pinned across token renewal.

### Service or Agent application with an account key

1. An administrator creates the stable account/restriction and roles once.
2. CI registers a release with a separate authorized registration identity and
   obtains its exact root UID. The application pulls that pinned graph or
   loads its bundle; runtime does not need declaration rights.
3. Deployment injects `WYRD_API_KEY`; explicit workload configuration or its
   WyrdState supplies the pinned UID. `WYRD_WORKLOAD_UID` remains a supported
   context input, not an extra secret or a requirement for all credentials.
4. The SDK exchanges key plus UID, receives the same account principal with
   version-specific Card access, and keeps an immutable context-bound client.
5. Every request uses its JWT. Rows retain account IDs, exact owner UID, and
   component UID separately. Concurrent R1/R2 clients cannot mutate one another.
6. Re-exchange the key with the same root before/after normal token expiry as
   the existing client does. Cache identity includes credential, root, audience,
   and existing issuer/tenant/endpoint dimensions. No latest fallback or
   unbounded retry on an invalid root.

### User A calls Service B, and further delegation

1. A supplies its signed access token to B using the existing application
   delegation convention. B has its own direct root-bound Service token.
2. B submits the two tokens to existing token exchange. Verify both, tenant,
   audiences, direct Service actor/root, current active actor account, and
   depth. Resolve B's current role grants and current restriction; revalidate
   the root already authenticated in its token. Do not select another root.
3. Effective operation permissions are the existing exact intersection of
   subject snapshot and actor's current grants. Effective Card access is the
   intersection of the subject's signed Card-access limit and B's resolved
   graph: any-registered is the identity set; two scoped sets intersect by
   UID. An unresolved restricted subject cannot delegate a Card authority it
   has not established; refuse that exchange with actionable missing-context
   detail. A normal rootless User is not unresolved and is accepted.
4. If permission intersection cannot be represented, refuse rather than widen
   or discard common authority. An empty Card intersection is representable
   and permits no Card-attributed action; otherwise granted generic work may
   remain possible.
5. Issue subject A with B as outer actor, B's workload UID/provenance, and the
   effective permissions/Card-access state. No second B principal is created.
6. B's requests record A/B and B's exact owner. Further Service delegation
   nests verified actors, intersects the already attenuated authority again,
   and selects the new outer actor's exact root as current owner. Historical
   actors are attribution, never a source of grants.

### External workload federation

Keep trusted workload issuer configuration and the existing
`/v1/admin/workload-bindings` owner. Its mapping must target an active stable
Service account rather than look up a principal from a Card version:

```text
CreateWorkloadBindingRequest:
  issuer: IssuerUrl                     existing
  subject: string                       existing verified external subject
  audience?: string                     existing audience constraint
  principal_id: PrincipalId             new stable account target
  workload_uid?: CardUid                optional explicitly configured pinned default

WorkloadBindingView:
  issuer: string
  subject: string
  audience: string | null
  principal_id: PrincipalId
  workload_uid: CardUid | null
```

Remove the old mapping's `card_ref` field. The optional default is context,
not its account identity. Verify issuer/subject/audience and trust policy before
resolving that mapping; request `tenant` remains only the existing routing
selector and never establishes tenant authority. Human issuers are not workload
issuers. The assertion cannot choose a principal by supplying an ID claim.

At jwt-bearer exchange use the explicit UID when supplied, otherwise the
configured default, otherwise ordinary rootless restriction semantics. All root
choices must match the mapped account restriction. A default does not expand
the restriction. Renew by obtaining a fresh external assertion and repeating
the same context; do not issue machine refresh tokens. Configure the stable account mapping explicitly in the fresh baseline; never
guess an account from a Card name.

### Browsers, administrators, platform, and System

Human OIDC, device approval, refresh rotation/reuse detection, saved login, and
Rust-owned browser sessions retain their existing owners and permissions.
Administrative key-based browser recovery resolves the same account and does
not count as workload runtime activity. No new password grant is introduced.

TenantAdmin stays tenant-bound. Platform GlobalAdmin/User credentials and
sessions operate only on the existing platform plane; a workload selector
cannot move them into a tenant. System is never a public token subject or
delegation actor. Internal verification may record the accepted subject with
System as actor while retaining the accepted owner and permissions.

## Public ingress, SDKs, and batching

| Surface | Required behavior |
| --- | --- |
| Ordinary Wyrd HTTP/SDK | Keep existing access-token presentation. Optional root request header is `x-wyrd-workload-uid`; it selects only context. |
| gRPC/OTLP | Use the same optional metadata name. Existing direct-key authentication resolves the account and optional root through the shared auth owner; bearer-only Users remain valid. |
| Gateway/provider SDK | Existing static-key calls still authenticate without UID and may invoke with sufficient grants. Accept the optional root header through the same owner so captures retain exact ownership when supplied. |
| MCP/CLI | Project the same account, restriction, credential, and context operations with typed arguments. No tool argument sets principal or tenant authority. |
| Native Arrow/Bifrost | Preserve Card/Run per-row correlation. Reject public managed owner/principal columns; stamp owner from accepted context. |
| Internal capture/peer forwarding | Preserve each row's verified account pair and exact root; trusted transport does not replace originating identity or allow mixed tenants. |

A stock provider SDK sending only the shared key cannot distinguish releases.
Its generic authorized call still works. To obtain exact Service ownership,
configure the optional header from the deployment's pinned context or use the
Wyrd integration that supplies it. Never infer a release from latest, resource
labels, the key, or activity in another process.

One context-bound client may batch different observed Cards and Runs. Different
root/credential contexts must not share mutable auth state, token-cache entries,
or buffered attribution accidentally. Retain record admission atomicity and
batch idempotency; a retry preserves its original accepted tenant/account/root
context and cannot be replayed under a different owner to change stored rows.

## Verification, queues, schedules, and activity

Direct verification checks the existing operation grant, effective Card-access
limit, exact binding/subject occurrence, and any claimed owner relationship.
Inputs and results preserve account subject/actor plus owner, subject, and
Verifier UIDs independently. Privileged rootless verification remains possible
under existing grants; binding-owned work obtains its exact owner from the
validated binding, not an administrative account's nonexistent root.

On submit, capture the verified effective permission set, explicit Card-access
state, participant root context, subject/actor chain, and non-secret provenance
as accepted authority. The current deliberate removal of a live credential must
not discard its descriptive origin; retain provenance separately from reusable
credential material. Keep the accepted inputs acknowledgement-before-queue
boundary. Claim/retry/restart restores that snapshot without role re-resolution,
new tokens, a new account, or guessing context from account ID.

For schedules, use the unique administratively associated runtime account for
the family, current grants at acceptance, and the binding's exact owner root.
Activity is keyed by exact root and account. Only successful direct machine
credential/assertion use with validated root qualifies, including existing
direct-key ingress where applicable. A User visit, browser recovery, delegated
exchange, generic rootless key use, or cached bearer request does not prove a
release is running. R2 activity cannot reactivate R1; reassociation does not
inherit another account's activity. Missing/inactive account, unusable root,
missing System, or insufficient permission accepts no scheduled work.

System performs only existing internal execution under the accepted authority.
Role/restriction/key changes affect fresh issuance or direct-key admission;
scheduled acceptance rechecks the associated account's current authority and
qualifying activity. Submission through a still-valid JWT retains its signed
authority without a new live role/key check. Already accepted execution
snapshots remain unchanged. Every later protected operation still checks
accepted authority, stages normal audit, and cannot widen it.

## Audit, evidence, and refusals

Direct API-key JWTs name account and `cid`. Delegated decisions name account
subject/actor and optional actor credential provenance; queued decisions retain
accepted origin without holding a live secret. Audit separately records the
validated execution root and the operation's target. Root selection,
restriction mutation, account creation, role changes, and credential operations
use existing permission decision/audit ownership. Do not add a blocking audit
write or a second audit sink.

The [shared table contract](table-storage-and-correlation.md) applies these
identities to all system and dynamic tables. Internal delivery preserves the
original per-record authority; audit preserves the original decision pair and
its existing unresolved sentinel rather than the outbox transport identity.
Telemetry uses the schema supplement: `sub_principal_id` and `act_principal_id`
are accounts; `workload_card_uid` is the acting/binding root; `subject_card_uid` is the
observed subject. Labels cannot forge any of these managed identities.

| Refusal | Public behavior |
| --- | --- |
| Invalid/revoked/expired key, wrong owner, inactive account | Existing non-enumerating key refusal and OAuth invalid-grant envelope. Never disclose secret or foreign account state. |
| Malformed UID or invalid parameter combination | Existing typed validation/invalid-request response, naming the safe field and expected grammar. |
| Explicit unknown, foreign, wrong-kind, inactive, or disallowed root | Existing `WYRD_AUTH_401_WORKLOAD_UID_REFUSED` family with revised optional-context remediation; unknown and foreign are indistinguishable. |
| Valid rootless restricted account tries graph-sensitive work | 403 authorization refusal with machine-readable reason `workload_context_required`; do not report the otherwise valid account as unauthenticated. |
| Missing operation grant or out-of-scope Card | Existing permission/domain scope refusal; OTLP retains its existing record-level rejection semantics where applicable. |
| Header attempts to change a signed root | Typed validation refusal with reason `signed_workload_mismatch`; no silent unpinning. |
| Family runtime account already associated | Proposed `WYRD_AUTH_409_WORKLOAD_BINDING_CONFLICT`; no partial account/binding change and no secret return. |
| Oversized graph/encoded token | Existing `WYRD_AUTH_413_CARD_SCOPE_TOO_LARGE`; never truncate or issue an unverifiable token. |
| Invalid claim shape, removed kind, bad audience/signature, inconsistent IDs/tenants | Existing invalid-token/audience/delegation catalog response; no fallback to rootless/unrestricted. |

Keep OAuth error envelopes at `/auth/token` and Wyrd problem/error projection
elsewhere. New detailed errors live in the existing typed catalog and project
identically across SDKs, HTTP, MCP, and CLI. Revise old remediation that demands
a UID for every key or suggests reissuing a deployment key.

## Fresh v0.3.0 baseline

No existing account, key, external mapping, token, or accepted queue requires
release migration. Development installations may recreate state and provision
fresh accounts, mappings, credentials, Cards, and sessions under the new
contract. No old Agent-kind token parser, principal-free-key conversion,
identity backfill, or dual-schema compatibility path is required.

Ordinary human/account auth behavior remains required within the new baseline:
a User uses normal login/access tokens, a Service key authenticates its stable
account, and optional workload context supplies exact-version scope. Future
key rotation, revocation, JWT lifetime, accepted-authority recovery, and live
schema/layout evolution retain their specified guarantees. Reset eligibility
is not permission to weaken durability after new work is accepted.

This packet does not execute a database, storage, credential, or worktree reset.
The final docsite task documents setup/reset under the shipped contract.

## Complete evidence obligations

The main acceptance IDs remain authoritative; the following makes their auth
coverage explicit without prescribing private tests or creating tasks:

- Principal/grant matrix: User OIDC/device/refresh/access token, TenantAdmin
  key, restricted/unrestricted Service key, external assertion, browser
  recovery, delegated User/Service, platform boundary, and internal System.
- Account administration: explicit ID assignment, no registration principals,
  role sources, restriction defaults/replacement, family conflict atomicity,
  credential overlap/list/revoke, and no accidental schedule reassignment.
- Claims: exact fields, every Card-access state, malformed/missing state,
  empty scoped intersection, canonical UID equality, removed kinds, tenant and
  subject mismatch, expiry/audience/depth, and encoded-token size.
- Execution: concurrent R1/R2 on one key, shared component, wrong component,
  privileged rootless User, false owner, request-context selection, immutable
  signed root, scoped native/OTLP equality, and provider SDK key-only calls.
- Delegation/recovery: both grant and Card intersections, preserved User/actor
  identities and roots, actor credential provenance, no actor-chain authority,
  renewal/rotation, accepted queue restart, scheduled per-root activity, and no
  System/role/root fallback.
- Fresh baseline: create accounts/keys, federation mappings, Cards, and
  accepted work without old-state conversion or compatibility routes; retain
  normal rootless auth and post-creation lifecycle guarantees.

Use real first-class SDK/server/Postgres journeys for public behavior and
focused in-process claim/scope negatives. Static agreement with this document
does not prove these runtime guarantees.

## Inspected source and authority

Repository facts and proposed shapes are distinguished above. Current source
supplies the reusable owners, not authority for the new contracts:

- [Token requests/responses](../../../../crates/wyrd-spec/src/auth/token.rs),
  [principal contracts](../../../../crates/wyrd-spec/src/auth/tenant_principals.rs),
  [federation contracts](../../../../crates/wyrd-spec/src/auth/admin.rs).
- [Principal administration](../../../../crates/wyrd/wyrd-server/src/components/principals/routes.rs),
  [issuer](../../../../crates/wyrd/wyrd-auth/src/issuance.rs),
  [key exchange/delegation](../../../../crates/wyrd/wyrd-auth/src/exchange_api_key.rs),
  [external assertion exchange](../../../../crates/wyrd/wyrd-auth/src/jwt_bearer.rs),
  [graph resolution](../../../../crates/wyrd/wyrd-auth/src/card_scope.rs).
- [Claims/verifier](../../../../crates/shared/wyrd-auth-verify/src/lib.rs),
  [runtime principal](../../../../crates/shared/wyrd-runtime/src/principal.rs),
  [accepted authority](../../../../crates/shared/wyrd-runtime/src/request_context.rs),
  [verification activity](../../../../crates/wyrd/wyrd-sql/src/queries/verification.rs).
- [Security authority](../../../../architecture/wyrd-security-posture.md) and
  [design authority](../../../../architecture/wyrd-design.md), with the explicit
  amendments enumerated in the parent specification.
- [RFC 8693](https://www.rfc-editor.org/rfc/rfc8693.html) supplies delegation
  subject/actor semantics, not Wyrd's arbitrary-UID authentication. The account
  reference and actor chain preserve that distinction.

## Revision history

| Spec revision | Status | Change |
| --- | --- | --- |
| 2 | draft | Full proposed auth system, exact public contracts, all grant/principal flows, Card limits, scope/provenance propagation, runtime scheduling, refusals, and cutover. |
| 3 | draft | Uses explicit stored `workload_card_uid`/`subject_card_uid` names and a concrete account/workload/component example; token and header context selectors remain unchanged. |
| 4 | draft | Connects accepted account/root attribution to the unified system/dynamic envelope and preserves audit decision identities during internal delivery. Auth grant and claim shapes are unchanged. |
| 5 | draft | Uses the fresh v0.3.0 baseline without auth migration or migration-only exact-root restrictions; preserves normal auth and frozen accepted authority, with the public bifrost catalog. |
| 6 | draft | Aligns packet revision with the configurable Bloom policy; auth and accepted-attribution contracts are unchanged. |
| 7 | draft | Aligns with evidence-read optimization under frozen accepted authority; auth semantics are unchanged. |
| 8 | draft | Aligns with the recorded Bloom decisions; auth and accepted-attribution semantics are unchanged. |
| 9 | draft | Aligns with whole-trace evidence under ordinary query authorization; binding/graph attribution does not imply trace row filtering. Auth claims and grants are unchanged. |
| 10 | draft | Aligns packet revision; any future evidence retry must retain accepted authority. Auth contracts are unchanged. |
| 11 | draft | Requires evidence retries to preserve the same accepted execution authority; auth grants and claim shapes are unchanged. |
| 12 | approved | Aligns the packet revision with closed table/trace contracts; accepted authority and all authentication contracts are unchanged. |
