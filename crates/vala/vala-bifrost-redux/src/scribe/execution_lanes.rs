//! Bounded CPU execution lanes used by Scribe admission.

use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use arrow::array::{
    Array, ArrayRef, FixedSizeBinaryBuilder, Int32Builder, StringArray, TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use num_traits::ToPrimitive;
use tokio::sync::{Notify, Semaphore, mpsc, oneshot};

use crate::catalog::TenantTableBinding;
use crate::contracts::{IngressPayload, ScribeError};
use crate::schema::fingerprint::SchemaFingerprint;
use crate::scribe::memory::ScribeMemoryBudget;
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::parquet_writer::{ParquetEncoded, encode_batch};
use crate::scribe::preprocess::{AdmittedAppend, PreparedAppend, prepare_append};
use crate::scribe::replay::ReplayedSealKey;
use crate::scribe::seal_key::SealKey;
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::{
    PreparedWalAppend, WalAppendResult, WalHandle, WalSegment, WalSegmentRef,
};
use std::str::FromStr;
use wyrd_runtime::Principal;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::managed_columns::{
    CARD_REF, CARD_UID, DATA_TENANT_ID, PRINCIPAL_ID, RUN_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME,
    WYRD_INGESTED_AT, WYRD_REQUEST_ID, WYRD_ROW_ORDINAL,
};

#[cfg(test)]
const INGRESS_QUEUE_ITEMS: usize = 256;
pub(crate) const PERSISTENCE_QUEUE_ITEMS: usize = 64;
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
    /// Closes admission to the ingress lane without waiting for active Rayon work.
    pub(crate) fn close(&self) {
        self.permits.close();
    }
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
        record_lane_state("ingress", 0, 0);
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

/// Decode one admitted Arrow payload, validate its projected schema and card
/// scope, then stamp the server-owned physical columns before persistence.
///
/// Native payloads may carry one nullable Utf8 `run_id` correlation field and
/// MAY additionally carry an optional caller-supplied `wyrd_event_time` column.
/// When present, that column MUST be exactly the managed physical type
/// (`Timestamp(Microsecond, UTC)`), MUST NOT be nullable-with-nulls, and MUST
/// NOT be duplicated; it is then preserved verbatim as the authoritative event
/// time rather than server-stamped. When absent, the server stamps
/// `wyrd_event_time` with the ingest receipt time, matching the historical
/// native contract. Projected payloads retain their correlation and event-time
/// fields as supplied by the projection path. In either mode, user schema
/// fingerprinting excludes correlation and managed columns, so accepting a
/// caller event-time column never perturbs a registered-schema fingerprint.
///
/// All other managed columns (`wyrd_ingested_at`, `wyrd_batch_id`,
/// `wyrd_row_ordinal`, `wyrd_request_id`, `data_tenant_id`, principal, and card
/// columns) remain unconditionally server-owned and are rejected when supplied.
///
/// # Errors
/// Returns [`ScribeError::InvalidFrame`] for malformed IPC, a reserved
/// server-owned column supplied by the caller, a `wyrd_event_time` column with
/// the wrong Arrow type, unit, timezone, nullability with nulls, or a duplicate
/// managed field, and for failed physical assembly; returns
/// [`ScribeError::FingerprintMismatch`] or authorization errors when the
/// admitted payload does not match the registered table contract.
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
    let rows = if batches.len() == 1 {
        batches
            .into_iter()
            .next()
            .ok_or(ScribeError::InvalidFrame)?
    } else {
        arrow::compute::concat_batches(&schema, &batches).map_err(|_| ScribeError::InvalidFrame)?
    };
    if rows.num_rows() >= i32::MAX as usize {
        return Err(ScribeError::TooManyRows {
            rows: rows.num_rows() as u64,
            limit: (i32::MAX - 1) as u64,
        });
    }
    for field in rows.schema().fields() {
        // `wyrd_event_time` is intentionally absent from this reserved set:
        // native payloads MAY supply it as the authoritative event time (it is
        // then validated and preserved in `stamp_correlation_columns`), and
        // projected payloads already carry it from the projection path. Every
        // other managed column remains unconditionally server-owned.
        let reserved = matches!(
            field.name().as_str(),
            CARD_UID
                | PRINCIPAL_ID
                | DATA_TENANT_ID
                | WYRD_BATCH_ID
                | WYRD_ROW_ORDINAL
                | WYRD_INGESTED_AT
                | WYRD_REQUEST_ID
        );
        let native_run_id = native_payload && field.name() == "run_id";
        let projected_correlation =
            !native_payload && matches!(field.name().as_str(), CARD_UID | PRINCIPAL_ID | "run_id");
        if reserved && !projected_correlation && !native_run_id {
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

/// Stamps server-owned correlation and managed columns onto one admitted batch.
///
/// Native Arrow payloads may supply one valid nullable `run_id` correlation
/// field; it is removed from the user projection and reinserted exactly once as
/// the canonical nullable physical column. Omitted native values become null.
///
/// Event time is governed by presence, identically for both payload modes: when
/// the admitted batch already carries `wyrd_event_time` the caller value is
/// preserved verbatim (kept in the user projection so it is written exactly
/// once, never duplicated and never re-stamped); when it is absent the server
/// stamps `wyrd_event_time` with the ingest receipt time. A caller-supplied
/// native `wyrd_event_time` is validated up front against the managed physical
/// type before it is trusted. `wyrd_ingested_at` is always the server receipt
/// time regardless of caller event time. Projected payloads retain their
/// correlation values unchanged.
///
/// # Errors
/// Returns [`ScribeError::InvalidFrame`] when a native payload supplies a
/// `wyrd_event_time` column that is not exactly `Timestamp(Microsecond, UTC)`,
/// contains any null, or is duplicated. Returns a typed Scribe error when
/// correlation resolution, timestamp construction, managed-array construction,
/// or final batch validation fails.
fn stamp_correlation_columns(
    rows: &RecordBatch,
    principal: &Principal,
    request_id: &RequestId,
    batch_id: uuid::Uuid,
    native_payload: bool,
) -> Result<RecordBatch, ScribeError> {
    let row_count = rows.num_rows();
    if native_payload {
        validate_native_event_time(rows)?;
    }
    let stamp_event_time = rows.schema().index_of(WYRD_EVENT_TIME).is_err();
    let native_run_id = if native_payload {
        let schema = rows.schema();
        let matches = schema
            .fields()
            .iter()
            .enumerate()
            .filter(|(_, field)| field.name() == "run_id")
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            return Err(ScribeError::InvalidFrame);
        }
        matches
            .first()
            .map(|(index, field)| {
                if field.data_type() != &DataType::Utf8 {
                    return Err(ScribeError::InvalidFrame);
                }
                Ok(Arc::clone(rows.column(*index)))
            })
            .transpose()?
    } else {
        None
    };
    let server_owned = server_owned_columns(native_payload, stamp_event_time);
    let card_uids = resolve_card_uids(rows, principal, row_count)?;
    let mut fields = user_fields(rows, &server_owned);
    let mut columns = user_columns(rows, &server_owned);
    if native_payload {
        fields.push(Field::new(RUN_ID, DataType::Utf8, true));
        columns.push(native_run_id.unwrap_or_else(|| {
            Arc::new(StringArray::from(vec![None::<&str>; row_count])) as ArrayRef
        }));
    }
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

/// Validates a caller-supplied native `wyrd_event_time` column before it is
/// trusted as the authoritative event time.
///
/// Native ingest MAY carry `wyrd_event_time`, but only when it is exactly the
/// managed physical type that [`append_managed_columns`] stamps —
/// `Timestamp(Microsecond)` with the `UTC` timezone — appears exactly once, and
/// contains no nulls. This mirrors how the server-stamped column is
/// constructed so caller and server values are physically interchangeable, and
/// keeps [`split_batch_by_event_day`](crate::scribe::seal_key) able to derive a
/// non-null partition day for every row. A batch with no `wyrd_event_time`
/// column is valid (the server stamps the value) and returns `Ok(())`.
///
/// # Errors
/// Returns [`ScribeError::InvalidFrame`] when the column is duplicated, is not
/// `Timestamp(Microsecond, UTC)`, or contains any null value.
fn validate_native_event_time(rows: &RecordBatch) -> Result<(), ScribeError> {
    let schema = rows.schema();
    let matches = schema
        .fields()
        .iter()
        .enumerate()
        .filter(|(_, field)| field.name() == WYRD_EVENT_TIME)
        .collect::<Vec<_>>();
    let (index, field) = match matches.as_slice() {
        [] => return Ok(()),
        [single] => *single,
        _ => return Err(ScribeError::InvalidFrame),
    };
    let expected = DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into()));
    if field.data_type() != &expected {
        return Err(ScribeError::InvalidFrame);
    }
    if rows.column(index).null_count() != 0 {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(())
}

/// Return physical columns excluded from user projection for one payload mode.
///
/// The two switches are independent. `native_payload` governs only the inbound
/// `run_id` field, which native payloads relinquish so the canonical nullable
/// physical `run_id` can be stamped exactly once. `stamp_event_time` governs
/// only `wyrd_event_time`: it is excluded (and re-stamped by
/// [`append_managed_columns`]) when the server owns the value, and retained in
/// the user projection when the caller supplied a valid event time in either
/// payload mode. Every remaining managed column is unconditionally server-owned
/// and therefore always excluded from the user projection.
fn server_owned_columns(native_payload: bool, stamp_event_time: bool) -> Vec<&'static str> {
    let mut columns = vec![
        CARD_REF,
        CARD_UID,
        PRINCIPAL_ID,
        WYRD_REQUEST_ID,
        WYRD_INGESTED_AT,
        WYRD_BATCH_ID,
        WYRD_ROW_ORDINAL,
        DATA_TENANT_ID,
    ];
    if stamp_event_time {
        columns.push(WYRD_EVENT_TIME);
    }
    if native_payload {
        columns.push("run_id");
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

/// Appends the always-server-owned managed columns to a partially-stamped batch.
///
/// The receipt timestamp, batch id, row ordinal, tenant, card uid, principal,
/// and request id are unconditionally server-owned and always appended here.
/// `wyrd_ingested_at` is always the server receipt time. `stamp_event_time`
/// selects only whether this function also materializes the canonical
/// server-stamped `wyrd_event_time` column (receipt time): it is `true` when the
/// admitted batch did not carry a caller event time, and `false` when a valid
/// caller-supplied `wyrd_event_time` was already retained in the user
/// projection and must not be duplicated or overwritten.
///
/// # Errors
/// Returns [`ScribeError::Internal`] when the system clock precedes the UNIX
/// epoch, the receipt timestamp exceeds Arrow's range, or batch-id stamping
/// fails, and [`ScribeError::TooManyRows`] when a row ordinal exceeds `i32`.
fn append_managed_columns(
    fields: &mut Vec<Field>,
    columns: &mut Vec<ArrayRef>,
    principal: &Principal,
    batch_id: uuid::Uuid,
    row_count: usize,
    stamp_event_time: bool,
) -> Result<(), ScribeError> {
    fields.extend([
        Field::new(CARD_UID, DataType::Utf8, true),
        Field::new(PRINCIPAL_ID, DataType::Utf8, false),
        Field::new(WYRD_REQUEST_ID, DataType::Utf8, false),
    ]);
    if stamp_event_time {
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
        Field::new(WYRD_ROW_ORDINAL, DataType::Int32, false),
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
    if stamp_event_time {
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
    let mut ordinal_builder = Int32Builder::with_capacity(row_count);
    for ordinal in 0..row_count {
        ordinal_builder.append_value(i32::try_from(ordinal).map_err(|_| {
            ScribeError::TooManyRows {
                rows: row_count as u64,
                limit: (i32::MAX - 1) as u64,
            }
        })?);
    }
    columns.push(Arc::new(ordinal_builder.finish()));
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
pub(crate) enum ScribePersistenceCpuOp {
    Preprocess(Box<AdmittedAppend>),
    EncodeParquet {
        frozen: Box<FrozenMemtable>,
        binding: TenantTableBinding,
        tenant: wyrd_spec::ids::DataTenantId,
    },
    RestoreReplay {
        replayed: Box<ReplayedSealKey>,
    },
}

/// Results produced by [`ScribePersistenceCpuPool`].
#[derive(Debug)]
pub(crate) enum ScribePersistenceCpuResult {
    Prepared(PreparedAppend),
    ParquetEncoded(ParquetEncoded),
    ReplayRestored(Box<FrozenMemtable>),
}

/// Bounded persistence CPU lane for day splitting and WAL serialization.
#[derive(Debug, Clone)]
pub struct ScribePersistenceCpuPool {
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

impl ScribePersistenceCpuPool {
    /// Closes admission to the persistence CPU lane without waiting for active Rayon work.
    pub(crate) fn close(&self) {
        self.permits.close();
    }
    /// Build the fixed persistence CPU lane.
    pub(crate) fn new(worker_count: usize) -> Self {
        Self::new_with_capacity(worker_count, PERSISTENCE_QUEUE_ITEMS)
    }

    /// Build the fixed persistence CPU lane with an explicit queue bound.
    ///
    /// # Panics
    ///
    /// Panics if the fixed Rayon pool cannot be constructed.
    pub fn new_with_capacity(worker_count: usize, capacity: usize) -> Self {
        Self::try_new_with_capacity(worker_count, capacity)
            .expect("Scribe persistence Rayon pool must be constructible")
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
            .thread_name(|index| format!("wyrd-scribe-persistence-cpu-{index}"))
            .build()?;
        record_lane_state("persistence", 0, 0);
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
        operation: ScribePersistenceCpuOp,
    ) -> Result<ScribePersistenceCpuResult, ScribeError> {
        let permit = match self.permits.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(tokio::sync::TryAcquireError::NoPermits) => {
                self.saturation_events.fetch_add(1, Ordering::Relaxed);
                self.permits
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("persistence CPU lane closed: {error}"),
                    })?
            }
            Err(tokio::sync::TryAcquireError::Closed) => {
                return Err(ScribeError::Internal {
                    detail: "persistence CPU lane closed".to_owned(),
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
            "persistence",
            depth.load(Ordering::Acquire),
            self.active.load(Ordering::Acquire),
        );
        let active = Arc::clone(&self.active);
        self.pool.spawn_fifo(move || {
            let started = std::time::Instant::now();
            active.fetch_add(1, Ordering::AcqRel);
            record_lane_state(
                "persistence",
                depth.load(Ordering::Acquire),
                active.load(Ordering::Acquire),
            );
            let result = catch_unwind(AssertUnwindSafe(|| match operation {
                ScribePersistenceCpuOp::Preprocess(append) => {
                    if !delay.is_zero() {
                        std::thread::sleep(delay);
                    }
                    prepare_append(*append).map(ScribePersistenceCpuResult::Prepared)
                }
                ScribePersistenceCpuOp::EncodeParquet {
                    frozen,
                    binding,
                    tenant,
                } => encode_batch(&frozen, &binding, tenant)
                    .map(ScribePersistenceCpuResult::ParquetEncoded),
                ScribePersistenceCpuOp::RestoreReplay { replayed } => {
                    let frozen = crate::scribe::memtable::Memtable::decode_replayed(&replayed)?;
                    Ok(ScribePersistenceCpuResult::ReplayRestored(Box::new(frozen)))
                }
            }));
            depth.fetch_sub(1, Ordering::AcqRel);
            active.fetch_sub(1, Ordering::AcqRel);
            record_lane_state(
                "persistence",
                depth.load(Ordering::Acquire),
                active.load(Ordering::Acquire),
            );
            drained.notify_waiters();
            drop(permit);
            let result = result.unwrap_or_else(|_| {
                panics.fetch_add(1, Ordering::Relaxed);
                Err(ScribeError::Internal {
                    detail: "Scribe persistence CPU worker panicked".to_owned(),
                })
            });
            if result.is_ok() {
                completed.fetch_add(1, Ordering::Relaxed);
            } else {
                failed.fetch_add(1, Ordering::Relaxed);
            }
            record_lane_job("persistence", result.is_ok(), started.elapsed());
            let _ = sender.send(result);
        });
        receiver.await.map_err(|_| ScribeError::Internal {
            detail: "Scribe persistence CPU worker dropped its result".to_owned(),
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
        wal: WalHandle,
        segments: Vec<Arc<WalSegment>>,
    },
    #[expect(
        dead_code,
        reason = "task 14 persistence submits manifest replacements through this closed operation"
    )]
    ReplaceManifest { path: PathBuf, contents: Bytes },
    AdvanceManifest {
        path: PathBuf,
        stream: StreamIdentity,
        seal_key: SealKey,
        sealed_lsn: crate::scribe::wal::WalLsn,
    },
    ReplayDirectoryStream {
        path: PathBuf,
        /// Replacement actor stream whose lower same-node epochs are eligible.
        recovery_stream: StreamIdentity,
        shard_senders: Vec<mpsc::Sender<crate::scribe::shards::ShardCommand>>,
        memory: ScribeMemoryBudget,
    },
    RetireWal {
        wal: WalHandle,
        segments: Vec<WalSegmentRef>,
    },
}

/// Results produced by [`ScribeWalIoPool`].
#[derive(Debug)]
pub(crate) enum ScribeWalIoResult {
    WalWritten { result: WalAppendResult },
    WalSynced,
    Completed,
    ReplayStreamCompleted { restored: usize },
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
    /// Closes admission to the WAL IO lane without waiting for active Rayon work.
    pub(crate) fn close(&self) {
        self.permits.close();
    }
    /// Build the fixed WAL IO lane.
    #[cfg(test)]
    pub(crate) fn new(worker_count: usize) -> Self {
        Self::new_with_capacity(worker_count, WAL_IO_QUEUE_ITEMS)
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
        record_lane_state("wal_io", 0, 0);
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
            let result = catch_unwind(AssertUnwindSafe(|| execute_wal_io(operation, delay)));
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

fn execute_wal_io(
    operation: ScribeWalIoOp,
    delay: std::time::Duration,
) -> Result<ScribeWalIoResult, ScribeError> {
    match operation {
        ScribeWalIoOp::WritePrepared { wal, append } => {
            let result = wal.append_prepared(append)?;
            Ok(ScribeWalIoResult::WalWritten { result })
        }
        ScribeWalIoOp::SyncWal { wal, segments } => {
            if !delay.is_zero() {
                std::thread::sleep(delay);
            }
            wal.sync_segments(&segments)?;
            Ok(ScribeWalIoResult::WalSynced)
        }
        ScribeWalIoOp::ReplaceManifest { path, contents } => {
            replace_manifest(&path, &contents)?;
            Ok(ScribeWalIoResult::Completed)
        }
        ScribeWalIoOp::AdvanceManifest {
            path,
            stream,
            seal_key,
            sealed_lsn,
        } => {
            let mut manifest = crate::scribe::manifest::read_manifest(&path)?
                .unwrap_or_else(|| crate::scribe::manifest::Manifest::new(stream));
            manifest.update_sealed_lsn(&seal_key, sealed_lsn);
            let contents = manifest.serialize()?;
            replace_manifest(&path, &contents)?;
            Ok(ScribeWalIoResult::Completed)
        }
        ScribeWalIoOp::ReplayDirectoryStream {
            path,
            recovery_stream,
            shard_senders,
            memory,
        } => {
            let mut restored = 0_usize;
            crate::scribe::replay::replay_wal_directory_stream_accounted(
                path,
                Some(recovery_stream),
                Some(&memory),
                |chunk| {
                    let crate::scribe::replay::ReplayChunk { states, memory } = chunk;
                    drop(memory);
                    for state in states.into_values() {
                        // Use the shard_id recorded in the WAL segment header
                        // rather than recomputing the routing key. Under
                        // batch-spread routing the routing key includes the
                        // client batch_id, which is unavailable here; the
                        // recorded lane is the authoritative dispatch target
                        // that holds the per-shard dedup state.
                        let shard = usize::from(state.shard_id);
                        let (response, receiver) = tokio::sync::oneshot::channel();
                        shard_senders[shard]
                            .blocking_send(crate::scribe::shards::ShardCommand::Replay {
                                state: Box::new(state),
                                response,
                            })
                            .map_err(|_| ScribeError::Internal {
                                detail: "replay owner dropped its command channel".to_owned(),
                            })?;
                        receiver
                            .blocking_recv()
                            .map_err(|_| ScribeError::Internal {
                                detail: "replay owner dropped its completion response".to_owned(),
                            })??;
                        restored = restored.saturating_add(1);
                    }
                    Ok(())
                },
            )?;
            Ok(ScribeWalIoResult::ReplayStreamCompleted { restored })
        }
        ScribeWalIoOp::RetireWal { wal, segments } => {
            wal.retire_segments(&segments)?;
            Ok(ScribeWalIoResult::Completed)
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Arc;

    use arrow::array::{
        ArrayRef, Int32Array, Int64Array, NullArray, StringArray, TimestampMicrosecondArray,
    };
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use uuid::Uuid;
    use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::ids::CardUid;
    use wyrd_spec::reference::{CardRef, CardRefScope};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::managed_columns::{
        CARD_REF, DATA_TENANT_ID, PRINCIPAL_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT,
        WYRD_REQUEST_ID, WYRD_ROW_ORDINAL,
    };

    use super::{
        ScribeIngressCpuPool, ScribePersistenceCpuOp, ScribePersistenceCpuPool, ScribeWalIoPool,
        decode, source_schema_fingerprint, stamp_correlation_columns,
    };
    use crate::contracts::{IngressPayload, ScribeError};
    use crate::schema::SchemaFingerprint;
    use crate::scribe::replay::ReplayedSealKey;
    use crate::scribe::seal_key::SealKey;
    use crate::scribe::stream_identity::StreamIdentity;

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
    fn stamping_preserves_untouched_user_array_identity() {
        let value = Arc::new(Int64Array::from(vec![1_i64, 2_i64])) as ArrayRef;
        let rows = batch(
            vec![Field::new("value", DataType::Int64, false)],
            vec![Arc::clone(&value)],
        );
        let stamped = stamp_correlation_columns(
            &rows,
            &principal(),
            &RequestId::now_v7(),
            Uuid::now_v7(),
            false,
        )
        .expect("stamp");
        let value_index = stamped.schema().index_of("value").expect("value column");
        assert!(Arc::ptr_eq(&value, stamped.column(value_index)));
    }

    /// Native stamping writes the complete canonical physical schema.
    #[test]
    fn native_stamping_materializes_nullable_run_id() {
        let rows = batch(
            vec![Field::new("value", DataType::Int64, false)],
            vec![Arc::new(Int64Array::from(vec![1_i64, 2_i64]))],
        );

        let stamped = stamp_correlation_columns(
            &rows,
            &principal(),
            &RequestId::now_v7(),
            Uuid::now_v7(),
            true,
        )
        .expect("stamp native batch");

        let run_id = stamped.schema().index_of("run_id").expect("run_id field");
        assert!(stamped.schema().field(run_id).is_nullable());
        assert_eq!(stamped.column(run_id).null_count(), stamped.num_rows());
        let expected = Schema::new(crate::schema::with_managed_columns(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        assert_eq!(
            SchemaFingerprint::from_arrow_schema(stamped.schema().as_ref()),
            SchemaFingerprint::from_arrow_schema(&expected),
        );
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
    fn persistence_cpu_runs_on_separate_named_rayon_thread() {
        assert!(
            ScribePersistenceCpuPool::new(1)
                .worker_name()
                .starts_with("wyrd-scribe-persistence-cpu-")
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

    /// Every concrete execution lane publishes its closed idle gauge series at construction.
    #[tokio::test(flavor = "current_thread")]
    async fn scribe_execution_lanes_register_closed_idle_gauges() {
        let recorder = wyrd_bench::BenchmarkRecorder::new();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let ingress = ScribeIngressCpuPool::new(1);
        let persistence = ScribePersistenceCpuPool::new(1);
        let wal_io = ScribeWalIoPool::new(1);

        let snapshot = recorder.snapshot();
        let expected = [
            "bifrost_scribe_lane_queued{lane=\"ingress\"}",
            "bifrost_scribe_lane_active{lane=\"ingress\"}",
            "bifrost_scribe_lane_queued{lane=\"persistence\"}",
            "bifrost_scribe_lane_active{lane=\"persistence\"}",
            "bifrost_scribe_lane_queued{lane=\"wal_io\"}",
            "bifrost_scribe_lane_active{lane=\"wal_io\"}",
        ];
        assert_eq!(snapshot.gauges.len(), expected.len());
        for series in expected {
            assert_eq!(snapshot.gauges.get(series), Some(&0.0), "{series}");
        }

        let worker_ingress = ingress.clone();
        let worker_persistence = persistence.clone();
        let worker_wal_io = wal_io.clone();
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("lane test runtime")
                .block_on(exercise_lane_owners(
                    &worker_ingress,
                    &worker_persistence,
                    &worker_wal_io,
                ));
        })
        .join()
        .expect("lane operation thread");

        for owner in [
            ingress.snapshot(),
            persistence.snapshot(),
            wal_io.snapshot(),
        ] {
            assert_eq!(owner.depth, 0);
            assert_eq!(owner.completed + owner.failed, 1);
        }

        let drained = recorder.snapshot();
        for series in expected {
            assert_eq!(drained.gauges.get(series), Some(&0.0), "{series}");
        }
    }

    /// Executes and drains one bounded operation through every concrete lane owner.
    async fn exercise_lane_owners(
        ingress: &ScribeIngressCpuPool,
        persistence: &ScribePersistenceCpuPool,
        wal_io: &ScribeWalIoPool,
    ) {
        ingress
            .run(|| Ok::<_, ScribeError>(()))
            .await
            .expect("ingress job");
        let replayed = ReplayedSealKey {
            stream: StreamIdentity::new(
                crate::scribe::stream_identity::NodeId::generate(),
                crate::scribe::stream_identity::WriterEpoch::new(1),
            ),
            seal_key: idle_seal_key(),
            shard_id: 0,
            audit_events: Vec::new(),
            data_records: Vec::new(),
            append_metas: Vec::new(),
            wal_segments: Vec::new(),
        };
        let _ = persistence
            .submit(ScribePersistenceCpuOp::RestoreReplay {
                replayed: Box::new(replayed),
            })
            .await;
        let manifest = tempfile::tempdir().expect("manifest directory");
        wal_io
            .submit(
                crate::scribe::execution_lanes::ScribeWalIoOp::AdvanceManifest {
                    path: manifest.path().join("manifest"),
                    stream: StreamIdentity::new(
                        crate::scribe::stream_identity::NodeId::generate(),
                        crate::scribe::stream_identity::WriterEpoch::new(1),
                    ),
                    seal_key: idle_seal_key(),
                    sealed_lsn: crate::scribe::wal::WalLsn::ZERO,
                },
            )
            .await
            .expect("WAL manifest replacement");
        ingress.drain().await;
        persistence.drain().await;
        wal_io.drain().await;
    }

    /// Builds the stable owner-local seal key used by lane transition proof.
    fn idle_seal_key() -> SealKey {
        SealKey::new(
            crate::test_support::tenant(),
            crate::catalog::TableRef::new(crate::namespaces::BifrostNamespace::Bifrost, "idle"),
            crate::scribe::seal_key::EventDay::new(
                chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid test day"),
            ),
        )
    }

    /// Projected payloads retain their caller-provided run identifier as the
    /// correlation value used by the projected-observation path.
    #[test]
    fn projected_run_id_remains_correlation_data() {
        let rows = batch(
            vec![Field::new("run_id", DataType::Utf8, false)],
            vec![Arc::new(StringArray::from(vec!["client-run"]))],
        );
        let error = decode(
            IngressPayload::ProjectedArrow(vec![rows.clone()]),
            &principal(),
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
        )
        .expect("projected run_id is correlation data");
        let run_id = error
            .column_by_name("run_id")
            .expect("projected run_id remains present")
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("run_id is Utf8");
        assert_eq!(run_id.value(0), "client-run");
    }

    /// Native payloads preserve one valid client run identifier while stamping
    /// exactly one authoritative nullable `run_id` column for the physical record.
    #[test]
    fn native_run_id_is_replaced_by_one_authoritative_nullable_field() {
        let card = CardRef::from_str("test/Service/python-integration-writer@1.0.0").expect("card");
        let card = CardRef {
            uid: Some(CardUid::new(Uuid::now_v7().to_string()).expect("card uid")),
            ..card
        };
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
            vec![
                Field::new("value", DataType::Int64, false),
                Field::new("run_id", DataType::Utf8, false),
                Field::new(CARD_REF, DataType::Utf8, false),
            ],
            vec![
                Arc::new(Int64Array::from(vec![1_i64])),
                Arc::new(StringArray::from(vec!["client-run"])),
                Arc::new(StringArray::from(vec![card.to_string()])),
            ],
        );
        let native = decode(
            IngressPayload::ArrowIpc({
                let mut payload = Vec::new();
                let mut writer = StreamWriter::try_new(&mut payload, rows.schema().as_ref())
                    .expect("IPC writer initializes");
                writer.write(&rows).expect("IPC batch writes");
                writer.finish().expect("IPC writer finishes");
                payload.into()
            }),
            &principal,
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
        )
        .expect("native run_id is preserved and canonicalized");
        assert_eq!(
            native
                .schema()
                .fields()
                .iter()
                .filter(|f| f.name() == "run_id")
                .count(),
            1
        );
        let run_id_index = native.schema().index_of("run_id").expect("native run_id");
        assert!(native.schema().field(run_id_index).is_nullable());
        let run_id = native
            .column(run_id_index)
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("native run_id is Utf8");
        assert_eq!(run_id.value(0), "client-run");

        let invalid_type = batch(
            vec![Field::new("run_id", DataType::Int64, false)],
            vec![Arc::new(Int64Array::from(vec![1_i64]))],
        );
        let invalid = decode(
            IngressPayload::ArrowIpc({
                let mut payload = Vec::new();
                let mut writer =
                    StreamWriter::try_new(&mut payload, invalid_type.schema().as_ref())
                        .expect("IPC writer initializes");
                writer.write(&invalid_type).expect("IPC batch writes");
                writer.finish().expect("IPC writer finishes");
                payload.into()
            }),
            &principal,
            source_schema_fingerprint(invalid_type.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
        )
        .expect_err("native run_id type is validated");
        assert!(matches!(invalid, ScribeError::InvalidFrame));

        let duplicate = batch(
            vec![
                Field::new("run_id", DataType::Utf8, false),
                Field::new("run_id", DataType::Utf8, false),
            ],
            vec![
                Arc::new(StringArray::from(vec!["first"])),
                Arc::new(StringArray::from(vec!["second"])),
            ],
        );
        let duplicate_error = decode(
            IngressPayload::ArrowIpc({
                let mut payload = Vec::new();
                let mut writer = StreamWriter::try_new(&mut payload, duplicate.schema().as_ref())
                    .expect("IPC writer initializes");
                writer.write(&duplicate).expect("IPC batch writes");
                writer.finish().expect("IPC writer finishes");
                payload.into()
            }),
            &principal,
            source_schema_fingerprint(duplicate.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
        )
        .expect_err("duplicate native physical fields fail closed");
        assert!(matches!(duplicate_error, ScribeError::InvalidFrame));
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

    /// Assigns one stable zero-based physical ordinal to every row in an admitted batch.
    #[test]
    fn row_ordinal_is_zero_based_within_batch() {
        let rows = batch(
            vec![Field::new("value", DataType::Int64, false)],
            vec![Arc::new(Int64Array::from(vec![1_i64, 2_i64, 3_i64]))],
        );
        let decoded = decode(
            IngressPayload::ProjectedArrow(vec![rows.clone()]),
            &principal(),
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
        )
        .expect("schema is valid");
        let ordinal = decoded
            .column_by_name(WYRD_ROW_ORDINAL)
            .expect("row ordinal is stamped")
            .as_any()
            .downcast_ref::<Int32Array>()
            .expect("row ordinal uses Int32");
        assert_eq!(ordinal.values(), &[0, 1, 2]);
    }

    /// Carries the row identity as the required non-null Arrow `Int32` field.
    #[test]
    fn row_ordinal_uses_int32_physical_schema() {
        let rows = batch(
            vec![Field::new("value", DataType::Int64, false)],
            vec![Arc::new(Int64Array::from(vec![1_i64]))],
        );
        let decoded = decode(
            IngressPayload::ProjectedArrow(vec![rows.clone()]),
            &principal(),
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
        )
        .expect("schema is valid");
        let schema = decoded.schema();
        let field = schema
            .field_with_name(WYRD_ROW_ORDINAL)
            .expect("physical row ordinal exists");
        assert_eq!(field.data_type(), &DataType::Int32);
        assert!(!field.is_nullable());
    }

    /// Rejects an unrepresentable batch in the ingress CPU lane before WAL dispatch.
    #[test]
    fn oversized_batch_rejected_before_wal_append() {
        let rows = batch(
            vec![Field::new("value", DataType::Null, true)],
            vec![Arc::new(NullArray::new(i32::MAX as usize))],
        );
        let error = decode(
            IngressPayload::ProjectedArrow(vec![rows.clone()]),
            &principal(),
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
        )
        .expect_err("unrepresentable ordinal range fails before WAL dispatch");
        assert!(matches!(
            error,
            ScribeError::TooManyRows { rows, limit }
                if rows == i32::MAX as u64 && limit == (i32::MAX - 1) as u64
        ));
    }

    /// Refuses a caller-supplied row ordinal before server preprocessing reaches WAL.
    #[test]
    fn caller_supplied_row_ordinal_is_rejected() {
        let rows = batch(
            vec![Field::new(WYRD_ROW_ORDINAL, DataType::Int32, false)],
            vec![Arc::new(Int32Array::from(vec![0_i32]))],
        );
        let error = decode(
            IngressPayload::ProjectedArrow(vec![rows.clone()]),
            &principal(),
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
        )
        .expect_err("row identity is server-owned");
        assert!(matches!(error, ScribeError::InvalidFrame));
    }

    /// Builds a card-scoped service principal used by native ingest arms.
    fn scoped_service_principal() -> (Principal, CardRef) {
        let card = CardRef::from_str("test/Service/python-integration-writer@1.0.0").expect("card");
        let card = CardRef {
            uid: Some(CardUid::new(Uuid::now_v7().to_string()).expect("card uid")),
            ..card
        };
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
        (principal, card)
    }

    /// Encodes one record batch as a native Arrow IPC stream payload.
    fn ipc_payload(rows: &RecordBatch) -> IngressPayload {
        let mut payload = Vec::new();
        let mut writer = StreamWriter::try_new(&mut payload, rows.schema().as_ref())
            .expect("IPC writer initializes");
        writer.write(rows).expect("IPC batch writes");
        writer.finish().expect("IPC writer finishes");
        IngressPayload::ArrowIpc(payload.into())
    }

    /// Constructs the managed physical `wyrd_event_time` field and matching array.
    fn managed_event_time(values: Vec<i64>) -> (Field, ArrayRef) {
        let field = Field::new(
            WYRD_EVENT_TIME,
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        );
        let array =
            Arc::new(TimestampMicrosecondArray::from(values).with_timezone("UTC")) as ArrayRef;
        (field, array)
    }

    /// A native payload MAY carry a valid caller `wyrd_event_time`: its values
    /// survive decode+stamp unchanged, exactly once, and the user-schema
    /// fingerprint is identical to the same payload without the column.
    #[test]
    fn native_caller_event_time_is_preserved_without_fingerprint_drift() {
        let (principal, card) = scoped_service_principal();
        let (event_field, event_array) = managed_event_time(vec![10_i64, 172_800_000_000_i64]);
        let rows_with = batch(
            vec![
                Field::new("value", DataType::Int64, false),
                Field::new(CARD_REF, DataType::Utf8, false),
                event_field,
            ],
            vec![
                Arc::new(Int64Array::from(vec![1_i64, 2_i64])),
                Arc::new(StringArray::from(vec![card.to_string(), card.to_string()])),
                event_array,
            ],
        );
        let rows_without = batch(
            vec![
                Field::new("value", DataType::Int64, false),
                Field::new(CARD_REF, DataType::Utf8, false),
            ],
            vec![
                Arc::new(Int64Array::from(vec![1_i64, 2_i64])),
                Arc::new(StringArray::from(vec![card.to_string(), card.to_string()])),
            ],
        );
        assert_eq!(
            source_schema_fingerprint(rows_with.schema().as_ref()),
            source_schema_fingerprint(rows_without.schema().as_ref()),
            "caller event time must not perturb the registered-schema fingerprint",
        );

        let decoded = decode(
            ipc_payload(&rows_with),
            &principal,
            source_schema_fingerprint(rows_with.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
        )
        .expect("native caller event time is accepted");
        assert_eq!(
            decoded
                .schema()
                .fields()
                .iter()
                .filter(|field| field.name() == WYRD_EVENT_TIME)
                .count(),
            1,
            "caller event time must not be duplicated",
        );
        let event = decoded
            .column_by_name(WYRD_EVENT_TIME)
            .expect("event time column present")
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .expect("event time is TimestampMicrosecond");
        assert_eq!(event.value(0), 10_i64);
        assert_eq!(event.value(1), 172_800_000_000_i64);
    }

    /// A native payload WITHOUT `wyrd_event_time` is still server-stamped exactly
    /// as before, and `wyrd_ingested_at` remains the server receipt time.
    #[test]
    fn native_without_event_time_is_server_stamped() {
        let (principal, card) = scoped_service_principal();
        let rows = batch(
            vec![
                Field::new("value", DataType::Int64, false),
                Field::new(CARD_REF, DataType::Utf8, false),
            ],
            vec![
                Arc::new(Int64Array::from(vec![1_i64])),
                Arc::new(StringArray::from(vec![card.to_string()])),
            ],
        );
        let decoded = decode(
            ipc_payload(&rows),
            &principal,
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
        )
        .expect("native ingest without event time is server-stamped");
        let event_field = decoded
            .schema()
            .field_with_name(WYRD_EVENT_TIME)
            .expect("server-stamped event time exists")
            .clone();
        assert_eq!(
            event_field.data_type(),
            &DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
        );
        assert!(!event_field.is_nullable());
        assert!(decoded.schema().index_of(WYRD_INGESTED_AT).is_ok());
    }

    /// A native `wyrd_event_time` with the wrong Arrow type, unit, timezone,
    /// nulls, or a duplicate field fails closed with `InvalidFrame`.
    #[test]
    fn native_event_time_physical_type_is_validated() {
        let (principal, card) = scoped_service_principal();
        let card_column = || Arc::new(StringArray::from(vec![card.to_string()])) as ArrayRef;

        let wrong_type = batch(
            vec![
                Field::new(WYRD_EVENT_TIME, DataType::Int64, false),
                Field::new(CARD_REF, DataType::Utf8, false),
            ],
            vec![Arc::new(Int64Array::from(vec![1_i64])), card_column()],
        );
        let wrong_unit = batch(
            vec![
                Field::new(
                    WYRD_EVENT_TIME,
                    DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into())),
                    false,
                ),
                Field::new(CARD_REF, DataType::Utf8, false),
            ],
            vec![
                Arc::new(
                    arrow::array::TimestampNanosecondArray::from(vec![1_i64]).with_timezone("UTC"),
                ),
                card_column(),
            ],
        );
        let missing_timezone = batch(
            vec![
                Field::new(
                    WYRD_EVENT_TIME,
                    DataType::Timestamp(TimeUnit::Microsecond, None),
                    false,
                ),
                Field::new(CARD_REF, DataType::Utf8, false),
            ],
            vec![
                Arc::new(TimestampMicrosecondArray::from(vec![1_i64])),
                card_column(),
            ],
        );
        let null_value = batch(
            vec![
                Field::new(
                    WYRD_EVENT_TIME,
                    DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                    true,
                ),
                Field::new(CARD_REF, DataType::Utf8, false),
            ],
            vec![
                Arc::new(TimestampMicrosecondArray::from(vec![None::<i64>]).with_timezone("UTC")),
                card_column(),
            ],
        );
        let (dup_field_a, dup_array_a) = managed_event_time(vec![1_i64]);
        let (dup_field_b, dup_array_b) = managed_event_time(vec![2_i64]);
        let duplicated = batch(
            vec![
                dup_field_a,
                dup_field_b,
                Field::new(CARD_REF, DataType::Utf8, false),
            ],
            vec![dup_array_a, dup_array_b, card_column()],
        );

        for rows in [
            wrong_type,
            wrong_unit,
            missing_timezone,
            null_value,
            duplicated,
        ] {
            let error = decode(
                ipc_payload(&rows),
                &principal,
                source_schema_fingerprint(rows.schema().as_ref()),
                &RequestId::now_v7(),
                Uuid::now_v7(),
            )
            .expect_err("invalid native event time fails closed");
            assert!(matches!(error, ScribeError::InvalidFrame), "{error:?}");
        }
    }

    /// Managed columns other than `wyrd_event_time` remain reserved and are
    /// rejected when a native payload supplies them.
    #[test]
    fn native_other_reserved_columns_still_rejected() {
        let (principal, card) = scoped_service_principal();
        for reserved in [
            WYRD_INGESTED_AT,
            WYRD_BATCH_ID,
            WYRD_REQUEST_ID,
            DATA_TENANT_ID,
        ] {
            let rows = batch(
                vec![
                    Field::new(reserved, DataType::Utf8, false),
                    Field::new(CARD_REF, DataType::Utf8, false),
                ],
                vec![
                    Arc::new(StringArray::from(vec!["caller"])),
                    Arc::new(StringArray::from(vec![card.to_string()])),
                ],
            );
            let error = decode(
                ipc_payload(&rows),
                &principal,
                source_schema_fingerprint(rows.schema().as_ref()),
                &RequestId::now_v7(),
                Uuid::now_v7(),
            )
            .expect_err("server-owned managed columns are reserved");
            assert!(matches!(error, ScribeError::InvalidFrame), "{reserved}");
        }
    }
}
