//! Typed, pod-local live-tail reads over writable and immutable memtable data.
//!
//! The Scribe reader owns no WAL or SQL access. Its local transport preserves
//! shallow Arrow batches; its private tonic transport authenticates with the
//! already-issued workload bearer and returns owned decoded frames.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arrow::array::{Array, StringArray};
use arrow::ipc::reader::StreamReader;
use arrow::ipc::writer::StreamWriter;
use async_trait::async_trait;
use chrono::Utc;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::api as tail;
use wyrd_tonic::tonic::metadata::MetadataValue;
use wyrd_tonic::tonic::{Request, transport::Channel};
use wyrd_tonic::wyrd::v1::scribe_tail_service_client::ScribeTailServiceClient;

use crate::catalog::TenantTableBinding;
use crate::contracts::ScribeError;
use crate::scribe::memtable::Memtable;
use crate::scribe::routing::shard_for;
use crate::scribe::seal_key::EventDay;
use crate::scribe::shards::ScribeShardRuntime;
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::WalLsn;

/// The only tail protocol revision understood by the Scribe v1 reader.
pub const TAIL_PROTOCOL_VERSION: u16 = 1;

/// Maximum expired fences reclaimed while servicing one foreground operation.
const OPPORTUNISTIC_EXPIRY_LIMIT: usize = 64;

/// Bounds retained Scribe fences and each page returned from one immutable interval.
#[derive(Debug, Clone, Copy)]
pub struct TailFenceConfig {
    /// Default upper bound for a fence lifetime.
    pub ttl: Duration,
    /// Maximum number of simultaneously retained fences.
    pub max_fences: usize,
    /// Maximum shallow Arrow bytes held by all retained fences.
    pub max_retained_bytes: usize,
    /// Maximum rows returned by one page irrespective of a caller's request.
    pub max_page_rows: u32,
    /// Maximum Arrow IPC bytes returned by one page irrespective of a caller's request.
    pub max_page_encoded_bytes: u32,
}

impl Default for TailFenceConfig {
    fn default() -> Self {
        Self {
            ttl: Duration::from_secs(30),
            max_fences: 256,
            max_retained_bytes: 512 * 1024 * 1024,
            max_page_rows: 4_096,
            max_page_encoded_bytes: 16 * 1024 * 1024,
        }
    }
}

/// A local page whose Arrow batches retain shallow ownership of Scribe arrays.
#[derive(Debug, Clone)]
pub struct LocalTailPage {
    /// Shallow row batches returned from the frozen interval.
    pub batches: Vec<Arc<arrow::record_batch::RecordBatch>>,
    /// The exact final row cursor when the interval has more rows.
    pub next: Option<tail::TailCursor>,
    /// Whether the interval is exhausted after this page.
    pub complete: bool,
}

/// Tail-read boundary with local shallow and remote owned-frame implementations.
#[async_trait]
pub trait TailReadTransport: Send + Sync {
    /// Acquires metadata for one immutable tail interval.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError`] when the transport cannot acquire the requested
    /// fence or Scribe rejects its validation bounds.
    async fn acquire_fence(
        &self,
        request: tail::AcquireTailFenceRequest,
    ) -> Result<tail::TailReadFence, TailReadError>;

    /// Reads one locally consumable row-precise page.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError`] when the retained interval is unavailable or a
    /// requested bound cannot be satisfied.
    async fn read_page(
        &self,
        request: tail::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError>;

    /// Idempotently releases a retained interval.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] when the local registry cannot be safely
    /// accessed.
    fn release_fence(&self, fence_id: tail::TailFenceId) -> Result<FenceRelease, TailReadError>;

    /// Awaits release completion when the transport has an asynchronous
    /// lifecycle (for example, an authenticated remote RPC).
    async fn release_fence_async(
        &self,
        fence_id: tail::TailFenceId,
    ) -> Result<FenceRelease, TailReadError> {
        self.release_fence(fence_id)
    }
}

/// In-process transport that preserves Scribe's shallow Arrow ownership.
#[derive(Debug, Clone)]
pub struct LocalTailReadTransport {
    /// The one Scribe-owned reader that retains interval state.
    reader: Arc<ScribeTailReader>,
}

impl LocalTailReadTransport {
    /// Wraps the local Scribe reader without introducing an IPC encode/decode hop.
    #[must_use]
    pub fn new(reader: Arc<ScribeTailReader>) -> Self {
        Self { reader }
    }
}

#[async_trait]
impl TailReadTransport for LocalTailReadTransport {
    /// Delegates metadata-only acquisition to the Scribe-owned reader.
    async fn acquire_fence(
        &self,
        request: tail::AcquireTailFenceRequest,
    ) -> Result<tail::TailReadFence, TailReadError> {
        self.reader.acquire_fence(request).await
    }

    /// Delegates local shallow page reads to the Scribe-owned reader.
    async fn read_page(
        &self,
        request: tail::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError> {
        self.reader.read_page(&request)
    }

    /// Delegates idempotent local release to the Scribe-owned reader.
    fn release_fence(&self, fence_id: tail::TailFenceId) -> Result<FenceRelease, TailReadError> {
        self.reader.release_fence(fence_id)
    }
}

/// Private tonic tail client that turns owned IPC frames back into Arrow batches.
///
/// The adapter is deliberately separate from [`TailReadTransport`]: local fence
/// release is synchronous, while a remote release is an authenticated RPC. Both
/// expose the same typed domain requests and row-precise page shape.
#[derive(Debug, Clone)]
pub struct TonicTailReadTransport {
    /// Cloneable tonic client over one configured private Scribe endpoint.
    client: ScribeTailServiceClient<Channel>,
    /// Already-issued service-workload bearer sent on every private RPC.
    access_token: MetadataValue<wyrd_tonic::tonic::metadata::Ascii>,
}

/// Remote tail-page ceiling including protobuf framing overhead.
const TAIL_RPC_MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024 + 64 * 1024;

impl TonicTailReadTransport {
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
            client: client.max_decoding_message_size(TAIL_RPC_MAX_MESSAGE_BYTES),
            access_token,
        })
    }

    /// Acquires immutable remote fence metadata with the workload bearer attached.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] for transport/status/conversion failures.
    pub async fn acquire_fence(
        &self,
        request: tail::AcquireTailFenceRequest,
    ) -> Result<tail::TailReadFence, TailReadError> {
        let mut client = self.client.clone();
        let response = client
            .acquire_fence(self.authenticated_request(request.into()))
            .await
            .map_err(|status| tonic_error(&status))?;
        response.into_inner().try_into().map_err(
            |error: wyrd_tonic::private_conversion::PrivateConversionError| TailReadError::State {
                detail: error.to_string(),
            },
        )
    }

    /// Reads one remote page and decodes its owned Arrow IPC frames.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] for transport, conversion, or IPC decode
    /// failures.
    pub async fn read_page(
        &self,
        request: tail::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError> {
        let mut client = self.client.clone();
        let response = client
            .read_fence_page(self.authenticated_request(request.into()))
            .await
            .map_err(|status| tonic_error(&status))?;
        let page: tail::TailPage = response.into_inner().try_into().map_err(
            |error: wyrd_tonic::private_conversion::PrivateConversionError| TailReadError::State {
                detail: error.to_string(),
            },
        )?;
        let batches = page
            .batches
            .iter()
            .map(|bytes| decode_batch(bytes))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(LocalTailPage {
            batches,
            next: page.next,
            complete: page.complete,
        })
    }

    /// Releases a remote fence idempotently with the workload bearer attached.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] when the private RPC fails.
    pub async fn release_fence(&self, fence_id: tail::TailFenceId) -> Result<(), TailReadError> {
        let mut client = self.client.clone();
        client
            .release_fence(
                self.authenticated_request(tail::ReleaseTailFenceRequest { fence_id }.into()),
            )
            .await
            .map_err(|status| tonic_error(&status))?;
        Ok(())
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

#[async_trait]
impl TailReadTransport for TonicTailReadTransport {
    /// Acquire a remote immutable fence through the authenticated tonic client.
    async fn acquire_fence(
        &self,
        request: tail::AcquireTailFenceRequest,
    ) -> Result<tail::TailReadFence, TailReadError> {
        TonicTailReadTransport::acquire_fence(self, request).await
    }

    /// Read and decode one remote owned-frame page.
    async fn read_page(
        &self,
        request: tail::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError> {
        TonicTailReadTransport::read_page(self, request).await
    }

    /// Declines synchronous remote release from a drop boundary.
    ///
    /// Lifecycle owners call [`Self::release_fence_async`] while they can await
    /// the authenticated RPC. Abandoned intervals rely on the bounded Scribe
    /// TTL instead of spawning detached cleanup from `Drop`.
    fn release_fence(&self, fence_id: tail::TailFenceId) -> Result<FenceRelease, TailReadError> {
        let _ = fence_id;
        Ok(FenceRelease { released: false })
    }

    /// Awaits the authenticated remote release RPC; no detached cleanup task is created.
    async fn release_fence_async(
        &self,
        fence_id: tail::TailFenceId,
    ) -> Result<FenceRelease, TailReadError> {
        TonicTailReadTransport::release_fence(self, fence_id).await?;
        Ok(FenceRelease { released: true })
    }
}

/// Converts an authenticated remote status into the local reader error shape.
fn tonic_error(status: &wyrd_tonic::tonic::Status) -> TailReadError {
    TailReadError::State {
        detail: status.to_string(),
    }
}

/// Decodes exactly one owned Arrow IPC batch received from the private service.
///
/// # Errors
///
/// Returns [`TailReadError::State`] for malformed IPC, an empty frame, or a
/// frame that carries more than the protocol's one batch.
fn decode_batch(bytes: &[u8]) -> Result<Arc<arrow::record_batch::RecordBatch>, TailReadError> {
    let mut reader =
        StreamReader::try_new(Cursor::new(bytes), None).map_err(|error| TailReadError::State {
            detail: format!("tail IPC decode failed: {error}"),
        })?;
    let batch = reader
        .next()
        .transpose()
        .map_err(|error| TailReadError::State {
            detail: format!("tail IPC read failed: {error}"),
        })?
        .ok_or_else(|| TailReadError::State {
            detail: "tail IPC frame contains no batch".to_owned(),
        })?;
    if reader.next().is_some() {
        return Err(TailReadError::State {
            detail: "tail IPC frame contains more than one batch".to_owned(),
        });
    }
    Ok(Arc::new(batch))
}

/// Idempotent outcome of releasing a tail fence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FenceRelease {
    /// Whether this call removed a retained fence and freed its capacity.
    pub released: bool,
}

/// Summary of one bounded expiry pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpiryReport {
    /// Number of expired fences removed during this pass.
    pub released: usize,
    /// Number of fences still retained after this pass.
    pub retained: usize,
}

/// Errors local to tail retention and paging before tonic maps them to statuses.
#[derive(Debug, thiserror::Error)]
pub enum TailReadError {
    /// The requested tail protocol is not served by this Scribe.
    #[error("unsupported tail protocol version {version}")]
    UnsupportedProtocol { version: u16 },
    /// The query deadline cannot retain a live interval.
    #[error("tail fence deadline has elapsed")]
    DeadlineElapsed,
    /// The provided binding cannot map to this Scribe's logical table owner.
    #[error("invalid tail binding")]
    Binding,
    /// The authenticated tenant does not own the retained fence binding.
    #[error("authenticated tenant cannot access this tail fence")]
    AccessDenied,
    /// A cursor names a different writer epoch than the retained stream.
    #[error("tail cursor writer epoch does not match the stream")]
    WriterEpochMismatch,
    /// A cursor is outside the immutable fence interval.
    #[error("tail cursor is outside the fence interval")]
    CursorOutOfRange,
    /// The observed live schema does not match the planned schema fingerprint.
    #[error("tail schema fingerprint does not match")]
    SchemaMismatch,
    /// The bounded fence registry cannot retain another interval.
    #[error("tail fence capacity is exhausted")]
    Capacity,
    /// A single row cannot be represented inside the configured response ceiling.
    #[error("a single tail row exceeds the encoded page ceiling")]
    OversizeRow,
    /// Arrow IPC encoding failed while calculating a page bound.
    #[error("tail Arrow IPC encoding failed: {detail}")]
    Encode { detail: String },
    /// Scribe state could not produce a consistent shallow snapshot.
    #[error("tail Scribe state failed: {detail}")]
    State { detail: String },
}

/// Concrete Scribe-owned reader that freezes one bounded live interval at a time.
#[derive(Debug)]
pub struct ScribeTailReader {
    /// Existing Scribe state reader used only to take a shallow point-in-time snapshot.
    source: Arc<FetchLiveTailService>,
    /// Retention and page ceilings enforced before caller-provided values.
    config: TailFenceConfig,
    /// Mutable retained fences and aggregate shallow-memory accounting.
    fences: Mutex<FenceRegistry>,
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

/// Parses the validated wire event-day into the local memtable partition key.
///
/// # Errors
///
/// Returns [`TailReadError::Binding`] when the wire date cannot form the local
/// UTC day representation.
fn event_day_from_wire(event_day: &tail::EventDay) -> Result<EventDay, TailReadError> {
    chrono::NaiveDate::parse_from_str(event_day.as_str(), "%Y-%m-%d")
        .map(EventDay::new)
        .map_err(|_| TailReadError::Binding)
}

/// Verifies that retained batches use the schema selected during planning.
///
/// An empty interval has no data schema to compare, so it is safely retained as
/// empty; the binding's catalog owner has already validated the requested schema
/// before it asks Scribe for the live cut.
///
/// # Errors
///
/// Returns [`TailReadError::SchemaMismatch`] when any shallow batch differs from
/// the required planning fingerprint.
fn validate_schema(
    batches: &[RetainedBatch],
    expected: &tail::SchemaFingerprint,
) -> Result<(), TailReadError> {
    for batch in batches {
        let actual = hex::encode(
            crate::contracts::projected_source_schema_fingerprint(batch.rows.schema().as_ref()).0,
        );
        if actual != expected.as_str() {
            return Err(TailReadError::SchemaMismatch);
        }
    }
    Ok(())
}

/// Returns the inclusive live edge of the retained snapshot.
///
/// # Errors
///
/// Returns [`TailReadError::State`] when persisted row identity is invalid, or
/// a cursor comparison error when the retained writer epoch is inconsistent.
fn inclusive_cursor(
    exclusive: &tail::TailCursor,
    writer_epoch: u64,
    batches: &[RetainedBatch],
) -> Result<tail::TailCursor, TailReadError> {
    let mut inclusive = exclusive.clone();
    for batch in batches {
        let ordinals =
            crate::schema::managed_columns::row_ordinals(batch.rows.as_ref()).map_err(|error| {
                TailReadError::State {
                    detail: format!("retained row identity invariant failed: {error}"),
                }
            })?;
        if let Some(row) = batch.rows.num_rows().checked_sub(1) {
            let ordinal = u32::try_from(ordinals.value(row)).map_err(|_| TailReadError::State {
                detail: "retained row ordinal is negative".to_owned(),
            })?;
            let candidate = tail::TailCursor {
                writer_epoch,
                wal_lsn: batch.lsn.as_u64(),
                batch_id: batch.batch_id,
                row_ordinal: ordinal,
            };
            if cursor_cmp(&candidate, &inclusive)? == std::cmp::Ordering::Greater {
                inclusive = candidate;
            }
        }
    }
    Ok(inclusive)
}

/// Validates a continuation against the retained writer epoch and exact fence bounds.
///
/// # Errors
///
/// Returns [`TailReadError::WriterEpochMismatch`] for a different writer epoch
/// and [`TailReadError::CursorOutOfRange`] outside `(exclusive, inclusive]`.
fn validate_after(
    after: Option<&tail::TailCursor>,
    fence: &tail::TailReadFence,
) -> Result<(), TailReadError> {
    let Some(after) = after else {
        return Ok(());
    };
    if after.writer_epoch != fence.stream.writer_epoch {
        return Err(TailReadError::WriterEpochMismatch);
    }
    if cursor_cmp(after, &fence.exclusive_sealed)? == std::cmp::Ordering::Less
        || cursor_cmp(after, &fence.inclusive_live)? == std::cmp::Ordering::Greater
    {
        return Err(TailReadError::CursorOutOfRange);
    }
    Ok(())
}

/// Orders two cursors only after proving their writer epochs agree.
///
/// # Errors
///
/// Returns [`TailReadError::WriterEpochMismatch`] when the cursors name
/// different writer epochs.
fn cursor_cmp(
    left: &tail::TailCursor,
    right: &tail::TailCursor,
) -> Result<std::cmp::Ordering, TailReadError> {
    if left.writer_epoch != right.writer_epoch {
        return Err(TailReadError::WriterEpochMismatch);
    }
    Ok((left.wal_lsn, left.batch_id, left.row_ordinal).cmp(&(
        right.wal_lsn,
        right.batch_id,
        right.row_ordinal,
    )))
}

/// Encodes one shallow row only to prove that the page byte ceiling is respected.
///
/// # Errors
///
/// Returns [`TailReadError::Encode`] when Arrow cannot encode the row.
fn encode_record_batch(batch: &arrow::record_batch::RecordBatch) -> Result<Vec<u8>, TailReadError> {
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &batch.schema()).map_err(|error| {
        TailReadError::Encode {
            detail: error.to_string(),
        }
    })?;
    writer.write(batch).map_err(|error| TailReadError::Encode {
        detail: error.to_string(),
    })?;
    writer.finish().map_err(|error| TailReadError::Encode {
        detail: error.to_string(),
    })?;
    Ok(bytes)
}

#[derive(Debug)]
struct FenceRegistry {
    /// Fences currently retaining shallow Scribe Arrow arrays.
    retained: HashMap<tail::TailFenceId, RetainedFence>,
    /// Aggregate Arrow array bytes retained by every live fence.
    retained_bytes: usize,
}

#[derive(Debug)]
struct RetainedFence {
    /// Immutable metadata returned by this fence acquisition.
    fence: tail::TailReadFence,
    /// Monotonic local expiration used independently of wall-clock conversion.
    expires_at: Instant,
    /// Shallow append batches frozen at acquisition.
    batches: Vec<RetainedBatch>,
    /// Bytes charged to this fence when releasing registry capacity.
    retained_bytes: usize,
}

#[derive(Debug)]
struct RetainedBatch {
    /// WAL position shared by every row in this admitted append.
    lsn: WalLsn,
    /// Immutable idempotency batch identity shared by every row in this append.
    batch_id: uuid::Uuid,
    /// Shared Arrow arrays retained without copying row payloads.
    rows: Arc<arrow::record_batch::RecordBatch>,
}

impl ScribeTailReader {
    /// Creates one bounded reader over the supplied pod-local Scribe source.
    #[must_use]
    pub fn new(source: Arc<FetchLiveTailService>, config: TailFenceConfig) -> Self {
        Self {
            source,
            config: TailFenceConfig {
                ttl: config.ttl.min(Duration::from_secs(30)),
                max_fences: config.max_fences.max(1),
                max_retained_bytes: config.max_retained_bytes.max(1),
                max_page_rows: config.max_page_rows.max(1),
                max_page_encoded_bytes: config.max_page_encoded_bytes.max(1),
            },
            fences: Mutex::new(FenceRegistry {
                retained: HashMap::new(),
                retained_bytes: 0,
            }),
        }
    }

    /// Acquires metadata for one exact `(exclusive, inclusive]` shallow interval.
    ///
    /// The source snapshot is taken before retention capacity is reserved, so a
    /// rejected capacity request cannot create a partial fence or leak retained rows.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError`] when the binding, protocol, deadline, stream,
    /// schema, or registry capacity is invalid, or when Scribe cannot snapshot
    /// its live rows.
    pub async fn acquire_fence(
        &self,
        request: tail::AcquireTailFenceRequest,
    ) -> Result<tail::TailReadFence, TailReadError> {
        if request.tail_protocol_version != TAIL_PROTOCOL_VERSION {
            return Err(TailReadError::UnsupportedProtocol {
                version: request.tail_protocol_version,
            });
        }
        let now = Utc::now();
        let remaining = request.deadline.signed_duration_since(now);
        let ttl = remaining
            .to_std()
            .map_err(|_| TailReadError::DeadlineElapsed)?;
        let ttl = ttl.min(self.config.ttl);
        if ttl.is_zero() {
            return Err(TailReadError::DeadlineElapsed);
        }
        let binding = binding_from_wire(&request.binding)?;
        let event_day = event_day_from_wire(&request.event_day)?;
        let stream = self.source.stream();
        let stream_epoch =
            u64::try_from(stream.writer_epoch.as_i64()).map_err(|_| TailReadError::State {
                detail: "Scribe writer epoch is negative".to_owned(),
            })?;
        if request.exclusive_sealed.writer_epoch != stream_epoch {
            return Err(TailReadError::WriterEpochMismatch);
        }
        let hot = self
            .source
            .fetch_hot_batches(FetchLiveTailRequest {
                binding: binding.clone(),
                target_stream: stream,
                start_day: event_day,
                end_day: event_day,
                after_lsn: WalLsn::ZERO,
                required_columns: Vec::new(),
            })
            .await
            .map_err(|error| TailReadError::State {
                detail: error.to_string(),
            })?;
        let mut batches = hot
            .into_iter()
            .map(|batch| {
                Ok(RetainedBatch {
                    lsn: batch.wal_lsn,
                    batch_id: uuid::Uuid::from_bytes(batch.batch_id),
                    rows: Arc::new(batch.rows),
                })
            })
            .collect::<Result<Vec<_>, TailReadError>>()?;
        batches.sort_by_key(|batch| (batch.lsn, batch.batch_id));
        validate_schema(&batches, &request.schema_fingerprint)?;
        let inclusive_live = inclusive_cursor(&request.exclusive_sealed, stream_epoch, &batches)?;
        let retained_bytes = batches
            .iter()
            .map(|batch| batch.rows.get_array_memory_size())
            .sum::<usize>();
        let expires_at =
            now + chrono::Duration::from_std(ttl).map_err(|_| TailReadError::DeadlineElapsed)?;
        let fence = tail::TailReadFence {
            fence_id: tail::TailFenceId::new(uuid::Uuid::now_v7()),
            binding: request.binding,
            event_day: request.event_day,
            stream: tail::TailStreamIdentity {
                node_id: tail::NodeId::new(stream.node_id.as_uuid()),
                writer_epoch: stream_epoch,
            },
            exclusive_sealed: request.exclusive_sealed,
            inclusive_live,
            schema_fingerprint: request.schema_fingerprint,
            tail_protocol_version: TAIL_PROTOCOL_VERSION,
            expires_at,
        };
        let mut registry = self.fences.lock().map_err(|error| TailReadError::State {
            detail: format!("tail fence registry lock poisoned: {error}"),
        })?;
        Self::reclaim_expired(&mut registry, Instant::now(), OPPORTUNISTIC_EXPIRY_LIMIT);
        if registry.retained.len() >= self.config.max_fences
            || registry.retained_bytes.saturating_add(retained_bytes)
                > self.config.max_retained_bytes
        {
            return Err(TailReadError::Capacity);
        }
        registry.retained_bytes = registry.retained_bytes.saturating_add(retained_bytes);
        registry.retained.insert(
            fence.fence_id,
            RetainedFence {
                fence: fence.clone(),
                expires_at: Instant::now() + ttl,
                batches,
                retained_bytes,
            },
        );
        metrics::counter!("bifrost_tail_fences_total", "outcome" => "acquired").increment(1);
        Ok(fence)
    }

    /// Reads one row-precise shallow page from a retained fence.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError`] when the fence expired, the continuation is
    /// outside its exact interval, or an individual row cannot meet the encoded
    /// byte bound.
    pub fn read_page(
        &self,
        request: &tail::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError> {
        self.read_page_inner(None, request.clone())
    }

    /// Reads one page only when the authenticated tenant owns the retained binding.
    ///
    /// The tenant check and shallow-batch lookup occur under the same registry
    /// lock, so an unauthorized caller cannot observe data between validation
    /// and paging.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::AccessDenied`] before reading retained data when
    /// `tenant` differs from the fence binding. Other failures match
    /// [`Self::read_page`].
    pub fn read_page_for_tenant(
        &self,
        tenant: DataTenantId,
        request: tail::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError> {
        self.read_page_inner(Some(tenant), request)
    }

    /// Implements local and authenticated paging under one atomic registry lookup.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError`] for poisoned state, an unknown or expired
    /// fence, unauthorized tenant, invalid continuation, encoding failure, or
    /// an unsatisfied one-row byte ceiling.
    fn read_page_inner(
        &self,
        tenant: Option<DataTenantId>,
        request: tail::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError> {
        let tail::TailPageRequest {
            fence_id,
            after,
            max_rows,
            max_encoded_bytes,
        } = request;
        let mut registry = self.fences.lock().map_err(|error| TailReadError::State {
            detail: format!("tail fence registry lock poisoned: {error}"),
        })?;
        Self::reclaim_expired(&mut registry, Instant::now(), OPPORTUNISTIC_EXPIRY_LIMIT);
        let retained = registry
            .retained
            .get(&fence_id)
            .ok_or(TailReadError::CursorOutOfRange)?;
        if tenant.is_some_and(|tenant| retained.fence.binding.tenant_id != tenant) {
            return Err(TailReadError::AccessDenied);
        }
        validate_after(after.as_ref(), &retained.fence)?;
        let row_limit = max_rows.min(self.config.max_page_rows).max(1) as usize;
        let byte_limit = max_encoded_bytes
            .min(self.config.max_page_encoded_bytes)
            .max(1) as usize;
        let after = after.as_ref().unwrap_or(&retained.fence.exclusive_sealed);
        let mut rows = Vec::new();
        let mut encoded_bytes = 0_usize;
        let mut next = None;
        for batch in &retained.batches {
            let ordinals = crate::schema::managed_columns::row_ordinals(batch.rows.as_ref())
                .map_err(|error| TailReadError::State {
                    detail: format!("retained row identity invariant failed: {error}"),
                })?;
            for row_index in 0..batch.rows.num_rows() {
                let cursor = tail::TailCursor {
                    writer_epoch: retained.fence.stream.writer_epoch,
                    wal_lsn: batch.lsn.as_u64(),
                    batch_id: batch.batch_id,
                    row_ordinal: u32::try_from(ordinals.value(row_index)).map_err(|_| {
                        TailReadError::State {
                            detail: "retained row ordinal is negative".to_owned(),
                        }
                    })?,
                };
                if cursor_cmp(&cursor, after)? != std::cmp::Ordering::Greater {
                    continue;
                }
                if cursor_cmp(&cursor, &retained.fence.inclusive_live)? != std::cmp::Ordering::Less
                    && cursor != retained.fence.inclusive_live
                {
                    continue;
                }
                let row = Arc::new(batch.rows.slice(row_index, 1));
                let row_bytes = encode_record_batch(row.as_ref())?.len();
                if rows.is_empty() && row_bytes > byte_limit {
                    return Err(TailReadError::OversizeRow);
                }
                if rows.len() == row_limit || encoded_bytes.saturating_add(row_bytes) > byte_limit {
                    return Ok(LocalTailPage {
                        batches: rows,
                        next,
                        complete: false,
                    });
                }
                encoded_bytes = encoded_bytes.saturating_add(row_bytes);
                next = Some(cursor);
                rows.push(row);
            }
        }
        Ok(LocalTailPage {
            batches: rows,
            next,
            complete: true,
        })
    }

    /// Releases one retained fence exactly once; repeat calls are successful no-ops.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] only when the local registry lock is poisoned.
    pub fn release_fence(
        &self,
        fence_id: tail::TailFenceId,
    ) -> Result<FenceRelease, TailReadError> {
        self.release_fence_inner(None, fence_id)
    }

    /// Releases a fence only when the authenticated tenant owns its binding.
    ///
    /// The ownership check precedes removal under the same registry lock. A
    /// denied release therefore leaves retained bytes and capacity unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::AccessDenied`] for a cross-tenant fence and
    /// [`TailReadError::State`] when the registry lock is poisoned.
    pub fn release_fence_for_tenant(
        &self,
        tenant: DataTenantId,
        fence_id: tail::TailFenceId,
    ) -> Result<FenceRelease, TailReadError> {
        self.release_fence_inner(Some(tenant), fence_id)
    }

    /// Implements local and authenticated release under one atomic registry mutation.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::AccessDenied`] before removal for a mismatched
    /// tenant or [`TailReadError::State`] when the registry lock is poisoned.
    fn release_fence_inner(
        &self,
        tenant: Option<DataTenantId>,
        fence_id: tail::TailFenceId,
    ) -> Result<FenceRelease, TailReadError> {
        let mut registry = self.fences.lock().map_err(|error| TailReadError::State {
            detail: format!("tail fence registry lock poisoned: {error}"),
        })?;
        Self::reclaim_expired(&mut registry, Instant::now(), OPPORTUNISTIC_EXPIRY_LIMIT);
        if registry.retained.get(&fence_id).is_some_and(|retained| {
            tenant.is_some_and(|tenant| retained.fence.binding.tenant_id != tenant)
        }) {
            return Err(TailReadError::AccessDenied);
        }
        let Some(retained) = registry.retained.remove(&fence_id) else {
            return Ok(FenceRelease { released: false });
        };
        registry.retained_bytes = registry
            .retained_bytes
            .saturating_sub(retained.retained_bytes);
        metrics::counter!("bifrost_tail_fences_total", "outcome" => "released").increment(1);
        Ok(FenceRelease { released: true })
    }

    /// Removes at most `max` expired fences and reports retained capacity after the pass.
    #[must_use]
    pub fn expire_due(&self, now: Instant, max: usize) -> ExpiryReport {
        let Ok(mut registry) = self.fences.lock() else {
            return ExpiryReport {
                released: 0,
                retained: 0,
            };
        };
        let released = Self::reclaim_expired(&mut registry, now, max);
        ExpiryReport {
            released,
            retained: registry.retained.len(),
        }
    }

    /// Reclaims a bounded number of expired entries and their byte accounting.
    fn reclaim_expired(registry: &mut FenceRegistry, now: Instant, max: usize) -> usize {
        let expired = registry
            .retained
            .iter()
            .filter_map(|(fence_id, retained)| (retained.expires_at <= now).then_some(*fence_id))
            .take(max)
            .collect::<Vec<_>>();
        for fence_id in &expired {
            if let Some(retained) = registry.retained.remove(fence_id) {
                registry.retained_bytes = registry
                    .retained_bytes
                    .saturating_sub(retained.retained_bytes);
            }
        }
        if !expired.is_empty() {
            metrics::counter!("bifrost_tail_fences_total", "outcome" => "expired")
                .increment(expired.len() as u64);
        }
        expired.len()
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
    pub start_day: EventDay,
    /// Inclusive last partition day governed by the query.
    pub end_day: EventDay,
    /// Emit only records with `LSN > after_lsn`.
    pub after_lsn: WalLsn,
    /// Columns required by Oracle filters, ordering, tripwire, and projection.
    pub required_columns: Vec<String>,
}

impl FetchLiveTailRequest {
    /// Return the fixed shard selected by the tenant/table route.
    #[must_use]
    pub fn shard_id(&self) -> usize {
        shard_for(self.binding.tenant, &self.binding.table_ref)
    }
}

/// One shallow, structural hot snapshot returned by a shard owner.
#[derive(Debug, Clone)]
pub struct HotBatch {
    /// Exact partition day owning the batch.
    pub partition_day: EventDay,
    /// WAL LSN of the append.
    pub wal_lsn: WalLsn,
    /// Idempotency identity of the append.
    pub batch_id: [u8; 16],
    /// Arrow rows projected to the request's required columns.
    pub rows: arrow::record_batch::RecordBatch,
}

/// Arrow IPC bytes for one complete admitted frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrowIpcBatch {
    /// LSN of the append on the target stream.
    pub lsn: WalLsn,
    /// Idempotency batch ID copied from the append metadata.
    pub batch_id: [u8; 16],
    /// Arrow IPC-encoded `RecordBatch` bytes.
    pub arrow_ipc: Vec<u8>,
}

/// Terminal and data frames for one bounded tail fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TailFrame {
    /// One complete, atomically emitted admitted frame.
    Batch(ArrowIpcBatch),
    /// All currently readable records after the requested LSN were emitted.
    Complete,
    /// The caller should resume strictly after this emitted LSN.
    Exhausted { resume_after_lsn: WalLsn },
}

/// Configuration for a live-tail reader.
#[derive(Debug, Clone, Copy)]
pub struct TailConfig {
    /// Maximum encoded Arrow bytes emitted before a terminal frame.
    pub max_bytes: usize,
}

impl Default for TailConfig {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Pod-local live-tail service over the Scribe memtable.
#[derive(Debug)]
pub struct FetchLiveTailService {
    stream: StreamIdentity,
    memtable: Option<Arc<Memtable>>,
    shards: Option<Arc<ScribeShardRuntime>>,
    config: TailConfig,
}

impl FetchLiveTailService {
    /// Construct a reader with the default bounded response size.
    #[must_use]
    pub fn new(stream: StreamIdentity, memtable: Arc<Memtable>) -> Self {
        Self::with_config(stream, memtable, TailConfig::default())
    }

    /// Construct a reader with an explicit byte budget.
    #[must_use]
    pub fn with_config(
        stream: StreamIdentity,
        memtable: Arc<Memtable>,
        config: TailConfig,
    ) -> Self {
        Self {
            stream,
            memtable: Some(memtable),
            shards: None,
            config: TailConfig {
                max_bytes: config.max_bytes.max(1),
            },
        }
    }

    /// Construct a production reader that submits snapshots to the owning
    /// shard command queue instead of traversing Scribe state directly.
    #[must_use]
    pub(crate) fn with_runtime(
        stream: StreamIdentity,
        shards: Arc<ScribeShardRuntime>,
        config: TailConfig,
    ) -> Self {
        Self {
            stream,
            memtable: None,
            shards: Some(shards),
            config: TailConfig {
                max_bytes: config.max_bytes.max(1),
            },
        }
    }

    /// Stream identity this service serves.
    #[must_use]
    pub fn stream(&self) -> StreamIdentity {
        self.stream
    }

    /// Return the canonical pod-local shard for a live-tail scope.
    #[must_use]
    pub fn shard_id(&self, request: &FetchLiveTailRequest) -> usize {
        request.shard_id()
    }

    /// Return the current writable and immutable batches for one shard.
    ///
    /// Stream, scope, day range, and projection validation happen before an
    /// Arrow response is encoded. Production calls pass through the bounded
    /// shard command queue; the direct memtable branch is only for the narrow
    /// in-process adapter used by unit tests.
    #[cfg(feature = "test-support")]
    pub async fn fetch_live_tail(
        &self,
        req: FetchLiveTailRequest,
    ) -> Result<Vec<TailFrame>, ScribeError> {
        if req.target_stream != self.stream {
            return Err(ScribeError::StreamMismatch {
                requested: req.target_stream,
                actual: self.stream,
            });
        }

        if req.start_day > req.end_day {
            return Err(ScribeError::Internal {
                detail: "live-tail start day is after end day".to_owned(),
            });
        }

        let hot_batches = if let Some(shards) = &self.shards {
            shards.snapshot(req.clone()).await?
        } else {
            self.memtable
                .as_ref()
                .ok_or_else(|| ScribeError::Internal {
                    detail: "direct tail memtable is not configured".to_owned(),
                })?
                .readable_batches_for_range(
                    req.binding.tenant,
                    &req.binding.table_ref,
                    req.start_day,
                    req.end_day,
                    &req.required_columns,
                )?
                .into_iter()
                .map(|readable| HotBatch {
                    partition_day: readable.partition_day,
                    wal_lsn: readable.meta.wal_lsn_max,
                    batch_id: readable.meta.batch_id,
                    rows: readable.batch,
                })
                .collect()
        };
        self.encode_frames(hot_batches, req.after_lsn, req.binding.tenant)
    }

    /// Return direct local Arrow handles for Oracle's `MemoryExec` path.
    pub async fn fetch_hot_batches(
        &self,
        request: FetchLiveTailRequest,
    ) -> Result<Vec<HotBatch>, ScribeError> {
        if request.target_stream != self.stream {
            return Err(ScribeError::StreamMismatch {
                requested: request.target_stream,
                actual: self.stream,
            });
        }
        if request.start_day > request.end_day {
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
                request.start_day,
                request.end_day,
                &request.required_columns,
            )?
            .into_iter()
            .map(|readable| HotBatch {
                partition_day: readable.partition_day,
                wal_lsn: readable.meta.wal_lsn_max,
                batch_id: readable.meta.batch_id,
                rows: readable.batch,
            })
            .collect())
    }

    fn encode_frames(
        &self,
        readable: Vec<HotBatch>,
        after_lsn: WalLsn,
        tenant: DataTenantId,
    ) -> Result<Vec<TailFrame>, ScribeError> {
        let mut candidates = Vec::new();
        for readable_batch in readable {
            let lsn = readable_batch.wal_lsn;
            if after_lsn != WalLsn::ZERO && lsn <= after_lsn {
                continue;
            }
            let arrow_ipc = encode_arrow_batch(&readable_batch.rows, tenant)?;
            candidates.push(ArrowIpcBatch {
                lsn,
                batch_id: readable_batch.batch_id,
                arrow_ipc,
            });
        }
        candidates.sort_by_key(|batch| batch.lsn);
        let candidate_count = candidates.len();

        let mut frames = Vec::new();
        let mut bytes = 0usize;
        let mut last_emitted = None;
        for (index, batch) in candidates.into_iter().enumerate() {
            let batch_bytes = batch.arrow_ipc.len();
            if last_emitted.is_some() && bytes.saturating_add(batch_bytes) > self.config.max_bytes {
                frames.push(TailFrame::Exhausted {
                    resume_after_lsn: last_emitted.ok_or_else(|| ScribeError::Internal {
                        detail: "tail budget exhausted without a resume LSN".to_owned(),
                    })?,
                });
                return Ok(frames);
            }

            bytes = bytes.saturating_add(batch_bytes);
            last_emitted = Some(batch.lsn);
            frames.push(TailFrame::Batch(batch));

            if index == candidate_count.saturating_sub(1) {
                frames.push(TailFrame::Complete);
                return Ok(frames);
            }
        }

        frames.push(TailFrame::Complete);
        Ok(frames)
    }
}

fn encode_arrow_batch(
    batch: &arrow::record_batch::RecordBatch,
    tenant: DataTenantId,
) -> Result<Vec<u8>, ScribeError> {
    let tenant_column =
        batch
            .column_by_name("data_tenant_id")
            .ok_or_else(|| ScribeError::Internal {
                detail: "live-tail batch is missing data_tenant_id".to_owned(),
            })?;
    let tenant_values = tenant_column
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| ScribeError::Internal {
            detail: "live-tail data_tenant_id must be Utf8".to_owned(),
        })?;
    let expected = tenant.to_string();
    for row in 0..tenant_values.len() {
        if tenant_values.is_null(row) || tenant_values.value(row) != expected {
            return Err(ScribeError::Internal {
                detail: format!("live-tail batch contains a row outside tenant {tenant}"),
            });
        }
    }

    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &batch.schema()).map_err(|error| {
        ScribeError::Internal {
            detail: format!("live-tail Arrow IPC writer init failed: {error}"),
        }
    })?;
    writer.write(batch).map_err(|error| ScribeError::Internal {
        detail: format!("live-tail Arrow IPC write failed: {error}"),
    })?;
    writer.finish().map_err(|error| ScribeError::Internal {
        detail: format!("live-tail Arrow IPC finish failed: {error}"),
    })?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use super::{FetchLiveTailService, ScribeTailReader, TailFenceConfig, cursor_cmp};
    use crate::scribe::memtable::Memtable;
    use crate::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::vala::api::{
        AcquireTailFenceRequest, EventDay, SchemaFingerprint, TailCursor, TenantTableBinding,
    };

    /// Builds one valid empty-interval request for opportunistic registry tests.
    fn empty_fence_request(tenant: DataTenantId) -> AcquireTailFenceRequest {
        AcquireTailFenceRequest {
            binding: TenantTableBinding {
                tenant_id: tenant,
                namespace: "bifrost".to_owned(),
                table: "events".to_owned(),
            },
            event_day: EventDay::new("2026-07-14").expect("fixture day"),
            exclusive_sealed: TailCursor {
                writer_epoch: 1,
                wal_lsn: 0,
                batch_id: uuid::Uuid::nil(),
                row_ordinal: 0,
            },
            deadline: chrono::Utc::now() + chrono::Duration::seconds(5),
            schema_fingerprint: SchemaFingerprint::new("empty-schema")
                .expect("fixture fingerprint"),
            tail_protocol_version: 1,
        }
    }

    /// Rejects cursor ordering across two independent writer epochs.
    #[test]
    fn cursor_rejects_cross_epoch_comparison() {
        let first = TailCursor {
            writer_epoch: 1,
            wal_lsn: 4,
            batch_id: uuid::Uuid::nil(),
            row_ordinal: 0,
        };
        let second = TailCursor {
            writer_epoch: 2,
            wal_lsn: 1,
            batch_id: uuid::Uuid::nil(),
            row_ordinal: 0,
        };
        assert!(cursor_cmp(&first, &second).is_err());
    }

    /// Reclaims abandoned expired capacity during a later production acquisition.
    #[tokio::test]
    async fn opportunistic_acquire_reclaims_expired_capacity() {
        let tenant = DataTenantId::new_v7();
        let stream = StreamIdentity::new(NodeId::generate(), WriterEpoch::new(1));
        let reader = ScribeTailReader::new(
            Arc::new(FetchLiveTailService::new(stream, Arc::new(Memtable::new()))),
            TailFenceConfig {
                max_fences: 1,
                ..TailFenceConfig::default()
            },
        );
        let first = reader
            .acquire_fence(empty_fence_request(tenant))
            .await
            .expect("first fence consumes capacity");
        {
            let mut registry = reader.fences.lock().expect("registry lock");
            registry
                .retained
                .get_mut(&first.fence_id)
                .expect("first fence retained")
                .expires_at = Instant::now()
                .checked_sub(Duration::from_secs(1))
                .expect("one second is within the monotonic clock range");
        }

        let second = reader
            .acquire_fence(empty_fence_request(tenant))
            .await
            .expect("foreground acquisition reclaims expired capacity");
        let registry = reader.fences.lock().expect("registry lock");
        assert_eq!(registry.retained.len(), 1);
        assert!(!registry.retained.contains_key(&first.fence_id));
        assert!(registry.retained.contains_key(&second.fence_id));
    }
}
