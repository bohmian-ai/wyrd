//! Tenant-scoped API-key mutation queries.
//!
//! Tenant-scoped functions here take `&mut TenantConn<'_>`.

use sqlx::types::Uuid;

use crate::TenantConn;

const REVOKE_API_KEY_SQL: &str = "UPDATE wyrd.auth_api_keys
            SET revoked_at = now()
          WHERE id = $1
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
