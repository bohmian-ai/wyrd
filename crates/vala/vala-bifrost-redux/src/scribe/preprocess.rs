//! Blocking preprocessing for admitted appends.

use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use std::time::Instant;
use uuid::Uuid;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::AuditEvent;

use crate::catalog::TableRef;
use crate::contracts::ScribeError;
use crate::scribe::admission::AdmissionReservation;
use crate::scribe::audit_envelope::encode_audit_event;
use crate::scribe::seal_key::{SealKey, split_batch_by_event_day};
use crate::scribe::wal::encode_append_frame_with_sequence;
use wyrd_spec::ids::DataTenantId;

/// A complete request accepted by pod-global admission.
#[derive(Debug)]
pub(crate) struct AdmittedAppend {
    pub request_id: RequestId,
    pub batch_id: Uuid,
    pub frame_sequence: u64,
    pub audit_event: AuditEvent,
    pub rows: RecordBatch,
    pub measured_wire_bytes: usize,
    pub admitted_bytes: usize,
    pub reservation: AdmissionReservation,
    pub tenant: DataTenantId,
    pub table: TableRef,
    pub queued_at: Instant,
}

/// A request after deterministic event-day splitting and serialization.
#[derive(Debug)]
pub(crate) struct PreparedAppend {
    pub request_id: RequestId,
    pub batch_id: Uuid,
    pub slices: Vec<PreparedSlice>,
    pub reservation: AdmissionReservation,
    pub queue_wait_us: u64,
}

/// One event-day slice ready for ordered WAL and memtable processing.
#[derive(Debug)]
pub(crate) struct PreparedSlice {
    pub id: AppendSliceId,
    pub seal_key: SealKey,
    pub audit_event: AuditEvent,
    pub rows: RecordBatch,
    pub wal_frame: Bytes,
    pub wal_bytes: usize,
    pub memtable_bytes: usize,
}

/// Idempotency identity for one batch routed to one seal-key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppendSliceId {
    pub batch_id: Uuid,
    pub frame_sequence: u64,
    pub seal_key: SealKey,
}

/// Split and serialize an admitted append. This function is run only by the
/// bounded post-ACK CPU lane, never on the ACK path.
pub(crate) fn prepare_append(admitted: AdmittedAppend) -> Result<PreparedAppend, ScribeError> {
    let AdmittedAppend {
        request_id,
        batch_id,
        frame_sequence,
        audit_event,
        rows,
        measured_wire_bytes: _measured_wire_bytes,
        admitted_bytes: _admitted_bytes,
        reservation,
        tenant,
        table,
        queued_at,
    } = admitted;

    let mut slices = Vec::new();
    for (event_day, day_rows) in split_batch_by_event_day(&rows)? {
        let seal_key = SealKey::new(tenant, table.clone(), event_day);
        let mut day_audit = audit_event.clone();
        day_audit.payload_summary = format!("{} rows", day_rows.num_rows());
        let audit_payload = encode_audit_event(&day_audit)?;

        let mut data_payload = Vec::new();
        {
            let mut writer =
                StreamWriter::try_new(&mut data_payload, &day_rows.schema()).map_err(|error| {
                    ScribeError::Internal {
                        detail: format!("Arrow IPC writer init failed: {error}"),
                    }
                })?;
            writer
                .write(&day_rows)
                .map_err(|error| ScribeError::Internal {
                    detail: format!("Arrow IPC write failed: {error}"),
                })?;
            writer.finish().map_err(|error| ScribeError::Internal {
                detail: format!("Arrow IPC finish failed: {error}"),
            })?;
        }

        let wal_frame = encode_append_frame_with_sequence(
            *batch_id.as_bytes(),
            frame_sequence,
            &audit_payload,
            &data_payload,
        )?;
        let wal_bytes = wal_frame.len();
        let memtable_bytes = day_rows.get_array_memory_size();
        slices.push(PreparedSlice {
            id: AppendSliceId {
                batch_id,
                frame_sequence,
                seal_key: seal_key.clone(),
            },
            seal_key,
            audit_event: day_audit,
            rows: day_rows,
            wal_frame,
            wal_bytes,
            memtable_bytes,
        });
    }

    Ok(PreparedAppend {
        request_id,
        batch_id,
        slices,
        reservation,
        queue_wait_us: u64::try_from(queued_at.elapsed().as_micros()).unwrap_or(u64::MAX),
    })
}
