//! The L2 `competing` workload: the heavy fixture with Forge compaction on,
//! and one stateful step whose every repetition (and warmup) runs open-loop
//! mixed ingest, open-loop mixed queries, and the client-queue lanes
//! together through capacity → overload → recovery.
//!
//! Capacity runs the first 5/12 of the window. Overload then fills Oracle's
//! queue behind held execution slots while ingest and queries continue, and
//! sends one more query, which must be refused as queue-full. Recovery runs
//! to the window's end; its readings start 30 s after overload ends, so
//! meeting the SLOs there means they were restored within 30 s. Queue-full
//! is the one declared refusal, and only from overload's start until that
//! grace ends.

use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use wyrd_testing::release_server::PEAK_SAMPLE_INTERVAL_MS;

use async_trait::async_trait;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use num_traits::ToPrimitive;
use secrecy::ExposeSecret as _;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_util::task::AbortOnDropHandle;
use vala_bifrost_redux::catalog::{BifrostCatalog, TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_client::transport::trace::with_benchmark_labels;
use wyrd_spec::DataTenantId;
use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};
use wyrd_telemetry::timing::Distribution;
use wyrd_testing::capacity::{
    Expected, NANOS_PER_MS, Percentiles, Population, QUERY_BOUNDARY, Selection, Window,
};
use wyrd_testing::experiment::{
    Arrivals, ExperimentError, Phase, Reading, Repetition, Step, Workload,
};
use wyrd_testing::release_server::LocalServer;

use crate::client_queue::{self, Emitted, LANES};
use crate::query::isolated::{INSERTED, MIB, SUCCESS};
use crate::query::run::{
    Admission, Bench, Overload, QUEUE_FULL, Sender, WRONG_RESULT, ask, micros,
};
use crate::query::workload::{Case, Fixture, INGEST_TABLE, TABLE};
use crate::query::{Lane, PARQUET_ENV, Residence, SPLIT, Scale, WriteOp, client, lanes, start};

/// Mixed queries in flight at most; a due query finding this many
/// unanswered is missed rather than sent.
pub const QUERY_CLIENTS: usize = 32;

/// Offered mixed queries per second, independent of answers: 4,320 per
/// 360 s repetition, 216 of them full scans at the 5% mix, above the
/// 4,000-query and 200-scan floors.
pub const QUERY_RATE: f64 = 12.0;

/// How long after overload the SLOs must hold again, at most half of what
/// remains of the window; recovery is read only after it.
const RECOVERY_GRACE: Duration = Duration::from_secs(30);

/// Pending ingest arrivals past which a scheduled arrival is missed rather
/// than sent, so a stalled server cannot grow the driver without bound.
const MAX_PENDING: usize = 4_096;

/// Concurrent writers loading the heavy fixture.
const LOADERS: usize = 16;

/// How long the heavy fixture may take to publish after its load.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(600);

/// How often a visibility probe asks for its row.
const VISIBILITY_POLL: Duration = Duration::from_millis(50);

/// How long a visibility probe asks before recording the row as not
/// visible.
const VISIBILITY_WAIT: Duration = Duration::from_secs(30);

/// Iceberg property that opts one table into compaction.
const ENABLE_COMPACTION: &str = "wyrd.forge.enable-compaction";

/// Iceberg property naming the promotion commits that make a table due.
const TRIGGER_SNAPSHOT_COUNT: &str = "wyrd.forge.compaction.trigger-snapshot-count";

/// Promotion commits that make an ingest table due for compaction.
const COMPACTION_TRIGGER: usize = 2;

/// The case a mixed query client sends for `sequence`: of every 20, 12
/// point lookups, 3 small and 2 1M-row dashboard aggregates, 2 whole-table
/// aggregates, and 1 full scan.
const fn mixed_case(sequence: u64) -> Case {
    match sequence % 20 {
        0..=11 => Case::Selective,
        12..=14 => Case::SmallAggregate,
        15..=16 => Case::MillionAggregate,
        17..=18 => Case::TableAggregate,
        _ => Case::FullScan,
    }
}

/// One answered or failed query.
struct Asked {
    /// When it answered.
    at: Instant,
    /// The admission phase it was sent in.
    phase: &'static str,
    /// What it asked.
    case: Case,
    /// Its diagnostic client latency in microseconds, or its error code.
    result: Result<u64, String>,
}

/// One acknowledged or failed ingest request.
struct Acked {
    /// When it answered.
    at: Instant,
    /// The admission phase it was sent in.
    phase: &'static str,
    /// Rows it carried.
    rows: u64,
    /// Encoded OTLP protobuf bytes it sent; zero for native writes.
    wire: u64,
    /// Its diagnostic client latency in microseconds, or its error code.
    result: Result<u64, String>,
}

/// What one open loop, an ingest path's or the mixed queries', scheduled,
/// sent, and observed.
struct Offered<T> {
    /// Arrivals the schedule planned.
    scheduled: u64,
    /// Arrivals sent.
    sent: u64,
    /// Planned arrivals never sent because the loop's bound was pending.
    missed: u64,
    /// Each sent arrival's lateness behind its schedule, microseconds.
    lateness_us: Vec<u64>,
    /// Every answered arrival.
    acked: Vec<T>,
    /// When the last pending arrival answered.
    drained: Option<Instant>,
}

impl<T> Default for Offered<T> {
    fn default() -> Self {
        Self {
            scheduled: 0,
            sent: 0,
            missed: 0,
            lateness_us: Vec::new(),
            acked: Vec::new(),
            drained: None,
        }
    }
}

/// Sends one arrival through `launch` every `interval` from `start`, the
/// window's length over `interval` of them rounded, so the last is due at
/// least half an interval before `end`; it never waits on an answer before
/// the next is due, and a due arrival finding `bound` pending, or waking at
/// or after `end`, is missed. Waits for every sent arrival and keeps its
/// answer.
async fn open_loop<T, Fut>(
    start: Instant,
    end: Instant,
    interval: Duration,
    bound: usize,
    mut launch: impl FnMut() -> Fut,
) -> Offered<T>
where
    T: Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    let mut offered = Offered::default();
    let mut pending = JoinSet::new();
    let planned = (end.saturating_duration_since(start).as_secs_f64() / interval.as_secs_f64())
        .round()
        .to_u32()
        .unwrap_or(0);
    for arrival in 0..planned {
        let due = start + interval.mul_f64(f64::from(arrival));
        tokio::time::sleep_until(due).await;
        while let Some(joined) = pending.try_join_next() {
            offered.acked.extend(joined.ok());
        }
        offered.scheduled += 1;
        if pending.len() >= bound || Instant::now() >= end {
            offered.missed += 1;
            continue;
        }
        offered.sent += 1;
        offered.lateness_us.push(micros(due.into_std()));
        pending.spawn(launch());
    }
    while let Some(joined) = pending.join_next().await {
        offered.acked.extend(joined.ok());
    }
    offered.drained = Some(Instant::now());
    offered
}

/// Runs each of `tasks` as its own Tokio task and joins them in order. The
/// returned future owns every task: dropping it, as a budget overrun or an
/// interrupt drops a repetition, aborts each one still running, so no lane
/// keeps emitting after measurement stopped.
fn owned<T, Fut>(
    tasks: impl IntoIterator<Item = Fut>,
) -> impl Future<Output = Vec<Result<T, tokio::task::JoinError>>>
where
    T: Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    futures_util::future::join_all(
        tasks
            .into_iter()
            .map(|task| AbortOnDropHandle::new(tokio::spawn(task))),
    )
}

/// The admission phases a repetition labels its offered requests with:
/// capacity until overload starts, overload until its recovery grace ends,
/// and recovery after; only capacity and recovery are judged.
const ADMISSIONS: [&str; 3] = ["capacity", "overload", "recovery"];

/// Which admission phase a request sent now belongs to. Recovery's start is
/// known only once overload ends.
struct Admitting {
    /// Capacity end and overload start.
    overload: Instant,
    /// Recovery-grace end, once overload ended.
    recovery: OnceLock<Instant>,
}

impl Admitting {
    /// The [`ADMISSIONS`] label of a request sent at `at`.
    fn label(&self, at: Instant) -> &'static str {
        if at < self.overload {
            ADMISSIONS[0]
        } else if self.recovery.get().is_some_and(|recovery| at >= *recovery) {
            ADMISSIONS[2]
        } else {
            ADMISSIONS[1]
        }
    }
}

/// The repetition's phase boundaries.
#[derive(Clone, Copy)]
struct Phases {
    /// Capacity end and overload start.
    overload: Instant,
    /// Overload end.
    overloaded: Instant,
    /// Window end: the admission stop.
    end: Instant,
}

impl Phases {
    /// Where the recovery grace ends and recovery starts: [`RECOVERY_GRACE`]
    /// after overload ends, or half of what then remains of the window when
    /// that is shorter, so a short window still reads a recovery.
    fn recovery(&self) -> Instant {
        let remaining = self.end.saturating_duration_since(self.overloaded);
        self.overloaded + RECOVERY_GRACE.min(remaining / 2)
    }

    /// Whether a queue-full refusal at `at` is the declared one: from
    /// overload's start until the recovery grace ends.
    fn refusal_allowed(&self, at: Instant) -> bool {
        at >= self.overload && at < self.recovery()
    }
}

/// The L2 server, its fixtures, and the shared cursors of every loop.
pub struct Competing {
    /// Where the server log and tenant files are kept.
    logs: PathBuf,
    /// The running server, once prepared.
    server: Option<LocalServer>,
    /// The query handle and exact answers, once prepared.
    bench: Option<Arc<Bench>>,
    /// Each ingest path's sender and cursor, in [`SPLIT`] order.
    senders: Vec<Lane>,
    /// The client-queue tenant, once provisioned.
    tenant: Option<Arc<client_queue::Tenant>>,
    /// The statement sequence every repetition continues.
    sequence: Arc<AtomicU64>,
    /// The fixture sizes it loads.
    scale: Scale,
}

impl Competing {
    /// An unprepared workload at `scale` keeping its files under `logs`.
    pub fn new(logs: PathBuf, scale: Scale) -> Self {
        Self {
            logs,
            scale,
            server: None,
            bench: None,
            senders: Vec::new(),
            tenant: None,
            sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Runs one whole capacity → overload → recovery sequence of `step` over
    /// `window`, labelling every offered request with its admission phase,
    /// freezes work, seconds, and CPU at the window end, drains, collects the
    /// drained server timings, and reads every phase's cohort from them.
    ///
    /// # Errors
    ///
    /// Returns a workload error for an unprepared workload, a step without a
    /// positive rate, or a scrape, overload, or lane failure.
    async fn competing(
        &self,
        step: &Step,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        let (Some(server), Some(bench), Some(tenant)) = (&self.server, &self.bench, &self.tenant)
        else {
            return Err(ExperimentError::workload("the workload was not prepared"));
        };
        let rate = step
            .params
            .get("ingest_rows_per_s")
            .and_then(serde_json::Value::as_f64)
            .filter(|rate| rate.is_finite() && *rate > 0.0)
            .ok_or_else(|| {
                ExperimentError::workload(format!(
                    "step `{}` declares no positive ingest rate",
                    step.id
                ))
            })?;
        let timings = server.timings().await.map_err(ExperimentError::workload)?;
        let (opened, before) = Window::open(server, Selection::RESOURCES)
            .await
            .map_err(ExperimentError::workload)?;
        let start = Instant::now();
        let end = start + window;
        let admitting = Admitting {
            overload: start + window * 5 / 12,
            recovery: OnceLock::new(),
        };
        let outstanding = Arc::new(AtomicI64::new(0));
        let (visible, probes) = mpsc::unbounded_channel();
        let ingest = futures_util::future::join_all(SPLIT.iter().zip(&self.senders).map(
            |((_, share), (op, sender, cursor))| {
                let probe = (*op == WriteOp::Native).then(|| visible.clone());
                offer(
                    (*op, sender, cursor),
                    rate * share,
                    (start, end),
                    &admitting,
                    &outstanding,
                    probe,
                )
            },
        ));
        drop(visible);
        let prober = probe(bench.queries(), probes, &admitting);
        let queries = mixed(bench, start, end, &self.sequence, &admitting);
        // Each lane is its own task: its WyrdState load and submissions are
        // synchronous work that, polled beside the open-loop schedulers,
        // would make their arrivals late.
        let lanes = owned((0..LANES.len()).map(|index| {
            let tenant = Arc::clone(tenant);
            async move { LANES[index].run(&tenant, end).await }
        }));
        let overload = async {
            tokio::time::sleep_until(admitting.overload).await;
            let before = outstanding.load(Ordering::Relaxed);
            let overload = bench.overload_queue(server).await;
            let overloaded = Instant::now();
            let phases = Phases {
                overload: admitting.overload,
                overloaded,
                end,
            };
            let _ = admitting.recovery.set(phases.recovery());
            let backlog = outstanding.load(Ordering::Relaxed);
            (before, overload, overloaded, backlog)
        };
        let stopped = async {
            tokio::time::sleep_until(end).await;
            outstanding.load(Ordering::Relaxed)
        };
        let work = async { tokio::join!(ingest, prober, queries, lanes, overload, stopped) };
        let (driven, usage, stopped) = Box::pin(opened.measure(server, end.into_std(), work))
            .await
            .map_err(ExperimentError::workload)?;
        let (offered, visibility, queries, lanes, overload, backlog_end) = driven;
        let asked = &queries.acked;
        let (backlog_capacity, overload, overloaded, backlog_overloaded) = overload;
        let overload = overload.map_err(ExperimentError::workload)?;
        let population = Population::collect(server, &timings, &expected(asked, &offered)).await;
        let phases = Phases {
            overload: admitting.overload,
            overloaded,
            end,
        };
        let mut repetition = Repetition {
            seconds: usage.seconds,
            processes: vec!["wyrd-server".to_owned()],
            server_cpu_seconds: usage.server_cpu_seconds,
            peak_memory_bytes: usage.peak_memory_bytes,
            memory_interval_ms: PEAK_SAMPLE_INTERVAL_MS,
            driver_cpu_seconds: usage.driver_cpu_seconds,
            ..Repetition::default()
        };
        let backlog = |rows: i64| Reading::exact(rows.to_f64().unwrap_or(f64::MAX) / rate);
        repetition.set("capacity.backlog_growth_s", backlog(backlog_capacity));
        repetition.set(
            "recovery.backlog_growth_s",
            backlog(backlog_end - backlog_overloaded),
        );
        population.validate(&mut repetition, "measured");
        read_queries(&mut repetition, asked, &population, &phases);
        read_ingest(&mut repetition, &offered, &visibility, &population, &phases);
        let inserted = stopped.sum(INSERTED, &[]) - before.sum(INSERTED, &[]);
        repetition.set("retained_mib", Reading::exact(inserted / MIB));
        read_arrivals(&mut repetition, &offered, &queries);
        read_overload(&mut repetition, &overload, &queries.acked, &phases);
        for (lane, emitted) in LANES.iter().zip(lanes) {
            let emitted = emitted
                .map_err(ExperimentError::workload)?
                .map_err(ExperimentError::workload)?;
            read_lane(&mut repetition, lane.name, &emitted, &phases);
        }
        Ok(repetition)
    }
}

/// The exact server observations the drained window owes, by case and
/// admission phase: each answered query, a wrong answer included, at
/// `bifrost.query`, and each acknowledged ingest request at its path's
/// boundary. Visibility probes, the overload's held queries, and the
/// client-queue lanes carry other labels, so none can cover an offered one.
fn expected(asked: &[Asked], offered: &[Offered<Acked>]) -> Vec<Expected<'static>> {
    let mut expected = Vec::new();
    for phase in ADMISSIONS {
        for case in MIXED {
            let answered = asked
                .iter()
                .filter(|asked| asked.case == case && asked.phase == phase)
                .filter(|asked| {
                    asked
                        .result
                        .as_ref()
                        .map_or_else(|code| code == WRONG_RESULT, |_| true)
                })
                .count();
            expected.push(Expected {
                boundary: QUERY_BOUNDARY,
                outcome: SUCCESS,
                case: case.label(),
                phase,
                count: answered.to_u64().unwrap_or(u64::MAX),
            });
        }
        for ((op, _), offered) in SPLIT.iter().zip(offered) {
            let acked = offered
                .acked
                .iter()
                .filter(|acked| acked.phase == phase && acked.result.is_ok())
                .count();
            expected.push(Expected {
                boundary: op.boundary(),
                outcome: SUCCESS,
                case: op.name(),
                phase,
                count: acked.to_u64().unwrap_or(u64::MAX),
            });
        }
    }
    expected
}

/// The mixed query classes.
const MIXED: [Case; 5] = [
    Case::Selective,
    Case::SmallAggregate,
    Case::MillionAggregate,
    Case::TableAggregate,
    Case::FullScan,
];

/// Sends path `op`'s `sender` open loop at `rows_per_s` from `start` until
/// `end`, each request labelled with `op` and its admission phase, counting
/// pending rows in `outstanding`, sending each native acknowledgement's last
/// id and time to `probe`, then waits for every pending arrival.
async fn offer(
    (op, sender, cursor): (WriteOp, &Sender, &Arc<AtomicI64>),
    rows_per_s: f64,
    (start, end): (Instant, Instant),
    admitting: &Admitting,
    outstanding: &Arc<AtomicI64>,
    probe: Option<mpsc::UnboundedSender<(Instant, i64)>>,
) -> Offered<Acked> {
    let chunk = sender.request_rows();
    let interval = Duration::from_secs_f64(chunk.to_f64().unwrap_or(f64::MAX) / rows_per_s);
    open_loop(start, end, interval, MAX_PENDING, || {
        let first = cursor.fetch_add(chunk, Ordering::Relaxed);
        let phase = admitting.label(Instant::now());
        let (sender, outstanding, probe) = (sender.clone(), Arc::clone(outstanding), probe.clone());
        outstanding.fetch_add(chunk, Ordering::Relaxed);
        async move {
            let sent = std::time::Instant::now();
            let sent_bytes =
                with_benchmark_labels(op.name(), phase, sender.send(first, first + chunk)).await;
            let wire = *sent_bytes.as_ref().unwrap_or(&0);
            let result = sent_bytes.map(|_| micros(sent));
            outstanding.fetch_sub(chunk, Ordering::Relaxed);
            let at = Instant::now();
            if let (Ok(_), Some(probe)) = (&result, probe) {
                let _ = probe.send((at, first + chunk - 1));
            }
            Acked {
                at,
                phase,
                rows: chunk.cast_unsigned(),
                wire,
                result,
            }
        }
    })
    .await
}

/// Asks, for every acknowledged native write on `acks`, until its last row
/// is queryable, and returns each acknowledgement's admission phase with
/// its visibility delay in microseconds, or `None` past
/// [`VISIBILITY_WAIT`]. Probes are incidental work, labelled `probe`.
async fn probe(
    queries: Arc<wyrd_client::Bifrost>,
    mut acks: mpsc::UnboundedReceiver<(Instant, i64)>,
    admitting: &Admitting,
) -> Vec<(&'static str, Option<u64>)> {
    let mut probes = JoinSet::new();
    while let Some((acked, id)) = acks.recv().await {
        let queries = Arc::clone(&queries);
        let phase = admitting.label(acked);
        probes.spawn(async move {
            let sql = format!("SELECT COUNT(*) AS n FROM {INGEST_TABLE} WHERE event_id = {id}");
            let one = vec![vec![1]];
            while acked.elapsed() < VISIBILITY_WAIT {
                let asked = ask(&queries, sql.clone(), &one);
                if with_benchmark_labels("probe", phase, asked).await.is_ok() {
                    return (phase, Some(micros(acked.into_std())));
                }
                tokio::time::sleep(VISIBILITY_POLL).await;
            }
            (phase, None)
        });
    }
    let mut seen = Vec::new();
    while let Some(joined) = probes.join_next().await {
        seen.extend(joined.ok());
    }
    seen
}

/// Sends mixed queries open loop at [`QUERY_RATE`] from `start` until
/// `end`, independent of answers, with at most [`QUERY_CLIENTS`] in flight,
/// each labelled with its class and admission phase.
async fn mixed(
    bench: &Arc<Bench>,
    start: Instant,
    end: Instant,
    sequence: &Arc<AtomicU64>,
    admitting: &Admitting,
) -> Offered<Asked> {
    let queries = bench.queries();
    let interval = Duration::from_secs_f64(1.0 / QUERY_RATE);
    open_loop(start, end, interval, QUERY_CLIENTS, || {
        let next = sequence.fetch_add(1, Ordering::Relaxed);
        let case = mixed_case(next);
        let phase = admitting.label(Instant::now());
        let (sql, expected) = bench.question(case, next);
        let queries = Arc::clone(&queries);
        async move {
            let sent = std::time::Instant::now();
            let result = with_benchmark_labels(case.label(), phase, ask(&queries, sql, &expected))
                .await
                .map(|()| micros(sent));
            Asked {
                at: Instant::now(),
                phase,
                case,
                result,
            }
        }
    })
    .await
}

/// Nearest-rank percentile `q`, milliseconds, of the successful `boundary`
/// observations of `case` admitted in `phase`; missing without one.
fn server_percentile(
    population: &Population,
    boundary: &str,
    (case, phase): (&str, &str),
    q: f64,
) -> Reading {
    population
        .only(case, phase)
        .elapsed(boundary, SUCCESS)
        .map_or_else(
            || Reading::missing(format!("no `{boundary}` {case} answered in the phase")),
            |distribution| Reading::quantile(&distribution, q, NANOS_PER_MS),
        )
}

/// The slowest of `samples`, milliseconds, or missing without one.
fn slowest(samples: &[u64], what: &str) -> Reading {
    samples.iter().max().map_or_else(
        || Reading::missing(format!("no {what} answered in the phase")),
        |max| {
            Reading::exact(max.to_f64().unwrap_or(f64::MAX) / 1e3)
                .with_samples(samples.len().to_u64().unwrap_or(0))
        },
    )
}

/// Sets every query reading: per-phase server latencies and answered
/// counts of the queries admitted in the phase, however late they
/// completed; the capacity-phase point percentiles and maximum; totals, errors, and
/// wrong answers. Totals credit only queries answered before the window
/// end, the endpoint of the measured CPU; later answers are counted
/// separately. Scanned rows and bytes per query average the executed-plan
/// scans of the mixed queries the server answered, never a probe's.
fn read_queries(
    repetition: &mut Repetition,
    asked: &[Asked],
    population: &Population,
    phases: &Phases,
) {
    for phase in [ADMISSIONS[0], ADMISSIONS[2]] {
        let id = |name: &str| format!("{phase}.{name}");
        let read =
            |case: Case, q| server_percentile(population, QUERY_BOUNDARY, (case.label(), phase), q);
        repetition.set(&id("point_p99_ms"), read(Case::Selective, 0.99));
        repetition.set(&id("small_p95_ms"), read(Case::SmallAggregate, 0.95));
        repetition.set(&id("million_p95_ms"), read(Case::MillionAggregate, 0.95));
        repetition.set(&id("table_p95_ms"), read(Case::TableAggregate, 0.95));
        repetition.set(&id("scan_p95_ms"), read(Case::FullScan, 0.95));
        let answered = asked
            .iter()
            .filter(|asked| asked.phase == phase && asked.result.is_ok())
            .count();
        repetition.set(
            &id("queries"),
            Reading::count(answered.to_u64().unwrap_or(0)),
        );
    }
    for (id, q) in [
        ("p50_ms", 0.5),
        ("p95_ms", 0.95),
        ("p99_ms", 0.99),
        ("max_ms", 1.0),
    ] {
        let point = Case::Selective.label();
        repetition.set(
            id,
            server_percentile(population, QUERY_BOUNDARY, (point, ADMISSIONS[0]), q),
        );
    }
    let answered = |late: bool, case: Option<Case>| {
        asked
            .iter()
            .filter(|asked| asked.result.is_ok() && (asked.at >= phases.end) == late)
            .filter(|asked| case.is_none_or(|case| asked.case == case))
            .count()
            .to_u64()
            .unwrap_or(0)
    };
    repetition.set("queries", Reading::count(answered(false, None)));
    repetition.set("queries_after_window", Reading::count(answered(true, None)));
    let mut mixed = Population::default();
    for phase in ADMISSIONS {
        for case in MIXED {
            let cohort = population.only(case.label(), phase);
            mixed.series.extend(cohort.series);
            mixed.scans.extend(cohort.scans);
        }
    }
    let [rows, bytes] = mixed.scanned_per_query(SUCCESS);
    repetition.set("scanned_rows_per_query", rows);
    repetition.set("scanned_bytes_per_query", bytes);
    repetition.set(
        "table_aggregates",
        Reading::count(answered(false, Some(Case::TableAggregate))),
    );
    let wrong = asked
        .iter()
        .filter(|asked| {
            asked
                .result
                .as_ref()
                .err()
                .is_some_and(|code| code == WRONG_RESULT)
        })
        .count();
    repetition.set("wrong_results", Reading::count(wrong.to_u64().unwrap_or(0)));
}

/// Sets every ingest reading: per-phase server acknowledgement p99 over
/// every successful offered write admitted in the phase, the diagnostic
/// visibility, and rows; totals; the drain; and adds
/// ingest failures and undeclared query refusals to `errors`. The row total
/// credits only acknowledgements before the window end, the endpoint of the
/// measured CPU; drain acknowledgements are counted separately and every
/// failure stays an error.
fn read_ingest(
    repetition: &mut Repetition,
    offered: &[Offered<Acked>],
    visibility: &[(&'static str, Option<u64>)],
    population: &Population,
    phases: &Phases,
) {
    let acked: Vec<&Acked> = offered.iter().flat_map(|offered| &offered.acked).collect();
    for phase in [ADMISSIONS[0], ADMISSIONS[2]] {
        let id = |name: &str| format!("{phase}.{name}");
        let mut acks: Option<Distribution> = None;
        for (op, _) in SPLIT {
            let cohort = population.only(op.name(), phase);
            let Some(part) = cohort.elapsed(op.boundary(), SUCCESS) else {
                continue;
            };
            match &mut acks {
                Some(total) => {
                    if let Err(reason) = total.add(&part) {
                        repetition.invalid.push(reason);
                    }
                }
                None => acks = Some(part),
            }
        }
        repetition.set(
            &id("write_ack_p99_ms"),
            acks.map_or_else(
                || Reading::missing("no write was acknowledged in the phase"),
                |acks| Reading::quantile(&acks, 0.99, NANOS_PER_MS),
            ),
        );
        let rows: u64 = acked
            .iter()
            .filter(|acked| acked.phase == phase && acked.result.is_ok())
            .map(|acked| acked.rows)
            .sum();
        repetition.set(&id("ingest_rows"), Reading::count(rows));
        let seen: Vec<Option<u64>> = visibility
            .iter()
            .filter(|(admitted, _)| *admitted == phase)
            .map(|(_, delay)| *delay)
            .collect();
        let reading = if seen.iter().any(Option::is_none) {
            Reading::above(VISIBILITY_WAIT.as_secs_f64() * 1e3)
        } else {
            slowest(
                &seen.into_iter().flatten().collect::<Vec<_>>(),
                "native write",
            )
        };
        repetition.set(&id("visibility_max_ms"), reading);
    }
    let rows = |late: bool| {
        acked
            .iter()
            .filter(|acked| acked.result.is_ok() && (acked.at >= phases.end) == late)
            .map(|acked| acked.rows)
            .sum()
    };
    repetition.set("ingest_rows", Reading::count(rows(false)));
    let wire: u64 = acked
        .iter()
        .filter(|acked| acked.result.is_ok() && acked.at < phases.end)
        .map(|acked| acked.wire)
        .sum();
    repetition.set(
        "otlp_wire_mib",
        Reading::exact(wire.to_f64().unwrap_or(f64::MAX) / MIB),
    );
    repetition.set("ingest_rows_after_window", Reading::count(rows(true)));
    let drained = offered
        .iter()
        .filter_map(|offered| offered.drained)
        .max()
        .map_or(0.0, |drained| {
            drained.saturating_duration_since(phases.end).as_secs_f64()
        });
    repetition.set("drain_s", Reading::exact(drained));
    let failed = acked.iter().filter(|acked| acked.result.is_err()).count();
    repetition.set("errors", Reading::count(failed.to_u64().unwrap_or(0)));
}

/// Sets the arrivals of every ingest path's open loop and the mixed
/// queries' together, and names each loop that missed a due arrival.
fn read_arrivals(
    repetition: &mut Repetition,
    offered: &[Offered<Acked>],
    queries: &Offered<Asked>,
) {
    let loops = offered
        .iter()
        .map(|offered| {
            (
                offered.scheduled,
                offered.sent,
                offered.missed,
                &offered.lateness_us,
                offered.acked.len(),
            )
        })
        .chain([(
            queries.scheduled,
            queries.sent,
            queries.missed,
            &queries.lateness_us,
            queries.acked.len(),
        )]);
    let mut arrivals = Arrivals::default();
    let mut lateness = Vec::new();
    for (scheduled, sent, missed, late, completed) in loops {
        arrivals.scheduled += scheduled;
        arrivals.sent += sent;
        arrivals.missed += missed;
        arrivals.completed += completed.to_u64().unwrap_or(0);
        lateness.extend(late);
    }
    arrivals.lateness_p99_us = Percentiles::raw(&lateness, 1.0).p99;
    let ingest_missed: u64 = offered.iter().map(|offered| offered.missed).sum();
    if ingest_missed > 0 {
        repetition.invalid.push(format!(
            "the driver missed {ingest_missed} of {} scheduled ingest arrivals: {MAX_PENDING} were pending or it woke at the window end",
            arrivals.scheduled - queries.scheduled
        ));
    }
    if queries.missed > 0 {
        repetition.invalid.push(format!(
            "the driver missed {} of {} scheduled mixed queries: {QUERY_CLIENTS} were in flight or it woke at the window end",
            queries.missed, queries.scheduled
        ));
    }
    repetition.arrivals = Some(arrivals);
}

/// Sets the overload readings, and adds every query failure except a
/// declared queue-full refusal to `errors`.
fn read_overload(
    repetition: &mut Repetition,
    overload: &Overload,
    asked: &[Asked],
    phases: &Phases,
) {
    repetition.set(
        "overload.queued",
        Reading::count(overload.queued.to_u64().unwrap_or(0)),
    );
    repetition.set(
        "overload.overflow_refused",
        Reading::count(u64::from(overload.overflow_refused)),
    );
    if !overload.overflow_refused {
        repetition.observations.push(format!(
            "the overflow query observed `{}`",
            overload.overflow
        ));
    }
    repetition.set(
        "overload.queue_drain_s",
        overload
            .drained_seconds
            .map_or_else(|| Reading::above(60.0), Reading::exact),
    );
    let refused = |asked: &&Asked| {
        asked
            .result
            .as_ref()
            .err()
            .is_some_and(|code| code == QUEUE_FULL)
    };
    let declared = asked
        .iter()
        .filter(refused)
        .filter(|asked| phases.refusal_allowed(asked.at))
        .count();
    repetition.set(
        "overload.queue_full",
        Reading::count(declared.to_u64().unwrap_or(0)),
    );
    let undeclared = asked
        .iter()
        .filter(|asked| asked.result.is_err())
        .filter(|asked| !(refused(asked) && phases.refusal_allowed(asked.at)))
        .count();
    let ingest = repetition.reading("errors").value.unwrap_or(0.0);
    repetition.set(
        "errors",
        Reading::exact(ingest + undeclared.to_f64().unwrap_or(f64::MAX)),
    );
}

/// Sets lane `name`'s readings.
fn read_lane(repetition: &mut Repetition, name: &str, emitted: &Emitted, phases: &Phases) {
    let id = |metric: &str| format!("queue.{name}.{metric}");
    let during = emitted
        .refusals
        .iter()
        .filter(|at| phases.refusal_allowed(**at))
        .count();
    let outside = emitted.refusals.len() - during;
    repetition.set(
        &id("refusals"),
        Reading::count(outside.to_u64().unwrap_or(0)),
    );
    repetition.set(
        &id("overload_refusals"),
        Reading::count(during.to_u64().unwrap_or(0)),
    );
    repetition.set(&id("row_mismatch"), Reading::count(emitted.row_mismatch));
    repetition.set(&id("bytes_growth"), Reading::exact(emitted.bytes_growth));
    repetition.set(&id("drain_s"), Reading::exact(emitted.drain_seconds));
    repetition.set(&id("achieved_rate"), Reading::exact(emitted.achieved_rate));
}

/// The Iceberg catalog over the serving platform login and the server's
/// `file://` store, as an operator reaches it: the storage settings are the
/// server's own store, never process environment.
///
/// # Errors
///
/// Returns missing DSN settings, or a storage or connection failure.
async fn operator_catalog(server: &LocalServer) -> crate::query::Result<BifrostCatalog> {
    let dsns = wyrd_sql::dsn::ResolvedDsns::from_env()?;
    let storage = StorageHandle::from_settings(StorageSettings {
        backend: BackendConfig::Local {
            root: server.storage_dir(),
        },
        require_encryption: false,
        presign_ttl: Duration::from_mins(10),
        part_size_bytes: 16 * 1024 * 1024,
        multipart_threshold_bytes: 100 * 1024 * 1024,
    })
    .await?;
    Ok(BifrostCatalog::new(
        dsns.catalog().expose_secret(),
        wyrd_testing::server::test_storage_owner(&storage),
        vala_sql::ValaPostgres::connect_from_dsns(&dsns).await?,
    )
    .await?)
}

/// Commits [`ENABLE_COMPACTION`] and a [`TRIGGER_SNAPSHOT_COUNT`] of
/// [`COMPACTION_TRIGGER`] onto `tenant`'s table `name` in `namespace`.
///
/// # Errors
///
/// Returns a binding, load, transaction, or commit failure.
async fn enable_compaction(
    catalog: &BifrostCatalog,
    tenant: DataTenantId,
    namespace: BifrostNamespace,
    name: &str,
) -> crate::query::Result<()> {
    let binding = TenantTableBinding::resolve((tenant, TableRef::new(namespace, name)))?;
    let iceberg = catalog.iceberg_catalog();
    let table = iceberg.load_table(&binding.table_ident()).await?;
    let tx = Transaction::new(&table);
    tx.update_table_properties()
        .set(ENABLE_COMPACTION.to_owned(), "true".to_owned())
        .set(
            TRIGGER_SNAPSHOT_COUNT.to_owned(),
            COMPACTION_TRIGGER.to_string(),
        )
        .apply(tx)?
        .commit_once(iceberg.as_ref())
        .await?;
    Ok(())
}

impl Competing {
    /// Writes one request through every ingest lane of the started server
    /// so each ingest table exists, loads and publishes the heavy fixture of
    /// [`Scale::heavy`] rows, computes the exact answers, enables compaction
    /// on every ingest table, and provisions the client-queue tenant.
    ///
    /// # Errors
    ///
    /// Returns a server that did not start, a refused seed write, or a
    /// client, load, publication, catalog, or provisioning failure.
    async fn start(&mut self) -> crate::query::Result<()> {
        let server = self.server.as_ref().ok_or("the server did not start")?;
        let client = client(server)?;
        for (op, sender, cursor) in &self.senders {
            let chunk = sender.request_rows();
            let first = cursor.fetch_add(chunk, Ordering::Relaxed);
            sender.send(first, first + chunk).await.map_err(|code| {
                format!("the `{}` seed write was refused with `{code}`", op.name())
            })?;
        }
        let fixture = Fixture {
            rows: self.scale.heavy,
        };
        let loaded = Sender::native(&client, TABLE, fixture)
            .await?
            .closed_loop(
                &Arc::new(AtomicI64::new(0)),
                fixture.rows,
                LOADERS,
                Admission::default(),
            )
            .await;
        if loaded.error_count() > 0 || loaded.rows != fixture.rows.cast_unsigned() {
            return Err(format!(
                "the heavy fixture loaded {} of {} rows with errors {:?}",
                loaded.rows, fixture.rows, loaded.errors
            )
            .into());
        }
        Residence::settle(server, SETTLE_TIMEOUT).await?;
        self.bench = Some(Arc::new(Bench::new(&client, fixture, &MIXED)));
        let [setup] = server.tenants() else {
            return Err("setup provisioned one tenant".into());
        };
        let tenant: DataTenantId = setup.tenant_id.parse()?;
        let catalog = operator_catalog(server).await?;
        let ingest_table = INGEST_TABLE.rsplit('.').next().unwrap_or(INGEST_TABLE);
        for (namespace, name) in [
            (BifrostNamespace::Datasets, ingest_table),
            (BifrostNamespace::Traces, "spans"),
            (BifrostNamespace::Logs, "records"),
        ] {
            enable_compaction(&catalog, tenant, namespace, name).await?;
        }
        self.tenant = Some(Arc::new(
            Box::pin(client_queue::Tenant::provision(
                setup,
                &self.logs.join("tenant"),
            ))
            .await?,
        ));
        Ok(())
    }
}

#[async_trait]
impl Workload for Competing {
    /// Starts the server, registers each ingest path in [`SPLIT`] order,
    /// and loads everything [`Competing::start`] names.
    ///
    /// # Errors
    ///
    /// Returns a workload error for a start, registration, or
    /// [`Competing::start`] failure. A started server is kept so
    /// [`Workload::finish`] stops it; nothing is retried.
    async fn prepare(&mut self) -> Result<(), ExperimentError> {
        std::fs::create_dir_all(&self.logs).map_err(ExperimentError::workload)?;
        let server = start(&PARQUET_ENV)
            .await
            .map_err(ExperimentError::workload)?;
        let senders = match client(&server) {
            Ok(client) => lanes(&client, &SPLIT.map(|(op, _)| op)).await,
            Err(error) => Err(error),
        };
        self.server = Some(server);
        self.senders = senders.map_err(ExperimentError::workload)?;
        Box::pin(self.start())
            .await
            .map_err(ExperimentError::workload)
    }

    /// The server's PID, once it runs.
    fn processes(&self) -> Vec<u32> {
        self.server
            .as_ref()
            .and_then(LocalServer::pid)
            .into_iter()
            .collect()
    }

    /// Runs the whole capacity → overload → recovery sequence; a warmup
    /// runs the same work.
    async fn measure(
        &mut self,
        step: &Step,
        _phase: Phase,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        Box::pin(self.competing(step, window)).await
    }

    /// Stops the server and keeps its log.
    async fn finish(&mut self) -> Result<(), ExperimentError> {
        self.senders.clear();
        self.bench = None;
        self.tenant = None;
        let Some(server) = self.server.take() else {
            return Ok(());
        };
        server
            .stop(&self.logs.join("server.log"))
            .await
            .map(|_| ())
            .map_err(ExperimentError::workload)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use tokio::time::Instant;

    use wyrd_telemetry::timing::{Distribution, IntervalScan, IntervalSeries};
    use wyrd_testing::capacity::NANOS_PER_MS;
    use wyrd_testing::capacity::Population;
    use wyrd_testing::experiment::{Reading, Repetition};

    use num_traits::ToPrimitive;

    use super::{
        Acked, Asked, Case, Offered, Phases, QUERY_CLIENTS, QUERY_RATE, RECOVERY_GRACE,
        WRONG_RESULT, expected, mixed_case, open_loop, owned, read_ingest, read_queries,
    };

    /// A successful `bifrost.query` series of `case` in `phase` with one
    /// observation of `nanos`.
    fn timed(case: &str, phase: &str, nanos: u64) -> IntervalSeries {
        IntervalSeries {
            boundary: "bifrost.query".to_owned(),
            outcome: "success".to_owned(),
            case: case.to_owned(),
            phase: phase.to_owned(),
            count: 1,
            elapsed: Distribution::of_durations(&[nanos]).expect("an in-range duration"),
            excluded: None,
            residual: None,
            missing_excluded: 0,
            out_of_range: 0,
            negative_residual: 0,
        }
    }

    /// Recovery starts [`RECOVERY_GRACE`] after overload in a full window and
    /// halfway through what remains of a short one, and the declared
    /// queue-full refusal window ends where recovery starts.
    ///
    /// # Panics
    ///
    /// Panics when the grace overruns a short window or shrinks a full one.
    #[test]
    fn recovery_grace_fits_the_window() {
        let start = Instant::now();
        let phases = |overloaded: u64, end: u64| Phases {
            overload: start + Duration::from_secs(5),
            overloaded: start + Duration::from_secs(overloaded),
            end: start + Duration::from_secs(end),
        };
        let full = phases(160, 360);
        assert_eq!(full.recovery(), full.overloaded + RECOVERY_GRACE);
        let smoke = phases(16, 36);
        assert_eq!(smoke.recovery(), start + Duration::from_secs(26));
        assert!(smoke.refusal_allowed(start + Duration::from_secs(25)));
        assert!(!smoke.refusal_allowed(start + Duration::from_secs(26)));
        assert!(!smoke.refusal_allowed(start + Duration::from_secs(4)));
        let late = phases(40, 36);
        assert_eq!(late.recovery(), late.overloaded);
    }

    /// A slow capacity query that completes during overload stays in the
    /// capacity cohort and its percentiles; a wrong answer is owed as a
    /// success; and a probe or another phase never covers a missing offered
    /// observation.
    ///
    /// # Panics
    ///
    /// Panics when a cohort is moved, covered, or miscounted.
    #[test]
    fn cohorts_follow_admission_not_completion() {
        let start = Instant::now();
        let phases = Phases {
            overload: start + Duration::from_secs(5),
            overloaded: start + Duration::from_secs(7),
            end: start + Duration::from_secs(12),
        };
        let asked = |phase, result| Asked {
            at: start + Duration::from_secs(6),
            phase,
            case: Case::Selective,
            result,
        };
        let asked = [
            asked("capacity", Ok(1)),
            asked("capacity", Ok(1)),
            asked("capacity", Err(WRONG_RESULT.to_owned())),
            asked("capacity", Err("WYRD_X".to_owned())),
        ];
        let owed = expected(&asked, &[]);
        let point = owed
            .iter()
            .find(|cohort| cohort.case == "point-lookup" && cohort.phase == "capacity")
            .expect("the capacity point cohort");
        assert_eq!(point.count, 3);
        let mut covered = Population {
            series: vec![
                timed("point-lookup", "capacity", 9_000_000),
                timed("point-lookup", "capacity", 2_000_000),
                timed("probe", "capacity", 50_000_000),
                timed("point-lookup", "overload", 40_000_000),
            ],
            ..Population::default()
        };
        covered.expect(&owed);
        assert_eq!(
            covered.invalid,
            [
                "2 `bifrost.query` timing observations of case point-lookup in phase capacity have outcome `success` for 3 completed requests",
                "1 `bifrost.query` timing observations of case point-lookup in phase overload have outcome `success` for 0 completed requests",
            ]
        );
        let mut repetition = Repetition::default();
        read_queries(&mut repetition, &asked, &covered, &phases);
        let p99 = repetition.reading("capacity.point_p99_ms");
        assert!(p99.value.is_some_and(|ms| ms >= 9.0), "{p99:?}");
        let selected = covered
            .only("point-lookup", "capacity")
            .elapsed("bifrost.query", "success")
            .expect("the selected capacity point cohort");
        let max = repetition.reading("max_ms");
        assert_eq!(max, Reading::quantile(&selected, 1.0, NANOS_PER_MS));
        assert_eq!((max.rank, max.samples), (Some(2), Some(2)), "{max:?}");
        assert!(max.value.is_some_and(|ms| ms >= 9.0), "{max:?}");
        assert_eq!(repetition.reading("capacity.queries").value, Some(2.0));
    }

    /// A slow answer never postpones a due query: the open loop sends every
    /// arrival on schedule and keeps every answer, misses arrivals once its
    /// bound is pending, the declared rate schedules exactly its rounded
    /// count with none due too close to the end to send, and it offers at
    /// least 4,000 mixed queries and 200 full scans per 360 s repetition.
    ///
    /// # Panics
    ///
    /// Panics when an answer delays an arrival or a count is wrong.
    #[tokio::test(start_paused = true)]
    async fn mixed_queries_are_offered_at_a_fixed_rate() {
        let slow = || tokio::time::sleep(Duration::from_secs(5));
        let second = Duration::from_secs(1);
        let interval = Duration::from_millis(100);
        let start = Instant::now();
        let offered = open_loop(start, start + second, interval, QUERY_CLIENTS, slow).await;
        assert_eq!(
            (offered.scheduled, offered.sent, offered.missed),
            (10, 10, 0)
        );
        assert_eq!(offered.acked.len(), 10);
        let start = Instant::now();
        let capped = open_loop(start, start + second, interval, 2, slow).await;
        assert_eq!((capped.scheduled, capped.sent, capped.missed), (10, 2, 8));
        let start = Instant::now();
        let declared = open_loop(
            start,
            start + Duration::from_secs(36),
            Duration::from_secs_f64(1.0 / QUERY_RATE),
            QUERY_CLIENTS,
            || async {},
        )
        .await;
        assert_eq!(
            (declared.scheduled, declared.sent, declared.missed),
            (432, 432, 0)
        );
        let offered = (QUERY_RATE * 360.0).to_u64().expect("a whole count");
        let scans = (0..offered)
            .filter(|sequence| mixed_case(*sequence) == Case::FullScan)
            .count();
        assert!(
            offered >= 4_000 && scans >= 200,
            "{offered} queries, {scans} scans"
        );
    }

    /// Every arrival a complete repetition schedules is sent or missed and
    /// every sent one answers before the drain instant; dropping the
    /// repetition mid-window, as a budget overrun or interrupt does, admits
    /// nothing more and aborts the scheduler's calls and every lane, so no
    /// work outlives the stop.
    ///
    /// # Panics
    ///
    /// Panics when an arrival is unaccounted for, is admitted after the
    /// stop, or a call or lane outlives it.
    #[tokio::test(start_paused = true)]
    async fn sustained_work_stops_admission_and_reconciles_drain() {
        let start = Instant::now();
        let slow = || tokio::time::sleep(Duration::from_millis(250));
        let complete = open_loop(
            start,
            start + Duration::from_secs(1),
            Duration::from_millis(100),
            2,
            slow,
        )
        .await;
        assert_eq!(complete.scheduled, complete.sent + complete.missed);
        assert!(complete.missed > 0, "the bound refuses some arrivals");
        assert_eq!(complete.acked.len().to_u64(), Some(complete.sent));
        assert!(
            complete
                .drained
                .is_some_and(|drained| drained >= start + Duration::from_millis(900))
        );

        let launched = Arc::new(AtomicU64::new(0));
        let live = Arc::new(());
        let start = Instant::now();
        let driving = open_loop(
            start,
            start + Duration::from_secs(1),
            Duration::from_millis(100),
            QUERY_CLIENTS,
            || {
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
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(
            launched.load(Ordering::Relaxed),
            3,
            "arrivals at 0, 100, and 200 ms only"
        );
        assert_eq!(Arc::strong_count(&live), 1, "no call outlives the stop");

        let emitted = Arc::new(AtomicU64::new(0));
        let lanes = owned((0..2).map(|_| {
            let emitted = Arc::clone(&emitted);
            async move {
                for _ in 0..100 {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    emitted.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
        let cut = tokio::time::timeout(Duration::from_millis(250), lanes).await;
        assert!(cut.is_err(), "the lanes outlast the cut");
        let at_stop = emitted.load(Ordering::Relaxed);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(
            emitted.load(Ordering::Relaxed),
            at_stop,
            "no lane emits after the stop"
        );
    }

    /// A due arrival whose wake comes after the end, behind a stall, is
    /// missed and never sent, the schedule's count is kept, and the
    /// admission sent before the stall still drains with its answer.
    ///
    /// # Panics
    ///
    /// Panics when an arrival is admitted after the end or one is lost.
    #[tokio::test(start_paused = true)]
    async fn arrivals_waking_after_the_end_are_missed() {
        let start = Instant::now();
        let mut stalled = false;
        let offered = open_loop(
            start,
            start + Duration::from_secs(1),
            Duration::from_millis(100),
            QUERY_CLIENTS,
            move || {
                let stall = !std::mem::replace(&mut stalled, true);
                async move {
                    if stall {
                        tokio::time::advance(Duration::from_secs(2)).await;
                    }
                    7
                }
            },
        )
        .await;
        assert_eq!(
            (offered.scheduled, offered.sent, offered.missed),
            (10, 1, 9)
        );
        assert_eq!(offered.acked, [7]);
    }

    /// Only queries and ingest rows and wire bytes answered before the
    /// window end, the measured CPU's endpoint, are credited as work; later
    /// answers are counted apart and a late failure stays an error. Scans
    /// per query average only the mixed classes' server scans, never a
    /// probe's.
    ///
    /// # Panics
    ///
    /// Panics when a late answer is credited or lost.
    #[test]
    fn only_answers_before_the_window_end_are_work() {
        let start = Instant::now();
        let end = start + Duration::from_secs(12);
        let phases = Phases {
            overload: start + Duration::from_secs(5),
            overloaded: start + Duration::from_secs(7),
            end,
        };
        let early = end - Duration::from_secs(1);
        let late = end + Duration::from_secs(1);
        let asked = |at, case, result| Asked {
            at,
            phase: "recovery",
            case,
            result,
        };
        let asked = [
            asked(early, Case::TableAggregate, Ok(1)),
            asked(end, Case::TableAggregate, Ok(1)),
            asked(late, Case::Selective, Ok(1)),
        ];
        let acked = |at, rows, result| Acked {
            at,
            phase: "recovery",
            rows,
            wire: rows * 1_048_576,
            result,
        };
        let offered = [Offered {
            acked: vec![
                acked(early, 10, Ok(1)),
                acked(late, 20, Ok(1)),
                acked(late, 40, Err("WYRD_X".to_owned())),
            ],
            ..Offered::default()
        }];
        let scan = |case: &str, rows: u64| {
            let facts = |value| Distribution::of_durations(&[value]).expect("a trackable fact");
            IntervalScan {
                outcome: "success".to_owned(),
                case: case.to_owned(),
                phase: "recovery".to_owned(),
                count: 1,
                rows: facts(rows),
                bytes: facts(9),
                missing_rows: 0,
                missing_bytes: 0,
                out_of_range: 0,
            }
        };
        let population = Population {
            series: vec![
                timed("table-aggregate", "recovery", 1),
                timed("probe", "recovery", 1),
            ],
            scans: vec![scan("table-aggregate", 7), scan("probe", 1000)],
            ..Population::default()
        };
        let mut repetition = Repetition::default();
        read_queries(&mut repetition, &asked, &population, &phases);
        read_ingest(&mut repetition, &offered, &[], &population, &phases);
        let value = |id: &str| repetition.reading(id).value;
        assert_eq!(value("queries"), Some(1.0));
        assert_eq!(value("table_aggregates"), Some(1.0));
        assert_eq!(value("queries_after_window"), Some(2.0));
        assert_eq!(value("ingest_rows"), Some(10.0));
        assert_eq!(value("ingest_rows_after_window"), Some(20.0));
        assert_eq!(value("otlp_wire_mib"), Some(10.0));
        assert_eq!(value("scanned_rows_per_query"), Some(7.0));
        assert_eq!(value("scanned_bytes_per_query"), Some(9.0));
        assert_eq!(value("errors"), Some(1.0));
    }

    /// Every 20 consecutive sequences send the declared mix: 60% point,
    /// 15% small and 10% 1M-row dashboard aggregates, 10% whole-table
    /// aggregates, and 5% full scans.
    ///
    /// # Panics
    ///
    /// Panics when the mix drifts.
    #[test]
    fn mixed_queries_follow_the_declared_split() {
        let count = |case| {
            (100..120)
                .filter(|sequence| mixed_case(*sequence) == case)
                .count()
        };
        assert_eq!(count(Case::Selective), 12);
        assert_eq!(count(Case::SmallAggregate), 3);
        assert_eq!(count(Case::MillionAggregate), 2);
        assert_eq!(count(Case::TableAggregate), 2);
        assert_eq!(count(Case::FullScan), 1);
    }
}
