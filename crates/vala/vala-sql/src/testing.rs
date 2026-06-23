//! Test-tier migration helpers for vala-sql.
//!
//! These helpers reproduce the production boot order (wyrd-sql schemas first,
//! then vala-sql migrations) against an [`sqlx::test`] pool. A bare
//! `sqlx::migrate!()` invocation against an empty pool fails because the vala
//! migrations depend on `platform.tenants` and `wyrd.current_tenant()` from
//! wyrd-sql — never re-transcribed here.

use sqlx::{AssertSqlSafe, PgPool};

use crate::SqlError;

/// Apply wyrd-sql prerequisites then vala-sql migrations against a test pool.
///
/// Reproduces the production boot order on a single connection: wyrd-sql
/// schemas and migrations first (platform + wyrd schemas, `platform.tenants`,
/// `wyrd.current_tenant()`), then the owned-schema pre-creates and vala
/// migrations. Call this in-body inside `#[sqlx::test]` (no `migrator =`
/// arg).
///
/// # Errors
/// Returns [`SqlError`] when any step fails.
pub async fn migrate_for_test(pool: &PgPool) -> Result<(), SqlError> {
    let mut conn = pool.acquire().await.map_err(SqlError::Connect)?;
    wyrd_sql::testing::migrate_for_test(&mut conn)
        .await?;
    for schema in crate::OWNED_SCHEMAS {
        sqlx::query(AssertSqlSafe(format!(
            "CREATE SCHEMA IF NOT EXISTS {schema}"
        )))
        .execute(&mut *conn)
        .await
        .map_err(SqlError::Connect)?;
    }
    sqlx::query(AssertSqlSafe(format!(
        "SET search_path TO {}",
        crate::MIGRATION_SEARCH_PATH
    )))
    .execute(&mut *conn)
    .await
    .map_err(SqlError::Connect)?;
    sqlx::migrate!("./migrations")
        .run(&mut *conn)
        .await
        .map_err(SqlError::from)
}

/// Seed a test tenant into `platform.tenants` so tenant-owned writes resolve.
///
/// New-v7 tenants are never seeded by the migration; this helper inserts the
/// FK anchor idempotently. Call before any TenantOwned write in a test body.
///
/// # Errors
/// Returns [`SqlError`] when the insert fails.
pub async fn seed_tenant(pool: &PgPool, tenant: sqlx::types::Uuid) -> Result<(), SqlError> {
    sqlx::query(
        "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status)
         VALUES ($1, $2, $2, 'active') ON CONFLICT (data_tenant_id) DO NOTHING",
    )
    .bind(tenant)
    .bind(format!("test-{}", tenant.simple()))
    .execute(pool)
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Derive the catalog connection URI for a test pool.
///
/// The iceberg-catalog-sql crate builds its own pool from this URI (it does
/// not accept a shared pool), so the round-trip test needs the ephemeral
/// database URL with `search_path=iceberg_catalog` baked in. The superuser
/// connects directly; `role=wyrd_catalog` mirrors prod ownership so the
/// tables the crate creates are owned by `wyrd_catalog`, matching the
/// `ALTER DEFAULT PRIVILEGES FOR ROLE wyrd_catalog` boundary in the migration.
pub fn catalog_uri(pool: &PgPool) -> String {
    use sqlx::ConnectOptions as _;
    let base = pool.connect_options().to_url_lossy();
    let sep = if base.query().is_some() { "&" } else { "?" };
    format!("{base}{sep}options=-c%20role%3Dwyrd_catalog%20-c%20search_path%3Diceberg_catalog")
}
