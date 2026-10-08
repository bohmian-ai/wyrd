---
id: TASK-017-R3
kind: remediation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 71
requirements: [REQ-196, REQ-210, REQ-212, REQ-213, REQ-215, REQ-216, AC-064, AC-065, AC-068, AC-069, AC-070]
depends_on: [TASK-017-R2]
parent_task: TASK-017
remediates: [FIND-ROLE-TARGET, FIND-ROLE-REVOKE, FIND-USER-ROLE-WIPE, FIND-PRINCIPAL-DISCOVERY, FIND-ROLE-SPRAWL, FIND-ADMIN-CARD-TAG, FIND-FLUSH-HOOK, FIND-LOCAL-FLOW, FIND-STOCK-CLIENT-AUTH, FIND-SIGNED-IN-RUN]
---

# TASK-017 R3: Role assignments on the principal, four persona Roles, and a frictionless local flow

## Authority and subject

- Proposed authority: spec revision 71. It is **not yet approved**, and this
  task must not start until the user approves it. The proposed spec text is in
  [Spec revision 71](#spec-revision-71-proposed-text) below.
- Original task: [TASK-017](../../tasks/TASK-017-sdk-test-standard-and-cleanup.md).
- Builds on the uncommitted TASK-017-R2 candidate
  ([R2](../TASK-017-r2/TASK-017-R2-one-client-one-surface.md)), which added
  `POST /v1/auth/grant-role` under revision 67 (REQ-212). This task replaces
  that route.
- Wyrd has not shipped. Migrations are edited in place, and no alias or
  compatibility route is kept.

## Why

R2 built role grants the wrong way round. It names the target by Card
(`{ card_ref, role }`), but a Role is assigned to a **principal**. The R2
response itself returns `principal_id`. The built-in Roles a grant names are
also overlapping, and they restrict what users can do by default. That is a
choice for each tenant's administrator, not for Wyrd. Out of the box, a
developer running locally must be able to register, run, verify, and query a
Service with no friction, whether they hold the setup admin key or signed in
through their company's IdP. Ten defects follow:

| Finding | Defect | Evidence |
|---|---|---|
| FIND-ROLE-TARGET | The grant addresses a Card, not the principal that holds the Role. It cannot reach a user or a Card-free automation principal. | `GrantRoleRequest { card_ref, role }` (`crates/wyrd-spec/src/auth/tenant_principals.rs:129`); handler at `crates/wyrd/wyrd-server/src/components/auth/routes.rs:674` |
| FIND-ROLE-REVOKE | No route revokes a Role, yet the security posture promises that "an administrator's revocation stands". | `architecture/wyrd-security-posture.md` (Principal and credential lifecycle); revoke exists only as `revoke_role_from_service_account` in SQL |
| FIND-USER-ROLE-WIPE | Every federated login replaces **all** of a user's Roles with the IdP set, so a Role an administrator grants a user directly is erased at the next sign-in. | `replace_user_roles` (`crates/wyrd/wyrd-sql/src/queries/auth/role_assignments.rs:36`), called from `crates/wyrd/wyrd-auth/src/callback.rs:417` ("nothing else in Wyrd grants a user a role") |
| FIND-PRINCIPAL-DISCOVERY | An administrator cannot find a user's `principal_id`, and `issue_key` does not return the Card principal's id. | No `GET /v1/principals`; `IssueKeyResponse` (`crates/wyrd-spec/src/auth/issue_key.rs:29`) has `key_id` but no `principal_id` |
| FIND-ROLE-SPRAWL | Seven built-in Roles overlap and leave gaps. `agent` is `writer` minus trigger writes, and its name collides with the `agent` principal kind and the `Agent` Card kind. `workload` and `wyrd_default` differ by one permission. `runtime_admin` holds one permission under a name that does not say what it manages. No built-in Role short of `admin` can invoke the gateway, read gateway or Operator configuration, define Bifrost tables, or query Bifrost as a human. `writer` cannot read audit, but `reader` can. A Service's default Role cannot download its own bundle, create its tables, or query. | `BUILTIN_ROLES` (`crates/shared/wyrd-runtime/src/builtin_roles.rs:21`) |
| FIND-ADMIN-CARD-TAG | An administrator's or user's key cannot run a Service. Any row tagged with a `card_ref` is refused when the principal carries no signed Card scope, so a person must issue a Service key before emitting evidence locally. | `validate_card_scope` (`crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:733`); the same rule on OTLP (`crates/wyrd/wyrd-server/src/http/otlp.rs:662`) and verification observations (`crates/wyrd/wyrd-server/src/verification/observations.rs`) |
| FIND-FLUSH-HOOK | SDK journeys call the test-server-only `flush_bifrost()` before querying. Queries already read each live Scribe's acknowledged memtable rows (`architecture/bifrost-design.md`, "Immutable planning and source authority"), so after a client `flush()` the rows should be queryable within a second or two. The hook either does nothing or hides a live-read defect, and a user who copies the journey cannot call it. | `WyrdTestServer::flush_bifrost` (`crates/wyrd/wyrd-testing/src/server.rs:1282`); calls in `sdks/*/tests/integration` |
| FIND-STOCK-CLIENT-AUTH | Stock OpenTelemetry exporters and stock OpenAI-compatible clients send only a static credential. The OTLP routes accept a static `x-wyrd-api-key`, but a signed-in user holds no API key, and every access token expires after 5 minutes. A signed-in developer therefore cannot export OTel telemetry at all. Every principal's stock gateway client fails 5 minutes after it read its token. | `require_otlp_authenticated` (`crates/wyrd/wyrd-server/src/http/middleware/authenticate.rs:58`); `openai.OpenAI(api_key=caller.access_token())` in `sdks/wyrd-sdk-python/tests/integration/test_gateway_inference.py:33` |
| FIND-SIGNED-IN-RUN | The enterprise local flow is untested end to end. A developer signs in with `wyrd auth login`, and every SDK resolves that saved login with no export (credential tier 5, `ClientConfig::resolve_credential`, `crates/shared/wyrd-client/src/config.rs:229`). Yet no journey runs a Service, records, verifies, or exports telemetry as that user. Before D18, every Card-tagged write from that user was refused. | `saved_user_auth` covers only Card reads, writes, and renewal |
| FIND-LOCAL-FLOW | No guide or journey shows the local flow end to end: start the server, authenticate, register a Service, and run it (state, telemetry, records, evals, drift). `docs/self-hosting/local-development` stops after registering one Prompt. | `docs/src/content/docs/self-hosting/local-development.svx` |

## Standard

The task follows the NIST RBAC reference model (ANSI/INCITS 359), which is also
the shape of Kubernetes RoleBindings and cloud IAM bindings:

```
principal ──(role assignment)──► role ──(permission assignment)──► Permission { resource, action, scope }
```

1. Permissions attach only to Roles. A principal never holds a Permission
   directly. Wyrd already does this (`wyrd.auth_roles.permissions`).
2. A principal holds Roles only through role assignments. An assignment is a
   sub-resource of the principal and is addressed by `principal_id`.
3. Role *definitions* and role *assignments* are managed separately. This task
   changes assignments only.
4. Writes are separated from reads, and the assigner cannot escalate itself.
5. Every assignment decision is audited.
6. When an external IdP manages some assignments, each assignment records its
   **source**. The IdP sync replaces only what the IdP owns, and the effective
   Roles are the union of all sources. Keycloak, Grafana, and AWS IAM Identity
   Center all keep provenance this way.

## Decisions

| # | Decision |
|---|---|
| D1 | A role assignment is addressed by `principal_id`, under the existing `/v1/principals/{principal_id}` resource. `POST /v1/auth/grant-role` and `GrantRoleRequest`/`GrantRoleResponse` are deleted. |
| D2 | Assignable principals are `user`, `service`, and `agent`, whether or not they are Card-bound. `tenant_admin` (it already holds `*`), `system`, platform principals, deleted principals, and another tenant's principals all answer the non-enumerating `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND`. A suspended principal is assignable; its Roles apply only once it is reinstated. |
| D3 | The server resolves the principal kind from the id. Callers send no `principal_kind`. Ids are UUIDv7 and unique across `wyrd.auth_users` and `wyrd.auth_service_accounts`. |
| D4 | Routes use idempotent REST verbs: `GET` lists, `PUT` grants, and `DELETE` revokes. `PUT` and `DELETE` always answer 200 with the resulting assignment set and a `changed` flag, so a retried call is safe and a caller can tell whether anything changed. |
| D5 | Writes (`PUT`, `DELETE`) require tenant administration (`*`). `service_accounts:write` is not enough, because its holder could grant itself `admin`. Reads (`GET .../roles`, `GET /v1/principals`) use the principal-administration surface's existing authority, `service_accounts:write`, as `list_credentials` does. A Kubernetes-style "grant only what you hold" escalation check is not added. |
| D6 | User assignments carry a `source`: `idp` or `direct`. The login sync replaces only `idp` rows. `direct` rows persist until an administrator revokes them. The effective Roles are the union. The same Role may be held from both sources at once. |
| D7 | `DELETE` removes only the `direct` assignment. A Role the user also holds through `idp` stays held. The response lists it with `source: "idp"` and `changed` reports whether a direct row was removed. Removing an IdP-held Role is done in the IdP or in the connection's group mapping. Each case returns 200 rather than a new error code. |
| D8 | Service and agent assignments have no `source` column: every row is `direct`. The automatic `workload` grant made when a Card principal is first projected (D15) is a direct assignment and can be revoked. Re-applying the Card does not re-grant it. |
| D9 | Discovery: `GET /v1/principals` lists the tenant's assignable principals with exact-match filters and keyset paging, and `IssueKeyResponse` gains `principal_id`. `CreateServicePrincipalResponse` already returns it. |
| D10 | A change takes effect at the principal's next token. Issuance and refresh already read assignments from the database (`crates/wyrd/wyrd-auth/src/issuance.rs:538`). Tokens already issued keep their snapshot for at most 5 minutes. |
| D11 | Surfaces: the `wyrd principal list` and `wyrd principal role {list,grant,revoke}` CLI commands, which every SDK's packaged `wyrd` executable gains with no binding change; and a production `Principals` handle with one identical surface in Rust, Python and TypeScript (REQ-210). `Principals` leaves the operator-only Rust exception: Python and TypeScript gain the handle's existing credential methods as well as the four new role and discovery methods. Journeys call the public handle, so a user can copy them. The Card-addressed test-CLI `grant_role` is deleted and gets no test-CLI replacement. No MCP tool is added. |
| D12 | The test server's user fixtures write user Roles as `idp`, because they stand in for an IdP login. A test-only helper must not be the only way to create a `direct` user Role; the public `PUT` route is. |
| D13 | The built-in Roles are exactly four, named for who holds them: `admin` (people who manage Wyrd for an organization), `editor` (developers, data scientists, AI engineers, and product managers), `workload` (principals that run Services and Agents), and `viewer` (stakeholders, auditors, and the IdP default). `viewer` ⊂ `workload` ⊂ `editor` ⊂ `admin`. `reader`, `writer`, `agent`, `runtime_admin`, and `wyrd_default` are deleted with no alias. Developers and product managers share `editor`: they do different work with the same permissions, and a narrower Role is the tenant's choice. See [Built-in roles](#built-in-roles). |
| D14 | Built-in Roles restrict only two things. Admin credential operations belong to `admin`. Changing Cards and artifacts belongs to people (`editor`), so that a running Service cannot rewrite what it is verified against. Everything else is open to every Role that can write. A tenant that wants less defines its own Roles later; Wyrd does not ship the restriction. |
| D15 | `workload` is the Role every Card-bound Service or Agent principal receives at first projection. It has every read, including its own bundle, and every runtime write: Bifrost tables, records, queries, evals, workflows, triggers, Operator invocation, and gateway invocation. |
| D16 | Principal administration is admin-only among the built-in Roles. The `service_accounts:write` permission and every existing gate on it stay unchanged, but no built-in Role other than `admin` (through `*`) holds it. This also closes the revision-67 note that `runtime_admin` could create a principal holding `admin`. Grant and revoke stay gated on `*` (D5), so a future custom Role holding `service_accounts:write` still cannot escalate. |
| D17 | An identity connection's `default_roles` defaults to `["viewer"]` when the field is omitted on create. An explicit `[]` still means no default Role. Existing connections keep their stored value. |
| D18 | Card tagging. A principal **not bound to a Card** (a user or a tenant administrator) may tag rows and verifications with any registered observation-target Card in its own tenant. A Card-bound principal keeps its Card scope (its Card plus the declared graph, as `architecture/wyrd-design.md` §18 defines it). That scope is evidence attribution, not a Role restriction: it stops a Service from recording evidence under another Card's identity. The ingest path resolves an unbound principal's `card_ref` against the tenant registry to stamp `card_uid`. A `card_ref` that is not registered answers the existing `WYRD_VALA_403_CARD_UNRESOLVED`. A registered Card whose kind is not an observation target answers the existing `WYRD_VALA_403_BIFROST_CARD_SCOPE`. The same rule applies to Bifrost ingest, OTLP, and verification observations. |
| D19 | SDK journeys never call `flush_bifrost()`. A journey queries right after the client's `flush()`, as a user would. The Python and TypeScript `flush_bifrost` bindings are deleted. The Rust `WyrdTestServer::flush_bifrost` stays only for engine tests whose subject is publication (`crates/wyrd/wyrd-testing/tests/bifrost/**`). If a journey fails without the hook, the failure is diagnosed with tracing as a live-read defect in Oracle or Scribe and fixed at its root. |
| D21 | Stock clients authenticate per request. Each SDK ships token-refreshing adapters that plug into the stock library's own per-request hook. They read `WyrdClient.access_token()` at send time, which returns the cached token and renews it 30 seconds before expiry through the client's one refreshing auth path. The adapters work for every credential tier: saved login, API key, workload token, and explicit credential. They wrap the stock exporter or client instead of replacing it. See [Stock-client authentication](#stock-client-authentication-d21). |
| D22 | OTel adapters use OTLP over HTTP/protobuf (`{server_url}/v1/traces`, `/v1/metrics`, `/v1/logs`) with the `x-wyrd-access-token: Bearer <token>` header. HTTP is used because every stock OTLP/HTTP exporter has a native per-request hook, while gRPC per-call credentials require TLS in Python and TypeScript. The server's OTLP routes and the static `x-wyrd-api-key` path for environment-configured exporters are unchanged. |
| D23 | Gateway adapters exist in Python (`httpx.Auth`) and TypeScript (`fetch`), the transports the stock OpenAI and Anthropic SDKs accept. Rust has no stock OpenAI client to adapt. A Rust caller reads `client.access_token().await` per request, so this is a REQ-210 rev 69 (a) ecosystem-adapter exception. |
| D24 | The enterprise local flow is proved as a signed-in user. The Keycloak fixture realm gains `carol` in a `wyrd-editors` group, and the test connections' `group_role_map` becomes `wyrd-admins → admin`, `wyrd-editors → editor`, and `wyrd-viewers → viewer`. The `signed_in_development` story runs a Service as `carol`'s saved login with no credential configured, and proves that the adapters keep working after her access token expires. |
| D20 | Local development needs one credential: the tenant admin key that `wyrd-server setup` prints. With it, a developer registers, hydrates, runs, verifies, and queries a Service. `wyrd auth issue-key` is a deployment step, not a local one. `docs/self-hosting/local-development` becomes the end-to-end guide, and the `local_development` story proves the guide in every SDK. |

## API contract

### Routes

| Method and path | Purpose | Gate | Audit action | Target string |
|---|---|---|---|---|
| `GET /v1/principals?kind=&email=&name=&limit=&after=` | List assignable principals | `service_accounts:write` | `auth.principal.list` | `principals` |
| `GET /v1/principals/{principal_id}/roles` | List a principal's Roles and their sources | `service_accounts:write` | `auth.principal.role.list` | `principal:{id}` |
| `PUT /v1/principals/{principal_id}/roles/{role}` | Grant a `direct` assignment (idempotent) | `*` | `auth.principal.role.grant` | `principal:{id}/role:{role}` |
| `DELETE /v1/principals/{principal_id}/roles/{role}` | Revoke the `direct` assignment (idempotent) | `*` | `auth.principal.role.revoke` | `principal:{id}/role:{role}` |

All four mount on `principals_router()`
(`crates/wyrd/wyrd-server/src/components/principals/routes.rs:54`), carry the
`Principals` OpenAPI tag, and stage their decision on the audit outbox through
the surface's existing authorize-and-stage helper before opening the tenant
transaction, both when allowed and when denied. `PUT` and `DELETE` have no
request body.

`GET /v1/principals` query parameters:

| Parameter | Meaning |
|---|---|
| `kind` | `user`, `service`, or `agent`; omitted means all three |
| `email` | Exact email match; matches users only |
| `name` | Exact principal-name match; matches service and agent principals only |
| `limit` | 1–200, default 100 |
| `after` | Keyset cursor: the last `principal_id` of the previous page. Results are ordered by `principal_id`, which is UUIDv7 and therefore creation-ordered. |

### Wire types (`crates/wyrd-spec/src/auth/tenant_principals.rs`)

They replace `GrantRoleRequest` and `GrantRoleResponse`. All derive `Serialize`,
`Deserialize`, and `JsonSchema`, plus `ToSchema` under `server`, and use
`deny_unknown_fields`.

```rust
/// Where a principal's role assignment came from.
pub enum RoleSource { Idp, Direct }            // serde: "idp" | "direct"

/// One Role a principal holds, and where the assignment came from.
pub struct RoleAssignment { pub role: String, pub source: RoleSource }

/// `GET /v1/principals/{id}/roles`.
pub struct PrincipalRoles {
    pub principal_id: PrincipalId,
    pub kind: PrincipalKindTag,               // user | service | agent
    pub roles: Vec<RoleAssignment>,           // ordered by (role, source)
}

/// `PUT` and `DELETE /v1/principals/{id}/roles/{role}`.
pub struct RoleAssignmentChange {
    pub principal_id: PrincipalId,
    pub kind: PrincipalKindTag,
    pub role: String,
    pub changed: bool,                         // a direct row was added / removed
    pub roles: Vec<RoleAssignment>,            // the full set after the change
}

/// One row of `GET /v1/principals`.
pub struct PrincipalSummary {
    pub principal_id: PrincipalId,
    pub kind: PrincipalKindTag,
    pub status: PrincipalStatus,               // active | suspended
    pub email: Option<String>,                 // users
    pub name: Option<String>,                  // service and agent
    pub card_ref: Option<CardRef>,             // Card-bound service and agent
}

pub struct PrincipalPage { pub principals: Vec<PrincipalSummary>, pub next: Option<PrincipalId> }
```

`IssueKeyResponse` (`crates/wyrd-spec/src/auth/issue_key.rs:29`) gains
`pub principal_id: PrincipalId`. If no `PrincipalStatus` wire enum exists yet,
add one with `active` and `suspended` only, because deleted principals are not
listed.

### Errors

These are the only codes the routes return. All exist today.

| Condition | Status | Code |
|---|---|---|
| Path id is not a UUID; `role` fails `RoleRef` (`[a-z0-9_]`); the Role does not exist in the tenant; `kind`, `limit`, or `after` is invalid; `email` given with `kind` set to service or agent | 400 | `WYRD_SPEC_400_VALIDATION` |
| No usable token | 401 | existing `WYRD_AUTH_401_*` |
| Caller lacks the route's gate | 403 | `WYRD_PERMISSION_403_DENIED_RBAC` |
| Unknown, foreign-tenant, deleted, `tenant_admin`, or `system` principal | 404 | `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND` |
| Tenant store failure | 500 | `WYRD_SPEC_500_INTERNAL` (cause logged, never returned) |
| Verifier unavailable | 503 | `WYRD_AUTH_503_VERIFY_UNAVAILABLE` |

### CLI

`wyrd principal` gains a `role` subcommand group and a `list` command
(`crates/wyrd/wyrd-cli/src/principal/`):

```text
wyrd principal list [--kind user|service|agent] [--email E] [--name N] [--limit N] [--after ID]
wyrd principal role list   <principal_id>
wyrd principal role grant  <principal_id> <role>
wyrd principal role revoke <principal_id> <role>
```

`wyrd auth grant-role` and `crates/wyrd/wyrd-cli/src/auth/grant_role.rs` are
deleted. `wyrd auth issue-key` prints the new `principal_id`.

The command handlers call the shared `wyrd_client::Principals` methods below;
they add no HTTP code of their own. `wyrd_cli::commands` gains no new
in-process functions, because the SDK journeys use the public handle.

Every SDK ships the same `wyrd` executable: Rust through `wyrd-cli`, Python
through `wyrd.cli`, and TypeScript through `run_wyrd_cli`
(`sdks/wyrd-sdk-ts/native/src/cli.rs`). The new commands therefore reach all
three with no binding change, and `test:cli:journey` proves them once.

### SDK surfaces

`Principals` is one production handle, with the same methods and result
types in every SDK. It follows the `OperatorConnections` projection pattern:
`sdks/wyrd-sdk-python/src/operators.rs` and
`sdks/wyrd-sdk-ts/native/src/operators.rs`.

| Operation | Rust (`wyrd_sdk::Principals`) | Python (`wyrd.principals.Principals`) | TypeScript (`@wyrd/sdk` `Principals`) | Returns |
|---|---|---|---|---|
| construct | `Principals::with_client(client)`, `Principals::from_env()` | `Principals(client=None)` | `new Principals({ client })` | handle |
| find principals (new) | `list(&PrincipalQuery)` | `list(kind=None, email=None, name=None, limit=None, after=None)` | `list({ kind, email, name, limit, after })` | `PrincipalPage` |
| list roles (new) | `roles(principal_id)` | `roles(principal_id)` | `roles(principalId)` | `PrincipalRoles` |
| grant role (new) | `grant_role(principal_id, role)` | `grant_role(principal_id, role)` | `grantRole(principalId, role)` | `RoleAssignmentChange` |
| revoke role (new) | `revoke_role(principal_id, role)` | `revoke_role(principal_id, role)` | `revokeRole(principalId, role)` | `RoleAssignmentChange` |
| create service principal | `create_service_principal(..)` | `create_service_principal(..)` | `createServicePrincipal(..)` | `CreateServicePrincipalResponse` |
| issue credential | `issue_credential(..)` | `issue_credential(..)` | `issueCredential(..)` | `IssuedCredential` |
| list credentials | `list_credentials(..)` | `list_credentials(..)` | `listCredentials(..)` | `CredentialListResponse` |
| revoke principal | `revoke_principal(..)` | `revoke_principal(..)` | `revokePrincipal(..)` | none |
| revoke credential | `revoke_credential(..)` | `revoke_credential(..)` | `revokeCredential(..)` | none |

- **Arguments.** The existing methods keep their current Rust arguments, and
  Python and TypeScript mirror them field for field. `principal_id` is the
  `PrincipalId` newtype in Rust and a UUID string in Python and TypeScript.
  `role` is a Role name string. `PrincipalQuery` holds the five optional
  filters.
- **Results.** Results are typed (Pydantic models in Python, declared
  interfaces in TypeScript) and generated from the `wyrd-spec` wire types.
  Nothing is returned as a raw dict.
- **Principal revocation.** `RevokePrincipalRequest` drops `principal_kind`
  and keeps `reason`, because the server resolves the kind from the id (D3).
  `wyrd principal revoke` drops `--kind` in the same change. Python and
  TypeScript take `revoke_principal(principal_id, reason)` /
  `revokePrincipal(principalId, { reason })`.
- **Python and TypeScript bindings.** Add
  `sdks/wyrd-sdk-python/src/principals.rs`, the
  `sdks/wyrd-sdk-python/python/wyrd/principals/` package (exports, generated
  stubs), and `sdks/wyrd-sdk-ts/native/src/principals.rs` plus its
  `@wyrd/sdk` export and regenerated declarations. The bindings only convert
  arguments and call `wyrd_client::Principals`.
- **Test CLI.** Delete `grant_role` from `wyrd_sdk::cli`, from
  `wyrd.testing.cli` (`sdks/wyrd-sdk-python/src/testing_cli.rs:410`) and from
  the `@wyrd/testing` `cli` (`sdks/wyrd-sdk-ts/native-testing/src/cli.rs`).
  `issue_key` / `issueKey` results expose `principal_id` / `principalId`.

## Built-in roles

`BUILTIN_ROLES` (`crates/shared/wyrd-runtime/src/builtin_roles.rs`) is the
only source of truth. The seed and every check read it.

| Role | Holder | Permissions |
|---|---|---|
| `viewer` | Stakeholders, auditors, the IdP default | **All reads:** `cards:read`, `artifacts:read`, `audit:read`, `operators:read`, `gateway:read`, `gateway_payload:read`, `bifrost_table:read`, `bifrost_query:read` |
| `workload` | Service and Agent principals, granted at first projection | all of `viewer`, plus **runtime writes:** `bifrost_table:write`, `bifrost_record:write`, `evals:run`, `workflows:run`, `triggers:write`, `operators:invoke`, and `gateway:invoke` on every model (scope `all`) |
| `editor` | Developers, data scientists, AI engineers, product managers | all of `workload`, plus **authoring:** `cards:write`, `cards:delete`, `artifacts:write`, `policy:lock`, `services:install` |
| `admin` | People who manage Wyrd for the organization | `*` |

Only `admin` holds the admin credential operations:
- principals, credentials, `issue_key`, trusted issuers, and workload bindings
  (`service_accounts:write`);
- Role grants and revokes (`*`);
- users (`users:write`) and identity connections
  (`identity_connections:write`);
- Operator connection secrets (`operators:write`) and gateway provider
  credentials and configuration (`gateway:write`, `gateway:delete`).

Platform-plane permissions (`tenants:*`, `platform_*`) and engine-internal
permissions (`bifrost_peer:invoke`, `bifrost_table:install`) belong to no
tenant Role.

| Old Role | Replacement |
|---|---|
| `admin` | `admin` |
| `writer`, `agent` | `editor` |
| `reader` | `viewer` |
| `runtime_admin` | `admin` |
| `wyrd_default`, `workload` | `workload` |

**Edits:**
- **Role list.** `BUILTIN_ROLES` gets the four Roles above, and
  `DEFAULT_CARD_ROLE` becomes `"workload"`.
- **Backfill migration.** Delete
  `crates/wyrd/wyrd-sql/migrations/20261002000100_workload_role.sql`. It only
  backfilled tenants that already existed, and none exist because Wyrd has not
  shipped. New tenants get their Roles from the server seed
  (`crates/wyrd/wyrd-auth/src/seed.rs`). Delete the seed test that checked
  that backfill.
- **Tests that pin the Roles.** Rewrite them in `builtin_roles.rs`:
  - each Role's exact permission set;
  - `viewer` ⊂ `workload` ⊂ `editor` ⊂ `admin`;
  - only `admin` covers `service_accounts:write`,
    `identity_connections:write`, `users:write`, `operators:write`,
    `gateway:write`, and `gateway:delete`;
  - `workload` lacks `cards:write` and `artifacts:write`.
- **Old Role names.** Every test, fixture, CLI example, doc, and test-server
  credential fixture that names an old Role moves to its replacement, using
  the table above as the only mapping. That covers:
  - `wyrd-testing` fixtures;
  - `wyrd-auth-issue` and `wyrd-auth-verify`;
  - `wyrd-auth` (`issuance.rs`, `refresh.rs`, `roles.rs`, `seed.rs`);
  - CLI tests and `trusted_issuer.rs`;
  - MCP tests;
  - `wyrd-server` integration tests;
  - the SDK journeys.

  A test whose point was that a Role is refused something keeps that point
  with the new Role that is still refused it. For example, `writer_is_refused`
  on Operator connections becomes `editor_is_refused`, and
  `reader_cannot_register_cards` becomes `viewer_cannot_register_cards`.
- **`default_roles` (D17).** Edit `CreateIdentityConnectionRequest`
  (`crates/wyrd-spec/src/auth/admin.rs:92`) and the create path that stores
  it.

## Card tagging (D18)

- **Token.** Today `resolve_card_scope`
  (`crates/wyrd/wyrd-server/src/auth/card_scope.rs`) gives a user or tenant
  administrator an empty scope, which ingest reads as "may tag nothing". It
  becomes an explicit unbound marker in the signed token, meaning "may tag
  any registered Card in the tenant". A Card-bound token keeps its bounded,
  UID-signed scope unchanged.
- **Ingest.** `validate_card_scope`
  (`crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:733`), the
  OTLP correlation path (`tables/traces/projection.rs`, `http/otlp.rs`), and
  `verification/observations.rs` admit an unbound principal's `card_ref` once
  it resolves against the tenant registry to an observation-target Card. The
  resolved UID is stamped as `card_uid`, exactly as for a bound principal.
- **Architecture.** Update `architecture/wyrd-design.md` §18 and the
  "`card_ref` is optional and authorized, not trusted" bullet, plus
  `architecture/wyrd-security-posture.md`, to state the unbound rule.

## Stock-client authentication (D21)

Every adapter computes the same header at send time:

```text
token = client.access_token()            # cached; renewed 30s before expiry
OTLP:    x-wyrd-access-token: Bearer {token}
Gateway: Authorization: Bearer {token}   # the gateway's documented bearer
```

`client` defaults to the ambient `WyrdClient()`, so a signed-in developer
passes nothing. A refusal (for example, a revoked login) surfaces as the
stock library's normal export or HTTP error, carrying the server's catalog
code. Adapters never retry, cache tokens of their own, or log a token.

### Python

| Surface | Signature | Behavior |
|---|---|---|
| `wyrd.otel.span_exporter` | `span_exporter(client: WyrdClient \| None = None) -> OTLPSpanExporter` | Returns `opentelemetry.exporter.otlp.proto.http.trace_exporter.OTLPSpanExporter(endpoint=f"{client.server_url}/v1/traces", session=session)`, where `session` is a `requests.Session` whose `auth` sets the header on every request. |
| `wyrd.otel.metric_exporter` | `metric_exporter(client=None) -> OTLPMetricExporter` | The same, at `/v1/metrics`. |
| `wyrd.otel.log_exporter` | `log_exporter(client=None) -> OTLPLogExporter` | The same, at `/v1/logs`. |
| `wyrd.gateway.GatewayAuth` | `GatewayAuth(client: WyrdClient \| None = None)`, an `httpx.Auth` | `sync_auth_flow` sets `Authorization` from `client.access_token()`. `async_auth_flow` reads the token through `asyncio.to_thread` so the event loop never blocks. |

Usage:

```python
from openai import OpenAI
import httpx
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from wyrd.client import WyrdClient
from wyrd.gateway import GatewayAuth
from wyrd.otel import span_exporter

traces = TracerProvider()
traces.add_span_processor(BatchSpanProcessor(span_exporter()))

client = WyrdClient()
llm = OpenAI(base_url=f"{client.server_url}/v1", api_key="wyrd",
             http_client=httpx.Client(auth=GatewayAuth(client)))
```

`GatewayAuth` overrides the `Authorization` header that the OpenAI SDK derives
from `api_key`, so `api_key` only has to be non-empty.

Packaging:
- The `otel` extra becomes `opentelemetry-api`, `opentelemetry-sdk`, and
  `opentelemetry-exporter-otlp-proto-http` (each `>=1.27`).
- A new `gateway` extra adds `httpx>=0.27`.
- `wyrd.otel` and `wyrd.gateway` import these lazily. A missing extra raises
  `ImportError` naming the extra to install.
- `install_run_correlation` and the `Run` span correlation in `wyrd/otel.py`
  are unchanged.

### TypeScript (`@wyrd/sdk`)

| Surface | Signature | Behavior |
|---|---|---|
| `spanExporter` | `spanExporter(options?: { client?: WyrdClient }): OTLPTraceExporter` | Returns `@opentelemetry/exporter-trace-otlp-proto`'s `OTLPTraceExporter({ url: \`${client.serverUrl}/v1/traces\`, headers: async () => ({ "x-wyrd-access-token": \`Bearer ${await client.accessToken()}\` }) })`. Async `headers` is the exporter's native per-request hook (`@opentelemetry/otlp-exporter-base` 0.222 `HeadersFactory`). |
| `metricExporter`, `logExporter` | the same options | The same, with `exporter-metrics-otlp-proto` at `/v1/metrics` and `exporter-logs-otlp-proto` at `/v1/logs`. |
| `gatewayFetch` | `gatewayFetch(options?: { client?: WyrdClient }): typeof fetch` | A `fetch` that sets `authorization: Bearer ${await client.accessToken()}` on each request, then calls the global `fetch`. Used as `new OpenAI({ baseURL: \`${client.serverUrl}/v1\`, apiKey: "wyrd", fetch: gatewayFetch({ client }) })`. |

Packaging:
- Exported from a new `@wyrd/sdk/otel` subpath (`sdks/wyrd-sdk-ts/wyrd/src/otel.ts`)
  and from the package root (`gatewayFetch`).
- The three `*-otlp-proto` exporters are optional `peerDependencies`. The
  grpc exporters stay dev dependencies for the existing `otel_export` story.
- `index.d.ts` is regenerated.

### Rust (`wyrd_sdk`)

| Surface | Signature | Behavior |
|---|---|---|
| `wyrd_sdk::otel::span_exporter` | `fn span_exporter(client: &WyrdClient) -> Result<opentelemetry_otlp::SpanExporter, WyrdError>` | Builds `SpanExporter::builder().with_http().with_protocol(Protocol::HttpBinary).with_endpoint(format!("{}/v1/traces", client.server_url())).with_http_client(WyrdOtlpHttp::new(client.clone())).build()`. |
| `metric_exporter`, `log_exporter` | the same | The same at `/v1/metrics` and `/v1/logs`. |
| `WyrdOtlpHttp` (private) | implements `opentelemetry_http::HttpClient` | `send_bytes` awaits `client.access_token()`, inserts the header, and sends through `reqwest`. It is the only per-request hook the OTLP/HTTP exporter offers, and it is async, so no background refresh task is needed. |

Packaging:
- The factories live in `crates/shared/wyrd-client/src/otel.rs` behind a new
  optional `otel` feature. That feature enables the optional
  `opentelemetry_sdk`, `opentelemetry-otlp` (`http-proto`, default features
  off), and `opentelemetry-http` dependencies. `wyrd-client` already depends
  on `opentelemetry`.
- `wyrd-sdk-rust` gets an `otel` feature that turns on `wyrd-client/otel` and
  re-exports the module as `wyrd_sdk::otel`. The Python and TypeScript
  bindings never turn it on, because they use their own runtime's
  OpenTelemetry.
- The factories live in `wyrd-client`, not the SDK, because TASK-017-R4's
  `WyrdState::start_telemetry` is an inherent method on a `wyrd-client` type
  and builds on them. The feature keeps the dependency cost off every caller
  that does not ask for it (AGENTS.md §2).
- `WyrdOtlpHttp` sends through the workspace `reqwest`, so the graph gains no
  second `reqwest` major from `opentelemetry-otlp`'s defaults.
- No Rust gateway adapter (D23).

## Data model (edit in place)

`crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql`, `wyrd.auth_user_roles`:

```sql
CREATE TABLE wyrd.auth_user_roles (
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    user_id         UUID NOT NULL,
    role_id         UUID NOT NULL,
    source          TEXT NOT NULL CHECK (source IN ('idp','direct')),
    granted_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, user_id, role_id, source),
    -- foreign keys unchanged
);
```

There is no default for `source`; every writer names it.
`wyrd.auth_service_account_roles` is unchanged (D8).

Queries in `crates/wyrd/wyrd-sql/src/queries/auth/role_assignments.rs`:

- `replace_user_roles` becomes `replace_idp_user_roles`. It deletes and inserts
  only rows `WHERE source = 'idp'`, and its conflict target becomes
  `(data_tenant_id, user_id, role_id, source)`.
- `grant_role_to_user` and `revoke_role_from_user` take a `source`. The public
  routes pass `direct`; the test-server fixtures pass `idp` (D12).
- `list_user_roles` returns the distinct role names across both sources, which
  feeds token issuance unchanged. A new `list_user_role_assignments` returns
  `(role, source)` pairs for the API.
- New `assignable_principal_by_id(conn, id) -> Option<(PrincipalKindTag, ...)>`
  reads both tables in one statement and excludes `tenant_admin` and
  `deleted` rows (D2, D3).
- New `list_principals(conn, filter, limit, after)`.

Also update every `ON CONFLICT (data_tenant_id, user_id, role_id)` and every
direct `auth_user_roles` read that the change touches. Known sites:

- `crates/wyrd/wyrd-testing/src/server.rs` (fixture `grant_role` calls,
  `PrincipalTable::User`);
- `crates/wyrd/wyrd-sql/tests/integration/pg_admin_principals.rs:736`;
- `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:728`;
- the `identity_e2e.rs` counts at `:3379`, `:3513` and `:4221`.

## Login sync

`crates/wyrd/wyrd-auth/src/callback.rs:417` calls `replace_idp_user_roles`.
The comment "nothing else in Wyrd grants a user a role" becomes a statement
that the IdP owns only the `idp` rows. The family lock and the
roles-sync audit stay as they are. `roles_changed` reflects only `idp`
changes.

## Write set

- **Spec contract:**
  - `crates/wyrd-spec/src/auth/{tenant_principals.rs,issue_key.rs,mod.rs}`;
  - generated schemas (`mise run codegen`).
- **SQL:**
  - `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql`;
  - `crates/wyrd/wyrd-sql/src/queries/auth/{role_assignments.rs,mod.rs}`;
  - the principal-lookup query module.
- **Auth:** `crates/wyrd/wyrd-auth/src/callback.rs`; the issue-key path that
  builds `IssueKeyResponse`.
- **Server:**
  - new handlers under `crates/wyrd/wyrd-server/src/components/principals/`;
  - remove `grant_role`, `tenant_auth_router` and its tests from
    `components/auth/routes.rs`;
  - remove the merge at `http/router.rs:77` if the router becomes empty;
  - `crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:153`.
- **CLI:** `crates/wyrd/wyrd-cli/src/principal/{mod.rs,role.rs,list.rs}`;
  delete `auth/grant_role.rs`; `auth/mod.rs`, `lib.rs`, `auth/issue_key.rs`.
- **Rust client and SDK:**
  - `crates/shared/wyrd-client/src/principals/handle.rs`;
  - `crates/wyrd-spec/src/auth/admin.rs` (`RevokePrincipalRequest`) and the
    revoke handler under `crates/wyrd/wyrd-server/src/components/principals/`;
  - `crates/wyrd/wyrd-cli/src/principal/revoke.rs` (drop `--kind`);
  - `sdks/wyrd-sdk-rust/src/lib.rs` (`cli` re-exports).
- **Python SDK:**
  - new `sdks/wyrd-sdk-python/src/principals.rs`, registered in `src/lib.rs`;
  - new `sdks/wyrd-sdk-python/python/wyrd/principals/` package, plus the
    `wyrd` top-level export and regenerated stubs;
  - `sdks/wyrd-sdk-python/src/testing_cli.rs` (delete `grant_role`; expose
    `principal_id` on the `issue_key` result) and its stubs.
- **TypeScript SDK:**
  - new `sdks/wyrd-sdk-ts/native/src/principals.rs`, registered in
    `native/src/lib.rs`;
  - the `@wyrd/sdk` export in `sdks/wyrd-sdk-ts/wyrd/src` and the
    regenerated declarations;
  - `sdks/wyrd-sdk-ts/native-testing/src/cli.rs` (delete `grantRole`; expose
    `principalId` on the `issueKey` result) and
    `sdks/wyrd-sdk-ts/testing/{index.js,index.d.ts,native.cjs,native.d.cts}`.
- **Journeys:**
  - the three `grant_role` story files are renamed to the `principal_roles`
    story (see below);
  - `sdks/wyrd-sdk-rust/tests/integration/main.rs`;
  - `fixtures/README.md`.
- **Test infrastructure:** `crates/wyrd/wyrd-testing/src/server.rs` (fixture
  sources).
- **Docs:**
  - `architecture/wyrd-security-posture.md`;
  - `docs/src/content/docs/concepts/authorization.svx`;
  - `docs/src/content/docs/reference/cli.svx`;
  - `TESTING.md`.
- **Built-in Roles:**
  - `crates/shared/wyrd-runtime/src/builtin_roles.rs`;
  - delete `crates/wyrd/wyrd-sql/migrations/20261002000100_workload_role.sql`;
  - `crates/wyrd/wyrd-auth/src/seed.rs`;
  - `crates/wyrd-spec/src/auth/admin.rs` and the identity-connection create
    path (`default_roles`);
  - every test, fixture, and doc that names an old Role (see
    [Built-in roles](#built-in-roles)).
- **Card tagging (D18):**
  - `crates/wyrd/wyrd-server/src/auth/card_scope.rs` and the token claim it
    feeds (`wyrd-auth-issue`, `wyrd-auth-verify`, `wyrd-runtime` principal);
  - `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs` and the
    OTLP correlation path;
  - `crates/wyrd/wyrd-server/src/verification/observations.rs`;
  - `architecture/wyrd-design.md`.
- **Flush hook (D19):**
  - every `flush_bifrost` / `flushBifrost` call under
    `sdks/*/tests/integration`;
  - delete the bindings in `sdks/wyrd-sdk-python/src/testing.rs` and its
    stub, and in `sdks/wyrd-sdk-ts/native-testing/src/lib.rs` and its
    declarations.
- **Stock-client authentication (D21–D23):**
  - Python: `sdks/wyrd-sdk-python/python/wyrd/otel.py` (exporter
    factories), `python/wyrd/gateway/__init__.py` (`GatewayAuth`), their
    stubs, and `pyproject.toml` extras;
  - TypeScript: new `sdks/wyrd-sdk-ts/wyrd/src/otel.ts`, `src/index.ts`
    (`gatewayFetch`), `package.json` (`exports`, optional
    `peerDependencies`), and regenerated declarations;
  - Rust: new `crates/shared/wyrd-client/src/otel.rs` and its `Cargo.toml`
    (`otel` feature); `sdks/wyrd-sdk-rust/src/lib.rs` and `Cargo.toml`
    (`otel` feature re-export); the workspace `Cargo.toml`
    (`opentelemetry-otlp` `http-proto` feature, `opentelemetry-http`);
  - the existing gateway journeys replace `api_key=caller.access_token()`
    with the adapters.
- **Signed-in flow (D24):**
  - `tests/fixtures/identity/keycloak-realm.json` and the Keycloak seeding
    script in `mise.toml` (`carol` in `wyrd-editors`);
  - `crates/wyrd/wyrd-testing/src/human_login.rs` and
    `crates/wyrd/wyrd-server/tests/integration/identity_ui_e2e.rs`
    (`group_role_map`);
  - the `signed_in_development` story files in all three SDKs.
- **Local flow (D20):**
  - `docs/src/content/docs/self-hosting/local-development.svx` (end-to-end
    guide);
  - the `local_development` story files in all three SDKs;
  - `fixtures/README.md`.
- **Packet:**
  - `spec.md` (revision 71);
  - `review/TASK-017-r2/parity.md`: delete the test-CLI grant row; add a
    `Principals` table; remove `Principals` from the "Operator-only Rust
    handles" section.

## Journeys

**Story `principal_roles`.** It replaces `grant_role` and keeps the same test
names in Rust, Python and TypeScript. Every step after `issue_key` calls the
public `Principals` handle, built from the tenant-admin client, so the act
portion can be copied into a notebook or a script. Its `fixtures/README.md` row registers
`observe_a_run/observed-model.yaml` and `observe_a_run/observed-service.yaml`.

| Test | Asserts |
|---|---|
| `default_service_key_runs_without_any_grant` | Using only the key `issue_key` returned, a client hydrates the Service bundle, registers a new `vala.datasets` table, writes and flushes a row, and reads it back with `bifrost.sql` immediately. |
| `service_key_needs_editor_to_change_cards` | The same key's `register_from_path` is refused with `WYRD_PERMISSION_403_DENIED_RBAC`. After `principals.grant_role(issued.principal_id, "editor")`, a fresh client from that key registers the Card. |
| `revoked_role_no_longer_reaches_the_next_token` | Grant `editor`, then `principals.revoke_role`. `changed` is true and `editor` is absent from `roles`. A fresh client from the same key is refused registration again. |
| `default_role_is_listed_as_a_direct_assignment` | `principals.roles(issued.principal_id)` is exactly `[{workload, direct}]`. |
| `only_a_tenant_admin_can_grant_a_role` | A `Principals` handle built from the Service's own key calling `grant_role` is refused `WYRD_PERMISSION_403_DENIED_RBAC`. |
| `user_found_by_email_holds_a_direct_role_beside_idp_roles` | `principals.list(kind="user", email=<saved-login fixture user>)` returns one user. `principals.grant_role(user, "editor")` lists `editor` as `direct` beside the fixture's `idp` Roles. |
| `revoking_an_idp_role_directly_changes_nothing` | `principals.revoke_role(user, <an idp role>)` returns `changed: false`, and the Role is still listed with `source: "idp"`. |

**Story `local_development`** (new, in all three SDKs). It follows
`docs/self-hosting/local-development` step for step. Every call uses the
deployment's tenant admin key and public SDK surfaces only. No test calls
`issue_key` or `flush_bifrost`. Its `fixtures/README.md` row registers
`observe_a_run/observed-model.yaml` and `observe_a_run/observed-service.yaml`.

| Test | Asserts |
|---|---|
| `admin_key_runs_a_registered_service` | Register and hydrate the Service with the admin client, then `WyrdState.from_path(bundle)`, `start_bifrost()`, and `run.for_card("model").observe.drift(...)`. After `flush()`, `bifrost.sql` immediately returns the rows, stamped with the Model's `card_uid` and the run's `run_id`. |
| `admin_key_verifies_a_components_output` | The same state's `run.for_card("agent").observe.verify(...)` returns a passing `Judgment`. |
| `admin_key_records_to_a_new_table` | `Bifrost(TableConfig(Row, "vala.datasets.local_rows")).register()` returns `"created"`. After `record(...)` and `flush()`, the row reads back immediately. |
| `admin_key_cannot_tag_an_unregistered_card` | Drift tagged with an unregistered `card_ref` is refused with `WYRD_VALA_403_CARD_UNRESOLVED` at `flush()`. |

**Story `signed_in_development`** (new, in all three SDKs, in the identity
lane with `saved_user_auth`). Each test starts from a configuration home
holding only one saved login, made by `save_human_login` through the
Keycloak device flow, with every `WYRD_*` credential variable unset. Every
client is `WyrdClient()` with no credential, which resolves the saved login.
Its `fixtures/README.md` row registers `observe_a_run/observed-model.yaml`
and `observe_a_run/observed-service.yaml`, as `carol`.

| Test | Asserts |
|---|---|
| `signed_in_editor_runs_a_registered_service` | As `carol`: register and hydrate the Service, `WyrdState.from_path(bundle)`, `start_bifrost()`, and drift for `model`. After `flush()`, `bifrost.sql` immediately returns the rows with the Model's `card_uid`, and `principal_id` is `carol`'s. |
| `signed_in_editor_verifies_a_components_output` | As `carol`: `run.for_card("agent").observe.verify(...)` returns a passing `Judgment`. |
| `signed_in_editor_exports_spans_with_the_wyrd_exporter` | As `carol`: a `TracerProvider` with `BatchSpanProcessor(span_exporter())` exports a span inside `with state.run(...)`. After `force_flush()`, `vala.traces.spans` returns it with the Run's `card_uid` and `run_id`. |
| `exporter_keeps_exporting_after_the_login_expires` | As `carol`: export one span, then `expire_saved_login`, then export a second span. Both read back, and the saved login was renewed (`saved_login_is_stale` is false). |
| `gateway_auth_keeps_calling_after_the_login_expires` | As `carol`: a stock OpenAI client with `GatewayAuth` / `gatewayFetch` calls the `mock` provider, then `expire_saved_login`, then calls again. Both answers arrive. Rust runs the same test with `reqwest` and `client.access_token()` per request (D23). |
| `signed_in_viewer_cannot_record` | As `bob` (`viewer`): `record(...)` then `flush()` is refused with `WYRD_PERMISSION_403_DENIED_RBAC`. |

**Admin-key stock clients.** The existing `gateway_inference` story replaces
`api_key=caller.access_token()` with the adapters. The `otel_export` story
gains `wyrd_exporter_span_reads_back_with_its_run` (the admin client and
`span_exporter()` inside `with state.run(...)`; the span reads back with
`card_uid` and `run_id`). `stock_exporter_span_reads_back_through_bifrost`
keeps proving the environment-only `x-wyrd-api-key` path.

**CLI journey** (`test:cli:journey`): a new journey runs `wyrd principal
list --kind service`, then `wyrd principal role grant <id> editor`,
`role list <id>` and `role revoke <id> editor`. It checks the printed
results and exit codes, and that a non-administrator profile's grant exits
non-zero with `WYRD_PERMISSION_403_DENIED_RBAC`. `wyrd principal revoke`
without `--kind` suspends a principal.

**Built-in Roles and tagging:**
- the `builtin_roles.rs` unit tests listed under
  [Built-in roles](#built-in-roles);
- a `seed.rs` `pg_test` proving a new tenant has exactly `viewer`,
  `workload`, `editor`, and `admin`;
- an identity-connection route test proving that an omitted `default_roles`
  stores `["viewer"]` and an explicit `[]` stores `[]`;
- a gateway invocation `pg_test` proving that a `workload` key may invoke a
  model and a `viewer` key is refused;
- Scribe and verification-observation tests proving that:
  - an unbound principal may tag a registered Card;
  - an unbound principal is refused for an unregistered Card
    (`WYRD_VALA_403_CARD_UNRESOLVED`) and for a control-plane Card
    (`WYRD_VALA_403_BIFROST_CARD_SCOPE`);
  - a bound principal is still refused outside its scope.
- The existing `card_scoped_key_cannot_write_another_cards_observations`
  journeys keep passing unchanged.

**Server route tests** (`pg_tests`, in the new principals role module):

- grant idempotency (`changed` true, then false);
- revoke idempotency;
- a non-administrator is refused on writes, and a reader without
  `service_accounts:write` is refused on reads;
- an unknown Role gives 400;
- `tenant_admin`, `system`, a deleted principal, and another tenant's principal
  each give 404;
- a suspended principal is assignable;
- a staged audit decision for each action, allowed and denied;
- `GET /v1/principals` filters and keyset paging;
- `issue_key` returns `principal_id`.

**Login sync** (`wyrd-auth` callback `pg_tests` and the
`test:identity:journey` IdP flow in
`crates/wyrd/wyrd-server/tests/integration/identity_e2e.rs`):

- a `direct` Role survives a second login that maps different groups;
- an `idp` Role the second login no longer maps is removed;
- the same Role held from both sources survives the loss of its `idp` row.

The existing `concurrent_callbacks_replace_roles_without_union` test keeps
passing, scoped to `idp` rows.

## Spec revision 71 (proposed text)

- **REQ-212**, replacing the whole requirement:
  - **Role assignments.** A Role is assigned to a principal, never to a Card.
    A tenant administrator (`*`) grants and revokes a user, service, or agent
    principal's Roles with `PUT` and `DELETE
    /v1/principals/{principal_id}/roles/{role}`.
  - Holders of `service_accounts:write` list them with `GET
    /v1/principals/{principal_id}/roles` and find principals with `GET
    /v1/principals`. `issue_key` returns the Card principal's `principal_id`.
  - Both writes are idempotent and return the resulting assignments with a
    `changed` flag.
  - A user assignment records its source, `idp` or `direct`. Login replaces
    only `idp` assignments, and a user's Roles are the union of both sources.
    `DELETE` removes only a direct assignment.
  - An unknown, foreign, deleted, `tenant_admin`, or `system` principal answers
    `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND`, and an unknown Role answers
    `WYRD_SPEC_400_VALIDATION`.
  - Each decision is staged on the audit outbox as `auth.principal.list`,
    `auth.principal.role.list`, `auth.principal.role.grant`, or
    `auth.principal.role.revoke`.
  - An assignment takes effect at the principal's next token.
  - The CLI surface is `wyrd principal list` and `wyrd principal role
    {list,grant,revoke}`. The SDK surface is the `Principals` handle, which
    is identical in Rust, Python and TypeScript.
- **REQ-210**, exception list: replace "the operator-only Rust `Principals`,
  `Platform`, and storage handles" with "the operator-only Rust `Platform`
  and storage handles". `Principals` is a shared surface from revision 71.
- **AC-064**, revised:
  - In each SDK, the `principal_roles` journey proves that a Service's
    default key runs, creates a table, and queries with no grant.
  - It grants `editor` to the Service principal by `principal_id`, registers a
    Card after a fresh key exchange, revokes the Role, and is refused again.
  - It lists `workload` as a direct assignment.
  - Granting is refused with `WYRD_PERMISSION_403_DENIED_RBAC` for a caller
    that is not a tenant administrator.
  - Rust route tests prove tenant isolation, non-enumerating not-found,
    idempotency, the read and write gates, and the staged audit decisions.
- **AC-068**, new:
  - A user's direct Role survives a later login.
  - An IdP Role that a later login no longer maps is removed.
  - Revoking an IdP-held Role directly changes nothing.
  - Proved in each SDK's `principal_roles` journey, in the `wyrd-auth`
    callback tests, and in the IdP identity journey.
- **REQ-213**, replacing the whole requirement:
  - **Built-in Roles.** Every tenant has exactly four built-in Roles, named
    for who holds them:
    - `viewer` (stakeholders, auditors): every read;
    - `workload` (Service and Agent principals): every read plus runtime
      writes, meaning Bifrost tables and records, evals, workflows, triggers,
      and Operator and gateway invocation;
    - `editor` (developers, data scientists, AI engineers, product managers):
      `workload` plus changing Cards, artifacts, and policy;
    - `admin`: `*`.
  - `viewer` ⊂ `workload` ⊂ `editor` ⊂ `admin`.
  - Only admin credential operations are reserved to `admin`: principals,
    credentials, Role grants, users, identity connections, Operator
    connection secrets, and gateway credentials and configuration.
  - A Card-bound Service or Agent principal receives `workload` at its first
    projection.
  - An identity connection's omitted `default_roles` is `["viewer"]`.
  - The Roles `reader`, `writer`, `agent`, `runtime_admin`, and
    `wyrd_default` no longer exist.
  - Narrower Roles are a tenant administrator's choice. Wyrd does not ship
    them.
- **REQ-215**, new:
  - **Frictionless local development.** The tenant admin key from
    `wyrd-server setup` alone registers, hydrates, runs, verifies, records
    to, and queries a Service.
  - A principal not bound to a Card may tag rows and verifications with any
    registered observation-target Card in its tenant. A Card-bound
    principal keeps its declared Card scope. An unregistered `card_ref`
    answers `WYRD_VALA_403_CARD_UNRESOLVED`, and a control-plane kind answers
    `WYRD_VALA_403_BIFROST_CARD_SCOPE`.
  - Rows a client has flushed are queryable without any server-side hook.
- **REQ-216**, new:
  - **Stock clients authenticate per request.** Each SDK ships OTel span,
    metric, and log exporter factories that wrap the stock OTLP/HTTP
    exporter and set `x-wyrd-access-token` from `WyrdClient.access_token()`
    on every export:
    - Python: `wyrd.otel.span_exporter`, `metric_exporter`, `log_exporter`;
    - TypeScript: `spanExporter`, `metricExporter`, `logExporter` from
      `@wyrd/sdk/otel`;
    - Rust: `wyrd_sdk::otel::*` behind the `otel` feature.
  - Python (`wyrd.gateway.GatewayAuth`, an `httpx.Auth`) and TypeScript
    (`gatewayFetch`) ship gateway adapters that set `Authorization` the same
    way.
  - Rust has no gateway adapter; that is a REQ-210 rev 69 (a)
    ecosystem-adapter exception.
  - The adapters work with every credential tier, including a saved login,
    and keep working across token expiry.
- **REQ-210**, exception (a): add the Python and TypeScript gateway adapters
  (`GatewayAuth`, `gatewayFetch`).
- **AC-070**, new:
  - In each SDK, the `signed_in_development` journey runs, verifies, and
    exports telemetry for a Service as a saved IdP login with no credential
    configured.
  - It proves that the OTel exporter and the gateway adapter keep working
    after the login's access token expires.
  - It proves that a `viewer` login is refused a record write.
- **AC-069**, new:
  - In each SDK, the `local_development` journey follows
    `docs/self-hosting/local-development` using only the admin key and
    public surfaces.
  - No SDK journey calls `flush_bifrost`.
- **Revision 65 note:** the credential-fixture Roles become `viewer`,
  `editor`, and another tenant's `admin`.
- **Security posture and authorization docs:** replace the `POST
  /v1/auth/grant-role` sentence with the routes above, and document role
  provenance for users. Replace the built-in Role tables with the four Roles
  above. `architecture/wyrd-design.md` states the unbound Card-tagging rule
  (D18).
- **Revision history entry:** "Revision 71: role assignments live on the
  principal" (FIND-ROLE-TARGET, FIND-ROLE-REVOKE, FIND-USER-ROLE-WIPE,
  FIND-PRINCIPAL-DISCOVERY, FIND-ROLE-SPRAWL, FIND-ADMIN-CARD-TAG,
  FIND-FLUSH-HOOK, FIND-LOCAL-FLOW, FIND-STOCK-CLIENT-AUTH, FIND-SIGNED-IN-RUN).

## Constraints and non-goals

- No alias for `POST /v1/auth/grant-role`, `wyrd auth grant-role`, or the
  Card-addressed test CLI functions.
- No custom Role CRUD (tenant-defined Role definitions); no per-service Role;
  no MCP role tools; no Python or TypeScript `Platform` or storage handle; no
  escalation-subset check; no new error code.
- No `source` column on `wyrd.auth_service_account_roles`.
- No Rust gateway adapter, no gRPC OTel adapter, and no change to the server's
  OTLP routes or to the static `x-wyrd-api-key` path.
- No per-model gateway Role, no separate identity-operator or product-manager
  Role, no restriction beyond D14 in any built-in Role, and no alias for a
  removed Role name.
- Do not keep a journey green by calling `flush_bifrost`. A failure without
  the hook is a live-read defect to diagnose and fix (D19).
- Do not weaken, skip, or ignore any test; do not hand-edit generated stubs,
  declarations, or schemas.
- Rustdoc every touched Rust item, with `# Errors` (AGENTS.md §16).

## Verification

Run every named test above by exact selector. Then run:

- `mise run codegen:check`, `mise run check:deps` and
  `mise run check:tenant-isolation`;
- `mise run test:principals:integration` (the OpenAPI document no longer has
  `/v1/auth/grant-role` and has the four new routes);
- `mise run test:cli:journey`;
- `mise run test:identity:journey`, plus each SDK's identity lane for
  `saved_user_auth` and `signed_in_development`;
- `mise run verify:rust-sdk`, `mise run verify:python-sdk` and
  `mise run verify:typescript-sdk`, one at a time;
- `mise run fmt`, `mise run lints`, `mise run py:lints`,
  `mise run py:typecheck` and `mise run docs:check`;
- `mise run test:sql` (the seed and
  migration set);
- `mise run test:bifrost` (Scribe tagging and live reads);
- `git grep -n "flush_bifrost\|flushBifrost" sdks/`, which must find
  nothing;
- a `git grep` for `runtime_admin`, `wyrd_default`, and the Role strings
  `"reader"`, `"writer"`, and `"agent"` used as Role names, which must find
  nothing outside `changes/`;
- `git grep -n "grant-role\|GrantRoleRequest\|card_ref.*role"` over code and
  docs, which must find nothing outside `changes/`;
- a check of the `Principals` methods against the SDK surfaces table: every
  operation exists in all three SDKs, with the same result type;
- `git diff --check`, then the tracked and untracked diff audit.

## Implementation evidence

| Finding / AC | Implementation | Verification | Result |
|---|---|---|---|
| FIND-ROLE-TARGET (AC-064) | | | |
| FIND-ROLE-REVOKE (AC-064) | | | |
| FIND-USER-ROLE-WIPE (AC-068) | | | |
| FIND-PRINCIPAL-DISCOVERY (REQ-212) | | | |
| `Principals` parity in all three SDKs (REQ-210) | | | |
| CLI `principal list` / `principal role` | | | |
| FIND-ROLE-SPRAWL (REQ-213) | | | |
| FIND-ADMIN-CARD-TAG (REQ-215) | | | |
| FIND-FLUSH-HOOK (REQ-215, AC-069) | | | |
| FIND-LOCAL-FLOW (AC-069) | | | |
| FIND-STOCK-CLIENT-AUTH (REQ-216, AC-070) | | | |
| FIND-SIGNED-IN-RUN (AC-070) | | | |
