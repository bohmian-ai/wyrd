mod pg_tests {
    //! Durable proof of tenant GitHub App installation association.
    //!
    //! Every case runs on real Postgres through tenant RLS connections: the
    //! conditional association, both uniqueness directions under concurrent
    //! claims, stale-transaction refusal, suspension, release and webhook
    //! routing, and the GitHub/OIDC separation of shared login state.
    //!
    //! Skipped automatically when the database environment is unset so the
    //! default suite stays credential-free.

    use std::time::Duration;

    use secrecy::SecretString;
    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{
        ConnectionTester, LoginInitiation, PrincipalId, PrincipalKindTag, Sha256Hex,
    };
    use wyrd_spec::github_connection::GithubRepositoryAccess;
    use wyrd_sql::queries::auth::{
        GithubLoginState, LoginState, bind_github_login_state, consume_github_login_state,
        consume_login_state, insert_github_login_state, insert_login_state,
    };
    use wyrd_sql::queries::github_installations::{
        GithubAssociation, GithubAssociationOutcome, GithubInstallationRow, GithubSlotBinding,
        associate_github_installation, github_installation, release_github_installation,
        remove_github_repositories, set_github_installation_suspended,
    };
    use wyrd_sql::row_types::auth::HumanConnectionBinding;

    const HOST: &str = "github.example.com";

    /// Skip when no database is configured, matching the sibling suites.
    fn database_url() -> Option<String> {
        std::env::var("WYRD_DATABASE_URL").ok()
    }

    /// A repository selection entry.
    fn repo(id: u64) -> GithubRepositoryAccess {
        GithubRepositoryAccess {
            repository_id: id.to_string(),
            full_name: format!("acme/repo-{id}"),
        }
    }

    /// The binding a transaction begun against an empty slot records.
    fn fresh() -> GithubSlotBinding {
        GithubSlotBinding {
            connection_id: Uuid::now_v7(),
            connection_revision: 0,
        }
    }

    /// The binding a transaction begun now in `tenant` records.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn begin(fixture: &PgFixture, tenant: DataTenantId) -> GithubSlotBinding {
        slot(fixture, tenant)
            .await
            .map_or_else(fresh, |row| row.binding())
    }

    /// The tenant's slot as committed.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn slot(fixture: &PgFixture, tenant: DataTenantId) -> Option<GithubInstallationRow> {
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        github_installation(&mut conn).await.expect("slot reads")
    }

    /// Associate `installation` with `repos` in `tenant` from `begun`, and
    /// commit unless the database aborted the transaction.
    ///
    /// # Panics
    /// Panics when a statement fails.
    async fn associate(
        fixture: &PgFixture,
        tenant: DataTenantId,
        begun: GithubSlotBinding,
        installation: i64,
        repos: &[GithubRepositoryAccess],
    ) -> GithubAssociationOutcome {
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        let outcome = associate_github_installation(
            &mut conn,
            begun,
            GithubAssociation {
                host: HOST,
                installation_id: installation,
                repositories: repos,
            },
        )
        .await
        .expect("association runs");
        if matches!(outcome, GithubAssociationOutcome::Associated(_)) {
            conn.commit().await.expect("association commits");
        } else {
            conn.rollback().await.expect("refusal rolls back");
        }
        outcome
    }

    /// Release `tenant`'s association and commit.
    ///
    /// # Panics
    /// Panics when the statement fails.
    async fn release(fixture: &PgFixture, tenant: DataTenantId) -> bool {
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        let released = release_github_installation(&mut conn, None)
            .await
            .expect("release runs");
        conn.commit().await.expect("release commits");
        released.is_some()
    }

    /// The association applies only at the bound slot and revision: a fresh
    /// association creates the slot, the same installation refreshes its
    /// selection, a different installation is refused, and a transaction
    /// begun before a disconnect or a reconnect never restores anything.
    ///
    /// # Panics
    /// Panics when any transition answers differently.
    #[tokio::test]
    async fn association_is_conditional_on_the_bound_slot_revision() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();

        let first = begin(&fixture, tenant).await;
        let racing_first = begin(&fixture, tenant).await;
        let GithubAssociationOutcome::Associated(row) =
            associate(&fixture, tenant, first, 11, &[repo(1), repo(2)]).await
        else {
            panic!("a fresh slot associates");
        };
        assert_eq!(row.revision, 1);
        assert_eq!(row.installation_id, Some(11));
        assert_eq!(row.host.as_deref(), Some(HOST));
        assert_eq!(row.repositories.0, vec![repo(1), repo(2)]);
        assert!(row.connected_at.is_some() && row.suspended_at.is_none());
        assert_eq!(
            associate(&fixture, tenant, racing_first, 11, &[repo(1)]).await,
            GithubAssociationOutcome::Stale,
            "a second fresh transaction cannot recreate an existing slot"
        );

        let refresh = row.binding();
        let GithubAssociationOutcome::Associated(refreshed) =
            associate(&fixture, tenant, refresh, 11, &[repo(2)]).await
        else {
            panic!("the same installation refreshes its selection");
        };
        assert_eq!(refreshed.revision, 2);
        assert_eq!(refreshed.repositories.0, vec![repo(2)]);
        assert_eq!(refreshed.connected_at, row.connected_at);
        assert_eq!(
            associate(&fixture, tenant, refresh, 11, &[repo(1)]).await,
            GithubAssociationOutcome::Stale,
            "a consumed revision is stale"
        );
        assert_eq!(
            associate(&fixture, tenant, refreshed.binding(), 12, &[repo(9)]).await,
            GithubAssociationOutcome::TenantOccupied,
            "a tenant associates one installation"
        );

        let before_disconnect = begin(&fixture, tenant).await;
        assert!(release(&fixture, tenant).await, "disconnect releases");
        assert!(!release(&fixture, tenant).await, "nothing left to release");
        let released = slot(&fixture, tenant).await.expect("the slot remains");
        assert_eq!(released.installation_id, None);
        assert_eq!(released.host, None);
        assert!(released.repositories.0.is_empty());
        assert_eq!(
            associate(&fixture, tenant, before_disconnect, 11, &[repo(1)]).await,
            GithubAssociationOutcome::Stale,
            "a callback begun before disconnect cannot restore the association"
        );

        let before_reconnect = begin(&fixture, tenant).await;
        let reconnect = begin(&fixture, tenant).await;
        assert!(matches!(
            associate(&fixture, tenant, reconnect, 12, &[repo(9)]).await,
            GithubAssociationOutcome::Associated(_)
        ));
        assert_eq!(
            associate(&fixture, tenant, before_reconnect, 11, &[repo(1)]).await,
            GithubAssociationOutcome::Stale,
            "a callback begun before a reconnect cannot replace it"
        );
        assert_eq!(
            slot(&fixture, tenant)
                .await
                .and_then(|row| row.installation_id),
            Some(12)
        );
    }

    /// One installation belongs to at most one tenant: a claimed or
    /// suspended installation refuses another tenant, release frees it, and
    /// the webhook route names only the current owner.
    ///
    /// # Panics
    /// Panics when another tenant takes or sees the installation.
    #[tokio::test]
    async fn installation_belongs_to_one_tenant_until_released() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = fixture
            .seed_additional_tenant("github-b")
            .await
            .expect("second tenant seeds");
        let route = |id| fixture.wyrd_postgres().github_installation_tenant(HOST, id);

        assert!(matches!(
            associate(&fixture, tenant_a, fresh(), 21, &[repo(1)]).await,
            GithubAssociationOutcome::Associated(_)
        ));
        assert_eq!(route(21).await.expect("route resolves"), Some(tenant_a));
        assert_eq!(
            associate(&fixture, tenant_b, fresh(), 21, &[repo(1)]).await,
            GithubAssociationOutcome::InstallationClaimed
        );
        assert!(slot(&fixture, tenant_b).await.is_none(), "B wrote nothing");
        assert!(
            !release(&fixture, tenant_b).await,
            "B cannot release A's association"
        );

        let mut conn = fixture.tenant_conn_for(tenant_a).await.expect("conn opens");
        let suspended = set_github_installation_suspended(&mut conn, HOST, 21, true)
            .await
            .expect("suspend runs")
            .expect("the associated installation suspends");
        assert!(suspended.suspended_at.is_some());
        assert_eq!(
            suspended.installation_id,
            Some(21),
            "suspension keeps ownership"
        );
        assert!(
            set_github_installation_suspended(&mut conn, HOST, 21, true)
                .await
                .expect("suspend runs")
                .is_none(),
            "an unchanged suspension writes nothing"
        );
        assert!(
            set_github_installation_suspended(&mut conn, HOST, 99, true)
                .await
                .expect("suspend runs")
                .is_none(),
            "another installation is not suspended"
        );
        conn.commit().await.expect("suspension commits");
        assert_eq!(route(21).await.expect("route resolves"), Some(tenant_a));
        assert_eq!(
            associate(&fixture, tenant_b, fresh(), 21, &[repo(1)]).await,
            GithubAssociationOutcome::InstallationClaimed,
            "a suspended installation stays claimed"
        );

        let mut conn = fixture.tenant_conn_for(tenant_a).await.expect("conn opens");
        let unsuspended = set_github_installation_suspended(&mut conn, HOST, 21, false)
            .await
            .expect("unsuspend runs")
            .expect("the installation unsuspends");
        assert!(unsuspended.suspended_at.is_none());
        conn.commit().await.expect("unsuspension commits");

        assert!(release(&fixture, tenant_a).await);
        assert_eq!(route(21).await.expect("route resolves"), None);
        assert!(matches!(
            associate(&fixture, tenant_b, fresh(), 21, &[repo(1)]).await,
            GithubAssociationOutcome::Associated(_)
        ));
        assert_eq!(route(21).await.expect("route resolves"), Some(tenant_b));
        assert_eq!(route(22).await.expect("route resolves"), None);
    }

    /// Concurrent claims resolve in Postgres: two tenants racing for one
    /// installation, and one tenant racing for two installations, each leave
    /// exactly one winner.
    ///
    /// # Panics
    /// Panics when both or neither claim wins.
    #[tokio::test]
    async fn concurrent_claims_have_one_winner_in_both_directions() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = fixture
            .seed_additional_tenant("github-race-b")
            .await
            .expect("second tenant seeds");
        let repos = [repo(1)];
        let claim = |installation| GithubAssociation {
            host: HOST,
            installation_id: installation,
            repositories: &repos,
        };

        // Two tenants, one installation: B's claim waits on A's uncommitted
        // index entry and is refused once A commits.
        let mut first = fixture.tenant_conn_for(tenant_a).await.expect("conn opens");
        assert!(matches!(
            associate_github_installation(&mut first, fresh(), claim(31))
                .await
                .expect("A claims"),
            GithubAssociationOutcome::Associated(_)
        ));
        let mut second = fixture.tenant_conn_for(tenant_b).await.expect("conn opens");
        let (contender, ()) = tokio::join!(
            associate_github_installation(&mut second, fresh(), claim(31)),
            async { first.commit().await.expect("A commits") },
        );
        assert_eq!(
            contender.expect("B's claim resolves"),
            GithubAssociationOutcome::InstallationClaimed
        );
        second.rollback().await.expect("B rolls back");

        // One tenant, two installations from the same released revision: the
        // second waits on the slot lock and finds its revision consumed.
        assert!(release(&fixture, tenant_a).await);
        let begun = begin(&fixture, tenant_a).await;
        let mut first = fixture.tenant_conn_for(tenant_a).await.expect("conn opens");
        assert!(matches!(
            associate_github_installation(&mut first, begun, claim(32))
                .await
                .expect("first claims"),
            GithubAssociationOutcome::Associated(_)
        ));
        let mut second = fixture.tenant_conn_for(tenant_a).await.expect("conn opens");
        let (contender, ()) = tokio::join!(
            associate_github_installation(&mut second, begun, claim(33)),
            async { first.commit().await.expect("first commits") },
        );
        assert_eq!(
            contender.expect("second resolves"),
            GithubAssociationOutcome::Stale
        );
        second.rollback().await.expect("second rolls back");

        // One fresh tenant racing two first associations: the slot's primary
        // key admits one.
        let mut first = fixture.tenant_conn_for(tenant_b).await.expect("conn opens");
        assert!(matches!(
            associate_github_installation(&mut first, fresh(), claim(34))
                .await
                .expect("first claims"),
            GithubAssociationOutcome::Associated(_)
        ));
        let mut second = fixture.tenant_conn_for(tenant_b).await.expect("conn opens");
        let (contender, ()) = tokio::join!(
            associate_github_installation(&mut second, fresh(), claim(35)),
            async { first.commit().await.expect("first commits") },
        );
        assert_eq!(
            contender.expect("second resolves"),
            GithubAssociationOutcome::Stale
        );
        second.rollback().await.expect("second rolls back");
        assert_eq!(
            slot(&fixture, tenant_b)
                .await
                .and_then(|row| row.installation_id),
            Some(34)
        );
        assert_eq!(
            slot(&fixture, tenant_a)
                .await
                .and_then(|row| row.installation_id),
            Some(32)
        );
    }

    /// Forced RLS confines every slot read and write to its tenant, and an
    /// access-removal webhook drops only the removed, selected repositories.
    ///
    /// # Panics
    /// Panics when a tenant sees or changes another tenant's slot, or the
    /// removal keeps or drops the wrong repositories.
    #[tokio::test]
    async fn slots_are_tenant_confined_and_track_removed_access() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = fixture
            .seed_additional_tenant("github-rls-b")
            .await
            .expect("second tenant seeds");
        assert!(matches!(
            associate(
                &fixture,
                tenant_a,
                fresh(),
                41,
                &[repo(1), repo(2), repo(3)]
            )
            .await,
            GithubAssociationOutcome::Associated(_)
        ));

        let (enabled, forced): (bool, bool) = sqlx::query_as(
            "SELECT relrowsecurity, relforcerowsecurity FROM pg_class
              WHERE oid = 'wyrd.github_installations'::regclass",
        )
        .fetch_one(fixture.app_pool())
        .await
        .expect("catalog reads");
        assert!(enabled && forced, "RLS is enabled and forced");

        let mut conn = fixture.tenant_conn_for(tenant_b).await.expect("conn opens");
        assert!(
            github_installation(&mut conn)
                .await
                .expect("reads")
                .is_none()
        );
        assert!(
            remove_github_repositories(&mut conn, HOST, 41, &["1".to_owned()])
                .await
                .expect("removal runs")
                .is_none()
        );
        assert!(
            set_github_installation_suspended(&mut conn, HOST, 41, true)
                .await
                .expect("suspend runs")
                .is_none()
        );
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.github_installations")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("count reads");
        assert_eq!(count, 0, "B sees no slot");
        conn.commit().await.expect("B commits");

        let mut conn = fixture.tenant_conn_for(tenant_a).await.expect("conn opens");
        assert!(
            remove_github_repositories(&mut conn, HOST, 41, &["9".to_owned()])
                .await
                .expect("removal runs")
                .is_none(),
            "removing an unselected repository writes nothing"
        );
        let removed =
            remove_github_repositories(&mut conn, HOST, 41, &["1".to_owned(), "3".to_owned()])
                .await
                .expect("removal runs")
                .expect("selected repositories are removed");
        assert_eq!(removed.repositories.0, vec![repo(2)]);
        assert!(removed.suspended_at.is_none());
        conn.commit().await.expect("A commits");
        assert_eq!(
            slot(&fixture, tenant_a).await.map(|row| row.repositories.0),
            Some(vec![repo(2)])
        );
    }

    /// A GitHub connection transaction for `actor` from `slot`, optionally
    /// already bound to `browser`.
    fn github_state(
        actor: PrincipalId,
        slot: GithubSlotBinding,
        browser: Option<Sha256Hex>,
    ) -> GithubLoginState {
        GithubLoginState {
            slot,
            issuer: "https://github.example.com".to_owned(),
            client_id: "Iv1.wyrd".to_owned(),
            redirect_uri: "https://wyrd.example.com/v1/integrations/github/callback".to_owned(),
            code_verifier: SecretString::from("verifier"),
            actor: ConnectionTester {
                principal_id: actor,
                principal_kind: PrincipalKindTag::User,
            },
            browser_binding: browser,
            installation_hint: None,
        }
    }

    /// GitHub connection transactions share login state but never its flow:
    /// the OIDC callback cannot consume one, the GitHub callback cannot
    /// consume an OIDC login, and a GitHub transaction binds and completes
    /// only for its beginning principal's browser, in its tenant, once.
    ///
    /// # Panics
    /// Panics when a cross-flow, foreign, or replayed transition succeeds or
    /// a valid one fails.
    #[tokio::test]
    async fn github_transactions_are_flow_principal_and_browser_bound() {
        if database_url().is_none() {
            return;
        }
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = fixture
            .seed_additional_tenant("github-state-b")
            .await
            .expect("second tenant seeds");
        let actor = PrincipalId::new(Uuid::now_v7());
        let stranger = PrincipalId::new(Uuid::now_v7());
        let browser = Sha256Hex::digest(b"browser-a");
        let other_browser = Sha256Hex::digest(b"browser-b");
        let ttl = Duration::from_mins(5);

        let github = Sha256Hex::digest(b"github-state");
        let mut conn = fixture.tenant_conn_for(tenant_a).await.expect("conn opens");
        assert!(
            insert_github_login_state(&mut conn, &github, &github_state(actor, fresh(), None), ttl)
                .await
                .expect("inserts")
        );
        let oidc = Sha256Hex::digest(b"oidc-state");
        assert!(
            insert_login_state(
                &mut conn,
                &oidc,
                &LoginState {
                    connection: HumanConnectionBinding {
                        connection_id: Uuid::now_v7(),
                        connection_revision: 1,
                    },
                    issuer: "https://idp.example.com".to_owned(),
                    client_id: "wyrd".to_owned(),
                    redirect_uri: "https://wyrd.example.com/auth/callback".to_owned(),
                    code_verifier: SecretString::from("verifier"),
                    nonce: "nonce".to_owned(),
                    initiation: LoginInitiation::Browser(browser),
                },
                ttl,
            )
            .await
            .expect("inserts")
        );
        conn.commit().await.expect("commits");

        let mut conn = fixture.tenant_conn_for(tenant_a).await.expect("conn opens");
        assert!(
            consume_login_state(&mut conn, &github)
                .await
                .expect("runs")
                .is_none(),
            "the OIDC callback cannot consume a GitHub transaction"
        );
        assert!(
            bind_github_login_state(&mut conn, &oidc, actor, &browser, None)
                .await
                .expect("runs")
                .is_none(),
            "an OIDC login is never bound as a GitHub transaction"
        );
        assert!(
            consume_github_login_state(&mut conn, &oidc, actor, &browser)
                .await
                .expect("runs")
                .is_none(),
            "the GitHub callback cannot consume an OIDC login"
        );
        assert!(
            consume_github_login_state(&mut conn, &github, actor, &browser)
                .await
                .expect("runs")
                .is_none(),
            "an unbound transaction cannot complete"
        );
        assert!(
            bind_github_login_state(&mut conn, &github, stranger, &browser, Some(7))
                .await
                .expect("runs")
                .is_none(),
            "another principal's browser cannot bind it"
        );
        conn.commit().await.expect("commits");

        let mut conn = fixture.tenant_conn_for(tenant_b).await.expect("conn opens");
        assert!(
            bind_github_login_state(&mut conn, &github, actor, &browser, Some(7))
                .await
                .expect("runs")
                .is_none(),
            "another tenant cannot bind it"
        );
        conn.commit().await.expect("commits");

        let mut conn = fixture.tenant_conn_for(tenant_a).await.expect("conn opens");
        let bound = bind_github_login_state(&mut conn, &github, actor, &browser, Some(7))
            .await
            .expect("runs")
            .expect("the beginning principal binds it");
        assert_eq!(bound.browser_binding, Some(browser));
        assert_eq!(bound.installation_hint, Some(7));
        assert!(
            bind_github_login_state(&mut conn, &github, actor, &other_browser, Some(8))
                .await
                .expect("runs")
                .is_none(),
            "a bound transaction is never rebound"
        );
        assert!(
            consume_github_login_state(&mut conn, &github, actor, &other_browser)
                .await
                .expect("runs")
                .is_none(),
            "another browser cannot complete it"
        );
        assert!(
            consume_github_login_state(&mut conn, &github, stranger, &browser)
                .await
                .expect("runs")
                .is_none(),
            "another principal cannot complete it"
        );
        conn.commit().await.expect("commits");

        let mut conn = fixture.tenant_conn_for(tenant_b).await.expect("conn opens");
        assert!(
            consume_github_login_state(&mut conn, &github, actor, &browser)
                .await
                .expect("runs")
                .is_none(),
            "another tenant cannot complete it"
        );
        conn.commit().await.expect("commits");

        let mut conn = fixture.tenant_conn_for(tenant_a).await.expect("conn opens");
        let consumed = consume_github_login_state(&mut conn, &github, actor, &browser)
            .await
            .expect("runs")
            .expect("the bound browser completes it");
        assert_eq!(consumed.installation_hint, Some(7));
        assert_eq!(consumed.actor.principal_id, actor);
        assert!(
            consume_github_login_state(&mut conn, &github, actor, &browser)
                .await
                .expect("runs")
                .is_none(),
            "a consumed transaction is never replayed"
        );
        assert!(
            consume_login_state(&mut conn, &oidc)
                .await
                .expect("runs")
                .is_some(),
            "the OIDC login is still pending for its own callback"
        );
        conn.commit().await.expect("commits");

        let expired = Sha256Hex::digest(b"github-expired");
        let mut conn = fixture.tenant_conn_for(tenant_a).await.expect("conn opens");
        assert!(
            insert_github_login_state(
                &mut conn,
                &expired,
                &github_state(actor, fresh(), Some(browser)),
                Duration::ZERO,
            )
            .await
            .expect("inserts")
        );
        assert!(
            consume_github_login_state(&mut conn, &expired, actor, &browser)
                .await
                .expect("runs")
                .is_none(),
            "an expired transaction cannot complete"
        );
        conn.commit().await.expect("commits");
    }
}
