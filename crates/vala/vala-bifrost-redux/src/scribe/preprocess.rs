//! Blocking preprocessing for admitted appends.

use arrow::buffer::Buffer;
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use std::time::Instant;
use tokio::sync::oneshot;
use uuid::Uuid;
use wyrd_runtime::Principal;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::AuditEvent;

use crate::catalog::TableRef;
use crate::contracts::ScribeError;
use crate::resources::ScribeMemoryLease;
use crate::schema::SchemaFingerprint;
use crate::scribe::admission::EventTimeWindow;
use crate::scribe::admission::InflightFrameReservation;
use crate::scribe::admission::REQUEST_OVERHEAD_BYTES;
use crate::scribe::audit_envelope::encode_audit_event_bounded;
use crate::scribe::memory::MemoryCategory;
use crate::scribe::seal_key::{EventDayPlan, SealKey, plan_event_days, split_batch_by_event_day};
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
    pub reservation: InflightFrameReservation,
    pub memory: ScribeMemoryLease,
    pub tenant: DataTenantId,
    pub table: TableRef,
    pub queued_at: Instant,
    pub durable_ack: Option<oneshot::Sender<Result<u64, ScribeError>>>,
}

/// Root-owned row source entering current-only persistence preprocessing.
#[derive(Debug)]
pub(crate) enum AdmittedRows {
    /// One already-stamped projected batch.
    Projected(RecordBatch),
    /// One native stream decoded a record batch at a time on the persistence lane.
    Native(NativeAdmittedRows),
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
    /// One receipt instant reused by WAL and post-COMMIT regeneration.
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
    pub durable_ack: Option<oneshot::Sender<Result<u64, ScribeError>>>,
}

/// Closed prepared-slice source used by the shard owner.
#[derive(Debug)]
pub(crate) enum PreparedSliceSet {
    /// Existing projected/test path with already materialized slices.
    Materialized(Vec<PreparedSlice>),
    /// Native path producing exactly one current slice per CPU-lane turn.
    Native(Option<Box<NativeSliceProducer>>),
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
    /// Current decoded source retained only across its day slices.
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
    /// Exact-capacity audit JSON ceiling retained from the root plan.
    wal_workspace_bytes: usize,
}

/// One decoded native source and its fixed event-day cursor.
#[derive(Debug)]
struct NativeCurrentSource {
    /// Stamped current source rows.
    rows: RecordBatch,
    /// Immutable current-source day descriptors.
    days: EventDayPlan,
    /// Next day descriptor to materialize.
    next_day: usize,
}

impl NativeSliceProducer {
    /// Returns the immutable complete slice count established before production.
    #[must_use]
    pub(crate) const fn slice_count(&self) -> u32 {
        self.slice_count
    }

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
        wal_workspace_bytes: usize,
    ) -> Result<Self, ScribeError> {
        let slice_count = count_native_slices(&source)?;
        let mut decoder = arrow::ipc::reader::StreamDecoder::new().with_require_alignment(true);
        feed_native_schema(&mut decoder, &source)?;
        Ok(Self {
            source,
            decoder,
            source_index: 0,
            current: None,
            slice_index: 0,
            slice_count,
            audit_event,
            tenant,
            table,
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
                if current.next_day < current.days.len() {
                    let (event_day, rows) =
                        current.days.materialize(&current.rows, current.next_day)?;
                    current.next_day += 1;
                    let mut slice = prepare_slice(
                        self.source.batch_id,
                        &self.audit_event,
                        self.tenant,
                        &self.table,
                        event_day,
                        rows,
                        self.wal_workspace_bytes,
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
                if self.slice_index != self.slice_count {
                    return Err(ScribeError::Internal {
                        detail: "native slice production diverged from count pass".to_owned(),
                    });
                }
                return Ok(None);
            };
            self.source_index += 1;
            let rows = stamp_native_source(rows, &self.source)?;
            let days = plan_event_days(&rows)?;
            self.current = Some(NativeCurrentSource {
                rows,
                days,
                next_day: 0,
            });
        }
    }

    /// Rewinds the retained source for deterministic post-COMMIT regeneration.
    ///
    /// The same root continues to own the aliased bytes; this operation only
    /// replaces fixed decoder state and resets ordinals.
    pub(crate) fn restart(&mut self) -> Result<(), ScribeError> {
        self.decoder = arrow::ipc::reader::StreamDecoder::new().with_require_alignment(true);
        feed_native_schema(&mut self.decoder, &self.source)?;
        self.source_index = 0;
        self.current = None;
        self.slice_index = 0;
        Ok(())
    }
}

impl PreparedSliceSet {
    /// Returns the exact remaining closed-set size without advancing the source.
    ///
    /// # Errors
    ///
    /// Returns an internal error when the native producer owner is absent or
    /// its fixed count cannot be represented by the current platform.
    pub(crate) fn exact_slice_capacity(&self) -> Result<usize, ScribeError> {
        match self {
            Self::Materialized(slices) => Ok(slices.len()),
            Self::Native(Some(producer)) => {
                usize::try_from(producer.slice_count()).map_err(|_| ScribeError::Internal {
                    detail: "native slice count exceeds platform capacity".to_owned(),
                })
            }
            Self::Native(None) => Err(ScribeError::Internal {
                detail: "native producer owner missing during capacity planning".to_owned(),
            }),
        }
    }
}

/// Counts native slices while retaining only one decoded source at a time.
///
/// # Errors
///
/// Returns [`ScribeError`] when decoding, stamping, day planning, or checked
/// slice-count conversion fails.
fn count_native_slices(source: &NativeAdmittedRows) -> Result<u32, ScribeError> {
    let mut decoder = arrow::ipc::reader::StreamDecoder::new().with_require_alignment(true);
    feed_native_schema(&mut decoder, source)?;
    let mut slices = 0_usize;
    for source_index in 0..source.source_count {
        let rows = decode_planned_native_source(&mut decoder, source, source_index)?
            .ok_or(ScribeError::InvalidFrame)?;
        let rows = stamp_native_source(rows, source)?;
        slices = slices
            .checked_add(plan_event_days(&rows)?.len())
            .ok_or_else(|| ScribeError::Internal {
                detail: "native slice count overflow".to_owned(),
            })?;
    }
    decoder.finish().map_err(|_| ScribeError::InvalidFrame)?;
    u32::try_from(slices).map_err(|_| ScribeError::Internal {
        detail: "native slice count exceeds WAL v4 range".to_owned(),
    })
}

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
        .is_some_and(|address| address & 7 == 0);
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
    rows: RecordBatch,
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

/// Split and serialize an admitted append on the bounded pre-ACK CPU lane.
pub(crate) fn prepare_append(admitted: AdmittedAppend) -> Result<PreparedAppend, ScribeError> {
    let AdmittedAppend {
        batch_id,
        audit_event,
        rows,
        measured_wire_bytes: _measured_wire_bytes,
        admitted_bytes: _admitted_bytes,
        wal_workspace_bytes,
        reservation,
        mut memory,
        tenant,
        table,
        queued_at,
        mut durable_ack,
    } = admitted;

    metrics::histogram!("bifrost_scribe_queue_wait_seconds")
        .record(queued_at.elapsed().as_secs_f64());

    let slices_result = (|| -> Result<(PreparedSliceSet, usize), ScribeError> {
        match rows {
            AdmittedRows::Projected(rows) => {
                let mut slices = Vec::new();
                append_prepared_slices(
                    &mut slices,
                    batch_id,
                    &audit_event,
                    tenant,
                    &table,
                    &rows,
                    wal_workspace_bytes,
                )?;
                let slice_count =
                    u32::try_from(slices.len()).map_err(|_| ScribeError::Internal {
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
                let prepared_bytes = prepared_slice_bytes(&slices, memory.bytes())?;
                Ok((PreparedSliceSet::Materialized(slices), prepared_bytes))
            }
            AdmittedRows::Native(native) => Ok((
                PreparedSliceSet::Native(Some(Box::new(NativeSliceProducer::new(
                    native,
                    audit_event,
                    tenant,
                    table.clone(),
                    wal_workspace_bytes,
                )?))),
                memory.bytes(),
            )),
        }
    })();
    let (slices, prepared_bytes) = match slices_result {
        Ok(result) => result,
        Err(error) => {
            notify_completion(&mut durable_ack, &error);
            return Err(error);
        }
    };
    if prepared_bytes > memory.bytes() {
        let error = ScribeError::DecodedPayloadTooLarge {
            bytes: prepared_bytes,
            limit: memory.bytes(),
        };
        notify_completion(&mut durable_ack, &error);
        return Err(error);
    }
    if let Err(error) = memory.resize_ingress(prepared_bytes) {
        notify_completion(&mut durable_ack, &error);
        return Err(error);
    }
    if let Err(error) = memory.transfer_category(MemoryCategory::Prepared) {
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
/// Returns [`ScribeError`] when event-day validation, audit encoding, Arrow IPC
/// serialization, or checked slice construction fails. Completed earlier
/// slices remain owned by `slices` and are dropped by the caller on refusal.
fn append_prepared_slices(
    slices: &mut Vec<PreparedSlice>,
    batch_id: Uuid,
    audit_event: &AuditEvent,
    tenant: DataTenantId,
    table: &TableRef,
    rows: &RecordBatch,
    wal_workspace_bytes: usize,
) -> Result<(), ScribeError> {
    for slice in split_batch_by_event_day(rows)? {
        let (event_day, day_rows) = slice?;
        slices.push(prepare_slice(
            batch_id,
            audit_event,
            tenant,
            table,
            event_day,
            day_rows,
            wal_workspace_bytes,
        )?);
    }
    Ok(())
}

/// Builds one fixed-capacity WAL slice from the current materialized day.
///
/// # Errors
///
/// Returns [`ScribeError`] when audit encoding or fixed IPC encoding fails.
fn prepare_slice(
    batch_id: Uuid,
    audit_event: &AuditEvent,
    tenant: DataTenantId,
    table: &TableRef,
    event_day: crate::scribe::seal_key::EventDay,
    rows: RecordBatch,
    wal_workspace_bytes: usize,
) -> Result<PreparedSlice, ScribeError> {
    let seal_key = SealKey::new(tenant, table.clone(), event_day);
    let mut day_audit = audit_event.clone();
    day_audit.payload_summary = format!("{} rows", rows.num_rows());
    let audit_payload = encode_audit_event_bounded(&day_audit, wal_workspace_bytes)?;
    let data_payload = encode_ipc_fixed(&rows)?;
    let wal_append = PreparedWalAppend::new(
        crate::scribe::wal::WalLsn::ZERO,
        *batch_id.as_bytes(),
        Bytes::from(audit_payload),
        data_payload,
    )
    .for_slice(
        seal_key.clone(),
        SchemaFingerprint::from_arrow_schema(&rows.schema()).0,
    );
    let memtable_bytes = rows.get_array_memory_size();
    Ok(PreparedSlice {
        id: AppendSliceId {
            batch_id,
            seal_key: seal_key.clone(),
            slice_index: 0,
        },
        seal_key,
        audit_event: day_audit,
        rows,
        wal_append,
        memtable_bytes,
    })
}

/// Encodes one current day into a capacity-frozen IPC payload.
///
/// The first writer pass retains no output and establishes the exact public
/// byte count. The second pass allocates once at that capacity and verifies
/// that the vector did not grow before ownership transfers into [`Bytes`].
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when either Arrow pass fails or the second
/// pass diverges from its counted length or initial capacity.
fn encode_ipc_fixed(rows: &RecordBatch) -> Result<Bytes, ScribeError> {
    let output_bytes = crate::scribe::material_plan::count_ipc_bytes(rows)?;
    let mut payload = Vec::with_capacity(output_bytes);
    let fixed_capacity = payload.capacity();
    {
        let mut writer = StreamWriter::try_new(&mut payload, &rows.schema()).map_err(|error| {
            ScribeError::Internal {
                detail: format!("Arrow IPC writer init failed: {error}"),
            }
        })?;
        writer.write(rows).map_err(|error| ScribeError::Internal {
            detail: format!("Arrow IPC write failed: {error}"),
        })?;
        writer.finish().map_err(|error| ScribeError::Internal {
            detail: format!("Arrow IPC finish failed: {error}"),
        })?;
    }
    if payload.len() != output_bytes || payload.capacity() != fixed_capacity {
        return Err(ScribeError::Internal {
            detail: "Arrow IPC fixed-capacity output diverged from count pass".to_owned(),
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

    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use bytes::Bytes;
    use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::ids::DataTenantId;
    use wyrd_spec::request_id::RequestId;

    use super::{
        EventTimeWindow, NativeAdmittedRows, decode_planned_native_source, feed_native_schema,
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
}
