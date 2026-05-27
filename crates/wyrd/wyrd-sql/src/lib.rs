//! Server-tier SQL scaffold for Wyrd control-plane storage.

#![deny(missing_docs)]

use sqlx::postgres::{PgPool, PgPoolOptions};

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

    /// Apply embedded SQL migrations.
    ///
    /// # Errors
    /// Returns [`SqlError::Migrate`] when migration execution fails.
    pub async fn migrate(&self) -> Result<(), SqlError> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(SqlError::Migrate)
    }

    /// Borrow the underlying Postgres pool.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn migrations_dir_embeds() {
        let migrator = sqlx::migrate!("./migrations");

        assert_eq!(migrator.migrations.len(), 1);
        assert_eq!(migrator.migrations[0].version, 1);
    }
}
