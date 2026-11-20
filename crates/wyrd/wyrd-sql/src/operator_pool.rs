//! Audited cross-tenant operator pool handle.
//!
//! `OperatorPool` wraps the platform pool — the audited operator session that
//! every table's `operator_access` row-level-security policy admits — which
//! operator and control-table functions use to enumerate or act across every
//! tenant partition. It is the sole mechanism that
//! query modules are allowed to hold for cross-tenant SQL; no query fn may take
//! a bare `&PgPool`.

use std::future::Future;

use sqlx::{PgPool, Postgres, Transaction};
use tokio::time::Instant;
use wyrd_spec::DataTenantId;

use crate::{SqlError, TenantConn};

/// Audited cross-tenant operator-session Postgres pool handle.
///
/// Wraps the platform pool. Every cross-tenant / operator /
/// control-table query fn takes `&OperatorPool` and calls `op.pool()` inline in
/// the `query!` invocation. Tenant-scoped transactions remain the responsibility
/// of [`crate::WyrdPostgres`] or the owning tier's equivalent Postgres handle.
///
/// The one-off `wyrd-server migrate` process also wraps its platform-login
/// pool in this handle to hold the migration lease and run the same schema
/// checks serving boot runs; that migration pool never exists in a serving
/// process.
///
/// `WyrdPostgres::operator_pool` returns `None` only for handles built without
/// a platform pool (DB-free tests); production boot fails fast on `None`.
#[derive(Clone)]
pub struct OperatorPool(PgPool);

impl OperatorPool {
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
    /// pool handing back the existing tenant-scoped transaction type. The
    /// operator session still sees every tenant here, which is exactly why
    /// `append_audit` names `data_tenant_id` in every statement rather than
    /// relying on the policy.
    ///
    /// # Errors
    /// Returns [`SqlError::Connect`] when the pool cannot begin a transaction,
    /// and [`SqlError::TxFailed`] when the tenant binding fails.
    pub async fn begin_platform_audited(&self) -> Result<TenantConn<'_>, SqlError> {
        TenantConn::acquire(&self.0, DataTenantId::SYSTEM_OWNER).await
    }

    /// Opens one operator transaction for an operation that must finish by
    /// `deadline`, or `None` once the deadline passes first.
    ///
    /// Waiting for a connection, `BEGIN`, and the setup statement all spend
    /// the same deadline; none of them restarts it. The transaction's
    /// statements then carry a transaction-local `statement_timeout` of the
    /// time left, so PostgreSQL itself cancels a statement still waiting, for
    /// example on a lock, with `query_canceled` (`57014`) and an abandoned
    /// write rolls back with the transaction rather than landing later. Each
    /// later step of the operation, including its commit, must also run
    /// [`within`] the same deadline. The timeout never returns to the pool. A
    /// `None` deadline waits as long as the pool and statements take.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the transaction cannot begin or its timeout
    /// cannot be set.
    pub async fn begin_bounded(
        &self,
        deadline: Option<Instant>,
    ) -> Result<Option<Transaction<'static, Postgres>>, SqlError> {
        let begin = async {
            let mut tx = self.0.begin().await.map_err(SqlError::from)?;
            if let Some(deadline) = deadline {
                // A zero timeout disables the limit, so the shortest bound is 1 ms.
                let millis = deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .clamp(1, i32::MAX as u128);
                sqlx::query("SELECT set_config('statement_timeout', $1, true)")
                    .bind(format!("{millis}ms"))
                    .execute(&mut *tx)
                    .await
                    .map_err(SqlError::from)?;
            }
            Ok(tx)
        };
        within(deadline, begin).await.transpose()
    }

    /// Borrow the underlying operator-session pool.
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

/// Await `operation` until `deadline`, or `None` once the deadline passes
/// first; a `None` deadline awaits it to completion.
///
/// An expired deadline starts nothing. A cut operation is dropped where it
/// stands: an open transaction it borrowed queues its rollback for the
/// connection's return to the pool, and a commit it had not yet sent is never
/// sent. A commit already sent may still land, so a cut caller counts only
/// what it observed and resumes from what is stored.
pub(crate) async fn within<F: Future>(
    deadline: Option<Instant>,
    operation: F,
) -> Option<F::Output> {
    let Some(deadline) = deadline else {
        return Some(operation.await);
    };
    if Instant::now() >= deadline {
        return None;
    }
    tokio::time::timeout_at(deadline, operation).await.ok()
}
