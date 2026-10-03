mod pg_tests {
    //! Real-Postgres proof of the singleton Forge leader term.

    use std::time::Duration;

    use sqlx::types::Uuid;
    use vala_sql::queries::forge_leader::{ForgeLeaderElection, ForgeLeaderTerm};
    use wyrd_dev_fixtures::pg::PgFixture;

    /// One live term excludes a standby; resignation hands over at once with a larger token.
    ///
    /// The successor's term must publish its own peer URI, and an expired or
    /// resigned row must never be returned as a route.
    ///
    /// # Panics
    /// Panics when the fixture cannot start or any election transition is wrong.
    #[tokio::test]
    async fn leader_term_excludes_standby_and_resign_hands_over() {
        let fixture = PgFixture::start().await.expect("fixture");
        let election = ForgeLeaderElection::new(fixture.operator_pool().clone());
        let ttl = Duration::from_secs(30);
        let (leader, standby) = (Uuid::now_v7(), Uuid::now_v7());

        let first = election
            .acquire(leader, Some("https://leader:7443"), ttl)
            .await
            .expect("acquire")
            .expect("an empty row elects the first caller");
        assert_eq!(
            election
                .acquire(standby, Some("https://standby:7443"), ttl)
                .await
                .expect("standby"),
            None,
            "a live term excludes a standby"
        );
        assert!(election.renew(leader, first, ttl).await.expect("renew"));
        assert!(
            !election
                .renew(standby, first, ttl)
                .await
                .expect("foreign renew")
        );
        assert_eq!(
            election.current().await.expect("current"),
            Some(ForgeLeaderTerm {
                owner: leader,
                fencing_token: first,
                peer_uri: Some("https://leader:7443".to_owned()),
            })
        );

        election.resign(leader, first).await.expect("resign");
        assert_eq!(election.current().await.expect("after resign"), None);
        assert!(
            !election
                .renew(leader, first, ttl)
                .await
                .expect("renew after resign")
        );
        let second = election
            .acquire(standby, Some("https://standby:7443"), ttl)
            .await
            .expect("successor")
            .expect("a resigned row elects the standby at once");
        assert!(second > first, "every term mints a larger token");

        let admin = fixture.superuser_pool().await.expect("admin");
        sqlx::query("UPDATE vala.forge_scheduler_state SET expires_at=statement_timestamp()-interval '1 second'")
            .execute(&admin)
            .await
            .expect("expire term");
        assert_eq!(
            election.current().await.expect("expired"),
            None,
            "an expired term is never a route"
        );
        assert!(
            election
                .acquire(leader, None, ttl)
                .await
                .expect("takeover")
                .is_some_and(|token| token > second)
        );
    }
}
