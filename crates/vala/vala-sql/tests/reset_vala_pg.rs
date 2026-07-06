//! Verifies that `reset_for_test` clears Vala-specific tables and re-seeds sentinel data.
//!
//! Run via `mise run test:sql`.

use wyrd_spec::DataTenantId;

#[tokio::test]
async fn reset_clears_vala_tables_and_reseeds_sentinel() {
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
        "ns.reset_test",
        &fingerprint,
        "tenant_owned",
        &[],
    )
    .await
    .unwrap();
    vala_sql::queries::olap_catalog::precommit(&mut conn, &table_uid, &batch_id)
        .await
        .unwrap();
    conn.commit().await.unwrap();

    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vala.olap_commits")
        .fetch_one(&db.migrator)
        .await
        .unwrap();
    assert_eq!(before, 1, "precommit row must exist before reset");

    vala_sql::testing::reset_for_test(&db)
        .await
        .expect("second reset");

    let after_commits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vala.olap_commits")
        .fetch_one(&db.migrator)
        .await
        .unwrap();
    assert_eq!(after_commits, 0, "olap_commits must be empty after reset");

    let after_events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vala.olap_recovery_events")
        .fetch_one(&db.migrator)
        .await
        .unwrap();
    assert_eq!(
        after_events, 0,
        "olap_recovery_events must be empty after reset"
    );

    let sentinel: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM platform.tenants WHERE slug = 'wyrd-system-owner'",
    )
    .fetch_one(&db.migrator)
    .await
    .unwrap();
    assert_eq!(sentinel, 1, "wyrd-system-owner sentinel must be re-seeded");
}
