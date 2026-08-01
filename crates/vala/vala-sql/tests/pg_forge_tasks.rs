mod pg_tests {
    //! Real-Postgres lifecycle, fencing, audit, and isolation proofs for Forge tasks.

    use chrono::{Duration, Utc};
    use sqlx::{PgPool, types::Uuid};
    use vala_sql::queries::forge_tasks::{ForgeClaimLimits, ForgeEnqueueBatch, ForgeTasks};
    use vala_sql::row_types::forge_tasks::{
        FORGE_TASK_PAYLOAD_VERSION, ForgeCleanupCandidate, ForgeCleanupCategory, ForgeCleanupPath,
        ForgeTaskEstimates, ForgeTaskEvidence, ForgeTaskLane, ForgeTaskPlan, ForgeTaskState,
        ForgeTaskStrategy, ForgeTaskTableIdentity, ForgeTaskTransition, ForgeTaskTransitionOutcome,
        NewForgeTask, SnapshotWatermark,
    };
    use vala_sql::{OperatorPool, TenantConn};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

    /// Exact persisted singleton large-lane state used by rollback assertions.
    type LargeLeaseSnapshot = (
        Option<Uuid>,
        Option<Uuid>,
        Option<Uuid>,
        i64,
        Option<chrono::DateTime<Utc>>,
    );

    /// Starts one isolated migrated database and returns its administrative pool.
    ///
    /// # Panics
    /// Panics when the repository PostgreSQL fixture cannot start.
    async fn setup() -> (PgFixture, PgPool) {
        let fixture = PgFixture::start().await.expect("fixture");
        let admin = fixture.superuser_pool().await.expect("admin pool");
        (fixture, admin)
    }

    /// Builds one valid enqueue request with caller-selected identity and lane.
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
                memory_bytes: 100,
                spill_bytes: 100,
                large_ceiling_bytes: 1000,
            },
            ready_at: Utc::now(),
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
            max_memory_bytes: 1_000,
            max_spill_bytes: 1_000,
            max_large_task_bytes: 2_000,
        }
    }

    /// Proves worker admission is independent of scheduler leadership and only
    /// advances its durable cursor after a successful fitting claim.
    ///
    /// # Panics
    /// Panics when PostgreSQL setup or cursor assertions fail.
    #[tokio::test]
    async fn worker_claim_cursor_is_independent_and_success_only() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new();
        let op = fixture.operator_pool();
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
                .claim_fair(op, owner, limits(1))
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
            .enqueue(
                op,
                &task(tenant, "worker-only", ForgeTaskLane::Ordinary, 91),
            )
            .await
            .expect("enqueue worker-only task");
        let mut no_fit = limits(1);
        no_fit.max_bytes = 1;
        assert!(
            tasks
                .claim_fair(op, owner, no_fit)
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
            .claim_fair(op, owner, limits(1))
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
        let tasks = ForgeTasks::new();
        let op = fixture.operator_pool();
        let tenant = fixture.data_tenant_id();
        let owner = Uuid::now_v7();
        let task_id = tasks
            .enqueue(
                op,
                &task(tenant, "superseded", ForgeTaskLane::LargeSingleton, 92),
            )
            .await
            .expect("enqueue superseded task");
        let claim = tasks
            .claim_fair(op, owner, limits(1))
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
        let rolled_back: (String, Option<Uuid>, i64, i64) = sqlx::query_as(
            "SELECT t.state,l.task_id,(SELECT count(*) FROM vala.forge_planning_demands),(SELECT count(*) FROM vala.audit_outbox) FROM vala.forge_tasks t CROSS JOIN vala.forge_large_lane_lease l WHERE t.task_id=$1 AND l.singleton",
        )
        .bind(task_id)
        .fetch_one(&admin)
        .await
        .expect("rolled-back cancellation state");
        assert_eq!(rolled_back.0, "claimed");
        assert_eq!(rolled_back.1, Some(task_id));
        assert_eq!((rolled_back.2, rolled_back.3), (0, 0));

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
        let terminal: (String, Option<Uuid>, Option<Uuid>, Option<Uuid>, i64, i64) =
            sqlx::query_as(
                "SELECT t.state,t.attempt_id,t.claimed_by,l.task_id,(SELECT count(*) FROM vala.forge_planning_demands),(SELECT count(*) FROM vala.audit_outbox WHERE operation='forge.task.cancelled') FROM vala.forge_tasks t CROSS JOIN vala.forge_large_lane_lease l WHERE t.task_id=$1 AND l.singleton",
            )
            .bind(task_id)
            .fetch_one(&admin)
            .await
            .expect("committed cancellation state");
        assert_eq!(terminal, ("cancelled".to_owned(), None, None, None, 1, 1));
        assert_eq!(tasks.reclaim_expired(op, 10).await.expect("reclaim"), 0);
    }

    /// Claims and starts one task with a caller-selected watermark.
    ///
    /// # Panics
    /// Panics when setup, claim, or start does not satisfy the test fixture.
    async fn claim_and_start(
        tasks: &ForgeTasks,
        op: &OperatorPool,
        task_id: Uuid,
        owner: Uuid,
        watermark: SnapshotWatermark,
    ) -> (Uuid, DataTenantId) {
        let claim = tasks
            .claim_fair(op, owner, limits(4))
            .await
            .expect("claim")
            .expect("claimed task");
        assert_eq!(claim.task_id, task_id);
        let attempt = claim.attempt_id.expect("attempt");
        tasks
            .start(op, task_id, attempt, owner, watermark)
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
        let tasks = ForgeTasks::new();
        let op = fixture.operator_pool();
        let tenant = fixture.data_tenant_id();
        let id = tasks
            .enqueue(op, &task(tenant, "race", ForgeTaskLane::Ordinary, 1))
            .await
            .expect("enqueue");
        assert_eq!(
            id,
            tasks
                .enqueue(op, &task(tenant, "race", ForgeTaskLane::Ordinary, 1))
                .await
                .expect("duplicate")
        );
        let owner = Uuid::now_v7();
        let _fence = tasks
            .acquire_scheduler(op, owner, 30)
            .await
            .expect("scheduler")
            .expect("fence");
        let claim_limits = limits(1);
        let (left, right) = tokio::join!(
            tasks.claim_fair(op, owner, claim_limits),
            tasks.claim_fair(op, owner, claim_limits)
        );
        let claims = [left.expect("left"), right.expect("right")];
        assert_eq!(claims.iter().filter(|claim| claim.is_some()).count(), 1);
        let claim = claims.into_iter().flatten().next().expect("winner");
        let attempt = claim.attempt_id.expect("attempt");
        assert!(
            tasks
                .heartbeat(op, id, Uuid::now_v7(), owner, 30)
                .await
                .is_err()
        );
        assert!(
            tasks
                .heartbeat(op, id, attempt, Uuid::now_v7(), 30)
                .await
                .is_err()
        );
        tasks
            .start(
                op,
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
                .retry(op, id, Uuid::now_v7(), owner, Utc::now())
                .await
                .is_err()
        );
        let mut oversized = task(tenant, "oversized", ForgeTaskLane::Ordinary, 11);
        oversized.estimates.bytes = 5_000;
        let oversized_id = tasks
            .enqueue(op, &oversized)
            .await
            .expect("oversized enqueue");
        assert!(
            tasks
                .claim_fair(op, owner, limits(2))
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

    /// Proves persisted tenant rotation survives scheduler takeover and large-lane exclusion is cluster-wide.
    ///
    /// # Panics
    /// Panics when PostgreSQL setup or a fencing assertion fails.
    #[tokio::test]
    async fn scheduler_takeover_preserves_cursor_and_large_lane_is_singleton() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new();
        let op = fixture.operator_pool();
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(tenant_b, "forge-second")
            .await
            .expect("seed tenant");
        tasks
            .enqueue(
                op,
                &task(tenant_a, "large-a", ForgeTaskLane::LargeSingleton, 2),
            )
            .await
            .expect("a");
        tasks
            .enqueue(
                op,
                &task(tenant_b, "large-b", ForgeTaskLane::LargeSingleton, 3),
            )
            .await
            .expect("b");
        let leader = Uuid::now_v7();
        let _fence = tasks
            .acquire_scheduler(op, leader, 30)
            .await
            .expect("leader")
            .expect("fence");
        let claim_limits = ForgeClaimLimits {
            lease_seconds: 1,
            ..limits(1)
        };
        let (left, right) = tokio::join!(
            tasks.claim_fair(op, leader, claim_limits),
            tasks.claim_fair(op, leader, claim_limits)
        );
        let claims = [left.expect("left"), right.expect("right")];
        assert_eq!(
            claims.iter().filter(|v| v.is_some()).count(),
            1,
            "large lane admits one cluster-wide"
        );
        let first = claims.into_iter().flatten().next().expect("first");
        let first_attempt = first.attempt_id.expect("large attempt");
        tasks
            .heartbeat(op, first.task_id, first_attempt, leader, 3)
            .await
            .expect("renew task and large lane");
        tokio::time::sleep(std::time::Duration::from_millis(1_200)).await;
        assert!(
            tasks
                .claim_fair(op, leader, claim_limits)
                .await
                .expect("post-renew claim")
                .is_none(),
            "renewed large generation blocks a second large claim beyond the original interval"
        );
        assert!(
            tasks
                .heartbeat(op, first.task_id, Uuid::now_v7(), leader, 3)
                .await
                .is_err()
        );
        assert!(
            tasks
                .heartbeat(op, first.task_id, first_attempt, Uuid::now_v7(), 3)
                .await
                .is_err()
        );
        sqlx::query("UPDATE vala.forge_scheduler_state SET expires_at=statement_timestamp()-interval '1 second'").execute(&admin).await.expect("expire leader");
        sqlx::query("UPDATE vala.forge_large_lane_lease SET expires_at=statement_timestamp()-interval '1 second'").execute(&admin).await.expect("expire large lane");
        sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1").bind(first.task_id).execute(&admin).await.expect("expire claim");
        assert!(
            tasks
                .heartbeat(op, first.task_id, first_attempt, leader, 3)
                .await
                .is_err(),
            "expired task/large generation cannot be revived by heartbeat"
        );
        assert_eq!(tasks.reclaim_expired(op, 10).await.expect("reclaim"), 1);
        let released: Option<Uuid> =
            sqlx::query_scalar("SELECT task_id FROM vala.forge_large_lane_lease WHERE singleton")
                .fetch_one(&admin)
                .await
                .expect("large lease after reclaim");
        assert!(
            released.is_none(),
            "reclaim releases the exact large-lane generation atomically"
        );
        let successor = Uuid::now_v7();
        let _successor_fence = tasks
            .acquire_scheduler(op, successor, 30)
            .await
            .expect("successor")
            .expect("takeover fence");
        let second = tasks
            .claim_fair(op, successor, claim_limits)
            .await
            .expect("successor claim")
            .expect("next tenant");
        assert_ne!(
            first.data_tenant_id, second.data_tenant_id,
            "cursor resumes strictly after prior tenant"
        );
    }

    /// Proves malformed returned rows roll back claim, cursor, and large-lane writes.
    ///
    /// # Panics
    /// Panics when malformed-row rollback is incomplete.
    #[tokio::test]
    async fn malformed_claim_rolls_back_all_coordination_state() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new();
        let op = fixture.operator_pool();
        let tenant = fixture.data_tenant_id();
        let task_id = tasks
            .enqueue(
                op,
                &task(tenant, "malformed-claim", ForgeTaskLane::LargeSingleton, 12),
            )
            .await
            .expect("enqueue");
        sqlx::query("UPDATE vala.forge_tasks SET plan=jsonb_set(plan,'{version}','99'::jsonb) WHERE task_id=$1").bind(task_id).execute(&admin).await.expect("corrupt plan");
        let owner = Uuid::now_v7();
        let _fence = tasks
            .acquire_scheduler(op, owner, 30)
            .await
            .expect("scheduler")
            .expect("fence");
        let scheduler_before: (i64, Option<Uuid>) = sqlx::query_as(
            "SELECT fencing_token,last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
        )
        .fetch_one(&admin)
        .await
        .expect("scheduler before");
        let large_before: LargeLeaseSnapshot = sqlx::query_as("SELECT task_id,attempt_id,owner,fencing_token,expires_at FROM vala.forge_large_lane_lease WHERE singleton").fetch_one(&admin).await.expect("large before");
        assert!(
            tasks.claim_fair(op, owner, limits(1)).await.is_err(),
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
        let large_after: LargeLeaseSnapshot = sqlx::query_as("SELECT task_id,attempt_id,owner,fencing_token,expires_at FROM vala.forge_large_lane_lease WHERE singleton").fetch_one(&admin).await.expect("large after");
        assert_eq!(
            large_after, large_before,
            "large-lane acquisition rolled back"
        );
    }

    /// Proves Prepared replay and both lifecycle audit boundaries are atomic under rollback.
    ///
    /// # Panics
    /// Panics when state/audit atomicity or replay assertions fail.
    #[tokio::test]
    async fn prepared_and_terminal_audit_are_atomic_and_replay_safe() {
        let (fixture, _admin) = setup().await;
        let tasks = ForgeTasks::new();
        let op = fixture.operator_pool();
        let tenant = fixture.data_tenant_id();
        let id = tasks
            .enqueue(op, &task(tenant, "audit", ForgeTaskLane::Ordinary, 4))
            .await
            .expect("enqueue");
        let owner = Uuid::now_v7();
        let _fence = tasks
            .acquire_scheduler(op, owner, 30)
            .await
            .expect("scheduler")
            .expect("fence");
        let (attempt, _) = claim_and_start(
            &tasks,
            op,
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
        assert!(
            tasks
                .retry(op, id, attempt, owner, Utc::now())
                .await
                .is_err()
        );
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
        let tasks = ForgeTasks::new();
        let op = fixture.operator_pool();
        let tenant = fixture.data_tenant_id();
        let original_owner = Uuid::now_v7();
        let task_id = tasks
            .enqueue(
                op,
                &task(tenant, "prepared-takeover", ForgeTaskLane::Ordinary, 73),
            )
            .await
            .expect("enqueue");
        let claimed = tasks
            .claim_fair(op, original_owner, limits(1))
            .await
            .expect("claim")
            .expect("task");
        assert_eq!(claimed.task_id, task_id);
        let attempt = claimed.attempt_id.expect("attempt");
        tasks
            .start(
                op,
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
            tasks.claim_prepared_for_reconciliation(op, successor_a, 30),
            tasks.claim_prepared_for_reconciliation(op, successor_b, 30),
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

    /// Proves bounded watermark/status failure modes, pruning safety, RLS, and grants.
    ///
    /// # Panics
    /// Panics when a database boundary or bounded-read assertion fails.
    #[tokio::test]
    async fn watermark_pruning_identity_and_database_boundaries_fail_closed() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new();
        let op = fixture.operator_pool();
        let tenant = fixture.data_tenant_id();
        assert!(ForgeTaskTableIdentity::new("unsupported", "vala.bifrost", "events").is_err());
        assert!(ForgeTaskTableIdentity::new("wyrd-redux", "unsupported", "events").is_err());
        assert!(ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "../events").is_err());
        let mut malformed = task(tenant, "malformed", ForgeTaskLane::Ordinary, 5);
        malformed.plan.version = 99;
        assert!(tasks.enqueue(op, &malformed).await.is_err());
        let _ready = tasks
            .enqueue(op, &task(tenant, "ready", ForgeTaskLane::Ordinary, 6))
            .await
            .expect("ready");
        let prepared_id = tasks
            .enqueue(op, &task(tenant, "prepared", ForgeTaskLane::Ordinary, 7))
            .await
            .expect("prepared enqueue");
        let terminal_id = tasks
            .enqueue(op, &task(tenant, "terminal", ForgeTaskLane::Ordinary, 8))
            .await
            .expect("terminal enqueue");
        let owner = Uuid::now_v7();
        let _fence = tasks
            .acquire_scheduler(op, owner, 30)
            .await
            .expect("scheduler")
            .expect("fence");
        let first = tasks
            .claim_fair(op, owner, limits(4))
            .await
            .expect("claim")
            .expect("claim");
        let attempt = first.attempt_id.expect("attempt");
        tasks
            .start(
                op,
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
            .enqueue(
                op,
                &task(tenant, "invalid-tenant", ForgeTaskLane::Ordinary, 9),
            )
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
            tasks.claim_fair(op, owner, limits(4)).await.is_err(),
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
        let tasks = ForgeTasks::new();
        let op = fixture.operator_pool();
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
                .upsert_periodic(op, tenant, &identity)
                .await
                .expect("periodic"),
            3
        );
        let owner = Uuid::now_v7();
        let fence = tasks
            .acquire_scheduler(op, owner, 30)
            .await
            .expect("lease")
            .expect("fence");
        let (listed, overflowed) = tasks
            .planning_demands(op, owner, fence, 1)
            .await
            .expect("bounded list");
        assert!(!overflowed);
        assert_eq!(listed[0].generation, 3);
        let captured = listed[0].clone();
        assert_eq!(
            tasks
                .upsert_periodic(op, tenant, &identity)
                .await
                .expect("newer"),
            4
        );
        let exact = task(tenant, "demand", ForgeTaskLane::Ordinary, 42);
        assert!(
            !tasks
                .enqueue_and_acknowledge(
                    op,
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
                .expect("stale CAS")
        );
        let (retryable, _) = tasks
            .planning_demands(op, owner, fence, 1)
            .await
            .expect("retry list");
        assert_eq!(retryable[0].generation, 4);
        let successor = Uuid::now_v7();
        sqlx::query("UPDATE vala.forge_scheduler_state SET expires_at=statement_timestamp()-interval '1 second'").execute(&admin).await.expect("expire leader");
        let successor_fence = tasks
            .acquire_scheduler(op, successor, 30)
            .await
            .expect("takeover")
            .expect("successor fence");
        let stale_terminal = task(tenant, "demand", ForgeTaskLane::LargeSingleton, 44);
        assert!(
            tasks
                .enqueue_and_acknowledge(
                    op,
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
        assert!(
            tasks
                .enqueue_and_acknowledge(
                    op,
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
                .expect("successor ack")
        );
        assert!(
            tasks
                .planning_demands(op, successor, successor_fence, 1)
                .await
                .expect("empty")
                .0
                .is_empty()
        );
        tasks
            .upsert_periodic(op, tenant, &identity)
            .await
            .expect("terminal demand");
        let terminal_demand = tasks
            .planning_demands(op, successor, successor_fence, 1)
            .await
            .expect("terminal list")
            .0
            .remove(0);
        let terminal = task(tenant, "demand", ForgeTaskLane::LargeSingleton, 43);
        assert!(
            tasks
                .enqueue_and_acknowledge(
                    op,
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
                .expect("terminal ack")
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
        assert!(
            !audit_grants
                .iter()
                .any(|value| value.1 == "wyrd_platform_admin" && value.2 == "DELETE")
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

    /// Proves strict-after tenant rotation, wraparound, takeover, RLS, and malformed-row refusal.
    ///
    /// # Panics
    /// Panics when the durable cursor or isolation boundary deviates.
    #[tokio::test]
    async fn planning_demand_cursor_bounds_hot_tenant_across_takeover() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new();
        let op = fixture.operator_pool();
        let hot = fixture.data_tenant_id();
        let cold = DataTenantId::new_v7();
        sqlx::query("INSERT INTO platform.tenants(data_tenant_id,slug,display_name,status) VALUES ($1,$2,$3,'active')").bind(cold.as_uuid()).bind(format!("cold-{cold}")).bind("cold").execute(&admin).await.expect("cold tenant");
        let identity = ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "rotation")
            .expect("identity");
        tasks
            .upsert_periodic(op, hot, &identity)
            .await
            .expect("hot demand");
        let hot_second =
            ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "rotation-hot-second")
                .expect("second hot identity");
        tasks
            .upsert_periodic(op, hot, &hot_second)
            .await
            .expect("second hot demand");
        tasks
            .upsert_periodic(op, cold, &identity)
            .await
            .expect("cold demand");
        let owner = Uuid::now_v7();
        let fence = tasks
            .acquire_scheduler(op, owner, 30)
            .await
            .expect("lease")
            .expect("fence");
        let page = tasks
            .planning_demands(op, owner, fence, 2)
            .await
            .expect("tenant page")
            .0;
        assert_eq!(page.len(), 2);
        assert_ne!(
            page[0].data_tenant_id, page[1].data_tenant_id,
            "one hot tenant cannot fill the bounded page"
        );
        let first = tasks
            .planning_demands(op, owner, fence, 1)
            .await
            .expect("first")
            .0
            .remove(0);
        let retained = tasks
            .planning_demands(op, owner, fence, 1)
            .await
            .expect("retry after interrupted planning")
            .0
            .remove(0);
        assert_eq!(
            retained.data_tenant_id, first.data_tenant_id,
            "planning failure or cancellation without acknowledgement retains demand"
        );
        let (backlog, oldest, overflowed) = tasks
            .planning_status(op, 1)
            .await
            .expect("incomplete status");
        assert_eq!(backlog, 1);
        assert!(oldest.is_some());
        assert!(
            overflowed,
            "two pending demands make a cap-one gauge pass incomplete"
        );
        tasks
            .upsert_periodic(op, first.data_tenant_id, &first.table_ref)
            .await
            .expect("concurrent newer generation");
        assert!(
            !tasks
                .enqueue_and_acknowledge(
                    op,
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
                .expect("CAS mismatch")
        );
        let second = tasks
            .planning_demands(op, owner, fence, 1)
            .await
            .expect("second")
            .0
            .remove(0);
        assert_ne!(
            first.data_tenant_id, second.data_tenant_id,
            "cursor advances on CAS mismatch so the other tenant is selected within bound two"
        );
        sqlx::query("UPDATE vala.forge_scheduler_state SET expires_at=statement_timestamp()-interval '1 second'").execute(&admin).await.expect("expire");
        let successor = Uuid::now_v7();
        let successor_fence = tasks
            .acquire_scheduler(op, successor, 30)
            .await
            .expect("takeover")
            .expect("fence");
        let resumed = tasks
            .planning_demands(op, successor, successor_fence, 1)
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
                .planning_demands(op, successor, successor_fence, 2)
                .await
                .is_err(),
            "malformed persisted demand fails closed"
        );
    }
}
