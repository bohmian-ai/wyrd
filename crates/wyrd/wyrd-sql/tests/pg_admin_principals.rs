mod pg_tests {
    //! Durable proof that identity and credentials are separate concerns.
    //!
    //! These tests drive real Postgres through [`PgFixture`], because every
    //! property under test is a property of the schema — tenancy constraints,
    //! Card-binding agreement, credential ownership, and lifecycle — rather than
    //! of Rust code that could be asserted in isolation.
    //!
    //! Skipped automatically when the database environment is unset so the
    //! default suite stays credential-free.

    use chrono::{DateTime, Duration, Utc};
    use sqlx::Row;
    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_sql::queries::auth::{
        credential_belongs_to, insert_api_key, insert_role, insert_service_account, insert_user,
        list_api_key_metadata, list_user_roles, replace_user_roles, tenant_admin_principal_id,
    };
    use wyrd_sql::queries::platform::credentials::{
        insert_platform_credential_tx, list_platform_credentials, platform_credential_by_prefix,
        revoke_platform_credential,
    };
    use wyrd_sql::queries::platform::principal_grants::{
        platform_grant_for_principal, set_platform_grant,
    };
    use wyrd_sql::queries::platform::principals::{
        insert_platform_principal, platform_principal_by_id,
    };

    /// Skip when no database is configured, matching the sibling Postgres suites.
    fn database_url() -> Option<String> {
        std::env::var("WYRD_DATABASE_URL").ok()
    }

    /// Insert one credential on its own committed transaction.
    ///
    /// Credential writes run on the caller's transaction so they commit with
    /// the allowance that permitted them; a test that only needs the row
    /// standing commits one immediately.
    async fn insert_credential(
        fixture: &PgFixture,
        id: Uuid,
        principal: Uuid,
        prefix: &str,
        secret_hash: &str,
        expires_at: Option<DateTime<Utc>>,
    ) {
        let mut conn = fixture
            .operator_pool()
            .begin_platform_audited()
            .await
            .expect("transaction opens");
        insert_platform_credential_tx(&mut conn, id, principal, prefix, secret_hash, expires_at)
            .await
            .expect("credential inserts");
        conn.commit().await.expect("credential commits");
    }

    /// Revoke one credential on its own committed transaction.
    async fn revoke_credential(fixture: &PgFixture, id: Uuid) -> bool {
        let mut conn = fixture
            .operator_pool()
            .begin_platform_audited()
            .await
            .expect("transaction opens");
        let revoked = revoke_platform_credential(&mut conn, id)
            .await
            .expect("revocation succeeds");
        conn.commit().await.expect("revocation commits");
        revoked
    }

    /// Store a distinguishable non-secret stand-in for an Argon2 verifier.
    ///
    /// These tests prove storage and lifecycle semantics, not hashing; the
    /// hashing contract belongs to the credential issuer that owns it.
    fn verifier(label: &str) -> String {
        format!("$argon2id$v=19$stub${label}")
    }

    /// A principal is durable on its own: it exists, reads back, and holds its
    /// grant with no credential anywhere in the store.
    #[tokio::test]
    async fn principal_persists_and_is_authorized_without_any_credential() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        let id = Uuid::now_v7();

        insert_platform_principal(pool, id, PrincipalKindTag::GlobalAdmin, "root")
            .await
            .expect("principal inserts");
        set_platform_grant(pool, id, &serde_json::json!([{"resource":"tenants"}]))
            .await
            .expect("grant inserts");

        let row = platform_principal_by_id(pool, id)
            .await
            .expect("read succeeds")
            .expect("principal exists");
        assert!(row.is_active());
        assert_eq!(row.name, "root");
        assert!(
            platform_grant_for_principal(pool, id)
                .await
                .expect("grant read succeeds")
                .is_some(),
            "authority is held by the principal, not by a credential"
        );
        assert!(
            list_platform_credentials(pool, id)
                .await
                .expect("credential listing succeeds")
                .is_empty(),
            "the principal exists with no credential at all"
        );
    }

    /// A platform principal cannot be given a tenant, because the table has no
    /// tenant column to give it. The constraint is structural rather than
    /// checked, so no write path can violate it.
    #[tokio::test]
    async fn platform_principals_have_no_tenant_column() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");

        let columns: Vec<String> = sqlx::query(
            "SELECT column_name FROM information_schema.columns
              WHERE table_schema = 'platform' AND table_name = 'principals'",
        )
        .fetch_all(fixture.operator_pool().pool())
        .await
        .expect("column introspection succeeds")
        .into_iter()
        .map(|row| row.get::<String, _>("column_name"))
        .collect();

        assert!(!columns.is_empty(), "platform.principals exists");
        assert!(
            !columns.iter().any(|name| name.contains("tenant")),
            "platform principals carry no tenant: {columns:?}"
        );
    }

    /// A tenant-scope kind cannot be stored at platform scope, so the two
    /// control planes cannot be conflated by a write.
    #[tokio::test]
    async fn platform_store_rejects_a_tenant_scope_kind() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");

        let result = insert_platform_principal(
            fixture.operator_pool(),
            Uuid::now_v7(),
            PrincipalKindTag::TenantAdmin,
            "misplaced",
        )
        .await;

        assert!(
            result.is_err(),
            "tenant-scope kind is refused at platform scope"
        );
        let stored: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM platform.principals WHERE principal_kind <> 'global_admin'",
        )
        .fetch_one(fixture.operator_pool().pool())
        .await
        .expect("count succeeds");
        assert_eq!(
            stored, 0,
            "nothing tenant-scoped reached the platform store"
        );
    }

    /// One principal holds several live credentials at once, which is what makes
    /// rotation an overlap rather than a gap, and revoking one leaves the others
    /// authenticating and the principal's authority untouched.
    #[tokio::test]
    async fn credentials_are_independent_of_each_other_and_of_authority() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        let principal = Uuid::now_v7();
        insert_platform_principal(pool, principal, PrincipalKindTag::GlobalAdmin, "rotating")
            .await
            .expect("principal inserts");
        set_platform_grant(
            pool,
            principal,
            &serde_json::json!([{"resource":"tenants"}]),
        )
        .await
        .expect("grant inserts");

        let (old, new) = (Uuid::now_v7(), Uuid::now_v7());
        insert_credential(
            &fixture,
            old,
            principal,
            "wyrd_global_a",
            &verifier("a"),
            None,
        )
        .await;
        insert_credential(
            &fixture,
            new,
            principal,
            "wyrd_global_b",
            &verifier("b"),
            None,
        )
        .await;

        let now = Utc::now();
        for prefix in ["wyrd_global_a", "wyrd_global_b"] {
            let row = platform_credential_by_prefix(pool, prefix)
                .await
                .expect("lookup succeeds")
                .expect("credential exists");
            assert!(row.is_usable(now), "{prefix} is live before rotation");
        }

        assert!(revoke_credential(&fixture, old).await);

        let revoked = platform_credential_by_prefix(pool, "wyrd_global_a")
            .await
            .expect("lookup succeeds")
            .expect("credential row survives revocation");
        assert!(
            !revoked.is_usable(now),
            "the superseded credential stops working"
        );
        let live = platform_credential_by_prefix(pool, "wyrd_global_b")
            .await
            .expect("lookup succeeds")
            .expect("credential exists");
        assert!(live.is_usable(now), "the replacement keeps working");

        assert!(
            platform_principal_by_id(pool, principal)
                .await
                .expect("read succeeds")
                .expect("principal survives")
                .is_active(),
            "revoking a credential never destroys the principal"
        );
        assert!(
            platform_grant_for_principal(pool, principal)
                .await
                .expect("grant read succeeds")
                .is_some(),
            "rotation never requires reconstructing authority"
        );
    }

    /// Revocation records when authority actually ended and does not rewrite it
    /// on a repeated call.
    #[tokio::test]
    async fn revocation_is_idempotent_and_preserves_the_first_time() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        let principal = Uuid::now_v7();
        let credential = Uuid::now_v7();
        insert_platform_principal(pool, principal, PrincipalKindTag::GlobalAdmin, "once")
            .await
            .expect("principal inserts");
        insert_credential(
            &fixture,
            credential,
            principal,
            "wyrd_global_once",
            &verifier("once"),
            None,
        )
        .await;

        assert!(revoke_credential(&fixture, credential).await);
        let first = platform_credential_by_prefix(pool, "wyrd_global_once")
            .await
            .expect("lookup succeeds")
            .expect("row exists")
            .revoked_at
            .expect("revocation time recorded");

        assert!(
            !revoke_credential(&fixture, credential).await,
            "a repeat revocation reports that it changed nothing"
        );
        let second = platform_credential_by_prefix(pool, "wyrd_global_once")
            .await
            .expect("lookup succeeds")
            .expect("row exists")
            .revoked_at
            .expect("revocation time still recorded");
        assert_eq!(first, second, "the original revocation time is preserved");
    }

    /// Expiry, revocation, principal suspension, and an unknown prefix all end
    /// in the same place: the credential is not usable.
    #[tokio::test]
    async fn every_invalid_condition_yields_an_unusable_credential() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        let now = Utc::now();

        let suspended = Uuid::now_v7();
        insert_platform_principal(pool, suspended, PrincipalKindTag::GlobalAdmin, "suspended")
            .await
            .expect("principal inserts");
        insert_credential(
            &fixture,
            Uuid::now_v7(),
            suspended,
            "wyrd_global_susp",
            &verifier("susp"),
            None,
        )
        .await;
        sqlx::query("UPDATE platform.principals SET status = 'suspended' WHERE id = $1")
            .bind(suspended)
            .execute(pool.pool())
            .await
            .expect("suspension succeeds");

        let expired_owner = Uuid::now_v7();
        insert_platform_principal(
            pool,
            expired_owner,
            PrincipalKindTag::GlobalAdmin,
            "expired",
        )
        .await
        .expect("principal inserts");
        insert_credential(
            &fixture,
            Uuid::now_v7(),
            expired_owner,
            "wyrd_global_exp",
            &verifier("exp"),
            Some(now - Duration::hours(1)),
        )
        .await;

        for prefix in ["wyrd_global_susp", "wyrd_global_exp"] {
            let row = platform_credential_by_prefix(pool, prefix)
                .await
                .expect("lookup succeeds")
                .expect("row exists");
            assert!(!row.is_usable(now), "{prefix} must not be usable");
        }
        assert!(
            platform_credential_by_prefix(pool, "wyrd_global_unknown")
                .await
                .expect("lookup succeeds")
                .is_none(),
            "an unknown prefix resolves to nothing for the caller to distinguish"
        );
    }

    /// Listing exposes only non-secret metadata, and the stored verifier is
    /// never the plaintext that was issued.
    #[tokio::test]
    async fn listing_returns_metadata_and_never_plaintext() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        let principal = Uuid::now_v7();
        let plaintext = "wyrd_global_supersecretvalue";
        insert_platform_principal(pool, principal, PrincipalKindTag::GlobalAdmin, "listed")
            .await
            .expect("principal inserts");
        insert_credential(
            &fixture,
            Uuid::now_v7(),
            principal,
            "wyrd_global_listed",
            &verifier("listed"),
            None,
        )
        .await;

        let listed = list_platform_credentials(pool, principal)
            .await
            .expect("listing succeeds");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].prefix, "wyrd_global_listed");
        assert!(listed[0].revoked_at.is_none());
        assert!(listed[0].last_used_at.is_none());

        let stored: String =
            sqlx::query_scalar("SELECT secret_hash FROM platform.credentials WHERE prefix = $1")
                .bind("wyrd_global_listed")
                .fetch_one(pool.pool())
                .await
                .expect("verifier read succeeds");
        assert_ne!(stored, plaintext, "the plaintext is never persisted");
        assert!(
            stored.starts_with("$argon2id$"),
            "only a verifier is stored: {stored}"
        );
    }

    /// A tenant administrative principal holds a credential without binding a
    /// Card, so an administrative identity needs no fabricated Card.
    #[tokio::test]
    async fn tenant_admin_principal_persists_without_a_card() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let principal = Uuid::now_v7();

        insert_service_account(
            &mut conn,
            principal,
            "tenant_admin",
            None,
            "tenant-admin",
            Some("tenant root of trust"),
            Uuid::now_v7(),
        )
        .await
        .expect("card-free administrative principal inserts");

        let row = sqlx::query(
            "SELECT card_kind, card_uid, card_ref, space, version
               FROM wyrd.auth_service_accounts WHERE id = $1",
        )
        .bind(principal)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("principal reads back");

        assert!(row.get::<Option<String>, _>("card_kind").is_none());
        assert!(row.get::<Option<Uuid>, _>("card_uid").is_none());
        assert!(
            row.get::<Option<serde_json::Value>, _>("card_ref")
                .is_none()
        );
        assert!(row.get::<Option<String>, _>("space").is_none());
        assert!(row.get::<Option<String>, _>("version").is_none());
    }

    /// An administrative principal may not bind a Card, so the Card-free rule is
    /// enforced durably rather than only by the write path that observes it.
    #[tokio::test]
    async fn tenant_admin_principal_cannot_bind_a_card() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        let result = sqlx::query(
            "INSERT INTO wyrd.auth_service_accounts
                 (id, data_tenant_id, principal_kind, card_kind, card_uid, card_ref,
                  space, name, version, status, created_by)
             VALUES ($1, $2, 'tenant_admin', 'Service', $3, '{}'::jsonb,
                     'system', 'bogus', '1.0.0', 'active', $4)",
        )
        .bind(Uuid::now_v7())
        .bind(fixture.data_tenant_id().as_uuid())
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .execute(&mut **conn.transaction())
        .await;

        assert!(
            result.is_err(),
            "an administrative principal cannot carry a Card binding"
        );
    }

    /// The tenant-scoped principal queries carry no tenant predicate of their
    /// own, so forced row-level security under [`TenantConn`] must be what
    /// confines them.
    ///
    /// Tenant A holds an administrative principal, its credential, a user, and
    /// a role; tenant B holds a same-named role. Tenant A sees and replaces its
    /// own rows, and resolving the shared role name binds only A's role. Tenant
    /// B sees no administrative principal, no credential metadata, and no
    /// ownership of A's credential.
    ///
    /// [`TenantConn`]: wyrd_sql::TenantConn
    #[tokio::test]
    async fn tenant_principal_queries_are_confined_by_row_level_security() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = fixture
            .seed_additional_tenant(&format!("rls-principals-{}", Uuid::now_v7()))
            .await
            .expect("second tenant seeds");
        let principal = Uuid::now_v7();
        let credential = Uuid::now_v7();
        let user = Uuid::now_v7();
        let role_a = Uuid::now_v7();
        let permissions = serde_json::json!([]);

        let mut conn = fixture.tenant_conn_for(tenant_b).await.expect("B opens");
        insert_role(&mut conn, Uuid::now_v7(), "rls-probe", &permissions, false)
            .await
            .expect("B role inserts");
        conn.commit().await.expect("B commits");

        let mut conn = fixture.tenant_conn_for(tenant_a).await.expect("A opens");
        insert_service_account(
            &mut conn,
            principal,
            "tenant_admin",
            None,
            "tenant-admin",
            None,
            Uuid::now_v7(),
        )
        .await
        .expect("A principal inserts");
        insert_api_key(
            &mut conn,
            credential,
            principal,
            "wyrd_sk_rlsprobe",
            &verifier("rls"),
            principal,
            None,
        )
        .await
        .expect("A credential inserts");
        insert_user(&mut conn, user, Some("rls@example.test"), "oidc", None)
            .await
            .expect("A user inserts");
        insert_role(&mut conn, role_a, "rls-probe", &permissions, false)
            .await
            .expect("A role inserts");
        replace_user_roles(&mut conn, user, &["rls-probe"])
            .await
            .expect("A replaces roles against its own role");
        assert_eq!(
            tenant_admin_principal_id(&mut conn).await.expect("A reads"),
            Some(principal)
        );
        assert_eq!(
            list_api_key_metadata(&mut conn, principal)
                .await
                .expect("A lists")
                .len(),
            1
        );
        assert!(
            credential_belongs_to(&mut conn, credential, principal)
                .await
                .expect("A checks ownership")
        );
        assert_eq!(
            list_user_roles(&mut conn, user)
                .await
                .expect("A lists roles"),
            ["rls-probe"]
        );
        let bound: Uuid =
            sqlx::query_scalar("SELECT role_id FROM wyrd.auth_user_roles WHERE user_id = $1")
                .bind(user)
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("A binding reads");
        assert_eq!(bound, role_a, "the shared role name binds A's role only");
        replace_user_roles(&mut conn, user, &[])
            .await
            .expect("A clears roles");
        assert!(
            list_user_roles(&mut conn, user)
                .await
                .expect("A lists roles")
                .is_empty()
        );
        conn.commit().await.expect("A commits");

        let mut conn = fixture.tenant_conn_for(tenant_b).await.expect("B reopens");
        assert_eq!(
            tenant_admin_principal_id(&mut conn).await.expect("B reads"),
            None
        );
        assert!(
            list_api_key_metadata(&mut conn, principal)
                .await
                .expect("B lists")
                .is_empty()
        );
        assert!(
            !credential_belongs_to(&mut conn, credential, principal)
                .await
                .expect("B checks ownership")
        );
    }
}
