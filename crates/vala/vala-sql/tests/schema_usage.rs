//! Regression test for S3.C2perm: wyrd_app must hold USAGE ON SCHEMA vala.
//!
//! Migration `20260801000002_vala_app_schema_usage` grants USAGE so the existing
//! per-table DML grants on `vala.*` become reachable. Without it, an INSERT into
//! `vala.bifrost_tables` through the request-path `wyrd_app` role fails with
//! Postgres 42501 (permission denied for schema vala) -- the exact gap the
//! shipped migrations left. This test uses the shared-DB `db.app` pool, which
//! connects as the runtime `wyrd_app` role, so the insert is gated by the same
//! grants production enforces; it would fail 42501 on base.
//!
//! Run via `mise run test:sql`.

use vala_sql::TenantConn;
use vala_sql::queries::olap_catalog::upsert_table;
use wyrd_spec::DataTenantId;

#[tokio::test]
async fn schema_usage_grant_lets_wyrd_app_register_bifrost_table() {
    let db = vala_sql::testing::shared().await.expect("shared db");
    vala_sql::testing::reset_for_test(&db).await.expect("reset");
    let tenant = DataTenantId::new_v7();
    vala_sql::testing::seed_tenant(&db.platform_admin, tenant.as_uuid())
        .await
        .unwrap();

    // `db.app` connects as the runtime `wyrd_app` role — the same RLS-enforced,
    // non-BYPASSRLS identity the request path uses. No `SET ROLE` dance is
    // needed: the insert is naturally gated by the grants production enforces,
    // and without USAGE ON SCHEMA vala it 42501s.
    let mut conn = TenantConn::acquire(&db.app, tenant).await.unwrap();

    let table_uid = [1u8; 16];
    let fingerprint = [0u8; 32];
    let partition_columns = vec!["day".to_string()];

    upsert_table(
        &mut conn,
        &table_uid,
        "space.table",
        &fingerprint,
        "tenant_owned",
        &partition_columns,
    )
    .await
    .expect("wyrd_app must reach vala.bifrost_tables once USAGE ON SCHEMA vala is granted");

    conn.commit().await.unwrap();
}
