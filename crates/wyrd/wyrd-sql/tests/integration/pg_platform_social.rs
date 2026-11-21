mod pg_tests {
    //! Durable proof for shared social provider storage: a replaced client
    //! starts a new revision that an earlier test can never certify, and the
    //! records admit only operator sessions.

    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_sql::queries::platform::social::{
        put_social_provider, social_provider, stamp_social_provider_tested,
    };

    /// Skip when no database is configured, matching the sibling suites.
    fn database_url() -> Option<String> {
        std::env::var("WYRD_DATABASE_URL").ok()
    }

    /// Store `client_id` as Google's client and return the new revision.
    async fn configure(fixture: &PgFixture, client_id: &str) -> i64 {
        let mut conn = fixture
            .operator_pool()
            .begin_platform_audited()
            .await
            .expect("transaction opens");
        let row = put_social_provider(&mut conn, "google", client_id, b"sealed")
            .await
            .expect("provider stores");
        conn.commit().await.expect("provider commits");
        row.revision
    }

    /// Replacing a client advances the revision, clears its test, and a test
    /// of the replaced revision certifies nothing.
    #[tokio::test]
    async fn a_test_of_a_replaced_revision_certifies_nothing() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        let first = configure(&fixture, "client-1").await;
        assert!(
            stamp_social_provider_tested(pool, "google", first)
                .await
                .expect("stamp runs"),
            "the current revision is tested"
        );
        let second = configure(&fixture, "client-2").await;
        assert_eq!(second, first + 1, "a replaced client is a new revision");
        let current = social_provider(pool, "google")
            .await
            .expect("provider reads")
            .expect("provider exists");
        assert!(
            !current.tested && !current.enabled,
            "a new revision starts untested"
        );
        assert!(
            !stamp_social_provider_tested(pool, "google", first)
                .await
                .expect("stamp runs"),
            "a test of the replaced revision is refused"
        );
    }

    /// The tenant runtime role reads no shared provider and writes none.
    #[tokio::test]
    async fn the_tenant_role_cannot_reach_shared_providers() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        configure(&fixture, "client-1").await;
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM platform.social_providers")
            .fetch_one(&mut **conn.transaction())
            .await
            .unwrap_or(0);
        assert_eq!(
            visible, 0,
            "no shared provider is visible to a tenant session"
        );
        drop(conn);
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let enabled = sqlx::query("UPDATE platform.login_settings SET signup_enabled = TRUE")
            .execute(&mut **conn.transaction())
            .await
            .map_or(0, |done| done.rows_affected());
        assert_eq!(enabled, 0, "a tenant session cannot change login settings");
    }
}
