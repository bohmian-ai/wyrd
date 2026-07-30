//! Operator-owned transaction boundary for cluster control-plane workflows.

use sqlx::{PgConnection, Postgres, Transaction};

use crate::{OperatorPool, SqlError};

/// One operator-role transaction with lifecycle hidden from query modules.
pub(crate) struct OperatorTransaction {
    /// SQLx transaction hidden behind the operator workflow boundary.
    inner: Transaction<'static, Postgres>,
}

impl OperatorTransaction {
    /// Starts one transaction from the audited operator pool.
    ///
    /// # Errors
    /// Returns [`SqlError`] when a connection cannot begin a transaction.
    pub(crate) async fn start(pool: &OperatorPool) -> Result<Self, SqlError> {
        Ok(Self {
            inner: pool.pool().begin().await.map_err(SqlError::from)?,
        })
    }

    /// Borrows the transaction as the SQLx connection executor.
    pub(crate) fn connection(&mut self) -> &mut PgConnection {
        &mut self.inner
    }

    /// Atomically publishes every mutation in the workflow.
    ///
    /// # Errors
    /// Returns [`SqlError`] when PostgreSQL cannot commit.
    pub(crate) async fn finish(self) -> Result<(), SqlError> {
        self.inner.commit().await.map_err(SqlError::from)
    }

    /// Discards every mutation in the workflow.
    ///
    /// # Errors
    /// Returns [`SqlError`] when PostgreSQL cannot roll back.
    pub(crate) async fn cancel(self) -> Result<(), SqlError> {
        self.inner.rollback().await.map_err(SqlError::from)
    }
}
