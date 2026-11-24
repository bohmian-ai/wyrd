//! Queries for `wyrd.auth_roles`.
//!
//! Tenant-scoped functions here take `&mut TenantConn<'_>`.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use serde_json::Value;
use sqlx::Row;
use sqlx::types::Uuid;

use crate::TenantConn;

const ROLES_BY_NAME_SQL: &str = r"
SELECT id, name, permissions, description
  FROM wyrd.auth_roles
 WHERE name = ANY($1)
";

const ROLE_BY_ID_SQL: &str = r"
SELECT id, name, permissions, description
  FROM wyrd.auth_roles
 WHERE data_tenant_id = wyrd.current_tenant()
   AND id = $1
";

const ROLE_BY_NAME_SQL: &str = r"
SELECT id, name, permissions, description
  FROM wyrd.auth_roles
 WHERE data_tenant_id = wyrd.current_tenant()
   AND name = $1
";

/// Every Role with its description and member count: the distinct
/// assignable principals holding it at any scope, directly or through a
/// Group, each counted once.
const LIST_ROLES_SQL: &str = concat!(
    r"
SELECT r0.id, r0.name, r0.permissions, r0.description,
       (SELECT count(*)
          FROM (",
    assignable_principals_union!(),
    r") principals
         WHERE principals.id IN (",
    role_holders!("r.id = r0.id"),
    r")) AS member_count
  FROM wyrd.auth_roles r0
 WHERE r0.data_tenant_id = wyrd.current_tenant()
 ORDER BY r0.name
"
);

const SET_ROLE_DESCRIPTION_SQL: &str = r"
UPDATE wyrd.auth_roles
   SET description = $2
 WHERE data_tenant_id = wyrd.current_tenant()
   AND id = $1
";

/// Raw role row used by server-tier permission resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleRow {
    /// Role id.
    pub id: Uuid,
    /// Role name.
    pub name: String,
    /// Raw JSONB permissions payload.
    pub permissions: Value,
    /// One-line summary, when the Role has one.
    pub description: Option<String>,
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
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                permissions: row.try_get("permissions")?,
                description: row.try_get("description")?,
            })
        })
        .collect()
}

const INSERT_ROLE_SQL: &str = r"
INSERT INTO wyrd.auth_roles (id, data_tenant_id, name, permissions, builtin)
VALUES ($1, $2, $3, $4, $5)
ON CONFLICT (data_tenant_id, name) DO UPDATE
  SET permissions = EXCLUDED.permissions
";

const DELETE_ROLE_SQL: &str = r"
DELETE FROM wyrd.auth_roles
 WHERE data_tenant_id = wyrd.current_tenant()
   AND name = $1
";

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
    id: Uuid,
    name: &str,
    permissions: &serde_json::Value,
    builtin: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(INSERT_ROLE_SQL)
        .bind(id)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(name)
        .bind(permissions)
        .bind(builtin)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}

/// Look up a role by id.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query or row decoding fails.
pub async fn role_by_id(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<Option<RoleRow>, sqlx::Error> {
    sqlx::query(ROLE_BY_ID_SQL)
        .bind(id)
        .fetch_optional(&mut **conn.transaction())
        .await?
        .map(role_row_from_sqlx_row)
        .transpose()
}

/// Look up a role by name.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query or row decoding fails.
pub async fn role_by_name(
    conn: &mut TenantConn<'_>,
    name: &str,
) -> Result<Option<RoleRow>, sqlx::Error> {
    sqlx::query(ROLE_BY_NAME_SQL)
        .bind(name)
        .fetch_optional(&mut **conn.transaction())
        .await?
        .map(role_row_from_sqlx_row)
        .transpose()
}

/// One Role as the Role list shows it.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct RoleListRow {
    /// Role id.
    pub id: Uuid,
    /// Role name.
    pub name: String,
    /// Raw JSONB permissions payload.
    pub permissions: Value,
    /// One-line summary, when the Role has one.
    pub description: Option<String>,
    /// Distinct assignable Users and Service accounts holding the Role at
    /// any scope, directly or through a Group.
    pub member_count: i64,
}

/// List every role in the current tenant with its description and member
/// count, ordered by name.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query or row decoding fails.
pub async fn list_roles(conn: &mut TenantConn<'_>) -> Result<Vec<RoleListRow>, sqlx::Error> {
    sqlx::query_as::<_, RoleListRow>(LIST_ROLES_SQL)
        .fetch_all(&mut **conn.transaction())
        .await
}

/// Replace Role `id`'s description; `None` clears it.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn set_role_description(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    description: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(SET_ROLE_DESCRIPTION_SQL)
        .bind(id)
        .bind(description)
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

fn role_row_from_sqlx_row(row: sqlx::postgres::PgRow) -> Result<RoleRow, sqlx::Error> {
    Ok(RoleRow {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        permissions: row.try_get("permissions")?,
        description: row.try_get("description")?,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        DELETE_ROLE_SQL, INSERT_ROLE_SQL, LIST_ROLES_SQL, ROLE_BY_ID_SQL, ROLE_BY_NAME_SQL,
        ROLES_BY_NAME_SQL,
    };

    #[test]
    fn roles_by_name_query_uses_single_any_lookup() {
        assert!(ROLES_BY_NAME_SQL.contains("FROM wyrd.auth_roles"));
        assert!(ROLES_BY_NAME_SQL.contains("name = ANY($1)"));
        assert!(!ROLES_BY_NAME_SQL.contains("data_tenant_id"));
    }

    #[test]
    fn insert_role_query_targets_auth_roles_with_upsert() {
        assert!(INSERT_ROLE_SQL.contains("INTO wyrd.auth_roles"));
        assert!(INSERT_ROLE_SQL.contains("id, data_tenant_id, name, permissions, builtin"));
        assert!(INSERT_ROLE_SQL.contains("ON CONFLICT (data_tenant_id, name) DO UPDATE"));
        let old_conflict_target = ["ON CONFLICT ", "(name)"].concat();
        assert!(!INSERT_ROLE_SQL.contains(&old_conflict_target));
    }

    #[test]
    fn delete_role_query_targets_auth_roles_by_name() {
        assert!(DELETE_ROLE_SQL.contains("FROM wyrd.auth_roles"));
        assert!(DELETE_ROLE_SQL.contains("AND name = $1"));
        assert!(DELETE_ROLE_SQL.contains("data_tenant_id = wyrd.current_tenant()"));
    }

    #[test]
    fn role_lookup_queries_include_id_and_permissions() {
        for sql in [ROLE_BY_ID_SQL, ROLE_BY_NAME_SQL] {
            assert!(sql.contains("SELECT id, name, permissions, description"));
            assert!(sql.contains("FROM wyrd.auth_roles"));
            assert!(sql.contains("data_tenant_id = wyrd.current_tenant()"));
        }
        assert!(ROLE_BY_ID_SQL.contains("AND id = $1"));
        assert!(ROLE_BY_NAME_SQL.contains("AND name = $1"));
        assert!(LIST_ROLES_SQL.contains("r0.data_tenant_id = wyrd.current_tenant()"));
        assert!(LIST_ROLES_SQL.contains("ORDER BY r0.name"));
    }
}
