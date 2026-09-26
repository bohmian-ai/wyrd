mod pg_tests {
    //! Live Postgres migration integration test.
    //!
    //! Skipped automatically when env vars are unset so the default test suite
    //! remains credential-free. Run with:
    //!   WYRD_DATABASE_URL=postgres://wyrd_app:<pw>@localhost/wyrd \
    //!   WYRD_DATABASE_MIGRATOR_PASSWORD=<migrator_pw> \
    //!   cargo test -p wyrd-sql --all-features --test migration_pg

    use sqlx::PgPool;
    use sqlx::types::Uuid;
    use std::time::Duration;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::storage::{StorageBackendKind, UploadId, WireProtocol};
    use wyrd_sql::pool::build_app_pool;
    use wyrd_sql::queries::auth::{
        TrustedIssuerWrite, WorkloadBindingWrite, delete_trusted_issuer, delete_workload_binding,
        delete_workload_bindings_for_issuer, insert_user, trusted_issuer_by_url,
        trusted_issuers_for_tenant, upsert_user_identity, user_by_email, user_by_id,
        user_id_by_identity, workload_binding_by_key, workload_binding_by_subject,
    };
    // `insert_trusted_issuer`/`insert_workload_binding` are referenced by full path
    // in `cloud_issuer_crud_write_path_conflict_and_cascade` because this test module
    // already defines local helpers of the same name with different signatures.
    use wyrd_sql::queries::auth::insert_trusted_issuer as insert_trusted_issuer_query;
    use wyrd_sql::queries::auth::insert_workload_binding as insert_workload_binding_query;
    use wyrd_sql::queries::storage;
    use wyrd_sql::{SqlError, SqlStore, TenantConn};

    const SYSTEM_TENANT_MIGRATION_VERSION: i64 = 20260601000015;

    /// The system-owner seed is exact, repeatable, and rejects ambiguous ownership.
    #[tokio::test]
    async fn system_owner_seed_migration_is_strict_and_idempotent() {
        let Some(_) = database_url() else {
            return;
        };
        let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("fixture starts with fresh migration");
        let pool = fixture.superuser_pool().await.expect("migrator pool");
        let system_id = DataTenantId::SYSTEM_OWNER.as_uuid();

        let exact: (
            String,
            String,
            String,
            Option<chrono::DateTime<chrono::Utc>>,
        ) = sqlx::query_as(
            "SELECT slug, display_name, status, deleted_at
                   FROM platform.tenants
                  WHERE data_tenant_id = $1",
        )
        .bind(system_id)
        .fetch_one(&pool)
        .await
        .expect("fresh migration seeds system tenant");
        assert_eq!(
            exact,
            (
                "wyrd-system".to_owned(),
                "Wyrd System".to_owned(),
                "active".to_owned(),
                None,
            )
        );

        wyrd_sql::migrate(&pool)
            .await
            .expect("repeat migration is idempotent");
        sqlx::query("DELETE FROM wyrd._sqlx_migrations WHERE version = $1")
            .bind(SYSTEM_TENANT_MIGRATION_VERSION)
            .execute(&pool)
            .await
            .expect("remove migration ledger for compatible-state proof");
        wyrd_sql::migrate(&pool)
            .await
            .expect("exact compatible preexistence is accepted");

        sqlx::query(
            "UPDATE platform.tenants
                SET display_name = 'Impostor System'
              WHERE data_tenant_id = $1",
        )
        .bind(system_id)
        .execute(&pool)
        .await
        .expect("stage incompatible sentinel");
        sqlx::query("DELETE FROM wyrd._sqlx_migrations WHERE version = $1")
            .bind(SYSTEM_TENANT_MIGRATION_VERSION)
            .execute(&pool)
            .await
            .expect("remove migration ledger for conflict proof");
        assert!(
            wyrd_sql::migrate(&pool).await.is_err(),
            "incompatible system tenant attributes must fail"
        );
        let display_name: (String,) =
            sqlx::query_as("SELECT display_name FROM platform.tenants WHERE data_tenant_id = $1")
                .bind(system_id)
                .fetch_one(&pool)
                .await
                .expect("conflicting sentinel remains");
        assert_eq!(display_name.0, "Impostor System");

        sqlx::query(
            "UPDATE platform.tenants
                SET display_name = 'Wyrd System'
              WHERE data_tenant_id = $1",
        )
        .bind(system_id)
        .execute(&pool)
        .await
        .expect("restore exact sentinel");
        wyrd_sql::migrate(&pool)
            .await
            .expect("restored exact sentinel migrates");

        sqlx::query("DELETE FROM platform.tenants WHERE data_tenant_id = $1")
            .bind(system_id)
            .execute(&pool)
            .await
            .expect("remove sentinel for slug conflict");
        let conflicting_tenant = DataTenantId::new_v7();
        sqlx::query(
            "INSERT INTO platform.tenants
                (data_tenant_id, slug, display_name, status)
             VALUES ($1, 'wyrd-system', 'Conflicting Tenant', 'active')",
        )
        .bind(conflicting_tenant.as_uuid())
        .execute(&pool)
        .await
        .expect("stage canonical slug conflict");
        sqlx::query("DELETE FROM wyrd._sqlx_migrations WHERE version = $1")
            .bind(SYSTEM_TENANT_MIGRATION_VERSION)
            .execute(&pool)
            .await
            .expect("remove migration ledger for slug conflict proof");
        assert!(
            wyrd_sql::migrate(&pool).await.is_err(),
            "canonical slug ownership by another tenant must fail"
        );
        let owner: (Uuid,) = sqlx::query_as(
            "SELECT data_tenant_id FROM platform.tenants WHERE slug = 'wyrd-system'",
        )
        .fetch_one(&pool)
        .await
        .expect("conflicting slug owner remains");
        assert_eq!(owner.0, conflicting_tenant.as_uuid());
    }

    #[tokio::test]
    /// The full migration set applies to an empty database and applying it
    /// again is a no-op, so a redeploy cannot half-apply schema.
    async fn migrations_apply_and_are_idempotent() {
        let Some(url) = database_url() else {
            return;
        };

        let store = SqlStore::connect(&url, 2)
            .await
            .expect("connects to postgres");
        assert_required_roles(store.pool()).await;

        store.migrate().await.expect("first migration run succeeds");
        store
            .migrate()
            .await
            .expect("second migration run is idempotent");

        let pool = store.pool();
        assert_regclass_exists(pool, "wyrd._sqlx_migrations", true).await;
        assert_regclass_exists(pool, "platform._sqlx_migrations", false).await;
        assert_regclass_exists(pool, "public._sqlx_migrations", false).await;
        assert_regclass_exists(pool, "platform.tenants", true).await;
        // The pre-principal platform identity model is retired; administrative
        // identity is principals, credentials, and grants.
        assert_regclass_exists(pool, "platform.users", false).await;
        assert_regclass_exists(pool, "platform.roles", false).await;
        assert_regclass_exists(pool, "platform.user_roles", false).await;
        assert_regclass_exists(pool, "platform.api_keys", false).await;
        assert_regclass_exists(pool, "platform.principals", true).await;
        assert_regclass_exists(pool, "platform.credentials", true).await;
        assert_regclass_exists(pool, "platform.principal_grants", true).await;
        // The greenfield baseline has one audit authority: the tenant
        // `vala.audit_staging` chain published into `vala.system.audit_log`.
        assert_regclass_exists(pool, "platform.audit_log", false).await;
        assert_regclass_exists(pool, "wyrd.auth_users", true).await;
        assert_regclass_exists(pool, "wyrd.auth_user_identities", true).await;
        assert_regclass_exists(pool, "wyrd.auth_login_state", true).await;
        assert_regclass_exists(pool, "wyrd.auth_refresh_tokens", true).await;
        assert_regclass_exists(pool, "wyrd.auth_trusted_issuers", true).await;
        assert_regclass_exists(pool, "wyrd.auth_workload_bindings", true).await;

        assert_platform_resolver_shape(pool).await;
        assert_current_tenant_parallel_restricted(pool).await;
        assert_rls_metadata(
            pool,
            "auth_%",
            &[
                "auth_api_keys",
                "auth_login_state",
                "auth_refresh_tokens",
                "auth_roles",
                "auth_service_account_roles",
                "auth_service_accounts",
                "auth_trusted_issuers",
                "auth_user_identities",
                "auth_user_roles",
                "auth_users",
                "auth_workload_bindings",
            ],
        )
        .await;
        assert_rls_metadata(
            pool,
            "gateway_batch%",
            &["gateway_batch_files", "gateway_batches"],
        )
        .await;
    }

    #[tokio::test]
    async fn refresh_token_hash_unique_constraint_exists() {
        let Some(url) = database_url() else {
            return;
        };

        let store = SqlStore::connect(&url, 2)
            .await
            .expect("connects to postgres");
        assert_required_roles(store.pool()).await;
        store.migrate().await.expect("migrations apply");

        let row: (bool,) = sqlx::query_as(
            "SELECT EXISTS (
            SELECT 1 FROM pg_indexes
            WHERE schemaname = 'wyrd'
              AND tablename = 'auth_refresh_tokens'
              AND indexname = 'auth_refresh_tokens_token_hash'
              AND indexdef LIKE '%UNIQUE%'
              AND indexdef LIKE '%data_tenant_id%'
              AND indexdef LIKE '%token_hash%'
        )",
        )
        .fetch_one(store.pool())
        .await
        .expect("index query succeeds");

        assert!(
            row.0,
            "auth_refresh_tokens_token_hash unique index must exist on (data_tenant_id, token_hash)"
        );
    }

    #[tokio::test]
    async fn duplicate_refresh_token_hash_rejected_within_tenant_only() {
        let Some(url) = database_url() else {
            return;
        };

        let store = SqlStore::connect(&url, 2)
            .await
            .expect("connects to postgres");
        assert_required_roles(store.pool()).await;
        store.migrate().await.expect("migrations apply");

        let pool = store.pool();
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let suffix = tenant_a.to_string();
        let user_a = Uuid::now_v7();
        let user_b = Uuid::now_v7();
        let token_a = Uuid::now_v7();
        let token_b = Uuid::now_v7();
        let token_c = Uuid::now_v7();
        let token_hash = format!("hash-collision-{suffix}");

        insert_tenant(pool, tenant_a, &format!("test-a-{suffix}")).await;
        insert_tenant(pool, tenant_b, &format!("test-b-{suffix}")).await;
        insert_auth_user(pool, tenant_a, user_a, "a").await;
        insert_auth_user(pool, tenant_b, user_b, "b").await;

        insert_refresh_token(pool, tenant_a, token_a, user_a, &token_hash)
            .await
            .expect("first same-tenant token inserts");

        let duplicate_same_tenant =
            insert_refresh_token(pool, tenant_a, token_b, user_a, &token_hash).await;
        assert!(
            duplicate_same_tenant.is_err(),
            "duplicate token_hash in the same tenant must be rejected"
        );

        insert_refresh_token(pool, tenant_b, token_c, user_b, &token_hash)
            .await
            .expect("same token_hash under a different tenant inserts");

        cleanup_refresh_token_test_rows(pool, tenant_a, tenant_b)
            .await
            .expect("test rows clean up");
    }

    #[tokio::test]
    async fn nullable_oidc_user_email_roundtrips() {
        let Some(migrator_url) = database_url() else {
            return;
        };
        let Some(app_url) = app_database_url() else {
            return;
        };

        let store = SqlStore::connect(&migrator_url, 2)
            .await
            .expect("migrator connects");
        assert_required_roles(store.pool()).await;
        store.migrate().await.expect("migrations apply");
        let app_pool = build_app_pool(&app_url).await.expect("app pool connects");

        let tenant = DataTenantId::new_v7();
        let suffix = tenant.as_uuid().to_string();
        let user_id = Uuid::now_v7();
        insert_tenant(store.pool(), tenant, &format!("oidc-user-{suffix}")).await;

        let mut conn = TenantConn::acquire(&app_pool, tenant)
            .await
            .expect("tenant conn acquired");
        insert_user(&mut conn, user_id, None, "oidc", None)
            .await
            .expect("null email user inserts");
        conn.commit().await.expect("insert commits");

        let mut conn = TenantConn::acquire(&app_pool, tenant)
            .await
            .expect("tenant conn acquired");
        let row = user_by_id(&mut conn, user_id)
            .await
            .expect("user lookup succeeds")
            .expect("user exists");
        assert!(row.email.is_none());

        let mut conn = TenantConn::acquire(&app_pool, tenant)
            .await
            .expect("tenant conn acquired");
        sqlx::query(
            "UPDATE wyrd.auth_users
            SET email = $1
          WHERE data_tenant_id = wyrd.current_tenant()
            AND id = $2",
        )
        .bind("present@example.com")
        .bind(user_id)
        .execute(&mut **conn.transaction())
        .await
        .expect("user email updates");
        conn.commit().await.expect("update commits");

        let mut conn = TenantConn::acquire(&app_pool, tenant)
            .await
            .expect("tenant conn acquired");
        let row = user_by_email(&mut conn, "present@example.com")
            .await
            .expect("email lookup succeeds")
            .expect("row exists");
        assert_eq!(row.id, user_id);
        assert_eq!(row.email.as_deref(), Some("present@example.com"));

        cleanup_auth_test_rows(store.pool(), &[tenant])
            .await
            .expect("test rows clean up");
    }

    #[tokio::test]
    async fn federated_identity_roundtrips_and_rejects_cross_tenant_user_id() {
        let Some(migrator_url) = database_url() else {
            return;
        };
        let Some(app_url) = app_database_url() else {
            return;
        };

        let store = SqlStore::connect(&migrator_url, 2)
            .await
            .expect("migrator connects");
        assert_required_roles(store.pool()).await;
        store.migrate().await.expect("migrations apply");
        let app_pool = build_app_pool(&app_url).await.expect("app pool connects");

        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let suffix = tenant_a.as_uuid().to_string();
        let user_a = Uuid::now_v7();
        let user_b = Uuid::now_v7();
        let issuer = "https://idp.example.com/realms/acme";

        insert_tenant(store.pool(), tenant_a, &format!("id-a-{suffix}")).await;
        insert_tenant(store.pool(), tenant_b, &format!("id-b-{suffix}")).await;

        let mut conn = TenantConn::acquire(&app_pool, tenant_a)
            .await
            .expect("tenant A conn acquired");
        insert_user(
            &mut conn,
            user_a,
            Some("tenant-a@example.com"),
            "oidc",
            None,
        )
        .await
        .expect("tenant A user inserts");
        upsert_user_identity(&mut conn, issuer, "sub-a", user_a)
            .await
            .expect("identity upserts");
        conn.commit().await.expect("tenant A commit succeeds");

        let mut conn = TenantConn::acquire(&app_pool, tenant_a)
            .await
            .expect("tenant A conn acquired");
        let looked_up = user_id_by_identity(&mut conn, issuer, "sub-a")
            .await
            .expect("identity lookup succeeds");
        assert_eq!(looked_up, Some(user_a));

        let mut conn = TenantConn::acquire(&app_pool, tenant_b)
            .await
            .expect("tenant B conn acquired");
        insert_user(
            &mut conn,
            user_b,
            Some("tenant-b@example.com"),
            "oidc",
            None,
        )
        .await
        .expect("tenant B user inserts");
        conn.commit().await.expect("tenant B commit succeeds");

        let mut conn = TenantConn::acquire(&app_pool, tenant_a)
            .await
            .expect("tenant A conn acquired");
        let error = upsert_user_identity(&mut conn, issuer, "sub-foreign", user_b)
            .await
            .expect_err("cross-tenant user id should fail");
        assert!(matches!(error, sqlx::Error::Database(_)));

        cleanup_auth_test_rows(store.pool(), &[tenant_a, tenant_b])
            .await
            .expect("test rows clean up");
    }

    #[tokio::test]
    async fn resolve_tenant_by_slug_behavioral_contract() {
        let Some(url) = database_url() else {
            return;
        };

        let store = SqlStore::connect(&url, 2)
            .await
            .expect("connects to postgres");
        assert_required_roles(store.pool()).await;
        store.migrate().await.expect("migrations apply");

        let pool = store.pool();
        let active_id = DataTenantId::new_v7();
        let suspended_id = DataTenantId::new_v7();
        let active_slug = format!("rslv-act-{}", active_id.as_uuid());
        let suspended_slug = format!("rslv-sus-{}", suspended_id.as_uuid());

        sqlx::query(
            "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status)
         VALUES ($1, $2, 'Resolver Active', 'active')",
        )
        .bind(active_id.as_uuid())
        .bind(&active_slug)
        .execute(pool)
        .await
        .expect("active tenant inserts");

        sqlx::query(
            "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status)
         VALUES ($1, $2, 'Resolver Suspended', 'suspended')",
        )
        .bind(suspended_id.as_uuid())
        .bind(&suspended_slug)
        .execute(pool)
        .await
        .expect("suspended tenant inserts");

        let active: (Option<String>,) =
            sqlx::query_as("SELECT CAST(platform.resolve_tenant_by_slug($1) AS TEXT)")
                .bind(&active_slug)
                .fetch_one(pool)
                .await
                .expect("resolver query succeeds for active slug");
        assert_eq!(
            active.0.as_deref(),
            Some(active_id.as_uuid().to_string().as_str()),
            "active slug must resolve to its UUID"
        );

        let suspended: (Option<String>,) =
            sqlx::query_as("SELECT CAST(platform.resolve_tenant_by_slug($1) AS TEXT)")
                .bind(&suspended_slug)
                .fetch_one(pool)
                .await
                .expect("resolver query succeeds for suspended slug");
        assert_eq!(suspended.0, None, "suspended slug must resolve to NULL");

        let missing: (Option<String>,) =
            sqlx::query_as("SELECT CAST(platform.resolve_tenant_by_slug($1) AS TEXT)")
                .bind("no-such-slug-wyrd-test")
                .fetch_one(pool)
                .await
                .expect("resolver query succeeds for missing slug");
        assert_eq!(missing.0, None, "missing slug must resolve to NULL");

        sqlx::query("DELETE FROM platform.tenants WHERE data_tenant_id IN ($1, $2)")
            .bind(active_id.as_uuid())
            .bind(suspended_id.as_uuid())
            .execute(pool)
            .await
            .expect("resolver test tenant cleanup succeeds");
    }

    #[tokio::test]
    async fn cross_tenant_rls_filters_row_by_tenant() {
        let Some(migrator_url) = database_url() else {
            return;
        };
        let Some(app_url) = app_database_url() else {
            return;
        };

        let store = SqlStore::connect(&migrator_url, 2)
            .await
            .expect("migrator connects");
        assert_required_roles(store.pool()).await;
        store.migrate().await.expect("migrations apply");

        let app_pool = build_app_pool(&app_url).await.expect("app pool connects");

        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let suffix = tenant_a.as_uuid().to_string();
        let user_id = Uuid::now_v7();

        insert_tenant(store.pool(), tenant_a, &format!("rls-a-{suffix}")).await;
        insert_tenant(store.pool(), tenant_b, &format!("rls-b-{suffix}")).await;
        insert_auth_user(store.pool(), tenant_a, user_id, "rls").await;

        {
            let mut conn = TenantConn::acquire(&app_pool, tenant_b)
                .await
                .expect("tenant_b conn acquired");
            let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM wyrd.auth_users")
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("count query succeeds");
            assert_eq!(
                count.0, 0,
                "tenant_b must see zero rows seeded for tenant_a"
            );
        }

        {
            let mut conn = TenantConn::acquire(&app_pool, tenant_a)
                .await
                .expect("tenant_a conn acquired");
            let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM wyrd.auth_users")
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("count query succeeds");
            assert_eq!(count.0, 1, "tenant_a must see their own seeded row");
        }

        cleanup_refresh_token_test_rows(store.pool(), tenant_a, tenant_b)
            .await
            .expect("RLS test rows clean up");
    }

    #[tokio::test]
    async fn storage_queries_round_trip_under_tenant_conn() {
        let Some(migrator_url) = database_url() else {
            return;
        };
        let Some(app_url) = app_database_url() else {
            return;
        };

        let store = SqlStore::connect(&migrator_url, 2)
            .await
            .expect("migrator connects");
        assert_required_roles(store.pool()).await;
        store.migrate().await.expect("migrations apply");

        let app_pool = build_app_pool(&app_url).await.expect("app pool connects");
        let tenant = DataTenantId::new_v7();
        let slug = format!("storage-{}", tenant.as_uuid());
        insert_tenant(store.pool(), tenant, &slug).await;

        let upload_id = Uuid::now_v7();
        let card_uid = DataTenantId::new_v7().as_uuid().to_string();
        let sha = "A".repeat(43) + "=";
        let storage_path = format!("{tenant}/cards/{card_uid}/model.bin");

        {
            let mut conn = TenantConn::acquire(&app_pool, tenant)
                .await
                .expect("tenant conn acquired");

            storage::multipart_uploads::insert_initiating(
                &mut conn,
                storage::multipart_uploads::NewMultipartUpload {
                    id: upload_id,
                    card_uid: &card_uid,
                    relative_path: "model.bin",
                    storage_path: &storage_path,
                    backend: StorageBackendKind::S3,
                    wire_protocol: WireProtocol::S3MultipartV1,
                    expected_sha256: &sha,
                    expected_size_bytes: 5_242_880,
                    content_type: Some("application/octet-stream"),
                    part_count_planned: 1,
                    part_size_bytes: 5_242_880,
                    block_count_planned: None,
                    ttl_secs: 3600,
                },
            )
            .await
            .expect("initiating upload inserts");

            storage::multipart_uploads::mark_pending(
                &mut conn,
                upload_id,
                Some("backend-upload-id"),
            )
            .await
            .expect("upload marks pending");

            let pending = storage::multipart_uploads::find_pending_for_dedupe(
                &mut conn,
                &card_uid,
                "model.bin",
                &sha,
            )
            .await
            .expect("pending lookup succeeds")
            .expect("pending row found");
            assert_eq!(pending.backend, StorageBackendKind::S3);
            assert_eq!(pending.wire_protocol, WireProtocol::S3MultipartV1);

            // Card activation may verify the object and complete the row
            // before the storage completion route persists its own result.
            storage::multipart_uploads::mark_completed_if_pending(&mut conn, upload_id)
                .await
                .expect("card activation completes the upload first");
            storage::multipart_uploads::mark_completed(&mut conn, upload_id)
                .await
                .expect("storage completion accepts the already completed upload");

            storage::artifact_metadata::insert(
                &mut conn,
                storage::artifact_metadata::NewArtifactMetadata {
                    storage_path: &storage_path,
                    card_uid: &card_uid,
                    size_bytes: 5_242_880,
                    sha256: &sha,
                    content_type: Some("application/octet-stream"),
                    sse_marker: Some("aws:kms"),
                    backend: StorageBackendKind::S3,
                },
            )
            .await
            .expect("artifact metadata inserts");

            let metadata = storage::artifact_metadata::get(&mut conn, &storage_path)
                .await
                .expect("artifact metadata query succeeds")
                .expect("artifact metadata row exists");
            assert_eq!(metadata.backend, StorageBackendKind::S3);
            assert_eq!(metadata.sha256, sha);

            storage::idempotency::store(
                &mut conn,
                "idem-key",
                &[7_u8; 32],
                200,
                &storage::idempotency::UploadInitReplaySeed {
                    upload_id: UploadId::new().to_string(),
                    storage_path: storage_path.clone(),
                    backend: StorageBackendKind::S3,
                    wire_protocol: WireProtocol::S3MultipartV1,
                },
                Duration::from_secs(60),
            )
            .await
            .expect("idempotency seed stores");

            let cached = storage::idempotency::get(&mut conn, "idem-key", &[7_u8; 32])
                .await
                .expect("idempotency seed fetch succeeds")
                .expect("idempotency seed exists");
            assert_eq!(cached.status, 200);
            assert_eq!(cached.seed.storage_path, storage_path);

            let conflict = storage::idempotency::get(&mut conn, "idem-key", &[8_u8; 32]).await;
            assert!(
                matches!(conflict, Err(wyrd_sql::SqlError::Conflict { .. })),
                "idempotency key reused with different body must return SqlError::Conflict, got {conflict:?}"
            );

            conn.commit().await.expect("storage transaction commits");
        }

        cleanup_storage_test_rows(store.pool(), &[tenant])
            .await
            .expect("storage test rows clean up");
    }

    #[tokio::test]
    async fn storage_rls_hides_artifact_metadata_across_tenants() {
        let Some(migrator_url) = database_url() else {
            return;
        };
        let Some(app_url) = app_database_url() else {
            return;
        };

        let store = SqlStore::connect(&migrator_url, 2)
            .await
            .expect("migrator connects");
        assert_required_roles(store.pool()).await;
        store.migrate().await.expect("migrations apply");

        let app_pool = build_app_pool(&app_url).await.expect("app pool connects");
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let suffix = tenant_a.as_uuid().to_string();
        insert_tenant(store.pool(), tenant_a, &format!("stor-a-{suffix}")).await;
        insert_tenant(store.pool(), tenant_b, &format!("stor-b-{suffix}")).await;

        let card_uid = DataTenantId::new_v7().as_uuid().to_string();
        let sha = "B".repeat(43) + "=";
        let storage_path = format!("{tenant_a}/cards/{card_uid}/x.bin");

        {
            let mut conn = TenantConn::acquire(&app_pool, tenant_a)
                .await
                .expect("tenant_a conn acquired");
            storage::artifact_metadata::insert(
                &mut conn,
                storage::artifact_metadata::NewArtifactMetadata {
                    storage_path: &storage_path,
                    card_uid: &card_uid,
                    size_bytes: 4,
                    sha256: &sha,
                    content_type: None,
                    sse_marker: None,
                    backend: StorageBackendKind::Local,
                },
            )
            .await
            .expect("tenant_a artifact metadata inserts");
            conn.commit().await.expect("tenant_a commit succeeds");
        }

        {
            let mut conn = TenantConn::acquire(&app_pool, tenant_b)
                .await
                .expect("tenant_b conn acquired");
            let hidden = storage::artifact_metadata::get(&mut conn, &storage_path)
                .await
                .expect("tenant_b artifact lookup succeeds");
            assert!(hidden.is_none(), "RLS must hide tenant_a storage rows");
        }

        cleanup_storage_test_rows(store.pool(), &[tenant_a, tenant_b])
            .await
            .expect("storage RLS test rows clean up");
    }

    #[tokio::test]
    async fn storage_admin_queries_find_and_abort_expired_uploads() {
        let Some(url) = database_url() else {
            return;
        };

        let store = SqlStore::connect(&url, 2)
            .await
            .expect("connects to postgres");
        assert_required_roles(store.pool()).await;
        store.migrate().await.expect("migrations apply");

        let tenant = DataTenantId::new_v7();
        let slug = format!("stor-admin-{}", tenant.as_uuid());
        insert_tenant(store.pool(), tenant, &slug).await;

        let upload_id = Uuid::now_v7();
        let card_uid = DataTenantId::new_v7().as_uuid().to_string();
        let sha = "C".repeat(43) + "=";
        let storage_path = format!("{tenant}/cards/{card_uid}/expired.bin");

        sqlx::query(
            "INSERT INTO wyrd.storage_multipart_uploads
             (id, data_tenant_id, card_uid, relative_path, storage_path,
              backend, wire_protocol, expected_sha256, expected_size_bytes,
              part_count_planned, part_size_bytes, status, expires_at)
         VALUES
             ($1, $2, $3, 'expired.bin', $4,
              's3', 's3_multipart_v1', $5, 1,
              1, 1, 'pending', now() - interval '1 hour')",
        )
        .bind(upload_id)
        .bind(tenant.as_uuid())
        .bind(&card_uid)
        .bind(&storage_path)
        .bind(&sha)
        .execute(store.pool())
        .await
        .expect("expired upload row inserts");

        let rows = storage::admin::multipart_uploads::expired_uploads_batch(
            store.pool(),
            10,
            std::time::Duration::from_secs(30),
        )
        .await
        .expect("expired upload query succeeds");
        assert!(
            rows.iter().any(|row| row.id == upload_id),
            "expired upload must be selected for sweeping"
        );

        let mut conn = TenantConn::acquire(store.pool(), tenant)
            .await
            .expect("tenant connection opens");
        storage::multipart_uploads::mark_aborted_if_open(&mut conn, upload_id, "test-sweeper")
            .await
            .expect("admin abort update succeeds");
        storage::multipart_uploads::mark_completed(&mut conn, upload_id)
            .await
            .expect_err("an aborted upload cannot be completed");
        conn.commit().await.expect("admin abort commits");

        cleanup_storage_test_rows(store.pool(), &[tenant])
            .await
            .expect("storage admin test rows clean up");
    }

    // ---------------------------------------------------------------------------
    // Cloud-issuer behavioral contract tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn cloud_issuer_unique_constraints_exist() {
        let Some(url) = database_url() else {
            return;
        };

        let store = SqlStore::connect(&url, 2)
            .await
            .expect("connects to postgres");
        assert_required_roles(store.pool()).await;
        store.migrate().await.expect("migrations apply");

        let pool = store.pool();

        let trusted_issuers_unique: (bool,) = sqlx::query_as(
            "SELECT EXISTS (
            SELECT 1 FROM pg_indexes
            WHERE schemaname = 'wyrd'
              AND tablename  = 'auth_trusted_issuers'
              AND indexdef LIKE '%UNIQUE%'
              AND indexdef LIKE '%data_tenant_id%'
              AND indexdef LIKE '%issuer_url%'
        )",
        )
        .fetch_one(pool)
        .await
        .expect("index query succeeds");

        assert!(
            trusted_issuers_unique.0,
            "auth_trusted_issuers must have a unique index on (data_tenant_id, issuer_url)"
        );

        let workload_bindings_unique: (bool,) = sqlx::query_as(
            "SELECT EXISTS (
            SELECT 1 FROM pg_indexes
            WHERE schemaname = 'wyrd'
              AND tablename  = 'auth_workload_bindings'
              AND indexdef LIKE '%UNIQUE%'
              AND indexdef LIKE '%data_tenant_id%'
              AND indexdef LIKE '%issuer_url%'
              AND indexdef LIKE '%subject%'
        )",
        )
        .fetch_one(pool)
        .await
        .expect("index query succeeds");

        assert!(
            workload_bindings_unique.0,
            "auth_workload_bindings must have a unique index on (data_tenant_id, issuer_url, subject)"
        );
    }

    #[tokio::test]
    async fn cloud_issuer_fk_rejects_delete_with_live_binding() {
        let Some(url) = database_url() else {
            return;
        };

        let store = SqlStore::connect(&url, 2)
            .await
            .expect("connects to postgres");
        assert_required_roles(store.pool()).await;
        store.migrate().await.expect("migrations apply");

        let pool = store.pool();
        let tenant = DataTenantId::new_v7();
        let suffix = tenant.as_uuid().to_string();
        let issuer_url = format!("https://idp.example.com/fk-test-{suffix}");

        insert_tenant(pool, tenant, &format!("fk-test-{suffix}")).await;
        insert_trusted_issuer(pool, tenant, &issuer_url).await;
        insert_workload_binding(
            pool,
            tenant,
            &issuer_url,
            "system:serviceaccount:default:wyrd",
            &serde_json::json!({
                "kind": "Service",
                "name": "billing",
                "version": "1.0.0",
                "space": "prod"
            }),
        )
        .await;

        // Attempt to delete the referenced issuer — must be rejected by the FK.
        let result = sqlx::query(
            "DELETE FROM wyrd.auth_trusted_issuers
          WHERE data_tenant_id = $1 AND issuer_url = $2",
        )
        .bind(tenant.as_uuid())
        .bind(&issuer_url)
        .execute(pool)
        .await;

        assert!(
            matches!(result, Err(sqlx::Error::Database(_))),
            "deleting an issuer with live workload bindings must fail with an FK violation: {result:?}"
        );

        // Cleanup: bindings first, then issuers, then tenants.
        sqlx::query("DELETE FROM wyrd.auth_workload_bindings WHERE data_tenant_id = $1")
            .bind(tenant.as_uuid())
            .execute(pool)
            .await
            .expect("binding cleanup succeeds");
        sqlx::query("DELETE FROM wyrd.auth_trusted_issuers WHERE data_tenant_id = $1")
            .bind(tenant.as_uuid())
            .execute(pool)
            .await
            .expect("issuer cleanup succeeds");
        sqlx::query("DELETE FROM platform.tenants WHERE data_tenant_id = $1")
            .bind(tenant.as_uuid())
            .execute(pool)
            .await
            .expect("tenant cleanup succeeds");
    }

    #[tokio::test]
    async fn cloud_issuer_card_ref_jsonb_round_trips() {
        let Some(url) = database_url() else {
            return;
        };

        let store = SqlStore::connect(&url, 2)
            .await
            .expect("connects to postgres");
        assert_required_roles(store.pool()).await;
        store.migrate().await.expect("migrations apply");

        let pool = store.pool();
        let tenant = DataTenantId::new_v7();
        let suffix = tenant.as_uuid().to_string();
        let issuer_url = format!("https://idp.example.com/card-ref-test-{suffix}");
        let subject = format!("system:sa:{suffix}");

        insert_tenant(pool, tenant, &format!("card-ref-{suffix}")).await;
        insert_trusted_issuer(pool, tenant, &issuer_url).await;

        // CardRef with all five fields (kind serializes to its wire name).
        let card_ref_json = serde_json::json!({
            "kind": "Service",
            "name": "billing-service",
            "version": "1.0.0",
            "space": "prod",
            "uid": "01890f28-7c4a-7000-98e7-4f4a3c2d1b01"
        });

        insert_workload_binding(pool, tenant, &issuer_url, &subject, &card_ref_json).await;

        let row: (serde_json::Value,) = sqlx::query_as(
            "SELECT card_ref FROM wyrd.auth_workload_bindings
          WHERE data_tenant_id = $1 AND issuer_url = $2 AND subject = $3",
        )
        .bind(tenant.as_uuid())
        .bind(&issuer_url)
        .bind(&subject)
        .fetch_one(pool)
        .await
        .expect("card_ref select succeeds");

        assert_eq!(
            row.0, card_ref_json,
            "card_ref JSONB must round-trip through Postgres without loss"
        );

        // Prove the stored JSON is valid for CardRef deserialization via serde.
        let card_ref: CardRef = serde_json::from_value(row.0)
            .expect("stored card_ref JSONB must deserialize to a valid CardRef");
        assert_eq!(card_ref.kind.wire_name(), "Service");
        assert_eq!(card_ref.name.to_string(), "billing-service");
        assert_eq!(card_ref.version.to_string(), "1.0.0");
        assert_eq!(
            card_ref.space.as_ref().map(ToString::to_string).as_deref(),
            Some("prod")
        );
        assert!(card_ref.uid.is_some(), "uid must survive the round-trip");

        // Cleanup.
        sqlx::query("DELETE FROM wyrd.auth_workload_bindings WHERE data_tenant_id = $1")
            .bind(tenant.as_uuid())
            .execute(pool)
            .await
            .expect("binding cleanup succeeds");
        sqlx::query("DELETE FROM wyrd.auth_trusted_issuers WHERE data_tenant_id = $1")
            .bind(tenant.as_uuid())
            .execute(pool)
            .await
            .expect("issuer cleanup succeeds");
        sqlx::query("DELETE FROM platform.tenants WHERE data_tenant_id = $1")
            .bind(tenant.as_uuid())
            .execute(pool)
            .await
            .expect("tenant cleanup succeeds");
    }

    #[tokio::test]
    async fn cloud_issuer_cross_tenant_rls_and_unique_constraints() {
        let Some(migrator_url) = database_url() else {
            return;
        };
        let Some(app_url) = app_database_url() else {
            return;
        };

        let store = SqlStore::connect(&migrator_url, 2)
            .await
            .expect("migrator connects");
        assert_required_roles(store.pool()).await;
        store.migrate().await.expect("migrations apply");

        let app_pool = build_app_pool(&app_url).await.expect("app pool connects");

        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let suffix = tenant_a.as_uuid().to_string();
        let issuer_url = format!("https://shared-idp.example.com/{suffix}");

        insert_tenant(store.pool(), tenant_a, &format!("ci-a-{suffix}")).await;
        insert_tenant(store.pool(), tenant_b, &format!("ci-b-{suffix}")).await;

        // Insert issuer for tenant A via migrator pool (bypasses RLS).
        insert_trusted_issuer(store.pool(), tenant_a, &issuer_url).await;

        // Tenant B must see zero rows — RLS filters out tenant A's issuers.
        {
            let mut conn = TenantConn::acquire(&app_pool, tenant_b)
                .await
                .expect("tenant_b conn acquired");
            let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM wyrd.auth_trusted_issuers")
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("count query succeeds");
            assert_eq!(
                count.0, 0,
                "tenant_b must see zero trusted issuers seeded for tenant_a"
            );
        }

        // Tenant A must see its own issuer.
        {
            let mut conn = TenantConn::acquire(&app_pool, tenant_a)
                .await
                .expect("tenant_a conn acquired");
            let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM wyrd.auth_trusted_issuers")
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("count query succeeds");
            assert_eq!(count.0, 1, "tenant_a must see its own trusted issuer");
        }

        // Same issuer_url is insertable under tenant B (cross-tenant isolation).
        insert_trusted_issuer(store.pool(), tenant_b, &issuer_url).await;

        // Duplicate (data_tenant_id, issuer_url) within tenant A must be rejected.
        let duplicate = insert_trusted_issuer_result(store.pool(), tenant_a, &issuer_url).await;
        assert!(
            duplicate.is_err(),
            "duplicate (data_tenant_id, issuer_url) within the same tenant must be rejected"
        );

        // Cleanup.
        sqlx::query("DELETE FROM wyrd.auth_trusted_issuers WHERE data_tenant_id IN ($1, $2)")
            .bind(tenant_a.as_uuid())
            .bind(tenant_b.as_uuid())
            .execute(store.pool())
            .await
            .expect("issuer cleanup succeeds");
        sqlx::query("DELETE FROM platform.tenants WHERE data_tenant_id IN ($1, $2)")
            .bind(tenant_a.as_uuid())
            .bind(tenant_b.as_uuid())
            .execute(store.pool())
            .await
            .expect("tenant cleanup succeeds");
    }

    #[tokio::test]
    async fn cloud_issuer_read_path_roundtrips_and_rls() {
        let Some(migrator_url) = database_url() else {
            return;
        };
        let Some(app_url) = app_database_url() else {
            return;
        };

        let store = SqlStore::connect(&migrator_url, 2)
            .await
            .expect("migrator connects");
        assert_required_roles(store.pool()).await;
        store.migrate().await.expect("migrations apply");

        let app_pool = build_app_pool(&app_url).await.expect("app pool connects");

        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let suffix = tenant_a.as_uuid().to_string();
        let issuer_url = format!("https://human-idp.example.com/{suffix}");
        let subject = format!("user-sub-{suffix}");

        // 12-byte nonce + 16-byte ciphertext (representative AES-GCM bytes).
        let secret_bytes: Vec<u8> = (0u8..28).collect();

        let card_ref_json = serde_json::json!({
            "kind": "Service",
            "name": "svc",
            "version": "1.0.0",
            "space": "prod"
        });

        insert_tenant(store.pool(), tenant_a, &format!("rp-a-{suffix}")).await;
        insert_tenant(store.pool(), tenant_b, &format!("rp-b-{suffix}")).await;

        // Insert a Human issuer with populated group_role_map and client_secret_enc.
        insert_human_issuer_with_secret(store.pool(), tenant_a, &issuer_url, &secret_bytes).await;
        insert_workload_binding(
            store.pool(),
            tenant_a,
            &issuer_url,
            &subject,
            &card_ref_json,
        )
        .await;

        // Read back under TenantConn(A) and assert every column round-trips.
        {
            let mut conn = TenantConn::acquire(&app_pool, tenant_a)
                .await
                .expect("tenant_a conn acquired");

            let issuers = trusted_issuers_for_tenant(&mut conn)
                .await
                .expect("issuer query succeeds");
            assert_eq!(issuers.len(), 1, "tenant_a must see exactly its own issuer");

            let row = &issuers[0];
            assert_eq!(row.issuer_url, issuer_url);
            assert_eq!(row.jwks_uri, format!("{issuer_url}/.well-known/jwks.json"));
            assert_eq!(row.expected_audience, "audience-test");
            assert_eq!(row.client_id, "client-id-test");
            assert_eq!(row.client_auth, "SecretBasic");
            assert_eq!(row.principal_kind, "Human");
            assert_eq!(row.jwks_ttl_secs, 300);
            assert_eq!(
                row.client_secret_enc.as_deref(),
                Some(secret_bytes.as_slice()),
                "client_secret_enc bytes must be identical — no decode or trim"
            );

            // group_role_map must survive round-trip (Human issuer column).
            let group_map = row
                .group_role_map
                .as_object()
                .expect("group_role_map is a JSON object");
            assert!(
                group_map.contains_key("admins"),
                "group_role_map must retain 'admins' key"
            );
            let admins = group_map["admins"].as_array().expect("admins is array");
            assert_eq!(admins.len(), 1);
            assert_eq!(admins[0], "admin-role");

            // default_roles round-trip.
            let default_roles = row
                .default_roles
                .as_array()
                .expect("default_roles is a JSON array");
            assert_eq!(default_roles.len(), 1);
            assert_eq!(default_roles[0], "default-role");

            // Binding lookup — NULL audience falls back to the NULL-audience row.
            let binding = workload_binding_by_subject(&mut conn, &issuer_url, &subject, None)
                .await
                .expect("binding query succeeds")
                .expect("binding must exist for tenant_a");
            assert_eq!(binding.issuer_url, issuer_url);
            assert_eq!(binding.subject, subject);
            assert!(binding.audience.is_none());
            let card_ref = &binding.card_ref;
            assert_eq!(card_ref.kind.wire_name(), "Service");
            assert_eq!(card_ref.name.to_string(), "svc");
            assert_eq!(card_ref.version.to_string(), "1.0.0");
            assert_eq!(
                card_ref.space.as_ref().map(ToString::to_string).as_deref(),
                Some("prod")
            );
        }

        // Seed the same issuer_url under tenant B.
        insert_trusted_issuer(store.pool(), tenant_b, &issuer_url).await;

        // Tenant A's read must still see exactly 1 issuer (its own) — not B's.
        {
            let mut conn = TenantConn::acquire(&app_pool, tenant_a)
                .await
                .expect("tenant_a conn acquired");
            let issuers = trusted_issuers_for_tenant(&mut conn)
                .await
                .expect("issuer query succeeds");
            assert_eq!(
                issuers.len(),
                1,
                "tenant_a must still see exactly 1 issuer after tenant_b's row is added"
            );
        }

        // Tenant B's binding lookup for tenant A's subject must return None (RLS).
        {
            let mut conn = TenantConn::acquire(&app_pool, tenant_b)
                .await
                .expect("tenant_b conn acquired");
            let binding = workload_binding_by_subject(&mut conn, &issuer_url, &subject, None)
                .await
                .expect("binding query succeeds");
            assert!(
                binding.is_none(),
                "tenant_b must not see tenant_a's workload binding"
            );
        }

        // Cleanup: bindings first (FK), then issuers, then tenants.
        sqlx::query("DELETE FROM wyrd.auth_workload_bindings WHERE data_tenant_id IN ($1, $2)")
            .bind(tenant_a.as_uuid())
            .bind(tenant_b.as_uuid())
            .execute(store.pool())
            .await
            .expect("binding cleanup succeeds");
        sqlx::query("DELETE FROM wyrd.auth_trusted_issuers WHERE data_tenant_id IN ($1, $2)")
            .bind(tenant_a.as_uuid())
            .bind(tenant_b.as_uuid())
            .execute(store.pool())
            .await
            .expect("issuer cleanup succeeds");
        sqlx::query("DELETE FROM platform.tenants WHERE data_tenant_id IN ($1, $2)")
            .bind(tenant_a.as_uuid())
            .bind(tenant_b.as_uuid())
            .execute(store.pool())
            .await
            .expect("tenant cleanup succeeds");
    }

    #[tokio::test]
    async fn cloud_issuer_crud_write_path_conflict_and_cascade() {
        let Some(migrator_url) = database_url() else {
            return;
        };
        let Some(app_url) = app_database_url() else {
            return;
        };

        let store = SqlStore::connect(&migrator_url, 2)
            .await
            .expect("migrator connects");
        store.migrate().await.expect("migrations apply");
        let app_pool = build_app_pool(&app_url).await.expect("app pool connects");

        let tenant = DataTenantId::new_v7();
        let suffix = tenant.as_uuid().to_string();
        let issuer_url = format!("https://crud-idp.example.com/{suffix}");
        let subject = format!("svc-sub-{suffix}");
        // Representative AES-GCM bytes (12-byte nonce + 16-byte ciphertext).
        let secret_bytes: Vec<u8> = (40u8..68).collect();

        insert_tenant(store.pool(), tenant, &format!("crud-{suffix}")).await;

        let issuer_write = TrustedIssuerWrite {
            issuer_url: issuer_url.clone(),
            jwks_uri: format!("{issuer_url}/jwks"),
            expected_audience: "wyrd-api".to_owned(),
            client_id: "wyrd-client".to_owned(),
            client_auth: "SecretBasic".to_owned(),
            claim_mapping: serde_json::json!({ "subject": "sub" }),
            group_role_map: serde_json::json!({}),
            default_roles: serde_json::json!([]),
            principal_kind: "Workload".to_owned(),
            jwks_ttl_secs: 3600,
            client_secret_enc: Some(secret_bytes.clone()),
        };
        let binding_write = WorkloadBindingWrite {
            issuer_url: issuer_url.clone(),
            subject: subject.clone(),
            audience: None,
            card_ref: serde_json::json!({
                "kind": "Service",
                "name": "svc",
                "version": "1.0.0",
                "space": "prod"
            }),
        };

        // Insert persists, and the encrypted-secret bytes round-trip verbatim.
        {
            let mut conn = TenantConn::acquire(&app_pool, tenant)
                .await
                .expect("conn acquired");
            insert_trusted_issuer_query(&mut conn, &issuer_write)
                .await
                .expect("issuer inserts");
            conn.commit().await.expect("insert commits");

            let mut conn = TenantConn::acquire(&app_pool, tenant)
                .await
                .expect("conn acquired");
            let row = trusted_issuer_by_url(&mut conn, &issuer_url)
                .await
                .expect("get query succeeds")
                .expect("issuer exists");
            assert_eq!(
                row.client_secret_enc.as_deref(),
                Some(secret_bytes.as_slice())
            );
            assert_eq!(row.client_auth, "SecretBasic");
        }

        // A duplicate insert raises a unique violation, not a silent upsert.
        {
            let mut conn = TenantConn::acquire(&app_pool, tenant)
                .await
                .expect("conn acquired");
            let error = insert_trusted_issuer_query(&mut conn, &issuer_write)
                .await
                .expect_err("duplicate issuer must conflict");
            assert!(matches!(
                SqlError::from(error),
                SqlError::UniqueViolation { .. }
            ));
        }

        // Insert a binding; a duplicate binding also conflicts.
        {
            let mut conn = TenantConn::acquire(&app_pool, tenant)
                .await
                .expect("conn acquired");
            insert_workload_binding_query(&mut conn, &binding_write)
                .await
                .expect("binding inserts");
            conn.commit().await.expect("binding commits");

            let mut conn = TenantConn::acquire(&app_pool, tenant)
                .await
                .expect("conn acquired");
            let error = insert_workload_binding_query(&mut conn, &binding_write)
                .await
                .expect_err("duplicate binding must conflict");
            assert!(matches!(
                SqlError::from(error),
                SqlError::UniqueViolation { .. }
            ));
        }

        // Deleting the issuer while a binding references it fails closed (FK restrict).
        {
            let mut conn = TenantConn::acquire(&app_pool, tenant)
                .await
                .expect("conn acquired");
            let error = delete_trusted_issuer(&mut conn, &issuer_url)
                .await
                .expect_err("issuer delete blocked by live binding");
            assert!(matches!(
                SqlError::from(error),
                SqlError::FkViolation { .. }
            ));
        }

        // Deleting a non-existent binding removes zero rows.
        {
            let mut conn = TenantConn::acquire(&app_pool, tenant)
                .await
                .expect("conn acquired");
            let removed = delete_workload_binding(&mut conn, &issuer_url, "no-such-subject")
                .await
                .expect("delete query succeeds");
            assert_eq!(removed, 0);
            conn.commit().await.expect("commit");
        }

        // Cascade: remove the bindings, then the issuer; both are gone.
        {
            let mut conn = TenantConn::acquire(&app_pool, tenant)
                .await
                .expect("conn acquired");
            let bindings_removed = delete_workload_bindings_for_issuer(&mut conn, &issuer_url)
                .await
                .expect("cascade binding delete succeeds");
            assert_eq!(bindings_removed, 1);
            let issuer_removed = delete_trusted_issuer(&mut conn, &issuer_url)
                .await
                .expect("issuer delete succeeds");
            assert_eq!(issuer_removed, 1);
            conn.commit().await.expect("cascade commits");

            let mut conn = TenantConn::acquire(&app_pool, tenant)
                .await
                .expect("conn acquired");
            assert!(
                trusted_issuer_by_url(&mut conn, &issuer_url)
                    .await
                    .expect("get succeeds")
                    .is_none()
            );
            assert!(
                workload_binding_by_key(&mut conn, &issuer_url, &subject)
                    .await
                    .expect("get succeeds")
                    .is_none()
            );
        }

        sqlx::query("DELETE FROM platform.tenants WHERE data_tenant_id = $1")
            .bind(tenant.as_uuid())
            .execute(store.pool())
            .await
            .expect("tenant cleanup succeeds");
    }

    async fn insert_human_issuer_with_secret(
        pool: &PgPool,
        data_tenant_id: DataTenantId,
        issuer_url: &str,
        client_secret_enc: &[u8],
    ) {
        sqlx::query(
            "INSERT INTO wyrd.auth_trusted_issuers
             (data_tenant_id, issuer_url, jwks_uri, expected_audience, client_id,
              client_auth, claim_mapping, group_role_map, default_roles,
              principal_kind, jwks_ttl_secs, client_secret_enc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(data_tenant_id.as_uuid())
        .bind(issuer_url)
        .bind(format!("{issuer_url}/.well-known/jwks.json"))
        .bind("audience-test")
        .bind("client-id-test")
        .bind("SecretBasic")
        .bind(serde_json::json!({"subject": "sub", "email": "email", "groups": "groups"}))
        .bind(serde_json::json!({"admins": ["admin-role"], "viewers": ["viewer-role"]}))
        .bind(serde_json::json!(["default-role"]))
        .bind("Human")
        .bind(300_i64)
        .bind(client_secret_enc)
        .execute(pool)
        .await
        .expect("human issuer with secret inserts");
    }

    async fn insert_trusted_issuer(pool: &PgPool, data_tenant_id: DataTenantId, issuer_url: &str) {
        insert_trusted_issuer_result(pool, data_tenant_id, issuer_url)
            .await
            .expect("trusted issuer inserts");
    }

    async fn insert_trusted_issuer_result(
        pool: &PgPool,
        data_tenant_id: DataTenantId,
        issuer_url: &str,
    ) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
        sqlx::query(
            "INSERT INTO wyrd.auth_trusted_issuers
             (data_tenant_id, issuer_url, jwks_uri, expected_audience, client_id,
              client_auth, claim_mapping, group_role_map, default_roles,
              principal_kind, jwks_ttl_secs)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(data_tenant_id.as_uuid())
        .bind(issuer_url)
        .bind(format!("{issuer_url}/.well-known/jwks.json"))
        .bind("wyrd-test-audience")
        .bind("wyrd-test-client")
        .bind("PrivateKeyJwt")
        .bind(serde_json::json!({"subject": "sub"}))
        .bind(serde_json::json!({}))
        .bind(serde_json::json!([]))
        .bind("Workload")
        .bind(3600_i64)
        .execute(pool)
        .await
    }

    async fn insert_workload_binding(
        pool: &PgPool,
        data_tenant_id: DataTenantId,
        issuer_url: &str,
        subject: &str,
        card_ref: &serde_json::Value,
    ) {
        sqlx::query(
            "INSERT INTO wyrd.auth_workload_bindings
             (data_tenant_id, issuer_url, subject, card_ref)
         VALUES ($1, $2, $3, $4)",
        )
        .bind(data_tenant_id.as_uuid())
        .bind(issuer_url)
        .bind(subject)
        .bind(card_ref)
        .execute(pool)
        .await
        .expect("workload binding inserts");
    }

    fn database_url() -> Option<String> {
        let app_url = std::env::var("WYRD_DATABASE_URL").ok()?;
        let migrator_password = std::env::var("WYRD_DATABASE_MIGRATOR_PASSWORD").ok()?;
        let mut url = url::Url::parse(&app_url).ok()?;
        url.set_username("wyrd_migrator").ok()?;
        url.set_password(Some(&migrator_password)).ok()?;
        Some(url.into())
    }

    fn app_database_url() -> Option<String> {
        std::env::var("WYRD_DATABASE_URL").ok()
    }

    async fn assert_required_roles(pool: &PgPool) {
        let rows: Vec<(String, bool)> = sqlx::query_as(
            "SELECT rolname, rolbypassrls
         FROM pg_roles
         WHERE rolname IN ('wyrd_migrator', 'wyrd_app', 'wyrd_platform_admin')
         ORDER BY rolname",
        )
        .fetch_all(pool)
        .await
        .expect("role metadata query succeeds");

        assert_eq!(
            rows,
            vec![
                ("wyrd_app".to_owned(), false),
                ("wyrd_migrator".to_owned(), true),
                ("wyrd_platform_admin".to_owned(), true),
            ],
            "Wyrd Postgres roles must exist with locked BYPASSRLS bits before migrations run"
        );
    }

    async fn assert_regclass_exists(pool: &PgPool, name: &str, expected: bool) {
        let row: (bool,) = sqlx::query_as("SELECT to_regclass($1) IS NOT NULL")
            .bind(name)
            .fetch_one(pool)
            .await
            .expect("to_regclass query succeeds");

        assert_eq!(row.0, expected, "unexpected existence for {name}");
    }

    async fn assert_current_tenant_parallel_restricted(pool: &PgPool) {
        let row: (bool,) = sqlx::query_as(
            "SELECT p.proparallel = 'r'
         FROM pg_proc p
         JOIN pg_namespace n ON n.oid = p.pronamespace
         WHERE n.nspname = 'wyrd'
           AND p.proname = 'current_tenant'",
        )
        .fetch_one(pool)
        .await
        .expect("current_tenant function metadata query succeeds");

        assert!(
            row.0,
            "wyrd.current_tenant() must be PARALLEL RESTRICTED — it reads a transaction-local GUC \
         that parallel workers do not inherit"
        );
    }

    async fn assert_platform_resolver_shape(pool: &PgPool) {
        let row: (bool, bool, bool) = sqlx::query_as(
            "SELECT
             p.prosecdef,
             p.provolatile = 's',
             COALESCE(p.proconfig, ARRAY[]::text[]) @> ARRAY['search_path=pg_catalog, platform']
         FROM pg_proc p
         JOIN pg_namespace n ON n.oid = p.pronamespace
         WHERE n.nspname = 'platform'
           AND p.proname = 'resolve_tenant_by_slug'
           AND pg_get_function_arguments(p.oid) = 'p_slug text'",
        )
        .fetch_one(pool)
        .await
        .expect("platform resolver metadata query succeeds");

        assert!(
            row.0,
            "platform.resolve_tenant_by_slug must be SECURITY DEFINER"
        );
        assert!(row.1, "platform.resolve_tenant_by_slug must be STABLE");
        assert!(
            row.2,
            "platform.resolve_tenant_by_slug must pin search_path to pg_catalog, platform"
        );

        // PUBLIC is a pseudo-role and cannot be passed to has_function_privilege
        // (it errors with `role "PUBLIC" does not exist`). Inspect pg_proc.proacl
        // directly: aclexplode emits grantee = 0 for the PUBLIC grant entry.
        let privileges: (bool, bool, bool) = sqlx::query_as(
        "SELECT
             EXISTS (
                 SELECT 1
                 FROM pg_proc p
                 JOIN pg_namespace n ON n.oid = p.pronamespace
                 CROSS JOIN LATERAL aclexplode(p.proacl) a
                 WHERE n.nspname = 'platform'
                   AND p.proname = 'resolve_tenant_by_slug'
                   AND a.grantee = 0
                   AND a.privilege_type = 'EXECUTE'
             ),
             has_function_privilege('wyrd_app', 'platform.resolve_tenant_by_slug(text)', 'EXECUTE'),
             has_function_privilege('wyrd_platform_admin', 'platform.resolve_tenant_by_slug(text)', 'EXECUTE')",
    )
    .fetch_one(pool)
    .await
    .expect("platform resolver privilege query succeeds");

        assert!(!privileges.0, "PUBLIC must not execute the tenant resolver");
        assert!(privileges.1, "wyrd_app must execute the tenant resolver");
        assert!(
            privileges.2,
            "wyrd_platform_admin must execute the tenant resolver"
        );
    }

    /// Asserts that the `wyrd` tables whose names match the SQL `like` pattern
    /// are exactly `expected_tables`, each enabling and forcing row-level
    /// security under the canonical `tenant_isolation` policy.
    ///
    /// # Panics
    ///
    /// Panics when the catalog query fails, the matching tables differ, or a
    /// table lacks forced RLS or the policy.
    async fn assert_rls_metadata(pool: &PgPool, like: &str, expected_tables: &[&str]) {
        let rows: Vec<(String, bool, bool, bool)> = sqlx::query_as(
            "SELECT
             c.relname,
             c.relrowsecurity,
             c.relforcerowsecurity,
             EXISTS (
                 SELECT 1
                 FROM pg_policies p
                 WHERE p.schemaname = 'wyrd'
                   AND p.tablename = c.relname
                   AND p.policyname = 'tenant_isolation'
                   AND p.qual = '(data_tenant_id = wyrd.current_tenant())'
                   AND p.with_check = '(data_tenant_id = wyrd.current_tenant())'
             )
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'wyrd'
           AND c.relkind = 'r'
           AND c.relname LIKE $1
         ORDER BY c.relname",
        )
        .bind(like)
        .fetch_all(pool)
        .await
        .expect("RLS metadata query succeeds");

        assert_eq!(
            rows.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
            expected_tables
        );

        for (table, rls_enabled, rls_forced, policy_exists) in rows {
            assert!(rls_enabled, "{table} must enable row-level security");
            assert!(rls_forced, "{table} must force row-level security");
            assert!(
                policy_exists,
                "{table} must have the tenant_isolation RLS policy"
            );
        }
    }

    async fn insert_tenant(pool: &PgPool, data_tenant_id: DataTenantId, slug: &str) {
        sqlx::query(
            "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status)
         VALUES ($1, $2, $3, 'active')",
        )
        .bind(data_tenant_id.as_uuid())
        .bind(slug)
        .bind(slug)
        .execute(pool)
        .await
        .expect("tenant inserts");
    }

    async fn insert_auth_user(
        pool: &PgPool,
        data_tenant_id: DataTenantId,
        id: Uuid,
        email_tag: &str,
    ) {
        sqlx::query(
            "INSERT INTO wyrd.auth_users (id, data_tenant_id, email, auth_type, status)
         VALUES ($1, $2, $3, 'password', 'active')",
        )
        .bind(id)
        .bind(data_tenant_id.as_uuid())
        .bind(format!("{}-{email_tag}@example.com", id.simple()))
        .execute(pool)
        .await
        .expect("auth user inserts");
    }

    async fn insert_refresh_token(
        pool: &PgPool,
        data_tenant_id: DataTenantId,
        id: Uuid,
        principal_id: Uuid,
        token_hash: &str,
    ) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
        sqlx::query(
            "INSERT INTO wyrd.auth_refresh_tokens
             (id, data_tenant_id, principal_kind, principal_id, token_hash, expires_at)
         VALUES ($1, $2, 'user', $3, $4, now() + interval '1 day')",
        )
        .bind(id)
        .bind(data_tenant_id.as_uuid())
        .bind(principal_id)
        .bind(token_hash)
        .execute(pool)
        .await
    }

    async fn cleanup_refresh_token_test_rows(
        pool: &PgPool,
        tenant_a: DataTenantId,
        tenant_b: DataTenantId,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM wyrd.auth_refresh_tokens WHERE data_tenant_id IN ($1, $2)")
            .bind(tenant_a.as_uuid())
            .bind(tenant_b.as_uuid())
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM wyrd.auth_users WHERE data_tenant_id IN ($1, $2)")
            .bind(tenant_a.as_uuid())
            .bind(tenant_b.as_uuid())
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM platform.tenants WHERE data_tenant_id IN ($1, $2)")
            .bind(tenant_a.as_uuid())
            .bind(tenant_b.as_uuid())
            .execute(pool)
            .await?;

        Ok(())
    }

    async fn cleanup_auth_test_rows(
        pool: &PgPool,
        tenants: &[DataTenantId],
    ) -> Result<(), sqlx::Error> {
        let tenant_ids = tenants
            .iter()
            .map(|tenant| tenant.as_uuid())
            .collect::<Vec<_>>();

        sqlx::query("DELETE FROM wyrd.auth_user_identities WHERE data_tenant_id = ANY($1)")
            .bind(&tenant_ids)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM wyrd.auth_users WHERE data_tenant_id = ANY($1)")
            .bind(&tenant_ids)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM platform.tenants WHERE data_tenant_id = ANY($1)")
            .bind(&tenant_ids)
            .execute(pool)
            .await?;

        Ok(())
    }

    async fn cleanup_storage_test_rows(
        pool: &PgPool,
        tenants: &[DataTenantId],
    ) -> Result<(), sqlx::Error> {
        let tenant_ids = tenants
            .iter()
            .map(|tenant| tenant.as_uuid())
            .collect::<Vec<_>>();

        sqlx::query("DELETE FROM wyrd.storage_idempotency_keys WHERE data_tenant_id = ANY($1)")
            .bind(&tenant_ids)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM wyrd.storage_artifact_metadata WHERE data_tenant_id = ANY($1)")
            .bind(&tenant_ids)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM wyrd.storage_multipart_uploads WHERE data_tenant_id = ANY($1)")
            .bind(&tenant_ids)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM platform.tenants WHERE data_tenant_id = ANY($1)")
            .bind(&tenant_ids)
            .execute(pool)
            .await?;

        Ok(())
    }
}
