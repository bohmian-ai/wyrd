//! Per-record-queue flush contract.
//!
//! The `Flushable` trait is the producer-side contract for every per-record
//! queue (`PsiFeatureQueue`, `SpcFeatureQueue`, `CustomMetricQueue`,
//! `EvalRecordQueue`, `AgentTaskQueue`, `DatasetQueue`, `ObservationQueue`,
//! `SpanQueue`, `QueueBus`). The per-record implementations land in PR4.0
//! section 22 / PR4.7.

use std::future::Future;

use crate::error::WyrdClientError;

/// Per-record-type queue contract.
///
/// Implementations are concrete generics; the executor spawns flush futures
/// on the producer's tokio runtime. `Send + 'static` is required so the
/// futures cross the channel boundary cleanly.
///
/// # Flush semantics
///
/// `flush` resolves `Ok(n)` with the count of records actually flushed onto
/// the wire, or `Err(WyrdClientError)` carrying a transport-specific failure.
/// After `Err`, `len()` must reflect the records that were not successfully
/// flushed: no silent drop and no half-flush bookkeeping leak. Per-queue
/// implementations in PR4.7 honor this contract by buffering on failure and
/// re-attempting on the next flush call.
pub trait Flushable: Send + 'static {
    /// Drain buffered records onto the wire. Resolves `Ok(n)` with the count
    /// of flushed records, or `Err(WyrdClientError)`.
    fn flush(&mut self) -> impl Future<Output = Result<usize, WyrdClientError>> + Send;

    /// Buffered (not-yet-flushed) record count.
    fn len(&self) -> usize;

    /// Returns `true` when this queue has no buffered records.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
