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
use wyrd_spec::storage::{StorageBackendKind, UploadId, WireProtocol};
use wyrd_sql::pool::build_app_pool;
use wyrd_sql::queries::platform::audit_log::{StorageAuditEvent, write_storage_event};
use wyrd_sql::queries::storage;
use wyrd_sql::{SqlStore, TenantConn};

#[tokio::test]
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
    assert_regclass_exists(pool, "platform.users", true).await;
    assert_regclass_exists(pool, "platform.audit_log", true).await;
    assert_regclass_exists(pool, "wyrd.auth_users", true).await;
    assert_regclass_exists(pool, "wyrd.auth_refresh_tokens", true).await;

    assert_platform_resolver_shape(pool).await;
    assert_current_tenant_parallel_restricted(pool).await;
    assert_auth_rls_metadata(pool).await;
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

        storage::multipart_uploads::mark_pending(&mut conn, upload_id, Some("backend-upload-id"))
            .await
            .expect("upload marks pending");

        let pending =
            storage::multipart_uploads::find_pending_for_dedupe(&mut conn, &card_uid, &sha)
                .await
                .expect("pending lookup succeeds")
                .expect("pending row found");
        assert_eq!(pending.backend, StorageBackendKind::S3);
        assert_eq!(pending.wire_protocol, WireProtocol::S3MultipartV1);

        storage::multipart_uploads::mark_completed(&mut conn, upload_id)
            .await
            .expect("upload marks completed");

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

        write_storage_event(
            &mut conn,
            StorageAuditEvent {
                subject_id: "test-subject",
                operation: "upload_complete",
                storage_path: &storage_path,
                status_code: 200,
                error_code: None,
                request_id: "test-request",
                backend: StorageBackendKind::S3,
                upload_id: Some(upload_id),
            },
        )
        .await
        .expect("storage audit event writes");

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
        Duration::from_secs(900),
    )
    .await
    .expect("expired upload query succeeds");
    assert!(
        rows.iter().any(|row| row.id == upload_id),
        "expired upload must be selected for sweeping"
    );

    storage::admin::multipart_uploads::mark_aborted_admin(store.pool(), upload_id, "test-sweeper")
        .await
        .expect("admin abort update succeeds");

    cleanup_storage_test_rows(store.pool(), &[tenant])
        .await
        .expect("storage admin test rows clean up");
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

async fn assert_auth_rls_metadata(pool: &PgPool) {
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
           AND c.relname LIKE 'auth_%'
         ORDER BY c.relname",
    )
    .fetch_all(pool)
    .await
    .expect("auth RLS metadata query succeeds");

    let expected_tables = [
        "auth_api_keys",
        "auth_refresh_tokens",
        "auth_roles",
        "auth_service_account_roles",
        "auth_service_accounts",
        "auth_user_roles",
        "auth_users",
    ];

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

async fn insert_auth_user(pool: &PgPool, data_tenant_id: DataTenantId, id: Uuid, email_tag: &str) {
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

async fn cleanup_storage_test_rows(
    pool: &PgPool,
    tenants: &[DataTenantId],
) -> Result<(), sqlx::Error> {
    let tenant_ids = tenants
        .iter()
        .map(|tenant| tenant.as_uuid())
        .collect::<Vec<_>>();

    sqlx::query("DELETE FROM wyrd.storage_access_ledger WHERE data_tenant_id = ANY($1)")
        .bind(&tenant_ids)
        .execute(pool)
        .await?;
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
