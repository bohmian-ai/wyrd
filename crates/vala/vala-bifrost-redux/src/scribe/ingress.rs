//! Bounded pre-ACK preparation for Scribe requests.

use crate::contracts::{FrameAdmission, ScribeError, ScribeIngressFrame};
use crate::scribe::admission::{AdmissionController, MAX_REQUEST_BYTES, REQUEST_OVERHEAD_BYTES};
use crate::scribe::execution_lanes::{
    ScribeIngressCpuPool, ScribePersistenceCpuOp, ScribePersistenceCpuPool,
    ScribePersistenceCpuResult,
};
use crate::scribe::memory::{MemoryCategory, ScribeMemoryBudget};
use crate::scribe::preprocess::AdmittedAppend;
use crate::scribe::shards::ScribeShardRuntime;
use std::time::Instant;

/// Prepare one request and dispatch its owned packet directly to its shard.
///
/// The global item reservation is acquired before decoding and remains attached
/// to the `AdmittedAppend`. There is intentionally no raw-ingress channel in
/// front of the fixed shard mailboxes: the Rayon ingress pool bounds CPU work,
/// while the 4,096-item reservation bounds every request retained by Scribe.
pub(super) async fn process_ingress_frame(
    frame: ScribeIngressFrame,
    admission: &AdmissionController,
    memory_budget: &ScribeMemoryBudget,
    ingress_cpu: &ScribeIngressCpuPool,
    persistence_cpu: &ScribePersistenceCpuPool,
    shards: &ScribeShardRuntime,
) -> Result<FrameAdmission, ScribeError> {
    let append_started = Instant::now();
    let table = frame.binding.table_ref.fqn();
    let shard =
        crate::scribe::routing::shard_for(frame.principal.tenant_id, &frame.binding.table_ref);
    frame
        .binding
        .validate_authenticated_tenant(frame.principal.tenant_id)
        .map_err(|_| ScribeError::InvalidFrame)?;
    if frame.measured_wire_bytes > MAX_REQUEST_BYTES {
        return Err(ScribeError::PayloadTooLarge {
            bytes: frame.measured_wire_bytes,
        });
    }

    let initial_bytes = frame
        .measured_wire_bytes
        .saturating_add(REQUEST_OVERHEAD_BYTES);
    let mut reservation = admission.try_reserve(table.clone(), initial_bytes)?;
    let mut memory = memory_budget
        .try_reserve_ingress(MemoryCategory::Raw, initial_bytes)
        .map_err(|_| ScribeError::IngestBusy {
            table: table.clone(),
        })?;
    memory.attach_shard(memory_budget.shard_accounting(), shard);

    let rows = ingress_cpu
        .decode(
            frame.payload,
            frame.principal.clone(),
            frame.expected_schema_fingerprint,
            frame.request_id.clone(),
            frame.batch_id,
        )
        .await?;
    memory.transfer_category(MemoryCategory::Decode);
    let estimated_bytes = rows
        .get_array_memory_size()
        .saturating_add(frame.measured_wire_bytes)
        .saturating_add(REQUEST_OVERHEAD_BYTES);
    reservation.resize(estimated_bytes)?;
    memory
        .resize_ingress(estimated_bytes)
        .map_err(|_| ScribeError::IngestBusy {
            table: table.clone(),
        })?;
    memory.transfer_category(MemoryCategory::Prepared);

    let (durable_tx, durable_rx) = tokio::sync::oneshot::channel();
    let tenant = frame.principal.tenant_id;
    let table_ref = frame.binding.table_ref.clone();
    let admitted = AdmittedAppend {
        batch_id: frame.batch_id,
        audit_event: frame.audit_event,
        rows,
        measured_wire_bytes: frame.measured_wire_bytes,
        admitted_bytes: reservation.bytes(),
        reservation,
        memory,
        tenant,
        table: table_ref.clone(),
        queued_at: Instant::now(),
        durable_ack: Some(durable_tx),
    };
    let rows_accepted = u64::try_from(admitted.rows.num_rows()).unwrap_or(u64::MAX);
    let prepared = match persistence_cpu
        .submit(ScribePersistenceCpuOp::Preprocess(Box::new(admitted)))
        .await?
    {
        ScribePersistenceCpuResult::Prepared(value) => value,
        ScribePersistenceCpuResult::ParquetEncoded(_)
        | ScribePersistenceCpuResult::ReplayRestored(_) => {
            return Err(ScribeError::Internal {
                detail: "persistence lane returned the wrong preparation result".to_owned(),
            });
        }
    };
    shards.try_send(prepared)?;
    durable_rx.await.map_err(|_| ScribeError::Internal {
        detail: "shard owner dropped durable batch completion".to_owned(),
    })??;
    metrics::counter!("bifrost_scribe_frames_total", "status" => "accepted").increment(1);
    metrics::counter!("bifrost_scribe_rows_total", "status" => "accepted").increment(rows_accepted);
    metrics::histogram!("bifrost_scribe_ack_seconds")
        .record(append_started.elapsed().as_secs_f64());
    Ok(FrameAdmission {
        batch_id: frame.batch_id,
        rows_accepted,
    })
}
