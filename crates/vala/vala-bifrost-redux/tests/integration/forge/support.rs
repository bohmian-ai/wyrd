//! Shared fixtures for the forge modules.
//!
//! Every item is used by more than one sibling module; a helper with a
//! single consumer lives in that module instead. Contains no tests.

use arrow::array::{
    FixedSizeBinaryBuilder, Int32Array, Int64Array, RecordBatch, StringArray,
    TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field};
use iceberg::spec::{
    DataContentType, DataFileBuilder, DataFileFormat, Datum, Literal, NullOrder, PrimitiveType,
    SortDirection, Struct, TableMetadata, Transform,
};
use iceberg::table::Table;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use iceberg::{
    Catalog, MetadataLocation, Namespace, NamespaceIdent, TableCommit, TableCreation, TableIdent,
};
use opendal::services::Fs;
use opendal::{Buffer, Operator};
use parquet::arrow::ArrowWriter;
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::{
    BifrostCatalog, CreateTableRequest, TableRef, TenantTableBinding,
};
use vala_bifrost_redux::forge::ForgeObjectStore;
use vala_bifrost_redux::forge::{
    Forge, ForgeBuildConfig, ForgeClock, ForgeConfig, ForgeLease, ForgeObjectPages,
    ForgeScheduleOutcome, ForgeScheduler, ForgeWorker, ForgeWorkerCompletionObserver,
    ForgeWorkerConfig, forge_lease_key,
};
use vala_bifrost_redux::maintenance::staging_file_channel;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::resources::{
    BifrostResourcePolicy, BifrostRole, ResourceSource, SystemResourceSnapshot,
};
use vala_sql::OperatorPool;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::queries::forge_tasks::ForgeTasks;
use vala_sql::row_types::forge_operations::{ForgeOperationFamily, ForgeOperationTransition};
use vala_sql::row_types::forge_tasks::{
    ForgeClaimStrategy, ForgeTaskClaim, ForgeTaskEstimates, ForgeTaskLane, ForgeTaskPlan,
    ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask,
};
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::WYRD_EVENT_TIME;
use wyrd_spec::vala::api::{
    AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeCompactionPhase,
    StoragePath,
};
use wyrd_storage::BackendConfig;

/// Counts ranged source reads while delegating bytes to a local operator.
#[derive(Debug)]
pub(crate) struct InstrumentedStore {
    /// Fixture-backed object operator.
    pub(crate) operator: Arc<Operator>,
    /// Number of whole-object reads.
    pub(crate) whole_reads: Arc<AtomicUsize>,
    /// Number of ranged reads.
    pub(crate) ranged_reads: Arc<AtomicUsize>,
    /// Peak concurrent ranged reads.
    pub(crate) active_reads: Arc<AtomicUsize>,
    /// Peak observed concurrency.
    pub(crate) peak_reads: Arc<AtomicUsize>,
    /// One-shot metadata-read failure used after a successful commit.
    pub(crate) fail_next_metadata_read: AtomicBool,
    /// One-shot pause consumed by the next successful rewrite output.
    pub(crate) pause_output_puts: AtomicUsize,
    /// Counts armed outputs that crossed the real PUT boundary.
    pub(crate) paused_output_puts: AtomicUsize,
    /// Wakes a test waiting for the armed real output.
    pub(crate) output_put_ready: tokio::sync::Notify,
    /// Releases the armed output notification.
    pub(crate) output_put_release: tokio::sync::Notify,
    /// Durable release state preventing a lost post-PUT wake-up.
    pub(crate) output_put_released: AtomicBool,
    /// Counts successful rewrite output notifications.
    pub(crate) output_put_calls: AtomicUsize,
    /// Counts bounded chunks accepted by output writers.
    pub(crate) output_chunks: AtomicUsize,
    /// Counts output writer openings across successful and aborted uploads.
    pub(crate) output_writer_opens: AtomicUsize,
    /// Counts multipart aborts after injected chunk failures.
    pub(crate) output_writer_aborts: AtomicUsize,
    /// Largest chunk supplied to an output writer.
    pub(crate) largest_output_chunk: AtomicUsize,
    /// One-shot chunk failure used to prove abort and same-attempt retry.
    pub(crate) fail_next_output_chunk: AtomicBool,
    /// Per-path cleanup delete attempts observed through the production store seam.
    pub(crate) delete_attempts: Mutex<HashMap<String, usize>>,
    /// Entries per orphan-listing page, or `0` to keep the single-page default.
    ///
    /// A positive value makes [`ForgeObjectStore::list_pages`] paginate the
    /// sorted listing into fixed-size pages, modeling a lexicographically
    /// ordered object backend so bounded-scan tests can exercise the page cap
    /// and cross-run drainage deterministically.
    pub(crate) list_page_entries: AtomicUsize,
}

impl InstrumentedStore {
    /// Arms a one-shot pause after the next successful rewrite output PUT.
    pub(crate) fn pause_after_next_output_put(&self) {
        self.pause_after_output_puts(1);
    }

    /// Waits until the armed output has crossed the real PUT boundary.
    pub(crate) async fn wait_for_output_put(&self) {
        self.wait_for_output_puts(1).await;
    }

    /// Arms pauses after an exact positive number of successful output PUTs.
    pub(crate) fn pause_after_output_puts(&self, count: usize) {
        assert!(count > 0, "paused output count must be positive");
        self.paused_output_puts.store(0, Ordering::Release);
        self.output_put_released.store(false, Ordering::Release);
        self.pause_output_puts.store(count, Ordering::Release);
    }

    /// Waits until every armed output has crossed its real PUT boundary.
    pub(crate) async fn wait_for_output_puts(&self, count: usize) {
        while self.paused_output_puts.load(Ordering::Acquire) < count {
            self.output_put_ready.notified().await;
        }
    }

    /// Releases one paused post-PUT notification.
    pub(crate) fn release_output_put(&self) {
        self.output_put_released.store(true, Ordering::Release);
        self.output_put_release.notify_waiters();
    }

    /// Arms one failure for the next exact Iceberg metadata read.
    pub(crate) fn fail_next_metadata_read(&self) {
        self.fail_next_metadata_read.store(true, Ordering::Release);
    }

    /// Returns the number of successful rewrite output boundaries observed.
    pub(crate) fn output_put_calls(&self) -> usize {
        self.output_put_calls.load(Ordering::Acquire)
    }

    /// Arms one failure on the next output chunk.
    pub(crate) fn fail_next_output_chunk(&self) {
        self.fail_next_output_chunk.store(true, Ordering::Release);
    }

    /// Returns the exact number of delete attempts observed for one object path.
    pub(crate) fn delete_attempts_for(&self, path: &str) -> usize {
        self.delete_attempts
            .lock()
            .expect("delete-attempt ledger lock")
            .get(path)
            .copied()
            .unwrap_or_default()
    }

    /// Returns the total cleanup delete attempts observed across all paths.
    pub(crate) fn total_delete_attempts(&self) -> usize {
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
    pub(crate) fn set_list_page_entries(&self, entries: usize) {
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
    async fn read_range(&self, path: &str, range: std::ops::Range<u64>) -> opendal::Result<Buffer> {
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
pub(crate) fn fixture_snapshot() -> SystemResourceSnapshot {
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
pub(crate) fn forge_policy(scratch_limit_bytes: u64) -> BifrostResourcePolicy {
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

pub(crate) struct Fixture {
    /// Embedded Postgres fixture.
    pub(crate) pg: PgFixture,
    /// Tenant and physical table identity.
    pub(crate) tenant: DataTenantId,
    /// Registered table binding.
    pub(crate) binding: TenantTableBinding,
    /// SQL operator pool.
    pub(crate) operator_pool: OperatorPool,
    /// Iceberg catalog used for output manifest assertions.
    pub(crate) catalog: Arc<dyn Catalog>,
    /// Staging operator.
    pub(crate) staging: Arc<Operator>,
    /// Temporary warehouse and spill root retained for object lifetime.
    pub(crate) root: TempDir,
    /// Source-read instrumentation.
    pub(crate) reads: Arc<InstrumentedStore>,
    /// Forge handle under test.
    pub(crate) forge: Arc<Forge>,
    /// Stable scheduler lease identity shared by bounded fixture passes.
    pub(crate) scheduler_owner: uuid::Uuid,
    /// Production worker owner used to drain exact durable fixture tasks.
    pub(crate) worker: ForgeWorker,
    /// The one role composition every worker in this fixture leases from.
    ///
    /// Retaining the composition rather than a raw governor is what makes
    /// the primary and sibling workers provably share a single process
    /// root instead of contending against independent ledgers.
    pub(crate) roles: vala_bifrost_redux::resources::BifrostRoleResources,
    /// Supervised completion observer shared with the worker under test.
    ///
    /// Present only for fixtures built to drive the supervised
    /// [`ForgeWorker::run`] loop through its claim-gate seam; ordinary
    /// direct-execution fixtures leave it `None`.
    pub(crate) completion: Option<ForgeWorkerCompletionObserver>,
}

/// One-shot wrapper applied to the registered catalog before Forge composition.
///
/// Lets a fixture interpose an observing or fault-injecting [`Catalog`] over
/// the production catalog Forge holds, without altering table registration,
/// which runs against the raw catalog first.
pub(crate) type CatalogDecorator = Box<dyn FnOnce(Arc<dyn Catalog>) -> Arc<dyn Catalog> + Send>;

impl Fixture {
    /// Register the Forge table through the production Bifrost catalog.
    ///
    /// # Panics
    ///
    /// Panics when production catalog registration or test-only target
    /// configuration fails because those are fixture invariants.
    pub(crate) async fn build_catalog(
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
    pub(crate) fn assert_physical_recipe(table: &iceberg::table::Table) {
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
        // The canonical layout injects nothing: the single key is the
        // built-in's own newest-first event-time declaration, and the
        // per-file-constant tenant column appears nowhere in it.
        assert_eq!(sort_fields.len(), 1);
        assert_eq!(sort_fields[0].source_id, event_time_id);
        assert_eq!(sort_fields[0].transform, Transform::Identity);
        assert_eq!(sort_fields[0].direction, SortDirection::Descending);
        assert_eq!(sort_fields[0].null_order, NullOrder::Last);
        assert!(
            sort_fields.iter().all(|field| field.source_id != tenant_id),
            "the canonical sort order must not carry data_tenant_id"
        );
    }

    /// Build a real catalog, staging store, and Forge owner.
    ///
    /// # Panics
    ///
    /// Panics when the embedded database, catalog, or fixture storage
    /// cannot be initialized; these are test-environment invariants.
    pub(crate) async fn new() -> Self {
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
    pub(crate) async fn new_with_config(
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
    pub(crate) async fn new_with_observer(observer: ForgeWorkerCompletionObserver) -> Self {
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
    pub(crate) async fn build(
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
    pub(crate) async fn schedule_and_execute(&self) -> ForgeScheduleOutcome {
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
    pub(crate) fn sibling_worker_with_retry_timeout(&self, retry_timeout: Duration) -> ForgeWorker {
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
    pub(crate) fn sibling_worker_with_config(&self, config: ForgeConfig) -> ForgeWorker {
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
    pub(crate) async fn persist_detached_prior_snapshot(&self) -> (i64, i64) {
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
    pub(crate) async fn plan_and_claim(&self) -> ForgeTaskClaim {
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
    pub(crate) fn durable_task(
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
    pub(crate) async fn seed_files(&self, count: usize, aged: bool) {
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
    pub(crate) async fn seed_files_at(&self, start: i64, count: usize, aged: bool) {
        self.seed_files_for_binding(&self.binding, start, count, aged)
            .await;
    }

    /// Seeds uniquely named Parquet objects for a selected registered table.
    ///
    /// # Panics
    ///
    /// Panics when encoding, storage, or tenant-scoped file-list writes fail.
    pub(crate) async fn seed_files_for_binding(
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
    pub(crate) async fn seed_current_recipe_files(&self, count: usize, aged: bool) {
        self.seed_files_for_binding_contract(&self.binding, 0, count, aged, true, true)
            .await;
    }

    /// Seeds legacy unmarked inputs through the same physical fixture path.
    pub(crate) async fn seed_unmarked_files(&self, count: usize, aged: bool) {
        self.seed_files_for_binding_contract(&self.binding, 0, count, aged, false, false)
            .await;
    }

    /// Seeds physical inputs with caller-selected footer and path identity.
    ///
    /// # Panics
    ///
    /// Panics when schema conversion, encoding, storage, or durable
    /// file-list persistence fails.
    pub(crate) async fn seed_files_for_binding_contract(
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
    pub(crate) async fn persist_seed_rows(
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
    pub(crate) async fn age_seeded_files(&self, binding: &TenantTableBinding) {
        sqlx::query("UPDATE vala.file_list SET created_at = now() - interval '3 minutes' WHERE data_tenant_id = $1 AND namespace=$2 AND table_name=$3").bind(self.tenant.as_uuid()).bind(&binding.logical_namespace).bind(&binding.table_name).execute(self.operator_pool.pool()).await.expect("age");
    }

    /// Registers and seeds a second table in this fixture's exact backend.
    ///
    /// # Panics
    ///
    /// Panics when catalog registration, table configuration, or seed
    /// persistence fails in the isolated integration fixture.
    pub(crate) async fn register_seeded_table(&self) -> TenantTableBinding {
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
    pub(crate) async fn delete_file_list_history(&self) -> u64 {
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
    pub(crate) async fn append_seed_manifest(&self, table: &iceberg::table::Table, index: i64) {
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
    pub(crate) async fn append_seed_manifest_at(
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
    pub(crate) async fn append_current_recipe_seed_manifest(
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
    pub(crate) async fn append_seed_manifest_paths_at(
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
                Datum::try_from_bytes(&(base + 99_999).to_le_bytes(), PrimitiveType::Timestamptz)
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
    pub(crate) async fn set_live_target_file_size(
        &self,
        table: &iceberg::table::Table,
        bytes: u64,
    ) {
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
    pub(crate) async fn prepare_two_file_live_debt(&self) -> (u64, u64) {
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
        let files = u64::try_from(group.files_for_test().len()).expect("live file count fits u64");
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
pub(crate) fn fixture_seed_event_time() -> chrono::DateTime<chrono::Utc> {
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
pub(crate) async fn fixture_seed_partition(
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
pub(crate) fn registered_granularity(
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
pub(crate) fn seed_object_path(
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

/// Assert every projected operation in one family references exact audit details.
///
/// # Panics
///
/// Panics when the tenant query fails, the family has no terminal row, or
/// any projection sequence/detail differs from its audit evidence.
pub(crate) async fn assert_terminal_family_parity(fixture: &Fixture, family: &str) {
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
pub(crate) async fn family_transition_counts(
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
pub(crate) fn operation_event(operation: &str, resource: &str, detail: AuditDetail) -> AuditEvent {
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
pub(crate) async fn append_operation(
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
    let operations = ForgeOperations::new(&event.resource, family).expect("operation family owner");
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
pub(crate) async fn assert_exact_terminal(fixture: &Fixture, resource: &str, expected_phase: &str) {
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

/// Builds exact never-published generation evidence for orphan-GC tests.
pub(crate) fn reset_detail_for_output(
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

/// Acquire the production table lease for one fixture.
///
/// # Panics
///
/// Panics when the lease query fails or another owner unexpectedly holds
/// the isolated fixture lease.
pub(crate) async fn acquire_fixture_lease(fixture: &Fixture) -> ForgeLease {
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
pub(crate) struct ProbeCatalog {
    /// Production catalog receiving every delegated operation.
    pub(crate) inner: Arc<dyn Catalog>,
    /// Count of `load_table` calls observed since construction.
    pub(crate) load_table_calls: Arc<AtomicUsize>,
    /// When set, `load_table` fails instead of delegating.
    pub(crate) fail_load: Arc<AtomicBool>,
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

    async fn list_tables(&self, namespace: &NamespaceIdent) -> iceberg::Result<Vec<TableIdent>> {
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
pub(crate) async fn make_current_files_right_sized(fixture: &Fixture) {
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
pub(crate) async fn append_expirable_history(fixture: &Fixture) {
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
pub(crate) async fn prepare_maintenance_claim(fixture: &Fixture) -> ForgeTaskClaim {
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
