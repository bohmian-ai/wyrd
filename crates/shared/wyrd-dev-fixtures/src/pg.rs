//! Shared Postgres fixtures for SQL integration tests.

use std::collections::hash_map::DefaultHasher;
use std::env;
use std::hash::{Hash, Hasher};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use secrecy::{ExposeSecret, SecretString};
use sqlx::{AssertSqlSafe, PgPool};
use url::Url;
use vala_sql::ValaPostgres;
use wyrd_spec::DataTenantId;
use wyrd_sql::dsn::ResolvedDsns;
use wyrd_sql::pool::build_pool;
use wyrd_sql::{PoolConfig, SqlError, TenantConn, WyrdPostgres};

static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Per-test Postgres fixture backed by a fixture-owned database.
pub struct PgFixture {
    wyrd: WyrdPostgres,
    vala: ValaPostgres,
    platform_admin_pool: PgPool,
    catalog_dsn: SecretString,
    migrator_dsn: SecretString,
    data_tenant_id: DataTenantId,
    tenant_slug: String,
    _test_db: TestDatabase,
}

/// Errors returned while starting a shared Postgres fixture.
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    /// SQL layer failed.
    #[error("sql layer failed: {0}")]
    Sql(#[from] SqlError),
}

impl PgFixture {
    /// Create an isolated test database and seed one deterministic test tenant row.
    ///
    /// # Errors
    /// Returns [`FixtureError`] when database creation, migration, or seed insert fails.
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
        self.wyrd.tenant_conn(data_tenant_id).await
    }

    /// Borrow the real control-plane handle.
    #[must_use]
    pub fn wyrd_postgres(&self) -> &WyrdPostgres {
        &self.wyrd
    }

    /// Borrow the real Vala warehouse handle.
    #[must_use]
    pub fn vala_postgres(&self) -> &ValaPostgres {
        &self.vala
    }

    /// Borrow the runtime `wyrd_app` pool.
    #[must_use]
    pub fn app_pool(&self) -> &PgPool {
        self.wyrd.app_pool()
    }

    /// Borrow the audited `wyrd_platform_admin` pool.
    #[must_use]
    pub fn platform_admin_pool(&self) -> &PgPool {
        &self.platform_admin_pool
    }

    /// Borrow the fixture database's Bifrost catalog DSN.
    #[must_use]
    pub fn catalog_dsn(&self) -> &SecretString {
        &self.catalog_dsn
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

    /// Open a pool connected as the `vala_recovery` role against the fixture database.
    ///
    /// Reads `VALA_RECOVERY_PASSWORD` from the environment. Callers that test
    /// Bifrost crash-recovery paths need this pool to simulate the recovery role.
    ///
    /// # Errors
    /// Returns [`FixtureError`] when `VALA_RECOVERY_PASSWORD` is unset or the
    /// pool cannot connect.
    pub async fn recovery_pool(&self) -> Result<PgPool, FixtureError> {
        let password =
            std::env::var(vala_sql::postgres::VALA_RECOVERY_PASSWORD_ENV).map_err(|_| {
                SqlError::InvariantViolation {
                    detail: "VALA_RECOVERY_PASSWORD must be set to connect recovery pool in tests"
                        .to_owned(),
                }
            })?;
        let dsns = self._test_db.resolved_dsns()?;
        Ok(vala_sql::postgres::connect_recovery_pool(&dsns, SecretString::from(password)).await?)
    }

    /// Build the iceberg catalog URI for this fixture's database.
    ///
    /// The iceberg-catalog-sql crate builds its own pool from this URI; it does
    /// not accept a shared pool handle. The URI sets `role=wyrd_catalog` and
    /// `search_path=iceberg_catalog` as PostgreSQL session options so that tables
    /// created by the catalog crate are owned by `wyrd_catalog`, matching the
    /// `ALTER DEFAULT PRIVILEGES FOR ROLE wyrd_catalog` boundary in the migration.
    #[must_use]
    pub fn catalog_uri(&self) -> String {
        let base = self.catalog_dsn.expose_secret();
        let sep = if base.contains('?') { "&" } else { "?" };
        format!("{base}{sep}options=-c%20role%3Dwyrd_catalog%20-c%20search_path%3Diceberg_catalog")
    }

    /// Seed an additional active tenant row with a caller-supplied isolation key.
    ///
    /// Use this when a test needs a tenant with a **specific** [`DataTenantId`]
    /// (e.g., the nil UUID sentinel, or a UUID carried across a serialized
    /// payload). For tests that only need a second distinct tenant, prefer
    /// [`seed_additional_tenant`][Self::seed_additional_tenant], which generates a
    /// fresh UUIDv7.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the tenant insert fails.
    pub async fn seed_additional_tenant_with_uuid(
        &self,
        data_tenant_id: DataTenantId,
        slug: &str,
    ) -> Result<(), SqlError> {
        seed_tenant(&self.platform_admin_pool, data_tenant_id, slug).await
    }

    /// Open a pool connected as the `wyrd_migrator` (table-owner) role.
    ///
    /// Use this pool in test assertions that need to read across all tenants
    /// without RLS — for example, inspecting `vala.olap_commits` or
    /// `vala.olap_recovery_events` after a multi-tenant write. The
    /// `wyrd_migrator` role is the table owner and bypasses all row-level
    /// security, making it the lowest-friction read path for raw assertion
    /// queries.
    ///
    /// A new pool is created on each call; cache it in a local if you need
    /// it more than once per test.
    ///
    /// # Errors
    /// Returns [`FixtureError`] when the pool cannot connect.
    pub async fn superuser_pool(&self) -> Result<PgPool, FixtureError> {
        build_pool(
            self.migrator_dsn.expose_secret(),
            PoolConfig::migrator_defaults(),
        )
        .await
        .map_err(SqlError::Connect)
        .map_err(FixtureError::Sql)
    }

    async fn start_seeded(
        data_tenant_id: DataTenantId,
        tenant_slug: String,
    ) -> Result<Self, FixtureError> {
        let test_db = TestDatabase::create().await?;
        let handles = test_db.connect_handles().await?;
        let resolved = test_db.resolved_dsns()?;
        let catalog_dsn = resolved.catalog_app;
        let migrator_dsn = resolved.migrator;
        seed_tenant(&handles.platform_admin, data_tenant_id, &tenant_slug).await?;

        Ok(Self {
            platform_admin_pool: handles.platform_admin,
            wyrd: handles.wyrd,
            vala: handles.vala,
            catalog_dsn,
            migrator_dsn,
            data_tenant_id,
            tenant_slug,
            _test_db: test_db,
        })
    }
}

struct TestDatabase {
    name: String,
    maintenance_dsn: SecretString,
}

struct TestDbHandles {
    wyrd: WyrdPostgres,
    vala: ValaPostgres,
    platform_admin: PgPool,
}

impl TestDatabase {
    async fn create() -> Result<Self, SqlError> {
        let base = resolved_external_test_dsns()?;
        let maintenance_dsn = database_dsn(&base.migrator, "wyrd")?;
        let name = unique_database_name();
        let maintenance_pool = build_pool(
            maintenance_dsn.expose_secret(),
            PoolConfig::migrator_defaults(),
        )
        .await
        .map_err(SqlError::Connect)?;

        sqlx::query(AssertSqlSafe(format!(
            "CREATE DATABASE {name} OWNER wyrd_migrator"
        )))
        .execute(&maintenance_pool)
        .await
        .map_err(SqlError::from)?;
        maintenance_pool.close().await;

        let test_db = Self {
            name,
            maintenance_dsn,
        };
        test_db.migrate(&base).await?;
        Ok(test_db)
    }

    async fn connect_handles(&self) -> Result<TestDbHandles, SqlError> {
        let dsns = self.resolved_dsns()?;
        let wyrd = WyrdPostgres::connect_from_dsns(&dsns).await?;
        let vala = ValaPostgres::connect_after_wyrd(&dsns).await?;
        let platform_admin = wyrd
            .platform_admin_pool()
            .cloned()
            .ok_or_else(|| SqlError::InvariantViolation {
                detail: "test DB env unset (WYRD_DATABASE_PLATFORM_ADMIN_PASSWORD); platform-admin pool is required for tenant seeding".to_owned(),
            })?;
        Ok(TestDbHandles {
            wyrd,
            vala,
            platform_admin,
        })
    }

    async fn migrate(&self, base: &ResolvedDsns) -> Result<(), SqlError> {
        let migrator_dsn = database_dsn(&base.migrator, &self.name)?;
        let migrator = build_pool(
            migrator_dsn.expose_secret(),
            PoolConfig::migrator_defaults(),
        )
        .await
        .map_err(SqlError::Connect)?;

        let result = async {
            wyrd_sql::migrate(&migrator).await?;
            vala_sql::migrate(&migrator).await
        }
        .await;
        migrator.close().await;
        result
    }

    fn resolved_dsns(&self) -> Result<ResolvedDsns, SqlError> {
        let base = resolved_external_test_dsns()?;
        Ok(ResolvedDsns {
            app: database_dsn(&base.app, &self.name)?,
            migrator: database_dsn(&base.migrator, &self.name)?,
            platform_admin: base
                .platform_admin
                .as_ref()
                .map(|dsn| database_dsn(dsn, &self.name))
                .transpose()?,
            catalog_app: database_dsn(&base.catalog_app, &self.name)?,
            recovery: database_dsn(&base.recovery, &self.name)?,
        })
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        let database_name = self.name.clone();
        let maintenance_dsn = self.maintenance_dsn.clone();
        let handle = thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    eprintln!("failed to build test database cleanup runtime: {error}");
                    return;
                }
            };

            runtime.block_on(async move {
                let pool = match build_pool(
                    maintenance_dsn.expose_secret(),
                    PoolConfig::migrator_defaults(),
                )
                .await
                {
                    Ok(pool) => pool,
                    Err(error) => {
                        eprintln!("failed to connect for test database cleanup: {error}");
                        return;
                    }
                };
                let drop_result = sqlx::query(AssertSqlSafe(format!(
                    "DROP DATABASE IF EXISTS {database_name} WITH (FORCE)"
                )))
                .execute(&pool)
                .await;
                pool.close().await;

                if let Err(error) = drop_result {
                    eprintln!("failed to drop test database {database_name}: {error}");
                }
            });
        });

        if handle.join().is_err() {
            eprintln!("test database cleanup thread panicked");
        }
    }
}

fn resolved_external_test_dsns() -> Result<ResolvedDsns, SqlError> {
    wyrd_sql::dsn::resolve_external_dsns_from_env()
        .map_err(|error| SqlError::InvariantViolation {
            detail: format!("test DB DSN config error: {error}"),
        })?
        .ok_or_else(|| SqlError::InvariantViolation {
            detail: "test DB env unset (WYRD_DATABASE_URL + WYRD_DATABASE_MIGRATOR_PASSWORD); refusing to boot embedded in tests".to_owned(),
        })
}

fn database_dsn(base: &SecretString, database: &str) -> Result<SecretString, SqlError> {
    let mut url =
        Url::parse(base.expose_secret()).map_err(|error| SqlError::InvariantViolation {
            detail: format!("test DB DSN parse error: {error}"),
        })?;
    url.set_path(database);
    Ok(SecretString::from(url.to_string()))
}

fn unique_database_name() -> String {
    let cwd = env::current_dir()
        .ok()
        .and_then(|path| path.into_os_string().into_string().ok())
        .unwrap_or_else(|| "unknown".to_owned());
    let mut hasher = DefaultHasher::new();
    cwd.hash(&mut hasher);
    let worktree_hash = hasher.finish();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = TEST_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    let raw = format!(
        "wyrd_test_{worktree_hash:016x}_{:x}_{counter:x}_{now:x}",
        process::id()
    );
    raw.chars().take(63).collect()
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
mod pg_tests {
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
