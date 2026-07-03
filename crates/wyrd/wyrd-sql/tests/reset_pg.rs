//! Live-Postgres self-tests for `wyrd_sql::testing::reset_owned_schemas`.
//!
//! These run serially under `mise run test:pg` against the dedicated `wyrd_test`
//! database (migrated once by `db:migrate:all`). The whole file is gated on the
//! `testing` feature so the DB-free `test:rust:workspace` shard — which compiles
//! wyrd-sql without `--all-features` — never sees the `wyrd_sql::testing` symbol.
#![cfg(feature = "testing")]

use wyrd_sql::OWNED_SCHEMAS;
use wyrd_sql::testing;

#[tokio::test]
async fn reset_empties_owned_tables_and_restarts_identity() {
    let db = testing::shared().await.expect("shared test DB");
    db.reset().await.expect("shared DB reset");
    let mut conn = db.migrator.acquire().await.expect("migrator connection");

    sqlx::query("DROP TABLE IF EXISTS wyrd.reset_selftest")
        .execute(&mut *conn)
        .await
        .expect("drops any leftover scratch table");
    sqlx::query(
        "CREATE TABLE wyrd.reset_selftest (
             id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
             label text NOT NULL
         )",
    )
    .execute(&mut *conn)
    .await
    .expect("creates scratch table in an owned schema");

    for label in ["a", "b", "c"] {
        sqlx::query("INSERT INTO wyrd.reset_selftest (label) VALUES ($1)")
            .bind(label)
            .execute(&mut *conn)
            .await
            .expect("seeds scratch row");
    }

    testing::reset_owned_schemas(&mut *conn, OWNED_SCHEMAS)
        .await
        .expect("reset succeeds");

    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.reset_selftest")
        .fetch_one(&mut *conn)
        .await
        .expect("counts scratch rows after reset");
    assert_eq!(
        remaining, 0,
        "reset must empty every data table in the owned schemas"
    );

    let next_id: i64 = sqlx::query_scalar(
        "INSERT INTO wyrd.reset_selftest (label) VALUES ('post-reset') RETURNING id",
    )
    .fetch_one(&mut *conn)
    .await
    .expect("inserts a row after reset");
    assert_eq!(
        next_id, 1,
        "RESTART IDENTITY must reset the identity sequence to 1"
    );

    sqlx::query("DROP TABLE wyrd.reset_selftest")
        .execute(&mut *conn)
        .await
        .expect("drops scratch table");
}

#[tokio::test]
async fn reset_preserves_sqlx_migrations_ledger() {
    let db = testing::shared().await.expect("shared test DB");
    db.reset().await.expect("shared DB reset");
    let mut conn = db.migrator.acquire().await.expect("migrator connection");

    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd._sqlx_migrations")
        .fetch_one(&mut *conn)
        .await
        .expect("counts the migration ledger before reset");
    assert!(
        before > 0,
        "the migration ledger must be populated before the reset runs"
    );

    testing::reset_owned_schemas(&mut *conn, OWNED_SCHEMAS)
        .await
        .expect("reset succeeds");

    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd._sqlx_migrations")
        .fetch_one(&mut *conn)
        .await
        .expect("counts the migration ledger after reset");
    assert_eq!(
        after, before,
        "_sqlx_migrations must survive the reset so migrate-once holds"
    );
}
