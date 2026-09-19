mod pg_tests {
    //! Durable proof for the platform identity plane's storage semantics.
    //!
    //! Every property here is a property of the schema and its statements —
    //! single-use login state, one-time subject pinning, the uniqueness that
    //! stops two principals competing for one identity — rather than of Rust
    //! code that could be asserted in isolation. They are the layer that makes
    //! the federated login path safe, and none of them is observable from a
    //! provider-free HTTP journey.
    //!
    //! Skipped automatically when the database environment is unset so the
    //! default suite stays credential-free.

    use chrono::{Duration, Utc};
    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_sql::queries::platform::identity::{
        insert_platform_identity_tx, insert_platform_login_state, pin_platform_identity,
        platform_identity_by_subject, platform_oidc_connection, purge_expired_platform_login_state,
        take_platform_login_state, upsert_platform_oidc_connection,
    };
    use wyrd_sql::queries::platform::principals::{
        count_active_platform_principals, insert_platform_principal, list_platform_principals,
        platform_principal_by_id, set_platform_principal_status,
    };

    /// Skip when no database is configured, matching the sibling suites.
    fn database_url() -> Option<String> {
        std::env::var("WYRD_DATABASE_URL").ok()
    }

    /// The issuer every test in this module registers against.
    const ISSUER: &str = "https://idp.example.com/realms/platform";

    /// Register a human platform principal awaiting its first login.
    async fn register(fixture: &PgFixture, name: &str, claim: &str) -> Uuid {
        let pool = fixture.operator_pool();
        let id = Uuid::now_v7();
        let mut tx = pool.begin().await.expect("transaction opens");
        wyrd_sql::queries::platform::principals::insert_platform_principal_tx(
            &mut tx,
            id,
            PrincipalKindTag::User,
            name,
        )
        .await
        .expect("principal inserts");
        insert_platform_identity_tx(&mut tx, id, ISSUER, claim)
            .await
            .expect("identity inserts");
        tx.commit().await.expect("registration commits");
        id
    }

    /// A login state is consumable exactly once.
    ///
    /// This is what makes PKCE and the nonce load-bearing: if the row survived
    /// consumption, an intercepted authorization code could be replayed against
    /// the same state forever.
    #[tokio::test]
    async fn login_state_is_consumed_exactly_once() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();

        insert_platform_login_state(
            pool,
            "state-1",
            "verifier-1",
            "nonce-1",
            ISSUER,
            "https://wyrd.example/callback",
            Utc::now() + Duration::minutes(5),
        )
        .await
        .expect("state inserts");

        let first = take_platform_login_state(pool, "state-1")
            .await
            .expect("first take succeeds")
            .expect("state exists");
        assert_eq!(first.nonce, "nonce-1");
        assert_eq!(first.code_verifier, "verifier-1");

        assert!(
            take_platform_login_state(pool, "state-1")
                .await
                .expect("second take succeeds")
                .is_none(),
            "a replayed callback finds nothing"
        );
    }

    /// An expired login state is unusable, and says nothing different when it is.
    #[tokio::test]
    async fn an_expired_login_state_is_refused_like_an_unknown_one() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();

        insert_platform_login_state(
            pool,
            "state-expired",
            "verifier",
            "nonce",
            ISSUER,
            "https://wyrd.example/callback",
            Utc::now() - Duration::seconds(1),
        )
        .await
        .expect("state inserts");

        assert!(
            take_platform_login_state(pool, "state-expired")
                .await
                .expect("take succeeds")
                .is_none(),
            "an expired state is refused"
        );
        assert!(
            take_platform_login_state(pool, "never-existed")
                .await
                .expect("take succeeds")
                .is_none(),
            "and an unknown state is refused identically"
        );
    }

    /// Purging expired state is housekeeping, never what makes it unusable.
    #[tokio::test]
    async fn purging_expired_state_removes_only_abandoned_logins() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();

        insert_platform_login_state(
            pool,
            "state-live",
            "v",
            "n",
            ISSUER,
            "https://wyrd.example/callback",
            Utc::now() + Duration::minutes(5),
        )
        .await
        .expect("live state inserts");
        insert_platform_login_state(
            pool,
            "state-dead",
            "v",
            "n",
            ISSUER,
            "https://wyrd.example/callback",
            Utc::now() - Duration::minutes(5),
        )
        .await
        .expect("dead state inserts");

        assert_eq!(
            purge_expired_platform_login_state(pool)
                .await
                .expect("purge succeeds"),
            1,
            "only the abandoned login is purged"
        );
        assert!(
            take_platform_login_state(pool, "state-live")
                .await
                .expect("take succeeds")
                .is_some(),
            "the live login is untouched"
        );
    }

    /// A subject pins to a pre-registered principal once, and only once.
    ///
    /// The `subject IS NULL` predicate is the whole safety property: after the
    /// first login the claim no longer matches, so a second subject presenting
    /// the same claim cannot take the principal over.
    #[tokio::test]
    async fn a_subject_pins_once_and_cannot_be_taken_over() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        let principal = register(&fixture, "ops-lead", "ops@example.com").await;

        let pinned = pin_platform_identity(pool, ISSUER, "ops@example.com", "subject-alice")
            .await
            .expect("pin succeeds")
            .expect("the registration matched");
        assert_eq!(pinned, principal);

        // A different subject presenting the same claim later gets nothing:
        // the registration is no longer unpinned.
        assert!(
            pin_platform_identity(pool, ISSUER, "ops@example.com", "subject-mallory")
                .await
                .expect("pin succeeds")
                .is_none(),
            "a second subject cannot capture an already-pinned principal"
        );

        // And the original subject still resolves to the same principal.
        let resolved = platform_identity_by_subject(pool, ISSUER, "subject-alice")
            .await
            .expect("lookup succeeds")
            .expect("the pinned identity resolves");
        assert_eq!(resolved.principal_id, principal);
        assert!(
            platform_identity_by_subject(pool, ISSUER, "subject-mallory")
                .await
                .expect("lookup succeeds")
                .is_none(),
            "the subject that failed to pin resolves to nothing"
        );
    }

    /// One claim registers at most one principal.
    ///
    /// Two unpinned registrations sharing a claim would make the first login a
    /// race for which principal gets captured.
    #[tokio::test]
    async fn one_claim_registers_at_most_one_principal() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        register(&fixture, "ops-lead", "ops@example.com").await;

        let second = Uuid::now_v7();
        insert_platform_principal(pool, second, PrincipalKindTag::User, "ops-lead-again")
            .await
            .expect("second principal inserts");
        let mut tx = pool.begin().await.expect("transaction opens");
        let duplicate =
            insert_platform_identity_tx(&mut tx, second, ISSUER, "ops@example.com").await;

        assert!(
            matches!(duplicate, Err(wyrd_sql::SqlError::UniqueViolation { .. })),
            "a repeated claim is refused: {duplicate:?}"
        );
    }

    /// Suspension is what makes the active-status check a live guard.
    #[tokio::test]
    async fn a_suspended_platform_principal_is_no_longer_active() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        let principal = register(&fixture, "ops-lead", "ops@example.com").await;

        assert!(
            platform_principal_by_id(pool, principal)
                .await
                .expect("read succeeds")
                .expect("principal exists")
                .is_active()
        );

        assert!(
            set_platform_principal_status(pool, principal, "suspended")
                .await
                .expect("suspension succeeds"),
            "suspending an active principal changes a row"
        );
        assert!(
            !platform_principal_by_id(pool, principal)
                .await
                .expect("read succeeds")
                .expect("principal exists")
                .is_active(),
            "a suspended principal may no longer authenticate"
        );
        assert!(
            !set_platform_principal_status(pool, principal, "suspended")
                .await
                .expect("repeat succeeds"),
            "suspending it again changes nothing"
        );
    }

    /// A listing shows suspended principals, and their pinned identity.
    #[tokio::test]
    async fn a_listing_shows_revoked_administrators_and_their_identity() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        let principal = register(&fixture, "ops-lead", "ops@example.com").await;
        pin_platform_identity(pool, ISSUER, "ops@example.com", "subject-alice")
            .await
            .expect("pin succeeds");
        set_platform_principal_status(pool, principal, "suspended")
            .await
            .expect("suspension succeeds");

        let listed = list_platform_principals(pool).await.expect("listing reads");
        let row = listed
            .iter()
            .find(|row| row.id == principal)
            .expect("the suspended principal is listed, not hidden");

        assert_eq!(row.status, "suspended");
        assert_eq!(row.match_claim.as_deref(), Some("ops@example.com"));
        assert_eq!(
            row.subject.as_deref(),
            Some("subject-alice"),
            "the listing names the identity this principal resolves from"
        );
    }

    /// The active count is what a lockout guard reads.
    #[tokio::test]
    async fn the_active_count_tracks_suspension() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        let before = count_active_platform_principals(pool)
            .await
            .expect("count reads");

        let principal = register(&fixture, "ops-lead", "ops@example.com").await;
        assert_eq!(
            count_active_platform_principals(pool)
                .await
                .expect("count reads"),
            before + 1
        );

        set_platform_principal_status(pool, principal, "suspended")
            .await
            .expect("suspension succeeds");
        assert_eq!(
            count_active_platform_principals(pool)
                .await
                .expect("count reads"),
            before,
            "a suspended principal no longer counts as a way in"
        );
    }

    /// The connection is a singleton: configuring again replaces it.
    #[tokio::test]
    async fn configuring_the_connection_again_replaces_it() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        let mapping = serde_json::json!({ "subject": ["sub"], "email": ["email"] });

        for audience in ["first-audience", "second-audience"] {
            upsert_platform_oidc_connection(
                pool,
                ISSUER,
                "https://idp.example.com/jwks",
                audience,
                "wyrd-platform",
                "Public",
                &mapping,
                300,
                None,
            )
            .await
            .expect("connection upserts");
        }

        let connection = platform_oidc_connection(pool)
            .await
            .expect("read succeeds")
            .expect("a connection is configured");
        assert_eq!(
            connection.expected_audience, "second-audience",
            "the second configuration replaced the first rather than adding one"
        );
    }
}
