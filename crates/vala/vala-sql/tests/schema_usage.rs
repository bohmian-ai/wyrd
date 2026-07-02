//! Regression test for S3.C2perm: wyrd_app must hold USAGE ON SCHEMA vala.
//!
//! Migration `20260801000002_vala_app_schema_usage` grants USAGE so the existing
//! per-table DML grants on `vala.*` become reachable. Without it, an INSERT into
//! `vala.bifrost_tables` through the request-path `wyrd_app` role fails with
//! Postgres 42501 (permission denied for schema vala) -- the exact gap the
//! shipped migrations left, masked in vala-bifrost's own tests by a superuser
//! pool. This test drops to `wyrd_app` privileges (SET ROLE after binding the
//! tenant GUC) and asserts the insert succeeds; it would fail 42501 on base.
//!
//!   DATABASE_URL=... cargo test -p vala-sql --all-features schema_usage
//!            -- --test-threads=1

use sqlx::PgPool;
use vala_sql::TenantConn;
use vala_sql::queries::olap_catalog::upsert_table;
use wyrd_spec::DataTenantId;

async fn setup(pool: &PgPool) -> DataTenantId {
    vala_sql::testing::migrate_for_test(pool).await.unwrap();
    let tenant = DataTenantId::new_v7();
    vala_sql::testing::seed_tenant(pool, tenant.as_uuid())
        .await
        .unwrap();
    tenant
}

#[sqlx::test(migrations = false)]
async fn schema_usage_grant_lets_wyrd_app_register_bifrost_table(pool: PgPool) {
    let tenant = setup(&pool).await;

    let mut conn = TenantConn::acquire(&pool, tenant).await.unwrap();

    // The `#[sqlx::test]` pool connects as the privileged migration role, which
    // bypasses the schema/DML/RLS checks the request path is subject to. Drop to
    // `wyrd_app` (the role the runtime app pool uses) so the insert is gated by
    // the same grants production enforces. The tenant GUC is already bound by
    // `TenantConn::acquire`; `SET ROLE` after it keeps `wyrd.current_tenant()`
    // resolving to `tenant`. Without USAGE ON SCHEMA vala this insert 42501s.
    sqlx::query("GRANT wyrd_app TO current_user")
        .execute(&mut **conn.transaction())
        .await
        .expect("grant wyrd_app membership to the test role");
    sqlx::query("SET ROLE wyrd_app")
        .execute(&mut **conn.transaction())
        .await
        .expect("drop to wyrd_app privileges");

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
