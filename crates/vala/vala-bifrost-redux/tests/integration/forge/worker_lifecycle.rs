//! Worker shutdown, crash, and reclaim: what a claim leaves behind and how
//! a successor recovers it.
//!
//! Module of the `forge` group; shared fixtures live in `support.rs`.
//!
//! The tests stay wrapped in `mod pg_tests` so the fast lane's `--skip pg_tests`
//! filter still excludes them.

mod pg_tests {

    use std::sync::Arc;
    use std::time::Duration;

    use sha2::{Digest, Sha256};

    use tokio_util::sync::CancellationToken;

    use vala_bifrost_redux::forge::{
        ForgeConfig, ForgeError, ForgeScheduler, ForgeWorker, ForgeWorkerCompletionObserver,
        ForgeWorkerConfig,
    };

    use crate::forge::support::*;

    /// Read one task's durable state and the tenant's active-claim count.
    ///
    /// The active-claim count mirrors the `forge_active_claims` metric: it
    /// counts rows in the pre-terminal `claimed`, `running`, and `prepared`
    /// states and excludes released `retryable` rows, so a clean shutdown drain
    /// is observable as the count falling to zero.
    ///
    /// # Panics
    ///
    /// Panics when either tenant-scoped query fails.
    async fn task_state_and_active_claims(fixture: &Fixture, task_id: uuid::Uuid) -> (String, i64) {
        let state: String =
            sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("task state");
        let active: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.forge_tasks \
             WHERE data_tenant_id=$1 AND state IN ('claimed','running','prepared')",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("active-claim count");
        (state, active)
    }

    /// Cancellation observed mid-rewrite, before the catalog commit, releases
    /// the in-flight claim to `retryable` and drains the active-claim count, so
    /// a successor reclaims it immediately without waiting for lease expiry and
    /// completes it exactly once.
    ///
    /// This is the in-flight (`execute_fenced`) pre-effect drain, re-proven
    /// through the migrated single release seam: the rewrite has written a real
    /// output PUT but not committed to the catalog, so the supervised slot's
    /// [`ForgeWorker::execute_and_settle_claim_for_test`] settlement — the same
    /// `run_slot` release decision — drains the claim losslessly.
    ///
    /// # Panics
    ///
    /// Panics when shutdown exceeds its bound, the cancelled attempt does not
    /// drain to `retryable`, the active-claim count does not fall to zero, or
    /// the successor cannot reclaim and finish the task exactly once.
    #[tokio::test]
    async fn worker_active_shutdown_releases_retryable_then_successor_reclaims() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let claim = fixture.plan_and_claim().await;
        let task_id = claim.task_id;
        fixture.reads.pause_after_next_output_put();
        let stop = CancellationToken::new();
        let execution = tokio::spawn({
            let worker = fixture.worker.clone();
            let stop = stop.clone();
            async move { worker.execute_and_settle_claim_for_test(claim, &stop).await }
        });
        tokio::time::timeout(Duration::from_secs(30), fixture.reads.wait_for_output_put())
            .await
            .expect("shutdown output boundary");
        stop.cancel();
        fixture.reads.release_output_put();
        let result = tokio::time::timeout(Duration::from_secs(30), execution)
            .await
            .expect("active shutdown bound")
            .expect("active worker join");
        assert!(matches!(result, Err(ForgeError::Shutdown)), "{result:?}");
        let (state, active) = task_state_and_active_claims(&fixture, task_id).await;
        assert_eq!(
            state, "retryable",
            "a pre-effect shutdown must release the in-flight claim to retryable"
        );
        assert_eq!(
            active, 0,
            "releasing the cancelled claim must drain the active-claim count"
        );
        let successor = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig {
                worker_concurrency: 1,
                per_tenant_active_cap: 1,
            },
            uuid::Uuid::now_v7(),
        )
        .expect("successor worker");
        assert!(
            successor
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("successor reclaim execution")
        );
        let (state, _) = task_state_and_active_claims(&fixture, task_id).await;
        assert_eq!(state, "succeeded");
        let succeeded: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND state='succeeded'",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("terminal state count");
        assert_eq!(
            succeeded, 1,
            "the pre-effect attempt must not commit a duplicate terminal"
        );
    }

    /// A cooperative shutdown observed after a maintenance operation's durable
    /// effect retains the claim through the migrated single release seam instead
    /// of releasing it.
    ///
    /// The maintenance dispatch observes the authority token, not the
    /// shutdown-sensitive `operation_stop`, so a graceful shutdown lets the
    /// operation commit its durable effect and reach `prepared`; the post-effect
    /// checkpoint then surfaces [`ForgeError::ShutdownRetained`]. The supervised
    /// slot's settlement must NOT match the pre-effect release guard: the claim
    /// stays `prepared` and counted as active for evidence-based and
    /// lease-expiry recovery. This is the post-effect complement to
    /// [`worker_active_shutdown_releases_retryable_then_successor_reclaims`],
    /// driven through the same
    /// [`ForgeWorker::execute_and_settle_claim_for_test`] seam.
    ///
    /// # Panics
    ///
    /// Panics when the maintenance boundary is not reached, the post-effect
    /// result is not [`ForgeError::ShutdownRetained`], the durable state is not
    /// the retained `prepared`, or the active-claim count is not the retained
    /// single claim.
    #[tokio::test]
    async fn worker_post_effect_shutdown_retains_claim_via_settlement() {
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
        let controls = fixture.forge.maintenance_controls_for_test();
        controls.arm_manifest_submission();
        let stop = CancellationToken::new();
        let execution = tokio::spawn({
            let worker = fixture.worker.clone();
            let stop = stop.clone();
            async move { worker.execute_and_settle_claim_for_test(claim, &stop).await }
        });
        tokio::pin!(execution);
        let arrival = async { controls.wait_manifest_submission().await };
        tokio::pin!(arrival);
        tokio::select! {
            () = &mut arrival => {}
            result = &mut execution => {
                panic!("maintenance returned before catalog boundary: {result:?}")
            }
        }
        stop.cancel();
        controls.release_manifest_submission();
        let result = (&mut execution).await.expect("post-effect worker join");
        assert!(
            matches!(result, Err(ForgeError::ShutdownRetained)),
            "a post-effect shutdown must surface ShutdownRetained: {result:?}"
        );
        let (state, active) = task_state_and_active_claims(&fixture, task_id).await;
        assert_eq!(
            state, "prepared",
            "a post-effect shutdown must retain the durable prepared state, never release it"
        );
        assert_eq!(
            active, 1,
            "retaining the post-effect claim must keep it counted as an active claim"
        );
    }

    /// A worker that crashes mid-execution without observing cancellation leaves
    /// a durable `running` claim that a successor reclaims only after the lease
    /// expires, then completes exactly once.
    ///
    /// This preserves crash-path recovery for the `running` state that the
    /// cooperative drain deliberately does not cover: cancellation is never
    /// observed, so the claim is retained and recovered through
    /// `reclaim_expired` after its lease TTL rather than released to
    /// `retryable`.
    ///
    /// # Panics
    ///
    /// Panics when the crashed `running` claim is not reclaimed after expiry or
    /// the successor does not complete it exactly once.
    #[tokio::test]
    async fn worker_running_crash_without_cancel_is_reclaimed_after_expiry() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let claim = fixture.plan_and_claim().await;
        let task_id = claim.task_id;
        sqlx::query(
            "UPDATE vala.forge_tasks SET state='running', watermark_snapshot_id=0, watermark_timestamp_ms=0, claim_expires_at=statement_timestamp()-interval '1 second',next_eligible_at=statement_timestamp()-interval '1 second' WHERE task_id=$1",
        )
        .bind(task_id)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("simulate crashed running attempt with an expired lease");
        let successor = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig {
                worker_concurrency: 1,
                per_tenant_active_cap: 1,
            },
            uuid::Uuid::now_v7(),
        )
        .expect("successor worker");
        assert!(
            !successor
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("successor bounded reclaim"),
            "lease reclaim persists backoff before another attempt"
        );
        sqlx::query(
            "UPDATE vala.forge_tasks SET next_eligible_at=statement_timestamp()-interval '1 second',ready_at=statement_timestamp()-interval '1 second' WHERE task_id=$1",
        )
        .bind(task_id)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("advance reclaimed task eligibility");
        assert!(
            successor
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("successor execution after backoff")
        );
        let (state, _) = task_state_and_active_claims(&fixture, task_id).await;
        assert_eq!(state, "succeeded");
        let succeeded: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND state='succeeded'",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("terminal state count");
        assert_eq!(
            succeeded, 1,
            "the crashed attempt committed nothing, so recovery completes exactly once"
        );
    }

    /// A cooperative shutdown observed after a supervised slot durably claims
    /// work, but before execution begins, releases the claim to `retryable` and
    /// drains the active-claim count so a successor reclaims it losslessly.
    ///
    /// This is the `run_slot` post-claim, pre-execute drain path: no pre-effect
    /// durable work was performed when the slot observes shutdown.
    ///
    /// # Panics
    ///
    /// Panics when planning does not enqueue exactly one task, the supervised
    /// slot does not reach its claim gate, the released task is not `retryable`,
    /// or the active-claim count does not drain to zero.
    #[tokio::test]
    async fn worker_shutdown_before_execute_releases_claim_to_retryable() {
        let fixture = Fixture::new_with_observer(ForgeWorkerCompletionObserver::new()).await;
        let completion = fixture
            .completion
            .clone()
            .expect("observer fixture exposes its completion observer");
        let stop = CancellationToken::new();
        let planned = ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
            .expect("fixture scheduler")
            .schedule_once(&stop)
            .await
            .expect("fixture planning pass");
        assert_eq!(planned.tasks_enqueued, 1, "planned outcome: {planned:?}");
        completion.hold_after_claims_for_test(1);
        let run = tokio::spawn({
            let worker = fixture.worker.clone();
            let stop = stop.clone();
            async move { worker.run(stop).await }
        });
        tokio::time::timeout(
            Duration::from_secs(30),
            completion.wait_for_claims_for_test(),
        )
        .await
        .expect("supervised slot reaches its claim gate");
        stop.cancel();
        completion.release_claims_for_test();
        tokio::time::timeout(Duration::from_secs(30), run)
            .await
            .expect("supervised shutdown bound")
            .expect("supervised run join")
            .expect("supervised run drains cleanly");
        let task_id: uuid::Uuid =
            sqlx::query_scalar("SELECT task_id FROM vala.forge_tasks WHERE data_tenant_id=$1")
                .bind(fixture.tenant.as_uuid())
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("single planned task");
        let (state, active) = task_state_and_active_claims(&fixture, task_id).await;
        assert_eq!(
            state, "retryable",
            "a shutdown observed before execution must release the claim to retryable"
        );
        assert_eq!(
            active, 0,
            "releasing the claim before execution must drain the active-claim count"
        );
    }

    /// A shutdown release against a claim that already advanced to `prepared`
    /// is benign: it retains the durable `prepared` state instead of erroring.
    ///
    /// The post-effect (`prepared`) row is past the pre-effect release guard, so
    /// [`ForgeWorker::release_cancelled_claim_for_test`] matches no row, returns
    /// `Ok(())`, and leaves the claim retained for reconciliation — proving a
    /// `prepared` claim under cancellation is observed as clean retention, never
    /// a release error, and stays counted as an active claim.
    ///
    /// # Panics
    ///
    /// Panics when the benign release errors, the `prepared` state is not
    /// retained, or the active-claim count changes.
    #[tokio::test]
    async fn prepared_claim_shutdown_release_is_benign_and_retained() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let claim = fixture.plan_and_claim().await;
        let task_id = claim.task_id;
        let attempt = claim.attempt_id.expect("claimed task carries an attempt");
        sqlx::query(
            "UPDATE vala.forge_tasks SET state='prepared', watermark_snapshot_id=0, watermark_timestamp_ms=0, evidence='{}'::jsonb WHERE task_id=$1 AND attempt_id=$2",
        )
        .bind(task_id)
        .bind(attempt)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("advance this worker's claim past the pre-effect guard");
        fixture
            .worker
            .release_cancelled_claim_for_test(task_id, attempt)
            .await
            .expect("releasing a prepared claim must be benign");
        let (state, active) = task_state_and_active_claims(&fixture, task_id).await;
        assert_eq!(
            state, "prepared",
            "a prepared claim under cancellation must be retained, not released"
        );
        assert_eq!(
            active, 1,
            "the retained prepared claim must remain an active claim"
        );
    }

    /// Reads the exact current metadata bytes and returns snapshot, location,
    /// and digest evidence for later recovery comparison.
    ///
    /// # Panics
    ///
    /// Panics when the committed fixture table lacks a snapshot or metadata
    /// location, escapes its table root, or its raw object cannot be read.
    async fn exact_current_metadata_evidence(
        fixture: &Fixture,
        table: &iceberg::table::Table,
    ) -> (i64, String, String) {
        let snapshot = table
            .metadata()
            .current_snapshot_id()
            .expect("task committed snapshot");
        let location = table
            .metadata_location_result()
            .expect("task committed metadata location")
            .to_owned();
        let relative = location
            .strip_prefix(&format!(
                "{}/",
                table.metadata().location().trim_end_matches('/')
            ))
            .expect("metadata location below fixture table");
        let key = format!(
            "{}/{relative}",
            fixture.binding.object_prefix.trim_end_matches('/')
        );
        let digest = format!(
            "sha256:{}",
            hex::encode(Sha256::digest(
                fixture
                    .staging
                    .read(&key)
                    .await
                    .expect("original metadata bytes")
                    .to_bytes()
            ))
        );
        (snapshot, location, digest)
    }

    /// Metadata evidence read failure leaves Running recovery state; after a
    /// later catalog snapshot, takeover finds the original exact task commit.
    ///
    /// # Panics
    ///
    /// Panics when missing evidence is fabricated, later catalog progress is
    /// mistaken for task evidence, recovery rewrites output, or the original
    /// committed bytes cannot terminalize and audit the task.
    #[tokio::test]
    async fn worker_recovers_exact_evidence_read_without_rewrite() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let claim = fixture.plan_and_claim().await;
        let task_id = claim.task_id;
        fixture.reads.fail_next_metadata_read();
        let result = fixture
            .worker
            .execute_claim(claim, &CancellationToken::new())
            .await;
        assert!(
            matches!(result, Err(ForgeError::ObjectStore(_))),
            "{result:?}"
        );
        let before_recovery = fixture.reads.output_put_calls();
        let retained: (String, bool) =
            sqlx::query_as("SELECT state,evidence IS NULL FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("retained evidence failure");
        assert_eq!(retained, ("retryable".to_owned(), true));
        let committed = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("committed table after evidence failure");
        let (original_snapshot, original_location, original_digest) =
            exact_current_metadata_evidence(&fixture, &committed).await;
        fixture.seed_files_at(99, 1, false).await;
        fixture.append_seed_manifest(&committed, 99).await;
        let advanced = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("intervening catalog table");
        assert_ne!(
            advanced.metadata().current_snapshot_id(),
            Some(original_snapshot),
            "intervening append must advance beyond the task commit"
        );
        assert_ne!(
            advanced
                .metadata_location_result()
                .expect("intervening metadata location"),
            original_location,
            "recovery must search retained metadata rather than current bytes"
        );
        sqlx::query(
            "UPDATE vala.forge_tasks SET next_eligible_at=statement_timestamp()-interval '1 second',ready_at=statement_timestamp()-interval '1 second' WHERE task_id=$1",
        )
        .bind(task_id)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("expire evidence-read attempt");
        let successor = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig {
                worker_concurrency: 1,
                per_tenant_active_cap: 1,
            },
            uuid::Uuid::now_v7(),
        )
        .expect("evidence successor");
        assert!(
            successor
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("evidence recovery")
        );
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("recovered evidence tenant connection");
        let recovered: (String, i64, String, String, i64) = sqlx::query_as(
            "SELECT state,(evidence->>'committed_snapshot_id')::bigint,\
                    evidence->>'committed_metadata_location',\
                    evidence->>'committed_metadata_digest',\
                    (SELECT count(*) FROM vala.audit_outbox \
                      WHERE resource='forge-task:' || $1::text \
                        AND operation IN ('forge.task.prepared','forge.task.succeeded')) \
               FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("recovered evidence state");
        assert_eq!(
            recovered,
            (
                "succeeded".to_owned(),
                original_snapshot,
                original_location,
                original_digest,
                2,
            )
        );
        assert_eq!(fixture.reads.output_put_calls(), before_recovery);
    }
}
