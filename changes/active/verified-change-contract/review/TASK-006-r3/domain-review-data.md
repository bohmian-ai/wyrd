# Data and storage domain review

## Immutable subject and boundary

Repository `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`; TASK-006 cumulative base `f8811ac5035c3aa165d34c38992f9889b3c9081f`; R2 remediation base `f500ea38bc749f36b3ee8d88893dcf7c0161435c`; candidate `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4` (HEAD during review). Reviewed external Postgres bootstrap, one-off migration, serving readiness, role and grant isolation, durable recovery, and Forge object-listing/scheduler effects. The candidate source was not edited.

## Authority and source coverage

| Boundary | Authority | Source and verification inspected | Result |
|---|---|---|---|
| Two serving roles and owner-only migration | Approved spec rev 40 REQ-154/156/157/158, AC-035; R2 remediation; `AGENTS.md` §§2, 9; `architecture/v1/00-foundations/{postgres-layout,sql-foundation}.md` | `roles.sql`, both schema migrations, `ResolvedDsns`, `main::migrate`, `ServerPostgres::connect_from_dsns`, SQL migration tests and startup image script | FAIL: readiness accepts incorrect roles and grants |
| Tenant RLS and catalog separation | REQ-156/157, AC-035; `architecture/v1/00-foundations/tenancy.md`, `sql-foundation.md`, `architecture/agent-rules.md` | `SchemaCheck`, `WyrdPostgres::validate_schema`, `ValaPostgres::validate_schema`, `TenantConn`, `OperatorPool`, `test-startup.sh` | FAIL: forced flag alone does not prove policy or complete privilege posture |
| Migration failure and accepted-data recovery | REQ-158, AC-035; `architecture/operations/deployment-and-release.md` | Ordered `wyrd_sql::migrate` and `vala_sql::migrate`, owner-only CLI, `test-startup.sh` injected failure/retry and restart journey, database docs | PASS within tested recoverable failure |
| Local durable and peer shared object storage | REQ-154/159/162, AC-034/036; `architecture/bifrost-design.md` durability and maintenance | Storage settings/handle/service, peer-mode local-backend rejection, `OpenDalForgeObjectStore` native and emulated listing, Forge worker and listing tests | PASS |
| Forge scheduler restart | AC-034 restart; Bifrost maintenance authority | Stable `node_id` to `scheduler_owner`, `ForgeScheduler::new`, startup image restart and Forge integration evidence | PASS |

## Material proposed findings

### DATA-R3-1 — INCORRECT: serving accepts the wrong PostgreSQL login identities

- **Obligation:** REQ-156 and AC-035 require serving to use exactly `wyrd_app` and `wyrd_platform_admin`, with the owner confined to the migration process; REQ-157 requires a misprovisioned database to remain unready.
- **Location:** `crates/wyrd/wyrd-sql/src/schema_check.rs:82-95`, called by `crates/wyrd/wyrd-sql/src/postgres.rs:114-120` and `crates/vala/vala-sql/src/postgres.rs:100-106`; `crates/wyrd/wyrd-sql/src/dsn.rs:46-58` accepts any parseable URL.
- **Evidence and reachable consequence:** `SchemaCheck::login` fetches `rolname` but checks only `rolsuper` and `rolbypassrls`. A non-superuser, non-bypass login other than `wyrd_app` with the necessary inherited/table privileges passes the app check; a BYPASSRLS login other than `wyrd_platform_admin` passes the platform check. Both serving pools then run under identities outside the approved role boundary. This also lets a misrouted owner-equivalent, non-superuser BYPASSRLS login reach serving if supplied as the platform DSN. The startup script tests only the happy-path named DSNs and owner-secret absence from that one container.
- **Testable correction:** At the existing `SchemaCheck::login` boundary, require the exact expected role name and its permitted role attributes for each pool. Exercise a wrong app login and a wrong platform login against migrated Postgres and assert startup refuses readiness.

### DATA-R3-2 — INCORRECT: security-grant-deficient schemas pass the migration and serving gates

- **Obligation:** REQ-157 and AC-035 require effective RLS and required grants before readiness; REQ-156 bars app catalog access and unrestricted platform DDL.
- **Location:** `crates/wyrd/wyrd-sql/src/schema_check.rs:99-124`, `crates/wyrd/wyrd-sql/src/postgres.rs:107-120`, `crates/vala/vala-sql/src/postgres.rs:100-117`, and `crates/wyrd/wyrd-server/src/main.rs:203-215`.
- **Evidence and reachable consequence:** The shared check tests only `relrowsecurity` and `relforcerowsecurity`; it never reads `pg_policy` or effective schema/table/function grants. Vala checks only whether the app can use `iceberg_catalog`, not other required or forbidden privileges. Wyrd checks no grants at all. After a successful migration, `GRANT CREATE ON SCHEMA wyrd TO wyrd_app` leaves ledgers and forced-RLS flags unchanged and passes both readiness paths, yet gives the tenant request login DDL power. Likewise, removing a tenant-isolation policy leaves forced RLS set and boot ready while legitimate tenant reads/writes fail. `test-startup.sh:113-120` tests checksum and `NO FORCE` only; it does not mutate grants or policy. The one-off `migrate` command prints a valid result under those post-migration states as well.
- **Testable correction:** Extend the existing SQL-owner readiness check to validate the task-required effective role grants and tenant isolation policies, and use that check in both one-off migration and serve boot. A focused Postgres test should mutate a required grant or policy after migration and show migration validation and serving readiness refuse it; a positive migrated database must still start.

## Verification limits

This is source and diff inspection. The supplied results report `test:server:startup`, `test:sql`, `test:storage:matrix`, the server/Oracle/Python/TypeScript journeys, fmt and lints passing; I did not rerun those lanes. Their current negative cases do not exercise the two misprovisioned states above. I did not inject a live crash or audit-history restore; the approved retry path and repository evidence cover recoverable migration failure, not arbitrary backup restoration.

## Overall result

**FAIL** — DATA-R3-1 and DATA-R3-2 leave the approved PostgreSQL role and security-readiness obligations unproven and allow concrete misprovisioned serving states.
