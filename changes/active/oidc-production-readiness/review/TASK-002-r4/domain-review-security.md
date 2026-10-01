# Tenant OIDC security domain review

## Subject and reviewed boundary

Reviewed the immutable range
`3fc085acf5b3a710d5dc80892bd2e664b3db6174..63e6a545db156a095b670a5bb8bc36f6f36fba32`
against approved `SPEC-oidc-production-readiness` revision 4, original
`TASK-002`, and remediation tasks/review evidence from R1, R2, and R3. `HEAD`
was the candidate before and after inspection.

The boundary was the complete tenant human OIDC flow and its security effects:
begin routing; state, PKCE, nonce, callback and code exchange; provider
discovery/JWKS and screened HTTP; ID-token signature, issuer, audience,
authorized party, algorithm, key and time checks; tenant/connection binding;
`(issuer, sub)` identity; provider-group role sync; audit atomicity and
redaction; sealed one-use completion; refresh cutoff/replay; machine-plane
separation; sealing rotation; and the lead-directed R4 test cleanup and added
advertised-HS256 proof. This is an acceptance audit, not speculative
hardening.

## Authority and source coverage

| Boundary | Governing authority | Source and evidence inspected | Result |
|---|---|---|---|
| Tenant and connection selection | REQ-006/007/015; INV-001/004; security posture tenant-identity rules | `wyrd-auth/src/login.rs`, `callback.rs`, `connections.rs`; server login/callback/routes; login-state migration and queries | PASS |
| State, PKCE, nonce, callback and completion secrecy | TASK packet-local contract; REQ-007; AC-003/007 | `wyrd-spec/src/auth/oidc.rs`; 256-bit generators and PKCE challenge; hashed forced-RLS state; consume-before-provider-IO; sealed one-use completion; fixed redirect/static callback responses | PASS |
| Provider IO, SSRF and DNS pinning | INV-004; security posture and architecture-constraints external-network rules | shared OIDC discovery, `ScreenedHttp`, bounded response and JWKS paths; proxy/redirect refusal, all-address screening and address pinning | PASS |
| ID-token algorithm, signature and key | REQ-007; security posture federation/cryptography rules | callback advertised-set check; shared verifier HMAC refusal, `kid` lookup, bounded JWKS refresh and signature verification; R4 mixed EdDSA/HS256 journey case | PASS |
| ID-token issuer, audience, authorized party, claims and time | REQ-007; INV-004; security posture explicit federation binding | callback `finish_id_token_exchange`; `ExternalVerifier::verify_external_against`; nonce/`azp` checks; refusal journey | **FAIL (SEC-R4-001)** |
| Tenant identity and RBAC | REQ-008/014/015/017; INV-002/003 | exact-`sub` authoring and stored-row checks; tenant-local identity upsert; mapped-role replacement; canonical role-sync event and rollback proof | PASS |
| Refresh cutoff and replay | REQ-016; TASK renewal contract; security posture refresh rules | refresh consume/rotation/family revocation; exact connection provenance and slot lock; migration revocation of unbound human rows | PASS |
| Audit transactionality and redaction | REQ-017; AGENTS audit rules; security posture audit/privacy rules | callback role-sync and token-exchange appends in the issuance transaction; denied outcome path; redacted event details; audit-failure rollback tests | PASS |
| Human, workload and platform separation | REQ-013/015; INV-003 | public token grant enum/routes; human-only refresh issuance; workload/API-key paths; machine-independence journey | PASS |
| Secret sealing and rotation | REQ-005/009; security posture secret rules | `SecretString` code/verifier handling; provider-secret and completion sealing; rotation/rewrap owner and completion-retention contract | PASS |

## Material proposed finding

### SEC-R4-001 — INCORRECT: validly signed ID tokens may omit issuer, audience, and issued-at claims, and future time gates are not enforced

- **Violated obligation:** SPEC REQ-007 requires verified ID-token issuer,
  audience, time, and claims; the task requires full ID-token verification
  before resolving identity; INV-004 and the security posture require external
  federation to fail closed against the explicitly configured issuer and
  audience.
- **Exact location:**
  `crates/shared/wyrd-auth-verify/src/lib.rs:538-550`, reached from
  `crates/wyrd/wyrd-auth/src/callback.rs:212-219`. The applicable
  `jsonwebtoken 9.3.1` `Validation::new` contract defaults
  `required_spec_claims` to only `exp`, leaves `validate_nbf` false, and checks
  configured issuer/audience only when those claims are present.
- **Evidence:** the callback supplies `set_issuer` and `set_audience` but never
  adds `iss` or `aud` to `required_spec_claims`. It never requires or validates
  OIDC's `iat` claim, and it leaves an optional future `nbf` ignored. Mapping
  later requires `sub`, while nonce and `azp` are checked separately, so none
  of those later steps closes the missing issuer/audience/time gap. The journey
  at `identity_e2e.rs:3353-3456` proves wrong `iss` and wrong `aud`, but has no
  missing-claim or future-time case; every fixture token includes `iat`.
- **Reachability:** every successful tenant callback uses this shared verifier
  before `(issuer, sub)` lookup, role mapping, refresh issuance, and sealed
  completion. An attacker who can obtain a signature from the configured
  issuer/key over a token lacking `iss` or `aud` can pass both configured-value
  checks because absence is not an error; a token without `iat`, or with a
  future `iat`/`nbf`, can likewise establish a Wyrd session if the remaining
  claims pass.
- **Observable consequence:** the callback can issue tenant authority from an
  ID token that does not prove the exact issuer/audience claim binding and does
  not satisfy the required OIDC time-claim contract. This widens acceptance
  beyond the configured federation trust boundary.
- **Testable correction:** in the shared external verifier, reuse
  `jsonwebtoken::Validation` to require `exp`, `iss`, and `aud`, enable `nbf`
  validation when present, and explicitly require a numeric `iat` that is not
  in the future beyond the existing allowed clock skew. Keep `sub` ownership in
  the existing exact claim-mapping path and keep nonce/`azp` in the callback;
  do not add a second verifier. Extend the existing callback-refusal journey
  (or focused shared-verifier proof where clock shaping is materially clearer)
  with validly signed tokens missing `iss`, missing `aud`, missing `iat`, and
  carrying future `iat`/`nbf`, proving refusal before any completion, User,
  roles, refresh row, or session is committed.

## Prior-finding and R4 change assessment

Prior findings `FIND-TASK-002-1` through `FIND-TASK-002-10` remain closed. In
particular, the R4 rustdoc now accurately assigns advertised-set membership to
the callback helper and symmetric-algorithm refusal to the shared verifier.
The added mixed `EdDSA`/`HS256` journey case reaches that composition and
proves it refuses the token without a completion. The gateway test-helper
rewrite has no authentication, tenancy, credential, or audit effect.

## Verification limits

- Static cumulative source/diff audit only. I did not rerun the recorded
  Postgres/Keycloak/Dex lanes. The task records successful focused journeys,
  the 27-test identity lane, principals, SQL, codegen, boundary, format and lint
  lanes; those checks do not include the missing-claim/future-time cases above.
- The TASK-003 BFF completion route and TASK-004 CLI handoff owner remain
  approved non-goals. Their server-owned sealed redemption primitive was in
  scope and inspected.
- No live Okta/Entra qualification was expected from TASK-002. Controlled
  provider evidence cannot compensate for the reachable validation gap.

## Overall result

**FAIL** — one material, bounded trust-boundary defect remains:
`SEC-R4-001`.
