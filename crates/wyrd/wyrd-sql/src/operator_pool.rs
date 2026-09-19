//! Audited cross-tenant BYPASSRLS pool handle.
//!
//! `OperatorPool` wraps the `wyrd_platform_admin` pool — the audited
//! cross-tenant BYPASSRLS pool that operator and control-table functions use to
//! enumerate or act across every tenant partition. It is the sole mechanism that
//! query modules are allowed to hold for cross-tenant SQL; no query fn may take
//! a bare `&PgPool`.

use sqlx::PgPool;
use wyrd_spec::DataTenantId;

use crate::{SqlError, TenantConn};

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
    /// Crate-private on purpose. A raw `sqlx::Transaction` is unrestricted SQL
    /// capability: handed across a crate boundary it lets any caller issue any
    /// statement the BYPASSRLS role can. The query modules in this crate are
    /// the only place that capability is bounded by a reviewed statement, so
    /// callers outside it take [`begin_platform_audited`](Self::begin_platform_audited)
    /// and get a [`TenantConn`] instead.
    ///
    /// # Errors
    /// Returns the database error when PostgreSQL cannot acquire a connection
    /// or begin the transaction.
    pub(crate) async fn begin(&self) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, sqlx::Error> {
        self.0.begin().await
    }

    /// Opens one operator transaction carrying the platform audit scope.
    ///
    /// A platform-control-plane decision has no owning tenant, so its canonical
    /// audit row stages under the `wyrd-system` sentinel
    /// (`DataTenantId::SYSTEM_OWNER`) while its effect lands in `platform.*`.
    /// Both must share one transaction, and only this boundary can write
    /// `platform.*`, so the returned [`TenantConn`] is an operator transaction
    /// that also carries the tenant key the canonical append names.
    ///
    /// This is not a third connection abstraction: it is the existing operator
    /// pool handing back the existing tenant-scoped transaction type. Row-level
    /// security is still bypassed here, which is exactly why `append_audit`
    /// names `data_tenant_id` in every statement rather than relying on the
    /// policy.
    ///
    /// # Errors
    /// Returns [`SqlError::Connect`] when the pool cannot begin a transaction,
    /// and [`SqlError::TxFailed`] when the tenant binding fails.
    pub async fn begin_platform_audited(&self) -> Result<TenantConn<'_>, SqlError> {
        TenantConn::acquire(&self.0, DataTenantId::SYSTEM_OWNER).await
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
