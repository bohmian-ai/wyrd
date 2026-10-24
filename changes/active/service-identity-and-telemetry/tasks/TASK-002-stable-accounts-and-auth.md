---
id: TASK-002
kind: implementation
status: review
spec: SPEC-service-identity-and-telemetry
spec_revision: 13
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-027, REQ-028, REQ-030, REQ-031, REQ-032, REQ-033, REQ-040, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, AC-001, AC-002, AC-003, AC-004, AC-005, AC-009, AC-010, AC-011, AC-014]
depends_on: [TASK-001]
---

> Historical baseline task. TASK-002 is treated as implemented by the maintainer's revision-14 instruction; this does not claim new review or test evidence. Revision-14 authority is the current spec and supplements. Service-only roots, configure and immutable environment propagation are assigned to TASK-009; earlier Service/Agent workload instructions below are superseded, not work to repeat.


## Outcome and Value

One Service account and its credentials work across releases, while every owned request retains its exact Service or Agent root and authorized component graph. Registering checkout v1 and v2 creates Cards only. An administrator creates checkout-runtime once; the same key selects either release without changing account, roles, or tenant. A User with sufficient grants still emits and verifies without first acquiring a machine identity or selecting a root.

## What Changes and What Must Stay True

Implement the complete approved [auth supplement](../supplements/auth-system.md) through existing account/credential/token/context/verification owners. Remove Agent principals, retaining Agent Cards. Permissions stay role-derived; context grants nothing and cannot falsify owner/subject/binding relationships. Ordinary authorized rootless/generic operations remain valid, and signed verification stays synchronous/store-free. Preserve WYRD_ACCESS_TOKEN/WYRD_API_KEY and optional WYRD_WORKLOAD_UID; never broaden test credentials to hide a failure.

The cost is a coordinated breaking auth/persistence baseline. No historical migration, key conversion, compatibility parser/route or destructive reset is required. TASK-001 supplies flat identity. TASK-003 owns warehouse envelopes/producers (including verification results), audit storage and catalog/grant renaming; this task owns verified context, authorization, binding and accepted/scheduled authority. TASK-006 owns evidence reads/retries. Coordinate shared wire/client edits at integration, without another owner; TASK-005 layout configuration can proceed independently after first-wave contracts settle. Update runtime identity/security architecture here; TASK-008 owns final docsite.

## Approach

1. Extend account restriction/association transactions and credentials; remove registration principal/default-role projection and release-bound issuance.
2. Extend existing issuer, graph resolver, claims/verifier and permission consumers; apply the same context at HTTP, OTLP/gRPC, gateway, native Bifrost, MCP and CLI ingress.
3. Extend shared renewal/cache and immutable WyrdState/Run propagation; close SDK bindings, exports, fixtures and generated contracts.
4. Preserve delegation/federation/accepted snapshots, move schedules/activity off version principals, and close targeted tests and owning architecture.

## Ordered Implementation Scenarios

### Scenario 1 — Accounts outlive releases

**Behavior.** Explicit account creation assigns one ID and returns its first key once; Service/Agent registration/deduplication create no principal or grant. Implement the supplement's exact summary/restriction PUT shapes: unrestricted default, explicit Service restriction/null User restriction, wildcard-only family association, unique tenant/kind/name, atomic replacement/conflict, unchanged-value idempotency, unchanged roles/keys/ID. Preserve direct/IdP role sources, one-time secrets, overlapping rotation, secret-free listing, owner/tenant checks, expiry/revocation and bounded existing JWT lifetime; releases never retire credentials.

**RED.** Add [account proof](#account-administration-proof) and [SDK account cases](#sdk-story); fail on missing restriction/conflict behavior or unwanted registration principals, not setup.

**GREEN.** Extend current account transactions/credential lifecycle, delete registration auth projection, and explicitly provision affected fixtures with their original intended grants; rerun both proofs.

**REFACTOR.** Delete obsolete version-principal lookup/projection and summary assumptions while preserving account ownership.

### Scenario 2 — Tokens state exact Card authority

**Behavior.** Optional workload_uid selects an active tenant Service/Agent root under the authenticated account's restriction without changing identity/grants. Sign any_registered for rootless unrestricted callers, root_required with the family snapshot for rootless restricted callers, and scoped exact UIDs for selected roots. Keep observation-target traversal, depth 16, membership 32 and encoded bearer 8 KiB; refuse unusable roots (foreign/unknown identically), malformed/missing/duplicate claims, inconsistent IDs/tenants/root combinations, removed Agent/System public kinds, invalid signature/key/issuer/audience/expiry and depth above five. Direct scoped roots must be members; delegated intersections can omit roots or be empty. Never truncate or issue machine refresh tokens.

**RED.** Add [claim proof](#claim-proof) and [issuance proof](#issuance-proof) with a table of valid/invalid combinations; current embedded account CardRefs must fail the new contract.

**GREEN.** Extend current graph/signing/verifier/request/error owners together; rerun scenario 1 and both proofs.

**REFACTOR.** Remove Card-bearing account projections; retain synchronous store-free verification and in-memory scoped UID checks.

### Scenario 3 — Optional context preserves ordinary callers and truthful ownership

**Behavior.** Enforce the supplement's caller matrix below through shared HTTP/OTLP/gRPC context selection, native admission and typed MCP/CLI arguments; no selector can establish principal/tenant or grant permissions.

| Case | Required result |
| --- | --- |
| Authorized rootless User/admin/unrestricted Service | Registered tenant emit/verify succeeds; owner null unless validated binding establishes it. Missing operation grant refuses. |
| Rootless family Service | Authorized generic work succeeds; Card-sensitive work returns 403 workload_context_required, even with wildcard grants. |
| x-wyrd-workload-uid on rootless bearer | Resolve under signed restriction/grants, without broader live-grant lookup. Unknown/foreign roots refuse identically. |
| Equal/different selector on signed root | Equal is idempotent; different returns signed_workload_mismatch, never unpins. |
| False root/subject/binding occurrence, including administrator | Refuse owned work; separately authorized unowned work remains possible. Unrelated registry/table reads keep ordinary permissions. |
| Gateway key only; generic telemetry | Authorized calls remain valid. Explicit context provides owner without cache collision. Telemetry may have owner only or neither; labels cannot forge correlation. |

**RED.** Add [ingress proof](#ingress-proof) and [SDK negatives](#sdk-story) using real human and least-privilege family actors; assert typed refusal reasons and OTLP record-level semantics across all signals/native frames.

**GREEN.** Extend current extraction/direct-key/Card admission and protocol adapters; rerun scenarios 1–2 and these proofs, preserving auth-before-payload admission and existing envelopes.

**REFACTOR.** Reuse existing root-selection/permission/graph ownership rather than another engine.

### Scenario 4 — Concurrent states and renewal keep their release

**Behavior.** Bundle loading stays offline; concurrent WyrdState instances sharing a key supply independent immutable R1/R2 roots automatically, including under User bearer, while component Run views change only subject. Renewal retains root and caches distinguish rootless/R1/R2 plus credential/audience/existing issuer/tenant/endpoint dimensions, including disk/gateway reuse. Raw clients/exporters accept explicit context; invalid roots never cause unbounded renewal.

**RED.** Add [cache proof](#client-cache-proof) and three-language [SDK story](#sdk-story), asserting offline no-IO load, same account/different owners, correct subjects, concurrent isolation and renewal.

**GREEN.** Extend AuthMiddleware/TokenExchange and immutable state/Run propagation through current facades/exporters/bindings; rerun scenarios 1–3 and read back TASK-003 envelope values after integration.

**REFACTOR.** Delete ambient mutable context and duplicate exchanges/headers; retain the shared Rust client transport/lifecycle.

### Scenario 5 — Delegation intersects both dimensions of authority

**Behavior.** A direct root-bound Service preserves subject/root, adds outer actor/root/optional cid and intersects subject snapshot permissions/Card access with actor current grants/restriction/resolved graph. any_registered is the set identity; exact scoped intersection may be empty, while unresolved root_required subjects refuse exchange. Refuse unrepresentable permission intersections without widening/discarding shared authority; preserve nested actor provenance without historical grants, current-actor ownership, absent top-level delegated cid and depth/tenant/kind checks.

**RED.** Add [delegation proof](#delegation-proof) and User-through-Service SDK cases for overlapping/disjoint scopes; assert exact account pair, roots/provenance and outside-intersection denials.

**GREEN.** Extend DelegateToken, existing PermissionSet intersection/actor claims and context; revalidate the actor's already selected root and current account authority, then rerun scenarios 1–4.

**REFACTOR.** Reuse actor-chain/intersection machinery and delete actor-history/version-principal fallback.

### Scenario 6 — Federation and human/platform boundaries remain explicit

**Behavior.** Workload binding maps trusted issuer/subject/audience to an active stable Service account and optional pinned default; explicit jwt-bearer workload_uid wins, then configured default, then rootless semantics, all within restriction. Assertion principal claims and tenant routing grant nothing. Preserve OIDC/device/refresh/saved-login/browser recovery and role-source behavior, tenant-bound TenantAdmin, separate platform authority, and no public System issuance/delegation; browser recovery never activates runtime activity.

**RED.** Add [federation proof](#federation-proof) for explicit/default/rootless and malicious mapping/root cases. Existing human flows use regression evidence under [identity setup](#identity-regression-setup); only changed behavior needs RED.

**GREEN.** Extend current binding DTO/SQL/routes/JwtBearer and client/CLI/MCP projections; rerun scenarios 1–5 and human regressions without machine refresh tokens.

**REFACTOR.** Delete CardRef-to-principal federation lookup; keep trust, human sessions and platform validation in their owners.

### Scenario 7 — Accepted work freezes identity, context and authority

**Behavior.** Direct/submitted/group-end verification validates exact owner/subject/binding occurrence; acknowledge accepted inputs before queue acceptance and freeze effective grants/Card state, participant roots, account pair, chain and non-secret provenance without secrets. Role/restriction/key changes, deletion, restart/retry cannot replace that snapshot or infer release from account ID; later protected calls still check accepted authority and stage nonblocking audit with separate target/root. System supplies no missing grants.

**RED.** Add [accepted proof](#accepted-authority-proof): submit delegated work, mutate live authority/revoke key, restart/claim/retry and assert unchanged snapshot/provenance, denied excess authority and no shared-component binding-owner swap.

**GREEN.** Extend AcceptedAuthority/ExecutionAuthority/admission/group/queue consumers, preserving descriptive origin separately from reusable credential material; rerun scenarios 1–6 and coordinate producer assertions with TASK-003.

**REFACTOR.** Remove accepted-work live/version-principal recovery; retain fencing/idempotency/leases/ack boundaries and leave trace retry policy to TASK-006.

### Scenario 8 — Only associated account activity activates an exact release schedule

**Behavior.** Scheduled acceptance checks unique family runtime account/current grants and exact binding root; only successful direct machine key/assertion use with validated root (including direct-key ingress) records root/account activity. R2 never activates R1; User/browser/delegated/rootless/cached-bearer requests do not count, other direct callers never reassign authority, and reassociation needs fresh new-account activity. Missing/inactive account, unusable root, insufficient grants or missing System accepts nothing; concurrent claims accept one occurrence then freeze authority.

**RED.** Add [schedule proof](#schedule-proof) with two releases, other authorized callers and reassociation; assert exact admission/refusal, no inherited activity and no duplicate occurrence.

**GREEN.** Extend existing activity SQL/binding admission/BackgroundAuthority/scheduler/queue for explicit family association plus root/account activity; bind durations and let PostgreSQL derive coordination instants, then rerun all proofs.

**REFACTOR.** Delete principal-to-release inference; retain existing scheduler/claim owners and internal System read capability without general authority fallback.

## Acceptance Criteria

| Obligations | Required observations and proof |
| --- | --- |
| REQ-001–006; AC-002/004/009/011 | Scenarios 1/6: stable explicit accounts and principal-owned credential lifecycle; no Card registration principals/grants; atomic unique family administration; human role sources and tenant/platform/System boundaries unchanged. |
| REQ-007–012/033; AC-001/003/011 | Scenarios 2/3/5: exact request/claim shapes, all three Card states, bounded valid signed tokens, generic/rootless success and false-owner/missing-grant/missing-context rejection. Missing states never authorize. |
| REQ-013–014; AC-002/004/011 | Scenarios 4/5/7: concurrent immutable state, context-keyed renewal/cache, exact delegation intersections and frozen restart authority/provenance. An account/root swap or broader grant is a failure. |
| REQ-015–017/040; AC-005/014 | Scenarios 3/7/8: exact binding occurrence survives direct/submitted/grouped/scheduled paths, explicit schedule association, per-root/account activity, no System fallback. Warehouse result shape belongs to TASK-003; evidence-read behavior to TASK-006. |
| REQ-027–028/030–032; AC-009/010/011 | Fresh account/key/federation/queue journeys use only the new contracts; SDKs/HTTP/gRPC/MCP/CLI expose the same typed restrictions/context/errors; architecture and generated contracts agree. No broader credential replacement hides failure. |
| INV-001–007 | Across every scenario: credential-only identity and tenancy, role-derived permissions, independent owner/subject/account, exact durable scope, valid generic/rootless behavior, server-owned decisions and nonblocking audit. TASK-003 proves corresponding stored envelope values. |

## Implementation Reference

No .codegraph directory was present. These inspected symbols/consumers ground reuse; paths are not a helper/file allowlist.

| Existing owner and inspected consumers/tests | Gap and selected extension |
| --- | --- |
| `components/principals/routes.rs::{create_service_principal,issue_credential,list_credentials_for,revoke_credential_for,summary}`; auth DTOs and `queries/auth/service_accounts.rs`; pg_principal_roles | Role validation already precedes atomic account/grants/key commit. Add restriction/unique association/PUT here, retaining credentials and role-source lifecycle. |
| `components/cards/service.rs` calls `queries/cards/auth_projection.rs::upsert_service_account_from_card`; SQL pg_cards_declare/pg_verification_bindings/pg_verifier_executions and testing verification fixtures call it | Delete principal/default-role projection and explicitly provision fixture accounts; retain Card/binding projection. |
| TenantTokenIssuer::{issue,verify,resolve,recover}, ExchangeApiKey/DelegateToken/JwtBearer, card_scope::resolve_card_ref_scope; issuance/exchange pg_tests | Existing issuance derives graph/activity from Card-bearing account. Extend complete issuance/direct-key/federation flows using existing bounded graph resolver, no new auth workflow. |
| AccessTokenClaims::into_verified, TokenVerifier, issue grant/ActClaim, Principal::{card_attribution,authorizes_card}; verifier negatives | Replace embedded account CardRefs with explicit Card state/participant context; preserve store-free verification and permission intersections. |
| auth/{caller_extractor,principal_extractor,token_extract,routes}, GatewayApiKeys (key-hash-only cache), Gate auth/CardRegistry::attributed_cards and signal validation | Extend existing extraction/context/Card-state enforcement and root-sensitive reuse at every ingress; coordinate TASK-003 field/producer edits instead of duplicating admission. |
| wyrd-client auth/config/saved_login/transport, Principals, WyrdState/Run/Bifrost lifecycle/exporters; Rust/Python/TS principal-role stories currently inspect card_ref and issue release-bound CLI keys | Extend immutable request context/cache/renewal and administration handle; close bindings, exports, authored stubs/generated declarations, CLI/MCP, fixtures/examples with intended grants. |
| AcceptedAuthority::new currently drops cid; VerificationControl::{execute,enqueue,resolve}, groups/submissions, ExecutionAuthority/VerifierExecutionQueue, BackgroundAuthority/SystemReadAuthority, VerificationScheduler; pg_verification_runtime | Preserve descriptive origin without secrets and exact accepted scope; distinguish accepted recovery from fresh scheduled resolution, with no new queue. |
| queries/verification.rs::{record_machine_authentication,binding_activity} joins owner to version principal last_authenticated_at | Extend existing activity/admission for unique family account plus root/account activity; retain PostgreSQL clock, RLS and fencing. |

Update runtime identity/account/credential/delegation/schedule sections in architecture/wyrd-design.md and architecture/wyrd-security-posture.md; preserve unaffected policy. TASK-003 owns stored-envelope/audit/Bifrost prose; TASK-008 owns docsite.

## Verification and Evidence

All commands run from repository root unless a working directory is stated. The runners/targets/setup below are verified from current manifests, test roots, TESTING.md and mise tasks. All new names below are **planned**; after adding a test, confirm its actual selector with nextest list/pytest collection/Vitest discovery before RED. A zero-test selection is not evidence. The implementer owns fixture structure, adds tests to existing targets, and records expected RED then GREEN. Use WYRD_LOG=info,wyrd_auth=debug,wyrd_server=debug when diagnosing failures; inspect traces without printing secrets.

### Account administration proof

Planned `pg_principal_roles::stable_account_restrictions_and_credentials` in the existing wyrd-server integration target; real HTTP/Postgres account atomicity, credentials, registry principal absence and administration negatives:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_principal_roles::stable_account_restrictions_and_credentials)"'
```

### Claim proof

Planned table-driven `tests::explicit_card_access_claims_are_fail_closed` in the existing verify-only library target, including encoded size and actor validation:

```bash
mise exec -- cargo nextest run --locked -p wyrd-auth-verify --lib -E 'test(=tests::explicit_card_access_claims_are_fail_closed)'
```

### Issuance proof

Planned `issuance::pg_tests::stable_account_root_selection_preserves_grants`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E "test(=issuance::pg_tests::stable_account_root_selection_preserves_grants)"'
```

### Ingress proof

Planned `negative::pg_tests::optional_workload_context_preserves_authentication` in wyrd-testing's existing otlp target reuses OtlpJourney and real HTTP/gRPC signal clients. Assert context and record-level refusals/readback across signals; native and gateway cases remain in the SDK story, using local provider mocks:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=negative::pg_tests::optional_workload_context_preserves_authentication)"'
```

### Client cache proof

Planned `auth::tests::renewal_and_caches_keep_workload_context_separate` uses the existing mock token server; no credentials, database or new cache harness:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=auth::tests::renewal_and_caches_keep_workload_context_separate)'
```

### Delegation proof

Planned `exchange_api_key::pg_tests::delegation_preserves_roots_and_exact_intersections`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E "test(=exchange_api_key::pg_tests::delegation_preserves_roots_and_exact_intersections)"'
```

### Federation proof

Planned `auth::jwt_bearer::pg_tests::stable_account_mapping_selects_only_allowed_context` in the current server library's local issuer fixture:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --lib -E "test(=auth::jwt_bearer::pg_tests::stable_account_mapping_selects_only_allowed_context)"'
```

### Accepted authority proof

Planned `pg_verification_runtime::accepted_authority_survives_account_and_context_changes`; existing integration module is gated by the existing test-support feature:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_runtime::accepted_authority_survives_account_and_context_changes)"'
```

### Schedule proof

Planned `pg_verification_runtime::only_associated_account_activity_activates_exact_root`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_runtime::only_associated_account_activity_activates_exact_root)"'
```

### SDK story

Extend existing principal-role story owners with planned matching story `stable_accounts_preserve_execution_identity`, using checked-in two-release/shared component Cards, explicit least-privilege accounts, a real WyrdTestServer and repository Postgres. Assert account administration/lifecycle and observable owned native emit/verify success/negative flows, key-only and context-bound gateway invocation/capture, concurrent state isolation, renewal and User-through-Service delegation. Keep one outcome per assertion/test and split cases within the existing story if needed; record exact selectors for any added names before RED. These three public stories are required, not substitutes for one another. For Python, the test is named `test_stable_accounts_preserve_execution_identity`; TypeScript title is exactly `stable accounts preserve execution identity`.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=principal_roles::stable_accounts_preserve_execution_identity)"'
mise run py:setup
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_principal_roles.py::test_stable_accounts_preserve_execution_identity'
mise run ts:build
mise run ts:build:testing
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/principal-roles.test.ts -t "^stable accounts preserve execution identity$"'
```

CLI and MCP project the same account/context operations and negative paths, in planned `principal_journey::stable_account_context_cli_journey` and `principals::pg_tests::stable_account_context_mcp_journey`. Drive public commands and real MCP clients, retaining read availability and explicit write scopes:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo build --locked -p wyrd-cli --bins --tests && cargo nextest run --locked -p wyrd-cli --test cli --run-ignored=all -E "test(=principal_journey::stable_account_context_cli_journey)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=principals::pg_tests::stable_account_context_mcp_journey)"'
```

### Identity regression setup

Existing human/browser/plane behavior uses regression-only proof, with no manufactured RED. Browser fixture is local; saved human renewal uses the identity lane's managed Keycloak/Dex and issuer environment. Docker is required, developer credentials are not. These existing selectors were confirmed from source:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::browser_session_authority_is_tenant_bound_and_revocable)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_KEYCLOAK_ISSUER=http://localhost:8180/realms/wyrd-test WYRD_DEX_ISSUER=http://localhost:5556 scripts/identity/with-identity-providers.sh -- cargo nextest run --locked -p wyrd-ai --test integration --run-ignored=all -E "test(=saved_user_auth::stale_login_refreshes_and_saves_the_renewal)"'
```

### Targeted closure checks

Run `mise run test:principals:unit` and `mise run test:principals:integration` for principal/credential/role regressions; these are the existing narrow owner lanes. After shared consumer integration run `mise run codegen:regen` then `mise run codegen:check`, `mise run check:deps`, `mise run check:tenant-isolation`, `mise run fmt:check` and `mise run lints`. For touched Python/TypeScript bindings run `mise run py:format:check`, `mise run py:lints`, `mise run py:typecheck`, `mise run ts:format:check`, `mise run ts:lints`, `mise run ts:typecheck`, and `mise run ts:napi:check`. Authored stubs and generated outputs must agree with the new public DTOs; do not hand-edit generated declarations. Architecture/schema agreement uses static review and codegen, with no manufactured RED. Full aggregate journeys and `gate` belong to integrated change review.

## Material Stop Conditions

- A new permission system, graph traversal/service, auth endpoint/grant, credential
  store, client orchestrator, migration or compatibility contract appears necessary:
  revalidate reuse first; route material changes through specification approval.
- Any proposal makes a UID mandatory for ordinary account/human authentication,
  permits role-name administrative bypass, widens a credential to hide failing
  emission, infers latest/root/account identity, or accepts absent/malformed scope.
- No exact subject/binding occurrence can be retained across shared components,
  actor chains, accepted recovery or scheduled authority; do not approximate it.
- Token scope/permissions/actor claims exceed the verifier limit: refuse issuance
  as specified; never truncate, raise limits or add a new lookup per observation.
- An implementation would migrate/reset actual developer state, change bounded JWT
  revocation semantics, introduce an auth epoch, or change trace retry/read policy.
- TASK-003's accepted context/envelope handoff cannot preserve original origin or
  causes shared source conflicts: resolve integration ownership with that task;
  do not duplicate managed attribution or leave the seam unverified.

## Authority Links

- [Approved specification revision 13](../spec.md), [complete auth contracts](../supplements/auth-system.md),
  [telemetry production](../supplements/telemetry-schemas.md), and
  [shared tables/origin](../supplements/table-storage-and-correlation.md).
- [Runtime identity](../../../../architecture/wyrd-design.md#runtime-identity),
  [security posture](../../../../architecture/wyrd-security-posture.md#principal-and-credential-lifecycle),
  [authorization](../../../../architecture/wyrd-security-posture.md#authorization-and-policy),
  [tenant binding](../../../../architecture/wyrd-security-posture.md#postgres-tenant-binding),
  [audit](../../../../architecture/wyrd-security-posture.md#audit-integrity-and-privacy).
- [Testing](../../../../TESTING.md), [execution discipline](../../../implementation-execution.md),
  [spec workflow](../../../spec-driven-development.md), [agent topic map](../../../../agent_docs/index.md),
  and the SQL/Python/TypeScript directory AGENTS.md rules.

## Implementation Evidence

Commits `036fc48fe`..`c92f46197` on `task/service-identity-and-telemetry/TASK-002`. Postgres selectors run through `scripts/postgres/with-test-postgres.sh` after `db:migrate:all:inner`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001–006; AC-002/004/009/011 — stable explicit accounts, restriction administration, no registration principals | `20261025000000_stable_service_accounts.sql` (`auth_service_accounts_one_family`); `principals/routes.rs::set_workload_restriction_for`; `wyrd-client` `Principals::set_workload_restriction`; CLI `create-principal --workload-kind/--workload-name`, `set-restriction`; MCP `principals.set_workload_restriction`; Python/TS bindings; registration auth projection and `/auth/issue-key` deleted | `pg_principal_roles::stable_account_restrictions_and_credentials`; `principal_journey::stable_account_context_cli_journey`; `principals::pg_tests::stable_account_context_mcp_journey`; `test:principals:unit`, `test:principals:integration` | PASS |
| REQ-007–012/033; AC-001/003/011 — exact claims, three Card states, bounded tokens | `wyrd-spec` auth `card_access`/`workload_uid`; `wyrd-auth-verify` fail-closed claims; issuance/exchange root selection | `wyrd-auth-verify tests::explicit_card_access_claims_are_fail_closed`; `issuance::pg_tests::stable_account_root_selection_preserves_grants` | PASS |
| Scenario 3 — optional context, typed refusals at every ingress | shared selector path (`token_extract.rs`, `WORKLOAD_UID_HEADER`); Bifrost gate rootless-selector refusal; OAuth refusals carry the Wyrd `code` extension member (`oauth.rs`, `OAuthErrorResponse.code`, client `oauth_error`) | `negative::pg_tests::optional_workload_context_preserves_authentication` (otlp 17/17); SDK stories assert `WYRD_AUTH_401_WORKLOAD_UID_REFUSED`, `WYRD_AUTH_401_API_KEY_INVALID`, `WYRD_AUTH_403_WORKLOAD_CONTEXT_REQUIRED` | PASS |
| REQ-013–014; AC-002/004/011 — concurrent states, context-keyed renewal/cache, delegation intersections | `AuthMiddleware`/`TokenExchange` keyed by root; `WyrdClient::for_workload`; delegation requires actor root | `auth::tests::renewal_and_caches_keep_workload_context_separate`; `exchange_api_key::pg_tests::delegation_preserves_roots_and_exact_intersections`; Rust `principal_roles::stable_accounts_preserve_execution_identity`, `gateway_inference::stable_account_gateway_calls_keep_execution_identity`, `gateway_inference::user_through_service_gateway_delegation_keeps_the_intersection`; Python `test_principal_roles.py::test_stable_accounts_preserve_execution_identity` (+3 split cases); TS `stable accounts preserve execution identity` (+2 split cases) | PASS |
| Scenario 6 — federation | workload binding by principal + optional pinned default; CLI `--principal/--workload-uid` | `auth::jwt_bearer::pg_tests::stable_account_mapping_selects_only_allowed_context`; `pg_browser_sessions::*`; `saved_user_auth::stale_login_refreshes_and_saves_the_renewal` (identity lane) | PASS |
| REQ-015–017/040; AC-005/014 — accepted authority, exact-root schedule activity | `record_machine_authentication(owner, root)`; accepted snapshot consumers | `pg_verification_runtime::accepted_authority_survives_account_and_context_changes`; `pg_verification_runtime::only_associated_account_activity_activates_exact_root` | PASS |
| REQ-027–028/030–032; AC-009/010/011 — same contracts on every surface; architecture/generated contracts agree | `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md` (`0b65f7ff3`); fixtures provision accounts explicitly; shared `WyrdTestServer::{family_account, exchange_api_key_for_workload, exchange_rooted, seed_workload_issuer}` | `codegen:regen` (no diff), `codegen:check`, `check:deps`, `check:tenant-isolation`, `docs:check`, `fmt:check`, `lints`, `py:{format:check,lints,typecheck}`, `ts:{format:check,lints,typecheck,napi:check}`, `git diff --check` | PASS |
| INV-001–007 | Covered by the rows above | Owner sweep: mcp 28/28, wyrd-testing server/oracle/scribe/forge/otlp, wyrd-server integration 254/254 | PASS |

### Diagnoses

- **Symptom:** SDK stories saw `WYRD_SPEC_400_VALIDATION` for a refused root or revoked key at `/auth/token`. **Evidence:** server log `wyrd_code="WYRD_AUTH_401_WORKLOAD_UID_REFUSED"`; body `{"error":"invalid_request"}`. **Cause:** `OAuthError` dropped the Wyrd code, so `wyrd-client` `oauth_error` mapped by OAuth code only. **Fix site:** `OAuthErrorResponse` gains the optional `code` extension member (RFC 6749 §8.2), set by `OAuthError::from`; the client prefers it. OAuth envelopes stay at `/auth/token` as the supplement requires.
- **Symptom:** delegation journeys failed with 400 `actor_has_no_root`. **Cause:** actors exchanged rootless; delegation now requires an actor root. **Fix site:** tests root the actor (`exchange_rooted`, Rust `Deployment::rooted`, Python/TS `rooted`); verification delegates root at the verified subject Card.
- **Symptom:** `observe_ingest::*` "lacks BifrostTable/Read". **Cause:** registration no longer grants `workload`. **Fix site:** the test grants `workload` explicitly; the now-unused `DEFAULT_CARD_ROLE` constant is deleted.

### Limits

- Not run (environment): wyrd-cli `init_*`/`guided_init` (needs a built UI), `cli_device_login_journey` (needs an IdP), two gateway Vault tests (`test:gateway:vault`), `identity_e2e` Keycloak config-binding tests.
- The off-nest `FromRequestParts` fallback ignores the selector; native Bifrost refuses a rootless selector by design.
- The `CredentialIssuance` audit detail has no producer after `/auth/issue-key` removal; audit storage belongs to TASK-003. Credential issuance decisions retain the subject/actor pair.
- Several proofs were written after their production code (no recorded RED).
- Stored workload-root columns belong to TASK-003; SDK release isolation is shown by run id.
- Non-goals stayed excluded: no migration of historical state, no compatibility route, no warehouse envelope, audit-storage or docsite change.
