---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-guided-setup
spec_revision: 5
requirements: [REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020, REQ-029, REQ-033, REQ-034, REQ-035, REQ-036, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-009, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011, AC-016, AC-017, AC-018, AC-019]
depends_on: []
parent_task:
remediates: []
---

# Exact-version deployment identity without per-release secret replacement

## Outcome and Value

CI registers each release and publishes its exact Card UID. Deployed processes
reuse one Service/Agent deployment key and supply that UID, so two versions can
run concurrently with distinct principals and evidence attribution. Rotation
replaces a secret deliberately; routine application releases do not.

Registration stays explicit. Deployed code pulls the registered graph by exact
UID and loads the resulting bundle through existing APIs, or loads a bundle
CI included in the image. It never calls declare to discover whether a write
will happen. Existing version-specific keys require one-time replacement;
there is no key migration.

## What Changes and What Must Stay True

Own deployment-key wire contracts, server credential lifecycle, persistence and
release compatibility, token and direct OTLP authentication, UID-aware client
configuration/cache/transport identity, state startup validation, declaration
UID output, all SDK projections and deployment example sources. Preserve
per-version principal roles and emit scopes. A key never grants roles, chooses
latest or crosses tenants. Unknown foreign selectors reveal no identity data.

Preserve unbound user/admin/CI keys and human login behavior. Revocation blocks
new issuance and direct-key OTLP immediately; existing tenant bearers retain
ordinary expiry. Preserve authentication/audit boundaries, retained metadata,
secret hashing and one-time secret disclosure. Changes need real migration
and rollout proof even though old keys themselves are not migrated.

TASK-001 runs independently on the existing state lifecycle and owns local
bootstrap, the setup MCP prompt, ASGI adapter and docsite learning/navigation.
Keep shared-client/state native and binding validation here; do not change the
ASGI integration or CLI bootstrap to implement identity. Shared files are
coordinated by section, not by serializing whole tasks. Existing registration,
pull and state-loading methods are the integration seam, with generic startup
errors propagated by the adapter.

Own the platform deployment guide and its executable example sources; coordinate
its navigation/imports with TASK-001. This task can prove identity using
WyrdTestServer and CLI declaration without depending on the init rename or ASGI
adapter. Combined doc publication, renamed-command examples and ASGI startup
refusals are verified once after both changes integrate. Do not replace that
proof with mocks or label pending integrated checks as passing.

No collector delegation, trusted-issuer redesign, new registry traversal,
replacement auth subsystem, extra state loader, or silent key broadening.
Kubernetes/OBI examples do not become portable core dependencies.

## Approach

1. Extend the existing credential owner with the approved Service/Agent
   selectors, metadata and independently revocable deployment keys.
2. Resolve the required workload UID during exchange and direct OTLP auth
   through the same tenant-bound authorization and token-issuance owners.
3. Bind shared-client resolution, refresh and cache/transport reuse to that UID.
4. Preserve explicit registration and use existing exact-UID pull/bundle loading
   for deployment, validating the root before online state writes.
5. Close CLI/MCP and Rust/Python/TypeScript projections and focused journeys.
6. Verify rolling release, rotation, one-time old-key replacement and the real
   Kubernetes example; document matching authority and release behavior.

## Ordered Implementation Scenarios

### Scenario 1 — Deployment-key issuance and independent management

**Behavior.** Administrators issue keys for Service/Agent kind, space and name, list/inspect non-secret metadata and revoke a key by ID through the approved HTTP, MCP, CLI and SDK projections. Two keys coexist; revoking one leaves the other usable. Absent/foreign IDs are indistinguishable. Card-bound principal issuance is refused; unbound principal credential management remains unchanged.

**RED.** Add planned CLI journey `auth_issue_key_journey::deployment_key_management` (D1) driving server operations and corresponding MCP/SDK reads. Expect missing version-independent contracts or management behavior.

**GREEN.** Extend the current issuance and principal credential owner with REQ-035 contracts and unchanged secret storage, permissions and audited commit boundaries. Rerun D1.

**REFACTOR.** Reuse tenant transactions, metadata queries and revoke behavior; remove any accidental aggregate principal or second credential lifecycle.

### Scenario 2 — Exact version selection and safe refusals

**Behavior.** Key plus UID issues that exact active version principal and current grants. Missing, malformed, unknown, inactive, other-Service/Agent and foreign-tenant selectors fail without fallback or disclosure. Suspended tenant/principal, expired/revoked key and non-deployment credentials with a UID fail under existing public error rules.

**RED.** Add planned Python journey `test_deployment_key_selection` (D2), using real auth/HTTP responses and registered versions. Expected RED is absent selection/refusal behavior, not bad harness credentials.

**GREEN.** Extend ExchangeApiKey and TenantTokenIssuer, preserving the existing OAuth envelopes and mapped errors. Keep per-version role revocation effective at next issuance. Rerun D1/D2.

**REFACTOR.** Centralize selection in the existing auth owner instead of duplicating checks in individual SDKs or OTLP routes.

### Scenario 3 — Credential precedence and token-cache isolation

**Behavior.** Validation follows the selected credential. An unused saved login does not defeat an explicit key. Different UIDs sharing one key/endpoint never share a subject through memory/disk cache or pooled transport; refresh preserves the UID.

**RED.** Add planned client unit test `config::tests::workload_uid_uses_selected_credential` (D3) and real-server Python `test_deployment_token_cache_isolation` (D4). Expect wrong selection, cache reuse or missing UID propagation.

**GREEN.** Extend the existing client config, AuthMiddleware renewal and transport identity. Isolate disk reuse and retain single-flight behavior for the same identity. Rerun D2–D4.

**REFACTOR.** Reuse the existing cache and transport owner; do not create a parallel token or credential resolver.

### Scenario 4 — Registration stays registration; deployed loading stays read-only

**Behavior.** Registration credentials still declare normally. Deployment-key registration is refused before a write. Pull by exact UID plus from_path loads registered state; offline loading makes no request. A bundle root different from the selected UID fails online startup before evidence writes, without declaring or substituting latest.

**RED.** Add planned Python `test_registered_deployment_loading` (D5), covering registry unchanged on refusal, exact-UID pull and mismatched bundle online startup. Expected RED is missing validation/separation.

**GREEN.** Extend the existing Cards and WyrdState boundaries only where missing. Preserve the normal declaration saga and offline bundle validation. CLI JSON reports exact identity for new, deduplicated and idempotent results. Rerun D1/D5.

**REFACTOR.** Reuse CardSelector::uid, Cards::pull and from_path; add no authoring-file comparator, new loader or credential-dependent declare behavior.

### Scenario 5 — Direct-key OTLP agrees with exchange

**Behavior.** Stock exporters send x-wyrd-api-key plus x-wyrd-workload-uid over HTTP and gRPC. Spans, logs and metrics carry the same selected publisher principal as token exchange. Wrong/missing/conflicting selectors fail before body decoding; revoked keys fail on the next export.

**RED.** Add planned OTLP journey `deployment_identity::deployment_key_routes` in the existing otlp target (D6). Use stock export requests and malformed bodies to distinguish pre-decode refusal.

**GREEN.** Reuse the server auth selection path before existing OTLP decoders, preserving row stamping and ordinary authorization/audit limits. Remove the unused wyrd.service.card_uid constant. Rerun D2/D6.

**REFACTOR.** Keep publisher identity out of row attributes; introduce no exporter-specific authentication implementation.

### Scenario 6 — Rolling release and rotation

**Behavior.** CLI declares two releases and reports their exact UIDs for all outcomes. Two processes share a deployment key and emit distinct-version traces/logs/metrics/observations. New key rollout succeeds before old key revocation; the old key then cannot exchange, while outstanding bearers retain bounded validity.

**RED.** Add planned Python `test_rolling_deployment_and_key_rotation` (D7), driving documented CLI commands and real SDK writes/queries. Expect incorrect attribution or inability to reuse a secret.

**GREEN.** Complete the documented release example with separate CI/runtime identities and explicit UID configuration. Cover original UID on redeclaration and preserve unbound credentials. Rerun D1/D2/D5–D7.

**REFACTOR.** Keep declare and pull independent, remove accidental per-release issuance and reuse existing evidence query contracts.

### Scenario 7 — Existing-key replacement and retained data

**Behavior.** An existing exact-version Card-bound key cannot exchange or directly export after cutover, while unbound keys remain usable. Metadata/audit/evidence survive and instructions explain reissuance. New deployment keys authenticate. Unsupported old/new serving overlap fails admission.

**RED.** Add planned Postgres test `pg_admin_principals::deployment_key_cutover_preserves_unbound_credentials` (D8), plus replacement assertions in D7. Expected RED is retained acceptance or silent broadening of old keys.

**GREEN.** Extend the owning SQL schema/query and compatibility authorities with an explicit target discriminator/constraint that separates unbound, retired version-bound and new deployment keys. Follow immutable migration and release-manifest rules; retain data and prove safe cutover. Rerun D1/D7/D8.

**REFACTOR.** Keep persistent representation in the SQL owner and credential behavior in auth; never rewrite historical migration files or erase audit metadata.

### Scenario 8 — Public SDK parity and Kubernetes deployment

**Behavior.** Rust, Python and TypeScript public clients expose the same management, selection, refresh and registration/loading behavior. The documented Kubernetes deployment reads its key from a Secret and passes distinct release UIDs; both revisions have their own stored evidence. Per-version grants remain independent.

**RED.** Add planned Rust `principal_roles::deployment_identity` (D9), TypeScript test `deployment keys select exact versions` (D10), and a gated Kubernetes deployment check D11. Missing projections or misattribution are RED; missing cluster prerequisites are setup work.

**GREEN.** Close client re-exports, PyO3 registration/package/stubs, N-API/generated declarations and example consumers. Extend existing kind provisioning for the focused release journey without synthetic load. Rerun D2/D4/D5/D7/D9–D11.

**REFACTOR.** Keep SDKs thin, regenerate declarations from their owners and reuse existing cluster setup/cleanup rather than a new deployment framework.

## Acceptance Criteria

| Obligations | Required observation and proof |
|---|---|
| REQ-009, AC-004 | D1/D7 observe exact version/UID for declared, deduplicated and replayed results; redeclaration keeps identity. |
| REQ-010–013, REQ-035, INV-001/003, AC-005/018 | D1/D2 prove management isolation, active exact-version selection, independent roles and safe refusal/metadata across HTTP/MCP/CLI/SDKs. |
| REQ-014/018, AC-006/011 | D2/D3/D9/D10 preserve credential precedence, unbound credentials and version-specific grant changes. |
| REQ-016–017, INV-005, AC-007 | D5/D9/D10 prove unchanged registration, refused deployed declaration, offline loading and exact-root validation before online writes. |
| REQ-015/034, AC-008 | D6 proves exchange-equivalent HTTP/gRPC direct-key OTLP and pre-decode refusal for all signal types. |
| REQ-019, INV-002/004/006, AC-009 | D7 proves two concurrent versions with one secret, separate CI/runtime identities and successful rotation without a CI provider dependency. |
| REQ-020, AC-016 | Static owner search shows no unused service attribute remains; TASK-001 closes public old-command removal. |
| REQ-033, AC-017 | D4/D9/D10 prove memory/disk/transport isolation and correct subject through refresh. |
| REQ-029, INV-009, AC-010 | D11 and the platform guide prove Kubernetes Secret/UID configuration and rolling-update attribution. Core SDK behavior remains portable. |
| REQ-036, AC-019 | D8 and rollout evidence prove no key migration/broadening, preserved unbound credentials, retained evidence and rejected unsafe skew. |

## Implementation Reference

- **Credential contracts and admin:** `wyrd-spec/src/auth/issue_key.rs` currently
  returns exact `card_ref` and `principal_id`. Server
  `components/auth/routes.rs::issue_key` authorizes service_accounts:write;
  `components/principals/routes.rs::{list_credentials_for,
  revoke_credential_for}` owns tenant-scoped metadata and revocation, consumed
  by `src/mcp/principals.rs` and `wyrd-client::Principals`. Extend that complete
  workflow for the approved target and endpoints. CLI consumers are
  `auth/issue_key.rs` and `principal/credential.rs`. Existing CLI journeys and
  SDK principal-role stories depend on exact-version issuance and must change
  with the contract; do not retain a hidden alternate issuance path.
- **Auth:** `crates/wyrd/wyrd-auth/src/exchange_api_key.rs::ExchangeApiKey`
  and `issuance.rs::TenantTokenIssuer` already perform verification, grants,
  scoped issuance and audit. Server `http/otlp.rs` and `grpc/otlp.rs` consume
  authentication before decoding. Missing behavior is selecting a version from
  a verified deployment credential; a second issuer would duplicate authority.
- **Persistence:** `wyrd-sql/src/queries/auth/api_keys.rs::{revoke_api_key,
  list_api_key_metadata, credential_belongs_to}` currently targets principal
  credentials. Existing auth migration bindings identify exact principals.
  Extend the typed target and constraints under forced RLS; the persisted
  invariant is exactly one supported credential target with no cross-tenant
  linkage. Private columns/index design is implementer-owned. The old key's
  verifier is never converted into a broader grant.
- **Client/state:** `wyrd-client/src/config.rs` owns credential precedence;
  `auth.rs::AuthMiddleware` owns cache/renewal; authenticated transport consumers
  reuse it. `cards/handle.rs::{CardSelector::uid, Cards::pull}` already pulls
  registered exact graphs atomically. `state.rs::{from_path, declare_with,
  start_bifrost_with_config}` owns offline validation, registration saga and
  online lifecycle. Extend the missing UID check, not graph orchestration.
- **Projections/tests:** SDK Rust principal-role stories, Python state and
  declaration journeys, TypeScript principal-role and declare/pull stories,
  `wyrd-cli --test cli` and `wyrd-testing --test otlp` already exercise these
  owners. Extend native wrappers, public exports and authored/generated
  declarations together. Python and Node lifetime coverage runs in its own
  language; Rust-only cache checks cannot substitute.
- **Deployment:** `scripts/server/test-kind-autoscale.sh` and
  `deploy/kubernetes/kind/` provide cluster, Postgres, storage, trust,
  readiness and cleanup patterns. The main mise file has no existing workload
  identity kind lane; add the focused gated entry below. Reuse provisioning,
  never the autoscale traffic generator. Own the platform guide, tested
  deployment example source and auth/SDK/security/Bifrost identity and
  operations rollout sections. TASK-001 owns doc navigation and CLI/ASGI prose;
  preserve unrelated staged documentation and server edits.

## Verification and Evidence

Every D1–D11 selector/task below is **planned**. Confirm actual test collection
and the resulting exact selector after adding it and before RED. Targets,
package runners and managed setup below were checked in current manifests,
test roots and `mise.toml`; private fixtures remain implementation-owned.

Build prerequisites: `mise run py:setup`, `mise run ts:build`,
`mise run ts:build:testing` and `mise run build:server`. Use sequential Cargo
builds. The managed-Postgres wrapper carries only repository test credentials;
run migrations inside that lifecycle as the table commands specify.

| Command | Purpose |
|---|---|
| **D1** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli -E "test(=auth_issue_key_journey::deployment_key_management)"'` | Public management and CLI identity output |
| **D2** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/integration/test_deployment_identity.py::test_deployment_key_selection'` | Exchange and safe negative cases |
| **D3** `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=config::tests::workload_uid_uses_selected_credential)'` | Selected-credential precedence |
| **D4** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/integration/test_deployment_identity.py::test_deployment_token_cache_isolation'` | Real-server cache/refresh/transport subjects |
| **D5** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/integration/test_deployment_identity.py::test_registered_deployment_loading'` | Registration/loading split |
| **D6** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=deployment_identity::deployment_key_routes)"'` | OTLP HTTP/gRPC and all signal attribution |
| **D7** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/integration/test_deployment_identity.py::test_rolling_deployment_and_key_rotation'` | Full CLI/Python release example |
| **D8** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_admin_principals::deployment_key_cutover_preserves_unbound_credentials)"'` | Persistent target cutover and retained data |
| **D9** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=principal_roles::deployment_identity)"'` | Focused Rust SDK deployment journey |
| **D10** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/principal-roles.test.ts -t "^deployment keys select exact versions$"'` | Focused TypeScript SDK deployment journey |
| **D11** `mise run test:deployment:kind` | Planned gated task: real documented workload Secret, two rollout revisions, stored per-version evidence, deterministic cleanup |

D11 is a new task to define and collect, not an existing passing lane. It runs
only the deployment identity scenario, does not create synthetic host load,
and records actual cluster/platform prerequisites and observations. Missing
Docker/kind is not positive evidence or a reason to weaken the required proof.

Run the affected pure auth/client/spec owner tests, then focused SQL auth
integration modules using the existing principal-integration recipe (not the
whole identity/SDK suites). Run `mise run fmt`, `mise run lints`,
`mise run py:format`, `mise run py:lints`, `mise run py:typecheck`,
`mise run ts:format`, `mise run ts:lints`, `mise run ts:typecheck`,
`mise run ts:napi:check`, `mise run codegen:check`, `mise run check:deps`,
`mise run check:tenant-isolation` and `mise exec -- git diff --check` for this
cross-contract write set. Python authored stubs and generated Node declarations
are part of the same change; regenerate through their owners.

Architecture/prose/example changes use static review and the focused example
journeys, with no manufactured RED. At integrated change review, run combined
docs/example verification, whole affected SDK journey suites, local-bootstrap
regressions and ASGI identity-refusal seams once. Record integrated evidence
as pending until that candidate passes; do not run blanket suites in each task.
Record scenario RED/GREEN, actual focused selectors, migration/cutover evidence,
rotation observations and every unresolved environment limitation honestly.

## Material Stop Conditions

Stop if the approved selector/metadata/public surfaces cannot express the
behavior, if version principals or grants must merge, if old secrets would gain
new authority, if data/audit retention or tenant isolation would weaken, or if
exact-UID loading needs a second registry orchestration. Do not add collector
identity or change trusted issuer behavior. Schema/contract updates already
required above are authorized scope, not an invitation to redesign auth.

## Authority Links

- [Approved revision 5](../spec.md)
- [Root rules](../../../../AGENTS.md)
- [Change workflow](../../../spec-driven-development.md)
- [Execution discipline](../../../implementation-execution.md)
- [Testing](../../../../TESTING.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Security posture](../../../../architecture/wyrd-security-posture.md)
- [Bifrost design](../../../../architecture/bifrost-design.md)
- [Deployment/release](../../../../architecture/operations/deployment-and-release.md)
- [SQL rules](../../../../crates/wyrd/wyrd-sql/AGENTS.md)
- [Python rules](../../../../sdks/wyrd-sdk-python/AGENTS.md)
- [TypeScript rules](../../../../sdks/wyrd-sdk-ts/AGENTS.md)
