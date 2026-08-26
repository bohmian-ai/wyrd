//! Every terminal path — success, timeout, error, drop, fence failure —
//! releases admission and query scratch before returning.
//!
//! Module of the `oracle` group; shared fixtures live in `support.rs`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use vala_bifrost_redux::oracle::{
    OracleConfig, QueryOptions, QueryResourceSnapshot, TailTransportDirectory,
    TestPostgresOracleAudit,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, OracleAdmissionDemand, QueryClass, QueryStreamFrame,
    QueryTerminalOutcome, VisibilityMode,
};

use super::support::*;
use futures_util::StreamExt;

/// Partial concurrent fence acquisition releases every successful sibling.
#[tokio::test]
async fn pg_bifrost_oracle_recovery_terminal_journey_partial_fence_cleanup() {
    let fixture = OracleFixture::new("oracle_partial_fence").await;
    let tails = Arc::new(TailTransportDirectory::default());
    let releases = Arc::new(AtomicUsize::new(0));
    for (fail_acquire, writer_epoch) in [(false, 1_u64), (true, 2_u64)] {
        let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
        tails.insert_live_stream(
            fixture.table.fqn(),
            node_id,
            writer_epoch,
            vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
            Arc::new(FenceProbeTransport {
                node_id,
                fail_acquire,
                fail_release: false,
                releases: Arc::clone(&releases),
                batches: Vec::new(),
                page_reads: Arc::new(AtomicUsize::new(0)),
            }),
        );
    }
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            tails,
            OracleConfig::default(),
        )
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
        .expect_err("one concurrent fence acquisition fails");
    assert_eq!(error, BifrostError::QueryVisibilityUnavailable);
    assert_eq!(releases.load(Ordering::SeqCst), 1);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// A synchronized post-acquisition deadline releases the complete cut before return.
#[tokio::test]
async fn fused_post_acquisition_timeout_releases_before_return() {
    let fixture = OracleFixture::new("oracle_audit_fence_timeout").await;
    let tails = Arc::new(TailTransportDirectory::default());
    let release_polls = Arc::new(AtomicUsize::new(0));
    let release_completions = Arc::new(AtomicUsize::new(0));
    for (block_release, writer_epoch) in [(true, 1_u64), (false, 2_u64)] {
        let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
        tails.insert_live_stream(
            fixture.table.fqn(),
            node_id,
            writer_epoch,
            vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
            Arc::new(CleanupReleaseProbeTransport {
                node_id,
                block_release,
                release_polls: Arc::clone(&release_polls),
                release_completions: Arc::clone(&release_completions),
            }),
        );
    }
    let entered = Arc::new(tokio::sync::Notify::new());
    let oracle = Arc::new(
        fixture
            .oracle(
                Arc::new(BlockingAudit {
                    entered: Arc::clone(&entered),
                }),
                tails,
                OracleConfig::default(),
            )
            .await,
    );
    let query_oracle = Arc::clone(&oracle);
    let context = fixture.context();
    let table = fixture.table.fqn();
    let query = tokio::spawn(async move {
        query_oracle
            .query_sql(
                context,
                BifrostQueryRequest {
                    sql: format!("SELECT value FROM {table}"),
                    visibility: VisibilityMode::Fused,
                    freshness: FreshnessPolicy::Strict,
                    deadline_ms: Some(100),
                },
            )
            .await
    });
    entered.notified().await;
    let error = query
        .await
        .expect("query task joins")
        .expect_err("audit wait reaches query deadline");
    assert_eq!(error, BifrostError::QueryTimeout);
    assert_eq!(release_polls.load(Ordering::SeqCst), 2);
    assert_eq!(release_completions.load(Ordering::SeqCst), 1);
    tokio::task::yield_now().await;
    assert_eq!(release_polls.load(Ordering::SeqCst), 2);
    assert_eq!(release_completions.load(Ordering::SeqCst), 1);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// Typed execution cannot begin while delegated analytical capacity is full.
///
/// # Panics
///
/// Panics when a typed plan reaches audit or provider reads before the
/// background delegated-capacity gate grants one complete unit.
#[tokio::test]
async fn typed_plan_waits_for_delegated_admission_before_execution() {
    let fixture = OracleFixture::new("oracle_typed_delegated_gate").await;
    let blocks =
        vala_sql::queries::oracle_admission::OracleAdmissionBlocks::new(fixture.pg.operator_pool());
    let allocation = blocks
        .allocate(
            OracleAdmissionDemand {
                tenant_id: fixture.tenant,
                principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
                query_class: QueryClass::Analytical,
                requested_units: 4,
                holder_node_id: fixture.role.key.node_id,
                holder_fencing_token: fixture.role.fencing_token,
            },
            chrono::Duration::seconds(10),
        )
        .await
        .expect("analytical capacity allocation");
    assert_eq!(allocation.rows.len(), 3);

    let decisions = Arc::new(AtomicUsize::new(0));
    let page_reads = Arc::new(AtomicUsize::new(0));
    let tails = Arc::new(TailTransportDirectory::default());
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
            releases: Arc::new(AtomicUsize::new(0)),
            batches: Vec::new(),
            page_reads: Arc::clone(&page_reads),
        }),
    );
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
                deadline: Instant::now() + Duration::from_millis(100),
            },
        )
        .await
        .expect_err("full delegated capacity blocks typed execution");
    assert_eq!(error, BifrostError::QueryTimeout);
    assert_eq!(decisions.load(Ordering::SeqCst), 0);
    assert_eq!(page_reads.load(Ordering::SeqCst), 0);
    shutdown_oracle(&oracle).await;
}

/// Typed Fused success commits one decision and produces one successful output.
#[tokio::test]
async fn typed_fused_success_commits_one_decision_and_output() {
    let fixture = OracleFixture::new("oracle_typed_success").await;
    let decisions = Arc::new(AtomicUsize::new(0));
    let tails = Arc::new(TailTransportDirectory::default());
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
            releases: Arc::new(AtomicUsize::new(0)),
            batches: Vec::new(),
            page_reads: Arc::new(AtomicUsize::new(0)),
        }),
    );
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
    let stream = oracle
        .query_plan(
            fixture.context(),
            plan,
            QueryOptions {
                visibility: VisibilityMode::Fused,
                deadline: Instant::now() + Duration::from_secs(5),
            },
        )
        .await
        .expect("typed query starts");
    assert_eq!(
        terminal(stream).await.outcome,
        QueryTerminalOutcome::Success
    );
    assert_eq!(decisions.load(Ordering::SeqCst), 1);
    shutdown_oracle(&oracle).await;
}

/// Typed first-lookahead failure releases its admitted query and disk runtime.
///
/// # Panics
///
/// Panics when the real typed provider/session path does not fail on the
/// tenant tripwire or does not restore exact admission and scratch baselines.
#[cfg(feature = "test-support")]
#[tokio::test]
async fn typed_first_batch_error_releases_admission_and_query_scratch() {
    let fixture = OracleFixture::new("oracle_typed_runtime_cleanup").await;
    let foreign = DataTenantId::new_v7();
    let _ = fixture.seed_hot_rows(&[(1, foreign)]).await;
    let config = OracleConfig {
        interactive_slots: 1,
        analytical_slots: 1,
        single_tenant_ceiling: 1,
        multi_tenant_ceiling: 1,
        ..OracleConfig::default()
    };
    let exact_share = 1024 * 1024 * 1024;
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            config,
        )
        .await;
    let baseline = oracle.runtime_inspection();
    let scratch_baseline = fixture.spill_descendant_count();
    assert_eq!(baseline.active_queries, 0);
    assert_eq!(baseline.reserved_spill_bytes, 0);

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
                visibility: VisibilityMode::PublishedOnly,
                deadline: Instant::now() + Duration::from_secs(5),
            },
        )
        .await
        .expect_err("typed first lookahead must reject the foreign row");
    assert_eq!(error, BifrostError::QueryTenantInvariant);
    let restored = oracle.runtime_inspection();
    assert_eq!(restored.active_queries, baseline.active_queries);
    assert_eq!(restored.reserved_spill_bytes, baseline.reserved_spill_bytes);
    assert_eq!(fixture.spill_descendant_count(), scratch_baseline);
    assert!(exact_share > 0, "configured spill share must be nonzero");
    shutdown_oracle(&oracle).await;
}

/// Dropping a real interactive SQL stream releases its zero-share runtime.
///
/// # Panics
///
/// Panics when SQL does not retain the admitted owner through stream lifetime or
/// when stream drop fails to restore exact admission and scratch baselines.
#[cfg(feature = "test-support")]
#[tokio::test]
async fn sql_stream_drop_releases_admission_and_query_scratch() {
    let fixture = OracleFixture::new("oracle_sql_runtime_cleanup").await;
    let _ = fixture.seed_hot_rows(&[(1, fixture.tenant)]).await;
    let config = OracleConfig {
        interactive_slots: 1,
        analytical_slots: 1,
        single_tenant_ceiling: 1,
        multi_tenant_ceiling: 1,
        ..OracleConfig::default()
    };
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            config,
        )
        .await;
    let baseline = oracle.runtime_inspection();
    let scratch_baseline = fixture.spill_descendant_count();
    assert_eq!(baseline.active_queries, 0);
    assert_eq!(baseline.reserved_spill_bytes, 0);

    let stream = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT * FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect("real SQL stream must start");
    let active = oracle.runtime_inspection();
    assert_eq!(active.active_queries, 1);
    assert_eq!(active.reserved_spill_bytes, 1024 * 1024 * 1024);
    assert_eq!(fixture.spill_descendant_count(), scratch_baseline);

    drop(stream);
    let restored = oracle.runtime_inspection();
    assert_eq!(restored.active_queries, baseline.active_queries);
    assert_eq!(restored.reserved_spill_bytes, baseline.reserved_spill_bytes);
    assert_eq!(fixture.spill_descendant_count(), scratch_baseline);
    shutdown_oracle(&oracle).await;
}

/// Dropping a real typed stream releases its exact spill grant and query scratch.
///
/// # Panics
///
/// Panics when typed execution bypasses the admitted spill share or when stream
/// drop fails to restore exact admission and scratch baselines.
#[cfg(feature = "test-support")]
#[tokio::test]
async fn typed_stream_drop_releases_admission_and_query_scratch() {
    let fixture = OracleFixture::new("oracle_typed_stream_cleanup").await;
    let _ = fixture.seed_hot_rows(&[(1, fixture.tenant)]).await;
    let config = OracleConfig {
        interactive_slots: 1,
        analytical_slots: 1,
        single_tenant_ceiling: 1,
        multi_tenant_ceiling: 1,
        ..OracleConfig::default()
    };
    let exact_share = 1024 * 1024 * 1024;
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            config,
        )
        .await;
    let baseline = oracle.runtime_inspection();
    let scratch_baseline = fixture.spill_descendant_count();
    let plan = oracle
        .typed_dataframe(fixture.tenant, &fixture.table.fqn())
        .await
        .expect("typed dataframe")
        .into_optimized_plan()
        .expect("typed plan");
    let stream = oracle
        .query_plan(
            fixture.context(),
            plan,
            QueryOptions {
                visibility: VisibilityMode::PublishedOnly,
                deadline: Instant::now() + Duration::from_secs(5),
            },
        )
        .await
        .expect("real typed stream must start");
    let active = oracle.runtime_inspection();
    assert_eq!(active.active_queries, 1);
    assert_eq!(active.reserved_spill_bytes, exact_share);
    assert_eq!(
        fixture.spill_descendant_count(),
        scratch_baseline,
        "a fully materialized first batch must not retain idle query scratch"
    );

    drop(stream);
    let restored = oracle.runtime_inspection();
    assert_eq!(restored.active_queries, baseline.active_queries);
    assert_eq!(restored.reserved_spill_bytes, baseline.reserved_spill_bytes);
    assert_eq!(fixture.spill_descendant_count(), scratch_baseline);
    shutdown_oracle(&oracle).await;
}

/// Remote-style release failure is awaited and `Drop` schedules no second release.
#[tokio::test]
async fn remote_fence_release_failure_is_observed_without_drop_spawn() {
    let fixture = OracleFixture::new("oracle_release_failure").await;
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
            fail_release: true,
            releases: Arc::clone(&releases),
            batches: Vec::new(),
            page_reads: Arc::new(AtomicUsize::new(0)),
        }),
    );
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            tails,
            OracleConfig::default(),
        )
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
        .expect_err("failed release is observed before query returns");
    assert_eq!(error, BifrostError::QueryVisibilityUnavailable);
    assert_eq!(releases.load(Ordering::SeqCst), 1);
    tokio::task::yield_now().await;
    assert_eq!(releases.load(Ordering::SeqCst), 1);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// A successful terminal releases local resources before success is observed.
///
/// # Panics
///
/// Panics when the production query cannot start, emits a non-success terminal,
/// or fails to release its local resource owner.
#[tokio::test]
async fn oracle_terminal_success_releases_local_resources_before_success() {
    let fixture = OracleFixture::new("oracle_terminal_local_release").await;
    let _ = fixture.seed_hot_rows(&[(1, fixture.tenant)]).await;
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let mut stream = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT * FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect("production Oracle stream");
    let probe = stream.resource_probe_for_test();
    let mut terminal_frame = None;
    while let Some(frame) = stream.frames.next().await {
        match frame.expect("query frame") {
            QueryStreamFrame::Schema(_) | QueryStreamFrame::Batch(_) => {}
            QueryStreamFrame::Terminal(frame) => {
                assert!(terminal_frame.is_none(), "query emitted duplicate terminal");
                terminal_frame = Some(frame);
                break;
            }
        }
    }
    let terminal = terminal_frame.expect("query stream ended without terminal");
    assert_eq!(terminal.outcome, QueryTerminalOutcome::Success);
    assert_eq!(probe.snapshot(), QueryResourceSnapshot::default());
    if let Some(frame) = stream.frames.next().await {
        panic!("unexpected post-terminal frame: {frame:?}");
    }
    drop(stream);
    assert_eq!(probe.snapshot(), QueryResourceSnapshot::default());
    drop(probe);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}
