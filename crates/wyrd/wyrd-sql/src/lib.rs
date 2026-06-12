//! Server-tier SQL scaffold for Wyrd control-plane storage.
//!
//! `wyrd-sql` owns the `platform` and `wyrd` PostgreSQL schemas. `platform`
//! holds state above the tenant boundary, including the future tenant catalog.
//! `wyrd` holds tenant-scoped control-plane state. Vala owns its own `vala`
//! schema in `vala-sql`; no `skald` schema exists in this phase.

#![deny(missing_docs)]

use sqlx::postgres::{PgPool, PgPoolOptions};

pub mod postgres_boot;

use postgres_boot::PoolConfig;

/// Platform-global schema owned by `wyrd-sql`.
pub const PLATFORM_SCHEMA: &str = "platform";
/// Tenant-scoped Wyrd control-plane schema owned by `wyrd-sql`.
pub const CONTROL_SCHEMA: &str = "wyrd";
/// Schemas whose migration lifecycle is owned by `wyrd-sql`.
pub const OWNED_SCHEMAS: &[&str] = &[PLATFORM_SCHEMA, CONTROL_SCHEMA];
/// Search path used only by the boot migrator connection.
pub const MIGRATION_SEARCH_PATH: &str = "wyrd, platform, public";

/// Apply embedded Wyrd SQL migrations against a boot-only migrator pool.
///
/// The supplied pool should authenticate as `wyrd_migrator`. Migration runs on
/// one dedicated connection with `search_path` set to `wyrd, platform, public`,
/// then the physical connection is closed so session state cannot return to the
/// pool.
///
/// # Errors
/// Returns [`SqlError::Connect`] when the connection or bootstrap SQL fails.
/// Returns [`SqlError::Migrate`] when migration execution fails.
pub async fn migrate(migrator_pool: &PgPool) -> Result<(), SqlError> {
    let mut conn = migrator_pool.acquire().await.map_err(SqlError::Connect)?;

    let result: Result<(), SqlError> = async {
        sqlx::query("CREATE SCHEMA IF NOT EXISTS platform")
            .execute(&mut *conn)
            .await?;
        sqlx::query("CREATE SCHEMA IF NOT EXISTS wyrd")
            .execute(&mut *conn)
            .await?;
        sqlx::query("SET search_path TO wyrd, platform, public")
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
            "failed to close wyrd-sql migration connection cleanly"
        );
    }

    result
}

/// SQL storage errors.
#[derive(Debug, thiserror::Error)]
pub enum SqlError {
    /// Database connection failed.
    #[error("database connection failed")]
    Connect(#[from] sqlx::Error),
    /// Database migration failed.
    #[error("migration failed")]
    Migrate(#[from] sqlx::migrate::MigrateError),
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
        let pool = postgres_boot::connect_pool(database_url, config)
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
    use std::path::Path;

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
