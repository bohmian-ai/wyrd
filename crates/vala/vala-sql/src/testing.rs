//! Test-tier migration helpers for vala-sql.
//!
//! These helpers reproduce the production boot order (wyrd-sql schemas first,
//! then vala-sql migrations) against an [`sqlx::test`] pool. A bare
//! `sqlx::migrate!()` invocation against an empty pool fails because the vala
//! migrations depend on `platform.tenants` and `wyrd.current_tenant()` from
//! wyrd-sql — never re-transcribed here.

use secrecy::SecretString;
use sqlx::{AssertSqlSafe, PgConnection, PgPool};

use crate::SqlError;

fn resolved_test_dsns() -> Result<wyrd_sql::dsn::ResolvedDsns, SqlError> {
    wyrd_sql::dsn::resolve_external_dsns_from_env()
        .map_err(|error| SqlError::InvariantViolation {
            detail: format!("test DB DSN config error: {error}"),
        })
        .and_then(|resolved| {
            resolved.ok_or_else(|| SqlError::InvariantViolation {
                detail: "shared test DB env unset (WYRD_DATABASE_URL + WYRD_DATABASE_MIGRATOR_PASSWORD); refusing to boot embedded in tests".to_owned(),
            })
        })
}

/// Shared pool set with Wyrd and Vala migrations guaranteed applied once.
///
/// # Errors
/// Returns [`SqlError`] when the shared Wyrd DB cannot be prepared or Vala
/// migrations fail.
pub async fn shared() -> Result<wyrd_sql::testing::SharedDb, SqlError> {
    Ok(shared_handles().await?.db)
}

/// Shared handles with Wyrd and Vala migrations guaranteed applied once.
/// Additive companion to `shared()` — returns the real `ValaPostgres` instead
/// of dropping it. Only the fixture (`wyrd-dev-fixtures`) needs both handles.
pub struct SharedVala {
    /// Wyrd control-plane shared DB (migrations applied).
    pub db: wyrd_sql::testing::SharedDb,
    /// Real Vala warehouse handle (migrations applied).
    pub vala: crate::ValaPostgres,
}

/// # Errors
/// Returns [`SqlError`] when the shared Wyrd DB cannot be prepared or Vala
/// migrations fail.
pub async fn shared_handles() -> Result<SharedVala, SqlError> {
    let db = wyrd_sql::testing::shared().await?;
    let dsns = resolved_test_dsns()?;
    let vala = crate::ValaPostgres::connect_after_wyrd(&dsns).await?;
    Ok(SharedVala { db, vala })
}

/// Connect to the shared test DB as `vala_recovery`.
///
/// # Errors
/// Returns [`SqlError`] when shared DB env is missing or the pool cannot connect.
pub async fn recovery_pool() -> Result<PgPool, SqlError> {
    let dsns = resolved_test_dsns()?;
    let password = std::env::var(crate::postgres::VALA_RECOVERY_PASSWORD_ENV).map_err(|_| {
        SqlError::InvariantViolation {
            detail: "VALA_RECOVERY_PASSWORD must be set to connect recovery pool in tests"
                .to_owned(),
        }
    })?;
    crate::postgres::connect_recovery_pool(&dsns, SecretString::from(password)).await
}

/// Reset Wyrd and Vala owned schemas, then restore migration-seeded sentinel data.
///
/// # Errors
/// Returns [`SqlError`] when reset or sentinel restore fails.
pub async fn reset_for_test(db: &wyrd_sql::testing::SharedDb) -> Result<(), SqlError> {
    let schemas: Vec<&str> = wyrd_sql::OWNED_SCHEMAS
        .iter()
        .chain(crate::OWNED_SCHEMAS.iter())
        .copied()
        .collect();
    let mut conn = db.migrator.acquire().await.map_err(SqlError::Connect)?;
    wyrd_sql::testing::reset_owned_schemas(&mut conn, &schemas).await?;
    sqlx::query("ALTER SEQUENCE vala.writer_fencing_seq RESTART WITH 1")
        .execute(&mut *conn)
        .await
        .map_err(SqlError::from)?;
    sqlx::query("ALTER SEQUENCE vala.recovery_fencing_seq RESTART WITH 1")
        .execute(&mut *conn)
        .await
        .map_err(SqlError::from)?;
    seed_system_owner(&mut conn).await
}

async fn seed_system_owner(conn: &mut PgConnection) -> Result<(), SqlError> {
    sqlx::query(
        "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status)
         VALUES ('00000000-0000-0000-0000-000000000000', 'wyrd-system-owner', 'Wyrd System Owner', 'active')
         ON CONFLICT (data_tenant_id) DO NOTHING",
    )
    .execute(&mut *conn)
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

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
    wyrd_sql::testing::migrate_for_test(&mut conn).await?;
    sqlx::query(
        "DO $$ BEGIN
           IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vala_recovery_owner') THEN
               CREATE ROLE vala_recovery_owner NOLOGIN BYPASSRLS;
           END IF;
           IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vala_recovery') THEN
               CREATE ROLE vala_recovery NOLOGIN;
           END IF;
           GRANT vala_recovery_owner TO current_user;
         END $$",
    )
    .execute(&mut *conn)
    .await
    .map_err(SqlError::Connect)?;
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
