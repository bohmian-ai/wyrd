//! Bounded CPU execution lanes used by Scribe admission.

use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use arrow::array::{Array, ArrayRef, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use iceberg::spec::Schema as IcebergSchema;
use tokio::sync::{Notify, Semaphore, mpsc, oneshot};

use crate::catalog::TenantTableBinding;
use crate::contracts::{IngressPayload, ScribeError};
use crate::resources::ScribeResources;
use crate::schema::fingerprint::SchemaFingerprint;
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::replay::ReplayedSealKey;
use crate::scribe::seal_key::SealKey;
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::{
    PreparedWalAppend, WalAppendResult, WalHandle, WalSegment, WalSegmentRef,
};
use std::str::FromStr;
use wyrd_runtime::Principal;
use wyrd_spec::ids::CardUid;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::managed_columns::{
    CARD_UID, PRINCIPAL_ID, RUN_ID, WYRD_INGESTED_AT, WYRD_REQUEST_ID,
};

use crate::tables::CorrelationPolicy;
use crate::tables::managed_columns::{
    ensure_managed_columns, is_managed_column, is_reserved_column,
};

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
    /// Conform plans shared by every decode this lane runs.
    conform: Arc<ConformPlans>,
}

impl ScribeIngressCpuPool {
    /// The conform plans this lane's decodes remember, shared with native
    /// sources that conform on the persistence lane.
    pub(crate) const fn conform_plans(&self) -> &Arc<ConformPlans> {
        &self.conform
    }

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
            conform: Arc::default(),
        })
    }

    /// Submit one native decode without waiting for an application queue slot.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the ingress queue is saturated,
    /// and propagates any error produced by the free [`decode`] function.
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
        let conform = Arc::clone(&self.conform);
        self.pool.spawn_fifo(move || {
            let started = std::time::Instant::now();
            active.fetch_add(1, Ordering::AcqRel);
            record_lane_started("ingress");
            let result = catch_unwind(AssertUnwindSafe(|| {
                decode(
                    payload,
                    &DecodeContext {
                        principal: &principal,
                        card_scope: card_scope.as_deref(),
                        expected_schema_fingerprint,
                        request_id: &request_id,
                        receipt_micros,
                        definition,
                        registered_schema: registered_schema.as_ref(),
                        conform: Some(&conform),
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
/// Native payloads may carry one nullable Utf8 `run_id` correlation field. The
/// table's time column is ordinary content the writer supplies. Every managed
/// column (`wyrd_ingested_at`, `wyrd_request_id`, principal, and card columns)
/// is unconditionally server-owned and is rejected when supplied.
///
/// A [`IngressPayload::Canonical`] payload is server-built: Gate's projection
/// already validated every Variant it encoded, and replay and the audit
/// publisher carry rows admitted that way. Its values are therefore trusted
/// and the built-in value validator is skipped; the reserved-column refusal
/// and the fingerprint check still bind its shape. A native IPC payload is
/// the trust boundary and is validated in full.
///
/// # Errors
/// Returns [`ScribeError::InvalidFrame`] for malformed IPC, a reserved
/// server-owned column supplied by the caller, a duplicate field, and for
/// failed physical assembly; returns [`ScribeError::FingerprintMismatch`]
/// or authorization errors when the admitted payload does not match the registered
/// table contract.
fn decode(
    payload: IngressPayload,
    context: &DecodeContext<'_>,
) -> Result<RecordBatch, ScribeError> {
    let trusted = matches!(payload, IngressPayload::Canonical(_));
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
    decode_rows(&rows, context, trusted)
}

/// Validates and stamps one current native record batch.
///
/// This entry point lets persistence preprocessing consume a native stream one
/// source at a time without collecting or concatenating its record batches.
///
/// # Errors
///
/// Returns the same schema, scope, row, and managed-column errors
/// as the ordinary ingress decode path.
pub(crate) fn decode_native_batch(
    rows: &RecordBatch,
    context: &DecodeContext<'_>,
) -> Result<RecordBatch, ScribeError> {
    decode_rows(rows, context, false)
}

/// The owned decode inputs one ingress frame hands to the bounded CPU lane.
///
/// The lane moves these into its worker, so they are owned rather than the
/// borrowed [`DecodeContext`] the decode itself assembles from them.
pub(crate) struct IngressDecodeInputs {
    /// Authenticated principal stamped as the writer.
    pub(crate) principal: Principal,
    /// Card UIDs every row's `card_uid` is authorized against: the frame's
    /// attributed UIDs, or the principal's signed scope.
    pub(crate) card_scope: Option<Vec<CardUid>>,
    /// Catalog fingerprint required of the caller-owned source schema.
    pub(crate) expected_schema_fingerprint: SchemaFingerprint,
    /// Stable request identity stamped into every accepted row.
    pub(crate) request_id: RequestId,
    /// Admission instant stamped as `wyrd_ingested_at`.
    pub(crate) receipt_micros: i64,
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
    /// Card UIDs every row's `card_uid` is authorized against: the frame's
    /// attributed UIDs, or the principal's signed scope.
    pub(crate) card_scope: Option<&'a [CardUid]>,
    /// Catalog fingerprint required of the caller-owned source schema.
    pub(crate) expected_schema_fingerprint: SchemaFingerprint,
    /// Stable request identity stamped into every accepted row.
    pub(crate) request_id: &'a RequestId,
    /// Admission instant in epoch microseconds, read once per batch and
    /// stamped as `wyrd_ingested_at`.
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
    pub(crate) registered_schema: Option<&'a Arc<IcebergSchema>>,
    /// The ingress lane's remembered conform plans; `None` builds the plan
    /// for every batch.
    pub(crate) conform: Option<&'a ConformPlans>,
}

/// Applies source-contract validation and server-managed stamping to one batch.
///
/// With a registered schema, the batch is first conformed to it
/// ([`conform_to_registered`]): columns are matched by name and put in the
/// registered order, equivalent spellings are cast to the registered type,
/// and a Variant identity mismatch is refused, so the writer's column order
/// and Arrow spelling never matter.
///
/// The final step stamps the registered Iceberg table's field ids onto every
/// field when the context carries that schema, so the WAL, staged runs, and
/// sealed objects all number their columns exactly as the table does.
///
/// `trusted` marks server-built rows; it is forwarded to
/// [`enforce_builtin_source_contract`], which then skips the value walk.
///
/// # Errors
///
/// Returns a stable Scribe refusal for a duplicated column name, reserved
/// columns, fingerprint mismatch, card-scope failure, or managed column
/// construction failure, a fingerprint mismatch
/// when a stamped field has no registered counterpart, and
/// [`ScribeError::ContractViolation`] carrying the catalogued error when a
/// built-in's declared Variant arrives without its extension, or when any
/// table's Variant holds a value that cannot be stored.
fn decode_rows(
    rows: &RecordBatch,
    context: &DecodeContext<'_>,
    trusted: bool,
) -> Result<RecordBatch, ScribeError> {
    let mut seen = std::collections::HashSet::with_capacity(rows.schema().fields().len());
    for field in rows.schema().fields() {
        // Duplicate names are refused for every table kind — built-in, dynamic,
        // and pre-declared — before fingerprint, correlation, or stamping work.
        // `card_uid` is a permitted client correlation column, so a duplicate
        // pair is removed wholesale by projection while name lookup only ever
        // resolves the first occurrence; an authorized first value would
        // otherwise hide a second, unauthorized assertion.
        if !seen.insert(field.name().as_str()) {
            return Err(ScribeError::InvalidFrame);
        }
        // `run_id` and `card_uid` are caller correlation that this function
        // relinquishes and restamps exactly once in their slots, `card_uid`
        // only after authorizing it. Every other reserved column is
        // unconditionally server-owned, so supplying one is a refusal rather
        // than an override.
        let name = field.name().as_str();
        if is_reserved_column(name) && name != RUN_ID && name != CARD_UID {
            return Err(ScribeError::InvalidFrame);
        }
    }
    let conformed;
    let rows = match context.registered_schema {
        Some(registered) => {
            conformed = match context.conform {
                Some(plans) => plans.conform(rows, registered)?,
                None => conform_to_registered(rows, registered)?,
            };
            &conformed
        }
        None => rows,
    };
    if let Some(definition) = context.definition {
        enforce_builtin_source_contract(rows, definition, trusted)?;
    }
    let actual_source_fingerprint = source_schema_fingerprint(rows.schema().as_ref());
    if actual_source_fingerprint != context.expected_schema_fingerprint {
        return Err(ScribeError::FingerprintMismatch {
            table: "resolved ingress table".to_owned(),
        });
    }
    if let (None, Some(registered)) = (context.definition, context.registered_schema) {
        enforce_dynamic_variants(rows, registered)?;
    }
    let stamped = stamp_correlation_columns(rows, context)?;
    match context.registered_schema {
        Some(registered) => crate::tables::stamp_registered_field_ids(&stamped, registered)
            .map_err(|_| ScribeError::FingerprintMismatch {
                table: "resolved ingress table".to_owned(),
            }),
        None => Ok(stamped),
    }
}

/// Conforms a batch to its table's registered Iceberg schema in one pass.
///
/// The registered Iceberg schema is the table's source of truth, so every
/// later check, the WAL, and storage see its Arrow form. Columns are matched
/// by name, never position, and put in the registered order; a column the
/// table does not register — a server-owned input such as `card_uid`, or a
/// column from a stale or wrong declaration — keeps its relative place after
/// the registered columns. Each matched column is then judged by
/// iceberg-rust's own Arrow conversion through
/// [`crate::tables::same_iceberg_type`]:
///
/// - the same Iceberg type already in the registered layout is kept exactly
///   as sent, field included, since the registered form differs only in
///   nested field metadata (Iceberg's field ids) that the layout ignores and
///   Arrow would refuse to relabel;
/// - the same Iceberg type in another Arrow spelling (`Int8` for `int`, a
///   `UTC` zone for `+00:00`, a list element named `item`) is cast once with
///   Arrow's `cast` to the registered type, keeping the sent field's name,
///   nullability, and metadata;
/// - a column whose storage matches but whose Variant identity does not — a
///   Struct sent for a Variant, a Variant sent for a Struct, or a Variant
///   marker with parameters, at any depth — is refused as an unsupported type
///   before any value is read, because the storage-only fingerprint cannot
///   see the extension;
/// - any other column is kept as sent, so the fingerprint check refuses it.
///
/// # Errors
///
/// Returns [`ScribeError::ContractViolation`] carrying
/// [`BifrostError::UnsupportedType`] for a Variant identity mismatch,
/// [`ScribeError::FingerprintMismatch`] when the registered schema has no
/// Arrow form or Arrow cannot cast an equivalent column, and
/// [`ScribeError::InvalidFrame`] when Arrow refuses the conformed batch, which
/// duplicate-free input cannot cause.
fn conform_to_registered(
    rows: &RecordBatch,
    registered: &IcebergSchema,
) -> Result<RecordBatch, ScribeError> {
    ConformPlan::build(&rows.schema(), registered)?.apply(rows)
}

/// How every batch of one sent Arrow schema conforms to one registered
/// Iceberg schema.
///
/// The decision is a pure function of the two schemas, so it is made once by
/// [`Self::build`] and then applied to each batch of that shape by
/// [`Self::apply`], which only moves or casts columns.
#[derive(Debug)]
struct ConformPlan {
    /// The conformed schema every applied batch carries.
    schema: SchemaRef,
    /// Each output column's sent index, with the registered type it is cast
    /// to when the sent spelling differs.
    columns: Vec<(usize, Option<DataType>)>,
}

impl ConformPlan {
    /// Decides the conformance of `sent` to `registered`, as documented on
    /// [`conform_to_registered`].
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::ContractViolation`] carrying
    /// [`BifrostError::UnsupportedType`] for a Variant identity mismatch and
    /// [`ScribeError::FingerprintMismatch`] when the registered schema has no
    /// Arrow form.
    fn build(sent: &Schema, registered: &IcebergSchema) -> Result<Self, ScribeError> {
        let target = iceberg::arrow::schema_to_arrow_schema(registered).map_err(|_| {
            ScribeError::FingerprintMismatch {
                table: "resolved ingress table".to_owned(),
            }
        })?;
        let registered_columns = target
            .fields()
            .iter()
            .filter_map(|wanted| Some((sent.index_of(wanted.name()).ok()?, Some(wanted))));
        let unregistered = (0..sent.fields().len())
            .filter(|&index| target.field_with_name(sent.field(index).name()).is_err())
            .map(|index| (index, None));
        let mut fields = Vec::with_capacity(sent.fields().len());
        let mut columns = Vec::with_capacity(sent.fields().len());
        for (index, wanted) in registered_columns.chain(unregistered) {
            let field = &sent.fields()[index];
            let cast = match wanted {
                Some(wanted) if crate::tables::same_iceberg_type(field, wanted) => {
                    (layout(field.data_type()) != layout(wanted.data_type()))
                        .then(|| wanted.data_type().clone())
                }
                Some(wanted)
                    if SchemaFingerprint::from_fields([field])
                        == SchemaFingerprint::from_fields([wanted]) =>
                {
                    return Err(ScribeError::ContractViolation(
                        BifrostError::UnsupportedType {
                            field: field.name().clone(),
                            data_type: field.data_type().to_string(),
                        },
                    ));
                }
                _ => None,
            };
            fields.push(match &cast {
                Some(data_type) => {
                    Arc::new(field.as_ref().clone().with_data_type(data_type.clone()))
                }
                None => Arc::clone(field),
            });
            columns.push((index, cast));
        }
        Ok(Self {
            schema: Arc::new(Schema::new_with_metadata(fields, sent.metadata().clone())),
            columns,
        })
    }

    /// Moves each column of `rows` into its conformed place, casting the ones
    /// whose sent spelling differs from the registered type.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::FingerprintMismatch`] when Arrow cannot cast an
    /// equivalent column and [`ScribeError::InvalidFrame`] when Arrow refuses
    /// the conformed batch, which duplicate-free input cannot cause.
    fn apply(&self, rows: &RecordBatch) -> Result<RecordBatch, ScribeError> {
        let columns = self
            .columns
            .iter()
            .map(|(index, cast)| {
                let column = rows.column(*index);
                match cast {
                    Some(data_type) => arrow::compute::cast(column, data_type).map_err(|_| {
                        ScribeError::FingerprintMismatch {
                            table: "resolved ingress table".to_owned(),
                        }
                    }),
                    None => Ok(Arc::clone(column)),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        RecordBatch::try_new(Arc::clone(&self.schema), columns)
            .map_err(|_| ScribeError::InvalidFrame)
    }
}

/// The conform plan of each registered schema the ingress lane decoded
/// against, for the sent schema shape it saw last.
///
/// A registered table's schema never changes and the catalog keeps its one
/// `Arc` for the node's life, so the plans grow with registered tables, never
/// with writes. Writers of one table send one shape in practice; a different
/// shape replaces the remembered plan, so alternating shapes only rebuild.
/// A refusal is never remembered.
#[derive(Debug, Default)]
pub(crate) struct ConformPlans {
    /// Plans keyed by the registered schema's address; each entry holds that
    /// `Arc`, so the address cannot be reused while the entry exists.
    plans: std::sync::RwLock<std::collections::HashMap<usize, RememberedPlan>>,
}

/// One remembered plan with the two schemas it was built for.
#[derive(Debug)]
struct RememberedPlan {
    /// Registered schema the plan conforms to, held to pin its address.
    registered: Arc<IcebergSchema>,
    /// Sent schema the plan was built from.
    sent: SchemaRef,
    /// The plan itself.
    plan: Arc<ConformPlan>,
}

impl ConformPlans {
    /// Conforms `rows` to `registered`, building and remembering the plan
    /// only when this registered schema has none for the sent shape.
    ///
    /// # Errors
    ///
    /// Returns the [`ConformPlan::build`] refusal for a non-conforming shape
    /// and the [`ConformPlan::apply`] failure for a failed cast.
    ///
    /// # Panics
    ///
    /// Panics when the plan lock is poisoned, which no holder can cause: it
    /// is held only to clone or insert one entry.
    fn conform(
        &self,
        rows: &RecordBatch,
        registered: &Arc<IcebergSchema>,
    ) -> Result<RecordBatch, ScribeError> {
        let sent = rows.schema();
        let key = Arc::as_ptr(registered).addr();
        let remembered = self
            .plans
            .read()
            .expect("the conform plan lock is never poisoned")
            .get(&key)
            .filter(|entry| {
                Arc::ptr_eq(&entry.registered, registered)
                    && (Arc::ptr_eq(&entry.sent, &sent) || entry.sent == sent)
            })
            .map(|entry| Arc::clone(&entry.plan));
        if let Some(plan) = remembered {
            return plan.apply(rows);
        }
        let plan = Arc::new(ConformPlan::build(&sent, registered)?);
        self.plans
            .write()
            .expect("the conform plan lock is never poisoned")
            .insert(
                key,
                RememberedPlan {
                    registered: Arc::clone(registered),
                    sent,
                    plan: Arc::clone(&plan),
                },
            );
        plan.apply(rows)
    }
}

/// Fingerprints one Arrow type's exact layout, nested names included.
///
/// Metadata is ignored, so a sent column already in the registered form is
/// left as is while one spelling a list element `item` or map entries
/// `entries` is cast to Iceberg's names like any other spelling.
fn layout(data_type: &DataType) -> SchemaFingerprint {
    SchemaFingerprint::from_arrow_schema_exact(&Schema::new(vec![Field::new(
        "",
        data_type.clone(),
        true,
    )]))
}

/// Validates a dynamic table's Variant values at the trust boundary.
///
/// A dynamic table has no built-in validator, so this is where its Variant
/// values are checked before admission; [`conform_to_registered`] has already
/// refused any Variant identity that differs from the registration. The
/// registered Iceberg schema, converted to Arrow by iceberg-rust, is the
/// declaration: never the supplied schema. The batch goes through
/// [`crate::tables::validate_declared_variants`], the same walk built-in
/// validators run, so each present Variant value must decode within the
/// encoding, depth, and size limits. It runs after the fingerprint check and
/// before card scope, stamping, dispatch, or any WAL mutation, and reads the
/// batch only.
///
/// # Errors
///
/// Returns [`ScribeError::FingerprintMismatch`] when the registered schema
/// has no Arrow form, and [`ScribeError::ContractViolation`] carrying the
/// Variant refusal for the first failing value in row, then field, order.
fn enforce_dynamic_variants(
    rows: &RecordBatch,
    registered: &IcebergSchema,
) -> Result<(), ScribeError> {
    let declared = iceberg::arrow::schema_to_arrow_schema(registered).map_err(|_| {
        ScribeError::FingerprintMismatch {
            table: "resolved ingress table".to_owned(),
        }
    })?;
    let declared: Vec<Field> = declared
        .fields()
        .iter()
        .map(|field| field.as_ref().clone())
        .collect();
    crate::tables::validate_declared_variants(&declared, rows)
        .map_err(ScribeError::ContractViolation)
}

/// Enforces one built-in's user contract before the fingerprint check.
///
/// The caller's user block — the batch without server-owned columns — goes
/// through the table's own value validator, the one seam every built-in
/// owns, so the Variant contract and a canonical signal's ledger refuse with
/// their catalogued code before the storage fingerprint could reduce them to
/// a mismatch. A canonical signal table additionally refuses any unknown
/// `wyrd_*` field first, and afterwards requires its user block to match
/// `(definition.arrow_fields)()` in order, so no structurally different Arrow
/// spelling reaches the physical schema. Field metadata is not compared: the
/// field id and sensitivity tag are the server's own physical identity,
/// stamped downstream from the registered table and the definition, so a
/// writer building from the published description neither supplies them nor
/// can be wrong about them.
///
/// `trusted` marks server-built rows whose values were validated when they
/// were built; for them the reserved-column refusal still runs but the value
/// validator does not walk the batch a second time.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for an unknown reserved column,
/// and [`ScribeError::ContractViolation`] carrying the validator's
/// catalogued refusal. Column order is not judged here: callers reorder by
/// name first, and the fingerprint check that follows owns the rest.
fn enforce_builtin_source_contract(
    rows: &RecordBatch,
    definition: &'static crate::tables::BuiltinTableDefinition,
    trusted: bool,
) -> Result<(), ScribeError> {
    let canonical = (definition.canonical_fields)().is_some();
    let schema = rows.schema();
    if canonical
        && schema
            .fields()
            .iter()
            .any(|field| field.name().starts_with("wyrd_"))
    {
        return Err(ScribeError::InvalidFrame);
    }
    if trusted {
        return Ok(());
    }
    let policy = definition.correlation_policy;
    let user = RecordBatch::try_new(
        Arc::new(Schema::new(user_fields(rows, policy))),
        user_columns(rows, policy),
    )
    .map_err(|_| ScribeError::InvalidFrame)?;
    (definition.canonical_validator)(&user).map_err(ScribeError::ContractViolation)?;
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

/// Fingerprints the caller-owned block of one source schema.
///
/// Server-owned correlation (`card_uid`, `principal_id`, and `run_id`) and
/// every `wyrd_*` column are dropped first, so the result is comparable with
/// the registered table's user-field fingerprint.
fn source_schema_fingerprint(schema: &Schema) -> SchemaFingerprint {
    let fields: Vec<Field> = schema
        .fields()
        .iter()
        .filter(|field| {
            let name = field.name().as_str();
            !matches!(name, CARD_UID | PRINCIPAL_ID | RUN_ID) && !name.starts_with("wyrd_")
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

/// Authorize every client-supplied `card_uid` against the writer's Card scope.
///
/// A batch without the column carries no Card correlation. Within the column,
/// a null is a valid uncorrelated row; a present value must parse as a
/// [`CardUid`] carried by a member of `scope`. `scope` is a Card-bound
/// principal's verified signed scope, whose members the mint resolved with
/// registry UIDs, or the registry-resolved Cards Gate attached for an unbound
/// writer, so a Card that is unregistered, of a non-observation kind, or
/// outside the signed scope is refused alike. Authorization is decided for the
/// whole batch before any row is admitted, so a single denied row refuses the
/// frame. Returns one canonical UID text per row, in row order, which is what
/// `card_uid` stores; the check performs no IO.
///
/// # Errors
///
/// Returns [`ScribeError::CardScopeDenied`] when the column is not UTF-8, or
/// when a row supplies a present value and there is no scope, the value is not
/// a UID, or no scope member carries it. A batch whose correlation column is
/// entirely null needs no scope.
fn authorized_card_uids(
    rows: &RecordBatch,
    scope: Option<&[CardUid]>,
) -> Result<Vec<Option<String>>, ScribeError> {
    let Some(column) = rows.column_by_name(CARD_UID) else {
        return Ok(vec![None; rows.num_rows()]);
    };
    let uids = column
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or(ScribeError::CardScopeDenied)?;
    uids.iter()
        .map(|uid| {
            uid.map(|raw| {
                let uid = CardUid::from_str(raw).map_err(|_| ScribeError::CardScopeDenied)?;
                if !scope.is_some_and(|scope| scope.contains(&uid)) {
                    return Err(ScribeError::CardScopeDenied);
                }
                Ok(uid.to_string())
            })
            .transpose()
        })
        .collect()
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
/// The stamped batch's managed field order is the
/// [`MANAGED_COLUMNS`](crate::tables::managed_columns::MANAGED_COLUMNS) order in
/// every payload mode (D88). `wyrd_ingested_at` is the one admission instant in
/// `context`, read once per batch at admission. Projected payloads retain
/// their remaining correlation values unchanged. Each row's `card_uid` is
/// authorized against `context.card_scope` before anything is stamped.
///
/// # Errors
/// Returns [`ScribeError::CardScopeDenied`] when a row's `card_uid` is not
/// authorized, and a typed Scribe error when timestamp construction,
/// managed-array construction, or final batch validation fails.
fn stamp_correlation_columns(
    rows: &RecordBatch,
    context: &DecodeContext<'_>,
) -> Result<RecordBatch, ScribeError> {
    let DecodeContext {
        principal,
        request_id,
        receipt_micros,
        ..
    } = context;
    let (principal, request_id, receipt_micros) = (*principal, *request_id, *receipt_micros);
    let card_uids = authorized_card_uids(rows, context.card_scope)?;
    let caller_run_id = caller_run_id_column(rows)?;
    let policy = correlation_policy(context);
    let mut fields = user_fields(rows);
    let mut columns = user_columns(rows);
    fields.extend(ensure_managed_columns(Vec::new(), policy));
    columns.extend(managed_arrays(
        rows,
        &ManagedValues {
            principal,
            card_uid: Arc::new(StringArray::from(card_uids)),
            request_id,
            receipt_micros,
            caller_run_id,
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
    if (definition.canonical_fields)().is_some() {
        enforce_canonical_physical_identity(&stamped, definition)?;
    }
    Ok(stamped)
}

/// Reports whether `name` is excluded from the user projection.
///
/// Every managed column is unconditionally excluded from the user projection
/// and materialized in its declared slot by [`managed_arrays`], regardless of
/// whether the caller supplied it. `run_id` is excluded for the same reason,
/// so the canonical nullable physical column is stamped exactly once, and
/// `card_uid` is excluded so only its authorized value is stamped.
fn is_server_owned(name: &str) -> bool {
    is_managed_column(name)
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

/// Resolves the correlation policy this write is decoded and stamped under.
///
/// The full envelope ([`CorrelationPolicy::Observation`]) is the default: a
/// dynamic table has no declaration to consult and keeps it. A built-in
/// stamps exactly the envelope its registered physical schema declares — a
/// [`CorrelationPolicy::None`] table carries its own identity columns — so the
/// sealed object is never wider or narrower than the table it is promoted
/// into.
fn correlation_policy(context: &DecodeContext<'_>) -> CorrelationPolicy {
    context
        .definition
        .map_or(CorrelationPolicy::Observation, |definition| {
            definition.correlation_policy
        })
}

/// The per-batch values the managed columns are stamped from.
struct ManagedValues<'a> {
    /// Authenticated writer stamped as `principal_id`.
    principal: &'a Principal,
    /// Authorized `card_uid` of each row, in row order.
    card_uid: ArrayRef,
    /// Request identity stamped as `wyrd_request_id`.
    request_id: &'a RequestId,
    /// Admission instant stamped as `wyrd_ingested_at`.
    receipt_micros: i64,
    /// Caller `run_id` column lifted out of the user projection, if supplied.
    caller_run_id: Option<ArrayRef>,
}

/// Builds one array per managed column `policy` appends, in declared order.
///
/// The layout — which columns, in which order, with which type — comes from
/// [`MANAGED_COLUMNS`](crate::tables::managed_columns::MANAGED_COLUMNS); this
/// function supplies only each column's values; `wyrd_ingested_at` is the
/// admission instant.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when a declared managed column has no
/// stamping rule here.
fn managed_arrays(
    rows: &RecordBatch,
    values: &ManagedValues<'_>,
    policy: CorrelationPolicy,
) -> Result<Vec<ArrayRef>, ScribeError> {
    let row_count = rows.num_rows();
    let admitted_at = Arc::new(
        TimestampMicrosecondArray::from(vec![values.receipt_micros; row_count])
            .with_timezone(iceberg::arrow::UTC_TIME_ZONE),
    ) as ArrayRef;
    policy
        .managed_columns()
        .map(|column| {
            Ok(match column.field.name {
                RUN_ID => values.caller_run_id.as_ref().map_or_else(
                    || Arc::new(StringArray::from(vec![None::<&str>; row_count])) as ArrayRef,
                    Arc::clone,
                ),
                CARD_UID => Arc::clone(&values.card_uid),
                PRINCIPAL_ID => Arc::new(StringArray::from(vec![
                    values.principal.id.to_string();
                    row_count
                ])),
                WYRD_REQUEST_ID => Arc::new(StringArray::from(vec![
                    values.request_id.as_str();
                    row_count
                ])),
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
        /// Acknowledgement fences that let a shard chain end at a bad record
        /// in its unacknowledged tail; `None` fails every bad record closed.
        fences: Option<crate::scribe::replay::ReplayFences>,
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
            fences,
        } => execute_replay_directory_stream(
            &path,
            wal.as_ref(),
            recovery_stream,
            &shard_senders,
            &memory,
            &cancelled,
            fences.as_ref(),
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
/// channel closure, shard restoration failure, or a failed fence lookup.
fn execute_replay_directory_stream(
    path: &Path,
    wal: &crate::scribe::wal::WalWriter,
    recovery_stream: StreamIdentity,
    shard_senders: &[mpsc::Sender<crate::scribe::shards::ShardCommand>],
    memory: &ScribeResources,
    cancelled: &AtomicBool,
    fences: Option<&crate::scribe::replay::ReplayFences>,
) -> Result<ScribeWalIoResult, ScribeError> {
    let mut restored = 0_usize;
    let lookup = |stream, shard| fences.map_or(Ok(None), |fences| fences.newest(stream, shard));
    let mut retirement_settlement = ReplayRetirementSettlement::new(path);
    crate::scribe::replay::replay_wal_directory_stream_accounted(
        path,
        Some(recovery_stream),
        Some(memory),
        Some(wal),
        Some(cancelled),
        fences.map(|_| &lookup as crate::scribe::wal::FenceLookup<'_>),
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
    use wyrd_spec::vala::BifrostError;
    use wyrd_spec::vala::managed_columns::{
        CARD_UID, PRINCIPAL_ID, WYRD_INGESTED_AT, WYRD_REQUEST_ID,
    };

    use bytes::Bytes;

    use super::{
        DecodeContext, ReplayRetirementSettlement, ScribeIngressCpuPool, ScribePersistenceCpuOp,
        ScribePersistenceCpuPool, ScribeWalIoPool, conform_to_registered, decode,
        record_lane_saturation, source_schema_fingerprint, stamp_correlation_columns,
        wait_lane_drained,
    };
    use crate::contracts::{IngressPayload, ScribeError};
    use crate::resources::{BifrostRole, BifrostRuntimeResources, ScribeMemoryCategory};
    use crate::schema::SchemaFingerprint;
    use crate::scribe::replay::ReplayedSealKey;
    use crate::scribe::seal_key::SealKey;
    use crate::scribe::stream_identity::StreamIdentity;
    use crate::tables::CorrelationPolicy;
    use crate::tables::managed_columns::ensure_managed_columns;
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
    /// Every field but the principal, its Card UID scope, and the request
    /// identity is the production default.
    fn stamping_context<'a>(
        principal: &'a Principal,
        card_scope: Option<&'a [CardUid]>,
        request_id: &'a RequestId,
    ) -> super::DecodeContext<'a> {
        super::DecodeContext {
            definition: None,
            principal,
            card_scope,
            expected_schema_fingerprint: SchemaFingerprint([0_u8; 32]),
            request_id,
            receipt_micros: receipt_now(),
            registered_schema: None,
            conform: None,
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
            stamp_correlation_columns(&rows, &stamping_context(&principal(), None, &RequestId::now_v7()))
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
            stamp_correlation_columns(&rows, &stamping_context(&principal(), None, &RequestId::now_v7()))
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

    /// The user columns the cast tests register, in Iceberg's Arrow form.
    fn cast_table_columns() -> Vec<Field> {
        let element = |data_type| Arc::new(Field::new("element", data_type, true));
        vec![
            Field::new("i", DataType::Int32, true),
            Field::new("u", DataType::Int64, true),
            Field::new(
                "ts",
                DataType::Timestamp(TimeUnit::Microsecond, Some("+00:00".into())),
                true,
            ),
            Field::new("s", DataType::Utf8, true),
            Field::new("b", DataType::LargeBinary, true),
            Field::new("l", DataType::List(element(DataType::Int32)), true),
            Field::new("n", DataType::List(element(DataType::Int64)), true),
        ]
    }

    /// Decode `rows` as one canonical batch for a table registered with
    /// [`cast_table_columns`], the path every production writer takes.
    ///
    /// # Errors
    ///
    /// Returns the decode refusal for `rows`.
    ///
    /// # Panics
    ///
    /// Panics when the registered schema does not convert to Iceberg.
    fn decode_for_cast_table(rows: RecordBatch) -> Result<RecordBatch, ScribeError> {
        let registered = crate::tables::iceberg_schema_for(&Schema::new(ensure_managed_columns(
            cast_table_columns(),
            CorrelationPolicy::Observation,
        )))
        .expect("the registered schema converts to Iceberg");
        decode(
            IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![rows])),
            &DecodeContext {
                principal: &principal(),
                card_scope: principal()
                    .card_ref_scope()
                    .map(CardRefScope::uids)
                    .as_deref(),
                expected_schema_fingerprint: source_schema_fingerprint(&Schema::new(cast_table_columns())),
                request_id: &RequestId::now_v7(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: Some(&Arc::new(registered)),
                conform: None,
            },
        )
    }

    /// One valid row and one null row of narrower spellings for every
    /// [`cast_table_columns`] column. The `ts` column holds the instant `1`
    /// labelled `ts_zone`, or a naive reading when it is `None`.
    ///
    /// The writer sends `Int8`, `UInt32`, a `UTC` zone, `LargeUtf8`, `Binary`,
    /// a `List<Int16>` with Arrow's default `item` element, and a `List<Int64>`
    /// differing only in that element name.
    fn narrow_spellings(ts_zone: Option<&str>) -> RecordBatch {
        let item = |data_type| DataType::List(Arc::new(Field::new("item", data_type, true)));
        let mut items = arrow::array::ListBuilder::new(arrow::array::Int16Builder::new());
        items.values().append_value(7);
        items.append(true);
        items.append(false);
        let mut renamed = arrow::array::ListBuilder::new(arrow::array::Int64Builder::new());
        renamed.values().append_value(9);
        renamed.append(true);
        renamed.append(false);
        batch(
            vec![
                Field::new("i", DataType::Int8, true),
                Field::new("u", DataType::UInt32, true),
                Field::new(
                    "ts",
                    DataType::Timestamp(TimeUnit::Microsecond, ts_zone.map(Into::into)),
                    true,
                ),
                Field::new("s", DataType::LargeUtf8, true),
                Field::new("b", DataType::Binary, true),
                Field::new("l", item(DataType::Int16), true),
                Field::new("n", item(DataType::Int64), true),
            ],
            vec![
                Arc::new(arrow::array::Int8Array::from(vec![Some(-3), None])),
                Arc::new(arrow::array::UInt32Array::from(vec![Some(u32::MAX), None])),
                Arc::new(
                    TimestampMicrosecondArray::from(vec![Some(1), None]).with_timezone_opt(ts_zone),
                ),
                Arc::new(arrow::array::LargeStringArray::from(vec![Some("x"), None])),
                Arc::new(arrow::array::BinaryArray::from(vec![
                    Some(b"y".as_slice()),
                    None,
                ])),
                Arc::new(items.finish()),
                Arc::new(renamed.finish()),
            ],
        )
    }

    /// Remembered conform plans conform exactly as a fresh plan does, and a
    /// second batch of the same sent shape reuses the first one's plan.
    ///
    /// # Panics
    ///
    /// Panics when the registered schema does not convert, a batch is
    /// refused, the remembered result differs from the fresh one, or the
    /// second batch rebuilds the plan.
    #[test]
    fn remembered_conform_plan_matches_a_fresh_plan_and_is_reused() {
        let registered = Arc::new(
            crate::tables::iceberg_schema_for(&Schema::new(cast_table_columns()))
                .expect("the registered schema converts to Iceberg"),
        );
        let plans = super::ConformPlans::default();

        let first = plans
            .conform(&narrow_spellings(Some("UTC")), &registered)
            .expect("the first batch conforms");
        let second = plans
            .conform(&narrow_spellings(Some("UTC")), &registered)
            .expect("the second batch conforms");

        assert_eq!(
            first,
            super::conform_to_registered(&narrow_spellings(Some("UTC")), &registered)
                .expect("a fresh plan conforms")
        );
        assert!(
            Arc::ptr_eq(&first.schema(), &second.schema()),
            "the second batch of one sent shape reuses the remembered plan"
        );
    }

    /// Spellings Iceberg stores as the registered column type are cast to it.
    ///
    /// Each [`narrow_spellings`] column decodes to exactly the registered
    /// layout, nested names included, with its values intact.
    ///
    /// # Panics
    ///
    /// Panics when the batch is refused, a column keeps its sent layout, or a
    /// value is lost.
    #[test]
    fn same_iceberg_type_spellings_cast_to_the_registered_type() {
        let stamped = decode_for_cast_table(narrow_spellings(Some("UTC")))
            .expect("same-Iceberg-type spellings are accepted");

        for declared in &cast_table_columns() {
            let schema = stamped.schema();
            let arrived = schema
                .field_with_name(declared.name())
                .expect("declared field");
            assert_eq!(
                super::layout(arrived.data_type()),
                super::layout(declared.data_type()),
                "`{}` arrives as {:?}",
                declared.name(),
                arrived.data_type()
            );
            let column = stamped
                .column_by_name(declared.name())
                .expect("declared column");
            assert_eq!(
                column.null_count(),
                1,
                "`{}` keeps its null",
                declared.name()
            );
        }
        let ints = stamped
            .column_by_name("u")
            .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
            .expect("u is Int64");
        assert_eq!(ints.value(0), i64::from(u32::MAX));
    }

    /// A zoned timestamp in any display zone is stored as the same instant
    /// under the `+00:00` label.
    ///
    /// # Panics
    ///
    /// Panics when a zone label is refused, kept, or changes the instant.
    #[test]
    fn a_zone_label_never_changes_the_instant() {
        for zone in ["America/Chicago", "-05:00", "+09:00"] {
            let stamped = decode_for_cast_table(narrow_spellings(Some(zone)))
                .unwrap_or_else(|error| panic!("`{zone}` is accepted: {error:?}"));

            let ts = stamped
                .column_by_name("ts")
                .and_then(|column| column.as_any().downcast_ref::<TimestampMicrosecondArray>())
                .expect("ts is a microsecond timestamp");
            assert_eq!(ts.timezone(), Some(iceberg::arrow::UTC_TIME_ZONE), "{zone}");
            assert_eq!(ts.value(0), 1, "{zone}");
        }
    }

    /// A naive timestamp is a wall-clock reading with no instant, so it is
    /// refused for a zoned column rather than guessed as UTC.
    ///
    /// # Panics
    ///
    /// Panics when the naive batch is accepted.
    #[test]
    fn a_naive_timestamp_is_refused_for_a_zoned_column() {
        assert!(matches!(
            decode_for_cast_table(narrow_spellings(None)),
            Err(ScribeError::FingerprintMismatch { .. })
        ));
    }

    /// A wider `Int64` sent for an `Int32` column is a different Iceberg type
    /// and is refused rather than cast.
    ///
    /// # Panics
    ///
    /// Panics when the wider batch is accepted.
    #[test]
    fn a_wider_type_is_refused() {
        let wider = batch(
            vec![Field::new("i", DataType::Int64, true)],
            vec![Arc::new(Int64Array::from(vec![Some(1_i64)]))],
        );

        assert!(matches!(
            decode_for_cast_table(wider),
            Err(ScribeError::FingerprintMismatch { .. })
        ));
    }

    /// A canonical built-in's validator refusal reaches the caller unchanged.
    ///
    /// An undeclared column also changes the storage fingerprint, so this
    /// proves the table's validator runs first and its catalogued
    /// `UndeclaredField` is not reduced to a fingerprint mismatch.
    ///
    /// # Panics
    ///
    /// Panics when the definition is missing or the decode does not refuse
    /// with the catalogued `UndeclaredField`.
    #[test]
    fn canonical_validator_refusal_keeps_its_catalogued_code() {
        let definition =
            crate::tables::builtin_table("traces", "spans").expect("spans is built in");
        let mut fields = (definition.arrow_fields)();
        fields.push(Field::new("surprise", DataType::Utf8, true));
        let rows = RecordBatch::new_empty(Arc::new(Schema::new(fields)));

        let refusal = decode(
            ipc_payload(&rows),
            &DecodeContext {
                principal: &principal(),
                card_scope: principal()
                    .card_ref_scope()
                    .map(CardRefScope::uids)
                    .as_deref(),
                expected_schema_fingerprint: SchemaFingerprint((definition.schema_fingerprint)()),
                request_id: &RequestId::now_v7(),
                receipt_micros: receipt_now(),
                definition: Some(definition),
                registered_schema: None,
                conform: None,
            },
        )
        .expect_err("an undeclared column is refused");

        assert!(
            matches!(
                refusal,
                ScribeError::ContractViolation(BifrostError::UndeclaredField { ref field, row: 0 })
                    if field == "surprise"
            ),
            "{refusal:?}"
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
        let mut context = stamping_context(&principal, None, &request_id);
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
                card_scope: principal()
                    .card_ref_scope()
                    .map(CardRefScope::uids)
                    .as_deref(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
                conform: None,
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
                card_scope: principal
                    .card_ref_scope()
                    .map(CardRefScope::uids)
                    .as_deref(),
                expected_schema_fingerprint: source_schema_fingerprint(schema),
                request_id: &RequestId::now_v7(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
                conform: None,
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
                Field::new(CARD_UID, DataType::Utf8, false),
            ],
            vec![
                Arc::new(Int64Array::from(vec![1_i64])),
                Arc::new(StringArray::from(vec!["client-run"])),
                Arc::new(StringArray::from(vec![uid_text(&card)])),
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

    /// A `card_uid` value outside the service principal's own Card scope is refused
    /// during decode, before any writer admission sees the frame.
    ///
    /// # Panics
    ///
    /// Panics when decode accepts the out-of-scope row or refuses it with anything
    /// other than `CardScopeDenied`.
    #[test]
    fn card_scope_rejects_before_writer_admission() {
        let card = resolved_card("prod/Service/billing@1.0.0");
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
            vec![Field::new(CARD_UID, DataType::Utf8, false)],
            vec![Arc::new(StringArray::from(vec![
                Uuid::now_v7().to_string(),
            ]))],
        );
        let error = decode(
            IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![
                rows.clone(),
            ])),
            &DecodeContext {
                principal: &principal,
                card_scope: principal
                    .card_ref_scope()
                    .map(CardRefScope::uids)
                    .as_deref(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
                conform: None,
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
    /// `card_uid` is a permitted client correlation column rather than a
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
                Field::new(CARD_UID, DataType::Utf8, true),
                Field::new(CARD_UID, DataType::Utf8, true),
            ],
            vec![
                Arc::new(StringArray::from(vec![None::<&str>])),
                Arc::new(StringArray::from(vec![Some(Uuid::now_v7().to_string())])),
            ],
        );
        let error = decode(
            IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![
                rows.clone(),
            ])),
            &DecodeContext {
                principal: &principal(),
                card_scope: principal()
                    .card_ref_scope()
                    .map(CardRefScope::uids)
                    .as_deref(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
                conform: None,
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
                card_scope: principal()
                    .card_ref_scope()
                    .map(CardRefScope::uids)
                    .as_deref(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
                conform: None,
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
                card_scope: principal()
                    .card_ref_scope()
                    .map(CardRefScope::uids)
                    .as_deref(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
                conform: None,
            },
        )
        .expect("schema is valid");
        assert!(decoded.schema().index_of("data_tenant_id").is_err());
        assert!(decoded.schema().index_of(PRINCIPAL_ID).is_ok());
        assert!(decoded.schema().index_of("wyrd_batch_id").is_err());
        assert!(decoded.schema().index_of(WYRD_INGESTED_AT).is_ok());
        assert!(decoded.schema().index_of(WYRD_INGESTED_AT).is_ok());
        assert!(decoded.schema().index_of(WYRD_REQUEST_ID).is_ok());
    }

    /// The signed UID text of a resolved scope member, as a writer sends it.
    ///
    /// # Panics
    /// Panics when `card` carries no UID.
    fn uid_text(card: &CardRef) -> String {
        card.uid
            .as_ref()
            .expect("resolved card carries a UID")
            .to_string()
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

    /// A stamped native payload has the canonical managed field order the
    /// Oracle's pinned sealed-fragment fingerprint requires.
    ///
    /// # Panics
    ///
    /// Panics when the payload is refused or when the stamped field order differs
    /// from the canonical managed order.
    #[test]
    fn native_stamping_lands_in_canonical_managed_order() {
        let (principal, card) = scoped_service_principal();
        let rows = batch(
            vec![
                Field::new("value", DataType::Int64, false),
                Field::new(CARD_UID, DataType::Utf8, false),
            ],
            vec![
                Arc::new(Int64Array::from(vec![1_i64])),
                Arc::new(StringArray::from(vec![uid_text(&card)])),
            ],
        );
        let decoded = decode(
            ipc_payload(&rows),
            &DecodeContext {
                principal: &principal,
                card_scope: principal
                    .card_ref_scope()
                    .map(CardRefScope::uids)
                    .as_deref(),
                expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                request_id: &RequestId::now_v7(),
                receipt_micros: receipt_now(),
                definition: None,
                registered_schema: None,
                conform: None,
            },
        )
        .expect("native ingest is stamped");
        assert_eq!(
            stamped_field_names(&decoded),
            canonical_managed_order(),
            "stamping must yield the canonical managed field order",
        );
    }

    /// Every server-owned column and any other `wyrd_*` name, such as the
    /// unmanaged `wyrd_custom`, is reserved and rejected when a native
    /// payload supplies it.
    ///
    /// # Panics
    ///
    /// Panics when a native payload supplying any other managed column is accepted
    /// or refused with anything other than `InvalidFrame`.
    #[test]
    fn native_other_reserved_columns_still_rejected() {
        let (principal, card) = scoped_service_principal();
        for reserved in [
            WYRD_INGESTED_AT,
            WYRD_REQUEST_ID,
            PRINCIPAL_ID,
            "wyrd_custom",
        ] {
            let rows = batch(
                vec![
                    Field::new(reserved, DataType::Utf8, false),
                    Field::new(CARD_UID, DataType::Utf8, false),
                ],
                vec![
                    Arc::new(StringArray::from(vec!["caller"])),
                    Arc::new(StringArray::from(vec![uid_text(&card)])),
                ],
            );
            let error = decode(
                ipc_payload(&rows),
                &DecodeContext {
                    principal: &principal,
                    card_scope: principal
                        .card_ref_scope()
                        .map(CardRefScope::uids)
                        .as_deref(),
                    expected_schema_fingerprint: source_schema_fingerprint(rows.schema().as_ref()),
                    request_id: &RequestId::now_v7(),
                    receipt_micros: receipt_now(),
                    definition: None,
                    registered_schema: None,
                    conform: None,
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

    /// How a single-row fixture carries its optional `card_uid` correlation.
    #[derive(Debug, Clone, Copy)]
    enum SuppliedCardUid<'a> {
        /// The batch has no `card_uid` column at all.
        Absent,
        /// The batch has the column and the row's value is null.
        Null,
        /// The batch has the column and the row carries this text.
        Text(&'a str),
    }

    /// Stamp one single-row batch carrying the supplied optional `card_uid`.
    ///
    /// # Errors
    /// Returns the stamping refusal, `CardScopeDenied` for an unauthorized UID.
    fn stamp_card_uid(
        principal: &Principal,
        card_uid: SuppliedCardUid<'_>,
    ) -> Result<RecordBatch, ScribeError> {
        let mut fields = vec![Field::new("value", DataType::Int64, false)];
        let mut columns: Vec<ArrayRef> = vec![Arc::new(Int64Array::from(vec![1_i64]))];
        let value = match card_uid {
            SuppliedCardUid::Absent => None,
            SuppliedCardUid::Null => Some(None),
            SuppliedCardUid::Text(text) => Some(Some(text)),
        };
        if let Some(value) = value {
            fields.push(Field::new(CARD_UID, DataType::Utf8, true));
            columns.push(Arc::new(StringArray::from(vec![value])));
        }
        let scope = principal.card_ref_scope().map(CardRefScope::uids);
        stamp_correlation_columns(
            &batch(fields, columns),
            &stamping_context(principal, scope.as_deref(), &RequestId::now_v7()),
        )
    }

    /// Read the single stamped `card_uid` value of a one-row batch.
    ///
    /// # Panics
    /// Panics when the batch has no `Utf8` `card_uid` column.
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

    /// Optional and scoped Card correlation stamps only signed UIDs.
    ///
    /// # Panics
    ///
    /// Panics when an absent or null correlation does not stamp a null
    /// `card_uid`, when a scoped member's signed UID is not stamped unchanged,
    /// or when a UID outside the scope, a UID-less membership's reference text,
    /// or malformed text is admitted.
    #[test]
    fn optional_and_scoped_card_correlations_stamp_signed_uids() {
        let root = resolved_card("prod/Service/checkout@1.0.0");
        let secondary = resolved_card("prod/Agent/planner@2.0.0");
        let principal = scoped_principal(
            &root,
            CardRefScope::from_root_and_members(&root, [secondary.clone()]),
        );

        for (label, supplied) in [
            ("absent", SuppliedCardUid::Absent),
            ("explicit null", SuppliedCardUid::Null),
        ] {
            let stamped = stamp_card_uid(&principal, supplied)
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
            let uid = uid_text(member);
            let stamped = stamp_card_uid(&principal, SuppliedCardUid::Text(&uid))
                .expect("a signed scope member is admitted");
            assert_eq!(
                stamped_card_uid(&stamped),
                Some(uid),
                "{member} stamps its own signed UID, never the root's"
            );
        }

        let unsigned = CardRef::from_str("prod/Service/unsigned@1.0.0").expect("card");
        let unsigned_principal = scoped_principal(
            &root,
            CardRefScope::from_root_and_members(&root, [unsigned.clone()]),
        );
        for (label, principal, supplied) in [
            (
                "a UID-less signed membership's reference",
                &unsigned_principal,
                unsigned.to_string(),
            ),
            (
                "malformed correlation text",
                &principal,
                "not-a-card-uid".to_owned(),
            ),
            (
                "a UID outside the scope",
                &principal,
                Uuid::now_v7().to_string(),
            ),
        ] {
            let error = stamp_card_uid(principal, SuppliedCardUid::Text(&supplied))
                .expect_err("{label} fails closed");
            assert!(
                matches!(error, ScribeError::CardScopeDenied),
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
    /// its signed one, or when a different Verifier's UID is admitted.
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
        let uid = uid_text(&verifier);

        let stamped = stamp_card_uid(&principal, SuppliedCardUid::Text(&uid))
            .expect("the signed Verifier is admitted");
        assert_eq!(stamped_card_uid(&stamped), Some(uid));

        let forged = uid_text(&resolved_card("prod/Verifier/other@1.0.0"));
        assert!(matches!(
            stamp_card_uid(&principal, SuppliedCardUid::Text(&forged)),
            Err(ScribeError::CardScopeDenied)
        ));
    }

    /// A column whose storage matches a registered Variant but whose Variant
    /// identity does not — the bare storage Struct, or the marker with
    /// parameters — is refused as an unsupported type naming the column,
    /// while the canonical Variant conforms unchanged.
    ///
    /// # Panics
    ///
    /// Panics when a mismatched identity conforms or is refused with another
    /// error, or the canonical Variant is refused.
    #[test]
    fn a_variant_identity_mismatch_is_an_unsupported_type() {
        let variant = wyrd_types::variant::variant_field("payload", true);
        let registered =
            iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(&Schema::new(vec![
                variant.clone(),
            ]))
            .expect("a Variant registers");
        let storage =
            arrow::array::StructArray::new_null(wyrd_types::variant::variant_storage_fields(), 1);
        let send = |field: Field| {
            RecordBatch::try_new(
                Arc::new(Schema::new(vec![field])),
                vec![Arc::new(storage.clone()) as ArrayRef],
            )
            .expect("batch builds")
        };
        let mut parameters = variant.metadata().clone();
        parameters.insert(
            arrow_schema::extension::EXTENSION_TYPE_METADATA_KEY.to_owned(),
            "{\"shredded\":true}".to_owned(),
        );

        conform_to_registered(&send(variant.clone()), &registered)
            .expect("the canonical Variant conforms");
        for sent in [
            Field::new("payload", variant.data_type().clone(), true),
            variant.with_metadata(parameters),
        ] {
            assert!(matches!(
                conform_to_registered(&send(sent), &registered),
                Err(ScribeError::ContractViolation(BifrostError::UnsupportedType { ref field, .. }))
                    if field == "payload"
            ));
        }
    }

    /// A registered `(a, b)` table: two optional Long columns.
    fn registered_ab() -> iceberg::spec::Schema {
        let long = || iceberg::spec::Type::Primitive(iceberg::spec::PrimitiveType::Long);
        iceberg::spec::Schema::builder()
            .with_fields(vec![
                Arc::new(iceberg::spec::NestedField::optional(1, "a", long())),
                Arc::new(iceberg::spec::NestedField::optional(2, "b", long())),
            ])
            .build()
            .expect("the (a, b) schema builds")
    }

    /// Columns are matched to the registered schema by name: a batch sent as
    /// `(card_uid, b, a)` comes back as `(a, b, card_uid)` with each column's
    /// values intact; the unregistered `card_uid` input keeps its place after
    /// the registered columns for the later checks to judge.
    ///
    /// # Panics
    ///
    /// Panics when the batch is refused, the order differs, or a value moves.
    #[test]
    fn columns_are_put_in_declared_order_by_name() {
        let rows = RecordBatch::try_from_iter([
            (
                CARD_UID,
                Arc::new(StringArray::from(vec![None::<&str>])) as ArrayRef,
            ),
            ("b", Arc::new(Int64Array::from(vec![2])) as ArrayRef),
            ("a", Arc::new(Int64Array::from(vec![1])) as ArrayRef),
        ])
        .expect("batch builds");

        let ordered = conform_to_registered(&rows, &registered_ab()).expect("names all match");

        let names = ordered
            .schema()
            .fields()
            .iter()
            .map(|field| field.name().clone())
            .collect::<Vec<_>>();
        assert_eq!(names, ["a", "b", CARD_UID]);
        assert_eq!(
            ordered.column(0).as_ref(),
            &Int64Array::from(vec![1]) as &dyn Array
        );
    }
}
