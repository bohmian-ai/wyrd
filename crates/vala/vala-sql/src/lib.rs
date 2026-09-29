//! Server-tier SQL migrations for Vala observability storage.
//!
//! `vala-sql` owns the `vala` PostgreSQL schema for observability control-plane
//! state. Wyrd control-plane and platform schemas remain owned by `wyrd-sql`;
//! no `skald` schema exists in this phase. Vala tenant-scoped work uses the
//! shared [`TenantConn`] wrapper for one Vala logical operation at a time, but
//! it does not extend Wyrd write transactions or call Wyrd query modules for
//! cross-crate transactional coordination. Downstream effects from committed
//! Wyrd state are a future outbox/event fanout concern.

#![deny(missing_docs)]

use sqlx::{AssertSqlSafe, PgConnection, PgPool};

pub mod postgres;
pub mod queries;
pub mod row_types;

pub use postgres::ValaPostgres;
pub use wyrd_sql::{OperatorPool, TenantConn, error::SqlError};

/// Tenant-scoped Vala observability schema owned by `vala-sql`.
pub const OBSERVABILITY_SCHEMA: &str = "vala";
/// Iceberg JDBC catalog schema owned by `vala-sql` migrations.
pub const ICEBERG_CATALOG_SCHEMA: &str = "iceberg_catalog";
/// Schemas whose migration lifecycle is owned by `vala-sql`.
pub const OWNED_SCHEMAS: &[&str] = &[OBSERVABILITY_SCHEMA, ICEBERG_CATALOG_SCHEMA];
/// Search path used only by the boot migrator connection.
pub const MIGRATION_SEARCH_PATH: &str = "vala, iceberg_catalog, public";
const MIGRATION_ADVISORY_LOCK_KEY: i64 = 0x0056_5441_4c41_5351;

/// Apply embedded Vala SQL migrations against a boot-only migrator pool.
///
/// The supplied pool is the same `wyrd_migrator` pool used by
/// `wyrd_sql::migrate`. Migration runs on one dedicated connection with
/// `search_path` set to `vala, public`, then the physical connection is closed
/// so session state cannot return to the pool.
///
/// # Errors
/// Returns [`SqlError::Connect`] when the connection or bootstrap SQL fails.
/// Returns [`SqlError::Migrate`] when migration execution fails.
pub async fn migrate(migrator_pool: &PgPool) -> Result<(), SqlError> {
    let mut conn = migrator_pool.acquire().await.map_err(SqlError::Connect)?;
    acquire_migration_advisory_lock(&mut conn).await?;

    let result: Result<(), SqlError> = async {
        for schema in OWNED_SCHEMAS {
            sqlx::query(AssertSqlSafe(format!(
                "CREATE SCHEMA IF NOT EXISTS {schema}"
            )))
            .execute(&mut *conn)
            .await
            .map_err(SqlError::Connect)?;
        }
        sqlx::query(AssertSqlSafe(format!(
            "SET search_path TO {MIGRATION_SEARCH_PATH}"
        )))
        .execute(&mut *conn)
        .await
        .map_err(SqlError::Connect)?;
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
            "failed to release vala-sql migration advisory lock after migration error"
        );
    }

    if let Err(error) = conn.close().await {
        tracing::warn!(
            error = %error,
            "failed to close vala-sql migration connection cleanly"
        );
    }

    result
}

/// Acquire the Vala SQL migration advisory lock on the current session.
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

/// Release the Vala SQL migration advisory lock on the current session.
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

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use crate::{MIGRATION_SEARCH_PATH, OWNED_SCHEMAS};

    use std::fs;
    use std::path::{Path, PathBuf};

    /// Query modules that intentionally coordinate cross-tenant operator state.
    const MIXED_EXECUTOR_QUERY_MODULES: &[&str] = &["forge_operations.rs", "forge_tasks.rs"];

    /// Returns whether a query module is a sanctioned mixed executor owner.
    fn is_mixed_executor_query_module(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| MIXED_EXECUTOR_QUERY_MODULES.contains(&name))
    }

    #[test]
    fn schema_ownership_is_explicit() {
        assert_eq!(OWNED_SCHEMAS, &["vala", "iceberg_catalog"]);
        assert_eq!(MIGRATION_SEARCH_PATH, "vala, iceberg_catalog, public");
        assert!(OWNED_SCHEMAS.contains(&"iceberg_catalog"));
        assert!(!OWNED_SCHEMAS.contains(&"platform"));
        assert!(!OWNED_SCHEMAS.contains(&"wyrd"));
        assert!(!OWNED_SCHEMAS.contains(&"skald"));
    }

    #[test]
    fn error_reexport_uses_wyrd_sql_catalog() {
        fn assert_same_type(_: Option<crate::SqlError>, _: Option<wyrd_sql::error::SqlError>) {}

        assert_same_type(None, None);
        assert_eq!(crate::SqlError::NoRows.code(), "WYRD_SQL_404_NO_ROWS");
    }

    #[test]
    fn migrations_embed_count_matches_files() {
        let migrator = sqlx::migrate!("./migrations");
        let files = migration_files();

        assert_eq!(migrator.migrations.len(), files.len());
        assert!(
            !migrator.migrations.is_empty(),
            "vala-sql must embed at least one migration"
        );
    }

    #[test]
    fn create_table_statements_stay_in_owned_schema() {
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
                        !OWNED_SCHEMAS
                            .iter()
                            .any(|schema| line.starts_with(&format!("CREATE TABLE {schema}.")))
                    })
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        assert!(
            unexpected.is_empty(),
            "CREATE TABLE statements must target an owned schema: {unexpected:?}"
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

    /// Verifies the Forge/Oracle migration partition and forward grant owner.
    #[test]
    fn forge_and_oracle_migrations_have_unique_forward_owners() {
        let files = migration_files();
        let forge = [
            "20260910000010_forge_tasks.sql",
            "20260910000011_forge_planning_demands.sql",
            "20260910000012_forge_worker_claim_state.sql",
        ];
        let oracle = ["20260910000013_oracle_coordination.sql"];
        for file in forge.into_iter().chain(oracle) {
            assert!(
                files.iter().any(|candidate| candidate == file),
                "missing migration {file}"
            );
        }
        let old_olap = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("migrations/20260619000001_olap_minimal.sql"),
        )
        .expect("older OLAP migration is readable");
        let old_olap_digest = Sha256::digest(old_olap.as_bytes());
        assert_eq!(
            format!("{old_olap_digest:x}"),
            "a2df17d99f1ee29c1ead1fca2ea30c0062cd918d9d5b5e3f41f30ac6cdc259ef",
            "older migration checksum must remain stable"
        );
        let oracle_coordination = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("migrations/20260910000013_oracle_coordination.sql"),
        )
        .expect("Oracle coordination migration is readable");
        assert!(
            oracle_coordination
                .contains("GRANT SELECT ON vala.bifrost_tables TO wyrd_platform_admin")
        );
    }

    #[test]
    fn queries_module_shape_matches_foundation_plan() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let required_paths = [
            "src/queries/mod.rs",
            "src/queries/profiles.rs",
            "src/queries/queues.rs",
            "src/queries/anchors.rs",
            "src/queries/monitor.rs",
            "src/queries/olap_catalog.rs",
            "src/row_types/mod.rs",
            "src/row_types/profiles.rs",
            "src/row_types/queues.rs",
            "src/row_types/anchors.rs",
            "src/row_types/monitor.rs",
            "src/row_types/olap_catalog.rs",
        ];

        for relative_path in required_paths {
            assert!(
                crate_dir.join(relative_path).exists(),
                "missing planned vala-sql path: {relative_path}"
            );
        }

        let query_doc = fs::read_to_string(crate_dir.join("src/queries/mod.rs"))
            .expect("Vala query module doc is readable");
        assert!(query_doc.contains("TenantConn"));
        assert!(query_doc.contains("data_tenant_id = $"));
        assert!(query_doc.contains("future outbox path"));
    }

    #[test]
    /// Verifies tenant query owners do not accept raw pools or transaction handles.
    fn tenant_scoped_query_stubs_do_not_take_raw_pool_executors() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let forbidden = rust_files_under(&crate_dir.join("src/queries"))
            .into_iter()
            .filter(|path| !is_mixed_executor_query_module(path))
            .filter_map(|path| {
                let body = fs::read_to_string(&path).expect("Vala query file is readable");
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
            "tenant-scoped Vala query modules must use TenantConn, not raw executors: {forbidden:?}"
        );
    }

    #[test]
    /// Verifies tenant query owners never issue transaction-control SQL themselves.
    fn tenant_scoped_query_modules_do_not_issue_transaction_control_sql() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let forbidden = rust_files_under(&crate_dir.join("src/queries"))
            .into_iter()
            .filter(|path| !is_mixed_executor_query_module(path))
            .filter_map(|path| {
                let body = fs::read_to_string(&path).expect("Vala query source is readable");
                let checked = without_line_comments(&body).to_ascii_uppercase();
                let has_transaction_control = ["BEGIN", "COMMIT", "ROLLBACK", "SAVEPOINT"]
                    .into_iter()
                    .any(|keyword| contains_sql_keyword(&checked, keyword));
                has_transaction_control.then_some(path)
            })
            .collect::<Vec<_>>();

        assert!(
            forbidden.is_empty(),
            "tenant-scoped Vala query modules must rely on TenantConn, not raw transaction SQL: {forbidden:?}"
        );
    }

    #[test]
    fn vala_sql_does_not_call_wyrd_query_modules() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let forbidden = rust_files_under(&crate_dir.join("src"))
            .into_iter()
            .filter_map(|path| {
                let body = fs::read_to_string(&path).expect("Vala source file is readable");
                let uncommented = without_line_comments(&body);
                let checked = production_source(&uncommented);
                (checked.contains("wyrd_sql::queries") || checked.contains("wyrd_sql :: queries"))
                    .then_some(path)
            })
            .collect::<Vec<_>>();

        assert!(
            forbidden.is_empty(),
            "vala-sql must not call wyrd-sql query modules for cross-crate transactions: {forbidden:?}"
        );
    }

    #[test]
    fn cross_crate_transaction_boundary_is_documented() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let lib_doc = fs::read_to_string(crate_dir.join("src/lib.rs"))
            .expect("Vala lib module doc is readable");
        let query_doc = fs::read_to_string(crate_dir.join("src/queries/mod.rs"))
            .expect("Vala query module doc is readable");

        assert!(lib_doc.contains("does not extend Wyrd write transactions"));
        assert!(lib_doc.contains("future outbox/event fanout"));
        assert!(query_doc.contains("must not open nested transactions"));
    }

    #[test]
    fn vala_sql_does_not_depend_on_skald() {
        let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("vala-sql manifest is readable");

        assert!(
            !manifest.contains("skald"),
            "vala-sql must not depend on Skald crates"
        );
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

    fn rust_files_under(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        collect_rust_files(dir, &mut files);
        files.sort();
        files
    }

    fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
        if !dir.exists() {
            return;
        }

        for entry in fs::read_dir(dir).expect("source directory is readable") {
            let path = entry.expect("source directory entry is readable").path();
            if path.is_dir() {
                collect_rust_files(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
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

    /// Returns whether a quoted SQL statement starts with transaction control.
    fn contains_sql_keyword(text: &str, keyword: &str) -> bool {
        text.split('"').skip(1).step_by(2).any(|literal| {
            let statement = literal.trim_start();
            statement.starts_with(keyword)
                && statement
                    .as_bytes()
                    .get(keyword.len())
                    .is_none_or(u8::is_ascii_whitespace)
        })
    }
}
