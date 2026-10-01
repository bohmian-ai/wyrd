# Domain review: tenancy, persistent data, durability, and concurrency

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `d861845f3f5d89aca413857dcfb8c1bbfaee349d`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation tasks: `TASK-002-R1-tenant-login-corrections.md` and
  `TASK-002-R2-repository-rule-corrections.md`
- Domain result: **PASS**

## Reviewed boundary

This review traced the cumulative base-to-candidate implementation through:

1. tenant route resolution, active-connection capture, and hashed login-state
   insertion under tenant RLS;
2. the narrow state-hash-to-tenant lookup, transition into the owning
   `TenantConn`, atomic one-use state consumption, and the commit before
   provider IO;
3. exact active-connection revalidation, tenant User resolution, durable role
   replacement, role-change audit, session/refresh issuance, sealed
   completion, and their shared transaction;
4. tenant-scoped, one-use completion redemption;
5. human refresh provenance, active-connection slot locking, successor
   rotation, stale-token family containment, and provider replacement cutoff;
6. same-issuer cross-tenant isolation and migration behavior for transient
   state, provenance-free User refresh rows, machine rows, and email
   uniqueness; and
7. the post-R2 lead-directed reuse cleanups and unrelated flaky-test repairs
   for any effect on audit data, transactionality, durability, or concurrency.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Effective tenant selection and least disclosure | Spec `REQ-006`, `REQ-007`, `REQ-015`, `INV-001`, `INV-004`; task packet-local login contract; security posture tenant/data isolation | `wyrd-auth/src/login.rs`; `wyrd-auth/src/callback.rs`; `wyrd-sql/src/postgres.rs::login_state_tenant`; migration `20260925000001_auth_login_state_binding.sql`; `pg_login_state.rs` | PASS |
| Forced-RLS login-state transitions | `AGENTS.md` §§2, 5, 9; `architecture/agent-rules.md` `TenantConn` and transaction rules; security posture | `wyrd-sql/src/queries/auth/login_state.rs`; migration RLS policy; `pg_login_state.rs::login_state_transitions_are_confined_to_the_owning_tenant` | PASS |
| One-use state and completion transitions | Task packet-local bounded-state and handoff contract; spec `REQ-007`, `INV-004` | `AuthorizationCodeExchange::complete`; `finish_id_token_exchange`; `HumanConnections::redeem_completion`; SQL consume/complete/redeem statements and Postgres tests | PASS |
| Login issuance and audit atomicity | Spec `REQ-008`, `REQ-017`; canonical same-transaction audit rule | `wyrd-auth/src/callback.rs`; `issuance.rs::issue_human_session`; `role_assignments.rs::replace_user_roles`; callback rollback tests | PASS |
| Provider replacement and refresh cutoff | Spec `REQ-014`, `REQ-016`; security posture access/refresh rules | `connections.rs` lifecycle owners; human-connection advisory lock; `issuance.rs::issue_human_session`; `refresh.rs::RefreshTokens::execute`; provider-switch and cutoff journey evidence | PASS |
| Refresh concurrency and replay durability | Spec `REQ-016`, `INV-004`; task renewal contract | `refresh_tokens.rs::consume_active_refresh`; successor insertion; route commit behavior for success and replay containment; refresh concurrency/replay tests | PASS |
| Cross-tenant and same-issuer isolation | Spec `REQ-015`, `INV-001`–`INV-004`; security posture plane separation | forced-RLS state/identity queries; state-owner lookup test; `same_issuer_two_tenant_isolation_keycloak`; callback-refusal journey evidence | PASS |
| Migration safety | Task migration obligations; persistent-data constraints | migrations `20260925000000` and `20260925000001`; `pg_migration::human_connection_upgrade_preflight`; full migration idempotence proof | PASS |
| Lead-directed follow-on changes | Caller authorization; unchanged domain authorities | `8b201627c..d861845f3` diff: audit helper reuse in `audit.rs`/`callback.rs`, algorithm-filter cleanup, documentation, test reuse, and two isolated flaky-test repairs | PASS; no domain behavior drift |

## Prior-finding closure

### FIND-TASK-002-3 — Closed

Provider-driven role replacement reports whether membership changed. A real
change appends one canonical `auth.user.roles.sync` event through the shared
audit append before session issuance and completion commit. An unchanged set
does not append it. The injected audit-staging failure proof establishes that
roles, User/session state, refresh issuance, and completion all roll back.
The later `principal_event` reuse cleanup preserves the same request id,
principal, resource, permission, operation, and outcome fields; it introduces
no second sink or commit boundary.

### FIND-TASK-002-5 — Closed

Purge, consume, complete, and redeem rely on forced RLS and carry no manual
tenant-selection predicate. Tenant ownership is written on insert. The live
Postgres proof drives tenant B against tenant A's purge, consume, complete, and
redeem paths and proves tenant A retains the valid one-use transitions.

### FIND-TASK-002-6 — Closed

The cross-tenant state lookup is the narrow inherent
`WyrdPostgres::login_state_tenant` operation over its private runtime app pool.
Its `SECURITY DEFINER` function is granted only to `wyrd_app`, fixes its search
path, returns only one tenant UUID, and excludes unknown, expired, and consumed
state. The callback supplies only the typed SHA-256 hash.

### FIND-TASK-002-8 and FIND-TASK-002-9 — No domain regression

R2's required rustdoc and module-top import corrections do not change SQL,
persistence, transaction boundaries, generated contracts, or concurrency.
The separate lead-directed cleanups and flaky-test repairs recorded in R2 are
authorized and likewise do not alter the reviewed data boundary.

## Material findings

None.

## Verified properties without findings

- A callback learns only the tenant of a pending random state hash, then the
  tenant-scoped `UPDATE ... consumed_at IS NULL ... RETURNING` is the
  authoritative one-winner transition. Consumption commits before discovery
  or token exchange, so concurrent/replayed callbacks cannot repeat provider
  IO.
- Successful completion rechecks the exact Active connection before opening
  the write transaction. It resolves the tenant User, replaces roles, stages
  any role-sync event, issues and audits the session, inserts the refresh row
  with exact connection provenance, seals the completion, and attaches it to
  the consumed state in one commit. Any failure before commit persists none of
  those writes.
- Completion redemption deletes and commits before opening the sealed payload.
  This deliberately makes the handoff one-use even when local unsealing fails;
  another tenant and a concurrent or repeated redeemer receive no row.
- Session issuance and every refresh successor take the same tenant-scoped
  transaction advisory lock as connection activation, deactivation, and
  removal. Therefore a lifecycle mutation either commits first and blocks the
  issuance, or waits until the issuance transaction commits. No replica-local
  cache weakens the cutoff.
- Refresh rotation atomically consumes the active row with `UPDATE ...
  RETURNING`; concurrent use has one winner. A genuine replay commits family
  revocation and its canonical event before returning refusal. A stale or
  replaced human connection produces no successor and rolls back the attempted
  consume.
- The migration discards only transient login state, revokes live unbound
  `user` refresh rows, preserves machine rows, and removes tenant-email
  uniqueness so same-email replacement-provider subjects do not inherit a
  prior identity. Migration tests exercise upgrade preflight and repeated
  application.

## Verification limits

- I inspected the complete base-to-candidate diff, relevant callers and
  transaction owners, migrations, SQL, Postgres tests, identity journeys, and
  both remediation evidence tables. Candidate HEAD remained
  `d861845f3f5d89aca413857dcfb8c1bbfaee349d` through report creation.
- I did not independently run Cargo, Postgres, Keycloak/Dex, or identity lanes
  in this review window. Runtime confidence relies on the recorded successful
  focused proofs and broader lanes plus source inspection of those tests. This
  is a verification limit, not a missing required boundary in the candidate.
- TASK-003's BFF completion route and TASK-004's CLI handoff persistence remain
  intentionally outside TASK-002. This review covers their server-owned sealed
  completion primitive, not those future public workflows.

## Overall result

**PASS.** The cumulative candidate preserves forced-RLS tenant isolation,
least-disclosure callback routing, durable one-use transitions, transactional
audit/session rollback, exact connection provenance and provider-replacement
cutoff, cross-replica concurrency safety, and migration behavior. No material
tenancy, persistent-data, durability, transactionality, or concurrency finding
remains.
