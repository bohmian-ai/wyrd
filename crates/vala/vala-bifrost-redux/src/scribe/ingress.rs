//! Bounded raw-ingress dispatch into Scribe's pre-ACK CPU lane.

use crate::contracts::{FrameAdmission, ScribeError, ScribeIngressFrame};
use crate::scribe::admission::{
    AdmissionController, IngressQueueBudget, IngressQueueConfig, IngressQueueReservation,
    MAX_REQUEST_BYTES, REQUEST_OVERHEAD_BYTES,
};
use crate::scribe::execution_lanes::ScribeIngressCpuPool;
use crate::scribe::preprocess::AdmittedAppend;
use crate::scribe::writer::TenantTableWriterRegistry;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::runtime::Handle;
use tokio::sync::{Notify, Semaphore, mpsc, oneshot};

struct IngressJob {
    frame: ScribeIngressFrame,
    reservation: IngressQueueReservation,
    response: oneshot::Sender<Result<FrameAdmission, ScribeError>>,
}

#[derive(Debug)]
pub(super) struct ScribeIngressQueue {
    sender: Mutex<Option<mpsc::Sender<IngressJob>>>,
    budget: IngressQueueBudget,
    closed: AtomicBool,
    active: Arc<AtomicUsize>,
    drained: Arc<Notify>,
    task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl ScribeIngressQueue {
    pub(super) fn new(
        items: usize,
        bytes: usize,
        admission: AdmissionController,
        ingress_cpu: ScribeIngressCpuPool,
        registry: Arc<TenantTableWriterRegistry>,
        coordination_runtime: &Handle,
    ) -> Self {
        let capacity = items.max(1);
        let budget = IngressQueueBudget::with_config(IngressQueueConfig {
            max_items: capacity,
            max_bytes: bytes,
        });
        let (sender, mut receiver) = mpsc::channel(capacity);
        let processing = Arc::new(Semaphore::new(capacity));
        let active = Arc::new(AtomicUsize::new(0));
        let drained = Arc::new(Notify::new());
        let task_active = Arc::clone(&active);
        let task_drained = Arc::clone(&drained);
        let task = coordination_runtime.spawn(async move {
            while let Some(job) = receiver.recv().await {
                let Ok(permit) = processing.clone().acquire_owned().await else {
                    break;
                };
                task_active.fetch_add(1, Ordering::AcqRel);
                let admission = admission.clone();
                let ingress_cpu = ingress_cpu.clone();
                let registry = Arc::clone(&registry);
                let active = Arc::clone(&task_active);
                let drained = Arc::clone(&task_drained);
                let IngressJob {
                    frame,
                    mut reservation,
                    response,
                } = job;
                tokio::spawn(async move {
                    let result = process_ingress_frame(
                        frame,
                        &mut reservation,
                        &admission,
                        &ingress_cpu,
                        &registry,
                    )
                    .await;
                    let _ = response.send(result);
                    active.fetch_sub(1, Ordering::AcqRel);
                    drop(permit);
                    drained.notify_waiters();
                });
            }
            loop {
                let notified = task_drained.notified();
                if task_active.load(Ordering::Acquire) == 0 {
                    break;
                }
                notified.await;
            }
        });
        Self {
            sender: Mutex::new(Some(sender)),
            budget,
            closed: AtomicBool::new(false),
            active,
            drained,
            task: tokio::sync::Mutex::new(Some(task)),
        }
    }

    pub(super) async fn submit(
        &self,
        frame: ScribeIngressFrame,
    ) -> Result<FrameAdmission, ScribeError> {
        if self.closed.load(Ordering::Acquire) {
            metrics::counter!("bifrost_scribe_admission_rejections_total", "reason" => "closed")
                .increment(1);
            return Err(ScribeError::IngressClosed);
        }
        if frame.measured_wire_bytes > MAX_REQUEST_BYTES {
            metrics::counter!("bifrost_scribe_admission_rejections_total", "reason" => "payload_too_large")
                .increment(1);
            return Err(ScribeError::PayloadTooLarge {
                bytes: frame.measured_wire_bytes,
            });
        }
        let table = frame.binding.table_ref.fqn();
        let bytes = frame
            .measured_wire_bytes
            .saturating_add(REQUEST_OVERHEAD_BYTES);
        let reservation = self.budget.try_reserve(table.clone(), bytes).inspect_err(|_| {
            metrics::counter!("bifrost_scribe_admission_rejections_total", "reason" => "raw_budget")
                .increment(1);
        })?;
        let (response, result) = oneshot::channel();
        let sender = self
            .sender
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("ingress sender lock poisoned: {error}"),
            })?
            .clone()
            .ok_or(ScribeError::IngressClosed)?;
        match sender.try_send(IngressJob {
            frame,
            reservation,
            response,
        }) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                metrics::counter!("bifrost_scribe_admission_rejections_total", "reason" => "ingress_queue_full")
                    .increment(1);
                return Err(ScribeError::IngestBusy { table });
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                metrics::counter!("bifrost_scribe_admission_rejections_total", "reason" => "closed")
                    .increment(1);
                return Err(ScribeError::IngressClosed);
            }
        }
        result.await.map_err(|_| ScribeError::Internal {
            detail: "ingress dispatcher dropped its frame result".to_owned(),
        })?
    }

    /// Stop accepting new frames, drop the producer, and await every accepted
    /// frame before writers are asked to drain.
    pub(super) async fn close_and_drain(&self) {
        self.closed.store(true, Ordering::Release);
        if let Ok(mut sender) = self.sender.lock() {
            sender.take();
        }
        loop {
            let notified = self.drained.notified();
            if self.active.load(Ordering::Acquire) == 0 {
                break;
            }
            notified.await;
        }
        if let Some(task) = self.task.lock().await.take() {
            let _ = task.await;
        }
    }
}

async fn process_ingress_frame(
    frame: ScribeIngressFrame,
    raw_reservation: &mut IngressQueueReservation,
    admission: &AdmissionController,
    ingress_cpu: &ScribeIngressCpuPool,
    registry: &TenantTableWriterRegistry,
) -> Result<FrameAdmission, ScribeError> {
    let append_started = Instant::now();
    frame
        .binding
        .validate_authenticated_tenant(frame.principal.tenant_id)
        .map_err(|_| ScribeError::InvalidFrame)?;
    let rows = ingress_cpu
        .decode(
            frame.payload,
            frame.principal.clone(),
            frame.expected_schema_fingerprint,
            frame.request_id.clone(),
            frame.batch_id,
        )
        .await?;
    let append_rows = rows.num_rows();
    let total_rows = frame
        .stream_rows_before
        .saturating_add(u64::try_from(append_rows).unwrap_or(u64::MAX));
    if total_rows > frame.stream_rows_limit {
        return Err(ScribeError::TooManyRows {
            rows: total_rows,
            limit: frame.stream_rows_limit,
        });
    }
    let estimated_bytes = rows
        .get_array_memory_size()
        .saturating_add(frame.measured_wire_bytes)
        .saturating_add(REQUEST_OVERHEAD_BYTES);
    let reservation = raw_reservation.transfer_to_retained(
        admission,
        frame.binding.table_ref.fqn(),
        estimated_bytes,
    )?;
    let admitted = AdmittedAppend {
        batch_id: frame.batch_id,
        frame_sequence: frame.frame_sequence,
        audit_event: frame.audit_event,
        rows,
        measured_wire_bytes: frame.measured_wire_bytes,
        admitted_bytes: reservation.bytes(),
        reservation,
        tenant: frame.principal.tenant_id,
        table: frame.binding.table_ref.clone(),
        queued_at: Instant::now(),
    };
    let rows_accepted = u64::try_from(admitted.rows.num_rows()).unwrap_or(u64::MAX);
    let (writer, created) = registry.get_or_create(frame.binding)?;
    if let Err(error) = TenantTableWriterRegistry::enqueue(&writer, admitted) {
        if created {
            registry.remove_if(&writer.key, writer.instance_id);
        }
        return Err(error);
    }
    metrics::counter!("bifrost_scribe_frames_total", "status" => "accepted").increment(1);
    metrics::counter!("bifrost_scribe_rows_total", "status" => "accepted").increment(rows_accepted);
    metrics::histogram!("bifrost_scribe_ack_seconds")
        .record(append_started.elapsed().as_secs_f64());
    Ok(FrameAdmission { rows_accepted })
}
