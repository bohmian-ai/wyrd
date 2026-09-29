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
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_sql::queries::auth::{
        ApiKeyStatus, api_key_by_prefix, api_key_status_by_prefix, credential_belongs_to,
        delete_service_account, insert_api_key, insert_refresh_token, insert_role,
        insert_service_account, insert_user, list_api_key_metadata, list_service_account_roles,
        list_user_roles, provision_system_principal, replace_user_roles,
        service_account_by_card_ref, service_account_by_id, suspend_service_account_principal,
        system_principal_id, tenant_admin_principal_id,
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
    /// Reads a platform credential's durable revocation time.
    ///
    /// # Panics
    ///
    /// Panics when the read fails or the credential was never revoked.
    async fn revoked_at_of(fixture: &PgFixture, id: Uuid) -> DateTime<Utc> {
        sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            "SELECT revoked_at FROM platform.credentials WHERE id = $1",
        )
        .bind(id)
        .fetch_one(fixture.operator_pool().pool())
        .await
        .expect("revocation time is readable")
        .expect("revocation time recorded")
    }

    async fn insert_credential(
        fixture: &PgFixture,
        id: Uuid,
        principal: Uuid,
        prefix: &str,
        secret_hash: &str,
        lifetime: Option<std::time::Duration>,
    ) {
        let mut conn = fixture
            .operator_pool()
            .begin_platform_audited()
            .await
            .expect("transaction opens");
        insert_platform_credential_tx(&mut conn, id, principal, prefix, secret_hash, lifetime)
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

        for prefix in ["wyrd_global_a", "wyrd_global_b"] {
            let row = platform_credential_by_prefix(pool, prefix)
                .await
                .expect("lookup succeeds")
                .expect("credential exists");
            assert!(row.usable, "{prefix} is live before rotation");
        }

        assert!(revoke_credential(&fixture, old).await);

        let revoked = platform_credential_by_prefix(pool, "wyrd_global_a")
            .await
            .expect("lookup succeeds")
            .expect("credential row survives revocation");
        assert!(!revoked.usable, "the superseded credential stops working");
        let live = platform_credential_by_prefix(pool, "wyrd_global_b")
            .await
            .expect("lookup succeeds")
            .expect("credential exists");
        assert!(live.usable, "the replacement keeps working");

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
        let first = revoked_at_of(&fixture, credential).await;

        assert!(
            !revoke_credential(&fixture, credential).await,
            "a repeat revocation reports that it changed nothing"
        );
        let second = revoked_at_of(&fixture, credential).await;
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
            // A zero lifetime expires the moment PostgreSQL writes it, so
            // every later statement sees it expired without host-clock aging.
            Some(std::time::Duration::ZERO),
        )
        .await;

        for prefix in ["wyrd_global_susp", "wyrd_global_exp"] {
            let row = platform_credential_by_prefix(pool, prefix)
                .await
                .expect("lookup succeeds")
                .expect("row exists");
            assert!(!row.usable, "{prefix} must not be usable");
        }
        assert!(
            platform_credential_by_prefix(pool, "wyrd_global_unknown")
                .await
                .expect("lookup succeeds")
                .is_none(),
            "an unknown prefix resolves to nothing for the caller to distinguish"
        );
    }

    /// Tenant API-key status and the relative expiry it reports both come from
    /// `PostgreSQL`, so status, active lookup, and the issued expiry agree.
    ///
    /// # Panics
    ///
    /// Panics when the fixture, inserts, or status assertions fail.
    #[tokio::test]
    async fn tenant_api_key_status_and_relative_expiry_use_database_time() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let principal = Uuid::now_v7();
        let mut conn = fixture
            .tenant_conn()
            .await
            .expect("tenant connection opens");
        insert_service_account(
            &mut conn,
            principal,
            "tenant_admin",
            None,
            "api-key-clock",
            None,
            Uuid::now_v7(),
        )
        .await
        .expect("principal inserts");

        // The insert returns the exact stored expiry the issuance consumer
        // reports, derived by PostgreSQL from the requested lifetime.
        let active_id = Uuid::now_v7();
        let (created_at, expires_at) = insert_api_key(
            &mut conn,
            active_id,
            principal,
            "wyrd_sk_active",
            &verifier("active"),
            principal,
            Some(std::time::Duration::from_secs(3600)),
        )
        .await
        .expect("active key inserts");
        let expires_at = expires_at.expect("a bound lifetime yields an expiry");
        let stored: DateTime<Utc> =
            sqlx::query_scalar("SELECT expires_at FROM wyrd.auth_api_keys WHERE id = $1")
                .bind(active_id)
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("stored expiry is readable");
        assert_eq!(
            expires_at, stored,
            "issuance reports exactly the expiry PostgreSQL stored"
        );
        assert_eq!(
            (expires_at - created_at).num_seconds(),
            3600,
            "the stored expiry is the requested lifetime after the insert"
        );

        // A zero lifetime is already expired in database time.
        let expired_id = Uuid::now_v7();
        insert_api_key(
            &mut conn,
            expired_id,
            principal,
            "wyrd_sk_expired",
            &verifier("expired"),
            principal,
            Some(std::time::Duration::ZERO),
        )
        .await
        .expect("expired key inserts");

        let revoked_id = Uuid::now_v7();
        insert_api_key(
            &mut conn,
            revoked_id,
            principal,
            "wyrd_sk_revoked",
            &verifier("revoked"),
            principal,
            Some(std::time::Duration::from_secs(3600)),
        )
        .await
        .expect("revoked key inserts");
        sqlx::query(
            "UPDATE wyrd.auth_api_keys SET revoked_at = statement_timestamp() WHERE id = $1",
        )
        .bind(revoked_id)
        .execute(&mut **conn.transaction())
        .await
        .expect("key revokes");

        assert_eq!(
            api_key_status_by_prefix(&mut conn, "wyrd_sk_active")
                .await
                .expect("status read"),
            ApiKeyStatus::Active
        );
        assert_eq!(
            api_key_status_by_prefix(&mut conn, "wyrd_sk_expired")
                .await
                .expect("status read"),
            ApiKeyStatus::Expired
        );
        assert_eq!(
            api_key_status_by_prefix(&mut conn, "wyrd_sk_revoked")
                .await
                .expect("status read"),
            ApiKeyStatus::Revoked
        );
        assert_eq!(
            api_key_status_by_prefix(&mut conn, "wyrd_sk_unknown")
                .await
                .expect("status read"),
            ApiKeyStatus::Missing
        );

        // Status and the active lookup are decided by the same clock.
        assert!(
            api_key_by_prefix(&mut conn, "wyrd_sk_active")
                .await
                .expect("lookup succeeds")
                .is_some(),
            "an active key resolves"
        );
        for prefix in ["wyrd_sk_expired", "wyrd_sk_revoked"] {
            assert!(
                api_key_by_prefix(&mut conn, prefix)
                    .await
                    .expect("lookup succeeds")
                    .is_none(),
                "{prefix} must not resolve as an active key"
            );
        }
        conn.commit().await.expect("commits");
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

    /// Version of the upgrade migration that admits and backfills the SYSTEM
    /// writer; the backfill proof replays it against an existing tenant.
    const SYSTEM_PRINCIPAL_MIGRATION_VERSION: i64 = 20260601000028;

    /// Provisioning the internal SYSTEM writer is idempotent: every call in a
    /// tenant returns one stable UUIDv7 row that is active, Card-free,
    /// role-free, and credential-free, and each tenant gets its own.
    ///
    /// # Panics
    /// Panics when provisioning diverges between calls, yields a non-v7 id or a
    /// second row, attaches a Card, role, or credential, or crosses tenants.
    #[tokio::test]
    async fn system_principal_provisioning_is_idempotent_and_stable() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_b = fixture
            .seed_additional_tenant(&format!("system-b-{}", Uuid::now_v7()))
            .await
            .expect("second tenant seeds");

        let mut conn = fixture.tenant_conn().await.expect("tenant opens");
        let first = provision_system_principal(&mut conn)
            .await
            .expect("first provisioning succeeds");
        conn.commit().await.expect("first provisioning commits");
        let mut conn = fixture.tenant_conn().await.expect("tenant reopens");
        let second = provision_system_principal(&mut conn)
            .await
            .expect("repeat provisioning succeeds");
        assert_eq!(first, second, "provisioning returns the one stable id");
        assert_eq!(first.get_version_num(), 7, "the SYSTEM id is a UUIDv7");
        assert_eq!(
            system_principal_id(&mut conn)
                .await
                .expect("lookup succeeds"),
            Some(first)
        );
        let rows = sqlx::query(
            "SELECT name, status, card_kind, card_uid, card_ref, space, version \
               FROM wyrd.auth_service_accounts WHERE principal_kind = 'system'",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("system rows read");
        assert_eq!(rows.len(), 1, "exactly one SYSTEM row per tenant");
        let row = &rows[0];
        assert_eq!(row.get::<String, _>("name"), "verification-results-writer");
        assert_eq!(row.get::<String, _>("status"), "active");
        for column in ["card_kind", "space", "version"] {
            assert_eq!(row.get::<Option<String>, _>(column), None, "{column}");
        }
        assert_eq!(row.get::<Option<Uuid>, _>("card_uid"), None);
        assert_eq!(row.get::<Option<serde_json::Value>, _>("card_ref"), None);
        assert!(
            list_service_account_roles(&mut conn, first)
                .await
                .expect("roles read")
                .is_empty()
        );
        assert!(
            list_api_key_metadata(&mut conn, first)
                .await
                .expect("credentials read")
                .is_empty()
        );
        conn.commit().await.expect("tenant A commits");

        let mut conn = fixture.tenant_conn_for(tenant_b).await.expect("B opens");
        assert_eq!(
            system_principal_id(&mut conn)
                .await
                .expect("B lookup succeeds"),
            None,
            "tenant A's writer is invisible to tenant B"
        );
        let tenant_b_writer = provision_system_principal(&mut conn)
            .await
            .expect("B provisions its own writer");
        assert_ne!(tenant_b_writer, first, "each tenant owns a distinct writer");
    }

    /// Every public path that names a machine principal refuses the SYSTEM
    /// writer as though it did not exist, and the store itself refuses giving
    /// it a lifecycle, a Card, a second row, or a refresh session.
    ///
    /// # Panics
    /// Panics when any public lookup resolves the writer, a lifecycle write
    /// changes it, or the store accepts a forbidden shape.
    #[tokio::test]
    async fn system_principal_is_absent_from_public_principal_paths() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let mut conn = fixture.tenant_conn().await.expect("tenant opens");
        let writer = provision_system_principal(&mut conn)
            .await
            .expect("provisioning succeeds");

        assert!(
            service_account_by_id(&mut conn, writer)
                .await
                .expect("lookup succeeds")
                .is_none(),
            "public get, credential, revoke, and exchange lookups omit the writer"
        );
        assert!(
            !suspend_service_account_principal(&mut conn, writer)
                .await
                .expect("suspend runs"),
            "public revocation cannot suspend the writer"
        );
        assert!(
            !delete_service_account(&mut conn, writer)
                .await
                .expect("delete runs"),
            "public deletion cannot delete the writer"
        );
        let verifier = CardRef {
            kind: CardKind::Verifier,
            name: CardName::new("verification-results-writer").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        };
        assert!(
            service_account_by_card_ref(&mut conn, "system", &verifier)
                .await
                .expect("card lookup succeeds")
                .is_none(),
            "workload binding and jwt-bearer resolve no Card-free writer"
        );
        assert_eq!(
            system_principal_id(&mut conn)
                .await
                .expect("lookup succeeds"),
            Some(writer),
            "the writer is still active after every refused lifecycle write"
        );
        conn.commit().await.expect("refusals commit");

        let refusals: [(&str, &str); 3] = [
            (
                "suspension",
                "UPDATE wyrd.auth_service_accounts SET status = 'suspended' \
                  WHERE principal_kind = 'system'",
            ),
            (
                "a Card binding",
                "UPDATE wyrd.auth_service_accounts \
                    SET card_kind = 'Verifier', card_uid = gen_random_uuid(), \
                        card_ref = '{}'::jsonb, space = 'prod', version = '1.0.0' \
                  WHERE principal_kind = 'system'",
            ),
            (
                "a second writer",
                "INSERT INTO wyrd.auth_service_accounts \
                    (id, data_tenant_id, principal_kind, name, status, created_by) \
                 VALUES (gen_random_uuid(), wyrd.current_tenant(), 'system', 'second', \
                         'active', gen_random_uuid())",
            ),
        ];
        for (label, statement) in refusals {
            let mut conn = fixture.tenant_conn().await.expect("tenant reopens");
            let result = sqlx::query(statement)
                .execute(&mut **conn.transaction())
                .await;
            assert!(result.is_err(), "the store must refuse {label}");
        }

        let mut conn = fixture.tenant_conn().await.expect("tenant reopens");
        let refresh = insert_refresh_token(
            &mut conn,
            Uuid::now_v7(),
            "system",
            writer,
            "system-refresh-probe",
            Utc::now() + Duration::days(1),
        )
        .await;
        assert!(refresh.is_err(), "the writer can hold no refresh session");
    }

    /// The upgrade migration backfills a tenant that predates it with exactly
    /// one SYSTEM writer, and replaying it keeps that same row. The platform
    /// sentinel tenant is not a data tenant and never receives one.
    ///
    /// # Panics
    /// Panics when the replayed migration fails, leaves the tenant without a
    /// writer, replaces or duplicates it, or provisions the sentinel tenant.
    #[tokio::test]
    async fn system_principal_upgrade_backfills_existing_tenants_idempotently() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture
            .seed_additional_tenant(&format!("system-upgrade-{}", Uuid::now_v7()))
            .await
            .expect("pre-existing tenant seeds");
        let pool = fixture.superuser_pool().await.expect("migrator pool");
        let read_writers = || async {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM wyrd.auth_service_accounts \
                  WHERE data_tenant_id = $1 AND principal_kind = 'system'",
            )
            .bind(tenant.as_uuid())
            .fetch_all(&pool)
            .await
            .expect("writer rows read")
        };
        assert!(
            read_writers().await.is_empty(),
            "a tenant seeded after migration starts without a writer"
        );

        let mut backfilled = Vec::new();
        for _ in 0..2 {
            sqlx::query("DELETE FROM wyrd._sqlx_migrations WHERE version = $1")
                .bind(SYSTEM_PRINCIPAL_MIGRATION_VERSION)
                .execute(&pool)
                .await
                .expect("migration ledger row removes");
            wyrd_sql::migrate(&pool)
                .await
                .expect("upgrade migration replays");
            backfilled.push(read_writers().await);
        }

        assert_eq!(
            backfilled[0].len(),
            1,
            "backfill creates exactly one writer"
        );
        assert_eq!(backfilled[0][0].get_version_num(), 7);
        assert_eq!(backfilled[0], backfilled[1], "replay keeps the same writer");
        let sentinel_writers: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM wyrd.auth_service_accounts WHERE data_tenant_id = $1",
        )
        .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("sentinel principals read");
        assert_eq!(
            sentinel_writers, 0,
            "the platform sentinel tenant owns no verification-result writer"
        );
    }
}
