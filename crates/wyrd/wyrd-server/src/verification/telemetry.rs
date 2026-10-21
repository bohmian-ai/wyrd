//! Per-execution verification profile: classification, phases, overhead.
//!
//! [`ExecutionTelemetry`] is the one owner of a Verifier execution's metric
//! lifetime, whether a queued attempt or a direct request: the runtime's
//! analog of Bifrost's per-query profile. It starts when the work reaches
//! the runtime (a direct request's HTTP arrival, a queued attempt's claim),
//! accumulates each [`Phase`]'s scoped intervals, and hands the engine a
//! [`BlockClock`] that records every action and check the Verifier runs.
//! Finishing emits one profile: the whole execution, the Verifier's own
//! block time, and the overhead between them (the execution less the union
//! of its blocks), plus at most one sample per applicable phase. Dropping
//! it unfinished, on error, cancellation, or shutdown, still returns the
//! active gauge exactly once. Every Verifier runs through the one
//! building-block engine, so series are labelled by `mode` only.

use std::future::Future;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures_util::FutureExt as _;
use tracing::Instrument as _;
use vala_verify::blocks::BlockClock;

use crate::app::metrics::{
    VERIFICATION_ACTIVE_RUNS, VERIFICATION_OVERHEAD_SECONDS, VERIFICATION_PHASE_DURATION_SECONDS,
    VERIFICATION_RUN_ATTEMPTS_TOTAL, VERIFICATION_RUN_DURATION_SECONDS,
    VERIFICATION_RUN_FAILURES_TOTAL, VERIFICATION_SUBMIT_SECONDS, VERIFICATION_VERIFIER_SECONDS,
};
use crate::http::middleware::metrics::RequestArrival;

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
/// `Engine` is the inclusive parent of `Prepare`, and the blocks the
/// Verifier runs inside it are its own execution, not overhead; phases
/// overlap and are never summed. A direct execution's time outside every
/// phase is its HTTP decode and the request's middleware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Authorize the caller and reserve the result's staging slots.
    Admit,
    /// Resolve the exact registered Verifier.
    Load,
    /// Authorized queued evidence retrieval; direct execution has none.
    InputRead,
    /// Input normalization, spec/baseline decoding, and plan construction.
    Prepare,
    /// Inclusive engine dispatch through judgment.
    Engine,
    /// Stage a direct execution's inputs and result on the Scribe outbox.
    Stage,
    /// Queued fenced durable state transition.
    Settlement,
}

impl Phase {
    /// Every phase, in emission order.
    const ALL: [Self; 7] = [
        Self::Admit,
        Self::Load,
        Self::InputRead,
        Self::Prepare,
        Self::Engine,
        Self::Stage,
        Self::Settlement,
    ];

    /// The stable metric label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admit => "admit",
            Self::Load => "load",
            Self::InputRead => "input_read",
            Self::Prepare => "prepare",
            Self::Engine => "engine",
            Self::Stage => "stage",
            Self::Settlement => "settlement",
        }
    }
}

/// Measured intervals per phase, indexed like [`Phase::ALL`].
type Ledger = [Vec<Interval>; 7];

/// Owner of one verification execution's metrics lifetime.
///
/// Created when a direct request reaches the server or a queued attempt is
/// claimed; it counts the attempt and raises the active gauge at once and
/// lowers the gauge exactly once on drop. A direct execution dropped before
/// [`finish`](Self::finish), a client disconnect, finishes as `cancelled`.
#[derive(Debug)]
pub struct ExecutionTelemetry {
    /// How the execution was admitted.
    mode: ExecutionMode,
    /// When the execution reached the runtime.
    started: Instant,
    /// Measured phase intervals.
    ledger: Mutex<Ledger>,
    /// Every block the engine ran for this execution.
    clock: BlockClock,
    /// Whether the execution's outcome was emitted.
    finished: bool,
}

impl ExecutionTelemetry {
    /// Start accounting for one `mode` execution from `since` — a direct
    /// request's HTTP arrival at this server, or a queued run's claim — or
    /// from now when there is neither (an MCP call): count the attempt and
    /// raise the active gauge.
    #[must_use]
    pub fn start(mode: ExecutionMode, since: Option<Instant>) -> Self {
        metrics::counter!(VERIFICATION_RUN_ATTEMPTS_TOTAL, "mode" => mode.as_str()).increment(1);
        metrics::gauge!(VERIFICATION_ACTIVE_RUNS, "mode" => mode.as_str()).increment(1.0);
        Self {
            mode,
            started: since.unwrap_or_else(Instant::now),
            ledger: Mutex::default(),
            clock: BlockClock::default(),
            finished: false,
        }
    }

    /// The execution's mode.
    #[must_use]
    pub const fn mode(&self) -> ExecutionMode {
        self.mode
    }

    /// The clock the engine records this execution's blocks on.
    #[must_use]
    pub fn clock(&self) -> BlockClock {
        self.clock.clone()
    }

    /// Await `future` as one interval of `phase`.
    ///
    /// The interval starts when this is called and is recorded when `future`
    /// completes; a future dropped before completion records nothing. This
    /// is a combinator rather than an `async fn` so `future` is held once.
    pub fn phase<F: Future>(&self, phase: Phase, future: F) -> impl Future<Output = F::Output> {
        let start = Instant::now();
        future.map(move |output| {
            self.record_phase(phase, start, Instant::now());
            output
        })
    }

    /// Run `work` synchronously as one interval of `phase`.
    pub fn time<T>(&self, phase: Phase, work: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let output = work();
        self.record_phase(phase, start, Instant::now());
        output
    }

    /// Await `future` as one `prepare` interval inside a `verification.prepare` span.
    pub fn prepare<F: Future>(&self, future: F) -> impl Future<Output = F::Output> {
        self.phase(
            Phase::Prepare,
            future.instrument(tracing::info_span!("verification.prepare")),
        )
    }

    /// Record one `[start, end)` interval of `phase`.
    pub fn record_phase(&self, phase: Phase, start: Instant, end: Instant) {
        let index = Phase::ALL
            .iter()
            .position(|candidate| *candidate == phase)
            .unwrap_or_default();
        self.lock()[index].push((start, end));
    }

    /// Emit the execution's profile under `outcome`.
    ///
    /// `failed` marks an unsuccessful execution outcome; a completed failed
    /// judgment is not one. Phases with no interval emit no sample. The
    /// active gauge is lowered when `self` drops at the end of this call.
    pub fn finish(mut self, outcome: &'static str, failed: bool) {
        self.emit(outcome, failed);
        self.finished = true;
    }

    /// Emit the outcome families of [`finish`](Self::finish): each phase,
    /// the whole execution, the Verifier's block time, and the overhead.
    fn emit(&self, outcome: &'static str, failed: bool) {
        let total = self.started.elapsed();
        let ledger = self.lock();
        let mode = self.mode.as_str();
        for (phase, intervals) in Phase::ALL.iter().zip(ledger.iter()) {
            if !intervals.is_empty() {
                metrics::histogram!(
                    VERIFICATION_PHASE_DURATION_SECONDS,
                    "mode" => mode,
                    "phase" => phase.as_str()
                )
                .record(union(intervals).as_secs_f64());
            }
        }
        let verifier = union(&self.clock.intervals());
        metrics::histogram!(VERIFICATION_VERIFIER_SECONDS, "mode" => mode, "outcome" => outcome)
            .record(verifier.as_secs_f64());
        metrics::histogram!(VERIFICATION_OVERHEAD_SECONDS, "mode" => mode, "outcome" => outcome)
            .record(total.saturating_sub(verifier).as_secs_f64());
        if failed {
            metrics::counter!(
                VERIFICATION_RUN_FAILURES_TOTAL,
                "mode" => mode,
                "outcome" => outcome
            )
            .increment(1);
        }
        metrics::histogram!(
            VERIFICATION_RUN_DURATION_SECONDS,
            "mode" => mode,
            "outcome" => outcome
        )
        .record(total.as_secs_f64());
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
    /// active gauge exactly once.
    fn drop(&mut self) {
        if !self.finished && self.mode == ExecutionMode::Direct {
            self.emit("cancelled", true);
        }
        metrics::gauge!(VERIFICATION_ACTIVE_RUNS, "mode" => self.mode.as_str()).decrement(1.0);
    }
}

/// Record one background submission's server time under `outcome`: from
/// its HTTP `arrival` until it was staged or refused. A submission that did
/// not arrive over HTTP (an MCP tool call) has no arrival and records none.
pub fn record_submission(arrival: Option<RequestArrival>, outcome: &'static str) {
    if let Some(arrival) = arrival {
        metrics::histogram!(VERIFICATION_SUBMIT_SECONDS, "outcome" => outcome)
            .record(arrival.instant().elapsed().as_secs_f64());
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

#[cfg(test)]
mod tests {
    //! Proof of the profile's overhead arithmetic and cancellation.

    use super::*;

    /// Offset `ms` milliseconds from `base`.
    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    /// Overlapping intervals count once and disjoint ones add.
    #[test]
    fn union_counts_overlap_once() {
        let t = Instant::now();
        let intervals = [
            (at(t, 10), at(t, 40)),
            (at(t, 20), at(t, 30)),
            (at(t, 35), at(t, 50)),
            (at(t, 90), at(t, 100)),
        ];
        assert_eq!(union(&intervals), Duration::from_millis(50));
        assert_eq!(union(&[]), Duration::ZERO);
    }

    /// The overhead is the whole execution from its arrival less the
    /// Verifier's block time: a profile started at an earlier arrival
    /// counts the time before it, and block time is subtracted.
    ///
    /// # Panics
    /// Panics when the rendered exposition lacks either family or the
    /// overhead omits the time since arrival.
    #[test]
    fn overhead_is_the_execution_less_its_blocks() {
        let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        let arrival = Instant::now()
            .checked_sub(Duration::from_millis(30))
            .expect("invariant: the monotonic clock is past 30 ms");
        metrics::with_local_recorder(&recorder, || {
            ExecutionTelemetry::start(ExecutionMode::Direct, Some(arrival))
                .finish("completed", false);
        });
        let text = handle.render();
        let sum = |family: &str| {
            text.lines()
                .find(|line| {
                    line.starts_with(&format!(
                        "{family}_sum{{mode=\"direct\",outcome=\"completed\"}}"
                    ))
                })
                .and_then(|line| line.rsplit(' ').next())
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or_else(|| panic!("{family} missing: {text}"))
        };
        assert!(sum("wyrd_verification_overhead_seconds") >= 0.030, "{text}");
        assert!(sum("wyrd_verification_verifier_seconds") < 0.030, "{text}");
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
            drop(ExecutionTelemetry::start(ExecutionMode::Direct, None));
            drop(ExecutionTelemetry::start(ExecutionMode::Queued, None));
        });
        let text = handle.render();
        assert!(
            text.contains(
                r#"wyrd_verification_run_failures_total{mode="direct",outcome="cancelled"} 1"#
            ),
            "{text}"
        );
        assert!(
            !text.contains(r#"mode="queued",outcome="cancelled""#),
            "{text}"
        );
    }
}
