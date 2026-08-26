//! Snapshot expiry and the manifest maintenance that must precede it,
//! including under sustained ingest.
//!
//! Module of the `forge` group; shared fixtures live in `support.rs`.

use std::time::Duration;
use vala_bifrost_redux::forge::{ForgeConfig, ForgeWorkerCompletionObserver};
use vala_sql::queries::forge_tasks::ForgeTasks;
use vala_sql::row_types::forge_tasks::{
    ForgeClaimStrategy, ForgeTaskStrategy, ForgeTaskTableIdentity,
};
use wyrd_testing::WyrdTestServer;

use super::support::*;

/// Runs the real retained-snapshot expiry path and proves one expiry commit occurs.
///
/// # Panics
///
/// Panics when two production staging commits cannot create retained history,
/// when the dedicated SnapshotExpiry task does not complete, or when the
/// authoritative expiry audit is absent.
#[tokio::test]
#[ignore = "gated journey: real Postgres, Forge maintenance, and Iceberg snapshots"]
async fn forge_snapshot_expiry_journey() {
    let server = start_maintenance_journey_server().await;
    let fixture = native_forge_group(&server, "journey_snapshot_expiry").await;
    commit_journey_staging_snapshot(&server, &fixture).await;
    append_native_forge_cycles(&server, &fixture, 3..5).await;
    commit_journey_staging_snapshot(&server, &fixture).await;
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("journey snapshot-expiry table");
    let newest_snapshot_ms = table
        .metadata()
        .snapshots()
        .map(|snapshot| snapshot.timestamp_ms())
        .max()
        .expect("journey snapshot-expiry retained history");
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(newest_snapshot_ms + 2)
                .expect("journey snapshot timestamp is UTC-representable"),
        )
        .expect("advance journey expiry clock");
    let mut config = fixture.config.clone();
    config.snapshot_retention = Duration::from_millis(1);
    config.manifest_rewrite_enabled = true;
    config.min_files = 3;
    config.max_files_per_bin = 3;
    config.max_files_per_tick = 3;
    let mut completed_expiry = false;
    for _ in 0..3 {
        let mut lifecycle = JourneyMaintenance::start(&fixture, config.clone());
        let strategy = lifecycle.run_one_success().await;
        lifecycle.shutdown().await;
        if strategy == ForgeTaskStrategy::SnapshotExpiry {
            completed_expiry = true;
            break;
        }
    }
    assert!(
        completed_expiry,
        "production journey must complete snapshot_expiry"
    );
    assert!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await
            >= 1,
        "production journey must persist an expiry commit audit"
    );
    server
        .shutdown()
        .await
        .expect("journey expiry server shutdown");
}

/// Proves a sustained compaction backlog never starves Forge snapshot expiry.
///
/// The fixture builds real retained Iceberg history through two production
/// staging-fold commits, then seeds an additional pool of uncompacted staging
/// files so a staging-fold compaction candidate stays continuously available on
/// every planning tick. With the maintenance trigger
/// (`maintenance_trigger_snapshot_count` / `maintenance_trigger_interval`) making
/// expiry due independently of that backlog, and the reserved maintenance worker
/// slot claiming maintenance ahead of ready compaction, the journey asserts that
/// the durable production scheduler and worker still complete `SnapshotExpiry`
/// and persist its commit audit while the compaction backlog remains pending.
///
/// # Panics
///
/// Panics when the fixture cannot build retained history, when no compaction
/// candidate is present to contend with maintenance, when the bounded production
/// maintenance loop never completes `SnapshotExpiry`, or when the authoritative
/// expiry commit audit is absent.
#[tokio::test]
#[ignore = "gated journey: real Postgres, sustained compaction backlog, and Forge maintenance"]
async fn sustained_ingest_does_not_starve_snapshot_expiry_journey() {
    let server = start_maintenance_journey_server().await;
    let fixture = native_forge_group(&server, "journey_sustained_ingest_maintenance").await;
    // Two production staging-fold commits create retained Iceberg history so the
    // maintenance trigger has snapshots beyond `retain_last` to expire.
    commit_journey_staging_snapshot(&server, &fixture).await;
    append_native_forge_cycles(&server, &fixture, 3..5).await;
    commit_journey_staging_snapshot(&server, &fixture).await;
    // Seed a sustained compaction backlog: uncompacted staging files keep a
    // staging-fold candidate available on every subsequent planning tick, so
    // maintenance must lead through real compaction contention rather than an
    // idle table.
    append_native_forge_cycles(&server, &fixture, 5..8).await;
    let pending_backlog: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.file_list \
         WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 \
         AND committed_snapshot_id IS NULL",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("sustained compaction backlog count");
    assert!(
        pending_backlog > 0,
        "journey must hold a live staging-fold compaction candidate while maintenance runs"
    );
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("journey sustained-ingest table");
    let newest_snapshot_ms = table
        .metadata()
        .snapshots()
        .map(|snapshot| snapshot.timestamp_ms())
        .max()
        .expect("journey sustained-ingest retained history");
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(newest_snapshot_ms + 2)
                .expect("journey snapshot timestamp is UTC-representable"),
        )
        .expect("advance journey expiry clock");
    let mut config = fixture.config.clone();
    config.snapshot_retention = Duration::from_millis(1);
    config.maintenance_trigger_snapshot_count = 1;
    config.maintenance_trigger_interval = Duration::from_millis(1);
    let mut completed_expiry = false;
    for _ in 0..3 {
        let mut lifecycle = JourneyMaintenance::start(&fixture, config.clone());
        let strategy = lifecycle.run_one_success().await;
        lifecycle.shutdown().await;
        if strategy == ForgeTaskStrategy::SnapshotExpiry {
            completed_expiry = true;
            break;
        }
    }
    assert!(
        completed_expiry,
        "maintenance must complete snapshot_expiry despite a pending compaction backlog"
    );
    assert!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await
            >= 1,
        "sustained-backlog journey must persist an expiry commit audit"
    );
    let remaining_backlog: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.file_list \
         WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 \
         AND committed_snapshot_id IS NULL",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("residual compaction backlog count");
    assert!(
        remaining_backlog > 0,
        "maintenance must lead ahead of the still-pending compaction backlog"
    );
    server
        .shutdown()
        .await
        .expect("journey sustained-ingest server shutdown");
}

/// Proves manifest submission precedes accepted snapshot expiry under one lease.
///
/// The journey holds the real manifest catalog boundary, observes that no
/// expiry terminal audit exists, then releases it and requires the accepted
/// expiry boundary before the worker may complete.
///
/// # Panics
///
/// Panics when retained history cannot be built, expiry reaches acceptance
/// before manifest submission, or the ordered maintenance task fails to settle.
#[tokio::test]
#[ignore = "gated journey: real Postgres and ordered Forge maintenance boundaries"]
async fn supporting_in_process_manifest_maintenance_precedes_expiry() {
    let server = start_maintenance_journey_server().await;
    let fixture = native_forge_group(&server, "journey_manifest_before_expiry").await;
    commit_journey_staging_snapshot(&server, &fixture).await;
    append_native_forge_cycles(&server, &fixture, 3..5).await;
    commit_journey_staging_snapshot(&server, &fixture).await;
    let table_ref = ForgeTaskTableIdentity::new(
        "wyrd-redux",
        &fixture.binding.logical_namespace,
        &fixture.binding.table_name,
    )
    .expect("ordered maintenance task identity");
    ForgeTasks::new(fixture.operator_pool.clone())
        .upsert_periodic(fixture.tenant, &table_ref)
        .await
        .expect("seed no-fit manifest demand");
    let tasks_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1")
            .bind(fixture.tenant.as_uuid())
            .fetch_one(fixture.operator_pool.pool())
            .await
            .expect("ordered maintenance task baseline");
    let mut no_fit = fixture.config.clone();
    no_fit.manifest_rewrite_enabled = true;
    no_fit.manifest_rewrite_target_size_bytes = 8 * 1024 * 1024;
    no_fit.manifest_rewrite_min_count = 2;
    no_fit.max_bytes_per_tick = 1;
    no_fit.maintenance_trigger_snapshot_count = usize::MAX;
    no_fit.maintenance_trigger_interval = Duration::from_hours(24 * 365);
    no_fit.snapshot_retention = Duration::from_hours(24 * 365);
    let mut fit = no_fit.clone();
    fit.max_files_per_tick = 1_000;
    fit.max_bytes_per_tick = fixture.config.max_bytes_per_tick;
    let no_fit_lifecycle = JourneyMaintenance::start(&fixture, no_fit);
    no_fit_lifecycle.run_scheduler_pass().await;
    let demand_retained: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND catalog_name=$2 AND namespace_name=$3 AND table_name=$4)",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&table_ref.catalog)
    .bind(&table_ref.namespace)
    .bind(&table_ref.table)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("observe no-fit manifest demand");
    let tasks_after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1")
            .bind(fixture.tenant.as_uuid())
            .fetch_one(fixture.operator_pool.pool())
            .await
            .expect("ordered maintenance task count after no-fit pass");
    assert!(
        demand_retained,
        "no-fit manifest demand must remain pending"
    );
    assert_eq!(
        tasks_after, tasks_before,
        "no-fit manifest demand must enqueue no task"
    );
    no_fit_lifecycle.shutdown().await;
    let mut manifest_only = JourneyMaintenance::start(&fixture, fit.clone());
    assert_eq!(
        manifest_only.run_one_success().await,
        ForgeTaskStrategy::ManifestRewrite,
        "manifest-only intent must retain its exact durable strategy"
    );
    manifest_only.shutdown().await;
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await,
        0,
        "manifest-only demand must not bypass the independent expiry trigger"
    );
    server
        .forge_clock()
        .advance(chrono::Duration::milliseconds(2))
        .expect("advance ordered maintenance clock");
    let mut config = fixture.config.clone();
    config.snapshot_retention = Duration::from_millis(1);
    config.manifest_rewrite_enabled = true;
    config.manifest_rewrite_target_size_bytes = 8 * 1024 * 1024;
    config.manifest_rewrite_min_count = 2;
    config.maintenance_trigger_snapshot_count = 1;
    config.maintenance_trigger_interval = Duration::from_millis(1);
    config.min_files = 3;
    config.max_files_per_bin = 3;
    config.max_files_per_tick = 3;
    config.max_bytes_per_tick = fit.max_bytes_per_tick;
    let mut lifecycle = JourneyMaintenance::start(&fixture, config);
    let controls = lifecycle.maintenance_controls.clone();
    controls.arm_manifest_submission();
    controls.arm_expiry_accepted();
    let mut attempt = Box::pin(lifecycle.run_one_success_at("ordered-expiry"));
    tokio::select! {
        () = controls.wait_manifest_submission() => {}
        strategy = &mut attempt => panic!("maintenance completed before manifest boundary: {strategy:?}"),
    }
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await,
        0,
        "expiry cannot commit while manifest submission is held"
    );
    controls.release_manifest_submission();
    tokio::select! {
        () = controls.wait_expiry_accepted() => {}
        strategy = &mut attempt => panic!("maintenance completed before expiry acceptance: {strategy:?}"),
    }
    controls.release_expiry_accepted();
    assert_eq!(attempt.await, ForgeTaskStrategy::SnapshotExpiry);
    lifecycle.shutdown().await;
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await,
        1,
        "ordered expiry must terminalize exactly once"
    );
    server
        .shutdown()
        .await
        .expect("ordered maintenance server shutdown");
}

/// Proves the bound supervised Forge owner executes maintenance without a test-owned engine.
///
/// # Panics
///
/// Panics when native ingestion, durable scheduling, or the supervised worker
/// completion boundary fails to settle within the bounded journey window.
#[tokio::test]
#[ignore = "gated journey: real Postgres and supervised Forge maintenance"]
async fn pg_bifrost_forge_manifest_maintenance_precedes_expiry() {
    supervised_temporary_pressure_defers_without_ownership().await;
    supervised_uncertain_commit_recovery_journey().await;
    let observer = ForgeWorkerCompletionObserver::new();
    let config = ForgeConfig {
        lease_ttl: Duration::from_secs(4),
        iceberg_total_retry_timeout: Duration::from_secs(1),
        catalog_request_timeout: Duration::from_secs(1),
        uncertainty_margin: Duration::from_secs(1),
        uncertainty_bound: Duration::from_secs(1),
        manifest_rewrite_enabled: true,
        manifest_rewrite_min_count: 2,
        maintenance_trigger_snapshot_count: 1,
        maintenance_trigger_interval: Duration::from_millis(1),
        ..ForgeConfig::default()
    };
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_config_for_test(config)
        .with_forge_completion_observer_for_test(observer.clone())
        .start_bound()
        .await
        .expect("bound supervised maintenance server");
    let fixture = native_forge_group(&server, "journey_supervised_maintenance").await;
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("close supervised maintenance staging partition");
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
    .expect("supervised maintenance uncompacted partition day");
    assert!(
        latest_uncompacted_partition.is_some_and(|partition_start| {
            partition_start
                < server
                    .forge_clock()
                    .now()
                    .expect("supervised maintenance Forge clock")
        }),
        "all uncompacted maintenance inputs must belong to a closed partition"
    );
    for generation in 0..2 {
        if generation == 1 {
            append_native_forge_cycles(&server, &fixture, 3..4).await;
        }
        let expected = observer.completed().saturating_add(1);
        trigger_supervised_scheduler(
            &server,
            Scenario {
                name: "supervised-maintenance-staging",
                tenants: 1,
            },
        )
        .await;
        tokio::time::timeout(
            Duration::from_secs(30),
            observer.wait_for_at_least(expected),
        )
        .await
        .expect("supervised maintenance staging publication");
        assert!(matches!(
            observer.completed_strategies().last(),
            Some(ForgeClaimStrategy::Known(ForgeTaskStrategy::StagingFold))
        ));
    }
    server
        .forge_clock()
        .advance(chrono::Duration::days(1))
        .expect("age supervised maintenance partition");
    let controls = server
        .forge_maintenance_controls_for_test()
        .expect("supervised maintenance controls");
    controls.arm_manifest_submission();
    controls.arm_expiry_accepted();
    let failed_attempt = observer.attempts().saturating_add(1);
    let expected = observer.completed().saturating_add(1);
    observer.hold_after_next_attempt_for_test();
    server
        .fail_after_forge_maintenance_prepared_for_test()
        .expect("arm supervised maintenance Prepared crash");
    server.trigger_forge_scheduler_for_test();
    tokio::time::timeout(Duration::from_secs(30), controls.wait_manifest_submission())
        .await
        .expect("manifest pre-submission boundary reached before snapshot expiry");
    let (maintenance_task, carrier_plan): (uuid::Uuid, serde_json::Value) = sqlx::query_as(
        "SELECT task_id,plan FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy='snapshot_expiry' AND state='running'",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("exact running maintenance carrier at manifest pre-submission boundary");
    let selected_manifest_paths = carrier_plan["inputs"]
        .as_array()
        .expect("maintenance carrier inputs")
        .iter()
        .map(|path| path.as_str().expect("manifest path input").to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(selected_manifest_paths.len() >= 2);
    let before_table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("load maintenance table before manifest rewrite");
    let before_manifest_facts =
        selected_manifest_facts(&before_table, &selected_manifest_paths, None).await;
    let before_live_paths = current_live_paths(&before_table).await;
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await,
        0,
        "snapshot expiry cannot commit while the manifest future is still lazy"
    );
    controls.release_manifest_submission();
    tokio::time::timeout(Duration::from_secs(30), controls.wait_expiry_accepted())
        .await
        .expect("snapshot expiry accepted boundary");
    controls.release_expiry_accepted();
    tokio::time::timeout(
        Duration::from_secs(30),
        observer.wait_for_held_attempt_for_test(),
    )
    .await
    .expect("maintenance Prepared crash held after returning to supervisor");
    assert_eq!(observer.attempts(), failed_attempt);
    let (prepared_task, claim_expires_at, prepared_watermark, prepared_watermark_ms, prepared_plan, prepared_evidence): (uuid::Uuid, chrono::DateTime<chrono::Utc>, i64, i64, serde_json::Value, serde_json::Value) = sqlx::query_as(
        "SELECT task_id,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,plan,evidence FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy='snapshot_expiry' AND state='prepared'",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("maintenance Prepared takeover evidence");
    assert_eq!(prepared_task, maintenance_task);
    assert_eq!(
        prepared_plan, carrier_plan,
        "Prepared preserves the immutable carrier plan"
    );
    assert!(
        prepared_evidence.is_object(),
        "Prepared preserves committed evidence"
    );
    observer.release_held_attempt_for_test();
    tokio::time::timeout(
        Duration::from_secs(12),
        async {
            loop {
                let expired: bool = sqlx::query_scalar(
                    "SELECT claim_expires_at < statement_timestamp() FROM vala.forge_tasks WHERE task_id=$1 AND state='prepared'",
                )
                .bind(prepared_task)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("observe exact Prepared lease expiry");
                if expired {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            observer.wait_for_at_least(expected).await;
        },
    )
    .await
    .expect("exact Prepared maintenance recovery after PostgreSQL lease expiry");
    let workflow = server
        .inspect_forge_workflow_for_test(fixture.tenant, &fixture.binding.table_name)
        .await
        .expect("supervised maintenance workflow");
    assert_eq!(workflow.active_claims, 0);
    assert_eq!(workflow.active_attempts, 0);
    let strategies = observer.completed_strategies();
    assert!(
        matches!(
            strategies.last(),
            Some(ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry))
        ),
        "one SnapshotExpiry carrier completes both due maintenance effects"
    );
    let guarded: GuardedMaintenanceRow = sqlx::query_as(
        "SELECT (plan->'parameters'->>'manifest_rewrite_due')::boolean,(plan->'parameters'->>'snapshot_expiry_due')::boolean,attempt_id,claimed_by,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,plan,evidence FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND task_id=$3 AND strategy='snapshot_expiry' AND state='succeeded'",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .bind(prepared_task)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("maintenance durable guards");
    assert!(guarded.0, "the carrier preserves manifest rewrite due");
    assert!(guarded.1, "the carrier preserves snapshot expiry due");
    assert_eq!(
        (guarded.2, guarded.3, guarded.4, guarded.5, guarded.6),
        (None, None, None, None, None)
    );
    assert_eq!(
        guarded.7, prepared_plan,
        "terminal carrier plan remains immutable"
    );
    for field in [
        "version",
        "committed_snapshot_id",
        "committed_metadata_location",
        "committed_metadata_digest",
        "cleanup_candidates",
    ] {
        assert_eq!(
            guarded.8[field], prepared_evidence[field],
            "terminal committed evidence field {field} remains immutable"
        );
    }
    let prepared_cleanup_cursor = prepared_evidence["deleted_candidate_count"]
        .as_u64()
        .expect("Prepared cleanup cursor");
    let terminal_cleanup_cursor = guarded.8["deleted_candidate_count"]
        .as_u64()
        .expect("terminal cleanup cursor");
    let cleanup_candidate_count = guarded.8["cleanup_candidates"]
        .as_array()
        .expect("terminal cleanup candidates")
        .len();
    assert_eq!(
        usize::try_from(terminal_cleanup_cursor).expect("bounded terminal cleanup cursor"),
        cleanup_candidate_count,
        "terminal cleanup cursor finalizes the immutable candidate list"
    );
    assert!(
        terminal_cleanup_cursor > prepared_cleanup_cursor,
        "Prepared recovery must durably advance the cleanup cursor"
    );
    assert!(claim_expires_at < chrono::Utc::now());
    assert!(prepared_watermark > 0 && prepared_watermark_ms >= 0);
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy IN ('manifest_rewrite','snapshot_expiry') AND state IN ('ready','claimed','running','retryable','prepared')",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("maintenance active-work guard");
    assert_eq!(active, 0, "maintenance leaves no durable active work");
    let active_table_leases: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.maintenance_leases WHERE lease_key LIKE $1 AND expires_at >= statement_timestamp()",
    )
    .bind(format!("forge:table:{}:%", fixture.tenant))
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("maintenance active table leases");
    assert_eq!(
        active_table_leases, 0,
        "maintenance leaves no active table lease"
    );
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.recovered")
            .await,
        1,
        "lost-response expiry recovery has one recovered audit"
    );
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await,
        0
    );
    let mut audit_conn = fixture
        .vala
        .tenant_conn(fixture.tenant)
        .await
        .expect("maintenance audit tenant connection");
    for operation in ["forge.task.prepared", "forge.task.succeeded"] {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=wyrd.current_tenant() AND operation=$1 AND resource=$2",
        )
        .bind(operation)
        .bind(format!("forge-task:{prepared_task}"))
        .fetch_one(&mut **audit_conn.transaction())
        .await
        .expect("exact maintenance task audit count");
        assert_eq!(count, 1, "{operation} must be emitted exactly once");
    }
    audit_conn
        .commit()
        .await
        .expect("close maintenance audit read");
    let after_table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("load maintenance table after rewrite and expiry");
    let after_manifest_facts = selected_manifest_facts(
        &after_table,
        &selected_manifest_paths,
        Some(before_manifest_facts.2),
    )
    .await;
    assert!(
        after_manifest_facts.0 < before_manifest_facts.0,
        "selected same-spec manifest cardinality must decrease"
    );
    assert_eq!(
        after_manifest_facts.1, before_manifest_facts.1,
        "manifest rewrite preserves selected-bin live files"
    );
    assert_eq!(
        current_live_paths(&after_table).await,
        before_live_paths,
        "maintenance preserves all live data-file membership"
    );
    let retained = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("load retained maintenance history")
        .metadata()
        .snapshots()
        .count();
    assert!(
        retained >= 2,
        "retention keeps the current snapshot and retained history"
    );
    let settled_attempts = observer.attempts();
    trigger_supervised_scheduler(
        &server,
        Scenario {
            name: "supervised-maintenance-no-work",
            tenants: 1,
        },
    )
    .await;
    assert_eq!(
        observer.attempts(),
        settled_attempts,
        "a settled maintenance table does not loop on no work"
    );
    server
        .shutdown()
        .await
        .expect("maintenance server shutdown");
}

/// Returns selected same-spec manifest cardinality and live file membership.
async fn selected_manifest_facts(
    table: &iceberg::table::Table,
    selected_paths: &std::collections::BTreeSet<String>,
    known_partition_spec: Option<i32>,
) -> (usize, std::collections::BTreeSet<String>, i32) {
    let snapshot = table
        .metadata()
        .current_snapshot()
        .expect("maintenance current snapshot");
    let manifest_list = table
        .manifest_list_reader(snapshot)
        .load()
        .await
        .expect("maintenance manifest list");
    let selected = manifest_list
        .entries()
        .iter()
        .filter(|manifest| selected_paths.contains(&manifest.manifest_path))
        .collect::<Vec<_>>();
    let partition_spec = known_partition_spec.unwrap_or_else(|| {
        selected
            .first()
            .expect("selected manifests remain visible before rewrite")
            .partition_spec_id
    });
    assert!(
        selected
            .iter()
            .all(|manifest| manifest.partition_spec_id == partition_spec)
    );
    let mut live_paths = std::collections::BTreeSet::new();
    for manifest_file in manifest_list
        .entries()
        .iter()
        .filter(|manifest| manifest.partition_spec_id == partition_spec)
    {
        let manifest = manifest_file
            .load_manifest(table.file_io())
            .await
            .expect("selected-spec manifest");
        live_paths.extend(
            manifest
                .entries()
                .iter()
                .filter(|entry| entry.is_alive())
                .map(|entry| entry.file_path().to_owned()),
        );
    }
    let cardinality = manifest_list
        .entries()
        .iter()
        .filter(|manifest| manifest.partition_spec_id == partition_spec)
        .count();
    (cardinality, live_paths, partition_spec)
}

/// One row of the manifest-maintenance guard projection.
///
/// `sqlx::query_as` needs the column tuple spelled out, and this projection
/// reads nine columns spanning both maintenance-due flags, claim ownership,
/// watermark identity, and the persisted plan and evidence documents. Naming it
/// keeps the assertion readable and gives the shape one place to change.
type GuardedMaintenanceRow = (
    bool,
    bool,
    Option<uuid::Uuid>,
    Option<uuid::Uuid>,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<i64>,
    Option<i64>,
    serde_json::Value,
    serde_json::Value,
);
