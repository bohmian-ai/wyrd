# TASK-002 R5 Task Implementation Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `b57d43d501c136591125b98fe78352e657b093b6`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation tasks: TASK-002-R1, TASK-002-R2, TASK-002-R3, and TASK-002-R4 in the prior review directories
- Reviewed range: complete cumulative `base..candidate` diff, 86 files, 8,646 insertions and 1,839 deletions

The candidate was `b57d43d501c136591125b98fe78352e657b093b6` before source inspection, after the focused check, and immediately before this report was written. Review artifacts are outside the immutable subject. Lead-directed reuse and test commits recorded separately in the task evidence were treated as authorized and were still checked for regression.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006: the route key is pre-login context only; begin selects one Active tenant connection and the common callback derives tenant and connection only from bounded one-use state | `wyrd-auth/src/login.rs::HumanConnections::begin_login`; `wyrd-auth/src/callback.rs:98-156`; `wyrd-sql/src/postgres.rs:154-180`; deployment-controlled redirect construction in the login owner | `tenant_human_login_journey`; `tenant_callback_refusal_journey`; `tenant_login_operations_publish_their_contract`; recorded identity lane 27/27 | PASS |
| REQ-007: code + PKCE, state, nonce, redirect, issuer, audience, algorithm, key, time, claims, and screened bounded provider IO fail closed | Callback consumes state before IO and rechecks the bound connection; `callback.rs:201-220` checks advertised algorithm, shared ID-token verification, nonce, and `azp`; `wyrd-auth-verify/src/lib.rs:508-601` requires `exp`/`iss`/`aud`, validates a present `nbf`, verifies signature/JWKS, and requires numeric non-future `iat` | Fresh `tests::oidc_id_token_requires_binding_and_time_claims` passed; recorded `tenant_callback_refusal_journey`, screening tests, algorithm/`azp` tests, and four required journeys passed | PASS |
| REQ-008: successful login creates/resolves a tenant User by exact `(issuer, sub)`, maps only current tenant roles, and gives an unmapped subject no privileged grant | Human input and stored-row decode require `subject == "sub"` (`pg_resolvers.rs:300-334,513-540`); callback identity/role path is `callback.rs:228-245`; no default role participates | `tenant_human_login_journey`; provider-switch same-email case; callback role-mapping and audit rollback tests | PASS |
| REQ-013: API-key and exact workload-assertion machine paths remain independent of human SSO | Human callback and refresh use User-only owners; workload exchange remains on `ExternalVerifier::verify_external` rather than the ID-token semantic entry; API-key and JWT-bearer grant arms remain distinct | `tenant_machine_independence_journey`; verifier proof confirms workload assertions need no OIDC `iat`; recorded workload journey passed | PASS |
| REQ-014: tested replacement preserves the current provider until activation and never transfers authority or links by email | Login state binds connection id/revision; callback and refresh re-read the Active slot; identity key remains issuer + subject; the migration removes tenant-email uniqueness | `tenant_provider_switch_journey`; connection lifecycle journeys and same-email/different-subject proof | PASS |
| REQ-015: one service can authenticate multiple tenants through separate connections without cross-tenant grants | State-to-tenant resolution reveals only the tenant needed to open `TenantConn`; all subsequent state, identity, role, and refresh work stays behind forced RLS | `same_issuer_two_tenant_isolation_keycloak`; `pg_login_state` cross-tenant tests; recorded `check:tenant-isolation` | PASS |
| REQ-016: replacement/removal stops old-connection renewal immediately while already issued access remains a five-minute snapshot | Refresh rows carry connection id/revision; `refresh.rs:193-218` delegates renewal to the issuer, whose connection-slot lock checks the exact binding; successor rows retain provenance | `tenant_provider_switch_journey`; `tenant_connection_session_cutoff_journey`; migration and refresh provenance tests | PASS |
| REQ-017: login and authority changes use canonical redacted transactional audit and fail closed when required audit cannot append | Callback appends role-sync only on a changed set and issues/audits in the same `TenantConn`; refresh containment appends through the canonical auth audit path before the route commits `Reused` (`routes.rs:231-244`) | Human-login and refusal journey audit cases; role-sync changed/unchanged/rollback tests; ancestor replay test asserts exactly one attributed containment event | PASS |
| INV-001: Host, forwarded headers, browser state, email, and provider bytes cannot select effective tenant or connection | Begin accepts the typed route key; callback accepts only code/state; opaque state lookup and bound row select tenant/connection; exact `sub` is the identity key | Header-hostility tests, wrong-tenant callback refusal, same-email replacement proof | PASS |
| INV-002: external identity is exactly `(issuer, subject)`; email is display data only | Exact `sub` validation at request and stored decode boundaries; `ensure_user_identity` receives the verified issuer and subject; email uniqueness/link lookup was removed | Exact-sub tests and provider-switch same-email separate-User journey | PASS |
| INV-003: platform, tenant-human, and workload authority planes remain distinct | Tenant callback issues only tenant User sessions; platform login uses its own pre-registered principal path; workload assertions stay on the generic external-verification entry | Machine-independence journey; platform and workload verifier tests | PASS |
| INV-004: trust, SSRF, replay, audit, and RLS uncertainty fails closed | Screened provider client, consume-before-IO state, required ID-token claims, nonce/`azp`, forced RLS, connection-slot checks, transactional audit, and family serialization are all on the production path | Refusal journey; fresh verifier test; recorded tenant-isolation and Postgres auth tests; `ancestor_replay_overlapping_rotation_revokes_successor` | PASS |
| AC-002 task slice: controlled tenant login proves User creation plus allowed and denied calls | State-bound callback, User resolution, mapped roles, issuance, and sealed completion compose in the real server | `tenant_human_login_journey` | PASS |
| AC-003 task slice: two tenants/providers and same-issuer isolation cannot cross-grant | Tenant-bound state, RLS, issuer/subject identity, and exact connection binding | Refusal journey plus `same_issuer_two_tenant_isolation_keycloak` | PASS |
| AC-005 task slice: workload assertion and machine credentials retain exact issuer/subject/audience/tenant binding under SSO | Machine token arms were preserved; generic verification now additionally requires issuer/audience presence and validates a present `nbf` without imposing ID-token `iat` | `tenant_machine_independence_journey`; workload verifier cases | PASS |
| AC-006: provider replacement cuts off old refresh, preserves owner recovery, applies mapping changes on next issuance, and does not email-link | Connection provenance and Active-slot locking; exact issuer/subject identity; roles recomputed in shared issuer | `tenant_provider_switch_journey` | PASS |
| AC-007 task slice: state/token/nonce/provider/audit/mapping/rotation/session-cutoff faults are covered | All refusal and lifecycle branches remain on the owners above; R4 closes missing ID-token claims and overlapping family containment | `tenant_callback_refusal_journey`; `tenant_provider_switch_journey`; connection rotation/cutoff tests; focused verifier and ancestor-overlap tests | PASS |
| Packet-local begin contract: exactly one browser hash or known CLI handoff, at least 256-bit opaque state, bounded durable binding | Typed begin request/response, initiation validation, random hashed state, durable tenant/connection/revision/redirect/PKCE/nonce/expiry row | Login unit/Postgres tests and served OpenAPI contract | PASS |
| Packet-local callback/completion contract: callback has no tenant selector, consumes before provider IO, returns no token JSON or provider code, and seals one-use completion | `AuthorizationCodeExchange` owner; fixed browser completion redirect/generic CLI result; sealed completion row and tenant-bound redemption | Refusal journey, callback unit/Postgres tests, OpenAPI contract | PASS |
| Packet-local refresh contract: family and Active connection serialize in one tenant transaction; replay containment covers concurrent descendants; blocked old connection issues no successor | `refresh.rs:121-226`; `refresh_tokens.rs:27-30,96-120`; fixed family-then-connection order; route commits the `Reused` containment transaction | `ancestor_replay_overlapping_rotation_revokes_successor`; same-row race, replay, provenance, and switch tests | PASS |
| Public authorization-code exchange retirement: callback is the only code-exchange owner; API-key, workload, and refresh grants remain | `TokenRequest::AuthorizationCode` and legacy GET login/client/CLI path are removed; callback contract returns no credential | `authorization_code_grant_is_retired`; refusal journey; OpenAPI/schema/codegen evidence | PASS |
| Constraint: reuse existing issuer, verifier, screened HTTP, audit, RLS, issuance, refresh, and connection owners; no second trust or persistence model | The cumulative implementation extends the existing concrete owners. R4 adds one semantic verifier method and one SQL advisory-lock query in the existing owners; no new dependency, service, table, trait, or compatibility layer | Cumulative diff inspection; prior review closure; recorded boundary checks | PASS |
| Constraint: security validation, audit, tenancy, replay protection, and five-minute access semantics must not be weakened | R4 strengthens required claims and serializes the family before classification; issuance and access TTL owners are unchanged | Focused verifier test, ancestor-overlap test, four journeys, tenant-isolation and principals lanes | PASS |
| Non-goal: no email linking, provider-token bearer authority, platform fallback, instant access-token revocation, second machine model, TASK-003 BFF completion, or TASK-004 CLI handoff persistence | No such path appears in the cumulative diff. CLI handoff is deliberately refused until its owner exists; browser redemption is exercised through the auth owner, not a TASK-003 route | Diff/source inspection and task evidence limits | PASS |
| Non-goal: no speculative abstraction, dependency, configuration, compatibility surface, or duplicate verifier/lock service | The minimal existing-owner corrections use `jsonwebtoken` validation and PostgreSQL transaction advisory locks already available in the repository | R4 diff inspection; Cargo manifests unchanged by R4 | PASS |

## Proposed findings

No task-acceptance findings are proposed. The source-local finding ledger is empty: there is no validated `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION` with a reachable task-owned consequence. Consequently, finding ID, violated obligation, exact defect location, observable consequence, and required correction fields are not applicable.

## Prior-finding closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` — mutable human subject mapping | Request validation and stored decode require exact OIDC `sub`; same-email replacement remains a separate User | CLOSED |
| `FIND-TASK-002-2` — missing OIDC `azp` semantics | `callback.rs:219` calls the dedicated authorized-party check before persistence | CLOSED |
| `FIND-TASK-002-3` — missing provider-driven role-change audit | Changed role sets append one canonical `auth.user.roles.sync` event in the issuance transaction; unchanged sets do not | CLOSED |
| `FIND-TASK-002-4` — provider-advertised algorithm not enforced | Callback checks fresh advertised membership, then the shared verifier rejects HMAC and verifies JWKS/signature | CLOSED |
| `FIND-TASK-002-5` — duplicate tenant predicates on login state | Tenant state transitions rely on forced RLS; the narrow global lookup remains least-disclosure only | CLOSED |
| `FIND-TASK-002-6` — raw pool propagated for state lookup | `WyrdPostgres::login_state_tenant` owns the narrow cross-tenant lookup over its private pool | CLOSED |
| `FIND-TASK-002-7` — printable PKCE verifier | Durable login-state values retain `SecretString` and redacted Debug behavior | CLOSED |
| `FIND-TASK-002-8` — incomplete required rustdoc | The prior exact item inventory remains documented, including fallible/panic contracts | CLOSED |
| `FIND-TASK-002-9` — function-scoped imports | The cited SHA-256, Utoipa, and Wiremock imports remain at module scope | CLOSED |
| `FIND-TASK-002-10` — false algorithm-helper rustdoc | Current helper rustdoc describes advertised-set membership and correctly names the shared verifier as HMAC owner | CLOSED |
| `FIND-TASK-002-11` — missing ID-token binding/time claims | `verify_external_against` requires `exp`/`iss`/`aud` and validates present `nbf`; `verify_id_token_against` requires numeric non-future `iat`; both tenant and platform ID-token callers use it; workload stays generic | CLOSED |
| `FIND-TASK-002-12` — ancestor replay can miss a concurrent successor | Every stored principal family is transaction-lock serialized before active/stale classification; the route holds it through rotation/containment commit; overlap proof leaves successor revoked and unusable | CLOSED |

## Verification limits

- I inspected the complete cumulative diff, all four remediation tasks and prior verdict/finding ledgers, the R4 production changes, their complete relevant bodies and callers, route commit behavior, and focused tests. No `.codegraph/` index exists, so Git, `rg`, and direct source inspection were used.
- Fresh in this review: `mise exec -- cargo nextest run --locked -p wyrd-auth-verify --lib -E 'test(=tests::oidc_id_token_requires_binding_and_time_claims)'` passed (1/1); cumulative `git diff --check` passed.
- Recorded candidate evidence reports the focused PostgreSQL ancestor-overlap proof, all four required identity journeys, full identity lane (27/27), principals unit/integration, SQL, tenant-isolation, format, lints, and diff hygiene passing. I did not rerun Docker/Keycloak/Postgres or broad workspace lanes in this bounded review.
- The journey carries the missing-`iat` refusal end to end; the shared-owner unit proof covers missing issuer/audience, malformed/future `iat`, future `nbf`, valid ID tokens, and unchanged workload semantics. This is credible because both production OIDC callers use that shared owner before identity or issuance.
- TASK-003 BFF completion, TASK-004 CLI handoff persistence/local credential storage, and live Okta/Entra qualification remain intentionally outside TASK-002.

## Overall result

**PASS**

The immutable cumulative candidate satisfies the original TASK-002 obligations and all four remediation tasks, closes prior findings 1 through 12, preserves the explicit constraints and non-goals, and introduces no task-owned scope drift requiring correction.
