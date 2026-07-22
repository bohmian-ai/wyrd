//! Bounded CPU execution lanes used by Scribe admission.

use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use arrow::record_batch::RecordBatch;
use tokio::sync::{Semaphore, oneshot};

use crate::contracts::{IngressPayload, ScribeError};

const INGRESS_QUEUE_ITEMS: usize = 256;

/// The latency-sensitive native decode lane. Its queue is application-bounded;
/// Rayon never becomes the source of untracked backpressure.
#[derive(Debug, Clone)]
pub(crate) struct ScribeIngressCpuPool {
    pool: Arc<rayon::ThreadPool>,
    permits: Arc<Semaphore>,
    depth: Arc<AtomicUsize>,
    panics: Arc<AtomicU64>,
}

impl ScribeIngressCpuPool {
    /// Build the fixed ingress pool with named worker threads.
    pub(crate) fn new(worker_count: usize) -> Self {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count.max(1))
            .thread_name(|index| format!("wyrd-scribe-ingress-cpu-{index}"))
            .build()
            .expect("Scribe ingress Rayon pool must be constructible during boot");
        Self {
            pool: Arc::new(pool),
            permits: Arc::new(Semaphore::new(INGRESS_QUEUE_ITEMS)),
            depth: Arc::new(AtomicUsize::new(0)),
            panics: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Submit one native decode without waiting for an application queue slot.
    pub(crate) async fn decode(
        &self,
        payload: IngressPayload,
    ) -> Result<Vec<RecordBatch>, ScribeError> {
        let permit =
            self.permits
                .clone()
                .try_acquire_owned()
                .map_err(|_| ScribeError::IngestBusy {
                    table: "ingress".to_owned(),
                })?;
        self.depth.fetch_add(1, Ordering::AcqRel);
        let (sender, receiver) = oneshot::channel();
        let depth = Arc::clone(&self.depth);
        let panics = Arc::clone(&self.panics);
        self.pool.spawn_fifo(move || {
            let result = catch_unwind(AssertUnwindSafe(|| decode(payload)));
            depth.fetch_sub(1, Ordering::AcqRel);
            drop(permit);
            let result = if let Ok(result) = result {
                result
            } else {
                panics.fetch_add(1, Ordering::Relaxed);
                Err(ScribeError::Internal {
                    detail: "Scribe ingress CPU worker panicked".to_owned(),
                })
            };
            let _ = sender.send(result);
        });
        receiver.await.map_err(|_| ScribeError::Internal {
            detail: "Scribe ingress CPU worker dropped its result".to_owned(),
        })?
    }
}

fn decode(payload: IngressPayload) -> Result<Vec<RecordBatch>, ScribeError> {
    match payload {
        IngressPayload::ArrowIpc(bytes) => {
            let reader = arrow::ipc::reader::StreamReader::try_new(Cursor::new(bytes), None)
                .map_err(|error| ScribeError::Internal {
                    detail: format!("Arrow IPC decode failed: {error}"),
                })?;
            reader
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| ScribeError::Internal {
                    detail: format!("Arrow IPC batch decode failed: {error}"),
                })
        }
        IngressPayload::ProjectedArrow(batches) => Ok(batches),
    }
}
