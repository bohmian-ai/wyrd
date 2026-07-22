//! Bounded CPU execution lanes used by Scribe admission.

use std::io::Cursor;
use std::path::PathBuf;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use arrow::array::{Array, ArrayRef, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use tokio::sync::{Semaphore, oneshot};

use crate::contracts::{IngressPayload, ScribeError};
use crate::schema::fingerprint::SchemaFingerprint;
use crate::scribe::preprocess::{AdmittedAppend, PreparedAppend, prepare_append};
use crate::scribe::wal::{WalHandle, WalLsn};
use crate::scribe::replay::ReplayedSealKey;
use std::str::FromStr;
use wyrd_runtime::Principal;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::{CARD_REF, CARD_UID, PRINCIPAL_ID};

const INGRESS_QUEUE_ITEMS: usize = 256;
const POST_ACK_QUEUE_ITEMS: usize = 64;
const WAL_IO_QUEUE_ITEMS: usize = 256;

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
        principal: Principal,
        expected_schema_fingerprint: SchemaFingerprint,
    ) -> Result<RecordBatch, ScribeError> {
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
            let result = catch_unwind(AssertUnwindSafe(|| {
                decode(payload, &principal, expected_schema_fingerprint)
            }));
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

fn decode(
    payload: IngressPayload,
    principal: &Principal,
    expected_schema_fingerprint: SchemaFingerprint,
) -> Result<RecordBatch, ScribeError> {
    let batches = match payload {
        IngressPayload::ArrowIpc(bytes) => {
            let reader = arrow::ipc::reader::StreamReader::try_new(Cursor::new(bytes), None)
                .map_err(|error| ScribeError::Internal {
                    detail: format!("Arrow IPC decode failed: {error}"),
                })?;
            reader
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| ScribeError::Internal {
                    detail: format!("Arrow IPC batch decode failed: {error}"),
                })?
        }
        IngressPayload::ProjectedArrow(batches) => batches,
    };
    let schema = batches.first().map(RecordBatch::schema).ok_or_else(|| {
        ScribeError::Internal {
            detail: "ingress frame contained no record batches".to_owned(),
        }
    })?;
    let rows = arrow::compute::concat_batches(&schema, &batches).map_err(|error| ScribeError::Internal {
        detail: format!("ingress frame concatenation failed: {error}"),
    })?;
    let actual_source_fingerprint = source_schema_fingerprint(rows.schema().as_ref());
    if actual_source_fingerprint != expected_schema_fingerprint {
        return Err(ScribeError::FingerprintMismatch {
            table: "resolved ingress table".to_owned(),
        });
    }
    validate_card_scope(&rows, principal)?;
    stamp_correlation_columns(rows, principal)
}

fn source_schema_fingerprint(schema: &Schema) -> SchemaFingerprint {
    let fields: Vec<Field> = schema
        .fields()
        .iter()
        .filter(|field| {
            !matches!(
                field.name().as_str(),
                CARD_REF | CARD_UID | PRINCIPAL_ID | "run_id" | "data_tenant_id"
            ) && !field.name().starts_with("wyrd_")
        })
        .map(|field| field.as_ref().clone())
        .collect();
    SchemaFingerprint::from_arrow_schema(&Schema::new(fields))
}

fn validate_card_scope(rows: &RecordBatch, principal: &Principal) -> Result<(), ScribeError> {
    let Some(column) = rows.column_by_name(CARD_REF) else {
        return Ok(());
    };
    let Some(scope) = principal.card_ref_scope() else {
        return Err(ScribeError::Internal {
            detail: "principal has no card scope".to_owned(),
        });
    };
    let cards = column
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| ScribeError::Internal {
            detail: "card_ref must be a UTF-8 column".to_owned(),
        })?;
    for index in 0..cards.len() {
        if cards.is_null(index) {
            return Err(ScribeError::Internal {
                detail: "card_ref cannot be null".to_owned(),
            });
        }
        let raw = cards.value(index);
        let card = CardRef::from_str(raw).map_err(|_| ScribeError::Internal {
            detail: format!("card_ref is invalid: {raw}"),
        })?;
        if !scope.authorizes(&card) {
            return Err(ScribeError::Internal {
                detail: format!("card_ref is outside principal scope: {raw}"),
            });
        }
    }
    Ok(())
}

fn stamp_correlation_columns(
    rows: RecordBatch,
    principal: &Principal,
) -> Result<RecordBatch, ScribeError> {
    let row_count = rows.num_rows();
    let card_ref_index = rows.schema().index_of(CARD_REF).ok();
    let bound_card_uid = principal
        .card_ref()
        .and_then(|card| card.uid.as_ref())
        .map(ToString::to_string);
    let card_uids = if let Some(index) = card_ref_index {
        let cards = rows
            .column(index)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| ScribeError::Internal {
                detail: "card_ref must be a UTF-8 column".to_owned(),
            })?;
        let mut values = Vec::with_capacity(row_count);
        for row in 0..row_count {
            if cards.is_null(row) {
                values.push(None);
                continue;
            }
            let raw = cards.value(row);
            let card = CardRef::from_str(raw).map_err(|_| ScribeError::Internal {
                detail: format!("card_ref is invalid: {raw}"),
            })?;
            let Some(bound) = principal.card_ref() else {
                return Err(ScribeError::Internal {
                    detail: "card_ref cannot be resolved without a bound card".to_owned(),
                });
            };
            if !bound.same_identity(&card) {
                return Err(ScribeError::Internal {
                    detail: format!("card_ref cannot be resolved: {raw}"),
                });
            }
            let uid = bound_card_uid.clone().ok_or_else(|| ScribeError::Internal {
                detail: format!("card_ref has no card_uid: {raw}"),
            })?;
            values.push(Some(uid));
        }
        values
    } else {
        vec![None; row_count]
    };
    let mut fields = rows
        .schema()
        .fields()
        .iter()
        .filter(|field| field.name() != CARD_REF)
        .map(|field| field.as_ref().clone())
        .collect::<Vec<Field>>();
    fields.push(Field::new(CARD_UID, DataType::Utf8, true));
    fields.push(Field::new(PRINCIPAL_ID, DataType::Utf8, false));
    let mut columns = rows
        .schema()
        .fields()
        .iter()
        .enumerate()
        .filter(|(_, field)| field.name() != CARD_REF)
        .map(|(index, _)| Arc::clone(rows.column(index)))
        .collect::<Vec<ArrayRef>>();
    columns.push(Arc::new(StringArray::from(card_uids)) as ArrayRef);
    columns.push(Arc::new(StringArray::from(
        vec![principal.id.to_string(); row_count],
    )) as ArrayRef);
    RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).map_err(|error| {
        ScribeError::Internal {
            detail: format!("correlation stamping failed: {error}"),
        }
    })
}

/// Closed set of CPU work permitted after admission has returned.
#[derive(Debug)]
pub(crate) enum ScribePostAckCpuOp {
    Preprocess(Box<AdmittedAppend>),
}

/// Results produced by [`ScribePostAckCpuPool`].
#[derive(Debug)]
pub(crate) enum ScribePostAckCpuResult {
    Prepared(PreparedAppend),
}

/// Bounded post-ACK CPU lane for day splitting and WAL serialization.
#[derive(Debug, Clone)]
pub(crate) struct ScribePostAckCpuPool {
    pool: Arc<rayon::ThreadPool>,
    permits: Arc<Semaphore>,
    depth: Arc<AtomicUsize>,
    panics: Arc<AtomicU64>,
    saturation_events: Arc<AtomicU64>,
    preprocess_delay: std::time::Duration,
}

impl ScribePostAckCpuPool {
    /// Build the fixed post-ACK CPU lane.
    pub(crate) fn new(worker_count: usize) -> Self {
        Self::with_delay(worker_count, std::time::Duration::ZERO)
    }

    pub(crate) fn with_delay(worker_count: usize, preprocess_delay: std::time::Duration) -> Self {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count.max(1))
            .thread_name(|index| format!("wyrd-scribe-post-ack-cpu-{index}"))
            .build()
            .expect("Scribe post-ACK Rayon pool must be constructible during boot");
        Self {
            pool: Arc::new(pool),
            permits: Arc::new(Semaphore::new(POST_ACK_QUEUE_ITEMS)),
            depth: Arc::new(AtomicUsize::new(0)),
            panics: Arc::new(AtomicU64::new(0)),
            saturation_events: Arc::new(AtomicU64::new(0)),
            preprocess_delay,
        }
    }

    pub(crate) async fn submit(
        &self,
        operation: ScribePostAckCpuOp,
    ) -> Result<ScribePostAckCpuResult, ScribeError> {
        let permit = match self.permits.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(tokio::sync::TryAcquireError::NoPermits) => {
                self.saturation_events.fetch_add(1, Ordering::Relaxed);
                self.permits
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("post-ACK CPU lane closed: {error}"),
                    })?
            }
            Err(tokio::sync::TryAcquireError::Closed) => {
                return Err(ScribeError::Internal {
                    detail: "post-ACK CPU lane closed".to_owned(),
                });
            }
        };
        let depth = Arc::clone(&self.depth);
        let panics = Arc::clone(&self.panics);
        let delay = self.preprocess_delay;
        let (sender, receiver) = oneshot::channel();
        depth.fetch_add(1, Ordering::AcqRel);
        self.pool.spawn_fifo(move || {
            let result = catch_unwind(AssertUnwindSafe(|| match operation {
                ScribePostAckCpuOp::Preprocess(append) => {
                    if !delay.is_zero() {
                        std::thread::sleep(delay);
                    }
                    prepare_append(*append).map(ScribePostAckCpuResult::Prepared)
                }
            }));
            depth.fetch_sub(1, Ordering::AcqRel);
            drop(permit);
            let result = result.unwrap_or_else(|_| {
                panics.fetch_add(1, Ordering::Relaxed);
                Err(ScribeError::Internal {
                    detail: "Scribe post-ACK CPU worker panicked".to_owned(),
                })
            });
            let _ = sender.send(result);
        });
        receiver.await.map_err(|_| ScribeError::Internal {
            detail: "Scribe post-ACK CPU worker dropped its result".to_owned(),
        })?
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> crate::scribe::telemetry::ExecutorSnapshot {
        crate::scribe::telemetry::ExecutorSnapshot {
            depth: self.depth.load(Ordering::Acquire),
            capacity: POST_ACK_QUEUE_ITEMS,
            saturation_events: self.saturation_events.load(Ordering::Relaxed),
        }
    }
}

/// Closed set of filesystem work permitted on the WAL IO lane.
#[derive(Debug)]
pub(crate) enum ScribeWalIoOp {
    WriteFrame { wal: WalHandle, frame: Bytes },
    SyncWal { wal: WalHandle },
    CreateOrRollSegment { wal: WalHandle },
    #[expect(
        dead_code,
        reason = "task 14 persistence submits manifest replacements through this closed operation"
    )]
    ReplaceManifest { path: PathBuf, contents: Bytes },
    ReplayDirectory { path: PathBuf },
    RetireWal { wal: WalHandle },
}

/// Results produced by [`ScribeWalIoPool`].
#[derive(Debug)]
pub(crate) enum ScribeWalIoResult {
    WalWritten { wal: WalHandle, lsn: WalLsn },
    WalSynced { wal: WalHandle },
    Completed,
    Replayed(HashMap<String, ReplayedSealKey>),
}

/// Bounded filesystem lane for WAL append, sync, replay support, and retirement.
#[derive(Debug, Clone)]
pub(crate) struct ScribeWalIoPool {
    pool: Arc<rayon::ThreadPool>,
    permits: Arc<Semaphore>,
    depth: Arc<AtomicUsize>,
    panics: Arc<AtomicU64>,
    saturation_events: Arc<AtomicU64>,
    sync_delay: std::time::Duration,
}

impl ScribeWalIoPool {
    /// Build the fixed WAL IO lane.
    pub(crate) fn new(worker_count: usize) -> Self {
        Self::with_delay(worker_count, std::time::Duration::ZERO)
    }

    pub(crate) fn with_delay(worker_count: usize, sync_delay: std::time::Duration) -> Self {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count.max(1))
            .thread_name(|index| format!("wyrd-scribe-wal-io-{index}"))
            .build()
            .expect("Scribe WAL IO Rayon pool must be constructible during boot");
        Self {
            pool: Arc::new(pool),
            permits: Arc::new(Semaphore::new(WAL_IO_QUEUE_ITEMS)),
            depth: Arc::new(AtomicUsize::new(0)),
            panics: Arc::new(AtomicU64::new(0)),
            saturation_events: Arc::new(AtomicU64::new(0)),
            sync_delay,
        }
    }

    pub(crate) async fn submit(
        &self,
        operation: ScribeWalIoOp,
    ) -> Result<ScribeWalIoResult, ScribeError> {
        let permit = match self.permits.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(tokio::sync::TryAcquireError::NoPermits) => {
                self.saturation_events.fetch_add(1, Ordering::Relaxed);
                self.permits
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("WAL IO lane closed: {error}"),
                    })?
            }
            Err(tokio::sync::TryAcquireError::Closed) => {
                return Err(ScribeError::Internal {
                    detail: "WAL IO lane closed".to_owned(),
                });
            }
        };
        let depth = Arc::clone(&self.depth);
        let panics = Arc::clone(&self.panics);
        let delay = self.sync_delay;
        let (sender, receiver) = oneshot::channel();
        depth.fetch_add(1, Ordering::AcqRel);
        self.pool.spawn_fifo(move || {
            let result = catch_unwind(AssertUnwindSafe(|| match operation {
                ScribeWalIoOp::WriteFrame { wal, frame } => {
                    let lsn = wal.append_frame(&frame)?;
                    Ok(ScribeWalIoResult::WalWritten { wal, lsn })
                }
                ScribeWalIoOp::SyncWal { wal } => {
                    if !delay.is_zero() {
                        std::thread::sleep(delay);
                    }
                    wal.sync_data()?;
                    Ok(ScribeWalIoResult::WalSynced { wal })
                }
                ScribeWalIoOp::CreateOrRollSegment { wal } => {
                    wal.create_or_roll_segment()?;
                    Ok(ScribeWalIoResult::Completed)
                }
                ScribeWalIoOp::ReplaceManifest { path, contents } => {
                    replace_manifest(&path, &contents)?;
                    Ok(ScribeWalIoResult::Completed)
                }
                ScribeWalIoOp::ReplayDirectory { path } => crate::scribe::replay::replay_wal_directory(path)
                    .map(ScribeWalIoResult::Replayed),
                ScribeWalIoOp::RetireWal { wal } => {
                    wal.sync_data()?;
                    Ok(ScribeWalIoResult::Completed)
                }
            }));
            depth.fetch_sub(1, Ordering::AcqRel);
            drop(permit);
            let result = result.unwrap_or_else(|_| {
                panics.fetch_add(1, Ordering::Relaxed);
                Err(ScribeError::Internal {
                    detail: "Scribe WAL IO worker panicked".to_owned(),
                })
            });
            let _ = sender.send(result);
        });
        receiver.await.map_err(|_| ScribeError::Internal {
            detail: "Scribe WAL IO worker dropped its result".to_owned(),
        })?
    }

    #[cfg(test)]
    #[expect(dead_code, reason = "WAL lane metrics are exposed by runtime telemetry")]
    pub(crate) fn snapshot(&self) -> crate::scribe::telemetry::ExecutorSnapshot {
        crate::scribe::telemetry::ExecutorSnapshot {
            depth: self.depth.load(Ordering::Acquire),
            capacity: WAL_IO_QUEUE_ITEMS,
            saturation_events: self.saturation_events.load(Ordering::Relaxed),
        }
    }
}

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
