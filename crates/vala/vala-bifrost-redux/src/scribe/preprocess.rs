//! Blocking preprocessing for admitted appends.

use arrow::array::{Array, ArrayData};
use arrow::buffer::Buffer;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use iceberg::spec::Schema;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::oneshot;
use uuid::Uuid;
use wyrd_runtime::Principal;
use wyrd_spec::request_id::RequestId;

use crate::catalog::TableRef;
use crate::catalog::TimeGranularity;
use crate::catalog::layout::TimePartition;
use crate::contracts::ScribeError;
use crate::resources::ScribeMemoryLease;
use crate::schema::SchemaFingerprint;
use crate::scribe::admission::EventTimeWindow;
use crate::scribe::admission::InflightFrameReservation;
use crate::scribe::admission::REQUEST_OVERHEAD_BYTES;
use crate::scribe::memory::MemoryCategory;
use crate::scribe::seal_key::{
    SealKey, TimePartitionPlan, plan_time_partitions, split_batch_by_time_partition,
};
use crate::scribe::wal::PreparedWalAppend;
use wyrd_spec::ids::DataTenantId;

/// The successful terminal a shard owner sends to one admitted append's waiter.
///
/// Sent only after the append's rows are query-visible, or after the durable
/// fence suppressed them as an already-committed replay of the same batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DurableCompletion {
    /// Rows the append carried, whether inserted now or already visible.
    pub rows: u64,
    /// Whether this append's group inserted the batch rather than suppressing
    /// a replay the fence or memtable identity showed was already committed.
    pub first_commit: bool,
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
    /// Registered partition granularity every slice of this append is bucketed to.
    pub partition_granularity: TimeGranularity,
    pub queued_at: Instant,
    /// Completion channel answered once the append is durable in the WAL, or
    /// with the typed error that refused it; `None` for fire-and-forget appends.
    pub durable_ack: Option<oneshot::Sender<Result<DurableCompletion, ScribeError>>>,
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
    /// Transport bytes retained until the final planned source is decoded.
    pub(crate) bytes: Bytes,
    /// Authenticated principal used for managed correlation columns.
    pub(crate) principal: Principal,
    /// Card UIDs every row's `card_uid` is authorized against: the frame's
    /// attributed UIDs, or the principal's signed scope.
    pub(crate) card_scope: Option<Vec<wyrd_spec::ids::CardUid>>,
    /// Catalog fingerprint validated independently for every source batch.
    pub(crate) expected_schema_fingerprint: SchemaFingerprint,
    /// Request identity stamped into every source batch.
    pub(crate) request_id: RequestId,
    /// Stable batch identity keying every produced slice.
    pub(crate) batch_id: Uuid,
    /// Immutable event-time acceptance window.
    pub(crate) event_time_window: EventTimeWindow,
    /// One receipt instant reused by planning, projection, and WAL identity.
    pub(crate) receipt_micros: i64,
    /// Start of the preflighted schema frame in retained transport bytes.
    pub(crate) schema_start: usize,
    /// Exclusive end of the preflighted schema frame including padding.
    pub(crate) schema_end: usize,
    /// Fixed preflight descriptors for each record-batch message.
    pub(crate) sources: [crate::scribe::material_plan::SourceMaterialPlan;
        crate::scribe::material_plan::MAX_SOURCE_PLANS],
    /// Live prefix length within `sources`.
    pub(crate) source_count: usize,
    /// Canonical built-in whose physical identity every source must preserve.
    pub(crate) definition: Option<&'static crate::tables::BuiltinTableDefinition>,
    /// Registered Iceberg schema whose field ids every stamped batch carries.
    ///
    /// `None` only for the embedded engine seam, which has no catalog owner
    /// and writes objects that no registered table promotes.
    pub(crate) registered_schema: Option<Arc<Schema>>,
    /// Expanded-data ceiling the decoded, stamped output must fit before WAL.
    pub(crate) expanded_limit_bytes: usize,
}

/// A request after deterministic event-day splitting and serialization.
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
    /// Push decoder retaining only its schema and current scratch.
    decoder: arrow::ipc::reader::StreamDecoder,
    /// Index of the next preflighted source descriptor.
    source_index: usize,
    /// Current decoded source retained only across its partition slices.
    current: Option<NativeCurrentSource>,
    /// Request correlation stamped on every produced slice.
    request_id: Uuid,
    /// Zero-based ordinal assigned to the next slice.
    slice_index: u32,
    /// Exact count established by the non-retaining first pass.
    slice_count: u32,
    /// Running decoded and stamped output checked against the expanded ceiling.
    output_bytes: usize,
    /// Authenticated tenant used by every produced seal key.
    tenant: DataTenantId,
    /// Logical table used by every produced seal key.
    table: TableRef,
    /// Registered partition granularity used to bucket every produced slice.
    partition_granularity: TimeGranularity,
}

/// One decoded native source and its fixed time-partition cursor.
#[derive(Debug)]
struct NativeCurrentSource {
    /// Stamped current source rows.
    rows: RecordBatch,
    /// Immutable current-source partition descriptors.
    partitions: TimePartitionPlan,
    /// Next partition descriptor to materialize.
    next_partition: usize,
}

impl NativeSliceProducer {
    /// Builds a producer after a non-retaining pass fixes the total slice count.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when native decoding, stamping, day planning, or
    /// slice-count arithmetic fails during the first pass.
    fn new(
        source: NativeAdmittedRows,
        request_id: Uuid,
        tenant: DataTenantId,
        table: TableRef,
        partition_granularity: TimeGranularity,
    ) -> Result<Self, ScribeError> {
        let mut decoder = arrow::ipc::reader::StreamDecoder::new().with_require_alignment(true);
        feed_native_schema(&mut decoder, &source)?;
        Ok(Self {
            source,
            decoder,
            source_index: 0,
            current: None,
            request_id,
            slice_index: 0,
            slice_count: 0,
            output_bytes: 0,
            tenant,
            table,
            partition_granularity,
        })
    }

    /// Produces one exact-capacity current slice and advances its owner state.
    ///
    /// Each decoded source is stamped and its actual retained output added to
    /// the running request total before any slice of it is produced, so output
    /// above the expanded ceiling is refused before WAL.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when decoding diverges from preflight, stamping
    /// or day materialization fails, or fixed-capacity IPC encoding fails, and
    /// [`ScribeError::DecodedPayloadTooLarge`] when actual output exceeds the
    /// expanded ceiling.
    pub(crate) fn next_slice(&mut self) -> Result<Option<PreparedSlice>, ScribeError> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if current.next_partition < current.partitions.len() {
                    let (partition, rows) = current
                        .partitions
                        .materialize(&current.rows, current.next_partition)?;
                    current.next_partition += 1;
                    let mut slice = prepare_slice(
                        SliceContext {
                            batch_id: self.source.batch_id,
                            request_id: self.request_id,
                            tenant: self.tenant,
                            table: &self.table,
                        },
                        partition,
                        rows,
                        None,
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
            let Some(rows) =
                decode_planned_native_source(&mut self.decoder, &self.source, self.source_index)?
            else {
                return Ok(None);
            };
            self.source_index += 1;
            let rows = stamp_native_source(&rows, &self.source)?;
            self.output_bytes = self
                .output_bytes
                .checked_add(crate::scribe::material_plan::retained_slice_bytes(&rows)?)
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
            let partitions = plan_time_partitions(&rows, self.partition_granularity)?;
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

/// Counts native slices while retaining only one decoded source at a time.
///
/// # Errors
///
/// Returns [`ScribeError`] when decoding, stamping, day planning, or checked
/// slice-count conversion fails.
/// Decodes the next record batch from a preflighted native source.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when Arrow decoding or final stream
/// completion diverges from the accepted metadata plan.
fn feed_native_schema(
    decoder: &mut arrow::ipc::reader::StreamDecoder,
    source: &NativeAdmittedRows,
) -> Result<(), ScribeError> {
    source
        .bytes
        .get(source.schema_start..source.schema_end)
        .ok_or(ScribeError::InvalidFrame)?;
    let mut input = Buffer::from(source.bytes.slice(source.schema_start..source.schema_end));
    if decoder
        .decode(&mut input)
        .map_err(|_| ScribeError::InvalidFrame)?
        .is_some()
        || !input.is_empty()
    {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(())
}

/// Decodes one descriptor-selected native batch with an admitted current-body copy.
///
/// Aligned transport bodies alias the retained request bytes. An unaligned body
/// is copied once into an exactly sized Arrow allocation while the retained raw
/// owner remains live; the next source cannot begin until this batch is dropped.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when a descriptor range diverges from
/// retained bytes or Arrow does not produce exactly one batch.
fn decode_planned_native_source(
    decoder: &mut arrow::ipc::reader::StreamDecoder,
    source: &NativeAdmittedRows,
    source_index: usize,
) -> Result<Option<RecordBatch>, ScribeError> {
    let Some(plan) = source
        .sources
        .get(source_index)
        .filter(|_| source_index < source.source_count)
    else {
        return Ok(None);
    };
    source
        .bytes
        .get(plan.frame_start..plan.body_start)
        .ok_or(ScribeError::InvalidFrame)?;
    let mut metadata = Buffer::from(source.bytes.slice(plan.frame_start..plan.body_start));
    if decoder
        .decode(&mut metadata)
        .map_err(|_| ScribeError::InvalidFrame)?
        .is_some()
        || !metadata.is_empty()
    {
        return Err(ScribeError::InvalidFrame);
    }
    let raw_body = source
        .bytes
        .get(plan.body_start..plan.body_end)
        .ok_or(ScribeError::InvalidFrame)?;
    let aligned = (source.bytes.as_ptr() as usize)
        .checked_add(plan.body_start)
        .is_some_and(|address| address.is_multiple_of(8));
    let mut body = if aligned {
        Buffer::from(source.bytes.slice(plan.body_start..plan.body_end))
    } else {
        let mut copied = arrow::buffer::MutableBuffer::new(raw_body.len());
        copied.extend_from_slice(raw_body);
        Buffer::from(copied)
    };
    let rows = decoder
        .decode(&mut body)
        .map_err(|_| ScribeError::InvalidFrame)?
        .ok_or(ScribeError::InvalidFrame)?;
    if !body.is_empty() {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(Some(rows))
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
            principal: &source.principal,
            card_scope: source.card_scope.as_deref(),
            expected_schema_fingerprint: source.expected_schema_fingerprint,
            request_id: &source.request_id,
            window: source.event_time_window,
            receipt_micros: source.receipt_micros,
            definition: source.definition,
            registered_schema: source.registered_schema.as_deref(),
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
/// WAL-v6 metadata records alongside the digest.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the accumulated buffer length
/// overflows its `usize` accumulator.
fn digest_array_data(
    data: &ArrayData,
    digest: &mut Sha256,
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

/// Computes the identity of the accepted correlation attribution on `rows`.
///
/// [`logical_data_identity`] deliberately drops the universal correlation
/// columns so a server restamp cannot break retry convergence, which also
/// means two callers can present the same user payload under different
/// accepted attribution and reach the same logical digest. A caller that
/// derives a durable identity from that digest must bind this second digest
/// as well; the two together describe both what the rows are and whose
/// correlation they carry. Columns are folded in schema order, and a batch
/// carrying no correlation column digests the domain separator alone.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when a correlation column's buffer length
/// overflows its accumulator.
pub(crate) fn correlation_data_identity(rows: &RecordBatch) -> Result<[u8; 32], ScribeError> {
    let mut digest = Sha256::new();
    digest.update(b"wyrd.correlation-identity.v1");
    let mut bytes = 0_usize;
    for (field, column) in rows.schema().fields().iter().zip(rows.columns()) {
        if !crate::tables::managed_columns::is_correlation_column(field.name()) {
            continue;
        }
        digest.update(field.name().as_bytes());
        digest_array_data(&column.to_data(), &mut digest, &mut bytes)?;
    }
    Ok(digest.finalize().into())
}

/// Computes the stable logical Arrow identity used only for retained retries.
///
/// Server-generated request and time columns change across a legitimate client
/// retry, so they are excluded. Schema identity remains an independent retained
/// fact, while every other Arrow buffer participates byte-for-byte.
///
/// # Errors
///
/// Returns an internal error when the stable buffer length exceeds its WAL-v6
/// metadata representation.
pub(crate) fn logical_data_identity(rows: &RecordBatch) -> Result<([u8; 32], u32), ScribeError> {
    let mut digest = Sha256::new();
    let mut bytes = 0_usize;
    for (field, column) in rows.schema().fields().iter().zip(rows.columns()) {
        // Managed columns describe *which* request carried the rows and whose
        // attribution they bear, not what the rows are: the request and time
        // columns are stamped fresh on every attempt, and correlation is bound
        // separately by `correlation_data_identity`. Binding any of them here
        // would make an honest client retry of the same batch look like a
        // different batch. The set is the managed-column declaration itself,
        // so a newly managed column cannot silently enter this identity.
        if crate::tables::managed_columns::is_managed_column(field.name()) {
            continue;
        }
        digest.update(field.name().as_bytes());
        let before = bytes;
        digest_array_data(&column.to_data(), &mut digest, &mut bytes)?;
        if tracing::enabled!(tracing::Level::DEBUG) {
            let mut column_digest = Sha256::new();
            let mut column_bytes = 0_usize;
            digest_array_data(&column.to_data(), &mut column_digest, &mut column_bytes)?;
            tracing::debug!(
                column = field.name().as_str(),
                digest = %hex_digest(&column_digest.finalize().into()),
                bytes = bytes.saturating_sub(before),
                "logical batch identity column contribution"
            );
        }
    }
    let bytes = u32::try_from(bytes).map_err(|_| ScribeError::Internal {
        detail: "logical Arrow identity exceeds its u32 bound".to_owned(),
    })?;
    Ok((digest.finalize().into(), bytes))
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
    /// Registered partition granularity used to bucket every slice.
    partition_granularity: TimeGranularity,
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
) -> Result<(PreparedSliceSet, usize), ScribeError> {
    let PreparedRowsContext {
        batch_id,
        request_id,
        tenant,
        table,
        partition_granularity,
        memory_bytes,
    } = context;
    match rows {
        AdmittedRows::Projected(rows) => {
            let mut slices = Vec::new();
            append_prepared_slices(
                &mut slices,
                SliceContext {
                    batch_id,
                    request_id,
                    tenant,
                    table: &table,
                },
                partition_granularity,
                &rows,
            )?;
            let slice_count = u32::try_from(slices.len()).map_err(|_| ScribeError::Internal {
                detail: "Scribe batch exceeds the v6 WAL slice-count bound".to_owned(),
            })?;
            for (slice_index, slice) in slices.iter_mut().enumerate() {
                let slice_index =
                    u32::try_from(slice_index).map_err(|_| ScribeError::Internal {
                        detail: "Scribe batch slice index exceeds the v6 WAL bound".to_owned(),
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
            let mut producer = NativeSliceProducer::new(
                *native,
                request_id,
                tenant,
                table,
                partition_granularity,
            )?;
            let mut slices = Vec::new();
            while let Some(slice) = producer.next_slice()? {
                slices.push(slice);
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
/// Returns an internal error when the slice set exceeds WAL v6 integer bounds.
fn assign_slice_ordinals(slices: &mut [PreparedSlice]) -> Result<(), ScribeError> {
    let count = u32::try_from(slices.len()).map_err(|_| ScribeError::Internal {
        detail: "Scribe batch exceeds the v6 WAL slice-count bound".to_owned(),
    })?;
    for (index, slice) in slices.iter_mut().enumerate() {
        let index = u32::try_from(index).map_err(|_| ScribeError::Internal {
            detail: "Scribe batch slice index exceeds the v6 WAL bound".to_owned(),
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
pub(crate) fn prepare_append(admitted: AdmittedAppend) -> Result<PreparedAppend, ScribeError> {
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
        partition_granularity,
        queued_at,
        mut durable_ack,
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
            partition_granularity,
            memory_bytes: memory.bytes(),
        },
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
    partition_granularity: TimeGranularity,
    rows: &RecordBatch,
) -> Result<(), ScribeError> {
    for slice in split_batch_by_time_partition(rows, partition_granularity)? {
        let (partition, partition_rows) = slice?;
        slices.push(prepare_slice(context, partition, partition_rows, None)?);
    }
    Ok(())
}

/// Borrowed immutable context shared by each current slice preparation.
#[derive(Clone, Copy)]
struct SliceContext<'a> {
    /// Stable logical batch identity.
    batch_id: Uuid,
    /// Request correlation stamped on the produced slice.
    request_id: Uuid,
    /// Authenticated tenant used by the seal key.
    tenant: DataTenantId,
    /// Logical table used by the seal key.
    table: &'a TableRef,
}

/// Builds one fixed-capacity WAL slice from the current materialized day.
///
/// # Errors
///
/// Returns [`ScribeError`] when fixed IPC encoding fails.
fn prepare_slice(
    context: SliceContext<'_>,
    partition: TimePartition,
    rows: RecordBatch,
    ipc_plan: Option<crate::scribe::fixed_ipc::FixedIpcPlan>,
) -> Result<PreparedSlice, ScribeError> {
    let seal_key = SealKey::new(context.tenant, context.table.clone(), partition);
    let data_payload = encode_ipc_fixed(&rows, ipc_plan)?;
    let identity_span = tracing::debug_span!(
        "scribe_logical_batch_identity",
        batch_id = %context.batch_id,
        table = %context.table.fqn(),
    );
    let (logical_data_digest, logical_data_len) =
        identity_span.in_scope(|| logical_data_identity(&rows))?;
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
    let memtable_bytes = crate::scribe::memory::retained_arrow_bytes(&rows);
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

/// Encodes one current day into a capacity-frozen IPC payload.
///
/// A fixed inline plan establishes the complete stream length before the sole
/// output allocation and revalidates physical facts during encoding.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when either Arrow pass fails or the second
/// pass diverges from its counted length or initial capacity.
fn encode_ipc_fixed(
    rows: &RecordBatch,
    plan: Option<crate::scribe::fixed_ipc::FixedIpcPlan>,
) -> Result<Bytes, ScribeError> {
    let plan = match plan {
        Some(plan) => plan,
        None => crate::scribe::fixed_ipc::FixedIpcPlan::count(rows)?,
    };
    let encoded_bytes = plan.encoded_bytes();
    let payload = plan.encode(rows)?;
    if payload.len() != encoded_bytes || payload.capacity() != encoded_bytes {
        return Err(ScribeError::Internal {
            detail: "fixed IPC encoder diverged from its admitted capacity".to_owned(),
        });
    }
    Ok(Bytes::from(payload))
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
fn hex_digest(digest: &[u8; 32]) -> String {
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
/// This is not the WAL v6 `wal_digest`. That digest binds exact payload bytes so
/// a substituted slice cannot be authorized by a valid terminal record. The two
/// answer different questions, are computed apart, and the v6 COMMIT record
/// persists both — see [`crate::scribe::wal::WalCommitIdentity`].
///
/// The owner exists so there is one layout for this durable identity. It is
/// computed exactly once, on the ingest write path, and the terminal COMMIT
/// carries the result; replay reads that durable field rather than recomputing
/// it, so the two paths cannot drift.
pub(crate) struct LogicalBatchDigest {
    /// Running SHA-256 over every slice pushed so far, in ordinal order.
    inner: Sha256,
}

impl LogicalBatchDigest {
    /// Starts an empty ordered accumulation.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            inner: Sha256::new(),
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
        data_digest: &[u8; 32],
        data_len: u32,
    ) {
        self.inner.update(slice_index.to_le_bytes());
        self.inner.update(schema_fingerprint);
        self.inner.update(data_digest);
        self.inner.update(data_len.to_le_bytes());
    }

    /// Returns the canonical digest over every pushed slice.
    #[must_use]
    pub(crate) fn finish(self) -> [u8; 32] {
        self.inner.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use bytes::Bytes;
    use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::ids::DataTenantId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::managed_columns::{WYRD_EVENT_TIME, WYRD_INGESTED_AT, WYRD_REQUEST_ID};

    use super::{
        EventTimeWindow, NativeAdmittedRows, decode_planned_native_source, feed_native_schema,
        logical_data_identity,
    };
    use crate::schema::SchemaFingerprint;
    use crate::scribe::material_plan::ScribeIngressPlanner;

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

    /// Builds the retained owner required by the descriptor-driven decoder.
    ///
    /// # Panics
    ///
    /// Panics when the fixture bytes do not plan as a native payload.
    fn planned_source(bytes: Bytes, fingerprint: SchemaFingerprint) -> NativeAdmittedRows {
        let plan = ScribeIngressPlanner::default()
            .plan_native(&bytes)
            .expect("native plan");
        let tenant = DataTenantId::new_v7();
        NativeAdmittedRows {
            definition: None,
            card_scope: None,
            registered_schema: None,
            expanded_limit_bytes: crate::gate::limits::IngestLimits::default().expanded_bytes(),
            bytes,
            principal: Principal {
                id: PrincipalId::new(uuid::Uuid::now_v7()),
                kind: PrincipalKind::User,
                tenant_id: tenant,
                roles: Vec::new(),
                effective_permissions: PermissionSet::new(),
                credential_id: None,
            },
            expected_schema_fingerprint: fingerprint,
            request_id: RequestId::now_v7(),
            batch_id: uuid::Uuid::now_v7(),
            event_time_window: EventTimeWindow::default(),
            receipt_micros: 0,
            schema_start: plan.native_schema_start,
            schema_end: plan.native_schema_end,
            sources: plan.sources,
            source_count: plan.source_count,
        }
    }

    /// Proves aligned bodies alias raw bytes while unaligned bodies copy one current body.
    #[test]
    fn native_descriptor_decoder_accepts_aligned_and_unaligned_transport() {
        let (aligned, fingerprint) = native_stream();
        let aligned_plan = ScribeIngressPlanner::default()
            .plan_native(&aligned)
            .expect("aligned plan");
        assert_eq!(aligned_plan.aligned_copy_bytes, 0);

        let mut shifted = Vec::with_capacity(aligned.len() + 1);
        shifted.push(0);
        shifted.extend_from_slice(&aligned);
        let unaligned = Bytes::from(shifted).slice(1..);
        let unaligned_plan = ScribeIngressPlanner::default()
            .plan_native(&unaligned)
            .expect("unaligned plan");
        assert_eq!(
            unaligned_plan.aligned_copy_bytes,
            unaligned_plan.sources[0].body_end - unaligned_plan.sources[0].body_start
        );

        for source in [
            planned_source(aligned, fingerprint),
            planned_source(unaligned, fingerprint),
        ] {
            let mut decoder = arrow::ipc::reader::StreamDecoder::new().with_require_alignment(true);
            feed_native_schema(&mut decoder, &source).expect("schema");
            let batch = decode_planned_native_source(&mut decoder, &source, 0)
                .expect("decode")
                .expect("batch");
            assert_eq!(batch.num_rows(), 2);
        }
    }

    /// Volatile server columns do not alter retained logical payload identity.
    #[test]
    fn logical_identity_ignores_server_receipt_columns_only() {
        fn rows(value: i64, request: &str, receipt: i64) -> RecordBatch {
            let schema = Arc::new(Schema::new(vec![
                Field::new("value", DataType::Int64, false),
                Field::new(WYRD_REQUEST_ID, DataType::Utf8, false),
                Field::new(WYRD_EVENT_TIME, DataType::Int64, false),
                Field::new(WYRD_INGESTED_AT, DataType::Int64, false),
            ]));
            RecordBatch::try_new(
                schema,
                vec![
                    Arc::new(Int64Array::from(vec![value])),
                    Arc::new(StringArray::from(vec![request])),
                    Arc::new(Int64Array::from(vec![receipt])),
                    Arc::new(Int64Array::from(vec![receipt])),
                ],
            )
            .expect("identity batch")
        }

        let first = logical_data_identity(&rows(7, "request-a", 10)).expect("first identity");
        let fresh = logical_data_identity(&rows(7, "request-b", 20)).expect("fresh identity");
        let changed = logical_data_identity(&rows(8, "request-c", 30)).expect("changed identity");
        assert_eq!(first, fresh);
        assert_ne!(first, changed);
    }
}
