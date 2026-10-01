# Tenancy, Persistent Data, Durability, and Concurrency Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `fa2bda92a7e79471b79b607870c1e86a9f35639c`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation inputs: TASK-002-R1 through TASK-002-R8 in their supplied review directories

The candidate remained exactly `fa2bda92a7e79471b79b607870c1e86a9f35639c` before this report was written.

## Reviewed boundary

This review traced the tenant login data path from route-key resolution through
`WyrdPostgres`, `TenantConn`, forced RLS login state, the cross-tenant state-hash
lookup, one-use consumption and completion redemption, OIDC User identity
creation, role replacement and audit, human-session issuance, refresh rotation,
replay containment, administrative User revocation, connection lifecycle
serialization, migration behavior, and the corresponding Postgres and
real-server tests. It also traced every production caller of
`replace_user_roles`, `lock_refresh_family`, `issue_human_session`,
`refresh_by_hash`, `revoke_refresh_family`, and the login-state transitions.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Tenant transaction ownership and RLS | `AGENTS.md` §§3, 9, 11; `architecture/agent-rules.md` raw-pool, `TenantConn`, and RLS rules; security posture tenant isolation | `wyrd-sql/src/tenant_conn.rs`; login-state, identity, role, refresh, connection, and principal queries; `pg_login_state.rs`; tenant-isolation evidence | FAIL: TD-002 |
| Login-state durability and one use | TASK-002 packet-local contract; REQ-006/007; INV-001/004 | migration `20260925000001_auth_login_state_binding.sql`; `login_state.rs`; `login.rs`; `callback.rs`; `postgres.rs::login_state_tenant`; Postgres state tests | PASS |
| Tenant identity and same-issuer separation | REQ-008/014/015; INV-002/003 | `callback.rs::ensure_user_identity`; user/identity queries and constraints; callback and identity journeys | PASS |
| Provider-derived role replacement and atomic audit | REQ-008/017; INV-003; canonical audit transaction rules | `callback.rs::finish_id_token_exchange`; `role_assignments.rs::replace_user_roles`; `issuance.rs::issue`; callback Postgres tests and role-change journey | FAIL: TD-001 |
| Refresh-family rotation, replay, and revocation | TASK-002 renewal contract; REQ-016; INV-004; R4/R6/R7 remediation decisions | `refresh.rs`; `refresh_tokens.rs`; `issuance.rs::issue_human_session`; `revoke.rs`; served refresh/revocation commit owners; deterministic overlap tests | PASS |
| Connection replacement/deactivation/removal cutoff | REQ-014/016; fixed family-before-connection order | `connections.rs`; human-connection SQL; callback and issuance rechecks; deactivation/rotation overlap proof | PASS |
| Migration and persistent-data transition | REQ-016; migration/test rules | login-state replacement migration; legacy human-refresh revocation; email uniqueness removal; `pg_migration.rs` | PASS |

## Material proposed findings

### TD-001 — INCORRECT / concurrency: concurrent callbacks can union provider roles and mint authority absent from one assertion

- **Violated obligation:** REQ-008 requires a successful callback to map that verified provider assertion's groups only to valid tenant roles, with no provider claim granting other authority. TASK-002 requires role persistence, issuance, and audit to be one correct tenant transaction. The R4–R7 serialization contract requires human-session issuance for one User family to serialize before authority is read and minted.
- **Exact location:** `crates/wyrd/wyrd-auth/src/callback.rs:228-256`, especially `replace_user_roles` at lines 241–246 before `issue_human_session`; `crates/wyrd/wyrd-auth/src/issuance.rs:490-493`, where the family lock is acquired only after that replacement; `crates/wyrd/wyrd-sql/src/queries/auth/role_assignments.rs:36-53`.
- **Evidence:** `replace_user_roles` is the sole production writer for OIDC User roles and runs before the existing per-User refresh-family advisory lock. Its one-statement delete/add replacement is atomic in one transaction but is not serialized against another callback for the same User. From an initially empty role set, callback A mapped only to role A and callback B mapped only to role B can concurrently execute their replacements: each statement sees no committed row from the other and inserts a different primary-key row. A then takes the family lock, mints with A, and commits. B subsequently takes the lock, and its later `list_user_roles` statement sees committed A plus its own B, so it mints and commits A+B. The durable set also ends A+B although neither verified assertion requested that union. Foreign-key checks on the same User are compatible and do not serialize these distinct role rows. The existing tests exercise sequential role changes and audit rollback, not two callbacks for one User with disjoint mapped groups.
- **Observable consequence:** a callback whose signed groups map only to role B can receive an access token carrying role A's permissions as well. Concurrent login can therefore retain or add tenant authority not present in that callback's verified provider assertion, and the durable role set no longer equals the asserted set.
- **Required testable correction:** after `ensure_user_identity` has returned the canonical User id and before `replace_user_roles`, acquire the existing tenant-qualified `lock_refresh_family(conn, "user", principal_id)`. Keep the role replacement, role-sync audit, session issuance, completion, and commit on the same `TenantConn`; `issue_human_session` may safely reacquire the transaction-scoped lock. This reuses the established family-before-connection order and serializes the entire provider-authority mutation before issuance without a new lock, table, trait, or isolation mode. Add one deterministic Postgres callback/service test that pauses callback A after it owns the family lock, starts callback B for the same User with a disjoint mapped role set, proves B waits on the advisory lock, then commits both and proves B's token and the final durable assignments contain only B's asserted role set. Preserve exact role-sync audit cardinality for each real change.

### TD-002 — VIOLATION / connection identity: the new login owner reaches through `WyrdPostgres` to a raw `PgPool`

- **Violated obligation:** `architecture/agent-rules.md` permits only `&mut TenantConn<'_>` and `&OperatorPool` in library connection signatures, reserves pool ownership to `WyrdPostgres`, and explicitly bans propagating raw `PgPool`. The storage pattern requires the connection type to encode the database role/capability.
- **Exact location:** `crates/wyrd/wyrd-auth/src/login.rs:89`; the invoked signature is `crates/wyrd/wyrd-sql/src/queries/platform/tenant_resolver.rs:17-20`.
- **Evidence:** the materially changed `HumanConnections::begin_login` calls `resolve_by_slug_for_app(self.postgres().app_pool(), ...)`. The resolver publicly accepts `&PgPool`; unlike the candidate's corrected state-hash lookup on `WyrdPostgres::login_state_tenant`, its type does not constrain the SECURITY DEFINER lookup to the runtime app-role pool. The current caller supplies the intended pool, so this is a boundary violation rather than a demonstrated cross-tenant disclosure, but it is an explicit repository constraint on this newly added login path.
- **Observable consequence:** the pre-authentication tenant-selection path depends on caller discipline for database role identity and exposes the raw pool past its owner, defeating the compile-time connection-capability boundary used to keep cross-tenant lookups narrow.
- **Required testable correction:** expose the slug-to-tenant lookup as a narrow inherent operation on the existing `WyrdPostgres` owner, using its private app pool internally, and make `HumanConnections::begin_login` pass only the typed slug. Do not add a repository trait or another pool wrapper. Static source proof must show the changed login path no longer calls `app_pool()` or a raw-pool resolver; retain the existing unknown/suspended/deleted generic refusal and tenant-resolution tests.

## Verification limits

- Independently inspected the complete cumulative diff and current source for the boundary above, traced all production callers named in this report, confirmed the exact candidate twice, and ran `git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174..fa2bda92a7e79471b79b607870c1e86a9f35639c` successfully.
- Reviewed the recorded successful evidence for `test:principals:unit`, `test:principals:integration`, `test:sql`, `test:identity:journey` (27 journeys), `check:tenant-isolation`, `check:from-pools-allowlist`, format, lints, codegen, and every focused R4/R6/R7 overlap test. Those commands were not rerun in this review wave.
- Existing deterministic proofs credibly cover login-state RLS/one-use semantics, ancestor replay versus rotation, rotation versus administrative revocation, both initial-issuance/revocation orderings, and connection deactivation versus rotation. No existing proof covers concurrent callbacks for the same User with disjoint mapped groups; that is the closure gap for TD-001.
- Provider qualification against live Okta/Entra accounts and TASK-003/TASK-004 BFF/CLI consumers are outside this task's implemented boundary and were not treated as defects here.

## Overall result

**FAIL**

The login-state, tenant isolation, refresh-family, administrative revocation,
initial issuance, migration, and connection-cutoff mechanisms are otherwise
coherent, but TD-001 leaves a reachable authority-expansion race and TD-002
leaves the newly changed pre-authentication tenant lookup outside the mandated
connection-capability boundary.
