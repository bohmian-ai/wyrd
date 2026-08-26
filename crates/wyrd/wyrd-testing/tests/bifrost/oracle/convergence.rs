//! Oracle journeys — Ordered reads over freshly compacted small files — an Oracle read-order
//! assertion whose setup runs Forge, which is why it lives here and not in the
//! `forge` binary.
//!
//! Module of the `oracle` binary; see `oracle.rs` for the capability it
//! proves and `support.rs` for the fixtures it shares.

use vala_bifrost_redux::forge::ForgeLifecycleEvent;
use vala_sql::row_types::forge_tasks::{ForgeClaimStrategy, ForgeTaskStrategy};
use wyrd_spec::DataTenantId;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{
    BifrostClusterSpec, ForgeCausalDiagnosis, ForgeCausalTelemetryReport, WyrdTestCluster,
};

use crate::support::*;

/// Real Forge workers replace the fixture's small files before an ordered read.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_forge_small_files_converge_before_ordered_read() {
    let cluster = WyrdTestCluster::start_spec_with_forge_completion_observer(
        BifrostClusterSpec::one_mixed().with_system_resources(forge_convergence_system_resources()),
    )
    .await
    .expect("Forge convergence cluster");
    let checkpoint = cluster
        .telemetry()
        .checkpoint()
        .expect("Forge causal telemetry checkpoint");
    let server = cluster.server(0).expect("Forge convergence server");
    let table = prepare_spill_table(&cluster, server, "forge_small_files", 1_000_000)
        .await
        .expect("Forge convergence fixture");
    let reader = client(server, "forge-small-files-reader")
        .await
        .expect("Forge convergence reader");
    assert_eq!(
        strict_unordered_summary(&reader, &format!("vala.bifrost.{table}"))
            .await
            .expect("pre-Forge exact dataset"),
        (1_000_000, 256_000_000)
    );
    server
        .forge_clock()
        .advance(chrono::Duration::days(1))
        .expect("close the fixture's event-day partition");
    let observer = cluster
        .forge_completion_observer()
        .expect("Forge completion observer");
    for transition in 1..=64 {
        let workflow = server
            .inspect_forge_workflow_for_test(cluster.data_tenant_id(), &table)
            .await
            .expect("inspect staging-fold convergence");
        if workflow.uncompacted_staging_files == 0 {
            break;
        }
        advance_forge_retry_for_journey(server, cluster.data_tenant_id(), &table).await;
        let expected_attempts = observer.attempts().saturating_add(1);
        let completed_passes = server.completed_forge_scheduler_passes_for_test();
        cluster.request_forge_scheduler_pass_for_test();
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            server.wait_for_forge_scheduler_passes_for_test(completed_passes + 1),
        )
        .await
        .expect("production staging scheduler pass completes");
        let transition_result = tokio::time::timeout(
            std::time::Duration::from_secs(90),
            observer.wait_for_attempts_at_least(expected_attempts),
        )
        .await;
        if transition_result.is_err() {
            let delta = cluster
                .telemetry()
                .delta_since(&checkpoint)
                .expect("stalled Forge continuation telemetry delta");
            let report = ForgeCausalTelemetryReport::from_production_delta(&delta);
            let workflow = server
                .inspect_forge_workflow_for_test(cluster.data_tenant_id(), &table)
                .await
                .expect("inspect stalled Forge continuation");
            let diagnosis = report
                .as_ref()
                .map_err(ToString::to_string)
                .and_then(|report| {
                    report
                        .diagnose(&workflow, None)
                        .map_err(|error| error.to_string())
                });
            panic!(
                "Forge continuation did not complete: transition={transition} diagnosis={diagnosis:?} telemetry={report:?} workflow={workflow:?}"
            );
        }
    }
    assert_eq!(
        server
            .inspect_forge_workflow_for_test(cluster.data_tenant_id(), &table)
            .await
            .expect("inspect staging-fold terminal state")
            .uncompacted_staging_files,
        0,
        "bounded production continuation must drain staging debt"
    );
    let before = server
        .inspect_forge_table_for_test(cluster.data_tenant_id(), &table)
        .await
        .expect("pre-compaction table inspection");
    assert!(before.data_file_count() > 1);
    assert!(before.total_data_file_bytes().expect("pre-Forge bytes") > 0);
    let forge_baseline = server
        .state()
        .forge()
        .expect("Forge composition")
        .resources()
        .snapshot()
        .expect("Forge resource baseline");
    for _ in 0..64 {
        if observer
            .completed_strategies()
            .contains(&ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles))
        {
            break;
        }
        advance_forge_retry_for_journey(server, cluster.data_tenant_id(), &table).await;
        let completed_passes = server.completed_forge_scheduler_passes_for_test();
        cluster.request_forge_scheduler_pass_for_test();
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            server.wait_for_forge_scheduler_passes_for_test(completed_passes + 1),
        )
        .await
        .expect("production rewrite scheduler pass completes");
        let expected_completions = observer.completed().saturating_add(1);
        if tokio::time::timeout(
            std::time::Duration::from_secs(90),
            observer.wait_for_at_least(expected_completions),
        )
        .await
        .is_err()
        {
            let delta = cluster
                .telemetry()
                .delta_since(&checkpoint)
                .expect("stalled rewrite telemetry delta");
            let report = ForgeCausalTelemetryReport::from_production_delta(&delta);
            let workflow = server
                .inspect_forge_workflow_for_test(cluster.data_tenant_id(), &table)
                .await
                .expect("inspect stalled production rewrite");
            let diagnosis = report
                .as_ref()
                .map_err(ToString::to_string)
                .and_then(|report| {
                    report
                        .diagnose(&workflow, None)
                        .map_err(|error| error.to_string())
                });
            panic!(
                "production maintenance task did not complete: diagnosis={diagnosis:?} telemetry={report:?} workflow={workflow:?} table={before:?}"
            );
        }
    }
    assert!(
        observer.completed_strategies().iter().any(|strategy| {
            *strategy == ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles)
        }),
        "bounded production continuation must reach the SmallFiles rewrite"
    );
    let events = observer.lifecycle_events();
    let planned_inputs = events
        .iter()
        .rev()
        .find_map(|event| match event {
            ForgeLifecycleEvent::Planned {
                table: observed,
                inputs,
                ..
            } if observed == &table => Some(inputs.clone()),
            _ => None,
        })
        .expect("table-scoped planned inputs");
    let after = server
        .inspect_forge_table_for_test(cluster.data_tenant_id(), &table)
        .await
        .expect("post-Forge table inspection");
    let rewrite = wyrd_testing::WyrdTestServer::compare_forge_rewrite_for_test(
        &before,
        &after,
        &planned_inputs,
    )
    .expect("exact Forge replacement comparison");
    let delta = cluster
        .telemetry()
        .delta_since(&checkpoint)
        .expect("converged Forge production telemetry delta");
    let report = ForgeCausalTelemetryReport::from_production_delta(&delta)
        .expect("converged Forge causal telemetry report");
    let workflow = server
        .inspect_forge_workflow_for_test(cluster.data_tenant_id(), &table)
        .await
        .expect("converged Forge durable workflow");
    assert_eq!(
        report
            .diagnose(&workflow, Some(&rewrite))
            .expect("Forge telemetry matches converged durable state"),
        ForgeCausalDiagnosis::Converged
    );
    assert!(after.data_file_count() < before.data_file_count());
    assert_eq!(rewrite.input_files.len(), planned_inputs.len());
    assert!(!rewrite.output_files.is_empty());
    assert!(rewrite.input_bytes > 0 && rewrite.output_bytes > 0);
    assert!(
        rewrite
            .output_files
            .iter()
            .all(|file| file.bytes <= after.maximum_healthy_file_bytes)
    );
    assert!(
        rewrite.output_files.len() == 1
            || rewrite
                .output_files
                .iter()
                .all(|file| file.bytes >= after.minimum_healthy_file_bytes)
    );
    assert_eq!(after.active_claims, 0);
    assert_eq!(after.active_attempts, 0);
    assert_eq!(
        server
            .state()
            .forge()
            .expect("Forge composition after rewrite")
            .resources()
            .snapshot()
            .expect("Forge resources after rewrite"),
        forge_baseline
    );
    assert_eq!(
        strict_spill_summary(&reader, &format!("vala.bifrost.{table}"))
            .await
            .expect("post-Forge ordered exact dataset"),
        (1_000_000, 256_000_000)
    );
    cluster
        .shutdown()
        .await
        .expect("Forge convergence shutdown");
}

/// Advances only a durably settled retry so the gated journey need not sleep through backoff.
///
/// The production worker has already consumed and classified the failed attempt;
/// this test-only clock step preserves that durable taxonomy while keeping the
/// convergence proof bounded.
///
/// # Panics
///
/// Panics when the test database cannot update the exact tenant-table retry row.
async fn advance_forge_retry_for_journey(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    table: &str,
) {
    let operator_pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("Forge journey operator pool");
    let pool = operator_pool.pool();
    let mut transaction = pool.begin().await.expect("begin Forge retry clock step");
    let retries = sqlx::query_as::<_, (uuid::Uuid, Option<String>)>(
        "SELECT task_id,failure_class FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND state='retryable' FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(table)
    .fetch_all(&mut *transaction)
    .await
    .expect("load durable Forge retry classification");
    for (task_id, failure_class) in &retries {
        assert!(
            matches!(
                failure_class.as_deref(),
                Some("transient_object_store" | "transient_coordination" | "storage_health")
            ),
            "task {task_id} must have a permitted durable transient class before clock advancement; observed {failure_class:?}"
        );
    }
    let task_ids = retries
        .into_iter()
        .map(|(task_id, _)| task_id)
        .collect::<Vec<_>>();
    if !task_ids.is_empty() {
        let updated = sqlx::query(
            "UPDATE vala.forge_tasks SET ready_at=statement_timestamp(),next_eligible_at=statement_timestamp()-interval '15 minutes' WHERE task_id=ANY($1) AND state='retryable'",
        )
        .bind(&task_ids)
        .execute(&mut *transaction)
        .await
        .expect("advance durable Forge retry eligibility")
        .rows_affected();
        assert_eq!(
            updated,
            u64::try_from(task_ids.len()).expect("retry fixture count fits u64"),
            "locked retry set must remain exact through eligibility advancement"
        );
    }
    transaction
        .commit()
        .await
        .expect("commit Forge retry clock step");
}
