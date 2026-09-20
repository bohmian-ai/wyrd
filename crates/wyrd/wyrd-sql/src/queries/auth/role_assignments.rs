//! Tenant-scoped role-assignment queries.
//!
//! Functions take `&mut TenantConn<'_>` and rely on database RLS for tenant
//! scoping.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::types::Uuid;

use crate::TenantConn;

const GRANT_ROLE_TO_USER_SQL: &str = r#"
        INSERT INTO wyrd.auth_user_roles (data_tenant_id, user_id, role_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (data_tenant_id, user_id, role_id) DO NOTHING
        "#;

const REVOKE_ROLE_FROM_USER_SQL: &str = "DELETE FROM wyrd.auth_user_roles
           WHERE data_tenant_id = wyrd.current_tenant()
             AND user_id = $1
             AND role_id = $2";

const LIST_USER_ROLES_SQL: &str = r#"
        SELECT r.name
          FROM wyrd.auth_user_roles ur
          JOIN wyrd.auth_roles r
            ON r.data_tenant_id = ur.data_tenant_id
           AND r.id = ur.role_id
         WHERE ur.data_tenant_id = wyrd.current_tenant()
           AND ur.user_id = $1
         ORDER BY r.name
        "#;

const REPLACE_USER_ROLES_SQL: &str = r#"
        WITH wanted AS (
            SELECT id
              FROM wyrd.auth_roles
             WHERE data_tenant_id = wyrd.current_tenant()
               AND name = ANY($2)
        ), removed AS (
            DELETE FROM wyrd.auth_user_roles
             WHERE data_tenant_id = wyrd.current_tenant()
               AND user_id = $1
               AND role_id NOT IN (SELECT id FROM wanted)
        )
        INSERT INTO wyrd.auth_user_roles (data_tenant_id, user_id, role_id)
        SELECT wyrd.current_tenant(), $1, id FROM wanted
        ON CONFLICT (data_tenant_id, user_id, role_id) DO NOTHING
        "#;

const GRANT_ROLE_TO_SERVICE_ACCOUNT_SQL: &str = r#"
        INSERT INTO wyrd.auth_service_account_roles (
            data_tenant_id, service_account_id, role_id
        ) VALUES ($1, $2, $3)
        ON CONFLICT (data_tenant_id, service_account_id, role_id) DO NOTHING
        "#;

const REVOKE_ROLE_FROM_SERVICE_ACCOUNT_SQL: &str = "DELETE FROM wyrd.auth_service_account_roles
           WHERE data_tenant_id = wyrd.current_tenant()
             AND service_account_id = $1
             AND role_id = $2";

const LIST_SERVICE_ACCOUNT_ROLES_SQL: &str = r#"
        SELECT r.name
          FROM wyrd.auth_service_account_roles sar
          JOIN wyrd.auth_roles r
            ON r.data_tenant_id = sar.data_tenant_id
           AND r.id = sar.role_id
         WHERE sar.data_tenant_id = wyrd.current_tenant()
           AND sar.service_account_id = $1
         ORDER BY r.name
        "#;

/// Grant a role to a user.
///
/// Returns `Ok(true)` when a row was inserted.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert.
pub async fn grant_role_to_user(
    conn: &mut TenantConn<'_>,
    user_id: Uuid,
    role_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(GRANT_ROLE_TO_USER_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(user_id)
        .bind(role_id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Revoke a role from a user.
///
/// Returns `Ok(true)` when a row was deleted.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn revoke_role_from_user(
    conn: &mut TenantConn<'_>,
    user_id: Uuid,
    role_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(REVOKE_ROLE_FROM_USER_SQL)
        .bind(user_id)
        .bind(role_id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Make a user's granted roles exactly the named set.
///
/// The federated sign-in path owns this: a human's authority is asserted by
/// their identity provider on every login, and nothing else in Wyrd writes
/// `wyrd.auth_user_roles`, so the login result is the whole truth for that
/// user. Persisting it is what lets a later refresh rotation re-read the roles
/// the session actually holds instead of minting an authority-free successor.
///
/// Names with no `wyrd.auth_roles` row are dropped rather than stored. Such a
/// name resolves to no permission anywhere in Wyrd, so keeping it would record
/// authority that does not exist. Roles the user holds and the new set omits
/// are removed in the same statement, which is how a provider-side revocation
/// reaches Wyrd.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the statement.
pub async fn replace_user_roles(
    conn: &mut TenantConn<'_>,
    user_id: Uuid,
    role_names: &[&str],
) -> Result<(), sqlx::Error> {
    sqlx::query(REPLACE_USER_ROLES_SQL)
        .bind(user_id)
        .bind(role_names)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}

/// List role names granted to a user, ordered by name.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn list_user_roles(
    conn: &mut TenantConn<'_>,
    user_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(LIST_USER_ROLES_SQL)
        .bind(user_id)
        .fetch_all(&mut **conn.transaction())
        .await
}

/// Grant a role to a service account.
///
/// Returns `Ok(true)` when a row was inserted.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert.
pub async fn grant_role_to_service_account(
    conn: &mut TenantConn<'_>,
    service_account_id: Uuid,
    role_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(GRANT_ROLE_TO_SERVICE_ACCOUNT_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(service_account_id)
        .bind(role_id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Revoke a role from a service account.
///
/// Returns `Ok(true)` when a row was deleted.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn revoke_role_from_service_account(
    conn: &mut TenantConn<'_>,
    service_account_id: Uuid,
    role_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(REVOKE_ROLE_FROM_SERVICE_ACCOUNT_SQL)
        .bind(service_account_id)
        .bind(role_id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// List role names granted to a service account, ordered by name.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn list_service_account_roles(
    conn: &mut TenantConn<'_>,
    service_account_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(LIST_SERVICE_ACCOUNT_ROLES_SQL)
        .bind(service_account_id)
        .fetch_all(&mut **conn.transaction())
        .await
}

#[cfg(test)]
mod tests {
    use super::{
        GRANT_ROLE_TO_SERVICE_ACCOUNT_SQL, GRANT_ROLE_TO_USER_SQL, LIST_SERVICE_ACCOUNT_ROLES_SQL,
        LIST_USER_ROLES_SQL, REVOKE_ROLE_FROM_SERVICE_ACCOUNT_SQL, REVOKE_ROLE_FROM_USER_SQL,
    };

    #[test]
    fn grants_are_idempotent() {
        assert!(
            GRANT_ROLE_TO_USER_SQL
                .contains("ON CONFLICT (data_tenant_id, user_id, role_id) DO NOTHING")
        );
        assert!(
            GRANT_ROLE_TO_SERVICE_ACCOUNT_SQL
                .contains("ON CONFLICT (data_tenant_id, service_account_id, role_id) DO NOTHING")
        );
    }

    #[test]
    fn revokes_target_join_tables_without_transaction_control() {
        assert!(REVOKE_ROLE_FROM_USER_SQL.contains("DELETE FROM wyrd.auth_user_roles"));
        assert!(
            REVOKE_ROLE_FROM_SERVICE_ACCOUNT_SQL
                .contains("DELETE FROM wyrd.auth_service_account_roles")
        );
        assert!(!REVOKE_ROLE_FROM_USER_SQL.contains("COMMIT"));
        assert!(!REVOKE_ROLE_FROM_SERVICE_ACCOUNT_SQL.contains("COMMIT"));
    }

    #[test]
    fn list_queries_join_roles_for_names() {
        for sql in [LIST_USER_ROLES_SQL, LIST_SERVICE_ACCOUNT_ROLES_SQL] {
            assert!(sql.contains("SELECT r.name"));
            assert!(sql.contains("JOIN wyrd.auth_roles r"));
            assert!(sql.contains("ORDER BY r.name"));
        }
    }
}
