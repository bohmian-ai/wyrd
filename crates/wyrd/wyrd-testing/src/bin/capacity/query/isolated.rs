//! The Bifrost L1 workload of one [`Experiment`]: one server and one closed
//! loop per measured step.
//!
//! `writes` loads no fixture: every write window takes its rows from a
//! cursor that only grows, so each repetition writes fresh identities
//! instead of retrying earlier ones. `hot` loads the events and span
//! fixtures and checks that every row stays in a memtable; `parquet` also
//! loads each write path's storage sample, settles everything once, and
//! reads the settled-state facts before any window.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use wyrd_testing::release_server::PEAK_SAMPLE_INTERVAL_MS;

use async_trait::async_trait;
use num_traits::ToPrimitive;
use wyrd_testing::capacity::{Expected, Population, Selection, Window};
use wyrd_testing::experiment::{ExperimentError, Phase, Reading, Repetition, Step, Workload};
use wyrd_testing::release_server::LocalServer;

use crate::query::run::{Admission, Bench, Outcome, Sender, WRONG_RESULT, ask};
use crate::query::telemetry::{self, Telemetry};
use crate::query::workload::{Case, Fixture, Rows, TABLE};
use crate::query::{
    Experiment, Lane, PAIR, Read, Residence, SETTLED, SPAN_FIXTURE_ROWS, WriteOp, client, lanes,
    parquet_bytes, read_op, smoke, start, table_files,
};

/// Concurrent writers loading the read fixture.
const LOADERS: usize = 16;

/// How long the read fixture may take to publish after its load.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Scribe's counter of Arrow bytes inserted into memtables.
pub const INSERTED: &str = "bifrost_scribe_memtable_bytes_inserted_total";

/// The outcome every judged Bifrost server timing has.
pub const SUCCESS: &str = "success";

/// The L1 server, its fixture, and the shared cursors of every closed loop.
pub struct Isolated {
    /// Where the server log is kept.
    logs: PathBuf,
    /// The experiment this workload runs.
    experiment: Experiment,
    /// The running server, once prepared.
    server: Option<LocalServer>,
    /// The query handle and exact answers, once prepared.
    bench: Option<Arc<Bench>>,
    /// Each write path's sender, in [`WriteOp::ALL`] order, once smoked.
    senders: Vec<Lane>,
    /// The statement sequence every read window continues.
    sequence: Arc<AtomicU64>,
    /// Spans the span table holds once prepared: indices `0..spans` of the
    /// spans lane's generator, the smoke's included.
    spans: i64,
    /// Each span filter's statement and exact answer, once prepared.
    filters: Vec<(Read, (String, Rows))>,
    /// The `parquet` settled-state facts, by step and metric, read once in
    /// [`Workload::prepare`].
    settled: Vec<(&'static str, &'static str, Reading)>,
}

impl Isolated {
    /// An unprepared `experiment` keeping its server log under `logs`.
    pub fn new(logs: PathBuf, experiment: Experiment) -> Self {
        Self {
            logs,
            experiment,
            server: None,
            bench: None,
            senders: Vec::new(),
            sequence: Arc::new(AtomicU64::new(0)),
            spans: 0,
            filters: Vec::new(),
            settled: Vec::new(),
        }
    }

    /// Runs one closed-loop window of `step` for `window` and reads its
    /// server timings, client diagnostics, and server and driver usage.
    ///
    /// Every request carries its case and `phase`; the window admits until
    /// its stop, and its work, seconds, and CPU stop there while requests
    /// then in flight drain, counted late. Every request the server answered
    /// must have exactly one successful server timing in its cohort; the
    /// judged percentiles are that whole cohort's, late ones included. Each write
    /// path after the first waits, before its first warmup, for every
    /// earlier row to publish, so no path's server CPU pays for another
    /// path's backlog; a path's 64-client step still carries its own
    /// 1-client backlog.
    ///
    /// # Errors
    ///
    /// Returns a workload error for an unprepared workload, an undeclared
    /// step, a scrape failure, earlier rows not published within the settle
    /// timeout, or, in `hot`, a row that left its memtable.
    async fn closed(
        &self,
        step: &Step,
        phase: Phase,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        let Some(server) = &self.server else {
            return Err(ExperimentError::workload("the workload was not prepared"));
        };
        let op = step.text("op").unwrap_or_default();
        let clients = step
            .number("clients")
            .and_then(|clients| usize::try_from(clients).ok())
            .ok_or_else(|| {
                ExperimentError::workload(format!("step `{}` declares no clients", step.id))
            })?;
        if phase == Phase::Warmup
            && step.number("clients") == Some(PAIR[0])
            && WriteOp::parse(op).is_some_and(|write| write != WriteOp::ALL[0])
        {
            Residence::settle(server, SETTLE_TIMEOUT)
                .await
                .map_err(ExperimentError::workload)?;
        }
        let read = read_op(op);
        let write = WriteOp::parse(op)
            .and_then(|op| self.senders.iter().find(|(declared, ..)| *declared == op));
        let case = match (read, write) {
            (Some(read), _) => read.case(),
            (None, Some((write, ..))) => write.name(),
            (None, None) => {
                return Err(ExperimentError::workload(format!(
                    "step `{}` names no known op `{op}`",
                    step.id
                )));
            }
        };
        let timings = server.timings().await.map_err(ExperimentError::workload)?;
        let (opened, before) = Window::open(server, Selection::RESOURCES)
            .await
            .map_err(ExperimentError::workload)?;
        let stop = Instant::now() + window;
        let admission = Admission::measured(stop, (case, phase.label()));
        let work = async {
            match (read, write) {
                (Some(read), _) => {
                    let bench = self.bench.as_ref().ok_or_else(|| {
                        ExperimentError::workload("the read fixture was not loaded")
                    })?;
                    let outcome = match read {
                        Read::Events(case) => {
                            bench.drive(case, clients, admission, &self.sequence).await
                        }
                        Read::Trace => {
                            let spans = self.spans;
                            bench
                                .drive_with(clients, admission, &self.sequence, move |sequence| {
                                    Telemetry::trace_lookup(spans, sequence)
                                })
                                .await
                        }
                        Read::Shredded | Read::LongTail => {
                            let (_, question) = self
                                .filters
                                .iter()
                                .find(|(filter, _)| *filter == read)
                                .cloned()
                                .ok_or_else(|| {
                                    ExperimentError::workload("the span fixture was not loaded")
                                })?;
                            bench
                                .drive_with(clients, admission, &self.sequence, move |_| {
                                    question.clone()
                                })
                                .await
                        }
                    };
                    Ok((outcome, None))
                }
                (None, Some((write, sender, cursor))) => Ok((
                    sender
                        .closed_loop(cursor, i64::MAX, clients, admission)
                        .await,
                    Some(*write),
                )),
                (None, None) => Err(ExperimentError::workload("unreachable op")),
            }
        };
        let (driven, usage, stopped) = opened
            .measure(server, stop, work)
            .await
            .map_err(ExperimentError::workload)?;
        let (outcome, write) = driven?;
        if self.experiment == Experiment::Hot {
            hot(server).await?;
        }
        let mut repetition = Repetition {
            seconds: usage.seconds,
            processes: vec!["wyrd-server".to_owned()],
            server_cpu_seconds: usage.server_cpu_seconds,
            peak_memory_bytes: usage.peak_memory_bytes,
            memory_interval_ms: PEAK_SAMPLE_INTERVAL_MS,
            driver_cpu_seconds: usage.driver_cpu_seconds,
            ..Repetition::default()
        };
        let boundary = write.map_or("bifrost.query", WriteOp::boundary);
        let cohort = Expected {
            boundary,
            outcome: SUCCESS,
            case,
            phase: phase.label(),
            count: outcome.answered(),
        };
        let population = Population::collect(server, &timings, &[cohort]).await;
        population.validate(&mut repetition, "measured");
        population.percentiles(
            &mut repetition,
            boundary,
            SUCCESS,
            false,
            &[("p50_ms", 0.5), ("p95_ms", 0.95), ("p99_ms", 0.99)],
        );
        repetition.set(
            "server_failures",
            Reading::count(population.others(boundary, SUCCESS)),
        );
        read_outcome(&mut repetition, &outcome, read);
        repetition.set("drain_seconds", Reading::exact(usage.drain_seconds));
        if read.is_some() {
            let [rows, bytes] = population.scanned_per_query(SUCCESS);
            repetition.set("scanned_rows_per_query", rows);
            repetition.set("scanned_bytes_per_query", bytes);
        }
        for (_, metric, reading) in self.settled.iter().filter(|(id, ..)| *id == step.id) {
            repetition.set(metric, reading.clone());
        }
        if let Some(write) = write {
            let inserted = stopped.sum(INSERTED, &[]) - before.sum(INSERTED, &[]);
            repetition.set("rows", Reading::count(outcome.rows - outcome.late_rows));
            repetition.set("late_rows", Reading::count(outcome.late_rows));
            repetition.set("retained_mib", Reading::exact(inserted / MIB));
            repetition.set(
                "wire_bytes",
                if write == WriteOp::Native {
                    Reading::missing("native writes send Arrow IPC, not OTLP")
                } else {
                    Reading::count(outcome.wire_bytes - outcome.late_wire_bytes)
                },
            );
            repetition.set(
                "visibility_p99_ms",
                Reading::missing(
                    "L1 measures closed-loop acknowledgement; visibility is judged at L2",
                ),
            );
            repetition.set(
                "storage_ratio",
                Reading::missing(
                    "measured by the `parquet` experiment after its settlement; `writes` never waits for publication",
                ),
            );
        }
        Ok(repetition)
    }
}

/// Rows each write path's storage sample writes in [`Workload::prepare`].
const STORAGE_ROWS: i64 = 200_000;

/// Bytes per MiB.
pub const MIB: f64 = 1_048_576.0;

/// Fails unless every row of the `hot` `server` is still in a memtable.
///
/// # Errors
///
/// Returns a workload error naming where rows went, or the scrape failure.
async fn hot(server: &LocalServer) -> Result<(), ExperimentError> {
    let residence = Residence::read(server)
        .await
        .map_err(ExperimentError::workload)?;
    residence
        .left_memtable()
        .map_or(Ok(()), |left| Err(ExperimentError::workload(left)))
}

/// Server counters the span-time pruning check compares: files and row
/// groups scanned.
const SCANNED: [&str; 2] = [
    "oracle_query_files_scanned_total",
    "oracle_query_row_groups_scanned_total",
];

/// Asks `question` once against the settled `server` and returns the
/// [`SCANNED`] counts it added.
///
/// # Errors
///
/// Returns the scrape failure or the query's error code, including a wrong
/// answer.
async fn scanned(
    server: &LocalServer,
    bench: &Bench,
    (sql, expected): (String, Rows),
) -> Result<[f64; 2], String> {
    let before = server.metrics().await.map_err(|error| error.to_string())?;
    ask(&bench.queries(), sql, &expected).await?;
    let after = server.metrics().await.map_err(|error| error.to_string())?;
    Ok(SCANNED.map(|family| after.sum(family, &[]) - before.sum(family, &[])))
}

/// Sets the client readings every closed-loop step shares: requests
/// completed by the stop and late, errors, diagnostic client round-trip
/// percentiles, and, for a `read`, measured and wrong queries.
fn read_outcome(repetition: &mut Repetition, outcome: &Outcome, read: Option<Read>) {
    let wrong = outcome.errors.get(WRONG_RESULT).copied().unwrap_or(0);
    let requests = outcome.measured();
    repetition.set("requests", Reading::count(requests));
    repetition.set("late_requests", Reading::count(outcome.late));
    repetition.set("errors", Reading::count(outcome.error_count()));
    for (id, q) in [
        ("client_p50_ms", 0.5),
        ("client_p95_ms", 0.95),
        ("client_p99_ms", 0.99),
    ] {
        repetition.set(id, Reading::percentile(&outcome.latencies_us, q, 1e3));
    }
    if let Some(read) = read {
        repetition.set("queries", Reading::count(requests));
        repetition.set("wrong_results", Reading::count(wrong));
        if read == Read::Events(Case::FullScan) {
            repetition.set(
                "p99_ms",
                Reading::missing("p95 only: a full-scan window holds too few samples for a p99"),
            );
        }
    }
}

/// The `parquet` settled-state facts of the settled `server`, in
/// [`SETTLED`] order: the span shredding and span-time pruning judgments
/// over its `spans` spans, then each write path's storage ratio from its
/// `samples` (path, table bytes before, inserted Arrow bytes, outcome).
async fn settled(
    server: &LocalServer,
    bench: &Bench,
    spans: i64,
    samples: Vec<(WriteOp, u64, f64, Outcome)>,
) -> Vec<(&'static str, &'static str, Reading)> {
    let root = server.storage_dir();
    let judged = |outcome: Result<String, String>| match outcome {
        Ok(summary) => {
            tracing::info!(%summary, "settled check passed");
            (Reading::count(0), Reading::count(1))
        }
        Err(summary) => {
            tracing::warn!(%summary, "settled check failed");
            (Reading::count(1), Reading::count(0))
        }
    };
    let (shredding, _) = judged(telemetry::judge_shredding(&table_files(
        &root,
        telemetry::TABLE,
    )));
    let all = scanned(server, bench, Telemetry::all_spans(spans)).await;
    let window = scanned(
        server,
        bench,
        Telemetry {
            spans: crate::query::WRITE_FIXTURE_ROWS,
        }
        .span_window(spans),
    )
    .await;
    let pruned = match (all, window) {
        (Ok(all), Ok(window)) => judged(telemetry::judge_pruning(all, window)).1,
        (Err(code), _) | (_, Err(code)) => {
            Reading::missing(format!("a pruning query failed with `{code}`"))
        }
    };
    let mut facts = vec![shredding, pruned];
    for op in WriteOp::ALL {
        let ratio = samples
            .iter()
            .find(|(sampled, ..)| *sampled == op)
            .map_or_else(
                || Reading::missing("the path's storage sample was not taken"),
                |(_, stored, inserted, sample)| {
                    let stored = parquet_bytes(&root, op.table()).saturating_sub(*stored);
                    if sample.error_count() > 0 {
                        Reading::missing(format!(
                            "the storage sample failed with {:?}",
                            sample.errors
                        ))
                    } else if *inserted > 0.0 && stored > 0 {
                        Reading::exact(stored.to_f64().unwrap_or(f64::MAX) / inserted)
                    } else {
                        Reading::missing("the storage sample inserted or published no bytes")
                    }
                },
            );
        facts.push(ratio);
    }
    SETTLED
        .iter()
        .zip(facts)
        .map(|((step, metric, ..), reading)| (*step, *metric, reading))
        .collect()
}

#[async_trait]
impl Workload for Isolated {
    /// Starts the server, registers each write path, and
    /// smokes every write path and the query boundary
    /// ([`crate::query::smoke`]) before any fixture loads.
    ///
    /// # Errors
    ///
    /// Returns a workload error for a server or registration
    /// failure, or the smoke's missing evidence. A started server is kept
    /// even when the smoke fails, so [`Workload::finish`] stops it; nothing
    /// is retried.
    async fn smoke(&mut self) -> Result<String, ExperimentError> {
        let server = start(self.experiment.env())
            .await
            .map_err(ExperimentError::workload)?;
        let client = client(&server).map_err(ExperimentError::workload)?;
        self.senders = lanes(&client, &WriteOp::ALL)
            .await
            .map_err(ExperimentError::workload)?;
        let proved = smoke(&server, &client, &self.senders).await;
        self.server = Some(server);
        proved.map_err(ExperimentError::Workload)
    }

    /// Loads the experiment's fixtures into the smoked server and computes
    /// the exact answers; `writes` loads nothing.
    ///
    /// Each read experiment loads [`SPAN_FIXTURE_ROWS`] spans after the
    /// smoke's, then the standard events fixture. `hot` then requires every
    /// row still in a memtable. `parquet` first loads [`STORAGE_ROWS`] fresh
    /// native and logs rows, each alone, settles everything once, and reads
    /// the settled-state facts ([`SETTLED`]): each path's storage ratio, its
    /// table's growth in published Parquet bytes over the `INSERTED` delta
    /// of its own load (the span fixture is the spans sample), the span
    /// shredding judgment, and the span-time pruning judgment.
    ///
    /// # Errors
    ///
    /// Returns a workload error for an unsmoked workload, a scrape failure,
    /// a fixture that loaded short or with errors, a `hot` row that left its
    /// memtable, or rows not published within the settle timeout. Rows
    /// already written stay in the server; nothing is retried.
    async fn prepare(&mut self) -> Result<(), ExperimentError> {
        if self.experiment == Experiment::Writes {
            return Ok(());
        }
        let server = self
            .server
            .as_ref()
            .ok_or_else(|| ExperimentError::workload("the workload was not smoked"))?;
        let client = client(server).map_err(ExperimentError::workload)?;
        let root = server.storage_dir();
        let parquet = self.experiment == Experiment::Parquet;
        let mut samples = Vec::new();
        for (op, sender, cursor) in &self.senders {
            let rows = match op {
                WriteOp::Spans => SPAN_FIXTURE_ROWS,
                WriteOp::Native | WriteOp::Logs if parquet => STORAGE_ROWS,
                WriteOp::Native | WriteOp::Logs => continue,
            };
            let stored = parquet_bytes(&root, op.table());
            let before = server.metrics().await.map_err(ExperimentError::workload)?;
            let end = cursor.load(Ordering::Relaxed) + rows;
            let sample = sender
                .closed_loop(cursor, end, LOADERS, Admission::default())
                .await;
            cursor.store(end, Ordering::Relaxed);
            let after = server.metrics().await.map_err(ExperimentError::workload)?;
            if *op == WriteOp::Spans {
                if sample.error_count() > 0 || sample.rows != rows.cast_unsigned() {
                    return Err(ExperimentError::workload(format!(
                        "the span fixture loaded {} of {rows} spans with errors {:?}",
                        sample.rows, sample.errors
                    )));
                }
                self.spans = end;
            }
            let inserted = after.sum(INSERTED, &[]) - before.sum(INSERTED, &[]);
            samples.push((*op, stored, inserted, sample));
        }
        let fixture = Fixture::standard();
        let loader = Sender::native(&client, TABLE, fixture)
            .await
            .map_err(ExperimentError::workload)?;
        let loaded = loader
            .closed_loop(
                &Arc::new(AtomicI64::new(0)),
                fixture.rows,
                LOADERS,
                Admission::default(),
            )
            .await;
        if loaded.error_count() > 0 || loaded.rows != fixture.rows.cast_unsigned() {
            return Err(ExperimentError::workload(format!(
                "the read fixture loaded {} of {} rows with errors {:?}",
                loaded.rows, fixture.rows, loaded.errors
            )));
        }
        let cases: Vec<Case> = self
            .experiment
            .reads()
            .iter()
            .filter_map(|(_, read, _)| match read {
                Read::Events(case) => Some(*case),
                _ => None,
            })
            .collect();
        let bench = Arc::new(Bench::new(&client, fixture, &cases));
        self.filters = vec![
            (Read::Shredded, Telemetry::shredded_filter(self.spans)),
            (Read::LongTail, Telemetry::long_tail_filter(self.spans)),
        ];
        if parquet {
            Residence::settle(server, SETTLE_TIMEOUT)
                .await
                .map_err(ExperimentError::workload)?;
            self.settled = settled(server, &bench, self.spans, samples).await;
        } else {
            hot(server).await?;
        }
        self.bench = Some(bench);
        Ok(())
    }

    /// The server's PID, once it runs.
    fn processes(&self) -> Vec<u32> {
        self.server
            .as_ref()
            .and_then(LocalServer::pid)
            .into_iter()
            .collect()
    }

    /// Runs one closed-loop window of `step`; a warmup runs the same work.
    ///
    /// # Errors
    ///
    /// Returns a workload error for an unprepared workload, an undeclared
    /// step, or a scrape failure. A cancelled window aborts its client tasks
    /// and their requests.
    async fn measure(
        &mut self,
        step: &Step,
        phase: Phase,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        self.closed(step, phase, window).await
    }

    /// Drops the senders and queries, then stops the server and keeps its
    /// log. The stop runs synchronously inside this future: `SIGTERM`, a
    /// bounded wait for a clean exit, and the log copy.
    ///
    /// # Errors
    ///
    /// Returns a workload error when the server exits unsuccessfully or
    /// outlives its stop grace (it is then killed), or its log cannot be
    /// copied. An unstarted workload finishes without error.
    async fn finish(&mut self) -> Result<(), ExperimentError> {
        self.senders.clear();
        self.bench = None;
        let Some(server) = self.server.take() else {
            return Ok(());
        };
        server
            .stop(&self.logs.join("server.log"))
            .map(|_| ())
            .map_err(ExperimentError::workload)
    }
}
