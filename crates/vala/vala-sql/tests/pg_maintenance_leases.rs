mod pg_tests {
    //! SQL integration tests for the maintenance-worker lease table (slice 01).
    //!
    //! Exercises the real fencing contract against a live Postgres:
    //! acquire → renew-with-correct-token → renew-with-wrong-token/owner →
    //! contention on a live lease. Run via `mise run test:sql`.

    use std::sync::Arc;

    use sqlx::types::Uuid;
    use tokio::sync::Barrier;
    use vala_sql::OperatorPool;
    use vala_sql::queries::maintenance_leases::{
        release_lease_fenced, renew_lease_fenced, try_acquire_lease,
    };
    use wyrd_dev_fixtures::pg::PgFixture;

    async fn operator(fixture: &PgFixture) -> OperatorPool {
        fixture.operator_pool().clone()
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

        assert!(!token.takeover);
        let renewed = renew_lease_fenced(
            &op,
            "compaction:table:alpha",
            owner,
            token.fencing_token,
            30,
        )
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
        let stale = renew_lease_fenced(
            &op,
            "snapshot_expiry:global",
            owner,
            token.fencing_token + 1,
            30,
        )
        .await
        .expect("renew");
        assert!(
            !stale,
            "renew with a mismatched fencing token must return false"
        );

        // A different owner with the right token value is also fenced out.
        let other_owner = renew_lease_fenced(
            &op,
            "snapshot_expiry:global",
            Uuid::now_v7(),
            token.fencing_token,
            30,
        )
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
        assert!(
            first.fencing_token > 0,
            "fencing token is strictly positive"
        );
        assert!(!first.takeover);

        // A second caller cannot take a still-live lease.
        let second = try_acquire_lease(&op, "orphan_files:global", Uuid::now_v7(), 60)
            .await
            .expect("acquire");
        assert!(
            second.is_none(),
            "a live lease held by another owner must not be re-acquired"
        );
    }

    #[tokio::test]
    async fn release_requires_the_current_fence() {
        let fixture = PgFixture::start().await.expect("fixture");
        let op = operator(&fixture).await;
        let owner = Uuid::now_v7();
        let key = "forge:table:release";

        let token = try_acquire_lease(&op, key, owner, 30)
            .await
            .expect("acquire")
            .expect("acquired");
        assert!(
            !release_lease_fenced(&op, key, owner, token.fencing_token + 1)
                .await
                .expect("stale release")
        );
        assert!(
            release_lease_fenced(&op, key, owner, token.fencing_token)
                .await
                .expect("release")
        );
        assert!(
            !release_lease_fenced(&op, key, owner, token.fencing_token)
                .await
                .expect("repeat release")
        );
    }

    /// Same-owner reacquire and release/reinsert are never classified as takeover.
    #[tokio::test]
    async fn same_owner_and_release_reinsert_are_not_takeovers() {
        let fixture = PgFixture::start().await.expect("fixture");
        let op = operator(&fixture).await;
        let key = format!("forge:reinsert:{}", Uuid::now_v7());
        let owner = Uuid::now_v7();
        let first = try_acquire_lease(&op, &key, owner, 30)
            .await
            .expect("first acquisition")
            .expect("first owner wins");
        let reacquired = try_acquire_lease(&op, &key, owner, 30)
            .await
            .expect("same-owner reacquire")
            .expect("same owner reacquires");
        assert!(!reacquired.takeover);
        assert!(reacquired.fencing_token > first.fencing_token);
        assert!(
            release_lease_fenced(&op, &key, owner, reacquired.fencing_token)
                .await
                .expect("release")
        );
        let inserted = try_acquire_lease(&op, &key, Uuid::now_v7(), 30)
            .await
            .expect("post-release insert")
            .expect("new owner inserts");
        assert!(!inserted.takeover);
    }

    #[tokio::test]
    async fn expired_lease_can_be_reacquired_with_a_new_token() {
        let fixture = PgFixture::start().await.expect("fixture");
        let op = operator(&fixture).await;
        let key = "forge:table:expiry";
        let first_owner = Uuid::now_v7();
        let second_owner = Uuid::now_v7();

        let first = try_acquire_lease(&op, key, first_owner, 30)
            .await
            .expect("acquire")
            .expect("acquired");
        sqlx::query(
            "UPDATE vala.maintenance_leases SET expires_at = now() - interval '1 second' WHERE lease_key = $1",
        )
        .bind(key)
        .execute(op.pool())
        .await
        .expect("expire lease");

        let second = try_acquire_lease(&op, key, second_owner, 30)
            .await
            .expect("reacquire")
            .expect("expired lease is reclaimable");
        assert!(second.fencing_token > first.fencing_token);
        assert!(second.takeover);
        assert!(
            !renew_lease_fenced(&op, key, first_owner, first.fencing_token, 30)
                .await
                .expect("stale renew")
        );
        assert!(
            release_lease_fenced(&op, key, second_owner, second.fencing_token)
                .await
                .expect("successor release")
        );
    }

    /// Proves a different owner replacing an expired row reports takeover.
    #[tokio::test]
    async fn expired_different_owner_reports_takeover() {
        let fixture = PgFixture::start().await.expect("fixture");
        let op = operator(&fixture).await;
        let key = format!("forge:takeover:{}", Uuid::now_v7());
        let first = try_acquire_lease(&op, &key, Uuid::now_v7(), 30)
            .await
            .expect("seed acquisition")
            .expect("seed owner wins");
        sqlx::query(
            "UPDATE vala.maintenance_leases SET expires_at = now() - interval '1 second' WHERE lease_key = $1",
        )
        .bind(&key)
        .execute(op.pool())
        .await
        .expect("expire seed");
        let second = try_acquire_lease(&op, &key, Uuid::now_v7(), 30)
            .await
            .expect("replacement acquisition")
            .expect("replacement owner wins");
        assert!(second.takeover);
        assert!(second.fencing_token > first.fencing_token);
    }

    /// Proves simultaneous acquisition of an absent row has one non-takeover winner.
    #[tokio::test]
    async fn concurrent_absent_row_acquisition_has_one_winner() {
        let fixture = PgFixture::start().await.expect("fixture");
        let key = format!("forge:absent-race:{}", Uuid::now_v7());
        let barrier = Arc::new(Barrier::new(3));
        let mut tasks = Vec::new();
        for _ in 0..2 {
            let op = operator(&fixture).await;
            let key = key.clone();
            let barrier = Arc::clone(&barrier);
            tasks.push(tokio::spawn(async move {
                let owner = Uuid::now_v7();
                barrier.wait().await;
                let result = try_acquire_lease(&op, &key, owner, 30)
                    .await
                    .expect("race acquisition");
                (owner, result)
            }));
        }
        barrier.wait().await;
        let left = tasks.remove(0).await.expect("left task");
        let right = tasks.remove(0).await.expect("right task");
        let winners = [left, right]
            .into_iter()
            .filter_map(|(owner, result)| result.map(|result| (owner, result)))
            .collect::<Vec<_>>();
        assert_eq!(winners.len(), 1);
        assert!(!winners[0].1.takeover);
        assert!(winners[0].1.fencing_token > 0);
        let stored: (Uuid, i64) = sqlx::query_as(
            "SELECT owner, fencing_token FROM vala.maintenance_leases WHERE lease_key = $1",
        )
        .bind(&key)
        .fetch_one(operator(&fixture).await.pool())
        .await
        .expect("stored winner");
        assert_eq!(stored, (winners[0].0, winners[0].1.fencing_token));
    }

    /// Proves concurrent challengers serialize to one authoritative takeover.
    #[tokio::test]
    async fn concurrent_expired_row_acquisition_has_one_takeover() {
        let fixture = PgFixture::start().await.expect("fixture");
        for iteration in 0..32 {
            let key = format!("forge:expired-race:{iteration}:{}", Uuid::now_v7());
            let seed = try_acquire_lease(&operator(&fixture).await, &key, Uuid::now_v7(), 30)
                .await
                .expect("seed acquisition")
                .expect("seed owner wins");
            sqlx::query(
                "UPDATE vala.maintenance_leases SET expires_at = now() - interval '1 second' WHERE lease_key = $1",
            )
            .bind(&key)
            .execute(operator(&fixture).await.pool())
            .await
            .expect("expire seed");
            let barrier = Arc::new(Barrier::new(3));
            let mut tasks = Vec::new();
            for _ in 0..2 {
                let op = operator(&fixture).await;
                let key = key.clone();
                let barrier = Arc::clone(&barrier);
                tasks.push(tokio::spawn(async move {
                    let owner = Uuid::now_v7();
                    barrier.wait().await;
                    let result = try_acquire_lease(&op, &key, owner, 30)
                        .await
                        .expect("race acquisition");
                    (owner, result)
                }));
            }
            barrier.wait().await;
            let left = tasks.remove(0).await.expect("left task");
            let right = tasks.remove(0).await.expect("right task");
            let winners = [left, right]
                .into_iter()
                .filter_map(|(owner, result)| result.map(|result| (owner, result)))
                .collect::<Vec<_>>();
            assert_eq!(winners.len(), 1);
            assert!(winners[0].1.takeover);
            assert!(winners[0].1.fencing_token > seed.fencing_token);
            let stored: (Uuid, i64) = sqlx::query_as(
                "SELECT owner, fencing_token FROM vala.maintenance_leases WHERE lease_key = $1",
            )
            .bind(&key)
            .fetch_one(operator(&fixture).await.pool())
            .await
            .expect("stored takeover winner");
            assert_eq!(stored, (winners[0].0, winners[0].1.fencing_token));
        }
    }
}
