---
id: TASK-002
kind: implementation
status: ready
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
