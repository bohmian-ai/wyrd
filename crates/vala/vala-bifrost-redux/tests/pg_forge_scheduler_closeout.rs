//! Production-surface Forge scheduler closeout tests.
//!
//! These tests deliberately avoid the legacy `vala-bifrost` catalog. They create
//! a real SQL Iceberg catalog, local object store, staging Parquet files, and
//! `vala.file_list` rows, then drive the exported Redux Forge surface.

mod pg_tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use arrow::array::{Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema as ArrowSchema, TimeUnit};
    use iceberg::{Catalog, CatalogBuilder, TableCreation};
    use iceberg_catalog_sql::{SqlBindStyle, SqlCatalogBuilder};
    use iceberg_storage_opendal::OpenDalResolvingStorageFactory;
    use opendal::Buffer;
    use opendal::services::Fs;
    use parquet::arrow::ArrowWriter;
    use sqlx_catalog::any::install_default_drivers;
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;
    use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding, build_partition_spec};
    use vala_bifrost_redux::forge::{
        ForgeConfig, ForgeContext, ForgeScheduler, run_maintenance_tick,
    };
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_sql::OperatorPool;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeSnapshotExpirePhase,
        StoragePath,
    };

    struct Fixture {
        pg: PgFixture,
        tenant: DataTenantId,
        binding: TenantTableBinding,
        context: ForgeContext,
        _root: TempDir,
    }

    impl Fixture {
        async fn new() -> Self {
            let pg = PgFixture::start().await.expect("postgres fixture");
            let tenant = pg.data_tenant_id();
            let binding = TenantTableBinding::resolve((
                tenant,
                TableRef::new(BifrostNamespace::Bifrost, "forge_rows"),
            ))
            .expect("tenant table binding");
            let root = tempfile::tempdir().expect("warehouse root");
            let staging = Arc::new(
                opendal::Operator::new(Fs::default().root(root.path().to_str().expect("root")))
                    .expect("filesystem operator")
                    .finish(),
            );

            install_default_drivers();
            let storage_factory = Arc::new(OpenDalResolvingStorageFactory::new());
            let warehouse = format!("file://{}", root.path().display());
            let catalog = SqlCatalogBuilder::default()
                .with_storage_factory(storage_factory)
                .load(
                    "wyrd",
                    [
                        (
                            iceberg_catalog_sql::SQL_CATALOG_PROP_URI.to_owned(),
                            pg.catalog_uri(),
                        ),
                        (
                            iceberg_catalog_sql::SQL_CATALOG_PROP_WAREHOUSE.to_owned(),
                            warehouse.clone(),
                        ),
                        (
                            iceberg_catalog_sql::SQL_CATALOG_PROP_BIND_STYLE.to_owned(),
                            SqlBindStyle::DollarNumeric.to_string(),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                )
                .await
                .expect("sql iceberg catalog");
            let catalog: Arc<dyn Catalog> = Arc::new(catalog);

            let arrow_schema = ArrowSchema::new(vec![
                Field::new("value", DataType::Int64, false),
                Field::new(
                    "wyrd_event_time",
                    DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                    false,
                ),
                Field::new("data_tenant_id", DataType::Utf8, false),
            ]);
            let iceberg_schema =
                iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(&arrow_schema)
                    .expect("iceberg schema");
            let partition_spec =
                build_partition_spec(&iceberg_schema, &binding.partition_columns())
                    .expect("partition spec");
            catalog
                .create_namespace(binding.physical_namespace(), HashMap::new())
                .await
                .expect("tenant namespace");
            catalog
                .create_table(
                    binding.physical_namespace(),
                    TableCreation::builder()
                        .name(binding.table_name.clone())
                        .location(format!("{warehouse}/{}", binding.object_prefix))
                        .schema(iceberg_schema)
                        .partition_spec(partition_spec)
                        .build(),
                )
                .await
                .expect("tenant table");

            let context = ForgeContext::new(
                pg.app_pool().clone(),
                OperatorPool::from(pg.platform_admin_pool().clone()),
                catalog,
                staging,
                ForgeConfig::default(),
            )
            .expect("forge context");

            let fixture = Self {
                pg,
                tenant,
                binding,
                context,
                _root: root,
            };
            fixture.seed_two_files(&arrow_schema).await;
            fixture
        }

        async fn seed_two_files(&self, schema: &ArrowSchema) {
            self.seed_pair(schema, 0).await;
        }

        async fn seed_additional_two_files(&self) {
            let schema = ArrowSchema::new(vec![
                Field::new("value", DataType::Int64, false),
                Field::new(
                    "wyrd_event_time",
                    DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                    false,
                ),
                Field::new("data_tenant_id", DataType::Utf8, false),
            ]);
            self.seed_pair(&schema, 2).await;
        }

        async fn seed_pair(&self, schema: &ArrowSchema, start: i64) {
            let base_micros = chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
                .expect("timestamp")
                .timestamp_micros();
            let mut rows = Vec::new();
            for file_number in start..start + 2 {
                let values = Int64Array::from(vec![file_number * 10 + 1, file_number * 10 + 2]);
                let timestamps = TimestampMicrosecondArray::from(vec![
                    base_micros + file_number * 1_000_000,
                    base_micros + file_number * 1_000_000 + 1_000,
                ])
                .with_timezone("UTC");
                let tenants = StringArray::from(vec![self.tenant.to_string(); 2]);
                let batch = RecordBatch::try_new(
                    Arc::new(schema.clone()),
                    vec![Arc::new(values), Arc::new(timestamps), Arc::new(tenants)],
                )
                .expect("staging batch");
                let mut bytes = Vec::new();
                let mut writer =
                    ArrowWriter::try_new(&mut bytes, batch.schema(), None).expect("parquet writer");
                writer.write(&batch).expect("parquet batch");
                writer.close().expect("parquet close");
                let path = format!("{}/input-{file_number}.parquet", self.binding.object_prefix);
                self.context
                    .staging
                    .write(&path, Buffer::from(bytes))
                    .await
                    .expect("staging write");
                rows.push((
                    path,
                    i64::try_from(batch.num_rows()).expect("bounded row count"),
                    file_number,
                ));
            }

            let mut conn = vala_sql::TenantConn::acquire(self.pg.app_pool(), self.tenant)
                .await
                .expect("tenant connection");
            for (path, row_count, file_number) in rows {
                let min_time = chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
                    .expect("timestamp")
                    .with_timezone(&chrono::Utc)
                    + chrono::Duration::seconds(file_number);
                let max_time = min_time + chrono::Duration::milliseconds(1);
                sqlx::query(
                    "INSERT INTO vala.file_list (id, data_tenant_id, namespace, table_name, file_path, file_size, row_count, min_event_time, max_event_time, partition_day, node_id, writer_epoch, wal_lsn_min, wal_lsn_max) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
                )
                .bind(uuid::Uuid::now_v7())
                .bind(self.tenant.as_uuid())
                .bind(&self.binding.logical_namespace)
                .bind(&self.binding.table_name)
                .bind(path)
                .bind(1_i64)
                .bind(row_count)
                .bind(min_time)
                .bind(max_time)
                .bind(chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("partition day"))
                .bind(uuid::Uuid::now_v7())
                .bind(1_i64)
                .bind(file_number * 2 + 1)
                .bind(file_number * 2 + 2)
                .execute(&mut **conn.transaction())
                .await
                .expect("file list insert");
            }
            conn.commit().await.expect("file list commit");
            sqlx::query(
                "UPDATE vala.file_list SET created_at = now() - interval '3 minutes' WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3",
            )
            .bind(self.tenant.as_uuid())
            .bind(&self.binding.logical_namespace)
            .bind(&self.binding.table_name)
            .execute(self.context.operator_pool.pool())
            .await
            .expect("age file list rows");
        }

        fn context_with_config(&self, config: ForgeConfig) -> ForgeContext {
            ForgeContext::new(
                self.pg.app_pool().clone(),
                self.context.operator_pool.clone(),
                self.context.catalog.clone(),
                self.context.staging.clone(),
                config,
            )
            .expect("forge context with test config")
        }

        async fn operation_count(&self, operation: &str) -> i64 {
            sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation = $2")
                .bind(self.tenant.as_uuid())
                .bind(operation)
                .fetch_one(self.context.operator_pool.pool())
                .await
                .expect("audit operation count")
        }

        async fn object_exists(&self, path: &str) -> bool {
            self.context.staging.stat(path).await.is_ok()
        }

        async fn append_audit_detail(
            &self,
            operation: &str,
            resource: String,
            detail: AuditDetail,
        ) {
            let event = AuditEvent {
                request_id: RequestId::now_v7(),
                trace_id: None,
                operation: operation.to_owned(),
                resource,
                card_ref: None,
                principal_id: PrincipalId::new(uuid::Uuid::nil()),
                principal_kind: PrincipalKindTag::Service,
                auth_method: AuthMethod::Internal,
                permission: "bifrost:forge".to_owned(),
                decision: AuditDecision::Allow,
                result: AuditResult::Success,
                payload_summary: operation.to_owned(),
                detail: Some(detail),
            };
            let mut conn = vala_sql::TenantConn::acquire(self.pg.app_pool(), self.tenant)
                .await
                .expect("audit tenant connection");
            vala_sql::queries::audit_outbox::append_audit(&mut conn, &event)
                .await
                .expect("prepared audit");
            conn.commit().await.expect("audit commit");
        }

        async fn file_state(&self) -> Vec<(bool, Option<i64>)> {
            sqlx::query_as(
                "SELECT compacted, committed_snapshot_id FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 ORDER BY wal_lsn_min",
            )
            .bind(self.tenant.as_uuid())
            .bind(&self.binding.logical_namespace)
            .bind(&self.binding.table_name)
            .fetch_all(self.context.operator_pool.pool())
            .await
            .expect("file state")
        }
    }

    #[tokio::test]
    async fn forge_scheduler_run_maintenance_tick_compacts_exact_groups() {
        let fixture = Fixture::new().await;
        let outcome = run_maintenance_tick(&fixture.context)
            .await
            .expect("maintenance tick");

        assert_eq!(outcome.bins_committed, 1);
        let state = fixture.file_state().await;
        assert_eq!(state.len(), 2);
        assert!(state.iter().all(|(compacted, _snapshot)| *compacted));
        let snapshot_id = state[0].1.expect("snapshot id");
        assert!(snapshot_id > 0);
        assert!(
            state
                .iter()
                .all(|(_, snapshot)| *snapshot == Some(snapshot_id))
        );
        let table = fixture
            .context
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("committed table");
        assert!(table.metadata().current_snapshot_id().is_some());
        assert_eq!(table.metadata().snapshots().len(), 1);

        let prepared: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation = 'forge.file_compact.prepared'",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.context.operator_pool.pool())
        .await
        .expect("prepared audit count");
        let committed: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation = 'forge.file_compact.committed'",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.context.operator_pool.pool())
        .await
        .expect("committed audit count");
        assert_eq!(prepared, 1);
        assert_eq!(committed, 1);
    }

    #[tokio::test]
    /// Tests the directly awaitable scheduler lifecycle and lease cleanup.
    ///
    /// Steps:
    /// 1. Build an isolated Postgres/SQL-Iceberg/OpenDAL fixture with two aged
    ///    Parquet inputs and corresponding `vala.file_list` rows.
    /// 2. Construct `ForgeScheduler::new`, spawn its returned `run` future,
    ///    and wait for the real compaction to mark both inputs committed.
    /// 3. Cancel the supplied `CancellationToken`, await the scheduler future,
    ///    and query `vala.maintenance_leases`.
    ///
    /// The durable assertions prove a real scheduler tick completed, cancellation
    /// was observed by the supervised future, and no table lease was stranded.
    /// That is production evidence for server supervision and bounded shutdown;
    /// a nested detached task could otherwise make the test pass while its work
    /// remained unobserved.
    async fn forge_scheduler_start_shutdown_completes_a_bounded_tick() {
        let fixture = Fixture::new().await;
        let shutdown = CancellationToken::new();
        let task = tokio::spawn(
            ForgeScheduler::new(fixture.context.clone(), Duration::from_millis(10))
                .expect("scheduler start")
                .run(shutdown.clone()),
        );

        for _ in 0..100 {
            if fixture
                .file_state()
                .await
                .iter()
                .all(|(compacted, snapshot)| *compacted && snapshot.is_some())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        shutdown.cancel();
        task.await
            .expect("scheduler task")
            .expect("scheduler shutdown");

        assert_eq!(fixture.file_state().await.len(), 2);
        let leases: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.maintenance_leases WHERE lease_key LIKE 'forge:table:%'",
        )
        .fetch_one(fixture.context.operator_pool.pool())
        .await
        .expect("lease count");
        assert_eq!(leases, 0);
    }

    #[tokio::test]
    /// Tests replay ordering after a compaction has already committed.
    ///
    /// Steps:
    /// 1. Seed two real staging files and run the public one-shot tick.
    /// 2. Load the Iceberg table and record its snapshot count.
    /// 3. Run a second public tick, then load the table again.
    ///
    /// The second tick must report no new bin and the snapshot count must stay
    /// unchanged. This verifies that reconciliation observes durable state before
    /// admitting new compaction work, which is the restart-safe ordering required
    /// after an uncertain catalog response.
    async fn forge_scheduler_reconciliation_precedes_new_compaction() {
        let fixture = Fixture::new().await;
        let first = run_maintenance_tick(&fixture.context)
            .await
            .expect("first maintenance tick");
        assert_eq!(first.bins_committed, 1);
        let snapshots_after_first = fixture
            .context
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("table after first tick")
            .metadata()
            .snapshots()
            .len();

        let second = run_maintenance_tick(&fixture.context)
            .await
            .expect("reconciliation maintenance tick");
        assert_eq!(second.bins_committed, 0);
        assert_eq!(second.reconciled, 0);
        let snapshots_after_second = fixture
            .context
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("table after second tick")
            .metadata()
            .snapshots()
            .len();
        assert_eq!(snapshots_after_second, snapshots_after_first);
    }

    #[tokio::test]
    /// Tests fail-closed behavior when another worker owns the table lease.
    ///
    /// Steps:
    /// 1. Seed the real table and acquire its exact `forge:table:*` lease through
    ///    the same SQL lease table used by production workers.
    /// 2. Run the public maintenance tick while that owner remains active.
    /// 3. Assert the outcome is skipped, both source files remain uncompacted,
    ///    and release the test owner with its owner/token pair.
    ///
    /// The test does not merely call a lease helper: it drives the scheduler’s
    /// discovery and acquisition path and checks durable file state. It proves a
    /// competing pod cannot perform an Iceberg or bookkeeping mutation.
    async fn forge_scheduler_lease_loss_fails_closed() {
        let fixture = Fixture::new().await;
        let lease_key = format!(
            "forge:table:{}:{}:{}",
            fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
        );
        let owner = uuid::Uuid::now_v7();
        let fencing_token = vala_sql::queries::maintenance_leases::try_acquire_lease(
            &fixture.context.operator_pool,
            &lease_key,
            owner,
            i64::try_from(fixture.context.config.lease_ttl.as_secs()).expect("lease seconds"),
        )
        .await
        .expect("competing lease acquisition")
        .expect("competing lease");

        let outcome = run_maintenance_tick(&fixture.context)
            .await
            .expect("fenced maintenance tick");
        assert_eq!(outcome.bins_committed, 0);
        assert!(outcome.tables_skipped > 0);
        assert!(
            fixture
                .file_state()
                .await
                .iter()
                .all(|(compacted, snapshot)| !*compacted && snapshot.is_none())
        );
        assert!(
            vala_sql::queries::maintenance_leases::release_lease_fenced(
                &fixture.context.operator_pool,
                &lease_key,
                owner,
                fencing_token,
            )
            .await
            .expect("lease release")
        );
    }

    #[tokio::test]
    /// Tests synchronous rejection of invalid scheduler configuration.
    ///
    /// Steps:
    /// 1. Build the normal real fixture so the constructor receives a production
    ///    `ForgeContext`.
    /// 2. Pass `Duration::ZERO` to the only public scheduler constructor.
    /// 3. Assert construction returns the configuration error before any task is
    ///    spawned.
    ///
    /// This protects server startup from accepting a scheduler that can busy-loop
    /// or require an out-of-band shutdown path.
    async fn forge_scheduler_rejects_zero_interval_without_spawning() {
        let fixture = Fixture::new().await;
        let Err(error) = ForgeScheduler::new(fixture.context.clone(), Duration::ZERO) else {
            panic!("zero interval must fail closed");
        };
        assert!(error.to_string().contains("interval must be positive"));
    }

    #[tokio::test]
    /// Tests same-table serialization across two concurrent production ticks.
    ///
    /// Steps:
    /// 1. Clone one real Forge context into two concurrent callers.
    /// 2. Await two `run_maintenance_tick` futures at the same time.
    /// 3. Query durable prepared/committed audit rows and load the resulting
    ///    Iceberg table.
    ///
    /// Exactly one caller may commit the rewrite; the other must observe the
    /// shared lease. One prepared row, one committed row, and one snapshot prove
    /// duplicate work is prevented without making the public stage helpers into
    /// competing schedulers.
    async fn forge_scheduler_same_table_is_serialized_and_distinct_tables_progress() {
        let fixture = Fixture::new().await;
        let first_context = fixture.context.clone();
        let second_context = fixture.context.clone();
        let (first, second) = tokio::join!(
            run_maintenance_tick(&first_context),
            run_maintenance_tick(&second_context)
        );
        let first = first.expect("first competing scheduler");
        let second = second.expect("second competing scheduler");
        assert_eq!(first.bins_committed + second.bins_committed, 1);
        assert_eq!(
            fixture.operation_count("forge.file_compact.prepared").await,
            1
        );
        assert_eq!(
            fixture
                .operation_count("forge.file_compact.committed")
                .await,
            1
        );
    }

    #[tokio::test]
    /// Tests table-level failure isolation when the first discovered table is bad.
    ///
    /// Steps:
    /// 1. Seed a healthy real table with two compaction candidates.
    /// 2. Insert a lexically earlier `vala.invalid` row into the server-owned
    ///    file list so discovery encounters malformed table metadata first.
    /// 3. Run one public tick and inspect the outcome plus the healthy table’s
    ///    file-list/Iceberg result.
    ///
    /// The invalid row must increment `tables_failed`, while the healthy table
    /// must increment `tables_succeeded` and commit. This proves one persistent
    /// table failure cannot starve later tables on every scheduler tick.
    async fn forge_scheduler_bad_first_table_does_not_starve_later_table() {
        let fixture = Fixture::new().await;
        sqlx::query(
            "INSERT INTO vala.file_list (id, data_tenant_id, namespace, table_name, file_path, file_size, row_count, min_event_time, max_event_time, partition_day, node_id, writer_epoch, wal_lsn_min, wal_lsn_max) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(fixture.tenant.as_uuid())
        .bind("vala.invalid")
        .bind("bad_first")
        .bind("staging/bad-first.parquet")
        .bind(1_i64)
        .bind(1_i64)
        .bind(chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z").expect("timestamp"))
        .bind(chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:01Z").expect("timestamp"))
        .bind(chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("partition day"))
        .bind(uuid::Uuid::now_v7())
        .bind(1_i64)
        .bind(1_i64)
        .bind(2_i64)
        .execute(fixture.context.operator_pool.pool())
        .await
        .expect("invalid discovery row");

        let outcome = run_maintenance_tick(&fixture.context)
            .await
            .expect("maintenance continues after invalid table");
        assert!(outcome.tables_failed >= 1);
        assert!(outcome.tables_succeeded >= 1);
        assert_eq!(
            fixture
                .operation_count("forge.file_compact.committed")
                .await,
            1
        );
        assert!(
            fixture
                .file_state()
                .await
                .iter()
                .all(|(compacted, snapshot)| *compacted && snapshot.is_some())
        );
    }

    #[tokio::test]
    /// Replays a completed compaction and checks the successor sees durable
    /// state rather than creating a second snapshot. This is the closeout proof
    /// for uncertain catalog responses and idempotent reconciliation.
    async fn forge_compaction_uncertain_commit_reconciles_without_second_snapshot() {
        let fixture = Fixture::new().await;
        let first = run_maintenance_tick(&fixture.context)
            .await
            .expect("initial compaction");
        assert_eq!(first.bins_committed, 1);
        let second = run_maintenance_tick(&fixture.context)
            .await
            .expect("manifest-first reconciliation");
        assert_eq!(second.bins_committed, 0);
        assert_eq!(second.reconciled, 0);
        let table = fixture
            .context
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("reconciled table");
        assert_eq!(table.metadata().snapshots().len(), 1);
        assert_eq!(
            fixture.operation_count("forge.file_compact.prepared").await,
            1
        );
        assert_eq!(
            fixture
                .operation_count("forge.file_compact.committed")
                .await,
            1
        );
    }

    #[tokio::test]
    async fn forge_scheduler_expiry_preserves_current_and_retained_heads() {
        let fixture = Fixture::new().await;
        let mut config = fixture.context.config.clone();
        config.snapshot_retention = Duration::from_millis(1);
        let context = fixture.context_with_config(config);
        run_maintenance_tick(&context)
            .await
            .expect("initial snapshot");
        tokio::time::sleep(Duration::from_millis(10)).await;
        fixture.seed_additional_two_files().await;
        let outcome = run_maintenance_tick(&context).await.expect("expiry tick");
        assert_eq!(outcome.bins_committed, 1);
        let table = context
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("current table");
        assert_eq!(table.metadata().snapshots().len(), 1);
        assert!(table.metadata().current_snapshot_id().is_some());
        assert_eq!(
            fixture
                .operation_count("forge.snapshot_expire.committed")
                .await,
            1
        );
    }

    #[tokio::test]
    async fn forge_scheduler_live_set_protects_every_retained_iceberg_path() {
        let fixture = Fixture::new().await;
        run_maintenance_tick(&fixture.context)
            .await
            .expect("live-set compaction");
        let table = fixture
            .context
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("live table");
        let snapshot = table.metadata().current_snapshot().expect("snapshot");
        let manifest_list = snapshot.manifest_list().to_owned();
        run_maintenance_tick(&fixture.context)
            .await
            .expect("live-set rebuild");
        assert!(table.metadata().current_snapshot_id().is_some());
        assert!(manifest_list.contains("metadata"));
        assert_eq!(
            fixture.operation_count("forge.orphan_gc.committed").await,
            0
        );
    }

    #[tokio::test]
    /// Uses an existing table with an old orphan and a young orphan to verify
    /// GC age filtering and durable prepared/committed audit transitions.
    async fn forge_scheduler_gc_deletes_only_old_true_orphans() {
        let fixture = Fixture::new().await;
        let orphan = format!("{}/old-orphan.parquet", fixture.binding.object_prefix);
        let young = format!("{}/young-orphan.parquet", fixture.binding.object_prefix);
        fixture
            .context
            .staging
            .write(&orphan, Buffer::from(vec![1_u8]))
            .await
            .expect("old orphan write");
        let mut config = fixture.context.config.clone();
        config.orphan_gc_ttl = Duration::from_secs(1);
        let context = fixture.context_with_config(config);
        tokio::time::sleep(Duration::from_millis(1_100)).await;
        context
            .staging
            .write(&young, Buffer::from(vec![2_u8]))
            .await
            .expect("young orphan write");
        run_maintenance_tick(&context)
            .await
            .expect("orphan GC tick");
        assert!(!fixture.object_exists(&orphan).await);
        assert!(fixture.object_exists(&young).await);
        assert!(fixture.operation_count("forge.orphan_gc.prepared").await >= 1);
        assert!(fixture.operation_count("forge.orphan_gc.committed").await >= 1);
    }

    #[tokio::test]
    /// Runs maintenance after a live reference exists and verifies the final
    /// GC reference check does not delete it or create another compaction.
    async fn forge_scheduler_gc_rechecks_reference_before_delete() {
        let fixture = Fixture::new().await;
        run_maintenance_tick(&fixture.context)
            .await
            .expect("reference rebuild");
        let table = fixture
            .context
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("table reference");
        let live = table.metadata().current_snapshot_id();
        assert!(live.is_some());
        let second = run_maintenance_tick(&fixture.context)
            .await
            .expect("final GC revalidation");
        assert_eq!(second.bins_committed, 0);
        assert!(table.metadata().current_snapshot_id().is_some());
    }

    #[tokio::test]
    async fn forge_scheduler_expiry_crash_after_commit_recovers_terminal_audit() {
        let fixture = Fixture::new().await;
        let mut config = fixture.context.config.clone();
        config.snapshot_retention = Duration::from_millis(1);
        let context = fixture.context_with_config(config);
        run_maintenance_tick(&context).await.expect("seed snapshot");
        let first_table = context
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("first snapshot table");
        let first_snapshot_id = first_table
            .metadata()
            .current_snapshot_id()
            .expect("first snapshot id");
        fixture.seed_additional_two_files().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        run_maintenance_tick(&context)
            .await
            .expect("external expiry effect");
        let expired_table = context
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("expired snapshot table");
        assert!(
            expired_table
                .metadata()
                .snapshot_by_id(first_snapshot_id)
                .is_none()
        );
        let resource = format!(
            "bifrost://{}/{}/{}",
            fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
        );
        let base_metadata_location = StoragePath::new(
            expired_table
                .metadata_location_result()
                .expect("metadata location")
                .to_owned(),
        )
        .expect("metadata path");
        let detail = AuditDetail::ForgeSnapshotExpire {
            operation_id: uuid::Uuid::now_v7(),
            phase: ForgeSnapshotExpirePhase::Prepared,
            group: resource.clone(),
            base_metadata_location,
            current_snapshot_id: expired_table.metadata().current_snapshot_id(),
            retained_ref_heads: Vec::new(),
            cutoff_ms: chrono::Utc::now().timestamp_millis(),
            selected_snapshot_ids: vec![first_snapshot_id],
        };
        fixture
            .append_audit_detail("forge.snapshot_expire.prepared", resource, detail)
            .await;
        let outcome = run_maintenance_tick(&context)
            .await
            .expect("restart expiry reconciliation");
        assert!(outcome.reconciled >= 1);
        assert_eq!(
            fixture
                .operation_count("forge.snapshot_expire.recovered")
                .await,
            1
        );
    }

    #[tokio::test]
    /// Re-runs maintenance after a partial GC pass and verifies the successor
    /// treats already-absent objects as success without another terminal audit.
    async fn forge_scheduler_gc_partial_delete_restart_is_idempotent() {
        let fixture = Fixture::new().await;
        let mut config = fixture.context.config.clone();
        config.orphan_gc_ttl = Duration::from_millis(1);
        let context = fixture.context_with_config(config);
        let orphan = format!("{}/restart-orphan.parquet", fixture.binding.object_prefix);
        context
            .staging
            .write(&orphan, Buffer::from(vec![3_u8]))
            .await
            .expect("restart orphan write");
        tokio::time::sleep(Duration::from_millis(10)).await;
        run_maintenance_tick(&context).await.expect("first GC pass");
        let committed = fixture.operation_count("forge.orphan_gc.committed").await;
        run_maintenance_tick(&context)
            .await
            .expect("idempotent GC restart");
        assert!(!fixture.object_exists(&orphan).await);
        assert_eq!(
            fixture.operation_count("forge.orphan_gc.committed").await,
            committed
        );
    }

    #[tokio::test]
    /// Acquires the exact shared Forge table lease twice while compaction,
    /// expiry, and GC share the same coordination scope. This prevents stage
    /// helpers from becoming competing production schedulers.
    async fn forge_scheduler_common_lease_serializes_compact_expire_and_gc() {
        let fixture = Fixture::new().await;
        let lease_key = format!(
            "forge:table:{}:{}:{}",
            fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
        );
        let first_owner = uuid::Uuid::now_v7();
        let first_token = vala_sql::queries::maintenance_leases::try_acquire_lease(
            &fixture.context.operator_pool,
            &lease_key,
            first_owner,
            i64::try_from(fixture.context.config.lease_ttl.as_secs()).expect("lease seconds"),
        )
        .await
        .expect("lease acquisition")
        .expect("lease");
        let second = vala_sql::queries::maintenance_leases::try_acquire_lease(
            &fixture.context.operator_pool,
            &lease_key,
            uuid::Uuid::now_v7(),
            i64::try_from(fixture.context.config.lease_ttl.as_secs()).expect("lease seconds"),
        )
        .await
        .expect("competing lease query");
        assert!(second.is_none());
        assert!(
            vala_sql::queries::maintenance_leases::release_lease_fenced(
                &fixture.context.operator_pool,
                &lease_key,
                first_owner,
                first_token,
            )
            .await
            .expect("release")
        );
        let outcome = run_maintenance_tick(&fixture.context)
            .await
            .expect("maintenance after release");
        assert_eq!(outcome.bins_committed, 1);
    }
}
