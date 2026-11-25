mod pg_tests {
    //! Durable proof that identity and credentials are separate concerns.
    //!
    //! These tests drive real Postgres through [`PgFixture`], because every
    //! property under test is a property of the schema — tenancy constraints,
    //! workload restrictions, credential ownership, and lifecycle — rather than
    //! of Rust code that could be asserted in isolation.
    //!
    //! Skipped automatically when the database environment is unset so the
    //! default suite stays credential-free.

    use chrono::{DateTime, Duration, Utc};
    use sqlx::Row;
    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::{PgFixture, UnmigratedDatabase};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{
        Attribution, PermissionScope, PrincipalId, PrincipalKindTag, RoleSource, WorkloadFamily,
        WorkloadKind,
    };
    use wyrd_spec::ids::CardName;
    use wyrd_sql::SqlError;
    use wyrd_sql::queries::auth::{
        ApiKeyStatus, PrincipalFilter, SubjectColumns, api_key_by_prefix, api_key_status_by_prefix,
        count_assignable_principals, credential_belongs_to, delete_service_account,
        family_account_id, insert_api_key, insert_group, insert_group_membership,
        insert_local_user, insert_refresh_token, insert_role, insert_role_binding,
        insert_service_account, insert_user, list_api_key_metadata, list_assignable_principals,
        list_role_bindings, list_roles, provision_system_principal, refresh_display_name,
        replace_idp_user_bindings, service_account_by_id, set_role_description,
        set_workload_restriction, suspend_service_account_principal, system_principal_id,
        tenant_admin_principal_id, user_profile,
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
            wyrd_spec::auth::Attribution::direct(wyrd_spec::auth::PrincipalId::new(Uuid::now_v7())),
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
            wyrd_spec::auth::Attribution::direct(wyrd_spec::auth::PrincipalId::new(principal)),
            Some(std::time::Duration::from_hours(1)),
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
            wyrd_spec::auth::Attribution::direct(wyrd_spec::auth::PrincipalId::new(principal)),
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
            wyrd_spec::auth::Attribution::direct(wyrd_spec::auth::PrincipalId::new(principal)),
            Some(std::time::Duration::from_hours(1)),
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
    ///
    /// # Panics
    /// Panics when the fixture or tenant connection cannot start, the
    /// card-free `tenant_admin` insert or its read-back fails, or the stored
    /// row carries a restriction kind or name.
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
            wyrd_spec::auth::Attribution::direct(wyrd_spec::auth::PrincipalId::new(Uuid::now_v7())),
        )
        .await
        .expect("card-free administrative principal inserts");

        let row = sqlx::query(
            "SELECT restriction_kind, restriction_name
               FROM wyrd.auth_service_accounts WHERE id = $1",
        )
        .bind(principal)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("principal reads back");

        assert!(row.get::<Option<String>, _>("restriction_kind").is_none());
        assert!(row.get::<Option<String>, _>("restriction_name").is_none());
    }

    /// Only a Service account may carry a workload restriction, so the rule is
    /// enforced durably rather than only by the write path that observes it.
    ///
    /// # Panics
    /// Panics when the fixture or tenant connection cannot start, or when
    /// Postgres accepts a `tenant_admin` row restricted to a Service family.
    #[tokio::test]
    async fn tenant_admin_principal_cannot_carry_a_workload_restriction() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        let result = sqlx::query(
            "INSERT INTO wyrd.auth_service_accounts
                 (id, data_tenant_id, principal_kind, restriction_kind, restriction_name,
                  name, status, sub_principal_id, act_principal_id)
             VALUES ($1, $2, 'tenant_admin', 'Service', 'bogus', 'bogus', 'active', $3, $3)",
        )
        .bind(Uuid::now_v7())
        .bind(fixture.data_tenant_id().as_uuid())
        .bind(Uuid::now_v7())
        .execute(&mut **conn.transaction())
        .await;

        assert!(
            result.is_err(),
            "an administrative principal cannot carry a workload restriction"
        );
    }

    /// One live account per family: a second restriction to the same family
    /// conflicts on insert and on replacement, replacement is atomic and
    /// idempotent, and releasing or deleting the holder frees the family.
    ///
    /// # Panics
    /// Panics when a duplicate family is accepted, a replacement misreports
    /// its row, or the family account lookup diverges.
    #[tokio::test]
    async fn family_restriction_is_unique_and_replaceable() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let family = WorkloadFamily {
            kind: WorkloadKind::Service,
            name: CardName::new("checkout").expect("static name is valid"),
        };
        let attribution = Attribution::direct(wyrd_spec::auth::PrincipalId::new(Uuid::now_v7()));
        let (first, second) = (Uuid::now_v7(), Uuid::now_v7());
        insert_service_account(
            &mut conn,
            first,
            "service",
            Some(&family),
            "checkout-runtime",
            None,
            attribution,
        )
        .await
        .expect("first account inserts");
        insert_service_account(
            &mut conn,
            second,
            "service",
            None,
            "checkout-next",
            None,
            attribution,
        )
        .await
        .expect("second account inserts");
        assert_eq!(
            family_account_id(&mut conn, &family).await.expect("reads"),
            Some(first)
        );
        assert!(
            set_workload_restriction(&mut conn, first, Some(&family))
                .await
                .expect("unchanged value is idempotent")
        );
        conn.commit().await.expect("setup commits");

        let mut conn = fixture.tenant_conn().await.expect("tenant reopens");
        let duplicate = set_workload_restriction(&mut conn, second, Some(&family)).await;
        assert!(
            matches!(
                duplicate.map_err(SqlError::from),
                Err(SqlError::UniqueViolation { .. })
            ),
            "a second account cannot claim the family"
        );
        drop(conn);

        let mut conn = fixture.tenant_conn().await.expect("tenant reopens");
        assert!(
            set_workload_restriction(&mut conn, first, None)
                .await
                .expect("holder releases")
        );
        assert!(
            set_workload_restriction(&mut conn, second, Some(&family))
                .await
                .expect("successor claims the freed family")
        );
        assert_eq!(
            family_account_id(&mut conn, &family).await.expect("reads"),
            Some(second)
        );
        assert!(
            delete_service_account(&mut conn, second)
                .await
                .expect("successor deletes")
        );
        assert_eq!(
            family_account_id(&mut conn, &family).await.expect("reads"),
            None
        );
        assert!(
            set_workload_restriction(&mut conn, first, Some(&family))
                .await
                .expect("a deleted holder frees the family")
        );
        assert!(
            !set_workload_restriction(&mut conn, second, None)
                .await
                .expect("update runs"),
            "a deleted account cannot be restricted"
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
            wyrd_spec::auth::Attribution::direct(wyrd_spec::auth::PrincipalId::new(Uuid::now_v7())),
        )
        .await
        .expect("A principal inserts");
        insert_api_key(
            &mut conn,
            credential,
            principal,
            "wyrd_sk_rlsprobe",
            &verifier("rls"),
            wyrd_spec::auth::Attribution::direct(wyrd_spec::auth::PrincipalId::new(principal)),
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
        replace_idp_user_bindings(&mut conn, user, &["rls-probe"])
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
        let bindings = list_role_bindings(&mut conn)
            .await
            .expect("A lists bindings");
        assert_eq!(bindings.len(), 1);
        assert_eq!(
            bindings[0].role_id, role_a,
            "the shared role name binds A's role only"
        );
        assert_eq!(bindings[0].subject.user_id, Some(user));
        replace_idp_user_bindings(&mut conn, user, &[])
            .await
            .expect("A clears roles");
        assert!(
            list_role_bindings(&mut conn)
                .await
                .expect("A lists bindings")
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
    const SYSTEM_PRINCIPAL_MIGRATION_VERSION: i64 = 20_260_601_000_028;

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
        let first = provision_system_principal(&mut conn, Attribution::unresolved())
            .await
            .expect("first provisioning succeeds");
        conn.commit().await.expect("first provisioning commits");
        let mut conn = fixture.tenant_conn().await.expect("tenant reopens");
        let second = provision_system_principal(&mut conn, Attribution::unresolved())
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
            "SELECT name, status, restriction_kind, restriction_name \
               FROM wyrd.auth_service_accounts WHERE principal_kind = 'system'",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("system rows read");
        assert_eq!(rows.len(), 1, "exactly one SYSTEM row per tenant");
        let row = &rows[0];
        assert_eq!(row.get::<String, _>("name"), "verification-results-writer");
        assert_eq!(row.get::<String, _>("status"), "active");
        for column in ["restriction_kind", "restriction_name"] {
            assert_eq!(row.get::<Option<String>, _>(column), None, "{column}");
        }
        assert!(
            list_role_bindings(&mut conn)
                .await
                .expect("bindings read")
                .iter()
                .all(|binding| binding.subject.service_account_id != Some(first))
        );
        assert!(
            list_api_key_metadata(&mut conn, first)
                .await
                .expect("credentials read")
                .is_empty()
        );
        conn.commit().await.expect("tenant A commits");

        let mut conn = fixture.tenant_conn_for(tenant_b).await.expect("B opens");
        let seeded_b_writer = system_principal_id(&mut conn)
            .await
            .expect("B lookup succeeds")
            .expect("a seeded tenant has its own writer");
        let tenant_b_writer = provision_system_principal(&mut conn, Attribution::unresolved())
            .await
            .expect("B provisioning succeeds");
        assert_eq!(
            tenant_b_writer, seeded_b_writer,
            "B keeps its seeded writer"
        );
        assert_ne!(tenant_b_writer, first, "each tenant owns a distinct writer");
    }

    /// Every public path that names a machine principal refuses the SYSTEM
    /// writer as though it did not exist, and the store itself refuses giving
    /// it a lifecycle, a workload restriction, a second row, or a refresh session.
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
        let writer = provision_system_principal(&mut conn, Attribution::unresolved())
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
                "a workload restriction",
                "UPDATE wyrd.auth_service_accounts \
                    SET restriction_kind = 'Service', restriction_name = 'writer' \
                  WHERE principal_kind = 'system'",
            ),
            (
                "a second writer",
                "INSERT INTO wyrd.auth_service_accounts \
                    (id, data_tenant_id, principal_kind, name, status, sub_principal_id, act_principal_id) \
                 VALUES (gen_random_uuid(), wyrd.current_tenant(), 'system', 'second', \
                         'active', gen_random_uuid(), gen_random_uuid())",
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
    /// The replay runs on a database stopped at that migration, because a
    /// later migration may reshape the table it writes.
    ///
    /// # Panics
    /// Panics when the replayed migration fails, leaves the tenant without a
    /// writer, replaces or duplicates it, or provisions the sentinel tenant.
    #[tokio::test]
    async fn system_principal_upgrade_backfills_existing_tenants_idempotently() {
        let Some(_) = database_url() else {
            return;
        };
        let database = UnmigratedDatabase::create()
            .await
            .expect("empty database creates");
        let pool = database.migrator_pool().clone();
        let run_to_system_migration = || async {
            let mut conn = pool.acquire().await.expect("migrator connects");
            for statement in [
                "CREATE SCHEMA IF NOT EXISTS platform",
                "CREATE SCHEMA IF NOT EXISTS wyrd",
                "SET search_path TO wyrd, platform, public",
            ] {
                sqlx::query(statement)
                    .execute(&mut *conn)
                    .await
                    .expect("migration bootstrap runs");
            }
            sqlx::migrate!("./migrations")
                .run_to(SYSTEM_PRINCIPAL_MIGRATION_VERSION, &mut *conn)
                .await
                .expect("upgrade migration applies");
            conn.close().await.expect("bootstrap connection closes");
        };
        run_to_system_migration().await;
        let tenant = DataTenantId::new_v7();
        sqlx::query(
            "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status)
             VALUES ($1, 'system-upgrade', 'system-upgrade', 'active')",
        )
        .bind(tenant.as_uuid())
        .execute(&pool)
        .await
        .expect("pre-existing tenant inserts");
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
            "the pre-upgrade tenant starts without a writer"
        );

        let mut backfilled = Vec::new();
        for _ in 0..2 {
            sqlx::query("DELETE FROM wyrd._sqlx_migrations WHERE version = $1")
                .bind(SYSTEM_PRINCIPAL_MIGRATION_VERSION)
                .execute(&pool)
                .await
                .expect("migration ledger row removes");
            run_to_system_migration().await;
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

    /// The display name User `id` stores, as its own profile reads it.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn display_name_of(
        conn: &mut wyrd_sql::TenantConn<'_>,
        id: Uuid,
    ) -> Option<Option<String>> {
        user_profile(conn, id)
            .await
            .expect("profile reads")
            .map(|profile| profile.display_name)
    }

    /// A User's display name is nullable, set and replaced by each nonblank
    /// provider name, and kept when a sign-in omits it or sends a blank one.
    /// A password User has none. Another tenant can neither read nor change
    /// it, and its own names never appear in this tenant's directory.
    ///
    /// # Panics
    /// Panics when a stored name differs or crosses tenants.
    #[tokio::test]
    async fn display_names_are_nullable_refreshed_and_tenant_confined() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_b = fixture
            .seed_additional_tenant(&format!("names-b-{}", Uuid::now_v7()))
            .await
            .expect("second tenant seeds");
        let social = Uuid::now_v7();
        let local = Uuid::now_v7();
        let mut conn = fixture.tenant_conn().await.expect("A opens");
        insert_user(&mut conn, social, Some("ada@example.test"), "oidc", None)
            .await
            .expect("social User inserts");
        insert_local_user(&mut conn, local, "local-only", &verifier("local"), None)
            .await
            .expect("password User inserts");
        assert_eq!(display_name_of(&mut conn, social).await, Some(None));
        for (name, stored) in [
            (Some("Ada"), "Ada"),
            (None, "Ada"),
            (Some("   "), "Ada"),
            (Some(" Ada Lovelace "), "Ada Lovelace"),
        ] {
            refresh_display_name(&mut conn, social, name)
                .await
                .expect("name refreshes");
            assert_eq!(
                display_name_of(&mut conn, social).await,
                Some(Some(stored.to_owned())),
                "after {name:?}"
            );
        }
        let profile = user_profile(&mut conn, local)
            .await
            .expect("profile reads")
            .expect("password User exists");
        assert_eq!(profile.username.as_deref(), Some("local-only"));
        assert_eq!(profile.display_name, None);
        let listed = list_assignable_principals(
            &mut conn,
            PrincipalFilter {
                email: Some("ada@example.test"),
                ..PrincipalFilter::default()
            },
            10,
        )
        .await
        .expect("directory lists");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].display_name.as_deref(), Some("Ada Lovelace"));
        conn.commit().await.expect("A commits");

        let other = Uuid::now_v7();
        let mut conn = fixture.tenant_conn_for(tenant_b).await.expect("B opens");
        assert_eq!(
            display_name_of(&mut conn, social).await,
            None,
            "B cannot read A's User"
        );
        refresh_display_name(&mut conn, social, Some("Mallory"))
            .await
            .expect("B's update runs");
        insert_user(&mut conn, other, Some("bea@example.test"), "oidc", None)
            .await
            .expect("B User inserts");
        refresh_display_name(&mut conn, other, Some("Bea Only"))
            .await
            .expect("B names its User");
        conn.commit().await.expect("B commits");

        let mut conn = fixture.tenant_conn().await.expect("A reopens");
        assert_eq!(
            display_name_of(&mut conn, social).await,
            Some(Some("Ada Lovelace".to_owned())),
            "B's update reaches no A row"
        );
        let names: Vec<Option<String>> =
            list_assignable_principals(&mut conn, PrincipalFilter::default(), 200)
                .await
                .expect("directory lists")
                .into_iter()
                .map(|row| row.display_name)
                .collect();
        assert!(
            !names.contains(&Some("Bea Only".to_owned())),
            "B's names stay in B: {names:?}"
        );
    }

    /// The Role filter, its status counts and each Role's member count share
    /// one holder rule: an account holding the Role at any scope, directly or
    /// through a Group, counts once; a memberless Group adds no one; counts
    /// span every page; and another tenant's holders of a same-named Role
    /// never count.
    ///
    /// # Panics
    /// Panics when a page, count, Role assignment or description differs.
    #[tokio::test]
    async fn role_holders_filter_count_and_member_count_alike() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_b = fixture
            .seed_additional_tenant(&format!("holders-b-{}", Uuid::now_v7()))
            .await
            .expect("second tenant seeds");
        let payments: PermissionScope = serde_json::from_value(serde_json::json!({
            "card": { "family": { "kind": "Prompt", "name": "payments" } }
        }))
        .expect("scope parses");
        let user = |id: Uuid| SubjectColumns {
            user_id: Some(id),
            service_account_id: None,
            group_id: None,
        };
        let role = Uuid::now_v7();
        let [direct, grouped, both, suspended, bystander] =
            std::array::from_fn::<_, 5, _>(|_| Uuid::now_v7());
        let service = Uuid::now_v7();
        let (group, empty_group) = (Uuid::now_v7(), Uuid::now_v7());

        let mut conn = fixture.tenant_conn().await.expect("A opens");
        insert_role(
            &mut conn,
            role,
            "holder-probe",
            &serde_json::json!([]),
            false,
        )
        .await
        .expect("role inserts");
        for (id, label) in [
            (direct, "direct"),
            (grouped, "grouped"),
            (both, "both"),
            (suspended, "suspended"),
            (bystander, "bystander"),
        ] {
            insert_user(
                &mut conn,
                id,
                Some(&format!("{label}@holders.test")),
                "oidc",
                None,
            )
            .await
            .expect("User inserts");
        }
        sqlx::query("UPDATE wyrd.auth_users SET status = 'suspended' WHERE id = $1")
            .bind(suspended)
            .execute(&mut **conn.transaction())
            .await
            .expect("User suspends");
        insert_service_account(
            &mut conn,
            service,
            "service",
            None,
            "holder-service",
            None,
            Attribution::direct(PrincipalId::new(Uuid::now_v7())),
        )
        .await
        .expect("Service inserts");
        insert_group(&mut conn, group, "holder-group")
            .await
            .expect("group inserts");
        insert_group(&mut conn, empty_group, "holder-empty")
            .await
            .expect("empty group inserts");
        for member in [grouped, both] {
            insert_group_membership(&mut conn, group, user(member))
                .await
                .expect("member joins");
        }
        let service_subject = SubjectColumns {
            user_id: None,
            service_account_id: Some(service),
            group_id: None,
        };
        let group_subject = |id: Uuid| SubjectColumns {
            user_id: None,
            service_account_id: None,
            group_id: Some(id),
        };
        for (subject, scope) in [
            (user(direct), &PermissionScope::All),
            (user(both), &payments),
            (user(suspended), &PermissionScope::All),
            (service_subject, &payments),
            (group_subject(group), &PermissionScope::All),
            (group_subject(empty_group), &PermissionScope::All),
        ] {
            insert_role_binding(
                &mut conn,
                Uuid::now_v7(),
                role,
                subject,
                scope,
                RoleSource::Direct,
            )
            .await
            .expect("binding inserts");
        }
        conn.commit().await.expect("A commits");

        let mut conn = fixture.tenant_conn_for(tenant_b).await.expect("B opens");
        let (b_role, b_user) = (Uuid::now_v7(), Uuid::now_v7());
        insert_role(
            &mut conn,
            b_role,
            "holder-probe",
            &serde_json::json!([]),
            false,
        )
        .await
        .expect("B role inserts");
        insert_user(&mut conn, b_user, Some("b@holders.test"), "oidc", None)
            .await
            .expect("B User inserts");
        insert_role_binding(
            &mut conn,
            Uuid::now_v7(),
            b_role,
            user(b_user),
            &PermissionScope::All,
            RoleSource::Direct,
        )
        .await
        .expect("B binding inserts");
        conn.commit().await.expect("B commits");

        let mut conn = fixture.tenant_conn().await.expect("A reopens");
        let holders = PrincipalFilter {
            role: Some("holder-probe"),
            ..PrincipalFilter::default()
        };
        let mut seen = Vec::new();
        let mut after = None;
        loop {
            let page =
                list_assignable_principals(&mut conn, PrincipalFilter { after, ..holders }, 2)
                    .await
                    .expect("page lists");
            let counts =
                count_assignable_principals(&mut conn, PrincipalFilter { after, ..holders })
                    .await
                    .expect("counts read");
            assert_eq!(
                (counts.active, counts.suspended),
                (4, 1),
                "counts span every page"
            );
            let Some(last) = page.last().map(|row| row.id) else {
                break;
            };
            seen.extend(page);
            after = Some(last);
        }
        let mut ids: Vec<Uuid> = seen.iter().map(|row| row.id).collect();
        ids.sort();
        let mut expected = vec![direct, grouped, both, suspended, service];
        expected.sort();
        assert_eq!(
            ids, expected,
            "each holder once; the bystander and B absent"
        );
        let roles_of = |id: Uuid| {
            seen.iter()
                .find(|row| row.id == id)
                .map(|row| row.roles.clone())
                .expect("holder listed")
        };
        assert_eq!(
            roles_of(both),
            serde_json::json!([
                { "role": "holder-probe", "scope": "all" },
                { "role": "holder-probe", "scope": payments },
            ]),
            "a direct and a Group grant are both listed with their scopes"
        );
        assert_eq!(
            roles_of(grouped),
            serde_json::json!([{ "role": "holder-probe", "scope": "all" }])
        );

        let services = count_assignable_principals(
            &mut conn,
            PrincipalFilter {
                kind: Some("service"),
                ..holders
            },
        )
        .await
        .expect("counts read");
        assert_eq!((services.active, services.suspended), (1, 0));
        let nobody = PrincipalFilter {
            role: Some("no-such-role"),
            ..PrincipalFilter::default()
        };
        assert!(
            list_assignable_principals(&mut conn, nobody, 10)
                .await
                .expect("page lists")
                .is_empty()
        );
        let zero = count_assignable_principals(&mut conn, nobody)
            .await
            .expect("counts read");
        assert_eq!((zero.active, zero.suspended), (0, 0));

        let probe = |rows: Vec<wyrd_sql::queries::auth::RoleListRow>| {
            rows.into_iter()
                .find(|row| row.name == "holder-probe")
                .expect("probe role listed")
        };
        let listed = probe(list_roles(&mut conn).await.expect("roles list"));
        assert_eq!(
            listed.member_count, 5,
            "member count equals the filter's total"
        );
        assert_eq!(listed.description, None);
        set_role_description(&mut conn, role, Some("Reads payments"))
            .await
            .expect("description sets");
        let described = probe(list_roles(&mut conn).await.expect("roles list"));
        assert_eq!(described.description.as_deref(), Some("Reads payments"));
        set_role_description(&mut conn, role, None)
            .await
            .expect("description clears");
        let cleared = probe(list_roles(&mut conn).await.expect("roles list"));
        assert_eq!(cleared.description, None);
        conn.commit().await.expect("A commits");

        let mut conn = fixture.tenant_conn_for(tenant_b).await.expect("B reopens");
        let b_listed = probe(list_roles(&mut conn).await.expect("B roles list"));
        assert_eq!(b_listed.member_count, 1, "B counts only its own holder");
    }
}
