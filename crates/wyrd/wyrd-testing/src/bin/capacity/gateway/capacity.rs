//! Gateway L2 `capacity`: open-loop contention through capacity, overload,
//! and recovery.
//!
//! Every repetition (and the warmup) is one fixed schedule from the settled
//! server: the declared rate for the capacity phase, [`OVERLOAD_FACTOR`]
//! times it for the overload phase, and the declared rate again for
//! recovery, whose calls after [`RECOVERY_GRACE`] are judged. [`Schedule`]
//! places every arrival in advance and [`open_loop`] launches each at its
//! instant whatever the responses do: an arrival more than [`MISS_AFTER`]
//! late is missed and one beyond [`MAX_IN_FLIGHT`] open calls is capped,
//! //! never replayed or queued. Every call declares its mode's case and the
//! phase that admitted it. Work, seconds, and CPU stop at the schedule's
//! end; calls then in flight drain into their cohort and count late. The
//! SLOs judge the server's own timing histograms (from handler entry to the
//! buffered answer, stream head, or final frame handoff) of each admitted
//! phase's cohort once every call drained, every tenant together: tenants
//! are never a snapshot label. Client chunk lag, each data event's receipt behind its
//! [`PacedUpstream`] send, is diagnostic. A sampler reads in-flight calls, active server calls, and
//! outbox backlog every second, and scrapes at each phase boundary attribute
//! shedding to the phase that caused it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use wyrd_testing::release_server::PEAK_SAMPLE_INTERVAL_MS;

use async_trait::async_trait;
use num_traits::ToPrimitive;
use reqwest::Client;
use secrecy::SecretString;
use serde_json::{Value, json};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use wyrd_client::transport::trace::with_benchmark_labels;
use wyrd_testing::capacity::{
    Expected, NANOS_PER_MS, Percentiles, Population, Selection, Usage, Window, release_binary,
};
use wyrd_testing::experiment::{
    Arrivals, ExperimentError, Phase, Reading, Repetition, Step, Workload,
};
use wyrd_testing::gateway_capture::{PacedUpstream, Pacing};
use wyrd_testing::release_server::{LocalServer, Metrics};

use crate::gateway::run::{
    BUFFERED, Drain, Durable, Integrity, STREAM_END, STREAM_HEAD, SUCCEEDED, outcome_key, settle,
};
use crate::gateway::workload::{CapturePolicy, Shape, Tenant, Timed, call, http};
use crate::gateway::{Case, PROCESS, Result, param, shutdown};

/// The overload phase's rate over the declared rate.
pub const OVERLOAD_FACTOR: f64 = 2.0;

/// Recovery calls scheduled this soon after overload ends are not judged
/// against the SLOs: the gateway has this long to recover.
pub const RECOVERY_GRACE: Duration = Duration::from_secs(30);

/// The leading and trailing interval of the capacity phase whose backlog is
/// compared.
pub const EDGE: Duration = Duration::from_secs(3);

/// Calls the driver holds open at once; an arrival beyond it is capped.
pub const MAX_IN_FLIGHT: usize = 16_384;

/// Lateness past which an arrival is missed rather than sent late.
pub const MISS_AFTER: Duration = Duration::from_millis(20);

/// Warmed keys each tenant's arrivals rotate through.
pub const KEYS_PER_TENANT: usize = 8;

/// The tenants `setup` provisions, in [`contention`] order.
pub const TENANTS: [&str; 4] = ["bench-a", "bench-b", "bench-c", "bench-d"];

/// The admission refusals a configured gateway limit or budget answers;
/// the only way an overload call may end unanswered.
pub const ADMISSION_REFUSALS: [&str; 2] = [
    "WYRD_GATEWAY_429_LIMIT_EXCEEDED",
    "WYRD_GATEWAY_429_BUDGET_EXCEEDED",
];

/// How often the backlog sampler reads.
const SAMPLE_EVERY: Duration = Duration::from_secs(1);

/// Lead before a schedule's first arrival, so the driver starts on time.
const LEAD: Duration = Duration::from_millis(50);

/// The deterministic upstream every call reaches.
pub const PACING: Pacing = Pacing {
    buffered_delay: Duration::from_millis(100),
    buffered_bytes: 1024,
    stream_delay: Duration::from_millis(250),
    chunks: 32,
    chunk_interval: Duration::from_millis(20),
};

/// The tenant and mode of arrival `index`: ten slots per cycle, seven for
/// the first tenant and one for each other, each slot's mode alternating
/// every cycle so every tenant sends both modes equally.
#[must_use]
pub const fn contention(index: u64) -> (usize, bool) {
    let slot = index % 10;
    let tenant = match slot {
        7 => 1,
        8 => 2,
        9 => 3,
        _ => 0,
    };
    (tenant, (index / 10 + slot) % 2 == 1)
}

/// The p99 SLOs every capacity and recovery phase is judged by,
/// milliseconds of exact server time from handler entry, and which of them
/// the invocation overrode.
#[derive(Debug, Clone, PartialEq)]
pub struct Slo {
    /// Until the buffered answer's handoff (`gateway.buffered`).
    pub completion: f64,
    /// Until a stream's head handoff (`gateway.stream.head`).
    pub first_token: f64,
    /// Until a stream's final frame handoff (`gateway.stream.end`).
    pub stream: f64,
    /// The `--max-*-ms` flags given, fixed in the declaration's controls.
    pub overrides: Vec<String>,
}

impl Slo {
    /// The defaults with any `--max-completion-ms`, `--max-first-token-ms`,
    /// or `--max-stream-ms` override applied.
    ///
    /// # Errors
    ///
    /// Returns a message for a repeated flag or a bound that is not a
    /// positive finite number.
    pub fn parse(arguments: &[String]) -> std::result::Result<Self, String> {
        let mut overrides = Vec::new();
        let mut bound = |flag: &str, default: f64| -> std::result::Result<f64, String> {
            let Some(value) = crate::flags::value(arguments, flag)? else {
                return Ok(default);
            };
            overrides.push(flag.to_owned());
            value
                .parse::<f64>()
                .ok()
                .filter(|ms| ms.is_finite() && *ms > 0.0)
                .ok_or_else(|| format!("{flag} takes positive finite milliseconds, got `{value}`"))
        };
        let (completion, first_token, stream) = (
            bound("--max-completion-ms", 200.0)?,
            bound("--max-first-token-ms", 350.0)?,
            bound("--max-stream-ms", 1_100.0)?,
        );
        Ok(Self {
            completion,
            first_token,
            stream,
            overrides,
        })
    }

    /// The bounds as the declaration records them.
    #[must_use]
    pub fn limits(&self) -> Value {
        json!({
            "completion": self.completion,
            "first_token": self.first_token,
            "stream": self.stream,
        })
    }
}

/// How one repetition's arrival window splits into phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Phases {
    /// The declared rate, from the schedule's start.
    pub capacity: Duration,
    /// [`OVERLOAD_FACTOR`] times the rate, after capacity.
    pub overload: Duration,
    /// The declared rate again, after overload.
    pub recovery: Duration,
    /// The start of recovery whose calls are not judged.
    pub grace: Duration,
}

impl Phases {
    /// `window` split two thirds capacity, one sixth overload, the rest
    /// recovery, with [`RECOVERY_GRACE`] (at most half the recovery) unjudged.
    #[must_use]
    pub fn of(window: Duration) -> Self {
        let capacity = window * 2 / 3;
        let overload = window / 6;
        let recovery = window.saturating_sub(capacity + overload);
        Self {
            capacity,
            overload,
            recovery,
            grace: RECOVERY_GRACE.min(recovery / 2),
        }
    }

    /// Where overload ends.
    #[must_use]
    pub fn overload_end(&self) -> Duration {
        self.capacity + self.overload
    }

    /// Where judged recovery starts.
    #[must_use]
    pub fn judged(&self) -> Duration {
        self.overload_end() + self.grace
    }

    /// The phase a call scheduled at `offset` belongs to.
    #[must_use]
    pub fn phase(&self, offset: Duration) -> Period {
        if offset < self.capacity {
            Period::Capacity
        } else if offset < self.judged() {
            Period::Overload
        } else {
            Period::Recovery
        }
    }

    /// The schedule at `rate`: capacity, overload, then recovery.
    #[must_use]
    pub fn schedule(&self, rate: f64) -> Schedule {
        Schedule::new(vec![
            (rate, self.capacity),
            (rate * OVERLOAD_FACTOR, self.overload),
            (rate, self.recovery),
        ])
    }
}

/// The judged phase of a call; the recovery grace counts with overload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    /// Before overload.
    Capacity,
    /// Overload and the recovery grace after it.
    Overload,
    /// Judged recovery.
    Recovery,
}

impl Period {
    /// Every period, in schedule order.
    pub const ALL: [Self; 3] = [Self::Capacity, Self::Overload, Self::Recovery];

    /// The closed benchmark phase its calls declare.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Capacity => "capacity",
            Self::Overload => "overload",
            Self::Recovery => "recovery",
        }
    }
}

/// The closed benchmark case of a call of `stream` mode.
#[must_use]
pub const fn mode(stream: bool) -> Case {
    if stream { Case::ChatStream } else { Case::Chat }
}

/// The server timings of the calls `phase` admitted, both modes.
#[must_use]
pub fn cohort(population: &Population, phase: Period) -> Population {
    let mut cohort = population.only(mode(false).label(), phase.label());
    let streamed = population.only(mode(true).label(), phase.label());
    cohort.series.extend(streamed.series);
    cohort
}

/// Every arrival of a run of fixed-rate segments, placed in advance.
#[derive(Debug, Clone)]
pub struct Schedule {
    /// Segments in order: aggregate rate and duration.
    segments: Vec<(f64, Duration)>,
}

/// One scheduled arrival.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Planned {
    /// Index across the whole schedule.
    pub index: u64,
    /// Its offset from the schedule's start.
    pub offset: Duration,
}

impl Schedule {
    /// The arrivals of `segments` in order, each `1 / rate` apart.
    #[must_use]
    pub const fn new(segments: Vec<(f64, Duration)>) -> Self {
        Self { segments }
    }

    /// Every arrival in order: `rate * duration` per segment, rounded, the
    /// first at the segment's start.
    #[must_use]
    pub fn planned(&self) -> Vec<Planned> {
        let mut planned = Vec::new();
        let mut start = Duration::ZERO;
        for (rate, duration) in &self.segments {
            let count = (rate * duration.as_secs_f64())
                .round()
                .to_u64()
                .unwrap_or(0);
            for step in 0..count {
                planned.push(Planned {
                    index: planned.len() as u64,
                    offset: start + Duration::from_secs_f64(step as f64 / rate),
                });
            }
            start += *duration;
        }
        planned
    }
}

/// What the open-loop driver did with every scheduled arrival.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Driven {
    /// Arrivals scheduled.
    pub scheduled: u64,
    /// Arrivals sent.
    pub sent: u64,
    /// Arrivals dropped for being more than [`MISS_AFTER`] late, or for
    /// waking at or after the schedule's end, when measurement stopped.
    pub missed: u64,
    /// Arrivals dropped because [`MAX_IN_FLIGHT`] calls were open.
    pub capped: u64,
    /// Send lateness of each sent arrival, microseconds.
    pub lateness_us: Vec<u64>,
    /// Most calls open at once.
    pub peak_in_flight: usize,
}

/// Launches every arrival of `schedule` from `start` as its instant comes,
/// never waiting on a response: each sent arrival runs in `calls`, the
/// measurement's owned task set, and holds one of `slots` (sized `cap`)
/// until the future `launch` returns for it completes; an arrival whose
/// wake comes at or after the schedule's end is missed, never sent. Returns
/// once every sent call has completed. Dropping the future stops launching at once;
/// calls still in flight stay in `calls` for their owner to abort and join.
pub async fn open_loop<F, Fut>(
    schedule: &Schedule,
    start: Instant,
    slots: Arc<Semaphore>,
    cap: usize,
    calls: &Mutex<JoinSet<()>>,
    mut launch: F,
) -> Driven
where
    F: FnMut(Planned) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let mut driven = Driven::default();
    let end = start
        + schedule
            .segments
            .iter()
            .map(|(_, length)| *length)
            .sum::<Duration>();
    for planned in schedule.planned() {
        let at = start + planned.offset;
        tokio::time::sleep_until(at).await;
        driven.scheduled += 1;
        let woke = Instant::now();
        let late = woke.saturating_duration_since(at);
        if late > MISS_AFTER || woke >= end {
            driven.missed += 1;
            continue;
        }
        let Ok(slot) = Arc::clone(&slots).try_acquire_owned() else {
            driven.capped += 1;
            continue;
        };
        driven.peak_in_flight = driven.peak_in_flight.max(cap - slots.available_permits());
        driven.lateness_us.push(crate::gateway::run::micros(late));
        driven.sent += 1;
        let call = launch(planned);
        let mut calls = calls.lock().unwrap_or_else(PoisonError::into_inner);
        while calls.try_join_next().is_some() {}
        calls.spawn(async move {
            call.await;
            drop(slot);
        });
    }
    let cap = u32::try_from(cap).unwrap_or(u32::MAX);
    let _all = slots.acquire_many(cap).await;
    driven
}

/// One completed call.
#[derive(Debug, Clone)]
pub struct Arrived {
    /// Tenant index into [`TENANTS`].
    pub tenant: usize,
    /// Whether it streamed.
    pub stream: bool,
    /// Its scheduled offset from the schedule's start.
    pub offset: Duration,
    /// Client timings and outcome, from just before the send.
    pub timed: Timed,
    /// The upstream's send time of each data event, or of the buffered body.
    pub sends: Vec<std::time::Instant>,
}

/// One backlog reading.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Backlog {
    /// Seconds since the schedule's start.
    pub offset: f64,
    /// Calls the driver held open.
    pub in_flight: f64,
    /// The server's active gateway calls.
    pub active: f64,
    /// Items pending in every outbox.
    pub pending: f64,
}

/// The largest growth of a backlog series' mean from the first [`EDGE`]
/// to the last [`EDGE`] before `until` seconds, or `None` when an edge has
/// no reading.
#[must_use]
pub fn backlog_growth(backlog: &[Backlog], until: f64) -> Option<f64> {
    let edge = EDGE.as_secs_f64();
    let mean = |low: f64, high: f64, read: fn(&Backlog) -> f64| {
        let values: Vec<f64> = backlog
            .iter()
            .filter(|reading| reading.offset >= low && reading.offset < high)
            .map(read)
            .collect();
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    };
    let series: [fn(&Backlog) -> f64; 3] = [
        |reading| reading.in_flight,
        |reading| reading.active,
        |reading| reading.pending,
    ];
    series.iter().try_fold(f64::NEG_INFINITY, |growth, read| {
        Some(growth.max(mean(until - edge, until, *read)? - mean(0.0, edge, *read)?))
    })
}

/// Rows a tenant could read in `vala.gateway.calls` over a repetition,
/// beside the calls it answered and the most it could have had admitted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ownership {
    /// New rows the tenant's administrator reads.
    pub rows: i64,
    /// Its answered calls: each was admitted, so each must be captured.
    pub answered: u64,
    /// Its sent calls less those refused: none other could be captured.
    pub admissible: u64,
}

/// Tenants whose rows fall outside their own answered-to-admissible calls
/// (with `shed` rows forgiven), plus one when the rows do not sum to within
/// `shed` below `staged`.
#[must_use]
pub fn misattributed(ownership: &[Ownership], staged: f64, shed: f64) -> u64 {
    let slack = shed.to_i64().unwrap_or(0);
    let mut rows = 0_i64;
    let mut misses = 0;
    for owned in ownership {
        rows += owned.rows;
        let answered = i64::try_from(owned.answered).unwrap_or(i64::MAX);
        let admissible = i64::try_from(owned.admissible).unwrap_or(i64::MAX);
        if owned.rows.saturating_add(slack) < answered || owned.rows > admissible {
            misses += 1;
        }
    }
    let rows = rows as f64;
    if rows > staged + 0.5 || rows < staged - shed - 0.5 {
        misses += 1;
    }
    misses
}

/// A tenant's, or every tenant's, answered calls of one mode: diagnostic
/// client chunk lag.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Distribution {
    /// Each stream data event's client receipt behind its upstream send,
    /// microseconds.
    pub lag_us: Vec<u64>,
    /// Streams whose data events and upstream sends did not pair one to one.
    pub unmatched: u64,
}

impl Distribution {
    /// Adds one answered call.
    pub fn add(&mut self, call: &Arrived) {
        if !call.stream {
            return;
        }
        if call.sends.len() != call.timed.events.len() {
            self.unmatched += 1;
        }
        let micros = crate::gateway::run::micros;
        for (received, sent) in call.timed.events.iter().zip(&call.sends) {
            self.lag_us
                .push(micros(received.saturating_duration_since(*sent)));
        }
    }

    /// Of `calls`' answered calls of `stream` mode from `tenant` (every
    /// tenant when `None`) in `phase`.
    #[must_use]
    pub fn of(
        calls: &[Arrived],
        phases: &Phases,
        phase: Period,
        tenant: Option<usize>,
        stream: bool,
    ) -> Self {
        let mut distribution = Self::default();
        for call in calls.iter().filter(|call| {
            call.timed.outcome.answered()
                && call.stream == stream
                && tenant.is_none_or(|tenant| tenant == call.tenant)
                && phases.phase(call.offset) == phase
        }) {
            distribution.add(call);
        }
        distribution
    }
}

/// The worst of `readings`: the largest value with the fewest samples, so
/// sample sufficiency holds for every reading; missing when any is.
#[must_use]
pub fn worst(readings: &[Reading]) -> Reading {
    let mut worst: Option<Reading> = None;
    for reading in readings {
        if !reading.is_present() || reading.above.is_some() {
            return reading.clone();
        }
        worst = Some(match worst {
            None => reading.clone(),
            Some(current) => Reading {
                value: current
                    .value
                    .zip(reading.value)
                    .map(|(left, right)| left.max(right)),
                samples: current.samples.min(reading.samples),
                ..current
            },
        });
    }
    worst.unwrap_or_else(|| Reading::missing("no distribution"))
}

/// The three p99 SLO readings of `phase` from its server timing
/// `population`, every tenant together, and its diagnostic chunk lag, the
/// worst tenant's in capacity, by declared metric name.
fn slo_readings(
    calls: &[Arrived],
    phases: &Phases,
    phase: Period,
    population: &Population,
) -> [(&'static str, Reading); 4] {
    let tenants: Vec<Option<usize>> = match phase {
        Period::Recovery => vec![None],
        _ => (0..TENANTS.len()).map(Some).collect(),
    };
    let lag = worst(
        &tenants
            .iter()
            .map(|tenant| {
                let streamed = Distribution::of(calls, phases, phase, *tenant, true);
                Reading::percentile(&streamed.lag_us, 0.99, 1e3)
            })
            .collect::<Vec<_>>(),
    );
    let p99 = |boundary| {
        population.elapsed(boundary, SUCCEEDED).map_or_else(
            || Reading::missing(format!("no `{boundary}` call succeeded in the phase")),
            |elapsed| Reading::quantile(&elapsed, 0.99, NANOS_PER_MS),
        )
    };
    [
        ("completion", p99(BUFFERED)),
        ("first_token", p99(STREAM_HEAD)),
        ("chunk_lag", lag),
        ("stream", p99(STREAM_END)),
    ]
}

/// What one L2 repetition observed, before it becomes a [`Repetition`].
#[derive(Debug, Clone)]
pub struct Observed {
    /// The declared rate.
    pub rate: f64,
    /// The phase split.
    pub phases: Phases,
    /// What the driver did.
    pub driven: Driven,
    /// Every sent call.
    pub calls: Vec<Arrived>,
    /// Backlog readings from the schedule's start.
    pub backlog: Vec<Backlog>,
    /// Server and driver usage over the whole schedule.
    pub usage: Usage,
    /// Capture integrity from the baseline to the post-drain scrape.
    pub integrity: Integrity,
    /// Items shed from the overload start to the judged recovery start.
    pub overload_shed: f64,
    /// Calls the server admitted and settled, baseline to post-drain.
    pub admitted: f64,
    /// Per tenant, its durable capture rows.
    pub ownership: Vec<Ownership>,
    /// The schedule's server timings, from its start until every call
    /// completed: their reconciliation's invalid reasons and health.
    pub population: Population,
    /// Server timings from the schedule's start to overload's start.
    pub capacity: Population,
    /// Server timings from judged recovery's start until every call
    /// completed.
    pub recovery: Population,
    /// Accounted bytes the Scribe outbox wrote while arrivals ran.
    pub durable_bytes: f64,
    /// The drain after arrivals stopped.
    pub drain: Drain,
    /// Answered calls that completed after the schedule's end.
    pub late: u64,
}

impl Observed {
    /// The repetition's readings by declared metric id, its arrivals, and
    /// every reason its evidence cannot judge it.
    #[must_use]
    pub fn repetition(&self) -> Repetition {
        let usage = &self.usage;
        let mut repetition = Repetition {
            seconds: usage.seconds,
            processes: vec![PROCESS.to_owned()],
            server_cpu_seconds: usage.server_cpu_seconds,
            peak_memory_bytes: usage.peak_memory_bytes,
            memory_interval_ms: PEAK_SAMPLE_INTERVAL_MS,
            driver_cpu_seconds: usage.driver_cpu_seconds,
            arrivals: Some(Arrivals {
                scheduled: self.driven.scheduled,
                sent: self.driven.sent,
                completed: self.calls.len() as u64,
                missed: self.driven.missed + self.driven.capped,
                lateness_p99_us: Percentiles::raw(&self.driven.lateness_us, 1.0).p99,
            }),
            ..Repetition::default()
        };
        for (phase, name, population) in [
            (Period::Capacity, "capacity", &self.capacity),
            (Period::Recovery, "recovery", &self.recovery),
        ] {
            for (metric, reading) in slo_readings(&self.calls, &self.phases, phase, population) {
                repetition.set(&format!("{name}.{metric}_p99_ms"), reading);
            }
        }
        let mut errors = [0_u64; 3];
        let mut refused = 0_u64;
        for call in self
            .calls
            .iter()
            .filter(|call| !call.timed.outcome.answered())
        {
            let phase = self.phases.phase(call.offset);
            if phase == Period::Overload
                && ADMISSION_REFUSALS.contains(&outcome_key(&call.timed.outcome).as_str())
            {
                refused += 1;
            } else {
                errors[phase as usize] += 1;
            }
        }
        let [capacity, overload, recovery] = errors;
        repetition.set("capacity.errors", Reading::count(capacity));
        repetition.set("overload.unexpected", Reading::count(overload));
        repetition.set("recovery.errors", Reading::count(recovery));
        repetition.set("overload.refused", Reading::count(refused));
        repetition.set("errors", Reading::count(capacity + overload + recovery));
        let answered = self
            .calls
            .iter()
            .filter(|call| call.timed.outcome.answered())
            .count() as u64;
        repetition.set("requests", Reading::count(answered - self.late));
        repetition.set("late_requests", Reading::count(self.late));
        repetition.set("drain_seconds", Reading::exact(usage.drain_seconds));
        self.capacity.percentiles(
            &mut repetition,
            BUFFERED,
            SUCCEEDED,
            false,
            &[("p50_ms", 0.5), ("p95_ms", 0.95), ("p99_ms", 0.99)],
        );
        self.capacity.percentiles(
            &mut repetition,
            BUFFERED,
            SUCCEEDED,
            true,
            &[("capacity.overhead_p99_ms", 0.99)],
        );
        repetition.set(
            "capacity.backlog_growth",
            backlog_growth(&self.backlog, self.phases.capacity.as_secs_f64()).map_or_else(
                || Reading::missing("backlog readings missing at an edge of the capacity phase"),
                Reading::exact,
            ),
        );
        let integrity = &self.integrity;
        let capture = |value: f64| {
            if integrity.reset || !self.admitted.is_finite() || self.admitted < 0.0 {
                Reading::missing("a capture, outbox, or admission counter reset")
            } else {
                Reading::exact(value)
            }
        };
        repetition.set("overload.shed", capture(self.overload_shed));
        repetition.set("capture.staged", capture(integrity.staged));
        repetition.set(
            "capture.unstaged",
            capture((integrity.staged - self.admitted).abs()),
        );
        repetition.set("capture.dropped", capture(integrity.dropped));
        repetition.set("capture.lost", capture(integrity.unshed_loss()));
        repetition.set(
            "capture.shed_outside_overload",
            capture(integrity.shed - self.overload_shed),
        );
        repetition.set(
            "capture.misattributed",
            capture(misattributed(&self.ownership, integrity.staged, integrity.shed) as f64),
        );
        repetition.set("capture_bytes", capture(self.durable_bytes));
        repetition.set("drain_s", Reading::exact(self.drain.seconds));
        repetition.set("undrained", Reading::exact(self.drain.remaining));
        let unmatched: u64 = [Period::Capacity, Period::Overload, Period::Recovery]
            .into_iter()
            .map(|phase| Distribution::of(&self.calls, &self.phases, phase, None, true).unmatched)
            .sum();
        if unmatched > 0 {
            repetition.invalid.push(format!(
                "{unmatched} streams whose data events did not pair with upstream sends"
            ));
        }
        self.population.validate(&mut repetition, "measured");
        if self.ownership.len() != TENANTS.len() {
            repetition
                .invalid
                .push("a tenant's capture rows were not read".to_owned());
        }
        repetition.invalid.extend(integrity.unwritten());
        if self.overload_shed > 0.0 {
            repetition.observations.push(format!(
                "overload shed {} outbox items at the staging limit ({:.1}% of the repetition's outbox items)",
                self.overload_shed,
                integrity.shed_fraction() * 100.0
            ));
        }
        if refused == 0
            && backlog_growth(&self.backlog, self.phases.overload_end().as_secs_f64())
                .is_none_or(|growth| growth <= self.rate)
        {
            repetition.observations.push(
                "no admission refusal or backlog saturation observed in overload: overload safety not demonstrated".to_owned(),
            );
        }
        repetition
    }
}

/// The arrivals the driver launches, with everything a call needs.
#[derive(Clone)]
pub struct Driver {
    /// The shared keep-alive client.
    http: Client,
    /// Each tenant's warmed keys.
    keys: Arc<Vec<Vec<SecretString>>>,
    /// The paced upstream, for each call's send times.
    upstream: Arc<PacedUpstream>,
    /// The buffered and the streamed call shapes.
    shapes: Arc<[Shape; 2]>,
}

impl Driver {
    /// A driver of `tenants`' keys against `upstream` over `http`.
    #[must_use]
    pub fn new(http: Client, tenants: &[Tenant], upstream: Arc<PacedUpstream>) -> Self {
        let shape = |prefix: &str, stream: bool| Shape {
            model: format!("bench-{prefix}/gpt-4o"),
            stream,
            request_bytes: 1024,
            expected: if stream {
                PACING.chunks
            } else {
                PACING.buffered_bytes
            },
        };
        Self {
            http,
            keys: Arc::new(tenants.iter().map(|tenant| tenant.keys.clone()).collect()),
            upstream,
            shapes: Arc::new([shape("chat", false), shape("stream", true)]),
        }
    }

    /// Launches the [`contention`] arrivals of `phases`' schedule at `rate`
    /// from `start` into `calls`, each marked `<label>-<index>` so its
    /// upstream send times match and labelled with its mode and admitting
    /// [`Period`], holding at most [`MAX_IN_FLIGHT`] of `slots`. Resolves
    /// with the driver's counts and every sent call once all have
    /// completed; the caller's future owns the schedule, so cancelling it
    /// stops launching.
    pub async fn drive(
        &self,
        (phases, rate): (&Phases, f64),
        start: Instant,
        label: &str,
        slots: Arc<Semaphore>,
        calls: &Mutex<JoinSet<()>>,
    ) -> (Driven, Vec<Arrived>) {
        let arrived = Arc::new(Mutex::new(Vec::new()));
        let launch = |planned: Planned| {
            let (tenant, stream) = contention(planned.index);
            let keys = &self.keys[tenant];
            let key = keys[usize::try_from(planned.index).unwrap_or(0) % keys.len()].clone();
            let marker = format!("{label}-{}", planned.index);
            let shape = self.shapes[usize::from(stream)].clone();
            let body = shape.body(&marker);
            let labels = (mode(stream).label(), phases.phase(planned.offset).label());
            let (http, upstream, arrived) = (
                self.http.clone(),
                Arc::clone(&self.upstream),
                Arc::clone(&arrived),
            );
            async move {
                let timed =
                    with_benchmark_labels(labels.0, labels.1, call(&http, &key, &shape, body))
                        .await;
                let sends = upstream.take(&marker);
                arrived
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(Arrived {
                        tenant,
                        stream,
                        offset: planned.offset,
                        timed,
                        sends,
                    });
            }
        };
        let schedule = phases.schedule(rate);
        let driven = open_loop(&schedule, start, slots, MAX_IN_FLIGHT, calls, launch).await;
        let arrived = std::mem::take(&mut *arrived.lock().unwrap_or_else(PoisonError::into_inner));
        (driven, arrived)
    }
}

/// Reads [`Backlog`] every [`SAMPLE_EVERY`] from `start` until `stop`.
///
/// # Errors
///
/// Returns scrape failures.
async fn sample(
    server: &LocalServer,
    slots: &Semaphore,
    start: Instant,
    stop: &CancellationToken,
) -> Result<Vec<Backlog>> {
    let mut readings = Vec::new();
    let mut next = start;
    while !stop.is_cancelled() {
        tokio::select! {
            () = tokio::time::sleep_until(next) => {}
            () = stop.cancelled() => break,
        }
        let metrics = server.metrics().await?;
        readings.push(Backlog {
            offset: next.saturating_duration_since(start).as_secs_f64(),
            in_flight: (MAX_IN_FLIGHT - slots.available_permits()) as f64,
            active: metrics.sum("wyrd_gateway_active_calls", &[]),
            pending: metrics.sum("outbox_pending", &[]),
        });
        next += SAMPLE_EVERY;
    }
    Ok(readings)
}

/// Scrapes `server` at `start + at`.
///
/// # Errors
///
/// Returns the scrape failure.
async fn scrape_at(server: &LocalServer, start: Instant, at: Duration) -> Result<Metrics> {
    tokio::time::sleep_until(start + at).await;
    Ok(server.metrics().await?)
}

/// The L2 workload: one server, four tenants with metadata capture, the
/// paced upstream, and the open-loop driver.
pub struct Capacity {
    /// The server, once prepared.
    server: Option<LocalServer>,
    /// The paced upstream, once prepared.
    upstream: Option<Arc<PacedUpstream>>,
    /// Every provisioned tenant, in [`TENANTS`] order.
    tenants: Vec<Tenant>,
    /// The arrivals' driver, once prepared.
    driver: Option<Driver>,
    /// Where the server log is copied when it stops.
    log: PathBuf,
    /// Every call the measurement launched; a cancelled measurement leaves
    /// its in-flight calls here for [`Workload::finish`] to abort and join
    /// before the drain.
    calls: Mutex<JoinSet<()>>,
}

impl Capacity {
    /// An unprepared workload that will copy the server log to `log`.
    #[must_use]
    pub fn new(log: PathBuf) -> Self {
        Self {
            server: None,
            upstream: None,
            tenants: Vec::new(),
            driver: None,
            log,
            calls: Mutex::new(JoinSet::new()),
        }
    }

    /// Starts the release server with [`TENANTS`], the paced upstream, and [`KEYS_PER_TENANT`]
    /// keys per tenant with metadata capture.
    ///
    /// # Errors
    ///
    /// Returns a server, upstream, or provisioning failure.
    async fn start(&mut self) -> Result<()> {
        let binary = release_binary()?;
        let server = LocalServer::start(&binary, &TENANTS, &[]).await?;
        let upstream = Arc::new(PacedUpstream::start(PACING, MAX_IN_FLIGHT).await?);
        for setup in server.tenants() {
            let tenant = Tenant::provision(setup, &upstream.uri(), KEYS_PER_TENANT).await?;
            tenant.capture(CapturePolicy::Metadata).await?;
            self.tenants.push(tenant);
        }
        if self.tenants.len() != TENANTS.len() {
            return Err(format!("setup provisioned {} tenants", self.tenants.len()).into());
        }
        self.driver = Some(Driver::new(http()?, &self.tenants, Arc::clone(&upstream)));
        self.upstream = Some(upstream);
        self.server = Some(server);
        Ok(())
    }

    /// Each tenant's readable capture rows.
    ///
    /// # Errors
    ///
    /// Returns the capture read failure.
    async fn captured(&self) -> Result<Vec<u64>> {
        let mut rows = Vec::with_capacity(self.tenants.len());
        for tenant in &self.tenants {
            rows.push(tenant.captured().await?);
        }
        Ok(rows)
    }

    /// Runs `phase` of `step` over `window`: the whole capacity, overload,
    /// recovery schedule, measured until its end while admitted calls
    /// drain, then the capture drain and reads, and each admitted phase's
    /// cohort of the drained server timings.
    ///
    /// # Errors
    ///
    /// Returns an undeclared rate, a scrape, capture read, or driver
    /// failure.
    async fn step(&self, step: &Step, phase: Phase, window: Duration) -> Result<Repetition> {
        let rate = param(step, "rate")
            .filter(|rate| *rate > 0.0)
            .ok_or_else(|| format!("step `{}` declares no rate", step.id))?;
        let (Some(server), Some(upstream), Some(driver)) =
            (&self.server, &self.upstream, &self.driver)
        else {
            return Err("the workload was not prepared".into());
        };
        let phases = Phases::of(window);
        upstream.clear();
        let before = self.captured().await?;
        let slots = Arc::new(Semaphore::new(MAX_IN_FLIGHT));
        let stop = CancellationToken::new();
        let timings = server.timings().await?;
        let (traffic, baseline) = Window::open(server, Selection::RESOURCES).await?;
        let start = Instant::now() + LEAD;
        let label = format!("{}-{phase:?}", step.id);
        let driving = async {
            let driven = driver
                .drive(
                    (&phases, rate),
                    start,
                    &label,
                    Arc::clone(&slots),
                    &self.calls,
                )
                .await;
            stop.cancel();
            driven
        };
        let boundaries = async {
            let overload = scrape_at(server, start, phases.capacity).await?;
            let recovery = scrape_at(server, start, phases.judged()).await?;
            Result::Ok((overload, recovery))
        };
        let end = (start + window).into_std();
        let work =
            async { tokio::join!(driving, sample(server, &slots, start, &stop), boundaries) };
        let ((driven, backlog, boundaries), usage, stopped) =
            traffic.measure(server, end, work).await?;
        let (driven, calls) = driven;
        let drain = settle(server).await?;
        let last = server.metrics().await?;
        if phase == Phase::Warmup {
            return Ok(Repetition::default());
        }
        let (at_overload, at_recovery) = boundaries?;
        let mut expected = Vec::new();
        for period in Period::ALL {
            for stream in [false, true] {
                let answered = calls
                    .iter()
                    .filter(|call| call.stream == stream && call.timed.outcome.answered())
                    .filter(|call| phases.phase(call.offset) == period)
                    .count() as u64;
                for boundary in crate::gateway::run::boundaries(stream) {
                    expected.push(Expected {
                        boundary,
                        outcome: SUCCEEDED,
                        case: mode(stream).label(),
                        phase: period.label(),
                        count: answered,
                    });
                }
            }
        }
        let population = Population::collect(server, &timings, &expected).await;
        let late = calls
            .iter()
            .filter(|call| call.timed.outcome.answered() && call.timed.done >= end)
            .count() as u64;
        let after = self.captured().await?;
        let ownership = (0..TENANTS.len())
            .map(|tenant| {
                let mine = || calls.iter().filter(move |call| call.tenant == tenant);
                let count = |rows: &[u64]| {
                    rows.get(tenant)
                        .and_then(|rows| i64::try_from(*rows).ok())
                        .unwrap_or(0)
                };
                Ownership {
                    rows: count(&after) - count(&before),
                    answered: mine().filter(|call| call.timed.outcome.answered()).count() as u64,
                    admissible: mine().filter(|call| !call.timed.outcome.refused()).count() as u64,
                }
            })
            .collect();
        let admitted = last.sum(
            "wyrd_gateway_phase_duration_seconds_count",
            &["phase=\"account\""],
        ) - baseline.sum(
            "wyrd_gateway_phase_duration_seconds_count",
            &["phase=\"account\""],
        );
        let observed = Observed {
            rate,
            phases,
            driven,
            calls,
            backlog: backlog?,
            usage,
            integrity: Integrity::between(&baseline, &last),
            overload_shed: Integrity::between(&at_overload, &at_recovery).shed,
            admitted,
            ownership,
            capacity: cohort(&population, Period::Capacity),
            recovery: cohort(&population, Period::Recovery),
            population,
            durable_bytes: Durable::between(&baseline, &stopped).bytes,
            drain,
            late,
        };
        Ok(observed.repetition())
    }
}

#[async_trait]
impl Workload for Capacity {
    /// Starts the server, upstream, tenants, and driver.
    ///
    /// # Errors
    ///
    /// Returns a server, upstream, or provisioning failure.
    async fn prepare(&mut self) -> std::result::Result<(), ExperimentError> {
        self.start().await.map_err(ExperimentError::workload)
    }

    /// The server's PID while it runs.
    fn processes(&self) -> Vec<u32> {
        self.server.iter().filter_map(LocalServer::pid).collect()
    }

    /// Runs `phase` of `step` over `window`.
    ///
    /// # Errors
    ///
    /// Returns an undeclared rate, a scrape, capture read, or driver
    /// failure.
    async fn measure(
        &mut self,
        step: &Step,
        phase: Phase,
        window: Duration,
    ) -> std::result::Result<Repetition, ExperimentError> {
        self.step(step, phase, window)
            .await
            .map_err(ExperimentError::workload)
    }

    /// Aborts and joins every call a cancelled measurement left in flight,
    /// so no request arrives during the drain, then settles and stops the
    /// server loss-free.
    ///
    /// # Errors
    ///
    /// Returns every reason the stop was not loss-free.
    async fn finish(&mut self) -> std::result::Result<(), ExperimentError> {
        self.calls
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .shutdown()
            .await;
        let Some(server) = self.server.take() else {
            return Ok(());
        };
        shutdown(server, &self.log)
            .await
            .map_err(ExperimentError::workload)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tokio::sync::Semaphore;
    use tokio::task::JoinSet;
    use tokio::time::Instant;
    use wyrd_telemetry::timing::{Distribution, IntervalSeries};
    use wyrd_testing::capacity::{Population, UNLABELLED, Usage};
    use wyrd_testing::experiment::Reading;

    use super::{
        Arrived, Backlog, Driven, Observed, Ownership, Period, Phases, Schedule, backlog_growth,
        contention, misattributed, open_loop, worst,
    };
    use crate::gateway::run::{BUFFERED, Drain, Integrity, STREAM_END, STREAM_HEAD, SUCCEEDED};
    use crate::gateway::workload::{Outcome, Timed};

    /// The last due arrival, waking past the schedule's end but within
    /// [`super::MISS_AFTER`] behind a stalled call, is missed and never
    /// sent, while every earlier call is still joined.
    ///
    /// # Panics
    ///
    /// Panics when an arrival is admitted after the end or a call is lost.
    #[tokio::test(start_paused = true)]
    async fn the_last_arrival_waking_after_the_end_is_missed() {
        let calls = Mutex::new(JoinSet::new());
        let done = Arc::new(AtomicU64::new(0));
        let schedule = Schedule::new(vec![(100.0, Duration::from_secs(1))]);
        let driven = open_loop(
            &schedule,
            Instant::now(),
            Arc::new(Semaphore::new(1_000)),
            1_000,
            &calls,
            |planned| {
                let done = Arc::clone(&done);
                async move {
                    if planned.index == 98 {
                        tokio::time::advance(Duration::from_millis(25)).await;
                    }
                    done.fetch_add(1, Ordering::SeqCst);
                }
            },
        )
        .await;
        assert_eq!(
            (driven.scheduled, driven.sent, driven.missed, driven.capped),
            (100, 99, 1, 0)
        );
        assert_eq!(done.load(Ordering::SeqCst), 99);
    }

    /// Cancelling the scheduler before a later due arrival launches nothing
    /// more, leaves the calls in flight in the measurement's owned set, and
    /// aborting and joining that set, as finish does before its drain,
    /// leaves no live call.
    ///
    /// # Panics
    ///
    /// Panics when an arrival launches after cancellation or a call
    /// outlives the join.
    #[tokio::test(start_paused = true)]
    async fn cancelled_scheduler_launches_nothing_and_its_calls_are_joined() {
        let calls = Mutex::new(JoinSet::new());
        let launched = AtomicU64::new(0);
        let live = Arc::new(());
        let schedule = Schedule::new(vec![(10.0, Duration::from_secs(1))]);
        let driving = open_loop(
            &schedule,
            Instant::now(),
            Arc::new(Semaphore::new(100)),
            100,
            &calls,
            |_| {
                launched.fetch_add(1, Ordering::Relaxed);
                let held = Arc::clone(&live);
                async move {
                    std::future::pending::<()>().await;
                    drop(held);
                }
            },
        );
        let cut = tokio::time::timeout(Duration::from_millis(250), driving).await;
        assert!(cut.is_err(), "the schedule outlasts the cut");
        let at_cancel = launched.load(Ordering::Relaxed);
        assert_eq!(at_cancel, 3, "arrivals at 0, 100, and 200 ms");
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(launched.load(Ordering::Relaxed), at_cancel);
        assert_eq!(Arc::strong_count(&live), 4, "three calls stay owned");
        calls
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .shutdown()
            .await;
        assert_eq!(Arc::strong_count(&live), 1, "no call outlives the join");
    }

    /// Open-loop arrivals keep the offered rate whatever responses do,
    /// count a stalled driver's late arrivals missed instead of bursting
    /// them, count arrivals over the in-flight bound capped, and spread
    /// contention 70/10/10/10 with both modes per tenant.
    ///
    /// # Panics
    ///
    /// Panics when a schedule, split, or driver count is wrong.
    #[test]
    fn arrivals_preserve_offered_rate_and_contention() {
        let planned = Schedule::new(vec![
            (100.0, Duration::from_secs(1)),
            (50.0, Duration::from_secs(2)),
        ])
        .planned();
        assert_eq!(planned.len(), 200);
        assert_eq!(planned[100].offset, Duration::from_secs(1));
        assert_eq!(planned[101].offset, Duration::from_millis(1_020));

        let split: Vec<(usize, bool)> = (0..1_000).map(contention).collect();
        for (tenant, share) in [(0, 700), (1, 100), (2, 100), (3, 100)] {
            let mine: Vec<bool> = split
                .iter()
                .filter(|(owner, _)| *owner == tenant)
                .map(|(_, stream)| *stream)
                .collect();
            assert_eq!(mine.len(), share, "tenant {tenant}");
            assert_eq!(
                mine.iter().filter(|stream| **stream).count(),
                share / 2,
                "tenant {tenant}"
            );
        }

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .expect("a paused test runtime");
        runtime.block_on(async {
            let calls = Mutex::new(JoinSet::new());
            let schedule = Schedule::new(vec![(100.0, Duration::from_secs(1))]);
            let slow = || async { tokio::time::sleep(Duration::from_secs(5)).await };
            let driven = open_loop(
                &schedule,
                Instant::now(),
                Arc::new(Semaphore::new(1_000)),
                1_000,
                &calls,
                |_| slow(),
            )
            .await;
            assert_eq!(
                (driven.scheduled, driven.sent, driven.missed, driven.capped),
                (100, 100, 0, 0)
            );
            assert_eq!(driven.peak_in_flight, 100);
            assert!(driven.lateness_us.iter().all(|late| *late == 0));
            let schedule = Schedule::new(vec![(100.0, Duration::from_millis(100))]);
            let driven = open_loop(
                &schedule,
                Instant::now(),
                Arc::new(Semaphore::new(2)),
                2,
                &calls,
                |_| slow(),
            )
            .await;
            assert_eq!((driven.scheduled, driven.sent, driven.capped), (10, 2, 8));
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("a test runtime");
        runtime.block_on(async {
            let calls = Mutex::new(JoinSet::new());
            let schedule = Schedule::new(vec![(1_000.0, Duration::from_millis(100))]);
            let driven = open_loop(
                &schedule,
                Instant::now(),
                Arc::new(Semaphore::new(1_000)),
                1_000,
                &calls,
                |planned| {
                    if planned.index == 10 {
                        std::thread::sleep(Duration::from_millis(60));
                    }
                    async {}
                },
            )
            .await;
            assert_eq!(driven.scheduled, 100);
            assert!(driven.missed >= 30, "{driven:?}");
            assert_eq!(driven.sent + driven.missed + driven.capped, 100);
            assert!(driven.lateness_us.iter().all(|late| *late <= 20_000));
        });
    }

    /// A 360 s repetition splits 240/60/60 with a 30 s unjudged recovery
    /// grace that counts with overload, and its schedule offers twice the
    /// rate only in overload.
    ///
    /// # Panics
    ///
    /// Panics when a phase boundary or rate is wrong.
    #[test]
    fn phases_split_capacity_overload_and_judged_recovery() {
        let phases = Phases::of(Duration::from_secs(360));
        assert_eq!(
            (
                phases.capacity,
                phases.overload,
                phases.recovery,
                phases.grace
            ),
            (
                Duration::from_secs(240),
                Duration::from_secs(60),
                Duration::from_secs(60),
                Duration::from_secs(30)
            )
        );
        assert_eq!(phases.phase(Duration::from_secs(239)), Period::Capacity);
        assert_eq!(phases.phase(Duration::from_secs(240)), Period::Overload);
        assert_eq!(phases.phase(Duration::from_secs(329)), Period::Overload);
        assert_eq!(phases.phase(Duration::from_secs(330)), Period::Recovery);
        assert_eq!(phases.schedule(10.0).planned().len(), 2_400 + 1_200 + 600);
        assert_eq!(
            Phases::of(Duration::from_secs(120)).grace,
            Duration::from_secs(10)
        );
    }

    /// One series of successful `boundary` observations, each `elapsed_ms`
    /// long, 0.1 ms of each outside its provider waits.
    ///
    /// # Panics
    ///
    /// Panics when a duration is outside the histogram's trackable range or
    /// an `elapsed_ms` is under 0.1 ms.
    fn timing(boundary: &str, elapsed_ms: &[u64]) -> IntervalSeries {
        let durations = |samples: Vec<u64>| Distribution::of_durations(&samples).expect("in range");
        let elapsed: Vec<u64> = elapsed_ms.iter().map(|ms| ms * 1_000_000).collect();
        IntervalSeries {
            boundary: boundary.to_owned(),
            outcome: SUCCEEDED.to_owned(),
            case: UNLABELLED.to_owned(),
            phase: UNLABELLED.to_owned(),
            count: elapsed.len() as u64,
            excluded: Some(durations(elapsed.iter().map(|ns| ns - 100_000).collect())),
            residual: Some(durations(vec![100_000; elapsed.len()])),
            elapsed: durations(elapsed),
            missing_excluded: 0,
            out_of_range: 0,
            negative_residual: 0,
        }
    }

    /// One phase's server timings: every tenant's buffered and streamed
    /// calls, the last tenant's slower, each stream's head at 10 ms.
    ///
    /// # Panics
    ///
    /// Panics when [`timing`] does.
    fn phase() -> Population {
        Population {
            series: vec![
                timing(BUFFERED, &[50, 50, 50, 90]),
                timing(STREAM_HEAD, &[10; 4]),
                timing(STREAM_END, &[50, 50, 50, 90]),
            ],
            ..Population::default()
        }
    }

    /// Whether `reading` holds `value`: exactly, or inside its bucket.
    fn holds(reading: &Reading, value: f64) -> bool {
        match (reading.above, reading.value) {
            (Some(above), Some(upper)) => above < value && value <= upper,
            (None, Some(exact)) => (exact - value).abs() < 1e-9,
            _ => false,
        }
    }

    /// One call of `tenant` in `stream` mode scheduled at `offset` seconds
    /// that ended `outcome` `done_ms` after its send, its events each 2 ms
    /// behind its upstream send.
    fn arrived(
        tenant: usize,
        stream: bool,
        offset: u64,
        outcome: Outcome,
        done_ms: u64,
    ) -> Arrived {
        let scheduled = std::time::Instant::now();
        let at = |ms: u64| scheduled + Duration::from_millis(ms);
        let events: Vec<_> = if stream {
            (1..=4).map(|n| at(n * 10)).collect()
        } else {
            Vec::new()
        };
        let sends = if stream {
            (1..=4).map(|n| at(n * 10 - 2)).collect()
        } else {
            Vec::new()
        };
        Arrived {
            tenant,
            stream,
            offset: Duration::from_secs(offset),
            timed: Timed {
                sent: scheduled,
                headers: None,
                events,
                done: at(done_ms),
                outcome,
            },
            sends,
        }
    }

    /// Readings from a clean repetition of every tenant and mode in every
    /// phase; `extra` calls are added.
    fn observed(extra: Vec<Arrived>) -> Observed {
        let phases = Phases::of(Duration::from_secs(360));
        let mut calls = Vec::new();
        for offset in [10, 250, 340] {
            for tenant in 0..4 {
                for stream in [false, true] {
                    let done = if tenant == 3 { 90 } else { 50 };
                    calls.push(arrived(tenant, stream, offset, Outcome::Answered, done));
                }
            }
        }
        calls.extend(extra);
        let backlog = [0.0, 1.0, 2.0, 237.0, 238.0, 239.0]
            .map(|offset| Backlog {
                offset,
                in_flight: 10.0,
                active: 5.0,
                pending: 1.0,
            })
            .to_vec();
        Observed {
            rate: 100.0,
            phases,
            driven: Driven {
                scheduled: calls.len() as u64,
                sent: calls.len() as u64,
                lateness_us: vec![10; calls.len()],
                ..Driven::default()
            },
            ownership: (0..4)
                .map(|_| Ownership {
                    rows: 6,
                    answered: 6,
                    admissible: 6,
                })
                .collect(),
            calls,
            backlog,
            usage: Usage {
                seconds: 360.0,
                server_cpu_seconds: Some(100.0),
                ..Usage::default()
            },
            integrity: Integrity {
                staged: 24.0,
                written: 3.0,
                written_items: 24.0,
                ..Integrity::default()
            },
            overload_shed: 0.0,
            admitted: 24.0,
            population: Population::default(),
            capacity: phase(),
            recovery: phase(),
            durable_bytes: 4_096.0,
            drain: Drain {
                seconds: 0.5,
                remaining: 0.0,
            },
            late: 0,
        }
    }

    /// A reading's exact value.
    fn exact(repetition: &wyrd_testing::experiment::Repetition, id: &str) -> Option<f64> {
        let reading = repetition.reading(id);
        reading.above.is_none().then_some(reading.value).flatten()
    }

    /// A clean repetition judges each phase over every tenant by its server
    /// timing histograms from handler entry, with the buffered overhead its
    /// residual after provider waits and chunk lag a client diagnostic of
    /// the worst tenant; an invalid timing window invalidates the evidence;
    /// declared
    /// admission refusals count only in overload, every other unanswered
    /// call is an error of its phase, shed items are attributed to overload
    /// or outside it, and unpaired stream events invalidate the evidence.
    ///
    /// # Panics
    ///
    /// Panics when a reading or classification is wrong.
    #[test]
    fn capacity_repetition_classifies_phases_and_capture() {
        let clean = observed(Vec::new()).repetition();
        for (id, value) in [
            ("capacity.completion_p99_ms", 90.0),
            ("recovery.stream_p99_ms", 90.0),
            ("capacity.first_token_p99_ms", 10.0),
            ("capacity.overhead_p99_ms", 0.1),
            ("p99_ms", 90.0),
            ("p50_ms", 50.0),
        ] {
            assert!(
                holds(&clean.reading(id), value),
                "{id}: {:?}",
                clean.reading(id)
            );
        }
        assert_eq!(exact(&clean, "capacity.chunk_lag_p99_ms"), Some(2.0));
        assert_eq!(clean.reading("capacity.completion_p99_ms").samples, Some(4));
        for id in [
            "capacity.errors",
            "overload.unexpected",
            "recovery.errors",
            "capacity.backlog_growth",
            "capture.unstaged",
            "capture.lost",
            "capture.misattributed",
            "capture.shed_outside_overload",
        ] {
            assert_eq!(exact(&clean, id), Some(0.0), "{id}");
        }
        assert!(clean.invalid.is_empty(), "{:?}", clean.invalid);
        let arrivals = clean.arrivals.expect("arrivals");
        assert_eq!((arrivals.missed, arrivals.lateness_p99_us), (0, Some(10.0)));
        assert_eq!(clean.observations.len(), 2, "{:?}", clean.observations);

        let mut untimed = observed(Vec::new());
        untimed.population.invalid.push("a short window".to_owned());
        untimed.recovery = Population::default();
        let untimed = untimed.repetition();
        assert_eq!(untimed.invalid, ["a short window"]);
        assert!(!untimed.reading("recovery.completion_p99_ms").is_present());

        let refused = |offset| {
            arrived(
                0,
                false,
                offset,
                Outcome::Refused("WYRD_GATEWAY_429_LIMIT_EXCEEDED".to_owned()),
                1,
            )
        };
        let mut broken = observed(vec![
            refused(250),
            refused(320),
            refused(10),
            refused(340),
            arrived(1, true, 250, Outcome::TimedOut, 1),
        ]);
        broken.integrity.lost = 5.0;
        broken.integrity.shed = 4.0;
        broken.overload_shed = 3.0;
        broken.admitted = 23.0;
        broken.calls[1].sends.pop();
        let broken = broken.repetition();
        for (id, value) in [
            ("overload.refused", 2.0),
            ("overload.unexpected", 1.0),
            ("capacity.errors", 1.0),
            ("recovery.errors", 1.0),
            ("errors", 3.0),
            ("capture.lost", 1.0),
            ("overload.shed", 3.0),
            ("capture.shed_outside_overload", 1.0),
            ("capture.unstaged", 1.0),
        ] {
            assert_eq!(exact(&broken, id), Some(value), "{id}");
        }
        assert_eq!(broken.invalid.len(), 1, "{:?}", broken.invalid);

        let mut sparse = observed(Vec::new());
        sparse.backlog.retain(|reading| reading.offset < 100.0);
        sparse.integrity.reset = true;
        let sparse = sparse.repetition();
        assert!(!sparse.reading("capacity.backlog_growth").is_present());
        assert!(!sparse.reading("capture.lost").is_present());
    }

    /// Backlog growth is the largest series growth between the capacity
    /// phase's edges; the worst reading keeps the fewest samples and is
    /// missing when any is; ownership counts tenants reading outside their
    /// own calls and a total off the staged captures, forgiving shed rows.
    ///
    /// # Panics
    ///
    /// Panics when a pure judgment input is wrong.
    #[test]
    fn backlog_worst_and_ownership_are_pure() {
        let reading = |offset, active| Backlog {
            offset,
            in_flight: 1.0,
            active,
            pending: 0.0,
        };
        let backlog = vec![reading(0.0, 2.0), reading(2.0, 4.0), reading(9.0, 400.0)];
        assert_eq!(backlog_growth(&backlog, 10.0), Some(397.0));
        assert_eq!(backlog_growth(&backlog[..2], 10.0), None);

        let worst = worst(&[
            Reading::exact(3.0).with_samples(2_000),
            Reading::exact(5.0).with_samples(1_500),
            Reading::exact(4.0).with_samples(1_000),
        ]);
        assert_eq!((worst.value, worst.samples), (Some(5.0), Some(1_000)));
        assert!(!super::worst(&[Reading::exact(1.0), Reading::missing("none")]).is_present());

        let own = |rows, answered, admissible| Ownership {
            rows,
            answered,
            admissible,
        };
        assert_eq!(misattributed(&[own(5, 5, 6), own(1, 1, 1)], 6.0, 0.0), 0);
        assert_eq!(misattributed(&[own(4, 5, 6), own(1, 1, 1)], 6.0, 0.0), 2);
        assert_eq!(misattributed(&[own(4, 5, 6), own(1, 1, 1)], 6.0, 1.0), 0);
        assert_eq!(misattributed(&[own(7, 5, 6), own(1, 1, 1)], 8.0, 0.0), 1);
    }
}
