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

use sqlx::{AssertSqlSafe, PgPool};

pub mod queries;
pub mod row_types;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use wyrd_sql::{TenantConn, error::SqlError};

/// Tenant-scoped Vala observability schema owned by `vala-sql`.
pub const OBSERVABILITY_SCHEMA: &str = "vala";
/// Iceberg JDBC catalog schema owned by `vala-sql` migrations.
pub const ICEBERG_CATALOG_SCHEMA: &str = "iceberg_catalog";
/// Schemas whose migration lifecycle is owned by `vala-sql`.
pub const OWNED_SCHEMAS: &[&str] = &[OBSERVABILITY_SCHEMA, ICEBERG_CATALOG_SCHEMA];
/// Search path used only by the boot migrator connection.
pub const MIGRATION_SEARCH_PATH: &str = "vala, iceberg_catalog, public";

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

    let result: Result<(), SqlError> = async {
        for schema in OWNED_SCHEMAS {
            sqlx::query(AssertSqlSafe(format!(
                "CREATE SCHEMA IF NOT EXISTS {schema}"
            )))
            .execute(&mut *conn)
            .await
            .map_err(SqlError::Connect)?;
        }
        // Pre-commit schema grants before migrations start. PostgreSQL 17
        // enforces that the new owner has CREATE on a function's schema during
        // ALTER FUNCTION OWNER. Within-transaction grants are not visible to the
        // ACL cache at that point, so we commit them here before sqlx::migrate!.
        sqlx::query(
            "DO $$ BEGIN
                 IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vala_recovery_owner') THEN
                     GRANT USAGE, CREATE ON SCHEMA vala TO vala_recovery_owner;
                 END IF;
                 IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vala_recovery') THEN
                     GRANT USAGE ON SCHEMA vala TO vala_recovery;
                 END IF;
                 IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vala_audit_relay') THEN
                     GRANT USAGE, CREATE ON SCHEMA vala TO vala_audit_relay;
                 END IF;
             END $$",
        )
        .execute(&mut *conn)
        .await
        .map_err(SqlError::Connect)?;
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

    if let Err(error) = conn.close().await {
        tracing::warn!(
            error = %error,
            "failed to close vala-sql migration connection cleanly"
        );
    }

    result
}

#[cfg(test)]
mod tests {
    use crate::{MIGRATION_SEARCH_PATH, OWNED_SCHEMAS};

    use std::fs;
    use std::path::{Path, PathBuf};

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

    #[test]
    fn queries_module_shape_matches_foundation_plan() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let required_paths = [
            "src/queries/mod.rs",
            "src/queries/profiles.rs",
            "src/queries/queues.rs",
            "src/queries/anchors.rs",
            "src/queries/alerts.rs",
            "src/queries/monitor.rs",
            "src/queries/olap_catalog.rs",
            "src/row_types/mod.rs",
            "src/row_types/profiles.rs",
            "src/row_types/queues.rs",
            "src/row_types/anchors.rs",
            "src/row_types/alerts.rs",
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
    fn tenant_scoped_query_stubs_do_not_take_raw_pool_executors() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let forbidden = rust_files_under(&crate_dir.join("src/queries"))
            .into_iter()
            .filter_map(|path| {
                let body = fs::read_to_string(&path).expect("Vala query file is readable");
                // diagnostics-gated modules are explicitly tenant-free by design.
                if body.contains("#![cfg(feature = \"diagnostics\")]") {
                    return None;
                }
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
    fn tenant_scoped_query_modules_do_not_issue_transaction_control_sql() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let forbidden = rust_files_under(&crate_dir.join("src/queries"))
            .into_iter()
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
    fn shared_pool_contract_uses_wyrd_runtime_pool_by_reference() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let lib = fs::read_to_string(crate_dir.join("src/lib.rs"))
            .expect("Vala lib module doc is readable");
        let manifest =
            fs::read_to_string(crate_dir.join("Cargo.toml")).expect("manifest is readable");
        let uncommented_lib = without_line_comments(&lib);
        let production_lib = production_source(&uncommented_lib);

        assert!(lib.contains("shared [`TenantConn`] wrapper"));
        assert!(production_lib.contains("pub async fn migrate(migrator_pool: &PgPool)"));
        assert!(manifest.contains("wyrd-sql"));
        assert!(
            !production_lib.contains("PoolConfig")
                && !production_lib.contains("build_pool")
                && !production_lib.contains("PostgresBoot"),
            "vala-sql must consume Wyrd-owned pools by reference, not build another pool"
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

    fn contains_sql_keyword(text: &str, keyword: &str) -> bool {
        let mut start = 0;
        while let Some(pos) = text[start..].find(keyword) {
            let abs = start + pos;
            let before_ok =
                abs == 0 || !matches!(text.as_bytes()[abs - 1], b'A'..=b'Z' | b'0'..=b'9' | b'_');
            let end = abs + keyword.len();
            let after_ok = end >= text.len()
                || !matches!(text.as_bytes()[end], b'A'..=b'Z' | b'0'..=b'9' | b'_');
            if before_ok && after_ok {
                return true;
            }
            start = abs + 1;
        }
        false
    }
}
