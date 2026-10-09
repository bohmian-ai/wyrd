---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-verification-closeout
spec_revision: 2
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

- `../spec.md` revision 2: REQ-001..003, AC-001..002
- `AGENTS.md`; `architecture/agent-rules.md`;
  `architecture/wyrd-security-posture.md`; `architecture/wyrd-design.md`;
  `TESTING.md` (definitive Wyrd guide for test ergonomics,
  understandability, structure, ownership, and lane selection)

## Implementation Evidence

Commits: `035be3105`, `a6257f909`, `472617b9e`, `2cca3a65a`, `540b16663`,
`d4da18bee`, `b5ab2dd5d`, `8f65cb716`, `729174f44`, `24be8c7ff`, `a2e20233a`,
`0abd5b8fb`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Principal discovery and idempotent direct grant/revoke with source provenance, audit, and tenant isolation (Scenario 1, AC-001) | `crates/wyrd/wyrd-server/src/components/principals/routes.rs` (`GET /v1/principals`, `PUT`/`DELETE /v1/principals/{id}/roles/{role}`, audit `auth.principal.list`, `auth.principal.role.{list,grant,revoke}`); `wyrd-sql` `role_assignments.rs` (`source` in key); `wyrd_client::principals::Principals::{list,roles,grant_role,revoke_role}`; CLI `wyrd principal list` / `principal role list\|grant\|revoke`; `IssueKeyResponse.principal_id` | `mise run test:principals:integration` (all summaries pass); `mise run check:tenant-isolation`; `mise run codegen:check` | PASS |
| Direct user assignments survive login; revoke removes only `direct` (AC-001) | IdP login replaces only `idp` rows; token issuance reads distinct roles across sources | `principal_roles::direct_and_idp_user_assignments_coexist` in Rust (`cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E 'test(/^principal_roles::/)'`), Python `tests/integration/test_principal_roles.py::test_direct_and_idp_user_assignments_coexist`, TypeScript `tests/integration/principal-roles.test.ts` "direct and idp user assignments coexist" | PASS |
| Four built-in roles replace retired names without aliases; Card principals get `workload`; omitted trusted-issuer roles default to `viewer` | `crates/shared/wyrd-runtime/src/builtin_roles.rs` (`BUILTIN_ROLES`, `DEFAULT_CARD_ROLE`, `DEFAULT_ISSUER_ROLE`) | `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/^builtin_roles::tests::/)'` (`exactly_four_roles_are_built_in`, `roles_are_strictly_nested`); `principal_roles::granted_editor_reaches_the_next_token_until_revoked`, `only_a_tenant_admin_assigns_roles` in all three SDKs | PASS |
| One unbound-principal attribution rule across Bifrost, OTLP, and verification (Scenario 2) | `Principal::card_attribution()` → `CardAttribution::{Scoped,AnyRegistered}`; Gate `attribution::CardRegistry` resolves unbound `card_refs` (cap `MAX_ATTRIBUTED_CARDS = 32`); verification `service.rs` `resolve_direct` / `subject_in_scope` use the same rule | `mise run test:bifrost:journey:otlp` incl. `wyrd-testing::otlp negative::pg_tests::unbound_writer_attributes_spans_to_registered_cards_only` (registered → attributed; unregistered → rejected `wyrd.card_ref is outside the principal's Card scope`; none → unattributed); `mise run test:bifrost:journey:observe` | PASS |
| Stock-client adapters resolve the token per request (Rust OTLP/HTTP behind `otel`; Python OTLP/HTTP + `GatewayAuth`; TS exporters + `gatewayFetch`); missing optional packages name what to install | `sdks/wyrd-sdk-rust/src/otel.rs`; `sdks/wyrd-sdk-python/python/wyrd/otel.py` (`span_exporter`, `log_exporter`, `metric_exporter`, `otel` extra), `python/wyrd/gateway/_auth.py` (`gateway` extra); `sdks/wyrd-sdk-ts/wyrd/src/otel.ts` (`@wyrd/sdk/otel`, optional peers), `gatewayFetch` in `src/index.ts` | `uv run python -m pytest -q tests/unit/gateway/test_gateway_auth.py` (3 passed: httpx, httpx2, async); `pnpm exec vitest run tests/unit/gateway.test.ts` ("gatewayFetch sends a fresh access token on every request"); `mise run ts:pack:check` | PASS |
| `local_development`: setup admin key registers, hydrates, invokes, observes, verifies, exports, and queries with no key issued and no flush (AC-002) | `sdks/wyrd-sdk-rust/tests/integration/local_development.rs`, `sdks/wyrd-sdk-python/tests/integration/test_local_development.py`, `sdks/wyrd-sdk-ts/wyrd/tests/integration/local-development.test.ts` + `tests/support/local-development.ts`; harness `tenant_admin_key` / `tenantAdminKey` | Rust `-E 'test(=local_development::admin_key_completes_the_local_workflow)'` via `mise run test:bifrost:journey:observe`; Python `uv run python -m pytest -q -m integration tests/integration/test_local_development.py` via `mise run test:bifrost:journey:python` (68 passed); TS via `mise run test:bifrost:journey:typescript` (64 passed) | PASS |
| `signed_in_development`: saved-login user completes the workflow; stock clients keep working after the original token expires (AC-002) | `signed_in_development.rs`, `test_signed_in_development.py`, `signed-in-development.test.ts`; harness `access_ttl_seconds` / `accessTtlSeconds` (zero skew) | `WYRD_IDENTITY_TARGET=rust mise run test:identity:journey` (7 passed), `WYRD_IDENTITY_TARGET=python …` (7 passed), `WYRD_IDENTITY_TARGET=typescript …` (7 passed); each asserts the lapsed original token gets 401, then Gateway invoke and OTLP export still succeed | PASS |
| Local-development guide matches the proved workflow; docs describe principal roles | `docs/src/content/docs/self-hosting/local-development.svx` §6, `concepts/authorization.svx`, `reference/cli.svx`, `self-hosting/sso-and-oidc.svx`; `architecture/wyrd-security-posture.md`, `architecture/wyrd-design.md`; `fixtures/README.md` | `mise run docs:check` | PASS |
| Format, lints, boundaries, generated contracts | — | `mise run fmt`, `mise run lints`, `mise run py:lints`, `mise run py:format:check`, `mise run py:typecheck`, `mise run py:test:unit` (593 passed), `mise run ts:lints`/`ts:format:check`/`ts:typecheck`, `mise run ts:test:unit` (63 passed), `mise run codegen:check`, `mise run check:deps`, `mise run check:py-wheel-no-testing`, `mise run test:gateway:journey`, `git diff --check` | PASS |

### Decisions

- `GatewayAuth` subclasses both `httpx.Auth` and `httpx2.Auth`. The current
  OpenAI (3.x) and Anthropic Python SDKs build on `httpx2` and google-genai on
  `httpx`; one instance serves either. Its flow overrides keep the bases'
  signatures because each base types them with its own `Request`.
- Omitted `default_roles` default to `viewer` for trusted issuers; human OIDC
  connections keep roles only from `group_role_map`.
- Unbound attribution resolves at most 32 Cards per request
  (`MAX_ATTRIBUTED_CARDS`). Unbound Services share the rule with users and the
  tenant administrator.
- Only the new local and signed-in journeys are flush-free here. Removing
  `flush_bifrost` from the remaining journeys is REQ-009/010 (TASK-003).
- The Python harness's `cleanup` argument was removed. It was never
  implemented, no caller passed it, and adding `access_ttl_seconds` would
  otherwise push `__new__` over Clippy's argument limit.

### Diagnoses

- **Signed-in Rust journey 503 "gateway credential protection is unavailable".**
  The test gateway's managed-secret keyring covers only the fixture tenant
  (`test_gateway_config(fixture.data_tenant_id(), …)`), and alice was signed
  in to a seeded tenant. Fix: sign alice in to `FIXTURE_TENANT_SLUG`.
- **Signed-in Rust journey `ClientTransportDown` on Bifrost start.**
  `ClientConfig::from_environment` derives the default gRPC port. Fix: set
  `config.grpc.endpoint = server.grpc_url()`.
- **`test_workflow_parameter_injection.py::test_explicit_workflow_bindings`
  `KeyError: 'editor'`.** The role rename in `035be3105` also renamed a
  workflow step lookup (`steps["writer"]`). Fix: restore the step name.
- **TS signed-in journey "this credential already names its tenant".** The
  `@wyrd/testing` CLI shim forwards the client's access token as an explicit
  credential, and `WYRD_TENANT` beside it is correctly refused. The fresh
  config home holds one saved login, so the test sets no `WYRD_TENANT`, like
  the Rust journey.

### Non-goals and scope

No role aliases, direct permissions on principals, MCP principal surface,
compatibility routes, or second refresh path were added. Rust provides no
Gateway adapter. All changed files are within the expected write set, plus the
identity, Bifrost, and TS integration lane selectors in `mise.toml` and the
pack inventory in `scripts/check_ts_package.mjs`.

Result: **IMPLEMENTED**.
