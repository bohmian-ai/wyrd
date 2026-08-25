//! Blocking preprocessing for admitted appends.

use arrow::array::{Array, ArrayData};
use arrow::buffer::Buffer;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use tokio::sync::oneshot;
use uuid::Uuid;
use wyrd_runtime::Principal;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::AuditEvent;
use wyrd_spec::vala::managed_columns::{WYRD_EVENT_TIME, WYRD_INGESTED_AT, WYRD_REQUEST_ID};

use crate::catalog::TableRef;
use crate::catalog::TimeGranularity;
use crate::catalog::layout::TimePartition;
use crate::contracts::ScribeError;
use crate::resources::ScribeMemoryLease;
use crate::schema::SchemaFingerprint;
use crate::scribe::admission::EventTimeWindow;
use crate::scribe::admission::InflightFrameReservation;
use crate::scribe::admission::REQUEST_OVERHEAD_BYTES;
use crate::scribe::audit_envelope::encode_audit_event_bounded;
use crate::scribe::memory::MemoryCategory;
use crate::scribe::seal_key::{
    SealKey, TimePartitionPlan, plan_time_partitions, split_batch_by_time_partition,
};
use crate::scribe::wal::PreparedWalAppend;
use wyrd_spec::ids::DataTenantId;

/// A complete request accepted by pod-global admission.
#[derive(Debug)]
pub(crate) struct AdmittedAppend {
    pub batch_id: Uuid,
    pub audit_event: AuditEvent,
    pub rows: AdmittedRows,
    pub measured_wire_bytes: usize,
    pub admitted_bytes: usize,
    /// Admitted exact-capacity ceiling for one current audit JSON payload.
    pub wal_workspace_bytes: usize,
    /// Maximum root envelope that must later replay this accepted unit.
    pub maximum_scribe_envelope_bytes: usize,
    pub reservation: InflightFrameReservation,
    pub memory: ScribeMemoryLease,
    pub tenant: DataTenantId,
    pub table: TableRef,
    /// Registered partition granularity every slice of this append is bucketed to.
    pub partition_granularity: TimeGranularity,
    pub queued_at: Instant,
    pub durable_ack: Option<oneshot::Sender<Result<u64, ScribeError>>>,
    /// Move-only lifecycle observation retained beside the admitted root.
    pub lifecycle: crate::scribe::telemetry::ScribeIngressLifecycleOwner,
}

/// Root-owned row source entering current-only persistence preprocessing.
#[derive(Debug)]
pub(crate) enum AdmittedRows {
    /// One already-stamped projected batch.
    Projected(RecordBatch),
    /// One native stream decoded a record batch at a time on the persistence lane.
    Native(Box<NativeAdmittedRows>),
    /// One typed OTLP request projected only when persistence requests its slice.
    Otlp(Box<OtlpAdmittedRows>),
}

/// Closed typed request retained by the private Scribe OTLP producer.
#[derive(Debug)]
pub(crate) enum OtlpTypedRows {
    /// Typed trace export.
    Traces(Box<wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest>),
    /// Typed metrics export.
    Metrics(Box<wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest>),
    /// Typed logs export.
    Logs(Box<wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest>),
}

/// Retained typed OTLP request and deterministic managed-column context.
#[derive(Debug)]
pub(crate) struct OtlpAdmittedRows {
    /// Move-only typed request consumed by the sole pre-WAL projection pass.
    pub(crate) request: OtlpTypedRows,
    /// Authenticated principal stamped into the current projected slice.
    pub(crate) principal: Principal,
    /// Catalog fingerprint checked against the projected schema.
    pub(crate) expected_schema_fingerprint: SchemaFingerprint,
    /// Stable request identity stamped into every accepted row.
    pub(crate) request_id: RequestId,
    /// Stable batch identity stamped into every accepted row.
    pub(crate) batch_id: Uuid,
    /// One authoritative receipt instant and partition day.
    pub(crate) receipt_micros: i64,
    /// Configured combined Arrow plus IPC ceiling for the sole current slice.
    pub(crate) material_limit: usize,
    /// Outcome published by the producer after its deterministic projection pass.
    pub(crate) outcome: Arc<OnceLock<crate::contracts::ScribeOtlpOutcome>>,
}

/// Retained native source and immutable stamping context.
#[derive(Debug, Clone)]
pub(crate) struct NativeAdmittedRows {
    /// Transport bytes retained until the final planned source is decoded.
    pub(crate) bytes: Bytes,
    /// Authenticated principal used for managed correlation columns.
    pub(crate) principal: Principal,
    /// Catalog fingerprint validated independently for every source batch.
    pub(crate) expected_schema_fingerprint: SchemaFingerprint,
    /// Request identity stamped into every source batch.
    pub(crate) request_id: RequestId,
    /// Stable batch identity stamped into every source batch.
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
}

/// A request after deterministic event-day splitting and serialization.
#[derive(Debug)]
pub(crate) struct PreparedAppend {
    pub batch_id: Uuid,
    pub tenant: DataTenantId,
    pub table: TableRef,
    pub prepared_bytes: usize,
    pub slices: PreparedSliceSet,
    pub reservation: InflightFrameReservation,
    pub memory: Option<ScribeMemoryLease>,
    /// Exact retained and persistence facts from the sole materialization.
    pub exact_material: ExactMaterialFacts,
    /// Maximum root envelope checked again with exact grouped-candidate facts.
    pub maximum_scribe_envelope_bytes: usize,
    pub durable_ack: Option<oneshot::Sender<Result<u64, ScribeError>>>,
    /// Move-only lifecycle observation retained beside the admitted root.
    pub lifecycle: Option<crate::scribe::telemetry::ScribeIngressLifecycleOwner>,
}

/// Closed prepared-slice source used by the shard owner.
#[derive(Debug)]
pub(crate) enum PreparedSliceSet {
    /// The complete once-materialized slice set retained through visibility.
    Materialized(Vec<PreparedSlice>),
}

/// Exact byte facts measured after the accepted unit is materialized once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactMaterialFacts {
    /// Complete retained live-set charged after atomic shrink.
    pub(crate) retained_live: usize,
    /// Largest whole stored Arrow batch in this unit.
    pub(crate) largest_stored_batch: usize,
    /// Largest candidate persistence must later admit without splitting input.
    pub(crate) persistence_candidate_peak: usize,
    /// Immutable candidate plus its serial incremental Parquet workspace.
    pub(crate) persistence_envelope_peak: usize,
}

/// One-shot typed OTLP current-slice producer.
#[derive(Debug)]
pub(crate) struct OtlpSliceProducer {
    /// Retained request and immutable stamping context.
    source: OtlpAdmittedRows,
    /// Whether the sole receipt-partition slice has been produced.
    produced: bool,
    /// Canonical batch audit moved only into the current durable slice.
    audit_event: AuditEvent,
    /// Authenticated tenant used by the slice seal key.
    tenant: DataTenantId,
    /// Logical table used by the slice seal key.
    table: TableRef,
    /// Registered partition granularity used to bucket the produced slice.
    partition_granularity: TimeGranularity,
    /// Exact-capacity audit JSON ceiling.
    wal_workspace_bytes: usize,
}

impl OtlpSliceProducer {
    /// Builds one retained producer without projecting the typed request.
    #[must_use]
    fn new(
        source: OtlpAdmittedRows,
        audit_event: AuditEvent,
        tenant: DataTenantId,
        table: TableRef,
        partition_granularity: TimeGranularity,
        wal_workspace_bytes: usize,
    ) -> Self {
        Self {
            source,
            produced: false,
            audit_event,
            tenant,
            table,
            partition_granularity,
            wal_workspace_bytes,
        }
    }

    /// Projects and prepares the sole receipt-day slice.
    ///
    /// # Errors
    ///
    /// Returns a stable projection, schema, stamping, day, or IPC preparation
    /// error. A second call after success returns exhaustion without allocating.
    pub(crate) fn next_slice(&mut self) -> Result<Option<PreparedSlice>, ScribeError> {
        if self.produced {
            return Ok(None);
        }
        let (batch, outcome) = match &self.source.request {
            OtlpTypedRows::Traces(request) => {
                let (batch, outcome) = crate::scribe::direct_traces::project(
                    request,
                    self.source.material_limit,
                    &self.source.principal,
                    self.source.expected_schema_fingerprint,
                    &self.source.request_id,
                    self.source.batch_id,
                    self.source.receipt_micros,
                )?;
                (batch, crate::contracts::ScribeOtlpOutcome::Traces(outcome))
            }
            OtlpTypedRows::Metrics(request) => {
                let (batch, outcome) = crate::scribe::direct_metrics::project(
                    request,
                    self.source.material_limit,
                    &self.source.principal,
                    self.source.expected_schema_fingerprint,
                    &self.source.request_id,
                    self.source.batch_id,
                    self.source.receipt_micros,
                )?;
                (batch, crate::contracts::ScribeOtlpOutcome::Metrics(outcome))
            }
            OtlpTypedRows::Logs(request) => {
                let (batch, outcome) = crate::scribe::direct_logs::project(
                    request,
                    self.source.material_limit,
                    &self.source.principal,
                    self.source.expected_schema_fingerprint,
                    &self.source.request_id,
                    self.source.batch_id,
                    self.source.receipt_micros,
                )?;
                (batch, crate::contracts::ScribeOtlpOutcome::Logs(outcome))
            }
        };
        if self.source.outcome.get().is_none() {
            let _ = self.source.outcome.set(outcome);
        }
        self.produced = true;
        let Some(batch) = batch else {
            return Ok(None);
        };
        let crate::scribe::otlp_managed::OtlpManagedBatch { rows, ipc_plan } = batch;
        let receipt = chrono::DateTime::from_timestamp_micros(self.source.receipt_micros)
            .ok_or(ScribeError::InvalidFrame)?;
        let partition = self
            .partition_granularity
            .bucket(receipt)
            .map_err(|error| ScribeError::Internal {
                detail: format!("OTLP receipt time has no partition: {error}"),
            })?;
        let current_bytes = rows
            .get_array_memory_size()
            .checked_add(ipc_plan.encoded_bytes())
            .ok_or(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: self.source.material_limit,
            })?;
        if current_bytes > self.source.material_limit {
            return Err(ScribeError::DecodedPayloadTooLarge {
                bytes: current_bytes,
                limit: self.source.material_limit,
            });
        }
        let mut slice = prepare_slice(
            SliceContext {
                batch_id: self.source.batch_id,
                audit_event: &self.audit_event,
                tenant: self.tenant,
                table: &self.table,
                wal_workspace_bytes: self.wal_workspace_bytes,
            },
            partition,
            rows,
            Some(ipc_plan),
        )?;
        slice.wal_append.assign_slice_ordinal(0, 1);
        slice.id.slice_index = 0;
        Ok(Some(slice))
    }
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
    /// Zero-based ordinal assigned to the next slice.
    slice_index: u32,
    /// Exact count established by the non-retaining first pass.
    slice_count: u32,
    /// Canonical batch audit cloned only into the current durable slice.
    audit_event: AuditEvent,
    /// Authenticated tenant used by every produced seal key.
    tenant: DataTenantId,
    /// Logical table used by every produced seal key.
    table: TableRef,
    /// Registered partition granularity used to bucket every produced slice.
    partition_granularity: TimeGranularity,
    /// Exact-capacity audit JSON ceiling retained from the root plan.
    wal_workspace_bytes: usize,
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
        audit_event: AuditEvent,
        tenant: DataTenantId,
        table: TableRef,
        partition_granularity: TimeGranularity,
        wal_workspace_bytes: usize,
    ) -> Result<Self, ScribeError> {
        let mut decoder = arrow::ipc::reader::StreamDecoder::new().with_require_alignment(true);
        feed_native_schema(&mut decoder, &source)?;
        Ok(Self {
            source,
            decoder,
            source_index: 0,
            current: None,
            slice_index: 0,
            slice_count: 0,
            audit_event,
            tenant,
            table,
            partition_granularity,
            wal_workspace_bytes,
        })
    }

    /// Produces one exact-capacity current slice and advances its owner state.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when decoding diverges from preflight, stamping
    /// or day materialization fails, or fixed-capacity IPC encoding fails.
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
                            audit_event: &self.audit_event,
                            tenant: self.tenant,
                            table: &self.table,
                            wal_workspace_bytes: self.wal_workspace_bytes,
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
        &source.principal,
        source.expected_schema_fingerprint,
        &source.request_id,
        source.batch_id,
        source.event_time_window,
        source.receipt_micros,
    )
}

/// One event-day slice ready for ordered WAL and memtable processing.
#[derive(Debug)]
pub(crate) struct PreparedSlice {
    pub id: AppendSliceId,
    pub seal_key: SealKey,
    pub audit_event: AuditEvent,
    pub rows: RecordBatch,
    pub wal_append: PreparedWalAppend,
    pub memtable_bytes: usize,
}

/// Computes the stable logical Arrow identity used only for retained retries.
///
/// Server-generated request and time columns change across a legitimate client
/// retry, so they are excluded. Schema identity remains an independent retained
/// fact, while every other Arrow buffer participates byte-for-byte.
///
/// # Errors
///
/// Returns an internal error when the stable buffer length exceeds its WAL-v4
/// metadata representation.
pub(crate) fn logical_data_identity(rows: &RecordBatch) -> Result<([u8; 32], u32), ScribeError> {
    fn update(data: &ArrayData, digest: &mut Sha256, bytes: &mut usize) -> Result<(), ScribeError> {
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
            update(child, digest, bytes)?;
        }
        Ok(())
    }

    let mut digest = Sha256::new();
    let mut bytes = 0_usize;
    for (field, column) in rows.schema().fields().iter().zip(rows.columns()) {
        if matches!(
            field.name().as_str(),
            WYRD_REQUEST_ID | WYRD_EVENT_TIME | WYRD_INGESTED_AT
        ) {
            continue;
        }
        digest.update(field.name().as_bytes());
        update(&column.to_data(), &mut digest, &mut bytes)?;
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
    /// Canonical audit envelope retained by each produced slice.
    audit_event: AuditEvent,
    /// Authenticated tenant used by every seal key.
    tenant: DataTenantId,
    /// Logical table used by every seal key.
    table: TableRef,
    /// Registered partition granularity used to bucket every slice.
    partition_granularity: TimeGranularity,
    /// Exact-capacity audit JSON ceiling.
    wal_workspace_bytes: usize,
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
/// Returns a stable day-planning, native decode, IPC, audit, or checked-size
/// refusal while the caller still owns the root lease.
fn prepare_rows(
    rows: AdmittedRows,
    context: PreparedRowsContext,
) -> Result<(PreparedSliceSet, usize), ScribeError> {
    let PreparedRowsContext {
        batch_id,
        audit_event,
        tenant,
        table,
        partition_granularity,
        wal_workspace_bytes,
        memory_bytes,
    } = context;
    match rows {
        AdmittedRows::Projected(rows) => {
            let mut slices = Vec::new();
            append_prepared_slices(
                &mut slices,
                SliceContext {
                    batch_id,
                    audit_event: &audit_event,
                    tenant,
                    table: &table,
                    wal_workspace_bytes,
                },
                partition_granularity,
                &rows,
            )?;
            let slice_count = u32::try_from(slices.len()).map_err(|_| ScribeError::Internal {
                detail: "Scribe batch exceeds the v4 WAL slice-count bound".to_owned(),
            })?;
            for (slice_index, slice) in slices.iter_mut().enumerate() {
                let slice_index =
                    u32::try_from(slice_index).map_err(|_| ScribeError::Internal {
                        detail: "Scribe batch slice index exceeds the v4 WAL bound".to_owned(),
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
                audit_event,
                tenant,
                table,
                partition_granularity,
                wal_workspace_bytes,
            )?;
            let mut slices = Vec::new();
            while let Some(slice) = producer.next_slice()? {
                slices.push(slice);
            }
            assign_slice_ordinals(&mut slices)?;
            let prepared_bytes = prepared_slice_bytes(&slices, memory_bytes)?;
            Ok((PreparedSliceSet::Materialized(slices), prepared_bytes))
        }
        AdmittedRows::Otlp(otlp) => {
            let mut producer = OtlpSliceProducer::new(
                *otlp,
                audit_event,
                tenant,
                table,
                partition_granularity,
                wal_workspace_bytes,
            );
            let slices = producer.next_slice()?.into_iter().collect::<Vec<_>>();
            let prepared_bytes = prepared_slice_bytes(&slices, memory_bytes)?;
            Ok((PreparedSliceSet::Materialized(slices), prepared_bytes))
        }
    }
}

/// Assigns final WAL ordinals after the single materialization traversal.
///
/// # Errors
///
/// Returns an internal error when the slice set exceeds WAL v4 integer bounds.
fn assign_slice_ordinals(slices: &mut [PreparedSlice]) -> Result<(), ScribeError> {
    let count = u32::try_from(slices.len()).map_err(|_| ScribeError::Internal {
        detail: "Scribe batch exceeds the v4 WAL slice-count bound".to_owned(),
    })?;
    for (index, slice) in slices.iter_mut().enumerate() {
        let index = u32::try_from(index).map_err(|_| ScribeError::Internal {
            detail: "Scribe batch slice index exceeds the v4 WAL bound".to_owned(),
        })?;
        slice.wal_append.assign_slice_ordinal(index, count);
        slice.id.slice_index = index;
    }
    Ok(())
}

/// Split and serialize an admitted append on the bounded pre-ACK CPU lane.
///
/// # Errors
///
/// Returns the preparation error after notifying the durable waiter, or a
/// stable material refusal when the retained prepared source exceeds its root.
pub(crate) fn prepare_append(admitted: AdmittedAppend) -> Result<PreparedAppend, ScribeError> {
    let AdmittedAppend {
        batch_id,
        audit_event,
        rows,
        measured_wire_bytes: _measured_wire_bytes,
        admitted_bytes: _admitted_bytes,
        wal_workspace_bytes,
        maximum_scribe_envelope_bytes,
        reservation,
        mut memory,
        tenant,
        table,
        partition_granularity,
        queued_at,
        mut durable_ack,
        mut lifecycle,
    } = admitted;

    metrics::histogram!("bifrost_scribe_queue_wait_seconds")
        .record(queued_at.elapsed().as_secs_f64());

    let slices_result = prepare_rows(
        rows,
        PreparedRowsContext {
            batch_id,
            audit_event,
            tenant,
            table: table.clone(),
            partition_granularity,
            wal_workspace_bytes,
            memory_bytes: memory.bytes(),
        },
    );
    let (slices, prepared_bytes) = match slices_result {
        Ok(result) => result,
        Err(error) => {
            lifecycle.refuse();
            notify_completion(&mut durable_ack, &error);
            return Err(error);
        }
    };
    let PreparedSliceSet::Materialized(materialized) = &slices;
    for slice in materialized {
        let bytes = slice
            .memtable_bytes
            .saturating_add(slice.wal_append.audit.len())
            .saturating_add(slice.wal_append.data.len());
        lifecycle.materialized(bytes);
    }
    if prepared_bytes > memory.bytes() {
        let error = ScribeError::DecodedPayloadTooLarge {
            bytes: prepared_bytes,
            limit: memory.bytes(),
        };
        lifecycle.refuse();
        notify_completion(&mut durable_ack, &error);
        return Err(error);
    }
    if let Err(error) = memory.shrink_to(prepared_bytes) {
        let error = ScribeError::Internal {
            detail: format!("exact material shrink violated the admitted upper bound: {error}"),
        };
        lifecycle.refuse();
        notify_completion(&mut durable_ack, &error);
        return Err(error);
    }
    if let Err(error) = memory.transfer_category(MemoryCategory::Prepared) {
        lifecycle.refuse();
        notify_completion(&mut durable_ack, &error);
        return Err(error);
    }

    let exact_material = exact_material_facts(&slices, prepared_bytes)?;
    if exact_material.persistence_envelope_peak > maximum_scribe_envelope_bytes {
        let error = ScribeError::DecodedPayloadTooLarge {
            bytes: exact_material.persistence_envelope_peak,
            limit: maximum_scribe_envelope_bytes,
        };
        lifecycle.refuse();
        notify_completion(&mut durable_ack, &error);
        return Err(error);
    }
    Ok(PreparedAppend {
        batch_id,
        tenant,
        table,
        prepared_bytes,
        slices,
        reservation,
        memory: Some(memory),
        exact_material,
        maximum_scribe_envelope_bytes,
        durable_ack,
        lifecycle: Some(lifecycle),
    })
}

/// Measures exact retained and whole-batch persistence facts.
fn exact_material_facts(
    slices: &PreparedSliceSet,
    retained_live_bytes: usize,
) -> Result<ExactMaterialFacts, ScribeError> {
    let largest_stored_batch_bytes = match slices {
        PreparedSliceSet::Materialized(slices) => slices
            .iter()
            .map(|slice| slice.memtable_bytes)
            .max()
            .unwrap_or(0),
    };
    let persistence_candidate_peak = match slices {
        PreparedSliceSet::Materialized(slices) => {
            let mut by_key = std::collections::HashMap::<SealKey, Vec<(usize, usize)>>::new();
            for slice in slices {
                by_key
                    .entry(slice.seal_key.clone())
                    .or_default()
                    .push((slice.rows.num_rows(), slice.memtable_bytes));
            }
            by_key.into_values().try_fold(0_usize, |largest, facts| {
                crate::scribe::parquet_writer::largest_candidate_bytes_from_facts(facts)
                    .map(|candidate| largest.max(candidate))
            })?
        }
    };
    let persistence_envelope_peak = persistence_candidate_peak
        .checked_add(crate::scribe::memory::parquet_candidate_incremental_bytes(
            persistence_candidate_peak,
        )?)
        .ok_or_else(|| ScribeError::Internal {
            detail: "persistence replayability envelope overflowed".to_owned(),
        })?;
    Ok(ExactMaterialFacts {
        retained_live: retained_live_bytes,
        largest_stored_batch: largest_stored_batch_bytes,
        persistence_candidate_peak,
        persistence_envelope_peak,
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
                .and_then(|value| value.checked_add(slice.wal_append.audit.len()))
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
/// Returns [`ScribeError`] when time-partition validation, audit encoding,
/// Arrow IPC serialization, or checked slice construction fails. Completed
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
    /// Canonical logical-batch audit envelope.
    audit_event: &'a AuditEvent,
    /// Authenticated tenant used by the seal key.
    tenant: DataTenantId,
    /// Logical table used by the seal key.
    table: &'a TableRef,
    /// Exact-capacity audit JSON ceiling.
    wal_workspace_bytes: usize,
}

/// Builds one fixed-capacity WAL slice from the current materialized day.
///
/// # Errors
///
/// Returns [`ScribeError`] when audit encoding or fixed IPC encoding fails.
fn prepare_slice(
    context: SliceContext<'_>,
    partition: TimePartition,
    rows: RecordBatch,
    ipc_plan: Option<crate::scribe::fixed_ipc::FixedIpcPlan>,
) -> Result<PreparedSlice, ScribeError> {
    let seal_key = SealKey::new(context.tenant, context.table.clone(), partition);
    let mut slice_audit = context.audit_event.clone();
    slice_audit.payload_summary = format!("{} rows", rows.num_rows());
    let audit_payload = encode_audit_event_bounded(&slice_audit, context.wal_workspace_bytes)?;
    let data_payload = encode_ipc_fixed(&rows, ipc_plan)?;
    let (logical_data_digest, logical_data_len) = logical_data_identity(&rows)?;
    let wal_append = PreparedWalAppend::new(
        crate::scribe::wal::WalLsn::ZERO,
        *context.batch_id.as_bytes(),
        Bytes::from(audit_payload),
        data_payload,
    )
    .for_slice(
        seal_key.clone(),
        SchemaFingerprint::from_arrow_schema(&rows.schema()).0,
    )
    .with_logical_data_identity(logical_data_digest, logical_data_len);
    let memtable_bytes = rows.get_array_memory_size();
    Ok(PreparedSlice {
        id: AppendSliceId {
            batch_id: context.batch_id,
            seal_key: seal_key.clone(),
            slice_index: 0,
        },
        seal_key,
        audit_event: slice_audit,
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
    completion: &mut Option<oneshot::Sender<Result<u64, ScribeError>>>,
    error: &ScribeError,
) {
    if let Some(sender) = completion.take() {
        let _ = sender.send(Err(error.completion_copy()));
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
    fn planned_source(bytes: Bytes, fingerprint: SchemaFingerprint) -> NativeAdmittedRows {
        let plan = ScribeIngressPlanner::default()
            .plan_native(&bytes, 0)
            .expect("native plan");
        let tenant = DataTenantId::new_v7();
        NativeAdmittedRows {
            bytes,
            principal: Principal {
                id: PrincipalId::new(uuid::Uuid::now_v7()),
                kind: PrincipalKind::User,
                tenant_id: tenant,
                roles: Vec::new(),
                effective_permissions: PermissionSet::new(),
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
            .plan_native(&aligned, 0)
            .expect("aligned plan");
        assert_eq!(aligned_plan.aligned_copy_bytes, 0);

        let mut shifted = Vec::with_capacity(aligned.len() + 1);
        shifted.push(0);
        shifted.extend_from_slice(&aligned);
        let unaligned = Bytes::from(shifted).slice(1..);
        let unaligned_plan = ScribeIngressPlanner::default()
            .plan_native(&unaligned, 0)
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
