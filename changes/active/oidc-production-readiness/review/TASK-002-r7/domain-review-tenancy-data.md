# Domain review: tenancy, persistent data, concurrency, and durability

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `e1ce3c847c14d306c69a704cd15ce6686db9e62e`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation authority: TASK-002 R1 through R6 and their prior verdicts and validated ledgers

The candidate remained at the stated commit throughout this review. Lead-directed reuse and test commits recorded separately in the evidence tables were treated as authorized rather than scope drift and were inspected for data-path regression.

## Reviewed boundary

I traced the cumulative base-to-candidate implementation across:

- login-state migration, forced RLS, the least-disclosure state-owner lookup, and `WyrdPostgres`/`TenantConn` acquisition;
- login-state insert, consume, completion, and redemption, including transaction and commit ownership;
- callback identity upsert, role replacement, audit, first session issuance, refresh persistence, and rollback behavior;
- refresh lookup, tenant routing, family advisory locking, active-row consumption, rotation, replay containment, connection provenance, and successor insertion;
- connection activation, replacement, deactivation, and removal versus the connection-slot lock;
- administrative User revocation, suspension, refresh-family retirement, route audit, and commit boundaries;
- the OIDC migrations, RLS policies, foreign keys, email uniqueness removal, and legacy unbound-family revocation;
- focused SQL/concurrency tests, the four real-server identity journeys, recorded verification evidence, and every runtime refresh-family mutation caller.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Tenant SQL and transaction ownership | `AGENTS.md` §§6, 9, 11; `architecture/agent-rules.md` TenantConn/RLS/caller-commit rules; security posture tenant/data isolation | `login_state.rs`, `refresh_tokens.rs`, `human_connections.rs`, `user_identities.rs`, `users.rs`, `role_assignments.rs`, `postgres.rs`, server auth handlers | PASS |
| Login-state persistence and tenant derivation | REQ-006/007, INV-001/004, TASK-002 packet-local contract | `20260925000001_auth_login_state_binding.sql`, `HumanConnections::begin_login`, `AuthorizationCodeExchange::{execute,complete,finish_id_token_exchange}`, `pg_login_state.rs` | PASS |
| Connection lifecycle cutoff | REQ-014/016, AC-006/007; family-before-connection ordering | `HumanConnections::{activate,deactivate,remove}`, `TenantTokenIssuer::issue_human_session`, connection-slot SQL, `connection_deactivation_overlapping_rotation_ends_successor` | PASS |
| Refresh rotation and replay containment | REQ-016, INV-004, R4/R5 | `RefreshTokens::execute`, `lock_refresh_family`, `consume_active_refresh`, `revoke_refresh_family`, deterministic replay/rotation overlap proof | PASS |
| Administrative User revocation | security posture immediate next-issuance refusal; R6; route transaction/audit contract | `revoke_principal_in_conn`, `revoke_principal`, R6 overlap proof, all `issue_human_session` and refresh-row insertion callers | **FAIL — TD-R7-001** |
| Migration durability | task provenance contract and migration evidence | both `20260925000000` and `20260925000001`, `pg_migration.rs` coverage | PASS |

## Prior-finding closure

| Prior finding | Current-candidate result |
|---|---|
| `FIND-TASK-002-1` through `-4` | CLOSED: exact OIDC `sub`, authorized-party checks, transactional role-sync audit, and discovered algorithm policy remain intact. |
| `FIND-TASK-002-5` and `-6` | CLOSED: login-state transitions rely on forced RLS, while the callback uses only the narrow `WyrdPostgres::login_state_tenant` definer lookup before opening `TenantConn`. |
| `FIND-TASK-002-7` | CLOSED: the durable PKCE verifier remains `SecretString` immediately after decode and redacted under `Debug`. |
| `FIND-TASK-002-8` through `-11` | CLOSED with no data-path regression: required documentation/import corrections and mandatory OIDC binding/time claims remain present. |
| `FIND-TASK-002-12` and `-13` | CLOSED: refresh lookup precedes the tenant-qualified family lock, classification follows it, and replay containment cannot miss a concurrently committed rotation successor. |
| `FIND-TASK-002-14` | CLOSED for its diagnosed path: administrative User revocation now takes the same family lock as refresh rotation before suspension/family retirement, and its focused overlap proof verifies that a rotation successor is retired. `TD-R7-001` is a distinct first-issuance caller omitted from that lock protocol. |

## Material proposed finding

### TD-R7-001 — First human-session issuance can recreate refresh authority after administrative revocation commits

- **Classification:** INCORRECT / REGRESSION BOUNDARY
- **Violated obligation:** The security posture requires suspending a principal to refuse its next issuance immediately. R6 requires administrative User revocation and refresh-family creation/mutation to serialize so successful revocation leaves no renewable authority. TASK-002 also requires principal blocking and renewal behavior to fail closed while preserving caller-owned atomic transactions.
- **Exact location:** `crates/wyrd/wyrd-auth/src/issuance.rs:475-526` (`TenantTokenIssuer::issue_human_session`); called for first login at `crates/wyrd/wyrd-auth/src/callback.rs:248-256`. Competing revocation is `crates/wyrd/wyrd-auth/src/revoke.rs:42-68`, committed by `crates/wyrd/wyrd-server/src/auth/revoke.rs:107-121`.
- **Reachability and evidence:** `issue_human_session` takes only `lock_human_connection_slot`. On an initial callback (`rotated_from = None`) it does not take `lock_refresh_family`. It then reads the User as Active in `TenantTokenIssuer::issue`, signs and audits, and later inserts the first refresh row. The User read is an ordinary `SELECT` without a row lock. Therefore an already-authenticated callback for an existing User can read Active; administrative revocation can then take the family lock, suspend the User, revoke the family visible to its update, and commit; the callback can subsequently insert and commit a new active refresh row from the stale successful principal read. The R6 rotation overlap test cannot exercise this path because rotation takes the family lock before classification; the first-login callback does not.
- **Observable consequence:** `POST /v1/principals/{id}/revoke` can return after durably suspending the User and retiring the then-visible family while an in-flight OIDC callback subsequently commits a new access token and renewable refresh credential for that revoked User. The row survives as active durable authority and can become usable if the User is later reactivated, contradicting the revocation and immediate-next-issuance contracts.
- **Required testable correction:** Reuse the existing tenant-qualified refresh-family lock in the shared human-session issuance owner for initial issuance, before the existing connection-slot lock and before reading principal status. Preserve the established family-before-connection order. Rotation already enters this owner holding that same family lock, so the implementation must reuse the existing mechanism without changing public contracts, persistence, or caller-owned commit semantics. Add one deterministic Postgres proof using the production callback/session-issuance and administrative-revocation owners: hold an initial issuance after it owns the family lock, show revocation waits and then retires the inserted row; also exercise the reverse ordering so issuance waiting behind a committed revocation re-reads the suspended User and refuses without inserting a refresh row.

This is the smallest root-cause boundary: every production human refresh row is inserted by `issue_human_session`, while the administrative revocation owner already takes the matching family lock. No new lock, table, lease, isolation level, retry layer, or public API is warranted.

## Verified properties without findings

- The login-state row is globally keyed only by the 256-bit state digest, carries exactly one initiation binding, and is consumed in its RLS tenant transaction before provider IO. Completion and redemption remain one-use.
- The state-owner definer function exposes only the tenant of a pending state and is granted only to `wyrd_app`; the callback then opens a tenant-bound transaction and cannot derive authority from that lookup.
- Callback role replacement, role-sync audit, token-exchange audit, first refresh insertion, and sealed completion share one `TenantConn` transaction; audit or completion failure rolls the whole unit back.
- Refresh rotation resolves immutable family ownership, takes the tenant-qualified family lock, classifies under that lock, takes the connection lock second, copies connection provenance, and leaves commit to the route caller.
- Connection lifecycle operations and human issuance share the connection-slot lock, so replacement, deactivation, and removal serialize with both first issuance and rotation. The late lead-directed deactivation-overlap proof covers the production owner and introduces no scope drift.
- Administrative User revocation now correctly serializes with refresh rotation, and User suspension plus visible-family retirement remain atomic with the served authorization audit.
- The migration revokes live provenance-free User refresh rows, leaves machine rows unbound, preserves connection tombstones referenced by sessions, drops email uniqueness for non-linking semantics, and restores forced RLS on recreated login state.
- No callee accepting `&mut TenantConn<'_>` commits or rolls back. Production commit ownership remains at the connection/callback/login/revocation server or handle boundary.

## Verification limits

- Static review covered the complete changed-file inventory and cumulative diff, all R1-R6 tasks and prior ledgers, applicable authorities, the full bodies and callers named above, both OIDC migrations, SQL tests, concurrency tests, and recorded evidence. No usable `.codegraph/` index exists, so source tracing used Git, `rg`, and direct reads.
- `git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174..e1ce3c847c14d306c69a704cd15ce6686db9e62e` passed, and `HEAD` remained the immutable candidate.
- I did not rerun Cargo, Postgres, Docker, Keycloak/Dex, migrations, lint, or broad identity lanes in this bounded Wave 1 slot. Runtime confidence for passing properties relies on inspected deterministic tests and the candidate's recorded green focused and broader commands.
- No current test overlaps administrative User revocation with first human-session issuance. Existing proofs cover revocation versus refresh rotation and connection deactivation versus rotation; neither can detect `TD-R7-001`.
- TASK-003's BFF completion route, TASK-004's CLI handoff persistence, and live-provider qualification remain intentional downstream/change-level work.

## Overall result

**FAIL.** The candidate closes `FIND-TASK-002-14` for refresh rotation and preserves the reviewed tenant-RLS, login-state, migration, callback transaction, connection-lifecycle, replay-containment, and prior-remediation properties. One bounded concurrency/durability gap remains: initial human-session issuance does not participate in the refresh-family lock, so it can create renewable authority after administrative User revocation commits. The correction reuses the existing lock in the existing issuance owner and needs one focused Postgres overlap proof; it requires no specification or persistence-model revision.
