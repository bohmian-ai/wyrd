//! Task scheduling: slot reservation, contention, demand-generation
//! refresh, and live discovery convergence.
//!
//! Module of the `forge` group; shared fixtures live in `support.rs`.
//!
//! The tests stay wrapped in `mod pg_tests` so the fast lane's `--skip pg_tests`
//! filter still excludes them.

mod pg_tests {

    use std::time::Duration;

    use tokio_util::sync::CancellationToken;

    use vala_bifrost_redux::forge::{
        ForgeConfig, ForgeError, ForgeScheduleOutcome, ForgeScheduler, IcebergCandidateFile,
    };

    use vala_sql::queries::forge_tasks::ForgeTasks;

    use vala_sql::row_types::forge_tasks::{
        FORGE_TASK_PAYLOAD_VERSION, ForgeClaimStrategy, ForgeTaskPlan, ForgeTaskStrategy,
        ForgeTaskTableIdentity,
    };

    use crate::forge::support::*;

    /// A reserved maintenance slot claims a ready maintenance task ahead of an
    /// older, ready compaction task, proving compaction backlog cannot starve
    /// maintenance at the claim seam (AC2).
    ///
    /// The compaction task is enqueued first, so a strategy-blind FIFO claim
    /// would take it; the reserved slot's maintenance-first claim call takes the
    /// snapshot-expiry task instead.
    #[tokio::test]
    async fn reserved_slot_prefers_maintenance_over_ready_compaction() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), false, 0, fixture_snapshot()).await;
        let tasks = ForgeTasks::new(fixture.operator_pool.clone());
        tasks
            .enqueue(&fixture.durable_task(
                ForgeTaskStrategy::SmallFiles,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: vec!["data/compaction.parquet".to_owned()],
                    parameters: serde_json::json!({"kind":"small_files"}),
                },
                11,
            ))
            .await
            .expect("compaction task enqueue");
        tasks
            .enqueue(&fixture.durable_task(
                ForgeTaskStrategy::SnapshotExpiry,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: vec!["metadata/expiry.json".to_owned()],
                    parameters: serde_json::json!({"kind":"maintenance"}),
                },
                22,
            ))
            .await
            .expect("maintenance task enqueue");
        let claim = fixture
            .worker
            .claim_next_for_test(true)
            .await
            .expect("reserved-slot claim query")
            .expect("reserved-slot claimed task");
        assert!(
            matches!(
                claim.strategy,
                ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry)
            ),
            "the reserved slot must claim maintenance ahead of an older ready compaction task: {:?}",
            claim.strategy
        );
    }

    /// The reserved maintenance slot falls back to compaction when no maintenance
    /// work is ready, so its capacity is reserved but never idled (AC2).
    #[tokio::test]
    async fn reserved_slot_falls_back_to_compaction_without_maintenance() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), false, 0, fixture_snapshot()).await;
        let tasks = ForgeTasks::new(fixture.operator_pool.clone());
        tasks
            .enqueue(&fixture.durable_task(
                ForgeTaskStrategy::SmallFiles,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: vec!["data/compaction.parquet".to_owned()],
                    parameters: serde_json::json!({"kind":"small_files"}),
                },
                33,
            ))
            .await
            .expect("compaction task enqueue");
        let claim = fixture
            .worker
            .claim_next_for_test(true)
            .await
            .expect("reserved-slot fallback claim query")
            .expect("reserved-slot fallback claimed task");
        assert!(
            matches!(
                claim.strategy,
                ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles)
            ),
            "the reserved slot must fall back to compaction when no maintenance is ready: {:?}",
            claim.strategy
        );
    }

    /// A second scheduler remains a successful standby while the leader lease is live.
    #[tokio::test]
    async fn scheduler_contention_is_a_standby_outcome() {
        let fixture = Fixture::new().await;
        let stop = CancellationToken::new();
        let leader = ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
            .expect("leader scheduler");
        let standby = ForgeScheduler::with_owner_for_test(&fixture.forge, uuid::Uuid::now_v7())
            .expect("standby scheduler");

        let leader_outcome = leader.schedule_once(&stop).await.expect("leader pass");
        assert!(
            !leader_outcome.standby,
            "leader outcome: {leader_outcome:?}"
        );
        let standby_outcome = standby
            .schedule_once(&stop)
            .await
            .expect("live lease contention is not a scheduler failure");
        assert!(
            standby_outcome.standby,
            "contending scheduler must report standby: {standby_outcome:?}"
        );
        assert_eq!(
            standby_outcome,
            ForgeScheduleOutcome {
                standby: true,
                ..ForgeScheduleOutcome::default()
            }
        );
    }

    /// A planning demand replaced during its acknowledgement is retried once from
    /// the newer generation without publishing the pass as complete.
    #[tokio::test]
    async fn scheduler_retries_replaced_demand_generation_once() {
        let fixture = Fixture::new().await;
        let stop = CancellationToken::new();
        let scheduler =
            ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
                .expect("fixture scheduler");
        scheduler.pause_before_demand_acknowledgement_for_test();
        let scheduling = scheduler.schedule_once(&stop);
        tokio::pin!(scheduling);
        tokio::select! {
            () = scheduler.wait_for_demand_acknowledgement_pause_for_test() => {}
            result = &mut scheduling => panic!("scheduler returned before demand acknowledgement pause: {result:?}"),
        }

        let identity = ForgeTaskTableIdentity::new(
            "wyrd-redux",
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        )
        .expect("fixture task identity");
        ForgeTasks::new(fixture.operator_pool.clone())
            .upsert_periodic(fixture.tenant, &identity)
            .await
            .expect("replace demand generation");
        scheduler.release_demand_acknowledgement_pause_for_test();

        let outcome = scheduling.await.expect("generation retry scheduling pass");
        assert_eq!(outcome.demands_seen, 1, "outcome: {outcome:?}");
        assert_eq!(outcome.demands_acknowledged, 1, "outcome: {outcome:?}");
        assert_eq!(outcome.tasks_enqueued, 1, "outcome: {outcome:?}");
        assert!(
            outcome.incomplete,
            "generation replacement prevents completion publication"
        );
        assert_eq!(scheduler.complete_publications_for_test(), 0);

        let durable: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND catalog_name='wyrd-redux' AND namespace_name=$2 AND table_name=$3 AND strategy='staging_fold'), (SELECT count(*) FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND catalog_name='wyrd-redux' AND namespace_name=$2 AND table_name=$3)",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("retried durable state");
        assert_eq!(
            durable,
            (1, 0),
            "retry must leave one task and no stale demand"
        );
    }

    /// Cancellation after successor-demand refresh preserves the successor without
    /// retry-side planning, acknowledgement, cursor, audit, or completion effects.
    #[tokio::test]
    async fn scheduler_cancellation_after_demand_refresh_preserves_successor() {
        let fixture = Fixture::new().await;
        let stop = CancellationToken::new();
        let scheduler =
            ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
                .expect("fixture scheduler");
        let cursor_before: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
        )
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("pre-cancellation scheduler state");
        let mut audit_conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("pre-cancellation audit connection");
        let audits_before: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$1")
                .bind(fixture.tenant.as_uuid())
                .fetch_one(&mut **audit_conn.transaction())
                .await
                .expect("pre-cancellation audit count");
        scheduler.pause_before_demand_acknowledgement_for_test();
        scheduler.pause_after_demand_refresh_for_test();
        let scheduling = scheduler.schedule_once(&stop);
        tokio::pin!(scheduling);
        tokio::select! {
            () = scheduler.wait_for_demand_acknowledgement_pause_for_test() => {}
            result = &mut scheduling => panic!("scheduler returned before demand acknowledgement pause: {result:?}"),
        }

        let identity = ForgeTaskTableIdentity::new(
            "wyrd-redux",
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        )
        .expect("fixture task identity");
        let successor_generation = ForgeTasks::new(fixture.operator_pool.clone())
            .upsert_periodic(fixture.tenant, &identity)
            .await
            .expect("replace demand generation");
        scheduler.release_demand_acknowledgement_pause_for_test();
        tokio::select! {
            () = scheduler.wait_for_demand_refresh_pause_for_test() => {}
            result = &mut scheduling => panic!("scheduler returned before successor refresh pause: {result:?}"),
        }

        stop.cancel();
        scheduler.release_demand_refresh_pause_for_test();
        let outcome = tokio::time::timeout(Duration::from_secs(1), scheduling)
            .await
            .expect("cancelled scheduler pass must return boundedly")
            .expect("cancelled scheduler pass");
        assert_eq!(outcome.demands_seen, 1, "outcome: {outcome:?}");
        assert_eq!(outcome.tasks_enqueued, 0, "outcome: {outcome:?}");
        assert_eq!(outcome.demands_acknowledged, 0, "outcome: {outcome:?}");
        assert!(outcome.incomplete, "outcome: {outcome:?}");
        assert_eq!(scheduler.complete_publications_for_test(), 0);

        let after: (i64, i64, Option<uuid::Uuid>) = sqlx::query_as(
            "SELECT (SELECT generation FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND catalog_name='wyrd-redux' AND namespace_name=$2 AND table_name=$3), (SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND catalog_name='wyrd-redux' AND namespace_name=$2 AND table_name=$3), last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("post-cancellation scheduler state");
        let mut audit_conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("post-cancellation audit connection");
        let audits_after: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$1")
                .bind(fixture.tenant.as_uuid())
                .fetch_one(&mut **audit_conn.transaction())
                .await
                .expect("post-cancellation audit count");
        assert_eq!(after, (successor_generation, 0, cursor_before));
        assert_eq!(audits_after, audits_before);
    }

    /// Proves a catalog registration drives planning and every later maintenance stage without staging history.
    #[tokio::test]
    async fn registered_table_without_file_list_history_is_still_scheduled() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                max_concurrent_reads: 2,
                ..ForgeConfig::default()
            },
            false,
            3,
            fixture_snapshot(),
        )
        .await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("empty registered table");
        fixture.append_seed_manifest(&table, 0).await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("first historical snapshot");
        fixture.append_seed_manifest(&table, 1).await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("second historical snapshot");
        fixture
            .append_seed_manifest_at(
                &table,
                2,
                chrono::DateTime::parse_from_rfc3339("2026-07-15T12:00:00Z")
                    .expect("tail event time")
                    .into(),
            )
            .await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("tail snapshot table");
        fixture.set_live_target_file_size(&table, 100_000_000).await;
        assert_eq!(fixture.delete_file_list_history().await, 3);

        let outcome = fixture.schedule_and_execute().await;
        assert_eq!(outcome.demands_seen, 1, "outcome: {outcome:?}");
        assert_eq!(outcome.demands_acknowledged, 1, "outcome: {outcome:?}");
        assert_eq!(outcome.tasks_enqueued, 1, "outcome: {outcome:?}");
        assert!(!outcome.incomplete, "outcome: {outcome:?}");

        fixture.seed_files(2, true).await;
        let owner = fixture
            .pg
            .superuser_pool()
            .await
            .expect("catalog owner pool");
        sqlx::query(
            "ALTER TABLE vala.bifrost_tables RENAME TO bifrost_tables_unavailable_for_test",
        )
        .execute(&owner)
        .await
        .expect("hide catalog roster");
        let failed_discovery =
            ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
                .expect("fixture scheduler")
                .schedule_once(&CancellationToken::new())
                .await;
        sqlx::query(
            "ALTER TABLE vala.bifrost_tables_unavailable_for_test RENAME TO bifrost_tables",
        )
        .execute(&owner)
        .await
        .expect("restore catalog roster");
        assert!(
            failed_discovery.is_err(),
            "catalog discovery failure must not fall back to file_list history"
        );
    }

    /// Proves the scheduler carries its captured plan base to the committed live-replacement fence.
    #[tokio::test]
    async fn scheduler_forwards_plan_base_snapshot_to_live_replacement() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                max_concurrent_reads: 2,
                max_bins_per_tick: 1,
                ..ForgeConfig::default()
            },
            true,
            2,
            fixture_snapshot(),
        )
        .await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("empty historical table");
        fixture.set_live_target_file_size(&table, 100_000_000).await;
        let first = fixture.schedule_and_execute().await;
        assert_eq!(first.tasks_enqueued, 1, "first outcome: {first:?}");
        fixture.seed_files_at(100, 2, true).await;
        let second = fixture.schedule_and_execute().await;
        assert_eq!(second.tasks_enqueued, 1, "second outcome: {second:?}");
        fixture.seed_files_at(999, 1, false).await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("tail base table");
        fixture
            .append_seed_manifest_at(
                &table,
                999,
                chrono::DateTime::parse_from_rfc3339("2026-07-14T23:00:00Z")
                    .expect("tail event time")
                    .into(),
            )
            .await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("base snapshot table");
        let plan = fixture
            .forge
            .discover_live_rewrites_for_test(&fixture.binding, &table, chrono::Utc::now())
            .await
            .expect("historical live plan");
        let base_snapshot_id = plan.base_snapshot_id_for_test();
        let selected = plan
            .groups_for_test()
            .first()
            .expect("scheduler must select one historical replacement group first");
        assert!(
            selected.files_for_test().len() >= 2,
            "the first scheduler-selected group must contain the historical additions: {selected:?}"
        );
        let candidate_snapshot_ids = selected
            .files_for_test()
            .iter()
            .map(IcebergCandidateFile::source_snapshot_id_for_test)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(candidate_snapshot_ids.len(), 2);
        assert!(
            candidate_snapshot_ids
                .iter()
                .all(|candidate_snapshot_id| *candidate_snapshot_id != base_snapshot_id),
            "the newer tail snapshot must make the plan base distinct from its historical additions"
        );
        assert_eq!(fixture.delete_file_list_history().await, 5);

        let outcome = fixture.schedule_and_execute().await;
        assert_eq!(outcome.tasks_enqueued, 1, "outcome: {outcome:?}");
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("base fence tenant connection");
        let fenced_base_snapshot_id: i64 = sqlx::query_scalar(
            "SELECT (current_detail ->> 'base_snapshot_id')::bigint \
             FROM vala.forge_operation_state \
             WHERE data_tenant_id = wyrd.current_tenant() \
               AND family = 'iceberg_rewrite' AND phase = 'committed'",
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("committed live fence detail");
        assert_eq!(fenced_base_snapshot_id, base_snapshot_id);
        assert!(!outcome.incomplete, "outcome: {outcome:?}");
    }

    /// Bounded live discovery refuses before reservation and durable debt drains to zero.
    ///
    /// # Panics
    ///
    /// Panics when bounded discovery reserves work or live debt fails to converge.
    #[tokio::test]
    async fn bounded_live_discovery_and_debt_converge() {
        let bounded = Fixture::new_with_config(
            ForgeConfig {
                min_files: 2,
                max_files_per_bin: 2,
                max_files_per_tick: 2,
                ..ForgeConfig::default()
            },
            true,
            3,
            fixture_snapshot(),
        )
        .await;
        let table = bounded
            .catalog
            .load_table(&bounded.binding.table_ident())
            .await
            .expect("empty bounded table");
        bounded.append_seed_manifest(&table, 0).await;
        let table = bounded
            .catalog
            .load_table(&bounded.binding.table_ident())
            .await
            .expect("first bounded snapshot");
        bounded.append_seed_manifest(&table, 1).await;
        let table = bounded
            .catalog
            .load_table(&bounded.binding.table_ident())
            .await
            .expect("second bounded snapshot");
        bounded.append_seed_manifest(&table, 2).await;
        let table = bounded
            .catalog
            .load_table(&bounded.binding.table_ident())
            .await
            .expect("cap-plus-one bounded snapshot");
        let refusal = bounded
            .forge
            .discover_live_rewrites_for_test(&bounded.binding, &table, chrono::Utc::now())
            .await;
        assert!(matches!(refusal, Err(ForgeError::Capacity { .. })));
        let refused_tasks: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1")
                .bind(bounded.tenant.as_uuid())
                .fetch_one(bounded.operator_pool.pool())
                .await
                .expect("refused task count");
        assert_eq!(refused_tasks, 0);

        let fixture = Fixture::new_with_config(
            ForgeConfig {
                min_files: 2,
                max_files_per_bin: 8,
                max_files_per_tick: 8,
                max_bins_per_tick: 8,
                ..ForgeConfig::default()
            },
            true,
            0,
            fixture_snapshot(),
        )
        .await;
        let (expected_debt_files, expected_debt_bytes) = fixture.prepare_two_file_live_debt().await;
        let debt = fixture.schedule_and_execute().await;
        assert_eq!(debt.tasks_enqueued, 1, "{debt:?}");
        assert_eq!(
            (debt.compaction_debt_files, debt.compaction_debt_bytes),
            (expected_debt_files, expected_debt_bytes),
            "{debt:?}"
        );
        let durable_inputs: i64 = sqlx::query_scalar("SELECT jsonb_array_length(plan->'inputs')::bigint FROM vala.forge_tasks WHERE data_tenant_id=$1 AND strategy='small_files' ORDER BY created_at DESC LIMIT 1").bind(fixture.tenant.as_uuid()).fetch_one(fixture.operator_pool.pool()).await.expect("durable inputs");
        assert_eq!(
            debt.compaction_debt_files,
            u64::try_from(durable_inputs).expect("input count")
        );
        let converged = fixture.schedule_and_execute().await;
        assert_eq!(
            (
                converged.compaction_debt_files,
                converged.compaction_debt_bytes,
                converged.tasks_enqueued
            ),
            (0, 0, 0),
            "{converged:?}"
        );
    }
}
