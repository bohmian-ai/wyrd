# Security domain review

## Subject and boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `ddd80c7a7b723a2f39a729e5aebe6b6072d2b983`
- Scope: tenant and platform authentication, token verification and revocation,
  principal/credential/role/tenant lifecycle, plane and tenant isolation, and
  transactional authorization audit.
- Authorities: `AGENTS.md` sections 2, 3, 9, 11 and 12;
  `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`;
  `architecture/v1/00-foundations/{service-identity,tenancy}.md`; approved
  `SPEC-admin-principals` revision 10, especially `REQ-005`, `REQ-007`,
  `REQ-010`, `REQ-012` through `REQ-019`, `REQ-031`, `REQ-037`, `INV-003`,
  `INV-004`, `INV-007`, `INV-011` through `INV-013`, and `AC-003` through
  `AC-010`; original tasks 001 through 008; and the R6 remediation packet.

## Authority and source coverage

| Boundary | Source traced | Result |
|---|---|---|
| Tenant access-token verification | `wyrd-auth-verify::TokenVerifier::verify`, production construction in `wyrd-server/src/boot/auth.rs`, `SqlRevocationCheck`, and both admission SQL reads | **FAIL**: the uncached read observes tenant admission and epoch, but not principal status (`SEC-R7-1`) |
| Tenant credential/principal revocation | tenant credential HTTP/MCP owner, `revoke_api_key`, `revoke_service_account_principal`, `revoke_principal_in_conn`, verifier cache-hit and cache-miss paths | **FAIL**: same-second successor tokens are rejected (`SEC-R7-2`), and one stable refusal loses its audit while lookup failures are swallowed (`SEC-R7-3`) |
| Tenant role changes | OIDC role replacement, next-second user epoch advancement, refresh-family handling, role assignment SQL | PASS for reachable changed paths: role withdrawal and successor issuance share the existing ordered epoch mechanism |
| Tenant lifecycle and RLS | `TenantConn`, combined tenant-admission query, tenant suspend/resume, tenant principal routes, cross-tenant resource checks | PASS except where principal status is omitted by `SEC-R7-1`; tenant identity remains derived from verified claims and SQL remains RLS-bound |
| Platform plane | platform extractor, uncached session confirmation, principal/credential/grant reads, `PlatformAuthorization`, status and credential revocation | PASS: token scope is structural, current credential/principal/grant state is re-read, and platform decisions use the audited operator transaction |
| No-effect authorization audit | principal create/issue/list/revoke, tenant status, platform-admin registration, platform credential routes | **FAIL** only for principal-revoke not-found/wrong-kind in `SEC-R7-3`; the R6-named no-effect branches commit their decision without effects |
| Local transfer trust boundary | local upload/download route, tenant-path validator, local URL encoding and client dispatch | PASS: authenticated tenant identity controls the prefix, traversal segments/backslashes are rejected, and SQL/paths are not constructed from unchecked input |
| Injection, secrets and dependencies | changed SQL, URL/path handling, trace annotations, credential DTOs, manifests and lockfile | PASS: changed SQL is parameterized; credential/token material is skipped or redacted; R6 removes cache/listener dependencies and adds no new supply-chain surface |

## Security Audit

### Critical

- None.

### High

- **`SEC-R7-1` — INCORRECT — inactive tenant principals keep using live tokens.**
  [`crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:69`](../../../../../crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs)
  and `:96` read only `tokens_not_before`; they do not read or predicate on the
  principal's `status`, while
  [`crates/wyrd/wyrd-sql/src/queries/cards/delete.rs:148`](../../../../../crates/wyrd/wyrd-sql/src/queries/cards/delete.rs)
  has a reachable served Card-delete path that sets its Service/Agent principal
  to `deleted` without moving the epoch. On a warm-token request,
  [`TokenVerifier::verify`](../../../../../crates/shared/wyrd-auth-verify/src/lib.rs)
  returns the cached principal whenever the unchanged epoch does not reject it;
  a cache miss likewise resolves permissions without making status part of the
  revocation decision. **Exploit path:** an attacker holding a Service or Agent
  bearer token continues calling tenant routes after an administrator deletes
  the backing Card/principal, for up to the token lifetime, defeating the
  operator's containment action. **Impact:** deleted authority remains usable,
  violating `REQ-005`, `REQ-012`, `INV-011`, `INV-013`, `AC-008`, and `AC-010`.
  **Required correction:** extend the existing one-round-trip principal
  admission result to distinguish an active principal from a missing,
  suspended, or deleted one, and make `SqlRevocationCheck` reject the latter on
  both warm and fresh verification paths; add a real-server proof that warms a
  Service/Agent token, deletes its Card, and observes refusal on the next
  request. Do not add another cache, query, listener, or blacklist.

### Medium

- **`SEC-R7-2` — INCORRECT — same-second re-exchange breaks overlap rotation.**
  [`crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:198`](../../../../../crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs)
  stores a sub-second `now()`, while machine access-token `iat` is whole-second
  precision and verification rejects when `iat < epoch`; therefore a surviving
  credential exchanged after revocation but within that same second mints a
  token older than the stored epoch and that token is refused. The journey at
  [`platform_admin_e2e.rs:912`](../../../../../crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs)
  proves only that the sibling credential can exchange, not that the returned
  token authorizes, and its own replay test records this limitation. **Impact:**
  rotation can cause a deterministic short outage, contradicting `REQ-007`,
  `REQ-010`, task 006's uninterrupted-rotation acceptance, and R6's explicit
  requirement that ordered successors remain valid. **Required correction:**
  reuse the existing human role-withdrawal ordering: advance the epoch to a
  representable whole-second boundary and mint a post-revocation machine token
  no earlier than that stored epoch; prove in the real-server rotation journey
  that B's immediately exchanged token reaches a protected route after A is
  revoked, while A's warm predecessor is refused.

- **`SEC-R7-3` — VIOLATION — principal-revoke misses erase the decision and SQL read failures masquerade as not-found.**
  [`crates/wyrd/wyrd-auth/src/revoke.rs:45`](../../../../../crates/wyrd/wyrd-auth/src/revoke.rs)
  and `:57` discard lookup errors with `.ok().flatten()`, and
  [`crates/wyrd/wyrd-server/src/auth/revoke.rs:98`](../../../../../crates/wyrd/wyrd-server/src/auth/revoke.rs)
  appends the allowed decision but returns a stable `PrincipalNotFound` without
  committing it. **Impact:** an authorized probe for an unknown or wrong-kind
  principal leaves no required audit evidence, while a database read failure is
  falsely rendered as a caller-visible 404 and also rolls the audit back;
  operators cannot distinguish an ordinary miss from an unhealthy security
  store. This violates `REQ-037`, `INV-011`, `AC-009`, and the canonical audit
  rule. **Required correction:** propagate lookup/store errors as the existing
  internal failure so the whole transaction rolls back, but commit the already
  appended decision before returning the stable unknown/wrong-kind refusal.
  Add focused Postgres-backed proof for exactly one decision and zero mutation
  on both stable misses, plus an injected lookup failure that commits neither
  decision nor effect.

### Low / Defense In Depth

- None within the approved task; no optional hardening is promoted to a finding.

### Positive Controls

- Production tenant verification now re-reads tenant admission and the
  authorization epoch on every cache hit and miss, and resolver errors fail
  closed.
- Tenant IDs come from verified claims and tenant SQL remains behind
  `TenantConn`/RLS; platform operations remain behind `OperatorPool` and a
  structurally tenantless caller.
- Platform sessions re-read their credential or principal and current grant on
  every request, so platform revocation is not stale-cached.
- Credential verification retains fixed-cost indistinguishable refusals, stores
  only Argon2 verifiers, and does not place bearer material in traces or audit.
- Local transfer paths are authenticated, tenant-prefix checked, and traversal
  validated before filesystem access.

## Verification limits

- This was a review-only static audit; candidate source was not edited and the
  large recorded lane set was not re-run.
- The appended evidence reports the broad required lanes green, but no cited
  proof exercises deletion/status against a warm tenant token, no rotation
  proof spends the immediately returned sibling token after revocation, and no
  principal-revoke proof asserts audit persistence on a not-found/wrong-kind
  result or distinguishes lookup failure.
- Candidate identity remained `ddd80c7a7b723a2f39a729e5aebe6b6072d2b983`
  throughout this review.

## Overall result

**FAIL** — three reachable, bounded security/authentication obligations remain
unsatisfied.
