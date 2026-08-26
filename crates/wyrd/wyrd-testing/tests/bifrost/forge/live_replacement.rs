//! Live replacement interleavings: audit failure, plan-base selection, and
//! every catalog-boundary outcome including an uncertain response.
//!
//! Module of the `forge` group; shared fixtures live in `interleaving_support.rs`.

use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::forge::{ForgeLease, IcebergRewriteDisposition, forge_lease_key};
use wyrd_testing::bifrost::{CommitUncertaintyCatalog, seed_forge_group};

use super::interleaving_support::*;

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// A rejected `Prepared` append returns its original error and reclaims only rewrite-owned outputs.
async fn live_replacement_prepared_audit_failure_reclaims_unreferenced_outputs() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "live_rewrite_prepared_failure").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let (context, table, plan, mut lease) = live_replacement_context(&fixture, control).await;
    let base = plan.base_snapshot_id_for_test();
    let group = plan
        .groups_for_test()
        .iter()
        .find(|group| group.files_for_test().len() >= 2)
        .expect("eligible group")
        .clone();
    let before_paths = current_live_paths(&table).await;
    let before_snapshots = table.metadata().snapshots().len();
    context.fail_next_prepared_live_audit_for_test();

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
        .expect_err("injected Prepared append failure");
    assert!(
        error
            .to_string()
            .contains("injected Prepared audit append failure"),
        "the audit error must remain authoritative: {error}"
    );

    let retained = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("unchanged table");
    assert_eq!(retained.metadata().snapshots().len(), before_snapshots);
    assert_eq!(current_live_paths(&retained).await, before_paths);
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        0
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.committed")
            .await,
        0
    );
    let entries = fixture
        .staging
        .list(&fixture.binding.object_prefix)
        .await
        .expect("rewrite output listing");
    assert!(
        !entries
            .iter()
            .any(|entry| entry.path().contains("/data/forge/")),
        "Prepared audit failure retained rewrite-owned output: {entries:?}"
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// A successful replacement deletes exactly its planned inputs and records one typed phase pair.
async fn live_replacement_successfully_replaces_exact_live_set() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "live_rewrite_success").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let (context, table, plan, mut lease) = live_replacement_context(&fixture, control).await;
    let base = plan.base_snapshot_id_for_test();
    let group = plan
        .groups_for_test()
        .iter()
        .find(|group| group.files_for_test().len() >= 2)
        .expect("eligible group")
        .clone();
    let mut expected_paths = current_live_paths(&table).await;
    for input in group.files_for_test() {
        assert!(
            expected_paths.remove(input.catalog_path_for_test()),
            "every selected input must be live"
        );
    }

    let disposition = context
        .replace_live_group_for_test(
            &mut lease,
            &fixture.binding,
            &table,
            base,
            &group,
            &CancellationToken::new(),
        )
        .await
        .expect("live replacement commits");
    let IcebergRewriteDisposition::Committed {
        operation_id,
        snapshot_id,
        input_files,
        output_files,
        input_rows,
        output_rows,
        ..
    } = disposition
    else {
        panic!("live replacement must commit")
    };
    assert_eq!(input_files, group.files_for_test().len());
    assert!(output_files > 0, "replacement must retain rewritten rows");
    assert_eq!(input_rows, output_rows, "replacement must preserve rows");

    let replaced = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("replaced table");
    assert_single_snapshot_advance(&replaced, base);
    assert_eq!(
        replaced
            .metadata()
            .current_snapshot_id()
            .expect("replacement snapshot"),
        snapshot_id
    );
    let outputs = assert_prepared_live_outputs_exist(&fixture).await;
    expected_paths.extend(outputs);
    assert_eq!(current_live_paths(&replaced).await, expected_paths);
    let summary = replaced
        .metadata()
        .current_snapshot()
        .expect("replacement snapshot")
        .summary();
    assert_eq!(
        summary.additional_properties.get("forge.workflow"),
        Some(&"iceberg-rewrite".to_owned())
    );
    assert_eq!(
        summary.additional_properties.get("forge.operation_id"),
        Some(&operation_id.to_string())
    );

    let details = live_rewrite_details(&fixture).await;
    assert_eq!(details.len(), 2);
    for (detail, phase) in details.iter().zip(["prepared", "committed"]) {
        assert_eq!(detail["kind"], "forge_iceberg_rewrite");
        assert_eq!(detail["phase"], phase);
        assert_eq!(detail["operation_id"], operation_id.to_string());
        assert_eq!(detail["base_snapshot_id"], base);
    }
    assert_eq!(details[1]["committed_snapshot_id"], snapshot_id);
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// A stale explicit plan base rejects replacement even when the current table has later files.
async fn live_replacement_uses_plan_base_not_file_addition_snapshot() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "live_rewrite_plan_base").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let (context, _table, plan, lease) = live_replacement_context(&fixture, control).await;
    let base = plan.base_snapshot_id_for_test();
    let group = plan
        .groups_for_test()
        .iter()
        .find(|group| group.files_for_test().len() >= 2)
        .expect("eligible group")
        .clone();

    assert!(
        lease
            .release(&fixture.operator_pool)
            .await
            .expect("plan lease release")
    );
    fixture.append_forge_file(4).await;
    fixture.append_forge_file(5).await;
    fold_staged_pair_without_live_replacement(&fixture).await;
    restore_healthy_live_target(&fixture).await;
    let candidate_table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("candidate table");
    assert_ne!(
        candidate_table.metadata().current_snapshot_id(),
        Some(base),
        "candidate additions must not reuse the plan base"
    );
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
    .expect("stale-plan lease query")
    .expect("stale-plan lease");

    let disposition = context
        .replace_live_group_for_test(
            &mut lease,
            &fixture.binding,
            &candidate_table,
            base,
            &group,
            &CancellationToken::new(),
        )
        .await
        .expect("stale plan is a normal disposition");
    assert_eq!(disposition, IcebergRewriteDisposition::SnapshotChanged);
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        0
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.committed")
            .await,
        0
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Cancelling at the real catalog boundary leaves prepared live outputs and no terminal audit.
async fn live_replacement_cancellation_at_catalog_boundary_preserves_prepared() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "live_rewrite_cancel").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let (context, table, plan, mut lease) =
        live_replacement_context(&fixture, control.clone()).await;
    let base = plan.base_snapshot_id_for_test();
    let group = plan
        .groups_for_test()
        .iter()
        .find(|group| group.files_for_test().len() >= 2)
        .expect("eligible group")
        .clone();
    let before = table.metadata().snapshots().len();
    let before_paths = current_live_paths(&table).await;
    control.pause_before_commit();
    let stop = CancellationToken::new();
    let task = tokio::spawn({
        let stop = stop.clone();
        let binding = fixture.binding.clone();
        async move {
            context
                .replace_live_group_for_test(&mut lease, &binding, &table, base, &group, &stop)
                .await
        }
    });
    control.wait_for_before_commit().await;
    stop.cancel();
    control.reject_paused_before_commit();
    assert!(task.await.expect("cancelled replacement task").is_err());
    let retained = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("base table");
    assert_eq!(retained.metadata().snapshots().len(), before);
    assert_eq!(current_live_paths(&retained).await, before_paths);
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
    assert_prepared_live_outputs_exist(&fixture).await;
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// A lease theft after catalog acceptance leaves exactly one replacement and no terminal audit.
async fn live_replacement_lease_theft_after_catalog_acceptance_claims_no_terminal() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "live_rewrite_lease_theft").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let (context, table, plan, mut lease) =
        live_replacement_context(&fixture, control.clone()).await;
    let base = plan.base_snapshot_id_for_test();
    let group = plan
        .groups_for_test()
        .iter()
        .find(|group| group.files_for_test().len() >= 2)
        .expect("eligible group")
        .clone();
    let mut expected_paths = current_live_paths(&table).await;
    for input in group.files_for_test() {
        assert!(
            expected_paths.remove(input.catalog_path_for_test()),
            "discovered replacement input must be live"
        );
    }
    control.pause_after_commit();
    let task = tokio::spawn({
        let binding = fixture.binding.clone();
        let stop = CancellationToken::new();
        async move {
            context
                .replace_live_group_for_test(&mut lease, &binding, &table, base, &group, &stop)
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(30), control.wait_for_commit())
        .await
        .expect("replacement reached accepted catalog commit");
    let (owner, token) = steal_forge_lease(&fixture).await;
    control.reject_paused_commit();
    let error = task
        .await
        .expect("stale replacement task")
        .expect_err("post-acceptance lease theft must remain uncertain");
    assert!(
        error
            .to_string()
            .contains("injected stale Forge lease after catalog commit"),
        "unexpected post-acceptance error: {error}"
    );
    let replaced = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("replaced table");
    assert_single_snapshot_advance(&replaced, base);
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
    expected_paths.extend(assert_prepared_live_outputs_exist(&fixture).await);
    assert_eq!(current_live_paths(&replaced).await, expected_paths);
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &forge_lease_key(
                fixture.tenant,
                &fixture.binding.logical_namespace,
                &fixture.binding.table_name
            ),
            owner,
            token
        )
        .await
        .expect("successor release")
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// An accepted but uncertain catalog response preserves prepared live-rewrite evidence.
async fn live_replacement_uncertain_catalog_response_preserves_prepared() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "live_rewrite_uncertain").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let (context, table, plan, mut lease) =
        live_replacement_context(&fixture, control.clone()).await;
    let base = plan.base_snapshot_id_for_test();
    let group = plan
        .groups_for_test()
        .iter()
        .find(|group| group.files_for_test().len() >= 2)
        .expect("eligible group")
        .clone();
    let mut expected_paths = current_live_paths(&table).await;
    for input in group.files_for_test() {
        assert!(
            expected_paths.remove(input.catalog_path_for_test()),
            "discovered replacement input must be live"
        );
    }
    control.fail_after_next_commit();
    let stop = CancellationToken::new();
    let error = context
        .replace_live_group_for_test(&mut lease, &fixture.binding, &table, base, &group, &stop)
        .await
        .expect_err("uncertain catalog response");
    assert!(
        error
            .to_string()
            .contains("injected post-commit uncertainty"),
        "unexpected live replacement error: {error}"
    );
    let replaced = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("uncertain table");
    assert_single_snapshot_advance(&replaced, base);
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
    expected_paths.extend(assert_prepared_live_outputs_exist(&fixture).await);
    assert_eq!(current_live_paths(&replaced).await, expected_paths);
    assert!(
        lease
            .release(&fixture.operator_pool)
            .await
            .expect("original lease release")
    );
    server.shutdown().await.expect("server shutdown");
}
