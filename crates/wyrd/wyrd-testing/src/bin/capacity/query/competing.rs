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
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Duration;
use wyrd_testing::release_server::PEAK_SAMPLE_INTERVAL_MS;

use async_trait::async_trait;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use num_traits::ToPrimitive;
use secrecy::ExposeSecret as _;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::{BifrostCatalog, TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_client::transport::trace::with_benchmark_case;
use wyrd_spec::DataTenantId;
use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};
use wyrd_telemetry::timing::{Distribution, TimingSnapshot};
use wyrd_testing::capacity::{
    NANOS_PER_MS, Percentiles, Population, QUERY_BOUNDARY, Selection, UNLABELLED, Window,
};
use wyrd_testing::experiment::{
    Arrivals, ExperimentError, Phase, Reading, Repetition, Step, Workload,
};
use wyrd_testing::release_server::LocalServer;

use crate::client_queue::{self, Emitted, LANES};
use crate::query::isolated::{INSERTED, MIB, SUCCESS};
use crate::query::run::{Bench, Overload, QUEUE_FULL, Sender, WRONG_RESULT, ask, micros};
use crate::query::workload::{Case, Fixture, INGEST_TABLE, TABLE};
use crate::query::{Lane, PARQUET_ENV, Residence, SPLIT, WriteOp, client, lanes, smoke, start};

/// Mixed queries in flight at most; a due query finding this many
/// unanswered is missed rather than sent.
pub const QUERY_CLIENTS: usize = 32;

/// Offered mixed queries per second, independent of answers: 4,320 per
/// 360 s repetition, 216 of them full scans at the 5% mix, above the
/// 4,000-query and 200-scan floors.
pub const QUERY_RATE: f64 = 12.0;

/// How long after overload the SLOs must hold again; recovery is read only
/// after it.
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
    /// What it asked.
    case: Case,
    /// Its diagnostic client latency in microseconds, or its error code.
    result: Result<u64, String>,
}

/// One acknowledged or failed ingest request.
struct Acked {
    /// When it answered.
    at: Instant,
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

/// Sends one arrival through `launch` every `interval` from `start` until
/// `end`, never waiting on an answer before the next is due; a due arrival
/// finding `bound` pending is missed. Waits for every sent arrival and
/// keeps its answer.
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
    for arrival in 0_u32.. {
        let due = start + interval.mul_f64(f64::from(arrival));
        if due >= end {
            break;
        }
        tokio::time::sleep_until(due).await;
        while let Some(joined) = pending.try_join_next() {
            offered.acked.extend(joined.ok());
        }
        offered.scheduled += 1;
        if pending.len() >= bound {
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

/// The repetition's phase boundaries.
#[derive(Clone, Copy)]
struct Phases {
    /// Window start.
    start: Instant,
    /// Capacity end and overload start.
    overload: Instant,
    /// Overload end.
    overloaded: Instant,
    /// Window end.
    end: Instant,
}

impl Phases {
    /// Whether `at` falls in the capacity phase.
    fn capacity(&self, at: Instant) -> bool {
        at >= self.start && at < self.overload
    }

    /// Whether `at` falls in recovery after its grace.
    fn recovered(&self, at: Instant) -> bool {
        at >= self.overloaded + RECOVERY_GRACE && at < self.end
    }

    /// Whether a queue-full refusal at `at` is the declared one: from
    /// overload's start until the recovery grace ends.
    fn refusal_allowed(&self, at: Instant) -> bool {
        at >= self.overload && at < self.overloaded + RECOVERY_GRACE
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
}

impl Competing {
    /// An unprepared workload keeping its files under `logs`.
    pub fn new(logs: PathBuf) -> Self {
        Self {
            logs,
            server: None,
            bench: None,
            senders: Vec::new(),
            tenant: None,
            sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Runs one whole capacity → overload → recovery sequence of `step` over
    /// `window`, drains, collects its server timings, and reads every phase's
    /// evidence.
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
        let overload_at = start + window * 5 / 12;
        let outstanding = Arc::new(AtomicI64::new(0));
        let (visible, probes) = mpsc::unbounded_channel();
        let ingest = futures_util::future::join_all(SPLIT.iter().zip(&self.senders).map(
            |((_, share), (op, sender, cursor))| {
                let probe = (*op == WriteOp::Native).then(|| visible.clone());
                offer(
                    sender,
                    cursor,
                    rate * share,
                    start,
                    end,
                    &outstanding,
                    probe,
                )
            },
        ));
        drop(visible);
        let prober = probe(bench.queries(), probes);
        let queries = mixed(bench, start, end, &self.sequence);
        let lanes = futures_util::future::join_all(LANES.iter().map(|lane| {
            let tenant = Arc::clone(tenant);
            async move { lane.run(&tenant, end).await }
        }));
        let overload = async {
            tokio::time::sleep_until(overload_at).await;
            let capacity = server.timings().await;
            let before = outstanding.load(Ordering::Relaxed);
            let overload = bench.overload_queue(server).await;
            let overloaded = Instant::now();
            let backlog = outstanding.load(Ordering::Relaxed);
            let recovered = overloaded + RECOVERY_GRACE;
            let recovery = if recovered < end {
                tokio::time::sleep_until(recovered).await;
                Some(server.timings().await)
            } else {
                None
            };
            (before, overload, overloaded, backlog, capacity, recovery)
        };
        let closer = async {
            tokio::time::sleep_until(end).await;
            let backlog = outstanding.load(Ordering::Relaxed);
            let timings = server.timings().await;
            (opened.close(server).await, backlog, timings)
        };
        let (offered, visibility, queries, lanes, overload, (closed, backlog_end, ended)) =
            tokio::join!(ingest, prober, queries, lanes, overload, closer);
        let asked = &queries.acked;
        let (backlog_capacity, overload, overloaded, backlog_overloaded, capacity, recovery) =
            overload;
        let overload = overload.map_err(ExperimentError::workload)?;
        let (usage, after) = closed.map_err(ExperimentError::workload)?;
        let population =
            Population::collect(server, &timings, &expected(asked, &offered), false).await;
        let mut refused = Vec::new();
        let mut between = |low: &TimingSnapshot, high: &Result<TimingSnapshot, _>| match high {
            Ok(high) => high.since(low).map_or_else(
                |reason| {
                    refused.push(reason);
                    Population::default()
                },
                Population::from_interval,
            ),
            Err(error) => {
                refused.push(format!("the timing snapshot is unreadable: {error}"));
                Population::default()
            }
        };
        let windows = Windows {
            whole: between(&timings, &ended),
            capacity: between(&timings, &capacity),
            recovery: match (&recovery, &ended) {
                (Some(Ok(low)), high) => between(low, high),
                (Some(Err(error)), _) => {
                    refused.push(format!("the timing snapshot is unreadable: {error}"));
                    Population::default()
                }
                (None, _) => Population::default(),
            },
        };
        let phases = Phases {
            start,
            overload: overload_at,
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
            invalid: refused,
            ..Repetition::default()
        };
        let backlog = |rows: i64| Reading::exact(rows.to_f64().unwrap_or(f64::MAX) / rate);
        repetition.set("capacity.backlog_growth_s", backlog(backlog_capacity));
        repetition.set(
            "recovery.backlog_growth_s",
            backlog(backlog_end - backlog_overloaded),
        );
        population.validate(&mut repetition);
        read_queries(&mut repetition, asked, &windows, &phases);
        read_ingest(&mut repetition, &offered, &visibility, &windows, &phases);
        let inserted = after.sum(INSERTED, &[]) - before.sum(INSERTED, &[]);
        repetition.set("retained_mib", Reading::exact(inserted / MIB));
        read_arrivals(&mut repetition, &offered, &queries);
        read_overload(&mut repetition, &overload, &queries.acked, &phases);
        for (lane, emitted) in LANES.iter().zip(lanes) {
            let emitted = emitted.map_err(ExperimentError::workload)?;
            read_lane(&mut repetition, lane.name, &emitted, &phases);
        }
        Ok(repetition)
    }
}

/// One judged phase: its name, its server timing population, and whether
/// a client answer at an instant falls in it.
type PhaseWindow<'a> = (&'a str, &'a Population, &'a dyn Fn(Instant) -> bool);

/// Server timing populations between the repetition's phase snapshots.
#[derive(Default)]
struct Windows {
    /// Window start to window end.
    whole: Population,
    /// Window start to overload start.
    capacity: Population,
    /// Recovery-grace end to window end; empty when the grace outlasts the
    /// window.
    recovery: Population,
}

/// The least server observations the window's answers prove, from the
/// window start until every answer drained: each answered query at
/// `bifrost.query` under its class's case, and each acknowledged ingest
/// request at its path's boundary. Visibility probes and the client-queue
/// lanes reach the same boundaries, and native writes and OTLP exports carry
/// no case, so every count is a lower bound.
fn expected(asked: &[Asked], offered: &[Offered<Acked>]) -> Vec<(&'static str, &'static str, u64)> {
    let cases = [
        Case::Selective,
        Case::SmallAggregate,
        Case::MillionAggregate,
        Case::TableAggregate,
        Case::FullScan,
    ];
    let mut expected: Vec<_> = cases
        .iter()
        .map(|case| {
            let answered = asked
                .iter()
                .filter(|asked| asked.case == *case && asked.result.is_ok())
                .count();
            (QUERY_BOUNDARY, case.label(), answered as u64)
        })
        .collect();
    for ((op, _), offered) in SPLIT.iter().zip(offered) {
        let acked = offered.acked.iter().filter(|acked| acked.result.is_ok());
        expected.push((op.boundary(), UNLABELLED, acked.count() as u64));
    }
    expected
}

/// Sends `sender`'s path open loop at `rows_per_s` from `start` until `end`,
/// counting pending rows in `outstanding`, sending each native
/// acknowledgement's last id and time to `probe`, then waits for every
/// pending arrival.
async fn offer(
    sender: &Sender,
    cursor: &Arc<AtomicI64>,
    rows_per_s: f64,
    start: Instant,
    end: Instant,
    outstanding: &Arc<AtomicI64>,
    probe: Option<mpsc::UnboundedSender<(Instant, i64)>>,
) -> Offered<Acked> {
    let chunk = sender.request_rows();
    let interval = Duration::from_secs_f64(chunk.to_f64().unwrap_or(f64::MAX) / rows_per_s);
    open_loop(start, end, interval, MAX_PENDING, || {
        let first = cursor.fetch_add(chunk, Ordering::Relaxed);
        let (sender, outstanding, probe) = (sender.clone(), Arc::clone(outstanding), probe.clone());
        outstanding.fetch_add(chunk, Ordering::Relaxed);
        async move {
            let sent = std::time::Instant::now();
            let sent_bytes = sender.send(first, first + chunk).await;
            let wire = *sent_bytes.as_ref().unwrap_or(&0);
            let result = sent_bytes.map(|_| micros(sent));
            outstanding.fetch_sub(chunk, Ordering::Relaxed);
            let at = Instant::now();
            if let (Ok(_), Some(probe)) = (&result, probe) {
                let _ = probe.send((at, first + chunk - 1));
            }
            Acked {
                at,
                rows: chunk.cast_unsigned(),
                wire,
                result,
            }
        }
    })
    .await
}

/// Asks, for every acknowledged native write on `acks`, until its last row
/// is queryable, and returns each acknowledgement's time with its
/// visibility delay in microseconds, or `None` past [`VISIBILITY_WAIT`].
async fn probe(
    queries: Arc<wyrd_client::Bifrost>,
    mut acks: mpsc::UnboundedReceiver<(Instant, i64)>,
) -> Vec<(Instant, Option<u64>)> {
    let mut probes = JoinSet::new();
    while let Some((acked, id)) = acks.recv().await {
        let queries = Arc::clone(&queries);
        probes.spawn(async move {
            let sql = format!("SELECT COUNT(*) AS n FROM {INGEST_TABLE} WHERE event_id = {id}");
            let one = vec![vec![1]];
            while acked.elapsed() < VISIBILITY_WAIT {
                let asked = ask(&queries, sql.clone(), &one);
                if with_benchmark_case("probe", asked).await.is_ok() {
                    return (acked, Some(micros(acked.into_std())));
                }
                tokio::time::sleep(VISIBILITY_POLL).await;
            }
            (acked, None)
        });
    }
    let mut seen = Vec::new();
    while let Some(joined) = probes.join_next().await {
        seen.extend(joined.ok());
    }
    seen
}

/// Sends mixed queries open loop at [`QUERY_RATE`] from `start` until
/// `end`, independent of answers, with at most [`QUERY_CLIENTS`] in flight.
async fn mixed(
    bench: &Arc<Bench>,
    start: Instant,
    end: Instant,
    sequence: &Arc<AtomicU64>,
) -> Offered<Asked> {
    let queries = bench.queries();
    let interval = Duration::from_secs_f64(1.0 / QUERY_RATE);
    open_loop(start, end, interval, QUERY_CLIENTS, || {
        let next = sequence.fetch_add(1, Ordering::Relaxed);
        let case = mixed_case(next);
        let (sql, expected) = bench.question(case, next);
        let queries = Arc::clone(&queries);
        async move {
            let sent = std::time::Instant::now();
            let result = with_benchmark_case(case.label(), ask(&queries, sql, &expected))
                .await
                .map(|()| micros(sent));
            Asked {
                at: Instant::now(),
                case,
                result,
            }
        }
    })
    .await
}

/// Nearest-rank percentile `q`, milliseconds, of the successful `boundary`
/// observations of `case` in `population`; missing without one.
fn server_percentile(population: &Population, boundary: &str, case: &str, q: f64) -> Reading {
    population
        .only(case, UNLABELLED)
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

/// Sets every query reading: per-phase server latencies from each phase's
/// snapshots and client-answered counts, the capacity-phase point
/// percentiles, totals, errors, and wrong answers. Totals credit only
/// queries answered before the window end, the endpoint of the measured
/// CPU; later answers are counted separately. Scanned rows and bytes per
/// query average the executed-plan scans of the mixed queries the server
/// answered in the window.
fn read_queries(repetition: &mut Repetition, asked: &[Asked], windows: &Windows, phases: &Phases) {
    let phase_readings: [PhaseWindow<'_>; 2] = [
        ("capacity", &windows.capacity, &|at| phases.capacity(at)),
        ("recovery", &windows.recovery, &|at| phases.recovered(at)),
    ];
    for (phase, population, within) in phase_readings {
        let id = |name: &str| format!("{phase}.{name}");
        let read = |case: Case, q| server_percentile(population, QUERY_BOUNDARY, case.label(), q);
        repetition.set(&id("point_p99_ms"), read(Case::Selective, 0.99));
        repetition.set(&id("small_p95_ms"), read(Case::SmallAggregate, 0.95));
        repetition.set(&id("million_p95_ms"), read(Case::MillionAggregate, 0.95));
        repetition.set(&id("table_p95_ms"), read(Case::TableAggregate, 0.95));
        repetition.set(&id("scan_p95_ms"), read(Case::FullScan, 0.95));
        let answered = asked
            .iter()
            .filter(|asked| within(asked.at) && asked.result.is_ok())
            .count();
        repetition.set(
            &id("queries"),
            Reading::count(answered.to_u64().unwrap_or(0)),
        );
    }
    for (id, q) in [("p50_ms", 0.5), ("p95_ms", 0.95), ("p99_ms", 0.99)] {
        let point = Case::Selective.label();
        repetition.set(
            id,
            server_percentile(&windows.capacity, QUERY_BOUNDARY, point, q),
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
    for case in [
        Case::Selective,
        Case::SmallAggregate,
        Case::MillionAggregate,
        Case::TableAggregate,
        Case::FullScan,
    ] {
        mixed
            .scans
            .extend(windows.whole.only(case.label(), UNLABELLED).scans);
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
/// every successful write-boundary observation between the phase's
/// snapshots (the client-queue lanes' writes included, since native writes
/// carry no case), the diagnostic visibility, and rows; totals; the drain; and adds
/// ingest failures and undeclared query refusals to `errors`. The row total
/// credits only acknowledgements before the window end, the endpoint of the
/// measured CPU; drain acknowledgements are counted separately and every
/// failure stays an error.
fn read_ingest(
    repetition: &mut Repetition,
    offered: &[Offered<Acked>],
    visibility: &[(Instant, Option<u64>)],
    windows: &Windows,
    phases: &Phases,
) {
    let writes = SPLIT.map(|(op, _)| op.boundary());
    let acked: Vec<&Acked> = offered.iter().flat_map(|offered| &offered.acked).collect();
    let phase_readings: [PhaseWindow<'_>; 2] = [
        ("capacity", &windows.capacity, &|at| phases.capacity(at)),
        ("recovery", &windows.recovery, &|at| phases.recovered(at)),
    ];
    for (phase, population, within) in phase_readings {
        let id = |name: &str| format!("{phase}.{name}");
        let unlabelled = population.only(UNLABELLED, UNLABELLED);
        let mut acks: Option<Distribution> = None;
        for boundary in writes {
            let Some(part) = unlabelled.elapsed(boundary, SUCCESS) else {
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
            .filter(|acked| within(acked.at) && acked.result.is_ok())
            .map(|acked| acked.rows)
            .sum();
        repetition.set(&id("ingest_rows"), Reading::count(rows));
        let seen: Vec<Option<u64>> = visibility
            .iter()
            .filter(|(at, _)| within(*at))
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
            "the driver missed {ingest_missed} of {} scheduled ingest arrivals with {MAX_PENDING} pending",
            arrivals.scheduled - queries.scheduled
        ));
    }
    if queries.missed > 0 {
        repetition.invalid.push(format!(
            "the driver missed {} of {} scheduled mixed queries with {QUERY_CLIENTS} in flight",
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
    /// Loads and publishes the heavy fixture into the smoked server,
    /// computes the exact answers, enables compaction on every ingest table
    /// the smoke created, and provisions the client-queue tenant.
    ///
    /// # Errors
    ///
    /// Returns a missing smoke, or a client, load, publication, catalog, or
    /// provisioning failure.
    async fn start(&mut self) -> crate::query::Result<()> {
        let server = self.server.as_ref().ok_or("the workload was not smoked")?;
        let client = client(server)?;
        let fixture = Fixture::heavy();
        let loaded = Sender::native(&client, TABLE, fixture)
            .await?
            .closed_loop(
                &Arc::new(AtomicI64::new(0)),
                fixture.rows,
                LOADERS,
                &CancellationToken::new(),
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
        self.bench = Some(Arc::new(Bench::new(
            &client,
            fixture,
            &[
                Case::Selective,
                Case::SmallAggregate,
                Case::MillionAggregate,
                Case::TableAggregate,
                Case::FullScan,
            ],
        )));
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
    /// Starts the server, registers each ingest path in
    /// [`SPLIT`] order, and smokes every write path and the query boundary
    /// ([`crate::query::smoke`]) before any fixture loads; the smoke's
    /// writes also create every ingest table compaction is enabled on.
    async fn smoke(&mut self) -> Result<String, ExperimentError> {
        std::fs::create_dir_all(&self.logs).map_err(ExperimentError::workload)?;
        let server = start(&PARQUET_ENV)
            .await
            .map_err(ExperimentError::workload)?;
        let client = client(&server).map_err(ExperimentError::workload)?;
        self.senders = lanes(&client, &SPLIT.map(|(op, _)| op))
            .await
            .map_err(ExperimentError::workload)?;
        let proved = smoke(&server, &client, &self.senders).await;
        self.server = Some(server);
        proved.map_err(ExperimentError::Workload)
    }

    /// Loads everything [`Competing::start`] names.
    async fn prepare(&mut self) -> Result<(), ExperimentError> {
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
            .map(|_| ())
            .map_err(ExperimentError::workload)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use wyrd_telemetry::timing::{Distribution, IntervalScan};
    use wyrd_testing::capacity::Population;
    use wyrd_testing::experiment::Repetition;

    use num_traits::ToPrimitive;

    use super::{
        Acked, Asked, Case, Offered, Phases, QUERY_CLIENTS, QUERY_RATE, Windows, mixed_case,
        open_loop, read_ingest, read_queries,
    };

    /// A slow answer never postpones a due query: the open loop sends every
    /// arrival on schedule and keeps every answer, misses arrivals once its
    /// bound is pending, and the declared rate offers at least 4,000 mixed
    /// queries and 200 full scans per 360 s repetition.
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
        let offered = (QUERY_RATE * 360.0).to_u64().expect("a whole count");
        let scans = (0..offered)
            .filter(|sequence| mixed_case(*sequence) == Case::FullScan)
            .count();
        assert!(
            offered >= 4_000 && scans >= 200,
            "{offered} queries, {scans} scans"
        );
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
            start,
            overload: start + Duration::from_secs(5),
            overloaded: start + Duration::from_secs(7),
            end,
        };
        let early = end - Duration::from_secs(1);
        let late = end + Duration::from_secs(1);
        let asked = |at, case, result| Asked { at, case, result };
        let asked = [
            asked(early, Case::TableAggregate, Ok(1)),
            asked(end, Case::TableAggregate, Ok(1)),
            asked(late, Case::Selective, Ok(1)),
        ];
        let acked = |at, rows, result| Acked {
            at,
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
                phase: "none".to_owned(),
                count: 1,
                rows: facts(rows),
                bytes: facts(9),
                missing_rows: 0,
                missing_bytes: 0,
                out_of_range: 0,
            }
        };
        let windows = Windows {
            whole: Population {
                scans: vec![scan("table-aggregate", 7), scan("probe", 1000)],
                ..Population::default()
            },
            ..Windows::default()
        };
        let mut repetition = Repetition::default();
        read_queries(&mut repetition, &asked, &windows, &phases);
        read_ingest(&mut repetition, &offered, &[], &windows, &phases);
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
