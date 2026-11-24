//! Blocking preprocessing for admitted appends.

use crate::scribe::wal::{ContentDigest, ContentHasher};
use arrow::array::{Array, ArrayData};
use arrow::buffer::Buffer;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use iceberg::spec::Schema;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::oneshot;
use uuid::Uuid;
use wyrd_spec::request_id::RequestId;

use crate::catalog::PartitionScheme;
use crate::catalog::PhysicalPartition;
use crate::catalog::TableRef;
use crate::contracts::ScribeError;
use crate::resources::ScribeMemoryLease;
use crate::schema::SchemaFingerprint;
use crate::scribe::admission::InflightFrameReservation;
use crate::scribe::admission::REQUEST_OVERHEAD_BYTES;
use crate::scribe::memory::MemoryCategory;
use crate::scribe::profile::WriteProfile;
use crate::scribe::seal_key::{PartitionPlan, SealKey, plan_partitions, split_batch_by_partition};
use crate::scribe::wal::PreparedWalAppend;
use wyrd_spec::ids::DataTenantId;

/// The successful terminal a shard owner sends to one admitted append's waiter.
///
/// Sent only after the append's rows are query-visible, after the durable
/// fence suppressed them as an already-committed replay of the same batch, or
/// while the shard holds its group for [`AcceptedUnder`] verification.
#[derive(Debug)]
pub(crate) struct DurableCompletion {
    /// Rows the append carried, whether inserted now or already visible.
    pub rows: u64,
    /// Whether this append's group inserted the batch rather than suppressing
    /// a replay the fence or memtable identity showed was already committed.
    pub first_commit: bool,
    /// The durable fence's claim that another table version already spent
    /// this batch id.
    ///
    /// Such an append's rows were suppressed and nothing is acknowledged yet:
    /// the caller decides whether it is that batch by preparing it under the
    /// accepting version, and the shard holds the append's tenant group
    /// uncommitted until it answers.
    pub accepted_under: Option<AcceptedUnder>,
    /// When the shard owner sent this completion, so the waiter times its
    /// delivery.
    pub sent: Instant,
}

/// A batch id the durable fence accepted under another table version, held
/// open until the caller proves or refuses its retained input as that batch.
///
/// The shard has rolled back its tenant group's fresh fences and waits on
/// `verdict` outside any SQL transaction: `true` resumes fencing without this
/// batch, while `false` or a dropped sender fails the group, so a
/// contradiction commits none of its fresh peers.
#[derive(Debug)]
pub(crate) struct AcceptedUnder {
    /// Table version whose fence accepted the batch id.
    pub version: wyrd_spec::vala::ids::TableVersion,
    /// Answered `true` only when the retained input is exactly that batch.
    pub verdict: oneshot::Sender<bool>,
}

/// A complete request accepted by pod-global admission.
#[derive(Debug)]
pub(crate) struct AdmittedAppend {
    /// Stable batch identity keying every produced slice and the commit fence.
    pub batch_id: Uuid,
    /// Request correlation retained for the durable batch-commit fence.
    pub request_id: Uuid,
    /// Admission instant stamped as `wyrd_ingested_at` and stored on the fence.
    pub ingested_at_micros: i64,
    pub rows: AdmittedRows,
    pub measured_wire_bytes: usize,
    pub reservation: InflightFrameReservation,
    pub memory: ScribeMemoryLease,
    pub tenant: DataTenantId,
    pub table: TableRef,
    /// Registered time axis every slice of this append is bucketed by.
    pub partition_scheme: PartitionScheme,
    pub queued_at: Instant,
    /// Completion channel answered once the append is durable in the WAL, or
    /// with the typed error that refused it; `None` for fire-and-forget appends.
    pub durable_ack: Option<oneshot::Sender<Result<DurableCompletion, ScribeError>>>,
    /// The request's write profile, shared with its ingress waiter and
    /// emitted once both have dropped it.
    pub profile: Arc<WriteProfile>,
}

/// Root-owned row source entering current-only persistence preprocessing.
#[derive(Debug)]
pub(crate) enum AdmittedRows {
    /// One already-stamped projected batch.
    Projected(RecordBatch),
    /// One native stream decoded a record batch at a time on the persistence lane.
    Native(Box<NativeAdmittedRows>),
}

/// Retained native source and immutable stamping context.
#[derive(Debug, Clone)]
pub(crate) struct NativeAdmittedRows {
    /// Transport-owned Arrow IPC stream, decoded zero-copy where aligned.
    pub(crate) bytes: Bytes,
    /// Source of the trusted subject/actor pair stamped as managed
    /// correlation columns.
    pub(crate) attribution: crate::contracts::StampedAttribution,
    /// Verified workload Service release a frame-attributed write stamps.
    pub(crate) workload: Option<wyrd_spec::ids::CardUid>,
    /// Card UIDs every row's `subject_card_uid` is authorized against: the frame's
    /// attributed UIDs, or the principal's signed scope.
    pub(crate) card_scope: Option<Vec<wyrd_spec::ids::CardUid>>,
    /// Accepted context the shared Service, environment, and subject
    /// dimensions are stamped from; `None` keeps each row's own values.
    pub(crate) dimensions: Option<crate::tables::FrameDimensions>,
    /// Caller-supplied fingerprint validated independently for every source
    /// batch, when one was supplied.
    pub(crate) expected_schema_fingerprint: Option<SchemaFingerprint>,
    /// Request identity stamped into every source batch.
    pub(crate) request_id: RequestId,
    /// Stable batch identity keying every produced slice.
    pub(crate) batch_id: Uuid,
    /// One receipt instant reused by planning, projection, and WAL identity.
    pub(crate) receipt_micros: i64,
    /// Canonical built-in whose physical identity every source must preserve.
    pub(crate) definition: Option<&'static crate::tables::BuiltinTableDefinition>,
    /// Registered Iceberg schema whose field ids every stamped batch carries.
    ///
    /// `None` only for the embedded engine seam, which has no catalog owner
    /// and writes objects that no registered table promotes.
    pub(crate) registered_schema: Option<Arc<Schema>>,
    /// The ingress lane's remembered conform plans, shared so a native source
    /// conforms each batch as the lane's decode does.
    pub(crate) conform: Arc<crate::scribe::execution_lanes::ConformPlans>,
    /// Expanded-data ceiling the decoded, stamped output must fit before WAL.
    pub(crate) expanded_limit_bytes: usize,
}

/// A request after deterministic time-partition splitting and serialization.
#[derive(Debug)]
pub(crate) struct PreparedAppend {
    /// Stable batch identity keying every produced slice and the commit fence.
    pub batch_id: Uuid,
    /// Admission instant stamped as `wyrd_ingested_at` and stored on the fence.
    pub ingested_at_micros: i64,
    pub tenant: DataTenantId,
    pub table: TableRef,
    pub prepared_bytes: usize,
    pub slices: PreparedSliceSet,
    pub reservation: InflightFrameReservation,
    pub memory: Option<ScribeMemoryLease>,
    /// Completion channel carried from the queued append, answered once the
    /// prepared frame is durable or refused; `None` for fire-and-forget appends.
    pub durable_ack: Option<oneshot::Sender<Result<DurableCompletion, ScribeError>>>,
    /// The request's write profile, charged its shard wait and group stages
    /// by the shard that acknowledges it.
    pub profile: Arc<WriteProfile>,
    /// When preprocessing finished; the shard wait runs from here until the
    /// append's group starts.
    pub prepared_at: Instant,
}

impl PreparedAppend {
    /// The table version every slice of this batch was prepared under, or
    /// `None` for a batch with no slice.
    ///
    /// Preprocessing plans every slice from one partition scheme, so the
    /// first slice's seal key speaks for the batch.
    pub(crate) fn table_version(&self) -> Option<wyrd_spec::vala::ids::TableVersion> {
        let PreparedSliceSet::Materialized(slices) = &self.slices;
        slices.first().map(|slice| slice.seal_key.table_version)
    }

    /// The batch's logical slice-set digest, slice count, and row count, as
    /// the shard computes them for the durable fence.
    ///
    /// Slices are folded in ordinal order through [`LogicalBatchDigest`], so
    /// an honest retry reproduces the digest its first attempt fenced.
    /// Returns `None` when the set is not exactly the ordinals `0..count` or a
    /// count does not fit its field.
    pub(crate) fn logical_identity(&self) -> Option<(ContentDigest, u32, u64)> {
        let PreparedSliceSet::Materialized(slices) = &self.slices;
        let mut ordered: Vec<&PreparedSlice> = slices.iter().collect();
        ordered.sort_unstable_by_key(|slice| slice.id.slice_index);
        let mut logical = LogicalBatchDigest::new();
        let mut rows = 0_u64;
        for (ordinal, slice) in ordered.iter().enumerate() {
            if usize::try_from(slice.id.slice_index).ok()? != ordinal {
                return None;
            }
            let wal = &slice.wal_append;
            logical.push_slice(
                slice.id.slice_index,
                &wal.schema_fingerprint,
                &wal.logical_data_digest,
                wal.logical_data_len,
            );
            rows = rows.checked_add(u64::try_from(slice.rows.num_rows()).ok()?)?;
        }
        Some((logical.finish(), u32::try_from(ordered.len()).ok()?, rows))
    }
}

/// Closed prepared-slice source used by the shard owner.
#[derive(Debug)]
pub(crate) enum PreparedSliceSet {
    /// The complete once-materialized slice set retained through visibility.
    Materialized(Vec<PreparedSlice>),
}

/// Stateful current-only native slice producer.
#[derive(Debug)]
pub(crate) struct NativeSliceProducer {
    /// Deterministic retained source and stamping context.
    source: NativeAdmittedRows,
    /// Arrow's push decoder, which validates every batch it yields.
    ///
    /// Left at Arrow's default alignment policy, it aliases aligned buffers
    /// and copies only an unaligned buffer into an aligned allocation.
    decoder: arrow::ipc::reader::StreamDecoder,
    /// Undecoded remainder of the transport stream.
    pending: Buffer,
    /// Current decoded source retained only across its partition slices.
    current: Option<NativeCurrentSource>,
    /// Request correlation stamped on every produced slice.
    request_id: Uuid,
    /// Zero-based ordinal assigned to the next slice.
    slice_index: u32,
    /// Exact count established by the non-retaining first pass.
    slice_count: u32,
    /// Running expanded bytes of the decoded batches, checked against the
    /// expanded ceiling.
    output_bytes: usize,
    /// Authenticated tenant used by every produced seal key.
    tenant: DataTenantId,
    /// Logical table used by every produced seal key.
    table: TableRef,
    /// Registered time axis used to bucket every produced slice.
    partition_scheme: PartitionScheme,
    /// Sorted distinct partitions of every source decoded so far.
    partitions: Vec<PhysicalPartition>,
}

/// One decoded native source and its fixed time-partition cursor.
#[derive(Debug)]
struct NativeCurrentSource {
    /// Stamped current source rows.
    rows: RecordBatch,
    /// Immutable current-source partition descriptors.
    partitions: PartitionPlan,
    /// Next partition descriptor to materialize.
    next_partition: usize,
}

impl NativeSliceProducer {
    /// Builds a producer positioned at the start of the native stream.
    ///
    /// The stream bytes are shared with the decoder by reference count, so no
    /// transport bytes are copied here.
    fn new(
        source: NativeAdmittedRows,
        request_id: Uuid,
        tenant: DataTenantId,
        table: TableRef,
        partition_scheme: PartitionScheme,
    ) -> Self {
        let pending = Buffer::from(Bytes::clone(&source.bytes));
        Self {
            source,
            decoder: arrow::ipc::reader::StreamDecoder::new(),
            pending,
            current: None,
            request_id,
            slice_index: 0,
            slice_count: 0,
            output_bytes: 0,
            tenant,
            table,
            partition_scheme,
            partitions: Vec::new(),
        }
    }

    /// Decodes the next record batch from the remaining stream bytes.
    ///
    /// Arrow consumes the schema message on the first call and yields one
    /// validated batch per call. Once every byte is consumed, Arrow's `finish`
    /// refuses a stream that stopped inside a message.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when Arrow refuses the stream:
    /// malformed framing or metadata, a batch that fails validation, bytes after
    /// end-of-stream, or a truncated final message.
    fn decode_next(&mut self) -> Result<Option<RecordBatch>, ScribeError> {
        while !self.pending.is_empty() {
            // ponytail: arrow-ipc 59 panics instead of erring on some malformed
            // input (arrow-rs #10647, fixed in 60.0.0; #11044, unreleased). The
            // panic is caller input, so it maps to the same refusal as an Arrow
            // error. Delete the catch once the pinned arrow carries both fixes.
            let decoded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.decoder.decode(&mut self.pending)
            }));
            if let Some(rows) = decoded
                .map_err(|_| ScribeError::InvalidFrame)?
                .map_err(|_| ScribeError::InvalidFrame)?
            {
                return Ok(Some(rows));
            }
        }
        self.decoder
            .finish()
            .map_err(|_| ScribeError::InvalidFrame)?;
        Ok(None)
    }

    /// Produces one exact-capacity current slice and advances its owner state.
    ///
    /// Each decoded batch's expanded bytes are added to the running request
    /// total before it is stamped or sliced, so a request above the expanded
    /// ceiling is refused before WAL.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when Arrow refuses the stream, stamping
    /// or day materialization fails, or Arrow IPC encoding fails, and
    /// [`ScribeError::DecodedPayloadTooLarge`] when actual output exceeds the
    /// expanded ceiling.
    pub(crate) fn next_slice(
        &mut self,
        encoders: &EncoderContexts,
    ) -> Result<Option<PreparedSlice>, ScribeError> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if current.next_partition < current.partitions.len() {
                    let (partition, rows) = current
                        .partitions
                        .materialize(&current.rows, current.next_partition)?;
                    current.next_partition += 1;
                    let mut slice = prepare_slice(
                        SliceContext {
                            encoders,
                            batch_id: self.source.batch_id,
                            request_id: self.request_id,
                            tenant: self.tenant,
                            table: &self.table,
                            table_version: self.partition_scheme.table_version(),
                        },
                        partition,
                        rows,
                    )?;
                    slice
                        .wal_append
                        .assign_slice_ordinal(self.slice_index, self.slice_count);
                    slice.id.slice_index = self.slice_index;
                    self.slice_index =
                        self.slice_index
                            .checked_add(1)
                            .ok_or_else(|| ScribeError::Internal {
                                detail: "native slice ordinal overflow".to_owned(),
                            })?;
                    return Ok(Some(slice));
                }
                self.current = None;
            }
            let Some(rows) = self.decode_next()? else {
                return Ok(None);
            };
            self.output_bytes = self
                .output_bytes
                .checked_add(crate::scribe::material_plan::expanded_bytes(&rows)?)
                .ok_or(ScribeError::DecodedPayloadTooLarge {
                    bytes: usize::MAX,
                    limit: self.source.expanded_limit_bytes,
                })?;
            if self.output_bytes > self.source.expanded_limit_bytes {
                return Err(ScribeError::DecodedPayloadTooLarge {
                    bytes: self.output_bytes,
                    limit: self.source.expanded_limit_bytes,
                });
            }
            let rows = stamp_native_source(&rows, &self.source)?;
            let partitions = plan_partitions(&rows, &self.partition_scheme)?;
            partitions.union_into(&mut self.partitions)?;
            self.current = Some(NativeCurrentSource {
                rows,
                partitions,
                next_partition: 0,
            });
        }
    }
}

impl PreparedSliceSet {
    /// Returns the exact remaining closed-set size without advancing the source.
    ///
    pub(crate) fn exact_slice_capacity(&self) -> usize {
        match self {
            Self::Materialized(slices) => slices.len(),
        }
    }
}

/// Applies deterministic native contract validation and managed stamping.
///
/// # Errors
///
/// Returns the stable fingerprint, scope, event-time, or managed-column error
/// produced by the native decode contract.
fn stamp_native_source(
    rows: &RecordBatch,
    source: &NativeAdmittedRows,
) -> Result<RecordBatch, ScribeError> {
    crate::scribe::execution_lanes::decode_native_batch(
        rows,
        &crate::scribe::execution_lanes::DecodeContext {
            attribution: source.attribution,
            workload: source.workload.as_ref(),
            card_scope: source.card_scope.as_deref(),
            dimensions: source.dimensions.as_ref(),
            expected_schema_fingerprint: source.expected_schema_fingerprint,
            request_id: &source.request_id,
            receipt_micros: source.receipt_micros,
            definition: source.definition,
            registered_schema: source.registered_schema.as_ref(),
            conform: Some(&source.conform),
        },
    )
}

/// One event-day slice ready for ordered WAL and memtable processing.
#[derive(Debug)]
pub(crate) struct PreparedSlice {
    pub id: AppendSliceId,
    pub seal_key: SealKey,
    /// Request correlation the terminal COMMIT record persists.
    pub request_id: Uuid,
    pub rows: RecordBatch,
    pub wal_append: PreparedWalAppend,
    pub memtable_bytes: usize,
}

/// Folds one Arrow array's logical bytes into `digest` and its length.
///
/// The walk is deliberately buffer-level rather than value-level: it covers
/// length, offset, the validity bitmap, every data buffer, and every child
/// recursively, so two batches agree only when their Arrow representation
/// agrees byte-for-byte. `bytes` accumulates the contributed buffer length the
/// WAL-v8 metadata records alongside the digest.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the accumulated buffer length
/// overflows its `usize` accumulator.
fn digest_array_data(
    data: &ArrayData,
    digest: &mut ContentHasher,
    bytes: &mut usize,
) -> Result<(), ScribeError> {
    digest.update(data.len().to_le_bytes());
    digest.update(data.offset().to_le_bytes());
    if let Some(nulls) = data.nulls() {
        let buffer = nulls.buffer().as_slice();
        digest.update(buffer);
        *bytes = bytes
            .checked_add(buffer.len())
            .ok_or_else(|| ScribeError::Internal {
                detail: "logical Arrow identity length overflow".to_owned(),
            })?;
    }
    for buffer in data.buffers() {
        let buffer = buffer.as_slice();
        digest.update(buffer);
        *bytes = bytes
            .checked_add(buffer.len())
            .ok_or_else(|| ScribeError::Internal {
                detail: "logical Arrow identity length overflow".to_owned(),
            })?;
    }
    for child in data.child_data() {
        digest_array_data(child, digest, bytes)?;
    }
    Ok(())
}

/// Reports whether `name` is a managed column a legitimate retry restamps
/// with a fresh value: the request and ingestion-time columns.
///
/// The managed correlation columns are not volatile: an honest retry of one
/// accepted batch carries the same workload, subject, Run, and account pair,
/// so they belong to what the rows are.
fn is_volatile_column(name: &str) -> bool {
    crate::tables::managed_columns::is_managed_column(name)
        && !crate::tables::managed_columns::is_correlation_column(name)
}

/// Reports whether `batch_id` was derived from its rows' content.
///
/// Gate derives each OTLP batch id from the XXH3-128 logical identity of the
/// accepted rows and marks it UUID version 8; client ids must be version 7.
/// Such an id is the content, so its slices carry an empty logical identity
/// (zero digest and length) on the live path and in replay alike, and a retry
/// is recognised by id alone.
pub(crate) fn is_content_derived(batch_id: Uuid) -> bool {
    batch_id.get_version() == Some(uuid::Version::Custom)
}

/// Computes the stable logical Arrow identity used only for retained retries.
///
/// Server-generated request and time columns change across a legitimate client
/// retry, so they are excluded. The managed correlation columns are bound, so
/// a batch id replayed under another workload, subject, Run, or account pair
/// is a different batch rather than a retry. Schema identity remains an
/// independent retained fact, while every other Arrow buffer participates
/// byte-for-byte.
///
/// # Errors
///
/// Returns an internal error when the stable buffer length exceeds its WAL-v8
/// metadata representation.
pub(crate) fn logical_data_identity(
    rows: &RecordBatch,
) -> Result<(ContentDigest, u32), ScribeError> {
    let mut digest = ContentHasher::new();
    let mut bytes = 0_usize;
    for (field, column) in rows.schema().fields().iter().zip(rows.columns()) {
        // The request and time columns describe *which* attempt carried the
        // rows, not what they are: they are stamped fresh on every attempt, so
        // binding them would make an honest client retry of the same batch
        // look like a different batch. The set is the managed-column
        // declaration itself, so a newly managed column enters this identity
        // unless it is declared request- or time-scoped.
        if is_volatile_column(field.name()) {
            continue;
        }
        digest.update(field.name().as_bytes());
        let before = bytes;
        digest_array_data(&column.to_data(), &mut digest, &mut bytes)?;
        if tracing::enabled!(tracing::Level::DEBUG) {
            let mut column_digest = ContentHasher::new();
            let mut column_bytes = 0_usize;
            digest_array_data(&column.to_data(), &mut column_digest, &mut column_bytes)?;
            tracing::debug!(
                column = field.name().as_str(),
                digest = %hex_digest(&column_digest.finish()),
                bytes = bytes.saturating_sub(before),
                "logical batch identity column contribution"
            );
        }
    }
    let bytes = u32::try_from(bytes).map_err(|_| ScribeError::Internal {
        detail: "logical Arrow identity exceeds its u32 bound".to_owned(),
    })?;
    Ok((digest.finish(), bytes))
}

/// Idempotency identity for one logical slice within a routed batch.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppendSliceId {
    /// Stable logical batch identity supplied by the caller.
    pub batch_id: Uuid,
    /// Canonical event-day route for this slice.
    pub seal_key: SealKey,
    /// Canonical zero-based ordinal within the complete logical batch.
    pub slice_index: u32,
}

/// Move-only context used to convert admitted rows into their prepared source.
struct PreparedRowsContext {
    /// Stable logical batch identity.
    batch_id: Uuid,
    /// Request correlation stamped on every produced slice.
    request_id: Uuid,
    /// Authenticated tenant used by every seal key.
    tenant: DataTenantId,
    /// Logical table used by every seal key.
    table: TableRef,
    /// Registered time axis used to bucket every slice.
    partition_scheme: PartitionScheme,
    /// Root bytes retained while the prepared source is live.
    memory_bytes: usize,
}

/// Converts admitted rows into a materialized or current-only prepared source.
///
/// Native and OTLP requests retain their move-only producer state; only the
/// engine fixture path materializes its complete closed slice set here.
///
/// # Errors
///
/// Returns a stable day-planning, native decode, IPC, or checked-size
/// refusal while the caller still owns the root lease.
fn prepare_rows(
    rows: AdmittedRows,
    context: PreparedRowsContext,
    encoders: &EncoderContexts,
) -> Result<(PreparedSliceSet, usize), ScribeError> {
    let PreparedRowsContext {
        batch_id,
        request_id,
        tenant,
        table,
        partition_scheme,
        memory_bytes,
    } = context;
    match rows {
        AdmittedRows::Projected(rows) => {
            let mut slices = Vec::new();
            append_prepared_slices(
                &mut slices,
                SliceContext {
                    encoders,
                    batch_id,
                    request_id,
                    tenant,
                    table: &table,
                    table_version: partition_scheme.table_version(),
                },
                &partition_scheme,
                &rows,
            )?;
            let slice_count = u32::try_from(slices.len()).map_err(|_| ScribeError::Internal {
                detail: "Scribe batch exceeds the v8 WAL slice-count bound".to_owned(),
            })?;
            for (slice_index, slice) in slices.iter_mut().enumerate() {
                let slice_index =
                    u32::try_from(slice_index).map_err(|_| ScribeError::Internal {
                        detail: "Scribe batch slice index exceeds the v8 WAL bound".to_owned(),
                    })?;
                slice
                    .wal_append
                    .assign_slice_ordinal(slice_index, slice_count);
                slice.id.slice_index = slice_index;
            }
            let prepared_bytes = prepared_slice_bytes(&slices, memory_bytes)?;
            Ok((PreparedSliceSet::Materialized(slices), prepared_bytes))
        }
        AdmittedRows::Native(native) => {
            let mut producer =
                NativeSliceProducer::new(*native, request_id, tenant, table, partition_scheme);
            let mut slices = Vec::new();
            while let Some(slice) = producer.next_slice(encoders)? {
                slices.push(slice);
            }
            if slices.is_empty() {
                return Err(ScribeError::InvalidFrame);
            }
            assign_slice_ordinals(&mut slices)?;
            let prepared_bytes = prepared_slice_bytes(&slices, memory_bytes)?;
            Ok((PreparedSliceSet::Materialized(slices), prepared_bytes))
        }
    }
}

/// Assigns final WAL ordinals after the single materialization traversal.
///
/// # Errors
///
/// Returns an internal error when the slice set exceeds WAL v8 integer bounds.
fn assign_slice_ordinals(slices: &mut [PreparedSlice]) -> Result<(), ScribeError> {
    let count = u32::try_from(slices.len()).map_err(|_| ScribeError::Internal {
        detail: "Scribe batch exceeds the v8 WAL slice-count bound".to_owned(),
    })?;
    for (index, slice) in slices.iter_mut().enumerate() {
        let index = u32::try_from(index).map_err(|_| ScribeError::Internal {
            detail: "Scribe batch slice index exceeds the v8 WAL bound".to_owned(),
        })?;
        slice.wal_append.assign_slice_ordinal(index, count);
        slice.id.slice_index = index;
    }
    Ok(())
}

/// Split and serialize an admitted append on the bounded pre-ACK CPU lane.
///
/// The returned `prepared_bytes` is the exact retained memtable-plus-WAL set;
/// the ingress owner charges it on the shared root before WAL/ACK.
///
/// # Errors
///
/// Returns the preparation or category-transition error after notifying the
/// durable waiter.
pub(crate) fn prepare_append(
    admitted: AdmittedAppend,
    encoders: &EncoderContexts,
) -> Result<PreparedAppend, ScribeError> {
    let AdmittedAppend {
        batch_id,
        request_id,
        ingested_at_micros,
        rows,
        measured_wire_bytes: _measured_wire_bytes,
        reservation,
        mut memory,
        tenant,
        table,
        partition_scheme,
        queued_at,
        mut durable_ack,
        profile,
    } = admitted;

    metrics::histogram!("bifrost_scribe_queue_wait_seconds")
        .record(queued_at.elapsed().as_secs_f64());

    let slices_result = prepare_rows(
        rows,
        PreparedRowsContext {
            batch_id,
            request_id,
            tenant,
            table: table.clone(),
            partition_scheme,
            memory_bytes: memory.bytes(),
        },
        encoders,
    );
    let (slices, prepared_bytes) = match slices_result {
        Ok(result) => result,
        Err(error) => {
            notify_completion(&mut durable_ack, &error);
            return Err(error);
        }
    };
    if let Err(error) = memory.transfer_category(MemoryCategory::Prepared) {
        notify_completion(&mut durable_ack, &error);
        return Err(error);
    }

    Ok(PreparedAppend {
        batch_id,
        ingested_at_micros,
        tenant,
        table,
        prepared_bytes,
        slices,
        reservation,
        memory: Some(memory),
        durable_ack,
        profile,
        prepared_at: Instant::now(),
    })
}

/// Computes retained bytes for the existing materialized projected path.
///
/// # Errors
///
/// Returns [`ScribeError::DecodedPayloadTooLarge`] on checked arithmetic
/// overflow.
fn prepared_slice_bytes(slices: &[PreparedSlice], limit: usize) -> Result<usize, ScribeError> {
    slices.iter().try_fold(
        REQUEST_OVERHEAD_BYTES,
        |total, slice| -> Result<usize, ScribeError> {
            total
                .checked_add(slice.memtable_bytes)
                .and_then(|value| value.checked_add(slice.wal_append.data.len()))
                .ok_or(ScribeError::DecodedPayloadTooLarge {
                    bytes: usize::MAX,
                    limit,
                })
        },
    )
}

/// Splits and serializes only the current decoded source batch.
///
/// # Errors
///
/// Returns [`ScribeError`] when time-partition validation, Arrow IPC
/// serialization, or checked slice construction fails. Completed
/// earlier slices remain owned by `slices` and are dropped by the caller on
/// refusal.
fn append_prepared_slices(
    slices: &mut Vec<PreparedSlice>,
    context: SliceContext<'_>,
    partition_scheme: &PartitionScheme,
    rows: &RecordBatch,
) -> Result<(), ScribeError> {
    for slice in split_batch_by_partition(rows, partition_scheme)? {
        let (partition, partition_rows) = slice?;
        slices.push(prepare_slice(context, partition, partition_rows)?);
    }
    Ok(())
}

/// Borrowed immutable context shared by each current slice preparation.
#[derive(Clone, Copy)]
struct SliceContext<'a> {
    /// The lane's reusable IPC encoder contexts.
    encoders: &'a EncoderContexts,
    /// Stable logical batch identity.
    batch_id: Uuid,
    /// Request correlation stamped on the produced slice.
    request_id: Uuid,
    /// Authenticated tenant used by the seal key.
    tenant: DataTenantId,
    /// Logical table used by the seal key.
    table: &'a TableRef,
    /// Table version the seal key binds the slice to.
    table_version: wyrd_spec::vala::ids::TableVersion,
}

/// Builds one exact-capacity WAL slice from the current materialized day.
///
/// # Errors
///
/// Returns [`ScribeError`] when Arrow IPC encoding fails or the logical
/// identity cannot be represented.
fn prepare_slice(
    context: SliceContext<'_>,
    partition: PhysicalPartition,
    rows: RecordBatch,
) -> Result<PreparedSlice, ScribeError> {
    let seal_key = SealKey::new(context.tenant, context.table.clone(), partition)
        .with_table_version(context.table_version);
    let memtable_bytes = crate::scribe::memory::retained_arrow_bytes(&rows);
    let data_payload = context.encoders.encode(&rows)?;
    let identity_span = tracing::debug_span!(
        "scribe_logical_batch_identity",
        batch_id = %context.batch_id,
        table = %context.table.fqn(),
    );
    let (logical_data_digest, logical_data_len) = if is_content_derived(context.batch_id) {
        (ContentDigest::default(), 0)
    } else {
        identity_span.in_scope(|| logical_data_identity(&rows))?
    };
    let wal_append = PreparedWalAppend::new(
        crate::scribe::wal::WalLsn::ZERO,
        *context.batch_id.as_bytes(),
        data_payload,
    )
    .for_slice(
        seal_key.clone(),
        SchemaFingerprint::from_arrow_schema(&rows.schema()).0,
    )
    .with_logical_data_identity(logical_data_digest, logical_data_len);
    Ok(PreparedSlice {
        id: AppendSliceId {
            batch_id: context.batch_id,
            seal_key: seal_key.clone(),
            slice_index: 0,
        },
        seal_key,
        request_id: context.request_id,
        rows,
        wal_append,
        memtable_bytes,
    })
}
thread_local! {
    /// This thread's index among the ingress lane's workers; `None` on every
    /// other thread, including other Rayon pools' workers.
    static INGRESS_WORKER: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Marks the current thread as ingress lane worker `index`, the only threads
/// whose [`EncoderContexts`] slot is used; the ingress pool's start handler.
pub(crate) fn mark_ingress_worker(index: usize) {
    INGRESS_WORKER.set(Some(index));
}

/// One ingress worker's IPC write context, kept between streams, and the
/// `Metadata` lease charging the bytes it retains; it outlives every request,
/// so no request's `Prepared` charge carries it.
#[derive(Debug)]
struct RetainedEncoder {
    /// Compressor and scratch the next stream on this worker reuses.
    context: arrow::ipc::writer::IpcWriteContext,
    /// Charges exactly [`arrow::ipc::writer::IpcWriteContext::retained_bytes`].
    lease: ScribeMemoryLease,
}

/// The ingress lane's reusable slice encoder contexts: one slot per worker
/// thread, each charged to Scribe memory while it holds a context.
///
/// A context carries only compressor and scratch allocations; every stream
/// starts its own schema, dictionary and end-of-stream state, so its bytes
/// equal a freshly encoded stream's. A worker takes its slot's context out
/// for one encode and restores it only after the stream finishes, re-charged
/// at its measured size: a failed or panicking encode drops the context and
/// its lease with the encoder, a context larger than the cap or one the
/// memory root refuses is dropped, and [`Self::clear`] releases every slot
/// at shutdown. A caller outside the lane's workers, or the empty default,
/// encodes with a fresh context and keeps nothing.
#[derive(Debug, Default)]
pub(crate) struct EncoderContexts {
    /// One slot per lane worker, indexed by its [`mark_ingress_worker`]
    /// index; only that worker locks it, so the lock is never contended.
    slots: Box<[std::sync::Mutex<Option<RetainedEncoder>>]>,
    /// The memory root retained contexts are charged to.
    resources: Option<crate::resources::ScribeResources>,
    /// Largest retained context kept for reuse.
    cap: usize,
    /// Set by [`Self::clear`]; a closed set keeps nothing.
    closed: std::sync::atomic::AtomicBool,
}

impl EncoderContexts {
    /// Creates `workers` empty slots charged to `resources`, each keeping a
    /// context of at most `cap` retained bytes.
    pub(crate) fn new(
        workers: usize,
        resources: crate::resources::ScribeResources,
        cap: usize,
    ) -> Self {
        Self {
            slots: (0..workers).map(|_| std::sync::Mutex::new(None)).collect(),
            resources: Some(resources),
            cap,
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Encodes one slice as a standalone compressed Arrow IPC stream,
    /// joined into one exact-length payload the WAL writes in a single
    /// vectored write.
    ///
    /// The payload is byte for byte the stream Arrow's `StreamWriter` writes
    /// with the same options, whether or not the worker's context was reused,
    /// so every WAL reader still decodes it with Arrow's standard
    /// `StreamReader`. The stream stays within
    /// [`crate::scribe::material_plan::wal_payload_bound`], which canonical
    /// admission reserved; its length is charged with the slice.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when compression is unavailable,
    /// either Arrow step fails, or memory accounting is poisoned while the
    /// context is re-charged. A busy memory root only drops the context.
    pub(crate) fn encode(&self, rows: &RecordBatch) -> Result<Bytes, ScribeError> {
        let slot = INGRESS_WORKER.get().and_then(|index| self.slots.get(index));
        let retained = slot.and_then(|slot| slot.lock().ok()?.take());
        let (context, lease) = retained.map_or_else(
            || (arrow::ipc::writer::IpcWriteContext::default(), None),
            |retained| (retained.context, Some(retained.lease)),
        );
        let options = arrow::ipc::writer::IpcWriteOptions::default()
            .try_with_compression(Some(crate::scribe::material_plan::WAL_IPC_COMPRESSION))
            .and_then(|options| {
                options.try_with_compression_level(Some(
                    crate::scribe::material_plan::WAL_IPC_COMPRESSION_LEVEL,
                ))
            })
            .map_err(|error| ScribeError::Internal {
                detail: format!("Arrow IPC compression unavailable: {error}"),
            })?;
        let mut encoder = arrow::ipc::writer::StreamEncoder::try_new_with_context(
            &rows.schema(),
            options,
            context,
        )
        .map_err(|error| ScribeError::Internal {
            detail: format!("Arrow IPC encode init failed: {error}"),
        })?;
        let mut chunks = encoder
            .encode(rows)
            .map_err(|error| ScribeError::Internal {
                detail: format!("Arrow IPC encode failed: {error}"),
            })?;
        let (tail, context) =
            encoder
                .finish_with_context()
                .map_err(|error| ScribeError::Internal {
                    detail: format!("Arrow IPC encode finish failed: {error}"),
                })?;
        chunks.extend(tail);
        if let Some(slot) = slot {
            self.restore(slot, context, lease)?;
        }
        let mut payload = Vec::with_capacity(chunks.iter().map(arrow::buffer::Buffer::len).sum());
        for chunk in &chunks {
            payload.extend_from_slice(chunk.as_slice());
        }
        Ok(Bytes::from(payload))
    }

    /// Keeps `context` in `slot`, charged at its measured size.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when memory accounting is poisoned.
    fn restore(
        &self,
        slot: &std::sync::Mutex<Option<RetainedEncoder>>,
        mut context: arrow::ipc::writer::IpcWriteContext,
        lease: Option<ScribeMemoryLease>,
    ) -> Result<(), ScribeError> {
        let bytes = context.retained_bytes();
        let Some(resources) = &self.resources else {
            return Ok(());
        };
        if bytes == 0 || bytes > self.cap || self.is_closed() {
            return Ok(());
        }
        let lease = match lease {
            Some(mut lease) => lease.resize_ingress(bytes).map(|()| lease),
            None => resources.try_reserve_ingress(MemoryCategory::Metadata, bytes),
        };
        let lease = match lease {
            Ok(lease) => lease,
            Err(ScribeError::IngestBusy { .. }) => return Ok(()),
            Err(error) => return Err(error),
        };
        if let Ok(mut slot) = slot.lock() {
            *slot = Some(RetainedEncoder { context, lease });
            // A clear that raced this restore must not leave it charged.
            if self.is_closed() {
                slot.take();
            }
        }
        Ok(())
    }

    /// Whether [`Self::clear`] has closed this set.
    fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Releases every retained context and keeps none from now on.
    pub(crate) fn clear(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::Release);
        for slot in &self.slots {
            if let Ok(mut slot) = slot.lock() {
                slot.take();
            }
        }
    }

    /// Returns the bytes every slot's lease charges: worker-owned memory no
    /// request or shard holds, reported by Scribe inspection.
    pub(crate) fn charged_bytes(&self) -> usize {
        self.slots
            .iter()
            .filter_map(|slot| {
                slot.lock()
                    .ok()?
                    .as_ref()
                    .map(|retained| retained.lease.bytes())
            })
            .sum()
    }
}

fn notify_completion(
    completion: &mut Option<oneshot::Sender<Result<DurableCompletion, ScribeError>>>,
    error: &ScribeError,
) {
    if let Some(sender) = completion.take() {
        let _ = sender.send(Err(error.completion_copy()));
    }
}

/// Renders a digest as lowercase hex for a diagnostic field.
///
/// Only diagnostics use this: the durable identity always compares raw bytes.
#[must_use]
pub(crate) fn hex_digest(digest: &ContentDigest) -> String {
    use std::fmt::Write as _;
    digest
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// Accumulates the canonical logical identity of one ordered batch slice set.
///
/// The durable Scribe batch fence answers "is this the same batch the client
/// already sent?". That question must be settled by the rows themselves, so
/// this digest binds only stable logical facts: the slice ordinal, the approved
/// Arrow schema fingerprint, and the logical data identity from
/// [`logical_data_identity`], which deliberately omits the per-request managed
/// columns. It must never bind the WAL slice payload, whose managed columns
/// carry fresh per-request timestamps on every attempt and would therefore
/// make an honest client retry look like a contradiction.
///
/// This is not the WAL v8 `wal_digest`. That digest binds exact payload bytes so
/// a substituted slice cannot be authorized by a valid terminal record. The two
/// answer different questions, are computed apart, and the v8 COMMIT record
/// persists both — see [`crate::scribe::wal::WalCommitIdentity`].
///
/// The owner exists so there is one layout for this durable identity. It is
/// computed exactly once, on the ingest write path, and the terminal COMMIT
/// carries the result; replay reads that durable field rather than recomputing
/// it, so the two paths cannot drift.
pub(crate) struct LogicalBatchDigest {
    /// Running XXH3-128 over every slice pushed so far, in ordinal order.
    inner: ContentHasher,
}

impl LogicalBatchDigest {
    /// Starts an empty ordered accumulation.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            inner: ContentHasher::new(),
        }
    }

    /// Binds one slice's stable logical identity in its ordinal position.
    ///
    /// The caller must push slices in ascending `slice_index` order; ordering
    /// is part of the identity, so a reordered set yields a different digest.
    pub(crate) fn push_slice(
        &mut self,
        slice_index: u32,
        schema_fingerprint: &[u8; 32],
        data_digest: &ContentDigest,
        data_len: u32,
    ) {
        self.inner.update(slice_index.to_le_bytes());
        self.inner.update(schema_fingerprint);
        self.inner.update(data_digest);
        self.inner.update(data_len.to_le_bytes());
    }

    /// Returns the canonical digest over every pushed slice.
    #[must_use]
    pub(crate) fn finish(self) -> ContentDigest {
        self.inner.finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::array::{
        Array, ArrayRef, Decimal128Array, Int64Array, StringArray, Time64MicrosecondArray,
    };
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use bytes::Bytes;
    use wyrd_spec::auth::Attribution;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::ids::DataTenantId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::managed_columns::{WORKLOAD_CARD_UID, WYRD_INGESTED_AT, WYRD_REQUEST_ID};

    use super::{EncoderContexts, NativeAdmittedRows, NativeSliceProducer, logical_data_identity};
    use crate::catalog::TableRef;
    use crate::catalog::{TimeAxis, TimeGranularity};
    use crate::contracts::ScribeError;
    use crate::namespaces::BifrostNamespace;
    use crate::schema::SchemaFingerprint;

    /// Encodes one scalar native stream whose body can be shifted off alignment.
    fn native_stream() -> (Bytes, SchemaFingerprint) {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![1_i64, 2]))],
        )
        .expect("native batch");
        let fingerprint = SchemaFingerprint::from_arrow_schema(schema.as_ref());
        let mut bytes = Vec::new();
        let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("stream writer");
        writer.write(&batch).expect("stream batch");
        writer.finish().expect("stream finish");
        (Bytes::from(bytes), fingerprint)
    }

    /// Builds a producer over `bytes` partitioning daily on `wyrd_ingested_at`.
    ///
    /// # Panics
    ///
    /// Never panics; construction performs no IO or decoding.
    fn producer(bytes: Bytes, fingerprint: SchemaFingerprint) -> NativeSliceProducer {
        let tenant = DataTenantId::new_v7();
        let source = NativeAdmittedRows {
            definition: None,
            card_scope: None,
            dimensions: None,
            registered_schema: None,
            conform: Arc::default(),
            expanded_limit_bytes: crate::gate::limits::IngestLimits::default().expanded_bytes(),
            bytes,
            attribution: Attribution::direct(PrincipalId::new(uuid::Uuid::now_v7())).into(),
            workload: None,
            expected_schema_fingerprint: Some(fingerprint),
            request_id: RequestId::now_v7(),
            batch_id: uuid::Uuid::now_v7(),
            receipt_micros: 0,
        };
        NativeSliceProducer::new(
            source,
            uuid::Uuid::now_v7(),
            tenant,
            TableRef::new(BifrostNamespace::Bifrost, "events"),
            crate::catalog::PartitionScheme::time_only(TimeAxis::new(
                WYRD_INGESTED_AT,
                TimeGranularity::Day,
            )),
        )
    }

    /// Decodes every batch of `bytes` through the native producer's decoder.
    ///
    /// # Errors
    ///
    /// Returns the producer's refusal for the first batch Arrow rejects.
    fn decode_all(bytes: Bytes) -> Result<Vec<RecordBatch>, ScribeError> {
        let (_, fingerprint) = native_stream();
        let mut producer = producer(bytes, fingerprint);
        let mut batches = Vec::new();
        while let Some(batch) = producer.decode_next()? {
            batches.push(batch);
        }
        Ok(batches)
    }

    /// Encodes one single-column, one-batch stream for refusal fixtures.
    ///
    /// # Panics
    ///
    /// Panics when the field and array cannot form or encode a valid batch.
    fn scalar_stream(field: Field, array: ArrayRef) -> Vec<u8> {
        let schema = Arc::new(Schema::new(vec![field]));
        let batch = RecordBatch::try_new(Arc::clone(&schema), vec![array]).expect("scalar batch");
        let mut bytes = Vec::new();
        let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("stream writer");
        writer.write(&batch).expect("batch write");
        writer.finish().expect("stream finish");
        bytes
    }

    /// Returns the schema message's `FlatBuffer` range in one stream.
    ///
    /// # Panics
    ///
    /// Panics when the fixture lacks the continuation-framed schema prefix.
    fn schema_metadata_bounds(bytes: &[u8]) -> (usize, usize) {
        let len = u32::from_le_bytes(bytes[4..8].try_into().expect("schema length"));
        (8, 8 + usize::try_from(len).expect("metadata length"))
    }

    /// Returns the schema's first field from one stream's schema message.
    ///
    /// # Panics
    ///
    /// Panics when the metadata is not a schema message with a field.
    fn first_field(metadata: &[u8]) -> arrow::ipc::Field<'_> {
        arrow::ipc::root_as_message(metadata)
            .expect("message")
            .header_as_schema()
            .and_then(|schema| schema.fields())
            .map(|fields| fields.get(0))
            .expect("schema field")
    }

    /// Aligned bodies decode zero-copy and unaligned bodies decode via Arrow's copy.
    #[test]
    fn native_decoder_accepts_aligned_and_unaligned_transport() {
        let (aligned, _) = native_stream();
        let mut shifted = Vec::with_capacity(aligned.len() + 1);
        shifted.push(0);
        shifted.extend_from_slice(&aligned);
        let unaligned = Bytes::from(shifted).slice(1..);
        for bytes in [aligned, unaligned] {
            let batches = decode_all(bytes).expect("decodes");
            assert_eq!(batches.len(), 1);
            assert_eq!(batches[0].num_rows(), 2);
        }
    }

    /// Arrow refuses a stream cut inside a message and bytes after end-of-stream.
    #[test]
    fn native_decoder_refuses_truncated_and_trailing_bytes() {
        let (bytes, _) = native_stream();
        let cut = bytes.slice(..bytes.len() - 12);
        assert!(matches!(decode_all(cut), Err(ScribeError::InvalidFrame)));
        let mut trailing = bytes.to_vec();
        trailing.extend_from_slice(&[0_u8; 8]);
        assert!(matches!(
            decode_all(Bytes::from(trailing)),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// A null in a column the stream declares nonnullable fails Arrow validation.
    #[test]
    fn native_decoder_refuses_nonnullable_nulls() {
        let mut bytes = scalar_stream(
            Field::new("id", DataType::Int64, true),
            Arc::new(Int64Array::from(vec![Some(1_i64), None])),
        );
        let (start, end) = schema_metadata_bounds(&bytes);
        let offset = {
            let field = first_field(&bytes[start..end]);
            field._tab.loc() + usize::from(field._tab.vtable().get(arrow::ipc::Field::VT_NULLABLE))
        };
        bytes[start + offset] = 0;
        assert!(matches!(
            decode_all(Bytes::from(bytes)),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// An illegal Time unit and bit-width pair is refused by Arrow's schema decode.
    #[test]
    fn native_decoder_refuses_illegal_time_width() {
        let mut bytes = scalar_stream(
            Field::new(
                "time",
                DataType::Time64(arrow::datatypes::TimeUnit::Microsecond),
                false,
            ),
            Arc::new(Time64MicrosecondArray::from(vec![1_i64])),
        );
        let (start, end) = schema_metadata_bounds(&bytes);
        let offset = {
            let time = first_field(&bytes[start..end])
                .type_as_time()
                .expect("time metadata");
            time._tab.loc() + usize::from(time._tab.vtable().get(arrow::ipc::Time::VT_BITWIDTH))
        };
        bytes[start + offset..start + offset + 4].copy_from_slice(&32_i32.to_le_bytes());
        assert!(matches!(
            decode_all(Bytes::from(bytes)),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// Decimal precision beyond Decimal128's width never reaches WAL.
    ///
    /// Arrow decodes the widened declaration, so the refusal comes from the
    /// registered-schema fingerprint the producer checks on every batch.
    #[test]
    fn native_producer_refuses_decimal_precision_overflow() {
        let array = Decimal128Array::from(vec![Some(123_i128)])
            .with_precision_and_scale(10, 2)
            .expect("decimal fixture");
        let mut bytes = scalar_stream(
            Field::new("amount", DataType::Decimal128(10, 2), true),
            Arc::new(array),
        );
        let (start, end) = schema_metadata_bounds(&bytes);
        let offset = {
            let decimal = first_field(&bytes[start..end])
                .type_as_decimal()
                .expect("decimal metadata");
            decimal._tab.loc()
                + usize::from(decimal._tab.vtable().get(arrow::ipc::Decimal::VT_PRECISION))
        };
        bytes[start + offset..start + offset + 4].copy_from_slice(&39_i32.to_le_bytes());
        let registered = SchemaFingerprint::from_arrow_schema(&Schema::new(vec![Field::new(
            "amount",
            DataType::Decimal128(10, 2),
            true,
        )]));
        let refusal =
            producer(Bytes::from(bytes), registered).next_slice(&EncoderContexts::default());
        assert!(
            matches!(refusal, Err(ScribeError::FingerprintMismatch { .. })),
            "{refusal:?}"
        );
    }

    /// Stamped output above the expanded ceiling is refused before any WAL slice.
    ///
    /// # Panics
    ///
    /// Panics when the stream fits a one-byte ceiling or the refusal differs.
    #[test]
    fn native_producer_refuses_output_above_expanded_ceiling() {
        let (bytes, fingerprint) = native_stream();
        let mut producer = producer(bytes, fingerprint);
        producer.source.expanded_limit_bytes = 1;
        assert!(matches!(
            producer.next_slice(&EncoderContexts::default()),
            Err(ScribeError::DecodedPayloadTooLarge { limit: 1, .. })
        ));
    }

    /// Volatile server columns do not alter retained logical payload identity,
    /// and accepted correlation does.
    ///
    /// A fresh request id and ingestion time keep the identity of the same
    /// rows, while a changed user value or a changed workload is a different
    /// batch.
    ///
    /// # Panics
    ///
    /// Panics when a fixture batch does not build, an identity cannot be
    /// computed, or a volatile change alters it or a stable change does not.
    #[test]
    fn logical_identity_ignores_server_receipt_columns_only() {
        /// One stamped row for the identity comparisons: `value` is the user
        /// payload and `workload` the stable accepted correlation, both of
        /// which the identity binds, while `request` and `receipt` fill the
        /// volatile `wyrd_request_id` and `wyrd_ingested_at` columns it skips.
        ///
        /// # Panics
        ///
        /// Panics when the arrays do not build a batch of the declared schema.
        fn rows(value: i64, workload: &str, request: &str, receipt: i64) -> RecordBatch {
            let schema = Arc::new(Schema::new(vec![
                Field::new("value", DataType::Int64, false),
                Field::new(WORKLOAD_CARD_UID, DataType::Utf8, true),
                Field::new(WYRD_REQUEST_ID, DataType::Utf8, false),
                Field::new(WYRD_INGESTED_AT, DataType::Int64, false),
            ]));
            RecordBatch::try_new(
                schema,
                vec![
                    Arc::new(Int64Array::from(vec![value])),
                    Arc::new(StringArray::from(vec![workload])),
                    Arc::new(StringArray::from(vec![request])),
                    Arc::new(Int64Array::from(vec![receipt])),
                ],
            )
            .expect("identity batch")
        }

        let identity = |rows: RecordBatch| logical_data_identity(&rows).expect("identity");
        let first = identity(rows(7, "s1", "request-a", 10));
        assert_eq!(first, identity(rows(7, "s1", "request-b", 20)));
        assert_ne!(first, identity(rows(8, "s1", "request-c", 30)));
        assert_ne!(first, identity(rows(7, "s2", "request-a", 10)));
    }

    /// The WAL payload is an Arrow stream within its bound that Arrow reads back.
    ///
    /// Covers a nested Map column with a null map and a null value so the
    /// encoder is proven on the shapes the removed hand-written encoder needed
    /// per-type code for.
    ///
    /// # Panics
    /// Panics when encoding fails, the payload exceeds its bound, or the
    /// decoded batch differs.
    #[test]
    fn wal_payload_round_trips_within_its_bound() {
        let mut builder = arrow::array::MapBuilder::new(
            None,
            arrow::array::StringBuilder::new(),
            arrow::array::Int64Builder::new(),
        );
        builder.keys().append_value("a");
        builder.values().append_value(1);
        builder.keys().append_value("b");
        builder.values().append_null();
        builder.append(true).expect("first map");
        builder.append(false).expect("null map");
        let map = builder.finish();
        let schema = Arc::new(Schema::new(vec![Field::new(
            "m",
            map.data_type().clone(),
            true,
        )]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(map) as _]).expect("map batch");

        let payload = joined(EncoderContexts::default().encode(&batch).expect("encodes"));

        assert!(
            payload.len()
                <= crate::scribe::material_plan::wal_payload_bound(&batch).expect("bounds")
        );
        let mut reader =
            arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(payload), None)
                .expect("valid stream");
        assert_eq!(reader.next().expect("one batch").expect("decodes"), batch);
        assert!(reader.next().is_none());
    }

    /// Copies an encoded payload into the bytes a reader decodes.
    fn joined(payload: Bytes) -> Vec<u8> {
        payload.to_vec()
    }

    /// Encodes `rows` through Arrow's `StreamWriter`, the exact byte
    /// reference for a slice's WAL payload.
    fn reference_ipc(rows: &RecordBatch) -> Vec<u8> {
        let options = arrow::ipc::writer::IpcWriteOptions::default()
            .try_with_compression(Some(crate::scribe::material_plan::WAL_IPC_COMPRESSION))
            .and_then(|options| {
                options.try_with_compression_level(Some(
                    crate::scribe::material_plan::WAL_IPC_COMPRESSION_LEVEL,
                ))
            })
            .expect("WAL compression");
        let mut bytes = Vec::new();
        let mut writer = StreamWriter::try_new_with_options(&mut bytes, &rows.schema(), options)
            .expect("reference writer");
        writer.write(rows).expect("reference batch");
        writer.finish().expect("reference finish");
        drop(writer);
        bytes
    }

    /// A prepared slice's WAL payload is byte for byte the stream Arrow's
    /// `StreamWriter` produces, for nested, null, dictionary and offset-sliced
    /// columns and for each tenant's independent slice.
    #[test]
    fn prepared_slices_carry_reference_ipc_bytes() {
        use arrow::array::{DictionaryArray, ListArray, StringArray};
        use arrow::datatypes::Int32Type;

        let list = ListArray::from_iter_primitive::<arrow::datatypes::Int64Type, _, _>(
            (0..64_i64)
                .map(|row| (row % 5 != 0).then(|| (0..row % 4).map(Some).collect::<Vec<_>>())),
        );
        let dictionary: DictionaryArray<Int32Type> = (0..64)
            .map(|row| (row % 3 != 0).then_some(["x", "y", "z"][row % 3]))
            .collect();
        let text =
            StringArray::from_iter((0..64).map(|row| (row % 7 != 0).then(|| format!("v{row}"))));
        let rows = RecordBatch::try_from_iter(vec![
            ("list", Arc::new(list) as ArrayRef),
            ("dictionary", Arc::new(dictionary) as ArrayRef),
            ("text", Arc::new(text) as ArrayRef),
        ])
        .expect("mixed batch")
        .slice(3, 50);
        let table = TableRef::new(BifrostNamespace::Bifrost, "mixed");
        for tenant in [DataTenantId::new_v7(), DataTenantId::new_v7()] {
            let slice = super::prepare_slice(
                super::SliceContext {
                    encoders: &EncoderContexts::default(),
                    batch_id: uuid::Uuid::new_v4(),
                    request_id: uuid::Uuid::new_v4(),
                    tenant,
                    table: &table,
                    table_version: wyrd_spec::vala::ids::TableVersion::INITIAL,
                },
                crate::catalog::PhysicalPartition::time_only(crate::test_support::day_partition(
                    2026, 1, 1,
                )),
                rows.clone(),
            )
            .expect("prepares slice");
            assert_eq!(slice.wal_append.data.as_ref(), reference_ipc(&rows));
        }
    }

    /// A lane worker reuses one charged encoder context across independent
    /// streams of alternating tenants, schemas and dictionaries, while every
    /// earlier stream's output stays live: each stream is byte for byte the
    /// fresh-writer reference, the slot's lease charges exactly the retained
    /// bytes, a context over the cap or refused by the memory root is
    /// dropped without failing the encode, and clearing releases everything.
    #[test]
    fn lane_workers_reuse_charged_encoder_contexts() {
        use crate::resources::{BifrostRole, BifrostRuntimeResources, ScribeMemoryCategory};
        use arrow::array::{DictionaryArray, StringArray};
        use arrow::datatypes::Int32Type;

        let roles =
            BifrostRuntimeResources::composed_for_test(1 << 30, 1 << 30, [BifrostRole::Scribe]);
        let scribe = roles.scribe().expect("Scribe role").clone();
        let charged = || {
            scribe
                .snapshot()
                .expect("snapshot")
                .scribe_memory_used_bytes
        };
        let worker = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .start_handler(super::mark_ingress_worker)
            .build()
            .expect("one worker");
        let dictionary: DictionaryArray<Int32Type> =
            (0..512).map(|row| ["x", "y", "z"][row % 3]).collect();
        let streams = [
            RecordBatch::try_from_iter(vec![("dictionary", Arc::new(dictionary) as ArrayRef)])
                .expect("dictionary rows"),
            RecordBatch::try_from_iter(vec![(
                "value",
                Arc::new(Int64Array::from_iter_values(0..2048)) as ArrayRef,
            )])
            .expect("integer rows"),
            RecordBatch::try_from_iter(vec![(
                "text",
                Arc::new(StringArray::from_iter_values(
                    (0..512).map(|row| format!("t{row}")),
                )) as ArrayRef,
            )])
            .expect("text rows"),
        ];

        let encoders = EncoderContexts::new(1, scribe.clone(), usize::MAX);
        let mut outputs = Vec::new();
        for rows in streams.iter().cycle().take(6) {
            let chunks = worker.install(|| encoders.encode(rows)).expect("encodes");
            assert_eq!(joined(chunks.clone()), reference_ipc(rows));
            outputs.push(chunks);
            assert!(encoders.charged_bytes() > 0, "the worker kept no context");
            assert_eq!(charged(), encoders.charged_bytes());
            assert_eq!(
                scribe.memory_snapshot().categories[ScribeMemoryCategory::Metadata as usize],
                encoders.charged_bytes(),
                "a retained context is worker metadata, not a request's prepared slice"
            );
        }
        let nested = DataType::Dictionary(
            Box::new(DataType::Int32),
            Box::new(DataType::Dictionary(
                Box::new(DataType::Int32),
                Box::new(DataType::Utf8),
            )),
        );
        let unencodable =
            RecordBatch::try_from_iter(vec![("nested", arrow::array::new_null_array(&nested, 1))])
                .expect("dictionary-of-dictionary rows");
        assert!(worker.install(|| encoders.encode(&unencodable)).is_err());
        assert_eq!(
            (encoders.charged_bytes(), charged()),
            (0, 0),
            "a failed encode drops its context and lease"
        );
        worker
            .install(|| encoders.encode(&streams[0]))
            .expect("encodes after a failure");
        assert!(encoders.charged_bytes() > 0);

        encoders.clear();
        assert_eq!(charged(), 0);
        let after_clear = worker
            .install(|| encoders.encode(&streams[0]))
            .expect("encodes");
        assert_eq!(joined(after_clear), reference_ipc(&streams[0]));
        assert_eq!(charged(), 0, "a cleared set keeps nothing");

        let oversized = EncoderContexts::new(1, scribe.clone(), 1);
        worker
            .install(|| oversized.encode(&streams[1]))
            .expect("encodes");
        assert_eq!((oversized.charged_bytes(), charged()), (0, 0));

        let hog = scribe
            .try_reserve_ingress(ScribeMemoryCategory::Raw, scribe.ingress_limit_bytes())
            .expect("whole ingress budget");
        let refused = EncoderContexts::new(1, scribe.clone(), usize::MAX);
        let chunks = worker
            .install(|| refused.encode(&streams[2]))
            .expect("a refused charge still encodes");
        assert_eq!(joined(chunks), reference_ipc(&streams[2]));
        assert_eq!(refused.charged_bytes(), 0);
        drop(hog);
        assert_eq!(charged(), 0);

        let outside = EncoderContexts::new(1, scribe, usize::MAX);
        outside.encode(&streams[0]).expect("encodes off the lane");
        let unrelated = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .expect("another pool");
        unrelated
            .install(|| outside.encode(&streams[0]))
            .expect("encodes on another pool");
        assert_eq!(
            outside.charged_bytes(),
            0,
            "a caller off the lane keeps nothing"
        );
    }

    /// A closure that runs while an encode holds its worker's context keeps
    /// that context charged until the encode settles, then never restores it.
    ///
    /// The in-flight state is the one [`EncoderContexts::encode`] creates: the
    /// slot's context and lease taken out for the stream. Completion restores
    /// through [`EncoderContexts::restore`]; failure drops both.
    ///
    /// # Panics
    /// Panics when encoding fails, the in-flight lease is released before it
    /// settles, or a settled encode refills or stays charged in a closed set.
    #[test]
    fn closure_during_encode_keeps_its_lease_until_settlement() {
        use crate::resources::{BifrostRole, BifrostRuntimeResources};

        let roles =
            BifrostRuntimeResources::composed_for_test(1 << 30, 1 << 30, [BifrostRole::Scribe]);
        let scribe = roles.scribe().expect("Scribe role").clone();
        let charged = || {
            scribe
                .snapshot()
                .expect("snapshot")
                .scribe_memory_used_bytes
        };
        let worker = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .start_handler(super::mark_ingress_worker)
            .build()
            .expect("one worker");
        let rows = RecordBatch::try_from_iter(vec![(
            "value",
            Arc::new(Int64Array::from_iter_values(0..2048)) as ArrayRef,
        )])
        .expect("integer rows");

        for completes in [true, false] {
            let encoders = EncoderContexts::new(1, scribe.clone(), usize::MAX);
            let chunks = worker.install(|| encoders.encode(&rows)).expect("encodes");
            assert_eq!(joined(chunks), reference_ipc(&rows));
            let warmed = encoders.charged_bytes();
            assert!(warmed > 0, "the worker kept no context");

            let in_flight = encoders.slots[0]
                .lock()
                .expect("slot")
                .take()
                .expect("retained context");
            encoders.clear();
            assert_eq!(
                charged(),
                warmed,
                "closure released a lease an encode still holds"
            );

            if completes {
                encoders
                    .restore(&encoders.slots[0], in_flight.context, Some(in_flight.lease))
                    .expect("restore settles");
            } else {
                drop(in_flight);
            }
            assert_eq!(
                (encoders.charged_bytes(), charged()),
                (0, 0),
                "a settled encode refilled or stayed charged in a closed set"
            );
            assert!(encoders.slots[0].lock().expect("slot").is_none());
        }
    }

    /// Incompressible buffers stay within the reserved bound; compressible
    /// ones shrink.
    ///
    /// Random binary values and random integers make Arrow fall back to
    /// uncompressed buffers, the worst case the bound covers; a dictionary
    /// column adds a second compressed message. A constant column must
    /// compress below the uncompressed stream.
    ///
    /// # Panics
    /// Panics when encoding fails, the payload exceeds its bound, the decoded
    /// batch differs, or the constant column does not compress.
    #[test]
    fn wal_payload_bound_covers_incompressible_data() {
        use arrow::array::{BinaryArray, DictionaryArray};
        use arrow::datatypes::Int32Type;
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let values = (0..4_096)
            .map(|_| {
                (0..rng.gen_range(1..96))
                    .map(|_| rng.r#gen::<u8>())
                    .collect::<Vec<u8>>()
            })
            .collect::<Vec<_>>();
        let random = BinaryArray::from_iter_values(values.iter());
        let integers = Int64Array::from_iter_values((0..4_096).map(|_| rng.r#gen::<i64>()));
        let dictionary: DictionaryArray<Int32Type> =
            (0..4_096).map(|index| ["x", "y", "z"][index % 3]).collect();
        let batch = RecordBatch::try_from_iter(vec![
            ("random", Arc::new(random) as ArrayRef),
            ("integers", Arc::new(integers) as ArrayRef),
            ("dictionary", Arc::new(dictionary) as ArrayRef),
        ])
        .expect("incompressible batch");

        let payload = joined(EncoderContexts::default().encode(&batch).expect("encodes"));
        assert!(
            payload.len()
                <= crate::scribe::material_plan::wal_payload_bound(&batch).expect("bounds")
        );
        let mut reader =
            arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(payload), None)
                .expect("valid stream");
        assert_eq!(reader.next().expect("one batch").expect("decodes"), batch);

        let constant = RecordBatch::try_from_iter(vec![(
            "constant",
            Arc::new(Int64Array::from_iter_values(std::iter::repeat_n(42, 4_096))) as ArrayRef,
        )])
        .expect("constant batch");
        let mut uncompressed = Vec::new();
        let mut writer =
            StreamWriter::try_new(&mut uncompressed, &constant.schema()).expect("writer");
        writer.write(&constant).expect("writes");
        writer.finish().expect("finishes");
        drop(writer);
        assert!(
            joined(
                EncoderContexts::default()
                    .encode(&constant)
                    .expect("encodes")
            )
            .len()
                < uncompressed.len()
        );
    }
}
