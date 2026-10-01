# Data-domain review — TASK-006 R4

**Subject:** `f8811ac5035c3aa165d34c38992f9889b3c9081f..58cabb529b93da366959db796ac3b596a6c6c1e6` (R3 increment from `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4`). Candidate HEAD was unchanged at inspection. **Result: FAIL.**

## Boundary and authority coverage

| Boundary | Source and callers traced | Authority |
|---|---|---|
| One-off migration and lease | `wyrd-server/src/main.rs::migrate` → `MigrationLease::acquire/apply/release` → both SQL crate migrators → both `verify_schema` calls; pool defaults and lease integration test | Spec rev 41 REQ-157/158, AC-035; R3 FIND-17; `architecture/operations/deployment-and-release.md` migration contract; AGENTS.md §§3, 9, 11 |
| Serving logins and readiness | `ServerPostgres::connect_from_dsns` → `WyrdPostgres::validate_schema`, `ValaPostgres::validate_schema` → `OperatorPool` schema checks; serving DSN resolution, role bootstrap, migrations and fixture tests | Spec REQ-156/157, AC-035; R3 FIND-18/19/20; `architecture/agent-rules.md` raw-pool, RLS and cross-tier rules; AGENTS.md §§3, 9 |
| Tenant data and grants | Wyrd/Vala SQL migrations, `OperatorPool::verify_tenant_isolation/verify_schema_privileges`, readiness and startup mutation tests | Spec REQ-156/157/158, AC-035; `architecture/wyrd-security-posture.md`; `architecture/operations/deployment-and-release.md` |

FIND-17 is closed: one fixed-key advisory lock remains held on the direct owner session across both migrators and post-validation; lock waiting is bounded, and the competing-migrator test exercises release and retry. FIND-18 is closed: both session and effective login names are checked, and role attributes and membership are checked. FIND-20 is closed: serving checks use `OperatorPool` and the existing SQL owners; the raw owner session is confined to the one-off migration boundary.

## Material proposed finding

### DATA-R4-1 — INCORRECT — altered tenant policy passes readiness (FIND-19 remains open)

**Obligation:** Spec REQ-157 requires RLS and required-grant verification before readiness; AC-035 requires a security-deficient database to stay unready. R3 FIND-19 specifically requires checking the tenant-isolation policies established by migrations.

**Exact location:** `crates/wyrd/wyrd-sql/src/schema_check.rs`, `OperatorPool::verify_tenant_isolation`, particularly the `pg_get_expr(p.polqual, p.polrelid) ~ '^\\([a-z_]+ = wyrd\\.current_tenant\\(\\)\\)$'` predicate. Production callers are `wyrd_sql::verify_schema` and `vala_sql::verify_schema`, reached by both `wyrd-server migrate` post-validation and `ServerPostgres::connect_from_dsns` serving startup.

**Evidence and consequence:** The predicate accepts **any** lower-case column compared with `wyrd.current_tenant()`, although the migrated policy compares `data_tenant_id`. For example, `wyrd.auth_users` has both `id UUID` and `data_tenant_id UUID`; an owner can run `ALTER POLICY tenant_isolation ON wyrd.auth_users USING (id = wyrd.current_tenant()) WITH CHECK (id = wyrd.current_tenant())`. The altered policy keeps the required name, permissive command, PUBLIC role, forced-RLS flags, and matching USING/WITH CHECK expressions. Its deparsed expression matches the readiness regex, while the policy no longer isolates by `data_tenant_id`; ordinary tenant auth rows become inaccessible or rows with an `id` equal to another tenant's ID can be exposed. Neither the fixture policy-removal test nor the startup policy-rename test covers this altered-policy case. Thus owner migration validation and serving startup can both report ready for policy drift that the task expressly requires them to reject. This is one root-cause gap in the shared SQL check, not a request for a generic schema linter.

**Testable correction:** Make the existing `OperatorPool::verify_tenant_isolation` require the policy expression to compare **`data_tenant_id`** to `wyrd.current_tenant()` for both USING and WITH CHECK; preserve the existing role, permissive-policy, and forced-RLS checks. In the existing Postgres schema-readiness mutation test, alter a policy to compare another UUID column and prove both post-migration validation and serving validation refuse; restore the policy and prove readiness and tenant work still pass. No new abstraction is needed.

## Verification limits

Recorded R3 evidence includes passing `test:sql`, `test:server:startup`, `check:from-pools-allowlist`, fmt and lints, with exact focused commands for the named SQL tests. I inspected source and recorded evidence but did not rerun a live Postgres or startup lane. The green mutation tests cover a removed policy, an extra permissive policy, and widened schema/TRUNCATE grants; they do not exercise a same-name, wrong-column policy. No unrelated data or migration change is proposed.
