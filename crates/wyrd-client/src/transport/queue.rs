//! Per-record queue flush contract.

use std::future::Future;

use crate::error::WyrdClientError;

/// Per-record-type queue contract.
///
/// `flush` returns the count of records successfully sent. After an error,
/// `len()` must reflect records that remain buffered and were not flushed.
pub trait Flushable: Send + 'static {
    /// Drain buffered records onto the wire.
    fn flush(&mut self) -> impl Future<Output = Result<usize, WyrdClientError>> + Send;

    /// Buffered record count.
    fn len(&self) -> usize;

    /// Returns `true` when no records are buffered.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
