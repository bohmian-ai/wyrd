# TASK-002 Wave 1 Task Implementation Review — R4

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `63e6a545db156a095b670a5bb8bc36f6f36fba32`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Prior remediation tasks: `TASK-002-R1-tenant-login-corrections.md`,
  `TASK-002-R2-repository-rule-corrections.md`, and
  `TASK-002-R3-algorithm-helper-rustdoc-correction.md`
- Reviewed range: complete cumulative base-to-candidate diff, 75 files,
  7,185 insertions and 1,713 deletions

The checked-out candidate equaled the stated commit before and after review.
The worktree was clean when the subject was established. The repository has no
`.codegraph/` index, so inspection used Git, `rg`, and source reads. Review
artifacts are outside the immutable subject.

The lead-directed reuse cleanups and test repairs recorded in the remediation
evidence tables were treated as authorized work, not scope drift, and were
still checked for regression. No implementation summary was accepted as proof:
the cumulative Git diff, current owners, contracts, migration, routes, tests,
and post-R3 changes were inspected directly.

## Overall result

**PASS**

The cumulative candidate satisfies the original task and all three remediation
packets. Tenant login derives authority only from one-use server state and the
tenant's exact Active connection, verifies the complete human OIDC boundary,
provisions exact `(issuer, sub)` tenant Users, grants only currently mapped
tenant roles, audits issuance and role changes transactionally, and preserves
independent machine authentication. The public authorization-code token grant
and token-bearing callback are retired. All ten prior findings are closed. I
found no material MISSING, INCORRECT, DRIFT, VIOLATION, or REGRESSION finding in
the task-acceptance scope.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006 and INV-001: route/host/header context cannot become authority; callback tenant and connection come only from bounded server state | `HumanConnections::begin_login` resolves the route key, binds the exact Active connection revision and configured callback, and stores a state digest; `AuthorizationCodeExchange::execute` resolves only that digest through `WyrdPostgres::login_state_tenant`; `CallbackQuery` contains only `code` and `state` | Header-hostility and indistinguishable-login tests; `tenant_callback_refusal_journey`; OpenAPI contract proof | PASS |
| REQ-007 and INV-004: authorization code with PKCE, nonce, one-use state, exact redirect, complete ID-token verification, screened calls, and fail-closed refusal | `LoginState` binds issuer, client, redirect, PKCE verifier, nonce, initiation, and connection revision; callback consumes and commits before provider IO; `verify_id_token_algorithm`, `ExternalVerifier::verify_external_against`, `verify_nonce`, `verify_authorized_party`, and `bound_connection` precede identity persistence | `tenant_callback_refusal_journey`, including replay, token claims, advertised-algorithm, advertised-HS256/shared-verifier, and outage cases; screened-destination and focused callback tests | PASS |
| REQ-008 and INV-002: exact `(issuer, sub)` tenant User identity, no email linking, mapped tenant roles only, and zero default privilege | `ConnectionInput::validate` and stored connection decode require `HUMAN_SUBJECT_CLAIM == "sub"`; `ensure_user_identity` keys only by issuer and verified subject; `role_names_to_refs` uses verified groups and configured tenant mappings without default roles | Fresh `human_subject_must_be_exactly_sub` pass; stored-row decode proof; same-email/different-subject proof; `tenant_human_login_journey` | PASS |
| REQ-013 and INV-003: Service/Agent API-key and exact workload-federation paths remain independent of human SSO and platform authority | `/auth/token` retains separate API-key and JWT-bearer grants; the human callback issues only a tenant `User` session and does not modify workload/platform verification | `tenant_machine_independence_journey`; workload JWT-bearer positive and wrong issuer/subject/audience/tenant cases | PASS |
| REQ-014: tested replacement becomes the sole login source without identity or authority inheritance | Callback re-reads the exact Active connection id/revision, issuer, and client before persistence; identity is exact issuer/subject and no email lookup remains | `tenant_provider_switch_journey`; distinct-subject proof; connection lifecycle journeys | PASS |
| REQ-015: a login and resulting authority remain tenant isolated, including a shared issuer | State transitions and redemption run through forced-RLS `TenantConn`; the narrow definer lookup returns only the pending state's tenant; completion and issued authority remain tenant-bound | Same-issuer two-tenant journey; wrong-tenant callback case; `pg_login_state` owner and transition isolation proofs; `check:tenant-isolation` evidence | PASS |
| REQ-016: old connection blocks login/renewal immediately, successor refresh provenance stays exact, mappings update at next issuance, and access tokens retain only the five-minute snapshot | Refresh rows carry connection id/revision; human refresh and callback lock/recheck the Active binding and recompute grants; migration revokes provenance-free User refresh rows; shared issuer retains the five-minute access lifetime | Provider-switch and session-cutoff journeys; refresh provenance and migration tests | PASS |
| REQ-017: successful issuance and provider-driven role changes use canonical transactional audit; required audit failure issues nothing | `replace_user_roles` reports mutation; changed roles append one `auth.user.roles.sync` event through `append_auth_audit`; token exchange, role replacement, refresh issuance, completion, and audit share the tenant transaction | Changed/unchanged role-sync test; injected audit-failure rollback proof; human-login and callback-refusal journey audit assertions | PASS |
| Packet-local begin contract: exactly one browser-flow hash or CLI handoff binding; unknown handoff and unrecordable binding fail closed | `BeginLogin::initiation`, `known_initiation`, the mutually exclusive login-state columns, uniqueness constraints, and `ON CONFLICT DO NOTHING` preserve one binding per login; CLI remains unavailable until TASK-004 supplies its owner | Contract and Postgres login-state tests; callback-refusal binding cases | PASS |
| Packet-local callback/completion contract: common callback has no tenant selector, consumes before IO, seals a one-use completion, and returns only a fixed browser redirect or static CLI page | `AuthorizationCodeExchange`, `complete_login_state`, `HumanConnections::redeem_completion`, and route response selection retain provider code and Wyrd tokens outside URLs and callback JSON | Callback route/Postgres tests; refusal journey; OpenAPI proof | PASS |
| Authorization-code bypass retirement | `TokenRequest` has no authorization-code variant; shared-client `begin_login`, legacy CLI login, and `GET /auth/login` were removed; callback is the sole code-exchange owner | Fresh `authorization_code_grant_is_retired` pass; schema/codegen/OpenAPI evidence; refusal journey rejects the retired grant | PASS |
| AC-002: real-server tenant login, mapped authorization, denial, unmapped zero grants, and audit | Full begin/callback/completion/credential path in `identity_e2e.rs` | `tenant_human_login_journey` recorded green | PASS |
| AC-003: two-tenant separation, wrong-tenant callback, and same-issuer refusal | State-owner lookup, forced-RLS transitions/redemption, and tenant-bound issuance | `tenant_callback_refusal_journey`; `same_issuer_two_tenant_isolation_keycloak`; lifecycle journeys | PASS |
| AC-005: machine paths keep working with SSO active and exact workload binding fails closed | Existing shared API-key/workload owners remain separate from the human connection and callback path | `tenant_machine_independence_journey` recorded green | PASS |
| AC-006: tested provider switch, recovery path, separate same-email User, and no inherited authority | Exact Active binding, exact issuer/subject identity, and current-role recomputation | `tenant_provider_switch_journey` recorded green | PASS |
| AC-007 within TASK-002 ownership: outage, unsafe URLs, invalid nonce/issuer/audience/signature/algorithm/`azp`, replay/expiry, inactive connection, audit failure, mapping/key/secret changes, unmapped subject, and old-session cutoff | Shared screening, verification, state, audit, connection, refresh, and sealing owners implement the refusal boundaries | Refusal, switch, human-login, connection-rotation, cutoff, focused unit/Postgres tests, and the new advertised-HS256 journey case | PASS |
| Preserve refresh replay-family containment and five-minute access-token semantics | Existing `TenantTokenIssuer` and refresh-family owner remain the human issuance path; the new connection provenance check precedes successor issuance without weakening replay revocation | Provider-switch/cutoff and existing refresh tests; principals unit/integration evidence | PASS |
| Prior remediation R1: exact `sub`, OIDC `azp`, role-change audit, advertised algorithm policy, RLS-only transitions, owner-bound lookup, and PKCE redaction | Current source retains every R1 correction in its original owner | Focused and journey evidence cited above; fresh PKCE Debug-redaction test pass | PASS |
| Prior remediation R2: required rustdoc and module-top imports | The cited `Sha256Hex` projections, SQL constants, signing helper, and test alias are documented; SHA-256, Utoipa, and Wiremock imports remain in module import blocks | Static cumulative-diff/source inspection; recorded format/lint/codegen checks | PASS |
| Prior remediation R3 / FIND-TASK-002-10: algorithm helper documents only advertised-set membership and names the shared symmetric-algorithm owner | `verify_id_token_algorithm` rustdoc matches its `filter_map` membership body and links the immediately following `ExternalVerifier::verify_external_against`; the remediation changed documentation, not executable policy | Direct post-R3 diff and body/caller inspection; `cargo doc -p wyrd-auth --no-deps`, format, lints, and diff-check recorded green; advertised-HS256 journey proves the composition | PASS |
| Lead-directed reuse cleanups preserve accepted behavior | `principal_event` preserves canonical role-sync fields; shared verifier remains the single HMAC rejection owner; callback test helpers preserve setup; sealing runbook records the completion TTL | Source/caller inspection and existing callback/audit/journey evidence | PASS |
| Lead-directed test repairs preserve assertions and product behavior | Gateway helper compares the same terminal JSON with `let ... else`; Forge readiness repair only orders the test after its boot pass | Direct diff inspection; gateway 39-test and final `test:wyrd` evidence | PASS |
| Non-goals and prohibited changes | No email linking, provider-token bearer authority, platform fallback, instant-revocation promise, new machine model, compatibility route, parallel audit sink, dependency, TASK-003 BFF route, or TASK-004 CLI handoff implementation entered the cumulative source diff | Cumulative public-contract, route, source, migration, docs, and test inspection | PASS |

## Prior-finding closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` — mutable human subject claim | Request validation and stored-row decode require exact `sub`; same-email/different-subject login remains distinct | CLOSED |
| `FIND-TASK-002-2` — missing OIDC authorized-party validation | Callback enforces multi-audience and present-`azp` rules before identity or issuance | CLOSED |
| `FIND-TASK-002-3` — missing provider-driven role audit | Changed role sets append exactly one canonical role-sync event transactionally; unchanged sets append none; audit failure rolls back | CLOSED |
| `FIND-TASK-002-4` — provider algorithm policy not enforced | Fresh discovery constrains advertised-set membership and the immediately following shared verifier rejects HMAC before key lookup; the mixed EdDSA/HS256 journey exercises the owner seam | CLOSED |
| `FIND-TASK-002-5` — duplicate tenant predicates | Login-state transitions rely on forced RLS without manual tenant selection and retain cross-tenant Postgres proof | CLOSED |
| `FIND-TASK-002-6` — raw-pool state lookup | `WyrdPostgres::login_state_tenant` owns the private app-pool query and exposes only the typed state hash | CLOSED |
| `FIND-TASK-002-7` — printable PKCE verifier | `LoginState` stores `SecretString`; fresh Debug-redaction proof passed | CLOSED |
| `FIND-TASK-002-8` — incomplete required rustdoc | Every exact R2 item remains substantively documented with applicable `# Errors`/`# Panics` | CLOSED |
| `FIND-TASK-002-9` — function-scoped imports | The cited imports remain in their module import blocks with required feature gating and aliasing | CLOSED |
| `FIND-TASK-002-10` — false algorithm-helper rustdoc | Current rustdoc accurately limits the helper to advertised-set membership and identifies the shared verifier as the symmetric-algorithm owner; no executable behavior was changed by the correction | CLOSED |

## Proposed findings

None.

## Verification limits

- I inspected the complete cumulative diff and current task-owned source across
  contracts, client retirement, login/callback orchestration, connection
  decoding, issuance/audit, SQL and migration, HTTP/OpenAPI routes, generated
  schemas, documentation, and identity journeys. I also inspected every
  post-R3 executable/test change and its relevant owner/caller.
- Fresh checks passed for exact human `sub`, authorization-code grant
  retirement, PKCE Debug redaction, and cumulative `git diff --check`.
- The candidate records green results for all four required identity journeys,
  the full 27-test identity lane, principals unit/integration, SQL,
  tenant-isolation, pool-boundary, client-tier, codegen, docs, format, lints,
  `cargo doc -p wyrd-auth --no-deps`, gateway adapter tests, and final
  `test:wyrd` with 2,213 passing tests. I did not rerun the expensive Docker,
  Postgres, IdP, or broad workspace lanes during this bounded review.
- TASK-003's BFF `/login/complete` redemption route and TASK-004's CLI handoff
  persistence remain intentional non-goals; this candidate supplies only their
  server-owned primitives.
