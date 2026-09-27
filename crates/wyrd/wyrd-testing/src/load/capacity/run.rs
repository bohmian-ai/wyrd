//! One single-pod query capacity benchmark: setup, warmup, measurement, drain.
//!
//! The pod is one containerized `All` child of [`BifrostProcessCluster`]; the
//! driver, the parent, and PostgreSQL run outside its limit. Every measured
//! query goes through `wyrd_client::Bifrost` over the pod's public listeners,
//! and every server figure comes from the pod's installed production recorder
//! and its own cgroup.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use arrow::array::{Array as _, Int64Array, RecordBatch};
use parquet::file::reader::{FileReader as _, SerializedFileReader};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use wyrd_client::bifrost::{BifrostClientError, TableConfig};
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_client::{Bifrost, WyrdClient};
use wyrd_spec::vala::api::{
    BifrostQueryRequest, QueryExecutionPath, QueryTerminalErrorCode, QueryTerminalOutcome,
};

use super::schedule::{FixedRateDriver, FixedRateRun, ProbeResult, ShortQueryOutcome};
use super::workload;
use crate::bifrost::process_cluster::{
    BenchmarkContainer, BifrostProcessCluster, CGROUP_EVIDENCE_FILES, ProcessClusterError,
    ProcessNode,
};

/// Offered short-query rates, in queries per second.
pub const OFFERED_RATES: [u64; 2] = [500, 1_000];

/// Fractions of the resolved Interactive slot units held by live streams.
pub const LIVE_OCCUPANCY: [f64; 4] = [0.0, 0.25, 0.5, 1.0];

/// How long each live stream is kept admitted before it is drained.
const LIVE_HOLD: Duration = Duration::from_secs(5);

/// Leader deadline every live stream carries; longer than [`LIVE_HOLD`].
const LIVE_DEADLINE_MS: i64 = 30_000;

/// Most short queries the driver keeps in flight; arrivals beyond it are
/// counted as missed launches.
const MAX_IN_FLIGHT: usize = 1_024;

/// How long the driver waits for outstanding short queries after a window.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

/// Largest scheduling lag at which the driver still counts as offering its
/// configured rate.
const MAX_DRIVER_LAG: Duration = Duration::from_millis(100);

/// Interval between gauge samples during a measured window.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

/// How long held streams have to all open before a mixed run is refused.
const LIVE_OPEN_TIMEOUT: Duration = Duration::from_secs(30);

/// Largest gRPC message the seeding client sends or accepts.
const CLIENT_MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;

/// Counter families whose measured-window delta every report row carries.
const COUNTER_FAMILIES: [&str; 6] = [
    "oracle_query_files_scanned_total",
    "oracle_query_row_groups_scanned_total",
    "oracle_query_row_groups_pruned_total",
    "oracle_query_rows_total",
    "bifrost_gate_query_streams_total",
    "bifrost_scribe_rows_total",
];

/// Failures that stop the benchmark rather than become a report row.
#[derive(Debug, thiserror::Error)]
pub enum CapacityError {
    /// The process cluster or its control protocol failed.
    #[error(transparent)]
    Cluster(#[from] ProcessClusterError),
    /// The public client refused a setup or preflight call.
    #[error(transparent)]
    Client(#[from] BifrostClientError),
    /// The public client could not be configured.
    #[error(transparent)]
    ClientConfig(#[from] wyrd_client::error::WyrdClientError),
    /// A fixture batch could not be built.
    #[error(transparent)]
    Arrow(#[from] arrow::error::ArrowError),
    /// A report or raw sample could not be written.
    #[error("benchmark output: {0}")]
    Output(String),
    /// Setup produced data the fixed workload does not describe.
    #[error("benchmark preflight: {0}")]
    Preflight(String),
}

/// Operator-chosen run settings; everything else is fixed by the workload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkSettings {
    /// Directory the report, raw samples, and raw server snapshots land in.
    pub output: PathBuf,
    /// Unmeasured warmup before each measured window.
    pub warmup: Duration,
    /// Length of each measured window.
    pub measurement: Duration,
}

impl BenchmarkSettings {
    /// Reads `WYRD_BENCH_OUTPUT_DIR`, `WYRD_BENCH_WARMUP_SECONDS` (default 60),
    /// and `WYRD_BENCH_MEASURE_SECONDS` (default 300).
    ///
    /// The durations exist so one short smoke interval can check the raw
    /// samples against the report before a dedicated run; the benchmark itself
    /// uses the defaults.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityError::Output`] when a duration is not whole seconds.
    pub fn from_env() -> Result<Self, CapacityError> {
        let seconds = |name: &str, default: u64| match std::env::var(name) {
            Ok(value) => value
                .parse()
                .map(Duration::from_secs)
                .map_err(|error| CapacityError::Output(format!("{name}: {error}"))),
            Err(_) => Ok(Duration::from_secs(default)),
        };
        Ok(Self {
            output: std::env::var_os("WYRD_BENCH_OUTPUT_DIR").map_or_else(
                || PathBuf::from("target/bifrost-query-capacity"),
                PathBuf::from,
            ),
            warmup: seconds("WYRD_BENCH_WARMUP_SECONDS", 60)?,
            measurement: seconds("WYRD_BENCH_MEASURE_SECONDS", 300)?,
        })
    }
}

/// Environment, fixture, and statements recorded with every report.
#[derive(Debug, Clone, Serialize)]
pub struct RunMetadata {
    /// `git rev-parse HEAD` of the working tree, or `unknown`.
    pub server_commit: String,
    /// Child binary the pod ran.
    pub server_binary: PathBuf,
    /// SHA-256 of that binary.
    pub server_binary_sha256: String,
    /// CPUs visible to the driver host.
    pub host_cpus: usize,
    /// `MemTotal` of the driver host, in bytes.
    pub host_memory_bytes: u64,
    /// Pod container `cpu.max`.
    pub container_cpu_max: Option<String>,
    /// Pod container `memory.max`.
    pub container_memory_max: Option<String>,
    /// Resolved Oracle slot units reported by the pod's recorder.
    pub oracle_slot_limit: f64,
    /// Resolved Interactive floor slot units reported by the pod's recorder.
    pub oracle_interactive_floor: f64,
    /// Where the driver and PostgreSQL ran relative to the pod.
    pub placement: String,
    /// Table schema as written.
    pub schema: String,
    /// Published and live row counts.
    pub published_rows: i64,
    /// Acknowledged, unflushed rows.
    pub live_rows: i64,
    /// Published Parquet objects, row groups, and bytes after setup.
    pub data_shape: DataShape,
    /// The eight short statements followed by the live statement.
    pub sql: Vec<String>,
    /// Expected result digest of each statement, in the same order.
    pub expected_digests: Vec<String>,
    /// Warmup length, in seconds.
    pub warmup_seconds: u64,
    /// Measured window length, in seconds.
    pub measurement_seconds: u64,
}

/// Published object layout of the benchmark table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct DataShape {
    /// Parquet objects under the table's storage prefix.
    pub files: u64,
    /// Row groups across those objects.
    pub row_groups: u64,
    /// Bytes across those objects.
    pub bytes: u64,
}

/// What the held live streams did over one combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct LiveStreamTally {
    /// Streams that held for the full interval and returned every live ID.
    pub completed: u64,
    /// Streams refused at admission.
    pub refused: u64,
    /// Streams that ran on the Analytical path.
    pub not_interactive: u64,
    /// Streams with any other terminal or a wrong ID set.
    pub failed: u64,
}

/// One offered-rate / live-occupancy report row.
#[derive(Debug, Clone, Serialize)]
pub struct CombinationReport {
    /// Offered short queries per second.
    pub offered_rate: u64,
    /// Fraction of Interactive slot units meant to be held by live streams.
    pub live_occupancy: f64,
    /// Live streams the occupancy asked for.
    pub target_live_streams: usize,
    /// Smallest and largest open live-stream count sampled in the window.
    pub live_streams_sampled: [usize; 2],
    /// Smallest and largest used Interactive slot units sampled in the window.
    pub interactive_units_sampled: [f64; 2],
    /// Arrivals scheduled.
    pub scheduled: u64,
    /// Arrivals sent.
    pub sent: u64,
    /// Arrivals dropped at the driver's in-flight bound.
    pub missed_launches: u64,
    /// Sent queries with no terminal after drain.
    pub abandoned: u64,
    /// Terminals that arrived during drain.
    pub drain_completions: u64,
    /// Measured-window terminals per category.
    pub outcomes: BTreeMap<ShortQueryOutcome, u64>,
    /// Measured-window `Success` terminals per second.
    pub successes_per_second: f64,
    /// p50/p95/p99 send-to-first-row of measured successes, microseconds.
    pub first_row_us: [Option<u64>; 3],
    /// p50/p95/p99 send-to-terminal of measured successes, microseconds.
    pub terminal_us: [Option<u64>; 3],
    /// Largest scheduled-to-launch lag, microseconds.
    pub max_driver_lag_us: u64,
    /// Driver process CPU consumed during the window, seconds.
    pub driver_cpu_seconds: f64,
    /// Server counter deltas across the window.
    pub server_deltas: BTreeMap<String, f64>,
    /// Pod cgroup deltas and levels across the window.
    pub container: BTreeMap<String, f64>,
    /// PostgreSQL `SELECT 1` latency before and after the window, ms.
    pub postgres_select_ms: [f64; 2],
    /// Held live-stream outcomes.
    pub live: LiveStreamTally,
    /// Whether the offered rate was sent, drained, and succeeded in full.
    pub sustained: bool,
    /// First boundary the evidence points at when not sustained.
    pub saturated_boundary: &'static str,
    /// Conditions that make this row not a valid measurement.
    pub invalid: Vec<String>,
}

/// Counter, gauge, cgroup, driver, and database readings at one instant.
#[derive(Debug, Clone, Default)]
struct Snapshot {
    /// Counter totals, including the labelled admission series.
    counters: BTreeMap<String, f64>,
    /// Pod cgroup readings by `file:key`.
    container: BTreeMap<String, f64>,
    /// Driver process CPU seconds.
    driver_cpu_seconds: f64,
    /// PostgreSQL `SELECT 1` round trip, milliseconds.
    postgres_select_ms: f64,
}

/// A running single-pod benchmark and everything it seeded.
pub struct QueryCapacityBenchmark {
    /// The one containerized pod and its shared resources.
    cluster: BifrostProcessCluster,
    /// Public query client every measured and live query uses.
    queries: Arc<Bifrost>,
    /// Operator settings.
    settings: BenchmarkSettings,
    /// Recorded environment, fixture, and statements.
    metadata: RunMetadata,
}

impl QueryCapacityBenchmark {
    /// Setup: launches the verified pod, seeds the fixture, and preflights
    /// every statement.
    ///
    /// Writes IDs `0..1,048,576` in eight 131,072-ID batches through the public
    /// ingest client, flushing and publishing each, then acknowledges IDs
    /// `1,048,576..1,081,344` without flushing. Each short statement must then
    /// return its exact 20 IDs and the live statement all 32,768 live IDs
    /// across more than one batch, all with terminal `Success`.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityError::Cluster`] when the pod cannot be launched or is
    /// not limited to its envelope, [`CapacityError::Client`] when
    /// registration, ingest, or a preflight query fails, and
    /// [`CapacityError::Preflight`] when a result differs from the fixture.
    pub async fn setup(binary: &Path, settings: BenchmarkSettings) -> Result<Self, CapacityError> {
        let mut cluster =
            BifrostProcessCluster::start_benchmark(binary, BenchmarkContainer::from_env()).await?;
        let api_key = cluster
            .provision_foreign_public_api_key("query-capacity")
            .await?;
        let client = public_client(pod(&mut cluster)?, &api_key)?;
        let writer = Bifrost::connect_with_table(
            &client,
            TableConfig::from_arrow(workload::TABLE, workload::schema())?,
        )
        .await?;
        writer.register().await?;
        for batch in 0..workload::PUBLISHED_ROWS / workload::PUBLISHED_BATCH_ROWS {
            let first = batch * workload::PUBLISHED_BATCH_ROWS;
            write_ids(&writer, first, first + workload::PUBLISHED_BATCH_ROWS).await?;
            let node = pod(&mut cluster)?;
            tokio::task::block_in_place(|| node.flush().and_then(|()| node.refresh_snapshot()))?;
        }
        write_ids(&writer, workload::LIVE_START, workload::LIVE_END).await?;

        let queries = Arc::new(Bifrost::query_only(&client));
        let mut sql = Vec::new();
        let mut expected_digests = Vec::new();
        for bucket in 0..workload::SHORT_BUCKETS {
            let expected = workload::short_expected(bucket);
            let (ids, _) = preflight(&queries, &workload::short_sql(bucket)).await?;
            if ids != expected {
                return Err(CapacityError::Preflight(format!(
                    "bucket {bucket} returned {} IDs, not its 20 expected IDs",
                    ids.len()
                )));
            }
            sql.push(workload::short_sql(bucket));
            expected_digests.push(workload::id_digest(&expected));
        }
        let (mut live, batches) = preflight(&queries, &workload::live_sql()).await?;
        live.sort_unstable();
        let expected: Vec<i64> = (workload::LIVE_START..workload::LIVE_END).collect();
        if live != expected || batches < 2 {
            return Err(CapacityError::Preflight(format!(
                "the live statement returned {} IDs across {batches} batches",
                live.len()
            )));
        }
        sql.push(workload::live_sql());
        expected_digests.push(workload::id_digest(&expected));

        let metadata =
            RunMetadata::collect(&mut cluster, binary, &settings, sql, expected_digests)?;
        Ok(Self {
            cluster,
            queries,
            settings,
            metadata,
        })
    }

    /// Runs every offered-rate / live-occupancy combination and writes the
    /// report.
    ///
    /// # Errors
    ///
    /// Returns the first cluster, client, or output failure; an invalid
    /// measurement is a report row, not an error.
    pub async fn run(mut self) -> Result<Vec<CombinationReport>, CapacityError> {
        write_json(&self.settings.output.join("metadata.json"), &self.metadata)?;
        let mut rows = Vec::new();
        for rate in OFFERED_RATES {
            for occupancy in LIVE_OCCUPANCY {
                let row = self.measure(rate, occupancy).await?;
                println!("{}", render_row(&row));
                rows.push(row);
            }
        }
        write_json(&self.settings.output.join("report.json"), &rows)?;
        let table = render_table(&self.metadata, &rows);
        std::fs::write(self.settings.output.join("report.txt"), &table)
            .map_err(|error| CapacityError::Output(error.to_string()))?;
        println!("{table}");
        self.cluster.shutdown()?;
        Ok(rows)
    }

    /// Warmup, measurement, and drain for one combination.
    ///
    /// Held streams start and are verified first, so the measured window
    /// begins with the intended occupancy already in place.
    ///
    /// # Errors
    ///
    /// Returns cluster or output failures.
    async fn measure(
        &mut self,
        rate: u64,
        occupancy: f64,
    ) -> Result<CombinationReport, CapacityError> {
        let label = format!("{rate}qps-{:03}pct", (occupancy * 100.0).round() as u64);
        let raw = self.settings.output.join(&label);
        std::fs::create_dir_all(&raw).map_err(|error| CapacityError::Output(error.to_string()))?;
        let mut invalid = Vec::new();
        let target = (occupancy * self.metadata.oracle_slot_limit).round() as usize;
        let live = if target == 0 {
            None
        } else {
            Some(self.hold_live_streams(target, &mut invalid).await?)
        };

        self.warmup(rate).await;

        // Measurement.
        let before = self.snapshot(&raw.join("before"))?;
        let queries = Arc::clone(&self.queries);
        let window = self.settings.measurement;
        let window_end = Instant::now() + window;
        let driver = tokio::spawn(async move {
            FixedRateDriver::new(rate, MAX_IN_FLIGHT)
                .run(window, DRAIN_TIMEOUT, move |sequence| {
                    short_query(Arc::clone(&queries), sequence % workload::SHORT_BUCKETS)
                })
                .await
        });
        let mut streams = [usize::MAX, 0];
        let mut units = [f64::MAX, 0.0_f64];
        while Instant::now() + SAMPLE_INTERVAL < window_end {
            tokio::time::sleep(SAMPLE_INTERVAL).await;
            let open = live.as_ref().map_or(0, LiveStreams::open);
            streams = [streams[0].min(open), streams[1].max(open)];
            let used = interactive_units(pod(&mut self.cluster)?)?;
            units = [units[0].min(used), units[1].max(used)];
        }
        tokio::time::sleep_until(window_end).await;
        let after = self.snapshot(&raw.join("after"))?;

        // Drain.
        let run = driver
            .await
            .map_err(|error| CapacityError::Output(format!("driver task: {error}")))?;
        let live = match live {
            Some(live) => live.stop().await,
            None => LiveStreamTally::default(),
        };
        write_samples(&raw.join("samples.jsonl"), &run)?;
        if live.refused + live.not_interactive + live.failed > 0 {
            invalid.push(format!("held live streams did not all complete: {live:?}"));
        }
        if run.max_launch_lag > MAX_DRIVER_LAG {
            invalid.push(format!(
                "the driver fell {:?} behind its schedule and could not offer {rate}/s",
                run.max_launch_lag
            ));
        }
        Ok(MeasuredWindow {
            rate,
            occupancy,
            target,
            streams,
            units,
            run,
            before,
            after,
            live,
            invalid,
        }
        .report())
    }

    /// Warmup: offers `rate` for the warmup interval and discards the samples.
    async fn warmup(&self, rate: u64) {
        let queries = Arc::clone(&self.queries);
        FixedRateDriver::new(rate, MAX_IN_FLIGHT)
            .run(self.settings.warmup, DRAIN_TIMEOUT, move |sequence| {
                short_query(Arc::clone(&queries), sequence % workload::SHORT_BUCKETS)
            })
            .await;
    }

    /// Starts `target` held live streams and verifies they occupy exactly
    /// `target` Interactive slot units for the hold interval.
    ///
    /// Each stream must be admitted as Interactive — an Analytical admission
    /// or a used-unit increase other than `target` invalidates the run — and
    /// the occupancy must still be `target` just before the first stream's hold
    /// ends. Violations are pushed onto `invalid` rather than returned, so the
    /// combination is reported as a failed workload condition.
    ///
    /// # Errors
    ///
    /// Returns cluster control failures.
    async fn hold_live_streams(
        &mut self,
        target: usize,
        invalid: &mut Vec<String>,
    ) -> Result<LiveStreams, CapacityError> {
        let node = pod(&mut self.cluster)?;
        let admitted =
            |node: &mut ProcessNode, class: &str| admission_total(node, class, "admitted");
        let interactive_before = tokio::task::block_in_place(|| admitted(node, "interactive"))?;
        let analytical_before = tokio::task::block_in_place(|| admitted(node, "analytical"))?;
        let units_before = interactive_units(node)?;
        let started = Instant::now();
        let live = LiveStreams::start(Arc::clone(&self.queries), target);
        while live.open() < target && started.elapsed() < LIVE_OPEN_TIMEOUT {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let node = pod(&mut self.cluster)?;
        let interactive =
            tokio::task::block_in_place(|| admitted(node, "interactive"))? - interactive_before;
        let analytical =
            tokio::task::block_in_place(|| admitted(node, "analytical"))? - analytical_before;
        let held = interactive_units(node)? - units_before;
        let expected = target as f64;
        if live.open() < target {
            invalid.push(format!(
                "only {} of {target} live streams opened",
                live.open()
            ));
        }
        if analytical > 0.0 {
            invalid.push(format!(
                "{analytical} live streams were admitted as Analytical"
            ));
        }
        if interactive < expected || (held - expected).abs() > f64::EPSILON {
            invalid.push(format!(
                "{target} held streams added {interactive} Interactive admissions and {held} used units"
            ));
        }
        let verify_at = started + LIVE_HOLD - Duration::from_millis(500);
        if Instant::now() >= verify_at {
            invalid.push("live streams took longer than their hold to open".to_owned());
        } else {
            tokio::time::sleep_until(verify_at).await;
            let still = interactive_units(pod(&mut self.cluster)?)? - units_before;
            if (still - expected).abs() > f64::EPSILON {
                invalid.push(format!(
                    "held streams occupied {still} units near the end of their hold, not {target}"
                ));
            }
        }
        Ok(live)
    }

    /// Reads counters, cgroup files, driver CPU, and PostgreSQL latency, and
    /// keeps the pod's raw exposition and cgroup files under `raw`.
    ///
    /// # Errors
    ///
    /// Returns cluster control or output failures.
    fn snapshot(&mut self, raw: &Path) -> Result<Snapshot, CapacityError> {
        let started = std::time::Instant::now();
        let postgres = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(sqlx::query("SELECT 1").execute(self.cluster.fixture().app_pool()))
        });
        let postgres_select_ms = match postgres {
            Ok(_) => started.elapsed().as_secs_f64() * 1_000.0,
            Err(_) => f64::NAN,
        };
        let node = pod(&mut self.cluster)?;
        let captured = node.root().join("evidence").join(
            raw.file_name()
                .map_or_else(|| "snapshot".into(), std::ffi::OsStr::to_os_string),
        );
        std::fs::create_dir_all(&captured)
            .map_err(|error| CapacityError::Output(error.to_string()))?;
        tokio::task::block_in_place(|| node.capture_resource_evidence(&captured))?;
        std::fs::create_dir_all(raw).map_err(|error| CapacityError::Output(error.to_string()))?;
        let mut container = BTreeMap::new();
        for file in CGROUP_EVIDENCE_FILES
            .iter()
            .copied()
            .chain(["metrics.prom"])
        {
            let Ok(contents) = std::fs::read_to_string(captured.join(file)) else {
                continue;
            };
            std::fs::write(raw.join(file), &contents)
                .map_err(|error| CapacityError::Output(error.to_string()))?;
            if !matches!(file, "metrics.prom" | "cpu.max") {
                container.extend(cgroup_values(file, &contents));
            }
        }
        let mut counters = tokio::task::block_in_place(|| node.metric_totals(&COUNTER_FAMILIES))?;
        for class in ["interactive", "analytical"] {
            for outcome in ["admitted", "rejected"] {
                let total = tokio::task::block_in_place(|| admission_total(node, class, outcome))?;
                counters.insert(
                    format!("oracle_admission_total{{class={class},outcome={outcome}}}"),
                    total,
                );
            }
        }
        Ok(Snapshot {
            counters,
            container,
            driver_cpu_seconds: driver_cpu_seconds(),
            postgres_select_ms,
        })
    }
}

impl RunMetadata {
    /// Records the environment, the pod's limits and slot plan, and the
    /// published data shape after setup.
    ///
    /// # Errors
    ///
    /// Returns cluster control failures and binary read failures.
    fn collect(
        cluster: &mut BifrostProcessCluster,
        binary: &Path,
        settings: &BenchmarkSettings,
        sql: Vec<String>,
        expected_digests: Vec<String>,
    ) -> Result<Self, CapacityError> {
        let binary_bytes =
            std::fs::read(binary).map_err(|error| CapacityError::Output(error.to_string()))?;
        let data_shape = data_shape(cluster.storage_root());
        let node = pod(cluster)?;
        let directory = node.root().join("evidence").join("metadata");
        std::fs::create_dir_all(&directory)
            .map_err(|error| CapacityError::Output(error.to_string()))?;
        let evidence = tokio::task::block_in_place(|| node.capture_resource_evidence(&directory))?;
        let oracle_slot_limit = tokio::task::block_in_place(|| slot_units(node, "limit"))?;
        let oracle_interactive_floor =
            tokio::task::block_in_place(|| slot_units(node, "interactive_floor"))?;
        let postgres = std::env::var("WYRD_DATABASE_URL")
            .ok()
            .and_then(|url| url::Url::parse(&url).ok())
            .map_or_else(
                || "unknown".to_owned(),
                |url| {
                    format!(
                        "{}:{}",
                        url.host_str().unwrap_or("unknown"),
                        url.port().unwrap_or(5432)
                    )
                },
            );
        Ok(Self {
            server_commit: std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map_or_else(
                    || "unknown".to_owned(),
                    |output| String::from_utf8_lossy(&output.stdout).trim().to_owned(),
                ),
            server_binary: binary.to_path_buf(),
            server_binary_sha256: hex::encode(Sha256::digest(&binary_bytes)),
            host_cpus: std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
            host_memory_bytes: host_memory_bytes(),
            container_cpu_max: evidence.cpu_max,
            container_memory_max: evidence.memory_max,
            oracle_slot_limit,
            oracle_interactive_floor,
            placement: format!(
                "driver and process-cluster parent on the host outside the pod container; \
                 PostgreSQL at {postgres} outside the pod container"
            ),
            schema: format!("{:?}", workload::schema()),
            published_rows: workload::PUBLISHED_ROWS,
            live_rows: workload::LIVE_END - workload::LIVE_START,
            data_shape,
            sql,
            expected_digests,
            warmup_seconds: settings.warmup.as_secs(),
            measurement_seconds: settings.measurement.as_secs(),
        })
    }
}

/// Everything one combination's window produced, before it becomes a row.
struct MeasuredWindow {
    /// Offered short queries per second.
    rate: u64,
    /// Intended live occupancy fraction.
    occupancy: f64,
    /// Live streams the occupancy asked for.
    target: usize,
    /// Smallest and largest open live-stream count sampled.
    streams: [usize; 2],
    /// Smallest and largest used Interactive slot units sampled.
    units: [f64; 2],
    /// The driver's client run.
    run: FixedRateRun,
    /// Server readings at the window start.
    before: Snapshot,
    /// Server readings at the window end.
    after: Snapshot,
    /// Held live-stream outcomes.
    live: LiveStreamTally,
    /// Invalidating conditions found so far.
    invalid: Vec<String>,
}

impl MeasuredWindow {
    /// Assembles the report row: category counts and latency percentiles from
    /// the client run, counter and cumulative cgroup deltas between the
    /// snapshots, and whether the offered rate was sustained.
    fn report(self) -> CombinationReport {
        let Self {
            rate,
            occupancy,
            target,
            streams,
            units,
            run,
            before,
            after,
            live,
            invalid,
        } = self;
        let outcomes = ShortQueryOutcome::ALL
            .into_iter()
            .map(|outcome| (outcome, run.measured(outcome)))
            .collect::<BTreeMap<_, _>>();
        let server_deltas = after
            .counters
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    value - before.counters.get(name).copied().unwrap_or(0.0),
                )
            })
            .collect();
        let mut container = BTreeMap::new();
        for (key, value) in &after.container {
            let cumulative = key.starts_with("cpu.stat:") || key.starts_with("memory.events:");
            let reported = if cumulative {
                value - before.container.get(key).copied().unwrap_or(0.0)
            } else {
                *value
            };
            container.insert(key.clone(), reported);
        }
        let sustained = invalid.is_empty()
            && run.missed_launches == 0
            && run.abandoned == 0
            && run.successes_per_second() >= 0.99 * rate as f64;
        let saturated_boundary = if sustained {
            "none"
        } else if run.max_launch_lag > MAX_DRIVER_LAG {
            "driver"
        } else if container
            .get("memory.events:oom_kill")
            .copied()
            .unwrap_or(0.0)
            > 0.0
        {
            "container memory"
        } else if container
            .get("cpu.stat:nr_throttled")
            .copied()
            .unwrap_or(0.0)
            > 0.0
        {
            "container cpu"
        } else if outcomes[&ShortQueryOutcome::AdmissionRefused] > 0 {
            "oracle admission"
        } else if after.postgres_select_ms > 100.0 {
            "postgres"
        } else {
            "query latency"
        };
        CombinationReport {
            offered_rate: rate,
            live_occupancy: occupancy,
            target_live_streams: target,
            live_streams_sampled: if streams[0] == usize::MAX {
                [0, 0]
            } else {
                streams
            },
            interactive_units_sampled: if units[0] == f64::MAX {
                [0.0, 0.0]
            } else {
                units
            },
            scheduled: run.scheduled,
            sent: run.sent,
            missed_launches: run.missed_launches,
            abandoned: run.abandoned,
            drain_completions: run.drain_completions(),
            outcomes,
            successes_per_second: run.successes_per_second(),
            first_row_us: run.success_percentiles(|sample| sample.first_row_latency_us),
            terminal_us: run.success_percentiles(|sample| Some(sample.terminal_latency_us)),
            max_driver_lag_us: u64::try_from(run.max_launch_lag.as_micros()).unwrap_or(u64::MAX),
            driver_cpu_seconds: after.driver_cpu_seconds - before.driver_cpu_seconds,
            server_deltas,
            container,
            postgres_select_ms: [before.postgres_select_ms, after.postgres_select_ms],
            live,
            sustained,
            saturated_boundary,
            invalid,
        }
    }
}

/// Live streams held open for [`LIVE_HOLD`] each and replaced only after a
/// terminal, until stopped.
struct LiveStreams {
    /// Stops every holder after its current stream.
    cancel: CancellationToken,
    /// Streams currently past their first batch and not yet drained.
    open: Arc<AtomicUsize>,
    /// One holder task per stream slot.
    holders: JoinSet<LiveStreamTally>,
}

impl LiveStreams {
    /// Starts `streams` holders, each issuing the live statement in a loop.
    ///
    /// A holder reads the first batch, counts itself open, waits
    /// [`LIVE_HOLD`] without reading so the stream stays admitted, then drains
    /// the rest, counts itself closed, and checks the terminal is an
    /// Interactive `Success` with every live ID. Only then does it issue the
    /// next stream. A refused stream is retried after a short pause.
    fn start(queries: Arc<Bifrost>, streams: usize) -> Self {
        let stop = CancellationToken::new();
        let open = Arc::new(AtomicUsize::new(0));
        let mut holders = JoinSet::new();
        for _ in 0..streams {
            let (queries, stop, open) = (Arc::clone(&queries), stop.clone(), Arc::clone(&open));
            holders.spawn(async move {
                let mut tally = LiveStreamTally::default();
                while !stop.is_cancelled() {
                    hold_one(&queries, &open, &mut tally).await;
                }
                tally
            });
        }
        Self {
            cancel: stop,
            open,
            holders,
        }
    }

    /// Streams currently held open.
    fn open(&self) -> usize {
        self.open.load(Ordering::Acquire)
    }

    /// Stops every holder after its current stream and sums their tallies.
    async fn stop(mut self) -> LiveStreamTally {
        self.cancel.cancel();
        let mut total = LiveStreamTally::default();
        while let Some(joined) = self.holders.join_next().await {
            let tally = joined.unwrap_or(LiveStreamTally {
                failed: 1,
                ..LiveStreamTally::default()
            });
            total.completed += tally.completed;
            total.refused += tally.refused;
            total.not_interactive += tally.not_interactive;
            total.failed += tally.failed;
        }
        total
    }
}

/// Issues one live stream, holds it, drains it, and records its outcome.
async fn hold_one(queries: &Bifrost, open: &AtomicUsize, tally: &mut LiveStreamTally) {
    let request = BifrostQueryRequest {
        sql: workload::live_sql(),
        deadline_ms: Some(LIVE_DEADLINE_MS),
    };
    let mut stream = match queries.query(&request).await {
        Ok(stream) => stream,
        Err(error) => {
            if classify_error(&error) == ShortQueryOutcome::AdmissionRefused {
                tally.refused += 1;
                tokio::time::sleep(Duration::from_millis(100)).await;
            } else {
                tally.failed += 1;
            }
            return;
        }
    };
    let mut ids = Vec::new();
    let mut counted = false;
    let drained = loop {
        match stream.next_batch().await {
            Ok(Some(batch)) => {
                ids.extend(batch_ids(&batch));
                if !counted {
                    counted = true;
                    open.fetch_add(1, Ordering::AcqRel);
                    tokio::time::sleep(LIVE_HOLD).await;
                }
            }
            Ok(None) => break true,
            Err(_) => break false,
        }
    };
    if counted {
        open.fetch_sub(1, Ordering::AcqRel);
    }
    ids.sort_unstable();
    match stream.terminal() {
        Some(terminal) if terminal.execution_path != QueryExecutionPath::Interactive => {
            tally.not_interactive += 1;
        }
        Some(terminal)
            if drained
                && terminal.outcome == QueryTerminalOutcome::Success
                && ids
                    .iter()
                    .copied()
                    .eq(workload::LIVE_START..workload::LIVE_END) =>
        {
            tally.completed += 1;
        }
        _ => tally.failed += 1,
    }
}

/// Issues one short read for `bucket` and classifies its terminal.
///
/// Latency starts where the scheduler polls this future, immediately before
/// the request is sent; the first-row instant is when the first non-empty
/// batch arrives.
async fn short_query(queries: Arc<Bifrost>, bucket: u64) -> ProbeResult {
    let request = BifrostQueryRequest {
        sql: workload::short_sql(bucket),
        deadline_ms: None,
    };
    let mut stream = match queries.query(&request).await {
        Ok(stream) => stream,
        Err(error) => {
            return ProbeResult {
                outcome: classify_error(&error),
                first_row_at: None,
            };
        }
    };
    let mut ids = Vec::new();
    let mut first_row_at = None;
    loop {
        match stream.next_batch().await {
            Ok(Some(batch)) => {
                if batch.num_rows() > 0 {
                    first_row_at.get_or_insert_with(Instant::now);
                }
                ids.extend(batch_ids(&batch));
            }
            Ok(None) => break,
            Err(error) => {
                return ProbeResult {
                    outcome: classify_error(&error),
                    first_row_at,
                };
            }
        }
    }
    let outcome = match stream.terminal().map(|terminal| terminal.outcome) {
        Some(QueryTerminalOutcome::Success) if ids == workload::short_expected(bucket) => {
            ShortQueryOutcome::Success
        }
        Some(QueryTerminalOutcome::Success) => ShortQueryOutcome::WrongResult,
        Some(QueryTerminalOutcome::Degraded) => ShortQueryOutcome::Degraded,
        Some(QueryTerminalOutcome::Failed) => ShortQueryOutcome::Failed,
        None => ShortQueryOutcome::TransportError,
    };
    ProbeResult {
        outcome,
        first_row_at,
    }
}

/// Maps a client error onto its report category by its stable code.
fn classify_error(error: &BifrostClientError) -> ShortQueryOutcome {
    if let Some(terminal) = error.terminal() {
        return match terminal.error.as_ref().map(|error| error.code) {
            Some(QueryTerminalErrorCode::QueryTimeout) => ShortQueryOutcome::Deadline,
            _ => ShortQueryOutcome::Failed,
        };
    }
    let code = wyrd_spec::error::WyrdError::from(error).code();
    if code == "WYRD_VALA_429_QUERY_ADMISSION_REJECTED" {
        ShortQueryOutcome::AdmissionRefused
    } else if code == "WYRD_VALA_504_QUERY_TIMEOUT" {
        ShortQueryOutcome::Deadline
    } else {
        ShortQueryOutcome::TransportError
    }
}

/// Returns the `id` column of one result batch.
fn batch_ids(batch: &RecordBatch) -> Vec<i64> {
    batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .map(|ids| ids.iter().flatten().collect())
        .unwrap_or_default()
}

/// Runs one statement to completion and requires terminal `Success`.
///
/// Returns the IDs in returned order and the number of non-empty batches.
///
/// # Errors
///
/// Returns the client error, or [`CapacityError::Preflight`] for any other
/// terminal.
async fn preflight(queries: &Bifrost, sql: &str) -> Result<(Vec<i64>, usize), CapacityError> {
    let mut stream = queries
        .query(&BifrostQueryRequest {
            sql: sql.to_owned(),
            deadline_ms: Some(LIVE_DEADLINE_MS),
        })
        .await?;
    let mut ids = Vec::new();
    let mut batches = 0;
    while let Some(batch) = stream.next_batch().await? {
        if batch.num_rows() > 0 {
            batches += 1;
        }
        ids.extend(batch_ids(&batch));
    }
    match stream.terminal().map(|terminal| terminal.outcome) {
        Some(QueryTerminalOutcome::Success) => Ok((ids, batches)),
        other => Err(CapacityError::Preflight(format!(
            "`{sql}` ended with {other:?}"
        ))),
    }
}

/// Writes IDs `start..end` through the public ingest client in bounded requests.
///
/// # Errors
///
/// Returns the fixture or client error.
async fn write_ids(writer: &Bifrost, start: i64, end: i64) -> Result<(), CapacityError> {
    let mut first = start;
    while first < end {
        let last = (first + workload::INGEST_REQUEST_ROWS).min(end);
        writer
            .write_batch(workload::TABLE, &workload::rows(first, last)?)
            .await?;
        first = last;
    }
    Ok(())
}

/// Builds the public client for the benchmark tenant against `node`.
///
/// # Errors
///
/// Returns the client configuration error.
fn public_client(
    node: &ProcessNode,
    api_key: &secrecy::SecretString,
) -> Result<WyrdClient, CapacityError> {
    Ok(WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: format!("http://{}", node.grpc_addr()),
            connect_retries: 0,
            max_message_bytes: CLIENT_MAX_MESSAGE_BYTES,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: format!("http://{}", node.http_addr()),
            ..HttpConfig::default()
        },
        credential: Some(api_key.clone()),
        ..ClientConfig::default()
    })?)
}

/// Returns the benchmark's one pod.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Resource`] when the cluster has no pod.
fn pod(cluster: &mut BifrostProcessCluster) -> Result<&mut ProcessNode, ProcessClusterError> {
    cluster
        .nodes_mut()
        .first_mut()
        .ok_or_else(|| ProcessClusterError::Resource("the benchmark has no pod".to_owned()))
}

/// Reads one `bifrost_oracle_local_slot_units` gauge.
///
/// # Errors
///
/// Returns cluster control failures.
fn slot_units(node: &mut ProcessNode, kind: &str) -> Result<f64, ProcessClusterError> {
    let family = "bifrost_oracle_local_slot_units";
    let labels = BTreeMap::from([("kind".to_owned(), kind.to_owned())]);
    Ok(node
        .metric_totals_labeled(&[family], &labels)?
        .get(family)
        .copied()
        .unwrap_or(0.0))
}

/// Used Interactive slot units: all used units less Analytical ones.
///
/// # Errors
///
/// Returns cluster control failures.
fn interactive_units(node: &mut ProcessNode) -> Result<f64, ProcessClusterError> {
    tokio::task::block_in_place(|| {
        Ok(slot_units(node, "used")? - slot_units(node, "analytical_used")?)
    })
}

/// Reads the `oracle_admission_total` series for one class and outcome.
///
/// # Errors
///
/// Returns cluster control failures.
fn admission_total(
    node: &mut ProcessNode,
    class: &str,
    outcome: &str,
) -> Result<f64, ProcessClusterError> {
    let family = "oracle_admission_total";
    let labels = BTreeMap::from([
        ("class".to_owned(), class.to_owned()),
        ("outcome".to_owned(), outcome.to_owned()),
    ]);
    Ok(node
        .metric_totals_labeled(&[family], &labels)?
        .get(family)
        .copied()
        .unwrap_or(0.0))
}

/// Parses one cgroup file into `file:key` readings.
///
/// Flat-keyed files (`cpu.stat`, `memory.events`) yield one reading per line;
/// single-value files (`memory.current`, `memory.peak`) yield `file:value`.
/// Limit files and non-numeric values are skipped.
fn cgroup_values(file: &str, contents: &str) -> Vec<(String, f64)> {
    let mut values = Vec::new();
    for line in contents.lines() {
        let mut fields = line.split_whitespace();
        match (fields.next(), fields.next(), fields.next()) {
            (Some(key), Some(value), None) => {
                if let Ok(value) = value.parse() {
                    values.push((format!("{file}:{key}"), value));
                }
            }
            (Some(value), None, None) => {
                if let Ok(value) = value.parse() {
                    values.push((format!("{file}:value"), value));
                }
            }
            _ => {}
        }
    }
    values
}

/// Counts the benchmark table's Parquet objects, row groups, and bytes.
///
/// Walks the shared object store for `.parquet` files whose path names the
/// table, reading each footer with standard Parquet parsing. An unreadable
/// object is counted as a file with no row groups.
fn data_shape(storage_root: &Path) -> DataShape {
    let mut shape = DataShape::default();
    let mut pending = vec![storage_root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "parquet")
                && path.to_string_lossy().contains("query_capacity")
            {
                shape.files += 1;
                shape.bytes += entry.metadata().map_or(0, |metadata| metadata.len());
                if let Some(reader) = std::fs::File::open(&path)
                    .ok()
                    .and_then(|file| SerializedFileReader::new(file).ok())
                {
                    shape.row_groups +=
                        u64::try_from(reader.metadata().num_row_groups()).unwrap_or(u64::MAX);
                }
            }
        }
    }
    shape
}

/// Driver process CPU seconds from `/proc/self/stat`.
///
/// ponytail: assumes the Linux default 100 clock ticks per second rather than
/// calling `sysconf`; read `_SC_CLK_TCK` if a runner uses another rate.
fn driver_cpu_seconds() -> f64 {
    let Ok(stat) = std::fs::read_to_string("/proc/self/stat") else {
        return f64::NAN;
    };
    // Fields after the parenthesised command name; utime and stime are the
    // 14th and 15th fields overall, so the 12th and 13th after `)`.
    let Some((_, rest)) = stat.rsplit_once(')') else {
        return f64::NAN;
    };
    let ticks: Vec<f64> = rest
        .split_whitespace()
        .skip(11)
        .take(2)
        .filter_map(|value| value.parse().ok())
        .collect();
    ticks.iter().sum::<f64>() / 100.0
}

/// `MemTotal` from `/proc/meminfo`, in bytes, or zero.
fn host_memory_bytes() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|meminfo| {
            meminfo
                .lines()
                .find_map(|line| line.strip_prefix("MemTotal:"))
                .and_then(|value| {
                    value
                        .trim()
                        .trim_end_matches("kB")
                        .trim()
                        .parse::<u64>()
                        .ok()
                })
        })
        .map_or(0, |kib| kib * 1024)
}

/// Writes `value` as pretty JSON.
///
/// # Errors
///
/// Returns [`CapacityError::Output`] when encoding or writing fails.
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), CapacityError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| CapacityError::Output(error.to_string()))?;
    }
    let encoded = serde_json::to_vec_pretty(value)
        .map_err(|error| CapacityError::Output(error.to_string()))?;
    std::fs::write(path, encoded).map_err(|error| CapacityError::Output(error.to_string()))
}

/// Writes every raw client sample of one window as JSON lines.
///
/// # Errors
///
/// Returns [`CapacityError::Output`] when encoding or writing fails.
fn write_samples(path: &Path, run: &FixedRateRun) -> Result<(), CapacityError> {
    let mut lines = Vec::new();
    for sample in &run.samples {
        serde_json::to_writer(&mut lines, sample)
            .map_err(|error| CapacityError::Output(error.to_string()))?;
        lines.push(b'\n');
    }
    std::fs::write(path, lines).map_err(|error| CapacityError::Output(error.to_string()))
}

/// Renders a latency in milliseconds, or `-`.
fn millis(micros: Option<u64>) -> String {
    micros.map_or_else(
        || "-".to_owned(),
        |micros| format!("{:.1}", micros as f64 / 1_000.0),
    )
}

/// Renders one report row as a single table line.
fn render_row(row: &CombinationReport) -> String {
    let count = |outcome| row.outcomes.get(&outcome).copied().unwrap_or(0);
    format!(
        "{:>5} {:>4.0}% {:>3}/{:<3} {:>8.1} {:>7} {:>7} {:>7} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>7} {:>5} {:<16} {}",
        row.offered_rate,
        row.live_occupancy * 100.0,
        row.live_streams_sampled[0],
        row.target_live_streams,
        row.successes_per_second,
        millis(row.terminal_us[1]),
        millis(row.terminal_us[2]),
        millis(row.first_row_us[2]),
        count(ShortQueryOutcome::Success),
        count(ShortQueryOutcome::Degraded),
        count(ShortQueryOutcome::Failed),
        count(ShortQueryOutcome::AdmissionRefused),
        count(ShortQueryOutcome::Deadline),
        count(ShortQueryOutcome::TransportError),
        count(ShortQueryOutcome::WrongResult),
        row.missed_launches,
        row.drain_completions,
        row.sustained,
        row.saturated_boundary,
        if row.invalid.is_empty() {
            "valid".to_owned()
        } else {
            row.invalid.join("; ")
        },
    )
}

/// Renders the metadata and every row as the human-readable report.
fn render_table(metadata: &RunMetadata, rows: &[CombinationReport]) -> String {
    let mut out = format!(
        "Bifrost single-pod query capacity\n\
         commit {} binary {} ({})\n\
         host {} CPUs {} bytes; pod cpu.max {:?} memory.max {:?}; Oracle slots {} (interactive floor {})\n\
         {}\n\
         data: {} published + {} live rows; {} files, {} row groups, {} bytes\n\
         warmup {}s, measurement {}s per row\n",
        metadata.server_commit,
        metadata.server_binary.display(),
        metadata.server_binary_sha256,
        metadata.host_cpus,
        metadata.host_memory_bytes,
        metadata.container_cpu_max,
        metadata.container_memory_max,
        metadata.oracle_slot_limit,
        metadata.oracle_interactive_floor,
        metadata.placement,
        metadata.published_rows,
        metadata.live_rows,
        metadata.data_shape.files,
        metadata.data_shape.row_groups,
        metadata.data_shape.bytes,
        metadata.warmup_seconds,
        metadata.measurement_seconds,
    );
    for (sql, digest) in metadata.sql.iter().zip(&metadata.expected_digests) {
        out.push_str(&format!("  {digest}  {sql}\n"));
    }
    out.push_str(
        "\n rate  live  held/tgt  ok/s   p95ms   p99ms  1st99  ok     degr   fail   refuse dline  transp wrong  missed drained sust  boundary         validity\n",
    );
    for row in rows {
        out.push_str(&render_row(row));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cgroup files parse into flat readings and skip limits.
    ///
    /// # Panics
    ///
    /// Panics when a reading is missing or a limit is parsed.
    #[test]
    fn cgroup_evidence_parses_flat_and_single_value_files() {
        let stat = cgroup_values("cpu.stat", "usage_usec 10\nnr_throttled 2\n");
        assert_eq!(
            stat,
            vec![
                ("cpu.stat:usage_usec".to_owned(), 10.0),
                ("cpu.stat:nr_throttled".to_owned(), 2.0)
            ]
        );
        assert_eq!(
            cgroup_values("memory.peak", "4096\n"),
            vec![("memory.peak:value".to_owned(), 4096.0)]
        );
    }
}
