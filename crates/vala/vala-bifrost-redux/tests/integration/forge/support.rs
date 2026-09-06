//! Tier-2 promotion fixture: a real Scribe and a real Forge over one database.
//!
//! Contains no tests. Everything here composes production owners — the real
//! `BifrostCatalog`, the real `ScribeImpl`, the real `Forge` scheduler, and the
//! real `ForgeWorker` — over a repository-managed Postgres and a local
//! warehouse. No durable state is fabricated: every `vala.file_list` row this
//! fixture presents to Forge was written by Scribe from a batch it encoded.
//!
//! Two thin adapters exist because their production counterparts are
//! server-private: a counting [`ForgeObjectStore`] over the same operator, and
//! a delegating [`iceberg::Catalog`] that can refuse or park one commit. Both
//! are plumbing around the real dependency, not a second implementation of it.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arrow::array::{Int64Array, RecordBatch, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use async_trait::async_trait;
use iceberg::table::Table;
use iceberg::{
    Catalog, Error as IcebergError, ErrorKind as IcebergErrorKind, Namespace, NamespaceIdent,
    TableCommit, TableCreation, TableIdent,
};
use opendal::{Buffer, Entry, Metadata, Operator};
use secrecy::ExposeSecret as _;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_bifrost_redux::catalog::{
    BifrostCatalog, CreateTableRequest, TableRef, TenantTableBinding,
};
use vala_bifrost_redux::forge::{
    Forge, ForgeBuildConfig, ForgeClock, ForgeClockControl, ForgeConfig, ForgeError,
    ForgeObjectStore, ForgeRoleReadiness, ForgeScheduler, ForgeSchedulerTrigger, ForgeTelemetry,
    ForgeWorker, ForgeWorkerCompletionObserver, ForgeWorkerConfig,
};
use vala_bifrost_redux::maintenance::staging_file_channel;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::resources::{
    BifrostResourcePolicy, BifrostRole, BifrostRuntimeResources, BifrostVolumeRoots,
    ResourceSource, SystemResourceSnapshot,
};
use vala_bifrost_redux::scribe::admission::AdmissionConfig;
use vala_bifrost_redux::scribe::wal::{WalConfig, WalWriter};
use vala_bifrost_redux::scribe::{
    NativeIngressTestFrame, ScribeBuildConfig, ScribeExecutionPools, ScribeImpl,
    ScribeIngressCpuPool, ScribePersistenceConfig, ScribePersistenceCpuPool, ScribePressureConfig,
    ScribeWalIoPool,
};
use wyrd_spec::DataTenantId;

/// Bounded wait every fixture handshake uses instead of a sleep.
const FIXTURE_BOUND: Duration = Duration::from_secs(30);

/// Object-store seam that delegates every call and counts what promotion did.
///
/// The promotion route is defined by what it does *not* do to the object
/// store: it reads footers and never writes or deletes a data object. Counting
/// the delegated calls is therefore the direct proof, and it stays honest
/// because every call still reaches the real operator.
#[derive(Debug)]
pub(crate) struct CountingObjectStore {
    /// Real operator every delegated call runs against.
    inner: Arc<Operator>,
    /// Number of rewrite output writers opened, which promotion must leave at zero.
    output_writers: AtomicUsize,
    /// Number of delegated deletes, which promotion must leave at zero.
    deletes: AtomicUsize,
    /// Number of delegated object reads, shared so a seam can snapshot it.
    reads: Arc<AtomicUsize>,
    /// Number of delegated object stats, which prove a pre-IO refusal.
    stats: AtomicUsize,
    /// Optional pause applied to one delegated stat.
    stat_pause: CallPause,
    /// Remaining delegated stats to fail with an injected transient error.
    stat_errors: AtomicUsize,
    /// Optional pause applied to one delegated delete, before it is submitted.
    delete_pause: CallPause,
    /// Remaining delegated deletes to submit and then report as unknown.
    delete_errors: AtomicUsize,
    /// Remaining delegated deletes to submit and then report as already absent.
    delete_absences: AtomicUsize,
    /// Entries per orphan-listing page; `0` keeps the single-page default.
    list_page_entries: AtomicUsize,
    /// Every `start_after` cursor orphan listing has been given, in order.
    list_cursors: Mutex<Vec<Option<String>>>,
}

/// Deterministic pause seam over exactly one delegated object-store call.
///
/// A cleanup candidate's fresh reachability proof begins with a stat, so
/// suspending that one call is the only place a test can observe the durable
/// state a preparation committed while no Postgres transaction is open and no
/// deletion has been submitted. Suspending the delete instead holds the caller
/// inside the polled deletion future, which is the only place the boundary
/// between a pre- and a post-submission outcome can be driven. Both handshakes
/// use `Notify::notify_one`, whose permit is stored, so neither side can miss
/// the other and no sleep is needed. A caller that cancels rather than releases
/// simply drops the suspended future, leaving the real operator untouched.
#[derive(Debug, Default)]
struct CallPause {
    /// One-based call ordinal to suspend at; `0` disables the gate.
    at: AtomicUsize,
    /// Signalled once the selected call is suspended.
    arrived: tokio::sync::Notify,
    /// Signalled by the test to let the suspended call proceed.
    release: tokio::sync::Notify,
}

impl CountingObjectStore {
    /// Wrap the fixture operator in a counting seam.
    pub(crate) fn new(inner: Arc<Operator>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            output_writers: AtomicUsize::new(0),
            deletes: AtomicUsize::new(0),
            reads: Arc::new(AtomicUsize::new(0)),
            stats: AtomicUsize::new(0),
            stat_pause: CallPause::default(),
            stat_errors: AtomicUsize::new(0),
            delete_pause: CallPause::default(),
            delete_errors: AtomicUsize::new(0),
            delete_absences: AtomicUsize::new(0),
            list_page_entries: AtomicUsize::new(0),
            list_cursors: Mutex::new(Vec::new()),
        })
    }

    /// Splits orphan listing into pages of exactly `entries` keys.
    ///
    /// The production adapter owns page granularity, so a scan-bound proof
    /// needs a seam that can make a page small enough for the per-run page cap
    /// to bite deterministically.
    pub(crate) fn page_listing_by(&self, entries: usize) {
        self.list_page_entries.store(entries, Ordering::Release);
    }

    /// Returns every cursor orphan listing has been resumed from, in order.
    pub(crate) fn list_cursors(&self) -> Vec<Option<String>> {
        self.list_cursors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Return how many objects were stat'ed.
    pub(crate) fn stats(&self) -> usize {
        self.stats.load(Ordering::Acquire)
    }

    /// Suspend the stat whose one-based ordinal is `at`, counting from now.
    ///
    /// The caller then awaits [`Self::stat_paused`] and finally calls
    /// [`Self::release_stat`], which is the whole handshake: no other stat is
    /// affected and nothing is timed.
    pub(crate) fn pause_stat_at(&self, at: usize) {
        self.stat_pause
            .at
            .store(self.stats() + at, Ordering::Release);
    }

    /// Wait until the armed stat is suspended inside the delegated call.
    pub(crate) async fn stat_paused(&self) {
        self.stat_pause.arrived.notified().await;
    }

    /// Let the suspended stat proceed to the real operator.
    pub(crate) fn release_stat(&self) {
        self.stat_pause.release.notify_one();
    }

    /// Fail the next `count` delegated stats with an injected transient error.
    ///
    /// The error is an `Unexpected` opendal failure rather than `NotFound`, so
    /// it exercises the branch where the object's existence stays unknown.
    pub(crate) fn fail_next_stats(&self, count: usize) {
        self.stat_errors.store(count, Ordering::Release);
    }

    /// Let the next `count` delegated deletes take effect, then report failure.
    ///
    /// This is the acceptance-unknown shape a real object store produces when
    /// the deletion happened but its acknowledgement was lost, so the caller
    /// must treat the candidate as uncertain rather than deleted.
    pub(crate) fn fail_next_deletes(&self, count: usize) {
        self.delete_errors.store(count, Ordering::Release);
    }

    /// Let the next `count` delegated deletes take effect, then report absence.
    ///
    /// This is what a store reports when the object is already gone by the time
    /// the deletion is applied, which the caller must treat as a proven absence
    /// rather than as an unknown acceptance.
    pub(crate) fn not_found_next_deletes(&self, count: usize) {
        self.delete_absences.store(count, Ordering::Release);
    }

    /// Suspend the delete whose one-based ordinal is `at`, counting from now.
    ///
    /// The suspension happens before the real operator is called, so a caller
    /// that cancels instead of releasing leaves the object intact while its
    /// deletion counts as submitted.
    pub(crate) fn pause_delete_at(&self, at: usize) {
        self.delete_pause
            .at
            .store(self.deletes() + at, Ordering::Release);
    }

    /// Wait until the armed delete is suspended inside the delegated call.
    pub(crate) async fn delete_paused(&self) {
        self.delete_pause.arrived.notified().await;
    }

    /// Let the suspended delete proceed into the real operator.
    pub(crate) fn release_delete(&self) {
        self.delete_pause.release.notify_one();
    }

    /// Borrow the shared read counter so a catalog seam can snapshot it.
    pub(crate) fn read_counter(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.reads)
    }

    /// Return how many rewrite outputs were opened.
    pub(crate) fn output_writers(&self) -> usize {
        self.output_writers.load(Ordering::Acquire)
    }

    /// Return how many objects were deleted.
    pub(crate) fn deletes(&self) -> usize {
        self.deletes.load(Ordering::Acquire)
    }

    /// Return how many objects or ranges were read.
    pub(crate) fn reads(&self) -> usize {
        self.reads.load(Ordering::Acquire)
    }
}

#[async_trait]
impl ForgeObjectStore for CountingObjectStore {
    async fn output_writer(
        &self,
        operator: &Operator,
        path: &str,
        chunk_bytes: usize,
    ) -> opendal::Result<opendal::Writer> {
        self.output_writers.fetch_add(1, Ordering::AcqRel);
        operator.writer_with(path).chunk(chunk_bytes).await
    }

    async fn read(&self, path: &str) -> opendal::Result<Buffer> {
        self.reads.fetch_add(1, Ordering::AcqRel);
        self.inner.read(path).await
    }

    async fn read_range(&self, path: &str, range: std::ops::Range<u64>) -> opendal::Result<Buffer> {
        self.reads.fetch_add(1, Ordering::AcqRel);
        self.inner.read_with(path).range(range).await
    }

    async fn list(&self, prefix: &str) -> opendal::Result<Vec<Entry>> {
        self.inner.list_with(prefix).recursive(true).await
    }

    /// Page the real listing while preserving exclusive, lexicographic cursors.
    ///
    /// The production adapter delegates the cursor to the backend; the
    /// filesystem operator behind this fixture cannot, so the same semantics
    /// are applied here over a real recursive listing. Recording each cursor is
    /// what lets a resume proof assert that earlier pages are never relisted.
    async fn list_pages(
        &self,
        prefix: &str,
        start_after: Option<&str>,
    ) -> opendal::Result<vala_bifrost_redux::forge::ForgeObjectPages> {
        self.list_cursors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(start_after.map(ToOwned::to_owned));
        let mut entries = self.list(prefix).await?;
        entries.retain(|entry| entry.metadata().is_file());
        if let Some(cursor) = start_after {
            entries.retain(|entry| entry.path() > cursor);
        }
        entries.sort_unstable_by(|left, right| left.path().cmp(right.path()));
        let per_page = self.list_page_entries.load(Ordering::Acquire);
        let pages: Vec<opendal::Result<Vec<Entry>>> = if per_page == 0 {
            vec![Ok(entries)]
        } else {
            entries
                .chunks(per_page)
                .map(|chunk| Ok(chunk.to_vec()))
                .collect()
        };
        Ok(Box::pin(futures_util::stream::iter(pages)))
    }

    async fn stat(&self, path: &str) -> opendal::Result<Metadata> {
        let ordinal = self.stats.fetch_add(1, Ordering::AcqRel) + 1;
        if self.stat_pause.at.load(Ordering::Acquire) == ordinal {
            self.stat_pause.at.store(0, Ordering::Release);
            self.stat_pause.arrived.notify_one();
            self.stat_pause.release.notified().await;
        }
        if self
            .stat_errors
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            return Err(opendal::Error::new(
                opendal::ErrorKind::Unexpected,
                "injected expired-cleanup stat failure",
            ));
        }
        self.inner.stat(path).await
    }

    async fn delete(&self, path: &str) -> opendal::Result<()> {
        let ordinal = self.deletes.fetch_add(1, Ordering::AcqRel) + 1;
        if self.delete_pause.at.load(Ordering::Acquire) == ordinal {
            self.delete_pause.at.store(0, Ordering::Release);
            self.delete_pause.arrived.notify_one();
            self.delete_pause.release.notified().await;
        }
        let submitted = self.inner.delete(path).await;
        if self
            .delete_errors
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            submitted?;
            return Err(opendal::Error::new(
                opendal::ErrorKind::Unexpected,
                "injected expired-cleanup delete acknowledgement failure",
            ));
        }
        if self
            .delete_absences
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            submitted?;
            return Err(opendal::Error::new(
                opendal::ErrorKind::NotFound,
                "injected expired-cleanup delete absence",
            ));
        }
        submitted
    }
}

/// Acknowledges that cancellation dropped the parked catalog commit.
///
/// The guard is armed only while `update_table` awaits the fixture release. A
/// deliberate release disarms it; cancellation drops it and wakes the test that
/// has to observe the drain branch rather than guess at it.
struct ParkedCommitDropAck<'a> {
    /// Observable acknowledgement state, set before any waiter registers.
    dropped: &'a AtomicBool,
    /// Wakeup for a waiter already suspended when cancellation lands.
    ready: &'a tokio::sync::Notify,
    /// Whether dropping this guard must publish the acknowledgement.
    armed: bool,
}

impl<'a> ParkedCommitDropAck<'a> {
    /// Arm acknowledgement for one parked catalog commit.
    fn new(dropped: &'a AtomicBool, ready: &'a tokio::sync::Notify) -> Self {
        Self {
            dropped,
            ready,
            armed: true,
        }
    }

    /// Disarm after the fixture released the commit on purpose.
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ParkedCommitDropAck<'_> {
    /// Publish acknowledgement only when the parked commit was cancelled.
    fn drop(&mut self) {
        if self.armed {
            self.dropped.store(true, Ordering::Release);
            self.ready.notify_waiters();
        }
    }
}

/// Catalog seam that delegates everything and can refuse or park one commit.
///
/// Only `update_table` is instrumented, and only before delegation, which is
/// exactly where certain non-acceptance lives: a refusal issued here is
/// knowledge that nothing landed, so the branches under test are the real ones
/// rather than a fabricated post-commit state.
#[derive(Debug)]
pub(crate) struct PromotionCatalogSeam {
    /// Real catalog every call is delegated to.
    inner: Arc<dyn Catalog>,
    /// Count of delegated and refused commit attempts.
    attempts: AtomicUsize,
    /// Count of delegated table loads, used to prove pre-IO refusal.
    loads: AtomicUsize,
    /// Remaining commits to refuse outright as definite conflicts.
    reject_budget: AtomicUsize,
    /// Object-store read count observed when the last conflict was issued.
    reads_at_conflict: AtomicUsize,
    /// Shared object-store read counter this seam samples on refusal.
    reads: Arc<AtomicUsize>,
    /// One-shot arm for parking the next commit before delegation.
    park_next: AtomicBool,
    /// Records that a parked commit reached the seam.
    parked: AtomicBool,
    /// Whether the parked commit is released as a definite conflict.
    reject_parked: AtomicBool,
    /// Wakes tests waiting for the parked commit.
    parked_ready: tokio::sync::Notify,
    /// Releases the parked commit.
    parked_release: tokio::sync::Notify,
    /// Records that cancellation dropped the parked commit.
    parked_dropped: AtomicBool,
    /// Wakes tests waiting for that cancellation.
    parked_drop_ready: tokio::sync::Notify,
    /// Whether every accepted commit's response is discarded before returning.
    lose_response: AtomicBool,
    /// Storage adapter every loaded table is rebound to, once one is installed.
    ///
    /// The managed core reads inputs and writes rewrite outputs through the
    /// [`FileIO`] carried by the table it was handed, so replacing it here is
    /// the one place a scenario can observe or refuse an individual output
    /// without reimplementing the production write path.
    file_io: std::sync::OnceLock<iceberg::io::FileIO>,
}

impl PromotionCatalogSeam {
    /// Wrap the fixture catalog, sampling `reads` whenever a conflict is issued.
    pub(crate) fn new(inner: Arc<dyn Catalog>, reads: Arc<AtomicUsize>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            attempts: AtomicUsize::new(0),
            loads: AtomicUsize::new(0),
            reject_budget: AtomicUsize::new(0),
            reads_at_conflict: AtomicUsize::new(0),
            reads,
            park_next: AtomicBool::new(false),
            parked: AtomicBool::new(false),
            reject_parked: AtomicBool::new(false),
            parked_ready: tokio::sync::Notify::new(),
            parked_release: tokio::sync::Notify::new(),
            parked_dropped: AtomicBool::new(false),
            parked_drop_ready: tokio::sync::Notify::new(),
            lose_response: AtomicBool::new(false),
            file_io: std::sync::OnceLock::new(),
        })
    }

    /// Rebinds every table this seam loads onto `file_io`.
    ///
    /// Installed once per seam: a second install would leave earlier and later
    /// loads of the same table reading through different adapters, which is a
    /// scenario nothing needs and every assertion would have to reason about.
    ///
    /// # Panics
    ///
    /// Panics when an adapter is already installed.
    pub(crate) fn intercept_file_io(&self, file_io: iceberg::io::FileIO) {
        assert!(
            self.file_io.set(file_io).is_ok(),
            "one seam installs at most one storage adapter"
        );
    }

    /// Refuse the next `count` commits as definite, non-retryable conflicts.
    pub(crate) fn reject_next_commits(&self, count: usize) {
        self.attempts.store(0, Ordering::Release);
        self.reject_budget.store(count, Ordering::Release);
    }

    /// Return how many commit attempts crossed this seam.
    pub(crate) fn attempts(&self) -> usize {
        self.attempts.load(Ordering::Acquire)
    }

    /// Return how many table loads crossed this seam.
    pub(crate) fn loads(&self) -> usize {
        self.loads.load(Ordering::Acquire)
    }

    /// Return the object-store read count sampled at the last refusal.
    pub(crate) fn reads_at_conflict(&self) -> usize {
        self.reads_at_conflict.load(Ordering::Acquire)
    }

    /// Park the next commit before it is delegated.
    pub(crate) fn park_next_commit(&self) {
        self.parked.store(false, Ordering::Release);
        self.reject_parked.store(false, Ordering::Release);
        self.parked_dropped.store(false, Ordering::Release);
        self.park_next.store(true, Ordering::Release);
    }

    /// Wait until a commit is parked at this seam.
    pub(crate) async fn wait_for_parked_commit(&self) {
        while !self.parked.load(Ordering::Acquire) {
            self.parked_ready.notified().await;
        }
    }

    /// Wait until cancellation dropped the parked commit.
    pub(crate) async fn wait_for_parked_commit_drop(&self) {
        while !self.parked_dropped.load(Ordering::Acquire) {
            self.parked_drop_ready.notified().await;
        }
    }

    /// Release the parked commit as a definite conflict and park the next one.
    ///
    /// Arming the follow-up park before the release is what makes the retry
    /// observable: the conflicted call returns, production re-derives and
    /// submits once more, and that second call stops here instead of racing the
    /// test to the real catalog. The parked second call is never released, so
    /// only the production budget can end it.
    pub(crate) fn reject_parked_commit_and_park_next(&self) {
        self.parked.store(false, Ordering::Release);
        self.parked_dropped.store(false, Ordering::Release);
        self.park_next.store(true, Ordering::Release);
        self.reject_parked.store(true, Ordering::Release);
        self.parked_release.notify_waiters();
    }

    /// Arm or disarm losing every accepted commit's response.
    ///
    /// The fault fires strictly after `inner.update_table` returned success, so
    /// the mutation is durably applied and only the caller's knowledge of it is
    /// lost. That is the one shape a pre-delegation refusal cannot produce. It
    /// stays armed rather than firing once because the pinned Iceberg
    /// transaction retries a retryable error itself; only a fault that outlives
    /// that budget delivers the uncertainty to the Forge expiry owner.
    pub(crate) fn lose_commit_responses(&self, armed: bool) {
        self.lose_response.store(armed, Ordering::Release);
    }

    /// Release the parked commit as a definite conflict.
    pub(crate) fn reject_parked_commit(&self) {
        self.reject_parked.store(true, Ordering::Release);
        self.parked_release.notify_waiters();
    }
}

#[async_trait]
impl Catalog for PromotionCatalogSeam {
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
        self.loads.fetch_add(1, Ordering::AcqRel);
        let loaded = self.inner.load_table(table).await?;
        let Some(file_io) = self.file_io.get() else {
            return Ok(loaded);
        };
        let mut builder = Table::builder()
            .identifier(loaded.identifier().clone())
            .metadata(loaded.metadata_ref())
            .file_io(file_io.clone())
            .runtime(iceberg::Runtime::current());
        if let Some(location) = loaded.metadata_location() {
            builder = builder.metadata_location(location.to_owned());
        }
        builder.build()
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
        self.attempts.fetch_add(1, Ordering::AcqRel);
        if self
            .reject_budget
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |budget| {
                budget.checked_sub(1)
            })
            .is_ok()
        {
            self.reads_at_conflict
                .store(self.reads.load(Ordering::Acquire), Ordering::Release);
            return Err(IcebergError::new(
                IcebergErrorKind::Unexpected,
                "injected definite Forge commit conflict",
            )
            .with_retryable(false));
        }
        if self.park_next.swap(false, Ordering::AcqRel) {
            let mut ack = ParkedCommitDropAck::new(&self.parked_dropped, &self.parked_drop_ready);
            self.parked.store(true, Ordering::Release);
            self.parked_ready.notify_waiters();
            self.parked_release.notified().await;
            ack.disarm();
            if self.reject_parked.load(Ordering::Acquire) {
                self.reads_at_conflict
                    .store(self.reads.load(Ordering::Acquire), Ordering::Release);
                return Err(IcebergError::new(
                    IcebergErrorKind::Unexpected,
                    "injected definite Forge commit conflict at the parked boundary",
                )
                .with_retryable(false));
            }
        }
        let committed = self.inner.update_table(commit).await?;
        if self.lose_response.load(Ordering::Acquire) {
            drop(committed);
            return Err(IcebergError::new(
                IcebergErrorKind::Unexpected,
                "injected lost Forge commit response after catalog acceptance",
            )
            .with_retryable(true));
        }
        Ok(committed)
    }
}

/// One `vala.file_list` row projected to its promotion settlement columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileRow {
    /// Durable row identity, which is also the promoted-file identity.
    pub(crate) id: uuid::Uuid,
    /// Canonical object path Scribe published.
    pub(crate) file_path: String,
    /// Whether the row has left the hot source.
    pub(crate) compacted: bool,
    /// Snapshot that represents the row, once settled.
    pub(crate) committed_snapshot_id: Option<i64>,
}

/// A real Scribe and a real Forge composed over one repository-managed database.
///
/// The fixture owns every durable dependency directly rather than borrowing a
/// booted server: `vala-bifrost-redux` is a dependency of `wyrd-server`, so a
/// tier-2 test cannot start one. What it composes is nonetheless the production
/// graph — the same catalog owner, the same Scribe, and the same Forge
/// scheduler and worker a server would build.
/// One durable `vala.forge_tasks` row as the scheduler persisted it.
#[derive(Debug)]
pub(crate) struct ForgeTaskRow {
    /// Durable identity of the task.
    pub(crate) task_id: uuid::Uuid,
    /// Persisted strategy discriminator.
    pub(crate) strategy: String,
    /// Persisted scheduling state.
    pub(crate) state: String,
    /// Snapshot the task was bound to at enqueue.
    pub(crate) base_snapshot_id: i64,
    /// Immutable plan payload bound at enqueue.
    pub(crate) plan: serde_json::Value,
    /// Canonical hash of that plan payload.
    pub(crate) plan_hash: Vec<u8>,
}

pub(crate) struct PromotionIntegrationFixture {
    /// Real catalog owner used for registration, cuts, and Forge commits.
    pub(crate) catalog: Arc<BifrostCatalog>,
    /// Privileged pool used for read-only durable inspection.
    pub(crate) operator_pool: vala_sql::OperatorPool,
    /// Tenant-scoped SQL handle Forge transitions run through.
    pub(crate) vala: vala_sql::ValaPostgres,
    /// Raw staging operator shared by Scribe, the catalog, and Forge.
    pub(crate) staging: Arc<Operator>,
    /// Tenant that owns the table and every sealed row.
    pub(crate) tenant: DataTenantId,
    /// Physical and logical identity of the seeded table.
    pub(crate) binding: TenantTableBinding,
    /// Validated Forge limits every supervised pair is built with.
    pub(crate) config: ForgeConfig,
    /// Narrow Forge capability issued by this fixture's resource composition.
    forge_resources: vala_bifrost_redux::resources::ForgeResources,
    /// Real Scribe retained so its owned WAL and workers outlive the seals,
    /// and reused by [`PromotionIntegrationFixture::seal_more`] to publish
    /// further hot objects through the same writer.
    scribe: Arc<ScribeImpl>,
    /// Database retained for the fixture lifetime and reused by scenarios that
    /// compose a second production owner — an Oracle reader authority, say —
    /// over the exact same durable state.
    pub(crate) database: wyrd_dev_fixtures::pg::PgFixture,
    /// Warehouse root retained for the fixture lifetime.
    _warehouse: tempfile::TempDir,
    /// WAL root retained for the fixture lifetime.
    _wal_root: tempfile::TempDir,
    /// Scratch root retained for the fixture lifetime and used as the base for
    /// each supervised Forge owner's attempt-scoped rewrite spill directory.
    scratch_root: tempfile::TempDir,
}

impl PromotionIntegrationFixture {
    /// Plans the fixture's real promotion and returns its production worker with
    /// the requested existing observer gates, without starting that worker yet.
    ///
    /// # Panics
    /// Panics when scheduler construction, planning, or worker construction fails.
    pub(crate) async fn plan_worker_for_test(
        &self,
        observer: ForgeWorkerCompletionObserver,
        stop: &CancellationToken,
    ) -> ForgeWorker {
        let store = CountingObjectStore::new(Arc::clone(&self.staging));
        let forge = self.build_forge_for_test(
            self.catalog.iceberg_catalog(),
            store,
            ForgeClock::system(),
            observer,
            ForgeSchedulerTrigger::default(),
        );
        ForgeScheduler::new(&forge)
            .expect("scheduler")
            .schedule_once(stop)
            .await
            .expect("plan");
        ForgeWorker::new(forge, ForgeWorkerConfig::default(), Uuid::now_v7()).expect("worker")
    }

    /// Commits real Prepared evidence, refuses its terminal settlement, and expires
    /// that owner so a production recovery claim can reconcile the same evidence.
    ///
    /// # Panics
    /// Panics when fault installation, durable preparation, or restoration fails.
    pub(crate) async fn prepare_recovery_episode(
        &self,
        forge: &Arc<Forge>,
        stop: &CancellationToken,
    ) {
        let admin = self
            .database
            .superuser_pool()
            .await
            .expect("fixture administrator");
        sqlx::query("CREATE FUNCTION vala.fail_episode_terminal() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.state='succeeded' THEN RAISE EXCEPTION 'held terminal settlement'; END IF; RETURN NEW; END $$")
        .execute(&admin).await.expect("terminal fault");
        sqlx::query("CREATE TRIGGER fail_episode_terminal BEFORE UPDATE ON vala.forge_tasks FOR EACH ROW EXECUTE FUNCTION vala.fail_episode_terminal()")
        .execute(&admin).await.expect("terminal fault boundary");
        let first = ForgeWorker::new(
            Arc::clone(forge),
            ForgeWorkerConfig::default(),
            Uuid::now_v7(),
        )
        .expect("worker");
        assert!(
            first.execute_one_for_test(stop).await.is_err(),
            "terminal SQL refusal must surface"
        );
        let state: String = sqlx::query_scalar("SELECT state FROM vala.forge_tasks")
            .fetch_one(self.operator_pool.pool())
            .await
            .expect("prepared state");
        assert_eq!(
            state, "prepared",
            "real evidence must have committed before terminal refusal"
        );
        sqlx::query("DROP TRIGGER fail_episode_terminal ON vala.forge_tasks")
            .execute(&admin)
            .await
            .expect("restore terminal settlement");
        sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=now()-interval '1 hour'")
            .execute(&admin)
            .await
            .expect("expired owner");
    }

    /// Start Postgres, compose the production graph, and seal two hot objects.
    ///
    /// # Panics
    ///
    /// Panics when any fixture dependency cannot start, when the table cannot
    /// be registered, or when a real Scribe seal publishes fewer than the two
    /// `vala.file_list` rows a promotion group needs.
    pub(crate) async fn start(table_name: &str) -> Self {
        let database = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("Postgres fixture");
        let tenant = database.data_tenant_id();
        let warehouse = tempfile::tempdir().expect("warehouse directory");
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let scratch_root = tempfile::tempdir().expect("scratch directory");

        let storage = local_storage_owner(warehouse.path());
        // The filesystem service resumes a listing from `start_after` but does
        // not advertise it, and a Forge worker refuses a staging backend that
        // cannot resume a bounded orphan scan. The fixture stands in for a
        // production object store, so it declares the support it actually has.
        let staging = Arc::new(storage.operator().clone().layer(
            opendal::layers::CapabilityOverrideLayer::new(|mut capability| {
                capability.list_with_start_after = true;
                capability
            }),
        ));
        let catalog = Arc::new(
            BifrostCatalog::new(
                database.catalog_dsn().expose_secret(),
                Arc::clone(&storage),
                database.vala_postgres().clone(),
            )
            .await
            .expect("Bifrost catalog over the fixture warehouse"),
        );

        let roles = fixture_roles(scratch_root.path(), wal_root.path());
        let scribe_resources = roles.scribe().expect("fixture Scribe capability");
        let forge_resources = roles.forge().expect("fixture Forge capability");

        let scribe = start_scribe(
            &database,
            Arc::clone(&catalog),
            Arc::clone(&staging),
            wal_root.path(),
            scribe_resources,
        )
        .await;

        let binding = create_table(&catalog, tenant, table_name).await;
        let operator_pool = database.operator_pool().clone();
        let seeded_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT now()")
            .fetch_one(operator_pool.pool())
            .await
            .expect("fixture seed marker");
        let ingress_schema = ingress_schema();
        for file_number in 0..2_i64 {
            append_and_seal(
                &scribe,
                &catalog,
                tenant,
                &binding,
                &ingress_batch(&ingress_schema, file_number),
            )
            .await;
        }
        age_files(&operator_pool, tenant, &binding, seeded_at).await;

        let fixture = Self {
            catalog,
            operator_pool,
            vala: database.vala_postgres().clone(),
            staging,
            tenant,
            binding,
            config: ForgeConfig::default(),
            forge_resources,
            scribe,
            database,
            _warehouse: warehouse,
            _wal_root: wal_root,
            scratch_root,
        };
        let sealed = fixture.file_rows().await;
        assert_eq!(
            sealed.len(),
            2,
            "a real Scribe seal must publish two hot objects, saw {sealed:?}"
        );
        fixture
    }

    /// Build one production Forge owner over explicit catalog and store seams.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot produce a validated Forge graph.
    pub(crate) fn build_forge_for_test(
        &self,
        catalog: Arc<dyn Catalog>,
        object_store: Arc<dyn ForgeObjectStore>,
        clock: ForgeClock,
        completion_observer: ForgeWorkerCompletionObserver,
        scheduler_trigger: ForgeSchedulerTrigger,
    ) -> Arc<Forge> {
        let (_publisher, hints) =
            staging_file_channel(self.config.max_hints_per_wake).expect("fixture hint capacity");
        Arc::new(
            Forge::new(ForgeBuildConfig {
                resources: self.forge_resources.clone(),
                vala: self.vala.clone(),
                operator_pool: self.operator_pool.clone(),
                catalog,
                staging: Arc::clone(&self.staging),
                staging_lists_by_cursor: self
                    .staging
                    .info()
                    .full_capability()
                    .list_with_start_after,
                object_store,
                rewrite_spill_root: self.scratch_root.path().join("forge"),
                hints,
                config: self.config.clone(),
                maintenance_interval: Duration::from_hours(1),
                clock,
                completion_observer: Some(completion_observer),
                scheduler_trigger: Some(scheduler_trigger),
                telemetry: Arc::new(ForgeTelemetry::new()),
            })
            .expect("fixture Forge"),
        )
    }

    /// Returns the real Scribe every fixture append and seal is published through.
    ///
    /// A live-tail proof needs the same writer that owns the sealed objects, so
    /// the reader it builds observes exactly the state this fixture produced.
    pub(crate) fn scribe(&self) -> &Arc<ScribeImpl> {
        &self.scribe
    }

    /// Appends one batch through real ingress and leaves it unsealed.
    ///
    /// `file_number` separates the appended values the same way the seeded
    /// seals do, so a live-tail read can tell live rows from sealed ones.
    ///
    /// # Panics
    ///
    /// Panics when registration lookup or ingest fails.
    pub(crate) async fn append_without_seal(&self, file_number: i64) {
        append_only(
            &self.scribe,
            &self.catalog,
            self.tenant,
            &self.binding,
            &ingress_batch(&ingress_schema(), file_number),
        )
        .await;
    }

    /// Returns the registered projected source fingerprint of the fixture table.
    ///
    /// A live-tail acquisition validates every retained batch against this exact
    /// value, so a proof must ask the catalog rather than recompute it.
    ///
    /// # Panics
    ///
    /// Panics when the table has no durable registration.
    pub(crate) async fn schema_fingerprint(&self) -> wyrd_spec::vala::api::SchemaFingerprint {
        let registered = self
            .catalog
            .table_registration(&self.binding.table_ref, self.tenant)
            .await
            .expect("fixture table registration")
            .0;
        wyrd_spec::vala::api::SchemaFingerprint::new(hex::encode(registered.0))
            .expect("a registered fingerprint is a canonical wire fingerprint")
    }

    /// Reads the promotion settlement columns of the fixture table, in durable order.
    ///
    /// # Panics
    ///
    /// Panics when the read-only diagnostic query fails.
    pub(crate) async fn file_rows(&self) -> Vec<FileRow> {
        sqlx::query_as::<_, (uuid::Uuid, String, bool, Option<i64>)>(
            "SELECT id, file_path, compacted, committed_snapshot_id FROM vala.file_list \
             WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 \
             ORDER BY created_at, file_ordinal, id",
        )
        .bind(self.tenant.as_uuid())
        .bind(&self.binding.logical_namespace)
        .bind(&self.binding.table_name)
        .fetch_all(self.operator_pool.pool())
        .await
        .expect("fixture file-list inspection")
        .into_iter()
        .map(
            |(id, file_path, compacted, committed_snapshot_id)| FileRow {
                id,
                file_path,
                compacted,
                committed_snapshot_id,
            },
        )
        .collect()
    }

    /// Reads every durable promotion record of the fixture table, in durable order.
    ///
    /// The raw JSON is returned rather than the decoded record so a scenario
    /// can mutate one field of the persisted evidence exactly as a corrupted or
    /// tampered row would carry it.
    ///
    /// # Panics
    ///
    /// Panics when the read-only diagnostic query fails.
    pub(crate) async fn promotion_records(&self) -> Vec<(uuid::Uuid, serde_json::Value)> {
        sqlx::query_as::<_, (uuid::Uuid, serde_json::Value)>(
            "SELECT id, promotion_record FROM vala.file_list \
             WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 \
             ORDER BY created_at, file_ordinal, id",
        )
        .bind(self.tenant.as_uuid())
        .bind(&self.binding.logical_namespace)
        .bind(&self.binding.table_name)
        .fetch_all(self.operator_pool.pool())
        .await
        .expect("fixture promotion-record inspection")
    }

    /// Replaces one row's durable promotion record.
    ///
    /// Writing the evidence directly is the only way to separate what Forge
    /// proves about the object from what Scribe happened to record: the object
    /// itself stays untouched, so any refusal comes from re-deriving the
    /// object's own footer rather than from re-reading the same row twice.
    ///
    /// # Panics
    ///
    /// Panics when the update fails or does not name exactly one row.
    pub(crate) async fn set_promotion_record(&self, id: uuid::Uuid, record: &serde_json::Value) {
        let updated = sqlx::query("UPDATE vala.file_list SET promotion_record = $2 WHERE id = $1")
            .bind(id)
            .bind(record)
            .execute(self.operator_pool.pool())
            .await
            .expect("fixture promotion-record update")
            .rows_affected();
        assert_eq!(updated, 1, "one promotion record is replaced at a time");
    }

    /// Seals `count` more hot objects through the same real Scribe.
    ///
    /// Existing rows are already aged into promotion eligibility, so a second
    /// seal is the only way to put a table into the one state the scheduler
    /// ordering rule is about: live promoted data the rewrite route wants and
    /// unpublished hot data the promotion route owes, at the same time.
    ///
    /// # Panics
    ///
    /// Panics when a seal publishes no new `vala.file_list` row.
    pub(crate) async fn seal_more(&self, count: usize) {
        let before = self.file_rows().await.len();
        let first = i64::try_from(before).expect("fixture row counts stay representable");
        let seeded_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT now()")
            .fetch_one(self.operator_pool.pool())
            .await
            .expect("fixture seed marker");
        let schema = ingress_schema();
        for file_number in 0..i64::try_from(count).expect("fixture seal counts stay representable")
        {
            append_and_seal(
                &self.scribe,
                &self.catalog,
                self.tenant,
                &self.binding,
                &ingress_batch(&schema, first + file_number),
            )
            .await;
        }
        age_files(&self.operator_pool, self.tenant, &self.binding, seeded_at).await;
        assert_eq!(
            self.file_rows().await.len(),
            before + count,
            "a real Scribe seal publishes one row per batch"
        );
    }

    /// Registers a sibling and seals real inputs through this fixture's Scribe.
    ///
    /// # Panics
    /// Panics when registration, sealing, or eligibility aging fails.
    pub(crate) async fn register_and_seal_table(&self, name: &str, count: usize) {
        let binding = create_table(&self.catalog, self.tenant, name).await;
        let seeded_at = chrono::Utc::now();
        let schema = ingress_schema();
        for number in 0..count {
            append_and_seal(
                &self.scribe,
                &self.catalog,
                self.tenant,
                &binding,
                &ingress_batch(
                    &schema,
                    i64::try_from(number).expect("bounded fixture count"),
                ),
            )
            .await;
        }
        age_files(&self.operator_pool, self.tenant, &binding, seeded_at).await;
    }

    /// Reads every durable Forge task of this fixture's table, in durable order.
    ///
    /// The rows are returned untyped so a scenario asserts on the exact durable
    /// values the scheduler wrote — strategy, state, bound base snapshot, and
    /// bound plan — rather than on a decoded shape that could normalize a
    /// mistake away.
    ///
    /// # Panics
    ///
    /// Panics when the read-only diagnostic query fails.
    pub(crate) async fn forge_tasks(&self) -> Vec<ForgeTaskRow> {
        sqlx::query_as::<_, (uuid::Uuid, String, String, i64, serde_json::Value, Vec<u8>)>(
            "SELECT task_id, strategy, state, base_snapshot_id, plan, plan_hash \
             FROM vala.forge_tasks \
             WHERE data_tenant_id = $1 AND namespace_name = $2 AND table_name = $3 \
             ORDER BY created_at, task_id",
        )
        .bind(self.tenant.as_uuid())
        .bind(&self.binding.logical_namespace)
        .bind(&self.binding.table_name)
        .fetch_all(self.operator_pool.pool())
        .await
        .expect("fixture Forge task inspection")
        .into_iter()
        .map(
            |(task_id, strategy, state, base_snapshot_id, plan, plan_hash)| ForgeTaskRow {
                task_id,
                strategy,
                state,
                base_snapshot_id,
                plan,
                plan_hash,
            },
        )
        .collect()
    }

    /// Reads the durable Iceberg-rewrite operation phases for this tenant.
    ///
    /// # Panics
    ///
    /// Panics when the read-only diagnostic query fails.
    pub(crate) async fn rewrite_phases(&self) -> Vec<String> {
        sqlx::query_scalar::<_, String>(
            "SELECT phase FROM vala.forge_operation_state \
             WHERE data_tenant_id = $1 AND family = 'iceberg_rewrite' \
             ORDER BY prepared_at, operation_id",
        )
        .bind(self.tenant.as_uuid())
        .fetch_all(self.operator_pool.pool())
        .await
        .expect("fixture operation-state inspection")
    }

    /// Counts the Forge audit rows this tenant's hash-chained outbox holds.
    ///
    /// Every durable Forge transition appends exactly one row, so an unchanged
    /// count across a held attempt is the direct evidence that the attempt
    /// recorded no Prepared, Reset, Committed, or Recovered transition — a
    /// stronger statement than the absence of an operation-state row, which a
    /// transition could in principle write without.
    ///
    /// # Panics
    ///
    /// Panics when the read-only diagnostic query fails.
    pub(crate) async fn forge_audit_count(&self) -> i64 {
        let mut conn = self
            .vala
            .tenant_conn(self.tenant)
            .await
            .expect("fixture tenant connection");
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM vala.audit_outbox WHERE operation LIKE 'forge.%'",
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("fixture Forge audit inspection");
        conn.commit().await.expect("fixture audit read commit");
        count
    }

    /// Ages every live claim deadline past due for this fixture's tenant.
    ///
    /// A worker that dies mid-attempt leaves its claim held until the deadline
    /// lapses, and that lapse is wall-clock time no test can wait for. Moving
    /// the stored deadline into the past is the same kind of aging the file
    /// helpers do: it makes time pass, and leaves the production reclaim
    /// transaction to decide what that means.
    ///
    /// # Panics
    ///
    /// Panics when the update fails.
    pub(crate) async fn expire_claims(&self) {
        sqlx::query(
            "UPDATE vala.forge_tasks SET claim_expires_at = statement_timestamp() - interval '1 minute' \
             WHERE data_tenant_id = $1 AND state IN ('claimed', 'running')",
        )
        .bind(self.tenant.as_uuid())
        .execute(self.operator_pool.pool())
        .await
        .expect("fixture claim aging");
    }

    /// Expires the durable table lease this fixture's Forge owner holds.
    ///
    /// The lease row is the real fence owner, and every renewal is conditional
    /// on it still being unexpired, so aging it here is exactly what a lost
    /// fence looks like to production code: the next `renew` matches no row and
    /// reports the lease as lost. Nothing about the claim, the attempt, or the
    /// table is touched.
    ///
    /// # Panics
    ///
    /// Panics when the update fails.
    pub(crate) async fn expire_table_lease(&self) {
        let lease_key = vala_bifrost_redux::forge::forge_lease_key(
            self.tenant,
            &self.binding.logical_namespace,
            &self.binding.table_name,
        );
        let expired = sqlx::query(
            "UPDATE vala.maintenance_leases \
             SET expires_at = statement_timestamp() - interval '1 minute' WHERE lease_key = $1",
        )
        .bind(&lease_key)
        .execute(self.operator_pool.pool())
        .await
        .expect("fixture lease aging");
        assert_eq!(
            expired.rows_affected(),
            1,
            "the publication under test holds exactly one table lease"
        );
    }

    /// Retires the retry backoff the production reclaim path just imposed.
    ///
    /// Reclaim deliberately holds a reclaimed task back for tens of seconds so
    /// a failing task cannot spin. That interval is wall-clock time, so a
    /// scenario proving what the *takeover* does has to age past it rather than
    /// sleep through it.
    ///
    /// # Panics
    ///
    /// Panics when the update fails.
    pub(crate) async fn clear_task_backoff(&self) {
        sqlx::query(
            "UPDATE vala.forge_tasks SET ready_at = statement_timestamp(), \
             next_eligible_at = statement_timestamp() WHERE data_tenant_id = $1",
        )
        .bind(self.tenant.as_uuid())
        .execute(self.operator_pool.pool())
        .await
        .expect("fixture backoff aging");
    }

    /// Reads the durable promotion operation phases for this fixture's tenant.
    ///
    /// # Panics
    ///
    /// Panics when the read-only diagnostic query fails.
    pub(crate) async fn promotion_phases(&self) -> Vec<String> {
        sqlx::query_scalar::<_, String>(
            "SELECT phase FROM vala.forge_operation_state \
             WHERE data_tenant_id = $1 AND family = 'scribe_promotion' \
             ORDER BY prepared_at, operation_id",
        )
        .bind(self.tenant.as_uuid())
        .fetch_all(self.operator_pool.pool())
        .await
        .expect("fixture operation-state inspection")
    }

    /// Counts unexpired Forge leases still held over the fixture table.
    ///
    /// The key is derived through the production builder, so a re-scoped lease
    /// identity fails this read rather than silently counting zero.
    ///
    /// # Panics
    ///
    /// Panics when the read-only diagnostic query fails.
    pub(crate) async fn live_leases(&self) -> i64 {
        let lease_key = vala_bifrost_redux::forge::forge_lease_key(
            self.tenant,
            &self.binding.logical_namespace,
            &self.binding.table_name,
        );
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM vala.maintenance_leases \
             WHERE lease_key = $1 AND expires_at > statement_timestamp()",
        )
        .bind(&lease_key)
        .fetch_one(self.operator_pool.pool())
        .await
        .expect("fixture lease inspection")
    }

    /// Collects the live data-file paths of the table's current snapshot.
    ///
    /// The table-relative suffix is rewritten onto the tenant object prefix,
    /// exactly as the catalog's own pinning rule does, so a path here compares
    /// directly with a `vala.file_list` path.
    ///
    /// # Panics
    ///
    /// Panics when the table, its manifest list, or a manifest cannot be read.
    pub(crate) async fn live_data_paths(&self) -> BTreeSet<String> {
        let catalog = self.catalog.iceberg_catalog();
        let table = catalog
            .load_table(&self.binding.table_ident())
            .await
            .expect("fixture table load");
        let mut paths = BTreeSet::new();
        let Some(snapshot) = table.metadata().current_snapshot() else {
            return paths;
        };
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .expect("fixture manifest list");
        for manifest_file in manifests.entries() {
            let manifest = manifest_file
                .load_manifest(table.file_io())
                .await
                .expect("fixture manifest");
            for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
                let path = entry.data_file().file_path().to_owned();
                let canonical = path
                    .strip_prefix(table.metadata().location())
                    .and_then(|suffix| suffix.strip_prefix('/'))
                    .map(|suffix| format!("{}/{suffix}", self.binding.object_prefix))
                    .unwrap_or(path);
                paths.insert(canonical);
            }
        }
        paths
    }

    /// Returns the pod-wide scratch root every attempt takes its child from.
    ///
    /// An admitted attempt creates one `forge-runtime-{task}-{attempt}-*` child
    /// beneath this root and removes it when its lease is finalized, so the
    /// absence of a child naming an attempt is the direct evidence that the
    /// attempt's scratch was released rather than leaked.
    pub(crate) fn rewrite_spill_root(&self) -> std::path::PathBuf {
        self.scratch_root.path().join("forge")
    }

    /// Snapshots every object under the table prefix with its exact content hash.
    ///
    /// Comparing the map across a promotion is the direct proof that no data
    /// object was created, rewritten, or deleted: any PUT changes either the
    /// key set or one object's digest.
    ///
    /// # Panics
    ///
    /// Panics when the staging operator cannot be listed or read.
    pub(crate) async fn object_digests(&self) -> BTreeMap<String, String> {
        let prefix = format!("{}/", self.binding.object_prefix);
        let entries = self
            .staging
            .list_with(&prefix)
            .recursive(true)
            .await
            .expect("fixture object listing");
        let mut digests = BTreeMap::new();
        for entry in entries {
            if entry.metadata().is_dir() {
                continue;
            }
            let bytes = self
                .staging
                .read(entry.path())
                .await
                .expect("fixture object read");
            digests.insert(entry.path().to_owned(), {
                use sha2::Digest as _;
                hex::encode(sha2::Sha256::digest(bytes.to_bytes()))
            });
        }
        digests
    }
}

/// Production scheduler and worker supervisors retained for one tier-2 test.
///
/// Deliberately the same lifecycle a server role supervisor runs: the loops are
/// the production ones, and the only additions are the passive trigger and
/// observer the production owner already accepts, so one pass and one attempt
/// are observable without polling or sleeping.
pub(crate) struct SupervisedPromotion {
    /// Passive wake-up for deterministic production scheduler passes.
    scheduler_trigger: ForgeSchedulerTrigger,
    /// Passive observation of returned and successful worker attempts.
    worker_observer: ForgeWorkerCompletionObserver,
    /// Cancellation boundary for the production scheduler loop.
    scheduler_stop: CancellationToken,
    /// Cancellation boundary observed by active worker execution.
    worker_stop: CancellationToken,
    /// Running production scheduler supervisor.
    scheduler_task: JoinHandle<Result<(), ForgeError>>,
    /// Running production worker supervisor.
    worker_task: Option<JoinHandle<Result<(), ForgeError>>>,
    /// Whether a replacement worker is armed but not yet spawned.
    worker_armed: bool,
    /// Forge graph kept alive for the supervised lifetime.
    ///
    /// Retaining it is what lets a scenario stop and restart the worker over
    /// one scheduler generation: the singleton planning fence is TTL-bound and
    /// is never released on shutdown, so a second supervisor in one test would
    /// stand by and plan nothing.
    forge: Arc<Forge>,
}

impl SupervisedPromotion {
    /// Start one production scheduler and worker over explicit seams.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot construct a validated worker graph.
    pub(crate) fn start(
        fixture: &PromotionIntegrationFixture,
        catalog: Arc<dyn Catalog>,
        object_store: Arc<dyn ForgeObjectStore>,
        clock: ForgeClock,
    ) -> Self {
        let scheduler_trigger = ForgeSchedulerTrigger::with_owner_for_test(uuid::Uuid::now_v7());
        let worker_observer = ForgeWorkerCompletionObserver::new();
        let forge = fixture.build_forge_for_test(
            catalog,
            object_store,
            clock,
            worker_observer.clone(),
            scheduler_trigger.clone(),
        );
        let worker = ForgeWorker::new(
            Arc::clone(&forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("fixture Forge worker");
        let scheduler_stop = CancellationToken::new();
        let worker_stop = CancellationToken::new();
        let scheduler_task = tokio::spawn({
            let forge = Arc::clone(&forge);
            let stop = scheduler_stop.clone();
            async move { forge.run(stop, ForgeRoleReadiness::detached()).await }
        });
        let worker_task = tokio::spawn({
            let stop = worker_stop.clone();
            async move { worker.run(stop, ForgeRoleReadiness::detached()).await }
        });
        Self {
            scheduler_trigger,
            worker_observer,
            scheduler_stop,
            worker_stop,
            scheduler_task,
            worker_task: Some(worker_task),
            worker_armed: false,
            forge,
        }
    }

    /// Borrows the retained Forge graph so a scenario can drive one production
    /// owner directly instead of through a claimed worker attempt.
    pub(crate) fn forge(&self) -> Arc<Forge> {
        Arc::clone(&self.forge)
    }

    /// Shares the production completion observer this supervisor registered.
    ///
    /// The same observer instance reaches every worker built from
    /// [`Self::forge`], so a scenario can arm one of its passive barriers on a
    /// directly constructed worker and still be holding the production seam.
    pub(crate) fn observer(&self) -> &ForgeWorkerCompletionObserver {
        &self.worker_observer
    }

    /// Runs the production reclaim transaction once, outside the worker loop.
    ///
    /// The supervised loop reclaims and then claims in one iteration, so a
    /// scenario that must observe the reclaim *before* the next claim cannot
    /// get there through a tick. This calls the same reclaim the loop calls,
    /// on a worker built from the same retained graph.
    ///
    /// # Panics
    ///
    /// Panics when the worker cannot be built or the reclaim fails.
    pub(crate) async fn reclaim_expired_claims(&self) {
        let worker = ForgeWorker::new(
            Arc::clone(&self.forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("fixture Forge worker");
        worker
            .reclaim_expired_attempts_for_test(16)
            .await
            .expect("fixture reclaim pass");
    }

    /// Arm a replacement worker over the same scheduler generation.
    ///
    /// Every `run_one_*` helper stops the worker so the caller's assertions
    /// cannot race a retry. A scenario that needs a second attempt therefore
    /// restarts the worker rather than building a second supervisor, which
    /// would stand by behind the first one's unexpired planning fence.
    ///
    /// The replacement's cancellation token is installed here, so a caller may
    /// capture it before the attempt starts. The worker itself is spawned by
    /// the next `run_one_*` call,
    /// after that call has armed its returned-attempt barrier. Spawning here
    /// instead would let the worker claim an already-ready task and return an
    /// unheld attempt before the barrier existed, which is exactly the retry
    /// the helpers exist to exclude.
    ///
    /// # Panics
    ///
    /// Panics when a worker is already running or armed.
    pub(crate) fn restart_worker(&mut self) {
        assert!(
            self.worker_task.is_none() && !self.worker_armed,
            "a supervisor runs one worker at a time"
        );
        self.worker_stop = CancellationToken::new();
        self.worker_armed = true;
    }

    /// Spawn the armed replacement worker, if the caller armed one.
    ///
    /// # Panics
    ///
    /// Panics when the validated worker graph cannot be constructed.
    fn start_armed_worker(&mut self) {
        if !self.worker_armed {
            return;
        }
        let worker = ForgeWorker::new(
            Arc::clone(&self.forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("fixture Forge worker");
        let stop = self.worker_stop.clone();
        self.worker_task = Some(tokio::spawn(async move {
            worker.run(stop, ForgeRoleReadiness::detached()).await
        }));
        self.worker_armed = false;
    }

    /// Request and await one production planning pass without running work.
    ///
    /// # Panics
    ///
    /// Panics when the scheduler misses its deterministic bound.
    pub(crate) async fn schedule_only(&self) {
        self.schedule_once().await;
    }

    /// Request and await one pass from the running production scheduler.
    ///
    /// # Panics
    ///
    /// Panics when the scheduler misses its deterministic bound.
    async fn schedule_once(&self) {
        let expected = self.scheduler_trigger.completed_passes().saturating_add(1);
        self.scheduler_trigger.request_pass();
        tokio::time::timeout(
            FIXTURE_BOUND,
            self.scheduler_trigger.wait_for_passes_at_least(expected),
        )
        .await
        .expect("production Forge scheduler pass bound");
    }

    /// Schedule one pass and await exactly one successful worker attempt.
    ///
    /// The attempt is held while the worker is stopped so no retry can race
    /// the caller's assertions.
    ///
    /// # Panics
    ///
    /// Panics when a deterministic bound is missed or the attempt failed.
    pub(crate) async fn run_one_success(&mut self) {
        let expected = self.worker_observer.completed().saturating_add(1);
        let expected_errors = self.worker_observer.returned_errors().len();
        self.worker_observer.hold_after_next_attempt_for_test();
        self.start_armed_worker();
        self.schedule_once().await;
        tokio::time::timeout(
            FIXTURE_BOUND,
            self.worker_observer.wait_for_held_attempt_for_test(),
        )
        .await
        .expect("production Forge worker attempt bound");
        assert_eq!(
            self.worker_observer.returned_errors().len(),
            expected_errors,
            "the attempt was expected to succeed: {:?}",
            self.worker_observer.returned_errors()
        );
        self.stop_worker().await;
        assert_eq!(self.worker_observer.completed(), expected);
    }

    /// Await exactly one successful worker attempt without planning anything.
    ///
    /// The worker slot claims from Postgres on its own loop, so a caller whose
    /// task is already `ready` does not need a planning pass to reach it — and
    /// must not take one. A pass issued while a promotion is still in flight
    /// observes the table mid-settlement and can bind a rewrite against the
    /// pre-promotion snapshot, which makes any later "exactly one rewrite"
    /// statement depend on scheduler and worker interleaving. This settles the
    /// already-enqueued work and leaves planning to the caller's own pass.
    ///
    /// # Panics
    ///
    /// Panics when a deterministic bound is missed or the attempt failed.
    pub(crate) async fn settle_one_success(&mut self) {
        let expected = self.worker_observer.completed().saturating_add(1);
        let expected_errors = self.worker_observer.returned_errors().len();
        self.worker_observer.hold_after_next_attempt_for_test();
        self.start_armed_worker();
        tokio::time::timeout(
            FIXTURE_BOUND,
            self.worker_observer.wait_for_held_attempt_for_test(),
        )
        .await
        .expect("production Forge worker attempt bound");
        assert_eq!(
            self.worker_observer.returned_errors().len(),
            expected_errors,
            "the attempt was expected to succeed: {:?}",
            self.worker_observer.returned_errors()
        );
        self.stop_worker().await;
        assert_eq!(self.worker_observer.completed(), expected);
    }

    /// Settle at least one successful attempt without planning anything.
    ///
    /// A route whose passes are deliberately bounded — orphan collection with
    /// one listing page per pass — settles many short attempts in the time the
    /// held-attempt barrier takes to stop the worker. Requiring exactly one
    /// completion would make the scenario depend on that timing, so this
    /// requires progress and success rather than a single episode.
    ///
    /// # Panics
    ///
    /// Panics when a deterministic bound is missed, an attempt returned an
    /// error, or the worker made no progress at all.
    pub(crate) async fn settle_some_success(&mut self) {
        let before = self.worker_observer.completed();
        let expected_errors = self.worker_observer.returned_errors().len();
        self.worker_observer.hold_after_next_attempt_for_test();
        self.start_armed_worker();
        tokio::time::timeout(
            FIXTURE_BOUND,
            self.worker_observer.wait_for_held_attempt_for_test(),
        )
        .await
        .expect("production Forge worker attempt bound");
        assert_eq!(
            self.worker_observer.returned_errors().len(),
            expected_errors,
            "the attempt was expected to succeed: {:?}",
            self.worker_observer.returned_errors()
        );
        self.stop_worker().await;
        assert!(
            self.worker_observer.completed() > before,
            "the worker settled no attempt"
        );
    }

    /// Schedule one pass and await exactly one returned worker error while
    /// `during` drives the seam the attempt is parked on.
    ///
    /// The control that decides how the attempt ends can only run *while* the
    /// worker is parked at the catalog seam, so it cannot be applied before
    /// scheduling or after the attempt is held.
    ///
    /// # Panics
    ///
    /// Panics when a deterministic bound is missed or the attempt unexpectedly
    /// succeeded.
    pub(crate) async fn run_one_failure_while<F>(mut self, during: F) -> Self
    where
        F: std::future::Future<Output = ()>,
    {
        let expected_errors = self
            .worker_observer
            .returned_errors()
            .len()
            .saturating_add(1);
        self.worker_observer.hold_after_next_attempt_for_test();
        self.start_armed_worker();
        self.schedule_once().await;
        tokio::time::timeout(FIXTURE_BOUND, during)
            .await
            .expect("parked production commit seam bound");
        tokio::time::timeout(
            FIXTURE_BOUND,
            self.worker_observer.wait_for_held_attempt_for_test(),
        )
        .await
        .expect("production Forge worker attempt bound");
        self.stop_worker().await;
        assert_eq!(
            self.worker_observer.returned_errors().len(),
            expected_errors,
            "the attempt was expected to return exactly one error: {:?}",
            self.worker_observer.returned_errors()
        );
        self
    }

    /// Schedule one pass and await exactly one returned worker error.
    ///
    /// Unlike [`Self::run_one_failure_while`] no seam is parked, because the
    /// refusal under test happens before the attempt ever reaches the catalog.
    ///
    /// # Panics
    ///
    /// Panics when a deterministic bound is missed or the attempt succeeded.
    pub(crate) async fn run_one_failure(&mut self) -> String {
        let before = self.worker_observer.returned_errors();
        self.worker_observer.hold_after_next_attempt_for_test();
        self.start_armed_worker();
        self.schedule_once().await;
        tokio::time::timeout(
            FIXTURE_BOUND,
            self.worker_observer.wait_for_held_attempt_for_test(),
        )
        .await
        .expect("production Forge worker attempt bound");
        self.stop_worker().await;
        let after = self.worker_observer.returned_errors();
        assert_eq!(
            after.len(),
            before.len().saturating_add(1),
            "the attempt was expected to return exactly one error: {after:?}"
        );
        after
            .last()
            .expect("one error was just returned")
            .to_owned()
    }

    /// Schedule one pass and hold the rewrite between its handoff and its
    /// publication while `during` mutates real publication authority.
    ///
    /// The barrier is the only point at which a knowable authority change is
    /// both possible and consequential: managed execution has produced its
    /// handoff and outputs, and nothing has yet been derived, audited, or
    /// submitted. `during` therefore mutates the same durable owner production
    /// code will consult — the lease row, the cancellation token, the clock, or
    /// the catalog — rather than any test-only verdict.
    ///
    /// Returns the one error the held attempt returned.
    ///
    /// # Panics
    ///
    /// Panics when a deterministic bound is missed or the attempt did not
    /// return exactly one error.
    pub(crate) async fn run_one_failure_holding_handoff<F>(&mut self, during: F) -> String
    where
        F: std::future::Future<Output = ()>,
    {
        let before = self.worker_observer.returned_errors();
        self.worker_observer
            .hold_after_next_rewrite_handoff_for_test();
        self.worker_observer.hold_after_next_attempt_for_test();
        self.start_armed_worker();
        self.schedule_once().await;
        tokio::time::timeout(
            FIXTURE_BOUND,
            self.worker_observer
                .wait_for_held_rewrite_handoff_for_test(),
        )
        .await
        .expect("production Forge rewrite handoff bound");
        tokio::time::timeout(FIXTURE_BOUND, during)
            .await
            .expect("held publication authority mutation bound");
        self.worker_observer.release_held_rewrite_handoff_for_test();
        tokio::time::timeout(
            FIXTURE_BOUND,
            self.worker_observer.wait_for_held_attempt_for_test(),
        )
        .await
        .expect("production Forge worker attempt bound");
        self.stop_worker().await;
        let after = self.worker_observer.returned_errors();
        assert_eq!(
            after.len(),
            before.len().saturating_add(1),
            "the held attempt was expected to return exactly one error: {after:?}"
        );
        after
            .last()
            .expect("one error was just returned")
            .to_owned()
    }

    /// Borrows the token production worker execution observes as shutdown.
    pub(crate) fn worker_stop(&self) -> CancellationToken {
        self.worker_stop.clone()
    }

    /// Borrows the errors production worker attempts returned so far.
    pub(crate) fn returned_errors(&self) -> Vec<String> {
        self.worker_observer.returned_errors()
    }

    /// Borrows the possible-output set the most recent returned failure carried.
    ///
    /// `None` when no attempt has returned an error yet, or when the newest one
    /// was not an unsettled rewrite. The evidence is taken typed off the
    /// production observer rather than parsed out of the rendered error,
    /// because the object identities are exactly what a refusal has to preserve
    /// and the wrapper's text carries only their count.
    pub(crate) fn last_possible_rewrite_outputs(
        &self,
    ) -> Option<Vec<vala_bifrost_redux::forge::ForgeUnsettledOutput>> {
        self.worker_observer
            .returned_unsettled_outputs()
            .pop()
            .flatten()
    }

    /// Cancel and join the worker before it can retry a returned attempt.
    ///
    /// # Panics
    ///
    /// Panics when the worker misses its bounded shutdown or exits unexpectedly.
    async fn stop_worker(&mut self) {
        self.worker_stop.cancel();
        self.worker_observer.release_held_attempt_for_test();
        let task = self.worker_task.take().expect("worker is stopped once");
        tokio::time::timeout(FIXTURE_BOUND, task)
            .await
            .expect("production Forge worker shutdown bound")
            .expect("production Forge worker task")
            .expect("production Forge worker shutdown");
    }

    /// Cancel and join both production supervisors.
    ///
    /// # Panics
    ///
    /// Panics when either production loop misses its bounded shutdown.
    pub(crate) async fn shutdown(mut self) {
        self.scheduler_stop.cancel();
        if self.worker_task.is_some() {
            self.stop_worker().await;
        }
        tokio::time::timeout(FIXTURE_BOUND, self.scheduler_task)
            .await
            .expect("production Forge scheduler shutdown bound")
            .expect("production Forge scheduler task")
            .expect("production Forge scheduler shutdown");
    }
}

/// Builds the local-filesystem storage owner both Scribe and the catalog use.
///
/// One owner backs both, so the object Scribe writes is at the path the
/// catalog resolves and the path Forge reads — which is the whole point of an
/// append-in-place promotion.
fn local_storage_owner(root: &std::path::Path) -> Arc<vala_bifrost_redux::storage::BifrostStorage> {
    let signer = wyrd_storage::signer::BackendSigner::Local(
        wyrd_storage::local::LocalSigner::new(root.to_path_buf()).expect("fixture local signer"),
    );
    Arc::new(vala_bifrost_redux::storage::BifrostStorage::new(
        Arc::new(wyrd_storage::handle::StorageHandle::new(signer)),
        vala_bifrost_redux::storage::BifrostStoragePolicy::resolve(
            vala_bifrost_redux::storage::BifrostStorageConfig::default(),
            2 * 1024 * 1024 * 1024,
            false,
        )
        .expect("fixture storage policy"),
        None,
    ))
}

/// Composes Scribe and Forge capabilities over the fixture's real volume roots.
///
/// The snapshot is injected rather than probed so capacity is deterministic
/// across machines, while the volume roots are the fixture's actual
/// directories, so scratch accounting is measured against real files.
///
/// # Panics
///
/// Panics when the injected snapshot and policy cannot compose valid roles,
/// which is a construction invariant rather than an input.
fn fixture_roles(
    scratch_root: &std::path::Path,
    wal_root: &std::path::Path,
) -> vala_bifrost_redux::resources::BifrostRoleResources {
    let scribe_stage = wal_root.join("scribe-stage");
    let scribe_output = scratch_root.join("scribe-output");
    let forge_scratch = scratch_root.join("forge");
    let oracle_scratch = scratch_root.join("oracle");
    for root in [
        &scribe_stage,
        &scribe_output,
        &forge_scratch,
        &oracle_scratch,
    ] {
        std::fs::create_dir_all(root).expect("fixture volume root");
    }
    BifrostRuntimeResources::from_snapshot(
        SystemResourceSnapshot {
            memory_limit_bytes: 4 * 1024 * 1024 * 1024,
            effective_cpu: 4,
            scratch_capacity_bytes: 10 * 1024 * 1024 * 1024,
            scratch_available_bytes: 10 * 1024 * 1024 * 1024,
            memory_source: ResourceSource::Injected,
            cpu_source: ResourceSource::Injected,
        },
        BifrostResourcePolicy {
            roles: [BifrostRole::Scribe, BifrostRole::Forge]
                .into_iter()
                .collect(),
            memory_limit_bytes: None,
            unmanaged_reserve_bytes: None,
            scratch_limit_bytes: None,
            effective_cpu: None,
            oracle_query_slot_limit: None,
            scratch_root: scratch_root.to_owned(),
            volume_roots: Some(BifrostVolumeRoots {
                wal: wal_root.to_owned(),
                scribe_stage,
                scribe_output_scratch: scribe_output,
                forge_scratch,
                oracle_scratch,
            }),
        },
    )
    .expect("fixture Bifrost resources")
    .compose_roles()
    .expect("fixture role resources")
}

/// Registers the publication fence and builds one real Scribe over the fixture.
///
/// Extracted from [`PromotionIntegrationFixture::start`] because the Scribe
/// graph is a self-contained composition: a node identity, its fence row, its
/// WAL writer, and the production build config that binds them to the shared
/// catalog and operator. Nothing about it depends on the Forge side of the
/// fixture.
///
/// # Panics
///
/// Panics when the fence cannot be registered or when the WAL writer, geometry,
/// or Scribe graph cannot be constructed, each of which is a fixture-setup
/// invariant.
async fn start_scribe(
    database: &wyrd_dev_fixtures::pg::PgFixture,
    catalog: Arc<BifrostCatalog>,
    staging: Arc<Operator>,
    wal_root: &std::path::Path,
    resources: vala_bifrost_redux::resources::ScribeResources,
) -> Arc<ScribeImpl> {
    let (_, output_scratch) = resources
        .volume_capabilities()
        .expect("fixture Scribe volumes");
    let node_id = vala_bifrost_redux::scribe::stream_identity::NodeId::generate();
    register_scribe_fence(database, node_id).await;
    let wal = Arc::new(
        WalWriter::new(
            wal_root,
            *node_id.as_uuid().as_bytes(),
            1,
            WalConfig::default(),
        )
        .expect("fixture WAL writer"),
    );
    Arc::new(
        ScribeImpl::new_with_execution_pools(ScribeBuildConfig {
            catalog: Some(catalog),
            operator: staging,
            wal,
            stream: vala_bifrost_redux::scribe::stream_identity::StreamIdentity::new(
                node_id,
                vala_bifrost_redux::scribe::stream_identity::WriterEpoch::new(1),
            ),
            admission: AdmissionConfig::default(),
            coordination_runtime: tokio::runtime::Handle::current(),
            execution_pools: ScribeExecutionPools::new(
                ScribeIngressCpuPool::new_with_capacity(2, 256),
                ScribePersistenceCpuPool::new_with_capacity(2, 64),
                ScribeWalIoPool::new_with_capacity(2, 256),
            ),
            persistence: Some(
                ScribePersistenceConfig::new(Arc::new(database.vala_postgres().clone()), 64, 2)
                    .with_operator_pool(database.operator_pool().clone())
                    .with_output_scratch(output_scratch),
            ),
            resources,
            ingest_limits: vala_bifrost_redux::gate::limits::IngestLimits::default(),
            geometry:
                vala_bifrost_redux::scribe::geometry::ScribeGeometry::for_uniform_shard_rotation(
                    WalConfig::default().segment_bytes,
                    vala_bifrost_redux::scribe::memtable::MEMTABLE_ROTATION_BYTES,
                    ScribePressureConfig::default().seal_max_age,
                )
                .expect("fixture Scribe geometry"),
            staging_file_publisher: None,
        })
        .expect("fixture Scribe"),
    )
}

/// Registers the cluster-node row Scribe's publication fence requires.
///
/// Scribe refuses to publish a `vala.file_list` row without a live fence, so a
/// fixture that skipped this would be testing the fence, not promotion.
///
/// # Panics
///
/// Panics when the privileged registration cannot be issued.
async fn register_scribe_fence(
    database: &wyrd_dev_fixtures::pg::PgFixture,
    node_id: vala_bifrost_redux::scribe::stream_identity::NodeId,
) {
    let superuser = database.superuser_pool().await.expect("superuser pool");
    sqlx::query(
        "INSERT INTO vala.cluster_nodes \
         (data_tenant_id,node_id,role,advertise_addr,fencing_token,started_at,heartbeat_at) \
         VALUES ($1,$2,'scribe','127.0.0.1:1',$3,now(),now()) \
         ON CONFLICT (data_tenant_id,node_id,role) \
         DO UPDATE SET fencing_token=EXCLUDED.fencing_token,heartbeat_at=now()",
    )
    .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
    .bind(node_id.as_uuid())
    .bind(1_i64)
    .execute(&superuser)
    .await
    .expect("register the Scribe publication fence");
}

/// The partition day every fixture row lands in: yesterday, UTC.
///
/// Scribe sets the floor with its event-time acceptance window, and Forge sets
/// the ceiling by refusing to act on a partition that is still open. Yesterday
/// is the most recent closed day, so it satisfies both with the widest margin.
///
/// # Panics
///
/// Panics when midnight is not representable, which cannot happen for a UTC day.
pub(crate) fn fixture_day() -> chrono::NaiveDate {
    chrono::Utc::now().date_naive() - chrono::Duration::days(1)
}

/// User-visible ingress schema for the fixture table.
///
/// `wyrd_event_time` is carried explicitly so the fixture selects its own event
/// day; Scribe lifts that column into the managed slot verbatim.
fn ingress_schema() -> Arc<ArrowSchema> {
    Arc::new(ArrowSchema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new(
            wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME,
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
    ]))
}

/// Builds one two-row ingress batch whose event times land inside the fixture day.
///
/// `file_number` separates the batches so each sealed object carries a distinct
/// value and event-time range, which makes the pair a genuine promotion group.
///
/// # Panics
///
/// Panics when the fixture timestamp or batch cannot be constructed.
fn ingress_batch(schema: &Arc<ArrowSchema>, file_number: i64) -> RecordBatch {
    let base = fixture_day()
        .and_hms_opt(12, 0, 0)
        .expect("fixture timestamp")
        .and_utc()
        .timestamp_micros()
        + file_number * 1_000_000;
    RecordBatch::try_new(
        Arc::clone(schema),
        vec![
            Arc::new(Int64Array::from(vec![file_number, file_number + 10])),
            Arc::new(
                TimestampMicrosecondArray::from(vec![base, base + 1_000]).with_timezone("UTC"),
            ),
        ],
    )
    .expect("fixture ingress batch")
}

/// Encodes one batch as the native Arrow IPC stream Scribe ingress accepts.
///
/// # Panics
///
/// Panics when the batch cannot be written to an IPC stream.
fn ingress_ipc(batch: &RecordBatch) -> bytes::Bytes {
    let mut ipc = Vec::new();
    {
        let mut writer =
            arrow::ipc::writer::StreamWriter::try_new(&mut ipc, batch.schema().as_ref())
                .expect("fixture IPC writer");
        writer.write(batch).expect("fixture IPC batch");
        writer.finish().expect("fixture IPC finish");
    }
    bytes::Bytes::from(ipc)
}

/// Registers the fixture table through the real catalog and returns its binding.
///
/// # Panics
///
/// Panics when the binding cannot be resolved or the table cannot be created.
async fn create_table(
    catalog: &BifrostCatalog,
    tenant: DataTenantId,
    table_name: &str,
) -> TenantTableBinding {
    let binding =
        TenantTableBinding::resolve((tenant, TableRef::new(BifrostNamespace::Bifrost, table_name)))
            .expect("fixture table binding");
    catalog
        .create_table(CreateTableRequest {
            table: binding.table_ref.clone(),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant,
            physical_layout: Some(wyrd_spec::vala::api::PhysicalLayoutWire {
                partition_granularity: wyrd_spec::vala::api::TimeGranularityWire::Day,
                sort_keys: Vec::new(),
                bloom_columns: Vec::new(),
            }),
            audit: None,
        })
        .await
        .expect("fixture table");
    binding
}

/// Drives one real Scribe append and seal for the fixture table.
///
/// Every durable value of the resulting `vala.file_list` row — path, size, row
/// count, event-time bounds, LSN range, partition — is derived by Scribe from
/// the batch it encoded, so the fixture cannot describe a file production would
/// never produce.
///
/// # Panics
///
/// Panics when registration lookup, ingest, or seal fails, each of which is a
/// fixture-setup invariant.
async fn append_and_seal(
    scribe: &ScribeImpl,
    catalog: &BifrostCatalog,
    tenant: DataTenantId,
    binding: &TenantTableBinding,
    batch: &RecordBatch,
) {
    append_only(scribe, catalog, tenant, binding, batch).await;
    scribe.flush_staged().await.expect("fixture Scribe seal");
}

/// Drives one real Scribe append for the fixture table without sealing it.
///
/// The rows stay in the writer's live memtable, which is the only state a
/// live-tail read is allowed to see before a seal publishes an object. Sealing
/// is a separate step so a lifetime proof can hold both states at once.
///
/// # Panics
///
/// Panics when registration lookup or ingest fails, each of which is a
/// fixture-setup invariant.
async fn append_only(
    scribe: &ScribeImpl,
    catalog: &BifrostCatalog,
    tenant: DataTenantId,
    binding: &TenantTableBinding,
    batch: &RecordBatch,
) {
    let (fingerprint, _) = catalog
        .table_registration(&binding.table_ref, tenant)
        .await
        .expect("fixture table registration");
    let principal = wyrd_runtime::principal::Principal {
        id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
        kind: wyrd_runtime::principal::PrincipalKind::User,
        tenant_id: tenant,
        roles: Vec::new(),
        effective_permissions: wyrd_runtime::PermissionSet::new(),
    };
    let audit_event = wyrd_spec::vala::api::AuditEvent::new(
        wyrd_spec::request_id::RequestId::now_v7(),
        None,
        "bifrost.write".to_owned(),
        format!(
            "bifrost://{}/{}",
            binding.logical_namespace, binding.table_name
        ),
        None,
        principal.id,
        wyrd_spec::auth::PrincipalKindTag::User,
        wyrd_spec::vala::api::AuthMethod::Internal,
        "bifrost_write:write".to_owned(),
        wyrd_spec::vala::api::AuditDecision::Allow,
        wyrd_spec::vala::api::AuditResult::Success,
        "promotion fixture ingest".to_owned(),
    );
    scribe
        .ingest_native_for_test(NativeIngressTestFrame {
            principal,
            table: binding.table_ref.clone(),
            expected_schema_fingerprint: fingerprint,
            request_id: wyrd_spec::request_id::RequestId::now_v7(),
            batch_id: uuid::Uuid::now_v7(),
            audit_event,
            payload: ingress_ipc(batch),
        })
        .await
        .expect("fixture Scribe ingest");
}

/// Ages every `file_list` row the fixture just sealed.
///
/// Forge's staging discovery ignores rows younger than its settle floor, so a
/// fixture that wants its files considered on the next pass has to backdate
/// them. Scoping by `since` leaves rows an earlier step kept young untouched.
///
/// # Panics
///
/// Panics when the aging update fails.
async fn age_files(
    operator_pool: &vala_sql::OperatorPool,
    tenant: DataTenantId,
    binding: &TenantTableBinding,
    since: chrono::DateTime<chrono::Utc>,
) {
    sqlx::query(
        "UPDATE vala.file_list SET created_at = now() - interval '3 minutes' \
         WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 AND created_at >= $4",
    )
    .bind(tenant.as_uuid())
    .bind(&binding.logical_namespace)
    .bind(&binding.table_name)
    .bind(since)
    .execute(operator_pool.pool())
    .await
    .expect("fixture file aging");
}

/// Builds one manual Forge clock and its control for deadline-bounded proofs.
///
/// # Panics
///
/// Panics when the manual clock cannot be initialized.
pub(crate) fn manual_clock() -> (ForgeClock, ForgeClockControl) {
    ForgeClock::manual(chrono::Utc::now())
}

/// The one Tier-2 telemetry observer every Forge integration test installs.
///
/// A Forge integration test that only proves durable state cannot distinguish
/// "the route ran" from "the route ran and reported what it did", and the
/// production route is the *only* thing allowed to report. This owner installs
/// the real metrics recorder and the production-shaped OpenTelemetry pipeline
/// once per test process, then exposes deltas relative to the moment it was
/// installed, so a test asserts on production emission rather than on a
/// test-only signal. It exists here, centrally, so no test grows a private
/// telemetry path of its own.
///
/// Installation is process-wide and happens exactly once. Nextest runs every
/// test in its own process, so a checkpoint per test is a checkpoint per
/// process; a second installation in one process is a fixture defect and
/// panics rather than silently observing nothing.
pub(crate) struct ForgeTelemetryCheckpoint {
    /// Process-wide metrics recorder every production counter writes into.
    recorder: Arc<wyrd_bench::BenchmarkRecorder>,
    /// Handle over spans exported by the production tracing pipeline.
    capture: wyrd_telemetry::TestTraceCapture,
    /// Number of spans finished before the workload started.
    span_checkpoint: usize,
    /// Keeps the installed provider alive for the lifetime of the test.
    _telemetry: wyrd_telemetry::TelemetryGuard,
}

impl ForgeTelemetryCheckpoint {
    /// Installs the production telemetry pipeline and marks a starting point.
    ///
    /// # Panics
    ///
    /// Panics when a global metrics recorder or tracing subscriber is already
    /// installed in this process, which means two fixtures are competing for
    /// one process-wide seam and no delta would be trustworthy.
    pub(crate) fn install() -> Self {
        let recorder = wyrd_bench::BenchmarkRecorder::new()
            .install()
            .expect("no other global metrics recorder is installed in this test process");
        let (telemetry, capture) =
            wyrd_telemetry::init_test_capture(wyrd_telemetry::TelemetryConfig {
                filter: "info".to_owned(),
                service_name: Some("forge-integration".to_owned()),
                sample_ratio: Some(1.0),
                ..wyrd_telemetry::TelemetryConfig::default()
            })
            .expect("no other global tracing subscriber is installed in this test process");
        Self {
            span_checkpoint: capture.checkpoint(),
            recorder,
            capture,
            _telemetry: telemetry,
        }
    }

    /// Returns the production spans finished since installation, by name.
    pub(crate) fn spans_named(&self, name: &str) -> Vec<wyrd_telemetry::CapturedSpan> {
        self.spans_named_since(self.span_checkpoint, name)
    }

    /// Marks the current end of the finished-span stream.
    ///
    /// Installation is once per process, so a scenario that drives several
    /// independent phases in one process needs a moving origin: a phase that
    /// asserts "no catalog commit was reported" must not be answered by a
    /// commit an earlier phase legitimately made. Pass the returned mark to
    /// [`Self::spans_named_since`].
    pub(crate) fn mark(&self) -> usize {
        self.capture.checkpoint()
    }

    /// Returns the production spans finished since `mark`, by name.
    pub(crate) fn spans_named_since(
        &self,
        mark: usize,
        name: &str,
    ) -> Vec<wyrd_telemetry::CapturedSpan> {
        self.capture
            .finished_since(mark)
            .into_iter()
            .filter(|span| span.name == name)
            .collect()
    }

    /// Returns every production metric series recorded since installation.
    ///
    /// A family-presence assertion cannot distinguish a balanced gauge from a
    /// leaked one, so a scenario that must prove exact label sets or a return
    /// to zero reads the raw series instead.
    pub(crate) fn snapshot(&self) -> wyrd_bench::BenchmarkMetricSnapshot {
        self.recorder.snapshot()
    }

    /// Asserts every named production metric family was registered and used.
    ///
    /// # Panics
    ///
    /// Panics naming the missing families when the route did not emit them.
    pub(crate) fn require_metrics(&self, families: &[&str]) {
        self.recorder
            .require_metrics(families)
            .expect("the production route emits its declared metric families");
    }
}
