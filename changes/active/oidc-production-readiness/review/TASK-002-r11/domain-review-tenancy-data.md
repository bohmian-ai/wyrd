# Domain review: tenancy, persistent data, and concurrency

## Immutable subject and boundary

- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `bae424cc647dad4be80e7debec976d0b7b3e4cf8` (checked-out HEAD)
- Approved authority: `spec.md` revision 5, original `TASK-002-tenant-login.md`, both R10 remediation packets, `AGENTS.md`, `architecture/agent-rules.md`, and `architecture/wyrd-security-posture.md`.
- Reviewed scope: tenant slug resolution; `OperatorPool` and `TenantConn` SQL capabilities; forced RLS and login-state transaction boundaries; User status, role, session, and refresh writers; family and connection lock ordering. The repository has no `.codegraph/` directory, so source and caller tracing used Git and `rg`.

## Authority and source coverage

| Boundary | Authority | Source and caller evidence | Result |
|---|---|---|---|
| Pre-tenant slug resolution | R10 human-directed packet §1; agent-rules SQL capability rule; security posture tenant identity rule | `wyrd-sql/src/postgres.rs:152-175` owns the only Rust resolver; `queries/platform/tenant_resolver.rs:24-46` takes `&OperatorPool`. `wyrd-auth/src/login.rs:83-97`, `wyrd-server/src/auth/jwt_bearer.rs:79-90`, and `boot/issuer.rs:70-87` call that owner. An absent operator pool returns `InsufficientPrivilege`; the SQL function returns only an active tenant id or `None`. | PASS |
| Scoped SQL capabilities | Agent-rules raw-pool and transaction-ownership rules; R10 human-directed packet §1 | Production query modules, auth, auth routes, and boot have no raw `PgPool` signatures, fields, or `app_pool()` borrow in the checked source. Auth audit and workload resolver hold `WyrdPostgres`; storage maintenance takes `OperatorPool`. `scripts/check_tenant_isolation.py:87-101,390-405` scans these owners, and the distinct construction check remains. | PASS |
| RLS and one-use login state | REQ-006/007/015; INV-001/004; task packet callback contract | `WyrdPostgres::login_state_tenant` is the narrow state-hash-only pre-tenant lookup. `queries/auth/login_state.rs:23-86,159-276` uses `TenantConn` for insert, consume, completion, and redemption. The callback consumes before provider IO; successful identity, role, issuance, audit, and sealed completion share one caller-owned transaction in `wyrd-auth/src/callback.rs:216-276`. | PASS |
| User authority writers | R10 human-directed packet §3; REQ-008/016/017; agent-rules lock and audit rules | Production writes route through callback role replacement (`callback.rs:239-276`), human session issuance (`issuance.rs:482-537`), refresh rotation (`refresh.rs:119-219`), and administrative User revocation (`revoke.rs:42-69`). SQL writers are `role_assignments.rs`, `refresh_tokens.rs`, and `revocation.rs`; other discovered callers of direct User role/status helpers are tests or fixtures. | PASS |
| Serialization and lock order | R10 human-directed packet §3; REQ-016; INV-004 | Callback locks the User family before role replacement; issuance locks family then connection slot before status/grant reads and refresh insert; refresh derives the family from an RLS-confined stored hash, then locks before consume/classification; revocation now locks before `user_by_id`, suspension, and family revoke. All locks are transaction scoped, and no `&mut TenantConn` callee commits or rolls back. | PASS |
| Sealed completion durability | REQ-005 revision 5; R10 human-directed packet §4 | `connections.rs:463-471` refuses activation without a keyring, including a secretless provider. `login.rs:83-86,166-190` requires it at begin and redemption; redemption commits the one-use delete before opening the sealed bytes. `callback.rs:205-276` seals and stores the pair in the same issuance transaction. | PASS |

## Prior finding and remediation closure

R10's tenancy review found no material domain defect at `6e21d8e`. The cumulative candidate still closes the earlier state, RLS, identity, replay, revocation, first issuance, role serialization, and slug owner findings (`FIND-TASK-002-1`, `-3`, `-5` through `-7`, `-12`, `-14`, `-15`, `-18`, and `-22`) at their existing owners. The R11 fix moves User revocation's family lock ahead of its first User read (`revoke.rs:50-66`) and reuses the existing Postgres advisory-lock wait helper in the refresh overlap test. The latest fix did not change the production slug resolver, SQL capability check, callback transaction, or RLS queries.

## Material proposed findings

None within this domain. The reviewed production paths retain one pre-tenant resolver, scoped SQL capability types, caller-owned atomic transactions, and a consistent family-before-connection lock order.

## Verification limits

- Fresh `git diff --check` for the complete base-to-candidate range passed. I inspected the candidate source, cumulative changed surfaces, latest remediation diff, and recorded focused and broad verification in the R10 packets; I did not rerun Cargo, Postgres, or the identity journey in this review slot.
- The remediation record reports passing `mise run check:tenant-isolation`, `mise run check:from-pools-allowlist`, format, lints, exact slug and overlap selectors, and `mise run test:identity:journey` (27/27). These are recorded implementation results, not fresh review executions.
- TASK-003 browser flow-cookie redemption and TASK-004 CLI verifier-held handoff remain downstream. Their caller authentication is not established by TASK-002's SQL one-use proof.

## Overall result

**PASS**
