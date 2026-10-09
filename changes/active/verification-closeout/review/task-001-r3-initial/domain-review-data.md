# Data domain review — TASK-001 R3

**Subject:** `c46afdcac` → `437205debc628538ba6aa4ec828601c7c40145b4`, approved spec revision 2. **Result: FAIL.**

## Boundary and authority

Reviewed the persistent principal directory, user and service-account Role assignments, tenant RLS, login synchronization, issuance, principal routes, and the database migration/readiness path. Governing requirements are spec REQ-001/REQ-002 and R3 decisions 2, 6, and 10; `AGENTS.md` §§2, 3, 9, 11–12; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md` (tenant/data isolation); and `architecture/references/doctrine/architecture-constraints.md` (TenantConn/RLS).

## Source coverage and assessment

- `migrations/20260601000001_auth.sql` adds assignment `source` to the original table definition; `20261002000100_workload_role.sql` is removed. `wyrd-sql::MIGRATOR`, `MigrationLease::apply`, and `OperatorPool::verify_migrations` enforce an embedded migration ledger and checksums at migration and serving readiness.
- `role_assignments.rs` stores source in the user assignment key, confines IdP replacement and direct revocation to their respective source, reads distinct effective Role names for issuance, and orders source-bearing assignments. The prior Card-addressed grant wrote only service-account assignments; the old login writer owned user assignments. This supports labeling legacy user rows as `idp` in a forward migration.
- `principal_directory.rs` selects only non-deleted users and Service/Agent accounts. The routes obtain a caller-derived `TenantConn`, authorize before writes, resolve principal and Role inside its tenant transaction, and use conflict handling for repeated grants. The tables retain FORCE RLS and composite tenant foreign keys. Callback serialization through `lock_refresh_family` covers simultaneous IdP replacements for the same user. No separate material RLS or ordinary idempotency defect was established.
- `seed_builtin_roles_for_tenant` inserts only at tenant provisioning and leaves existing rows untouched. `auth_projection::grant_default_role` grants `workload` only when that Role exists. Existing custom Roles remain outside that seed, as intended.

## Material finding

### DATA-001 — Existing databases cannot upgrade to sourced assignments and four built-in Roles

**Classification:** MISSING / REGRESSION. **Obligation:** REQ-001 requires persistent `idp`/`direct` provenance, and REQ-002 requires exactly four built-in Roles while preserving existing custom Roles. Wyrd supports migrated serving databases, not only empty ones.

**Location:** `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:106-116`; removed `crates/wyrd/wyrd-sql/migrations/20261002000100_workload_role.sql`; `crates/wyrd/wyrd-auth/src/seed.rs:19-45`.

**Evidence and consequence:** The candidate edits an already-versioned migration, changing its checksum. An existing database retains the old checksum in `wyrd._sqlx_migrations`; `OperatorPool::verify_migrations` compares it to the candidate's embedded checksum at `schema_check.rs:76-97`, and `MigrationLease::apply` runs SQLx migration validation. The database also retains the old `auth_user_roles` table without `source`, so candidate login and assignment SQL cannot operate there. There is no new migration that adds and backfills `source`, changes the primary key, reconciles old built-in Role rows and their grants, and installs the new `editor`/`viewer` set. Provisioning seed runs for new tenants only and uses `ON CONFLICT DO NOTHING`, so it cannot repair existing tenants. Removing the later workload migration does not perform that reconciliation. Existing deployments either fail migration/readiness or, if checksum validation were bypassed, fail Role writes and continue to expose retired built-ins. Fresh-database tests cannot prove an upgrade.

**Required correction and proof:** Restore previously released migration files unchanged. Add a forward migration that converts existing user grants to `idp` (the prior login-owned source), installs the source-aware key, updates built-in permissions and assignments to the approved four-Role model without deleting custom Roles, and handles the previously applied workload migration. Prove migration from the base schema with populated users, custom Roles, existing Card principals, and assignments; then prove old and new login/assignment operations and serving readiness. Do not reset the migration ledger or require a fresh database.

## Verification limits

The implementation evidence reports green fresh-schema principal integration, SDK journeys, `check:tenant-isolation`, and codegen checks. The reviewed migration integration test creates a fresh database and reruns the candidate migrator; it does not start with the base commit's applied migration ledger and populated rows. This was a static audit of the immutable candidate; no test lane was rerun and no other review report was read.
