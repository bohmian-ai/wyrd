//! Queries for `wyrd.auth_roles` and `wyrd.auth_user_roles`.
//!
//! Tenant-scoped functions here take `&mut TenantConn<'_>`.

use serde_json::Value;
use sqlx::Row;

use crate::TenantConn;

const ROLES_BY_NAME_SQL: &str = r#"
SELECT name, permissions
  FROM wyrd.auth_roles
 WHERE name = ANY($1)
"#;

/// Raw role row used by server-tier permission resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleRow {
    /// Role name.
    pub name: String,
    /// Raw JSONB permissions payload.
    pub permissions: Value,
}

/// Look up auth role permission payloads by role name.
///
/// Returns the subset of requested names present in `wyrd.auth_roles`.
/// `TenantConn` binds the current tenant for RLS; this helper intentionally
/// does not duplicate that tenant predicate in SQL.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query or row decoding fails.
pub async fn roles_by_name(
    conn: &mut TenantConn<'_>,
    names: &[&str],
) -> Result<Vec<RoleRow>, sqlx::Error> {
    if names.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(ROLES_BY_NAME_SQL)
        .bind(names)
        .fetch_all(&mut **conn.transaction())
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok(RoleRow {
                name: row.try_get("name")?,
                permissions: row.try_get("permissions")?,
            })
        })
        .collect()
}

const INSERT_ROLE_SQL: &str = r#"
INSERT INTO wyrd.auth_roles (name, permissions)
VALUES ($1, $2)
ON CONFLICT (name) DO UPDATE
  SET permissions = EXCLUDED.permissions
"#;

const DELETE_ROLE_SQL: &str = r#"
DELETE FROM wyrd.auth_roles
 WHERE name = $1
"#;

/// Insert or update a role's permission payload.
///
/// Uses an upsert so callers can call this idempotently during seeding or
/// operator-driven role management. The builtin-immutability trigger fires
/// for builtin role names and returns a database error.
///
/// # Errors
/// Returns a SQLx error when the constraint trigger fires (builtin role name)
/// or when Postgres rejects the query.
pub async fn insert_role(
    conn: &mut TenantConn<'_>,
    name: &str,
    permissions: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(INSERT_ROLE_SQL)
        .bind(name)
        .bind(permissions)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}

/// Delete a role by name.
///
/// Returns `Ok(true)` when a row was deleted, `Ok(false)` when no row matched.
/// The builtin-immutability trigger fires for builtin role names and returns
/// a database error.
///
/// # Errors
/// Returns a SQLx error when the constraint trigger fires (builtin role name)
/// or when Postgres rejects the query.
pub async fn delete_role(conn: &mut TenantConn<'_>, name: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(DELETE_ROLE_SQL)
        .bind(name)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::{DELETE_ROLE_SQL, INSERT_ROLE_SQL, ROLES_BY_NAME_SQL};

    #[test]
    fn roles_by_name_query_uses_single_any_lookup() {
        assert!(ROLES_BY_NAME_SQL.contains("FROM wyrd.auth_roles"));
        assert!(ROLES_BY_NAME_SQL.contains("name = ANY($1)"));
        assert!(!ROLES_BY_NAME_SQL.contains("data_tenant_id"));
    }

    #[test]
    fn insert_role_query_targets_auth_roles_with_upsert() {
        assert!(INSERT_ROLE_SQL.contains("INTO wyrd.auth_roles"));
        assert!(INSERT_ROLE_SQL.contains("ON CONFLICT (name) DO UPDATE"));
    }

    #[test]
    fn delete_role_query_targets_auth_roles_by_name() {
        assert!(DELETE_ROLE_SQL.contains("FROM wyrd.auth_roles"));
        assert!(DELETE_ROLE_SQL.contains("WHERE name = $1"));
    }
}
