//! SQL integration tests for Stage 2 OLAP recovery primitives.
//!
//! Covers the widened state FSM, the recovery audit table, and the
//! SECURITY DEFINER claim/finalize routines added by the olap_recovery
//! migration. Run against a live Postgres:
//!   DATABASE_URL=... cargo test -p vala-sql --all-features --test olap_recovery

use sqlx::PgPool;
use sqlx::types::Uuid;
use wyrd_spec::DataTenantId;

const TABLE_UID: [u8; 16] = [0x10; 16];
const BATCH_ID: [u8; 16] = [0x20; 16];
const BATCH_ID_2: [u8; 16] = [0x21; 16];

async fn setup(pool: &PgPool) -> DataTenantId {
    vala_sql::testing::migrate_for_test(pool).await.unwrap();
    let tenant = DataTenantId::new_v7();
    vala_sql::testing::seed_tenant(pool, tenant.as_uuid())
        .await
        .unwrap();

    let fingerprint = [0u8; 32];
    let mut conn = vala_sql::TenantConn::acquire(pool, tenant).await.unwrap();
    vala_sql::queries::olap_catalog::upsert_table(
        &mut conn,
        &TABLE_UID,
        "ns.tbl",
        &fingerprint,
        "tenant_owned",
        &[],
    )
    .await
    .unwrap();
    conn.commit().await.unwrap();

    tenant
}

#[sqlx::test(migrations = false)]
async fn aborted_state_accepted(pool: PgPool) {
    let tenant = setup(&pool).await;

    let mut conn = vala_sql::TenantConn::acquire(&pool, tenant).await.unwrap();
    vala_sql::queries::olap_catalog::precommit(&mut conn, &TABLE_UID, &BATCH_ID)
        .await
        .unwrap();
    vala_sql::queries::olap_catalog::finalize_aborted(&mut conn, &TABLE_UID, &BATCH_ID)
        .await
        .unwrap();
    conn.commit().await.unwrap();

    let mut conn = vala_sql::TenantConn::acquire(&pool, tenant).await.unwrap();
    let row = vala_sql::queries::olap_catalog::lookup_idempotent(&mut conn, &TABLE_UID, &BATCH_ID)
        .await
        .unwrap()
        .expect("row must exist");
    conn.commit().await.unwrap();

    assert_eq!(row.state, "aborted");
}

#[sqlx::test(migrations = false)]
async fn audit_row_roundtrips_with_byte_ids(pool: PgPool) {
    let tenant = setup(&pool).await;

    let owner = Uuid::from_bytes([0x01u8; 16]);
    let token: i64 = 999;
    let snapshot_id: i64 = 12345;

    let mut conn = vala_sql::TenantConn::acquire(&pool, tenant).await.unwrap();
    vala_sql::queries::olap_catalog::precommit(&mut conn, &TABLE_UID, &BATCH_ID)
        .await
        .unwrap();
    conn.commit().await.unwrap();

    let mut conn = vala_sql::TenantConn::acquire(&pool, tenant).await.unwrap();
    vala_sql::queries::olap_catalog::record_fence_loss_after_append(
        &mut conn,
        &TABLE_UID,
        &BATCH_ID,
        owner,
        token,
        snapshot_id,
        false,
        Some("iceberg append succeeded but fence lost"),
    )
    .await
    .unwrap();
    conn.commit().await.unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.olap_recovery_events
          WHERE table_uid = $1 AND batch_id = $2
            AND event_kind = 'fence_lost_after_append'",
    )
    .bind(TABLE_UID.as_slice())
    .bind(BATCH_ID.as_slice())
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(count, 1, "audit row must be inserted");
}

#[sqlx::test(migrations = false)]
async fn claim_skips_live_lease(pool: PgPool) {
    let tenant = setup(&pool).await;

    let owner = Uuid::from_bytes([0x02u8; 16]);
    let token: i64 = 1;

    let mut conn = vala_sql::TenantConn::acquire(&pool, tenant).await.unwrap();
    vala_sql::queries::olap_catalog::precommit(&mut conn, &TABLE_UID, &BATCH_ID)
        .await
        .unwrap();
    vala_sql::queries::olap_catalog::record_writer_lease(
        &mut conn, &TABLE_UID, &BATCH_ID, owner, token, 3600,
    )
    .await
    .unwrap();
    conn.commit().await.unwrap();

    let recovery_owner = Uuid::from_bytes([0x03u8; 16]);
    let mut conn = vala_sql::TenantConn::acquire(&pool, tenant).await.unwrap();
    let claimed =
        vala_sql::queries::olap_catalog::claim_stale_precommits(&mut conn, recovery_owner, 10)
            .await
            .unwrap();
    conn.commit().await.unwrap();

    assert!(
        claimed.is_empty(),
        "live-lease row must not be claimed by recovery"
    );
}

#[sqlx::test(migrations = false)]
async fn claim_takes_expired_lease(pool: PgPool) {
    let tenant = setup(&pool).await;

    let mut conn = vala_sql::TenantConn::acquire(&pool, tenant).await.unwrap();
    vala_sql::queries::olap_catalog::precommit(&mut conn, &TABLE_UID, &BATCH_ID)
        .await
        .unwrap();
    conn.commit().await.unwrap();

    sqlx::query(
        "UPDATE vala.olap_commits
            SET writer_owner         = $3,
                writer_fencing_token = 1,
                writer_lease_expires_at = now() - interval '1 minute'
          WHERE table_uid = $1 AND batch_id = $2",
    )
    .bind(TABLE_UID.as_slice())
    .bind(BATCH_ID.as_slice())
    .bind(Uuid::from_bytes([0x04u8; 16]))
    .execute(&pool)
    .await
    .unwrap();

    let recovery_owner = Uuid::from_bytes([0x05u8; 16]);
    let mut conn = vala_sql::TenantConn::acquire(&pool, tenant).await.unwrap();
    let claimed =
        vala_sql::queries::olap_catalog::claim_stale_precommits(&mut conn, recovery_owner, 10)
            .await
            .unwrap();
    conn.commit().await.unwrap();

    assert_eq!(claimed.len(), 1, "expired-lease row must be claimed");
    assert_eq!(claimed[0].table_uid, TABLE_UID.as_slice());
    assert_eq!(claimed[0].batch_id, BATCH_ID.as_slice());
    assert_eq!(claimed[0].fqn, "ns.tbl");
    assert_eq!(claimed[0].namespace, "ns");
    assert_eq!(claimed[0].name, "tbl");
    assert_ne!(claimed[0].fencing_token, 0);
}

#[sqlx::test(migrations = false)]
async fn renew_fence_returns_false_after_claim(pool: PgPool) {
    let tenant = setup(&pool).await;

    let writer_owner = Uuid::from_bytes([0x06u8; 16]);
    let writer_token: i64 = 42;

    let mut conn = vala_sql::TenantConn::acquire(&pool, tenant).await.unwrap();
    vala_sql::queries::olap_catalog::precommit(&mut conn, &TABLE_UID, &BATCH_ID_2)
        .await
        .unwrap();
    conn.commit().await.unwrap();

    sqlx::query(
        "UPDATE vala.olap_commits
            SET writer_owner            = $3,
                writer_fencing_token    = $4,
                writer_lease_expires_at = now() - interval '1 minute'
          WHERE table_uid = $1 AND batch_id = $2",
    )
    .bind(TABLE_UID.as_slice())
    .bind(BATCH_ID_2.as_slice())
    .bind(writer_owner)
    .bind(writer_token)
    .execute(&pool)
    .await
    .unwrap();

    let recovery_owner = Uuid::from_bytes([0x07u8; 16]);
    let mut conn = vala_sql::TenantConn::acquire(&pool, tenant).await.unwrap();
    let claimed =
        vala_sql::queries::olap_catalog::claim_stale_precommits(&mut conn, recovery_owner, 10)
            .await
            .unwrap();
    conn.commit().await.unwrap();

    assert_eq!(claimed.len(), 1, "recovery must have claimed the row");

    let mut conn = vala_sql::TenantConn::acquire(&pool, tenant).await.unwrap();
    let held = vala_sql::queries::olap_catalog::renew_writer_fence(
        &mut conn,
        &TABLE_UID,
        &BATCH_ID_2,
        writer_owner,
        writer_token,
        60,
    )
    .await
    .unwrap();
    conn.commit().await.unwrap();

    assert!(
        !held,
        "renew_writer_fence must return false when recovery has claimed the row"
    );
}
