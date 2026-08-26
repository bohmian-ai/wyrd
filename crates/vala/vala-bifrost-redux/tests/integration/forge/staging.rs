//! Staging projection: rollback, idempotent replay, exact row parity, and
//! overflow behavior.
//!
//! Module of the `forge` group; shared fixtures live in `support.rs`.
//!
//! The tests stay wrapped in `mod pg_tests` so the fast lane's `--skip pg_tests`
//! filter still excludes them.

mod pg_tests {

    use tokio_util::sync::CancellationToken;

    use vala_bifrost_redux::forge::{ForgeConfig, ForgeLease, ForgeScheduler};

    use vala_sql::row_types::forge_operations::{ForgeOperationFamily, ForgeOperationTransition};

    use wyrd_spec::vala::api::{AuditDetail, ForgeCompactionPhase, StoragePath};

    use crate::forge::support::*;

    /// Build one deterministic staging-fold detail for an exact phase.
    fn staging_detail(
        operation_id: uuid::Uuid,
        resource: &str,
        phase: ForgeCompactionPhase,
    ) -> AuditDetail {
        staging_detail_for_inputs(
            operation_id,
            resource,
            phase,
            vec![uuid::Uuid::now_v7()],
            vec![StoragePath::new("staging/input.parquet").expect("input path")],
        )
    }

    /// Build one staging-fold detail with exact fixture input identities.
    fn staging_detail_for_inputs(
        operation_id: uuid::Uuid,
        resource: &str,
        phase: ForgeCompactionPhase,
        input_file_ids: Vec<uuid::Uuid>,
        input_paths: Vec<StoragePath>,
    ) -> AuditDetail {
        AuditDetail::ForgeCompaction {
            operation_id,
            phase,
            group: resource.to_owned(),
            input_file_ids,
            input_paths,
            output_paths: vec![StoragePath::new("data/output.parquet").expect("output path")],
            snapshot_id: matches!(
                phase,
                ForgeCompactionPhase::Committed | ForgeCompactionPhase::Recovered
            )
            .then_some(7),
            writer_recipe_version: "bifrost-writer-v1".to_owned(),
        }
    }

    /// A projection failure rolls back staging claims and its paired audit.
    #[tokio::test]
    async fn staging_projection_failure_rolls_back_claim_state_and_audit() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                max_concurrent_reads: 2,
                ..ForgeConfig::default()
            },
            true,
            2,
            fixture_snapshot(),
        )
        .await;
        let owner = fixture.pg.superuser_pool().await.expect("table-owner pool");
        sqlx::query(
            r"CREATE FUNCTION vala.reject_forge_operation_state_for_test()
                 RETURNS trigger LANGUAGE plpgsql AS $$
               BEGIN
                 RAISE EXCEPTION 'injected forge operation-state failure';
               END
               $$",
        )
        .execute(&owner)
        .await
        .expect("projection failure function");
        sqlx::query(
            r"CREATE TRIGGER reject_forge_operation_state_for_test
                 BEFORE INSERT ON vala.forge_operation_state
                 FOR EACH ROW
                 EXECUTE FUNCTION vala.reject_forge_operation_state_for_test()",
        )
        .execute(&owner)
        .await
        .expect("projection failure trigger");

        let stop = CancellationToken::new();
        let outcome = ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
            .expect("fixture scheduler")
            .schedule_once(&stop)
            .await
            .expect("isolated table plan");
        assert_eq!(outcome.tasks_enqueued, 1, "outcome: {outcome:?}");
        assert!(
            fixture.worker.execute_one_for_test(&stop).await.is_err(),
            "the injected operation projection must reject worker publication"
        );

        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("rollback tenant connection");
        let claimed: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.file_list \
             WHERE data_tenant_id = wyrd.current_tenant() AND compacted",
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("claimed file count");
        assert_eq!(claimed, 0);
        assert_eq!(
            family_transition_counts(&fixture, "staging_fold", "forge.file_compact.").await,
            (0, 0)
        );

        sqlx::query(
            r"DROP TRIGGER reject_forge_operation_state_for_test
                 ON vala.forge_operation_state",
        )
        .execute(&owner)
        .await
        .expect("remove projection failure trigger");
        sqlx::query("DROP FUNCTION vala.reject_forge_operation_state_for_test()")
            .execute(&owner)
            .await
            .expect("remove projection failure function");
    }

    /// Staging Recovered and Reset each preserve parity under terminal replay.
    #[tokio::test]
    async fn staging_recovered_and_reset_terminal_replays_are_exactly_idempotent() {
        let fixture = Fixture::new().await;
        for (suffix, terminal_phase, expected_phase) in [
            ("recovered", ForgeCompactionPhase::Recovered, "recovered"),
            ("reset", ForgeCompactionPhase::Reset, "reset"),
        ] {
            let resource = format!("bifrost://{}/tests/staging-{suffix}", fixture.tenant);
            let operation_id = uuid::Uuid::now_v7();
            let prepared = operation_event(
                "forge.file_compact.prepared",
                &resource,
                staging_detail(operation_id, &resource, ForgeCompactionPhase::Prepared),
            );
            assert!(matches!(
                append_operation(&fixture, ForgeOperationFamily::StagingFold, &prepared, true)
                    .await,
                ForgeOperationTransition::Applied { .. }
            ));
            let terminal = operation_event(
                &format!("forge.file_compact.{suffix}"),
                &resource,
                staging_detail(operation_id, &resource, terminal_phase),
            );
            assert!(matches!(
                append_operation(
                    &fixture,
                    ForgeOperationFamily::StagingFold,
                    &terminal,
                    false
                )
                .await,
                ForgeOperationTransition::Applied { .. }
            ));
            assert!(matches!(
                append_operation(
                    &fixture,
                    ForgeOperationFamily::StagingFold,
                    &terminal,
                    false
                )
                .await,
                ForgeOperationTransition::AlreadyApplied { .. }
            ));
            assert_exact_terminal(&fixture, &resource, expected_phase).await;
        }
    }

    /// Invoke one production staging reconciliation writer.
    ///
    /// # Panics
    ///
    /// Panics when the selected production writer returns an error.
    async fn invoke_staging_reconciliation_writer(
        fixture: &Fixture,
        lease: &mut ForgeLease,
        phase: ForgeCompactionPhase,
        ids: &[uuid::Uuid],
        detail: &AuditDetail,
    ) {
        let day = fixture_seed_partition(fixture).await;
        match phase {
            ForgeCompactionPhase::Recovered => fixture
                .forge
                .stamp_reconciled_for_test(lease, &fixture.binding, day, ids, detail, 7)
                .await
                .expect("production recovered writer"),
            ForgeCompactionPhase::Reset => fixture
                .forge
                .reset_reconciled_for_test(lease, &fixture.binding, day, ids, detail)
                .await
                .expect("production reset writer"),
            _ => panic!("test helper accepts only recovered or reset"),
        }
    }

    /// Assert the production staging writer's exact `file_list` result.
    ///
    /// # Panics
    ///
    /// Panics when the tenant query fails or row state differs from the phase.
    async fn assert_staging_writer_rows(
        fixture: &Fixture,
        ids: &[uuid::Uuid],
        phase: ForgeCompactionPhase,
    ) {
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("staging result tenant connection");
        let states: Vec<(bool, Option<i64>)> = sqlx::query_as(
            "SELECT compacted, committed_snapshot_id FROM vala.file_list \
             WHERE data_tenant_id = wyrd.current_tenant() AND id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("staging writer row effects");
        let expected = match phase {
            ForgeCompactionPhase::Recovered => (true, Some(7)),
            ForgeCompactionPhase::Reset => (false, None),
            _ => panic!("test helper accepts only recovered or reset"),
        };
        assert_eq!(states, vec![expected; ids.len()]);
    }

    /// Drive one production staging reconciliation writer and assert row effects.
    ///
    /// # Panics
    ///
    /// Panics when fixture SQL, lease acquisition, or the production writer
    /// fails, or when replay changes file-list or audit cardinality.
    async fn drive_staging_reconciliation_writer(fixture: &Fixture, phase: ForgeCompactionPhase) {
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("staging writer tenant connection");
        let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
            "SELECT id, file_path FROM vala.file_list \
             WHERE data_tenant_id = wyrd.current_tenant() ORDER BY id LIMIT 2",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("staging writer inputs");
        let ids = rows.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        let paths = rows
            .iter()
            .map(|(_, path)| StoragePath::new(path.clone()).expect("fixture input path"))
            .collect::<Vec<_>>();
        sqlx::query(
            "UPDATE vala.file_list SET compacted = true \
             WHERE data_tenant_id = wyrd.current_tenant() AND id = ANY($1)",
        )
        .bind(&ids)
        .execute(&mut **conn.transaction())
        .await
        .expect("prepare staging rows");
        conn.commit().await.expect("prepare staging rows commit");

        let resource = format!(
            "bifrost://{}/{}/{}",
            fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
        );
        let operation_id = uuid::Uuid::now_v7();
        let detail = staging_detail_for_inputs(
            operation_id,
            &resource,
            ForgeCompactionPhase::Prepared,
            ids.clone(),
            paths,
        );
        let prepared = operation_event("forge.file_compact.prepared", &resource, detail.clone());
        append_operation(fixture, ForgeOperationFamily::StagingFold, &prepared, true).await;
        let mut lease = acquire_fixture_lease(fixture).await;
        invoke_staging_reconciliation_writer(fixture, &mut lease, phase, &ids, &detail).await;
        let before_replay =
            family_transition_counts(fixture, "staging_fold", "forge.file_compact.").await;
        invoke_staging_reconciliation_writer(fixture, &mut lease, phase, &ids, &detail).await;
        assert_eq!(
            family_transition_counts(fixture, "staging_fold", "forge.file_compact.").await,
            before_replay
        );
        assert_staging_writer_rows(fixture, &ids, phase).await;
        assert_exact_terminal(
            fixture,
            &resource,
            match phase {
                ForgeCompactionPhase::Recovered => "recovered",
                ForgeCompactionPhase::Reset => "reset",
                _ => unreachable!("phase checked above"),
            },
        )
        .await;
    }

    /// The production staging recovery writer stamps rows and replays once.
    #[tokio::test]
    async fn staging_recovered_writer_stamps_rows_with_exact_parity() {
        drive_staging_reconciliation_writer(&Fixture::new().await, ForgeCompactionPhase::Recovered)
            .await;
    }

    /// The production staging reset writer restores rows and replays once.
    #[tokio::test]
    async fn staging_reset_writer_restores_rows_with_exact_parity() {
        drive_staging_reconciliation_writer(&Fixture::new().await, ForgeCompactionPhase::Reset)
            .await;
    }

    /// An overflowed staging projection blocks before touching any visible operation.
    #[tokio::test]
    async fn staging_projection_overflow_leaves_every_visible_row_prepared() {
        let config = ForgeConfig {
            max_open_operations_per_table: 1,
            ..ForgeConfig::default()
        };
        let fixture = Fixture::new_with_config(config, true, 4, fixture_snapshot()).await;
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("overflow tenant connection");
        let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
            "SELECT id, file_path FROM vala.file_list \
             WHERE data_tenant_id = wyrd.current_tenant() ORDER BY id LIMIT 2",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("overflow inputs");
        let ids = rows.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        sqlx::query(
            "UPDATE vala.file_list SET compacted = true \
             WHERE data_tenant_id = wyrd.current_tenant() AND id = ANY($1)",
        )
        .bind(&ids)
        .execute(&mut **conn.transaction())
        .await
        .expect("hide overflow inputs");
        conn.commit().await.expect("commit hidden overflow inputs");

        let resource = format!(
            "bifrost://{}/{}/{}",
            fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
        );
        let input_paths = rows
            .iter()
            .map(|(_, path)| StoragePath::new(path.clone()).expect("overflow input path"))
            .collect::<Vec<_>>();
        let mut operation_ids = Vec::new();
        for _ in 0..2 {
            let operation_id = uuid::Uuid::now_v7();
            operation_ids.push(operation_id);
            let detail = staging_detail_for_inputs(
                operation_id,
                &resource,
                ForgeCompactionPhase::Prepared,
                ids.clone(),
                input_paths.clone(),
            );
            let prepared = operation_event("forge.file_compact.prepared", &resource, detail);
            assert!(matches!(
                append_operation(&fixture, ForgeOperationFamily::StagingFold, &prepared, true)
                    .await,
                ForgeOperationTransition::Applied { .. }
            ));
        }
        sqlx::query(
            "UPDATE vala.forge_operation_state \
             SET prepared_at = now() - interval '1 hour' \
             WHERE data_tenant_id = $1 AND resource = $2 AND family = 'staging_fold'",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&resource)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("age overflow projection");

        let outcome = fixture.schedule_and_execute().await;
        assert_eq!(outcome.tasks_enqueued, 1, "outcome: {outcome:?}");
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("overflow assertion connection");
        let states: Vec<(bool, Option<i64>)> = sqlx::query_as(
            "SELECT compacted, committed_snapshot_id FROM vala.file_list \
             WHERE data_tenant_id = wyrd.current_tenant() AND id = ANY($1) ORDER BY id",
        )
        .bind(&ids)
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("overflow input states");
        assert_eq!(states, vec![(true, None); ids.len()]);
        let operations: Vec<(String, Option<i64>)> = sqlx::query_as(
            "SELECT phase, terminal_audit_seq FROM vala.forge_operation_state \
             WHERE data_tenant_id = wyrd.current_tenant() AND operation_id = ANY($1) \
             ORDER BY operation_id",
        )
        .bind(&operation_ids)
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("overflow operation states");
        assert_eq!(operations, vec![("prepared".to_owned(), None); 2]);
    }
}
