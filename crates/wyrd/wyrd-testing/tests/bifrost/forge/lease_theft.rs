//! Lease theft at each point around the catalog commit — every case must
//! fail closed or clean its outputs.
//!
//! Module of the `forge` group; shared fixtures live in `interleaving_support.rs`.

use std::sync::Arc;
use std::time::Duration;
use vala_bifrost_redux::forge::ForgeObjectStore;
use wyrd_testing::bifrost::{CommitUncertaintyCatalog, ForgeObjectStoreControl, seed_forge_group};

use super::interleaving_support::*;

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests fencing immediately after a real Iceberg commit and before SQL
/// bookkeeping.
///
/// Steps:
/// 1. Seed real staged Parquet and file-list rows, then wrap the real catalog
///    so it pauses after accepting the Iceberg commit.
/// 2. Take over the durable table lease while the stale worker is paused and
///    resume the wrapper with a retryable stale-fence response.
/// 3. Assert the stale worker leaves the source rows without a committed
///    snapshot and writes no terminal audit row; release the successor lease
///    and run a normal tick to reconcile the committed snapshot.
async fn forge_compaction_lease_theft_after_catalog_commit_fails_closed() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "compaction_fence_rows").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    control.pause_after_commit();
    let mut lifecycle = SupervisedForge::start(
        &fixture,
        fixture.config.clone(),
        control.clone(),
        Arc::clone(&fixture.object_store),
    );
    lifecycle.hold_next_attempt();
    lifecycle.schedule_once().await;
    tokio::time::timeout(Duration::from_secs(30), control.wait_for_commit())
        .await
        .expect("snapshot expiry reached accepted catalog commit");
    let (successor_owner, successor_token) = steal_forge_lease(&fixture).await;
    control.reject_paused_commit();
    lifecycle.wait_for_held_attempt().await;
    assert_eq!(lifecycle.worker_observer.completed(), 0);
    lifecycle.stop_worker().await;
    lifecycle.shutdown().await;
    let stopped_task = expire_stopped_running_claim(&fixture).await;
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        0
    );
    assert_eq!(
        live_rewrite_state_phase(&fixture).await,
        None,
        "pre-PUT cancellation must not fabricate an open live operation"
    );
    assert!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 AND committed_snapshot_id IS NULL",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("uncommitted source rows")
        == 2
    );
    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &lease_key,
            successor_owner,
            successor_token,
        )
        .await
        .expect("successor release")
    );
    advance_forge_clock_past_uncertainty(&server);
    let mut successor = SupervisedForge::start_default(
        &fixture,
        recovery_config_without_live_replacement(&fixture),
    );
    successor.run_one_success().await;
    assert_eq!(
        successor.worker_observer.completed_tasks(),
        vec![stopped_task]
    );
    successor.shutdown().await;
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.recovered")
            .await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests fencing immediately before a real Iceberg commit.
///
/// The catalog wrapper pauses at the existing `Catalog::update_table` seam,
/// the test takes over the durable lease, and the wrapper rejects the stale
/// commit without delegating it. The source rows remain prepared but no
/// snapshot or terminal bookkeeping is created.
async fn forge_compaction_lease_theft_before_catalog_commit_fails_closed() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "compaction_precommit_fence_rows").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    control.pause_before_commit();
    let mut lifecycle = SupervisedForge::start(
        &fixture,
        fixture.config.clone(),
        control.clone(),
        Arc::clone(&fixture.object_store),
    );
    lifecycle.hold_next_attempt();
    lifecycle.schedule_once().await;
    control.wait_for_before_commit().await;
    let (successor_owner, successor_token) = steal_forge_lease(&fixture).await;
    control.reject_paused_before_commit();
    lifecycle.wait_for_held_attempt().await;
    assert_eq!(lifecycle.worker_observer.completed(), 0);
    lifecycle.stop_worker().await;
    lifecycle.shutdown().await;
    let snapshots = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("precommit catalog table")
        .metadata()
        .snapshots()
        .len();
    assert_eq!(snapshots, 0);
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        0
    );
    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &lease_key,
            successor_owner,
            successor_token,
        )
        .await
        .expect("successor release")
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests that lease loss immediately before an output PUT prevents every
/// stale output and leaves no rewrite-owned object for the successor.
async fn forge_compaction_lease_theft_before_output_put_cleans_rewrite_outputs() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "compaction_output_fence_rows").await;
    let control = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    let barrier = OutputPutBarrier::new(Arc::clone(&control));
    let config = fixture.config.clone();
    let mut lifecycle = SupervisedForge::start(
        &fixture,
        config,
        Arc::clone(&fixture.catalog),
        barrier.clone(),
    );
    lifecycle.hold_next_attempt();
    lifecycle.schedule_once().await;
    tokio::time::timeout(Duration::from_secs(30), barrier.wait_until_reached())
        .await
        .expect("rewrite reached bounded pre-PUT barrier");
    let (successor_owner, successor_token) = steal_forge_lease(&fixture).await;
    barrier.release();
    lifecycle.wait_for_held_attempt().await;
    assert_eq!(lifecycle.worker_observer.completed(), 0);
    lifecycle.stop_worker().await;
    lifecycle.shutdown().await;
    let stopped_task = expire_stopped_running_claim(&fixture).await;
    let entries = control
        .list(&fixture.binding.object_prefix)
        .await
        .expect("list rewrite-owned objects");
    assert!(
        !entries
            .iter()
            .any(|entry| entry.path().contains("/data/forge/")),
        "stale worker left output objects: {entries:?}"
    );
    assert_eq!(
        fixture.operation_count("forge.file_compact.prepared").await,
        0
    );
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        0
    );
    assert!(
        live_rewrite_state_rows(&fixture).await.is_empty(),
        "pre-PUT lease loss must not create an iceberg-rewrite operation-state row"
    );

    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &lease_key,
            successor_owner,
            successor_token,
        )
        .await
        .expect("successor release")
    );
    let mut successor = SupervisedForge::start_default(&fixture, fixture.config.clone());
    successor.run_one_success().await;
    assert_eq!(
        successor.worker_observer.completed_tasks(),
        vec![stopped_task]
    );
    successor.shutdown().await;
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("successor table");
    assert_eq!(table.metadata().snapshots().len(), 1);
    assert_eq!(
        fixture.operation_count("forge.file_compact.prepared").await,
        1
    );
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests expiry fencing after the real snapshot-expiry commit and before its
/// recovered terminal audit append.
///
/// Steps:
/// 1. Create two real Iceberg snapshots through normal Forge compaction, then
///    configure a zero retention window and converge the first maintenance
///    rewrite so its successor task has a newer active watermark.
/// 2. Pause a real catalog response only after expiry removes a snapshot, take over the
///    table lease, and resume with a stale-fence error.
/// 3. Assert no terminal expiry audit is written by the stale worker, then
///    release the successor lease and let the next production tick reconcile
///    the already-applied expiry exactly once.
async fn forge_expiry_lease_theft_before_terminal_audit_fails_closed() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "expiry_fence_rows").await;
    fold_staged_pair_without_live_replacement(&fixture).await;
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    fold_staged_pair_without_live_replacement(&fixture).await;
    advance_forge_clock_past_uncertainty(&server);

    let mut config = fixture.config.clone();
    config.min_files = 3;
    config.max_files_per_bin = 3;
    config.max_files_per_tick = 3;
    config.snapshot_retention = Duration::from_millis(1);
    config.maintenance_trigger_snapshot_count = 1;
    config.maintenance_trigger_interval = Duration::from_nanos(1);
    fixture.append_forge_file(4).await;
    fixture.append_forge_file(5).await;
    fold_staged_pair_without_live_replacement(&fixture).await;
    let table_before_expiry = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("pre-expiry table");
    let snapshots_before_expiry = table_before_expiry.metadata().snapshots().len();
    let current_before_expiry = table_before_expiry.metadata().current_snapshot_id();
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    control.pause_after_snapshot_removal();
    let mut lifecycle = SupervisedForge::start(
        &fixture,
        config,
        control.clone(),
        Arc::clone(&fixture.object_store),
    );
    lifecycle.hold_next_attempt();
    lifecycle.schedule_once().await;
    if tokio::time::timeout(Duration::from_secs(30), control.wait_for_commit())
        .await
        .is_err()
    {
        let tasks: Vec<(String, String)> = sqlx::query_as(
            "SELECT state,strategy FROM vala.forge_tasks WHERE data_tenant_id=$1 ORDER BY created_at,task_id",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_all(fixture.operator_pool.pool())
        .await
        .expect("expiry timeout task diagnostics");
        panic!(
            "snapshot expiry did not reach accepted catalog commit; tasks={tasks:?}; attempts={}; errors={:?}",
            lifecycle.worker_observer.attempts(),
            lifecycle.worker_observer.returned_errors(),
        );
    }
    let (successor_owner, successor_token) = steal_forge_lease(&fixture).await;
    control.reject_paused_commit();
    lifecycle.wait_for_held_attempt().await;
    assert_eq!(lifecycle.worker_observer.completed(), 0);
    lifecycle.stop_worker().await;
    lifecycle.shutdown().await;
    let stopped_task = expire_stopped_running_claim(&fixture).await;
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await,
        0
    );
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.recovered")
            .await,
        0
    );
    let table_after_expiry = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("post-expiry table");
    let snapshots = table_after_expiry.metadata().snapshots().len();
    assert_eq!(
        snapshots,
        snapshots_before_expiry
            .checked_sub(1)
            .expect("expiry fixture retains at least one predecessor"),
        "the accepted expiry commit must remove exactly one eligible snapshot",
    );
    assert!(
        current_before_expiry.is_some_and(|snapshot_id| table_after_expiry
            .metadata()
            .snapshot_by_id(snapshot_id)
            .is_some()),
        "snapshot expiry must retain the active task watermark",
    );
    assert!(
        table_after_expiry
            .metadata()
            .current_snapshot_id()
            .is_some_and(|snapshot_id| table_after_expiry
                .metadata()
                .snapshot_by_id(snapshot_id)
                .is_some()),
        "the post-maintenance current snapshot must remain retained",
    );

    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &lease_key,
            successor_owner,
            successor_token,
        )
        .await
        .expect("successor release")
    );
    let mut successor = SupervisedForge::start_default(
        &fixture,
        recovery_config_without_live_replacement(&fixture),
    );
    successor.run_one_success().await;
    assert_eq!(
        successor.worker_observer.completed_tasks(),
        vec![stopped_task]
    );
    successor.shutdown().await;
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.recovered")
            .await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}
