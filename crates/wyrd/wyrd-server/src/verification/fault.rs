//! Test-only faults injected between the result writer and Scribe.
//!
//! [`PublicationFault`] wraps the pod's real Scribe in a decorator the
//! runtime's result writer submits through, so a runtime test can refuse,
//! stall, or lose the acknowledgement of one table's batch and observe every
//! batch the writer submitted.

use std::sync::{Arc, Mutex};

use vala_bifrost_redux::catalog::{TableRef, TableUid};
use vala_bifrost_redux::contracts::{
    FrameAdmission, IngressPayload, Scribe, ScribeError, ScribeIngressFrame,
};
use wyrd_spec::DataTenantId;

/// One-shot faults on result writes, plus a record of every submitted batch.
///
/// `hang_next` stalls forever before one table's batch,
/// modelling a crash mid-write; `lose_ack_next` lets Scribe durably accept
/// one table's batch and then reports it busy, so the writer must resend the
/// identical batch. Each fault fires once.
#[derive(Debug, Clone, Default)]
pub struct PublicationFault {
    /// One-shot stall before this table's next batch.
    hang_on: Arc<Mutex<Option<String>>>,
    /// One-shot lost acknowledgement after this table's durable batch.
    lose_ack_on: Arc<Mutex<Option<String>>>,
    /// Every batch submitted past a stall, in submission order.
    sent: Arc<Mutex<Vec<SentBatch>>>,
}

/// One result batch exactly as the writer submitted it to Scribe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentBatch {
    /// Fully qualified destination table.
    pub table: String,
    /// The batch ID Scribe deduplicates on.
    pub batch_id: uuid::Uuid,
    /// The Arrow IPC stream.
    pub bytes: Vec<u8>,
}

impl PublicationFault {
    /// Stall forever before the next batch for `table`.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned.
    pub fn hang_next(&self, table: &str) {
        *self.hang_on.lock().expect("fault lock") = Some(table.to_owned());
    }

    /// Report the next durably accepted batch for `table` as busy.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned.
    pub fn lose_ack_next(&self, table: &str) {
        *self.lose_ack_on.lock().expect("fault lock") = Some(table.to_owned());
    }

    /// Every batch submitted so far, in submission order.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned.
    #[must_use]
    pub fn sent(&self) -> Vec<SentBatch> {
        self.sent.lock().expect("fault lock").clone()
    }

    /// Wrap `inner` so every submission passes through these faults.
    #[must_use]
    pub(crate) fn wrap(&self, inner: Arc<dyn Scribe>) -> FaultScribe {
        FaultScribe {
            inner,
            fault: self.clone(),
        }
    }
}

/// Scribe decorator applying a [`PublicationFault`] to every submission.
pub(crate) struct FaultScribe {
    /// The pod's real Scribe.
    inner: Arc<dyn Scribe>,
    /// Shared fault state.
    fault: PublicationFault,
}

#[async_trait::async_trait]
impl Scribe for FaultScribe {
    /// Mirrors the real Scribe's readiness.
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }

    /// Stall, or record and forward `frame`, as armed for its table.
    ///
    /// # Errors
    /// Returns `IngestBusy` for an armed lost acknowledgement, and otherwise
    /// the real Scribe's outcome.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned.
    async fn ingest_frame(&self, frame: ScribeIngressFrame) -> Result<FrameAdmission, ScribeError> {
        let table = frame.table.fqn();
        if take_if(&self.fault.hang_on, &table) {
            std::future::pending::<()>().await;
        }
        let IngressPayload::ArrowIpc(bytes) = &frame.payload else {
            return self.inner.ingest_frame(frame).await;
        };
        self.fault.sent.lock().expect("fault lock").push(SentBatch {
            table: table.clone(),
            batch_id: frame.batch_id,
            bytes: bytes.to_vec(),
        });
        let admission = self.inner.ingest_frame(frame).await?;
        if take_if(&self.fault.lose_ack_on, &table) {
            return Err(ScribeError::IngestBusy { table });
        }
        Ok(admission)
    }

    /// Forwards table resolution to the real Scribe.
    ///
    /// # Errors
    /// Returns the real Scribe's resolution error.
    async fn resolve_write_table(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
    ) -> Result<TableUid, ScribeError> {
        self.inner.resolve_write_table(tenant, table).await
    }
}

/// Clear `slot` and return true when it names `table`.
///
/// # Panics
/// Panics when the fault lock is poisoned.
fn take_if(slot: &Mutex<Option<String>>, table: &str) -> bool {
    let mut armed = slot.lock().expect("fault lock");
    if armed.as_deref() == Some(table) {
        *armed = None;
        return true;
    }
    false
}
