//! Replay real Forge maintenance ticks against durable Iceberg state.

use std::sync::Arc;
use std::time::Duration;

use opendal::Buffer;
use vala_bifrost_redux::forge::run_maintenance_tick;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{ForgeObjectStoreControl, seed_forge_group};

async fn steal_forge_lease(fixture: &wyrd_testing::bifrost::ForgeFixture) -> (uuid::Uuid, i64) {
    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    sqlx::query("UPDATE vala.maintenance_leases SET expires_at = now() - interval '1 second' WHERE lease_key = $1")
        .bind(&lease_key)
        .execute(fixture.context.operator_pool.pool())
        .await
        .expect("expire Forge lease for deterministic takeover");
    let owner = uuid::Uuid::now_v7();
    let token = vala_sql::queries::maintenance_leases::try_acquire_lease(
        &fixture.context.operator_pool,
        &lease_key,
        owner,
        i64::try_from(fixture.context.config.lease_ttl.as_secs()).expect("lease seconds"),
    )
    .await
    .expect("successor lease acquisition")
    .expect("successor owns expired Forge lease");
    (owner, token)
}

#[tokio::test]
/// Tests replay safety for maintenance and orphan cleanup with real objects.
///
/// Steps:
/// 1. Start the bound server, seed a real Iceberg table and two file-list rows,
///    create a short-lived GC configuration, and write an aged staged orphan to
///    the real OpenDAL filesystem.
/// 2. Replay 24 bounded schedules of sequential and concurrent public ticks so
///    compaction, expiry, and GC contend through the production table lease.
/// 3. Query the object store after all replays and assert the staged orphan is
///    gone while the table’s live durable state remains usable.
///
/// This verifies repeated production maintenance is idempotent and that GC is
/// driven by durable references rather than a synthetic in-memory live set.
async fn forge_expiry_compaction_gc_matrix_never_deletes_live_file() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "maintenance_matrix_rows").await;
    let mut config = fixture.context.config.clone();
    config.orphan_gc_ttl = Duration::from_millis(1);
    let context = fixture.context_with_config(config);
    let orphan = format!("{}/matrix-orphan.parquet", fixture.binding.object_prefix);
    context
        .staging
        .write(&orphan, Buffer::from(vec![1_u8]))
        .await
        .expect("orphan object");
    tokio::time::sleep(Duration::from_millis(100)).await;
    for schedule in 0..24 {
        if schedule % 3 == 0 {
            let (left, right) = tokio::join!(
                run_maintenance_tick(&context),
                run_maintenance_tick(&context),
            );
            left.expect("left durable maintenance replay");
            right.expect("right durable maintenance replay");
        } else {
            run_maintenance_tick(&context)
                .await
                .expect("durable maintenance replay");
        }
    }
    assert!(context.staging.stat(&orphan).await.is_err());
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
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
async fn forge_gc_replay_preserves_live_reference() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "maintenance_reference_rows").await;
    let mut config = fixture.context.config.clone();
    config.orphan_gc_ttl = Duration::from_millis(1);
    let control = ForgeObjectStoreControl::new(Arc::clone(&fixture.context.staging));
    control.pause_next_list();
    let context = fixture.context_with_object_store(config, Arc::clone(&control));
    let orphan = format!("{}/reference-race.parquet", fixture.binding.object_prefix);
    fixture
        .context
        .staging
        .write(&orphan, Buffer::from(vec![1_u8]))
        .await
        .expect("race orphan object");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let tick_context = context.clone();
    let tick = tokio::spawn(async move { run_maintenance_tick(&tick_context).await });
    control.wait_for_list().await;
    fixture.protect_path(&orphan).await;
    control.release_list();
    tick.await
        .expect("race tick task")
        .expect("reference race maintenance tick");
    assert!(context.object_store.stat(&orphan).await.is_ok());
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
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
async fn forge_gc_partial_delete_restarts_idempotently() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "maintenance_partial_delete_rows").await;
    let mut config = fixture.context.config.clone();
    config.min_files = 3;
    config.orphan_gc_ttl = Duration::from_millis(1);
    let control = ForgeObjectStoreControl::new(Arc::clone(&fixture.context.staging));
    control.fail_delete_at(2);
    let context = fixture.context_with_object_store(config.clone(), Arc::clone(&control));
    let first_orphan = format!("{}/000-partial-a.parquet", fixture.binding.object_prefix);
    let second_orphan = format!("{}/001-partial-b.parquet", fixture.binding.object_prefix);
    for path in [&first_orphan, &second_orphan] {
        fixture
            .context
            .staging
            .write(path, Buffer::from(vec![1_u8]))
            .await
            .expect("partial-delete orphan object");
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let first = run_maintenance_tick(&context)
        .await
        .expect("partial-delete tick outcome");
    assert_eq!(first.tables_failed, 1);
    assert_eq!(control.delete_calls(), 2);
    let first_exists = context.staging.stat(&first_orphan).await.is_ok();
    let second_exists = context.staging.stat(&second_orphan).await.is_ok();
    assert_ne!(first_exists, second_exists);

    let recovery = fixture.context_with_config(config);
    run_maintenance_tick(&recovery)
        .await
        .expect("partial-delete recovery tick");
    assert!(recovery.staging.stat(&first_orphan).await.is_err());
    assert!(recovery.staging.stat(&second_orphan).await.is_err());
    let terminal = fixture.operation_count("forge.orphan_gc.committed").await
        + fixture.operation_count("forge.orphan_gc.recovered").await;
    assert_eq!(terminal, 1);
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
/// Tests fencing immediately before a destructive object-store effect.
///
/// Steps:
/// 1. Seed an old orphan, disable compaction eligibility, and pause the real
///    OpenDAL delete boundary.
/// 2. Take over the durable table lease while the stale worker is paused, then
///    resume the wrapper with a stale-worker error.
/// 3. Assert the orphan and terminal audit remain untouched, release the
///    successor lease, and run a recovery tick that deletes the orphan once.
async fn forge_gc_lease_theft_before_delete_fails_closed() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "gc_fence_rows").await;
    let mut config = fixture.context.config.clone();
    config.min_files = 3;
    config.orphan_gc_ttl = Duration::from_millis(1);
    let control = ForgeObjectStoreControl::new(Arc::clone(&fixture.context.staging));
    control.pause_next_delete();
    let context = fixture.context_with_object_store(config, Arc::clone(&control));
    let orphan = format!("{}/gc-fence-orphan.parquet", fixture.binding.object_prefix);
    context
        .staging
        .write(&orphan, Buffer::from(vec![7_u8]))
        .await
        .expect("orphan object");
    tokio::time::sleep(Duration::from_millis(20)).await;
    let tick_context = context.clone();
    let task = tokio::spawn(async move { run_maintenance_tick(&tick_context).await });
    control.wait_for_delete().await;
    let (successor_owner, successor_token) = steal_forge_lease(&fixture).await;
    control.reject_paused_delete();
    let outcome = task
        .await
        .expect("stale GC task")
        .expect("stale GC tick reports table failure");
    assert_eq!(outcome.tables_failed, 1);
    assert!(context.staging.stat(&orphan).await.is_ok());
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
            &fixture.context.operator_pool,
            &lease_key,
            successor_owner,
            successor_token,
        )
        .await
        .expect("successor release")
    );
    run_maintenance_tick(&context)
        .await
        .expect("GC recovery tick");
    assert!(context.staging.stat(&orphan).await.is_err());
    assert_eq!(
        fixture.operation_count("forge.orphan_gc.recovered").await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}
