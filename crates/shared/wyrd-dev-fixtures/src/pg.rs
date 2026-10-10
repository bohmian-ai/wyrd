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

/// Login that owns every test database and migrates it, as a deployment's
/// platform login does. `with-test-postgres.sh` creates it.
const PLATFORM_LOGIN: &str = "wyrd_platform";
/// Per-database setup a DBA performs for a two-login deployment.
const DATABASE_SETUP: &str = include_str!("../../../../scripts/postgres/test-database-setup.sql");

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
    /// Shared cluster-administrator pool on this database, retained for raw
    /// assertion probes that row-level security and grants must not limit.
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
        let assertion_pool = test_db.superuser_pool().await?;
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

    /// Borrow the RLS-scoped tenant pool.
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
    /// can prove RLS scoping. The row is written through the platform
    /// operator pool, matching the boot seed path.
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

    /// Clone the fixture-owned pool connected to this database as the cluster
    /// superuser.
    ///
    /// Use this pool in test assertions that need to read or change state
    /// across all tenants without RLS or grants in the way. Postgres exempts a
    /// superuser from every policy, which is why serving boot refuses one;
    /// serving code never holds this pool.
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
        let assertion_pool = test_db.superuser_pool().await?;
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
    /// Platform-login operator pool on the empty database. Precedes `_test_db`
    /// so it drops before the database is removed.
    migrator: PgPool,
    /// Database owner whose drop removes the isolated database.
    _test_db: TestDatabase,
}

impl UnmigratedDatabase {
    /// Create an empty isolated database and connect the platform login to it.
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

    /// The platform-login operator pool, which owns DDL on this database.
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
    /// Creates an isolated database and applies migrations through the
    /// platform login, exactly as `wyrd-server migrate` does for a deployment.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the admin DSN is missing or invalid, the
    /// database cannot be created, or migrations fail.
    async fn create() -> Result<Self, SqlError> {
        let test_db = Self::create_empty().await?;
        test_db.migrate().await?;
        Ok(test_db)
    }

    /// Creates an isolated database owned by [`PLATFORM_LOGIN`] and performs
    /// the DBA's per-database setup ([`DATABASE_SETUP`]) without applying
    /// migrations.
    ///
    /// # Errors
    /// Returns [`SqlError`] when database creation or setup fails.
    async fn create_empty() -> Result<Self, SqlError> {
        let admin_dsn = test_database_admin_dsn("wyrd")?;
        let name = unique_database_name();
        let admin_pool = build_pool(admin_dsn.expose_secret(), PoolConfig::migrator_defaults())
            .await
            .map_err(SqlError::Connect)?;
        let created = sqlx::query(AssertSqlSafe(format!(
            "CREATE DATABASE {name} OWNER {PLATFORM_LOGIN}"
        )))
        .execute(&admin_pool)
        .await
        .map_err(SqlError::from);
        admin_pool.close().await;
        created?;

        let test_db = Self {
            name,
            admin_dsn,
            owned: true,
        };
        let superuser = test_db.superuser_pool().await?;
        let setup = sqlx::raw_sql(DATABASE_SETUP)
            .execute(&superuser)
            .await
            .map_err(SqlError::from);
        superuser.close().await;
        setup?;
        Ok(test_db)
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

    /// Apply Wyrd then Vala migrations through a transient platform-login
    /// pool, under one migration lease as `wyrd-server migrate` does.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the platform pool cannot connect, the lease
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

    /// Open an operator pool on this fixture database as the platform login,
    /// which owns it and every migrated object.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the serving DSNs cannot be resolved or the
    /// pool cannot connect.
    async fn owner_pool(&self) -> Result<PgPool, SqlError> {
        build_pool(
            self.resolved_dsns()?.platform().expose_secret(),
            PoolConfig::migrator_defaults(),
        )
        .await
        .map_err(SqlError::Connect)
    }

    /// Open a pool on this fixture database as the cluster superuser.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the admin DSN cannot be rewritten or the pool
    /// cannot connect.
    async fn superuser_pool(&self) -> Result<PgPool, SqlError> {
        let dsn = database_dsn(&self.admin_dsn, &self.name)?;
        build_pool(dsn.expose_secret(), PoolConfig::migrator_defaults())
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
            platform_admin: base
                .platform_admin
                .as_ref()
                .map(|dsn| database_dsn(dsn, &self.name))
                .transpose()?,
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

    use super::{PLATFORM_LOGIN, PgFixture, database_dsn};
    use wyrd_spec::DataTenantId;
    use wyrd_sql::PoolConfig;
    use wyrd_sql::SqlError;
    use wyrd_sql::pool::build_pool;
    use wyrd_sql::tenant_conn::CURRENT_TENANT_GUC;

    /// A real fixture migrates, binds tenants, and preserves the database
    /// lifecycle boundary between the DBA and the serving logins.
    #[tokio::test]
    async fn fixture_smoke() {
        let fixture = PgFixture::start().await.expect("fixture starts");

        assert_required_schemas(&fixture).await;
        assert_single_tenant(&fixture).await;
        assert_seeded_tenant(&fixture, fixture.tenant_slug()).await;
        assert_current_tenant_bound(&fixture).await;
        assert_bare_app_pool_fails_loudly(&fixture).await;
        assert_database_authority_boundary(&fixture).await;
    }

    /// The platform login owns the database and every migrated schema, while
    /// neither serving login can create databases and the tenant login cannot
    /// drop one.
    async fn assert_database_authority_boundary(fixture: &PgFixture) {
        let superuser = fixture.superuser_pool().expect("superuser pool");
        let owner: String = sqlx::query_scalar(
            "SELECT owner.rolname FROM pg_database database \
             JOIN pg_roles owner ON owner.oid=database.datdba WHERE database.datname=$1",
        )
        .bind(&fixture.test_db.name)
        .fetch_one(&superuser)
        .await
        .expect("database owner reads");
        assert_eq!(owner, PLATFORM_LOGIN);
        let schema_owners: Vec<(String, String)> = sqlx::query_as(
            "SELECT namespace.nspname, owner.rolname FROM pg_namespace namespace \
             JOIN pg_roles owner ON owner.oid=namespace.nspowner \
             WHERE namespace.nspname IN ('iceberg_catalog','platform','wyrd','vala') \
             ORDER BY namespace.nspname",
        )
        .fetch_all(&superuser)
        .await
        .expect("migration schema owners read");
        assert_eq!(
            schema_owners,
            ["iceberg_catalog", "platform", "vala", "wyrd"]
                .map(|schema| (schema.to_owned(), PLATFORM_LOGIN.to_owned()))
                .to_vec()
        );

        let serving = fixture
            .test_db
            .resolved_dsns()
            .expect("fixture DSNs resolve");
        for (login, statements) in [
            (
                &serving.app,
                &[
                    "CREATE DATABASE wyrd_forbidden_create",
                    "DROP DATABASE wyrd",
                ][..],
            ),
            (
                serving.platform_login(),
                &["CREATE DATABASE wyrd_forbidden_create"][..],
            ),
        ] {
            let maintenance_dsn = database_dsn(login, "postgres").expect("serving DSN rewrites");
            let pool = build_pool(
                maintenance_dsn.expose_secret(),
                PoolConfig::migrator_defaults(),
            )
            .await
            .expect("serving login connects to maintenance database");
            for statement in statements {
                let error = sqlx::query(AssertSqlSafe(*statement))
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
    /// missing, or whose tables lost forced row-level security or their exact
    /// policies, and a tenant login widened past row-level security; once
    /// restored, readiness and ordinary tenant and platform work pass again.
    #[tokio::test]
    async fn validate_schema_refuses_drifted_or_unprotected_schemas() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let superuser = fixture.superuser_pool().expect("superuser pool");
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

        for (break_sql, restore_sql, refusal) in [
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
                "ALTER POLICY operator_access ON platform.tenants TO PUBLIC",
                "ALTER POLICY operator_access ON platform.tenants TO wyrd_platform",
                "tenants",
            ),
            (
                "CREATE POLICY widened ON wyrd.verifier_runs USING (true)",
                "DROP POLICY widened ON wyrd.verifier_runs",
                "verifier_runs",
            ),
            (
                "GRANT TRUNCATE ON wyrd.cards TO wyrd_tenant",
                "REVOKE TRUNCATE ON wyrd.cards FROM wyrd_tenant",
                "holds TRUNCATE",
            ),
        ] {
            sqlx::query(AssertSqlSafe(break_sql))
                .execute(&superuser)
                .await
                .expect("superuser breaks tenant isolation");
            assert!(
                matches!(
                    fixture.wyrd_postgres().validate_schema().await,
                    Err(SqlError::SchemaNotReady { ref detail }) if detail.contains(refusal)
                ),
                "{break_sql}"
            );
            sqlx::query(AssertSqlSafe(restore_sql))
                .execute(&superuser)
                .await
                .expect("superuser restores tenant isolation");
        }

        sqlx::raw_sql(
            "CREATE TABLE iceberg_catalog.exposed_probe (id int); \
             GRANT SELECT ON iceberg_catalog.exposed_probe TO wyrd_tenant",
        )
        .execute(&superuser)
        .await
        .expect("superuser exposes a catalog table");
        assert!(matches!(
            fixture.vala_postgres().validate_schema(operator).await,
            Err(SqlError::SchemaNotReady { ref detail }) if detail.contains("iceberg_catalog")
        ));
        sqlx::query("DROP TABLE iceberg_catalog.exposed_probe")
            .execute(&superuser)
            .await
            .expect("superuser removes the exposed table");

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
        .execute(&superuser)
        .await
        .expect("superuser corrupts a Wyrd checksum");
        assert!(matches!(
            fixture.wyrd_postgres().validate_schema().await,
            Err(SqlError::MigrateChecksum { .. })
        ));

        sqlx::query(
            "DELETE FROM vala._sqlx_migrations \
             WHERE version = (SELECT max(version) FROM vala._sqlx_migrations)",
        )
        .execute(&superuser)
        .await
        .expect("superuser removes a Vala ledger row");
        assert!(matches!(
            fixture.vala_postgres().validate_schema(operator).await,
            Err(SqlError::SchemaNotReady { ref detail }) if detail.contains("is not applied")
        ));
    }

    /// Serving readiness refuses two-login deployments whose logins would
    /// defeat row-level security: a `BYPASSRLS` tenant login, a tenant login
    /// that is a member of the owner, and a platform login that does not own
    /// Wyrd's objects.
    #[tokio::test]
    async fn validate_schema_refuses_logins_that_defeat_row_security() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let superuser = fixture.superuser_pool().expect("superuser pool");
        let suffix = fixture.test_db.name.clone();
        let bypass = format!("bypass_{suffix}");
        let member = format!("member_{suffix}");
        for statement in [
            format!("CREATE ROLE {bypass} LOGIN BYPASSRLS PASSWORD 'probe'"),
            format!("CREATE ROLE {member} LOGIN PASSWORD 'probe' IN ROLE {PLATFORM_LOGIN}"),
        ] {
            sqlx::query(AssertSqlSafe(statement))
                .execute(&superuser)
                .await
                .expect("superuser creates a probe login");
        }

        let dsns = fixture.test_db.resolved_dsns().expect("serving DSNs");
        let platform = dsns.platform_login().clone();
        for (dsns, refusal) in [
            (
                ResolvedDsns {
                    app: login_as(&dsns.app, &bypass),
                    platform_admin: Some(platform.clone()),
                },
                "BYPASSRLS",
            ),
            (
                ResolvedDsns {
                    app: login_as(&dsns.app, &member),
                    platform_admin: Some(platform.clone()),
                },
                "owns",
            ),
            (
                ResolvedDsns {
                    app: dsns.app.clone(),
                    platform_admin: Some(dsns.app.clone()),
                },
                "does not own",
            ),
        ] {
            let wyrd = WyrdPostgres::connect_from_dsns(&dsns)
                .await
                .expect("probe pools build");
            assert!(
                matches!(
                    wyrd.validate_schema().await,
                    Err(SqlError::SchemaNotReady { ref detail }) if detail.contains(refusal)
                ),
                "{refusal} login passed readiness"
            );
            let vala = ValaPostgres::connect_from_dsns(&dsns)
                .await
                .expect("probe Vala pool builds");
            if refusal != "does not own" {
                assert!(matches!(
                    vala.validate_schema(fixture.operator_pool()).await,
                    Err(SqlError::SchemaNotReady { ref detail }) if detail.contains(refusal)
                ));
            }
        }

        for role in [&bypass, &member] {
            sqlx::query(AssertSqlSafe(format!("DROP ROLE {role}")))
                .execute(&superuser)
                .await
                .expect("superuser drops the probe login");
        }
        fixture
            .wyrd_postgres()
            .validate_schema()
            .await
            .expect("the two serving logins still validate");
    }

    /// With no platform URL, Wyrd and Vala readiness accept the one ordinary
    /// login that owns Wyrd's objects, and refuse a superuser and a login that
    /// does not own them.
    #[tokio::test]
    async fn validate_schema_shared_login_posture() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let superuser = database_dsn(&fixture.test_db.admin_dsn, &fixture.test_db.name)
            .expect("fixture superuser DSN rewrites");
        let dsns = fixture.test_db.resolved_dsns().expect("serving DSNs");
        for (login, refusal) in [
            (dsns.platform_login().clone(), None),
            (superuser, Some("superuser")),
            (dsns.app.clone(), Some("does not own")),
        ] {
            let dsns = ResolvedDsns {
                app: login,
                platform_admin: None,
            };
            let wyrd = WyrdPostgres::connect_from_dsns(&dsns)
                .await
                .expect("shared-login pools build");
            let result = wyrd.validate_schema().await;
            match refusal {
                None => {
                    result.expect("the owning login serves alone");
                    let vala = ValaPostgres::connect_from_dsns(&dsns)
                        .await
                        .expect("shared-login Vala pool builds");
                    let operator = wyrd.operator_pool().expect("platform pool exists");
                    vala.validate_schema(&operator)
                        .await
                        .expect("the owning login serves Vala alone");
                }
                Some(refusal) => assert!(
                    matches!(
                        result,
                        Err(SqlError::SchemaNotReady { ref detail }) if detail.contains(refusal)
                    ),
                    "accepted a shared {refusal} login"
                ),
            }
        }
    }

    /// Rewrites a serving DSN to log in as `role` with the probe password.
    fn login_as(dsn: &SecretString, role: &str) -> SecretString {
        let mut url = url::Url::parse(dsn.expose_secret()).expect("serving DSN parses");
        url.set_username(role).expect("DSN accepts a username");
        url.set_password(Some("probe"))
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

    /// Repeated superuser probes reuse one fixture pool without exhausting
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
