//! Integration proof for bounded Forge rewrites and Iceberg metadata.

mod pg_tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;

    use arrow::array::{
        FixedSizeBinaryBuilder, Int32Array, Int64Array, RecordBatch, StringArray,
        TimestampMicrosecondArray,
    };
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use datafusion::execution::memory_pool::GreedyMemoryPool;
    use iceberg::Catalog;
    use iceberg::spec::{
        DataContentType, DataFile, DataFileBuilder, DataFileFormat, Datum, Literal, NullOrder,
        PrimitiveType, SortDirection, Struct, Transform, Type,
    };
    use iceberg::transaction::{AddColumn, ApplyTransactionAction, Transaction};
    use opendal::services::Fs;
    use opendal::{Buffer, Operator};
    use parquet::arrow::ArrowWriter;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use secrecy::ExposeSecret;
    use tempfile::TempDir;
    use vala_bifrost_redux::catalog::{
        BifrostCatalog, CreateTableRequest, TableRef, TenantTableBinding,
    };
    use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
    use vala_bifrost_redux::forge::{
        Forge, ForgeBuildConfig, ForgeConfig, ForgeObjectStore, ForgeRewriteRuntime,
    };
    use vala_bifrost_redux::maintenance::staging_file_channel;
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_bifrost_redux::schema::{SchemaFingerprint, with_managed_columns};
    use vala_bifrost_redux::scribe::memory::BifrostMemoryGovernor;
    use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
    use vala_bifrost_redux::scribe::wal::{WalConfig, WalWriter};
    use vala_bifrost_redux::scribe::{
        ScribeBuildConfig, ScribeExecutionPools, ScribeImpl, ScribeIngressCpuPool,
        ScribeLaneConfig, ScribePersistenceConfig, ScribePersistenceCpuPool, ScribeWalIoPool,
    };
    use vala_sql::OperatorPool;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::{RUN_ID, WYRD_EVENT_TIME};
    use wyrd_storage::BackendConfig;

    /// Counts ranged source reads while delegating bytes to a local operator.
    #[derive(Debug)]
    struct InstrumentedStore {
        /// Fixture-backed object operator.
        operator: Arc<Operator>,
        /// Number of whole-object reads.
        whole_reads: Arc<AtomicUsize>,
        /// Number of ranged reads.
        ranged_reads: Arc<AtomicUsize>,
        /// Peak concurrent ranged reads.
        active_reads: Arc<AtomicUsize>,
        /// Peak observed concurrency.
        peak_reads: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ForgeObjectStore for InstrumentedStore {
        /// Reject whole-object reads so the rewrite cannot hide an unbounded path.
        async fn read(&self, _path: &str) -> opendal::Result<Buffer> {
            self.whole_reads.fetch_add(1, Ordering::Relaxed);
            Err(opendal::Error::new(
                opendal::ErrorKind::Unsupported,
                "whole-object read forbidden",
            ))
        }

        /// Perform a native local range request and record bounded concurrency.
        async fn read_range(
            &self,
            path: &str,
            range: std::ops::Range<u64>,
        ) -> opendal::Result<Buffer> {
            self.ranged_reads.fetch_add(1, Ordering::Relaxed);
            let active = self.active_reads.fetch_add(1, Ordering::AcqRel) + 1;
            self.peak_reads.fetch_max(active, Ordering::AcqRel);
            let result = self.operator.reader(path).await?.read(range).await;
            self.active_reads.fetch_sub(1, Ordering::AcqRel);
            result
        }

        /// List fixture objects for cleanup and orphan checks.
        async fn list(&self, prefix: &str) -> opendal::Result<Vec<opendal::Entry>> {
            self.operator.list_with(prefix).recursive(true).await
        }

        /// Stat one fixture object.
        async fn stat(&self, path: &str) -> opendal::Result<opendal::Metadata> {
            self.operator.stat(path).await
        }

        /// Delete one rewrite-owned object.
        async fn delete(&self, path: &str) -> opendal::Result<()> {
            self.operator.delete(path).await
        }
    }

    /// Real dependencies used by all incremental Forge proofs.
    struct Fixture {
        /// Embedded Postgres fixture.
        pg: PgFixture,
        /// Tenant and physical table identity.
        tenant: DataTenantId,
        /// Registered table binding.
        binding: TenantTableBinding,
        /// SQL operator pool.
        operator_pool: OperatorPool,
        /// Iceberg catalog used for output manifest assertions.
        catalog: Arc<dyn Catalog>,
        /// Staging operator.
        staging: Arc<Operator>,
        /// Temporary warehouse and spill root retained for object lifetime.
        _root: TempDir,
        /// Source-read instrumentation.
        reads: Arc<InstrumentedStore>,
        /// Forge handle under test.
        forge: Forge,
    }

    impl Fixture {
        /// Register the Forge table through the production Bifrost catalog.
        ///
        /// # Panics
        ///
        /// Panics when production catalog registration or test-only target
        /// configuration fails because those are fixture invariants.
        async fn build_catalog(
            pg: &PgFixture,
            root: &TempDir,
            binding: &TenantTableBinding,
        ) -> Arc<dyn Catalog> {
            let backend = BackendConfig::Local {
                root: root.path().to_path_buf(),
            };
            let catalog = BifrostCatalog::new(
                pg.catalog_dsn().expose_secret(),
                &backend,
                pg.vala_postgres().clone(),
            )
            .await
            .expect("production catalog");
            catalog
                .create_table(CreateTableRequest {
                    table: binding.table_ref.clone(),
                    user_fields: vec![Field::new("value", DataType::Int64, false)],
                    tenant: binding.tenant,
                    audit: None,
                })
                .await
                .expect("production table registration");
            let iceberg_catalog = catalog.iceberg_catalog();
            let table = iceberg_catalog
                .load_table(&binding.table_ident())
                .await
                .expect("registered table");
            Self::assert_physical_recipe(&table);
            let action = Transaction::new(&table)
                .update_table_properties()
                .set("write.target-file-size-bytes".to_owned(), "3600".to_owned());
            ApplyTransactionAction::apply(action, Transaction::new(&table))
                .expect("target property action")
                .commit(iceberg_catalog.as_ref())
                .await
                .expect("target property commit");
            let configured = iceberg_catalog
                .load_table(&binding.table_ident())
                .await
                .expect("configured table");
            Self::assert_physical_recipe(&configured);
            iceberg_catalog
        }

        /// Assert that registration emitted Forge's fixed Iceberg physical recipe.
        fn assert_physical_recipe(table: &iceberg::table::Table) {
            let schema = table.metadata().current_schema();
            let event_time_id = schema
                .field_by_name("wyrd_event_time")
                .expect("event time field")
                .id;
            let tenant_id = schema
                .field_by_name("data_tenant_id")
                .expect("tenant field")
                .id;
            let row_ordinal = schema
                .field_by_name("wyrd_row_ordinal")
                .expect("row identity field");
            assert_eq!(
                row_ordinal.field_type,
                Box::new(Type::Primitive(PrimitiveType::Int))
            );
            assert!(row_ordinal.required);
            let partition_fields = table.metadata().default_partition_spec().fields();
            assert_eq!(partition_fields.len(), 1);
            assert_eq!(partition_fields[0].source_id, event_time_id);
            assert_eq!(partition_fields[0].name, "wyrd_event_time_day");
            assert_eq!(partition_fields[0].transform, Transform::Day);

            let sort_fields = &table.metadata().default_sort_order().fields;
            assert_eq!(sort_fields.len(), 2);
            for (field, source_id) in sort_fields.iter().zip([tenant_id, event_time_id]) {
                assert_eq!(field.source_id, source_id);
                assert_eq!(field.transform, Transform::Identity);
                assert_eq!(field.direction, SortDirection::Ascending);
                assert_eq!(field.null_order, NullOrder::Last);
            }
        }

        /// Build a real catalog, staging store, and Forge owner without source files.
        ///
        /// # Panics
        ///
        /// Panics when the embedded database, catalog, or fixture storage
        /// cannot be initialized; these are test-environment invariants.
        async fn new_unseeded() -> Self {
            let pg = PgFixture::start().await.expect("postgres fixture");
            let tenant = pg.data_tenant_id();
            let table_name = format!("incremental_rows_{}", uuid::Uuid::now_v7().simple());
            let binding = TenantTableBinding::resolve((
                tenant,
                TableRef::new(BifrostNamespace::Bifrost, table_name),
            ))
            .expect("binding");
            let root = tempfile::tempdir().expect("warehouse root");
            let staging = Arc::new(
                Operator::new(Fs::default().root(root.path().to_str().expect("root")))
                    .expect("operator")
                    .finish(),
            );
            let catalog = Self::build_catalog(&pg, &root, &binding).await;
            let operator_pool = pg.operator_pool().clone();
            let whole_reads = Arc::new(AtomicUsize::new(0));
            let reads = Arc::new(InstrumentedStore {
                operator: Arc::clone(&staging),
                whole_reads: Arc::clone(&whole_reads),
                ranged_reads: Arc::new(AtomicUsize::new(0)),
                active_reads: Arc::new(AtomicUsize::new(0)),
                peak_reads: Arc::new(AtomicUsize::new(0)),
            });
            let (_publisher, hints) = staging_file_channel(16).expect("hint channel");
            let config = ForgeConfig {
                max_concurrent_reads: 2,
                ..ForgeConfig::default()
            };
            let runtime = ForgeRewriteRuntime::new(
                // DataFusion 53 reserves 10 MiB for an external-sort merge.
                // Leave that reservation available, then make the fixture
                // exceed the remaining bounded pool with real Arrow batches.
                Arc::new(GreedyMemoryPool::new(16 * 1024 * 1024)),
                &root.path().join("spill"),
                config.spill_limit_bytes,
            )
            .expect("runtime");
            let forge = Forge::new(ForgeBuildConfig {
                vala: pg.vala_postgres().clone(),
                operator_pool: operator_pool.clone(),
                catalog: Arc::clone(&catalog),
                staging: Arc::clone(&staging),
                object_store: Arc::clone(&reads) as Arc<dyn ForgeObjectStore>,
                rewrite_runtime: runtime,
                hints,
                config,
                maintenance_interval: Duration::from_millis(10),
            })
            .expect("forge");
            Self {
                pg,
                tenant,
                binding,
                operator_pool,
                catalog,
                staging,
                _root: root,
                reads,
                forge,
            }
        }

        /// Build the standard Forge fixture with four staged source files.
        ///
        /// # Panics
        ///
        /// Panics when fixture construction or source seeding fails.
        async fn new() -> Self {
            let fixture = Self::new_unseeded().await;
            fixture.seed_files(4, false).await;
            fixture
        }

        /// Return the stable three-column staging schema.
        fn schema() -> Schema {
            Schema::new(with_managed_columns(vec![Field::new(
                "value",
                DataType::Int64,
                false,
            )]))
        }

        /// Builds the real bounded Scribe used to seal incremental generations.
        ///
        /// # Panics
        ///
        /// Panics when WAL, lane, or Scribe construction fails.
        fn generation_scribe(&self) -> (tempfile::TempDir, Arc<ScribeImpl>) {
            let wal_root = tempfile::tempdir().expect("WAL root");
            let node_id = uuid::Uuid::now_v7();
            let wal = Arc::new(
                WalWriter::new(
                    wal_root.path(),
                    *node_id.as_bytes(),
                    1,
                    WalConfig::default(),
                )
                .expect("WAL writer"),
            );
            let memory = BifrostMemoryGovernor::new(512 * 1024 * 1024).expect("memory governor");
            let lanes = ScribeLaneConfig {
                ingress_cpu_threads: 1,
                persistence_cpu_threads: 1,
                wal_io_threads: 1,
            };
            let pools = ScribeExecutionPools::new(
                ScribeIngressCpuPool::new_with_capacity(lanes.ingress_cpu_threads, 16),
                ScribePersistenceCpuPool::new_with_capacity(lanes.persistence_cpu_threads, 16),
                ScribeWalIoPool::new_with_capacity(lanes.wal_io_threads, 16),
            );
            let (publisher, _inbox) = staging_file_channel(16).expect("Scribe hint channel");
            let scribe = Arc::new(ScribeImpl::new_with_execution_pools(ScribeBuildConfig {
                operator: Arc::clone(&self.staging),
                wal,
                stream: StreamIdentity::new(NodeId::new(node_id), WriterEpoch::new(1)),
                admission: vala_bifrost_redux::scribe::admission::AdmissionConfig::default(),
                coordination_runtime: tokio::runtime::Handle::current(),
                execution_pools: pools,
                persistence: Some(ScribePersistenceConfig::new(
                    Arc::new(self.pg.vala_postgres().clone()),
                    16,
                    1,
                )),
                memory_budget: Some(memory.scribe_budget()),
                staging_file_publisher: Some(publisher),
            }));
            (wal_root, scribe)
        }

        /// Writes four generations through Scribe's WAL, memtable, seal, and Parquet path.
        ///
        /// Returns the exact logical value and batch-local ordinal pairs expected
        /// after Forge rewrites the sealed files.
        ///
        /// # Panics
        ///
        /// Panics when Scribe construction, admission, persistence, or fixture
        /// SQL cannot establish the sealed input state.
        async fn seal_row_identity_generations(&self) -> Vec<(i64, i32)> {
            let (_wal_root, scribe) = self.generation_scribe();
            scribe.replay_wal_async().await.expect("empty replay");
            let principal = Principal::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                PrincipalKind::User,
                self.tenant,
                Vec::new(),
                PermissionSet::new(),
            );
            let event_time = chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
                .expect("event time")
                .timestamp_micros();
            let mut expected = Vec::new();
            for generation in 0_i64..4 {
                let values = (0_i64..3)
                    .map(|row| generation * 10 + row)
                    .collect::<Vec<_>>();
                let row_count = values.len();
                expected.extend(values.iter().enumerate().map(|(ordinal, value)| {
                    (
                        *value,
                        i32::try_from(ordinal).expect("test ordinal fits i32"),
                    )
                }));
                let schema = Arc::new(Schema::new(vec![
                    Field::new(
                        WYRD_EVENT_TIME,
                        DataType::Timestamp(TimeUnit::Microsecond, None),
                        false,
                    ),
                    Field::new("value", DataType::Int64, false),
                    Field::new(RUN_ID, DataType::Utf8, true),
                ]));
                let rows = RecordBatch::try_new(
                    schema,
                    vec![
                        Arc::new(TimestampMicrosecondArray::from(vec![
                            event_time + generation;
                            row_count
                        ])),
                        Arc::new(Int64Array::from(values)),
                        Arc::new(StringArray::from(vec![None::<String>; row_count])),
                    ],
                )
                .expect("Scribe source batch");
                scribe
                    .append_durable(ScribeAppend {
                        principal: principal.clone(),
                        table: self.binding.table_ref.clone(),
                        schema_fingerprint: SchemaFingerprint::from_arrow_schema(
                            rows.schema().as_ref(),
                        ),
                        request_id: RequestId::now_v7(),
                        batch_id: uuid::Uuid::now_v7(),
                        measured_wire_bytes: rows.get_array_memory_size(),
                        rows,
                    })
                    .await
                    .expect("Scribe append");
                scribe
                    .flush_writable_for_test()
                    .await
                    .expect("generation flush");
                let deadline = std::time::Instant::now() + Duration::from_secs(10);
                loop {
                    let stats = scribe.memtable_stats().expect("memtable stats");
                    if scribe.persistence_queue_depth_for_test() == 0
                        && stats.pending_generations == 0
                    {
                        break;
                    }
                    assert!(
                        std::time::Instant::now() < deadline,
                        "sealed generation did not publish"
                    );
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
            scribe
                .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
                .await;
            sqlx::query(
                "UPDATE vala.file_list
                    SET created_at = now() - interval '3 minutes'
                  WHERE data_tenant_id = $1
                    AND namespace = $2
                    AND table_name = $3",
            )
            .bind(self.tenant.as_uuid())
            .bind(&self.binding.logical_namespace)
            .bind(&self.binding.table_name)
            .execute(self.operator_pool.pool())
            .await
            .expect("sealed inputs age for Forge");
            expected
        }

        /// Reads every live Iceberg output and returns its logical value/ordinal pairs.
        ///
        /// # Panics
        ///
        /// Panics when manifest or Parquet output cannot be decoded through the
        /// fixture's production catalog and object store.
        async fn live_row_identity(&self) -> Vec<(i64, i32)> {
            let table = self
                .catalog
                .load_table(&self.binding.table_ident())
                .await
                .expect("committed table");
            let location = table.metadata().location().trim_end_matches('/');
            let snapshot = table.metadata().current_snapshot().expect("snapshot");
            let manifests = table
                .manifest_list_reader(snapshot)
                .load()
                .await
                .expect("manifest list");
            let mut identity = Vec::new();
            for manifest_file in manifests.entries() {
                let manifest = manifest_file
                    .load_manifest(table.file_io())
                    .await
                    .expect("manifest");
                for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
                    let catalog_path = entry.data_file().file_path();
                    let relative = catalog_path
                        .strip_prefix(&format!("{location}/"))
                        .expect("catalog path remains below table location");
                    let object_path = format!(
                        "{}/{relative}",
                        self.binding.object_prefix.trim_end_matches('/')
                    );
                    let bytes = self
                        .staging
                        .read(&object_path)
                        .await
                        .expect("Forge output reads");
                    let reader = ParquetRecordBatchReaderBuilder::try_new(bytes.to_bytes())
                        .expect("Parquet reader")
                        .build()
                        .expect("Parquet batches");
                    for batch in reader {
                        let batch = batch.expect("Parquet batch");
                        let values = batch
                            .column_by_name("value")
                            .expect("logical value persists")
                            .as_any()
                            .downcast_ref::<Int64Array>()
                            .expect("value remains Int64");
                        let ordinals = batch
                            .column_by_name("wyrd_row_ordinal")
                            .expect("row identity persists")
                            .as_any()
                            .downcast_ref::<Int32Array>()
                            .expect("row identity remains Int32");
                        identity.extend(
                            (0..batch.num_rows())
                                .map(|row| (values.value(row), ordinals.value(row))),
                        );
                    }
                }
            }
            identity
        }

        /// Seed Parquet objects and durable file-list rows, optionally aged.
        ///
        /// # Panics
        ///
        /// Panics when fixture encoding, object writes, or durable inserts
        /// fail; each operation is required to establish the test invariant.
        async fn seed_files(&self, count: usize, aged: bool) {
            self.seed_files_at(0, count, aged).await;
        }

        /// Seed uniquely named Parquet objects and durable file-list rows.
        ///
        /// The caller supplies `start` so one catalog fixture can retain files
        /// from multiple manifest-writing stages without replacing an earlier
        /// object path.
        ///
        /// # Panics
        ///
        /// Panics when fixture encoding, object writes, or durable inserts
        /// fail; each operation is required to establish the test invariant.
        async fn seed_files_at(&self, start: i64, count: usize, aged: bool) {
            let schema = Self::schema();
            let day = chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("day");
            let base = chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
                .expect("time")
                .timestamp_micros();
            let principal_id = PrincipalId::new(uuid::Uuid::nil()).to_string();
            let mut rows = Vec::new();
            for index in 0..count {
                let index = start
                    .checked_add(i64::try_from(index).expect("file index fits i64"))
                    .expect("fixture file index fits i64");
                let row_count = 100_000_i64;
                let row_count_usize = usize::try_from(row_count).expect("row count fits usize");
                let mut batch_ids = FixedSizeBinaryBuilder::with_capacity(row_count_usize, 16);
                for _ in 0..row_count_usize {
                    batch_ids
                        .append_value([0_u8; 16])
                        .expect("fixed batch identifier");
                }
                let batch = RecordBatch::try_new(
                    Arc::new(schema.clone()),
                    vec![
                        Arc::new(Int64Array::from(
                            (0..row_count).map(|row| index + row).collect::<Vec<_>>(),
                        )),
                        Arc::new(StringArray::from(vec![None::<&str>; row_count_usize])),
                        Arc::new(StringArray::from(vec![None::<&str>; row_count_usize])),
                        Arc::new(StringArray::from(vec![
                            principal_id.as_str();
                            row_count_usize
                        ])),
                        Arc::new(StringArray::from(vec!["request"; row_count_usize])),
                        Arc::new(
                            TimestampMicrosecondArray::from(
                                (0..row_count)
                                    .map(|row| base + index + row)
                                    .collect::<Vec<_>>(),
                            )
                            .with_timezone("UTC"),
                        ),
                        Arc::new(
                            TimestampMicrosecondArray::from(
                                (0..row_count)
                                    .map(|row| base + index + row)
                                    .collect::<Vec<_>>(),
                            )
                            .with_timezone("UTC"),
                        ),
                        Arc::new(batch_ids.finish()),
                        Arc::new(Int32Array::from_iter_values(
                            0..i32::try_from(row_count).expect("test row count fits i32"),
                        )),
                        Arc::new(StringArray::from(vec![
                            self.tenant.to_string();
                            row_count_usize
                        ])),
                    ],
                )
                .expect("batch");
                let mut bytes = Vec::new();
                let mut writer =
                    ArrowWriter::try_new(&mut bytes, batch.schema(), None).expect("writer");
                writer.write(&batch).expect("write");
                writer.close().expect("close");
                let path = format!("{}/input-{index}.parquet", self.binding.object_prefix);
                self.staging
                    .write(&path, Buffer::from(bytes))
                    .await
                    .expect("object");
                rows.push((path, index, row_count));
            }
            let mut conn = vala_sql::TenantConn::acquire(self.pg.app_pool(), self.tenant)
                .await
                .expect("tenant conn");
            for (path, index, row_count) in rows {
                sqlx::query("INSERT INTO vala.file_list (id,data_tenant_id,namespace,table_name,file_path,file_size,row_count,min_event_time,max_event_time,partition_day,node_id,writer_epoch,wal_lsn_min,wal_lsn_max) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)").bind(uuid::Uuid::now_v7()).bind(self.tenant.as_uuid()).bind(&self.binding.logical_namespace).bind(&self.binding.table_name).bind(path).bind(100_i64).bind(row_count).bind(chrono::DateTime::from_timestamp_micros(base + index).expect("min")).bind(chrono::DateTime::from_timestamp_micros(base + index + row_count).expect("max")).bind(day).bind(uuid::Uuid::now_v7()).bind(1_i64).bind(index * 2 + 1).bind(index * 2 + 2).execute(&mut **conn.transaction()).await.expect("file list");
            }
            conn.commit().await.expect("commit");
            if aged {
                sqlx::query("UPDATE vala.file_list SET created_at = now() - interval '3 minutes' WHERE data_tenant_id = $1").bind(self.tenant.as_uuid()).execute(self.operator_pool.pool()).await.expect("age");
            }
        }

        /// Append one existing fixture Parquet object under the table's current schema.
        ///
        /// This bypasses staging eligibility so manifest-schema tests construct
        /// a precise Iceberg snapshot without coupling to unrelated scheduler
        /// state in the shared integration database.
        ///
        /// # Panics
        ///
        /// Panics when fixture metadata cannot form a valid complete Iceberg
        /// data-file entry or the catalog rejects the test append.
        async fn append_seed_manifest(&self, table: &iceberg::table::Table, index: i64) {
            let object_path = format!("{}/input-{index}.parquet", self.binding.object_prefix);
            let catalog_path = format!(
                "{}/input-{index}.parquet",
                table.metadata().location().trim_end_matches('/')
            );
            let size = self
                .staging
                .stat(&object_path)
                .await
                .expect("seed object metadata")
                .content_length();
            let event_time_id = table
                .metadata()
                .current_schema()
                .field_by_name(WYRD_EVENT_TIME)
                .expect("event time field")
                .id;
            let base = chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
                .expect("time")
                .timestamp_micros();
            let day = chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("day");
            let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch");
            let partition_days = i32::try_from(day.signed_duration_since(epoch).num_days())
                .expect("fixture partition day fits i32");
            let data_file = DataFileBuilder::default()
                .content(DataContentType::Data)
                .file_path(catalog_path)
                .file_format(DataFileFormat::Parquet)
                .partition(Struct::from_iter([Some(Literal::int(partition_days))]))
                .record_count(100_000)
                .file_size_in_bytes(size)
                .lower_bounds(std::collections::HashMap::from([(
                    event_time_id,
                    Datum::try_from_bytes(&base.to_le_bytes(), PrimitiveType::Timestamptz)
                        .expect("lower event bound"),
                )]))
                .upper_bounds(std::collections::HashMap::from([(
                    event_time_id,
                    Datum::try_from_bytes(
                        &(base + 99_999).to_le_bytes(),
                        PrimitiveType::Timestamptz,
                    )
                    .expect("upper event bound"),
                )]))
                .sort_order_id(
                    i32::try_from(table.metadata().default_sort_order_id())
                        .expect("sort order ID fits i32"),
                )
                .partition_spec_id(table.metadata().default_partition_spec_id())
                .build()
                .expect("seed data file");
            let append = Transaction::new(table)
                .fast_append()
                .add_data_files([data_file]);
            ApplyTransactionAction::apply(append, Transaction::new(table))
                .expect("seed append action")
                .commit(self.catalog.as_ref())
                .await
                .expect("seed manifest append");
        }
    }

    /// Validate the field metrics emitted for one committed Parquet file.
    fn assert_output_metrics(data_file: &iceberg::spec::DataFile) {
        assert_eq!(
            data_file.file_format(),
            iceberg::spec::DataFileFormat::Parquet
        );
        assert!(data_file.file_size_in_bytes() > 0);
        let expected = (1..=10).collect();
        let expected_bounds = [1, 4, 5, 6, 7, 8, 9, 10].into_iter().collect();
        assert_eq!(
            data_file
                .value_counts()
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            expected
        );
        assert_eq!(
            data_file
                .column_sizes()
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            expected
        );
        assert_eq!(
            data_file
                .lower_bounds()
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            expected_bounds
        );
        assert_eq!(
            data_file
                .upper_bounds()
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            expected_bounds
        );
        assert!(
            data_file
                .value_counts()
                .values()
                .all(|count| *count == data_file.record_count())
        );
        assert_eq!(
            data_file
                .null_value_counts()
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            expected
        );
        assert_eq!(
            data_file.null_value_counts().values().copied().sum::<u64>(),
            data_file.record_count() * 2,
            "the production schema carries two nullable correlation columns"
        );
        assert!(data_file.nan_value_counts().is_empty());
        let offsets = data_file.split_offsets().expect("Parquet split offsets");
        assert!(!offsets.is_empty());
        assert!(offsets.windows(2).all(|pair| pair[0] < pair[1]));
    }

    /// Read the live manifest and validate every committed output file.
    async fn committed_output_totals(fixture: &Fixture) -> (usize, u64) {
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("committed table");
        let snapshot = table.metadata().current_snapshot().expect("snapshot");
        let expected_sort_order_id = i32::try_from(table.metadata().default_sort_order_id())
            .expect("fixture sort order ID fits i32");
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .expect("manifest list");
        let mut files = 0;
        let mut rows = 0;
        for manifest_file in manifests.entries() {
            let manifest = manifest_file
                .load_manifest(table.file_io())
                .await
                .expect("manifest");
            for entry in manifest.entries() {
                if entry.is_alive() {
                    files += 1;
                    let data_file = entry.data_file();
                    rows += data_file.record_count();
                    assert_output_metrics(data_file);
                    assert_eq!(data_file.sort_order_id(), Some(expected_sort_order_id));
                }
            }
        }
        (files, rows)
    }

    /// Real Parquet inputs spill, rotate, and conserve rows under one Forge operation.
    #[tokio::test]
    async fn streaming_rewrite_spills_and_commits_multiple_outputs() {
        let fixture = Fixture::new().await;
        // The default fixture seeds four files. Thirty-two more 100k-row files
        // make the sort exceed the bounded 16 MiB pool while keeping the
        // external journey deterministic and reasonably sized.
        fixture.seed_files(32, true).await;
        let outcome = fixture.forge.run_once().await.expect("rewrite");
        assert_eq!(outcome.bins_committed, 1, "outcome: {outcome:?}");
        assert!(
            outcome.spill_bytes > 0,
            "rewrite must spill under bounded memory"
        );
        assert!(outcome.spill_bytes <= ForgeConfig::default().spill_limit_bytes);
        assert!(outcome.outputs_committed >= 2);
        assert_eq!(outcome.input_rows, 3_200_000);
        assert_eq!(outcome.output_rows, 3_200_000);
        assert_eq!(fixture.reads.whole_reads.load(Ordering::Relaxed), 0);
        assert!(fixture.reads.ranged_reads.load(Ordering::Relaxed) > 0);
        assert!(fixture.reads.peak_reads.load(Ordering::Relaxed) <= 2);
        let (output_files, output_rows) = committed_output_totals(&fixture).await;
        assert!(output_files >= 2, "rotation must commit multiple outputs");
        assert_eq!(
            output_rows, 3_200_000,
            "rewrite must conserve every input row"
        );
    }

    /// Preserves logical values and immutable batch ordinals from Scribe seal through Forge.
    #[tokio::test]
    async fn seal_and_forge_preserve_row_identity() {
        let fixture = Fixture::new_unseeded().await;
        let mut expected = fixture.seal_row_identity_generations().await;
        let outcome = fixture.forge.run_once().await.expect("Forge rewrite");
        assert_eq!(outcome.bins_committed, 1, "outcome: {outcome:?}");
        assert_eq!(outcome.input_rows, expected.len() as u64);
        assert_eq!(outcome.output_rows, expected.len() as u64);

        let mut actual = fixture.live_row_identity().await;
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(actual, expected);
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
        let current_day = chrono::NaiveDate::from_ymd_opt(2026, 7, 15).expect("fixed day");

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
            "groups preserve the discovery and T2 input order"
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
                "T2 receives each group in candidate order"
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
