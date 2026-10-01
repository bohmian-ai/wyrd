# TASK-002 R8 Tenancy, Persistent Data, Concurrency, and Durability Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `ced8acaabce7dbe1682bef46d65d073b07e0dbd9`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation authority: TASK-002 R1 through R7 and their prior verdict and finding history

`HEAD` remained the candidate above during this review. Lead-directed reuse and
test commits recorded separately in the task evidence were treated as
authorized and were inspected for regressions rather than classified as scope
drift.

## Reviewed boundary

This review traced the complete cumulative base-to-candidate implementation for:

- tenant selection at login initiation and the header-free callback;
- the cross-RLS login-state owner lookup and the transition back onto a forced-RLS
  `TenantConn`;
- hashed, expiring, one-use login state and sealed, one-use completion storage;
- tenant User identity, role replacement, connection provenance, and
  transaction-owned issuance;
- refresh lookup, rotation, replay containment, and refresh-family advisory-lock
  identity and order;
- callback/initial issuance, refresh rotation, connection lifecycle, and
  administrative User revocation overlap;
- caller-owned commit/rollback, canonical audit coupling, migration behavior,
  and fresh committed-state verification; and
- the R7 correction and deterministic two-ordering Postgres proof.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Tenant isolation and connection provenance | `AGENTS.md` §§2, 3, 9, 11; `architecture/agent-rules.md` SQL-tenancy rules; security posture tenant boundary; spec REQ-006–008, REQ-015–016, INV-001–004 | `wyrd-auth/src/login.rs`, `callback.rs`, `connections.rs`; `wyrd-sql/src/postgres.rs`, `queries/auth/login_state.rs`; migration `20260925000001_auth_login_state_binding.sql`; `pg_login_state.rs`; identity journeys | PASS |
| Cross-RLS callback lookup | Least-disclosure server ownership, role separation, and fail-closed tenant selection in the task and security posture | `WyrdPostgres::login_state_tenant`; `SECURITY DEFINER` function with fixed search path, revoked public execute, `wyrd_app` grant, and only an unconsumed/unexpired tenant-id result; callback immediately opens a tenant transaction and consumes through RLS | PASS |
| Login-state durability and one use | TASK-002 state/completion contract; REQ-007; INV-004; persistent-data and transaction rules | Globally unique SHA-256 state key; initiation-binding uniqueness/check constraints; atomic consume update; completion-only-after-consume constraint; delete-returning redemption; PostgreSQL-derived expiry; callback consume commit before provider IO | PASS |
| Transaction ownership and audit durability | `TenantConn` caller-owned lifecycle rule; canonical audit authority; REQ-017 | Login, callback, issuance, refresh, and revocation bodies and their route commit boundaries; callback success writes User/roles/audit/session/completion in one transaction; refresh/revocation writes and audit remain caller-committed | PASS |
| Refresh rotation and replay containment | TASK-002 refresh contract; R4/R5; security posture renewal rules | `refresh.rs::RefreshTokens::execute`; `refresh_tokens.rs::{refresh_by_hash,lock_refresh_family,consume_active_refresh,revoke_refresh_family,insert_human_refresh_token}`; replay/rotation tests and corrected lifecycle rustdoc | PASS |
| Connection mutation versus issuance/rotation | REQ-014/016; R4/R6; fixed family-before-connection order | `issue_human_session` takes family then connection slot; refresh uses the same order; connection lifecycle takes the slot and exact-revision checks cut off replacement/deactivation; connection-overlap proof | PASS |
| Administrative User revocation versus rotation | Security posture next-issuance/refreshed-authority rule; R6 | `revoke_principal_in_conn` takes the tenant-qualified User family lock, suspends the User, and retires the family in one caller-owned transaction; deterministic rotation/revocation overlap proof | PASS |
| Administrative User revocation versus initial issuance | R7 and `FIND-TASK-002-15` | `TenantTokenIssuer::issue_human_session` now acquires `lock_refresh_family(conn, "user", principal_id)` before the connection lock and User-status read; fresh two-ordering Postgres proof establishes wait/re-read or subsequent family retirement | PASS |
| Migration and retained rows | Task migration requirements; persistent-data and RLS authorities | New login-state table is forced-RLS; transient old login state is deliberately discarded; provenance-free active human refresh rows are revoked; email uniqueness is removed to preserve `(issuer, subject)` identity | PASS |
| Prior findings | R1–R7 validated ledgers | `FIND-TASK-002-1` through `FIND-TASK-002-15` rechecked at their current owners and seams | PASS |

## Concurrency trace

The current family mutation order is consistent and tenant-qualified:

1. Refresh resolves the immutable stored family identity without classifying
   lifecycle state, acquires the tenant/principal family lock, then consumes or
   contains replay.
2. Human-session issuance acquires that same family lock before the connection
   slot, rechecks the exact Active connection revision, and only then reads the
   User and writes access audit plus the bound refresh successor.
3. Administrative User revocation acquires the same family lock, suspends the
   User, and revokes every active row before its route-owned commit.
4. Connection mutation and human issuance serialize on the connection slot;
   family-before-connection is the only path that holds both locks.

Consequently, an initial issuance or rotation that wins first commits before
revocation and its new row is visible to the revoker, while revocation that wins
first makes the later issuance re-read a suspended User and refuse. Replay
containment classifies only after the family lock, so it cannot miss a
concurrently committed successor. No callee commits or rolls back a
`TenantConn`.

## Material findings

None.

The R7 candidate closes `FIND-TASK-002-15` at the existing shared owner with the
existing lock. No material tenancy, persistent-data, concurrency, durability,
or migration defect remains in the reviewed TASK-002 boundary.

## Verification notes and limits

- Freshly run during this review, exit 0:
  `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E 'test(=issuance::pg_tests::initial_session_issuance_and_user_revocation_serialize)'"`.
  The wrapper also ran both repository migration-idempotence tests successfully.
- Fresh `git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174..ced8acaabce7dbe1682bef46d65d073b07e0dbd9`
  passed.
- Recorded candidate evidence reports passing principals unit/integration, SQL,
  tenant-isolation, all 27 identity journeys, format, lints, and diff hygiene.
  This bounded domain review did not rerun those broader lanes, Docker IdP
  journeys, or the full workspace gate.
- Static inspection covered the cumulative source and prior finding history,
  with focused re-execution of the only new executable R7 seam. No CodeGraph
  index exists, so source and caller tracing used Git, `rg`, and direct reads.
- TASK-003 BFF redemption, TASK-004 CLI persistence, and live Okta/Entra
  qualification remain explicit downstream/change-level work and were not
  treated as TASK-002 gaps.

## Overall result

**PASS**
