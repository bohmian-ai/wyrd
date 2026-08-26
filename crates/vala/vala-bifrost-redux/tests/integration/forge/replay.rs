//! Terminal replay and live reconciliation: idempotence, atomic conflict
//! resolution, and transition parity.
//!
//! Module of the `forge` group; shared fixtures live in `support.rs`.
//!
//! The tests stay wrapped in `mod pg_tests` so the fast lane's `--skip pg_tests`
//! filter still excludes them.

mod pg_tests {

    use std::time::Duration;

    use iceberg::spec::{DataFile, DataFileBuilder, PrimitiveType, Type};

    use iceberg::transaction::{AddColumn, ApplyTransactionAction, Transaction};

    use opendal::Buffer;

    use tokio_util::sync::CancellationToken;

    use vala_bifrost_redux::forge::{
        ForgeConfig, ForgeLease, IcebergRewriteGroup, forge_lease_key,
    };

    use vala_sql::queries::forge_operations::ForgeOperations;

    use vala_sql::row_types::forge_operations::ForgeOperationFamily;

    use wyrd_spec::vala::api::{
        AuditDetail, ForgeIcebergRewritePhase, ForgeOrphanGcPhase, ForgeSnapshotExpirePhase,
        StoragePath,
    };

    use crate::forge::support::*;

    /// Build one deterministic snapshot-expiry detail for an exact phase.
    fn expiry_transition_detail(
        operation_id: uuid::Uuid,
        resource: &str,
        phase: ForgeSnapshotExpirePhase,
    ) -> AuditDetail {
        AuditDetail::ForgeSnapshotExpire {
            operation_id,
            phase,
            group: resource.to_owned(),
            base_metadata_location: StoragePath::new("metadata/v1.metadata.json")
                .expect("metadata path"),
            current_snapshot_id: Some(11),
            retained_ref_heads: vec![11],
            cutoff_ms: 10,
            selected_snapshot_ids: vec![3, 7],
        }
    }

    /// Build one deterministic orphan-GC detail for an exact phase.
    fn gc_transition_detail(
        operation_id: uuid::Uuid,
        resource: &str,
        phase: ForgeOrphanGcPhase,
    ) -> AuditDetail {
        let candidate = StoragePath::new("data/orphan.parquet").expect("candidate path");
        AuditDetail::ForgeOrphanGc {
            operation_id,
            phase,
            group: resource.to_owned(),
            candidate_paths: vec![candidate.clone()],
            deleted_paths: (!matches!(phase, ForgeOrphanGcPhase::Prepared))
                .then_some(vec![candidate])
                .unwrap_or_default(),
            skipped_paths: Vec::new(),
        }
    }

    /// Snapshot-expiry Recovered replay leaves one state row and two audits.
    #[tokio::test]
    async fn expiry_recovered_terminal_replay_is_exactly_idempotent() {
        let fixture = Fixture::new().await;
        let resource = format!("bifrost://{}/tests/expiry-recovered", fixture.tenant);
        let operation_id = uuid::Uuid::now_v7();
        let prepared = operation_event(
            "forge.snapshot_expire.prepared",
            &resource,
            expiry_transition_detail(operation_id, &resource, ForgeSnapshotExpirePhase::Prepared),
        );
        append_operation(
            &fixture,
            ForgeOperationFamily::SnapshotExpire,
            &prepared,
            true,
        )
        .await;
        let recovered = operation_event(
            "forge.snapshot_expire.recovered",
            &resource,
            expiry_transition_detail(operation_id, &resource, ForgeSnapshotExpirePhase::Recovered),
        );
        let mut lease = acquire_fixture_lease(&fixture).await;
        fixture
            .forge
            .append_expiry_transition_for_test(
                &mut lease,
                fixture.tenant,
                recovered.detail.as_ref().expect("recovered detail"),
                "forge.snapshot_expire.recovered",
            )
            .await
            .expect("production expiry recovery writer");
        let before_replay =
            family_transition_counts(&fixture, "snapshot_expire", "forge.snapshot_expire.").await;
        fixture
            .forge
            .append_expiry_transition_for_test(
                &mut lease,
                fixture.tenant,
                recovered.detail.as_ref().expect("recovered detail"),
                "forge.snapshot_expire.recovered",
            )
            .await
            .expect("production expiry recovery replay");
        assert_eq!(
            family_transition_counts(&fixture, "snapshot_expire", "forge.snapshot_expire.").await,
            before_replay
        );
        assert_exact_terminal(&fixture, &resource, "recovered").await;
    }

    /// Orphan-GC Recovered replay is idempotent and wrong-family input is atomic.
    #[tokio::test]
    async fn gc_recovered_replay_and_wrong_family_conflict_are_atomic() {
        let fixture = Fixture::new().await;
        let resource = format!("bifrost://{}/tests/gc-recovered", fixture.tenant);
        let operation_id = uuid::Uuid::now_v7();
        let prepared = operation_event(
            "forge.orphan_gc.prepared",
            &resource,
            gc_transition_detail(operation_id, &resource, ForgeOrphanGcPhase::Prepared),
        );
        append_operation(&fixture, ForgeOperationFamily::OrphanGc, &prepared, true).await;
        let recovered = operation_event(
            "forge.orphan_gc.recovered",
            &resource,
            gc_transition_detail(operation_id, &resource, ForgeOrphanGcPhase::Recovered),
        );
        let mut lease = acquire_fixture_lease(&fixture).await;
        fixture
            .forge
            .append_gc_transition_for_test(
                &mut lease,
                fixture.tenant,
                recovered.detail.as_ref().expect("recovered detail"),
                "forge.orphan_gc.recovered",
            )
            .await
            .expect("production GC recovery writer");
        let before_replay =
            family_transition_counts(&fixture, "orphan_gc", "forge.orphan_gc.").await;
        fixture
            .forge
            .append_gc_transition_for_test(
                &mut lease,
                fixture.tenant,
                recovered.detail.as_ref().expect("recovered detail"),
                "forge.orphan_gc.recovered",
            )
            .await
            .expect("production GC recovery replay");
        assert_eq!(
            family_transition_counts(&fixture, "orphan_gc", "forge.orphan_gc.").await,
            before_replay
        );
        assert_exact_terminal(&fixture, &resource, "recovered").await;

        let wrong_resource = format!("bifrost://{}/tests/wrong-family", fixture.tenant);
        let wrong_event = operation_event(
            "forge.snapshot_expire.prepared",
            &wrong_resource,
            expiry_transition_detail(
                uuid::Uuid::now_v7(),
                &wrong_resource,
                ForgeSnapshotExpirePhase::Prepared,
            ),
        );
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("wrong-family tenant connection");
        let owner = ForgeOperations::new(&wrong_resource, ForgeOperationFamily::OrphanGc)
            .expect("wrong-family owner");
        owner
            .append_prepared(&mut conn, &wrong_event)
            .await
            .expect_err("wrong family must conflict");
        drop(conn);
        assert_eq!(
            family_transition_counts(&fixture, "orphan_gc", "forge.snapshot_expire.").await,
            (1, 0),
            "wrong-family event must add no state or audit"
        );
    }

    /// Evolve the fixture table and return its reloaded current schema identity.
    ///
    /// # Panics
    ///
    /// Panics when the schema action or catalog reload fails.
    async fn evolve_table_schema(
        fixture: &Fixture,
        old_table: &iceberg::table::Table,
    ) -> (iceberg::table::Table, i32) {
        let schema_action =
            Transaction::new(old_table)
                .update_schema()
                .add_column(AddColumn::optional(
                    "evolved_value",
                    Type::Primitive(PrimitiveType::String),
                ));
        ApplyTransactionAction::apply(schema_action, Transaction::new(old_table))
            .expect("schema action")
            .commit(fixture.catalog.as_ref())
            .await
            .expect("schema evolution");
        let evolved_table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("evolved table");
        let current_schema_id = evolved_table.metadata().current_schema_id();
        (evolved_table, current_schema_id)
    }

    /// Builds a current snapshot containing both original and evolved-schema manifest files.
    async fn mixed_schema_current_table(fixture: &Fixture) -> (iceberg::table::Table, i32, i32) {
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("initial table");
        fixture.append_seed_manifest(&table, 0).await;
        let old_table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("old-schema table");
        fixture.append_seed_manifest(&old_table, 1).await;
        let old_table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("two-file old-schema table");
        let old_snapshot = old_table
            .metadata()
            .current_snapshot()
            .expect("old-schema snapshot");
        let old_schema_id = old_snapshot.schema_id().expect("old schema");
        let (evolved_table, current_schema_id) = evolve_table_schema(fixture, &old_table).await;
        assert_ne!(
            old_schema_id, current_schema_id,
            "schema update must commit"
        );
        let manifests = old_table
            .manifest_list_reader(old_snapshot)
            .load()
            .await
            .expect("old manifest list");
        let mut old_file = None;
        for manifest_file in manifests.entries() {
            let manifest = manifest_file
                .load_manifest(old_table.file_io())
                .await
                .expect("old manifest");
            if let Some(entry) = manifest.entries().iter().find(|entry| entry.is_alive()) {
                old_file = Some(entry.data_file().clone());
                break;
            }
        }
        let old_file = old_file.expect("old live file");
        let current_path = format!(
            "{}/data/mixed-schema-current.parquet",
            evolved_table.metadata().location().trim_end_matches('/')
        );
        let current_object_path = format!(
            "{}/data/mixed-schema-current.parquet",
            fixture.binding.object_prefix
        );
        let bytes = fixture
            .staging
            .read(&format!(
                "{}/input-0.parquet",
                fixture.binding.object_prefix
            ))
            .await
            .expect("read old-schema fixture object");
        fixture
            .staging
            .write(&current_object_path, bytes)
            .await
            .expect("write current-schema fixture object");
        let append = Transaction::new(&evolved_table)
            .fast_append()
            .add_data_files([copied_data_file(
                &old_file,
                current_path,
                evolved_table.metadata().default_partition_spec_id(),
            )]);
        ApplyTransactionAction::apply(append, Transaction::new(&evolved_table))
            .expect("append action")
            .commit(fixture.catalog.as_ref())
            .await
            .expect("current-schema append");
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("current table");
        let snapshot = table
            .metadata()
            .current_snapshot()
            .expect("current snapshot");
        assert_eq!(
            snapshot.schema_id().expect("current schema"),
            current_schema_id,
            "new manifest must use the evolved current schema"
        );
        (table, old_schema_id, current_schema_id)
    }

    /// Discovers one real current snapshot through manifests and preserves its
    /// complete candidate identity in a deterministic table plan.
    #[tokio::test]
    async fn current_snapshot_preserves_manifest_writer_schema_identity() {
        let fixture = Fixture::new().await;
        let (table, old_schema_id, current_schema_id) = mixed_schema_current_table(&fixture).await;
        let current_day = chrono::NaiveDate::from_ymd_opt(2026, 7, 15)
            .expect("fixed day")
            .and_hms_opt(0, 0, 0)
            .expect("midnight")
            .and_utc();

        let first = fixture
            .forge
            .discover_live_rewrites_for_test(&fixture.binding, &table, current_day)
            .await
            .expect("manifest discovery");
        let second = fixture
            .forge
            .discover_live_rewrites_for_test(&fixture.binding, &table, current_day)
            .await
            .expect("repeat manifest discovery");

        assert_eq!(first, second, "manifest order cannot affect the table plan");
        let groups = first.groups_for_test();
        let ordered_files = groups
            .iter()
            .flat_map(vala_bifrost_redux::forge::IcebergRewriteGroup::files_for_test)
            .collect::<Vec<_>>();
        let schema_ids = ordered_files
            .iter()
            .map(|file| file.schema_id_for_test())
            .collect::<Vec<_>>();
        let order_keys = ordered_files
            .iter()
            .map(|file| file.sort_key_for_test())
            .collect::<Vec<_>>();

        assert!(
            order_keys.windows(2).all(|pair| pair[0] <= pair[1]),
            "groups preserve the discovery and rewrite-stage input order"
        );
        assert_eq!(
            schema_ids
                .iter()
                .filter(|&&schema_id| schema_id == current_schema_id)
                .count(),
            1,
            "the appended manifest keeps its evolved writer schema"
        );
        assert!(
            schema_ids.contains(&old_schema_id),
            "the current snapshot retains at least one old-schema manifest file"
        );
        for group in groups {
            let files = group.files_for_test();
            assert!(
                files
                    .windows(2)
                    .all(|pair| pair[0].sort_key_for_test() <= pair[1].sort_key_for_test()),
                "the rewrite stage receives each group in candidate order"
            );
            for file in files {
                assert_eq!(
                    group.is_obsolete_schema_for_test(),
                    file.schema_id_for_test() == old_schema_id,
                    "only old-manifest candidates are obsolete-schema rewrites: {group:?}"
                );
            }
        }
    }

    /// Build two staging outputs and return one executable live-rewrite plan.
    ///
    /// # Panics
    ///
    /// Panics when staging, catalog configuration, discovery, or lease
    /// acquisition fails because each is a fixture invariant.
    async fn prepare_live_transition(
        fixture: &Fixture,
    ) -> (iceberg::table::Table, i64, IcebergRewriteGroup, ForgeLease) {
        fixture.schedule_and_execute().await;
        fixture.seed_files_at(100, 2, true).await;
        fixture.schedule_and_execute().await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("two-output live table");
        let action = Transaction::new(&table).update_table_properties().set(
            "write.target-file-size-bytes".to_owned(),
            "3000000".to_owned(),
        );
        ApplyTransactionAction::apply(action, Transaction::new(&table))
            .expect("live target property action")
            .commit(fixture.catalog.as_ref())
            .await
            .expect("live target property commit");
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("live target table");
        let current_day = chrono::NaiveDate::from_ymd_opt(2026, 7, 15)
            .expect("fixed day")
            .and_hms_opt(0, 0, 0)
            .expect("midnight")
            .and_utc();
        let plan = fixture
            .forge
            .discover_live_rewrites_for_test(&fixture.binding, &table, current_day)
            .await
            .expect("live replacement plan");
        let group = plan
            .groups_for_test()
            .iter()
            .find(|group| group.files_for_test().len() >= 2)
            .expect("eligible two-file live group")
            .clone();
        let lease = ForgeLease::acquire(
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
        .expect("live lease query")
        .expect("live lease");
        (table, plan.base_snapshot_id_for_test(), group, lease)
    }

    /// Live Prepared failure is atomic, while retry commits exact state/audit parity once.
    #[tokio::test]
    async fn live_replacement_injection_and_replay_preserve_transition_parity() {
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
        let (table, base_snapshot_id, group, mut lease) = prepare_live_transition(&fixture).await;

        fixture.forge.fail_next_prepared_live_audit_for_test();
        fixture
            .forge
            .replace_live_group_for_test(
                &mut lease,
                &fixture.binding,
                &table,
                base_snapshot_id,
                &group,
                &CancellationToken::new(),
            )
            .await
            .expect_err("Prepared injection must fail");
        assert_eq!(
            family_transition_counts(&fixture, "iceberg_rewrite", "forge.iceberg_rewrite.").await,
            (0, 0)
        );

        fixture
            .forge
            .replace_live_group_for_test(
                &mut lease,
                &fixture.binding,
                &table,
                base_snapshot_id,
                &group,
                &CancellationToken::new(),
            )
            .await
            .expect("live replacement retry");
        assert_terminal_family_parity(&fixture, "iceberg_rewrite").await;
        let committed_counts =
            family_transition_counts(&fixture, "iceberg_rewrite", "forge.iceberg_rewrite.").await;
        assert_eq!(committed_counts, (1, 2));

        fixture
            .forge
            .replace_live_group_for_test(
                &mut lease,
                &fixture.binding,
                &table,
                base_snapshot_id,
                &group,
                &CancellationToken::new(),
            )
            .await
            .expect("stale replay disposition");
        assert_eq!(
            family_transition_counts(&fixture, "iceberg_rewrite", "forge.iceberg_rewrite.").await,
            committed_counts,
            "replay must not duplicate state or audit"
        );
    }

    /// Aged abandoned live outputs are terminally reset exactly once and left
    /// intact for delayed orphan GC.
    ///
    /// Reconciliation is the logical-enqueue side of the deletion boundary: it
    /// records the exact output generation as `Reset` and never touches object
    /// storage, so `orphan_gc` alone owns TTL, refreshed protection, physical
    /// deletion, and the deletion audit. The surviving object is the observable
    /// proof of that split, and the replay proves the closed reset is not
    /// reopened.
    #[tokio::test]
    async fn live_reconciliation_resets_abandoned_output_and_replays_reset() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                max_concurrent_reads: 2,
                uncertainty_bound: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            true,
            2,
            fixture_snapshot(),
        )
        .await;
        let (mut lease, output_key) = prepare_abandoned_live_operation(&fixture).await;

        let outcome = fixture
            .forge
            .reconcile_live_replacements_for_test(
                &mut lease,
                &fixture.binding,
                &CancellationToken::new(),
                chrono::Utc::now(),
            )
            .await
            .expect("aged abandoned replacement resets");
        assert_eq!(outcome.reset, 1);
        assert!(!outcome.blocked);
        assert!(
            fixture
                .staging
                .exists(&output_key)
                .await
                .expect("reset output existence check"),
            "a logical reset must leave its output generation for orphan GC"
        );
        let replay = fixture
            .forge
            .reconcile_live_replacements_for_test(
                &mut lease,
                &fixture.binding,
                &CancellationToken::new(),
                chrono::Utc::now(),
            )
            .await
            .expect("closed reset has no open replay");
        assert_eq!(replay.reset, 0);
        assert_eq!(
            family_transition_counts(&fixture, "iceberg_rewrite", "forge.iceberg_rewrite.").await,
            (1, 2)
        );
    }

    /// Prepare one aged abandoned operation whose output is absent from manifests.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup, catalog inspection, storage, lease acquisition,
    /// or operation persistence fails.
    async fn prepare_abandoned_live_operation(fixture: &Fixture) -> (ForgeLease, String) {
        fixture.schedule_and_execute().await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("staging table");
        let snapshot = table
            .metadata()
            .current_snapshot()
            .expect("current snapshot");
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .expect("current manifest list");
        let mut input_paths = Vec::new();
        for manifest_file in manifests.entries() {
            let manifest = manifest_file
                .load_manifest(table.file_io())
                .await
                .expect("current manifest");
            input_paths.extend(
                manifest
                    .entries()
                    .iter()
                    .filter(|entry| entry.is_alive())
                    .map(|entry| {
                        StoragePath::new(entry.file_path()).expect("catalog input path is valid")
                    }),
            );
        }
        assert!(!input_paths.is_empty());
        let base_snapshot_id = snapshot.snapshot_id();
        let partition_spec_id = table.metadata().default_partition_spec_id();
        let lease = ForgeLease::acquire(
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
        .expect("reset lease query")
        .expect("reset lease");
        let operation_id = uuid::Uuid::now_v7();
        let resource = format!(
            "bifrost://{}/{}/{}",
            fixture.tenant, fixture.binding.table_ref.namespace, fixture.binding.table_ref.name
        );
        let output_key = format!(
            "{}/data/reset-{}.parquet",
            fixture.binding.object_prefix, operation_id
        );
        fixture
            .staging
            .write(&output_key, Buffer::from("prepared-output"))
            .await
            .expect("prepared output exists before reset");
        let detail = AuditDetail::ForgeIcebergRewrite {
            operation_id,
            phase: ForgeIcebergRewritePhase::Prepared,
            group: resource.clone(),
            base_snapshot_id,
            committed_snapshot_id: None,
            partition_spec_id,
            time_partition: fixture_seed_partition(fixture).await.to_wire(),
            target_file_size_bytes: 3_000_000,
            input_paths,
            output_paths: vec![
                StoragePath::new(output_key.clone()).expect("prepared output path is valid"),
            ],
            writer_recipe_version: "bifrost-writer-v1".to_owned(),
        };
        append_operation(
            fixture,
            ForgeOperationFamily::IcebergRewrite,
            &operation_event("forge.iceberg_rewrite.prepared", &resource, detail),
            true,
        )
        .await;
        (lease, output_key)
    }

    /// Clone one complete manifest data-file identity for a distinct fixture path.
    ///
    /// The copied Parquet bytes retain valid metrics and partition values while
    /// the following fast append writes a manifest under the evolved schema.
    ///
    /// # Panics
    ///
    /// Panics when the pinned Iceberg builder rejects a complete data-file
    /// identity copied from the old live manifest.
    fn copied_data_file(source: &DataFile, file_path: String, partition_spec_id: i32) -> DataFile {
        let mut builder = DataFileBuilder::default();
        builder
            .content(source.content_type())
            .file_path(file_path)
            .file_format(source.file_format())
            .partition(source.partition().clone())
            .record_count(source.record_count())
            .file_size_in_bytes(source.file_size_in_bytes())
            .column_sizes(source.column_sizes().clone())
            .value_counts(source.value_counts().clone())
            .null_value_counts(source.null_value_counts().clone())
            .nan_value_counts(source.nan_value_counts().clone())
            .lower_bounds(source.lower_bounds().clone())
            .upper_bounds(source.upper_bounds().clone())
            .partition_spec_id(partition_spec_id);
        if let Some(sort_order_id) = source.sort_order_id() {
            builder.sort_order_id(sort_order_id);
        }
        builder.build().expect("complete copied data-file identity")
    }
}
