# Security Audit

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `fa2bda92a7e79471b79b607870c1e86a9f35639c`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation inputs: `TASK-002-R1` through `TASK-002-R8` in their supplied review directories

The candidate was still the named commit after source inspection and
`git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174..fa2bda92a7e79471b79b607870c1e86a9f35639c` completed successfully.

## Reviewed boundary

The review traced the reachable tenant-human authentication boundary from the
anonymous begin request through state creation, callback routing, provider IO,
ID-token verification, tenant User resolution, role replacement, Wyrd session
issuance, sealed completion redemption, refresh rotation, connection cutoff,
and administrative principal revocation. It also checked the adjacent workload
and platform paths far enough to establish principal-plane separation and to
ensure the shared verifier changes did not make provider assertions Wyrd bearer
authority.

The trace covered:

- login-state entropy, hashing, PKCE, nonce, exact configured redirect, bounded
  expiry, one-use consumption, initiation binding, sealed completion, and
  header-free tenant recovery;
- fresh discovery plus redirect-disabled, proxy-free, bounded, DNS-screened and
  address-pinned discovery, token, and JWKS requests;
- ID-token header algorithm policy, asymmetric signature and key verification,
  issuer, audience, `azp`, expiry, `nbf`, `iat`, nonce, and OIDC Subject
  Identifier validation;
- exact active connection id/revision revalidation under the connection-slot
  lock before authority is minted;
- tenant-RLS identity resolution by `(issuer, sub)`, email non-linking,
  groups-only mapping to existing tenant roles, zero default authority, and
  transactional role-sync and issuance audit;
- tenant-qualified refresh-family serialization across initial issuance,
  rotation, ancestor replay containment, and User revocation, including the
  fixed family-before-connection lock order;
- removal of the public authorization-code token grant and absence of Wyrd
  tokens, provider codes, or state from the successful callback response;
- secret-bearing wire types, PKCE redaction, sealed provider credentials and
  completions, redacted public views, and tracing exclusions.

## Authority and source coverage

| Boundary | Governing authority | Source inspected |
|---|---|---|
| Identity, tenant, and principal planes | `AGENTS.md`; `architecture/wyrd-design.md` auth doctrine; `architecture/wyrd-security-posture.md`; spec `REQ-006`–`REQ-008`, `REQ-013`–`REQ-017`, `INV-001`–`INV-004` | `wyrd-auth/src/login.rs`, `callback.rs`, `issuance.rs`, `refresh.rs`, `revoke.rs`, `connections.rs`, `platform_login.rs`; server auth adapters and routes |
| OIDC and external trust | Spec `REQ-004`, `REQ-007`, `INV-004`; repository SSRF rules; OIDC obligations fixed by the task packet | `wyrd-auth-verify/src/lib.rs`; `wyrd-auth-oidc/src/{provider,jwks,screening,claims}.rs`; human-connection and OIDC wire contracts |
| Persistent state and tenancy | `AGENTS.md` SQL rules; `architecture/agent-rules.md`; security-posture tenant isolation and audit rules | login-state migration; `wyrd-sql` login-state, refresh-token, role-assignment, user, and `WyrdPostgres` owners |
| Lifecycle and audit | Spec `REQ-016`, `REQ-017`, `AC-007`; security-posture token and audit lifecycle | connection mutation/slot locking, issuance, refresh containment, User revocation, canonical auth audit append paths |
| Required proof | `AGENTS.md` test taxonomy; routed testing and spec-driven-development references; original task and R1–R8 remediation proof requirements | four `identity_e2e` TASK-002 journeys; focused verifier, callback, login-state, refresh, issuance, and revocation tests named in the task records |

## Critical

None.

## High

None.

## Medium

None.

## Low / Defense In Depth

None. No optional hardening item is promoted into an acceptance finding.

## Positive Controls

- The callback derives the tenant only from the SHA-256 of at least 256 bits of
  server-generated state through the narrow `WyrdPostgres::login_state_tenant`
  capability, then consumes the forced-RLS row before provider IO. Host,
  forwarded headers, callback paths, email, and provider claims cannot select
  tenant authority.
- PKCE verifier material is a `SecretString`; provider codes and token fields
  use redacted secret wrappers; completed Wyrd sessions are sealed at rest and
  redeemed once by the original browser-flow binding.
- Every provider fetch is re-screened at use time, refuses redirects and
  proxies, rejects any blocked DNS answer, pins the accepted address set,
  preserves TLS hostname verification, limits decoded bodies, and times out.
- Tenant callback verification composes advertised-algorithm membership with
  the shared asymmetric verifier, mandatory `exp`/`iss`/`aud`, present-`nbf`
  validation, bounded non-future numeric `iat`, OIDC `sub` syntax, nonce, and
  `azp` checks before identity or authority persistence.
- The callback revalidates the exact active connection before identity work,
  while `issue_human_session` locks and rechecks that revision in the issuance
  transaction. Replacement, deactivation, and removal therefore fail closed
  across replicas.
- Human identities key only on the verified `(issuer, sub)`. Email cannot link
  identities, provider groups map only to existing tenant roles, unmapped
  subjects receive no privileged role, and no claim names a Wyrd permission.
- Successful role mutation, token issuance, refresh rotation/containment, and
  revocation retain canonical transactional audit coupling. Required audit
  failure on the session-establishing path rolls back User-role, refresh,
  completion, and session effects.
- One tenant-qualified transaction advisory lock serializes all production
  human refresh-family creators and mutators. Initial login, rotation, replay
  containment, and User revocation cannot leave renewable authority outside a
  committed revocation snapshot.
- Workload assertions remain on their exact trusted-issuer and workload-binding
  path, while tenant and platform human principals remain distinct. External
  provider tokens never become Wyrd API bearer tokens.

## Verification limits

This review was static and review-only. It did not rerun the long Postgres,
provider, or full identity lanes in the shared checkout. The supplied task and
R1–R8 implementation records report successful focused RED/GREEN proofs and
successful `test:principals:unit`, `test:principals:integration`, `test:sql`,
`test:identity:journey` (27 journeys), tenant-isolation, codegen where
applicable, format, lint, and diff checks. I inspected the named tests and their
production owners, including the deterministic overlap tests for replay,
revocation, and first issuance. Live Okta, Keycloak, and Entra production
qualification remains the specification-level `AC-008` evidence owned outside
this bounded TASK-002 implementation review.

## Material findings

The validated security finding set is empty.

## Overall result

**PASS** — the cumulative candidate satisfies the TASK-002 OIDC/authentication
security obligations inspected here, and all eight supplied remediation
closures remain present on reachable production paths.
