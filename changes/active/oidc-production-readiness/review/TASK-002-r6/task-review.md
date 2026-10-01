# TASK-002 R6 Task Implementation Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `0ca117a744ddcb7414b104c4382027970531b608`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation tasks: TASK-002-R1 through TASK-002-R5 at the paths supplied for this review
- Reviewed range: complete cumulative `base..candidate` diff, 93 files, 9,537 insertions and 1,842 deletions

The candidate remained `0ca117a744ddcb7414b104c4382027970531b608`
before inspection, after focused verification, and immediately before this
report was written. Review artifacts are outside the immutable subject.
Lead-directed reuse and test commits recorded separately in the task evidence
were treated as authorized and were still checked for regression.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006: the route key is pre-login context only; begin selects one Active tenant connection and the callback derives tenant and connection only from bounded one-use state | `wyrd-auth/src/login.rs::HumanConnections::begin_login`; `wyrd-auth/src/callback.rs::AuthorizationCodeExchange::complete`; `wyrd-sql/src/postgres.rs::login_state_tenant`; deployment-owned redirect construction | `tenant_human_login_journey`; `tenant_callback_refusal_journey`; served OpenAPI contract; recorded identity lane 27/27 | PASS |
| REQ-007: PKCE, state, nonce, redirect, signature, issuer, audience, authorized party, algorithm, key, time, claims, and screened provider IO fail closed | State is consumed before provider IO; the callback checks freshly advertised algorithm membership, calls `verify_id_token_against`, verifies nonce and `azp`, then rechecks the bound Active connection | Fresh `oidc_id_token_requires_binding_and_time_claims` passed; recorded refusal journey, screening, algorithm, `azp`, and four journey results | PASS |
| REQ-008: successful login resolves or creates a tenant User by exact `(issuer, sub)`, maps only current tenant roles, and grants no privileged default | Human input and stored connection decoding require exact `sub`; callback identity and role synchronization feed the existing tenant issuer; email is display-only | Human-login journey, same-email/different-subject proof, role mapping and audit rollback tests | PASS |
| REQ-013: API-key and exact workload-assertion paths remain independent of human SSO | Human callback and refresh are User-only; workload assertions retain the generic external-verification path and machine token grants remain distinct | `tenant_machine_independence_journey`; recorded workload verifier and journey evidence | PASS |
| REQ-014: provider replacement is tested before activation and cannot transfer identity or authority by email | State and refresh rows bind exact connection id/revision; callback and renewal re-read the Active slot; identity remains issuer plus exact subject; migration removes tenant-email uniqueness | `tenant_provider_switch_journey`; connection lifecycle journeys; same-email separate-User proof | PASS |
| REQ-015: shared service and shared issuer use cannot cross tenant authority | State lookup discloses only the tenant needed to open `TenantConn`; state, identity, role, completion, and refresh operations then remain behind forced RLS | Same-issuer two-tenant proof, wrong-tenant callback refusal, login-state Postgres isolation, recorded tenant-isolation check | PASS |
| REQ-016: replacement/removal stops old-connection login and renewal while existing access remains a five-minute snapshot | Refresh rows retain connection provenance; issuance locks and verifies the exact Active connection; successor rows copy provenance; access-token lifetime owner is unchanged | Provider-switch and connection-cutoff journeys; migration and refresh provenance tests | PASS |
| REQ-017: login, role changes, and replay containment use canonical redacted audit, with required audit failure failing closed | Role synchronization and session issuance share the tenant transaction; refresh-family containment appends canonical audit before the route commits the `Reused` result | Login/refusal audit journey cases, changed/unchanged role audit tests, audit rollback tests, ancestor replay containment proof | PASS |
| INV-001: headers, paths, browser state, email, and provider bytes cannot select effective tenant or connection | Typed route-key resolution precedes state creation; callback accepts only code/state; opaque state and the bound durable row select tenant/connection; no fallback selector exists | Header-hostility tests, replay/unknown-state cases, wrong-tenant redemption and same-email proofs | PASS |
| INV-002: stable human identity is exactly `(issuer, subject)` and email never links identity | Exact `sub` validation occurs at request and stored-row decode boundaries; `ensure_user_identity` receives verified issuer and subject; email lookup/uniqueness was removed | Exact-sub contract tests and provider-switch same-email journey | PASS |
| INV-003: platform, tenant-human, and workload authority planes remain separate | Tenant callback issues only tenant User sessions; platform login has its own pre-registered-principal path; workload assertions keep their existing binding path | Machine-independence journey plus platform/workload verifier coverage | PASS |
| INV-004: OIDC trust, SSRF, replay, audit, refresh containment, and RLS uncertainty fail closed | Screened provider client, consume-before-IO state, required claims, nonce/`azp`, forced RLS, connection-slot verification, transactional audit, and family-before-classification serialization are all on the production path | Refusal journey; fresh verifier test; recorded isolation and Postgres proofs; deterministic ancestor-overlap test | PASS |
| AC-002 task slice: controlled tenant login creates a User and proves allowed and denied calls | State-bound callback, identity/role resolution, session issuance, and sealed completion compose in the real server | `tenant_human_login_journey` | PASS |
| AC-003 task slice: tenant/provider separation and same-issuer isolation prevent cross-grant | Tenant-bound state, RLS, exact identity, and exact connection binding | `tenant_callback_refusal_journey`; `same_issuer_two_tenant_isolation_keycloak` | PASS |
| AC-005 task slice: machine credentials keep working with SSO and wrong workload bindings fail | Machine grant arms and generic workload verifier remain separate from OIDC ID-token semantics | `tenant_machine_independence_journey`; workload verifier cases | PASS |
| AC-006: provider switch cuts off old renewal, preserves recovery, applies mapping changes on next issuance, and does not email-link | Active-slot and refresh provenance enforcement plus exact issuer/subject identity | `tenant_provider_switch_journey` | PASS |
| AC-007 task slice: state/token/nonce/provider/audit/mapping/rotation/session-cutoff faults are exercised | Refusal and lifecycle branches stay on their established owners; R4 adds mandatory ID-token semantics and family serialization | Refusal/switch journeys, connection cutoff/rotation tests, focused verifier and overlap tests | PASS |
| Packet login contract: exactly one browser or known CLI binding, at least 256-bit opaque state, tenantless callback, fixed safe response, one-use sealed completion | Typed login request/response, initiation validation, random hashed state, durable binding, `AuthorizationCodeExchange`, and tenant-bound completion redemption | Login unit/Postgres tests, refusal journey, OpenAPI contract | PASS |
| Packet renewal contract: serialize a principal refresh family before classification and before the connection-slot lock; blocked old connections issue no successor | `RefreshTokens::execute` performs `refresh_by_hash` -> `lock_refresh_family` -> `consume_active_refresh`; issuer then locks/verifies the connection in the same caller-owned transaction | Same-row race, replay/provenance tests, provider-switch journey, `ancestor_replay_overlapping_rotation_revokes_successor` | PASS |
| Public authorization-code exchange retirement | Legacy authorization-code token grant, GET login/client helper, and CLI path are removed; callback returns no credential while API-key/workload/refresh grants remain | Grant deserialization refusal, callback no-token checks, OpenAPI/schema/codegen evidence | PASS |
| R1: exact `sub`, `azp`, role-change audit, advertised algorithm membership, RLS-only transitions, owner-bound state lookup, and PKCE redaction | Current contract, callback, SQL owner, audit, and `SecretString` source retain all seven corrections | R1 focused and journey evidence remains represented in the cumulative tests | PASS |
| R2: all cited Rust items have substantive docs and imports remain at module scope | Current cited items retain workflow/error/panic documentation and the SHA-256, Utoipa, and Wiremock imports remain module-level | Recorded `cargo doc`, lint, and focused inspection evidence | PASS |
| R3: algorithm-helper rustdoc describes only advertised-set membership and identifies the shared HMAC-rejection owner | `verify_id_token_algorithm` docs and body agree; `ExternalVerifier::verify_external_against` remains the asymmetric-policy owner | Source/caller inspection and advertised-HS256 refusal journey | PASS |
| R4: OIDC ID-token claims are mandatory and refresh replay cannot miss a concurrent successor | `verify_id_token_against` layers numeric non-future `iat` over required `exp`/`iss`/`aud` and present-`nbf` validation; refresh takes a tenant-qualified family lock before classification | Fresh verifier test; deterministic Postgres ancestor-overlap proof recorded green | PASS |
| R5: `refresh_by_hash` documentation reflects the current lookup-before-lock-before-classification workflow without executable change | Current rustdoc describes all lifecycle states, the production rotation order, and test-only post-revocation observation; commit `0ca117a74` changes only this rustdoc plus its task evidence | Direct body/caller inspection; cumulative `git diff --check`; recorded format and lint results | PASS |
| Constraint: reuse existing verifier, screened HTTP, audit, RLS, issuance, refresh, connection, and SQL owners without a second trust/persistence model | The implementation extends cohesive existing owners; family serialization uses PostgreSQL transaction advisory locking already available; no new runtime dependency or compatibility service was added | Cumulative diff and manifest inspection; recorded boundary checks | PASS |
| Constraint: validation, audit, tenancy, replay protection, and five-minute access semantics are not weakened | Remediations strengthen existing shared owners and preserve caller-owned transaction boundaries and token TTL | Focused and journey evidence above | PASS |
| Non-goals: no email linking, provider-token bearer authority, platform fallback, instant access-token revocation, new machine model, TASK-003 BFF route, or TASK-004 CLI persistence | No such path exists in the cumulative diff; browser redemption and CLI binding stop at the task-owned primitives | Source, public-contract, route, migration, and docs inspection | PASS |
| Authorized lead-directed reuse and test commits do not regress task behavior | Reuse stays on existing audit/verifier/test owners; test-only repairs preserve asserted behavior | Cumulative diff and recorded targeted test evidence | PASS |

## Proposed findings

No task-acceptance findings are proposed. The source-local finding ledger is
empty: no reachable task-owned `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or
`REGRESSION` remains in the immutable candidate.

The R5 task described two production `refresh_by_hash` callers, but current
source has one production rotation caller and test-only lifecycle-observation
callers. The implementation evidence explicitly records that correction, and
the updated rustdoc truthfully describes the reachable uses. Treating the
stale caller count as authority would require inventing a production revocation
caller, contrary to R5's documentation-only outcome and non-goals.

## Prior-finding closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` — mutable human subject mapping | Exact OIDC `sub` is required at request and stored decode boundaries | CLOSED |
| `FIND-TASK-002-2` — missing OIDC `azp` semantics | Callback enforces multi-audience and present-`azp` binding before persistence | CLOSED |
| `FIND-TASK-002-3` — missing role-change audit | Changed role sets append canonical audit transactionally; unchanged sets do not | CLOSED |
| `FIND-TASK-002-4` — provider algorithm not enforced | Fresh advertised membership precedes shared asymmetric signature verification | CLOSED |
| `FIND-TASK-002-5` — duplicate tenant predicates | Login-state transitions rely on forced RLS | CLOSED |
| `FIND-TASK-002-6` — raw pool state lookup | `WyrdPostgres::login_state_tenant` owns the narrow privileged lookup | CLOSED |
| `FIND-TASK-002-7` — printable PKCE verifier | Durable verifier storage remains `SecretString` with redacted Debug | CLOSED |
| `FIND-TASK-002-8` — incomplete Rust documentation | The cited item inventory retains substantive workflow and error/panic docs | CLOSED |
| `FIND-TASK-002-9` — function-scoped imports | Cited imports remain in module import blocks | CLOSED |
| `FIND-TASK-002-10` — false algorithm-helper rustdoc | Helper docs match its advertised-membership-only role | CLOSED |
| `FIND-TASK-002-11` — optional OIDC binding/time claims | Shared ID-token verification now requires issuer/audience/expiry and numeric non-future issuance time while preserving workload semantics | CLOSED |
| `FIND-TASK-002-12` — ancestor replay can miss a concurrent successor | Family serialization precedes classification and remains held through rotation or containment commit | CLOSED |
| `FIND-TASK-002-13` — stale refresh lookup ordering rustdoc | Current docs state lookup, family lock, then classification and accurately identify test-only lifecycle observation | CLOSED |

## Verification limits

- I inspected the complete cumulative diff, the original task, all five
  remediation tasks, current relevant bodies and callers, route commit
  behavior, contracts, SQL, migration, and focused tests. No `.codegraph/`
  index exists, so Git, `rg`, and direct source inspection were used.
- Fresh in this review: `mise exec -- cargo nextest run --locked -p
  wyrd-auth-verify --lib -E
  'test(=tests::oidc_id_token_requires_binding_and_time_claims)'` passed 1/1;
  cumulative `git diff --check` passed.
- Recorded candidate evidence reports the deterministic refresh-overlap proof,
  all four required identity journeys, full identity lane (27/27), principals
  unit/integration, SQL, tenant isolation, codegen/docs, format, lints, and diff
  hygiene passing. I did not rerun Docker, Keycloak/Dex, Postgres, or broad
  workspace lanes in this bounded review.
- TASK-003 BFF completion, TASK-004 CLI handoff persistence/local credential
  storage, and live Okta/Entra qualification remain intentional non-goals.

## Overall result

**PASS**

The immutable cumulative candidate satisfies the original TASK-002 obligations
and R1 through R5, closes `FIND-TASK-002-1` through
`FIND-TASK-002-13`, preserves the explicit constraints and non-goals, and
introduces no task-owned scope drift requiring correction.
