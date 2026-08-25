mod pg_tests {
    //! Real-Postgres lifecycle, fencing, audit, and isolation proofs for Forge tasks.

    use std::time::Duration as StdDuration;

    use chrono::{Duration, Utc};
    use sqlx::{PgPool, types::Uuid};
    use vala_sql::TenantConn;
    use vala_sql::queries::forge_tasks::{ForgeClaimLimits, ForgeEnqueueBatch, ForgeTasks};
    use vala_sql::row_types::forge_tasks::{
        FORGE_TASK_PAYLOAD_VERSION, ForgeClaimStrategy, ForgeCleanupCandidate,
        ForgeCleanupCategory, ForgeCleanupPath, ForgeTaskEstimates, ForgeTaskEvidence,
        ForgeTaskLane, ForgeTaskPlan, ForgeTaskState, ForgeTaskStrategy, ForgeTaskTableIdentity,
        ForgeTaskTransition, ForgeTaskTransitionOutcome, MAINTENANCE_STRATEGIES, NewForgeTask,
        SnapshotWatermark,
    };
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

    /// Starts one isolated migrated database and returns its administrative pool.
    ///
    /// # Panics
    /// Panics when the repository PostgreSQL fixture cannot start.
    async fn setup() -> (PgFixture, PgPool) {
        let fixture = PgFixture::start().await.expect("fixture");
        let admin = fixture.superuser_pool().await.expect("admin pool");
        (fixture, admin)
    }

    /// Builds one valid, already-eligible enqueue request with caller-selected identity and lane.
    ///
    /// The one-second margin keeps host and container clocks from making an
    /// immediate claim nondeterministically observe a future `ready_at`.
    ///
    /// # Panics
    /// Panics when the fixed test identity is invalid.
    fn task(tenant: DataTenantId, table: &str, lane: ForgeTaskLane, hash: u8) -> NewForgeTask {
        NewForgeTask {
            data_tenant_id: tenant,
            table_ref: ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", table)
                .expect("identity"),
            strategy: ForgeTaskStrategy::SmallFiles,
            lane,
            base_snapshot_id: i64::from(hash),
            plan: ForgeTaskPlan {
                version: FORGE_TASK_PAYLOAD_VERSION,
                inputs: vec![format!("data/{table}.parquet")],
                parameters: serde_json::json!({}),
            },
            plan_hash: [hash; 32],
            estimates: ForgeTaskEstimates {
                files: 1,
                bytes: 100,
                parallelism: 1,
                memory_bytes: 40 * 1024 * 1024,
                spill_bytes: 50,
                large_ceiling_bytes: 64 * 1024 * 1024,
                envelope: Some(vala_sql::row_types::forge_tasks::ForgeTaskEnvelope {
                    version: vala_sql::row_types::forge_tasks::FORGE_ENVELOPE_VERSION,
                    reader_permits: 1,
                    decoded_batch_bytes: 10,
                    decoded_input_bytes: 10,
                    sort_working_bytes: 30,
                    sort_merge_reservation_bytes: 10,
                    encoder_buffer_bytes: 40,
                    upload_chunk_bytes: 20,
                    footer_encoded_bytes: 8 * 1024 * 1024,
                    footer_decode_workspace_bytes: 32 * 1024 * 1024,
                    sort_spill_bytes: 50,
                    output_scratch_bytes: 50,
                }),
            },
            ready_at: Utc::now() - Duration::seconds(1),
        }
    }

    /// Builds one ready snapshot-expiry maintenance task for a given table.
    ///
    /// # Panics
    /// Panics when the fixed test identity is invalid.
    fn maintenance_task(tenant: DataTenantId, table: &str, hash: u8) -> NewForgeTask {
        NewForgeTask {
            strategy: ForgeTaskStrategy::SnapshotExpiry,
            plan: ForgeTaskPlan {
                version: FORGE_TASK_PAYLOAD_VERSION,
                inputs: vec![format!("metadata/{table}.avro")],
                parameters: serde_json::json!({"kind": "maintenance"}),
            },
            ..task(tenant, table, ForgeTaskLane::Ordinary, hash)
        }
    }

    /// Builds one internal lifecycle audit event.
    fn event(operation: &str, task_id: Uuid) -> AuditEvent {
        AuditEvent::new(
            RequestId::now_v7(),
            None,
            operation.to_owned(),
            format!("forge-task:{task_id}"),
            None,
            PrincipalId::new(Uuid::now_v7()),
            PrincipalKindTag::User,
            AuthMethod::Internal,
            "bifrost.forge".to_owned(),
            AuditDecision::Allow,
            AuditResult::Success,
            "redacted".to_owned(),
        )
    }

    /// Returns permissive positive capacity limits for lifecycle tests.
    fn limits(max_active_per_tenant: u32) -> ForgeClaimLimits {
        ForgeClaimLimits {
            max_active_per_tenant,
            lease_seconds: 30,
            max_files: 10,
            max_bytes: 1_000,
            max_parallelism: 4,
            max_memory_bytes: 128 * 1024 * 1024,
            max_spill_bytes: 1_000,
            max_large_task_bytes: 2_000,
        }
    }

    /// Version-two envelope terms survive enqueue and fair-claim decoding exactly.
    #[tokio::test]
    async fn forge_envelope_v2_round_trips_and_aggregates_match() {
        let (fixture, _admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let expected = task(
            fixture.data_tenant_id(),
            "envelope-round-trip",
            ForgeTaskLane::Ordinary,
            211,
        );
        tasks.enqueue(&expected).await.expect("enqueue envelope");

        let claim = tasks
            .claim_fair(Uuid::now_v7(), limits(1), None)
            .await
            .expect("claim query")
            .expect("claim");

        assert_eq!(claim.estimates.files, expected.estimates.files);
        assert_eq!(claim.estimates.bytes, expected.estimates.bytes);
        assert_eq!(claim.estimates.parallelism, expected.estimates.parallelism);
        assert_eq!(
            claim.estimates.memory_bytes,
            expected.estimates.memory_bytes
        );
        assert_eq!(claim.estimates.spill_bytes, expected.estimates.spill_bytes);
        assert_eq!(claim.estimates.envelope, expected.estimates.envelope);
        let envelope = claim.estimates.envelope.expect("version-two envelope");
        assert_eq!(
            envelope.memory_bytes().expect("resident total"),
            claim.estimates.memory_bytes
        );
        assert_eq!(
            envelope.scratch_bytes().expect("scratch total"),
            claim.estimates.spill_bytes
        );
    }

    /// Version-zero rows decode without manufacturing executable detail terms.
    #[tokio::test]
    async fn legacy_envelope_defaults_are_non_executable() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let legacy = task(
            fixture.data_tenant_id(),
            "legacy-default",
            ForgeTaskLane::Ordinary,
            212,
        );
        let task_id = tasks.enqueue(&legacy).await.expect("enqueue legacy seed");
        make_legacy(&admin, task_id, false).await;

        let claim = tasks
            .claim_fair(Uuid::now_v7(), limits(1), None)
            .await
            .expect("claim query")
            .expect("legacy claim");

        assert_eq!(claim.task_id, task_id);
        assert_eq!(claim.estimates.envelope, None);
        assert_eq!(claim.estimates.memory_bytes, 100);
        assert_eq!(claim.estimates.spill_bytes, 100);
    }

    /// Oversized legacy rows bypass only envelope and lane resource bounds.
    #[tokio::test]
    async fn oversized_legacy_envelope_bypasses_resource_bounds_for_supersession() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let legacy = task(
            fixture.data_tenant_id(),
            "legacy-oversized",
            ForgeTaskLane::LargeSingleton,
            213,
        );
        let task_id = tasks.enqueue(&legacy).await.expect("enqueue legacy seed");
        make_legacy(&admin, task_id, true).await;

        let claim = tasks
            .claim_fair(Uuid::now_v7(), limits(1), None)
            .await
            .expect("claim query")
            .expect("oversized legacy claim");

        assert_eq!(claim.task_id, task_id);
        assert_eq!(claim.estimates.envelope, None);
        assert_eq!(claim.estimates.files, 1);
        assert_eq!(claim.estimates.memory_bytes, 100_000);
    }

    /// Converts one seeded version-one row into the migration-defined legacy shape.
    ///
    /// # Panics
    /// Panics when the exact test row cannot be converted.
    async fn make_legacy(admin: &PgPool, task_id: Uuid, oversized: bool) {
        let estimate = if oversized { 100_000_i64 } else { 100_i64 };
        sqlx::query(
            "UPDATE vala.forge_tasks SET envelope_version=0, decoded_batch_bytes=NULL, decoded_input_bytes=NULL, sort_working_bytes=NULL, sort_merge_reservation_bytes=NULL, encoder_buffer_bytes=NULL, upload_chunk_bytes=NULL, footer_encoded_bytes=NULL, footer_decode_workspace_bytes=NULL, sort_spill_bytes=NULL, output_scratch_bytes=NULL, estimated_files=1, estimated_bytes=$2, estimated_parallelism=1, estimated_memory_bytes=$2, estimated_spill_bytes=$2, large_task_ceiling_bytes=$2 WHERE task_id=$1",
        )
        .bind(task_id)
        .bind(estimate)
        .execute(admin)
        .await
        .expect("convert legacy row");
    }

    /// Persisted envelope terms drive both fair claim admission and skew visibility.
    #[tokio::test]
    async fn persisted_envelope_claim_gate_and_unclaimable_signal_match() {
        let (fixture, _admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let tenant = fixture.data_tenant_id();
        let fitting = task(tenant, "envelope-fit", ForgeTaskLane::Ordinary, 201);
        let mut oversized = task(tenant, "envelope-over", ForgeTaskLane::Ordinary, 202);
        oversized
            .estimates
            .envelope
            .as_mut()
            .expect("version-two test envelope")
            .encoder_buffer_bytes = 128 * 1024 * 1024;
        oversized.estimates.memory_bytes = oversized
            .estimates
            .envelope
            .expect("version-two test envelope")
            .memory_bytes()
            .expect("oversized resident total");
        tasks.enqueue(&fitting).await.expect("fitting envelope");
        tasks.enqueue(&oversized).await.expect("oversized envelope");
        let fitting_id = tasks.task_id_for_plan(&fitting).await.expect("fitting id");
        let oversized_id = tasks
            .task_id_for_plan(&oversized)
            .await
            .expect("oversized id");
        let claim = tasks
            .claim_fair(Uuid::now_v7(), limits(2), None)
            .await
            .expect("claim query")
            .expect("fitting task");
        assert_eq!(claim.task_id, fitting_id);
        assert_eq!(
            tasks
                .unclaimable_ready_task_ids(limits(2))
                .await
                .expect("unclaimable identities"),
            vec![oversized_id]
        );
    }

    /// Retry settlement persists the closed class, bounded delay, and volume deferral.
    #[tokio::test]
    async fn failure_taxonomy_backoff_and_volume_deferral_are_durable() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let tenant = fixture.data_tenant_id();
        let owner = Uuid::now_v7();
        let task = task(tenant, "fault-taxonomy", ForgeTaskLane::Ordinary, 203);
        let task_id = tasks.enqueue(&task).await.expect("enqueue fault task");
        let defaults: (i32, Option<String>, bool, Option<String>) = sqlx::query_as(
            "SELECT attempt_count,failure_class,next_eligible_at<=statement_timestamp(),failed_volume_identity FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task_id)
        .fetch_one(&admin)
        .await
        .expect("failure taxonomy defaults");
        assert_eq!(defaults, (0, None, true, None));
        let claim = tasks
            .claim_fair_for_volume(owner, limits(1), None, Some("volume-a"))
            .await
            .expect("initial claim")
            .expect("fault task");
        let attempt = claim.attempt_id.expect("attempt identity");

        assert_eq!(
            tasks
                .retry_failure(task_id, attempt, owner, "storage_health", Some("volume-a"))
                .await
                .expect("storage-health retry"),
            1
        );
        let persisted: (String, i32, Option<String>, Option<String>, bool) = sqlx::query_as(
            "SELECT state,attempt_count,failure_class,failed_volume_identity,next_eligible_at>statement_timestamp() FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task_id)
        .fetch_one(&admin)
        .await
        .expect("persisted failure taxonomy");
        assert_eq!(persisted.0, "retryable");
        assert_eq!(persisted.1, 1);
        assert_eq!(persisted.2.as_deref(), Some("storage_health"));
        assert_eq!(persisted.3.as_deref(), Some("volume-a"));
        assert!(persisted.4, "backoff remains in the future");
        assert!(
            tasks
                .claim_fair_for_volume(Uuid::now_v7(), limits(1), None, Some("volume-b"))
                .await
                .expect("claim during backoff")
                .is_none(),
            "all volumes honor next eligibility"
        );

        sqlx::query("UPDATE vala.forge_tasks SET ready_at=statement_timestamp(),next_eligible_at=statement_timestamp() WHERE task_id=$1")
            .bind(task_id)
            .execute(&admin)
            .await
            .expect("advance first eligibility");
        assert!(
            tasks
                .claim_fair_for_volume(Uuid::now_v7(), limits(1), None, Some("volume-a"))
                .await
                .expect("same-volume deferred claim")
                .is_none(),
            "the failed volume is softly deferred for one additional backoff"
        );
        let healthy_claim = tasks
            .claim_fair_for_volume(Uuid::now_v7(), limits(1), None, Some("volume-b"))
            .await
            .expect("different-volume claim")
            .expect("a healthy volume may take over");
        assert_eq!(healthy_claim.task_id, task_id);
    }

    /// Capacity refusal preserves retry budget while lease reclaim consumes it without audit.
    #[tokio::test]
    async fn capacity_refusal_and_expired_reclaim_have_distinct_settlement() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let tenant = fixture.data_tenant_id();
        let owner = Uuid::now_v7();
        let capacity = task(tenant, "capacity-refusal", ForgeTaskLane::Ordinary, 204);
        let capacity_id = tasks
            .enqueue(&capacity)
            .await
            .expect("enqueue capacity task");
        let claim = tasks
            .claim_fair(owner, limits(1), None)
            .await
            .expect("capacity claim")
            .expect("capacity task");
        tasks
            .release_capacity_refused(
                capacity_id,
                claim.attempt_id.expect("capacity attempt"),
                owner,
            )
            .await
            .expect("release capacity refusal");
        let capacity_row: (i32, Option<String>) = sqlx::query_as(
            "SELECT attempt_count,failure_class FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(capacity_id)
        .fetch_one(&admin)
        .await
        .expect("capacity settlement");
        assert_eq!(capacity_row.0, 0);
        assert_eq!(capacity_row.1.as_deref(), Some("capacity_refused"));

        sqlx::query("UPDATE vala.forge_tasks SET ready_at=statement_timestamp(),next_eligible_at=statement_timestamp() WHERE task_id=$1")
            .bind(capacity_id)
            .execute(&admin)
            .await
            .expect("make capacity task eligible");
        let reclaimed_claim = tasks
            .claim_fair(owner, limits(1), None)
            .await
            .expect("reclaim candidate")
            .expect("capacity task claimable again");
        let reclaimed_attempt = reclaimed_claim.attempt_id.expect("reclaimed attempt");
        sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1")
            .bind(capacity_id)
            .execute(&admin)
            .await
            .expect("expire attempt");
        let audit_before: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&admin)
            .await
            .expect("audit count before reclaim");
        assert_eq!(
            tasks
                .reclaim_expired_attempts(1)
                .await
                .expect("bounded reclaim"),
            vec![(capacity_id, reclaimed_attempt)]
        );
        let reclaimed: (String, i32, bool) = sqlx::query_as(
            "SELECT state,attempt_count,next_eligible_at>statement_timestamp() FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(capacity_id)
        .fetch_one(&admin)
        .await
        .expect("reclaimed state");
        assert_eq!(reclaimed.0, "retryable");
        assert_eq!(reclaimed.1, 1);
        assert!(reclaimed.2);
        let audit_after: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&admin)
            .await
            .expect("audit count after reclaim");
        assert_eq!(audit_after, audit_before, "reclaim remains audit-free");
    }

    /// Proves worker admission is independent of scheduler leadership and only
    /// advances its durable cursor after a successful fitting claim.
    ///
    /// # Panics
    /// Panics when PostgreSQL setup or cursor assertions fail.
    #[tokio::test]
    async fn worker_claim_cursor_is_independent_and_success_only() {
        let (fixture, admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let owner = Uuid::now_v7();
        let tenant = fixture.data_tenant_id();
        let initial: (Option<Uuid>,) = sqlx::query_as(
            "SELECT last_tenant_id FROM vala.forge_worker_claim_state WHERE singleton",
        )
        .fetch_one(&admin)
        .await
        .expect("initial worker cursor");
        assert_eq!(initial.0, None);
        assert!(
            tasks
                .claim_fair(owner, limits(1), None)
                .await
                .expect("empty claim")
                .is_none()
        );
        let empty: (Option<Uuid>,) = sqlx::query_as(
            "SELECT last_tenant_id FROM vala.forge_worker_claim_state WHERE singleton",
        )
        .fetch_one(&admin)
        .await
        .expect("empty worker cursor");
        assert_eq!(empty.0, None);

        tasks
            .enqueue(&task(tenant, "worker-only", ForgeTaskLane::Ordinary, 91))
            .await
            .expect("enqueue worker-only task");
        let mut no_fit = limits(1);
        no_fit.max_bytes = 1;
        assert!(
            tasks
                .claim_fair(owner, no_fit, None)
                .await
                .expect("no-fit claim")
                .is_none()
        );
        let unchanged: (Option<Uuid>,) = sqlx::query_as(
            "SELECT last_tenant_id FROM vala.forge_worker_claim_state WHERE singleton",
        )
        .fetch_one(&admin)
        .await
        .expect("no-fit worker cursor");
        assert_eq!(unchanged.0, None);

        let claim = tasks
            .claim_fair(owner, limits(1), None)
            .await
            .expect("independent claim")
            .expect("worker claims without scheduler lease");
        assert_eq!(claim.execution_tenant_id, tenant);
        assert_eq!(claim.data_tenant_id, tenant);
        let advanced: (Option<Uuid>,) = sqlx::query_as(
            "SELECT last_tenant_id FROM vala.forge_worker_claim_state WHERE singleton",
        )
        .fetch_one(&admin)
        .await
        .expect("advanced worker cursor");
        assert_eq!(advanced.0, Some(tenant.as_uuid()));
    }

    /// Superseded cancellation is terminal, audited, lane-releasing, and repairable.
    ///
    /// # Panics
    /// Panics when rollback, exact cancellation, demand repair, or reclaim
    /// invariants fail against PostgreSQL.
    #[tokio::test]
    async fn superseded_cancellation_is_atomic_terminal_progress() {
        let (fixture, admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant = fixture.data_tenant_id();
        let owner = Uuid::now_v7();
        let task_id = tasks
            .enqueue(&task(
                tenant,
                "superseded",
                ForgeTaskLane::LargeSingleton,
                92,
            ))
            .await
            .expect("enqueue superseded task");
        let claim = tasks
            .claim_fair(owner, limits(1), None)
            .await
            .expect("claim query")
            .expect("claimed task");
        assert_eq!(claim.task_id, task_id);

        {
            let mut rollback = TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .expect("rollback tenant connection");
            tasks
                .cancel_superseded(
                    &mut rollback,
                    &claim,
                    &event("forge.task.cancelled", task_id),
                )
                .await
                .expect("rollback cancellation");
        }
        let rolled_back: (String, i64, i64) = sqlx::query_as(
            "SELECT t.state,(SELECT count(*) FROM vala.forge_planning_demands),(SELECT count(*) FROM vala.audit_outbox) FROM vala.forge_tasks t WHERE t.task_id=$1",
        )
        .bind(task_id)
        .fetch_one(&admin)
        .await
        .expect("rolled-back cancellation state");
        assert_eq!(rolled_back.0, "claimed");
        assert_eq!((rolled_back.1, rolled_back.2), (0, 0));

        let mut commit = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("commit tenant connection");
        tasks
            .cancel_superseded(&mut commit, &claim, &event("forge.task.cancelled", task_id))
            .await
            .expect("commit cancellation");
        commit
            .commit()
            .await
            .expect("commit superseded cancellation");
        let terminal: (String, Option<Uuid>, Option<Uuid>, i64, i64) = sqlx::query_as(
            "SELECT t.state,t.attempt_id,t.claimed_by,(SELECT count(*) FROM vala.forge_planning_demands),(SELECT count(*) FROM vala.audit_outbox WHERE operation='forge.task.cancelled') FROM vala.forge_tasks t WHERE t.task_id=$1",
        )
        .bind(task_id)
        .fetch_one(&admin)
        .await
        .expect("committed cancellation state");
        assert_eq!(terminal, ("cancelled".to_owned(), None, None, 1, 1));
        assert_eq!(tasks.reclaim_expired(10).await.expect("reclaim"), 0);
    }

    /// Claims and starts one task with a caller-selected watermark.
    ///
    /// # Panics
    /// Panics when setup, claim, or start does not satisfy the test fixture.
    async fn claim_and_start(
        tasks: &ForgeTasks,
        task_id: Uuid,
        owner: Uuid,
        watermark: SnapshotWatermark,
    ) -> (Uuid, DataTenantId) {
        let claim = tasks
            .claim_fair(owner, limits(4), None)
            .await
            .expect("claim")
            .expect("claimed task");
        assert_eq!(claim.task_id, task_id);
        let attempt = claim.attempt_id.expect("attempt");
        tasks
            .start(task_id, attempt, owner, watermark)
            .await
            .expect("start");
        (attempt, claim.data_tenant_id)
    }

    /// Proves duplicate enqueue, concurrent at-most-once claim, and stale attempt fencing.
    ///
    /// # Panics
    /// Panics when PostgreSQL setup or a lifecycle assertion fails.
    #[tokio::test]
    async fn competing_claim_is_at_most_once_and_stale_attempts_fail() {
        let (fixture, _admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant = fixture.data_tenant_id();
        let id = tasks
            .enqueue(&task(tenant, "race", ForgeTaskLane::Ordinary, 1))
            .await
            .expect("enqueue");
        assert_eq!(
            id,
            tasks
                .enqueue(&task(tenant, "race", ForgeTaskLane::Ordinary, 1))
                .await
                .expect("duplicate")
        );
        let owner = Uuid::now_v7();
        let _fence = tasks
            .acquire_scheduler(owner, 30)
            .await
            .expect("scheduler")
            .expect("fence");
        let claim_limits = limits(1);
        let (left, right) = tokio::join!(
            tasks.claim_fair(owner, claim_limits, None),
            tasks.claim_fair(owner, claim_limits, None)
        );
        let claims = [left.expect("left"), right.expect("right")];
        assert_eq!(claims.iter().filter(|claim| claim.is_some()).count(), 1);
        let claim = claims.into_iter().flatten().next().expect("winner");
        let attempt = claim.attempt_id.expect("attempt");
        assert!(
            tasks
                .heartbeat(id, Uuid::now_v7(), owner, 30)
                .await
                .is_err()
        );
        assert!(
            tasks
                .heartbeat(id, attempt, Uuid::now_v7(), 30)
                .await
                .is_err()
        );
        tasks
            .start(
                id,
                attempt,
                owner,
                SnapshotWatermark {
                    snapshot_id: 900,
                    timestamp_ms: 10,
                },
            )
            .await
            .expect("start");
        assert!(
            tasks
                .retry(id, Uuid::now_v7(), owner, Utc::now())
                .await
                .is_err()
        );
        let mut oversized = task(tenant, "oversized", ForgeTaskLane::Ordinary, 11);
        oversized.estimates.bytes = 5_000;
        let oversized_id = tasks.enqueue(&oversized).await.expect("oversized enqueue");
        assert!(
            tasks
                .claim_fair(owner, limits(2), None)
                .await
                .expect("capacity claim")
                .is_none(),
            "ordinary work beyond byte capacity is not claimed"
        );
        let mut unschedulable = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("unschedulable conn");
        tasks
            .unschedulable(
                &mut unschedulable,
                oversized_id,
                &event("forge.task.unschedulable", oversized_id),
            )
            .await
            .expect("terminalize oversized");
        unschedulable.commit().await.expect("commit unschedulable");
    }

    /// Proves the durable tenant cursor rotates strictly forward across a
    /// scheduler takeover: after a leader claims one tenant's work and its fence
    /// expires, a successor leader resumes at the next tenant. The worker cursor
    /// is independent of scheduler leadership, so heartbeat expiry and reclaim
    /// of the first claim do not rewind the ring.
    ///
    /// # Panics
    /// Panics when PostgreSQL setup or a cursor assertion fails.
    #[tokio::test]
    async fn scheduler_takeover_preserves_cursor() {
        let (fixture, admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(tenant_b, "forge-second")
            .await
            .expect("seed tenant");
        tasks
            .enqueue(&task(tenant_a, "rotate-a", ForgeTaskLane::Ordinary, 2))
            .await
            .expect("a");
        tasks
            .enqueue(&task(tenant_b, "rotate-b", ForgeTaskLane::Ordinary, 3))
            .await
            .expect("b");
        let leader = Uuid::now_v7();
        let _fence = tasks
            .acquire_scheduler(leader, 30)
            .await
            .expect("leader")
            .expect("fence");
        let claim_limits = ForgeClaimLimits {
            lease_seconds: 1,
            ..limits(1)
        };
        let first = tasks
            .claim_fair(leader, claim_limits, None)
            .await
            .expect("first claim")
            .expect("first tenant");
        let first_attempt = first.attempt_id.expect("attempt");
        assert!(
            tasks
                .heartbeat(first.task_id, Uuid::now_v7(), leader, 3)
                .await
                .is_err(),
            "wrong attempt cannot renew"
        );
        assert!(
            tasks
                .heartbeat(first.task_id, first_attempt, Uuid::now_v7(), 3)
                .await
                .is_err(),
            "wrong owner cannot renew"
        );
        sqlx::query("UPDATE vala.forge_scheduler_state SET expires_at=statement_timestamp()-interval '1 second'").execute(&admin).await.expect("expire leader");
        sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1").bind(first.task_id).execute(&admin).await.expect("expire claim");
        assert!(
            tasks
                .heartbeat(first.task_id, first_attempt, leader, 3)
                .await
                .is_err(),
            "expired claim cannot be revived by heartbeat"
        );
        assert_eq!(tasks.reclaim_expired(10).await.expect("reclaim"), 1);
        let successor = Uuid::now_v7();
        let _successor_fence = tasks
            .acquire_scheduler(successor, 30)
            .await
            .expect("successor")
            .expect("takeover fence");
        let second = tasks
            .claim_fair(successor, claim_limits, None)
            .await
            .expect("successor claim")
            .expect("next tenant");
        assert_ne!(
            first.data_tenant_id, second.data_tenant_id,
            "cursor resumes strictly after prior tenant"
        );
    }

    /// Proves oversized single-file compactions on distinct tables run
    /// concurrently across owners, bounded only by the per-owner
    /// one-active-large rule (D78): two owners each claim a large task in the
    /// same window, while a second large claim by an owner that already holds an
    /// active large task is refused. Replaces the removed cluster-wide singleton.
    ///
    /// # Panics
    /// Panics when PostgreSQL setup or a concurrency assertion fails.
    #[tokio::test]
    async fn large_tasks_on_distinct_tables_claim_concurrently_across_owners() {
        let (fixture, _admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(tenant_b, "forge-second")
            .await
            .expect("seed tenant");
        tasks
            .enqueue(&task(tenant_a, "large-a", ForgeTaskLane::LargeSingleton, 2))
            .await
            .expect("a");
        tasks
            .enqueue(&task(tenant_b, "large-b", ForgeTaskLane::LargeSingleton, 3))
            .await
            .expect("b");
        let owner_a = Uuid::now_v7();
        let owner_b = Uuid::now_v7();
        let (left, right) = tokio::join!(
            tasks.claim_fair(owner_a, limits(1), None),
            tasks.claim_fair(owner_b, limits(1), None)
        );
        let claims = [
            left.expect("left").expect("owner A claims a large task"),
            right.expect("right").expect("owner B claims a large task"),
        ];
        assert_eq!(
            claims
                .iter()
                .filter(|c| c.lane == ForgeTaskLane::LargeSingleton)
                .count(),
            2,
            "both large tasks on distinct tables claim concurrently across owners"
        );
        assert_ne!(
            claims[0].task_id, claims[1].task_id,
            "each owner claims a distinct large task"
        );

        // The per-owner rule refuses a second concurrent large claim: enqueue a
        // third large task on a new table and prove owner A cannot take it while
        // its first large task is still active.
        tasks
            .enqueue(&task(
                tenant_a,
                "large-a-second",
                ForgeTaskLane::LargeSingleton,
                4,
            ))
            .await
            .expect("second large for owner A tenant");
        assert!(
            tasks
                .claim_fair(owner_a, limits(4), None)
                .await
                .expect("owner A second large claim")
                .is_none(),
            "owner A cannot hold two active large tasks at once"
        );
    }

    /// Proves an owner holding one active large task cannot claim a second large
    /// task until its first reaches a terminal state, at which point the freed
    /// per-owner slot admits the next large claim (D78).
    ///
    /// # Panics
    /// Panics when the per-owner large bound does not release on terminalization.
    #[tokio::test]
    async fn same_owner_second_large_claim_refused_until_terminal() {
        let (fixture, _admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant = fixture.data_tenant_id();
        let owner = Uuid::now_v7();
        let first_id = tasks
            .enqueue(&task(
                tenant,
                "large-first",
                ForgeTaskLane::LargeSingleton,
                21,
            ))
            .await
            .expect("first large");
        tasks
            .enqueue(&task(
                tenant,
                "large-second",
                ForgeTaskLane::LargeSingleton,
                22,
            ))
            .await
            .expect("second large");
        let first = tasks
            .claim_fair(owner, limits(4), None)
            .await
            .expect("first claim")
            .expect("owner claims first large");
        assert_eq!(first.task_id, first_id);
        let attempt = first.attempt_id.expect("attempt");
        assert!(
            tasks
                .claim_fair(owner, limits(4), None)
                .await
                .expect("second claim")
                .is_none(),
            "second large claim refused while first is active"
        );

        // Drive the first large task to a terminal state, freeing the owner's slot.
        tasks
            .start(
                first_id,
                attempt,
                owner,
                SnapshotWatermark {
                    snapshot_id: 21,
                    timestamp_ms: 21,
                },
            )
            .await
            .expect("start first large");
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        tasks
            .terminal(
                &mut conn,
                ForgeTaskTransition {
                    task_id: first_id,
                    attempt_id: attempt,
                    owner,
                    expected: ForgeTaskState::Running,
                    next: ForgeTaskState::Failed,
                },
                &event("forge.task.failed", first_id),
            )
            .await
            .expect("terminalize first large");
        conn.commit().await.expect("commit terminal");
        let second = tasks
            .claim_fair(owner, limits(4), None)
            .await
            .expect("post-terminal claim")
            .expect("owner claims second large after first terminal");
        assert_eq!(second.lane, ForgeTaskLane::LargeSingleton);
        assert_ne!(second.task_id, first_id, "distinct second large task");
    }

    /// Proves claimability excludes an active table generation before FIFO
    /// selection while independent ordinary work continues.
    ///
    /// # Panics
    /// Panics when same-table work consumes a slot or independent work stalls.
    #[tokio::test]
    async fn active_table_generation_defers_same_table_and_claims_independent_work() {
        let (fixture, _admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant = fixture.data_tenant_id();
        let owner = Uuid::now_v7();
        let first = tasks
            .enqueue(&task(tenant, "same-table", ForgeTaskLane::Ordinary, 61))
            .await
            .expect("first generation");
        tasks
            .enqueue(&task(tenant, "same-table", ForgeTaskLane::Ordinary, 62))
            .await
            .expect("second generation");
        let independent = tasks
            .enqueue(&task(tenant, "independent", ForgeTaskLane::Ordinary, 63))
            .await
            .expect("independent task");
        tasks
            .acquire_scheduler(owner, 30)
            .await
            .expect("scheduler")
            .expect("fence");
        let claimed_first = tasks
            .claim_fair(owner, limits(4), None)
            .await
            .expect("first claim")
            .expect("first task");
        assert_eq!(claimed_first.task_id, first);
        let claimed_second = tasks
            .claim_fair(owner, limits(4), None)
            .await
            .expect("second claim")
            .expect("independent task remains claimable");
        assert_eq!(claimed_second.task_id, independent);
        let deferred: String = sqlx::query_scalar(
            "SELECT state FROM vala.forge_tasks WHERE table_name='same-table' AND task_id<>$1",
        )
        .bind(first)
        .fetch_one(op.pool())
        .await
        .expect("deferred state");
        assert_eq!(deferred, "ready");
    }

    /// Proves that an owner holding an active large task still makes ordinary
    /// progress, that a same-owner second large claim is refused (D78), and that
    /// the database rejects a multi-file singleton-large row.
    ///
    /// # Panics
    /// Panics when ordinary progress stalls, the per-owner large bound leaks, or
    /// the durable single-file check weakens.
    #[tokio::test]
    async fn active_large_allows_ordinary_progress_refuses_second_large_and_rejects_multi_file() {
        let (fixture, admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant = fixture.data_tenant_id();
        let owner = Uuid::now_v7();
        let mut large = task(tenant, "large", ForgeTaskLane::LargeSingleton, 71);
        large.ready_at = Utc::now() - Duration::seconds(1);
        let large_id = tasks.enqueue(&large).await.expect("large task");
        let ordinary_id = tasks
            .enqueue(&task(tenant, "ordinary", ForgeTaskLane::Ordinary, 72))
            .await
            .expect("ordinary task");
        tasks
            .acquire_scheduler(owner, 30)
            .await
            .expect("scheduler")
            .expect("fence");
        assert_eq!(
            tasks
                .claim_fair(owner, limits(4), None)
                .await
                .expect("large claim")
                .expect("large task")
                .task_id,
            large_id
        );
        assert_eq!(
            tasks
                .claim_fair(owner, limits(4), None)
                .await
                .expect("ordinary claim")
                .expect("ordinary remains eligible")
                .task_id,
            ordinary_id
        );
        // A second large task on a distinct table is refused for the same owner
        // while its first large task is active, even though ordinary work flows.
        tasks
            .enqueue(&task(
                tenant,
                "large-second",
                ForgeTaskLane::LargeSingleton,
                73,
            ))
            .await
            .expect("second large task");
        assert!(
            tasks
                .claim_fair(owner, limits(4), None)
                .await
                .expect("second large claim")
                .is_none(),
            "same owner cannot hold two active large tasks"
        );
        let invalid = sqlx::query(
            "UPDATE vala.forge_tasks SET lane='large_singleton',estimated_files=2 WHERE task_id=$1",
        )
        .bind(ordinary_id)
        .execute(&admin)
        .await;
        assert!(
            invalid.is_err(),
            "PostgreSQL must enforce singleton file count"
        );
    }

    /// Proves malformed returned rows roll back the claim and cursor writes.
    ///
    /// # Panics
    /// Panics when malformed-row rollback is incomplete.
    #[tokio::test]
    async fn malformed_claim_rolls_back_all_coordination_state() {
        let (fixture, admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant = fixture.data_tenant_id();
        let task_id = tasks
            .enqueue(&task(
                tenant,
                "malformed-claim",
                ForgeTaskLane::LargeSingleton,
                12,
            ))
            .await
            .expect("enqueue");
        sqlx::query("UPDATE vala.forge_tasks SET plan=jsonb_set(plan,'{version}','99'::jsonb) WHERE task_id=$1").bind(task_id).execute(&admin).await.expect("corrupt plan");
        let owner = Uuid::now_v7();
        let _fence = tasks
            .acquire_scheduler(owner, 30)
            .await
            .expect("scheduler")
            .expect("fence");
        let scheduler_before: (i64, Option<Uuid>) = sqlx::query_as(
            "SELECT fencing_token,last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
        )
        .fetch_one(&admin)
        .await
        .expect("scheduler before");
        assert!(
            tasks.claim_fair(owner, limits(1), None).await.is_err(),
            "unknown persisted plan fails claim conversion"
        );
        let task_after: (String, Option<Uuid>, Option<Uuid>) = sqlx::query_as(
            "SELECT state,attempt_id,claimed_by FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task_id)
        .fetch_one(&admin)
        .await
        .expect("task after");
        assert_eq!(
            task_after,
            ("ready".to_owned(), None, None),
            "malformed task claim rolled back"
        );
        let scheduler_after: (i64, Option<Uuid>) = sqlx::query_as(
            "SELECT fencing_token,last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
        )
        .fetch_one(&admin)
        .await
        .expect("scheduler after");
        assert_eq!(
            scheduler_after, scheduler_before,
            "cursor and generation rolled back"
        );
    }

    /// Proves Prepared replay and both lifecycle audit boundaries are atomic under rollback.
    ///
    /// # Panics
    /// Panics when state/audit atomicity or replay assertions fail.
    #[tokio::test]
    async fn prepared_and_terminal_audit_are_atomic_and_replay_safe() {
        let (fixture, _admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant = fixture.data_tenant_id();
        let id = tasks
            .enqueue(&task(tenant, "audit", ForgeTaskLane::Ordinary, 4))
            .await
            .expect("enqueue");
        let owner = Uuid::now_v7();
        let _fence = tasks
            .acquire_scheduler(owner, 30)
            .await
            .expect("scheduler")
            .expect("fence");
        let (attempt, _) = claim_and_start(
            &tasks,
            id,
            owner,
            SnapshotWatermark {
                snapshot_id: 50,
                timestamp_ms: 500,
            },
        )
        .await;
        let evidence = ForgeTaskEvidence {
            version: 1,
            committed_snapshot_id: Some(51),
            committed_metadata_location: Some("metadata/v51.json".to_owned()),
            committed_metadata_digest: Some("digest".to_owned()),
            cleanup_candidates: vec![ForgeCleanupCandidate {
                category: ForgeCleanupCategory::Data,
                table: ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "audit")
                    .expect("candidate table"),
                path: ForgeCleanupPath::new("tenants/t/vala.bifrost/audit/data/old.parquet")
                    .expect("candidate path"),
            }],
            deleted_candidate_count: 0,
        };
        let prepared_event = event("forge.task.prepared", id);
        let wrong_table_evidence = ForgeTaskEvidence {
            cleanup_candidates: vec![ForgeCleanupCandidate {
                category: ForgeCleanupCategory::Data,
                table: ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "other")
                    .expect("wrong table"),
                path: ForgeCleanupPath::new("tenants/t/vala.bifrost/other/data/old.parquet")
                    .expect("wrong path"),
            }],
            ..evidence.clone()
        };
        let mut wrong_table_conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("wrong-table conn");
        assert!(
            tasks
                .prepared(
                    &mut wrong_table_conn,
                    id,
                    attempt,
                    owner,
                    &wrong_table_evidence,
                    &prepared_event
                )
                .await
                .is_err(),
            "cleanup evidence bound to another table is rejected before mutation"
        );
        drop(wrong_table_conn);
        {
            let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .expect("rollback conn");
            assert_eq!(
                tasks
                    .prepared(&mut conn, id, attempt, owner, &evidence, &prepared_event)
                    .await
                    .expect("prepared"),
                ForgeTaskTransitionOutcome::Applied
            );
        }
        let mut verify = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("verify");
        assert_eq!(
            tasks.status(&mut verify, 10).await.expect("status").tasks[0].state,
            ForgeTaskState::Running
        );
        let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&mut **verify.transaction())
            .await
            .expect("audit count");
        assert_eq!(audit_count, 0);
        verify.commit().await.expect("commit");
        let mut prepared_conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("prepared conn");
        assert_eq!(
            tasks
                .prepared(
                    &mut prepared_conn,
                    id,
                    attempt,
                    owner,
                    &evidence,
                    &prepared_event
                )
                .await
                .expect("prepared"),
            ForgeTaskTransitionOutcome::Applied
        );
        assert_eq!(
            tasks
                .prepared(
                    &mut prepared_conn,
                    id,
                    attempt,
                    owner,
                    &evidence,
                    &prepared_event
                )
                .await
                .expect("replay"),
            ForgeTaskTransitionOutcome::AlreadyApplied
        );
        prepared_conn.commit().await.expect("commit prepared");
        {
            let mut cursor_rollback = TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .expect("cursor rollback");
            tasks
                .advance_cleanup_cursor(&mut cursor_rollback, id, attempt, owner, 0, 1)
                .await
                .expect("advance rollback cursor");
        }
        let mut cursor_commit = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("cursor commit");
        assert!(
            tasks
                .advance_cleanup_cursor(&mut cursor_commit, id, attempt, owner, 1, 2)
                .await
                .is_err(),
            "a stale expected cursor fails closed"
        );
        tasks
            .advance_cleanup_cursor(&mut cursor_commit, id, attempt, owner, 0, 1)
            .await
            .expect("advance committed cursor");
        cursor_commit.commit().await.expect("commit cursor");
        let mut cursor_verify = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("cursor verify");
        let cursor_page = tasks
            .status(&mut cursor_verify, 10)
            .await
            .expect("cursor status");
        assert_eq!(
            cursor_page.tasks[0]
                .evidence
                .as_ref()
                .expect("Prepared evidence")
                .deleted_candidate_count,
            1
        );
        cursor_verify.commit().await.expect("commit cursor verify");
        let terminal = ForgeTaskTransition {
            task_id: id,
            attempt_id: attempt,
            owner,
            expected: ForgeTaskState::Prepared,
            next: ForgeTaskState::Succeeded,
        };
        {
            let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .expect("terminal rollback");
            tasks
                .terminal(&mut conn, terminal, &event("forge.task.succeeded", id))
                .await
                .expect("terminal");
        }
        let mut after = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("after rollback");
        let page = tasks.status(&mut after, 10).await.expect("status");
        assert_eq!(page.tasks[0].state, ForgeTaskState::Prepared);
        let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&mut **after.transaction())
            .await
            .expect("audit count");
        assert_eq!(audit_count, 1, "terminal state and audit both rolled back");
        assert!(
            tasks
                .terminal(
                    &mut after,
                    ForgeTaskTransition {
                        expected: ForgeTaskState::Running,
                        ..terminal
                    },
                    &event("forge.task.succeeded", id)
                )
                .await
                .is_err()
        );
        after.commit().await.expect("commit");
        let mut commit_terminal = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("terminal commit");
        assert_eq!(
            tasks
                .terminal(
                    &mut commit_terminal,
                    terminal,
                    &event("forge.task.succeeded", id),
                )
                .await
                .expect("terminal commit"),
            ForgeTaskTransitionOutcome::Applied
        );
        commit_terminal.commit().await.expect("commit terminal");
        assert!(tasks.retry(id, attempt, owner, Utc::now()).await.is_err());
        let mut terminal_replay = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("terminal replay");
        assert!(
            tasks
                .terminal(
                    &mut terminal_replay,
                    terminal,
                    &event("forge.task.succeeded", id),
                )
                .await
                .is_err(),
            "terminal task cannot reopen or terminalize twice"
        );
    }

    /// Prepared takeover preserves the committing generation and exact evidence.
    ///
    /// # Panics
    /// Panics when PostgreSQL setup or takeover invariants fail.
    #[tokio::test]
    async fn prepared_reconciliation_takeover_is_at_most_once() {
        let (fixture, admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant = fixture.data_tenant_id();
        let original_owner = Uuid::now_v7();
        let task_id = tasks
            .enqueue(&task(
                tenant,
                "prepared-takeover",
                ForgeTaskLane::Ordinary,
                73,
            ))
            .await
            .expect("enqueue");
        let claimed = tasks
            .claim_fair(original_owner, limits(1), None)
            .await
            .expect("claim")
            .expect("task");
        assert_eq!(claimed.task_id, task_id);
        let attempt = claimed.attempt_id.expect("attempt");
        tasks
            .start(
                task_id,
                attempt,
                original_owner,
                SnapshotWatermark {
                    snapshot_id: 73,
                    timestamp_ms: 73,
                },
            )
            .await
            .expect("start");
        let evidence = ForgeTaskEvidence {
            version: FORGE_TASK_PAYLOAD_VERSION,
            committed_snapshot_id: Some(730),
            committed_metadata_location: Some("metadata/v730.json".to_owned()),
            committed_metadata_digest: Some(format!("sha256:{}", "7".repeat(64))),
            cleanup_candidates: Vec::new(),
            deleted_candidate_count: 0,
        };
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        tasks
            .prepared(
                &mut conn,
                task_id,
                attempt,
                original_owner,
                &evidence,
                &event("forge.task.prepared", task_id),
            )
            .await
            .expect("Prepared transition");
        conn.commit().await.expect("commit Prepared");
        sqlx::query(
            "UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1",
        )
        .bind(task_id)
        .execute(&admin)
        .await
        .expect("expire Prepared owner");

        let successor_a = Uuid::now_v7();
        let successor_b = Uuid::now_v7();
        let (takeover_a, takeover_b) = tokio::join!(
            tasks.claim_prepared_for_reconciliation(successor_a, 30),
            tasks.claim_prepared_for_reconciliation(successor_b, 30),
        );
        let taken = [
            takeover_a.expect("takeover A"),
            takeover_b.expect("takeover B"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        assert_eq!(taken.len(), 1, "Prepared takeover is at most once");
        assert_eq!(taken[0].task_id, task_id);
        assert_eq!(taken[0].attempt_id, Some(attempt));
        assert_eq!(taken[0].state, ForgeTaskState::Prepared);
        assert_eq!(taken[0].evidence, Some(evidence));
        assert!(matches!(
            taken[0].claimed_by,
            Some(owner) if owner == successor_a || owner == successor_b
        ));
    }

    /// Proves prepared-reconciliation takeover works for a large task without
    /// the removed cluster-wide lease (D78): a successor owner assumes an expired
    /// Prepared large attempt via `claim_expires_at` expiry alone, keeping the
    /// committing generation and evidence.
    ///
    /// # Panics
    /// Panics when large-task Prepared takeover does not preserve its generation.
    #[tokio::test]
    async fn prepared_reconciliation_takeover_recovers_large_task() {
        let (fixture, admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant = fixture.data_tenant_id();
        let original_owner = Uuid::now_v7();
        let task_id = tasks
            .enqueue(&task(
                tenant,
                "large-prepared-takeover",
                ForgeTaskLane::LargeSingleton,
                74,
            ))
            .await
            .expect("enqueue");
        let claimed = tasks
            .claim_fair(original_owner, limits(1), None)
            .await
            .expect("claim")
            .expect("task");
        assert_eq!(claimed.task_id, task_id);
        assert_eq!(claimed.lane, ForgeTaskLane::LargeSingleton);
        let attempt = claimed.attempt_id.expect("attempt");
        tasks
            .start(
                task_id,
                attempt,
                original_owner,
                SnapshotWatermark {
                    snapshot_id: 74,
                    timestamp_ms: 74,
                },
            )
            .await
            .expect("start");
        let evidence = ForgeTaskEvidence {
            version: FORGE_TASK_PAYLOAD_VERSION,
            committed_snapshot_id: Some(740),
            committed_metadata_location: Some("metadata/v740.json".to_owned()),
            committed_metadata_digest: Some(format!("sha256:{}", "4".repeat(64))),
            cleanup_candidates: Vec::new(),
            deleted_candidate_count: 0,
        };
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        tasks
            .prepared(
                &mut conn,
                task_id,
                attempt,
                original_owner,
                &evidence,
                &event("forge.task.prepared", task_id),
            )
            .await
            .expect("Prepared transition");
        conn.commit().await.expect("commit Prepared");
        sqlx::query(
            "UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1",
        )
        .bind(task_id)
        .execute(&admin)
        .await
        .expect("expire Prepared owner");

        let successor = Uuid::now_v7();
        let taken = tasks
            .claim_prepared_for_reconciliation(successor, 30)
            .await
            .expect("takeover")
            .expect("large Prepared task recovered");
        assert_eq!(taken.task_id, task_id);
        assert_eq!(taken.attempt_id, Some(attempt));
        assert_eq!(taken.state, ForgeTaskState::Prepared);
        assert_eq!(taken.evidence, Some(evidence));
        assert_eq!(taken.claimed_by, Some(successor));
    }

    /// Proves success and successor demand share one caller-owned transaction.
    ///
    /// # Panics
    /// Panics when rollback exposes either effect or commit does not expose both.
    #[tokio::test]
    async fn terminal_success_atomically_requests_successor_demand() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let tenant = fixture.data_tenant_id();
        let table = ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "atomic-successor")
            .expect("table identity");
        let owner = Uuid::now_v7();
        let task_id = tasks
            .enqueue(&task(
                tenant,
                "atomic-successor",
                ForgeTaskLane::Ordinary,
                91,
            ))
            .await
            .expect("enqueue");
        let claim = tasks
            .claim_fair(owner, limits(1), None)
            .await
            .expect("claim")
            .expect("task");
        let attempt = claim.attempt_id.expect("attempt");
        tasks
            .start(
                task_id,
                attempt,
                owner,
                SnapshotWatermark {
                    snapshot_id: 91,
                    timestamp_ms: 91,
                },
            )
            .await
            .expect("start");
        let evidence = ForgeTaskEvidence {
            version: FORGE_TASK_PAYLOAD_VERSION,
            committed_snapshot_id: Some(92),
            committed_metadata_location: Some("metadata/v92.json".to_owned()),
            committed_metadata_digest: Some(format!("sha256:{}", "9".repeat(64))),
            cleanup_candidates: Vec::new(),
            deleted_candidate_count: 0,
        };
        let mut prepared = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("prepared tenant");
        tasks
            .prepared(
                &mut prepared,
                task_id,
                attempt,
                owner,
                &evidence,
                &event("forge.task.prepared", task_id),
            )
            .await
            .expect("prepared");
        prepared.commit().await.expect("commit prepared");

        let transition = ForgeTaskTransition {
            task_id,
            attempt_id: attempt,
            owner,
            expected: ForgeTaskState::Prepared,
            next: ForgeTaskState::Succeeded,
        };
        let mut rolled_back = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("rollback tenant");
        tasks
            .terminal_and_request_replan(
                &mut rolled_back,
                transition,
                &table,
                vala_sql::row_types::forge_tasks::TaskProgressEffect::Progressed,
                &event("forge.task.succeeded", task_id),
            )
            .await
            .expect("stage terminal and demand");
        drop(rolled_back);
        let state: String =
            sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(&admin)
                .await
                .expect("state after rollback");
        assert_eq!(state, "prepared");
        let demand_count: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND table_name=$2")
            .bind(tenant.as_uuid())
            .bind(&table.table)
            .fetch_one(&admin)
            .await
            .expect("demand after rollback");
        assert_eq!(demand_count, 0);

        let audit_before: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&admin)
            .await
            .expect("audit before");
        let mut committed = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("commit tenant");
        tasks
            .terminal_and_request_replan(
                &mut committed,
                transition,
                &table,
                vala_sql::row_types::forge_tasks::TaskProgressEffect::Progressed,
                &event("forge.task.succeeded", task_id),
            )
            .await
            .expect("commit terminal and demand");
        committed.commit().await.expect("commit both effects");
        let state: String =
            sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(&admin)
                .await
                .expect("state after commit");
        assert_eq!(state, "succeeded");
        let demand_generation: i64 = sqlx::query_scalar("SELECT generation FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND table_name=$2")
            .bind(tenant.as_uuid())
            .bind(&table.table)
            .fetch_one(&admin)
            .await
            .expect("successor demand");
        assert_eq!(demand_generation, 1);
        let audit_after: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&admin)
            .await
            .expect("audit after");
        assert_eq!(audit_after, audit_before + 1);

        let noop_table = ForgeTaskTableIdentity::new(
            "wyrd-redux",
            "vala.bifrost",
            "atomic-noop-acknowledgement",
        )
        .expect("no-op table identity");
        let noop_id = tasks
            .enqueue(&task(
                tenant,
                "atomic-noop-acknowledgement",
                ForgeTaskLane::Ordinary,
                93,
            ))
            .await
            .expect("enqueue no-op");
        let noop_claim = tasks
            .claim_fair(owner, limits(1), None)
            .await
            .expect("claim no-op")
            .expect("no-op task");
        let noop_attempt = noop_claim.attempt_id.expect("no-op attempt");
        tasks
            .start(
                noop_id,
                noop_attempt,
                owner,
                SnapshotWatermark {
                    snapshot_id: 93,
                    timestamp_ms: 93,
                },
            )
            .await
            .expect("start no-op");
        let mut noop_prepared = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("no-op prepared tenant");
        tasks
            .prepared(
                &mut noop_prepared,
                noop_id,
                noop_attempt,
                owner,
                &ForgeTaskEvidence {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    committed_snapshot_id: Some(93),
                    committed_metadata_location: Some("metadata/v93.json".to_owned()),
                    committed_metadata_digest: Some(format!("sha256:{}", "a".repeat(64))),
                    cleanup_candidates: Vec::new(),
                    deleted_candidate_count: 0,
                },
                &event("forge.task.prepared", noop_id),
            )
            .await
            .expect("prepare no-op");
        noop_prepared.commit().await.expect("commit no-op prepared");
        let mut noop_terminal = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("no-op terminal tenant");
        tasks
            .terminal_and_request_replan(
                &mut noop_terminal,
                ForgeTaskTransition {
                    task_id: noop_id,
                    attempt_id: noop_attempt,
                    owner,
                    expected: ForgeTaskState::Prepared,
                    next: ForgeTaskState::Succeeded,
                },
                &noop_table,
                vala_sql::row_types::forge_tasks::TaskProgressEffect::NoOpAcknowledged {
                    snapshot_id: 93,
                    commit_count: 7,
                },
                &event("forge.task.succeeded", noop_id),
            )
            .await
            .expect("terminal no-op");
        noop_terminal
            .commit()
            .await
            .expect("commit no-op acknowledgement");
        let acknowledged: (Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT acknowledged_snapshot_id,acknowledged_commit_count FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND table_name=$2",
        ).bind(tenant.as_uuid()).bind(&noop_table.table).fetch_one(&admin).await
            .expect("durable no-op acknowledgement");
        assert_eq!(acknowledged, (Some(93), Some(7)));
    }

    /// Proves bounded watermark/status failure modes, pruning safety, RLS, and grants.
    ///
    /// # Panics
    /// Panics when a database boundary or bounded-read assertion fails.
    #[tokio::test]
    async fn watermark_pruning_identity_and_database_boundaries_fail_closed() {
        let (fixture, admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant = fixture.data_tenant_id();
        assert!(ForgeTaskTableIdentity::new("unsupported", "vala.bifrost", "events").is_err());
        assert!(ForgeTaskTableIdentity::new("wyrd-redux", "unsupported", "events").is_err());
        assert!(ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "../events").is_err());
        let mut malformed = task(tenant, "malformed", ForgeTaskLane::Ordinary, 5);
        malformed.plan.version = 99;
        assert!(tasks.enqueue(&malformed).await.is_err());
        let _ready = tasks
            .enqueue(&task(tenant, "ready", ForgeTaskLane::Ordinary, 6))
            .await
            .expect("ready");
        let prepared_id = tasks
            .enqueue(&task(tenant, "prepared", ForgeTaskLane::Ordinary, 7))
            .await
            .expect("prepared enqueue");
        let terminal_id = tasks
            .enqueue(&task(tenant, "terminal", ForgeTaskLane::Ordinary, 8))
            .await
            .expect("terminal enqueue");
        let owner = Uuid::now_v7();
        let _fence = tasks
            .acquire_scheduler(owner, 30)
            .await
            .expect("scheduler")
            .expect("fence");
        let first = tasks
            .claim_fair(owner, limits(4), None)
            .await
            .expect("claim")
            .expect("claim");
        let attempt = first.attempt_id.expect("attempt");
        tasks
            .start(
                first.task_id,
                attempt,
                owner,
                SnapshotWatermark {
                    snapshot_id: 900,
                    timestamp_ms: 10,
                },
            )
            .await
            .expect("start");
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant");
        let identity = ForgeTaskTableIdentity::new(
            "wyrd-redux",
            "vala.bifrost",
            if first.task_id == prepared_id {
                "prepared"
            } else {
                "ready"
            },
        )
        .expect("identity");
        assert!(tasks.watermarks(&mut conn, &identity, 0).await.is_err());
        let (marks, overflow) = tasks
            .watermarks(&mut conn, &identity, 1)
            .await
            .expect("watermark");
        assert_eq!(marks.len(), 1);
        assert!(!overflow);
        assert_eq!(
            marks[0],
            SnapshotWatermark {
                snapshot_id: 900,
                timestamp_ms: 10
            }
        );
        conn.commit().await.expect("commit");
        sqlx::query("DROP INDEX vala.forge_tasks_publication_active")
            .execute(&admin)
            .await
            .expect("remove uniqueness only to arrange overflow corruption");
        sqlx::query("INSERT INTO vala.forge_tasks SELECT $2,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,base_snapshot_id+100,plan,$3,estimated_files,estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,large_task_ceiling_bytes,'running',$4,claimed_by,claim_expires_at,2,20,evidence,ready_at,created_at,updated_at FROM vala.forge_tasks WHERE task_id=$1")
            .bind(first.task_id).bind(Uuid::now_v7()).bind([10_u8;32].as_slice()).bind(Uuid::now_v7()).execute(&admin).await.expect("arrange second active watermark");
        let mut overflow_conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("overflow conn");
        let (_, overflow) = tasks
            .watermarks(&mut overflow_conn, &identity, 1)
            .await
            .expect("overflow watermark");
        assert!(overflow, "cap+1 watermark read reports overflow");
        overflow_conn.commit().await.expect("commit overflow");
        let pair_error=sqlx::query("UPDATE vala.forge_tasks SET watermark_snapshot_id=1,watermark_timestamp_ms=NULL WHERE task_id=$1").bind(first.task_id).execute(&admin).await;
        assert!(
            pair_error.is_err(),
            "database rejects missing watermark pair"
        );
        let negative_timestamp =
            sqlx::query("UPDATE vala.forge_tasks SET watermark_timestamp_ms=-1 WHERE task_id=$1")
                .bind(first.task_id)
                .execute(&admin)
                .await;
        assert!(
            negative_timestamp.is_err(),
            "database rejects overflow/invalid timestamp domain"
        );
        sqlx::query("UPDATE vala.forge_tasks SET state='prepared',evidence='{\"version\":1,\"committed_snapshot_id\":900,\"committed_metadata_location\":\"metadata/v900.json\",\"committed_metadata_digest\":\"digest\",\"cleanup_candidates\":[],\"deleted_candidate_count\":0}'::jsonb WHERE task_id=$1")
            .bind(first.task_id)
            .execute(&admin)
            .await
            .expect("arrange uncertain Prepared");
        sqlx::query("UPDATE vala.forge_tasks SET state='succeeded',updated_at=statement_timestamp()-interval '2 days' WHERE task_id=$1").bind(terminal_id).execute(&admin).await.expect("terminalize retained row");
        sqlx::query("UPDATE vala.forge_tasks SET updated_at=statement_timestamp()-interval '2 days' WHERE data_tenant_id=$1 AND task_id<>$2").bind(tenant.as_uuid()).bind(terminal_id).execute(&admin).await.expect("age nonterminal rows");
        let mut prune = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("prune");
        assert_eq!(
            tasks
                .prune_terminal(&mut prune, Utc::now() - Duration::days(1), 1)
                .await
                .expect("prune"),
            1,
            "one bounded terminal row pruned"
        );
        let page = tasks.status(&mut prune, 10).await.expect("status");
        assert!(
            page.tasks
                .iter()
                .any(|task| task.state == ForgeTaskState::Prepared),
            "Prepared uncertainty is preserved"
        );
        assert!(
            page.tasks
                .iter()
                .any(|task| task.state == ForgeTaskState::Ready),
            "Ready work is preserved"
        );
        prune.commit().await.expect("commit");
        let tenant_b = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(tenant_b, "forge-rls")
            .await
            .expect("seed b");
        let mut conn_b = TenantConn::acquire(fixture.app_pool(), tenant_b)
            .await
            .expect("tenant b");
        assert!(
            tasks
                .status(&mut conn_b, 10)
                .await
                .expect("rls status")
                .tasks
                .is_empty()
        );
        conn_b.commit().await.expect("commit b");
        let grants:Vec<(String,String)>=sqlx::query_as("SELECT grantee,privilege_type FROM information_schema.role_table_grants WHERE table_schema='vala' AND table_name='forge_tasks'").fetch_all(&admin).await.expect("grants");
        assert!(grants.iter().any(|v| v.0 == "wyrd_app" && v.1 == "SELECT"));
        assert!(
            !grants.iter().any(|v| v.0 == "wyrd_app" && v.1 == "INSERT"),
            "tenant role cannot bypass operator-owned enqueue"
        );
        assert!(
            grants
                .iter()
                .any(|v| v.0 == "wyrd_platform_admin" && v.1 == "UPDATE")
        );
        let invalid_id = tasks
            .enqueue(&task(tenant, "invalid-tenant", ForgeTaskLane::Ordinary, 9))
            .await
            .expect("invalid seed");
        let invalid_tenant = Uuid::nil();
        sqlx::query("UPDATE vala.forge_tasks SET data_tenant_id=$1 WHERE task_id=$2")
            .bind(invalid_tenant)
            .bind(invalid_id)
            .execute(&admin)
            .await
            .expect("corrupt tenant");
        let cursor_before: Option<Uuid> = sqlx::query_scalar(
            "SELECT last_tenant_id FROM vala.forge_worker_claim_state WHERE singleton",
        )
        .fetch_one(&admin)
        .await
        .expect("cursor before malformed tenant claim");
        assert!(
            tasks.claim_fair(owner, limits(4), None).await.is_err(),
            "persisted invalid tenant fails before returning claim"
        );
        let rollback: (String, Option<Uuid>, Option<Uuid>, Option<Uuid>) = sqlx::query_as(
            "SELECT state,attempt_id,claimed_by,(SELECT last_tenant_id FROM vala.forge_worker_claim_state WHERE singleton) FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(invalid_id)
        .fetch_one(&admin)
        .await
        .expect("malformed tenant claim rollback");
        assert_eq!(
            rollback,
            ("ready".to_owned(), None, None, cursor_before),
            "claim decoding failure rolls back task ownership and cursor"
        );
    }

    /// Proves coalescing generations, CAS acknowledgement, fencing, RLS, grants, and no audit.
    ///
    /// # Panics
    /// Panics when any durable demand invariant is violated.
    #[tokio::test]
    async fn planning_demand_is_fenced_bounded_and_operational_only() {
        let (fixture, admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant = fixture.data_tenant_id();
        let identity =
            ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "demand").expect("identity");
        let audit_before: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&admin)
            .await
            .expect("audit before");
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant");
        assert_eq!(
            tasks
                .upsert_hint(&mut conn, tenant, &identity)
                .await
                .expect("first"),
            1
        );
        assert_eq!(
            tasks
                .upsert_hint(&mut conn, tenant, &identity)
                .await
                .expect("duplicate"),
            2
        );
        assert!(
            tasks
                .upsert_hint(&mut conn, DataTenantId::new_v7(), &identity)
                .await
                .is_err()
        );
        conn.commit().await.expect("commit demand");
        assert_eq!(
            tasks
                .upsert_periodic(tenant, &identity)
                .await
                .expect("periodic"),
            3
        );
        let owner = Uuid::now_v7();
        let fence = tasks
            .acquire_scheduler(owner, 30)
            .await
            .expect("lease")
            .expect("fence");
        tasks
            .renew_scheduler(owner, fence, StdDuration::from_secs(30))
            .await
            .expect("renew exact scheduler generation");
        let generation_after_renewal: i64 = sqlx::query_scalar(
            "SELECT fencing_token FROM vala.forge_scheduler_state WHERE singleton",
        )
        .fetch_one(&admin)
        .await
        .expect("read renewed scheduler generation");
        assert_eq!(generation_after_renewal, fence);
        let (listed, overflowed) = tasks
            .planning_demands(owner, fence, 1)
            .await
            .expect("bounded list");
        assert!(!overflowed);
        assert_eq!(listed[0].generation, 3);
        let captured = listed[0].clone();
        let mismatched = task(
            DataTenantId::new_v7(),
            "demand",
            ForgeTaskLane::Ordinary,
            41,
        );
        assert!(
            tasks
                .enqueue_and_acknowledge(
                    owner,
                    fence,
                    &captured,
                    ForgeEnqueueBatch {
                        executable: std::slice::from_ref(&mismatched),
                        unschedulable: &[],
                    },
                    |id| event("forge.task.unschedulable", id),
                )
                .await
                .is_err(),
            "tenant mismatch must fail before any mutation"
        );
        let mismatched_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.forge_tasks WHERE plan_hash=$1")
                .bind([41_u8; 32].as_slice())
                .fetch_one(&admin)
                .await
                .expect("mismatch rollback count");
        assert_eq!(mismatched_count, 0);
        assert_eq!(
            tasks
                .upsert_periodic(tenant, &identity)
                .await
                .expect("newer"),
            4
        );
        let exact = task(tenant, "demand", ForgeTaskLane::Ordinary, 42);
        let stale_cursor_before: Option<Uuid> = sqlx::query_scalar(
            "SELECT last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
        )
        .fetch_one(&admin)
        .await
        .expect("stale cursor before");
        let stale_audit_before: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&admin)
            .await
            .expect("stale audit before");
        assert!(
            tasks
                .enqueue_and_acknowledge(
                    owner,
                    fence,
                    &captured,
                    ForgeEnqueueBatch {
                        executable: std::slice::from_ref(&exact),
                        unschedulable: &[],
                    },
                    |id| event("forge.task.unschedulable", id)
                )
                .await
                .is_err(),
            "stale demand generation must abort the transaction"
        );
        let stale_task_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.forge_tasks WHERE plan_hash=$1")
                .bind([42_u8; 32].as_slice())
                .fetch_one(&admin)
                .await
                .expect("stale task rollback");
        assert_eq!(stale_task_count, 0);
        let stale_audit_after: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&admin)
            .await
            .expect("stale audit after");
        assert_eq!(stale_audit_after, stale_audit_before);
        let stale_cursor_after: Option<Uuid> = sqlx::query_scalar(
            "SELECT last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
        )
        .fetch_one(&admin)
        .await
        .expect("stale cursor after");
        assert_eq!(stale_cursor_after, stale_cursor_before);
        let (retryable, _) = tasks
            .planning_demands(owner, fence, 1)
            .await
            .expect("retry list");
        assert_eq!(retryable[0].generation, 4);
        let successor = Uuid::now_v7();
        sqlx::query("UPDATE vala.forge_scheduler_state SET expires_at=statement_timestamp()-interval '1 second'").execute(&admin).await.expect("expire leader");
        let successor_fence = tasks
            .acquire_scheduler(successor, 30)
            .await
            .expect("takeover")
            .expect("successor fence");
        let stale_terminal = task(tenant, "demand", ForgeTaskLane::LargeSingleton, 44);
        assert!(
            tasks
                .enqueue_and_acknowledge(
                    owner,
                    fence,
                    &retryable[0],
                    ForgeEnqueueBatch {
                        executable: &[],
                        unschedulable: std::slice::from_ref(&stale_terminal),
                    },
                    |id| event("forge.task.unschedulable", id)
                )
                .await
                .is_err()
        );
        let stale_terminal_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.forge_tasks WHERE plan_hash=$1")
                .bind([44_u8; 32].as_slice())
                .fetch_one(&admin)
                .await
                .expect("stale task rollback");
        assert_eq!(
            stale_terminal_count, 0,
            "fence loss rolls back terminal task and audit"
        );
        assert_eq!(
            tasks
                .enqueue_and_acknowledge(
                    successor,
                    successor_fence,
                    &retryable[0],
                    ForgeEnqueueBatch {
                        executable: std::slice::from_ref(&exact),
                        unschedulable: &[],
                    },
                    |id| event("forge.task.unschedulable", id)
                )
                .await
                .expect("successor ack"),
            1
        );
        assert!(
            tasks
                .enqueue_and_acknowledge(
                    successor,
                    successor_fence,
                    &retryable[0],
                    ForgeEnqueueBatch {
                        executable: std::slice::from_ref(&exact),
                        unschedulable: &[],
                    },
                    |id| event("forge.task.unschedulable", id),
                )
                .await
                .is_err(),
            "replayed acknowledged generation must fail without duplicates"
        );
        let committed_exact_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.forge_tasks WHERE plan_hash=$1")
                .bind([42_u8; 32].as_slice())
                .fetch_one(&admin)
                .await
                .expect("committed exact count");
        assert_eq!(committed_exact_count, 1);
        assert!(
            tasks
                .planning_demands(successor, successor_fence, 1)
                .await
                .expect("empty")
                .0
                .is_empty()
        );
        tasks
            .upsert_periodic(tenant, &identity)
            .await
            .expect("terminal demand");
        let terminal_demand = tasks
            .planning_demands(successor, successor_fence, 1)
            .await
            .expect("terminal list")
            .0
            .remove(0);
        let terminal = task(tenant, "demand", ForgeTaskLane::LargeSingleton, 43);
        assert_eq!(
            tasks
                .enqueue_and_acknowledge(
                    successor,
                    successor_fence,
                    &terminal_demand,
                    ForgeEnqueueBatch {
                        executable: &[],
                        unschedulable: std::slice::from_ref(&terminal),
                    },
                    |id| event("forge.task.unschedulable", id)
                )
                .await
                .expect("terminal ack"),
            1
        );
        let terminal_count: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name='demand' AND state='unschedulable'").bind(tenant.as_uuid()).fetch_one(&admin).await.expect("terminal count");
        assert_eq!(terminal_count, 1);
        let audit_after: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&admin)
            .await
            .expect("audit after");
        assert_eq!(
            audit_after,
            audit_before + 1,
            "only terminal Unschedulable emits audit; demand coordination does not"
        );
        let forced: bool = sqlx::query_scalar("SELECT relforcerowsecurity FROM pg_class WHERE oid='vala.forge_planning_demands'::regclass").fetch_one(&admin).await.expect("forced RLS");
        assert!(forced);
        let grants: Vec<(String, String)> = sqlx::query_as("SELECT grantee,privilege_type FROM information_schema.role_table_grants WHERE table_schema='vala' AND table_name='forge_planning_demands'").fetch_all(&admin).await.expect("grants");
        assert!(
            grants
                .iter()
                .any(|value| value.0 == "wyrd_app" && value.1 == "INSERT")
        );
        assert!(
            !grants
                .iter()
                .any(|value| value.0 == "wyrd_app" && value.1 == "DELETE")
        );
        let audit_grants: Vec<(String, String, String)> = sqlx::query_as("SELECT table_name,grantee,privilege_type FROM information_schema.role_table_grants WHERE table_schema='vala' AND table_name IN ('audit_chain_head','audit_outbox')").fetch_all(&admin).await.expect("audit grants");
        let mut operator_audit_grants = audit_grants
            .iter()
            .filter(|value| value.1 == "wyrd_platform_admin")
            .map(|value| (value.0.clone(), value.2.clone()))
            .collect::<Vec<_>>();
        operator_audit_grants.sort();
        assert_eq!(
            operator_audit_grants,
            vec![
                ("audit_chain_head".to_owned(), "INSERT".to_owned()),
                ("audit_chain_head".to_owned(), "SELECT".to_owned()),
                ("audit_chain_head".to_owned(), "UPDATE".to_owned()),
                ("audit_outbox".to_owned(), "INSERT".to_owned()),
            ],
            "operator audit authority is append-only and exact"
        );
        assert!(
            audit_grants
                .iter()
                .any(|value| value.0 == "audit_chain_head"
                    && value.1 == "wyrd_platform_admin"
                    && value.2 == "UPDATE")
        );
        assert!(audit_grants.iter().any(|value| value.0 == "audit_outbox"
            && value.1 == "wyrd_platform_admin"
            && value.2 == "INSERT"));
        assert!(!audit_grants.iter().any(|value| value.0 == "audit_outbox"
            && value.1 == "wyrd_platform_admin"
            && matches!(value.2.as_str(), "SELECT" | "UPDATE" | "DELETE")));
        assert!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM vala.audit_outbox")
                .fetch_one(op.pool())
                .await
                .is_err(),
            "operator cannot directly read tenant audit rows"
        );
        assert!(
            sqlx::query("UPDATE vala.audit_outbox SET result=result")
                .execute(op.pool())
                .await
                .is_err(),
            "operator cannot directly update tenant audit rows"
        );
        assert!(
            sqlx::query("DELETE FROM vala.audit_outbox")
                .execute(op.pool())
                .await
                .is_err(),
            "operator cannot directly delete tenant audit rows"
        );
        sqlx::query("INSERT INTO vala.forge_planning_demands(data_tenant_id,catalog_name,namespace_name,table_name,last_source) SELECT $1,'wyrd-redux','vala.bifrost','scale-'||g,'periodic' FROM generate_series(1,1000) g").bind(tenant.as_uuid()).execute(&admin).await.expect("scale demands");
        sqlx::query("SET enable_seqscan=off")
            .execute(&admin)
            .await
            .expect("force index visibility");
        let plan: Vec<(String,)> = sqlx::query_as("EXPLAIN (FORMAT TEXT) SELECT data_tenant_id FROM vala.forge_planning_demands ORDER BY data_tenant_id,last_requested_at,catalog_name,namespace_name,table_name LIMIT 10").fetch_all(&admin).await.expect("explain");
        assert!(
            plan.iter()
                .any(|line| line.0.contains("forge_planning_demands_ring")),
            "bounded scale query can use the ring index: {plan:?}"
        );
    }

    /// Proves failures after task, audit, demand, and cursor mutations roll
    /// back the complete fenced planning transaction.
    ///
    /// # Panics
    /// Panics when failure injection or any rollback assertion fails.
    #[tokio::test]
    async fn enqueue_failure_after_each_atomic_step_rolls_back_everything() {
        let (fixture, admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant = fixture.data_tenant_id();
        let identity = ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "step-failure")
            .expect("identity");
        let owner = Uuid::now_v7();
        let fence = tasks
            .acquire_scheduler(owner, 30)
            .await
            .expect("scheduler")
            .expect("fence");
        sqlx::query("CREATE FUNCTION vala.fail_forge_planning_step() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected Forge planning failure'; END $$")
            .execute(&admin).await.expect("failure function");
        let stages = [
            (
                "audit_outbox",
                "CREATE TRIGGER forge_fail_step BEFORE INSERT ON vala.audit_outbox FOR EACH ROW EXECUTE FUNCTION vala.fail_forge_planning_step()",
            ),
            (
                "forge_planning_demands",
                "CREATE TRIGGER forge_fail_step BEFORE DELETE ON vala.forge_planning_demands FOR EACH ROW EXECUTE FUNCTION vala.fail_forge_planning_step()",
            ),
            (
                "forge_scheduler_state",
                "CREATE TRIGGER forge_fail_step BEFORE UPDATE ON vala.forge_scheduler_state FOR EACH ROW EXECUTE FUNCTION vala.fail_forge_planning_step()",
            ),
            (
                "forge_scheduler_state",
                "CREATE CONSTRAINT TRIGGER forge_fail_step AFTER UPDATE ON vala.forge_scheduler_state DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION vala.fail_forge_planning_step()",
            ),
        ];
        for (index, (trigger_table, create_trigger)) in stages.into_iter().enumerate() {
            tasks
                .upsert_periodic(tenant, &identity)
                .await
                .expect("demand");
            let demand = tasks
                .planning_demands(owner, fence, 1)
                .await
                .expect("page")
                .0
                .remove(0);
            let cursor_before: Option<Uuid> = sqlx::query_scalar(
                "SELECT last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
            )
            .fetch_one(&admin)
            .await
            .expect("cursor before");
            let audit_before: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
                .fetch_one(&admin)
                .await
                .expect("audit before");
            sqlx::query(sqlx::AssertSqlSafe(create_trigger.to_owned()))
                .execute(&admin)
                .await
                .expect("create trigger");
            let hash = u8::try_from(80 + index).expect("bounded stage");
            let terminal = task(tenant, "step-failure", ForgeTaskLane::LargeSingleton, hash);
            assert!(
                tasks
                    .enqueue_and_acknowledge(
                        owner,
                        fence,
                        &demand,
                        ForgeEnqueueBatch {
                            executable: &[],
                            unschedulable: std::slice::from_ref(&terminal)
                        },
                        |id| event("forge.task.unschedulable", id)
                    )
                    .await
                    .is_err()
            );
            let drop_trigger = format!("DROP TRIGGER forge_fail_step ON vala.{trigger_table}");
            sqlx::query(sqlx::AssertSqlSafe(drop_trigger))
                .execute(&admin)
                .await
                .expect("drop trigger");
            let task_count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM vala.forge_tasks WHERE plan_hash=$1")
                    .bind([hash; 32].as_slice())
                    .fetch_one(&admin)
                    .await
                    .expect("task count");
            assert_eq!(task_count, 0, "task stage {index}");
            let audit_after: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
                .fetch_one(&admin)
                .await
                .expect("audit after");
            assert_eq!(audit_after, audit_before, "audit stage {index}");
            let generation: Option<i64> = sqlx::query_scalar("SELECT generation FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND catalog_name=$2 AND namespace_name=$3 AND table_name=$4").bind(tenant.as_uuid()).bind(&identity.catalog).bind(&identity.namespace).bind(&identity.table).fetch_optional(&admin).await.expect("demand generation");
            assert_eq!(generation, Some(demand.generation), "demand stage {index}");
            let cursor_after: Option<Uuid> = sqlx::query_scalar(
                "SELECT last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
            )
            .fetch_one(&admin)
            .await
            .expect("cursor after");
            assert_eq!(cursor_after, cursor_before, "cursor stage {index}");
        }
        sqlx::query("DROP FUNCTION vala.fail_forge_planning_step()")
            .execute(&admin)
            .await
            .expect("drop failure function");
    }

    /// Proves strict-after tenant rotation, wraparound, takeover, RLS, and malformed-row refusal.
    ///
    /// # Panics
    /// Panics when the durable cursor or isolation boundary deviates.
    #[tokio::test]
    async fn planning_demand_cursor_bounds_hot_tenant_across_takeover() {
        let (fixture, admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let hot = fixture.data_tenant_id();
        let cold = DataTenantId::new_v7();
        sqlx::query("INSERT INTO platform.tenants(data_tenant_id,slug,display_name,status) VALUES ($1,$2,$3,'active')").bind(cold.as_uuid()).bind(format!("cold-{cold}")).bind("cold").execute(&admin).await.expect("cold tenant");
        let identity = ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "rotation")
            .expect("identity");
        tasks
            .upsert_periodic(hot, &identity)
            .await
            .expect("hot demand");
        let hot_second =
            ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "rotation-hot-second")
                .expect("second hot identity");
        tasks
            .upsert_periodic(hot, &hot_second)
            .await
            .expect("second hot demand");
        tasks
            .upsert_periodic(cold, &identity)
            .await
            .expect("cold demand");
        let owner = Uuid::now_v7();
        let fence = tasks
            .acquire_scheduler(owner, 30)
            .await
            .expect("lease")
            .expect("fence");
        let page = tasks
            .planning_demands(owner, fence, 2)
            .await
            .expect("tenant page")
            .0;
        assert_eq!(page.len(), 2);
        assert_ne!(
            page[0].data_tenant_id, page[1].data_tenant_id,
            "one hot tenant cannot fill the bounded page"
        );
        let first = tasks
            .planning_demands(owner, fence, 1)
            .await
            .expect("first")
            .0
            .remove(0);
        let retained = tasks
            .planning_demands(owner, fence, 1)
            .await
            .expect("retry after interrupted planning")
            .0
            .remove(0);
        assert_eq!(
            retained.data_tenant_id, first.data_tenant_id,
            "planning failure or cancellation without acknowledgement retains demand"
        );
        let (backlog, oldest, overflowed) =
            tasks.planning_status(1).await.expect("incomplete status");
        assert_eq!(backlog, 1);
        assert!(oldest.is_some());
        assert!(
            overflowed,
            "two pending demands make a cap-one gauge pass incomplete"
        );
        tasks
            .upsert_periodic(first.data_tenant_id, &first.table_ref)
            .await
            .expect("concurrent newer generation");
        assert!(
            tasks
                .enqueue_and_acknowledge(
                    owner,
                    fence,
                    &first,
                    ForgeEnqueueBatch {
                        executable: &[],
                        unschedulable: &[],
                    },
                    |id| event("forge.task.unschedulable", id),
                )
                .await
                .is_err(),
            "a stale demand generation fails the atomic acknowledgement"
        );
        let current = tasks
            .planning_demands(owner, fence, 1)
            .await
            .expect("current generation after rollback")
            .0
            .remove(0);
        assert_eq!(
            current.data_tenant_id, first.data_tenant_id,
            "a stale generation conflict leaves the tenant cursor unchanged"
        );
        assert_eq!(
            tasks
                .enqueue_and_acknowledge(
                    owner,
                    fence,
                    &current,
                    ForgeEnqueueBatch {
                        executable: &[],
                        unschedulable: &[],
                    },
                    |id| event("forge.task.unschedulable", id),
                )
                .await
                .expect("acknowledge current generation"),
            0
        );
        let second = tasks
            .planning_demands(owner, fence, 1)
            .await
            .expect("second")
            .0
            .remove(0);
        assert_ne!(
            first.data_tenant_id, second.data_tenant_id,
            "acknowledging the current generation advances to the other tenant within bound two"
        );
        sqlx::query("UPDATE vala.forge_scheduler_state SET expires_at=statement_timestamp()-interval '1 second'").execute(&admin).await.expect("expire");
        assert!(
            tasks.planning_demands(owner, fence, 1).await.is_err(),
            "expired leadership cannot appear as a live empty demand page"
        );
        assert!(
            tasks
                .renew_scheduler(owner, fence, StdDuration::from_secs(30))
                .await
                .is_err(),
            "expired leadership cannot be renewed"
        );
        let successor = Uuid::now_v7();
        let successor_fence = tasks
            .acquire_scheduler(successor, 30)
            .await
            .expect("takeover")
            .expect("fence");
        assert!(
            tasks.planning_demands(owner, fence, 1).await.is_err(),
            "replaced leadership remains stale"
        );
        let resumed = tasks
            .planning_demands(successor, successor_fence, 1)
            .await
            .expect("resumed")
            .0
            .remove(0);
        assert_eq!(
            resumed.data_tenant_id, second.data_tenant_id,
            "takeover preserves cursor-selected pending tenant"
        );
        let mut other = TenantConn::acquire(fixture.app_pool(), DataTenantId::new_v7())
            .await
            .expect("other tenant conn");
        let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.forge_planning_demands")
            .fetch_one(&mut **other.transaction())
            .await
            .expect("RLS read");
        assert_eq!(
            visible, 0,
            "forced RLS denies cross-tenant demand visibility"
        );
        other.commit().await.expect("other commit");
        sqlx::query("ALTER TABLE vala.forge_planning_demands DROP CONSTRAINT forge_planning_demands_last_source_check").execute(&admin).await.expect("drop source check");
        sqlx::query("UPDATE vala.forge_planning_demands SET last_source='malformed' WHERE data_tenant_id=$1").bind(second.data_tenant_id.as_uuid()).execute(&admin).await.expect("corrupt source");
        assert!(
            tasks
                .planning_demands(successor, successor_fence, 2)
                .await
                .is_err(),
            "malformed persisted demand fails closed"
        );
    }

    /// Proves a maintenance-filtered claim reaches a ready maintenance task past
    /// a compaction backlog, and that an unfiltered claim still drains
    /// compaction.
    ///
    /// This is the durable half of the reserved maintenance slot: even with
    /// more ready compaction tasks than workers, the strategy-filtered claim a
    /// reserved slot issues first always selects maintenance when it is ready,
    /// so snapshot expiry cannot be starved by compaction load.
    ///
    /// # Panics
    /// Panics when PostgreSQL setup or claim assertions fail.
    #[tokio::test]
    async fn maintenance_strategy_filter_claims_past_compaction_backlog() {
        let (fixture, _admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let owner = Uuid::now_v7();
        let tenant = fixture.data_tenant_id();

        // A compaction backlog across distinct tables, each independently ready.
        for index in 0..3_u8 {
            tasks
                .enqueue(&task(
                    tenant,
                    &format!("compaction_{index}"),
                    ForgeTaskLane::Ordinary,
                    index,
                ))
                .await
                .expect("enqueue compaction backlog");
        }
        // One ready maintenance task on its own table.
        tasks
            .enqueue(&maintenance_task(tenant, "maintenance_table", 200))
            .await
            .expect("enqueue maintenance");

        // The reserved slot's first attempt: restricted to maintenance
        // strategies, it selects the maintenance task despite the larger,
        // fair-ordered compaction backlog.
        let maintenance = tasks
            .claim_fair(owner, limits(8), Some(MAINTENANCE_STRATEGIES))
            .await
            .expect("maintenance-filtered claim")
            .expect("maintenance task claimable past backlog");
        assert_eq!(
            maintenance.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry),
            "filtered claim must select the maintenance strategy"
        );

        // The unfiltered fallback still claims a compaction task from the
        // remaining backlog, so the reservation does not starve compaction.
        let compaction = tasks
            .claim_fair(owner, limits(8), None)
            .await
            .expect("unfiltered fallback claim")
            .expect("compaction task remains claimable");
        assert_eq!(
            compaction.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles),
            "unfiltered claim drains the compaction backlog"
        );
    }
}
