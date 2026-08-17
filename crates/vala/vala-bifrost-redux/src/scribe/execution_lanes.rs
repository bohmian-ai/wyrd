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
use crate::resources::ScribeResources;
use crate::schema::fingerprint::SchemaFingerprint;
use crate::scribe::admission::EventTimeWindow;
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::parquet_writer::{ParquetEncoded, encode_batch};
use crate::scribe::preprocess::{
    AdmittedAppend, NativeSliceProducer, OtlpSliceProducer, PreparedAppend, PreparedSlice,
    prepare_append,
};
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

/// Count one lane-saturation rejection, labelled by the lane that shed it (D84).
///
/// A saturated CPU lane rejects a job with `IngestBusy` when no application
/// queue slot is free. The internal `saturation_events` counter already tracks
/// this for the lane's own health snapshot, but the rejection was invisible in
/// production telemetry; this makes it a first-class
/// `bifrost_scribe_lane_saturation_total{lane}` counter so a lane shedding load
/// is distinguishable from a memory-ceiling rejection. The `lane` label reuses
/// the closed lane vocabulary shared with [`record_lane_state`] and
/// [`record_lane_job`] and carries no tenant, table, or request identity.
fn record_lane_saturation(lane: &'static str) {
    metrics::counter!("bifrost_scribe_lane_saturation_total", "lane" => lane).increment(1);
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
    ///
    /// The `window` argument carries the pod-wide event-time acceptance bounds
    /// sourced from [`AdmissionConfig`] by the caller. It is passed by value
    /// into the Rayon closure so no heap allocation is required.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the ingress queue is saturated,
    /// and propagates any error produced by the free [`decode`] function,
    /// including [`ScribeError::EventTimeOutOfRange`] when a present
    /// `wyrd_event_time` value falls outside the acceptance window.
    pub(crate) async fn decode(
        &self,
        payload: IngressPayload,
        principal: Principal,
        expected_schema_fingerprint: SchemaFingerprint,
        request_id: RequestId,
        batch_id: uuid::Uuid,
        window: EventTimeWindow,
    ) -> Result<RecordBatch, ScribeError> {
        let Ok(permit) = self.permits.clone().try_acquire_owned() else {
            self.saturation_events.fetch_add(1, Ordering::Relaxed);
            record_lane_saturation("ingress");
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
                    window,
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

    #[cfg(test)]
    pub(crate) async fn run<T, F>(&self, job: F) -> Result<T, ScribeError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, ScribeError> + Send + 'static,
    {
        let Ok(permit) = self.permits.clone().try_acquire_owned() else {
            self.saturation_events.fetch_add(1, Ordering::Relaxed);
            record_lane_saturation("ingress");
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
/// NOT be duplicated; it is then validated against the acceptance `window` and,
/// if in range, preserved verbatim as the authoritative event time. When absent,
/// the server stamps `wyrd_event_time` with the ingest receipt time. Projected
/// payloads retain their event-time field as supplied; a present projected
/// `wyrd_event_time` is also validated against the same window.
///
/// In either mode, user schema fingerprinting excludes correlation and managed
/// columns, so accepting a caller event-time column never perturbs a
/// registered-schema fingerprint.
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
/// [`ScribeError::EventTimeOutOfRange`] when a present `wyrd_event_time` value
/// falls outside the acceptance `window`; returns [`ScribeError::FingerprintMismatch`]
/// or authorization errors when the admitted payload does not match the registered
/// table contract.
fn decode(
    payload: IngressPayload,
    principal: &Principal,
    expected_schema_fingerprint: SchemaFingerprint,
    request_id: &RequestId,
    batch_id: uuid::Uuid,
    window: EventTimeWindow,
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
        IngressPayload::OtlpTraces(_)
        | IngressPayload::OtlpMetrics(_)
        | IngressPayload::OtlpLogs(_) => return Err(ScribeError::InvalidFrame),
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
    decode_rows(
        &rows,
        &DecodeContext {
            principal,
            expected_schema_fingerprint,
            request_id,
            batch_id,
            native_payload,
            window,
            receipt_micros: None,
        },
    )
}

/// Validates and stamps one current native record batch.
///
/// This entry point lets persistence preprocessing consume a native stream one
/// source at a time without collecting or concatenating its record batches.
///
/// # Errors
///
/// Returns the same schema, scope, event-time, row, and managed-column errors
/// as the ordinary ingress decode path.
pub(crate) fn decode_native_batch(
    rows: &RecordBatch,
    principal: &Principal,
    expected_schema_fingerprint: SchemaFingerprint,
    request_id: &RequestId,
    batch_id: uuid::Uuid,
    window: EventTimeWindow,
    receipt_micros: i64,
) -> Result<RecordBatch, ScribeError> {
    decode_rows(
        rows,
        &DecodeContext {
            principal,
            expected_schema_fingerprint,
            request_id,
            batch_id,
            native_payload: true,
            window,
            receipt_micros: Some(receipt_micros),
        },
    )
}

/// Immutable validation and stamping context for one decoded batch.
struct DecodeContext<'a> {
    /// Authenticated principal used for scope checks and managed columns.
    principal: &'a Principal,
    /// Catalog fingerprint required of the caller-owned source schema.
    expected_schema_fingerprint: SchemaFingerprint,
    /// Stable request identity stamped into every accepted row.
    request_id: &'a RequestId,
    /// Stable batch identity stamped into every accepted row.
    batch_id: uuid::Uuid,
    /// Whether the caller supplied native Arrow rather than projected rows.
    native_payload: bool,
    /// Accepted caller event-time window.
    window: EventTimeWindow,
    /// Fixed receipt time retained by current-only native production.
    receipt_micros: Option<i64>,
}

/// Applies source-contract validation and server-managed stamping to one batch.
///
/// # Errors
///
/// Returns a stable Scribe refusal for row overflow, reserved columns,
/// fingerprint mismatch, card-scope failure, invalid event time, or managed
/// column construction failure.
fn decode_rows(
    rows: &RecordBatch,
    context: &DecodeContext<'_>,
) -> Result<RecordBatch, ScribeError> {
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
        let native_run_id = context.native_payload && field.name() == "run_id";
        let projected_correlation = !context.native_payload
            && matches!(field.name().as_str(), CARD_UID | PRINCIPAL_ID | "run_id");
        if reserved && !projected_correlation && !native_run_id {
            return Err(ScribeError::InvalidFrame);
        }
    }
    let actual_source_fingerprint = source_schema_fingerprint(rows.schema().as_ref());
    if actual_source_fingerprint != context.expected_schema_fingerprint {
        return Err(ScribeError::FingerprintMismatch {
            table: "resolved ingress table".to_owned(),
        });
    }
    validate_card_scope(rows, context.principal)?;
    stamp_correlation_columns(
        rows,
        context.principal,
        context.request_id,
        context.batch_id,
        context.native_payload,
        context.window,
        context.receipt_micros,
    )
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

/// Captures one deterministic receipt timestamp for planning and regeneration.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the system clock precedes the UNIX
/// epoch or the microsecond count exceeds Arrow's signed timestamp range.
pub(crate) fn current_receipt_micros() -> Result<i64, ScribeError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| ScribeError::Internal {
            detail: format!("system clock is before UNIX epoch: {error}"),
        })?
        .as_micros()
        .try_into()
        .map_err(|_| ScribeError::Internal {
            detail: "receipt timestamp exceeds Arrow range".to_owned(),
        })
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
/// first validated against the managed physical type (native path only), then
/// checked against the bounded acceptance `window`, and — if accepted — the
/// column is removed from the user projection and its array is reinserted
/// verbatim (never re-stamped) in the canonical managed slot by
/// [`append_managed_columns`]; when the column is absent the server stamps
/// `wyrd_event_time` with the ingest receipt time in that same slot. Either way
/// `wyrd_event_time` is written exactly once and lands between `wyrd_request_id`
/// and `wyrd_ingested_at`, so the stamped batch's full field order equals
/// [`with_managed_columns`](crate::schema::with_managed_columns) in every payload
/// mode (D88). `wyrd_ingested_at` is always the server receipt time regardless of
/// caller event time. Projected payloads retain their remaining correlation
/// values unchanged.
///
/// The `window` check uses a single `receipt_micros` computed once at the
/// entry of this function so an in-flight clock tick cannot split a batch
/// verdict.
///
/// # Errors
/// Returns [`ScribeError::InvalidFrame`] when a native payload supplies a
/// `wyrd_event_time` column that is not exactly `Timestamp(Microsecond, UTC)`,
/// contains any null, or is duplicated. Returns [`ScribeError::EventTimeOutOfRange`]
/// when a present `wyrd_event_time` value (either mode) falls outside the
/// acceptance `window`. Returns a typed Scribe error when correlation resolution,
/// timestamp construction, managed-array construction, or final batch validation
/// fails.
fn stamp_correlation_columns(
    rows: &RecordBatch,
    principal: &Principal,
    request_id: &RequestId,
    batch_id: uuid::Uuid,
    native_payload: bool,
    window: EventTimeWindow,
    receipt_micros: Option<i64>,
) -> Result<RecordBatch, ScribeError> {
    let row_count = rows.num_rows();
    // Compute one receipt instant for the whole batch so clock ticks mid-batch
    // cannot split the verdict.
    let receipt_micros = receipt_micros.map_or_else(current_receipt_micros, Ok)?;
    // Native type/null/duplicate checks come first (T38). A malformed column
    // stays `InvalidFrame` regardless of window membership.
    if native_payload {
        validate_native_event_time(rows)?;
    }
    // Enforce the acceptance window for a present column in either payload mode.
    if rows.schema().index_of(WYRD_EVENT_TIME).is_ok() {
        enforce_event_time_window(rows, window, receipt_micros)?;
    }
    // A present `wyrd_event_time` (validated above for native payloads and
    // window-checked above for both modes) is always lifted out of the user
    // projection here and reinserted verbatim in the canonical managed slot by
    // `append_managed_columns`, never re-stamped. When absent, the server stamps
    // the receipt time into that slot instead. Decoupling "exclude from the user
    // block" from "stamp the receipt value" keeps the stamped field order equal
    // to `with_managed_columns` in every payload mode (D88).
    let caller_event_time = rows
        .schema()
        .index_of(WYRD_EVENT_TIME)
        .ok()
        .map(|index| Arc::clone(rows.column(index)));
    let native_run_id = if native_payload {
        let schema = rows.schema();
        let mut matched = None;
        for (index, field) in schema.fields().iter().enumerate() {
            if field.name() != "run_id" {
                continue;
            }
            if matched.replace((index, field)).is_some() {
                return Err(ScribeError::InvalidFrame);
            }
        }
        matched
            .map(|(index, field)| {
                if field.data_type() != &DataType::Utf8 {
                    return Err(ScribeError::InvalidFrame);
                }
                Ok(Arc::clone(rows.column(index)))
            })
            .transpose()?
    } else {
        None
    };
    let server_owned = server_owned_columns(native_payload);
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
        caller_event_time,
        receipt_micros,
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
/// keeps the day-partition splitter able to derive a non-null partition day for
/// every row. A batch with no `wyrd_event_time` column is valid (the server
/// stamps the value) and returns `Ok(())`.
///
/// Value-range checking is a separate concern handled by
/// [`enforce_event_time_window`], which runs after this function for the native
/// path and independently for the projected path.
///
/// # Errors
/// Returns [`ScribeError::InvalidFrame`] when the column is duplicated, is not
/// `Timestamp(Microsecond, UTC)`, or contains any null value.
fn validate_native_event_time(rows: &RecordBatch) -> Result<(), ScribeError> {
    let schema = rows.schema();
    let mut matched = None;
    for (index, field) in schema.fields().iter().enumerate() {
        if field.name() != WYRD_EVENT_TIME {
            continue;
        }
        if matched.replace((index, field)).is_some() {
            return Err(ScribeError::InvalidFrame);
        }
    }
    let Some((index, field)) = matched else {
        return Ok(());
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

/// Enforces the bounded acceptance `window` for a present `wyrd_event_time`
/// column against a single per-batch `receipt_micros` instant.
///
/// This helper is the single enforcement point for both the native Arrow IPC
/// and projected OTLP surfaces. It must be called only when the schema already
/// contains `WYRD_EVENT_TIME` (the caller checks presence before calling).
/// For the native path, [`validate_native_event_time`] must run first so only
/// well-typed, non-null values reach this function.
///
/// On the first out-of-range element the function returns immediately with
/// [`ScribeError::EventTimeOutOfRange`] carrying that element's value and both
/// bound instants. The entire batch is therefore rejected pre-admission and no
/// rows are written.
///
/// # Errors
/// Returns [`ScribeError::EventTimeOutOfRange`] when any non-null element of
/// the `wyrd_event_time` column falls outside the inclusive window
/// `[receipt_micros − past, receipt_micros + future]`. Returns
/// [`ScribeError::Internal`] when the column cannot be downcast to
/// `TimestampMicrosecondArray` (which would imply a caller error on the
/// projected path that bypassed type validation).
fn enforce_event_time_window(
    rows: &RecordBatch,
    window: EventTimeWindow,
    receipt_micros: i64,
) -> Result<(), ScribeError> {
    let index = rows
        .schema()
        .index_of(WYRD_EVENT_TIME)
        .map_err(|_| ScribeError::Internal {
            detail: "enforce_event_time_window called without wyrd_event_time column".to_owned(),
        })?;
    let column = rows.column(index);
    let array = column
        .as_any()
        .downcast_ref::<TimestampMicrosecondArray>()
        .ok_or_else(|| ScribeError::Internal {
            detail: "wyrd_event_time column is not TimestampMicrosecondArray".to_owned(),
        })?;
    // Compute bounds once for the whole batch.
    let past_micros = i64::try_from(window.past.as_micros()).unwrap_or(i64::MAX);
    let future_micros = i64::try_from(window.future.as_micros()).unwrap_or(i64::MAX);
    let lo = receipt_micros.saturating_sub(past_micros);
    let hi = receipt_micros.saturating_add(future_micros);
    for idx in 0..array.len() {
        if array.is_null(idx) {
            // The window check is value-level and only range-checks non-null
            // values, so it tolerates nulls here. The native path already
            // rejects nulls upstream in validate_native_event_time. A present
            // projected event-time column is later lifted verbatim into the
            // non-nullable canonical managed slot (D88), so a null value there
            // fails batch construction rather than being range-checked.
            continue;
        }
        let value = array.value(idx);
        if value < lo || value > hi {
            return Err(ScribeError::EventTimeOutOfRange {
                value_micros: value,
                past_bound_micros: lo,
                future_bound_micros: hi,
            });
        }
    }
    Ok(())
}

/// Return physical columns excluded from user projection for one payload mode.
///
/// Every managed column is unconditionally excluded from the user projection and
/// materialized in its canonical slot by [`append_managed_columns`], regardless
/// of whether the caller supplied it. In particular `wyrd_event_time` is always
/// excluded: when the caller supplied a valid value it is reinserted verbatim in
/// the canonical slot, and when it is absent the server stamps the receipt time
/// there. Decoupling exclusion from the value source keeps the stamped field
/// order equal to [`with_managed_columns`](crate::schema::with_managed_columns)
/// in every payload mode (D88). `native_payload` additionally governs the inbound
/// `run_id` field, which native payloads relinquish so the canonical nullable
/// physical `run_id` can be stamped exactly once; projected payloads keep their
/// `run_id` as correlation data in the user projection.
fn server_owned_columns(native_payload: bool) -> Vec<&'static str> {
    let mut columns = vec![
        CARD_REF,
        CARD_UID,
        PRINCIPAL_ID,
        WYRD_REQUEST_ID,
        WYRD_EVENT_TIME,
        WYRD_INGESTED_AT,
        WYRD_BATCH_ID,
        WYRD_ROW_ORDINAL,
        DATA_TENANT_ID,
    ];
    if native_payload {
        columns.push("run_id");
    }
    columns
}

fn user_fields(rows: &RecordBatch, server_owned: &[&str]) -> Vec<Field> {
    let schema = rows.schema();
    let capacity = schema
        .fields()
        .iter()
        .filter(|field| !server_owned.contains(&field.name().as_str()))
        .count();
    let mut fields = Vec::with_capacity(capacity);
    fields.extend(
        schema
            .fields()
            .iter()
            .filter(|field| !server_owned.contains(&field.name().as_str()))
            .map(|field| field.as_ref().clone()),
    );
    debug_assert_eq!(fields.capacity(), capacity);
    fields
}

fn user_columns(rows: &RecordBatch, server_owned: &[&str]) -> Vec<ArrayRef> {
    let schema = rows.schema();
    let capacity = schema
        .fields()
        .iter()
        .filter(|field| !server_owned.contains(&field.name().as_str()))
        .count();
    let mut columns = Vec::with_capacity(capacity);
    columns.extend(
        schema
            .fields()
            .iter()
            .enumerate()
            .filter(|(_, field)| !server_owned.contains(&field.name().as_str()))
            .map(|(index, _)| Arc::clone(rows.column(index))),
    );
    debug_assert_eq!(columns.capacity(), capacity);
    columns
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
    let mut resolved = Vec::with_capacity(row_count);
    for row in 0..row_count {
        resolved.push({
            if cards.is_null(row) {
                None
            } else {
                let raw = cards.value(row);
                let card = CardRef::from_str(raw).map_err(|_| ScribeError::CardUnresolved)?;
                if !bound.same_identity(&card) {
                    return Err(ScribeError::CardUnresolved);
                }
                bound_card_uid
                    .clone()
                    .map(Some)
                    .ok_or(ScribeError::CardUnresolved)?
            }
        });
    }
    if resolved.capacity() != row_count {
        return Err(ScribeError::Internal {
            detail: "card UID projection exceeded exact row capacity".to_owned(),
        });
    }
    Ok(resolved)
}

/// Appends the always-server-owned managed columns to a partially-stamped batch.
///
/// The receipt timestamp, batch id, row ordinal, tenant, card uid, principal,
/// and request id are unconditionally server-owned and always appended here.
/// `wyrd_ingested_at` is always the server receipt time. `wyrd_event_time` is
/// always materialized in the canonical slot (between `wyrd_request_id` and
/// `wyrd_ingested_at`), so the stamped field order equals
/// [`with_managed_columns`](crate::schema::with_managed_columns) in every payload
/// mode (D88). Only the event-time *value* varies: `caller_event_time` carries a
/// valid caller-supplied array (already validated and window-checked, lifted out
/// of the user projection) that is written verbatim exactly once and never
/// re-stamped; when it is `None` the server stamps the receipt time into the same
/// slot.
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
    caller_event_time: Option<ArrayRef>,
    receipt_micros: i64,
) -> Result<(), ScribeError> {
    fields.extend([
        Field::new(CARD_UID, DataType::Utf8, true),
        Field::new(PRINCIPAL_ID, DataType::Utf8, false),
        Field::new(WYRD_REQUEST_ID, DataType::Utf8, false),
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
        Field::new(WYRD_ROW_ORDINAL, DataType::Int32, false),
        Field::new(DATA_TENANT_ID, DataType::Utf8, false),
    ]);
    let timestamp_array = Arc::new(
        TimestampMicrosecondArray::from(vec![receipt_micros; row_count]).with_timezone("UTC"),
    ) as ArrayRef;
    // Event time lands in the canonical slot exactly once: the caller's array
    // verbatim when supplied, otherwise the server receipt instant.
    columns.push(caller_event_time.unwrap_or_else(|| Arc::clone(&timestamp_array)));
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
    /// Advance one root-owned native producer by at most one slice.
    ProduceNativeSlice {
        /// Current-only producer state moved into the detached Rayon job.
        producer: Box<NativeSliceProducer>,
        /// Root-backed owner that must outlive every allocation in the job.
        memory: crate::resources::ScribeMemoryLease,
    },
    /// Advance the root-owned one-shot OTLP producer.
    ProduceOtlpSlice {
        /// Current-only producer state moved into the detached Rayon job.
        producer: Box<OtlpSliceProducer>,
        /// Root-backed owner that must outlive every allocation in the job.
        memory: crate::resources::ScribeMemoryLease,
    },
    EncodeParquet(Box<EncodeParquetOp>),
    RestoreReplay {
        replayed: Box<ReplayedSealKey>,
    },
    /// Test-only stalled work proving a detached job retains its root owner.
    #[cfg(test)]
    HoldMemory {
        /// Root-backed bytes that must remain charged through job completion.
        memory: crate::resources::ScribeMemoryLease,
        /// Deterministic signal emitted after the detached job owns the lease.
        started: std::sync::mpsc::SyncSender<()>,
        /// Deterministic release gate controlled by the cancellation test.
        release: std::sync::mpsc::Receiver<()>,
    },
}

/// Move-only inputs for one bounded Parquet encoding lane operation.
#[derive(Debug)]
pub(crate) struct EncodeParquetOp {
    /// Frozen generation encoded without retaining the shard actor.
    pub(crate) frozen: Box<FrozenMemtable>,
    /// Tenant-qualified physical table binding.
    pub(crate) binding: TenantTableBinding,
    /// Authenticated tenant checked again by the encoder.
    pub(crate) tenant: wyrd_spec::ids::DataTenantId,
    /// Generation-owned output scratch directory.
    pub(crate) scratch_dir: std::path::PathBuf,
    /// Deterministic artifact basename.
    pub(crate) object_base: String,
    /// Exact pre-writer footer child retained through sealed inspection.
    pub(crate) footer_reservation: crate::scribe::memory::EncodedFooterReservation,
}

/// Results produced by [`ScribePersistenceCpuPool`].
#[derive(Debug)]
pub(crate) enum ScribePersistenceCpuResult {
    Prepared(PreparedAppend),
    /// Native producer state returned with its optional current slice.
    NativeSliceProduced {
        /// Producer to retain for the next bounded turn.
        producer: Box<NativeSliceProducer>,
        /// Current slice, or `None` after exact exhaustion.
        slice: Option<PreparedSlice>,
        /// Root-backed owner returned only after the detached job completes.
        memory: crate::resources::ScribeMemoryLease,
    },
    /// OTLP producer state returned with its optional current slice.
    OtlpSliceProduced {
        /// Producer retained for post-COMMIT deterministic regeneration.
        producer: Box<OtlpSliceProducer>,
        /// Sole current slice, or `None` for an empty/exhausted request.
        slice: Option<PreparedSlice>,
        /// Root-backed owner returned only after the detached job completes.
        memory: crate::resources::ScribeMemoryLease,
    },
    ParquetEncoded(ParquetEncoded),
    ReplayRestored(Box<FrozenMemtable>),
}

/// Executes one persistence operation after it has moved onto a Rayon worker.
///
/// Keeping dispatch inside the detached worker preserves ownership of root
/// memory for native and OTLP production even when the async submitter is
/// cancelled.
///
/// # Errors
///
/// Returns the stable preprocessing, projection, encoding, replay, or
/// test-synchronization error produced by the selected operation.
fn execute_persistence_operation(
    operation: ScribePersistenceCpuOp,
    preprocess_delay: std::time::Duration,
) -> Result<ScribePersistenceCpuResult, ScribeError> {
    match operation {
        ScribePersistenceCpuOp::Preprocess(append) => {
            if !preprocess_delay.is_zero() {
                std::thread::sleep(preprocess_delay);
            }
            prepare_append(*append).map(ScribePersistenceCpuResult::Prepared)
        }
        ScribePersistenceCpuOp::ProduceNativeSlice {
            mut producer,
            memory,
        } => {
            let slice = producer.next_slice()?;
            Ok(ScribePersistenceCpuResult::NativeSliceProduced {
                producer,
                slice,
                memory,
            })
        }
        ScribePersistenceCpuOp::ProduceOtlpSlice {
            mut producer,
            memory,
        } => {
            let slice = producer.next_slice()?;
            Ok(ScribePersistenceCpuResult::OtlpSliceProduced {
                producer,
                slice,
                memory,
            })
        }
        ScribePersistenceCpuOp::EncodeParquet(operation) => {
            let EncodeParquetOp {
                frozen,
                binding,
                tenant,
                scratch_dir,
                object_base,
                footer_reservation,
            } = *operation;
            encode_batch(
                &frozen,
                &binding,
                tenant,
                &scratch_dir,
                &object_base,
                footer_reservation,
            )
            .map(ScribePersistenceCpuResult::ParquetEncoded)
        }
        ScribePersistenceCpuOp::RestoreReplay { replayed } => {
            let frozen = crate::scribe::memtable::Memtable::decode_replayed(&replayed)?;
            Ok(ScribePersistenceCpuResult::ReplayRestored(Box::new(frozen)))
        }
        #[cfg(test)]
        ScribePersistenceCpuOp::HoldMemory {
            memory,
            started,
            release,
        } => {
            started.send(()).map_err(|error| ScribeError::Internal {
                detail: format!("stalled ownership test could not signal start: {error}"),
            })?;
            release.recv().map_err(|error| ScribeError::Internal {
                detail: format!("stalled ownership test release failed: {error}"),
            })?;
            drop(memory);
            Err(ScribeError::Internal {
                detail: "stalled ownership test completed".to_owned(),
            })
        }
    }
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

    /// Acquires one application-level persistence lane permit.
    ///
    /// A saturated lane waits on the bounded semaphore and records one
    /// saturation event; a closed lane refuses the operation.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the persistence lane is closed.
    async fn acquire_permit(&self) -> Result<tokio::sync::OwnedSemaphorePermit, ScribeError> {
        match Arc::clone(&self.permits).try_acquire_owned() {
            Ok(permit) => Ok(permit),
            Err(tokio::sync::TryAcquireError::NoPermits) => {
                self.saturation_events.fetch_add(1, Ordering::Relaxed);
                Arc::clone(&self.permits)
                    .acquire_owned()
                    .await
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("persistence CPU lane closed: {error}"),
                    })
            }
            Err(tokio::sync::TryAcquireError::Closed) => Err(ScribeError::Internal {
                detail: "persistence CPU lane closed".to_owned(),
            }),
        }
    }

    /// Runs one persistence CPU operation on the fixed Rayon pool.
    ///
    /// The operation and every root-backed owner it contains move into the
    /// detached worker. Dropping the async waiter therefore cannot release
    /// admitted memory before the worker exits.
    ///
    /// # Errors
    ///
    /// Returns the operation error, a stable panic refusal, or an internal
    /// error when the worker drops its result channel.
    pub(crate) async fn submit(
        &self,
        operation: ScribePersistenceCpuOp,
    ) -> Result<ScribePersistenceCpuResult, ScribeError> {
        let permit = self.acquire_permit().await?;
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
            let result = catch_unwind(AssertUnwindSafe(|| {
                execute_persistence_operation(operation, delay)
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
        memory: ScribeResources,
    },
    RetireWal {
        wal: WalHandle,
        segments: Vec<WalSegmentRef>,
    },
}

/// Results produced by [`ScribeWalIoPool`].
#[derive(Debug)]
pub(crate) enum ScribeWalIoResult {
    WalWritten {
        result: WalAppendResult,
    },
    WalSynced,
    Completed,
    ReplayStreamCompleted {
        /// Number of restored immutable generations.
        restored: usize,
        /// WAL references retained until the replay reader releases this worker.
        retirements: Vec<crate::scribe::shards::ReplayRetirement>,
    },
}

/// Bounded filesystem lane for WAL append, sync, replay support, and retirement.
#[derive(Debug, Clone)]
pub struct ScribeWalIoPool {
    /// Rayon executor dedicated to bounded WAL filesystem operations.
    pool: Arc<rayon::ThreadPool>,
    /// Queue permits bounding active and waiting WAL operations.
    permits: Arc<Semaphore>,
    /// Current queued and active operation count.
    depth: Arc<AtomicUsize>,
    /// Current operations executing on Rayon workers.
    active: Arc<AtomicUsize>,
    /// Worker panics converted into typed failures.
    panics: Arc<AtomicU64>,
    /// Successfully completed WAL operations.
    completed: Arc<AtomicU64>,
    /// WAL operations that returned a typed failure.
    failed: Arc<AtomicU64>,
    /// Times admission encountered a full WAL queue.
    saturation_events: Arc<AtomicU64>,
    /// Notification used by bounded drain waiters.
    drained: Arc<Notify>,
    /// Deterministic synchronization delay used by approved test harnesses.
    sync_delay: std::time::Duration,
    /// Maximum admitted WAL operations.
    capacity: usize,
    /// Retire operations submitted through the production lane in unit tests.
    #[cfg(test)]
    retire_submissions: Arc<AtomicU64>,
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
            #[cfg(test)]
            retire_submissions: Arc::new(AtomicU64::new(0)),
        })
    }

    pub(crate) async fn submit(
        &self,
        operation: ScribeWalIoOp,
    ) -> Result<ScribeWalIoResult, ScribeError> {
        #[cfg(test)]
        if matches!(&operation, ScribeWalIoOp::RetireWal { .. }) {
            self.retire_submissions.fetch_add(1, Ordering::AcqRel);
        }
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

    /// Returns retirement submissions observed at the production WAL lane boundary.
    #[cfg(test)]
    pub(crate) fn retire_submissions_for_test(&self) -> u64 {
        self.retire_submissions.load(Ordering::Acquire)
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
            let mut retirements = Vec::new();
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
                        if let Some(retirement) =
                            receiver
                                .blocking_recv()
                                .map_err(|_| ScribeError::Internal {
                                    detail: "replay owner dropped its completion response"
                                        .to_owned(),
                                })??
                        {
                            retirements.push(retirement);
                        }
                        restored = restored.saturating_add(1);
                    }
                    Ok(())
                },
            )?;
            Ok(ScribeWalIoResult::ReplayStreamCompleted {
                restored,
                retirements,
            })
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
        Array, ArrayRef, Int32Array, Int64Array, NullArray, StringArray, TimestampMicrosecondArray,
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

    use bytes::Bytes;

    use super::{
        ScribeIngressCpuPool, ScribePersistenceCpuOp, ScribePersistenceCpuPool, ScribeWalIoPool,
        decode, record_lane_saturation, source_schema_fingerprint, stamp_correlation_columns,
    };
    use crate::contracts::{IngressPayload, ScribeError};
    use crate::resources::{BifrostRole, BifrostRuntimeResources, ScribeMemoryCategory};
    use crate::schema::SchemaFingerprint;
    use crate::scribe::admission::EventTimeWindow;
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
            EventTimeWindow::default(),
            None,
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
            EventTimeWindow::default(),
            None,
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

    /// Canceling an awaiter cannot release bytes still owned by detached Rayon work.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn canceled_persistence_waiter_retains_memory_until_job_exit() {
        let roles = BifrostRuntimeResources::composed_for_test(
            1024 * 1024 * 1024,
            1024 * 1024 * 1024,
            [BifrostRole::Scribe],
        );
        let scribe = roles.scribe().expect("Scribe role");
        let memory = scribe
            .try_reserve_ingress(ScribeMemoryCategory::Raw, 4096)
            .expect("root-backed test lease");
        let pool = ScribePersistenceCpuPool::new(1);
        let submitted = pool.clone();
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
        let waiter = tokio::spawn(async move {
            submitted
                .submit(ScribePersistenceCpuOp::HoldMemory {
                    memory,
                    started: started_tx,
                    release: release_rx,
                })
                .await
        });
        tokio::task::spawn_blocking(move || started_rx.recv())
            .await
            .expect("start waiter")
            .expect("detached job started");
        waiter.abort();
        assert_eq!(
            scribe
                .snapshot()
                .expect("charged snapshot")
                .scribe_memory_used_bytes,
            4096
        );
        release_tx.send(()).expect("release detached job");
        pool.drain().await;
        assert_eq!(
            scribe
                .snapshot()
                .expect("settled snapshot")
                .scribe_memory_used_bytes,
            0
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
            EventTimeWindow::default(),
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

    /// Encodes a `RecordBatch` as Arrow IPC bytes for native ingress tests.
    fn ipc_bytes(b: &RecordBatch) -> Bytes {
        let mut payload = Vec::new();
        let mut w = StreamWriter::try_new(&mut payload, b.schema().as_ref()).expect("IPC writer");
        w.write(b).expect("IPC write");
        w.finish().expect("IPC finish");
        payload.into()
    }

    /// Decode one native ingress payload with fixed request, batch-id, and
    /// event-window fixtures, deriving the expected fingerprint from `schema`.
    ///
    /// The `run_id` canonicalization cases differ only in their input schema, so
    /// this helper keeps the boilerplate `decode` arguments out of each case.
    ///
    /// # Errors
    /// Returns the same [`ScribeError`] as [`decode`] when the payload fails
    /// native decoding, schema validation, or physical-field canonicalization.
    fn decode_native(
        payload: IngressPayload,
        principal: &Principal,
        schema: &Schema,
    ) -> Result<RecordBatch, ScribeError> {
        decode(
            payload,
            principal,
            source_schema_fingerprint(schema),
            &RequestId::now_v7(),
            Uuid::now_v7(),
            EventTimeWindow::default(),
        )
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
        let native = decode_native(
            IngressPayload::ArrowIpc(ipc_bytes(&rows)),
            &principal,
            rows.schema().as_ref(),
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
        let invalid = decode_native(
            IngressPayload::ArrowIpc(ipc_bytes(&invalid_type)),
            &principal,
            invalid_type.schema().as_ref(),
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
        let dup_err = decode_native(
            IngressPayload::ArrowIpc(ipc_bytes(&duplicate)),
            &principal,
            duplicate.schema().as_ref(),
        )
        .expect_err("duplicate native physical fields fail closed");
        assert!(matches!(dup_err, ScribeError::InvalidFrame));
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
            EventTimeWindow::default(),
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
            EventTimeWindow::default(),
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
            EventTimeWindow::default(),
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
            EventTimeWindow::default(),
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
            EventTimeWindow::default(),
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
            EventTimeWindow::default(),
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
            EventTimeWindow::default(),
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

    /// Field names of a stamped batch, in physical order.
    fn stamped_field_names(batch: &RecordBatch) -> Vec<String> {
        batch
            .schema()
            .fields()
            .iter()
            .map(|field| field.name().clone())
            .collect()
    }

    /// The canonical managed field-name order for a single `value` user column.
    fn canonical_managed_order() -> Vec<String> {
        Schema::new(crate::schema::with_managed_columns(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]))
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .collect()
    }

    /// D88: a stamped native payload that carries a valid caller `wyrd_event_time`
    /// has the full canonical field order, with `wyrd_event_time` lifted out of
    /// the user block into the managed slot (between `wyrd_request_id` and
    /// `wyrd_ingested_at`) — the order the Oracle's pinned sealed-fragment
    /// fingerprint requires. Before D88 the caller column stayed in the user block
    /// and diverged from [`with_managed_columns`], breaking every sealed read.
    #[test]
    fn native_caller_event_time_lands_in_canonical_managed_slot() {
        let (principal, card) = scoped_service_principal();
        let (event_field, event_array) =
            managed_event_time(vec![now_micros_offset(-60), now_micros_offset(-120)]);
        let rows = batch(
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
        let decoded = decode(
            ipc_payload(&rows),
            &principal,
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
            EventTimeWindow::default(),
        )
        .expect("native caller event time is accepted");
        assert_eq!(
            stamped_field_names(&decoded),
            canonical_managed_order(),
            "caller event time must yield the canonical managed field order",
        );
    }

    /// D88: a stamped native payload WITHOUT a caller `wyrd_event_time` (server
    /// stamps receipt time) has the identical canonical field order — the
    /// event-time value source does not perturb the physical layout.
    #[test]
    fn native_server_stamped_event_time_lands_in_canonical_managed_slot() {
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
            EventTimeWindow::default(),
        )
        .expect("native ingest without event time is server-stamped");
        assert_eq!(
            stamped_field_names(&decoded),
            canonical_managed_order(),
            "server-stamped event time must yield the canonical managed field order",
        );
    }

    /// D88: a stamped projected payload with a preserved caller `wyrd_event_time`
    /// also produces the canonical managed field order (`run_id` retained as
    /// correlation data, event time lifted into the managed slot).
    #[test]
    fn projected_preserved_event_time_lands_in_canonical_managed_slot() {
        let (event_field, event_array) = managed_event_time(vec![now_micros_offset(-3600)]);
        let rows = batch(
            vec![
                Field::new("value", DataType::Int64, false),
                Field::new("run_id", DataType::Utf8, true),
                event_field,
            ],
            vec![
                Arc::new(Int64Array::from(vec![1_i64])),
                Arc::new(StringArray::from(vec![Some("client-run")])),
                event_array,
            ],
        );
        let decoded = decode(
            IngressPayload::ProjectedArrow(vec![rows.clone()]),
            &principal(),
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
            EventTimeWindow::default(),
        )
        .expect("projected preserved event time is accepted");
        assert_eq!(
            stamped_field_names(&decoded),
            canonical_managed_order(),
            "projected preserved event time must yield the canonical managed field order",
        );
    }

    /// D88: a caller-supplied event time is preserved verbatim in the canonical
    /// slot and is never re-stamped — it is distinct from the receipt-time
    /// `wyrd_ingested_at`, proving the managed slot holds the caller value, not a
    /// server-stamped one.
    #[test]
    fn caller_event_time_value_is_verbatim_not_receipt_time() {
        let (principal, card) = scoped_service_principal();
        // A distinctive in-window instant well clear of "now" so it cannot
        // coincide with the receipt timestamp stamped into wyrd_ingested_at.
        let caller = now_micros_offset(-6 * 60 * 60);
        let (event_field, event_array) = managed_event_time(vec![caller]);
        let rows = batch(
            vec![
                Field::new("value", DataType::Int64, false),
                Field::new(CARD_REF, DataType::Utf8, false),
                event_field,
            ],
            vec![
                Arc::new(Int64Array::from(vec![1_i64])),
                Arc::new(StringArray::from(vec![card.to_string()])),
                event_array,
            ],
        );
        let decoded = decode(
            ipc_payload(&rows),
            &principal,
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
            EventTimeWindow::default(),
        )
        .expect("native caller event time is accepted");
        let event = decoded
            .column_by_name(WYRD_EVENT_TIME)
            .expect("event time column present")
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .expect("event time is TimestampMicrosecond");
        assert_eq!(
            event.value(0),
            caller,
            "caller event time is preserved verbatim"
        );
        let ingested = decoded
            .column_by_name(WYRD_INGESTED_AT)
            .expect("ingested_at column present")
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .expect("ingested_at is TimestampMicrosecond");
        assert_ne!(
            event.value(0),
            ingested.value(0),
            "caller event time must not be re-stamped with the receipt time",
        );
    }

    /// A native payload MAY carry a valid caller `wyrd_event_time`: its values
    /// survive decode+stamp unchanged, exactly once, and the user-schema
    /// fingerprint is identical to the same payload without the column.
    #[test]
    fn native_caller_event_time_is_preserved_without_fingerprint_drift() {
        let (principal, card) = scoped_service_principal();
        // Use recent timestamps within the default window (within 1 day of now).
        let recent_a = now_micros_offset(-60); // 60 seconds ago
        let recent_b = now_micros_offset(-120); // 120 seconds ago
        let (event_field, event_array) = managed_event_time(vec![recent_a, recent_b]);
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
            EventTimeWindow::default(),
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
        assert_eq!(event.value(0), recent_a);
        assert_eq!(event.value(1), recent_b);
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
            EventTimeWindow::default(),
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
                EventTimeWindow::default(),
            )
            .expect_err("invalid native event time fails closed");
            assert!(matches!(error, ScribeError::InvalidFrame), "{error:?}");
        }
    }

    // -----------------------------------------------------------------------
    // Event-time acceptance window tests (T42 / D85)
    // -----------------------------------------------------------------------

    /// Computes epoch-microseconds for `now + offset_secs` without panicking.
    fn now_micros_offset(offset_secs: i64) -> i64 {
        let now_u128 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock is post-epoch")
            .as_micros();
        let now = i64::try_from(now_u128).unwrap_or(i64::MAX);
        now.saturating_add(offset_secs.saturating_mul(1_000_000))
    }

    /// A narrow window used by several tests: 2 days past, 2 hours future.
    fn narrow_window() -> EventTimeWindow {
        const SECS_PER_HOUR: u64 = 3_600;
        const SECS_PER_DAY: u64 = 86_400;
        EventTimeWindow {
            past: std::time::Duration::from_secs(2 * SECS_PER_DAY),
            future: std::time::Duration::from_secs(2 * SECS_PER_HOUR),
        }
    }

    /// Native batch with event times inside the default window is accepted and
    /// the values are preserved verbatim.
    #[test]
    fn native_event_time_within_window_accepted() {
        let (principal, card) = scoped_service_principal();
        let t = now_micros_offset(-3600); // 1 hour ago
        let (event_field, event_array) = managed_event_time(vec![t]);
        let rows = batch(
            vec![
                Field::new("value", DataType::Int64, false),
                Field::new(CARD_REF, DataType::Utf8, false),
                event_field,
            ],
            vec![
                Arc::new(Int64Array::from(vec![1_i64])),
                Arc::new(StringArray::from(vec![card.to_string()])),
                event_array,
            ],
        );
        let decoded = decode(
            ipc_payload(&rows),
            &principal,
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
            EventTimeWindow::default(),
        )
        .expect("in-window native event time is accepted");
        let arr = decoded
            .column_by_name(WYRD_EVENT_TIME)
            .expect("event time present")
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .expect("TimestampMicrosecond");
        assert_eq!(arr.value(0), t, "value is preserved verbatim");
    }

    /// Value exactly at the past edge (receipt − past) is accepted (inclusive).
    #[test]
    fn native_event_time_past_edge_accepted() {
        let (principal, card) = scoped_service_principal();
        let window = narrow_window();
        // past_secs − 1 s safety margin to absorb any clock tick between
        // the test value computation and the window enforcement.
        let edge = now_micros_offset(-(2 * 24 * 60 * 60 - 1));
        let (event_field, event_array) = managed_event_time(vec![edge]);
        let rows = batch(
            vec![
                Field::new("value", DataType::Int64, false),
                Field::new(CARD_REF, DataType::Utf8, false),
                event_field,
            ],
            vec![
                Arc::new(Int64Array::from(vec![1_i64])),
                Arc::new(StringArray::from(vec![card.to_string()])),
                event_array,
            ],
        );
        decode(
            ipc_payload(&rows),
            &principal,
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
            window,
        )
        .expect("past-edge value is accepted (inclusive bound)");
    }

    /// Value exactly at the future edge (receipt + future) is accepted (inclusive).
    #[test]
    fn native_event_time_future_edge_accepted() {
        let (principal, card) = scoped_service_principal();
        let window = narrow_window();
        // future_secs − 1 s safety margin.
        let edge = now_micros_offset(2 * 60 * 60 - 1);
        let (event_field, event_array) = managed_event_time(vec![edge]);
        let rows = batch(
            vec![
                Field::new("value", DataType::Int64, false),
                Field::new(CARD_REF, DataType::Utf8, false),
                event_field,
            ],
            vec![
                Arc::new(Int64Array::from(vec![1_i64])),
                Arc::new(StringArray::from(vec![card.to_string()])),
                event_array,
            ],
        );
        decode(
            ipc_payload(&rows),
            &principal,
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
            window,
        )
        .expect("future-edge value is accepted (inclusive bound)");
    }

    /// A value older than the past bound is rejected with `EventTimeOutOfRange`.
    /// The whole batch is refused; no partial write occurs.
    #[test]
    fn native_event_time_before_past_bound_rejected() {
        let (principal, card) = scoped_service_principal();
        // 31 days ago, well outside the default 30-day past window.
        let old = now_micros_offset(-(31 * 24 * 60 * 60));
        let (event_field, event_array) = managed_event_time(vec![old]);
        let rows = batch(
            vec![
                Field::new("value", DataType::Int64, false),
                Field::new(CARD_REF, DataType::Utf8, false),
                event_field,
            ],
            vec![
                Arc::new(Int64Array::from(vec![1_i64])),
                Arc::new(StringArray::from(vec![card.to_string()])),
                event_array,
            ],
        );
        let err = decode(
            ipc_payload(&rows),
            &principal,
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
            EventTimeWindow::default(),
        )
        .expect_err("31-day-old value must be rejected");
        assert!(
            matches!(
                err,
                ScribeError::EventTimeOutOfRange {
                    value_micros,
                    ..
                } if value_micros == old
            ),
            "expected EventTimeOutOfRange with the offending value; got {err:?}"
        );
    }

    /// A value further in the future than the future bound is rejected with
    /// `EventTimeOutOfRange`.
    #[test]
    fn native_event_time_after_future_bound_rejected() {
        let (principal, card) = scoped_service_principal();
        // 25 hours ahead, outside the default 24-hour future window.
        let future = now_micros_offset(25 * 60 * 60);
        let (event_field, event_array) = managed_event_time(vec![future]);
        let rows = batch(
            vec![
                Field::new("value", DataType::Int64, false),
                Field::new(CARD_REF, DataType::Utf8, false),
                event_field,
            ],
            vec![
                Arc::new(Int64Array::from(vec![1_i64])),
                Arc::new(StringArray::from(vec![card.to_string()])),
                event_array,
            ],
        );
        let err = decode(
            ipc_payload(&rows),
            &principal,
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
            EventTimeWindow::default(),
        )
        .expect_err("25-hour-future value must be rejected");
        assert!(
            matches!(
                err,
                ScribeError::EventTimeOutOfRange {
                    value_micros,
                    ..
                } if value_micros == future
            ),
            "expected EventTimeOutOfRange with the offending value; got {err:?}"
        );
    }

    /// A projected (OTLP) batch with an in-window event time is accepted.
    #[test]
    fn projected_event_time_within_window_accepted() {
        let t = now_micros_offset(-3600);
        let (event_field, event_array) = managed_event_time(vec![t]);
        let rows = batch(
            vec![Field::new("value", DataType::Int64, false), event_field],
            vec![Arc::new(Int64Array::from(vec![1_i64])), event_array],
        );
        let decoded = decode(
            IngressPayload::ProjectedArrow(vec![rows.clone()]),
            &principal(),
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
            EventTimeWindow::default(),
        )
        .expect("projected in-window event time is accepted");
        let arr = decoded
            .column_by_name(WYRD_EVENT_TIME)
            .expect("event time present")
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .expect("TimestampMicrosecond");
        assert_eq!(arr.value(0), t, "projected value is preserved verbatim");
    }

    /// A projected (OTLP) batch with an out-of-range event time is rejected with
    /// `EventTimeOutOfRange` — proving both paths share enforcement.
    #[test]
    fn projected_event_time_out_of_range_rejected() {
        let old = now_micros_offset(-(31 * 24 * 60 * 60));
        let (event_field, event_array) = managed_event_time(vec![old]);
        let rows = batch(
            vec![Field::new("value", DataType::Int64, false), event_field],
            vec![Arc::new(Int64Array::from(vec![1_i64])), event_array],
        );
        let err = decode(
            IngressPayload::ProjectedArrow(vec![rows.clone()]),
            &principal(),
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
            EventTimeWindow::default(),
        )
        .expect_err("projected out-of-range event time must be rejected");
        assert!(
            matches!(
                err,
                ScribeError::EventTimeOutOfRange {
                    value_micros,
                    ..
                } if value_micros == old
            ),
            "expected EventTimeOutOfRange; got {err:?}"
        );
    }

    /// When `wyrd_event_time` is absent the server stamps receipt time and no
    /// window check runs — behavior is byte-identical to the prior contract.
    #[test]
    fn absent_event_time_unchanged_regression() {
        let rows = batch(
            vec![Field::new("value", DataType::Int64, false)],
            vec![Arc::new(Int64Array::from(vec![1_i64]))],
        );
        // Very tight window (1 second past/future) to confirm no spurious check.
        let tight_window = EventTimeWindow {
            past: std::time::Duration::from_secs(1),
            future: std::time::Duration::from_secs(1),
        };
        let decoded = decode(
            IngressPayload::ProjectedArrow(vec![rows.clone()]),
            &principal(),
            source_schema_fingerprint(rows.schema().as_ref()),
            &RequestId::now_v7(),
            Uuid::now_v7(),
            tight_window,
        )
        .expect("absent event time is server-stamped without window check");
        assert!(
            decoded.schema().index_of(WYRD_EVENT_TIME).is_ok(),
            "server-stamped event time must be present"
        );
        // Confirm it is NOT the tight_window that rejected it.
        let arr = decoded
            .column_by_name(WYRD_EVENT_TIME)
            .expect("event time stamped")
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .expect("TimestampMicrosecond");
        assert!(!arr.is_null(0), "server-stamped event time is non-null");
    }

    /// `EventTimeWindow::contains` arithmetic — boundary and overflow cases.
    #[test]
    fn event_time_window_contains_boundary_and_overflow() {
        let window = EventTimeWindow {
            past: std::time::Duration::from_secs(1),
            future: std::time::Duration::from_millis(500),
        };
        let receipt = 2_000_000_i64;
        // Inclusive past edge.
        assert!(window.contains(1_000_000, receipt));
        // Inclusive future edge.
        assert!(window.contains(2_500_000, receipt));
        // One µs before past edge → rejected.
        assert!(!window.contains(999_999, receipt));
        // One µs after future edge → rejected.
        assert!(!window.contains(2_500_001, receipt));
        // Saturating arithmetic at i64 boundaries must not panic.
        // Duration::MAX saturates past_micros/future_micros to i64::MAX,
        // so lo = receipt.saturating_sub(i64::MAX) and hi = receipt.saturating_add(i64::MAX).
        // With receipt = 0: lo = i64::MIN + 1, hi = i64::MAX.
        let wide = EventTimeWindow {
            past: std::time::Duration::MAX,
            future: std::time::Duration::MAX,
        };
        // i64::MIN is excluded because lo = i64::MIN + 1 after saturating_sub.
        assert!(!wide.contains(i64::MIN, 0));
        // i64::MIN + 1 is the lowest accepted value.
        assert!(wide.contains(i64::MIN + 1, 0));
        // i64::MAX is always accepted.
        assert!(wide.contains(i64::MAX, 0));
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
                EventTimeWindow::default(),
            )
            .expect_err("server-owned managed columns are reserved");
            assert!(matches!(error, ScribeError::InvalidFrame), "{reserved}");
        }
    }

    /// Lane saturation emits a closed lane-labelled rejection counter.
    ///
    /// Proves [`record_lane_saturation`] advances
    /// `bifrost_scribe_lane_saturation_total{lane}` under the closed lane
    /// vocabulary and carries no tenant, table, or request identity, so a lane
    /// shedding load is distinguishable in telemetry from a memory rejection.
    #[test]
    fn lane_saturation_emits_rejection_counter() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            record_lane_saturation("ingress");
            record_lane_saturation("ingress");
        });
        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_scribe_lane_saturation_total{lane=\"ingress\"}")
                .copied(),
            Some(2)
        );
        assert!(!snapshot.counters.keys().any(|key| {
            ["tenant", "table", "request", "node", "error", "sql"]
                .iter()
                .any(|forbidden| key.contains(forbidden))
        }));
    }
}
