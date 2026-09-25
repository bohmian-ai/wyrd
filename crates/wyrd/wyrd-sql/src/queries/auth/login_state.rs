//! Tenant-scoped login state queries for OIDC authorization-code flow.
//!
//! Functions here take `&mut TenantConn<'_>` and rely on Postgres RLS.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use crate::TenantConn;
use crate::row_types::auth::HumanConnectionBinding;

const INSERT_LOGIN_STATE_SQL: &str = r#"
    INSERT INTO wyrd.auth_login_state (
        data_tenant_id, state, code_verifier, nonce, issuer, redirect_uri, expires_at,
        connection_id, connection_revision
    ) VALUES ($1, $2, $3, $4, $5, $6,
              statement_timestamp() + ($7 * interval '1 second'), $8, $9)
"#;

const TAKE_LOGIN_STATE_SQL: &str = r#"
    DELETE FROM wyrd.auth_login_state
     WHERE data_tenant_id = $1
       AND state = $2
       AND expires_at > statement_timestamp()
    RETURNING code_verifier, nonce, issuer, redirect_uri, connection_id, connection_revision
"#;

/// One login-state row: written when a login begins and returned by its
/// single-use consume.
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
    /// The exact connection revision the login began through.
    #[sqlx(flatten)]
    pub connection: HumanConnectionBinding,
}

/// Insert a login-state row whose expiry `PostgreSQL` derives from `ttl`.
///
/// The caller binds a lifetime, never an absolute instant, so the row's expiry
/// and the consume predicate that evaluates it share one clock. The row also
/// binds the exact connection id and revision the login began through, which
/// the callback requires to still be Active when it issues the session.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert.
pub async fn insert_login_state(
    conn: &mut TenantConn<'_>,
    state: &str,
    row: &LoginStateRow,
    ttl: Duration,
) -> Result<(), sqlx::Error> {
    sqlx::query(INSERT_LOGIN_STATE_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(state)
        .bind(&row.code_verifier)
        .bind(&row.nonce)
        .bind(&row.issuer)
        .bind(&row.redirect_uri)
        .bind(ttl.as_secs_f64())
        .bind(row.connection.connection_id)
        .bind(row.connection.connection_revision)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}

/// Consume a login-state row exactly once, using database time for expiry.
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

    /// Both statements stay tenant-bound and let `PostgreSQL` own expiry.
    #[test]
    fn login_state_insert_and_take_queries_are_tenant_scoped() {
        assert!(INSERT_LOGIN_STATE_SQL.contains("data_tenant_id"));
        assert!(INSERT_LOGIN_STATE_SQL.contains("expires_at"));
        assert!(TAKE_LOGIN_STATE_SQL.contains("DELETE FROM wyrd.auth_login_state"));
        assert!(TAKE_LOGIN_STATE_SQL.contains("expires_at > statement_timestamp()"));
        assert!(TAKE_LOGIN_STATE_SQL.contains("RETURNING code_verifier"));
    }
}
