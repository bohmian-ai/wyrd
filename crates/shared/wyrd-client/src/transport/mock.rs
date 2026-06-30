//! In-memory loopback transport for tests.
//!
//! [`MockTransport`] is a zero-network transport that records every drain call
//! in an in-process buffer and optionally injects a
//! [`WyrdClientError::TransportDown`] on a configured call number. Use it in
//! unit tests that need a concrete transport without a live server.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use tokio::sync::Mutex;

use crate::error::WyrdClientError;
use crate::transport::config::MockConfig;

/// A single drain call recorded by [`MockTransport`].
#[derive(Debug, Clone)]
pub struct MockRecord {
    /// Opaque payload bytes recorded from the drain call.
    pub payload: Vec<u8>,
}

/// In-memory loopback transport for unit tests.
///
/// Constructed from a [`MockConfig`]. Drain calls are recorded in an internal
/// buffer; `fail_on_drain` injects a [`WyrdClientError::TransportDown`] on the
/// configured call number (1-based). Use [`MockTransport::records`] to assert
/// on what was drained.
#[derive(Clone, Debug)]
pub struct MockTransport {
    label: String,
    fail_on_drain: Option<u32>,
    drain_count: Arc<AtomicU32>,
    records: Arc<Mutex<Vec<MockRecord>>>,
}

impl MockTransport {
    /// Build a transport from a [`MockConfig`].
    #[must_use]
    pub fn new(config: &MockConfig) -> Self {
        Self {
            label: config.label.clone(),
            fail_on_drain: config.fail_on_drain,
            drain_count: Arc::new(AtomicU32::new(0)),
            records: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Return the label from the original [`MockConfig`].
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Number of drain calls made (including failed ones).
    #[must_use]
    pub fn drain_count(&self) -> u32 {
        self.drain_count.load(Ordering::SeqCst)
    }

    /// Record a drain payload.
    ///
    /// Increments the internal call counter. If `fail_on_drain` is configured
    /// and the current call number matches, returns
    /// [`WyrdClientError::TransportDown`] without recording the payload.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::TransportDown`] when `fail_on_drain` matches
    /// the current call count.
    pub async fn drain(&self, payload: Vec<u8>) -> Result<(), WyrdClientError> {
        let count = self.drain_count.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_on_drain == Some(count) {
            return Err(WyrdClientError::TransportDown {
                transport: "mock".to_owned(),
                message: format!("mock transport configured to fail on drain call {count}"),
            });
        }
        self.records.lock().await.push(MockRecord { payload });
        Ok(())
    }

    /// Return all successfully recorded drain payloads.
    pub async fn records(&self) -> Vec<MockRecord> {
        self.records.lock().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::MockTransport;
    use crate::error::WyrdClientError;
    use crate::transport::config::MockConfig;

    fn transport(fail_at: Option<u32>) -> MockTransport {
        MockTransport::new(&MockConfig {
            label: "test".to_owned(),
            fail_on_drain: fail_at,
        })
    }

    #[tokio::test]
    async fn drain_records_payload() {
        let t = transport(None);
        t.drain(b"hello".to_vec()).await.expect("drain ok");
        let records = t.records().await;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].payload, b"hello");
    }

    #[tokio::test]
    async fn drain_count_increments_on_each_drain() {
        let t = transport(None);
        t.drain(vec![1]).await.expect("drain 1");
        t.drain(vec![2]).await.expect("drain 2");
        assert_eq!(t.drain_count(), 2);
    }

    #[tokio::test]
    async fn fail_on_drain_injects_transport_down() {
        let t = transport(Some(1));
        let err = t.drain(vec![]).await.expect_err("should fail on call 1");
        assert!(
            matches!(err, WyrdClientError::TransportDown { transport, .. } if transport == "mock")
        );
        assert_eq!(t.drain_count(), 1);
    }

    #[tokio::test]
    async fn fail_on_drain_second_call() {
        let t = transport(Some(2));
        t.drain(b"ok".to_vec()).await.expect("call 1 ok");
        let err = t.drain(b"fail".to_vec()).await.expect_err("call 2 fails");
        assert!(matches!(err, WyrdClientError::TransportDown { .. }));
        // Only the successful call is recorded.
        let records = t.records().await;
        assert_eq!(records.len(), 1);
    }

    #[tokio::test]
    async fn label_matches_config() {
        let t = MockTransport::new(&MockConfig {
            label: "my-buffer".to_owned(),
            fail_on_drain: None,
        });
        assert_eq!(t.label(), "my-buffer");
    }

    #[tokio::test]
    async fn clone_shares_state() {
        let t1 = transport(None);
        let t2 = t1.clone();
        t1.drain(b"from-t1".to_vec()).await.expect("drain");
        let records = t2.records().await;
        assert_eq!(records.len(), 1, "clone must share the same buffer");
    }
}
