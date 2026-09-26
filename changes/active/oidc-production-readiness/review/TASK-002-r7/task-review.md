# TASK-002 Task Implementation Review — R7

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `e1ce3c847c14d306c69a704cd15ce6686db9e62e`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation inputs: TASK-002 R1 through R6 at their recorded review paths
- Reviewed range: complete cumulative base-to-candidate diff, 101 files

The candidate remained at the stated commit during this review. Lead-directed
reuse and test commits recorded in the evidence tables were treated as
authorized inputs, not scope drift, and were inspected for regression.

## Overall result: PASS

The cumulative candidate satisfies the original task and closes all fourteen
prior findings. No material task-acceptance finding remains.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006: route key is pre-login routing context only; callback derives tenant and connection solely from one-use state | `HumanConnections::begin_login`, `AuthorizationCodeExchange::complete`, the tenantless callback handler, and `WyrdPostgres::login_state_tenant` bind the resolved tenant and exact Active connection into opaque durable state; callback accepts only code/state | Login handler hostility tests, login-state Postgres tests, callback refusal journey, and OpenAPI contract evidence recorded in TASK-002 | PASS |
| REQ-007: authorization code, PKCE, state, nonce, exact redirect, bounded state, screened provider IO, and complete ID-token verification fail closed | State is random and hashed, consumption commits before provider IO, provider destinations use screened clients, and the callback verifies issuer, audience, `azp`, nonce, signature/key, advertised asymmetric algorithm, required expiry/issuance claims, and time bounds | `tenant_callback_refusal_journey`, callback screening tests, advertised-HS256 refusal proof, and shared verifier tests | PASS |
| REQ-008: verified provider identity provisions a tenant User by exact `(issuer, sub)`, maps only current tenant roles, and grants no privileged default | Human connection input and stored decode require `sub`; callback resolves identity by issuer/subject, replaces mapped roles, and issues through the existing tenant issuer | `tenant_human_login_journey`, exact-sub tests, mapped/unmapped callback Postgres tests | PASS |
| REQ-013: machine credentials remain independent of human SSO | API-key and workload assertion grant owners remain separate from tenant OIDC callback and human refresh paths | `tenant_machine_independence_journey` and workload binding verifier coverage | PASS |
| REQ-014: provider replacement is tested before activation and cannot transfer identity or authority | Login state and refresh rows bind exact connection id/revision; callback and renewal re-read the Active slot; email is display-only and no longer unique/linking authority | `tenant_provider_switch_journey`, same-email separate-User proof, connection lifecycle journeys | PASS |
| REQ-015: shared issuers and services cannot cross tenant authority | The privileged state lookup discloses only tenant id; all identity, role, completion, issuance, and refresh work then runs on `TenantConn` behind forced RLS | Same-issuer two-tenant journey, wrong-tenant callback/redemption cases, login-state isolation tests, recorded tenant-isolation gate | PASS |
| REQ-016: inactive/replaced/removed connection blocks login and renewal; access remains a five-minute snapshot | Callback checks the consumed connection binding; `issue_human_session` locks the tenant connection slot and verifies exact Active id/revision before minting; successor refresh rows preserve provenance | Provider-switch and connection-cutoff journeys; fresh `connection_deactivation_overlapping_rotation_ends_successor` focused Postgres test passed | PASS |
| REQ-017: security-significant outcomes and role changes use canonical redacted audit; required audit failure establishes no session | Role synchronization, issuance, refresh containment, and administrative revocation use the existing audit path and caller-owned transaction boundaries; refusal audit remains on the documented non-blocking path | Login/refusal audit journey cases, changed/unchanged role audit tests, audit rollback and replay-containment proofs | PASS |
| INV-001: paths, hosts, headers, browser state, email, and provider bytes cannot select effective tenant/connection | Typed route-key resolution precedes state creation; after initiation only opaque bound state selects the tenant and connection; no fallback selector exists | Header-hostility, replay/unknown-state, wrong-tenant, and same-email cases | PASS |
| INV-002: stable human identity is exactly `(issuer, subject)` and email never links | `sub` is required at request and stored-row boundaries; `ensure_user_identity` receives verified issuer and subject; email lookup and uniqueness linking were removed | Exact-sub tests and provider-switch same-email journey | PASS |
| INV-003: platform, tenant-human, and workload planes remain distinct | Tenant callback issues only tenant User authority; platform login and workload assertion flows retain their own owners and verifier semantics | Human, machine-independence, and platform/workload verifier evidence | PASS |
| INV-004: trust, SSRF/DNS, replay, audit, refresh containment, and RLS uncertainty fail closed | Screened provider IO, consume-before-IO state, forced RLS, canonical audit, family-before-connection locking, and exact Active-connection checks are on production paths | Refusal journey, tenant-isolation evidence, ancestor replay overlap, administrative revocation overlap, and connection deactivation overlap tests | PASS |
| AC-002 task slice: controlled real-provider login creates a User and proves allowed and denied Wyrd calls | State-bound callback, exact identity/role resolution, tenant issuance, and sealed completion compose through the real server | `tenant_human_login_journey` | PASS |
| AC-003 task slice: tenant/provider state and same-issuer users remain tenant-separated | State lookup supplies only the tenant needed to open RLS; identity and completion remain tenant-bound | `tenant_callback_refusal_journey` and `same_issuer_two_tenant_isolation_keycloak` | PASS |
| AC-005 task slice: API-key and exactly bound workload credentials continue with SSO | Machine grant arms are unchanged and separate from OIDC ID-token policy | `tenant_machine_independence_journey` | PASS |
| AC-006: provider switch cuts old renewal off, preserves owner recovery, applies new mapping on issuance, and never email-links | Active-slot/provenance enforcement and exact issuer/subject identity implement the required switch semantics | `tenant_provider_switch_journey` | PASS |
| AC-007 task slice: outage, unsafe destination, wrong state/nonce/issuer/audience/key/algorithm/claims, replay, inactive connection, audit failure, and mapping/lifecycle faults are exercised | Failure branches stay on the shared verifier, screened HTTP, state, audit, refresh, and connection owners | Refusal/switch journeys plus focused verifier, replay, revocation, and connection overlap tests | PASS |
| Packet initiation/callback/completion contract | `BeginLogin` requires exactly one browser or known CLI binding; state is at least 256 bits; callback contains no tenant selector or credentials; browser completion is sealed and redirected to the fixed same-origin path | Binding-shape, replay, completion, response-leak, schema, and OpenAPI tests | PASS |
| Packet renewal contract and refresh provenance | New OIDC User refresh rows require connection id/revision; legacy unbound rows are revoked; lookup derives the family, family lock precedes classification, and issuance takes the connection lock second | Migration/provenance tests, `ancestor_replay_overlapping_rotation_revokes_successor`, and fresh R6 overlap proofs | PASS |
| Public authorization-code exchange retirement | Authorization-code `/auth/token` grant, legacy client/CLI exchange, and token-bearing callback response are absent; API-key, workload, and refresh grants remain | Grant deserialization refusal, callback no-token checks, OpenAPI/schema/codegen evidence | PASS |
| Constraint: reuse existing issuer, verifier, screened HTTP, audit, RLS, refresh, connection, and SQL owners | Candidate composes the existing concrete owners; R4/R6 reuse transaction advisory locks rather than adding a lock service, trait, persistence model, or dependency | Cumulative diff and manifest inspection | PASS |
| Constraint: preserve validation, error handling, replay protection, tenant isolation, caller-owned transactions, and five-minute access semantics | Corrections strengthen shared owners without committing inside `TenantConn` callees or changing access TTL | Source/caller tracing and focused regression evidence | PASS |
| Non-goals: no email linking, provider-token bearer authority, platform fallback, instant access revocation promise, new machine model, TASK-003 BFF route, TASK-004 CLI persistence, or compatibility route | None of these paths appears in the cumulative source or public contract | Diff, route, contract, migration, and documentation inspection | PASS |
| Authorized lead-directed reuse and test commits preserve task behavior | Reuse remains on established audit/verifier/test owners; test-only fixes do not alter production contracts | Cumulative commit and diff inspection | PASS |

## Prior-finding closure

| Finding | Current closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` — mutable human subject mapping | Exact OIDC `sub` is required for new and stored connection configuration | CLOSED |
| `FIND-TASK-002-2` — missing OIDC `azp` semantics | Human ID-token validation enforces present and multi-audience authorized-party binding | CLOSED |
| `FIND-TASK-002-3` — missing role-change audit | Changed provider-mapped roles append canonical evidence in the issuance transaction | CLOSED |
| `FIND-TASK-002-4` — provider algorithm not enforced | Fresh advertised algorithm membership precedes shared asymmetric verification | CLOSED |
| `FIND-TASK-002-5` — duplicate tenant predicates | Tenant login-state transitions rely on forced RLS | CLOSED |
| `FIND-TASK-002-6` — raw pool state lookup | Narrow `WyrdPostgres::login_state_tenant` owns the cross-tenant lookup | CLOSED |
| `FIND-TASK-002-7` — printable PKCE verifier | Durable verifier values remain secret-backed and redacted | CLOSED |
| `FIND-TASK-002-8` — incomplete Rust documentation | R2's cited inventory retains substantive workflow, invariant, error, and panic documentation | CLOSED |
| `FIND-TASK-002-9` — function-scoped imports | Cited imports remain in module-level import blocks | CLOSED |
| `FIND-TASK-002-10` — false algorithm-helper rustdoc | Helper docs limit it to advertised-set membership and name the shared verifier owner | CLOSED |
| `FIND-TASK-002-11` — optional OIDC binding/time claims | ID-token callers require issuer, audience, expiry, and numeric non-future issuance time; workload semantics remain separate | CLOSED |
| `FIND-TASK-002-12` — replay/rotation race | Tenant-qualified family locking occurs before lifecycle classification and remains held through caller commit | CLOSED |
| `FIND-TASK-002-13` — stale refresh lookup rustdoc | Documentation matches lookup, family lock, classification, and test-only lifecycle observation | CLOSED |
| `FIND-TASK-002-14` — administrative revocation can miss a concurrent successor | User revocation now takes the existing family lock after existence resolution and before suspension/family mutation; the caller retains it through audit-coupled commit | CLOSED |

## R6 correction audit

`revoke_principal_in_conn` now acquires `lock_refresh_family(conn, "user",
id)` only in the User branch, after proving the User exists and before either
suspension or `revoke_refresh_family`. The production server route supplies one
`TenantConn`, appends the allowed authorization decision, calls the owner, and
commits only after the owner succeeds. Consequently the advisory family lock
is retained through both writes and audit commit. Service and Agent revocation
remain unchanged.

The competing rotation path obtains the same tenant/kind/principal family lock
before `consume_active_refresh`, then takes the connection-slot lock during
human issuance. R6 therefore preserves the established family-before-connection
order. The fresh deterministic Postgres proof observed administrative
revocation waiting behind an open rotation and then verified the successor was
revoked as `principal_revoked`, no active family row remained, and the User was
suspended. The additional connection-deactivation overlap proof passed and
shows lifecycle mutation waits on the connection-slot owner while the next
rotation refuses the retired connection without minting another successor.

## Proposed material findings

None.

## Verification limits

- CodeGraph reported that no index is available, so caller and source tracing
  used Git, `rg`, and direct file inspection.
- This review freshly ran both focused Postgres proofs through the repository
  setup wrapper: `revoke::pg_tests::refresh_rotation_overlapping_user_revocation_retires_successor`
  and `refresh::pg_tests::connection_deactivation_overlapping_rotation_ends_successor`;
  each passed, as did the migration checks invoked by the wrapper.
- Fresh `git diff --check` for the cumulative range passed. The bounded review
  did not rerun the full Docker/Keycloak/Dex identity journey, principals
  aggregate lanes, SQL aggregate, codegen, docs, format, or workspace lints;
  their candidate results are recorded in TASK-002 and R1–R6 evidence.
- TASK-003 BFF redemption, TASK-004 CLI persistence, and live Okta/Entra
  qualification remain intentional downstream/change-level work rather than
  missing TASK-002 behavior.

## Result

**PASS**

The implementation satisfies every mapped original obligation, preserves the
explicit non-goals, and closes `FIND-TASK-002-1` through
`FIND-TASK-002-14`. No remediation task is proposed by this reviewer.
