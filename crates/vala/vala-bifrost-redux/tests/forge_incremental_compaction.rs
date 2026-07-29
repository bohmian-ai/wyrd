//! Integration proof for bounded Forge rewrites and Iceberg metadata.

mod pg_tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;

    use arrow::array::{
        FixedSizeBinaryBuilder, Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray,
    };
    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::execution::memory_pool::GreedyMemoryPool;
    use iceberg::Catalog;
    use iceberg::spec::{NullOrder, SortDirection, Transform};
    use iceberg::transaction::{ApplyTransactionAction, Transaction};
    use opendal::services::Fs;
    use opendal::{Buffer, Operator};
    use parquet::arrow::ArrowWriter;
    use secrecy::ExposeSecret;
    use tempfile::TempDir;
    use vala_bifrost_redux::catalog::{
        BifrostCatalog, CreateTableRequest, TableRef, TenantTableBinding,
    };
    use vala_bifrost_redux::forge::{
        Forge, ForgeBuildConfig, ForgeConfig, ForgeObjectStore, ForgeRewriteRuntime,
    };
    use vala_bifrost_redux::maintenance::staging_file_channel;
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_bifrost_redux::schema::with_managed_columns;
    use vala_sql::OperatorPool;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
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

        /// Build a real catalog, staging store, and Forge owner.
        ///
        /// # Panics
        ///
        /// Panics when the embedded database, catalog, or fixture storage
        /// cannot be initialized; these are test-environment invariants.
        async fn new() -> Self {
            let pg = PgFixture::start().await.expect("postgres fixture");
            let tenant = pg.data_tenant_id();
            let binding = TenantTableBinding::resolve((
                tenant,
                TableRef::new(BifrostNamespace::Bifrost, "incremental_rows"),
            ))
            .expect("binding");
            let root = tempfile::tempdir().expect("warehouse root");
            let staging = Arc::new(
                Operator::new(Fs::default().root(root.path().to_str().expect("root")))
                    .expect("operator")
                    .finish(),
            );
            let catalog = Self::build_catalog(&pg, &root, &binding).await;
            let operator_pool = OperatorPool::from(pg.platform_admin_pool().clone());
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
            let fixture = Self {
                pg,
                tenant,
                binding,
                operator_pool,
                catalog,
                staging,
                _root: root,
                reads,
                forge,
            };
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

        /// Seed Parquet objects and durable file-list rows, optionally aged.
        ///
        /// # Panics
        ///
        /// Panics when fixture encoding, object writes, or durable inserts
        /// fail; each operation is required to establish the test invariant.
        async fn seed_files(&self, count: usize, aged: bool) {
            let schema = Self::schema();
            let day = chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("day");
            let base = chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
                .expect("time")
                .timestamp_micros();
            let mut rows = Vec::new();
            for index in 0..count {
                let index = i64::try_from(index).expect("file index fits i64");
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
                        Arc::new(StringArray::from(vec![None::<&str>; row_count_usize])),
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
    }

    /// Validate the field metrics emitted for one committed Parquet file.
    fn assert_output_metrics(data_file: &iceberg::spec::DataFile) {
        assert_eq!(
            data_file.file_format(),
            iceberg::spec::DataFileFormat::Parquet
        );
        assert!(data_file.file_size_in_bytes() > 0);
        let expected = (1..=9).collect();
        let expected_bounds = [1, 5, 6, 7, 8, 9].into_iter().collect();
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
            data_file.record_count() * 3,
            "the production schema carries three nullable correlation columns"
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
}
