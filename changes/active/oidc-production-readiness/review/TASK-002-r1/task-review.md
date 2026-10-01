# TASK-002 Task Implementation Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `87de451ed87ad059cefd579eb15ef4b028a92547`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Reviewed range: the complete base-to-candidate diff (46 files, 3,728 insertions, 1,663 deletions), plus the surrounding identity, verifier, SQL, route, and test owners needed to trace the changed behavior.

## Overall result: FAIL

The candidate implements the state-bound callback, tenant isolation, sealed completion handoff, connection-bound renewal, replacement cutoff, transactional successful-login audit, and machine-plane separation. It does not yet satisfy the required human identity or complete OIDC audience validation contracts. Two reachable, bounded findings remain.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006: route key is routing context only; callback derives tenant/connection from one-use state and redirects only to the deployment origin | `wyrd-auth/src/login.rs:88-132`; `wyrd-auth/src/callback.rs:104-174`; `wyrd-server/src/components/auth/routes.rs:303-338`; `wyrd-sql/src/queries/platform/tenant_resolver.rs:37-72` | `login_ignores_request_headers`; callback refusal journey same-issuer/cross-tenant step; OpenAPI contract test | PASS |
| REQ-007: code+PKCE, state, nonce, exact redirect, bounded one-time state, and complete ID-token signature/issuer/audience/algorithm/key/time/claims verification | State/PKCE/nonce and consume-before-IO are implemented in `login.rs`, `callback.rs`, and `login_state.rs`; shared verification is called at `callback.rs:222-227` | Refusal journey covers wrong scalar audience, nonce, issuer, signature, symmetric algorithm, replay, outage, and unsafe destinations. It does not cover or reject a multi-audience token whose `azp` names another client. | **FAIL (TR-002)** |
| REQ-008: exact provider provisions tenant User by verified `(issuer, subject)`, maps groups only to tenant roles, and grants no privileged default | User creation and role replacement occur transactionally at `callback.rs:230-265`; unmapped groups resolve to no role | Human login journey and callback Postgres tests cover mapped/unmapped roles | **FAIL (TR-001)** — configured claim mapping may substitute email or another claim for OIDC `sub` |
| REQ-013: Service/Agent API key and workload federation remain independent of human SSO | Existing API-key and JWT-bearer token arms remain separate; human refresh provenance is limited to User rows | `tenant_machine_independence_journey` covers API key and exact workload binding plus wrong issuer/subject/audience/tenant | PASS |
| REQ-014: tested replacement precedes activation; old identity authority is not inherited and email never links | Activation changes the bound connection revision; callback keys identities through `ensure_user_identity` | Provider-switch journey covers old refresh cutoff, same-email users under different issuers, mapping on next login, and owner recovery | **FAIL (TR-001)** — a permitted non-`sub` mapping can still link distinct subjects of the same issuer by email or another mutable claim |
| REQ-015: multi-tenant users authenticate separately and one tenant never grants another | Login state resolves one tenant; completion redemption uses tenant RLS | Callback refusal journey and `same_issuer_two_tenant_isolation_keycloak` | PASS |
| REQ-016: old connection stops login/renewal immediately; mapping applies on next issuance; access snapshot remains at most five minutes | Callback rechecks binding; `issue_human_session` locks/rechecks active connection; refresh carries connection id/revision; access TTL remains bounded | Provider-switch and session-cutoff journeys plus refresh unit coverage | PASS |
| REQ-017: security-significant login and role outcomes are redacted/audited; audit failure establishes no session | Successful issuance, refresh row, completion, and canonical audit share one tenant transaction; denied callbacks use the canonical best-effort refusal path | Callback refusal journey injects audit failure and proves no completion/refresh row; human journey checks success/denial audit | PASS |
| INV-001: paths, hosts, headers, browser state, email, and unverified tokens never select effective tenant/connection | Tenant comes from route resolver only before login and state resolver only after callback; headers are not handler inputs | Header-hostility tests and wrong-tenant redemption test | PASS |
| INV-002: identity is `(issuer, subject)`; email is display data only | `ensure_user_identity` keys on the string exposed as `verified.subject` | Same-email/different-issuer test covers only one case | **FAIL (TR-001)** — `verified.subject` is an administrator-selected claim path, not necessarily OIDC `sub` |
| INV-003: platform, tenant-user, and workload planes remain distinct | Human callback always issues `PrincipalKindTag::User`; platform login and workload exchange remain separate | Human/machine journeys and wrong-binding cases | PASS |
| INV-004: OIDC trust, TLS/SSRF, key rotation, replay protection, audit, and RLS fail closed | Screened/pinned HTTP, one-use state, RLS, connection lock, and transactional issue path are present | Screening tests, refusal journey, tenant-isolation check evidence | **FAIL (TR-002)** — authorized-party validation is absent for multi-audience ID tokens |
| AC-002: controlled real-provider login maps role, permits one Wyrd call, and denies one | Full callback and issuance path exists | `tenant_human_login_journey` | PASS |
| AC-003 task slice: tenant-separated provider state and same-issuer cross-tenant refusal | State hash resolver and tenant-bound redemption | Callback refusal journey and same-issuer two-tenant journey | PASS |
| AC-005 task slice: machine paths continue with SSO and inexact bindings fail | Token arms remain independent | `tenant_machine_independence_journey` | PASS |
| AC-006: replacement yields a separate User without email linking or inherited authority | Provider-switch path and identity table are present | `tenant_provider_switch_journey` | **FAIL (TR-001)** |
| AC-007 task slice: trust-boundary faults, replay, inactive connection, audit failure, mapping, and old-session cutoff | Failure branches are implemented in callback/login/refresh owners | Named refusal/switch/cutoff journeys and narrow screening tests | **FAIL (TR-002)** — audience evidence exercises only a single wrong `aud`, not required multi-audience/authorized-party semantics |
| Packet-local login contract: exactly one browser/CLI binding; 256-bit state; tenantless callback; consume before provider IO; fixed safe response | `BeginLogin::initiation`; 32-byte state; definer lookup; `consume_login_state` commit; sealed completion and fixed redirect/static page | Binding-shape, replay, token-leak, and callback response tests | PASS |
| Packet-local renewal contract: new human refresh rows carry connection provenance; unbound legacy rows revoked; successor preserves provenance | Migration revokes unbound User rows; refresh and issuance require/copy exact connection binding | Migration and refresh tests; provider-switch/cutoff journeys | PASS |
| Required retirement: no public authorization-code grant or token-bearing callback response; API-key/workload/refresh grants stay | `TokenRequest::AuthorizationCode`, client begin helper, legacy CLI login, and GET login route removed; callback returns redirect/static HTML | Token deserialization refusal, callback no-token assertions, OpenAPI contract | PASS |
| Non-goal: no email linking, provider-token API authority, platform fallback, instant-revocation promise, or new machine identity model | No provider token is accepted as Wyrd bearer; no platform fallback/new machine model; docs preserve bounded access snapshots | Source and journeys cover all except the configurable subject path | **FAIL (TR-001)** for email linking; other non-goals PASS |
| Ponytail scope: reuse existing issuer verifier, screened HTTP, issuer, refresh, RLS, and audit owners; no speculative compatibility surface | Candidate primarily composes existing owners and deletes the bypassable legacy flow | Diff inspection found no separate compatibility route or redundant auth owner | PASS |

## Proposed findings

### TR-001 — INCORRECT: human identity can be keyed by email or another configured claim instead of OIDC `sub`

- Violated obligations: REQ-008, REQ-014, INV-002, AC-006, and the task's explicit `(issuer, subject)` / no-email-linking contract.
- Exact locations:
  - `crates/wyrd-spec/src/auth/human_connection.rs:179-240`
  - `crates/shared/wyrd-auth-oidc/src/claims.rs:38-62`
  - `crates/wyrd/wyrd-auth/src/callback.rs:222-243`
  - `crates/wyrd/wyrd-auth/src/callback.rs:496-513`
- Evidence: `ConnectionInput` accepts any non-empty `claim_mapping.subject`. The shared claim mapper places that arbitrary claim into `verified.subject`, and the callback passes it unchanged to `ensure_user_identity`, whose durable key is `(issuer, that value)`. A valid connection configured with `subject: "email"` therefore aliases two different OIDC `sub` values at the same issuer when their email matches, or changes the principal identity when one person's email changes. The provider-switch journey always configures `subject: "sub"`, so it does not exercise the reachable invalid configuration.
- Observable consequence: distinct provider subjects can resolve to the same tenant User and inherit that User's roles/history, while one subject whose mapped value changes can be provisioned as a second User. Email becomes identity authority despite the approved invariant.
- Required testable correction: make the tenant human-login contract use the verified standard OIDC `sub` claim unconditionally. At the public human-connection boundary, reject any subject mapping other than exact `sub` (or remove that configurable field for human connections while retaining shared workload mapping where needed), and keep optional email/groups mappings separate. Add a focused contract test refusing `subject: "email"` and a callback/real-server proof that two signed tokens from one issuer with different `sub` and the same email create distinct Users without authority transfer.

### TR-002 — INCORRECT: multi-audience ID tokens do not enforce the OIDC authorized party (`azp`)

- Violated obligations: REQ-007, INV-004, and AC-007.
- Exact locations:
  - `crates/shared/wyrd-auth-verify/src/lib.rs:506-563`
  - `crates/wyrd/wyrd-auth/src/callback.rs:222-228`
  - `crates/wyrd/wyrd-server/tests/identity_e2e.rs:3277-3340`
- Evidence: the shared verifier asks `jsonwebtoken` only whether the configured client id appears in `aud`; after decoding, it maps claims without inspecting `azp`. The callback adds only nonce verification. The journey's audience case changes `aud` to one wrong scalar value; it never presents `aud: [wyrd-client-id, other-client]` with missing or mismatched `azp`. Thus a correctly signed, current token from the trusted issuer can pass when Wyrd is one audience but another client is the authorized party.
- Observable consequence: an ID token issued for another client can establish a Wyrd tenant User session when the issuer includes Wyrd as an additional audience, contrary to the exact client/audience binding required for authorization-code login.
- Required testable correction: in the existing shared external-verifier owner, validate OIDC authorized-party semantics after signature/issuer/audience/time verification: when `aud` contains multiple values, require string `azp == expected_audience`; when `azp` is present for a single audience, require the same equality. Add focused verifier cases for missing, mismatched, and matching `azp`, plus one callback refusal case proving no completion or refresh row is written for a mismatched authorized party.

## Verification notes and limits

- Inspected the complete diff and the current bodies/callers for login initiation, callback exchange, external verification, identity resolution, connection activation, refresh rotation, SQL state/completion queries, auth routes, generated auth contracts, CLI retirement, docs, and the four named identity journeys.
- Confirmed the four required ignored journey selectors and their focused setup routing exist in `mise.toml`; inspected their assertions rather than relying on the implementation evidence table.
- No verification command was re-run during this review. The candidate records prior successful identity, principals, SQL, codegen, tenant-isolation, format, and lint lanes, but those results do not cover TR-001 or TR-002 because the relevant inputs are absent from the tests.
- The candidate commit identity was fixed for this review. The untracked review directory is review output and not part of the immutable candidate.
