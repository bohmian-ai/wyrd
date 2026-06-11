use wyrd_client::error::WyrdClientError;
use wyrd_client::transport::Flushable;

/// Minimal in-memory queue used to verify the trait shape. It buffers a count
/// and supports failure injection on the nth flush.
struct CountingQueue {
    buffered: usize,
    fail_on_flush: Option<u32>,
    flush_calls: u32,
}

impl CountingQueue {
    fn new(initial: usize, fail_on_flush: Option<u32>) -> Self {
        Self {
            buffered: initial,
            fail_on_flush,
            flush_calls: 0,
        }
    }
}

impl Flushable for CountingQueue {
    async fn flush(&mut self) -> Result<usize, WyrdClientError> {
        self.flush_calls += 1;
        if let Some(n) = self.fail_on_flush {
            if self.flush_calls == n {
                return Err(WyrdClientError::TransportDown {
                    transport: "mock".to_string(),
                    message: "injected".to_string(),
                });
            }
        }
        let drained = self.buffered;
        self.buffered = 0;
        Ok(drained)
    }

    fn len(&self) -> usize {
        self.buffered
    }
}

#[tokio::test]
async fn flushable_returns_ok_count_on_success() {
    let mut q = CountingQueue::new(7, None);
    let n = q.flush().await.unwrap();
    assert_eq!(n, 7);
    assert_eq!(q.len(), 0);
    assert!(q.is_empty());
}

#[tokio::test]
async fn flushable_failed_flush_preserves_buffered_count() {
    let mut q = CountingQueue::new(5, Some(1));
    let err = q.flush().await.unwrap_err();
    assert!(matches!(err, WyrdClientError::TransportDown { .. }));
    assert_eq!(q.len(), 5);
    assert!(!q.is_empty());
}

#[tokio::test]
async fn flushable_subsequent_flush_after_failure_succeeds() {
    let mut q = CountingQueue::new(3, Some(1));
    let _ = q.flush().await.unwrap_err();
    let n = q.flush().await.unwrap();
    assert_eq!(n, 3);
    assert_eq!(q.len(), 0);
}
