//! Test-tier migration helper for wyrd-sql.
//!
//! Applies embedded wyrd-sql migrations against an [`sqlx::test`] pool —
//! including the role bootstrap that production infra handles outside of
//! migrations. Call this before vala-sql or any crate that depends on the
//! platform and wyrd schemas being in place.

use sqlx::{AssertSqlSafe, PgConnection};

use crate::error::SqlError;

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
