//! Live Postgres Vala migration integration test.
//!
//! Skipped automatically when Wyrd database env vars are unset so the default
//! test suite remains credential-free.

use secrecy::ExposeSecret;

#[tokio::test]
async fn vala_migrations_apply_and_are_idempotent() {
    let Some(url) = std::env::var("WYRD_DATABASE_URL").ok() else {
        return;
    };
    let _ = url;

    let dsns = wyrd_sql::dsn::resolve_external_dsns_from_env()
        .expect("dsn resolve")
        .expect("WYRD_DATABASE_URL + WYRD_DATABASE_MIGRATOR_PASSWORD set");
    let pool = wyrd_sql::pool::build_pool(
        dsns.migrator.expose_secret(),
        wyrd_sql::PoolConfig::migrator_defaults(),
    )
    .await
    .expect("migrator pool");

    wyrd_sql::migrate(&pool)
        .await
        .expect("wyrd migrate is idempotent");
    vala_sql::migrate(&pool).await.expect("first vala migrate");
    vala_sql::migrate(&pool)
        .await
        .expect("second vala migrate is idempotent");

    for schema in vala_sql::OWNED_SCHEMAS {
        let exists: (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)")
                .bind(schema)
                .fetch_one(&pool)
                .await
                .expect("schema query");
        assert!(exists.0, "{schema} schema exists after vala migrate");
    }
}
