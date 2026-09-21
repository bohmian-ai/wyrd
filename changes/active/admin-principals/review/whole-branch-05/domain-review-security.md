# Security, RBAC, authentication, federation, revocation, and audit domain review

**Overall: FAIL**

## Immutable subject

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `c9e1092bbdb4df3781eb91b0eb33150e00df7623`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 10,
  SHA-256 `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md` through
  `TASK-008-*.md`
- Prior review and remediation:
  `changes/active/admin-principals/review/whole-branch-04/{verdict.md,findings-validation.md,domain-review-security.md,TASK-001-008-R4-close-cumulative-findings.md}`

The candidate and specification checksum were rechecked before this report was
written. Product source was not modified.

## Reviewed boundary

This review traced the complete cumulative security boundary across both
administrative planes: principal and credential persistence, tenant API-key
exchange, human OIDC login and refresh rotation, platform credential and OIDC
session issuance, per-request token extraction and verification, tenant
admission and authorization epochs, platform session anchoring, permission and
last-administrator decisions, tenant isolation, canonical audit staging and
attribution, CLI credential use, and MCP's shared authentication/replay path.

The cumulative base-to-candidate diff and the R4 remediation delta were both
inspected. Prior findings were treated as leads only; closure was re-evaluated
against current source and reachable callers.

## Authority and source coverage

| Boundary | Governing authority | Source/caller coverage | Result |
|---|---|---|---|
| Principal and credential model | `REQ-001`-`REQ-011`, `INV-001`-`INV-003`; service-identity authority | principal-generic tenant/platform stores, Argon2 issue/verify, metadata/list/revoke paths, root initialization/recovery | PASS for inspected paths |
| Tenant token issue, refresh, and verification | `REQ-012`-`REQ-015`, `REQ-048`, `INV-011`-`INV-013`; security foundation | `/auth/token` -> API-key/delegation/refresh owners -> `TokenVerifier` -> `SqlRevocationCheck` -> tenant extractors | PASS except `SEC-WB05-1` |
| Tenant OIDC and role authority | `REQ-034`-`REQ-035`, `INV-013`; security posture | callback verification/nonce -> identity resolution -> role replacement -> epoch -> session issue -> protected route | **FAIL — `SEC-WB05-1`** |
| Platform credential and OIDC entry | `REQ-041`-`REQ-046`, `REQ-037`, `AC-009`; R4 finding 5 correction | `/auth/platform/token`, platform callback -> identity lookup/pin -> platform session issue -> canonical staging | **FAIL — `SEC-WB05-2`** |
| Authorization and audit atomicity | `REQ-016`-`REQ-019`, `REQ-037`, `AC-009`; `AGENTS.md` and agent-rules audit invariants | tenant admin authorization, `PlatformAuthorization`, effect/refusal branches, `vala.audit_staging` append | **FAIL — `SEC-WB05-3`** |
| Plane and tenant isolation | `REQ-018`, `REQ-031`, `INV-004`, `INV-007`, `INV-015`; tenancy authority | tenant/platform claim shapes and extractors, `TenantConn` RLS, `OperatorPool`, cross-plane route callers | PASS for inspected paths |
| Revocation, suspension, and last-admin protection | `REQ-005`, `REQ-028`, `INV-005`, `INV-013` | credential/principal revocation, refresh-family retirement, per-request platform anchors, serialized usable-admin count | PASS except the same-second role-withdrawal race in `SEC-WB05-1` |
| OIDC network and secret boundary | `REQ-043`-`REQ-046`, `INV-002`, `INV-011`-`INV-012`; SSRF authority | discovery, token exchange, JWKS refresh, resolved-address screening/pinning, redirect refusal, sealed/redacted client secrets | PASS for inspected paths |
| CLI and MCP authentication | `REQ-047`, `INV-015`, `AC-013`-`AC-014` | CLI shared-client construction; MCP `WyrdMcpHttpClient` bearer injection, one forced refresh, one replay | PASS for inspected paths |

## Security Audit

### Critical

- None.

### High

- **`SEC-WB05-1` — INCORRECT — [`crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:87`](../../../../../crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs) truncates the role-change epoch to whole seconds, so a pre-change token minted in that same second survives.**
  **Violated obligation:** approved-spec `INV-013` requires revoking a role grant
  to take effect no later than the next request; R4 finding 3 specifically
  requires the old OIDC token to be refused after a real role-set change.
  **Evidence:** `advance_user_epoch_to_second` stores
  `date_trunc('second', now())` at lines 105-119. Access-token `iat` is also
  whole-second precision, and `TokenVerifier::verify` revokes only when
  `token.iat < epoch`, not when equal. The callback calls this helper before
  issuing the reduced-authority successor at
  `crates/wyrd/wyrd-auth/src/callback.rs:219-243`. Therefore an old token issued
  earlier in the same wall-clock second has `iat == epoch` and is admitted.
  The real-provider journey is too slow to force this boundary and does not pin
  same-second issuance. **Exploit path:** a user holding `runtime_admin` obtains
  an access token, has the IdP group withdrawn within the same second, and
  completes the next login/callback before that second rolls over. The old
  bearer still verifies with its signed privileged role and can exercise that
  authority until its normal 15-minute expiry. **Observable consequence:** a
  completed role withdrawal does not stop the old authorization on the next
  request, leaving a bounded but real privilege-escalation window.
  **Required testable correction:** make the epoch/claim ordering unambiguous at
  the precision actually carried by access tokens, so every pre-change token is
  strictly before the successor while the successor remains admissible. Reuse
  the existing epoch and token-issuance owners; do not add a blacklist or second
  cache. Add a deterministic test that fixes both old-token issuance and role
  replacement inside one second, then proves the old token is refused and the
  successor authenticates with reduced authority. Preserve unchanged-login
  stability.

### Medium

- **`SEC-WB05-2` — INCORRECT — [`crates/wyrd/wyrd-auth/src/platform_login.rs:258`](../../../../../crates/wyrd/wyrd-auth/src/platform_login.rs) commits first-login identity pinning outside the audited platform-session grant transaction.**
  **Violated obligation:** `REQ-037`/`AC-009` require attributable, fail-closed
  administrative audit, and the approved R4 finding 5 correction explicitly
  requires platform identity pin, canonical append, and commit to be one grant
  boundary. **Evidence:** `resolve_principal` calls pool-scoped
  `pin_platform_identity` at lines 278-305; that query autocommits through
  `OperatorPool`. Only afterwards does `PlatformSessions::issue_federated` open
  a new audited transaction, append `auth.token.exchange`, and commit at
  `platform_sessions.rs:195-217`. **Observable consequence:** if canonical
  audit staging or session issuance fails after first-login pinning, the request
  returns no token but the permanent `(issuer, subject)` binding survives with
  no committed grant event. A retry now follows the already-pinned path, so the
  durable identity transition cannot be reconstructed from the canonical
  record the change requires. **Required testable correction:** move the
  existing subject lookup/pin, active-principal check, session mint, and
  canonical append onto one `begin_platform_audited` transaction owned by the
  platform login/session workflow. Add no new audit table or service. Inject
  append failure on an unpinned registration and prove no subject/pinned time
  commits and no token returns; then prove a successful retry commits exactly
  one pin and one credential-free exchange event.

- **`SEC-WB05-3` — INCORRECT — [`crates/wyrd/wyrd-server/src/components/platform/credentials.rs:230`](../../../../../crates/wyrd/wyrd-server/src/components/platform/credentials.rs) drops an already-appended authorization decision on logical refusal paths.**
  **Violated obligation:** `REQ-037`, `AC-009`, and the repository audit rule
  require every permission evaluation, allowed or denied, to commit its
  canonical row; the route's own contract says a missing-credential probe
  commits because the attempt is security evidence. **Evidence:**
  `PlatformAuthorization::authorize` returns an open transaction already
  carrying an allowed `platform.authz` row. The unknown/wrong-owner branch at
  lines 221-232 returns `404` without committing it, and the concurrent/already-
  revoked branch at lines 234-247 does the same. Dropping `decision` rolls the
  row back. The same shape exists on the tenant administration surface at
  `crates/wyrd/wyrd-server/src/components/admin/routes.rs:688-698`, where an
  allowed workload-binding deletion appends its decision and then returns
  `404` without committing when no row was removed. **Observable consequence:**
  an authorized caller can probe or replay administrative deletes and receive
  stable responses while leaving no durable evidence that Wyrd evaluated the
  privileged permission. This creates a deliberate blind spot in incident
  reconstruction and contradicts the fail-closed single audit path.
  **Required testable correction:** in each existing owner, commit the already-
  appended decision before returning a stable logical refusal that performs no
  effect, while keeping store failures and partial mutations rollback-coupled.
  Prefer a shared local return pattern only where one already exists; do not add
  another audit abstraction. Add real-Postgres tests for unknown/wrong-owner and
  replayed platform credential revoke plus absent workload-binding delete,
  asserting one committed decision row and no resource mutation.

### Low / Defense In Depth

- None. Optional hardening and unrelated pre-existing debt were excluded.

### Positive Controls

- Tenant revocation-store uncertainty now invalidates cached positives and
  returns the stable verifier-unavailable refusal on both cache-hit and
  cache-miss paths.
- User principal revocation advances the epoch and retires the refresh family
  in one `TenantConn`; refresh replay containment commits and attributes the
  consumed refresh row.
- Tenant API-key, delegated, human refresh/login, platform credential, and
  platform federated success paths stage canonical token-grant events before
  returning tokens; credential-backed grants carry the non-secret credential
  id.
- Platform sessions are claim-separated from tenant tokens and re-read their
  credential or principal plus current platform grant on every request. Tenant
  operations remain RLS-bound through `TenantConn`.
- Invalid tenant and platform credentials perform fixed-cost Argon2
  verification and converge on non-enumerating public failures. Plaintext
  credentials and OIDC secrets remain redacted and verifier-only at rest.
- OIDC provider requests use resolved-address screening, pin the connection to
  the screened addresses, reject redirects, and always block metadata/link-local
  ranges.
- Initialization discloses and flushes the root credential before committing,
  so a failed output leaves the deployment uninitialized and retryable.
- MCP now reuses the configured `WyrdClient` HTTP/auth owners and bounds reactive
  renewal to one forced re-exchange and one replay.

## Verification limits

This was a review-only static audit. No source was edited and no Cargo,
Postgres, OIDC-provider, CLI, or MCP test was rerun. The R4 packet records the
required final lanes as green, and those results are credible for their selected
scenarios; they do not close the three source-level gaps above:

- the role-withdrawal journey does not deterministically keep issuance and
  epoch advancement in the same second;
- the platform session audit tests begin after identity resolution and do not
  inject append failure around an unpinned first login; and
- existing authorization tests do not assert committed audit rows on the named
  post-allowance `404` branches.

The candidate remained
`c9e1092bbdb4df3781eb91b0eb33150e00df7623` at the final integrity check.
All findings are bounded implementation defects within approved behavior; none
requires a new product, public API, architecture, or persistent-data decision.

## Overall

**FAIL.** Proposed finding IDs: `SEC-WB05-1`, `SEC-WB05-2`, and
`SEC-WB05-3`.
