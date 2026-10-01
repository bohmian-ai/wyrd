# Security Domain Review

## Boundary

Reviewed the immutable range `3fc085acf5b3a710d5dc80892bd2e664b3db6174..87de451ed87ad059cefd579eb15ef4b028a92547` for tenant human OIDC initiation/callback, state/PKCE/nonce, code exchange, external-token verification, SSRF/DNS pinning, tenant/principal resolution, provider-group RBAC mapping, refresh connection binding, machine-path separation, secret handling, and canonical audit behavior. This is an acceptance review of TASK-002 only, not a broader hardening review.

## Authority and source coverage

| Boundary | Authority | Source/evidence inspected | Result |
|---|---|---|---|
| Server-owned tenant/connection selection | spec REQ-006/007/015; INV-001/004; security posture tenant identity | `wyrd-auth/src/login.rs`, `callback.rs`; `wyrd-sql/.../login_state.rs`, migration; server login/callback adapters | PASS |
| PKCE, state, nonce, redirect and completion secrecy | TASK packet-local contract; REQ-007; AC-003/007 | `wyrd-spec/src/auth/oidc.rs`; auth login/callback; routes; identity journeys | PASS |
| Provider HTTP, SSRF and DNS pinning | agent-rules SSRF rule; security posture SSRF defense; INV-004 | `wyrd-auth-oidc/src/screening.rs`, JWKS fetch, auth discovery/token exchange | PASS |
| ID-token signature/issuer/audience/algorithm/key/time/claims | REQ-007; security posture federation algorithm rule | callback `finish_id_token_exchange`; `wyrd-auth-verify::verify_external_against`; provider metadata | FAIL (SEC-002) |
| Tenant User and RBAC mapping | REQ-008/014/015; INV-002/003 | `ensure_user_identity`, `role_names_to_refs`, `replace_user_roles`, tenant issuance | PASS for authority; audit gap below |
| Connection-bound renewal | REQ-016; TASK renewal contract | refresh consumer, `issue_human_session`, refresh migration | PASS |
| Human/workload/platform separation | REQ-013/015; INV-003 | token grant enum/routes; machine journey; tenant connection owner | PASS |
| Canonical audit/failure behavior | REQ-017; AGENTS/agent-rules audit rules | callback failure audit, issuance audit, role replacement, refusal journey | FAIL (SEC-001) |
| Secret exposure | REQ-005/009; security posture secrets | secret wrappers, sealed completion, callback response/redirect, logs, fixtures | PASS |

## Proposed findings

### SEC-001 — MISSING: provider-driven role changes have no canonical role-change audit evidence

- Violated obligation: SPEC REQ-017 requires role changes to produce redacted canonical audit evidence under their owning authority; TASK-002 requires issuance and audit to commit together.
- Exact location: `crates/wyrd/wyrd-auth/src/callback.rs:244-251` replaces the User's durable roles from verified provider groups. The only event appended later is the generic allowed `auth.token.exchange` at `crates/wyrd/wyrd-auth/src/issuance.rs:414,429-432`; its `AuditDetail::TokenExchange` records principals/delegation/expiry but no prior/new roles or role-change fact (`issuance.rs:583-611`). `crates/wyrd/wyrd-server/tests/identity_e2e.rs:2954-2964` asserts only the generic exchange row.
- Evidence/reachability: every successful OIDC callback reaches `replace_user_roles`; a changed IdP group set can grant or revoke tenant roles on the next login. The retained audit history contains an issuance event but no evidence that assignments changed or what redacted role set became authoritative.
- Observable consequence: operators cannot attribute or reconstruct a provider-driven privilege grant/revocation from canonical audit history, despite the task's explicit audit requirement. This is not speculative: the provider-switch journey changes mapping and issues new authority.
- Testable correction: on the callback transaction that owns `replace_user_roles`, append canonical redacted role-change evidence when the durable assignment changes, before commit, and fail the login closed if that append fails. Reuse the existing canonical append path; do not create another audit sink. Add focused coverage proving changed roles produce the role-change evidence, unchanged roles do not claim a change, and injected append failure leaves roles/session/completion uncommitted.

### SEC-002 — MISSING: callback verification trusts the JWT header's asymmetric algorithm instead of an issuer-approved algorithm set

- Violated obligation: REQ-007 requires ID-token algorithm verification; `architecture/wyrd-security-posture.md:168-170` requires federation tokens only from an explicitly configured issuer, audience, algorithm, and claim mapping.
- Exact location: `crates/wyrd/wyrd-auth/src/callback.rs:222-226` delegates to `verify_external_against`. `crates/shared/wyrd-auth-verify/src/lib.rs:514-543` rejects only HMAC, then creates `Validation::new(header.alg)`. `IssuerVerification` (`crates/shared/wyrd-auth-oidc/src/registry.rs:150-160`) carries no allowed algorithm, while discovery already exposes `id_token_signing_alg_values_supported`. JWKS parsing also ignores JWK `alg` (`crates/shared/wyrd-auth-oidc/src/jwks.rs:46-69`).
- Evidence/reachability: every tenant callback executes this path. A token using any asymmetric algorithm compatible with the selected JWKS key is evaluated under the algorithm named by the untrusted header, even when discovery did not advertise/approve it. The journey's “algorithm” case exercises only HS256 rejection and does not prove issuer-approved asymmetric algorithm pinning.
- Observable consequence: Wyrd can accept an ID token under a provider-unadvertised/unapproved asymmetric algorithm, defeating the closed algorithm policy and widening the trust contract beyond the tested provider configuration. Exploitation requires access to an alternate signing surface/key use at the trusted issuer, so this is a bounded but real federation-policy gap, not a claim that unsigned tokens are accepted.
- Testable correction: bind callback verification to the discovered/qualified provider's supported asymmetric algorithm set (or the equivalent existing trusted-issuer mechanism) and reject a header algorithm outside it before identity resolution. Add a focused callback/verifier test using a valid JWKS-compatible signature under an unadvertised asymmetric algorithm and prove rejection; retain valid advertised-algorithm and rotation coverage.

## Security Audit severity projection

### Critical

None.

### High

None.

### Medium

- SEC-001: missing durable audit attribution for provider-driven privilege changes.
- SEC-002: external ID-token algorithm is not pinned to issuer-approved policy.

### Low / Defense In Depth

None retained; optional hardening was excluded.

### Positive Controls

- Raw state has 256 bits of entropy and only its SHA-256 digest is persisted; consume is atomic and committed before provider IO.
- Tenant/connection/redirect/PKCE/nonce are server-bound; callback accepts no tenant selector or host-derived authority.
- Provider discovery, token, and JWKS calls use bounded, proxy-free, redirect-disabled clients pinned to screened DNS answers; internal/metadata ranges are policy-blocked.
- ID tokens are signature-, issuer-, audience-, key-, expiry-, claim-, and nonce-checked; unknown keys get one bounded refresh and fail closed.
- Completion payloads are sealed at rest and redeemed once by a 256-bit browser-flow hash; callback returns only a fixed same-origin redirect/static page, never Wyrd tokens or provider codes.
- `(issuer, subject)` is the identity key; email does not link users. Unknown role names and unmapped groups grant nothing; tenant RLS remains in force.
- Human refresh rows bind exact connection id/revision and old-connection renewal fails; machine grants remain separate.
- Successful token issuance and its generic exchange audit are atomic; injected audit failure rolls back session, refresh row, roles, and completion.

## Verification limits

- Static source/diff audit only; I did not rerun the 27-test IdP/Postgres journey lane within the sub-review budget. TASK-002 records successful focused runs and broader lanes, but this review independently inspected the named tests rather than treating that table as proof.
- The TASK-003 BFF completion route and TASK-004 CLI handoff owner do not exist in this candidate by approved task boundary; only their sealed redemption primitive/contract was reviewed.
- No live Okta/Entra qualification was expected from TASK-002; controlled Keycloak/Dex/mock evidence was inspected.

## Overall result

FAIL
