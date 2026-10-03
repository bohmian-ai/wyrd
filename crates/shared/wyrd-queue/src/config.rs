//! Producer/queue configuration.

use std::time::Duration;

use crate::error::WyrdQueueError;

/// Tuning knobs for one client handle's producers.
///
/// The only memory bound is `client_byte_limit_bytes`, shared by every
/// producer of the handle; nothing is preallocated from it. Values are
/// honoured as configured once [`Self::validate`] accepts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueConfig {
    /// Handle-wide ceiling for every client-owned row, sealed-batch,
    /// in-flight, and retained byte. One `max_message_bytes` of it is kept
    /// back from admission so admitted rows can always be sealed.
    pub client_byte_limit_bytes: usize,
    /// How long the first staged row may wait for neighbours before its
    /// producer seals a batch. `0` seals as soon as rows are staged.
    pub linger_ms: u64,
    /// Sealed batches one producer may have in flight at once.
    pub max_in_flight: usize,
    /// Send deadline (ms) for one sink attempt; an elapsed deadline retains
    /// the batch as an ambiguous outcome.
    pub flush_timeout_ms: u64,
    /// Sealed IPC ceiling (bytes), which is also the staged-byte seal target.
    /// A single row over the ceiling → `WYRD_CLIENT_413_PAYLOAD_TOO_LARGE`.
    pub max_message_bytes: usize,
}

impl QueueConfig {
    /// The default handle-wide byte budget: 256 MiB.
    pub const DEFAULT_CLIENT_BYTE_LIMIT: usize = 256 * 1024 * 1024;

    /// The default configuration with an optional handle-wide byte budget.
    ///
    /// The one door SDK boundaries use for their byte-budget override: `None`
    /// keeps [`Self::DEFAULT_CLIENT_BYTE_LIMIT`]. The value is not checked
    /// here; connecting refuses a budget that cannot seal a message through
    /// [`Self::validate`].
    #[must_use]
    pub fn with_client_byte_limit(client_byte_limit_bytes: Option<usize>) -> Self {
        Self {
            client_byte_limit_bytes: client_byte_limit_bytes
                .unwrap_or(Self::DEFAULT_CLIENT_BYTE_LIMIT),
            ..Self::default()
        }
    }

    /// Checks that the configuration can admit and seal at least one message.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::ConfigInvalid`] when `max_message_bytes` or
    /// `max_in_flight` is zero, or when `client_byte_limit_bytes` is smaller
    /// than one message of admission plus one message of sealing headroom.
    pub fn validate(&self) -> Result<(), WyrdQueueError> {
        if self.max_message_bytes == 0 {
            return Err(WyrdQueueError::ConfigInvalid {
                field: "max_message_bytes",
                reason: "must be positive".to_owned(),
            });
        }
        if self.max_in_flight == 0 {
            return Err(WyrdQueueError::ConfigInvalid {
                field: "max_in_flight",
                reason: "must be positive".to_owned(),
            });
        }
        let minimum = self.max_message_bytes.saturating_mul(2);
        if self.client_byte_limit_bytes < minimum {
            return Err(WyrdQueueError::ConfigInvalid {
                field: "client_byte_limit_bytes",
                reason: format!(
                    "{} is below {minimum}: one max_message_bytes of rows plus one of sealing headroom",
                    self.client_byte_limit_bytes
                ),
            });
        }
        Ok(())
    }

    /// Returns the linger before a staged row forces a seal.
    #[must_use]
    pub(crate) fn linger(&self) -> Duration {
        Duration::from_millis(self.linger_ms)
    }

    /// Returns the bounded send deadline, treating zero as one millisecond.
    ///
    /// A nonzero timeout guarantees that a borrowed sink future can be
    /// cancelled while the queue retains its sealed owner for ambiguity
    /// resolution; zero would otherwise create an accidental busy timeout.
    #[must_use]
    pub(crate) fn flush_timeout(&self) -> Duration {
        Duration::from_millis(self.flush_timeout_ms.max(1))
    }
}

impl Default for QueueConfig {
    /// 256 MiB budget, 5 ms linger, one send in flight, 30 s send
    /// deadline, and 4 MiB messages. One send in flight is the smallest value
    /// that sustains 50,000 rows/s in the AC-041 ingest benchmark
    /// (`mise run bench:bifrost:ingest-capacity`): a busy slot lets staging
    /// grow, so batches scale with load rather than with the slot count.
    fn default() -> Self {
        Self {
            client_byte_limit_bytes: Self::DEFAULT_CLIENT_BYTE_LIMIT,
            linger_ms: 5,
            max_in_flight: 1,
            flush_timeout_ms: 30_000,
            max_message_bytes: 4 * 1024 * 1024,
        }
    }
}
