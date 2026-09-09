//! Deterministic snapshot-expiration boundaries used only by Forge tests.
//!
//! The gates exist so an integration test can hold one production expiration
//! future at an exact catalog boundary and observe durable state there. They
//! compile only with `test-support` and have no production consumer.

#![cfg(feature = "test-support")]

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

/// Deterministic one-shot catalog boundaries for Forge snapshot-expiration tests.
#[cfg(feature = "test-support")]
#[derive(Clone, Default)]
pub struct ExpiryTestControls {
    /// Expiry-lifecycle boundary before its first catalog submission.
    expiry_submit: Arc<ExpiryTestGate>,
    /// Expiry accepted-response boundary.
    expiry_accepted: Arc<ExpiryTestGate>,
}

/// One armed boundary with lossless arrival and release notifications.
#[cfg(feature = "test-support")]
#[derive(Default)]
struct ExpiryTestGate {
    /// Whether the next boundary must pause.
    armed: std::sync::atomic::AtomicBool,
    /// Whether the armed boundary has arrived.
    arrived: std::sync::atomic::AtomicBool,
    /// Whether the test released the boundary.
    released: std::sync::atomic::AtomicBool,
    /// Wakeup for arrival waiters.
    arrival: tokio::sync::Notify,
    /// Wakeup for the paused production future.
    release: tokio::sync::Notify,
}

#[cfg(feature = "test-support")]
impl ExpiryTestGate {
    /// Arms this boundary for one production arrival.
    fn arm(&self) {
        self.arrived
            .store(false, std::sync::atomic::Ordering::Release);
        self.released
            .store(false, std::sync::atomic::Ordering::Release);
        self.armed.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Waits until the armed production future reaches this boundary.
    ///
    /// The `Notified` future is pinned and enabled (registered against the
    /// `arrival` notify) BEFORE the final `arrived` re-check, so a `pause` that
    /// stores `arrived` and calls `notify_waiters` in the window between the
    /// check and the await cannot be a lost wakeup.
    async fn wait(&self) {
        loop {
            let notified = self.arrival.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.arrived.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    /// Releases the production future held at this boundary.
    fn release(&self) {
        self.released
            .store(true, std::sync::atomic::Ordering::Release);
        self.release.notify_waiters();
    }

    /// Pauses one armed production arrival until release or cancellation.
    async fn pause(&self, stop: &CancellationToken) -> bool {
        if !self.armed.swap(false, std::sync::atomic::Ordering::AcqRel) {
            return false;
        }
        self.arrived
            .store(true, std::sync::atomic::Ordering::Release);
        self.arrival.notify_waiters();
        loop {
            if self.released.load(std::sync::atomic::Ordering::Acquire) {
                return false;
            }
            tokio::select! {
                () = self.release.notified() => {}
                () = stop.cancelled() => return true,
            }
        }
    }
}

#[cfg(feature = "test-support")]
impl ExpiryTestControls {
    /// Pauses the expiry lifecycle before its first catalog submission when armed.
    pub(super) async fn pause_expiry_submission(&self, stop: &CancellationToken) -> bool {
        self.expiry_submit.pause(stop).await
    }

    /// Pauses after an accepted expiry response when armed.
    pub(super) async fn pause_expiry_accepted(&self, stop: &CancellationToken) -> bool {
        self.expiry_accepted.pause(stop).await
    }

    /// Arms the next expiry lifecycle before its first catalog submission.
    pub fn arm_expiry_submission(&self) {
        self.expiry_submit.arm();
    }
    /// Waits for the armed expiry lifecycle boundary.
    pub async fn wait_expiry_submission(&self) {
        self.expiry_submit.wait().await;
    }
    /// Releases the armed expiry lifecycle boundary.
    pub fn release_expiry_submission(&self) {
        self.expiry_submit.release();
    }
    /// Arms the boundary after the catalog accepts an expiry commit.
    pub fn arm_expiry_accepted(&self) {
        self.expiry_accepted.arm();
    }
    /// Waits for the accepted expiry response.
    pub async fn wait_expiry_accepted(&self) {
        self.expiry_accepted.wait().await;
    }
    /// Releases the accepted expiry response boundary.
    pub fn release_expiry_accepted(&self) {
        self.expiry_accepted.release();
    }
}
