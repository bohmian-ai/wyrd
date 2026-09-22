//! Tenant-scoped Card registration lifecycle transitions.
#![deny(missing_docs)]

// raw-query grep allowlist: lifecycle transitions post-date the sqlx offline cache;
// run `mise run sqlx:prepare` to promote these tenant-bound statements to macros.

use chrono::{DateTime, Utc};
use sqlx::types::Uuid;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;

use crate::OperatorPool;
use crate::tenant_conn::TenantConn;

/// Registration side effects that can be retried by the lifecycle worker.
pub const RECONCILE_KIND_REGISTRATION: &str = "registration";
/// Immutable Card blob persistence side effects that can be retried.
pub const RECONCILE_KIND_BLOB: &str = "blob";
/// Manifest verification and Card activation side effects that can be retried.
pub const RECONCILE_KIND_FINALIZATION: &str = "finalization";
/// Artifact and upload cleanup side effects that can be retried.
pub const RECONCILE_KIND_CLEANUP: &str = "cleanup";

/// Maximum number of lifecycle attempts, including the initial attempt.
pub const MAX_RECONCILE_ATTEMPTS: i32 = 3;

/// One tenant-owned Card claimed for reconciliation.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CardReconcileClaim {
    /// Card being reconciled.
    pub card_uid: Uuid,
    /// Tenant that owns the Card.
    pub data_tenant_id: Uuid,
    /// Registration operation that created the Card, when present.
    pub registration_operation_id: Option<Uuid>,
    /// Lifecycle side-effect category.
    pub reconcile_kind: String,
    /// Attempt number assigned by the claim transaction.
    pub reconcile_attempts: i32,
    /// Lease owner assigned to this claim batch.
    pub reconcile_lease_owner: Uuid,
    /// Lease deadline assigned by the claim transaction, in database time.
    pub reconcile_lease_expires_at: DateTime<Utc>,
    /// Seconds of lease remaining as measured by the claim statement itself.
    ///
    /// Callers project this remainder onto a local monotonic `Instant` instead
    /// of comparing the database deadline with the host wall clock.
    pub lease_remaining_seconds: f64,
}

/// Claim due Card lifecycle work through the audited cross-tenant operator pool.
///
/// Eligibility and the new lease deadline are both derived from the claim
/// statement's `statement_timestamp()`, so a host clock that leads or lags
/// PostgreSQL cannot defer or shorten reconciliation. `lease_seconds` is the
/// requested lease length; the statement returns the remainder it actually
/// granted.
///
/// The single claim statement only stamps the lease and attempt number; its
/// statement transaction commits before storage IO or retry delays begin.
///
/// # Errors
///
/// Returns `registry_unavailable` when the claim statement fails.
// tenant-isolation: cross-tenant OperatorPool
pub async fn claim_card_reconciliation(
    operator: &OperatorPool,
    lease_seconds: i64,
    limit: i64,
) -> Result<Vec<CardReconcileClaim>, WyrdError> {
    let lease_owner = Uuid::now_v7();
    let rows = sqlx::query_as::<_, CardReconcileClaim>(
        r#"WITH candidates AS (
                SELECT card_uid, data_tenant_id
                  FROM wyrd.cards
                 WHERE (
                         (reconcile_status = 'pending'
                          AND reconcile_next_attempt_at <= statement_timestamp())
                      OR (reconcile_status = 'leased'
                          AND reconcile_lease_expires_at <= statement_timestamp())
                       )
                   AND (
                         reconcile_attempts < $3
                      OR (reconcile_attempts = $3 AND reconcile_status = 'leased')
                       )
                 ORDER BY reconcile_next_attempt_at NULLS FIRST,
                          reconcile_lease_expires_at NULLS FIRST,
                          updated_at
                 FOR UPDATE SKIP LOCKED
                 LIMIT $2
            )
            UPDATE wyrd.cards card
               SET reconcile_status = 'leased',
                   reconcile_attempts = CASE
                       WHEN card.reconcile_attempts < $3
                       THEN card.reconcile_attempts + 1
                       ELSE card.reconcile_attempts
                   END,
                   reconcile_lease_owner = $1,
                   reconcile_lease_expires_at =
                       statement_timestamp() + ($4 * interval '1 second'),
                   updated_at = now()
              FROM candidates
             WHERE card.card_uid = candidates.card_uid
               AND card.data_tenant_id = candidates.data_tenant_id
         RETURNING card.card_uid,
                   card.data_tenant_id,
                   card.registration_operation_id,
                   card.reconcile_kind,
                   card.reconcile_attempts,
                   card.reconcile_lease_owner,
                   card.reconcile_lease_expires_at,
                   EXTRACT(EPOCH FROM (card.reconcile_lease_expires_at
                                       - statement_timestamp()))::double precision
                       AS lease_remaining_seconds"#,
    )
    .bind(lease_owner)
    .bind(limit)
    .bind(MAX_RECONCILE_ATTEMPTS)
    .bind(lease_seconds as f64)
    .fetch_all(operator.pool())
    .await
    .map_err(|error| {
        tracing::error!(%error, "card reconciliation claim failed");
        WyrdError::registry_unavailable("card registry unavailable")
    })?;
    Ok(rows)
}

/// Re-check that a claimed Card still owns a live reconciliation lease.
pub async fn lock_card_reconciliation_lease(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    lease_owner: Uuid,
) -> Result<bool, WyrdError> {
    let found = sqlx::query_scalar::<_, bool>(
        r#"SELECT true
             FROM wyrd.cards
            WHERE card_uid = $1
              AND reconcile_status = 'leased'
              AND reconcile_lease_owner = $2
              AND reconcile_lease_expires_at > statement_timestamp()
            FOR UPDATE"#,
    )
    .bind(card_uid.as_uuid())
    .bind(lease_owner)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(|error| {
        tracing::error!(%error, %card_uid, "card reconciliation lease recheck failed");
        WyrdError::registry_unavailable("card registry unavailable")
    })?;
    Ok(found.unwrap_or(false))
}

/// Schedule tenant-owned lifecycle work after a client-visible side effect failed.
///
/// `retry_delay_seconds` is a delay, not a deadline: PostgreSQL derives the
/// next attempt from its own `statement_timestamp()` so host clock skew cannot
/// make freshly scheduled work ineligible.
///
/// # Errors
///
/// Returns `registry_unavailable` when the update statement fails.
pub async fn schedule_card_reconciliation(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    kind: &str,
    retry_delay_seconds: i64,
    error_code: &str,
    error_message: &str,
) -> Result<bool, WyrdError> {
    let result = sqlx::query(
        r#"UPDATE wyrd.cards
              SET reconcile_kind = $2,
                  reconcile_status = 'pending',
                  reconcile_attempts = 0,
                  reconcile_next_attempt_at =
                      statement_timestamp() + ($3 * interval '1 second'),
                  reconcile_lease_owner = NULL,
                  reconcile_lease_expires_at = NULL,
                  reconcile_last_error_code = $4,
                  reconcile_last_error_message = $5,
                  reconcile_dead_lettered_at = NULL,
                  updated_at = now()
            WHERE card_uid = $1
              AND reconcile_status IN ('idle', 'pending')
              AND status IN ('pending', 'failed', 'deleted')"#,
    )
    .bind(card_uid.as_uuid())
    .bind(kind)
    .bind(retry_delay_seconds as f64)
    .bind(error_code)
    .bind(error_message)
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| {
        tracing::error!(%error, %card_uid, "card reconciliation schedule failed");
        WyrdError::registry_unavailable("card registry unavailable")
    })?;
    Ok(result.rows_affected() == 1)
}

/// Return a claimed row to the retry queue without consuming an attempt.
///
/// `retry_delay_seconds` is a delay evaluated against PostgreSQL's own
/// `statement_timestamp()`.
///
/// # Errors
///
/// Returns `registry_unavailable` when the update statement fails.
pub async fn reschedule_card_reconciliation(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    lease_owner: Uuid,
    retry_delay_seconds: i64,
    error_code: &str,
    error_message: &str,
) -> Result<bool, WyrdError> {
    let result = sqlx::query(
        r#"UPDATE wyrd.cards
              SET reconcile_status = 'pending',
                  reconcile_attempts = GREATEST(reconcile_attempts - 1, 0),
                  reconcile_next_attempt_at =
                      statement_timestamp() + ($3 * interval '1 second'),
                  reconcile_lease_owner = NULL,
                  reconcile_lease_expires_at = NULL,
                  reconcile_last_error_code = $4,
                  reconcile_last_error_message = $5,
                  updated_at = now()
            WHERE card_uid = $1
              AND reconcile_status = 'leased'
              AND reconcile_lease_owner = $2"#,
    )
    .bind(card_uid.as_uuid())
    .bind(lease_owner)
    .bind(retry_delay_seconds as f64)
    .bind(error_code)
    .bind(error_message)
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| {
        tracing::error!(%error, %card_uid, "card reconciliation lease reschedule failed");
        WyrdError::registry_unavailable("card registry unavailable")
    })?;
    Ok(result.rows_affected() == 1)
}

/// Mark claimed or client-owned lifecycle work complete and release its lease.
pub async fn mark_card_reconciliation_succeeded(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    lease_owner: Option<Uuid>,
) -> Result<bool, WyrdError> {
    let result = match lease_owner {
        Some(lease_owner) => {
            sqlx::query(
                r#"UPDATE wyrd.cards
                  SET reconcile_kind = 'registration',
                      reconcile_status = 'idle',
                      reconcile_attempts = 0,
                      reconcile_next_attempt_at = NULL,
                      reconcile_lease_owner = NULL,
                      reconcile_lease_expires_at = NULL,
                      reconcile_last_error_code = NULL,
                      reconcile_last_error_message = NULL,
                      reconcile_dead_lettered_at = NULL,
                      updated_at = now()
                WHERE card_uid = $1
                  AND reconcile_status = 'leased'
                  AND reconcile_lease_owner = $2
                  AND reconcile_lease_expires_at > statement_timestamp()"#,
            )
            .bind(card_uid.as_uuid())
            .bind(lease_owner)
            .execute(&mut **conn.transaction())
            .await
        }
        None => {
            sqlx::query(
                r#"UPDATE wyrd.cards
                  SET reconcile_kind = 'registration',
                      reconcile_status = 'idle',
                      reconcile_attempts = 0,
                      reconcile_next_attempt_at = NULL,
                      reconcile_lease_owner = NULL,
                      reconcile_lease_expires_at = NULL,
                      reconcile_last_error_code = NULL,
                      reconcile_last_error_message = NULL,
                      reconcile_dead_lettered_at = NULL,
                      updated_at = now()
                WHERE card_uid = $1
                  AND reconcile_status <> 'dead_lettered'"#,
            )
            .bind(card_uid.as_uuid())
            .execute(&mut **conn.transaction())
            .await
        }
    }
    .map_err(|error| {
        tracing::error!(%error, %card_uid, "card reconciliation success update failed");
        WyrdError::registry_unavailable("card registry unavailable")
    })?;
    Ok(result.rows_affected() == 1)
}

/// Record a failed claimed attempt, dead-lettering exactly the third failure.
///
/// `retry_delay_seconds` is a delay evaluated against PostgreSQL's own
/// `statement_timestamp()`.
///
/// # Errors
///
/// Returns `registry_unavailable` when the update statement fails.
pub async fn record_card_reconciliation_failure(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    lease_owner: Uuid,
    retry_delay_seconds: i64,
    error_code: &str,
    error_message: &str,
) -> Result<bool, WyrdError> {
    let status = sqlx::query_scalar::<_, String>(
        r#"UPDATE wyrd.cards
              SET reconcile_status = CASE
                      WHEN reconcile_attempts >= $3 THEN 'dead_lettered'
                      ELSE 'pending'
                  END,
                  reconcile_next_attempt_at = CASE
                      WHEN reconcile_attempts >= $3 THEN NULL
                      ELSE statement_timestamp() + ($4 * interval '1 second')
                  END,
                  reconcile_lease_owner = NULL,
                  reconcile_lease_expires_at = NULL,
                  reconcile_last_error_code = $5,
                  reconcile_last_error_message = $6,
                  reconcile_dead_lettered_at = CASE
                      WHEN reconcile_attempts >= $3 THEN now()
                      ELSE NULL
                  END,
                  updated_at = now()
            WHERE card_uid = $1
              AND reconcile_status = 'leased'
              AND reconcile_lease_owner = $2
              AND reconcile_lease_expires_at > statement_timestamp()
         RETURNING reconcile_status"#,
    )
    .bind(card_uid.as_uuid())
    .bind(lease_owner)
    .bind(MAX_RECONCILE_ATTEMPTS)
    .bind(retry_delay_seconds as f64)
    .bind(error_code)
    .bind(error_message)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(|error| {
        tracing::error!(%error, %card_uid, "card reconciliation failure update failed");
        WyrdError::registry_unavailable("card registry unavailable")
    })?;
    Ok(status.as_deref() == Some("dead_lettered"))
}

/// One manifest entry plus the storage upload row bound to it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CardManifestCompletionRow {
    /// Manifest-relative artifact path.
    pub relative_path: String,
    /// Manifest-declared SHA-256 digest.
    pub expected_sha256: String,
    /// Manifest-declared byte length.
    pub expected_size_bytes: i64,
    /// Storage upload identifier recorded during upload initialization.
    pub upload_id: Option<Uuid>,
    /// Card-manifest lifecycle state.
    pub manifest_status: String,
    /// Card UID recorded by the storage upload row.
    pub storage_card_uid: Option<String>,
    /// Relative path recorded by the storage upload row.
    pub storage_relative_path: Option<String>,
    /// Tenant-scoped storage object path.
    pub storage_path: Option<String>,
    /// Storage upload lifecycle state.
    pub storage_status: Option<String>,
    /// SHA-256 bound to the storage upload.
    pub storage_expected_sha256: Option<String>,
    /// Byte length bound to the storage upload.
    pub storage_expected_size_bytes: Option<i64>,
    /// Storage backend bound to the upload.
    pub storage_backend: Option<String>,
    /// Whether PostgreSQL considers the linked upload session live and resumable.
    ///
    /// The verdict combines the upload status with `expires_at >
    /// statement_timestamp()` inside the owning query, so no caller compares a
    /// storage deadline with the host clock.
    pub storage_upload_live: bool,
}

/// Load all manifest entries and their upload rows for one Card.
pub async fn manifest_completion_rows(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
) -> Result<Vec<CardManifestCompletionRow>, WyrdError> {
    sqlx::query_as::<_, CardManifestCompletionRow>(
        r#"SELECT m.relative_path,
                  m.expected_sha256,
                  m.size_bytes AS expected_size_bytes,
                  m.upload_id,
                  m.upload_status AS manifest_status,
                  u.card_uid AS storage_card_uid,
                  u.relative_path AS storage_relative_path,
                  u.storage_path,
                  u.status AS storage_status,
                  u.expected_sha256 AS storage_expected_sha256,
                  u.expected_size_bytes AS storage_expected_size_bytes,
                  u.backend AS storage_backend,
                  COALESCE(u.status IN ('initiating', 'pending')
                           AND u.expires_at > statement_timestamp(), false)
                      AS storage_upload_live
             FROM wyrd.card_artifact_manifest m
              LEFT JOIN wyrd.storage_multipart_uploads u
               ON u.id = m.upload_id
            WHERE m.card_uid = $1
            ORDER BY m.relative_path"#,
    )
    .bind(card_uid.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(|error| {
        tracing::error!(%error, %card_uid, "card manifest lifecycle lookup failed");
        WyrdError::registry_unavailable("card registry unavailable")
    })
}

/// Mark one storage-bound manifest entry verified after storage verification.
pub async fn mark_manifest_verified(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    upload_id: Uuid,
) -> Result<bool, WyrdError> {
    let result = sqlx::query(
        r#"UPDATE wyrd.card_artifact_manifest
              SET upload_status = 'verified', verified_at = now()
            WHERE card_uid = $1
              AND upload_id = $2
              AND upload_status IN ('pending', 'uploaded', 'verified')"#,
    )
    .bind(card_uid.as_uuid())
    .bind(upload_id)
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| {
        tracing::error!(%error, %card_uid, %upload_id, "card manifest verification update failed");
        WyrdError::registry_unavailable("card registry unavailable")
    })?;
    Ok(result.rows_affected() == 1)
}

/// Record the durable immutable Card blob URI after a successful object write.
pub async fn record_card_blob(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    blob_uri: &str,
) -> Result<(), WyrdError> {
    sqlx::query(
        r#"UPDATE wyrd.cards
              SET card_blob_uri = $2, blob_failed_at = NULL, updated_at = now()
            WHERE card_uid = $1
              AND status = 'pending'"#,
    )
    .bind(card_uid.as_uuid())
    .bind(blob_uri)
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| {
        tracing::error!(%error, %card_uid, "card blob URI update failed");
        WyrdError::registry_unavailable("card registry unavailable")
    })?;
    // A concurrent completion may have activated the row after the caller's
    // state read. Treat that lost update as an idempotent no-op; the following
    // activation check/reload observes the winner's durable state.
    Ok(())
}

/// Record the latest failed immutable Card blob write for reconciliation.
pub async fn record_blob_failure(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
) -> Result<(), WyrdError> {
    sqlx::query(
        r#"UPDATE wyrd.cards
              SET blob_failed_at = now(), updated_at = now()
            WHERE card_uid = $1
              AND status = 'pending'"#,
    )
    .bind(card_uid.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| {
        tracing::error!(%error, %card_uid, "card blob failure update failed");
        WyrdError::registry_unavailable("card registry unavailable")
    })?;
    Ok(())
}

/// Lock a Card and confirm that it is still pending before finalization writes.
pub async fn lock_pending_card_for_activation(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
) -> Result<bool, WyrdError> {
    let status = sqlx::query_scalar::<_, String>(
        r#"SELECT status
             FROM wyrd.cards
            WHERE card_uid = $1
            FOR UPDATE"#,
    )
    .bind(card_uid.as_uuid())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(|error| {
        tracing::error!(%error, %card_uid, "card activation state recheck failed");
        WyrdError::registry_unavailable("card registry unavailable")
    })?;
    Ok(status.as_deref() == Some("pending"))
}

/// Activate a Card only when its blob and every manifest entry are durable.
pub async fn activate_card(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
) -> Result<bool, WyrdError> {
    let result = sqlx::query(
        r#"UPDATE wyrd.cards c
              SET status = 'active', finalized_at = now(), updated_at = now()
            WHERE c.card_uid = $1
              AND c.status = 'pending'
              AND c.card_blob_uri IS NOT NULL
              AND NOT EXISTS (
                    SELECT 1
                      FROM wyrd.card_artifact_manifest m
                     WHERE m.card_uid = c.card_uid
                       AND m.upload_status <> 'verified'
              )"#,
    )
    .bind(card_uid.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| {
        tracing::error!(%error, %card_uid, "card activation update failed");
        WyrdError::registry_unavailable("card registry unavailable")
    })?;
    Ok(result.rows_affected() == 1)
}

/// Mark a pending Card failed after internal compensation.
pub async fn fail_card(conn: &mut TenantConn<'_>, card_uid: &CardUid) -> Result<bool, WyrdError> {
    let result = sqlx::query(
        r#"UPDATE wyrd.cards
              SET status = 'failed',
                  reconcile_kind = 'cleanup',
                  reconcile_status = 'pending',
                  reconcile_attempts = 0,
                  reconcile_next_attempt_at = now(),
                  reconcile_lease_owner = NULL,
                  reconcile_lease_expires_at = NULL,
                  reconcile_last_error_code = NULL,
                  reconcile_last_error_message = NULL,
                  reconcile_dead_lettered_at = NULL,
                  updated_at = now()
            WHERE card_uid = $1
              AND status = 'pending'"#,
    )
    .bind(card_uid.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| {
        tracing::error!(%error, %card_uid, "card failure transition failed");
        WyrdError::registry_unavailable("card registry unavailable")
    })?;
    Ok(result.rows_affected() == 1)
}
