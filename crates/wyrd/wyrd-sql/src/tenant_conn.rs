//! Tenant-scoped Postgres transaction wrapper.
//!
//! A `TenantConn` is the transaction boundary for one tenant-scoped logical
//! operation. Handlers and workers acquire one wrapper, pass the same mutable
//! reference through all tenant-scoped query modules involved in that operation,
//! then commit once at the boundary. Query modules must not open nested SQLx
//! transactions or issue transaction-control SQL; dropping the wrapper rolls the
//! transaction back when an error leaves the operation early.

use sqlx::{AssertSqlSafe, PgPool, Postgres, Transaction};
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
    /// The tenant binding and `BEGIN` travel as one simple-query message (see
    /// [`begin_tenant_sql`]), so opening a tenant transaction costs one round
    /// trip instead of two.
    ///
    /// # Errors
    /// Returns [`SqlError::TxFailed`] when Postgres rejects the begin-and-bind
    /// statement; Postgres has already rolled it back, so the connection
    /// returns to the pool idle. Returns [`SqlError::Connect`] when the pool
    /// cannot supply a connection or the connection fails.
    pub async fn acquire(pool: &'a PgPool, data_tenant_id: DataTenantId) -> Result<Self, SqlError> {
        let tx = begin_bound(pool, begin_tenant_sql(data_tenant_id)).await?;
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

    /// Roll the transaction back now, releasing its row locks before the
    /// caller continues, instead of deferring the rollback to drop.
    ///
    /// # Errors
    /// Returns [`SqlError::TxFailed`] when Postgres rejects the rollback.
    pub async fn rollback(self) -> Result<(), SqlError> {
        self.tx.rollback().await.map_err(SqlError::TxFailed)
    }
}

fn tenant_binding_value(data_tenant_id: DataTenantId) -> String {
    data_tenant_id.as_uuid().to_string()
}

/// Builds the one statement that binds a tenant and opens its transaction.
///
/// The binding comes before `BEGIN` on purpose. Postgres runs a multi-statement
/// simple query as one implicit transaction, and a later `BEGIN` turns that
/// same transaction into an explicit one, so the transaction-local binding
/// survives into it. If the binding fails, `BEGIN` never runs and Postgres
/// rolls the implicit transaction back by itself. The reverse order would
/// leave the connection inside an aborted transaction that SQLx does not roll
/// back, poisoning the next borrower.
///
/// The tenant is inlined because a simple query takes no parameters. That is
/// injection-safe only because the value is a typed [`DataTenantId`] rendered
/// through `Uuid`'s hyphenated-hex `Display`, which cannot contain a quote;
/// never widen this parameter to a string.
fn begin_tenant_sql(data_tenant_id: DataTenantId) -> String {
    format!(
        "SELECT set_config('{CURRENT_TENANT_GUC}', '{}', true); BEGIN",
        tenant_binding_value(data_tenant_id)
    )
}

/// Opens a pooled transaction with `statement` as its begin statement.
///
/// SQLx accepts the transaction only when Postgres reports it open afterwards.
/// Its pool-acquire, connect, and TLS future is boxed here, once for every
/// tenant transaction, because inlining that deep chain into callers' futures
/// exceeds the compiler's layout depth limit on deep server paths such as Oracle
/// query forwarding in release builds.
///
/// # Errors
/// Returns [`SqlError::TxFailed`] when Postgres rejects `statement`, and
/// [`SqlError::Connect`] for pool or connection failures.
async fn begin_bound(
    pool: &PgPool,
    statement: String,
) -> Result<Transaction<'static, Postgres>, SqlError> {
    Box::pin(pool.begin_with(AssertSqlSafe(statement)))
        .await
        .map_err(|error| match error {
            sqlx::Error::Database(_) => SqlError::TxFailed(error),
            _ => SqlError::Connect(error),
        })
}

#[cfg(test)]
mod tests {
    use super::{
        BIND_CURRENT_TENANT_SQL, CURRENT_TENANT_GUC, begin_tenant_sql, tenant_binding_value,
    };
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

    /// The combined statement binds the exact tenant UUID transaction-locally
    /// before `BEGIN`, with nothing else in the text.
    #[test]
    fn begin_tenant_sql_binds_then_begins() {
        let data_tenant_id = DataTenantId::new_v7();

        assert_eq!(
            begin_tenant_sql(data_tenant_id),
            format!("SELECT set_config('app.current_tenant', '{data_tenant_id}', true); BEGIN")
        );
    }
}

/// Postgres-backed tenant transaction tests, skipped by the fast lanes.
#[cfg(test)]
mod pg_tests {
    use super::{TenantConn, begin_bound};
    use crate::SqlError;
    use sqlx::postgres::PgPoolOptions;
    use wyrd_spec::DataTenantId;

    /// A failed begin-and-bind leaves no transaction behind: on a one-connection
    /// pool, the same connection then opens a tenant transaction whose binding
    /// is visible inside it and gone after commit.
    ///
    /// # Panics
    /// Panics when `WYRD_DATABASE_URL` is unset (`mise run test:sql` sets it)
    /// or when any step of the transaction round trip fails.
    #[tokio::test]
    async fn failed_bind_rolls_back_and_the_connection_stays_usable() {
        let url = std::env::var("WYRD_DATABASE_URL").expect("test:sql sets WYRD_DATABASE_URL");
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("app pool connects");

        let failed = begin_bound(&pool, "SELECT 1/0; BEGIN".to_owned()).await;
        assert!(matches!(failed, Err(SqlError::TxFailed(_))), "{failed:?}");

        let tenant = DataTenantId::new_v7();
        let mut conn = TenantConn::acquire(&pool, tenant)
            .await
            .expect("the pooled connection is idle, not aborted");
        let bound: String = sqlx::query_scalar("SELECT current_setting('app.current_tenant')")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("tenant binding reads inside the transaction");
        assert_eq!(bound, tenant.to_string());
        conn.commit().await.expect("tenant transaction commits");

        let after: Option<String> =
            sqlx::query_scalar("SELECT NULLIF(current_setting('app.current_tenant', true), '')")
                .fetch_one(&pool)
                .await
                .expect("setting reads after commit");
        assert_eq!(after, None, "the binding is transaction-local");
    }
}
