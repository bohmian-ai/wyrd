//! Commit-window reconciliation: periodic and hinted work serialize without
//! a false terminal audit.
//!
//! Module of the `forge` group; shared fixtures live in `interleaving_support.rs`.

use std::sync::Arc;
use std::time::Duration;
use vala_bifrost_redux::maintenance::{StagingFileCommitted, StagingPublishOutcome};
use wyrd_testing::bifrost::{
    CommitUncertaintyCatalog, seed_forge_group, seed_forge_group_for_tenant,
};

use super::interleaving_support::*;

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge incremental interleaving lane"]
/// Replays competing hinted/periodic-shaped one-shot ticks against one real
/// table and then verifies lease theft fails closed before a successor tick
/// converges the durable state.
///
/// # Errors
///
/// The journey fails when durable lease, SQL, or Iceberg operations diverge
/// from the production Forge contract.
async fn periodic_and_hints_serialize_without_false_terminal_audit() {
    let hinted_server = start_engine_fixture_server().await;
    let hinted_tenant = hinted_server
        .seed_tenant("hint-periodic")
        .await
        .expect("hinted fixture tenant");
    let hinted =
        seed_forge_group_for_tenant(&hinted_server, hinted_tenant, "hint_periodic_rows").await;
    let hint_control = CommitUncertaintyCatalog::new(Arc::clone(&hinted.catalog));
    hint_control.pause_before_commit();
    let mut hinted_lifecycle = SupervisedForge::start(
        &hinted,
        hinted.config.clone(),
        hint_control.clone(),
        Arc::clone(&hinted.object_store),
    );
    hinted_lifecycle.hold_next_attempt();
    let day = vala_bifrost_redux::catalog::layout::TimePartition::new(
        vala_bifrost_redux::catalog::layout::TimeGranularity::Day,
        chrono::NaiveDate::from_ymd_opt(2026, 7, 14)
            .expect("partition day")
            .and_hms_opt(0, 0, 0)
            .expect("midnight")
            .and_utc(),
    )
    .expect("midnight is a daily partition boundary");
    assert_eq!(
        hinted_lifecycle
            .publisher
            .try_publish(StagingFileCommitted::new(hinted.binding.clone(), day)),
        StagingPublishOutcome::Published
    );
    hinted_lifecycle.schedule_once().await;
    tokio::time::timeout(
        Duration::from_secs(30),
        hint_control.wait_for_before_commit(),
    )
    .await
    .expect("hinted pre-acceptance catalog boundary");
    hinted_lifecycle.worker_stop.cancel();
    tokio::time::timeout(
        Duration::from_secs(3),
        hint_control.wait_for_before_commit_drop(),
    )
    .await
    .expect("hint cancellation must drop the paused catalog call");
    hinted_lifecycle.wait_for_held_attempt().await;
    hinted_lifecycle.stop_worker().await;
    hinted_lifecycle.shutdown().await;
    let stopped_task = expire_stopped_running_claim(&hinted).await;
    assert_eq!(
        hinted.operation_count("forge.file_compact.committed").await,
        0
    );
    let mut recovery = SupervisedForge::start(
        &hinted,
        recovery_config_without_live_replacement(&hinted),
        hint_control,
        Arc::clone(&hinted.object_store),
    );
    recovery.hold_next_attempt();
    recovery.schedule_once().await;
    recovery.wait_for_held_attempt().await;
    assert_eq!(recovery.worker_observer.attempts(), 1);
    assert_eq!(recovery.worker_observer.completed(), 0);
    assert!(recovery.worker_observer.completed_tasks().is_empty());
    assert!(!recovery.worker_observer.returned_errors().is_empty());
    recovery.stop_worker().await;
    recovery.shutdown().await;
    let settled_retry: (uuid::Uuid, String, i32, Option<String>) = sqlx::query_as(
        "SELECT task_id,state,attempt_count,failure_class FROM vala.forge_tasks WHERE data_tenant_id = $1 AND task_id = $2",
    )
    .bind(hinted.tenant.as_uuid())
    .bind(stopped_task)
    .fetch_one(hinted.operator_pool.pool())
    .await
    .expect("hinted successor settles the failed recovery attempt with bounded retry state");
    assert_eq!(settled_retry.0, stopped_task);
    assert_eq!(settled_retry.1, "retryable");
    assert_eq!(settled_retry.2, 1);
    assert_eq!(settled_retry.3.as_deref(), Some("transient_object_store"));
    assert_eq!(
        hinted.operation_count("forge.file_compact.committed").await,
        0
    );
    assert_eq!(
        hinted.operation_count("forge.file_compact.recovered").await,
        0
    );
    assert_eq!(
        hinted
            .catalog
            .load_table(&hinted.binding.table_ident())
            .await
            .expect("hinted cancellation table")
            .metadata()
            .snapshots()
            .len(),
        0
    );
    hinted_server
        .shutdown()
        .await
        .expect("hinted server shutdown");

    let periodic_server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&periodic_server, "incremental_interleaving_rows").await;
    let lifecycle = SupervisedForge::start_default(&fixture, fixture.config.clone());
    let expected = lifecycle.worker_observer.completed().saturating_add(1);
    lifecycle.schedule_once().await;
    lifecycle.schedule_once().await;
    tokio::time::timeout(
        Duration::from_secs(30),
        lifecycle.worker_observer.wait_for_at_least(expected),
    )
    .await
    .expect("serialized worker completion bound");
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        1
    );

    let (owner, token) = steal_forge_lease(&fixture).await;
    lifecycle.schedule_once().await;
    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &lease_key,
            owner,
            token,
        )
        .await
        .expect("release interleaving lease")
    );
    lifecycle.schedule_once().await;
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        1
    );
    lifecycle.shutdown().await;
    periodic_server
        .shutdown()
        .await
        .expect("periodic server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Exercises cancellation on both sides of the Iceberg commit acceptance
/// boundary and proves the successor preserves the corresponding durable state.
///
/// The first fixture pauses immediately before catalog acceptance.  The stop
/// token is then cancelled while the catalog future is held, and the fixture
/// proves cancellation dropped that future before the supervised scheduler
/// returns within a bound.
/// The second fixture repeats the sequence after the real catalog has accepted
/// the commit but before its response reaches Forge. The pre-acceptance path
/// remains prepared during the uncertainty window; the post-acceptance path
/// reconciles the already-created snapshot. Both release the table lease and
/// never create a duplicate terminal event.
///
/// # Errors
///
/// The journey fails when the real Wyrd server, SQL lease, Iceberg catalog, or
/// audit transitions do not satisfy the cancellation and reconciliation
/// contract. The test proves cancellation wins at the controlled catalog
/// boundary before it bounds the scheduler join.
async fn forge_commit_cancellation_windows_reconcile_once() {
    let server = start_engine_fixture_server().await;

    let before = seed_forge_group(&server, "cancel_before_acceptance").await;
    let before_control = CommitUncertaintyCatalog::new(Arc::clone(&before.catalog));
    before_control.pause_before_commit();
    let mut before_lifecycle = SupervisedForge::start(
        &before,
        before.config.clone(),
        before_control.clone(),
        Arc::clone(&before.object_store),
    );
    before_lifecycle.hold_next_attempt();
    let day = vala_bifrost_redux::catalog::layout::TimePartition::new(
        vala_bifrost_redux::catalog::layout::TimeGranularity::Day,
        chrono::NaiveDate::from_ymd_opt(2026, 7, 14)
            .expect("partition day")
            .and_hms_opt(0, 0, 0)
            .expect("midnight")
            .and_utc(),
    )
    .expect("midnight is a daily partition boundary");
    assert_eq!(
        before_lifecycle
            .publisher
            .try_publish(StagingFileCommitted::new(before.binding.clone(), day)),
        StagingPublishOutcome::Published
    );
    before_lifecycle.schedule_once().await;
    tokio::time::timeout(
        Duration::from_secs(30),
        before_control.wait_for_before_commit(),
    )
    .await
    .expect("before-acceptance catalog boundary");
    before_lifecycle.worker_stop.cancel();
    tokio::time::timeout(
        Duration::from_secs(3),
        before_control.wait_for_before_commit_drop(),
    )
    .await
    .expect("before-acceptance cancellation must drop the paused catalog call");
    before_lifecycle.wait_for_held_attempt().await;
    before_lifecycle.stop_worker().await;
    before_lifecycle.shutdown().await;
    let _before_task = expire_stopped_running_claim(&before).await;
    assert_eq!(
        before.operation_count("forge.file_compact.prepared").await,
        1
    );
    assert_eq!(
        before.operation_count("forge.file_compact.committed").await,
        0
    );
    let mut before_successor = SupervisedForge::start(
        &before,
        recovery_config_without_live_replacement(&before),
        before_control,
        Arc::clone(&before.object_store),
    );
    before_successor.hold_next_attempt();
    before_successor.schedule_once().await;
    before_successor.wait_for_held_attempt().await;
    assert_eq!(before_successor.worker_observer.completed(), 0);
    assert_eq!(before_successor.worker_observer.attempts(), 1);
    before_successor.stop_worker().await;
    before_successor.shutdown().await;
    assert_eq!(
        before.operation_count("forge.file_compact.committed").await,
        0
    );
    assert_eq!(
        before.operation_count("forge.file_compact.recovered").await,
        0
    );
    assert_eq!(
        before
            .catalog
            .load_table(&before.binding.table_ident())
            .await
            .expect("before-acceptance table")
            .metadata()
            .snapshots()
            .len(),
        0
    );

    let after_tenant = server
        .seed_tenant("cancel-after-acceptance")
        .await
        .expect("post-acceptance fixture tenant");
    let after = seed_forge_group_for_tenant(&server, after_tenant, "cancel_after_acceptance").await;
    let after_control = CommitUncertaintyCatalog::new(Arc::clone(&after.catalog));
    after_control.pause_after_commit();
    let mut after_lifecycle = SupervisedForge::start(
        &after,
        after.config.clone(),
        after_control.clone(),
        Arc::clone(&after.object_store),
    );
    after_lifecycle.hold_next_attempt();
    assert_eq!(
        after_lifecycle
            .publisher
            .try_publish(StagingFileCommitted::new(after.binding.clone(), day)),
        StagingPublishOutcome::Published
    );
    after_lifecycle.schedule_once().await;
    if tokio::time::timeout(Duration::from_secs(30), after_control.wait_for_commit())
        .await
        .is_err()
    {
        let tasks: Vec<(uuid::Uuid, String, String, i64, i64)> = sqlx::query_as(
            "SELECT data_tenant_id, state, strategy, estimated_files, estimated_bytes \
             FROM vala.forge_tasks ORDER BY created_at, task_id",
        )
        .fetch_all(after.operator_pool.pool())
        .await
        .expect("after-acceptance task diagnostics");
        panic!(
            "after-acceptance catalog boundary: attempts={}, completed={}, tasks={tasks:?}",
            after_lifecycle.worker_observer.attempts(),
            after_lifecycle.worker_observer.completed(),
        );
    }
    after_lifecycle.worker_stop.cancel();
    tokio::time::timeout(
        Duration::from_secs(3),
        after_control.wait_for_after_commit_drop(),
    )
    .await
    .expect("after-acceptance cancellation must drop the paused catalog call");
    after_lifecycle.wait_for_held_attempt().await;
    after_lifecycle.stop_worker().await;
    after_lifecycle.shutdown().await;
    let after_task = expire_stopped_running_claim(&after).await;
    assert_eq!(
        after.operation_count("forge.file_compact.prepared").await,
        1
    );
    advance_forge_clock_past_uncertainty(&server);
    let mut after_successor =
        SupervisedForge::start_default(&after, recovery_config_without_live_replacement(&after));
    after_successor.hold_next_attempt();
    after_successor.schedule_once().await;
    after_successor.wait_for_held_attempt().await;
    assert_eq!(after_successor.worker_observer.attempts(), 1);
    assert!(after_successor.worker_observer.returned_errors().is_empty());
    after_successor.stop_worker().await;
    assert_eq!(after_successor.worker_observer.completed(), 1);
    assert_eq!(
        after_successor.worker_observer.completed_tasks(),
        vec![after_task]
    );
    assert_eq!(
        after_successor.worker_observer.completed_strategies(),
        vec![vala_sql::row_types::forge_tasks::ForgeClaimStrategy::Known(
            vala_sql::row_types::forge_tasks::ForgeTaskStrategy::StagingFold,
        )]
    );
    after_successor.shutdown().await;
    assert_eq!(
        after.operation_count("forge.file_compact.committed").await,
        0
    );
    assert_eq!(
        after.operation_count("forge.file_compact.recovered").await,
        1
    );
    assert_eq!(
        after
            .catalog
            .load_table(&after.binding.table_ident())
            .await
            .expect("after-acceptance table")
            .metadata()
            .snapshots()
            .len(),
        1
    );
    server.shutdown().await.expect("server shutdown");
}
