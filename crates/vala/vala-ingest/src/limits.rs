//! Aggregate stream bounds and the per-tenant concurrency registry.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use wyrd_spec::ids::DataTenantId;

use crate::error::IngestError;

/// Hard aggregate bounds enforced as frames arrive, before any commit.
#[derive(Clone, Debug)]
pub struct IngestLimits {
    /// Maximum size of one decompressed Arrow IPC frame.
    pub max_frame_bytes: usize,
    /// Sum of frame bytes across the stream.
    pub max_stream_bytes: u64,
    /// Sum of decoded rows across the stream.
    pub max_stream_rows: u64,
    /// Maximum number of frames in a stream.
    pub max_stream_frames: u64,
    /// Maximum gap between frames.
    pub idle_deadline: Duration,
    /// Maximum time from stream open to half-close.
    pub total_deadline: Duration,
    /// Maximum concurrent streams per `DataTenantId`.
    pub max_concurrent_streams_per_tenant: usize,
    /// tonic `max_decoding_message_size` (default 4 MiB silently drops large
    /// frames; the server raises it and enforces its own cap instead).
    pub max_decoding_message_size: usize,
}

impl Default for IngestLimits {
    fn default() -> Self {
        Self {
            max_frame_bytes: 32 * 1024 * 1024,
            max_stream_bytes: 256 * 1024 * 1024,
            max_stream_rows: 50_000_000,
            max_stream_frames: 100_000,
            idle_deadline: Duration::from_secs(30),
            total_deadline: Duration::from_secs(600),
            max_concurrent_streams_per_tenant: 16,
            max_decoding_message_size: 32 * 1024 * 1024 + 64 * 1024,
        }
    }
}

/// Per-tenant stream-concurrency registry backed by a [`Semaphore`] per
/// `DataTenantId`. A permit is held for the lifetime of a stream and released
/// (via `Drop`) on completion or abort.
#[derive(Debug)]
pub struct StreamSemaphores {
    inner: Mutex<HashMap<DataTenantId, Arc<Semaphore>>>,
    per_tenant: usize,
}

impl StreamSemaphores {
    /// Build a registry granting `per_tenant` concurrent streams to each tenant.
    #[must_use]
    pub fn new(per_tenant: usize) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            per_tenant,
        }
    }

    fn semaphore_for(&self, tenant: DataTenantId) -> Arc<Semaphore> {
        let mut guard = self
            .inner
            .lock()
            .expect("stream semaphore registry poisoned");
        Arc::clone(
            guard
                .entry(tenant)
                .or_insert_with(|| Arc::new(Semaphore::new(self.per_tenant))),
        )
    }

    /// Acquire a stream slot for `tenant`, or reject when the tenant is at its
    /// concurrency limit.
    ///
    /// # Errors
    /// Returns [`IngestError::TooManyStreams`] when no permit is available.
    pub fn acquire(&self, tenant: DataTenantId) -> Result<OwnedSemaphorePermit, IngestError> {
        self.semaphore_for(tenant)
            .try_acquire_owned()
            .map_err(|_| IngestError::TooManyStreams)
    }
}
