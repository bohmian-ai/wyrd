# Domain review: tenancy, persistent data, concurrency, and durability

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `6e21d8ed00d5159ec71e3f2e2414e80fd16f76af`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation authority: TASK-002-R1 through TASK-002-R9 and their prior
  verdicts and validated finding ledgers

The checked-out candidate was exactly
`6e21d8ed00d5159ec71e3f2e2414e80fd16f76af` before this report was written.
The repository has no `.codegraph/` directory, so the review used Git, `rg`,
and direct source and caller inspection.

## Reviewed boundary

This cumulative review traced:

- pre-authentication tenant-slug resolution through the narrow
  `WyrdPostgres` capability and the state-hash-only cross-RLS callback lookup;
- forced-RLS `TenantConn` acquisition and caller-owned transaction lifecycles;
- hashed login-state insertion, atomic one-use consumption before provider IO,
  completion attachment, and one-use tenant-bound redemption;
- exact Active connection id/revision re-reads before identity persistence and
  under the connection-slot lock before session issuance;
- canonical `(issuer, subject)` User creation, concurrent identity upsert,
  email non-linking, exact provider-role replacement, and role-sync audit;
- initial human-session issuance, refresh provenance, family locking,
  rotation, replay containment, and successor persistence;
- administrative User revocation and connection replacement, deactivation, or
  removal racing with callback issuance and refresh rotation;
- the login-state, refresh-provenance, and email-uniqueness migrations; and
- the focused Postgres concurrency tests and real-server identity journeys
  that exercise these seams.

All production callers of `login_state_tenant`, `consume_login_state`,
`replace_user_roles`, `lock_refresh_family`, `issue_human_session`,
`refresh_by_hash`, `revoke_refresh_family`, and
`revoke_principal_in_conn` were traced.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Tenant selection and SQL capability identity | `AGENTS.md` §§2, 3, 9, 11; `architecture/agent-rules.md` raw-pool, `TenantConn`, RLS, and transaction-ownership rules; security posture tenant isolation; REQ-006/007/015 and INV-001/004 | `wyrd-sql/src/postgres.rs:150-201`; `wyrd-auth/src/login.rs:83-139`; `wyrd-auth/src/callback.rs:96-168`; login-state migration and `pg_login_state.rs` | PASS |
| Login-state durability and one use | TASK-002 packet-local login contract; REQ-007; INV-004 | migration `20260925000001_auth_login_state_binding.sql`; `queries/auth/login_state.rs:23-274`; `AuthorizationCodeExchange::{execute,complete}`; state-transition Postgres tests | PASS |
| Connection revision binding and lifecycle cutoff | REQ-014/016; task replacement/renewal scenario; fixed family-before-connection lock order | `callback.rs:170-306`; `issuance.rs:453-538`; `connections.rs:457-593`; human-connection SQL; provider-switch and deactivation-overlap proofs | PASS |
| Tenant User identity and email non-linking | REQ-008/014/015; INV-002/003 | `callback.rs:494-519`; `queries/auth/user_identities.rs`; `queries/auth/users.rs`; identity and email-uniqueness migrations; same-issuer and provider-switch journeys | PASS |
| Provider-role replacement and audit atomicity | REQ-008/017; INV-003; canonical same-transaction audit rules; R9 `FIND-TASK-002-18` | `callback.rs:205-277`; `role_assignments.rs:34-53,117-146`; `issuance.rs:360-450`; `concurrent_callbacks_replace_roles_without_union`; audit rollback tests | PASS |
| Refresh rotation and replay containment | REQ-016; INV-004; R4/R5 | `refresh.rs:84-227`; `refresh_tokens.rs:27-237`; served refresh commit handling in `components/auth/routes.rs:232-271`; deterministic replay/rotation proof | PASS |
| Administrative revocation versus rotation and first issuance | Security posture next-issuance refusal; R6/R7 | `revoke.rs:16-89`; `issuance.rs:453-538`; served revocation transaction; deterministic revocation/rotation and revocation/initial-issuance proofs | PASS |
| Persistent-data migration | TASK-002 refresh provenance and identity contract; repository migration rules | migrations `20260925000000_auth_human_connections.sql` and `20260925000001_auth_login_state_binding.sql`; `pg_migration.rs` legacy-row and idempotence coverage | PASS |

## Concurrency and durability trace

The current lock and transaction protocol is coherent:

1. The callback resolves the canonical User, takes that tenant/User refresh-
   family lock before replacing provider roles, and retains it through the
   role-sync audit, human-session issuance, completion sealing, and commit.
2. `issue_human_session` takes or transaction-locally reacquires the same
   family lock before the tenant connection-slot lock, then rechecks the exact
   Active connection revision and the User's current status before signing or
   inserting a refresh row.
3. Refresh resolves the immutable stored family identity, takes the family
   lock before lifecycle classification, then consumes or contains replay. Its
   successor copies the original connection provenance.
4. Administrative User revocation takes the same family lock before suspending
   the User and retiring every active family row. Whichever operation wins is
   visible to the waiter before it mutates or issues.
5. Connection lifecycle mutations take the connection-slot lock. Paths holding
   both locks always acquire family first and connection second, so replacement,
   deactivation, removal, callback issuance, and refresh rotation serialize
   without an inverted lock order.

No function accepting `&mut TenantConn<'_>` commits or rolls back. Successful
callback identity, roles, role-sync audit, token-exchange audit, refresh row,
sealed completion, and completion attachment remain one caller-owned atomic
unit. Refresh-replay containment is the deliberate refusal path whose route
commits the family revocation and canonical audit before returning the typed
error.

## Prior-finding closure

| Prior finding | Current-candidate result |
|---|---|
| `FIND-TASK-002-1` | CLOSED: public authoring and stored connection decode require literal OIDC `sub`; durable identity remains tenant-qualified `(issuer, subject)`, and email is non-unique and non-linking. |
| `FIND-TASK-002-3` | CLOSED: a real provider-role change appends one canonical role-sync event in the same callback transaction; audit failure rolls back identity/role/session effects. |
| `FIND-TASK-002-5` | CLOSED: login-state purge, consume, complete, and redeem rely on forced RLS rather than duplicate tenant predicates. |
| `FIND-TASK-002-6` | CLOSED: state-hash ownership is an inherent `WyrdPostgres` operation over its private app pool and returns only the pending row's tenant id. |
| `FIND-TASK-002-7` | CLOSED: the durable PKCE verifier is secret-backed and redacted after SQL decode. |
| `FIND-TASK-002-12` | CLOSED: refresh family locking precedes lifecycle classification, preventing ancestor replay from missing a concurrently committed successor. |
| `FIND-TASK-002-14` | CLOSED: administrative User revocation shares the family lock with rotation and contains an overlapping successor. |
| `FIND-TASK-002-15` | CLOSED: the sole human-session refresh-row owner takes the family lock before status and connection checks, covering both initial issuance/revocation orderings. |
| `FIND-TASK-002-18` | CLOSED: callback role replacement now takes the existing User family lock before changing roles. Concurrent callbacks cannot union disjoint mappings; each token and the final durable set equal one serialized assertion. |
| `FIND-TASK-002-22` | CLOSED: begin-login passes only the typed slug to `WyrdPostgres::resolve_tenant_slug`; runtime app-pool selection remains private to that owner. |
| Other findings `FIND-TASK-002-2`, `-4`, `-8` through `-11`, `-13`, `-16`, `-17`, and `-19` through `-21` | No tenancy, persistence, concurrency, durability, audit-atomicity, or migration regression was introduced at their corrected seams. |

## Material findings

None.

The cumulative candidate leaves no material reachable tenancy, persistent-
data, durability, concurrency, RLS, migration, or transactional-audit defect
within TASK-002's approved boundary.

## Verification notes and limits

- Fresh `git diff --check
  3fc085acf5b3a710d5dc80892bd2e664b3db6174..6e21d8ed00d5159ec71e3f2e2414e80fd16f76af`
  passed during this review.
- Inspected recorded green evidence for `test:principals:unit`,
  `test:principals:integration`, `test:sql`, `test:identity:journey` (27
  journeys), `check:tenant-isolation`, `check:from-pools-allowlist`, codegen,
  format, lints, and diff hygiene. These broad lanes were not rerun in this
  Wave 1 slot.
- Inspected the deterministic focused proofs for concurrent callbacks,
  ancestor replay versus rotation, administrative revocation versus rotation,
  administrative revocation versus first issuance, and connection
  deactivation versus rotation. Their task records report successful exact
  selectors; this review did not start another shared Cargo/Postgres run.
- Live Okta/Entra qualification, TASK-003 BFF completion, and TASK-004 CLI
  handoff/persistence remain downstream or change-level work and were not
  treated as TASK-002 domain gaps.

## Overall result

**PASS**

The R9 correction closes `FIND-TASK-002-18` at the existing per-User family
owner and closes `FIND-TASK-002-22` at `WyrdPostgres`. The cumulative candidate
preserves tenant isolation, caller-owned atomicity, one-use login state,
connection-provenance cutoff, exact provider-role replacement, refresh replay
containment, and administrative revocation across the reviewed concurrent
paths.
