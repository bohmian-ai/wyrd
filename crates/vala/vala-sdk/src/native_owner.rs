//! PyO3-free ownership seam for terminal-safe Bifrost query streams.

#[cfg(any(feature = "python", test))]
use arrow::record_batch::RecordBatch;

#[cfg(feature = "python")]
use crate::QueryResultStream;
#[cfg(any(feature = "python", test))]
use crate::ValaSdkError;

/// Rust-owned query stream implementation shared by Python and native tests.
#[cfg(any(feature = "python", test))]
pub(crate) enum NativeStreamOwner {
    /// Production Vala stream.
    #[cfg(feature = "python")]
    Production(Box<QueryResultStream>),
    #[cfg(test)]
    /// Injectable owner used by deterministic native-owner tests.
    Test(TestStreamOwner),
}

#[cfg(any(feature = "python", test))]
impl NativeStreamOwner {
    /// Poll one decoded batch or terminal state from the underlying owner.
    ///
    /// # Errors
    /// Returns the owner's protocol, Arrow, transport, terminal, or
    /// incomplete-stream error without changing its retained terminal state.
    pub(crate) async fn next_batch(&mut self) -> Result<Option<RecordBatch>, ValaSdkError> {
        match self {
            #[cfg(feature = "python")]
            Self::Production(stream) => stream.next_batch().await,
            #[cfg(test)]
            Self::Test(stream) => stream.next_batch().await,
        }
    }

    /// Return terminal metadata retained by the underlying owner.
    pub(crate) fn terminal(&self) -> Option<&wyrd_spec::vala::api::QueryTerminalFrame> {
        match self {
            #[cfg(feature = "python")]
            Self::Production(stream) => stream.terminal(),
            #[cfg(test)]
            Self::Test(stream) => stream.terminal(),
        }
    }
}

#[cfg(test)]
/// Deterministic stream owner used to exercise native cleanup branches.
pub(crate) struct TestStreamOwner {
    /// Results returned by successive native polls.
    next: std::collections::VecDeque<Result<Option<RecordBatch>, ValaSdkError>>,
    /// Terminal metadata exposed while projecting a failed or successful end.
    terminal: Option<wyrd_spec::vala::api::QueryTerminalFrame>,
    /// Sentinel set when the native owner drops this stream.
    dropped: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Counts calls into the injected owner so closed polls prove no re-entry.
    polls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[cfg(test)]
impl TestStreamOwner {
    /// Create an owner with injected poll results and terminal metadata.
    pub(crate) fn new(
        next: std::collections::VecDeque<Result<Option<RecordBatch>, ValaSdkError>>,
        terminal: Option<wyrd_spec::vala::api::QueryTerminalFrame>,
        dropped: std::sync::Arc<std::sync::atomic::AtomicBool>,
        polls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        Self {
            next,
            terminal,
            dropped,
            polls,
        }
    }

    /// Return the next injected result.
    ///
    /// # Errors
    /// Returns the injected error, or [`ValaSdkError::IncompleteQueryStream`]
    /// after all injected results have been consumed.
    async fn next_batch(&mut self) -> Result<Option<RecordBatch>, ValaSdkError> {
        self.polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.next
            .pop_front()
            .unwrap_or(Err(ValaSdkError::IncompleteQueryStream))
    }

    /// Return injected terminal metadata.
    fn terminal(&self) -> Option<&wyrd_spec::vala::api::QueryTerminalFrame> {
        self.terminal.as_ref()
    }
}

#[cfg(test)]
impl Drop for TestStreamOwner {
    /// Mark the sentinel before the native method returns its error.
    fn drop(&mut self) {
        self.dropped
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::{NativeStreamOwner, TestStreamOwner};
    use crate::ValaSdkError;

    /// The native owner returns injected errors without Python interaction.
    #[test]
    fn native_owner_returns_error_and_drops_without_python() {
        let dropped = Arc::new(AtomicBool::new(false));
        let polls = Arc::new(AtomicUsize::new(0));
        let test_owner = TestStreamOwner::new(
            std::collections::VecDeque::from([Err(ValaSdkError::IncompleteQueryStream)]),
            None,
            Arc::clone(&dropped),
            Arc::clone(&polls),
        );
        let mut owner = NativeStreamOwner::Test(test_owner);
        let error = wyrd_runtime::runtime()
            .block_on(owner.next_batch())
            .expect_err("incomplete stream is returned by native owner");
        assert!(matches!(error, ValaSdkError::IncompleteQueryStream));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        drop(owner);
        assert!(dropped.load(Ordering::SeqCst));
    }

    /// The native owner returns a batch and retains terminal state without PyO3.
    #[test]
    fn native_owner_returns_batch_without_python() {
        let dropped = Arc::new(AtomicBool::new(false));
        let polls = Arc::new(AtomicUsize::new(0));
        let test_owner = TestStreamOwner::new(
            std::collections::VecDeque::from([Ok(None)]),
            None,
            Arc::clone(&dropped),
            Arc::clone(&polls),
        );
        let mut owner = NativeStreamOwner::Test(test_owner);
        let result = wyrd_runtime::runtime().block_on(owner.next_batch());
        assert!(result.expect("native poll succeeds").is_none());
        assert!(owner.terminal().is_none());
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        drop(owner);
        assert!(dropped.load(Ordering::SeqCst));
    }
}
