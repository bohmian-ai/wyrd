---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-verification-closeout
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, AC-001, AC-002]
depends_on: []
provenance: TASK-017-R3
---

# R3: Principal roles and frictionless local operation

## Outcome and Value

Administrators assign Roles to principals rather than Cards, direct and IdP
assignments coexist safely, Wyrd ships four understandable built-in roles, and
a developer can operate Wyrd through an admin key or saved login without
issuing a Card-scoped key or using a test-only publication hook.

## Owners, Scope, Consumers, and Prohibited Changes

- Principal contracts belong in `wyrd-spec`; durable assignments and
  provenance belong in `wyrd-sql`; authorization, login synchronization, and
  issuance remain owned by `wyrd-auth` and the server.
- `wyrd_client::Principals` owns the shared client surface projected by the
  Rust, Python, and TypeScript SDKs. CLI commands call that surface rather than
  duplicating transport.
- Existing Bifrost, OTLP, and verification attribution owners apply the same
  unbound-principal rule while retaining Card-bound scope.
- Preserve tenant isolation, audit every allowed and denied administration
  decision, and keep already-issued token semantics unchanged.
- Do not add role aliases, direct permissions on principals, an MCP surface,
  compatibility routes, or a second authentication-refresh path.

## Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Principal administration | `Principals`, principals router, role-assignment queries | principal integration tests and SDK principal callers | Assignments target Cards; discovery and revoke are absent | Extend the existing principal resource and client handle | None |
| IdP role synchronization | login callback and role-assignment queries | saved-login and identity integration tests | Login replaces direct assignments | Track assignment source and replace only IdP-owned rows | Source is required to preserve administrator grants |
| Built-in roles | `BUILTIN_ROLES` and Card-principal projection | authorization and issuance tests | Overlapping roles and incomplete runtime permissions | Replace the built-in set with the four specified roles | None |
| Card attribution | Gate, OTLP authentication, verification observations | Bifrost/OTLP/verification journeys | Unbound users cannot tag registered Cards | Reuse registry resolution for unbound principals; retain Card scope for bound principals | None |
| Stock-client authentication | `WyrdClient::access_token` and installed stock-client hooks | gateway and OTLP journeys | Static access tokens expire | Adapt the stock client's per-request hook to the existing refresh path | Only ecosystem adapters required by the stock libraries |

## Approach

1. Replace Card-addressed role grants with principal discovery and idempotent
   principal assignment APIs, preserving source provenance and audit behavior.
2. Replace the built-in role set and update initial Card-principal projection,
   identity defaults, issuance, fixtures, and authorization expectations.
3. Project the principal surface through the shared client, CLI, and all three
   SDKs; remove the obsolete grant route and helpers.
4. Apply one unbound-principal attribution rule across Bifrost, OTLP, and
   verification observations, and remove publication-flush calls from user
   journeys.
5. Add the minimal stock-client auth adapters and prove the admin-key and
   saved-login local workflows in the existing journeys and guide.

## Required Contract Detail

The implementation must preserve the complete R3 contract rather than only its
headline outcome:

- Replace `GrantRoleRequest`/`GrantRoleResponse` with the principal-centered
  role/source, role-change, summary, and page wire types from the spec.
- Mount the four principal routes from the principal router. `PUT` and `DELETE`
  have no request body and return HTTP 200 with `changed` plus the complete
  resulting assignment set.
- `GET /v1/principals` accepts exact `kind`, `email`, and `name` filters,
  `limit` 1–200 (default 100), and a UUIDv7 `after` cursor.
- `IssueKeyResponse` exposes `principal_id`. The CLI provides
  `principal list` and `principal role list|grant|revoke`; it deletes
  `auth grant-role`.
- The Python, TypeScript, and Rust `Principals` handles provide credential
  listing/revocation plus principal discovery and role operations with the
  same typed results.
- `auth_user_roles` records `source` in its primary key. Every writer names the
  source; IdP replacement touches only `idp`; token issuance reads distinct
  effective Roles across sources. Service-account assignments remain direct.
- `BUILTIN_ROLES` remains the sole source for the exact role hierarchy and
  permission sets; seed, fixtures, docs, and tests consume it.
- Identity connections default omitted Roles to `viewer`, while explicit `[]`
  remains empty.
- Python OTLP exporters use HTTP/protobuf and per-request session auth;
  `GatewayAuth` implements sync and async `httpx.Auth`. TypeScript exporter
  headers and `gatewayFetch` resolve the token per request. Rust OTLP/HTTP uses
  the shared client's async token path behind the `otel` feature. Missing
  optional runtime packages name the extra to install.

The retained public stories are:

| Story | Required proof |
|---|---|
| `principal_roles` | default Service key has direct `workload`; it can run/query but cannot author until direct `editor` is granted; revoke affects the next token; only admin grants; direct and IdP user assignments coexist |
| `local_development` | setup admin key registers, hydrates, runs, observes, verifies, exports, and queries without issuing a key or flushing the server |
| `signed_in_development` | saved-login user completes the same workflow and stock clients continue after the original access token expires |

## Ordered Implementation Scenarios

### Scenario 1 — Administrators manage principal Roles safely

**Behavior.** An administrator discovers assignable principals, lists their
Role sources, and idempotently grants or revokes direct assignments. IdP login
updates only IdP assignments. Foreign, deleted, and non-assignable principals
remain non-enumerable.

**RED.** Extend the existing principal integration target to prove discovery,
direct/IdP coexistence, idempotency, authorization, audit, and tenant
isolation. The current Card-addressed surface and destructive login sync fail
those assertions.

**GREEN.** Extend the existing contracts, queries, router, authorization, and
login-sync owners; project the same behavior through `Principals` and the three
SDKs.

**REFACTOR.** Delete the obsolete Card-addressed grant contract, route, CLI,
and test helpers after all consumers use the principal surface.

### Scenario 2 — The four roles and attribution rules support real workloads

**Behavior.** The four built-in roles have the specified ordering and a new
Service or Agent principal receives `workload`. Unbound users can attribute
evidence to registered observation-target Cards; Card-bound principals cannot
escape their Card scope.

**RED.** Extend the existing authorization, Gate, OTLP, and verification
integration coverage with allowed unbound attribution and refused foreign,
unregistered, wrong-kind, and out-of-scope attribution. Existing roles and
scope handling fail the new expectations.

**GREEN.** Replace the built-in definitions and apply the shared attribution
decision at the existing ingest owners without adding a second policy layer.

**REFACTOR.** Remove retired built-in names and duplicated attribution checks;
retain one shared rule per owning tier.

### Scenario 3 — Public clients complete local and signed-in workflows

**Behavior.** Admin-key and saved-login users register, invoke, observe,
verify, export telemetry, and query through public clients. Stock gateway and
OTLP libraries refresh credentials per request, including after token expiry.
No journey calls the test-only Bifrost publication flush.

**RED.** Extend the existing local-development and saved-user journeys in each
SDK. They currently require extra credentials, static tokens, or the flush
hook.

**GREEN.** Add only the stock-library adapters and public SDK projections
needed by those journeys, then update the local-development guide to match the
proved workflow.

**REFACTOR.** Share the existing client token-refresh owner and checked-in Card
fixtures; do not introduce another credential cache or client abstraction.

## Acceptance Criteria

- AC-001 and AC-002 pass across the server, CLI, and all three SDKs.
- Direct user assignments survive login; revocation removes only the direct
  source; assignment responses are deterministic and idempotent.
- The four built-in roles replace the retired names without aliases.
- Attribution behavior is identical across Bifrost, OTLP, and verification.
- Local and signed-in journeys use public surfaces and no publication flush.

## Expected Write Set and Consumer Closure

- Contracts and generated surfaces: `crates/wyrd-spec`, generated schemas,
  OpenAPI, Python stubs, and TypeScript declarations.
- Server/auth/SQL: principals routes and service, role assignments, login sync,
  issuance, built-in roles, Card-principal projection, Gate, OTLP, and
  verification observation authorization.
- Clients: shared `Principals`, CLI principal commands, and Rust/Python/
  TypeScript SDK projections and auth adapters.
- Tests/docs: existing principal, identity, Gate, OTLP, verification, local
  development, and saved-login owners plus the local-development guide.

## Verification and Evidence

- `mise run test:principals:integration`
- `mise run test:bifrost:journey:otlp`
- `mise run test:gateway:journey`
- `mise run codegen:check`
- `mise run check:deps`
- `mise run check:tenant-isolation`
- `mise run fmt`
- `mise run lints`
- `mise run py:lints`
- `mise run py:typecheck`
- `mise run docs:check`

The implementation report records exact selectors for every named journey and
focused Rust test after their final names are confirmed with the native test
collector. Broad lanes above do not replace those focused Red/Green commands.

## Material Stop Conditions

- A required stock library exposes no supported per-request authentication
  hook.
- Principal provenance cannot be introduced without destructive migration of
  an existing assignment source.
- Unbound attribution cannot resolve a Card through the existing tenant
  registry boundary.

## Authority Links

- `../spec.md` revision 1: REQ-001..003, AC-001..002
- `AGENTS.md`; `architecture/agent-rules.md`;
  `architecture/wyrd-security-posture.md`; `architecture/wyrd-design.md`;
  `TESTING.md` (definitive Wyrd guide for test ergonomics,
  understandability, structure, ownership, and lane selection)
