//! Tenant-scoped upload initialization idempotency cache.
// raw-query grep allowlist: storage tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use crate::error::SqlError;
use crate::tenant_conn::TenantConn;
use serde::{Deserialize, Serialize};
use sqlx::types::JsonValue;
use std::time::Duration;
use wyrd_spec::storage::{StorageBackendKind, WireProtocol};

const STORAGE_INIT_ADVISORY_CLASS: i32 = 0x0C_A2_D0_02;

/// Serialize initialization for one tenant-scoped idempotency or artifact key.
///
/// The lock is transaction-scoped, so it is released automatically on commit or
/// rollback. The surrounding [`TenantConn`] supplies the tenant binding; the
/// advisory class keeps these locks separate from registry version-line locks.
/// Callers take this lock before checking the cache or performing the external
/// storage initialization so concurrent retries cannot mint duplicate uploads.
///
/// # Errors
/// Returns [`SqlError`] when PostgreSQL cannot acquire the advisory lock.
pub async fn lock_init(conn: &mut TenantConn<'_>, identity: &str) -> Result<(), SqlError> {
    sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2))")
        .bind(STORAGE_INIT_ADVISORY_CLASS)
        .bind(identity)
        .execute(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

/// Non-bearer replay seed cached for upload initialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct UploadInitReplaySeed {
    /// Upload identifier rendered for the public storage API.
    pub upload_id: String,
    /// Full tenant-scoped storage path.
    pub storage_path: String,
    /// Configured backend.
    pub backend: StorageBackendKind,
    /// Upload wire protocol.
    pub wire_protocol: WireProtocol,
}

/// Cached idempotency value for a matching key and request body hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedSeed {
    /// Cached response status.
    pub status: i16,
    /// Cached non-bearer replay seed.
    pub seed: UploadInitReplaySeed,
}

#[derive(Debug, sqlx::FromRow)]
struct IdempotencyRow {
    body_sha256: Vec<u8>,
    response_status: i16,
    response_body: JsonValue,
}

/// Fetch a cached seed when the key and body hash match.
///
/// # Errors
/// Returns [`SqlError::Conflict`] when the same key is reused with a different
/// body hash. Returns [`SqlError`] when the query fails or cached JSON is invalid.
pub async fn get(
    conn: &mut TenantConn<'_>,
    idempotency_key: &str,
    body_sha256: &[u8],
) -> Result<Option<CachedSeed>, SqlError> {
    let row = sqlx::query_as::<_, IdempotencyRow>(
        r#"
        SELECT body_sha256, response_status, response_body
        FROM wyrd.storage_idempotency_keys
        WHERE idempotency_key = $1
          AND expires_at > now()
        "#,
    )
    .bind(idempotency_key)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    let Some(row) = row else {
        return Ok(None);
    };

    if row.body_sha256 != body_sha256 {
        return Err(SqlError::Conflict {
            detail: format!("idempotency key {idempotency_key:?} reused with different body"),
        });
    }

    let seed = serde_json::from_value(row.response_body).map_err(|error| {
        SqlError::InvariantViolation {
            detail: format!("idempotency key {idempotency_key:?} has invalid replay seed: {error}"),
        }
    })?;

    Ok(Some(CachedSeed {
        status: row.response_status,
        seed,
    }))
}

/// Store a non-bearer replay seed for upload initialization.
///
/// # Errors
/// Returns [`SqlError`] when serialization fails or Postgres rejects the write.
pub async fn store(
    conn: &mut TenantConn<'_>,
    idempotency_key: &str,
    body_sha256: &[u8],
    response_status: i16,
    seed: &UploadInitReplaySeed,
    ttl: Duration,
) -> Result<(), SqlError> {
    let response_body =
        serde_json::to_value(seed).map_err(|error| SqlError::InvariantViolation {
            detail: format!("failed to serialize upload init replay seed: {error}"),
        })?;

    sqlx::query(
        r#"
        INSERT INTO wyrd.storage_idempotency_keys (
            data_tenant_id,
            idempotency_key,
            body_sha256,
            response_status,
            response_body,
            expires_at
        )
        VALUES (
            wyrd.current_tenant(),
            $1,
            $2,
            $3,
            $4,
            now() + ($5::text || ' seconds')::interval
        )
        ON CONFLICT (data_tenant_id, idempotency_key) DO UPDATE SET
            body_sha256 = EXCLUDED.body_sha256,
            response_status = EXCLUDED.response_status,
            response_body = EXCLUDED.response_body,
            expires_at = EXCLUDED.expires_at
        "#,
    )
    .bind(idempotency_key)
    .bind(body_sha256)
    .bind(response_status)
    .bind(response_body)
    .bind(ttl.as_secs() as i64)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(())
}
