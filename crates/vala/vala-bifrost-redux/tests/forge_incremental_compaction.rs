//! Integration proof for bounded Forge rewrites and Iceberg metadata.

mod pg_tests {
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    use std::time::Duration;

    use arrow::array::{
        FixedSizeBinaryBuilder, Int32Array, Int64Array, RecordBatch, StringArray,
        TimestampMicrosecondArray,
    };
    use arrow::datatypes::{DataType, Field};
    use iceberg::spec::{
        DataContentType, DataFile, DataFileBuilder, DataFileFormat, Datum, Literal, NullOrder,
        PrimitiveType, SortDirection, StatisticsFile, Struct, TableMetadata, Transform, Type,
    };
    use iceberg::table::Table;
    use iceberg::transaction::{AddColumn, ApplyTransactionAction, Transaction};
    use iceberg::{
        Catalog, MetadataLocation, Namespace, NamespaceIdent, TableCommit, TableCreation,
        TableIdent,
    };
    use opendal::services::Fs;
    use opendal::{Buffer, Operator};
    use parquet::arrow::ArrowWriter;
    use secrecy::ExposeSecret;
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;
    use vala_bifrost_redux::catalog::{
        BifrostCatalog, CreateTableRequest, TableRef, TenantTableBinding,
    };
    use vala_bifrost_redux::forge::{
        Forge, ForgeBuildConfig, ForgeClock, ForgeConfig, ForgeError, ForgeLease, ForgeObjectPages,
        ForgeObjectStore, ForgeScheduleOutcome, ForgeScheduler, ForgeWorker,
        ForgeWorkerCompletionObserver, ForgeWorkerConfig, IcebergCandidateFile,
        IcebergRewriteGroup, deterministic_output_path_for_test, forge_lease_key,
    };
    use vala_bifrost_redux::maintenance::staging_file_channel;
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_sql::OperatorPool;
    use vala_sql::queries::forge_operations::ForgeOperations;
    use vala_sql::queries::forge_tasks::ForgeTasks;
    use vala_sql::row_types::forge_operations::{ForgeOperationFamily, ForgeOperationTransition};
    use vala_sql::row_types::forge_tasks::{
        FORGE_TASK_PAYLOAD_VERSION, ForgeClaimStrategy, ForgeTaskClaim, ForgeTaskEstimates,
        ForgeTaskLane, ForgeTaskPlan, ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask,
    };
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::WYRD_EVENT_TIME;
    use wyrd_spec::vala::api::{
        AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeCompactionPhase,
        ForgeIcebergRewritePhase, ForgeOrphanGcPhase, ForgeSnapshotExpirePhase, StoragePath,
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
        /// One-shot metadata-read failure used after a successful commit.
        fail_next_metadata_read: AtomicBool,
        /// One-shot pause consumed by the next successful rewrite output.
        pause_output_puts: AtomicUsize,
        /// Counts armed outputs that crossed the real PUT boundary.
        paused_output_puts: AtomicUsize,
        /// Wakes a test waiting for the armed real output.
        output_put_ready: tokio::sync::Notify,
        /// Releases the armed output notification.
        output_put_release: tokio::sync::Notify,
        /// Durable release state preventing a lost post-PUT wake-up.
        output_put_released: AtomicBool,
        /// Counts successful rewrite output notifications.
        output_put_calls: AtomicUsize,
        /// Counts bounded chunks accepted by output writers.
        output_chunks: AtomicUsize,
        /// Counts output writer openings across successful and aborted uploads.
        output_writer_opens: AtomicUsize,
        /// Counts multipart aborts after injected chunk failures.
        output_writer_aborts: AtomicUsize,
        /// Largest chunk supplied to an output writer.
        largest_output_chunk: AtomicUsize,
        /// One-shot chunk failure used to prove abort and same-attempt retry.
        fail_next_output_chunk: AtomicBool,
        /// Per-path cleanup delete attempts observed through the production store seam.
        delete_attempts: Mutex<HashMap<String, usize>>,
        /// Entries per orphan-listing page, or `0` to keep the single-page default.
        ///
        /// A positive value makes [`ForgeObjectStore::list_pages`] paginate the
        /// sorted listing into fixed-size pages, modeling a lexicographically
        /// ordered object backend so bounded-scan tests can exercise the page cap
        /// and cross-run drainage deterministically.
        list_page_entries: AtomicUsize,
    }

    impl InstrumentedStore {
        /// Arms a one-shot pause after the next successful rewrite output PUT.
        fn pause_after_next_output_put(&self) {
            self.pause_after_output_puts(1);
        }

        /// Waits until the armed output has crossed the real PUT boundary.
        async fn wait_for_output_put(&self) {
            self.wait_for_output_puts(1).await;
        }

        /// Arms pauses after an exact positive number of successful output PUTs.
        fn pause_after_output_puts(&self, count: usize) {
            assert!(count > 0, "paused output count must be positive");
            self.paused_output_puts.store(0, Ordering::Release);
            self.output_put_released.store(false, Ordering::Release);
            self.pause_output_puts.store(count, Ordering::Release);
        }

        /// Waits until every armed output has crossed its real PUT boundary.
        async fn wait_for_output_puts(&self, count: usize) {
            while self.paused_output_puts.load(Ordering::Acquire) < count {
                self.output_put_ready.notified().await;
            }
        }

        /// Releases one paused post-PUT notification.
        fn release_output_put(&self) {
            self.output_put_released.store(true, Ordering::Release);
            self.output_put_release.notify_waiters();
        }

        /// Arms one failure for the next exact Iceberg metadata read.
        fn fail_next_metadata_read(&self) {
            self.fail_next_metadata_read.store(true, Ordering::Release);
        }

        /// Returns the number of successful rewrite output boundaries observed.
        fn output_put_calls(&self) -> usize {
            self.output_put_calls.load(Ordering::Acquire)
        }

        /// Arms one failure on the next output chunk.
        fn fail_next_output_chunk(&self) {
            self.fail_next_output_chunk.store(true, Ordering::Release);
        }

        /// Returns the exact number of delete attempts observed for one object path.
        fn delete_attempts_for(&self, path: &str) -> usize {
            self.delete_attempts
                .lock()
                .expect("delete-attempt ledger lock")
                .get(path)
                .copied()
                .unwrap_or_default()
        }

        /// Returns the total cleanup delete attempts observed across all paths.
        fn total_delete_attempts(&self) -> usize {
            self.delete_attempts
                .lock()
                .expect("delete-attempt ledger lock")
                .values()
                .sum()
        }

        /// Sets the entries-per-page cap for the paginated orphan listing seam.
        ///
        /// A positive `entries` makes [`ForgeObjectStore::list_pages`] emit fixed
        /// pages over the sorted listing so a test can drive the per-run page cap;
        /// `0` restores the default single-page adaptation.
        fn set_list_page_entries(&self, entries: usize) {
            self.list_page_entries.store(entries, Ordering::Release);
        }
    }

    #[async_trait::async_trait]
    impl ForgeObjectStore for InstrumentedStore {
        /// Opens the production chunked writer while recording retry attempts.
        async fn output_writer(
            &self,
            operator: &opendal::Operator,
            path: &str,
            chunk_bytes: usize,
        ) -> opendal::Result<opendal::Writer> {
            self.output_writer_opens.fetch_add(1, Ordering::AcqRel);
            operator.writer_with(path).chunk(chunk_bytes).await
        }

        /// Records bounded chunks and optionally injects one retryable failure.
        async fn write_output_chunk(
            &self,
            writer: &mut opendal::Writer,
            chunk: bytes::Bytes,
        ) -> opendal::Result<()> {
            self.output_chunks.fetch_add(1, Ordering::AcqRel);
            self.largest_output_chunk
                .fetch_max(chunk.len(), Ordering::AcqRel);
            if self.fail_next_output_chunk.swap(false, Ordering::AcqRel) {
                return Err(opendal::Error::new(
                    opendal::ErrorKind::Unexpected,
                    "injected Forge output chunk failure",
                ));
            }
            writer.write(chunk).await
        }

        /// Records and delegates abort of one incomplete output writer.
        async fn abort_output_writer(&self, writer: &mut opendal::Writer) -> opendal::Result<()> {
            self.output_writer_aborts.fetch_add(1, Ordering::AcqRel);
            writer.abort().await
        }

        /// Reject whole-object reads so the rewrite cannot hide an unbounded path.
        async fn read(&self, path: &str) -> opendal::Result<Buffer> {
            if std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            {
                if self.fail_next_metadata_read.swap(false, Ordering::AcqRel) {
                    return Err(opendal::Error::new(
                        opendal::ErrorKind::Unexpected,
                        "injected Forge metadata evidence read failure",
                    ));
                }
                return self.operator.read(path).await;
            }
            self.whole_reads.fetch_add(1, Ordering::Relaxed);
            Err(opendal::Error::new(
                opendal::ErrorKind::Unsupported,
                "whole-object data read forbidden",
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

        /// Observes and optionally pauses after one real rewrite output PUT.
        async fn after_output_put(&self, _path: &str) {
            self.output_put_calls.fetch_add(1, Ordering::AcqRel);
            let paused = self
                .pause_output_puts
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                    if remaining > 0 {
                        Some(remaining - 1)
                    } else {
                        None
                    }
                })
                .is_ok();
            if paused {
                self.paused_output_puts.fetch_add(1, Ordering::AcqRel);
                self.output_put_ready.notify_waiters();
                while !self.output_put_released.load(Ordering::Acquire) {
                    self.output_put_release.notified().await;
                }
            }
        }

        /// List fixture objects for cleanup and orphan checks.
        async fn list(&self, prefix: &str) -> opendal::Result<Vec<opendal::Entry>> {
            self.operator.list_with(prefix).recursive(true).await
        }

        /// Paginate the sorted listing when a page size is armed, else one page.
        ///
        /// With `list_page_entries == 0` this reproduces the trait default's
        /// single-page adaptation. With a positive page size it retains only
        /// file entries, sorts the listing by path, and slices it into fixed
        /// pages, modeling a flat, lexicographically ordered object store (S3,
        /// GCS, Azure) whose recursive listing yields objects and no directory
        /// markers, so the orphan-GC page cap and cross-run drainage are
        /// deterministic rather than perturbed by the local filesystem
        /// backend's synthetic directory entries.
        async fn list_pages(&self, prefix: &str) -> opendal::Result<ForgeObjectPages> {
            let page_entries = self.list_page_entries.load(Ordering::Acquire);
            if page_entries == 0 {
                let entries = self.list(prefix).await?;
                return Ok(Box::pin(futures_util::stream::once(
                    futures_util::future::ready(Ok(entries)),
                )));
            }
            let mut entries = self.list(prefix).await?;
            entries.retain(|entry| entry.metadata().mode() == opendal::EntryMode::FILE);
            entries.sort_by(|left, right| left.path().cmp(right.path()));
            let pages = entries
                .chunks(page_entries)
                .map(|chunk| Ok(chunk.to_vec()))
                .collect::<Vec<opendal::Result<Vec<opendal::Entry>>>>();
            Ok(Box::pin(futures_util::stream::iter(pages)))
        }

        /// Stat one fixture object.
        async fn stat(&self, path: &str) -> opendal::Result<opendal::Metadata> {
            self.operator.stat(path).await
        }

        /// Delete one rewrite-owned object.
        async fn delete(&self, path: &str) -> opendal::Result<()> {
            {
                let mut attempts = self
                    .delete_attempts
                    .lock()
                    .expect("delete-attempt ledger lock");
                *attempts.entry(path.to_owned()).or_default() += 1;
            }
            self.operator.delete(path).await
        }
    }

    /// Real dependencies used by all incremental Forge proofs.
    /// Returns the fixture's complete raw resource observation.
    ///
    /// The fixture injects observations only; the production policy and role
    /// composition derive every grant from them, so no fixture path computes a
    /// reserve, floor, elastic, grant, scratch, or partition value itself.
    fn fixture_snapshot() -> SystemResourceSnapshot {
        SystemResourceSnapshot {
            memory_limit_bytes: 1024 * 1024 * 1024,
            effective_cpu: 4,
            scratch_capacity_bytes: 4 * 1024 * 1024 * 1024,
            scratch_available_bytes: 4 * 1024 * 1024 * 1024,
            memory_source: ResourceSource::Injected,
            cpu_source: ResourceSource::Injected,
        }
    }

    /// Returns the Forge-only policy every fixture worker composes from.
    fn forge_policy(scratch_limit_bytes: u64) -> BifrostResourcePolicy {
        BifrostResourcePolicy {
            roles: [BifrostRole::Forge].into_iter().collect(),
            memory_limit_bytes: None,
            unmanaged_reserve_bytes: None,
            scratch_limit_bytes: Some(scratch_limit_bytes),
            effective_cpu: None,
            oracle_query_slot_limit: None,
            scratch_root: std::path::PathBuf::new(),
            volume_roots: None,
        }
    }

    use vala_bifrost_redux::resources::{
        BifrostResourcePolicy, BifrostRole, ResourceSource, SystemResourceSnapshot,
    };

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
        root: TempDir,
        /// Source-read instrumentation.
        reads: Arc<InstrumentedStore>,
        /// Forge handle under test.
        forge: Arc<Forge>,
        /// Stable scheduler lease identity shared by bounded fixture passes.
        scheduler_owner: uuid::Uuid,
        /// Production worker owner used to drain exact durable fixture tasks.
        worker: ForgeWorker,
        /// The one role composition every worker in this fixture leases from.
        ///
        /// Retaining the composition rather than a raw governor is what makes
        /// the primary and sibling workers provably share a single process
        /// root instead of contending against independent ledgers.
        roles: vala_bifrost_redux::resources::BifrostRoleResources,
        /// Supervised completion observer shared with the worker under test.
        ///
        /// Present only for fixtures built to drive the supervised
        /// [`ForgeWorker::run`] loop through its claim-gate seam; ordinary
        /// direct-execution fixtures leave it `None`.
        completion: Option<ForgeWorkerCompletionObserver>,
    }

    /// SQL projection and audit parity columns for one staging operation.
    type StagingParityRow = (String, i64, Option<i64>, bool, Option<bool>);

    /// One-shot wrapper applied to the registered catalog before Forge composition.
    ///
    /// Lets a fixture interpose an observing or fault-injecting [`Catalog`] over
    /// the production catalog Forge holds, without altering table registration,
    /// which runs against the raw catalog first.
    type CatalogDecorator = Box<dyn FnOnce(Arc<dyn Catalog>) -> Arc<dyn Catalog> + Send>;

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
                    physical_layout: None,
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
            assert_eq!(partition_fields[0].name, "wyrd_event_time_hour");
            assert_eq!(partition_fields[0].transform, Transform::Hour);

            let sort_fields = &table.metadata().default_sort_order().fields;
            assert_eq!(sort_fields.len(), 2);
            // The canonical layout leads with the ascending tenant prefix and
            // then orders newest-first on event time.
            for (field, (source_id, direction)) in sort_fields.iter().zip([
                (tenant_id, SortDirection::Ascending),
                (event_time_id, SortDirection::Descending),
            ]) {
                assert_eq!(field.source_id, source_id);
                assert_eq!(field.transform, Transform::Identity);
                assert_eq!(field.direction, direction);
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
                fixture_snapshot(),
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
            snapshot: SystemResourceSnapshot,
        ) -> Self {
            Self::build(
                config,
                aged_inputs,
                initial_file_count,
                snapshot,
                None,
                None,
            )
            .await
        }

        /// Build a real fixture whose worker runs the supervised loop under a
        /// caller-supplied completion observer.
        ///
        /// The observer's claim gate lets a test pause a supervised slot after
        /// its durable claim, drive cooperative shutdown, and then release the
        /// slot to observe the drain-to-`retryable` behavior of the real
        /// [`ForgeWorker::run`] loop.
        ///
        /// # Panics
        ///
        /// Panics when the embedded database, catalog, storage, or Forge owner
        /// cannot be initialized; these are test-environment invariants.
        async fn new_with_observer(observer: ForgeWorkerCompletionObserver) -> Self {
            Self::build(
                ForgeConfig::default(),
                true,
                2,
                fixture_snapshot(),
                Some(observer),
                None,
            )
            .await
        }

        /// Construct the fixture with an optional supervised completion observer.
        ///
        /// # Panics
        ///
        /// Panics when the embedded database, catalog, storage, or Forge owner
        /// cannot be initialized; these are test-environment invariants.
        async fn build(
            config: ForgeConfig,
            aged_inputs: bool,
            initial_file_count: usize,
            snapshot: SystemResourceSnapshot,
            completion: Option<ForgeWorkerCompletionObserver>,
            catalog_decorator: Option<CatalogDecorator>,
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
            let catalog = match catalog_decorator {
                Some(decorate) => decorate(catalog),
                None => catalog,
            };
            let operator_pool = pg.operator_pool().clone();
            let whole_reads = Arc::new(AtomicUsize::new(0));
            let reads = Arc::new(InstrumentedStore {
                operator: Arc::clone(&staging),
                whole_reads: Arc::clone(&whole_reads),
                ranged_reads: Arc::new(AtomicUsize::new(0)),
                active_reads: Arc::new(AtomicUsize::new(0)),
                peak_reads: Arc::new(AtomicUsize::new(0)),
                fail_next_metadata_read: AtomicBool::new(false),
                pause_output_puts: AtomicUsize::new(0),
                paused_output_puts: AtomicUsize::new(0),
                output_put_ready: tokio::sync::Notify::new(),
                output_put_release: tokio::sync::Notify::new(),
                output_put_released: AtomicBool::new(false),
                output_put_calls: AtomicUsize::new(0),
                output_chunks: AtomicUsize::new(0),
                output_writer_opens: AtomicUsize::new(0),
                output_writer_aborts: AtomicUsize::new(0),
                largest_output_chunk: AtomicUsize::new(0),
                fail_next_output_chunk: AtomicBool::new(false),
                delete_attempts: Mutex::new(HashMap::new()),
                list_page_entries: AtomicUsize::new(0),
            });
            let (_publisher, hints) = staging_file_channel(16).expect("hint channel");
            let roles = vala_bifrost_redux::resources::BifrostRuntimeResources::from_snapshot(
                snapshot,
                forge_policy(config.spill_limit_bytes),
            )
            .expect("injected observation must satisfy the resource policy")
            .compose_roles()
            .expect("composition must be issued from an unpoisoned root");
            let forge = Arc::new(
                Forge::new(ForgeBuildConfig {
                    resources: roles
                        .forge()
                        .expect("composition must issue the Forge capability"),
                    vala: pg.vala_postgres().clone(),
                    operator_pool: operator_pool.clone(),
                    catalog: Arc::clone(&catalog),
                    staging: Arc::clone(&staging),
                    object_store: Arc::clone(&reads) as Arc<dyn ForgeObjectStore>,
                    rewrite_spill_root: root.path().join("spill"),
                    hints,
                    config,
                    maintenance_interval: Duration::from_millis(10),
                    clock: ForgeClock::system(),
                    completion_observer: completion.clone(),
                    scheduler_trigger: None,
                    telemetry: Arc::new(vala_bifrost_redux::forge::ForgeTelemetry::new()),
                })
                .expect("forge"),
            );
            let worker = ForgeWorker::new(
                Arc::clone(&forge),
                ForgeWorkerConfig {
                    worker_concurrency: 1,
                    per_tenant_active_cap: 1,
                },
                uuid::Uuid::now_v7(),
            )
            .expect("fixture worker");
            let fixture = Self {
                pg,
                tenant,
                binding,
                operator_pool,
                catalog,
                staging,
                root,
                reads,
                forge,
                scheduler_owner: uuid::Uuid::now_v7(),
                worker,
                roles,
                completion,
            };
            fixture.seed_files(initial_file_count, aged_inputs).await;
            fixture
        }

        /// Runs one durable planning pass and drains every immediately claimable task.
        ///
        /// # Panics
        ///
        /// Panics when scheduling, claiming, execution, evidence persistence, or
        /// terminal reconciliation fails because each is the behavior under test.
        async fn schedule_and_execute(&self) -> ForgeScheduleOutcome {
            let stop = CancellationToken::new();
            let scheduler = ForgeScheduler::with_owner_for_test(&self.forge, self.scheduler_owner)
                .expect("fixture scheduler");
            let outcome = scheduler
                .schedule_once(&stop)
                .await
                .expect("fixture planning pass");
            while self
                .worker
                .execute_one_for_test(&stop)
                .await
                .expect("fixture worker task")
            {}
            outcome
        }

        /// Build a sibling worker that shares this fixture's durable state but
        /// carries a tightened Iceberg retry timeout.
        ///
        /// The sibling reuses the same catalog, operator pool, staging store, and
        /// object store, so a claim this fixture already took remains executable
        /// through it, and it inherits this fixture worker's ownership identity so
        /// the durable claim transitions (fenced on `claimed_by`/`attempt_id`)
        /// still match. Only the sibling's Forge carries `retry_timeout`, which
        /// lets a test trip the maintenance manifest-rewrite timeout on execution
        /// without subjecting the fixture's own setup commits to it. The
        /// aggressive `snapshot_retention`/`orphan_gc_ttl` mirror the maintenance
        /// fixtures so the periodic demand remains a genuine expiry candidate.
        ///
        /// # Panics
        ///
        /// Panics when the runtime, Forge, or worker cannot be constructed; these
        /// are test-environment invariants.
        fn sibling_worker_with_retry_timeout(&self, retry_timeout: Duration) -> ForgeWorker {
            self.sibling_worker_with_config(ForgeConfig {
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                iceberg_total_retry_timeout: retry_timeout,
                manifest_rewrite_enabled: true,
                manifest_rewrite_min_count: 2,
                ..ForgeConfig::default()
            })
        }

        /// Builds a same-owner worker with caller-selected execution policy.
        fn sibling_worker_with_config(&self, config: ForgeConfig) -> ForgeWorker {
            let (_publisher, hints) = staging_file_channel(16).expect("sibling hint channel");
            let forge = Arc::new(
                Forge::new(ForgeBuildConfig {
                    resources: self
                        .roles
                        .forge()
                        .expect("the sibling worker must share the fixture's one Forge root"),
                    vala: self.pg.vala_postgres().clone(),
                    operator_pool: self.operator_pool.clone(),
                    catalog: Arc::clone(&self.catalog),
                    staging: Arc::clone(&self.staging),
                    object_store: Arc::clone(&self.reads) as Arc<dyn ForgeObjectStore>,
                    rewrite_spill_root: self.root.path().join("sibling-spill"),
                    hints,
                    config,
                    maintenance_interval: Duration::from_millis(10),
                    clock: ForgeClock::system(),
                    completion_observer: None,
                    scheduler_trigger: None,
                    telemetry: Arc::new(vala_bifrost_redux::forge::ForgeTelemetry::new()),
                })
                .expect("sibling forge"),
            );
            ForgeWorker::new(
                forge,
                ForgeWorkerConfig {
                    worker_concurrency: 1,
                    per_tenant_active_cap: 1,
                },
                self.worker.owner_for_test(),
            )
            .expect("sibling worker")
        }

        /// Persists a metadata version whose prior snapshot is detached from every ref.
        ///
        /// This fixture-only mutation removes the current snapshot's parent link,
        /// writes the resulting valid Iceberg metadata file, and atomically advances
        /// the SQL catalog pointer. The returned snapshot ID remains present in the
        /// persisted snapshot map but is unreachable from every named reference.
        async fn persist_detached_prior_snapshot(&self) -> (i64, i64) {
            let table = self
                .catalog
                .load_table(&self.binding.table_ident())
                .await
                .expect("table with snapshot history");
            let current = table
                .metadata()
                .current_snapshot()
                .expect("current snapshot");
            let detached_id = current.parent_snapshot_id().expect("prior snapshot");
            let detached_timestamp = table
                .metadata()
                .snapshot_by_id(detached_id)
                .expect("retained prior snapshot")
                .timestamp_ms();
            let current_id = current.snapshot_id();
            let mut metadata_json =
                serde_json::to_value(table.metadata()).expect("serialize fixture Iceberg metadata");
            let snapshots = metadata_json["snapshots"]
                .as_array_mut()
                .expect("snapshot array");
            let current_json = snapshots
                .iter_mut()
                .find(|snapshot| snapshot["snapshot-id"].as_i64() == Some(current_id))
                .expect("serialized current snapshot");
            current_json
                .as_object_mut()
                .expect("snapshot object")
                .remove("parent-snapshot-id");
            let detached_metadata: TableMetadata =
                serde_json::from_value(metadata_json).expect("valid detached Iceberg metadata");
            let old_location = table
                .metadata_location()
                .expect("current metadata location");
            let new_location = MetadataLocation::from_str(old_location)
                .expect("parsed metadata location")
                .with_next_version();
            detached_metadata
                .write_to(table.file_io(), &new_location)
                .await
                .expect("persist detached metadata");
            let catalog_pool = sqlx::PgPool::connect(self.pg.catalog_dsn().expose_secret())
                .await
                .expect("catalog test connection");
            let ident = self.binding.table_ident();
            let updated = sqlx::query(
                "UPDATE iceberg_tables SET metadata_location=$1,previous_metadata_location=$2 \
                 WHERE catalog_name='wyrd-redux' AND table_namespace=$3 AND table_name=$4 \
                   AND metadata_location=$2",
            )
            .bind(new_location.to_string())
            .bind(old_location)
            .bind(ident.namespace().join("."))
            .bind(ident.name())
            .execute(&catalog_pool)
            .await
            .expect("advance fixture catalog metadata")
            .rows_affected();
            assert_eq!(updated, 1, "fixture metadata pointer must advance once");
            (detached_id, detached_timestamp)
        }

        /// Plans one exact task and returns its production SQL claim envelope.
        ///
        /// # Panics
        ///
        /// Panics when scheduling or admission does not produce exactly one
        /// task for the isolated fixture table.
        async fn plan_and_claim(&self) -> ForgeTaskClaim {
            let stop = CancellationToken::new();
            let planned = ForgeScheduler::with_owner_for_test(&self.forge, self.scheduler_owner)
                .expect("fixture scheduler")
                .schedule_once(&stop)
                .await
                .expect("fixture planning pass");
            assert_eq!(planned.tasks_enqueued, 1, "planned outcome: {planned:?}");
            self.worker
                .claim_for_test()
                .await
                .expect("fixture claim query")
                .expect("fixture claimed task")
        }

        /// Builds one exact durable task for worker validation tests.
        ///
        /// # Panics
        ///
        /// Panics only when the fixture's production table identity cannot be
        /// represented by the already-validated SQL contract.
        fn durable_task(
            &self,
            strategy: ForgeTaskStrategy,
            plan: ForgeTaskPlan,
            hash: u8,
        ) -> NewForgeTask {
            NewForgeTask {
                data_tenant_id: self.tenant,
                table_ref: ForgeTaskTableIdentity::new(
                    "wyrd-redux",
                    &self.binding.logical_namespace,
                    &self.binding.table_name,
                )
                .expect("fixture durable identity"),
                strategy,
                lane: ForgeTaskLane::Ordinary,
                base_snapshot_id: 0,
                plan,
                plan_hash: [hash; 32],
                estimates: ForgeTaskEstimates {
                    envelope: Some(vala_sql::row_types::forge_tasks::ForgeTaskEnvelope {
                        version: vala_sql::row_types::forge_tasks::FORGE_ENVELOPE_VERSION,
                        reader_permits: 1,
                        decoded_batch_bytes: 16 * 1024 * 1024,
                        decoded_input_bytes: 16 * 1024 * 1024,
                        sort_working_bytes: 42 * 1024 * 1024,
                        sort_merge_reservation_bytes: 10 * 1024 * 1024,
                        encoder_buffer_bytes: 32 * 1024 * 1024,
                        upload_chunk_bytes: 8 * 1024 * 1024,
                        footer_encoded_bytes: 8 * 1024 * 1024,
                        footer_decode_workspace_bytes: 32 * 1024 * 1024,
                        sort_spill_bytes: 512 * 1024 * 1024,
                        output_scratch_bytes: 512 * 1024 * 1024,
                    }),
                    files: 1,
                    bytes: 1,
                    parallelism: 1,
                    memory_bytes: 106 * 1024 * 1024,
                    spill_bytes: 512 * 1024 * 1024,
                    large_ceiling_bytes: 2 * 1024 * 1024 * 1024,
                },
                ready_at: chrono::Utc::now() - chrono::Duration::seconds(1),
            }
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
            self.seed_files_for_binding(&self.binding, start, count, aged)
                .await;
        }

        /// Seeds uniquely named Parquet objects for a selected registered table.
        ///
        /// # Panics
        ///
        /// Panics when encoding, storage, or tenant-scoped file-list writes fail.
        async fn seed_files_for_binding(
            &self,
            binding: &TenantTableBinding,
            start: i64,
            count: usize,
            aged: bool,
        ) {
            self.seed_files_for_binding_contract(binding, start, count, aged, true, false)
                .await;
        }

        /// Seed writer-v2 objects directly under Forge's current recipe layout.
        ///
        /// # Panics
        ///
        /// Panics when encoding, storage, or durable file-list writes fail.
        async fn seed_current_recipe_files(&self, count: usize, aged: bool) {
            self.seed_files_for_binding_contract(&self.binding, 0, count, aged, true, true)
                .await;
        }

        /// Seeds legacy unmarked inputs through the same physical fixture path.
        async fn seed_unmarked_files(&self, count: usize, aged: bool) {
            self.seed_files_for_binding_contract(&self.binding, 0, count, aged, false, false)
                .await;
        }

        /// Seeds physical inputs with caller-selected footer and path identity.
        ///
        /// # Panics
        ///
        /// Panics when schema conversion, encoding, storage, or durable
        /// file-list persistence fails.
        async fn seed_files_for_binding_contract(
            &self,
            binding: &TenantTableBinding,
            start: i64,
            count: usize,
            aged: bool,
            writer_v2: bool,
            current_recipe_layout: bool,
        ) {
            let table = self
                .catalog
                .load_table(&binding.table_ident())
                .await
                .expect("registered fixture table");
            let schema = iceberg::arrow::schema_to_arrow_schema(table.metadata().current_schema())
                .expect("registered Arrow schema");
            let base_event_time = fixture_seed_event_time();
            let partition = registered_granularity(&table)
                .bucket(base_event_time)
                .expect("a fixture event time always buckets");
            let base = base_event_time.timestamp_micros();
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
                        Arc::new(StringArray::from(vec!["principal"; row_count_usize])),
                        Arc::new(StringArray::from(vec!["request"; row_count_usize])),
                        Arc::new(
                            TimestampMicrosecondArray::from(
                                (0..row_count)
                                    .map(|row| base + index + row)
                                    .collect::<Vec<_>>(),
                            )
                            .with_timezone("+00:00"),
                        ),
                        Arc::new(
                            TimestampMicrosecondArray::from(
                                (0..row_count)
                                    .map(|row| base + index + row)
                                    .collect::<Vec<_>>(),
                            )
                            .with_timezone("+00:00"),
                        ),
                        Arc::new(batch_ids.finish()),
                        Arc::new(Int32Array::from_iter_values(
                            (0..row_count_usize)
                                .map(|row| i32::try_from(row).expect("row ordinal fits i32")),
                        )),
                        Arc::new(StringArray::from(vec![
                            self.tenant.to_string();
                            row_count_usize
                        ])),
                    ],
                )
                .expect("batch");
                let path = seed_object_path(binding, index, current_recipe_layout);
                let metadata = if writer_v2 {
                    vala_bifrost_redux::parquet::BifrostParquetMemoryEnvelope::metadata_for_batch(
                        &batch, &path,
                    )
                    .expect("writer-v2 metadata")
                } else {
                    Vec::new()
                };
                let mut bytes = Vec::new();
                let mut writer = ArrowWriter::try_new(
                    &mut bytes,
                    batch.schema(),
                    Some(
                        vala_bifrost_redux::parquet::writer_properties::bifrost_writer_properties_with_metadata(
                            batch.num_rows(),
                            metadata,
                            &[],
                        ),
                    ),
                )
                .expect("writer");
                writer.write(&batch).expect("write");
                writer.close().expect("close");
                self.staging
                    .write(&path, Buffer::from(bytes))
                    .await
                    .expect("object");
                rows.push((path, index, row_count));
            }
            self.persist_seed_rows(binding, rows, base, partition, aged)
                .await;
        }

        /// Persists generated fixture objects into the durable staging roster.
        async fn persist_seed_rows(
            &self,
            binding: &TenantTableBinding,
            rows: Vec<(String, i64, i64)>,
            base: i64,
            partition: vala_bifrost_redux::catalog::layout::TimePartition,
            aged: bool,
        ) {
            let mut conn = vala_sql::TenantConn::acquire(self.pg.app_pool(), self.tenant)
                .await
                .expect("tenant conn");
            for (object_path, index, row_count) in rows {
                sqlx::query("INSERT INTO vala.file_list (id,data_tenant_id,namespace,table_name,file_path,file_size,row_count,min_event_time,max_event_time,partition_granularity,partition_start,node_id,writer_epoch,wal_lsn_min,wal_lsn_max) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)").bind(uuid::Uuid::now_v7()).bind(self.tenant.as_uuid()).bind(&binding.logical_namespace).bind(&binding.table_name).bind(object_path).bind(100_i64).bind(row_count).bind(chrono::DateTime::from_timestamp_micros(base + index).expect("min")).bind(chrono::DateTime::from_timestamp_micros(base + index + row_count).expect("max")).bind(partition.granularity_str()).bind(partition.start_utc()).bind(uuid::Uuid::now_v7()).bind(1_i64).bind(index * 2 + 1).bind(index * 2 + 2).execute(&mut **conn.transaction()).await.expect("file list");
            }
            conn.commit().await.expect("commit");
            if aged {
                self.age_seeded_files(binding).await;
            }
        }

        /// Ages the seeded staging rows beyond the right-size open-window threshold.
        async fn age_seeded_files(&self, binding: &TenantTableBinding) {
            sqlx::query("UPDATE vala.file_list SET created_at = now() - interval '3 minutes' WHERE data_tenant_id = $1 AND namespace=$2 AND table_name=$3").bind(self.tenant.as_uuid()).bind(&binding.logical_namespace).bind(&binding.table_name).execute(self.operator_pool.pool()).await.expect("age");
        }

        /// Registers and seeds a second table in this fixture's exact backend.
        ///
        /// # Panics
        ///
        /// Panics when catalog registration, table configuration, or seed
        /// persistence fails in the isolated integration fixture.
        async fn register_seeded_table(&self) -> TenantTableBinding {
            let table_name = format!("concurrent_rows_{}", uuid::Uuid::now_v7().simple());
            let binding = TenantTableBinding::resolve((
                self.tenant,
                TableRef::new(BifrostNamespace::Bifrost, table_name),
            ))
            .expect("concurrent binding");
            let backend = BackendConfig::Local {
                root: self.root.path().to_path_buf(),
            };
            let catalog = BifrostCatalog::new(
                self.pg.catalog_dsn().expose_secret(),
                &backend,
                self.pg.vala_postgres().clone(),
            )
            .await
            .expect("concurrent production catalog");
            catalog
                .create_table(CreateTableRequest {
                    table: binding.table_ref.clone(),
                    user_fields: vec![Field::new("value", DataType::Int64, false)],
                    tenant: binding.tenant,
                    physical_layout: None,
                    audit: None,
                })
                .await
                .expect("concurrent table registration");
            let table = self
                .catalog
                .load_table(&binding.table_ident())
                .await
                .expect("concurrent registered table");
            let action = Transaction::new(&table)
                .update_table_properties()
                .set("write.target-file-size-bytes".to_owned(), "3600".to_owned());
            ApplyTransactionAction::apply(action, Transaction::new(&table))
                .expect("concurrent target property action")
                .commit(self.catalog.as_ref())
                .await
                .expect("concurrent target property commit");
            self.seed_files_for_binding(&binding, 100, 2, true).await;
            binding
        }

        /// Delete this fixture table's staging history while preserving its registration.
        ///
        /// # Panics
        ///
        /// Panics when the fixture-owner test query cannot remove the exact
        /// tenant/table history needed to exercise a catalog-only roster.
        async fn delete_file_list_history(&self) -> u64 {
            sqlx::query(
                "DELETE FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3",
            )
            .bind(self.tenant.as_uuid())
            .bind(&self.binding.logical_namespace)
            .bind(&self.binding.table_name)
            .execute(
                &self
                    .pg
                    .superuser_pool()
                    .await
                    .expect("fixture owner pool"),
            )
            .await
            .expect("delete fixture file-list history")
            .rows_affected()
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
            self.append_seed_manifest_at(
                table,
                index,
                chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
                    .expect("fixed event time")
                    .into(),
            )
            .await;
        }

        /// Append one seed object with an exact event-time partition.
        ///
        /// This lets scheduler integration tests add a newer tail snapshot
        /// without making that tail an eligible historical replacement input.
        ///
        /// # Panics
        ///
        /// Panics when fixture metadata cannot form a valid complete Iceberg
        /// data-file entry or the catalog rejects the test append.
        async fn append_seed_manifest_at(
            &self,
            table: &iceberg::table::Table,
            index: i64,
            event_time: chrono::DateTime<chrono::Utc>,
        ) {
            let object_path = format!("{}/input-{index}.parquet", self.binding.object_prefix);
            let catalog_path = format!(
                "{}/input-{index}.parquet",
                table.metadata().location().trim_end_matches('/')
            );
            self.append_seed_manifest_paths_at(table, &object_path, catalog_path, event_time)
                .await;
        }

        /// Append one seed object under Forge's current writer-recipe layout.
        ///
        /// The object is encoded at its final identity so its footer and catalog
        /// path agree while live discovery classifies it by right size instead
        /// of as an obsolete-writer singleton.
        ///
        /// # Panics
        ///
        /// Panics when the seeded object is absent or its manifest cannot commit.
        async fn append_current_recipe_seed_manifest(
            &self,
            table: &iceberg::table::Table,
            index: i64,
        ) {
            let object_path = format!(
                "{}/data/forge/bifrost-writer-v2/fixture-{index}.parquet",
                self.binding.object_prefix
            );
            let catalog_path = format!(
                "{}/data/forge/bifrost-writer-v2/fixture-{index}.parquet",
                table.metadata().location().trim_end_matches('/')
            );
            self.append_seed_manifest_paths_at(
                table,
                &object_path,
                catalog_path,
                chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
                    .expect("fixed event time")
                    .into(),
            )
            .await;
        }

        /// Append one physical seed path with a caller-selected catalog identity.
        ///
        /// # Panics
        ///
        /// Panics when object metadata, Iceberg identity construction, or the
        /// catalog commit fails.
        async fn append_seed_manifest_paths_at(
            &self,
            table: &iceberg::table::Table,
            object_path: &str,
            catalog_path: String,
            event_time: chrono::DateTime<chrono::Utc>,
        ) {
            let size = self
                .staging
                .stat(object_path)
                .await
                .expect("seed object metadata")
                .content_length();
            let event_time_id = table
                .metadata()
                .current_schema()
                .field_by_name(WYRD_EVENT_TIME)
                .expect("event time field")
                .id;
            let base = event_time.timestamp_micros();
            let partition_value = registered_granularity(table)
                .bucket(event_time)
                .expect("a fixture event time always buckets")
                .iceberg_transform_value();
            let data_file = DataFileBuilder::default()
                .content(DataContentType::Data)
                .file_path(catalog_path)
                .file_format(DataFileFormat::Parquet)
                .partition(Struct::from_iter([Some(Literal::int(partition_value))]))
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

        /// Set the live right-size target used by a scheduler integration proof.
        ///
        /// # Panics
        ///
        /// Panics when the catalog cannot commit the fixture-only table-property update.
        async fn set_live_target_file_size(&self, table: &iceberg::table::Table, bytes: u64) {
            let action = Transaction::new(table)
                .update_table_properties()
                .set("write.target-file-size-bytes".to_owned(), bytes.to_string());
            ApplyTransactionAction::apply(action, Transaction::new(table))
                .expect("live target property action")
                .commit(self.catalog.as_ref())
                .await
                .expect("live target property commit");
        }

        /// Build one deterministic two-file live group and return its exact debt.
        ///
        /// # Panics
        ///
        /// Panics when seed encoding, catalog commits, manifest discovery, or
        /// checked debt accounting fails to establish the fixture invariant.
        async fn prepare_two_file_live_debt(&self) -> (u64, u64) {
            self.seed_current_recipe_files(2, true).await;
            let table = self
                .catalog
                .load_table(&self.binding.table_ident())
                .await
                .expect("empty convergence table");
            self.set_live_target_file_size(&table, 100_000_000).await;
            let table = self
                .catalog
                .load_table(&self.binding.table_ident())
                .await
                .expect("target-sized convergence table");
            self.append_current_recipe_seed_manifest(&table, 0).await;
            let table = self
                .catalog
                .load_table(&self.binding.table_ident())
                .await
                .expect("first convergence snapshot");
            self.append_current_recipe_seed_manifest(&table, 1).await;
            let table = self
                .catalog
                .load_table(&self.binding.table_ident())
                .await
                .expect("two-file convergence snapshot");
            assert_eq!(self.delete_file_list_history().await, 2);
            let live = self
                .forge
                .discover_live_rewrites_for_test(&self.binding, &table, chrono::Utc::now())
                .await
                .expect("two-file live debt");
            let [group] = live.groups_for_test() else {
                panic!("expected exactly one live rewrite group: {live:?}");
            };
            let files =
                u64::try_from(group.files_for_test().len()).expect("live file count fits u64");
            let bytes = group
                .files_for_test()
                .iter()
                .try_fold(0_u64, |total, file| {
                    total.checked_add(file.file_size_bytes_for_test())
                })
                .expect("live file bytes fit u64");
            assert_eq!(files, 2);
            assert!(bytes > 0);
            (files, bytes)
        }
    }

    /// The single event time every seeded fixture row carries.
    ///
    /// Seed rows, reconciliation stamps, and hand-built audit details all have
    /// to name the same partition, so they all derive it from this instant
    /// rather than each restating a boundary.
    ///
    /// # Panics
    ///
    /// Panics only if this literal stops being a valid RFC 3339 instant.
    fn fixture_seed_event_time() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
            .expect("the fixture seed instant is valid RFC 3339")
            .into()
    }

    /// The partition every seeded fixture row lands in for one registered table.
    ///
    /// # Panics
    ///
    /// Panics when the table is not registered or its spec is not the Bifrost
    /// recipe.
    async fn fixture_seed_partition(
        fixture: &Fixture,
    ) -> vala_bifrost_redux::catalog::layout::TimePartition {
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("registered fixture table");
        registered_granularity(&table)
            .bucket(fixture_seed_event_time())
            .expect("a fixture event time always buckets")
    }

    /// Reads the exact time granularity a registered fixture table declares.
    ///
    /// Fixture rows and hand-built Iceberg data files must land in the same
    /// partition the catalog itself derived at registration, so both read the
    /// granularity back off the table's own partition spec rather than
    /// restating it.
    ///
    /// # Panics
    ///
    /// Panics when the spec does not carry exactly one time transform, which
    /// only happens if registration stopped emitting the Bifrost recipe.
    fn registered_granularity(
        table: &iceberg::table::Table,
    ) -> vala_bifrost_redux::catalog::layout::TimeGranularity {
        use vala_bifrost_redux::catalog::layout::TimeGranularity;
        let [field] = table.metadata().default_partition_spec().fields() else {
            panic!("the Bifrost recipe declares exactly one partition field");
        };
        match field.transform {
            Transform::Hour => TimeGranularity::Hour,
            Transform::Day => TimeGranularity::Day,
            other => panic!("Bifrost partitions only hourly or daily, found {other:?}"),
        }
    }

    /// Return the physical seed identity selected for one fixture file.
    fn seed_object_path(
        binding: &TenantTableBinding,
        index: i64,
        current_recipe_layout: bool,
    ) -> String {
        if current_recipe_layout {
            format!(
                "{}/data/forge/bifrost-writer-v2/fixture-{index}.parquet",
                binding.object_prefix
            )
        } else {
            format!("{}/input-{index}.parquet", binding.object_prefix)
        }
    }

    /// Validate the field metrics emitted for one committed Parquet file.
    fn assert_output_metrics(data_file: &iceberg::spec::DataFile) {
        assert_eq!(
            data_file.file_format(),
            iceberg::spec::DataFileFormat::Parquet
        );
        assert!(data_file.file_size_in_bytes() > 0);
        // One user column plus the nine managed columns appended by
        // `with_managed_columns`; every leaf column carries value, size, and
        // null-count metrics. Fields 2 and 3 (`run_id`, `card_uid`) are the two
        // nullable correlation columns and are entirely null in this fixture, so
        // they carry no lower/upper bounds while the eight non-null columns do.
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

    /// Read one task's durable state and the tenant's active-claim count.
    ///
    /// The active-claim count mirrors the `forge_active_claims` metric: it
    /// counts rows in the pre-terminal `claimed`, `running`, and `prepared`
    /// states and excludes released `retryable` rows, so a clean shutdown drain
    /// is observable as the count falling to zero.
    ///
    /// # Panics
    ///
    /// Panics when either tenant-scoped query fails.
    async fn task_state_and_active_claims(fixture: &Fixture, task_id: uuid::Uuid) -> (String, i64) {
        let state: String =
            sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("task state");
        let active: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.forge_tasks \
             WHERE data_tenant_id=$1 AND state IN ('claimed','running','prepared')",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("active-claim count");
        (state, active)
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

    /// Builds exact never-published generation evidence for orphan-GC tests.
    fn reset_detail_for_output(
        operation_id: uuid::Uuid,
        resource: &str,
        phase: ForgeCompactionPhase,
        output: &str,
    ) -> AuditDetail {
        AuditDetail::ForgeCompaction {
            operation_id,
            phase,
            group: resource.to_owned(),
            input_file_ids: vec![uuid::Uuid::now_v7()],
            input_paths: vec![StoragePath::new("staging/input.parquet").expect("input")],
            output_paths: vec![StoragePath::new(output.to_owned()).expect("output")],
            snapshot_id: None,
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

    /// Proves terminal file-list history neither protects an expired object nor
    /// is required to reclaim it.
    ///
    /// Physical reclamation of a never-published Forge generation belongs to
    /// `orphan_gc` alone, gated on refreshed catalog, non-terminal `file_list`,
    /// and open-operation protection plus the TTL floor under the exact table
    /// lease. A terminal `file_list` row is none of those, so it must neither
    /// suppress the delete nor stand in for the protection set: one pass
    /// reclaims the aged, unreferenced generation without any terminal operation
    /// evidence naming it, which is what makes an attempt that died before its
    /// `Prepared` transition reclaimable at all.
    ///
    /// # Panics
    ///
    /// Panics when the real catalog roster, tenant row, object store, or Forge
    /// GC workflow violates exact orphan provenance.
    #[tokio::test]
    async fn terminal_file_list_history_does_not_protect_expired_object() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                min_files: 10,
                orphan_gc_ttl: Duration::from_millis(1),
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
        )
        .await;
        let path = deterministic_output_path_for_test(
            &fixture.binding.object_prefix,
            uuid::Uuid::now_v7(),
            0,
        );
        fixture
            .staging
            .write(&path, Buffer::from(vec![1_u8]))
            .await
            .expect("orphan object");
        let now = chrono::Utc::now();
        let orphan_partition = vala_bifrost_redux::catalog::layout::TimeGranularity::Day
            .bucket(now)
            .expect("the current instant always buckets");
        sqlx::query(
            "INSERT INTO vala.file_list (
                id, data_tenant_id, namespace, table_name, file_path, file_size,
                row_count, min_event_time, max_event_time,
                partition_granularity, partition_start,
                node_id, writer_epoch, wal_lsn_min, wal_lsn_max,
                compacted, committed_snapshot_id
             ) VALUES ($1,$2,$3,$4,$5,1,1,$6,$6,$7,$8,$9,1,1,2,true,1)",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .bind(&path)
        .bind(now)
        .bind(orphan_partition.granularity_str())
        .bind(orphan_partition.start_utc())
        .bind(uuid::Uuid::now_v7())
        .execute(
            &fixture
                .pg
                .superuser_pool()
                .await
                .expect("fixture owner pool"),
        )
        .await
        .expect("terminal staging history");
        tokio::time::sleep(Duration::from_millis(5)).await;

        let deleted = fixture
            .forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("Forge GC pass");
        assert_eq!(
            deleted, 1,
            "terminal file-list history must not suppress orphan reclamation"
        );
        assert!(fixture.staging.stat(&path).await.is_err());
    }

    /// Delegating catalog that counts `load_table` and can fail it on demand.
    ///
    /// Interposed over the production catalog through the fixture's catalog
    /// decorator. `load_table_calls` counts every table load Forge performs
    /// during a bounded orphan-GC run so a test can prove the maintenance
    /// protection set is loaded a bounded number of times per batch rather than
    /// once per candidate; `fail_load` makes the same seam fail so a test can
    /// prove the run fails closed and deletes nothing when protection cannot be
    /// established.
    #[derive(Debug)]
    struct ProbeCatalog {
        /// Production catalog receiving every delegated operation.
        inner: Arc<dyn Catalog>,
        /// Count of `load_table` calls observed since construction.
        load_table_calls: Arc<AtomicUsize>,
        /// When set, `load_table` fails instead of delegating.
        fail_load: Arc<AtomicBool>,
    }

    #[async_trait::async_trait]
    impl Catalog for ProbeCatalog {
        async fn list_namespaces(
            &self,
            parent: Option<&NamespaceIdent>,
        ) -> iceberg::Result<Vec<NamespaceIdent>> {
            self.inner.list_namespaces(parent).await
        }

        async fn create_namespace(
            &self,
            namespace: &NamespaceIdent,
            properties: HashMap<String, String>,
        ) -> iceberg::Result<Namespace> {
            self.inner.create_namespace(namespace, properties).await
        }

        async fn get_namespace(&self, namespace: &NamespaceIdent) -> iceberg::Result<Namespace> {
            self.inner.get_namespace(namespace).await
        }

        async fn namespace_exists(&self, namespace: &NamespaceIdent) -> iceberg::Result<bool> {
            self.inner.namespace_exists(namespace).await
        }

        async fn update_namespace(
            &self,
            namespace: &NamespaceIdent,
            properties: HashMap<String, String>,
        ) -> iceberg::Result<()> {
            self.inner.update_namespace(namespace, properties).await
        }

        async fn drop_namespace(&self, namespace: &NamespaceIdent) -> iceberg::Result<()> {
            self.inner.drop_namespace(namespace).await
        }

        async fn list_tables(
            &self,
            namespace: &NamespaceIdent,
        ) -> iceberg::Result<Vec<TableIdent>> {
            self.inner.list_tables(namespace).await
        }

        async fn create_table(
            &self,
            namespace: &NamespaceIdent,
            creation: TableCreation,
        ) -> iceberg::Result<Table> {
            self.inner.create_table(namespace, creation).await
        }

        async fn load_table(&self, table: &TableIdent) -> iceberg::Result<Table> {
            self.load_table_calls.fetch_add(1, Ordering::AcqRel);
            if self.fail_load.load(Ordering::Acquire) {
                return Err(iceberg::Error::new(
                    iceberg::ErrorKind::Unexpected,
                    "injected orphan-GC protection load failure",
                ));
            }
            self.inner.load_table(table).await
        }

        async fn drop_table(&self, table: &TableIdent) -> iceberg::Result<()> {
            self.inner.drop_table(table).await
        }

        async fn purge_table(&self, table: &TableIdent) -> iceberg::Result<()> {
            self.inner.purge_table(table).await
        }

        async fn table_exists(&self, table: &TableIdent) -> iceberg::Result<bool> {
            self.inner.table_exists(table).await
        }

        async fn rename_table(&self, src: &TableIdent, dest: &TableIdent) -> iceberg::Result<()> {
            self.inner.rename_table(src, dest).await
        }

        async fn register_table(
            &self,
            table: &TableIdent,
            metadata_location: String,
        ) -> iceberg::Result<Table> {
            self.inner.register_table(table, metadata_location).await
        }

        async fn update_table(&self, commit: TableCommit) -> iceberg::Result<Table> {
            self.inner.update_table(commit).await
        }
    }

    /// Writes one aged, Reset-evidenced orphan object and returns its object path.
    ///
    /// Mirrors the single-orphan seeding used by
    /// [`terminal_file_list_history_does_not_protect_expired_object`]: a fresh
    /// staging object plus a prepared/reset staging-fold operation pair under a
    /// distinct operation id, which is the positive never-published evidence the
    /// GC owner requires before deleting the object. `ordinal` keeps each seeded
    /// path distinct within one table.
    ///
    /// # Panics
    ///
    /// Panics when the staging write or either operation transition fails,
    /// because successful seeding is a fixture invariant.
    async fn seed_evidenced_orphan(fixture: &Fixture, ordinal: usize) -> String {
        let path = deterministic_output_path_for_test(
            &fixture.binding.object_prefix,
            uuid::Uuid::now_v7(),
            ordinal,
        );
        fixture
            .staging
            .write(&path, Buffer::from(vec![1_u8]))
            .await
            .expect("seed orphan object");
        let resource = format!(
            "bifrost://{}/{}/{}",
            fixture.tenant, fixture.binding.table_ref.namespace, fixture.binding.table_ref.name
        );
        let operation_id = uuid::Uuid::now_v7();
        let prepared = operation_event(
            "forge.file_compact.prepared",
            &resource,
            reset_detail_for_output(
                operation_id,
                &resource,
                ForgeCompactionPhase::Prepared,
                &path,
            ),
        );
        append_operation(fixture, ForgeOperationFamily::StagingFold, &prepared, true).await;
        let reset = operation_event(
            "forge.file_compact.reset",
            &resource,
            reset_detail_for_output(operation_id, &resource, ForgeCompactionPhase::Reset, &path),
        );
        append_operation(fixture, ForgeOperationFamily::StagingFold, &reset, false).await;
        path
    }

    /// A per-run page cap defers the unscanned tail as Partial and drains across runs.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup, seeding, or any bounded GC pass fails.
    #[tokio::test]
    async fn orphan_gc_page_cap_defers_remainder_and_drains_across_runs() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                orphan_gc_ttl: Duration::from_millis(1),
                orphan_gc_max_list_pages: 1,
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
        )
        .await;
        // One entry per page against a single-page cap means each run may only
        // scan the lexicographically first object; data-prefixed orphans sort
        // ahead of the table's metadata objects.
        fixture.reads.set_list_page_entries(1);
        let mut orphans = Vec::new();
        for ordinal in 0..3 {
            orphans.push(seed_evidenced_orphan(&fixture, ordinal).await);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;

        let first = fixture
            .forge
            .run_orphan_gc_report_for_test(&fixture.binding)
            .await
            .expect("first bounded orphan pass");
        assert!(first.partial, "page cap must defer the unscanned tail");
        assert_eq!(first.deleted, 1, "one page yields exactly one deletion");

        let mut deleted = first.deleted;
        for _ in 0..8 {
            if deleted == orphans.len() {
                break;
            }
            let report = fixture
                .forge
                .run_orphan_gc_report_for_test(&fixture.binding)
                .await
                .expect("successor bounded orphan pass");
            deleted += report.deleted;
        }
        assert_eq!(
            deleted,
            orphans.len(),
            "committed-deletion progress must drain the full set across runs"
        );
        assert_eq!(
            fixture.reads.total_delete_attempts(),
            orphans.len(),
            "each orphan is deleted exactly once across the bounded runs"
        );
        for orphan in &orphans {
            assert!(
                fixture.staging.stat(orphan).await.is_err(),
                "every drained orphan is durably removed"
            );
        }
    }

    /// An exhausted per-run wall-clock budget defers cleanly without any deletion.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup, seeding, or either GC pass fails.
    #[tokio::test]
    async fn orphan_gc_run_budget_defers_without_deletion() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                orphan_gc_ttl: Duration::from_millis(1),
                orphan_gc_run_budget: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
        )
        .await;
        let orphan = seed_evidenced_orphan(&fixture, 0).await;
        tokio::time::sleep(Duration::from_millis(5)).await;

        let budgeted = fixture
            .forge
            .run_orphan_gc_report_for_test(&fixture.binding)
            .await
            .expect("budget-bounded orphan pass");
        assert!(
            budgeted.partial,
            "an exhausted budget marks the run Partial"
        );
        assert_eq!(budgeted.deleted, 0, "no deletion happens past the budget");
        assert_eq!(
            fixture.reads.total_delete_attempts(),
            0,
            "no delete is even attempted once the budget is exhausted"
        );
        assert!(
            fixture.staging.stat(&orphan).await.is_ok(),
            "the deferred orphan survives the budgeted run intact for a successor"
        );
    }

    /// One batch loads maintenance protection a bounded number of times, not per candidate.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup, seeding, or the batched GC pass fails.
    #[tokio::test]
    async fn orphan_gc_loads_protection_once_per_batch() {
        const CANDIDATES: usize = 5;
        let load_table_calls = Arc::new(AtomicUsize::new(0));
        let fail_load = Arc::new(AtomicBool::new(false));
        let decorator_calls = Arc::clone(&load_table_calls);
        let decorator_fail = Arc::clone(&fail_load);
        let fixture = Fixture::build(
            ForgeConfig {
                orphan_gc_ttl: Duration::from_millis(1),
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
            None,
            Some(Box::new(move |inner| {
                Arc::new(ProbeCatalog {
                    inner,
                    load_table_calls: decorator_calls,
                    fail_load: decorator_fail,
                }) as Arc<dyn Catalog>
            })),
        )
        .await;
        for ordinal in 0..CANDIDATES {
            seed_evidenced_orphan(&fixture, ordinal).await;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;

        load_table_calls.store(0, Ordering::Release);
        let report = fixture
            .forge
            .run_orphan_gc_report_for_test(&fixture.binding)
            .await
            .expect("batched orphan pass");
        assert_eq!(report.deleted, CANDIDATES, "the full batch is deleted");
        assert!(!report.partial, "the batch completes in one run");
        let loads = load_table_calls.load(Ordering::Acquire);
        assert!(
            loads < CANDIDATES,
            "protection loads ({loads}) must be independent of the candidate count ({CANDIDATES})"
        );
    }

    /// A protection-load failure fails the run closed and attempts no deletion.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup or seeding fails.
    #[tokio::test]
    async fn orphan_gc_fails_closed_when_protection_load_fails() {
        let load_table_calls = Arc::new(AtomicUsize::new(0));
        let fail_load = Arc::new(AtomicBool::new(false));
        let decorator_calls = Arc::clone(&load_table_calls);
        let decorator_fail = Arc::clone(&fail_load);
        let fixture = Fixture::build(
            ForgeConfig {
                orphan_gc_ttl: Duration::from_millis(1),
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
            None,
            Some(Box::new(move |inner| {
                Arc::new(ProbeCatalog {
                    inner,
                    load_table_calls: decorator_calls,
                    fail_load: decorator_fail,
                }) as Arc<dyn Catalog>
            })),
        )
        .await;
        let orphan = seed_evidenced_orphan(&fixture, 0).await;
        tokio::time::sleep(Duration::from_millis(5)).await;

        fail_load.store(true, Ordering::Release);
        let error = fixture
            .forge
            .run_orphan_gc_report_for_test(&fixture.binding)
            .await
            .expect_err("protection load failure must fail the run closed");
        assert!(matches!(error, ForgeError::Catalog(_)), "error: {error:?}");
        assert_eq!(
            fixture.reads.total_delete_attempts(),
            0,
            "a fail-closed run attempts no deletion"
        );
        assert!(
            fixture.staging.stat(&orphan).await.is_ok(),
            "the orphan survives a fail-closed run"
        );
    }

    /// Journey: an evidenced, aged orphan is deleted while young and unevidenced
    /// objects survive one bounded orphan-GC run.
    ///
    /// Gated out of the fast lane because it sleeps past a real TTL boundary.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup, seeding, or the GC pass fails.
    #[tokio::test]
    #[ignore = "journey: real-time TTL boundary, run in the gated lane"]
    async fn orphan_gc_deletes_evidenced_past_ttl_and_preserves_young() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                orphan_gc_ttl: Duration::from_secs(1),
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
        )
        .await;
        // Evidenced and aged past the TTL: eligible for deletion.
        let evidenced = seed_evidenced_orphan(&fixture, 0).await;
        // Aged past the TTL but carries no Reset evidence: the evidence gate
        // must keep it.
        let unevidenced = deterministic_output_path_for_test(
            &fixture.binding.object_prefix,
            uuid::Uuid::now_v7(),
            1,
        );
        fixture
            .staging
            .write(&unevidenced, Buffer::from(vec![2_u8]))
            .await
            .expect("aged unevidenced object");
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        // Written after the sleep: within the TTL floor and must survive.
        let young = deterministic_output_path_for_test(
            &fixture.binding.object_prefix,
            uuid::Uuid::now_v7(),
            2,
        );
        fixture
            .staging
            .write(&young, Buffer::from(vec![3_u8]))
            .await
            .expect("young object");

        let report = fixture
            .forge
            .run_orphan_gc_report_for_test(&fixture.binding)
            .await
            .expect("journey orphan pass");
        assert_eq!(
            report.deleted, 1,
            "only the evidenced aged orphan is deleted"
        );
        assert!(
            fixture.staging.stat(&evidenced).await.is_err(),
            "the evidenced aged orphan is removed"
        );
        assert!(
            fixture.staging.stat(&unevidenced).await.is_ok(),
            "the unevidenced object survives the evidence gate"
        );
        assert!(
            fixture.staging.stat(&young).await.is_ok(),
            "the young object survives the TTL floor"
        );
    }

    /// Proves retained-snapshot traversal fails closed above its configured cap.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup or catalog commits fail, or traversal accepts
    /// more retained snapshots than configured.
    async fn assert_retained_snapshot_cap() {
        let capped = ForgeConfig {
            max_retained_snapshots_per_table: 1,
            ..ForgeConfig::default()
        };
        let fixture = Fixture::new_with_config(capped, true, 4, fixture_snapshot()).await;
        fixture.schedule_and_execute().await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("first capped table");
        fixture.seed_files_at(99, 1, true).await;
        fixture.append_seed_manifest(&table, 99).await;
        assert!(
            fixture
                .forge
                .load_maintenance_protection_for_test(&fixture.binding)
                .await
                .is_err()
        );
    }

    /// Proves the production loader traverses real retained catalog history and statistics.
    ///
    /// # Panics
    ///
    /// Panics when fixture commits fail, a real retained object is omitted, or
    /// the retained-snapshot cap does not fail closed.
    #[tokio::test]
    async fn maintenance_protection_real_catalog_inventory_and_cap() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 4, fixture_snapshot()).await;
        fixture.schedule_and_execute().await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("table with data snapshot");
        let snapshot_id = table
            .metadata()
            .current_snapshot_id()
            .expect("data snapshot id");
        let statistics_key = format!("{}/metadata/stats.puffin", fixture.binding.object_prefix);
        fixture
            .staging
            .write(&statistics_key, Buffer::from(vec![1_u8]))
            .await
            .expect("statistics object");
        let statistics = StatisticsFile {
            snapshot_id,
            statistics_path: format!("{}/metadata/stats.puffin", table.metadata().location()),
            file_size_in_bytes: 1,
            file_footer_size_in_bytes: 0,
            key_metadata: None,
            blob_metadata: Vec::new(),
        };
        let update = Transaction::new(&table)
            .update_statistics()
            .set_statistics(statistics);
        ApplyTransactionAction::apply(update, Transaction::new(&table))
            .expect("statistics action")
            .commit(fixture.catalog.as_ref())
            .await
            .expect("statistics commit");

        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("retained table");
        let protection = fixture
            .forge
            .load_maintenance_protection_for_test(&fixture.binding)
            .await
            .expect("production protection");
        let paths = protection.paths_for_test();
        assert!(paths.contains(&statistics_key));
        assert!(
            table
                .metadata()
                .metadata_log()
                .iter()
                .all(|entry| paths.iter().any(|path| entry.metadata_file.ends_with(path)))
        );
        for snapshot in table.metadata().snapshots() {
            assert!(
                paths
                    .iter()
                    .any(|path| snapshot.manifest_list().ends_with(path))
            );
            let manifests = table
                .manifest_list_reader(snapshot)
                .load()
                .await
                .expect("manifest list");
            for manifest_file in manifests.entries() {
                assert!(
                    paths
                        .iter()
                        .any(|path| manifest_file.manifest_path.ends_with(path))
                );
                let manifest = manifest_file
                    .load_manifest(table.file_io())
                    .await
                    .expect("manifest");
                for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
                    assert!(
                        paths
                            .iter()
                            .any(|path| entry.data_file().file_path().ends_with(path))
                    );
                }
            }
        }

        assert_retained_snapshot_cap().await;
    }

    /// Real Parquet inputs spill, rotate, and conserve rows under one Forge operation.
    #[tokio::test]
    async fn constrained_streaming_rewrite_respects_complete_envelope() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                max_files_per_bin: 32,
                max_files_per_tick: 32,
                max_bins_per_tick: 32,
                max_concurrent_reads: 2,
                max_memory_bytes: 256 * 1024 * 1024,
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
        )
        .await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("benchmark-shaped table");
        fixture.set_live_target_file_size(&table, 64 * 1024).await;
        // Thirty-two 100k-row files make the sort exceed the bounded 16 MiB
        // pool while exactly consuming the benchmark-shaped shared file budget.
        fixture.seed_files(32, true).await;
        vala_bifrost_redux::resources::reset_memory_peak_for_test();
        vala_bifrost_redux::forge::reset_scratch_peak_for_test();
        let outcome = fixture.schedule_and_execute().await;
        assert_eq!(outcome.tasks_enqueued, 1, "outcome: {outcome:?}");
        let envelope: (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT decoded_batch_bytes,decoded_input_bytes,sort_working_bytes,sort_merge_reservation_bytes,encoder_buffer_bytes,upload_chunk_bytes,sort_spill_bytes,output_scratch_bytes,estimated_memory_bytes,footer_encoded_bytes FROM vala.forge_tasks WHERE data_tenant_id=$1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("persisted execution envelope");
        assert_eq!(fixture.reads.whole_reads.load(Ordering::Relaxed), 0);
        assert!(fixture.reads.ranged_reads.load(Ordering::Relaxed) > 0);
        assert!(
            fixture.reads.peak_reads.load(Ordering::Relaxed)
                <= usize::try_from(envelope.1 / envelope.0).expect("reader permits")
        );
        assert!(
            fixture.reads.output_chunks.load(Ordering::Acquire) > 1,
            "a multi-output rewrite must traverse the bounded chunk seam"
        );
        assert!(
            fixture.reads.largest_output_chunk.load(Ordering::Acquire)
                <= usize::try_from(envelope.5).expect("upload term"),
            "no upload write may exceed the persisted upload term"
        );
        let peak_memory = vala_bifrost_redux::resources::memory_peak_for_test();
        assert!(
            peak_memory <= usize::try_from(envelope.8).expect("resident total"),
            "leased-pool peak must remain independent of total output: {peak_memory}"
        );
        assert_eq!(envelope.2, 2 * envelope.0 + envelope.3);
        assert_eq!(
            envelope.8,
            envelope.1 + envelope.2 + envelope.4 + envelope.5 + envelope.9
        );
        assert!(
            vala_bifrost_redux::resources::memory_consumer_peak_for_test(
                "forge-rewrite-decoded-batch"
            ) <= usize::try_from(envelope.0).expect("decoded term")
        );
        assert!(
            vala_bifrost_redux::resources::memory_consumer_peak_for_test("forge-rewrite-output")
                <= usize::try_from(envelope.4 + envelope.5).expect("output resident terms")
        );
        let sort_peak = vala_bifrost_redux::resources::memory_consumer_peak_for_test("Sort");
        assert!(
            sort_peak <= usize::try_from(envelope.2).expect("sort working term"),
            "sort peak {sort_peak} exceeds persisted term {}",
            envelope.2
        );
        assert!(
            vala_bifrost_redux::forge::scratch_peak_for_test()
                <= u64::try_from(envelope.6 + envelope.7).expect("scratch total")
        );
        let released = fixture
            .forge
            .resources_for_test()
            .snapshot()
            .expect("released resource snapshot");
        assert_eq!(released.elastic_memory_used_bytes, 0);
        assert_eq!(released.scratch_used_bytes, 0);
        assert_eq!(released.forge_reader_permits_used, 0);
        let (output_files, output_rows) = committed_output_totals(&fixture).await;
        assert!(output_files >= 2, "rotation must commit multiple outputs");
        assert_eq!(
            output_rows, 3_200_000,
            "rewrite must conserve every input row"
        );
        assert_staging_transition_parity(&fixture).await;
    }

    /// A failed output chunk aborts and retries the sealed file in one attempt.
    #[tokio::test]
    async fn streaming_output_chunk_failure_retries_without_duplicate_publication() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                max_files_per_bin: 4,
                max_files_per_tick: 4,
                max_bins_per_tick: 4,
                max_concurrent_reads: 1,
                max_memory_bytes: 256 * 1024 * 1024,
                spill_limit_bytes: 1024 * 1024 * 1024,
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
        )
        .await;
        fixture.seed_files(2, true).await;
        fixture.reads.fail_next_output_chunk();
        let snapshots_before = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("table before retry")
            .metadata()
            .snapshots()
            .count();

        let outcome = fixture.schedule_and_execute().await;

        assert_eq!(outcome.tasks_enqueued, 1, "outcome: {outcome:?}");
        assert_eq!(
            fixture.reads.output_writer_aborts.load(Ordering::Acquire),
            1
        );
        assert_eq!(
            fixture.reads.output_writer_opens.load(Ordering::Acquire),
            fixture.reads.output_put_calls() + 1,
            "the one aborted writer is reopened without duplicating a successful output"
        );
        assert!(
            fixture.reads.output_chunks.load(Ordering::Acquire) >= 2,
            "the injected failure must be followed by a same-attempt retry"
        );
        let (output_files, output_rows) = committed_output_totals(&fixture).await;
        assert!(output_files > 0, "the retry publishes its output set");
        assert_eq!(output_rows, 200_000, "the retry must conserve all rows");
        let snapshots_after = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("table after retry")
            .metadata()
            .snapshots()
            .count();
        assert_eq!(
            snapshots_after,
            snapshots_before + 1,
            "one task attempt must produce exactly one Iceberg publication"
        );
        assert_staging_transition_parity(&fixture).await;
    }

    /// One output larger than the upload bound reaches the store as many chunks.
    #[tokio::test]
    async fn scaled_upload_uses_exact_chunk_and_bounded_buffer() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                max_files_per_bin: 64,
                max_files_per_tick: 64,
                max_bins_per_tick: 64,
                max_concurrent_reads: 2,
                max_memory_bytes: 512 * 1024 * 1024,
                spill_limit_bytes: 2 * 1024 * 1024 * 1024,
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
        )
        .await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("chunking table");
        fixture
            .set_live_target_file_size(&table, 768 * 1024 * 1024)
            .await;
        fixture.seed_files(64, true).await;
        vala_bifrost_redux::resources::reset_memory_peak_for_test();

        let outcome = fixture.schedule_and_execute().await;

        assert_eq!(outcome.tasks_enqueued, 1, "outcome: {outcome:?}");
        assert!(
            fixture.reads.output_put_calls() > 1,
            "a target above the physical cap must rotate under one operation"
        );
        assert!(
            fixture.reads.output_chunks.load(Ordering::Acquire) > 1,
            "one output above 8 MiB must use multiple writes"
        );
        assert!(fixture.reads.largest_output_chunk.load(Ordering::Acquire) <= 8 * 1024 * 1024);
        let peak_memory = vala_bifrost_redux::resources::memory_peak_for_test();
        assert!(
            peak_memory <= 512 * 1024 * 1024,
            "decoded, encoder, upload, and sort consumers must stay within the lease: {peak_memory}"
        );
        assert!(
            peak_memory < 768 * 1024 * 1024,
            "peak governed memory must remain below the configured output target"
        );
        let (output_files, output_rows) = committed_output_totals(&fixture).await;
        assert!(output_files > 1);
        assert_eq!(
            output_files,
            fixture.reads.output_put_calls(),
            "every capped physical output is committed under the one operation"
        );
        assert_eq!(output_rows, 6_400_000);
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
            fixture_snapshot(),
        )
        .await;

        let first = fixture.schedule_and_execute().await;
        assert_eq!(first.tasks_enqueued, 1, "first outcome: {first:?}");
        fixture.seed_files_at(100, 4, true).await;
        let second = fixture.schedule_and_execute().await;
        assert_eq!(second.tasks_enqueued, 1, "second outcome: {second:?}");

        let (expired_candidates, pending_terminals) = fixture
            .forge
            .run_snapshot_expiry_commit_boundary_for_test(&fixture.binding)
            .await
            .expect("post-commit evidence boundary");
        assert!(
            expired_candidates > 0,
            "exact cleanup candidates survive commit"
        );
        assert_eq!(
            pending_terminals, 1,
            "expiry stays Prepared before task evidence"
        );
        fixture
            .forge
            .run_snapshot_expiry_for_test(&fixture.binding)
            .await
            .expect("snapshot expiry recovery pass");
        let orphan = deterministic_output_path_for_test(
            &fixture.binding.object_prefix,
            uuid::Uuid::now_v7(),
            0,
        );
        fixture
            .staging
            .write(&orphan, Buffer::from(vec![1_u8]))
            .await
            .expect("destructive maintenance orphan");
        let resource = format!(
            "bifrost://{}/{}/{}",
            fixture.tenant, fixture.binding.table_ref.namespace, fixture.binding.table_ref.name
        );
        let operation_id = uuid::Uuid::now_v7();
        let prepared = operation_event(
            "forge.file_compact.prepared",
            &resource,
            reset_detail_for_output(
                operation_id,
                &resource,
                ForgeCompactionPhase::Prepared,
                &orphan,
            ),
        );
        append_operation(&fixture, ForgeOperationFamily::StagingFold, &prepared, true).await;
        let reset = operation_event(
            "forge.file_compact.reset",
            &resource,
            reset_detail_for_output(
                operation_id,
                &resource,
                ForgeCompactionPhase::Reset,
                &orphan,
            ),
        );
        append_operation(&fixture, ForgeOperationFamily::StagingFold, &reset, false).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        fixture
            .forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("orphan collection pass");

        assert_terminal_family_parity(&fixture, "snapshot_expire").await;
        assert_terminal_family_parity(&fixture, "orphan_gc").await;
    }

    /// Aligns the live target with the SMALLEST current file to suppress rewrite work.
    ///
    /// The right-size policy (`src/forge/right_size.rs`) treats a file as
    /// undersized below `0.75 * target` and oversized above `1.8 * target`.
    /// Anchoring the target on the largest alive file classified every smaller
    /// file undersized whenever the live spread exceeded `1.333` (max/min),
    /// leaving a live `SmallFiles` rewrite candidate that the planner claims ahead
    /// of the periodic maintenance demand. Anchoring on the smallest alive file
    /// keeps every file at or above the target, so nothing is undersized, and it
    /// admits the full `1.8` spread before any file is classified oversized —
    /// which fully drains the compaction candidate so `plan_and_claim` claims the
    /// `SnapshotExpiry` maintenance task the maintenance fixtures require.
    async fn make_current_files_right_sized(fixture: &Fixture) {
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("table");
        let snapshot = table.metadata().current_snapshot().expect("snapshot");
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .expect("manifests");
        let mut target = u64::MAX;
        for file in manifests.entries() {
            let manifest = file.load_manifest(table.file_io()).await.expect("manifest");
            for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
                target = target.min(entry.data_file().file_size_in_bytes());
            }
        }
        let target = if target == u64::MAX { 2 } else { target.max(2) };
        fixture.set_live_target_file_size(&table, target).await;
    }

    /// Appends timestamp-distinct snapshots so maintenance fault tests reach a real expiry effect.
    async fn append_expirable_history(fixture: &Fixture) {
        fixture.seed_files_at(900, 3, false).await;
        for index in 900..903 {
            tokio::time::sleep(Duration::from_millis(2)).await;
            let table = fixture
                .catalog
                .load_table(&fixture.binding.table_ident())
                .await
                .expect("maintenance history table");
            fixture.append_seed_manifest(&table, index).await;
        }
        fixture.delete_file_list_history().await;
        make_current_files_right_sized(fixture).await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("maintenance history verification table");
        let current_timestamp = table
            .metadata()
            .current_snapshot()
            .expect("maintenance history current snapshot")
            .timestamp_ms();
        assert!(
            table
                .metadata()
                .snapshots()
                .filter(|snapshot| snapshot.timestamp_ms() < current_timestamp)
                .count()
                >= 2,
            "maintenance history must contain timestamp-distinct expirable ancestors"
        );
    }

    /// Builds two committed snapshots and claims the resulting periodic maintenance task.
    async fn prepare_maintenance_claim(fixture: &Fixture) -> ForgeTaskClaim {
        fixture.schedule_and_execute().await;
        make_current_files_right_sized(fixture).await;
        append_expirable_history(fixture).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        let identity = ForgeTaskTableIdentity::new(
            "wyrd-redux",
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        )
        .expect("maintenance identity");
        ForgeTasks::new(fixture.operator_pool.clone())
            .upsert_periodic(fixture.tenant, &identity)
            .await
            .expect("periodic maintenance demand");
        let claim = fixture.plan_and_claim().await;
        assert!(matches!(
            claim.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry)
        ));
        claim
    }

    /// A due maintenance trigger leads planning even while a live compaction
    /// candidate is present, proving snapshot expiry is not starved by sustained
    /// compaction load (AC1).
    ///
    /// Warm-up runs two ordinary compaction-and-execute cycles under a
    /// one-nanosecond trigger interval: during each warm-up plan the table holds
    /// at most `retain_last` snapshots, so `commits_since_last_maintenance` is
    /// zero and the trigger does not fire mid-warm-up. Only after the second
    /// commit does the interval arm become due. Fresh staging files are then
    /// seeded and deliberately NOT right-sized away (the contrast with
    /// [`prepare_maintenance_claim`], which drains compaction so expiry wins
    /// through the both-empty fallback), so a live compaction candidate coexists
    /// with the due maintenance candidate on the planned tick; the claimed
    /// strategy proves maintenance took the tick's single slot ahead of it.
    #[tokio::test]
    async fn maintenance_leads_planning_amid_compaction_backlog() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        fixture.schedule_and_execute().await;
        fixture.seed_files_at(100, 4, true).await;
        fixture.schedule_and_execute().await;
        // Fresh, undrained staging files keep a live compaction candidate for
        // this tick; the maintenance-family fixtures right-size to drain it, this
        // one does not, so both candidate classes are present when planning runs.
        fixture.seed_files_at(200, 4, true).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        let identity = ForgeTaskTableIdentity::new(
            "wyrd-redux",
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        )
        .expect("maintenance identity");
        ForgeTasks::new(fixture.operator_pool.clone())
            .upsert_periodic(fixture.tenant, &identity)
            .await
            .expect("periodic maintenance demand");
        let claim = fixture.plan_and_claim().await;
        assert!(
            matches!(
                claim.strategy,
                ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry)
            ),
            "a due maintenance trigger must lead planning ahead of a live compaction candidate: {:?}",
            claim.strategy
        );
    }

    /// A durable no-op acknowledgement prevents a still-due trigger from starving compaction.
    #[tokio::test]
    async fn acknowledged_noop_maintenance_allows_staging_debt_to_converge() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        fixture.schedule_and_execute().await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("no-op table");
        let snapshot = table.metadata().current_snapshot().expect("no-op snapshot");
        let manifest = format!("{}/metadata/missing.avro", table.metadata().location());
        let mut no_op_task = fixture.durable_task(
            ForgeTaskStrategy::SnapshotExpiry,
            ForgeTaskPlan {
                version: FORGE_TASK_PAYLOAD_VERSION,
                inputs: vec![manifest],
                parameters: serde_json::json!({"kind":"maintenance","trigger_commit_count":0}),
            },
            219,
        );
        no_op_task.base_snapshot_id = snapshot.snapshot_id();
        ForgeTasks::new(fixture.operator_pool.clone())
            .enqueue(&no_op_task)
            .await
            .expect("no-op task");
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("no-op claim")
            .expect("no-op task claim");
        fixture
            .sibling_worker_with_config(ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_hours(24 * 365),
                orphan_gc_ttl: Duration::from_nanos(1),
                ..ForgeConfig::default()
            })
            .execute_claim(claim, &CancellationToken::new())
            .await
            .expect("production no-op settlement");
        fixture.seed_files_at(500, 4, true).await;

        let mut remaining = 4_i64;
        for _ in 0..4 {
            fixture.schedule_and_execute().await;
            remaining = sqlx::query_scalar("SELECT count(*) FROM vala.file_list WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 AND NOT compacted")
                .bind(fixture.tenant.as_uuid())
                .bind(&fixture.binding.logical_namespace)
                .bind(&fixture.binding.table_name)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("remaining staging debt");
            if remaining == 0 {
                break;
            }
        }
        assert_eq!(
            remaining, 0,
            "acknowledged no-op must not starve staging debt"
        );
    }

    /// A table whose manifest history exceeds `max_concurrent_reads` plans a
    /// schedulable expiry on successive bounded ticks instead of wedging in the
    /// terminal `Unschedulable` lane or pinning one idempotent plan (AC3).
    ///
    /// The regression is the deep-history wedge, which lives in the planning
    /// layer: before the parallelism bound, `maintenance_candidate` set
    /// `parallelism = manifest_count`, so a table with more retained manifests
    /// than `max_concurrent_reads` (here `1`) planned a candidate whose
    /// parallelism exceeded `max_parallelism` and whose input count was not one,
    /// classifying it `Unschedulable` on every tick — a permanent stall. This
    /// proof is deliberately plan-only (`schedule_once`, never executed): the
    /// wedge is the capacity classification, and executing synthetic
    /// fast-appended history would exercise unrelated expiry ancestry mechanics.
    /// The first tick must plan a schedulable expiry (`unschedulable == 0`,
    /// `tasks_enqueued == 1`); a second tick, after fresh manifest history moves
    /// the current snapshot, must again plan schedulable and advance to a distinct
    /// `plan_hash`, proving successive bounded ticks progress rather than pinning
    /// the retired plan.
    #[tokio::test]
    async fn deep_manifest_history_progresses_without_wedging() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                max_concurrent_reads: 1,
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            true,
            9,
            fixture_snapshot(),
        )
        .await;
        // Build a deep manifest history directly, one fast-append per seeded
        // object, so the current snapshot's manifest list has six entries —
        // strictly more than `max_concurrent_reads`.
        for index in 0..6 {
            let table = fixture
                .catalog
                .load_table(&fixture.binding.table_ident())
                .await
                .expect("historical table load");
            fixture.append_seed_manifest(&table, index).await;
        }
        // Drop the staging file-list rows so only the manifest history drives
        // planning; no compaction candidate competes with maintenance here.
        fixture.delete_file_list_history().await;
        let snapshots_before = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("deep-history table")
            .metadata()
            .snapshots()
            .count();
        // Deeper than the configured `max_concurrent_reads` of 1, so the
        // pre-bound parallelism would have exceeded `max_parallelism`.
        assert!(
            snapshots_before > 1,
            "fixture must build manifest history deeper than max_concurrent_reads: {snapshots_before}"
        );
        let identity = ForgeTaskTableIdentity::new(
            "wyrd-redux",
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        )
        .expect("deep-history identity");
        let tasks = ForgeTasks::new(fixture.operator_pool.clone());
        let stop = CancellationToken::new();
        tokio::time::sleep(Duration::from_millis(5)).await;
        tasks
            .upsert_periodic(fixture.tenant, &identity)
            .await
            .expect("first periodic maintenance demand");
        let first = ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
            .expect("first-tick scheduler")
            .schedule_once(&stop)
            .await
            .expect("first bounded planning tick");
        assert_eq!(
            first.unschedulable, 0,
            "deep manifest history must not wedge in the Unschedulable lane: {first:?}"
        );
        assert_eq!(
            first.tasks_enqueued, 1,
            "the first bounded tick must plan exactly one schedulable expiry: {first:?}"
        );

        // Append fresh manifest history so the second bounded tick plans from a
        // new current snapshot rather than re-deriving the first plan.
        for index in 6..9 {
            let table = fixture
                .catalog
                .load_table(&fixture.binding.table_ident())
                .await
                .expect("second-round table load");
            fixture.append_seed_manifest(&table, index).await;
        }
        fixture.delete_file_list_history().await;
        make_current_files_right_sized(&fixture).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        tasks
            .upsert_periodic(fixture.tenant, &identity)
            .await
            .expect("second periodic maintenance demand");
        let second = ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
            .expect("second-tick scheduler")
            .schedule_once(&stop)
            .await
            .expect("second bounded planning tick");
        assert_eq!(
            second.unschedulable, 0,
            "the second bounded tick must also be schedulable: {second:?}"
        );
        assert_eq!(
            second.tasks_enqueued, 1,
            "the second bounded tick must plan exactly one schedulable expiry: {second:?}"
        );
        let distinct_plans: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT plan_hash) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND strategy='snapshot_expiry'",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("distinct maintenance plans");
        assert_eq!(
            distinct_plans, 2,
            "successive bounded ticks must plan distinct expiries, not pin one idempotent plan"
        );
    }

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

    /// Authority loss injected at a maintenance catalog boundary.
    ///
    /// Graceful shutdown is no longer a member: it no longer reaches the
    /// maintenance authority token, so its complete-through behavior is proven
    /// separately by [`assert_maintenance_shutdown_completes_through`].
    #[derive(Clone, Copy)]
    enum MaintenanceAuthorityLoss {
        /// Expire the durable task claim and let its heartbeat cancel work.
        Claim,
        /// Replace the table lease generation and let its heartbeat cancel work.
        TableLease,
    }

    /// Verifies that cancellation at a maintenance boundary published no durable effect.
    ///
    /// # Panics
    ///
    /// Panics when the audit, task-evidence, or catalog queries fail, or when
    /// cancellation left a durable transition, cleanup evidence, changed table
    /// metadata, an output write, or an object-deletion attempt behind.
    async fn assert_cancelled_maintenance_state(
        fixture: &Fixture,
        task_id: uuid::Uuid,
        metadata_before: &str,
        snapshot_before: Option<i64>,
        output_puts_before: usize,
        deletes_before: usize,
    ) {
        let (states, audits) =
            family_transition_counts(fixture, "snapshot_expire", "forge.snapshot_expire.").await;
        assert_eq!(states, 0);
        assert_eq!(audits, 0);
        let evidence: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT evidence FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("cancelled maintenance evidence");
        assert!(
            evidence
                .as_ref()
                .and_then(|value| value.get("cleanup_candidates"))
                .is_none()
        );
        let table_after = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("post-cancellation maintenance table");
        assert_eq!(
            table_after.metadata_location(),
            Some(metadata_before),
            "catalog metadata changed across the cancellation boundary"
        );
        assert_eq!(
            table_after.metadata().current_snapshot_id(),
            snapshot_before,
            "current snapshot changed across the cancellation boundary"
        );
        assert_eq!(fixture.reads.output_put_calls(), output_puts_before);
        assert_eq!(fixture.reads.total_delete_attempts(), deletes_before);
    }

    /// Exercises one deterministic maintenance boundary under every loss source.
    ///
    /// Authority loss (claim expiry, table-lease theft) and graceful shutdown
    /// now diverge: the heartbeat cancels the maintenance authority token only
    /// on authority loss, so those two variants still stop at the boundary with
    /// nothing durable crossing it, while shutdown no longer reaches that token
    /// and the operation completes through to a recoverable terminal.
    async fn assert_maintenance_boundary_loss(expiry_submission: bool) {
        for loss in [
            MaintenanceAuthorityLoss::Claim,
            MaintenanceAuthorityLoss::TableLease,
        ] {
            Box::pin(assert_maintenance_authority_loss(expiry_submission, loss)).await;
        }
        Box::pin(assert_maintenance_shutdown_completes_through(
            expiry_submission,
        ))
        .await;
    }

    /// Stops one maintenance boundary on genuine authority loss with no durable effect.
    ///
    /// The armed boundary is driven to ARRIVE (raced against the worker join so a
    /// premature return surfaces its `Result` immediately instead of a
    /// misattributed timeout), then the chosen authority is revoked; the
    /// heartbeat propagates the loss to the maintenance authority token, the
    /// gate releases via cancellation, and the claim retains no effect past the
    /// boundary.
    async fn assert_maintenance_authority_loss(
        expiry_submission: bool,
        loss: MaintenanceAuthorityLoss,
    ) {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                manifest_rewrite_enabled: true,
                manifest_rewrite_min_count: 2,
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        let claim = prepare_maintenance_claim(&fixture).await;
        let task_id = claim.task_id;
        let table_before = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("pre-cancellation maintenance table");
        let metadata_before = table_before
            .metadata_location()
            .expect("pre-cancellation metadata location")
            .to_owned();
        let snapshot_before = table_before.metadata().current_snapshot_id();
        let output_puts_before = fixture.reads.output_put_calls();
        let deletes_before = fixture.reads.total_delete_attempts();
        let controls = fixture.forge.maintenance_controls_for_test();
        if expiry_submission {
            controls.arm_expiry_submission();
        } else {
            controls.arm_manifest_submission();
        }
        let stop = CancellationToken::new();
        let execution = tokio::spawn({
            let worker = fixture.worker.clone();
            let stop = stop.clone();
            async move { worker.execute_claim(claim, &stop).await }
        });
        tokio::pin!(execution);
        let arrival = async {
            if expiry_submission {
                controls.wait_expiry_submission().await;
            } else {
                controls.wait_manifest_submission().await;
            }
        };
        tokio::pin!(arrival);
        tokio::select! {
            () = &mut arrival => {}
            result = &mut execution => {
                panic!("maintenance returned before catalog boundary: {result:?}")
            }
        }
        match loss {
            MaintenanceAuthorityLoss::Claim => {
                sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1")
                    .bind(task_id).execute(fixture.operator_pool.pool()).await.expect("expire maintenance claim");
            }
            MaintenanceAuthorityLoss::TableLease => {
                sqlx::query("UPDATE vala.maintenance_leases SET owner=$2,fencing_token=fencing_token+1 WHERE lease_key=$1")
                    .bind(forge_lease_key(fixture.tenant, &fixture.binding.logical_namespace, &fixture.binding.table_name))
                    .bind(uuid::Uuid::now_v7()).execute(fixture.operator_pool.pool()).await.expect("steal maintenance lease");
            }
        }
        let result = (&mut execution).await.expect("maintenance worker join");
        assert!(result.is_err(), "authority loss must stop maintenance");
        assert_cancelled_maintenance_state(
            &fixture,
            task_id,
            &metadata_before,
            snapshot_before,
            output_puts_before,
            deletes_before,
        )
        .await;
    }

    /// Recovers an expired maintenance claim on a fresh successor worker and
    /// asserts the recovery reaches a KNOWN terminal without re-committing.
    ///
    /// Expires the durable claim, runs one successor pass, and proves the
    /// recovered metadata location is byte-identical to `operation_location`
    /// (no second manifest commit) while the durable task state advances to
    /// `succeeded`. Shared by the shutdown and interrupted-attempt scenarios so
    /// each caller stays a focused, readable assertion.
    ///
    /// # Panics
    /// Panics when the successor worker cannot be built, the recovery pass does
    /// not report progress, the recovered location differs from
    /// `operation_location`, or the durable state is not `succeeded`. These are
    /// test-environment invariants.
    async fn assert_successor_recovers_to_known_terminal(
        fixture: &Fixture,
        task_id: uuid::Uuid,
        operation_location: &str,
    ) {
        sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1")
            .bind(task_id).execute(fixture.operator_pool.pool()).await.expect("expire shutdown task claim");
        let successor = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("successor worker");
        assert!(
            successor
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("shutdown maintenance recovery")
        );
        let recovered_location = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("recovered maintenance table")
            .metadata_location()
            .expect("recovered metadata location")
            .to_owned();
        assert_eq!(
            recovered_location, operation_location,
            "successor must not perform a second manifest commit"
        );
        let terminal: String =
            sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("recovered terminal state");
        assert_eq!(
            terminal, "succeeded",
            "successor must drive the claim to a known terminal"
        );
    }

    /// A graceful shutdown does not cancel an in-flight maintenance operation.
    ///
    /// Shutdown cancels only the shutdown-sensitive `operation_stop`; the
    /// maintenance authority token is untouched, so once the boundary is
    /// released the operation runs to its own outcome. The post-effect drain
    /// then returns without terminalizing the attempt, leaving the durable
    /// effect committed exactly once and the claim recoverable. A successor
    /// pass reaches the KNOWN terminal without a second manifest commit.
    async fn assert_maintenance_shutdown_completes_through(expiry_submission: bool) {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                manifest_rewrite_enabled: true,
                manifest_rewrite_min_count: 2,
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        let claim = prepare_maintenance_claim(&fixture).await;
        let task_id = claim.task_id;
        let table_before = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("pre-shutdown maintenance table");
        let snapshot_before = table_before.metadata().current_snapshot_id();
        let controls = fixture.forge.maintenance_controls_for_test();
        if expiry_submission {
            controls.arm_expiry_submission();
        } else {
            controls.arm_manifest_submission();
        }
        let stop = CancellationToken::new();
        let execution = tokio::spawn({
            let worker = fixture.worker.clone();
            let stop = stop.clone();
            async move { worker.execute_claim(claim, &stop).await }
        });
        tokio::pin!(execution);
        let arrival = async {
            if expiry_submission {
                controls.wait_expiry_submission().await;
            } else {
                controls.wait_manifest_submission().await;
            }
        };
        tokio::pin!(arrival);
        tokio::select! {
            () = &mut arrival => {}
            result = &mut execution => {
                panic!("maintenance returned before catalog boundary: {result:?}")
            }
        }
        stop.cancel();
        if expiry_submission {
            controls.release_expiry_submission();
        } else {
            controls.release_manifest_submission();
        }
        let result = (&mut execution).await.expect("maintenance worker join");
        assert!(
            result.is_err(),
            "post-effect shutdown retains the claim for recovery: {result:?}"
        );
        let operation_location = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("post-operation maintenance table")
            .metadata_location()
            .expect("post-operation metadata location")
            .to_owned();
        // (a) The maintenance operation committed exactly once through shutdown.
        let table_after_operation = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("post-operation maintenance table snapshot");
        assert_ne!(
            table_after_operation.metadata().current_snapshot_id(),
            snapshot_before,
            "maintenance must complete its commit through graceful shutdown"
        );
        // (b)/(c) A successor recovers the claim to a KNOWN terminal without a
        // second manifest commit.
        assert_successor_recovers_to_known_terminal(&fixture, task_id, &operation_location).await;
    }

    /// Manifest submission stops before expiry under claim, lease, and shutdown loss.
    #[tokio::test]
    async fn maintenance_manifest_submission_cancellation_matrix_is_effect_ordered() {
        Box::pin(assert_maintenance_boundary_loss(false)).await;
    }

    /// Expiry submission preserves only its Prepared evidence under every cancellation source.
    #[tokio::test]
    async fn maintenance_expiry_submission_cancellation_matrix_is_effect_ordered() {
        Box::pin(assert_maintenance_boundary_loss(true)).await;
    }

    /// A manifest commit that outlives its retry timeout retains the claim for recovery.
    ///
    /// This pins the timeout backstop that bounds the now-shutdown-decoupled
    /// maintenance operation: a `1ns` `iceberg_total_retry_timeout` elapses on
    /// the first poll of the real catalog commit (which must yield for IO),
    /// mapping to the unknown-acceptance `Reconciliation` and leaving nothing
    /// durable behind, so the claim stays retained for lease-based recovery.
    ///
    /// The fixture itself keeps a normal retry timeout so its `StagingFold` setup
    /// commits succeed; the tightened `1ns` timeout applies only to the sibling
    /// worker that executes the already-taken maintenance claim, so the timeout
    /// is exercised exactly where the assertion targets it — the maintenance
    /// manifest rewrite — and never corrupts claim preparation.
    #[tokio::test]
    async fn maintenance_commit_timeout_retains_claim_for_recovery() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                manifest_rewrite_enabled: true,
                manifest_rewrite_min_count: 2,
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        let claim = prepare_maintenance_claim(&fixture).await;
        let task_id = claim.task_id;
        let table_before = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("pre-timeout maintenance table");
        let metadata_before = table_before
            .metadata_location()
            .expect("pre-timeout metadata location")
            .to_owned();
        let snapshot_before = table_before.metadata().current_snapshot_id();
        let output_puts_before = fixture.reads.output_put_calls();
        let deletes_before = fixture.reads.total_delete_attempts();
        let tight = fixture.sibling_worker_with_retry_timeout(Duration::from_nanos(1));
        let stop = CancellationToken::new();
        let result = tight.execute_claim(claim, &stop).await;
        assert!(
            matches!(result, Err(ForgeError::Reconciliation { .. })),
            "manifest commit timeout must map to unknown-acceptance Reconciliation: {result:?}"
        );
        assert_cancelled_maintenance_state(
            &fixture,
            task_id,
            &metadata_before,
            snapshot_before,
            output_puts_before,
            deletes_before,
        )
        .await;
    }

    /// An accepted expiry cancelled before response processing is recovered exactly once.
    #[tokio::test]
    async fn maintenance_accepted_then_cancel_recovers_without_duplicate_catalog_effect() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        let claim = prepare_maintenance_claim(&fixture).await;
        let task_id = claim.task_id;
        let controls = fixture.forge.maintenance_controls_for_test();
        controls.arm_expiry_accepted();
        let stop = CancellationToken::new();
        let execution = tokio::spawn({
            let worker = fixture.worker.clone();
            let stop = stop.clone();
            async move { worker.execute_claim(claim, &stop).await }
        });
        tokio::time::timeout(Duration::from_secs(30), controls.wait_expiry_accepted())
            .await
            .expect("accepted expiry response");
        let accepted_location = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("accepted expiry table")
            .metadata_location()
            .expect("accepted metadata location")
            .to_owned();
        // Graceful shutdown no longer cancels an in-flight maintenance operation;
        // revoke the claim so the heartbeat propagates authority loss to the
        // maintenance authority token and releases the accepted-response gate via
        // cancellation. All assertions below are unchanged.
        sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1")
            .bind(task_id).execute(fixture.operator_pool.pool()).await.expect("expire accepted maintenance claim");
        let result = tokio::time::timeout(Duration::from_secs(30), execution)
            .await
            .expect("accepted cancellation bound")
            .expect("accepted cancellation join");
        // Authority loss is detected by the claim heartbeat, and `execute_fenced`
        // surfaces the heartbeat's fence conflict (`heartbeat_result?`) ahead of
        // the maintenance operation's own unknown-acceptance `Reconciliation`
        // (`completion?`). Because the heartbeat is the sole canceller of the
        // maintenance authority token, its fence conflict is the deterministic
        // surfaced variant here; both are non-success authority-loss outcomes
        // that retain the accepted expiry effect for exactly-once successor
        // recovery, which the assertions below prove.
        assert!(
            matches!(
                result,
                Err(ForgeError::Sql(_) | ForgeError::Reconciliation { .. })
            ),
            "interrupted accepted-expiry attempt must fail with an authority-loss error that retains the effect: {result:?}"
        );
        let (states, audits) =
            family_transition_counts(&fixture, "snapshot_expire", "forge.snapshot_expire.").await;
        assert_eq!((states, audits), (1, 1));
        sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1")
            .bind(task_id).execute(fixture.operator_pool.pool()).await.expect("expire accepted task claim");
        let successor = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("successor worker");
        assert!(
            !successor
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("accepted expiry reclaim"),
            "an expired accepted attempt is reclaimed into durable backoff"
        );
        sqlx::query(
            "UPDATE vala.forge_tasks SET next_eligible_at=statement_timestamp()-interval '1 second',ready_at=statement_timestamp()-interval '1 second' WHERE task_id=$1",
        )
        .bind(task_id)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("advance accepted recovery eligibility");
        assert!(
            successor
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("accepted expiry recovery after backoff")
        );
        let recovered_location = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("recovered expiry table")
            .metadata_location()
            .expect("recovered metadata location")
            .to_owned();
        assert_eq!(recovered_location, accepted_location);
        assert_eq!(
            family_transition_counts(&fixture, "snapshot_expire", "forge.snapshot_expire.",).await,
            (1, 2)
        );
        assert_completed_cleanup_evidence(&fixture).await;
        assert_exact_cleanup_attempts(&fixture, task_id).await;
    }

    /// Proves recovery deletes every durable cleanup candidate exactly once.
    async fn assert_exact_cleanup_attempts(fixture: &Fixture, task_id: uuid::Uuid) {
        let evidence: serde_json::Value =
            sqlx::query_scalar("SELECT evidence FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("accepted expiry cleanup evidence");
        let candidates = evidence["cleanup_candidates"]
            .as_array()
            .expect("accepted cleanup candidates");
        assert!(!candidates.is_empty());
        for candidate in candidates {
            let object_path = candidate["path"].as_str().expect("cleanup candidate path");
            assert_eq!(
                fixture.reads.delete_attempts_for(object_path),
                1,
                "cleanup candidate must be attempted exactly once: {object_path}"
            );
        }
        assert_eq!(
            fixture.reads.total_delete_attempts(),
            candidates.len(),
            "successor must not attempt deletes outside exact cleanup evidence"
        );
    }

    /// A persisted active watermark detached from every Iceberg ref fails before effects.
    #[tokio::test]
    async fn persisted_detached_watermark_rejects_expiry_before_prepared_or_cleanup() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        fixture.schedule_and_execute().await;
        fixture.seed_files_at(100, 4, true).await;
        fixture.schedule_and_execute().await;
        let (detached_id, detached_timestamp) = fixture.persist_detached_prior_snapshot().await;
        let detached_table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("persisted detached table");
        let metadata_before = detached_table
            .metadata_location()
            .expect("detached metadata location")
            .to_owned();
        let current_before = detached_table.metadata().current_snapshot_id();
        let task_id = ForgeTasks::new(fixture.operator_pool.clone())
            .enqueue(&NewForgeTask {
                base_snapshot_id: detached_id,
                ..fixture.durable_task(
                    ForgeTaskStrategy::StagingFold,
                    ForgeTaskPlan {
                        version: FORGE_TASK_PAYLOAD_VERSION,
                        inputs: vec!["detached-watermark.parquet".to_owned()],
                        parameters: serde_json::json!({"kind":"staging_fold"}),
                    },
                    221,
                )
            })
            .await
            .expect("detached watermark task");
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("detached watermark claim query")
            .expect("detached watermark claim");
        assert_eq!(claim.task_id, task_id);
        sqlx::query(
            "UPDATE vala.forge_tasks SET state='running',watermark_snapshot_id=$2,watermark_timestamp_ms=$3 WHERE task_id=$1",
        )
        .bind(task_id)
        .bind(detached_id)
        .bind(detached_timestamp)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("activate detached persisted watermark");
        let result = fixture
            .forge
            .run_snapshot_expiry_for_test(&fixture.binding)
            .await;
        assert!(
            matches!(result, Err(ForgeError::SnapshotExpiry { ref detail }) if detail.contains("detached")),
            "{result:?}"
        );
        assert_eq!(
            family_transition_counts(&fixture, "snapshot_expire", "forge.snapshot_expire.",).await,
            (0, 0)
        );
        let after = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("unchanged detached table");
        assert_eq!(after.metadata_location(), Some(metadata_before.as_str()));
        assert_eq!(after.metadata().current_snapshot_id(), current_before);
        let evidence: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT evidence FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("detached watermark cleanup evidence");
        assert!(evidence.is_none());
    }

    /// Checks completed cleanup evidence against object storage and live metadata.
    async fn assert_completed_cleanup_evidence(fixture: &Fixture) {
        let evidence: serde_json::Value = sqlx::query_scalar("SELECT evidence FROM vala.forge_tasks WHERE data_tenant_id=$1 AND catalog_name='wyrd-redux' AND namespace_name=$2 AND table_name=$3 AND strategy='snapshot_expiry' AND state='succeeded' ORDER BY updated_at DESC LIMIT 1")
            .bind(fixture.tenant.as_uuid()).bind(&fixture.binding.logical_namespace).bind(&fixture.binding.table_name)
            .fetch_one(fixture.operator_pool.pool()).await.expect("maintenance evidence");
        let candidates = evidence["cleanup_candidates"]
            .as_array()
            .expect("candidates");
        assert!(!candidates.is_empty());
        assert_eq!(
            evidence["deleted_candidate_count"].as_u64(),
            Some(candidates.len() as u64)
        );
        let paths = candidates
            .iter()
            .map(|candidate| candidate["path"].as_str().expect("path").to_owned())
            .collect::<Vec<_>>();
        assert!(paths.windows(2).all(|pair| pair[0] < pair[1]));
        for path in &paths {
            assert!(
                fixture.staging.stat(path).await.is_err(),
                "still exists: {path}"
            );
        }
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("table");
        let current = table.metadata_location().expect("metadata location");
        assert!(paths.iter().all(|path| !current.ends_with(path)));
    }

    /// Proves a later Reset-backed destructive pass remains available after takeover.
    async fn assert_later_destructive_maintenance(fixture: &Fixture, resource: &str) {
        let orphan = deterministic_output_path_for_test(
            &fixture.binding.object_prefix,
            uuid::Uuid::now_v7(),
            0,
        );
        fixture
            .staging
            .write(&orphan, Buffer::from(vec![1_u8]))
            .await
            .expect("orphan");
        let operation_id = uuid::Uuid::now_v7();
        let prepared = operation_event(
            "forge.file_compact.prepared",
            resource,
            reset_detail_for_output(
                operation_id,
                resource,
                ForgeCompactionPhase::Prepared,
                &orphan,
            ),
        );
        append_operation(fixture, ForgeOperationFamily::StagingFold, &prepared, true).await;
        let reset = operation_event(
            "forge.file_compact.reset",
            resource,
            reset_detail_for_output(operation_id, resource, ForgeCompactionPhase::Reset, &orphan),
        );
        append_operation(fixture, ForgeOperationFamily::StagingFold, &reset, false).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        fixture
            .forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("later maintenance");
        assert!(fixture.staging.stat(&orphan).await.is_err());
    }

    /// Proves nonmatching task evidence stops takeover before destructive progress.
    async fn assert_mismatched_takeover_fails_closed(
        fixture: &Fixture,
        task_id: uuid::Uuid,
        stop: &CancellationToken,
    ) -> serde_json::Value {
        let original: serde_json::Value =
            sqlx::query_scalar("SELECT evidence FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("evidence");
        let candidate = original["cleanup_candidates"][0]["path"]
            .as_str()
            .expect("candidate")
            .to_owned();
        sqlx::query("UPDATE vala.forge_tasks SET evidence=jsonb_set(jsonb_set(evidence,'{cleanup_candidates}','[]'::jsonb),'{deleted_candidate_count}','0'::jsonb) WHERE task_id=$1")
            .bind(task_id).execute(fixture.operator_pool.pool()).await.expect("mismatch");
        let worker = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("worker");
        worker
            .execute_one_for_test(stop)
            .await
            .expect_err("mismatch must fail");
        let unchanged: (String, i64, String) = sqlx::query_as("SELECT state,(evidence->>'deleted_candidate_count')::bigint,o.phase FROM vala.forge_tasks t CROSS JOIN vala.forge_operation_state o WHERE t.task_id=$1 AND o.family='snapshot_expire'")
            .bind(task_id).fetch_one(fixture.operator_pool.pool()).await.expect("state");
        assert_eq!(unchanged, ("prepared".to_owned(), 0, "prepared".to_owned()));
        assert!(fixture.staging.stat(&candidate).await.is_ok());
        original
    }

    /// Asserts the scheduled maintenance demand is a ready snapshot-expiry task,
    /// claims it, and injects a crash immediately after the Prepared write.
    ///
    /// Guards the scheduler contract that `tasks_enqueued` counts a genuinely
    /// executable `ready` row (not a terminal `unschedulable` one), confirms the
    /// claimed strategy is `SnapshotExpiry`, then arms the post-Prepared failure
    /// injection so `execute_claim` crashes after persisting Prepared evidence.
    ///
    /// # Panics
    /// Panics when the durable row is not a `ready` snapshot-expiry task, no
    /// claimable maintenance task is available, the claimed strategy differs, or
    /// the injected crash does not surface an error. These are test invariants.
    async fn claim_scheduled_expiry_demand_and_crash_after_prepared(
        fixture: &Fixture,
        stop: &CancellationToken,
    ) {
        // The enqueued demand must be a genuinely executable, ready snapshot-expiry
        // task, not a terminal `unschedulable` row still counted in `tasks_enqueued`.
        let scheduled_row: (String, String) = sqlx::query_as(
            "SELECT strategy,state FROM vala.forge_tasks WHERE data_tenant_id=$1 AND strategy='snapshot_expiry'",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("scheduled maintenance row");
        assert_eq!(
            scheduled_row,
            ("snapshot_expiry".to_owned(), "ready".to_owned())
        );
        fixture.worker.fail_after_maintenance_prepared_for_test();
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("claim maintenance task")
            .expect("maintenance task available to claim");
        assert!(
            matches!(
                claim.strategy,
                ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry)
            ),
            "claimed task must be the snapshot-expiry maintenance demand: {:?}",
            claim.strategy
        );
        fixture
            .worker
            .execute_claim(claim, stop)
            .await
            .expect_err("injected post-Prepared crash");
    }

    /// Production scheduling persists exact ordered cleanup evidence before deleting it.
    #[tokio::test]
    async fn scheduled_maintenance_deletes_only_evidenced_expired_objects() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        fixture.schedule_and_execute().await;
        make_current_files_right_sized(&fixture).await;
        append_expirable_history(&fixture).await;
        tokio::time::sleep(Duration::from_millis(5)).await;

        let identity = ForgeTaskTableIdentity::new(
            "wyrd-redux",
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        )
        .expect("maintenance identity");
        let tasks = ForgeTasks::new(fixture.operator_pool.clone());
        tasks
            .upsert_periodic(fixture.tenant, &identity)
            .await
            .expect("periodic maintenance demand");
        let stop = CancellationToken::new();
        let scheduled =
            ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
                .expect("maintenance scheduler")
                .schedule_once(&stop)
                .await
                .expect("maintenance schedule");
        assert_eq!(scheduled.tasks_enqueued, 1, "maintenance: {scheduled:?}");
        claim_scheduled_expiry_demand_and_crash_after_prepared(&fixture, &stop).await;
        let prepared_task: uuid::Uuid = sqlx::query_scalar(
            "UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' \
             WHERE data_tenant_id=$1 AND strategy='snapshot_expiry' AND state='prepared' RETURNING task_id",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("expired Prepared maintenance claim");
        let original_evidence =
            assert_mismatched_takeover_fails_closed(&fixture, prepared_task, &stop).await;
        sqlx::query(
            "UPDATE vala.forge_tasks SET evidence=$2,claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1",
        )
        .bind(prepared_task)
        .bind(original_evidence)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("restore exact cleanup evidence");
        let takeover = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("takeover worker");
        assert!(
            takeover
                .execute_one_for_test(&stop)
                .await
                .expect("Prepared takeover")
        );
        let task_state: String =
            sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(prepared_task)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("reconciled task");
        assert_eq!(task_state, "succeeded");
        let resource = format!(
            "bifrost://{}/{}/{}",
            fixture.tenant, fixture.binding.table_ref.namespace, fixture.binding.table_ref.name
        );
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("reconciled expiry tenant connection");
        let expiry_state: (String, i64) = sqlx::query_as(
            "SELECT phase,(SELECT count(*) FROM vala.audit_outbox WHERE operation LIKE 'forge.snapshot_expire.%') \
             FROM vala.forge_operation_state WHERE resource=$1 AND family='snapshot_expire'",
        )
        .bind(&resource)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("reconciled expiry projection");
        assert_eq!(expiry_state, ("recovered".to_owned(), 2));
        assert_completed_cleanup_evidence(&fixture).await;

        assert_later_destructive_maintenance(&fixture, &resource).await;
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

    /// A superseded base cancels before rewrite IO and durably requests its successor.
    #[tokio::test]
    async fn superseded_worker_task_has_no_external_effect_and_replans() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let stop = CancellationToken::new();
        let scheduler =
            ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
                .expect("fixture scheduler");
        let planned = scheduler.schedule_once(&stop).await.expect("initial plan");
        assert_eq!(planned.tasks_enqueued, 1);

        let empty = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("empty planned table");
        fixture.append_seed_manifest(&empty, 0).await;
        let superseding_snapshot = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("superseding table")
            .metadata()
            .current_snapshot_id();
        let forge_prefix = format!("{}/data/forge/", fixture.binding.object_prefix);
        let before = fixture
            .staging
            .list_with(&forge_prefix)
            .recursive(true)
            .await
            .expect("pre-cancel Forge object list");

        assert!(
            fixture
                .worker
                .execute_one_for_test(&stop)
                .await
                .expect("cancel superseded task")
        );
        let after = fixture
            .staging
            .list_with(&forge_prefix)
            .recursive(true)
            .await
            .expect("post-cancel Forge object list");
        assert_eq!(after.len(), before.len(), "cancellation wrote no outputs");
        let current_snapshot = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("post-cancel table")
            .metadata()
            .current_snapshot_id();
        assert_eq!(current_snapshot, superseding_snapshot);
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("superseded state tenant connection");
        let state: (String, i64, i64, i64) = sqlx::query_as(
            "SELECT state,(SELECT count(*) FROM vala.file_list WHERE data_tenant_id=$1 AND compacted),(SELECT count(*) FROM vala.forge_planning_demands WHERE data_tenant_id=$1),(SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$1 AND operation='forge.task.cancelled' AND payload_summary='base_snapshot_superseded') FROM vala.forge_tasks WHERE data_tenant_id=$1 ORDER BY created_at LIMIT 1",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("superseded durable state");
        assert_eq!(state, ("cancelled".to_owned(), 0, 1, 1));

        let successor = scheduler
            .schedule_once(&stop)
            .await
            .expect("successor plan");
        assert_eq!(successor.tasks_enqueued, 1);
    }

    /// Enqueues, directly claims, and proves one closed worker payload fails terminally.
    ///
    /// # Panics
    ///
    /// Panics when admission, direct execution, or terminal-state evidence fails.
    async fn assert_worker_payload_fails_before_effect(
        fixture: &Fixture,
        tasks: &ForgeTasks,
        strategy: ForgeTaskStrategy,
        plan: ForgeTaskPlan,
        hash: u8,
    ) {
        let task_id = tasks
            .enqueue(&fixture.durable_task(strategy, plan, hash))
            .await
            .expect("validation task enqueue");
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("validation claim")
            .expect("validation task");
        assert_eq!(claim.task_id, task_id);
        fixture
            .worker
            .execute_claim(claim, &CancellationToken::new())
            .await
            .expect("closed validation terminalization");
        let state: String =
            sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("validation state");
        assert_eq!(state, "failed");
    }

    /// Direct worker validation rejects closed-strategy and malformed payloads.
    ///
    /// # Panics
    ///
    /// Panics when SQL admission, worker validation, or before-effect evidence
    /// differs from the closed T13 task contract.
    #[tokio::test]
    async fn worker_rejects_reserved_and_malformed_tasks_before_effect() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), false, 0, fixture_snapshot()).await;
        let tasks = ForgeTasks::new(fixture.operator_pool.clone());
        for (strategy, plan, hash) in [
            (
                ForgeTaskStrategy::FullIdentity,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: vec!["data/reserved.parquet".to_owned()],
                    parameters: serde_json::json!({"kind":"full_identity"}),
                },
                201,
            ),
            (
                ForgeTaskStrategy::SnapshotExpiry,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: vec!["metadata/v1.json".to_owned()],
                    parameters: serde_json::json!({"kind":"snapshot_expiry"}),
                },
                202,
            ),
            (
                ForgeTaskStrategy::StagingFold,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: Vec::new(),
                    parameters: serde_json::json!({"kind":"staging_fold"}),
                },
                203,
            ),
        ] {
            assert_worker_payload_fails_before_effect(&fixture, &tasks, strategy, plan, hash).await;
        }
        assert_eq!(fixture.reads.output_put_calls(), 0);
        let lease_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.maintenance_leases WHERE lease_key=$1")
                .bind(forge_lease_key(
                    fixture.tenant,
                    &fixture.binding.logical_namespace,
                    &fixture.binding.table_name,
                ))
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("validation lease count");
        assert_eq!(lease_count, 0);
    }

    /// An unknown raw strategy is terminally audited and releases its claim so
    /// the same worker slot can execute the next valid task.
    ///
    /// # Panics
    ///
    /// Panics when corruption setup, quarantine evidence, release state, or
    /// valid successor execution differs from the worker contract.
    #[tokio::test]
    async fn worker_quarantines_unknown_strategy_and_continues_slot() {
        let unknown_fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let tasks = ForgeTasks::new(unknown_fixture.operator_pool.clone());
        let unknown_id = tasks
            .enqueue(&unknown_fixture.durable_task(
                ForgeTaskStrategy::StagingFold,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: vec!["data/unknown.parquet".to_owned()],
                    parameters: serde_json::json!({"kind":"staging_fold"}),
                },
                204,
            ))
            .await
            .expect("unknown task seed");
        let admin = unknown_fixture
            .pg
            .superuser_pool()
            .await
            .expect("validation admin");
        sqlx::query("ALTER TABLE vala.forge_tasks DROP CONSTRAINT forge_tasks_strategy_check")
            .execute(&admin)
            .await
            .expect("drop strategy constraint for corruption proof");
        sqlx::query("UPDATE vala.forge_tasks SET strategy='unknown' WHERE task_id=$1")
            .bind(unknown_id)
            .execute(&admin)
            .await
            .expect("inject unknown strategy");
        let mut status_conn = unknown_fixture
            .pg
            .vala_postgres()
            .tenant_conn(unknown_fixture.tenant)
            .await
            .expect("unknown status tenant connection");
        assert!(
            tasks.status(&mut status_conn, 8).await.is_err(),
            "canonical status decoding must fail closed on an unknown strategy"
        );
        drop(status_conn);
        enqueue_exact_staging_task(&unknown_fixture, &unknown_fixture.binding, 205).await;
        assert!(
            unknown_fixture
                .worker
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("unknown strategy quarantine"),
            "unknown persisted strategy must be claimed and terminalized"
        );
        let quarantined: (String, bool, bool, bool, i64, i64) = sqlx::query_as(
            "SELECT state,attempt_id IS NULL,claimed_by IS NULL,claim_expires_at IS NULL,\
                    (SELECT count(*) FROM vala.audit_outbox \
                      WHERE resource='forge-task:' || $1::text \
                        AND payload_summary LIKE '%unknown%'),\
                    (SELECT count(*) FROM vala.maintenance_leases WHERE lease_key=$2) \
               FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(unknown_id)
        .bind(forge_lease_key(
            unknown_fixture.tenant,
            &unknown_fixture.binding.logical_namespace,
            &unknown_fixture.binding.table_name,
        ))
        .fetch_one(&admin)
        .await
        .expect("unknown strategy quarantine state");
        assert_eq!(quarantined, ("failed".to_owned(), true, true, true, 1, 0));
        assert_eq!(unknown_fixture.reads.output_put_calls(), 0);
        assert!(
            unknown_fixture
                .worker
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("slot continues to valid task"),
            "same slot must claim the next supported task"
        );
        let succeeded: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND state='succeeded'",
        )
        .bind(unknown_fixture.tenant.as_uuid())
        .fetch_one(&admin)
        .await
        .expect("valid successor state");
        assert_eq!(succeeded, 1);
        assert!(unknown_fixture.reads.output_put_calls() > 0);
    }

    /// A mismatched independently projected execution tenant never reaches SQL
    /// tenant binding, the table lease, catalog publication, or object output.
    ///
    /// # Panics
    ///
    /// Panics when the worker accepts a mismatched claim context or mutates any
    /// durable/external state before rejecting it.
    #[tokio::test]
    async fn worker_rejects_claim_tenant_mismatch_before_every_effect() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), false, 0, fixture_snapshot()).await;
        let tasks = ForgeTasks::new(fixture.operator_pool.clone());
        let task_id = tasks
            .enqueue(&fixture.durable_task(
                ForgeTaskStrategy::StagingFold,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: vec!["data/tenant-mismatch.parquet".to_owned()],
                    parameters: serde_json::json!({"kind":"staging_fold"}),
                },
                205,
            ))
            .await
            .expect("tenant mismatch seed");
        let mut claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("tenant mismatch claim")
            .expect("tenant mismatch task");
        claim.execution_tenant_id = DataTenantId::new_v7();
        assert!(
            fixture
                .worker
                .execute_claim(claim, &CancellationToken::new())
                .await
                .is_err()
        );
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("tenant mismatch evidence connection");
        let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("tenant mismatch audit evidence");
        let evidence: (String, i64) = sqlx::query_as(
            "SELECT state,(SELECT count(*) FROM vala.maintenance_leases) FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task_id)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("tenant mismatch evidence");
        assert_eq!(evidence, ("claimed".to_owned(), 0));
        assert_eq!(audit_count, 0);
        assert_eq!(fixture.reads.output_put_calls(), 0);
        assert_eq!(
            fixture
                .catalog
                .load_table(&fixture.binding.table_ident())
                .await
                .expect("unchanged mismatch table")
                .metadata()
                .current_snapshot_id(),
            None
        );
    }

    /// Direct claim-envelope identity failures stop before tenant SQL, lease,
    /// catalog, or object effects for every unsupported identity dimension.
    ///
    /// # Panics
    ///
    /// Panics when catalog, namespace, or table validation reaches any effect
    /// boundary or mutates the claimed task.
    #[tokio::test]
    async fn worker_rejects_invalid_claim_identity_before_every_effect() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), false, 0, fixture_snapshot()).await;
        let task_id = ForgeTasks::new(fixture.operator_pool.clone())
            .enqueue(&fixture.durable_task(
                ForgeTaskStrategy::StagingFold,
                ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs: vec!["data/invalid-identity.parquet".to_owned()],
                    parameters: serde_json::json!({"kind":"staging_fold"}),
                },
                206,
            ))
            .await
            .expect("invalid identity seed");
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("invalid identity claim")
            .expect("invalid identity task");
        let reads_before = fixture.reads.whole_reads.load(Ordering::Acquire);
        let ranges_before = fixture.reads.ranged_reads.load(Ordering::Acquire);
        let outputs_before = fixture.reads.output_put_calls();
        for invalid in [
            ForgeTaskTableIdentity {
                catalog: "other".to_owned(),
                namespace: "vala.bifrost".to_owned(),
                table: "events".to_owned(),
            },
            ForgeTaskTableIdentity {
                catalog: "wyrd-redux".to_owned(),
                namespace: "unknown".to_owned(),
                table: "events".to_owned(),
            },
            ForgeTaskTableIdentity {
                catalog: "wyrd-redux".to_owned(),
                namespace: "vala.bifrost".to_owned(),
                table: "../events".to_owned(),
            },
        ] {
            let mut invalid_claim = claim.clone();
            invalid_claim.table_ref = invalid;
            assert!(
                fixture
                    .worker
                    .execute_claim(invalid_claim, &CancellationToken::new())
                    .await
                    .is_err(),
                "invalid identity must be rejected"
            );
        }
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("invalid identity evidence connection");
        let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("invalid identity audit evidence");
        let evidence: (String, i64) = sqlx::query_as(
            "SELECT state,(SELECT count(*) FROM vala.maintenance_leases) \
               FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task_id)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("invalid identity effect evidence");
        assert_eq!(evidence, ("claimed".to_owned(), 0));
        assert_eq!(audit_count, 0);
        assert_eq!(
            fixture.reads.whole_reads.load(Ordering::Acquire),
            reads_before
        );
        assert_eq!(
            fixture.reads.ranged_reads.load(Ordering::Acquire),
            ranges_before
        );
        assert_eq!(fixture.reads.output_put_calls(), outputs_before);
        assert_eq!(
            fixture
                .catalog
                .load_table(&fixture.binding.table_ident())
                .await
                .expect("unchanged invalid-identity table")
                .metadata()
                .current_snapshot_id(),
            None
        );
    }

    /// Worker authority selected for one deterministic heartbeat-loss proof.
    #[derive(Clone, Copy)]
    enum WorkerAuthorityLoss {
        /// Expire only the durable task/large-lane claim.
        Claim,
        /// Replace only the table-scoped publication lease generation.
        TableLease,
    }

    /// Pauses one real task after output PUT and proves one authority loss
    /// cancels execution without conflating the independent surviving fence.
    ///
    /// # Panics
    ///
    /// Panics when the real worker does not observe the injected authority
    /// loss within the bounded heartbeat interval or reports the wrong class.
    async fn assert_worker_authority_loss(loss: WorkerAuthorityLoss) {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let claim = fixture.plan_and_claim().await;
        let task_id = claim.task_id;
        let claim_owner = claim.claimed_by.expect("claimed worker owner");
        let lease_key = forge_lease_key(
            fixture.tenant,
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        );
        fixture.reads.pause_after_next_output_put();
        let stop = CancellationToken::new();
        let execution = tokio::spawn({
            let worker = fixture.worker.clone();
            let stop = stop.clone();
            async move { worker.execute_claim(claim, &stop).await }
        });
        tokio::time::timeout(Duration::from_secs(30), fixture.reads.wait_for_output_put())
            .await
            .expect("worker output PUT boundary");
        let lease_before: (uuid::Uuid, i64) = sqlx::query_as(
            "SELECT owner,fencing_token FROM vala.maintenance_leases WHERE lease_key=$1",
        )
        .bind(&lease_key)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("active table lease");
        match loss {
            WorkerAuthorityLoss::Claim => {
                sqlx::query(
                    "UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 second' WHERE task_id=$1",
                )
                .bind(task_id)
                .execute(fixture.operator_pool.pool())
                .await
                .expect("expire task claim only");
            }
            WorkerAuthorityLoss::TableLease => {
                sqlx::query(
                    "UPDATE vala.maintenance_leases SET owner=$2,fencing_token=fencing_token+1 WHERE lease_key=$1",
                )
                .bind(&lease_key)
                .bind(uuid::Uuid::now_v7())
                .execute(fixture.operator_pool.pool())
                .await
                .expect("replace table lease only");
            }
        }
        tokio::time::sleep(Duration::from_millis(75)).await;
        match loss {
            WorkerAuthorityLoss::Claim => {
                let lease_after: (uuid::Uuid, i64) = sqlx::query_as(
                    "SELECT owner,fencing_token FROM vala.maintenance_leases WHERE lease_key=$1",
                )
                .bind(&lease_key)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("surviving table lease");
                assert_eq!(lease_after, lease_before, "table fence was not stolen");
            }
            WorkerAuthorityLoss::TableLease => {
                let claim_live: bool = sqlx::query_scalar(
                    "SELECT claimed_by=$2 AND claim_expires_at>statement_timestamp() FROM vala.forge_tasks WHERE task_id=$1",
                )
                .bind(task_id)
                .bind(claim_owner)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("surviving task claim");
                assert!(claim_live, "task claim remains independently live");
            }
        }
        fixture.reads.release_output_put();
        let result = tokio::time::timeout(Duration::from_secs(30), execution)
            .await
            .expect("authority-loss shutdown bound")
            .expect("authority-loss worker join");
        match loss {
            WorkerAuthorityLoss::Claim => {
                assert!(matches!(result, Err(ForgeError::Sql(_))), "{result:?}");
            }
            WorkerAuthorityLoss::TableLease => {
                assert!(
                    matches!(result, Err(ForgeError::FenceLost { .. })),
                    "{result:?}"
                );
            }
        }
    }

    /// Claim heartbeat loss cancels a paused real worker while its table fence survives.
    #[tokio::test]
    async fn worker_claim_loss_cancels_independently() {
        assert_worker_authority_loss(WorkerAuthorityLoss::Claim).await;
    }

    /// Table-lease loss cancels a paused real worker while its claim survives.
    #[tokio::test]
    async fn worker_table_lease_loss_cancels_independently() {
        assert_worker_authority_loss(WorkerAuthorityLoss::TableLease).await;
    }

    /// A live table lease excludes the direct worker path before rewrite output.
    ///
    /// # Panics
    ///
    /// Panics when a second worker bypasses the table-scoped publication fence.
    #[tokio::test]
    async fn worker_same_table_lease_excludes_publication() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let claim = fixture.plan_and_claim().await;
        let competing = ForgeLease::acquire(
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
        .expect("competing lease query")
        .expect("competing lease");
        let result = fixture
            .worker
            .execute_claim(claim, &CancellationToken::new())
            .await;
        assert!(matches!(result, Err(ForgeError::FenceLost { .. })));
        assert_eq!(fixture.reads.output_put_calls(), 0);
        assert!(
            competing
                .release(&fixture.operator_pool)
                .await
                .expect("competing lease release")
        );
    }

    /// Resource refusal returns the durable claim to retryable before rewrite IO.
    ///
    /// # Panics
    ///
    /// Panics when the production worker performs object output, retains its
    /// claim, or projects anything other than typed capacity pressure.
    #[tokio::test]
    async fn forge_waits_without_io_when_resources_do_not_fit() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let resources = fixture.forge.resources_for_test();
        let plan = fixture.roles.plan();
        let blocker_envelope = vala_bifrost_redux::forge::ForgeEnvelopeSizer::size(
            1,
            1,
            1,
            vala_bifrost_redux::forge::ForgeCapacity {
                max_files: 1,
                max_bytes: u64::MAX,
                max_parallelism: 1,
                max_memory_bytes: u64::try_from(plan.elastic_memory_bytes)
                    .expect("blocker memory capacity"),
                max_spill_bytes: plan.scratch_limit_bytes,
                max_large_task_bytes: u64::MAX,
            },
        )
        .expect("blocker envelope");
        let blocker = resources
            .try_acquire_rewrite(vala_bifrost_redux::resources::ForgeRewriteRequest {
                envelope: blocker_envelope,
                memory_bytes: plan.elastic_memory_bytes,
                scratch_bytes: plan.scratch_limit_bytes,
                reader_permits: u16::try_from(plan.effective_cpu).unwrap_or(u16::MAX),
            })
            .expect("test owner occupies all Forge resources");
        let claim = fixture.plan_and_claim().await;
        let task_id = claim.task_id;
        let outputs_before = fixture.reads.output_put_calls();
        let result = fixture
            .worker
            .execute_and_settle_claim_for_test(claim, &CancellationToken::new())
            .await;
        assert!(
            matches!(result, Err(ForgeError::Capacity { .. })),
            "{result:?}"
        );
        assert_eq!(fixture.reads.output_put_calls(), outputs_before);
        let state: String =
            sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("capacity-refused task state");
        assert_eq!(state, "retryable");
        drop(blocker);
        let snapshot = resources.snapshot().expect("released resource snapshot");
        assert_eq!(snapshot.elastic_memory_used_bytes, 0);
        assert_eq!(snapshot.scratch_used_bytes, 0);
    }

    /// Execution-envelope exhaustion terminalizes immediately without retry.
    #[tokio::test]
    async fn execution_envelope_exhaustion_terminalizes_without_retry() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let stop = CancellationToken::new();
        let planned = ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
            .expect("fixture scheduler")
            .schedule_once(&stop)
            .await
            .expect("planning pass");
        assert_eq!(planned.tasks_enqueued, 1);
        let decoded = 16_i64 * 1024 * 1024;
        let merge = 1024_i64 * 1024;
        let updated = sqlx::query(
            "UPDATE vala.forge_tasks SET decoded_batch_bytes=$2,decoded_input_bytes=$2,estimated_parallelism=1,sort_merge_reservation_bytes=$3,sort_working_bytes=2*$2+$3,sort_spill_bytes=1,estimated_memory_bytes=3*$2+$3+encoder_buffer_bytes+upload_chunk_bytes+footer_encoded_bytes,estimated_spill_bytes=1+output_scratch_bytes WHERE data_tenant_id=$1 AND state='ready'",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(decoded)
        .bind(merge)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("constrain persisted production sort envelope")
        .rows_affected();
        assert_eq!(updated, 1);
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("constrained claim query")
            .expect("constrained claim");
        let task_id = claim.task_id;
        let result = fixture
            .worker
            .execute_and_settle_claim_for_test(claim, &stop)
            .await;
        assert!(
            matches!(result, Err(ForgeError::ExecutionEnvelopeExceeded { .. })),
            "production SortExec must surface typed envelope exhaustion: {result:?}"
        );

        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("execution refusal evidence tenant connection");
        let settled: (String, i32, Option<String>, i64) = sqlx::query_as(
            "SELECT state,attempt_count,failure_class,(SELECT count(*) FROM vala.audit_outbox WHERE resource='forge-task:' || $1::text AND operation='forge.task.failed') FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("settled execution refusal");
        assert_eq!(
            settled,
            (
                "failed".to_owned(),
                1,
                Some("capacity_refused".to_owned()),
                1
            )
        );
        assert_eq!(fixture.reads.output_put_calls(), 0);
        assert!(
            !fixture
                .worker
                .execute_one_for_test(&stop)
                .await
                .expect("terminal capacity refusal cannot be reclaimed")
        );
        let released = fixture
            .forge
            .resources_for_test()
            .snapshot()
            .expect("released resource snapshot");
        assert_eq!(released.elastic_memory_used_bytes, 0);
        assert_eq!(released.scratch_used_bytes, 0);
        assert_eq!(released.forge_reader_permits_used, 0);
    }

    /// Unmarked sources refuse after footer reads and before the first data page.
    #[tokio::test]
    async fn unmarked_source_refuses_before_first_data_page_range() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), false, 0, fixture_snapshot()).await;
        fixture.seed_unmarked_files(2, true).await;
        vala_bifrost_redux::forge::reset_data_page_reads_for_test();
        let stop = CancellationToken::new();
        let outcome = ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
            .expect("fixture scheduler")
            .schedule_once(&stop)
            .await
            .expect("unmarked planning pass");
        assert_eq!(outcome.tasks_enqueued, 1);
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("unmarked claim query")
            .expect("unmarked claim");
        let error = fixture
            .worker
            .execute_claim(claim, &stop)
            .await
            .expect_err("unmarked source must refuse");
        assert!(matches!(error, ForgeError::DataRefusal { .. }));
        assert!(fixture.reads.ranged_reads.load(Ordering::Acquire) > 0);
        assert_eq!(
            vala_bifrost_redux::forge::data_page_reads_for_test(),
            0,
            "footer identity refusal must precede every data-page request"
        );
        assert_eq!(fixture.reads.output_put_calls(), 0);
    }

    /// A claimed legacy task is auditedly superseded and replanned before input IO.
    #[tokio::test]
    async fn legacy_task_is_auditedly_superseded_and_replanned_before_io() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let stop = CancellationToken::new();
        let planned = ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
            .expect("fixture scheduler")
            .schedule_once(&stop)
            .await
            .expect("planning pass");
        assert_eq!(planned.tasks_enqueued, 1);
        sqlx::query(
            "UPDATE vala.forge_tasks SET envelope_version=0, decoded_batch_bytes=NULL, decoded_input_bytes=NULL, sort_working_bytes=NULL, sort_merge_reservation_bytes=NULL, encoder_buffer_bytes=NULL, upload_chunk_bytes=NULL, footer_encoded_bytes=NULL, footer_decode_workspace_bytes=NULL, sort_spill_bytes=NULL, output_scratch_bytes=NULL, estimated_files=1, estimated_bytes=9223372036854775807, estimated_parallelism=1, estimated_memory_bytes=9223372036854775807, estimated_spill_bytes=9223372036854775807, large_task_ceiling_bytes=9223372036854775807 WHERE data_tenant_id=$1 AND state='ready'",
        )
        .bind(fixture.tenant.as_uuid())
        .execute(fixture.operator_pool.pool())
        .await
        .expect("convert planned task to legacy");
        let claim = fixture
            .worker
            .claim_for_test()
            .await
            .expect("claim query")
            .expect("legacy claim");
        let task_id = claim.task_id;
        let reads_before = fixture.reads.ranged_reads.load(Ordering::Relaxed);
        fixture
            .worker
            .execute_claim(claim, &stop)
            .await
            .expect("legacy supersession");

        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("legacy evidence tenant connection");
        let evidence: (String, i32, i64, i64) = sqlx::query_as(
            "SELECT state,attempt_count,(SELECT count(*) FROM vala.forge_planning_demands WHERE data_tenant_id=$1),(SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$1 AND operation='forge.task.cancelled') FROM vala.forge_tasks WHERE task_id=$2",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(task_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("legacy settlement evidence");
        assert_eq!(evidence, ("cancelled".to_owned(), 0, 1, 1));
        assert_eq!(
            fixture.reads.ranged_reads.load(Ordering::Relaxed),
            reads_before
        );
        assert_eq!(fixture.reads.output_put_calls(), 0);
    }

    /// Enqueues one exact ordinary staging task for a registered fixture table.
    ///
    /// # Panics
    ///
    /// Panics when exact inputs, identity construction, or enqueueing fails.
    async fn enqueue_exact_staging_task(fixture: &Fixture, binding: &TenantTableBinding, hash: u8) {
        let inputs: Vec<String> = sqlx::query_scalar(
            "SELECT file_path FROM vala.file_list WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 ORDER BY file_path",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&binding.logical_namespace)
        .bind(&binding.table_name)
        .fetch_all(fixture.operator_pool.pool())
        .await
        .expect("concurrent exact inputs");
        ForgeTasks::new(fixture.operator_pool.clone())
            .enqueue(&NewForgeTask {
                data_tenant_id: fixture.tenant,
                table_ref: ForgeTaskTableIdentity::new(
                    "wyrd-redux",
                    &binding.logical_namespace,
                    &binding.table_name,
                )
                .expect("concurrent task identity"),
                strategy: ForgeTaskStrategy::StagingFold,
                lane: ForgeTaskLane::Ordinary,
                base_snapshot_id: 0,
                plan: ForgeTaskPlan {
                    version: FORGE_TASK_PAYLOAD_VERSION,
                    inputs,
                    parameters: serde_json::json!({"kind":"staging_fold"}),
                },
                plan_hash: [hash; 32],
                estimates: ForgeTaskEstimates {
                    envelope: Some(vala_sql::row_types::forge_tasks::ForgeTaskEnvelope {
                        version: vala_sql::row_types::forge_tasks::FORGE_ENVELOPE_VERSION,
                        reader_permits: 1,
                        decoded_batch_bytes: 16 * 1024 * 1024,
                        decoded_input_bytes: 16 * 1024 * 1024,
                        sort_working_bytes: 42 * 1024 * 1024,
                        sort_merge_reservation_bytes: 10 * 1024 * 1024,
                        encoder_buffer_bytes: 32 * 1024 * 1024,
                        upload_chunk_bytes: 8 * 1024 * 1024,
                        footer_encoded_bytes: 8 * 1024 * 1024,
                        footer_decode_workspace_bytes: 32 * 1024 * 1024,
                        sort_spill_bytes: 512 * 1024 * 1024,
                        output_scratch_bytes: 512 * 1024 * 1024,
                    }),
                    files: 2,
                    bytes: 200,
                    parallelism: 1,
                    memory_bytes: 106 * 1024 * 1024,
                    spill_bytes: 512 * 1024 * 1024,
                    large_ceiling_bytes: 2 * 1024 * 1024 * 1024,
                },
                ready_at: chrono::Utc::now(),
            })
            .await
            .expect("concurrent exact task");
    }

    /// Two fixed worker slots execute independent table rewrites concurrently.
    ///
    /// Both real output PUTs must reach the shared pause before either is
    /// released, which proves progress is not serialized by a process-global
    /// task or publication lock.
    ///
    /// # Panics
    ///
    /// Panics when planning does not enqueue both tables, admission serializes
    /// the claims, or either direct worker execution fails.
    #[tokio::test]
    async fn worker_independent_tables_execute_concurrently() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let second_binding = fixture.register_seeded_table().await;
        for (binding, hash) in [(&fixture.binding, 206_u8), (&second_binding, 207_u8)] {
            enqueue_exact_staging_task(&fixture, binding, hash).await;
        }
        let stop = CancellationToken::new();
        let worker = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig {
                worker_concurrency: 2,
                per_tenant_active_cap: 2,
            },
            uuid::Uuid::now_v7(),
        )
        .expect("two-slot worker");
        let first = worker
            .claim_for_test()
            .await
            .expect("first concurrent claim")
            .expect("first concurrent task");
        let Some(second) = worker
            .claim_for_test()
            .await
            .expect("second concurrent claim")
        else {
            panic!("second concurrent task missing");
        };
        assert_ne!(first.table_ref, second.table_ref);
        fixture.reads.pause_after_output_puts(2);
        let first_task = tokio::spawn({
            let worker = worker.clone();
            let stop = stop.clone();
            async move { worker.execute_claim(first, &stop).await }
        });
        let second_task = tokio::spawn({
            let worker = worker.clone();
            let stop = stop.clone();
            async move { worker.execute_claim(second, &stop).await }
        });
        tokio::time::timeout(
            Duration::from_secs(30),
            fixture.reads.wait_for_output_puts(2),
        )
        .await
        .expect("both independent outputs reach PUT concurrently");
        fixture.reads.release_output_put();
        let (first_result, second_result) = tokio::join!(first_task, second_task);
        first_result
            .expect("first concurrent worker join")
            .expect("first concurrent execution");
        second_result
            .expect("second concurrent worker join")
            .expect("second concurrent execution");
        let succeeded: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND state='succeeded'",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("concurrent terminal states");
        assert_eq!(succeeded, 2);
    }

    /// Cancellation observed mid-rewrite, before the catalog commit, releases
    /// the in-flight claim to `retryable` and drains the active-claim count, so
    /// a successor reclaims it immediately without waiting for lease expiry and
    /// completes it exactly once.
    ///
    /// This is the in-flight (`execute_fenced`) pre-effect drain, re-proven
    /// through the migrated single release seam: the rewrite has written a real
    /// output PUT but not committed to the catalog, so the supervised slot's
    /// [`ForgeWorker::execute_and_settle_claim_for_test`] settlement — the same
    /// `run_slot` release decision — drains the claim losslessly.
    ///
    /// # Panics
    ///
    /// Panics when shutdown exceeds its bound, the cancelled attempt does not
    /// drain to `retryable`, the active-claim count does not fall to zero, or
    /// the successor cannot reclaim and finish the task exactly once.
    #[tokio::test]
    async fn worker_active_shutdown_releases_retryable_then_successor_reclaims() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let claim = fixture.plan_and_claim().await;
        let task_id = claim.task_id;
        fixture.reads.pause_after_next_output_put();
        let stop = CancellationToken::new();
        let execution = tokio::spawn({
            let worker = fixture.worker.clone();
            let stop = stop.clone();
            async move { worker.execute_and_settle_claim_for_test(claim, &stop).await }
        });
        tokio::time::timeout(Duration::from_secs(30), fixture.reads.wait_for_output_put())
            .await
            .expect("shutdown output boundary");
        stop.cancel();
        fixture.reads.release_output_put();
        let result = tokio::time::timeout(Duration::from_secs(30), execution)
            .await
            .expect("active shutdown bound")
            .expect("active worker join");
        assert!(matches!(result, Err(ForgeError::Shutdown)), "{result:?}");
        let (state, active) = task_state_and_active_claims(&fixture, task_id).await;
        assert_eq!(
            state, "retryable",
            "a pre-effect shutdown must release the in-flight claim to retryable"
        );
        assert_eq!(
            active, 0,
            "releasing the cancelled claim must drain the active-claim count"
        );
        let successor = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig {
                worker_concurrency: 1,
                per_tenant_active_cap: 1,
            },
            uuid::Uuid::now_v7(),
        )
        .expect("successor worker");
        assert!(
            successor
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("successor reclaim execution")
        );
        let (state, _) = task_state_and_active_claims(&fixture, task_id).await;
        assert_eq!(state, "succeeded");
        let succeeded: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND state='succeeded'",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("terminal state count");
        assert_eq!(
            succeeded, 1,
            "the pre-effect attempt must not commit a duplicate terminal"
        );
    }

    /// A cooperative shutdown observed after a maintenance operation's durable
    /// effect retains the claim through the migrated single release seam instead
    /// of releasing it.
    ///
    /// The maintenance dispatch observes the authority token, not the
    /// shutdown-sensitive `operation_stop`, so a graceful shutdown lets the
    /// operation commit its durable effect and reach `prepared`; the post-effect
    /// checkpoint then surfaces [`ForgeError::ShutdownRetained`]. The supervised
    /// slot's settlement must NOT match the pre-effect release guard: the claim
    /// stays `prepared` and counted as active for evidence-based and
    /// lease-expiry recovery. This is the post-effect complement to
    /// [`worker_active_shutdown_releases_retryable_then_successor_reclaims`],
    /// driven through the same
    /// [`ForgeWorker::execute_and_settle_claim_for_test`] seam.
    ///
    /// # Panics
    ///
    /// Panics when the maintenance boundary is not reached, the post-effect
    /// result is not [`ForgeError::ShutdownRetained`], the durable state is not
    /// the retained `prepared`, or the active-claim count is not the retained
    /// single claim.
    #[tokio::test]
    async fn worker_post_effect_shutdown_retains_claim_via_settlement() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                maintenance_trigger_interval: Duration::from_nanos(1),
                snapshot_retention: Duration::from_nanos(1),
                orphan_gc_ttl: Duration::from_nanos(1),
                manifest_rewrite_enabled: true,
                manifest_rewrite_min_count: 2,
                ..ForgeConfig::default()
            },
            true,
            4,
            fixture_snapshot(),
        )
        .await;
        let claim = prepare_maintenance_claim(&fixture).await;
        let task_id = claim.task_id;
        let controls = fixture.forge.maintenance_controls_for_test();
        controls.arm_manifest_submission();
        let stop = CancellationToken::new();
        let execution = tokio::spawn({
            let worker = fixture.worker.clone();
            let stop = stop.clone();
            async move { worker.execute_and_settle_claim_for_test(claim, &stop).await }
        });
        tokio::pin!(execution);
        let arrival = async { controls.wait_manifest_submission().await };
        tokio::pin!(arrival);
        tokio::select! {
            () = &mut arrival => {}
            result = &mut execution => {
                panic!("maintenance returned before catalog boundary: {result:?}")
            }
        }
        stop.cancel();
        controls.release_manifest_submission();
        let result = (&mut execution).await.expect("post-effect worker join");
        assert!(
            matches!(result, Err(ForgeError::ShutdownRetained)),
            "a post-effect shutdown must surface ShutdownRetained: {result:?}"
        );
        let (state, active) = task_state_and_active_claims(&fixture, task_id).await;
        assert_eq!(
            state, "prepared",
            "a post-effect shutdown must retain the durable prepared state, never release it"
        );
        assert_eq!(
            active, 1,
            "retaining the post-effect claim must keep it counted as an active claim"
        );
    }

    /// A worker that crashes mid-execution without observing cancellation leaves
    /// a durable `running` claim that a successor reclaims only after the lease
    /// expires, then completes exactly once.
    ///
    /// This preserves crash-path recovery for the `running` state that the
    /// cooperative drain deliberately does not cover: cancellation is never
    /// observed, so the claim is retained and recovered through
    /// `reclaim_expired` after its lease TTL rather than released to
    /// `retryable`.
    ///
    /// # Panics
    ///
    /// Panics when the crashed `running` claim is not reclaimed after expiry or
    /// the successor does not complete it exactly once.
    #[tokio::test]
    async fn worker_running_crash_without_cancel_is_reclaimed_after_expiry() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let claim = fixture.plan_and_claim().await;
        let task_id = claim.task_id;
        sqlx::query(
            "UPDATE vala.forge_tasks SET state='running', watermark_snapshot_id=0, watermark_timestamp_ms=0, claim_expires_at=statement_timestamp()-interval '1 second',next_eligible_at=statement_timestamp()-interval '1 second' WHERE task_id=$1",
        )
        .bind(task_id)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("simulate crashed running attempt with an expired lease");
        let successor = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig {
                worker_concurrency: 1,
                per_tenant_active_cap: 1,
            },
            uuid::Uuid::now_v7(),
        )
        .expect("successor worker");
        assert!(
            !successor
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("successor bounded reclaim"),
            "lease reclaim persists backoff before another attempt"
        );
        sqlx::query(
            "UPDATE vala.forge_tasks SET next_eligible_at=statement_timestamp()-interval '1 second',ready_at=statement_timestamp()-interval '1 second' WHERE task_id=$1",
        )
        .bind(task_id)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("advance reclaimed task eligibility");
        assert!(
            successor
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("successor execution after backoff")
        );
        let (state, _) = task_state_and_active_claims(&fixture, task_id).await;
        assert_eq!(state, "succeeded");
        let succeeded: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND state='succeeded'",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("terminal state count");
        assert_eq!(
            succeeded, 1,
            "the crashed attempt committed nothing, so recovery completes exactly once"
        );
    }

    /// A cooperative shutdown observed after a supervised slot durably claims
    /// work, but before execution begins, releases the claim to `retryable` and
    /// drains the active-claim count so a successor reclaims it losslessly.
    ///
    /// This is the `run_slot` post-claim, pre-execute drain path: no pre-effect
    /// durable work was performed when the slot observes shutdown.
    ///
    /// # Panics
    ///
    /// Panics when planning does not enqueue exactly one task, the supervised
    /// slot does not reach its claim gate, the released task is not `retryable`,
    /// or the active-claim count does not drain to zero.
    #[tokio::test]
    async fn worker_shutdown_before_execute_releases_claim_to_retryable() {
        let fixture = Fixture::new_with_observer(ForgeWorkerCompletionObserver::new()).await;
        let completion = fixture
            .completion
            .clone()
            .expect("observer fixture exposes its completion observer");
        let stop = CancellationToken::new();
        let planned = ForgeScheduler::with_owner_for_test(&fixture.forge, fixture.scheduler_owner)
            .expect("fixture scheduler")
            .schedule_once(&stop)
            .await
            .expect("fixture planning pass");
        assert_eq!(planned.tasks_enqueued, 1, "planned outcome: {planned:?}");
        completion.hold_after_claims_for_test(1);
        let run = tokio::spawn({
            let worker = fixture.worker.clone();
            let stop = stop.clone();
            async move { worker.run(stop).await }
        });
        tokio::time::timeout(
            Duration::from_secs(30),
            completion.wait_for_claims_for_test(),
        )
        .await
        .expect("supervised slot reaches its claim gate");
        stop.cancel();
        completion.release_claims_for_test();
        tokio::time::timeout(Duration::from_secs(30), run)
            .await
            .expect("supervised shutdown bound")
            .expect("supervised run join")
            .expect("supervised run drains cleanly");
        let task_id: uuid::Uuid =
            sqlx::query_scalar("SELECT task_id FROM vala.forge_tasks WHERE data_tenant_id=$1")
                .bind(fixture.tenant.as_uuid())
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("single planned task");
        let (state, active) = task_state_and_active_claims(&fixture, task_id).await;
        assert_eq!(
            state, "retryable",
            "a shutdown observed before execution must release the claim to retryable"
        );
        assert_eq!(
            active, 0,
            "releasing the claim before execution must drain the active-claim count"
        );
    }

    /// A shutdown release against a claim that already advanced to `prepared`
    /// is benign: it retains the durable `prepared` state instead of erroring.
    ///
    /// The post-effect (`prepared`) row is past the pre-effect release guard, so
    /// [`ForgeWorker::release_cancelled_claim_for_test`] matches no row, returns
    /// `Ok(())`, and leaves the claim retained for reconciliation — proving a
    /// `prepared` claim under cancellation is observed as clean retention, never
    /// a release error, and stays counted as an active claim.
    ///
    /// # Panics
    ///
    /// Panics when the benign release errors, the `prepared` state is not
    /// retained, or the active-claim count changes.
    #[tokio::test]
    async fn prepared_claim_shutdown_release_is_benign_and_retained() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let claim = fixture.plan_and_claim().await;
        let task_id = claim.task_id;
        let attempt = claim.attempt_id.expect("claimed task carries an attempt");
        sqlx::query(
            "UPDATE vala.forge_tasks SET state='prepared', watermark_snapshot_id=0, watermark_timestamp_ms=0, evidence='{}'::jsonb WHERE task_id=$1 AND attempt_id=$2",
        )
        .bind(task_id)
        .bind(attempt)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("advance this worker's claim past the pre-effect guard");
        fixture
            .worker
            .release_cancelled_claim_for_test(task_id, attempt)
            .await
            .expect("releasing a prepared claim must be benign");
        let (state, active) = task_state_and_active_claims(&fixture, task_id).await;
        assert_eq!(
            state, "prepared",
            "a prepared claim under cancellation must be retained, not released"
        );
        assert_eq!(
            active, 1,
            "the retained prepared claim must remain an active claim"
        );
    }

    /// Reads the exact current metadata bytes and returns snapshot, location,
    /// and digest evidence for later recovery comparison.
    ///
    /// # Panics
    ///
    /// Panics when the committed fixture table lacks a snapshot or metadata
    /// location, escapes its table root, or its raw object cannot be read.
    async fn exact_current_metadata_evidence(
        fixture: &Fixture,
        table: &iceberg::table::Table,
    ) -> (i64, String, String) {
        let snapshot = table
            .metadata()
            .current_snapshot_id()
            .expect("task committed snapshot");
        let location = table
            .metadata_location_result()
            .expect("task committed metadata location")
            .to_owned();
        let relative = location
            .strip_prefix(&format!(
                "{}/",
                table.metadata().location().trim_end_matches('/')
            ))
            .expect("metadata location below fixture table");
        let key = format!(
            "{}/{relative}",
            fixture.binding.object_prefix.trim_end_matches('/')
        );
        let digest = format!(
            "sha256:{}",
            hex::encode(Sha256::digest(
                fixture
                    .staging
                    .read(&key)
                    .await
                    .expect("original metadata bytes")
                    .to_bytes()
            ))
        );
        (snapshot, location, digest)
    }

    /// Metadata evidence read failure leaves Running recovery state; after a
    /// later catalog snapshot, takeover finds the original exact task commit.
    ///
    /// # Panics
    ///
    /// Panics when missing evidence is fabricated, later catalog progress is
    /// mistaken for task evidence, recovery rewrites output, or the original
    /// committed bytes cannot terminalize and audit the task.
    #[tokio::test]
    async fn worker_recovers_exact_evidence_read_without_rewrite() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 2, fixture_snapshot()).await;
        let claim = fixture.plan_and_claim().await;
        let task_id = claim.task_id;
        fixture.reads.fail_next_metadata_read();
        let result = fixture
            .worker
            .execute_claim(claim, &CancellationToken::new())
            .await;
        assert!(
            matches!(result, Err(ForgeError::ObjectStore(_))),
            "{result:?}"
        );
        let before_recovery = fixture.reads.output_put_calls();
        let retained: (String, bool) =
            sqlx::query_as("SELECT state,evidence IS NULL FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("retained evidence failure");
        assert_eq!(retained, ("retryable".to_owned(), true));
        let committed = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("committed table after evidence failure");
        let (original_snapshot, original_location, original_digest) =
            exact_current_metadata_evidence(&fixture, &committed).await;
        fixture.seed_files_at(99, 1, false).await;
        fixture.append_seed_manifest(&committed, 99).await;
        let advanced = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("intervening catalog table");
        assert_ne!(
            advanced.metadata().current_snapshot_id(),
            Some(original_snapshot),
            "intervening append must advance beyond the task commit"
        );
        assert_ne!(
            advanced
                .metadata_location_result()
                .expect("intervening metadata location"),
            original_location,
            "recovery must search retained metadata rather than current bytes"
        );
        sqlx::query(
            "UPDATE vala.forge_tasks SET next_eligible_at=statement_timestamp()-interval '1 second',ready_at=statement_timestamp()-interval '1 second' WHERE task_id=$1",
        )
        .bind(task_id)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("expire evidence-read attempt");
        let successor = ForgeWorker::new(
            Arc::clone(&fixture.forge),
            ForgeWorkerConfig {
                worker_concurrency: 1,
                per_tenant_active_cap: 1,
            },
            uuid::Uuid::now_v7(),
        )
        .expect("evidence successor");
        assert!(
            successor
                .execute_one_for_test(&CancellationToken::new())
                .await
                .expect("evidence recovery")
        );
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("recovered evidence tenant connection");
        let recovered: (String, i64, String, String, i64) = sqlx::query_as(
            "SELECT state,(evidence->>'committed_snapshot_id')::bigint,\
                    evidence->>'committed_metadata_location',\
                    evidence->>'committed_metadata_digest',\
                    (SELECT count(*) FROM vala.audit_outbox \
                      WHERE resource='forge-task:' || $1::text \
                        AND operation IN ('forge.task.prepared','forge.task.succeeded')) \
               FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("recovered evidence state");
        assert_eq!(
            recovered,
            (
                "succeeded".to_owned(),
                original_snapshot,
                original_location,
                original_digest,
                2,
            )
        );
        assert_eq!(fixture.reads.output_put_calls(), before_recovery);
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
            time_partition: fixture_seed_partition(&fixture).await.to_wire(),
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
