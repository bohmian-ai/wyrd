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
        delete_platform_oidc_connection, insert_platform_identity_tx, insert_platform_login_state,
        pin_platform_identity, platform_identity_by_subject, platform_oidc_connection,
        purge_expired_platform_login_state, take_platform_login_state,
        upsert_platform_oidc_connection,
    };
    use wyrd_sql::queries::platform::principal_grants::set_platform_grant;
    use wyrd_sql::queries::platform::principals::{
        StatusChange, insert_platform_principal, list_platform_principals,
        platform_principal_by_id, set_platform_principal_status,
    };

    /// Skip when no database is configured, matching the sibling suites.
    fn database_url() -> Option<String> {
        std::env::var("WYRD_DATABASE_URL").ok()
    }

    /// The issuer every test in this module registers against.
    const ISSUER: &str = "https://idp.example.com/realms/platform";

    /// The grant the lockout guard requires a surviving administrator to hold.
    ///
    /// Spelled here rather than imported so this suite proves the storage-level
    /// containment test, not the server's idea of what the set contains.
    fn required_grant() -> serde_json::Value {
        serde_json::json!(["tenants:write", "platform_identity:write"])
    }

    /// Give a registered principal the required grant.
    async fn grant(fixture: &PgFixture, principal: Uuid) {
        set_platform_grant(fixture.operator_pool(), principal, &required_grant())
            .await
            .expect("grant installs");
    }

    /// Apply one guarded status change on its own transaction.
    ///
    /// Mirrors the route: the guard, the write, and (in the server) the
    /// authorization record share one transaction, and only a permitted change
    /// commits. A refusal rolls back, which is what makes the advisory lock
    /// serialize concurrent suspensions.
    async fn set_status(fixture: &PgFixture, id: Uuid, status: &str) -> StatusChange {
        let mut conn = fixture
            .operator_pool()
            .begin_platform_audited()
            .await
            .expect("transaction opens");
        let outcome = set_platform_principal_status(&mut conn, id, status, &required_grant())
            .await
            .expect("guard runs");
        if outcome == StatusChange::Changed {
            conn.commit().await.expect("change commits");
        }
        outcome
    }

    /// Install the deployment's one OIDC connection for `issuer`.
    ///
    /// The lockout guard only counts a pinned identity whose issuer is the
    /// connection currently served, so a test that wants a federated
    /// administrator to be a way in has to install one.
    async fn connect(fixture: &PgFixture, issuer: &str) {
        let mut conn = fixture
            .operator_pool()
            .begin_platform_audited()
            .await
            .expect("transaction opens");
        upsert_platform_oidc_connection(
            &mut conn,
            issuer,
            "https://idp.example.com/jwks",
            "wyrd-platform",
            "wyrd-platform",
            "Public",
            &serde_json::json!({"subject": ["sub"], "email": ["email"], "groups": null}),
            300,
            None,
        )
        .await
        .expect("connection upserts");
        conn.commit().await.expect("connection commits");
    }

    /// Register a human administrator who can actually sign in.
    ///
    /// Registration alone is not a way in: the subject is pinned at first login
    /// and the guard counts only pinned identities, so a test that needs a
    /// usable survivor pins one here.
    async fn register_pinned(fixture: &PgFixture, name: &str, claim: &str) -> Uuid {
        let id = register(fixture, name, claim).await;
        pin_platform_identity(
            fixture.operator_pool(),
            ISSUER,
            claim,
            &format!("subject-{name}"),
        )
        .await
        .expect("pin succeeds");
        id
    }

    /// Register a human platform principal awaiting its first login.
    async fn register(fixture: &PgFixture, name: &str, claim: &str) -> Uuid {
        let pool = fixture.operator_pool();
        let id = Uuid::now_v7();
        let mut conn = pool
            .begin_platform_audited()
            .await
            .expect("transaction opens");
        wyrd_sql::queries::platform::principals::insert_platform_principal_tx(
            &mut conn,
            id,
            PrincipalKindTag::User,
            name,
        )
        .await
        .expect("principal inserts");
        insert_platform_identity_tx(&mut conn, id, ISSUER, claim)
            .await
            .expect("identity inserts");
        conn.commit().await.expect("registration commits");
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
        let mut conn = pool
            .begin_platform_audited()
            .await
            .expect("transaction opens");
        let duplicate =
            insert_platform_identity_tx(&mut conn, second, ISSUER, "ops@example.com").await;

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
        let survivor = register(&fixture, "ops-second", "second@example.com").await;
        grant(&fixture, principal).await;
        grant(&fixture, survivor).await;

        assert!(
            platform_principal_by_id(pool, principal)
                .await
                .expect("read succeeds")
                .expect("principal exists")
                .is_active()
        );

        assert_eq!(
            set_status(&fixture, principal, "suspended").await,
            StatusChange::Changed,
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
        assert_eq!(
            set_status(&fixture, principal, "suspended").await,
            StatusChange::Unchanged,
            "suspending it again changes nothing"
        );
    }

    /// The guard protects the last administrator that can actually get back in.
    ///
    /// Three shapes have to be told apart, and the old count could tell apart
    /// none of them: a principal with no grant is not a way in, a granted
    /// principal with no credential and no pinned identity is not a way in
    /// either, and only the one that is both must be refused.
    #[tokio::test]
    async fn only_a_usable_administrator_is_protected_from_suspension() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        connect(&fixture, ISSUER).await;

        // A registered human with a pinned identity and the required grant is
        // the deployment's only way in.
        let only_way_in = register_pinned(&fixture, "ops-lead", "ops@example.com").await;
        grant(&fixture, only_way_in).await;
        assert_eq!(
            set_status(&fixture, only_way_in, "suspended").await,
            StatusChange::WouldStrandDeployment,
            "the last usable administrator cannot be suspended"
        );

        // An ungranted principal is not protection, so suspending it is free.
        let ungranted = register(&fixture, "ops-observer", "observer@example.com").await;
        assert_eq!(
            set_status(&fixture, ungranted, "suspended").await,
            StatusChange::Changed,
            "a principal with no platform authority was never a way in"
        );

        // Nor is a granted principal nothing can authenticate as: it has no
        // credential and no pinned identity.
        let unreachable = Uuid::now_v7();
        insert_platform_principal(
            fixture.operator_pool(),
            unreachable,
            PrincipalKindTag::GlobalAdmin,
            "ops-orphan",
        )
        .await
        .expect("principal inserts");
        grant(&fixture, unreachable).await;
        assert_eq!(
            set_status(&fixture, unreachable, "suspended").await,
            StatusChange::Changed,
            "authority nothing can authenticate as is not a way back in"
        );
        assert_eq!(
            set_status(&fixture, only_way_in, "suspended").await,
            StatusChange::WouldStrandDeployment,
            "neither of those made the real administrator suspendable"
        );

        // Nor is a granted human whose subject was never pinned. A registration
        // awaiting its first login resolves no token, so it cannot sign in.
        let unpinned = register(&fixture, "ops-pending", "pending@example.com").await;
        grant(&fixture, unpinned).await;
        assert_eq!(
            set_status(&fixture, only_way_in, "suspended").await,
            StatusChange::WouldStrandDeployment,
            "an unpinned registration is not a way in"
        );

        // A second usable administrator does.
        let survivor = register_pinned(&fixture, "ops-second", "second@example.com").await;
        grant(&fixture, survivor).await;
        assert_eq!(
            set_status(&fixture, only_way_in, "suspended").await,
            StatusChange::Changed
        );
    }

    /// A pinned identity stops being a way in when its connection is removed.
    ///
    /// Removal is an exposed route, and the deployment keeps its global
    /// credential afterwards. What it must not do is leave the guard counting
    /// administrators nothing can verify any more.
    #[tokio::test]
    async fn a_removed_connection_stops_its_identities_counting() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        connect(&fixture, ISSUER).await;

        let federated = register_pinned(&fixture, "ops-lead", "ops@example.com").await;
        grant(&fixture, federated).await;

        // A credential holder is the subject under test: it must stay protected
        // while the federated administrator is the only other candidate.
        let machine = Uuid::now_v7();
        insert_platform_principal(pool, machine, PrincipalKindTag::GlobalAdmin, "ops-root")
            .await
            .expect("principal inserts");
        grant(&fixture, machine).await;
        credential(&fixture, machine, "wyp_removed").await;
        assert_eq!(
            set_status(&fixture, machine, "suspended").await,
            StatusChange::Changed,
            "a live federated administrator is a surviving way in"
        );
        set_status(&fixture, machine, "active").await;

        let mut removal = pool
            .begin_platform_audited()
            .await
            .expect("transaction opens");
        delete_platform_oidc_connection(&mut removal)
            .await
            .expect("connection removes");
        removal.commit().await.expect("removal commits");

        assert_eq!(
            set_status(&fixture, machine, "suspended").await,
            StatusChange::WouldStrandDeployment,
            "with the connection gone the credential holder is the last way in"
        );
    }

    /// Issue a live platform credential for `principal`.
    ///
    /// The hash is a literal because the guard only asks whether an unrevoked,
    /// unexpired row exists; no verification happens here.
    async fn credential(fixture: &PgFixture, principal: Uuid, prefix: &str) {
        sqlx::query(
            "INSERT INTO platform.credentials (id, principal_id, prefix, secret_hash)
             VALUES ($1, $2, $3, 'argon2-placeholder')",
        )
        .bind(Uuid::now_v7())
        .bind(principal)
        .bind(prefix)
        .execute(fixture.operator_pool().pool())
        .await
        .expect("credential inserts");
    }

    /// Two administrators suspending each other at once leave one standing.
    ///
    /// Serialization is the whole guarantee: read-then-write without it lets
    /// both transactions see the other's subject still active and both commit,
    /// which is exactly the lockout the guard exists to prevent.
    #[tokio::test]
    async fn concurrent_suspensions_cannot_empty_the_platform() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");

        connect(&fixture, ISSUER).await;
        let first = register_pinned(&fixture, "ops-first", "first@example.com").await;
        let second = register_pinned(&fixture, "ops-second", "second@example.com").await;
        grant(&fixture, first).await;
        grant(&fixture, second).await;

        let (left, right) = tokio::join!(
            set_status(&fixture, first, "suspended"),
            set_status(&fixture, second, "suspended"),
        );
        let outcomes = [left, right];
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| **outcome == StatusChange::Changed)
                .count(),
            1,
            "exactly one suspension wins: {outcomes:?}"
        );
        assert!(
            outcomes.contains(&StatusChange::WouldStrandDeployment),
            "the loser is refused rather than silently applied: {outcomes:?}"
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
        set_status(&fixture, principal, "suspended").await;

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
            let mut conn = pool
                .begin_platform_audited()
                .await
                .expect("transaction opens");
            upsert_platform_oidc_connection(
                &mut conn,
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
            conn.commit().await.expect("connection commits");
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
