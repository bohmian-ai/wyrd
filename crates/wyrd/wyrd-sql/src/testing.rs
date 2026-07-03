//! Test-tier migration helper for wyrd-sql.
//!
//! Applies embedded wyrd-sql migrations against an [`sqlx::test`] pool —
//! including the role bootstrap that production infra handles outside of
//! migrations. Call this before vala-sql or any crate that depends on the
//! platform and wyrd schemas being in place.

use secrecy::ExposeSecret;
use sqlx::{AssertSqlSafe, PgConnection, PgPool};

use crate::error::SqlError;
use crate::pool::{PoolConfig, build_pool};

/// Shared pool set for live-Postgres tests against the docker `wyrd_test` DB.
pub struct SharedDb {
    /// Superuser migrator pool. Use for migration and reset only.
    pub migrator: PgPool,
    /// RLS-enforced app pool. Test bodies use this pool.
    pub app: PgPool,
    /// BYPASSRLS platform-admin pool. Use for tenant seeding.
    pub platform_admin: PgPool,
}

/// Connect to the shared docker `wyrd_test` DB and return role-specific pools.
///
/// # Errors
/// Returns [`SqlError`] when the shared test DB env is missing or invalid,
/// migrations fail, or any role pool cannot be built.
pub async fn shared() -> Result<SharedDb, SqlError> {
    let dsns = resolved_test_dsns()?;
    let postgres = crate::WyrdPostgres::connect_from_dsns(&dsns).await?;

    let migrator = build_pool(
        dsns.migrator.expose_secret(),
        PoolConfig::migrator_defaults(),
    )
    .await
    .map_err(SqlError::Connect)?;
    let platform_admin = postgres
        .platform_admin_pool()
        .cloned()
        .ok_or_else(|| SqlError::InvariantViolation {
            detail: "shared test DB env unset (WYRD_DATABASE_PLATFORM_ADMIN_PASSWORD); platform-admin pool is required for tenant seeding".to_owned(),
        })?;

    Ok(SharedDb {
        migrator,
        app: postgres.app_pool().clone(),
        platform_admin,
    })
}

fn resolved_test_dsns() -> Result<crate::dsn::ResolvedDsns, SqlError> {
    let resolved = crate::dsn::resolve_external_dsns_from_env().map_err(|error| {
        SqlError::InvariantViolation {
            detail: format!("test DB DSN config error: {error}"),
        }
    })?;
    let Some(dsns) = resolved else {
        return Err(SqlError::InvariantViolation {
            detail: "shared test DB env unset (WYRD_DATABASE_URL + WYRD_DATABASE_MIGRATOR_PASSWORD); refusing to boot embedded in tests".to_owned(),
        });
    };
    Ok(dsns)
}

impl SharedDb {
    /// Clean-slate reset of Wyrd-owned schemas. Call at the start of each test.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the reset connection or truncate fails.
    pub async fn reset(&self) -> Result<(), SqlError> {
        let mut conn = self.migrator.acquire().await.map_err(SqlError::Connect)?;
        reset_owned_schemas(&mut conn, crate::OWNED_SCHEMAS).await
    }
}

const TEST_ROLE_BOOTSTRAP_SQL: &str = r#"
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_migrator') THEN
        CREATE ROLE wyrd_migrator LOGIN BYPASSRLS;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_app') THEN
        CREATE ROLE wyrd_app LOGIN;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_platform_admin') THEN
        CREATE ROLE wyrd_platform_admin LOGIN BYPASSRLS;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_catalog') THEN
        CREATE ROLE wyrd_catalog NOLOGIN;
    END IF;
END $$;
"#;

/// Apply wyrd-sql prerequisites against a single `#[sqlx::test]` connection.
///
/// Creates the cluster roles the platform migration asserts, then runs the
/// embedded wyrd-sql migrations (platform + wyrd schemas, `platform.tenants`,
/// `wyrd.current_tenant()`). Call this first in any test helper that chains
/// further migrations on the same connection.
///
/// # Errors
/// Returns [`SqlError`] when role creation or migration execution fails.
pub async fn migrate_for_test(conn: &mut PgConnection) -> Result<(), SqlError> {
    sqlx::query(TEST_ROLE_BOOTSTRAP_SQL)
        .execute(&mut *conn)
        .await
        .map_err(SqlError::Connect)?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS platform")
        .execute(&mut *conn)
        .await
        .map_err(SqlError::Connect)?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS wyrd")
        .execute(&mut *conn)
        .await
        .map_err(SqlError::Connect)?;
    sqlx::query("SET search_path TO wyrd, platform, public")
        .execute(&mut *conn)
        .await
        .map_err(SqlError::Connect)?;
    sqlx::migrate!("./migrations")
        .run(&mut *conn)
        .await
        .map_err(SqlError::from)
}

/// Clean-slate reset of every data table in the passed owned schemas.
///
/// Discovers the ordinary and partitioned tables in `owned_schemas` from
/// `pg_catalog` — excluding the `_sqlx_migrations` ledger so the migrate-once
/// contract holds — and empties them with a single
/// `TRUNCATE … RESTART IDENTITY CASCADE`. `CASCADE` owns foreign-key order, so
/// the statement is order-independent and stays correct as tables are added.
///
/// `conn` must be a superuser (the docker-compose bootstrap `wyrd_migrator`) or
/// a table owner: audit tables are append-only via `REVOKE DELETE` plus block
/// triggers, and runtime `iceberg_catalog` tables are `wyrd_catalog`-owned, so
/// only superuser `TRUNCATE` bypasses both privilege checks and ownership.
/// Discovery from `pg_catalog` is load-bearing precisely because those runtime
/// tables have no compile-time list.
///
/// Standalone sequences (e.g. `vala.writer_fencing_seq`) are **not** reset by
/// `RESTART IDENTITY`, which touches only identity/serial columns.
///
/// # Errors
/// Returns [`SqlError::Query`] when table discovery or the truncate fails.
pub async fn reset_owned_schemas(
    conn: &mut PgConnection,
    owned_schemas: &[&str],
) -> Result<(), SqlError> {
    let schema_list: Vec<String> = owned_schemas
        .iter()
        .map(|schema| (*schema).to_owned())
        .collect();

    let table_list: Option<String> = sqlx::query_scalar(
        "SELECT string_agg(format('%I.%I', n.nspname, c.relname), ', ')
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname::text = ANY($1)
           AND c.relkind IN ('r', 'p')
           AND NOT c.relispartition
           AND c.relname <> '_sqlx_migrations'",
    )
    .bind(&schema_list)
    .fetch_one(&mut *conn)
    .await
    .map_err(SqlError::from)?;

    let Some(table_list) = table_list else {
        return Ok(());
    };

    sqlx::query(AssertSqlSafe(format!(
        "TRUNCATE TABLE {table_list} RESTART IDENTITY CASCADE"
    )))
    .execute(&mut *conn)
    .await
    .map_err(SqlError::from)?;

    Ok(())
}
