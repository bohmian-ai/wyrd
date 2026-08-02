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
use wyrd_sql::{OperatorPool, PoolConfig, SqlError, TenantConn, WyrdPostgres};

static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Per-test Postgres fixture backed by a fixture-owned database.
pub struct PgFixture {
    /// Runtime Wyrd handle that opens RLS-bound tenant transactions.
    wyrd: WyrdPostgres,
    /// Runtime Vala handle that opens tenant transactions for analytical state.
    vala: ValaPostgres,
    /// Cross-tenant handle used only for fixture setup and operator assertions.
    operator_pool: OperatorPool,
    /// Catalog-role DSN used by embedded Iceberg catalog fixtures.
    catalog_dsn: SecretString,
    /// Migrator-role DSN reserved for schema-level fixture assertions.
    migrator_dsn: SecretString,
    /// Tenant seeded when this fixture starts.
    data_tenant_id: DataTenantId,
    /// Human-readable slug associated with the seeded tenant.
    tenant_slug: String,
    /// Database owner whose drop implementation cleans up the isolated database.
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

    /// Build independent runtime pools for one simulated Wyrd process.
    ///
    /// Cluster fixtures share the migrated database and durable data, but each
    /// simulated process must own its own app, platform-admin, and Vala SQLx
    /// pools just as separately started production processes do.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError`] when the fixture database DSNs cannot be resolved
    /// or either fresh runtime pool graph cannot connect.
    pub async fn fresh_runtime_handles(&self) -> Result<(WyrdPostgres, ValaPostgres), SqlError> {
        let dsns = self._test_db.resolved_dsns()?;
        let wyrd = WyrdPostgres::connect_from_dsns(&dsns).await?;
        let vala = ValaPostgres::connect_after_wyrd(&dsns).await?;
        Ok((wyrd, vala))
    }

    /// Borrow the runtime `wyrd_app` pool.
    #[must_use]
    pub fn app_pool(&self) -> &PgPool {
        self.wyrd.app_pool()
    }

    /// Borrow the audited cross-tenant operator handle.
    #[must_use]
    pub fn operator_pool(&self) -> &OperatorPool {
        &self.operator_pool
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
        seed_tenant(&self.operator_pool, data_tenant_id, slug).await?;
        Ok(data_tenant_id)
    }

    /// Return the fixture's seeded tenant slug.
    #[must_use]
    pub fn tenant_slug(&self) -> &str {
        &self.tenant_slug
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
        seed_tenant(&self.operator_pool, data_tenant_id, slug).await
    }

    /// Open a pool connected as the `wyrd_migrator` (table-owner) role.
    ///
    /// Use this pool in test assertions that need to read across all tenants
    /// without RLS. The `wyrd_migrator` role is the table owner and has the
    /// `BYPASSRLS` attribute, making it the lowest-friction read path for raw
    /// assertion queries. It is not a PostgreSQL superuser and cannot create
    /// databases.
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

    /// Start an isolated fixture and seed the requested tenant identity.
    ///
    /// # Errors
    /// Returns [`FixtureError`] when database creation, migration, role-handle
    /// construction, or tenant seeding fails.
    async fn start_seeded(
        data_tenant_id: DataTenantId,
        tenant_slug: String,
    ) -> Result<Self, FixtureError> {
        let test_db = TestDatabase::create().await?;
        let handles = test_db.connect_handles().await?;
        let resolved = test_db.resolved_dsns()?;
        let catalog_dsn = resolved.catalog_app;
        let migrator_dsn = resolved.migrator;
        seed_tenant(&handles.operator_pool, data_tenant_id, &tenant_slug).await?;

        Ok(Self {
            operator_pool: handles.operator_pool,
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

/// Owns one ephemeral database and the admin authority required to clean it up.
struct TestDatabase {
    /// Unique database name allocated for this fixture instance.
    name: String,
    /// Neutral cluster-admin DSN used only for database lifecycle operations.
    admin_dsn: SecretString,
}

struct TestDbHandles {
    /// Runtime Wyrd handle for tenant-scoped fixture operations.
    wyrd: WyrdPostgres,
    /// Runtime Vala handle for tenant-scoped analytical fixture operations.
    vala: ValaPostgres,
    /// Cross-tenant operator capability required during fixture seeding.
    operator_pool: OperatorPool,
}

impl TestDatabase {
    /// Creates an isolated database, grants the migrator its narrow database
    /// privileges, and applies migrations through the migrator connection.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the admin DSN is missing or invalid, the
    /// database cannot be created or granted, or migrations fail.
    async fn create() -> Result<Self, SqlError> {
        let base = resolved_external_test_dsns()?;
        let admin_dsn = test_database_admin_dsn("wyrd")?;
        let name = unique_database_name();
        let admin_pool = build_pool(admin_dsn.expose_secret(), PoolConfig::migrator_defaults())
            .await
            .map_err(SqlError::Connect)?;

        sqlx::query(AssertSqlSafe(format!("CREATE DATABASE {name}")))
            .execute(&admin_pool)
            .await
            .map_err(SqlError::from)?;
        sqlx::query(AssertSqlSafe(format!(
            "GRANT CONNECT, CREATE ON DATABASE {name} TO wyrd_migrator"
        )))
        .execute(&admin_pool)
        .await
        .map_err(SqlError::from)?;
        admin_pool.close().await;

        let test_db = Self { name, admin_dsn };
        test_db.migrate(&base).await?;
        Ok(test_db)
    }

    /// Connect the typed runtime handles used by a migrated fixture database.
    ///
    /// # Errors
    /// Returns [`SqlError`] when DSN resolution, pool construction, or the
    /// required operator capability is unavailable.
    async fn connect_handles(&self) -> Result<TestDbHandles, SqlError> {
        let dsns = self.resolved_dsns()?;
        let wyrd = WyrdPostgres::connect_from_dsns(&dsns).await?;
        let vala = ValaPostgres::connect_after_wyrd(&dsns).await?;
        let operator_pool = wyrd
            .operator_pool()
            .ok_or_else(|| SqlError::InvariantViolation {
                detail: "test DB env unset (WYRD_DATABASE_PLATFORM_ADMIN_PASSWORD); platform-admin pool is required for tenant seeding".to_owned(),
            })?;
        Ok(TestDbHandles {
            wyrd,
            vala,
            operator_pool,
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
        })
    }
}

impl Drop for TestDatabase {
    /// Drops the owned ephemeral database through the neutral admin connection.
    fn drop(&mut self) {
        let database_name = self.name.clone();
        let admin_dsn = self.admin_dsn.clone();
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
                let pool =
                    match build_pool(admin_dsn.expose_secret(), PoolConfig::migrator_defaults())
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

/// Resolves the neutral admin DSN to a specific ephemeral database name.
///
/// # Errors
/// Returns [`SqlError::InvariantViolation`] when the lifecycle-only admin
/// environment variable is unset or the DSN cannot be parsed or rewritten.
fn test_database_admin_dsn(database: &str) -> Result<SecretString, SqlError> {
    let value = env::var("WYRD_TEST_DATABASE_ADMIN_URL").map_err(|_| {
        SqlError::InvariantViolation {
            detail: "test DB env unset (WYRD_TEST_DATABASE_ADMIN_URL); refusing to create or drop fixture database".to_owned(),
        }
    })?;
    database_dsn(&SecretString::from(value), database)
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

/// Insert one platform tenant through the fixture's operator capability.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the tenant insert.
async fn seed_tenant(
    operator_pool: &OperatorPool,
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
    .execute(operator_pool.pool())
    .await
    .map_err(SqlError::from)?;

    Ok(())
}

#[cfg(test)]
mod pg_tests {
    use secrecy::ExposeSecret;

    use super::{PgFixture, database_dsn};
    use wyrd_spec::DataTenantId;
    use wyrd_sql::PoolConfig;
    use wyrd_sql::pool::build_pool;
    use wyrd_sql::tenant_conn::CURRENT_TENANT_GUC;

    /// A real fixture migrates, binds tenants, and preserves the database
    /// lifecycle boundary between its neutral administrator and migrator.
    #[tokio::test]
    async fn fixture_smoke() {
        let fixture = PgFixture::start().await.expect("fixture starts");

        assert_required_schemas(&fixture).await;
        assert_required_roles(&fixture).await;
        assert_single_tenant(&fixture).await;
        assert_seeded_tenant(&fixture, fixture.tenant_slug()).await;
        assert_current_tenant_bound(&fixture).await;
        assert_bare_app_pool_fails_loudly(&fixture).await;
        assert_database_authority_boundary(&fixture).await;
    }

    /// The lifecycle administrator owns the database while the migrator owns
    /// migrated schemas but cannot create or drop databases.
    async fn assert_database_authority_boundary(fixture: &PgFixture) {
        let admin = build_pool(
            fixture._test_db.admin_dsn.expose_secret(),
            PoolConfig::migrator_defaults(),
        )
        .await
        .expect("neutral administrator connects");
        let owner: String = sqlx::query_scalar(
            "SELECT owner.rolname FROM pg_database database \
             JOIN pg_roles owner ON owner.oid=database.datdba WHERE database.datname=$1",
        )
        .bind(&fixture._test_db.name)
        .fetch_one(&admin)
        .await
        .expect("database owner reads");
        assert_eq!(owner, "wyrd_test_admin");
        admin.close().await;

        let fixture_admin_dsn = database_dsn(&fixture._test_db.admin_dsn, &fixture._test_db.name)
            .expect("fixture administrator DSN rewrites");
        let fixture_admin = build_pool(
            fixture_admin_dsn.expose_secret(),
            PoolConfig::migrator_defaults(),
        )
        .await
        .expect("neutral administrator connects to fixture database");
        let schema_owners: Vec<(String, String)> = sqlx::query_as(
            "SELECT namespace.nspname, owner.rolname FROM pg_namespace namespace \
             JOIN pg_roles owner ON owner.oid=namespace.nspowner \
             WHERE namespace.nspname IN ('platform','wyrd','vala') ORDER BY namespace.nspname",
        )
        .fetch_all(&fixture_admin)
        .await
        .expect("migration schema owners read");
        assert_eq!(
            schema_owners,
            vec![
                ("platform".to_owned(), "wyrd_migrator".to_owned()),
                ("vala".to_owned(), "wyrd_migrator".to_owned()),
                ("wyrd".to_owned(), "wyrd_migrator".to_owned()),
            ]
        );
        let recovery_authority: (bool, bool, bool) = sqlx::query_as(
            "SELECT has_function_privilege('wyrd_platform_admin', \
               'vala.append_oracle_admission_recovery_audit(text,bigint,bigint,bigint,bigint,bigint)', 'EXECUTE'), \
             NOT has_table_privilege('wyrd_platform_admin','vala.audit_chain_head','SELECT,INSERT,UPDATE,DELETE'), \
             NOT has_table_privilege('wyrd_platform_admin','vala.audit_outbox','SELECT,INSERT,UPDATE,DELETE')",
        )
        .fetch_one(&fixture_admin)
        .await
        .expect("Oracle recovery audit authority reads");
        assert_eq!(recovery_authority, (true, true, true));
        fixture_admin.close().await;

        let postgres_migrator_dsn =
            database_dsn(&fixture.migrator_dsn, "postgres").expect("migrator DSN rewrites");
        let migrator = build_pool(
            postgres_migrator_dsn.expose_secret(),
            PoolConfig::migrator_defaults(),
        )
        .await
        .expect("migrator connects to maintenance database");
        for statement in [
            "CREATE DATABASE wyrd_forbidden_create",
            "DROP DATABASE wyrd",
        ] {
            let error = sqlx::query(statement)
                .execute(&migrator)
                .await
                .expect_err("migrator database lifecycle operation is denied");
            assert_eq!(
                error
                    .as_database_error()
                    .and_then(|database| database.code())
                    .as_deref(),
                Some("42501"),
                "{statement}"
            );
        }
        migrator.close().await;
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
                    .fetch_one(fixture.operator_pool().pool())
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
        .fetch_all(fixture.operator_pool().pool())
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
                .fetch_one(fixture.operator_pool().pool())
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
        .fetch_all(fixture.operator_pool().pool())
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
