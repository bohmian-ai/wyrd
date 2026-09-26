# Domain review: tenancy, persistence, and transactional durability

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `8b201627c0a957dccf46649d00c8c205689bc5de`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-002-r1/TASK-002-R1-tenant-login-corrections.md`
- Domain result: **PASS**

## Reviewed boundary

This review traced the cumulative base-to-candidate implementation through:

1. hashed login-state insertion under tenant RLS and the narrow cross-tenant
   state-hash-to-tenant lookup;
2. callback transition from that lookup into the owning `TenantConn`, atomic
   one-use consumption before provider IO, and replay/concurrency behavior;
3. verified identity resolution, durable role replacement, role-change audit,
   token/refresh issuance, sealed completion, and their shared transaction;
4. tenant-scoped, one-use completion redemption;
5. exact human-connection provenance on initial and successor refresh rows,
   active-connection locking, rotation, and replay-family containment;
6. migration replacement of transient login state, revocation of live
   provenance-free User refresh rows, and preservation of machine rows; and
7. the prior tenancy/data findings `FIND-TASK-002-3`,
   `FIND-TASK-002-5`, and `FIND-TASK-002-6`.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Effective tenant selection and least disclosure | Spec `REQ-006`, `REQ-007`, `REQ-015`, `INV-001`, `INV-004`; task packet-local login contract; security posture tenant isolation | `wyrd-sql/src/postgres.rs:150-180`; migration `20260925000001_auth_login_state_binding.sql:55-74`; `wyrd-auth/src/callback.rs:96-145`; `wyrd-sql/tests/pg_login_state.rs` lookup proof | PASS |
| Forced-RLS state transitions | `AGENTS.md` §§2, 9; `architecture/agent-rules.md` `TenantConn` rules; architecture constraints and patterns | `wyrd-sql/src/queries/auth/login_state.rs:23-64,140-250`; migration lines 23-53; `pg_login_state.rs::login_state_transitions_are_confined_to_the_owning_tenant` | PASS |
| One-use state and provider-IO ordering | Task packet-local bounded-state contract; spec `REQ-007`, `INV-004` | `wyrd-auth/src/callback.rs:130-168`; `login_state.rs:39-47,180-209`; callback refusal journey and SQL transition proof | PASS |
| Login completion atomicity and durability | Spec `REQ-008`, `REQ-017`; task issuance transaction; canonical audit rules | `wyrd-auth/src/callback.rs:200-266`; `wyrd-auth/src/issuance.rs:475-528`; `login_state.rs:211-250`; callback Postgres audit-failure proof | PASS |
| Role-change audit rollback | Spec `REQ-017`; remediation `FIND-TASK-002-3`; canonical same-transaction audit requirement | `wyrd-sql/src/queries/auth/role_assignments.rs:26-48,117-145`; `wyrd-auth/src/callback.rs:227-265,557-574`; server callback tests `changed_roles_are_audited_once_and_unchanged_roles_never` and `a_failed_role_sync_audit_rolls_back_the_whole_login` | PASS |
| Completion redemption | Task packet-local sealed one-use handoff contract | `wyrd-auth/src/login.rs:151-189`; `login_state.rs:58-64,234-250`; cross-tenant and repeat-redemption tests | PASS |
| Refresh provenance, locking, rotation, and replay | Spec `REQ-016`, `INV-004`; task renewal contract; security posture access/refresh rules | `wyrd-auth/src/refresh.rs:82-228`; `wyrd-auth/src/issuance.rs:454-528`; `wyrd-sql/src/queries/auth/refresh_tokens.rs`; refresh Postgres concurrency/replay tests | PASS |
| Migration behavior | Task migration obligations; persistent-data constraints | migration `20260925000001_auth_login_state_binding.sql:20-90`; prior connection migration; `wyrd-sql/tests/pg_migration.rs:472-529` | PASS |
| Transaction ownership | `architecture/agent-rules.md`: a callee accepting `&mut TenantConn` never commits or rolls back | Login-state, role, refresh, and issuance query functions leave lifecycle to their owning services; commits remain at workflow/route boundaries | PASS |

## Prior-finding closure

### FIND-TASK-002-3 — Closed

`replace_user_roles` now reports whether the one-statement replacement added
or removed durable membership. The callback appends exactly one canonical
`auth.user.roles.sync` event only on change, before `issue_human_session`,
sealed completion, and the shared commit
(`wyrd-auth/src/callback.rs:227-265`). The unchanged-set test proves no sync
event; the injected staging failure proves the role update, User/session,
refresh row, and completion roll back together. The existing token-exchange
audit remains in the same issuance transaction.

### FIND-TASK-002-5 — Closed

Purge, consume, complete, and redeem no longer carry or bind a manual
`data_tenant_id` predicate (`wyrd-sql/src/queries/auth/login_state.rs:23-64,
151-250`). Tenant ownership is written on insert, while enabled and forced RLS
is the sole selector for every transition. The replacement Postgres proof
drives tenant B against tenant A's purge, consume, complete, and redeem paths,
then proves tenant A's valid transitions remain one-use.

### FIND-TASK-002-6 — Closed

The state-hash lookup is now the narrow inherent
`WyrdPostgres::login_state_tenant` operation, which uses the owner's private
runtime application pool internally (`wyrd-sql/src/postgres.rs:150-180`). The
callback supplies only a typed SHA-256 state hash. The `SECURITY DEFINER`
function remains granted only to `wyrd_app`, returns one nullable tenant UUID,
fixes its search path, and filters unknown, expired, and consumed state. Its
Postgres proof covers two tenants plus unknown, expired, and consumed inputs.

## Material findings

None. The reviewed tenancy and persistent-data boundary has no independently
validated material defect.

## Verified properties without findings

- A raw route key is used only to find a tenant's current Active connection at
  login start. After state is written, the callback derives tenant and
  connection only from server-owned state; the definer lookup discloses only
  the owning tenant UUID for a pending random state hash.
- The lookup-to-consume sequence fails closed under races: globally unique
  `state_hash` names one row, and the tenant-scoped `UPDATE ... consumed_at IS
  NULL ... RETURNING` is the authoritative one-winner transition. Consumption
  commits before discovery or token exchange, so a concurrent or replayed
  callback cannot repeat provider IO.
- Successful completion revalidates the exact Active connection, then resolves
  the User, replaces roles, stages any role-sync event, issues and audits the
  session, inserts its provenance-bound refresh row, seals the completion, and
  attaches it to the consumed state in one tenant transaction. Any failure
  before commit rolls all of those writes back.
- Completion redemption deletes the tenant-visible, unexpired row and commits
  before opening the sealed value. That intentionally prefers losing an
  unusable completion over permitting replay; another tenant and a second
  redemption see no row.
- Human refresh consumes the active row atomically, rejects machine or
  provenance-free rows, takes the shared connection-slot lock, requires the
  exact connection ID/revision to remain Active, and copies that binding to
  the successor. The route commits replay-family revocation and its canonical
  audit event before returning the replay refusal.
- The migration discards transient state, revokes only live unbound `user`
  refresh rows, leaves machine rows live, and removes tenant-email uniqueness
  so a replacement provider cannot inherit an existing identity by email.

## Verification limits

- I inspected the complete base-to-candidate diff, all changed code in this
  domain, the relevant callers and transaction owners, the named Postgres and
  journey tests, and the remediation's recorded green evidence. The candidate
  remained `8b201627c0a957dccf46649d00c8c205689bc5de` through report creation.
- I did not independently run Cargo, Postgres, migration, or identity lanes.
  Review agents share this checkout and target directory, and repository rules
  prohibit overlapping Cargo-backed runs. Runtime confidence therefore relies
  on the candidate's recorded successful focused tests and broader lanes plus
  source inspection of those tests.
- TASK-003's BFF completion route and TASK-004's CLI handoff persistence remain
  intentionally outside TASK-002. This review covers the server-owned sealed
  completion primitive they consume, not those future public workflows.

## Overall result

**PASS.** Prior findings `FIND-TASK-002-3`, `FIND-TASK-002-5`, and
`FIND-TASK-002-6` are closed. The cumulative candidate preserves forced-RLS
tenant isolation, least-disclosure state routing, atomic one-use transitions,
transactional audit/session rollback, migration safety, and refresh-family
provenance and replay containment within the reviewed boundary.
