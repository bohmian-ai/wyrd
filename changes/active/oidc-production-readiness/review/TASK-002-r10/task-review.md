# TASK-002 R10 Task Implementation Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `6e21d8ed00d5159ec71e3f2e2414e80fd16f76af`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation inputs: TASK-002 R1 through R9, including each remediation task,
  verdict, and validated finding ledger

The candidate was checked before inspection, after fresh focused verification,
and immediately before this report was written. It remained the named commit.
The repository has no `.codegraph/` directory, so the review used Git, `rg`,
and direct source and caller inspection.

## Result

**PASS**

The complete base-to-candidate implementation satisfies the original task and
closes stable findings `FIND-TASK-002-1` through `FIND-TASK-002-22`. The R9
candidate correction moves callback role replacement behind the existing
tenant-qualified User refresh-family lock, so the callback's mapped role set,
role-change audit, issued access token, refresh row, and sealed completion are
one serialized transaction. Its focused Postgres proof passed freshly. The
remaining R9 corrections align the shared token schema with runtime behavior,
restore scrubbed write-handler tracing and substantive router documentation,
and keep begin-login's application-pool selection inside `WyrdPostgres`.

No missing behavior, incorrect behavior, scope drift, violated task constraint,
or regression was found. The lead-authorized reuse cleanups and gateway/Forge
test repairs recorded by the R2/R3 review authority remain behavior-preserving
and are not task-scope findings.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006 / INV-001: the route key is untrusted pre-login routing context; the callback derives tenant and connection only from deployment-controlled, one-use state | `HumanConnections::begin_login` resolves only the typed route slug through `WyrdPostgres::resolve_tenant_slug`, stores the connection revision, issuer, client, fixed redirect, PKCE verifier, nonce, and initiation; `AuthorizationCodeExchange::execute` resolves only the state hash through `WyrdPostgres::login_state_tenant` and consumes it before provider IO | `login::pg_tests::unknown_tenant_and_no_connection_are_indistinguishable`; `tenant_callback_refusal_journey`; `same_issuer_two_tenant_isolation_keycloak`; recorded pool/tenant boundary checks | PASS |
| REQ-007 / INV-004: authorization code, PKCE, nonce, exact redirect, bounded one-use state, screened provider IO, and full fail-closed ID-token verification | `AuthorizationCodeExchange::{complete,finish_id_token_exchange}`; `verify_id_token_algorithm`; `verify_authorized_party`; shared `ExternalVerifier::{verify_external_against,verify_id_token_against}` require advertised asymmetric verification, signature/key, issuer, audience, expiry, valid present `nbf`, numeric non-future `iat`, valid OIDC `sub`, nonce, and `azp` before persistence | `tenant_callback_refusal_journey`; focused advertised-HS256/`azp` callback proofs; fresh `tests::oidc_id_token_requires_binding_and_time_claims` (1 passed); prior unsafe-destination and PKCE-redaction proofs | PASS |
| REQ-008 / INV-002 / INV-003: exact tenant `(issuer, sub)` User identity, email never links, mapped current tenant roles only, and no privileged default | Human connection input and stored decode require literal `sub`; `ensure_user_identity` uses only issuer and verified subject; `role_names_to_refs` maps verified groups to configured tenant roles and ignores defaults; `replace_user_roles` owns the exact durable set | `tenant_human_login_journey`; `tenant_provider_switch_journey`; distinct-subject/same-email callback proof; unmapped-subject and changed/unchanged role-sync proofs | PASS |
| REQ-008 / REQ-017 concurrency and atomicity: concurrent callbacks cannot combine provider authority, and role change, audit, issuance, and completion commit together | `finish_id_token_exchange` takes `lock_refresh_family(&mut conn, "user", principal_id)` immediately after canonical identity resolution and before role replacement, retains the same `TenantConn` through role-sync audit, `issue_human_session`, completion, and commit; `issue_human_session` preserves family-before-connection order | Fresh `auth::callback::pg_tests::concurrent_callbacks_replace_roles_without_union` passed under repository-managed Postgres; injected role-sync audit-failure proof records rollback | PASS |
| REQ-013 / AC-005: Service and Agent API-key and exact workload-assertion paths remain independent of human SSO | Machine grants remain separate `TokenRequest` variants and use existing issuance paths; only `issue_human_session` supplies a refresh token; `TokenResponse` source and generated schemas now state that every machine grant re-presents its durable credential | `tenant_machine_independence_journey`; `workload_jwt_bearer_journey_keycloak`; recorded `codegen:check` | PASS |
| REQ-014 / AC-006: a tested replacement can activate without inherited authority or email linking | Callback and refresh both bind the exact connection id/revision; identity remains tenant `(issuer, sub)`; same-email replacement subjects create separate Users | `tenant_provider_switch_journey` proves tested activation, old-refresh refusal, separate same-email User, mapping update, and owner recovery | PASS |
| REQ-015 / AC-003: tenant authentication and authority remain isolated, including shared issuers | Login state is tenant-owned under forced RLS; the cross-tenant callback lookup reveals only the owning tenant id; all subsequent identity, role, refresh, and audit operations use that tenant's `TenantConn` | `same_issuer_two_tenant_isolation_keycloak`; `tenant_callback_refusal_journey`; login-state cross-tenant Postgres proof; recorded `check:tenant-isolation` | PASS |
| REQ-016: old-connection login and renewal stop immediately; access-token authority remains the existing five-minute snapshot; next issuance uses current mappings | Callback rechecks the exact active connection, while human issuance and refresh serialize on family then connection-slot locks and persist connection provenance on every successor; the migration revokes provenance-free human refresh rows | `tenant_provider_switch_journey`; `tenant_connection_session_cutoff_journey`; deterministic replay/rotation, admin-revocation/rotation, initial-issuance/revocation, and connection-deactivation/rotation overlap proofs | PASS |
| REQ-017 / AC-007: login outcomes, provider role changes, issuance, replay containment, and revocation use canonical redacted audit; required audit failure establishes no session | Changed role sets append `auth.user.roles.sync` and issuance appends the canonical token event in the same caller-owned transaction; refusal and lifecycle paths retain their canonical audit owners; secret-bearing handler arguments are skipped by tracing | `tenant_human_login_journey`; `tenant_callback_refusal_journey`; changed/unchanged role audit and injected audit-failure proofs; refresh/revocation concurrency proofs | PASS |
| Packet-local callback and completion contract: callback accepts no tenant selector, emits no code or Wyrd token, and stores a sealed one-use completion for the bound Browser or CLI initiation | `CallbackQuery` contains only code/state; callback returns the fixed same-origin completion redirect or generic CLI page; login completion is sealed and redeemed against the stored initiation; no token-bearing callback response remains | `tenant_callback_refusal_journey`; callback route tests; generated callback schemas and `tenant_login_operations_publish_their_contract` | PASS |
| Retire public authorization-code token exchange and its bypassing clients while retaining API-key, workload, delegation, and refresh grants | `TokenRequest::AuthorizationCode`, the public client helper, CLI login/paste path, and old login route are absent; `/auth/token` retains only the approved non-code grants | `auth::token::tests::authorization_code_grant_is_retired`; OpenAPI contract proof; schema/codegen evidence; CLI/docs diff inspection | PASS |
| Public contract, server ownership, SQL capability, secret handling, and observability constraints | Typed contracts live in `wyrd-spec`; durable workflows remain in Rust owners; login uses `WyrdPostgres::resolve_tenant_slug` rather than propagating a raw pool; login state holds the verifier as `SecretString`; `POST /auth/token` has `#[tracing::instrument(level = "debug", skip_all)]` | Recorded `codegen:check`, `check:from-pools-allowlist`, `check:tenant-isolation`, `check:client-tier`, format, lints, and docs checks; direct R10 inspection | PASS |
| AC-002 task slice: deployed controlled-provider login provisions a User, maps an allowed role, and proves a denial and zero-default grants | Real-server login journey drives the public begin/callback contract and redeems the owner completion before authorization checks | `tenant_human_login_journey` in the recorded 27/27 identity lane | PASS |
| AC-003 task slice: two tenant/provider configurations, wrong-tenant callback refusal, and same-issuer isolation | Two controlled Keycloak realms plus Dex are configured by the identity lane; tenant and issuer bindings remain independent | `tenant_callback_refusal_journey`, `tenant_provider_switch_journey`, and `same_issuer_two_tenant_isolation_keycloak` | PASS |
| AC-005 task slice: machine credentials remain exact and independent with SSO enabled | Human changes do not alter API-key or workload verification/issuance owners | `tenant_machine_independence_journey` | PASS |
| AC-006 task slice: provider replacement neither links by email nor transfers prior authority | Exact subject identity and active connection revision checks precede role persistence and issuance | `tenant_provider_switch_journey` | PASS |
| AC-007 task slice: outage, unsafe endpoints, invalid token/claims, state replay/expiry, inactive connection, unsupported auth, audit failure, mapping change, rotation, and multi-replica durable behavior have proportionate proof | Shared screened HTTP, verifier, state, connection, transaction, audit, family-lock, and sealing owners cover the refusal boundaries; branches infeasible with local real providers remain focused unit/Postgres proofs as the task permits | `tenant_callback_refusal_journey`, provider-switch and session-cutoff journeys, callback/Postgres refusal tests, verifier tests, migration tests, and recorded full identity lane | PASS |
| Explicit non-goals: no provider-token bearer authority, email linking, platform fallback, instant-revocation promise, new machine identity model, compatibility route, TASK-003 BFF route, or TASK-004 handoff implementation | Provider tokens terminate at the callback verifier; platform and tenant planes remain distinct; docs describe bounded access snapshots; `known_initiation` intentionally refuses CLI until TASK-004 supplies the handoff and no BFF completion route is added here | Complete diff and caller inspection; machine/provider-switch journeys; prior R1-R9 validation ledgers | PASS |
| Prior stable findings `FIND-TASK-002-1` through `FIND-TASK-002-22` | Each diagnosed owner now has the required exact-sub, `azp`, role audit, advertised-algorithm, RLS, pool-boundary, secret-redaction, documentation/import, claims, refresh-family serialization, subject-syntax, callback-role serialization, schema, tracing, and router-contract correction | R1-R9 closure evidence plus fresh R10 verifier and callback-concurrency tests | PASS |

## Proposed findings

None. The validated task-review finding ledger is empty.

## Verification notes and limits

- Fresh R10 checks passed: cumulative `git diff --check`; exact
  `wyrd-auth-verify` OIDC binding/time/subject test; and the exact
  repository-managed Postgres callback-concurrency test. The Postgres wrapper
  also reran both migration idempotence targets successfully.
- The candidate records successful R1-R9 focused proofs,
  `test:principals:unit`, `test:principals:integration`, `test:sql`, all 27
  identity journeys, codegen, docs, tenant/pool/client boundary checks,
  formatting, lints, and cumulative diff hygiene. This reviewer did not rerun
  every broad or Docker/IdP-backed lane.
- Live Okta/Entra production qualification belongs to AC-008 and later change
  scope, while TASK-003 owns BFF session completion and TASK-004 owns CLI
  handoff and saved credentials. Their absence is not a TASK-002 defect.
- Candidate at final check: `6e21d8ed00d5159ec71e3f2e2414e80fd16f76af`.
