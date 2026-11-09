//! Test-only faults injected between the result writer and Scribe.
//!
//! [`PublicationFault`] wraps the pod's real Scribe in a decorator the
//! runtime's result outbox writes through, so a runtime test can lose or hold
//! the acknowledgement of one table's batch, settle the outbox, and observe
//! every batch it submitted.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use vala_bifrost_redux::catalog::{TableRef, TableUid};
use vala_bifrost_redux::contracts::{FrameAdmission, Scribe, ScribeError, ScribeIngressFrame};
use wyrd_spec::DataTenantId;
use wyrd_sql::WyrdPostgres;

use crate::scribe_outbox::{ScribeOutbox, ScribeSink, payload_ipc};

/// One-shot faults on result writes, plus a record of every submitted batch.
///
/// `lose_ack_next` lets Scribe durably accept one table's batch and then
/// reports it busy, so the outbox must resend the identical batch;
/// `reject_next` refuses one table's next batch terminally without
/// forwarding it; `hold_next` keeps one table's next batch from Scribe until
/// released. Each fault fires once.
#[derive(Clone, Default)]
pub struct PublicationFault {
    /// One-shot lost acknowledgement after this table's durable batch.
    lose_ack_on: Arc<Mutex<Option<String>>>,
    /// One-shot terminal refusal of this table's next batch.
    reject_on: Arc<Mutex<Option<String>>>,
    /// One-shot hold before this table's next batch reaches Scribe.
    hold_on: Arc<Mutex<Option<(String, Hold)>>>,
    /// Every batch submitted, in submission order.
    sent: Arc<Mutex<Vec<SentBatch>>>,
    /// The result outbox the runtime writes through, once composed.
    outbox: Arc<OnceLock<Arc<ScribeOutbox>>>,
}

/// One result batch exactly as the writer submitted it to Scribe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentBatch {
    /// Fully qualified destination table.
    pub table: String,
    /// The batch ID Scribe deduplicates on.
    pub batch_id: uuid::Uuid,
    /// The batch's rows as one Arrow IPC stream: the stream it carried, or
    /// its in-process rows encoded.
    pub bytes: Vec<u8>,
}

impl PublicationFault {
    /// Report the next durably accepted batch for `table` as busy.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned.
    pub fn lose_ack_next(&self, table: &str) {
        *self.lose_ack_on.lock().expect("fault lock") = Some(table.to_owned());
    }

    /// Refuse the next batch for `table` terminally, as Scribe refuses a
    /// batch it cannot admit, without forwarding it.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned.
    pub fn reject_next(&self, table: &str) {
        *self.reject_on.lock().expect("fault lock") = Some(table.to_owned());
    }

    /// Hold the next batch for `table` before it reaches Scribe until the
    /// returned [`Hold`] is released.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned.
    pub fn hold_next(&self, table: &str) -> Hold {
        let hold = Hold::default();
        *self.hold_on.lock().expect("fault lock") = Some((table.to_owned(), hold.clone()));
        hold
    }

    /// Every batch submitted once everything staged so far is written, in
    /// submission order.
    ///
    /// Settles the composed result outbox for up to thirty seconds first, so
    /// a run that completed after staging has its batches recorded.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned or staged results stay
    /// unwritten past the wait.
    pub async fn sent(&self) -> Vec<SentBatch> {
        if let Some(outbox) = self.outbox.get() {
            let left = outbox
                .settle(Instant::now() + Duration::from_secs(30))
                .await;
            assert_eq!(left, 0, "staged results are written");
        }
        self.sent.lock().expect("fault lock").clone()
    }

    /// The result outbox writing through these faults to `inner` and
    /// recording grouped activity through `groups`, created on first use and
    /// shared by every later caller.
    ///
    /// The outbox's writer starts on the shared Wyrd runtime, like every
    /// server owner, so a test server's teardown can still drive and cancel
    /// its in-flight writes after the test's own runtime stops polling.
    pub(crate) fn outbox(&self, inner: Arc<dyn Scribe>, groups: WyrdPostgres) -> Arc<ScribeOutbox> {
        let fault = self.clone();
        Arc::clone(self.outbox.get_or_init(move || {
            let _shared = wyrd_runtime::runtime().enter();
            ScribeSink::local_outbox(Arc::new(FaultScribe { inner, fault }), Some(groups))
        }))
    }
}

/// One held batch: [`arrived`](Self::arrived) once it reaches the fault,
/// forwarded to Scribe once [`release`](Self::release)d.
#[derive(Clone, Default)]
pub struct Hold {
    /// Cancelled once the held batch reaches the fault.
    arrived: CancellationToken,
    /// Cancelled to forward the held batch.
    released: CancellationToken,
}

impl Hold {
    /// Wait until the held batch reaches the fault.
    pub async fn arrived(&self) {
        self.arrived.cancelled().await;
    }

    /// Forward the held batch to Scribe.
    pub fn release(&self) {
        self.released.cancel();
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

    /// Record and forward `frame`, rejecting it, holding it, or losing its
    /// acknowledgement when armed.
    ///
    /// # Errors
    /// Returns `FingerprintMismatch` for an armed rejection, `IngestBusy` for
    /// an armed lost acknowledgement, and otherwise
    /// the real Scribe's outcome.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned.
    async fn ingest_frame(&self, frame: ScribeIngressFrame) -> Result<FrameAdmission, ScribeError> {
        let table = frame.table.fqn();
        self.fault.sent.lock().expect("fault lock").push(SentBatch {
            table: table.clone(),
            batch_id: frame.batch_id,
            bytes: payload_ipc(&frame.payload).to_vec(),
        });
        if take_if(&self.fault.reject_on, &table) {
            return Err(ScribeError::FingerprintMismatch { table });
        }
        let held = {
            let mut armed = self.fault.hold_on.lock().expect("fault lock");
            match armed.take() {
                Some((held, hold)) if held == table => Some(hold),
                other => {
                    *armed = other;
                    None
                }
            }
        };
        if let Some(hold) = held {
            hold.arrived.cancel();
            hold.released.cancelled().await;
        }
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
