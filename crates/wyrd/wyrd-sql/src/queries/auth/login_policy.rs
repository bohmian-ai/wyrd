//! A tenant's login policy: whether company SSO is its only human sign-in
//! method. Forced RLS selects the tenant.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use crate::{SqlError, TenantConn};

/// Whether the RLS tenant requires company SSO; `false` when it has no
/// policy.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn requires_company_sso(conn: &mut TenantConn<'_>) -> Result<bool, SqlError> {
    sqlx::query_scalar::<_, bool>("SELECT require_company_sso FROM wyrd.auth_login_policies")
        .fetch_optional(&mut **conn.transaction())
        .await
        .map(|required| required.unwrap_or(false))
        .map_err(SqlError::from)
}
