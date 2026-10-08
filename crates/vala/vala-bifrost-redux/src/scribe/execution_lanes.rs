//! Bounded CPU execution lanes used by Scribe admission.

use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use arrow::array::{Array, ArrayRef, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use iceberg::spec::Schema as IcebergSchema;
use tokio::sync::{Notify, Semaphore, mpsc, oneshot};

use crate::catalog::TenantTableBinding;
use crate::contracts::{IngressPayload, ScribeError};
use crate::resources::ScribeResources;
use crate::schema::fingerprint::SchemaFingerprint;
use crate::scribe::admission::EventTimeWindow;
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::replay::ReplayedSealKey;
use crate::scribe::seal_key::SealKey;
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::{
    PreparedWalAppend, WalAppendResult, WalHandle, WalSegment, WalSegmentRef,
};
use std::str::FromStr;
use wyrd_runtime::Principal;
use wyrd_spec::reference::{CardRef, CardRefScope};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::managed_columns::{
    CARD_REF, CARD_UID, PRINCIPAL_ID, RUN_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT, WYRD_REQUEST_ID,
};

use crate::tables::CorrelationPolicy;
use crate::tables::managed_columns::{ensure_managed_columns, is_managed_column};

#[cfg(test)]
const INGRESS_QUEUE_ITEMS: usize = 256;
#[cfg(test)]
const WAL_IO_QUEUE_ITEMS: usize = 256;

/// Publish both lane gauges at zero so the lane's series exist before any job.
///
/// A Prometheus gauge only appears in a render once a handle has been created
/// for its exact label set, so a lane that never received work would otherwise
/// leave a hole rather than a zero and be indistinguishable from a lane that is
/// absent. Called once per pool from its constructor, which is also the only
/// point at which the lane's occupancy is authoritatively zero.
fn register_lane_gauges(lane: &'static str) {
    metrics::gauge!("bifrost_scribe_lane_queued", "lane" => lane).increment(0.0);
    metrics::gauge!("bifrost_scribe_lane_active", "lane" => lane).increment(0.0);
}

/// Wait until a lane's outstanding-job counter reaches zero.
///
/// The wake edge is the `Notify::notify_waiters()` call a lane worker makes
/// immediately after decrementing `depth` on its terminal. That call wakes only
/// waiters that are *already registered*, and `Notify::notified()` does not
/// register a waiter until its future is first polled. Building the future,
/// observing a non-zero `depth`, and only then awaiting therefore drops any
/// notification published in between, parking the drain forever on a lane that
/// has already gone idle. The window is small but genuinely reachable: the
/// decrement and the notify run on a Rayon worker thread concurrent with this
/// caller.
///
/// Registering with [`tokio::sync::futures::Notified::enable`] before sampling
/// `depth` closes it. Any `notify_waiters()` published after that registration
/// is delivered to this waiter, and the loop re-checks `depth` after each wake
/// so a superseded or spurious edge simply re-arms. Returns immediately when
/// the lane is already idle.
async fn wait_lane_drained(depth: &AtomicUsize, drained: &Notify) {
    loop {
        let notified = drained.notified();
        tokio::pin!(notified);
        // Register before the load: a terminal that lands between the two must
        // wake this waiter rather than fall into a gap where it does not exist.
        notified.as_mut().enable();
        if depth.load(Ordering::Acquire) == 0 {
            return;
        }
        notified.await;
    }
}

/// Count one job entering the lane's application queue.
///
/// Paired one-to-one with the `depth` increment at submit. The gauge is moved
/// additively rather than set from a snapshot of `depth`: a read-then-set
/// publishes a value another thread may already have superseded, which leaves
/// the gauge stale — including stale non-zero once the lane has quiesced.
/// Every mutation of this gauge is a single `+1`/`-1` at the transition that
/// causes it, so concurrent lane workers compose instead of clobbering.
fn record_lane_enqueued(lane: &'static str) {
    metrics::gauge!("bifrost_scribe_lane_queued", "lane" => lane).increment(1.0);
}

/// Move one job from waiting to running when its lane worker starts it.
///
/// `bifrost_scribe_lane_queued` counts only jobs waiting to start, so the
/// start transition decrements it and increments `bifrost_scribe_lane_active`.
/// See [`record_lane_enqueued`] for why both moves are additive.
fn record_lane_started(lane: &'static str) {
    metrics::gauge!("bifrost_scribe_lane_queued", "lane" => lane).decrement(1.0);
    metrics::gauge!("bifrost_scribe_lane_active", "lane" => lane).increment(1.0);
}

/// Release one running job from the active gauge once its worker finished.
///
/// Runs on every terminal including a caught panic, after
/// [`record_lane_started`] already removed the job from the waiting gauge, so
/// a quiesced lane settles both gauges at exactly zero.
fn record_lane_finished(lane: &'static str) {
    metrics::gauge!("bifrost_scribe_lane_active", "lane" => lane).decrement(1.0);
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
/// the closed lane vocabulary shared with [`record_lane_enqueued`] and
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
        register_lane_gauges("ingress");
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
    /// sourced from `AdmissionConfig` by the caller. It is passed by value
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
        inputs: IngressDecodeInputs,
    ) -> Result<RecordBatch, ScribeError> {
        let IngressDecodeInputs {
            principal,
            card_scope,
            expected_schema_fingerprint,
            request_id,
            receipt_micros,
            window,
            definition,
            registered_schema,
        } = inputs;
        let Ok(permit) = self.permits.clone().try_acquire_owned() else {
            self.saturation_events.fetch_add(1, Ordering::Relaxed);
            record_lane_saturation("ingress");
            return Err(ScribeError::IngestBusy {
                table: "ingress".to_owned(),
            });
        };
        self.depth.fetch_add(1, Ordering::AcqRel);
        record_lane_enqueued("ingress");
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
            record_lane_started("ingress");
            let result = catch_unwind(AssertUnwindSafe(|| {
                decode(
                    payload,
                    &DecodeContext {
                        principal: &principal,
                        card_scope: card_scope.as_ref(),
                        expected_schema_fingerprint,
                        request_id: &request_id,
                        window,
                        receipt_micros,
                        definition,
                        registered_schema: registered_schema.as_deref(),
                    },
                )
            }));
            depth.fetch_sub(1, Ordering::AcqRel);
            active.fetch_sub(1, Ordering::AcqRel);
            record_lane_finished("ingress");
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

    /// Runs one pre-ACK CPU job, such as preprocessing an admitted append, on
    /// this lane.
    ///
    /// Request-path work stays here rather than on the persistence lane, so a
    /// background staging encode or claim merge never queues ahead of a write.
    /// The job is moved into the worker, so a cancelled caller still leaves the
    /// job owning whatever memory it carries until it finishes.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the ingress queue is saturated,
    /// the job's own error, or [`ScribeError::Internal`] when the worker panics
    /// or drops its result.
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
        record_lane_enqueued("ingress");
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
            record_lane_started("ingress");
            let result = catch_unwind(AssertUnwindSafe(job));
            depth.fetch_sub(1, Ordering::AcqRel);
            active.fetch_sub(1, Ordering::AcqRel);
            record_lane_finished("ingress");
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

    /// Wait until every job submitted to this lane has reached a terminal.
    ///
    /// Shutdown uses this to establish that no worker thread is still touching
    /// WAL state, resource leases, or lane metrics before the owning Scribe
    /// releases them. Delegates to [`wait_lane_drained`], which registers for
    /// the drain edge before sampling the counter so an already-idle lane
    /// cannot be missed.
    pub(crate) async fn drain(&self) {
        wait_lane_drained(&self.depth, &self.drained).await;
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
/// All other managed columns (`wyrd_ingested_at`, `wyrd_request_id`,
/// principal, and card columns) remain
/// unconditionally server-owned and are rejected when supplied.
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
    context: &DecodeContext<'_>,
) -> Result<RecordBatch, ScribeError> {
    let batches = match payload {
        IngressPayload::ArrowIpc(bytes) => {
            let reader = arrow::ipc::reader::StreamReader::try_new(Cursor::new(bytes), None)
                .map_err(|_| ScribeError::InvalidFrame)?;
            reader
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| ScribeError::InvalidFrame)?
        }
        IngressPayload::Canonical(canonical) => canonical.batches,
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
    decode_rows(&rows, context)
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
    context: &DecodeContext<'_>,
) -> Result<RecordBatch, ScribeError> {
    decode_rows(rows, context)
}

/// The owned decode inputs one ingress frame hands to the bounded CPU lane.
///
/// The lane moves these into its worker, so they are owned rather than the
/// borrowed [`DecodeContext`] the decode itself assembles from them.
pub(crate) struct IngressDecodeInputs {
    /// Authenticated principal stamped as the writer.
    pub(crate) principal: Principal,
    /// Card scope every `card_ref` is authorized and stamped against: the
    /// registry-resolved Cards of an unbound writer, or the signed scope.
    pub(crate) card_scope: Option<CardRefScope>,
    /// Catalog fingerprint required of the caller-owned source schema.
    pub(crate) expected_schema_fingerprint: SchemaFingerprint,
    /// Stable request identity stamped into every accepted row.
    pub(crate) request_id: RequestId,
    /// Admission instant stamped as `wyrd_ingested_at` and anchoring the
    /// event-time window.
    pub(crate) receipt_micros: i64,
    /// Accepted caller event-time window.
    pub(crate) window: EventTimeWindow,
    /// Canonical built-in whose physical identity the decode must preserve.
    pub(crate) definition: Option<&'static crate::tables::BuiltinTableDefinition>,
    /// Registered Iceberg schema whose field ids every stamped batch carries.
    ///
    /// `None` only for the embedded engine seam, which has no catalog owner
    /// and writes objects that no registered table promotes.
    pub(crate) registered_schema: Option<Arc<IcebergSchema>>,
}

/// Immutable validation and stamping context for one decoded batch.
///
/// Native persistence preprocessing builds one of these per record batch of a
/// request; every batch of one request stamps the same request identity and
/// admission instant.
pub(crate) struct DecodeContext<'a> {
    /// Authenticated principal stamped as the writer.
    pub(crate) principal: &'a Principal,
    /// Card scope every `card_ref` is authorized and stamped against: the
    /// registry-resolved Cards of an unbound writer, or the signed scope.
    pub(crate) card_scope: Option<&'a CardRefScope>,
    /// Catalog fingerprint required of the caller-owned source schema.
    pub(crate) expected_schema_fingerprint: SchemaFingerprint,
    /// Stable request identity stamped into every accepted row.
    pub(crate) request_id: &'a RequestId,
    /// Accepted caller event-time window.
    pub(crate) window: EventTimeWindow,
    /// Admission instant in epoch microseconds, read once per batch.
    ///
    /// Stamped as `wyrd_ingested_at`, used as `wyrd_event_time` when the caller
    /// supplies none, and the anchor of the event-time acceptance window.
    pub(crate) receipt_micros: i64,
    /// Canonical built-in whose physical identity this decode must preserve.
    ///
    /// `Some` only for a canonical signal table. A dynamic or pre-declared
    /// table leaves this `None` and keeps the existing catalog-fingerprint and
    /// managed-field policy.
    pub(crate) definition: Option<&'static crate::tables::BuiltinTableDefinition>,
    /// Registered Iceberg schema whose field ids every stamped batch carries.
    ///
    /// `None` only for the embedded engine seam, which has no catalog owner
    /// and writes objects that no registered table promotes.
    pub(crate) registered_schema: Option<&'a IcebergSchema>,
}

/// Applies source-contract validation and server-managed stamping to one batch.
///
/// The final step stamps the registered Iceberg table's field ids onto every
/// field when the context carries that schema, so the WAL, staged runs, and
/// sealed objects all number their columns exactly as the table does.
///
/// # Errors
///
/// Returns a stable Scribe refusal for a duplicated column name, reserved
/// columns, fingerprint mismatch, card-scope failure, invalid event
/// time, or managed column construction failure, and a fingerprint mismatch
/// when a stamped field has no registered counterpart.
fn decode_rows(
    rows: &RecordBatch,
    context: &DecodeContext<'_>,
) -> Result<RecordBatch, ScribeError> {
    let mut seen = std::collections::HashSet::with_capacity(rows.schema().fields().len());
    for field in rows.schema().fields() {
        // Duplicate names are refused for every table kind — built-in, dynamic,
        // and pre-declared — before fingerprint, correlation, or stamping work.
        // `card_ref` is a permitted client correlation column, so a duplicate
        // pair is removed wholesale by projection while name lookup only ever
        // resolves the first occurrence; an authorized first value would
        // otherwise hide a second, unauthorized assertion.
        if !seen.insert(field.name().as_str()) {
            return Err(ScribeError::InvalidFrame);
        }
        // A caller MAY supply `wyrd_event_time` as the authoritative event
        // time; it is then validated and preserved in
        // `stamp_correlation_columns`. `run_id` is caller correlation that this
        // function relinquishes and restamps exactly once in its slot. Every
        // other managed column is unconditionally server-owned, so supplying
        // one is a refusal rather than an override.
        let name = field.name().as_str();
        if is_managed_column(name) && name != WYRD_EVENT_TIME && name != RUN_ID {
            return Err(ScribeError::InvalidFrame);
        }
    }
    let actual_source_fingerprint = source_schema_fingerprint(rows.schema().as_ref());
    if actual_source_fingerprint != context.expected_schema_fingerprint {
        return Err(ScribeError::FingerprintMismatch {
            table: "resolved ingress table".to_owned(),
        });
    }
    if let Some(definition) = context
        .definition
        .filter(|definition| definition.canonical_validator.is_some())
    {
        enforce_canonical_source_contract(rows, definition)?;
    }
    validate_card_scope(rows, context.card_scope)?;
    let stamped = stamp_correlation_columns(rows, context)?;
    match context.registered_schema {
        Some(registered) => crate::tables::stamp_registered_field_ids(&stamped, registered)
            .map_err(|_| ScribeError::FingerprintMismatch {
                table: "resolved ingress table".to_owned(),
            }),
        None => Ok(stamped),
    }
}

/// Enforces one canonical built-in's exact user contract before stamping.
///
/// The caller of a canonical signal table owns exactly the table's declared
/// ledger plus the permitted correlation columns (`card_ref`, `run_id`, and an
/// optional `wyrd_event_time`). This rejects any unknown `wyrd_*` field, runs
/// the table's own registered value validator over
/// the remaining user block, and then requires that block to match
/// `(definition.arrow_fields)()` in order, name, nullability, and type shape,
/// so no structurally different Arrow spelling reaches the physical schema.
/// Field metadata is not compared: the field id and sensitivity tag are the
/// server's own physical identity, stamped downstream from the registered
/// table and the definition, so
/// a writer building from the published description neither supplies them nor
/// can be wrong about them.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for an unknown reserved column, and
/// [`ScribeError::FingerprintMismatch`] when the table's value
/// validator refuses the batch or the remaining user fields differ from the
/// table's declared fields.
fn enforce_canonical_source_contract(
    rows: &RecordBatch,
    definition: &'static crate::tables::BuiltinTableDefinition,
) -> Result<(), ScribeError> {
    let schema = rows.schema();
    for field in schema.fields() {
        if field.name().starts_with("wyrd_") && field.name() != WYRD_EVENT_TIME {
            return Err(ScribeError::InvalidFrame);
        }
    }
    let permitted = [CARD_REF, RUN_ID, WYRD_EVENT_TIME];
    let mut fields = Vec::with_capacity(schema.fields().len());
    let mut columns = Vec::with_capacity(schema.fields().len());
    for (index, field) in schema.fields().iter().enumerate() {
        if permitted.contains(&field.name().as_str()) {
            continue;
        }
        fields.push(field.as_ref().clone());
        columns.push(Arc::clone(rows.column(index)));
    }
    let declared = (definition.arrow_fields)();
    let shape_matches = fields.len() == declared.len()
        && fields.iter().zip(&declared).all(|(supplied, expected)| {
            supplied.name() == expected.name()
                && supplied.is_nullable() == expected.is_nullable()
                && supplied.data_type().equals_datatype(expected.data_type())
        });
    if !shape_matches {
        return Err(ScribeError::FingerprintMismatch {
            table: format!("{}.{}", definition.namespace, definition.name),
        });
    }
    let user = RecordBatch::try_new(Arc::new(Schema::new(fields)), columns)
        .map_err(|_| ScribeError::InvalidFrame)?;
    if let Some(validate) = definition.canonical_validator {
        validate(&user).map_err(|reason| {
            tracing::warn!(
                table = %format!("{}.{}", definition.namespace, definition.name),
                %reason,
                "canonical user block was refused by the table's value validator"
            );
            ScribeError::FingerprintMismatch {
                table: format!("{}.{}", definition.namespace, definition.name),
            }
        })?;
    }
    Ok(())
}

/// Confirms a stamped canonical batch still carries its table-owned identity.
///
/// The stamped batch is built from `(definition.schema)()`, so its correlation
/// and managed `Field`s — sensitivity metadata, nullability — are
/// the table's own rather than locally reconstructed. This re-derives the
/// canonical physical fingerprint from the stamped schema and compares it with
/// the definition's resolved identity, so any future divergence between the
/// stamping order and the table's declaration fails closed here.
///
/// # Errors
///
/// Returns [`ScribeError::FingerprintMismatch`] when the stamped physical
/// schema does not fingerprint to the table's resolved canonical physical
/// identity.
fn enforce_canonical_physical_identity(
    stamped: &RecordBatch,
    definition: &'static crate::tables::BuiltinTableDefinition,
) -> Result<(), ScribeError> {
    let mismatch = || ScribeError::FingerprintMismatch {
        table: format!("{}.{}", definition.namespace, definition.name),
    };
    let expected = crate::tables::ResolvedSchemaIdentity::for_builtin(definition)
        .canonical_physical_fingerprint
        .ok_or_else(mismatch)?;
    let actual = crate::tables::CanonicalPhysicalFingerprint::from_physical_fields(
        stamped.schema().fields(),
    )
    .map_err(|_| mismatch())?;
    if actual == expected {
        Ok(())
    } else {
        Err(mismatch())
    }
}

fn source_schema_fingerprint(schema: &Schema) -> SchemaFingerprint {
    let fields: Vec<Field> = schema
        .fields()
        .iter()
        .filter(|field| {
            !matches!(
                field.name().as_str(),
                CARD_REF | CARD_UID | PRINCIPAL_ID | "run_id"
            ) && !field.name().starts_with("wyrd_")
        })
        .map(|field| field.as_ref().clone())
        .collect();
    SchemaFingerprint::from_arrow_schema(&Schema::new(fields))
}

/// Captures one deterministic receipt timestamp for planning and projection.
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

/// Authorize every client-supplied `card_ref` against the writer's Card scope.
///
/// A batch without the column carries no Card correlation and is admitted
/// unchanged. Within the column, a null is a valid uncorrelated row; a present
/// value must parse under the `CardRef` grammar and name an identity `scope`
/// authorizes. `scope` is a Card-bound principal's verified signed scope, or
/// the registry-resolved Cards Gate attached for an unbound writer, so a Card
/// that is unregistered, of a non-observation kind, or outside the signed
/// scope is refused alike. Authorization is decided for the whole batch
/// before any row is admitted, so a single denied row refuses the frame.
///
/// # Errors
///
/// Returns [`ScribeError::CardScopeDenied`] when the column is not UTF-8, or
/// when a row supplies a present value and there is no scope, the value is
/// malformed, or the value lies outside the scope. A batch whose correlation
/// column is entirely null needs no scope.
fn validate_card_scope(
    rows: &RecordBatch,
    scope: Option<&CardRefScope>,
) -> Result<(), ScribeError> {
    let Some(column) = rows.column_by_name(CARD_REF) else {
        return Ok(());
    };
    let cards = column
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or(ScribeError::CardScopeDenied)?;
    for index in 0..cards.len() {
        if cards.is_null(index) {
            // An absent correlation is a valid row: it simply carries no Card.
            continue;
        }
        let scope = scope.ok_or(ScribeError::CardScopeDenied)?;
        let raw = cards.value(index);
        let card = CardRef::from_str(raw).map_err(|_| ScribeError::CardScopeDenied)?;
        if !scope.authorizes(&card) {
            return Err(ScribeError::CardScopeDenied);
        }
    }
    Ok(())
}

/// Lifts a caller-supplied `run_id` column out of a payload, if it carries one.
///
/// `run_id` is a correlation name the caller may echo, so the column is taken
/// verbatim rather than re-stamped. Uniqueness is checked here because a
/// duplicated correlation name would make the batch's logical identity
/// ambiguous once the managed block is appended.
///
/// # Errors
/// Returns [`ScribeError::InvalidFrame`] when the payload carries more than one
/// `run_id` column or when its type is not `Utf8`.
fn caller_run_id_column(rows: &RecordBatch) -> Result<Option<ArrayRef>, ScribeError> {
    let schema = rows.schema();
    let mut matched = None;
    for (index, field) in schema.fields().iter().enumerate() {
        if field.name() != RUN_ID {
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
        .transpose()
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
/// `wyrd_event_time` with the admission instant in that same slot. Either way
/// `wyrd_event_time` is written exactly once in its declared slot, so the
/// stamped batch's managed field order is the
/// [`MANAGED_COLUMNS`](crate::tables::managed_columns::MANAGED_COLUMNS) order in
/// every payload mode (D88). `wyrd_ingested_at` is always the admission instant
/// regardless of caller event time. Projected payloads retain their remaining
/// correlation values unchanged.
///
/// The `window` check and both time columns use the one admission instant in
/// `context`, read once per batch at admission.
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
    context: &DecodeContext<'_>,
) -> Result<RecordBatch, ScribeError> {
    let DecodeContext {
        principal,
        request_id,
        window,
        receipt_micros,
        ..
    } = context;
    let (principal, request_id, window) = (*principal, *request_id, *window);
    let receipt_micros = *receipt_micros;
    // Type/null/duplicate checks come first (T38). A malformed column stays
    // `InvalidFrame` regardless of window membership.
    validate_native_event_time(rows)?;
    // Enforce the acceptance window for a present caller column.
    if rows.schema().index_of(WYRD_EVENT_TIME).is_ok() {
        let past_exempt = context
            .definition
            .is_some_and(|definition| definition.past_event_time_exempt);
        enforce_event_time_window(rows, window, receipt_micros, past_exempt)?;
    }
    // A present `wyrd_event_time` (validated above for native payloads and
    // window-checked above for both modes) is always lifted out of the user
    // projection here and reinserted verbatim in its managed slot by
    // `managed_arrays`, never re-stamped. When absent, the server stamps the
    // admission instant into that slot instead. Decoupling "exclude from the
    // user block" from "stamp the admission value" keeps the stamped field
    // order equal to the managed-column declaration in every payload mode (D88).
    let caller_event_time = rows
        .schema()
        .index_of(WYRD_EVENT_TIME)
        .ok()
        .map(|index| Arc::clone(rows.column(index)));
    let caller_run_id = caller_run_id_column(rows)?;
    let mut fields = user_fields(rows);
    let mut columns = user_columns(rows);
    // A table whose rows carry their own identity declares
    // `CorrelationPolicy::None`, and its registered physical schema has no
    // correlation or request slots. Stamping them anyway would seal an object
    // with more columns than the table it is promoted into.
    let policy = if correlation_envelope_applies(context) {
        CorrelationPolicy::Observation
    } else {
        CorrelationPolicy::None
    };
    fields.extend(ensure_managed_columns(Vec::new(), policy));
    columns.extend(managed_arrays(
        rows,
        &ManagedValues {
            principal,
            card_scope: context.card_scope,
            request_id,
            receipt_micros,
            caller_run_id,
            caller_event_time,
        },
        policy,
    )?);
    // A canonical built-in constructs only the arrays here: every correlation
    // and managed `Field` — with its sensitivity metadata — is
    // cloned from the table's own physical schema, and the locally assembled
    // field list is compared against it first so a drifting stamping order is
    // refused rather than silently relabelled.
    let Some(definition) = context.definition else {
        return RecordBatch::try_new(Arc::new(Schema::new(fields)), columns)
            .map_err(|_| ScribeError::InvalidFrame);
    };
    let physical = (definition.schema)();
    let aligned = physical.fields().len() == fields.len()
        && physical
            .fields()
            .iter()
            .zip(&fields)
            .all(|(owned, assembled)| {
                owned.name() == assembled.name()
                    && owned.is_nullable() == assembled.is_nullable()
                    && owned.data_type().equals_datatype(assembled.data_type())
            });
    if !aligned {
        return Err(ScribeError::FingerprintMismatch {
            table: format!("{}.{}", definition.namespace, definition.name),
        });
    }
    // The arrays are the writer's; the identity on them is the table's. Every
    // column is re-typed onto the physical field it fills, so the stamped
    // batch carries the table layer's sensitivity tags whether or not the
    // writer echoed them.
    let columns = physical
        .fields()
        .iter()
        .zip(columns)
        .map(|(field, column)| {
            crate::tables::restamp_field_identity(column.as_ref(), field.data_type())
                .map_err(|_| ScribeError::InvalidFrame)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let stamped = RecordBatch::try_new(physical, columns).map_err(|_| ScribeError::InvalidFrame)?;
    if definition.canonical_validator.is_some() {
        enforce_canonical_physical_identity(&stamped, definition)?;
    }
    Ok(stamped)
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
/// `[receipt_micros − past, receipt_micros + future]`. A `past_exempt` table
/// drops the lower bound and keeps the future bound, so an aged backlog stays
/// publishable while a forward clock skew is still refused. Returns
/// [`ScribeError::Internal`] when the column cannot be downcast to
/// `TimestampMicrosecondArray` (which would imply a caller error on the
/// projected path that bypassed type validation).
fn enforce_event_time_window(
    rows: &RecordBatch,
    window: EventTimeWindow,
    receipt_micros: i64,
    past_exempt: bool,
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
    let lo = if past_exempt {
        i64::MIN
    } else {
        receipt_micros.saturating_sub(past_micros)
    };
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

/// Reports whether `name` is excluded from the user projection.
///
/// Every managed column is unconditionally excluded from the user projection
/// and materialized in its declared slot by [`managed_arrays`], regardless of
/// whether the caller supplied it. In particular `wyrd_event_time` is always
/// excluded: when the caller supplied a valid value it is reinserted verbatim
/// in its slot, and when it is absent the server stamps the admission instant
/// there. `run_id` is excluded for the same reason, so the canonical nullable
/// physical column is stamped exactly once. `card_ref` is a Gate input that
/// resolves to `card_uid` and is never stored.
fn is_server_owned(name: &str) -> bool {
    name == CARD_REF || is_managed_column(name)
}

/// Returns the caller's user fields, in order, without server-owned columns.
fn user_fields(rows: &RecordBatch) -> Vec<Field> {
    rows.schema()
        .fields()
        .iter()
        .filter(|field| !is_server_owned(field.name()))
        .map(|field| field.as_ref().clone())
        .collect()
}

/// Returns the caller's user columns, in order, without server-owned columns.
fn user_columns(rows: &RecordBatch) -> Vec<ArrayRef> {
    rows.schema()
        .fields()
        .iter()
        .zip(rows.columns())
        .filter(|(field, _)| !is_server_owned(field.name()))
        .map(|(_, column)| Arc::clone(column))
        .collect()
}

/// Resolves row card references against the writer's Card scope.
///
/// A null row reference resolves to no UID and needs no scope, so an entirely
/// uncorrelated batch resolves without consulting it. A present reference is
/// matched by exact identity — kind, space, name, and version — against
/// `scope`, and only the UID carried on that same member is stamped: the UID
/// the mint signed for a Card-bound principal, or the registry UID Gate
/// resolved for an unbound writer. Each member stamps its own UID; a
/// principal's root is never substituted for another identity, and a
/// client-supplied UID is never trusted. Resolution performs no IO.
///
/// # Errors
///
/// Returns [`ScribeError::CardUnresolved`] when the card column has the wrong
/// type, or when a row supplies a present reference and there is no scope, the
/// reference is malformed, its identity lies outside the scope, or the
/// matching member carries no UID.
pub(crate) fn resolve_card_uids(
    rows: &RecordBatch,
    scope: Option<&CardRefScope>,
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
    let mut resolved = Vec::with_capacity(row_count);
    for row in 0..row_count {
        resolved.push({
            if cards.is_null(row) {
                None
            } else {
                let scope = scope.ok_or(ScribeError::CardUnresolved)?;
                let raw = cards.value(row);
                let card = CardRef::from_str(raw).map_err(|_| ScribeError::CardUnresolved)?;
                if !scope.authorizes(&card) {
                    return Err(ScribeError::CardUnresolved);
                }
                let member = scope
                    .as_slice()
                    .iter()
                    .find(|member| member.same_identity(&card))
                    .ok_or(ScribeError::CardUnresolved)?;
                Some(
                    member
                        .uid
                        .as_ref()
                        .ok_or(ScribeError::CardUnresolved)?
                        .to_string(),
                )
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

/// Reports whether this write appends the universal correlation envelope.
///
/// The envelope is the default: a dynamic table has no declaration to consult
/// and keeps it. A built-in that declares [`CorrelationPolicy::None`] carries
/// its own identity columns instead, and its registered physical schema has no
/// envelope slots — so stamping one would seal an object wider than the table
/// it is promoted into.
fn correlation_envelope_applies(context: &DecodeContext<'_>) -> bool {
    context.definition.is_none_or(|definition| {
        definition.correlation_policy != crate::tables::CorrelationPolicy::None
    })
}

/// The per-batch values the managed columns are stamped from.
struct ManagedValues<'a> {
    /// Authenticated writer stamped as `principal_id`.
    principal: &'a Principal,
    /// Card scope each row's `card_ref` resolves its `card_uid` against.
    card_scope: Option<&'a CardRefScope>,
    /// Request identity stamped as `wyrd_request_id`.
    request_id: &'a RequestId,
    /// Admission instant stamped as `wyrd_ingested_at`.
    receipt_micros: i64,
    /// Caller `run_id` column lifted out of the user projection, if supplied.
    caller_run_id: Option<ArrayRef>,
    /// Validated caller `wyrd_event_time` column, if supplied.
    caller_event_time: Option<ArrayRef>,
}

/// Builds one array per managed column `policy` appends, in declared order.
///
/// The layout — which columns, in which order, with which type — comes from
/// [`MANAGED_COLUMNS`](crate::tables::managed_columns::MANAGED_COLUMNS); this
/// function supplies only each column's values. `wyrd_event_time` is the
/// caller's validated array verbatim when supplied, otherwise the admission
/// instant, and `wyrd_ingested_at` is always the admission instant.
///
/// # Errors
///
/// Returns [`ScribeError::CardUnresolved`] when a `card_ref` cannot be resolved
/// against the principal's signed scope, and [`ScribeError::Internal`] when a
/// declared managed column has no stamping rule here.
fn managed_arrays(
    rows: &RecordBatch,
    values: &ManagedValues<'_>,
    policy: CorrelationPolicy,
) -> Result<Vec<ArrayRef>, ScribeError> {
    let row_count = rows.num_rows();
    let admitted_at = Arc::new(
        TimestampMicrosecondArray::from(vec![values.receipt_micros; row_count])
            .with_timezone("UTC"),
    ) as ArrayRef;
    policy
        .managed_columns()
        .map(|column| {
            Ok(match column.field.name {
                RUN_ID => values.caller_run_id.as_ref().map_or_else(
                    || Arc::new(StringArray::from(vec![None::<&str>; row_count])) as ArrayRef,
                    Arc::clone,
                ),
                CARD_UID => Arc::new(StringArray::from(resolve_card_uids(
                    rows,
                    values.card_scope,
                    row_count,
                )?)),
                PRINCIPAL_ID => Arc::new(StringArray::from(vec![
                    values.principal.id.to_string();
                    row_count
                ])),
                WYRD_REQUEST_ID => Arc::new(StringArray::from(vec![
                    values.request_id.as_str();
                    row_count
                ])),
                WYRD_EVENT_TIME => values
                    .caller_event_time
                    .as_ref()
                    .map_or_else(|| Arc::clone(&admitted_at), Arc::clone),
                WYRD_INGESTED_AT => Arc::clone(&admitted_at),
                other => {
                    return Err(ScribeError::Internal {
                        detail: format!("managed column {other} has no stamping rule"),
                    });
                }
            })
        })
        .collect()
}

/// Closed set of CPU work permitted after admission has returned.
#[derive(Debug)]
pub(crate) enum ScribePersistenceCpuOp {
    /// Encode one frozen generation into durable staged runs.
    StageMember(Box<StageMemberOp>),
    /// Merge one claim's staged runs into rolling sealed objects.
    AssembleClaim(Box<AssembleClaimOp>),
    RestoreReplay {
        replayed: Box<ReplayedSealKey>,
    },
    /// Test-only stalled work proving a detached job retains its root owner.
    ///
    /// Boxed like every sibling so this test-only variant does not set the
    /// size of the operation every production submission moves through.
    #[cfg(test)]
    HoldMemory(Box<HoldMemoryOp>),
}

/// Move-only inputs for the test-only stalled-ownership persistence job.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct HoldMemoryOp {
    /// Root-backed bytes that must remain charged through job completion.
    memory: crate::resources::ScribeMemoryLease,
    /// Deterministic signal emitted after the detached job owns the lease.
    started: std::sync::mpsc::SyncSender<()>,
    /// Deterministic release gate controlled by the cancellation test.
    release: std::sync::mpsc::Receiver<()>,
}

/// Move-only inputs for staging one frozen generation onto the staging volume.
///
/// Staging is the blocking half of the durable boundary the WAL retires
/// against, so it runs on the same bounded lane as Parquet encoding rather
/// than on a runtime thread: the rows are sorted, encoded, fsynced, and
/// preflighted before the operation returns.
#[derive(Debug)]
pub(crate) struct StageMemberOp {
    /// Frozen generation staged without retaining the shard actor.
    pub(crate) frozen: Box<FrozenMemtable>,
    /// Tenant-qualified physical table binding.
    pub(crate) binding: TenantTableBinding,
    /// Registered physical write recipe resolved before the lane was entered.
    pub(crate) layout: std::sync::Arc<crate::catalog::layout::PhysicalLayout>,
    /// Shard, epoch, node, and WAL facts describing where the rows came from.
    pub(crate) origin: crate::scribe::member_stager::StagedMemberOrigin,
    /// Scribe capability charged for the batches the writer materializes.
    pub(crate) memory: crate::resources::ScribeResources,
    /// Pod-level owner of the staged namespace the runs are written into.
    pub(crate) staging: std::sync::Arc<crate::scribe::staging_runtime::ScribeStagingRuntime>,
}

/// Move-only inputs for merging one claim's staged runs into sealed objects.
///
/// The merge decodes one bounded batch per member at a time, which is CPU and
/// allocation bound rather than IO bound, so it belongs on the same lane as
/// encoding rather than on a runtime thread serving other shards.
#[derive(Debug)]
pub(crate) struct AssembleClaimOp {
    /// Claim whose members are being merged.
    pub(crate) claim: Box<crate::scribe::assembly::StagingClaim>,
    /// Runs gathered and re-validated for that claim.
    pub(crate) runs: Box<crate::scribe::claim_assembly::ClaimRuns>,
    /// Claim-owned output directory receiving the sealed objects.
    pub(crate) scratch_dir: std::path::PathBuf,
    /// Scribe capability charged for the batches the writer materializes.
    pub(crate) memory: crate::resources::ScribeResources,
    /// Pod-level owner holding the schema and layout the claim merges under.
    pub(crate) staging: std::sync::Arc<crate::scribe::staging_runtime::ScribeStagingRuntime>,
}

/// Results produced by [`ScribePersistenceCpuPool`].
#[derive(Debug)]
pub(crate) enum ScribePersistenceCpuResult {
    ReplayRestored(Box<FrozenMemtable>),
    MemberStaged(Box<crate::scribe::member_stager::StagedRuns>),
    ClaimAssembled(Box<crate::scribe::claim_assembly::AssembledClaim>),
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
) -> Result<ScribePersistenceCpuResult, ScribeError> {
    match operation {
        ScribePersistenceCpuOp::StageMember(operation) => stage_member(*operation),
        ScribePersistenceCpuOp::AssembleClaim(operation) => assemble_claim(*operation),
        ScribePersistenceCpuOp::RestoreReplay { replayed } => {
            let frozen = crate::scribe::memtable::Memtable::decode_replayed(&replayed)?;
            Ok(ScribePersistenceCpuResult::ReplayRestored(Box::new(frozen)))
        }
        #[cfg(test)]
        ScribePersistenceCpuOp::HoldMemory(held) => {
            let HoldMemoryOp {
                memory,
                started,
                release,
            } = *held;
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

/// Encodes one frozen generation into durable staged runs.
///
/// The context the runs are recorded under is the one they are encoded with,
/// so a later merge of this member cannot pick up a schema or sort order the
/// rows were not written against.
///
/// # Errors
///
/// Returns the staging failure when the member directory, encoding, fsync,
/// preflight, or durable byte admission refuses the member.
fn stage_member(operation: StageMemberOp) -> Result<ScribePersistenceCpuResult, ScribeError> {
    let StageMemberOp {
        frozen,
        binding,
        layout,
        origin,
        memory,
        staging,
    } = operation;
    let context = crate::scribe::staging_runtime::ClaimContext {
        schema: std::sync::Arc::clone(&frozen.schema),
        layout: (*layout).clone(),
        binding: binding.clone(),
    };
    staging
        .encode_member(
            crate::scribe::member_stager::StageMemberRequest {
                frozen: &frozen,
                binding: &binding,
                layout: &layout,
                origin,
                memory,
            },
            context,
        )
        .map(|staged| ScribePersistenceCpuResult::MemberStaged(Box::new(staged)))
}

/// Merges one claim's staged runs into rolling sealed objects.
///
/// # Errors
///
/// Returns the assembly failure when a run cannot be decoded, the writer
/// refuses a batch, or the merge writes a different number of rows than the
/// claim's members promised.
fn assemble_claim(operation: AssembleClaimOp) -> Result<ScribePersistenceCpuResult, ScribeError> {
    let AssembleClaimOp {
        claim,
        runs,
        scratch_dir,
        memory,
        staging,
    } = operation;
    staging
        .assemble(
            &claim,
            crate::scribe::staging_runtime::AssembleRequest {
                runs: &runs,
                scratch_dir: &scratch_dir,
                memory,
            },
        )
        .map(|assembled| ScribePersistenceCpuResult::ClaimAssembled(Box::new(assembled)))
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
    capacity: usize,
}

impl ScribePersistenceCpuPool {
    /// Closes admission to the persistence CPU lane without waiting for active Rayon work.
    pub(crate) fn close(&self) {
        self.permits.close();
    }
    /// Builds a persistence CPU lane with the fixed test queue bound.
    ///
    /// Production boot always states its own bound through
    /// [`Self::try_new_with_capacity`]; only bounded tests want a default.
    ///
    /// # Panics
    ///
    /// Panics if the fixed Rayon pool cannot be constructed.
    #[cfg(test)]
    pub(crate) fn new(worker_count: usize) -> Self {
        Self::new_with_capacity(worker_count, 64)
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
        let capacity = capacity.max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count.max(1))
            .thread_name(|index| format!("wyrd-scribe-persistence-cpu-{index}"))
            .build()?;
        register_lane_gauges("persistence");
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

    /// Returns how many Rayon threads run this lane's work.
    #[must_use]
    pub fn thread_count(&self) -> usize {
        self.pool.current_num_threads()
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
        let (sender, receiver) = oneshot::channel();
        depth.fetch_add(1, Ordering::AcqRel);
        record_lane_enqueued("persistence");
        let active = Arc::clone(&self.active);
        self.pool.spawn_fifo(move || {
            let started = std::time::Instant::now();
            active.fetch_add(1, Ordering::AcqRel);
            record_lane_started("persistence");
            let result = catch_unwind(AssertUnwindSafe(|| {
                execute_persistence_operation(operation)
            }));
            depth.fetch_sub(1, Ordering::AcqRel);
            active.fetch_sub(1, Ordering::AcqRel);
            record_lane_finished("persistence");
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

    /// Wait until every job submitted to this lane has reached a terminal.
    ///
    /// Shutdown uses this to establish that no worker thread is still touching
    /// WAL state, resource leases, or lane metrics before the owning Scribe
    /// releases them. Delegates to [`wait_lane_drained`], which registers for
    /// the drain edge before sampling the counter so an already-idle lane
    /// cannot be missed.
    pub(crate) async fn drain(&self) {
        wait_lane_drained(&self.depth, &self.drained).await;
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
    /// Writes one current ingress slice while retaining its complete root owner.
    WriteIngressSlice {
        /// Fixed shard WAL handle receiving the prepared slice.
        wal: WalHandle,
        /// Move-only current IPC payload and framing facts.
        append: PreparedWalAppend,
        /// Root lease retained until the detached WAL write returns.
        memory: crate::resources::ScribeMemoryLease,
    },
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
        reason = "manifest replacement persistence submits through this closed operation"
    )]
    ReplaceManifest { path: PathBuf, contents: Bytes },
    AdvanceManifest {
        path: PathBuf,
        stream: StreamIdentity,
        seal_key: SealKey,
        sealed_lsn: crate::scribe::wal::WalLsn,
    },
    /// Replays one recovery inventory while pinning the current segment in the
    /// existing WAL owner; an error or cancellation retains unread source bytes.
    ReplayDirectoryStream {
        path: PathBuf,
        /// Existing WAL writer whose retirement references pin the segment being read.
        wal: Arc<crate::scribe::wal::WalWriter>,
        /// Replacement actor stream whose lower same-node epochs are eligible.
        recovery_stream: StreamIdentity,
        shard_senders: Vec<mpsc::Sender<crate::scribe::shards::ShardCommand>>,
        memory: ScribeResources,
        /// Cooperative shutdown flag checked between replay records and settlements.
        cancelled: Arc<AtomicBool>,
    },
    RetireWal {
        wal: WalHandle,
        segments: Vec<WalSegmentRef>,
    },
}

/// Results produced by [`ScribeWalIoPool`].
#[derive(Debug)]
pub(crate) enum ScribeWalIoResult {
    /// One ingress slice was accepted by its WAL owner.
    IngressSliceWritten {
        /// Durable WAL append identity.
        result: WalAppendResult,
        /// Root lease returned after detached WAL work completed.
        memory: crate::resources::ScribeMemoryLease,
    },
    WalWritten {
        result: WalAppendResult,
    },
    WalSynced,
    Completed,
    /// Reports a fully settled in-worker replay after each manifest was durably
    /// advanced before generation retirement and all safe reader pins closed.
    ReplayStreamCompleted {
        /// Number of restored immutable generations.
        restored: usize,
        /// Maximum completed retirements awaiting settlement at one time.
        retirement_high_water: usize,
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
    /// Sync operations submitted through the production lane in unit tests.
    #[cfg(test)]
    sync_submissions: Arc<AtomicU64>,
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
        register_lane_gauges("wal_io");
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
            #[cfg(test)]
            sync_submissions: Arc::new(AtomicU64::new(0)),
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
        #[cfg(test)]
        if matches!(&operation, ScribeWalIoOp::SyncWal { .. }) {
            self.sync_submissions.fetch_add(1, Ordering::AcqRel);
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
        record_lane_enqueued("wal_io");
        let active = Arc::clone(&self.active);
        self.pool.spawn_fifo(move || {
            let started = std::time::Instant::now();
            active.fetch_add(1, Ordering::AcqRel);
            record_lane_started("wal_io");
            let result = catch_unwind(AssertUnwindSafe(|| execute_wal_io(operation, delay)));
            depth.fetch_sub(1, Ordering::AcqRel);
            active.fetch_sub(1, Ordering::AcqRel);
            record_lane_finished("wal_io");
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

    /// Wait until every job submitted to this lane has reached a terminal.
    ///
    /// Shutdown uses this to establish that no worker thread is still touching
    /// WAL state, resource leases, or lane metrics before the owning Scribe
    /// releases them. Delegates to [`wait_lane_drained`], which registers for
    /// the drain edge before sampling the counter so an already-idle lane
    /// cannot be missed.
    pub(crate) async fn drain(&self) {
        wait_lane_drained(&self.depth, &self.drained).await;
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

    /// Returns sync submissions observed at the production WAL lane boundary.
    #[cfg(test)]
    pub(crate) fn sync_submissions_for_test(&self) -> u64 {
        self.sync_submissions.load(Ordering::Acquire)
    }
}

/// Executes one detached WAL operation while retaining its admitted owners.
///
/// # Errors
///
/// Returns the WAL append, synchronization, or retirement error produced by
/// the selected operation.
fn execute_wal_io(
    operation: ScribeWalIoOp,
    delay: std::time::Duration,
) -> Result<ScribeWalIoResult, ScribeError> {
    match operation {
        ScribeWalIoOp::WriteIngressSlice {
            wal,
            append,
            memory,
        } => {
            let result = wal.append_prepared(append)?;
            Ok(ScribeWalIoResult::IngressSliceWritten { result, memory })
        }
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
            wal,
            recovery_stream,
            shard_senders,
            memory,
            cancelled,
        } => execute_replay_directory_stream(
            &path,
            wal.as_ref(),
            recovery_stream,
            &shard_senders,
            &memory,
            &cancelled,
        ),
        ScribeWalIoOp::RetireWal { wal, segments } => {
            wal.retire_segments(&segments)?;
            Ok(ScribeWalIoResult::Completed)
        }
    }
}

/// Replays one bounded WAL inventory and synchronously settles each shard handoff.
///
/// Cancellation is checked before every new shard send. Once sent, the response
/// is awaited before cancellation may terminate recovery, preserving one owner
/// and deterministic settlement.
///
/// # Errors
///
/// Returns [`ScribeError`] for discovery, admission, corruption, cancellation,
/// channel closure, or shard restoration failure.
fn execute_replay_directory_stream(
    path: &Path,
    wal: &crate::scribe::wal::WalWriter,
    recovery_stream: StreamIdentity,
    shard_senders: &[mpsc::Sender<crate::scribe::shards::ShardCommand>],
    memory: &ScribeResources,
    cancelled: &AtomicBool,
) -> Result<ScribeWalIoResult, ScribeError> {
    let mut restored = 0_usize;
    let mut retirement_settlement = ReplayRetirementSettlement::new(path);
    crate::scribe::replay::replay_wal_directory_stream_accounted(
        path,
        Some(recovery_stream),
        Some(memory),
        Some(wal),
        Some(cancelled),
        |chunk| {
            if cancelled.load(Ordering::Acquire) {
                return Err(ScribeError::Internal {
                    detail: "WAL replay cancelled".to_owned(),
                });
            }
            let shard = chunk
                .states
                .values()
                .next()
                .map(|state| usize::from(state.shard_id))
                .ok_or_else(|| ScribeError::Internal {
                    detail: "replay chunk omitted reconstructed state".to_owned(),
                })?;
            if chunk
                .states
                .values()
                .any(|state| usize::from(state.shard_id) != shard)
            {
                return Err(ScribeError::Internal {
                    detail: "replay chunk crossed recorded shard lanes".to_owned(),
                });
            }
            let state_count = chunk.states.len();
            let (response, receiver) = tokio::sync::oneshot::channel();
            // A lane recorded under an earlier shard count maps onto the
            // running one; the owner retains the replayed segments by path.
            shard_senders[shard % shard_senders.len()]
                .blocking_send(crate::scribe::shards::ShardCommand::Replay {
                    chunk: Box::new(chunk),
                    response,
                })
                .map_err(|_| ScribeError::Internal {
                    detail: "replay owner dropped its command channel".to_owned(),
                })?;
            let outcome = receiver
                .blocking_recv()
                .map_err(|_| ScribeError::Internal {
                    detail: "replay owner dropped its completion response".to_owned(),
                })??;
            for retirement in &outcome.retirements {
                retirement_settlement.settle(retirement)?;
            }
            restored = restored.saturating_add(state_count);
            Ok(crate::scribe::replay::ReplayChunkSettlement {
                identity_memory: outcome.identity_memory,
            })
        },
    )?;
    Ok(ScribeWalIoResult::ReplayStreamCompleted {
        restored,
        retirement_high_water: retirement_settlement.high_water(),
    })
}

/// Settles one completed replay publication before accepting another response.
struct ReplayRetirementSettlement<'a> {
    /// Root containing the recovered stream manifests.
    wal_dir: &'a Path,
    /// Completed retirements currently awaiting durable settlement.
    pending: usize,
    /// Maximum pending count observed during this replay operation.
    high_water: usize,
}

impl<'a> ReplayRetirementSettlement<'a> {
    /// Creates an empty settlement owner for one WAL root.
    #[must_use]
    const fn new(wal_dir: &'a Path) -> Self {
        Self {
            wal_dir,
            pending: 0,
            high_water: 0,
        }
    }

    /// Durably advances one replay watermark before releasing its generation references.
    ///
    /// The replay reader pins its current segment independently, so releasing a
    /// completed generation cannot unlink a segment that still has unread records.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when manifest publication or WAL retirement fails.
    fn settle(
        &mut self,
        retirement: &crate::scribe::shards::ReplayRetirement,
    ) -> Result<(), ScribeError> {
        self.pending = self.pending.saturating_add(1);
        self.high_water = self.high_water.max(self.pending);
        let manifest_path =
            crate::scribe::replay::stream_directory(self.wal_dir, retirement.stream)
                .join("manifest");
        let result = (|| {
            let mut manifest = crate::scribe::manifest::read_manifest(&manifest_path)?
                .unwrap_or_else(|| crate::scribe::manifest::Manifest::new(retirement.stream));
            manifest.update_sealed_lsn(&retirement.seal_key, retirement.sealed_lsn);
            replace_manifest(&manifest_path, &manifest.serialize()?)?;
            retirement.wal.retire_segments(&retirement.segments)
        })();
        self.pending = self.pending.saturating_sub(1);
        result
    }

    /// Returns the maximum number of completed retirements retained simultaneously.
    #[must_use]
    const fn high_water(&self) -> usize {
        self.high_water
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

    use arrow::array::{Array, ArrayRef, Int64Array, StringArray, TimestampMicrosecondArray};
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
        CARD_REF, CARD_UID, PRINCIPAL_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT, WYRD_REQUEST_ID,
    };

    use bytes::Bytes;

    use super::{
        DecodeContext, ReplayRetirementSettlement, ScribeIngressCpuPool, ScribePersistenceCpuOp,
        ScribePersistenceCpuPool, ScribeWalIoPool, decode, record_lane_saturation,
        source_schema_fingerprint, stamp_correlation_columns, wait_lane_drained,
    };
    use crate::contracts::{IngressPayload, ScribeError};
    use crate::resources::{BifrostRole, BifrostRuntimeResources, ScribeMemoryCategory};
    use crate::schema::SchemaFingerprint;
    use crate::scribe::admission::EventTimeWindow;
    use crate::scribe::replay::ReplayedSealKey;
    use crate::scribe::seal_key::SealKey;
    use crate::scribe::stream_identity::StreamIdentity;
    use crate::tables::CorrelationPolicy;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Notify;

    fn principal() -> Principal {
        Principal::new(
            PrincipalId::new(Uuid::now_v7()),
            PrincipalKind::User,
            crate::test_support::tenant(),
            Vec::new(),
            PermissionSet::new(),
        )
    }

    /// Build the default stamping context these unit cases decode under.
    ///
    /// Every field but the principal and request identity is the production
    /// default.
    fn stamping_context<'a>(
        principal: &'a Principal,
        request_id: &'a RequestId,
    ) -> super::DecodeContext<'a> {
        super::DecodeContext {
            definition: None,
            principal,
            card_scope: principal.card_ref_scope(),
            expected_schema_fingerprint: SchemaFingerprint([0_u8; 32]),
            request_id,
            window: EventTimeWindow::default(),
            receipt_micros: receipt_now(),
            registered_schema: None,
        }
    }

    /// The current admission instant, for fixtures whose event times are
    /// derived from the same clock.
    ///
    /// # Panics
    ///
    /// Panics when the system clock cannot be read as microseconds since the epoch.
    fn receipt_now() -> i64 {
        super::current_receipt_micros().expect("the receipt clock is readable")
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
        let stamped =
            stamp_correlation_columns(&rows, &stamping_context(&principal(), &RequestId::now_v7()))
                .expect("stamp");
        let value_index = stamped.schema().index_of("value").expect("value column");
        assert!(Arc::ptr_eq(&value, stamped.column(value_index)));
    }

    /// Native stamping writes the complete canonical physical schema.
    ///
    /// # Panics
    ///
    /// Panics when stamping fails, when the stamped batch has no nullable all-null
    /// `run_id`, or when its fingerprint differs from the canonical managed schema
    /// for the same user field.
    #[test]
    fn native_stamping_materializes_nullable_run_id() {
        let rows = batch(
            vec![Field::new("value", DataType::Int64, false)],
            vec![Arc::new(Int64Array::from(vec![1_i64, 2_i64]))],
        );

        let stamped =
            stamp_correlation_columns(&rows, &stamping_context(&principal(), &RequestId::now_v7()))
                .expect("stamp native batch");

        let run_id = stamped.schema().index_of("run_id").expect("run_id field");
        assert!(stamped.schema().field(run_id).is_nullable());
        assert_eq!(stamped.column(run_id).null_count(), stamped.num_rows());
        let expected = Schema::new(crate::tables::managed_columns::ensure_managed_columns(
            vec![Field::new("value", DataType::Int64, false)],
            CorrelationPolicy::Observation,
        ));
        assert_eq!(
            SchemaFingerprint::from_arrow_schema(stamped.schema().as_ref()),
            SchemaFingerprint::from_arrow_schema(&expected),
        );
    }

    /// Stamping writes the one admission instant as `wyrd_ingested_at` on every
    /// row and writes no per-row batch column.
    ///
    /// # Panics
    ///
    /// Panics when the stamped batch carries `wyrd_batch_id`, or when any row's
    /// ingestion time differs from the admission instant in the context.
    #[test]
    fn stamping_writes_the_admission_instant_and_no_batch_column() {
        /// Fixed admission instant, distinct from any live clock reading, so every
        /// stamped row can be compared against one exact value.
        const ADMITTED_AT: i64 = 1_700_000_000_123_456;
        let rows = batch(
            vec![Field::new("value", DataType::Int64, false)],
            vec![Arc::new(Int64Array::from(vec![1_i64, 2_i64]))],
        );
        let principal = principal();
        let request_id = RequestId::now_v7();
        let mut context = stamping_context(&principal, &request_id);
        context.receipt_micros = ADMITTED_AT;

        let stamped = stamp_correlation_columns(&rows, &context).expect("stamp");

        assert!(
            stamped.schema().index_of("wyrd_batch_id").is_err(),
            "rows carry no per-row batch identity"
        );
        let ingested = stamped
            .column_by_name(WYRD_INGESTED_AT)
            .expect("ingestion time is stamped")
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .expect("ingestion time is a microsecond timestamp");
        assert!(
            ingested.iter().all(|value| value == Some(ADMITTED_AT)),
            "every row carries the admission instant"
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

    /// An already-idle lane must settle the drain without needing a wake edge.
    ///
    /// The lane worker publishes `notify_waiters()` only on a terminal, so a
    /// drain that arrives after the last job finished will never see another
    /// edge. It has to observe the zeroed counter directly or hang.
    #[tokio::test]
    async fn drain_returns_immediately_when_lane_is_already_idle() {
        let depth = AtomicUsize::new(0);
        let drained = Notify::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            wait_lane_drained(&depth, &drained),
        )
        .await
        .expect("an idle lane must not park the drain");
    }

    /// A terminal published concurrently with the drain must still wake it.
    ///
    /// Covers the ordinary concurrent case: a worker reaches its terminal on
    /// another thread while the drain is waiting, and the drain settles rather
    /// than parking. This is coverage, **not** a lost-wakeup regression test —
    /// the lost-wakeup window in the pre-`enable` implementation spans only the
    /// few instructions between sampling `depth` and registering the waiter,
    /// and a sibling thread cannot be steered into it (this test passes against
    /// the unfixed implementation). The registration-before-sample ordering in
    /// [`wait_lane_drained`] rests on Tokio's documented `Notified::enable`
    /// contract, not on this test.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn drain_observes_terminal_published_concurrently() {
        for iteration in 0..2_000 {
            let depth = Arc::new(AtomicUsize::new(1));
            let drained = Arc::new(Notify::new());
            let worker_depth = Arc::clone(&depth);
            let worker_drained = Arc::clone(&drained);
            // Mirror a lane worker terminal exactly: decrement, then notify.
            let worker = std::thread::spawn(move || {
                worker_depth.fetch_sub(1, Ordering::AcqRel);
                worker_drained.notify_waiters();
            });
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                wait_lane_drained(&depth, &drained),
            )
            .await
            .unwrap_or_else(|_| {
                panic!("drain missed a concurrent terminal on iteration {iteration}")
            });
            worker.join().expect("lane terminal thread");
        }
    }

    /// Draining the production ingress pool settles after real submitted work.
    ///
    /// Exercises the drain through `ScribeIngressCpuPool::drain`, so the Rayon
    /// closure — not a hand-built counter — is what publishes the terminal,
    /// and asserts the lane's own counter reaches zero with it. Guards the
    /// production drain path against ordinary regressions such as a missing
    /// notify or a counter that never settles; it does not reach the narrow
    /// lost-wakeup interleaving.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn ingress_pool_drain_settles_after_submitted_work() {
        let pool = ScribeIngressCpuPool::new(2);
        for iteration in 0..500 {
            let job = pool.run(|| Ok::<_, crate::contracts::ScribeError>(()));
            let (job_result, drain_result) = tokio::join!(
                job,
                tokio::time::timeout(std::time::Duration::from_secs(5), pool.drain())
            );
            assert!(job_result.is_ok(), "lane job {iteration} must succeed");
            drain_result
                .unwrap_or_else(|_| panic!("ingress drain parked on iteration {iteration}"));
            assert_eq!(pool.snapshot().depth, 0);
        }
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
                .submit(ScribePersistenceCpuOp::HoldMemory(Box::new(
                    super::HoldMemoryOp {
                        memory,
                        started: started_tx,
                        release: release_rx,
                    },
                )))
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
            data_records: Vec::new(),
            append_metas: Vec::new(),
            wal_segments: Vec::new(),
            commits: Vec::new(),
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
            crate::test_support::day_partition(2026, 1, 1),
        )
    }

    /// Incremental replay settlement never retains more than one completed group.
    #[test]
    fn replay_retirement_settlement_has_one_pending_group() {
        let root = tempfile::tempdir().expect("WAL root");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let stream = StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1));
        let writer = crate::scribe::wal::WalWriter::new(
            root.path(),
            *node.as_bytes(),
            1,
            crate::scribe::wal::WalConfig::default(),
        )
        .expect("WAL writer");
        let wal = writer.handle_for_shard(0).expect("WAL handle");
        let stream_dir = crate::scribe::replay::stream_directory(root.path(), stream);
        std::fs::create_dir_all(&stream_dir).expect("stream directory");
        let seal_key = idle_seal_key();
        let mut settlement = ReplayRetirementSettlement::new(root.path());
        for index in 1_u64..=2 {
            let path = stream_dir.join(format!("retirement-{index}.wal"));
            std::fs::write(&path, index.to_le_bytes()).expect("retirement fixture");
            let segment = crate::scribe::wal::WalSegmentRef { path };
            wal.retain_segments(std::slice::from_ref(&segment))
                .expect("generation reference");
            let retirement = crate::scribe::shards::ReplayRetirement {
                wal: wal.clone(),
                segments: vec![segment.clone()],
                stream,
                seal_key: seal_key.clone(),
                sealed_lsn: crate::scribe::wal::WalLsn::new(index),
            };
            settlement.settle(&retirement).expect("settlement");
            assert!(!segment.path.exists(), "settled segment retires");
            settlement
                .settle(&retirement)
                .expect("settlement retry is idempotent");
        }
        assert_eq!(settlement.high_water(), 1);
        assert_eq!(
            crate::scribe::manifest::read_manifest(stream_dir.join("manifest"))
                .expect("manifest read")
                .expect("manifest")
                .get_sealed_lsn(&seal_key),
            Some(crate::scribe::wal::WalLsn::new(2))
        );
    }

    /// Projected payloads retain their caller-provided run identifier as the
    /// correlation value used by the projected-observation path.
    ///
    /// # Panics
    ///
    /// Panics when the projected payload is refused or when its `run_id` column is
    /// missing, not UTF-8, or not the caller's value.
    #[test]
    fn projected_run_id_remains_correlation_data() {
        let rows = batch(
            vec![Field::new("run_id", DataType::Utf8, false)],
            vec![Arc::new(StringArray::from(vec!["client-run"]))],
        );
        let error = decode(
            IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![
                rows.clone(),
            ])),
            &DecodeContext {
                principal: &principal(),
                card_scope: principal().card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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
            &DecodeContext {
                principal,
                card_scope: principal.card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(schema),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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
                card_ref: Some(card.clone()),
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

    /// A `card_ref` value outside the service principal's own Card scope is refused
    /// during decode, before any writer admission sees the frame.
    ///
    /// # Panics
    ///
    /// Panics when decode accepts the out-of-scope row or refuses it with anything
    /// other than `CardScopeDenied`.
    #[test]
    fn card_scope_rejects_before_writer_admission() {
        let card = CardRef::from_str("prod/Service/billing@1.0.0").expect("card");
        let principal = Principal::new(
            PrincipalId::new(Uuid::now_v7()),
            PrincipalKind::Service {
                card_ref: Some(card.clone()),
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
            IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![
                rows.clone(),
            ])),
            &DecodeContext {
                principal: &principal,
                card_scope: principal.card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
        )
        .expect_err("card outside scope");
        assert!(matches!(
            error,
            crate::contracts::ScribeError::CardScopeDenied
        ));
    }

    /// A duplicated Arrow column name is refused before any correlation or
    /// managed-column work, on a dynamic table with no built-in definition.
    ///
    /// `card_ref` is a permitted client correlation column rather than a
    /// server-owned one, so a duplicate pair is removed wholesale by the later
    /// projection while name lookup only ever sees the first occurrence. An
    /// authorized (here null) first column could therefore hide a second,
    /// out-of-scope assertion. The shared decode guard rejects the frame first.
    ///
    /// # Panics
    ///
    /// Panics when decode accepts the duplicated column or refuses it with anything
    /// other than `InvalidFrame`.
    #[test]
    fn duplicate_arrow_column_names_fail_closed_for_dynamic_tables() {
        let rows = batch(
            vec![
                Field::new(CARD_REF, DataType::Utf8, true),
                Field::new(CARD_REF, DataType::Utf8, true),
            ],
            vec![
                Arc::new(StringArray::from(vec![None::<&str>])),
                Arc::new(StringArray::from(vec![Some("prod/Service/other@1.0.0")])),
            ],
        );
        let error = decode(
            IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![
                rows.clone(),
            ])),
            &DecodeContext {
                principal: &principal(),
                card_scope: principal().card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
        )
        .expect_err("a duplicated column name fails closed before WAL");
        assert!(matches!(error, ScribeError::InvalidFrame));
    }

    /// Decode keeps the caller's field order and binds each value to its field by
    /// name, so a payload whose fields are not in alphabetical order keeps every
    /// value under its own name.
    ///
    /// # Panics
    ///
    /// Panics when decode refuses the payload, when the decoded fields change
    /// order, or when either column's value moves to the other field.
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
            IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![
                rows.clone(),
            ])),
            &DecodeContext {
                principal: &principal(),
                card_scope: principal().card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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

    /// Decode finishes schema validation and server stamping before
    /// acknowledgement: the decoded batch carries the principal, event-time,
    /// ingestion-time, and request columns, and neither a tenant column nor a
    /// per-row batch column.
    ///
    /// # Panics
    ///
    /// Panics when decode refuses the payload, when a stamped managed column is
    /// missing, or when a tenant or batch-identity column is present.
    #[test]
    fn ipc_decode_schema_type_validation_and_stamping_precede_ack() {
        let rows = batch(
            vec![Field::new("value", DataType::Int64, false)],
            vec![Arc::new(Int64Array::from(vec![1_i64]))],
        );
        let decoded = decode(
            IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![
                rows.clone(),
            ])),
            &DecodeContext {
                principal: &principal(),
                card_scope: principal().card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
        )
        .expect("schema is valid");
        assert!(decoded.schema().index_of("data_tenant_id").is_err());
        assert!(decoded.schema().index_of(PRINCIPAL_ID).is_ok());
        assert!(decoded.schema().index_of("wyrd_batch_id").is_err());
        assert!(decoded.schema().index_of(WYRD_EVENT_TIME).is_ok());
        assert!(decoded.schema().index_of(WYRD_INGESTED_AT).is_ok());
        assert!(decoded.schema().index_of(WYRD_REQUEST_ID).is_ok());
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
                card_ref: Some(card.clone()),
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
        Schema::new(crate::tables::managed_columns::ensure_managed_columns(
            vec![Field::new("value", DataType::Int64, false)],
            CorrelationPolicy::Observation,
        ))
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
    /// and diverged from [`ensure_managed_columns`], breaking every sealed read.
    ///
    /// # Panics
    ///
    /// Panics when the caller event time is refused or when the stamped field order
    /// differs from the canonical managed order.
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
            &DecodeContext {
                principal: &principal,
                card_scope: principal.card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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
    ///
    /// # Panics
    ///
    /// Panics when the payload is refused or when the stamped field order differs
    /// from the canonical managed order.
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
            &DecodeContext {
                principal: &principal,
                card_scope: principal.card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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
    ///
    /// # Panics
    ///
    /// Panics when the projected event time is refused or when the stamped field
    /// order differs from the canonical managed order.
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
            IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![
                rows.clone(),
            ])),
            &DecodeContext {
                principal: &principal(),
                card_scope: principal().card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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
    ///
    /// # Panics
    ///
    /// Panics when the caller event time is refused, when either time column is
    /// missing or not a microsecond timestamp, when the event time differs from the
    /// caller value, or when it equals the receipt time.
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
            &DecodeContext {
                principal: &principal,
                card_scope: principal.card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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
    ///
    /// # Panics
    ///
    /// Panics when the user-schema fingerprints differ, when the caller event time
    /// is refused, or when the decoded event-time column is missing, mistyped, or
    /// not the caller's values.
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
            &DecodeContext {
                principal: &principal,
                card_scope: principal.card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows_with.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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
    ///
    /// # Panics
    ///
    /// Panics when the payload is refused, when the server-stamped event time is
    /// missing, mistyped, or nullable, or when no ingestion time is stamped.
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
            &DecodeContext {
                principal: &principal,
                card_scope: principal.card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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
    ///
    /// # Panics
    ///
    /// Panics when any malformed event-time variant is accepted or refused with
    /// anything other than `InvalidFrame`.
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
                &DecodeContext {
                    principal: &principal,
                    card_scope: principal.card_ref_scope(),
                    expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                    request_id: &RequestId::now_v7(),
                    window: EventTimeWindow::default(),
                    receipt_micros: receipt_now(),
                    definition: None,
                    registered_schema: None,
                },
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
    ///
    /// # Panics
    ///
    /// Panics when the in-window value is refused or when the decoded event time is
    /// missing, mistyped, or not the caller value.
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
            &DecodeContext {
                principal: &principal,
                card_scope: principal.card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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
    ///
    /// # Panics
    ///
    /// Panics when a value exactly at the past bound is refused.
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
            &DecodeContext {
                principal: &principal,
                card_scope: principal.card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window,
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
        )
        .expect("past-edge value is accepted (inclusive bound)");
    }

    /// Value exactly at the future edge (receipt + future) is accepted (inclusive).
    ///
    /// # Panics
    ///
    /// Panics when a value exactly at the future bound is refused.
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
            &DecodeContext {
                principal: &principal,
                card_scope: principal.card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window,
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
        )
        .expect("future-edge value is accepted (inclusive bound)");
    }

    /// A value older than the past bound is rejected with `EventTimeOutOfRange`.
    /// The whole batch is refused; no partial write occurs.
    ///
    /// # Panics
    ///
    /// Panics when the value older than the past bound is not refused with
    /// `EventTimeOutOfRange`.
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
            &DecodeContext {
                principal: &principal,
                card_scope: principal.card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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
    ///
    /// # Panics
    ///
    /// Panics when the value beyond the future bound is not refused with
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
            &DecodeContext {
                principal: &principal,
                card_scope: principal.card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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
    ///
    /// # Panics
    ///
    /// Panics when the in-window projected value is refused or when the decoded
    /// event time is missing, mistyped, or not the caller value.
    #[test]
    fn projected_event_time_within_window_accepted() {
        let t = now_micros_offset(-3600);
        let (event_field, event_array) = managed_event_time(vec![t]);
        let rows = batch(
            vec![Field::new("value", DataType::Int64, false), event_field],
            vec![Arc::new(Int64Array::from(vec![1_i64])), event_array],
        );
        let decoded = decode(
            IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![
                rows.clone(),
            ])),
            &DecodeContext {
                principal: &principal(),
                card_scope: principal().card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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
    ///
    /// # Panics
    ///
    /// Panics when the out-of-range projected value is not refused with
    /// `EventTimeOutOfRange`.
    #[test]
    fn projected_event_time_out_of_range_rejected() {
        let old = now_micros_offset(-(31 * 24 * 60 * 60));
        let (event_field, event_array) = managed_event_time(vec![old]);
        let rows = batch(
            vec![Field::new("value", DataType::Int64, false), event_field],
            vec![Arc::new(Int64Array::from(vec![1_i64])), event_array],
        );
        let err = decode(
            IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![
                rows.clone(),
            ])),
            &DecodeContext {
                principal: &principal(),
                card_scope: principal().card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: EventTimeWindow::default(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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
    ///
    /// # Panics
    ///
    /// Panics when the payload is refused or when the server-stamped event time is
    /// missing, mistyped, or null.
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
            IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![
                rows.clone(),
            ])),
            &DecodeContext {
                principal: &principal(),
                card_scope: principal().card_ref_scope(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                window: tight_window,
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
            },
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
    ///
    /// # Panics
    ///
    /// Panics when a native payload supplying any other managed column is accepted
    /// or refused with anything other than `InvalidFrame`.
    #[test]
    fn native_other_reserved_columns_still_rejected() {
        let (principal, card) = scoped_service_principal();
        for reserved in [WYRD_INGESTED_AT, WYRD_REQUEST_ID, PRINCIPAL_ID] {
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
                &DecodeContext {
                    principal: &principal,
                    card_scope: principal.card_ref_scope(),
                    expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                    request_id: &RequestId::now_v7(),
                    window: EventTimeWindow::default(),
                    receipt_micros: receipt_now(),
                    definition: None,
                    registered_schema: None,
                },
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

    /// Resolve one card text into a reference carrying a freshly minted UID.
    ///
    /// Scope members are signed with their registry UID at mint time, so a test
    /// scope must model the same shape: identity plus a trusted UID.
    fn resolved_card(text: &str) -> CardRef {
        let card = CardRef::from_str(text).expect("scope member parses");
        CardRef {
            uid: Some(CardUid::new(Uuid::now_v7().to_string()).expect("card uid")),
            ..card
        }
    }

    /// Build a service principal whose signed scope is exactly `scope`.
    fn scoped_principal(root: &CardRef, scope: CardRefScope) -> Principal {
        Principal::new(
            PrincipalId::new(Uuid::now_v7()),
            PrincipalKind::Service {
                card_ref: Some(root.clone()),
                card_ref_scope: scope,
            },
            crate::test_support::tenant(),
            Vec::new(),
            PermissionSet::new(),
        )
    }

    /// How a single-row fixture carries its optional `card_ref` correlation.
    #[derive(Debug, Clone, Copy)]
    enum SuppliedCardRef<'a> {
        /// The batch has no `card_ref` column at all.
        Absent,
        /// The batch has the column and the row's value is null.
        Null,
        /// The batch has the column and the row carries this text.
        Text(&'a str),
    }

    /// Stamp one single-row batch carrying the supplied optional `card_ref`.
    fn stamp_card_ref(
        principal: &Principal,
        card_ref: SuppliedCardRef<'_>,
    ) -> Result<RecordBatch, ScribeError> {
        let mut fields = vec![Field::new("value", DataType::Int64, false)];
        let mut columns: Vec<ArrayRef> = vec![Arc::new(Int64Array::from(vec![1_i64]))];
        let value = match card_ref {
            SuppliedCardRef::Absent => None,
            SuppliedCardRef::Null => Some(None),
            SuppliedCardRef::Text(text) => Some(Some(text)),
        };
        if let Some(value) = value {
            fields.push(Field::new(CARD_REF, DataType::Utf8, true));
            columns.push(Arc::new(StringArray::from(vec![value])));
        }
        stamp_correlation_columns(
            &batch(fields, columns),
            &stamping_context(principal, &RequestId::now_v7()),
        )
    }

    /// Read the single stamped `card_uid` value of a one-row batch.
    fn stamped_card_uid(stamped: &RecordBatch) -> Option<String> {
        let index = stamped
            .schema()
            .index_of(CARD_UID)
            .expect("card_uid column");
        let uids = stamped
            .column(index)
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("card_uid is Utf8");
        (!uids.is_null(0)).then(|| uids.value(0).to_owned())
    }

    /// Optional and scoped Card correlation stamps only signed, trusted UIDs.
    ///
    /// # Panics
    ///
    /// Panics when an absent or null correlation does not stamp a null
    /// `card_uid`, when a scoped member does not stamp its own signed UID, when
    /// a client-supplied UID displaces the signed one, when the principal's
    /// root is substituted for an unrelated Card, or when a UID-less,
    /// malformed, or out-of-scope assertion is admitted.
    #[test]
    fn optional_and_scoped_card_correlations_stamp_trusted_uids() {
        let root = resolved_card("prod/Service/checkout@1.0.0");
        let secondary = resolved_card("prod/Agent/planner@2.0.0");
        let principal = scoped_principal(
            &root,
            CardRefScope::from_root_and_members(&root, [secondary.clone()]),
        );

        for (label, supplied) in [
            ("absent", SuppliedCardRef::Absent),
            ("explicit null", SuppliedCardRef::Null),
        ] {
            let stamped = stamp_card_ref(&principal, supplied)
                .unwrap_or_else(|_| panic!("{label} correlation is admitted"));
            assert_eq!(
                stamped_card_uid(&stamped),
                None,
                "{label} correlation stamps no card_uid"
            );
            let principal_index = stamped
                .schema()
                .index_of(PRINCIPAL_ID)
                .expect("principal_id column");
            let principals = stamped
                .column(principal_index)
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("principal_id is Utf8");
            assert_eq!(
                principals.value(0),
                principal.id.to_string(),
                "{label} correlation still stamps the exact principal"
            );
        }

        for member in [&root, &secondary] {
            let identity = CardRef {
                uid: None,
                ..member.clone()
            };
            let stamped = stamp_card_ref(&principal, SuppliedCardRef::Text(&identity.to_string()))
                .expect("a signed scope member is admitted");
            assert_eq!(
                stamped_card_uid(&stamped).as_deref(),
                member.uid.as_ref().map(CardUid::as_str),
                "{identity} stamps its own signed UID, never the root's"
            );
        }

        let forged = CardRef {
            uid: Some(CardUid::new(Uuid::now_v7().to_string()).expect("forged uid")),
            ..secondary.clone()
        };
        let stamped = stamp_card_ref(&principal, SuppliedCardRef::Text(&forged.to_string()))
            .expect("a client UID does not change the signed identity");
        assert_eq!(
            stamped_card_uid(&stamped).as_deref(),
            secondary.uid.as_ref().map(CardUid::as_str),
            "the signed UID wins over the client-supplied one"
        );

        let unsigned = CardRef::from_str("prod/Service/unsigned@1.0.0").expect("card");
        let unsigned_principal = scoped_principal(
            &root,
            CardRefScope::from_root_and_members(&root, [unsigned.clone()]),
        );
        for (label, principal, supplied) in [
            (
                "a UID-less signed membership",
                &unsigned_principal,
                unsigned.to_string(),
            ),
            (
                "malformed correlation text",
                &principal,
                "not-a-card-ref".to_owned(),
            ),
            (
                "an out-of-scope identity",
                &principal,
                "prod/Service/other@1.0.0".to_owned(),
            ),
        ] {
            let error = stamp_card_ref(principal, SuppliedCardRef::Text(&supplied))
                .expect_err("{label} fails closed");
            assert!(
                matches!(error, ScribeError::CardUnresolved),
                "{label} fails closed without registry IO"
            );
        }
    }

    /// The SYSTEM writer may correlate result rows only with its one signed
    /// Verifier, and Scribe stamps that Verifier's signed UID.
    ///
    /// The SYSTEM principal has no root Card, so its signed scope is the sole
    /// authority for row correlation: a row naming another Verifier is a
    /// forgery and refuses the frame before admission.
    ///
    /// # Panics
    ///
    /// Panics when the signed Verifier is refused or stamps a UID other than
    /// its signed one, or when a different Verifier is admitted by either the
    /// admission scope check or the stamping resolution.
    #[test]
    fn system_writer_correlates_only_its_signed_verifier() {
        let verifier = resolved_card("prod/Verifier/drift@1.0.0");
        let principal = Principal::new(
            PrincipalId::new(Uuid::now_v7()),
            PrincipalKind::System {
                card_ref_scope: CardRefScope::own(&verifier),
            },
            crate::test_support::tenant(),
            Vec::new(),
            PermissionSet::new(),
        );
        let identity = CardRef {
            uid: None,
            ..verifier.clone()
        }
        .to_string();

        let stamped = stamp_card_ref(&principal, SuppliedCardRef::Text(&identity))
            .expect("the signed Verifier is admitted");
        assert_eq!(
            stamped_card_uid(&stamped).as_deref(),
            verifier.uid.as_ref().map(CardUid::as_str),
        );

        let forged = "prod/Verifier/other@1.0.0";
        let rows = batch(
            vec![Field::new(CARD_REF, DataType::Utf8, true)],
            vec![Arc::new(StringArray::from(vec![Some(forged)]))],
        );
        assert!(matches!(
            super::validate_card_scope(&rows, principal.card_ref_scope()),
            Err(ScribeError::CardScopeDenied)
        ));
        assert!(matches!(
            stamp_card_ref(&principal, SuppliedCardRef::Text(forged)),
            Err(ScribeError::CardUnresolved)
        ));
    }
}
