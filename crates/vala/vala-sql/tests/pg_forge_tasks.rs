mod pg_tests {
    //! Real-Postgres lifecycle, fencing, audit, and isolation proofs for Forge tasks.

    use std::time::Duration as StdDuration;

    use chrono::{Duration, Utc};
    use sqlx::{PgPool, types::Uuid};
    use vala_sql::TenantConn;
    use vala_sql::queries::forge_tasks::{ForgeClaimLimits, ForgeEnqueueBatch, ForgeTasks};
    use vala_sql::row_types::forge_operations::{ForgeClaimTable, ForgeExpirationAuthority};
    use vala_sql::row_types::forge_tasks::{
        ExpiredCleanupCandidateRequest, ExpiredCleanupOutcome, ExpiredCleanupPayload,
        FORGE_TASK_PAYLOAD_VERSION, ForgeClaimStrategy, ForgeCleanupCandidate,
        ForgeCleanupCategory, ForgeCleanupPath, ForgeTaskEstimates, ForgeTaskEvidence,
        ForgeTaskPlan, ForgeTaskRowEvidence, ForgeTaskState, ForgeTaskStrategy,
        ForgeTaskTableIdentity, ForgeTaskTransition, ForgeTaskTransitionOutcome,
        MAINTENANCE_STRATEGIES, NewForgeTask, ORPHAN_CLEANUP_PAYLOAD_VERSION, OrphanCleanupPayload,
        SnapshotWatermark,
    };
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;

    /// Starts one isolated migrated database and returns its administrative pool.
    ///
    /// # Panics
    /// Panics when the repository PostgreSQL fixture cannot start.
    async fn setup() -> (PgFixture, PgPool) {
        let fixture = PgFixture::start().await.expect("fixture");
        let admin = fixture.superuser_pool().await.expect("admin pool");
        (fixture, admin)
    }

    /// Builds one valid, already-eligible enqueue request with caller-selected identity.
    ///
    /// The one-second margin keeps host and container clocks from making an
    /// immediate claim nondeterministically observe a future `ready_at`.
    ///
    /// # Panics
    /// Panics when the fixed test identity is invalid.
    fn task(tenant: DataTenantId, table: &str, hash: u8) -> NewForgeTask {
        NewForgeTask {
            data_tenant_id: tenant,
            table_ref: ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", table)
                .expect("identity"),
            strategy: ForgeTaskStrategy::SmallFiles,
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
            ..task(tenant, table, hash)
        }
    }

    /// Returns permissive positive claim limits for lifecycle tests.
    fn limits(max_active_per_tenant: u32) -> ForgeClaimLimits {
        ForgeClaimLimits {
            max_active_per_tenant,
            lease_seconds: 30,
        }
    }

    /// Rewrite enqueue and claim carry no synthetic execution envelope or lane.
    ///
    /// The superseded Forge path persisted a per-task execution envelope and a
    /// `lane` discriminator, then filtered the fair claim on those synthetic
    /// memory, spill, and scratch predicates. Local compaction admission now
    /// owns every execution decision, so the durable row must describe the work
    /// only — identity, strategy, plan, and observed input files/bytes — and the
    /// claim must order purely on tenant fairness and time. This asserts both
    /// halves against the live schema and the live claim.
    ///
    /// # Panics
    /// Panics when an envelope or lane column survives, or when a claim that is
    /// ordered only by readiness fails to return the oldest eligible task.
    #[tokio::test]
    async fn rewrite_claims_do_not_persist_or_filter_synthetic_execution_envelopes() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let tenant = fixture.data_tenant_id();
        let columns: Vec<String> = sqlx::query_scalar(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema='vala' AND table_name='forge_tasks'",
        )
        .fetch_all(&admin)
        .await
        .expect("forge_tasks columns");
        for banned in [
            "lane",
            "envelope_version",
            "estimated_parallelism",
            "estimated_memory_bytes",
            "estimated_spill_bytes",
            "large_task_ceiling_bytes",
            "decoded_batch_bytes",
            "decoded_input_bytes",
            "sort_working_bytes",
            "sort_merge_reservation_bytes",
            "encoder_buffer_bytes",
            "upload_chunk_bytes",
            "footer_encoded_bytes",
            "footer_decode_workspace_bytes",
            "sort_spill_bytes",
        ] {
            assert!(
                !columns.iter().any(|column| column == banned),
                "vala.forge_tasks must not persist the synthetic execution column {banned}"
            );
        }
        // Two ready rewrite tasks on distinct tables differ only in readiness, so
        // an order that still consulted a synthetic execution predicate could not
        // return them oldest-first.
        let owner = Uuid::now_v7();
        let mut older = task(tenant, "older", 51);
        older.ready_at = Utc::now() - Duration::seconds(120);
        let older_id = tasks.enqueue(&older).await.expect("older task");
        let mut newer = task(tenant, "newer", 52);
        newer.ready_at = Utc::now() - Duration::seconds(1);
        tasks.enqueue(&newer).await.expect("newer task");
        tasks
            .acquire_scheduler(owner, 30)
            .await
            .expect("scheduler")
            .expect("fence");
        assert_eq!(
            tasks
                .claim_fair(owner, limits(2), None)
                .await
                .expect("claim")
                .expect("an eligible rewrite task")
                .task_id,
            older_id,
            "the fair claim orders on tenant fairness and readiness alone"
        );
    }

    /// Retry settlement persists the closed failure class and its bounded delay.
    #[tokio::test]
    async fn failure_taxonomy_backoff_is_durable() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let tenant = fixture.data_tenant_id();
        let owner = Uuid::now_v7();
        let task = task(tenant, "fault-taxonomy", 203);
        let task_id = tasks.enqueue(&task).await.expect("enqueue fault task");
        let defaults: (i32, Option<String>, bool) = sqlx::query_as(
            "SELECT attempt_count,failure_class,next_eligible_at<=statement_timestamp() FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task_id)
        .fetch_one(&admin)
        .await
        .expect("failure taxonomy defaults");
        assert_eq!(defaults, (0, None, true));
        let claim = tasks
            .claim_fair(owner, limits(1), None)
            .await
            .expect("initial claim")
            .expect("fault task");
        let attempt = claim.attempt_id.expect("attempt identity");

        assert_eq!(
            tasks
                .retry_failure(task_id, attempt, owner, "storage_health")
                .await
                .expect("storage-health retry"),
            1
        );
        let persisted: (String, i32, Option<String>, bool) = sqlx::query_as(
            "SELECT state,attempt_count,failure_class,next_eligible_at>statement_timestamp() FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task_id)
        .fetch_one(&admin)
        .await
        .expect("persisted failure taxonomy");
        assert_eq!(persisted.0, "retryable");
        assert_eq!(persisted.1, 1);
        assert_eq!(persisted.2.as_deref(), Some("storage_health"));
        assert!(persisted.3, "backoff remains in the future");
        assert!(
            tasks
                .claim_fair(Uuid::now_v7(), limits(1), None)
                .await
                .expect("claim during backoff")
                .is_none(),
            "the retry backoff defers every owner equally"
        );

        sqlx::query("UPDATE vala.forge_tasks SET ready_at=statement_timestamp(),next_eligible_at=statement_timestamp() WHERE task_id=$1")
            .bind(task_id)
            .execute(&admin)
            .await
            .expect("advance eligibility");
        let resumed = tasks
            .claim_fair(Uuid::now_v7(), limits(1), None)
            .await
            .expect("post-backoff claim")
            .expect("an eligible retryable task is reclaimable");
        assert_eq!(resumed.task_id, task_id);
    }

    /// Capacity refusal preserves retry budget while lease reclaim consumes it without audit.
    #[tokio::test]
    async fn capacity_refusal_and_expired_reclaim_have_distinct_settlement() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let tenant = fixture.data_tenant_id();
        let owner = Uuid::now_v7();
        let capacity = task(tenant, "capacity-refusal", 204);
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
        let audit_before: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_staging")
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
        let audit_after: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_staging")
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
            .enqueue(&task(tenant, "worker-only", 91))
            .await
            .expect("enqueue worker-only task");
        assert!(
            tasks
                .claim_fair(owner, limits(1), Some(&[ForgeTaskStrategy::SnapshotExpiry]))
                .await
                .expect("refused claim")
                .is_none(),
            "a strategy filter that matches nothing admits nothing"
        );
        let unchanged: (Option<Uuid>,) = sqlx::query_as(
            "SELECT last_tenant_id FROM vala.forge_worker_claim_state WHERE singleton",
        )
        .fetch_one(&admin)
        .await
        .expect("refused worker cursor");
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

    /// Superseded cancellation is terminal, audited, and repairable.
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
            .enqueue(&task(tenant, "superseded", 92))
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
                .cancel_superseded(&mut rollback, &claim)
                .await
                .expect("rollback cancellation");
        }
        let rolled_back: (String, i64, i64) = sqlx::query_as(
            "SELECT t.state,(SELECT count(*) FROM vala.forge_planning_demands),(SELECT count(*) FROM vala.audit_staging) FROM vala.forge_tasks t WHERE t.task_id=$1",
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
            .cancel_superseded(&mut commit, &claim)
            .await
            .expect("commit cancellation");
        commit
            .commit()
            .await
            .expect("commit superseded cancellation");
        let terminal: (String, Option<Uuid>, Option<Uuid>, i64) = sqlx::query_as(
            "SELECT t.state,t.attempt_id,t.claimed_by,(SELECT count(*) FROM vala.forge_planning_demands) FROM vala.forge_tasks t WHERE t.task_id=$1",
        )
        .bind(task_id)
        .fetch_one(&admin)
        .await
        .expect("committed cancellation state");
        assert_eq!(terminal, ("cancelled".to_owned(), None, None, 1));
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
            .enqueue(&task(tenant, "race", 1))
            .await
            .expect("enqueue");
        assert_eq!(
            id,
            tasks
                .enqueue(&task(tenant, "race", 1))
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
            .enqueue(&task(tenant_a, "rotate-a", 2))
            .await
            .expect("a");
        tasks
            .enqueue(&task(tenant_b, "rotate-b", 3))
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
            .enqueue(&task(tenant, "same-table", 61))
            .await
            .expect("first generation");
        tasks
            .enqueue(&task(tenant, "same-table", 62))
            .await
            .expect("second generation");
        let independent = tasks
            .enqueue(&task(tenant, "independent", 63))
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
            .enqueue(&task(tenant, "malformed-claim", 12))
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
            .enqueue(&task(tenant, "audit", 4))
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
            prepared_candidate_index: None,
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
                    &wrong_table_evidence
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
                    .prepared(&mut conn, id, attempt, owner, &evidence)
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
        let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_staging")
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
                .prepared(&mut prepared_conn, id, attempt, owner, &evidence)
                .await
                .expect("prepared"),
            ForgeTaskTransitionOutcome::Applied
        );
        assert_eq!(
            tasks
                .prepared(&mut prepared_conn, id, attempt, owner, &evidence)
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
            tasks.terminal(&mut conn, terminal).await.expect("terminal");
        }
        let mut after = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("after rollback");
        let page = tasks.status(&mut after, 10).await.expect("status");
        assert_eq!(page.tasks[0].state, ForgeTaskState::Prepared);
        let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_staging")
            .fetch_one(&mut **after.transaction())
            .await
            .expect("audit count");
        assert_eq!(
            audit_count, 0,
            "a rolled-back terminal transition leaves no lineage and no audit"
        );
        assert!(
            tasks
                .terminal(
                    &mut after,
                    ForgeTaskTransition {
                        expected: ForgeTaskState::Running,
                        ..terminal
                    }
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
                .terminal(&mut commit_terminal, terminal)
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
                .terminal(&mut terminal_replay, terminal)
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
            .enqueue(&task(tenant, "prepared-takeover", 73))
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
            prepared_candidate_index: None,
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
            .prepared(&mut conn, task_id, attempt, original_owner, &evidence)
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
        assert_eq!(
            taken[0].evidence,
            Some(ForgeTaskRowEvidence::Publication(evidence))
        );
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
            .enqueue(&task(tenant, "large-prepared-takeover", 74))
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
                    snapshot_id: 74,
                    timestamp_ms: 74,
                },
            )
            .await
            .expect("start");
        let evidence = ForgeTaskEvidence {
            prepared_candidate_index: None,
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
            .prepared(&mut conn, task_id, attempt, original_owner, &evidence)
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
        assert_eq!(
            taken.evidence,
            Some(ForgeTaskRowEvidence::Publication(evidence))
        );
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
            .enqueue(&task(tenant, "atomic-successor", 91))
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
            prepared_candidate_index: None,
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
            .prepared(&mut prepared, task_id, attempt, owner, &evidence)
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

        let audit_before: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_staging")
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
        let audit_after: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_staging")
            .fetch_one(&admin)
            .await
            .expect("audit after");
        assert_eq!(
            audit_after, audit_before,
            "successor demand evaluates no permission, so it appends no audit"
        );

        let noop_table = ForgeTaskTableIdentity::new(
            "wyrd-redux",
            "vala.bifrost",
            "atomic-noop-acknowledgement",
        )
        .expect("no-op table identity");
        let noop_id = tasks
            .enqueue(&task(tenant, "atomic-noop-acknowledgement", 93))
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
                    prepared_candidate_index: None,
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    committed_snapshot_id: Some(93),
                    committed_metadata_location: Some("metadata/v93.json".to_owned()),
                    committed_metadata_digest: Some(format!("sha256:{}", "a".repeat(64))),
                    cleanup_candidates: Vec::new(),
                    deleted_candidate_count: 0,
                },
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
        let mut malformed = task(tenant, "malformed", 5);
        malformed.plan.version = 99;
        assert!(tasks.enqueue(&malformed).await.is_err());
        let _ready = tasks
            .enqueue(&task(tenant, "ready", 6))
            .await
            .expect("ready");
        let prepared_id = tasks
            .enqueue(&task(tenant, "prepared", 7))
            .await
            .expect("prepared enqueue");
        let terminal_id = tasks
            .enqueue(&task(tenant, "terminal", 8))
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
        sqlx::query("INSERT INTO vala.forge_tasks SELECT $2,data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id+100,plan,$3,estimated_files,estimated_bytes,'running',$4,claimed_by,claim_expires_at,2,20,evidence,ready_at,created_at,updated_at FROM vala.forge_tasks WHERE task_id=$1")
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
            .enqueue(&task(tenant, "invalid-tenant", 9))
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
        let audit_before: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_staging")
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
        let mismatched = task(DataTenantId::new_v7(), "demand", 41);
        assert!(
            tasks
                .enqueue_and_acknowledge(
                    owner,
                    fence,
                    &captured,
                    ForgeEnqueueBatch {
                        executable: std::slice::from_ref(&mismatched)
                    }
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
        let exact = task(tenant, "demand", 42);
        let stale_cursor_before: Option<Uuid> = sqlx::query_scalar(
            "SELECT last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
        )
        .fetch_one(&admin)
        .await
        .expect("stale cursor before");
        let stale_audit_before: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_staging")
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
                        executable: std::slice::from_ref(&exact)
                    }
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
        let stale_audit_after: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_staging")
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
        let stale_terminal = task(tenant, "demand", 44);
        assert!(
            tasks
                .enqueue_and_acknowledge(
                    owner,
                    fence,
                    &retryable[0],
                    ForgeEnqueueBatch {
                        executable: std::slice::from_ref(&stale_terminal)
                    }
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
                        executable: std::slice::from_ref(&exact)
                    }
                )
                .await
                .expect("successor ack")
                .len(),
            1
        );
        assert!(
            tasks
                .enqueue_and_acknowledge(
                    successor,
                    successor_fence,
                    &retryable[0],
                    ForgeEnqueueBatch {
                        executable: std::slice::from_ref(&exact)
                    }
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
        let terminal = task(tenant, "demand", 43);
        assert_eq!(
            tasks
                .enqueue_and_acknowledge(
                    successor,
                    successor_fence,
                    &terminal_demand,
                    ForgeEnqueueBatch {
                        executable: std::slice::from_ref(&terminal)
                    }
                )
                .await
                .expect("terminal ack")
                .len(),
            1
        );
        let terminal_count: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name='demand' AND state='ready'").bind(tenant.as_uuid()).fetch_one(&admin).await.expect("enqueued count");
        assert_eq!(terminal_count, 2);
        let audit_after: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_staging")
            .fetch_one(&admin)
            .await
            .expect("audit after");
        assert_eq!(
            audit_after, audit_before,
            "demand coordination and enqueue emit no audit"
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
        let audit_grants: Vec<(String, String, String)> = sqlx::query_as("SELECT table_name,grantee,privilege_type FROM information_schema.role_table_grants WHERE table_schema='vala' AND table_name IN ('audit_chain_head','audit_staging')").fetch_all(&admin).await.expect("audit grants");
        assert!(
            !audit_grants
                .iter()
                .any(|value| value.1 == "wyrd_platform_admin"),
            "Forge evaluates no permission, so the operator holds no audit authority: {audit_grants:?}"
        );
        assert!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM vala.audit_staging")
                .fetch_one(op.pool())
                .await
                .is_err(),
            "operator cannot directly read tenant audit rows"
        );
        assert!(
            sqlx::query("UPDATE vala.audit_staging SET outcome=outcome")
                .execute(op.pool())
                .await
                .is_err(),
            "operator cannot directly update tenant audit rows"
        );
        assert!(
            sqlx::query("DELETE FROM vala.audit_staging")
                .execute(op.pool())
                .await
                .is_err(),
            "operator cannot directly delete tenant audit rows"
        );
        assert!(
            sqlx::query("INSERT INTO vala.audit_staging (data_tenant_id,seq,prev_hash,entry_hash,request_id,operation,resource,principal_id,principal_kind,permission,outcome) VALUES (wyrd.current_tenant(),1,decode(repeat('00',32),'hex'),decode(repeat('00',32),'hex'),'x','x','x',gen_random_uuid(),'service','x','allowed')")
                .execute(op.pool())
                .await
                .is_err(),
            "operator cannot append tenant audit rows"
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

    /// Proves failures after task, demand, and cursor mutations roll back the
    /// complete fenced planning transaction.
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
            let audit_before: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_staging")
                .fetch_one(&admin)
                .await
                .expect("audit before");
            sqlx::query(sqlx::AssertSqlSafe(create_trigger.to_owned()))
                .execute(&admin)
                .await
                .expect("create trigger");
            let hash = u8::try_from(80 + index).expect("bounded stage");
            let terminal = task(tenant, "step-failure", hash);
            assert!(
                tasks
                    .enqueue_and_acknowledge(
                        owner,
                        fence,
                        &demand,
                        ForgeEnqueueBatch {
                            executable: std::slice::from_ref(&terminal)
                        }
                    )
                    .await
                    .is_err(),
                "enqueue stage {index} must fail"
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
            let audit_after: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_staging")
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

    /// One tenant's repeatedly re-demanded table cannot starve its siblings.
    ///
    /// Roster repair re-requests every table it knows about on each pass, in a
    /// stable order, and each re-request resets `last_requested_at`. The table
    /// refreshed first therefore stays its tenant's oldest demand forever, so a
    /// page that carried only that one demand per tenant would never reach the
    /// tables behind it. The page must reach them within one pass whenever the
    /// bound has room.
    ///
    /// # Panics
    /// Panics when a demanded table is absent from a page with room for it.
    #[tokio::test]
    async fn planning_demand_page_reaches_every_table_of_one_tenant() {
        let (fixture, _admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let tenant = fixture.data_tenant_id();
        let starved = ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "starved")
            .expect("starved identity");
        let noisy = ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "noisy")
            .expect("noisy identity");
        tasks
            .upsert_periodic(tenant, &starved)
            .await
            .expect("starved demand");
        tasks
            .upsert_periodic(tenant, &noisy)
            .await
            .expect("noisy demand");
        let owner = Uuid::now_v7();
        let fence = tasks
            .acquire_scheduler(owner, 30)
            .await
            .expect("lease")
            .expect("fence");
        for _ in 0..3 {
            tasks
                .upsert_periodic(tenant, &noisy)
                .await
                .expect("noisy re-demand");
            tasks
                .upsert_periodic(tenant, &starved)
                .await
                .expect("starved re-demand");
            let page = tasks
                .planning_demands(owner, fence, 8)
                .await
                .expect("tenant page")
                .0;
            assert!(
                page.iter()
                    .any(|demand| demand.table_ref.table == starved.table),
                "a re-demanded sibling must not hold the tenant's only planning slot"
            );
        }
    }

    /// Proves tenant-qualified exclusion before ranking/overflow, live-generation
    /// continuity, strict-after rotation, takeover, RLS, and malformed-row refusal.
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
        assert_eq!(
            tasks
                .acquire_scheduler(owner, 30)
                .await
                .expect("live reacquisition"),
            Some(fence),
            "the same live ownership generation must retain its local cycle"
        );
        let excluded = std::collections::BTreeSet::from([(hot, identity.clone())]);
        let (eligible, overflow) = tasks
            .planning_demands_excluding(owner, fence, 2, &excluded)
            .await
            .expect("cycle-aware tenant page");
        assert!(!overflow, "excluded demand must not affect overflow");
        assert_eq!(eligible.len(), 2);
        assert!(
            eligible
                .iter()
                .any(|d| d.data_tenant_id == hot && d.table_ref == hot_second)
        );
        assert!(
            eligible
                .iter()
                .any(|d| d.data_tenant_id == cold && d.table_ref == identity),
            "exclusion must include the tenant, not merely the logical table"
        );
        assert!(
            tasks
                .planning_demands_excluding(owner, fence, 1, &excluded)
                .await
                .expect("bounded eligible page")
                .1
        );
        let excluded = std::collections::BTreeSet::from([
            (hot, identity.clone()),
            (hot, hot_second.clone()),
            (cold, identity.clone()),
        ]);
        let (empty, overflow) = tasks
            .planning_demands_excluding(owner, fence, 1, &excluded)
            .await
            .expect("exhausted traversal with durable demand remaining");
        assert!(empty.is_empty() && !overflow);
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
        let status = tasks
            .planning_demand_status()
            .await
            .expect("authoritative demand status");
        assert_eq!(
            status.demands, 3,
            "the demand snapshot counts both hot tables and the cold table, not one bounded page"
        );
        assert!(status.oldest_requested_at.is_some());
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
                    ForgeEnqueueBatch { executable: &[] }
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
                    ForgeEnqueueBatch { executable: &[] }
                )
                .await
                .expect("acknowledge current generation")
                .len(),
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
                .enqueue(&task(tenant, &format!("compaction_{index}"), index))
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

    /// Builds the locked Scribe-promotion task parameters for one ordered file set.
    ///
    /// The shape is the durable contract: `kind` is `scribe_promotion`, the
    /// target branch is bound, and the ordered `file_list` IDs, logical paths,
    /// and normalized checksums accompany the digest computed over exactly
    /// those tuples.
    fn scribe_promotion_parameters(
        branch: &str,
        files: &[wyrd_spec::vala::api::ForgePromotedFile],
    ) -> serde_json::Value {
        serde_json::json!({
            "kind": "scribe_promotion",
            "branch": branch,
            "file_ids": files
                .iter()
                .map(|file| file.file_id().hyphenated().to_string())
                .collect::<Vec<_>>(),
            "paths": files
                .iter()
                .map(|file| file.path().as_str().to_owned())
                .collect::<Vec<_>>(),
            "checksums": files
                .iter()
                .map(|file| file.checksum().to_owned())
                .collect::<Vec<_>>(),
            "promoted_file_set_digest":
                wyrd_spec::vala::api::ForgePromotedFileSetDigest::compute(files).as_str()})
    }

    /// A `scribe_promotion` task survives enqueue, fair claim, and projection
    /// with its exact strategy and parameters, while an unrecognized persisted
    /// strategy tag quarantines at the claim boundary instead of decaying into
    /// promotion.
    ///
    /// The two halves are one obligation: the durable strategy set must admit
    /// exactly `scribe_promotion` and nothing else, so the same test proves the
    /// canonical constraint accepts the new value and that a forward-
    /// incompatible or corrupted tag is retained verbatim as
    /// [`ForgeClaimStrategy::Unknown`]. Promotion is ordinary publication
    /// work, so it is deliberately excluded from the maintenance-reserved slot.
    ///
    /// # Panics
    /// Panics when PostgreSQL setup, enqueue, claim, or the exact strategy,
    /// and parameter assertions fail.
    #[tokio::test]
    async fn scribe_promotion_task_round_trips_and_unknown_strategy_quarantines() {
        let (fixture, admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant = fixture.data_tenant_id();

        let promoted = [
            wyrd_spec::vala::api::ForgePromotedFile::new(
                Uuid::from_u128(1),
                wyrd_spec::vala::api::StoragePath::new("data/hot-a.parquet").expect("path"),
                "AA11",
            )
            .expect("promoted file"),
            wyrd_spec::vala::api::ForgePromotedFile::new(
                Uuid::from_u128(2),
                wyrd_spec::vala::api::StoragePath::new("data/hot-b.parquet").expect("path"),
                "bb22",
            )
            .expect("promoted file"),
        ];
        let parameters = scribe_promotion_parameters("main", &promoted);
        let promotion = NewForgeTask {
            strategy: ForgeTaskStrategy::ScribePromotion,
            plan: ForgeTaskPlan {
                version: FORGE_TASK_PAYLOAD_VERSION,
                inputs: vec![
                    "data/hot-a.parquet".to_owned(),
                    "data/hot-b.parquet".to_owned(),
                ],
                parameters: parameters.clone(),
            },
            ..task(tenant, "promotion_table", 7)
        };

        assert_eq!(
            ForgeTaskStrategy::ScribePromotion.as_str(),
            "scribe_promotion"
        );
        assert!(
            !ForgeTaskStrategy::ScribePromotion.is_maintenance(),
            "promotion is publication work, not maintenance-family cleanup"
        );
        assert!(
            !MAINTENANCE_STRATEGIES.contains(&ForgeTaskStrategy::ScribePromotion),
            "the reserved maintenance slot must not claim promotion"
        );

        let promotion_task_id = tasks.enqueue(&promotion).await.expect("enqueue promotion");
        let stored: String =
            sqlx::query_scalar("SELECT strategy FROM vala.forge_tasks WHERE task_id=$1")
                .bind(promotion_task_id)
                .fetch_one(&admin)
                .await
                .expect("stored strategy");
        assert_eq!(stored, "scribe_promotion");

        assert!(
            tasks
                .claim_fair(Uuid::now_v7(), limits(8), Some(MAINTENANCE_STRATEGIES))
                .await
                .expect("maintenance-filtered claim")
                .is_none(),
            "promotion must not be claimable through the maintenance filter"
        );

        let claim = tasks
            .claim_fair(Uuid::now_v7(), limits(8), None)
            .await
            .expect("unfiltered claim")
            .expect("promotion task is claimable");
        assert_eq!(claim.task_id, promotion_task_id);
        assert_eq!(
            claim.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::ScribePromotion)
        );
        assert_eq!(claim.plan.parameters, parameters);
        assert_eq!(claim.plan.parameters["kind"], "scribe_promotion");

        // A raw tag no worker version recognizes can only reach the claim
        // boundary through corruption or a forward-incompatible writer, so it
        // is arranged with the migrator role after dropping the canonical
        // constraint that ordinary writers cannot bypass.
        let quarantined = tasks
            .enqueue(&NewForgeTask {
                ..task(tenant, "quarantine_table", 9)
            })
            .await
            .expect("enqueue quarantine candidate");
        sqlx::query("ALTER TABLE vala.forge_tasks DROP CONSTRAINT forge_tasks_strategy_check")
            .execute(&admin)
            .await
            .expect("drop strategy check");
        sqlx::query("UPDATE vala.forge_tasks SET strategy='promotion_v2' WHERE task_id=$1")
            .bind(quarantined)
            .execute(&admin)
            .await
            .expect("persist unknown strategy");

        let unknown = tasks
            .claim_fair(Uuid::now_v7(), limits(8), None)
            .await
            .expect("claim of unknown strategy")
            .expect("unknown strategy still claims for quarantine");
        assert_eq!(unknown.task_id, quarantined);
        assert_eq!(
            unknown.strategy,
            ForgeClaimStrategy::Unknown("promotion_v2".to_owned()),
            "an unrecognized tag is retained verbatim and never mapped to a known strategy"
        );
        assert_ne!(
            unknown.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::ScribePromotion),
            "an unrecognized tag must never fall back to promotion"
        );
    }

    /// Fixed table identity every expired-cleanup proof binds to.
    fn cleanup_table() -> ForgeClaimTable {
        ForgeClaimTable {
            table_uid: [9_u8; 16],
            catalog_name: "wyrd-redux".to_owned(),
            namespace_name: "vala.bifrost".to_owned(),
            table_name: "cleanup".to_owned(),
            table_uuid: Uuid::now_v7(),
        }
    }

    /// Builds one table-bound cleanup candidate for the cleanup fixture table.
    ///
    /// # Panics
    /// Panics when the fixed identity or path is invalid.
    fn cleanup_candidate(name: &str) -> ForgeCleanupCandidate {
        ForgeCleanupCandidate {
            category: ForgeCleanupCategory::Data,
            table: ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "cleanup")
                .expect("identity"),
            path: ForgeCleanupPath::new(format!("cleanup/data/{name}.parquet")).expect("path"),
        }
    }

    /// Seeds the registered table, its maintenance-authority row, and one live
    /// lease so fenced cleanup transitions have every durable root they check.
    ///
    /// # Panics
    /// Panics when any seeding statement fails.
    async fn seed_cleanup_roots(
        superuser: &PgPool,
        tenant: DataTenantId,
        authority: &ForgeExpirationAuthority,
        table: &ForgeClaimTable,
    ) {
        sqlx::query("INSERT INTO vala.bifrost_tables (data_tenant_id,table_uid,fqn,fingerprint,physical_layout) VALUES ($1,$2,'vala.bifrost.cleanup',decode(repeat('00',32),'hex'),'{}'::jsonb)")
            .bind(tenant.as_uuid())
            .bind(table.table_uid.as_slice())
            .execute(superuser)
            .await
            .expect("seed bifrost table");
        sqlx::query("INSERT INTO vala.bifrost_table_maintenance_authority (data_tenant_id,catalog_name,namespace_name,table_name,table_uid) VALUES ($1,$2,$3,$4,$5)")
            .bind(tenant.as_uuid())
            .bind(&table.catalog_name)
            .bind(&table.namespace_name)
            .bind(&table.table_name)
            .bind(table.table_uid.as_slice())
            .execute(superuser)
            .await
            .expect("seed maintenance authority");
        sqlx::query("INSERT INTO vala.maintenance_leases (lease_key,owner,fencing_token,expires_at,heartbeat_at) VALUES ($1,$2,$3,now()+interval '10 minutes',now())")
            .bind(&authority.lease_key)
            .bind(authority.worker_id)
            .bind(authority.lease_fencing_token)
            .execute(superuser)
            .await
            .expect("seed lease");
    }

    /// Inserts one old succeeded snapshot-expiration source carrying candidates.
    ///
    /// # Panics
    /// Panics when the insert fails.
    async fn seed_expiration_source(
        superuser: &PgPool,
        tenant: DataTenantId,
        table: &ForgeClaimTable,
        candidates: &[ForgeCleanupCandidate],
    ) -> Uuid {
        seed_expiration_evidence(
            superuser,
            tenant,
            table,
            &handoff_evidence(candidates.to_vec()),
        )
        .await
    }

    /// Builds the canonical well-formed handoff evidence one expiration leaves.
    ///
    /// Callers mutate the returned value to express exactly one malformed axis,
    /// which keeps every refusal assertion attributable to that one field.
    fn handoff_evidence(candidates: Vec<ForgeCleanupCandidate>) -> ForgeTaskEvidence {
        ForgeTaskEvidence {
            prepared_candidate_index: None,
            version: FORGE_TASK_PAYLOAD_VERSION,
            committed_snapshot_id: Some(4242),
            committed_metadata_location: Some("cleanup/metadata/00042-committed.json".to_owned()),
            committed_metadata_digest: Some("b".repeat(64)),
            cleanup_candidates: candidates,
            deleted_candidate_count: 0,
        }
    }

    /// Inserts one succeeded snapshot-expiration source carrying exact evidence.
    ///
    /// The evidence is written as raw JSONB without passing a validator, which
    /// is what makes a malformed durable row reachable at all: the refusal under
    /// test belongs to the reader, not to the writer that never ran.
    ///
    /// # Panics
    /// Panics when the insert fails.
    async fn seed_expiration_evidence(
        superuser: &PgPool,
        tenant: DataTenantId,
        table: &ForgeClaimTable,
        evidence: &ForgeTaskEvidence,
    ) -> Uuid {
        let task_id = Uuid::now_v7();
        sqlx::query("INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,state,evidence,ready_at,updated_at) VALUES ($1,$2,$3,$4,$5,'snapshot_expiry',41,'{\"version\":1,\"inputs\":[\"m.avro\"],\"parameters\":{}}'::jsonb,decode(repeat('11',32),'hex'),1,1,'succeeded',$6::jsonb,now()-interval '2 days',now()-interval '2 days')")
            .bind(task_id)
            .bind(tenant.as_uuid())
            .bind(&table.catalog_name)
            .bind(&table.namespace_name)
            .bind(&table.table_name)
            .bind(vala_sql::row_types::forge_tasks::evidence_to_value(evidence).to_string())
            .execute(superuser)
            .await
            .expect("seed expiration source");
        task_id
    }

    /// Reads one Forge task's persisted evidence, if any.
    ///
    /// # Panics
    /// Panics when the read or decode fails.
    async fn evidence_of(superuser: &PgPool, task_id: Uuid) -> Option<ForgeTaskEvidence> {
        let raw: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT evidence FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(superuser)
                .await
                .expect("evidence read");
        raw.map(|value| {
            vala_sql::row_types::forge_tasks::evidence_from_json(value).expect("decode")
        })
    }

    /// Asserts both cleanup admission paths refuse one malformed durable source.
    ///
    /// The bounded read and the locked enqueue are separate entry points into
    /// the same handoff, so a source that only one of them rejects still reaches
    /// durable cleanup state. Each call seeds exactly one malformed source,
    /// drives both paths, and proves the enqueue transaction left no cleanup
    /// row, no audit, and an untouched demand behind.
    ///
    /// # Panics
    ///
    /// Panics when a diagnostic read fails or either path accepts the source.
    #[expect(
        clippy::too_many_arguments,
        reason = "the assertion needs the whole durable admission context and owns no state of its own"
    )]
    async fn assert_malformed_handoff_source_is_refused(
        fixture: &PgFixture,
        admin: &PgPool,
        tasks: &ForgeTasks,
        tenant: DataTenantId,
        table: &ForgeClaimTable,
        identity: &ForgeTaskTableIdentity,
        owner: Uuid,
        fence: i64,
        evidence: &ForgeTaskEvidence,
        axis: &str,
    ) {
        let source = seed_expiration_evidence(admin, tenant, table, evidence).await;
        assert!(
            tasks
                .unconsumed_expiration_handoff(tenant, identity)
                .await
                .is_err(),
            "the bounded handoff read accepted a source with {axis}"
        );

        let mut hint = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("hint conn");
        tasks
            .upsert_hint(&mut hint, tenant, identity)
            .await
            .expect("demand");
        hint.commit().await.expect("commit hint");
        let (demands, _) = tasks
            .planning_demands(owner, fence, 4)
            .await
            .expect("demands");
        let demand = demands
            .into_iter()
            .find(|value| value.table_ref == *identity)
            .expect("malformed source still raises a cleanup demand");

        let mut cleanup = task(tenant, "cleanup", 9);
        cleanup.strategy = ForgeTaskStrategy::ExpiredCleanup;
        cleanup.base_snapshot_id = evidence.committed_snapshot_id.expect("committed snapshot");
        cleanup.plan = ForgeTaskPlan {
            version: FORGE_TASK_PAYLOAD_VERSION,
            inputs: Vec::new(),
            parameters: ExpiredCleanupPayload::from_handoff(source, evidence)
                .expect("the malformed axis is not the committed identity")
                .to_value(),
        };
        cleanup.estimates.files = 2;

        let cleanup_rows = || async {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM vala.forge_tasks WHERE strategy='expired_cleanup'",
            )
            .fetch_one(admin)
            .await
            .expect("cleanup row count")
        };
        let audit_rows = || async {
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM vala.audit_staging")
                .fetch_one(admin)
                .await
                .expect("audit row count")
        };
        let before_cleanup = cleanup_rows().await;
        let before_audits = audit_rows().await;
        assert!(
            tasks
                .enqueue_and_acknowledge(
                    owner,
                    fence,
                    &demand,
                    ForgeEnqueueBatch {
                        executable: std::slice::from_ref(&cleanup)
                    }
                )
                .await
                .is_err(),
            "the locked enqueue admission accepted a source with {axis}"
        );
        assert_eq!(
            cleanup_rows().await,
            before_cleanup,
            "a refused {axis} source inserts no cleanup task"
        );
        assert_eq!(
            audit_rows().await,
            before_audits,
            "a refused {axis} source appends no enqueue audit"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT generation FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND catalog_name=$2 AND namespace_name=$3 AND table_name=$4")
                .bind(tenant.as_uuid())
                .bind(&identity.catalog)
                .bind(&identity.namespace)
                .bind(&identity.table)
                .fetch_one(admin)
                .await
                .expect("demand survives"),
            demand.generation,
            "a refused {axis} source leaves the demand unacknowledged"
        );

        sqlx::query("DELETE FROM vala.forge_tasks WHERE task_id=$1")
            .bind(source)
            .execute(admin)
            .await
            .expect("drop the malformed source");
        sqlx::query("DELETE FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND catalog_name=$2 AND namespace_name=$3 AND table_name=$4")
            .bind(tenant.as_uuid())
            .bind(&identity.catalog)
            .bind(&identity.namespace)
            .bind(&identity.table)
            .execute(admin)
            .await
            .expect("drop the demand this subcase raised");
    }

    #[tokio::test]
    async fn expired_cleanup_handoff_and_candidate_lifecycle_are_exact_atomic_and_audited() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let tenant = fixture.data_tenant_id();
        let table = cleanup_table();
        let identity =
            ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "cleanup").expect("identity");
        let candidates = vec![cleanup_candidate("a"), cleanup_candidate("b")];
        let source = seed_expiration_source(&admin, tenant, &table, &candidates).await;

        // The generic enqueue path can never create a cleanup task.
        let mut generic = task(tenant, "cleanup", 3);
        generic.strategy = ForgeTaskStrategy::ExpiredCleanup;
        assert!(
            tasks.enqueue(&generic).await.is_err(),
            "generic enqueue refuses expired cleanup"
        );

        // The source is retained by pruning until a cleanup plan references it.
        let mut prune = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("prune conn");
        assert_eq!(
            tasks
                .prune_terminal(&mut prune, Utc::now(), 10)
                .await
                .expect("prune before enqueue"),
            0
        );
        prune.commit().await.expect("commit prune");

        // The bounded handoff read names exactly that source and its candidates.
        let payload = tasks
            .unconsumed_expiration_handoff(tenant, &identity)
            .await
            .expect("handoff read")
            .expect("one unconsumed handoff");
        assert_eq!(payload.source_task_id, source);
        assert_eq!(payload.committed_snapshot_id, 4242);
        assert_eq!(payload.cleanup_candidates, candidates);
        let encoded = payload.to_value();
        let object = encoded.as_object().expect("closed object");
        assert_eq!(
            object.len(),
            7,
            "the payload is a closed seven-field object"
        );
        assert_eq!(object["kind"], "expired_cleanup");
        assert_eq!(object["version"], 1);

        // Unknown fields and versions fail closed.
        let mut unknown = object.clone();
        unknown.insert("extra".to_owned(), serde_json::json!(1));
        assert!(
            ExpiredCleanupPayload::from_value(&serde_json::Value::Object(unknown), false).is_err()
        );
        let mut bad_version = object.clone();
        bad_version.insert("version".to_owned(), serde_json::json!(2));
        assert!(
            ExpiredCleanupPayload::from_value(&serde_json::Value::Object(bad_version), false)
                .is_err()
        );

        // The cleanup task is inserted and the demand acknowledged in one pass.
        let mut hint = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("hint conn");
        tasks
            .upsert_hint(&mut hint, tenant, &identity)
            .await
            .expect("demand");
        hint.commit().await.expect("commit hint");
        let owner = Uuid::now_v7();
        let fence = tasks
            .acquire_scheduler(owner, 30)
            .await
            .expect("scheduler lease")
            .expect("fence");
        let (demands, _) = tasks
            .planning_demands(owner, fence, 4)
            .await
            .expect("demands");
        let demand = demands
            .into_iter()
            .find(|value| value.table_ref == identity)
            .expect("cleanup demand");
        let mut cleanup = task(tenant, "cleanup", 5);
        cleanup.strategy = ForgeTaskStrategy::ExpiredCleanup;
        cleanup.base_snapshot_id = payload.committed_snapshot_id;
        cleanup.plan = ForgeTaskPlan {
            version: FORGE_TASK_PAYLOAD_VERSION,
            inputs: Vec::new(),
            parameters: payload.to_value(),
        };
        cleanup.estimates.files = 2;

        // A copy that changed a candidate is refused against the locked source.
        let mut divergent = cleanup.clone();
        let mut shifted = payload.cleanup_candidates.clone();
        shifted.reverse();
        let mut divergent_payload = payload.clone();
        divergent_payload.cleanup_candidates = shifted;
        divergent.plan.parameters = divergent_payload.to_value();
        assert!(
            tasks
                .enqueue_and_acknowledge(
                    owner,
                    fence,
                    &demand,
                    ForgeEnqueueBatch {
                        executable: std::slice::from_ref(&divergent)
                    }
                )
                .await
                .is_err(),
            "a candidate vector that disagrees with its source is refused"
        );

        assert_eq!(
            tasks
                .enqueue_and_acknowledge(
                    owner,
                    fence,
                    &demand,
                    ForgeEnqueueBatch {
                        executable: std::slice::from_ref(&cleanup)
                    }
                )
                .await
                .expect("enqueue cleanup")
                .len(),
            1
        );
        let cleanup_id: Uuid = sqlx::query_scalar(
            "SELECT task_id FROM vala.forge_tasks WHERE strategy='expired_cleanup'",
        )
        .fetch_one(&admin)
        .await
        .expect("cleanup task id");

        // Once a cleanup plan references it, the source prunes normally.
        let mut prune_after = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("prune after");
        assert_eq!(
            tasks
                .prune_terminal(&mut prune_after, Utc::now(), 10)
                .await
                .expect("prune after enqueue"),
            1
        );
        prune_after.commit().await.expect("commit prune after");
        assert_eq!(
            tasks
                .unconsumed_expiration_handoff(tenant, &identity)
                .await
                .expect("no handoff remains"),
            None
        );

        // A malformed durable source is refused synchronously by both the
        // bounded read and the locked enqueue, before any cleanup row exists.
        let unknown_version = ForgeTaskEvidence {
            version: FORGE_TASK_PAYLOAD_VERSION + 1,
            ..handoff_evidence(candidates.clone())
        };
        let invalid_cursor = ForgeTaskEvidence {
            deleted_candidate_count: 5,
            ..handoff_evidence(candidates.clone())
        };
        let prepared_cursor = ForgeTaskEvidence {
            prepared_candidate_index: Some(4),
            ..handoff_evidence(candidates.clone())
        };
        let sibling = ForgeCleanupCandidate {
            category: ForgeCleanupCategory::Data,
            table: ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "sibling")
                .expect("sibling identity"),
            path: ForgeCleanupPath::new("sibling/data/a.parquet").expect("sibling path"),
        };
        let foreign_table = handoff_evidence(vec![sibling]);
        for (evidence, axis) in [
            (&unknown_version, "an unknown evidence version"),
            (&invalid_cursor, "an invalid deletion cursor"),
            (&prepared_cursor, "an invalid prepared cursor"),
            (&foreign_table, "a candidate bound to another table"),
        ] {
            assert_malformed_handoff_source_is_refused(
                &fixture, &admin, &tasks, tenant, &table, &identity, owner, fence, evidence, axis,
            )
            .await;
        }

        // Preparation and settlement are source-independent from here on.
        let authority = ForgeExpirationAuthority {
            task_id: cleanup_id,
            attempt_id: Uuid::now_v7(),
            worker_id: Uuid::now_v7(),
            lease_key: "forge:table:cleanup".to_owned(),
            lease_fencing_token: 7,
        };
        seed_cleanup_roots(&admin, tenant, &authority, &table).await;
        sqlx::query("UPDATE vala.forge_tasks SET state='running',attempt_id=$2,claimed_by=$3,claim_expires_at=now()+interval '10 minutes',watermark_snapshot_id=4242,watermark_timestamp_ms=1 WHERE task_id=$1")
            .bind(cleanup_id)
            .bind(authority.attempt_id)
            .bind(authority.worker_id)
            .execute(&admin)
            .await
            .expect("claim cleanup task");

        let index = 0_u32;
        assert_eq!(
            tasks
                .prepare_expired_cleanup_candidate(
                    tenant,
                    ExpiredCleanupCandidateRequest {
                        authority: &authority,
                        table: &table,
                        index,
                        candidate: &candidates[0]
                    },
                )
                .await
                .expect("prepare candidate zero"),
            ForgeTaskTransitionOutcome::Applied
        );
        let prepared = evidence_of(&admin, cleanup_id).await.expect("evidence");
        assert_eq!(prepared.prepared_candidate_index, Some(0));
        assert_eq!(prepared.deleted_candidate_count, 0);
        assert_eq!(prepared.cleanup_candidates, candidates);

        // The exact already-prepared tuple replays read-only.
        assert_eq!(
            tasks
                .prepare_expired_cleanup_candidate(
                    tenant,
                    ExpiredCleanupCandidateRequest {
                        authority: &authority,
                        table: &table,
                        index: 0,
                        candidate: &candidates[0]
                    },
                )
                .await
                .expect("replay preparation"),
            ForgeTaskTransitionOutcome::AlreadyApplied
        );

        // A stale owner can neither prepare nor settle.
        let stale = ForgeExpirationAuthority {
            worker_id: Uuid::now_v7(),
            ..authority.clone()
        };
        assert!(
            tasks
                .settle_expired_cleanup_candidate(
                    tenant,
                    ExpiredCleanupCandidateRequest {
                        authority: &stale,
                        table: &table,
                        index: 0,
                        candidate: &candidates[0]
                    },
                    ExpiredCleanupOutcome::Deleted,
                )
                .await
                .is_err(),
            "a stale owner cannot settle"
        );

        // Confirmed deletion is the only thing that advances candidate zero.
        tasks
            .settle_expired_cleanup_candidate(
                tenant,
                ExpiredCleanupCandidateRequest {
                    authority: &authority,
                    table: &table,
                    index: 0,
                    candidate: &candidates[0],
                },
                ExpiredCleanupOutcome::Deleted,
            )
            .await
            .expect("settle candidate zero");
        let advanced = evidence_of(&admin, cleanup_id).await.expect("evidence");
        assert_eq!(advanced.deleted_candidate_count, 1);
        assert_eq!(advanced.prepared_candidate_index, None);

        // Refusal and uncertainty audit without moving the frontier.
        tasks
            .prepare_expired_cleanup_candidate(
                tenant,
                ExpiredCleanupCandidateRequest {
                    authority: &authority,
                    table: &table,
                    index: 1,
                    candidate: &candidates[1],
                },
            )
            .await
            .expect("prepare candidate one");
        for outcome in [
            ExpiredCleanupOutcome::Refused,
            ExpiredCleanupOutcome::Uncertain,
        ] {
            tasks
                .settle_expired_cleanup_candidate(
                    tenant,
                    ExpiredCleanupCandidateRequest {
                        authority: &authority,
                        table: &table,
                        index: 1,
                        candidate: &candidates[1],
                    },
                    outcome,
                )
                .await
                .expect("record a non-advancing outcome");
            let held = evidence_of(&admin, cleanup_id).await.expect("evidence");
            assert_eq!(held.deleted_candidate_count, 1);
            assert_eq!(held.prepared_candidate_index, Some(1));
            assert_eq!(held.cleanup_candidates, candidates);
        }

        // Proven absence advances to the terminal frontier.
        tasks
            .settle_expired_cleanup_candidate(
                tenant,
                ExpiredCleanupCandidateRequest {
                    authority: &authority,
                    table: &table,
                    index: 1,
                    candidate: &candidates[1],
                },
                ExpiredCleanupOutcome::Missing,
            )
            .await
            .expect("settle candidate one as missing");
        let drained = evidence_of(&admin, cleanup_id).await.expect("evidence");
        assert_eq!(drained.deleted_candidate_count, 2);
        assert_eq!(drained.prepared_candidate_index, None);

        // Success is available only at that empty prepared frontier.
        let mut terminal_conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("terminal conn");
        tasks
            .terminal(
                &mut terminal_conn,
                ForgeTaskTransition {
                    task_id: cleanup_id,
                    attempt_id: authority.attempt_id,
                    owner: authority.worker_id,
                    expected: ForgeTaskState::Prepared,
                    next: ForgeTaskState::Succeeded,
                },
            )
            .await
            .expect("terminal success");
        terminal_conn.commit().await.expect("commit terminal");
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(cleanup_id)
                .fetch_one(&admin)
                .await
                .expect("final state"),
            "succeeded"
        );
    }

    /// Builds the canonical closed orphan-cleanup plan for one scan prefix.
    fn orphan_plan(prefix: &str, age_cutoff_ms: i64) -> ForgeTaskPlan {
        ForgeTaskPlan {
            version: FORGE_TASK_PAYLOAD_VERSION,
            inputs: vec![prefix.to_owned()],
            parameters: OrphanCleanupPayload {
                version: ORPHAN_CLEANUP_PAYLOAD_VERSION,
                age_cutoff_ms,
            }
            .to_value(),
        }
    }

    /// Builds one enqueueable periodic orphan-cleanup task for `table`.
    ///
    /// # Panics
    /// Panics when the fixed identity is invalid.
    fn orphan_task(
        tenant: DataTenantId,
        table: &str,
        prefix: &str,
        age_cutoff_ms: i64,
    ) -> NewForgeTask {
        NewForgeTask {
            strategy: ForgeTaskStrategy::OrphanCleanup,
            plan: orphan_plan(prefix, age_cutoff_ms),
            ..task(tenant, table, 7)
        }
    }

    /// Reads one task's raw persisted evidence column.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn raw_evidence(superuser: &PgPool, task_id: Uuid) -> Option<serde_json::Value> {
        sqlx::query_scalar("SELECT evidence FROM vala.forge_tasks WHERE task_id=$1")
            .bind(task_id)
            .fetch_one(superuser)
            .await
            .expect("evidence read")
    }

    /// Reads one task's durable state and attempt budget together.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn state_and_attempts(superuser: &PgPool, task_id: Uuid) -> (String, i32) {
        sqlx::query_as("SELECT state,attempt_count FROM vala.forge_tasks WHERE task_id=$1")
            .bind(task_id)
            .fetch_one(superuser)
            .await
            .expect("state read")
    }

    /// The orphan-cleanup plan, cursor, retry, and completion contracts are closed.
    ///
    /// One periodic orphan-cleanup task is fully described by its tenant, its
    /// table, one immutable scan prefix, and one immutable age cutoff. This
    /// pins that whole contract against real PostgreSQL: the exact plan and
    /// cursor shapes round-trip, no Reset row and no source-operation identity
    /// is required or accepted anywhere in it, and every widened, malformed, or
    /// out-of-prefix value is refused before it can become durable state.
    ///
    /// The lifecycle half is what makes a bounded scan resumable. A cursor
    /// checkpoint survives the `Running -> Retryable` release and the later
    /// claim with the same task identity and an unchanged failure budget, so a
    /// partial scan is ordinary progress rather than a failed attempt. Work
    /// whose ownership is ambiguous cannot advance the cursor at all. Exhaustion
    /// clears the cursor and marks the task succeeded in the same transaction as
    /// its one success audit, proven by forcing that audit append to fail and
    /// observing the transition roll back with it.
    ///
    /// # Panics
    /// Panics when PostgreSQL setup or any closure assertion fails.
    #[tokio::test]
    async fn orphan_cleanup_plan_cursor_retry_and_completion_are_closed() {
        let (fixture, admin) = setup().await;
        let op = fixture.operator_pool();
        let tasks = ForgeTasks::new(op.clone());
        let tenant = fixture.data_tenant_id();
        let prefix = format!("tenants/{tenant}/bifrost/orphan/data/forge/v1");
        let cutoff_ms = 1_700_000_000_000_i64;

        // The closed plan shape: exactly one normalized prefix, and parameters
        // that name only the payload kind, its version, and the immutable
        // cutoff. Every other spelling is refused before enqueue can persist it.
        for (broken, why) in [
            (
                ForgeTaskPlan {
                    inputs: Vec::new(),
                    ..orphan_plan(&prefix, cutoff_ms)
                },
                "a scan with no prefix names no work",
            ),
            (
                ForgeTaskPlan {
                    inputs: vec![prefix.clone(), format!("{prefix}/extra")],
                    ..orphan_plan(&prefix, cutoff_ms)
                },
                "a widened input set would give the task two scan identities",
            ),
            (
                ForgeTaskPlan {
                    inputs: vec![format!("/{prefix}")],
                    ..orphan_plan(&prefix, cutoff_ms)
                },
                "an absolute key is not a normalized object key",
            ),
            (
                ForgeTaskPlan {
                    inputs: vec![format!("{prefix}/../sibling")],
                    ..orphan_plan(&prefix, cutoff_ms)
                },
                "a traversal segment escapes the scan prefix",
            ),
            (
                ForgeTaskPlan {
                    parameters: serde_json::json!({
                        "version": 1,
                        "kind": "orphan_cleanup",
                        "age_cutoff_ms": cutoff_ms,
                        "source_operation_id": Uuid::now_v7().to_string()}),
                    ..orphan_plan(&prefix, cutoff_ms)
                },
                "a source-operation identity is not part of this contract",
            ),
            (
                ForgeTaskPlan {
                    parameters: serde_json::json!({"version": 2, "kind": "orphan_cleanup", "age_cutoff_ms": cutoff_ms}),
                    ..orphan_plan(&prefix, cutoff_ms)
                },
                "an unknown payload version fails closed",
            ),
            (
                ForgeTaskPlan {
                    parameters: serde_json::json!({"version": 1, "kind": "expired_cleanup", "age_cutoff_ms": cutoff_ms}),
                    ..orphan_plan(&prefix, cutoff_ms)
                },
                "another strategy's payload kind is not this strategy's payload",
            ),
            (
                ForgeTaskPlan {
                    parameters: serde_json::json!({"version": 1, "kind": "orphan_cleanup", "age_cutoff_ms": -1}),
                    ..orphan_plan(&prefix, cutoff_ms)
                },
                "a negative cutoff is not a wall-clock instant",
            ),
        ] {
            assert!(
                tasks
                    .enqueue(&NewForgeTask {
                        plan: broken,
                        ..orphan_task(tenant, "orphan", &prefix, cutoff_ms)
                    })
                    .await
                    .is_err(),
                "{why}"
            );
        }

        let id = tasks
            .enqueue(&orphan_task(tenant, "orphan", &prefix, cutoff_ms))
            .await
            .expect("orphan cleanup enqueue");
        let owner = Uuid::now_v7();
        let fence = tasks
            .acquire_scheduler(owner, 30)
            .await
            .expect("scheduler")
            .expect("fence");
        let table = ForgeClaimTable {
            table_uid: [7_u8; 16],
            catalog_name: "wyrd-redux".to_owned(),
            namespace_name: "vala.bifrost".to_owned(),
            table_name: "orphan".to_owned(),
            table_uuid: Uuid::now_v7(),
        };
        let claim = tasks
            .claim_fair(owner, limits(4), Some(MAINTENANCE_STRATEGIES))
            .await
            .expect("claim")
            .expect("claimed orphan task");
        assert_eq!(claim.task_id, id);
        assert_eq!(
            claim.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::OrphanCleanup)
        );
        assert_eq!(
            claim
                .plan
                .orphan_cleanup_payload(ForgeTaskStrategy::OrphanCleanup, true)
                .expect("payload round-trips"),
            OrphanCleanupPayload {
                version: ORPHAN_CLEANUP_PAYLOAD_VERSION,
                age_cutoff_ms: cutoff_ms
            },
            "the immutable cutoff survives enqueue and claim unchanged"
        );
        assert_eq!(
            claim
                .plan
                .orphan_cleanup_prefix(true)
                .expect("prefix round-trips"),
            prefix
        );
        assert!(
            claim.evidence.is_none(),
            "a fresh orphan task starts with no cursor at all"
        );
        let attempt = claim.attempt_id.expect("attempt");
        tasks
            .start(
                id,
                attempt,
                owner,
                SnapshotWatermark {
                    snapshot_id: 7,
                    timestamp_ms: 700,
                },
            )
            .await
            .expect("start");
        let authority = ForgeExpirationAuthority {
            task_id: id,
            attempt_id: attempt,
            worker_id: owner,
            lease_key: format!("forge:table:{tenant}:vala.bifrost.orphan"),
            lease_fencing_token: fence,
        };
        sqlx::query("INSERT INTO vala.maintenance_leases (lease_key,owner,fencing_token,expires_at,heartbeat_at) VALUES ($1,$2,$3,now()+interval '10 minutes',now())")
            .bind(&authority.lease_key)
            .bind(authority.worker_id)
            .bind(authority.lease_fencing_token)
            .execute(&admin)
            .await
            .expect("seed lease");

        // The cursor records traversal only, and only inside this task's own
        // prefix. A sibling table's key, a traversal segment, and the prefix
        // itself are all refused, so a cursor can never widen what the task
        // scans or name work outside its table.
        for (bad, why) in [
            (
                format!("tenants/{tenant}/bifrost/other/data/forge/v1/object.parquet"),
                "a sibling table's key is not beneath this task's prefix",
            ),
            (
                format!("{prefix}/../object.parquet"),
                "a traversal segment escapes the prefix",
            ),
            (
                prefix.clone(),
                "the prefix itself is not strictly beneath the prefix",
            ),
            (String::new(), "an empty key names no object"),
        ] {
            assert!(
                tasks
                    .checkpoint_orphan_cleanup_cursor(tenant, &authority, &table, &bad)
                    .await
                    .is_err(),
                "{why}"
            );
        }
        assert!(
            raw_evidence(&admin, id).await.is_none(),
            "a refused checkpoint writes no evidence"
        );

        let first_key = format!("{prefix}/aaa/00000000-0000-7000-8000-000000000001.parquet");
        assert_eq!(
            tasks
                .checkpoint_orphan_cleanup_cursor(tenant, &authority, &table, &first_key)
                .await
                .expect("first checkpoint"),
            ForgeTaskTransitionOutcome::Applied
        );
        assert_eq!(
            raw_evidence(&admin, id).await.expect("cursor evidence"),
            serde_json::json!({"version": 1, "start_after": first_key}),
            "the persisted cursor is exactly the closed two-field object"
        );

        // Ambiguous ownership cannot move the frontier: a stale attempt, a
        // foreign owner, and a foreign table each fail before any write.
        for (ambiguous, why) in [
            (
                ForgeExpirationAuthority {
                    attempt_id: Uuid::now_v7(),
                    ..authority.clone()
                },
                "a stale attempt no longer owns this task",
            ),
            (
                ForgeExpirationAuthority {
                    worker_id: Uuid::now_v7(),
                    ..authority.clone()
                },
                "a foreign owner never held this claim",
            ),
            (
                ForgeExpirationAuthority {
                    lease_fencing_token: fence + 1,
                    ..authority.clone()
                },
                "a stale fence has lost table authority",
            ),
        ] {
            assert!(
                tasks
                    .checkpoint_orphan_cleanup_cursor(
                        tenant,
                        &ambiguous,
                        &table,
                        &format!("{prefix}/zzz/object.parquet")
                    )
                    .await
                    .is_err(),
                "{why}"
            );
        }
        assert!(
            tasks
                .checkpoint_orphan_cleanup_cursor(
                    tenant,
                    &authority,
                    &ForgeClaimTable {
                        table_name: "other".to_owned(),
                        ..table.clone()
                    },
                    &format!("{prefix}/zzz/object.parquet")
                )
                .await
                .is_err(),
            "a cross-table checkpoint cannot reach this task"
        );
        assert_eq!(
            raw_evidence(&admin, id).await.expect("cursor evidence"),
            serde_json::json!({"version": 1, "start_after": first_key}),
            "no ambiguous caller moved the frontier"
        );

        // A partial scan releases the task without consuming failure budget,
        // and the cursor survives both the release and the successor claim.
        let (_, attempts_before) = state_and_attempts(&admin, id).await;
        tasks
            .retry(id, attempt, owner, Utc::now() - Duration::seconds(1))
            .await
            .expect("partial scan release");
        let (state, attempts_after) = state_and_attempts(&admin, id).await;
        assert_eq!(state, "retryable");
        assert_eq!(
            attempts_after, attempts_before,
            "a partial scan is progress, not an attempt-consuming failure"
        );
        let resumed = tasks
            .claim_fair(owner, limits(4), Some(MAINTENANCE_STRATEGIES))
            .await
            .expect("resume claim")
            .expect("resumed orphan task");
        assert_eq!(resumed.task_id, id, "the successor is the same task");
        assert_eq!(
            resumed
                .evidence
                .as_ref()
                .and_then(ForgeTaskRowEvidence::orphan_scan)
                .map(|cursor| cursor.start_after.clone()),
            Some(first_key.clone()),
            "the successor resumes from the exact persisted frontier"
        );
        let resumed_attempt = resumed.attempt_id.expect("resumed attempt");
        assert_ne!(resumed_attempt, attempt, "the successor is a new attempt");
        tasks
            .start(
                id,
                resumed_attempt,
                owner,
                SnapshotWatermark {
                    snapshot_id: 7,
                    timestamp_ms: 700,
                },
            )
            .await
            .expect("resume start");
        let resumed_authority = ForgeExpirationAuthority {
            attempt_id: resumed_attempt,
            ..authority.clone()
        };
        let last_key = format!("{prefix}/zzz/00000000-0000-7000-8000-000000000002.parquet");
        tasks
            .checkpoint_orphan_cleanup_cursor(tenant, &resumed_authority, &table, &last_key)
            .await
            .expect("resumed checkpoint");

        assert_eq!(
            tasks
                .complete_orphan_cleanup(tenant, &resumed_authority, &table)
                .await
                .expect("exhaustion completes the task"),
            ForgeTaskTransitionOutcome::Applied
        );
        assert_eq!(state_and_attempts(&admin, id).await.0, "succeeded");
        assert!(
            raw_evidence(&admin, id).await.is_none(),
            "exhaustion clears the traversal cursor"
        );
        assert!(
            tasks
                .complete_orphan_cleanup(tenant, &resumed_authority, &table)
                .await
                .is_err(),
            "a completed orphan task cannot complete twice"
        );
        assert!(
            tasks
                .checkpoint_orphan_cleanup_cursor(tenant, &resumed_authority, &table, &last_key)
                .await
                .is_err(),
            "a completed orphan task cannot resume its scan"
        );
    }
    /// Forces one enqueued task into an exact durable ownership shape.
    ///
    /// Recovery is defined over durable columns rather than over the path that
    /// produced them, so the states under test are written directly. `owner`
    /// names the holder of the claim and `expired` decides whether that claim
    /// still authorizes work, which is exactly the pair the predicate reads.
    ///
    /// # Panics
    /// Panics when the update fails.
    async fn force_claim_state(
        superuser: &PgPool,
        task_id: Uuid,
        state: &str,
        owner: Uuid,
        expired: bool,
    ) {
        sqlx::query(
            "UPDATE vala.forge_tasks SET state=$2,claimed_by=$3,attempt_id=$4,\
             claim_expires_at=statement_timestamp()+($5*interval '1 minute'),\
             watermark_snapshot_id=CASE WHEN $2 IN ('running','prepared') THEN 41 END,\
             watermark_timestamp_ms=CASE WHEN $2 IN ('running','prepared') THEN 700 END,\
             evidence=CASE WHEN $2='prepared' \
                 THEN '{\"version\":1,\"cleanup_candidates\":[],\"deleted_candidate_count\":0}'::jsonb END,\
             updated_at=statement_timestamp() WHERE task_id=$1",
        )
        .bind(task_id)
        .bind(state)
        .bind(owner)
        .bind(Uuid::now_v7())
        .bind(if expired { -10_i64 } else { 10_i64 })
        .execute(superuser)
        .await
        .expect("force claim state");
    }

    /// Deletes every seeded task so the next case asserts in isolation.
    ///
    /// # Panics
    /// Panics when the delete fails.
    async fn clear_tasks(superuser: &PgPool) {
        sqlx::query("DELETE FROM vala.forge_tasks")
            .execute(superuser)
            .await
            .expect("clear tasks");
    }

    /// Exact shape of one seeded unowned cleanup row.
    ///
    /// The recovery predicate is decided by the combination of strategy, state,
    /// retained cursor, and time gate, so those travel together as one value
    /// rather than as a long positional argument list.
    struct UnownedCleanupRow<'a> {
        /// Table this cleanup row belongs to.
        table_name: &'a str,
        /// Cleanup strategy the row is claimed under.
        strategy: ForgeTaskStrategy,
        /// Durable plan persisted with the row.
        plan: &'a ForgeTaskPlan,
        /// Exact durable state the row is parked in.
        state: &'a str,
        /// Retained cursor evidence the recovery claim must resume.
        evidence: &'a serde_json::Value,
        /// Whether the row's fair-claim time gate is still in the future.
        deferred: bool,
    }

    /// Inserts one unowned cleanup row carrying an exact durable cursor.
    ///
    /// Expired cleanup is only reachable through a validated expiration
    /// handoff, and both cleanup strategies reach the recovery states under
    /// test through a release rather than an enqueue, so the row is written
    /// directly.
    ///
    /// # Panics
    /// Panics when the insert fails.
    async fn seed_unowned_cleanup_row(
        superuser: &PgPool,
        tenant: DataTenantId,
        row: UnownedCleanupRow<'_>,
    ) -> Uuid {
        let UnownedCleanupRow {
            table_name,
            strategy,
            plan,
            state,
            evidence,
            deferred,
        } = row;
        let task_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,\
             table_name,strategy,base_snapshot_id,plan,plan_hash,estimated_files,\
             estimated_bytes,state,evidence,ready_at,next_eligible_at,updated_at) \
             VALUES ($1,$2,'wyrd-redux','vala.bifrost',$3,$4,4242,$5::jsonb,\
             decode(repeat('22',32),'hex'),1,100,$6,$7::jsonb,\
             statement_timestamp()+($8*interval '1 hour'),\
             statement_timestamp()+($8*interval '1 hour'),statement_timestamp())",
        )
        .bind(task_id)
        .bind(tenant.as_uuid())
        .bind(table_name)
        .bind(strategy.as_str())
        .bind(serde_json::to_string(&plan_to_value(plan)).expect("plan json"))
        .bind(state)
        .bind(evidence.to_string())
        .bind(if deferred { 1_i64 } else { -1_i64 })
        .execute(superuser)
        .await
        .expect("seed unowned cleanup row");
        task_id
    }

    /// Renders one task plan as the JSON shape the durable column stores.
    fn plan_to_value(plan: &ForgeTaskPlan) -> serde_json::Value {
        serde_json::json!({
            "version": plan.version,
            "inputs": plan.inputs,
            "parameters": plan.parameters})
    }

    /// Unattended work is exactly this owner's pre-terminal rows minus its own.
    ///
    /// A running worker's live attempts are ordinary in-flight work, and only
    /// the caller knows which those are, so the exclusion list is what
    /// separates them from the residue of an attempt it released without being
    /// able to account for. Both edges matter: an empty list must ask the
    /// broader question a caller holding no attempts needs, and a listed task
    /// must not keep its own owner unready forever.
    ///
    /// # Panics
    /// Panics when PostgreSQL setup or any state assertion fails.
    #[tokio::test]
    async fn unattended_work_excludes_the_attempts_its_caller_still_holds() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let tenant = fixture.data_tenant_id();
        let owner = Uuid::now_v7();
        let foreign = Uuid::now_v7();

        assert!(
            !tasks
                .has_unattended_work(owner, &[])
                .await
                .expect("empty predicate"),
            "an empty queue leaves nothing unattended"
        );

        let attended = tasks
            .enqueue(&task(tenant, "attended", 21))
            .await
            .expect("enqueue attended task");
        force_claim_state(&admin, attended, "running", owner, false).await;
        assert!(
            tasks
                .has_unattended_work(owner, &[])
                .await
                .expect("unlisted predicate"),
            "a caller holding no attempts still owns every pre-terminal row it claimed"
        );
        assert!(
            !tasks
                .has_unattended_work(owner, &[attended])
                .await
                .expect("listed predicate"),
            "an attempt its caller is still executing is not residue"
        );

        // The released residue: the same owner, a second pre-terminal row, and
        // nothing in the caller's hands that accounts for it.
        let released = tasks
            .enqueue(&task(tenant, "released", 22))
            .await
            .expect("enqueue released task");
        force_claim_state(&admin, released, "running", owner, false).await;
        assert!(
            tasks
                .has_unattended_work(owner, &[attended])
                .await
                .expect("residue predicate"),
            "a pre-terminal row this owner is not executing keeps it unready"
        );

        // Another worker's live claim is that worker's problem, never this
        // one's: gating on it would stop a healthy owner indefinitely.
        clear_tasks(&admin).await;
        let elsewhere = tasks
            .enqueue(&task(tenant, "foreign", 23))
            .await
            .expect("enqueue foreign task");
        force_claim_state(&admin, elsewhere, "running", foreign, true).await;
        assert!(
            !tasks
                .has_unattended_work(owner, &[])
                .await
                .expect("foreign predicate"),
            "an attempt another owner holds is not this owner's unattended work"
        );
    }

    /// Recovery covers exactly the durable residue a new owner must resolve.
    ///
    /// Worker readiness is gated on this predicate, so both halves of it must
    /// be exact. Too narrow and a worker advertises itself while a reader can
    /// still observe a half-settled effect nobody is reconciling; too wide and
    /// ordinary demand — or another worker's live claim — would keep the worker
    /// unready forever, because neither ever resolves without new slots.
    ///
    /// The recovery claim is the second half: it selects only unowned cleanup
    /// cursors and keeps the fair claim's existing timing gates, so a deferred
    /// cursor stays unready without becoming claimable early.
    ///
    /// # Panics
    /// Panics when PostgreSQL setup or any state assertion fails.
    #[tokio::test]
    async fn recoverable_cleanup_claim_and_predicate_cover_exact_states() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let tenant = fixture.data_tenant_id();
        let owner = Uuid::now_v7();
        let foreign = Uuid::now_v7();

        assert!(
            !tasks
                .has_recoverable_work(owner)
                .await
                .expect("empty predicate"),
            "an empty queue leaves nothing to recover"
        );

        // A pre-terminal attempt this owner still holds is unresolved work: it
        // is this worker's own residue from a previous process lifetime.
        for state in ["claimed", "running", "prepared"] {
            let id = tasks
                .enqueue(&task(tenant, "owned", 11))
                .await
                .expect("enqueue owned task");
            force_claim_state(&admin, id, state, owner, false).await;
            assert!(
                tasks
                    .has_recoverable_work(owner)
                    .await
                    .expect("owned predicate"),
                "a current-owner {state} attempt is unresolved recovery work"
            );
            clear_tasks(&admin).await;
        }

        // A lapsed claim is unresolved regardless of who held it: the previous
        // owner can no longer prove what it did to durable state.
        for state in ["claimed", "running", "prepared"] {
            let id = tasks
                .enqueue(&task(tenant, "expired", 12))
                .await
                .expect("enqueue expired task");
            force_claim_state(&admin, id, state, foreign, true).await;
            assert!(
                tasks
                    .has_recoverable_work(owner)
                    .await
                    .expect("expired predicate"),
                "an expired foreign {state} attempt is unresolved recovery work"
            );
            clear_tasks(&admin).await;
        }

        // An unowned cleanup row carrying cursor evidence is residue too: some
        // earlier attempt already deleted objects and checkpointed its scan,
        // and that scan is not finished. Both cleanup strategies and both
        // unowned states count, and the recovery claim selects them.
        let prefix = format!("tenants/{tenant}/bifrost/orphan/data/forge/v1");
        let cursor = serde_json::json!({
            "version": 1,
            "start_after": format!("{prefix}/aaa/object.parquet")});
        let expired_plan = ForgeTaskPlan {
            version: FORGE_TASK_PAYLOAD_VERSION,
            inputs: Vec::new(),
            parameters: ExpiredCleanupPayload::from_handoff(
                Uuid::now_v7(),
                &handoff_evidence(vec![cleanup_candidate("one")]),
            )
            .expect("committed handoff identity")
            .to_value(),
        };
        let expired_evidence =
            vala_sql::row_types::forge_tasks::evidence_to_value(&handoff_evidence(vec![
                cleanup_candidate("one"),
            ]));
        for state in ["ready", "retryable"] {
            for (label, table_name, strategy, plan, evidence) in [
                (
                    "orphan",
                    "orphan",
                    ForgeTaskStrategy::OrphanCleanup,
                    orphan_plan(&prefix, 1_700_000_000_000),
                    cursor.clone(),
                ),
                (
                    "expired",
                    "cleanup",
                    ForgeTaskStrategy::ExpiredCleanup,
                    expired_plan.clone(),
                    expired_evidence.clone(),
                ),
            ] {
                let id = seed_unowned_cleanup_row(
                    &admin,
                    tenant,
                    UnownedCleanupRow {
                        table_name,
                        strategy,
                        plan: &plan,
                        state,
                        evidence: &evidence,
                        deferred: false,
                    },
                )
                .await;
                assert!(
                    tasks
                        .has_recoverable_work(owner)
                        .await
                        .expect("cleanup predicate"),
                    "an unowned {state} {label} cleanup cursor is recovery work"
                );
                let claimed = tasks
                    .claim_recoverable_cleanup(owner, limits(4))
                    .await
                    .expect("recovery claim")
                    .expect("the cleanup cursor is claimable");
                assert_eq!(
                    claimed.task_id, id,
                    "recovery claims the exact {state} {label} cursor"
                );
                clear_tasks(&admin).await;
            }
        }

        // A deferred cursor keeps the worker unready without becoming claimable
        // early: the predicate ignores the time gates, the recovery claim keeps
        // the fair claim's existing ones.
        let deferred = seed_unowned_cleanup_row(
            &admin,
            tenant,
            UnownedCleanupRow {
                table_name: "deferred",
                strategy: ForgeTaskStrategy::OrphanCleanup,
                plan: &orphan_plan(&prefix, 1_700_000_000_000),
                state: "retryable",
                evidence: &cursor,
                deferred: true,
            },
        )
        .await;
        assert!(
            tasks
                .has_recoverable_work(owner)
                .await
                .expect("deferred predicate"),
            "a deferred cursor is still unresolved recovery work"
        );
        assert!(
            tasks
                .claim_recoverable_cleanup(owner, limits(4))
                .await
                .expect("deferred recovery claim")
                .is_none(),
            "a deferred cursor is not claimable before its own timing gates"
        );
        sqlx::query(
            "UPDATE vala.forge_tasks SET ready_at=statement_timestamp(),\
             next_eligible_at=statement_timestamp() WHERE task_id=$1",
        )
        .bind(deferred)
        .execute(&admin)
        .await
        .expect("release the cursor gates");
        assert_eq!(
            tasks
                .claim_recoverable_cleanup(owner, limits(4))
                .await
                .expect("released recovery claim")
                .expect("the released cursor is claimable")
                .task_id,
            deferred,
            "the exact released cursor is the one recovery claims"
        );
        clear_tasks(&admin).await;

        // Ordinary demand, an evidence-free cleanup row, and another worker's
        // live claim are all outside recovery: none of them is residue this
        // owner must resolve before it may advertise itself.
        let ordinary = tasks
            .enqueue(&task(tenant, "ordinary", 14))
            .await
            .expect("enqueue ordinary task");
        let bare = tasks
            .enqueue(&orphan_task(tenant, "bare", &prefix, 1_700_000_000_000))
            .await
            .expect("enqueue evidence-free cleanup");
        assert!(
            raw_evidence(&admin, bare).await.is_none(),
            "a freshly enqueued cleanup task carries no cursor"
        );
        let live = tasks
            .enqueue(&task(tenant, "live", 15))
            .await
            .expect("enqueue live foreign claim");
        force_claim_state(&admin, live, "running", foreign, false).await;
        assert!(
            !tasks
                .has_recoverable_work(owner)
                .await
                .expect("excluded predicate"),
            "ready demand, a bare cleanup row, and a live foreign claim are not recovery"
        );
        assert!(
            tasks
                .claim_recoverable_cleanup(owner, limits(4))
                .await
                .expect("excluded recovery claim")
                .is_none(),
            "recovery claims no ordinary task and no evidence-free cleanup row"
        );
        let _ = (ordinary, bare);
    }

    /// The telemetry snapshot reports exact demand and exact pending work.
    ///
    /// Demand and pending tasks are two different populations: the demand
    /// count is every unacknowledged row with its earliest request time, while
    /// pending work counts only `ready` and `retryable` task rows per strategy
    /// with their earliest `ready_at`. A claimed, running, prepared, or
    /// terminal row is owned or finished and must not appear as queued work,
    /// and a strategy with nothing pending must produce no row at all rather
    /// than a stale one.
    ///
    /// # Panics
    ///
    /// Panics when the fixture, seeding, or either aggregate read fails, or
    /// when a snapshot disagrees with the durable rows.
    #[tokio::test]
    async fn forge_telemetry_snapshot_reports_exact_demand_and_pending_work() {
        let (fixture, admin) = setup().await;
        let tasks = ForgeTasks::new(fixture.operator_pool().clone());
        let tenant = fixture.data_tenant_id();

        for table in ["demand-old", "demand-new", "demand-third"] {
            let identity = ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", table)
                .expect("demand identity");
            tasks
                .upsert_periodic(tenant, &identity)
                .await
                .expect("planning demand");
        }
        let oldest_demand = Utc::now() - Duration::hours(3);
        sqlx::query(
            "UPDATE vala.forge_planning_demands SET first_requested_at=$1 WHERE table_name='demand-old'",
        )
        .bind(oldest_demand)
        .execute(&admin)
        .await
        .expect("age the oldest demand");

        // Two pending rows per strategy for the three strategies that stay
        // queued, plus one row each parked in every non-pending state.
        let pending_strategies = [
            ForgeTaskStrategy::SmallFiles,
            ForgeTaskStrategy::SnapshotExpiry,
            ForgeTaskStrategy::ScribePromotion,
        ];
        let oldest_ready = Utc::now() - Duration::hours(2);
        let mut hash = 1_u8;
        for (index, strategy) in pending_strategies.into_iter().enumerate() {
            for suffix in ["ready", "retryable"] {
                let table = format!("pending-{index}-{suffix}");
                let seed = NewForgeTask {
                    strategy,
                    ..task(tenant, &table, hash)
                };
                hash += 1;
                let task_id = tasks.enqueue(&seed).await.expect("pending seed");
                sqlx::query("UPDATE vala.forge_tasks SET state=$1, ready_at=$2 WHERE task_id=$3")
                    .bind(if suffix == "ready" {
                        "ready"
                    } else {
                        "retryable"
                    })
                    .bind(if index == 0 && suffix == "ready" {
                        oldest_ready
                    } else {
                        Utc::now()
                    })
                    .bind(task_id)
                    .execute(&admin)
                    .await
                    .expect("park the pending row");
            }
        }
        for (index, state) in [
            "claimed",
            "running",
            "prepared",
            "succeeded",
            "failed",
            "cancelled",
        ]
        .into_iter()
        .enumerate()
        {
            // Parked under a strategy that is also pending, so an owned or
            // finished row inflating its queue count fails here.
            let seed = task(tenant, &format!("owned-{index}"), hash);
            hash += 1;
            let task_id = tasks.enqueue(&seed).await.expect("non-pending seed");
            sqlx::query(
                "UPDATE vala.forge_tasks SET state=$2,\
                 claimed_by=CASE WHEN $2 IN ('claimed','running','prepared') THEN $3 END,\
                 attempt_id=CASE WHEN $2 IN ('claimed','running','prepared') THEN $4 END,\
                 claim_expires_at=CASE WHEN $2 IN ('claimed','running','prepared') \
                     THEN statement_timestamp()+interval '10 minutes' END,\
                 watermark_snapshot_id=CASE WHEN $2 IN ('running','prepared') THEN 41 END,\
                 watermark_timestamp_ms=CASE WHEN $2 IN ('running','prepared') THEN 700 END,\
                 evidence=CASE WHEN $2='prepared' \
                     THEN '{\"version\":1,\"cleanup_candidates\":[],\"deleted_candidate_count\":0}'::jsonb END,\
                 updated_at=statement_timestamp() WHERE task_id=$1",
            )
            .bind(task_id)
            .bind(state)
            .bind(Uuid::now_v7())
            .bind(Uuid::now_v7())
            .execute(&admin)
            .await
            .expect("park the non-pending row");
        }

        let demand = tasks
            .planning_demand_status()
            .await
            .expect("demand snapshot");
        assert_eq!(demand.demands, 3, "every unacknowledged demand row counts");
        assert_eq!(
            demand
                .oldest_requested_at
                .expect("an outstanding demand has a request time")
                .timestamp(),
            oldest_demand.timestamp(),
            "the demand snapshot reports the stored earliest request, not an age"
        );

        let pending = tasks.pending_task_status().await.expect("pending snapshot");
        assert_eq!(
            pending
                .iter()
                .map(|status| status.strategy)
                .collect::<std::collections::BTreeSet<_>>(),
            pending_strategies.into_iter().collect(),
            "only strategies with ready or retryable rows appear at all"
        );
        for status in &pending {
            assert_eq!(
                status.pending, 2,
                "{:?} counts exactly its ready and retryable rows",
                status.strategy
            );
        }
        let small_files = pending
            .iter()
            .find(|status| status.strategy == ForgeTaskStrategy::SmallFiles)
            .expect("the aged strategy is pending");
        assert_eq!(
            small_files
                .oldest_ready_at
                .expect("a pending row has a ready time")
                .timestamp(),
            oldest_ready.timestamp(),
            "the pending snapshot reports the stored earliest ready_at, not an age"
        );
    }
}
