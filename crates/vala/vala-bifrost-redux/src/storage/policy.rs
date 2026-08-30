//! The validated, immutable policy one Bifrost storage owner applies.

use std::time::Duration;

/// One storage policy, immutable for the owner's lifetime.
///
/// `Copy` rather than `Arc`-shared: it is five scalars, it never changes after
/// boot, and every read path wants it without an indirection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BifrostStoragePolicy {
    /// Per-attempt request timeout applied by the operator's timeout layer.
    request_timeout: Duration,
    /// Additional attempts a bounded read may make after its first.
    max_retries: u32,
    /// Wall-clock ceiling on a complete retry sequence, not one attempt.
    max_retry_elapsed: Duration,
    /// Coarse ceiling on concurrent backend requests for the whole node.
    max_concurrent_requests: usize,
    /// Charged-byte ceiling for decoded metadata; zero disables retention.
    metadata_cache_bytes: u64,
}

impl BifrostStoragePolicy {
    /// Builds one policy from its five configured scalars.
    ///
    /// Bound and cross-field validation belongs to the configuration owner and
    /// is not applied here yet; this constructor exists so the storage owner
    /// has a policy value to carry.
    #[must_use]
    pub const fn new(
        request_timeout_ms: u64,
        max_retries: u32,
        max_retry_elapsed_ms: u64,
        max_concurrent_requests: usize,
        metadata_cache_bytes: u64,
    ) -> Self {
        Self {
            request_timeout: Duration::from_millis(request_timeout_ms),
            max_retries,
            max_retry_elapsed: Duration::from_millis(max_retry_elapsed_ms),
            max_concurrent_requests,
            metadata_cache_bytes,
        }
    }

    /// Returns the per-attempt request timeout.
    #[must_use]
    pub const fn request_timeout(self) -> Duration {
        self.request_timeout
    }

    /// Returns the additional attempts a bounded read may make.
    #[must_use]
    pub const fn max_retries(self) -> u32 {
        self.max_retries
    }

    /// Returns the wall-clock ceiling on one complete retry sequence.
    #[must_use]
    pub const fn max_retry_elapsed(self) -> Duration {
        self.max_retry_elapsed
    }

    /// Returns the coarse concurrent-request ceiling for the whole node.
    #[must_use]
    pub const fn max_concurrent_requests(self) -> usize {
        self.max_concurrent_requests
    }

    /// Returns the charged-byte ceiling for decoded metadata.
    ///
    /// Zero is the explicit disabled composition: the owner constructs no
    /// resident cache at all and every lookup bypasses retention.
    #[must_use]
    pub const fn metadata_cache_bytes(self) -> u64 {
        self.metadata_cache_bytes
    }
}
