//! Typed, pod-local live-tail reads over writable and immutable memtable data.
//!
//! The Scribe live source owns no WAL or SQL access. Oracle discovers the
//! partitions a Scribe serves through the authenticated private
//! `ListActiveStreams` RPC and reads them as lazily produced live batches.

use std::sync::Arc;

use async_trait::async_trait;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::api as tail;
use wyrd_tonic::tonic::metadata::MetadataValue;
use wyrd_tonic::tonic::{Request, transport::Channel};
use wyrd_tonic::wyrd::v1::scribe_tail_service_client::ScribeTailServiceClient;

use crate::catalog::TenantTableBinding;
use crate::catalog::layout::TimePartition;
use crate::contracts::ScribeError;
use crate::scribe::memtable::{Memtable, ReadableBatchLimits};
use crate::scribe::routing::shard_for;
use crate::scribe::shards::ScribeShardRuntime;
use crate::scribe::stream_identity::StreamIdentity;

/// The only tail protocol revision understood by the Scribe v1 reader.
pub const TAIL_PROTOCOL_VERSION: u16 = 1;

/// Operation audience carried by a private Scribe-tail ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailTicketAudience {
    /// Metadata-only active-stream discovery.
    List,
}

/// Transport-neutral claims signed by the server tail authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailTicketClaims {
    /// Query identity owning the private operation.
    pub query_id: uuid::Uuid,
    /// Tenant bound by the authenticated query context.
    pub tenant_id: DataTenantId,
    /// Canonical tenant table name.
    pub canonical_table: String,
    /// Exact Scribe node and writer epoch selected by discovery.
    pub node_id: uuid::Uuid,
    /// Current writer epoch for stale-incarnation rejection.
    pub writer_epoch: u64,
    /// Absolute query deadline in UTC.
    pub deadline: chrono::DateTime<chrono::Utc>,
    /// Narrow operation audience.
    pub audience: TailTicketAudience,
    /// Single-use replay identity retained through expiry.
    pub nonce: Vec<u8>,
}

impl TailTicketClaims {
    /// Validates the signed claims against one operation's exact request tuple.
    ///
    /// # Errors
    /// Returns [`TailReadError::Authorization`] when query, tenant, table,
    /// stream, epoch, or deadline differs from the signed request.
    pub fn validate_binding(
        &self,
        query_id: uuid::Uuid,
        tenant_id: DataTenantId,
        canonical_table: &str,
        node_id: uuid::Uuid,
        writer_epoch: u64,
        deadline: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), TailReadError> {
        if self.query_id != query_id
            || self.tenant_id != tenant_id
            || self.canonical_table != canonical_table
            || self.node_id != node_id
            || self.writer_epoch != writer_epoch
            || self.deadline < deadline
        {
            return Err(TailReadError::Authorization {
                detail: "tail ticket binding is invalid".to_owned(),
            });
        }
        Ok(())
    }
}

/// Exact request tuple a signed tail ticket must authorize.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailTicketBinding {
    /// Query identity selected by Oracle.
    pub query_id: uuid::Uuid,
    /// Tenant identity selected by the authenticated request.
    pub tenant_id: DataTenantId,
    /// Canonical table selected by the catalog cut.
    pub canonical_table: String,
    /// Scribe node selected by discovery.
    pub node_id: uuid::Uuid,
    /// Writer epoch selected by discovery.
    pub writer_epoch: u64,
    /// Request deadline the ticket must cover.
    pub deadline: chrono::DateTime<chrono::Utc>,
}

/// Narrow signer for private active-stream discovery tickets.
pub trait TailTicketMinter: Send + Sync {
    /// Signs one operation-scoped claim set.
    ///
    /// # Errors
    /// Returns a closed authority error when claims cannot be encoded or signed.
    fn mint_tail_ticket(&self, claims: &TailTicketClaims) -> Result<Vec<u8>, TailReadError>;
}

/// Durable audit collaborator required before returning a tail authorization
/// rejection.  The server maps these narrow reasons to its audit catalog.
#[async_trait]
pub trait TailSecurityAudit: Send + Sync {
    /// Records a failure whose tenant claim is not trusted.
    ///
    /// # Errors
    /// Returns [`TailReadError`] when the durable audit append is unavailable.
    async fn append_unverified_tail_rejection(&self, reason: &str) -> Result<(), TailReadError>;

    /// Records a failure after the signed tenant claim is verified.
    ///
    /// # Errors
    /// Returns [`TailReadError`] when the durable audit append is unavailable.
    async fn append_verified_tail_violation(
        &self,
        tenant_id: DataTenantId,
        reason: &str,
    ) -> Result<(), TailReadError>;
}

/// No-op audit used only by isolated transport tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopTailSecurityAudit;

#[async_trait]
impl TailSecurityAudit for NoopTailSecurityAudit {
    /// Accepts the isolated unverified rejection.
    ///
    /// # Errors
    /// This test-only sink never fails.
    async fn append_unverified_tail_rejection(&self, _reason: &str) -> Result<(), TailReadError> {
        Ok(())
    }

    /// Accepts the isolated tenant violation.
    ///
    /// # Errors
    /// This test-only sink never fails.
    async fn append_verified_tail_violation(
        &self,
        _tenant_id: DataTenantId,
        _reason: &str,
    ) -> Result<(), TailReadError> {
        Ok(())
    }
}

/// Narrow verifier for private active-stream discovery tickets.
#[async_trait]
pub trait TailTicketVerifier: Send + Sync {
    /// Records a rejection whose signed tenant tuple is not trusted.
    ///
    /// # Errors
    /// Returns [`TailReadError`] when durable audit cannot append the rejection.
    async fn audit_unverified_rejection(&self, _reason: &str) -> Result<(), TailReadError> {
        Ok(())
    }

    /// Records a rejection after a signed tenant tuple has been decoded.
    ///
    /// # Errors
    /// Returns [`TailReadError`] when durable audit cannot append the rejection.
    async fn audit_verified_violation(
        &self,
        _tenant_id: DataTenantId,
        _reason: &str,
    ) -> Result<(), TailReadError> {
        Ok(())
    }

    /// Validates one decoded ticket against the exact operation tuple.
    ///
    /// # Errors
    /// Returns [`TailReadError::Authorization`] for any tuple mismatch or
    /// [`TailReadError`] when the rejection audit fails.
    async fn verify_tail_ticket_binding(
        &self,
        claims: &TailTicketClaims,
        binding: &TailTicketBinding,
    ) -> Result<(), TailReadError> {
        claims.validate_binding(
            binding.query_id,
            binding.tenant_id,
            &binding.canonical_table,
            binding.node_id,
            binding.writer_epoch,
            binding.deadline,
        )
    }

    /// Verifies signature, key, expiry, audience, and consumes the nonce before
    /// the caller checks operation-specific bindings.
    async fn verify_tail_ticket_unbound(
        &self,
        _encoded: &[u8],
        _audience: TailTicketAudience,
    ) -> Result<TailTicketClaims, TailReadError> {
        Err(TailReadError::Authorization {
            detail: "tail authority does not expose unbound claims".to_owned(),
        })
    }

    /// Verifies claims before Scribe state is listed or mutated.
    ///
    /// # Errors
    /// Returns [`TailReadError`] for signature, audience, expiry, binding,
    /// epoch, replay, or audit failures.
    async fn verify_tail_ticket(
        &self,
        encoded: &[u8],
        expected: &TailTicketClaims,
    ) -> Result<(), TailReadError>;
}

/// One active partition scope returned by private Scribe discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveTailStream {
    /// Exact partition whose live rows remain in Scribe memory.
    pub time_partition: tail::TimePartitionWire,
    /// Exact Scribe stream incarnation serving that partition.
    pub stream: tail::TailStreamIdentity,
}

/// Private tonic client for authenticated active-stream discovery on one Scribe.
#[derive(Debug, Clone)]
pub struct TonicTailReadTransport {
    /// Cloneable tonic client over one configured private Scribe endpoint.
    client: ScribeTailServiceClient<Channel>,
    /// Already-issued service-workload bearer sent on every private RPC.
    access_token: MetadataValue<wyrd_tonic::tonic::metadata::Ascii>,
}

impl TonicTailReadTransport {
    /// Discovers active event-day scopes through the private Scribe RPC.
    ///
    /// # Errors
    /// Returns [`TailReadError::State`] for transport, conversion, or server
    /// status failures.
    pub async fn list_active_streams(
        &self,
        binding: tail::TenantTableBinding,
        query_id: uuid::Uuid,
        ticket: Vec<u8>,
    ) -> Result<Vec<ActiveTailStream>, TailReadError> {
        let mut client = self.client.clone();
        let response = client
            .list_active_streams(self.authenticated_request(
                wyrd_tonic::wyrd::v1::ListActiveStreamsRequest {
                    tail_ticket: ticket,
                    binding: Some(binding.into()),
                    query_id: query_id.as_bytes().to_vec(),
                },
            ))
            .await
            .map_err(|status| tonic_error(&status))?;
        response
            .into_inner()
            .streams
            .into_iter()
            .map(|stream| {
                let time_partition: tail::TimePartitionWire = stream
                    .time_partition
                    .ok_or(TailReadError::Binding)?
                    .try_into()
                    .map_err(|_| TailReadError::Binding)?;
                let stream = stream.stream.ok_or_else(|| TailReadError::State {
                    detail: "active tail stream omitted identity".to_owned(),
                })?;
                let node_id =
                    uuid::Uuid::parse_str(&stream.node_id).map_err(|_| TailReadError::State {
                        detail: "active tail stream node id is invalid".to_owned(),
                    })?;
                Ok(ActiveTailStream {
                    time_partition,
                    stream: tail::TailStreamIdentity {
                        node_id: tail::NodeId::new(node_id),
                        writer_epoch: stream.writer_epoch,
                    },
                })
            })
            .collect()
    }

    /// Creates a remote transport using the authenticated private Scribe channel.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] when the bearer cannot be represented as
    /// gRPC metadata.
    pub fn new(
        client: ScribeTailServiceClient<Channel>,
        bearer: &str,
    ) -> Result<Self, TailReadError> {
        let access_token =
            MetadataValue::try_from(format!("Bearer {bearer}").as_str()).map_err(|_| {
                TailReadError::State {
                    detail: "tail bearer cannot be represented as gRPC metadata".to_owned(),
                }
            })?;
        Ok(Self {
            client,
            access_token,
        })
    }

    /// Adds the required private workload credential before a remote lookup.
    fn authenticated_request<T>(&self, message: T) -> Request<T> {
        let mut request = Request::new(message);
        request
            .metadata_mut()
            .insert("x-wyrd-access-token", self.access_token.clone());
        request
    }
}

/// Converts an authenticated remote status into the local reader error shape.
fn tonic_error(status: &wyrd_tonic::tonic::Status) -> TailReadError {
    TailReadError::State {
        detail: status.to_string(),
    }
}

/// Errors local to live-tail discovery before tonic maps them to statuses.
#[derive(Debug, thiserror::Error)]
pub enum TailReadError {
    /// A private ticket failed cryptographic or tuple validation.
    #[error("tail authorization failed: {detail}")]
    Authorization { detail: String },
    /// The query deadline elapsed before discovery completed.
    #[error("tail discovery deadline has elapsed")]
    DeadlineElapsed,
    /// The provided binding cannot map to this Scribe's logical table owner.
    #[error("invalid tail binding")]
    Binding,
    /// Scribe state could not produce a consistent shallow snapshot.
    #[error("tail Scribe state failed: {detail}")]
    State { detail: String },
}

/// Converts the private wire binding into the established local table owner.
///
/// # Errors
///
/// Returns [`TailReadError::Binding`] when the namespace, table, or tenant cannot
/// form one valid local tenant/table binding.
fn binding_from_wire(
    binding: &tail::TenantTableBinding,
) -> Result<TenantTableBinding, TailReadError> {
    let namespace = crate::namespaces::BifrostNamespace::from_domain_namespace(&binding.namespace)
        .ok_or(TailReadError::Binding)?;
    TenantTableBinding::resolve((
        binding.tenant_id,
        crate::catalog::TableRef::new(namespace, &binding.table),
    ))
    .map_err(|_| TailReadError::Binding)
}

/// One-shot pause held inside Scribe live production before a later batch exists.
///
/// A journey needs to prove Oracle received an earlier live batch while the
/// Scribe has not yet produced the next one, and that the open read survives a
/// long wait. Arming this stops exactly one live producer at that point until
/// the test releases it.
#[cfg(feature = "test-support")]
#[derive(Debug, Default)]
pub struct ScribeLiveProductionPause {
    /// Whether one producer should still be stopped.
    armed: std::sync::atomic::AtomicBool,
    /// Whether a producer has reached the pause.
    entered: std::sync::atomic::AtomicBool,
    /// Wakes a waiter once a producer reaches the pause.
    entered_notify: tokio::sync::Notify,
    /// Wakes the paused producer once the test releases it.
    release_notify: tokio::sync::Notify,
}

#[cfg(feature = "test-support")]
impl ScribeLiveProductionPause {
    /// Arms the pause for the next live producer that reaches it.
    pub fn arm(&self) {
        self.entered
            .store(false, std::sync::atomic::Ordering::Release);
        self.armed.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Waits until a producer is stopped on the pause.
    pub async fn wait_entered(&self) {
        loop {
            // Registered before the check so an entry between the check and
            // the await is not missed.
            let entered = self.entered_notify.notified();
            if self.entered.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            entered.await;
        }
    }

    /// Releases the paused producer and disarms the pause.
    pub fn release(&self) {
        self.armed
            .store(false, std::sync::atomic::Ordering::Release);
        self.release_notify.notify_waiters();
    }

    /// Stops one armed producer here, consuming the arming exactly once.
    ///
    /// Unarmed producers pass straight through.
    async fn hold(&self) {
        if !self.armed.swap(false, std::sync::atomic::Ordering::AcqRel) {
            return;
        }
        let released = self.release_notify.notified();
        tokio::pin!(released);
        self.entered
            .store(true, std::sync::atomic::Ordering::Release);
        self.entered_notify.notify_waiters();
        released.await;
    }
}

/// Process-wide live-production pause shared by the harness and producers.
#[cfg(feature = "test-support")]
static SCRIBE_LIVE_PRODUCTION_PAUSE: std::sync::OnceLock<
    std::sync::Arc<ScribeLiveProductionPause>,
> = std::sync::OnceLock::new();

/// Returns the process-wide Scribe live-production pause.
#[cfg(feature = "test-support")]
#[must_use]
pub fn scribe_live_production_pause_for_test() -> std::sync::Arc<ScribeLiveProductionPause> {
    std::sync::Arc::clone(
        SCRIBE_LIVE_PRODUCTION_PAUSE
            .get_or_init(|| std::sync::Arc::new(ScribeLiveProductionPause::default())),
    )
}

/// Live producers currently open in this process, across every Scribe.
#[cfg(feature = "test-support")]
static OPEN_LIVE_PRODUCERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Returns how many Scribe live producers, with their snapshot references,
/// are still open in this process.
#[cfg(feature = "test-support")]
#[must_use]
pub fn open_live_producers_for_test() -> usize {
    OPEN_LIVE_PRODUCERS.load(std::sync::atomic::Ordering::Acquire)
}

/// Counts one open live producer for the test harness until it drops.
///
/// Zero-sized and inert unless `test-support` is enabled.
#[derive(Debug)]
struct OpenLiveProducer;

impl OpenLiveProducer {
    /// Records one newly opened producer.
    fn open() -> Self {
        #[cfg(feature = "test-support")]
        OPEN_LIVE_PRODUCERS.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        Self
    }
}

#[cfg(feature = "test-support")]
impl Drop for OpenLiveProducer {
    /// Records that the producer and its snapshot references were released.
    fn drop(&mut self) {
        OPEN_LIVE_PRODUCERS.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

/// Exact, bounded hot-read request handed from Oracle to Scribe.
#[derive(Debug, Clone)]
pub struct FetchLiveTailRequest {
    /// Authenticated tenant/table binding resolved by Oracle.
    pub binding: TenantTableBinding,
    /// The writer stream the caller believes it is talking to.
    pub target_stream: StreamIdentity,
    /// Inclusive first partition day governed by the query.
    pub start_partition: TimePartition,
    /// Inclusive last partition day governed by the query.
    pub end_partition: TimePartition,
    /// Columns required by Oracle filters, ordering, tripwire, and projection.
    pub required_columns: Vec<String>,
    /// Signed closed predicates the assignment authorized for this scan.
    ///
    /// Scribe applies this conjunction to the assembled snapshot before
    /// returning it, so a selective query ships only matching rows back to
    /// the follower instead of the whole live tail.
    pub predicates: Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
    /// Maximum shallow Arrow batches materialized by the snapshot.
    pub max_batches: usize,
    /// Maximum source-derived Arrow bytes retained by the snapshot.
    pub max_retained_bytes: usize,
}

impl FetchLiveTailRequest {
    /// Return any consistent shard index for attribution purposes.
    ///
    /// Under batch-spread routing the live-tail data for one (tenant, table)
    /// may be spread across multiple shard lanes, so this value is used only
    /// for approximate attribution and diagnostics — not for dispatch. Use
    /// `ScribeShardRuntime::snapshot` for the
    /// fan-out that merges results across all shards.
    #[must_use]
    pub fn shard_id(&self) -> usize {
        // Use a stable zero-UUID as the batch_id placeholder so the
        // attribution-only shard is deterministic across calls on the same
        // request. The fan-out dispatch in ScribeShardRuntime::snapshot is the
        // authoritative multi-shard read path.
        shard_for(
            self.binding.tenant,
            &self.binding.table_ref,
            uuid::Uuid::nil(),
        )
    }
}

/// One shallow, structural hot snapshot returned by a shard owner.
#[derive(Debug, Clone)]
pub struct HotBatch {
    /// Exact partition day owning the batch.
    pub partition_day: TimePartition,
    /// Immutable generation holding the rows, or `None` while they are still
    /// in a writable bucket.
    ///
    /// A live read skips the staged runs of every generation its memtable
    /// snapshot already served, so each generation reaches a reader once.
    pub generation: Option<crate::scribe::hot_source::GenerationOrdinal>,
    /// Arrow rows projected to the request's required columns.
    pub rows: arrow::record_batch::RecordBatch,
}

/// One opened Scribe live read whose batches are produced only when pulled.
///
/// Opening takes the bounded shallow memtable snapshot and leases the staged
/// members not already served by it; nothing is filtered, decoded, or copied
/// until the consumer asks for the next batch. The snapshot references and the
/// staged lease live exactly as long as this value or the stream built from
/// it, so dropping the stream — at completion, cancellation, or disconnect —
/// releases them together, and an already-leased staged run stays readable
/// even if publication retires its authority meanwhile.
#[derive(Debug)]
pub struct LiveTailBatches {
    /// Shallow memtable batches in acknowledgement order, not yet filtered.
    memtable: Vec<HotBatch>,
    /// Lease keeping every staged run below readable until release.
    staged: Option<crate::scribe::hot_source::StagedSourceLease>,
    /// Leased staged members the memtable cut did not already serve.
    unserved: Vec<crate::scribe::hot_source::StagedSource>,
    /// Signed projection closure, in caller order.
    required_columns: Vec<String>,
    /// Signed predicate conjunction every produced row must satisfy.
    predicates: Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
    /// Test-harness accounting of this open producer.
    open: OpenLiveProducer,
}

impl LiveTailBatches {
    /// Wraps an already captured memtable snapshot with no staged members.
    ///
    /// Used by resolver fixtures that supply their own cohort.
    #[must_use]
    pub fn from_snapshot(
        memtable: Vec<HotBatch>,
        predicates: Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
    ) -> Self {
        Self {
            memtable,
            staged: None,
            unserved: Vec::new(),
            required_columns: Vec::new(),
            predicates,
            open: OpenLiveProducer::open(),
        }
    }

    /// Produces this read's rows one batch per pull.
    ///
    /// Memtable batches come first, then each unserved staged run decoded one
    /// bounded window at a time; each batch is filtered by the signed
    /// predicates just before it is yielded and batches retaining no row are
    /// skipped. Because the next batch is produced only when the consumer polls
    /// again, a slow consumer holds production back rather than accumulating
    /// output here.
    ///
    /// # Errors
    ///
    /// The stream yields [`ScribeError::Internal`] when a signed predicate
    /// cannot be compiled or evaluated, or a staged run cannot be opened,
    /// projected, or decoded; it ends after the first error.
    pub fn into_stream(
        self,
    ) -> futures_util::stream::BoxStream<
        'static,
        Result<arrow::record_batch::RecordBatch, ScribeError>,
    > {
        let Self {
            memtable,
            staged,
            unserved,
            required_columns,
            predicates,
            open,
        } = self;
        Box::pin(async_stream::try_stream! {
            let _open = open;
            let _staged = staged;
            let mut produced = false;
            for batch in memtable {
                #[cfg(feature = "test-support")]
                if produced {
                    scribe_live_production_pause_for_test().hold().await;
                }
                let rows = FetchLiveTailService::retain_signed(batch.rows, &predicates)?;
                if rows.num_rows() == 0 {
                    continue;
                }
                produced = true;
                yield rows;
            }
            let reader = crate::scribe::staged_tail::StagedTailReader::default();
            for source in &unserved {
                for run in &source.runs {
                    for window in reader.run_batches(run, &required_columns, &predicates)? {
                        #[cfg(feature = "test-support")]
                        if produced {
                            scribe_live_production_pause_for_test().hold().await;
                        }
                        let rows = window?;
                        produced = true;
                        yield rows;
                    }
                }
            }
        })
    }
}

/// Pod-local live-tail service over the Scribe memtable.
#[derive(Debug)]
pub struct FetchLiveTailService {
    /// Exact pod-local WAL stream incarnation served by this source.
    stream: StreamIdentity,
    /// Direct in-process memtable used only by narrow fixtures.
    memtable: Option<Arc<Memtable>>,
    /// Pod-wide authority registry naming which staged members serve rows.
    ///
    /// `None` for the direct-memtable fixtures, which have no staged boundary:
    /// their generations never leave memory.
    hot_sources: Option<Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>>,
    /// Production shard runtime that owns live generation state.
    shards: Option<Arc<ScribeShardRuntime>>,
}

impl FetchLiveTailService {
    /// Construct a reader over a direct in-process memtable.
    ///
    /// The direct-memtable branch backs the narrow in-process adapter used by
    /// unit tests; production readers submit shard snapshots through
    /// `FetchLiveTailService::with_runtime`.
    #[must_use]
    pub fn new(stream: StreamIdentity, memtable: Arc<crate::scribe::memtable::Memtable>) -> Self {
        Self {
            stream,
            hot_sources: None,
            memtable: Some(memtable),
            shards: None,
        }
    }

    /// Construct a production reader that submits snapshots to the owning
    /// shard command queue instead of traversing Scribe state directly.
    #[must_use]
    pub(crate) fn with_runtime(
        stream: StreamIdentity,
        shards: Arc<ScribeShardRuntime>,
        hot_sources: Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>,
    ) -> Self {
        Self {
            stream,
            hot_sources: Some(hot_sources),
            memtable: None,
            shards: Some(shards),
        }
    }

    /// Binds a direct-memtable reader to a hot-source authority registry.
    ///
    /// Test fixtures use this to exercise the staged-run half of a live-tail
    /// read without standing up the shard runtime.
    #[cfg(test)]
    #[must_use]
    fn with_hot_sources(
        mut self,
        hot_sources: Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>,
    ) -> Self {
        self.hot_sources = Some(hot_sources);
        self
    }

    /// Stream identity this service serves.
    #[must_use]
    pub fn stream(&self) -> StreamIdentity {
        self.stream
    }

    /// Lists active event-day scopes for one authenticated tenant/table.
    ///
    /// This is intentionally metadata-only: it returns the partitions this
    /// stream still serves live, never rows. Oracle plans a live leaf per
    /// returned partition and reads it through [`Self::open_live_batches`].
    ///
    /// # Errors
    /// Returns [`TailReadError::Binding`] for an invalid wire binding and
    /// [`TailReadError::State`] when the Scribe snapshot cannot be read.
    pub fn list_active_streams(
        &self,
        binding: &tail::TenantTableBinding,
    ) -> Result<Vec<(tail::TimePartitionWire, tail::TailStreamIdentity)>, TailReadError> {
        let binding = binding_from_wire(binding)?;
        let keys = self
            .active_seal_keys_for_tenant(binding.tenant)
            .map_err(|error| TailReadError::State {
                detail: error.to_string(),
            })?;
        tracing::debug!(
            tenant = %binding.tenant,
            table = %binding.table_ref,
            active_key_count = keys.len(),
            active_keys = ?keys,
            "listed active Scribe tail keys"
        );
        let stream = self.stream;
        let writer_epoch =
            u64::try_from(stream.writer_epoch.as_i64()).map_err(|_| TailReadError::State {
                detail: "Scribe writer epoch is negative".to_owned(),
            })?;
        let mut partitions = keys
            .into_iter()
            .filter(|key| key.table == binding.table_ref)
            .map(|key| key.partition)
            .collect::<Vec<_>>();
        partitions.sort();
        partitions.dedup();
        Ok(partitions
            .into_iter()
            .map(TimePartition::to_wire)
            .map(|partition| {
                (
                    partition,
                    tail::TailStreamIdentity {
                        node_id: tail::NodeId::new(stream.node_id.as_uuid()),
                        writer_epoch,
                    },
                )
            })
            .collect())
    }

    /// Lists the tenant-owned table/day scopes that still have live Scribe
    /// state.  The list is a point-in-time discovery cut; retirement is driven
    /// by the durable file-list commit and therefore naturally removes a key
    /// from subsequent cuts.
    ///
    /// # Errors
    /// Returns [`ScribeError`] when a shard inspection snapshot or test
    /// memtable lock cannot be read.
    pub fn active_seal_keys_for_tenant(
        &self,
        tenant: DataTenantId,
    ) -> Result<Vec<crate::scribe::seal_key::SealKey>, ScribeError> {
        let mut keys = if let Some(memtable) = &self.memtable {
            memtable.seal_keys_for_tenant(tenant)?
        } else {
            self.shards
                .as_ref()
                .ok_or_else(|| ScribeError::Internal {
                    detail: "tail source has no shard runtime".to_owned(),
                })?
                .active_seal_keys_for_tenant(tenant)?
        };
        if let Some(hot_sources) = &self.hot_sources {
            keys.extend(
                hot_sources
                    .staged_seal_keys_for_tenant(tenant)
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("discover staged live-tail keys: {error}"),
                    })?,
            );
        }
        keys.sort_by_key(ToString::to_string);
        keys.dedup();
        Ok(keys)
    }

    /// Return the canonical pod-local shard for a live-tail scope.
    #[must_use]
    pub fn shard_id(&self, request: &FetchLiveTailRequest) -> usize {
        request.shard_id()
    }

    /// Opens one lazily produced live read over this Scribe's stream.
    ///
    /// Validation, the bounded memtable snapshot, and the staged lease happen
    /// here; filtering and staged decoding happen only as the returned
    /// producer is pulled.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when stream/range validation fails, the bounded
    /// snapshot exceeds its count or retained-byte ceiling, the owning
    /// memtable/shard cannot produce the projection, or the staged registry is
    /// unavailable.
    pub async fn open_live_batches(
        &self,
        request: FetchLiveTailRequest,
    ) -> Result<LiveTailBatches, ScribeError> {
        let staged_request = request.clone();
        let memtable = self.memtable_batches(request).await?;
        let served = Self::served_generations(&memtable);
        let staged = self.staged_lease(&staged_request)?;
        let unserved = staged
            .as_ref()
            .map(|lease| {
                lease
                    .sources()
                    .iter()
                    .filter(|source| !served.contains(&(source.key.partition, source.generation)))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        Ok(LiveTailBatches {
            memtable,
            staged,
            unserved,
            required_columns: staged_request.required_columns,
            predicates: staged_request.predicates,
            open: OpenLiveProducer::open(),
        })
    }

    /// Validates one request and takes its bounded shallow memtable snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the request names another stream or an
    /// inverted range, the snapshot exceeds its count or retained-byte ceiling,
    /// or the owning memtable/shard cannot produce the projection.
    async fn memtable_batches(
        &self,
        request: FetchLiveTailRequest,
    ) -> Result<Vec<HotBatch>, ScribeError> {
        if request.target_stream != self.stream {
            return Err(ScribeError::StreamMismatch {
                requested: request.target_stream,
                actual: self.stream,
            });
        }
        if request.start_partition > request.end_partition {
            return Err(ScribeError::Internal {
                detail: "live-tail start day is after end day".to_owned(),
            });
        }
        if let Some(shards) = &self.shards {
            return shards.snapshot(request).await;
        }
        Ok(self
            .memtable
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "direct tail memtable is not configured".to_owned(),
            })?
            .readable_batches_for_range(
                request.binding.tenant,
                &request.binding.table_ref,
                request.start_partition,
                request.end_partition,
                &request.required_columns,
                ReadableBatchLimits {
                    max_batches: request.max_batches,
                    max_retained_bytes: request.max_retained_bytes,
                },
            )?
            .into_iter()
            .map(|readable| HotBatch {
                partition_day: readable.partition_day,
                generation: readable.generation,
                rows: readable.batch,
            })
            .collect())
    }

    /// Names the immutable generations a memtable snapshot already served.
    ///
    /// Their staged runs must be skipped so each generation reaches a reader
    /// once.
    fn served_generations(
        batches: &[HotBatch],
    ) -> std::collections::HashSet<(TimePartition, crate::scribe::hot_source::GenerationOrdinal)>
    {
        batches
            .iter()
            .filter_map(|batch| {
                batch
                    .generation
                    .map(|generation| (batch.partition_day, generation))
            })
            .collect()
    }

    /// Leases the staged members that serve this request's range.
    ///
    /// A generation whose Arrow was released after staging is invisible to the
    /// shard snapshot, so without this a live-tail reader would see a gap
    /// between staging and publication. The registry hands back only the
    /// generations it still holds staged authority for, so a generation a
    /// published object already serves is absent rather than filtered out.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the registry is unavailable.
    fn staged_lease(
        &self,
        request: &FetchLiveTailRequest,
    ) -> Result<Option<crate::scribe::hot_source::StagedSourceLease>, ScribeError> {
        let Some(hot_sources) = &self.hot_sources else {
            return Ok(None);
        };
        hot_sources
            .staged_sources(
                request.binding.tenant,
                &request.binding.table_ref,
                request.start_partition,
                request.end_partition,
            )
            .map(Some)
            .map_err(|error| ScribeError::Internal {
                detail: format!("resolve the staged members serving a live-tail read: {error}"),
            })
    }

    /// Keeps only the rows of one batch the signed predicate conjunction admits.
    ///
    /// An empty conjunction admits every row and returns the batch unchanged.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] when a signed predicate cannot be
    /// compiled against, or evaluated over, the batch's own schema.
    fn retain_signed(
        rows: arrow::record_batch::RecordBatch,
        predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
    ) -> Result<arrow::record_batch::RecordBatch, ScribeError> {
        if predicates.is_empty() {
            return Ok(rows);
        }
        let filter = crate::oracle::exec::ScanPredicateFilter::compile(&rows.schema(), predicates)
            .map_err(|error| ScribeError::Internal {
                detail: format!("live-tail predicate is invalid for this snapshot: {error}"),
            })?;
        filter.retain(rows).map_err(|error| ScribeError::Internal {
            detail: format!("live-tail predicate evaluation failed: {error}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{FetchLiveTailService, TailReadError, TailTicketAudience, TailTicketClaims};
    use crate::scribe::memtable::Memtable;
    use crate::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
    use crate::scribe::wal::WalLsn;
    use arrow::array::{Int64Array, RecordBatch};
    use arrow::datatypes::{DataType, Field, Schema};
    use wyrd_spec::DataTenantId;

    /// Builds one direct live-tail service over a memtable holding two
    /// appended batches of `value` rows, so a selective fetch can be compared
    /// against the unfiltered one.
    ///
    /// # Panics
    /// Panics when the fixture batches, seal key, or memtable inserts violate
    /// their construction invariants.
    fn selective_tail_fixture(
        tenant: DataTenantId,
        day: crate::catalog::layout::TimePartition,
        stream: StreamIdentity,
    ) -> (FetchLiveTailService, super::TenantTableBinding) {
        use crate::catalog::TableRef;
        use crate::scribe::seal_key::SealKey;
        use crate::scribe::wal::ScribeAppendMeta;

        // The managed row ordinal is part of every persisted append, and a fence
        // cursor is derived from it, so the fixture carries it exactly as
        // ingress would. It is excluded from the projected source fingerprint.
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Int64, false),
            Field::new(
                wyrd_spec::vala::managed_columns::WYRD_ROW_ORDINAL,
                DataType::Int32,
                false,
            ),
        ]));
        let batch = |values: Vec<i64>| {
            let ordinals = (0..i32::try_from(values.len()).expect("fixture row count fits i32"))
                .collect::<Vec<_>>();
            RecordBatch::try_new(
                Arc::clone(&schema),
                vec![
                    Arc::new(Int64Array::from(values)),
                    Arc::new(arrow::array::Int32Array::from(ordinals)),
                ],
            )
            .expect("valid fixture batch")
        };
        let table = TableRef::new(crate::namespaces::BifrostNamespace::Bifrost, "events");
        let key = SealKey::new(tenant, table.clone(), day);
        let memtable = Arc::new(Memtable::new());
        let meta = |lsn: u64, rows: usize| ScribeAppendMeta {
            batch_id: *uuid::Uuid::now_v7().as_bytes(),
            schema_fingerprint: [0; 32],
            data_digest: [0; 32],
            data_len: 0,
            payload_digest: [0; 32],
            payload_len: 0,
            slice_index: 0,
            slice_count: 1,
            rows_accepted: rows,
            wal_lsn_min: WalLsn::new(lsn),
            wal_lsn_max: WalLsn::new(lsn),
            seal_key: key.to_string(),
        };
        memtable
            .insert(&key, meta(1, 3), batch(vec![1, 2, 3]))
            .expect("first fixture batch inserts");
        // Freezing between the appends is what makes this fixture cover both
        // source states a live tail must keep readable for its lease: the first
        // generation becomes immutable frozen Arrow, and the rows appended
        // after it stay in the active writable bucket.
        memtable
            .freeze(&key)
            .expect("first fixture generation freezes");
        memtable
            .insert(&key, meta(2, 2), batch(vec![4, 5]))
            .expect("second fixture batch inserts");
        let binding =
            super::TenantTableBinding::resolve((tenant, table)).expect("fixture binding resolves");
        (FetchLiveTailService::new(stream, memtable), binding)
    }

    /// Writes `rows` as one staged Parquet run under `directory`.
    ///
    /// # Panics
    /// Panics when the fixture cannot write its own run.
    fn write_staged_run(
        directory: &std::path::Path,
        schema: Arc<Schema>,
        rows: &RecordBatch,
    ) -> std::path::PathBuf {
        let run = directory.join("run-0.parquet");
        let mut writer = parquet::arrow::ArrowWriter::try_new(
            std::fs::File::create(&run).expect("staged run file"),
            schema,
            None,
        )
        .expect("staged run writer");
        writer.write(rows).expect("staged run rows");
        writer.close().expect("staged run footer");
        run
    }

    /// Opens one live read and drains every batch it produces.
    ///
    /// # Panics
    /// Panics when the read cannot open or any produced batch fails.
    async fn live_batches(
        service: &FetchLiveTailService,
        request: super::FetchLiveTailRequest,
    ) -> Vec<RecordBatch> {
        futures_util::TryStreamExt::try_collect(
            service
                .open_live_batches(request)
                .await
                .expect("live read opens")
                .into_stream(),
        )
        .await
        .expect("live read drains")
    }

    /// Returns every `value` a live read yields, sorted, so duplicates across
    /// sources stay visible.
    ///
    /// # Panics
    /// Panics when the read fails or the first column is not `Int64`.
    async fn hot_values(
        service: &FetchLiveTailService,
        request: super::FetchLiveTailRequest,
    ) -> Vec<i64> {
        let mut values = live_batches(service, request)
            .await
            .iter()
            .flat_map(|batch| {
                batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .expect("Int64 value column")
                    .values()
                    .to_vec()
            })
            .collect::<Vec<_>>();
        values.sort_unstable();
        values
    }

    /// One frozen generation whose rows are both in the memtable and in a
    /// registered staged run, served by a direct-memtable tail service.
    struct StagedGenerationFixture {
        /// Service reading the memtable and the registry.
        service: FetchLiveTailService,
        /// Memtable holding the frozen generation.
        memtable: Arc<crate::scribe::memtable::Memtable>,
        /// Registry naming the generation's staged runs.
        registry: Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>,
        /// Seal key of the generation.
        key: crate::scribe::seal_key::SealKey,
        /// The staged generation.
        generation: crate::scribe::hot_source::GenerationOrdinal,
        /// Staged member holding the runs.
        member: crate::scribe::assembly::StagedMemberId,
        /// Request reading the generation's day.
        request: super::FetchLiveTailRequest,
        /// Directory owning the staged run; the run is deleted when it drops.
        directory: tempfile::TempDir,
    }

    /// Builds a [`StagedGenerationFixture`] holding rows `[1, 2, 3]`.
    ///
    /// # Panics
    /// Panics when any fixture step fails.
    fn staged_generation_fixture() -> StagedGenerationFixture {
        use crate::catalog::TableRef;
        use crate::scribe::assembly::StagedMemberId;
        use crate::scribe::hot_source::{HotAuthority, ScribeHotSourceRegistry};
        use crate::scribe::memtable::Memtable;
        use crate::scribe::seal_key::SealKey;
        use crate::scribe::wal::ScribeAppendMeta;

        let tenant = DataTenantId::new_v7();
        let day = crate::test_support::day_partition(2026, 7, 14);
        let stream = StreamIdentity::new(NodeId::generate(), WriterEpoch::new(1));
        let table = TableRef::new(crate::namespaces::BifrostNamespace::Bifrost, "events");
        let key = SealKey::new(tenant, table.clone(), day);
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let rows = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![1_i64, 2, 3]))],
        )
        .expect("fixture batch");
        let registry = Arc::new(ScribeHotSourceRegistry::new());
        let memtable = Arc::new(Memtable::new().with_hot_sources(0, Arc::clone(&registry)));
        memtable
            .insert(
                &key,
                ScribeAppendMeta {
                    batch_id: *uuid::Uuid::now_v7().as_bytes(),
                    schema_fingerprint: [0; 32],
                    data_digest: [0; 32],
                    data_len: 0,
                    payload_digest: [0; 32],
                    payload_len: 0,
                    slice_index: 0,
                    slice_count: 1,
                    rows_accepted: 3,
                    wal_lsn_min: WalLsn::new(1),
                    wal_lsn_max: WalLsn::new(1),
                    seal_key: key.to_string(),
                },
                rows.clone(),
            )
            .expect("fixture batch inserts");
        memtable.freeze(&key).expect("fixture generation freezes");
        let [(generation, HotAuthority::Memtable)] = registry
            .live_generations(&key)
            .expect("registry is readable")[..]
        else {
            panic!("freezing registers exactly one memtable-authoritative generation");
        };

        let directory = tempfile::tempdir().expect("staged run directory");
        let run = write_staged_run(directory.path(), schema, &rows);
        let member = StagedMemberId::new(0, generation.get());
        registry
            .advance(
                &key,
                generation,
                HotAuthority::StagedRun {
                    member,
                    runs: vec![run],
                    bytes: 4_096,
                    wal: (WalLsn::new(1), WalLsn::new(1)),
                },
            )
            .expect("the generation moves to its staged runs");

        let service = FetchLiveTailService::new(stream, Arc::clone(&memtable))
            .with_hot_sources(Arc::clone(&registry));
        let binding =
            super::TenantTableBinding::resolve((tenant, table)).expect("fixture binding resolves");
        let request = super::FetchLiveTailRequest {
            binding,
            target_stream: stream,
            start_partition: day,
            end_partition: day,
            required_columns: vec!["value".to_owned()],
            predicates: Vec::new(),
            max_batches: 64,
            max_retained_bytes: 64 * 1024 * 1024,
        };
        StagedGenerationFixture {
            service,
            memtable,
            registry,
            key,
            generation,
            member,
            request,
            directory,
        }
    }

    /// A generation whose staged runs are already registered is read once.
    ///
    /// Staging advances a generation's registry authority to its staged runs
    /// before the owning shard learns the member is durable, so for a moment
    /// both the frozen Arrow and the runs hold the same rows. A live-tail read
    /// in that window, and one after the shard marks the generation durable,
    /// must each return every row exactly once.
    ///
    /// # Panics
    /// Panics when the fixture cannot be built or a read returns any row other
    /// than exactly once.
    #[tokio::test]
    async fn a_generation_staged_before_the_shard_settles_is_read_once() {
        let StagedGenerationFixture {
            service,
            memtable,
            generation,
            member,
            request,
            directory: _directory,
            ..
        } = staged_generation_fixture();
        let read = || hot_values(&service, request.clone());

        assert_eq!(read().await, vec![1, 2, 3], "staged but not yet durable");
        memtable
            .complete_staged(generation.get(), member)
            .expect("the shard marks the generation durable");
        assert_eq!(
            read().await,
            vec![1, 2, 3],
            "durable and served by its runs"
        );
    }

    /// A live read opened over durable staged runs keeps them readable after
    /// publication retires their authority, and holds its lease until its
    /// stream drops.
    ///
    /// # Panics
    /// Panics when the fixture cannot be built, the leased run cannot be read
    /// after publication, or the lease does not follow the stream.
    #[tokio::test]
    async fn an_open_live_read_keeps_staged_runs_across_publication() {
        use crate::scribe::hot_source::HotAuthority;

        let StagedGenerationFixture {
            service,
            memtable,
            registry,
            key,
            generation,
            member,
            request,
            directory: _directory,
            ..
        } = staged_generation_fixture();
        memtable
            .complete_staged(generation.get(), member)
            .expect("the shard marks the generation durable");
        let open = service
            .open_live_batches(request.clone())
            .await
            .expect("the live read opens");
        assert_eq!(registry.leases(&key, generation).expect("locked"), 1);
        registry
            .advance(
                &key,
                generation,
                HotAuthority::Published {
                    object_keys: vec!["hot/events/0001.parquet".to_owned()],
                },
            )
            .expect("the generation publishes");
        let mut stream = open.into_stream();
        let batch = futures_util::StreamExt::next(&mut stream)
            .await
            .expect("the staged run yields a batch")
            .expect("a leased staged run stays readable after publication");
        let values = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("Int64 value column")
            .values()
            .to_vec();
        assert_eq!(values, vec![1, 2, 3], "published after open, still read");
        assert_eq!(
            registry.leases(&key, generation).expect("locked"),
            1,
            "an open stream keeps its lease"
        );
        drop(stream);
        assert_eq!(
            registry.leases(&key, generation).expect("locked"),
            0,
            "dropping the stream releases the lease"
        );
    }

    /// A signed predicate is applied inside Scribe, so a selective live-tail
    /// fetch returns the same rows the leader would have kept but ships
    /// strictly fewer of them into follower attempt encoding.
    ///
    /// # Panics
    /// Panics when the fixture cannot be built or either snapshot fails.
    #[tokio::test]
    async fn selective_live_tail_fetch_returns_only_signed_rows() {
        use wyrd_spec::vala::assignment_authority::{ScanLiteral, ScanPredicate};

        let tenant = DataTenantId::new_v7();
        let day = crate::test_support::day_partition(2026, 7, 14);
        let stream = StreamIdentity::new(NodeId::generate(), WriterEpoch::new(1));
        let (service, binding) = selective_tail_fixture(tenant, day, stream);
        let request = |predicates: Vec<ScanPredicate>| super::FetchLiveTailRequest {
            binding: binding.clone(),
            target_stream: stream,
            start_partition: day,
            end_partition: day,
            required_columns: vec!["value".to_owned()],
            predicates,
            max_batches: 64,
            max_retained_bytes: 64 * 1024 * 1024,
        };

        let unfiltered = live_batches(&service, request(Vec::new())).await;
        let unfiltered_rows: usize = unfiltered.iter().map(RecordBatch::num_rows).sum();
        assert_eq!(unfiltered.len(), 2);
        assert_eq!(unfiltered_rows, 5);

        let selective = live_batches(
            &service,
            request(vec![ScanPredicate::Eq(
                "value".to_owned(),
                ScanLiteral::I64(2),
            )]),
        )
        .await;
        // The batch holding no matching row is dropped entirely rather than
        // returned empty, and the surviving batch carries only row `2`.
        assert_eq!(selective.len(), 1);
        let selective_rows: usize = selective.iter().map(RecordBatch::num_rows).sum();
        assert_eq!(selective_rows, 1);
        assert!(selective_rows < unfiltered_rows);
        let retained = selective[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("Int64 value column")
            .value(0);
        assert_eq!(retained, 2);
    }

    /// Rejects a signed ticket tuple when the caller changes the tenant binding.
    #[test]
    fn ticket_binding_rejects_cross_tenant_request() {
        let signed_tenant = DataTenantId::new_v7();
        let requested_tenant = DataTenantId::new_v7();
        let claims = TailTicketClaims {
            query_id: uuid::Uuid::new_v4(),
            tenant_id: signed_tenant,
            canonical_table: "vala.bifrost.events".to_owned(),
            node_id: uuid::Uuid::new_v4(),
            writer_epoch: 7,
            deadline: chrono::Utc::now() + chrono::Duration::seconds(5),
            audience: TailTicketAudience::List,
            nonce: vec![1; 16],
        };
        assert!(matches!(
            claims.validate_binding(
                claims.query_id,
                requested_tenant,
                &claims.canonical_table,
                claims.node_id,
                claims.writer_epoch,
                claims.deadline,
            ),
            Err(TailReadError::Authorization { .. })
        ));
    }
}
