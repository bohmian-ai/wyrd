//! Live rewrite interleavings: cancellation and restart around the prepared
//! boundary, and what a fence loss leaves behind.
//!
//! Module of the `forge` group; shared fixtures live in `interleaving_support.rs`.

use arrow::util::display::array_value_to_string;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::forge::{ForgeLease, forge_lease_key};
use vala_bifrost_redux::maintenance::{StagingFileCommitted, StagingPublishOutcome};
use wyrd_testing::bifrost::{CommitUncertaintyCatalog, ForgeObjectStoreControl, seed_forge_group};

use super::interleaving_support::*;

/// Read the exact logical row multiset from every current-snapshot Parquet file.
async fn current_logical_rows(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
    table: &iceberg::table::Table,
) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for catalog_path in current_live_paths(table).await {
        let object_path = catalog_path
            .find(&fixture.binding.object_prefix)
            .map(|offset| &catalog_path[offset..])
            .expect("live catalog path belongs to the fixture table");
        let bytes = fixture
            .staging
            .read(object_path)
            .await
            .expect("live Parquet object")
            .to_bytes();
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes)
            .expect("live Parquet reader")
            .build()
            .expect("live Parquet batch reader");
        for batch in reader {
            let batch = batch.expect("live Parquet batch");
            for row in 0..batch.num_rows() {
                rows.push(
                    batch
                        .columns()
                        .iter()
                        .map(|column| {
                            array_value_to_string(column.as_ref(), row)
                                .expect("logical row value formatting")
                        })
                        .collect(),
                );
            }
        }
    }
    rows.sort();
    rows
}

/// Produce one durable Prepared live rewrite by cancelling at the successful PUT boundary.
async fn prepare_cancelled_live_rewrite(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> (Arc<ForgeObjectStoreControl>, BTreeSet<String>) {
    fold_staged_pair_without_live_replacement(fixture).await;
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    fold_staged_pair_without_live_replacement(fixture).await;

    let control = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    let context = fixture.context_with_object_store(fixture.config.clone(), Arc::clone(&control));
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("live rewrite table");
    let action = Transaction::new(&table).update_table_properties().set(
        "write.target-file-size-bytes".to_owned(),
        (16 * 1024 * 1024).to_string(),
    );
    ApplyTransactionAction::apply(action, Transaction::new(&table))
        .expect("target-size property update")
        .commit(fixture.catalog.as_ref())
        .await
        .expect("target-size property commit");
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("target-sized live rewrite table");
    let before_paths = current_live_paths(&table).await;
    let plan = context
        .discover_live_rewrites_for_test(
            &fixture.binding,
            &table,
            chrono::NaiveDate::from_ymd_opt(2026, 7, 15)
                .expect("current day")
                .and_hms_opt(0, 0, 0)
                .expect("midnight")
                .and_utc(),
        )
        .await
        .expect("live rewrite discovery");
    assert_eq!(plan.groups_for_test().len(), 1, "exactly one rewrite group");
    let group = plan.groups_for_test()[0].clone();
    assert_eq!(group.files_for_test().len(), 2, "exactly two inputs");
    let input_bytes: u64 = group
        .files_for_test()
        .iter()
        .map(|file| file.file_size_bytes_for_test())
        .sum();
    assert!(
        input_bytes < 1024 * 1024,
        "fixture inputs must stay below 1 MiB"
    );
    assert_eq!(
        table
            .metadata()
            .table_properties()
            .expect("Iceberg table properties")
            .write_target_file_size_bytes,
        16 * 1024 * 1024
    );
    let base = plan.base_snapshot_id_for_test();
    let mut lease = ForgeLease::acquire(
        &fixture.operator_pool,
        forge_lease_key(
            fixture.tenant,
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        ),
        uuid::Uuid::now_v7(),
        fixture.config.lease_ttl,
    )
    .await
    .expect("post-PUT lease query")
    .expect("post-PUT lease");
    control.pause_after_next_output_put();
    let stop = CancellationToken::new();
    let operator_pool = fixture.operator_pool.clone();
    let old_context = Arc::downgrade(&context);
    let task = tokio::spawn({
        let binding = fixture.binding.clone();
        let stop = stop.clone();
        async move {
            let result = context
                .replace_live_group_for_test(&mut lease, &binding, &table, base, &group, &stop)
                .await;
            let release = lease.release(&operator_pool).await;
            (result, release)
        }
    });
    tokio::time::timeout(Duration::from_secs(30), control.wait_for_output_put())
        .await
        .expect("successful output PUT boundary");
    let output = control
        .last_output_path()
        .expect("armed output PUT records its exact path");
    fixture
        .staging
        .stat(&output)
        .await
        .expect("output is durable while notification is paused");
    stop.cancel();
    control.release_output_put();
    let (result, release) = tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("cancelled replacement shutdown bound")
        .expect("cancelled replacement task");
    assert!(
        matches!(result, Err(vala_bifrost_redux::forge::ForgeError::Shutdown)),
        "post-PUT cancellation must stop after Prepared: {result:?}"
    );
    assert!(release.expect("cancelled replacement lease release"));
    assert!(
        old_context.upgrade().is_none(),
        "the first-process Forge allocation must be dropped after bounded shutdown"
    );
    assert_eq!(control.output_put_calls(), 1);
    assert_eq!(
        live_rewrite_state_phase(fixture).await.as_deref(),
        Some("prepared")
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        1
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.committed")
            .await,
        0
    );
    let details = live_rewrite_details(fixture).await;
    assert_eq!(details.len(), 1, "Prepared is the only durable phase");
    assert_eq!(
        details[0]["output_paths"]
            .as_array()
            .expect("Prepared output paths")
            .len(),
        1,
        "the forced replacement produces exactly one durable output"
    );
    restore_healthy_live_target(fixture).await;
    (control, before_paths)
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Cancellation after the real output PUT persists Prepared before bounded shutdown.
async fn live_rewrite_post_put_cancellation_persists_prepared_before_shutdown() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "live_rewrite_post_put_cancel").await;
    let (control, before_paths) = prepare_cancelled_live_rewrite(&fixture).await;
    assert_eq!(control.output_put_calls(), 1);
    let retained = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("unchanged base table");
    assert_eq!(current_live_paths(&retained).await, before_paths);
    assert_prepared_live_outputs_exist(&fixture).await;
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// A fresh Forge resets one abandoned output and preserves the original live set.
async fn restart_resets_abandoned_live_output_exactly_once() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "live_rewrite_restart_reset").await;
    let (control, before_paths) = prepare_cancelled_live_rewrite(&fixture).await;
    let prepared_rows = live_rewrite_state_rows(&fixture).await;
    assert_eq!(prepared_rows.len(), 1);
    assert_eq!(prepared_rows[0].0, "prepared");
    assert_eq!(prepared_rows[0].2, prepared_rows[0].3);
    let base_table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("pre-reset logical table");
    let expected_rows = current_logical_rows(&fixture, &base_table).await;
    assert!(!expected_rows.is_empty());
    let output = control.last_output_path().expect("prepared output path");
    let config = recovery_config_without_live_replacement(&fixture);
    advance_forge_clock_past_uncertainty(&server);
    let mut lifecycle =
        SupervisedForge::start(&fixture, config, Arc::clone(&fixture.catalog), control);
    lifecycle.run_one_success().await;
    lifecycle.shutdown().await;
    let rows = live_rewrite_state_rows(&fixture).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "reset");
    assert_eq!(rows[0].1, prepared_rows[0].1);
    assert_eq!(rows[0].2, prepared_rows[0].2);
    assert_eq!(rows[0].3["operation_id"], prepared_rows[0].1.to_string());
    assert_eq!(rows[0].3["phase"], "reset");
    assert_eq!(rows[0].3["input_paths"], prepared_rows[0].2["input_paths"]);
    assert_eq!(
        rows[0].3["output_paths"],
        prepared_rows[0].2["output_paths"]
    );
    assert_eq!(rows[0].3["committed_snapshot_id"], serde_json::Value::Null);
    assert_eq!(
        fixture.operation_count("forge.iceberg_rewrite.reset").await,
        1
    );
    assert!(fixture.staging.stat(&output).await.is_err());
    let retained = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("reset table");
    assert_eq!(current_live_paths(&retained).await, before_paths);
    assert_eq!(
        current_logical_rows(&fixture, &retained).await,
        expected_rows
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Replaying a fresh Forge after Reset does not append a second terminal transition.
async fn restart_replay_does_not_duplicate_live_terminal_transition() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "live_rewrite_restart_replay").await;
    let (control, _) = prepare_cancelled_live_rewrite(&fixture).await;
    let config = recovery_config_without_live_replacement(&fixture);
    advance_forge_clock_past_uncertainty(&server);
    let mut lifecycle =
        SupervisedForge::start(&fixture, config, Arc::clone(&fixture.catalog), control);
    lifecycle.run_one_success().await;
    assert_eq!(
        fixture.operation_count("forge.iceberg_rewrite.reset").await,
        1
    );
    lifecycle.schedule_once().await;
    lifecycle.shutdown().await;
    assert_eq!(
        fixture.operation_count("forge.iceberg_rewrite.reset").await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// A fresh Forge recovers one catalog-accepted uncertain replacement exactly once.
async fn restart_recovers_catalog_accepted_live_rewrite_exactly_once() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "live_rewrite_restart_recovered").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let (context, table, plan, mut lease) =
        live_replacement_context(&fixture, Arc::clone(&control)).await;
    let old_context = Arc::downgrade(&context);
    let base = plan.base_snapshot_id_for_test();
    assert_eq!(
        plan.groups_for_test().len(),
        1,
        "recovered restart fixture must discover exactly one group"
    );
    let group = plan.groups_for_test()[0].clone();
    assert_eq!(
        group.files_for_test().len(),
        2,
        "recovered restart fixture must rewrite exactly two inputs"
    );
    let expected_rows = current_logical_rows(&fixture, &table).await;
    assert!(!expected_rows.is_empty());
    let mut expected_paths = current_live_paths(&table).await;
    for input in group.files_for_test() {
        assert!(expected_paths.remove(input.catalog_path_for_test()));
    }
    control.fail_after_next_commit();
    let error = context
        .replace_live_group_for_test(
            &mut lease,
            &fixture.binding,
            &table,
            base,
            &group,
            &CancellationToken::new(),
        )
        .await
        .expect_err("accepted commit response remains uncertain");
    assert!(
        error
            .to_string()
            .contains("injected post-commit uncertainty"),
        "unexpected uncertainty error: {error}"
    );
    assert!(
        lease
            .release(&fixture.operator_pool)
            .await
            .expect("uncertain replacement lease release")
    );
    drop(context);
    assert!(
        old_context.upgrade().is_none(),
        "the uncertain first-process Forge must be dropped before restart"
    );
    let outputs = assert_prepared_live_outputs_exist(&fixture).await;
    assert_eq!(
        outputs.len(),
        1,
        "recovered restart fixture must persist exactly one output"
    );
    expected_paths.extend(outputs);
    let prepared_rows = live_rewrite_state_rows(&fixture).await;
    assert_eq!(prepared_rows.len(), 1);
    assert_eq!(prepared_rows[0].0, "prepared");
    assert_eq!(prepared_rows[0].2, prepared_rows[0].3);

    let config = recovery_config_without_live_replacement(&fixture);
    advance_forge_clock_past_uncertainty(&server);
    let mut lifecycle =
        SupervisedForge::start(&fixture, config, control, Arc::clone(&fixture.object_store));
    lifecycle.hold_next_attempt();
    lifecycle.schedule_once().await;
    lifecycle.wait_for_held_attempt().await;
    assert_eq!(lifecycle.worker_observer.attempts(), 1);
    assert_eq!(lifecycle.worker_observer.completed(), 0);
    assert!(
        lifecycle
            .worker_observer
            .returned_errors()
            .iter()
            .any(|error| error.contains("post-commit uncertainty remains unresolved"))
    );
    lifecycle.stop_worker().await;
    let rows = live_rewrite_state_rows(&fixture).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "recovered");
    assert_eq!(rows[0].1, prepared_rows[0].1);
    assert_eq!(rows[0].2, prepared_rows[0].2);
    assert_eq!(rows[0].3["operation_id"], prepared_rows[0].1.to_string());
    assert_eq!(rows[0].3["phase"], "recovered");
    assert_eq!(rows[0].3["input_paths"], prepared_rows[0].2["input_paths"]);
    assert_eq!(
        rows[0].3["output_paths"],
        prepared_rows[0].2["output_paths"]
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.recovered")
            .await,
        1
    );
    let replaced = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("recovered table");
    assert_single_snapshot_advance(&replaced, base);
    assert_eq!(
        rows[0].3["committed_snapshot_id"],
        replaced
            .metadata()
            .current_snapshot_id()
            .expect("recovered current snapshot")
    );
    assert_eq!(current_live_paths(&replaced).await, expected_paths);
    assert_eq!(
        current_logical_rows(&fixture, &replaced).await,
        expected_rows
    );
    let operation_id = rows[0].1.to_string();
    let summary = replaced
        .metadata()
        .current_snapshot()
        .expect("recovered current snapshot")
        .summary();
    assert_eq!(
        summary.additional_properties.get("forge.workflow"),
        Some(&"iceberg-rewrite".to_owned())
    );
    assert_eq!(
        summary.additional_properties.get("forge.operation_id"),
        Some(&operation_id)
    );
    lifecycle.schedule_once().await;
    lifecycle.shutdown().await;
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.recovered")
            .await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Fence loss at the Reset delete boundary leaves Prepared and its output protected.
async fn fence_loss_before_reset_delete_keeps_prepared_and_protected() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "live_rewrite_reset_fence").await;
    let (control, _) = prepare_cancelled_live_rewrite(&fixture).await;
    let output = control.last_output_path().expect("prepared output path");
    control.pause_next_delete();
    let config = recovery_config_without_live_replacement(&fixture);
    advance_forge_clock_past_uncertainty(&server);
    let mut lifecycle = SupervisedForge::start(
        &fixture,
        config,
        Arc::clone(&fixture.catalog),
        control.clone(),
    );
    lifecycle.hold_next_attempt();
    lifecycle.schedule_once().await;
    tokio::time::timeout(Duration::from_secs(30), control.wait_for_delete())
        .await
        .expect("Reset reached delete boundary");
    let (owner, token) = steal_forge_lease(&fixture).await;
    control.reject_paused_delete();
    lifecycle.wait_for_held_attempt().await;
    assert_eq!(lifecycle.worker_observer.completed(), 0);
    lifecycle.stop_worker().await;
    lifecycle.shutdown().await;
    assert_eq!(
        live_rewrite_state_phase(&fixture).await.as_deref(),
        Some("prepared")
    );
    assert_eq!(
        fixture.operation_count("forge.iceberg_rewrite.reset").await,
        0
    );
    fixture
        .staging
        .stat(&output)
        .await
        .expect("Prepared output remains protected");
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &forge_lease_key(
                fixture.tenant,
                &fixture.binding.logical_namespace,
                &fixture.binding.table_name,
            ),
            owner,
            token,
        )
        .await
        .expect("successor reset lease release")
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Shutdown at the Reset delete boundary lets the authority-owned effect finish.
async fn cancellation_at_reset_delete_keeps_prepared_without_terminal() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "live_rewrite_reset_cancel").await;
    let (control, before_paths) = prepare_cancelled_live_rewrite(&fixture).await;
    let output = control.last_output_path().expect("prepared output path");
    control.pause_after_delete_for_path(&output);
    let config = recovery_config_without_live_replacement(&fixture);
    advance_forge_clock_past_uncertainty(&server);
    let mut lifecycle = SupervisedForge::start(
        &fixture,
        config,
        Arc::clone(&fixture.catalog),
        control.clone(),
    );
    let old_context = Arc::downgrade(&lifecycle.forge);
    lifecycle.hold_next_attempt();
    assert_eq!(
        lifecycle.publisher.try_publish(StagingFileCommitted::new(
            fixture.binding.clone(),
            vala_bifrost_redux::catalog::layout::TimePartition::new(
                vala_bifrost_redux::catalog::layout::TimeGranularity::Day,
                chrono::NaiveDate::from_ymd_opt(2026, 7, 15)
                    .expect("partition day")
                    .and_hms_opt(0, 0, 0)
                    .expect("midnight")
                    .and_utc(),
            )
            .expect("midnight is a daily partition boundary"),
        )),
        StagingPublishOutcome::Published
    );
    lifecycle.schedule_once().await;
    if tokio::time::timeout(Duration::from_secs(30), control.wait_for_completed_delete())
        .await
        .is_err()
    {
        let tasks: Vec<(String, String, i32, bool)> = sqlx::query_as(
            "SELECT state,strategy,attempt_count,next_eligible_at<=statement_timestamp() FROM vala.forge_tasks WHERE data_tenant_id=$1 ORDER BY created_at,task_id",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_all(fixture.operator_pool.pool())
        .await
        .expect("reset timeout task diagnostics");
        panic!(
            "Reset did not delete exact prepared output {output}; deletes={:?}; tasks={tasks:?}; attempts={}; errors={:?}",
            control.delete_paths(),
            lifecycle.worker_observer.attempts(),
            lifecycle.worker_observer.returned_errors(),
        );
    }
    lifecycle.worker_stop.cancel();
    control.release_completed_delete();
    lifecycle.wait_for_held_attempt().await;
    lifecycle.stop_worker().await;
    lifecycle.shutdown().await;
    assert!(
        old_context.upgrade().is_none(),
        "cancelled Reset Forge must be dropped after scheduler shutdown"
    );
    let rows = live_rewrite_state_rows(&fixture).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "reset");
    assert_eq!(
        fixture.operation_count("forge.iceberg_rewrite.reset").await,
        1
    );
    assert!(
        fixture.staging.stat(&output).await.is_err(),
        "the authority-owned reset completes its exact delete"
    );
    let retained = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("cancelled Reset table");
    assert_eq!(current_live_paths(&retained).await, before_paths);
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// A failed terminal append cannot falsely close Prepared after Reset deletion.
async fn live_rewrite_terminal_append_failure_leaves_prepared() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "live_rewrite_terminal_failure").await;
    let (control, before_paths) = prepare_cancelled_live_rewrite(&fixture).await;
    let output = control.last_output_path().expect("prepared output path");
    let config = recovery_config_without_live_replacement(&fixture);
    advance_forge_clock_past_uncertainty(&server);
    let mut failed = SupervisedForge::start(
        &fixture,
        config.clone(),
        Arc::clone(&fixture.catalog),
        control.clone(),
    );
    failed.forge.fail_next_terminal_live_audit_for_test();
    failed.hold_next_attempt();
    failed.schedule_once().await;
    failed.wait_for_held_attempt().await;
    assert_eq!(failed.worker_observer.completed(), 0);
    failed.stop_worker().await;
    failed.shutdown().await;
    let stopped_task = expire_stopped_running_claim(&fixture).await;
    assert_eq!(
        live_rewrite_state_phase(&fixture).await.as_deref(),
        Some("prepared")
    );
    assert_eq!(
        fixture.operation_count("forge.iceberg_rewrite.reset").await,
        0
    );
    assert!(
        fixture.staging.stat(&output).await.is_err(),
        "the delete may complete, but terminal state must not be fabricated"
    );

    let mut replay =
        SupervisedForge::start(&fixture, config, Arc::clone(&fixture.catalog), control);
    replay.run_one_success().await;
    assert_eq!(replay.worker_observer.completed_tasks(), vec![stopped_task]);
    replay.shutdown().await;
    assert_eq!(
        live_rewrite_state_phase(&fixture).await.as_deref(),
        Some("reset")
    );
    assert_eq!(
        fixture.operation_count("forge.iceberg_rewrite.reset").await,
        1
    );
    let retained = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("terminal replay table");
    assert_eq!(current_live_paths(&retained).await, before_paths);
    server.shutdown().await.expect("server shutdown");
}
