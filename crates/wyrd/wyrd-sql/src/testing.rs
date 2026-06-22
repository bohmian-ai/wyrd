//! Test-tier migration helper for wyrd-sql.
//!
//! Applies embedded wyrd-sql migrations against an [`sqlx::test`] pool —
//! including the role bootstrap that production infra handles outside of
//! migrations. Call this before vala-sql or any crate that depends on the
//! platform and wyrd schemas being in place.

use sqlx::PgConnection;

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
