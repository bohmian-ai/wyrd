//! Shared auth repository helpers.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use wyrd_runtime::PrincipalId;
use wyrd_sql::{SqlError, TenantConn};

/// Look up a User principal's email in the caller's tenant.
///
/// Returns `Ok(None)` for unknown principals and for Service / Agent
/// principals, which have no `auth_users` row by design.
pub async fn lookup_user_email(
    conn: &mut TenantConn<'_>,
    principal_id: &PrincipalId,
) -> Result<Option<String>, SqlError> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT email FROM wyrd.auth_users WHERE id = $1 LIMIT 1")
            .bind(principal_id.as_uuid())
            .fetch_optional(&mut **conn.transaction())
            .await
            .map_err(SqlError::from)?;
    Ok(row.and_then(|(email,)| email))
}

#[cfg(test)]
mod pg_tests {
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::PrincipalId;
    use wyrd_spec::DataTenantId;

    use super::lookup_user_email;

    #[tokio::test]
    async fn lookup_user_email_returns_email_for_known_user() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let principal_id = PrincipalId::new(uuid::Uuid::new_v4());
        insert_user(&fixture, tenant, principal_id, Some("known@example.com")).await;

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let email = lookup_user_email(&mut conn, &principal_id)
            .await
            .expect("lookup succeeds");

        assert_eq!(email.as_deref(), Some("known@example.com"));
    }

    #[tokio::test]
    async fn lookup_user_email_returns_none_for_unknown_uuid() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let principal_id = PrincipalId::new(uuid::Uuid::new_v4());

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let email = lookup_user_email(&mut conn, &principal_id)
            .await
            .expect("lookup succeeds");

        assert!(email.is_none());
    }

    #[tokio::test]
    async fn lookup_user_email_returns_none_for_null_email() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let principal_id = PrincipalId::new(uuid::Uuid::new_v4());
        insert_user(&fixture, tenant, principal_id, None).await;

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let email = lookup_user_email(&mut conn, &principal_id)
            .await
            .expect("lookup succeeds");

        assert!(email.is_none());
    }

    #[tokio::test]
    async fn lookup_user_email_is_tenant_scoped() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(tenant_b, "test-tenant-2")
            .await
            .expect("tenant B inserts");
        let principal_id = PrincipalId::new(uuid::Uuid::new_v4());
        insert_user(
            &fixture,
            tenant_a,
            principal_id,
            Some("tenant-a@example.com"),
        )
        .await;

        let mut conn = fixture
            .tenant_conn_for(tenant_b)
            .await
            .expect("tenant B conn opens");
        let email = lookup_user_email(&mut conn, &principal_id)
            .await
            .expect("lookup succeeds");

        assert!(email.is_none());
    }

    async fn insert_user(
        fixture: &PgFixture,
        tenant: DataTenantId,
        principal_id: PrincipalId,
        email: Option<&str>,
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
