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
mod pg_tests {
    use std::sync::Arc;

    use serde_json::json;
    use wyrd_auth_verify::PermissionResolver;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{Permission, RoleRef};
    use wyrd_sql::queries::auth::RoleRow;

    use crate::seed::seed_builtin_roles_for_tenant;

    use super::SqlPermissionResolver;
    use super::permission_set_from_rows;

    #[test]
    fn decodes_and_merges_permissions_from_existing_role_rows() {
        let rows = vec![
            RoleRow {
                id: uuid::Uuid::nil(),
                name: "reader".to_owned(),
                permissions: json!([
                    { "resource": "cards", "action": "read" },
                    { "resource": "artifacts", "action": "read" }
                ]),
            },
            RoleRow {
                id: uuid::Uuid::nil(),
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
            id: uuid::Uuid::nil(),
            name: "bad_role".to_owned(),
            permissions: json!([{ "resource": "cards", "action": "unknown" }]),
        }];

        let error = permission_set_from_rows(rows).expect_err("permissions should fail");

        assert!(
            error.to_string().contains("bad_role"),
            "error should identify the corrupt role: {error}"
        );
    }

    #[tokio::test]
    async fn resolved_set_matches_constant() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("builtin roles seed");
        conn.commit().await.expect("seed transaction commits");
        let resolver = SqlPermissionResolver::new(Arc::new(fixture.app_pool().clone()));

        let set = resolver
            .resolve(
                &tenant,
                &[RoleRef::new("agent").expect("role name is valid")],
            )
            .await
            .expect("permissions resolve");

        assert!(set.contains(&Permission::card_write()));
        assert!(!set.contains(&Permission::trigger_write()));
    }

    #[tokio::test]
    async fn runtime_admin_can_delegate() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("builtin roles seed");
        conn.commit().await.expect("seed transaction commits");
        let resolver = SqlPermissionResolver::new(Arc::new(fixture.app_pool().clone()));

        let set = resolver
            .resolve(
                &tenant,
                &[RoleRef::new("runtime_admin").expect("role name is valid")],
            )
            .await
            .expect("permissions resolve");

        assert!(set.contains(&Permission::delegation_issue()));
        assert!(set.contains(&Permission::service_accounts_write()));
    }
}
