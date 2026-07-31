//! Audited cross-tenant BYPASSRLS pool handle.
//!
//! `OperatorPool` wraps the `wyrd_platform_admin` pool — the audited
//! cross-tenant BYPASSRLS pool that operator and control-table functions use to
//! enumerate or act across every tenant partition. It is the sole mechanism that
//! query modules are allowed to hold for cross-tenant SQL; no query fn may take
//! a bare `&PgPool`.

use sqlx::PgPool;

/// Audited cross-tenant BYPASSRLS Postgres pool handle.
///
/// Wraps the `wyrd_platform_admin` pool. Every cross-tenant / operator /
/// control-table query fn takes `&OperatorPool` and calls `op.pool()` inline in
/// the `query!` invocation. Tenant-scoped transactions remain the responsibility
/// of [`crate::WyrdPostgres`] or the owning tier's equivalent Postgres handle.
///
/// `None` when no cross-tenant role is configured (single-app or dev setup).
/// Production boot that requires cross-tenant maintenance fails fast when the
/// accessor on `WyrdPostgres` / `ServerPostgres` returns `None`.
#[derive(Clone)]
pub struct OperatorPool(PgPool);

impl OperatorPool {
    /// Opens one operator-owned transaction for a bounded cross-tenant operation.
    ///
    /// The transaction remains borrowed from this handle and is owned by the
    /// caller until commit or rollback. Query modules use this boundary rather
    /// than reaching through to the underlying pool and constructing a raw
    /// transaction themselves.
    ///
    /// # Errors
    /// Returns the database error when PostgreSQL cannot acquire a connection
    /// or begin the transaction.
    pub async fn begin(&self) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, sqlx::Error> {
        self.0.begin().await
    }

    /// Borrow the underlying BYPASSRLS pool.
    ///
    /// Pass the returned reference directly to `query!` / `query_as!` as the
    /// executor. Do not store it separately.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.0
    }
}

impl From<PgPool> for OperatorPool {
    fn from(pool: PgPool) -> Self {
        Self(pool)
    }
}
