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
#[cfg(feature = "bench")]
use std::sync::Arc;
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
/// `Engine` is the inclusive parent of `Prepare`, and of `Setup` in a queued
/// execution, and the blocks the
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
    /// Construction of the execution's hosted block executors and engine.
    Setup,
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
    const ALL: [Self; 8] = [
        Self::Admit,
        Self::Load,
        Self::InputRead,
        Self::Setup,
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
            Self::Setup => "setup",
            Self::Prepare => "prepare",
            Self::Engine => "engine",
            Self::Stage => "stage",
            Self::Settlement => "settlement",
        }
    }

    /// The benchmark timing boundary of this phase of a direct execution,
    /// recorded as a part of its `verification.direct` observation.
    #[cfg(feature = "bench")]
    const fn direct_boundary(self) -> &'static str {
        match self {
            Self::Admit => "verification.direct.admit",
            Self::Load => "verification.direct.load",
            Self::InputRead => "verification.direct.input_read",
            Self::Setup => "verification.direct.setup",
            Self::Prepare => "verification.direct.prepare",
            Self::Engine => "verification.direct.engine",
            Self::Stage => "verification.direct.stage",
            Self::Settlement => "verification.direct.settlement",
        }
    }
}

/// The benchmark timing boundary of a direct execution's time outside
/// every phase: its HTTP decode, middleware, and any wait between phases.
#[cfg(feature = "bench")]
const DIRECT_OUTSIDE_BOUNDARY: &str = "verification.direct.outside";

/// Diagnostic parts that split [`DIRECT_OUTSIDE_BOUNDARY`] of an execution
/// with at least one phase, summing to it: the time before its first phase
/// (arrival, middleware, decode), the gaps between phases, and the time
/// after its last phase (result construction and teardown).
#[cfg(feature = "bench")]
const DIRECT_OUTSIDE_SPLIT: [&str; 3] = [
    "verification.direct.outside.before",
    "verification.direct.outside.between",
    "verification.direct.outside.after",
];

/// Measured intervals per phase, indexed like [`Phase::ALL`].
type Ledger = [Vec<Interval>; 8];

/// Nested Agent work a benchmark attributes a direct execution's engine
/// time to. Envelopes are inclusive: a tool call holds its sandbox
/// operation, a model call holds its Gateway phases and provider waits, and
/// concurrent Agents overlap, so envelopes are never summed or subtracted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Envelope {
    /// Materializing an Agent invocation's workspace, admission included.
    Workspace,
    /// One built-in tool call, from authorization until its result.
    Tool,
    /// One `code.run` sandbox operation: admission, worker checkout, the
    /// program's run, worker IPC, and its host waits.
    Sandbox,
    /// One sandbox operation's wait for process memory, from its retained
    /// bytes' hold until its grant or refusal, including idle workers it
    /// reaps to make room; nested in its sandbox envelope.
    Capacity,
    /// One sandbox turn's wait for a runnable permit, from asking until the
    /// turn holds one; nested in its sandbox envelope.
    Runnable,
    /// One in-process governed model call through the Gateway.
    Model,
}

impl Envelope {
    /// Every envelope, in emission order.
    #[cfg(feature = "bench")]
    const ALL: [Self; 6] = [
        Self::Workspace,
        Self::Tool,
        Self::Sandbox,
        Self::Capacity,
        Self::Runnable,
        Self::Model,
    ];

    /// The benchmark timing boundary of this envelope of a direct
    /// execution, recorded as a part of its `verification.direct`
    /// observation.
    #[cfg(feature = "bench")]
    const fn direct_boundary(self) -> &'static str {
        match self {
            Self::Workspace => "verification.direct.workspace",
            Self::Tool => "verification.direct.tool",
            Self::Sandbox => "verification.direct.sandbox",
            Self::Capacity => "verification.direct.capacity",
            Self::Runnable => "verification.direct.runnable",
            Self::Model => "verification.direct.model",
        }
    }
}

/// One execution's record of [`Envelope`] intervals; cloning shares it.
///
/// The hosted tools and the Gateway caller time their work through it. A
/// `bench` build allocates a record only for a direct execution that starts
/// while a benchmark recorder is installed; otherwise, and for every
/// [`Default`] record a constructor holds until the execution's replaces it,
/// it holds nothing and reads no clock. Without `bench` it is a zero-sized
/// no-op. Only a direct execution's profile reads it.
#[derive(Debug, Clone, Default)]
pub struct Envelopes(#[cfg(feature = "bench")] Option<Arc<Mutex<[Vec<Interval>; 6]>>>);

impl Envelopes {
    /// A record for a `mode` execution: holding intervals only for a direct
    /// execution of a `bench` build with the recorder installed.
    fn of(mode: ExecutionMode) -> Self {
        #[cfg(feature = "bench")]
        return Self(
            (mode == ExecutionMode::Direct && wyrd_benchmark::enabled()).then(Arc::default),
        );
        #[cfg(not(feature = "bench"))]
        {
            let _ = mode;
            Self()
        }
    }

    /// Await `future` as one interval of `envelope`; a future dropped
    /// before completion records nothing.
    pub fn time<F: Future>(
        &self,
        envelope: Envelope,
        future: F,
    ) -> impl Future<Output = F::Output> {
        #[cfg(feature = "bench")]
        let start = self.0.is_some().then(Instant::now);
        #[cfg(not(feature = "bench"))]
        let _ = envelope;
        future.map(move |output| {
            #[cfg(feature = "bench")]
            if let Some(start) = start {
                self.record(envelope, start, Instant::now());
            }
            output
        })
    }

    /// Record one `[start, end)` interval of `envelope` into a held record.
    pub fn record(&self, envelope: Envelope, start: Instant, end: Instant) {
        #[cfg(not(feature = "bench"))]
        let _ = (envelope, start, end);
        #[cfg(feature = "bench")]
        if let Some(record) = &self.0 {
            let index = Envelope::ALL
                .iter()
                .position(|candidate| *candidate == envelope)
                .unwrap_or_default();
            record
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)[index]
                .push((start, end));
        }
    }

    /// Each reached envelope's part and the union of its intervals.
    #[cfg(feature = "bench")]
    fn parts(&self) -> Vec<(&'static str, Duration)> {
        let Some(record) = &self.0 else {
            return Vec::new();
        };
        let recorded = record
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Envelope::ALL
            .iter()
            .zip(recorded.iter())
            .filter(|(_, intervals)| !intervals.is_empty())
            .map(|(envelope, intervals)| (envelope.direct_boundary(), union(intervals)))
            .collect()
    }
}

/// Owner of one verification execution's metrics lifetime.
///
/// Created when a direct request reaches the server or a queued attempt is
/// claimed; it raises the active gauge at once and lowers it exactly once on
/// drop. The attempt is counted when its outcome is emitted, so a queued
/// claim dropped without [`finish`](Self::finish) — a deferral that spends
/// no attempt — emits no attempt series. A direct execution dropped before
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
    /// The nested Agent work inside the engine, while benchmarking.
    envelopes: Envelopes,
    /// Whether the execution's outcome was emitted.
    finished: bool,
}

impl ExecutionTelemetry {
    /// Start accounting for one `mode` execution from `since` — a direct
    /// request's HTTP arrival at this server, or a queued run's claim — or
    /// from now when there is neither (an MCP call): raise the active gauge.
    #[must_use]
    pub fn start(mode: ExecutionMode, since: Option<Instant>) -> Self {
        metrics::gauge!(VERIFICATION_ACTIVE_RUNS, "mode" => mode.as_str()).increment(1.0);
        Self {
            mode,
            started: since.unwrap_or_else(Instant::now),
            ledger: Mutex::default(),
            clock: BlockClock::default(),
            envelopes: Envelopes::of(mode),
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

    /// The record the hosted tools and Gateway caller time this
    /// execution's nested Agent work on.
    #[must_use]
    pub fn envelopes(&self) -> Envelopes {
        self.envelopes.clone()
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

    /// Emit the outcome families of [`finish`](Self::finish): the attempt
    /// count, each phase, the whole execution, the Verifier's block time,
    /// and the overhead.
    ///
    /// While benchmarking, a direct execution's duration and block union are
    /// recorded instead through [`wyrd_benchmark::record_timing_parts`]
    /// under the current span's benchmark classification at boundary
    /// `verification.direct`, with each measured phase at its
    /// [`Phase::direct_boundary`] and the time outside every phase at
    /// [`DIRECT_OUTSIDE_BOUNDARY`], so each duration is counted once, then
    /// that outside time's [`DIRECT_OUTSIDE_SPLIT`] diagnostic parts, plus
    /// each reached [`Envelope`]'s union nested inside the engine; no
    /// benchmark reads a queued execution's timing, so it keeps its
    /// histograms.
    fn emit(&self, outcome: &'static str, failed: bool) {
        let total = self.started.elapsed();
        let ledger = self.lock();
        let mode = self.mode.as_str();
        metrics::counter!(VERIFICATION_RUN_ATTEMPTS_TOTAL, "mode" => mode).increment(1);
        if failed {
            metrics::counter!(
                VERIFICATION_RUN_FAILURES_TOTAL,
                "mode" => mode,
                "outcome" => outcome
            )
            .increment(1);
        }
        let verifier = union(&self.clock.intervals());
        #[cfg(feature = "bench")]
        if self.mode == ExecutionMode::Direct && wyrd_benchmark::enabled() {
            let mut parts = Vec::with_capacity(Phase::ALL.len() + Envelope::ALL.len() + 4);
            for (phase, intervals) in Phase::ALL.iter().zip(ledger.iter()) {
                if !intervals.is_empty() {
                    parts.push((phase.direct_boundary(), union(intervals)));
                }
            }
            let phased: Vec<Interval> = ledger.iter().flatten().copied().collect();
            let outside = total.saturating_sub(union(&phased));
            parts.push((DIRECT_OUTSIDE_BOUNDARY, outside));
            let first = phased.iter().map(|(start, _)| *start).min();
            let last = phased.iter().map(|(_, end)| *end).max();
            if let (Some(first), Some(last)) = (first, last) {
                let before = first.saturating_duration_since(self.started);
                let after = (self.started + total).saturating_duration_since(last);
                parts.extend([
                    (DIRECT_OUTSIDE_SPLIT[0], before),
                    (
                        DIRECT_OUTSIDE_SPLIT[1],
                        outside.saturating_sub(before).saturating_sub(after),
                    ),
                    (DIRECT_OUTSIDE_SPLIT[2], after),
                ]);
            }
            parts.extend(self.envelopes.parts());
            wyrd_benchmark::record_timing_parts(
                &tracing::Span::current(),
                "verification.direct",
                outcome,
                total,
                Some(verifier),
                &parts,
            );
            return;
        }
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
        metrics::histogram!(VERIFICATION_VERIFIER_SECONDS, "mode" => mode, "outcome" => outcome)
            .record(verifier.as_secs_f64());
        metrics::histogram!(VERIFICATION_OVERHEAD_SECONDS, "mode" => mode, "outcome" => outcome)
            .record(total.saturating_sub(verifier).as_secs_f64());
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

    /// Without an installed benchmark recorder neither an execution nor a
    /// hosted tool or Gateway caller's default record creates envelope
    /// state, and timing work through them records nothing.
    ///
    /// # Panics
    /// Panics when a disabled execution or default record holds state.
    #[test]
    fn disabled_envelopes_do_not_create_records() {
        assert!(!wyrd_benchmark::enabled());
        let telemetry = ExecutionTelemetry::start(ExecutionMode::Direct, None);
        let envelopes = telemetry.envelopes();
        let start = Instant::now();
        envelopes.record(Envelope::Model, start, start + Duration::from_millis(1));
        assert!(
            envelopes.0.is_none(),
            "a disabled execution holds no record"
        );
        assert!(
            Envelopes::default().0.is_none(),
            "a default record holds none"
        );
        telemetry.finish("completed", false);
    }

    /// A direct execution's hosted setup is its own part between load and
    /// engine: it is counted once, never as time outside every phase, and
    /// the execution's total is unchanged.
    ///
    /// # Panics
    /// Panics when the setup part is missing or mismeasured, or the outside
    /// part absorbs it.
    #[test]
    fn hosted_setup_is_a_direct_part_outside_the_engine() {
        wyrd_benchmark::install().expect("the benchmark recorder installs");
        let before = wyrd_benchmark::snapshot();
        let start = Instant::now()
            .checked_sub(Duration::from_millis(100))
            .expect("invariant: the monotonic clock is past 100 ms");
        let telemetry = ExecutionTelemetry::start(ExecutionMode::Direct, Some(start));
        telemetry.record_phase(Phase::Load, at(start, 0), at(start, 10));
        telemetry.record_phase(Phase::Setup, at(start, 10), at(start, 40));
        telemetry.record_phase(Phase::Engine, at(start, 40), at(start, 90));
        telemetry.finish("completed", false);
        let interval = wyrd_benchmark::snapshot()
            .since(&before)
            .expect("one process");
        let part = |boundary: &str| {
            interval
                .series
                .iter()
                .find(|series| series.boundary == boundary)
                .unwrap_or_else(|| panic!("no {boundary}: {:?}", interval.series))
        };
        let within = |boundary: &str, low_ms: u64, high_ms: u64| {
            let series = part(boundary);
            assert_eq!(series.count, 1, "{boundary}");
            series.elapsed.maximum().is_some_and(|bucket| {
                bucket.highest >= low_ms * 1_000_000 && bucket.lowest <= high_ms * 1_000_000
            })
        };
        assert!(within("verification.direct.setup", 30, 30));
        assert!(
            within("verification.direct.outside", 10, 20),
            "outside excludes setup"
        );
        assert!(within("verification.direct", 100, 110));
    }

    /// A direct execution's time outside every phase is split, as
    /// diagnostic parts, into the time before its first phase (arrival,
    /// middleware, decode), the gaps between phases, and the time after its
    /// last phase (result construction and teardown); the three sum to the
    /// unchanged outside part.
    ///
    /// # Panics
    /// Panics when a split part is missing or mismeasured.
    #[test]
    fn outside_time_splits_around_the_phases() {
        wyrd_benchmark::install().expect("the benchmark recorder installs");
        let before = wyrd_benchmark::snapshot();
        let start = Instant::now()
            .checked_sub(Duration::from_millis(100))
            .expect("invariant: the monotonic clock is past 100 ms");
        let telemetry = ExecutionTelemetry::start(ExecutionMode::Direct, Some(start));
        telemetry.record_phase(Phase::Admit, at(start, 20), at(start, 30));
        telemetry.record_phase(Phase::Engine, at(start, 45), at(start, 60));
        telemetry.finish("completed", false);
        let interval = wyrd_benchmark::snapshot()
            .since(&before)
            .expect("one process");
        let within = |boundary: &str, low_ms: u64, high_ms: u64| {
            let series = interval
                .series
                .iter()
                .find(|series| series.boundary == boundary)
                .unwrap_or_else(|| panic!("no {boundary}: {:?}", interval.series));
            assert_eq!(series.count, 1, "{boundary}");
            series.elapsed.maximum().is_some_and(|bucket| {
                bucket.highest >= low_ms * 1_000_000 && bucket.lowest <= high_ms * 1_000_000
            })
        };
        assert!(within("verification.direct.outside.before", 20, 20));
        assert!(within("verification.direct.outside.between", 15, 15));
        assert!(within("verification.direct.outside.after", 40, 50));
        assert!(within("verification.direct.outside", 75, 85));
    }

    /// A direct execution's nested Agent envelopes (workspace setup, each
    /// tool call, the sandbox operation inside `code.run`, its wait for
    /// process memory, and each model call) are each one part per execution, the union of their intervals,
    /// so two Agents' overlapping model calls count once. They nest inside
    /// the engine and change neither the execution's total, its Verifier
    /// exclusion, nor the time outside every phase; an envelope the
    /// execution never reached records no part rather than a zero.
    ///
    /// # Panics
    /// Panics when an envelope part is missing, mismeasured, or counted for
    /// an execution that never reached it, or when it changes the total or
    /// the outside part.
    #[test]
    fn canonical_phases_explain_total_latency() {
        wyrd_benchmark::install().expect("the benchmark recorder installs");
        let before = wyrd_benchmark::snapshot();
        let start = Instant::now()
            .checked_sub(Duration::from_millis(200))
            .expect("invariant: the monotonic clock is past 200 ms");
        let telemetry = ExecutionTelemetry::start(ExecutionMode::Direct, Some(start));
        telemetry.record_phase(Phase::Engine, at(start, 10), at(start, 190));
        let envelopes = telemetry.envelopes();
        for (envelope, from, to) in [
            (Envelope::Workspace, 20, 25),
            (Envelope::Workspace, 40, 45),
            (Envelope::Model, 30, 60),
            (Envelope::Model, 40, 70),
            (Envelope::Model, 120, 150),
            (Envelope::Tool, 80, 110),
            (Envelope::Sandbox, 85, 105),
            (Envelope::Capacity, 86, 90),
            (Envelope::Runnable, 91, 93),
            (Envelope::Runnable, 92, 94),
        ] {
            envelopes.record(envelope, at(start, from), at(start, to));
        }
        telemetry.finish("completed", false);
        ExecutionTelemetry::start(ExecutionMode::Direct, None).finish("completed", false);
        let interval = wyrd_benchmark::snapshot()
            .since(&before)
            .expect("one process");
        let part = |boundary: &str| {
            interval
                .series
                .iter()
                .find(|series| series.boundary == boundary)
                .unwrap_or_else(|| panic!("no {boundary}: {:?}", interval.series))
        };
        let within = |boundary: &str, count: u64, low_ms: u64, high_ms: u64| {
            let series = part(boundary);
            assert_eq!(series.count, count, "{boundary}");
            series.elapsed.maximum().is_some_and(|bucket| {
                bucket.highest >= low_ms * 1_000_000 && bucket.lowest <= high_ms * 1_000_000
            })
        };
        assert!(within("verification.direct.workspace", 1, 10, 10));
        assert!(
            within("verification.direct.model", 1, 70, 70),
            "overlapping model calls count once"
        );
        assert!(within("verification.direct.tool", 1, 30, 30));
        assert!(within("verification.direct.sandbox", 1, 20, 20));
        assert!(
            within("verification.direct.capacity", 1, 4, 4),
            "the capacity wait is its own named part"
        );
        assert!(
            within("verification.direct.runnable", 1, 3, 3),
            "overlapping runnable waits count once"
        );
        assert!(within("verification.direct.engine", 1, 180, 180));
        assert!(
            within("verification.direct.outside", 2, 20, 30),
            "envelopes nest in the engine and never reduce the outside part"
        );
        assert!(
            within("verification.direct", 2, 200, 210),
            "envelopes never reduce the total"
        );
    }
}
