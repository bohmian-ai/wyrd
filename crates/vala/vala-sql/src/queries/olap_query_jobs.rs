//! Async query-job storage for `vala.olap_query_jobs`.
//!
//! Pure tenant-bound row layer: no SQL validation, no DataFusion, no routes.
//! Redaction happens upstream; these functions persist `sql_normalized` /
//! `params_redacted` verbatim and never see raw literals. Both functions run
//! under the caller's `data_tenant_id` bind via [`TenantConn`].

// raw-query grep allowlist: olap control tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::types::{JsonValue, Uuid};
use wyrd_spec::vala::api::{AsyncQueryStatus, JobUid};
use wyrd_sql::TenantConn;

use crate::SqlError;
use crate::row_types::olap_query_jobs::OlapQueryJobRow;

/// Borrowed insert payload for [`enqueue_query_job`].
pub struct NewQueryJob<'a> {
    /// Caller-supplied idempotency key, unique per tenant.
    pub idempotency_key: &'a str,
    /// Normalized SQL text; raw literals must already be stripped.
    pub sql_normalized: &'a str,
    /// Redacted bound parameters.
    pub params_redacted: &'a JsonValue,
}

/// Enqueue an async query job under the caller's tenant bind, idempotently.
///
/// Uses `INSERT … ON CONFLICT (data_tenant_id, idempotency_key) DO NOTHING`.
/// A retried submit with the same `idempotency_key` returns the existing
/// [`JobUid`] — never a second row, never an error. The row lands `queued`
/// with `executor_availability = pending_stage5` (no executor exists yet).
///
/// # Errors
/// Returns [`SqlError`] when the insert fails or an RLS policy rejects the row.
pub async fn enqueue_query_job(
    conn: &mut TenantConn<'_>,
    job: NewQueryJob<'_>,
) -> Result<JobUid, SqlError> {
    let candidate = Uuid::now_v7();
    let job_uid: Uuid = sqlx::query_scalar(
        r#"
        WITH new_job AS (
            INSERT INTO vala.olap_query_jobs
                (data_tenant_id, job_uid, idempotency_key, sql_normalized, params_redacted)
            VALUES (wyrd.current_tenant(), $1, $2, $3, $4)
            ON CONFLICT (data_tenant_id, idempotency_key) DO NOTHING
            RETURNING job_uid
        )
        SELECT job_uid FROM new_job
        UNION ALL
        SELECT job_uid FROM vala.olap_query_jobs
         WHERE data_tenant_id = wyrd.current_tenant()
           AND idempotency_key = $2
           AND NOT EXISTS (SELECT 1 FROM new_job)
        "#,
    )
    .bind(candidate)
    .bind(job.idempotency_key)
    .bind(job.sql_normalized)
    .bind(job.params_redacted.clone())
    .fetch_one(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(JobUid(job_uid))
}

/// Read one job's [`AsyncQueryStatus`] by [`JobUid`] under the tenant bind.
///
/// A job owned by another tenant is invisible under RLS and returns
/// `Ok(None)` — it never leaks the job's existence.
///
/// # Errors
/// Returns [`SqlError`] when the query fails or a stored literal is out of domain.
pub async fn query_job_status(
    conn: &mut TenantConn<'_>,
    job_uid: JobUid,
) -> Result<Option<AsyncQueryStatus>, SqlError> {
    let row = sqlx::query_as::<_, OlapQueryJobRow>(
        r#"
        SELECT data_tenant_id, job_uid, idempotency_key, state, executor_availability,
               sql_normalized, params_redacted, query_class, admission_mode,
               error_code, error_detail, submitted_at, updated_at
          FROM vala.olap_query_jobs
         WHERE job_uid = $1
           AND data_tenant_id = wyrd.current_tenant()
        "#,
    )
    .bind(job_uid.0)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    row.map(OlapQueryJobRow::into_status).transpose()
}
