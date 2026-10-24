//! The L1 `isolated` workload: one server, the standard fixture published as
//! Parquet, and one closed loop per measured step.
//!
//! Read steps run before any write step, so their windows never share the
//! server with a publication of later writes. Every write window takes its
//! rows from a cursor that only grows, so each repetition writes fresh
//! identities instead of retrying earlier ones.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64};
use std::time::{Duration, Instant, SystemTime};
use wyrd_testing::release_server::PEAK_SAMPLE_INTERVAL_MS;

use async_trait::async_trait;
use num_traits::ToPrimitive;
use tokio_util::sync::CancellationToken;
use wyrd_testing::capacity::{Collector, Selection, Window};
use wyrd_testing::experiment::{ExperimentError, Phase, Reading, Repetition, Step, Workload};
use wyrd_testing::release_server::LocalServer;

use crate::query::logs::Logs;
use crate::query::run::{Bench, Otlp, Outcome, Sender, WRONG_RESULT};
use crate::query::telemetry::Telemetry;
use crate::query::workload::{Case, Fixture, INGEST_TABLE, TABLE};
use crate::query::{Residence, WRITE_FIXTURE_ROWS, WriteOp, client, read_case, start};

/// Concurrent writers loading the read fixture.
const LOADERS: usize = 16;

/// How long the read fixture may take to publish after its load.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Scribe's counter of Arrow bytes inserted into memtables.
const INSERTED: &str = "bifrost_scribe_memtable_bytes_inserted_total";

/// Oracle's counter of rows its executed plans scanned.
const ROWS_SCANNED: &str = "oracle_query_rows_scanned_total";

/// Oracle's counter of bytes its executed plans scanned.
const BYTES_SCANNED: &str = "oracle_query_bytes_scanned_total";

/// The outcome every judged Bifrost server timing has.
pub const SUCCESS: &str = "success";

/// The L1 server, its fixture, and the shared cursors of every closed loop.
pub struct Isolated {
    /// Where the server log and raw timings are kept.
    logs: PathBuf,
    /// The collector the server exports its timings to, once prepared.
    collector: Option<Collector>,
    /// The running server, once prepared.
    server: Option<LocalServer>,
    /// The query handle and exact answers, once prepared.
    bench: Option<Arc<Bench>>,
    /// Each write path's sender, in [`WriteOp::ALL`] order, once prepared.
    senders: Vec<(WriteOp, Sender, Arc<AtomicI64>)>,
    /// The statement sequence every read window continues.
    sequence: Arc<AtomicU64>,
}

impl Isolated {
    /// An unprepared workload keeping its server log under `logs`.
    pub fn new(logs: PathBuf) -> Self {
        Self {
            logs,
            collector: None,
            server: None,
            bench: None,
            senders: Vec::new(),
            sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Runs one closed-loop window of `step` for `window` and reads its
    /// server timings, client diagnostics, and server and driver usage.
    ///
    /// Every request the window sent must have exactly one server timing at
    /// its boundary; the judged percentiles are the successful ones'.
    ///
    /// # Errors
    ///
    /// Returns a workload error for an unprepared workload, an undeclared
    /// step, or a scrape failure.
    async fn closed(
        &self,
        step: &Step,
        phase: Phase,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        let (Some(server), Some(bench), Some(collector)) =
            (&self.server, &self.bench, &self.collector)
        else {
            return Err(ExperimentError::workload("the workload was not prepared"));
        };
        let op = step.text("op").unwrap_or_default();
        let clients = step
            .number("clients")
            .and_then(|clients| usize::try_from(clients).ok())
            .ok_or_else(|| {
                ExperimentError::workload(format!("step `{}` declares no clients", step.id))
            })?;
        let from = SystemTime::now();
        let (opened, before) = Window::open(server, Selection::RESOURCES)
            .await
            .map_err(ExperimentError::workload)?;
        let (outcome, write) = if let Some(case) = read_case(op) {
            let deadline = Instant::now() + window;
            (
                bench.drive(case, clients, deadline, &self.sequence).await,
                None,
            )
        } else if let Some((write, sender, cursor)) = WriteOp::parse(op)
            .and_then(|op| self.senders.iter().find(|(declared, ..)| *declared == op))
        {
            let stop = CancellationToken::new();
            let timer = stop.clone();
            tokio::spawn(async move {
                tokio::time::sleep(window).await;
                timer.cancel();
            });
            (
                sender.closed_loop(cursor, i64::MAX, clients, &stop).await,
                Some(*write),
            )
        } else {
            return Err(ExperimentError::workload(format!(
                "step `{}` names no known op `{op}`",
                step.id
            )));
        };
        let (usage, after) = opened
            .close(server)
            .await
            .map_err(ExperimentError::workload)?;
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
        let sent = outcome.latencies_us.len() as u64 + outcome.error_count();
        let population = collector
            .window(&format!("{}/{phase:?}", step.id), from, &[(boundary, sent)])
            .await;
        population.validate(&mut repetition);
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
        read_outcome(&mut repetition, &outcome, read_case(op));
        if read_case(op).is_some() {
            let queries = outcome.latencies_us.len() as f64;
            let per_query = |family: &str| {
                let scanned = after.sum(family, &[]) - before.sum(family, &[]);
                if queries > 0.0 {
                    Reading::exact(scanned / queries)
                } else {
                    Reading::missing("no query was answered")
                }
            };
            repetition.set("scanned_rows_per_query", per_query(ROWS_SCANNED));
            repetition.set("scanned_bytes_per_query", per_query(BYTES_SCANNED));
        }
        if let Some(write) = write {
            let inserted = after.sum(INSERTED, &[]) - before.sum(INSERTED, &[]);
            repetition.set("rows", Reading::count(outcome.rows));
            repetition.set("retained_mib", Reading::exact(inserted / MIB));
            repetition.set(
                "wire_bytes",
                if write == WriteOp::Native {
                    Reading::missing("native writes send Arrow IPC, not OTLP")
                } else {
                    Reading::count(outcome.wire_bytes)
                },
            );
            repetition.set(
                "visibility_p99_ms",
                Reading::missing(
                    "L1 measures closed-loop acknowledgement; visibility is judged at L2",
                ),
            );
            repetition.set("storage_ratio", Reading::missing("publication continues past the window, so its Parquet bytes are not attributable to it"));
        }
        Ok(repetition)
    }
}

/// Bytes per MiB.
const MIB: f64 = 1_048_576.0;

/// Sets the client readings every closed-loop step shares: requests,
/// errors, diagnostic client round-trip percentiles, and, for a read
/// `case`, answered and wrong queries.
fn read_outcome(repetition: &mut Repetition, outcome: &Outcome, case: Option<Case>) {
    let wrong = outcome.errors.get(WRONG_RESULT).copied().unwrap_or(0);
    let requests = outcome.latencies_us.len().to_u64().unwrap_or(u64::MAX);
    repetition.set("requests", Reading::count(requests));
    repetition.set("errors", Reading::count(outcome.error_count()));
    for (id, q) in [
        ("client_p50_ms", 0.5),
        ("client_p95_ms", 0.95),
        ("client_p99_ms", 0.99),
    ] {
        repetition.set(id, Reading::percentile(&outcome.latencies_us, q, 1e3));
    }
    if let Some(case) = case {
        repetition.set("queries", Reading::count(requests));
        repetition.set("wrong_results", Reading::count(wrong));
        if case == Case::FullScan {
            repetition.set(
                "p99_ms",
                Reading::missing("p95 only: a full-scan window holds too few samples for a p99"),
            );
        }
    }
}

#[async_trait]
impl Workload for Isolated {
    /// Starts the server, writes the standard fixture, waits until every row
    /// is published as Parquet, computes the exact answers, and registers
    /// each write path.
    async fn prepare(&mut self) -> Result<(), ExperimentError> {
        let collector = Collector::start(&self.logs)
            .await
            .map_err(ExperimentError::workload)?;
        let server = start(&collector).await.map_err(ExperimentError::workload)?;
        self.collector = Some(collector);
        let client = client(&server).map_err(ExperimentError::workload)?;
        let fixture = Fixture::standard();
        let loader = Sender::native(&client, TABLE, fixture)
            .await
            .map_err(ExperimentError::workload)?;
        let loaded = loader
            .closed_loop(
                &Arc::new(AtomicI64::new(0)),
                fixture.rows,
                LOADERS,
                &CancellationToken::new(),
            )
            .await;
        if loaded.error_count() > 0 || loaded.rows != fixture.rows.cast_unsigned() {
            return Err(ExperimentError::workload(format!(
                "the read fixture loaded {} of {} rows with errors {:?}",
                loaded.rows, fixture.rows, loaded.errors
            )));
        }
        Residence::settle(&server, SETTLE_TIMEOUT)
            .await
            .map_err(ExperimentError::workload)?;
        self.bench = Some(Arc::new(Bench::new(
            &client,
            fixture,
            &[Case::Selective, Case::SmallAggregate, Case::FullScan],
        )));
        let writes = Fixture {
            rows: WRITE_FIXTURE_ROWS,
        };
        let otlp = Otlp::connect(&client)
            .await
            .map_err(ExperimentError::workload)?;
        let native = Sender::native(&client, INGEST_TABLE, writes)
            .await
            .map_err(ExperimentError::workload)?;
        self.senders = vec![
            (WriteOp::Native, native, Arc::new(AtomicI64::new(0))),
            (
                WriteOp::Spans,
                Sender::Spans(
                    otlp.clone(),
                    Telemetry {
                        spans: WRITE_FIXTURE_ROWS,
                    },
                ),
                Arc::new(AtomicI64::new(0)),
            ),
            (
                WriteOp::Logs,
                Sender::Logs(
                    otlp,
                    Logs {
                        records: WRITE_FIXTURE_ROWS,
                    },
                ),
                Arc::new(AtomicI64::new(0)),
            ),
        ];
        self.server = Some(server);
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
    async fn measure(
        &mut self,
        step: &Step,
        phase: Phase,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        self.closed(step, phase, window).await
    }

    /// Stops the server and keeps its log.
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
