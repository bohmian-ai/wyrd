//! A tenant's login policy: whether company SSO is its only human sign-in
//! method. Forced RLS selects the tenant.
//!
//! [`set_login_policy`] changes the policy at an expected revision and, in
//! the same transaction, [`revoke_non_company_credentials`] ends every
//! password and social browser session, password-anchored refresh family and
//! password-approved device authorization. Each change sweeps, so a
//! credential created while a change was committing never outlives the next
//! one, and disabling the policy revives nothing.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use crate::{SqlError, TenantConn};

/// The tenant's policy row, absent until its first change.
const POLICY_SQL: &str = "SELECT require_company_sso, revision FROM wyrd.auth_login_policies";

/// Lock the tenant's policy row, if it has one, for a change.
const LOCK_POLICY_SQL: &str =
    "SELECT require_company_sso, revision FROM wyrd.auth_login_policies FOR UPDATE";

/// Write the policy at the next revision; the first change creates the row
/// at revision 1.
const WRITE_POLICY_SQL: &str = r"
    INSERT INTO wyrd.auth_login_policies (data_tenant_id, require_company_sso, revision)
    VALUES (wyrd.current_tenant(), $1, 1)
    ON CONFLICT (data_tenant_id) DO UPDATE
       SET require_company_sso = EXCLUDED.require_company_sso,
           revision = wyrd.auth_login_policies.revision + 1,
           updated_at = statement_timestamp()
    RETURNING revision
";

/// Active Users with no identity at `$1`, the Active connection's issuer.
const USERS_WITHOUT_IDENTITY_SQL: &str = r"
    SELECT count(*) FROM wyrd.auth_users u
     WHERE u.status = 'active'
       AND NOT EXISTS (
           SELECT 1 FROM wyrd.auth_user_identities i
            WHERE i.data_tenant_id = u.data_tenant_id
              AND i.user_id = u.id
              AND i.issuer = $1)
";

/// End every live password or social browser session.
const REVOKE_SESSIONS_SQL: &str = r"
    UPDATE wyrd.auth_browser_sessions
       SET revoked_at = statement_timestamp()
     WHERE revoked_at IS NULL
       AND (password_revision IS NOT NULL OR social_provider IS NOT NULL)
";

/// Retire every live password-anchored refresh token.
const REVOKE_REFRESH_SQL: &str = r"
    UPDATE wyrd.auth_refresh_tokens
       SET revoked_at = statement_timestamp(),
           revoked_reason = 'login_policy_changed'
     WHERE revoked_at IS NULL
       AND password_revision IS NOT NULL
";

/// Drop every device authorization a password sign-in approved, with any
/// login state bound to it.
const REVOKE_DEVICES_SQL: &str = r"
    WITH device AS (
        DELETE FROM wyrd.auth_device_authorizations
         WHERE password_revision IS NOT NULL
        RETURNING device_id
    )
    DELETE FROM wyrd.auth_login_state
     WHERE device_id IN (SELECT device_id FROM device)
";

/// The tenant's login policy as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct LoginPolicyRow {
    /// Company SSO is the only human sign-in method.
    pub require_company_sso: bool,
    /// The policy revision; `0` for a tenant that never changed it.
    pub revision: i64,
}

/// Whether the RLS tenant requires company SSO; `false` when it has no
/// policy.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn requires_company_sso(conn: &mut TenantConn<'_>) -> Result<bool, SqlError> {
    login_policy(conn)
        .await
        .map(|policy| policy.require_company_sso)
}

/// The RLS tenant's login policy; not required at revision `0` when it has
/// no row.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn login_policy(conn: &mut TenantConn<'_>) -> Result<LoginPolicyRow, SqlError> {
    read_policy(conn, POLICY_SQL).await
}

/// [`login_policy`], locking the row against a concurrent change until the
/// caller's transaction ends.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn lock_login_policy(conn: &mut TenantConn<'_>) -> Result<LoginPolicyRow, SqlError> {
    read_policy(conn, LOCK_POLICY_SQL).await
}

/// Read the policy row with `sql`, defaulting a missing row.
async fn read_policy(
    conn: &mut TenantConn<'_>,
    sql: &'static str,
) -> Result<LoginPolicyRow, SqlError> {
    sqlx::query_as::<_, LoginPolicyRow>(sql)
        .fetch_optional(&mut **conn.transaction())
        .await
        .map(|row| {
            row.unwrap_or(LoginPolicyRow {
                require_company_sso: false,
                revision: 0,
            })
        })
        .map_err(SqlError::from)
}

/// Write `require_company_sso` at the next policy revision and return it.
///
/// The caller holds [`lock_login_policy`] and has checked the revision it
/// expects; concurrent first changes still serialize on the primary key.
///
/// # Errors
/// Returns [`SqlError::Query`] when the write fails.
pub async fn set_login_policy(
    conn: &mut TenantConn<'_>,
    require_company_sso: bool,
) -> Result<i64, SqlError> {
    sqlx::query_scalar::<_, i64>(WRITE_POLICY_SQL)
        .bind(require_company_sso)
        .fetch_one(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}

/// End every password and social credential of the RLS tenant: browser
/// sessions, password-anchored refresh tokens and password-approved device
/// authorizations. Company and recovery credentials are untouched.
///
/// # Errors
/// Returns [`SqlError::Query`] when a write fails.
pub async fn revoke_non_company_credentials(conn: &mut TenantConn<'_>) -> Result<(), SqlError> {
    for sql in [REVOKE_SESSIONS_SQL, REVOKE_REFRESH_SQL, REVOKE_DEVICES_SQL] {
        sqlx::query(sql)
            .execute(&mut **conn.transaction())
            .await
            .map_err(SqlError::from)?;
    }
    Ok(())
}

/// How many active Users of the RLS tenant have no identity at `issuer`.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn users_without_identity(
    conn: &mut TenantConn<'_>,
    issuer: &str,
) -> Result<i64, SqlError> {
    sqlx::query_scalar::<_, i64>(USERS_WITHOUT_IDENTITY_SQL)
        .bind(issuer)
        .fetch_one(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}
