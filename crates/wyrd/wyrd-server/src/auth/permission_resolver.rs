//! SQL-backed permission resolution for verified JWT roles.

use std::sync::Arc;

use sqlx::PgPool;
use wyrd_auth_verify::{PermissionResolver, ResolveError};
use wyrd_runtime::{Permission, PermissionSet, RoleRef};
use wyrd_spec::DataTenantId;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{RoleRow, roles_by_name};

/// Server-tier resolver that maps role names to effective permissions.
#[derive(Debug, Clone)]
pub struct SqlPermissionResolver {
    pool: Arc<PgPool>,
}

impl SqlPermissionResolver {
    /// Construct a resolver backed by the Wyrd app Postgres pool.
    #[must_use]
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

impl PermissionResolver for SqlPermissionResolver {
    #[tracing::instrument(skip(self), fields(tenant_id = %tenant_id, role_count = roles.len()))]
    async fn resolve(
        &self,
        tenant_id: &DataTenantId,
        roles: &[RoleRef],
    ) -> Result<PermissionSet, ResolveError> {
        if roles.is_empty() {
            return Ok(PermissionSet::default());
        }

        let mut conn = TenantConn::acquire(&self.pool, *tenant_id)
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, "permission resolver failed to acquire tenant connection");
                ResolveError::Unavailable(error.to_string())
            })?;
        let names = roles.iter().map(RoleRef::as_str).collect::<Vec<_>>();
        let rows = roles_by_name(&mut conn, &names).await.map_err(|error| {
            tracing::warn!(error = %error, "permission resolver query failed");
            ResolveError::Unavailable(error.to_string())
        })?;

        permission_set_from_rows(rows)
    }
}

fn permission_set_from_rows(rows: Vec<RoleRow>) -> Result<PermissionSet, ResolveError> {
    let mut set = PermissionSet::default();
    for row in rows {
        let permissions = permissions_from_row(&row)?;
        for permission in permissions {
            set.insert(permission);
        }
    }
    Ok(set)
}

fn permissions_from_row(row: &RoleRow) -> Result<Vec<Permission>, ResolveError> {
    serde_json::from_value(row.permissions.clone()).map_err(|source| {
        ResolveError::BadPermissionsJson {
            role: row.name.clone(),
            source,
        }
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wyrd_runtime::Permission;
    use wyrd_sql::queries::auth::RoleRow;

    use super::permission_set_from_rows;

    #[test]
    fn decodes_and_merges_permissions_from_existing_role_rows() {
        let rows = vec![
            RoleRow {
                name: "reader".to_owned(),
                permissions: json!([
                    { "resource": "cards", "action": "read" },
                    { "resource": "artifacts", "action": "read" }
                ]),
            },
            RoleRow {
                name: "admin".to_owned(),
                permissions: json!([{ "resource": "wildcard", "action": "wildcard" }]),
            },
        ];

        let set = permission_set_from_rows(rows).expect("role permissions decode");

        assert_eq!(set.len(), 1);
        assert!(set.contains(&Permission::wildcard()));
        assert!(set.contains(&Permission::card_read()));
    }

    #[test]
    fn reports_role_name_for_malformed_permissions() {
        let rows = vec![RoleRow {
            name: "bad_role".to_owned(),
            permissions: json!([{ "resource": "cards", "action": "unknown" }]),
        }];

        let error = permission_set_from_rows(rows).expect_err("permissions should fail");

        assert!(
            error.to_string().contains("bad_role"),
            "error should identify the corrupt role: {error}"
        );
    }
}
