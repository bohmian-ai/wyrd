//! Query slots for `platform.credentials`.
//!
//! A credential authenticates exactly one platform principal. Only the Argon2
//! verifier and non-secret lookup metadata are stored; the plaintext is returned
//! once by the issuing operation and is never recoverable afterwards.
//!
//! A principal may hold several live credentials at once, which is what makes
//! rotation an overlap rather than a gap: issue the replacement, verify it,
//! then revoke the superseded one.
// raw-query grep allowlist: platform administrative tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use sqlx::types::Uuid;

use sqlx::{Postgres, Transaction};

use crate::{OperatorPool, SqlError};

/// Credential lookup row joined to its owning principal.
///
/// Carries the principal's status so an authentication attempt can reject a
/// suspended principal without a second read, and so every invalid condition
/// collapses to one indistinguishable outcome for the caller.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlatformCredentialLookupRow {
    /// Credential id.
    pub id: Uuid,
    /// Owning principal id.
    pub principal_id: Uuid,
    /// Stored Argon2 PHC verifier.
    pub secret_hash: String,
    /// Owning principal's lifecycle status.
    pub principal_status: String,
    /// Revocation time, when revoked.
    pub revoked_at: Option<DateTime<Utc>>,
    /// Expiry, when the credential is bounded.
    pub expires_at: Option<DateTime<Utc>>,
}

impl PlatformCredentialLookupRow {
    /// True when this credential is presently usable.
    ///
    /// Usable means the credential is neither revoked nor expired and its
    /// principal is active. Callers must still verify the secret; this decides
    /// only the lifecycle half.
    #[must_use]
    pub fn is_usable(&self, now: DateTime<Utc>) -> bool {
        self.revoked_at.is_none()
            && self.expires_at.is_none_or(|expiry| expiry > now)
            && self.principal_status == "active"
    }
}

/// Non-secret credential metadata for listing.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlatformCredentialMetadataRow {
    /// Credential id.
    pub id: Uuid,
    /// Non-secret lookup prefix.
    pub prefix: String,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Expiry, when the credential is bounded.
    pub expires_at: Option<DateTime<Utc>>,
    /// Revocation time, when revoked.
    pub revoked_at: Option<DateTime<Utc>>,
    /// Last successful use.
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Insert a platform credential, storing only its verifier.
///
/// # Errors
/// Returns [`SqlError::Query`] when the insert fails, including when `prefix`
/// collides or `principal_id` does not name an existing platform principal.
pub async fn insert_platform_credential(
    pool: &OperatorPool,
    id: Uuid,
    principal_id: Uuid,
    prefix: &str,
    secret_hash: &str,
    expires_at: Option<DateTime<Utc>>,
) -> Result<(), SqlError> {
    sqlx::query(INSERT_PLATFORM_CREDENTIAL_SQL)
        .bind(id)
        .bind(principal_id)
        .bind(prefix)
        .bind(secret_hash)
        .bind(expires_at)
        .execute(pool.pool())
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

/// Insert a platform credential inside a caller-owned transaction.
///
/// Used where the credential is only meaningful together with what else the
/// transaction writes — notably deployment initialization, where a principal
/// without its first credential would be an unusable root that the unique name
/// then makes impossible to replace.
///
/// # Errors
/// Returns [`SqlError::Query`] when the insert fails, including on a repeated
/// prefix or an unknown principal.
pub async fn insert_platform_credential_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    principal_id: Uuid,
    prefix: &str,
    secret_hash: &str,
    expires_at: Option<DateTime<Utc>>,
) -> Result<(), SqlError> {
    sqlx::query(INSERT_PLATFORM_CREDENTIAL_SQL)
        .bind(id)
        .bind(principal_id)
        .bind(prefix)
        .bind(secret_hash)
        .bind(expires_at)
        .execute(&mut **tx)
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

/// The one credential insert, shared by the pool and transaction entry points
/// so they cannot drift apart.
const INSERT_PLATFORM_CREDENTIAL_SQL: &str = "INSERT INTO platform.credentials
     (id, principal_id, prefix, secret_hash, expires_at)
 VALUES ($1, $2, $3, $4, $5)";

/// Look up a platform credential by its non-secret prefix.
///
/// Returns the verifier and lifecycle state for the caller to check. An unknown
/// prefix returns `None`, which the caller must render indistinguishable from a
/// wrong, expired, or revoked secret.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn platform_credential_by_prefix(
    pool: &OperatorPool,
    prefix: &str,
) -> Result<Option<PlatformCredentialLookupRow>, SqlError> {
    sqlx::query_as::<_, PlatformCredentialLookupRow>(
        "SELECT c.id,
                c.principal_id,
                c.secret_hash,
                p.status AS principal_status,
                c.revoked_at,
                c.expires_at
           FROM platform.credentials c
           JOIN platform.principals p ON p.id = c.principal_id
          WHERE c.prefix = $1",
    )
    .bind(prefix)
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// Look up a platform credential by its durable id.
///
/// Used when verifying a platform token, which names the credential that minted
/// it: revoking that credential must stop its tokens, so the lifecycle state is
/// re-read rather than trusted from the token.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn platform_credential_by_id(
    pool: &OperatorPool,
    id: Uuid,
) -> Result<Option<PlatformCredentialLookupRow>, SqlError> {
    sqlx::query_as::<_, PlatformCredentialLookupRow>(
        "SELECT c.id,
                c.principal_id,
                c.secret_hash,
                p.status AS principal_status,
                c.revoked_at,
                c.expires_at
           FROM platform.credentials c
           JOIN platform.principals p ON p.id = c.principal_id
          WHERE c.id = $1",
    )
    .bind(id)
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// List a platform principal's credential metadata, newest first.
///
/// Never returns secret material: listing exists so an operator can see what to
/// rotate or revoke, not to recover a credential.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn list_platform_credentials(
    pool: &OperatorPool,
    principal_id: Uuid,
) -> Result<Vec<PlatformCredentialMetadataRow>, SqlError> {
    sqlx::query_as::<_, PlatformCredentialMetadataRow>(
        "SELECT id, prefix, created_at, expires_at, revoked_at, last_used_at
           FROM platform.credentials
          WHERE principal_id = $1
          ORDER BY created_at DESC",
    )
    .bind(principal_id)
    .fetch_all(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// Revoke one platform credential.
///
/// Revocation is idempotent and leaves the first revocation time in place, so a
/// repeated call does not rewrite when authority actually ended. Returns whether
/// this call performed the revocation.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn revoke_platform_credential(pool: &OperatorPool, id: Uuid) -> Result<bool, SqlError> {
    let result = sqlx::query(
        "UPDATE platform.credentials
            SET revoked_at = now()
          WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(pool.pool())
    .await
    .map_err(SqlError::from)?;
    Ok(result.rows_affected() == 1)
}

/// Record a successful use of a platform credential.
///
/// Best-effort observability for operators deciding what is still in service.
/// It is not part of the authentication decision.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn touch_platform_credential(pool: &OperatorPool, id: Uuid) -> Result<(), SqlError> {
    sqlx::query("UPDATE platform.credentials SET last_used_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool.pool())
        .await
        .map_err(SqlError::from)?;
    Ok(())
}
