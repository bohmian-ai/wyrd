# TASK-002 cumulative implementation review

## Proposed findings

### TASK-REV-001 — concurrent callbacks can combine disjoint provider role assertions

- **Classification:** INCORRECT
- **Violated obligation:** REQ-008 and INV-003 require a human session to receive only roles mapped from that callback's verified provider groups; TASK-002 requires mapped roles and issuance to commit as one tenant transaction.
- **Location:** `crates/wyrd/wyrd-auth/src/callback.rs:228-256`; `crates/wyrd/wyrd-sql/src/queries/auth/role_assignments.rs:36-53`; `crates/wyrd/wyrd-auth/src/issuance.rs:482-510`.
- **Evidence:** `finish_id_token_exchange` calls `replace_user_roles` before `issue_human_session`. The latter is where the existing per-User refresh-family advisory lock is first acquired. `REPLACE_USER_ROLES_SQL` deletes roles visible to that statement and independently inserts the requested set, but it does not lock the User or family. For an existing User with no roles, two valid callbacks whose signed group sets map to disjoint roles can each insert its own row before either reaches the family lock. The callback that acquires the family lock second then reads both its own uncommitted row and the first callback's committed row in `TenantTokenIssuer::issue`, so its token and the final durable set contain the union rather than its verified set. The current role tests and journey logins are sequential and do not exercise this reachable overlap.
- **Observable consequence:** a callback whose verified groups map only to role B can receive role A's permissions too. Future refreshes also read the combined durable set, so authority not asserted by the latest provider response remains renewable.
- **Required testable correction:** reuse the existing tenant-qualified refresh-family lock in the callback transaction after canonical User resolution and before `replace_user_roles`; retain it through role audit, session issuance, completion, and commit. `issue_human_session` may safely reacquire the transaction lock. Add one deterministic Postgres proof using the production callback/session owners with the same existing User and disjoint mapped groups; force both orderings and prove each issued token contains only its own callback's mapped roles, the final durable roles equal the later serialized callback's exact set, and no union survives.

### TASK-REV-002 — the generated token contract falsely says API-key exchange returns a refresh token

- **Classification:** INCORRECT
- **Violated obligation:** TASK-002 requires human-connection provenance on human refresh rows and explicitly requires refresh provenance to be absent from Service/Agent paths; REQ-013 and the runtime-identity authority keep machine renewal on API-key re-exchange. The task also requires public schemas and documentation to agree with runtime behavior.
- **Location:** `crates/wyrd-spec/src/auth/token.rs:94-99`; `crates/wyrd-spec/schemas/auth_token_response.json:26`; `crates/wyrd-spec/tests/schemas/auth_token_response.json:26`.
- **Evidence:** the materially revised `TokenResponse::refresh_token` documentation lists `wyrd_api_key` among credentials that issue a refresh token, and that sentence is published in both generated schemas. `TenantTokenIssuer::issue` constructs `refresh_token: None` at `crates/wyrd/wyrd-auth/src/issuance.rs:445-448`; `architecture/wyrd-design.md` states that machine API-key exchange issues no refresh token and renews by re-exchanging the durable key.
- **Observable consequence:** OpenAPI/schema consumers are told to expect renewable refresh authority that the server never returns, contradicting the machine-identity contract and encouraging incorrect client persistence and renewal behavior.
- **Required testable correction:** remove API-key exchange from the source field documentation so it says that only human OIDC login and human refresh rotation return this field, regenerate the two source-derived schemas with the existing generator, and run `mise run codegen:check`. No runtime or API-shape change is needed.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006 / INV-001: route key is pre-login routing only; callback derives tenant solely from one-use state; deployment owns redirect origin | `HumanConnections::begin_login`; `WyrdPostgres::login_state_tenant`; `AuthorizationCodeExchange::execute`; fixed completion URL | `unknown_tenant_and_no_connection_are_indistinguishable`; login header/Host tests; `tenant_callback_refusal_journey` | PASS |
| REQ-007 / INV-004: authorization code, PKCE, nonce, state, exact redirect, issuer, audience, algorithm, key, time, claims, screened bounded provider IO | login state stores PKCE/nonce/redirect; callback consumes before IO; `discover_provider`; `verify_id_token_algorithm`; `verify_id_token_against`; nonce/azp checks | verifier claims test; callback Postgres tests; screening tests; refusal journey | PASS |
| REQ-008 / INV-002: stable `(issuer, sub)` User identity, no email linking, zero default role, groups map only to tenant roles | exact human `sub` validation; `ensure_user_identity`; `role_names_to_refs`; `replace_user_roles`; shared issuer | same-email/different-subject tests; human login and provider-switch journeys | **FAIL — TASK-REV-001** |
| REQ-013 / INV-003: Service/Agent API-key and workload paths remain separate from human identity | API-key and `jwt-bearer` paths remain separate; machine issuance returns no refresh token | `tenant_machine_independence_journey`; workload journey | **FAIL — TASK-REV-002 (published contract)** |
| REQ-014: tested replacement becomes the sole active connection; same-email replacement identity inherits nothing | connection binding/revision checks in callback and issuance; identity key includes issuer and subject | `tenant_provider_switch_journey` | PASS |
| REQ-015: same person authenticates separately per tenant; no cross-tenant callback or authority | global state lookup reveals only owning tenant; all durable login operations use forced-RLS `TenantConn` | login-state RLS tests; same-issuer two-tenant journey; callback-refusal journey | PASS |
| REQ-016: old connection blocks login/renewal immediately; access token remains bounded; next issuance reads current grants | connection-slot lock plus exact revision provenance; five-minute access TTL; refresh uses current stored roles | provider-switch and connection-cutoff journeys; connection-overlap Postgres proof | PASS |
| REQ-017: successful issuance and role mutation use canonical transactional audit; audit failure establishes no session | `append_auth_audit` shares callback transaction; failure path rolls back; refusal audit is separate best-effort work | role-sync audit and injected audit-failure tests; login/refusal journeys | PASS |
| Retire public authorization-code grant and token-bearing callback response | `TokenRequest::AuthorizationCode` and legacy CLI login removed; callback returns fixed redirect/static page only | `authorization_code_grant_is_retired`; OpenAPI contract; refusal journey | PASS |
| Human refresh rows carry exact connection provenance; missing/old provenance cannot renew | migration revokes unbound User rows; issuance inserts id/revision; refresh propagates and checks them | migration preflight; provider switch/cutoff journeys | PASS |
| Replay containment and same-principal family mutations serialize through commit | `RefreshTokens::execute`, User revocation, and `issue_human_session` use `lock_refresh_family`; fixed family-before-connection ordering | ancestor replay/rotation, rotation/revocation, initial issuance/revocation overlap proofs | PASS |
| AC-002: real-server human login proves allowed and denied calls and unmapped zero authority | real callback and issuance path | `tenant_human_login_journey` | PASS |
| AC-003: two tenants/providers, same-issuer isolation, mutation/removal, wrong-tenant refusal | tenant-bound state, trust, RLS, and connection lifecycle | refusal journey, same-issuer isolation, connection journeys | PASS |
| AC-005: machine API key and exact workload federation keep working with SSO active; wrong bindings refuse | independent API-key and workload exchange owners | `tenant_machine_independence_journey` | PASS |
| AC-006: provider switch preserves recovery and never links or transfers authority by email | issuer+subject identity; tested candidate activation; old provenance cutoff | `tenant_provider_switch_journey` | PASS |
| AC-007: outage, unsafe destinations, invalid claims/nonce/state, replay, inactive connection, audit failure, mapping change, and old-session cutoff fail closed | shared verifier, screened IO, one-use state, transactional issuance/audit, connection/family locks | refusal journey plus focused verifier, Postgres, and overlap tests | **FAIL — concurrent disjoint role assertions are not covered and violate the mapped-role boundary (TASK-REV-001)** |
| Non-goals: no email linking, provider-token bearer authority, platform fallback, instant access-token revocation, new machine identity model, TASK-003 BFF, or TASK-004 handoff | legacy grant/CLI path removed; CLI handoff deliberately refused; BFF redemption primitive has no route here | source and cumulative diff inspection | PASS |
| Smallest implementation / no unrelated product expansion | existing owners and installed mechanisms are reused; no new dependency or public compatibility route | cumulative diff and remediation records | PASS |

## Prior-remediation closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| FIND-TASK-002-1 | public and stored human connection mappings require exact `sub`; same-email subjects remain distinct | CLOSED |
| FIND-TASK-002-2 | callback enforces OIDC `azp` rules after verification and before persistence | CLOSED |
| FIND-TASK-002-3 | sequential durable role-set changes append one canonical transactional `auth.user.roles.sync` event; unchanged sets do not | CLOSED; TASK-REV-001 is a distinct concurrency gap in exact-set replacement/issuance |
| FIND-TASK-002-4 | fresh discovery algorithms gate the token before shared asymmetric verification | CLOSED |
| FIND-TASK-002-5 | login-state transitions rely on forced RLS without duplicate tenant predicates | CLOSED |
| FIND-TASK-002-6 | cross-tenant state-to-tenant lookup is a narrow `WyrdPostgres` inherent operation | CLOSED |
| FIND-TASK-002-7 | PKCE verifier is held in `SecretString` and redacted by `Debug` | CLOSED |
| FIND-TASK-002-8 | the exact R2 Rust items carry the required substantive rustdoc/error/panic contracts | CLOSED; TASK-REV-002 is a different, materially revised public field contract |
| FIND-TASK-002-9 | cited function-local imports moved to module import blocks | CLOSED |
| FIND-TASK-002-10 | algorithm helper rustdoc matches advertised-set membership and names the shared verifier's HMAC ownership | CLOSED |
| FIND-TASK-002-11 | generic verification requires `exp`/`iss`/`aud`, validates present `nbf`; OIDC entry additionally requires valid `iat` | CLOSED |
| FIND-TASK-002-12 | refresh classifies and mutates only after the family lock; overlapping ancestor replay contains the successor | CLOSED |
| FIND-TASK-002-13 | `refresh_by_hash` rustdoc matches lookup-before-lock/classification and its actual callers | CLOSED |
| FIND-TASK-002-14 | administrative User revocation takes the family lock before suspension/family retirement | CLOSED |
| FIND-TASK-002-15 | initial human-session issuance takes the family lock before connection lock and principal read | CLOSED |
| FIND-TASK-002-16 | `print_tokens` rustdoc now describes only token output | CLOSED |
| FIND-TASK-002-17 | OIDC verifier rejects missing/non-string/empty/non-ASCII/oversized subjects while generic workload mapping is unchanged | CLOSED |

## Verification notes and limits

- Reviewed the immutable complete range `3fc085acf5b3a710d5dc80892bd2e664b3db6174..fa2bda92a7e79471b79b607870c1e86a9f35639c`, the approved revision-4 specification, original task, all R1-R8 remediation tasks, applicable architecture/security authority, manifests/tasks, production owners, generated schemas, and cited tests.
- Independently ran `mise exec -- cargo nextest run --locked -p wyrd-auth-verify --lib -E 'test(=tests::oidc_id_token_requires_binding_and_time_claims)'`: 1 passed. `git diff --check` for the immutable range also passed.
- The implementation records report green focused Postgres proofs, `test:principals:unit`, `test:principals:integration`, `test:sql`, `test:identity:journey` (27/27), codegen, tenant/client/pool boundary checks, formatting, lints, and docs checks. This review did not rerun the Docker/Postgres/provider lanes.
- No existing proof overlaps two callbacks for one established User with disjoint mapped role sets. That missing concurrency proof is material because source inspection establishes the unsafe interleaving in TASK-REV-001.

## Overall result

**FAIL**

The candidate closes all seventeen prior findings, but it does not yet satisfy the original task exactly: concurrent valid callbacks can mint and persist authority not asserted by the later callback, and the generated token contract contradicts the machine API-key renewal behavior.
