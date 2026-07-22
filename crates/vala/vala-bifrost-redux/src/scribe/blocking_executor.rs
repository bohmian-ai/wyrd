//! Fixed-width blocking executor for Scribe preprocessing and WAL work.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use bytes::Bytes;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};

use crate::contracts::ScribeError;
use crate::scribe::preprocess::{AdmittedAppend, PreparedAppend, prepare_append};
use crate::scribe::wal::{WalHandle, WalLsn};

const DEFAULT_QUEUE_CAPACITY: usize = 256;
const DEFAULT_WORKERS: usize = 8;

#[derive(Debug, Clone, Copy, Default)]
struct BlockingDelays {
    preprocess: Duration,
    sync: Duration,
}

/// Operations allowed to run on the bounded blocking executor.
#[derive(Debug)]
pub(crate) enum ScribeBlockingOp {
    Preprocess(Box<AdmittedAppend>),
    WriteFrame {
        wal: WalHandle,
        frame: Bytes,
    },
    SyncWal {
        wal: WalHandle,
    },
    CreateOrRollSegment {
        wal: WalHandle,
    },
    #[expect(
        dead_code,
        reason = "task 14 persistence submits manifest replacements through this closed operation"
    )]
    ReplaceManifest {
        path: PathBuf,
        contents: Bytes,
    },
    RetireWal {
        wal: WalHandle,
    },
}

/// Results returned by blocking operations.
#[derive(Debug)]
pub(crate) enum ScribeBlockingResult {
    Prepared(PreparedAppend),
    WalWritten { wal: WalHandle, lsn: WalLsn },
    WalSynced { wal: WalHandle },
    Completed,
}

struct BlockingRequest {
    operation: ScribeBlockingOp,
    response: oneshot::Sender<Result<ScribeBlockingResult, ScribeError>>,
    permit: OwnedSemaphorePermit,
}

#[derive(Debug)]
struct ExecutorInner {
    sender: Mutex<Option<mpsc::SyncSender<BlockingRequest>>>,
    receiver: Arc<Mutex<mpsc::Receiver<BlockingRequest>>>,
    permits: Arc<Semaphore>,
    depth: AtomicUsize,
    saturation_events: AtomicU64,
}

/// Point-in-time metrics for the bounded blocking executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutorSnapshot {
    /// Operations admitted and not yet completed.
    pub depth: usize,
    /// Maximum number of admitted operations.
    pub capacity: usize,
    /// Number of submissions that waited for a permit.
    pub saturation_events: u64,
}

/// Cloneable handle to the fixed blocking worker pool.
#[derive(Debug, Clone)]
pub(crate) struct ScribeBlockingExecutor {
    inner: Arc<ExecutorInner>,
}

impl ScribeBlockingExecutor {
    /// Start a fixed worker pool with the plan's bounded queue.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::with_workers(DEFAULT_WORKERS)
    }

    /// Start a worker pool with an explicit worker count for focused tests.
    #[must_use]
    pub(crate) fn with_workers(worker_count: usize) -> Self {
        Self::with_workers_internal(worker_count, Duration::ZERO, Duration::ZERO)
    }

    #[cfg(test)]
    pub(crate) fn with_workers_and_delays(
        worker_count: usize,
        preprocess_delay: Duration,
        sync_delay: Duration,
    ) -> Self {
        Self::with_workers_internal(worker_count, preprocess_delay, sync_delay)
    }

    fn with_workers_internal(
        worker_count: usize,
        preprocess_delay: Duration,
        sync_delay: Duration,
    ) -> Self {
        let worker_count = worker_count.max(1);
        let (sender, receiver) = mpsc::sync_channel(DEFAULT_QUEUE_CAPACITY);
        let inner = Arc::new(ExecutorInner {
            sender: Mutex::new(Some(sender)),
            receiver: Arc::new(Mutex::new(receiver)),
            permits: Arc::new(Semaphore::new(DEFAULT_QUEUE_CAPACITY)),
            depth: AtomicUsize::new(0),
            saturation_events: AtomicU64::new(0),
        });

        for worker_index in 0..worker_count {
            let receiver = Arc::clone(&inner.receiver);
            let executor_inner = Arc::clone(&inner);
            let delays = BlockingDelays {
                preprocess: preprocess_delay,
                sync: sync_delay,
            };
            let worker_name = format!("wyrd-scribe-blocking-{worker_index}");
            let _ = thread::Builder::new().name(worker_name).spawn(move || {
                loop {
                    let request = {
                        let receiver = match receiver.lock() {
                            Ok(receiver) => receiver,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                        receiver.recv()
                    };
                    let Ok(request) = request else {
                        break;
                    };
                    executor_inner.depth.fetch_sub(1, Ordering::AcqRel);
                    let result = execute(request.operation, delays);
                    let _ = request.response.send(result);
                    drop(request.permit);
                }
            });
        }

        Self { inner }
    }

    /// Submit one operation without spawning an unbounded blocking task.
    ///
    /// A semaphore permit accounts for every operation before it enters the
    /// synchronous channel. The caller waits asynchronously for capacity and
    /// for the worker's oneshot result, so neither queue admission nor a worker
    /// operation blocks a Tokio core thread.
    pub(crate) async fn submit(
        &self,
        operation: ScribeBlockingOp,
    ) -> Result<ScribeBlockingResult, ScribeError> {
        let permit = match self.inner.permits.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(tokio::sync::TryAcquireError::NoPermits) => {
                self.inner.saturation_events.fetch_add(1, Ordering::Relaxed);
                self.inner
                    .permits
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("blocking executor semaphore closed: {error}"),
                    })?
            }
            Err(tokio::sync::TryAcquireError::Closed) => {
                return Err(ScribeError::Internal {
                    detail: "blocking executor semaphore closed".to_string(),
                });
            }
        };
        let (response, result) = oneshot::channel();
        let request = BlockingRequest {
            operation,
            response,
            permit,
        };

        let sender = self
            .inner
            .sender
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("blocking executor sender lock poisoned: {error}"),
            })?
            .as_ref()
            .cloned()
            .ok_or_else(|| ScribeError::Internal {
                detail: "blocking executor is shut down".to_string(),
            })?;
        self.inner.depth.fetch_add(1, Ordering::AcqRel);
        if let Err(error) = sender.try_send(request) {
            self.inner.depth.fetch_sub(1, Ordering::AcqRel);
            return Err(ScribeError::Internal {
                detail: format!("blocking executor queue rejected operation: {error}"),
            });
        }

        result.await.map_err(|error| ScribeError::Internal {
            detail: format!("blocking executor worker dropped result: {error}"),
        })?
    }

    /// Stop accepting new work. Existing workers finish their current item.
    pub(crate) fn shutdown(&self) {
        if let Ok(mut sender) = self.inner.sender.lock() {
            sender.take();
        }
    }

    /// Return queue depth and saturation counters for inspection/telemetry.
    #[must_use]
    pub(crate) fn snapshot(&self) -> ExecutorSnapshot {
        ExecutorSnapshot {
            depth: self.inner.depth.load(Ordering::Acquire),
            capacity: DEFAULT_QUEUE_CAPACITY,
            saturation_events: self.inner.saturation_events.load(Ordering::Relaxed),
        }
    }
}

/// Execute one blocking operation on a fixed executor worker.
fn execute(
    operation: ScribeBlockingOp,
    delays: BlockingDelays,
) -> Result<ScribeBlockingResult, ScribeError> {
    match operation {
        ScribeBlockingOp::Preprocess(append) => {
            if !delays.preprocess.is_zero() {
                thread::sleep(delays.preprocess);
            }
            prepare_append(*append).map(ScribeBlockingResult::Prepared)
        }
        ScribeBlockingOp::WriteFrame { wal, frame } => {
            let lsn = wal.append_frame(&frame)?;
            Ok(ScribeBlockingResult::WalWritten { wal, lsn })
        }
        ScribeBlockingOp::SyncWal { wal } => {
            if !delays.sync.is_zero() {
                thread::sleep(delays.sync);
            }
            wal.sync_data()?;
            Ok(ScribeBlockingResult::WalSynced { wal })
        }
        ScribeBlockingOp::CreateOrRollSegment { wal } => {
            wal.create_or_roll_segment()?;
            Ok(ScribeBlockingResult::Completed)
        }
        ScribeBlockingOp::ReplaceManifest { path, contents } => {
            replace_manifest(&path, &contents)?;
            Ok(ScribeBlockingResult::Completed)
        }
        ScribeBlockingOp::RetireWal { wal } => {
            wal.sync_data()?;
            Ok(ScribeBlockingResult::Completed)
        }
    }
}

/// Atomically replace a manifest after syncing both the temporary file and its
/// parent directory.
fn replace_manifest(path: &PathBuf, contents: &[u8]) -> Result<(), ScribeError> {
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, contents).map_err(|error| ScribeError::Internal {
        detail: format!("manifest write failed: {error}"),
    })?;
    let file = std::fs::File::open(&temporary).map_err(|error| ScribeError::Internal {
        detail: format!("manifest reopen failed: {error}"),
    })?;
    file.sync_all().map_err(|error| ScribeError::Internal {
        detail: format!("manifest sync failed: {error}"),
    })?;
    std::fs::rename(&temporary, path).map_err(|error| ScribeError::Internal {
        detail: format!("manifest replace failed: {error}"),
    })?;
    if let Some(parent) = path.parent() {
        let parent_file = std::fs::File::open(parent).map_err(|error| ScribeError::Internal {
            detail: format!("manifest parent reopen failed: {error}"),
        })?;
        parent_file
            .sync_all()
            .map_err(|error| ScribeError::Internal {
                detail: format!("manifest parent sync failed: {error}"),
            })?;
    }
    Ok(())
}
