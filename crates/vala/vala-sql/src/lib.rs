//! Server-tier SQL migrations for Vala observability storage.
//!
//! `vala-sql` owns the `vala` PostgreSQL schema for observability control-plane
//! state. Wyrd control-plane and platform schemas remain owned by `wyrd-sql`;
//! no `skald` schema exists in this phase.

#![deny(missing_docs)]

use sqlx::PgPool;

/// Tenant-scoped Vala observability schema owned by `vala-sql`.
pub const OBSERVABILITY_SCHEMA: &str = "vala";
/// Schemas whose migration lifecycle is owned by `vala-sql`.
pub const OWNED_SCHEMAS: &[&str] = &[OBSERVABILITY_SCHEMA];
/// Search path used only by the boot migrator connection.
pub const MIGRATION_SEARCH_PATH: &str = "vala, public";

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
        sqlx::query("CREATE SCHEMA IF NOT EXISTS vala")
            .execute(&mut *conn)
            .await?;
        sqlx::query("SET search_path TO vala, public")
            .execute(&mut *conn)
            .await?;
        sqlx::migrate!("./migrations")
            .run(&mut *conn)
            .await
            .map_err(SqlError::Migrate)
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

/// Vala SQL migration errors.
#[derive(Debug, thiserror::Error)]
pub enum SqlError {
    /// Database connection or bootstrap SQL failed.
    #[error("database connection failed")]
    Connect(#[from] sqlx::Error),
    /// Database migration failed.
    #[error("migration failed")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

#[cfg(test)]
mod tests {
    use crate::{MIGRATION_SEARCH_PATH, OWNED_SCHEMAS};

    use std::fs;
    use std::path::Path;

    #[test]
    fn schema_ownership_is_explicit() {
        assert_eq!(OWNED_SCHEMAS, &["vala"]);
        assert_eq!(MIGRATION_SEARCH_PATH, "vala, public");
        assert!(!OWNED_SCHEMAS.contains(&"platform"));
        assert!(!OWNED_SCHEMAS.contains(&"wyrd"));
        assert!(!OWNED_SCHEMAS.contains(&"skald"));
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
                    .filter(|line| !line.starts_with("CREATE TABLE vala."))
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        assert!(
            unexpected.is_empty(),
            "CREATE TABLE statements must target vala.*: {unexpected:?}"
        );
    }

    #[test]
    fn migration_filenames_match_sequential_versions() {
        let migrator = sqlx::migrate!("./migrations");
        let files = migration_files();

        for (index, (migration, file_name)) in migrator.migrations.iter().zip(files).enumerate() {
            let version = i64::try_from(index + 1).expect("migration index fits in i64");
            let prefix = format!("{version:04}_");

            assert_eq!(migration.version, version);
            assert!(
                file_name.starts_with(&prefix),
                "migration file {file_name} must start with {prefix}"
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
