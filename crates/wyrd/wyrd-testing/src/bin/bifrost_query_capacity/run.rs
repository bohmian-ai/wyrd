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
use wyrd_tonic::otlp::logs_service::logs_service_client::LogsServiceClient;
use wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient;
use wyrd_tonic::prost::Message as _;
use wyrd_tonic::tonic::Request;
use wyrd_tonic::tonic::transport::Channel;

use crate::Result;
use crate::logs::{self, Logs};
use crate::telemetry::{self, Telemetry};
use crate::workload::{self, Case, Fixture, Rows};
use wyrd_testing::capacity::driver_cpu_seconds;
use wyrd_testing::profile::{Capture, Profile};
use wyrd_testing::release_server::{LocalServer, MemoryPeak, Metrics};

/// Untimed load before each window, so connection setup is not measured.
const WARMUP: Duration = Duration::from_secs(2);

/// Measured window of each query step.
const WINDOW: Duration = Duration::from_secs(10);

/// Sampling frequency of a profiled window, hertz: high, because a
/// single-client window spends only a few milliseconds per request.
const PROFILE_HERTZ: u32 = 999;

/// Oracle's per-query scan counters a window differences, so a step shows
/// how much each query opened and pruned.
pub const SCAN_COUNTERS: [&str; 8] = [
    "oracle_query_files_scanned_total",
    "oracle_query_partitions_scanned_total",
    "oracle_query_row_groups_scanned_total",
    "oracle_query_row_groups_pruned_total",
    "oracle_query_row_groups_pruned_bloom_total",
    "oracle_query_rows_pruned_page_index_total",
    "oracle_query_row_groups_sized_total",
    "bifrost_scan_sizing_fallback_total",
];

/// Most row groups Oracle's scan sizing may keep per row group the scan
/// reads before a window is flagged: dynamic filters let a scan read fewer
/// than sizing could rule out, but not this many fewer.
const SIZING_DIVERGENCE: f64 = 4.0;

/// Oracle's per-query profile: its timed-event counter, by `event` label,
/// and its count of emitted profiles, one per settled query.
const QUERY_PROFILE: ProfileFamily = ProfileFamily {
    nanoseconds: "oracle_query_profile_nanoseconds_total",
    profiles: "oracle_query_profiles_total",
};

/// Scribe's per-request write profile: its timed-event counter, by `event`
/// label, and its count of emitted profiles, one per settled write request.
const WRITE_PROFILE: ProfileFamily = ProfileFamily {
    nanoseconds: "scribe_write_profile_nanoseconds_total",
    profiles: "scribe_write_profiles_total",
};

/// Seconds every Vala runtime-pool acquisition waited, all callers summed.
const POOL_WAIT: &str = "vala_postgres_pool_acquire_seconds_sum";

/// Vala runtime-pool acquisitions, the divisor of [`POOL_WAIT`].
const POOL_ACQUIRES: &str = "vala_postgres_pool_acquire_seconds_count";

/// The server's write-path series a write step differences, keyed by the
/// name its Pareto prints: the Gate's request seconds, and the rows, bytes,
/// and fsyncs the window committed. Per-step times come from
/// [`WRITE_PROFILE`].
const WRITE_SERIES: [(&str, &str, &[&str]); 4] = [
    (
        GATE_WRITE,
        "bifrost_gate_request_duration_seconds_sum",
        &["operation=\"write\""],
    ),
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

/// [`WRITE_SERIES`] key of the Gate's write-request seconds, the server
/// side of each request the write profile divides.
const GATE_WRITE: &str = "Gate.write_request";

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
    /// Wall-clock completion of every successful request, Unix milliseconds,
    /// index-aligned with `latencies_us` so a slow request can be matched to
    /// server log timestamps.
    pub completed_unix_ms: Vec<u64>,
    /// Failed requests by stable error code, `wrong-result`, or terminal.
    pub errors: BTreeMap<String, u64>,
    /// Rows acknowledged by a write step.
    pub rows: u64,
    /// Encoded OTLP protobuf bytes of the acknowledged export requests, the
    /// wire volume an OTLP ingest rate is quoted in; zero for native writes.
    pub wire_bytes: u64,
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

    /// Records one successful request sent at `sent`, completing now.
    fn record(&mut self, sent: Instant) {
        self.latencies_us.push(micros(sent));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| {
                u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
            });
        self.completed_unix_ms.push(now);
    }

    /// Folds another client's results into this one.
    fn merge(&mut self, other: Self) {
        self.latencies_us.extend(other.latencies_us);
        self.completed_unix_ms.extend(other.completed_unix_ms);
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
    /// New Oracle query profile events.
    pub query_profile: ProfileDelta,
    /// New Scribe write profile events.
    pub write_profile: ProfileDelta,
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

impl Usage {
    /// This window's new count of the [`SCAN_COUNTERS`] entry `counter`, or
    /// zero for a name that is not one.
    #[must_use]
    pub fn scanned(&self, counter: &str) -> f64 {
        SCAN_COUNTERS
            .iter()
            .position(|name| *name == counter)
            .map_or(0.0, |index| self.scan_counts[index])
    }

    /// Returns why this window's scan sizing diverged from its scans, or
    /// `None` when it did not.
    ///
    /// Sizing diverges when it fell back (a dropped filter or an unread
    /// footer, counted by `bifrost_scan_sizing_fallback_total`) or kept more
    /// than [`SIZING_DIVERGENCE`] times the row groups the scans read. Either
    /// leaves the answer exact and only mis-sizes partitions, so the report
    /// flags it rather than failing the row.
    pub fn sizing_divergence(&self) -> Option<String> {
        let sized = self.scanned("oracle_query_row_groups_sized_total");
        let scanned = self.scanned("oracle_query_row_groups_scanned_total");
        let fallbacks = self.scanned("bifrost_scan_sizing_fallback_total");
        (fallbacks > 0.0 || sized > SIZING_DIVERGENCE * scanned.max(1.0)).then(|| {
            format!(
                "sizing DIVERGENT: {sized:.0} row groups sized, {scanned:.0} scanned, \
                 {fallbacks:.0} fallbacks"
            )
        })
    }
}

/// The two counters one server profile family is scraped from.
#[derive(Debug, Clone, Copy)]
struct ProfileFamily {
    /// Nanoseconds counter, labelled by `event`.
    nanoseconds: &'static str,
    /// Emitted-profile counter, the divisor turning an event's nanoseconds
    /// into its mean per profiled unit.
    profiles: &'static str,
}

impl ProfileFamily {
    /// This family's counters in one scrape.
    fn read(self, metrics: &Metrics) -> ProfileDelta {
        ProfileDelta {
            nanoseconds: metrics.by_label(self.nanoseconds, "event"),
            profiles: metrics.sum(self.profiles, &[]),
        }
    }
}

/// One profile family's event nanoseconds and profile count: totals in one
/// scrape, or their growth over a window.
#[derive(Debug, Default)]
pub struct ProfileDelta {
    /// Nanoseconds of each profile event.
    nanoseconds: BTreeMap<String, f64>,
    /// Profiles emitted.
    pub profiles: f64,
}

impl ProfileDelta {
    /// The growth from `start` to these totals.
    fn since(self, start: &Self) -> Self {
        Self {
            nanoseconds: self
                .nanoseconds
                .into_iter()
                .map(|(event, ns)| {
                    let before = start.nanoseconds.get(&event).copied().unwrap_or(0.0);
                    (event, ns - before)
                })
                .collect(),
            profiles: self.profiles - start.profiles,
        }
    }

    /// Mean milliseconds per profiled unit of each event that accrued time,
    /// largest first.
    fn pareto(&self) -> Vec<(String, f64)> {
        let profiles = self.profiles.max(1.0);
        let mut events: Vec<(String, f64)> = self
            .nanoseconds
            .iter()
            .filter(|(_, ns)| **ns > 0.0)
            .map(|(event, ns)| (event.clone(), ns / profiles / 1e6))
            .collect();
        events.sort_by(|left, right| right.1.total_cmp(&left.1));
        events
    }
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
    /// Oracle query profile events at start.
    query_profile: ProfileDelta,
    /// Scribe write profile events at start.
    write_profile: ProfileDelta,
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
            query_profile: QUERY_PROFILE.read(&metrics),
            write_profile: WRITE_PROFILE.read(&metrics),
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
            query_profile: QUERY_PROFILE.read(&metrics).since(&self.query_profile),
            write_profile: WRITE_PROFILE.read(&metrics).since(&self.write_profile),
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
        let fixture = self.fixture;
        self.send_requests(
            rows,
            workload::REQUEST_ROWS,
            writers,
            stop,
            move |first, last| {
                let writer = Arc::clone(&writer);
                async move {
                    let batch = fixture
                        .batch(first, last)
                        .map_err(|_| "fixture".to_owned())?;
                    writer
                        .write_batch(table, &batch)
                        .await
                        .map_err(|error| code(&error))
                }
            },
        )
        .await
    }

    /// Exports rows `rows` of `signal`'s fixture through the server's OTLP
    /// gRPC collector with the client's exchanged bearer, `writers`
    /// exporters at a time, in [`Signal::request_rows`]-row requests, as an
    /// application's OpenTelemetry SDK batch exporters would.
    ///
    /// Each export resolves at the durable acknowledgement, so the rate is
    /// the durable row rate, and the encoded size of each acknowledged
    /// request adds to [`Measured::wire_bytes`]. A rejected export is counted
    /// by its gRPC code, and a partial success that drops rows as
    /// `partial-success`.
    ///
    /// # Errors
    ///
    /// Returns the connection or evidence failure.
    pub async fn export(
        &self,
        signal: Signal,
        rows: std::ops::Range<i64>,
        writers: usize,
    ) -> Result<Measured> {
        let channel = Channel::from_shared(self.client.grpc_url().to_owned())?
            .connect()
            .await?;
        let auth = self.client.auth();
        let wire_bytes = Arc::new(AtomicU64::new(0));
        let counted = Arc::clone(&wire_bytes);
        let mut measured = self
            .send_requests(
                rows,
                signal.request_rows(),
                writers,
                CancellationToken::new(),
                move |first, last| {
                    let (channel, auth, counted) =
                        (channel.clone(), Arc::clone(&auth), Arc::clone(&counted));
                    async move {
                        let bearer = auth.bearer().await.map_err(|_| "bearer".to_owned())?;
                        let token = format!("Bearer {}", bearer.expose());
                        let (encoded, rejected) = match signal {
                            Signal::Spans(fixture) => {
                                let body = fixture.request(first, last);
                                let encoded = body.encoded_len();
                                let response = TraceServiceClient::new(channel)
                                    .export(authorized(body, &token)?)
                                    .await
                                    .map_err(|status| format!("{:?}", status.code()))?;
                                let partial = response.into_inner().partial_success;
                                (encoded, partial.map_or(0, |partial| partial.rejected_spans))
                            }
                            Signal::Logs(fixture) => {
                                let body = fixture.request(first, last);
                                let encoded = body.encoded_len();
                                let response = LogsServiceClient::new(channel)
                                    .export(authorized(body, &token)?)
                                    .await
                                    .map_err(|status| format!("{:?}", status.code()))?;
                                let partial = response.into_inner().partial_success;
                                (
                                    encoded,
                                    partial.map_or(0, |partial| partial.rejected_log_records),
                                )
                            }
                        };
                        if rejected > 0 {
                            return Err("partial-success".to_owned());
                        }
                        counted.fetch_add(encoded as u64, Ordering::Relaxed);
                        Ok(())
                    }
                },
            )
            .await?;
        measured.wire_bytes = wire_bytes.load(Ordering::Relaxed);
        Ok(measured)
    }

    /// Exports rows `rows` of `signal` from `writers` exporters, capturing
    /// the window with `perf`.
    ///
    /// The OTLP counterpart of [`Self::write_step`]: it prints the same write
    /// Pareto, with the OTLP wire rate beside the retained Arrow rate.
    ///
    /// # Errors
    ///
    /// Returns connection, evidence, or capture failures.
    pub async fn export_step(
        &self,
        signal: Signal,
        rows: std::ops::Range<i64>,
        writers: usize,
    ) -> Result<Measured> {
        let capture = self.capture(signal.name(), writers)?;
        let measured = self.export(signal, rows, writers).await?;
        let metadata = serde_json::json!({
            "case": signal.name(),
            "clients": writers,
            "wire_bytes": measured.wire_bytes,
            "write": measured.usage.write,
            "profile_mean_ms": measured.usage.write_profile.pareto(),
        });
        let profile = capture.finish(&metadata)?;
        report_write_pareto(&measured, &profile);
        Ok(measured)
    }

    /// Sends `rows` in `chunk`-row requests from `writers` concurrent
    /// senders until every request is sent or `stop` is cancelled, and
    /// measures the window.
    ///
    /// The senders share one cursor, so requests go out in row order. `send`
    /// resolves at the durable acknowledgement of rows `first..last`; its
    /// error is the stable code it is counted by.
    ///
    /// # Errors
    ///
    /// Returns the window's evidence failure.
    async fn send_requests<F, Sent>(
        &self,
        rows: std::ops::Range<i64>,
        chunk: i64,
        writers: usize,
        stop: CancellationToken,
        send: F,
    ) -> Result<Measured>
    where
        F: Fn(i64, i64) -> Sent + Clone + Send + 'static,
        Sent: Future<Output = std::result::Result<(), String>> + Send,
    {
        let window = Window::open(self.server).await?;
        let next = Arc::new(AtomicI64::new(rows.start));
        let end = rows.end;
        let mut tasks = JoinSet::new();
        for _ in 0..writers {
            let (next, stop, send) = (Arc::clone(&next), stop.clone(), send.clone());
            tasks.spawn(async move {
                let mut measured = Measured::default();
                while !stop.is_cancelled() {
                    let first = next.fetch_add(chunk, Ordering::Relaxed);
                    if first >= end {
                        break;
                    }
                    let last = (first + chunk).min(end);
                    let sent = Instant::now();
                    match send(first, last).await {
                        Ok(()) => {
                            measured.record(sent);
                            measured.rows += (last - first) as u64;
                        }
                        Err(code) => *measured.errors.entry(code).or_default() += 1,
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
            "profile_mean_ms": measured.usage.write_profile.pareto(),
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
        let questions = cases.iter().flat_map(|case| {
            (0..case.variants()).map(|sequence| {
                let (sql, expected) = self.question(*case, sequence);
                (case.name(), sql, expected)
            })
        });
        self.check(questions.collect()).await
    }

    /// Asks each named question once, in order, and checks its exact answer.
    ///
    /// # Errors
    ///
    /// Returns evidence failures; a wrong answer is counted under its
    /// question's name, not returned.
    pub async fn check(&self, questions: Vec<(&'static str, String, Rows)>) -> Result<Measured> {
        let window = Window::open(self.server).await?;
        let mut measured = Measured {
            clients: 1,
            ..Measured::default()
        };
        for (name, sql, expected) in questions {
            let sent = Instant::now();
            match ask(&self.queries, sql, &expected).await {
                Ok(()) => measured.record(sent),
                Err(error) => {
                    *measured
                        .errors
                        .entry(format!("{name}: {error}"))
                        .or_default() += 1;
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
        // Operator times sum across partitions, so an event may exceed wall
        // time; the events are read beside the client latency, never summed.
        let latency = measured.usage.query_profile.pareto();
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
                        Ok(()) => measured.record(sent),
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
        measured.usage.query_profile.profiles
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

/// Prints one write window's per-request step times, throughput, and CPU
/// Pareto, or why the CPU capture is not usable evidence.
///
/// Step times are the Scribe write profile's mean per request. Its steps
/// are serial, so they sum to the request's time inside Scribe; the Gate's
/// write request contains them, and the client latency contains the Gate.
fn report_write_pareto(measured: &Measured, profile: &Profile) {
    let write = &measured.usage.write;
    let acks = measured.usage.write_profile.profiles.max(1.0);
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
    if measured.wire_bytes > 0 {
        let wire = measured.wire_bytes as f64 / f64::from(1 << 20);
        eprintln!(
            "  OTLP wire: {:.1} MiB/s, {:.1} MiB/s per server core, {:.0} B per row",
            wire / measured.usage.seconds.max(f64::EPSILON),
            wire / cores.max(f64::EPSILON),
            measured.wire_bytes as f64 / measured.rows.max(1) as f64,
        );
    }
    let gate_ms = write.get(GATE_WRITE).copied().unwrap_or(0.0) * 1e3 / acks;
    eprintln!(
        "  {gate_ms:8.3} ms {:5.1}%  {GATE_WRITE}, {acks:.0} profiles",
        100.0 * gate_ms / client_mean_ms.max(f64::EPSILON)
    );
    for (event, ms) in measured.usage.write_profile.pareto() {
        eprintln!(
            "  {ms:8.3} ms {:5.1}%  {event}",
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

/// The OTLP signal an export step sends: which fixture, collector service,
/// and Bifrost table the rows go through.
#[derive(Debug, Clone, Copy)]
pub enum Signal {
    /// Telemetry spans through the trace service into `vala.traces.spans`.
    Spans(Telemetry),
    /// Bulky Kubernetes logs through the logs service into
    /// `vala.logs.records`.
    Logs(Logs),
}

impl Signal {
    /// The step and profile label of this signal.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Spans(_) => "otlp spans",
            Self::Logs(_) => "otlp logs",
        }
    }

    /// Rows per export request: the SDK batch processors' export size.
    #[must_use]
    pub const fn request_rows(self) -> i64 {
        match self {
            Self::Spans(_) => telemetry::REQUEST_SPANS,
            Self::Logs(_) => logs::REQUEST_RECORDS,
        }
    }

    /// The output directory of this signal's `--writes` sweep.
    #[must_use]
    pub const fn mode(self) -> &'static str {
        match self {
            Self::Spans(_) => "telemetry-writes",
            Self::Logs(_) => "logs-writes",
        }
    }

    /// Rows in this signal's fixture.
    #[must_use]
    pub const fn rows(self) -> i64 {
        match self {
            Self::Spans(fixture) => fixture.spans,
            Self::Logs(fixture) => fixture.records,
        }
    }
}

/// Wraps `body` in a gRPC request carrying the `x-wyrd-access-token` bearer.
///
/// # Errors
///
/// Returns `bearer` when `token` is not a valid metadata value.
fn authorized<T>(body: T, token: &str) -> std::result::Result<Request<T>, String> {
    let mut request = Request::new(body);
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        token.parse().map_err(|_| "bearer".to_owned())?,
    );
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::{Measured, SCAN_COUNTERS, Usage};

    /// A window flags its scan sizing when sizing fell back or kept more
    /// than four times the row groups the scans read, and only then.
    ///
    /// # Panics
    ///
    /// Panics when a window is flagged or passed against the rule.
    #[test]
    fn sizing_divergence_flags_fallbacks_and_overkept_row_groups() {
        let window = |sized: f64, scanned: f64, fallbacks: f64| {
            let mut usage = Usage::default();
            for (counter, value) in SCAN_COUNTERS.iter().zip(&mut usage.scan_counts) {
                *value = match *counter {
                    "oracle_query_row_groups_sized_total" => sized,
                    "oracle_query_row_groups_scanned_total" => scanned,
                    "bifrost_scan_sizing_fallback_total" => fallbacks,
                    _ => 0.0,
                };
            }
            usage.sizing_divergence()
        };
        assert_eq!(window(0.0, 0.0, 0.0), None, "an idle window agrees");
        assert_eq!(window(8.0, 2.0, 0.0), None, "four times is within bounds");
        assert_eq!(window(4.0, 0.0, 0.0), None, "no scan allows four groups");
        assert!(
            window(9.0, 2.0, 0.0).is_some(),
            "over four times is flagged"
        );
        assert!(window(1.0, 1.0, 1.0).is_some(), "any fallback is flagged");
    }

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
