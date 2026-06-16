//! Shared auth repository helpers.

use sqlx::PgPool;
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_sql::{SqlError, TenantConn};

/// Look up a User principal's email in the caller's tenant.
///
/// Returns `Ok(None)` for unknown principals and for Service / Agent
/// principals, which have no `auth_users` row by design.
pub async fn lookup_user_email(
    pool: &PgPool,
    tenant_id: &DataTenantId,
    principal_id: &PrincipalId,
) -> Result<Option<String>, SqlError> {
    let mut conn = TenantConn::acquire(pool, *tenant_id).await?;
    let row: Option<(String,)> =
        sqlx::query_as("SELECT email FROM wyrd.auth_users WHERE id = $1 LIMIT 1")
            .bind(principal_id.as_uuid())
            .fetch_optional(&mut **conn.transaction())
            .await
            .map_err(SqlError::from)?;
    Ok(row.map(|(email,)| email))
}

#[cfg(test)]
mod tests {
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::PrincipalId;
    use wyrd_spec::DataTenantId;

    use super::lookup_user_email;

    #[tokio::test]
    async fn lookup_user_email_returns_email_for_known_user() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let principal_id = PrincipalId::new(uuid::Uuid::new_v4());
        insert_user(&fixture, tenant, principal_id, "known@example.com").await;

        let email = lookup_user_email(fixture.app_pool(), &tenant, &principal_id)
            .await
            .expect("lookup succeeds");

        assert_eq!(email.as_deref(), Some("known@example.com"));
    }

    #[tokio::test]
    async fn lookup_user_email_returns_none_for_unknown_uuid() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let principal_id = PrincipalId::new(uuid::Uuid::new_v4());

        let email = lookup_user_email(fixture.app_pool(), &tenant, &principal_id)
            .await
            .expect("lookup succeeds");

        assert!(email.is_none());
    }

    #[tokio::test]
    async fn lookup_user_email_is_tenant_scoped() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = DataTenantId::new_v7();
        sqlx::query(
            "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status)
             VALUES ($1, 'test-tenant-2', 'Test Tenant 2', 'active')",
        )
        .bind(tenant_b.as_uuid())
        .execute(fixture.platform_admin_pool())
        .await
        .expect("tenant B inserts");
        let principal_id = PrincipalId::new(uuid::Uuid::new_v4());
        insert_user(&fixture, tenant_a, principal_id, "tenant-a@example.com").await;

        let email = lookup_user_email(fixture.app_pool(), &tenant_b, &principal_id)
            .await
            .expect("lookup succeeds");

        assert!(email.is_none());
    }

    async fn insert_user(
        fixture: &PgFixture,
        tenant: DataTenantId,
        principal_id: PrincipalId,
        email: &str,
    ) {
        let mut conn = fixture
            .tenant_conn_for(tenant)
            .await
            .expect("tenant conn opens");
        sqlx::query(
            "INSERT INTO wyrd.auth_users (id, data_tenant_id, email, auth_type, status)
             VALUES ($1, $2, $3, 'password', 'active')",
        )
        .bind(principal_id.as_uuid())
        .bind(tenant.as_uuid())
        .bind(email)
        .execute(&mut **conn.transaction())
        .await
        .expect("user inserts");
        conn.commit().await.expect("user insert commits");
    }
}
