---
id: TASK-001
kind: implementation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-017, INV-003, INV-004, INV-006, AC-001, AC-003, AC-007, AC-009]
depends_on: []
---

# Tenant OIDC connection administration

## Outcome and Value

An authorized tenant administrator can configure, test, activate, replace, deactivate, and remove one tenant-owned human login connection through a typed headless API. OIDC remains optional for startup, provisioning, and machine use.

## Owners, Scope, Consumers, and Prohibited Changes

The server owns trust, tenant-scoped persistence, secret protection, authorization, and audit. Likely owners: wyrd-auth, wyrd-server, wyrd-sql, and public contracts in wyrd-spec. The login service and UI consume this contract. Use TenantConn, canonical audit, and screened provider IO. Preserve the separate platform connection. Exclude hosted signup, a commercial stub, a second trust store, and per-replica tenant secret variables.

## Approach

1. Define the tenant connection contract and lifecycle with at most one active connection and a testable replacement candidate.
2. Expose authorized create, redacted inspect, test, activate, rotate, deactivate, and remove operations plus the exact deployment-controlled callback URL.
3. Encrypt stored provider secrets, support sealing-secret rotation, and close the audit and generated-contract surfaces.

## Packet-local connection contract

`wyrd-auth` owns one tenant-scoped human connection authority. Introduce a
human-only durable connection record; migrate existing `Human` issuer rows out
of `wyrd.auth_trusted_issuers`, then use that existing table only for workload
issuers/bindings. Never read both stores for human trust. Preserve workload
rows and their issuer foreign keys untouched. Migration promotes the sole
existing human issuer for a tenant to active; if a tenant has several, the
preflight fails with the tenant identifier and an operator resolution step
before migration. No arbitrary issuer wins. Historical connection IDs remain
as non-login tombstones while refresh/session references may exist. Preserve
existing subject/email/group claim mapping and bounded JWKS TTL. A nonempty
legacy `default_roles`, incompatible stored audience/client ID, or unsupported
client-auth value fails preflight with a redacted repair step; never silently
discard or carry an unsafe grant. Change the existing trusted-issuer admin
HTTP and CLI writers/readers so `Human` is refused with a typed
`HUMAN_CONNECTION_REQUIRED` error and directs operators to this connection
API; workload operations remain. Remove or redirect any old human boot seed
through this same durable contract. No public path may acknowledge a Human
issuer row that login ignores. If a legacy Human issuer is referenced by a
workload binding, copy its human configuration into the new connection owner
and retain the old row as Workload with the binding intact; otherwise remove
the migrated Human row. Update old SQL fixtures that insert a Human row only
to back a workload binding to use Workload after migration.

The public `wyrd-spec` shapes (names may follow existing naming conventions,
but fields and semantics are fixed) are:

```text
HumanConnection { id: UUID, tenant_id: TenantId, revision: u64,
  state: Candidate|Active|Inactive, issuer: URL, client_id: String,
  client_auth: SecretBasic|SecretPost|Public,
  claim_mapping: VerifiedClaimMapping,
  group_role_map: Map<Group, Vec<TenantRole>>, jwks_ttl_secs: u64,
  tested_revision: Option<u64>, tested_until: Option<Timestamp>,
  created_at, updated_at, callback_url: URL }
ConnectionInput { issuer, client_id, client_auth, client_secret?: Secret,
  claim_mapping, group_role_map, expected_revision?: u64 }
ConnectionActivate { expected_revision: u64, recovery_api_key: Secret }
ConnectionView = HumanConnection without secrets or encryption metadata
```

Tenant-scoped bearer authority, not a request tenant ID, selects the tenant for
`GET /v1/identity/oidc/connections`, `PUT /v1/identity/oidc/candidate`,
`POST /v1/identity/oidc/candidate/test`,
`POST /v1/identity/oidc/candidate/activate`,
`POST /v1/identity/oidc/active/deactivate`, and
`DELETE /v1/identity/oidc/connections/{id}`. GET returns the active and optional
candidate redacted views. PUT creates/replaces one candidate and increments its
revision. It is also the secret/role-map rotation path, including when issuer
is unchanged. Test validates discovery, client authentication, issuer, callback,
and screened network behavior and stamps only the exact tested revision with
`tested_until = tested_at + 15 minutes`. All six operations require a tenant principal with the new
`identity_connections:write` permission, granted to the built-in tenant
administrator role and never to platform or workload principals by default.
The existing `service_accounts:write` alone does not authorize human SSO
configuration. Authenticate and audit the allow/refusal before any provider
network IO; a refused caller can see neither connection metadata nor secrets.
Activation requires that stamp, unexpired `tested_until`, and
`expected_revision`. It also verifies the supplied existing headless
tenant-administrator recovery API key against the same tenant and
`identity_connections:write` authority before retiring the old provider.
The key is never stored in the connection record and is redacted from every
response/audit/log; the UI collects it only for this action. A stale revision,
expired/failed test, missing/invalid recovery key, or competing mutation returns a typed
`CONNECTION_CONFLICT`/`CONNECTION_NOT_TESTED` refusal. Unsupported
`PrivateKeyJwt` returns `UNSUPPORTED_CLIENT_AUTH`; it is never offered by UI or
schema. `Public` requires no secret; secret methods require a secret. The
callback URL comes from configured public origin and is read-only.
Activation after the 15-minute test window requires a fresh test and fails
closed during provider outage.

All mutations lock the tenant connection slot, authorize, validate, and append
the canonical redacted audit decision in the same tenant transaction as the
state change. Network test occurs before that transaction; activation rechecks
its tested revision and all invariants under the lock. Exactly one Active and
at most one Candidate are enforced by tenant-scoped database constraints;
activation retires the prior Active atomically. Deactivate/remove block new
login and renewal immediately; removal tombstones the record while references
exist. Audit append failure rolls the mutation back. A replica reads the
durable active record on login/renewal; any cache is invalidated by revision
and cannot extend the effective old-connection lifetime. Sealing-key rotation
uses versioned ciphertext/key IDs and a tested rewrap procedure: retain the
old key through rewrap and verification, switch the write key, then retire it
only when no ciphertext references it. Never log either key or plaintext.

## Ordered Implementation Scenarios

### Scenario 1 — Optional setup and tenant isolation

**Behavior.** An OIDC-off server starts, provisions tenants, and serves existing credential and machine paths. Tenant A may administer only A's connection; B's connection and secrets remain isolated. Unauthorized decisions are refused and audited. TASK-003 owns the OIDC-off UI journey.

**RED.** Add a real-server Postgres journey across two tenants; observe missing management behavior or improper cross-tenant access.

**GREEN.** Add the minimal tenant connection contract, durable owner, and authorized API; rerun the journey.

**REFACTOR.** Reuse existing tenant SQL and audit owners without a parallel configuration store.

### Scenario 2 — Safe candidate activation

**Behavior.** The administrator can test a candidate before activation. Activation is one tenant-scoped transition with one active provider. The shown callback matches the deployed public origin; unsafe discovery, bad issuer, or unsupported client authentication fails closed.

**RED.** Add a provider-backed server journey covering candidate test, activation, and rejected configuration; observe absent transition or unsafe acceptance.

**GREEN.** Add screened validation and atomic lifecycle behavior; rerun both scenarios.

**REFACTOR.** Share the existing screened OIDC capability and typed errors.

### Scenario 3 — Secret and replica safety

**Behavior.** Read responses, browser-visible data, errors, logs, audit, and generated artifacts contain no provider secret. Sealing-secret rotation preserves connections. Changes are visible across serving replicas without restart.

**RED.** Add real-store redaction/rotation and two-replica lifecycle journeys; observe leakage, lost decryptability, or stale state.

**GREEN.** Complete secret protection and shared durable visibility; rerun prior scenarios.

**REFACTOR.** Retain one tenant connection authority and one audit write path.

## Acceptance Criteria

REQ-001–005 and REQ-017 hold under two tenants, same issuer, failure, rotation, and replacement. No secret crosses a read or evidence boundary; no unsupported client-auth method is offered.

## Expected Write Set and Consumer Closure

Likely wyrd-auth, wyrd-server, wyrd-sql, wyrd-spec, migrations, server journeys, OpenAPI, generated schemas, and rotation instructions. Paths guide ownership, not a private file allowlist. TASK-003 projects the management API in the UI.

## Verification and Evidence

Each newly named test must run through its exact focused mise exec command with repository-managed setup. Broader lanes: mise run test:principals:integration; mise run test:identity:journey; mise run codegen:check; mise run check:tenant-isolation; mise run fmt; mise run lints. The integration lane must prove the served OpenAPI document. Record redacted secret-rotation and two-tenant evidence. Documentation-only portions need static review, not manufactured RED.

Planned `wyrd-server --test identity_e2e` ignored tests
`tenant_connection_admin_journey` and `tenant_connection_rotation_journey`
use the existing Postgres/Keycloak/Dex identity setup (`mise run
test:identity:journey`). Within that setup, run each selector via
`mise exec -- cargo nextest run --locked -p wyrd-server --test identity_e2e
--run-ignored=all -E 'test(=tenant_connection_admin_journey)'` and the same
command with `tenant_connection_rotation_journey`. The first test creates A/B,
checks same-issuer isolation, redacted GET, wrong-tenant and unauthorized
refusals plus audit, served OpenAPI, and old Human admin/CLI write refusal
without breaking Workload writes; it maps AC-003/009. The second tests
candidate revision, concurrent activation, unsafe discovery, secret rewrap,
two-replica visibility, failed-audit rollback, and migration preflight for
legacy claim maps/default roles in a separate pre-migration test; it maps
AC-007. Add the two
selectors to `test:identity:journey:inner` only after listing them with
`mise exec -- cargo nextest list --locked -p wyrd-server --test identity_e2e`;
keep the existing identity tests selected. The lane must fail if either new
selector selects zero tests. Use `mise run test:principals:integration` for the
served OpenAPI assertion and existing auth SQL coverage. Existing
`test:identity:journey` owns provider/Postgres setup; do not treat a bare
focused command outside that environment as journey proof.

Make `test:identity:journey:inner` accept optional
`WYRD_IDENTITY_TARGET=server` and `WYRD_IDENTITY_FILTER=<exact test name>`.
The outer `test:identity:journey` still owns Postgres, Keycloak, Dex, and
cleanup; inner selects only that named test when set and checks selection
count is one before execution. From a clean checkout the focused commands
are `mise exec -- env WYRD_IDENTITY_TARGET=server
WYRD_IDENTITY_FILTER=tenant_connection_admin_journey mise run
test:identity:journey` and the same with
`tenant_connection_rotation_journey`. With no filter the lane runs all
existing and new identity journeys. The inner `cargo nextest` command above
is the exact test selector executed by this setup wrapper.

Add `pg_tests::human_connection_upgrade_preflight` to existing
`wyrd-sql --test pg_migration`. For each case use a fresh isolated database
with migrations through the version immediately before the new
human-connection migration: (a) one valid Human row with custom claim
mapping/TTL and a workload binding, (b) nonempty legacy default roles, and
(c) two Human rows for one tenant. Apply the new migration to each database:
(a) must preserve mapping/TTL and binding while moving human trust; (b)/(c)
must fail with a redacted repair reason and leave the old database unchanged.
This cannot be proved by an identity
journey that runs after `db:migrate:inner`. Focus it from a clean checkout
with `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc
"mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_migration
-E 'test(=pg_tests::human_connection_upgrade_preflight)'"` after confirming
the selector with `mise exec -- cargo nextest list --locked -p wyrd-sql
--test pg_migration`. Keep the ordinary `mise run test:sql` migration lane
green as well.

## Material Stop Conditions

Stop for a second trust authority, different callback contract, weaker SSRF control, changed secret/audit guarantee, or commercial onboarding hook.

## Authority Links

[Approved spec](../spec.md); [AGENTS.md](../../../../AGENTS.md); [agent rules](../../../../architecture/agent-rules.md); [security posture](../../../../architecture/wyrd-security-posture.md).

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: OIDC optional per tenant; no IdP or OIDC env needed without a connection | `wyrd-server` boot/config treat public origin and sealing key as optional; `HumanConnections::active_trusted_issuer` returns none when no Active row | `mise run test:identity:journey` (22/22, all servers start connectionless), `mise run test:platform:journey` (38/38) | PASS |
| REQ-002: at most one Active per tenant; same issuer isolated across tenants | migration `20260925000000_auth_human_connections.sql` (RLS, one-Active/one-Candidate partial unique indexes); `wyrd-auth/src/connections.rs` slot lock | `tenant_connection_admin_journey` steps 2-3 (two tenants, same Keycloak issuer, B cannot reach A); `tenant_connection_rotation_journey` step 4 (concurrent activation, one winner); `mise run check:tenant-isolation` | PASS |
| REQ-003: authorized create, redacted inspect, test, replace/rotate, activate, deactivate, remove over headless API; no restart | `wyrd-server/src/components/admin/identity.rs` (six `/v1/identity/oidc/*` routes, `identity_connections:write`); login/callback read Active per request | admin journey steps 1, 4, 6 (service_accounts:write refused and audited, Human TOML/admin/CLI writers refused, Workload writes kept, deactivate stops login, delete tombstones); rotation journey step 6 (replica A serves B's rotation without restart) | PASS |
| REQ-004: exact callback from public origin; discovery/issuer validated; audience = client_id; `private_key_jwt` refused | `ServerAuth.public_origin` → `callback_url`; `test_candidate` screened discovery + client-auth probe; `wyrd-spec/src/auth/human_connection.rs` `ConnectionInput::from_json` | admin journey asserts `callback_url`; rotation journey step 1-2 (unsafe discovery fails closed, wrong secret fails probe); `cargo nextest run -p wyrd-spec --lib -E 'test(/^auth::human_connection::/)'` (4/4, `PrivateKeyJwt` refused with `WYRD_AUTH_400_UNSUPPORTED_CLIENT_AUTH`; unit-only because it is a pure input check with no cross-boundary state); `identity_connection_operations_publish_their_contract` (enum excludes `PrivateKeyJwt`) | PASS |
| REQ-005: secret sealed at rest, absent from reads/errors/audit/generated artifacts; sealing key rotatable | `wyrd-crypt` `SealingKeyring` (write + retained keys), `wyrd-auth/src/sealing.rs` `SealedSecretRewrap` on boot; docs `self-hosting/authentication.svx` rotation runbook | admin journey (no secret in GET, sealed ciphertext differs, audit text lacks secret); rotation journey step 5 (K1→K2 boot rewrap, K2-only keyring opens without rewrap); OpenAPI test asserts no `secret` property on `HumanConnectionView`; `mise run codegen:check` | PASS |
| REQ-017: redacted canonical audit for mutations; audit failure cannot establish a connection | handlers record decisions through `crate::audit` before IO; owner commits in the decision transaction | admin journey reads retained `identity.oidc.*` audit (allowed and denied); rotation journey step 3 (injected `vala.audit_staging` failure on activate → 503, candidate still Candidate) | PASS |
| Migration preflight (a)/(b)/(c) | migration preflight block with `Repair:` messages | `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_migration -E 'test(=pg_tests::human_connection_upgrade_preflight)'"`; `mise run test:sql` | PASS |
| Focused journey selection fails on zero matches | `mise.toml` `test:identity:journey:inner` `WYRD_IDENTITY_TARGET`/`WYRD_IDENTITY_FILTER` count check | `mise exec -- env WYRD_IDENTITY_TARGET=server WYRD_IDENTITY_FILTER=tenant_connection_admin_journey mise run test:identity:journey` and `..._rotation_journey` (exit 0) | PASS |
| Served OpenAPI contract | `openapi.rs` Identity tag, utoipa route registrations | `mise run test:principals:integration` (`pg_openapi_contract` 18/18) | PASS |

Verification commands (all exit 0): both focused journeys; `mise run test:identity:journey`;
`mise run test:principals:integration`; `mise run test:sql`; `mise run test:platform:journey`;
the focused `human_connection_upgrade_preflight`; `wyrd-auth --lib`, `wyrd-server --lib`
(auth/admin/config/boot), `wyrd-cli --lib` trusted_issuer, `wyrd-crypt`, `wyrd-spec`
human_connection under the test Postgres wrapper; `mise run codegen:check`;
`mise run check:tenant-isolation`; `mise run docs:check`; `mise run fmt`; `mise run lints`;
`git diff --check`.

Non-goals kept out: no UI (TASK-003), no login redirect-URI or renewal changes (TASK-002),
no `private_key_jwt`, no second trust store. Journeys run replicas in-process over one
shared Postgres rather than a deployed multi-pod topology.
