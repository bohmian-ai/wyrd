//! Per-execution verification telemetry: classification, phases, and waits.
//!
//! [`ExecutionTelemetry`] is the one owner of a Verifier execution's metric
//! lifetime, whether a queued attempt or a direct request. It classifies the
//! execution into the closed [`VerifierKind`] set once its exact Verifier is
//! loaded, accumulates each [`Phase`]'s elapsed intervals, and records the
//! dependency and provider waits measured inside the engine. Finishing emits
//! one attempt observation and at most one sample per applicable phase;
//! dropping it — on error, cancellation, or shutdown — still returns the
//! active gauge exactly once. Engine overhead is the engine's elapsed time
//! less the union of the waits clipped to it, so nested or concurrent waits
//! are subtracted once and synchronous folding or scoring stays local work.

use std::future::Future;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tracing::Instrument as _;

use wyrd_spec::verification::VerifierKind;

use crate::app::metrics::{
    VERIFICATION_ACTIVE_RUNS, VERIFICATION_ENGINE_OVERHEAD_SECONDS,
    VERIFICATION_PHASE_DURATION_SECONDS, VERIFICATION_RUN_ATTEMPTS_TOTAL,
    VERIFICATION_RUN_DURATION_SECONDS, VERIFICATION_RUN_FAILURES_TOTAL,
};

/// One measured `[start, end)` interval on the process-monotonic clock.
type Interval = (Instant, Instant);

/// The closed `mode` dimension: how the execution was admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// A claimed attempt of a durable queued run.
    Queued,
    /// A synchronous supplied-input request.
    Direct,
}

impl ExecutionMode {
    /// The stable metric and span label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Direct => "direct",
        }
    }
}

/// The closed `phase` dimension of `wyrd_verification_phase_duration_seconds`.
///
/// `Engine` is the inclusive parent of `InputRead` and `Prepare`; phases
/// overlap and are never summed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Resolve the exact registered Verifier.
    Load,
    /// Authorized queued evidence retrieval; direct execution has none.
    InputRead,
    /// Input normalization, spec/baseline decoding, and plan construction.
    Prepare,
    /// Inclusive engine dispatch through judgment.
    Engine,
    /// Queued fenced durable state transition.
    Settlement,
}

impl Phase {
    /// Every phase, in emission order.
    const ALL: [Self; 5] = [
        Self::Load,
        Self::InputRead,
        Self::Prepare,
        Self::Engine,
        Self::Settlement,
    ];

    /// The stable metric label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Load => "load",
            Self::InputRead => "input_read",
            Self::Prepare => "prepare",
            Self::Engine => "engine",
            Self::Settlement => "settlement",
        }
    }
}

/// Mutable accounting of one execution, guarded by one short-held lock.
#[derive(Debug)]
struct Ledger {
    /// The current classification, `Unknown` until the Verifier loads.
    kind: VerifierKind,
    /// Whether the attempt counter has been emitted.
    counted: bool,
    /// Measured intervals per phase, indexed like [`Phase::ALL`].
    phases: [Vec<Interval>; 6],
}

/// Shareable recorder of one execution's dependency, evidence, and provider waits.
///
/// Cloning shares the same interval list, so a wait measured inside a spawned
/// task, such as a judge invocation on the Eval executor's task set, joins
/// the execution that owns it.
#[derive(Debug, Clone, Default)]
pub struct WaitSink {
    /// Measured waits, in completion order.
    intervals: std::sync::Arc<Mutex<Vec<Interval>>>,
}

impl WaitSink {
    /// Await `future` as one wait.
    pub async fn wait<F: Future>(&self, future: F) -> F::Output {
        let start = Instant::now();
        let output = future.await;
        self.record(start, Instant::now());
        output
    }

    /// Record one `[start, end)` wait measured by the caller.
    pub fn record(&self, start: Instant, end: Instant) {
        self.lock().push((start, end));
    }

    /// Lock the intervals, recovering them from a poisoned lock.
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Interval>> {
        self.intervals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Owner of one verification execution's metrics lifetime.
///
/// Created when a queued attempt is claimed or a direct request enters the
/// runtime; it raises the active gauge at once and lowers it exactly once on
/// drop, under whatever kind the execution then carries. A direct execution
/// dropped before [`finish`](Self::finish) — a client disconnect — finishes
/// as `cancelled`.
#[derive(Debug)]
pub struct ExecutionTelemetry {
    /// How the execution was admitted.
    mode: ExecutionMode,
    /// When the execution entered the runtime.
    started: Instant,
    /// Classification and phases.
    ledger: Mutex<Ledger>,
    /// Waits measured inside the engine.
    waits: WaitSink,
    /// Whether the execution's outcome was emitted.
    finished: bool,
}

impl ExecutionTelemetry {
    /// Start accounting for one `mode` execution, initially `unknown`.
    #[must_use]
    pub fn start(mode: ExecutionMode) -> Self {
        metrics::gauge!(
            VERIFICATION_ACTIVE_RUNS,
            "kind" => VerifierKind::Unknown.as_str(),
            "mode" => mode.as_str()
        )
        .increment(1.0);
        Self {
            mode,
            started: Instant::now(),
            ledger: Mutex::new(Ledger {
                kind: VerifierKind::Unknown,
                counted: false,
                phases: Default::default(),
            }),
            waits: WaitSink::default(),
            finished: false,
        }
    }

    /// The execution's mode.
    #[must_use]
    pub const fn mode(&self) -> ExecutionMode {
        self.mode
    }

    /// The execution's current kind.
    #[must_use]
    pub fn kind(&self) -> VerifierKind {
        self.lock().kind
    }

    /// Classify the execution once its exact Verifier is known.
    ///
    /// Moves the active gauge from the previous kind to `kind`, emits the
    /// attempt counter the first time, and records `kind` on the current span.
    pub fn classify(&self, kind: VerifierKind) {
        let mut ledger = self.lock();
        if ledger.kind != kind {
            self.active(ledger.kind).decrement(1.0);
            self.active(kind).increment(1.0);
            ledger.kind = kind;
        }
        if !ledger.counted {
            ledger.counted = true;
            self.attempts(kind);
        }
        tracing::Span::current().record("kind", kind.as_str());
    }

    /// Await `future` as one interval of `phase`.
    pub async fn phase<F: Future>(&self, phase: Phase, future: F) -> F::Output {
        let start = Instant::now();
        let output = future.await;
        self.record_phase(phase, start, Instant::now());
        output
    }

    /// Await `future` as one `prepare` interval inside a `verification.prepare` span.
    pub async fn prepare<F: Future>(&self, future: F) -> F::Output {
        self.phase(
            Phase::Prepare,
            future.instrument(tracing::info_span!("verification.prepare")),
        )
        .await
    }

    /// Record one `[start, end)` interval of `phase`.
    pub fn record_phase(&self, phase: Phase, start: Instant, end: Instant) {
        let index = Phase::ALL
            .iter()
            .position(|candidate| *candidate == phase)
            .unwrap_or_default();
        self.lock().phases[index].push((start, end));
    }

    /// Await `future` as one dependency, evidence, or provider wait.
    pub async fn wait<F: Future>(&self, future: F) -> F::Output {
        self.waits.wait(future).await
    }

    /// The shareable sink recording this execution's waits.
    #[must_use]
    pub fn waits(&self) -> WaitSink {
        self.waits.clone()
    }

    /// Emit the execution's duration, phases, overhead, and failure under `outcome`.
    ///
    /// `failed` marks an unsuccessful execution outcome; a completed failed
    /// judgment is not one. Phases with no interval emit no sample. The
    /// active gauge is lowered when `self` drops at the end of this call.
    pub fn finish(mut self, outcome: &'static str, failed: bool) {
        self.emit(outcome, failed);
        self.finished = true;
    }

    /// Emit the outcome families of [`finish`](Self::finish).
    fn emit(&self, outcome: &'static str, failed: bool) {
        let ledger = self.lock();
        let (kind, mode) = (ledger.kind.as_str(), self.mode.as_str());
        for (phase, intervals) in Phase::ALL.iter().zip(&ledger.phases) {
            if !intervals.is_empty() {
                metrics::histogram!(
                    VERIFICATION_PHASE_DURATION_SECONDS,
                    "kind" => kind,
                    "mode" => mode,
                    "phase" => phase.as_str()
                )
                .record(union(intervals).as_secs_f64());
            }
        }
        let engine = &ledger.phases[3];
        if !engine.is_empty() {
            metrics::histogram!(
                VERIFICATION_ENGINE_OVERHEAD_SECONDS,
                "kind" => kind,
                "mode" => mode,
                "outcome" => outcome
            )
            .record(overhead(engine, &self.waits.lock()).as_secs_f64());
        }
        if failed {
            metrics::counter!(
                VERIFICATION_RUN_FAILURES_TOTAL,
                "kind" => kind,
                "mode" => mode,
                "outcome" => outcome
            )
            .increment(1);
        }
        metrics::histogram!(
            VERIFICATION_RUN_DURATION_SECONDS,
            "kind" => kind,
            "mode" => mode,
            "outcome" => outcome
        )
        .record(self.started.elapsed().as_secs_f64());
    }

    /// The active gauge series of `kind` in this execution's mode.
    fn active(&self, kind: VerifierKind) -> metrics::Gauge {
        metrics::gauge!(
            VERIFICATION_ACTIVE_RUNS,
            "kind" => kind.as_str(),
            "mode" => self.mode.as_str()
        )
    }

    /// Emit one attempt observation under `kind`.
    fn attempts(&self, kind: VerifierKind) {
        metrics::counter!(
            VERIFICATION_RUN_ATTEMPTS_TOTAL,
            "kind" => kind.as_str(),
            "mode" => self.mode.as_str()
        )
        .increment(1);
    }

    /// Lock the ledger, recovering it from a poisoned lock.
    ///
    /// No code path panics while holding it; recovery keeps telemetry from
    /// ever failing an execution.
    fn lock(&self) -> std::sync::MutexGuard<'_, Ledger> {
        self.ledger
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Drop for ExecutionTelemetry {
    /// Finish an abandoned direct execution as `cancelled`, then lower the
    /// active gauge exactly once and count an execution that was never
    /// classified as `unknown`.
    fn drop(&mut self) {
        if !self.finished && self.mode == ExecutionMode::Direct {
            self.emit("cancelled", true);
        }
        let ledger = self
            .ledger
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (kind, counted) = (ledger.kind, ledger.counted);
        self.active(kind).decrement(1.0);
        if !counted {
            self.attempts(kind);
        }
    }
}

/// Measures the waits of one streaming read as the gaps between folds.
///
/// A streaming evidence read interleaves IO with synchronous folding. Each
/// [`folding`](Self::folding) call closes the wait since the previous fold
/// and [`folded`](Self::folded) reopens it, so fold CPU stays local work.
#[derive(Debug)]
pub struct StreamWaits<'a> {
    /// The sink of the execution the waits belong to.
    waits: &'a WaitSink,
    /// Start of the wait in progress.
    since: Instant,
}

impl<'a> StreamWaits<'a> {
    /// Open a wait at the start of a streaming read.
    #[must_use]
    pub fn open(telemetry: &'a ExecutionTelemetry) -> Self {
        Self {
            waits: &telemetry.waits,
            since: Instant::now(),
        }
    }

    /// Close the current wait as a batch arrives for folding.
    pub fn folding(&self) {
        self.waits.record(self.since, Instant::now());
    }

    /// Reopen the wait once a batch has been folded.
    pub fn folded(&mut self) {
        self.since = Instant::now();
    }

    /// Close the final wait when the stream settles.
    pub fn close(self) {
        self.waits.record(self.since, Instant::now());
    }
}

/// Total length of the union of `intervals`.
fn union(intervals: &[Interval]) -> Duration {
    let mut sorted: Vec<Interval> = intervals
        .iter()
        .copied()
        .filter(|(start, end)| end > start)
        .collect();
    sorted.sort_unstable();
    let mut total = Duration::ZERO;
    let mut current: Option<Interval> = None;
    for (start, end) in sorted {
        current = match current {
            Some((open, close)) if start <= close => Some((open, close.max(end))),
            Some((open, close)) => {
                total += close - open;
                Some((start, end))
            }
            None => Some((start, end)),
        };
    }
    if let Some((open, close)) = current {
        total += close - open;
    }
    total
}

/// Engine elapsed time less the union of `waits` clipped to the engine.
///
/// Each wait is clipped to each engine interval, so a wait straddling the
/// engine boundary or overlapping another wait is subtracted once.
fn overhead(engine: &[Interval], waits: &[Interval]) -> Duration {
    let clipped: Vec<Interval> = engine
        .iter()
        .flat_map(|(open, close)| {
            waits
                .iter()
                .map(move |(start, end)| ((*start).max(*open), (*end).min(*close)))
        })
        .collect();
    union(engine).saturating_sub(union(&clipped))
}

#[cfg(test)]
mod tests {
    //! Proof of classification and the wait-union arithmetic.

    use super::*;

    /// Offset `ms` milliseconds from `base`.
    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    /// Nested and concurrent waits are subtracted once, waits outside the
    /// engine are clipped away, and an engine without waits is all overhead.
    #[test]
    fn overhead_subtracts_the_union_of_clipped_waits_once() {
        let t = Instant::now();
        let engine = [(at(t, 0), at(t, 100))];
        let waits = [
            (at(t, 10), at(t, 40)),
            (at(t, 20), at(t, 30)),
            (at(t, 35), at(t, 50)),
            (at(t, 90), at(t, 150)),
        ];
        assert_eq!(overhead(&engine, &waits), Duration::from_millis(50));
        assert_eq!(overhead(&engine, &[]), Duration::from_millis(100));
        assert_eq!(overhead(&engine, &[(at(t, 0), at(t, 200))]), Duration::ZERO);
    }

    /// A streaming read's fold time stays local work: only the gaps between
    /// folds are waits.
    #[test]
    fn streaming_fold_time_is_not_a_wait() {
        let telemetry = ExecutionTelemetry::start(ExecutionMode::Direct);
        let engine_start = Instant::now();
        let mut waits = StreamWaits::open(&telemetry);
        waits.folding();
        std::thread::sleep(Duration::from_millis(20));
        waits.folded();
        waits.close();
        let engine_end = Instant::now();
        let local = overhead(&[(engine_start, engine_end)], &telemetry.waits.lock());
        assert!(local >= Duration::from_millis(20), "{local:?}");
    }

    /// A direct execution dropped before finishing — a client disconnect —
    /// records one `cancelled` failure; an abandoned queued attempt does not.
    ///
    /// # Panics
    /// Panics when the rendered exposition lacks the direct cancellation or
    /// carries a queued one.
    #[test]
    fn abandoned_direct_execution_finishes_cancelled() {
        let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        metrics::with_local_recorder(&recorder, || {
            drop(ExecutionTelemetry::start(ExecutionMode::Direct));
            drop(ExecutionTelemetry::start(ExecutionMode::Queued));
        });
        let text = handle.render();
        assert!(
            text.contains(
                r#"wyrd_verification_run_failures_total{kind="unknown",mode="direct",outcome="cancelled"} 1"#
            ),
            "{text}"
        );
        assert!(
            !text.contains(r#"mode="queued",outcome="cancelled""#),
            "{text}"
        );
    }
}
