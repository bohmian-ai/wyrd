# TASK-002 Task Implementation Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `8b201627c0a957dccf46649d00c8c205689bc5de`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-002-r1/TASK-002-R1-tenant-login-corrections.md`
- Reviewed range: complete cumulative base-to-candidate diff (59 files, 5,458 insertions, 1,688 deletions), including the prior verdict and validated findings solely to assess closure.

## Overall result: PASS

The cumulative candidate satisfies the original tenant-login task and closes all seven prior findings. The callback derives tenant and connection only from one-use state, verifies the provider response under the exact human-login policy, resolves immutable `(issuer, sub)` identity, synchronizes and audits tenant roles atomically, seals the issued Wyrd session for one-use redemption, cuts renewal off at connection replacement, and leaves machine authentication independent. No material missing, incorrect, drifted, violating, or regressed behavior was found.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006: route key is routing context only; the common callback derives tenant/connection solely from one-use state and returns only the fixed browser redirect or generic CLI page | `wyrd-auth/src/login.rs:51-132`; `wyrd-auth/src/callback.rs:96-168`; `wyrd-server/src/components/auth/routes.rs:322-355` | `login_ignores_request_headers`; `the_host_header_cannot_select_a_tenant`; `tenant_human_login_journey`; OpenAPI contract | PASS |
| REQ-007: PKCE, state, nonce, exact redirect, bounded one-time state, verified signature/issuer/audience/authorized party/algorithm/key/time/claims, and screened provider IO | `login.rs:65-129`; consume-before-IO at `callback.rs:130-168`; algorithm, verifier, nonce, and `azp` checks at `callback.rs:200-219,571-620`; screened discovery/token calls at `callback.rs:323-449` | `tenant_callback_refusal_journey`; `verify_authorized_party_enforces_azp_for_the_client`; `a_multi_audience_token_needs_azp_naming_the_client`; `an_unadvertised_id_token_algorithm_is_refused`; screening tests | PASS |
| REQ-008: exact provider provisions a tenant User by verified `(issuer, sub)`; only mapped current tenant roles grant authority; unmapped subjects have no privileged default | Exact `sub` validation in `wyrd-spec/src/auth/human_connection.rs:204-253`; stored-row fail-closed decode in `wyrd-auth/src/pg_resolvers.rs:522-558`; identity and role resolution at `callback.rs:227-255` | `human_subject_must_be_exactly_sub`; `stored_human_connection_requires_the_sub_subject_claim`; `same_email_different_subjects_are_distinct_users`; `tenant_human_login_journey` | PASS |
| REQ-013: Service/Agent API keys and exact workload assertions remain independent of human SSO | Existing API-key and JWT-bearer token arms remain distinct; human-only refresh provenance is enforced in `wyrd-auth/src/refresh.rs` and `issuance.rs` | `tenant_machine_independence_journey`; existing workload/API-key journey coverage | PASS |
| REQ-014: a tested replacement activates atomically, does not inherit prior identity authority, and never links by email | Callback and refresh require exact connection id/revision; identity key is issuer plus exact `sub`; migration removes tenant-email uniqueness | `tenant_provider_switch_journey`; `same_email_different_subjects_are_distinct_users`; TASK-001 connection activation journeys | PASS |
| REQ-015: tenant membership and authority never cross tenants, including shared issuers | Narrow state-hash tenant resolution on `WyrdPostgres` at `wyrd-sql/src/postgres.rs:150-181`; every subsequent state and identity operation uses tenant RLS | Same-issuer cross-tenant step in `tenant_callback_refusal_journey`; `same_issuer_two_tenant_isolation_keycloak`; `login_state_tenant_names_only_the_owner_of_a_pending_state` | PASS |
| REQ-016: old connection stops new login and renewal; successors preserve provenance; access snapshots remain at most five minutes; changed mappings apply on the next admitted login/issuance | Active binding rechecked before callback issuance and under connection-slot lock; refresh rows carry exact connection provenance; migration revokes unbound legacy User rows | `tenant_provider_switch_journey`; `tenant_connection_session_cutoff_journey`; migration and refresh tests | PASS |
| REQ-017: login outcomes and provider-driven role changes produce redacted canonical audit evidence; required audit failure establishes no session | Role replacement reports mutation and appends `auth.user.roles.sync` in the issuance transaction at `callback.rs:235-265`; token-exchange audit remains in the shared issuer; refusals use the canonical best-effort path | `changed_roles_are_audited_once_and_unchanged_roles_never`; `a_failed_role_sync_audit_rolls_back_the_whole_login`; callback journey audit-failure step | PASS |
| INV-001: paths, hosts, headers, browser state, email, and unverified provider material cannot choose effective tenant/connection after login begins | Begin accepts only route key plus initiation binding; callback hashes opaque state and performs a one-column owner lookup; no header or email fallback exists | Header-hostility, unknown/replayed state, and wrong-tenant redemption cases | PASS |
| INV-002: stable identity is `(issuer, sub)` and email never links identity | Exact `sub` enforced both at input and stored decode; `ensure_user_identity` uses issuer and verified subject; email uniqueness removed | Public/stored validation tests; same-email/different-subject and provider-switch proofs | PASS |
| INV-003: platform, tenant-user, and workload planes remain distinct | Human callback issues `User`; platform and workload verification paths were not merged or reused as tenant fallback | Human login and machine-independence journeys | PASS |
| INV-004: discovery is metadata rather than trust; SSRF/DNS pinning, algorithm/key checks, replay protection, audit, and RLS fail closed | Screened/pinned HTTP remains the only provider IO; advertised asymmetric algorithm is required before verification; state is consumed atomically; login-state transitions rely on forced RLS | Refusal journey, callback screening tests, advertised-algorithm tests, login-state Postgres isolation tests | PASS |
| AC-002 task slice: real-provider tenant login creates a User, maps a role, permits one call, and denies one | Complete begin/callback/redemption and tenant issuance flow | `tenant_human_login_journey` | PASS |
| AC-003 task slice: concurrent tenants and same-issuer/wrong-tenant callbacks remain isolated | State-only callback tenant resolution and tenant-bound redemption | `tenant_callback_refusal_journey`; same-issuer isolation journey | PASS |
| AC-005 task slice: machine paths continue with SSO and wrong issuer/subject/audience/tenant fail | Machine grant paths remain unchanged and separate | `tenant_machine_independence_journey` | PASS |
| AC-006: replacement preserves recovery and creates a separate same-email User without inherited authority | Exact connection cutoff and `(issuer, sub)` identity | `tenant_provider_switch_journey` | PASS |
| AC-007 task slice: state/token/provider faults, outage, unsafe destination, inactive connection, audit failure, mapping change, and old-session cutoff fail closed | Shared callback, screened IO, atomic audit/issuance, and connection-bound refresh owners | Refusal/switch/cutoff journeys plus narrow screening, algorithm, `azp`, audit, and sealing-key tests | PASS |
| Packet-local contract: exactly one browser/CLI initiation binding; at least 256-bit opaque state; callback has no tenant selector; consume precedes provider IO; completion is sealed and one-use | `BeginLogin::initiation`; 32-byte random state; `LoginState`; `consume_login_state`; fixed callback responses and `redeem_completion` | Binding-shape, replay, redirect/no-token, Debug-redaction, and Postgres one-use tests | PASS |
| Packet-local renewal: new OIDC refresh rows require connection provenance; legacy unbound rows are revoked; active id/revision is locked and rechecked; successor retains provenance | Refresh migration, `RefreshTokens::execute`, and `TenantTokenIssuer::issue_human_session` | Provider-switch, session-cutoff, refresh, and migration tests | PASS |
| Required retirement: `/auth/token` no longer accepts authorization code; callback never emits tokens; API-key/workload/refresh grants stay; no compatibility route remains | `TokenRequest::AuthorizationCode`, legacy client/CLI login helper, and GET login route removed; callback returns redirect/static HTML only | `authorization_code_grant_is_retired`; refusal journey; OpenAPI and generated schema checks | PASS |
| Non-goals and Ponytail scope: no provider token as Wyrd authority, email linking, platform fallback, instant-revocation promise, new machine model, new dependency, alternate audit path, or speculative compatibility abstraction | Existing verifier, issuer, screened HTTP, SQL/RLS, refresh, audit, and sealing owners are reused; the remediation deletes duplicate state types and raw-pool query exposure | Complete diff and caller inspection; recorded boundary/lint/docs checks | PASS |

## Prior-finding closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| FIND-TASK-002-1: configurable human subject could replace OIDC `sub` | Exact `sub` is required at the public input and stored-row decode boundaries; same-email/different-subject proof exists | CLOSED |
| FIND-TASK-002-2: missing OIDC `azp` validation | `verify_authorized_party` rejects missing `azp` for multiple audiences and every mismatched present `azp` before persistence | CLOSED |
| FIND-TASK-002-3: role replacement lacked role-change audit | `replace_user_roles` returns whether membership changed; exactly one canonical role-sync event is appended on change in the same transaction, and audit failure rolls everything back | CLOSED |
| FIND-TASK-002-4: untrusted header selected the ID-token algorithm | Callback requires the header algorithm to be in fresh discovery's supported asymmetric set before shared verification | CLOSED |
| FIND-TASK-002-5: TenantConn state transitions duplicated tenant predicates | Transition SQL relies on forced RLS without manual tenant selection; live Postgres tests cover purge/consume/complete/redeem isolation | CLOSED |
| FIND-TASK-002-6: state owner lookup exposed raw `PgPool` | Lookup is a narrow inherent `WyrdPostgres::login_state_tenant` operation over its private app pool | CLOSED |
| FIND-TASK-002-7: PKCE verifier leaked through derived `Debug` | `LoginState.code_verifier` is `SecretString`; only SQL binding and provider exchange expose it; sentinel Debug test passes | CLOSED |

## Proposed findings

None.

## Verification notes and limits

- Independently inspected the complete cumulative diff and the current implementations/callers for connection input/decode, login initiation, callback exchange, provider verification, identity resolution, role synchronization, audit, state persistence/RLS, completion redemption, refresh rotation, public routes/contracts, migrations, generated schemas, documentation, and the four required identity journeys.
- Re-ran the exact focused tests `auth::human_connection::tests::human_subject_must_be_exactly_sub`, `queries::auth::login_state::tests::initiation_columns_round_trip_and_refuse_ambiguity`, and `queries::auth::login_state::tests::login_state_debug_redacts_the_pkce_verifier`; all three passed. `git diff --check` for the cumulative range also passed.
- The candidate records successful focused Postgres callback/remediation tests and the broader `test:identity:journey` (27/27), principals, SQL, codegen, tenant-isolation, pool-boundary, client-tier, formatting, lint, and diff checks in the remediation evidence. Those expensive lanes were not rerun during this bounded review.
- TASK-003 still owns the browser `/login/complete` route and TASK-004 still owns server-issued CLI handoffs. This candidate correctly refuses every unknown CLI handoff and exposes only the shared sealed-redemption primitive; those deferred owners are not TASK-002 acceptance gaps.
- Candidate identity remained `8b201627c0a957dccf46649d00c8c205689bc5de` throughout this review.
