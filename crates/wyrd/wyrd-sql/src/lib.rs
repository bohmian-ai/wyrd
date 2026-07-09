//! Server-tier SQL scaffold for Wyrd control-plane storage.
//!
//! `wyrd-sql` owns the `platform` and `wyrd` PostgreSQL schemas. `platform`
//! holds state above the tenant boundary, including the future tenant catalog.
//! `wyrd` holds tenant-scoped control-plane state. Vala owns its own `vala`
//! schema in `vala-sql`; no `skald` schema exists in this phase.

#![deny(missing_docs)]

use sqlx::postgres::{PgConnection, PgPool, PgPoolOptions};

pub mod dsn;
pub mod error;
pub mod pool;
pub mod postgres;
#[cfg(feature = "embedded-postgres")]
pub mod postgres_boot;
pub mod queries;
pub mod query;
pub mod row_types;
pub mod tenant_conn;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use error::SqlError;
pub use pool::PoolConfig;
pub use postgres::WyrdPostgres;
pub use row_types::cards::{
    AuditCardRegistrationRow, CardRegistrationOperation, CardRegistrationOutcome, CardRow,
    CardStatus, NewAuditCardRegistrationRow, NewCardRow, ParsedCardRow,
};
pub use tenant_conn::TenantConn;

/// Platform-global schema owned by `wyrd-sql`.
pub const PLATFORM_SCHEMA: &str = "platform";
/// Tenant-scoped Wyrd control-plane schema owned by `wyrd-sql`.
pub const CONTROL_SCHEMA: &str = "wyrd";
/// Schemas whose migration lifecycle is owned by `wyrd-sql`.
pub const OWNED_SCHEMAS: &[&str] = &[PLATFORM_SCHEMA, CONTROL_SCHEMA];
/// Search path used only by the boot migrator connection.
pub const MIGRATION_SEARCH_PATH: &str = "wyrd, platform, public";
const MIGRATION_ADVISORY_LOCK_KEY: i64 = 0x0057_5952_4453_514c;

/// Apply embedded Wyrd SQL migrations against a boot-only migrator pool.
///
/// The supplied pool **must** authenticate as `wyrd_migrator`. Migration runs on
/// one dedicated connection with `search_path` set to `wyrd, platform, public`,
/// then the physical connection is closed so session state cannot return to the
/// pool. Calling with an `wyrd_app`-role DSN will fail on the bootstrap DDL and
/// return [`SqlError::InsufficientPrivilege`].
///
/// # Errors
/// Returns [`SqlError::Connect`] when the connection itself fails.
/// Returns [`SqlError::InsufficientPrivilege`] when the role lacks DDL privileges.
/// Returns [`SqlError::Migrate`] when migration execution fails.
pub async fn migrate(migrator_pool: &PgPool) -> Result<(), SqlError> {
    let mut conn = migrator_pool.acquire().await.map_err(SqlError::Connect)?;
    acquire_migration_advisory_lock(&mut conn).await?;

    let result: Result<(), SqlError> = async {
        sqlx::query("CREATE SCHEMA IF NOT EXISTS platform")
            .execute(&mut *conn)
            .await
            .map_err(classify_bootstrap_error)?;
        sqlx::query("CREATE SCHEMA IF NOT EXISTS wyrd")
            .execute(&mut *conn)
            .await
            .map_err(classify_bootstrap_error)?;
        sqlx::query("SET search_path TO wyrd, platform, public")
            .execute(&mut *conn)
            .await
            .map_err(classify_bootstrap_error)?;
        sqlx::migrate!("./migrations")
            .run(&mut *conn)
            .await
            .map_err(SqlError::from)
    }
    .await;

    let unlock_result = release_migration_advisory_lock(&mut conn).await;
    if let Err(error) = unlock_result {
        if result.is_ok() {
            return Err(error);
        }
        tracing::warn!(
            error = %error,
            "failed to release wyrd-sql migration advisory lock after migration error"
        );
    }

    if let Err(error) = conn.close().await {
        tracing::warn!(
            error = %error,
            "failed to close wyrd-sql migration connection cleanly"
        );
    }

    result
}

/// Acquire the Wyrd SQL migration advisory lock on the current session.
///
/// # Errors
/// Returns [`SqlError::Connect`] when Postgres cannot acquire the lock.
async fn acquire_migration_advisory_lock(conn: &mut PgConnection) -> Result<(), SqlError> {
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(MIGRATION_ADVISORY_LOCK_KEY)
        .execute(&mut *conn)
        .await
        .map_err(SqlError::Connect)?;
    Ok(())
}

/// Release the Wyrd SQL migration advisory lock on the current session.
///
/// # Errors
/// Returns [`SqlError::Connect`] when Postgres cannot release the lock.
async fn release_migration_advisory_lock(conn: &mut PgConnection) -> Result<(), SqlError> {
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(MIGRATION_ADVISORY_LOCK_KEY)
        .execute(&mut *conn)
        .await
        .map_err(SqlError::Connect)?;
    Ok(())
}

fn classify_bootstrap_error(error: sqlx::Error) -> SqlError {
    if let sqlx::Error::Database(ref dbe) = error
        && dbe.code().as_deref() == Some("42501")
    {
        return SqlError::InsufficientPrivilege {
            detail: dbe.message().to_owned(),
        };
    }
    SqlError::Connect(error)
}

/// Control-plane Postgres handle.
///
/// The store is cloneable because it wraps an internal connection pool.
#[derive(Clone)]
pub struct SqlStore {
    pool: PgPool,
}

impl SqlStore {
    /// Connect to Postgres with a bounded pool.
    ///
    /// This does not run migrations.
    ///
    /// # Errors
    /// Returns [`SqlError::Connect`] when the database connection fails.
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, SqlError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await
            .map_err(SqlError::Connect)?;
        Ok(Self { pool })
    }

    /// Connect to Postgres with role-specific pool configuration.
    ///
    /// This does not run migrations.
    ///
    /// # Errors
    /// Returns [`SqlError::Connect`] when the DSN cannot be parsed or the
    /// database connection fails.
    pub async fn connect_with(database_url: &str, config: PoolConfig) -> Result<Self, SqlError> {
        let pool = pool::connect_pool(database_url, config)
            .await
            .map_err(SqlError::Connect)?;
        Ok(Self { pool })
    }

    /// Apply embedded SQL migrations.
    ///
    /// # Errors
    /// Returns [`SqlError::Migrate`] when migration execution fails.
    pub async fn migrate(&self) -> Result<(), SqlError> {
        migrate(&self.pool).await
    }

    /// Borrow the underlying Postgres pool.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use crate::{MIGRATION_SEARCH_PATH, OWNED_SCHEMAS};

    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn schema_ownership_is_explicit() {
        assert_eq!(OWNED_SCHEMAS, &["platform", "wyrd"]);
        assert_eq!(MIGRATION_SEARCH_PATH, "wyrd, platform, public");
        assert!(!OWNED_SCHEMAS.contains(&"vala"));
        assert!(!OWNED_SCHEMAS.contains(&"skald"));
    }

    #[test]
    fn migrations_embed_count_matches_files() {
        let migrator = sqlx::migrate!("./migrations");
        let files = migration_files();

        assert_eq!(migrator.migrations.len(), files.len());
        assert!(
            !migrator.migrations.is_empty(),
            "wyrd-sql must embed at least one migration"
        );
    }

    #[test]
    fn create_table_statements_stay_in_owned_schemas() {
        let unexpected = migration_files()
            .into_iter()
            .flat_map(|file_name| {
                let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("migrations")
                    .join(file_name);
                fs::read_to_string(path)
                    .expect("migration sql is readable")
                    .lines()
                    .map(str::trim_start)
                    .filter(|line| line.starts_with("CREATE TABLE "))
                    .filter(|line| {
                        !line.starts_with("CREATE TABLE wyrd.")
                            && !line.starts_with("CREATE TABLE platform.")
                    })
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        assert!(
            unexpected.is_empty(),
            "CREATE TABLE statements must target wyrd.* or platform.*: {unexpected:?}"
        );
    }

    #[test]
    fn migration_filenames_match_timestamp_versions() {
        let migrator = sqlx::migrate!("./migrations");
        let files = migration_files();

        let mut prev_version = 0i64;
        for (migration, file_name) in migrator.migrations.iter().zip(&files) {
            let version = migration.version;
            let prefix = format!("{version}_");

            assert!(
                version > prev_version,
                "migration version {version} must be greater than previous {prev_version}"
            );
            prev_version = version;

            assert!(
                file_name.starts_with(&prefix),
                "migration file {file_name} must start with its version {prefix}"
            );
            assert!(
                file_name.ends_with(".sql"),
                "migration file {file_name} must be forward-only .sql"
            );
            assert!(
                !file_name.ends_with(".up.sql") && !file_name.ends_with(".down.sql"),
                "migration file {file_name} must not use reversible suffixes"
            );
            assert!(
                file_name
                    .trim_end_matches(".sql")
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'),
                "migration file {file_name} must use snake_case"
            );
        }
    }

    #[test]
    fn queries_module_shape_matches_foundation_plan() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let required_paths = [
            "src/queries/mod.rs",
            "src/queries/auth/mod.rs",
            "src/queries/auth/users.rs",
            "src/queries/auth/roles.rs",
            "src/queries/auth/role_assignments.rs",
            "src/queries/auth/api_keys.rs",
            "src/queries/auth/refresh_tokens.rs",
            "src/queries/auth/login_state.rs",
            "src/queries/auth/user_identities.rs",
            "src/queries/auth/sql",
            "src/queries/platform/mod.rs",
            "src/queries/platform/tenant_resolver.rs",
            "src/queries/platform/tenants.rs",
            "src/queries/platform/users.rs",
            "src/queries/platform/roles.rs",
            "src/queries/platform/api_keys.rs",
            "src/queries/platform/audit_log.rs",
            "src/queries/storage/mod.rs",
            "src/queries/storage/multipart_uploads.rs",
            "src/queries/storage/artifact_metadata.rs",
            "src/queries/storage/access_ledger.rs",
            "src/queries/storage/idempotency.rs",
            "src/queries/storage/admin/mod.rs",
            "src/queries/storage/admin/multipart_uploads.rs",
            "src/queries/storage/admin/idempotency.rs",
            "src/queries/platform/sql",
            "src/row_types/mod.rs",
            "src/row_types/auth/mod.rs",
            "src/row_types/platform/mod.rs",
        ];

        for relative_path in required_paths {
            assert!(
                crate_dir.join(relative_path).exists(),
                "missing planned wyrd-sql path: {relative_path}"
            );
        }

        let auth_doc = fs::read_to_string(crate_dir.join("src/queries/auth/mod.rs"))
            .expect("auth query module doc is readable");
        assert!(auth_doc.contains("&mut TenantConn<'_>"));
        assert!(auth_doc.contains("data_tenant_id = $"));

        let resolver =
            fs::read_to_string(crate_dir.join("src/queries/platform/tenant_resolver.rs"))
                .expect("tenant resolver module is readable");
        assert!(resolver.contains("resolve_by_slug_for_app"));
        assert!(resolver.contains("&TenantSlug"));
        assert!(resolver.contains("platform.resolve_tenant_by_slug($1)"));
        assert!(resolver.contains("raw-query grep"));
    }

    #[test]
    fn tenant_scoped_query_stubs_do_not_take_raw_pool_executors() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let forbidden = rust_files_under(&crate_dir.join("src/queries/auth"))
            .into_iter()
            .filter_map(|path| {
                let body = fs::read_to_string(&path).expect("auth query file is readable");
                let checked = without_line_comments(&body);
                (checked.contains("&PgPool")
                    || checked.contains("PgPool,")
                    || checked.contains("Transaction<'_")
                    || checked.contains("Transaction < '_")
                    || checked.contains(".begin("))
                .then_some(path)
            })
            .collect::<Vec<_>>();

        assert!(
            forbidden.is_empty(),
            "tenant-scoped auth query modules must use TenantConn, not raw executors: {forbidden:?}"
        );
    }

    #[test]
    fn tenant_scoped_query_modules_do_not_issue_transaction_control_sql() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let forbidden = rust_files_under(&crate_dir.join("src/queries/auth"))
            .into_iter()
            .chain(sql_files_under(&crate_dir.join("src/queries/auth/sql")))
            .filter_map(|path| {
                let body = fs::read_to_string(&path).expect("query source is readable");
                let uncommented = without_line_comments(&body);
                let checked = production_source(&uncommented).to_ascii_uppercase();
                let has_transaction_control = ["BEGIN", "COMMIT", "ROLLBACK", "SAVEPOINT"]
                    .into_iter()
                    .any(|keyword| checked.contains(keyword));
                has_transaction_control.then_some(path)
            })
            .collect::<Vec<_>>();

        assert!(
            forbidden.is_empty(),
            "tenant-scoped query modules must rely on TenantConn, not raw transaction SQL: {forbidden:?}"
        );
    }

    #[test]
    fn wyrd_sql_does_not_coordinate_transactions_with_vala_sql() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let manifest =
            fs::read_to_string(crate_dir.join("Cargo.toml")).expect("manifest is readable");
        assert!(
            !manifest.contains("vala-sql") && !manifest.contains("vala_sql"),
            "wyrd-sql must not depend on vala-sql for cross-crate transactions"
        );

        let forbidden = rust_files_under(&crate_dir.join("src"))
            .into_iter()
            .filter_map(|path| {
                let body = fs::read_to_string(&path).expect("source file is readable");
                let uncommented = without_line_comments(&body);
                let checked = production_source(&uncommented);
                (checked.contains("vala_sql::queries") || checked.contains("vala_sql :: queries"))
                    .then_some(path)
            })
            .collect::<Vec<_>>();

        assert!(
            forbidden.is_empty(),
            "wyrd-sql must not call vala-sql query modules: {forbidden:?}"
        );
    }

    #[test]
    fn transaction_discipline_is_documented() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let repo_dir = crate_dir
            .ancestors()
            .nth(3)
            .expect("crate lives three levels below repo root");
        let sql_foundation =
            fs::read_to_string(repo_dir.join("architecture/v1/00-foundations/sql-foundation.md"))
                .expect("SQL foundation architecture doc is readable");
        let tenant_conn = fs::read_to_string(crate_dir.join("src/tenant_conn.rs"))
            .expect("TenantConn is readable");
        let queries_doc = fs::read_to_string(crate_dir.join("src/queries/mod.rs"))
            .expect("query module doc is readable");

        assert!(sql_foundation.contains("Every tenant-scoped logical operation opens exactly one"));
        assert!(sql_foundation.contains("Cross-crate transactional coordination is not supported"));
        assert!(tenant_conn.contains("transaction boundary for one tenant-scoped logical"));
        assert!(tenant_conn.contains("operation. Handlers and workers"));
        assert!(queries_doc.contains("future outbox path"));
    }

    #[test]
    fn storage_migrations_preserve_foundation_locks() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let migrations = migration_files()
            .into_iter()
            .filter(|file_name| file_name.contains("storage"))
            .map(|file_name| {
                fs::read_to_string(crate_dir.join("migrations").join(file_name))
                    .expect("storage migration is readable")
            })
            .collect::<Vec<_>>()
            .join("\n");

        for table in [
            "wyrd.storage_multipart_uploads",
            "wyrd.storage_artifact_metadata",
            "wyrd.storage_access_ledger",
            "wyrd.storage_idempotency_keys",
        ] {
            assert!(
                migrations.contains(table),
                "storage migration must create or configure {table}"
            );
        }

        let forbidden_parts_table = ["storage_multipart_upload", "_parts"].concat();
        assert!(
            !migrations.contains(&forbidden_parts_table),
            "storage foundation must not add a per-part uploads table"
        );
        assert!(
            migrations.contains("ENABLE ROW LEVEL SECURITY")
                && migrations.contains("FORCE ROW LEVEL SECURITY"),
            "storage tables must enable and force RLS"
        );
        assert!(
            migrations.contains("backend_upload_id")
                && migrations.contains("expected_size_bytes")
                && migrations.contains("block_count_planned")
                && migrations.contains("wire_protocol"),
            "multipart rows must persist restart-safe non-bearer completion state"
        );
    }

    #[test]
    fn storage_migrations_revoke_inherited_broad_privileges() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let storage = fs::read_to_string(
            crate_dir
                .join("migrations")
                .join("20260601000002_storage.sql"),
        )
        .expect("storage migration is readable");
        let idempotency = fs::read_to_string(
            crate_dir
                .join("migrations")
                .join("20260601000003_storage_idempotency.sql"),
        )
        .expect("storage idempotency migration is readable");
        let migrations = format!("{storage}\n{idempotency}");

        for (table, app_grant, admin_grant) in [
            (
                "wyrd.storage_multipart_uploads",
                "GRANT SELECT, INSERT, UPDATE ON wyrd.storage_multipart_uploads TO wyrd_app;",
                "GRANT SELECT, UPDATE ON wyrd.storage_multipart_uploads TO wyrd_platform_admin;",
            ),
            (
                "wyrd.storage_artifact_metadata",
                "GRANT SELECT, INSERT, UPDATE ON wyrd.storage_artifact_metadata TO wyrd_app;",
                "GRANT SELECT ON wyrd.storage_artifact_metadata TO wyrd_platform_admin;",
            ),
            (
                "wyrd.storage_access_ledger",
                "GRANT SELECT, INSERT ON wyrd.storage_access_ledger TO wyrd_app;",
                "GRANT SELECT, INSERT ON wyrd.storage_access_ledger TO wyrd_platform_admin;",
            ),
            (
                "wyrd.storage_idempotency_keys",
                "GRANT SELECT, INSERT ON wyrd.storage_idempotency_keys TO wyrd_app;",
                "GRANT SELECT, DELETE ON wyrd.storage_idempotency_keys TO wyrd_platform_admin;",
            ),
        ] {
            assert!(
                migrations.contains(&format!("REVOKE ALL ON TABLE {table} FROM wyrd_app;")),
                "{table} must revoke inherited app table privileges"
            );
            assert!(
                migrations.contains(&format!(
                    "REVOKE ALL ON TABLE {table} FROM wyrd_platform_admin;"
                )),
                "{table} must revoke inherited platform-admin table privileges"
            );
            assert!(
                migrations.contains(app_grant),
                "{table} must grant the exact intended app privileges"
            );
            assert!(
                migrations.contains(admin_grant),
                "{table} must grant the exact intended platform-admin privileges"
            );
        }

        for forbidden in [
            "GRANT SELECT, INSERT, UPDATE, DELETE ON wyrd.storage_multipart_uploads",
            "GRANT SELECT, INSERT, UPDATE, DELETE ON wyrd.storage_artifact_metadata",
            "GRANT SELECT, INSERT, DELETE ON wyrd.storage_idempotency_keys TO wyrd_app",
        ] {
            assert!(
                !migrations.contains(forbidden),
                "storage migrations must not leave broad grant shape: {forbidden}"
            );
        }

        assert!(
            migrations.contains(
                "REVOKE ALL ON SEQUENCE wyrd.storage_access_ledger_id_seq FROM wyrd_app;"
            ),
            "ledger sequence must revoke inherited app privileges before narrow grant"
        );
        assert!(
            migrations.contains(
                "REVOKE ALL ON SEQUENCE wyrd.storage_access_ledger_id_seq FROM wyrd_platform_admin;"
            ),
            "ledger sequence must revoke inherited platform-admin privileges before narrow grant"
        );
        assert!(
            migrations.contains(
                "GRANT USAGE, SELECT ON SEQUENCE wyrd.storage_access_ledger_id_seq TO wyrd_app;"
            ) && migrations.contains(
                "GRANT USAGE, SELECT ON SEQUENCE wyrd.storage_access_ledger_id_seq TO wyrd_platform_admin;"
            ),
            "ledger inserts require narrow sequence usage grants"
        );
    }

    #[test]
    fn storage_query_boundaries_are_explicit() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let tenant_modules = [
            "multipart_uploads.rs",
            "artifact_metadata.rs",
            "access_ledger.rs",
            "idempotency.rs",
        ];

        for file_name in tenant_modules {
            let path = crate_dir.join("src/queries/storage").join(file_name);
            let body = fs::read_to_string(&path).expect("storage tenant query is readable");
            let checked = without_line_comments(&body);

            assert!(
                checked.contains("&mut TenantConn<'_>"),
                "{file_name} must expose tenant-scoped functions through TenantConn"
            );
            assert!(
                !checked.contains("&PgPool")
                    && !checked.contains("PgPool,")
                    && !checked.contains(".begin("),
                "{file_name} must not take raw pools or open transactions"
            );
        }

        let admin_dir = crate_dir.join("src/queries/storage/admin");
        let admin_body = rust_files_under(&admin_dir)
            .into_iter()
            .map(|path| fs::read_to_string(path).expect("storage admin query is readable"))
            .collect::<Vec<_>>()
            .join("\n");
        let admin_checked = without_line_comments(&admin_body);

        assert!(
            admin_checked.contains("&PgPool"),
            "storage admin queries must make their cross-tenant pool boundary explicit"
        );
        assert!(
            !admin_checked.contains("TenantConn"),
            "storage admin query implementations must not pretend to be tenant-scoped"
        );
    }

    fn rust_files_under(dir: &Path) -> Vec<PathBuf> {
        files_under_with_extension(dir, "rs")
    }

    fn sql_files_under(dir: &Path) -> Vec<PathBuf> {
        files_under_with_extension(dir, "sql")
    }

    fn files_under_with_extension(dir: &Path, extension: &str) -> Vec<PathBuf> {
        let mut files = Vec::new();
        collect_files_with_extension(dir, extension, &mut files);
        files.sort();
        files
    }

    fn collect_files_with_extension(dir: &Path, extension: &str, files: &mut Vec<PathBuf>) {
        if !dir.exists() {
            return;
        }

        for entry in fs::read_dir(dir).expect("source directory is readable") {
            let path = entry.expect("source directory entry is readable").path();
            if path.is_dir() {
                collect_files_with_extension(&path, extension, files);
            } else if path
                .extension()
                .is_some_and(|actual_extension| actual_extension == extension)
            {
                files.push(path);
            }
        }
    }

    fn without_line_comments(source: &str) -> String {
        source
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !trimmed.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn production_source(source: &str) -> &str {
        source.split("\n#[cfg(test)]").next().unwrap_or(source)
    }

    fn migration_files() -> Vec<String> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut files = fs::read_dir(&dir)
            .expect("migrations directory is readable")
            .map(|entry| {
                entry
                    .expect("migration directory entry is readable")
                    .file_name()
                    .into_string()
                    .expect("migration filename is utf-8")
            })
            .filter(|file_name| file_name.ends_with(".sql"))
            .collect::<Vec<_>>();
        files.sort();
        files
    }
}
