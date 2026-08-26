//! Publication recovery and orphan cleanup after panic, including
//! pre-prepared verified outputs.
//!
//! Module of the `forge` group; shared fixtures live in `support.rs`.

use iceberg::transaction::{ApplyTransactionAction, Transaction};
use opendal::Buffer;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::forge::{
    ForgeConfig, ForgeLease, ForgeWorker, ForgeWorkerCompletionObserver, ForgeWorkerConfig,
};
use vala_bifrost_redux::maintenance::{StagingFileCommitted, StagingPublishOutcome};
use vala_sql::row_types::forge_tasks::{ForgeClaimStrategy, ForgeTaskStrategy};
use wyrd_server::config::BifrostTarget;
use wyrd_spec::vala::api::{AuditDetail, ForgeIcebergRewritePhase, StoragePath};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::ForgeObjectStoreControl;
use wyrd_testing::bifrost::forge_harness::seed_forge_group;

use super::support::*;

/// Appends public batches behind one tenant flush so Forge observes the complete group.
async fn append_native_forge_group_with_rows(
    server: &WyrdTestServer,
    fixture: &wyrd_testing::bifrost::ForgeFixture,
    sequences: impl IntoIterator<Item = i64>,
    rows_per_batch: usize,
) {
    append_native_forge_cycles_with_flush_mode(server, fixture, sequences, rows_per_batch, false)
        .await;
}

/// Proves production supervision fails stop on Forge panics and restart recovers once.
///
/// # Panics
///
/// Panics when the shared panic-recovery scenario violates its supervision,
/// durable recovery, publication, or resource-settlement invariants.
#[tokio::test]
#[ignore = "gated journey: real supervised server panic and lease-expiry recovery"]
async fn supervised_forge_panic_fails_stop_and_recovers_exactly_once() {
    supervised_forge_panic_recovery_scenario().await;
}

/// Runs real orphan collection and proves an aged, unreferenced object is deleted.
///
/// # Panics
///
/// Panics when the production maintenance path does not complete SnapshotExpiry
/// and its coupled orphan collector, retain the object, or omit its terminal
/// orphan-GC audit.
#[tokio::test]
#[ignore = "gated journey: real Postgres, Forge maintenance, and object cleanup"]
async fn forge_orphan_cleanup_journey() {
    forge_orphan_cleanup_lifecycle_journey().await;
}

/// Drives one reset generation through protected production orphan collection.
///
/// # Panics
///
/// Panics when an aged reset generation is not reclaimed exclusively through
/// maintenance or when its terminal orphan audit is absent.
async fn forge_orphan_cleanup_lifecycle_journey() {
    let server = start_maintenance_journey_server().await;
    let fixture = native_forge_group(&server, "journey_orphan_cleanup").await;
    commit_journey_staging_snapshot(&server, &fixture).await;
    let (input_path, partition_granularity, partition_start): (
        String,
        String,
        chrono::DateTime<chrono::Utc>,
    ) = sqlx::query_as(
        "SELECT file_path, partition_granularity, partition_start FROM vala.file_list \
         WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 \
         AND committed_snapshot_id IS NOT NULL ORDER BY id LIMIT 1",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("journey live input path");
    let partition = vala_bifrost_redux::catalog::layout::TimePartition::from_durable_columns(
        &partition_granularity,
        partition_start,
    )
    .expect("durable file-list rows carry an exact time partition");
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("journey reset table");
    let base_snapshot_id = table
        .metadata()
        .current_snapshot_id()
        .expect("journey reset base snapshot");
    let operation_id = uuid::Uuid::now_v7();
    let orphan = format!(
        "{}/data/forge/bifrost-writer-v1/{operation_id}-00000.parquet",
        fixture.binding.object_prefix,
    );
    fixture
        .staging
        .write(&orphan, Buffer::from(vec![9_u8]))
        .await
        .expect("journey reset generation object");
    let resource = format!(
        "bifrost://{}/{}/{}",
        fixture.tenant, fixture.binding.table_ref.namespace, fixture.binding.table_ref.name,
    );
    let detail = AuditDetail::ForgeIcebergRewrite {
        operation_id,
        phase: ForgeIcebergRewritePhase::Prepared,
        group: resource,
        base_snapshot_id,
        committed_snapshot_id: None,
        partition_spec_id: table.metadata().default_partition_spec_id(),
        time_partition: partition.to_wire(),
        target_file_size_bytes: 1,
        input_paths: vec![StoragePath::new(input_path).expect("journey live input storage path")],
        output_paths: vec![
            StoragePath::new(orphan.clone()).expect("journey reset output storage path"),
        ],
        writer_recipe_version: "bifrost-writer-v1".to_owned(),
    };
    let object_store = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    let mut config = fixture.config.clone();
    config.orphan_gc_ttl = Duration::from_millis(1);
    let forge = fixture.context_with_object_store(config, Arc::clone(&object_store));
    let lease_key = vala_bifrost_redux::forge::forge_lease_key(
        fixture.tenant,
        &fixture.binding.logical_namespace,
        &fixture.binding.table_name,
    );
    let mut lease = ForgeLease::acquire(
        &fixture.operator_pool,
        lease_key,
        uuid::Uuid::now_v7(),
        fixture.config.lease_ttl,
    )
    .await
    .expect("journey reset lease query")
    .expect("journey reset lease");
    forge
        .append_prepared_live_rewrite_for_test(&mut lease, &fixture.binding, partition, detail)
        .await
        .expect("journey Prepared live rewrite");
    let reconcile_now = chrono::Utc::now()
        + chrono::Duration::from_std(fixture.config.uncertainty_bound)
            .expect("journey uncertainty bound")
        + chrono::Duration::seconds(1);
    let reconciled = forge
        .reconcile_live_replacements_for_test(
            &mut lease,
            &fixture.binding,
            &CancellationToken::new(),
            reconcile_now,
        )
        .await
        .expect("journey Reset reconciliation");
    assert_eq!(
        reconciled.reset, 1,
        "stable Prepared output must Reset once"
    );
    assert!(
        object_store.delete_paths().is_empty(),
        "Reset reconciliation must enqueue without remote deletion"
    );
    assert!(
        fixture.staging.stat(&orphan).await.is_ok(),
        "Reset output remains durable for delayed orphan GC"
    );
    lease
        .release(&fixture.operator_pool)
        .await
        .expect("journey reset lease release");
    let modified = fixture
        .staging
        .stat(&orphan)
        .await
        .expect("journey orphan metadata")
        .last_modified()
        .expect("journey orphan modification time")
        .into_inner()
        .as_millisecond();
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(modified + 100)
                .expect("journey orphan timestamp is UTC-representable"),
        )
        .expect("advance journey orphan clock");
    assert_eq!(
        forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("aged reset orphan-GC pass"),
        1,
        "orphan GC must delete the aged Reset generation once"
    );
    assert!(fixture.staging.stat(&orphan).await.is_err());
    assert_eq!(
        object_store
            .delete_paths()
            .iter()
            .filter(|path| path.as_str() == orphan.as_str())
            .count(),
        1,
        "orphan GC is the sole exact-once physical-delete owner"
    );
    assert_eq!(
        forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("idempotent reset orphan-GC pass"),
        0,
        "successor orphan GC must not repeat deletion"
    );
    assert_eq!(
        fixture.operation_count("forge.orphan_gc.committed").await
            + fixture.operation_count("forge.orphan_gc.recovered").await,
        1,
        "one terminal orphan-GC audit must settle the deletion"
    );
    server
        .shutdown()
        .await
        .expect("journey orphan server shutdown");
}

/// Proves a verified output whose `Prepared` audit fails is reclaimed only by orphan GC.
///
/// The production rewrite is held immediately after the shared uploader has
/// verified a real object. The injected audit failure then leaves that object
/// without a `Prepared` or Reset projection. Production orphan GC first
/// protects it below the TTL floor, then deletes it under the exact table lease
/// after the server-owned clock makes it eligible.
///
/// # Panics
///
/// Panics when the verified upload is deleted by the rewrite path, becomes
/// eligible before its TTL, survives an eligible production GC pass, is
/// deleted more than once, or lacks exactly one terminal orphan-GC audit.
#[tokio::test]
#[ignore = "supporting integration seam: direct owner controls"]
async fn supporting_preprepared_verified_output_orphan_gc_journey() {
    let server_config = vala_bifrost_redux::forge::ForgeConfig {
        min_files: 2,
        max_files_per_bin: 2,
        max_files_per_tick: 2,
        ..vala_bifrost_redux::forge::ForgeConfig::default()
    };
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_process_role_for_test(BifrostTarget::All)
        .with_forge_config_for_test(server_config)
        .start_bound()
        .await
        .expect("bound pre-Prepared orphan journey server");
    let fixture = native_forge_group(&server, "journey_prepared_audit_failure_gc").await;
    commit_journey_staging_snapshot(&server, &fixture).await;
    append_native_forge_cycles(&server, &fixture, 3..5).await;
    commit_journey_staging_snapshot(&server, &fixture).await;

    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("age committed files beyond the live-rewrite cutoff");
    let (partition_granularity, partition_start): (String, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as(
            "SELECT partition_granularity, partition_start FROM vala.file_list \
             WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 \
             AND committed_snapshot_id IS NOT NULL ORDER BY partition_start LIMIT 1",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("live-rewrite partition cutoff");
    let partition_day = vala_bifrost_redux::catalog::layout::TimePartition::from_durable_columns(
        &partition_granularity,
        partition_start,
    )
    .expect("durable file-list rows carry an exact time partition");
    assert_eq!(
        server
            .forge_publisher()
            .try_publish(StagingFileCommitted::new(
                fixture.binding.clone(),
                partition_day,
            )),
        StagingPublishOutcome::Published,
        "production demand channel must accept the live-rewrite hint"
    );

    let object_store = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    let mut rewrite_config = fixture.config.clone();
    rewrite_config.min_files = 2;
    rewrite_config.max_files_per_bin = 2;
    rewrite_config.max_files_per_tick = 2;
    let forge = fixture.context_with_object_store(rewrite_config, Arc::clone(&object_store));
    let completed_passes = server.completed_forge_scheduler_passes_for_test();
    server.request_forge_scheduler_pass_for_test();
    server
        .wait_for_forge_scheduler_passes_for_test(completed_passes + 1)
        .await;
    let queued: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM vala.forge_tasks \
         WHERE data_tenant_id=$1 AND namespace_name=$2 AND table_name=$3 \
         AND strategy='live_rewrite' AND state IN ('ready','retryable'))",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("observe server-owned live-rewrite enqueue");
    let planner_diagnostics: Vec<(String, String)> = sqlx::query_as(
        "SELECT strategy,state FROM vala.forge_tasks WHERE data_tenant_id=$1 ORDER BY created_at",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("read production planner diagnostics");
    assert!(
        queued,
        "production planner must enqueue the live rewrite: {planner_diagnostics:?}"
    );
    forge.fail_next_prepared_live_audit_for_test();
    object_store.pause_after_next_output_put();
    let worker = ForgeWorker::new(
        Arc::clone(&forge),
        ForgeWorkerConfig::default(),
        uuid::Uuid::now_v7(),
    )
    .expect("pre-Prepared failure worker");
    let attempt =
        tokio::spawn(async move { worker.execute_one_for_test(&CancellationToken::new()).await });
    tokio::time::timeout(Duration::from_secs(30), object_store.wait_for_output_put())
        .await
        .expect("verified output PUT boundary");
    let orphan = object_store
        .last_output_path()
        .expect("verified output path");
    assert!(
        fixture.staging.stat(&orphan).await.is_ok(),
        "shared uploader must leave verified bytes durable"
    );
    object_store.release_output_put();
    assert!(
        tokio::time::timeout(Duration::from_secs(30), attempt)
            .await
            .expect("pre-Prepared failure worker bound")
            .expect("pre-Prepared failure worker join")
            .is_err(),
        "the injected Prepared audit failure must remain authoritative"
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        0,
        "the failure must precede durable Prepared evidence"
    );
    assert!(
        object_store.delete_paths().is_empty(),
        "the rewrite path must leave remote deletion exclusively to orphan GC"
    );
    assert_eq!(
        forge
            .gc_eligibility_for_test(&fixture.binding, &orphan)
            .await
            .expect("young pre-Prepared eligibility"),
        "TooYoung",
        "the TTL floor protects a freshly verified generation"
    );
    assert_eq!(
        forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("young pre-Prepared GC pass"),
        0,
        "orphan GC must not delete below the TTL floor"
    );
    assert!(
        fixture.staging.stat(&orphan).await.is_ok(),
        "the young verified generation must remain durable"
    );

    let modified = fixture
        .staging
        .stat(&orphan)
        .await
        .expect("pre-Prepared orphan metadata")
        .last_modified()
        .expect("pre-Prepared orphan modification time")
        .into_inner()
        .as_millisecond();
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(modified + 100)
                .expect("pre-Prepared orphan timestamp is UTC-representable"),
        )
        .expect("advance pre-Prepared orphan clock");
    let mut gc_config = fixture.config.clone();
    gc_config.orphan_gc_ttl = Duration::from_millis(1);
    let gc_forge = fixture.context_with_object_store(gc_config, Arc::clone(&object_store));
    assert_eq!(
        gc_forge
            .gc_eligibility_for_test(&fixture.binding, &orphan)
            .await
            .expect("aged pre-Prepared eligibility"),
        "Eligible"
    );
    let terminal_before = fixture.operation_count("forge.orphan_gc.committed").await
        + fixture.operation_count("forge.orphan_gc.recovered").await;
    assert!(
        gc_forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("aged pre-Prepared GC pass")
            >= 1,
        "production orphan GC must reclaim the aged generation"
    );
    assert!(
        fixture.staging.stat(&orphan).await.is_err(),
        "the aged pre-Prepared generation must be absent"
    );
    assert_eq!(
        object_store
            .delete_paths()
            .iter()
            .filter(|path| path.as_str() == orphan.as_str())
            .count(),
        1,
        "the exact generation must be deleted once"
    );
    let terminal_after = fixture.operation_count("forge.orphan_gc.committed").await
        + fixture.operation_count("forge.orphan_gc.recovered").await;
    assert_eq!(
        terminal_after,
        terminal_before + 1,
        "one orphan-GC terminal audit must settle the deletion"
    );
    assert_eq!(
        gc_forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("idempotent pre-Prepared GC pass"),
        0,
        "a successor pass must not repeat the deletion"
    );
    assert_eq!(
        object_store
            .delete_paths()
            .iter()
            .filter(|path| path.as_str() == orphan.as_str())
            .count(),
        1,
        "successor GC must preserve exactly-once deletion"
    );
    server
        .shutdown()
        .await
        .expect("pre-Prepared orphan server shutdown");
}

/// Proves ambiguous publication recovery and protected orphan GC converge once.
///
/// # Panics
///
/// Panics when the production recovery path duplicates a catalog update or
/// rows, or when protected maintenance fails to reclaim its reset generation.
#[tokio::test]
#[ignore = "gated journey: real Forge publication recovery and orphan collection"]
async fn pg_bifrost_forge_publication_recovery_and_orphan_gc() {
    supervised_prepared_audit_failure_and_gc_journey().await;
    supervised_uncertain_commit_recovery_journey().await;
}

/// Drives verified publication failure and later GC through the bound Forge roles.
///
/// # Panics
///
/// Panics when the durable planner does not expose the exact small-files
/// live-rewrite plan, the shared uploader boundary is not reached, the
/// supervised failure performs eager deletion, or its lawful snapshot-expiry
/// carrier does not settle orphan collection exactly once.
async fn supervised_prepared_audit_failure_and_gc_journey() {
    let observer = ForgeWorkerCompletionObserver::new();
    let config = ForgeConfig {
        min_files: 2,
        max_files_per_bin: 64,
        max_files_per_tick: 64,
        orphan_gc_ttl: Duration::from_millis(1),
        snapshot_retention: Duration::from_hours(48),
        ..ForgeConfig::default()
    };
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_config_for_test(config)
        .with_forge_completion_observer_for_test(observer.clone())
        .start_bound()
        .await
        .expect("bound publication supervision server");
    let fixture = seed_forge_group(&server, "journey_supervised_prepared_gc").await;
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("close supervised publication staging partition");
    let latest_uncompacted_partition: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT max(partition_start) FROM vala.file_list \
         WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 \
         AND NOT compacted",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("supervised publication uncompacted partition day");
    assert!(
        latest_uncompacted_partition.is_some_and(|partition_start| {
            partition_start
                < server
                    .forge_clock()
                    .now()
                    .expect("supervised publication Forge clock")
        }),
        "all uncompacted publication inputs must belong to a closed partition"
    );
    let controls = server
        .forge_object_store_control_for_test()
        .expect("supervised Forge object-store controls");
    let mut staging_worker_held = false;
    for generation in 0..2 {
        if generation == 1 {
            observer.hold_after_next_attempt_for_test();
            let table = fixture
                .catalog
                .load_table(&fixture.binding.table_ident())
                .await
                .expect("publication first-fold table");
            let target_update = Transaction::new(&table).update_table_properties().set(
                "write.target-file-size-bytes".to_owned(),
                (10 * 1024 * 1024).to_string(),
            );
            ApplyTransactionAction::apply(target_update, Transaction::new(&table))
                .expect("multi-output target property update")
                .commit(fixture.catalog.as_ref())
                .await
                .expect("multi-output target property commit");
            append_native_forge_group_with_rows(
                &server,
                &fixture,
                0..8,
                14 * FORGE_CONVERGENCE_BATCH_ROWS,
            )
            .await;
        }
        let expected = observer.completed().saturating_add(1);
        let expected_attempts = observer.attempts().saturating_add(1);
        trigger_supervised_scheduler(
            &server,
            Scenario {
                name: "supervised-staging-publication",
                tenants: 1,
            },
        )
        .await;
        if generation == 1 {
            tokio::time::timeout(
                Duration::from_secs(30),
                observer.wait_for_attempts_at_least(expected_attempts),
            )
            .await
            .expect("supervised staging publication attempt");
            tokio::time::timeout(
                Duration::from_secs(30),
                observer.wait_for_held_attempt_for_test(),
            )
            .await
            .expect("hold publication worker after staging");
            staging_worker_held = true;
        } else {
            tokio::time::timeout(
                Duration::from_secs(30),
                observer.wait_for_at_least(expected),
            )
            .await
            .expect("supervised staging publication");
            assert!(matches!(
                observer.completed_strategies().last(),
                Some(ForgeClaimStrategy::Known(ForgeTaskStrategy::StagingFold))
            ));
        }
    }
    for _ in 0..4 {
        let remaining: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.file_list WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 AND NOT compacted",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("publication staging remainder");
        if remaining == 0 {
            break;
        }
        assert!(staging_worker_held);
        let expected_attempts = observer.attempts().saturating_add(1);
        observer.hold_after_next_attempt_for_test();
        observer.release_held_attempt_for_test();
        trigger_supervised_scheduler(
            &server,
            Scenario {
                name: "supervised-staging-publication-remainder",
                tenants: 1,
            },
        )
        .await;
        tokio::time::timeout(
            Duration::from_secs(30),
            observer.wait_for_attempts_at_least(expected_attempts),
        )
        .await
        .expect("supervised staging publication remainder attempt");
        tokio::time::timeout(
            Duration::from_secs(30),
            observer.wait_for_held_attempt_for_test(),
        )
        .await
        .expect("hold publication worker after staging remainder");
        staging_worker_held = true;
    }
    let uncompacted_files: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.file_list WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 AND NOT compacted",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("closed-partition uncompacted file count");
    assert_eq!(
        uncompacted_files, 0,
        "both staging folds must be catalog-published"
    );
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("publication catalog table");
    assert!(
        table.metadata().snapshots().count() >= 2,
        "the staging folds must create multiple snapshots"
    );
    let catalog_paths = current_live_paths(&table)
        .await
        .into_iter()
        .collect::<Vec<_>>();
    assert!(
        catalog_paths.len() >= 2,
        "the two staging folds must expose multiple physical outputs"
    );
    let nonterminal_expiry_tasks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy='snapshot_expiry' AND state IN ('ready','claimed','running','prepared','retryable')",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("pre-rewrite snapshot-expiry task count");
    assert_eq!(
        nonterminal_expiry_tasks, 0,
        "snapshot retention must not compete with the live rewrite"
    );
    trigger_supervised_scheduler(
        &server,
        Scenario {
            name: "supervised-incomplete-multi-output-plan",
            tenants: 1,
        },
    )
    .await;
    let (planned_state, planned_inputs): (String, Vec<String>) = sqlx::query_as(
        "SELECT state,ARRAY(SELECT jsonb_array_elements_text(plan->'inputs') ORDER BY 1) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy='small_files' AND state IN ('ready','retryable') ORDER BY created_at DESC LIMIT 1",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("observe planned multi-output rewrite");
    assert_eq!(planned_state, "ready");
    assert!(
        planned_inputs.len() >= 2
            && planned_inputs
                .iter()
                .all(|input| catalog_paths.contains(input)),
        "the multi-output failure must target one whole multi-file bin"
    );
    let publication_base = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("multi-output publication base");
    let publication_snapshot = publication_base
        .metadata()
        .current_snapshot_id()
        .expect("multi-output publication snapshot");
    let prepared_before = fixture
        .operation_count("forge.iceberg_rewrite.prepared")
        .await;
    let verified_outputs_before = controls.output_put_calls();
    let returned_errors_before = observer.returned_errors().len();
    controls.fail_output_put_at_ordinal_for_test(1);
    controls.pause_after_next_output_put();
    let expected_failure_attempts = observer.attempts().saturating_add(1);
    assert!(staging_worker_held);
    observer.release_held_attempt_for_test();
    trigger_supervised_scheduler(
        &server,
        Scenario {
            name: "supervised-incomplete-multi-output",
            tenants: 1,
        },
    )
    .await;
    tokio::time::timeout(Duration::from_secs(30), controls.wait_for_output_put())
        .await
        .expect("verified ordinal-zero output PUT");
    let incomplete_output = controls
        .last_output_path()
        .expect("verified ordinal-zero output path");
    controls.release_output_put();
    tokio::time::timeout(
        Duration::from_secs(30),
        observer.wait_for_attempts_at_least(expected_failure_attempts),
    )
    .await
    .expect("incomplete multi-output attempt");
    let incomplete_table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("table after incomplete multi-output attempt");
    assert_eq!(
        controls.output_put_calls(),
        verified_outputs_before + 1,
        "output ordinal zero must verify before ordinal one reaches its injected failure"
    );
    assert_eq!(
        observer.returned_errors().len(),
        returned_errors_before + 1,
        "the selected second-output failure must return from the supervised attempt"
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        prepared_before,
        "an incomplete verified output set must not append Prepared evidence"
    );
    assert_eq!(
        incomplete_table.metadata().current_snapshot_id(),
        Some(publication_snapshot),
        "an incomplete verified output set must not replace the Iceberg snapshot"
    );
    assert!(
        fixture.staging.stat(&incomplete_output).await.is_ok(),
        "the verified prefix remains available to delayed fenced orphan GC"
    );
    let incomplete_task: uuid::Uuid = sqlx::query_scalar(
        "SELECT task_id FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 \
         AND strategy='small_files' AND state='retryable' AND failure_class='transient_object_store' \
         AND attempt_id IS NULL AND claimed_by IS NULL ORDER BY created_at DESC LIMIT 1",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("incomplete live rewrite retry row");
    server
        .fail_next_forge_prepared_audit_for_test()
        .expect("arm Prepared audit failure");
    controls.pause_after_next_output_put();
    advance_transient_forge_retry(&server, incomplete_task, "transient_object_store", false).await;
    let expected_attempts = observer.attempts().saturating_add(1);
    trigger_supervised_scheduler(
        &server,
        Scenario {
            name: "supervised-prepared-gc",
            tenants: 1,
        },
    )
    .await;
    tokio::time::timeout(Duration::from_secs(30), controls.wait_for_output_put())
        .await
        .expect("supervised verified output PUT");
    let output = controls
        .last_output_path()
        .expect("supervised verified output path");
    let (strategy, state, base_snapshot_id, inputs): (String, String, i64, Vec<String>) =
        sqlx::query_as(
        "SELECT strategy,state,base_snapshot_id,ARRAY(SELECT jsonb_array_elements_text(plan->'inputs') ORDER BY 1) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy='small_files' AND plan->'parameters'->>'kind'='live_rewrite' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("observe exact durable live-rewrite plan");
    assert_eq!(strategy, "small_files");
    assert_eq!(state, "running");
    assert_eq!(
        base_snapshot_id,
        table
            .metadata()
            .current_snapshot_id()
            .expect("current base snapshot")
    );
    assert!(
        inputs.len() >= 2 && inputs.iter().all(|input| catalog_paths.contains(input)),
        "the durable plan must retain one whole pinned catalog bin"
    );
    assert!(fixture.staging.stat(&output).await.is_ok());
    controls.release_output_put();
    tokio::time::timeout(
        Duration::from_secs(30),
        observer.wait_for_attempts_at_least(expected_attempts),
    )
    .await
    .expect("Prepared audit failure observation");
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        0
    );
    assert!(controls.delete_paths().is_empty());
    assert!(fixture.staging.stat(&output).await.is_ok());
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("make snapshot retention and orphan TTL due");
    let terminal_before = fixture.operation_count("forge.orphan_gc.committed").await
        + fixture.operation_count("forge.orphan_gc.recovered").await;
    controls.pause_next_delete();
    server.trigger_forge_scheduler_for_test();
    tokio::time::timeout(Duration::from_secs(30), controls.wait_for_delete())
        .await
        .expect("orphan GC final recheck admitted one delete");
    assert!(fixture.staging.stat(&output).await.is_ok());
    let (expiry_task, expiry_state, snapshot_expiry_due): (uuid::Uuid, String, bool) =
        sqlx::query_as(
            "SELECT task_id,state,(plan->'parameters'->>'snapshot_expiry_due')::boolean FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy='snapshot_expiry' AND state='prepared' ORDER BY created_at DESC LIMIT 1",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("prepared snapshot-expiry orphan-GC carrier");
    assert_eq!(expiry_state, "prepared");
    assert!(
        snapshot_expiry_due,
        "the lawful carrier must be due for snapshot expiry"
    );
    observer.hold_after_next_attempt_for_test();
    controls.release_paused_delete();
    tokio::time::timeout(
        Duration::from_secs(30),
        observer.wait_for_held_attempt_for_test(),
    )
    .await
    .expect("held snapshot-expiry attempt after durable execution");
    let expiry_terminal_state: String = sqlx::query_scalar(
        "SELECT state FROM vala.forge_tasks WHERE data_tenant_id=$1 AND task_id=$2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(expiry_task)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("terminal snapshot-expiry carrier");
    let mut conn = fixture
        .vala
        .tenant_conn(fixture.tenant)
        .await
        .expect("snapshot-expiry audit tenant connection");
    let expiry_terminal_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=wyrd.current_tenant() AND operation='forge.task.succeeded' AND resource=$1",
    )
    .bind(format!("forge-task:{expiry_task}"))
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("terminal snapshot-expiry carrier audit count");
    assert_eq!(expiry_terminal_state, "succeeded");
    assert_eq!(expiry_terminal_audits, 1);
    assert!(fixture.staging.stat(&output).await.is_err());
    assert!(
        fixture.staging.stat(&incomplete_output).await.is_err(),
        "orphan GC must reclaim the aged pre-Prepared output prefix"
    );
    assert_eq!(
        controls
            .delete_paths()
            .iter()
            .filter(|path| path.as_str() == output.as_str())
            .count(),
        1,
        "orphan GC deletes the verified generation once"
    );
    assert_eq!(
        controls
            .delete_paths()
            .iter()
            .filter(|path| path.as_str() == incomplete_output.as_str())
            .count(),
        1,
        "orphan GC deletes the incomplete generation prefix once"
    );
    let terminal_after = fixture.operation_count("forge.orphan_gc.committed").await
        + fixture.operation_count("forge.orphan_gc.recovered").await;
    assert_eq!(terminal_after, terminal_before + 1);
    let shutdown = tokio::spawn(async move { server.shutdown().await });
    tokio::task::yield_now().await;
    observer.release_held_attempt_for_test();
    shutdown
        .await
        .expect("publication shutdown task")
        .expect("publication server shutdown");
}

/// Proves Forge fixture rebuilds retain the server-owned wall clock.
#[tokio::test]
#[ignore = "gated journey: real bound Wyrd server, Postgres, and Forge"]
async fn forge_fixture_rebuilds_retain_manual_forge_clock() {
    let server = WyrdTestServer::builder()
        .start_bound()
        .await
        .expect("server");
    let fixture = native_forge_group(&server, "manual_forge_clock").await;
    let initial = fixture
        .forge
        .clock_for_test()
        .now()
        .expect("initial Forge clock");
    let advanced = server
        .forge_clock()
        .advance(chrono::Duration::seconds(1))
        .expect("advance Forge clock");
    let catalog = fixture.context_with_catalog(fixture.config.clone(), fixture.catalog.clone());
    assert!(advanced > initial);
    assert_eq!(
        catalog.clock_for_test().now().expect("catalog clock"),
        advanced
    );
    server.shutdown().await.expect("server shutdown");
}
