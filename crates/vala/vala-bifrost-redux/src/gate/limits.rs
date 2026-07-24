//! Unary batch bounds and the per-tenant concurrency registry.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use wyrd_spec::ids::DataTenantId;

use super::error::IngestError;

/// Hard bounds enforced before a batch enters Scribe.
#[derive(Clone, Debug)]
pub struct IngestLimits {
    /// Maximum size of one decompressed Arrow IPC batch.
    pub max_frame_bytes: usize,
    /// Maximum concurrent batches per `DataTenantId`.
    pub max_concurrent_batches_per_tenant: usize,
    /// tonic `max_decoding_message_size` (default 4 MiB silently drops large
    /// frames; the server raises it and enforces its own cap instead).
    pub max_decoding_message_size: usize,
}

impl Default for IngestLimits {
    fn default() -> Self {
        Self {
            max_frame_bytes: 32 * 1024 * 1024,
            max_concurrent_batches_per_tenant: 16,
            max_decoding_message_size: 32 * 1024 * 1024 + 64 * 1024,
        }
    }
}

/// Per-tenant batch-concurrency registry backed by a [`Semaphore`] per
/// `DataTenantId`. A permit is held for the lifetime of a stream and released
/// (via `Drop`) on completion or abort.
#[derive(Debug)]
pub struct BatchSemaphores {
    inner: Mutex<HashMap<DataTenantId, Arc<Semaphore>>>,
    per_tenant: usize,
}

impl BatchSemaphores {
    /// Build a registry granting `per_tenant` concurrent batches to each tenant.
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

    /// Acquire a batch slot for `tenant`, or reject when the tenant is at its
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
