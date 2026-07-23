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
use num_traits::ToPrimitive;
use tokio::sync::{Notify, Semaphore, oneshot};

use crate::catalog::TenantTableBinding;
use crate::contracts::{IngressPayload, ScribeError};
use crate::schema::fingerprint::SchemaFingerprint;
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::memtable::Memtable;
use crate::scribe::parquet_writer::{ParquetEncoded, encode_batch};
use crate::scribe::preprocess::{AdmittedAppend, PreparedAppend, prepare_append};
use crate::scribe::replay::ReplayedSealKey;
use crate::scribe::wal::{PreparedWalAppend, WalAppendResult, WalHandle, WalSegment};
use std::str::FromStr;
use wyrd_runtime::Principal;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::managed_columns::{
    CARD_REF, CARD_UID, DATA_TENANT_ID, PRINCIPAL_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME,
    WYRD_INGESTED_AT, WYRD_REQUEST_ID,
};

#[cfg(test)]
const INGRESS_QUEUE_ITEMS: usize = 256;
#[cfg(test)]
const POST_ACK_QUEUE_ITEMS: usize = 64;
#[cfg(test)]
const WAL_IO_QUEUE_ITEMS: usize = 256;

fn record_lane_state(lane: &'static str, queued: usize, active: usize) {
    metrics::gauge!("bifrost_scribe_lane_queued", "lane" => lane)
        .set(queued.to_f64().unwrap_or(f64::MAX));
    metrics::gauge!("bifrost_scribe_lane_active", "lane" => lane)
        .set(active.to_f64().unwrap_or(f64::MAX));
}

fn record_lane_job(lane: &'static str, succeeded: bool, elapsed: std::time::Duration) {
    let status = if succeeded { "completed" } else { "failed" };
    metrics::counter!("bifrost_scribe_lane_jobs_total", "lane" => lane, "status" => status)
        .increment(1);
    metrics::histogram!("bifrost_scribe_lane_job_seconds", "lane" => lane)
        .record(elapsed.as_secs_f64());
}

/// The latency-sensitive native decode lane. Its queue is application-bounded;
/// Rayon never becomes the source of untracked backpressure.
#[derive(Debug, Clone)]
pub struct ScribeIngressCpuPool {
    pool: Arc<rayon::ThreadPool>,
    permits: Arc<Semaphore>,
    depth: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    panics: Arc<AtomicU64>,
    completed: Arc<AtomicU64>,
    failed: Arc<AtomicU64>,
    saturation_events: Arc<AtomicU64>,
    drained: Arc<Notify>,
    capacity: usize,
}

impl ScribeIngressCpuPool {
    /// Build the fixed ingress pool with named worker threads.
    #[cfg(test)]
    pub(crate) fn new(worker_count: usize) -> Self {
        Self::new_with_capacity(worker_count, INGRESS_QUEUE_ITEMS)
    }

    /// Build the fixed ingress pool with an explicit application queue bound.
    ///
    /// # Panics
    ///
    /// Panics if the fixed Rayon pool cannot be constructed during boot.
    pub fn new_with_capacity(worker_count: usize, capacity: usize) -> Self {
        Self::try_new_with_capacity(worker_count, capacity)
            .expect("Scribe ingress Rayon pool must be constructible")
    }

    /// Fallible pool builder used by server boot.
    pub fn try_new_with_capacity(
        worker_count: usize,
        capacity: usize,
    ) -> Result<Self, rayon::ThreadPoolBuildError> {
        let capacity = capacity.max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count.max(1))
            .thread_name(|index| format!("wyrd-scribe-ingress-cpu-{index}"))
            .build()?;
        Ok(Self {
            pool: Arc::new(pool),
            permits: Arc::new(Semaphore::new(capacity)),
            depth: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            panics: Arc::new(AtomicU64::new(0)),
            completed: Arc::new(AtomicU64::new(0)),
            failed: Arc::new(AtomicU64::new(0)),
            saturation_events: Arc::new(AtomicU64::new(0)),
            drained: Arc::new(Notify::new()),
            capacity,
        })
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
        let Ok(permit) = self.permits.clone().try_acquire_owned() else {
            self.saturation_events.fetch_add(1, Ordering::Relaxed);
            return Err(ScribeError::IngestBusy {
                table: "ingress".to_owned(),
            });
        };
        self.depth.fetch_add(1, Ordering::AcqRel);
        record_lane_state(
            "ingress",
            self.depth.load(Ordering::Acquire),
            self.active.load(Ordering::Acquire),
        );
        let (sender, receiver) = oneshot::channel();
        let depth = Arc::clone(&self.depth);
        let active = Arc::clone(&self.active);
        let drained = Arc::clone(&self.drained);
        let panics = Arc::clone(&self.panics);
        let completed = Arc::clone(&self.completed);
        let failed = Arc::clone(&self.failed);
        self.pool.spawn_fifo(move || {
            let started = std::time::Instant::now();
            active.fetch_add(1, Ordering::AcqRel);
            record_lane_state(
                "ingress",
                depth.load(Ordering::Acquire),
                active.load(Ordering::Acquire),
            );
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
            active.fetch_sub(1, Ordering::AcqRel);
            record_lane_state(
                "ingress",
                depth.load(Ordering::Acquire),
                active.load(Ordering::Acquire),
            );
            drained.notify_waiters();
            drop(permit);
            let result = if let Ok(result) = result {
                result
            } else {
                panics.fetch_add(1, Ordering::Relaxed);
                Err(ScribeError::Internal {
                    detail: "Scribe ingress CPU worker panicked".to_owned(),
                })
            };
            if result.is_ok() {
                completed.fetch_add(1, Ordering::Relaxed);
            } else {
                failed.fetch_add(1, Ordering::Relaxed);
            }
            record_lane_job("ingress", result.is_ok(), started.elapsed());
            let _ = sender.send(result);
        });
        receiver.await.map_err(|_| ScribeError::Internal {
            detail: "Scribe ingress CPU worker dropped its result".to_owned(),
        })?
    }

    pub(crate) async fn run<T, F>(&self, job: F) -> Result<T, ScribeError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, ScribeError> + Send + 'static,
    {
        let Ok(permit) = self.permits.clone().try_acquire_owned() else {
            self.saturation_events.fetch_add(1, Ordering::Relaxed);
            return Err(ScribeError::IngestBusy {
                table: "ingress".to_owned(),
            });
        };
        self.depth.fetch_add(1, Ordering::AcqRel);
        record_lane_state(
            "ingress",
            self.depth.load(Ordering::Acquire),
            self.active.load(Ordering::Acquire),
        );
        let (sender, receiver) = oneshot::channel();
        let depth = Arc::clone(&self.depth);
        let active = Arc::clone(&self.active);
        let drained = Arc::clone(&self.drained);
        let panics = Arc::clone(&self.panics);
        let completed = Arc::clone(&self.completed);
        let failed = Arc::clone(&self.failed);
        self.pool.spawn_fifo(move || {
            let started = std::time::Instant::now();
            active.fetch_add(1, Ordering::AcqRel);
            record_lane_state(
                "ingress",
                depth.load(Ordering::Acquire),
                active.load(Ordering::Acquire),
            );
            let result = catch_unwind(AssertUnwindSafe(job));
            depth.fetch_sub(1, Ordering::AcqRel);
            active.fetch_sub(1, Ordering::AcqRel);
            record_lane_state(
                "ingress",
                depth.load(Ordering::Acquire),
                active.load(Ordering::Acquire),
            );
            drained.notify_waiters();
            drop(permit);
            let result = if let Ok(result) = result {
                result
            } else {
                panics.fetch_add(1, Ordering::Relaxed);
                Err(ScribeError::Internal {
                    detail: "Scribe ingress CPU worker panicked".to_owned(),
                })
            };
            if result.is_ok() {
                completed.fetch_add(1, Ordering::Relaxed);
            } else {
                failed.fetch_add(1, Ordering::Relaxed);
            }
            record_lane_job("ingress", result.is_ok(), started.elapsed());
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
            saturation_events: self.saturation_events.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            panicked: self.panics.load(Ordering::Relaxed),
        }
    }

    pub(crate) async fn drain(&self) {
        loop {
            let notified = self.drained.notified();
            if self.depth.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }

    #[cfg(test)]
    fn worker_name(&self) -> String {
        self.pool.install(|| {
            std::thread::current()
                .name()
                .unwrap_or("unnamed")
                .to_owned()
        })
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
                .map_err(|_| ScribeError::InvalidFrame)?;
            reader
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| ScribeError::InvalidFrame)?
        }
        IngressPayload::ProjectedArrow(batches) => batches,
    };
    let schema = batches
        .first()
        .map(RecordBatch::schema)
        .ok_or(ScribeError::InvalidFrame)?;
    let rows =
        arrow::compute::concat_batches(&schema, &batches).map_err(|_| ScribeError::InvalidFrame)?;
    for field in rows.schema().fields() {
        let reserved = match field.name().as_str() {
            CARD_UID | PRINCIPAL_ID | "run_id" | DATA_TENANT_ID | WYRD_BATCH_ID
            | WYRD_INGESTED_AT | WYRD_REQUEST_ID => true,
            WYRD_EVENT_TIME => native_payload,
            _ => false,
        };
        let projected_correlation =
            !native_payload && matches!(field.name().as_str(), CARD_UID | PRINCIPAL_ID | "run_id");
        if reserved && !projected_correlation {
            return Err(ScribeError::InvalidFrame);
        }
    }
    let actual_source_fingerprint = source_schema_fingerprint(rows.schema().as_ref());
    if actual_source_fingerprint != expected_schema_fingerprint {
        return Err(ScribeError::FingerprintMismatch {
            table: "resolved ingress table".to_owned(),
        });
    }
    validate_card_scope(&rows, principal)?;
    stamp_correlation_columns(&rows, principal, request_id, batch_id, native_payload)
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
        return Err(ScribeError::CardScopeDenied);
    };
    let cards = column
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or(ScribeError::CardScopeDenied)?;
    for index in 0..cards.len() {
        if cards.is_null(index) {
            return Err(ScribeError::CardScopeDenied);
        }
        let raw = cards.value(index);
        let card = CardRef::from_str(raw).map_err(|_| ScribeError::CardScopeDenied)?;
        if !scope.authorizes(&card) {
            return Err(ScribeError::CardScopeDenied);
        }
    }
    Ok(())
}

fn stamp_correlation_columns(
    rows: &RecordBatch,
    principal: &Principal,
    request_id: &RequestId,
    batch_id: uuid::Uuid,
    native_payload: bool,
) -> Result<RecordBatch, ScribeError> {
    let row_count = rows.num_rows();
    let stamp_event_time = native_payload || rows.schema().index_of(WYRD_EVENT_TIME).is_err();
    let server_owned = server_owned_columns(stamp_event_time);
    let card_uids = resolve_card_uids(rows, principal, row_count)?;
    let mut fields = user_fields(rows, &server_owned);
    let mut columns = user_columns(rows, &server_owned);
    columns.push(Arc::new(StringArray::from(card_uids)) as ArrayRef);
    columns.push(Arc::new(StringArray::from(vec![
        principal.id.to_string();
        row_count
    ])));
    columns.push(Arc::new(StringArray::from(vec![
        request_id.as_str();
        row_count
    ])));
    append_managed_columns(
        &mut fields,
        &mut columns,
        principal,
        batch_id,
        row_count,
        stamp_event_time,
    )?;
    RecordBatch::try_new(Arc::new(Schema::new(fields)), columns)
        .map_err(|_| ScribeError::InvalidFrame)
}

fn server_owned_columns(native_payload: bool) -> Vec<&'static str> {
    let mut columns = vec![
        CARD_REF,
        CARD_UID,
        PRINCIPAL_ID,
        WYRD_REQUEST_ID,
        WYRD_INGESTED_AT,
        WYRD_BATCH_ID,
        DATA_TENANT_ID,
    ];
    if native_payload {
        columns.push(WYRD_EVENT_TIME);
    }
    columns
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
        .ok_or(ScribeError::CardUnresolved)?;
    let bound = principal.card_ref().ok_or(ScribeError::CardUnresolved)?;
    let bound_card_uid = bound.uid.as_ref().map(ToString::to_string);
    (0..row_count)
        .map(|row| {
            if cards.is_null(row) {
                return Ok(None);
            }
            let raw = cards.value(row);
            let card = CardRef::from_str(raw).map_err(|_| ScribeError::CardUnresolved)?;
            if !bound.same_identity(&card) {
                return Err(ScribeError::CardUnresolved);
            }
            bound_card_uid
                .clone()
                .map(Some)
                .ok_or(ScribeError::CardUnresolved)
        })
        .collect()
}

fn append_managed_columns(
    fields: &mut Vec<Field>,
    columns: &mut Vec<ArrayRef>,
    principal: &Principal,
    batch_id: uuid::Uuid,
    row_count: usize,
    native_payload: bool,
) -> Result<(), ScribeError> {
    fields.extend([
        Field::new(CARD_UID, DataType::Utf8, true),
        Field::new(PRINCIPAL_ID, DataType::Utf8, false),
        Field::new(WYRD_REQUEST_ID, DataType::Utf8, false),
    ]);
    if native_payload {
        fields.push(Field::new(
            WYRD_EVENT_TIME,
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ));
    }
    fields.extend([
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
    if native_payload {
        columns.push(Arc::clone(&timestamp_array));
    }
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
    RestoreReplay {
        memtable: Arc<Memtable>,
        replayed: Box<ReplayedSealKey>,
    },
}

/// Results produced by [`ScribePostAckCpuPool`].
#[derive(Debug)]
pub(crate) enum ScribePostAckCpuResult {
    Prepared(PreparedAppend),
    ParquetEncoded(ParquetEncoded),
    ReplayRestored,
}

/// Bounded post-ACK CPU lane for day splitting and WAL serialization.
#[derive(Debug, Clone)]
pub struct ScribePostAckCpuPool {
    pool: Arc<rayon::ThreadPool>,
    permits: Arc<Semaphore>,
    depth: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    panics: Arc<AtomicU64>,
    completed: Arc<AtomicU64>,
    failed: Arc<AtomicU64>,
    saturation_events: Arc<AtomicU64>,
    drained: Arc<Notify>,
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
            .expect("test post-ACK Rayon pool must be constructible")
    }

    /// Build the fixed post-ACK CPU lane with an explicit queue bound.
    ///
    /// # Panics
    ///
    /// Panics if the fixed Rayon pool cannot be constructed.
    pub fn new_with_capacity(worker_count: usize, capacity: usize) -> Self {
        Self::try_new_with_capacity(worker_count, capacity)
            .expect("Scribe post-ACK Rayon pool must be constructible")
    }

    /// Fallible pool builder used by server boot.
    pub fn try_new_with_capacity(
        worker_count: usize,
        capacity: usize,
    ) -> Result<Self, rayon::ThreadPoolBuildError> {
        Self::with_capacity_and_delay(worker_count, capacity, std::time::Duration::ZERO)
    }

    fn with_capacity_and_delay(
        worker_count: usize,
        capacity: usize,
        preprocess_delay: std::time::Duration,
    ) -> Result<Self, rayon::ThreadPoolBuildError> {
        let capacity = capacity.max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count.max(1))
            .thread_name(|index| format!("wyrd-scribe-post-ack-cpu-{index}"))
            .build()?;
        Ok(Self {
            pool: Arc::new(pool),
            permits: Arc::new(Semaphore::new(capacity)),
            depth: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            panics: Arc::new(AtomicU64::new(0)),
            completed: Arc::new(AtomicU64::new(0)),
            failed: Arc::new(AtomicU64::new(0)),
            saturation_events: Arc::new(AtomicU64::new(0)),
            drained: Arc::new(Notify::new()),
            preprocess_delay,
            capacity,
        })
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
        let drained = Arc::clone(&self.drained);
        let panics = Arc::clone(&self.panics);
        let completed = Arc::clone(&self.completed);
        let failed = Arc::clone(&self.failed);
        let delay = self.preprocess_delay;
        let (sender, receiver) = oneshot::channel();
        depth.fetch_add(1, Ordering::AcqRel);
        record_lane_state(
            "post_ack",
            depth.load(Ordering::Acquire),
            self.active.load(Ordering::Acquire),
        );
        let active = Arc::clone(&self.active);
        self.pool.spawn_fifo(move || {
            let started = std::time::Instant::now();
            active.fetch_add(1, Ordering::AcqRel);
            record_lane_state(
                "post_ack",
                depth.load(Ordering::Acquire),
                active.load(Ordering::Acquire),
            );
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
                ScribePostAckCpuOp::RestoreReplay { memtable, replayed } => memtable
                    .restore_replayed(&replayed)
                    .map(|_| ScribePostAckCpuResult::ReplayRestored),
            }));
            depth.fetch_sub(1, Ordering::AcqRel);
            active.fetch_sub(1, Ordering::AcqRel);
            record_lane_state(
                "post_ack",
                depth.load(Ordering::Acquire),
                active.load(Ordering::Acquire),
            );
            drained.notify_waiters();
            drop(permit);
            let result = result.unwrap_or_else(|_| {
                panics.fetch_add(1, Ordering::Relaxed);
                Err(ScribeError::Internal {
                    detail: "Scribe post-ACK CPU worker panicked".to_owned(),
                })
            });
            if result.is_ok() {
                completed.fetch_add(1, Ordering::Relaxed);
            } else {
                failed.fetch_add(1, Ordering::Relaxed);
            }
            record_lane_job("post_ack", result.is_ok(), started.elapsed());
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
            completed: self.completed.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            panicked: self.panics.load(Ordering::Relaxed),
        }
    }

    pub(crate) async fn drain(&self) {
        loop {
            let notified = self.drained.notified();
            if self.depth.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }

    #[cfg(test)]
    fn worker_name(&self) -> String {
        self.pool.install(|| {
            std::thread::current()
                .name()
                .unwrap_or("unnamed")
                .to_owned()
        })
    }
}

/// Closed set of filesystem work permitted on the WAL IO lane.
#[derive(Debug)]
pub(crate) enum ScribeWalIoOp {
    WritePrepared {
        wal: WalHandle,
        append: PreparedWalAppend,
    },
    SyncWal {
        segments: Vec<Arc<WalSegment>>,
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
    WalWritten {
        wal: WalHandle,
        result: WalAppendResult,
    },
    WalSynced,
    Completed,
    Replayed(HashMap<String, ReplayedSealKey>),
}

/// Bounded filesystem lane for WAL append, sync, replay support, and retirement.
#[derive(Debug, Clone)]
pub struct ScribeWalIoPool {
    pool: Arc<rayon::ThreadPool>,
    permits: Arc<Semaphore>,
    depth: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    panics: Arc<AtomicU64>,
    completed: Arc<AtomicU64>,
    failed: Arc<AtomicU64>,
    saturation_events: Arc<AtomicU64>,
    drained: Arc<Notify>,
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
        Self::try_new_with_capacity_and_delay(worker_count, WAL_IO_QUEUE_ITEMS, sync_delay)
            .expect("test WAL IO Rayon pool must be constructible")
    }

    /// Build the fixed WAL IO lane with an explicit queue bound.
    ///
    /// # Panics
    ///
    /// Panics if the fixed Rayon pool cannot be constructed.
    pub fn new_with_capacity(worker_count: usize, capacity: usize) -> Self {
        Self::try_new_with_capacity(worker_count, capacity)
            .expect("Scribe WAL IO Rayon pool must be constructible")
    }

    /// Fallible pool builder used by server boot.
    pub fn try_new_with_capacity(
        worker_count: usize,
        capacity: usize,
    ) -> Result<Self, rayon::ThreadPoolBuildError> {
        Self::try_new_with_capacity_and_delay(worker_count, capacity, std::time::Duration::ZERO)
    }

    /// Build the WAL lane with a deterministic sync delay for benchmark and
    /// failure-injection harnesses. Production boot passes zero delay.
    pub fn try_new_with_capacity_and_delay(
        worker_count: usize,
        capacity: usize,
        sync_delay: std::time::Duration,
    ) -> Result<Self, rayon::ThreadPoolBuildError> {
        let capacity = capacity.max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count.max(1))
            .thread_name(|index| format!("wyrd-scribe-wal-io-{index}"))
            .build()?;
        Ok(Self {
            pool: Arc::new(pool),
            permits: Arc::new(Semaphore::new(capacity)),
            depth: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            panics: Arc::new(AtomicU64::new(0)),
            completed: Arc::new(AtomicU64::new(0)),
            failed: Arc::new(AtomicU64::new(0)),
            saturation_events: Arc::new(AtomicU64::new(0)),
            drained: Arc::new(Notify::new()),
            sync_delay,
            capacity,
        })
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
        let drained = Arc::clone(&self.drained);
        let panics = Arc::clone(&self.panics);
        let completed = Arc::clone(&self.completed);
        let failed = Arc::clone(&self.failed);
        let delay = self.sync_delay;
        let (sender, receiver) = oneshot::channel();
        depth.fetch_add(1, Ordering::AcqRel);
        record_lane_state(
            "wal_io",
            depth.load(Ordering::Acquire),
            self.active.load(Ordering::Acquire),
        );
        let active = Arc::clone(&self.active);
        self.pool.spawn_fifo(move || {
            let started = std::time::Instant::now();
            active.fetch_add(1, Ordering::AcqRel);
            record_lane_state(
                "wal_io",
                depth.load(Ordering::Acquire),
                active.load(Ordering::Acquire),
            );
            let result = catch_unwind(AssertUnwindSafe(|| match operation {
                ScribeWalIoOp::WritePrepared { wal, append } => {
                    let result = wal.append_prepared(append)?;
                    Ok(ScribeWalIoResult::WalWritten { wal, result })
                }
                ScribeWalIoOp::SyncWal { segments } => {
                    if !delay.is_zero() {
                        std::thread::sleep(delay);
                    }
                    WalHandle::sync_segments(&segments)?;
                    Ok(ScribeWalIoResult::WalSynced)
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
            active.fetch_sub(1, Ordering::AcqRel);
            record_lane_state(
                "wal_io",
                depth.load(Ordering::Acquire),
                active.load(Ordering::Acquire),
            );
            drained.notify_waiters();
            drop(permit);
            let result = result.unwrap_or_else(|_| {
                panics.fetch_add(1, Ordering::Relaxed);
                Err(ScribeError::Internal {
                    detail: "Scribe WAL IO worker panicked".to_owned(),
                })
            });
            if result.is_ok() {
                completed.fetch_add(1, Ordering::Relaxed);
            } else {
                failed.fetch_add(1, Ordering::Relaxed);
            }
            record_lane_job("wal_io", result.is_ok(), started.elapsed());
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
            completed: self.completed.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            panicked: self.panics.load(Ordering::Relaxed),
        }
    }

    pub(crate) async fn drain(&self) {
        loop {
            let notified = self.drained.notified();
            if self.depth.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }

    #[cfg(test)]
    fn worker_name(&self) -> String {
        self.pool.install(|| {
            std::thread::current()
                .name()
                .unwrap_or("unnamed")
                .to_owned()
        })
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Arc;

    use arrow::array::{ArrayRef, Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use uuid::Uuid;
    use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::reference::{CardRef, CardRefScope};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::managed_columns::{
        CARD_REF, DATA_TENANT_ID, PRINCIPAL_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT,
        WYRD_REQUEST_ID,
    };

    use super::{
        ScribeIngressCpuPool, ScribePostAckCpuPool, ScribeWalIoPool, decode,
        source_schema_fingerprint,
    };
    use crate::contracts::IngressPayload;

    fn principal() -> Principal {
        Principal::new(
            PrincipalId::new(Uuid::now_v7()),
            PrincipalKind::User,
            crate::test_support::tenant(),
            Vec::new(),
            PermissionSet::new(),
        )
    }

    fn batch(fields: Vec<Field>, columns: Vec<ArrayRef>) -> RecordBatch {
        RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).expect("test batch")
    }

    #[test]
    fn ingress_cpu_runs_on_named_rayon_thread() {
        assert!(
            ScribeIngressCpuPool::new(1)
                .worker_name()
                .starts_with("wyrd-scribe-ingress-cpu-")
        );
    }

    #[tokio::test]
    async fn ingress_cpu_runs_bounded_projection_jobs_on_its_lane() {
        let pool = ScribeIngressCpuPool::new_with_capacity(1, 1);
        let worker_name = pool
            .run(|| {
                Ok(std::thread::current()
                    .name()
                    .unwrap_or("unnamed")
                    .to_owned())
            })
            .await
            .expect("projection job completes");
        assert!(worker_name.starts_with("wyrd-scribe-ingress-cpu-"));
        let snapshot = pool.snapshot();
        assert_eq!(snapshot.depth, 0);
        assert_eq!(snapshot.completed, 1);
        assert_eq!(snapshot.failed, 0);
        assert_eq!(snapshot.panicked, 0);
    }

    #[tokio::test]
    async fn rayon_lane_panic_releases_permit_and_returns_typed_error() {
        let pool = ScribeIngressCpuPool::new_with_capacity(1, 1);
        let error = pool
            .run(|| -> Result<(), crate::contracts::ScribeError> {
                panic!("intentional lane panic");
            })
            .await
            .expect_err("panic must become a typed error");
        assert!(matches!(
            error,
            crate::contracts::ScribeError::Internal { .. }
        ));
        let result = pool
            .run(|| Ok::<_, crate::contracts::ScribeError>(()))
            .await;
        assert!(result.is_ok(), "panic must release the queue permit");
        let snapshot = pool.snapshot();
        assert_eq!(snapshot.depth, 0);
        assert_eq!(snapshot.panicked, 1);
    }

    #[test]
    fn post_ack_cpu_runs_on_separate_named_rayon_thread() {
        assert!(
            ScribePostAckCpuPool::new(1)
                .worker_name()
                .starts_with("wyrd-scribe-post-ack-cpu-")
        );
    }

    #[test]
    fn wal_io_runs_on_separate_named_rayon_thread() {
        assert!(
            ScribeWalIoPool::new(1)
                .worker_name()
                .starts_with("wyrd-scribe-wal-io-")
        );
    }

    #[test]
    fn reserved_columns_reject_before_writer_admission() {
        let rows = batch(
            vec![Field::new("run_id", DataType::Utf8, false)],
            vec![Arc::new(StringArray::from(vec!["client-run"]))],
        );
        let mut payload = Vec::new();
        {
            let mut writer = StreamWriter::try_new(&mut payload, &rows.schema())
                .expect("IPC writer initializes");
            writer.write(&rows).expect("IPC batch writes");
            writer.finish().expect("IPC writer finishes");
        }
        let error = decode(
            IngressPayload::ArrowIpc(payload.into()),
            &principal(),
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
        )
        .expect_err("run_id is server-owned");
        assert!(matches!(error, crate::contracts::ScribeError::InvalidFrame));
    }

    #[test]
    fn card_scope_rejects_before_writer_admission() {
        let card = CardRef::from_str("prod/Service/billing@1.0.0").expect("card");
        let principal = Principal::new(
            PrincipalId::new(Uuid::now_v7()),
            PrincipalKind::Service {
                card_ref: card.clone(),
                card_ref_scope: CardRefScope::own(&card),
            },
            crate::test_support::tenant(),
            Vec::new(),
            PermissionSet::new(),
        );
        let rows = batch(
            vec![Field::new(CARD_REF, DataType::Utf8, false)],
            vec![Arc::new(StringArray::from(vec![
                "prod/Service/other@1.0.0",
            ]))],
        );
        let error = decode(
            IngressPayload::ProjectedArrow(vec![rows.clone()]),
            &principal,
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
        )
        .expect_err("card outside scope");
        assert!(matches!(
            error,
            crate::contracts::ScribeError::CardScopeDenied
        ));
    }

    #[test]
    fn schema_fields_map_by_name_not_position() {
        let rows = batch(
            vec![
                Field::new("second", DataType::Int64, false),
                Field::new("first", DataType::Int64, false),
            ],
            vec![
                Arc::new(Int64Array::from(vec![2_i64])),
                Arc::new(Int64Array::from(vec![1_i64])),
            ],
        );
        let decoded = decode(
            IngressPayload::ProjectedArrow(vec![rows.clone()]),
            &principal(),
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
        )
        .expect("schema is valid");
        assert_eq!(decoded.schema().field(0).name(), "second");
        assert_eq!(decoded.schema().field(1).name(), "first");
        assert_eq!(
            decoded
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .value(0),
            2
        );
        assert_eq!(
            decoded
                .column(1)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .value(0),
            1
        );
    }

    #[test]
    fn ipc_decode_schema_type_validation_and_stamping_precede_ack() {
        let rows = batch(
            vec![Field::new("value", DataType::Int64, false)],
            vec![Arc::new(Int64Array::from(vec![1_i64]))],
        );
        let batch_id = Uuid::now_v7();
        let decoded = decode(
            IngressPayload::ProjectedArrow(vec![rows.clone()]),
            &principal(),
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            batch_id,
        )
        .expect("schema is valid");
        assert!(decoded.schema().index_of(DATA_TENANT_ID).is_ok());
        assert!(decoded.schema().index_of(PRINCIPAL_ID).is_ok());
        assert!(decoded.schema().index_of(WYRD_BATCH_ID).is_ok());
        assert!(decoded.schema().index_of(WYRD_EVENT_TIME).is_ok());
        assert!(decoded.schema().index_of(WYRD_INGESTED_AT).is_ok());
        assert!(decoded.schema().index_of(WYRD_REQUEST_ID).is_ok());
    }
}
