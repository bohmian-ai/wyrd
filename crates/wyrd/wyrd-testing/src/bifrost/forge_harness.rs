//! Shared Postgres-backed Forge fixture for gated journeys and interleavings.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use arrow::array::{
    FixedSizeBinaryBuilder, Int32Array, Int64Array, RecordBatch, StringArray,
    TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use async_trait::async_trait;
use iceberg::table::Table;
use iceberg::{Catalog, Namespace, NamespaceIdent, TableCommit, TableCreation, TableIdent};
use iceberg::{Error as IcebergError, ErrorKind as IcebergErrorKind};
use opendal::{
    Buffer, Entry, Error as ObjectStoreError, ErrorKind as ObjectStoreErrorKind, Metadata, Operator,
};
use parquet::arrow::ArrowWriter;
use vala_bifrost_redux::catalog::{
    BifrostCatalog, CreateTableRequest, TableRef, TenantTableBinding,
};
use vala_bifrost_redux::forge::{
    Forge, ForgeBuildConfig, ForgeConfig, ForgeObjectStore, ForgeSchedulerTrigger,
    ForgeWorkerCompletionObserver,
};
use vala_bifrost_redux::maintenance::StagingFilePublisher;
use vala_bifrost_redux::maintenance::staging_file_channel;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::resources::{
    BifrostResourcePolicy, BifrostRole, BifrostRuntimeResources, ResourceSource,
    SystemResourceSnapshot,
};
use vala_bifrost_redux::schema::with_managed_columns;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_spec::DataTenantId;
use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};

use super::super::WyrdTestServer;

/// Constructs the production Bifrost bootstrap from complete Forge observations.
///
/// # Errors
///
/// Returns the production resource error when the raw observation cannot cover
/// the process reserve and enabled-role policy.
fn forge_runtime_resources(
    scratch_root: &Path,
    memory_limit_bytes: usize,
) -> Result<BifrostRuntimeResources, Box<dyn std::error::Error + Send + Sync>> {
    let runtime = BifrostRuntimeResources::from_snapshot(
        SystemResourceSnapshot {
            memory_limit_bytes,
            effective_cpu: 4,
            scratch_capacity_bytes: 10 * 1024 * 1024 * 1024,
            scratch_available_bytes: 10 * 1024 * 1024 * 1024,
            memory_source: ResourceSource::Injected,
            cpu_source: ResourceSource::Injected,
        },
        BifrostResourcePolicy {
            roles: [BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge]
                .into_iter()
                .collect(),
            memory_limit_bytes: None,
            unmanaged_reserve_bytes: None,
            scratch_limit_bytes: None,
            effective_cpu: None,
            scratch_root: scratch_root.to_owned(),
            volume_roots: None,
        },
    )?;
    Ok(runtime)
}

/// Durable objects and SQL identity used by a Forge integration test.
#[derive(Clone)]
pub struct ForgeFixture {
    /// The server-shaped Forge handle used by the journey.
    pub forge: Arc<Forge>,
    /// SQL handle retained for fixture assertions and scoped rewrites.
    pub vala: vala_sql::ValaPostgres,
    /// Operator pool retained for durable fixture assertions.
    pub operator_pool: vala_sql::OperatorPool,
    /// Iceberg catalog retained for fault-injection wrappers.
    pub catalog: Arc<dyn Catalog>,
    /// Staging operator retained for fixture object writes.
    pub staging: Arc<opendal::Operator>,
    /// Forge object-store seam retained for scoped fault injection.
    pub object_store: Arc<dyn ForgeObjectStore>,
    /// Forge-owned spill root kept alive for the fixture lifetime.
    spill_root: Arc<tempfile::TempDir>,
    /// Validated configuration used to construct the production-shaped handle.
    pub config: ForgeConfig,
    /// Shared parent budget used to construct the DataFusion memory pool.
    memory: vala_bifrost_redux::scribe::memory::BifrostMemoryGovernor,
    /// The logical and physical identity of the seeded table.
    pub binding: TenantTableBinding,
    /// The tenant that owns the table and file-list rows.
    pub tenant: DataTenantId,
}

/// Real dependency graph used to seed a Forge fixture without an HTTP server.
#[derive(Clone)]
struct ForgeFixtureResources {
    /// The production-shaped Forge owner that consumes seeded work.
    forge: Arc<Forge>,
    /// Tenant-scoped durable state used by file-list setup.
    vala: vala_sql::ValaPostgres,
    /// Privileged Forge operation pool.
    operator_pool: vala_sql::OperatorPool,
    /// Catalog owner used to create the physical test table.
    bifrost_catalog: Arc<BifrostCatalog>,
    /// Real object store shared by setup and Forge.
    staging: Arc<Operator>,
    /// Forge cleanup/rewrite object-store seam.
    object_store: Arc<dyn ForgeObjectStore>,
    /// Keep the Forge spill root alive for the complete fixture lifetime.
    spill_root: Arc<tempfile::TempDir>,
    /// Shared production memory governor.
    memory: vala_bifrost_redux::scribe::memory::BifrostMemoryGovernor,
    /// Validated configuration held by rebuilt fixtures.
    config: ForgeConfig,
}

/// Passive controls attached to real supervised Forge loops in test-tier compositions.
#[derive(Default)]
struct ForgeFixtureSupervision {
    /// Observer notified only after a production worker completes durable work.
    completion_observer: Option<ForgeWorkerCompletionObserver>,
    /// Trigger that wakes the production scheduler loop without invoking it directly.
    scheduler_trigger: Option<ForgeSchedulerTrigger>,
}

/// Owns a real Forge fixture without composing an HTTP or gRPC server.
pub struct StandaloneForgeFixture {
    /// Database lifetime guard shared by catalog, scheduler, and worker owners.
    _database: Arc<PgFixture>,
    /// Object storage lifetime guard shared by setup and Forge.
    _storage: Arc<StorageHandle>,
    /// Local backend lifetime guard.
    _storage_root: Arc<tempfile::TempDir>,
    /// Seeded Forge owner and durable table state.
    fixtures: Vec<ForgeFixture>,
}

impl StandaloneForgeFixture {
    /// Start real Postgres, catalog, object storage, and Forge owners without a server.
    ///
    /// # Errors
    ///
    /// Returns fixture, storage, catalog, or Forge construction errors. No HTTP,
    /// gRPC, `AppState`, or [`WyrdTestServer`] is created by this constructor.
    pub async fn start(table_name: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::start_topology(table_name, 1).await
    }

    /// Start one shared standalone Forge graph with the requested tenant count.
    ///
    /// # Errors
    ///
    /// Returns fixture, tenant, storage, catalog, or Forge construction errors,
    /// including a zero-tenant topology.
    pub async fn start_topology(
        table_name: &str,
        tenant_count: u32,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if tenant_count == 0 {
            return Err("standalone Forge topology needs at least one tenant".into());
        }
        let database = Arc::new(PgFixture::start().await?);
        let storage_root = Arc::new(tempfile::tempdir()?);
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: storage_root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: std::time::Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test".to_owned()),
        })
        .await?;
        let catalog = crate::server::test_redux_catalog(&database, &storage).await?;
        let config = ForgeConfig::default();
        let spill_root = Arc::new(tempfile::tempdir()?);
        let runtime_resources =
            forge_runtime_resources(spill_root.path(), 10 * 1024 * 1024 * 1024)?;
        let roles = runtime_resources
            .compose_roles()
            .map_err(|error| crate::server::WyrdTestServerError::Start(error.to_string()))?;
        let memory = roles
            .memory_ledger()
            .expect("standalone Forge fixture must own its inspection ledger");
        let staging = Arc::new(storage.operator().clone());
        let object_store: Arc<dyn ForgeObjectStore> =
            ForgeObjectStoreControl::new(Arc::clone(&staging));
        let (_, inbox) = staging_file_channel(config.max_hints_per_wake)?;
        let forge = Arc::new(Forge::new(ForgeBuildConfig {
            resources: roles.forge().ok_or_else(|| {
                crate::server::WyrdTestServerError::Start(
                    "Forge composition must issue a Forge capability".to_owned(),
                )
            })?,
            vala: database.vala_postgres().clone(),
            operator_pool: database.operator_pool().clone(),
            catalog: catalog.iceberg_catalog(),
            staging: Arc::clone(&staging),
            object_store: Arc::clone(&object_store),
            rewrite_spill_root: spill_root.path().to_owned(),
            hints: inbox,
            config: config.clone(),
            maintenance_interval: std::time::Duration::from_secs(60),
            clock: vala_bifrost_redux::forge::ForgeClock::system(),
            completion_observer: None,
            scheduler_trigger: None,
            telemetry: Arc::new(vala_bifrost_redux::forge::ForgeTelemetry::new()),
        })?);
        let resources = ForgeFixtureResources {
            forge,
            vala: database.vala_postgres().clone(),
            operator_pool: database.operator_pool().clone(),
            bifrost_catalog: catalog,
            staging,
            object_store,
            spill_root,
            memory,
            config,
        };
        let mut tenants = Vec::with_capacity(usize::try_from(tenant_count)?);
        tenants.push(database.data_tenant_id());
        for index in 1..tenant_count {
            tenants.push(
                database
                    .seed_additional_tenant(&format!("forge-benchmark-{index}"))
                    .await?,
            );
        }
        let partition_day =
            chrono::NaiveDate::from_ymd_opt(2026, 7, 14).ok_or("invalid standalone fixture day")?;
        let mut fixtures = Vec::with_capacity(tenants.len());
        for tenant in tenants {
            fixtures.push(
                seed_forge_group_with_resources(
                    resources.clone(),
                    tenant,
                    table_name,
                    false,
                    &[partition_day],
                )
                .await,
            );
        }
        Ok(Self {
            _database: database,
            _storage: storage,
            _storage_root: storage_root,
            fixtures,
        })
    }

    /// Return the seeded real Forge fixture.
    ///
    /// # Panics
    ///
    /// Panics only if the constructor invariant requiring at least one tenant
    /// fixture is violated.
    #[must_use]
    pub fn fixture(&self) -> &ForgeFixture {
        &self.fixtures[0]
    }

    /// Borrow every tenant-scoped table in the shared standalone Forge graph.
    #[must_use]
    pub fn fixtures(&self) -> &[ForgeFixture] {
        &self.fixtures
    }
}

/// Read-only probe for the DataFusion pool owned by one Forge fixture.
#[derive(Clone)]
pub struct ForgeMemoryProbe {
    /// Closure-backed reservation read that avoids exposing DataFusion types.
    sample: Arc<dyn Fn() -> usize + Send + Sync>,
}

impl ForgeMemoryProbe {
    /// Build a probe over the exact pool supplied to Forge.
    fn new(sample: impl Fn() -> usize + Send + Sync + 'static) -> Self {
        Self {
            sample: Arc::new(sample),
        }
    }

    /// Return the pool's current reservation in bytes.
    #[must_use]
    pub fn current_reserved(&self) -> usize {
        (self.sample)()
    }
}

/// Catalog wrapper used by uncertainty tests.
///
/// `update_table` delegates the real commit first, then returns one retryable
/// error. This models a lost response after the catalog durably accepted the
/// commit, so the next production tick must reconcile Iceberg state rather
/// than create a second snapshot.
#[derive(Debug)]
pub struct CommitUncertaintyCatalog {
    inner: Arc<dyn Catalog>,
    controls: Arc<CommitUncertaintyControls>,
}

/// Shared deterministic state for one or more process-local uncertainty catalogs.
///
/// A cluster gives every Forge process its own catalog and database pools, then
/// shares this narrow test control so one journey can arm the same fault across
/// all real worker processes without sharing a runtime catalog handle.
#[derive(Debug)]
pub(crate) struct CommitUncertaintyControls {
    /// Counts all delegated commit attempts across process-local wrappers.
    update_attempts: AtomicUsize,
    /// One-shot panic mode at the real catalog commit boundary.
    panic_mode: AtomicU8,
    /// Records that an armed catalog panic reached its production boundary.
    panic_reached: AtomicBool,
    /// Wakes the journey waiting for the armed catalog panic.
    panic_ready: tokio::sync::Notify,
    /// Arms one injected retryable response after a durable commit.
    fail_after_next_commit: AtomicBool,
    /// Refuses later commits after a simulated lost response.
    uncertainty_active: AtomicBool,
    /// Arms a response-boundary pause after one commit.
    pause_after_commit: AtomicBool,
    /// Whether to pause only after a commit removes retained snapshots.
    pause_after_snapshot_removal: AtomicBool,
    /// Records that the post-commit pause has been reached.
    commit_reached: AtomicBool,
    /// Selects a stale-worker response after the post-commit pause.
    reject_after_commit: AtomicBool,
    /// Wakes waiters when a post-commit pause is reached.
    commit_ready: tokio::sync::Notify,
    /// Releases a paused post-commit response.
    commit_release: tokio::sync::Notify,
    /// Records cancellation while a post-commit response was paused.
    after_commit_dropped: AtomicBool,
    /// Wakes waiters when the paused post-commit call is cancelled.
    after_commit_drop_ready: tokio::sync::Notify,
    /// Arms a pause before a delegated commit.
    pause_before_commit: AtomicBool,
    /// Records that the pre-commit pause has been reached.
    before_commit_reached: AtomicBool,
    /// Selects a stale-worker response before delegation.
    reject_before_commit: AtomicBool,
    /// Wakes waiters when a pre-commit pause is reached.
    before_commit_ready: tokio::sync::Notify,
    /// Releases a paused pre-commit call.
    before_commit_release: tokio::sync::Notify,
    /// Records cancellation while a pre-commit call was paused.
    before_commit_dropped: AtomicBool,
    /// Wakes waiters when the paused pre-commit call is cancelled.
    before_commit_drop_ready: tokio::sync::Notify,
}

/// Acknowledges that cancellation dropped a paused catalog call.
///
/// The guard remains armed only while `update_table` awaits the fixture's
/// release notification. Normal fixture release disarms it before returning to
/// the wrapped catalog; cancellation drops it and wakes the test that must
/// prove the cancellation branch won.
struct PausedCatalogCallDropAck<'a> {
    /// Atomic state that makes an acknowledgement observable before a waiter registers.
    dropped: &'a AtomicBool,
    /// Wakeup for waiters that are already suspended when cancellation drops the call.
    ready: &'a tokio::sync::Notify,
    /// Whether dropping this guard must publish cancellation acknowledgement.
    armed: bool,
}

impl<'a> PausedCatalogCallDropAck<'a> {
    /// Arm acknowledgement for one paused catalog call.
    fn new(dropped: &'a AtomicBool, ready: &'a tokio::sync::Notify) -> Self {
        Self {
            dropped,
            ready,
            armed: true,
        }
    }

    /// Disarm acknowledgement after the fixture deliberately releases the call.
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for PausedCatalogCallDropAck<'_> {
    /// Publish cancellation acknowledgement only while the catalog call is paused.
    fn drop(&mut self) {
        if self.armed {
            self.dropped.store(true, Ordering::Release);
            self.ready.notify_waiters();
        }
    }
}

/// Wait until a paused catalog call records that cancellation dropped it.
///
/// The atomic check precedes each notification wait so callers cannot miss an
/// acknowledgement that was published before they registered.
async fn wait_for_paused_catalog_call_drop(dropped: &AtomicBool, ready: &tokio::sync::Notify) {
    while !dropped.load(Ordering::Acquire) {
        ready.notified().await;
    }
}

impl CommitUncertaintyCatalog {
    /// Wrap a real catalog and arm one post-commit retryable response.
    #[must_use]
    pub fn new(inner: Arc<dyn Catalog>) -> Arc<Self> {
        Self::new_with_controls(inner, Arc::new(CommitUncertaintyControls::new()))
    }

    /// Wrap one process-local catalog with controls shared by sibling wrappers.
    ///
    #[must_use]
    pub(crate) fn new_with_controls(
        inner: Arc<dyn Catalog>,
        controls: Arc<CommitUncertaintyControls>,
    ) -> Arc<Self> {
        Arc::new(Self { inner, controls })
    }
}

impl CommitUncertaintyControls {
    /// Build the unarmed shared control state for process-local catalog wrappers.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            update_attempts: AtomicUsize::new(0),
            panic_mode: AtomicU8::new(0),
            panic_reached: AtomicBool::new(false),
            panic_ready: tokio::sync::Notify::new(),
            fail_after_next_commit: AtomicBool::new(false),
            uncertainty_active: AtomicBool::new(false),
            pause_after_commit: AtomicBool::new(false),
            pause_after_snapshot_removal: AtomicBool::new(false),
            commit_reached: AtomicBool::new(false),
            reject_after_commit: AtomicBool::new(false),
            commit_ready: tokio::sync::Notify::new(),
            commit_release: tokio::sync::Notify::new(),
            after_commit_dropped: AtomicBool::new(false),
            after_commit_drop_ready: tokio::sync::Notify::new(),
            pause_before_commit: AtomicBool::new(false),
            before_commit_reached: AtomicBool::new(false),
            reject_before_commit: AtomicBool::new(false),
            before_commit_ready: tokio::sync::Notify::new(),
            before_commit_release: tokio::sync::Notify::new(),
            before_commit_dropped: AtomicBool::new(false),
            before_commit_drop_ready: tokio::sync::Notify::new(),
        }
    }
}

impl CommitUncertaintyCatalog {
    /// Panic once immediately before the next delegated catalog commit.
    pub fn panic_before_next_commit(&self) {
        self.controls.panic_reached.store(false, Ordering::Release);
        self.controls.panic_mode.store(1, Ordering::Release);
    }

    /// Panic once after the next delegated catalog commit succeeds.
    pub fn panic_after_next_commit(&self) {
        self.controls.panic_reached.store(false, Ordering::Release);
        self.controls.panic_mode.store(2, Ordering::Release);
    }

    /// Disarm any catalog panic that has not yet fired.
    pub fn disarm_panic(&self) {
        self.controls.panic_mode.store(0, Ordering::Release);
    }

    /// Wait until an armed panic reaches the real catalog boundary.
    pub async fn wait_for_panic(&self) {
        while !self.controls.panic_reached.load(Ordering::Acquire) {
            self.controls.panic_ready.notified().await;
        }
    }

    /// Make the next successful catalog update appear retryably uncertain.
    pub fn fail_after_next_commit(&self) {
        self.controls
            .fail_after_next_commit
            .store(true, Ordering::Release);
    }

    /// Pause after the real catalog has accepted one commit.
    pub fn pause_after_commit(&self) {
        self.controls.update_attempts.store(0, Ordering::Release);
        self.controls.commit_reached.store(false, Ordering::Release);
        self.controls
            .reject_after_commit
            .store(false, Ordering::Release);
        self.controls
            .after_commit_dropped
            .store(false, Ordering::Release);
        self.controls
            .pause_after_snapshot_removal
            .store(false, Ordering::Release);
        self.controls
            .pause_after_commit
            .store(true, Ordering::Release);
    }

    /// Pause after a catalog commit semantically removes at least one snapshot.
    ///
    /// The wrapper compares retained snapshot IDs immediately before and after
    /// the delegated update. Earlier manifest rewrites therefore pass through,
    /// while the actual snapshot-expiry commit is held after acceptance.
    pub fn pause_after_snapshot_removal(&self) {
        self.controls.update_attempts.store(0, Ordering::Release);
        self.controls.commit_reached.store(false, Ordering::Release);
        self.controls
            .reject_after_commit
            .store(false, Ordering::Release);
        self.controls
            .after_commit_dropped
            .store(false, Ordering::Release);
        self.controls
            .pause_after_commit
            .store(false, Ordering::Release);
        self.controls
            .pause_after_snapshot_removal
            .store(true, Ordering::Release);
    }

    /// Wait until the real catalog commit has completed and the wrapper is
    /// holding the response boundary open.
    pub async fn wait_for_commit(&self) {
        while !self.controls.commit_reached.load(Ordering::Acquire) {
            self.controls.commit_ready.notified().await;
        }
    }

    /// Wait until cancellation drops the paused post-acceptance catalog call.
    ///
    /// The caller supplies its own timeout so the surrounding integration test
    /// controls the complete scheduler-bound assertion.
    pub async fn wait_for_after_commit_drop(&self) {
        wait_for_paused_catalog_call_drop(
            &self.controls.after_commit_dropped,
            &self.controls.after_commit_drop_ready,
        )
        .await;
    }

    /// Mark the paused caller stale and let it observe the injected response.
    pub fn reject_paused_commit(&self) {
        self.controls
            .reject_after_commit
            .store(true, Ordering::Release);
        self.controls.commit_release.notify_waiters();
    }

    /// Return catalog update attempts observed since the uncertainty seam was armed.
    #[must_use]
    pub fn update_attempts(&self) -> usize {
        self.controls.update_attempts.load(Ordering::Acquire)
    }

    /// Pause immediately before delegating a catalog commit.
    pub fn pause_before_commit(&self) {
        self.controls
            .before_commit_reached
            .store(false, Ordering::Release);
        self.controls
            .reject_before_commit
            .store(false, Ordering::Release);
        self.controls
            .before_commit_dropped
            .store(false, Ordering::Release);
        self.controls
            .pause_before_commit
            .store(true, Ordering::Release);
    }

    /// Wait until the production commit has reached the wrapped catalog seam.
    pub async fn wait_for_before_commit(&self) {
        while !self.controls.before_commit_reached.load(Ordering::Acquire) {
            self.controls.before_commit_ready.notified().await;
        }
    }

    /// Wait until cancellation drops the paused pre-acceptance catalog call.
    ///
    /// The caller supplies its own timeout so the surrounding integration test
    /// controls the complete scheduler-bound assertion.
    pub async fn wait_for_before_commit_drop(&self) {
        wait_for_paused_catalog_call_drop(
            &self.controls.before_commit_dropped,
            &self.controls.before_commit_drop_ready,
        )
        .await;
    }

    /// Reject the paused commit as stale and resume the caller.
    pub fn reject_paused_before_commit(&self) {
        self.controls
            .reject_before_commit
            .store(true, Ordering::Release);
        self.controls.before_commit_release.notify_waiters();
    }
}

#[async_trait]
impl Catalog for CommitUncertaintyCatalog {
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
        self.controls.update_attempts.fetch_add(1, Ordering::AcqRel);
        if self
            .controls
            .panic_mode
            .compare_exchange(1, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.controls.panic_reached.store(true, Ordering::Release);
            self.controls.panic_ready.notify_waiters();
            panic!("injected Forge panic immediately before catalog commit");
        }
        if self.controls.uncertainty_active.load(Ordering::Acquire) {
            return Err(IcebergError::new(
                IcebergErrorKind::Unexpected,
                "injected post-commit uncertainty remains unresolved",
            )
            .with_retryable(true));
        }
        if self
            .controls
            .pause_before_commit
            .swap(false, Ordering::AcqRel)
        {
            let mut drop_ack = PausedCatalogCallDropAck::new(
                &self.controls.before_commit_dropped,
                &self.controls.before_commit_drop_ready,
            );
            self.controls
                .before_commit_reached
                .store(true, Ordering::Release);
            self.controls.before_commit_ready.notify_waiters();
            self.controls.before_commit_release.notified().await;
            drop_ack.disarm();
            if self.controls.reject_before_commit.load(Ordering::Acquire) {
                return Err(IcebergError::new(
                    IcebergErrorKind::Unexpected,
                    "injected stale Forge lease before catalog commit",
                )
                .with_retryable(false));
            }
        }
        let observe_snapshot_removal = self
            .controls
            .pause_after_snapshot_removal
            .load(Ordering::Acquire);
        let snapshots_before = if observe_snapshot_removal {
            Some(
                self.inner
                    .load_table(commit.identifier())
                    .await?
                    .metadata()
                    .snapshots()
                    .map(|snapshot| snapshot.snapshot_id())
                    .collect::<std::collections::BTreeSet<_>>(),
            )
        } else {
            None
        };
        let table = self.inner.update_table(commit).await?;
        if self
            .controls
            .panic_mode
            .compare_exchange(2, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.controls.panic_reached.store(true, Ordering::Release);
            self.controls.panic_ready.notify_waiters();
            panic!("injected Forge panic after catalog commit before acknowledgement");
        }
        let removed_snapshot = snapshots_before.is_some_and(|before| {
            let after = table
                .metadata()
                .snapshots()
                .map(|snapshot| snapshot.snapshot_id())
                .collect::<std::collections::BTreeSet<_>>();
            before.difference(&after).next().is_some()
        });
        let pause_after_commit = self
            .controls
            .pause_after_commit
            .swap(false, Ordering::AcqRel)
            || (removed_snapshot
                && self
                    .controls
                    .pause_after_snapshot_removal
                    .swap(false, Ordering::AcqRel));
        if pause_after_commit {
            let mut drop_ack = PausedCatalogCallDropAck::new(
                &self.controls.after_commit_dropped,
                &self.controls.after_commit_drop_ready,
            );
            self.controls.commit_reached.store(true, Ordering::Release);
            self.controls.commit_ready.notify_waiters();
            self.controls.commit_release.notified().await;
            drop_ack.disarm();
            if self.controls.reject_after_commit.load(Ordering::Acquire) {
                return Err(IcebergError::new(
                    IcebergErrorKind::Unexpected,
                    "injected stale Forge lease after catalog commit",
                )
                .with_retryable(true));
            }
        }
        if self
            .controls
            .fail_after_next_commit
            .swap(false, Ordering::AcqRel)
        {
            self.controls
                .uncertainty_active
                .store(true, Ordering::Release);
            return Err(IcebergError::new(
                IcebergErrorKind::Unexpected,
                "injected post-commit uncertainty",
            )
            .with_retryable(true));
        }
        Ok(table)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use opendal::Operator;

    use super::{
        AtomicBool, ForgeObjectStore, ForgeObjectStoreControl, Ordering, PausedCatalogCallDropAck,
        wait_for_paused_catalog_call_drop,
    };

    /// Dropping an armed paused-call guard publishes an acknowledgement even before waiting starts.
    #[tokio::test]
    async fn paused_catalog_call_drop_acknowledges_without_a_lost_notification() {
        let dropped = AtomicBool::new(false);
        let ready = tokio::sync::Notify::new();
        drop(PausedCatalogCallDropAck::new(&dropped, &ready));

        tokio::time::timeout(
            Duration::from_secs(1),
            wait_for_paused_catalog_call_drop(&dropped, &ready),
        )
        .await
        .expect("drop acknowledgement must remain observable");
    }

    /// Normal fixture release disarms the paused-call guard without publishing cancellation.
    #[test]
    fn paused_catalog_call_normal_release_disarms_drop_acknowledgement() {
        let dropped = AtomicBool::new(false);
        let ready = tokio::sync::Notify::new();
        let mut acknowledgement = PausedCatalogCallDropAck::new(&dropped, &ready);
        acknowledgement.disarm();
        drop(acknowledgement);

        assert!(!dropped.load(Ordering::Acquire));
    }

    /// The post-PUT control pauses one armed call while counting every successful notification.
    #[tokio::test]
    async fn output_put_control_is_one_shot_and_records_path() {
        let operator = Operator::new(opendal::services::Memory::default())
            .expect("memory operator builder")
            .finish();
        let control = ForgeObjectStoreControl::new(Arc::new(operator));
        control.pause_after_next_output_put();
        let paused = tokio::spawn({
            let control = Arc::clone(&control);
            async move {
                control
                    .after_output_put("table/data/forge/first.parquet")
                    .await;
            }
        });
        tokio::time::timeout(Duration::from_secs(1), control.wait_for_output_put())
            .await
            .expect("armed post-PUT notification must become observable");
        assert_eq!(
            control.last_output_path().as_deref(),
            Some("table/data/forge/first.parquet")
        );
        assert_eq!(control.output_put_calls(), 1);
        control.release_output_put();
        tokio::time::timeout(Duration::from_secs(1), paused)
            .await
            .expect("released post-PUT task completion bound")
            .expect("released post-PUT task");

        tokio::time::timeout(
            Duration::from_secs(1),
            control.after_output_put("table/data/forge/second.parquet"),
        )
        .await
        .expect("unarmed notification must return immediately");
        assert_eq!(control.output_put_calls(), 2);
        assert_eq!(
            control.last_output_path().as_deref(),
            Some("table/data/forge/first.parquet")
        );
    }
}

/// Scoped OpenDAL controls for deterministic list/delete interleavings.
///
/// The wrapper delegates every operation to the real local filesystem. A test
/// may pause one list return to insert a durable file-list reference, or fail
/// one selected delete, without changing production Forge behavior.
#[derive(Debug)]
pub struct ForgeObjectStoreControl {
    /// Real production-shaped operator used for every delegated operation.
    inner: Arc<Operator>,
    /// One-shot arm flag consumed by the next successful output notification.
    pause_next_output_put: AtomicBool,
    /// Durable-in-process signal that the armed notification reached its pause.
    output_put_reached: AtomicBool,
    /// Wakes tasks waiting for the armed successful output notification.
    output_put_ready: tokio::sync::Notify,
    /// Releases the armed notification after the test schedules its interleaving.
    output_put_release: tokio::sync::Notify,
    /// Counts every successful output PUT notification, armed or unarmed.
    output_put_calls: AtomicUsize,
    /// Retains the exact path observed by the most recent armed notification.
    last_output_path: Mutex<Option<String>>,
    /// One-shot arm flag for the next delegated object listing.
    pause_next_list: AtomicBool,
    /// Signals that the armed listing has returned from the real operator.
    list_returned: AtomicBool,
    /// Wakes tasks waiting for the armed listing.
    list_ready: tokio::sync::Notify,
    /// Releases the armed listing after its deterministic interleaving.
    list_release: tokio::sync::Notify,
    /// Counts delegated delete attempts.
    delete_count: AtomicUsize,
    /// Exact paths supplied to delegated deletes in call order.
    delete_paths: Mutex<Vec<String>>,
    /// Selects one one-based delete call for injected failure.
    fail_delete_at: AtomicUsize,
    /// One-shot arm flag for the next delegated delete.
    pause_next_delete: AtomicBool,
    /// Signals that the armed delete reached the wrapper boundary.
    delete_reached: AtomicBool,
    /// Selects whether the armed delete returns a stale-worker error.
    reject_delete: AtomicBool,
    /// Wakes tasks waiting for the armed delete.
    delete_ready: tokio::sync::Notify,
    /// Releases the armed delete after its deterministic interleaving.
    delete_release: tokio::sync::Notify,
    /// One exact object path paused after its real delete returns.
    pause_after_delete_path: Mutex<Option<String>>,
    /// Signals that the armed real delete completed successfully.
    delete_completed: AtomicBool,
    /// Wakes tests waiting at the post-delete cancellation boundary.
    delete_completed_ready: tokio::sync::Notify,
    /// Releases the armed post-delete boundary back to Forge.
    delete_completed_release: tokio::sync::Notify,
}

impl ForgeObjectStoreControl {
    /// Wrap the real staging operator used by a Forge context.
    #[must_use]
    pub fn new(inner: Arc<Operator>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            pause_next_output_put: AtomicBool::new(false),
            output_put_reached: AtomicBool::new(false),
            output_put_ready: tokio::sync::Notify::new(),
            output_put_release: tokio::sync::Notify::new(),
            output_put_calls: AtomicUsize::new(0),
            last_output_path: Mutex::new(None),
            pause_next_list: AtomicBool::new(false),
            list_returned: AtomicBool::new(false),
            list_ready: tokio::sync::Notify::new(),
            list_release: tokio::sync::Notify::new(),
            delete_count: AtomicUsize::new(0),
            delete_paths: Mutex::new(Vec::new()),
            fail_delete_at: AtomicUsize::new(0),
            pause_next_delete: AtomicBool::new(false),
            delete_reached: AtomicBool::new(false),
            reject_delete: AtomicBool::new(false),
            delete_ready: tokio::sync::Notify::new(),
            delete_release: tokio::sync::Notify::new(),
            pause_after_delete_path: Mutex::new(None),
            delete_completed: AtomicBool::new(false),
            delete_completed_ready: tokio::sync::Notify::new(),
            delete_completed_release: tokio::sync::Notify::new(),
        })
    }

    /// Pause the next successful rewrite output after its real PUT completes.
    pub fn pause_after_next_output_put(&self) {
        self.output_put_reached.store(false, Ordering::Release);
        *self
            .last_output_path
            .lock()
            .expect("Forge output-path observation lock must not be poisoned") = None;
        self.pause_next_output_put.store(true, Ordering::Release);
    }

    /// Wait until the armed output has crossed the successful real PUT boundary.
    pub async fn wait_for_output_put(&self) {
        while !self.output_put_reached.load(Ordering::Acquire) {
            self.output_put_ready.notified().await;
        }
    }

    /// Resume the post-PUT notification without changing output ownership.
    pub fn release_output_put(&self) {
        self.output_put_release.notify_one();
    }

    /// Return the number of successful rewrite PUT notifications observed.
    #[must_use]
    pub fn output_put_calls(&self) -> usize {
        self.output_put_calls.load(Ordering::Acquire)
    }

    /// Return the exact path captured by the most recent armed notification.
    #[must_use]
    pub fn last_output_path(&self) -> Option<String> {
        self.last_output_path
            .lock()
            .expect("Forge output-path observation lock must not be poisoned")
            .clone()
    }

    /// Pause one list after the real object-store response is available.
    pub fn pause_next_list(&self) {
        self.list_returned.store(false, Ordering::Release);
        self.pause_next_list.store(true, Ordering::Release);
    }

    /// Wait until the paused list has completed against the real store.
    pub async fn wait_for_list(&self) {
        while !self.list_returned.load(Ordering::Acquire) {
            self.list_ready.notified().await;
        }
    }

    /// Resume the paused list call.
    pub fn release_list(&self) {
        self.list_release.notify_waiters();
    }

    /// Fail the selected one-based delete call with a transient object error.
    pub fn fail_delete_at(&self, call: usize) {
        self.fail_delete_at.store(call, Ordering::Release);
    }

    /// Pause immediately before one real delete call.
    pub fn pause_next_delete(&self) {
        self.delete_reached.store(false, Ordering::Release);
        self.reject_delete.store(false, Ordering::Release);
        self.pause_next_delete.store(true, Ordering::Release);
    }

    /// Wait until the selected delete reached the wrapped OpenDAL boundary.
    pub async fn wait_for_delete(&self) {
        while !self.delete_reached.load(Ordering::Acquire) {
            self.delete_ready.notified().await;
        }
    }

    /// Resume the paused delete without injecting a storage failure.
    pub fn release_paused_delete(&self) {
        self.delete_release.notify_one();
    }

    /// Pause after the exact real delete succeeds but before Forge observes its return.
    pub fn pause_after_delete_for_path(&self, path: &str) {
        self.delete_completed.store(false, Ordering::Release);
        *self
            .pause_after_delete_path
            .lock()
            .expect("Forge post-delete path lock must not be poisoned") = Some(path.to_owned());
    }

    /// Wait until the armed real delete has completed successfully.
    pub async fn wait_for_completed_delete(&self) {
        loop {
            let notified = self.delete_completed_ready.notified();
            if self.delete_completed.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    /// Resume Forge after the armed real delete has completed.
    pub fn release_completed_delete(&self) {
        self.delete_completed_release.notify_one();
    }

    /// Reject the paused delete as a stale-worker effect and resume it.
    pub fn reject_paused_delete(&self) {
        self.reject_delete.store(true, Ordering::Release);
        self.delete_release.notify_waiters();
    }

    /// Return the number of delete calls that reached this scoped wrapper.
    #[must_use]
    pub fn delete_calls(&self) -> usize {
        self.delete_count.load(Ordering::Acquire)
    }

    /// Return the exact paths supplied to real deletes in call order.
    #[must_use]
    pub fn delete_paths(&self) -> Vec<String> {
        self.delete_paths
            .lock()
            .expect("Forge delete-path lock must not be poisoned")
            .clone()
    }
}

#[async_trait]
impl ForgeObjectStore for ForgeObjectStoreControl {
    /// Observe a durable output and optionally pause the one armed call.
    ///
    /// The real write has already completed through the production staging
    /// operator. This infallible notification only records and synchronizes;
    /// it never performs a write or changes output ownership.
    async fn after_output_put(&self, path: &str) {
        self.output_put_calls.fetch_add(1, Ordering::AcqRel);
        if !self.pause_next_output_put.swap(false, Ordering::AcqRel) {
            return;
        }
        *self
            .last_output_path
            .lock()
            .expect("Forge output-path observation lock must not be poisoned") =
            Some(path.to_owned());
        self.output_put_reached.store(true, Ordering::Release);
        self.output_put_ready.notify_waiters();
        self.output_put_release.notified().await;
    }

    /// Read exactly the requested byte range through OpenDAL's native reader.
    ///
    /// Forge uses bounded reads for Parquet metadata and row-group admission;
    /// fetching the complete object here would bypass that memory guardrail.
    ///
    /// # Errors
    ///
    /// Returns the OpenDAL error when the reader cannot open or fetch the
    /// requested range.
    async fn read_range(&self, path: &str, range: std::ops::Range<u64>) -> opendal::Result<Buffer> {
        self.inner.reader(path).await?.read(range).await
    }

    async fn read(&self, path: &str) -> opendal::Result<Buffer> {
        self.inner.read(path).await
    }

    async fn list(&self, prefix: &str) -> opendal::Result<Vec<Entry>> {
        let entries = self.inner.list_with(prefix).recursive(true).await?;
        if self.pause_next_list.swap(false, Ordering::AcqRel) {
            self.list_returned.store(true, Ordering::Release);
            self.list_ready.notify_waiters();
            self.list_release.notified().await;
        }
        Ok(entries)
    }

    async fn stat(&self, path: &str) -> opendal::Result<Metadata> {
        self.inner.stat(path).await
    }

    async fn delete(&self, path: &str) -> opendal::Result<()> {
        let call = self.delete_count.fetch_add(1, Ordering::AcqRel) + 1;
        self.delete_paths
            .lock()
            .expect("Forge delete-path lock must not be poisoned")
            .push(path.to_owned());
        if call == self.fail_delete_at.load(Ordering::Acquire) {
            return Err(ObjectStoreError::new(
                ObjectStoreErrorKind::Unexpected,
                "injected Forge delete failure",
            ));
        }
        if self.pause_next_delete.swap(false, Ordering::AcqRel) {
            self.delete_reached.store(true, Ordering::Release);
            self.delete_ready.notify_waiters();
            self.delete_release.notified().await;
            if self.reject_delete.load(Ordering::Acquire) {
                return Err(ObjectStoreError::new(
                    ObjectStoreErrorKind::Unexpected,
                    "injected stale Forge lease before delete",
                ));
            }
        }
        self.inner.delete(path).await?;
        let pause_after_delete = {
            let mut target = self
                .pause_after_delete_path
                .lock()
                .expect("Forge post-delete path lock must not be poisoned");
            if target.as_deref() == Some(path) {
                target.take();
                true
            } else {
                false
            }
        };
        if pause_after_delete {
            self.delete_completed.store(true, Ordering::Release);
            self.delete_completed_ready.notify_waiters();
            self.delete_completed_release.notified().await;
        }
        Ok(())
    }
}

impl ForgeFixture {
    /// Clone the server-owned context with a test-specific validated config.
    #[must_use]
    pub fn context_with_config(&self, config: ForgeConfig) -> Arc<Forge> {
        self.build_forge(
            config,
            Arc::clone(&self.catalog),
            Arc::clone(&self.object_store),
        )
    }

    /// Clone the server-owned graph with passive controls for its real supervised loops.
    ///
    /// The returned owner still executes only through [`Forge::run`] and
    /// `ForgeWorker::run`; the controls observe completion and request a normal
    /// production scheduler wakeup without planning or executing work themselves.
    #[must_use]
    pub fn context_with_supervision(
        &self,
        config: ForgeConfig,
        completion_observer: ForgeWorkerCompletionObserver,
        scheduler_trigger: ForgeSchedulerTrigger,
    ) -> Arc<Forge> {
        self.build_forge_with_publisher_and_memory_probe_and_supervision(
            config,
            Arc::clone(&self.catalog),
            Arc::clone(&self.object_store),
            None,
            ForgeFixtureSupervision {
                completion_observer: Some(completion_observer),
                scheduler_trigger: Some(scheduler_trigger),
            },
        )
        .map(|(forge, _publisher, _probe)| forge)
        .expect("validated Forge fixture config")
    }

    /// Build a worker-observed Forge graph with explicit catalog and object-store seams.
    ///
    /// The returned publisher owns the matching production hint inbox. The
    /// observer remains passive; planning and execution still belong to the
    /// production `Forge::run` and `ForgeWorker::run` supervisors.
    #[must_use]
    pub fn context_with_worker_supervision(
        &self,
        config: ForgeConfig,
        catalog: Arc<dyn Catalog>,
        object_store: Arc<dyn ForgeObjectStore>,
        completion_observer: ForgeWorkerCompletionObserver,
        scheduler_trigger: ForgeSchedulerTrigger,
    ) -> (Arc<Forge>, StagingFilePublisher) {
        self.build_forge_with_publisher_and_memory_probe_and_supervision(
            config,
            catalog,
            object_store,
            None,
            ForgeFixtureSupervision {
                completion_observer: Some(completion_observer),
                scheduler_trigger: Some(scheduler_trigger),
            },
        )
        .map(|(forge, publisher, _probe)| (forge, publisher))
        .expect("validated Forge fixture config")
    }

    /// Builds a supervised Forge worker against an explicit scratch root.
    ///
    /// This read-only configuration seam lets gated journeys exercise real
    /// filesystem health failures without adding production fault injection.
    #[must_use]
    pub fn context_with_worker_supervision_and_spill_root(
        &self,
        config: ForgeConfig,
        completion_observer: ForgeWorkerCompletionObserver,
        scheduler_trigger: ForgeSchedulerTrigger,
        spill_root: &std::path::Path,
    ) -> Arc<Forge> {
        self.build_forge_with_publisher_and_memory_probe_and_supervision_at(
            config,
            Arc::clone(&self.catalog),
            Arc::clone(&self.object_store),
            None,
            ForgeFixtureSupervision {
                completion_observer: Some(completion_observer),
                scheduler_trigger: Some(scheduler_trigger),
            },
            Some(spill_root),
        )
        .map(|(forge, _publisher, _probe)| forge)
        .expect("validated Forge fixture spill-root config")
    }

    /// Build a production-shaped Forge together with its paired local hint
    /// publisher for scheduler journey tests.
    #[must_use]
    pub fn context_with_config_and_publisher(
        &self,
        config: ForgeConfig,
    ) -> (Arc<Forge>, StagingFilePublisher) {
        self.build_forge_with_publisher(
            config,
            Arc::clone(&self.catalog),
            Arc::clone(&self.object_store),
        )
    }

    /// Build a Forge graph from a constrained process-memory observation.
    #[must_use]
    pub fn context_with_constrained_memory_and_publisher(
        &self,
        config: ForgeConfig,
        system_memory_limit_bytes: usize,
    ) -> (Arc<Forge>, StagingFilePublisher) {
        self.build_forge_with_publisher_and_memory_probe(
            config,
            Arc::clone(&self.catalog),
            Arc::clone(&self.object_store),
            Some(system_memory_limit_bytes),
        )
        .map(|(forge, publisher, _probe)| (forge, publisher))
        .expect("validated Forge fixture config")
    }

    /// Build from a constrained process observation and expose root ownership.
    #[must_use]
    pub fn context_with_constrained_memory_and_probe(
        &self,
        config: ForgeConfig,
        system_memory_limit_bytes: usize,
    ) -> (Arc<Forge>, StagingFilePublisher, ForgeMemoryProbe) {
        self.build_forge_with_publisher_and_memory_probe(
            config,
            Arc::clone(&self.catalog),
            Arc::clone(&self.object_store),
            Some(system_memory_limit_bytes),
        )
        .expect("validated Forge fixture config")
    }

    /// Build a Forge with a scoped catalog and its paired local hint publisher.
    ///
    /// The publisher remains connected to the exact inbox owned by the
    /// returned Forge, allowing interleaving tests to drive the hinted
    /// scheduler branch rather than a periodic fallback.
    #[must_use]
    pub fn context_with_catalog_and_publisher(
        &self,
        config: ForgeConfig,
        catalog: Arc<dyn Catalog>,
    ) -> (Arc<Forge>, StagingFilePublisher) {
        self.build_forge_with_publisher(config, catalog, Arc::clone(&self.object_store))
    }

    /// Return a read-only snapshot of the shared Bifrost memory parent.
    #[must_use]
    pub fn memory_snapshot(&self) -> vala_bifrost_redux::scribe::memory::MemorySnapshot {
        self.memory.snapshot()
    }

    /// Report whether the Forge-owned spill root has no retained children.
    #[must_use]
    pub fn spill_root_is_empty(&self) -> bool {
        directory_tree_is_empty(self.spill_root.path())
    }

    /// Clone the context with a scoped catalog wrapper for one test journey.
    pub fn context_with_catalog(
        &self,
        config: ForgeConfig,
        catalog: Arc<dyn Catalog>,
    ) -> Arc<Forge> {
        self.build_forge(config, catalog, Arc::clone(&self.object_store))
    }

    /// Clone the context with a scoped OpenDAL wrapper for one test journey.
    pub fn context_with_object_store<S>(
        &self,
        config: ForgeConfig,
        object_store: Arc<S>,
    ) -> Arc<Forge>
    where
        S: ForgeObjectStore + 'static,
    {
        self.build_forge(config, Arc::clone(&self.catalog), object_store)
    }

    /// Build a Forge with a scoped object-store wrapper and its paired hint publisher.
    #[must_use]
    pub fn context_with_object_store_and_publisher<S>(
        &self,
        config: ForgeConfig,
        object_store: Arc<S>,
    ) -> (Arc<Forge>, StagingFilePublisher)
    where
        S: ForgeObjectStore + 'static,
    {
        self.build_forge_with_publisher(config, Arc::clone(&self.catalog), object_store)
    }

    /// Build a Forge with the production dependency shape and scoped seams.
    fn build_forge(
        &self,
        config: ForgeConfig,
        catalog: Arc<dyn Catalog>,
        object_store: Arc<dyn ForgeObjectStore>,
    ) -> Arc<Forge> {
        self.build_forge_with_publisher(config, catalog, object_store)
            .0
    }

    /// Construct one Forge graph and retain the publisher paired with its inbox.
    fn build_forge_with_publisher(
        &self,
        config: ForgeConfig,
        catalog: Arc<dyn Catalog>,
        object_store: Arc<dyn ForgeObjectStore>,
    ) -> (Arc<Forge>, StagingFilePublisher) {
        self.build_forge_with_publisher_and_memory(config, catalog, object_store, None)
    }

    /// Construct a Forge graph from an optional raw process-memory observation.
    fn build_forge_with_publisher_and_memory(
        &self,
        config: ForgeConfig,
        catalog: Arc<dyn Catalog>,
        object_store: Arc<dyn ForgeObjectStore>,
        memory_limit_bytes: Option<usize>,
    ) -> (Arc<Forge>, StagingFilePublisher) {
        let (forge, publisher, _probe) = self
            .build_forge_with_publisher_and_memory_probe(
                config,
                catalog,
                object_store,
                memory_limit_bytes,
            )
            .expect("validated Forge fixture config");
        (forge, publisher)
    }

    /// Construct Forge and a probe over its exact DataFusion pool.
    ///
    /// # Errors
    ///
    /// Returns an error when the supplied Forge configuration is invalid.
    fn build_forge_with_publisher_and_memory_probe(
        &self,
        config: ForgeConfig,
        catalog: Arc<dyn Catalog>,
        object_store: Arc<dyn ForgeObjectStore>,
        memory_limit_bytes: Option<usize>,
    ) -> Result<(Arc<Forge>, StagingFilePublisher, ForgeMemoryProbe), &'static str> {
        self.build_forge_with_publisher_and_memory_probe_and_supervision(
            config,
            catalog,
            object_store,
            memory_limit_bytes,
            ForgeFixtureSupervision::default(),
        )
    }

    /// Construct Forge, its memory probe, and optional passive supervisor controls.
    ///
    /// # Errors
    ///
    /// Returns an error when the supplied Forge configuration is invalid.
    fn build_forge_with_publisher_and_memory_probe_and_supervision(
        &self,
        config: ForgeConfig,
        catalog: Arc<dyn Catalog>,
        object_store: Arc<dyn ForgeObjectStore>,
        memory_limit_bytes: Option<usize>,
        supervision: ForgeFixtureSupervision,
    ) -> Result<(Arc<Forge>, StagingFilePublisher, ForgeMemoryProbe), &'static str> {
        self.build_forge_with_publisher_and_memory_probe_and_supervision_at(
            config,
            catalog,
            object_store,
            memory_limit_bytes,
            supervision,
            None,
        )
    }

    /// Constructs Forge with an optional caller-owned scratch root.
    fn build_forge_with_publisher_and_memory_probe_and_supervision_at(
        &self,
        config: ForgeConfig,
        catalog: Arc<dyn Catalog>,
        object_store: Arc<dyn ForgeObjectStore>,
        memory_limit_bytes: Option<usize>,
        supervision: ForgeFixtureSupervision,
        spill_root: Option<&std::path::Path>,
    ) -> Result<(Arc<Forge>, StagingFilePublisher, ForgeMemoryProbe), &'static str> {
        let (publisher, inbox) =
            staging_file_channel(config.max_hints_per_wake).expect("validated Forge hint capacity");
        let runtime_root = spill_root.map_or_else(
            || {
                self.spill_root
                    .path()
                    .join(format!("forge-pod-{}", uuid::Uuid::now_v7()))
            },
            std::path::Path::to_path_buf,
        );
        let runtime_resources = forge_runtime_resources(
            &runtime_root,
            memory_limit_bytes.unwrap_or(10 * 1024 * 1024 * 1024),
        )
        .map_err(|_| "invalid Forge resource fixture")?;
        let roles = runtime_resources
            .compose_roles()
            .map_err(|_| "invalid Forge resource composition")?;
        let forge_resources = roles
            .forge()
            .ok_or("Forge composition must issue a Forge capability")?;
        let probe_resources = forge_resources.clone();
        let probe = ForgeMemoryProbe::new(move || {
            probe_resources
                .snapshot()
                .map_or(0, |snapshot| snapshot.elastic_memory_used_bytes)
        });
        let forge = Arc::new(
            Forge::new(ForgeBuildConfig {
                resources: forge_resources,
                vala: self.vala.clone(),
                operator_pool: self.operator_pool.clone(),
                catalog,
                staging: Arc::clone(&self.staging),
                object_store,
                rewrite_spill_root: runtime_root,
                hints: inbox,
                config,
                maintenance_interval: std::time::Duration::from_secs(60),
                clock: self.forge.clock_for_test(),
                completion_observer: supervision.completion_observer,
                scheduler_trigger: supervision.scheduler_trigger,
                telemetry: std::sync::Arc::new(vala_bifrost_redux::forge::ForgeTelemetry::new()),
            })
            .map_err(|_| "invalid Forge fixture config")?,
        );
        Ok((forge, publisher, probe))
    }

    /// Append one aged Scribe-shaped Parquet file and its durable file-list row.
    pub async fn append_forge_file(&self, sequence: i64) {
        self.append_forge_file_for_day(
            sequence,
            chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("partition day"),
            true,
        )
        .await;
    }

    /// Append a large aged file for spill-focused sustained journeys.
    ///
    /// # Panics
    ///
    /// Panics when the requested row count cannot be represented in the
    /// durable fixture metadata or the test object cannot be encoded.
    pub async fn append_forge_file_with_rows(&self, sequence: i64, rows: usize) {
        self.append_forge_file_for_day_with_rows(
            sequence,
            chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("partition day"),
            true,
            rows,
        )
        .await;
    }

    /// Append one Scribe-shaped file for an explicit partition day.
    pub async fn append_forge_file_for_day(
        &self,
        sequence: i64,
        partition_day: chrono::NaiveDate,
        aged: bool,
    ) {
        self.append_forge_file_for_day_with_rows(sequence, partition_day, aged, 1)
            .await;
    }

    /// Append a Scribe-shaped file with an explicit row count and partition.
    ///
    /// # Panics
    ///
    /// Panics when `rows` is zero or the test object cannot be encoded.
    async fn append_forge_file_for_day_with_rows(
        &self,
        sequence: i64,
        partition_day: chrono::NaiveDate,
        aged: bool,
        rows: usize,
    ) {
        assert!(rows > 0, "Forge fixture files must contain rows");
        let schema = ArrowSchema::new(with_managed_columns(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let base = partition_day
            .and_hms_opt(12, 0, 0)
            .expect("Forge fixture timestamp")
            .and_utc()
            .timestamp_micros()
            + sequence * 1_000_000;
        let values = (0..rows)
            .map(|offset| {
                sequence.saturating_mul(1_000_000)
                    + i64::try_from(offset).expect("Forge fixture row offset")
            })
            .collect::<Vec<_>>();
        let times = (0..rows)
            .map(|offset| base + i64::try_from(offset).expect("row offset") * 1_000)
            .collect::<Vec<_>>();
        let tenants = vec![self.tenant.to_string(); rows];
        let mut batch_ids = FixedSizeBinaryBuilder::with_capacity(rows, 16);
        for _ in 0..rows {
            batch_ids
                .append_value([0_u8; 16])
                .expect("fixed batch identifier");
        }
        let batch = RecordBatch::try_new(
            Arc::new(schema.clone()),
            vec![
                Arc::new(Int64Array::from(values)),
                Arc::new(StringArray::from(vec![None::<&str>; rows])),
                Arc::new(StringArray::from(vec![None::<&str>; rows])),
                Arc::new(StringArray::from(vec!["principal"; rows])),
                Arc::new(StringArray::from(vec!["request"; rows])),
                Arc::new(TimestampMicrosecondArray::from(times).with_timezone("UTC")),
                Arc::new(
                    TimestampMicrosecondArray::from(
                        (0..rows)
                            .map(|offset| base + i64::try_from(offset).expect("row offset") * 1_000)
                            .collect::<Vec<_>>(),
                    )
                    .with_timezone("UTC"),
                ),
                Arc::new(batch_ids.finish()),
                Arc::new(Int32Array::from(
                    (0..rows)
                        .map(|offset| i32::try_from(offset).expect("row ordinal"))
                        .collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(tenants)),
            ],
        )
        .expect("Forge fixture append batch");
        let mut bytes = Vec::new();
        let mut writer =
            ArrowWriter::try_new(&mut bytes, batch.schema(), None).expect("Parquet writer");
        writer.write(&batch).expect("Parquet batch");
        writer.close().expect("Parquet close");
        let path = format!(
            "{}/sustained-{sequence}.parquet",
            self.binding.object_prefix
        );
        let file_size = i64::try_from(bytes.len()).expect("Forge fixture file size");
        self.staging
            .write(&path, Buffer::from(bytes))
            .await
            .expect("Forge fixture append object");
        let file_id = uuid::Uuid::now_v7();
        sqlx::query(
            "INSERT INTO vala.file_list (id, data_tenant_id, namespace, table_name, file_path, file_size, row_count, min_event_time, max_event_time, partition_day, node_id, writer_epoch, wal_lsn_min, wal_lsn_max) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(file_id)
        .bind(self.tenant.as_uuid())
        .bind(&self.binding.logical_namespace)
        .bind(&self.binding.table_name)
        .bind(&path)
        .bind(file_size)
        .bind(i64::try_from(rows).expect("Forge fixture row count"))
        .bind(chrono::DateTime::from_timestamp_micros(base).expect("timestamp"))
        .bind(chrono::DateTime::from_timestamp_micros(base + 1_000_000).expect("timestamp"))
        .bind(partition_day)
        .bind(uuid::Uuid::now_v7())
        .bind(1_i64)
        .bind(sequence * 2 + 1)
        .bind(sequence * 2 + 2)
        .execute(self.operator_pool.pool())
        .await
        .expect("Forge fixture file-list append");
        if aged {
            sqlx::query(
                "UPDATE vala.file_list SET created_at = now() - interval '3 minutes' WHERE id = $1",
            )
            .bind(file_id)
            .execute(self.operator_pool.pool())
            .await
            .expect("Forge fixture append aging");
        }
    }

    /// Add a pending `file_list` reference for an existing object.
    ///
    /// GC race tests call this after the real object listing has paused. The
    /// next production live-set rebuild must therefore protect the path and
    /// skip deletion even though it was absent from the initial live set.
    pub async fn protect_path(&self, path: &str) {
        sqlx::query(
            "INSERT INTO vala.file_list (id, data_tenant_id, namespace, table_name, file_path, file_size, row_count, min_event_time, max_event_time, partition_day, node_id, writer_epoch, wal_lsn_min, wal_lsn_max) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(self.tenant.as_uuid())
        .bind(&self.binding.logical_namespace)
        .bind(&self.binding.table_name)
        .bind(path)
        .bind(1_i64)
        .bind(1_i64)
        .bind(chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z").expect("timestamp"))
        .bind(chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:01Z").expect("timestamp"))
        .bind(chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("partition day"))
        .bind(uuid::Uuid::now_v7())
        .bind(1_i64)
        .bind(1_i64)
        .bind(1_i64)
        .execute(self.operator_pool.pool())
        .await
        .expect("Forge fixture live reference");
    }

    /// Count one tenant/table-scoped Forge audit operation.
    pub async fn operation_count(&self, operation: &str) -> i64 {
        let mut conn = self
            .vala
            .tenant_conn(self.tenant)
            .await
            .expect("Forge fixture audit tenant connection");
        sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = wyrd.current_tenant() AND resource = $1 AND operation = $2",
        )
        .bind(format!(
            "bifrost://{}/{}/{}",
            self.tenant, self.binding.logical_namespace, self.binding.table_name
        ))
        .bind(operation)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("Forge fixture audit count")
    }
}

/// Return whether a fixture-owned spill directory contains no files.
fn directory_tree_is_empty(path: &Path) -> bool {
    std::fs::read_dir(path)
        .map(|entries| {
            entries.flatten().all(|entry| {
                let child = entry.path();
                child.is_dir() && directory_tree_is_empty(&child)
            })
        })
        .unwrap_or(false)
}

/// Create an Iceberg table, staging Parquet files, and aged `vala.file_list`
/// rows in the real Wyrd test server.
pub async fn seed_forge_group(server: &WyrdTestServer, table_name: &str) -> ForgeFixture {
    seed_forge_group_for_tenant(server, server.data_tenant_id(), table_name).await
}

/// Create the same durable Forge fixture for an explicitly selected tenant.
///
/// The server owns the shared catalog, object store, and SQL pools; selecting
/// the tenant here lets one production scheduler tick prove tenant-qualified
/// discovery and object prefixes without introducing a second fixture stack.
pub async fn seed_forge_group_for_tenant(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    table_name: &str,
) -> ForgeFixture {
    seed_forge_group_for_tenant_with_schema(server, tenant, table_name, false).await
}

/// Create a durable Forge fixture with an optional table-specific schema
/// column. The variant is useful for proving that schema fingerprints do not
/// cross physical-table boundaries during one scheduler tick.
pub async fn seed_forge_group_for_tenant_with_schema(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    table_name: &str,
    schema_variant: bool,
) -> ForgeFixture {
    seed_forge_group_for_tenant_with_schema_and_days(
        server,
        tenant,
        table_name,
        schema_variant,
        &[chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("partition day")],
    )
    .await
}

/// Create a durable Forge fixture with explicit partition days and an optional
/// table-specific schema column.
pub async fn seed_forge_group_for_tenant_with_schema_and_days(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    table_name: &str,
    schema_variant: bool,
    partition_days: &[chrono::NaiveDate],
) -> ForgeFixture {
    let resources = ForgeFixtureResources {
        forge: server
            .state()
            .forge()
            .cloned()
            .expect("production server has Forge"),
        vala: server.state().postgres.vala().clone(),
        operator_pool: server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool"),
        bifrost_catalog: Arc::clone(
            server
                .state()
                .bifrost_redux
                .as_ref()
                .expect("Bifrost Redux"),
        ),
        staging: Arc::new(server.state().storage.operator().clone()),
        object_store: ForgeObjectStoreControl::new(Arc::new(
            server.state().storage.operator().clone(),
        )),
        spill_root: Arc::new(tempfile::tempdir().expect("Forge spill root")),
        memory: server
            .state()
            .bifrost_memory
            .clone()
            .expect("Bifrost memory governor"),
        config: ForgeConfig::default(),
    };
    seed_forge_group_with_resources(
        resources,
        tenant,
        table_name,
        schema_variant,
        partition_days,
    )
    .await
}

/// Seed a real Forge fixture from explicit non-server dependencies.
async fn seed_forge_group_with_resources(
    resources: ForgeFixtureResources,
    tenant: DataTenantId,
    table_name: &str,
    schema_variant: bool,
    partition_days: &[chrono::NaiveDate],
) -> ForgeFixture {
    assert!(
        !partition_days.is_empty(),
        "Forge fixture needs one partition day"
    );
    let forge = resources.forge.clone();
    let bifrost_catalog = &resources.bifrost_catalog;
    let staging = Arc::clone(&resources.staging);
    let binding =
        TenantTableBinding::resolve((tenant, TableRef::new(BifrostNamespace::Bifrost, table_name)))
            .expect("Forge fixture table binding");
    let mut fields = vec![Field::new("value", DataType::Int64, false)];
    if schema_variant {
        fields.push(Field::new("schema_variant", DataType::Int64, false));
    }
    bifrost_catalog
        .create_table(CreateTableRequest {
            table: binding.table_ref.clone(),
            user_fields: fields,
            tenant,
            audit: None,
        })
        .await
        .expect("production Forge fixture table");
    let catalog = bifrost_catalog.iceberg_catalog();
    let schema = ArrowSchema::new(with_managed_columns(if schema_variant {
        vec![
            Field::new("value", DataType::Int64, false),
            Field::new("schema_variant", DataType::Int64, false),
        ]
    } else {
        vec![Field::new("value", DataType::Int64, false)]
    }));

    let mut conn = resources
        .vala
        .tenant_conn(tenant)
        .await
        .expect("Forge fixture tenant connection");
    for (day_index, partition_day) in partition_days.iter().enumerate() {
        let base = partition_day
            .and_hms_opt(12, 0, 0)
            .expect("Forge fixture timestamp")
            .and_utc()
            .timestamp_micros();
        for file_number in 0..2_i64 {
            let file_number =
                i64::try_from(day_index).expect("partition day index") * 2 + file_number;
            let mut columns = vec![
                Arc::new(Int64Array::from(vec![file_number, file_number + 10]))
                    as Arc<dyn arrow::array::Array>,
            ];
            if schema_variant {
                columns
                    .push(Arc::new(Int64Array::from(vec![1_i64, 1_i64]))
                        as Arc<dyn arrow::array::Array>);
            }
            let mut batch_ids = FixedSizeBinaryBuilder::with_capacity(2, 16);
            for _ in 0..2 {
                batch_ids
                    .append_value([0_u8; 16])
                    .expect("fixed batch identifier");
            }
            columns.extend([
                Arc::new(StringArray::from(vec![None::<&str>; 2])) as Arc<dyn arrow::array::Array>,
                Arc::new(StringArray::from(vec![None::<&str>; 2])) as Arc<dyn arrow::array::Array>,
                Arc::new(StringArray::from(vec!["principal"; 2])) as Arc<dyn arrow::array::Array>,
                Arc::new(StringArray::from(vec!["request"; 2])) as Arc<dyn arrow::array::Array>,
                Arc::new(
                    TimestampMicrosecondArray::from(vec![
                        base + file_number * 1_000_000,
                        base + file_number * 1_000_000 + 1_000,
                    ])
                    .with_timezone("UTC"),
                ) as Arc<dyn arrow::array::Array>,
                Arc::new(
                    TimestampMicrosecondArray::from(vec![
                        base + file_number * 1_000_000,
                        base + file_number * 1_000_000 + 1_000,
                    ])
                    .with_timezone("UTC"),
                ) as Arc<dyn arrow::array::Array>,
                Arc::new(batch_ids.finish()) as Arc<dyn arrow::array::Array>,
                Arc::new(Int32Array::from(vec![0_i32, 1_i32])) as Arc<dyn arrow::array::Array>,
                Arc::new(StringArray::from(vec![tenant.to_string(); 2]))
                    as Arc<dyn arrow::array::Array>,
            ]);
            let batch = RecordBatch::try_new(Arc::new(schema.clone()), columns)
                .expect("Forge fixture batch");
            let mut bytes = Vec::new();
            let mut writer =
                ArrowWriter::try_new(&mut bytes, batch.schema(), None).expect("Parquet writer");
            writer.write(&batch).expect("Parquet batch");
            writer.close().expect("Parquet close");
            let path = format!("{}/journey-{file_number}.parquet", binding.object_prefix);
            let size = i64::try_from(bytes.len()).expect("Forge fixture file size");
            staging
                .write(&path, Buffer::from(bytes))
                .await
                .expect("Forge fixture object");
            let min_time = chrono::DateTime::from_timestamp_micros(base + file_number * 1_000_000)
                .expect("Forge fixture timestamp");
            sqlx::query(
            "INSERT INTO vala.file_list (id, data_tenant_id, namespace, table_name, file_path, file_size, row_count, min_event_time, max_event_time, partition_day, node_id, writer_epoch, wal_lsn_min, wal_lsn_max) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(tenant.as_uuid())
        .bind(&binding.logical_namespace)
        .bind(&binding.table_name)
        .bind(path)
        .bind(size)
        .bind(2_i64)
        .bind(min_time)
        .bind(min_time + chrono::Duration::milliseconds(1))
        .bind(*partition_day)
        .bind(uuid::Uuid::now_v7())
        .bind(1_i64)
        .bind(file_number * 2 + 1)
        .bind(file_number * 2 + 2)
        .execute(&mut **conn.transaction())
        .await
        .expect("Forge fixture file-list row");
        }
    }
    conn.commit().await.expect("Forge fixture commit");
    sqlx::query(
        "UPDATE vala.file_list SET created_at = now() - interval '3 minutes' WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3",
    )
    .bind(tenant.as_uuid())
    .bind(&binding.logical_namespace)
    .bind(&binding.table_name)
    .execute(resources.operator_pool.pool())
    .await
    .expect("Forge fixture aging");

    ForgeFixture {
        forge,
        vala: resources.vala,
        operator_pool: resources.operator_pool,
        catalog,
        staging: Arc::clone(&staging),
        object_store: resources.object_store,
        spill_root: resources.spill_root,
        memory: resources.memory,
        config: resources.config,
        binding,
        tenant,
    }
}

/// Real Forge worker lifecycle proofs over the one production resource root.
///
/// Every test drives the production `ForgeWorker` against real Postgres,
/// catalog, and object storage. The fixture only injects raw observations and
/// fault seams; it never constructs a governor, a pool, or a runtime.
#[cfg(test)]
mod worker_lifecycle_tests {
    use std::sync::Arc;

    use tokio_util::sync::CancellationToken;
    use vala_bifrost_redux::forge::{ForgeWorker, ForgeWorkerConfig};
    use vala_bifrost_redux::resources::ForgeRewriteRequest;

    use super::{
        CommitUncertaintyCatalog, ForgeFixture, ForgeObjectStoreControl, StandaloneForgeFixture,
    };

    /// Seeds one compaction-ready standalone fixture with a bounded bin.
    async fn lifecycle_fixture(table: &str) -> StandaloneForgeFixture {
        let standalone = StandaloneForgeFixture::start(table)
            .await
            .expect("standalone Forge fixture");
        standalone.fixture().append_forge_file(101).await;
        standalone.fixture().append_forge_file(102).await;
        standalone
    }

    /// Returns a bounded compaction config that plans one small rewrite bin.
    fn lifecycle_config(fixture: &ForgeFixture) -> vala_bifrost_redux::forge::ForgeConfig {
        let mut config = fixture.config.clone();
        config.max_files_per_bin = 2;
        config.max_files_per_tick = 2;
        config
    }

    /// Awaits one fixture signal under a bounded lifecycle deadline.
    async fn bounded<F: std::future::Future>(label: &str, future: F) -> F::Output {
        tokio::time::timeout(std::time::Duration::from_secs(60), future)
            .await
            .unwrap_or_else(|_| panic!("{label} exceeded its bounded lifecycle deadline"))
    }

    /// A running rewrite holds its operation lease and returns it on success.
    #[tokio::test]
    async fn forge_harness_observes_worker_operation_lease() {
        let standalone = lifecycle_fixture("lease_observed").await;
        let fixture = standalone.fixture();
        let catalog = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
        let (forge, _publisher) = fixture.context_with_worker_supervision(
            lifecycle_config(fixture),
            Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
            Arc::clone(&fixture.object_store),
            vala_bifrost_redux::forge::ForgeWorkerCompletionObserver::new(),
            vala_bifrost_redux::forge::ForgeSchedulerTrigger::with_owner_for_test(
                uuid::Uuid::now_v7(),
            ),
        );
        assert!(
            forge.run_once().await.expect("planning pass").groups_seen > 0,
            "the lifecycle fixture must plan durable rewrite work"
        );
        let resources = forge.resources_for_test();
        let baseline = resources
            .snapshot()
            .expect("baseline")
            .elastic_memory_used_bytes;
        let worker = ForgeWorker::new(
            Arc::clone(&forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("production worker");
        catalog.pause_before_commit();
        let stop = CancellationToken::new();
        let attempt = tokio::spawn({
            let stop = stop.clone();
            async move { worker.execute_one_for_test(&stop).await }
        });
        bounded("paused commit", catalog.wait_for_before_commit()).await;
        let held = resources
            .snapshot()
            .expect("held")
            .elastic_memory_used_bytes;
        assert!(
            held > baseline,
            "a running rewrite must hold its exact operation lease"
        );
        catalog.reject_paused_before_commit();
        let _ = bounded("attempt completion", attempt)
            .await
            .expect("attempt joins");
        let released = resources.snapshot().expect("released");
        assert_eq!(released.elastic_memory_used_bytes, baseline);
        assert_eq!(released.scratch_used_bytes, 0);
    }

    /// Capacity refusal performs no data IO and retains the retryable claim.
    #[tokio::test]
    async fn forge_harness_refusal_releases_claim_without_data_io() {
        let standalone = lifecycle_fixture("refusal_no_io").await;
        let fixture = standalone.fixture();
        let object_store = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
        let (forge, _publisher) = fixture.context_with_worker_supervision(
            lifecycle_config(fixture),
            Arc::clone(&fixture.catalog),
            Arc::clone(&object_store) as Arc<dyn vala_bifrost_redux::forge::ForgeObjectStore>,
            vala_bifrost_redux::forge::ForgeWorkerCompletionObserver::new(),
            vala_bifrost_redux::forge::ForgeSchedulerTrigger::with_owner_for_test(
                uuid::Uuid::now_v7(),
            ),
        );
        assert!(
            forge.run_once().await.expect("planning pass").groups_seen > 0,
            "the lifecycle fixture must plan durable rewrite work"
        );
        let resources = forge.resources_for_test();
        let plan = resources.snapshot().expect("plan snapshot").plan;
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
            .try_acquire_rewrite(ForgeRewriteRequest {
                envelope: blocker_envelope,
                memory_bytes: plan.elastic_memory_bytes,
                scratch_bytes: plan.scratch_limit_bytes,
                reader_permits: 1,
            })
            .expect("the test owner occupies all Forge capacity");
        let worker = ForgeWorker::new(
            Arc::clone(&forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("production worker");

        let error = worker
            .execute_one_for_test(&CancellationToken::new())
            .await
            .expect_err("a refused attempt must surface its typed capacity error");
        assert!(
            matches!(
                error,
                vala_bifrost_redux::forge::ForgeError::Capacity { .. }
            ),
            "capacity refusal must keep its original typed error: {error}"
        );
        assert_eq!(
            object_store.output_put_calls(),
            0,
            "a refused attempt must not write any rewrite output"
        );
        let state: Option<String> = sqlx::query_scalar(
            "SELECT state FROM vala.forge_tasks WHERE data_tenant_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_optional(fixture.operator_pool.pool())
        .await
        .expect("durable task state");
        assert_eq!(
            state.as_deref(),
            Some("claimed"),
            "a refused claim keeps its existing bounded-reclaim recovery state"
        );
        let refused = resources.snapshot().expect("post-refusal snapshot");
        assert_eq!(refused.elastic_memory_used_bytes, plan.elastic_memory_bytes);
        assert_eq!(refused.scratch_used_bytes, plan.scratch_limit_bytes);
        drop(blocker);
        let released = resources.snapshot().expect("released snapshot");
        assert_eq!(released.elastic_memory_used_bytes, 0);
        assert_eq!(released.scratch_used_bytes, 0);
    }

    /// A failed rewrite returns its original error and restores both baselines.
    #[tokio::test]
    async fn forge_worker_error_restores_resource_baselines() {
        let standalone = lifecycle_fixture("error_baselines").await;
        let fixture = standalone.fixture();
        let catalog = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
        let (forge, _publisher) = fixture.context_with_worker_supervision(
            lifecycle_config(fixture),
            Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
            Arc::clone(&fixture.object_store),
            vala_bifrost_redux::forge::ForgeWorkerCompletionObserver::new(),
            vala_bifrost_redux::forge::ForgeSchedulerTrigger::with_owner_for_test(
                uuid::Uuid::now_v7(),
            ),
        );
        assert!(
            forge.run_once().await.expect("planning pass").groups_seen > 0,
            "the lifecycle fixture must plan durable rewrite work"
        );
        let resources = forge.resources_for_test();
        let baseline = resources.snapshot().expect("baseline");
        catalog.fail_after_next_commit();
        let worker = ForgeWorker::new(
            Arc::clone(&forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("production worker");
        let outcome = worker.execute_one_for_test(&CancellationToken::new()).await;
        assert!(
            outcome.map_or(true, |executed| executed),
            "the faulted attempt must have claimed and executed durable work"
        );
        let after = resources.snapshot().expect("post-error snapshot");
        assert_eq!(
            after.elastic_memory_used_bytes,
            baseline.elastic_memory_used_bytes
        );
        assert_eq!(after.scratch_used_bytes, baseline.scratch_used_bytes);
    }

    /// Cancellation drops the attempt runtime before the lease returns.
    #[tokio::test]
    async fn forge_worker_cancellation_drops_runtime_before_lease_release() {
        let standalone = lifecycle_fixture("cancel_before_release").await;
        let fixture = standalone.fixture();
        let catalog = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
        let (forge, _publisher) = fixture.context_with_worker_supervision(
            lifecycle_config(fixture),
            Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
            Arc::clone(&fixture.object_store),
            vala_bifrost_redux::forge::ForgeWorkerCompletionObserver::new(),
            vala_bifrost_redux::forge::ForgeSchedulerTrigger::with_owner_for_test(
                uuid::Uuid::now_v7(),
            ),
        );
        assert!(
            forge.run_once().await.expect("planning pass").groups_seen > 0,
            "the lifecycle fixture must plan durable rewrite work"
        );
        let resources = forge.resources_for_test();
        let baseline = resources.snapshot().expect("baseline");
        let worker = ForgeWorker::new(
            Arc::clone(&forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("production worker");
        catalog.pause_before_commit();
        let stop = CancellationToken::new();
        let attempt = tokio::spawn({
            let stop = stop.clone();
            async move { worker.execute_one_for_test(&stop).await }
        });
        bounded("paused commit", catalog.wait_for_before_commit()).await;
        stop.cancel();
        catalog.reject_paused_before_commit();
        let _ = bounded("cancelled attempt", attempt)
            .await
            .expect("attempt joins");
        let after = resources.snapshot().expect("post-cancellation snapshot");
        assert_eq!(
            after.elastic_memory_used_bytes,
            baseline.elastic_memory_used_bytes
        );
        assert_eq!(after.scratch_used_bytes, baseline.scratch_used_bytes);
    }

    /// An abruptly dropped attempt still returns its exact lease to the root.
    #[tokio::test]
    async fn forge_worker_drop_restores_resource_baselines() {
        let standalone = lifecycle_fixture("drop_baselines").await;
        let fixture = standalone.fixture();
        let catalog = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
        let (forge, _publisher) = fixture.context_with_worker_supervision(
            lifecycle_config(fixture),
            Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
            Arc::clone(&fixture.object_store),
            vala_bifrost_redux::forge::ForgeWorkerCompletionObserver::new(),
            vala_bifrost_redux::forge::ForgeSchedulerTrigger::with_owner_for_test(
                uuid::Uuid::now_v7(),
            ),
        );
        assert!(
            forge.run_once().await.expect("planning pass").groups_seen > 0,
            "the lifecycle fixture must plan durable rewrite work"
        );
        let resources = forge.resources_for_test();
        let baseline = resources.snapshot().expect("baseline");
        let worker = ForgeWorker::new(
            Arc::clone(&forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("production worker");
        catalog.pause_before_commit();
        let stop = CancellationToken::new();
        let attempt = tokio::spawn({
            let stop = stop.clone();
            async move { worker.execute_one_for_test(&stop).await }
        });
        bounded("paused commit", catalog.wait_for_before_commit()).await;
        attempt.abort();
        let _ = attempt.await;
        catalog.reject_paused_before_commit();
        for _ in 0..200 {
            if resources
                .snapshot()
                .expect("snapshot")
                .elastic_memory_used_bytes
                == baseline.elastic_memory_used_bytes
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        let after = resources.snapshot().expect("post-drop snapshot");
        assert_eq!(
            after.elastic_memory_used_bytes,
            baseline.elastic_memory_used_bytes
        );
        assert_eq!(after.scratch_used_bytes, baseline.scratch_used_bytes);
    }
}
