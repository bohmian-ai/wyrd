//! Principal revocation queries.
//!
//! Revoking a principal suspends it. Status is what every tenant issuance path
//! reads before it mints, so suspension refuses the principal's next token on
//! every grant while leaving its grants, credentials, and data intact.
//! Tokens already issued lapse at their five-minute expiry.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::types::Uuid;

use crate::TenantConn;

/// Suspend a user principal in the caller's tenant.
///
/// A deleted user stays deleted; an already-suspended user is left as is and
/// still reported as found, so a repeated revocation is idempotent.
///
/// # Errors
/// Returns a SQLx error on database failure.
pub async fn suspend_user_principal(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE wyrd.auth_users
           SET status = CASE WHEN status = 'active' THEN 'suspended' ELSE status END,
               updated_at = now()
         WHERE data_tenant_id = wyrd.current_tenant()
           AND id = $1
        "#,
    )
    .bind(id)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Suspend a service or agent principal in the caller's tenant.
///
/// Same contract as [`suspend_user_principal`], on the machine table.
///
/// # Errors
/// Returns a SQLx error on database failure.
pub async fn suspend_service_account_principal(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE wyrd.auth_service_accounts
           SET status = CASE WHEN status = 'active' THEN 'suspended' ELSE status END,
               updated_at = now()
         WHERE data_tenant_id = wyrd.current_tenant()
           AND id = $1
        "#,
    )
    .bind(id)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(result.rows_affected() > 0)
}
