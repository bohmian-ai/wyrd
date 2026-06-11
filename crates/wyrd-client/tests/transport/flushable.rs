use wyrd_client::error::WyrdClientError;
use wyrd_client::transport::Flushable;

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
        if self.fail_on_flush == Some(self.flush_calls) {
            return Err(WyrdClientError::TransportDown {
                transport: "mock".to_string(),
                message: "injected".to_string(),
            });
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
    let mut queue = CountingQueue::new(7, None);
    let count = queue.flush().await.expect("flush succeeds");
    assert_eq!(count, 7);
    assert_eq!(queue.len(), 0);
    assert!(queue.is_empty());
}

#[tokio::test]
async fn flushable_failed_flush_preserves_buffered_count() {
    let mut queue = CountingQueue::new(5, Some(1));
    let error = queue.flush().await.expect_err("flush fails");
    assert!(matches!(error, WyrdClientError::TransportDown { .. }));
    assert_eq!(queue.len(), 5);
    assert!(!queue.is_empty());
}

#[tokio::test]
async fn flushable_subsequent_flush_after_failure_succeeds() {
    let mut queue = CountingQueue::new(3, Some(1));
    let _ = queue.flush().await.expect_err("first flush fails");
    let count = queue.flush().await.expect("second flush succeeds");
    assert_eq!(count, 3);
    assert_eq!(queue.len(), 0);
}
