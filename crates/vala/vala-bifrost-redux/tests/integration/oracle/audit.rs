//! The read decision is audited before rows are permitted, and an audit
//! failure fails closed without releasing bytes.
//!
//! Module of the `oracle` group; shared fixtures live in `support.rs`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use vala_bifrost_redux::oracle::{
    BifrostQueryReadDecision, OracleAudit, OracleConfig, QueryOptions, TailTransportDirectory,
    TestPostgresOracleAudit,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{
    AuditDetail, BifrostQueryRequest, FreshnessPolicy, QueryAuditDigest, QueryClass,
    QueryExecutionMode, VisibilityMode,
};

use super::support::*;

/// Creates one valid locked read decision for SQL-audit integration.
fn locked_decision() -> BifrostQueryReadDecision {
    let digest = |value: &str| QueryAuditDigest::new(value).expect("valid audit digest");
    BifrostQueryReadDecision::try_new(AuditDetail::BifrostQueryReadDecision {
        query_digest: digest("sha256:query"),
        query_class: QueryClass::Interactive,
        visibility: VisibilityMode::PublishedOnly,
        binding_digests: vec![digest("sha256:binding")],
        snapshot_digest: digest("sha256:snapshot"),
        manifest_digest: digest("sha256:manifest"),
        projection_digest: digest("sha256:projection"),
        permission_digest: digest("sha256:permission"),
        execution: QueryExecutionMode::Local,
        selected_node_count: 1,
        worker_count: 0,
        slot_units: 1,
        retry_ordinal: 0,
        deadline_ms: 1_000,
    })
    .expect("locked decision")
}

/// Real tenant SQL audit appends the exact locked detail and commits it.
#[tokio::test]
async fn oracle_postgres_audit_commits_locked_read_decision() {
    let fixture = OracleFixture::new("oracle_audit").await;
    let context = fixture.context();
    let audit = TestPostgresOracleAudit::new(fixture.pg.vala_postgres().clone());
    audit
        .append_read_decision(&context, locked_decision())
        .await
        .expect("audit commits");
    let mut conn = fixture
        .pg
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant conn");
    let detail: String = sqlx::query_scalar(
        "SELECT detail::text FROM vala.audit_outbox \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND operation = 'bifrost.query.read_decision'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("audit detail");
    conn.commit().await.expect("audit read commit");
    assert!(detail.contains("bifrost_query_read_decision"));
}

/// A refused audit returns before a pinned missing hot object can be read.
#[tokio::test]
async fn pg_bifrost_oracle_multitenant_isolation_and_fairness_journey_audit_failure_prebyte() {
    let fixture = OracleFixture::new("oracle_audit_gate").await;
    fixture.seed_missing_hot_row().await;
    let oracle = fixture
        .oracle(
            Arc::new(FailingAudit),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let error = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT value FROM {} ORDER BY value", fixture.table.fqn()),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect_err("audit refusal fails before missing object read");
    assert_eq!(error, BifrostError::QueryAuditUnavailable);
    shutdown_oracle(&oracle).await;
}

/// A failed security append aborts the tripwire without emitting a row batch.
#[tokio::test]
async fn pg_bifrost_oracle_multitenant_isolation_and_fairness_journey_tripwire_audit_failure() {
    let fixture = OracleFixture::new("oracle_tripwire_audit_failure").await;
    let _ = fixture.seed_hot_rows(&[(9, DataTenantId::new_v7())]).await;
    let oracle = fixture
        .oracle(
            Arc::new(SecurityFailingAudit {
                reads: TestPostgresOracleAudit::new(fixture.pg.vala_postgres().clone()),
            }),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let error = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT count(*) AS total FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect_err("security audit failure must reject before stream framing");
    assert_eq!(error, BifrostError::QueryAuditUnavailable);
    let mut conn = fixture
        .pg
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant conn");
    let security_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_outbox \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND operation = 'bifrost.query.security_violation'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("security audit count");
    conn.commit().await.expect("security audit read commit");
    assert_eq!(security_events, 0);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// A post-acquisition audit refusal awaits every release, including after one sibling fails.
#[tokio::test]
async fn fused_audit_failure_releases_every_fence_before_return() {
    let fixture = OracleFixture::new("oracle_audit_fence_cleanup").await;
    let tails = Arc::new(TailTransportDirectory::default());
    let releases = Arc::new(AtomicUsize::new(0));
    for (fail_release, writer_epoch) in [(true, 1_u64), (false, 2_u64)] {
        let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
        tails.insert_live_stream(
            fixture.table.fqn(),
            node_id,
            writer_epoch,
            vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
            Arc::new(FenceProbeTransport {
                node_id,
                fail_acquire: false,
                fail_release,
                releases: Arc::clone(&releases),
                batches: Vec::new(),
                page_reads: Arc::new(AtomicUsize::new(0)),
            }),
        );
    }
    let oracle = fixture
        .oracle(Arc::new(FailingAudit), tails, OracleConfig::default())
        .await;
    let error = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT value FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::Fused,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect_err("audit refusal fails the query");
    assert_eq!(error, BifrostError::QueryAuditUnavailable);
    assert_eq!(releases.load(Ordering::SeqCst), 2);
    tokio::task::yield_now().await;
    assert_eq!(releases.load(Ordering::SeqCst), 2);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// Typed Fused acquisition failure records no success decision or provider read.
#[tokio::test]
async fn typed_fused_acquisition_failure_precedes_audit_and_read() {
    let fixture = OracleFixture::new("oracle_typed_acquire_failure").await;
    let tails = Arc::new(TailTransportDirectory::default());
    let releases = Arc::new(AtomicUsize::new(0));
    let page_reads = Arc::new(AtomicUsize::new(0));
    let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
    tails.insert_live_stream(
        fixture.table.fqn(),
        node_id,
        1,
        vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
        Arc::new(FenceProbeTransport {
            node_id,
            fail_acquire: true,
            fail_release: false,
            releases,
            batches: Vec::new(),
            page_reads: Arc::clone(&page_reads),
        }),
    );
    let decisions = Arc::new(AtomicUsize::new(0));
    let oracle = fixture
        .oracle(
            Arc::new(CountingAudit {
                decisions: Arc::clone(&decisions),
                fail: false,
            }),
            tails,
            OracleConfig::default(),
        )
        .await;
    let plan = oracle
        .typed_dataframe(fixture.tenant, &fixture.table.fqn())
        .await
        .expect("typed dataframe")
        .into_optimized_plan()
        .expect("typed plan");
    let error = oracle
        .query_plan(
            fixture.context(),
            plan,
            QueryOptions {
                visibility: VisibilityMode::Fused,
                deadline: Instant::now() + Duration::from_secs(5),
            },
        )
        .await
        .expect_err("typed acquisition fails");
    assert_eq!(error, BifrostError::QueryVisibilityUnavailable);
    assert_eq!(decisions.load(Ordering::SeqCst), 0);
    assert_eq!(page_reads.load(Ordering::SeqCst), 0);
    shutdown_oracle(&oracle).await;
}

/// Typed Fused audit failure releases the pre-acquired complete cut before return.
#[tokio::test]
async fn typed_fused_audit_failure_releases_before_return() {
    let fixture = OracleFixture::new("oracle_typed_audit_failure").await;
    let tails = Arc::new(TailTransportDirectory::default());
    let releases = Arc::new(AtomicUsize::new(0));
    let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
    tails.insert_live_stream(
        fixture.table.fqn(),
        node_id,
        1,
        vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
        Arc::new(FenceProbeTransport {
            node_id,
            fail_acquire: false,
            fail_release: false,
            releases: Arc::clone(&releases),
            batches: Vec::new(),
            page_reads: Arc::new(AtomicUsize::new(0)),
        }),
    );
    let decisions = Arc::new(AtomicUsize::new(0));
    let oracle = fixture
        .oracle(
            Arc::new(CountingAudit {
                decisions: Arc::clone(&decisions),
                fail: true,
            }),
            tails,
            OracleConfig::default(),
        )
        .await;
    let plan = oracle
        .typed_dataframe(fixture.tenant, &fixture.table.fqn())
        .await
        .expect("typed dataframe")
        .into_optimized_plan()
        .expect("typed plan");
    let error = oracle
        .query_plan(
            fixture.context(),
            plan,
            QueryOptions {
                visibility: VisibilityMode::Fused,
                deadline: Instant::now() + Duration::from_secs(5),
            },
        )
        .await
        .expect_err("typed audit refuses query");
    assert_eq!(error, BifrostError::QueryAuditUnavailable);
    assert_eq!(decisions.load(Ordering::SeqCst), 1);
    assert_eq!(releases.load(Ordering::SeqCst), 1);
    tokio::task::yield_now().await;
    assert_eq!(releases.load(Ordering::SeqCst), 1);
    shutdown_oracle(&oracle).await;
}
