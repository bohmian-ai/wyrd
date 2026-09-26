mod pg_tests {
    //! Durable proof that tenant login state is confined by forced RLS alone.
    //!
    //! The login-state statements carry no tenant predicate: the caller's
    //! [`wyrd_sql::TenantConn`] and the table's forced policy are the only
    //! tenant selection. These cases drive every transition from a second
    //! tenant's connection against the first tenant's row, and prove the one
    //! cross-tenant lookup the callback uses names only the owning tenant.
    //!
    //! Skipped automatically when the database environment is unset so the
    //! default suite stays credential-free.

    use std::time::Duration;

    use secrecy::SecretString;
    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{LoginInitiation, Sha256Hex};
    use wyrd_sql::queries::auth::{
        LoginState, complete_login_state, consume_login_state, insert_login_state,
        redeem_login_completion,
    };
    use wyrd_sql::row_types::auth::HumanConnectionBinding;

    /// Skip when no database is configured, matching the sibling suites.
    fn database_url() -> Option<String> {
        std::env::var("WYRD_DATABASE_URL").ok()
    }

    /// A browser login state bound to the flow whose hash is `flow`.
    fn login_state(flow: Sha256Hex) -> LoginState {
        LoginState {
            connection: HumanConnectionBinding {
                connection_id: Uuid::now_v7(),
                connection_revision: 1,
            },
            issuer: "https://idp.example.com".to_owned(),
            client_id: "wyrd".to_owned(),
            redirect_uri: "https://wyrd.example.com/auth/callback".to_owned(),
            code_verifier: SecretString::from("verifier"),
            nonce: "nonce".to_owned(),
            initiation: LoginInitiation::Browser(flow),
        }
    }

    /// Record a login in `tenant` under `state` and `flow` with `ttl`, and
    /// commit it.
    ///
    /// # Panics
    /// Panics when the row is not written.
    async fn begin(
        fixture: &PgFixture,
        tenant: DataTenantId,
        state: &Sha256Hex,
        flow: Sha256Hex,
        ttl: Duration,
    ) {
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        assert!(
            insert_login_state(&mut conn, state, &login_state(flow), ttl)
                .await
                .expect("state inserts"),
            "the login is recorded"
        );
        conn.commit().await.expect("state commits");
    }

    /// Whether `tenant` can see a row with `state`, expired or not.
    ///
    /// # Panics
    /// Panics when the count query fails.
    async fn visible(fixture: &PgFixture, tenant: DataTenantId, state: &Sha256Hex) -> bool {
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM wyrd.auth_login_state WHERE state_hash = $1)",
        )
        .bind(state.as_bytes().as_slice())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("visibility query runs")
    }

    /// Consume `state` from `tenant`'s connection and commit.
    ///
    /// # Panics
    /// Panics when the statement or commit fails.
    async fn consume(fixture: &PgFixture, tenant: DataTenantId, state: &Sha256Hex) -> bool {
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        let consumed = consume_login_state(&mut conn, state)
            .await
            .expect("consume runs");
        conn.commit().await.expect("consume commits");
        consumed.is_some()
    }

    /// Complete `state` from `tenant`'s connection and commit.
    ///
    /// # Panics
    /// Panics when the statement or commit fails.
    async fn complete(fixture: &PgFixture, tenant: DataTenantId, state: &Sha256Hex) -> bool {
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        let completed = complete_login_state(&mut conn, state, b"sealed", Duration::from_mins(2))
            .await
            .expect("complete runs");
        conn.commit().await.expect("complete commits");
        completed
    }

    /// Redeem the completion bound to `flow` from `tenant`'s connection and
    /// commit.
    ///
    /// # Panics
    /// Panics when the statement or commit fails.
    async fn redeem(fixture: &PgFixture, tenant: DataTenantId, flow: Sha256Hex) -> bool {
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        let sealed = redeem_login_completion(&mut conn, &LoginInitiation::Browser(flow))
            .await
            .expect("redeem runs");
        conn.commit().await.expect("redeem commits");
        sealed.is_some()
    }

    /// Tenant B cannot purge, consume, complete, or redeem tenant A's login
    /// state, while tenant A performs each valid transition exactly once.
    ///
    /// # Panics
    /// Panics when any cross-tenant transition takes effect or a valid one
    /// fails or repeats.
    #[tokio::test]
    async fn login_state_transitions_are_confined_to_the_owning_tenant() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = fixture
            .seed_additional_tenant("login-state-b")
            .await
            .expect("second tenant seeds");

        let expired = Sha256Hex::digest(b"a-expired");
        begin(
            &fixture,
            tenant_a,
            &expired,
            Sha256Hex::digest(b"a-expired-flow"),
            Duration::ZERO,
        )
        .await;
        begin(
            &fixture,
            tenant_b,
            &Sha256Hex::digest(b"b-state"),
            Sha256Hex::digest(b"b-flow"),
            Duration::from_mins(5),
        )
        .await;
        assert!(
            visible(&fixture, tenant_a, &expired).await,
            "tenant B's purge leaves tenant A's expired row alone"
        );
        assert!(!visible(&fixture, tenant_b, &expired).await);

        let state = Sha256Hex::digest(b"a-state");
        let flow = Sha256Hex::digest(b"a-flow");
        begin(&fixture, tenant_a, &state, flow, Duration::from_mins(5)).await;
        assert!(
            !visible(&fixture, tenant_a, &expired).await,
            "tenant A's own purge removes its expired row"
        );

        assert!(
            !consume(&fixture, tenant_b, &state).await,
            "B cannot consume"
        );
        assert!(consume(&fixture, tenant_a, &state).await, "A consumes");
        assert!(!consume(&fixture, tenant_a, &state).await, "once");

        assert!(
            !complete(&fixture, tenant_b, &state).await,
            "B cannot complete"
        );
        assert!(complete(&fixture, tenant_a, &state).await, "A completes");
        assert!(!complete(&fixture, tenant_a, &state).await, "once");

        assert!(!redeem(&fixture, tenant_b, flow).await, "B cannot redeem");
        assert!(redeem(&fixture, tenant_a, flow).await, "A redeems");
        assert!(!redeem(&fixture, tenant_a, flow).await, "once");
    }

    /// The callback's state lookup through `WyrdPostgres` names the owning
    /// tenant of a pending state only; unknown, expired, and consumed states
    /// name none.
    ///
    /// # Panics
    /// Panics when any lookup answers differently.
    #[tokio::test]
    async fn login_state_tenant_names_only_the_owner_of_a_pending_state() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let postgres = fixture.wyrd_postgres();
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = fixture
            .seed_additional_tenant("login-lookup-b")
            .await
            .expect("second tenant seeds");
        let pending_a = Sha256Hex::digest(b"pending-a");
        let pending_b = Sha256Hex::digest(b"pending-b");
        let expired = Sha256Hex::digest(b"expired");
        let consumed = Sha256Hex::digest(b"consumed");
        let long = Duration::from_mins(5);
        begin(
            &fixture,
            tenant_a,
            &pending_a,
            Sha256Hex::digest(b"fa"),
            long,
        )
        .await;
        begin(
            &fixture,
            tenant_b,
            &pending_b,
            Sha256Hex::digest(b"fb"),
            long,
        )
        .await;
        begin(
            &fixture,
            tenant_a,
            &consumed,
            Sha256Hex::digest(b"fc"),
            long,
        )
        .await;
        assert!(consume(&fixture, tenant_a, &consumed).await);
        // Recorded last so no later insert's purge removes it before lookup.
        begin(
            &fixture,
            tenant_a,
            &expired,
            Sha256Hex::digest(b"fe"),
            Duration::ZERO,
        )
        .await;
        assert!(visible(&fixture, tenant_a, &expired).await);

        let lookup = |hash: Sha256Hex| async move {
            postgres
                .login_state_tenant(&hash)
                .await
                .expect("lookup runs")
        };
        assert_eq!(lookup(pending_a).await, Some(tenant_a));
        assert_eq!(lookup(pending_b).await, Some(tenant_b));
        assert_eq!(lookup(Sha256Hex::digest(b"unknown")).await, None);
        assert_eq!(lookup(expired).await, None);
        assert_eq!(lookup(consumed).await, None);
    }
}
