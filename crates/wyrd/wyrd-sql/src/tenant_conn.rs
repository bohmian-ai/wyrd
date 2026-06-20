//! Tenant-scoped Postgres transaction wrapper.
//!
//! A `TenantConn` is the transaction boundary for one tenant-scoped logical
//! operation. Handlers and workers acquire one wrapper, pass the same mutable
//! reference through all tenant-scoped query modules involved in that operation,
//! then commit once at the boundary. Query modules must not open nested SQLx
//! transactions or issue transaction-control SQL; dropping the wrapper rolls the
//! transaction back when an error leaves the operation early.

use sqlx::{PgPool, Postgres, Transaction};
use wyrd_spec::DataTenantId;

use crate::SqlError;

/// PostgreSQL GUC holding the tenant UUID for row-level-security policies.
pub const CURRENT_TENANT_GUC: &str = "app.current_tenant";

/// Parameterized query that binds the current tenant for one transaction.
///
/// `set_config(..., true)` is the function form of `SET LOCAL`, so the tenant
/// value can be bound as a parameter and the setting evaporates at commit or
/// rollback.
pub const BIND_CURRENT_TENANT_SQL: &str = "SELECT set_config('app.current_tenant', $1, true)";

/// Tenant-scoped transaction for queries that must run under RLS.
///
/// Acquiring a `TenantConn` opens a transaction on the runtime `wyrd_app` pool
/// and binds `app.current_tenant` to the supplied tenant UUID for that
/// transaction only. Dropping without [`commit`](Self::commit) rolls the
/// transaction back through SQLx's normal `Transaction` drop behavior. The same
/// value should be threaded through all reads and writes for the operation;
/// nested transactions and savepoints are intentionally outside the v1 SQL
/// foundation.
pub struct TenantConn<'a> {
    tx: Transaction<'a, Postgres>,
    data_tenant_id: DataTenantId,
}

impl<'a> TenantConn<'a> {
    /// Open a transaction and bind the current tenant for its lifetime.
    ///
    /// # Errors
    /// Returns [`SqlError::Connect`] when the pool cannot begin a transaction.
    /// Returns [`SqlError::TxFailed`] when the tenant binding query fails.
    pub async fn acquire(pool: &'a PgPool, data_tenant_id: DataTenantId) -> Result<Self, SqlError> {
        let mut tx = pool.begin().await.map_err(SqlError::Connect)?;
        sqlx::query(BIND_CURRENT_TENANT_SQL)
            .bind(tenant_binding_value(data_tenant_id))
            .execute(&mut *tx)
            .await
            .map_err(SqlError::TxFailed)?;

        Ok(Self { tx, data_tenant_id })
    }

    /// Return the tenant isolation key bound on this transaction.
    #[must_use]
    pub fn data_tenant_id(&self) -> DataTenantId {
        self.data_tenant_id
    }

    /// Borrow the underlying transaction for query modules.
    pub fn transaction(&mut self) -> &mut Transaction<'a, Postgres> {
        &mut self.tx
    }

    /// Commit the transaction.
    ///
    /// # Errors
    /// Returns [`SqlError::TxFailed`] when Postgres rejects the commit.
    pub async fn commit(self) -> Result<(), SqlError> {
        self.tx.commit().await.map_err(SqlError::TxFailed)
    }
}

fn tenant_binding_value(data_tenant_id: DataTenantId) -> String {
    data_tenant_id.as_uuid().to_string()
}

#[cfg(test)]
mod tests {
    use super::{BIND_CURRENT_TENANT_SQL, CURRENT_TENANT_GUC, tenant_binding_value};
    use wyrd_spec::DataTenantId;

    #[test]
    fn tenant_binding_uses_transaction_local_parameter() {
        assert_eq!(CURRENT_TENANT_GUC, "app.current_tenant");
        assert_eq!(
            BIND_CURRENT_TENANT_SQL,
            "SELECT set_config('app.current_tenant', $1, true)"
        );
        assert!(BIND_CURRENT_TENANT_SQL.contains("$1"));
        assert!(BIND_CURRENT_TENANT_SQL.ends_with(", true)"));
        assert!(!BIND_CURRENT_TENANT_SQL.contains("missing_ok"));
    }

    #[test]
    fn tenant_binding_value_uses_data_tenant_uuid() {
        let data_tenant_id = DataTenantId::new_v7();
        let binding = tenant_binding_value(data_tenant_id);

        assert_eq!(binding, data_tenant_id.to_string());
        assert_eq!(binding.parse::<DataTenantId>().unwrap(), data_tenant_id);
    }
}
