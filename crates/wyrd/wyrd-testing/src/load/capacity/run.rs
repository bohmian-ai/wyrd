//! The Bifrost OLAP capacity benchmark: one visible sequence of named cases.
//!
//! Every pod is a local child of [`BifrostProcessCluster`] in its own
//! 4-CPU/8-GiB systemd user scope; the driver, the parent, and PostgreSQL run
//! outside it. Every query and write goes through `wyrd_client::Bifrost` over
//! the pod's public listeners, and every server figure comes from the pod's
//! production recorder and its own cgroup. [`QueryCapacityBenchmark::standard`]
//! and [`QueryCapacityBenchmark::heavy`] read top to bottom as start, seed,
//! validate, warm up, measure, validate, report.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::time::Duration;

use arrow::array::{AsArray as _, RecordBatch};
use arrow::datatypes::{DataType, Int64Type};
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
    BifrostQueryRequest, QueryClass, QueryTerminalErrorCode, QueryTerminalOutcome,
};

use super::schedule::{
    ClosedLoopDriver, FixedRateDriver, FixedRateRun, ProbeResult, ShortQueryOutcome, percentiles,
};
use super::workload::{self, Case, Fixture, Rows, Target};
use crate::bifrost::process_cluster::{
    BifrostProcessCluster, CGROUP_EVIDENCE_FILES, ProcessClusterError, ProcessNode,
    ProcessNodeTarget,
};

/// Client counts the selective and small-aggregate sweeps run at.
pub const SWEEP_CONCURRENCY: [usize; 6] = [1, 4, 8, 16, 32, 64];

/// Unmeasured warmup before every timed window.
const WARMUP: Duration = Duration::from_secs(2);

/// Measured length of every timed window except the mixed pair.
const WINDOW: Duration = Duration::from_secs(10);

/// Measured length of the read-only and concurrent-write mixed windows.
const MIXED_WINDOW: Duration = Duration::from_secs(30);

/// Serial completions of the broad-window and full-scan cases.
const SERIAL_RUNS: u64 = 3;

/// Offered mixed reads per second, the same with and without writes.
const MIXED_OFFER: u64 = 110;

/// Most mixed reads in flight, the same with and without writes.
const MIXED_IN_FLIGHT: usize = 64;

/// Every fifth mixed arrival is the 1-million-row aggregate; the other four
/// are small aggregates.
const MIXED_MEDIUM_EVERY: u64 = 5;

/// Clients in the million-row aggregate and remote-live windows.
const EIGHT_CLIENTS: usize = 8;

/// Concurrent public writers in every ingest measurement.
const WRITERS: usize = 4;

/// Leader deadline every measured query carries.
const QUERY_DEADLINE_MS: i64 = 120_000;

/// Leader deadline of full-queue holders and waiters; longer than the case.
const WAITER_DEADLINE_MS: i64 = 600_000;

/// Oracle's per-node waiting places.
const QUEUE_PLACES: usize = 1_000;

/// How long the full queue may take to fill or to drain after cancellation.
const QUEUE_SETTLE: Duration = Duration::from_secs(60);

/// Peak pod memory every row must stay below.
const MEMORY_CEILING_BYTES: f64 = 7.0 * 1024.0 * 1024.0 * 1024.0;

/// Acknowledged rows per second every ingest row must reach.
const MIN_INGEST_ROWS_PER_SECOND: f64 = 100_000.0;

/// Mixed p95 at or above this multiple of its read-only baseline fails.
const MAX_MIXED_DEGRADATION: f64 = 1.2;

/// Rows the remote-live cluster publishes: 64 disjoint small windows.
const REMOTE_ROWS: i64 = 64 * workload::SMALL_ROWS;

/// Acknowledged, unflushed rows the remote Scribe holds.
const REMOTE_LIVE_ROWS: i64 = 32_768;

/// How long each live stream stays admitted: longer than warmup plus window.
const LIVE_HOLD: Duration = Duration::from_secs(14);

/// How long held streams have to open before the remote window is invalid.
const LIVE_OPEN_TIMEOUT: Duration = Duration::from_secs(30);

/// Largest scheduling lag at which the fixed-rate driver still offers its rate.
const MAX_DRIVER_LAG: Duration = Duration::from_millis(100);

/// How long fixed-rate windows wait for outstanding queries.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

/// Largest gRPC message the public client sends or accepts.
const CLIENT_MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;

/// Failures that stop the benchmark rather than become a report row.
#[derive(Debug, thiserror::Error)]
pub enum CapacityError {
    /// The process cluster or its control protocol failed.
    #[error(transparent)]
    Cluster(#[from] ProcessClusterError),
    /// The public client refused a setup call.
    #[error(transparent)]
    Client(#[from] BifrostClientError),
    /// The public client could not be configured.
    #[error(transparent)]
    ClientConfig(#[from] wyrd_client::error::WyrdClientError),
    /// A fixture batch could not be built.
    #[error(transparent)]
    Arrow(#[from] arrow::error::ArrowError),
    /// A report, raw sample, or log could not be written.
    #[error("benchmark output: {0}")]
    Output(String),
    /// The seeded fixture does not answer as the workload states.
    #[error("benchmark validation: {0}")]
    Validation(String),
    /// A row or summary did not pass; the report was written.
    #[error("benchmark requirement failed: {0}")]
    Requirement(String),
}

/// Operator settings; every window and target is fixed by the workload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkSettings {
    /// Child binary each pod runs.
    pub binary: PathBuf,
    /// Directory the report, raw samples, logs, and snapshots land in.
    pub output: PathBuf,
    /// Run the 100-million-row heavy-scan qualification instead.
    pub heavy: bool,
}

impl BenchmarkSettings {
    /// Reads `WYRD_BENCH_NODE_BINARY` (default: the sibling
    /// `bifrost_peer_test_node`), `WYRD_BENCH_OUTPUT_DIR` (default
    /// `target/bifrost-query-capacity`), and `WYRD_BENCH_HEAVY_SCAN=1`.
    ///
    /// The standard suite writes under `<output>/standard`, the heavy one
    /// under `<output>/heavy`, so an earlier report beside them is kept.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityError::Output`] when the current executable path is
    /// unavailable for the default binary.
    pub fn from_env() -> Result<Self, CapacityError> {
        let heavy = std::env::var("WYRD_BENCH_HEAVY_SCAN").is_ok_and(|value| value == "1");
        let binary = match std::env::var_os("WYRD_BENCH_NODE_BINARY") {
            Some(binary) => PathBuf::from(binary),
            None => std::env::current_exe()
                .map_err(|error| CapacityError::Output(error.to_string()))?
                .with_file_name("bifrost_peer_test_node"),
        };
        let output = std::env::var_os("WYRD_BENCH_OUTPUT_DIR").map_or_else(
            || PathBuf::from("target/bifrost-query-capacity"),
            PathBuf::from,
        );
        Ok(Self {
            binary,
            output: output.join(if heavy { "heavy" } else { "standard" }),
            heavy,
        })
    }
}

/// Whether a row's measurement is valid and, if so, met its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Verdict {
    /// Valid, and every target met.
    Pass,
    /// Valid, and a target missed.
    Fail,
    /// Not a valid measurement of what the row names.
    #[default]
    Invalid,
}

impl std::fmt::Display for Verdict {
    /// Renders `PASS`, `FAIL`, or `INVALID`.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Invalid => "INVALID",
        })
    }
}

/// One pod's CPU and memory over one row, from its own cgroup.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct Resources {
    /// Mean CPUs used over the row: `usage_usec` delta over the window.
    pub cpus_used: f64,
    /// CPUs the cgroup's `cpu.max` allows.
    pub cpu_limit: f64,
    /// CFS periods in the row.
    pub periods: f64,
    /// Periods in which the cgroup was throttled.
    pub throttled_periods: f64,
    /// Total throttled time, seconds.
    pub throttled_seconds: f64,
    /// `memory.peak` at the row's end: the pod's peak since it started.
    pub peak_memory_bytes: f64,
    /// OOM kills during the row.
    pub oom_kills: f64,
}

impl Resources {
    /// Whether the evidence shows CPU saturation over `window_seconds`.
    ///
    /// Saturated means at least 90% of the quota was used, or at least 10% of
    /// periods were throttled for at least 10% of the window. A rare throttle
    /// under a mostly idle quota is scheduling noise, not a CPU limit.
    #[must_use]
    pub fn cpu_saturated(&self, window_seconds: f64) -> bool {
        let used = self.cpu_limit > 0.0 && self.cpus_used >= 0.9 * self.cpu_limit;
        let throttled = self.periods > 0.0
            && self.throttled_periods / self.periods >= 0.1
            && self.throttled_seconds >= 0.1 * window_seconds;
        used || throttled
    }
}

/// Server-side evidence over one row, from the pod's metric exposition.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct ServerEvidence {
    /// `oracle_admission_total{outcome="rejected"}` deltas by `reason`.
    pub rejected: BTreeMap<String, f64>,
    /// `oracle_query_bytes_scanned_total` delta; `None` when not exposed.
    pub scan_bytes: Option<f64>,
    /// Mean `oracle_query_phase_seconds` per phase, milliseconds.
    pub phase_mean_ms: BTreeMap<String, f64>,
    /// Mean `wyrd_postgres_pool_acquire_seconds` for the app pool, ms.
    pub pool_wait_mean_ms: Option<f64>,
}

/// One read workload at one concurrency.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct ReadRow {
    /// Case name, prefixed for mixed and remote rows.
    pub workload: String,
    /// Clients (closed loop) or in-flight bound (fixed rate).
    pub concurrency: usize,
    /// Offered arrivals per second for fixed-rate rows.
    pub offered_per_second: Option<u64>,
    /// Rows the statement is meant to examine.
    pub rows_examined: i64,
    /// Measured seconds.
    pub window_seconds: f64,
    /// Measured-window terminals per category.
    pub outcomes: BTreeMap<ShortQueryOutcome, u64>,
    /// Measured `Success` terminals per second.
    pub qps: f64,
    /// Client send-to-terminal p50/p95/p99 of successes, milliseconds.
    pub latency_ms: [Option<f64>; 3],
    /// Mixed rows: p95 per case, milliseconds.
    pub case_p95_ms: BTreeMap<String, f64>,
    /// Physical scan bytes per second over the window.
    pub scan_bytes_per_second: Option<f64>,
    /// Fixed-rate arrivals the in-flight bound or driver lag dropped.
    pub missed_launches: u64,
    /// Queries with no terminal after drain.
    pub abandoned: u64,
    /// Live streams the row must hold for its whole window.
    pub live_target: usize,
    /// Fewest live streams open at any sample in the window.
    pub live_open_min: usize,
    /// Held-stream outcomes.
    pub live: LiveStreamTally,
    /// Seconds the fixture this row read took to seed; `None` if unknown.
    pub fixture_seed_seconds: Option<f64>,
    /// The queried pod's resources.
    pub resources: Resources,
    /// The remote Scribe pod's resources, when there is one.
    pub peer_resources: Option<Resources>,
    /// The queried pod's server evidence.
    pub server: ServerEvidence,
    /// Validity problems found while measuring.
    pub invalid: Vec<String>,
    /// The judgement.
    pub verdict: Verdict,
    /// Why the verdict is what it is.
    pub reason: String,
    /// First measured limit for a non-passing row.
    pub bottleneck: String,
}

impl ReadRow {
    /// Measured terminals of `outcome`.
    fn count(&self, outcome: ShortQueryOutcome) -> u64 {
        self.outcomes.get(&outcome).copied().unwrap_or(0)
    }

    /// Judges the row against `target`, and a mixed row against `baseline`.
    ///
    /// Invalid first: a wrong result, a missed offer or abandoned query, a
    /// refused or dropped live holder, missing percentiles or concurrency,
    /// missing physical scan bytes when `scan_required`, or an unknown seed
    /// duration. A valid row fails on any unsuccessful terminal, memory at or
    /// above the ceiling or an OOM kill, a missed latency, rate, or scan
    /// target, or a mixed p95 at least 20% above its read-only baseline.
    /// A non-passing row then names its bottleneck.
    pub fn judge(&mut self, target: &Target, scan_required: bool, baseline: Option<&Self>) {
        let mut invalid = self.invalid.clone();
        let wrong = self.count(ShortQueryOutcome::WrongResult);
        if wrong > 0 {
            invalid.push(format!("{wrong} wrong results"));
        }
        if self.missed_launches > 0 {
            invalid.push(format!(
                "{} arrivals missed the offer",
                self.missed_launches
            ));
        }
        if self.abandoned > 0 {
            invalid.push(format!("{} queries abandoned", self.abandoned));
        }
        let live = self.live;
        if self.live_target > 0
            && (live.refused + live.failed + live.not_interactive > 0
                || live.completed < self.live_target as u64
                || self.live_open_min < self.live_target)
        {
            invalid.push(format!(
                "held {} of {} live streams: {live:?}",
                self.live_open_min, self.live_target
            ));
        }
        if self.latency_ms.iter().any(Option::is_none) {
            invalid.push("missing p50/p95/p99".to_owned());
        }
        if self.concurrency == 0 {
            invalid.push("missing QPS concurrency".to_owned());
        }
        if scan_required && !self.scan_bytes_per_second.is_some_and(|rate| rate > 0.0) {
            invalid.push("missing physical scan bytes".to_owned());
        }
        if self.fixture_seed_seconds.is_none() {
            invalid.push("missing fixture seed duration".to_owned());
        }

        let mut failed = Vec::new();
        let unsuccessful =
            self.outcomes.values().sum::<u64>() - self.count(ShortQueryOutcome::Success) - wrong;
        if unsuccessful > 0 {
            failed.push(format!("{unsuccessful} queries did not succeed"));
        }
        failed.extend(memory_failure(&self.resources));
        let limits = [
            ("p50", target.p50_ms, self.latency_ms[0]),
            ("p95", target.p95_ms, self.latency_ms[1]),
            ("p99", target.p99_ms, self.latency_ms[2]),
        ];
        for (label, limit, value) in limits {
            if let (Some(limit), Some(value)) = (limit, value)
                && value >= limit
            {
                failed.push(format!("{label} {value:.1} ms >= {limit} ms"));
            }
        }
        if let Some(floor) = target.min_qps
            && self.qps < floor
        {
            failed.push(format!("{:.1} QPS < {floor}", self.qps));
        }
        if let (Some(floor), Some(rate)) =
            (target.min_scan_bytes_per_second, self.scan_bytes_per_second)
            && rate < floor
        {
            failed.push(format!(
                "scan {:.0} MB/s < {:.0} MB/s",
                rate / 1e6,
                floor / 1e6
            ));
        }
        if let Some(baseline) = baseline {
            for (case, base) in &baseline.case_p95_ms {
                if let Some(p95) = self.case_p95_ms.get(case)
                    && *p95 >= base * MAX_MIXED_DEGRADATION
                {
                    failed.push(format!(
                        "{case} p95 {p95:.1} ms is {:.0}% above read-only {base:.1} ms",
                        (p95 / base - 1.0) * 100.0
                    ));
                }
            }
        }
        let has_target = limits.iter().any(|(_, limit, _)| limit.is_some())
            || target.min_qps.is_some()
            || target.min_scan_bytes_per_second.is_some()
            || baseline.is_some();
        (self.verdict, self.reason) = if !invalid.is_empty() {
            (Verdict::Invalid, invalid.join("; "))
        } else if !failed.is_empty() {
            (Verdict::Fail, failed.join("; "))
        } else if has_target {
            (Verdict::Pass, "targets met".to_owned())
        } else {
            (Verdict::Pass, "exact result; no numeric target".to_owned())
        };
        self.bottleneck = match self.verdict {
            Verdict::Pass => "-".to_owned(),
            Verdict::Invalid if wrong > 0 => "result correctness".to_owned(),
            Verdict::Invalid => "measurement validity".to_owned(),
            Verdict::Fail => self.classify(),
        };
    }

    /// Names the first measured limit of a failed row from its evidence.
    ///
    /// In order: memory (OOM, ceiling, or a memory/spill refusal), Oracle
    /// execution admission (by server refusal reason, else the client's
    /// refusals), Oracle queue deadline, a
    /// deadline outside the Oracle queue (snapshot or execution), CPU
    /// saturation, PostgreSQL pool wait, then the slowest server phase, and
    /// `undetermined` when no evidence points anywhere.
    fn classify(&self) -> String {
        let rejected = |reason: &str| self.server.rejected.get(reason).copied().unwrap_or(0.0);
        if memory_failure(&self.resources).is_some() || rejected("memory") + rejected("spill") > 0.0
        {
            return "memory".to_owned();
        }
        if let Some((reason, _)) = self
            .server
            .rejected
            .iter()
            .filter(|(reason, count)| {
                ["queue_full", "class_capacity", "tenant_budget"].contains(&reason.as_str())
                    && **count > 0.0
            })
            .max_by(|left, right| left.1.total_cmp(right.1))
        {
            return format!("oracle admission: {reason}");
        }
        if self.count(ShortQueryOutcome::AdmissionRefused) > 0 {
            return "oracle admission".to_owned();
        }
        if rejected("queue_deadline") > 0.0 {
            return "oracle queue deadline".to_owned();
        }
        if self.count(ShortQueryOutcome::Deadline) > 0 {
            return "snapshot or execution deadline".to_owned();
        }
        if self.resources.cpu_saturated(self.window_seconds) {
            return "cgroup cpu".to_owned();
        }
        if self
            .server
            .pool_wait_mean_ms
            .is_some_and(|wait| wait >= 5.0)
        {
            return "postgres pool wait".to_owned();
        }
        self.server
            .phase_mean_ms
            .iter()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map_or_else(
                || "undetermined".to_owned(),
                |(phase, mean)| format!("server phase {phase} ({mean:.1} ms mean)"),
            )
    }
}

/// Names a memory failure: an OOM kill or a peak at or above the ceiling.
fn memory_failure(resources: &Resources) -> Option<String> {
    if resources.oom_kills > 0.0 {
        Some(format!("{} OOM kills", resources.oom_kills))
    } else if resources.peak_memory_bytes >= MEMORY_CEILING_BYTES {
        Some(format!(
            "peak memory {:.2} GiB >= 7 GiB",
            resources.peak_memory_bytes / 1_073_741_824.0
        ))
    } else {
        None
    }
}

/// One public ingest measurement: the seed or the concurrent writes.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct IngestRow {
    /// Measurement name.
    pub workload: String,
    /// Concurrent public writers.
    pub writers: usize,
    /// Rows per `write_batch` request.
    pub rows_per_request: i64,
    /// First send to last acknowledgement, seconds.
    pub seconds: f64,
    /// Rows the server acknowledged durably.
    pub acknowledged_rows: u64,
    /// Acknowledged rows per second.
    pub rows_per_second: f64,
    /// Acknowledged Arrow value bytes per second.
    pub input_bytes_per_second: f64,
    /// Send-to-acknowledgement p50/p95/p99, milliseconds.
    pub ack_ms: [Option<f64>; 3],
    /// Refused requests by stable error code.
    pub refused: BTreeMap<String, u64>,
    /// Committed Parquet files after publication.
    pub files: u64,
    /// Mean committed file size, bytes.
    pub average_file_bytes: f64,
    /// Whether every acknowledged row read back exactly.
    pub read_back: bool,
    /// The pod's resources over the writes.
    pub resources: Resources,
    /// The judgement.
    pub verdict: Verdict,
    /// Why.
    pub reason: String,
}

impl IngestRow {
    /// Invalid on a failed read-back, no acknowledged rows, or no batch
    /// latency; fails on refusals, a rate below 100,000 rows/s, or memory.
    pub fn judge(&mut self) {
        let mut invalid = Vec::new();
        if !self.read_back {
            invalid.push("acknowledged rows did not read back".to_owned());
        }
        if self.acknowledged_rows == 0 || self.ack_ms[1].is_none() {
            invalid.push("no acknowledged batch".to_owned());
        }
        let mut failed = Vec::new();
        let refused = self.refused.values().sum::<u64>();
        if refused > 0 {
            failed.push(format!("{refused} refused requests: {:?}", self.refused));
        }
        if self.rows_per_second < MIN_INGEST_ROWS_PER_SECOND {
            failed.push(format!("{:.0} rows/s < 100000", self.rows_per_second));
        }
        failed.extend(memory_failure(&self.resources));
        (self.verdict, self.reason) = if !invalid.is_empty() {
            (Verdict::Invalid, invalid.join("; "))
        } else if !failed.is_empty() {
            (Verdict::Fail, failed.join("; "))
        } else {
            (Verdict::Pass, "targets met".to_owned())
        };
    }
}

/// The full-queue case: 1,000 waiters, the overflow, and their release.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct QueueRow {
    /// Streams holding every executable slot.
    pub holders: usize,
    /// Most queries Oracle reported waiting.
    pub queued_peak: f64,
    /// Waiters that ended before cancellation.
    pub early_terminals: u64,
    /// The 1,001st query's category.
    pub overflow: Option<ShortQueryOutcome>,
    /// `queue_full` refusals the server counted over the case.
    pub queue_full_refusals: f64,
    /// Cancellation to an empty queue, seconds; `None` if it never emptied.
    pub release_seconds: Option<f64>,
    /// The pod's resources over the case.
    pub resources: Resources,
    /// The judgement.
    pub verdict: Verdict,
    /// Why.
    pub reason: String,
}

impl QueueRow {
    /// Passes only when the queue filled to 1,000 with no early terminal, the
    /// overflow was one retryable queue-full refusal, cancellation emptied it
    /// within the settle time, and memory stayed below the ceiling.
    fn judge(&mut self) {
        let mut failed = Vec::new();
        if self.queued_peak < QUEUE_PLACES as f64 {
            failed.push(format!("only {} queries queued", self.queued_peak));
        }
        if self.early_terminals > 0 {
            failed.push(format!("{} waiters ended early", self.early_terminals));
        }
        if self.overflow != Some(ShortQueryOutcome::AdmissionRefused)
            || self.queue_full_refusals < 1.0
        {
            failed.push(format!(
                "overflow was {:?} with {} queue_full refusals",
                self.overflow, self.queue_full_refusals
            ));
        }
        if self.release_seconds.is_none() {
            failed.push("the queue did not empty after cancellation".to_owned());
        }
        failed.extend(memory_failure(&self.resources));
        (self.verdict, self.reason) = if failed.is_empty() {
            (Verdict::Pass, "filled, overflowed, and released".to_owned())
        } else {
            (Verdict::Fail, failed.join("; "))
        };
    }
}

/// A sweep's capacity: the best rate among rows within their latency target.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SweepSummary {
    /// Case name.
    pub workload: String,
    /// Highest QPS of a passing row, and its concurrency.
    pub best: Option<(f64, usize)>,
    /// The case's QPS floor.
    pub min_qps: f64,
    /// The judgement.
    pub verdict: Verdict,
}

impl SweepSummary {
    /// Passes when some passing row reached `min_qps`; invalid when any row
    /// of the sweep was invalid.
    fn new(workload: &str, rows: &[ReadRow], min_qps: f64) -> Self {
        let best = rows
            .iter()
            .filter(|row| row.verdict == Verdict::Pass)
            .map(|row| (row.qps, row.concurrency))
            .max_by(|left, right| left.0.total_cmp(&right.0));
        let verdict = if rows.iter().any(|row| row.verdict == Verdict::Invalid) {
            Verdict::Invalid
        } else if best.is_some_and(|(qps, _)| qps >= min_qps) {
            Verdict::Pass
        } else {
            Verdict::Fail
        };
        Self {
            workload: workload.to_owned(),
            best,
            min_qps,
            verdict,
        }
    }
}

/// Environment, fixture geometry, and statements recorded with the report.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RunMetadata {
    /// `git rev-parse HEAD`, or `unknown`.
    pub commit: String,
    /// SHA-256 of the child binary.
    pub binary_sha256: String,
    /// CPUs visible to the driver host.
    pub host_cpus: usize,
    /// `MemTotal` of the driver host, bytes.
    pub host_memory_bytes: u64,
    /// The queried pod's cgroup path, `cpu.max`, and `memory.max`.
    pub pod_cgroup: [Option<String>; 3],
    /// Resolved Oracle slot units.
    pub oracle_slots: f64,
    /// Seeded rows.
    pub fixture_rows: i64,
    /// Published Parquet files, row groups, and bytes of the read table.
    pub data_shape: DataShape,
    /// Case, statement, and SHA-256 of its expected rows, for variant zero.
    pub cases: Vec<(String, String, String)>,
}

/// Published object layout of one table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct DataShape {
    /// Parquet objects.
    pub files: u64,
    /// Row groups across them.
    pub row_groups: u64,
    /// Bytes across them.
    pub bytes: u64,
}

/// Everything one benchmark mode produced.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    /// Environment and fixture.
    pub metadata: RunMetadata,
    /// Actual seconds of each setup and measurement phase, in order.
    pub phases: Vec<(String, f64)>,
    /// Ingest rows.
    pub ingest: Vec<IngestRow>,
    /// Read rows.
    pub reads: Vec<ReadRow>,
    /// Sweep capacities.
    pub sweeps: Vec<SweepSummary>,
    /// The full-queue case, standard mode only.
    pub queue: Option<QueueRow>,
}

impl Report {
    /// Names every row and summary that did not pass.
    fn failures(&self) -> Vec<String> {
        let ingest = self
            .ingest
            .iter()
            .filter(|row| row.verdict != Verdict::Pass)
            .map(|row| format!("{}: {} {}", row.workload, row.verdict, row.reason));
        let reads = self
            .reads
            .iter()
            .filter(|row| row.verdict != Verdict::Pass)
            .map(|row| {
                format!(
                    "{}@{}: {} {}",
                    row.workload, row.concurrency, row.verdict, row.reason
                )
            });
        let sweeps = self
            .sweeps
            .iter()
            .filter(|sweep| sweep.verdict != Verdict::Pass)
            .map(|sweep| format!("{} capacity: {}", sweep.workload, sweep.verdict));
        let queue = self
            .queue
            .iter()
            .filter(|queue| queue.verdict != Verdict::Pass)
            .map(|queue| format!("full-queue: {}", queue.reason));
        ingest.chain(reads).chain(sweeps).chain(queue).collect()
    }

    /// Renders the human-readable report.
    #[must_use]
    pub fn render(&self) -> String {
        let meta = &self.metadata;
        let mut out = format!(
            "Bifrost OLAP capacity: commit {} binary sha256 {}\n\
             host {} CPUs {:.1} GiB; pod cgroup {:?} cpu.max {:?} memory.max {:?}; Oracle slots {}\n\
             fixture {} rows: {} files, {} row groups, {} bytes\n",
            meta.commit,
            meta.binary_sha256,
            meta.host_cpus,
            meta.host_memory_bytes as f64 / 1_073_741_824.0,
            meta.pod_cgroup[0],
            meta.pod_cgroup[1],
            meta.pod_cgroup[2],
            meta.oracle_slots,
            meta.fixture_rows,
            meta.data_shape.files,
            meta.data_shape.row_groups,
            meta.data_shape.bytes,
        );
        for (case, sql, digest) in &meta.cases {
            out.push_str(&format!("  {case:<16} {digest:.16}  {sql}\n"));
        }
        out.push_str("\nphases (s): ");
        let phases = self
            .phases
            .iter()
            .map(|(phase, seconds)| format!("{phase} {seconds:.1}"))
            .collect::<Vec<_>>();
        out.push_str(&phases.join(" | "));
        out.push_str(
            "\n\ningest              writers rows/s    MB/s   ack p50/p95/p99 ms   files avg-MB  \
             cpu    peak-GiB verdict reason\n",
        );
        for row in &self.ingest {
            out.push_str(&format!(
                "{:<20} {:>7} {:>9.0} {:>7.1} {:>20} {:>5} {:>6.1}  {:<6} {:>8.2} {:<7} {}\n",
                row.workload,
                row.writers,
                row.rows_per_second,
                row.input_bytes_per_second / 1e6,
                triple(row.ack_ms),
                row.files,
                row.average_file_bytes / 1e6,
                cpu(&row.resources),
                row.resources.peak_memory_bytes / 1_073_741_824.0,
                row.verdict,
                row.reason,
            ));
        }
        out.push_str(
            "\nread                 conc offer  qps       p50/p95/p99 ms       ok     refused dline  \
             fail   wrong  scan-MB/s cpu    peak-GiB verdict reason | bottleneck\n",
        );
        for row in &self.reads {
            out.push_str(&render_read(row));
        }
        for sweep in &self.sweeps {
            out.push_str(&format!(
                "{} capacity: best {} within latency target; floor {} QPS: {}\n",
                sweep.workload,
                sweep.best.map_or_else(
                    || "none".to_owned(),
                    |(qps, clients)| format!("{qps:.1} QPS at {clients} clients")
                ),
                sweep.min_qps,
                sweep.verdict,
            ));
        }
        if let Some(queue) = &self.queue {
            out.push_str(&format!(
                "\nfull-queue: {} holders, {} queued, {} early terminals, overflow {:?}, \
                 {} queue_full refusals, released in {}; peak {:.2} GiB: {} {}\n",
                queue.holders,
                queue.queued_peak,
                queue.early_terminals,
                queue.overflow,
                queue.queue_full_refusals,
                queue
                    .release_seconds
                    .map_or_else(|| "never".to_owned(), |seconds| format!("{seconds:.2}s")),
                queue.resources.peak_memory_bytes / 1_073_741_824.0,
                queue.verdict,
                queue.reason,
            ));
        }
        out.push_str("\nserver phase means (ms) and PostgreSQL app-pool wait\n");
        for row in &self.reads {
            let phases = row
                .server
                .phase_mean_ms
                .iter()
                .map(|(phase, mean)| format!("{phase} {mean:.2}"))
                .collect::<Vec<_>>()
                .join(" | ");
            out.push_str(&format!(
                "{:<20} {:>4} pool {} | {phases}\n",
                row.workload,
                row.concurrency,
                row.server
                    .pool_wait_mean_ms
                    .map_or_else(|| "-".to_owned(), |wait| format!("{wait:.2}")),
            ));
        }
        out
    }
}

/// Renders `p50/p95/p99` milliseconds.
fn triple(values: [Option<f64>; 3]) -> String {
    values
        .map(|value| value.map_or_else(|| "-".to_owned(), |ms| format!("{ms:.1}")))
        .join("/")
}

/// Renders mean CPUs used over the limit.
fn cpu(resources: &Resources) -> String {
    format!("{:.2}/{:.0}", resources.cpus_used, resources.cpu_limit)
}

/// Renders one read row as a table line.
fn render_read(row: &ReadRow) -> String {
    let refused = row.count(ShortQueryOutcome::AdmissionRefused)
        + row.count(ShortQueryOutcome::SecurityRefused);
    let failed = row.count(ShortQueryOutcome::Failed)
        + row.count(ShortQueryOutcome::Degraded)
        + row.count(ShortQueryOutcome::TransportError);
    format!(
        "{:<20} {:>4} {:>5} {:>9.1} {:>20} {:>6} {:>7} {:>6} {:>6} {:>6} {:>10} {:<6} {:>8.2} {:<7} {} | {}\n",
        row.workload,
        row.concurrency,
        row.offered_per_second
            .map_or_else(|| "-".to_owned(), |offer| offer.to_string()),
        row.qps,
        triple(row.latency_ms),
        row.count(ShortQueryOutcome::Success),
        refused,
        row.count(ShortQueryOutcome::Deadline),
        failed,
        row.count(ShortQueryOutcome::WrongResult),
        row.scan_bytes_per_second
            .map_or_else(|| "-".to_owned(), |rate| format!("{:.1}", rate / 1e6)),
        cpu(&row.resources),
        row.resources.peak_memory_bytes / 1_073_741_824.0,
        row.verdict,
        row.reason,
        row.bottleneck,
    )
}

/// What the held live streams did over one row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct LiveStreamTally {
    /// Streams that held for their interval and returned every live ID.
    pub completed: u64,
    /// Streams refused at admission.
    pub refused: u64,
    /// Streams that ran on the Analytical path.
    pub not_interactive: u64,
    /// Streams with any other terminal or a wrong ID set.
    pub failed: u64,
}

/// A pod's metric exposition and cgroup readings at one instant.
#[derive(Debug, Clone, Default)]
struct Snapshot {
    /// Every exposed series, keyed by its rendered name and labels.
    metrics: BTreeMap<String, f64>,
    /// Cgroup readings by `file:key`.
    cgroup: BTreeMap<String, f64>,
    /// CPUs `cpu.max` allows.
    cpu_limit: f64,
}

impl Snapshot {
    /// Asks `node` for its evidence, copies the raw files into `raw`, and
    /// parses them.
    ///
    /// # Errors
    ///
    /// Returns cluster control or output failures.
    fn capture(node: &mut ProcessNode, raw: &Path) -> Result<Self, CapacityError> {
        let captured = node
            .root()
            .join("evidence")
            .join(uuid::Uuid::now_v7().simple().to_string());
        create_dir(&captured)?;
        tokio::task::block_in_place(|| node.capture_resource_evidence(&captured))?;
        create_dir(raw)?;
        let mut snapshot = Self::default();
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
            match file {
                "metrics.prom" => snapshot.metrics = exposition(&contents),
                "cpu.max" => snapshot.cpu_limit = cpu_limit(&contents),
                _ => snapshot.cgroup.extend(cgroup_values(file, &contents)),
            }
        }
        Ok(snapshot)
    }

    /// This snapshot's growth since `before` over `window_seconds`.
    fn resources_since(&self, before: &Self, window_seconds: f64) -> Resources {
        let delta = |key: &str| {
            self.cgroup.get(key).copied().unwrap_or(0.0)
                - before.cgroup.get(key).copied().unwrap_or(0.0)
        };
        Resources {
            cpus_used: delta("cpu.stat:usage_usec") / 1e6 / window_seconds.max(f64::EPSILON),
            cpu_limit: self.cpu_limit,
            periods: delta("cpu.stat:nr_periods"),
            throttled_periods: delta("cpu.stat:nr_throttled"),
            throttled_seconds: delta("cpu.stat:throttled_usec") / 1e6,
            peak_memory_bytes: self.cgroup.get("memory.peak:value").copied().unwrap_or(0.0),
            oom_kills: delta("memory.events:oom_kill"),
        }
    }

    /// Server evidence accumulated since `before`.
    fn server_since(&self, before: &Self) -> ServerEvidence {
        let delta = |family: &str, labels: &[&str]| -> Option<f64> {
            let mut total = None;
            for (series, value) in &self.metrics {
                let Some(rest) = series.strip_prefix(family) else {
                    continue;
                };
                if !(rest.is_empty() || rest.starts_with('{'))
                    || !labels.iter().all(|label| rest.contains(label))
                {
                    continue;
                }
                let earlier = before.metrics.get(series).copied().unwrap_or(0.0);
                *total.get_or_insert(0.0) += value - earlier;
            }
            total
        };
        let mut evidence = ServerEvidence {
            scan_bytes: delta("oracle_query_bytes_scanned_total", &[]),
            ..ServerEvidence::default()
        };
        for series in self.metrics.keys() {
            if series.starts_with("oracle_admission_total{")
                && series.contains("outcome=\"rejected\"")
                && let Some(reason) = label(series, "reason")
                && !evidence.rejected.contains_key(reason)
            {
                let reason_label = format!("reason=\"{reason}\"");
                let count = delta(
                    "oracle_admission_total",
                    &["outcome=\"rejected\"", &reason_label],
                )
                .unwrap_or(0.0);
                if count > 0.0 {
                    evidence.rejected.insert(reason.to_owned(), count);
                }
            }
            if series.starts_with("oracle_query_phase_seconds_count{")
                && let Some(phase) = label(series, "phase")
                && !evidence.phase_mean_ms.contains_key(phase)
            {
                let phase_label = format!("phase=\"{phase}\"");
                let count = delta("oracle_query_phase_seconds_count", &[&phase_label]);
                let sum = delta("oracle_query_phase_seconds_sum", &[&phase_label]);
                if let (Some(count), Some(sum)) = (count, sum)
                    && count > 0.0
                {
                    evidence
                        .phase_mean_ms
                        .insert(phase.to_owned(), sum / count * 1_000.0);
                }
            }
        }
        let pool = ["pool=\"app\""];
        if let (Some(count), Some(sum)) = (
            delta("wyrd_postgres_pool_acquire_seconds_count", &pool),
            delta("wyrd_postgres_pool_acquire_seconds_sum", &pool),
        ) && count > 0.0
        {
            evidence.pool_wait_mean_ms = Some(sum / count * 1_000.0);
        }
        evidence
    }
}

/// Returns the value of label `name` in one rendered series.
fn label<'a>(series: &'a str, name: &str) -> Option<&'a str> {
    let start = series.find(&format!("{name}=\""))? + name.len() + 2;
    let length = series[start..].find('"')?;
    Some(&series[start..start + length])
}

/// Parses a Prometheus text exposition into series and values.
fn exposition(contents: &str) -> BTreeMap<String, f64> {
    contents
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let (series, value) = line.rsplit_once(' ')?;
            Some((series.to_owned(), value.parse().ok()?))
        })
        .collect()
}

/// CPUs a cgroup `cpu.max` (`quota period`) allows; zero when unlimited.
fn cpu_limit(contents: &str) -> f64 {
    let mut fields = contents.split_whitespace();
    match (
        fields.next().and_then(|quota| quota.parse::<f64>().ok()),
        fields.next().and_then(|period| period.parse::<f64>().ok()),
    ) {
        (Some(quota), Some(period)) if period > 0.0 => quota / period,
        _ => 0.0,
    }
}

/// Parses one cgroup file into `file:key` readings.
///
/// Flat-keyed files (`cpu.stat`, `memory.events`) yield one reading per line;
/// single-value files (`memory.current`, `memory.peak`) yield `file:value`.
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

/// Acknowledged ingest over one measurement, before it becomes a row.
#[derive(Debug, Default)]
struct IngestRun {
    /// Acknowledged `(first event_id, rows)` requests.
    acknowledged: Vec<(i64, i64)>,
    /// Send-to-acknowledgement of acknowledged requests, microseconds.
    latencies_us: Vec<u64>,
    /// Acknowledged Arrow value bytes.
    bytes: u64,
    /// Refused requests by stable code.
    refused: BTreeMap<String, u64>,
    /// First send to last acknowledgement.
    elapsed: Duration,
}

impl IngestRun {
    /// Writes `fixture` rows from `first` in [`workload::REQUEST_ROWS`]-row
    /// requests from [`WRITERS`] concurrent public writers, until `last` or,
    /// when given, until `stop` is cancelled.
    ///
    /// `write_batch` resolves only at the durable acknowledgement, so the
    /// acknowledged rate is the durable write rate. A refused request is
    /// counted by its stable code and its range skipped.
    async fn write(
        writer: &Arc<Bifrost>,
        table: &'static str,
        fixture: Fixture,
        last: i64,
        stop: Option<CancellationToken>,
    ) -> Self {
        let next = Arc::new(AtomicI64::new(0));
        let started = Instant::now();
        let mut writers = JoinSet::new();
        for _ in 0..WRITERS {
            let (writer, next, stop) = (Arc::clone(writer), Arc::clone(&next), stop.clone());
            writers.spawn(async move {
                let mut run = Self::default();
                while !stop.as_ref().is_some_and(CancellationToken::is_cancelled) {
                    let first = next.fetch_add(workload::REQUEST_ROWS, Ordering::Relaxed);
                    if first >= last {
                        break;
                    }
                    let end = (first + workload::REQUEST_ROWS).min(last);
                    let batch = match fixture.batch(first, end) {
                        Ok(batch) => batch,
                        Err(error) => {
                            *run.refused.entry(error.to_string()).or_default() += 1;
                            continue;
                        }
                    };
                    let sent = Instant::now();
                    match writer.write_batch(table, &batch).await {
                        Ok(()) => {
                            run.latencies_us.push(
                                u64::try_from(sent.elapsed().as_micros()).unwrap_or(u64::MAX),
                            );
                            run.acknowledged.push((first, end - first));
                            run.bytes += value_bytes(&batch);
                        }
                        Err(error) => {
                            let code = wyrd_spec::error::WyrdError::from(&error).code();
                            *run.refused.entry(code.to_owned()).or_default() += 1;
                        }
                    }
                }
                run
            });
        }
        let mut total = Self::default();
        while let Some(joined) = writers.join_next().await {
            let Ok(run) = joined else {
                *total
                    .refused
                    .entry("writer panicked".to_owned())
                    .or_default() += 1;
                continue;
            };
            total.acknowledged.extend(run.acknowledged);
            total.latencies_us.extend(run.latencies_us);
            total.bytes += run.bytes;
            for (code, count) in run.refused {
                *total.refused.entry(code).or_default() += count;
            }
        }
        total.elapsed = started.elapsed();
        total
    }

    /// Rows acknowledged.
    fn rows(&self) -> i64 {
        self.acknowledged.iter().map(|(_, rows)| rows).sum()
    }

    /// The report row, before read-back, file shape, and judgement.
    fn row(&self, name: &str, resources: Resources) -> IngestRow {
        let seconds = self.elapsed.as_secs_f64().max(f64::EPSILON);
        let rows = u64::try_from(self.rows()).unwrap_or(0);
        IngestRow {
            workload: name.to_owned(),
            writers: WRITERS,
            rows_per_request: workload::REQUEST_ROWS,
            seconds,
            acknowledged_rows: rows,
            rows_per_second: rows as f64 / seconds,
            input_bytes_per_second: self.bytes as f64 / seconds,
            ack_ms: percentiles(self.latencies_us.clone()).map(to_ms),
            refused: self.refused.clone(),
            resources,
            ..IngestRow::default()
        }
    }
}

/// Arrow value bytes of one fixture batch: five `i64`s plus payload bytes.
fn value_bytes(batch: &RecordBatch) -> u64 {
    let payload = batch
        .column(5)
        .as_string_opt::<i32>()
        .map_or(0, |payloads| payloads.values().len());
    u64::try_from(batch.num_rows() * 40 + payload).unwrap_or(u64::MAX)
}

/// Microseconds as milliseconds.
fn to_ms(micros: Option<u64>) -> Option<f64> {
    micros.map(|micros| micros as f64 / 1_000.0)
}

/// Every precomputed aggregate answer, by case and variant.
type Answers = BTreeMap<Case, Vec<Rows>>;

/// One running cluster, its clients, and the fixture it holds.
struct Bench {
    /// The pods; the queried one is first.
    cluster: BifrostProcessCluster,
    /// Public client on the queried pod.
    client: WyrdClient,
    /// Public query client every measured query uses.
    queries: Arc<Bifrost>,
    /// The seeded fixture.
    fixture: Fixture,
    /// Exact answers for every aggregate variant.
    answers: Arc<Answers>,
    /// Seconds the fixture took to seed.
    seed_seconds: Option<f64>,
    /// Where this cluster's evidence lands.
    output: PathBuf,
    /// Actual seconds of each phase so far.
    phases: Vec<(String, f64)>,
    /// When the current phase started.
    phase_start: Instant,
}

impl Bench {
    /// Launches `targets` in verified benchmark scopes and registers `tables`
    /// through the public client on the first pod.
    ///
    /// # Errors
    ///
    /// Returns launch, key, client, or registration failures.
    async fn start(
        settings: &BenchmarkSettings,
        targets: &[ProcessNodeTarget],
        fixture: Fixture,
        output: PathBuf,
    ) -> Result<(Self, secrecy::SecretString), CapacityError> {
        let started = Instant::now();
        let mut cluster = BifrostProcessCluster::start_benchmark(&settings.binary, targets).await?;
        let api_key = cluster
            .provision_foreign_public_api_key("olap-capacity")
            .await?;
        let client = public_client(pod(&mut cluster)?, &api_key)?;
        let queries = Arc::new(Bifrost::query_only(&client));
        let mut bench = Self {
            cluster,
            client,
            queries,
            fixture,
            answers: Arc::default(),
            seed_seconds: None,
            output,
            phases: Vec::new(),
            phase_start: started,
        };
        bench.phase("launch");
        Ok((bench, api_key))
    }

    /// Records the phase that just ended.
    fn phase(&mut self, name: &str) {
        let now = Instant::now();
        self.phases
            .push((name.to_owned(), (now - self.phase_start).as_secs_f64()));
        self.phase_start = now;
    }

    /// Registers `table` and returns a writer bound to it.
    ///
    /// # Errors
    ///
    /// Returns client or registration failures.
    async fn writer(&self, table: &str) -> Result<Arc<Bifrost>, CapacityError> {
        let writer = Bifrost::connect_with_table(
            &self.client,
            TableConfig::from_arrow(table, workload::schema())?,
        )
        .await?;
        writer.register().await?;
        Ok(Arc::new(writer))
    }

    /// Seeds the fixture through public ingest as an ingest row, publishes
    /// it, computes every answer, and validates each case once.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityError::Validation`] when a write was refused or a
    /// case's first answer differs, and cluster or client failures.
    async fn seed(&mut self, cases: &[Case]) -> Result<IngestRow, CapacityError> {
        let writer = self.writer(workload::TABLE).await?;
        let raw = self.output.join("seed");
        let before = Snapshot::capture(pod(&mut self.cluster)?, &raw.join("before"))?;
        let run = IngestRun::write(
            &writer,
            workload::TABLE,
            self.fixture,
            self.fixture.rows,
            None,
        )
        .await;
        let after = Snapshot::capture(pod(&mut self.cluster)?, &raw.join("after"))?;
        self.seed_seconds = Some(run.elapsed.as_secs_f64());
        let mut row = run.row(
            &format!("seed-{}m", self.fixture.rows / 1_000_000),
            after.resources_since(&before, run.elapsed.as_secs_f64()),
        );
        if run.rows() != self.fixture.rows {
            return Err(CapacityError::Validation(format!(
                "seed acknowledged {} of {} rows: {:?}",
                run.rows(),
                self.fixture.rows,
                run.refused
            )));
        }
        self.phase("seed");
        self.publish()?;
        self.phase("publish");
        let shape = data_shape(self.cluster.storage_root(), "events");
        row.files = shape.files;
        row.average_file_bytes = shape.bytes as f64 / shape.files.max(1) as f64;

        let fixture = self.fixture;
        let wanted = cases.to_vec();
        self.answers = Arc::new(
            tokio::task::spawn_blocking(move || {
                wanted
                    .into_iter()
                    .filter(|case| *case != Case::Selective)
                    .map(|case| {
                        let answers = (0..case.variants())
                            .map(|variant| case.expected(fixture, variant))
                            .collect();
                        (case, answers)
                    })
                    .collect()
            })
            .await
            .map_err(|error| CapacityError::Output(error.to_string()))?,
        );
        self.phase("answers");
        for &case in cases {
            let probe = run_case(&self.queries, self.fixture, &self.answers, case, 0).await;
            if probe.outcome != ShortQueryOutcome::Success {
                return Err(CapacityError::Validation(format!(
                    "{} returned {:?} for `{}`",
                    case.name(),
                    probe.outcome,
                    case.sql(self.fixture, 0)
                )));
            }
        }
        row.read_back = true;
        row.judge();
        self.phase("validate");
        Ok(row)
    }

    /// Flushes the queried pod's Scribe and publishes a fresh snapshot.
    ///
    /// # Errors
    ///
    /// Returns cluster control failures.
    fn publish(&mut self) -> Result<(), CapacityError> {
        let node = pod(&mut self.cluster)?;
        tokio::task::block_in_place(|| node.flush().and_then(|()| node.refresh_snapshot()))?;
        Ok(())
    }

    /// Warmup then one closed-loop window of `case` at `concurrency`, or
    /// [`SERIAL_RUNS`] serial completions when `serial`, judged against the
    /// case's target.
    ///
    /// Sweep rows are judged on latency only; [`SweepSummary`] judges their
    /// rate.
    ///
    /// # Errors
    ///
    /// Returns cluster control or output failures.
    async fn closed(
        &mut self,
        case: Case,
        concurrency: usize,
        serial: bool,
        sweep: bool,
    ) -> Result<ReadRow, CapacityError> {
        let driver = ClosedLoopDriver::new(concurrency);
        let issue = {
            let (queries, fixture, answers) = (
                Arc::clone(&self.queries),
                self.fixture,
                Arc::clone(&self.answers),
            );
            move |sequence| {
                let (queries, answers) = (Arc::clone(&queries), Arc::clone(&answers));
                async move { run_case(&queries, fixture, &answers, case, sequence).await }
            }
        };
        if !serial {
            driver.run(WARMUP, u64::MAX, issue.clone()).await;
        }
        let name = format!("{}-c{concurrency}", case.name());
        let raw = self.output.join(&name);
        let before = Snapshot::capture(pod(&mut self.cluster)?, &raw.join("before"))?;
        let run = if serial {
            driver
                .run(Duration::from_secs(3_600), SERIAL_RUNS, issue)
                .await
        } else {
            driver.run(WINDOW, u64::MAX, issue).await
        };
        let after = Snapshot::capture(pod(&mut self.cluster)?, &raw.join("after"))?;
        write_samples(&raw.join("samples.jsonl"), &run)?;
        let mut row = self.read_row(case.name(), concurrency, &run, &before, &after);
        row.rows_examined = case.rows_examined(self.fixture);
        let mut target = case.target(self.fixture);
        if sweep {
            target.min_qps = None;
        }
        let scan_required = matches!(case, Case::BroadWindow | Case::FullScan);
        row.judge(&target, scan_required, None);
        println!("{}", render_read(&row).trim_end());
        Ok(row)
    }

    /// The row fields every read measurement shares.
    fn read_row(
        &self,
        workload: &str,
        concurrency: usize,
        run: &FixedRateRun,
        before: &Snapshot,
        after: &Snapshot,
    ) -> ReadRow {
        let seconds = run.window.as_secs_f64();
        let server = after.server_since(before);
        ReadRow {
            workload: workload.to_owned(),
            concurrency,
            window_seconds: seconds,
            outcomes: ShortQueryOutcome::ALL
                .into_iter()
                .map(|outcome| (outcome, run.measured(outcome)))
                .collect(),
            qps: run.successes_per_second(),
            latency_ms: run
                .success_percentiles(|sample| Some(sample.terminal_latency_us))
                .map(to_ms),
            scan_bytes_per_second: server
                .scan_bytes
                .filter(|bytes| *bytes > 0.0)
                .map(|bytes| bytes / seconds.max(f64::EPSILON)),
            missed_launches: run.missed_launches,
            abandoned: run.abandoned,
            fixture_seed_seconds: self.seed_seconds,
            resources: after.resources_since(before, seconds),
            server,
            ..ReadRow::default()
        }
    }

    /// The mixed pair: the read-only baseline, then the same offer while
    /// writers fill [`workload::INGEST_TABLE`], then a read-back of every
    /// acknowledged write.
    ///
    /// # Errors
    ///
    /// Returns cluster control, client, or output failures.
    async fn mixed(&mut self) -> Result<(ReadRow, ReadRow, IngestRow), CapacityError> {
        let ingest = self.writer(workload::INGEST_TABLE).await?;
        let mut baseline = self.mixed_window("mixed-read-only", None).await?.0;
        baseline.judge(&mixed_target(), false, None);
        println!("{}", render_read(&baseline).trim_end());
        let (mut writes, run) = self
            .mixed_window("mixed-with-writes", Some(&ingest))
            .await?;
        writes.judge(&mixed_target(), false, Some(&baseline));
        println!("{}", render_read(&writes).trim_end());
        let (run, resources) = run.unwrap_or_default();
        let mut row = run.row("ingest-during-reads", resources);
        self.publish()?;
        let shape = data_shape(self.cluster.storage_root(), "events_ingest");
        row.files = shape.files;
        row.average_file_bytes = shape.bytes as f64 / shape.files.max(1) as f64;
        let expected = vec![vec![
            run.rows(),
            run.acknowledged
                .iter()
                .map(|(first, rows)| (*first..first + rows).sum::<i64>())
                .sum(),
        ]];
        let sql = format!(
            "SELECT COUNT(*), SUM(event_id) FROM {}",
            workload::INGEST_TABLE
        );
        row.read_back = collect(&self.queries, &sql).await.ok() == Some(expected);
        row.judge();
        Ok((baseline, writes, row))
    }

    /// One mixed window: warmup and [`MIXED_WINDOW`] at [`MIXED_OFFER`] per
    /// second, four small aggregates to one million-row aggregate, with
    /// writers running from warmup to the window's end when `writer` is set.
    ///
    /// # Errors
    ///
    /// Returns cluster control or output failures.
    async fn mixed_window(
        &mut self,
        name: &str,
        writer: Option<&Arc<Bifrost>>,
    ) -> Result<(ReadRow, Option<(IngestRun, Resources)>), CapacityError> {
        let stop = CancellationToken::new();
        let raw = self.output.join(name);
        let writes_before = Snapshot::capture(pod(&mut self.cluster)?, &raw.join("writes-before"))?;
        let writers = writer.map(|writer| {
            let (writer, fixture, stop) = (Arc::clone(writer), self.fixture, stop.clone());
            tokio::spawn(async move {
                IngestRun::write(
                    &writer,
                    workload::INGEST_TABLE,
                    fixture,
                    i64::MAX,
                    Some(stop),
                )
                .await
            })
        });
        let issue = {
            let (queries, fixture, answers) = (
                Arc::clone(&self.queries),
                self.fixture,
                Arc::clone(&self.answers),
            );
            move |sequence| {
                let (queries, answers) = (Arc::clone(&queries), Arc::clone(&answers));
                async move {
                    run_case(&queries, fixture, &answers, mixed_case(sequence), sequence).await
                }
            }
        };
        let driver = FixedRateDriver::new(MIXED_OFFER, MIXED_IN_FLIGHT);
        driver.run(WARMUP, DRAIN_TIMEOUT, issue.clone()).await;
        let before = Snapshot::capture(pod(&mut self.cluster)?, &raw.join("before"))?;
        let run = driver.run(MIXED_WINDOW, DRAIN_TIMEOUT, issue).await;
        stop.cancel();
        let after = Snapshot::capture(pod(&mut self.cluster)?, &raw.join("after"))?;
        let ingest = match writers {
            Some(writers) => {
                let run = writers
                    .await
                    .map_err(|error| CapacityError::Output(format!("writers: {error}")))?;
                let resources = after.resources_since(&writes_before, run.elapsed.as_secs_f64());
                Some((run, resources))
            }
            None => None,
        };
        write_samples(&raw.join("samples.jsonl"), &run)?;
        let mut row = self.read_row(name, MIXED_IN_FLIGHT, &run, &before, &after);
        row.offered_per_second = Some(MIXED_OFFER);
        if run.max_launch_lag > MAX_DRIVER_LAG {
            row.invalid.push(format!(
                "the driver fell {:?} behind its schedule",
                run.max_launch_lag
            ));
        }
        for case in [Case::SmallAggregate, Case::MillionAggregate] {
            let [_, p95, _] = run.success_percentiles(|sample| {
                (mixed_case(sample.sequence) == case).then_some(sample.terminal_latency_us)
            });
            if let Some(p95) = to_ms(p95) {
                row.case_p95_ms.insert(case.name().to_owned(), p95);
            }
        }
        Ok((row, ingest))
    }

    /// Fills Oracle's queue with 1,000 waiters behind held slots, overflows
    /// it once, then cancels every waiter and times the release.
    ///
    /// Holders are undrained `SELECT event_id` streams over the whole table,
    /// one per slot unit: those past admission hold a slot for as long as they
    /// are unread, and the rest wait in the queue. Selective waiters fill the
    /// remaining places. The 1,001st query must be a retryable queue-full
    /// refusal, and aborting every waiter must empty the queue.
    ///
    /// # Errors
    ///
    /// Returns cluster control or output failures.
    async fn full_queue(&mut self) -> Result<QueueRow, CapacityError> {
        let raw = self.output.join("full-queue");
        let before = Snapshot::capture(pod(&mut self.cluster)?, &raw.join("before"))?;
        let slots = gauge(
            pod(&mut self.cluster)?,
            "bifrost_oracle_local_slot_units",
            "kind",
            "limit",
        )?;
        let holders_count = slots as usize;
        let release = CancellationToken::new();
        let opened = Arc::new(AtomicUsize::new(0));
        let mut holders = JoinSet::new();
        for _ in 0..holders_count {
            let (queries, release, opened) = (
                Arc::clone(&self.queries),
                release.clone(),
                Arc::clone(&opened),
            );
            holders.spawn(async move {
                let request = BifrostQueryRequest {
                    sql: format!("SELECT event_id FROM {}", workload::TABLE),
                    deadline_ms: Some(WAITER_DEADLINE_MS),
                };
                if let Ok(mut stream) = queries.query(&request).await
                    && let Ok(Some(_)) = stream.next_batch().await
                {
                    opened.fetch_add(1, Ordering::AcqRel);
                    release.cancelled().await;
                }
            });
        }
        let mut row = QueueRow {
            holders: holders_count,
            ..QueueRow::default()
        };
        let settle = Instant::now() + QUEUE_SETTLE;
        let mut queued = self.queued()?;
        while opened.load(Ordering::Acquire) + (queued as usize) < holders_count
            && Instant::now() < settle
        {
            tokio::time::sleep(Duration::from_millis(100)).await;
            queued = self.queued()?;
        }
        let mut waiters = JoinSet::new();
        for sequence in 0..QUEUE_PLACES.saturating_sub(queued as usize) {
            let (queries, fixture) = (Arc::clone(&self.queries), self.fixture);
            waiters.spawn(async move { run_waiter(&queries, fixture, sequence as u64).await });
        }
        while queued < QUEUE_PLACES as f64 && Instant::now() < settle {
            tokio::time::sleep(Duration::from_millis(100)).await;
            queued = self.queued()?;
        }
        row.queued_peak = queued;
        let overflow = tokio::time::timeout(
            Duration::from_secs(30),
            run_case(
                &self.queries,
                self.fixture,
                &self.answers,
                Case::Selective,
                0,
            ),
        )
        .await;
        row.overflow = overflow.ok().map(|probe| probe.outcome);
        let full = Snapshot::capture(pod(&mut self.cluster)?, &raw.join("full"))?;
        while waiters.try_join_next().is_some() {
            row.early_terminals += 1;
        }
        let cancelled = Instant::now();
        waiters.abort_all();
        while waiters.join_next().await.is_some() {}
        while Instant::now() < cancelled + QUEUE_SETTLE {
            if self.queued()? == 0.0 {
                row.release_seconds = Some(cancelled.elapsed().as_secs_f64());
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        release.cancel();
        holders.abort_all();
        while holders.join_next().await.is_some() {}
        let after = Snapshot::capture(pod(&mut self.cluster)?, &raw.join("after"))?;
        row.queue_full_refusals = full
            .server_since(&before)
            .rejected
            .get("queue_full")
            .copied()
            .unwrap_or(0.0);
        row.resources = after.resources_since(&before, 1.0);
        row.judge();
        Ok(row)
    }

    /// Queries Oracle reports waiting, across classes.
    ///
    /// # Errors
    ///
    /// Returns cluster control failures.
    fn queued(&mut self) -> Result<f64, CapacityError> {
        let node = pod(&mut self.cluster)?;
        Ok(
            tokio::task::block_in_place(|| node.metric_totals(&["oracle_queries_queued"]))?
                .get("oracle_queries_queued")
                .copied()
                .unwrap_or(0.0),
        )
    }

    /// Collects the environment and fixture geometry.
    ///
    /// # Errors
    ///
    /// Returns cluster control or binary read failures.
    fn metadata(
        &mut self,
        settings: &BenchmarkSettings,
        cases: &[Case],
    ) -> Result<RunMetadata, CapacityError> {
        let binary = std::fs::read(&settings.binary)
            .map_err(|error| CapacityError::Output(error.to_string()))?;
        let data_shape = data_shape(self.cluster.storage_root(), "events");
        let node = pod(&mut self.cluster)?;
        let directory = node.root().join("evidence").join("metadata");
        create_dir(&directory)?;
        let evidence = tokio::task::block_in_place(|| node.capture_resource_evidence(&directory))?;
        let oracle_slots = gauge(node, "bifrost_oracle_local_slot_units", "kind", "limit")?;
        let cases = cases
            .iter()
            .map(|case| {
                let expected = case.expected(self.fixture, 0);
                let digest = hex::encode(Sha256::digest(
                    serde_json::to_vec(&expected).unwrap_or_default(),
                ));
                (case.name().to_owned(), case.sql(self.fixture, 0), digest)
            })
            .collect();
        Ok(RunMetadata {
            commit: std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map_or_else(
                    || "unknown".to_owned(),
                    |output| String::from_utf8_lossy(&output.stdout).trim().to_owned(),
                ),
            binary_sha256: hex::encode(Sha256::digest(&binary)),
            host_cpus: std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
            host_memory_bytes: host_memory_bytes(),
            pod_cgroup: [evidence.cgroup, evidence.cpu_max, evidence.memory_max],
            oracle_slots,
            fixture_rows: self.fixture.rows,
            data_shape,
            cases,
        })
    }

    /// Copies every pod's stderr log beside the evidence, then shuts down.
    ///
    /// # Errors
    ///
    /// Returns output or shutdown failures.
    fn finish(mut self) -> Result<Vec<(String, f64)>, CapacityError> {
        for (index, node) in self.cluster.nodes().iter().enumerate() {
            std::fs::copy(
                node.root().join("stderr.log"),
                self.output.join(format!("pod-{index}.stderr.log")),
            )
            .map_err(|error| CapacityError::Output(error.to_string()))?;
        }
        self.cluster.shutdown()?;
        self.phase("shutdown");
        Ok(self.phases)
    }
}

/// The mixed case an arrival issues.
const fn mixed_case(sequence: u64) -> Case {
    if sequence % MIXED_MEDIUM_EVERY == MIXED_MEDIUM_EVERY - 1 {
        Case::MillionAggregate
    } else {
        Case::SmallAggregate
    }
}

/// The mixed read target: at least 100 successful reads per second.
fn mixed_target() -> Target {
    Target {
        min_qps: Some(100.0),
        ..Target::default()
    }
}

/// The benchmark's entry points.
pub struct QueryCapacityBenchmark;

impl QueryCapacityBenchmark {
    /// The standard suite on one 10-million-row fixture, then the remote-live
    /// window on a second cluster; writes the report and judges it.
    ///
    /// # Errors
    ///
    /// Returns setup failures, and [`CapacityError::Requirement`] naming every
    /// row that did not pass after the report is written.
    pub async fn standard(settings: &BenchmarkSettings) -> Result<Report, CapacityError> {
        let fixture = Fixture::standard();
        let cases = [
            Case::Selective,
            Case::SmallAggregate,
            Case::MillionAggregate,
            Case::TableAggregate,
            Case::BroadWindow,
            Case::FullScan,
        ];
        let (mut bench, _) = Bench::start(
            settings,
            &[ProcessNodeTarget::All],
            fixture,
            settings.output.clone(),
        )
        .await?;
        let mut report = Report::default();
        report.ingest.push(bench.seed(&cases).await?);
        report.metadata = bench.metadata(settings, &cases)?;

        for case in [Case::Selective, Case::SmallAggregate] {
            let mut rows = Vec::new();
            for concurrency in SWEEP_CONCURRENCY {
                rows.push(bench.closed(case, concurrency, false, true).await?);
            }
            let floor = case.target(fixture).min_qps.unwrap_or(0.0);
            report
                .sweeps
                .push(SweepSummary::new(case.name(), &rows, floor));
            report.reads.extend(rows);
        }
        bench.phase("sweeps");
        report.reads.push(
            bench
                .closed(Case::MillionAggregate, EIGHT_CLIENTS, false, false)
                .await?,
        );
        report
            .reads
            .push(bench.closed(Case::TableAggregate, 1, false, false).await?);
        report
            .reads
            .push(bench.closed(Case::BroadWindow, 1, true, false).await?);
        report
            .reads
            .push(bench.closed(Case::FullScan, 1, true, false).await?);
        bench.phase("aggregates");
        let (baseline, writes, ingest) = bench.mixed().await?;
        report.reads.extend([baseline, writes]);
        report.ingest.push(ingest);
        bench.phase("mixed");
        report.queue = Some(bench.full_queue().await?);
        bench.phase("full-queue");
        report.phases = bench.finish()?;

        let (remote, phases) = remote_live(settings).await?;
        report.reads.push(remote);
        report.phases.extend(
            phases
                .into_iter()
                .map(|(phase, seconds)| (format!("remote-{phase}"), seconds)),
        );
        publish(&settings.output, &report)
    }

    /// The heavy-scan qualification on a fresh 100-million-row fixture: the
    /// broad window and full scan, each three times at one client.
    ///
    /// # Errors
    ///
    /// As [`Self::standard`].
    pub async fn heavy(settings: &BenchmarkSettings) -> Result<Report, CapacityError> {
        let cases = [Case::BroadWindow, Case::FullScan];
        let (mut bench, _) = Bench::start(
            settings,
            &[ProcessNodeTarget::All],
            Fixture::heavy(),
            settings.output.clone(),
        )
        .await?;
        let mut report = Report::default();
        report.ingest.push(bench.seed(&cases).await?);
        report.metadata = bench.metadata(settings, &cases)?;
        for case in cases {
            report.reads.push(bench.closed(case, 1, true, false).await?);
        }
        bench.phase("scans");
        report.phases = bench.finish()?;
        publish(&settings.output, &report)
    }
}

/// The remote-live window on its own cluster: a mixed pod and a Scribe pod
/// holding acknowledged, unflushed rows that every held stream must read.
///
/// Publishes [`REMOTE_ROWS`] through the mixed pod, acknowledges
/// [`REMOTE_LIVE_ROWS`] more through the Scribe pod without flushing, then
/// holds half the Interactive slot units with live streams while eight clients
/// run the small aggregate for warmup plus one window.
///
/// # Errors
///
/// Returns setup, cluster, client, or output failures.
async fn remote_live(
    settings: &BenchmarkSettings,
) -> Result<(ReadRow, Vec<(String, f64)>), CapacityError> {
    let fixture = Fixture {
        rows: REMOTE_ROWS,
        broad_hours: workload::DAY_HOURS,
    };
    let (mut bench, api_key) = Bench::start(
        settings,
        &[ProcessNodeTarget::All, ProcessNodeTarget::Scribe],
        fixture,
        settings.output.join("remote-live"),
    )
    .await?;
    bench.seed(&[Case::SmallAggregate]).await?;
    let scribe = bench
        .cluster
        .nodes()
        .get(1)
        .ok_or_else(|| CapacityError::Output("the remote cluster has no Scribe".to_owned()))?;
    let remote = Bifrost::connect_with_table(
        &public_client(scribe, &api_key)?,
        TableConfig::from_arrow(workload::TABLE, workload::schema())?,
    )
    .await?;
    let live = (REMOTE_ROWS, REMOTE_ROWS + REMOTE_LIVE_ROWS);
    let mut first = live.0;
    while first < live.1 {
        let last = (first + workload::REQUEST_ROWS).min(live.1);
        remote
            .write_batch(workload::TABLE, &fixture.batch(first, last)?)
            .await?;
        first = last;
    }
    bench.phase("live-ack");

    let slots = gauge(
        pod(&mut bench.cluster)?,
        "bifrost_oracle_local_slot_units",
        "kind",
        "limit",
    )?;
    let target = ((slots / 2.0).floor() as usize).max(1);
    let streams = LiveStreams::start(Arc::clone(&bench.queries), target, live);
    let opened = Instant::now();
    while streams.open() < target && opened.elapsed() < LIVE_OPEN_TIMEOUT {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let driver = ClosedLoopDriver::new(EIGHT_CLIENTS);
    let issue = {
        let (queries, answers) = (Arc::clone(&bench.queries), Arc::clone(&bench.answers));
        move |sequence| {
            let (queries, answers) = (Arc::clone(&queries), Arc::clone(&answers));
            async move { run_case(&queries, fixture, &answers, Case::SmallAggregate, sequence).await }
        }
    };
    driver.run(WARMUP, u64::MAX, issue.clone()).await;
    let raw = bench.output.join("remote-live-window");
    let before = Snapshot::capture(pod(&mut bench.cluster)?, &raw.join("before"))?;
    let peer_before = Snapshot::capture(scribe_pod(&mut bench.cluster)?, &raw.join("peer-before"))?;
    let run = tokio::spawn(async move { driver.run(WINDOW, u64::MAX, issue).await });
    let mut open_min = streams.open();
    while !run.is_finished() {
        tokio::time::sleep(Duration::from_millis(250)).await;
        open_min = open_min.min(streams.open());
    }
    let run = run
        .await
        .map_err(|error| CapacityError::Output(format!("driver: {error}")))?;
    let after = Snapshot::capture(pod(&mut bench.cluster)?, &raw.join("after"))?;
    let peer_after = Snapshot::capture(scribe_pod(&mut bench.cluster)?, &raw.join("peer-after"))?;
    let tally = streams.stop().await;
    write_samples(&raw.join("samples.jsonl"), &run)?;
    let mut row = bench.read_row("remote-live", EIGHT_CLIENTS, &run, &before, &after);
    row.rows_examined = Case::SmallAggregate.rows_examined(fixture);
    row.live_target = target;
    row.live_open_min = open_min;
    row.live = tally;
    row.peer_resources = Some(peer_after.resources_since(&peer_before, run.window.as_secs_f64()));
    row.judge(&Case::SmallAggregate.target(fixture), false, None);
    println!("{}", render_read(&row).trim_end());
    bench.phase("window");
    Ok((row, bench.finish()?))
}

/// Writes the report files, prints the report, and fails on any non-pass.
///
/// # Errors
///
/// Returns output failures, then [`CapacityError::Requirement`] naming every
/// row and summary that did not pass.
fn publish(output: &Path, report: &Report) -> Result<Report, CapacityError> {
    write_json(&output.join("report.json"), report)?;
    let rendered = report.render();
    std::fs::write(output.join("report.txt"), &rendered)
        .map_err(|error| CapacityError::Output(error.to_string()))?;
    println!("{rendered}");
    let failures = report.failures();
    if failures.is_empty() {
        Ok(report.clone())
    } else {
        Err(CapacityError::Requirement(failures.join("; ")))
    }
}

/// Live streams held open for [`LIVE_HOLD`] each, until stopped.
struct LiveStreams {
    /// Stops every holder after its current stream.
    cancel: CancellationToken,
    /// Streams past their first batch and not yet drained.
    open: Arc<AtomicUsize>,
    /// One holder task per stream.
    holders: JoinSet<LiveStreamTally>,
}

impl LiveStreams {
    /// Starts `streams` holders over the live `event_id` range `live`.
    ///
    /// A holder reads the first batch, counts itself open, waits
    /// [`LIVE_HOLD`] without reading so the stream stays admitted, then
    /// drains, counts itself closed, and checks for an Interactive `Success`
    /// with every live ID. A refused stream is retried after a short pause.
    fn start(queries: Arc<Bifrost>, streams: usize, live: (i64, i64)) -> Self {
        let cancel = CancellationToken::new();
        let open = Arc::new(AtomicUsize::new(0));
        let mut holders = JoinSet::new();
        for _ in 0..streams {
            let (queries, stop, open) = (Arc::clone(&queries), cancel.clone(), Arc::clone(&open));
            holders.spawn(async move {
                let mut tally = LiveStreamTally::default();
                while !stop.is_cancelled() {
                    hold_one(&queries, &open, &mut tally, live).await;
                }
                tally
            });
        }
        Self {
            cancel,
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

/// Issues one live stream over `live`, holds it, drains it, and records it.
async fn hold_one(
    queries: &Bifrost,
    open: &AtomicUsize,
    tally: &mut LiveStreamTally,
    live: (i64, i64),
) {
    let request = BifrostQueryRequest {
        sql: format!(
            "SELECT event_id FROM {} WHERE event_id >= {} AND event_id < {}",
            workload::TABLE,
            live.0,
            live.1
        ),
        deadline_ms: Some(QUERY_DEADLINE_MS),
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
                if let Ok(rows) = int_rows(&batch) {
                    ids.extend(rows.into_iter().filter_map(|row| row.first().copied()));
                }
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
        Some(terminal) if terminal.query_class != QueryClass::Interactive => {
            tally.not_interactive += 1;
        }
        Some(terminal)
            if drained
                && terminal.outcome == QueryTerminalOutcome::Success
                && ids.iter().copied().eq(live.0..live.1) =>
        {
            tally.completed += 1;
        }
        _ => tally.failed += 1,
    }
}

/// Issues `case` for `sequence` and checks its exact answer and terminal.
///
/// Latency starts where the scheduler polls this future, immediately before
/// the request is sent; the first-row instant is the first non-empty batch.
async fn run_case(
    queries: &Bifrost,
    fixture: Fixture,
    answers: &Answers,
    case: Case,
    sequence: u64,
) -> ProbeResult {
    let request = BifrostQueryRequest {
        sql: case.sql(fixture, sequence),
        deadline_ms: Some(QUERY_DEADLINE_MS),
    };
    probe(queries, &request, |rows| match case {
        Case::Selective => *rows == case.expected(fixture, sequence),
        _ => answers
            .get(&case)
            .and_then(|variants| variants.get((sequence % case.variants()) as usize))
            .is_some_and(|expected| rows == expected),
    })
    .await
}

/// A full-queue waiter: one selective query with the long waiter deadline.
async fn run_waiter(queries: &Bifrost, fixture: Fixture, sequence: u64) -> ProbeResult {
    let request = BifrostQueryRequest {
        sql: Case::Selective.sql(fixture, sequence),
        deadline_ms: Some(WAITER_DEADLINE_MS),
    };
    probe(queries, &request, |rows| {
        *rows == Case::Selective.expected(fixture, sequence)
    })
    .await
}

/// Runs `request` to its terminal and categorises it, checking the rows with
/// `exact` on a `Success` terminal.
async fn probe(
    queries: &Bifrost,
    request: &BifrostQueryRequest,
    exact: impl FnOnce(&Rows) -> bool,
) -> ProbeResult {
    let mut stream = match queries.query(request).await {
        Ok(stream) => stream,
        Err(error) => {
            return ProbeResult {
                outcome: classify_error(&error),
                first_row_at: None,
            };
        }
    };
    let mut rows = Vec::new();
    let mut first_row_at = None;
    let mut convertible = true;
    loop {
        match stream.next_batch().await {
            Ok(Some(batch)) => {
                if batch.num_rows() > 0 {
                    first_row_at.get_or_insert_with(Instant::now);
                }
                match int_rows(&batch) {
                    Ok(batch_rows) => rows.extend(batch_rows),
                    Err(_) => convertible = false,
                }
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
    let terminal = stream.terminal();
    let outcome = match terminal.map(|terminal| terminal.outcome) {
        Some(QueryTerminalOutcome::Success) if convertible && exact(&rows) => {
            ShortQueryOutcome::Success
        }
        Some(QueryTerminalOutcome::Success) => ShortQueryOutcome::WrongResult,
        Some(QueryTerminalOutcome::Degraded) => ShortQueryOutcome::Degraded,
        Some(QueryTerminalOutcome::Failed)
            if terminal
                .and_then(|terminal| terminal.error.as_ref())
                .is_some_and(|error| error.code == QueryTerminalErrorCode::QueryTimeout) =>
        {
            ShortQueryOutcome::Deadline
        }
        Some(QueryTerminalOutcome::Failed)
            if terminal
                .and_then(|terminal| terminal.error.as_ref())
                .is_some_and(|error| error.code == QueryTerminalErrorCode::QueryPeerSecurity) =>
        {
            ShortQueryOutcome::SecurityRefused
        }
        Some(QueryTerminalOutcome::Failed) => ShortQueryOutcome::Failed,
        None => ShortQueryOutcome::TransportError,
    };
    ProbeResult {
        outcome,
        first_row_at,
    }
}

/// Casts every column of `batch` to `i64` and returns its rows.
///
/// # Errors
///
/// Returns the cast error for a column that is not integral.
fn int_rows(batch: &RecordBatch) -> Result<Rows, arrow::error::ArrowError> {
    let columns = batch
        .columns()
        .iter()
        .map(|column| arrow::compute::cast(column, &DataType::Int64))
        .collect::<Result<Vec<_>, _>>()?;
    let columns = columns
        .iter()
        .map(|column| column.as_primitive::<Int64Type>())
        .collect::<Vec<_>>();
    Ok((0..batch.num_rows())
        .map(|row| columns.iter().map(|column| column.value(row)).collect())
        .collect())
}

/// Runs `sql` to a `Success` terminal and returns its integer rows.
///
/// # Errors
///
/// Returns client failures, and [`CapacityError::Validation`] for any other
/// terminal or a non-integral column.
async fn collect(queries: &Bifrost, sql: &str) -> Result<Rows, CapacityError> {
    let mut stream = queries
        .query(&BifrostQueryRequest {
            sql: sql.to_owned(),
            deadline_ms: Some(QUERY_DEADLINE_MS),
        })
        .await?;
    let mut rows = Vec::new();
    while let Some(batch) = stream.next_batch().await? {
        rows.extend(int_rows(&batch)?);
    }
    match stream.terminal().map(|terminal| terminal.outcome) {
        Some(QueryTerminalOutcome::Success) => Ok(rows),
        other => Err(CapacityError::Validation(format!(
            "`{sql}` ended with {other:?}"
        ))),
    }
}

/// Maps a client error onto its report category by its stable code, and
/// logs it at debug level so a failing category can be diagnosed.
fn classify_error(error: &BifrostClientError) -> ShortQueryOutcome {
    tracing::debug!(%error, "benchmark query failed");
    if let Some(terminal) = error.terminal() {
        return match terminal.error.as_ref().map(|error| error.code) {
            Some(QueryTerminalErrorCode::QueryTimeout) => ShortQueryOutcome::Deadline,
            Some(QueryTerminalErrorCode::QueryPeerSecurity) => ShortQueryOutcome::SecurityRefused,
            _ => ShortQueryOutcome::Failed,
        };
    }
    match wyrd_spec::error::WyrdError::from(error).code() {
        "WYRD_VALA_429_QUERY_ADMISSION_REJECTED" | "WYRD_VALA_429_QUERY_QUEUE_FULL" => {
            ShortQueryOutcome::AdmissionRefused
        }
        "WYRD_VALA_504_QUERY_TIMEOUT" => ShortQueryOutcome::Deadline,
        "WYRD_VALA_403_QUERY_PEER_SECURITY" => ShortQueryOutcome::SecurityRefused,
        _ => ShortQueryOutcome::TransportError,
    }
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

/// Returns the queried pod: the cluster's first.
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

/// Returns the remote Scribe pod: the cluster's second.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Resource`] when there is none.
fn scribe_pod(
    cluster: &mut BifrostProcessCluster,
) -> Result<&mut ProcessNode, ProcessClusterError> {
    cluster
        .nodes_mut()
        .get_mut(1)
        .ok_or_else(|| ProcessClusterError::Resource("the benchmark has no Scribe pod".to_owned()))
}

/// Reads one gauge series selected by one label.
///
/// # Errors
///
/// Returns cluster control failures.
fn gauge(
    node: &mut ProcessNode,
    family: &str,
    label: &str,
    value: &str,
) -> Result<f64, ProcessClusterError> {
    let labels = BTreeMap::from([(label.to_owned(), value.to_owned())]);
    Ok(
        tokio::task::block_in_place(|| node.metric_totals_labeled(&[family], &labels))?
            .get(family)
            .copied()
            .unwrap_or(0.0),
    )
}

/// Counts one table's Parquet objects, row groups, and bytes.
///
/// Walks the shared object store for `.parquet` files under a directory named
/// exactly `table`, reading each footer with standard Parquet parsing.
fn data_shape(storage_root: &Path, table: &str) -> DataShape {
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
                && path.components().any(|part| part.as_os_str() == table)
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

/// Creates `directory` and its parents.
///
/// # Errors
///
/// Returns [`CapacityError::Output`] when creation fails.
fn create_dir(directory: &Path) -> Result<(), CapacityError> {
    std::fs::create_dir_all(directory).map_err(|error| CapacityError::Output(error.to_string()))
}

/// Writes `value` as pretty JSON.
///
/// # Errors
///
/// Returns [`CapacityError::Output`] when encoding or writing fails.
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), CapacityError> {
    if let Some(parent) = path.parent() {
        create_dir(parent)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A small-aggregate row that meets its target and is valid.
    fn valid_row() -> ReadRow {
        ReadRow {
            workload: "small-aggregate".to_owned(),
            concurrency: 16,
            window_seconds: 60.0,
            outcomes: BTreeMap::from([(ShortQueryOutcome::Success, 9_000)]),
            qps: 150.0,
            latency_ms: [Some(10.0), Some(20.0), Some(30.0)],
            fixture_seed_seconds: Some(40.0),
            resources: Resources {
                cpus_used: 2.47,
                cpu_limit: 4.0,
                periods: 600.0,
                throttled_periods: 2.0,
                throttled_seconds: 0.389,
                peak_memory_bytes: 2.0 * 1_073_741_824.0,
                oom_kills: 0.0,
            },
            ..ReadRow::default()
        }
    }

    /// Judges `row` against the standard small-aggregate target.
    fn judged(mut row: ReadRow) -> ReadRow {
        row.judge(
            &Case::SmallAggregate.target(Fixture::standard()),
            false,
            None,
        );
        row
    }

    /// The report names the measured limit: a rare throttle under a mostly
    /// idle quota is not CPU saturation, and a deadline outside the queue, an
    /// execution admission refusal, a queue deadline, a memory refusal, and
    /// real CPU saturation are each named apart.
    ///
    /// The first case is the observed 2 of 600 throttled periods for 0.389 s
    /// at 2.47 of 4 CPUs over 60 seconds.
    ///
    /// # Panics
    ///
    /// Panics when any case is misnamed or not judged a failure.
    #[test]
    fn capacity_report_distinguishes_cpu_and_query_refusals() {
        assert!(!valid_row().resources.cpu_saturated(60.0));
        let failing = |outcome, reason: Option<&str>| {
            let mut row = valid_row();
            row.outcomes.insert(outcome, 5);
            if let Some(reason) = reason {
                row.server.rejected.insert(reason.to_owned(), 5.0);
            }
            let row = judged(row);
            assert_eq!(row.verdict, Verdict::Fail, "{}", row.reason);
            row.bottleneck
        };
        assert_eq!(
            failing(ShortQueryOutcome::Deadline, None),
            "snapshot or execution deadline"
        );
        assert_eq!(
            failing(ShortQueryOutcome::AdmissionRefused, Some("queue_full")),
            "oracle admission: queue_full"
        );
        assert_eq!(
            failing(ShortQueryOutcome::Deadline, Some("queue_deadline")),
            "oracle queue deadline"
        );
        assert_eq!(failing(ShortQueryOutcome::Failed, Some("memory")), "memory");

        let mut saturated = valid_row();
        saturated.latency_ms[1] = Some(150.0);
        saturated.resources.cpus_used = 3.9;
        saturated.resources.throttled_periods = 300.0;
        saturated.resources.throttled_seconds = 20.0;
        let saturated = judged(saturated);
        assert_eq!(saturated.verdict, Verdict::Fail);
        assert_eq!(saturated.bottleneck, "cgroup cpu");

        let mut slow = valid_row();
        slow.latency_ms[1] = Some(150.0);
        slow.server
            .phase_mean_ms
            .insert("metadata_load".to_owned(), 90.0);
        slow.server
            .phase_mean_ms
            .insert("admission".to_owned(), 1.0);
        assert_eq!(
            judged(slow).bottleneck,
            "server phase metadata_load (90.0 ms mean)"
        );
    }

    /// Invalid measurements are INVALID, a valid miss is FAIL, and the human
    /// report carries every read and ingest metric and the exact reason.
    ///
    /// # Panics
    ///
    /// Panics when an invalid row counts as a pass or failure, the degraded
    /// mixed row passes, or the rendered report omits a required field.
    #[test]
    fn invalid_capacity_rows_never_count_as_success() {
        assert_eq!(judged(valid_row()).verdict, Verdict::Pass);
        let invalid = |change: fn(&mut ReadRow)| {
            let mut row = valid_row();
            change(&mut row);
            let row = judged(row);
            assert_eq!(row.verdict, Verdict::Invalid, "{}", row.reason);
            row.reason
        };
        assert!(
            invalid(|row| {
                row.live_target = 4;
                row.live_open_min = 4;
                row.live = LiveStreamTally {
                    completed: 4,
                    refused: 1,
                    ..LiveStreamTally::default()
                };
            })
            .contains("live streams")
        );
        assert!(
            invalid(|row| {
                row.outcomes.insert(ShortQueryOutcome::WrongResult, 1);
            })
            .contains("1 wrong results")
        );
        assert!(invalid(|row| row.missed_launches = 3).contains("missed the offer"));
        assert!(invalid(|row| row.latency_ms[2] = None).contains("missing p50/p95/p99"));
        assert!(invalid(|row| row.concurrency = 0).contains("missing QPS concurrency"));
        let mut scan = valid_row();
        scan.judge(&Target::default(), true, None);
        assert_eq!(scan.verdict, Verdict::Invalid);
        assert!(scan.reason.contains("missing physical scan bytes"));

        let mut baseline = valid_row();
        baseline
            .case_p95_ms
            .insert("small-aggregate".to_owned(), 10.0);
        let mut mixed = valid_row();
        mixed.case_p95_ms.insert("small-aggregate".to_owned(), 12.0);
        mixed.judge(&mixed_target(), false, Some(&baseline));
        assert_eq!(mixed.verdict, Verdict::Fail);
        assert!(
            mixed.reason.contains("20% above read-only 10.0 ms"),
            "{}",
            mixed.reason
        );
        mixed.case_p95_ms.insert("small-aggregate".to_owned(), 11.9);
        mixed.judge(&mixed_target(), false, Some(&baseline));
        assert_eq!(mixed.verdict, Verdict::Pass);

        let mut ingest = IngestRow {
            workload: "seed-10m".to_owned(),
            writers: 4,
            rows_per_request: workload::REQUEST_ROWS,
            seconds: 40.0,
            acknowledged_rows: 10_000_000,
            rows_per_second: 250_000.0,
            input_bytes_per_second: 20_000_000.0,
            ack_ms: [Some(100.0), Some(210.5), Some(300.0)],
            files: 12,
            average_file_bytes: 50_000_000.0,
            read_back: true,
            resources: valid_row().resources,
            ..IngestRow::default()
        };
        ingest.judge();
        assert_eq!(ingest.verdict, Verdict::Pass);
        ingest.read_back = false;
        ingest.judge();
        assert_eq!(ingest.verdict, Verdict::Invalid);
        assert_eq!(ingest.reason, "acknowledged rows did not read back");

        let refused = judged(ReadRow {
            outcomes: BTreeMap::from([
                (ShortQueryOutcome::Success, 9_000),
                (ShortQueryOutcome::AdmissionRefused, 7),
            ]),
            ..valid_row()
        });
        assert_eq!(
            SweepSummary::new("small-aggregate", std::slice::from_ref(&refused), 100.0).verdict,
            Verdict::Fail
        );
        let report = Report {
            ingest: vec![ingest],
            reads: vec![refused],
            ..Report::default()
        };
        let rendered = report.render();
        for expected in [
            "seed-10m",
            "250000",
            "20.0",
            "100.0/210.5/300.0",
            "150.0",
            "10.0/20.0/30.0",
            "2.47/4",
            "2.00",
            "INVALID acknowledged rows did not read back",
            "FAIL 7 queries did not succeed | oracle",
        ] {
            assert!(
                rendered.contains(expected),
                "missing {expected}:\n{rendered}"
            );
        }
        assert_eq!(report.failures().len(), 2);
    }

    /// A heavy scan row passes only with its exact answer, the physical scan
    /// counter, and a known seed duration; a slow scan rate is a failure.
    ///
    /// # Panics
    ///
    /// Panics when any missing evidence still passes or the slow scan passes.
    #[test]
    fn heavy_scan_requires_exact_result_and_physical_bytes() {
        let heavy = Fixture::heavy();
        let target = Case::FullScan.target(heavy);
        let row = || ReadRow {
            workload: "full-scan".to_owned(),
            concurrency: 1,
            outcomes: BTreeMap::from([(ShortQueryOutcome::Success, 3)]),
            latency_ms: [Some(1_500.0), Some(1_600.0), Some(1_600.0)],
            scan_bytes_per_second: Some(800_000_000.0),
            fixture_seed_seconds: Some(400.0),
            ..valid_row()
        };
        let judged = |mut row: ReadRow| {
            row.judge(&target, true, None);
            row
        };
        assert_eq!(judged(row()).verdict, Verdict::Pass);
        let wrong = judged(ReadRow {
            outcomes: BTreeMap::from([
                (ShortQueryOutcome::Success, 2),
                (ShortQueryOutcome::WrongResult, 1),
            ]),
            ..row()
        });
        assert_eq!(wrong.verdict, Verdict::Invalid);
        assert_eq!(wrong.bottleneck, "result correctness");
        let unscanned = judged(ReadRow {
            scan_bytes_per_second: None,
            ..row()
        });
        assert_eq!(unscanned.verdict, Verdict::Invalid);
        let unseeded = judged(ReadRow {
            fixture_seed_seconds: None,
            ..row()
        });
        assert_eq!(unseeded.verdict, Verdict::Invalid);
        assert!(unseeded.reason.contains("missing fixture seed duration"));
        let slow = judged(ReadRow {
            scan_bytes_per_second: Some(300_000_000.0),
            ..row()
        });
        assert_eq!(slow.verdict, Verdict::Fail);
        assert!(
            slow.reason.contains("scan 300 MB/s < 500 MB/s"),
            "{}",
            slow.reason
        );
    }

    /// Exposition, `cpu.max`, and cgroup files parse into the deltas a row
    /// reports.
    ///
    /// # Panics
    ///
    /// Panics when a series, label, limit, or reading is misparsed.
    #[test]
    fn server_evidence_parses_exposition_and_cgroup_files() {
        let before = Snapshot {
            metrics: exposition(
                "oracle_admission_total{class=\"interactive\",outcome=\"rejected\",reason=\"queue_full\"} 1\n",
            ),
            cgroup: cgroup_values("cpu.stat", "usage_usec 1000000\nnr_periods 10\n")
                .into_iter()
                .collect(),
            cpu_limit: 0.0,
        };
        let after = Snapshot {
            metrics: exposition(
                "# TYPE x counter\n\
                 oracle_admission_total{class=\"interactive\",outcome=\"rejected\",reason=\"queue_full\"} 4\n\
                 oracle_admission_total{class=\"analytical\",outcome=\"admitted\",reason=\"none\"} 9\n\
                 oracle_query_bytes_scanned_total{class=\"interactive\"} 2048\n\
                 oracle_query_phase_seconds_sum{phase=\"hot_cut\"} 0.004\n\
                 oracle_query_phase_seconds_count{phase=\"hot_cut\"} 2\n\
                 wyrd_postgres_pool_acquire_seconds_sum{outcome=\"acquired\",pool=\"app\"} 0.01\n\
                 wyrd_postgres_pool_acquire_seconds_count{outcome=\"acquired\",pool=\"app\"} 5\n",
            ),
            cgroup: cgroup_values("cpu.stat", "usage_usec 5000000\nnr_periods 30\n")
                .into_iter()
                .chain(cgroup_values("memory.peak", "4096\n"))
                .collect(),
            cpu_limit: cpu_limit("400000 100000\n"),
        };
        let server = after.server_since(&before);
        assert_eq!(
            server.rejected,
            BTreeMap::from([("queue_full".to_owned(), 3.0)])
        );
        assert_eq!(server.scan_bytes, Some(2048.0));
        assert_eq!(server.phase_mean_ms.get("hot_cut"), Some(&2.0));
        assert_eq!(server.pool_wait_mean_ms, Some(2.0));
        let resources = after.resources_since(&before, 2.0);
        assert!((resources.cpus_used - 2.0).abs() < f64::EPSILON);
        assert!((resources.cpu_limit - 4.0).abs() < f64::EPSILON);
        assert!((resources.periods - 20.0).abs() < f64::EPSILON);
        assert!((resources.peak_memory_bytes - 4096.0).abs() < f64::EPSILON);
        assert_eq!(cpu_limit("max 100000"), 0.0);
    }
}
