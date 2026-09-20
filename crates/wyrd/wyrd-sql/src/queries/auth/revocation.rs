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

/// Advance a user's authorization epoch to the start of the next whole second
/// and return the value it was set to.
///
/// The federated sign-in path calls this when the provider's asserted role set
/// no longer matches the persisted one. Access tokens carry role names in
/// signed claims, so a removed role stays spendable until the epoch moves past
/// the tokens that name it.
///
/// The epoch has to land on a whole second, because `iat` is whole seconds and
/// a sub-second epoch would retire tokens by rounding rather than by order. The
/// *next* second is what makes the withdrawal complete: truncating to the
/// current second leaves a token minted earlier in that same second with
/// `iat == epoch`, and the verifier retires a token only when `iat` is strictly
/// older, so that predecessor would keep spending the role the provider just
/// withdrew. The caller mints the successor at exactly the returned instant, so
/// it is admitted while every earlier token is not.
///
/// Returns the stored epoch, or `None` when no row matched.
///
/// # Errors
/// Returns a SQLx error on database failure.
pub async fn advance_user_epoch_to_next_second(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        UPDATE wyrd.auth_users
           SET tokens_not_before = date_trunc('second', now()) + interval '1 second'
         WHERE data_tenant_id = wyrd.current_tenant()
           AND id = $1
        RETURNING tokens_not_before
        "#,
    )
    .bind(id)
    .fetch_optional(&mut **conn.transaction())
    .await
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
