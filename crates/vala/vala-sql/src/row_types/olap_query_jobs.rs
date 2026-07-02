//! Row type for the Vala async query-job store (`vala.olap_query_jobs`).
//!
//! Pure sqlx row that projects to the S3.C2a wire types
//! ([`wyrd_spec::vala::api`]) at the SQL boundary. No SQL validation and no
//! DataFusion live here — `vala-sql` stays a pure row layer.

use sqlx::types::{JsonValue, Uuid, chrono};
use wyrd_spec::vala::api::{AsyncJobState, AsyncQueryStatus, ExecutorAvailability, JobUid};

use crate::SqlError;

/// One async query-job row from `vala.olap_query_jobs`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OlapQueryJobRow {
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Server-assigned job identifier.
    pub job_uid: Uuid,
    /// Caller-supplied idempotency key, unique per tenant.
    pub idempotency_key: String,
    /// Lifecycle state literal (schema CHECK domain).
    pub state: String,
    /// Executor-availability literal (`pending_stage5` in Stage 3).
    pub executor_availability: String,
    /// Normalized SQL text; raw literals never reach this column.
    pub sql_normalized: String,
    /// Redacted bound parameters.
    pub params_redacted: JsonValue,
    /// Stage-5 classifier output; `NULL` in Stage 3.
    pub query_class: Option<String>,
    /// Stage-5 admission decision; `NULL` in Stage 3.
    pub admission_mode: Option<String>,
    /// Stable Wyrd error code on failure.
    pub error_code: Option<String>,
    /// Sanitized error detail on failure.
    pub error_detail: Option<String>,
    /// Wall-clock submission time.
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    /// Wall-clock last-update time.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl OlapQueryJobRow {
    /// Project this row to the [`AsyncQueryStatus`] wire type.
    ///
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] when a stored `state` or
    /// `executor_availability` literal falls outside the schema CHECK domain.
    pub fn into_status(self) -> Result<AsyncQueryStatus, SqlError> {
        Ok(AsyncQueryStatus {
            job_uid: JobUid(self.job_uid),
            state: parse_state(&self.state)?,
            executor_availability: parse_executor_availability(&self.executor_availability)?,
            error_code: self.error_code,
            error_detail: self.error_detail,
        })
    }
}

fn parse_state(value: &str) -> Result<AsyncJobState, SqlError> {
    match value {
        "queued" => Ok(AsyncJobState::Queued),
        "claimed" => Ok(AsyncJobState::Claimed),
        "running" => Ok(AsyncJobState::Running),
        "succeeded" => Ok(AsyncJobState::Succeeded),
        "failed" => Ok(AsyncJobState::Failed),
        "canceled" => Ok(AsyncJobState::Canceled),
        other => Err(SqlError::InvariantViolation {
            detail: format!("unknown olap_query_jobs.state literal: {other}"),
        }),
    }
}

fn parse_executor_availability(value: &str) -> Result<ExecutorAvailability, SqlError> {
    match value {
        "available" => Ok(ExecutorAvailability::Available),
        "pending_stage5" => Ok(ExecutorAvailability::PendingStage5),
        other => Err(SqlError::InvariantViolation {
            detail: format!("unknown olap_query_jobs.executor_availability literal: {other}"),
        }),
    }
}
