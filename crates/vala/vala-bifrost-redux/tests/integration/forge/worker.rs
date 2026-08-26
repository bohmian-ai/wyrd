//! Worker admission and refusal: task validation, claim identity and tenancy
//! checks, lease exclusion, and resource-fit gating before any IO.
//!
//! Module of the `forge` group; shared fixtures live in `support.rs`.
//!
//! The tests stay wrapped in `mod pg_tests` so the fast lane's `--skip pg_tests`
//! filter still excludes them.

mod pg_tests {

    use std::sync::{Arc, atomic::Ordering};
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;
    use vala_bifrost_redux::catalog::TenantTableBinding;
    use vala_bifrost_redux::forge::{
        ForgeConfig, ForgeError, ForgeLease, ForgeScheduler, ForgeWorker, ForgeWorkerConfig,
        forge_lease_key,
    };

    use vala_sql::queries::forge_tasks::ForgeTasks;

    use vala_sql::row_types::forge_tasks::{
        FORGE_TASK_PAYLOAD_VERSION, ForgeTaskEstimates, ForgeTaskLane, ForgeTaskPlan,
        ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask,
    };

    use wyrd_spec::DataTenantId;

    use crate::forge::support::*;

    /// A superseded base cancels before rewrite IO and durably requests its successor.
    #[tokio::test]
    async fn superseded_worker_task_has_no_external_effect_and_replans() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let stop = CancellationToken::new();
        let scheduler =
            ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
                .expect("fixture scheduler");
        let planned = scheduler.schedule_once(&stop).await.expect("initial plan");
        assert_eq!(planned.tasks_enqueued, 1);

        let empty = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("empty planned table");
        fixture.append_seed_manifest(&empty, 0).await;
        let superseding_snapshot = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("superseding table")
            .metadata()
            .current_snapshot_id();
        let forge_prefix = format!("{}/data/forge/", fixture.binding.object_prefix);
        let before = fixture
            .staging
            .list_with(&forge_prefix)
            .recursive(true)
            .await
            .expect("pre-cancel Forge object list");

        assert!(
            fixture
                .worker
                .execute_one_for_test(&stop)
                .await
                .expect("cancel superseded task")
        );
        let after = fixture
            .staging
            .list_with(&forge_prefix)
            .recursive(true)
            .await
            .expect("post-cancel Forge object list");
        assert_eq!(after.len(), before.len(), "cancellation wrote no outputs");
        let current_snapshot = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("post-cancel table")
            .metadata()
            .current_snapshot_id();
        assert_eq!(current_snapshot, superseding_snapshot);
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("superseded state tenant connection");
        let state: (String, i64, i64, i64) = sqlx::query_as(
            "SELECT state,(SELECT count(*) FROM vala.file_list WHERE data_tenant_id=$1 AND compacted),(SELECT count(*) FROM vala.forge_planning_demands WHERE data_tenant_id=$1),(SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$1 AND operation='forge.task.cancelled' AND payload_summary='base_snapshot_superseded') FROM vala.forge_tasks WHERE data_tenant_id=$1 ORDER BY created_at LIMIT 1",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("superseded durable state");
        assert_eq!(state, ("cancelled".to_owned(), 0, 1, 1));

        let successor = scheduler
            .schedule_once(&stop)
            .await
            .expect("successor plan");
        assert_eq!(successor.tasks_enqueued, 1);
    }

    /// Enqueues, directly claims, and proves one closed worker payload fails terminally.
    ///
    /// # Panics
    ///
    /// Panics when admission, direct execution, or terminal-state evidence fails.
    async fn assert_worker_payload_fails_before_effect(
        fixture: &Fixture,
        tasks: &ForgeTasks,
        strategy: ForgeTaskStrategy,
        plan: ForgeTaskPlan,
        hash: u8,
    ) {
        let task_id = tasks
            .enqueue(&fixture.durable_task(strategy, plan, hash))
            .await
            .expect("validation task enqueue");
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("validation claim")
            .expect("validation task");
        assert_eq!(claim.task_id, task_id);
        fixture
            .worker
            .execute_claim(claim, &CancellationToken::new())
            .await
            .expect("closed validation terminalization");
        let state: String =
            sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("validation state");
        assert_eq!(state, "failed");
    }

    /// Direct worker validation rejects closed-strategy and malformed payloads.
    ///
    /// # Panics
    ///
    /// Panics when SQL admission, worker validation, or before-effect evidence
    /// differs from the closed task contract.
    #[tokio::test]
    async fn worker_rejects_reserved_and_malformed_tasks_before_effect() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), false, 0, fixture_snapshot()).await;
        let tasks = ForgeTasks::new(fixture.operator_pool.clone());
        for (strategy, plan, hash) in [
            (
                ForgeTaskStrategy::FullIdentity,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: vec!["data/reserved.parquet".to_owned()],
                    parameters: serde_json::json!({"kind":"full_identity"}),
                },
                201,
            ),
            (
                ForgeTaskStrategy::SnapshotExpiry,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: vec!["metadata/v1.json".to_owned()],
                    parameters: serde_json::json!({"kind":"snapshot_expiry"}),
                },
                202,
            ),
            (
                ForgeTaskStrategy::StagingFold,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: Vec::new(),
                    parameters: serde_json::json!({"kind":"staging_fold"}),
                },
                203,
            ),
        ] {
            assert_worker_payload_fails_before_effect(&fixture, &tasks, strategy, plan, hash).await;
        }
        assert_eq!(fixture.reads.output_put_calls(), 0);
        let lease_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.maintenance_leases WHERE lease_key=$1")
                .bind(forge_lease_key(
                    fixture.tenant,
                    &fixture.binding.logical_namespace,
                    &fixture.binding.table_name,
                ))
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("validation lease count");
        assert_eq!(lease_count, 0);
    }

    /// An unknown raw strategy is terminally audited and releases its claim so
    /// the same worker slot can execute the next valid task.
    ///
    /// # Panics
    ///
    /// Panics when corruption setup, quarantine evidence, release state, or
    /// valid successor execution differs from the worker contract.
    #[tokio::test]
    async fn worker_quarantines_unknown_strategy_and_continues_slot() {
        let unknown_fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let tasks = ForgeTasks::new(unknown_fixture.operator_pool.clone());
        let unknown_id = tasks
            .enqueue(&unknown_fixture.durable_task(
                ForgeTaskStrategy::StagingFold,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: vec!["data/unknown.parquet".to_owned()],
                    parameters: serde_json::json!({"kind":"staging_fold"}),
                },
                204,
            ))
            .await
            .expect("unknown task seed");
        let admin = unknown_fixture
            .pg
            .superuser_pool()
            .await
            .expect("validation admin");
        sqlx::query("ALTER TABLE vala.forge_tasks DROP CONSTRAINT forge_tasks_strategy_check")
            .execute(&admin)
            .await
            .expect("drop strategy constraint for corruption proof");
        sqlx::query("UPDATE vala.forge_tasks SET strategy='unknown' WHERE task_id=$1")
            .bind(unknown_id)
            .execute(&admin)
            .await
            .expect("inject unknown strategy");
        let mut status_conn = unknown_fixture
            .pg
            .vala_postgres()
            .tenant_conn(unknown_fixture.tenant)
            .await
            .expect("unknown status tenant connection");
        assert!(
            tasks.status(&mut status_conn, 8).await.is_err(),
            "canonical status decoding must fail closed on an unknown strategy"
        );
        drop(status_conn);
        enqueue_exact_staging_task(&unknown_fixture, &unknown_fixture.binding, 205).await;
        assert!(
            unknown_fixture
                .worker
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("unknown strategy quarantine"),
            "unknown persisted strategy must be claimed and terminalized"
        );
        let quarantined: (String, bool, bool, bool, i64, i64) = sqlx::query_as(
            "SELECT state,attempt_id IS NULL,claimed_by IS NULL,claim_expires_at IS NULL,\
                    (SELECT count(*) FROM vala.audit_outbox \
                      WHERE resource='forge-task:' || $1::text \
                        AND payload_summary LIKE '%unknown%'),\
                    (SELECT count(*) FROM vala.maintenance_leases WHERE lease_key=$2) \
               FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(unknown_id)
        .bind(forge_lease_key(
            unknown_fixture.tenant,
            &unknown_fixture.binding.logical_namespace,
            &unknown_fixture.binding.table_name,
        ))
        .fetch_one(&admin)
        .await
        .expect("unknown strategy quarantine state");
        assert_eq!(quarantined, ("failed".to_owned(), true, true, true, 1, 0));
        assert_eq!(unknown_fixture.reads.output_put_calls(), 0);
        assert!(
            unknown_fixture
                .worker
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("slot continues to valid task"),
            "same slot must claim the next supported task"
        );
        let succeeded: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND state='succeeded'",
        )
        .bind(unknown_fixture.tenant.as_uuid())
        .fetch_one(&admin)
        .await
        .expect("valid successor state");
        assert_eq!(succeeded, 1);
        assert!(unknown_fixture.reads.output_put_calls() > 0);
    }

    /// A mismatched independently projected execution tenant never reaches SQL
    /// tenant binding, the table lease, catalog publication, or object output.
    ///
    /// # Panics
    ///
    /// Panics when the worker accepts a mismatched claim context or mutates any
    /// durable/external state before rejecting it.
    #[tokio::test]
    async fn worker_rejects_claim_tenant_mismatch_before_every_effect() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), false, 0, fixture_snapshot()).await;
        let tasks = ForgeTasks::new(fixture.operator_pool.clone());
        let task_id = tasks
            .enqueue(&fixture.durable_task(
                ForgeTaskStrategy::StagingFold,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: vec!["data/tenant-mismatch.parquet".to_owned()],
                    parameters: serde_json::json!({"kind":"staging_fold"}),
                },
                205,
            ))
            .await
            .expect("tenant mismatch seed");
        let mut claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("tenant mismatch claim")
            .expect("tenant mismatch task");
        claim.execution_tenant_id = DataTenantId::new_v7();
        assert!(
            fixture
                .worker
                .execute_claim(claim, &CancellationToken::new())
                .await
                .is_err()
        );
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("tenant mismatch evidence connection");
        let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("tenant mismatch audit evidence");
        let evidence: (String, i64) = sqlx::query_as(
            "SELECT state,(SELECT count(*) FROM vala.maintenance_leases) FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task_id)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("tenant mismatch evidence");
        assert_eq!(evidence, ("claimed".to_owned(), 0));
        assert_eq!(audit_count, 0);
        assert_eq!(fixture.reads.output_put_calls(), 0);
        assert_eq!(
            fixture
                .catalog
                .load_table(&fixture.binding.table_ident())
                .await
                .expect("unchanged mismatch table")
                .metadata()
                .current_snapshot_id(),
            None
        );
    }

    /// Direct claim-envelope identity failures stop before tenant SQL, lease,
    /// catalog, or object effects for every unsupported identity dimension.
    ///
    /// # Panics
    ///
    /// Panics when catalog, namespace, or table validation reaches any effect
    /// boundary or mutates the claimed task.
    #[tokio::test]
    async fn worker_rejects_invalid_claim_identity_before_every_effect() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), false, 0, fixture_snapshot()).await;
        let task_id = ForgeTasks::new(fixture.operator_pool.clone())
            .enqueue(&fixture.durable_task(
                ForgeTaskStrategy::StagingFold,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: vec!["data/invalid-identity.parquet".to_owned()],
                    parameters: serde_json::json!({"kind":"staging_fold"}),
                },
                206,
            ))
            .await
            .expect("invalid identity seed");
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("invalid identity claim")
            .expect("invalid identity task");
        let reads_before = fixture.reads.whole_reads.load(Ordering::Acquire);
        let ranges_before = fixture.reads.ranged_reads.load(Ordering::Acquire);
        let outputs_before = fixture.reads.output_put_calls();
        for invalid in [
            ForgeTaskTableIdentity {
                catalog: "other".to_owned(),
                namespace: "vala.bifrost".to_owned(),
                table: "events".to_owned(),
            },
            ForgeTaskTableIdentity {
                catalog: "wyrd-redux".to_owned(),
                namespace: "unknown".to_owned(),
                table: "events".to_owned(),
            },
            ForgeTaskTableIdentity {
                catalog: "wyrd-redux".to_owned(),
                namespace: "vala.bifrost".to_owned(),
                table: "../events".to_owned(),
            },
        ] {
            let mut invalid_claim = claim.clone();
            invalid_claim.table_ref = invalid;
            assert!(
                fixture
                    .worker
                    .execute_claim(invalid_claim, &CancellationToken::new())
                    .await
                    .is_err(),
                "invalid identity must be rejected"
            );
        }
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("invalid identity evidence connection");
        let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("invalid identity audit evidence");
        let evidence: (String, i64) = sqlx::query_as(
            "SELECT state,(SELECT count(*) FROM vala.maintenance_leases) \
               FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task_id)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("invalid identity effect evidence");
        assert_eq!(evidence, ("claimed".to_owned(), 0));
        assert_eq!(audit_count, 0);
        assert_eq!(
            fixture.reads.whole_reads.load(Ordering::Acquire),
            reads_before
        );
        assert_eq!(
            fixture.reads.ranged_reads.load(Ordering::Acquire),
            ranges_before
        );
        assert_eq!(fixture.reads.output_put_calls(), outputs_before);
        assert_eq!(
            fixture
                .catalog
                .load_table(&fixture.binding.table_ident())
                .await
                .expect("unchanged invalid-identity table")
                .metadata()
                .current_snapshot_id(),
            None
        );
    }

    /// Worker authority selected for one deterministic heartbeat-loss proof.
    #[derive(Clone, Copy)]
    enum WorkerAuthorityLoss {
        /// Expire only the durable task/large-lane claim.
        Claim,
        /// Replace only the table-scoped publication lease generation.
        TableLease,
    }

    /// Pauses one real task after output PUT and proves one authority loss
    /// cancels execution without conflating the independent surviving fence.
    ///
    /// # Panics
    ///
    /// Panics when the real worker does not observe the injected authority
    /// loss within the bounded heartbeat interval or reports the wrong class.
    async fn assert_worker_authority_loss(loss: WorkerAuthorityLoss) {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let claim = fixture.plan_and_claim().await;
        let task_id = claim.task_id;
        let claim_owner = claim.claimed_by.expect("claimed worker owner");
        let lease_key = forge_lease_key(
            fixture.tenant,
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        );
        fixture.reads.pause_after_next_output_put();
        let stop = CancellationToken::new();
        let execution = tokio::spawn({
            let worker = fixture.worker.clone();
            let stop = stop.clone();
            async move { worker.execute_claim(claim, &stop).await }
        });
        tokio::time::timeout(Duration::from_secs(30), fixture.reads.wait_for_output_put())
            .await
            .expect("worker output PUT boundary");
        let lease_before: (uuid::Uuid, i64) = sqlx::query_as(
            "SELECT owner,fencing_token FROM vala.maintenance_leases WHERE lease_key=$1",
        )
        .bind(&lease_key)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("active table lease");
        match loss {
            WorkerAuthorityLoss::Claim => {
                sqlx::query(
                    "UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1",
                )
                .bind(task_id)
                .execute(fixture.operator_pool.pool())
                .await
                .expect("expire task claim only");
            }
            WorkerAuthorityLoss::TableLease => {
                sqlx::query(
                    "UPDATE vala.maintenance_leases SET owner=$2,fencing_token=fencing_token+1 WHERE lease_key=$1",
                )
                .bind(&lease_key)
                .bind(uuid::Uuid::now_v7())
                .execute(fixture.operator_pool.pool())
                .await
                .expect("replace table lease only");
            }
        }
        tokio::time::sleep(Duration::from_millis(75)).await;
        match loss {
            WorkerAuthorityLoss::Claim => {
                let lease_after: (uuid::Uuid, i64) = sqlx::query_as(
                    "SELECT owner,fencing_token FROM vala.maintenance_leases WHERE lease_key=$1",
                )
                .bind(&lease_key)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("surviving table lease");
                assert_eq!(lease_after, lease_before, "table fence was not stolen");
            }
            WorkerAuthorityLoss::TableLease => {
                let claim_live: bool = sqlx::query_scalar(
                    "SELECT claimed_by=$2 AND claim_expires_at>statement_timestamp() FROM vala.forge_tasks WHERE task_id=$1",
                )
                .bind(task_id)
                .bind(claim_owner)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("surviving task claim");
                assert!(claim_live, "task claim remains independently live");
            }
        }
        fixture.reads.release_output_put();
        let result = tokio::time::timeout(Duration::from_secs(30), execution)
            .await
            .expect("authority-loss shutdown bound")
            .expect("authority-loss worker join");
        match loss {
            WorkerAuthorityLoss::Claim => {
                assert!(matches!(result, Err(ForgeError::Sql(_))), "{result:?}");
            }
            WorkerAuthorityLoss::TableLease => {
                assert!(
                    matches!(result, Err(ForgeError::FenceLost { .. })),
                    "{result:?}"
                );
            }
        }
    }

    /// Claim heartbeat loss cancels a paused real worker while its table fence survives.
    #[tokio::test]
    async fn worker_claim_loss_cancels_independently() {
        assert_worker_authority_loss(WorkerAuthorityLoss::Claim).await;
    }

    /// Table-lease loss cancels a paused real worker while its claim survives.
    #[tokio::test]
    async fn worker_table_lease_loss_cancels_independently() {
        assert_worker_authority_loss(WorkerAuthorityLoss::TableLease).await;
    }

    /// A live table lease excludes the direct worker path before rewrite output.
    ///
    /// # Panics
    ///
    /// Panics when a second worker bypasses the table-scoped publication fence.
    #[tokio::test]
    async fn worker_same_table_lease_excludes_publication() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let claim = fixture.plan_and_claim().await;
        let competing = ForgeLease::acquire(
            &fixture.operator_pool,
            forge_lease_key(
                fixture.tenant,
                &fixture.binding.logical_namespace,
                &fixture.binding.table_name,
            ),
            uuid::Uuid::now_v7(),
            ForgeConfig::default().lease_ttl,
        )
        .await
        .expect("competing lease query")
        .expect("competing lease");
        let result = fixture
            .worker
            .execute_claim(claim, &CancellationToken::new())
            .await;
        assert!(matches!(result, Err(ForgeError::FenceLost { .. })));
        assert_eq!(fixture.reads.output_put_calls(), 0);
        assert!(
            competing
                .release(&fixture.operator_pool)
                .await
                .expect("competing lease release")
        );
    }

    /// Resource refusal returns the durable claim to retryable before rewrite IO.
    ///
    /// # Panics
    ///
    /// Panics when the production worker performs object output, retains its
    /// claim, or projects anything other than typed capacity pressure.
    #[tokio::test]
    async fn forge_waits_without_io_when_resources_do_not_fit() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let resources = fixture.forge.resources_for_test();
        let plan = fixture.roles.plan();
        let blocker_envelope = vala_bifrost_redux::forge::ForgeEnvelopeSizer::size(
            1,
            1,
            1,
            vala_bifrost_redux::forge::ForgeCapacity {
                max_files: 1,
                max_bytes: u64::MAX,
                max_parallelism: 1,
                max_memory_bytes: u64::try_from(plan.elastic_memory_bytes)
                    .expect("blocker memory capacity"),
                max_spill_bytes: plan.scratch_limit_bytes,
                max_large_task_bytes: u64::MAX,
            },
        )
        .expect("blocker envelope");
        let blocker = resources
            .try_acquire_rewrite(vala_bifrost_redux::resources::ForgeRewriteRequest {
                envelope: blocker_envelope,
                memory_bytes: plan.elastic_memory_bytes,
                scratch_bytes: plan.scratch_limit_bytes,
                reader_permits: u16::try_from(plan.effective_cpu).unwrap_or(u16::MAX),
            })
            .expect("test owner occupies all Forge resources");
        let claim = fixture.plan_and_claim().await;
        let task_id = claim.task_id;
        let outputs_before = fixture.reads.output_put_calls();
        let result = fixture
            .worker
            .execute_and_settle_claim_for_test(claim, &CancellationToken::new())
            .await;
        assert!(
            matches!(result, Err(ForgeError::Capacity { .. })),
            "{result:?}"
        );
        assert_eq!(fixture.reads.output_put_calls(), outputs_before);
        let state: String =
            sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("capacity-refused task state");
        assert_eq!(state, "retryable");
        drop(blocker);
        let snapshot = resources.snapshot().expect("released resource snapshot");
        assert_eq!(snapshot.elastic_memory_used_bytes, 0);
        assert_eq!(snapshot.scratch_used_bytes, 0);
    }

    /// Execution-envelope exhaustion terminalizes immediately without retry.
    #[tokio::test]
    async fn execution_envelope_exhaustion_terminalizes_without_retry() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let stop = CancellationToken::new();
        let planned = ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
            .expect("fixture scheduler")
            .schedule_once(&stop)
            .await
            .expect("planning pass");
        assert_eq!(planned.tasks_enqueued, 1);
        let decoded = 16_i64 * 1024 * 1024;
        let merge = 1024_i64 * 1024;
        let updated = sqlx::query(
            "UPDATE vala.forge_tasks SET decoded_batch_bytes=$2,decoded_input_bytes=$2,estimated_parallelism=1,sort_merge_reservation_bytes=$3,sort_working_bytes=2*$2+$3,sort_spill_bytes=1,estimated_memory_bytes=3*$2+$3+encoder_buffer_bytes+upload_chunk_bytes+footer_encoded_bytes,estimated_spill_bytes=1 WHERE data_tenant_id=$1 AND state='ready'",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(decoded)
        .bind(merge)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("constrain persisted production sort envelope")
        .rows_affected();
        assert_eq!(updated, 1);
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("constrained claim query")
            .expect("constrained claim");
        let task_id = claim.task_id;
        let result = fixture
            .worker
            .execute_and_settle_claim_for_test(claim, &stop)
            .await;
        assert!(
            matches!(result, Err(ForgeError::ExecutionEnvelopeExceeded { .. })),
            "production SortExec must surface typed envelope exhaustion: {result:?}"
        );

        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("execution refusal evidence tenant connection");
        let settled: (String, i32, Option<String>, i64) = sqlx::query_as(
            "SELECT state,attempt_count,failure_class,(SELECT count(*) FROM vala.audit_outbox WHERE resource='forge-task:' || $1::text AND operation='forge.task.failed') FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("settled execution refusal");
        assert_eq!(
            settled,
            (
                "failed".to_owned(),
                1,
                Some("capacity_refused".to_owned()),
                1
            )
        );
        assert_eq!(fixture.reads.output_put_calls(), 0);
        assert!(
            !fixture
                .worker
                .execute_one_for_test(&stop)
                .await
                .expect("terminal capacity refusal cannot be reclaimed")
        );
        let released = fixture
            .forge
            .resources_for_test()
            .snapshot()
            .expect("released resource snapshot");
        assert_eq!(released.elastic_memory_used_bytes, 0);
        assert_eq!(released.scratch_used_bytes, 0);
        assert_eq!(released.forge_reader_permits_used, 0);
    }

    /// Unmarked sources refuse after footer reads and before the first data page.
    #[tokio::test]
    async fn unmarked_source_refuses_before_first_data_page_range() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), false, 0, fixture_snapshot()).await;
        fixture.seed_unmarked_files(2, true).await;
        vala_bifrost_redux::forge::reset_data_page_reads_for_test();
        let stop = CancellationToken::new();
        let outcome = ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
            .expect("fixture scheduler")
            .schedule_once(&stop)
            .await
            .expect("unmarked planning pass");
        assert_eq!(outcome.tasks_enqueued, 1);
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("unmarked claim query")
            .expect("unmarked claim");
        let error = fixture
            .worker
            .execute_claim(claim, &stop)
            .await
            .expect_err("unmarked source must refuse");
        assert!(matches!(error, ForgeError::DataRefusal { .. }));
        assert!(fixture.reads.ranged_reads.load(Ordering::Acquire) > 0);
        assert_eq!(
            vala_bifrost_redux::forge::data_page_reads_for_test(),
            0,
            "footer identity refusal must precede every data-page request"
        );
        assert_eq!(fixture.reads.output_put_calls(), 0);
    }

    /// A claimed legacy task is auditedly superseded and replanned before input IO.
    #[tokio::test]
    async fn legacy_task_is_auditedly_superseded_and_replanned_before_io() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let stop = CancellationToken::new();
        let planned = ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
            .expect("fixture scheduler")
            .schedule_once(&stop)
            .await
            .expect("planning pass");
        assert_eq!(planned.tasks_enqueued, 1);
        sqlx::query(
            "UPDATE vala.forge_tasks SET envelope_version=0, decoded_batch_bytes=NULL, decoded_input_bytes=NULL, sort_working_bytes=NULL, sort_merge_reservation_bytes=NULL, encoder_buffer_bytes=NULL, upload_chunk_bytes=NULL, footer_encoded_bytes=NULL, footer_decode_workspace_bytes=NULL, sort_spill_bytes=NULL, estimated_files=1, estimated_bytes=9223372036854775807, estimated_parallelism=1, estimated_memory_bytes=9223372036854775807, estimated_spill_bytes=9223372036854775807, large_task_ceiling_bytes=9223372036854775807 WHERE data_tenant_id=$1 AND state='ready'",
        )
        .bind(fixture.tenant.as_uuid())
        .execute(fixture.operator_pool.pool())
        .await
        .expect("convert planned task to legacy");
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("claim query")
            .expect("legacy claim");
        let task_id = claim.task_id;
        let reads_before = fixture.reads.ranged_reads.load(Ordering::Relaxed);
        fixture
            .worker
            .execute_claim(claim, &stop)
            .await
            .expect("legacy supersession");

        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("legacy evidence tenant connection");
        let evidence: (String, i32, i64, i64) = sqlx::query_as(
            "SELECT state,attempt_count,(SELECT count(*) FROM vala.forge_planning_demands WHERE data_tenant_id=$1),(SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$1 AND operation='forge.task.cancelled') FROM vala.forge_tasks WHERE task_id=$2",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(task_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("legacy settlement evidence");
        assert_eq!(evidence, ("cancelled".to_owned(), 0, 1, 1));
        assert_eq!(
            fixture.reads.ranged_reads.load(Ordering::Relaxed),
            reads_before
        );
        assert_eq!(fixture.reads.output_put_calls(), 0);
    }

    /// Enqueues one exact ordinary staging task for a registered fixture table.
    ///
    /// # Panics
    ///
    /// Panics when exact inputs, identity construction, or enqueueing fails.
    async fn enqueue_exact_staging_task(fixture: &Fixture, binding: &TenantTableBinding, hash: u8) {
        let inputs: Vec<String> = sqlx::query_scalar(
            "SELECT file_path FROM vala.file_list WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 ORDER BY file_path",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&binding.logical_namespace)
        .bind(&binding.table_name)
        .fetch_all(fixture.operator_pool.pool())
        .await
        .expect("concurrent exact inputs");
        ForgeTasks::new(fixture.operator_pool.clone())
            .enqueue(&NewForgeTask {
                data_tenant_id: fixture.tenant,
                table_ref: ForgeTaskTableIdentity::new(
                    "wyrd-redux",
                    &binding.logical_namespace,
                    &binding.table_name,
                )
                .expect("concurrent task identity"),
                strategy: ForgeTaskStrategy::StagingFold,
                lane: ForgeTaskLane::Ordinary,
                base_snapshot_id: 0,
                plan: ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs,
                    parameters: serde_json::json!({"kind":"staging_fold"}),
                },
                plan_hash: [hash; 32],
                estimates: ForgeTaskEstimates {
                    envelope: Some(vala_sql::row_types::forge_tasks::ForgeTaskEnvelope {
                        version: vala_sql::row_types::forge_tasks::FORGE_ENVELOPE_VERSION,
                        reader_permits: 1,
                        decoded_batch_bytes: 16 * 1024 * 1024,
                        decoded_input_bytes: 16 * 1024 * 1024,
                        sort_working_bytes: 42 * 1024 * 1024,
                        sort_merge_reservation_bytes: 10 * 1024 * 1024,
                        encoder_buffer_bytes: 32 * 1024 * 1024,
                        upload_chunk_bytes: 8 * 1024 * 1024,
                        footer_encoded_bytes: 8 * 1024 * 1024,
                        footer_decode_workspace_bytes: 32 * 1024 * 1024,
                        sort_spill_bytes: 512 * 1024 * 1024,
                    }),
                    files: 2,
                    bytes: 200,
                    parallelism: 1,
                    memory_bytes: 106 * 1024 * 1024,
                    spill_bytes: 512 * 1024 * 1024,
                    large_ceiling_bytes: 2 * 1024 * 1024 * 1024,
                },
                ready_at: chrono::Utc::now(),
            })
            .await
            .expect("concurrent exact task");
    }

    /// Two fixed worker slots execute independent table rewrites concurrently.
    ///
    /// Both real output PUTs must reach the shared pause before either is
    /// released, which proves progress is not serialized by a process-global
    /// task or publication lock.
    ///
    /// # Panics
    ///
    /// Panics when planning does not enqueue both tables, admission serializes
    /// the claims, or either direct worker execution fails.
    #[tokio::test]
    async fn worker_independent_tables_execute_concurrently() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let second_binding = fixture.register_seeded_table().await;
        for (binding, hash) in [(&fixture.binding, 206_u8), (&second_binding, 207_u8)] {
            enqueue_exact_staging_task(&fixture, binding, hash).await;
        }
        let stop = CancellationToken::new();
        let worker = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig {
                worker_concurrency: 2,
                per_tenant_active_cap: 2,
            },
            uuid::Uuid::now_v7(),
        )
        .expect("two-slot worker");
        let first = worker
            .claim_for_test()
            .await
            .expect("first concurrent claim")
            .expect("first concurrent task");
        let Some(second) = worker
            .claim_for_test()
            .await
            .expect("second concurrent claim")
        else {
            panic!("second concurrent task missing");
        };
        assert_ne!(first.table_ref, second.table_ref);
        fixture.reads.pause_after_output_puts(2);
        let first_task = tokio::spawn({
            let worker = worker.clone();
            let stop = stop.clone();
            async move { worker.execute_claim(first, &stop).await }
        });
        let second_task = tokio::spawn({
            let worker = worker.clone();
            let stop = stop.clone();
            async move { worker.execute_claim(second, &stop).await }
        });
        tokio::time::timeout(
            Duration::from_secs(30),
            fixture.reads.wait_for_output_puts(2),
        )
        .await
        .expect("both independent outputs reach PUT concurrently");
        fixture.reads.release_output_put();
        let (first_result, second_result) = tokio::join!(first_task, second_task);
        first_result
            .expect("first concurrent worker join")
            .expect("first concurrent execution");
        second_result
            .expect("second concurrent worker join")
            .expect("second concurrent execution");
        let succeeded: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND state='succeeded'",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("concurrent terminal states");
        assert_eq!(succeeded, 2);
    }
}
