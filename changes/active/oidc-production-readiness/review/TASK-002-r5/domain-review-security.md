# Security Audit

## Subject and reviewed boundary

Reviewed the immutable range
`3fc085acf5b3a710d5dc80892bd2e664b3db6174..b57d43d501c136591125b98fe78352e657b093b6`
against approved `SPEC-oidc-production-readiness` revision 4, original
`TASK-002`, remediation tasks and validated findings R1 through R4, repository
rules, the runtime-auth design, and the security posture. `HEAD` was the named
candidate before and after review.

The boundary was traced end to end: unauthenticated begin and callback state
binding; tenant and Active-provider selection; screened and DNS-pinned
discovery, token, and JWKS requests; PKCE, nonce, issuer, audience, `azp`,
advertised algorithm, asymmetric signature, key, expiry, `nbf`, `iat`, and
claim verification; exact `(issuer, sub)` identity with no email linking;
provider-group-to-tenant-role synchronization; transactional canonical audit;
sealed one-use completion; refresh provenance, rotation, replay containment,
and family serialization; workload/API-key/platform separation; secret
redaction; and retirement of the public authorization-code grant, token-bearing
callback response, old login route, and CLI callback parser. Lead-directed
reuse and test commits recorded separately in the evidence tables were treated
as authorized and were still inspected for security regressions.

## Authority and source coverage

| Boundary | Governing authority | Source and evidence inspected | Result |
|---|---|---|---|
| Begin routing and effective tenant/provider selection | REQ-006/007/015; INV-001/004; task packet-local contract; security posture tenant-identity rule | `wyrd-auth/src/login.rs:51-139`; `callback.rs:70-297`; `wyrd-sql/src/queries/auth/login_state.rs`; migration `20260925000001_auth_login_state_binding.sql`; `WyrdPostgres::login_state_tenant`; server login/callback handlers | PASS |
| State, PKCE, nonce, redirect, and completion binding | REQ-007; AC-003/007; task packet-local contract | 256-bit state/nonce and PKCE-S256 generation; hashed forced-RLS state; consume-and-commit before provider IO; exact recorded redirect; `verify_nonce`; sealed completion and delete-on-redeem; callback response at `components/auth/routes.rs:274-355` | PASS |
| Provider SSRF screening and pinning | INV-004; `agent-rules.md` SSRF rules; security posture Source/SSRF boundary | `wyrd-auth-oidc/src/screening.rs`; `discover_provider`; `exchange_code_for_id_token`; shared JWKS fetch; production `DeploymentProfile::screened_http` | PASS |
| ID-token signature, key, algorithm, issuer, audience, `azp`, time, and claims | REQ-007; INV-004; R1/R3/R4 remediation; security posture closed federation/crypto policy | `callback.rs:159-220,569-624`; `wyrd-auth-verify/src/lib.rs:484-602`; tenant and platform callers; focused verifier and callback-refusal proofs | PASS |
| Exact human identity and tenant RBAC | REQ-008/014/015; INV-002/003 | exact-`sub` request/stored-row validation; `ensure_user_identity` at `callback.rs:484-509`; email uniqueness removal; mapped-role calculation and tenant-local replacement; same-email/different-subject and unmapped-user proofs | PASS |
| Transactional canonical audit | REQ-017; AGENTS/agent-rules single-path and fail-closed audit rules; security posture audit/privacy | role-sync append at `callback.rs:228-266`; token-exchange append in `issuance.rs:360-450`; canonical `vala.audit_staging`; rollback proof for audit failure; redacted denied outcome | PASS |
| Sealed completion and secret handling | REQ-005/007/009; security posture secret rules | `SecretString` for code, client secret, and PKCE verifier; redacted `Debug`; provider-secret sealing; token completion sealing and bounded redemption; `skip_all` callback instrumentation | PASS |
| Refresh provenance, cutoff, replay, and serialization | REQ-016; INV-004; task renewal contract; R4 remediation | `refresh.rs:84-227`; `refresh_tokens.rs:27-120,165-228`; exact connection provenance; family-first then connection-slot lock; route-owned success/reuse commit at `components/auth/routes.rs:219-258`; deterministic ancestor-replay overlap proof | PASS |
| Human, workload, tenant, and platform separation | REQ-013/015; INV-003; runtime-auth design | public `TokenRequest`; human-only refresh issuance; unchanged API-key/JWT-bearer paths; platform-specific verifier/session path; machine-independence journey | PASS |
| Public route and grant retirement | task packet-local contract; no compatibility aliases | `TokenRequest` has no `AuthorizationCode`; callback returns only fixed redirect/static HTML; no public completion-redemption route in TASK-002; removed client/CLI login flow; served OpenAPI contract and schema snapshots | PASS |

## Critical

None.

## High

None.

## Medium

None.

## Low / Defense In Depth

None. Optional hardening and behavior owned by TASK-003/TASK-004 were excluded.

## Material proposed findings

None. No reachable security defect remains within the approved TASK-002
boundary.

## Prior finding closure

| Prior finding | Independent result |
|---|---|
| `FIND-TASK-002-1` — mutable mapped human subject | CLOSED: both connection input and stored-row decode require exact `sub`; the callback keys identity only by verified issuer and subject. |
| `FIND-TASK-002-2` — missing OIDC `azp` semantics | CLOSED: `verify_authorized_party` rejects absent `azp` for multiple audiences and every present mismatch before identity resolution. |
| `FIND-TASK-002-3` — missing provider-role-change audit | CLOSED: a changed durable role set stages one canonical `auth.user.roles.sync` event in the issuance transaction; an append failure rolls back roles, tokens, refresh state, and completion. |
| `FIND-TASK-002-4` — unpinned ID-token algorithm | CLOSED: fresh discovery constrains the header algorithm, then the shared verifier independently refuses HMAC and verifies the asymmetric JWKS key/signature. |
| `FIND-TASK-002-5` — duplicate tenant predicates | CLOSED: login-state transitions rely on forced RLS; cross-tenant ownership is not caller-selectable. |
| `FIND-TASK-002-6` — raw-pool state-owner lookup | CLOSED: the cross-RLS lookup is a narrow inherent `WyrdPostgres` capability over its private app pool and returns only the owning tenant for an unconsumed, unexpired hash. |
| `FIND-TASK-002-7` — printable PKCE verifier | CLOSED: public `LoginState` carries `SecretString`; the private decoded string is wrapped immediately and the debug regression proof remains. |
| `FIND-TASK-002-8` — incomplete Rust documentation | CLOSED for the security boundary: the reviewed owners, invariants, errors, transaction ownership, and partial-progress semantics are documented. |
| `FIND-TASK-002-9` — hidden function imports | CLOSED: the cited imports remain at module scope. |
| `FIND-TASK-002-10` — false algorithm-helper documentation | CLOSED: current rustdoc limits the helper to advertised-set membership and correctly assigns HMAC/signature enforcement to the shared verifier. |
| `FIND-TASK-002-11` — optional ID-token binding/time claims | CLOSED: `verify_external_against` requires `exp`/`iss`/`aud` and validates present `nbf`; `verify_id_token_against` additionally requires numeric, non-future `iat`, and both tenant and platform OIDC callers use it while workload assertions retain the generic contract. |
| `FIND-TASK-002-12` — replay containment race | CLOSED: each stored tenant-principal refresh family takes one transaction advisory lock before active/stale classification and holds it through successor issuance or family revocation, audit, and the route-owned commit; lock ordering is family then connection. |

## Positive Controls

- Raw login state and nonce use OS randomness; only the SHA-256 state digest is persisted, and state is atomically consumed before provider IO.
- The callback accepts no tenant selector. Tenant, connection revision, issuer,
  client, redirect URI, PKCE verifier, nonce, and completion binding come only
  from server-owned state and are revalidated against the Active connection.
- Provider requests resolve once, reject any blocked address, pin the accepted
  addresses while preserving TLS hostname validation, disable proxies and
  redirects, bound total time and decoded response size, and always block
  metadata/link-local destinations.
- Tenant ID tokens are checked against fresh advertised algorithms and the
  shared JWKS verifier, including required issuer/audience/expiry, optional
  `nbf`, required `iat`, nonce, and authorized-party semantics.
- Human identity uses exact verified `(issuer, sub)` within forced tenant RLS;
  email is display data only. Unmapped groups and unknown tenant roles grant no
  authority, and human connection default roles are not applied.
- Successful issuance, role mutation evidence, refresh rotation, and replay
  containment use the canonical transactional audit path and fail closed when
  required audit cannot be staged.
- Completed credentials are sealed at rest and redeemed once by their original
  initiation binding; callback output contains neither provider code nor Wyrd
  token.
- OIDC user refresh rows retain exact connection id/revision provenance;
  replacement or deactivation blocks successors while already-issued access
  tokens retain only their bounded snapshot lifetime.
- API-key and workload grants issue no human refresh token and do not inherit
  provider roles; platform OIDC remains a separate tenantless plane.

## Verification limits

- This was a bounded static cumulative audit. I inspected the approved
  specification/task, applicable repository/security authorities, all R1-R4
  finding ledgers and remediation tasks, the complete changed-file inventory,
  and the full production bodies/callers named above. The repository has no
  `.codegraph/` index, so Git, `rg`, and direct source inspection were used.
- I did not rerun expensive Cargo, Postgres, Docker, Keycloak/Dex, or broad
  lanes. The candidate records successful exact R4 verifier/concurrency/refusal
  proofs and the broader principals, SQL, tenant-isolation, identity, format,
  and lint lanes. I inspected those tests and their exercised owners rather
  than accepting the evidence table alone.
- `git diff --check` passed for the immutable range. No current source or
  verification artifact was modified by this review.
- TASK-003's authenticated BFF completion route and TASK-004's durable CLI
  handoff owner remain approved non-goals. Their TASK-002 primitives—sealed,
  initiation-bound completion and refusal of unknown CLI handoffs—were in scope
  and passed.
- Live Okta/Entra qualification belongs to later plan tasks; controlled
  Keycloak/Dex/mock evidence is sufficient for this implementation boundary but
  is not a production-provider support claim.

## Overall result

**PASS** — the current candidate closes all prior security findings and no new
material security/OIDC/RBAC/trust-boundary finding was demonstrated.
