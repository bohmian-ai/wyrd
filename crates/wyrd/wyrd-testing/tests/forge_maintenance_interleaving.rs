//! Replay real Forge maintenance ticks against durable Iceberg state.

use std::sync::Arc;
use std::time::Duration;

use opendal::Buffer;
use vala_bifrost_redux::forge::ForgeObjectStore;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{
    CommitUncertaintyCatalog, ForgeObjectStoreControl, seed_forge_group,
};

/// Acquire a successor owner after expiring the fixture's table lease.
///
/// The helper models a second worker taking ownership while the first worker
/// is paused at a destructive or terminal-audit boundary.
///
/// # Errors
///
/// Panics when the lease cannot be expired or acquired, because either result
/// means the deterministic takeover precondition was not established.
async fn steal_forge_lease(fixture: &wyrd_testing::bifrost::ForgeFixture) -> (uuid::Uuid, i64) {
async fn steal_forge_lease(fixture: &wyrd_testing::bifrost::ForgeFixture) -> (uuid::Uuid, i64) {
    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    sqlx::query("UPDATE vala.maintenance_leases SET expires_at = now() - interval '1 second' WHERE lease_key = $1")
        .bind(&lease_key)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("expire Forge lease for deterministic takeover");
    let owner = uuid::Uuid::now_v7();
    let token = vala_sql::queries::maintenance_leases::try_acquire_lease(
        &fixture.operator_pool,
        &lease_key,
        owner,
        i64::try_from(fixture.config.lease_ttl.as_secs()).expect("lease seconds"),
    )
    .await
    .expect("successor lease acquisition")
    .expect("successor owns expired Forge lease");
    (owner, token)
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests the list-to-delete GC race with a real durable file-list reference.
///
/// Steps:
/// 1. Seed a real table and age an orphan object in the local OpenDAL store.
/// 2. Pause the real GC list return, insert a durable `vala.file_list` row for
///    that path, and resume the production tick.
/// 3. Assert the object remains present and the durable audit table still has
///    exactly one compaction commit.
///
/// This proves that a reference appearing after the initial live-set/list
/// boundary is observed by the final per-object live-set check before delete.
///
/// # Errors
///
/// The test panics if the server, barriers, durable maintenance tick, or
/// object-store assertions fail. Cancellation is bounded by the test runtime;
/// no detached maintenance task is permitted to outlive the test.
async fn forge_gc_replay_preserves_live_reference() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "maintenance_reference_rows").await;
    let mut config = fixture.config.clone();
    config.orphan_gc_ttl = Duration::from_millis(1);
    let control = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    control.pause_next_list();
    let context = fixture.context_with_object_store(config, Arc::clone(&control));
    let orphan = format!("{}/reference-race.parquet", fixture.binding.object_prefix);
    fixture
        .staging
        .write(&orphan, Buffer::from(vec![1_u8]))
        .await
        .expect("race orphan object");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let tick_context = context.clone();
    let tick = tokio::spawn(async move { tick_context.run_once().await });
    control.wait_for_list().await;
    fixture.protect_path(&orphan).await;
    control.release_list();
    tick.await
        .expect("race tick task")
        .expect("reference race maintenance tick");
    assert!(control.stat(&orphan).await.is_ok());
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
/// Tests partial GC failure and restart recovery with real object-store state.
///
/// Steps:
/// 1. Seed a real table, disable compaction eligibility, and write two aged
///    orphan objects inside its owned prefix.
/// 2. Wrap the real OpenDAL operator so the second delete fails after the first
///    delete has already succeeded; run one public maintenance tick.
/// 3. Restart through a fresh production context, reconcile the prepared GC
///    operation, and assert both objects are absent and one terminal audit
///    record describes the recovered operation.
///
/// The induced failure is a bounded partial batch, not a synthetic state
/// transition. Durable object absence plus the terminal audit proves restart
/// safety, idempotent handling of the already-absent first object, and no
/// dependence on a process-local deletion list.
///
/// # Errors
///
/// The test panics when the injected delete failure, restart tick, or terminal
/// audit assertions do not observe the expected durable state. Restart is
/// synchronous with the test and does not leave a detached task running.
async fn forge_gc_partial_delete_restarts_idempotently() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "maintenance_partial_delete_rows").await;
    let mut config = fixture.config.clone();
    config.min_files = 3;
    config.orphan_gc_ttl = Duration::from_millis(1);
    let control = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    control.fail_delete_at(2);
    let context = fixture.context_with_object_store(config.clone(), Arc::clone(&control));
    let first_orphan = format!("{}/000-partial-a.parquet", fixture.binding.object_prefix);
    let second_orphan = format!("{}/001-partial-b.parquet", fixture.binding.object_prefix);
    for path in [&first_orphan, &second_orphan] {
        fixture
            .staging
            .write(path, Buffer::from(vec![1_u8]))
            .await
            .expect("partial-delete orphan object");
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let first = context
        .run_once()
        .await
        .expect("partial-delete tick outcome");
    assert_eq!(first.tables_failed, 1);
    assert_eq!(control.delete_calls(), 2);
    let first_exists = control.stat(&first_orphan).await.is_ok();
    let second_exists = control.stat(&second_orphan).await.is_ok();
    assert_ne!(first_exists, second_exists);

    let recovery = fixture.context_with_config(config);
    recovery
        .run_once()
        .await
        .expect("partial-delete recovery tick");
    assert!(fixture.staging.stat(&first_orphan).await.is_err());
    assert!(fixture.staging.stat(&second_orphan).await.is_err());
    let terminal = fixture.operation_count("forge.orphan_gc.committed").await
        + fixture.operation_count("forge.orphan_gc.recovered").await;
    assert_eq!(terminal, 1);
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests fencing immediately before a destructive object-store effect.
///
/// Steps:
/// 1. Seed an old orphan, disable compaction eligibility, and pause the real
///    OpenDAL delete boundary.
/// 2. Take over the durable table lease while the stale worker is paused, then
///    resume the wrapper with a stale-worker error.
/// 3. Assert the orphan and terminal audit remain untouched, release the
///    successor lease, and run a recovery tick that deletes the orphan once.
///
/// # Errors
///
/// The test panics when lease takeover, stale-worker fencing, or recovery
/// assertions fail. The paused task is always joined before the successor
/// lease is released, so cancellation cannot escape the test boundary.
async fn forge_gc_lease_theft_before_delete_fails_closed() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "gc_fence_rows").await;
    let mut config = fixture.config.clone();
    config.min_files = 3;
    config.orphan_gc_ttl = Duration::from_millis(1);
    let control = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    control.pause_next_delete();
    let context = fixture.context_with_object_store(config, Arc::clone(&control));
    let orphan = format!("{}/gc-fence-orphan.parquet", fixture.binding.object_prefix);
    fixture
        .staging
        .write(&orphan, Buffer::from(vec![7_u8]))
        .await
        .expect("orphan object");
    tokio::time::sleep(Duration::from_millis(20)).await;
    let tick_context = context.clone();
    let task = tokio::spawn(async move { tick_context.run_once().await });
    control.wait_for_delete().await;
    let (successor_owner, successor_token) = steal_forge_lease(&fixture).await;
    control.reject_paused_delete();
    let outcome = task
        .await
        .expect("stale GC task")
        .expect("stale GC tick reports table failure");
    assert_eq!(outcome.tables_failed, 1);
    assert!(control.stat(&orphan).await.is_ok());
    assert_eq!(
        fixture.operation_count("forge.orphan_gc.committed").await,
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
    context.run_once().await.expect("GC recovery tick");
    assert!(fixture.staging.stat(&orphan).await.is_err());
    assert_eq!(
        fixture.operation_count("forge.orphan_gc.recovered").await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Reconciles snapshot expiry after lease takeover while preserving the live
/// head and the retention-selected snapshot head.
///
/// Two real compaction commits create durable Iceberg snapshots. Expiry is
/// paused after the catalog applies its commit; a successor takes the table
/// lease and fences the stale worker before its terminal audit. The recovery
/// tick then reconciles the already-applied expiry exactly once and confirms
/// that the current snapshot remains readable while the retained head is not
/// removed by replay.
///
/// # Errors
///
/// The test panics if compaction, takeover, fencing, reconciliation, or the
/// snapshot-head assertions fail. The paused expiry task is joined before
/// lease release, so cancellation remains bounded to this test.
async fn forge_expiry_takeover_reconciles_current_and_retained_heads() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "maintenance_expiry_rows").await;

    fixture.forge.run_once().await.expect("first snapshot");
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    fixture.forge.run_once().await.expect("second snapshot");
    let before = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("table before expiry");
    assert_eq!(before.metadata().snapshots().len(), 2);

    let mut config = fixture.config.clone();
    config.snapshot_retention = Duration::from_millis(1);
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    control.pause_after_commit();
    let context = fixture.context_with_catalog(config, control.clone());
    let task = tokio::spawn(async move { context.run_once().await });
    control.wait_for_commit().await;
    let (successor_owner, successor_token) = steal_forge_lease(&fixture).await;
    control.reject_paused_commit();
    let outcome = task
        .await
        .expect("stale expiry task")
        .expect("stale expiry tick reports failure");
    assert_eq!(outcome.tables_failed, 1);
    assert_eq!(fixture.operation_count("forge.snapshot_expire.committed").await, 0);

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
    fixture
        .forge
        .run_once()
        .await
        .expect("expiry recovery tick");
    assert_eq!(fixture.operation_count("forge.snapshot_expire.recovered").await, 1);
    let after = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("table after expiry recovery");
    assert_eq!(after.metadata().snapshots().len(), 1);
    assert!(after.metadata().current_snapshot_id().is_some());
    server.shutdown().await.expect("server shutdown");
}
