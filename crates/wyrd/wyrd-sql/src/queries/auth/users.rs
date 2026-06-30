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
        r#"
        INSERT INTO wyrd.auth_users (
            id, data_tenant_id, email, password_hash, auth_type, status
        ) VALUES ($1, $2, $3, $4, $5, 'active')
        "#,
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
        r#"
        SELECT id, email, auth_type, status, created_at, updated_at
          FROM wyrd.auth_users
         WHERE data_tenant_id = wyrd.current_tenant()
           AND id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// Look up a user by email.
///
/// Email is unique per tenant.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query or row decoding fails.
pub async fn user_by_email(
    conn: &mut TenantConn<'_>,
    email: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(
        r#"
        SELECT id, email, auth_type, status, created_at, updated_at
          FROM wyrd.auth_users
         WHERE data_tenant_id = wyrd.current_tenant()
           AND email = $1
        "#,
    )
    .bind(email)
    .fetch_optional(&mut **conn.transaction())
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
    const INSERT_USER_SQL: &str = r#"
        INSERT INTO wyrd.auth_users (
            id, data_tenant_id, email, password_hash, auth_type, status
        ) VALUES ($1, $2, $3, $4, $5, 'active')
        "#;
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
