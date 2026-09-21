//! Principal revocation epoch queries.
//!
//! The admission reads run on every token verify and are never cached; they
//! are primary-key lookups that Postgres serves from shared_buffers.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use sqlx::Error as SqlxError;
use sqlx::types::Uuid;
use wyrd_spec::DataTenantId;

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

/// Tenant admission and one principal's authorization epoch, read together.
///
/// The token verifier needs both answers on every request, so they are
/// returned from one statement over one tenant connection rather than two
/// round trips. Neither value is cached anywhere: a revocation or a tenant
/// suspension committed before the read is visible to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrincipalAdmission {
    /// Whether the tenant's lifecycle state admits credentials at all.
    pub tenant_admits: bool,
    /// The principal's `tokens_not_before`; `None` when the principal does not
    /// exist or has never been revoked.
    pub tokens_not_before: Option<DateTime<Utc>>,
}

/// Read tenant admission and a user principal's authorization epoch in one
/// statement.
///
/// # Errors
/// Returns a SQLx error on database failure. A failure is not a refusal: the
/// caller must fail closed.
pub async fn user_admission(
    conn: &mut TenantConn<'_>,
    tenant: DataTenantId,
    id: Uuid,
) -> Result<PrincipalAdmission, SqlxError> {
    read_admission(
        conn,
        tenant,
        id,
        r#"
        SELECT platform.tenant_admits_credentials($1),
               (SELECT tokens_not_before
                  FROM wyrd.auth_users
                 WHERE data_tenant_id = wyrd.current_tenant()
                   AND id = $2)
        "#,
    )
    .await
}

/// Read tenant admission and a service-account-backed principal's
/// (`tenant_admin`, `service`, or `agent`) authorization epoch in one
/// statement.
///
/// # Errors
/// Returns a SQLx error on database failure. A failure is not a refusal: the
/// caller must fail closed.
pub async fn service_account_admission(
    conn: &mut TenantConn<'_>,
    tenant: DataTenantId,
    id: Uuid,
) -> Result<PrincipalAdmission, SqlxError> {
    read_admission(
        conn,
        tenant,
        id,
        r#"
        SELECT platform.tenant_admits_credentials($1),
               (SELECT tokens_not_before
                  FROM wyrd.auth_service_accounts
                 WHERE data_tenant_id = wyrd.current_tenant()
                   AND id = $2)
        "#,
    )
    .await
}

/// Run one admission-and-epoch statement whose first column is the tenant
/// admission verdict and whose second is the principal's epoch.
///
/// # Errors
/// Returns a SQLx error on database failure.
async fn read_admission(
    conn: &mut TenantConn<'_>,
    tenant: DataTenantId,
    id: Uuid,
    sql: &'static str,
) -> Result<PrincipalAdmission, SqlxError> {
    let (tenant_admits, tokens_not_before): (bool, Option<DateTime<Utc>>) = sqlx::query_as(sql)
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_one(&mut **conn.transaction())
        .await?;
    Ok(PrincipalAdmission {
        tenant_admits,
        tokens_not_before,
    })
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
) -> Result<Option<DateTime<Utc>>, SqlxError> {
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
