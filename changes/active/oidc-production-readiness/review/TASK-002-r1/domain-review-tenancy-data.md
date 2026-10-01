# Domain review: tenancy, persistence, and transactional durability

## Immutable subject

- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `87de451ed87ad059cefd579eb15ef4b028a92547`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Domain result: **FAIL**

The candidate's runtime behavior is coherently tenant-bound: the cross-tenant callback lookup returns only a tenant id, the callback then consumes state atomically under that tenant's RLS transaction before provider IO, successful issuance/audit/completion commit together, completion redemption is one-use, and human refresh rotation inherits exact connection provenance while retaining the existing replay-family containment. Two repository-mandated tenancy/data access boundaries are nevertheless violated by new code.

## Reviewed boundary

This review traced:

1. tenant route resolution and insertion of hashed login state;
2. the `SECURITY DEFINER` state-hash lookup across RLS and transition into `TenantConn`;
3. consume-and-commit before OIDC discovery or token exchange;
4. active-connection revalidation, user/role persistence, refresh issuance, canonical allowed audit, sealed completion, and their common commit;
5. tenant-scoped one-use completion redemption;
6. human refresh provenance, connection-slot locking, successor rotation, and replay-family revocation;
7. migration replacement of transient state, revocation of provenance-free user refresh rows, and removal of email uniqueness; and
8. real-server and Postgres evidence for cross-tenant refusal, provider replacement, audit failure, migration, and replay behavior.

## Authority and source coverage

| Boundary | Governing authority | Source/evidence inspected | Result |
|---|---|---|---|
| Effective tenant selection | Spec `REQ-006`, `REQ-007`, `REQ-015`, `INV-001`, `INV-004`; security posture “Security principles”, “Tenant and data isolation” | `wyrd-auth/src/login.rs:88-142`; `wyrd-auth/src/callback.rs:113-187`; migration `20260925000001_auth_login_state_binding.sql:23-76`; `identity_e2e.rs:3235-3254` | PASS behavior; FAIL connection-capability shape (TD-001) |
| Tenant SQL isolation | `AGENTS.md` §2/§9; `architecture/agent-rules.md` TenantConn rules; security posture “Tenant and data isolation”; architecture patterns transaction/RLS rules | `wyrd-sql/src/queries/auth/login_state.rs:19-65,189-307`; migration RLS policy at lines 51-55 | FAIL (TD-002) |
| One-use state and provider-IO ordering | Task packet-local login contract; spec `REQ-007`, `INV-004` | `callback.rs:147-186`; `login_state.rs:36-45,238-263`; `auth/callback.rs` Postgres tests; `identity_e2e.rs` callback-refusal flow | PASS |
| Login completion atomicity/durability | Spec `REQ-008`, `REQ-017`; task issuance transaction; canonical audit rules | `callback.rs:212-272`; `issuance.rs:475-527`; `login_state.rs:47-65,265-307`; server callback Postgres audit-failure test; `identity_e2e.rs:3374-3416` | PASS |
| Refresh provenance, locking, rotation, replay | Spec `REQ-016`, `INV-004`; security posture access/refresh rules; task packet-local renewal contract | `refresh.rs:109-229`; `issuance.rs:455-527`; `refresh_tokens.rs`; migration lines 78-86; refresh Postgres replay tests | PASS |
| Migration behavior | Task migration obligations; data durability rules | `20260925000001_auth_login_state_binding.sql`; `wyrd-sql/tests/pg_migration.rs:469-526`; same-email provider-switch journey | PASS |
| Transaction ownership | `architecture/agent-rules.md`: a callee taking `&mut TenantConn` never commits/rolls back | SQL query functions leave lifecycle to owners; commits occur in `HumanConnections`, `AuthorizationCodeExchange`, and `LoginCompletions` | PASS |

## Material findings

### TD-001 — New cross-tenant resolver accepts a raw `PgPool`

- **Classification:** VIOLATION
- **Violated obligation:** `architecture/agent-rules.md` permits only `&mut TenantConn<'_>` for tenant work and `&OperatorPool` for cross-tenant privileged work in library function signatures, and explicitly bans `&sqlx::PgPool`. The security posture requires privileged pool capabilities to be excluded from handlers by construction.
- **Location:** `crates/wyrd/wyrd-sql/src/queries/platform/tenant_resolver.rs:51-60`; caller at `crates/wyrd/wyrd-auth/src/callback.rs:119-123`.
- **Evidence:** `resolve_by_login_state_for_app(pool: &PgPool, ...)` is newly added and executes against whichever raw pool a caller supplies. Its current caller extracts `postgres.app_pool()` and passes that raw pool across the library boundary. The SQL function itself is deliberately narrow, but the Rust API does not encode that the runtime application role is the only admissible capability.
- **Reachable consequence:** any present or future caller can invoke this cross-tenant resolver with an operator, migrator, fixture, or otherwise misconfigured pool without a type-level boundary. That defeats the repository's construction-time guarantee for cross-tenant access and makes correctness depend on every call site selecting the right role.
- **Required testable correction:** make the lookup an inherent operation on the existing `WyrdPostgres` owner so it uses that owner's private `wyrd_app` pool without putting any raw pool in a function signature. Keep the SQL function's one-column result and fail-closed behavior. Prove the callback still resolves a valid state and rejects unknown/consumed/cross-tenant state, then run `mise run check:tenant-isolation` and the focused callback/Postgres tests.

### TD-002 — New `TenantConn` queries duplicate RLS with manual tenant predicates

- **Classification:** VIOLATION
- **Violated obligation:** `architecture/agent-rules.md` states that `TenantConn`/Postgres RLS is the load-bearing tenant boundary and forbids manual per-query tenant filters on a `TenantConn` path because they can drift. `architecture/wyrd-security-posture.md` and the architecture patterns reference repeat the RLS-owned boundary.
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/login_state.rs:19-24,36-65,204-224,238-305`; the new unit assertion at lines 317-327 requires the prohibited shape.
- **Evidence:** purge, consume, complete, and redeem SQL all add `data_tenant_id = $1`, and their Rust functions derive/bind that value from `TenantConn`. The table already has enabled and forced RLS with `data_tenant_id = wyrd.current_tenant()` in migration lines 51-55. The candidate's test asserts the duplicated predicates rather than proving isolation through RLS.
- **Reachable consequence:** the same tenant boundary is encoded twice and can diverge as the table/query evolves; reviewers and maintainers can no longer treat `TenantConn` plus policy as the single authoritative boundary. This is exactly the drift mode the mandatory rule prohibits, even though current predicates happen to agree.
- **Required testable correction:** keep the tenant id on insert as row ownership data, but remove manual tenant-selection predicates and their binds from TenantConn-backed purge/consume/complete/redeem operations. Replace the string-shape assertion with a focused Postgres proof that tenant B cannot consume, complete, or redeem tenant A's row while tenant A can do so once. Run that exact test and `mise run check:tenant-isolation`.

## Verified properties without findings

- `state_hash` is globally unique and the definer function exposes only the owning active/unconsumed tenant id; the subsequent `UPDATE ... consumed_at IS NULL ... RETURNING` is the authoritative one-winner transition.
- `AuthorizationCodeExchange::complete` commits consumption before the first provider network operation, so replay and concurrent callbacks cannot repeat provider exchange.
- `finish_id_token_exchange` revalidates provider trust, opens a fresh tenant transaction, resolves `(issuer, subject)`, replaces roles, locks/rechecks the exact active connection during `issue_human_session`, appends the allowed canonical audit through the shared issuer, inserts the provenance-bound refresh row, seals the completion, and commits only after all steps succeed.
- `LoginCompletions::redeem` deletes and commits the tenant-scoped sealed completion before unsealing; any local unseal failure loses that completion rather than permitting replay.
- Human refresh consumes atomically, carries `human_connection_id` and revision into successors, reuses the shared connection-slot lock, and retains audited family revocation for stale-token replay.
- The migration deliberately drops transient login state, revokes only live unbound `user` refresh rows, leaves machine rows unaffected, and removes tenant-email uniqueness so a replacement issuer cannot inherit identity by email.

## Verification limits

- I inspected the complete base-to-candidate diff and current candidate source at `87de451ed87ad059cefd579eb15ef4b028a92547`; the candidate commit did not change during this review.
- I relied on the task's recorded green command evidence and inspected the named tests. I did not launch Cargo/Postgres/identity lanes because review agents share one checkout/target and repository rules prohibit overlapping Cargo runs; no independent runtime result was produced by this domain review.
- TASK-003's BFF redemption route and TASK-004's CLI handoff persistence are intentionally outside this task. This review covers the server-owned completion primitive they consume, not those future public journeys.

## Overall result

**FAIL.** The tenant-routing, one-use consumption, persistence, refresh, migration, and audit transaction behavior is otherwise sound, but TD-001 and TD-002 violate mandatory connection-capability and RLS ownership rules on newly added code. Both are bounded corrections within the approved task and require no specification revision.
