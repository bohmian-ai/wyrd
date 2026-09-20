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

/// Advance a user's authorization epoch to the start of the current second.
///
/// The federated sign-in path calls this when the provider's asserted role set
/// no longer matches the persisted one. Access tokens carry role names in
/// signed claims, so a removed role stays spendable until the epoch moves past
/// the tokens that name it.
///
/// The truncation is what makes the successor usable. Access-token `iat` is
/// whole seconds, so an epoch carrying sub-second precision could land ahead of
/// a token issued moments later in the same transaction and revoke the session
/// the login just established. `date_trunc` to the second removes that race in
/// the only direction it can go wrong; tokens issued in an earlier second are
/// still retired.
///
/// Returns `Ok(true)` when a row was updated, `Ok(false)` when no row matched.
///
/// # Errors
/// Returns a SQLx error on database failure.
pub async fn advance_user_epoch_to_second(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE wyrd.auth_users
           SET tokens_not_before = date_trunc('second', now())
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
