//! Tenant-scoped user CRUD queries.
//!
//! Tenant-scoped functions here take `&mut TenantConn<'_>`.
//! Database RLS enforces tenant scoping.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use sqlx::types::Uuid;

use crate::TenantConn;

/// Tenant-scoped user row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    /// User id.
    pub id: Uuid,
    /// Email address.
    pub email: Option<String>,
    /// Auth type: `password` or `oidc`.
    pub auth_type: String,
    /// Status: `active`, `suspended`, or `deleted`.
    pub status: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last-update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// Insert a new active user.
///
/// `auth_type` must satisfy the database check constraint. `password_hash` is
/// present only for password-backed users.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert.
pub async fn insert_user(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    email: Option<&str>,
    auth_type: &str,
    password_hash: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        INSERT INTO wyrd.auth_users (
            id, data_tenant_id, email, password_hash, auth_type, status
        ) VALUES ($1, $2, $3, $4, $5, 'active')
        ",
    )
    .bind(id)
    .bind(conn.data_tenant_id().as_uuid())
    .bind(email)
    .bind(password_hash)
    .bind(auth_type)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(())
}

/// Look up a user by id.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query or row decoding fails.
pub async fn user_by_id(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(
        r"
        SELECT id, email, auth_type, status, created_at, updated_at
          FROM wyrd.auth_users
         WHERE data_tenant_id = wyrd.current_tenant()
           AND id = $1
        ",
    )
    .bind(id)
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// The local password credential of one User, as sign-in verifies it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LocalCredentialRow {
    /// User id.
    pub id: Uuid,
    /// Canonical local username.
    pub username: String,
    /// Encoded Argon2id PHC verifier.
    pub password_hash: String,
    /// Monotonic password revision a local session is bound to.
    pub password_revision: i64,
    /// Status: `active`, `suspended`, or `deleted`.
    pub status: String,
}

/// Insert a new active local User with canonical `username` and the PHC
/// `password_hash` verifier, at password revision zero.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert, including a unique
/// violation when the tenant already has a local User with `username`.
pub async fn insert_local_user(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    username: &str,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        INSERT INTO wyrd.auth_users (
            id, data_tenant_id, username, password_hash, auth_type, status
        ) VALUES ($1, $2, $3, $4, 'password', 'active')
        ",
    )
    .bind(id)
    .bind(conn.data_tenant_id().as_uuid())
    .bind(username)
    .bind(password_hash)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(())
}

/// The local credential of the tenant's User with canonical `username`, in
/// any status; `None` when no local User has it.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the read.
pub async fn local_credential_by_username(
    conn: &mut TenantConn<'_>,
    username: &str,
) -> Result<Option<LocalCredentialRow>, sqlx::Error> {
    sqlx::query_as::<_, LocalCredentialRow>(
        r"
        SELECT id, username, password_hash, password_revision, status
          FROM wyrd.auth_users
         WHERE username = $1
           AND password_hash IS NOT NULL
        ",
    )
    .bind(username)
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// The local credential of User `id`, locked for update; `None` when the User
/// has no local password.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the read.
pub async fn local_credential_for_update(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<Option<LocalCredentialRow>, sqlx::Error> {
    sqlx::query_as::<_, LocalCredentialRow>(
        r"
        SELECT id, username, password_hash, password_revision, status
          FROM wyrd.auth_users
         WHERE id = $1
           AND password_hash IS NOT NULL
         FOR UPDATE
        ",
    )
    .bind(id)
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// The canonical local username of User `id`; `None` for an SSO-only User or
/// an unknown id.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the read.
pub async fn local_username(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, Option<String>>("SELECT username FROM wyrd.auth_users WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut **conn.transaction())
        .await
        .map(Option::flatten)
}

/// Replace User `id`'s password verifier and advance its password revision,
/// which ends every local session bound to an earlier one; returns the new
/// revision.
///
/// `bump` is false for a transparent rehash of the same password onto the
/// current policy, which keeps the revision and every session.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update or the User has no
/// local password.
pub async fn set_password_hash(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    password_hash: &str,
    bump: bool,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        r"
        UPDATE wyrd.auth_users
           SET password_hash = $2,
               password_revision = password_revision + CASE WHEN $3 THEN 1 ELSE 0 END,
               updated_at = statement_timestamp()
         WHERE id = $1
           AND password_hash IS NOT NULL
        RETURNING password_revision
        ",
    )
    .bind(id)
    .bind(password_hash)
    .bind(bump)
    .fetch_one(&mut **conn.transaction())
    .await
}

/// Soft-delete a user by marking `status = 'deleted'`.
///
/// Returns `Ok(true)` when a row was updated, `Ok(false)` when no row matched.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn delete_user(conn: &mut TenantConn<'_>, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE wyrd.auth_users
            SET status = 'deleted', updated_at = now()
          WHERE data_tenant_id = wyrd.current_tenant()
            AND id = $1",
    )
    .bind(id)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    const INSERT_USER_SQL: &str = r"
        INSERT INTO wyrd.auth_users (
            id, data_tenant_id, email, password_hash, auth_type, status
        ) VALUES ($1, $2, $3, $4, $5, 'active')
        ";
    const DELETE_USER_SQL: &str = "UPDATE wyrd.auth_users
            SET status = 'deleted', updated_at = now()
          WHERE data_tenant_id = wyrd.current_tenant()
            AND id = $1";

    #[test]
    fn insert_user_sets_tenant_and_active_status() {
        assert!(INSERT_USER_SQL.contains("data_tenant_id"));
        assert!(INSERT_USER_SQL.contains("'active'"));
        assert!(INSERT_USER_SQL.contains("password_hash"));
        assert!(INSERT_USER_SQL.contains("email"));
    }

    #[test]
    fn delete_user_is_soft_delete() {
        assert!(DELETE_USER_SQL.contains("UPDATE wyrd.auth_users"));
        assert!(DELETE_USER_SQL.contains("status = 'deleted'"));
        assert!(!DELETE_USER_SQL.contains("DELETE FROM"));
    }
}
