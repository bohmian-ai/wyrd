//! Live Postgres migration integration test.
//!
//! Skipped automatically when DATABASE_URL is unset so the default test suite
//! remains credential-free. Run with:
//!   DATABASE_URL=postgres://... cargo test -p wyrd-sql --all-features

use sqlx::PgPool;
use wyrd_spec::DataTenantId;
use wyrd_sql::SqlStore;

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
    let user_a = format!("test-user-a-{suffix}");
    let user_b = format!("test-user-b-{suffix}");
    let token_a = format!("test-token-a-{suffix}");
    let token_b = format!("test-token-b-{suffix}");
    let token_c = format!("test-token-c-{suffix}");
    let token_hash = format!("hash-collision-{suffix}");

    insert_tenant(pool, tenant_a, &format!("test-a-{suffix}")).await;
    insert_tenant(pool, tenant_b, &format!("test-b-{suffix}")).await;
    insert_auth_user(pool, tenant_a, &user_a, "a").await;
    insert_auth_user(pool, tenant_b, &user_b, "b").await;

    insert_refresh_token(pool, tenant_a, &token_a, &user_a, &token_hash)
        .await
        .expect("first same-tenant token inserts");

    let duplicate_same_tenant =
        insert_refresh_token(pool, tenant_a, &token_b, &user_a, &token_hash).await;
    assert!(
        duplicate_same_tenant.is_err(),
        "duplicate token_hash in the same tenant must be rejected"
    );

    insert_refresh_token(pool, tenant_b, &token_c, &user_b, &token_hash)
        .await
        .expect("same token_hash under a different tenant inserts");

    cleanup_refresh_token_test_rows(pool, tenant_a, tenant_b)
        .await
        .expect("test rows clean up");
}

fn database_url() -> Option<String> {
    std::env::var("DATABASE_URL").ok()
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

    let privileges: (bool, bool, bool) = sqlx::query_as(
        "SELECT
             has_function_privilege('PUBLIC', 'platform.resolve_tenant_by_slug(text)', 'EXECUTE'),
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
        "auth_governance_tokens",
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

async fn insert_auth_user(pool: &PgPool, data_tenant_id: DataTenantId, id: &str, email_tag: &str) {
    sqlx::query(
        "INSERT INTO wyrd.auth_users (id, data_tenant_id, email, auth_type, status)
         VALUES ($1, $2, $3, 'password', 'active')",
    )
    .bind(id)
    .bind(data_tenant_id.as_uuid())
    .bind(format!("{id}-{email_tag}@example.com"))
    .execute(pool)
    .await
    .expect("auth user inserts");
}

async fn insert_refresh_token(
    pool: &PgPool,
    data_tenant_id: DataTenantId,
    id: &str,
    user_id: &str,
    token_hash: &str,
) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    sqlx::query(
        "INSERT INTO wyrd.auth_refresh_tokens
             (id, data_tenant_id, user_id, token_hash, expires_at)
         VALUES ($1, $2, $3, $4, now() + interval '1 day')",
    )
    .bind(id)
    .bind(data_tenant_id.as_uuid())
    .bind(user_id)
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
