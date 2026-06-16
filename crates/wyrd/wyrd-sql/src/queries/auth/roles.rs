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

#[cfg(test)]
mod tests {
    use super::ROLES_BY_NAME_SQL;

    #[test]
    fn roles_by_name_query_uses_single_any_lookup() {
        assert!(ROLES_BY_NAME_SQL.contains("FROM wyrd.auth_roles"));
        assert!(ROLES_BY_NAME_SQL.contains("name = ANY($1)"));
        assert!(!ROLES_BY_NAME_SQL.contains("data_tenant_id"));
    }
}
