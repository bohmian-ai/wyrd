//! Blocking preprocessing for admitted appends.

use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use std::io::Cursor;
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
use crate::scribe::audit_envelope::encode_audit_event;
use crate::scribe::memory::MemoryCategory;
use crate::scribe::seal_key::{SealKey, split_batch_by_event_day};
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
#[derive(Debug)]
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
}

/// A request after deterministic event-day splitting and serialization.
#[derive(Debug)]
pub(crate) struct PreparedAppend {
    pub batch_id: Uuid,
    pub tenant: DataTenantId,
    pub table: TableRef,
    pub prepared_bytes: usize,
    pub slices: Vec<PreparedSlice>,
    pub reservation: InflightFrameReservation,
    pub memory: Option<ScribeMemoryLease>,
    pub durable_ack: Option<oneshot::Sender<Result<u64, ScribeError>>>,
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

/// Idempotency identity for one batch routed to one seal-key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppendSliceId {
    pub batch_id: Uuid,
    pub seal_key: SealKey,
}

/// Split and serialize an admitted append on the bounded pre-ACK CPU lane.
pub(crate) fn prepare_append(admitted: AdmittedAppend) -> Result<PreparedAppend, ScribeError> {
    let AdmittedAppend {
        batch_id,
        audit_event,
        rows,
        measured_wire_bytes: _measured_wire_bytes,
        admitted_bytes: _admitted_bytes,
        reservation,
        mut memory,
        tenant,
        table,
        queued_at,
        mut durable_ack,
    } = admitted;

    metrics::histogram!("bifrost_scribe_queue_wait_seconds")
        .record(queued_at.elapsed().as_secs_f64());

    let slices_result = (|| -> Result<Vec<PreparedSlice>, ScribeError> {
        let mut slices = Vec::new();
        match rows {
            AdmittedRows::Projected(rows) => {
                append_prepared_slices(&mut slices, batch_id, &audit_event, tenant, &table, &rows)?
            }
            AdmittedRows::Native(native) => {
                let reader =
                    arrow::ipc::reader::StreamReader::try_new(Cursor::new(native.bytes), None)
                        .map_err(|_| ScribeError::InvalidFrame)?;
                for source in reader {
                    let source = source.map_err(|_| ScribeError::InvalidFrame)?;
                    let rows = crate::scribe::execution_lanes::decode_native_batch(
                        source,
                        &native.principal,
                        native.expected_schema_fingerprint,
                        &native.request_id,
                        native.batch_id,
                        native.event_time_window,
                    )?;
                    append_prepared_slices(
                        &mut slices,
                        batch_id,
                        &audit_event,
                        tenant,
                        &table,
                        &rows,
                    )?;
                }
            }
        }
        let slice_count = u32::try_from(slices.len()).map_err(|_| ScribeError::Internal {
            detail: "Scribe batch exceeds the v4 WAL slice-count bound".to_owned(),
        })?;
        for (slice_index, slice) in slices.iter_mut().enumerate() {
            slice.wal_append.assign_slice_ordinal(
                u32::try_from(slice_index).map_err(|_| ScribeError::Internal {
                    detail: "Scribe batch slice index exceeds the v4 WAL bound".to_owned(),
                })?,
                slice_count,
            );
        }
        Ok(slices)
    })();
    let slices = match slices_result {
        Ok(slices) => slices,
        Err(error) => {
            notify_completion(&mut durable_ack, &error);
            return Err(error);
        }
    };

    let prepared_bytes = slices.iter().try_fold(
        REQUEST_OVERHEAD_BYTES,
        |total, slice| -> Result<usize, ScribeError> {
            total
                .checked_add(slice.memtable_bytes)
                .and_then(|value| value.checked_add(slice.wal_append.audit.len()))
                .and_then(|value| value.checked_add(slice.wal_append.data.len()))
                .ok_or(ScribeError::DecodedPayloadTooLarge {
                    bytes: usize::MAX,
                    limit: memory.bytes(),
                })
        },
    );
    let prepared_bytes = match prepared_bytes {
        Ok(bytes) => bytes,
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
) -> Result<(), ScribeError> {
    for slice in split_batch_by_event_day(rows)? {
        let (event_day, day_rows) = slice?;
        let seal_key = SealKey::new(tenant, table.clone(), event_day);
        let mut day_audit = audit_event.clone();
        day_audit.payload_summary = format!("{} rows", day_rows.num_rows());
        let audit_payload = encode_audit_event(&day_audit)?;
        let data_payload = encode_ipc_fixed(&day_rows)?;
        let wal_append = PreparedWalAppend::new(
            crate::scribe::wal::WalLsn::ZERO,
            *batch_id.as_bytes(),
            Bytes::from(audit_payload),
            data_payload,
        )
        .for_slice(
            seal_key.clone(),
            SchemaFingerprint::from_arrow_schema(&day_rows.schema()).0,
        );
        let memtable_bytes = day_rows.get_array_memory_size();
        slices.push(PreparedSlice {
            id: AppendSliceId {
                batch_id,
                seal_key: seal_key.clone(),
            },
            seal_key,
            audit_event: day_audit,
            rows: day_rows,
            wal_append,
            memtable_bytes,
        });
    }
    Ok(())
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
