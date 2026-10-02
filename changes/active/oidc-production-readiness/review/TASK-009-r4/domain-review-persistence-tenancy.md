# TASK-009 r4 persistence and tenancy domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `0bd3686e8bb763b07376f84661946aadc6200bfb`
- Candidate tree: `86fb8e11681b4d2e60bba2aa7ee3bf2c4f134153`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Current remediation: `changes/active/oidc-production-readiness/review/TASK-009-r3/TASK-009-R3-platform-login-boundary-and-contract.md`

The candidate commit and tree remained unchanged during this review. `.codegraph/`
is absent, so source and caller tracing used Git and repository search.

Binding human direction is applied exactly: `FIND-TASK-009-5`,
`FIND-TASK-009-14`, and `FIND-TASK-009-15` remain withdrawn and are not
reopened, renamed, or implemented indirectly. In particular, this review does
not require an audience migration preflight, overlap column, or dual write.
Placement, naming, structure, and wording alone are non-blocking.

## Reviewed boundary and authority coverage

| Boundary | Authority and source coverage | Result |
|---|---|---|
| Tenant SQL isolation and login-state routing | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `wyrd-auth/src/{connections,login,callback}.rs`; `wyrd-sql/src/postgres.rs`; tenant auth queries | **PASS.** Tenant connection lifecycle, login-state consumption, identity resolution, role synchronization, issuance, and completion use tenant-bound `TenantConn` transactions under RLS. The common callback's pre-tenant lookup is the existing narrow `SECURITY DEFINER` state-hash lookup, after which all state access occurs in the resolved tenant transaction. |
| Platform/tenant plane separation and SQL capabilities | Security posture platform/tenant separation; `wyrd-server/src/components/platform/identity.rs`; `wyrd-auth/src/{platform_authz,platform_login,platform_sessions}.rs`; `wyrd-sql/src/{operator_pool,queries/platform/identity}.rs` | **PASS.** Cross-tenant platform reads use `OperatorPool`; audited platform mutations use the operator-created `TenantConn` returned by `begin_platform_audited`. No changed production signature propagates a raw `PgPool`, and no tenant credential or tenant transaction reaches the platform plane. |
| Configure-before-persist and failure durability | Spec REQ-003/004/005/007 and INV-004; TASK-009; R3 remediation; `configure_connection`; `RelyingParty::{discover,fetch}`; platform connection upsert; served platform test | **PASS.** Full metadata and JWKS discovery succeeds before sealing or upserting. Discovery/JWKS refusal leaves the existing durable singleton row unchanged. Secret sealing and the connection upsert precede the single commit; a sealing, write, audit, or commit failure cannot publish the requested row. |
| Same-issuer cache replacement versus durable authority | R3 preserved behavior; `RelyingParty::{discover,cached}`; `PlatformLogin::{begin,complete}`; `platform_oidc_connection`; served replacement proof | **PASS.** Successful fresh discovery replaces only public issuer metadata/JWKS in the process cache. Every begin and callback still reloads the durable platform connection for client credentials, issuer, and audience. Cache replacement before commit therefore cannot create platform authority or publish the requested client configuration; a later durable failure leaves the old row authoritative. |
| Tenant candidate revision lifecycle | Spec REQ-002/003/016/017; `HumanConnections::{begin_test,tested_candidate,stamp_test_sign_in,activate,active_connection}`; callback completion path | **PASS.** Provider IO runs outside tenant transactions and locks. The test state binds the exact tenant candidate revision; callback and activation re-check that revision, issuer, and client through RLS and the slot lock. Test stamping and activation commit with their authorization audit evidence. |
| Platform login-state and identity durability | `PlatformLogin::{begin,complete}`; platform identity queries; `PlatformSessions::issue_federated` | **PASS.** Platform state is inserted and atomically consumed through `OperatorPool`; connection replacement invalidates an in-flight state whose issuer no longer matches. Subject pinning, session issuance, and canonical audit commit as one operator transaction. |
| Audience derivation and migration | Spec REQ-004; OIDC Core client-ID audience rule; `20261002000001_platform_oidc_client_audience.sql`; platform request/view/row and decoder; lead direction for `FIND-TASK-009-14` | **PASS under binding direction.** The human ID-token audience is derived from `client_id`, and the redundant platform `expected_audience` column is dropped directly. No preflight, compatibility column, overlap representation, or dual-write is required. Workload audience configuration remains separate. |
| Secret persistence and exposure | Spec REQ-005; `seal_platform_client_secret`; tenant `HumanConnections::stage`; platform/tenant row decoders and response projections | **PASS.** Secret-bearing methods require the deployment keyring, persist only sealed bytes, fail closed when the key is unavailable, and omit secret material from returned views. Cache entries contain provider metadata/JWKS, not client secrets. |
| Audit transaction boundaries | Repository audit rules; `PlatformAuthorization::authorize`; platform configure upsert/commit; tenant connection lifecycle and callback issuance | **PASS.** Platform configuration's allowed decision and singleton upsert share one transaction; audit append failure refuses before mutation. Tenant lifecycle mutations and successful session issuance likewise stage authorization evidence on their owning tenant transaction. Callback refusal audit remains the explicitly best-effort failure record and does not make a failed grant succeed. |

## Persistence and failure-path assessment

Platform configuration authorizes first and holds the operator-created audited
transaction (`components/platform/identity.rs:214-223`), then performs full
screened discovery and JWKS loading (`:249-258`). Only after discovery succeeds
does it seal the client secret and upsert the singleton on that same transaction
(`:260-288`). `RelyingParty::discover` inserts into the process cache only after
both provider fetches succeed and leaves the prior entry untouched on failure
(`shared/wyrd-auth-oidc/src/relying_party.rs:397-435`). The served platform test
asserts unavailable and undecodable JWKS return the stable `503` while the
stored `client_id` remains unchanged, and proves successful same-issuer
reconfiguration supplies the next begin/callback from the refreshed cache
(`wyrd-server/tests/platform_admin_e2e.rs:1607-1656,1794-1815`).

The cache is not durable authority. `PlatformLogin::begin` and `complete` first
reload the singleton connection through `OperatorPool`; the connection supplies
the issuer, client ID, sealed client authentication, and hence the ID-token
audience. The cache supplies only discovery metadata and JWKS keyed by issuer.
Consequently, a cache refresh followed by a seal, SQL, audit, or commit failure
does not make the requested client configuration usable.

Tenant login remains separately tenant-bound. The route key resolves only to a
tenant id, `active_connection` reads the Active revision through an RLS
`TenantConn`, and login state records that revision, issuer, and client. The
callback resolves the owning tenant from the opaque state hash through the
single narrow definer function, consumes the row under that tenant's RLS, and
re-checks the exact Active binding before issuing and sealing a session. No
platform connection or cache is a fallback for a tenant failure.

## Material proposed findings

None.

The prior proposal concerning pre-existing human `jwks_ttl_secs` surfaces was
independently rejected in round three as task-unrelated public/schema debt and
is not revived. The public `PlatformLogin::relying_party` accessor is also not
reported: `FIND-TASK-009-15` is withdrawn by binding lead direction, and its
placement has no behavioral, security, tenancy, durability, or public-contract
consequence.

## Prior-finding closure

- `FIND-TASK-009-13`: **CLOSED.** Platform setup performs full discovery and
  JWKS loading through the process-owned cache before persistence; failed
  discovery preserves the durable row and successful same-issuer setup refreshes
  the provider used by login.
- `FIND-TASK-009-14`: **WITHDRAWN — NOT REOPENED.** The direct unreleased-column
  removal remains as directed, with no preflight, overlap column, or dual write.
- `FIND-TASK-009-15`: **WITHDRAWN — NOT REOPENED.** No placement-only owner
  remediation is required.
- `FIND-TASK-009-5`: **WITHDRAWN — NOT REOPENED.** No equivalent
  provider-error redaction requirement is introduced.
- `FIND-TASK-009-16`: **CLOSED in this boundary.** The existing served platform
  failure cases now assert `503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`, while
  preserving the stored row, and the served OpenAPI test pins that response.

## Verification and limits

Available candidate verification includes passing exact, repository-wrapped
selectors for:

- `federated_platform_sign_in_runs_through_the_served_callback` — 1/1 passed;
- `the_served_document_describes_the_composed_surface` — 1/1 passed.

Source inspection also covered the existing PostgreSQL proofs for one-use
platform login state and singleton connection replacement, plus the tenant RLS,
candidate revision, audit, and sealed-secret paths named above. This reviewer
did not rerun lanes. Under the standing narrowest-lane rule, full identity and
every-language journeys are final change-review work, not a TASK-009 task-review
limit. No persistence/tenancy finding depends on an unrun broad lane.

## Overall result

**PASS** — the cumulative candidate preserves tenant RLS, platform/tenant
separation, SQL capability boundaries, audited atomic writes, secret sealing,
client-ID audience derivation, and durable authority despite process-local cache
replacement. No material persistence or tenancy finding remains.
