//! Query slots for `platform.principal_grants`.
//!
//! Authority lives here rather than on the principal row, so issuing, rotating,
//! or revoking a credential never changes what a principal may do. The stored
//! payload is the same JSONB permission projection `wyrd.auth_roles.permissions`
//! uses, so one permission vocabulary and one synchronous checker serve the
//! platform and tenant planes alike.
// raw-query grep allowlist: platform administrative tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use serde_json::Value;
use sqlx::types::Uuid;

use crate::{OperatorPool, SqlError, TenantConn};

/// Replace a platform principal's granted permission set.
///
/// Grants are stored as one row per principal, so this upserts rather than
/// accumulating. The caller owns the permission payload and its validation.
///
/// # Errors
/// Returns [`SqlError::Query`] when the write fails, including when
/// `principal_id` does not name an existing platform principal.
pub async fn set_platform_grant_tx(
    conn: &mut TenantConn<'_>,
    principal_id: Uuid,
    permissions: &Value,
) -> Result<(), SqlError> {
    sqlx::query(SET_PLATFORM_GRANT_SQL)
        .bind(principal_id)
        .bind(permissions)
        .execute(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

/// The one grant upsert, shared by every entry point so they cannot drift.
const SET_PLATFORM_GRANT_SQL: &str =
    "INSERT INTO platform.principal_grants (principal_id, permissions)
     VALUES ($1, $2)
     ON CONFLICT (principal_id)
     DO UPDATE SET permissions = EXCLUDED.permissions, updated_at = now()";

/// Install or replace a platform principal's grant.
///
/// # Errors
/// Returns [`SqlError::Query`] when the upsert fails.
pub async fn set_platform_grant(
    pool: &OperatorPool,
    principal_id: Uuid,
    permissions: &Value,
) -> Result<(), SqlError> {
    sqlx::query(SET_PLATFORM_GRANT_SQL)
        .bind(principal_id)
        .bind(permissions)
        .execute(pool.pool())
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

/// Read a platform principal's granted permission set.
///
/// Returns `None` when the principal holds no grant, which denies every
/// platform operation: absence is deny, and there is no implicit authority.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn platform_grant_for_principal(
    pool: &OperatorPool,
    principal_id: Uuid,
) -> Result<Option<Value>, SqlError> {
    sqlx::query_scalar::<_, Value>(
        "SELECT permissions FROM platform.principal_grants WHERE principal_id = $1",
    )
    .bind(principal_id)
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}
