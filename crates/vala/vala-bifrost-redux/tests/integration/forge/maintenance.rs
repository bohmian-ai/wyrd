//! Maintenance transitions: planning under backlog, submission and
//! cancellation ordering, commit timeout recovery, and evidenced expiry.
//!
//! Module of the `forge` group; shared fixtures live in `support.rs`.
//!
//! The tests stay wrapped in `mod pg_tests` so the fast lane's `--skip pg_tests`
//! filter still excludes them.

mod pg_tests {

    use std::sync::Arc;
    use std::time::Duration;

    use opendal::Buffer;

    use tokio_util::sync::CancellationToken;

    use vala_bifrost_redux::forge::{
        ForgeConfig, ForgeError, ForgeScheduler, ForgeWorker, ForgeWorkerConfig,
        deterministic_output_path_for_test, forge_lease_key,
    };

    use vala_sql::queries::forge_tasks::ForgeTasks;
    use vala_sql::row_types::forge_operations::ForgeOperationFamily;
    use vala_sql::row_types::forge_tasks::{
        FORGE_TASK_PAYLOAD_VERSION, ForgeClaimStrategy, ForgeTaskPlan, ForgeTaskStrategy,
        ForgeTaskTableIdentity, NewForgeTask,
    };

    use wyrd_spec::vala::api::ForgeCompactionPhase;

    use crate::forge::support::*;

    /// Periodic expiry and orphan collection write exact state/audit parity.
    #[tokio::test]
    async fn destructive_maintenance_transitions_preserve_audit_projection_parity() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                max_concurrent_reads: 2,
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;

        let first = fixture.schedule_and_execute().await;
        assert_eq!(first.tasks_enqueued, 1, "first outcome: {first:?}");
        fixture.seed_files_at(100, 4, true).await;
        let second = fixture.schedule_and_execute().await;
        assert_eq!(second.tasks_enqueued, 1, "second outcome: {second:?}");

        let (expired_candidates, pending_terminals) = fixture
            .forge
            .run_snapshot_expiry_commit_boundary_for_test(&fixture.binding)
            .await
            .expect("post-commit evidence boundary");
        assert!(
            expired_candidates > 0,
            "exact cleanup candidates survive commit"
        );
        assert_eq!(
            pending_terminals, 1,
            "expiry stays Prepared before task evidence"
        );
        fixture
            .forge
            .run_snapshot_expiry_for_test(&fixture.binding)
            .await
            .expect("snapshot expiry recovery pass");
        let orphan = deterministic_output_path_for_test(
            &fixture.binding.object_prefix,
            uuid::Uuid::now_v7(),
            0,
        );
        fixture
            .staging
            .write(&orphan, Buffer::from(vec![1_u8]))
            .await
            .expect("destructive maintenance orphan");
        let resource = format!(
            "bifrost://{}/{}/{}",
            fixture.tenant, fixture.binding.table_ref.namespace, fixture.binding.table_ref.name
        );
        let operation_id = uuid::Uuid::now_v7();
        let prepared = operation_event(
            "forge.file_compact.prepared",
            &resource,
            reset_detail_for_output(
                operation_id,
                &resource,
                ForgeCompactionPhase::Prepared,
                &orphan,
            ),
        );
        append_operation(&fixture, ForgeOperationFamily::StagingFold, &prepared, true).await;
        let reset = operation_event(
            "forge.file_compact.reset",
            &resource,
            reset_detail_for_output(
                operation_id,
                &resource,
                ForgeCompactionPhase::Reset,
                &orphan,
            ),
        );
        append_operation(&fixture, ForgeOperationFamily::StagingFold, &reset, false).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        fixture
            .forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("orphan collection pass");

        assert_terminal_family_parity(&fixture, "snapshot_expire").await;
        assert_terminal_family_parity(&fixture, "orphan_gc").await;
    }

    /// A due maintenance trigger leads planning even while a live compaction
    /// candidate is present, proving snapshot expiry is not starved by sustained
    /// compaction load (AC1).
    ///
    /// Warm-up runs two ordinary compaction-and-execute cycles under a
    /// one-nanosecond trigger interval: during each warm-up plan the table holds
    /// at most `retain_last` snapshots, so `commits_since_last_maintenance` is
    /// zero and the trigger does not fire mid-warm-up. Only after the second
    /// commit does the interval arm become due. Fresh staging files are then
    /// seeded and deliberately NOT right-sized away (the contrast with
    /// [`prepare_maintenance_claim`], which drains compaction so expiry wins
    /// through the both-empty fallback), so a live compaction candidate coexists
    /// with the due maintenance candidate on the planned tick; the claimed
    /// strategy proves maintenance took the tick's single slot ahead of it.
    #[tokio::test]
    async fn maintenance_leads_planning_amid_compaction_backlog() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        fixture.schedule_and_execute().await;
        fixture.seed_files_at(100, 4, true).await;
        fixture.schedule_and_execute().await;
        // Fresh, undrained staging files keep a live compaction candidate for
        // this tick; the maintenance-family fixtures right-size to drain it, this
        // one does not, so both candidate classes are present when planning runs.
        fixture.seed_files_at(200, 4, true).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        let identity = ForgeTaskTableIdentity::new(
            "wyrd-redux",
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        )
        .expect("maintenance identity");
        ForgeTasks::new(fixture.operator_pool.clone())
            .upsert_periodic(fixture.tenant, &identity)
            .await
            .expect("periodic maintenance demand");
        let claim = fixture.plan_and_claim().await;
        assert!(
            matches!(
                claim.strategy,
                ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry)
            ),
            "a due maintenance trigger must lead planning ahead of a live compaction candidate: {:?}",
            claim.strategy
        );
    }

    /// A durable no-op acknowledgement prevents a still-due trigger from starving compaction.
    #[tokio::test]
    async fn acknowledged_noop_maintenance_allows_staging_debt_to_converge() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        fixture.schedule_and_execute().await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("no-op table");
        let snapshot = table.metadata().current_snapshot().expect("no-op snapshot");
        let manifest = format!("{}/metadata/missing.avro", table.metadata().location());
        let mut no_op_task = fixture.durable_task(
            ForgeTaskStrategy::SnapshotExpiry,
            ForgeTaskPlan {
                version: FORGE_TASK_PAYLOAD_VERSION,
                inputs: vec![manifest],
                parameters: serde_json::json!({"kind":"maintenance","trigger_commit_count":0}),
            },
            219,
        );
        no_op_task.base_snapshot_id = snapshot.snapshot_id();
        ForgeTasks::new(fixture.operator_pool.clone())
            .enqueue(&no_op_task)
            .await
            .expect("no-op task");
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("no-op claim")
            .expect("no-op task claim");
        fixture
            .sibling_worker_with_config(ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_hours(24 * 365),
                orphan_gc_ttl: Duration::from_nanos(1),
                ..ForgeConfig::default()
            })
            .execute_claim(claim, &CancellationToken::new())
            .await
            .expect("production no-op settlement");
        fixture.seed_files_at(500, 4, true).await;

        let mut remaining = 4_i64;
        for _ in 0..4 {
            fixture.schedule_and_execute().await;
            remaining = sqlx::query_scalar("SELECT count(*) FROM vala.file_list WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 AND NOT compacted")
                .bind(fixture.tenant.as_uuid())
                .bind(&fixture.binding.logical_namespace)
                .bind(&fixture.binding.table_name)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("remaining staging debt");
            if remaining == 0 {
                break;
            }
        }
        assert_eq!(
            remaining, 0,
            "acknowledged no-op must not starve staging debt"
        );
    }

    /// A table whose manifest history exceeds `max_concurrent_reads` plans a
    /// schedulable expiry on successive bounded ticks instead of wedging in the
    /// terminal `Unschedulable` lane or pinning one idempotent plan (AC3).
    ///
    /// The regression is the deep-history wedge, which lives in the planning
    /// layer: before the parallelism bound, `maintenance_candidate` set
    /// `parallelism = manifest_count`, so a table with more retained manifests
    /// than `max_concurrent_reads` (here `1`) planned a candidate whose
    /// parallelism exceeded `max_parallelism` and whose input count was not one,
    /// classifying it `Unschedulable` on every tick — a permanent stall. This
    /// proof is deliberately plan-only (`schedule_once`, never executed): the
    /// wedge is the capacity classification, and executing synthetic
    /// fast-appended history would exercise unrelated expiry ancestry mechanics.
    /// The first tick must plan a schedulable expiry (`unschedulable == 0`,
    /// `tasks_enqueued == 1`); a second tick, after fresh manifest history moves
    /// the current snapshot, must again plan schedulable and advance to a distinct
    /// `plan_hash`, proving successive bounded ticks progress rather than pinning
    /// the retired plan.
    #[tokio::test]
    async fn deep_manifest_history_progresses_without_wedging() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                max_concurrent_reads: 1,
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            true,
            9,
            fixture_snapshot(),
        )
        .await;
        // Build a deep manifest history directly, one fast-append per seeded
        // object, so the current snapshot's manifest list has six entries —
        // strictly more than `max_concurrent_reads`.
        for index in 0..6 {
            let table = fixture
                .catalog
                .load_table(&fixture.binding.table_ident())
                .await
                .expect("historical table load");
            fixture.append_seed_manifest(&table, index).await;
        }
        // Drop the staging file-list rows so only the manifest history drives
        // planning; no compaction candidate competes with maintenance here.
        fixture.delete_file_list_history().await;
        let snapshots_before = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("deep-history table")
            .metadata()
            .snapshots()
            .count();
        // Deeper than the configured `max_concurrent_reads` of 1, so the
        // pre-bound parallelism would have exceeded `max_parallelism`.
        assert!(
            snapshots_before > 1,
            "fixture must build manifest history deeper than max_concurrent_reads: {snapshots_before}"
        );
        let identity = ForgeTaskTableIdentity::new(
            "wyrd-redux",
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        )
        .expect("deep-history identity");
        let tasks = ForgeTasks::new(fixture.operator_pool.clone());
        let stop = CancellationToken::new();
        tokio::time::sleep(Duration::from_millis(5)).await;
        tasks
            .upsert_periodic(fixture.tenant, &identity)
            .await
            .expect("first periodic maintenance demand");
        let first = ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
            .expect("first-tick scheduler")
            .schedule_once(&stop)
            .await
            .expect("first bounded planning tick");
        assert_eq!(
            first.unschedulable, 0,
            "deep manifest history must not wedge in the Unschedulable lane: {first:?}"
        );
        assert_eq!(
            first.tasks_enqueued, 1,
            "the first bounded tick must plan exactly one schedulable expiry: {first:?}"
        );

        // Append fresh manifest history so the second bounded tick plans from a
        // new current snapshot rather than re-deriving the first plan.
        for index in 6..9 {
            let table = fixture
                .catalog
                .load_table(&fixture.binding.table_ident())
                .await
                .expect("second-round table load");
            fixture.append_seed_manifest(&table, index).await;
        }
        fixture.delete_file_list_history().await;
        make_current_files_right_sized(&fixture).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        tasks
            .upsert_periodic(fixture.tenant, &identity)
            .await
            .expect("second periodic maintenance demand");
        let second = ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
            .expect("second-tick scheduler")
            .schedule_once(&stop)
            .await
            .expect("second bounded planning tick");
        assert_eq!(
            second.unschedulable, 0,
            "the second bounded tick must also be schedulable: {second:?}"
        );
        assert_eq!(
            second.tasks_enqueued, 1,
            "the second bounded tick must plan exactly one schedulable expiry: {second:?}"
        );
        let distinct_plans: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT plan_hash) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND strategy='snapshot_expiry'",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("distinct maintenance plans");
        assert_eq!(
            distinct_plans, 2,
            "successive bounded ticks must plan distinct expiries, not pin one idempotent plan"
        );
    }

    /// Authority loss injected at a maintenance catalog boundary.
    ///
    /// Graceful shutdown is no longer a member: it no longer reaches the
    /// maintenance authority token, so its complete-through behavior is proven
    /// separately by [`assert_maintenance_shutdown_completes_through`].
    #[derive(Clone, Copy)]
    enum MaintenanceAuthorityLoss {
        /// Expire the durable task claim and let its heartbeat cancel work.
        Claim,
        /// Replace the table lease generation and let its heartbeat cancel work.
        TableLease,
    }

    /// Verifies that cancellation at a maintenance boundary published no durable effect.
    ///
    /// # Panics
    ///
    /// Panics when the audit, task-evidence, or catalog queries fail, or when
    /// cancellation left a durable transition, cleanup evidence, changed table
    /// metadata, an output write, or an object-deletion attempt behind.
    async fn assert_cancelled_maintenance_state(
        fixture: &Fixture,
        task_id: uuid::Uuid,
        metadata_before: &str,
        snapshot_before: Option<i64>,
        output_puts_before: usize,
        deletes_before: usize,
    ) {
        let (states, audits) =
            family_transition_counts(fixture, "snapshot_expire", "forge.snapshot_expire.").await;
        assert_eq!(states, 0);
        assert_eq!(audits, 0);
        let evidence: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT evidence FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("cancelled maintenance evidence");
        assert!(
            evidence
                .as_ref()
                .and_then(|value| value.get("cleanup_candidates"))
                .is_none()
        );
        let table_after = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("post-cancellation maintenance table");
        assert_eq!(
            table_after.metadata_location(),
            Some(metadata_before),
            "catalog metadata changed across the cancellation boundary"
        );
        assert_eq!(
            table_after.metadata().current_snapshot_id(),
            snapshot_before,
            "current snapshot changed across the cancellation boundary"
        );
        assert_eq!(fixture.reads.output_put_calls(), output_puts_before);
        assert_eq!(fixture.reads.total_delete_attempts(), deletes_before);
    }

    /// Exercises one deterministic maintenance boundary under every loss source.
    ///
    /// Authority loss (claim expiry, table-lease theft) and graceful shutdown
    /// now diverge: the heartbeat cancels the maintenance authority token only
    /// on authority loss, so those two variants still stop at the boundary with
    /// nothing durable crossing it, while shutdown no longer reaches that token
    /// and the operation completes through to a recoverable terminal.
    async fn assert_maintenance_boundary_loss(expiry_submission: bool) {
        for loss in [
            MaintenanceAuthorityLoss::Claim,
            MaintenanceAuthorityLoss::TableLease,
        ] {
            Box::pin(assert_maintenance_authority_loss(expiry_submission, loss)).await;
        }
        Box::pin(assert_maintenance_shutdown_completes_through(
            expiry_submission,
        ))
        .await;
    }

    /// Stops one maintenance boundary on genuine authority loss with no durable effect.
    ///
    /// The armed boundary is driven to ARRIVE (raced against the worker join so a
    /// premature return surfaces its `Result` immediately instead of a
    /// misattributed timeout), then the chosen authority is revoked; the
    /// heartbeat propagates the loss to the maintenance authority token, the
    /// gate releases via cancellation, and the claim retains no effect past the
    /// boundary.
    async fn assert_maintenance_authority_loss(
        expiry_submission: bool,
        loss: MaintenanceAuthorityLoss,
    ) {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                manifest_rewrite_enabled: true,
                manifest_rewrite_min_count: 2,
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        let claim = prepare_maintenance_claim(&fixture).await;
        let task_id = claim.task_id;
        let table_before = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("pre-cancellation maintenance table");
        let metadata_before = table_before
            .metadata_location()
            .expect("pre-cancellation metadata location")
            .to_owned();
        let snapshot_before = table_before.metadata().current_snapshot_id();
        let output_puts_before = fixture.reads.output_put_calls();
        let deletes_before = fixture.reads.total_delete_attempts();
        let controls = fixture.forge.maintenance_controls_for_test();
        if expiry_submission {
            controls.arm_expiry_submission();
        } else {
            controls.arm_manifest_submission();
        }
        let stop = CancellationToken::new();
        let execution = tokio::spawn({
            let worker = fixture.worker.clone();
            let stop = stop.clone();
            async move { worker.execute_claim(claim, &stop).await }
        });
        tokio::pin!(execution);
        let arrival = async {
            if expiry_submission {
                controls.wait_expiry_submission().await;
            } else {
                controls.wait_manifest_submission().await;
            }
        };
        tokio::pin!(arrival);
        tokio::select! {
            () = &mut arrival => {}
            result = &mut execution => {
                panic!("maintenance returned before catalog boundary: {result:?}")
            }
        }
        match loss {
            MaintenanceAuthorityLoss::Claim => {
                sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1")
                    .bind(task_id).execute(fixture.operator_pool.pool()).await.expect("expire maintenance claim");
            }
            MaintenanceAuthorityLoss::TableLease => {
                sqlx::query("UPDATE vala.maintenance_leases SET owner=$2,fencing_token=fencing_token+1 WHERE lease_key=$1")
                    .bind(forge_lease_key(fixture.tenant, &fixture.binding.logical_namespace, &fixture.binding.table_name))
                    .bind(uuid::Uuid::now_v7()).execute(fixture.operator_pool.pool()).await.expect("steal maintenance lease");
            }
        }
        let result = (&mut execution).await.expect("maintenance worker join");
        assert!(result.is_err(), "authority loss must stop maintenance");
        assert_cancelled_maintenance_state(
            &fixture,
            task_id,
            &metadata_before,
            snapshot_before,
            output_puts_before,
            deletes_before,
        )
        .await;
    }

    /// Recovers an expired maintenance claim on a fresh successor worker and
    /// asserts the recovery reaches a KNOWN terminal without re-committing.
    ///
    /// Expires the durable claim, runs one successor pass, and proves the
    /// recovered metadata location is byte-identical to `operation_location`
    /// (no second manifest commit) while the durable task state advances to
    /// `succeeded`. Shared by the shutdown and interrupted-attempt scenarios so
    /// each caller stays a focused, readable assertion.
    ///
    /// # Panics
    /// Panics when the successor worker cannot be built, the recovery pass does
    /// not report progress, the recovered location differs from
    /// `operation_location`, or the durable state is not `succeeded`. These are
    /// test-environment invariants.
    async fn assert_successor_recovers_to_known_terminal(
        fixture: &Fixture,
        task_id: uuid::Uuid,
        operation_location: &str,
    ) {
        sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1")
            .bind(task_id).execute(fixture.operator_pool.pool()).await.expect("expire shutdown task claim");
        let successor = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("successor worker");
        assert!(
            successor
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("shutdown maintenance recovery")
        );
        let recovered_location = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("recovered maintenance table")
            .metadata_location()
            .expect("recovered metadata location")
            .to_owned();
        assert_eq!(
            recovered_location, operation_location,
            "successor must not perform a second manifest commit"
        );
        let terminal: String =
            sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("recovered terminal state");
        assert_eq!(
            terminal, "succeeded",
            "successor must drive the claim to a known terminal"
        );
    }

    /// A graceful shutdown does not cancel an in-flight maintenance operation.
    ///
    /// Shutdown cancels only the shutdown-sensitive `operation_stop`; the
    /// maintenance authority token is untouched, so once the boundary is
    /// released the operation runs to its own outcome. The post-effect drain
    /// then returns without terminalizing the attempt, leaving the durable
    /// effect committed exactly once and the claim recoverable. A successor
    /// pass reaches the KNOWN terminal without a second manifest commit.
    async fn assert_maintenance_shutdown_completes_through(expiry_submission: bool) {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                manifest_rewrite_enabled: true,
                manifest_rewrite_min_count: 2,
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        let claim = prepare_maintenance_claim(&fixture).await;
        let task_id = claim.task_id;
        let table_before = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("pre-shutdown maintenance table");
        let snapshot_before = table_before.metadata().current_snapshot_id();
        let controls = fixture.forge.maintenance_controls_for_test();
        if expiry_submission {
            controls.arm_expiry_submission();
        } else {
            controls.arm_manifest_submission();
        }
        let stop = CancellationToken::new();
        let execution = tokio::spawn({
            let worker = fixture.worker.clone();
            let stop = stop.clone();
            async move { worker.execute_claim(claim, &stop).await }
        });
        tokio::pin!(execution);
        let arrival = async {
            if expiry_submission {
                controls.wait_expiry_submission().await;
            } else {
                controls.wait_manifest_submission().await;
            }
        };
        tokio::pin!(arrival);
        tokio::select! {
            () = &mut arrival => {}
            result = &mut execution => {
                panic!("maintenance returned before catalog boundary: {result:?}")
            }
        }
        stop.cancel();
        if expiry_submission {
            controls.release_expiry_submission();
        } else {
            controls.release_manifest_submission();
        }
        let result = (&mut execution).await.expect("maintenance worker join");
        assert!(
            result.is_err(),
            "post-effect shutdown retains the claim for recovery: {result:?}"
        );
        let operation_location = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("post-operation maintenance table")
            .metadata_location()
            .expect("post-operation metadata location")
            .to_owned();
        // (a) The maintenance operation committed exactly once through shutdown.
        let table_after_operation = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("post-operation maintenance table snapshot");
        assert_ne!(
            table_after_operation.metadata().current_snapshot_id(),
            snapshot_before,
            "maintenance must complete its commit through graceful shutdown"
        );
        // (b)/(c) A successor recovers the claim to a KNOWN terminal without a
        // second manifest commit.
        assert_successor_recovers_to_known_terminal(&fixture, task_id, &operation_location).await;
    }

    /// Manifest submission stops before expiry under claim, lease, and shutdown loss.
    #[tokio::test]
    async fn maintenance_manifest_submission_cancellation_matrix_is_effect_ordered() {
        Box::pin(assert_maintenance_boundary_loss(false)).await;
    }

    /// Expiry submission preserves only its Prepared evidence under every cancellation source.
    #[tokio::test]
    async fn maintenance_expiry_submission_cancellation_matrix_is_effect_ordered() {
        Box::pin(assert_maintenance_boundary_loss(true)).await;
    }

    /// A manifest commit that outlives its retry timeout retains the claim for recovery.
    ///
    /// This pins the timeout backstop that bounds the now-shutdown-decoupled
    /// maintenance operation: a `1ns` `iceberg_total_retry_timeout` elapses on
    /// the first poll of the real catalog commit (which must yield for IO),
    /// mapping to the unknown-acceptance `Reconciliation` and leaving nothing
    /// durable behind, so the claim stays retained for lease-based recovery.
    ///
    /// The fixture itself keeps a normal retry timeout so its `StagingFold` setup
    /// commits succeed; the tightened `1ns` timeout applies only to the sibling
    /// worker that executes the already-taken maintenance claim, so the timeout
    /// is exercised exactly where the assertion targets it — the maintenance
    /// manifest rewrite — and never corrupts claim preparation.
    #[tokio::test]
    async fn maintenance_commit_timeout_retains_claim_for_recovery() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                manifest_rewrite_enabled: true,
                manifest_rewrite_min_count: 2,
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        let claim = prepare_maintenance_claim(&fixture).await;
        let task_id = claim.task_id;
        let table_before = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("pre-timeout maintenance table");
        let metadata_before = table_before
            .metadata_location()
            .expect("pre-timeout metadata location")
            .to_owned();
        let snapshot_before = table_before.metadata().current_snapshot_id();
        let output_puts_before = fixture.reads.output_put_calls();
        let deletes_before = fixture.reads.total_delete_attempts();
        let tight = fixture.sibling_worker_with_retry_timeout(Duration::from_nanos(1));
        let stop = CancellationToken::new();
        let result = tight.execute_claim(claim, &stop).await;
        assert!(
            matches!(result, Err(ForgeError::Reconciliation { .. })),
            "manifest commit timeout must map to unknown-acceptance Reconciliation: {result:?}"
        );
        assert_cancelled_maintenance_state(
            &fixture,
            task_id,
            &metadata_before,
            snapshot_before,
            output_puts_before,
            deletes_before,
        )
        .await;
    }

    /// An accepted expiry cancelled before response processing is recovered exactly once.
    #[tokio::test]
    async fn maintenance_accepted_then_cancel_recovers_without_duplicate_catalog_effect() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        let claim = prepare_maintenance_claim(&fixture).await;
        let task_id = claim.task_id;
        let controls = fixture.forge.maintenance_controls_for_test();
        controls.arm_expiry_accepted();
        let stop = CancellationToken::new();
        let execution = tokio::spawn({
            let worker = fixture.worker.clone();
            let stop = stop.clone();
            async move { worker.execute_claim(claim, &stop).await }
        });
        tokio::time::timeout(Duration::from_secs(30), controls.wait_expiry_accepted())
            .await
            .expect("accepted expiry response");
        let accepted_location = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("accepted expiry table")
            .metadata_location()
            .expect("accepted metadata location")
            .to_owned();
        // Graceful shutdown no longer cancels an in-flight maintenance operation;
        // revoke the claim so the heartbeat propagates authority loss to the
        // maintenance authority token and releases the accepted-response gate via
        // cancellation. All assertions below are unchanged.
        sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1")
            .bind(task_id).execute(fixture.operator_pool.pool()).await.expect("expire accepted maintenance claim");
        let result = tokio::time::timeout(Duration::from_secs(30), execution)
            .await
            .expect("accepted cancellation bound")
            .expect("accepted cancellation join");
        // Authority loss is detected by the claim heartbeat, and `execute_fenced`
        // surfaces the heartbeat's fence conflict (`heartbeat_result?`) ahead of
        // the maintenance operation's own unknown-acceptance `Reconciliation`
        // (`completion?`). Because the heartbeat is the sole canceller of the
        // maintenance authority token, its fence conflict is the deterministic
        // surfaced variant here; both are non-success authority-loss outcomes
        // that retain the accepted expiry effect for exactly-once successor
        // recovery, which the assertions below prove.
        assert!(
            matches!(
                result,
                Err(ForgeError::Sql(_) | ForgeError::Reconciliation { .. })
            ),
            "interrupted accepted-expiry attempt must fail with an authority-loss error that retains the effect: {result:?}"
        );
        let (states, audits) =
            family_transition_counts(&fixture, "snapshot_expire", "forge.snapshot_expire.").await;
        assert_eq!((states, audits), (1, 1));
        sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1")
            .bind(task_id).execute(fixture.operator_pool.pool()).await.expect("expire accepted task claim");
        let successor = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("successor worker");
        assert!(
            !successor
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("accepted expiry reclaim"),
            "an expired accepted attempt is reclaimed into durable backoff"
        );
        sqlx::query(
            "UPDATE vala.forge_tasks SET next_eligible_at=statement_timestamp()-interval '1 second',ready_at=statement_timestamp()-interval '1 second' WHERE task_id=$1",
        )
        .bind(task_id)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("advance accepted recovery eligibility");
        assert!(
            successor
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("accepted expiry recovery after backoff")
        );
        let recovered_location = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("recovered expiry table")
            .metadata_location()
            .expect("recovered metadata location")
            .to_owned();
        assert_eq!(recovered_location, accepted_location);
        assert_eq!(
            family_transition_counts(&fixture, "snapshot_expire", "forge.snapshot_expire.",).await,
            (1, 2)
        );
        assert_completed_cleanup_evidence(&fixture).await;
        assert_exact_cleanup_attempts(&fixture, task_id).await;
    }

    /// Proves recovery deletes every durable cleanup candidate exactly once.
    async fn assert_exact_cleanup_attempts(fixture: &Fixture, task_id: uuid::Uuid) {
        let evidence: serde_json::Value =
            sqlx::query_scalar("SELECT evidence FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("accepted expiry cleanup evidence");
        let candidates = evidence["cleanup_candidates"]
            .as_array()
            .expect("accepted cleanup candidates");
        assert!(!candidates.is_empty());
        for candidate in candidates {
            let object_path = candidate["path"].as_str().expect("cleanup candidate path");
            assert_eq!(
                fixture.reads.delete_attempts_for(object_path),
                1,
                "cleanup candidate must be attempted exactly once: {object_path}"
            );
        }
        assert_eq!(
            fixture.reads.total_delete_attempts(),
            candidates.len(),
            "successor must not attempt deletes outside exact cleanup evidence"
        );
    }

    /// A persisted active watermark detached from every Iceberg ref fails before effects.
    #[tokio::test]
    async fn persisted_detached_watermark_rejects_expiry_before_prepared_or_cleanup() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        fixture.schedule_and_execute().await;
        fixture.seed_files_at(100, 4, true).await;
        fixture.schedule_and_execute().await;
        let (detached_id, detached_timestamp) = fixture.persist_detached_prior_snapshot().await;
        let detached_table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("persisted detached table");
        let metadata_before = detached_table
            .metadata_location()
            .expect("detached metadata location")
            .to_owned();
        let current_before = detached_table.metadata().current_snapshot_id();
        let task_id = ForgeTasks::new(fixture.operator_pool.clone())
            .enqueue(&NewForgeTask {
                base_snapshot_id: detached_id,
                ..fixture.durable_task(
                    ForgeTaskStrategy::StagingFold,
                    ForgeTaskPlan {
                        version: FORGE_TASK_PAYLOAD_VERSION,
                        inputs: vec!["detached-watermark.parquet".to_owned()],
                        parameters: serde_json::json!({"kind":"staging_fold"}),
                    },
                    221,
                )
            })
            .await
            .expect("detached watermark task");
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("detached watermark claim query")
            .expect("detached watermark claim");
        assert_eq!(claim.task_id, task_id);
        sqlx::query(
            "UPDATE vala.forge_tasks SET state='running',watermark_snapshot_id=$2,watermark_timestamp_ms=$3 WHERE task_id=$1",
        )
        .bind(task_id)
        .bind(detached_id)
        .bind(detached_timestamp)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("activate detached persisted watermark");
        let result = fixture
            .forge
            .run_snapshot_expiry_for_test(&fixture.binding)
            .await;
        assert!(
            matches!(result, Err(ForgeError::SnapshotExpiry { ref detail }) if detail.contains("detached")),
            "{result:?}"
        );
        assert_eq!(
            family_transition_counts(&fixture, "snapshot_expire", "forge.snapshot_expire.",).await,
            (0, 0)
        );
        let after = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("unchanged detached table");
        assert_eq!(after.metadata_location(), Some(metadata_before.as_str()));
        assert_eq!(after.metadata().current_snapshot_id(), current_before);
        let evidence: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT evidence FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("detached watermark cleanup evidence");
        assert!(evidence.is_none());
    }

    /// Checks completed cleanup evidence against object storage and live metadata.
    async fn assert_completed_cleanup_evidence(fixture: &Fixture) {
        let evidence: serde_json::Value = sqlx::query_scalar("SELECT evidence FROM vala.forge_tasks WHERE data_tenant_id=$1 AND catalog_name='wyrd-redux' AND namespace_name=$2 AND table_name=$3 AND strategy='snapshot_expiry' AND state='succeeded' ORDER BY updated_at DESC LIMIT 1")
            .bind(fixture.tenant.as_uuid()).bind(&fixture.binding.logical_namespace).bind(&fixture.binding.table_name)
            .fetch_one(fixture.operator_pool.pool()).await.expect("maintenance evidence");
        let candidates = evidence["cleanup_candidates"]
            .as_array()
            .expect("candidates");
        assert!(!candidates.is_empty());
        assert_eq!(
            evidence["deleted_candidate_count"].as_u64(),
            Some(candidates.len() as u64)
        );
        let paths = candidates
            .iter()
            .map(|candidate| candidate["path"].as_str().expect("path").to_owned())
            .collect::<Vec<_>>();
        assert!(paths.windows(2).all(|pair| pair[0] < pair[1]));
        for path in &paths {
            assert!(
                fixture.staging.stat(path).await.is_err(),
                "still exists: {path}"
            );
        }
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("table");
        let current = table.metadata_location().expect("metadata location");
        assert!(paths.iter().all(|path| !current.ends_with(path)));
    }

    /// Proves a later Reset-backed destructive pass remains available after takeover.
    async fn assert_later_destructive_maintenance(fixture: &Fixture, resource: &str) {
        let orphan = deterministic_output_path_for_test(
            &fixture.binding.object_prefix,
            uuid::Uuid::now_v7(),
            0,
        );
        fixture
            .staging
            .write(&orphan, Buffer::from(vec![1_u8]))
            .await
            .expect("orphan");
        let operation_id = uuid::Uuid::now_v7();
        let prepared = operation_event(
            "forge.file_compact.prepared",
            resource,
            reset_detail_for_output(
                operation_id,
                resource,
                ForgeCompactionPhase::Prepared,
                &orphan,
            ),
        );
        append_operation(fixture, ForgeOperationFamily::StagingFold, &prepared, true).await;
        let reset = operation_event(
            "forge.file_compact.reset",
            resource,
            reset_detail_for_output(operation_id, resource, ForgeCompactionPhase::Reset, &orphan),
        );
        append_operation(fixture, ForgeOperationFamily::StagingFold, &reset, false).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        fixture
            .forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("later maintenance");
        assert!(fixture.staging.stat(&orphan).await.is_err());
    }

    /// Proves nonmatching task evidence stops takeover before destructive progress.
    async fn assert_mismatched_takeover_fails_closed(
        fixture: &Fixture,
        task_id: uuid::Uuid,
        stop: &CancellationToken,
    ) -> serde_json::Value {
        let original: serde_json::Value =
            sqlx::query_scalar("SELECT evidence FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("evidence");
        let candidate = original["cleanup_candidates"][0]["path"]
            .as_str()
            .expect("candidate")
            .to_owned();
        sqlx::query("UPDATE vala.forge_tasks SET evidence=jsonb_set(jsonb_set(evidence,'{cleanup_candidates}','[]'::jsonb),'{deleted_candidate_count}','0'::jsonb) WHERE task_id=$1")
            .bind(task_id).execute(fixture.operator_pool.pool()).await.expect("mismatch");
        let worker = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("worker");
        worker
            .execute_one_for_test(stop)
            .await
            .expect_err("mismatch must fail");
        let unchanged: (String, i64, String) = sqlx::query_as("SELECT state,(evidence->>'deleted_candidate_count')::bigint,o.phase FROM vala.forge_tasks t CROSS JOIN vala.forge_operation_state o WHERE t.task_id=$1 AND o.family='snapshot_expire'")
            .bind(task_id).fetch_one(fixture.operator_pool.pool()).await.expect("state");
        assert_eq!(unchanged, ("prepared".to_owned(), 0, "prepared".to_owned()));
        assert!(fixture.staging.stat(&candidate).await.is_ok());
        original
    }

    /// Asserts the scheduled maintenance demand is a ready snapshot-expiry task,
    /// claims it, and injects a crash immediately after the Prepared write.
    ///
    /// Guards the scheduler contract that `tasks_enqueued` counts a genuinely
    /// executable `ready` row (not a terminal `unschedulable` one), confirms the
    /// claimed strategy is `SnapshotExpiry`, then arms the post-Prepared failure
    /// injection so `execute_claim` crashes after persisting Prepared evidence.
    ///
    /// # Panics
    /// Panics when the durable row is not a `ready` snapshot-expiry task, no
    /// claimable maintenance task is available, the claimed strategy differs, or
    /// the injected crash does not surface an error. These are test invariants.
    async fn claim_scheduled_expiry_demand_and_crash_after_prepared(
        fixture: &Fixture,
        stop: &CancellationToken,
    ) {
        // The enqueued demand must be a genuinely executable, ready snapshot-expiry
        // task, not a terminal `unschedulable` row still counted in `tasks_enqueued`.
        let scheduled_row: (String, String) = sqlx::query_as(
            "SELECT strategy,state FROM vala.forge_tasks WHERE data_tenant_id=$1 AND strategy='snapshot_expiry'",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("scheduled maintenance row");
        assert_eq!(
            scheduled_row,
            ("snapshot_expiry".to_owned(), "ready".to_owned())
        );
        fixture.worker.fail_after_maintenance_prepared_for_test();
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("claim maintenance task")
            .expect("maintenance task available to claim");
        assert!(
            matches!(
                claim.strategy,
                ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry)
            ),
            "claimed task must be the snapshot-expiry maintenance demand: {:?}",
            claim.strategy
        );
        fixture
            .worker
            .execute_claim(claim, stop)
            .await
            .expect_err("injected post-Prepared crash");
    }

    /// Production scheduling persists exact ordered cleanup evidence before deleting it.
    #[tokio::test]
    async fn scheduled_maintenance_deletes_only_evidenced_expired_objects() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        fixture.schedule_and_execute().await;
        make_current_files_right_sized(&fixture).await;
        append_expirable_history(&fixture).await;
        tokio::time::sleep(Duration::from_millis(5)).await;

        let identity = ForgeTaskTableIdentity::new(
            "wyrd-redux",
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        )
        .expect("maintenance identity");
        let tasks = ForgeTasks::new(fixture.operator_pool.clone());
        tasks
            .upsert_periodic(fixture.tenant, &identity)
            .await
            .expect("periodic maintenance demand");
        let stop = CancellationToken::new();
        let scheduled =
            ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
                .expect("maintenance scheduler")
                .schedule_once(&stop)
                .await
                .expect("maintenance schedule");
        assert_eq!(scheduled.tasks_enqueued, 1, "maintenance: {scheduled:?}");
        claim_scheduled_expiry_demand_and_crash_after_prepared(&fixture, &stop).await;
        let prepared_task: uuid::Uuid = sqlx::query_scalar(
            "UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' \
             WHERE data_tenant_id=$1 AND strategy='snapshot_expiry' AND state='prepared' RETURNING task_id",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("expired Prepared maintenance claim");
        let original_evidence =
            assert_mismatched_takeover_fails_closed(&fixture, prepared_task, &stop).await;
        sqlx::query(
            "UPDATE vala.forge_tasks SET evidence=$2,claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1",
        )
        .bind(prepared_task)
        .bind(original_evidence)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("restore exact cleanup evidence");
        let takeover = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("takeover worker");
        assert!(
            takeover
                .execute_one_for_test(&stop)
                .await
                .expect("Prepared takeover")
        );
        let task_state: String =
            sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(prepared_task)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("reconciled task");
        assert_eq!(task_state, "succeeded");
        let resource = format!(
            "bifrost://{}/{}/{}",
            fixture.tenant, fixture.binding.table_ref.namespace, fixture.binding.table_ref.name
        );
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("reconciled expiry tenant connection");
        let expiry_state: (String, i64) = sqlx::query_as(
            "SELECT phase,(SELECT count(*) FROM vala.audit_outbox WHERE operation LIKE 'forge.snapshot_expire.%') \
             FROM vala.forge_operation_state WHERE resource=$1 AND family='snapshot_expire'",
        )
        .bind(&resource)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("reconciled expiry projection");
        assert_eq!(expiry_state, ("recovered".to_owned(), 2));
        assert_completed_cleanup_evidence(&fixture).await;

        assert_later_destructive_maintenance(&fixture, &resource).await;
    }
}
