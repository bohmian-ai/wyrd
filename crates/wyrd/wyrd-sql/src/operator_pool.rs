//! Audited cross-tenant BYPASSRLS pool handle.
//!
//! `OperatorPool` wraps the `wyrd_platform_admin` pool — the audited
//! cross-tenant BYPASSRLS pool that operator and control-table functions use to
//! enumerate or act across every tenant partition. It is the sole mechanism that
//! query modules are allowed to hold for cross-tenant SQL; no query fn may take
//! a bare `&PgPool`.
//!
//! Transactions that a cross-tenant operator fn needs are opened here via
//! `tenant_conn`, which binds a chosen tenant on a transaction drawn from this
//! BYPASSRLS pool. The same `TenantConn` type is reused so query modules that
//! issue tenant-scoped SQL remain callable from the operator path without
//! modification.

use sqlx::PgPool;
use wyrd_spec::DataTenantId;

use crate::{SqlError, TenantConn};

/// Audited cross-tenant BYPASSRLS Postgres pool handle.
///
/// Wraps the `wyrd_platform_admin` pool. Every cross-tenant / operator /
/// control-table query fn takes `&OperatorPool` and calls `op.pool()` inline in
/// the `query!` invocation. Operator transactions (for fns that need to bind a
/// specific tenant) are opened via `tenant_conn`.
///
/// `None` when no cross-tenant role is configured (single-app or dev setup).
/// Production boot that requires cross-tenant maintenance fails fast when the
/// accessor on `WyrdPostgres` / `ServerPostgres` returns `None`.
#[derive(Clone)]
pub struct OperatorPool(PgPool);

impl OperatorPool {
    /// Borrow the underlying BYPASSRLS pool.
    ///
    /// Pass the returned reference directly to `query!` / `query_as!` as the
    /// executor. Do not store it separately.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.0
    }

    /// Open a transaction on the BYPASSRLS pool and bind it to `tenant_id`.
    ///
    /// Use this when a cross-tenant worker needs to self-transact for a specific
    /// tenant — for example a worker that iterates tenants and writes per-tenant
    /// state. The returned `TenantConn` is identical in shape to one opened via
    /// `WyrdPostgres::tenant_conn`, so all tenant-scoped query modules remain
    /// callable without modification.
    ///
    /// # Errors
    /// Returns [`SqlError`] when acquiring or binding the transaction fails.
    pub async fn tenant_conn(&self, tenant_id: DataTenantId) -> Result<TenantConn<'_>, SqlError> {
        TenantConn::acquire(&self.0, tenant_id).await
    }
}

impl From<PgPool> for OperatorPool {
    fn from(pool: PgPool) -> Self {
        Self(pool)
    }
}
