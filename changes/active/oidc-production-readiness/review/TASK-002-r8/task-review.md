# TASK-002 Task Implementation Review — R8

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `ced8acaabce7dbe1682bef46d65d073b07e0dbd9`
- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation inputs: TASK-002 R1 through R7, with each prior `verdict.md`
  and `findings-validation.md`
- Reviewed range: complete cumulative base-to-candidate diff, 109 files

`HEAD` equaled the candidate before and after inspection and focused
verification. Lead-directed reuse and test commits recorded separately in the
task evidence were treated as authorized inputs rather than scope drift; their
changes were still inspected for regression.

## Overall result: PASS

The cumulative candidate satisfies the original task, preserves its explicit
non-goals, and closes all fifteen prior findings. No material task-acceptance
finding remains.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006: the route key is pre-login routing context only, while callback tenant and connection come only from one-use server state | `HumanConnections::begin_login` resolves the route key before persisting a random hashed state bound to the exact Active connection; `AuthorizationCodeExchange::execute` uses `WyrdPostgres::login_state_tenant` and the callback accepts only `code` and `state` | Login header-hostility tests, login-state Postgres isolation tests, callback refusal journey, and OpenAPI evidence recorded in TASK-002 | PASS |
| REQ-007: authorization code, PKCE, state, nonce, exact redirect, bounded one-use state, screened provider calls, and full ID-token verification fail closed | `login.rs` generates 32-byte state and nonce plus a PKCE verifier; `callback.rs` commits state consumption before provider IO, uses screened discovery/token/JWKS clients, enforces the bound redirect/connection, advertised asymmetric algorithm, signature/key, issuer, audience, `azp`, nonce, expiry, `nbf`, and non-future numeric `iat` | `tenant_callback_refusal_journey`, screening tests, advertised-HS256 refusal proof, callback Postgres tests, and `oidc_id_token_requires_binding_and_time_claims` | PASS |
| REQ-008: a verified provider identity provisions a tenant User by exact `(issuer, sub)`, maps only current tenant roles, and has no privileged default | Human connection validation and stored decode require `sub`; `ensure_user_identity` keys on verified issuer/subject; callback replaces roles only from mapped verified groups and issues through `TenantTokenIssuer` | `tenant_human_login_journey`, exact-sub and same-email/distinct-subject tests, mapped/unmapped callback tests | PASS |
| REQ-013: machine credentials remain independent of human SSO | API-key and workload assertion owners and verifier semantics remain separate from the tenant OIDC callback and human refresh family | `tenant_machine_independence_journey` and workload-binding verifier coverage | PASS |
| REQ-014: provider replacement is tested before activation and cannot transfer identity or authority | Login state and every human refresh successor bind the exact connection id/revision; callback and renewal require that revision to remain Active; email is display-only and no longer unique/linking authority | `tenant_provider_switch_journey`, same-email separate-User proof, and connection lifecycle journeys | PASS |
| REQ-015: shared issuers and users cannot cross tenant authority | The narrow state-owner lookup reveals only the tenant needed to open `TenantConn`; identity, roles, login completion, issuance, and refresh then run behind forced RLS | Same-issuer two-tenant journey, wrong-tenant callback/redemption cases, login-state isolation test, and recorded tenant-isolation gate | PASS |
| REQ-016: an inactive/replaced/removed connection blocks login and renewal while issued access remains a five-minute snapshot | Callback rechecks its consumed binding; `issue_human_session` takes the family lock then connection-slot lock and verifies exact Active id/revision; successors retain the same provenance; legacy unbound human families are revoked by migration | Provider-switch and connection-cutoff journeys plus deterministic connection-deactivation/rotation proof | PASS |
| REQ-016 administrative revocation seam: successful User revocation cannot miss a concurrent rotation or first issuance | `revoke_principal_in_conn` takes the tenant-qualified User family lock before suspension/family revocation; the sole human refresh insertion owner now takes the same lock before the connection lock and status read, held by caller-owned transaction through commit | Fresh `issuance::pg_tests::initial_session_issuance_and_user_revocation_serialize` passed both orderings; recorded R6 rotation/revocation overlap proof remains | PASS |
| REQ-017: significant login, role, refresh-containment, and administrative outcomes use canonical redacted audit and required audit failure establishes no session | Role sync and successful issuance append on the callback transaction; refresh containment and administrative authorization/revocation use their established canonical transactional paths; callback refusal retains its documented best-effort path | Login/refusal audit journey cases, changed/unchanged role audit tests, audit rollback proof, and replay/revocation tests | PASS |
| INV-001: host, headers, paths, browser data, email, and unverified provider bytes cannot select effective tenant or connection | Only the typed route key selects the pre-login tenant; after state creation only the opaque state and stored binding select tenant/connection, with no fallback | Header-hostility, wrong-state/tenant, replay, same-issuer, and same-email tests | PASS |
| INV-002: stable identity is `(issuer, subject)` and email never links | Exact `sub` is required at request and stored-row boundaries; identity persistence uses issuer/subject; email lookup/uniqueness linking was removed | Exact-sub tests and provider-switch same-email journey | PASS |
| INV-003: platform, tenant-human, and workload planes remain distinct | Tenant callback issues only tenant User authority; platform login and workload assertion retain separate owners and policy | Human, machine-independence, platform, and workload evidence | PASS |
| INV-004: trust, SSRF/DNS, state replay, refresh replay, audit, and RLS uncertainty fail closed | Screened/pinned IO, consume-before-IO state, forced RLS, advertised algorithm plus shared verifier, exact connection checks, and family-before-connection locking are all on production paths | Refusal journey, tenant isolation, ancestor replay overlap, revocation overlaps, and connection-deactivation overlap proofs | PASS |
| AC-002 task slice: controlled-provider login creates a User and proves allowed and denied Wyrd calls | The state-bound callback composes exact identity/role resolution, tenant issuance, canonical audit, and sealed completion through the real server | `tenant_human_login_journey` | PASS |
| AC-003 task slice: tenants/providers and same-issuer identities remain separated | State lookup supplies only the tenant for RLS; connection, identity, completion, and issuance remain tenant-bound | `tenant_callback_refusal_journey` and `same_issuer_two_tenant_isolation_keycloak` | PASS |
| AC-005 task slice: API-key and exactly bound workload credentials continue under SSO | Machine grant arms remain separate from OIDC login and ID-token-only policy | `tenant_machine_independence_journey` | PASS |
| AC-006: provider switch cuts off old renewal, preserves recovery, applies mappings at issuance, and never email-links | Active-slot/provenance enforcement and exact issuer/subject identity implement the switch semantics | `tenant_provider_switch_journey` | PASS |
| AC-007 task slice: outage, unsafe destination, wrong state/nonce/issuer/audience/key/algorithm/claims, replay, inactive connection, audit failure, role changes, provider rotation, and session lifecycle faults have proof | Failure paths stay on the shared verifier, screened HTTP, one-use state, audit, refresh, connection, and issuance owners | Refusal/switch journeys plus focused verifier, audit, replay, revocation, and lifecycle overlap tests | PASS |
| Packet begin/callback/completion contract | `BeginLogin` requires exactly one browser or known CLI binding; callback has no tenant selector and emits neither provider code nor Wyrd token; browser success redirects to fixed `/login/complete`; completion is sealed and one-use | Binding, replay, response-leak, completion, schema, and OpenAPI tests | PASS |
| Packet renewal contract and refresh provenance | Human rows require connection id/revision; refresh derives the immutable family, locks before classification, consumes atomically, and issues through the shared human-session owner with family then connection ordering | Migration/provenance tests, ancestor replay overlap, R6/R7 revocation overlaps, and connection cutoff proof | PASS |
| Public authorization-code exchange retirement | The `/auth/token` authorization-code variant, legacy client/CLI exchange, GET login route, and token-bearing callback response are absent; API-key, workload, delegation, and refresh grants remain | Deserialization refusal, callback response checks, OpenAPI/schema/codegen evidence | PASS |
| Constraint: reuse existing issuer, verifier, screened HTTP, audit, RLS, refresh, connection, and SQL owners | The candidate composes existing concrete owners; concurrency corrections reuse the existing Postgres transaction advisory lock rather than adding a service, trait, persistence object, isolation mode, retry layer, or dependency | Complete diff, caller, mutation, and manifest inspection | PASS |
| Constraint: preserve validation, replay protection, tenant isolation, audit coupling, caller-owned transactions, and five-minute access semantics | Corrections strengthen shared owners without committing inside `TenantConn` callees or changing access-token lifetime | Source and caller tracing plus focused regression evidence | PASS |
| Non-goals and prohibited changes | No email linking, arbitrary provider-token bearer authority, platform fallback, instant access-token revocation promise, new machine model, compatibility route, TASK-003 BFF route, or TASK-004 CLI persistence entered the candidate | Route, contract, source, migration, documentation, and cumulative diff inspection | PASS |
| Authorized lead-directed reuse and test commits preserve task behavior | Reuse remains on existing audit, verifier, and test owners; test-only fixes do not alter production contracts | Commit and cumulative diff inspection | PASS |

## R7 correction audit

`TenantTokenIssuer::issue_human_session` is the only production caller of
`insert_human_refresh_token`; its two production callers are the initial OIDC
callback and refresh rotation. It now calls
`lock_refresh_family(conn, "user", principal_id)` before
`lock_human_connection_slot`, before `human_connection_is_active`, and before
`Self::issue` reads the User status. Because the lock is transaction-scoped and
the callback and token route own commit/rollback, it remains held through
issuance audit, refresh insertion, sealed completion where applicable, and
commit. Rotation's earlier acquisition of the same transaction advisory lock
is reentrant and preserves the fixed family-before-connection order.

Administrative User revocation takes the identical tenant/kind/principal lock
after proving the User exists and before suspension and family revocation. The
fresh focused Postgres proof exercised both reachable orderings without a
sleep-based race: issuance-first made revocation wait and then retire the new
row; revocation-first made issuance wait, re-read the suspended User, return
`PrincipalInactive`, and insert no row. No new synchronization owner or public
contract was added.

## Prior-finding closure

| Finding | Current closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` — mutable human subject mapping | New and stored human connections require exact OIDC `sub` | CLOSED |
| `FIND-TASK-002-2` — missing OIDC authorized-party semantics | Human ID-token verification enforces present and multi-audience `azp` binding | CLOSED |
| `FIND-TASK-002-3` — missing provider role-change audit | Changed mapped roles append one canonical event transactionally; unchanged roles do not | CLOSED |
| `FIND-TASK-002-4` — provider algorithm not enforced | Fresh discovery membership precedes shared asymmetric key/signature verification | CLOSED |
| `FIND-TASK-002-5` — duplicate tenant selection | Tenant login-state transitions rely on forced RLS | CLOSED |
| `FIND-TASK-002-6` — raw-pool state lookup | `WyrdPostgres::login_state_tenant` is the narrow lookup owner | CLOSED |
| `FIND-TASK-002-7` — printable PKCE verifier | Durable verifier values use `SecretString`; debug proof excludes plaintext | CLOSED |
| `FIND-TASK-002-8` — incomplete Rust documentation | R2's exact inventory retains substantive workflow, invariant, error, and panic documentation | CLOSED |
| `FIND-TASK-002-9` — hidden imports | The cited SHA-256, Utoipa, and Wiremock imports remain module-scoped | CLOSED |
| `FIND-TASK-002-10` — false algorithm-helper contract | Helper docs describe advertised membership only and name the shared verifier's enforcement role | CLOSED |
| `FIND-TASK-002-11` — optional OIDC binding/time claims | ID-token callers require issuer, audience, expiry, and numeric non-future issuance time; workload semantics remain separate | CLOSED |
| `FIND-TASK-002-12` — replay/rotation race | Family locking precedes lifecycle classification and remains held through caller commit | CLOSED |
| `FIND-TASK-002-13` — stale refresh lookup rustdoc | Documentation matches lookup, family locking, classification, and its test observation use | CLOSED |
| `FIND-TASK-002-14` — revocation can miss a concurrent refresh successor | User revocation takes the family lock before suspension and family mutation; the rotation overlap proof contains its successor | CLOSED |
| `FIND-TASK-002-15` — revocation can miss concurrent first issuance | The sole human-session owner takes the same family lock before connection/status checks; fresh two-ordering proof passed | CLOSED |

## Proposed material findings

None.

## Verification limits

- No `.codegraph/` directory exists, so caller and source tracing used Git,
  `rg`, and direct full-body inspection.
- This review freshly ran the exact repository-wrapped R7 Postgres test
  `issuance::pg_tests::initial_session_issuance_and_user_revocation_serialize`;
  migration setup and the focused test passed. Fresh cumulative
  `git diff --check` also passed.
- The bounded review did not rerun the full Docker/Keycloak/Dex identity lane,
  principals aggregates, SQL aggregate, codegen, docs, format, or workspace
  lints. Candidate evidence recorded in TASK-002 and R1–R7 reports each as
  passing, including all four required identity journeys and the 27-test full
  identity lane.
- Live Okta/Entra provider qualification is change-level evidence, while
  TASK-003 BFF redemption and TASK-004 CLI persistence are intentional
  downstream work rather than missing TASK-002 behavior.

## Result

**PASS**

The implementation satisfies every mapped original obligation, closes
`FIND-TASK-002-1` through `FIND-TASK-002-15`, and introduces no task-scope
remediation finding.
