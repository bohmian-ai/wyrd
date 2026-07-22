//! Bounded CPU execution lanes used by Scribe admission.

use std::collections::HashMap;
use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use arrow::array::{
    Array, ArrayRef, FixedSizeBinaryBuilder, StringArray, TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use tokio::sync::{Semaphore, oneshot};

use crate::catalog::TenantTableBinding;
use crate::contracts::{IngressPayload, ScribeError};
use crate::schema::fingerprint::SchemaFingerprint;
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::parquet_writer::{ParquetEncoded, encode_batch};
use crate::scribe::preprocess::{AdmittedAppend, PreparedAppend, prepare_append};
use crate::scribe::replay::ReplayedSealKey;
use crate::scribe::wal::{WalHandle, WalLsn};
use std::str::FromStr;
use wyrd_runtime::Principal;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::system_columns::{
    CARD_REF, CARD_UID, DATA_TENANT_ID, PRINCIPAL_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME,
    WYRD_INGESTED_AT,
};

#[cfg(test)]
const INGRESS_QUEUE_ITEMS: usize = 256;
#[cfg(test)]
const POST_ACK_QUEUE_ITEMS: usize = 64;
#[cfg(test)]
const WAL_IO_QUEUE_ITEMS: usize = 256;

/// The latency-sensitive native decode lane. Its queue is application-bounded;
/// Rayon never becomes the source of untracked backpressure.
#[derive(Debug, Clone)]
pub(crate) struct ScribeIngressCpuPool {
    pool: Arc<rayon::ThreadPool>,
    permits: Arc<Semaphore>,
    depth: Arc<AtomicUsize>,
    panics: Arc<AtomicU64>,
    capacity: usize,
}

impl ScribeIngressCpuPool {
    /// Build the fixed ingress pool with named worker threads.
    #[cfg(test)]
    pub(crate) fn new(worker_count: usize) -> Self {
        Self::new_with_capacity(worker_count, INGRESS_QUEUE_ITEMS)
    }

    /// Build the fixed ingress pool with an explicit application queue bound.
    pub(crate) fn new_with_capacity(worker_count: usize, capacity: usize) -> Self {
        let capacity = capacity.max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count.max(1))
            .thread_name(|index| format!("wyrd-scribe-ingress-cpu-{index}"))
            .build()
            .expect("Scribe ingress Rayon pool must be constructible during boot");
        Self {
            pool: Arc::new(pool),
            permits: Arc::new(Semaphore::new(capacity)),
            depth: Arc::new(AtomicUsize::new(0)),
            panics: Arc::new(AtomicU64::new(0)),
            capacity,
        }
    }

    /// Submit one native decode without waiting for an application queue slot.
    pub(crate) async fn decode(
        &self,
        payload: IngressPayload,
        principal: Principal,
        expected_schema_fingerprint: SchemaFingerprint,
        request_id: RequestId,
        batch_id: uuid::Uuid,
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
                decode(
                    payload,
                    &principal,
                    expected_schema_fingerprint,
                    &request_id,
                    batch_id,
                )
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

    pub(crate) fn snapshot(&self) -> crate::scribe::telemetry::ExecutorSnapshot {
        crate::scribe::telemetry::ExecutorSnapshot {
            depth: self.depth.load(Ordering::Acquire),
            capacity: self.capacity,
            saturation_events: 0,
        }
    }
}

fn decode(
    payload: IngressPayload,
    principal: &Principal,
    expected_schema_fingerprint: SchemaFingerprint,
    request_id: &RequestId,
    batch_id: uuid::Uuid,
) -> Result<RecordBatch, ScribeError> {
    let native_payload = matches!(payload, IngressPayload::ArrowIpc(_));
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
    let schema = batches
        .first()
        .map(RecordBatch::schema)
        .ok_or_else(|| ScribeError::Internal {
            detail: "ingress frame contained no record batches".to_owned(),
        })?;
    let rows = arrow::compute::concat_batches(&schema, &batches).map_err(|error| {
        ScribeError::Internal {
            detail: format!("ingress frame concatenation failed: {error}"),
        }
    })?;
    for field in rows.schema().fields() {
        let reserved = match field.name().as_str() {
            CARD_UID | PRINCIPAL_ID | "run_id" | DATA_TENANT_ID | WYRD_BATCH_ID
            | WYRD_INGESTED_AT => true,
            WYRD_EVENT_TIME => native_payload,
            _ => false,
        };
        if reserved {
            return Err(ScribeError::Internal {
                detail: format!(
                    "reserved system column supplied by client: {}",
                    field.name()
                ),
            });
        }
    }
    let actual_source_fingerprint = source_schema_fingerprint(rows.schema().as_ref());
    if actual_source_fingerprint != expected_schema_fingerprint {
        return Err(ScribeError::FingerprintMismatch {
            table: "resolved ingress table".to_owned(),
        });
    }
    validate_card_scope(&rows, principal)?;
    stamp_correlation_columns(&rows, principal, request_id, batch_id)
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
    rows: &RecordBatch,
    principal: &Principal,
    _request_id: &RequestId,
    batch_id: uuid::Uuid,
) -> Result<RecordBatch, ScribeError> {
    let row_count = rows.num_rows();
    let server_owned = server_owned_columns();
    let card_uids = resolve_card_uids(rows, principal, row_count)?;
    let mut fields = user_fields(rows, &server_owned);
    let mut columns = user_columns(rows, &server_owned);
    columns.push(Arc::new(StringArray::from(card_uids)) as ArrayRef);
    columns.push(Arc::new(StringArray::from(vec![
        principal.id.to_string();
        row_count
    ])));
    append_system_columns(&mut fields, &mut columns, principal, batch_id, row_count)?;
    RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).map_err(|error| {
        ScribeError::Internal {
            detail: format!("correlation stamping failed: {error}"),
        }
    })
}

fn server_owned_columns() -> [&'static str; 7] {
    [
        CARD_REF,
        CARD_UID,
        PRINCIPAL_ID,
        WYRD_EVENT_TIME,
        WYRD_INGESTED_AT,
        WYRD_BATCH_ID,
        DATA_TENANT_ID,
    ]
}

fn user_fields(rows: &RecordBatch, server_owned: &[&str]) -> Vec<Field> {
    rows.schema()
        .fields()
        .iter()
        .filter(|field| !server_owned.contains(&field.name().as_str()))
        .map(|field| field.as_ref().clone())
        .collect()
}

fn user_columns(rows: &RecordBatch, server_owned: &[&str]) -> Vec<ArrayRef> {
    rows.schema()
        .fields()
        .iter()
        .enumerate()
        .filter(|(_, field)| !server_owned.contains(&field.name().as_str()))
        .map(|(index, _)| Arc::clone(rows.column(index)))
        .collect()
}

fn resolve_card_uids(
    rows: &RecordBatch,
    principal: &Principal,
    row_count: usize,
) -> Result<Vec<Option<String>>, ScribeError> {
    let Some(index) = rows.schema().index_of(CARD_REF).ok() else {
        return Ok(vec![None; row_count]);
    };
    let cards = rows
        .column(index)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| ScribeError::Internal {
            detail: "card_ref must be a UTF-8 column".to_owned(),
        })?;
    let bound = principal.card_ref().ok_or_else(|| ScribeError::Internal {
        detail: "card_ref cannot be resolved without a bound card".to_owned(),
    })?;
    let bound_card_uid = bound.uid.as_ref().map(ToString::to_string);
    (0..row_count)
        .map(|row| {
            if cards.is_null(row) {
                return Ok(None);
            }
            let raw = cards.value(row);
            let card = CardRef::from_str(raw).map_err(|_| ScribeError::Internal {
                detail: format!("card_ref is invalid: {raw}"),
            })?;
            if !bound.same_identity(&card) {
                return Err(ScribeError::Internal {
                    detail: format!("card_ref cannot be resolved: {raw}"),
                });
            }
            bound_card_uid
                .clone()
                .map(Some)
                .ok_or_else(|| ScribeError::Internal {
                    detail: format!("card_ref has no card_uid: {raw}"),
                })
        })
        .collect()
}

fn append_system_columns(
    fields: &mut Vec<Field>,
    columns: &mut Vec<ArrayRef>,
    principal: &Principal,
    batch_id: uuid::Uuid,
    row_count: usize,
) -> Result<(), ScribeError> {
    fields.extend([
        Field::new(CARD_UID, DataType::Utf8, true),
        Field::new(PRINCIPAL_ID, DataType::Utf8, false),
        Field::new(
            WYRD_EVENT_TIME,
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new(
            WYRD_INGESTED_AT,
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new(WYRD_BATCH_ID, DataType::FixedSizeBinary(16), false),
        Field::new(DATA_TENANT_ID, DataType::Utf8, false),
    ]);
    let timestamp: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| ScribeError::Internal {
            detail: format!("system clock is before UNIX epoch: {error}"),
        })?
        .as_micros()
        .try_into()
        .map_err(|_| ScribeError::Internal {
            detail: "ingestion timestamp exceeds Arrow range".to_owned(),
        })?;
    let timestamp_array =
        Arc::new(TimestampMicrosecondArray::from(vec![timestamp; row_count]).with_timezone("UTC"))
            as ArrayRef;
    columns.push(Arc::clone(&timestamp_array));
    columns.push(timestamp_array);
    let mut batch_id_builder = FixedSizeBinaryBuilder::with_capacity(row_count, 16);
    for _ in 0..row_count {
        batch_id_builder
            .append_value(batch_id.as_bytes())
            .map_err(|error| ScribeError::Internal {
                detail: format!("batch id stamping failed: {error}"),
            })?;
    }
    columns.push(Arc::new(batch_id_builder.finish()));
    columns.push(Arc::new(StringArray::from(vec![
        principal
            .tenant_id
            .to_string();
        row_count
    ])));
    Ok(())
}

/// Closed set of CPU work permitted after admission has returned.
#[derive(Debug)]
pub(crate) enum ScribePostAckCpuOp {
    Preprocess(Box<AdmittedAppend>),
    EncodeParquet {
        frozen: Box<FrozenMemtable>,
        binding: TenantTableBinding,
        tenant: wyrd_spec::ids::DataTenantId,
    },
}

/// Results produced by [`ScribePostAckCpuPool`].
#[derive(Debug)]
pub(crate) enum ScribePostAckCpuResult {
    Prepared(PreparedAppend),
    ParquetEncoded(ParquetEncoded),
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
    capacity: usize,
}

impl ScribePostAckCpuPool {
    /// Build the fixed post-ACK CPU lane.
    pub(crate) fn new(worker_count: usize) -> Self {
        Self::new_with_capacity(worker_count, 64)
    }

    #[cfg(test)]
    pub(crate) fn with_delay(worker_count: usize, preprocess_delay: std::time::Duration) -> Self {
        Self::with_capacity_and_delay(worker_count, POST_ACK_QUEUE_ITEMS, preprocess_delay)
    }

    /// Build the fixed post-ACK CPU lane with an explicit queue bound.
    pub(crate) fn new_with_capacity(worker_count: usize, capacity: usize) -> Self {
        Self::with_capacity_and_delay(worker_count, capacity, std::time::Duration::ZERO)
    }

    fn with_capacity_and_delay(
        worker_count: usize,
        capacity: usize,
        preprocess_delay: std::time::Duration,
    ) -> Self {
        let capacity = capacity.max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count.max(1))
            .thread_name(|index| format!("wyrd-scribe-post-ack-cpu-{index}"))
            .build()
            .expect("Scribe post-ACK Rayon pool must be constructible during boot");
        Self {
            pool: Arc::new(pool),
            permits: Arc::new(Semaphore::new(capacity)),
            depth: Arc::new(AtomicUsize::new(0)),
            panics: Arc::new(AtomicU64::new(0)),
            saturation_events: Arc::new(AtomicU64::new(0)),
            preprocess_delay,
            capacity,
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
                ScribePostAckCpuOp::EncodeParquet {
                    frozen,
                    binding,
                    tenant,
                } => encode_batch(&frozen, &binding, tenant)
                    .map(ScribePostAckCpuResult::ParquetEncoded),
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

    pub(crate) fn snapshot(&self) -> crate::scribe::telemetry::ExecutorSnapshot {
        crate::scribe::telemetry::ExecutorSnapshot {
            depth: self.depth.load(Ordering::Acquire),
            capacity: self.capacity,
            saturation_events: self.saturation_events.load(Ordering::Relaxed),
        }
    }
}

/// Closed set of filesystem work permitted on the WAL IO lane.
#[derive(Debug)]
pub(crate) enum ScribeWalIoOp {
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
    ReplayDirectory {
        path: PathBuf,
    },
    RetireWal {
        wal: WalHandle,
    },
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
    capacity: usize,
}

impl ScribeWalIoPool {
    /// Build the fixed WAL IO lane.
    #[cfg(test)]
    pub(crate) fn new(worker_count: usize) -> Self {
        Self::new_with_capacity(worker_count, WAL_IO_QUEUE_ITEMS)
    }

    #[cfg(test)]
    pub(crate) fn with_delay(worker_count: usize, sync_delay: std::time::Duration) -> Self {
        Self::with_capacity_and_delay(worker_count, WAL_IO_QUEUE_ITEMS, sync_delay)
    }

    /// Build the fixed WAL IO lane with an explicit queue bound.
    pub(crate) fn new_with_capacity(worker_count: usize, capacity: usize) -> Self {
        Self::with_capacity_and_delay(worker_count, capacity, std::time::Duration::ZERO)
    }

    fn with_capacity_and_delay(
        worker_count: usize,
        capacity: usize,
        sync_delay: std::time::Duration,
    ) -> Self {
        let capacity = capacity.max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count.max(1))
            .thread_name(|index| format!("wyrd-scribe-wal-io-{index}"))
            .build()
            .expect("Scribe WAL IO Rayon pool must be constructible during boot");
        Self {
            pool: Arc::new(pool),
            permits: Arc::new(Semaphore::new(capacity)),
            depth: Arc::new(AtomicUsize::new(0)),
            panics: Arc::new(AtomicU64::new(0)),
            saturation_events: Arc::new(AtomicU64::new(0)),
            sync_delay,
            capacity,
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
                ScribeWalIoOp::ReplayDirectory { path } => {
                    crate::scribe::replay::replay_wal_directory(path)
                        .map(ScribeWalIoResult::Replayed)
                }
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

    pub(crate) fn snapshot(&self) -> crate::scribe::telemetry::ExecutorSnapshot {
        crate::scribe::telemetry::ExecutorSnapshot {
            depth: self.depth.load(Ordering::Acquire),
            capacity: self.capacity,
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
