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
    use iceberg::spec::{
        DataContentType, DataFile, DataFileBuilder, DataFileFormat, Datum, Literal, NullOrder,
        PrimitiveType, SortDirection, Struct, Transform, Type,
    };
    use iceberg::transaction::{AddColumn, ApplyTransactionAction, Transaction};
    use opendal::services::Fs;
    use opendal::{Buffer, Operator};
    use parquet::arrow::ArrowWriter;
    use secrecy::ExposeSecret;
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;
    use vala_bifrost_redux::catalog::{
        BifrostCatalog, CreateTableRequest, TableRef, TenantTableBinding,
    };
    use vala_bifrost_redux::forge::{
        Forge, ForgeBuildConfig, ForgeConfig, ForgeLease, ForgeObjectStore, ForgeRewriteRuntime,
        IcebergRewriteGroup, forge_lease_key,
    };
    use vala_bifrost_redux::maintenance::staging_file_channel;
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_bifrost_redux::schema::with_managed_columns;
    use vala_sql::OperatorPool;
    use vala_sql::queries::forge_operations::ForgeOperations;
    use vala_sql::row_types::forge_operations::{ForgeOperationFamily, ForgeOperationTransition};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::WYRD_EVENT_TIME;
    use wyrd_spec::vala::api::{
        AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeCompactionPhase,
        ForgeOrphanGcPhase, ForgeSnapshotExpirePhase, StoragePath,
    };
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

    /// SQL projection and audit parity columns for one staging operation.
    type StagingParityRow = (String, i64, Option<i64>, bool, Option<bool>);

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
            Self::new_with_config(
                ForgeConfig {
                    max_concurrent_reads: 2,
                    ..ForgeConfig::default()
                },
                false,
                4,
                16 * 1024 * 1024,
            )
            .await
        }

        /// Build a real fixture with caller-selected maintenance thresholds.
        ///
        /// # Panics
        ///
        /// Panics when the embedded database, catalog, storage, or Forge owner
        /// cannot be initialized; these are test-environment invariants.
        async fn new_with_config(
            config: ForgeConfig,
            aged_inputs: bool,
            initial_file_count: usize,
            memory_pool_bytes: usize,
        ) -> Self {
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
            let runtime = ForgeRewriteRuntime::new(
                // DataFusion 53 reserves 10 MiB for an external-sort merge.
                // Leave that reservation available, then make the fixture
                // exceed the remaining bounded pool with real Arrow batches.
                Arc::new(GreedyMemoryPool::new(memory_pool_bytes)),
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
            fixture.seed_files(initial_file_count, aged_inputs).await;
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

    /// Assert the staging-fold projection references the exact prepared and terminal audits.
    ///
    /// # Panics
    ///
    /// Panics when the tenant query fails or the completed fixture operation
    /// does not have one parity-valid projection row.
    async fn assert_staging_transition_parity(fixture: &Fixture) {
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("staging parity tenant connection");
        let rows: Vec<StagingParityRow> = sqlx::query_as(
            r"SELECT state.phase,
                     state.prepared_audit_seq,
                     state.terminal_audit_seq,
                     state.prepared_detail = prepared.detail::jsonb,
                     state.current_detail = terminal.detail::jsonb
                FROM vala.forge_operation_state AS state
                JOIN vala.audit_outbox AS prepared
                  ON prepared.data_tenant_id = state.data_tenant_id
                 AND prepared.seq = state.prepared_audit_seq
                LEFT JOIN vala.audit_outbox AS terminal
                  ON terminal.data_tenant_id = state.data_tenant_id
                 AND terminal.seq = state.terminal_audit_seq
               WHERE state.data_tenant_id = wyrd.current_tenant()
                 AND state.family = 'staging_fold'",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("staging projection parity query");

        assert_eq!(rows.len(), 1, "one staging operation must be projected");
        let (phase, prepared_seq, terminal_seq, prepared_matches, terminal_matches) = &rows[0];
        assert_eq!(phase, "committed");
        assert!(*prepared_seq > 0);
        assert!(terminal_seq.is_some());
        assert!(*prepared_matches);
        assert_eq!(*terminal_matches, Some(true));
    }

    /// Assert every projected operation in one family references exact audit details.
    ///
    /// # Panics
    ///
    /// Panics when the tenant query fails, the family has no terminal row, or
    /// any projection sequence/detail differs from its audit evidence.
    async fn assert_terminal_family_parity(fixture: &Fixture, family: &str) {
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("family parity tenant connection");
        let rows: Vec<(String, bool, Option<bool>)> = sqlx::query_as(
            r"SELECT state.phase,
                     state.prepared_detail = prepared.detail::jsonb,
                     state.current_detail = terminal.detail::jsonb
                FROM vala.forge_operation_state AS state
                JOIN vala.audit_outbox AS prepared
                  ON prepared.data_tenant_id = state.data_tenant_id
                 AND prepared.seq = state.prepared_audit_seq
                LEFT JOIN vala.audit_outbox AS terminal
                  ON terminal.data_tenant_id = state.data_tenant_id
                 AND terminal.seq = state.terminal_audit_seq
               WHERE state.data_tenant_id = wyrd.current_tenant()
                 AND state.family = $1",
        )
        .bind(family)
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("family projection parity query");

        assert!(
            !rows.is_empty(),
            "{family} must project a terminal operation"
        );
        for (phase, prepared_matches, terminal_matches) in rows {
            assert!(
                matches!(phase.as_str(), "committed" | "recovered"),
                "{family} remained nonterminal: {phase}"
            );
            assert!(prepared_matches, "{family} prepared detail drifted");
            assert_eq!(
                terminal_matches,
                Some(true),
                "{family} terminal detail drifted"
            );
        }
    }

    /// Count one family projection and its operation-prefix audits.
    ///
    /// # Panics
    ///
    /// Panics when either tenant-scoped count query fails.
    async fn family_transition_counts(
        fixture: &Fixture,
        family: &str,
        operation_prefix: &str,
    ) -> (i64, i64) {
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("family count tenant connection");
        let state_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.forge_operation_state \
             WHERE data_tenant_id = wyrd.current_tenant() AND family = $1",
        )
        .bind(family)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("family state count");
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox \
             WHERE data_tenant_id = wyrd.current_tenant() AND operation LIKE $1",
        )
        .bind(format!("{operation_prefix}%"))
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("family audit count");
        (state_count, audit_count)
    }

    /// Construct the established system-owned envelope for a transition test.
    fn operation_event(operation: &str, resource: &str, detail: AuditDetail) -> AuditEvent {
        AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: operation.to_owned(),
            resource: resource.to_owned(),
            card_ref: None,
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            principal_kind: PrincipalKindTag::Service,
            auth_method: AuthMethod::Internal,
            permission: "bifrost:forge".to_owned(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: operation.to_owned(),
            detail: Some(detail),
        }
    }

    /// Append and commit one prepared or terminal transition through the SQL owner.
    ///
    /// # Panics
    ///
    /// Panics when tenant acquisition, transition validation, persistence, or
    /// commit fails because successful transition setup is a test invariant.
    async fn append_operation(
        fixture: &Fixture,
        family: ForgeOperationFamily,
        event: &AuditEvent,
        prepared: bool,
    ) -> ForgeOperationTransition {
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("operation tenant connection");
        let operations =
            ForgeOperations::new(&event.resource, family).expect("operation family owner");
        let transition = if prepared {
            operations.append_prepared(&mut conn, event).await
        } else {
            operations.append_terminal(&mut conn, event).await
        }
        .expect("operation transition");
        conn.commit().await.expect("operation transition commit");
        transition
    }

    /// Assert one exact terminal phase has two parity-valid audit events.
    ///
    /// # Panics
    ///
    /// Panics when the tenant query fails or phase, cardinality, or canonical
    /// detail parity differs from the expected terminal transition.
    async fn assert_exact_terminal(fixture: &Fixture, resource: &str, expected_phase: &str) {
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("exact terminal tenant connection");
        let row: (String, i64, bool, bool) = sqlx::query_as(
            r"SELECT state.phase,
                     (SELECT count(*) FROM vala.audit_outbox AS audit
                       WHERE audit.data_tenant_id = state.data_tenant_id
                         AND audit.resource = state.resource),
                     state.prepared_detail = prepared.detail::jsonb,
                     state.current_detail = terminal.detail::jsonb
                FROM vala.forge_operation_state AS state
                JOIN vala.audit_outbox AS prepared
                  ON prepared.data_tenant_id = state.data_tenant_id
                 AND prepared.seq = state.prepared_audit_seq
                JOIN vala.audit_outbox AS terminal
                  ON terminal.data_tenant_id = state.data_tenant_id
                 AND terminal.seq = state.terminal_audit_seq
               WHERE state.data_tenant_id = wyrd.current_tenant()
                 AND state.resource = $1",
        )
        .bind(resource)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("exact terminal parity");
        assert_eq!(row, (expected_phase.to_owned(), 2, true, true));
    }

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

    /// Acquire the production table lease for one fixture.
    ///
    /// # Panics
    ///
    /// Panics when the lease query fails or another owner unexpectedly holds
    /// the isolated fixture lease.
    async fn acquire_fixture_lease(fixture: &Fixture) -> ForgeLease {
        ForgeLease::acquire(
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
        .expect("fixture lease query")
        .expect("fixture lease")
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
        assert_staging_transition_parity(&fixture).await;
    }

    /// Periodic expiry and orphan collection write exact state/audit parity.
    #[tokio::test]
    async fn destructive_maintenance_transitions_preserve_audit_projection_parity() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                max_concurrent_reads: 2,
                ..ForgeConfig::default()
            },
            true,
            4,
            16 * 1024 * 1024,
        )
        .await;

        let first = fixture
            .forge
            .run_once()
            .await
            .expect("first maintenance tick");
        assert_eq!(first.bins_committed, 1, "first outcome: {first:?}");
        fixture.seed_files_at(100, 4, true).await;
        let second = fixture
            .forge
            .run_once()
            .await
            .expect("second maintenance tick");
        assert_eq!(second.bins_committed, 1, "second outcome: {second:?}");

        assert_terminal_family_parity(&fixture, "snapshot_expire").await;
        assert_terminal_family_parity(&fixture, "orphan_gc").await;
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
            16 * 1024 * 1024,
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

        let outcome = fixture.forge.run_once().await.expect("isolated table tick");
        assert_eq!(outcome.tables_failed, 1, "outcome: {outcome:?}");

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
        let day = chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("fixture day");
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

    /// Build two staging outputs and return one executable live-rewrite plan.
    ///
    /// # Panics
    ///
    /// Panics when staging, catalog configuration, discovery, or lease
    /// acquisition fails because each is a fixture invariant.
    async fn prepare_live_transition(
        fixture: &Fixture,
    ) -> (iceberg::table::Table, i64, IcebergRewriteGroup, ForgeLease) {
        fixture.forge.run_once().await.expect("first staging fold");
        fixture.seed_files_at(100, 2, true).await;
        fixture.forge.run_once().await.expect("second staging fold");
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
        let current_day = chrono::NaiveDate::from_ymd_opt(2026, 7, 15).expect("fixed day");
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
            64 * 1024 * 1024,
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
