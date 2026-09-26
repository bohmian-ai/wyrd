---
id: TASK-002-R7
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-016, INV-004, AC-007]
depends_on: [TASK-002-R6]
parent_task: TASK-002
remediates: [FIND-TASK-002-15]
---

# Serialize initial human-session issuance with administrative revocation

## Authority and immutable subject

- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Review verdict:
  `changes/active/oidc-production-readiness/review/TASK-002-r7/verdict.md`
- Validated findings:
  `changes/active/oidc-production-readiness/review/TASK-002-r7/findings-validation.md`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Reviewed candidate: `e1ce3c847c14d306c69a704cd15ce6686db9e62e`
- Validated finding: `FIND-TASK-002-15`

This remediation closes one omitted participant in the existing refresh-family
serialization protocol without changing the approved specification, public
auth contract, persistence model, or accepted R1–R6 behavior.

## Issue diagnosis

### FIND-TASK-002-15 — Initial human-session issuance can commit new authority after administrative revocation

`TenantTokenIssuer::issue_human_session` is the sole production owner that
inserts human refresh rows. Refresh rotation reaches it after acquiring the
tenant-qualified refresh-family lock, but the callback reaches it for initial
issuance with no family lock. The owner takes only the human-connection slot
lock, reads the User's status through `TenantTokenIssuer::issue`, then signs and
audits an access token and inserts the first refresh row.

Administrative User revocation takes the refresh-family lock, suspends the
User, revokes the family visible to its update, and commits through the served
route. For an existing User, an overlapping callback can read `active` before
revocation, allow revocation to acquire its uncontended family lock and commit,
then insert and commit a new active refresh row plus access token and sealed
completion from the stale successful status read. The row remains renewable
authority and can become usable after later reactivation.

The existing R6 proof covers rotation versus revocation because rotation
already owns the family lock. It cannot exercise initial issuance, which has no
predecessor refresh row and currently takes no family lock. The connection-slot
lock does not close the gap because principal revocation neither mutates nor
locks the human connection.

## Intended correction outcome

Every human-session issuance, including first login, participates in the same
tenant-qualified refresh-family serialization as rotation and administrative
User revocation. Once revocation commits, an overlapping initial issuance
either preceded it and has its newly inserted row retired by revocation, or
follows it and re-reads the suspended User and refuses without minting or
persisting new authority.

## Decision-complete recommendation

At the start of the existing `TenantTokenIssuer::issue_human_session` owner,
acquire `lock_refresh_family(conn, "user", principal_id)` before the existing
human-connection slot lock and before `Self::issue` reads principal status.
Keep the existing `TenantConn`; callback and refresh routes already own their
transactions and commits, so the lock remains held through refresh insertion,
audit, completion, and commit or rollback.

This owner is the correct boundary because every production human refresh row
is inserted there. Rotation already holds the same transaction advisory lock;
PostgreSQL permits the owning transaction to reacquire it and retains it until
transaction end. Preserve the established family-before-connection lock order.
Do not put the lock only in the callback, where another caller could bypass the
owner invariant, or in the SQL insert after the stale principal read and
connection lock.

## Constraints and preserved behavior

- Keep all tenant work on `TenantConn` with caller-owned commit and rollback.
- Preserve forced RLS, canonical transactional audit, User suspension,
  `principal_revoked` family state, connection provenance, replay containment,
  sealed completion, and the five-minute access-token snapshot.
- Preserve family-before-connection ordering for both first issuance and
  refresh rotation.
- Preserve Service and Agent issuance/revocation behavior; they have no human
  refresh family.
- Preserve exact connection revision checks and all closures of
  `FIND-TASK-002-1` through `FIND-TASK-002-14`.
- Preserve public HTTP/OpenAPI/schema contracts, generated artifacts, and all
  four TASK-002 identity journeys.

## Non-goals

- No new persistence object, family identifier, lock service, trait, retry
  loop, isolation-level change, process-local synchronization, or dependency.
- No public API, error, migration, schema, generated-artifact, or token-lifetime
  change.
- No TASK-003 BFF completion, TASK-004 CLI persistence, provider work, email
  linking, or machine-identity redesign.
- No unrelated issuance, refresh, revocation, transaction, or test-harness
  refactor.

## Acceptance criteria

| Criterion | Finding closure |
|---|---|
| The shared human-session owner acquires the existing tenant-qualified refresh-family lock before the connection-slot lock and before reading User status, and retains it through the caller-owned commit or rollback. | `FIND-TASK-002-15` |
| When initial issuance owns the family lock first, administrative revocation waits and then retires the newly inserted row; committed state contains no active family row. | `FIND-TASK-002-15` |
| When administrative revocation owns the family lock first and commits suspension, initial issuance waits, re-reads the User as suspended, returns `PrincipalInactive`, and inserts no refresh row. | `FIND-TASK-002-15` |
| Existing rotation/replay serialization, family-before-connection order, audit coupling, connection cutoff, Service/Agent behavior, and prior remediations remain unchanged. | `FIND-TASK-002-15` |

## Focused proof and broader verification

Add one deterministic Postgres test named
`issuance::pg_tests::initial_session_issuance_and_user_revocation_serialize`
under the owning auth `pg_tests` using the production
`TenantTokenIssuer::issue_human_session` and
`revoke_principal_in_conn` owners. It must exercise both orderings without
sleep-based synchronization:

1. Hold initial issuance after it owns the family lock, observe administrative
   revocation waiting on that exact lock, commit issuance and then revocation,
   and verify from a fresh transaction that the new row is
   `principal_revoked` and no active family row remains.
2. Hold administrative revocation after it owns the family lock and suspends
   the User, start initial issuance, observe issuance waiting on that exact
   lock, commit revocation, and prove issuance returns `PrincipalInactive` and
   inserts no refresh row.

Run its exact focused command through the repository-managed Postgres wrapper:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E 'test(=issuance::pg_tests::initial_session_issuance_and_user_revocation_serialize)'"
```

Then run the narrow owning and regression lanes:

```bash
mise run test:principals:unit
mise run test:principals:integration
mise run test:sql
mise run test:identity:journey
mise run check:tenant-isolation
mise run fmt
mise run lints
git diff --check
```

If closure requires a new public contract, persistence model, lock abstraction,
transaction-isolation decision, or broader concurrency semantic, stop and route
that decision through specification revision rather than expanding this
remediation.

## Implementation evidence

Commit: `d5c04dd13` (fix + focused test).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| The shared human-session owner takes the tenant-qualified refresh-family lock before the connection-slot lock and before reading User status, held through the caller's commit or rollback. | `crates/wyrd/wyrd-auth/src/issuance.rs` `TenantTokenIssuer::issue_human_session` calls `lock_refresh_family(conn, "user", principal_id)` first, then `lock_human_connection_slot`, then `Self::issue`. Transaction-scoped advisory lock on the caller's `TenantConn`; rotation reacquires it in its own transaction. | Focused test observes each side's backend blocked on an `advisory` lock while the other holds the family lock. | PASS |
| Issuance owns the lock first: revocation waits, then retires the new row; no active family row. | Same. | `issuance::pg_tests::initial_session_issuance_and_user_revocation_serialize`, ordering 1: committed family is `(1 row, 0 active)`. | PASS |
| Revocation owns the lock first: issuance waits, re-reads the User as suspended, returns `PrincipalInactive`, inserts no row. | Same. | Same test, ordering 2: `Err(IssuanceError::PrincipalInactive)`, committed family `(0 rows)`. With the lock call removed the test fails (revocation never waits; 30s timeout panic in `wait_for_advisory_lock_wait`). | PASS |
| Rotation/replay serialization, family-before-connection order, audit, connection cutoff, Service/Agent behavior, prior remediations unchanged. | Only change in production code is the added lock call. `revoke::pg_tests` is now `pub(crate)` so the issuance test reuses its `wait_for_advisory_lock_wait` probe (doc generalized). | `test:principals:integration` (includes `issuance`, `refresh`, `revoke` pg_tests), `test:principals:unit`, `test:sql`, `test:identity:journey` (27/27), `check:tenant-isolation`, `fmt`, `lints`, `git diff --check` all exit 0. | PASS |

Commands run, each exiting 0: the focused `with-test-postgres.sh … nextest`
command, `mise run test:principals:unit`, `mise run test:principals:integration`,
`mise run test:sql`, `mise run test:identity:journey`,
`mise run check:tenant-isolation`, `mise run fmt`, `mise run lints`,
`git diff --check`.

Issuance audit (every production path that inserts refresh rows or mints
access authority, found by grepping `insert_refresh_token`,
`insert_human_refresh_token`, `issue_human_session`, `.issue(`,
`issue_access_token`, `issue_refresh_token`, `issue_platform_access_token`
over `crates` and `sdks`, excluding `#[cfg(test)]` modules and test harnesses):

| Path | Authority issued | Persists renewable authority? | Serialization vs revocation | Action |
|---|---|---|---|---|
| OIDC callback first login → `issue_human_session` (`callback.rs:249`) | access + human refresh row | yes | family lock before status read (`issuance.rs:490`) | **now locked** (this task); proven by both orderings of the focused test |
| Refresh rotation → `issue_human_session` (`refresh.rs:217`) | access + successor refresh row | yes | family lock at `refresh.rs:126`, reacquired at `issuance.rs:490` in the same transaction | already locked; proven by `revoke::pg_tests::refresh_rotation_overlapping_user_revocation_retires_successor` |
| API-key exchange → `TenantTokenIssuer::issue` (`exchange_api_key.rs:191`) | 5-minute access token | no refresh row (`refresh_token: None`) | status read in the grant transaction; machine revocation takes no family lock | cannot race beyond the accepted bound: a mint whose status read precedes the suspension commit is equivalent to a token issued just before revocation, which the spec lets lapse at its five-minute expiry; a mint after the commit re-reads `suspended` and refuses |
| Token-exchange delegation → `issue` (`exchange_api_key.rs:328`) | 5-minute access token | no | same | same as API-key exchange |
| Workload `jwt-bearer` → `issue` (`jwt_bearer.rs:72`) | 5-minute access token | no | same | same as API-key exchange |
| Platform credential exchange `PlatformSessions::exchange` (`platform_sessions.rs:164`) | platform session token | no refresh row | platform extractor re-confirms the session, credential, and principal against the store on every request before reading grants (`platform_extractor.rs:136`) | cannot race: a stale mint is refused on first use |
| Platform federated login `PlatformSessions::issue_federated` (`platform_sessions.rs:246`) | platform session token | no | same per-request store confirmation | cannot race |
| Machine `insert_refresh_token` (`service_accounts.rs:44`) | machine refresh row | — | — | no production caller (tests only) |
| Direct `issue_access_token` in `wyrd-server` (`authenticate.rs`, `principal_extractor.rs`, `caller_extractor.rs`, `boot/auth.rs`, `oracle/lifecycle_service.rs`) and `wyrd-testing/src/server.rs` | — | — | — | test code only |

Only initial human-session issuance could persist authority past a
revocation, and it is now serialized. Machine and platform paths persist no
renewable authority.

Non-goals held: no new persistence, lock abstraction, isolation change,
retry loop, public contract, migration, or generated artifact; no unrelated
file changed.
