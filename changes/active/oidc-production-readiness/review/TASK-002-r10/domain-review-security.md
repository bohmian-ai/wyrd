# Security Audit

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `6e21d8ed00d5159ec71e3f2e2414e80fd16f76af`
- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation inputs: `TASK-002-R1` through `TASK-002-R9` and their prior
  verdicts and validated finding ledgers

The candidate remained the named commit throughout this review.
`git diff --check
3fc085acf5b3a710d5dc80892bd2e664b3db6174..6e21d8ed00d5159ec71e3f2e2414e80fd16f76af`
completed successfully.

## Reviewed boundary

This review traced the reachable tenant-human security boundary from anonymous
login initiation through state creation, callback routing, provider calls,
ID-token verification, tenant User resolution, provider-group role
replacement, Wyrd credential issuance, one-use completion redemption, refresh
rotation and replay containment, connection lifecycle cutoff, and
administrative revocation. It also followed the adjacent workload and platform
paths far enough to verify that human federation did not widen either plane.

The review specifically covered:

- state entropy, hashing, initiation binding, PKCE, nonce, exact configured
  redirect, bounded expiry, consume-before-provider-IO behavior, sealed
  completion, and one-use redemption;
- callback tenant and connection selection, including the narrow state-owner
  lookup, forced-RLS state transitions, and the absence of Host, header, email,
  or provider-selected fallback routing;
- screened and address-pinned discovery, token, and JWKS requests with TLS,
  redirects and proxies disabled, body and time limits, and production network
  policy;
- ID-token algorithm advertisement, asymmetric key and signature verification,
  issuer, audience, `azp`, expiry, `nbf`, `iat`, nonce, key refresh, and OIDC
  Subject Identifier validation;
- identity resolution by tenant-scoped `(issuer, sub)`, email non-linking,
  groups-only mapping to existing tenant roles, zero default authority, and
  family-serialized replacement of provider roles;
- exact active-connection revision checks, five-minute access snapshots,
  refresh provenance, rotation, ancestor-replay containment, principal
  revocation, and the shared family-before-connection lock order;
- machine API-key and exact workload-assertion independence, platform/tenant
  plane separation, credential output boundaries, secret redaction, and the
  canonical transactional audit path.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Tenant and connection selection | Spec `REQ-006`, `REQ-007`, `REQ-015`; `INV-001`, `INV-004`; security posture tenant authority | `wyrd-auth/src/login.rs:50-140`, `callback.rs:96-168`; `WyrdPostgres::{resolve_tenant_slug,login_state_tenant}`; login-state migration and SQL; refusal and cross-tenant proofs | PASS |
| State, PKCE, nonce, callback and completion | TASK-002 packet-local contract; spec `REQ-007`; `AC-003`, `AC-007` | `wyrd-spec/src/auth/oidc.rs`; `wyrd-auth/src/login.rs`; `wyrd-sql/src/queries/auth/login_state.rs`; callback HTTP route; identity journeys | PASS |
| Provider network boundary | Agent-rules SSRF rule; security posture source/SSRF controls; spec `INV-004` | `wyrd-auth-oidc/src/screening.rs`, `jwks.rs`, provider discovery; `wyrd-auth/src/callback.rs:340-490`; screening proofs | PASS |
| OIDC code and ID-token verification | Spec `REQ-007`, `REQ-008`; security posture federation policy; OIDC obligations fixed by the approved task | `wyrd-auth/src/callback.rs:205-224,584-633`; `wyrd-auth-verify/src/lib.rs:508-612`; verifier and refusal tests | PASS |
| Identity and RBAC | Spec `REQ-008`, `REQ-014`, `REQ-015`; `INV-002`, `INV-003` | `ensure_user_identity`, `role_names_to_refs`, `replace_user_roles`; callback issuance transaction; same-email, unmapped-user, same-issuer, and concurrent-callback proofs | PASS |
| Credential issuance and renewal | Spec `REQ-016`; TASK-002 refresh provenance contract; security posture access/refresh lifecycle | `wyrd-auth/src/issuance.rs:453-538`, `refresh.rs:75-227`; refresh SQL and migration; provider-switch, replay, and cutoff proofs | PASS |
| Revocation and concurrency | Spec `REQ-016`, `INV-004`; prior findings `FIND-TASK-002-12`, `-14`, `-15`, `-18` | callback family lock at `callback.rs:232-276`; `revoke.rs:16-88`; deterministic rotation, revocation, first-issuance, and two-callback overlap tests | PASS |
| Machine and principal-plane separation | Spec `REQ-013`, `INV-003`; security posture principal planes | token grant routing, workload verifier/binding path, platform login, machine-independence journey | PASS |
| Secrets, responses, logs and traces | Spec `REQ-005`, `REQ-009`, `REQ-011`; security posture cryptography and secret handling | `SecretString` inputs/state, sealed provider secrets and completions, fixed callback responses, redacted `Debug`, scrubbed token-handler span, changed-source log inspection | PASS |
| Canonical audit | Spec `REQ-017`; `AGENTS.md` and agent-rules audit requirements | role-sync and token issuance in one tenant transaction; refresh containment and revocation audit; canonical `append_auth_audit`; audit-failure rollback proofs | PASS |

## Critical

None.

## High

None.

## Medium

None.

## Low / Defense In Depth

None. No optional hardening recommendation is promoted into this acceptance
audit.

## Positive Controls

- Login state uses 32 random bytes, is stored only as a SHA-256 digest, binds
  the exact tenant connection revision, issuer, client, redirect, PKCE
  verifier, nonce, and initiator, and is consumed and committed before the
  authorization code reaches the provider.
- The callback learns the tenant only through the least-disclosure
  `WyrdPostgres::login_state_tenant` capability, then runs every state and
  identity transition through `TenantConn` under forced RLS.
- All provider fetches re-resolve and screen at use time, reject any blocked
  address, pin the accepted address set, preserve TLS hostname verification,
  disable redirects and ambient proxies, and bound request time and decoded
  body size.
- Tenant ID-token verification composes fresh advertised-algorithm membership
  with asymmetric signature/JWKS verification and mandatory issuer, audience,
  expiry, time, subject, nonce, and authorized-party checks before identity or
  authority persistence.
- Human identity is exact verified `(issuer, sub)` within the RLS tenant.
  Email never links identities, provider claims cannot name Wyrd permissions,
  and only configured mappings to existing tenant roles contribute authority.
- The R9 correction takes the existing tenant-qualified User refresh-family
  lock before provider-role replacement and holds it through role audit,
  session issuance, sealed completion, and commit. The deterministic
  `concurrent_callbacks_replace_roles_without_union` proof exercises two
  disjoint callback mappings and checks exact per-token roles and the final
  durable set.
- Initial login, callback role replacement, refresh rotation, replay
  containment, and User revocation share one family lock; issuance and refresh
  preserve the family-before-connection order and re-read current principal,
  role, tenant, and connection state before minting renewable authority.
- Required role-change and token-exchange audits share the establishing tenant
  transaction, so audit failure leaves no role change, refresh row, completion,
  or issued session. Only the canonical audit staging path is used.
- Provider codes, PKCE verifiers, client secrets, Wyrd access tokens, and
  refresh tokens are secret-backed or sealed at their storage boundaries and
  are excluded from the callback response and scrubbed auth-handler tracing.
- Machine API-key exchange still returns no refresh token and workload
  assertions remain bound to the exact configured issuer, subject, audience,
  tenant, and machine principal. A provider token never becomes Wyrd bearer
  authority.

## Prior-finding closure

| Prior finding | Security disposition |
|---|---|
| `FIND-TASK-002-1`–`-4` | CLOSED: exact human `sub`, OIDC `azp`, canonical role-change audit, and advertised algorithm enforcement remain on the production callback path. |
| `FIND-TASK-002-5`–`-7` | CLOSED: login-state transitions rely on forced RLS, cross-tenant state routing stays behind `WyrdPostgres`, and the PKCE verifier remains redacted. |
| `FIND-TASK-002-8`–`-10` | CLOSED; these documentation/import corrections do not weaken the security owners they describe. |
| `FIND-TASK-002-11`–`-12` | CLOSED: mandatory OIDC binding/time claims and family-serialized replay containment remain present. |
| `FIND-TASK-002-13`–`-17` | CLOSED: refresh lookup documentation, revocation/rotation and first-issuance serialization, CLI documentation, and OIDC Subject Identifier validation remain present. |
| `FIND-TASK-002-18` | CLOSED: the callback takes the existing User family lock before role replacement, audit, and issuance; the new deterministic Postgres proof checks that concurrent disjoint mappings cannot union authority. |
| `FIND-TASK-002-19` | CLOSED: runtime and generated contracts agree that refresh tokens are human-only; machine renewal remains credential re-exchange. |
| `FIND-TASK-002-20`–`-21` | CLOSED: the token write handler uses `skip_all`, and the auth router accurately documents its shared admission boundary without introducing credential fields. |
| `FIND-TASK-002-22` | CLOSED: begin-login passes only `TenantSlug` to the narrow `WyrdPostgres::resolve_tenant_slug` capability; raw application-pool selection no longer leaves the owner. |

## Verification limits

- This was a static, review-only security pass. I did not rerun Cargo,
  Postgres, Docker, or provider lanes in the shared checkout.
- The original task and R1-R9 implementation records report green focused
  proofs, including deterministic overlap tests, all four TASK-002 identity
  journeys, the full identity lane (27 journeys), principals unit/integration,
  SQL, codegen, docs, tenant-isolation, raw-pool, client-tier, formatting,
  lint, and cumulative diff checks. I inspected the named R9 concurrency proof
  and its production owners.
- Mock providers prove the continuous protocol paths. Live Okta, Keycloak, and
  Entra qualification under spec `AC-008`, TASK-003 BFF completion, and
  TASK-004 CLI handoff persistence remain outside this bounded TASK-002 review
  and are not treated as defects here.

## Material findings

The proposed security finding ledger is empty.

## Overall result

**PASS** — the cumulative candidate satisfies the original TASK-002
security/OIDC/RBAC obligations, closes `FIND-TASK-002-1` through
`FIND-TASK-002-22` on the security-relevant paths, and introduces no new
material security finding.
