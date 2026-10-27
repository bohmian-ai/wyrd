//! The open-loop capacity workloads and what each measured window records.
//!
//! A capacity window offers a fixed aggregate rate whatever the responses
//! do: [`Schedule`] places every arrival in advance, and [`open_loop`]
//! launches each at its scheduled instant. An arrival more than
//! [`MISS_AFTER`] late is counted missed and dropped, never replayed in a
//! burst, and one arriving while [`MAX_IN_FLIGHT`] calls are open is counted
//! capped; the driver never waits for a free slot or retries a refusal.
//! Every call records its scheduled instant beside the client timings, and
//! the [`PacedUpstream`] send time of each data event, so completion, first
//! token, and chunk lag are all measured from the right clock. A sampler
//! reads in-flight calls, the server's active calls, and outbox backlog
//! every second for the backlog verdict.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use num_traits::ToPrimitive;

use reqwest::Client;
use secrecy::SecretString;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use wyrd_testing::capacity::{Readings, Selection, Telemetry, Usage, Window};
use wyrd_testing::gateway_capture::{PacedUpstream, Pacing};
use wyrd_testing::release_server::{LocalServer, Metrics};

use crate::gateway::Result;
use crate::gateway::run::{
    Evidence, Integrity, Profiler, SELECTION, gaps, micros, pareto, record, settle,
};
use crate::gateway::workload::{Shape, Tenant, Timed, call};

/// Default aggregate offered rates, requests per second, in run order.
pub const RATES: [f64; 5] = [50.0, 100.0, 250.0, 500.0, 1000.0];

/// Untimed arrivals at each rate before its window, settled before the
/// baseline.
pub const WARMUP: Duration = Duration::from_secs(2);

/// Measured arrival window of each rate.
pub const WINDOW: Duration = Duration::from_secs(15);

/// The high-rate interval of an overload sequence.
pub const OVERLOAD: Duration = Duration::from_secs(10);

/// The lower-rate interval after it.
pub const RECOVERY: Duration = Duration::from_secs(20);

/// The final part of [`RECOVERY`] whose calls judge recovery.
pub const RECOVERED: Duration = Duration::from_secs(15);

/// The leading and trailing interval of a window whose backlog is compared.
pub const EDGE: Duration = Duration::from_secs(3);

/// Calls the driver holds open at once; an arrival beyond it is capped.
pub const MAX_IN_FLIGHT: usize = 16_384;

/// Lateness past which an arrival is missed rather than sent late.
pub const MISS_AFTER: Duration = Duration::from_millis(20);

/// Send lateness p99 above which a window is not capacity evidence,
/// microseconds.
pub const LATENESS_LIMIT_US: u64 = 5_000;

/// Completed calls each tenant/mode distribution needs to judge an SLO.
pub const MIN_CALLS: usize = 30;

/// Warmed keys each tenant's arrivals rotate through.
pub const KEYS_PER_TENANT: usize = 8;

/// The tenants `setup` provisions; single-tenant cases use the first.
pub const TENANTS: [&str; 4] = ["bench-a", "bench-b", "bench-c", "bench-d"];

/// How often the backlog sampler reads.
const SAMPLE_EVERY: Duration = Duration::from_secs(1);

/// Lead before a schedule's first arrival, so the driver starts on time.
const LEAD: Duration = Duration::from_millis(50);

/// The deterministic upstream every capacity case calls.
pub const PACING: Pacing = Pacing {
    buffered_delay: Duration::from_millis(100),
    buffered_bytes: 1024,
    stream_delay: Duration::from_millis(250),
    chunks: 32,
    chunk_interval: Duration::from_millis(20),
};

/// One capacity case of the matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Load {
    /// One tenant's delayed buffered calls.
    Buffered,
    /// One tenant's paced streams.
    PacedStream,
    /// Four tenants at 70/10/10/10 percent, each alternating modes.
    TenantContention,
}

impl Load {
    /// Every case, in run order.
    pub const ALL: [Self; 3] = [Self::Buffered, Self::PacedStream, Self::TenantContention];

    /// The case `--case` names by number.
    pub fn numbered(item: &str) -> Option<Self> {
        match item {
            "1" => Some(Self::Buffered),
            "2" => Some(Self::PacedStream),
            "3" => Some(Self::TenantContention),
            _ => None,
        }
    }

    /// The report's name of the case, its number first.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Buffered => "1 buffered",
            Self::PacedStream => "2 paced-stream",
            Self::TenantContention => "3 tenant-contention",
        }
    }

    /// Whether its calls stream, buffer, or both.
    pub const fn modes(self) -> &'static [bool] {
        match self {
            Self::Buffered => &[false],
            Self::PacedStream => &[true],
            Self::TenantContention => &[false, true],
        }
    }

    /// The tenant and mode of arrival `index`. Contention cycles ten slots,
    /// seven for the first tenant and one for each other, and alternates
    /// each slot's mode every cycle, so every tenant sends both modes
    /// equally.
    pub const fn arrival(self, index: u64) -> (usize, bool) {
        match self {
            Self::Buffered => (0, false),
            Self::PacedStream => (0, true),
            Self::TenantContention => {
                let slot = index % 10;
                let tenant = match slot {
                    7 => 1,
                    8 => 2,
                    9 => 3,
                    _ => 0,
                };
                (tenant, (index / 10 + slot) % 2 == 1)
            }
        }
    }

    /// Tenants the case sends to.
    pub const fn tenants(self) -> usize {
        match self {
            Self::Buffered | Self::PacedStream => 1,
            Self::TenantContention => TENANTS.len(),
        }
    }
}

/// The caller-declared workload SLOs, seconds; `None` when not given.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Slo {
    /// Buffered completion p99 bound.
    pub answer: Option<f64>,
    /// First complete stream data event p99 bound.
    pub first_token: Option<f64>,
    /// Data-event lag behind its upstream send p99 bound.
    pub chunk_lag: Option<f64>,
    /// Complete stream receipt p99 bound.
    pub stream: Option<f64>,
}

impl Slo {
    /// Reads the `--max-*-ms` bounds, requiring every bound a mode of
    /// `loads` judges.
    ///
    /// # Errors
    ///
    /// Returns a message for a bound that is not a positive finite number or
    /// one a selected mode needs but was not given.
    pub fn parse(arguments: &[String], loads: &[Load]) -> std::result::Result<Self, String> {
        let bound = |flag: &str| -> std::result::Result<Option<f64>, String> {
            crate::flags::value(arguments, flag)?
                .map(|value| {
                    value
                        .parse::<f64>()
                        .ok()
                        .filter(|ms| ms.is_finite() && *ms > 0.0)
                        .map(|ms| ms / 1e3)
                        .ok_or_else(|| {
                            format!("{flag} takes positive finite milliseconds, got `{value}`")
                        })
                })
                .transpose()
        };
        let slo = Self {
            answer: bound("--max-answer-ms")?,
            first_token: bound("--max-first-token-ms")?,
            chunk_lag: bound("--max-chunk-lag-ms")?,
            stream: bound("--max-stream-ms")?,
        };
        let modes = loads.iter().flat_map(|load| load.modes());
        for stream in modes {
            let needed: &[(&str, Option<f64>)] = if *stream {
                &[
                    ("--max-first-token-ms", slo.first_token),
                    ("--max-chunk-lag-ms", slo.chunk_lag),
                    ("--max-stream-ms", slo.stream),
                ]
            } else {
                &[("--max-answer-ms", slo.answer)]
            };
            if let Some((flag, _)) = needed.iter().find(|(_, bound)| bound.is_none()) {
                return Err(format!("{flag} is required by a selected case"));
            }
        }
        Ok(slo)
    }
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
    /// Its segment.
    pub segment: usize,
    /// Its offset from the schedule's start.
    pub offset: Duration,
}

impl Schedule {
    /// The arrivals of `segments` in order, each `1 / rate` apart.
    pub const fn new(segments: Vec<(f64, Duration)>) -> Self {
        Self { segments }
    }

    /// Every arrival in order: `rate * duration` per segment, rounded, the
    /// first at the segment's start.
    pub fn planned(&self) -> Vec<Planned> {
        let mut planned = Vec::new();
        let mut start = Duration::ZERO;
        for (segment, (rate, duration)) in self.segments.iter().enumerate() {
            let count = (rate * duration.as_secs_f64())
                .round()
                .to_u64()
                .unwrap_or(0);
            for step in 0..count {
                planned.push(Planned {
                    index: planned.len() as u64,
                    segment,
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
    /// Arrivals dropped for being more than [`MISS_AFTER`] late.
    pub missed: u64,
    /// Arrivals dropped because [`MAX_IN_FLIGHT`] calls were open.
    pub capped: u64,
    /// Send lateness of each sent arrival, microseconds.
    pub lateness_us: Vec<u64>,
    /// Most calls open at once.
    pub peak_in_flight: usize,
}

/// Launches every arrival of `schedule` from `start` as its instant comes,
/// never waiting on a response: each sent arrival holds one of `slots`
/// (sized `cap`) until the future `launch` returns for it completes. Returns
/// once every sent call has completed.
pub async fn open_loop<F, Fut>(
    schedule: &Schedule,
    start: Instant,
    slots: Arc<Semaphore>,
    cap: usize,
    mut launch: F,
) -> Driven
where
    F: FnMut(Planned) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let mut driven = Driven::default();
    for planned in schedule.planned() {
        let at = start + planned.offset;
        tokio::time::sleep_until(at).await;
        driven.scheduled += 1;
        let late = Instant::now().saturating_duration_since(at);
        if late > MISS_AFTER {
            driven.missed += 1;
            continue;
        }
        let Ok(slot) = Arc::clone(&slots).try_acquire_owned() else {
            driven.capped += 1;
            continue;
        };
        driven.peak_in_flight = driven.peak_in_flight.max(cap - slots.available_permits());
        driven.lateness_us.push(micros(late));
        driven.sent += 1;
        let call = launch(planned);
        tokio::spawn(async move {
            call.await;
            drop(slot);
        });
    }
    let cap = u32::try_from(cap).unwrap_or(u32::MAX);
    let _all = slots.acquire_many(cap).await;
    driven
}

/// One completed capacity call.
#[derive(Debug, Clone)]
pub struct Arrived {
    /// Tenant index into [`TENANTS`].
    pub tenant: usize,
    /// Whether it streamed.
    pub stream: bool,
    /// Its scheduled offset from the schedule's start.
    pub offset: Duration,
    /// Its scheduled instant.
    pub scheduled: std::time::Instant,
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

    /// Spawns `load`'s arrivals of `schedule` from `start`, each marked
    /// `<label>-<index>` so its upstream send times match, holding at most
    /// [`MAX_IN_FLIGHT`] of `slots`. Resolves with the driver's counts and
    /// every sent call once all have completed.
    pub fn spawn(
        &self,
        load: Load,
        schedule: Schedule,
        start: Instant,
        label: String,
        slots: Arc<Semaphore>,
    ) -> JoinHandle<(Driven, Vec<Arrived>)> {
        let driver = self.clone();
        tokio::spawn(async move {
            let arrived = Arc::new(Mutex::new(Vec::new()));
            let launch = |planned: Planned| {
                let (tenant, stream) = load.arrival(planned.index);
                let keys = &driver.keys[tenant];
                let key = keys[usize::try_from(planned.index).unwrap_or(0) % keys.len()].clone();
                let marker = format!("{label}-{}", planned.index);
                let shape = driver.shapes[usize::from(stream)].clone();
                let body = shape.body(&marker);
                let (http, upstream, arrived) = (
                    driver.http.clone(),
                    Arc::clone(&driver.upstream),
                    Arc::clone(&arrived),
                );
                let scheduled = (start + planned.offset).into_std();
                async move {
                    let timed = call(&http, &key, &shape, body).await;
                    let sends = upstream.take(&marker);
                    arrived
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(Arrived {
                            tenant,
                            stream,
                            offset: planned.offset,
                            scheduled,
                            timed,
                            sends,
                        });
                }
            };
            let driven = open_loop(&schedule, start, slots, MAX_IN_FLIGHT, launch).await;
            let arrived =
                std::mem::take(&mut *arrived.lock().unwrap_or_else(PoisonError::into_inner));
            (driven, arrived)
        })
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

/// Rows each tenant could read in `bifrost.gateway.calls` over a window,
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

/// What one judged capacity window measured.
#[derive(Debug, Clone, Default)]
pub struct Measured {
    /// Row name.
    pub name: String,
    /// The aggregate offered rate.
    pub rate: f64,
    /// Arrival seconds the window offered.
    pub seconds: f64,
    /// What the driver did, over its whole schedule.
    pub driven: Driven,
    /// The calls this window judges.
    pub calls: Vec<Arrived>,
    /// Backlog readings this window judges.
    pub backlog: Vec<Backlog>,
    /// Server and driver usage over its arrivals.
    pub traffic: Usage,
    /// Seconds its drain took, or `None` when it did not settle.
    pub drain: Option<f64>,
    /// Server usage over the drain.
    pub drained: Usage,
    /// Server telemetry from its baseline to its final scrape.
    pub telemetry: Telemetry,
    /// Capture integrity from its baseline to its final scrape.
    pub integrity: Integrity,
    /// Requests the server parsed.
    pub server_arrived: f64,
    /// Calls the server admitted and settled: those eligible for capture.
    pub admitted: f64,
    /// Per tenant of the case, its durable capture rows.
    pub ownership: Vec<Ownership>,
    /// `None` without profiling; otherwise every missing evidence class.
    pub profile_gaps: Option<Vec<String>>,
}

/// The new observations of `family` matching `labels` from `before` to
/// `after`.
fn delta(before: &Metrics, after: &Metrics, family: &str, labels: &[&str]) -> f64 {
    after.sum(family, labels) - before.sum(family, labels)
}

/// What settling after a window found.
struct Drained {
    /// Seconds it took, or `None` when it did not settle.
    drain: Option<f64>,
    /// Server usage over it.
    usage: Usage,
    /// The final correctness scrape.
    last: Metrics,
    /// Each tenant's capture rows over the window.
    ownership: Vec<Ownership>,
}

impl Drained {
    /// Fills `measured`'s drain, telemetry, integrity, server counts, and
    /// ownership from `baseline` to this final scrape.
    fn reconcile(&self, measured: &mut Measured, baseline: &Metrics) {
        let phase = "wyrd_gateway_phase_duration_seconds_count";
        measured.drain = self.drain;
        measured.drained = self.usage.clone();
        measured.telemetry = Readings::read(baseline, SELECTION).since(&self.last);
        measured.integrity = Integrity::between(baseline, &self.last);
        measured.server_arrived = delta(baseline, &self.last, phase, &["phase=\"parse\""]);
        measured.admitted = delta(baseline, &self.last, phase, &["phase=\"account\""]);
        measured.ownership.clone_from(&self.ownership);
    }
}

/// One server's capacity runs: its tenants, the upstream, the driver, and
/// each window's profile.
pub struct Bench<'a> {
    /// The server.
    server: &'a LocalServer,
    /// Every provisioned tenant, in [`TENANTS`] order.
    tenants: &'a [Tenant],
    /// The upstream every call reaches.
    upstream: Arc<PacedUpstream>,
    /// The arrivals' driver.
    driver: Driver,
    /// Each window's profile.
    profiler: Profiler<'a>,
}

impl<'a> Bench<'a> {
    /// Capacity runs over `server`'s `tenants` against `upstream`, sending
    /// on `http` and profiling with `profiler`.
    pub fn new(
        server: &'a LocalServer,
        tenants: &'a [Tenant],
        upstream: Arc<PacedUpstream>,
        http: Client,
        profiler: Profiler<'a>,
    ) -> Self {
        Self {
            server,
            tenants,
            driver: Driver::new(http, tenants, Arc::clone(&upstream)),
            upstream,
            profiler,
        }
    }

    /// Runs `load` at `rate`: warmup, settle, baseline, the arrival window
    /// with backlog sampling, drain, final scrape and capture reads, and
    /// prints the Pareto.
    ///
    /// # Errors
    ///
    /// Returns a scrape, capture read, profiler, or driver failure, or a
    /// warmup that does not settle.
    pub async fn rate(&self, load: Load, rate: f64) -> Result<Measured> {
        let name = format!("{} {rate}/s", load.name());
        self.warm(load, rate, &name).await?;
        let before = self.captured(load).await?;
        let capture = self.profiler.start(&name)?;
        let (window, baseline) = Window::open(self.server, SELECTION).await?;
        let (driven, calls, backlog) = self
            .run(
                load,
                Schedule::new(vec![(rate, WINDOW)]),
                &name,
                Instant::now() + LEAD,
            )
            .await?;
        let (traffic, _) = window.close(self.server).await?;
        let drained = self.drain(load, &before, &calls).await?;
        let mut measured = Measured {
            name: name.clone(),
            rate,
            seconds: WINDOW.as_secs_f64(),
            driven,
            calls,
            backlog,
            traffic,
            ..Measured::default()
        };
        drained.reconcile(&mut measured, &baseline);
        let metadata = serde_json::json!({"step": name, "rate": rate});
        let profile = record(capture, &measured.telemetry, &metadata)?;
        print_pareto(&measured, profile.as_ref());
        measured.profile_gaps = profile.as_ref().map(gaps);
        Ok(measured)
    }

    /// Runs the overload sequence after `load`'s highest passing `rate`:
    /// [`OVERLOAD`] at twice it, then [`RECOVERY`] back at it, as one
    /// schedule. Returns the overload row (the high interval and the
    /// transition before recovery is judged) and the recovery row (the
    /// final [`RECOVERED`]), each with its own server window, telemetry,
    /// and profile; the drain, integrity, and capture ownership cover the
    /// whole sequence and appear on both.
    ///
    /// # Errors
    ///
    /// Returns a scrape, capture read, profiler, or driver failure.
    pub async fn overload(&self, load: Load, rate: f64) -> Result<(Measured, Measured)> {
        let high_name = format!("{} overload {}/s", load.name(), rate * 2.0);
        let low_name = format!("{} recovery {rate}/s", load.name());
        let cut = OVERLOAD + RECOVERY.saturating_sub(RECOVERED);
        let before = self.captured(load).await?;
        self.upstream.clear();
        let schedule = Schedule::new(vec![(rate * 2.0, OVERLOAD), (rate, RECOVERY)]);
        let start = Instant::now() + LEAD;
        let slots = Arc::new(Semaphore::new(MAX_IN_FLIGHT));
        let stop = CancellationToken::new();
        let high_capture = self.profiler.start(&high_name)?;
        let (high, baseline) = Window::open(self.server, SELECTION).await?;
        let driving =
            self.driver
                .spawn(load, schedule, start, high_name.clone(), Arc::clone(&slots));
        let phases = async {
            tokio::time::sleep_until(start + OVERLOAD).await;
            let (high, high_last) = high.close(self.server).await?;
            let high_telemetry = Readings::read(&baseline, SELECTION).since(&high_last);
            let metadata = serde_json::json!({"step": high_name, "rate": rate * 2.0});
            let telemetry = high_telemetry.clone();
            let high_profile =
                tokio::task::spawn_blocking(move || record(high_capture, &telemetry, &metadata));
            let low_capture = self.profiler.start(&low_name)?;
            tokio::time::sleep_until(start + cut).await;
            let (low, low_baseline) = Window::open(self.server, SELECTION).await?;
            let (driven, calls) = driving.await?;
            stop.cancel();
            let (low, low_last) = low.close(self.server).await?;
            let low_telemetry = Readings::read(&low_baseline, SELECTION).since(&low_last);
            let high = (high, high_telemetry, high_profile.await??);
            Result::Ok((high, (low, low_telemetry, low_capture), driven, calls))
        };
        let (phases, backlog) = tokio::join!(phases, sample(self.server, &slots, start, &stop));
        let (
            (high, high_telemetry, high_profile),
            (low, low_telemetry, low_capture),
            driven,
            calls,
        ) = phases?;
        let backlog = backlog?;
        let drained = self.drain(load, &before, &calls).await?;
        let (late, early): (Vec<Arrived>, Vec<Arrived>) =
            calls.into_iter().partition(|call| call.offset >= cut);
        let (early_backlog, late_backlog): (Vec<Backlog>, Vec<Backlog>) = backlog
            .into_iter()
            .partition(|reading| reading.offset < cut.as_secs_f64());
        let mut overload = Measured {
            name: high_name,
            rate: rate * 2.0,
            seconds: OVERLOAD.as_secs_f64(),
            driven: driven.clone(),
            calls: early,
            backlog: early_backlog
                .into_iter()
                .filter(|reading| reading.offset < OVERLOAD.as_secs_f64())
                .collect(),
            traffic: high,
            ..Measured::default()
        };
        let mut recovery = Measured {
            name: low_name,
            rate,
            seconds: RECOVERED.as_secs_f64(),
            driven,
            calls: late,
            backlog: late_backlog,
            traffic: low,
            ..Measured::default()
        };
        for (measured, telemetry) in [
            (&mut overload, high_telemetry),
            (&mut recovery, low_telemetry),
        ] {
            drained.reconcile(measured, &baseline);
            measured.telemetry = telemetry;
        }
        print_pareto(&overload, high_profile.as_ref());
        overload.profile_gaps = high_profile.as_ref().map(gaps);
        let metadata = serde_json::json!({"step": recovery.name, "rate": rate});
        let low_profile = record(low_capture, &recovery.telemetry, &metadata)?;
        print_pareto(&recovery, low_profile.as_ref());
        recovery.profile_gaps = low_profile.as_ref().map(gaps);
        Ok((overload, recovery))
    }

    /// Sends [`WARMUP`] of `load` at `rate`, warming every key, and settles.
    ///
    /// # Errors
    ///
    /// Returns a scrape or driver failure, or a warmup that does not settle.
    async fn warm(&self, load: Load, rate: f64, name: &str) -> Result<()> {
        let slots = Arc::new(Semaphore::new(MAX_IN_FLIGHT));
        let schedule = Schedule::new(vec![(rate, WARMUP)]);
        self.driver
            .spawn(
                load,
                schedule,
                Instant::now() + LEAD,
                format!("{name} warmup"),
                slots,
            )
            .await?;
        if settle(self.server).await?.is_none() {
            return Err(format!("{name}: warmup did not settle within 60 s").into());
        }
        self.upstream.clear();
        Ok(())
    }

    /// Drives `schedule` of `load` from `start` while sampling backlog.
    ///
    /// # Errors
    ///
    /// Returns a scrape or driver failure.
    async fn run(
        &self,
        load: Load,
        schedule: Schedule,
        label: &str,
        start: Instant,
    ) -> Result<(Driven, Vec<Arrived>, Vec<Backlog>)> {
        let slots = Arc::new(Semaphore::new(MAX_IN_FLIGHT));
        let stop = CancellationToken::new();
        let driving =
            self.driver
                .spawn(load, schedule, start, label.to_owned(), Arc::clone(&slots));
        let driving = async {
            let driven = driving.await;
            stop.cancel();
            driven
        };
        let (driven, backlog) = tokio::join!(driving, sample(self.server, &slots, start, &stop));
        let (driven, calls) = driven?;
        Ok((driven, calls, backlog?))
    }

    /// Settles the server after `calls`, then reads `load`'s tenants'
    /// capture rows against `before`.
    ///
    /// # Errors
    ///
    /// Returns a scrape or capture read failure.
    async fn drain(&self, load: Load, before: &[u64], calls: &[Arrived]) -> Result<Drained> {
        let (draining, _) = Window::open(self.server, Selection::RESOURCES).await?;
        let drain = settle(self.server).await?;
        let (usage, last) = draining.close(self.server).await?;
        let after = self.captured(load).await?;
        let ownership = (0..load.tenants())
            .map(|tenant| {
                let mine = || calls.iter().filter(move |call| call.tenant == tenant);
                Ownership {
                    rows: i64::try_from(after[tenant]).unwrap_or(i64::MAX)
                        - i64::try_from(before[tenant]).unwrap_or(i64::MAX),
                    answered: mine().filter(|call| call.timed.outcome.answered()).count() as u64,
                    admissible: mine().filter(|call| !call.timed.outcome.refused()).count() as u64,
                }
            })
            .collect();
        Ok(Drained {
            drain,
            usage,
            last,
            ownership,
        })
    }

    /// Each of `load`'s tenants' readable capture rows.
    ///
    /// # Errors
    ///
    /// Returns the capture read failure.
    async fn captured(&self, load: Load) -> Result<Vec<u64>> {
        let mut rows = Vec::with_capacity(load.tenants());
        for tenant in self.tenants.iter().take(load.tenants()) {
            rows.push(tenant.captured().await?);
        }
        Ok(rows)
    }
}

/// Prints `measured`'s runtime and CPU Pareto.
fn print_pareto(measured: &Measured, profile: Option<&wyrd_testing::profile::Profile>) {
    pareto(
        &Evidence {
            name: &measured.name,
            calls: measured.calls.len() as u64,
            errors: measured
                .calls
                .iter()
                .filter(|call| !call.timed.outcome.answered())
                .count() as u64,
            traffic: &measured.traffic,
            drain: measured.drain,
            telemetry: &measured.telemetry,
        },
        profile,
    );
}
