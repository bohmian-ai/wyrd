//! Tenant-scoped login state queries for OIDC authorization-code flow.
//!
//! Functions here take `&mut TenantConn<'_>` and rely on Postgres RLS.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use crate::TenantConn;
use chrono::{DateTime, Utc};

const INSERT_LOGIN_STATE_SQL: &str = r#"
    INSERT INTO wyrd.auth_login_state (
        data_tenant_id, state, code_verifier, nonce, issuer, redirect_uri, expires_at
    ) VALUES ($1, $2, $3, $4, $5, $6, $7)
"#;

const TAKE_LOGIN_STATE_SQL: &str = r#"
    DELETE FROM wyrd.auth_login_state
     WHERE data_tenant_id = $1
       AND state = $2
       AND expires_at > now()
    RETURNING code_verifier, nonce, issuer, redirect_uri
"#;

/// Row returned by a single-use login-state consume.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LoginStateRow {
    /// Server-generated PKCE verifier.
    pub code_verifier: String,
    /// Server-generated nonce.
    pub nonce: String,
    /// Trusted issuer URL.
    pub issuer: String,
    /// Callback redirect URI.
    pub redirect_uri: String,
}

/// Insert a login-state row.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert.
pub async fn insert_login_state(
    conn: &mut TenantConn<'_>,
    state: &str,
    code_verifier: &str,
    nonce: &str,
    issuer: &str,
    redirect_uri: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(INSERT_LOGIN_STATE_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(state)
        .bind(code_verifier)
        .bind(nonce)
        .bind(issuer)
        .bind(redirect_uri)
        .bind(expires_at)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}

/// Consume a login-state row exactly once.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn take_login_state(
    conn: &mut TenantConn<'_>,
    state: &str,
) -> Result<Option<LoginStateRow>, sqlx::Error> {
    sqlx::query_as::<_, LoginStateRow>(TAKE_LOGIN_STATE_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(state)
        .fetch_optional(&mut **conn.transaction())
        .await
}

#[cfg(test)]
mod tests {
    use super::{INSERT_LOGIN_STATE_SQL, TAKE_LOGIN_STATE_SQL};

    #[test]
    fn login_state_insert_and_take_queries_are_tenant_scoped() {
        assert!(INSERT_LOGIN_STATE_SQL.contains("data_tenant_id"));
        assert!(INSERT_LOGIN_STATE_SQL.contains("expires_at"));
        assert!(TAKE_LOGIN_STATE_SQL.contains("DELETE FROM wyrd.auth_login_state"));
        assert!(TAKE_LOGIN_STATE_SQL.contains("expires_at > now()"));
        assert!(TAKE_LOGIN_STATE_SQL.contains("RETURNING code_verifier"));
    }
}
