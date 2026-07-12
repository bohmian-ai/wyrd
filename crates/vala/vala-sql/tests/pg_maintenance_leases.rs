mod pg_tests {
    //! SQL integration tests for the maintenance-worker lease table (slice 01).
    //!
    //! Exercises the real fencing contract against a live Postgres:
    //! acquire → renew-with-correct-token → renew-with-wrong-token/owner →
    //! contention on a live lease. Run via `mise run test:sql`.

    use sqlx::types::Uuid;
    use vala_sql::OperatorPool;
    use vala_sql::queries::maintenance_leases::{renew_lease_fenced, try_acquire_lease};
    use wyrd_dev_fixtures::pg::PgFixture;

    async fn operator(fixture: &PgFixture) -> OperatorPool {
        OperatorPool::from(fixture.platform_admin_pool().clone())
    }

    #[tokio::test]
    async fn acquire_then_renew_with_correct_token_succeeds() {
        let fixture = PgFixture::start().await.expect("fixture");
        let op = operator(&fixture).await;
        let owner = Uuid::now_v7();

        let token = try_acquire_lease(&op, "compaction:table:alpha", owner, 30)
            .await
            .expect("acquire")
            .expect("brand-new lease is acquired");

        let renewed = renew_lease_fenced(&op, "compaction:table:alpha", owner, token, 30)
            .await
            .expect("renew");
        assert!(renewed, "renew with the owning (owner, token) must succeed");
    }

    #[tokio::test]
    async fn renew_with_wrong_token_reports_ownership_loss() {
        let fixture = PgFixture::start().await.expect("fixture");
        let op = operator(&fixture).await;
        let owner = Uuid::now_v7();

        let token = try_acquire_lease(&op, "snapshot_expiry:global", owner, 30)
            .await
            .expect("acquire")
            .expect("acquired");

        // A stale worker holding an older token must be fenced out.
        let stale = renew_lease_fenced(&op, "snapshot_expiry:global", owner, token + 1, 30)
            .await
            .expect("renew");
        assert!(
            !stale,
            "renew with a mismatched fencing token must return false"
        );

        // A different owner with the right token value is also fenced out.
        let other_owner =
            renew_lease_fenced(&op, "snapshot_expiry:global", Uuid::now_v7(), token, 30)
                .await
                .expect("renew");
        assert!(
            !other_owner,
            "renew with a different owner must return false"
        );
    }

    #[tokio::test]
    async fn acquire_contends_on_live_lease() {
        let fixture = PgFixture::start().await.expect("fixture");
        let op = operator(&fixture).await;

        let first = try_acquire_lease(&op, "orphan_files:global", Uuid::now_v7(), 60)
            .await
            .expect("acquire")
            .expect("first caller wins");
        assert!(first > 0, "fencing token is strictly positive");

        // A second caller cannot take a still-live lease.
        let second = try_acquire_lease(&op, "orphan_files:global", Uuid::now_v7(), 60)
            .await
            .expect("acquire");
        assert!(
            second.is_none(),
            "a live lease held by another owner must not be re-acquired"
        );
    }
}
