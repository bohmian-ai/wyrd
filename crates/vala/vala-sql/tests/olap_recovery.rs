//! SQL integration tests for Stage 2 OLAP recovery primitives.
//!
//! Covers the widened state FSM, the recovery audit table, and the
//! SECURITY DEFINER claim/finalize routines added by the olap_recovery
//! migration. Run against a live Postgres:
//!   mise run test:bifrost

use sqlx::types::Uuid;
use wyrd_spec::DataTenantId;
use wyrd_sql::testing::SharedDb;

async fn setup() -> (SharedDb, DataTenantId, [u8; 16], [u8; 16]) {
    let db = vala_sql::testing::shared().await.expect("shared db");
    vala_sql::testing::reset_for_test(&db).await.expect("reset");
    let tenant = DataTenantId::new_v7();
    vala_sql::testing::seed_tenant(&db.platform_admin, tenant.as_uuid())
        .await
        .unwrap();

    let table_uid = *uuid::Uuid::now_v7().as_bytes();
    let batch_id = *uuid::Uuid::now_v7().as_bytes();
    let fingerprint = [0u8; 32];
    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    vala_sql::queries::olap_catalog::upsert_table(
        &mut conn,
        &table_uid,
        "ns.tbl",
        &fingerprint,
        "tenant_owned",
        &[],
    )
    .await
    .unwrap();
    conn.commit().await.unwrap();

    (db, tenant, table_uid, batch_id)
}

#[tokio::test]
async fn aborted_state_accepted() {
    let (db, tenant, table_uid, batch_id) = setup().await;

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    vala_sql::queries::olap_catalog::precommit(
        &mut conn, &table_uid, &batch_id, "system", "system",
    )
    .await
    .unwrap();
    vala_sql::queries::olap_catalog::finalize_aborted(&mut conn, &table_uid, &batch_id)
        .await
        .unwrap();
    conn.commit().await.unwrap();

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    let row = vala_sql::queries::olap_catalog::lookup_idempotent(&mut conn, &table_uid, &batch_id)
        .await
        .unwrap()
        .expect("row must exist");
    conn.commit().await.unwrap();

    assert_eq!(row.state, "aborted");
}

#[tokio::test]
async fn audit_row_roundtrips_with_byte_ids() {
    let (db, tenant, table_uid, batch_id) = setup().await;

    let owner = Uuid::from_bytes([0x01u8; 16]);
    let token: i64 = 999;
    let snapshot_id: i64 = 12345;

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    vala_sql::queries::olap_catalog::precommit(
        &mut conn, &table_uid, &batch_id, "system", "system",
    )
    .await
    .unwrap();
    conn.commit().await.unwrap();

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    vala_sql::queries::olap_catalog::record_fence_loss_after_append(
        &mut conn,
        &table_uid,
        &batch_id,
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
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .fetch_one(&db.migrator)
    .await
    .unwrap();

    assert_eq!(count, 1, "audit row must be inserted");
}

#[tokio::test]
async fn claim_skips_live_lease() {
    let (db, tenant, table_uid, batch_id) = setup().await;

    let owner = Uuid::from_bytes([0x02u8; 16]);
    let token: i64 = 1;

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    vala_sql::queries::olap_catalog::precommit(
        &mut conn, &table_uid, &batch_id, "system", "system",
    )
    .await
    .unwrap();
    vala_sql::queries::olap_catalog::record_writer_lease(
        &mut conn, &table_uid, &batch_id, owner, token, 3600,
    )
    .await
    .unwrap();
    conn.commit().await.unwrap();

    let recovery_owner = Uuid::from_bytes([0x03u8; 16]);
    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
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

#[tokio::test]
async fn claim_takes_expired_lease() {
    let (db, tenant, table_uid, batch_id) = setup().await;

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    vala_sql::queries::olap_catalog::precommit(
        &mut conn, &table_uid, &batch_id, "system", "system",
    )
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
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .bind(Uuid::from_bytes([0x04u8; 16]))
    .execute(&db.migrator)
    .await
    .unwrap();

    let recovery_owner = Uuid::from_bytes([0x05u8; 16]);
    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    let claimed =
        vala_sql::queries::olap_catalog::claim_stale_precommits(&mut conn, recovery_owner, 10)
            .await
            .unwrap();
    conn.commit().await.unwrap();

    assert_eq!(claimed.len(), 1, "expired-lease row must be claimed");
    assert_eq!(claimed[0].table_uid, table_uid.as_slice());
    assert_eq!(claimed[0].batch_id, batch_id.as_slice());
    assert_eq!(claimed[0].fqn, "ns.tbl");
    assert_eq!(claimed[0].namespace, "ns");
    assert_eq!(claimed[0].name, "tbl");
    assert_ne!(claimed[0].fencing_token, 0);
}

#[tokio::test]
async fn renew_fence_returns_false_after_claim() {
    let (db, tenant, table_uid, _) = setup().await;
    let batch_id = *uuid::Uuid::now_v7().as_bytes();

    let writer_owner = Uuid::from_bytes([0x06u8; 16]);
    let writer_token: i64 = 42;

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    vala_sql::queries::olap_catalog::precommit(
        &mut conn, &table_uid, &batch_id, "system", "system",
    )
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
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .bind(writer_owner)
    .bind(writer_token)
    .execute(&db.migrator)
    .await
    .unwrap();

    let recovery_owner = Uuid::from_bytes([0x07u8; 16]);
    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    let claimed =
        vala_sql::queries::olap_catalog::claim_stale_precommits(&mut conn, recovery_owner, 10)
            .await
            .unwrap();
    conn.commit().await.unwrap();

    assert_eq!(claimed.len(), 1, "recovery must have claimed the row");

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    let held = vala_sql::queries::olap_catalog::renew_writer_fence(
        &mut conn,
        &table_uid,
        &batch_id,
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

#[tokio::test]
async fn scan_failed_leaves_precommit_and_allows_retry() {
    let (db, tenant, table_uid, batch_id) = setup().await;

    // Bare precommit (no lease) is immediately claimable.
    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    vala_sql::queries::olap_catalog::precommit(
        &mut conn, &table_uid, &batch_id, "system", "system",
    )
    .await
    .unwrap();
    conn.commit().await.unwrap();

    let recovery_owner = Uuid::from_bytes([0x08u8; 16]);
    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    let claimed =
        vala_sql::queries::olap_catalog::claim_stale_precommits(&mut conn, recovery_owner, 10)
            .await
            .unwrap();
    conn.commit().await.unwrap();
    assert_eq!(claimed.len(), 1, "bare precommit must be claimed");
    let token = claimed[0].fencing_token;

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    vala_sql::queries::olap_catalog::mark_recovery_scan_failed(
        &mut conn,
        &table_uid,
        &batch_id,
        token,
        "transient FileIO error",
    )
    .await
    .unwrap();
    conn.commit().await.unwrap();

    let (state, attempts, last_error, fencing_token): (String, i32, Option<String>, Option<i64>) =
        sqlx::query_as(
            "SELECT state, recovery_attempts, recovery_last_error, recovery_fencing_token
               FROM vala.olap_commits
              WHERE table_uid = $1 AND batch_id = $2",
        )
        .bind(table_uid.as_slice())
        .bind(batch_id.as_slice())
        .fetch_one(&db.migrator)
        .await
        .unwrap();

    assert_eq!(
        state, "precommit",
        "scan_failed must NOT advance to aborted"
    );
    assert_eq!(attempts, 1, "recovery_attempts must increment to 1");
    assert_eq!(last_error.as_deref(), Some("transient FileIO error"));
    assert!(
        fencing_token.is_none(),
        "recovery_fencing_token must be cleared so the row is re-claimable"
    );

    // Audit row captures the live recovery_owner (proves audit-before-update order).
    let audit_owner: Option<Uuid> = sqlx::query_scalar(
        "SELECT recovery_owner FROM vala.olap_recovery_events
          WHERE table_uid = $1 AND batch_id = $2 AND oracle_result = 'scan_failed'",
    )
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .fetch_one(&db.migrator)
    .await
    .unwrap();
    assert_eq!(
        audit_owner,
        Some(recovery_owner),
        "scan_failed audit row must record the claiming recovery owner"
    );

    // A second recovery pass can re-claim the row (retry is possible).
    let retry_owner = Uuid::from_bytes([0x09u8; 16]);
    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant)
        .await
        .unwrap();
    let reclaimed =
        vala_sql::queries::olap_catalog::claim_stale_precommits(&mut conn, retry_owner, 10)
            .await
            .unwrap();
    conn.commit().await.unwrap();
    assert_eq!(reclaimed.len(), 1, "scan_failed row must be re-claimable");
}
