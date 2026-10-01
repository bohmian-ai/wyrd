# TASK-002 R7 Security/OIDC/RBAC Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `e1ce3c847c14d306c69a704cd15ce6686db9e62e`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation inputs: TASK-002 R1 through R6 and prior validated findings
  `FIND-TASK-002-1` through `FIND-TASK-002-14`

`HEAD` equaled the named candidate before inspection and immediately before
this report was written. Lead-directed reuse and test commits recorded in the
task evidence were treated as authorized rather than scope drift and were
still inspected for security regression.

## Reviewed boundary

This review traced the cumulative tenant-human security boundary from
`POST /auth/login` through tenant and Active-connection selection, durable
one-use state, the common callback, screened discovery/token/JWKS traffic,
ID-token verification, tenant User resolution, provider-group role mapping,
session issuance, sealed completion, refresh rotation/replay/family handling,
and administrative User revocation. It also checked canonical audit coupling,
platform and machine authority-plane separation, and retirement of the public
authorization-code grant and token-bearing callback.

The R7-specific review additionally traced every runtime family-wide refresh
mutation and the lock order shared by refresh rotation, replay containment,
administrative User revocation, and human-connection lifecycle changes.

## Authority and source coverage

| Boundary | Governing authority | Source and caller coverage | Result |
|---|---|---|---|
| Effective tenant and provider selection | SPEC REQ-006/007/015; INV-001/004; TASK-002 packet-local contract; `architecture/wyrd-security-posture.md` | `wyrd-auth/src/login.rs`; `callback.rs:96-287`; login-state SQL and migration; `WyrdPostgres::login_state_tenant`; server login/callback routes | PASS |
| State, PKCE, nonce, redirect, and completion | REQ-007; AC-003/007 | 256-bit state/nonce, PKCE-S256, hashed forced-RLS state, consume-before-provider-IO, deployment callback, sealed binding redemption | PASS |
| Provider network and ID-token trust | REQ-007; INV-004; agent-rules SSRF requirements; security-posture federation and signing-key rules | `ScreenedHttp` callers; fresh discovery; callback algorithm membership; shared verifier signature, key, issuer, audience, expiry, `nbf`, `iat`; nonce and `azp` | PASS |
| Tenant identity and RBAC | REQ-008/014/015; INV-002/003 | exact verified `(issuer, sub)` identity; email display-only; current tenant group mapping; unknown/unmapped roles grant nothing; transactional role replacement and audit | PASS |
| Refresh provenance, replay, and connection cutoff | REQ-016; INV-004; R4/R5 remediations | `refresh.rs:91-224`; tenant-qualified family lock before classification; exact connection revision; family-before-connection lock order; route-owned commit | PASS |
| Administrative User revocation | REQ-016; AC-007; R6 remediation | `revoke.rs:42-67`; live server revoke route; `lock_refresh_family` before suspension/family update; deterministic overlapping-rotation proof | PASS |
| Audit and secret handling | REQ-005/017; canonical audit rules; security-posture secret rules | transactional success/role/replay/revocation audit; redacted refusal audit; `SecretString`/`SecretBearer`; callback `skip_all`; no provider or Wyrd token in callback output | PASS |
| Public bypass retirement and authority-plane separation | TASK-002 callback contract; REQ-013; INV-003 | no `TokenRequest::AuthorizationCode`; fixed redirect/static callback response; tenant, platform, API-key, and workload owners remain distinct | PASS |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required by the approved task.

### Positive Controls

- `login.rs` generates opaque 256-bit state and nonce plus PKCE-S256, stores
  only the state digest, binds the exact connection revision, issuer, client,
  deployment callback, and initiation, and refuses unknown CLI handoffs.
- `callback.rs:96-167` resolves tenant only from the opaque state, consumes and
  commits it before provider IO, exchanges the code with the stored PKCE and
  redirect values, and has no host, path, email, or platform fallback.
- `callback.rs:201-267` checks the freshly advertised algorithm, shared
  asymmetric signature/key/claim policy, nonce, and OIDC authorized party,
  revalidates the Active connection, then commits identity, roles, audit,
  access/refresh issuance, and sealed completion together.
- The shared external verifier refuses HMAC, requires `kid`, pins issuer and
  audience, requires `exp`/`iss`/`aud`, validates `nbf`, and requires a
  non-future numeric `iat` for OIDC ID tokens. Workload assertions retain their
  separate generic verification contract.
- Provider discovery, token exchange, and JWKS use DNS screening and address
  pinning with redirects and proxies disabled plus bounded time and body size.
- Human identity is tenant-RLS scoped and keyed on verified `(issuer, sub)`;
  email never links identities. Only configured groups mapped to existing
  tenant roles grant authority, with no privileged default.
- `refresh.rs:121-218` hashes the presented secret, resolves the stored family,
  takes the tenant-qualified transaction advisory lock before active/stale
  classification, preserves family-before-connection ordering, rejects
  non-human and unbound rows, and transactionally audits replay containment.
- `revoke.rs:50-67` now uses the same family lock after proving the User exists
  and before suspension or family revocation. The route owns the transaction,
  so suspension, retirement, authorization audit, and lock lifetime share the
  commit boundary.
- Callback instrumentation skips all inputs; durable PKCE and bearer values
  are secret-backed; public errors and audit details do not contain provider
  codes, client secrets, access tokens, refresh tokens, or PKCE verifiers.
- The callback returns only a fixed same-origin redirect or static HTML. The
  retired authorization-code token grant and old CLI callback-paste flow are
  absent, so callers cannot bypass initiation-bound completion.

## Prior-finding closure

| Prior finding | Current closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` exact OIDC subject | Human connection input and stored decode require `sub`; identity lookup uses verified issuer and subject, never email. | CLOSED |
| `FIND-TASK-002-2` OIDC `azp` | Authorized-party validation enforces the configured client, including multi-audience tokens. | CLOSED |
| `FIND-TASK-002-3` provider role-change audit | A changed role set stages canonical evidence in the issuance transaction; append failure prevents issuance. | CLOSED |
| `FIND-TASK-002-4` advertised algorithm | Fresh advertised-set membership precedes shared asymmetric signature/key verification and HMAC rejection. | CLOSED |
| `FIND-TASK-002-5` duplicated tenant selection | Forced RLS owns login-state transitions; only the least-disclosure state-owner lookup crosses RLS. | CLOSED |
| `FIND-TASK-002-6` raw-pool boundary | Cross-tenant state ownership remains the narrow inherent `WyrdPostgres` capability. | CLOSED |
| `FIND-TASK-002-7` printable PKCE | Durable login state uses `SecretString`; decoded storage is immediately wrapped and debug-redacted. | CLOSED |
| `FIND-TASK-002-8` Rust documentation | Security-relevant owners and items retain substantive workflow, error, panic, and invariant documentation. | CLOSED |
| `FIND-TASK-002-9` hidden imports | The cited imports remain module-scoped; no security behavior regressed. | CLOSED |
| `FIND-TASK-002-10` algorithm-helper contract | The helper accurately owns advertised membership only and names the shared verifier's enforcement role. | CLOSED |
| `FIND-TASK-002-11` optional binding/time claims | OIDC callers require issuer, audience, expiry, and non-future `iat`, and validate present `nbf`. | CLOSED |
| `FIND-TASK-002-12` replay/rotation race | Refresh operations take the family lock before classification and hold it through successor issuance or family containment and commit. | CLOSED |
| `FIND-TASK-002-13` stale refresh rustdoc | Current documentation matches lookup, family lock, and classification ordering. | CLOSED |
| `FIND-TASK-002-14` administrative revocation race | User revocation takes `lock_refresh_family` before suspension and family update; the focused Postgres overlap proof observes the wait and proves successor `C` is `principal_revoked` with no active family row. | CLOSED |

## Verification evidence and limits

- Static review covered the complete cumulative base-to-candidate diff, the
  approved spec and original task, R1-R6 remediation tasks and prior ledgers,
  security authorities, production bodies and callers named above, migrations,
  public contracts, and relevant focused/journey tests. The repository has no
  `.codegraph/` index, so Git, `rg`, and direct source inspection were used.
- `git diff --check
  3fc085acf5b3a710d5dc80892bd2e664b3db6174..e1ce3c847c14d306c69a704cd15ce6686db9e62e`
  passed. No dependency manifest or lockfile changed in the reviewed range.
- R6 records successful focused administrative-revocation overlap proof,
  connection-deactivation overlap proof, principals unit/integration, SQL,
  the 27-test identity journey lane, tenant isolation, format, lints, and diff
  hygiene. I inspected the proofs and production owners rather than treating
  the evidence table alone as acceptance.
- This bounded review did not rerun Docker/Postgres/Keycloak/Dex or broad Cargo
  lanes. Live Okta and Entra qualification belongs to the change-level
  production qualification task, not this implementation review.
- TASK-003 BFF session completion and TASK-004 durable CLI handoff/storage
  remain approved downstream non-goals. Their TASK-002 primitives and bypass
  refusals were reviewed here.

## Material proposed findings

None. The security/OIDC/RBAC finding ledger is empty.

## Overall result

**PASS** — the cumulative candidate closes `FIND-TASK-002-1` through
`FIND-TASK-002-14` and satisfies the reviewed OIDC, RBAC, tenant-isolation,
refresh-family, audit, secret-handling, and authority-plane obligations. No
reachable exploitable regression or bounded security finding remains.
