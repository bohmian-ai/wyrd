//! The application side of the benchmark: public-client writes and queries
//! against the running server, and what each step measured.
//!
//! Every step is a [`Bench`] method returning a [`Measured`]. Query steps are
//! closed loops: N clients each send one query, check its exact answer, and
//! send the next, for a fixed window after a short warm-up. Server evidence
//! for the window comes from the process's cgroup and `/metrics`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
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
use wyrd_testing::release_server::{LocalServer, MemoryPeak};

/// Untimed load before each window, so connection setup is not measured.
const WARMUP: Duration = Duration::from_secs(2);

/// Measured window of each query step.
const WINDOW: Duration = Duration::from_secs(10);

/// Concurrent public writers in every write step.
const WRITERS: usize = 4;

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
    /// Server CPU seconds from the cgroup's `cpu.stat`.
    pub server_cpu_seconds: f64,
    /// Peak server cgroup memory.
    pub peak_memory_bytes: u64,
    /// New `oom_kill` events in the server cgroup.
    pub oom_kills: u64,
    /// New Scribe seals (`bifrost_scribe_seal_total`).
    pub seals: f64,
    /// New physical bytes Oracle scanned (`oracle_query_bytes_scanned_total`).
    pub scanned_bytes: f64,
    /// CPU seconds this benchmark process used, to show the driver was not
    /// the bottleneck.
    pub driver_cpu_seconds: f64,
}

/// Server state at the start of a window, closed into a [`Usage`].
struct Window {
    /// Window start.
    started: Instant,
    /// Server `cpu.stat usage_usec` at start.
    server_cpu_us: u64,
    /// Server `memory.events oom_kill` at start.
    oom_kills: u64,
    /// Peak-memory descriptor reset at start.
    peak: MemoryPeak,
    /// `bifrost_scribe_seal_total` at start.
    seals: f64,
    /// `oracle_query_bytes_scanned_total` at start.
    scanned_bytes: f64,
    /// This process's CPU seconds at start.
    driver_cpu_seconds: f64,
}

impl Window {
    /// Records the server's counters now.
    ///
    /// # Errors
    ///
    /// Returns scrape or cgroup failures.
    async fn open(server: &LocalServer) -> Result<Self> {
        let metrics = server.metrics().await?;
        Ok(Self {
            server_cpu_us: server.cgroup_stat("cpu.stat", "usage_usec"),
            oom_kills: server.cgroup_stat("memory.events", "oom_kill"),
            peak: server.memory_peak()?,
            seals: metrics.sum("bifrost_scribe_seal_total", &[]),
            scanned_bytes: metrics.sum("oracle_query_bytes_scanned_total", &[]),
            driver_cpu_seconds: driver_cpu_seconds(),
            started: Instant::now(),
        })
    }

    /// Differences the server's counters against the start.
    ///
    /// # Errors
    ///
    /// Returns scrape or cgroup failures.
    async fn close(self, server: &LocalServer) -> Result<Usage> {
        let seconds = self.started.elapsed().as_secs_f64();
        let metrics = server.metrics().await?;
        Ok(Usage {
            seconds,
            server_cpu_seconds: server
                .cgroup_stat("cpu.stat", "usage_usec")
                .saturating_sub(self.server_cpu_us) as f64
                / 1e6,
            peak_memory_bytes: self.peak.read()?,
            oom_kills: server
                .cgroup_stat("memory.events", "oom_kill")
                .saturating_sub(self.oom_kills),
            seals: metrics.sum("bifrost_scribe_seal_total", &[]) - self.seals,
            scanned_bytes: metrics.sum("oracle_query_bytes_scanned_total", &[])
                - self.scanned_bytes,
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
    /// The server, for `/metrics` and cgroup evidence.
    server: &'a LocalServer,
    /// The public client every writer and query shares.
    client: &'a WyrdClient,
    /// Public query handle.
    queries: Arc<Bifrost>,
    /// Fixture size under test.
    fixture: Fixture,
    /// Exact answer of every aggregate variant, computed once.
    answers: Arc<BTreeMap<Case, Vec<Rows>>>,
}

impl<'a> Bench<'a> {
    /// Opens the query handle and computes the exact answers of `cases`.
    pub fn new(
        server: &'a LocalServer,
        client: &'a WyrdClient,
        fixture: Fixture,
        cases: &[Case],
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
        }
    }

    /// Registers `table` and writes the fixture's rows into it through the
    /// public client, [`WRITERS`] writers at a time, until `rows` are
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
        rows: i64,
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
        let next = Arc::new(AtomicI64::new(0));
        let mut writers = JoinSet::new();
        for _ in 0..WRITERS {
            let (writer, next, stop, fixture) = (
                Arc::clone(&writer),
                Arc::clone(&next),
                stop.clone(),
                self.fixture,
            );
            writers.spawn(async move {
                let mut measured = Measured::default();
                while !stop.is_cancelled() {
                    let first = next.fetch_add(workload::REQUEST_ROWS, Ordering::Relaxed);
                    if first >= rows {
                        break;
                    }
                    let last = (first + workload::REQUEST_ROWS).min(rows);
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
        let mut measured = join(writers).await;
        measured.clients = WRITERS;
        measured.usage = window.close(self.server).await?;
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
    /// [`WARMUP`].
    ///
    /// # Errors
    ///
    /// Returns evidence failures.
    pub async fn run(&self, case: Case, clients: usize) -> Result<Measured> {
        let sequence = Arc::new(AtomicU64::new(0));
        self.drive(case, clients, WARMUP, &sequence).await;
        let window = Window::open(self.server).await?;
        let mut measured = self.drive(case, clients, WINDOW, &sequence).await;
        measured.usage = window.close(self.server).await?;
        Ok(measured)
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
            self.write(workload::INGEST_TABLE, i64::MAX, stop.clone()),
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
