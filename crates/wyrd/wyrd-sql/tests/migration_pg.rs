//! Live Postgres migration integration test.
//!
//! Skipped automatically when DATABASE_URL is unset so the default test suite
//! remains credential-free. Run with:
//!   DATABASE_URL=postgres://... cargo test -p wyrd-sql --all-features

use wyrd_sql::SqlStore;

#[tokio::test]
async fn migrations_apply_and_are_idempotent() {
    let url = match std::env::var("DATABASE_URL") {
        Ok(u) => u,
        Err(_) => return,
    };

    let store = SqlStore::connect(&url, 2)
        .await
        .expect("connects to postgres");
    store.migrate().await.expect("first migration run succeeds");
    store
        .migrate()
        .await
        .expect("second migration run is idempotent");
}

#[tokio::test]
async fn refresh_token_hash_unique_constraint_exists() {
    let url = match std::env::var("DATABASE_URL") {
        Ok(u) => u,
        Err(_) => return,
    };

    let store = SqlStore::connect(&url, 2)
        .await
        .expect("connects to postgres");
    store.migrate().await.expect("migrations apply");

    let pool = store.pool();
    let row: (bool,) = sqlx::query_as(
        "SELECT EXISTS (
            SELECT 1 FROM pg_indexes
            WHERE tablename = 'refresh_tokens'
              AND indexname = 'idx_refresh_tokens_token_hash'
        )",
    )
    .fetch_one(pool)
    .await
    .expect("index query succeeds");

    assert!(
        row.0,
        "idx_refresh_tokens_token_hash unique index must exist"
    );
}

#[tokio::test]
async fn duplicate_refresh_token_hash_rejected() {
    let url = match std::env::var("DATABASE_URL") {
        Ok(u) => u,
        Err(_) => return,
    };

    let store = SqlStore::connect(&url, 2)
        .await
        .expect("connects to postgres");
    store.migrate().await.expect("migrations apply");
    let pool = store.pool();

    let user_id = "test-user-unique-hash";
    let _ = sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await;

    let _ = sqlx::query(
        "INSERT INTO users (id, email, auth_type, status)
         VALUES ($1, $2, 'password', 'active')
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(format!("{user_id}@example.com"))
    .execute(pool)
    .await
    .expect("user insert");

    let insert = "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at)
                  VALUES ($1, $2, $3, now() + interval '1 day')";

    sqlx::query(insert)
        .bind("tok-a")
        .bind(user_id)
        .bind("hash-collision-test")
        .execute(pool)
        .await
        .expect("first token inserts");

    let second = sqlx::query(insert)
        .bind("tok-b")
        .bind(user_id)
        .bind("hash-collision-test")
        .execute(pool)
        .await;

    assert!(
        second.is_err(),
        "duplicate token_hash must be rejected by unique index"
    );

    let _ = sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await;
}
