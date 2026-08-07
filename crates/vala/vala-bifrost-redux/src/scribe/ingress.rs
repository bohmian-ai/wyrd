//! Bounded pre-ACK preparation for Scribe requests.

use super::ScribeImpl;
use crate::contracts::{FrameAdmission, ScribeError, ScribeIngressFrame};
use crate::scribe::admission::{MAX_REQUEST_BYTES, REQUEST_OVERHEAD_BYTES};
use crate::scribe::execution_lanes::{ScribePersistenceCpuOp, ScribePersistenceCpuResult};
use crate::scribe::memory::MemoryCategory;
use crate::scribe::preprocess::AdmittedAppend;
use crate::scribe::routing::shard_for;
use std::time::Instant;

impl ScribeImpl {
    /// Prepares one request and dispatches its owned packet to its fixed shard.
    ///
    /// The global item reservation is acquired before decoding and remains
    /// attached to the `AdmittedAppend`. The ingress CPU lane bounds decoding,
    /// while the bounded shard mailbox controls retained work.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when tenant validation, request bounds, memory or
    /// admission reservation, decoding, preparation, dispatch, or durable ACK
    /// completion fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation before dispatch releases local reservations. Cancellation
    /// after dispatch may leave the accepted append in the shard queue; the
    /// shard owns its eventual completion and retry semantics.
    pub(super) async fn prepare_and_dispatch(
        &self,
        frame: ScribeIngressFrame,
    ) -> Result<FrameAdmission, ScribeError> {
        let append_started = Instant::now();
        let table = frame.binding.table_ref.fqn();
        let shard = shard_for(
            frame.principal.tenant_id,
            &frame.binding.table_ref,
            frame.batch_id,
        );
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
        let mut reservation = self.admission.try_reserve(table.clone(), initial_bytes)?;
        let mut memory = match self
            .memory
            .try_reserve_ingress(MemoryCategory::Raw, initial_bytes)
        {
            Ok(reservation) => reservation,
            Err(_) => self.reserve_ingress_after_pressure_seal(
                MemoryCategory::Raw,
                initial_bytes,
                &table,
            )?,
        };
        memory.attach_shard(self.memory.shard_accounting(), shard);

        #[cfg(any(test, feature = "test-support"))]
        self.pause_admitted_ingest_for_test().await;

        let rows = self
            .ingress_cpu
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
        self.resize_ingress_after_pressure_seal(&mut memory, estimated_bytes, &table)?;
        memory.transfer_category(MemoryCategory::Prepared);

        let (durable_tx, durable_rx) = tokio::sync::oneshot::channel();
        let admitted = AdmittedAppend {
            batch_id: frame.batch_id,
            audit_event: frame.audit_event,
            rows,
            measured_wire_bytes: frame.measured_wire_bytes,
            admitted_bytes: reservation.bytes(),
            reservation,
            memory,
            tenant: frame.principal.tenant_id,
            table: frame.binding.table_ref.clone(),
            queued_at: Instant::now(),
            durable_ack: Some(durable_tx),
        };
        let rows_accepted = u64::try_from(admitted.rows.num_rows()).unwrap_or(u64::MAX);
        let prepared = match self
            .persistence_cpu
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
        self.shards
            .try_send(prepared)
            .inspect_err(|_| super::record_scribe_rejection("queue"))?;
        durable_rx.await.map_err(|_| ScribeError::Internal {
            detail: "shard owner dropped durable batch completion".to_owned(),
        })??;
        metrics::counter!("bifrost_scribe_frames_total", "status" => "accepted").increment(1);
        metrics::counter!("bifrost_scribe_rows_total", "status" => "accepted")
            .increment(rows_accepted);
        metrics::histogram!("bifrost_scribe_ack_seconds")
            .record(append_started.elapsed().as_secs_f64());
        Ok(FrameAdmission {
            batch_id: frame.batch_id,
            rows_accepted,
        })
    }

    /// Reserve ingress bytes after one coordinated pressure seal and single retry.
    ///
    /// This is the admission-side realization of the D83 flush-first contract:
    /// on an ingress ceiling rejection, request a shard-count-invariant pressure
    /// seal toward the low-water mark ([`ScribeImpl::request_pressure_seal_toward_low_water`]),
    /// then retry the reservation exactly once. The cgroup 90% tripwire is left
    /// as an immediate `IngestBusy` — a container-level limit that a Scribe
    /// pressure seal cannot relieve — and is not retried. A retry that still
    /// fails records the memory rejection and returns `IngestBusy`, deferring to
    /// D71 client backoff for eventual admission. There is no busy-wait: freezing
    /// and persistence proceed asynchronously between the seal request and retry.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the cgroup tripwire is engaged or
    /// when the single post-seal retry still cannot fit under the ingress ceiling.
    fn reserve_ingress_after_pressure_seal(
        &self,
        category: MemoryCategory,
        bytes: usize,
        table: &str,
    ) -> Result<crate::scribe::memory::MemoryReservation, ScribeError> {
        if !self.memory.cgroup_tripwire_engaged() {
            self.request_pressure_seal_toward_low_water();
            if let Ok(reservation) = self.memory.try_reserve_ingress(category, bytes) {
                return Ok(reservation);
            }
        }
        super::record_scribe_rejection("memory");
        Err(ScribeError::IngestBusy {
            table: table.to_owned(),
        })
    }

    /// Grow an existing ingress reservation after one pressure seal and retry.
    ///
    /// The decode step raises the required ingress bytes from the pre-decode
    /// estimate to the measured Arrow size, so the reservation must grow. This
    /// mirrors [`Self::reserve_ingress_after_pressure_seal`] for the in-place
    /// resize path: on an ingress ceiling rejection it requests a
    /// shard-count-invariant pressure seal toward the low-water mark and retries
    /// the resize exactly once, while the cgroup 90% tripwire stays an immediate
    /// `IngestBusy` that a Scribe seal cannot relieve. A retry that still fails
    /// records the memory rejection and returns `IngestBusy`, deferring to D71
    /// client backoff.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the cgroup tripwire is engaged or
    /// when the single post-seal resize retry still exceeds the ingress ceiling.
    fn resize_ingress_after_pressure_seal(
        &self,
        memory: &mut crate::scribe::memory::MemoryReservation,
        estimated_bytes: usize,
        table: &str,
    ) -> Result<(), ScribeError> {
        if memory.resize_ingress(estimated_bytes).is_ok() {
            return Ok(());
        }
        if !self.memory.cgroup_tripwire_engaged() {
            self.request_pressure_seal_toward_low_water();
            if memory.resize_ingress(estimated_bytes).is_ok() {
                return Ok(());
            }
        }
        super::record_scribe_rejection("memory");
        Err(ScribeError::IngestBusy {
            table: table.to_owned(),
        })
    }

    /// Pause one admitted write at the deterministic test-support barrier.
    #[cfg(any(test, feature = "test-support"))]
    async fn pause_admitted_ingest_for_test(&self) {
        if let Some(stall) = self
            .ingest_stall
            .lock()
            .ok()
            .and_then(|mut current| current.take())
        {
            stall.entered.notify_waiters();
            let _completion = super::IngestStallCompletion(&stall);
            stall.release.notified().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ScribeImpl;
    use crate::contracts::ScribeError;
    use crate::scribe::memory::MemoryCategory;
    use std::time::{Duration, Instant};

    /// The admission memory path rejects with `IngestBusy` **only** when the
    /// coordinated pressure seal cannot free ingress capacity (the D83
    /// reject-only-when-backlogged contract), and admits as soon as capacity is
    /// available.
    ///
    /// A freshly constructed Scribe holds no writable buckets, so a pressure
    /// seal selects no victims and cannot drain a pinned ceiling: the single
    /// post-seal retry must still reject. Once the pin is released (capacity
    /// available, as it would be after an asynchronous seal completed), the
    /// same admission path admits instead of rejecting — proving the rejection
    /// is conditional on genuine backlog rather than unconditional.
    #[tokio::test]
    async fn admission_rejects_only_when_seal_cannot_free() {
        let scribe = ScribeImpl::new();
        let ceiling = scribe.memory.ingress_limit_bytes();

        // Pin ingress at its ceiling: no bucket exists to seal, so the retry
        // after the pressure-seal request cannot fit and must reject.
        let pinned = scribe
            .memory
            .try_reserve_ingress(MemoryCategory::Raw, ceiling)
            .expect("pin ingress at the ceiling");
        assert!(
            matches!(
                scribe.reserve_ingress_after_pressure_seal(MemoryCategory::Raw, 1, "table"),
                Err(ScribeError::IngestBusy { .. })
            ),
            "a backlogged ceiling must reject after the seal request"
        );

        // Release the pin so capacity is available; the same path now admits.
        drop(pinned);
        let admitted = scribe
            .reserve_ingress_after_pressure_seal(MemoryCategory::Raw, ceiling / 2, "table")
            .expect("admits once ingress capacity is available");
        drop(admitted);

        scribe
            .shutdown(Instant::now() + Duration::from_secs(1))
            .await;
    }
}
