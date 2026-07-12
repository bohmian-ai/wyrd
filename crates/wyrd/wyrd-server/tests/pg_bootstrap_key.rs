mod pg_tests {
    //! Integration coverage for the `bootstrap-key` issuance chain.

    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_server::boot::bootstrap::{BootstrapError, bootstrap_admin_key};
    use wyrd_spec::TenantSlug;

    #[tokio::test]
    async fn bootstrap_mints_runtime_admin_key_and_is_rerunnable() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let slug = TenantSlug::new(fixture.tenant_slug()).expect("seeded slug is valid");

        let first = bootstrap_admin_key(fixture.app_pool(), &slug)
            .await
            .expect("first bootstrap mints a key");
        let second = bootstrap_admin_key(fixture.app_pool(), &slug)
            .await
            .expect("re-run is idempotent-safe");

        assert!(first.expose().starts_with("wyrd_sk_"));
        assert_ne!(
            first.expose(),
            second.expose(),
            "each run mints a fresh key"
        );

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let roles = sqlx::query_scalar::<_, String>(
            r#"
        SELECT r.name
          FROM wyrd.auth_service_account_roles sar
          JOIN wyrd.auth_roles r
            ON r.data_tenant_id = sar.data_tenant_id
           AND r.id = sar.role_id
         WHERE sar.data_tenant_id = $1
        "#,
        )
        .bind(fixture.data_tenant_id().as_uuid())
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("service-account role query succeeds");

        assert!(
            roles.contains(&"runtime_admin".to_owned()),
            "bootstrap account carries runtime_admin, got {roles:?}"
        );

        let service_accounts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM wyrd.auth_service_accounts WHERE data_tenant_id = $1",
        )
        .bind(fixture.data_tenant_id().as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("service-account count query succeeds");
        assert_eq!(
            service_accounts, 1,
            "re-run reuses the single bootstrap service-account"
        );
    }

    #[tokio::test]
    async fn bootstrap_rejects_unknown_tenant() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let slug = TenantSlug::new("no-such-tenant").expect("slug is valid");

        let error = bootstrap_admin_key(fixture.app_pool(), &slug)
            .await
            .expect_err("unknown tenant is rejected");

        assert!(matches!(error, BootstrapError::UnknownTenant(_)));
    }
}
