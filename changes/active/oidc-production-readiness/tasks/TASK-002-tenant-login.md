---
id: TASK-002
kind: implementation
status: approved
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-006, REQ-007, REQ-008, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, INV-001, INV-002, INV-003, INV-004, AC-002, AC-003, AC-005, AC-006, AC-007]
depends_on: [TASK-001]
---

# Tenant human login and Wyrd authority

## Outcome and Value

The tenant login and callback authenticate against only the active provider, provision a tenant User, map current groups to tenant roles, and issue Wyrd authority without granting cross-tenant or platform access.

## Owners, Scope, Consumers, and Prohibited Changes

Rust server/auth owns provider verification, login state, principal resolution, issuance, renewal, and canonical audit. UI and CLI consume its public contract in later tasks. Reuse the existing token issuer, refresh replay rules, OIDC verifier, screened HTTP, and SQL tenancy. Exclude email-based linking, direct provider-token API authority, platform fallback, instant revocation promises, and a new machine identity model.

## Approach

1. Bind tenant and active connection in one-use server-owned state behind the deployment-controlled callback.
2. Verify code, PKCE, nonce, issuer, audience, signature, time, claims, and provider destinations before resolving identity.
3. Persist tenant identity and mapped roles; issue and audit Wyrd authority.
4. Enforce connection lifecycle on login and renewal while preserving bounded access-token snapshots.
5. Prove machine-key and exact workload-assertion paths remain independent.

## Packet-local login and renewal contract

The server's unauthenticated begin endpoint accepts
`BeginLogin { tenant_route_key, browser_flow_hash?: Sha256,
cli_handoff_id?: UUID }`; the route key is its only tenant routing context.
Exactly one initiation binding is required. The BFF supplies its flow-cookie
hash; the CLI supplies TASK-004's server-issued handoff ID. The server
resolves the tenant's current Active connection through TASK-001 and returns
an authorization URL. It refuses an unknown handoff or a flow binding that
cannot be recorded with its single-use state. `LoginState` is a random
opaque value with at least 256 bits of entropy; its tenant-scoped durable row
binds `tenant_id`, `connection_id`, connection revision, exact issuer/client ID,
deployment-controlled redirect URI, PKCE verifier, nonce, initiation kind
(`Browser|Cli`), optional CLI handoff ID, and bounded expiry. The common
`GET /auth/callback?code&state` accepts no tenant selector. A narrow,
server-role-only lookup of the state hash across tenant RLS returns only the
tenant ID needed to open `TenantConn`; it grants no principal authority and
cannot enumerate states. Within that tenant, atomically consume the matching
row before provider IO. Missing, expired, or replayed state fails with no
fallback to Host, route, email, or platform connection. A failure after
consumption requires a fresh login.

After code exchange and full ID-token verification, re-read the active
connection and require its ID/revision to equal the consumed state before
resolving `(issuer, subject)` or issuing Wyrd authority. No provider response
can change tenant or connection. Browser state binds a BFF-created random
flow ID hash; the callback stores the completed credential against that flow
and redirects to a fixed same-origin BFF completion path with no query
capability. TASK-003 redeems the flow using its HttpOnly flow cookie and BFF
service credential. CLI initiation instead binds TASK-004's handoff ID.
Neither callback response contains provider code or Wyrd tokens. The server
grants a tenant `User` principal with no privileged
default role; only verified group mappings to current tenant roles contribute
grants. Audit success and required authorization decisions in the issuance
transaction; audit failure issues nothing.

Retire the public `TokenRequest::AuthorizationCode { code, state }` grant on
`POST /auth/token` and the token-bearing JSON response on `GET
/auth/callback`; otherwise callers can bypass the BFF/CLI handoff. The common
callback is the only authorization-code exchange owner and returns only the
fixed safe browser redirect or generic CLI success page. Update the existing
CLI call site, server identity tests, HTTP/OpenAPI schema, generated clients,
and docs together; no compatibility route may still expose a code/state to
token exchange. API-key, workload, and refresh grants on `/auth/token` stay.

Extend the existing refresh row with nullable `human_connection_id` and
`human_connection_revision` for OIDC User sessions. They are mandatory for new
OIDC User refresh rows and absent for service/agent paths. Existing OIDC
refresh rows without provenance are revoked at migration; they cannot be
silently attributed to a new provider. During refresh, lock the token family
and active tenant connection in one tenant transaction; reject a missing,
inactive, deleted, or different ID/revision before rotation/issuance. Persist
the same provenance on successor rows. Mapping/grants are recomputed for the
next token. Preserve current replay-family revocation for a genuinely replayed
token; a blocked old-connection token issues no successor. Already issued
access tokens retain only their existing five-minute snapshot.

## Ordered Implementation Scenarios

### Scenario 1 — Verified tenant user login

**Behavior.** Login begun for tenant A returns through the common callback, resolves or creates its User by verified issuer and subject, maps applicable groups, and permits only tenant grants. An unmapped subject gets no privileged grant.

**RED.** Add a real-server OIDC client-to-server journey with an allowed and denied call; observe missing callback or tenant issuance.

**GREEN.** Complete state-bound callback, issuance, and canonical audit; rerun the journey.

**REFACTOR.** Keep verification and issuance on existing concrete owners.

### Scenario 2 — Fail closed at trust boundaries

**Behavior.** Wrong tenant/origin/state/nonce/issuer/audience/key, invalid signature/algorithm/claims, replay, inactive connection, unsafe URL, and outage cannot establish a session or affect another tenant.

**RED.** Extend the real-server journey with the cross-boundary refusals; use narrow verifier tests only where the full journey cannot force a branch. Observe an invalid callback accepted or misrouted.

**GREEN.** Close failures in the shared server path; rerun all scenarios.

**REFACTOR.** Consolidate refusal and redacted audit handling without fallback routing.

### Scenario 3 — Replacement and renewal

**Behavior.** A tested candidate leaves current login intact until activation. Old-connection renewal stops immediately; existing access lasts at most five minutes. A new provider subject is separate until authorized linking; email matches do not link. Mapping changes apply at next issuance.

**RED.** Add a two-provider switch journey including old refresh, same email, mapping change, and owner recovery; observe an old renewal or automatic link accepted.

**GREEN.** Enforce active-connection checks during human issuance and refresh; rerun prior scenarios.

**REFACTOR.** Keep one token issuer and one human refresh path.

### Scenario 4 — Machine paths remain independent

**Behavior.** With human SSO active, Service/Agent API keys and exactly bound workload assertions keep working. Wrong issuer, subject, audience, or tenant fails.

**RED.** Extend the identity journey with SSO enabled and negative machine bindings; expose any broken separation.

**GREEN.** Preserve or repair only the existing machine path necessary for this journey; rerun human and machine scenarios.

**REFACTOR.** Keep human and workload credential ownership distinct.

## Acceptance Criteria

Real-server journeys prove AC-002/003/005/006/007, including audit refusal and same-issuer cross-tenant rejection. Verified tenant identity alone supplies authority; no IdP token becomes a Wyrd API bearer.

## Expected Write Set and Consumer Closure

Likely wyrd-auth, wyrd-server auth routes, wyrd-spec auth contracts, SQL queries/migrations, OpenAPI, and identity journeys. TASK-003 and TASK-004 consume the public login and token contracts.

## Verification and Evidence

Run each newly named Rust test with its exact focused mise exec command and owning Postgres/IdP setup. Broader lanes: mise run test:identity:journey; mise run test:principals:integration; mise run test:principals:unit; mise run codegen:check; mise run check:tenant-isolation; mise run fmt; mise run lints. Record provider-switch, audit-failure, machine-isolation, and wrong-tenant evidence.

Add ignored `wyrd-server --test identity_e2e` selectors
`tenant_human_login_journey`, `tenant_callback_refusal_journey`,
`tenant_provider_switch_journey`, and `tenant_machine_independence_journey`
under the existing Postgres/Keycloak/Dex identity setup. Focus one with
`mise exec -- cargo nextest run --locked -p wyrd-server --test identity_e2e
--run-ignored=all -E 'test(=tenant_human_login_journey)'`; replace the selector
for each other named test. Confirm all four with `mise exec -- cargo nextest
list --locked -p wyrd-server --test identity_e2e`, then include them in
`test:identity:journey:inner` and run the full lane. The login test asserts
User creation, allowed/denied authorization, unmapped zero grants, and audit
(AC-002/007). The refusal test forces wrong state/tenant/origin, nonce,
issuer/audience/signature, replay, outage, and unsafe URLs with no session or
cross-tenant effect; it also asserts the public authorization-code POST is
refused and callback never emits token JSON (AC-003/007). The switch test activates a tested second
provider, proves old refresh refused immediately, five-minute access bound,
same-email separate User, mapping change at next issuance, and owner recovery
(AC-006/007). The machine test exercises API key and exact workload binding
while SSO is active, including wrong issuer/subject/audience/tenant (AC-005).
Keep existing identity tests selected and use narrow verifier unit tests only
for branches infeasible through provider fixtures, recording why. Focused
commands require the identity lane's already running provider/Postgres setup.

Use TASK-001's `test:identity:journey` focused setup wrapper from a clean
checkout: `mise exec -- env WYRD_IDENTITY_TARGET=server
WYRD_IDENTITY_FILTER=tenant_human_login_journey mise run
test:identity:journey`, replacing the filter with each of
`tenant_callback_refusal_journey`, `tenant_provider_switch_journey`, and
`tenant_machine_independence_journey`. The inner focused `cargo nextest`
commands above remain the exact selectors; the wrapper owns service setup,
cleanup, and one-test selection checks.

## Material Stop Conditions

Stop if correctness requires accepting unverified identity, weakening refresh replay protection, changing five-minute token semantics, inferring membership from email, or merging platform and tenant principals.

## Authority Links

[Approved spec](../spec.md); [AGENTS.md](../../../../AGENTS.md); [agent rules](../../../../architecture/agent-rules.md); [security posture](../../../../architecture/wyrd-security-posture.md).

## Implementation Evidence

Commits: `5267bef7f` (header-free login, sealed one-use completions,
retirements), `652830688` (four journeys, email non-unique), `21d8a8d19`
(schemas, OpenAPI contract test, docs, migration test).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006 (route key is untrusted routing context; generic refusal) | `wyrd-auth/src/login.rs::HumanConnections::begin_login`; `wyrd-server/src/auth/login.rs` (`POST /auth/login`) | `login::pg_tests::unknown_tenant_and_no_connection_are_indistinguishable`; `auth::login::pg_tests::*`; `conformance_login_rejects_bare_localhost_host` | PASS |
| REQ-007 (PKCE/state/nonce, one-time state, verified token, screened calls) | `login.rs` state insert (hashed, 5 min TTL); `callback.rs::AuthorizationCodeExchange::complete` consume-then-commit before IO; `finish_id_token_exchange` | `tenant_callback_refusal_journey` steps 1, 5; `callback::screening_tests`; `login::destination_tests` | PASS |
| REQ-008 (callback creates tenant User by (issuer, subject); tenant tokens) | `finish_id_token_exchange` → `ensure_user_identity`, `issue_human_session` | `tenant_human_login_journey` steps 1–2; `auth::callback::pg_tests::finish_issues_seals_and_audits_the_session` | PASS |
| REQ-013 (machine credentials independent of SSO) | unchanged API-key / jwt-bearer paths | `tenant_machine_independence_journey` | PASS |
| REQ-014 (tested replacement; no inherited authority) | connection lifecycle (TASK-001) + login bound to connection id/issuer/client | `tenant_provider_switch_journey` steps 2–5 | PASS |
| REQ-015 (per-tenant authentication; no cross-tenant grant) | state resolves tenant via `wyrd.auth_login_state_tenant`; completion redeemable only in its tenant | `tenant_callback_refusal_journey` step 4; `same_issuer_two_tenant_isolation_keycloak` | PASS |
| REQ-016 (old-connection renewal stops; mapping at next issuance; ≤5 min access) | refresh bound to connection; migration revokes provenance-free user refresh rows | `tenant_provider_switch_journey` steps 1, 2, 4; `tenant_connection_session_cutoff_journey`; `pg_migration::human_connection_upgrade_preflight` (d) | PASS |
| REQ-017 (audit; failure issues nothing) | session issue, seal, and audit in one transaction; failure audit with nil principal | `tenant_callback_refusal_journey` step 6; `tenant_human_login_journey` step 5 | PASS |
| INV-001 (no header/host/email selection) | begin reads only the body; callback reads only hashed state | `auth::login::pg_tests::login_ignores_request_headers`, `the_host_header_cannot_select_a_tenant`; refusal journey step 4 | PASS |
| INV-002 ((issuer, subject) identity; no email linking) | migration drops `auth_users_data_tenant_id_email_key`; `user_by_email` removed (no production caller) | `tenant_provider_switch_journey` step 3 | PASS |
| INV-003 (planes distinct; groups cannot bypass RBAC; no default roles) | groups-only mapping in `finish_id_token_exchange` | `auth::callback::pg_tests` role-mapping test; login journey step 4 (unmapped = zero grants) | PASS |
| INV-004 (fail-closed verification, replay, isolation) | state consume-once; nonce/aud/iss/sig checks; RLS FORCE on `auth_login_state` | refusal journey steps 1, 5; `mise run check:tenant-isolation` | PASS |
| AC-002 | — | `tenant_human_login_journey` | PASS |
| AC-003 | — | `tenant_callback_refusal_journey`, `same_issuer_two_tenant_isolation_keycloak`, `tenant_login_operations_publish_their_contract` | PASS |
| AC-005 | — | `tenant_machine_independence_journey`, `workload_jwt_bearer_journey_keycloak` | PASS |
| AC-006 | — | `tenant_provider_switch_journey` | PASS |
| AC-007 | — | refusal journey steps 1–6; switch journey step 4; `tenant_connection_rotation_journey`; `login::pg_tests::begin_without_a_sealing_key_is_refused_before_any_state` | PASS |

Retirements: `TokenRequest::AuthorizationCode` (`token::tests::authorization_code_grant_is_retired`),
`wyrd_client` `begin_login`, CLI `auth login` and `parse_callback_input`, the
`GET /auth/login` route; docs updated (no instant-revocation promise).

Commands (all exit 0):

- `mise exec -- env WYRD_IDENTITY_TARGET=server WYRD_IDENTITY_FILTER=<name> mise run test:identity:journey` for each of `tenant_human_login_journey`, `tenant_callback_refusal_journey`, `tenant_provider_switch_journey`, `tenant_machine_independence_journey`
- `mise run test:identity:journey` (27/27)
- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E 'test(/^login::/) | test(/^callback::/) | test(/^connections::/)' && cargo nextest run --locked -p wyrd-server --lib -E 'test(/^auth::login::/) | test(/^auth::callback::/) | test(/^components::auth::/)' && cargo nextest run --locked -p wyrd-spec --lib -E 'test(/auth::/)' && cargo nextest run --locked -p wyrd-sql --test pg_migration"`
- `mise run test:principals:integration` (includes `pg_openapi_contract::tenant_login_operations_publish_their_contract`)
- `mise run test:principals:unit`, `mise run test:sql`
- `mise run codegen:regen`, `mise run codegen:check`, `mise run docs:generate`, `mise run docs:check`
- `mise run check:tenant-isolation`, `mise run check:client-tier`
- `mise run fmt`, `mise run lints`, `git diff --check`

Python and TypeScript artifacts did not change (codegen produced no SDK diff),
so no py/TS lanes were required.

Material limits:

- Human login requires a configured sealing key; without one `begin_login`
  refuses with `WYRD_SPEC_400_VALIDATION` before storing state.
- A `cli_handoff_id` binding is refused with `INVALID_STATE` until TASK-004
  supplies the handoff (`login.rs::known_initiation` is the replacement point).
- The browser `/login/complete` BFF route that calls `HumanConnections::redeem_completion`
  is owned by TASK-003; journeys redeem through the owner directly.
- Unsafe discovered provider URLs and the missing sealing key are proven at
  unit/Postgres level only: the journey server is permissive toward local
  providers (Keycloak) and always configures a keyring.
- Email is no longer unique per tenant (`auth_users_data_tenant_id_email_key`
  dropped) because a replacement provider's same-email user must be a separate
  User (INV-002, AC-006).
