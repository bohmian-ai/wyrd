//! Approval decisions: whether an execution still accepts a decision, and
//! making a parked execution due once one is accepted.
//!
//! The decision itself is accepted in Bifrost beside the execution's state;
//! the queue only learns whether the execution is still open and that it is
//! due. Expiry is the execution's own deadline, its admission plus the
//! configured execution timeout, on PostgreSQL's clock.

use chrono::Duration;
use sqlx::Error as SqlxError;

use wyrd_spec::ids::ExecutionId;

use super::{VerifierExecutionQueue, timed};
use crate::TenantConn;

/// Whether execution `$1` is unfinished and its deadline, admission plus
/// `$2` milliseconds, is still ahead on the database clock.
const OPEN_SQL: &str = "
    SELECT status IN ('pending', 'retrying', 'running')
           AND created_at + ($2::bigint * INTERVAL '1 millisecond') > statement_timestamp()
      FROM wyrd.verifier_executions
     WHERE execution_id = $1
";

/// Make queued execution `$1` due now unless it is already due or held back
/// until activation.
const WAKE_SQL: &str = "
    UPDATE wyrd.verifier_executions
       SET next_attempt_at = statement_timestamp(), updated_at = statement_timestamp()
     WHERE execution_id = $1 AND status IN ('pending', 'retrying')
       AND next_attempt_at > statement_timestamp() AND next_attempt_at <> 'infinity'
";

impl VerifierExecutionQueue {
    /// Whether `execution_id` still accepts an approval decision: it is
    /// unfinished and its deadline, `deadline_for` after admission, has not
    /// passed. `None` for an execution the tenant does not hold.
    ///
    /// # Errors
    /// Returns the database error when the read fails.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.approval.open"))]
    pub async fn open_for_decision(
        &self,
        conn: &mut TenantConn<'_>,
        execution_id: ExecutionId,
        deadline_for: Duration,
    ) -> Result<Option<bool>, SqlxError> {
        timed("approval_open", async {
            sqlx::query_scalar(OPEN_SQL)
                .bind(execution_id.as_uuid())
                .bind(deadline_for.num_milliseconds())
                .fetch_optional(&mut **conn.transaction())
                .await
        })
        .await
    }

    /// Make the parked `execution_id` due now, so it consumes a decision
    /// accepted for it at once; returns whether it was parked. The caller
    /// commits. A lost wake loses nothing: a parked execution rechecks its
    /// decisions when its bounded park ends.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.approval.wake"))]
    pub async fn wake_decided(
        &self,
        conn: &mut TenantConn<'_>,
        execution_id: ExecutionId,
    ) -> Result<bool, SqlxError> {
        timed("approval_wake", async {
            let woken = sqlx::query(WAKE_SQL)
                .bind(execution_id.as_uuid())
                .execute(&mut **conn.transaction())
                .await?;
            Ok(woken.rows_affected() > 0)
        })
        .await
    }
}
