# TASK-002 R8 Security/OIDC/RBAC Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `ced8acaabce7dbe1682bef46d65d073b07e0dbd9`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation inputs: TASK-002 R1 through R7 and prior validated findings
  `FIND-TASK-002-1` through `FIND-TASK-002-15`

`HEAD` equaled the named candidate before inspection and immediately before
this report was written. Lead-directed reuse and test commits recorded in the
evidence tables were treated as authorized rather than scope drift and were
still inspected for security regression.

## Reviewed boundary

This review traced the cumulative tenant-human security boundary from
`POST /auth/login` through tenant and Active-connection selection, durable
one-use state, the common callback, screened discovery/token/JWKS traffic,
ID-token verification, exact external identity, tenant User resolution,
provider-group role mapping, session issuance, sealed completion, refresh
rotation/replay/family handling, and administrative User revocation. It also
checked canonical audit coupling, secret and response handling, platform and
machine authority-plane separation, the public authorization-code grant
retirement, and R7's initial-session serialization against revocation.

## Authority and source coverage

| Boundary | Governing authority | Source and caller coverage | Result |
|---|---|---|---|
| Effective tenant and provider selection | SPEC REQ-006/007/015; INV-001/004; TASK-002 packet-local contract; `architecture/wyrd-security-posture.md` | `wyrd-auth/src/login.rs`; `callback.rs`; login-state SQL and migration; `WyrdPostgres::login_state_tenant`; server login/callback routes | PASS |
| State, PKCE, nonce, redirect, and completion | REQ-007; AC-003/007 | 256-bit state/nonce, PKCE-S256, hashed forced-RLS state, consume-before-provider-IO, deployment callback, sealed binding redemption | PASS |
| Provider network and JWT cryptographic trust | REQ-007; INV-004; agent-rules SSRF requirements; security-posture federation/signing-key rules | `ScreenedHttp`; fresh discovery; callback advertised-algorithm membership; JWKS screening/pinning; shared signature, key, issuer, audience, expiry, `nbf`, `iat`, nonce, and `azp` checks | PASS |
| OIDC subject-claim validity | REQ-007/008/014; INV-002/004; full ID-token claims and exact `(issuer, sub)` identity | `wyrd-auth-verify/src/lib.rs:558-601`; `wyrd-auth-oidc/src/claims.rs:38-62`; callback identity resolution | **FAIL — SEC-R8-001** |
| Tenant RBAC and identity persistence | REQ-008/014/015; INV-002/003 | tenant-scoped issuer/subject identity lookup; email display-only; current tenant group mapping; unknown/unmapped roles grant nothing; transactional role replacement and audit | PASS apart from the invalid-subject admission above |
| Refresh provenance, replay, connection cutoff, and revocation | REQ-016; INV-004; R4/R6/R7 remediations | `refresh.rs`; `issuance.rs:453-538`; `revoke.rs:42-88`; family-before-connection locks; exact connection provenance; route-owned commits; deterministic overlap proofs | PASS |
| Audit and secret handling | REQ-005/017; canonical audit rules; security-posture secret rules | transactional success/role/replay/revocation audit; redacted refusal audit; secret wrappers; callback `skip_all`; fixed callback outputs | PASS |
| Public bypass retirement and authority-plane separation | TASK-002 callback contract; REQ-013; INV-003 | no authorization-code `TokenRequest`; fixed redirect/static callback response; tenant, platform, API-key, and workload owners remain distinct | PASS |

## Security Audit

### Critical

None.

### High

- **SEC-R8-001 — `[crates/shared/wyrd-auth-oidc/src/claims.rs:42]` and
  `[crates/shared/wyrd-auth-verify/src/lib.rs:585]`: an OIDC ID token with an
  invalid empty, non-ASCII, or over-255-byte `sub` is accepted as a tenant User
  identity.**
  - **Classification:** INCORRECT.
  - **Violated obligation:** REQ-007 requires full ID-token claim validation;
    REQ-008, REQ-014, and INV-002 require the exact verified OIDC
    `(issuer, sub)` to be the stable User identity; INV-004 requires claim
    verification to fail closed. An OIDC Subject Identifier is a locally
    unique, never-reassigned identifier of at most 255 ASCII characters.
  - **Exact location and evidence:** `map_claims` treats any JSON string as a
    present subject and returns it unchanged (`claims.rs:42-45,58-62`), including
    `""`, non-ASCII text, and values longer than 255 bytes.
    `ExternalVerifier::verify_external_against` maps those claims after
    signature/registered-claim validation (`lib.rs:549-569`), while the
    ID-token-specific owner validates only numeric, non-future `iat`
    (`lib.rs:585-601`). `AuthorizationCodeExchange::finish_id_token_exchange`
    then passes `verified.subject` to `ensure_user_identity`, whose durable key
    is `(issuer, subject)` (`wyrd-auth/src/callback.rs:213-235,484-509`). No later
    boundary rejects an invalid subject identifier.
  - **Plausible exploit path:** a trusted but faulty or attacker-influenced IdP
    emits signed ID tokens with an empty `sub` for more than one account. The
    first login creates the tenant User keyed by `(issuer, "")` and receives its
    mapped roles; a later distinct IdP account with the same invalid subject is
    resolved to that same User and inherits its durable identity and current
    Wyrd authority. Oversized or non-ASCII values likewise become durable
    identifiers outside the promised OIDC identity contract.
  - **Observable consequence:** distinct provider accounts can collapse onto
    one Wyrd User, producing cross-account authority inheritance despite the
    exact-subject and no-linking guarantees.
  - **Required testable correction:** in the existing shared
    `ExternalVerifier::verify_id_token_against` owner, require the raw `sub` to
    be a nonempty ASCII string no longer than 255 bytes before returning the
    verified identity. Keep workload assertions on `verify_external_against`
    unchanged because their subject mapping is a separate contract. Add a
    focused shared-verifier proof rejecting missing, empty, non-ASCII, and
    256-byte ID-token subjects while accepting a normal subject, plus one
    callback proof that a signed invalid-subject token creates no User,
    refresh row, or login completion. No new type, dependency, persistence
    object, or public contract is needed.

### Medium

None.

### Low / Defense In Depth

None required by the approved task.

### Positive Controls

- Login generates opaque 256-bit state and nonce plus PKCE-S256, stores only
  the state digest, binds the exact connection revision, issuer, client,
  deployment callback, and initiation, and refuses unknown CLI handoffs.
- The callback resolves tenant only from pending state, consumes and commits
  that state before provider IO, uses the stored PKCE verifier and redirect,
  and has no host, path, email, or platform fallback.
- Provider discovery, token exchange, and JWKS fetches use scheme and address
  screening, pinned DNS answers, disabled redirects/proxies, timeouts, and
  bounded bodies. JWT verification rejects HMAC, requires `kid`, pins issuer
  and audience, checks signature/expiry/`nbf`/`iat`, and enforces nonce and
  authorized-party semantics.
- Human connection authoring and stored-row decoding both force the mapping
  path to literal `sub`; email never participates in identity lookup. Only
  configured groups mapped to existing tenant roles grant authority, with no
  privileged default.
- Callback identity, role replacement, canonical audit, access/refresh
  issuance, and sealed completion share one tenant transaction. Required audit
  or completion failure rolls the unit back.
- Refresh rotation, replay containment, first issuance, administrative User
  revocation, and connection lifecycle use the established tenant-qualified
  transaction locks in family-before-connection order. The R7 owner-level lock
  closes the initial-issuance/revocation race without changing machine paths.
- Callback instrumentation skips all inputs; PKCE, provider codes, client
  secrets, access tokens, and refresh tokens are absent from public callback
  output, audit detail, and diagnostic fields inspected here.

## Prior-finding closure

| Prior finding | Current-candidate result |
|---|---|
| `FIND-TASK-002-1` through `-4` | CLOSED: human mapping is fixed to `sub`, `azp` is enforced, role changes use canonical audit, and advertised asymmetric algorithms are enforced. SEC-R8-001 concerns validation of the `sub` value, not selection of its claim path. |
| `FIND-TASK-002-5` through `-10` | CLOSED: login state uses forced RLS plus the narrow owner lookup; PKCE is secret-backed; documented owners and imports remain accurate. |
| `FIND-TASK-002-11` | CLOSED for its diagnosed claims: `exp`/`iss`/`aud` are required, present `nbf` is checked, and OIDC `iat` is numeric and non-future. SEC-R8-001 is the separately required OIDC Subject Identifier validity rule. |
| `FIND-TASK-002-12` through `-14` | CLOSED: family operations serialize before classification and administrative User revocation cannot miss a rotation successor. |
| `FIND-TASK-002-15` | CLOSED: `issue_human_session` now takes the family lock before the connection lock and principal-status read; both initial-issuance/revocation orderings have deterministic Postgres proof. |

## Verification limits

- Static review covered the complete cumulative changed-file inventory, all
  R1-R7 remediation tasks and prior ledgers, applicable authorities, the full
  production bodies and callers named above, both OIDC migrations, and the
  recorded focused and broader evidence. No `.codegraph/` index exists, so
  tracing used Git, `rg`, and direct source reads.
- I did not rerun Cargo, Postgres, Docker, provider, migration, lint, or broad
  identity lanes in this bounded Wave 1 slot. Runtime confidence for passing
  properties relies on inspected deterministic tests and the candidate's
  recorded green commands. SEC-R8-001 is established directly by the current
  claim extractor and verifier control flow and has no existing negative test.
- TASK-003's BFF completion route, TASK-004's CLI handoff persistence, and
  live-provider qualification remain intentional downstream/change-level work.

## Overall result

**FAIL.** The cumulative candidate preserves the reviewed tenant routing,
provider screening, cryptographic verification, RBAC, audit, plane separation,
refresh replay containment, connection cutoff, and R7 issuance/revocation
serialization. One bounded claims-validation defect remains: the shared OIDC
ID-token path accepts a `sub` value that is not a valid OIDC Subject Identifier,
allowing distinct provider accounts to collapse onto one durable tenant User.
The correction belongs in the existing shared ID-token verifier and requires no
specification, architecture, persistence, or public-contract revision.
