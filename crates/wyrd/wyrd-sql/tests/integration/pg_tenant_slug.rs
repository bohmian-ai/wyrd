mod pg_tests {
    //! Durable proof of the one tenant slug resolver.
    //!
    //! Tenant login, workload exchange, and boot all resolve a route slug
    //! through `WyrdPostgres::resolve_tenant_slug`. It runs on the operator
    //! pool and never falls back to the RLS app pool; the fail-closed case
    //! without an operator pool is a unit test beside the owner.
    //!
    //! Skipped automatically when the database environment is unset so the
    //! default suite stays credential-free.

    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::{DataTenantId, TenantSlug};

    /// Skip when no database is configured, matching the sibling suites.
    fn database_url() -> Option<String> {
        std::env::var("WYRD_DATABASE_URL").ok()
    }

    /// Parse a test slug.
    ///
    /// # Panics
    /// Panics when `slug` is not a valid tenant slug.
    fn slug(slug: &str) -> TenantSlug {
        TenantSlug::new(slug).expect("test slug is valid")
    }

    /// An active slug resolves to its tenant; a suspended or missing slug
    /// resolves to nothing, so every caller renders its generic refusal.
    #[tokio::test]
    async fn resolver_answers_only_active_slugs() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let postgres = fixture.wyrd_postgres();
        let suspended_slug = format!("slug-sus-{}", DataTenantId::new_v7().as_uuid().simple());
        let suspended = fixture
            .seed_additional_tenant(&suspended_slug)
            .await
            .expect("tenant seeds");
        sqlx::query("UPDATE platform.tenants SET status = 'suspended' WHERE data_tenant_id = $1")
            .bind(suspended.as_uuid())
            .execute(fixture.operator_pool().pool())
            .await
            .expect("tenant suspends");

        assert_eq!(
            postgres
                .resolve_tenant_slug(&slug(fixture.tenant_slug()))
                .await
                .expect("active slug resolves"),
            Some(fixture.data_tenant_id()),
        );
        assert_eq!(
            postgres
                .resolve_tenant_slug(&slug(&suspended_slug))
                .await
                .expect("suspended slug resolves"),
            None,
            "a suspended tenant is not resolvable"
        );
        assert_eq!(
            postgres
                .resolve_tenant_slug(&slug("no-such-tenant-slug"))
                .await
                .expect("missing slug resolves"),
            None,
            "a missing tenant is not resolvable"
        );
    }
}
