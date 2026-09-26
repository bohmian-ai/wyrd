# TASK-002 Wave 1 Task Implementation Review — R3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `d861845f3f5d89aca413857dcfb8c1bbfaee349d`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Prior remediation tasks: `TASK-002-R1-tenant-login-corrections.md` and
  `TASK-002-R2-repository-rule-corrections.md`
- Reviewed range: the complete cumulative base-to-candidate diff

The candidate was checked out at the stated commit before and after inspection.
The repository has no `.codegraph/` index, so source and caller inspection used
Git and `rg`.

## Overall result

**PASS**

The cumulative candidate satisfies the original task. Tenant login derives its
tenant and exact connection from one-use server state, verifies the complete
human OIDC boundary, provisions exact `(issuer, sub)` Users, grants only mapped
tenant roles, issues and renews Wyrd authority through the shared issuer, and
keeps machine authentication independent. The callback and `/auth/token`
contracts no longer expose a public authorization-code-to-token bypass. All
nine prior findings are closed. I found no material MISSING, INCORRECT, DRIFT,
VIOLATION, or REGRESSION finding within the task-acceptance scope.

The lead-authorized reuse cleanups and the two unrelated `test:wyrd` flaky-test
repairs are not scope drift. Inspection found the reuse changes behavior
preserving: `principal_event` reproduces the prior role-sync event fields,
the shared verifier still rejects every HMAC ID token before key lookup, and
the callback-test helpers preserve their setup. The gateway repair compares the
same terminal payload as parsed JSON, and the Forge repair only orders the test
against its scheduler boot pass.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006 and INV-001: the route key is routing context only; the callback derives tenant and connection solely from one-use state; public URLs are deployment controlled | `HumanConnections::begin_login` accepts `BeginLogin`, resolves the route key, binds the Active connection revision and configured callback, and stores only a state digest; `AuthorizationCodeExchange::execute` uses `WyrdPostgres::login_state_tenant`; the HTTP callback accepts only `code` and `state` | `login_ignores_request_headers`, `the_host_header_cannot_select_a_tenant`, `unknown_tenant_and_no_connection_are_indistinguishable`, `tenant_callback_refusal_journey`, OpenAPI contract proof | PASS |
| REQ-007 and INV-004: PKCE, nonce, bounded one-use state, exact redirect, issuer/audience/authorized-party/algorithm/signature/time/key/claims checks, screened provider IO, and fail-closed mismatch/outage | `LoginState` binds the PKCE verifier, nonce, issuer, client, redirect, and connection revision; callback consumes and commits before provider IO; `verify_id_token_algorithm`, `ExternalVerifier::verify_external_against`, `verify_nonce`, `verify_authorized_party`, `bound_connection`, and screened discovery/token exchange form the shared refusal path | `tenant_callback_refusal_journey`; callback tests for wrong audience, `azp`, unadvertised algorithm, audit failure, and no completion; screened destination tests | PASS |
| REQ-008 and INV-002: successful login creates/resolves a tenant User by exact `(issuer, sub)`; email never links; only verified mapped groups grant current tenant roles; unmapped Users get zero privileged grants | human connection input and stored-row decode require `sub`; `ensure_user_identity` keys on issuer and verified subject; `role_names_to_refs` ignores defaults and unknown groups; shared issuance resolves current grants | `human_subject_must_be_exactly_sub`; stored human-connection decode proof; `same_email_different_subjects_are_distinct_users`; `tenant_human_login_journey` | PASS |
| REQ-013 and INV-003: Service/Agent API-key and exact workload-assertion paths remain independent of human SSO and cannot inherit human roles or platform authority | `/auth/token` retains `WyrdApiKey` and `JwtBearer` branches; human callback uses only tenant `User` issuance and never changes workload/platform verification | `tenant_machine_independence_journey`; existing workload JWT-bearer journey and negative issuer/subject/audience/tenant cases | PASS |
| REQ-014: a tested replacement is activated before use; old and replacement subjects remain separate, including equal email; owner recovery remains | callback re-reads and matches the exact Active connection binding before persistence; identity is exact issuer/subject; no email lookup remains | `tenant_provider_switch_journey`; `same_email_different_subjects_are_distinct_users`; connection lifecycle journeys | PASS |
| REQ-015: tenant membership and same-issuer identities remain isolated | login-state transitions use forced RLS through `TenantConn`; the narrow definer lookup returns only the pending state's owner; completion redemption is tenant-scoped; issued authority is tenant-bound | `same_issuer_two_tenant_isolation_keycloak`; `login_state_transitions_are_confined_to_the_owning_tenant`; `login_state_tenant_names_only_the_owner_of_a_pending_state`; refusal journey wrong-tenant case | PASS |
| REQ-016: replacement/deactivation/deletion stops new login and human refresh; successor retains provenance; mappings apply at next issuance; existing access remains the five-minute snapshot | refresh rows carry connection id/revision; `RefreshTokens::execute` requires both and routes through `issue_human_session`, which locks/rechecks the Active binding and recomputes grants; migration revokes provenance-free User refresh rows | `tenant_provider_switch_journey`; `tenant_connection_session_cutoff_journey`; refresh provenance tests; `human_connection_upgrade_preflight` | PASS |
| REQ-017: successful issuance and role changes use canonical transactional audit; required audit failure issues nothing; refusals are redacted | token exchange, role replacement, completion, and canonical audit append share the callback tenant transaction; `roles_sync_event` uses the shared canonical event builder; failure audit has no rolled-back principal | changed/unchanged role-sync audit test; injected audit-failure rollback test; human-login and callback-refusal journeys | PASS |
| Packet-local begin contract: exactly one browser flow hash or CLI handoff, with unknown handoff and unrecordable binding refused | `BeginLogin::initiation`, `known_initiation`, digest-backed unique login-state columns and `num_nonnulls` constraint; CLI remains deliberately unavailable until TASK-004 owns the handoff | request contract tests; login Postgres tests; migration coverage | PASS |
| Packet-local callback/completion contract: common callback has no tenant selector, consumes before provider IO, stores a sealed one-use completion, and returns only fixed browser redirect or static CLI page | `CallbackQuery`; `AuthorizationCodeExchange`; `complete_login_state`; `HumanConnections::redeem_completion`; callback route returns `/login/complete` or `CLI_LOGIN_COMPLETE_PAGE` without capability-bearing query data | callback route/Postgres tests; `tenant_callback_refusal_journey`; OpenAPI contract test | PASS |
| Authorization-code bypass retirement: `/auth/token` cannot exchange provider code/state; no token-bearing callback JSON or compatibility route remains | `TokenRequest` contains only API key, RFC 8693 exchange, workload JWT bearer, and refresh; old shared-client and CLI login call sites/routes were removed; callback returns HTML/redirect | `authorization_code_grant_is_retired`; OpenAPI/schema/codegen evidence; refusal journey explicitly rejects the retired grant | PASS |
| AC-002: real-server tenant User login, mapped authorization, denial, unmapped zero grants, and audit | complete client/server login and redemption path in `identity_e2e.rs` | `tenant_human_login_journey` | PASS |
| AC-003: hosted-style tenant separation, wrong-tenant callback, and same-issuer cross-tenant refusal | state owner lookup plus forced-RLS transition/redemption and tenant-bound issuance | `tenant_callback_refusal_journey`; `same_issuer_two_tenant_isolation_keycloak`; connection lifecycle journeys | PASS |
| AC-005: machine identity continues with SSO active and exact workload binding fails closed | unchanged shared API-key and workload owners remain separate from the human connection/callback path | `tenant_machine_independence_journey` | PASS |
| AC-006: tested provider switch, owner recovery, no authority inheritance, no email linking | exact Active revision checks, exact issuer/subject identity, current role recomputation | `tenant_provider_switch_journey` | PASS |
| AC-007 within TASK-002 ownership: outage, unsafe destinations, invalid token/nonce/algorithm/`azp`, replay, inactive connection, audit failure, mapping change, key/secret rotation, unmapped subject, and old-session renewal cutoff | shared screening, verifier, state, audit, connection, and refresh owners implement the failure boundaries; fixed callback output preserves later BFF ownership | refusal, switch, human-login, connection-rotation, cutoff, and focused unit/Postgres tests recorded in the task/R1/R2 evidence | PASS |
| Preserve five-minute access-token semantics and refresh replay-family containment | shared `TenantTokenIssuer` remains the only human issuance owner; refresh replay branch and family revocation remain intact | provider-switch/cutoff and refresh tests; task evidence records principals unit/integration lanes | PASS |
| Non-goals: no email linking, provider-token bearer authority, platform fallback, instant access-token revocation promise, new machine model, BFF completion route, or CLI handoff implementation | no such contract or owner was added; docs retain bounded snapshot semantics; TASK-003/004 seams remain explicit primitives only | cumulative diff and public-contract inspection | PASS |
| Lead-directed reuse cleanups preserve the accepted tenant-login behavior | audit event construction is factored without field changes; HMAC is still rejected by `ExternalVerifier` before JWKS lookup; test setup shares existing helpers; old-key retention docs cover the existing two-minute completion TTL | callback/audit source inspection; existing security/callback journeys; recorded `test:wyrd`, lints, and docs checks | PASS |
| Lead-directed flaky-test repairs do not alter product behavior or weaken assertions | gateway test parses and compares the same terminal JSON independent of map order; coordinator standby test waits for the boot pass before driving the pass under test | source inspection; recorded `mise run test:wyrd` result (2,213 passed) | PASS |

## Prior-finding closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` — mutable human subject claim | request validation and stored-row decode both require exact `sub`; same-email/different-subject login remains distinct | CLOSED |
| `FIND-TASK-002-2` — missing OIDC authorized-party validation | tenant human callback enforces required/matching `azp` before identity or issuance | CLOSED |
| `FIND-TASK-002-3` — missing role-change audit | changed role sets append one canonical `auth.user.roles.sync` event transactionally; unchanged sets append none; audit failure rolls back | CLOSED |
| `FIND-TASK-002-4` — provider algorithm set not enforced | callback compares the token header algorithm to fresh discovery metadata before verification/persistence; the shared verifier independently rejects HMAC | CLOSED |
| `FIND-TASK-002-5` — duplicate tenant predicates on `TenantConn` transitions | transition SQL relies on forced RLS; cross-tenant Postgres proof covers purge/consume/complete/redeem | CLOSED |
| `FIND-TASK-002-6` — raw-pool login-state owner lookup | `WyrdPostgres::login_state_tenant` owns the private app-pool query and exposes only typed state hash to callback code | CLOSED |
| `FIND-TASK-002-7` — PKCE verifier exposed through `Debug` | the single `LoginState` stores `SecretString`; its debug regression proof passes | CLOSED |
| `FIND-TASK-002-8` — required rustdoc missing | the exact `Sha256Hex` projections, state-transition constants, signing helper panic contract, and refusal-case alias now carry the required substantive documentation | CLOSED |
| `FIND-TASK-002-9` — function-scoped imports | SHA-256, Utoipa, and Wiremock imports now live in their module import blocks with the required feature gate and alias | CLOSED |

## Proposed findings

None.

## Verification limits

- I inspected the complete cumulative diff, the original task and approved
  specification, prior findings/remediation, the task's core login, callback,
  refresh, SQL/migration, HTTP contract, generated-contract, documentation,
  and journey-test seams, plus every post-R2 executable cleanup and both
  lead-directed flaky-test repairs.
- Fresh focused checks in this review passed:
  `human_subject_must_be_exactly_sub`,
  `authorization_code_grant_is_retired`,
  `login_state_debug_redacts_the_pkce_verifier`, and cumulative
  `git diff --check`.
- The candidate records green results for all four required identity journeys,
  the full 27-test identity journey lane, principals unit/integration, SQL,
  tenant-isolation, pool-boundary, client-tier, codegen, docs, format, lints,
  and the final `test:wyrd` lane (2,213 passed). I did not rerun the Docker,
  Postgres, IdP, or whole-workspace lanes during this bounded review.
- TASK-003's BFF `/login/complete` redemption route and TASK-004's CLI handoff
  storage remain intentional non-goals; this candidate supplies only their
  server-owned primitives.
