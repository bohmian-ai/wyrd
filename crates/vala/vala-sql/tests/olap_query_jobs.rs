//! SQL integration tests for Stage 3 async query-job storage.
//!
//! Covers idempotent submit, status projection to the S3.C2a wire types, and
//! cross-tenant invisibility under RLS. Run via `mise run test:sql`.

use sqlx::types::JsonValue;
use vala_sql::queries::olap_query_jobs::{NewQueryJob, enqueue_query_job, query_job_status};
use wyrd_sql::testing::SharedDb;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AsyncJobState, ExecutorAvailability};

async fn setup() -> (SharedDb, DataTenantId) {
    let db = vala_sql::testing::shared().await.expect("shared db");
    vala_sql::testing::reset_for_test(&db).await.expect("reset");
    let tenant = DataTenantId::new_v7();
    vala_sql::testing::seed_tenant(&db.platform_admin, tenant.as_uuid())
        .await
        .unwrap();
    (db, tenant)
}

#[tokio::test]
async fn olap_query_jobs_enqueue_is_idempotent() {
    let (db, tenant) = setup().await;
    let params = JsonValue::Array(Vec::new());

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant).await.unwrap();
    let first = enqueue_query_job(
        &mut conn,
        NewQueryJob {
            idempotency_key: "idem-1",
            sql_normalized: "SELECT $1",
            params_redacted: &params,
        },
    )
    .await
    .unwrap();
    conn.commit().await.unwrap();

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant).await.unwrap();
    let second = enqueue_query_job(
        &mut conn,
        NewQueryJob {
            idempotency_key: "idem-1",
            sql_normalized: "SELECT $1",
            params_redacted: &params,
        },
    )
    .await
    .unwrap();
    conn.commit().await.unwrap();

    assert_eq!(first, second, "idempotent submit returns the same job_uid");

    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM vala.olap_query_jobs WHERE data_tenant_id = $1")
            .bind(tenant.as_uuid())
            .fetch_one(&db.migrator)
            .await
            .unwrap();
    assert_eq!(
        count, 1,
        "a repeated idempotency key must persist exactly one row"
    );
}

#[tokio::test]
async fn olap_query_jobs_status_reports_pending_stage5() {
    let (db, tenant) = setup().await;
    let params = JsonValue::Array(Vec::new());

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant).await.unwrap();
    let job_uid = enqueue_query_job(
        &mut conn,
        NewQueryJob {
            idempotency_key: "idem-2",
            sql_normalized: "SELECT 1",
            params_redacted: &params,
        },
    )
    .await
    .unwrap();
    conn.commit().await.unwrap();

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant).await.unwrap();
    let status = query_job_status(&mut conn, job_uid)
        .await
        .unwrap()
        .expect("owner must see its own job");
    conn.commit().await.unwrap();

    assert_eq!(status.job_uid, job_uid);
    assert_eq!(status.state, AsyncJobState::Queued);
    assert_eq!(
        status.executor_availability,
        ExecutorAvailability::PendingStage5
    );
    assert!(status.error_code.is_none());
    assert!(status.error_detail.is_none());
}

#[tokio::test]
async fn olap_query_jobs_cross_tenant_status_is_invisible() {
    let (db, tenant_a) = setup().await;
    let tenant_b = DataTenantId::new_v7();
    vala_sql::testing::seed_tenant(&db.platform_admin, tenant_b.as_uuid())
        .await
        .unwrap();
    let params = JsonValue::Array(Vec::new());

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant_a)
        .await
        .unwrap();
    let job_uid = enqueue_query_job(
        &mut conn,
        NewQueryJob {
            idempotency_key: "idem-3",
            sql_normalized: "SELECT 1",
            params_redacted: &params,
        },
    )
    .await
    .unwrap();
    conn.commit().await.unwrap();

    let mut conn = vala_sql::TenantConn::acquire(&db.app, tenant_b)
        .await
        .unwrap();
    let status = query_job_status(&mut conn, job_uid).await.unwrap();
    conn.commit().await.unwrap();

    assert!(
        status.is_none(),
        "a job in tenant A must be invisible to tenant B under RLS"
    );
}
