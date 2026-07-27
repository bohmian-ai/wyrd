//! The swap seam: [`BatchSink`], [`SealedBatch`], and the test [`MockSink`].
//!
//! `BatchSink` is the single most important type in the crate — the swap point
//! the client→server flow marks as "the only kind-specific code". The producer
//! hands a [`SealedBatch`] to a `dyn BatchSink` and is done; a new observation
//! kind is a new impl in its own surface crate, with zero change here.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use wyrd_spec::error::WyrdError;

/// One sealed, ready-to-ship batch handed from the producer to a sink.
///
/// Produced by the flush task; consumed by exactly one [`BatchSink::send`] call.
/// Carries **no `run_id` field**: correlation is per-row inside `frames`, never
/// batch-grain (a sealed batch freely mixes cards and runs — C-01).
#[derive(Debug, Clone)]
pub struct SealedBatch {
    /// Destination table identifier (`"namespace.table"`). Opaque to the queue —
    /// the sink forwards it to its typed service.
    pub table: String,
    /// Durable idempotency key. Minted **once at seal** (UUIDv7 bytes) and
    /// **stable across transport retries**. The producer never regenerates it.
    pub batch_id: [u8; 16],
    /// Arrow IPC stream bytes for one logical `RecordBatch` — user columns plus
    /// the two per-row correlation columns `card_ref` and `run_id`. The server
    /// stamps the per-request system columns; correlation rides inside `frames`,
    /// never as `SealedBatch` metadata.
    pub frames: Vec<u8>,
    /// Row count in this batch — for ack reconciliation and metrics.
    pub rows: u64,
}

/// The swap seam: one implementation per observation-kind transport.
///
/// `wyrd-queue` ships [`MockSink`] (tests); `vala-sdk` ships `BifrostIngestSink`
/// (Record); future kinds add their own sinks in their own crates — none of
/// which touch this file.
///
/// `Arc<dyn BatchSink>` is the chosen shape: runtime extensibility is the
/// explicit intent, and `send` is called once **per flush** (per N rows), not
/// per row — so dynamic dispatch is off the hot path (the hot path is `enqueue`,
/// which never touches the sink).
#[async_trait::async_trait]
pub trait BatchSink: Send + Sync + 'static {
    /// Ship one sealed batch to its typed service; return `rows_accepted`.
    ///
    /// Contract:
    /// - **Idempotent on `batch.batch_id`** — the server dedups, so a re-send of
    ///   the same id must be safe (the producer relies on this for retry).
    /// - Returns a [`WyrdError`] on failure. The producer inspects the error to
    ///   decide re-buffer vs. drop-with-log.
    async fn send(&self, batch: SealedBatch) -> Result<u64, WyrdError>;
}

/// In-memory loopback sink for producer tests.
///
/// Records every batch it receives and can be configured to fail a configurable
/// number of sends (to exercise re-buffer without a network).
#[derive(Debug, Default)]
pub struct MockSink {
    received: Mutex<Vec<SealedBatch>>,
    fail_next: AtomicUsize,
}

impl MockSink {
    /// Construct an empty mock sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Arrange for the next `n` sends to fail with a transport [`WyrdError`].
    ///
    /// Each failing send decrements the counter; once it reaches zero, sends
    /// succeed and record normally. A failing send records nothing — the
    /// producer re-buffers those rows.
    pub fn fail_next(&self, n: usize) {
        self.fail_next.store(n, Ordering::SeqCst);
    }

    /// Return a snapshot of every batch this sink has accepted (assertion surface).
    #[must_use]
    pub fn received(&self) -> Vec<SealedBatch> {
        self.received.lock().expect("mock sink poisoned").clone()
    }
}

#[async_trait::async_trait]
impl BatchSink for MockSink {
    async fn send(&self, batch: SealedBatch) -> Result<u64, WyrdError> {
        loop {
            let remaining = self.fail_next.load(Ordering::SeqCst);
            if remaining == 0 {
                break;
            }
            if self
                .fail_next
                .compare_exchange(remaining, remaining - 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Err(WyrdError::Internal {
                    message: "mock sink forced failure".to_owned(),
                    details: serde_json::json!({ "mock": true }),
                });
            }
        }
        let rows = batch.rows;
        self.received
            .lock()
            .expect("mock sink poisoned")
            .push(batch);
        Ok(rows)
    }
}
