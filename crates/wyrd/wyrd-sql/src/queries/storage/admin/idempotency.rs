//! Cross-tenant cleanup for expired storage idempotency rows.
// raw-query grep allowlist: storage tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use crate::error::SqlError;
use sqlx::PgPool;

/// Delete expired idempotency rows across tenants.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the cleanup query.
pub async fn reap_idempotency_keys(admin_pool: &PgPool, batch_size: i64) -> Result<u64, SqlError> {
    let result = sqlx::query(
        r#"
        WITH expired AS (
            SELECT data_tenant_id, idempotency_key
            FROM wyrd.storage_idempotency_keys
            WHERE expires_at < now()
            ORDER BY expires_at
            LIMIT $1
        )
        DELETE FROM wyrd.storage_idempotency_keys target
        USING expired
        WHERE target.data_tenant_id = expired.data_tenant_id
          AND target.idempotency_key = expired.idempotency_key
        "#,
    )
    .bind(batch_size)
    .execute(admin_pool)
    .await
    .map_err(SqlError::from)?;

    Ok(result.rows_affected())
}
