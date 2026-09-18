---
task: TASK-004
title: Tenant provisioning and bootstrap-key removal
spec: SPEC-admin-principals
spec_revision: 6
obligations: [REQ-025, REQ-026, REQ-027, REQ-028, REQ-036, REQ-038, REQ-040, INV-006, INV-010, AC-002, AC-007]
depends_on: [TASK-003]
---

## Objective

A global administrative credential creates a usable tenant in one authorized
operation: the tenant row, its tenant administrative principal, that principal's
administrative grant, its initial credential, and the tenant's required initial
state including builtin role seeding — after which the tenant is immediately
configurable with no further platform involvement. The tenant lifecycle
distinguishes provisioning, ready, failed, and suspended, and a tenant is never
reachable before it is ready. `wyrd-server bootstrap-key`, its fabricated
`system/bootstrap-admin@1.0.0` CardRef, and `SYSTEM_OPERATOR_ID` are removed.

## Constraints

- Provisioning is transactional where one transaction suffices and otherwise
  resumable to the same outcome. Retried or concurrent creation for the same
  requested identity converges on exactly one tenant with exactly one
  administrative principal, with no duplicates and no orphaned credentials.
- A failed or partial provisioning never presents as a usable tenant, and never
  leaves a tenant that appears usable but lacks its administrative principal.
- The tenant administrative credential plaintext is returned once in the
  creating response and is never recoverable afterwards.
- Directory consumers that service live tenants must not observe a
  non-ready tenant; the existing active-tenant directory read is included.
- A suspended tenant refuses authentication and authorization for its principals
  without destroying state, principals, or grants.
- Tenant directory writes run on the operator boundary; tenant-scope writes run
  under `TenantConn` RLS.
- Preserve from the removed command: the single-transaction shape with rollback,
  the once-printed plaintext that is never logged, and reuse of the production
  issuance and audit seams.
- Non-goal: billing, plans, quotas, custom domains, tenant deletion or data
  destruction, an `Organization` noun, a customer signup UI.

## Relevant Surface

- `crates/wyrd/wyrd-sql/migrations/`, `crates/wyrd/wyrd-sql/src/queries/platform/tenants.rs`
  — tenant status contract and the active-tenant directory read.
- `crates/wyrd/wyrd-server/src/components/` — the administrative HTTP surface.
- `crates/wyrd/wyrd-server/src/boot/bootstrap.rs`,
  `crates/wyrd/wyrd-server/src/main.rs`,
  `crates/wyrd/wyrd-server/tests/pg_bootstrap_key.rs`, and the
  `cli:bootstrap-key` task in `mise.toml` — removed by this task.
- `crates/wyrd/wyrd-auth/src/seed.rs` — idempotent builtin role seeding.
- `crates/wyrd/wyrd-cli/` — the tenant creation command.
- `crates/wyrd/wyrd-testing/src/server.rs`, `principal.rs` — fixture seams that
  currently depend on the removed bootstrap path.
- `docs/src/content/docs/self-hosting/` — running the server, authentication,
  local development.

## Approach

1. Extend the tenant lifecycle contract with provisioning and failure states and
   make every live-tenant consumer respect readiness.
2. Implement provisioning as one server-owned operation authorized on the
   platform plane, ordered so the tenant becomes ready only after its
   administrative principal, grant, credential, and initial state exist.
3. Define its idempotency and retry semantics durably, so concurrent and
   repeated creation converge rather than duplicate.
4. Expose the typed HTTP contract with stable errors, and project it as the CLI
   tenant creation command.
5. Add tenant listing, inspection, suspension, and resumption on the platform
   plane.
6. Remove `bootstrap-key`, its fabricated CardRef, `SYSTEM_OPERATOR_ID`, its
   test, and its task, and move the test fixture seams onto the new path.
7. Rewrite the self-hosting documentation to the three-command operator journey.

## Acceptance Criteria

- One authorized creation yields a ready tenant, an administrative principal
  holding tenant administration, seeded builtin roles, and a returned credential
  that immediately authenticates and configures the tenant with no further
  platform-plane call.
- An injected failure at each provisioning stage leaves no usable tenant, is
  observable as failed, and a retry converges on exactly one correct tenant.
- Concurrent creation for the same requested identity yields exactly one tenant
  and one administrative principal.
- A non-ready tenant is invisible to the live-tenant directory read and refuses
  authenticated tenant operations.
- Suspending a tenant stops its principals authenticating and stops their live
  tokens at the authorization epoch; resuming restores access with state and
  grants intact.
- A tenant-scope credential invoking any platform operation is refused without
  revealing the tenant directory.
- No reference to `bootstrap-key`, `bootstrap-admin`, or `SYSTEM_OPERATOR_ID`
  remains in code, tasks, tests, or documentation.
- Documentation describes install → initialize → create tenant → configure, with
  no step requiring database access.

## Verification

Scope is `VER-001` through `VER-006`.

```bash
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-sql -p wyrd-server -p wyrd-cli -p wyrd-testing --all-targets
mise run codegen:check
mise run docs:check
```

The primary proof is a real-server user journey driving initialization, tenant
creation, and first tenant configuration through the shipped CLI against a
running server with repository-managed Postgres — no database access at any
step. Provisioning failure, retry, and concurrency coverage is Postgres-backed
integration proof through `scripts/postgres/with-test-postgres.sh`. Add the
focused `mise` lane for this capability and run it, plus exact nextest
expressions for the tests you add.
