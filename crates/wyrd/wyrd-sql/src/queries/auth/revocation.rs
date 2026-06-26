//! Principal revocation epoch queries.
//!
//! These are hot-path reads called on every token verify. Results are cached by
//! the server-side `SqlRevocationCheck`; the queries themselves are simple
//! primary-key lookups that Postgres serves from shared_buffers.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use sqlx::types::Uuid;

use crate::TenantConn;

/// Return `tokens_not_before` for a user principal.
///
/// Returns `None` when the user does not exist or has never been revoked.
///
/// # Errors
/// Returns a SQLx error on database failure.
pub async fn user_revocation_epoch(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    let row: Option<(Option<DateTime<Utc>>,)> = sqlx::query_as(
        r#"
        SELECT tokens_not_before
          FROM wyrd.auth_users
         WHERE data_tenant_id = wyrd.current_tenant()
           AND id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&mut **conn.transaction())
    .await?;

    Ok(row.and_then(|(ts,)| ts))
}

/// Return `tokens_not_before` for a service account or agent principal.
///
/// Returns `None` when the service account does not exist or has never been revoked.
///
/// # Errors
/// Returns a SQLx error on database failure.
pub async fn service_account_revocation_epoch(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    let row: Option<(Option<DateTime<Utc>>,)> = sqlx::query_as(
        r#"
        SELECT tokens_not_before
          FROM wyrd.auth_service_accounts
         WHERE data_tenant_id = wyrd.current_tenant()
           AND id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&mut **conn.transaction())
    .await?;

    Ok(row.and_then(|(ts,)| ts))
}

/// Bump `tokens_not_before = now()` for a user, instantly revoking all live tokens.
///
/// Returns `Ok(true)` when a row was updated, `Ok(false)` when no row matched.
///
/// # Errors
/// Returns a SQLx error on database failure.
pub async fn revoke_user_principal(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE wyrd.auth_users
           SET tokens_not_before = now()
         WHERE data_tenant_id = wyrd.current_tenant()
           AND id = $1
        "#,
    )
    .bind(id)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Bump `tokens_not_before = now()` for a service account or agent.
///
/// Returns `Ok(true)` when a row was updated, `Ok(false)` when no row matched.
///
/// # Errors
/// Returns a SQLx error on database failure.
pub async fn revoke_service_account_principal(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE wyrd.auth_service_accounts
           SET tokens_not_before = now()
         WHERE data_tenant_id = wyrd.current_tenant()
           AND id = $1
        "#,
    )
    .bind(id)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(result.rows_affected() > 0)
}
