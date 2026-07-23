//! Tenant-scoped Card registration lifecycle transitions.
#![deny(missing_docs)]

// raw-query grep allowlist: lifecycle transitions post-date the sqlx offline cache;
// run `mise run sqlx:prepare` to promote these tenant-bound statements to macros.

use sqlx::types::Uuid;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;

use crate::tenant_conn::TenantConn;

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
                  u.backend AS storage_backend
             FROM wyrd.card_artifact_manifest m
             LEFT JOIN wyrd.storage_multipart_uploads u
               ON u.id = m.upload_id
              AND u.data_tenant_id = wyrd.current_tenant()
            WHERE m.data_tenant_id = wyrd.current_tenant()
              AND m.card_uid = $1
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
            WHERE data_tenant_id = wyrd.current_tenant()
              AND card_uid = $1
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
            WHERE data_tenant_id = wyrd.current_tenant()
              AND card_uid = $1
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
            WHERE data_tenant_id = wyrd.current_tenant()
              AND card_uid = $1
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
            WHERE data_tenant_id = wyrd.current_tenant()
              AND card_uid = $1
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
            WHERE c.data_tenant_id = wyrd.current_tenant()
              AND c.card_uid = $1
              AND c.status = 'pending'
              AND c.card_blob_uri IS NOT NULL
              AND NOT EXISTS (
                    SELECT 1
                      FROM wyrd.card_artifact_manifest m
                     WHERE m.data_tenant_id = wyrd.current_tenant()
                       AND m.card_uid = c.card_uid
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
              SET status = 'failed', updated_at = now()
            WHERE data_tenant_id = wyrd.current_tenant()
              AND card_uid = $1
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
