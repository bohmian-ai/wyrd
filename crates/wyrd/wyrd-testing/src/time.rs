//! Virtual-time helpers for Wyrd integration tests.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static CLOCK_PAUSED: AtomicBool = AtomicBool::new(false);

/// Owned handle to the Tokio test clock.
#[derive(Clone, Debug)]
pub struct ClockHandle(());

impl ClockHandle {
    pub(crate) const fn new() -> Self {
        Self(())
    }

    /// Advance the virtual clock.
    pub async fn advance(&self, duration: Duration) {
        if !CLOCK_PAUSED.swap(true, Ordering::SeqCst) {
            let _ = std::panic::catch_unwind(tokio::time::pause);
        }
        tokio::time::advance(duration).await;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::ClockHandle;

    #[tokio::test]
    async fn advance_moves_virtual_clock() {
        let handle = ClockHandle::new();
        handle.advance(Duration::ZERO).await;
        let sleep = tokio::time::sleep(Duration::from_secs(60));
        tokio::pin!(sleep);

        tokio::select! {
            () = &mut sleep => panic!("sleep should still be pending"),
            () = tokio::task::yield_now() => {}
        }

        handle.advance(Duration::from_secs(60)).await;
        sleep.await;
    }
}
