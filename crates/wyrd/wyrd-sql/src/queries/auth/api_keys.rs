//! Tenant-scoped API-key mutation queries.
//!
//! Tenant-scoped functions here take `&mut TenantConn<'_>`.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use sqlx::types::Uuid;

use crate::TenantConn;

const REVOKE_API_KEY_SQL: &str = "UPDATE wyrd.auth_api_keys
            SET revoked_at = now()
          WHERE data_tenant_id = wyrd.current_tenant()
            AND id = $1
            AND revoked_at IS NULL";

/// Mark an API key revoked.
///
/// This helper is idempotent. Re-revoking a revoked key leaves the original
/// `revoked_at` untouched and returns `Ok(false)`.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn revoke_api_key(
    conn: &mut TenantConn<'_>,
    api_key_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(REVOKE_API_KEY_SQL)
        .bind(api_key_id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Non-secret credential metadata for one tenant-scope principal.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApiKeyMetadataRow {
    /// Credential id, used to revoke it.
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

/// List a principal's credential metadata, newest first.
///
/// Returns no secret material. Listing exists so an operator can see what to
/// rotate or revoke, never to recover a credential: only the verifier is
/// stored, so there is nothing to return even in principle.
///
/// Revoked and expired credentials are included, because an operator rotating
/// needs to see that the superseded one really is gone.
///
/// # Errors
/// Returns the database error when the read fails.
pub async fn list_api_key_metadata(
    conn: &mut TenantConn<'_>,
    principal_id: Uuid,
) -> Result<Vec<ApiKeyMetadataRow>, sqlx::Error> {
    sqlx::query_as::<_, ApiKeyMetadataRow>(
        r#"
        SELECT id, prefix, created_at, expires_at, revoked_at, last_used_at
          FROM wyrd.auth_api_keys
         WHERE data_tenant_id = $1
           AND principal_id = $2
         ORDER BY created_at DESC
        "#,
    )
    .bind(conn.data_tenant_id().as_uuid())
    .bind(principal_id)
    .fetch_all(&mut **conn.transaction())
    .await
}

/// Report whether a credential belongs to a principal, inside this tenant.
///
/// Revocation is addressed by credential id, which is unique on its own, so
/// without this check a caller could name any principal in the path and still
/// revoke a credential belonging to another one. Row-level security already
/// confines that to the caller's own tenant; this confines it to the principal
/// the request actually named.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the read.
pub async fn credential_belongs_to(
    conn: &mut TenantConn<'_>,
    credential_id: Uuid,
    principal_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let found: Option<Uuid> = sqlx::query_scalar(
        "SELECT id
           FROM wyrd.auth_api_keys
          WHERE data_tenant_id = wyrd.current_tenant()
            AND id = $1
            AND principal_id = $2",
    )
    .bind(credential_id)
    .bind(principal_id)
    .fetch_optional(&mut **conn.transaction())
    .await?;
    Ok(found.is_some())
}

#[cfg(test)]
mod tests {
    use super::REVOKE_API_KEY_SQL;

    #[test]
    fn revoke_api_key_is_idempotent() {
        assert!(REVOKE_API_KEY_SQL.contains("UPDATE wyrd.auth_api_keys"));
        assert!(REVOKE_API_KEY_SQL.contains("SET revoked_at = now()"));
        assert!(REVOKE_API_KEY_SQL.contains("revoked_at IS NULL"));
        assert!(!REVOKE_API_KEY_SQL.contains("expires_at"));
    }
}
