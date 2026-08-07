//! Local, lossy wake-up signals shared by Scribe persistence and Forge.

use chrono::NaiveDate;
use thiserror::Error;

use crate::catalog::TenantTableBinding;

/// The exact durable table and partition day whose file-list row was committed.
#[derive(Debug, Clone)]
pub struct StagingFileCommitted {
    /// Resolved tenant/table identity validated by the committing Scribe path.
    binding: TenantTableBinding,
    /// Event-time day represented by the committed staged file.
    partition_day: NaiveDate,
}

impl StagingFileCommitted {
    /// Construct a wake-up signal without carrying any durable file metadata.
    #[must_use]
    pub fn new(binding: TenantTableBinding, partition_day: NaiveDate) -> Self {
        Self {
            binding,
            partition_day,
        }
    }

    /// Consume the signal into the exact identity Forge uses for discovery.
    pub(crate) fn into_parts(self) -> (TenantTableBinding, NaiveDate) {
        (self.binding, self.partition_day)
    }
}

/// Result of attempting a non-blocking staging-file publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StagingPublishOutcome {
    /// The bounded channel accepted the signal.
    Published,
    /// The bounded channel had no capacity; the signal was intentionally lost.
    DroppedFull,
    /// The Forge receiver was closed; the signal was intentionally lost.
    DroppedClosed,
}

/// Errors raised while constructing the local maintenance signal channel.
#[derive(Debug, Error)]
pub enum MaintenanceChannelError {
    /// A zero-capacity channel would violate the bounded asynchronous contract.
    #[error("staging-file channel capacity must be positive")]
    ZeroCapacity,
}

/// Cloneable producer for bounded, lossy staging-file wake-ups.
#[derive(Debug, Clone)]
pub struct StagingFilePublisher {
    /// Tokio sender whose capacity bounds retained wake-up signals.
    sender: tokio::sync::mpsc::Sender<StagingFileCommitted>,
}

/// Receiver for local staging-file wake-ups consumed by Forge.
#[derive(Debug)]
pub struct StagingFileInbox {
    /// Tokio receiver paired with the bounded publisher.
    receiver: tokio::sync::mpsc::Receiver<StagingFileCommitted>,
}

impl StagingFilePublisher {
    /// Attempt to publish immediately, never waiting on Forge or persistence.
    #[must_use]
    pub fn try_publish(&self, event: StagingFileCommitted) -> StagingPublishOutcome {
        match self.sender.try_send(event) {
            Ok(()) => StagingPublishOutcome::Published,
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                metrics::counter!("bifrost_staging_hint_dropped_total", "reason" => "full")
                    .increment(1);
                StagingPublishOutcome::DroppedFull
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                metrics::counter!("bifrost_staging_hint_dropped_total", "reason" => "closed")
                    .increment(1);
                StagingPublishOutcome::DroppedClosed
            }
        }
    }

    /// Return available bounded-channel capacity for lifecycle tests.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn capacity_for_test(&self) -> usize {
        self.sender.capacity()
    }
}

impl StagingFileInbox {
    /// Wait for the next wake-up, returning `None` after the producer closes.
    pub(crate) async fn recv(&mut self) -> Option<StagingFileCommitted> {
        self.receiver.recv().await
    }

    /// Poll for a wake-up without waiting for Scribe persistence.
    ///
    /// Gated to `test`/`test-support` because its only consumers are the inline
    /// lifecycle test and the `try_recv_for_test` wrapper; the production drain
    /// loop uses [`StagingFileInbox::recv`].
    ///
    /// # Errors
    /// Returns [`tokio::sync::mpsc::error::TryRecvError::Empty`] when no signal
    /// is currently queued, or `Closed` after the publisher is dropped.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn try_recv(
        &mut self,
    ) -> Result<StagingFileCommitted, tokio::sync::mpsc::error::TryRecvError> {
        self.receiver.try_recv()
    }

    /// Poll one advisory signal from a test-owned inbox without waiting.
    #[cfg(any(test, feature = "test-support"))]
    ///
    /// # Errors
    /// Returns [`tokio::sync::mpsc::error::TryRecvError::Empty`] when no signal
    /// is currently queued, or `Closed` after the publisher is dropped.
    pub fn try_recv_for_test(
        &mut self,
    ) -> Result<StagingFileCommitted, tokio::sync::mpsc::error::TryRecvError> {
        self.try_recv()
    }
}

/// Build a bounded local channel for advisory staging-file wake-ups.
///
/// # Errors
/// Returns [`MaintenanceChannelError::ZeroCapacity`] when `capacity` is zero.
pub fn staging_file_channel(
    capacity: usize,
) -> Result<(StagingFilePublisher, StagingFileInbox), MaintenanceChannelError> {
    if capacity == 0 {
        return Err(MaintenanceChannelError::ZeroCapacity);
    }
    let (sender, receiver) = tokio::sync::mpsc::channel(capacity);
    Ok((
        StagingFilePublisher { sender },
        StagingFileInbox { receiver },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::table_ref::TableRef;
    use crate::namespaces::BifrostNamespace;
    use wyrd_spec::DataTenantId;

    /// Zero capacity is rejected before Tokio channel construction.
    #[test]
    fn staging_file_channel_rejects_zero_capacity() {
        assert!(matches!(
            staging_file_channel(0),
            Err(MaintenanceChannelError::ZeroCapacity)
        ));
    }

    /// Publication reports bounded full and closed outcomes without waiting.
    #[tokio::test]
    async fn staging_file_publisher_reports_full_and_closed_without_waiting() {
        let (publisher, mut inbox) = staging_file_channel(1).expect("channel");
        let tenant = DataTenantId::new_v7();
        let binding =
            TenantTableBinding::resolve((tenant, TableRef::new(BifrostNamespace::Traces, "spans")))
                .expect("binding");
        let day = NaiveDate::from_ymd_opt(2026, 1, 2).expect("day");
        assert_eq!(
            publisher.try_publish(StagingFileCommitted::new(binding.clone(), day)),
            StagingPublishOutcome::Published
        );
        assert_eq!(
            publisher.try_publish(StagingFileCommitted::new(binding.clone(), day)),
            StagingPublishOutcome::DroppedFull
        );
        let _ = inbox.try_recv().expect("queued signal");
        drop(inbox);
        assert_eq!(
            publisher.try_publish(StagingFileCommitted::new(binding, day)),
            StagingPublishOutcome::DroppedClosed
        );
    }
}
