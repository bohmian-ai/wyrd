//! Typed, pod-local live-tail reads over writable and immutable memtable data.
//!
//! The Scribe live source owns no WAL or SQL access. Oracle discovers the
//! partitions a Scribe serves through the mTLS private `ListActiveStreams`
//! RPC and reads them as lazily produced live batches.

use std::path::PathBuf;
use std::sync::Arc;

use arrow::record_batch::RecordBatch;
#[cfg(feature = "test-support")]
use datafusion::physical_plan::SendableRecordBatchStream;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::api as tail;
use wyrd_tonic::tonic::{Code, transport::Channel};
use wyrd_tonic::wyrd::v1::scribe_tail_service_client::ScribeTailServiceClient;

use crate::catalog::TenantTableBinding;
use crate::catalog::layout::TimePartition;
use crate::contracts::ScribeError;
use crate::scribe::hot_source::StagedSourceLease;
use crate::scribe::memtable::Memtable;
use crate::scribe::shards::ScribeShardRuntime;
use crate::scribe::stream_identity::StreamIdentity;

/// The only tail protocol revision understood by the Scribe v1 reader.
pub const TAIL_PROTOCOL_VERSION: u16 = 1;

/// One active partition scope returned by private Scribe discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveTailStream {
    /// Exact partition whose live rows remain in Scribe memory.
    pub time_partition: tail::TimePartitionWire,
    /// Exact Scribe stream incarnation serving that partition.
    pub stream: tail::TailStreamIdentity,
}

/// Private tonic client for active-stream discovery on one Scribe.
///
/// The channel is the mTLS peer channel; the peer certificate is the only
/// credential, so requests carry no per-call bearer.
#[derive(Debug, Clone)]
pub struct TonicTailReadTransport {
    /// Cloneable tonic client over one configured private Scribe endpoint.
    client: ScribeTailServiceClient<Channel>,
}

impl TonicTailReadTransport {
    /// Discovers active event-day scopes through the private Scribe RPC.
    ///
    /// # Errors
    /// Returns the [`TailReadError`] class of the server status (see
    /// [`tonic_error`]), [`TailReadError::Binding`] for an unreadable returned
    /// partition, and [`TailReadError::State`] for a malformed returned stream
    /// identity.
    pub async fn list_active_streams(
        &self,
        binding: tail::TenantTableBinding,
    ) -> Result<Vec<ActiveTailStream>, TailReadError> {
        let mut client = self.client.clone();
        let response = client
            .list_active_streams(wyrd_tonic::wyrd::v1::ListActiveStreamsRequest {
                binding: Some(binding.into()),
            })
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

    /// Creates a remote transport over the mTLS private Scribe channel.
    #[must_use]
    pub fn new(client: ScribeTailServiceClient<Channel>) -> Self {
        Self { client }
    }
}

/// Converts a remote listing status into its closed local failure class.
///
/// The class, not the message, is what Oracle's query terminal decides on:
/// only `Unavailable` is availability loss of the listed Scribe. A deadline
/// stays a deadline, credential and tenant refusals stay authorization, a
/// rejected binding stays a binding fault, and every other status, including
/// cancellation and internal Scribe state failure, is a fatal state fault.
fn tonic_error(status: &wyrd_tonic::tonic::Status) -> TailReadError {
    let detail = status.message().to_owned();
    match status.code() {
        Code::Unavailable => TailReadError::Unavailable { detail },
        Code::DeadlineExceeded => TailReadError::DeadlineElapsed,
        Code::Unauthenticated | Code::PermissionDenied => TailReadError::Authorization { detail },
        Code::InvalidArgument => TailReadError::Binding,
        _ => TailReadError::State {
            detail: status.to_string(),
        },
    }
}

/// Errors local to live-tail discovery before tonic maps them to statuses.
#[derive(Debug, thiserror::Error)]
pub enum TailReadError {
    /// The private caller credential or its tenant binding was refused.
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
    /// A ready Scribe could not be reached or was not serving the listing.
    ///
    /// This is the only listing failure Oracle treats as live-source loss.
    #[error("tail Scribe unavailable: {detail}")]
    Unavailable { detail: String },
    /// A listed stream names a different incarnation or writer epoch than the
    /// membership snapshot the listing was sent to.
    #[error("tail stream identity is stale")]
    StaleIdentity,
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

/// Batches every Scribe live producer in this process has yielded.
#[cfg(feature = "test-support")]
static LIVE_BATCHES_PRODUCED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Returns how many batches Scribe live producers in this process have yielded.
///
/// A journey reads it twice to tell a producer still yielding from one parked
/// on its consumer, such as a response blocked on transport flow control.
#[cfg(feature = "test-support")]
#[must_use]
pub fn live_batches_produced_for_test() -> u64 {
    LIVE_BATCHES_PRODUCED.load(std::sync::atomic::Ordering::Acquire)
}

/// Counts one open live producer for the test harness until it drops.
///
/// Compiled only with `test-support`; production producers carry no counter.
#[cfg(feature = "test-support")]
#[derive(Debug)]
struct OpenLiveProducer;

#[cfg(feature = "test-support")]
impl OpenLiveProducer {
    /// Records one newly opened producer.
    fn open() -> Self {
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
    /// Columns required by Oracle filters, ordering, and projection.
    pub required_columns: Vec<String>,
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

/// One opened Scribe live read: its bounded memtable cut and the staged runs
/// that cut did not already serve.
///
/// Opening takes the bounded shallow memtable snapshot and leases the staged
/// members not already served by it; nothing is filtered, decoded, or copied
/// here. The Scribe follower turns it into one `DataFusion` plan: the memtable
/// rows become an in-memory source, the unserved runs are read by Oracle's
/// hot-Parquet scan, and the lease moves into that scan so an already-leased
/// run stays readable even if publication retires its authority meanwhile.
#[derive(Debug)]
pub struct LiveTailBatches {
    /// Shallow memtable batches in acknowledgement order, not yet filtered.
    memtable: Vec<HotBatch>,
    /// Lease keeping every staged run below readable until release.
    staged: Option<crate::scribe::hot_source::StagedSourceLease>,
    /// Leased staged members the memtable cut did not already serve.
    unserved: Vec<crate::scribe::hot_source::StagedSource>,
}

impl LiveTailBatches {
    /// Wraps an already captured memtable snapshot with no staged members.
    ///
    /// Used by resolver fixtures that supply their own cohort.
    #[must_use]
    pub fn from_snapshot(memtable: Vec<HotBatch>) -> Self {
        Self {
            memtable,
            staged: None,
            unserved: Vec::new(),
        }
    }

    /// Splits the read into its memtable rows, the unserved staged run paths
    /// in merge order, and the lease keeping those runs on disk.
    ///
    /// The caller must hold the lease for as long as it reads any run path.
    #[must_use]
    pub(crate) fn into_parts(self) -> (Vec<RecordBatch>, Vec<PathBuf>, Option<StagedSourceLease>) {
        let rows = self.memtable.into_iter().map(|batch| batch.rows).collect();
        let runs = self
            .unserved
            .into_iter()
            .flat_map(|source| source.runs)
            .collect();
        (rows, runs, self.staged)
    }
}

/// Wraps one Scribe fragment's output with the test harness's live-production
/// hooks.
///
/// The returned stream counts as one open live producer until it drops,
/// counts every batch it yields, and holds an armed [`ScribeLiveProductionPause`] before every batch after the
/// first, so a journey can observe Oracle holding an earlier batch while the
/// Scribe has not yet produced the next. Compiled only with `test-support`.
#[cfg(feature = "test-support")]
#[must_use]
pub(crate) fn observe_live_production_for_test(
    stream: SendableRecordBatchStream,
) -> SendableRecordBatchStream {
    let schema = stream.schema();
    let open = OpenLiveProducer::open();
    let observed = async_stream::stream! {
        let _open = open;
        let mut stream = stream;
        let mut produced = false;
        while let Some(batch) = futures_util::StreamExt::next(&mut stream).await {
            if produced {
                scribe_live_production_pause_for_test().hold().await;
            }
            produced = true;
            LIVE_BATCHES_PRODUCED.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            yield batch;
        }
    };
    Box::pin(datafusion::physical_plan::stream::RecordBatchStreamAdapter::new(schema, observed))
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
        let partitions = self
            .active_partitions_for_table(binding.tenant, &binding.table_ref)
            .map_err(|error| TailReadError::State {
                detail: error.to_string(),
            })?;
        tracing::debug!(
            tenant = %binding.tenant,
            table = %binding.table_ref,
            active_partition_count = partitions.len(),
            "listed active Scribe tail partitions"
        );
        let stream = self.stream;
        let writer_epoch =
            u64::try_from(stream.writer_epoch.as_i64()).map_err(|_| TailReadError::State {
                detail: "Scribe writer epoch is negative".to_owned(),
            })?;
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

    /// Lists the partitions of one tenant table that still have live Scribe
    /// state, sorted and deduplicated.
    ///
    /// Writable, pending immutable, and staged keys are all scanned for `table`
    /// alone. The list is a point-in-time discovery cut; retirement is driven
    /// by the durable file-list commit and therefore naturally removes a
    /// partition from subsequent cuts.
    ///
    /// # Errors
    /// Returns [`ScribeError`] when a shard inspection snapshot, memtable lock,
    /// or staged registry lock cannot be read.
    pub fn active_partitions_for_table(
        &self,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
    ) -> Result<Vec<TimePartition>, ScribeError> {
        let mut keys = if let Some(memtable) = &self.memtable {
            memtable.seal_keys_for_table(tenant, table)?
        } else {
            self.shards
                .as_ref()
                .ok_or_else(|| ScribeError::Internal {
                    detail: "tail source has no shard runtime".to_owned(),
                })?
                .active_seal_keys_for_table(tenant, table)?
        };
        if let Some(hot_sources) = &self.hot_sources {
            keys.extend(
                hot_sources
                    .staged_seal_keys_for_table(tenant, table)
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("discover staged live-tail keys: {error}"),
                    })?,
            );
        }
        let mut partitions = keys
            .into_iter()
            .map(|key| key.partition)
            .collect::<Vec<_>>();
        partitions.sort();
        partitions.dedup();
        Ok(partitions)
    }

    /// Opens one lazily produced live read over this Scribe's stream.
    ///
    /// Validation, the bounded memtable snapshot, and the staged lease happen
    /// here; filtering and staged decoding happen only in the plan the Scribe
    /// follower builds from the returned read.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when stream/range validation fails, a shard
    /// mailbox is full, the owning memtable/shard cannot produce the projection, or the staged registry is
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
        })
    }

    /// Validates one request and takes its bounded shallow memtable snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the request names another stream or an
    /// inverted range, a shard mailbox is full, or the owning memtable/shard cannot produce the projection.
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
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use super::{FetchLiveTailService, TailReadError, tonic_error};
    use crate::scribe::hot_source::HotAuthority;
    use crate::scribe::memtable::Memtable;
    use crate::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
    use crate::scribe::wal::WalLsn;
    use arrow::array::{Int64Array, RecordBatch};
    use arrow::datatypes::{DataType, Field, Schema};
    use wyrd_spec::DataTenantId;
    use wyrd_tonic::tonic::Status;

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

        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let batch = |values: Vec<i64>| {
            RecordBatch::try_new(
                Arc::clone(&schema),
                vec![Arc::new(Int64Array::from(values))],
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

    /// Listing collects only the named tenant table's partitions.
    ///
    /// The same memtable holds the listed table, another table of the same
    /// tenant, and the same table of another tenant, each on its own day. Only
    /// the listed table's day is returned, once, although it is held both
    /// frozen and writable.
    ///
    /// # Panics
    /// Panics when a fixture insert fails or the listing names another table's
    /// or tenant's partition.
    #[test]
    fn listing_collects_only_the_named_table() {
        use crate::catalog::TableRef;
        use crate::scribe::seal_key::SealKey;

        let tenant = DataTenantId::new_v7();
        let listed_day = crate::test_support::day_partition(2026, 7, 14);
        let stream = StreamIdentity::new(NodeId::generate(), WriterEpoch::new(1));
        let (service, binding) = selective_tail_fixture(tenant, listed_day, stream);
        let memtable = service
            .memtable
            .as_ref()
            .expect("fixture is direct-memtable");
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let rows = RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![9]))])
            .expect("valid fixture batch");
        for key in [
            SealKey::new(
                tenant,
                TableRef::new(crate::namespaces::BifrostNamespace::Bifrost, "other"),
                crate::test_support::day_partition(2026, 7, 15),
            ),
            SealKey::new(
                DataTenantId::new_v7(),
                binding.table_ref.clone(),
                crate::test_support::day_partition(2026, 7, 16),
            ),
        ] {
            let meta = crate::scribe::wal::ScribeAppendMeta {
                batch_id: *uuid::Uuid::now_v7().as_bytes(),
                schema_fingerprint: [0; 32],
                data_digest: [0; 32],
                data_len: 0,
                payload_digest: [0; 32],
                payload_len: 0,
                slice_index: 0,
                slice_count: 1,
                rows_accepted: 1,
                wal_lsn_min: WalLsn::new(3),
                wal_lsn_max: WalLsn::new(3),
                seal_key: key.to_string(),
            };
            memtable
                .insert(&key, meta, rows.clone())
                .expect("unlisted fixture batch inserts");
        }

        let partitions = service
            .active_partitions_for_table(tenant, &binding.table_ref)
            .expect("listing reads the memtable");

        assert_eq!(partitions, vec![listed_day]);
    }

    /// A listing status keeps its closed class instead of one state error.
    ///
    /// Only `Unavailable` is availability loss Oracle may degrade. Credential
    /// and tenant refusals stay authorization, a rejected binding stays a
    /// binding fault, a deadline stays a deadline, and cancellation or an
    /// internal Scribe failure is a fatal state fault.
    ///
    /// # Panics
    /// Panics when any status maps to a different class.
    #[test]
    fn listing_status_keeps_its_failure_class() {
        assert!(matches!(
            tonic_error(&Status::unavailable("scribe down")),
            TailReadError::Unavailable { .. }
        ));
        for refused in [
            Status::unauthenticated("bad bearer"),
            Status::permission_denied("bad ticket"),
        ] {
            assert!(matches!(
                tonic_error(&refused),
                TailReadError::Authorization { .. }
            ));
        }
        assert!(matches!(
            tonic_error(&Status::invalid_argument("binding")),
            TailReadError::Binding
        ));
        assert!(matches!(
            tonic_error(&Status::deadline_exceeded("late")),
            TailReadError::DeadlineElapsed
        ));
        for fatal in [Status::cancelled("gone"), Status::internal("state")] {
            assert!(matches!(tonic_error(&fatal), TailReadError::State { .. }));
        }
    }

    /// Writes `rows` as one staged Parquet run under `directory`, one row per
    /// row group, so a selective scan has row groups to prune. The footer
    /// records `tenant`, the way every staged run Scribe writes does.
    ///
    /// # Panics
    /// Panics when the fixture cannot write its own run.
    fn write_staged_run(
        directory: &std::path::Path,
        schema: Arc<Schema>,
        rows: &RecordBatch,
        tenant: DataTenantId,
    ) -> std::path::PathBuf {
        let run = directory.join("run-0.parquet");
        let mut writer = parquet::arrow::ArrowWriter::try_new(
            std::fs::File::create(&run).expect("staged run file"),
            schema,
            Some(
                parquet::file::properties::WriterProperties::builder()
                    .set_max_row_group_row_count(Some(1))
                    .set_key_value_metadata(Some(vec![crate::parquet::footer::tenant_key_value(
                        tenant,
                    )]))
                    .build(),
            ),
        )
        .expect("staged run writer");
        writer.write(rows).expect("staged run rows");
        writer.close().expect("staged run footer");
        run
    }

    /// Returns the `Int64` values of one batch's first column.
    ///
    /// # Panics
    /// Panics when the first column is not `Int64`.
    fn first_column_values(batch: &RecordBatch) -> Vec<i64> {
        batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("Int64 value column")
            .values()
            .to_vec()
    }

    /// Returns every `value` one opened live read serves, sorted, so
    /// duplicates across its memtable cut and staged runs stay visible.
    ///
    /// Staged runs are decoded directly while the read's lease is still held.
    ///
    /// # Panics
    /// Panics when a staged run cannot be decoded.
    fn served_values(read: super::LiveTailBatches) -> Vec<i64> {
        let (rows, runs, _lease) = read.into_parts();
        let mut values = rows
            .iter()
            .flat_map(first_column_values)
            .collect::<Vec<_>>();
        for run in runs {
            let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
                std::fs::File::open(run).expect("leased staged run opens"),
            )
            .expect("staged run footer")
            .build()
            .expect("staged run reader");
            for batch in reader {
                values.extend(first_column_values(&batch.expect("staged run batch")));
            }
        }
        values.sort_unstable();
        values
    }

    /// Opens one live read and returns every `value` it serves, sorted.
    ///
    /// # Panics
    /// Panics when the read cannot open or a staged run cannot be decoded.
    async fn hot_values(
        service: &FetchLiveTailService,
        request: super::FetchLiveTailRequest,
    ) -> Vec<i64> {
        served_values(
            service
                .open_live_batches(request)
                .await
                .expect("live read opens"),
        )
    }

    /// One frozen generation whose rows are both in the memtable and in a
    /// registered staged run, served by a direct-memtable tail service.
    pub(crate) struct StagedGenerationFixture {
        /// Service reading the memtable and the registry.
        pub(crate) service: Arc<FetchLiveTailService>,
        /// Memtable holding the frozen generation.
        pub(crate) memtable: Arc<crate::scribe::memtable::Memtable>,
        /// Registry naming the generation's staged runs.
        pub(crate) registry: Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>,
        /// Seal key of the generation.
        pub(crate) key: crate::scribe::seal_key::SealKey,
        /// The staged generation.
        pub(crate) generation: crate::scribe::hot_source::GenerationOrdinal,
        /// Staged member holding the runs.
        pub(crate) member: crate::scribe::assembly::StagedMemberId,
        /// Request reading the generation's day.
        pub(crate) request: super::FetchLiveTailRequest,
        /// Directory owning the staged run; the run is deleted when it drops.
        pub(crate) directory: tempfile::TempDir,
    }

    /// Builds a [`StagedGenerationFixture`] holding rows `[1, 2, 3]`.
    ///
    /// # Panics
    /// Panics when any fixture step fails.
    fn staged_generation_fixture() -> StagedGenerationFixture {
        staged_generation_fixture_with_runs(1)
    }

    /// Builds a [`StagedGenerationFixture`] whose generation is staged as
    /// `runs` identical runs of rows `[1, 2, 3]`, one row group per row.
    ///
    /// # Panics
    /// Panics when any fixture step fails.
    pub(crate) fn staged_generation_fixture_with_runs(runs: usize) -> StagedGenerationFixture {
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
        let run = write_staged_run(directory.path(), schema, &rows, tenant);
        let runs = (0..runs)
            .map(|index| {
                let copy = directory.path().join(format!("run-copy-{index}.parquet"));
                std::fs::copy(&run, &copy).expect("fixture staged run copies");
                copy
            })
            .collect();
        let member = StagedMemberId::new(0, generation.get());
        registry
            .advance(
                &key,
                generation,
                HotAuthority::StagedRun {
                    member,
                    runs,
                    bytes: 4_096,
                    wal: (WalLsn::new(1), WalLsn::new(1)),
                },
            )
            .expect("the generation moves to its staged runs");

        let service = Arc::new(
            FetchLiveTailService::new(stream, Arc::clone(&memtable))
                .with_hot_sources(Arc::clone(&registry)),
        );
        let binding =
            super::TenantTableBinding::resolve((tenant, table)).expect("fixture binding resolves");
        let request = super::FetchLiveTailRequest {
            binding,
            target_stream: stream,
            start_partition: day,
            end_partition: day,
            required_columns: vec!["value".to_owned()],
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
    /// publication retires their authority, and holds its lease until it
    /// drops.
    ///
    /// # Panics
    /// Panics when the fixture cannot be built, the leased run cannot be read
    /// after publication, or the lease does not follow the read.
    #[tokio::test]
    async fn an_open_live_read_keeps_staged_runs_across_publication() {
        let StagedGenerationFixture {
            service,
            memtable,
            registry,
            key,
            generation,
            member,
            request,
            directory: _directory,
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
        let (rows, runs, lease) = open.into_parts();
        assert!(rows.is_empty(), "a durable staged generation leaves memory");
        assert_eq!(runs.len(), 1, "the leased run is still named");
        assert!(
            runs[0].exists(),
            "a leased staged run stays on disk after publication"
        );
        assert_eq!(
            registry.leases(&key, generation).expect("locked"),
            1,
            "an open read keeps its lease"
        );
        drop(lease);
        assert_eq!(
            registry.leases(&key, generation).expect("locked"),
            0,
            "dropping the read releases the lease"
        );
    }
}
