//! Shared Postgres fixtures for SQL integration tests.

use sqlx::PgPool;
use wyrd_spec::DataTenantId;
use wyrd_sql::{SqlError, TenantConn};

/// Per-test Postgres fixture backed by the shared docker `wyrd_test` database.
pub struct PgFixture {
    app_pool: PgPool,
    platform_admin_pool: PgPool,
    data_tenant_id: DataTenantId,
    tenant_slug: String,
}

/// Errors returned while starting a shared Postgres fixture.
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    /// SQL layer failed.
    #[error("sql layer failed: {0}")]
    Sql(#[from] SqlError),
}

impl PgFixture {
    /// Reset the shared test database and seed one deterministic test tenant row.
    ///
    /// # Errors
    /// Returns [`FixtureError`] when shared DB setup, reset, or seed insert fails.
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

    /// Return the fixture's tenant isolation key.
    #[must_use]
    pub fn data_tenant_id(&self) -> DataTenantId {
        self.data_tenant_id
    }

    /// Seed an additional active tenant row and return its isolation key.
    ///
    /// **Test-only fixture — never runs on a real server boot.** This method is
    /// only available behind the `pg` feature, which is a dev-dependency of
    /// `wyrd-server` and is never compiled into a production binary.
    ///
    /// The fixture seeds one tenant at boot; multi-tenant isolation tests call
    /// this to provision a second tenant so two distinct [`TenantConn`] handles
    /// can prove RLS scoping. The row is written through the
    /// `wyrd_platform_admin` pool, matching the boot seed path.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the tenant insert fails.
    pub async fn seed_additional_tenant(&self, slug: &str) -> Result<DataTenantId, SqlError> {
        let data_tenant_id = DataTenantId::new_v7();
        seed_tenant(&self.platform_admin_pool, data_tenant_id, slug).await?;
        Ok(data_tenant_id)
    }

    /// Return the fixture's seeded tenant slug.
    #[must_use]
    pub fn tenant_slug(&self) -> &str {
        &self.tenant_slug
    }

    async fn start_seeded(
        data_tenant_id: DataTenantId,
        tenant_slug: String,
    ) -> Result<Self, FixtureError> {
        let db = vala_sql::testing::shared().await?;
        vala_sql::testing::reset_for_test(&db).await?;
        seed_tenant(&db.platform_admin, data_tenant_id, &tenant_slug).await?;

        Ok(Self {
            app_pool: db.app.clone(),
            platform_admin_pool: db.platform_admin.clone(),
            data_tenant_id,
            tenant_slug,
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
}
