//! Embedded Postgres fixtures for SQL integration tests.

use std::path::Path;

use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;
use tempfile::TempDir;
use wyrd_spec::DataTenantId;
use wyrd_sql::postgres_boot::{BootError, EmbeddedConfig, PostgresBoot};
use wyrd_sql::{PoolConfig, SqlError, TenantConn};

/// Per-test embedded Postgres fixture with Wyrd and Vala migrations applied.
pub struct PgFixture {
    app_pool: PgPool,
    platform_admin_pool: PgPool,
    boot: PostgresBoot,
    data_tenant_id: DataTenantId,
    tenant_slug: String,
    tempdir: TempDir,
}

/// Errors returned while starting an embedded Postgres fixture.
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    /// Temporary data directory creation failed.
    #[error("tempdir create failed: {0}")]
    TempDir(#[from] std::io::Error),
    /// Embedded Postgres boot failed.
    #[error("postgres boot failed: {0}")]
    Boot(#[from] BootError),
    /// Embedded boot did not return a platform-admin DSN.
    #[error("embedded Postgres boot did not return a platform-admin DSN")]
    MissingPlatformAdminDsn,
    /// SQL layer failed.
    #[error("sql layer failed: {0}")]
    Sql(#[from] SqlError),
}

impl PgFixture {
    /// Start embedded Postgres, run Wyrd and Vala migrations, and seed one
    /// deterministic test tenant row.
    ///
    /// # Errors
    /// Returns [`FixtureError`] when tempdir creation, embedded Postgres boot,
    /// pool construction, migration, or seed insert fails.
    pub async fn start() -> Result<Self, FixtureError> {
        let data_tenant_id = DataTenantId::new_v7();
        let tenant_slug = "test-tenant-1".to_owned();

        Self::start_seeded(data_tenant_id, tenant_slug).await
    }

    /// Start a fixture with a caller-supplied tenant slug.
    ///
    /// The tenant isolation key is still generated as a UUIDv7
    /// [`DataTenantId`].
    ///
    /// # Errors
    /// Returns [`FixtureError`] when fixture startup or tenant seeding fails.
    pub async fn start_with_slug(slug: impl Into<String>) -> Result<Self, FixtureError> {
        Self::start_seeded(DataTenantId::new_v7(), slug.into()).await
    }

    /// Open a tenant-scoped transaction bound to the seeded tenant.
    ///
    /// # Errors
    /// Returns [`SqlError`] when acquiring or binding the transaction fails.
    pub async fn tenant_conn(&self) -> Result<TenantConn<'_>, SqlError> {
        self.tenant_conn_for(self.data_tenant_id).await
    }

    /// Open a tenant-scoped transaction bound to a caller-supplied tenant.
    ///
    /// # Errors
    /// Returns [`SqlError`] when acquiring or binding the transaction fails.
    pub async fn tenant_conn_for(
        &self,
        data_tenant_id: DataTenantId,
    ) -> Result<TenantConn<'_>, SqlError> {
        TenantConn::acquire(&self.app_pool, data_tenant_id).await
    }

    /// Borrow the runtime `wyrd_app` pool.
    #[must_use]
    pub fn app_pool(&self) -> &PgPool {
        &self.app_pool
    }

    /// Borrow the audited `wyrd_platform_admin` pool.
    #[must_use]
    pub fn platform_admin_pool(&self) -> &PgPool {
        &self.platform_admin_pool
    }

    /// Resolve the Bifrost catalog DSN for this fixture's embedded Postgres.
    ///
    /// The DSN connects as `wyrd_catalog_app` with the `role=wyrd_catalog` +
    /// `search_path=iceberg_catalog` options baked in — the prod catalog
    /// identity — so a test can construct the same `WyrdCatalog` the server boot
    /// path does. Only the embedded boot surfaces this; the roles it needs are
    /// provisioned during `start_seeded`.
    ///
    /// # Errors
    /// Returns [`FixtureError`] when DSN resolution fails.
    pub fn catalog_dsn(&self) -> Result<SecretString, FixtureError> {
        Ok(self.boot.dsns()?.catalog_app)
    }

    /// Return the fixture's tenant isolation key.
    #[must_use]
    pub fn data_tenant_id(&self) -> DataTenantId {
        self.data_tenant_id
    }

    /// Return the fixture's seeded tenant slug.
    #[must_use]
    pub fn tenant_slug(&self) -> &str {
        &self.tenant_slug
    }

    /// Return the embedded Postgres port.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        match &self.boot {
            PostgresBoot::Embedded(handle) => Some(handle.port()),
            PostgresBoot::External { .. } => None,
        }
    }

    /// Return the root temporary directory that owns this fixture's data.
    #[must_use]
    pub fn tempdir_path(&self) -> &Path {
        self.tempdir.path()
    }

    async fn start_seeded(
        data_tenant_id: DataTenantId,
        tenant_slug: String,
    ) -> Result<Self, FixtureError> {
        let tempdir = tempfile::tempdir()?;
        let boot = PostgresBoot::embedded(EmbeddedConfig {
            data_dir: tempdir.path().join("pg"),
            port: 0,
            ..EmbeddedConfig::default()
        })
        .await?;
        let dsns = boot.dsns()?;

        let migrator_pool = wyrd_sql::pool::build_pool(
            dsns.migrator.expose_secret(),
            PoolConfig::migrator_defaults(),
        )
        .await
        .map_err(SqlError::Connect)?;
        let migrate_result = async {
            wyrd_sql::migrate(&migrator_pool).await?;
            vala_sql::migrate(&migrator_pool).await
        }
        .await;
        migrator_pool.close().await;
        migrate_result?;

        let app_pool =
            wyrd_sql::pool::build_pool(dsns.app.expose_secret(), PoolConfig::app_defaults())
                .await
                .map_err(SqlError::Connect)?;
        let platform_admin_dsn = dsns
            .platform_admin
            .ok_or(FixtureError::MissingPlatformAdminDsn)?;
        let platform_admin_pool = wyrd_sql::pool::build_pool(
            platform_admin_dsn.expose_secret(),
            PoolConfig::platform_admin_defaults(),
        )
        .await
        .map_err(SqlError::Connect)?;

        seed_tenant(&platform_admin_pool, data_tenant_id, &tenant_slug).await?;

        Ok(Self {
            app_pool,
            platform_admin_pool,
            boot,
            data_tenant_id,
            tenant_slug,
            tempdir,
        })
    }
}

async fn seed_tenant(
    platform_admin_pool: &PgPool,
    data_tenant_id: DataTenantId,
    tenant_slug: &str,
) -> Result<(), SqlError> {
    sqlx::query(
        "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status)
         VALUES ($1, $2, $3, 'active')",
    )
    .bind(data_tenant_id.as_uuid())
    .bind(tenant_slug)
    .bind("Test Tenant")
    .execute(platform_admin_pool)
    .await
    .map_err(SqlError::from)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;

    use super::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_sql::tenant_conn::CURRENT_TENANT_GUC;

    #[tokio::test]
    async fn fixture_smoke() {
        let fixture = PgFixture::start().await.expect("fixture starts");

        assert_required_schemas(&fixture).await;
        assert_required_roles(&fixture).await;
        assert_single_tenant(&fixture).await;
        assert_seeded_tenant(&fixture, fixture.tenant_slug()).await;
        assert_current_tenant_bound(&fixture).await;
        assert_bare_app_pool_fails_loudly(&fixture).await;
    }

    #[tokio::test]
    async fn start_with_slug_uses_custom_slug() {
        let fixture = PgFixture::start_with_slug("custom-tenant")
            .await
            .expect("fixture starts with custom slug");

        assert_eq!(fixture.tenant_slug(), "custom-tenant");
        assert_seeded_tenant(&fixture, "custom-tenant").await;
    }

    #[tokio::test]
    async fn ten_concurrent_fixtures_are_isolated() {
        let fixtures = futures_for_concurrent_start().await;

        let ports = fixtures
            .iter()
            .map(|fixture| fixture.port().expect("embedded port is available"))
            .collect::<HashSet<_>>();
        let tenant_ids = fixtures
            .iter()
            .map(PgFixture::data_tenant_id)
            .collect::<HashSet<_>>();
        let tempdirs = fixtures
            .iter()
            .map(|fixture| fixture.tempdir_path().to_path_buf())
            .collect::<HashSet<_>>();

        assert_eq!(ports.len(), fixtures.len());
        assert_eq!(tenant_ids.len(), fixtures.len());
        assert_eq!(tempdirs.len(), fixtures.len());
    }

    #[tokio::test]
    async fn data_dir_is_removed_after_drop() {
        let tempdir_path = {
            let fixture = PgFixture::start().await.expect("fixture starts");
            fixture.tempdir_path().to_path_buf()
        };

        assert_path_removed(tempdir_path);
    }

    async fn futures_for_concurrent_start() -> Vec<PgFixture> {
        let (a, b, c, d, e, f, g, h, i, j) = tokio::join!(
            PgFixture::start(),
            PgFixture::start(),
            PgFixture::start(),
            PgFixture::start(),
            PgFixture::start(),
            PgFixture::start(),
            PgFixture::start(),
            PgFixture::start(),
            PgFixture::start(),
            PgFixture::start(),
        );

        [a, b, c, d, e, f, g, h, i, j]
            .into_iter()
            .map(|result| result.expect("concurrent fixture starts"))
            .collect()
    }

    async fn assert_required_schemas(fixture: &PgFixture) {
        for schema in ["platform", "wyrd", "vala"] {
            let exists: (bool,) =
                sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)")
                    .bind(schema)
                    .fetch_one(fixture.platform_admin_pool())
                    .await
                    .expect("schema query succeeds");
            assert!(exists.0, "{schema} schema exists");
        }
    }

    async fn assert_required_roles(fixture: &PgFixture) {
        let rows: Vec<(String, bool)> = sqlx::query_as(
            "SELECT rolname, rolbypassrls
             FROM pg_roles
             WHERE rolname IN ('wyrd_app', 'wyrd_migrator', 'wyrd_platform_admin')
             ORDER BY rolname",
        )
        .fetch_all(fixture.platform_admin_pool())
        .await
        .expect("role metadata query succeeds");

        assert_eq!(
            rows,
            vec![
                ("wyrd_app".to_owned(), false),
                ("wyrd_migrator".to_owned(), true),
                ("wyrd_platform_admin".to_owned(), true),
            ]
        );
    }

    async fn assert_single_tenant(fixture: &PgFixture) {
        // vala_sql's olap_minimal migration seeds the reserved system-owner
        // sentinel tenant (nil UUID), so the fixture's own seeded tenant is the
        // only non-system row.
        let count: (i64,) =
            sqlx::query_as("SELECT count(*) FROM platform.tenants WHERE data_tenant_id <> $1")
                .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
                .fetch_one(fixture.platform_admin_pool())
                .await
                .expect("tenant count query succeeds");

        assert_eq!(count.0, 1);
    }

    async fn assert_seeded_tenant(fixture: &PgFixture, expected_slug: &str) {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT slug
             FROM platform.tenants
             WHERE data_tenant_id = $1",
        )
        .bind(fixture.data_tenant_id().as_uuid())
        .fetch_all(fixture.platform_admin_pool())
        .await
        .expect("tenant query succeeds");

        assert_eq!(rows, vec![(expected_slug.to_owned(),)]);
    }

    async fn assert_current_tenant_bound(fixture: &PgFixture) {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let current: (String,) = sqlx::query_as("SELECT current_setting($1)")
            .bind(CURRENT_TENANT_GUC)
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("current tenant setting is readable");

        assert_eq!(current.0, fixture.data_tenant_id().to_string());
        conn.commit().await.expect("tenant conn commits");
    }

    async fn assert_bare_app_pool_fails_loudly(fixture: &PgFixture) {
        let err = sqlx::query("SELECT count(*) FROM wyrd.auth_users")
            .execute(fixture.app_pool())
            .await
            .expect_err("bare app pool must fail without tenant binding");
        let msg = err.to_string();

        assert!(
            msg.contains("invalid input syntax for type uuid")
                || msg.contains("unrecognized configuration parameter"),
            "expected loud RLS failure, got: {err}"
        );
    }

    fn assert_path_removed(path: PathBuf) {
        assert!(
            !path.exists(),
            "fixture temporary data directory should be removed after drop: {path:?}"
        );
    }
}
