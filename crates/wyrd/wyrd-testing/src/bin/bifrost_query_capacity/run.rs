//! The application side of the benchmark: public-client writes and queries
//! against the running server, and what each step measured.
//!
//! Every step is a [`Bench`] method returning a [`Measured`]. Query steps are
//! closed loops: N clients each send one query, check its exact answer, and
//! send the next, for a fixed window after a short warm-up. Server evidence
//! for the window comes from the OS's readings of the server process and
//! `/metrics`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use arrow::array::{AsArray as _, RecordBatch};
use arrow::datatypes::{DataType, Int64Type};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use wyrd_client::bifrost::{BifrostClientError, TableConfig};
use wyrd_client::{Bifrost, WyrdClient};
use wyrd_spec::vala::api::{BifrostQueryRequest, QueryTerminalOutcome};

use crate::Result;
use crate::workload::{self, Case, Fixture, Rows};
use wyrd_testing::capacity::driver_cpu_seconds;
use wyrd_testing::profile::{Capture, Profile};
use wyrd_testing::release_server::{LocalServer, MemoryPeak};

/// Untimed load before each window, so connection setup is not measured.
const WARMUP: Duration = Duration::from_secs(2);

/// Measured window of each query step.
const WINDOW: Duration = Duration::from_secs(10);

/// Sampling frequency of a profiled window, hertz: high, because a
/// single-client window spends only a few milliseconds per request.
const PROFILE_HERTZ: u32 = 999;

/// Oracle's per-query scan counters a window differences, so a step shows
/// how much each query opened and pruned.
const SCAN_COUNTERS: [&str; 6] = [
    "oracle_query_files_scanned_total",
    "oracle_query_partitions_scanned_total",
    "oracle_query_row_groups_scanned_total",
    "oracle_query_row_groups_pruned_total",
    "oracle_query_row_groups_pruned_bloom_total",
    "oracle_query_rows_pruned_page_index_total",
];

/// Oracle's per-query profile counter of timed events, by `event` label.
const PROFILE_TIMES: &str = "oracle_query_profile_nanoseconds_total";

/// Profiles Oracle has emitted, one per settled query.
const PROFILE_COUNT: &str = "oracle_query_profiles_total";

/// Seconds every Vala runtime-pool acquisition waited, all callers summed.
const POOL_WAIT: &str = "vala_postgres_pool_acquire_seconds_sum";

/// Vala runtime-pool acquisitions, the divisor of [`POOL_WAIT`].
const POOL_ACQUIRES: &str = "vala_postgres_pool_acquire_seconds_count";

/// The server's write-path series a write step differences, keyed by the
/// name its Pareto prints: per-batch seconds (`*_seconds_sum`), the ack count
/// that divides them, and the rows, bytes, and fsyncs the window committed.
const WRITE_SERIES: [(&str, &str, &[&str]); 11] = [
    (
        "Gate.write_request",
        "bifrost_gate_request_duration_seconds_sum",
        &["operation=\"write\""],
    ),
    ("Scribe.ack", "bifrost_scribe_ack_seconds_sum", &[]),
    (
        "Scribe.ingress_job",
        "bifrost_scribe_lane_job_seconds_sum",
        &["lane=\"ingress\""],
    ),
    (
        "Scribe.wal_io_job",
        "bifrost_scribe_lane_job_seconds_sum",
        &["lane=\"wal_io\""],
    ),
    (
        "Scribe.wal_append",
        "bifrost_scribe_wal_append_seconds_sum",
        &[],
    ),
    (
        "Scribe.wal_fsync",
        "bifrost_scribe_wal_fsync_seconds_sum",
        &[],
    ),
    (
        "Scribe.queue_wait",
        "bifrost_scribe_queue_wait_seconds_sum",
        &[],
    ),
    (ACKS, "bifrost_scribe_ack_seconds_count", &[]),
    (FSYNCS, "bifrost_scribe_wal_fsync_seconds_count", &[]),
    (
        MEMTABLE_BYTES,
        "bifrost_scribe_memtable_bytes_inserted_total",
        &[],
    ),
    (
        MEMTABLE_ROWS,
        "bifrost_scribe_memtable_rows_inserted_total",
        &[],
    ),
];

/// [`WRITE_SERIES`] key of the durable acks, the per-batch divisor.
const ACKS: &str = "acks";

/// [`WRITE_SERIES`] key of the WAL fsyncs.
const FSYNCS: &str = "fsyncs";

/// [`WRITE_SERIES`] key of the retained Arrow bytes inserted into memtables.
pub const MEMTABLE_BYTES: &str = "memtable_bytes";

/// [`WRITE_SERIES`] key of the rows inserted into memtables.
const MEMTABLE_ROWS: &str = "memtable_rows";

/// Profile events printed per window, largest first.
const PARETO_ROWS: usize = 15;

/// Concurrent public writers in every write step.
pub const WRITERS: usize = 4;

/// Oracle's production queue places.
pub const QUEUE_PLACES: usize = 1_000;

/// How long the queue may take to fill or to empty.
const QUEUE_SETTLE: Duration = Duration::from_secs(60);

/// How long the query sent into the full queue may take to be refused.
const OVERFLOW_WAIT: Duration = Duration::from_secs(30);

/// Stable code of the queue-full refusal.
pub const QUEUE_FULL: &str = "WYRD_VALA_429_QUERY_QUEUE_FULL";

/// What one step measured.
#[derive(Debug, Default)]
pub struct Measured {
    /// Concurrent clients (or writers).
    pub clients: usize,
    /// Latency of every successful request, microseconds.
    pub latencies_us: Vec<u64>,
    /// Failed requests by stable error code, `wrong-result`, or terminal.
    pub errors: BTreeMap<String, u64>,
    /// Rows acknowledged by a write step.
    pub rows: u64,
    /// Server and driver usage over the window.
    pub usage: Usage,
}

impl Measured {
    /// Successful requests per second over the window.
    pub fn rate(&self) -> f64 {
        self.latencies_us.len() as f64 / self.usage.seconds.max(f64::EPSILON)
    }

    /// Failed requests.
    pub fn error_count(&self) -> u64 {
        self.errors.values().sum()
    }

    /// Nearest-rank percentile of successful latency in milliseconds.
    pub fn percentile_ms(&self, percentile: f64) -> Option<f64> {
        let mut sorted = self.latencies_us.clone();
        sorted.sort_unstable();
        let rank = ((percentile / 100.0) * sorted.len() as f64).ceil() as usize;
        sorted
            .get(rank.saturating_sub(1))
            .map(|micros| *micros as f64 / 1_000.0)
    }

    /// Folds another client's results into this one.
    fn merge(&mut self, other: Self) {
        self.latencies_us.extend(other.latencies_us);
        self.rows += other.rows;
        for (code, count) in other.errors {
            *self.errors.entry(code).or_default() += count;
        }
    }
}

/// Server and driver usage over one window.
#[derive(Debug, Default)]
pub struct Usage {
    /// Wall-clock window length.
    pub seconds: f64,
    /// Server CPU seconds.
    pub server_cpu_seconds: f64,
    /// Peak server memory (lifetime peak on macOS).
    pub peak_memory_bytes: u64,
    /// New OOM kills in a scoped server's cgroup; zero on the host, where an
    /// OOM kill ends the run.
    pub oom_kills: u64,
    /// New physical bytes Oracle scanned (`oracle_query_bytes_scanned_total`).
    pub scanned_bytes: f64,
    /// New counts of each [`SCAN_COUNTERS`] entry, in its order.
    pub scan_counts: [f64; SCAN_COUNTERS.len()],
    /// New nanoseconds of each Oracle query profile event.
    pub profile_ns: BTreeMap<String, f64>,
    /// New Oracle query profiles, the divisor of `profile_ns`.
    pub profiles: f64,
    /// New seconds spent waiting for a Vala runtime-pool connection.
    pub pool_wait_seconds: f64,
    /// New Vala runtime-pool acquisitions.
    pub pool_acquires: f64,
    /// New value of each [`WRITE_SERIES`] entry, by its key.
    pub write: BTreeMap<&'static str, f64>,
    /// CPU seconds this benchmark process used, to show the driver was not
    /// the bottleneck.
    pub driver_cpu_seconds: f64,
}

/// Server state at the start of a window, closed into a [`Usage`].
struct Window {
    /// Window start.
    started: Instant,
    /// Server CPU microseconds at start.
    server_cpu_us: u64,
    /// Server OOM kills at start.
    oom_kills: u64,
    /// Peak-memory window reset at start.
    peak: MemoryPeak,
    /// `oracle_query_bytes_scanned_total` at start.
    scanned_bytes: f64,
    /// Each [`SCAN_COUNTERS`] entry at start.
    scan_counts: [f64; SCAN_COUNTERS.len()],
    /// Each Oracle query profile event's nanoseconds at start.
    profile_ns: BTreeMap<String, f64>,
    /// Oracle query profiles at start.
    profiles: f64,
    /// [`POOL_WAIT`] at start.
    pool_wait_seconds: f64,
    /// [`POOL_ACQUIRES`] at start.
    pool_acquires: f64,
    /// Each [`WRITE_SERIES`] entry at start, by its key.
    write: BTreeMap<&'static str, f64>,
    /// This process's CPU seconds at start.
    driver_cpu_seconds: f64,
}

impl Window {
    /// Records the server's counters now.
    ///
    /// # Errors
    ///
    /// Returns scrape or memory-peak failures.
    async fn open(server: &LocalServer) -> Result<Self> {
        let metrics = server.metrics().await?;
        Ok(Self {
            server_cpu_us: server.cpu_usec(),
            oom_kills: server.oom_kills(),
            peak: server.memory_peak()?,
            scanned_bytes: metrics.sum("oracle_query_bytes_scanned_total", &[]),
            scan_counts: SCAN_COUNTERS.map(|name| metrics.sum(name, &[])),
            profile_ns: metrics.by_label(PROFILE_TIMES, "event"),
            profiles: metrics.sum(PROFILE_COUNT, &[]),
            pool_wait_seconds: metrics.sum(POOL_WAIT, &[]),
            pool_acquires: metrics.sum(POOL_ACQUIRES, &[]),
            write: WRITE_SERIES
                .iter()
                .map(|(key, family, labels)| (*key, metrics.sum(family, labels)))
                .collect(),
            driver_cpu_seconds: driver_cpu_seconds(),
            started: Instant::now(),
        })
    }

    /// Differences the server's counters against the start.
    ///
    /// # Errors
    ///
    /// Returns scrape or memory-peak failures.
    async fn close(self, server: &LocalServer) -> Result<Usage> {
        let seconds = self.started.elapsed().as_secs_f64();
        let metrics = server.metrics().await?;
        Ok(Usage {
            seconds,
            server_cpu_seconds: server.cpu_usec().saturating_sub(self.server_cpu_us) as f64 / 1e6,
            peak_memory_bytes: self.peak.read()?,
            oom_kills: server.oom_kills().saturating_sub(self.oom_kills),
            scanned_bytes: metrics.sum("oracle_query_bytes_scanned_total", &[])
                - self.scanned_bytes,
            scan_counts: std::array::from_fn(|index| {
                metrics.sum(SCAN_COUNTERS[index], &[]) - self.scan_counts[index]
            }),
            profile_ns: metrics
                .by_label(PROFILE_TIMES, "event")
                .into_iter()
                .map(|(event, ns)| {
                    let before = self.profile_ns.get(&event).copied().unwrap_or(0.0);
                    (event, ns - before)
                })
                .collect(),
            profiles: metrics.sum(PROFILE_COUNT, &[]) - self.profiles,
            pool_wait_seconds: metrics.sum(POOL_WAIT, &[]) - self.pool_wait_seconds,
            pool_acquires: metrics.sum(POOL_ACQUIRES, &[]) - self.pool_acquires,
            write: WRITE_SERIES
                .iter()
                .map(|(key, family, labels)| {
                    let before = self.write.get(key).copied().unwrap_or(0.0);
                    (*key, metrics.sum(family, labels) - before)
                })
                .collect(),
            driver_cpu_seconds: driver_cpu_seconds() - self.driver_cpu_seconds,
        })
    }
}

/// What the overload step observed.
#[derive(Debug, Default)]
pub struct Overload {
    /// Highest `oracle_queries_queued` seen while filling.
    pub queued: usize,
    /// Outcome of the query sent into the full queue: its error code,
    /// `answered`, or `queued` when it waited past [`OVERFLOW_WAIT`].
    pub overflow: String,
    /// Seconds for the queue to empty after the waiters disconnected.
    pub drained_seconds: Option<f64>,
    /// Server usage across the step.
    pub usage: Usage,
}

/// The application: one public client against the running server.
pub struct Bench<'a> {
    /// The server, for `/metrics` and resource evidence.
    server: &'a LocalServer,
    /// The public client every writer and query shares.
    client: &'a WyrdClient,
    /// Public query handle.
    queries: Arc<Bifrost>,
    /// Fixture size under test.
    fixture: Fixture,
    /// Exact answer of every aggregate variant, computed once.
    answers: Arc<BTreeMap<Case, Vec<Rows>>>,
    /// Where each query window's `perf` capture lands.
    profiles: PathBuf,
    /// Query windows run so far; numbers each capture's directory so a
    /// repeated case and concurrency keeps its own capture.
    windows: AtomicUsize,
}

impl<'a> Bench<'a> {
    /// Opens the query handle and computes the exact answers of `cases`.
    ///
    /// Every query window is captured with `perf` under
    /// `profiles/<nn>-<case>-<clients>/`.
    pub fn new(
        server: &'a LocalServer,
        client: &'a WyrdClient,
        fixture: Fixture,
        cases: &[Case],
        profiles: PathBuf,
    ) -> Self {
        let answers = cases
            .iter()
            .filter(|case| **case != Case::Selective)
            .map(|case| {
                let variants = (0..case.variants())
                    .map(|variant| case.expected(fixture, variant))
                    .collect();
                (*case, variants)
            })
            .collect();
        Self {
            server,
            client,
            queries: Arc::new(Bifrost::query_only(client)),
            fixture,
            answers: Arc::new(answers),
            profiles,
            windows: AtomicUsize::new(0),
        }
    }

    /// Registers `table` and writes the fixture's `rows` into it through the
    /// public client, `writers` writers at a time, until every row is
    /// acknowledged or `stop` is cancelled.
    ///
    /// Each `write_batch` resolves at the durable acknowledgement, so the
    /// rate is the durable write rate. A refused batch is counted by its
    /// stable code.
    ///
    /// # Errors
    ///
    /// Returns connection, registration, or evidence failures.
    pub async fn write(
        &self,
        table: &'static str,
        rows: std::ops::Range<i64>,
        writers: usize,
        stop: CancellationToken,
    ) -> Result<Measured> {
        let writer = Bifrost::connect_with_table(
            self.client,
            TableConfig::from_arrow(table, workload::schema())?,
        )
        .await?;
        writer.register().await?;
        let writer = Arc::new(writer);
        let window = Window::open(self.server).await?;
        let next = Arc::new(AtomicI64::new(rows.start));
        let end = rows.end;
        let mut tasks = JoinSet::new();
        for _ in 0..writers {
            let (writer, next, stop, fixture) = (
                Arc::clone(&writer),
                Arc::clone(&next),
                stop.clone(),
                self.fixture,
            );
            tasks.spawn(async move {
                let mut measured = Measured::default();
                while !stop.is_cancelled() {
                    let first = next.fetch_add(workload::REQUEST_ROWS, Ordering::Relaxed);
                    if first >= end {
                        break;
                    }
                    let last = (first + workload::REQUEST_ROWS).min(end);
                    let Ok(batch) = fixture.batch(first, last) else {
                        *measured.errors.entry("fixture".to_owned()).or_default() += 1;
                        continue;
                    };
                    let sent = Instant::now();
                    match writer.write_batch(table, &batch).await {
                        Ok(()) => {
                            measured.latencies_us.push(micros(sent));
                            measured.rows += (last - first) as u64;
                        }
                        Err(error) => *measured.errors.entry(code(&error)).or_default() += 1,
                    }
                }
                measured
            });
        }
        let mut measured = join(tasks).await;
        measured.clients = writers;
        measured.usage = window.close(self.server).await?;
        Ok(measured)
    }

    /// Writes `rows` of the fixture into `table` from `writers` writers,
    /// capturing the window with `perf`.
    ///
    /// Prints the window's write Pareto: the server's mean seconds per durable
    /// batch in each write-path stage beside the client's mean latency, the
    /// throughput in retained Arrow MB/s and MB/s per server core, fsyncs and
    /// pool acquires per batch, and the CPU Pareto by crate.
    ///
    /// # Errors
    ///
    /// Returns connection, registration, evidence, or capture failures.
    pub async fn write_step(
        &self,
        table: &'static str,
        rows: std::ops::Range<i64>,
        writers: usize,
    ) -> Result<Measured> {
        let capture = self.capture("write", writers)?;
        let measured = self
            .write(table, rows, writers, CancellationToken::new())
            .await?;
        let metadata = serde_json::json!({
            "case": "write",
            "clients": writers,
            "write": measured.usage.write,
        });
        let profile = capture.finish(&metadata)?;
        report_write_pareto(&measured, &profile);
        Ok(measured)
    }

    /// Asks every variant of each of `cases` once and checks its exact answer.
    ///
    /// # Errors
    ///
    /// Returns evidence failures; wrong answers are counted, not returned.
    pub async fn check_answers(&self, cases: &[Case]) -> Result<Measured> {
        let window = Window::open(self.server).await?;
        let mut measured = Measured {
            clients: 1,
            ..Measured::default()
        };
        for case in cases {
            for sequence in 0..case.variants() {
                let (sql, expected) = self.question(*case, sequence);
                let sent = Instant::now();
                match ask(&self.queries, sql, &expected).await {
                    Ok(()) => measured.latencies_us.push(micros(sent)),
                    Err(error) => {
                        *measured
                            .errors
                            .entry(format!("{}: {error}", case.name()))
                            .or_default() += 1;
                    }
                }
            }
        }
        measured.usage = window.close(self.server).await?;
        Ok(measured)
    }

    /// Runs `case` from `clients` closed-loop clients for [`WINDOW`] after
    /// [`WARMUP`], capturing the window with `perf`.
    ///
    /// Prints the window's latency Pareto, the Oracle query profile's mean
    /// time per query by event beside the client's mean latency, and its CPU
    /// Pareto, the capture's self CPU by crate scaled to server CPU per query.
    ///
    /// # Errors
    ///
    /// Returns evidence failures, or a capture that cannot start or finish.
    pub async fn run(&self, case: Case, clients: usize) -> Result<Measured> {
        let sequence = Arc::new(AtomicU64::new(0));
        self.drive(case, clients, WARMUP, &sequence).await;
        let capture = self.capture(case.name(), clients)?;
        let window = Window::open(self.server).await?;
        let mut measured = self.drive(case, clients, WINDOW, &sequence).await;
        measured.usage = window.close(self.server).await?;
        let queries = measured.latencies_us.len().max(1) as f64;
        let per_query: BTreeMap<&str, f64> = SCAN_COUNTERS
            .iter()
            .zip(measured.usage.scan_counts)
            .map(|(name, count)| (*name, count / queries))
            .collect();
        eprintln!("scan per query, {} x{clients}: {per_query:?}", case.name());
        let latency = latency_pareto(&measured);
        let metadata = serde_json::json!({
            "case": case.name(),
            "clients": clients,
            "scan_per_query": per_query,
            "profile_mean_ms": latency,
        });
        let profile = capture.finish(&metadata)?;
        report_pareto(case, &measured, &latency, &profile);
        Ok(measured)
    }

    /// Starts a `perf` capture of the server for the next window of the step
    /// named `step` at `clients`.
    ///
    /// # Errors
    ///
    /// Returns an exited server or a capture that cannot start, such as a
    /// missing `perf`.
    fn capture(&self, step: &str, clients: usize) -> Result<Capture> {
        let number = self.windows.fetch_add(1, Ordering::Relaxed);
        let pid = self.server.pid().ok_or("the profiled server has exited")?;
        let directory = self.profiles.join(format!("{number:02}-{step}-{clients}"));
        Capture::start(pid, 0, PROFILE_HERTZ, &directory)
    }

    /// Runs `case` at `clients` twice: alone, then while [`WRITERS`] writers
    /// write [`workload::INGEST_TABLE`]. Returns the read-only row, the
    /// loaded row, and the concurrent write row.
    ///
    /// # Errors
    ///
    /// Returns write-setup or evidence failures.
    pub async fn reads_while_writing(
        &self,
        case: Case,
        clients: usize,
    ) -> Result<(Measured, Measured, Measured)> {
        let alone = self.run(case, clients).await?;
        let stop = CancellationToken::new();
        let (loaded, written) = tokio::join!(
            async {
                let loaded = self.run(case, clients).await;
                stop.cancel();
                loaded
            },
            self.write(workload::INGEST_TABLE, 0..i64::MAX, WRITERS, stop.clone()),
        );
        Ok((alone, loaded?, written?))
    }

    /// Fills Oracle's queue and sends one more query.
    ///
    /// Holders first occupy every execution slot with a streaming scan they
    /// stop reading; waiters then queue until `oracle_queries_queued` reaches
    /// [`QUEUE_PLACES`]; one more query must be refused as queue-full.
    /// Disconnecting the waiters must empty the queue.
    ///
    /// # Errors
    ///
    /// Returns scrape or evidence failures.
    pub async fn overload_queue(&self) -> Result<Overload> {
        let window = Window::open(self.server).await?;
        let slots =
            self.server
                .metrics()
                .await?
                .sum("bifrost_oracle_local_slot_units", &["kind=\"limit\""]) as usize;
        let release = CancellationToken::new();
        let mut holders = JoinSet::new();
        for _ in 0..slots {
            let (queries, release) = (Arc::clone(&self.queries), release.clone());
            holders.spawn(async move {
                let request = BifrostQueryRequest {
                    sql: format!("SELECT event_id FROM {}", workload::TABLE),
                    deadline_ms: None,
                };
                if let Ok(mut stream) = queries.query(&request).await {
                    let _ = stream.next_batch().await;
                    release.cancelled().await;
                }
            });
        }
        // A waiter that reaches a free slot before the holders take them all
        // is answered and never queues, so seat every holder first.
        let seating = Instant::now();
        while seating.elapsed() < QUEUE_SETTLE {
            let used = self
                .server
                .metrics()
                .await?
                .sum("bifrost_oracle_local_slot_units", &["kind=\"used\""])
                as usize;
            if used >= slots {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let mut waiters = JoinSet::new();
        for sequence in 0..QUEUE_PLACES as u64 {
            let (queries, (sql, expected)) = (
                Arc::clone(&self.queries),
                self.question(Case::Selective, sequence),
            );
            waiters.spawn(async move { ask(&queries, sql, &expected).await });
        }
        let mut overload = Overload::default();
        let filling = Instant::now();
        while overload.queued < QUEUE_PLACES && filling.elapsed() < QUEUE_SETTLE {
            tokio::time::sleep(Duration::from_millis(100)).await;
            overload.queued = overload.queued.max(self.queued().await?);
        }
        let (sql, expected) = self.question(Case::Selective, u64::MAX);
        overload.overflow =
            match tokio::time::timeout(OVERFLOW_WAIT, ask(&self.queries, sql, &expected)).await {
                Ok(Ok(())) => "answered".to_owned(),
                Ok(Err(error)) => error,
                Err(_) => "queued".to_owned(),
            };
        waiters.abort_all();
        while waiters.join_next().await.is_some() {}
        let draining = Instant::now();
        while draining.elapsed() < QUEUE_SETTLE {
            if self.queued().await? == 0 {
                overload.drained_seconds = Some(draining.elapsed().as_secs_f64());
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        release.cancel();
        while holders.join_next().await.is_some() {}
        overload.usage = window.close(self.server).await?;
        Ok(overload)
    }

    /// Queries Oracle reports waiting.
    ///
    /// # Errors
    ///
    /// Returns scrape failures.
    async fn queued(&self) -> Result<usize> {
        Ok(self
            .server
            .metrics()
            .await?
            .sum("oracle_queries_queued", &[]) as usize)
    }

    /// The statement and exact answer of `case` for `sequence`.
    fn question(&self, case: Case, sequence: u64) -> (String, Rows) {
        question(self.fixture, &self.answers, case, sequence)
    }

    /// Runs `clients` closed-loop clients of `case` for `duration`.
    async fn drive(
        &self,
        case: Case,
        clients: usize,
        duration: Duration,
        sequence: &Arc<AtomicU64>,
    ) -> Measured {
        let deadline = Instant::now() + duration;
        let mut tasks = JoinSet::new();
        for _ in 0..clients {
            let (queries, answers, sequence, fixture) = (
                Arc::clone(&self.queries),
                Arc::clone(&self.answers),
                Arc::clone(sequence),
                self.fixture,
            );
            tasks.spawn(async move {
                let mut measured = Measured::default();
                while Instant::now() < deadline {
                    let next = sequence.fetch_add(1, Ordering::Relaxed);
                    let (sql, expected) = question(fixture, &answers, case, next);
                    let sent = Instant::now();
                    match ask(&queries, sql, &expected).await {
                        Ok(()) => measured.latencies_us.push(micros(sent)),
                        Err(error) => *measured.errors.entry(error).or_default() += 1,
                    }
                }
                measured
            });
        }
        let mut measured = join(tasks).await;
        measured.clients = clients;
        measured
    }
}

/// The statement and exact answer of `case` for `sequence`: selective answers
/// are computed per ID, aggregate answers come from the precomputed table.
fn question(
    fixture: Fixture,
    answers: &BTreeMap<Case, Vec<Rows>>,
    case: Case,
    sequence: u64,
) -> (String, Rows) {
    let expected = match answers.get(&case) {
        Some(variants) => variants[(sequence % case.variants()) as usize].clone(),
        None => case.expected(fixture, sequence),
    };
    (case.sql(fixture, sequence), expected)
}

/// Sends one query with the server's default deadline, reads it to its
/// terminal, and compares its rows with `expected`.
///
/// # Errors
///
/// Returns the stable error code, the failed terminal's code, or
/// `wrong-result`.
async fn ask(queries: &Bifrost, sql: String, expected: &Rows) -> std::result::Result<(), String> {
    let request = BifrostQueryRequest {
        sql,
        deadline_ms: None,
    };
    let mut stream = queries
        .query(&request)
        .await
        .map_err(|error| code(&error))?;
    let mut rows = Vec::new();
    while let Some(batch) = stream.next_batch().await.map_err(|error| code(&error))? {
        rows.extend(int_rows(&batch).map_err(|_| "non-integer column".to_owned())?);
    }
    match stream.terminal() {
        Some(terminal) if terminal.outcome == QueryTerminalOutcome::Success => {
            if rows == *expected {
                Ok(())
            } else {
                Err("wrong-result".to_owned())
            }
        }
        Some(terminal) => Err(match &terminal.error {
            Some(error) => format!("{:?}", error.code),
            None => format!("{:?}", terminal.outcome),
        }),
        None => Err("no-terminal".to_owned()),
    }
}

/// The stable code a client error projects to.
fn code(error: &BifrostClientError) -> String {
    tracing::debug!(%error, "benchmark request failed");
    wyrd_spec::error::WyrdError::from(error).code().to_owned()
}

/// Casts every column of `batch` to `i64` and returns its rows.
///
/// # Errors
///
/// Returns the cast error for a column that is not integral.
fn int_rows(batch: &RecordBatch) -> std::result::Result<Rows, arrow::error::ArrowError> {
    let columns = batch
        .columns()
        .iter()
        .map(|column| arrow::compute::cast(column, &DataType::Int64))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let columns: Vec<_> = columns
        .iter()
        .map(|column| column.as_primitive::<Int64Type>())
        .collect();
    Ok((0..batch.num_rows())
        .map(|row| columns.iter().map(|column| column.value(row)).collect())
        .collect())
}

/// Waits for every task and merges their results; a panicked task counts as
/// one `panicked` error.
async fn join(mut tasks: JoinSet<Measured>) -> Measured {
    let mut total = Measured::default();
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(measured) => total.merge(measured),
            Err(_) => *total.errors.entry("panicked".to_owned()).or_default() += 1,
        }
    }
    total
}

/// Microseconds since `sent`.
fn micros(sent: Instant) -> u64 {
    u64::try_from(sent.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Mean milliseconds per query of each Oracle query profile event, largest
/// first, from the window's counter deltas.
///
/// Operator times sum across partitions, so an event may exceed wall time;
/// the events are read beside the client latency, never summed into it.
fn latency_pareto(measured: &Measured) -> Vec<(String, f64)> {
    let profiles = measured.usage.profiles.max(1.0);
    let mut events: Vec<(String, f64)> = measured
        .usage
        .profile_ns
        .iter()
        .filter(|(_, ns)| **ns > 0.0)
        .map(|(event, ns)| (event.clone(), ns / profiles / 1e6))
        .collect();
    events.sort_by(|left, right| right.1.total_cmp(&left.1));
    events
}

/// Prints one window's latency and CPU Pareto tables, or why the CPU
/// capture is not usable evidence.
fn report_pareto(case: Case, measured: &Measured, latency: &[(String, f64)], profile: &Profile) {
    let queries = measured.latencies_us.len().max(1) as f64;
    let client_mean_ms = measured.latencies_us.iter().sum::<u64>() as f64 / queries / 1_000.0;
    let cpu_ms = measured.usage.server_cpu_seconds * 1e3 / queries;
    eprintln!(
        "latency pareto, {} x{}: client mean {client_mean_ms:.2} ms, p50 {:.2} ms, {} profiles",
        case.name(),
        measured.clients,
        measured.percentile_ms(50.0).unwrap_or_default(),
        measured.usage.profiles
    );
    for (event, ms) in latency.iter().take(PARETO_ROWS) {
        eprintln!(
            "  {ms:8.3} ms {:5.1}%  {event}",
            100.0 * ms / client_mean_ms.max(f64::EPSILON)
        );
    }
    let acquires = measured.usage.pool_acquires;
    eprintln!(
        "  postgres pool: {:.2} acquires per query, {:.3} ms mean wait",
        acquires / queries,
        measured.usage.pool_wait_seconds * 1e3 / acquires.max(1.0)
    );
    eprintln!(
        "cpu pareto, {} x{}: {cpu_ms:.2} ms server CPU per query ({})",
        case.name(),
        measured.clients,
        profile.directory.display()
    );
    if let Some(failure) = &profile.failure {
        eprintln!("  profile INCOMPLETE: {failure}");
    } else {
        for (owner, percent) in &profile.crates {
            eprintln!(
                "  {:8.3} ms {percent:5.1}%  {owner}",
                cpu_ms * percent / 100.0
            );
        }
    }
    report_off_cpu(queries, profile);
}

/// Prints the window's off-CPU stacks as milliseconds per query, or why the
/// off-CPU capture is not usable evidence.
///
/// Off-CPU time sums over every server thread, idle runtime workers
/// included, so it is read by stack beside the client latency, never summed
/// into it.
fn report_off_cpu(queries: f64, profile: &Profile) {
    eprintln!(
        "off-cpu pareto ({}/offcpu.folded):",
        profile.directory.display()
    );
    if let Some(failure) = &profile.off_cpu_failure {
        eprintln!("  off-cpu INCOMPLETE: {failure}");
        return;
    }
    for (stack, seconds) in &profile.off_cpu {
        eprintln!("  {:10.3} ms  {stack}", seconds * 1e3 / queries);
    }
}

/// Prints one write window's per-batch stage times, throughput, and CPU
/// Pareto, or why the CPU capture is not usable evidence.
///
/// Stage times are the server's histogram sums divided by its durable acks,
/// so they are mean seconds per batch. Stages overlap (the ack contains the
/// others), so they are read beside the client latency, never summed.
fn report_write_pareto(measured: &Measured, profile: &Profile) {
    let write = &measured.usage.write;
    let acks = write.get(ACKS).copied().unwrap_or(0.0).max(1.0);
    let batches = measured.latencies_us.len().max(1) as f64;
    let client_mean_ms = measured.latencies_us.iter().sum::<u64>() as f64 / batches / 1_000.0;
    let megabytes = write.get(MEMTABLE_BYTES).copied().unwrap_or(0.0) / 1e6;
    let cores = measured.usage.server_cpu_seconds;
    eprintln!(
        "write pareto x{}: {:.1} MB/s, {:.1} MB/s per server core, {:.0} rows/s, client mean {client_mean_ms:.2} ms per batch",
        measured.clients,
        megabytes / measured.usage.seconds.max(f64::EPSILON),
        megabytes / cores.max(f64::EPSILON),
        write.get(MEMTABLE_ROWS).copied().unwrap_or(0.0) / measured.usage.seconds.max(f64::EPSILON),
    );
    for (key, _, _) in WRITE_SERIES.iter().take(7) {
        let ms = write.get(key).copied().unwrap_or(0.0) * 1e3 / acks;
        eprintln!(
            "  {ms:8.3} ms {:5.1}%  {key}",
            100.0 * ms / client_mean_ms.max(f64::EPSILON)
        );
    }
    eprintln!(
        "  {:.2} fsyncs and {:.2} pool acquires per batch",
        write.get(FSYNCS).copied().unwrap_or(0.0) / acks,
        measured.usage.pool_acquires / acks
    );
    let cpu_ms = cores * 1e3 / acks;
    eprintln!(
        "cpu pareto, write x{}: {cpu_ms:.2} ms server CPU per batch ({})",
        measured.clients,
        profile.directory.display()
    );
    if let Some(failure) = &profile.failure {
        eprintln!("  profile INCOMPLETE: {failure}");
        return;
    }
    for (owner, percent) in &profile.crates {
        eprintln!(
            "  {:8.3} ms {percent:5.1}%  {owner}",
            cpu_ms * percent / 100.0
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{Measured, Usage};

    /// Percentiles are nearest-rank and the rate counts only successes.
    ///
    /// # Panics
    ///
    /// Panics when a percentile or the rate is miscomputed.
    #[test]
    fn measured_percentiles_are_nearest_rank() {
        let measured = Measured {
            latencies_us: (1..=100).map(|ms| ms * 1_000).collect(),
            errors: [("wrong-result".to_owned(), 5)].into(),
            usage: Usage {
                seconds: 10.0,
                ..Usage::default()
            },
            ..Measured::default()
        };
        assert_eq!(measured.percentile_ms(50.0), Some(50.0));
        assert_eq!(measured.percentile_ms(99.0), Some(99.0));
        assert_eq!(measured.rate(), 10.0);
        assert_eq!(measured.error_count(), 5);
        assert_eq!(Measured::default().percentile_ms(50.0), None);
    }
}
