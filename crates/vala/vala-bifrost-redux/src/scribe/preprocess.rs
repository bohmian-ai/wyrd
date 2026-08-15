//! Blocking preprocessing for admitted appends.

use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use std::time::Instant;
use tokio::sync::oneshot;
use uuid::Uuid;
use wyrd_spec::vala::api::AuditEvent;

use crate::catalog::TableRef;
use crate::contracts::ScribeError;
use crate::resources::ScribeMemoryLease;
use crate::schema::SchemaFingerprint;
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
    pub rows: RecordBatch,
    pub measured_wire_bytes: usize,
    pub admitted_bytes: usize,
    pub reservation: InflightFrameReservation,
    pub memory: ScribeMemoryLease,
    pub tenant: DataTenantId,
    pub table: TableRef,
    pub queued_at: Instant,
    pub durable_ack: Option<oneshot::Sender<Result<u64, ScribeError>>>,
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
        for (event_day, day_rows) in split_batch_by_event_day(&rows)? {
            let seal_key = SealKey::new(tenant, table.clone(), event_day);
            let mut day_audit = audit_event.clone();
            day_audit.payload_summary = format!("{} rows", day_rows.num_rows());
            let audit_payload = encode_audit_event(&day_audit)?;

            let mut data_payload = Vec::new();
            {
                let mut writer = StreamWriter::try_new(&mut data_payload, &day_rows.schema())
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("Arrow IPC writer init failed: {error}"),
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

            let wal_append = PreparedWalAppend::new(
                crate::scribe::wal::WalLsn::ZERO,
                *batch_id.as_bytes(),
                Bytes::from(audit_payload),
                Bytes::from(data_payload),
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
        Ok(slices)
    })();
    let slices = match slices_result {
        Ok(slices) => slices,
        Err(error) => {
            notify_completion(&mut durable_ack, &error);
            return Err(error);
        }
    };

    let prepared_bytes = slices.iter().fold(REQUEST_OVERHEAD_BYTES, |total, slice| {
        total
            .saturating_add(slice.memtable_bytes)
            .saturating_add(slice.wal_append.audit.len())
            .saturating_add(slice.wal_append.data.len())
    });
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

fn notify_completion(
    completion: &mut Option<oneshot::Sender<Result<u64, ScribeError>>>,
    error: &ScribeError,
) {
    if let Some(sender) = completion.take() {
        let _ = sender.send(Err(error.completion_copy()));
    }
}
