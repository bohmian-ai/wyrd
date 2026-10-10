---
id: TASK-001
spec: SPEC-local-server-bootstrap@4
depends_on: []
maps: [REQ-006, REQ-015, REQ-016, INV-002, INV-003, AC-003, AC-007, AC-008]
---

## Outcome and Value

One `WYRD_DATABASE_URL` naming an ordinary, operator-created login that owns
the database is sufficient for every server mode; an explicitly set
`WYRD_PLATFORM_DATABASE_URL` selects a separate platform login. Wyrd creates
no roles and grants nothing to named roles. Tenant isolation is pool-scoped
row-level security that holds with one or two logins, and boot refuses logins
that would silently defeat it. Docs state exactly which privileges each login
needs.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
| --- | --- | --- | --- | --- | --- |
| DSN resolution | `wyrd_sql::dsn::ResolvedDsns` | `wyrd-server::main`, `WyrdPostgres`/`ValaPostgres::connect_from_dsns`, `dsn::tests` | Platform URL is required; operator scope is a role attribute | Optional platform URL; platform and catalog DSNs carry `app.operator=on` through the existing `options=` suffix | None |
| Tenant isolation policies | Wyrd and Vala migrations (`tenant_isolation`, `admin_cross_tenant TO wyrd_platform_admin`, named grants, role-existence checks) | Every RLS table, `SECURITY DEFINER` lookups, migration seeds | Cross-tenant access depends on a provisioned `BYPASSRLS` role; platform-plane tables rely on named grants | Rewrite migrations in place: `wyrd.operator_session()`, an `operator_access` policy `TO CURRENT_USER` on every table, forced RLS on platform-plane tables, no named grants | One SQL predicate function; no new table |
| Boot posture | `WyrdPostgres::validate_schema`, `ValaPostgres::validate_schema`, `OperatorPool` checks in `schema_check.rs` | Server boot, `pg_migration`, dev-fixture refusal tests | Checks prove named roles and grants | Replace with login-property checks (superuser, `BYPASSRLS`, owner membership, `TRUNCATE`) and the new policy shape | No new check owner |
| Test Postgres | `with-test-postgres.sh`, `bootstrap/roles.sql`, `wyrd-dev-fixtures::pg`, startup/role scripts | Every SQL, journey, and startup lane | Provision named roles | Provision operator-style logins in the test harness as an operator would | None |
| Operator guidance | Agent rules, security/deployment/tenancy architecture, self-hosting docs | Local and production setup guides | Describe named roles and a shared-admin bypass | State per-login privileges for one- and two-login deployments | No new documentation system |

Preserve `TenantConn` and `OperatorPool` query boundaries, server permission
checks, and production signing/operator-key hardening. Do not add another DSN,
role bootstrap, compatibility alias, or alternate migration path.

## Approach

1. Resolve the optional platform URL in `ResolvedDsns`; operator and catalog
   DSNs set `app.operator=on`.
2. Rewrite migrations: drop role checks and named grants, add
   `wyrd.operator_session()`, give every table forced RLS with
   `operator_access TO CURRENT_USER`, and set the flag on cross-tenant
   `SECURITY DEFINER` functions and the migration session.
3. Replace named-role boot checks with login-property and policy-shape checks.
4. Move test harnesses to operator-created logins; delete `roles.sql`.
5. Reconcile authority and docs.

## Ordered Implementation Scenarios

### Scenario 1 — Effective URL selection

**Behavior.** The absent platform URL resolves to the database URL; an
explicit platform URL wins; platform and catalog DSNs carry the operator flag.
Missing or malformed input fails with a stable, redacted error.

**Proof.** `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(/^dsn::tests::/)'`.

### Scenario 2 — One ordinary login on a fresh database

**Behavior.** An ordinary login owning a fresh database migrates and serves;
a tenant connection sees only its tenant; an operator connection sees all
tenants; a superuser or `BYPASSRLS` login is refused at boot.

**Proof.** `pg_migration::pg_tests::single_url_boot_and_separate_url_regression`
through `scripts/postgres/with-test-postgres.sh`.

### Scenario 3 — Two logins

**Behavior.** A separate tenant login that sets `app.operator=on` still sees
only its tenant; a tenant login that owns Wyrd's objects, or a platform login
that does not, is refused.

**Proof.** The same integration test plus the dev-fixture refusal tests.

## Acceptance Criteria

AC-003, AC-007 (insufficient migration privilege), and AC-008 as stated in
spec revision 3.

## Expected Write Set and Consumer Closure

`crates/wyrd/wyrd-sql` (DSNs, migrations, boot checks, tests),
`crates/vala/vala-sql` (migrations, boot, tests), `crates/wyrd/wyrd-server`,
`crates/shared/wyrd-dev-fixtures`, `crates/wyrd/wyrd-testing`, Postgres and
startup scripts, `mise.toml`, architecture authority, and self-hosting docs.
Follow actual consumers; these paths are guidance, not an allowlist.

## Verification and Evidence

Run the exact scenario commands above, then `mise run test:sql`,
`mise run check:tenant-isolation`, `mise run docs:check`, `mise run fmt`,
`mise run lints`, and `git diff --check`.

## Docsite Rebuild Update

After implementation and verification pass, update the developer docsite
rebuild so developers can use what this task delivered. The rebuild lives in
the `wyrd-doc-site` worktree under the `developer-docsite-rebuild` change
packet. Its spec sets the page map, and its tasks set the page rules. Use
`$human-tech-docs`, keep pages `draft: true` with an accurate `status`, and
describe only behavior this task delivered and verified. Keep the development
setup minimal: put production detail in Operate, not Get started. Run every
documented command against the delivered build, then run the docsite's
`docs:check:commands`, `docs:linkcheck`, `docs:build`, and `docs:a11y`.
Record the pages changed and the checks run in Implementation Evidence.

Pages:

- Get started → *Set up Wyrd*: the one-login development database.
- Operate → *Deploy Wyrd for a team*: platform and tenant logins, tenant
  grants, migrate, and readiness.
- Operate → *Troubleshoot a deployment*: database refusals by message.
- Understand → *Identity and authorization*: row-level security isolates
  tenants, and Wyrd's roles are the only permission model.

Status: drafted and verified against the delivered build, together with the
spec revision 4 storage default. Not yet committed in the docsite worktree.

## Material Stop Conditions

Stop for spec revision if pool-scoped RLS cannot keep a separate tenant login
out of other tenants' rows without a Wyrd-provisioned role or grant.

## Authority Links

`changes/active/local-server-bootstrap/spec.md` revision 3;
`AGENTS.md` §§2, 9, 11; `architecture/agent-rules.md`;
`architecture/operations/deployment-and-release.md`;
`architecture/wyrd-security-posture.md`;
`architecture/v1/00-foundations/tenancy.md`.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Absent platform URL falls back; explicit wins; operator/catalog DSNs carry `app.operator=on`; redacted errors | `crates/wyrd/wyrd-sql/src/dsn.rs` (`platform_login`, `platform`, `catalog`, `shares_login`) | `dsn::tests::*` | PASS |
| One ordinary login migrates and serves; tenant sees only its tenant; operator sees all; superuser refused | Migrations (`wyrd.operator_session()`, `operator_access TO CURRENT_USER` on every table and both ledgers, fail-loud `wyrd.current_tenant()`); `schema_check.rs` (`verify_operator_session`, `verify_row_security`, `verify_tenant_pool`); `WyrdPostgres`/`ValaPostgres::validate_schema` | `pg_migration::pg_tests::single_url_boot_and_separate_url_regression`; `wyrd-dev-fixtures` `validate_schema_shared_login_posture` | PASS |
| Separate flagged tenant login sees only its tenant; owner-member, `BYPASSRLS`, or non-owning platform logins refused | `verify_tenant_pool` (separate-login branch), `verify_operator_session` ownership check | Same integration test; `validate_schema_refuses_logins_that_defeat_row_security`; `validate_schema_refuses_drifted_or_unprotected_schemas` | PASS |
| AC-007: insufficient migration privilege names the cause | `SqlError` remediations without role names; `wyrd-server migrate` runs as the platform login | `scripts/server/test-startup-prod.sh` keeps the DBA-credential refusal (not run; see limits) | NOT RUN |
| Wyrd provisions no role or grant | `bootstrap/roles.sql`, `test-roles.sh`, `test:postgres:roles` deleted; no `GRANT`/role check in any migration; harness creates logins as a DBA via `scripts/postgres/test-database-setup.sql` | `test:postgres:contract`; repository grep for `wyrd_app`/`wyrd_platform_admin` is empty outside spec history | PASS |
| Docs state per-login privileges for one and two logins | `self-hosting/database.svx` Logins section; local-dev, docker, configuration, Kubernetes guides; Bifrost privilege sections; architecture authorities; REQ-156 note | `docs:check:commands`, `docs:linkcheck`, `docs:build`, `docs:a11y` | PASS |

**Reuse.** DSN options reuse the existing `options=` suffix; the operator flag is
one SQL predicate; boot checks extend `schema_check.rs`; the harness reuses
`with-test-postgres.sh` and `PgFixture`. No new owner, table, or dependency.

**Removed check.** `test:postgres:roles` (and its nightly step) audited the
attributes and grants of the Wyrd-provisioned `wyrd_app`/`wyrd_platform_admin`
roles from `roles.sql`. Wyrd no longer provisions roles, so the property is
unreachable; the posture it approximated is now enforced at every boot by
`verify_operator_session` and `verify_tenant_pool`.

**Diagnoses.**

1. *Symptom:* migration as the ordinary platform login failed with
   `permission denied to set parameter "app.operator"`. *Evidence:* Postgres
   15+ `validate_option_array_item` rejects a custom placeholder in a function
   `SET` clause for non-superusers; earlier lanes migrated as superuser.
   *Cause:* five `SECURITY DEFINER` lookups used `SET app.operator = on`.
   *Fix site:* those migrations, converted to plpgsql that saves, sets, and
   restores the flag with `set_config(..., true)`; resolver shape test now
   expects volatility `v`.
2. *Symptom:* readiness refused `vala.forge_operation_state`. *Cause:* its
   multi-line `CREATE POLICY` lacked `operator_access`. *Fix site:* that
   migration; a scan of every migration finds no forced-RLS table without
   `operator_access`.
3. *Symptom:* a `BYPASSRLS` tenant login passed readiness. *Evidence:*
   `verify_tenant_pool` called `wyrd.operator_session()`, which errored before
   the attribute check for a login without `wyrd` usage. *Cause:* check order
   depended on schema access. *Fix site:* both login checks read the flag
   inline with `current_setting`.
4. *Symptom:* `vala-sql` `shipped_audit_staging_migration_is_immutable` failed
   in `test:sql` (digest mismatch). *Cause:* the migration was intentionally
   rewritten in place (named grants removed, `operator_access` added); nothing
   has shipped. *Fix site:* the pinned digest, re-pinned; an orphaned grant
   comment was removed first.
5. *Symptom (found in review of callers):* `vala_migrations_apply_and_are_idempotent`
   and the release benchmark harness migrated as the cluster superuser, which
   would make the superuser own Wyrd's objects. *Fix site:* the test and
   `LocalServer::start` migrate with the wrapper's DSNs as the platform login;
   the `owner_url` parameter was removed from its three callers.

**Accepted losses** are recorded in spec D-001: per-table command narrowing
from named-role grants is deliberately not reproduced (the server owns which
commands it issues; Postgres owns tenant isolation), and definer lookups are
executable by every login.

**Commands.**

- `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(/^dsn::tests::/)'`
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_migration::pg_tests::single_url_boot_and_separate_url_regression)'`
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-sql --test integration -E 'test(=pg_migration::pg_tests::vala_migrations_apply_and_are_idempotent) | test(=pg_migration::pg_tests::shipped_audit_staging_migration_is_immutable)'`
- `mise run test:sql`, `mise run test:postgres:contract`,
  `mise run check:tenant-isolation`, `mise run codegen:check`,
  `mise run fmt`, `mise run lints`, `git diff --check`
- `mise run docs:generate`, then `docs:check:commands`, `docs:linkcheck`,
  `docs:build`, `docs:a11y`. The `docs:check` drift step diffs against the
  index, so it passes only once the regenerated `llms*.txt` are committed.

**Limits.** `scripts/server/test-startup-prod.sh` and
`scripts/server/test-kind-autoscale.sh` were updated but not run in this task;
they run at change review. Non-goals (CLI, packages, MCP proxy) untouched.

### Diagnosis 6 — single login named `wyrd` fails migrate validation

- **Symptom:** On a fresh database owned by one ordinary login named `wyrd`,
  `wyrd-server migrate` (with only `WYRD_DATABASE_URL`) refuses with
  `platform.credentials does not force row-level security under exactly its
  operator_access and tenant_isolation policies`, even though the policy is correct.
- **Evidence:** As that login, `SHOW search_path` is `"$user", public`, so
  `pg_get_expr(polqual)` deparses to `operator_session()`. With
  `SET search_path = pg_catalog` it deparses to `wyrd.operator_session()`.
- **Cause:** `verify_row_security` compares deparsed text, and `pg_get_expr`
  drops the qualifier of any schema on the search path. A login named after a
  Wyrd schema puts that schema on the path through `"$user"`. The harness never
  caught this because it only ran the two-login journey with logins `wyrd_platform`
  and `wyrd_tenant`; the dev journey was never exercised.
- **Fix site:** `crates/wyrd/wyrd-sql/src/schema_check.rs::verify_row_security`,
  the single owner of the expression comparison. It now runs in its own
  transaction with `SET LOCAL search_path = pg_catalog`. No other caller
  compares `pg_get_expr` output.
- **Journey coverage:** the startup lane is split into
  `test:server:startup:dev` (`scripts/server/test-startup-dev.sh`: the
  container superuser creates login `wyrd` that owns `wyrd_dev`; the official
  image runs `migrate` and serve with only `WYRD_DATABASE_URL`, then `setup`) and `test:server:startup:prod`
  (`test-startup-prod.sh`, the existing two-login journey), sharing image build
  and Postgres start in `startup-common.sh`. `test:server:startup` runs both.

Verification: a manual dev journey on postgres:16 (fresh DB, login `wyrd`)
passes: migrate, then `readyz` 200, then `setup` prints `admin_credential`.
`mise run test:sql` passes (87 vala, 2 storage, plus the remaining SQL
targets). `clippy -p wyrd-sql --all-features -D warnings` and `fmt` are clean.
`mise run test:server:startup` (both journeys) is **NOT RUN to completion**: on this macOS host
the lane copies a host-built Mach-O `wyrd-server` into the Linux image
(`exec format error`). The lane needs a Linux host.

### Owner-directed change — default local storage (spec revision 4)

The owner changed REQ-007: an unset `WYRD_STORAGE_URL` means a local server, so
storage is `.wyrd/storage` under the working directory, created if missing.
`crates/wyrd/wyrd-storage/src/settings.rs::from_env` is the single owner;
`default_local_root` creates and returns the absolute root. A set
`file://` URL still requires an existing root. `env_parse::env_required` lost
its last caller and is deleted.

| Check | Result |
| --- | --- |
| `mise exec -- cargo nextest run --locked -p wyrd-storage --lib -E 'test(=settings::tests::unset_storage_url_creates_default_local_root)'` (with the other 7 `settings::` tests) | PASS |
| `cargo clippy --locked -p wyrd-storage --all-features --all-targets -D warnings`, `cargo fmt` | PASS |
| Real dev journey on postgres:16, only `WYRD_DATABASE_URL` set (plus macOS memory): migrate, `readyz` 200, `setup` prints `admin_credential`, `.wyrd/storage` created | PASS |
