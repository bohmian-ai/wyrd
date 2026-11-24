mod pg_tests {
    //! Durable proof that required company SSO ends password credentials
    //! even when one commits while the policy changes, and that turning the
    //! policy off revives none of them.

    use std::time::Duration;

    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::auth::Sha256Hex;
    use wyrd_sql::TenantConn;
    use wyrd_sql::queries::auth::login_policy::{revoke_non_company_credentials, set_login_policy};
    use wyrd_sql::queries::auth::{
        BrowserSessionAuthority, insert_browser_session, insert_local_user, live_browser_session,
        local_credential_for_update,
    };

    /// Insert a password session named `name` for `user` at `revision` on
    /// `conn`; whether it was recorded.
    async fn password_session(
        conn: &mut TenantConn<'_>,
        name: &str,
        user: Uuid,
        revision: i64,
    ) -> bool {
        insert_browser_session(
            conn,
            &Sha256Hex::digest(name.as_bytes()),
            user,
            BrowserSessionAuthority::Password(revision),
            Duration::from_secs(3600),
        )
        .await
        .expect("insert runs")
        .is_some()
    }

    /// Whether the session named `name` is live.
    async fn live(fixture: &PgFixture, name: &str) -> bool {
        let mut conn = fixture.tenant_conn().await.expect("conn opens");
        let live = live_browser_session(&mut conn, &Sha256Hex::digest(name.as_bytes()))
            .await
            .expect("read runs")
            .is_some();
        conn.commit().await.expect("read commits");
        live
    }

    /// Change the policy to `require` and sweep, committing.
    async fn change(fixture: &PgFixture, require: bool) {
        let mut conn = fixture.tenant_conn().await.expect("conn opens");
        set_login_policy(&mut conn, require)
            .await
            .expect("policy writes");
        revoke_non_company_credentials(&mut conn)
            .await
            .expect("sweep runs");
        conn.commit().await.expect("change commits");
    }

    /// A password session inserted before the policy commits but committed
    /// after its sweep is not live while company SSO is required; a new one
    /// is not recorded; turning the policy off revives neither it nor the
    /// session swept at enablement, while a new password session works.
    ///
    /// # Panics
    /// Panics when a password session is live under the policy or after it
    /// is turned off.
    #[tokio::test]
    async fn required_sso_ends_racing_password_sessions_for_good() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let user = Uuid::now_v7();
        let mut conn = fixture.tenant_conn().await.expect("conn opens");
        insert_local_user(
            &mut conn,
            user,
            "ada",
            "$argon2id$v=19$m=19456,t=2,p=1$x$y",
            None,
        )
        .await
        .expect("user inserts");
        let revision = local_credential_for_update(&mut conn, user)
            .await
            .expect("credential reads")
            .expect("a local credential")
            .password_revision;
        assert!(password_session(&mut conn, "before", user, revision).await);
        conn.commit().await.expect("user commits");

        let mut racing = fixture.tenant_conn().await.expect("conn opens");
        assert!(password_session(&mut racing, "racing", user, revision).await);
        change(&fixture, true).await;
        racing.commit().await.expect("racing commits");
        assert!(!live(&fixture, "before").await, "swept at enablement");
        assert!(!live(&fixture, "racing").await, "refused while required");
        let mut conn = fixture.tenant_conn().await.expect("conn opens");
        assert!(
            !password_session(&mut conn, "during", user, revision).await,
            "not recorded while required"
        );
        conn.commit().await.expect("commits");

        change(&fixture, false).await;
        assert!(!live(&fixture, "before").await, "not revived");
        assert!(!live(&fixture, "racing").await, "not revived");
        let mut conn = fixture.tenant_conn().await.expect("conn opens");
        assert!(password_session(&mut conn, "after", user, revision).await);
        conn.commit().await.expect("commits");
        assert!(
            live(&fixture, "after").await,
            "a new password session works"
        );
    }
}
