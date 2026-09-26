# TASK-002 R7 Structured Ponytail Findings Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `e1ce3c847c14d306c69a704cd15ce6686db9e62e`
- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation authority: TASK-002 R1 through R6, their prior verdicts, and
  stable findings `FIND-TASK-002-1` through `FIND-TASK-002-14`

`HEAD` equaled the candidate before and after source inspection. The untracked
R7 review directory is outside the immutable subject. Lead-directed reuse and
test commits recorded separately in the task evidence were treated as
authorized rather than scope drift and were still inspected for regression.

## Review-input completeness and method

The approved specification revision, original task, R1-R6 remediation tasks,
prior validated ledgers and verdicts, repository rules, applicable security,
Rust, SQL-tenancy, testing, and spec-driven authorities, cumulative changed-file
inventory and diff, current source and callers, relevant concurrency tests, and
all four R7 Wave 1 reports were inspected. The repository has no `.codegraph/`
directory, so caller tracing used Git, `rg`, and direct full-body inspection.

The Wave 1 union contains one proposed finding, `TD-R7-001`. Validation traced
every caller of `TenantTokenIssuer::issue_human_session`,
`lock_refresh_family`, and `insert_human_refresh_token`, plus the complete
callback, refresh, issuance, User lookup, administrative-revocation, SQL lock,
family-update, and route commit bodies. It also distinguished first issuance
from rotation: rotation already enters the issuer while holding the family
lock, while callback issuance enters it with no family lock.

## Wave 1 disposition

| Wave 1 report | Proposed ledger | Validation disposition |
|---|---|---|
| `task-review.md` | Empty | No separate proposal to validate. Its acceptance conclusion omitted the initial-issuance/revocation interleaving retained below. |
| `standards-review.md` | Empty | Confirmed empty for repository-rule-only findings. The retained defect is a reachable concurrency/task-contract failure, not a separate source-layout or boundary violation. |
| `domain-review-security.md` | Empty | Confirmed empty for its inspected OIDC, RBAC, trust, audit, and secret-handling paths. The retained defect is independently established from the security-posture next-issuance rule through the tenancy/concurrency trace. |
| `domain-review-tenancy-data.md` | `TD-R7-001` | **CONFIRMED** and retained as `FIND-TASK-002-15`. |

## Validation of `TD-R7-001`

### Reachability and complete caller trace

`TenantTokenIssuer::issue_human_session` has two production callers. The
callback calls it with `rotated_from = None` after resolving the existing or
new User and replacing roles. `RefreshTokens::execute` calls it with
`rotated_from = Some(active.id)` after resolving the stored row, acquiring
`lock_refresh_family`, and consuming the active row under that lock. Its other
callers are focused tests.

The shared issuer takes `lock_human_connection_slot`, verifies the exact Active
connection revision, and calls `TenantTokenIssuer::issue`. Human issuance then
reads the User with an ordinary `SELECT`, refuses a non-Active status, resolves
current grants, signs an access token, and appends issuance audit. Only after
that read does `issue_human_session` mint and insert the human refresh row.
Neither the first-issuance caller nor the shared issuer acquires the refresh
family lock.

Administrative User revocation is reached through the live
`POST /v1/principals/{id}/revoke` route. Its route-owned transaction appends the
authorization decision, calls `revoke_principal_in_conn`, and commits on
success. The User branch proves existence, takes the same tenant-qualified
family lock used by refresh rotation, suspends the User, and revokes all family
rows visible to its update before the route commits.

For an existing User, an in-flight callback can therefore read status
`active`; administrative revocation can then acquire its uncontended family
lock, suspend the User, update the then-visible family, and commit; the callback
can subsequently insert and commit a new active refresh row because the insert
has no User-status predicate or constraint. The callback transaction also
commits the newly signed access token, sealed completion, role state, and audit
after revocation has returned. This path is reachable without a dormant helper,
test-only call, or speculative future feature.

The R6 overlap proof cannot exercise this interleaving. Its rotation owns the
family lock before it calls the shared issuer, so revocation waits and its
post-wait update sees the successor. First issuance has no predecessor from
which to derive a lock and currently takes only the connection-slot lock.
Connection locking does not help because administrative User revocation does
not mutate or lock the human connection.

### Authority and consequence

The security posture requires suspending a principal to refuse its next
issuance immediately, while allowing only already-issued access tokens to keep
their bounded five-minute snapshot. The live revocation route likewise promises
that suspension prevents new token minting. TASK-002 owns human callback
issuance, requires trust and renewal failures to fail closed, and requires one
shared human refresh path. R4-R6 establish the existing tenant-qualified family
lock as the serialization mechanism for renewable-authority mutation and fix
the order as family before connection.

The missing lock permits new access and renewable refresh authority to commit
after administrative revocation has committed and returned. The refresh row
remains durably active even while its User is suspended and can become usable
again after reactivation. This is not the excluded promise to invalidate an
already-issued access token instantly; it is a new issuance committed after
revocation from a stale pre-revocation status read.

### Ponytail correction boundary

Deletion does not preserve the task: human callback must issue a renewable
session, and administrative revocation must suspend the User and retire its
renewable authority. No new abstraction is needed. The repository already has
the exact transaction-scoped, tenant-qualified advisory lock and both competing
operations already use the same `TenantConn` transaction model.

The minimum safe correction is to acquire
`lock_refresh_family(conn, "user", principal_id)` at the start of
`TenantTokenIssuer::issue_human_session`, before
`lock_human_connection_slot` and before `Self::issue` reads principal status.
This places the invariant at the one production owner that inserts every human
refresh row and preserves the established family-before-connection order.
Rotation may reacquire the same transaction advisory lock after its caller has
already acquired it; PostgreSQL transaction advisory locks are reentrant for
the owning session and remain held until transaction end. Keep caller-owned
commit, current audit coupling, connection checks, token contents, persistence,
and public contracts unchanged.

Putting the lock only in the callback would leave the shared session owner able
to insert without its invariant. Putting it in the SQL insert would acquire it
after the stale User read and after the connection lock, too late to close the
issuance race and contrary to the fixed lock order. A new table, row lock,
mutex, lease, retry layer, isolation mode, trait, service, dependency, or public
API would add machinery without closing a different obligation.

## Prior-finding closure

| Prior finding | Independent current-candidate result |
|---|---|
| `FIND-TASK-002-1` — exact human `sub` identity | **CLOSED.** Public and stored human connections require exact `sub`; identity lookup remains verified issuer plus subject. |
| `FIND-TASK-002-2` — OIDC authorized party | **CLOSED.** Human ID-token verification enforces the configured client, including multi-audience `azp`. |
| `FIND-TASK-002-3` — provider role-change audit | **CLOSED.** Changed role sets append canonical evidence in the callback transaction; append failure prevents commit. |
| `FIND-TASK-002-4` — advertised algorithm | **CLOSED.** Fresh advertised membership precedes shared asymmetric signature and key verification. |
| `FIND-TASK-002-5` — duplicate tenant selection | **CLOSED.** Login-state transitions rely on forced RLS without duplicate tenant selection. |
| `FIND-TASK-002-6` — raw pool propagation | **CLOSED.** The least-disclosure state-owner lookup remains a narrow inherent `WyrdPostgres` capability. |
| `FIND-TASK-002-7` — printable PKCE verifier | **CLOSED.** Durable PKCE state remains secret-backed and redacted. |
| `FIND-TASK-002-8` — Rust documentation | **CLOSED.** The R2 inventory retains substantive workflow, invariant, error, and panic documentation. |
| `FIND-TASK-002-9` — hidden imports | **CLOSED.** The cited imports remain module-scoped. |
| `FIND-TASK-002-10` — algorithm-helper contract | **CLOSED.** The helper documents advertised membership only and identifies the shared verifier's enforcement role. |
| `FIND-TASK-002-11` — optional binding/time claims | **CLOSED.** OIDC callers require issuer, audience, expiry, and numeric non-future issuance time while workload semantics remain separate. |
| `FIND-TASK-002-12` — replay/rotation race | **CLOSED.** Rotation and replay take the family lock before classification and retain it through commit. |
| `FIND-TASK-002-13` — stale refresh lookup rustdoc | **CLOSED.** Documentation matches lookup, family lock, classification, and test-only lifecycle observation. |
| `FIND-TASK-002-14` — administrative revocation versus rotation | **CLOSED for its diagnosed path.** Administrative User revocation now shares the family lock with rotation, and its overlap proof establishes successor containment. The new finding covers the distinct initial-issuance caller that never takes the lock. |

## Final deduplicated finding ledger

### FIND-TASK-002-15 — Initial human-session issuance can commit new authority after administrative revocation

- **Wave 1 source ID:** `TD-R7-001`
- **Status:** CONFIRMED
- **Classification:** INCORRECT
- **Violated obligation:** Principal suspension must refuse the next issuance;
  callback/session issuance and administrative revocation must fail closed and
  leave no renewable authority committed after successful revocation; all human
  refresh insertion must use the established family-before-connection
  serialization and caller-owned transaction boundary.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/issuance.rs:475-526`; first-login caller at
  `crates/wyrd/wyrd-auth/src/callback.rs:247-266`; stale status read at
  `crates/wyrd/wyrd-auth/src/issuance.rs:360-383`; competing revocation at
  `crates/wyrd/wyrd-auth/src/revoke.rs:42-67`; route commit at
  `crates/wyrd/wyrd-server/src/auth/revoke.rs:107-121`; lock and persistence
  owners at
  `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:96-120,173-236`.
- **Evidence:** Initial callback issuance reaches the sole production human
  refresh insertion owner with no family lock. It reads an Active User before
  insertion. Revocation locks and updates only the family visible before it
  commits. The initial issuer can then insert and commit a new active row from
  its stale successful status read. Rotation cannot exhibit this gap because
  its caller already owns the family lock.
- **Observable consequence:** A successful administrative User revocation can
  return before an overlapping callback commits a new access token and active
  refresh credential. The row survives suspension and can renew again after
  reactivation, contradicting next-issuance refusal and renewable-authority
  retirement.
- **Decision-complete correction:** Reuse
  `lock_refresh_family(conn, "user", principal_id)` at the start of
  `TenantTokenIssuer::issue_human_session`, before the existing connection-slot
  lock and principal-status read. Preserve the current concrete owner,
  transaction, route-owned commits, family-before-connection order, connection
  provenance, audit behavior, persistence model, and public contracts. Do not
  add another lock type, persistence object, retry protocol, isolation mode,
  abstraction, or dependency.
- **Focused closure proof:** Add one deterministic Postgres test in the owning
  auth `pg_tests`, using the production `TenantTokenIssuer` human-session and
  `revoke_principal_in_conn` owners. Prove both orderings: (1) hold initial
  issuance after it owns the family lock, observe revocation waiting, commit
  issuance then revocation, and verify from a fresh transaction that the new
  row is `principal_revoked` with no active family row; (2) hold revocation's
  family lock through suspension, start issuance, observe it waiting, commit
  revocation, and prove issuance re-reads the suspended User, returns
  `PrincipalInactive`, and inserts no refresh row. Run its exact
  repository-wrapped `mise exec -- cargo nextest run` selector, then the
  principals integration/unit, SQL, identity journey, tenant-isolation,
  format, lint, and diff-hygiene lanes required by the owning remediation.
- **Specification decision:** No revision is required. The approved behavior,
  existing concrete owners, and existing family lock already determine the
  correction.

No other finding is retained.

## Verification limits

- Static validation covered the complete cumulative changed-file inventory,
  R1-R6 authority and finding history, every current production caller and
  full body named above, current SQL lock/update/insert behavior, and the R6
  rotation/revocation and connection-deactivation overlap proofs.
- `git diff --check
  3fc085acf5b3a710d5dc80892bd2e664b3db6174..e1ce3c847c14d306c69a704cd15ce6686db9e62e`
  passed, and `HEAD` remained the immutable candidate.
- This bounded Wave 2 review did not rerun Cargo, Postgres, Docker,
  Keycloak/Dex, migrations, lints, or broad identity lanes. Recorded candidate
  evidence covers prior focused verifier and concurrency proofs, the 27-test
  identity lane, principals unit/integration, SQL, tenant isolation, format,
  lints, and diff hygiene.
- No current test overlaps administrative User revocation with initial human
  session issuance. Existing overlap tests cover revocation versus rotation
  and connection deactivation versus rotation; neither reaches this seam.
- TASK-003 BFF redemption, TASK-004 CLI persistence, and live Okta/Entra
  qualification remain intentional downstream or change-level work.

## Overall validation recommendation

**VALIDATED WITH FINDING — FIX_REQUIRED.** Retain
`FIND-TASK-002-15`. It is a bounded omitted participant in the existing
refresh-family serialization protocol. One existing lock call in the shared
human-session owner plus one deterministic two-ordering Postgres proof closes
the gap; no specification revision is required.
