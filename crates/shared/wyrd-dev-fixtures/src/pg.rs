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
use wyrd_sql::row_types::auth::HumanConnectionBinding;
use wyrd_sql::{
    MIGRATION_LEASE_WAIT, OperatorPool, PoolConfig, SqlError, TenantConn, WyrdPostgres,
};

static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Return the bound tenant's Active human connection binding, seeding a public
/// Active connection when the tenant has none.
///
/// Every human session is bound to the exact connection revision it logged in
/// through, and issuance refuses one whose connection is not Active. Tests
/// that mint or rotate human sessions without driving a provider use this to
/// give the session a real, Active connection to belong to.
///
/// # Errors
/// Returns [`SqlError`] when the read or insert fails.
pub async fn seed_active_human_connection(
    conn: &mut TenantConn<'_>,
) -> Result<HumanConnectionBinding, SqlError> {
    sqlx::query_as::<_, HumanConnectionBinding>(
        "WITH existing AS (
             SELECT connection_id, revision AS connection_revision
               FROM wyrd.auth_human_connections
              WHERE state = 'Active' AND removed_at IS NULL),
         inserted AS (
             INSERT INTO wyrd.auth_human_connections (
                 connection_id, data_tenant_id, revision, state, issuer_url, client_id,
                 client_auth, claim_mapping, group_role_map, jwks_ttl_secs, jwks_uri)
             SELECT $1, $2,
                    (SELECT COALESCE(max(revision), 0) + 1 FROM wyrd.auth_human_connections),
                    'Active', 'https://idp.fixture.test', 'wyrd-fixture', 'Public',
                    '{\"subject\": \"sub\"}'::jsonb, '{}'::jsonb, 300,
                    'https://idp.fixture.test/jwks'
              WHERE NOT EXISTS (SELECT 1 FROM existing)
             RETURNING connection_id, revision AS connection_revision)
         SELECT connection_id, connection_revision FROM existing
         UNION ALL
         SELECT connection_id, connection_revision FROM inserted",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(conn.data_tenant_id().as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

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
    /// Tenant seeded when this fixture starts.
    data_tenant_id: DataTenantId,
    /// Human-readable slug associated with the seeded tenant.
    tenant_slug: String,
    /// Shared database-owner pool retained for BYPASSRLS assertion probes.
    ///
    /// Callers receive cheap clones and must drop them normally. They must not
    /// call [`PgPool::close`] because SQLx closes the shared pool across every
    /// clone. This field precedes `test_db` so the pool owner drops before the
    /// ephemeral database is forcibly removed.
    assertion_pool: PgPool,
    /// Database owner whose drop implementation cleans up the isolated database.
    test_db: TestDatabase,
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

    /// Binds to a fixture database another process created and seeded.
    ///
    /// Multi-process harnesses need every child to reach the same database as
    /// its parent without recreating, re-migrating, or re-seeding it, and
    /// without dropping it when the child exits. The caller supplies the
    /// database name, tenant identity, and slug the creating process published.
    ///
    /// # Errors
    /// Returns [`FixtureError`] when DSN resolution or pool construction fails,
    /// or the platform-admin capability is unavailable.
    pub async fn attach(
        database_name: String,
        data_tenant_id: DataTenantId,
        tenant_slug: String,
    ) -> Result<Self, FixtureError> {
        let test_db = TestDatabase::attach(database_name).map_err(FixtureError::from)?;
        let handles = test_db.connect_handles().await?;
        let resolved = test_db.resolved_dsns()?;
        let assertion_pool = test_db.owner_pool().await?;
        Ok(Self {
            operator_pool: handles.operator_pool,
            wyrd: handles.wyrd,
            vala: handles.vala,
            catalog_dsn: resolved.catalog(),
            data_tenant_id,
            tenant_slug,
            assertion_pool,
            test_db,
        })
    }

    /// Returns the name of the database this fixture is bound to.
    ///
    /// Multi-process harnesses publish it to their children so every replica
    /// attaches to the same database rather than creating its own.
    #[must_use]
    pub fn database_name(&self) -> &str {
        &self.test_db.name
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
        let dsns = self.test_db.resolved_dsns()?;
        let wyrd = WyrdPostgres::connect_from_dsns(&dsns).await?;
        let vala = ValaPostgres::connect_from_dsns(&dsns).await?;
        Ok((wyrd, vala))
    }

    /// Makes every insert into `vala.audit_staging` fail until
    /// [`Self::restore_audit_staging`].
    ///
    /// Audit outbox tests use this to stand in for an unavailable database:
    /// each attempt to commit staged audit fails and is retried, while the rest
    /// of the database keeps serving the audited operation itself.
    ///
    /// # Errors
    /// Returns [`FixtureError`] when the trigger cannot be installed.
    pub async fn fail_audit_staging(&self) -> Result<(), FixtureError> {
        sqlx::raw_sql(
            r"CREATE OR REPLACE FUNCTION vala.test_fail_audit_staging()
               RETURNS trigger LANGUAGE plpgsql AS $$
               BEGIN
                 RAISE EXCEPTION 'injected audit staging failure';
               END;
               $$;
             CREATE TRIGGER test_fail_audit_staging
               BEFORE INSERT ON vala.audit_staging
               FOR EACH ROW EXECUTE FUNCTION vala.test_fail_audit_staging();",
        )
        .execute(&self.assertion_pool)
        .await
        .map_err(SqlError::from)?;
        Ok(())
    }

    /// Lets inserts into `vala.audit_staging` succeed again after
    /// [`Self::fail_audit_staging`].
    ///
    /// # Errors
    /// Returns [`FixtureError`] when the trigger cannot be dropped.
    pub async fn restore_audit_staging(&self) -> Result<(), FixtureError> {
        sqlx::raw_sql("DROP TRIGGER IF EXISTS test_fail_audit_staging ON vala.audit_staging")
            .execute(&self.assertion_pool)
            .await
            .map_err(SqlError::from)?;
        Ok(())
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

    /// Seed an additional active tenant row with a caller-supplied isolation key.
    ///
    /// Use this when a test needs a tenant with a **specific** [`DataTenantId`]
    /// (e.g., a UUID carried across a serialized
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

    /// Clone the fixture-owned pool connected as the database-owner login.
    ///
    /// Use this pool in test assertions that need to read across all tenants
    /// without RLS. The owner login ran the migrations, owns every migrated
    /// object, and bypasses row-level security, making it the lowest-friction
    /// read path for raw assertion queries. Serving code never holds it.
    ///
    /// The fixture retains this pool for its full lifetime so repeated probes
    /// do not create new SQLx pool graphs. Callers must drop returned clones
    /// normally and never call [`PgPool::close`], which closes the shared pool
    /// for every clone.
    ///
    /// # Errors
    /// The result remains fallible for API compatibility. Pool construction
    /// errors are returned by fixture startup, so this method returns `Ok`
    /// after a fixture has started successfully.
    pub fn superuser_pool(&self) -> Result<PgPool, FixtureError> {
        Ok(self.assertion_pool.clone())
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
        let catalog_dsn = test_db.resolved_dsns()?.catalog();
        let assertion_pool = test_db.owner_pool().await?;
        seed_tenant(&handles.operator_pool, data_tenant_id, &tenant_slug).await?;

        Ok(Self {
            operator_pool: handles.operator_pool,
            wyrd: handles.wyrd,
            vala: handles.vala,
            catalog_dsn,
            data_tenant_id,
            tenant_slug,
            assertion_pool,
            test_db,
        })
    }
}

/// An isolated, fixture-owned database with no migration applied.
///
/// Upgrade tests use it to stop the schema at a chosen historical version,
/// stage legacy rows, and then prove how a later migration treats them. The
/// database is dropped when this value drops.
pub struct UnmigratedDatabase {
    /// Migrator-role pool on the empty database. Precedes `_test_db` so it
    /// drops before the database is removed.
    migrator: PgPool,
    /// Database owner whose drop removes the isolated database.
    _test_db: TestDatabase,
}

impl UnmigratedDatabase {
    /// Create an empty isolated database and connect the migrator role to it.
    ///
    /// # Errors
    /// Returns [`FixtureError`] when the test DSNs are unset or invalid, or the
    /// database cannot be created, granted, or connected.
    pub async fn create() -> Result<Self, FixtureError> {
        let test_db = TestDatabase::create_empty().await?;
        let migrator = test_db.owner_pool().await?;
        Ok(Self {
            migrator,
            _test_db: test_db,
        })
    }

    /// The migrator-role pool, which owns DDL on this database.
    #[must_use]
    pub fn migrator_pool(&self) -> &PgPool {
        &self.migrator
    }
}

/// Owns one ephemeral database and the admin authority required to clean it up.
struct TestDatabase {
    /// Unique database name allocated for this fixture instance.
    name: String,
    /// Neutral cluster-admin DSN used only for database lifecycle operations.
    admin_dsn: SecretString,
    /// Whether dropping this handle also drops the database.
    ///
    /// A multi-process fixture has one creator and several attached readers of
    /// the same database. Only the creator may drop it; an attached handle that
    /// dropped the database would pull it out from under its siblings.
    owned: bool,
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
    /// Creates an isolated database and applies migrations through its owner
    /// login, exactly as `wyrd-server migrate` does for a deployment.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the admin DSN is missing or invalid, the
    /// database cannot be created, or migrations fail.
    async fn create() -> Result<Self, SqlError> {
        let test_db = Self::create_empty().await?;
        test_db.migrate().await?;
        Ok(test_db)
    }

    /// Creates an isolated database without applying migrations.
    ///
    /// # Errors
    /// Returns [`SqlError`] when database creation fails.
    async fn create_empty() -> Result<Self, SqlError> {
        let admin_dsn = test_database_admin_dsn("wyrd")?;
        let name = unique_database_name();
        let admin_pool = build_pool(admin_dsn.expose_secret(), PoolConfig::migrator_defaults())
            .await
            .map_err(SqlError::Connect)?;

        sqlx::query(AssertSqlSafe(format!("CREATE DATABASE {name}")))
            .execute(&admin_pool)
            .await
            .map_err(SqlError::from)?;
        admin_pool.close().await;

        Ok(Self {
            name,
            admin_dsn,
            owned: true,
        })
    }

    /// Binds to an already-created, already-migrated fixture database.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the admin DSN cannot be resolved from the
    /// environment.
    fn attach(name: String) -> Result<Self, SqlError> {
        let admin_dsn = test_database_admin_dsn("wyrd")?;
        Ok(Self {
            name,
            admin_dsn,
            owned: false,
        })
    }

    /// Connect the typed serving handles of a migrated fixture database and
    /// prove it is ready to serve, as serving boot does.
    ///
    /// # Errors
    /// Returns [`SqlError`] when DSN resolution, pool construction, or either
    /// schema readiness check fails.
    async fn connect_handles(&self) -> Result<TestDbHandles, SqlError> {
        let dsns = self.resolved_dsns()?;
        let wyrd = WyrdPostgres::connect_from_dsns(&dsns).await?;
        let vala = ValaPostgres::connect_from_dsns(&dsns).await?;
        wyrd.validate_schema().await?;
        let operator_pool = wyrd
            .operator_pool()
            .ok_or_else(|| SqlError::InvariantViolation {
                detail: "serving handles always carry a platform-admin pool".to_owned(),
            })?;
        vala.validate_schema(&operator_pool).await?;
        Ok(TestDbHandles {
            wyrd,
            vala,
            operator_pool,
        })
    }

    /// Apply Wyrd then Vala migrations through a transient owner pool, under
    /// one migration lease as `wyrd-server migrate` does.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the owner pool cannot connect, the lease
    /// cannot be acquired or released, or either migration set fails.
    async fn migrate(&self) -> Result<(), SqlError> {
        let owner = self.owner_pool().await?;
        let result = async {
            let mut lease = OperatorPool::from(owner.clone())
                .migration_lease(MIGRATION_LEASE_WAIT)
                .await?;
            let migrated = async {
                wyrd_sql::migrate(&mut lease).await?;
                vala_sql::migrate(&mut lease).await
            }
            .await;
            let released = lease.release().await;
            migrated.and(released)
        }
        .await;
        owner.close().await;
        result
    }

    /// Open a pool on this fixture database as its owner login.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the owner DSN cannot be rewritten or the pool
    /// cannot connect.
    async fn owner_pool(&self) -> Result<PgPool, SqlError> {
        let owner_dsn = database_dsn(&self.admin_dsn, &self.name)?;
        build_pool(owner_dsn.expose_secret(), PoolConfig::migrator_defaults())
            .await
            .map_err(SqlError::Connect)
    }

    /// Point the serving DSNs from the environment at this fixture database.
    ///
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] when the serving DSNs are unset
    /// or cannot be rewritten.
    fn resolved_dsns(&self) -> Result<ResolvedDsns, SqlError> {
        let base = ResolvedDsns::from_env().map_err(|error| SqlError::InvariantViolation {
            detail: format!("test DB serving DSN config error: {error}"),
        })?;
        Ok(ResolvedDsns {
            app: database_dsn(&base.app, &self.name)?,
            platform_admin: database_dsn(&base.platform_admin, &self.name)?,
        })
    }
}

impl Drop for TestDatabase {
    /// Drops the owned ephemeral database through the neutral admin connection.
    ///
    /// An attached handle owns nothing and returns immediately, leaving the
    /// database to the process that created it.
    fn drop(&mut self) {
        if !self.owned {
            return;
        }
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
    use secrecy::{ExposeSecret, SecretString};
    use sqlx::AssertSqlSafe;
    use vala_sql::ValaPostgres;
    use wyrd_sql::WyrdPostgres;
    use wyrd_sql::dsn::ResolvedDsns;

    use super::{PgFixture, database_dsn};
    use wyrd_spec::DataTenantId;
    use wyrd_sql::PoolConfig;
    use wyrd_sql::SqlError;
    use wyrd_sql::pool::build_pool;
    use wyrd_sql::tenant_conn::CURRENT_TENANT_GUC;

    /// A real fixture migrates, binds tenants, and preserves the database
    /// lifecycle boundary between its owner login and the serving logins.
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

    /// The owner login owns the database and every migrated schema, while
    /// neither serving login can create or drop databases.
    async fn assert_database_authority_boundary(fixture: &PgFixture) {
        let admin = build_pool(
            fixture.test_db.admin_dsn.expose_secret(),
            PoolConfig::migrator_defaults(),
        )
        .await
        .expect("neutral administrator connects");
        let owner: String = sqlx::query_scalar(
            "SELECT owner.rolname FROM pg_database database \
             JOIN pg_roles owner ON owner.oid=database.datdba WHERE database.datname=$1",
        )
        .bind(&fixture.test_db.name)
        .fetch_one(&admin)
        .await
        .expect("database owner reads");
        assert_eq!(owner, "wyrd_test_admin");
        admin.close().await;

        let fixture_admin_dsn = database_dsn(&fixture.test_db.admin_dsn, &fixture.test_db.name)
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
                ("platform".to_owned(), "wyrd_test_admin".to_owned()),
                ("vala".to_owned(), "wyrd_test_admin".to_owned()),
                ("wyrd".to_owned(), "wyrd_test_admin".to_owned()),
            ]
        );
        fixture_admin.close().await;

        let serving = fixture
            .test_db
            .resolved_dsns()
            .expect("fixture DSNs resolve");
        for login in [&serving.app, &serving.platform_admin] {
            let maintenance_dsn = database_dsn(login, "postgres").expect("serving DSN rewrites");
            let pool = build_pool(
                maintenance_dsn.expose_secret(),
                PoolConfig::migrator_defaults(),
            )
            .await
            .expect("serving login connects to maintenance database");
            for statement in [
                "CREATE DATABASE wyrd_forbidden_create",
                "DROP DATABASE wyrd",
            ] {
                let error = sqlx::query(statement)
                    .execute(&pool)
                    .await
                    .expect_err("serving database lifecycle operation is denied");
                assert_eq!(
                    error
                        .as_database_error()
                        .and_then(sqlx::error::DatabaseError::code)
                        .as_deref(),
                    Some("42501"),
                    "{statement}"
                );
            }
            pool.close().await;
        }
    }

    /// Serving readiness refuses a schema whose migrations drifted, are
    /// missing, or whose tenant tables lost row-level security, their
    /// isolation policy, or the narrow grants the migrations establish; once
    /// restored, readiness and ordinary tenant and platform work pass again.
    #[tokio::test]
    async fn validate_schema_refuses_drifted_or_unprotected_schemas() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let owner = fixture.superuser_pool().expect("owner pool");
        let operator = fixture.operator_pool();
        fixture
            .wyrd_postgres()
            .validate_schema()
            .await
            .expect("freshly migrated Wyrd schema validates");
        fixture
            .vala_postgres()
            .validate_schema(operator)
            .await
            .expect("freshly migrated Vala schema validates");

        for (break_sql, restore_sql, table) in [
            (
                "ALTER TABLE wyrd.storage_multipart_uploads NO FORCE ROW LEVEL SECURITY",
                "ALTER TABLE wyrd.storage_multipart_uploads FORCE ROW LEVEL SECURITY",
                "storage_multipart_uploads",
            ),
            (
                "DROP POLICY tenant_isolation ON wyrd.cards",
                "CREATE POLICY tenant_isolation ON wyrd.cards \
                 USING (data_tenant_id = wyrd.current_tenant()) \
                 WITH CHECK (data_tenant_id = wyrd.current_tenant())",
                "cards",
            ),
            (
                "CREATE POLICY widened ON wyrd.verifier_runs USING (true)",
                "DROP POLICY widened ON wyrd.verifier_runs",
                "verifier_runs",
            ),
        ] {
            sqlx::query(AssertSqlSafe(break_sql))
                .execute(&owner)
                .await
                .expect("owner breaks tenant isolation");
            assert!(
                matches!(
                    fixture.wyrd_postgres().validate_schema().await,
                    Err(SqlError::SchemaNotReady { ref detail }) if detail.contains(table)
                ),
                "{break_sql}"
            );
            sqlx::query(AssertSqlSafe(restore_sql))
                .execute(&owner)
                .await
                .expect("owner restores tenant isolation");
        }

        for (break_sql, restore_sql) in [
            (
                "GRANT CREATE ON SCHEMA wyrd TO wyrd_app",
                "REVOKE CREATE ON SCHEMA wyrd FROM wyrd_app",
            ),
            (
                "GRANT TRUNCATE ON wyrd.cards TO wyrd_app",
                "REVOKE TRUNCATE ON wyrd.cards FROM wyrd_app",
            ),
            (
                "GRANT CREATE ON SCHEMA platform TO wyrd_platform_admin",
                "REVOKE CREATE ON SCHEMA platform FROM wyrd_platform_admin",
            ),
        ] {
            sqlx::query(AssertSqlSafe(break_sql))
                .execute(&owner)
                .await
                .expect("owner widens a serving grant");
            assert!(
                matches!(
                    fixture.wyrd_postgres().validate_schema().await,
                    Err(SqlError::SchemaNotReady { ref detail }) if detail.contains("holds")
                ),
                "{break_sql}"
            );
            sqlx::query(AssertSqlSafe(restore_sql))
                .execute(&owner)
                .await
                .expect("owner restores the grant");
        }

        sqlx::query("GRANT USAGE ON SCHEMA iceberg_catalog TO wyrd_app")
            .execute(&owner)
            .await
            .expect("owner exposes the catalog");
        assert!(matches!(
            fixture.vala_postgres().validate_schema(operator).await,
            Err(SqlError::SchemaNotReady { ref detail }) if detail.contains("iceberg_catalog")
        ));
        sqlx::query("REVOKE USAGE ON SCHEMA iceberg_catalog FROM wyrd_app")
            .execute(&owner)
            .await
            .expect("owner hides the catalog again");

        fixture
            .wyrd_postgres()
            .validate_schema()
            .await
            .expect("restored Wyrd schema validates");
        fixture
            .vala_postgres()
            .validate_schema(operator)
            .await
            .expect("restored Vala schema validates");
        let mut conn = fixture
            .wyrd_postgres()
            .tenant_conn(fixture.data_tenant_id())
            .await
            .expect("tenant transaction opens");
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM wyrd.cards")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("tenant work runs after restore");
        drop(conn);
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM platform.tenants")
            .fetch_one(operator.pool())
            .await
            .expect("platform work runs after restore");

        sqlx::query(
            "UPDATE wyrd._sqlx_migrations SET checksum = '\\x00'::bytea \
             WHERE version = (SELECT max(version) FROM wyrd._sqlx_migrations)",
        )
        .execute(&owner)
        .await
        .expect("owner corrupts a Wyrd checksum");
        assert!(matches!(
            fixture.wyrd_postgres().validate_schema().await,
            Err(SqlError::MigrateChecksum { .. })
        ));

        sqlx::query(
            "DELETE FROM vala._sqlx_migrations \
             WHERE version = (SELECT max(version) FROM vala._sqlx_migrations)",
        )
        .execute(&owner)
        .await
        .expect("owner removes a Vala ledger row");
        assert!(matches!(
            fixture.vala_postgres().validate_schema(operator).await,
            Err(SqlError::SchemaNotReady { ref detail }) if detail.contains("is not applied")
        ));
    }

    /// Serving readiness refuses a serving DSN that logs in as any role other
    /// than the one its pool is for — a grant-capable app login and an
    /// owner-equivalent BYPASSRLS platform login alike.
    #[tokio::test]
    async fn validate_schema_refuses_substituted_serving_logins() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let owner = fixture.superuser_pool().expect("owner pool");
        let suffix = fixture.test_db.name.clone();
        let app_imposter = format!("wyrd_app_imposter_{suffix}");
        let platform_imposter = format!("wyrd_platform_imposter_{suffix}");
        for (role, attributes) in [
            (&app_imposter, "NOBYPASSRLS"),
            (&platform_imposter, "BYPASSRLS"),
        ] {
            sqlx::query(AssertSqlSafe(format!(
                "CREATE ROLE {role} LOGIN {attributes} PASSWORD 'imposter'"
            )))
            .execute(&owner)
            .await
            .expect("owner creates an imposter login");
        }

        let dsns = fixture.test_db.resolved_dsns().expect("serving DSNs");
        for (dsns, pool) in [
            (
                ResolvedDsns {
                    app: login_as(&dsns.app, &app_imposter),
                    platform_admin: dsns.platform_admin.clone(),
                },
                "app",
            ),
            (
                ResolvedDsns {
                    app: dsns.app.clone(),
                    platform_admin: login_as(&dsns.platform_admin, &platform_imposter),
                },
                "platform",
            ),
        ] {
            let wyrd = WyrdPostgres::connect_from_dsns(&dsns)
                .await
                .expect("imposter pools build");
            assert!(
                matches!(
                    wyrd.validate_schema().await,
                    Err(SqlError::SchemaNotReady { ref detail }) if detail.contains("imposter")
                ),
                "{pool} imposter passed readiness"
            );
            if pool == "app" {
                let vala = ValaPostgres::connect_from_dsns(&dsns)
                    .await
                    .expect("imposter Vala pool builds");
                assert!(matches!(
                    vala.validate_schema(fixture.operator_pool()).await,
                    Err(SqlError::SchemaNotReady { ref detail }) if detail.contains("imposter")
                ));
            }
        }

        for role in [&app_imposter, &platform_imposter] {
            sqlx::query(AssertSqlSafe(format!("DROP ROLE {role}")))
                .execute(&owner)
                .await
                .expect("owner drops the imposter login");
        }
        fixture
            .wyrd_postgres()
            .validate_schema()
            .await
            .expect("named serving logins still validate");
    }

    /// Rewrites a serving DSN to log in as `role` with the imposter password.
    fn login_as(dsn: &SecretString, role: &str) -> SecretString {
        let mut url = url::Url::parse(dsn.expose_secret()).expect("serving DSN parses");
        url.set_username(role).expect("DSN accepts a username");
        url.set_password(Some("imposter"))
            .expect("DSN accepts a password");
        SecretString::from(url.to_string())
    }

    #[tokio::test]
    async fn start_with_slug_uses_custom_slug() {
        let fixture = PgFixture::start_with_slug("custom-tenant")
            .await
            .expect("fixture starts with custom slug");

        assert_eq!(fixture.tenant_slug(), "custom-tenant");
        assert_seeded_tenant(&fixture, "custom-tenant").await;
    }

    /// Repeated table-owner probes reuse one fixture pool without exhausting
    /// the isolated Postgres server's connection budget.
    #[tokio::test]
    async fn assertion_pool_supports_repeated_fixture_probes() {
        let fixture = PgFixture::start().await.expect("fixture starts");

        for expected in 0_i32..32 {
            let assertion_pool = fixture.superuser_pool().expect("assertion pool clone");
            let (current_user, bypasses_rls, observed): (String, bool, i32) = sqlx::query_as(
                "SELECT current_user, rolbypassrls, $1::integer FROM pg_roles WHERE rolname = current_user",
            )
            .bind(expected)
            .fetch_one(&assertion_pool)
            .await
            .expect("table-owner assertion probe");

            assert_eq!(current_user, "wyrd_test_admin");
            assert!(bypasses_rls);
            assert_eq!(observed, expected);
            drop(assertion_pool);
        }
    }

    /// SQLx pool clones share lifecycle state, proving assertion callers must
    /// drop clones instead of explicitly closing them.
    #[tokio::test]
    async fn assertion_pool_clone_close_is_shared() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let first = fixture
            .superuser_pool()
            .expect("first assertion pool clone");
        let second = fixture
            .superuser_pool()
            .expect("second assertion pool clone");

        let (current_user, bypasses_rls): (String, bool) = sqlx::query_as(
            "SELECT current_user, rolbypassrls FROM pg_roles WHERE rolname = current_user",
        )
        .fetch_one(&first)
        .await
        .expect("first clone performs table-owner assertion");
        assert_eq!(current_user, "wyrd_test_admin");
        assert!(bypasses_rls);
        let observed: i32 = sqlx::query_scalar("SELECT 1")
            .fetch_one(&second)
            .await
            .expect("second clone performs assertion");
        assert_eq!(observed, 1);

        first.close().await;
        assert!(second.is_closed());
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
             WHERE rolname IN ('wyrd_app', 'wyrd_platform_admin')
             ORDER BY rolname",
        )
        .fetch_all(fixture.operator_pool().pool())
        .await
        .expect("role metadata query succeeds");

        assert_eq!(
            rows,
            vec![
                ("wyrd_app".to_owned(), false),
                ("wyrd_platform_admin".to_owned(), true),
            ]
        );
    }

    async fn assert_single_tenant(fixture: &PgFixture) {
        // vala_sql's olap_minimal migration seeds the reserved system-owner
        // system tenant, so the fixture's own seeded tenant is the
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
