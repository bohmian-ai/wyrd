//! Benchmarks as declared experiments: one workflow from declaration to a
//! persisted, regenerable record, shared by every engine.
//!
//! A run follows one order. A [`Declaration`] fixes the question, level,
//! sweep, controls, metric units and boundaries, criteria, validity
//! thresholds, and budget, and is sealed before any
//! workload runs. [`run`] then checks quiet-host eligibility, prepares the
//! [`Workload`], discards each step's warmup, measures three repetitions,
//! samples the host again, and judges every applicable criterion into a
//! [`Record`]. A clean, unprofiled full run of the whole plan publishes its
//! record append-only under [`HISTORY_DIRECTORY`]`/<engine>/<experiment>/`,
//! any other full run under [`LOCAL_DIRECTORY`], and every report is
//! rendered from it.
//! A [`Mode::Smoke`] runs that same workflow at reduced declared controls
//! and keeps its record under [`SMOKE_DIRECTORY`].
//!
//! A run is VALID or INVALID. VALID means it measured what it claims and
//! every correctness and integrity criterion ([`Criterion::every`]) held in
//! every repetition; INVALID names every reason it did not. VALID exits 0
//! and INVALID exits 2; nothing exits 1. A performance target
//! ([`Criterion::limit`]) judges the median repetition and reports
//! [`Outcome::Met`], [`Outcome::Missed`] or [`Outcome::Unsettled`]; it never
//! changes validity or the exit. A missing, reset, or unresolved reading is
//! INVALID evidence, never a favorable zero.
//!
//! Engines supply only their [`Workload`] (production setup and client
//! calls) and declarations; this module owns lifecycle, validity, judgment,
//! history, and reporting. Profiling through [`crate::profile`] is optional
//! diagnostic evidence that never changes a verdict.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::future::Future;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::profile::{HeapProfiler, ProfileMode, ProfileNote, Profiler};
use crate::release_server::{Host, HostLoad};

/// The only record version this module writes and reads: a VALID or
/// INVALID run whose targets report met, missed or unsettled, with server
/// timings from bounded histogram snapshots and percentiles as bucket bounds.
pub const SCHEMA_VERSION: u64 = 3;

/// Repetitions every measured step keeps.
pub const REPETITIONS: u8 = 3;

/// Where smoke records and evidence live, under the repository root:
/// local build output, never committed history.
pub const SMOKE_DIRECTORY: &str = "target/benchmarks/smoke";

/// Where history lives, under the repository root: the worktree of the
/// `benchmark-records` branch, holding `<engine>/<experiment>/` records.
pub const HISTORY_DIRECTORY: &str = "benchmarks/history";

/// Where a full run that may not enter history keeps its record, under the
/// repository root, beside its raw artifacts.
pub const LOCAL_DIRECTORY: &str = "target/benchmarks";

/// How long a workload may take to stop and drain after measurement before
/// the run gives up waiting and records the overrun.
const FINISH_GRACE: Duration = Duration::from_secs(90);

/// Why an experiment step could not run, publish, or be read.
#[derive(Debug, thiserror::Error)]
pub enum ExperimentError {
    /// The workload's server, client, or fixture failed.
    #[error("workload failed: {0}")]
    Workload(String),
    /// The record could not be written or published.
    #[error("record could not be persisted at {path}: {source}")]
    Persist {
        /// The file or directory that failed.
        path: PathBuf,
        /// The filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// The record is not a well-formed record of a version this reader reads.
    #[error("record is not a well-formed v3 record: {0}")]
    Malformed(String),
    /// The record names a schema version this reader does not interpret.
    #[error("record schema_version {0} is not supported; this reader reads version 3 only")]
    Unsupported(u64),
}

impl ExperimentError {
    /// Wraps any workload-side failure.
    pub fn workload(error: impl std::fmt::Display) -> Self {
        Self::Workload(error.to_string())
    }
}

/// The three benchmark engines; a record's path starts with this name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// Bifrost ingest and query.
    Bifrost,
    /// The verification runtime.
    Verification,
    /// The AI gateway.
    Gateway,
}

impl Engine {
    /// The engine's path-safe lowercase name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bifrost => "bifrost",
            Self::Verification => "verification",
            Self::Gateway => "gateway",
        }
    }
}

/// The experiment level: L1 isolated closed-loop work, or L2 open-loop
/// competing work. Persisted as the integer 1 or 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
pub enum Level {
    /// Fast isolated work: 5 s warmup, 3 × 5 s, at most 5 minutes.
    One,
    /// Competing workload: 2 min warmup, 3 × 6 min, 20 to 30 minutes.
    Two,
}

impl From<Level> for u8 {
    /// The persisted level integer.
    fn from(level: Level) -> Self {
        match level {
            Level::One => 1,
            Level::Two => 2,
        }
    }
}

impl TryFrom<u8> for Level {
    type Error = String;

    /// Reads a persisted level integer.
    ///
    /// # Errors
    ///
    /// Returns a message for any integer other than 1 or 2.
    fn try_from(level: u8) -> Result<Self, Self::Error> {
        match level {
            1 => Ok(Self::One),
            2 => Ok(Self::Two),
            other => Err(format!("level must be 1 or 2, got {other}")),
        }
    }
}

impl Level {
    /// The level's fixed warmup, repetition, budget, and minimum durations.
    #[must_use]
    pub const fn timing(self) -> Timing {
        match self {
            Self::One => Timing {
                warmup_seconds: 5,
                repetition_seconds: 5,
                repetitions: REPETITIONS,
                budget_seconds: 5 * 60,
                minimum_seconds: 0,
                phases: None,
            },
            Self::Two => Timing {
                warmup_seconds: 120,
                repetition_seconds: 360,
                repetitions: REPETITIONS,
                budget_seconds: 30 * 60,
                minimum_seconds: 20 * 60,
                phases: None,
            },
        }
    }

    /// The level's reduced smoke durations: one short discarded warmup and
    /// one short measured repetition per step, within a bounded smoke
    /// budget and no minimum. L2 windows stay long enough for every
    /// capacity, overload, and recovery phase to admit each declared case.
    #[must_use]
    pub const fn smoke_timing(self) -> Timing {
        match self {
            Self::One => Timing {
                warmup_seconds: 1,
                repetition_seconds: 3,
                repetitions: 1,
                budget_seconds: 10 * 60,
                minimum_seconds: 0,
                phases: None,
            },
            Self::Two => Timing {
                warmup_seconds: 3,
                repetition_seconds: 36,
                repetitions: 1,
                budget_seconds: 15 * 60,
                minimum_seconds: 0,
                phases: None,
            },
        }
    }

    /// The standardized L1 case clock: one 30 s warmup and three 30 s
    /// measured windows inside the [`Allowances::L1`] phase envelope, whose
    /// total is the case budget.
    #[must_use]
    pub const fn case_timing() -> Timing {
        Timing {
            warmup_seconds: 30,
            repetition_seconds: 30,
            repetitions: REPETITIONS,
            budget_seconds: Allowances::L1.total(),
            minimum_seconds: 0,
            phases: Some(Allowances::L1),
        }
    }

    /// The harness smoke of [`Self::case_timing`]: the same lifecycle,
    /// envelope and three windows, each 2 s after a 1 s warmup, so the
    /// harness lane exercises aggregation and drains without the
    /// performance clock.
    #[must_use]
    pub const fn case_smoke_timing() -> Timing {
        Timing {
            warmup_seconds: 1,
            repetition_seconds: 2,
            ..Self::case_timing()
        }
    }
}

/// The accepted per-case maximum allowance of each L1 lifecycle phase,
/// seconds. They are maxima, never sleeps, and each is shared by every
/// occurrence of its phase: the drain allowance covers every inter-window
/// drain and the final drain together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allowances {
    /// Host preflight, PostgreSQL creation, migrations, the fresh server
    /// and clients, fixture loading and exact fixture checks.
    pub setup_seconds: u64,
    /// The case's own discarded warmup.
    pub warmup_seconds: u64,
    /// The measured windows.
    pub measurement_seconds: u64,
    /// Every request drain, final reconciliation, storage settlement and
    /// evidence capture.
    pub drain_seconds: u64,
    /// Stopping and reaping owned processes and disposing of PostgreSQL,
    /// WAL and object storage.
    pub teardown_seconds: u64,
    /// Finalizing and atomically keeping the case record.
    pub persist_seconds: u64,
}

impl Allowances {
    /// The accepted L1 envelope: 240 + 30 + 90 + 150 + 60 + 20 = 590 s.
    pub const L1: Self = Self {
        setup_seconds: 240,
        warmup_seconds: 30,
        measurement_seconds: 90,
        drain_seconds: 150,
        teardown_seconds: 60,
        persist_seconds: 20,
    };

    /// The whole envelope, seconds.
    #[must_use]
    pub const fn total(self) -> u64 {
        self.setup_seconds
            + self.warmup_seconds
            + self.measurement_seconds
            + self.drain_seconds
            + self.teardown_seconds
            + self.persist_seconds
    }
}

/// Whether a run is a full experiment or a smoke of the same workflow at
/// reduced declared controls.
///
/// A smoke runs the same declaration, fixtures, steps, phases,
/// measurement, validity, and record as a full run with a smaller fixture
/// and [`Level::smoke_timing`]. It only shows the workflow runs: it judges
/// no criterion, so it is VALID or INVALID, never qualifies performance, and its records stay under
/// `target/benchmarks/smoke/`. A record without a mode is a full run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// The full declared experiment.
    #[default]
    Full,
    /// The reduced functional smoke.
    Smoke,
}

/// The step parameter that declares a full run's longer repetition
/// window, seconds, for a step the default window cannot give enough
/// samples; see [`Declaration::step_timing`].
pub const WINDOW_SECONDS: &str = "window_seconds";

/// The durations every step of a run uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timing {
    /// Discarded warmup before each step's repetitions.
    pub warmup_seconds: u64,
    /// Length of each measured repetition.
    pub repetition_seconds: u64,
    /// Measured repetitions per step.
    pub repetitions: u8,
    /// Hard budget from the start of the clock to the end of drain.
    pub budget_seconds: u64,
    /// Least elapsed time a complete run must take; zero when unbounded.
    pub minimum_seconds: u64,
    /// The L1 case protocol's phase allowances, which [`run`] enforces as
    /// absolute phase deadlines; `None` for a run outside the protocol,
    /// which only its budget bounds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phases: Option<Allowances>,
}

/// The driver's declared CPU budget in cores: the [`Validity::driver_cores`]
/// every run is judged against, and the worker count of the driver's runtime.
pub const DRIVER_CORES: u16 = 8;

/// The thresholds every run's validity checks use.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Validity {
    /// Largest fraction of logical CPUs non-benchmark processes may use in
    /// either host sample.
    pub host_busy_fraction: f64,
    /// Length of each host sample, seconds.
    pub host_sample_seconds: u64,
    /// The driver's declared CPU budget, cores.
    pub driver_cores: f64,
    /// Largest fraction of that budget the driver may use.
    pub driver_fraction: f64,
    /// Largest open-loop send-lateness p99, microseconds.
    pub lateness_p99_us: f64,
    /// Least samples per repetition behind each p99 criterion.
    pub p99_samples: u64,
    /// Least samples per repetition behind each p95 criterion.
    pub p95_samples: u64,
}

impl Default for Validity {
    /// The approved thresholds: 10% busy host over 10 s samples, 80% of an
    /// 8-core driver budget, 5 ms lateness, 1,000 p99 and 200 p95 samples.
    fn default() -> Self {
        Self {
            host_busy_fraction: 0.10,
            host_sample_seconds: 10,
            driver_cores: f64::from(DRIVER_CORES),
            driver_fraction: 0.80,
            lateness_p99_us: 5_000.0,
            p99_samples: 1_000,
            p95_samples: 200,
        }
    }
}

/// Which way a metric improves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Larger is better, such as work per CPU-second.
    Higher,
    /// Smaller is better, such as CPU per operation.
    Lower,
}

/// A limit criterion's comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    /// Strictly below the limit; equality fails.
    Lt,
    /// At or below the limit.
    Le,
    /// Strictly above the limit; equality fails.
    Gt,
    /// At or above the limit.
    Ge,
}

impl Op {
    /// The operator's symbol in a report.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Lt => "<",
            Self::Le => "≤",
            Self::Gt => ">",
            Self::Ge => "≥",
        }
    }

    /// Judges a value known to lie in `lo..=hi` (or above `lo` when `open`)
    /// against `limit`: `Some(true)` when every value in the interval meets
    /// it, `Some(false)` when none does, `None` when the interval straddles
    /// the limit. A missing `hi` is unbounded above.
    fn judge(self, lo: f64, open: bool, hi: Option<f64>, limit: f64) -> Option<bool> {
        let below = |strict: bool| match hi {
            Some(hi) if (strict && hi < limit) || (!strict && hi <= limit) => Some(true),
            _ if (open && lo >= limit) || (!open && (lo > limit || (strict && lo >= limit))) => {
                Some(false)
            }
            _ => None,
        };
        let above = |strict: bool| {
            if (open && lo >= limit)
                || (!open && ((strict && lo > limit) || (!strict && lo >= limit)))
            {
                return Some(true);
            }
            match hi {
                Some(hi) if (strict && hi <= limit) || (!strict && hi < limit) => Some(false),
                _ => None,
            }
        };
        match self {
            Self::Lt => below(true),
            Self::Le => below(false),
            Self::Gt => above(true),
            Self::Ge => above(false),
        }
    }
}

/// How a metric of one repetition is derived from its raw readings and
/// server CPU; derivation is declared, so a workload cannot pick it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Derive {
    /// Mean used server cores, `C/T`.
    Cores,
    /// Work per server CPU-second, `W/C × scale`.
    PerCpu {
        /// The raw work reading.
        work: String,
        /// Unit factor applied to the quotient.
        scale: f64,
    },
    /// Server CPU per unit of work, `C/W × scale`.
    CpuPer {
        /// The raw work reading.
        work: String,
        /// Unit factor applied to the quotient, such as 1e6 for µs.
        scale: f64,
    },
    /// Work per wall second per provisioned vCPU, `(W/T)/P × scale`:
    /// throughput against the CPUs available to the server, not those used.
    PerVcpu {
        /// The raw work reading.
        work: String,
        /// Unit factor applied to the quotient, such as `1/2^20` for MiB.
        scale: f64,
    },
    /// Work per wall second, `W/T × scale`.
    Rate {
        /// The raw work reading.
        work: String,
        /// Unit factor applied to the quotient.
        scale: f64,
    },
    /// Sampled peak server memory per achieved work rate, `M/(W/T)`.
    MemoryPer {
        /// The raw work reading.
        work: String,
    },
}

/// One declared metric: its stable id, explicit unit, timing or counting
/// boundary, preferred direction, and derivation when it is not raw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metric {
    /// Stable identifier, such as `p99_ms` or `rows_per_cpu_s`.
    pub id: String,
    /// Explicit unit, such as `ms` or `rows/cpu-s`.
    pub unit: String,
    /// What the metric spans or counts, for a literal reader.
    pub boundary: String,
    /// Which way the metric improves, which a reader comparing runs uses.
    pub direction: Option<Direction>,
    /// How the metric is derived, or `None` for a raw workload reading.
    pub derive: Option<Derive>,
}

/// The id prefix of a diagnostic reading: `diagnostic.<observation>.<stat>`,
/// one [`DIAGNOSTIC_STATS`] reading of one declared observation, such as one
/// Gateway phase. No criterion judges one; the report shows each per
/// repetition under [`Record::render`]'s `diagnostics` detail.
pub const DIAGNOSTIC: &str = "diagnostic.";

/// The readings of one diagnostic observation, by stat id and unit.
pub const DIAGNOSTIC_STATS: [(&str, &str); 5] = [
    ("count", "observations"),
    ("p50_us", "µs"),
    ("p95_us", "µs"),
    ("p99_us", "µs"),
    ("mean_us", "µs"),
];

impl Metric {
    /// The [`DIAGNOSTIC_STATS`] readings of diagnostic `observation` over
    /// `boundary`, which names its interval, population, and whether it
    /// overlaps another observation.
    #[must_use]
    pub fn diagnostic(observation: &str, boundary: &str) -> Vec<Self> {
        DIAGNOSTIC_STATS
            .map(|(stat, unit)| {
                Self::raw(&format!("{DIAGNOSTIC}{observation}.{stat}"), unit, boundary)
            })
            .to_vec()
    }

    /// A raw metric the workload reads.
    #[must_use]
    pub fn raw(id: &str, unit: &str, boundary: &str) -> Self {
        Self {
            id: id.to_owned(),
            unit: unit.to_owned(),
            boundary: boundary.to_owned(),
            direction: None,
            derive: None,
        }
    }

    /// `cores`: the window's mean busy server cores `C/T`, a descriptive
    /// reading beside provisioned `P` that no criterion compares.
    #[must_use]
    pub fn cores() -> Self {
        Self {
            derive: Some(Derive::Cores),
            ..Self::raw(
                "cores",
                "cores",
                "C/T: server CPU-seconds of the declared processes per wall second of the window",
            )
        }
    }

    /// A metric derived by `derive`, improving in `direction`.
    #[must_use]
    pub fn derived(
        id: &str,
        unit: &str,
        boundary: &str,
        direction: Direction,
        derive: Derive,
    ) -> Self {
        Self {
            id: id.to_owned(),
            unit: unit.to_owned(),
            boundary: boundary.to_owned(),
            direction: Some(direction),
            derive: Some(derive),
        }
    }
}

/// Which repetitions a limit criterion judges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Over {
    /// The median repetition: numeric criteria.
    Median,
    /// Every repetition: correctness and integrity criteria.
    Every,
}

/// How a criterion judges: an absolute comparison over the median
/// repetition for a target, or over every repetition for correctness and
/// integrity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    /// The comparison.
    pub op: Op,
    /// The limit, in the metric's unit.
    pub limit: f64,
    /// Which repetitions it judges.
    pub over: Over,
}

/// One declared criterion: which metric, on which steps, judged how.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Criterion {
    /// Stable identifier.
    pub id: String,
    /// The metric judged.
    pub metric: String,
    /// The steps it applies to.
    pub steps: Vec<String>,
    /// How it judges.
    pub rule: Rule,
    /// The percentile the metric reads, which sets per-repetition sample
    /// sufficiency.
    pub percentile: Option<u8>,
}

impl Criterion {
    /// A median-repetition limit on `metric` over `steps`.
    #[must_use]
    pub fn limit(id: &str, metric: &str, steps: &[String], op: Op, limit: f64) -> Self {
        Self {
            id: id.to_owned(),
            metric: metric.to_owned(),
            steps: steps.to_vec(),
            rule: Rule {
                op,
                limit,
                over: Over::Median,
            },
            percentile: None,
        }
    }

    /// A limit that every repetition must meet: correctness and integrity.
    #[must_use]
    pub fn every(id: &str, metric: &str, steps: &[String], op: Op, limit: f64) -> Self {
        Self {
            rule: Rule {
                op,
                limit,
                over: Over::Every,
            },
            ..Self::limit(id, metric, steps, op, limit)
        }
    }

    /// The same criterion reading percentile `percentile` (95 or 99).
    #[must_use]
    pub fn percentile(mut self, percentile: u8) -> Self {
        self.percentile = Some(percentile);
        self
    }

    /// The criterion's rule as a report cell.
    #[must_use]
    pub fn describe(&self, unit: &str) -> String {
        let Rule { op, limit, over } = self.rule;
        format!(
            "{} {} {} {unit}{}",
            self.metric,
            op.symbol(),
            number(limit),
            if over == Over::Every {
                " every rep"
            } else {
                ""
            }
        )
    }
}

/// What a declared step is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StepKind {
    /// Measured by the workload: warmup then repetitions.
    Measured,
    /// The measured step among `of` with the highest median `metric`,
    /// derived after measurement; sanity floors judge a sweep's peak.
    Peak {
        /// The sweep's steps.
        of: Vec<String>,
        /// The metric whose median picks the peak.
        metric: String,
    },
    /// `metric` of `scaled` divided by `factor` × `metric` of `base`, per
    /// repetition: `Q_N/(N × Q_1)`, with both inputs kept.
    Scaling {
        /// The one-client step.
        base: String,
        /// The N-client step.
        scaled: String,
        /// N.
        factor: f64,
        /// The rate metric compared.
        metric: String,
    },
}

/// One declared step: its id, parameters, and kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// Stable identifier within the experiment.
    pub id: String,
    /// The step's parameters, such as `clients` and the operation.
    pub params: BTreeMap<String, Value>,
    /// What the step is.
    pub kind: StepKind,
}

impl Step {
    /// A measured step.
    #[must_use]
    pub fn measured(id: &str, params: Value) -> Self {
        Self {
            id: id.to_owned(),
            params: object(params),
            kind: StepKind::Measured,
        }
    }

    /// A derived peak step over `of` by `metric`.
    #[must_use]
    pub fn peak(id: &str, of: &[String], metric: &str) -> Self {
        Self {
            id: id.to_owned(),
            params: BTreeMap::new(),
            kind: StepKind::Peak {
                of: of.to_vec(),
                metric: metric.to_owned(),
            },
        }
    }

    /// A derived scaling step `scaled / (factor × base)` of `metric`.
    #[must_use]
    pub fn scaling(id: &str, base: &str, scaled: &str, factor: f64, metric: &str) -> Self {
        Self {
            id: id.to_owned(),
            params: object(serde_json::json!({ "base": base, "scaled": scaled, "factor": factor })),
            kind: StepKind::Scaling {
                base: base.to_owned(),
                scaled: scaled.to_owned(),
                factor,
                metric: metric.to_owned(),
            },
        }
    }

    /// The step's integer parameter `name`, such as `clients`.
    #[must_use]
    pub fn number(&self, name: &str) -> Option<u64> {
        self.params.get(name).and_then(Value::as_u64)
    }

    /// The step's string parameter `name`, such as `op`.
    #[must_use]
    pub fn text(&self, name: &str) -> Option<&str> {
        self.params.get(name).and_then(Value::as_str)
    }
}

/// A JSON value's object fields, or none for any other value.
fn object(value: Value) -> BTreeMap<String, Value> {
    match value {
        Value::Object(fields) => fields.into_iter().collect(),
        _ => BTreeMap::new(),
    }
}

/// The measured source, read once at declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// Full measured-source SHA; empty when Git could not say.
    pub commit: String,
    /// Whether tracked files differed from `HEAD` at declaration.
    pub dirty: bool,
    /// Whether the run declared a dirty-tree override.
    pub dirty_override: bool,
    /// SHA-256 of the bytes of `git diff HEAD`, when dirty.
    pub diff_sha256: Option<String>,
}

impl Source {
    /// Reads `HEAD` and its tracked diff in `repository`, with the declared
    /// `dirty_override`. Untracked files, such as earlier records, do not
    /// dirty the source; Git failures leave the commit empty, which refuses
    /// the run.
    #[must_use]
    pub fn detect(repository: &Path, dirty_override: bool) -> Self {
        let git = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(repository)
                .args(args)
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| output.stdout)
        };
        let commit = git(&["rev-parse", "HEAD"])
            .map(|out| String::from_utf8_lossy(&out).trim().to_owned())
            .unwrap_or_default();
        let diff = git(&["diff", "HEAD"]).unwrap_or_default();
        Self::from_diff(commit, &diff, dirty_override)
    }

    /// The source of `commit` whose tracked diff is `diff`.
    #[must_use]
    pub fn from_diff(commit: String, diff: &[u8], dirty_override: bool) -> Self {
        let dirty = !diff.is_empty();
        Self {
            commit,
            dirty,
            dirty_override,
            diff_sha256: dirty.then(|| hex(&Sha256::digest(diff))),
        }
    }
}

/// Lowercase hexadecimal of `bytes`.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// The host a run measured on: metadata, never an efficiency denominator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostRecord {
    /// Operating system.
    pub os: String,
    /// Operating system version.
    pub os_version: String,
    /// CPU architecture.
    pub arch: String,
    /// CPU model.
    pub cpu_model: String,
    /// Logical CPUs.
    pub logical_cpus: usize,
    /// Physical memory, bytes.
    pub memory_bytes: u64,
    /// Declared storage context of the measured servers.
    pub storage: String,
    /// The one-line host fingerprint the record reports.
    pub fingerprint: String,
}

impl HostRecord {
    /// Records `host` with its declared `storage` context.
    #[must_use]
    pub fn new(host: &Host, storage: &str) -> Self {
        Self {
            os: host.os.to_owned(),
            os_version: host.os_version.clone(),
            arch: host.arch.to_owned(),
            cpu_model: host.cpu_model.clone(),
            logical_cpus: host.cpus,
            memory_bytes: host.memory_bytes,
            storage: storage.to_owned(),
            fingerprint: host.describe(),
        }
    }
}

/// The invocation a record reproduces: argument values plus the non-secret
/// configuration a rerun needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invocation {
    /// The arguments after the program name.
    pub args: Vec<String>,
    /// Configuration names and values; secret-bearing names keep only a
    /// symbolic value.
    pub config: BTreeMap<String, String>,
}

impl Invocation {
    /// Records `args` and `config`, replacing the value of any name that
    /// can carry a secret or private endpoint with `<set>`.
    #[must_use]
    pub fn new(args: &[String], config: &[(&str, &str)]) -> Self {
        const SENSITIVE: [&str; 7] = [
            "KEY",
            "TOKEN",
            "SECRET",
            "PASSWORD",
            "CREDENTIAL",
            "URL",
            "DSN",
        ];
        Self {
            args: args.to_vec(),
            config: config
                .iter()
                .map(|(name, value)| {
                    let upper = name.to_ascii_uppercase();
                    let value = if SENSITIVE.iter().any(|word| upper.contains(word)) {
                        "<set>".to_owned()
                    } else {
                        (*value).to_owned()
                    };
                    ((*name).to_owned(), value)
                })
                .collect(),
        }
    }
}

/// Everything fixed before measurement. [`Declaration::seal`] freezes it;
/// a [`Sealed`] declaration has no mutators, so observed performance cannot
/// change a criterion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Declaration {
    /// The engine measured.
    pub engine: Engine,
    /// Stable lowercase path-safe experiment id.
    pub experiment: String,
    /// The level.
    pub level: Level,
    /// The one-sentence question.
    pub question: String,
    /// Warmup, repetition, and budget durations.
    pub timing: Timing,
    /// Validity thresholds.
    pub validity: Validity,
    /// The variable sweep, such as client counts.
    pub variables: BTreeMap<String, Value>,
    /// Fixed controls: fixture identity, size, seed, projection, server
    /// limits, workload mix, and anything else a rerun must match.
    pub controls: BTreeMap<String, Value>,
    /// Every metric, raw or derived.
    pub metrics: Vec<Metric>,
    /// Every criterion.
    pub criteria: Vec<Criterion>,
    /// Every declared step, in order.
    pub steps: Vec<Step>,
    /// The selected subset, or `None` for full default coverage.
    pub selection: Option<Vec<String>>,
    /// The measured source.
    pub source: Source,
    /// Build profile, flags, compiler, and binary identities.
    pub build: Value,
    /// The invocation.
    pub command: Invocation,
    /// The host.
    pub host: HostRecord,
    /// Declaration-time refusals, such as a missing calibration.
    pub refusals: Vec<Reason>,
    /// Full or smoke.
    pub mode: Mode,
}

/// Controls every declaration names.
const REQUIRED_CONTROLS: [&str; 3] = ["fixture", "limits", "workload"];

impl Declaration {
    /// The reasons this declaration cannot admit a workload: missing
    /// question, steps, controls, metrics, or source; criteria naming
    /// undeclared metrics or steps; a dirty source without override; and
    /// declaration-time refusals.
    #[must_use]
    pub fn refusals(&self) -> Vec<Reason> {
        let mut reasons = self.refusals.clone();
        let mut refuse = |code: &str, message: String| reasons.push(Reason::new(code, message));
        if self.question.trim().is_empty() {
            refuse("declaration.missing", "no question is declared".to_owned());
        }
        if self.experiment.is_empty()
            || !self
                .experiment
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            refuse(
                "declaration.missing",
                format!(
                    "experiment id `{}` is not lowercase and path-safe",
                    self.experiment
                ),
            );
        }
        if self.steps.is_empty() {
            refuse("declaration.missing", "no step is declared".to_owned());
        }
        if self.criteria.is_empty() {
            refuse("declaration.missing", "no criterion is declared".to_owned());
        }
        for control in REQUIRED_CONTROLS {
            if !self.controls.contains_key(control) {
                refuse(
                    "declaration.missing",
                    format!("control `{control}` is not declared"),
                );
            }
        }
        for criterion in &self.criteria {
            if !self
                .metrics
                .iter()
                .any(|metric| metric.id == criterion.metric)
            {
                refuse(
                    "declaration.missing",
                    format!(
                        "criterion `{}` names undeclared metric `{}`",
                        criterion.id, criterion.metric
                    ),
                );
            }
            for step in &criterion.steps {
                if !self.steps.iter().any(|declared| declared.id == *step) {
                    refuse(
                        "declaration.missing",
                        format!(
                            "criterion `{}` names undeclared step `{step}`",
                            criterion.id
                        ),
                    );
                }
            }
        }
        if self.source.commit.is_empty() {
            refuse(
                "declaration.missing",
                "the measured source commit is unknown".to_owned(),
            );
        }
        if let Some(phases) = self.timing.phases {
            let windows = u64::from(self.timing.repetitions) * self.timing.repetition_seconds;
            if self.timing.warmup_seconds > phases.warmup_seconds
                || windows > phases.measurement_seconds
                || self.timing.budget_seconds != phases.total()
            {
                refuse(
                    "declaration.timing",
                    format!(
                        "the case clock ({} s warmup, {} x {} s windows, {} s budget) does not fit its phase allowances ({} s warmup, {} s measurement, {} s total)",
                        self.timing.warmup_seconds,
                        self.timing.repetitions,
                        self.timing.repetition_seconds,
                        self.timing.budget_seconds,
                        phases.warmup_seconds,
                        phases.measurement_seconds,
                        phases.total()
                    ),
                );
            }
        }
        if self.source.dirty && !self.source.dirty_override {
            refuse(
                "source.dirty",
                "tracked files differ from HEAD; commit them or declare --allow-dirty".to_owned(),
            );
        }
        reasons
    }

    /// Freezes the declaration for [`run`].
    #[must_use]
    pub fn seal(self) -> Sealed {
        Sealed(self)
    }

    /// The metric `id`, if declared.
    #[must_use]
    pub fn metric(&self, id: &str) -> Option<&Metric> {
        self.metrics.iter().find(|metric| metric.id == id)
    }

    /// The durations step `step` measures with: the run's [`Timing`], with
    /// a full run's repetitions lengthened to the step's declared
    /// [`WINDOW_SECONDS`] parameter when it has one. A step declares a
    /// longer window when the run's default cannot meet its sample floor;
    /// a smoke keeps its short windows, and a case under the L1 protocol
    /// ([`Timing::phases`]) its fixed windows.
    #[must_use]
    pub fn step_timing(&self, step: &Step) -> Timing {
        let declared = step.params.get(WINDOW_SECONDS).and_then(Value::as_u64);
        match declared {
            Some(seconds) if self.mode == Mode::Full && self.timing.phases.is_none() => Timing {
                repetition_seconds: seconds,
                ..self.timing
            },
            _ => self.timing,
        }
    }

    /// Whether step `id` runs under the selection.
    fn selected(&self, id: &str) -> bool {
        self.selection
            .as_ref()
            .is_none_or(|selected| selected.iter().any(|step| step == id))
    }
}

/// A frozen declaration: readable, never mutable.
#[derive(Debug, Clone)]
pub struct Sealed(Declaration);

impl Sealed {
    /// The frozen declaration.
    #[must_use]
    pub fn declaration(&self) -> &Declaration {
        &self.0
    }
}

/// A structured reason for a literal reader: a stable code, where it
/// applies, and what happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reason {
    /// Stable code, such as `host.busy` or `criterion.violated`.
    pub code: String,
    /// The step it applies to.
    pub step: Option<String>,
    /// The criterion it applies to.
    pub criterion: Option<String>,
    /// What happened.
    pub message: String,
}

impl Reason {
    /// A run-level reason.
    #[must_use]
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            step: None,
            criterion: None,
            message: message.into(),
        }
    }

    /// The same reason attached to `step`.
    #[must_use]
    pub fn at(mut self, step: &str) -> Self {
        self.step = Some(step.to_owned());
        self
    }

    /// The same reason attached to `criterion`.
    #[must_use]
    pub fn of(mut self, criterion: &str) -> Self {
        self.criterion = Some(criterion.to_owned());
        self
    }
}

/// VALID or INVALID, ordered by precedence. Every [`Reason`] a run or step
/// keeps makes it INVALID; a missed target is an [`Outcome`], not a reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Verdict {
    /// The run measured what it claims, and every correctness and integrity
    /// criterion held in every repetition.
    Valid,
    /// The run did not measure what it claims, or correctness or integrity
    /// failed in a repetition.
    Invalid,
}

impl Verdict {
    /// The process exit code: 0 or 2.
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::Valid => 0,
            Self::Invalid => 2,
        }
    }

    /// The verdict's report label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Valid => "VALID",
            Self::Invalid => "INVALID",
        }
    }

    /// VALID when `reasons` is empty, else INVALID.
    fn of(reasons: &[Reason]) -> Self {
        if reasons.is_empty() {
            Self::Valid
        } else {
            Self::Invalid
        }
    }

    /// The worst of several exit codes, as the L1 aggregates combine
    /// experiments: 0 when every code is 0, else 2, so a child that exits
    /// any other way is INVALID.
    #[must_use]
    pub fn combine(codes: &[u8]) -> u8 {
        if codes.iter().all(|code| *code == 0) {
            0
        } else {
            2
        }
    }
}

/// One criterion's outcome on one step.
///
/// A performance target is met, missed or unsettled and never changes the
/// run's validity. A correctness or integrity criterion that is missed or
/// unsettled in any repetition, or any criterion whose evidence is missing,
/// is [`Outcome::Invalid`] and makes the run INVALID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// The whole reading interval satisfies the comparison.
    Met,
    /// The whole reading interval violates the comparison.
    Missed,
    /// A censored or straddling target reading cannot settle the comparison.
    Unsettled,
    /// The evidence cannot be judged, or a correctness or integrity
    /// criterion did not hold in every repetition.
    Invalid,
}

impl Outcome {
    /// The outcome's report label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Met => "met",
            Self::Missed => "missed",
            Self::Unsettled => "unsettled",
            Self::Invalid => "INVALID",
        }
    }
}

/// One reading: an exact value, a censored value known only to exceed
/// `above` (a bounded wait that expired), the interval a median of censored
/// readings leaves, or nothing with a reason. Never NaN or infinity.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    /// The exact value, or an interval's upper bound; `None` for a censored
    /// or missing reading.
    pub value: Option<f64>,
    /// A censored reading's exclusive lower bound; `None` for an exact
    /// value.
    pub above: Option<f64>,
    /// Samples behind a percentile.
    pub samples: Option<u64>,
    /// The nearest rank `ceil(q * N)` of a histogram percentile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u64>,
    /// Why the value is missing or inapplicable.
    pub reason: Option<String>,
    /// The window histogram a percentile was read from, kept so a case's
    /// windows pool into its aggregate percentile ([`Self::pooled`]);
    /// `None` for any other reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<Pool>,
}

/// One window's histogram behind a percentile [`Reading`]: the percentile,
/// its unit divisor, and the histogram's parameters and nonzero buckets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pool {
    /// The percentile, `0.0..=1.0`.
    pub q: f64,
    /// The divisor from recorded values to the reading's unit.
    pub per: f64,
    /// The histogram's lowest and highest trackable values and significant
    /// digits, as [`wyrd_benchmark::Distribution::bounds`].
    pub bounds: [u64; 3],
    /// The window's nonzero buckets.
    pub buckets: wyrd_benchmark::Buckets,
}

impl Pool {
    /// The pool of `distribution` read at `q` and divided by `per`.
    fn of(distribution: &wyrd_benchmark::Distribution, q: f64, per: f64) -> Self {
        Self {
            q,
            per,
            bounds: distribution.bounds(),
            buckets: distribution.buckets(),
        }
    }
}

impl Reading {
    /// An exact value; a non-finite value is missing.
    #[must_use]
    pub fn exact(value: f64) -> Self {
        if value.is_finite() {
            Self {
                value: Some(value),
                ..Self::default()
            }
        } else {
            Self::missing("the value was not finite")
        }
    }

    /// A count.
    #[must_use]
    pub fn count(count: u64) -> Self {
        Self::exact(count as f64)
    }

    /// A censored reading: a bounded wait expired, so the value is only
    /// known to exceed `bound`.
    #[must_use]
    pub fn above(bound: f64) -> Self {
        Self {
            above: Some(bound),
            ..Self::default()
        }
    }

    /// A missing or inapplicable reading.
    #[must_use]
    pub fn missing(reason: impl Into<String>) -> Self {
        Self {
            reason: Some(reason.into()),
            ..Self::default()
        }
    }

    /// The same reading with `samples` behind it.
    #[must_use]
    pub fn with_samples(mut self, samples: u64) -> Self {
        self.samples = Some(samples);
        self
    }

    /// A value known to lie in the integer interval `[low, high]`, divided
    /// by `per`: exact when the bounds meet, else the interval
    /// `((low - 1) / per, high / per]`, so a threshold passes or fails only
    /// when the whole interval is on one side of it.
    #[must_use]
    pub fn bounded(low: f64, high: f64, per: f64) -> Self {
        if low >= high {
            return Self::exact(low / per);
        }
        Self {
            value: Some(high / per),
            above: Some((low - 1.0) / per),
            ..Self::default()
        }
    }

    /// The bounds of a histogram `distribution`'s mean, each value divided
    /// by `per`; missing without an observation.
    #[must_use]
    pub fn mean(distribution: &wyrd_benchmark::Distribution, per: f64) -> Self {
        let samples = distribution.len();
        if samples == 0 {
            return Self::missing("no observation");
        }
        let (low, high) = distribution.total_bounds();
        Self::bounded(low as f64, high as f64, per * samples as f64).with_samples(samples)
    }

    /// Nearest-rank percentile `q` of a histogram `distribution`, each value
    /// divided by `per`, as the [`Self::bounded`] interval of its bucket,
    /// with the sample count and rank; missing without samples.
    #[must_use]
    pub fn quantile(distribution: &wyrd_benchmark::Distribution, q: f64, per: f64) -> Self {
        distribution.quantile(q).map_or_else(
            || Self::missing("no successful sample"),
            |bucket| Self {
                rank: Some(bucket.rank),
                pool: Some(Pool::of(distribution, q, per)),
                ..Self::bounded(bucket.lowest as f64, bucket.highest as f64, per)
                    .with_samples(distribution.len())
            },
        )
    }

    /// The case-level percentile of window readings that each kept their
    /// histogram: the windows' bucket counts pooled and read again at the
    /// shared percentile, as the [`Self::quantile`] bucket interval with the
    /// pooled sample count; never an average of window percentiles. A
    /// window without a kept histogram, or windows kept at a different
    /// percentile, unit or histogram shape, leave the aggregate missing.
    #[must_use]
    pub fn pooled(readings: &[Self]) -> Self {
        let mut pooled: Option<(f64, f64, [u64; 3], wyrd_benchmark::Distribution)> = None;
        for (index, reading) in readings.iter().enumerate() {
            let Some(pool) = &reading.pool else {
                return Self::missing(format!("window {} kept no histogram to pool", index + 1));
            };
            let window = match wyrd_benchmark::Distribution::restore(pool.bounds, &pool.buckets) {
                Ok(window) => window,
                Err(error) => {
                    return Self::missing(format!("window {}: {error}", index + 1));
                }
            };
            match &mut pooled {
                None => pooled = Some((pool.q, pool.per, pool.bounds, window)),
                Some((q, per, bounds, distribution)) => {
                    if (*q, *per, *bounds) != (pool.q, pool.per, pool.bounds) {
                        return Self::missing(format!(
                            "window {} kept another percentile, unit or histogram",
                            index + 1
                        ));
                    }
                    if let Err(error) = distribution.add(&window) {
                        return Self::missing(format!("window {}: {error}", index + 1));
                    }
                }
            }
        }
        pooled.map_or_else(
            || Self::missing("no window was measured"),
            |(q, per, _, distribution)| Self::quantile(&distribution, q, per),
        )
    }

    /// Nearest-rank percentile `q` of exact raw `samples`, each divided by
    /// `per` (1e6 for nanoseconds to milliseconds), with the sample count;
    /// missing without samples. Dividing keeps a boundary exact: 500,000 ns
    /// is exactly 0.5 ms.
    /// The samples are also kept as a duration-shaped histogram so the
    /// case aggregate can pool them ([`Self::pooled`]).
    #[must_use]
    pub fn percentile(samples: &[u64], q: f64, per: f64) -> Self {
        crate::capacity::nearest_rank(samples, q, per).map_or_else(
            || Self::missing("no successful sample"),
            |value| Self {
                pool: wyrd_benchmark::Distribution::of_durations(samples)
                    .ok()
                    .map(|distribution| Pool::of(&distribution, q, per)),
                ..Self::exact(value).with_samples(samples.len() as u64)
            },
        )
    }

    /// The interval the value lies in: lower bound, whether it is
    /// exclusive, and upper bound (`None` for `+Inf`); `None` when missing.
    fn interval(&self) -> Option<(f64, bool, Option<f64>)> {
        match (self.above, self.value) {
            (Some(above), value) => Some((above, true, value)),
            (None, Some(value)) => Some((value, false, Some(value))),
            (None, None) => None,
        }
    }

    /// The same reading without its kept histogram, for a judgment or
    /// summary that repeats a window reading.
    fn bare(&self) -> Self {
        Self {
            pool: None,
            ..self.clone()
        }
    }

    /// The exact value, when this is not censored.
    fn exact_value(&self) -> Option<f64> {
        if self.above.is_some() {
            None
        } else {
            self.value
        }
    }

    /// Whether a value or censored bound is present.
    #[must_use]
    pub fn is_present(&self) -> bool {
        self.interval().is_some()
    }

    /// The median of three or more readings: the median of their lower
    /// bounds and of their upper bounds, an interval when any is censored; missing
    /// when any is missing.
    #[must_use]
    pub fn median(readings: &[Self]) -> Self {
        let mut lows = Vec::new();
        let mut highs = Vec::new();
        let mut open = false;
        for reading in readings {
            let Some((lo, exclusive, hi)) = reading.interval() else {
                return Self::missing(
                    reading
                        .reason
                        .clone()
                        .unwrap_or_else(|| "a repetition has no reading".to_owned()),
                );
            };
            open |= exclusive;
            lows.push(lo);
            highs.push(hi.unwrap_or(f64::INFINITY));
        }
        if lows.is_empty() {
            return Self::missing("no repetition was measured");
        }
        let middle = |values: &mut Vec<f64>| {
            values.sort_by(f64::total_cmp);
            values[values.len() / 2]
        };
        let (lo, hi) = (middle(&mut lows), middle(&mut highs));
        let samples = readings.iter().filter_map(|reading| reading.samples).min();
        let median = if open {
            Self {
                value: hi.is_finite().then_some(hi),
                above: Some(lo),
                ..Self::default()
            }
        } else {
            Self::exact(lo)
        };
        Self { samples, ..median }
    }

    /// The reading as a report cell, with `unit`.
    #[must_use]
    pub fn cell(&self, unit: &str) -> String {
        let cell = match (self.above, self.value) {
            (Some(above), Some(value)) => format!("({}, {}] {unit}", number(above), number(value)),
            (Some(above), None) => format!("> {} {unit}", number(above)),
            (None, Some(value)) => format!("{} {unit}", number(value)),
            (None, None) => format!("— {}", self.reason.as_deref().unwrap_or("not measured")),
        };
        cell.trim_end().to_owned()
    }
}

/// One distribution's count, nearest-rank p50, p95 and p99, and mean, each
/// value divided by `per` and read in `unit`.
fn distribution_cells(distribution: &wyrd_benchmark::Distribution, per: f64, unit: &str) -> String {
    let quantile = |q| Reading::quantile(distribution, q, per).cell(unit);
    format!(
        "n {}, p50 {}, p95 {}, p99 {}, mean {}",
        distribution.len(),
        quantile(0.5),
        quantile(0.95),
        quantile(0.99),
        Reading::mean(distribution, per).cell(unit)
    )
}

/// A number with at most three decimals and no trailing zeros.
fn number(value: f64) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// Scheduled open-loop arrivals over one repetition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Arrivals {
    /// Arrivals the schedule planned.
    pub scheduled: u64,
    /// Arrivals the driver sent.
    pub sent: u64,
    /// Sent arrivals that completed.
    pub completed: u64,
    /// Planned arrivals the driver never sent.
    pub missed: u64,
    /// Send-lateness p99, microseconds.
    pub lateness_p99_us: Option<f64>,
}

/// What a workload measured over one repetition (or warmup).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Repetition {
    /// Measured wall seconds.
    pub seconds: f64,
    /// The declared server processes whose CPU is summed, by role.
    pub processes: Vec<String>,
    /// Server CPU-seconds summed over `processes`, or `None` when a process
    /// was lost or a reading was unavailable or reset.
    pub server_cpu_seconds: Option<f64>,
    /// Sampled peak server resident memory, bytes.
    pub peak_memory_bytes: Option<u64>,
    /// Server resident memory as the window opened, bytes; peak less
    /// opening is the memory the window added. Absent from older records.
    #[serde(default)]
    pub opening_memory_bytes: Option<u64>,
    /// The memory sampling interval, milliseconds.
    pub memory_interval_ms: u64,
    /// Driver CPU-seconds, or `None` when unreadable.
    pub driver_cpu_seconds: Option<f64>,
    /// P: the logical CPUs available to the server processes on this one
    /// shared host, counted once however many server processes share them;
    /// `None` when unavailable or changed during the window.
    pub provisioned_vcpus: Option<u64>,
    /// Raw and derived metric readings by metric id.
    pub metrics: BTreeMap<String, Reading>,
    /// Open-loop arrivals, for open-loop work.
    pub arrivals: Option<Arrivals>,
    /// Reasons the workload found this repetition invalid.
    pub invalid: Vec<String>,
    /// Informative conditions, such as capture saturation; never verdicts.
    pub observations: Vec<String>,
    /// Each judged server timing cohort's expected and recorded
    /// observations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reconciliation: Vec<Reconciled>,
    /// The full server timing snapshots each window was read from, which
    /// the runner keeps under the run's target artifacts and never in the
    /// record.
    #[serde(skip)]
    pub timings: Vec<TimingEvidence>,
}

/// The process whose recorder produced timing evidence. Records written
/// before the driver recorded its own observations hold only server
/// evidence, so a missing role reads as [`Self::Server`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// The benchmark-owned `wyrd-server`.
    #[default]
    Server,
    /// The capacity driver: its clients and its local provider fixture.
    Driver,
    /// A Python SDK client process the driver starts: its clients only.
    Python,
    /// A TypeScript SDK client process the driver starts: its clients only.
    TypeScript,
}

impl Role {
    /// Every role, in report order.
    pub const ALL: [Self; 4] = [Self::Server, Self::Driver, Self::Python, Self::TypeScript];

    /// The role's name in records and reports.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Driver => "driver",
            Self::Python => "python",
            Self::TypeScript => "typescript",
        }
    }
}

/// One timing cohort's reconciliation: the observations a window owed at
/// one boundary, outcome, case, and phase in one process, and those it
/// recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reconciled {
    /// The process that recorded them.
    #[serde(default)]
    pub role: Role,
    /// The server timing boundary.
    pub boundary: String,
    /// The outcome the observations were owed with.
    pub outcome: String,
    /// The requests' case.
    pub case: String,
    /// The phase that admitted the requests.
    pub phase: String,
    /// Observations the requests the driver saw complete owe.
    pub expected: u64,
    /// Observations the server recorded.
    pub recorded: u64,
}

/// The full timing evidence of one window in one process: the cumulative
/// snapshots it differences, whose buckets rebuild every judged
/// distribution, and its reconciliation. A process's snapshots are never
/// differenced against another's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimingEvidence {
    /// The window's name within its repetition, such as `measured`.
    pub window: String,
    /// The process whose snapshots these are.
    #[serde(default)]
    pub role: Role,
    /// The snapshot at the window's open.
    pub before: wyrd_benchmark::BenchmarkSnapshot,
    /// The snapshot once the window's cohort drained.
    pub after: wyrd_benchmark::BenchmarkSnapshot,
    /// The window's expected and recorded observations.
    pub reconciliation: Vec<Reconciled>,
}

impl Repetition {
    /// Sets the [`DIAGNOSTIC_STATS`] readings of diagnostic `observation`
    /// from its interval `distribution` of nanoseconds: its count, and in
    /// microseconds its nearest-rank bucket bounds and the bounds of its
    /// mean. A refused distribution
    /// leaves every reading missing with its reason; [`Self::invalid`] then
    /// records it when the observation is `required`, since its absence
    /// means the server did not record what it declares.
    pub fn diagnose(
        &mut self,
        observation: &str,
        distribution: Result<wyrd_benchmark::Distribution, String>,
        required: bool,
    ) {
        let id = |stat: &str| format!("{DIAGNOSTIC}{observation}.{stat}");
        let distribution = match distribution {
            Ok(distribution) => distribution,
            Err(reason) => {
                for (stat, _) in DIAGNOSTIC_STATS {
                    self.set(&id(stat), Reading::missing(reason.clone()));
                }
                if required {
                    self.invalid
                        .push(format!("diagnostic `{observation}` is missing: {reason}"));
                }
                return;
            }
        };
        let per = 1e3;
        let samples = distribution.len();
        self.set(&id("count"), Reading::count(samples));
        for (stat, q) in [("p50_us", 0.5), ("p95_us", 0.95), ("p99_us", 0.99)] {
            self.set(&id(stat), Reading::quantile(&distribution, q, per));
        }
        self.set(&id("mean_us"), Reading::mean(&distribution, per));
    }

    /// Sets every [`DIAGNOSTIC_STATS`] reading of `observation` missing as
    /// inapplicable to this step, for `reason`.
    pub fn inapplicable(&mut self, observation: &str, reason: &str) {
        for (stat, _) in DIAGNOSTIC_STATS {
            self.set(
                &format!("{DIAGNOSTIC}{observation}.{stat}"),
                Reading::missing(format!("inapplicable: {reason}")),
            );
        }
    }

    /// Sets raw metric `id`.
    pub fn set(&mut self, id: &str, reading: Reading) {
        self.metrics.insert(id.to_owned(), reading);
    }

    /// The reading of `id`, or a missing one.
    #[must_use]
    pub fn reading(&self, id: &str) -> Reading {
        self.metrics
            .get(id)
            .cloned()
            .unwrap_or_else(|| Reading::missing(format!("`{id}` was not read")))
    }

    /// The case aggregate of measured `windows` under `metrics`: summed
    /// wall seconds, server and driver CPU, and the raw work every derived
    /// metric divides, so each rate and efficiency is a ratio of sums and
    /// never an average of window ratios; every percentile pooled from the
    /// windows' kept histograms ([`Reading::pooled`]); the largest peak
    /// memory; summed arrivals with the worst lateness. A window missing a
    /// summed reading leaves that aggregate reading, and every metric
    /// derived from it, missing with the reason, never zero. Other window
    /// readings stay per window.
    #[must_use]
    pub fn aggregate(metrics: &[Metric], windows: &[Self]) -> Self {
        let summed = |read: &dyn Fn(&Self) -> Option<f64>| -> Option<f64> {
            windows.iter().map(read).sum::<Option<f64>>()
        };
        let work: BTreeSet<&str> = metrics
            .iter()
            .filter_map(|metric| match &metric.derive {
                Some(
                    Derive::PerCpu { work, .. }
                    | Derive::CpuPer { work, .. }
                    | Derive::PerVcpu { work, .. }
                    | Derive::Rate { work, .. }
                    | Derive::MemoryPer { work },
                ) => Some(work.as_str()),
                Some(Derive::Cores) | None => None,
            })
            .collect();
        let vcpus: BTreeSet<Option<u64>> = windows
            .iter()
            .map(|window| window.provisioned_vcpus)
            .collect();
        let mut aggregate = Self {
            seconds: windows.iter().map(|window| window.seconds).sum(),
            processes: windows
                .first()
                .map(|window| window.processes.clone())
                .unwrap_or_default(),
            server_cpu_seconds: summed(&|window| window.server_cpu_seconds),
            peak_memory_bytes: windows
                .iter()
                .map(|window| window.peak_memory_bytes)
                .collect::<Option<Vec<u64>>>()
                .and_then(|peaks| peaks.into_iter().max()),
            opening_memory_bytes: windows
                .first()
                .and_then(|window| window.opening_memory_bytes),
            memory_interval_ms: windows
                .first()
                .map_or(0, |window| window.memory_interval_ms),
            driver_cpu_seconds: summed(&|window| window.driver_cpu_seconds),
            provisioned_vcpus: if vcpus.len() == 1 {
                vcpus.into_iter().next().flatten()
            } else {
                None
            },
            arrivals: windows
                .iter()
                .map(|window| window.arrivals)
                .collect::<Option<Vec<Arrivals>>>()
                .map(|arrivals| Arrivals {
                    scheduled: arrivals.iter().map(|a| a.scheduled).sum(),
                    sent: arrivals.iter().map(|a| a.sent).sum(),
                    completed: arrivals.iter().map(|a| a.completed).sum(),
                    missed: arrivals.iter().map(|a| a.missed).sum(),
                    lateness_p99_us: arrivals
                        .iter()
                        .map(|a| a.lateness_p99_us)
                        .collect::<Option<Vec<f64>>>()
                        .and_then(|late| late.into_iter().reduce(f64::max)),
                }),
            ..Self::default()
        };
        let ids: BTreeSet<&String> = windows
            .iter()
            .flat_map(|window| window.metrics.keys())
            .collect();
        for id in ids {
            let readings: Vec<Reading> = windows.iter().map(|window| window.reading(id)).collect();
            if readings.iter().any(|reading| reading.pool.is_some()) {
                aggregate
                    .metrics
                    .insert(id.clone(), Reading::pooled(&readings));
            } else if work.contains(id.as_str()) {
                let total = readings
                    .iter()
                    .enumerate()
                    .map(|(index, reading)| {
                        reading.exact_value().ok_or_else(|| {
                            format!(
                                "window {} has no exact `{id}`: {}",
                                index + 1,
                                reading.reason.as_deref().unwrap_or("not read")
                            )
                        })
                    })
                    .sum::<Result<f64, String>>();
                aggregate.metrics.insert(
                    id.clone(),
                    total.map_or_else(Reading::missing, Reading::exact),
                );
            }
        }
        aggregate.derive(metrics);
        aggregate
    }

    /// Adds every metric `metrics` derive from this repetition's raw
    /// readings and server CPU. A missing CPU or work reading, or a zero
    /// denominator, leaves the derived metric missing with its reason.
    fn derive(&mut self, metrics: &[Metric]) {
        let cpu = self.server_cpu_seconds;
        for metric in metrics {
            let Some(derive) = &metric.derive else {
                continue;
            };
            let work = |id: &str| {
                self.metrics
                    .get(id)
                    .and_then(Reading::exact_value)
                    .ok_or_else(|| format!("work reading `{id}` is missing"))
            };
            let cpu = || {
                cpu.filter(|cpu| *cpu > 0.0).ok_or_else(|| {
                    if cpu.is_some() {
                        "server CPU-seconds were zero".to_owned()
                    } else {
                        "server CPU-seconds are unavailable".to_owned()
                    }
                })
            };
            let seconds = || {
                Some(self.seconds)
                    .filter(|seconds| *seconds > 0.0)
                    .ok_or_else(|| "the window had no duration".to_owned())
            };
            let value: Result<f64, String> = match derive {
                Derive::Cores => cpu().and_then(|cpu| seconds().map(|seconds| cpu / seconds)),
                Derive::PerCpu { work: id, scale } => {
                    work(id).and_then(|work| cpu().map(|cpu| work / cpu * scale))
                }
                Derive::CpuPer { work: id, scale } => work(id).and_then(|work| {
                    if work > 0.0 {
                        cpu().map(|cpu| cpu / work * scale)
                    } else {
                        Err(format!("work `{id}` was zero"))
                    }
                }),
                Derive::PerVcpu { work: id, scale } => work(id).and_then(|work| {
                    let vcpus = self
                        .provisioned_vcpus
                        .filter(|vcpus| *vcpus > 0)
                        .ok_or_else(|| "provisioned vCPUs are unavailable or changed".to_owned())?;
                    seconds().map(|seconds| work / seconds / vcpus as f64 * scale)
                }),
                Derive::Rate { work: id, scale } => {
                    work(id).and_then(|work| seconds().map(|seconds| work / seconds * scale))
                }
                Derive::MemoryPer { work: id } => work(id).and_then(|work| {
                    let memory = self
                        .peak_memory_bytes
                        .ok_or_else(|| "peak memory was not sampled".to_owned())?;
                    let rate = seconds()?.min(f64::MAX);
                    if work > 0.0 {
                        Ok(memory as f64 / (work / rate))
                    } else {
                        Err(format!("work `{id}` was zero"))
                    }
                }),
            };
            let reading = value.map_or_else(Reading::missing, Reading::exact);
            self.metrics.insert(metric.id.clone(), reading);
        }
    }
}

/// One phase of a step a workload is asked to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Discarded warmup.
    Warmup,
    /// Measured repetition `n`, from zero.
    Repetition(u8),
}

impl Phase {
    /// The closed benchmark phase label a driver sends with every request
    /// it admits in this phase; a repetition beyond the declared labels is
    /// [`wyrd_benchmark::INVALID`], which the server counts.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Warmup => "warmup",
            Self::Repetition(0) => "repetition-1",
            Self::Repetition(1) => "repetition-2",
            Self::Repetition(2) => "repetition-3",
            Self::Repetition(_) => wyrd_benchmark::INVALID,
        }
    }
}

/// Writes `evidence`, one repetition's server timing snapshots, to
/// `timings/<step>/<phase>.json` under the run's `artifacts`.
///
/// # Errors
///
/// Returns the directory, encoding, or write failure.
fn keep_timings(
    artifacts: &Path,
    step: &str,
    phase: Phase,
    evidence: &[TimingEvidence],
) -> std::io::Result<()> {
    let directory = artifacts.join("timings").join(step);
    std::fs::create_dir_all(&directory)?;
    std::fs::write(
        directory.join(format!("{}.json", phase.label())),
        serde_json::to_vec(evidence)?,
    )
}

/// One engine's production work under the shared lifecycle.
///
/// The runner calls [`Workload::prepare`] once inside the clock, then [`Workload::measure`] for each step's warmup and
/// repetitions, then [`Workload::finish`]. A workload establishes
/// equivalent declared state before each repetition (fresh write
/// identities, a whole stateful sequence) and reads only its own server's
/// evidence.
#[async_trait]
pub trait Workload: Send {
    /// Starts the servers and loads and settles the fixtures at the
    /// declaration's scale. The runner calls it once and never retries it.
    ///
    /// # Errors
    ///
    /// Returns a workload error naming the engine and what did not start
    /// or load; the runner then measures no step. Cancellation at the
    /// budget or an interrupt drops the future, so fixtures may be partly
    /// loaded; [`Self::finish`] still runs and stops the servers.
    async fn prepare(&mut self) -> Result<(), ExperimentError>;

    /// PIDs of the declared server processes, for optional profiling.
    fn processes(&self) -> Vec<u32>;

    /// Where the server under test serves its heap profile, for optional
    /// profiling; `None`, the default, when it serves none.
    fn heap_profile_url(&self) -> Option<String> {
        None
    }

    /// The client processes beside the server and this driver, each with
    /// its role, for optional profiling; none by default. A workload that
    /// has any starts them in [`Self::prepare`] when profiled, so every
    /// step's captures include them.
    fn clients(&self) -> Vec<(Role, u32)> {
        Vec::new()
    }

    /// Saves each of [`Self::clients`]'s own heap captures of `step` under
    /// `directory`, returning the captures and a note for each
    /// one missing or partial. The runner calls it after every measured step
    /// of a profiled run; by default there is nothing to save.
    async fn capture_clients(&mut self, step: &str, directory: &Path) -> ProfileNote {
        let _ = (step, directory);
        ProfileNote::default()
    }

    /// Runs `phase` of `step` for `window` and returns what it measured.
    /// The runner calls it once per warmup and repetition and never retries
    /// a failed call.
    ///
    /// # Errors
    ///
    /// Returns a workload error for an unprepared workload, an undeclared
    /// step, or a failed read; the runner then measures no further phase of
    /// the step. Cancellation at the budget or an interrupt drops the future
    /// mid-window, discarding that repetition; work it launched may still be
    /// in flight until [`Self::finish`].
    async fn measure(
        &mut self,
        step: &Step,
        phase: Phase,
        window: Duration,
    ) -> Result<Repetition, ExperimentError>;

    /// Stops admission, drains, and stops the servers. The runner calls it
    /// once after any admitted run, including a cancelled or failed one,
    /// bounded by its finish grace, and never retries it.
    ///
    /// # Errors
    ///
    /// Returns a workload error when the drain or stop was not clean; the
    /// record is then INVALID. Servers are released either way; one that
    /// outlived its stop grace was killed.
    async fn finish(&mut self) -> Result<(), ExperimentError>;

    /// The outstanding publication and maintenance work the server still
    /// holds, read after the last window drained and before
    /// [`Self::finish`]; the runner keeps it in an L1 case's [`Lifecycle`].
    /// `None`, the default, when the workload reads none.
    ///
    /// # Errors
    ///
    /// Returns a workload error when the read failed; the case is then
    /// INVALID, since its required evidence is missing.
    async fn outstanding(&mut self) -> Result<Option<Value>, ExperimentError> {
        Ok(None)
    }
}

/// What a `--profile` run collects and the CPU profiler it invokes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profiling {
    /// The collectors, for both process roles.
    pub mode: ProfileMode,
    /// The CPU profiler program, [`crate::profile::SAMPLY`] outside tests.
    pub program: PathBuf,
    /// Where sampled executables are kept by build ID, the repository's
    /// [`crate::profile::SYMBOLS`] outside tests.
    pub symbols: PathBuf,
}

/// Where a run keeps raw artifacts and whether it profiles.
#[derive(Debug, Clone)]
pub struct Options {
    /// Raw artifacts directory under `target/`.
    pub artifacts: PathBuf,
    /// What `--profile` collects, or `None` without it.
    pub profile: Option<Profiling>,
    /// Cancelled when the run is interrupted.
    pub interrupt: CancellationToken,
    /// Setup time the case already spent before [`run`] started, such as
    /// creating its PostgreSQL; charged to the L1 setup allowance. Zero
    /// outside the case protocol.
    pub setup_spent: Duration,
}

/// One process a case owned while it ran, identified by PID and start
/// time, since a PID alone may be reused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owned {
    /// What the process is.
    pub role: Role,
    /// Its PID.
    pub pid: u32,
    /// Its start, seconds since the Unix epoch, when readable.
    pub started_unix: Option<u64>,
}

impl Owned {
    /// The process `pid` as it runs now, in `role`.
    #[must_use]
    pub fn read(role: Role, pid: u32) -> Self {
        Self {
            role,
            pid,
            started_unix: crate::release_server::os::process_started(pid),
        }
    }

    /// Whether this same process, its PID with the same start, still runs.
    #[must_use]
    pub fn alive(&self) -> bool {
        let now = Self::read(self.role, self.pid);
        now.started_unix.is_some() && now.started_unix == self.started_unix
    }
}

/// One warmup or window of a case: how long it admitted work and how long
/// its in-flight requests took to drain after the stop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaseWindow {
    /// The phase label, such as `warmup` or `repetition-2`.
    pub phase: String,
    /// Seconds of admission, at most the declared length.
    pub admitted_seconds: f64,
    /// Seconds after the stop until the call returned.
    pub drained_seconds: f64,
}

/// The L1 case lifecycle a record keeps: each phase's measured seconds
/// against its allowance, every window's admission and drain, the processes
/// the case owned, and the outstanding publication and maintenance work it
/// captured before teardown. The supervisor completes teardown and persist
/// after the case process exits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lifecycle {
    /// The allowances judged.
    pub allowances: Allowances,
    /// Setup seconds, including any spent before the runner started.
    pub setup_seconds: f64,
    /// Warmup admission seconds.
    pub warmup_seconds: f64,
    /// Measured admission seconds.
    pub measurement_seconds: f64,
    /// Every drain and evidence capture, inter-window and final.
    pub drain_seconds: f64,
    /// Stopping, reaping and disposing of owned resources.
    pub teardown_seconds: f64,
    /// Finalizing and keeping the case record, once the supervisor kept it.
    pub persist_seconds: Option<f64>,
    /// Each warmup and window, in order.
    pub windows: Vec<PhaseWindow>,
    /// The processes the case owned once prepared.
    pub owned: Vec<Owned>,
    /// Outstanding publication and maintenance work before teardown, as
    /// the workload read it; `None` when it read none.
    pub outstanding: Option<Value>,
}

impl Lifecycle {
    /// A ledger under `allowances` that already spent `setup` seconds.
    fn new(allowances: Allowances, setup: Duration) -> Self {
        Self {
            allowances,
            setup_seconds: setup.as_secs_f64(),
            warmup_seconds: 0.0,
            measurement_seconds: 0.0,
            drain_seconds: 0.0,
            teardown_seconds: 0.0,
            persist_seconds: None,
            windows: Vec::new(),
            owned: Vec::new(),
            outstanding: None,
        }
    }

    /// What remains of an allowance of `allowed` seconds after `used`.
    fn left(allowed: u64, used: f64) -> Duration {
        Duration::from_secs_f64((allowed as f64 - used).max(0.0))
    }

    /// What remains of the setup allowance.
    #[must_use]
    pub fn setup_left(&self) -> Duration {
        Self::left(self.allowances.setup_seconds, self.setup_seconds)
    }

    /// What remains of the shared drain allowance.
    #[must_use]
    pub fn drain_left(&self) -> Duration {
        Self::left(self.allowances.drain_seconds, self.drain_seconds)
    }

    /// What remains of the teardown allowance.
    #[must_use]
    pub fn teardown_left(&self) -> Duration {
        Self::left(self.allowances.teardown_seconds, self.teardown_seconds)
    }

    /// Charges one `phase` call of declared `length` that returned after
    /// `elapsed`: admission up to `length`, the rest to the drain.
    fn window(&mut self, phase: Phase, length: Duration, elapsed: Duration) {
        let admitted = elapsed.min(length).as_secs_f64();
        let drained = elapsed.saturating_sub(length).as_secs_f64();
        if phase == Phase::Warmup {
            self.warmup_seconds += admitted;
        } else {
            self.measurement_seconds += admitted;
        }
        self.drain_seconds += drained;
        self.windows.push(PhaseWindow {
            phase: phase.label().to_owned(),
            admitted_seconds: admitted,
            drained_seconds: drained,
        });
    }

    /// Charges `elapsed` of the final drain and stop: to what remains of
    /// the drain allowance first, then to teardown.
    pub fn finish(&mut self, elapsed: Duration) {
        let drain = elapsed.min(self.drain_left());
        self.drain_seconds += drain.as_secs_f64();
        self.teardown_seconds += elapsed.saturating_sub(drain).as_secs_f64();
    }

    /// Every phase over its allowance, as an INVALID reason naming it.
    #[must_use]
    pub fn overruns(&self) -> Vec<Reason> {
        let a = self.allowances;
        [
            ("setup", self.setup_seconds, a.setup_seconds),
            ("warmup", self.warmup_seconds, a.warmup_seconds),
            (
                "measurement",
                self.measurement_seconds,
                a.measurement_seconds,
            ),
            ("drain and capture", self.drain_seconds, a.drain_seconds),
            ("teardown", self.teardown_seconds, a.teardown_seconds),
            (
                "persist and report",
                self.persist_seconds.unwrap_or(0.0),
                a.persist_seconds,
            ),
        ]
        .into_iter()
        .filter(|(_, used, allowed)| *used > *allowed as f64)
        .map(|(phase, used, allowed)| {
            Reason::new(
                "phase.overrun",
                format!("{phase} took {used:.1}s, over its {allowed}s allowance"),
            )
        })
        .collect()
    }
}

/// One criterion's outcome on one step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Judgment {
    /// The criterion id.
    pub criterion: String,
    /// The value judged: the median, peak, or worst repetition.
    pub observed: Reading,
    /// Each repetition's reading.
    pub values: Vec<Reading>,
    /// Met, missed, unsettled or invalid; `None` for an inapplicable
    /// criterion.
    pub outcome: Option<Outcome>,
    /// Why it was missed, unsettled, invalid or inapplicable.
    pub reason: Option<String>,
    /// Whether the criterion was judged: false only in a smoke, which
    /// judges no criterion once its evidence is present.
    pub applicable: bool,
}

/// The `compare` control an experiment may declare: which step param the
/// report's summary puts side by side, the metrics it compares, and a note
/// per step group, keyed by the group's params as the summary labels them
/// (such as `clients=64 rate=3000`).
#[derive(Debug, Deserialize)]
struct Comparison {
    /// The step param whose values become the summary's columns.
    by: String,
    /// The metrics compared, one row each, in order.
    metrics: Vec<String>,
    /// What each group answers, for a literal reader.
    #[serde(default)]
    notes: BTreeMap<String, String>,
}

/// One metric's spread over the repetitions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    /// The metric's unit.
    pub unit: String,
    /// The median repetition reading.
    pub median: Reading,
    /// The smallest exact repetition value.
    pub min: Option<f64>,
    /// The largest exact repetition value.
    pub max: Option<f64>,
    /// `max - min`.
    pub spread: Option<f64>,
}

impl Summary {
    /// The median and the repetition range as one comparison cell, such as
    /// `100 [98–101]`; a histogram median shows its bucket.
    fn spread_cell(&self) -> String {
        let median = self.median.cell("");
        match self.min.zip(self.max) {
            Some((min, max)) => format!("{median} [{}–{}]", number(min), number(max)),
            None => median,
        }
    }
}

/// How a step came to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// The workload measured it.
    Measured,
    /// Derived from measured steps.
    Derived,
    /// Declared but not measured.
    Unmeasured,
    /// Outside the selected subset.
    Omitted,
}

/// One step of a record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepRecord {
    /// The step id.
    pub id: String,
    /// The step's parameters.
    pub params: BTreeMap<String, Value>,
    /// How it came to be.
    pub status: Status,
    /// The warmup and repetition durations it used.
    pub timing: Timing,
    /// Every measured repetition, in order.
    pub repetitions: Vec<Repetition>,
    /// The case aggregate of the measured windows under the L1 case
    /// protocol ([`Repetition::aggregate`]), which its targets judge;
    /// `None` outside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregate: Option<Repetition>,
    /// Each metric's median, minimum, maximum, and spread.
    pub summary: BTreeMap<String, Summary>,
    /// Each applicable criterion's outcome.
    pub criteria: Vec<Judgment>,
    /// The step verdict.
    pub verdict: Verdict,
    /// Why the step is INVALID.
    pub reasons: Vec<Reason>,
}

/// The run's validity observations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidityRecord {
    /// The thresholds used.
    pub thresholds: Validity,
    /// The host sample before the run.
    pub host_before: Option<HostLoad>,
    /// The host sample after the run.
    pub host_after: Option<HostLoad>,
    /// The budget, seconds.
    pub budget_seconds: u64,
    /// The minimum complete duration, seconds.
    pub minimum_seconds: u64,
    /// Monotonic elapsed seconds.
    pub elapsed_seconds: f64,
    /// Every run-level validity reason.
    pub reasons: Vec<Reason>,
}

/// Raw local artifacts a record refers to; history never depends on them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifacts {
    /// The run's raw artifacts directory under `target/`.
    pub directory: Option<String>,
    /// What optional profiling produced.
    pub profile: ProfileNote,
}

/// The controls a record keeps: timing, validity, and declared values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Controls {
    /// Warmup, repetition, and budget durations.
    pub timing: Timing,
    /// Validity thresholds.
    pub validity: Validity,
    /// Declared fixed controls.
    pub values: BTreeMap<String, Value>,
    /// The selected subset, or `None` for full default coverage.
    pub selection: Option<Vec<String>>,
}

/// The criteria a record keeps: metric definitions and criteria.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CriteriaRecord {
    /// Every declared metric.
    pub metrics: Vec<Metric>,
    /// Every declared criterion.
    pub criteria: Vec<Criterion>,
}

/// One persisted experiment record, schema version [`SCHEMA_VERSION`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    /// [`SCHEMA_VERSION`].
    pub schema_version: u64,
    /// The engine.
    pub engine: Engine,
    /// The experiment id.
    pub experiment: String,
    /// The level, 1 or 2.
    pub level: Level,
    /// The question.
    pub question: String,
    /// UTC start, RFC 3339.
    pub started_at: DateTime<Utc>,
    /// UTC finish, RFC 3339, including partial runs.
    pub finished_at: DateTime<Utc>,
    /// Monotonic elapsed seconds.
    pub elapsed_seconds: f64,
    /// Full measured-source SHA.
    pub commit: String,
    /// Source dirtiness at declaration.
    pub dirty: bool,
    /// Whether a dirty override was declared.
    pub dirty_override: bool,
    /// SHA-256 of `git diff HEAD`, when dirty.
    pub diff_sha256: Option<String>,
    /// The host.
    pub host: HostRecord,
    /// The build.
    pub build: Value,
    /// The invocation.
    pub command: Invocation,
    /// The variable sweep.
    pub variables: BTreeMap<String, Value>,
    /// Fixed controls, validity, and budget declarations.
    pub controls: Controls,
    /// Metric definitions and criteria.
    pub criteria: CriteriaRecord,
    /// Every declared step.
    pub steps: Vec<StepRecord>,
    /// Validity observations.
    pub validity: ValidityRecord,
    /// VALID or INVALID.
    pub verdict: Verdict,
    /// Every reason the run is INVALID, run- and step-level.
    pub reasons: Vec<Reason>,
    /// Optional local artifacts.
    pub artifacts: Artifacts,
    /// Full or smoke; a record without one is a full run.
    #[serde(default)]
    pub mode: Mode,
    /// The L1 case lifecycle, for a run under the case protocol; absent
    /// from every other record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<Lifecycle>,
}

/// Runs a sealed declaration's workload and returns its record.
///
/// The clock starts here. A refused declaration admits no workload. Each
/// host sample comes from `host`, given the declared sample length. The
/// workload is prepared, each selected measured step warms up and measures
/// its repetitions, the workload finishes, and the host is sampled again.
/// An overrun, interruption, or workload failure stops admission, keeps the
/// partial evidence, and marks every remaining step unmeasured; it never
/// fabricates a metric. Optional profile captures save beside the drain and
/// the after sample, and the budget and minimum judge the run's elapsed time
/// when that work ends, never the saving. `started_at` and `finished_at` are
/// actual instants, the latter after saving.
pub async fn run<W, H, F>(
    sealed: Sealed,
    workload: &mut W,
    mut host: H,
    options: &Options,
) -> Record
where
    W: Workload + ?Sized,
    H: FnMut(Duration) -> F + Send,
    F: Future<Output = Option<HostLoad>> + Send,
{
    let declaration = sealed.0;
    let started_at = Utc::now();
    let clock = Instant::now();
    let timing = declaration.timing;
    let deadline = clock + Duration::from_secs(timing.budget_seconds);
    let mut validity = declaration.refusals();
    let mut measured: BTreeMap<String, (Vec<Repetition>, Vec<Reason>)> = BTreeMap::new();
    let mut host_before = None;
    let mut host_after = None;
    let mut profile = ProfileNote::default();
    let sample = Duration::from_secs(declaration.validity.host_sample_seconds);
    let interrupt = options.interrupt.clone();
    let mut stopped: Option<Reason> = None;
    let mut worked = None;
    let mut lifecycle = timing
        .phases
        .map(|allowances| Lifecycle::new(allowances, options.setup_spent));
    let deadline = deadline
        - options
            .setup_spent
            .min(Duration::from_secs(timing.budget_seconds));

    if validity.is_empty() {
        host_before = host(sample).await;
        validity.extend(host_reasons(
            host_before.as_ref(),
            &declaration.validity,
            "before",
        ));
    }
    let admitted = validity.is_empty();
    if admitted {
        let setup_deadline = lifecycle
            .as_ref()
            .map_or(deadline, |ledger| deadline.min(clock + ledger.setup_left()));
        let preparing = bounded(setup_deadline, &interrupt, workload.prepare()).await;
        if let Some(ledger) = &mut lifecycle {
            ledger.setup_seconds += clock.elapsed().as_secs_f64();
            ledger.owned = workload
                .processes()
                .into_iter()
                .map(|pid| (Role::Server, pid))
                .chain(workload.clients())
                .map(|(role, pid)| Owned::read(role, pid))
                .collect();
        }
        match phase_bounded(preparing, lifecycle.as_ref(), "setup") {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                stopped = Some(Reason::new(
                    "workload.failed",
                    format!("preparation failed: {error}"),
                ));
            }
            Err(reason) => stopped = Some(reason),
        }
        let profiles = options.artifacts.join("profiles");
        let profiling = options.profile.as_ref().filter(|_| stopped.is_none());
        let clients = workload.clients();
        let targets: Vec<(Role, u32)> = workload
            .processes()
            .into_iter()
            .map(|pid| (Role::Server, pid))
            .chain(std::iter::once((Role::Driver, std::process::id())))
            .chain(clients.iter().copied())
            .collect();
        profile = ProfileNote {
            requested: profiling.is_some(),
            mode: profiling.map(|profiling| profiling.mode),
            roles: if profiling.is_some() {
                [Role::Server, Role::Driver]
                    .into_iter()
                    .chain(clients.iter().map(|(role, _)| *role))
                    .collect()
            } else {
                Vec::new()
            },
            complete: profiling.is_some(),
            ..ProfileNote::default()
        };
        let cpu = profiling.filter(|profiling| profiling.mode.cpu());
        let heap_mode = profiling.is_some_and(|profiling| profiling.mode.heap());
        let heap_url = heap_mode.then(|| workload.heap_profile_url()).flatten();
        let mut heap = HeapProfiler::start(heap_url, heap_mode, &profiles).await;
        let mut saving = None;
        for step in &declaration.steps {
            if stopped.is_some()
                || step.kind != StepKind::Measured
                || !declaration.selected(&step.id)
            {
                continue;
            }
            if let Some(previous) = saving.take() {
                profile.absorb(saved(previous).await);
            }
            let profiler = cpu.map_or_else(Profiler::disabled, |cpu| {
                Profiler::start(
                    &cpu.program,
                    &targets,
                    &profiles.join(&step.id),
                    &cpu.symbols,
                )
            });
            let (repetitions, reasons) = measured.entry(step.id.clone()).or_default();
            let window = Duration::from_secs(declaration.step_timing(step).repetition_seconds);
            let warmup = Duration::from_secs(timing.warmup_seconds);
            let phases = std::iter::once(Phase::Warmup)
                .chain((0..timing.repetitions).map(Phase::Repetition));
            for phase in phases {
                let length = if phase == Phase::Warmup {
                    warmup
                } else {
                    window
                };
                let available = provisioned_vcpus();
                let opened = Instant::now();
                let call_deadline = lifecycle.as_ref().map_or(deadline, |ledger| {
                    deadline.min(opened + length + ledger.drain_left())
                });
                let called = bounded(
                    call_deadline,
                    &interrupt,
                    workload.measure(step, phase, length),
                )
                .await;
                if let Some(ledger) = &mut lifecycle {
                    ledger.window(phase, length, opened.elapsed());
                }
                match phase_bounded(called, lifecycle.as_ref(), "drain and capture") {
                    Ok(Ok(mut repetition)) => {
                        if let Err(error) =
                            keep_timings(&options.artifacts, &step.id, phase, &repetition.timings)
                        {
                            repetition.invalid.push(format!(
                                "the server timing evidence could not be kept: {error}"
                            ));
                        }
                        let after = provisioned_vcpus();
                        repetition.provisioned_vcpus = available.filter(|_| available == after);
                        match (available, after) {
                            (Some(before), Some(after)) if before == after => {}
                            (before, after) => repetition.invalid.push(format!(
                                "provisioned vCPUs were {} at the window's open and {} at its close",
                                before.map_or_else(|| "unavailable".to_owned(), |p| p.to_string()),
                                after.map_or_else(|| "unavailable".to_owned(), |p| p.to_string()),
                            )),
                        }
                        if phase != Phase::Warmup {
                            repetitions.push(repetition);
                        }
                    }
                    Ok(Err(error)) => {
                        reasons
                            .push(Reason::new("workload.failed", error.to_string()).at(&step.id));
                        break;
                    }
                    Err(reason) => {
                        stopped = Some(reason.at(&step.id));
                        break;
                    }
                }
            }
            heap.capture(&step.id).await;
            if profiling.is_some() {
                profile.absorb(workload.capture_clients(&step.id, &profiles).await);
            }
            saving = Some(tokio::task::spawn_blocking(move || profiler.finish()));
        }
        if let Some(ledger) = &mut lifecycle {
            let reading = Instant::now();
            let left = ledger.drain_left();
            match tokio::time::timeout(left, workload.outstanding()).await {
                Ok(Ok(outstanding)) => ledger.outstanding = outstanding,
                Ok(Err(error)) => validity.push(Reason::new(
                    "evidence.missing",
                    format!(
                        "outstanding publication and maintenance work could not be read: {error}"
                    ),
                )),
                Err(_) => validity.push(Reason::new(
                    "phase.deadline",
                    "the drain and capture allowance ran out reading outstanding work",
                )),
            }
            ledger.drain_seconds += reading.elapsed().as_secs_f64();
        }
        let grace = lifecycle.as_ref().map_or(FINISH_GRACE, |ledger| {
            ledger.drain_left() + ledger.teardown_left()
        });
        let finishing = Instant::now();
        let finished = tokio::time::timeout(grace, workload.finish()).await;
        if let Some(ledger) = &mut lifecycle {
            ledger.finish(finishing.elapsed());
        }
        match finished {
            Ok(Ok(())) => {}
            Ok(Err(error)) => validity.push(Reason::new(
                "workload.failed",
                format!("drain or stop failed: {error}"),
            )),
            Err(_) => validity.push(Reason::new(
                "budget.exceeded",
                format!(
                    "drain and stop did not finish within {:.0}s",
                    grace.as_secs_f64()
                ),
            )),
        }
        if let Some(reason) = stopped.take() {
            validity.push(reason);
        }
        let sampling = Instant::now();
        host_after = host(sample).await;
        if let Some(ledger) = &mut lifecycle {
            ledger.drain_seconds += sampling.elapsed().as_secs_f64();
        }
        validity.extend(host_reasons(
            host_after.as_ref(),
            &declaration.validity,
            "after",
        ));
        worked = Some(clock.elapsed());
        if let Some(last) = saving {
            profile.absorb(saved(last).await);
        }
        heap.finish(&mut profile);
        if profile.requested && profile.captures.is_empty() && profile.notes.is_empty() {
            profile.absorb(ProfileNote {
                notes: vec!["no measured step was profiled".to_owned()],
                ..ProfileNote::default()
            });
        }
    }
    let elapsed = worked.unwrap_or_else(|| clock.elapsed()) + options.setup_spent;
    if let Some(ledger) = &lifecycle {
        validity.extend(ledger.overruns());
    }
    if elapsed > Duration::from_secs(timing.budget_seconds) {
        validity.push(Reason::new(
            "budget.exceeded",
            format!(
                "the run took {:.1}s, over its {}s budget",
                elapsed.as_secs_f64(),
                timing.budget_seconds
            ),
        ));
    }
    let complete = admitted
        && validity.is_empty()
        && declaration
            .steps
            .iter()
            .filter(|step| step.kind == StepKind::Measured && declaration.selected(&step.id))
            .all(|step| {
                measured
                    .get(&step.id)
                    .is_some_and(|(reps, _)| reps.len() == usize::from(timing.repetitions))
            });
    if complete && elapsed < Duration::from_secs(timing.minimum_seconds) {
        validity.push(Reason::new(
            "duration.short",
            format!(
                "the complete run took {:.1}s, under its {}s minimum",
                elapsed.as_secs_f64(),
                timing.minimum_seconds
            ),
        ));
    }
    let finished_at = Utc::now();
    let mut steps = judge(&declaration, measured, admitted);
    if timing.phases.is_none() {
        for repetition in steps
            .iter_mut()
            .flat_map(|step| step.repetitions.iter_mut())
        {
            for reading in repetition.metrics.values_mut() {
                reading.pool = None;
            }
        }
    }
    let validity_record = ValidityRecord {
        thresholds: declaration.validity,
        host_before,
        host_after,
        budget_seconds: timing.budget_seconds,
        minimum_seconds: timing.minimum_seconds,
        elapsed_seconds: elapsed.as_secs_f64(),
        reasons: validity.clone(),
    };
    let mut reasons = validity;
    reasons.extend(steps.iter().flat_map(|step| step.reasons.iter().cloned()));
    let verdict = Verdict::of(&reasons);
    Record {
        schema_version: SCHEMA_VERSION,
        engine: declaration.engine,
        experiment: declaration.experiment.clone(),
        level: declaration.level,
        question: declaration.question.clone(),
        started_at,
        finished_at,
        elapsed_seconds: elapsed.as_secs_f64(),
        commit: declaration.source.commit.clone(),
        dirty: declaration.source.dirty,
        dirty_override: declaration.source.dirty_override,
        diff_sha256: declaration.source.diff_sha256.clone(),
        host: declaration.host.clone(),
        build: declaration.build.clone(),
        command: declaration.command.clone(),
        variables: declaration.variables.clone(),
        controls: Controls {
            timing,
            validity: declaration.validity,
            values: declaration.controls.clone(),
            selection: declaration.selection.clone(),
        },
        criteria: CriteriaRecord {
            metrics: declaration.metrics.clone(),
            criteria: declaration.criteria.clone(),
        },
        steps,
        validity: validity_record,
        verdict,
        reasons,
        artifacts: Artifacts {
            directory: Some(options.artifacts.display().to_string()),
            profile,
        },
        mode: declaration.mode,
        lifecycle,
    }
}

/// The note of a step's CPU captures once `saving` finished them; a
/// panicked save is noted, never lost silently.
async fn saved(saving: tokio::task::JoinHandle<ProfileNote>) -> ProfileNote {
    saving.await.unwrap_or_else(|_| ProfileNote {
        notes: vec!["the profiler thread panicked".to_owned()],
        ..ProfileNote::default()
    })
}

/// Awaits `work` until the budget `deadline` or an interrupt, whichever is
/// first; the reason names which stopped it.
async fn bounded<T>(
    deadline: Instant,
    interrupt: &CancellationToken,
    work: impl Future<Output = T>,
) -> Result<T, Reason> {
    tokio::select! {
        biased;
        () = interrupt.cancelled() => Err(Reason::new("interrupted", "the run was interrupted; admission stopped")),
        outcome = tokio::time::timeout_at(deadline, work) => outcome.map_err(|_| {
            Reason::new("budget.exceeded", "the budget ran out; admission stopped")
        }),
    }
}

/// `outcome` of a [`bounded`] call under the case `lifecycle`: a budget
/// stop of a case is the named `phase`'s allowance running out, since every
/// case call is bounded by its phase before the budget.
fn phase_bounded<T>(
    outcome: Result<T, Reason>,
    lifecycle: Option<&Lifecycle>,
    phase: &str,
) -> Result<T, Reason> {
    outcome.map_err(|reason| match lifecycle {
        Some(_) if reason.code == "budget.exceeded" => Reason::new(
            "phase.deadline",
            format!("the {phase} allowance ran out; admission stopped"),
        ),
        _ => reason,
    })
}

/// Why a host sample taken `when` makes the run invalid.
fn host_reasons(sample: Option<&HostLoad>, validity: &Validity, when: &str) -> Vec<Reason> {
    match sample {
        None => vec![Reason::new(
            "host.unreadable",
            format!("the host sample {when} the run could not be read"),
        )],
        Some(load) if load.fraction() > validity.host_busy_fraction => vec![Reason::new(
            "host.busy",
            format!(
                "{when} the run, non-benchmark processes used {:.2} of {} logical CPUs ({:.1}% > {:.0}%): {:?}",
                load.other_cores,
                load.logical_cpus,
                load.fraction() * 100.0,
                validity.host_busy_fraction * 100.0,
                load.busiest
            ),
        )],
        Some(_) => Vec::new(),
    }
}

/// P for this host: the logical CPUs available to this process and the
/// server children it launches, which share them. `None` when the platform
/// cannot say. Portable through the standard library alone.
fn provisioned_vcpus() -> Option<u64> {
    std::thread::available_parallelism()
        .ok()
        .and_then(|vcpus| u64::try_from(vcpus.get()).ok())
}

/// Turns measured repetitions into judged step records, including derived
/// steps and unmeasured or omitted declared steps.
fn judge(
    declaration: &Declaration,
    mut measured: BTreeMap<String, (Vec<Repetition>, Vec<Reason>)>,
    admitted: bool,
) -> Vec<StepRecord> {
    for (repetitions, _) in measured.values_mut() {
        for repetition in repetitions.iter_mut() {
            repetition.derive(&declaration.metrics);
        }
    }
    let mut records: Vec<StepRecord> = Vec::new();
    for step in &declaration.steps {
        let timing = declaration.step_timing(step);
        let (status, repetitions, mut reasons, params) = if declaration.selected(&step.id) {
            match &step.kind {
                StepKind::Measured => match measured.remove(&step.id) {
                    Some((repetitions, reasons)) if !repetitions.is_empty() => {
                        (Status::Measured, repetitions, reasons, step.params.clone())
                    }
                    started => (
                        Status::Unmeasured,
                        Vec::new(),
                        started
                            .map(|(_, reasons)| reasons)
                            .unwrap_or_default()
                            .into_iter()
                            .chain([Reason::new(
                                "step.unmeasured",
                                if admitted {
                                    "the run stopped before this step measured a repetition"
                                } else {
                                    "the run admitted no workload"
                                },
                            )
                            .at(&step.id)])
                            .collect(),
                        step.params.clone(),
                    ),
                },
                StepKind::Peak { of, metric } => peak(&records, of, metric, step),
                StepKind::Scaling {
                    base,
                    scaled,
                    factor,
                    metric,
                } => scaling(&records, base, scaled, *factor, metric, step),
            }
        } else {
            (Status::Omitted, Vec::new(), Vec::new(), step.params.clone())
        };
        if status == Status::Measured {
            if repetitions.len() < usize::from(timing.repetitions) {
                reasons.push(
                    Reason::new(
                        "repetition.missing",
                        format!(
                            "{} of {} repetitions were measured",
                            repetitions.len(),
                            timing.repetitions
                        ),
                    )
                    .at(&step.id),
                );
            }
            for (index, repetition) in repetitions.iter().enumerate() {
                reasons.extend(
                    repetition_reasons(repetition, &declaration.validity, declaration.mode)
                        .into_iter()
                        .map(|reason| Reason {
                            message: format!("repetition {}: {}", index + 1, reason.message),
                            ..reason.at(&step.id)
                        }),
                );
            }
        }
        let summary = summarize(declaration, &repetitions);
        let aggregate = (status == Status::Measured && declaration.timing.phases.is_some())
            .then(|| Repetition::aggregate(&declaration.metrics, &repetitions));
        let mut criteria = Vec::new();
        if matches!(status, Status::Measured | Status::Derived) {
            for criterion in declaration
                .criteria
                .iter()
                .filter(|criterion| criterion.steps.contains(&step.id))
            {
                let judgment = judge_criterion(
                    criterion,
                    &repetitions,
                    aggregate.as_ref(),
                    &declaration.validity,
                    timing.repetitions,
                    declaration.mode,
                );
                if judgment.outcome == Some(Outcome::Invalid) {
                    reasons.push(
                        Reason::new(
                            "criterion.invalid",
                            judgment.reason.clone().unwrap_or_default(),
                        )
                        .at(&step.id)
                        .of(&criterion.id),
                    );
                }
                criteria.push(judgment);
            }
        }
        let verdict = Verdict::of(&reasons);
        records.push(StepRecord {
            id: step.id.clone(),
            params,
            status,
            timing,
            repetitions,
            aggregate,
            summary,
            criteria,
            verdict,
            reasons,
        });
    }
    records
}

/// The step record type a derived step is built from.
type Derived = (
    Status,
    Vec<Repetition>,
    Vec<Reason>,
    BTreeMap<String, Value>,
);

/// The peak step among `of` by median `metric`, copying its repetitions.
fn peak(records: &[StepRecord], of: &[String], metric: &str, step: &Step) -> Derived {
    let best = records
        .iter()
        .filter(|record| {
            of.contains(&record.id)
                && record.status == Status::Measured
                && record.verdict != Verdict::Invalid
        })
        .filter_map(|record| {
            let values: Vec<Reading> = record
                .repetitions
                .iter()
                .map(|rep| rep.reading(metric))
                .collect();
            Reading::median(&values)
                .exact_value()
                .map(|value| (value, record))
        })
        .max_by(|left, right| left.0.total_cmp(&right.0));
    match best {
        Some((_, record)) => {
            let mut params = record.params.clone();
            params.insert("peak_of".to_owned(), Value::String(record.id.clone()));
            (
                Status::Derived,
                record.repetitions.clone(),
                Vec::new(),
                params,
            )
        }
        None => (
            Status::Derived,
            Vec::new(),
            vec![
                Reason::new(
                    "step.unmeasured",
                    format!("no valid step of {of:?} has a median `{metric}`"),
                )
                .at(&step.id),
            ],
            step.params.clone(),
        ),
    }
}

/// The scaling step `scaled/(factor × base)` of `metric`, per repetition.
fn scaling(
    records: &[StepRecord],
    base: &str,
    scaled: &str,
    factor: f64,
    metric: &str,
    step: &Step,
) -> Derived {
    let find = |id: &str| {
        records
            .iter()
            .find(|record| record.id == id && record.status == Status::Measured)
    };
    let (Some(base), Some(scaled)) = (find(base), find(scaled)) else {
        return (
            Status::Derived,
            Vec::new(),
            vec![
                Reason::new("step.unmeasured", "a scaling input step was not measured")
                    .at(&step.id),
            ],
            step.params.clone(),
        );
    };
    let repetitions = base
        .repetitions
        .iter()
        .zip(&scaled.repetitions)
        .map(|(one, many)| {
            let mut repetition = Repetition {
                seconds: many.seconds,
                ..Repetition::default()
            };
            let (q1, qn) = (one.reading(metric), many.reading(metric));
            let value = match (q1.exact_value(), qn.exact_value()) {
                (Some(q1), Some(qn)) if q1 > 0.0 => Reading::exact(qn / (factor * q1)),
                _ => Reading::missing("a scaling input rate is missing or zero"),
            };
            repetition.set("scaling", value);
            repetition.set("base_rate", q1);
            repetition.set("scaled_rate", qn);
            repetition
        })
        .collect();
    (
        Status::Derived,
        repetitions,
        Vec::new(),
        step.params.clone(),
    )
}

/// Why one repetition is invalid: workload reasons, process loss and
/// missing or zero CPU, driver saturation, and open-loop misses or lateness. A smoke, which only shows the workflow runs, does not judge
/// driver saturation: a busy driver changes measured latency, not whether
/// the workflow works.
fn repetition_reasons(repetition: &Repetition, validity: &Validity, mode: Mode) -> Vec<Reason> {
    let mut reasons: Vec<Reason> = repetition
        .invalid
        .iter()
        .map(|reason| Reason::new("workload.invalid", reason.clone()))
        .collect();
    match repetition.server_cpu_seconds {
        None => reasons.push(Reason::new(
            "reading.missing",
            "server CPU-seconds are unavailable: a process was lost or a counter reset",
        )),
        Some(cpu) if cpu <= 0.0 => reasons.push(Reason::new(
            "reading.missing",
            "server CPU-seconds were zero: the window measured no server work",
        )),
        Some(_) => {}
    }
    match repetition.driver_cpu_seconds {
        None => reasons.push(Reason::new(
            "reading.missing",
            "driver CPU-seconds are unavailable",
        )),
        Some(driver) if mode == Mode::Full && repetition.seconds > 0.0 => {
            let cores = driver / repetition.seconds;
            let limit = validity.driver_cores * validity.driver_fraction;
            if cores > limit {
                reasons.push(Reason::new(
                    "driver.saturated",
                    format!("the driver used {cores:.2} cores, over {limit:.2} (80% of its {} core budget)", validity.driver_cores),
                ));
            }
        }
        Some(_) => {}
    }
    if let Some(arrivals) = repetition.arrivals {
        if arrivals.missed > 0 {
            reasons.push(Reason::new(
                "arrivals.missed",
                format!(
                    "{} of {} scheduled arrivals were never sent ({} sent, {} completed)",
                    arrivals.missed, arrivals.scheduled, arrivals.sent, arrivals.completed
                ),
            ));
        }
        match arrivals.lateness_p99_us {
            Some(late) if late > validity.lateness_p99_us => reasons.push(Reason::new(
                "arrivals.late",
                format!(
                    "send-lateness p99 {late:.0} µs exceeds {:.0} µs",
                    validity.lateness_p99_us
                ),
            )),
            None if arrivals.sent > 0 => {
                reasons.push(Reason::new(
                    "reading.missing",
                    "send lateness was not recorded",
                ));
            }
            _ => {}
        }
    }
    reasons
}

/// Each metric's median, minimum, maximum, and spread over `repetitions`.
fn summarize(declaration: &Declaration, repetitions: &[Repetition]) -> BTreeMap<String, Summary> {
    let mut ids: Vec<&String> = repetitions
        .iter()
        .flat_map(|rep| rep.metrics.keys())
        .collect();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .map(|id| {
            let readings: Vec<Reading> = repetitions.iter().map(|rep| rep.reading(id)).collect();
            let exact: Option<Vec<f64>> = readings.iter().map(Reading::exact_value).collect();
            let (min, max) =
                exact
                    .filter(|values| !values.is_empty())
                    .map_or((None, None), |values| {
                        (
                            values.iter().copied().reduce(f64::min),
                            values.iter().copied().reduce(f64::max),
                        )
                    });
            let unit = declaration
                .metric(id)
                .map_or_else(String::new, |metric| metric.unit.clone());
            let summary = Summary {
                unit,
                median: Reading::median(&readings),
                min,
                max,
                spread: min.zip(max).map(|(min, max)| max - min),
            };
            (id.clone(), summary)
        })
        .collect()
}

/// Judges one criterion over a step's repetitions. A target reports met,
/// missed or unsettled from its median repetition. A correctness or
/// integrity criterion is met only when every repetition meets it, and is
/// otherwise [`Outcome::Invalid`]. A smoke judges no criterion: once its
/// evidence is present, every criterion is inapplicable, so a smoke is only
/// VALID or INVALID; a missing reading or an empty percentile population is
/// [`Outcome::Invalid`] in both modes. Under the L1 case protocol a target
/// judges the case `aggregate` the summary displays instead of the median
/// repetition, at its recorded precision and bucket bounds and before any
/// display rounding; sample floors and correctness criteria still hold per
/// window.
fn judge_criterion(
    criterion: &Criterion,
    repetitions: &[Repetition],
    aggregate: Option<&Repetition>,
    validity: &Validity,
    expected: u8,
    mode: Mode,
) -> Judgment {
    let values: Vec<Reading> = repetitions
        .iter()
        .map(|rep| rep.reading(&criterion.metric).bare())
        .collect();
    let observed = aggregate.map_or_else(
        || Reading::median(&values),
        |aggregate| aggregate.reading(&criterion.metric).bare(),
    );
    let mut judgment = Judgment {
        criterion: criterion.id.clone(),
        observed,
        values: values.clone(),
        outcome: Some(Outcome::Invalid),
        reason: None,
        applicable: true,
    };
    let inapplicable = |mut judgment: Judgment, reason: String| {
        judgment.outcome = None;
        judgment.applicable = false;
        judgment.reason = Some(reason);
        judgment
    };
    if values.len() < usize::from(expected) {
        judgment.reason = Some(format!(
            "{} of {expected} repetitions have evidence",
            values.len()
        ));
        return judgment;
    }
    if let Some(missing) = values.iter().find(|value| !value.is_present()) {
        judgment.reason = Some(format!(
            "`{}` is missing: {}",
            criterion.metric,
            missing.reason.as_deref().unwrap_or("not read")
        ));
        return judgment;
    }
    let required = match criterion.percentile {
        Some(99) => Some(validity.p99_samples),
        Some(95) => Some(validity.p95_samples),
        _ => None,
    };
    if let Some(required) = required
        && let Some((index, short)) = values
            .iter()
            .enumerate()
            .find(|(_, value)| value.samples.unwrap_or(0) < required)
    {
        if mode == Mode::Smoke && short.samples.unwrap_or(0) > 0 {
            let counts: Vec<String> = values
                .iter()
                .map(|value| value.samples.unwrap_or(0).to_string())
                .collect();
            return inapplicable(
                judgment,
                format!(
                    "not judgeable at smoke size: p{} has {} samples per repetition, under its {required}-sample floor",
                    criterion.percentile.unwrap_or_default(),
                    counts.join("/")
                ),
            );
        }
        judgment.reason = Some(format!(
            "repetition {} has {} samples behind p{}, under {required}",
            index + 1,
            short.samples.unwrap_or(0),
            criterion.percentile.unwrap_or_default()
        ));
        return judgment;
    }
    if mode == Mode::Smoke {
        return inapplicable(
            judgment,
            "not judged in a smoke: a smoke only shows the workflow runs".to_owned(),
        );
    }
    let Rule { op, limit, over } = criterion.rule;
    if over == Over::Median && !judgment.observed.is_present() {
        judgment.reason = Some(format!(
            "the case aggregate of `{}` is missing: {}",
            criterion.metric,
            judgment.observed.reason.as_deref().unwrap_or("not read")
        ));
        return judgment;
    }
    let median = judgment.observed.clone();
    let judged: Vec<&Reading> = match over {
        Over::Median => vec![&median],
        Over::Every => values.iter().collect(),
    };
    let mut outcome = Some(true);
    for (index, reading) in judged.iter().enumerate() {
        let Some((lo, open, hi)) = reading.interval() else {
            outcome = None;
            break;
        };
        match op.judge(lo, open, hi, limit) {
            Some(true) => {}
            Some(false) => {
                if over == Over::Every {
                    judgment.observed = (*reading).clone();
                    judgment.reason = Some(format!(
                        "repetition {} read {}, missing {} {}",
                        index + 1,
                        reading.cell(""),
                        op.symbol(),
                        number(limit)
                    ));
                }
                outcome = Some(false);
                break;
            }
            None => {
                outcome = None;
                judgment.reason = Some(format!(
                    "the reading {} straddles {} {}; a censored reading cannot establish it",
                    reading.cell(""),
                    op.symbol(),
                    number(limit)
                ));
                break;
            }
        }
    }
    judgment.outcome = Some(match (outcome, over) {
        (Some(true), _) => Outcome::Met,
        (Some(false), Over::Median) => {
            judgment.reason = Some(format!(
                "{} {} misses {} {}",
                if aggregate.is_some() {
                    "aggregate"
                } else {
                    "median"
                },
                judgment.observed.cell(""),
                op.symbol(),
                number(limit)
            ));
            Outcome::Missed
        }
        (None, Over::Median) => Outcome::Unsettled,
        (_, Over::Every) => Outcome::Invalid,
    });
    judgment
}

impl Record {
    /// The record's file name for collision number `n` (1 is the first):
    /// `YYYYMMDDTHHMMSSZ-<sha9>.json`, then `-2`, `-3`, ...
    #[must_use]
    pub fn file_name(&self, n: u32) -> String {
        let short: String = self.commit.chars().take(9).collect();
        let stamp = self.started_at.format("%Y%m%dT%H%M%SZ");
        if n <= 1 {
            format!("{stamp}-{short}.json")
        } else {
            format!("{stamp}-{short}-{n}.json")
        }
    }

    /// Publishes the record under `<engine>/<experiment>/` of
    /// `root/`[`HISTORY_DIRECTORY`] when it may enter history
    /// ([`Record::historical`]), of `root/`[`SMOKE_DIRECTORY`] for a smoke,
    /// and of `root/`[`LOCAL_DIRECTORY`] otherwise: written to a temporary
    /// file in that directory, synced, then linked to the first free
    /// collision-numbered name, so history is never overwritten and a
    /// partial file never appears complete.
    ///
    /// # Errors
    ///
    /// Returns [`ExperimentError::Persist`] when the directory, write, sync,
    /// or rename fails.
    pub fn publish(&self, root: &Path) -> Result<PathBuf, ExperimentError> {
        let base = if self.historical() {
            HISTORY_DIRECTORY
        } else if self.mode == Mode::Smoke {
            SMOKE_DIRECTORY
        } else {
            LOCAL_DIRECTORY
        };
        let directory = root
            .join(base)
            .join(self.engine.name())
            .join(&self.experiment);
        let fail = |path: &Path| {
            let path = path.to_path_buf();
            move |source| ExperimentError::Persist { path, source }
        };
        std::fs::create_dir_all(&directory).map_err(fail(&directory))?;
        let json = serde_json::to_vec_pretty(self).map_err(|error| ExperimentError::Persist {
            path: directory.clone(),
            source: std::io::Error::other(error),
        })?;
        let mut file = tempfile::NamedTempFile::new_in(&directory).map_err(fail(&directory))?;
        file.write_all(&json).map_err(fail(file.path()))?;
        file.write_all(b"\n").map_err(fail(file.path()))?;
        file.as_file().sync_all().map_err(fail(file.path()))?;
        let mut n = 1;
        loop {
            let destination = directory.join(self.file_name(n));
            match file.persist_noclobber(&destination) {
                Ok(_) => return Ok(destination),
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                    file = error.file;
                    n += 1;
                }
                Err(error) => {
                    return Err(ExperimentError::Persist {
                        path: destination,
                        source: error.error,
                    });
                }
            }
        }
    }

    /// Whether the record enters history: a full run of the whole plan from
    /// a clean source without `--profile` or Gateway's paced `--pace`
    /// diagnostic, VALID or INVALID. A `--steps`, `--profile`, `--pace`,
    /// dirty or `--smoke` run never does.
    #[must_use]
    pub fn historical(&self) -> bool {
        self.mode == Mode::Full
            && self.controls.selection.is_none()
            && !self.dirty
            && !self
                .command
                .args
                .iter()
                .any(|argument| argument == "--profile" || argument == "--pace")
    }

    /// Reads a record file.
    ///
    /// # Errors
    ///
    /// Returns a read failure, [`ExperimentError::Unsupported`] for another
    /// schema version, or [`ExperimentError::Malformed`].
    pub fn read(path: &Path) -> Result<Self, ExperimentError> {
        Self::read_record(path)
    }

    /// Reads a record, then each measured repetition's server snapshots
    /// the run kept beside it (`timings/<step>/repetition-N.json` under
    /// its artifacts directory), so [`Self::render`] lists every server
    /// series.
    ///
    /// # Errors
    ///
    /// Returns the [`Self::read`] failure.
    pub fn read_with_timings(path: &Path) -> Result<Self, ExperimentError> {
        let mut record = Self::read_record(path)?;
        let Some(directory) = record.artifacts.directory.clone() else {
            return Ok(record);
        };
        let timings = Path::new(&directory).join("timings");
        for step in &mut record.steps {
            for (index, repetition) in step.repetitions.iter_mut().enumerate() {
                let Ok(index) = u8::try_from(index) else {
                    continue;
                };
                let kept = timings
                    .join(&step.id)
                    .join(format!("{}.json", Phase::Repetition(index).label()));
                if let Ok(bytes) = std::fs::read(kept)
                    && let Ok(evidence) = serde_json::from_slice(&bytes)
                {
                    repetition.timings = evidence;
                }
            }
        }
        Ok(record)
    }

    /// Reads one record file.
    fn read_record(path: &Path) -> Result<Self, ExperimentError> {
        let text = std::fs::read_to_string(path).map_err(|source| ExperimentError::Persist {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text)
    }

    /// Parses a record, checking its version before its shape.
    ///
    /// # Errors
    ///
    /// Returns [`ExperimentError::Unsupported`] for a version other than
    /// [`SCHEMA_VERSION`] and [`ExperimentError::Malformed`] for anything not
    /// a record of that version.
    pub fn parse(text: &str) -> Result<Self, ExperimentError> {
        let value: Value = serde_json::from_str(text)
            .map_err(|error| ExperimentError::Malformed(error.to_string()))?;
        match value.get("schema_version").and_then(Value::as_u64) {
            Some(SCHEMA_VERSION) => {}
            Some(other) => return Err(ExperimentError::Unsupported(other)),
            None => {
                return Err(ExperimentError::Malformed(
                    "schema_version is missing".to_owned(),
                ));
            }
        }
        serde_json::from_value(value).map_err(|error| ExperimentError::Malformed(error.to_string()))
    }

    /// The record's exit code.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        self.verdict.exit_code()
    }

    /// Compares this record, the earlier run, with `after`, a later run of
    /// the same experiment: the Comparison section of a benchmark report.
    ///
    /// Each step both runs measured gets a table, in `after`'s step order,
    /// with a row for every metric either summarizes, diagnostics aside as in
    /// [`Record::render`]'s step detail: each run's median and
    /// repetition range, the change of the medians, and whether `after`'s
    /// range lies outside `before`'s; a histogram median's change compares
    /// bucket upper bounds. An outside change names its direction,
    /// and whether that is better for a metric that declares a
    /// [`Direction`]; an overlapping one is no measured change. A step only
    /// one run measured is listed by name.
    ///
    /// # Errors
    ///
    /// Returns a message when the runs are of different engines,
    /// experiments, or levels, either is not VALID, or either was measured
    /// under the L1 case protocol, whose comparison methodology is not yet
    /// defined.
    pub fn compare(&self, after: &Self) -> Result<String, String> {
        if [self, after]
            .iter()
            .any(|record| record.lifecycle.is_some())
        {
            return Err(
                "an L1 case-protocol record is not comparable with this reader, which compares schema-3 experiment records"
                    .to_owned(),
            );
        }
        if (self.engine, &self.experiment, self.level)
            != (after.engine, &after.experiment, after.level)
        {
            return Err(format!(
                "{} {} and {} {} are different experiments",
                self.engine.name(),
                self.experiment,
                after.engine.name(),
                after.experiment
            ));
        }
        if let Some(invalid) = [self, after]
            .into_iter()
            .find(|record| record.verdict != Verdict::Valid)
        {
            return Err(format!(
                "the {} run of {} is INVALID; compare only VALID runs",
                &invalid.commit[..invalid.commit.len().min(9)],
                invalid.started_at.to_rfc3339()
            ));
        }
        let mut out = String::new();
        let _ = writeln!(
            out,
            "# compare {} L{} {}\n",
            self.engine.name(),
            u8::from(self.level),
            self.experiment
        );
        for (label, record) in [("before", self), ("after", after)] {
            let _ = writeln!(
                out,
                "{label}: commit {}{} | started {}",
                record.commit,
                if record.dirty { " (dirty)" } else { "" },
                record.started_at.to_rfc3339()
            );
        }
        out.push_str(
            "\nMedians of the repetitions, [min–max] their range. A change is \
             outside the spread when the ranges do not overlap; otherwise it is \
             no measured change. A histogram median shows its bucket: its change \
             compares the upper bounds, and with no exact range it has no spread \
             verdict. Diagnostic readings are in each run's report.\n",
        );
        let earlier = self.measured();
        let later = after.measured();
        let direction = |id: &str| {
            after
                .criteria
                .metrics
                .iter()
                .find(|metric| metric.id == id)
                .and_then(|metric| metric.direction)
        };
        for step in after.steps.iter().filter(|step| !step.summary.is_empty()) {
            let Some(before) = earlier.get(step.id.as_str()) else {
                continue;
            };
            let _ = writeln!(out, "\n## {}\n", step.id);
            out.push_str("| metric | before | after | change | outside spread |\n");
            out.push_str("|---|---|---|---|---|\n");
            let ids: std::collections::BTreeSet<&String> = before
                .summary
                .keys()
                .chain(step.summary.keys())
                .filter(|id| !id.starts_with(DIAGNOSTIC))
                .collect();
            for id in ids {
                let (was, now) = (before.summary.get(id), step.summary.get(id));
                let unit = now.or(was).map_or("", |summary| summary.unit.as_str());
                let cell = |summary: Option<&Summary>| {
                    summary.map_or_else(|| "—".to_owned(), Summary::spread_cell)
                };
                let exact = |summary: Option<&Summary>| summary.and_then(|s| s.median.value);
                let change = match (exact(was), exact(now)) {
                    (Some(was), Some(now)) if was != 0.0 => {
                        format!("{:+.1}%", (now - was) / was * 100.0)
                    }
                    _ => "—".to_owned(),
                };
                let range = |summary: Option<&Summary>| summary.and_then(|s| s.min.zip(s.max));
                let outside = match (range(was), range(now), exact(was), exact(now)) {
                    (Some((low, high)), Some((min, max)), Some(was), Some(now))
                        if min > high || max < low =>
                    {
                        let higher = now > was;
                        let verdict = match (direction(id), higher) {
                            (Some(Direction::Higher), true) | (Some(Direction::Lower), false) => {
                                ", better"
                            }
                            (Some(_), _) => ", worse",
                            (None, _) => "",
                        };
                        format!(
                            "outside: {}{verdict}",
                            if higher { "higher" } else { "lower" }
                        )
                    }
                    (Some(_), Some(_), _, _) => "no measured change".to_owned(),
                    _ => "—".to_owned(),
                };
                let _ = writeln!(
                    out,
                    "| {id} ({unit}) | {} | {} | {change} | {outside} |",
                    cell(was),
                    cell(now)
                );
            }
        }
        for (label, only, other) in [("before", &earlier, &later), ("after", &later, &earlier)] {
            let only: Vec<&str> = only
                .keys()
                .filter(|id| !other.contains_key(*id))
                .copied()
                .collect();
            if !only.is_empty() {
                let _ = writeln!(out, "\nMeasured only {label}: {}", only.join(", "));
            }
        }
        Ok(out)
    }

    /// Each step this record measured, by id.
    fn measured(&self) -> BTreeMap<&str, &StepRecord> {
        self.steps
            .iter()
            .filter(|step| !step.summary.is_empty())
            .map(|step| (step.id.as_str(), step))
            .collect()
    }

    /// Renders the report: a title, the common table with one row per
    /// step and criterion, engine detail per step, and every reason. The
    /// same text comes from a fresh or a re-read record.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "# {} L{} {} — {}\n\n{}\n\ncommit {}{} | host {} {} {} | elapsed {:.1}s of {}s | started {}\n",
            self.engine.name(),
            u8::from(self.level),
            self.experiment,
            self.verdict.label(),
            self.question,
            self.commit,
            if self.dirty {
                format!(
                    " dirty (override, diff {})",
                    self.diff_sha256.as_deref().unwrap_or("?")
                )
            } else {
                String::new()
            },
            self.host.fingerprint,
            self.host.os_version,
            self.host.arch,
            self.elapsed_seconds,
            self.validity.budget_seconds,
            self.started_at.to_rfc3339(),
        );
        if self.mode == Mode::Smoke {
            out.push_str("SMOKE: functional pipeline evidence at reduced controls, never a performance qualification.\n\n");
        }
        if let Some(method) = self
            .controls
            .values
            .get("timing")
            .and_then(|timing| timing.get("method"))
            .and_then(Value::as_str)
        {
            let _ = writeln!(out, "timing (schema {}): {method}\n", self.schema_version);
        }
        self.render_comparison(&mut out);
        out.push_str(REPORT_HEADER);
        out.push('\n');
        out.push_str("|---|---|---|---|---|---|---|---|---|---|---|\n");
        for step in &self.steps {
            let cells = self.step_cells(step);
            let units = |id: &str| {
                self.criteria
                    .metrics
                    .iter()
                    .find(|metric| metric.id == id)
                    .map_or("", |metric| metric.unit.as_str())
            };
            if step.criteria.is_empty() {
                let verdict = if step.status == Status::Omitted {
                    "— omitted by selection".to_owned()
                } else {
                    step.verdict.label().to_owned()
                };
                let _ = writeln!(
                    out,
                    "| {cells} | — no criterion applies | — no criterion applies | {verdict} |"
                );
                continue;
            }
            for judgment in &step.criteria {
                let criterion = self
                    .criteria
                    .criteria
                    .iter()
                    .find(|criterion| criterion.id == judgment.criterion);
                let unit = criterion.map_or("", |criterion| units(&criterion.metric));
                let rule = criterion.map_or_else(
                    || judgment.criterion.clone(),
                    |criterion| criterion.describe(unit),
                );
                let spread = step
                    .summary
                    .get(criterion.map_or("", |criterion| criterion.metric.as_str()))
                    .and_then(|summary| {
                        summary
                            .spread
                            .map(|spread| format!(" spread {}", number(spread)))
                    })
                    .unwrap_or_default();
                let verdict = match judgment.outcome {
                    Some(outcome) => match &judgment.reason {
                        Some(reason) if outcome != Outcome::Met => {
                            format!("{}: {reason}", outcome.label())
                        }
                        _ => outcome.label().to_owned(),
                    },
                    None => format!(
                        "— inapplicable: {}",
                        judgment.reason.as_deref().unwrap_or("not judgeable")
                    ),
                };
                let _ = writeln!(
                    out,
                    "| {cells} | {}: {rule} | {}{spread} | {verdict} |",
                    judgment.criterion,
                    judgment.observed.cell(unit)
                );
            }
        }
        out.push_str("\n## Step detail\n\n");
        for step in self.steps.iter().filter(|step| !step.summary.is_empty()) {
            let _ = writeln!(
                out,
                "- {} ({:?}, {} repetitions):",
                step.id,
                step.status,
                step.repetitions.len()
            );
            for (id, summary) in step
                .summary
                .iter()
                .filter(|(id, _)| !id.starts_with(DIAGNOSTIC))
            {
                let range = summary
                    .min
                    .zip(summary.max)
                    .map(|(min, max)| format!(" min {} max {}", number(min), number(max)))
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "  - {id}: median {}{range}",
                    summary.median.cell(&summary.unit)
                );
            }
            for (index, repetition) in step.repetitions.iter().enumerate() {
                if let Some(arrivals) = repetition.arrivals {
                    let _ = writeln!(
                        out,
                        "  - repetition {} arrivals: {} scheduled, {} sent, {} completed, {} missed, lateness p99 {}",
                        index + 1,
                        arrivals.scheduled,
                        arrivals.sent,
                        arrivals.completed,
                        arrivals.missed,
                        arrivals.lateness_p99_us.map_or_else(
                            || "— not recorded".to_owned(),
                            |late| format!("{} µs", number(late))
                        )
                    );
                }
                for observation in &repetition.observations {
                    let _ = writeln!(out, "  - repetition {} observed: {observation}", index + 1);
                }
            }
            self.render_diagnostics(&mut out, step);
            Self::render_server(&mut out, step);
        }
        if !self.reasons.is_empty() {
            out.push_str("\n## Reasons\n\n");
            for reason in &self.reasons {
                let place = [reason.step.as_deref(), reason.criterion.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join("/");
                let _ = writeln!(out, "- {} {}: {}", reason.code, place, reason.message);
            }
        }
        self.render_profile(&mut out);
        let _ = writeln!(
            out,
            "\nverdict: {} (exit {})",
            self.verdict.label(),
            self.exit_code()
        );
        out
    }

    /// Writes the side-by-side summary the record's `compare` control
    /// declares, ahead of the common table, so a reader compares like with
    /// like without picking rows out of it. Measured steps are grouped by
    /// the params that tell them apart, all but `by`; each group is one
    /// table with one row per declared metric's median and one column per
    /// `by` value, under the group's declared note. A record without the
    /// control renders without the summary.
    fn render_comparison(&self, out: &mut String) {
        let Some(comparison) = self
            .controls
            .values
            .get("compare")
            .and_then(|value| serde_json::from_value::<Comparison>(value.clone()).ok())
        else {
            return;
        };
        let steps: Vec<&StepRecord> = self
            .steps
            .iter()
            .filter(|step| !step.summary.is_empty())
            .collect();
        let keys: BTreeSet<&String> = steps
            .iter()
            .flat_map(|step| step.params.keys())
            .filter(|key| **key != comparison.by)
            .collect();
        let varying: Vec<&String> = keys
            .into_iter()
            .filter(|key| {
                let first = steps.first().and_then(|step| step.params.get(*key));
                steps.iter().any(|step| step.params.get(*key) != first)
            })
            .collect();
        let mut columns: Vec<&Value> = Vec::new();
        let mut groups: Vec<(String, Vec<&StepRecord>)> = Vec::new();
        for step in steps {
            let Some(column) = step.params.get(&comparison.by) else {
                continue;
            };
            if !columns.contains(&column) {
                columns.push(column);
            }
            let label = varying
                .iter()
                .filter_map(|key| step.params.get(*key).map(|value| format!("{key}={value}")))
                .collect::<Vec<_>>()
                .join(" ");
            match groups.iter_mut().find(|(known, _)| *known == label) {
                Some((_, members)) => members.push(step),
                None => groups.push((label, vec![step])),
            }
        }
        if groups.is_empty() {
            return;
        }
        let names: Vec<String> = columns
            .iter()
            .map(|column| {
                column
                    .as_str()
                    .map_or_else(|| column.to_string(), str::to_owned)
            })
            .collect();
        let _ = writeln!(
            out,
            "## By {}\n\nMedians of the repetitions. A histogram reading shows its bucket's upper bound; the table below has both bounds.\n",
            comparison.by
        );
        for (label, members) in groups {
            let _ = writeln!(out, "### {label}\n");
            if let Some(note) = comparison.notes.get(&label) {
                let _ = writeln!(out, "{note}\n");
            }
            let _ = writeln!(out, "| metric | {} |", names.join(" | "));
            let _ = writeln!(out, "|---|{}", "---|".repeat(names.len()));
            for metric in &comparison.metrics {
                let cells: Vec<String> = columns
                    .iter()
                    .map(|column| {
                        members
                            .iter()
                            .find(|step| step.params.get(&comparison.by) == Some(*column))
                            .and_then(|step| step.summary.get(metric))
                            .and_then(|summary| summary.median.value)
                            .map_or_else(|| "—".to_owned(), number)
                    })
                    .collect();
                let _ = writeln!(
                    out,
                    "| {metric} ({}) | {} |",
                    self.unit(metric),
                    cells.join(" | ")
                );
            }
            out.push('\n');
        }
    }

    /// Writes the profile evidence to `out`, when profiling was requested:
    /// its mode and roles, whether it completed, its captures per role and
    /// each capture's path, its notes, and the limits of its scope. A
    /// record that predates profile modes and roles profiled the server
    /// only, and says so.
    fn render_profile(&self, out: &mut String) {
        let profile = &self.artifacts.profile;
        if !profile.requested {
            return;
        }
        let scope = profile.mode.map_or_else(
            || " of server (recorded before profile modes and roles)".to_owned(),
            |mode| {
                let roles: Vec<&str> = profile.roles.iter().map(|role| role.name()).collect();
                format!(" {} of {}", mode.name(), roles.join(" and "))
            },
        );
        let role = |capture: &str| {
            let file = Path::new(capture)
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            Role::ALL
                .into_iter()
                .find(|role| {
                    let name = role.name();
                    file.starts_with(&format!("cpu-{name}-"))
                        || file.starts_with(&format!("heap-{name}-"))
                })
                .unwrap_or(Role::Server)
        };
        let per_role: Vec<String> = Role::ALL
            .into_iter()
            .filter(|named| {
                matches!(named, Role::Server | Role::Driver) || profile.roles.contains(named)
            })
            .map(|named| {
                let count = profile
                    .captures
                    .iter()
                    .filter(|capture| role(capture) == named)
                    .count();
                format!("{} {count}", named.name())
            })
            .collect();
        let _ = writeln!(
            out,
            "\nprofile{scope}: {} ({} captures: {}){}",
            if profile.complete {
                "complete"
            } else {
                "incomplete"
            },
            profile.captures.len(),
            per_role.join(", "),
            if profile.notes.is_empty() {
                String::new()
            } else {
                format!("; {}", profile.notes.join("; "))
            }
        );
        out.push_str("- scope: a CPU capture spans one measured step from its warmup through its last repetition and settle; a heap capture holds the samples since sampling started, taken after each step; neither is aligned to a repetition window, and a profiled run is diagnostic, never a qualification\n");
        for capture in &profile.captures {
            let _ = writeln!(out, "- {} capture: {capture}", role(capture).name());
        }
        for symbols in &profile.symbols {
            let kept = symbols.executable.as_ref().map_or_else(
                || "executable not kept".to_owned(),
                |kept| format!("build {} kept at {}", kept.build_id, kept.path),
            );
            let _ = writeln!(
                out,
                "- symbols: {}: {} of {} executable samples unresolved; {kept}",
                symbols.sidecar, symbols.unresolved, symbols.samples
            );
        }
    }

    /// Writes `step`'s diagnostic readings to `out`: each declared or read
    /// observation under the process role that recorded it (`driver.`
    /// observations are the driver's, every other one the server's) with its
    /// declared boundary, then its reading in every repetition, so no median
    /// conceals a slow repetition and a repetition without one says why. A
    /// step of a record that declares and read none, such as one that
    /// predates them, says they are unavailable, and one with no driver
    /// observation says the driver's are, rather than reading as zero.
    fn render_diagnostics(&self, out: &mut String, step: &StepRecord) {
        let mut observations: Vec<&str> = self
            .criteria
            .metrics
            .iter()
            .map(|metric| metric.id.as_str())
            .chain(
                step.repetitions
                    .iter()
                    .flat_map(|repetition| repetition.metrics.keys().map(String::as_str)),
            )
            .filter_map(|id| id.strip_prefix(DIAGNOSTIC)?.rsplit_once('.'))
            .map(|(observation, _)| observation)
            .collect();
        observations.sort_unstable();
        observations.dedup();
        if observations.is_empty() {
            out.push_str("  - diagnostics: — unavailable: this record declares and read none\n");
            return;
        }
        out.push_str(
            "  - diagnostics (wall time; nearest-rank bucket bounds; rows overlap and never add or subtract):\n",
        );
        let driver = format!("{}.", Role::Driver.name());
        for observation in &observations {
            let id = |stat: &str| format!("{DIAGNOSTIC}{observation}.{stat}");
            let boundary = self
                .criteria
                .metrics
                .iter()
                .find(|metric| metric.id == id("count"))
                .map_or("undeclared", |metric| metric.boundary.as_str());
            let (role, name) = observation
                .strip_prefix(&driver)
                .map_or((Role::Server, *observation), |name| (Role::Driver, name));
            let _ = writeln!(out, "    - {} {name}: {boundary}", role.name());
            for (index, repetition) in step.repetitions.iter().enumerate() {
                let count = repetition.reading(&id("count"));
                let cells = if count.is_present() {
                    std::iter::once(format!("n {}", count.cell("")))
                        .chain(DIAGNOSTIC_STATS[1..].iter().map(|(stat, unit)| {
                            let label = stat.strip_suffix("_us").unwrap_or(stat);
                            format!("{label} {}", repetition.reading(&id(stat)).cell(unit))
                        }))
                        .collect::<Vec<_>>()
                        .join(", ")
                } else {
                    count.cell("")
                };
                let _ = writeln!(out, "      - repetition {}: {cells}", index + 1);
            }
        }
        if !observations
            .iter()
            .any(|observation| observation.starts_with(&driver))
        {
            out.push_str(
                "    - driver: — unavailable: this record holds no driver observation for this step\n",
            );
        }
    }

    /// Writes every server series each repetition's kept snapshots hold:
    /// each timing series, scan series, counter that moved, gauge, and
    /// histogram with an observation in the window, rebuilt from the
    /// record's snapshots alone. A window whose snapshots do not difference
    /// says why.
    fn render_server(out: &mut String, step: &StepRecord) {
        use wyrd_benchmark::{HistogramUnit, IntervalValue};
        for (index, repetition) in step.repetitions.iter().enumerate() {
            for evidence in repetition
                .timings
                .iter()
                .filter(|evidence| evidence.role == Role::Server)
            {
                let _ = writeln!(
                    out,
                    "  - repetition {} server {} window (nearest-rank bucket bounds):",
                    index + 1,
                    evidence.window
                );
                let interval = match evidence.after.since(&evidence.before) {
                    Ok(interval) => interval,
                    Err(reason) => {
                        let _ = writeln!(out, "    - — unreadable: {reason}");
                        continue;
                    }
                };
                for series in &interval.series {
                    let _ = writeln!(
                        out,
                        "    - timing {} {} case {} phase {}: {}",
                        series.boundary,
                        series.outcome,
                        series.case,
                        series.phase,
                        distribution_cells(&series.elapsed, 1e3, "µs")
                    );
                }
                for scan in &interval.scans {
                    let _ = writeln!(
                        out,
                        "    - scan {} case {} phase {}: rows {}; bytes {}",
                        scan.outcome,
                        scan.case,
                        scan.phase,
                        distribution_cells(&scan.rows, 1.0, "rows"),
                        distribution_cells(&scan.bytes, 1.0, "B")
                    );
                }
                for metric in &interval.metrics {
                    let name = if metric.labels.is_empty() {
                        metric.name.clone()
                    } else {
                        let labels: Vec<String> = metric
                            .labels
                            .iter()
                            .map(|(label, value)| format!("{label}={value}"))
                            .collect();
                        format!("{}{{{}}}", metric.name, labels.join(","))
                    };
                    let _ = match &metric.value {
                        IntervalValue::Counter(0) => Ok(()),
                        IntervalValue::Counter(increase) => {
                            writeln!(out, "    - counter {name}: +{increase}")
                        }
                        IntervalValue::Gauge { value, peak } => writeln!(
                            out,
                            "    - gauge {name}: {} peak {}",
                            number(*value),
                            number(*peak)
                        ),
                        IntervalValue::Histogram { distribution, .. }
                            if distribution.is_empty() =>
                        {
                            Ok(())
                        }
                        IntervalValue::Histogram { unit, distribution } => {
                            let (per, label) = match unit {
                                HistogramUnit::Nanoseconds => (1e3, "µs"),
                                HistogramUnit::Billionths => (unit.per_unit() as f64, ""),
                            };
                            writeln!(
                                out,
                                "    - histogram {name}: {}",
                                distribution_cells(distribution, per, label)
                            )
                        }
                    };
                }
            }
        }
    }

    /// The first eight common cells of a step row.
    fn step_cells(&self, step: &StepRecord) -> String {
        let median = |id: &str| step.summary.get(id).map(|summary| summary.median.clone());
        let params = if step.params.is_empty() {
            "— none".to_owned()
        } else {
            step.params
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let rate = median("rate").map_or_else(
            || "— no rate for this step".to_owned(),
            |rate| {
                let normalized = self.criteria.metrics.iter().filter(|metric| {
                    matches!(
                        metric.derive,
                        Some(
                            Derive::PerVcpu { .. } | Derive::PerCpu { .. } | Derive::CpuPer { .. }
                        )
                    )
                });
                std::iter::once(rate.cell(&self.unit("rate")))
                    .chain(normalized.filter_map(|metric| {
                        median(&metric.id)
                            .filter(Reading::is_present)
                            .map(|reading| reading.cell(&metric.unit))
                    }))
                    .collect::<Vec<_>>()
                    .join("; ")
            },
        );
        let triple = |prefix: &str| {
            let cells: Vec<String> = ["p50_ms", "p95_ms", "p99_ms"]
                .iter()
                .map(|id| {
                    median(&format!("{prefix}{id}")).map_or_else(
                        || "—".to_owned(),
                        |reading| reading.cell("").trim().to_owned(),
                    )
                })
                .collect();
            (!cells.iter().all(|cell| cell == "—")).then(|| cells.join(" / "))
        };
        let latency = match (triple(""), triple("client_")) {
            (None, None) => "— no request latency for this step".to_owned(),
            (server, client) => format!(
                "{} (client {})",
                server.as_deref().unwrap_or("—"),
                client.as_deref().unwrap_or("—")
            ),
        };
        let errors = median("errors").map_or_else(
            || "— not counted".to_owned(),
            |errors| {
                let total: f64 = step
                    .repetitions
                    .iter()
                    .filter_map(|rep| rep.reading("errors").exact_value())
                    .sum();
                format!(
                    "{} total (median {})",
                    number(total),
                    errors.cell("").trim()
                )
            },
        );
        let mid = middle(&step.repetitions);
        let cpu = mid.map_or_else(
            || "— not measured".to_owned(),
            |rep| match rep.server_cpu_seconds {
                Some(cpu) if rep.seconds > 0.0 => format!(
                    "{} s / {} cores used / {} vCPU",
                    number(cpu),
                    number(cpu / rep.seconds),
                    rep.provisioned_vcpus
                        .map_or_else(|| "—".to_owned(), |vcpus| vcpus.to_string())
                ),
                _ => "— unavailable".to_owned(),
            },
        );
        let memory = step
            .repetitions
            .iter()
            .filter_map(|rep| {
                rep.peak_memory_bytes
                    .map(|bytes| (bytes, rep.memory_interval_ms))
            })
            .max()
            .map_or_else(
                || "— not sampled".to_owned(),
                |(bytes, interval)| {
                    format!(
                        "{} MiB (sampled {interval} ms)",
                        number(bytes as f64 / f64::from(1 << 20))
                    )
                },
            );
        let driver = mid.map_or_else(
            || "— not measured".to_owned(),
            |rep| {
                rep.driver_cpu_seconds.map_or_else(
                    || "— unavailable".to_owned(),
                    |cpu| format!("{} s", number(cpu)),
                )
            },
        );
        format!(
            "{} | {params} | {rate} | {latency} | {errors} | {cpu} | {memory} | {driver}",
            step.id
        )
    }

    /// The declared unit of metric `id`.
    fn unit(&self, id: &str) -> String {
        self.criteria
            .metrics
            .iter()
            .find(|metric| metric.id == id)
            .map_or_else(String::new, |metric| metric.unit.clone())
    }
}

/// The repetition with the median server CPU, the one a report row shows.
fn middle(repetitions: &[Repetition]) -> Option<&Repetition> {
    let mut sorted: Vec<&Repetition> = repetitions.iter().collect();
    sorted.sort_by(|left, right| {
        left.server_cpu_seconds
            .unwrap_or(f64::NAN)
            .total_cmp(&right.server_cpu_seconds.unwrap_or(f64::NAN))
    });
    sorted.get(sorted.len() / 2).copied()
}

/// The common report header, exactly.
pub const REPORT_HEADER: &str = "| step | params | rate | server p50/p95/p99 ms (client) | errors | server CPU | peak mem | driver CPU | criterion | observed | outcome |";

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, VecDeque};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;
    use serde_json::{Value, json};
    use tokio_util::sync::CancellationToken;

    use super::{
        Arrivals, Criterion, Declaration, Derive, Direction, Engine, ExperimentError, HostRecord,
        Invocation, Level, Metric, Mode, Op, Options, Outcome, Phase, Profiling, REPORT_HEADER,
        Reading, Record, Repetition, Role, Source, Status, Step, Validity, Verdict, Workload, run,
    };
    use crate::capacity::{Population, UNLABELLED};
    use crate::profile::fixture::{Sidecar, profiler};
    use crate::profile::{Archived, ProfileMode, ProfileNote, Symbolized};
    use crate::release_server::Histogram;
    use crate::release_server::HostLoad;

    /// A quiet host sample.
    fn quiet() -> HostLoad {
        HostLoad {
            logical_cpus: 32,
            seconds: 10.0,
            other_cores: 1.0,
            busiest: Vec::new(),
        }
    }

    /// A host sample with `cores` of outside use on 32 CPUs.
    fn loaded(cores: f64) -> HostLoad {
        HostLoad {
            other_cores: cores,
            ..quiet()
        }
    }

    /// The representative experiment's declaration: two measured steps, a
    /// strict p99 SLO, an every-repetition error criterion, and a derived
    /// efficiency.
    fn declaration() -> Declaration {
        let steps = vec!["op-1".to_owned(), "op-64".to_owned()];
        Declaration {
            engine: Engine::Verification,
            experiment: "direct-verify".to_owned(),
            level: Level::One,
            question: "How much server CPU and latency does direct verification add?".to_owned(),
            timing: Level::One.timing(),
            validity: Validity::default(),
            variables: BTreeMap::from([("clients".to_owned(), json!([1, 64]))]),
            controls: BTreeMap::from([
                ("fixture".to_owned(), json!("synthetic")),
                ("limits".to_owned(), json!("server defaults")),
                ("workload".to_owned(), json!("closed loop")),
            ]),
            metrics: vec![
                Metric::raw("p99_ms", "ms", "server arrival to staging"),
                Metric::raw("client_p99_ms", "ms", "client send to acknowledgement"),
                Metric::raw("errors", "requests", "unexpected errors"),
                Metric::raw("rate", "ops/s", "successful operations per second"),
                Metric::raw("ops", "ops", "successful operations"),
                Metric::derived(
                    "ops_per_cpu_s",
                    "ops/cpu-s",
                    "successful operations per server CPU-second",
                    Direction::Higher,
                    Derive::PerCpu {
                        work: "ops".to_owned(),
                        scale: 1.0,
                    },
                ),
            ],
            criteria: vec![
                Criterion::limit("overhead-slo", "p99_ms", &steps[1..], Op::Lt, 0.5).percentile(99),
                Criterion::every("no-errors", "errors", &steps, Op::Le, 0.0),
            ],
            steps: vec![
                Step::measured("op-1", json!({ "clients": 1 })),
                Step::measured("op-64", json!({ "clients": 64 })),
            ],
            selection: None,
            source: Source::from_diff(
                "0123456789abcdef0123456789abcdef01234567".to_owned(),
                b"",
                false,
            ),
            build: json!({ "profile": "release" }),
            command: Invocation::new(&["l1".to_owned(), "verification".to_owned()], &[]),
            host: HostRecord {
                os: "linux".to_owned(),
                os_version: "test".to_owned(),
                arch: "x86_64".to_owned(),
                cpu_model: "test cpu".to_owned(),
                logical_cpus: 32,
                memory_bytes: 1 << 34,
                storage: "file:// temporary directory".to_owned(),
                fingerprint: "test cpu, 32 CPU / 16 GiB, linux".to_owned(),
            },
            refusals: Vec::new(),
            mode: Mode::Full,
        }
    }

    /// A good repetition: 0.3 ms p99 over 2,000 samples, no errors, 1,000
    /// operations on 2 CPU-seconds over 10 s.
    fn good() -> Repetition {
        let mut repetition = Repetition {
            seconds: 10.0,
            processes: vec!["wyrd-server".to_owned()],
            server_cpu_seconds: Some(2.0),
            peak_memory_bytes: Some(512 << 20),
            memory_interval_ms: 10,
            driver_cpu_seconds: Some(1.0),
            ..Repetition::default()
        };
        repetition.set("p99_ms", Reading::exact(0.3).with_samples(2_000));
        repetition.set("errors", Reading::count(0));
        repetition.set("rate", Reading::exact(100.0));
        repetition.set("ops", Reading::exact(1_000.0));
        repetition
    }

    /// What the scripted workload saw.
    #[derive(Debug, Default)]
    struct Seen {
        /// Phases, in order, with their window.
        calls: Vec<(String, Phase, Duration)>,
        /// Whether prepare and finish ran.
        prepared: bool,
        /// Whether finish ran.
        finished: bool,
        /// Whether `watch` existed when finish began draining.
        saved_at_drain: Option<bool>,
    }

    /// A workload replaying scripted repetitions; each call sleeps its
    /// window (or `stall`) on the paused clock.
    struct Scripted {
        /// Repetitions to return, in call order; warmups take one too.
        script: VecDeque<Result<Repetition, String>>,
        /// What the runner asked for.
        seen: Arc<Mutex<Seen>>,
        /// Extra paused-clock time each measurement takes.
        stall: Duration,
        /// Cancelled after this many calls, when set.
        interrupt: Option<(usize, CancellationToken)>,
        /// Real time each step's warmup blocks, so an external profiler
        /// started just before it is running when the paused clock races on.
        settle: Option<Duration>,
        /// Why preparation fails, when set.
        unprepared: Option<String>,
        /// A file whose presence finish notes as the drain begins.
        watch: Option<PathBuf>,
    }

    impl Scripted {
        /// Every call returns `repetition`.
        fn always(repetition: &Repetition) -> Self {
            Self {
                script: std::iter::repeat_n(Ok(repetition.clone()), 64).collect(),
                seen: Arc::default(),
                stall: Duration::ZERO,
                interrupt: None,
                settle: None,
                unprepared: None,
                watch: None,
            }
        }
    }

    #[async_trait]
    impl Workload for Scripted {
        async fn prepare(&mut self) -> Result<(), ExperimentError> {
            if let Some(missing) = self.unprepared.take() {
                return Err(ExperimentError::Workload(missing));
            }
            self.seen.lock().expect("seen").prepared = true;
            Ok(())
        }

        fn processes(&self) -> Vec<u32> {
            vec![std::process::id()]
        }

        async fn measure(
            &mut self,
            step: &Step,
            phase: Phase,
            window: Duration,
        ) -> Result<Repetition, ExperimentError> {
            let count = {
                let mut seen = self.seen.lock().expect("seen");
                seen.calls.push((step.id.clone(), phase, window));
                seen.calls.len()
            };
            if let Some((after, token)) = &self.interrupt
                && count >= *after
            {
                token.cancel();
            }
            if phase == Phase::Warmup
                && let Some(settle) = self.settle
            {
                std::thread::sleep(settle);
            }
            tokio::time::sleep(window + self.stall).await;
            self.script
                .pop_front()
                .unwrap_or_else(|| Err("script exhausted".to_owned()))
                .map_err(ExperimentError::Workload)
        }

        async fn finish(&mut self) -> Result<(), ExperimentError> {
            let mut seen = self.seen.lock().expect("seen");
            seen.finished = true;
            seen.saved_at_drain = self.watch.as_ref().map(|watch| watch.exists());
            Ok(())
        }
    }

    /// Options writing artifacts under `directory`, without profiling.
    fn options(directory: &Path) -> Options {
        Options {
            artifacts: directory.to_path_buf(),
            profile: None,
            interrupt: CancellationToken::new(),
            setup_spent: Duration::ZERO,
        }
    }

    /// Runs `declaration` over `workload` with `samples` as the host's
    /// before and after samples.
    async fn execute(
        declaration: Declaration,
        workload: &mut Scripted,
        samples: [Option<HostLoad>; 2],
        options: &Options,
    ) -> Record {
        let mut samples = VecDeque::from(samples);
        run(
            declaration.seal(),
            workload,
            move |_| std::future::ready(samples.pop_front().flatten()),
            options,
        )
        .await
    }

    /// The codes of a record's run-level validity reasons.
    fn validity(record: &Record) -> Vec<&str> {
        record
            .validity
            .reasons
            .iter()
            .map(|reason| reason.code.as_str())
            .collect()
    }

    /// The codes of a record's reasons.
    fn codes(record: &Record) -> Vec<&str> {
        record
            .reasons
            .iter()
            .map(|reason| reason.code.as_str())
            .collect()
    }

    /// Comparing two VALID runs of one experiment gives each step both
    /// measured a row per metric: the medians and ranges, the change, and
    /// whether it is outside both runs' spread, in which direction, and,
    /// for a metric that declares one, whether that is better. Runs of
    /// different experiments, or an INVALID run, are refused.
    ///
    /// # Panics
    ///
    /// Panics when a row is missing or wrong, or a refused pair compares.
    #[tokio::test(start_paused = true)]
    async fn compare_marks_each_change_against_both_spreads() {
        let directory = tempfile::tempdir().expect("artifacts");
        let options = options(directory.path());
        let before = execute(
            declaration(),
            &mut Scripted::always(&good()),
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        let mut faster = good();
        faster.set("ops", Reading::exact(2_000.0));
        let after = execute(
            declaration(),
            &mut Scripted::always(&faster),
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;

        let compared = before.compare(&after).expect("compares");

        assert!(compared.contains("## op-64\n"), "{compared}");
        assert!(
            compared.contains(
                "| ops (ops) | 1000 [1000–1000] | 2000 [2000–2000] | +100.0% | outside: higher |"
            ),
            "{compared}"
        );
        assert!(
            compared.contains("| ops_per_cpu_s (ops/cpu-s) | 500 [500–500] | 1000 [1000–1000] | +100.0% | outside: higher, better |"),
            "{compared}"
        );
        assert!(
            compared.contains(
                "| rate (ops/s) | 100 [100–100] | 100 [100–100] | +0.0% | no measured change |"
            ),
            "{compared}"
        );
        let mut other = declaration();
        other.experiment = "other".to_owned();
        let other = execute(
            other,
            &mut Scripted::always(&good()),
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert!(before.compare(&other).is_err());
        let invalid = execute(
            declaration(),
            &mut Scripted::always(&good()),
            [None, None],
            &options,
        )
        .await;
        assert_eq!(invalid.verdict, Verdict::Invalid);
        assert!(before.compare(&invalid).is_err());
    }

    /// A declared `compare` control puts each group of steps side by side,
    /// one column per language and one row per metric, under the group's
    /// note, ahead of the common table; a record without it has no summary.
    ///
    /// # Panics
    ///
    /// Panics when the summary is missing, misplaced, or wrong.
    #[tokio::test(start_paused = true)]
    async fn report_compares_declared_steps_side_by_side() {
        let directory = tempfile::tempdir().expect("artifacts");
        let options = options(directory.path());
        let mut declared = declaration();
        declared.steps = vec![
            Step::measured("op-1", json!({ "clients": 1, "language": "rust" })),
            Step::measured("op-python-1", json!({ "clients": 1, "language": "python" })),
            Step::measured("op-64", json!({ "clients": 64, "language": "rust" })),
            Step::measured(
                "op-python-64",
                json!({ "clients": 64, "language": "python" }),
            ),
        ];
        let plain = execute(
            declared.clone(),
            &mut Scripted::always(&good()),
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert!(!plain.render().contains("## By"));

        declared.controls.insert(
            "compare".to_owned(),
            json!({
                "by": "language",
                "metrics": ["p99_ms", "rate"],
                "notes": { "clients=64": "Overloaded: do not compare languages here." },
            }),
        );
        let compared = execute(
            declared,
            &mut Scripted::always(&good()),
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        let report = compared.render();
        let expected = "## By language

Medians of the repetitions. A histogram reading shows its bucket's upper bound; the table below has both bounds.

### clients=1

| metric | rust | python |
|---|---|---|
| p99_ms (ms) | 0.3 | 0.3 |
| rate (ops/s) | 100 | 100 |

### clients=64

Overloaded: do not compare languages here.

| metric | rust | python |
|---|---|---|
| p99_ms (ms) | 0.3 | 0.3 |
| rate (ops/s) | 100 | 100 |

";
        assert!(report.contains(expected), "{report}");
        assert!(report.find(expected) < report.find(REPORT_HEADER));
    }

    /// V1: a complete declaration admits the workload and is recorded
    /// unchanged; a missing control or question, a dirty source without
    /// override, and an undeclared criterion metric each refuse the run as
    /// INVALID with no workload call; a dirty override stores the digest and
    /// still runs every other check; criteria the record keeps are the
    /// sealed ones whatever the workload measured; a failed preparation is
    /// INVALID with its named missing evidence and admits no step.
    ///
    /// # Panics
    ///
    /// Panics when a refusal admits work, a valid declaration is refused, or
    /// the recorded criteria or controls differ from the declaration.
    #[tokio::test(start_paused = true)]
    async fn declaration_precedes_workload() {
        let directory = tempfile::tempdir().expect("artifacts");
        let options = options(directory.path());
        let mut workload = Scripted::always(&good());
        let valid = execute(
            declaration(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(valid.verdict, Verdict::Valid, "{:?}", valid.reasons);
        assert_eq!(
            valid.criteria.criteria,
            declaration().criteria,
            "criteria are the sealed ones"
        );
        assert_eq!(valid.controls.values, declaration().controls);
        assert_eq!(valid.commit, declaration().source.commit);
        assert!(workload.seen.lock().expect("seen").prepared);

        let refused = |mutate: fn(&mut Declaration), code: &'static str| {
            let mut declared = declaration();
            mutate(&mut declared);
            (declared, code)
        };
        for (declared, code) in [
            refused(
                |d| {
                    d.controls.remove("fixture");
                },
                "declaration.missing",
            ),
            refused(|d| d.question.clear(), "declaration.missing"),
            refused(
                |d| d.criteria[0].metric = "p999_ms".to_owned(),
                "declaration.missing",
            ),
            refused(|d| d.criteria.clear(), "declaration.missing"),
            refused(
                |d| {
                    d.source =
                        Source::from_diff(d.source.commit.clone(), b"diff --git a b\n", false);
                },
                "source.dirty",
            ),
        ] {
            let mut workload = Scripted::always(&good());
            let record = execute(
                declared,
                &mut workload,
                [Some(quiet()), Some(quiet())],
                &options,
            )
            .await;
            let seen = workload.seen.lock().expect("seen");
            assert_eq!(record.verdict, Verdict::Invalid);
            assert!(
                codes(&record).contains(&code),
                "{code} in {:?}",
                record.reasons
            );
            assert!(
                !seen.prepared && seen.calls.is_empty(),
                "a refusal admits no workload"
            );
            assert!(
                record
                    .steps
                    .iter()
                    .all(|step| step.status == Status::Unmeasured)
            );
            assert!(
                record.validity.host_before.is_none(),
                "no host sample before a refusal"
            );
        }

        let mut dirty = declaration();
        dirty.source = Source::from_diff(dirty.source.commit.clone(), b"diff --git a b\n", true);
        let mut workload = Scripted::always(&good());
        let record = execute(
            dirty,
            &mut workload,
            [Some(quiet()), Some(loaded(8.0))],
            &options,
        )
        .await;
        assert!(record.dirty && record.dirty_override);
        assert_eq!(
            record.diff_sha256.as_deref(),
            Some("10d3857e66825b3404560ea728e8ac3882e3e5e0aef09df8f73f0ad9bc1e4059"),
            "SHA-256 of the tracked diff bytes"
        );
        assert_eq!(
            record.verdict,
            Verdict::Invalid,
            "the override bypasses no other check"
        );
        assert_eq!(validity(&record), ["host.busy"]);

        let mut workload = Scripted::always(&good());
        workload.unprepared = Some("bifrost: the span fixture loaded short".to_owned());
        let record = execute(
            declaration(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        let seen = workload.seen.lock().expect("seen");
        assert_eq!(record.verdict, Verdict::Invalid);
        assert_eq!(validity(&record), ["workload.failed"]);
        assert!(record.reasons[0].message.contains("loaded short"));
        assert!(
            !seen.prepared && seen.calls.is_empty() && seen.finished,
            "a failed preparation admits no step, and still stops what it started"
        );
    }

    /// V2: each validity case independently invalidates with a typed
    /// reason, exact thresholds stay eligible, a busy preflight admits no
    /// workload, and an invalid step beside a valid miss keeps both.
    ///
    /// # Panics
    ///
    /// Panics when an invalid case passes, a boundary case is refused, or
    /// precedence collapses a reason.
    #[tokio::test(start_paused = true)]
    async fn invalid_observations_preserve_reasons() {
        let directory = tempfile::tempdir().expect("artifacts");
        let options = options(directory.path());
        let case = |mutate: fn(&mut Repetition)| {
            let mut repetition = good();
            mutate(&mut repetition);
            repetition
        };

        let mut workload = Scripted::always(&good());
        let busy = execute(
            declaration(),
            &mut workload,
            [Some(loaded(3.21)), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(validity(&busy), ["host.busy"]);
        assert!(
            !workload.seen.lock().expect("seen").prepared,
            "a busy preflight admits nothing"
        );
        let mut workload = Scripted::always(&good());
        let edge = execute(
            declaration(),
            &mut workload,
            [Some(loaded(3.2)), Some(loaded(3.2))],
            &options,
        )
        .await;
        assert_eq!(
            edge.verdict,
            Verdict::Valid,
            "exactly 10% is eligible: {:?}",
            edge.reasons
        );
        let mut workload = Scripted::always(&good());
        let after = execute(
            declaration(),
            &mut workload,
            [Some(quiet()), Some(loaded(4.0))],
            &options,
        )
        .await;
        assert_eq!(validity(&after), ["host.busy"]);
        let mut workload = Scripted::always(&good());
        let unreadable = execute(
            declaration(),
            &mut workload,
            [None, Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(validity(&unreadable), ["host.unreadable"]);

        for (repetition, code, verdict) in [
            (
                case(|r| r.driver_cpu_seconds = Some(64.1)),
                "driver.saturated",
                Verdict::Invalid,
            ),
            (
                case(|r| r.driver_cpu_seconds = Some(64.0)),
                "",
                Verdict::Valid,
            ),
            (
                case(|r| r.driver_cpu_seconds = None),
                "reading.missing",
                Verdict::Invalid,
            ),
            (
                case(|r| r.server_cpu_seconds = None),
                "reading.missing",
                Verdict::Invalid,
            ),
            (
                case(|r| {
                    r.arrivals = Some(Arrivals {
                        scheduled: 100,
                        sent: 99,
                        completed: 99,
                        missed: 1,
                        lateness_p99_us: Some(10.0),
                    });
                }),
                "arrivals.missed",
                Verdict::Invalid,
            ),
            (
                case(|r| {
                    r.arrivals = Some(Arrivals {
                        scheduled: 100,
                        sent: 100,
                        completed: 100,
                        missed: 0,
                        lateness_p99_us: Some(5_000.1),
                    });
                }),
                "arrivals.late",
                Verdict::Invalid,
            ),
            (
                case(|r| {
                    r.arrivals = Some(Arrivals {
                        scheduled: 100,
                        sent: 100,
                        completed: 100,
                        missed: 0,
                        lateness_p99_us: Some(5_000.0),
                    });
                }),
                "",
                Verdict::Valid,
            ),
            (
                case(|r| r.set("p99_ms", Reading::exact(0.3).with_samples(999))),
                "criterion.invalid",
                Verdict::Invalid,
            ),
            (
                case(|r| r.set("p99_ms", Reading::exact(0.3).with_samples(1_000))),
                "",
                Verdict::Valid,
            ),
            (
                case(|r| r.set("p99_ms", Reading::missing("histogram counter reset"))),
                "criterion.invalid",
                Verdict::Invalid,
            ),
            (
                case(|r| r.invalid.push("wyrd-server exited".to_owned())),
                "workload.invalid",
                Verdict::Invalid,
            ),
        ] {
            let mut workload = Scripted::always(&repetition);
            let record = execute(
                declaration(),
                &mut workload,
                [Some(quiet()), Some(quiet())],
                &options,
            )
            .await;
            assert_eq!(record.verdict, verdict, "{code}: {:?}", record.reasons);
            if !code.is_empty() {
                assert!(
                    codes(&record).contains(&code),
                    "{code} in {:?}",
                    record.reasons
                );
            }
        }
        let missed = Scripted::always(&case(|r| {
            r.arrivals = Some(Arrivals {
                scheduled: 100,
                sent: 97,
                completed: 96,
                missed: 3,
                lateness_p99_us: Some(10.0),
            });
        }));
        let mut missed = missed;
        let record = execute(
            declaration(),
            &mut missed,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        let arrivals = record.steps[0].repetitions[0]
            .arrivals
            .expect("arrivals kept");
        assert_eq!(
            (
                arrivals.scheduled,
                arrivals.sent,
                arrivals.completed,
                arrivals.missed
            ),
            (100, 97, 96, 3)
        );

        let mut slow_and_lost = Scripted::always(&good());
        slow_and_lost.script = std::iter::repeat_n(Ok(case(|r| r.server_cpu_seconds = None)), 4)
            .chain(std::iter::repeat_n(
                Ok(case(|r| {
                    r.set("p99_ms", Reading::exact(0.7).with_samples(2_000));
                })),
                4,
            ))
            .collect();
        let record = execute(
            declaration(),
            &mut slow_and_lost,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(record.verdict, Verdict::Invalid);
        assert_eq!(record.exit_code(), 2);
        assert_eq!(record.steps[0].verdict, Verdict::Invalid);
        assert_eq!(
            (record.steps[1].verdict, record.steps[1].criteria[0].outcome),
            (Verdict::Valid, Some(Outcome::Missed)),
            "the target miss stays visible and invalidates nothing"
        );
        assert!(
            !codes(&record).is_empty()
                && codes(&record).iter().all(|code| *code == "reading.missing"),
            "{:?}",
            codes(&record)
        );
    }

    /// V2: L1 measures 3 × 5 s within 5 minutes; a step declaring
    /// [`WINDOW_SECONDS`] measures that window in a full run and records
    /// it, a smoke keeps its short windows, and a short window's
    /// sample-floor miss stays INVALID rather than being lengthened.
    ///
    /// # Panics
    ///
    /// Panics when a declared window is ignored, leaks into a smoke, or a
    /// floor miss passes.
    #[tokio::test(start_paused = true)]
    async fn l1_windows_preserve_floors_and_budget() {
        let directory = tempfile::tempdir().expect("artifacts");
        let options = options(directory.path());
        let timing = Level::One.timing();
        assert_eq!(
            (
                timing.warmup_seconds,
                timing.repetition_seconds,
                timing.repetitions,
                timing.budget_seconds
            ),
            (5, 5, 3, 300)
        );
        let mut windowed = declaration();
        windowed.steps[0] =
            Step::measured("op-1", json!({ "clients": 1, (super::WINDOW_SECONDS): 10 }));
        let lengths = |calls: &[(String, Phase, Duration)], step: &str| -> Vec<u64> {
            calls
                .iter()
                .filter(|call| call.0 == step)
                .map(|call| call.2.as_secs())
                .collect()
        };
        let mut workload = Scripted::always(&good());
        let record = execute(
            windowed.clone(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        let calls = workload.seen.lock().expect("seen").calls.clone();
        assert_eq!(lengths(&calls, "op-1"), [5, 10, 10, 10]);
        assert_eq!(lengths(&calls, "op-64"), [5, 5, 5, 5]);
        assert_eq!(record.steps[0].timing.repetition_seconds, 10);
        assert_eq!(record.steps[1].timing.repetition_seconds, 5);
        assert_eq!(record.steps[0].params[super::WINDOW_SECONDS], json!(10));

        let mut smoke = windowed;
        smoke.mode = Mode::Smoke;
        smoke.timing = Level::One.smoke_timing();
        let mut workload = Scripted::always(&good());
        execute(
            smoke,
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        let calls = workload.seen.lock().expect("seen").calls.clone();
        assert_eq!(
            lengths(&calls, "op-1"),
            [1, 3],
            "a smoke keeps its short windows"
        );

        let mut short = good();
        short.set("p99_ms", Reading::exact(0.1).with_samples(999));
        let mut workload = Scripted::always(&short);
        let record = execute(
            declaration(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(
            record.steps[1].verdict,
            Verdict::Invalid,
            "one sample under the floor"
        );
    }

    /// V3: warmup is called but never recorded, three repetitions keep
    /// their own differing percentiles, the L1 budget stops an overrun with
    /// partial evidence and unmeasured steps, an interruption stops
    /// admission, a workload failure marks the repetition missing, and L2
    /// enforces both its hard budget and its minimum duration.
    ///
    /// # Panics
    ///
    /// Panics when warmup leaks into evidence, repetitions merge, or a
    /// budget, minimum, interruption, or failure is not recorded honestly.
    #[tokio::test(start_paused = true)]
    async fn repetitions_exclude_warmup_and_bound_execution() {
        let directory = tempfile::tempdir().expect("artifacts");
        let options = options(directory.path());
        let with = |p99: f64| {
            let mut repetition = good();
            repetition.set("p99_ms", Reading::exact(p99).with_samples(2_000));
            repetition
        };
        let mut workload = Scripted::always(&good());
        workload.script = [
            with(9.0),
            with(0.2),
            with(0.4),
            with(0.3),
            with(9.0),
            with(0.1),
            with(0.45),
            with(0.2),
        ]
        .into_iter()
        .map(Ok)
        .collect();
        let record = execute(
            declaration(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        let (calls, finished) = {
            let seen = workload.seen.lock().expect("seen");
            (seen.calls.clone(), seen.finished)
        };
        let phases: Vec<Phase> = calls.iter().map(|call| call.1).collect();
        assert_eq!(
            phases[..4],
            [
                Phase::Warmup,
                Phase::Repetition(0),
                Phase::Repetition(1),
                Phase::Repetition(2)
            ]
        );
        assert_eq!(calls[0].2, Duration::from_secs(5));
        assert_eq!(calls[1].2, Duration::from_secs(5));
        assert!(finished);
        let step = &record.steps[1];
        let p99: Vec<Option<f64>> = step
            .repetitions
            .iter()
            .map(|rep| rep.reading("p99_ms").value)
            .collect();
        assert_eq!(
            p99,
            [Some(0.1), Some(0.45), Some(0.2)],
            "warmup's 9 ms is discarded"
        );
        assert_eq!(
            step.summary["p99_ms"].median.value,
            Some(0.2),
            "median repetition, not merged samples"
        );
        assert_eq!(
            step.summary["p99_ms"].spread.map(|s| (s * 100.0).round()),
            Some(35.0)
        );
        assert_eq!(record.verdict, Verdict::Valid);
        assert!(
            (record.elapsed_seconds - 2.0 * 20.0).abs() < 1e-6,
            "two steps of 5 s + 3 × 5 s"
        );

        let mut overrun = Scripted::always(&good());
        overrun.stall = Duration::from_secs(100);
        let record = record_timed(declaration(), &mut overrun, &options).await;
        assert!(
            codes(&record).contains(&"budget.exceeded"),
            "{:?}",
            record.reasons
        );
        assert_eq!(record.verdict, Verdict::Invalid);
        assert_eq!(record.steps[0].status, Status::Measured);
        assert!(
            record.steps[0].repetitions.len() < 3,
            "partial evidence kept"
        );
        assert!(codes(&record).contains(&"repetition.missing"));
        assert_eq!(record.steps[1].status, Status::Unmeasured);
        assert!(
            record.steps[1].repetitions.is_empty(),
            "no fabricated metrics"
        );
        assert!(record.elapsed_seconds >= 300.0 && record.elapsed_seconds < 300.0 + 120.0);
        assert!(record.finished_at >= record.started_at);
        assert!(
            overrun.seen.lock().expect("seen").finished,
            "drain still runs after an overrun"
        );

        let token = CancellationToken::new();
        let mut interrupted = Scripted::always(&good());
        interrupted.interrupt = Some((2, token.clone()));
        let options_interrupted = Options {
            interrupt: token,
            ..options.clone()
        };
        let record = record_timed(declaration(), &mut interrupted, &options_interrupted).await;
        assert!(codes(&record).contains(&"interrupted"));
        assert_eq!(record.steps[1].status, Status::Unmeasured);
        assert!(
            interrupted.seen.lock().expect("seen").calls.len() == 2,
            "admission stopped"
        );

        let mut failing = Scripted::always(&good());
        failing.script = [Ok(good()), Ok(good()), Err("server exited".to_owned())]
            .into_iter()
            .chain(std::iter::repeat_n(Ok(good()), 8))
            .collect();
        let record = record_timed(declaration(), &mut failing, &options).await;
        assert!(
            codes(&record).contains(&"workload.failed")
                && codes(&record).contains(&"repetition.missing")
        );
        assert_eq!(record.steps[0].repetitions.len(), 1);
        assert_eq!(
            record.steps[1].verdict,
            Verdict::Valid,
            "later steps still measure"
        );

        let mut two = declaration();
        two.level = Level::Two;
        two.timing = Level::Two.timing();
        let mut short_run = two.clone();
        short_run.selection = Some(vec!["op-64".to_owned()]);
        short_run.timing.repetition_seconds = 300;
        let mut short = Scripted::always(&good());
        let record = record_timed(short_run, &mut short, &options).await;
        assert!(
            codes(&record).contains(&"duration.short"),
            "one step takes 17 minutes: {:?}",
            record.reasons
        );
        assert_eq!(record.steps[0].status, Status::Omitted);
        let mut long = Scripted::always(&good());
        long.stall = Duration::from_secs(200);
        let record = record_timed(two, &mut long, &options).await;
        assert!(codes(&record).contains(&"budget.exceeded"));
        assert!(!codes(&record).contains(&"duration.short"));
    }

    /// Runs `declaration` with quiet host samples.
    async fn record_timed(
        declaration: Declaration,
        workload: &mut Scripted,
        options: &Options,
    ) -> Record {
        execute(
            declaration,
            workload,
            [Some(quiet()), Some(quiet())],
            options,
        )
        .await
    }

    /// V4: derived metrics recompute W/C, C/W, C/T, (W/T)/P, rates, and
    /// memory intensity from the summed server process window; unavailable
    /// or reset CPU, unavailable P, and zero denominators are missing with
    /// reasons, never zero; host core metadata is never a denominator. Server
    /// timings recorded into the benchmark histograms and differenced
    /// between two snapshots give nearest-rank residual buckets, so p99s of
    /// 499,000, 500,000, and 501,000 ns judge met, unsettled, missed under a
    /// strict 0.5 ms limit whatever the client saw; a zero residual counts;
    /// extra, out-of-range, unexcluded, or negative observations are
    /// invalid, never clamped.
    ///
    /// # Panics
    ///
    /// Panics when arithmetic, units, scope, collection, or judgment is wrong.
    #[tokio::test]
    async fn metrics_preserve_process_scope_and_units() {
        let metrics = [
            Metric::derived("cores", "cores", "C/T", Direction::Lower, Derive::Cores),
            Metric::derived(
                "rows_per_cpu_s",
                "rows/cpu-s",
                "W/C",
                Direction::Higher,
                Derive::PerCpu {
                    work: "rows".to_owned(),
                    scale: 1.0,
                },
            ),
            Metric::derived(
                "cpu_us_per_row",
                "µs/row",
                "C/W",
                Direction::Lower,
                Derive::CpuPer {
                    work: "rows".to_owned(),
                    scale: 1e6,
                },
            ),
            Metric::derived(
                "mb_per_cpu_s",
                "MB/cpu-s",
                "B/C",
                Direction::Higher,
                Derive::PerCpu {
                    work: "bytes".to_owned(),
                    scale: 1e-6,
                },
            ),
            Metric::derived(
                "mib_per_cpu_s",
                "MiB/cpu-s",
                "B/C",
                Direction::Higher,
                Derive::PerCpu {
                    work: "bytes".to_owned(),
                    scale: 1.0 / f64::from(1 << 20),
                },
            ),
            Metric::derived(
                "rows_per_s_per_vcpu",
                "rows/s/vCPU",
                "(W/T)/P",
                Direction::Higher,
                Derive::PerVcpu {
                    work: "rows".to_owned(),
                    scale: 1.0,
                },
            ),
            Metric::derived(
                "rate",
                "rows/s",
                "W/T",
                Direction::Higher,
                Derive::Rate {
                    work: "rows".to_owned(),
                    scale: 1.0,
                },
            ),
            Metric::derived(
                "memory_per_rate",
                "bytes/(rows/s)",
                "M/(W/T)",
                Direction::Lower,
                Derive::MemoryPer {
                    work: "rows".to_owned(),
                },
            ),
        ];
        let mut repetition = Repetition {
            seconds: 10.0,
            processes: vec!["wyrd-server".to_owned(), "forge".to_owned()],
            server_cpu_seconds: Some(1.5 + 2.5),
            peak_memory_bytes: Some(1_000_000),
            ..Repetition::default()
        };
        repetition.set("rows", Reading::exact(2_000_000.0));
        repetition.set("bytes", Reading::exact(8_000_000.0));
        repetition.derive(&metrics);
        let value = |id: &str| repetition.reading(id).value.expect(id);
        assert!(
            (value("cores") - 0.4).abs() < 1e-12,
            "C/T over both processes, not host CPUs"
        );
        assert!((value("rows_per_cpu_s") - 500_000.0).abs() < 1e-6);
        assert!((value("cpu_us_per_row") - 2.0).abs() < 1e-9);
        assert!((value("mb_per_cpu_s") - 2.0).abs() < 1e-12, "MB is 10^6");
        assert!(
            (value("mib_per_cpu_s") - 8_000_000.0 / 4.0 / 1_048_576.0).abs() < 1e-12,
            "MiB is 2^20"
        );
        assert!((value("rate") - 200_000.0).abs() < 1e-9);
        assert!((value("memory_per_rate") - 5.0).abs() < 1e-12);

        let mut lost = Repetition {
            seconds: 10.0,
            server_cpu_seconds: None,
            ..Repetition::default()
        };
        lost.set("rows", Reading::exact(10.0));
        lost.derive(&metrics);
        assert_eq!(lost.reading("rows_per_cpu_s").value, None);
        assert!(
            lost.reading("rows_per_cpu_s")
                .reason
                .expect("reason")
                .contains("unavailable")
        );
        assert!(
            lost.reading("memory_per_rate")
                .reason
                .expect("reason")
                .contains("not sampled")
        );
        let mut idle = Repetition {
            seconds: 10.0,
            server_cpu_seconds: Some(0.0),
            ..Repetition::default()
        };
        idle.set("rows", Reading::exact(0.0));
        idle.derive(&metrics);
        assert!(
            idle.reading("rows_per_cpu_s")
                .reason
                .expect("reason")
                .contains("zero")
        );
        assert!(
            idle.reading("cpu_us_per_row")
                .reason
                .expect("reason")
                .contains("zero")
        );

        let per_vcpu = |vcpus: Option<u64>| {
            let mut repetition = Repetition {
                seconds: 10.0,
                server_cpu_seconds: Some(4.0),
                provisioned_vcpus: vcpus,
                ..Repetition::default()
            };
            repetition.set("rows", Reading::exact(2_000_000.0));
            repetition.derive(&metrics);
            (
                repetition.reading("rows_per_s_per_vcpu"),
                repetition.reading("rows_per_cpu_s").value,
            )
        };
        let ((four, at_four), (eight, at_eight)) = (per_vcpu(Some(4)), per_vcpu(Some(8)));
        assert_eq!((four.value, eight.value), (Some(50_000.0), Some(25_000.0)));
        assert_eq!(at_four, at_eight, "W/C ignores provisioned vCPUs");
        assert!(
            per_vcpu(None)
                .0
                .reason
                .expect("reason")
                .contains("provisioned vCPUs")
        );

        wyrd_benchmark::install().expect("the benchmark recorder installs");
        let verifier = 2_000_000;
        let window = |requests: &[(u64, Option<u64>)], expected: u64| {
            let before = wyrd_benchmark::snapshot();
            let request = tracing::Span::none();
            for (elapsed, excluded) in requests {
                wyrd_benchmark::record_timing(
                    &request,
                    "verification.direct",
                    "completed",
                    std::time::Duration::from_nanos(*elapsed),
                    excluded.map(std::time::Duration::from_nanos),
                );
            }
            let interval = wyrd_benchmark::snapshot()
                .since(&before)
                .expect("one process");
            let mut population = Population::from_interval(interval);
            population.expect(&[crate::capacity::Expected {
                boundary: "verification.direct",
                outcome: "completed",
                case: UNLABELLED,
                phase: UNLABELLED,
                count: expected,
            }]);
            let mut repetition = Repetition::default();
            population.validate(&mut repetition, "measured");
            population.percentiles(
                &mut repetition,
                "verification.direct",
                "completed",
                true,
                &[("overhead_p99_ms", 0.99)],
            );
            repetition
        };
        let samples = |p99: u64| -> Vec<(u64, Option<u64>)> {
            (0..100_u64)
                .map(|index| {
                    let residual = match index {
                        98 => p99,
                        99 => 10_000_000,
                        _ => 1_000,
                    };
                    (residual + verifier, Some(verifier))
                })
                .collect()
        };
        for (p99, verdict) in [
            (499_000, Some(true)),
            (500_000, None),
            (501_000, Some(false)),
        ] {
            let judged = window(&samples(p99), 100);
            assert!(judged.invalid.is_empty(), "{:?}", judged.invalid);
            let reading = judged.reading("overhead_p99_ms");
            assert_eq!(reading.samples, Some(100));
            let (lo, open, hi) = reading.interval().expect("interval");
            assert!(
                lo < p99 as f64 / 1e6 && hi >= Some(p99 as f64 / 1e6),
                "the 99th of 100 residuals lies in its bucket's bounds"
            );
            assert_eq!(
                Op::Lt.judge(lo, open, hi, 0.5),
                verdict,
                "{p99} ns passes or fails only when its whole bucket does"
            );
            let mut delayed = window(&samples(p99), 100);
            delayed.set("client_p99_ms", Reading::exact(p99 as f64 / 1e6 + 50.0));
            assert_eq!(
                delayed.reading("overhead_p99_ms"),
                reading,
                "client delay never moves server latency"
            );
        }
        let exact = window(&[(verifier + 1_000, Some(verifier)), (5, Some(5))], 2);
        let reading = exact.reading("overhead_p99_ms");
        assert_eq!(
            (reading.value, reading.above),
            (Some(0.001), None),
            "a value in a unit-wide bucket is exact"
        );
        let mut zero = Repetition::default();
        Population::from_interval({
            let before = wyrd_benchmark::snapshot();
            wyrd_benchmark::record_timing(
                &tracing::Span::none(),
                "verification.direct",
                "completed",
                std::time::Duration::ZERO,
                Some(std::time::Duration::ZERO),
            );
            wyrd_benchmark::snapshot()
                .since(&before)
                .expect("one process")
        })
        .percentiles(
            &mut zero,
            "verification.direct",
            "completed",
            true,
            &[("overhead_p99_ms", 0.99)],
        );
        assert_eq!(
            zero.reading("overhead_p99_ms").value,
            Some(0.0),
            "a zero residual is counted, never missing"
        );
        let mut extra = samples(1_000);
        extra.push((1_000 + verifier, Some(verifier)));
        let extra = window(&extra, 100);
        assert!(
            extra.invalid[0].contains("101 `verification.direct`"),
            "{:?}",
            extra.invalid
        );
        let mut overflowed = samples(1_000);
        overflowed[0].0 = wyrd_benchmark::HIGHEST_NS * 2;
        let overflowed = window(&overflowed, 100);
        assert!(
            overflowed
                .invalid
                .iter()
                .any(|reason| reason.contains("outside the trackable range")),
            "{:?}",
            overflowed.invalid
        );
        let mut negative = samples(1_000);
        negative[0].1 = Some(negative[0].0 + 1);
        let negative = window(&negative, 100);
        assert!(
            negative
                .invalid
                .iter()
                .any(|reason| reason.contains("excluded waits exceeding elapsed time")),
            "never clamped: {:?}",
            negative.invalid
        );
        let mut unexcluded = samples(1_000);
        unexcluded[0].1 = None;
        let unexcluded = window(&unexcluded, 100);
        assert!(
            unexcluded
                .invalid
                .iter()
                .any(|reason| reason.contains("inconsistent excluded components")),
            "never substituted: {:?}",
            unexcluded.invalid
        );

        let layout = |buckets: Vec<(f64, f64)>| Histogram {
            buckets,
            sum: 0.05,
            count: 100.0,
        };
        assert_eq!(
            layout(vec![(0.001, 100.0), (f64::INFINITY, 100.0)]).mean(),
            layout(vec![
                (0.00025, 10.0),
                (0.0005, 60.0),
                (f64::INFINITY, 100.0)
            ])
            .mean(),
            "a diagnostic mean is the same under any bucket layout"
        );

        assert_eq!(
            Op::Lt.judge(0.25, true, Some(0.5), 0.5),
            None,
            "≤ 0.5 cannot prove < 0.5"
        );
        assert_eq!(Op::Le.judge(0.25, true, Some(0.5), 0.5), Some(true));
        assert_eq!(
            Op::Lt.judge(0.1, true, None, 0.5),
            None,
            "an unbounded censored reading never passes"
        );
        assert_eq!(
            Op::Lt.judge(5.0, false, Some(5.0), 5.0),
            Some(false),
            "equality fails a strict SLO"
        );
        assert_eq!(Op::Le.judge(100.0, false, Some(100.0), 100.0), Some(true));
        assert_eq!(
            Op::Ge.judge(200_000.0, false, Some(200_000.0), 200_000.0),
            Some(true)
        );
        assert_eq!(Op::Gt.judge(1.0, false, Some(1.0), 1.0), Some(false));
        let median = Reading::median(&[
            Reading::above(0.25),
            Reading::exact(0.5),
            Reading::exact(0.1),
        ]);
        assert_eq!((median.above, median.value), (Some(0.25), Some(0.5)));
    }

    /// V5: strict target equality and noise each miss a target and leave the
    /// run VALID, while correctness failing in only one repetition
    /// invalidates it.
    ///
    /// # Panics
    ///
    /// Panics when a comparison or criterion judges against the rule.
    #[tokio::test(start_paused = true)]
    async fn criteria_distinguish_target_misses_and_invalidity() {
        let directory = tempfile::tempdir().expect("artifacts");
        let options = options(directory.path());
        let with = |p99: f64, errors: u64| {
            let mut repetition = good();
            repetition.set("p99_ms", Reading::exact(p99).with_samples(2_000));
            repetition.set("errors", Reading::count(errors));
            repetition
        };
        let scripted = |reps: [Repetition; 4]| {
            let mut workload = Scripted::always(&good());
            workload.script = std::iter::repeat_n(Ok(good()), 4)
                .chain(reps.into_iter().map(Ok))
                .collect();
            workload
        };
        let mut equal = scripted([good(), with(0.5, 0), with(0.5, 0), with(0.5, 0)]);
        let record = execute(
            declaration(),
            &mut equal,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(
            (
                record.verdict,
                record.exit_code(),
                record.steps[1].criteria[0].outcome
            ),
            (Verdict::Valid, 0, Some(Outcome::Missed)),
            "p99 equal to 500 µs misses < 500 µs and stays VALID"
        );
        let mut noisy = scripted([good(), with(0.1, 0), with(0.6, 0), with(0.9, 0)]);
        let record = execute(
            declaration(),
            &mut noisy,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(
            (
                record.verdict,
                record.exit_code(),
                record.steps[1].criteria[0].outcome
            ),
            (Verdict::Valid, 0, Some(Outcome::Missed)),
            "spread never excuses a target miss, which never invalidates"
        );
        let censored = || {
            let mut repetition = good();
            repetition.set("p99_ms", Reading::above(0.25).with_samples(2_000));
            repetition
        };
        let mut straddles = scripted([good(), censored(), censored(), censored()]);
        let record = execute(
            declaration(),
            &mut straddles,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(
            (
                record.verdict,
                record.exit_code(),
                record.steps[1].criteria[0].outcome
            ),
            (Verdict::Valid, 0, Some(Outcome::Unsettled)),
            "a censored p99 straddling 500 µs leaves the target unsettled and the run VALID"
        );
        let mut once = scripted([good(), with(0.1, 0), with(0.1, 1), with(0.1, 0)]);
        let record = execute(
            declaration(),
            &mut once,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(
            (
                record.verdict,
                record.exit_code(),
                record.steps[1].criteria[1].outcome
            ),
            (Verdict::Invalid, 2, Some(Outcome::Invalid)),
            "an error in one repetition invalidates the run"
        );
        assert_eq!(record.steps[1].criteria[1].observed.value, Some(1.0));
    }

    /// V7: records round-trip, publish append-only with collision suffixes
    /// and no temporary residue, fail honestly when the directory cannot be
    /// written, refuse unsupported versions and malformed input, keep nulls
    /// for missing values, store the dirty digest, and never carry a secret
    /// configuration value. Only a clean, unprofiled full run of the whole
    /// plan enters `benchmarks/history/`, VALID or INVALID; a dirty,
    /// profiled, paced, or `--steps` run stays under `target/benchmarks/`.
    ///
    /// # Panics
    ///
    /// Panics when history is overwritten, a failure succeeds, a reader is
    /// permissive, a sentinel leaks, or an ineligible run enters history.
    #[tokio::test(start_paused = true)]
    async fn history_is_append_only_private_and_versioned() {
        let root = tempfile::tempdir().expect("root");
        let options = options(&root.path().join("target"));
        let sentinel = "wyrd_sk_SENTINEL_SECRET_VALUE";
        let mut declared = declaration();
        declared.source = Source::from_diff(
            declared.source.commit.clone(),
            b"diff --git a/x b/x\n",
            true,
        );
        declared.command = Invocation::new(
            &["l1".to_owned()],
            &[
                ("WYRD_API_KEY", sentinel),
                ("WYRD_STORAGE_URL", "s3://private-bucket"),
                ("WYRD_LOG", "info"),
            ],
        );
        let mut workload = Scripted::always(&good());
        let complete = execute(
            declared.clone(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        let mut failing = Scripted::always(&good());
        failing.stall = Duration::from_secs(150);
        let partial = execute(
            declared,
            &mut failing,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;

        let first = complete.publish(root.path()).expect("publish");
        let second = complete.publish(root.path()).expect("publish again");
        let third = partial.publish(root.path()).expect("publish partial");
        assert_ne!(first, second, "a collision never overwrites");
        assert!(second.to_string_lossy().ends_with("-2.json"));
        assert!(
            first
                .to_string_lossy()
                .contains(&format!("-{}", &complete.commit[..9]))
        );
        assert!(
            first.starts_with(
                root.path()
                    .join("target/benchmarks/verification/direct-verify")
            ),
            "a dirty run never enters history"
        );
        let entries: Vec<PathBuf> = std::fs::read_dir(first.parent().expect("parent"))
            .expect("listing")
            .map(|entry| entry.expect("entry").path())
            .collect();
        assert!(
            entries
                .iter()
                .all(|path| path.extension().is_some_and(|ext| ext == "json")),
            "no temporary residue: {entries:?}"
        );
        assert_eq!(entries.len(), 3);
        let clean = execute(
            declaration(),
            &mut Scripted::always(&good()),
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        let mut invalid = clean.clone();
        invalid.verdict = Verdict::Invalid;
        let mut profiled = clean.clone();
        profiled.command = Invocation::new(&["l1".to_owned(), "--profile".to_owned()], &[]);
        let mut paced = clean.clone();
        paced.command = Invocation::new(
            &["l1".to_owned(), "--pace".to_owned(), "100".to_owned()],
            &[],
        );
        let mut selected = clean.clone();
        selected.controls.selection = Some(vec!["op-64".to_owned()]);
        for (record, base) in [
            (&clean, "benchmarks/history"),
            (&invalid, "benchmarks/history"),
            (&profiled, "target/benchmarks"),
            (&paced, "target/benchmarks"),
            (&selected, "target/benchmarks"),
        ] {
            let path = record.publish(root.path()).expect("publish");
            assert!(
                path.starts_with(root.path().join(base).join("verification/direct-verify")),
                "{}",
                path.display()
            );
        }

        for (path, original) in [(&first, &complete), (&third, &partial)] {
            let text = std::fs::read_to_string(path).expect("record text");
            assert!(
                !text.contains(sentinel) && !text.contains("private-bucket"),
                "no secret or endpoint"
            );
            assert!(
                !text.contains("NaN") && !text.contains("Infinity"),
                "no non-finite numbers"
            );
            let read = Record::read(path).expect("read back");
            assert_eq!(&read, original, "round trip");
            assert_eq!(read.diff_sha256.as_ref().map(String::len), Some(64));
        }
        assert_eq!(partial.verdict, Verdict::Invalid);
        let unmeasured = serde_json::to_value(&partial.steps[1]).expect("step json");
        assert_eq!(unmeasured["status"], "unmeasured");
        let missing = serde_json::to_value(Reading::missing("lost")).expect("reading json");
        assert_eq!(missing["value"], serde_json::Value::Null);

        let text = std::fs::read_to_string(&first).expect("record text");
        let future = text.replacen("\"schema_version\": 3", "\"schema_version\": 4", 1);
        assert!(matches!(
            Record::parse(&future),
            Err(ExperimentError::Unsupported(4))
        ));
        assert!(matches!(
            Record::parse("{\"verdict\": \"PASS\"}"),
            Err(ExperimentError::Malformed(_))
        ));
        assert!(matches!(
            Record::parse(&text[..text.len() / 2]),
            Err(ExperimentError::Malformed(_))
        ));
        let mut extended: serde_json::Value = serde_json::from_str(&text).expect("record json");
        extended["verdict_override"] = json!("PASS");
        assert!(
            matches!(
                Record::parse(&extended.to_string()),
                Err(ExperimentError::Malformed(_))
            ),
            "an unknown top-level field refuses interpretation"
        );

        let blocked = root.path().join("blocked");
        std::fs::write(&blocked, "a file where the directory must be").expect("blocker");
        assert!(matches!(
            complete.publish(&blocked),
            Err(ExperimentError::Persist { .. })
        ));
    }

    /// A smoke runs the same lifecycle over the selected steps with one
    /// short warmup and one repetition each: it judges no target, so a
    /// short percentile, a missed limit and recorded errors are inapplicable
    /// and survive readback with their observed values, while an empty
    /// population and zero CPU stay INVALID; its record round-trips
    /// `mode: smoke` under `target/benchmarks/smoke/` and a failed write is
    /// still a persistence error; a saturated driver does not invalidate it.
    /// A record without a mode reads as full.
    ///
    /// # Panics
    ///
    /// Panics when a smoke skips or repeats work, waives an integrity check,
    /// judges a target or driver saturation, lands in history, or a record without a mode does
    /// not read as full.
    #[tokio::test(start_paused = true)]
    async fn smoke_runs_the_selected_workflow_and_stays_out_of_history() {
        let root = tempfile::tempdir().expect("root");
        let options = options(&root.path().join("target"));
        let mut smoke = declaration();
        smoke.mode = Mode::Smoke;
        smoke.timing = Level::One.smoke_timing();
        smoke.selection = Some(vec!["op-64".to_owned()]);
        let mut short = good();
        short.set("p99_ms", Reading::exact(0.3).with_samples(40));
        let mut workload = Scripted::always(&short);
        let record = execute(
            smoke.clone(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(
            workload.seen.lock().expect("seen").calls,
            [
                ("op-64".to_owned(), Phase::Warmup, Duration::from_secs(1)),
                (
                    "op-64".to_owned(),
                    Phase::Repetition(0),
                    Duration::from_secs(3)
                ),
            ],
            "only the selected step, one warmup and one repetition"
        );
        assert_eq!(record.verdict, Verdict::Valid, "{:?}", record.reasons);
        let step = record
            .steps
            .iter()
            .find(|step| step.id == "op-64")
            .expect("selected step");
        let slo = step
            .criteria
            .iter()
            .find(|judgment| judgment.criterion == "overhead-slo")
            .expect("slo judgment");
        assert!(!slo.applicable && slo.outcome.is_none());
        assert!(
            slo.reason
                .as_deref()
                .is_some_and(|reason| reason.contains("40 samples")),
            "{slo:?}"
        );
        let mut full = smoke.clone();
        full.mode = Mode::Full;
        full.timing = Level::One.timing();
        let floored = execute(
            full,
            &mut Scripted::always(&short),
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(
            floored.verdict,
            Verdict::Invalid,
            "a full run keeps its floor"
        );
        let mut slow = good();
        slow.set("p99_ms", Reading::exact(9.0).with_samples(2_000));
        slow.set("errors", Reading::count(10));
        let missed = execute(
            smoke.clone(),
            &mut Scripted::always(&slow),
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(
            missed.verdict,
            Verdict::Valid,
            "a smoke judges no target: {:?}",
            missed.reasons
        );
        let path = missed.publish(root.path()).expect("publish");
        let read = Record::read(&path).expect("read back");
        assert_eq!(read.render(), missed.render());
        let step = read
            .steps
            .iter()
            .find(|step| step.id == "op-64")
            .expect("selected step");
        assert_eq!(step.repetitions[0].reading("errors").value, Some(10.0));
        for (criterion, observed) in [("overhead-slo", 9.0), ("no-errors", 10.0)] {
            let judgment = step
                .criteria
                .iter()
                .find(|judgment| judgment.criterion == criterion)
                .expect("judgment");
            assert!(!judgment.applicable && judgment.outcome.is_none());
            assert_eq!(judgment.observed.value, Some(observed));
            assert_eq!(
                judgment.reason.as_deref(),
                Some("not judged in a smoke: a smoke only shows the workflow runs")
            );
        }

        let mut busy = good();
        busy.driver_cpu_seconds = Some(1_000.0);
        let saturated = execute(
            smoke.clone(),
            &mut Scripted::always(&busy),
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(
            saturated.verdict,
            Verdict::Valid,
            "a smoke does not judge driver saturation: {:?}",
            saturated.reasons
        );

        let mut empty = good();
        empty.set("p99_ms", Reading::missing("no timing observation"));
        let mut idle = good();
        idle.server_cpu_seconds = Some(0.0);
        for broken in [empty, idle] {
            let record = execute(
                smoke.clone(),
                &mut Scripted::always(&broken),
                [Some(quiet()), Some(quiet())],
                &options,
            )
            .await;
            assert_eq!(record.verdict, Verdict::Invalid, "{:?}", record.reasons);
        }

        let path = record.publish(root.path()).expect("publish");
        assert!(
            path.starts_with(
                root.path()
                    .join("target/benchmarks/smoke/verification/direct-verify")
            )
        );
        assert!(
            !root.path().join("benchmarks").exists(),
            "nothing in history"
        );
        let read = Record::read(&path).expect("read back");
        assert_eq!(read.mode, Mode::Smoke);
        assert!(read.render().contains("SMOKE"));
        let blocked = root.path().join("blocked");
        std::fs::write(&blocked, "a file where the directory must be").expect("blocker");
        assert!(matches!(
            record.publish(&blocked),
            Err(ExperimentError::Persist { .. })
        ));

        let admitted = execute(
            declaration(),
            &mut Scripted::always(&good()),
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(admitted.verdict, Verdict::Valid, "{:?}", admitted.reasons);

        let mut legacy: serde_json::Value = serde_json::to_value(&admitted).expect("json");
        legacy.as_object_mut().expect("object").remove("mode");
        let legacy = Record::parse(&legacy.to_string()).expect("legacy record");
        assert_eq!(legacy.mode, Mode::Full);
    }

    /// A cohort's latency is read from the window start until it drained,
    /// so a completion after the CPU window's end is in its distribution
    /// and drain accounting while its work counts only what completed
    /// before; warmup observations before the first snapshot are in
    /// neither, and a replaced server never yields an interval.
    ///
    /// # Panics
    ///
    /// Panics when a window keeps the wrong observations or a replaced
    /// process differences.
    #[test]
    fn histogram_cohorts_preserve_measurement_and_drain_windows() {
        use crate::capacity::{Population, UNLABELLED};
        use wyrd_benchmark as timing;

        timing::install().expect("the benchmark recorder installs");
        let record = |micros: u64| {
            wyrd_benchmark::record_timing(
                &tracing::Span::none(),
                "bifrost.query",
                "success",
                Duration::from_micros(micros),
                None,
            );
        };
        record(5_000_000);
        record(5_000_000);
        let before = timing::snapshot();
        for _ in 0..3 {
            record(100);
        }
        let closed = timing::snapshot();
        record(900_000);
        let drained = timing::snapshot();
        let window = |after: &timing::BenchmarkSnapshot| {
            Population::from_interval(after.since(&before).expect("one process"))
        };
        let (measured, mut cohort) = (window(&closed), window(&drained));
        let owed = crate::capacity::Expected {
            boundary: "bifrost.query",
            outcome: "success",
            case: UNLABELLED,
            phase: UNLABELLED,
            count: 4,
        };
        cohort.expect(&[owed]);
        assert!(cohort.invalid.is_empty(), "{:?}", cohort.invalid);
        assert_eq!(measured.count("bifrost.query"), 3, "work");
        let latency = cohort.elapsed("bifrost.query", "success").expect("latency");
        let p99 = Reading::quantile(&latency, 0.99, crate::capacity::NANOS_PER_MS);
        assert!(
            p99.above.is_some_and(|above| above < 900.0)
                && p99.value.is_some_and(|value| value >= 900.0),
            "the drained completion is the cohort's p99, never the warmup: {p99:?}"
        );
        let mut short = window(&closed);
        short.expect(&[owed]);
        assert_eq!(short.invalid.len(), 1, "the drain completion is owed");
        let mut replaced = drained.clone();
        replaced.process = "another".to_owned();
        let error = replaced.since(&before).expect_err("a replaced process");
        assert!(error.contains("replaced"), "{error}");
    }

    /// A repetition's server timing snapshots are kept under the target
    /// artifacts, outside the record, and rebuild its every judged
    /// percentile, maximum, rank, and bound and its reconciliation after the
    /// server and the in-memory population are gone; the record keeps the
    /// ranks and reconciliation. Evidence that cannot be kept makes every
    /// repetition INVALID.
    ///
    /// # Panics
    ///
    /// Panics when the kept evidence is missing, rebuilds another reading,
    /// or its loss is not invalid.
    #[tokio::test(start_paused = true)]
    async fn kept_timing_evidence_rebuilds_judged_readings() {
        use crate::capacity::{Expected, Population, UNLABELLED};
        use wyrd_benchmark as timing;

        timing::install().expect("the benchmark recorder installs");
        let before = timing::snapshot();
        for (micros, outcome) in [(100, "success"), (300, "success"), (900, "error")] {
            wyrd_benchmark::record_timing(
                &tracing::Span::none(),
                "bifrost.query",
                outcome,
                Duration::from_micros(micros),
                None,
            );
        }
        let after = timing::snapshot();
        let judged = [("p50_ms", 0.5), ("p95_ms", 0.95), ("p99_ms", 0.99)];
        let reduce = |before: &timing::BenchmarkSnapshot, after: timing::BenchmarkSnapshot| {
            let mut population = Population::between(before, after).expect("one process");
            population.expect(&[Expected {
                boundary: "bifrost.query",
                outcome: "success",
                case: UNLABELLED,
                phase: UNLABELLED,
                count: 2,
            }]);
            let mut repetition = good();
            population.validate(&mut repetition, "measured");
            population.percentiles(&mut repetition, "bifrost.query", "success", false, &judged);
            repetition
        };
        let measured = reduce(&before, after);
        assert!(measured.invalid.is_empty(), "{:?}", measured.invalid);
        let root = tempfile::tempdir().expect("root");
        let artifacts = root.path().join("target/run");
        let mut workload = Scripted::always(&measured);
        let record = execute(
            declaration(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options(&artifacts),
        )
        .await;
        let kept = std::fs::read(artifacts.join("timings/op-1/repetition-1.json")).expect("kept");
        let kept: Vec<super::TimingEvidence> = serde_json::from_slice(&kept).expect("evidence");
        let [window] = kept.as_slice() else {
            panic!("one window: {kept:?}");
        };
        assert_eq!(window.window, "measured");
        let rebuilt = reduce(&window.before, window.after.clone());
        let stored = &record.steps[0].repetitions[0];
        for id in ["p50_ms", "p95_ms", "p99_ms", "max_ms"] {
            let reading = stored.reading(id);
            assert!(
                reading.is_present() && reading.rank.is_some(),
                "{id}: {reading:?}"
            );
            assert_eq!(
                rebuilt.reading(id).bare(),
                reading,
                "{id}: a schema-3 record keeps no window histogram"
            );
        }
        assert_eq!(window.reconciliation, stored.reconciliation);
        assert_eq!(stored.reconciliation[0].recorded, 2);
        let read = Record::parse(&serde_json::to_string(&record).expect("record json"))
            .expect("a v2 record");
        assert_eq!(
            read.steps[0].repetitions[0].reconciliation,
            stored.reconciliation
        );

        let blocked = root.path().join("blocked");
        std::fs::write(&blocked, "a file").expect("blocker");
        let mut workload = Scripted::always(&measured);
        let lost = execute(
            declaration(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options(&blocked.join("run")),
        )
        .await;
        for repetition in lost.steps.iter().flat_map(|step| &step.repetitions) {
            assert!(
                repetition.invalid.iter().any(
                    |reason| reason.starts_with("the server timing evidence could not be kept")
                ),
                "{:?}",
                repetition.invalid
            );
        }
        assert_ne!(lost.exit_code(), 0);
    }

    /// A written record is version 3 and names its histogram method and
    /// bucket bounds in the regenerated report; a version-1 or version-2
    /// record, or any other version, is refused.
    ///
    /// # Panics
    ///
    /// Panics when a version, method, or bound is lost, or an older record
    /// reads.
    #[tokio::test(start_paused = true)]
    async fn histogram_records_preserve_method_and_refuse_older_versions() {
        let root = tempfile::tempdir().expect("root");
        let options = options(&root.path().join("target"));
        let mut declared = declaration();
        declared
            .controls
            .insert("timing".to_owned(), crate::capacity::timing_declaration());
        let mut bounded = good();
        bounded.set(
            "p99_ms",
            Reading::bounded(399_000.0, 401_000.0, 1e6).with_samples(2_000),
        );
        let mut workload = Scripted::always(&bounded);
        let record = execute(
            declared,
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        let path = record.publish(root.path()).expect("publish");
        let text = std::fs::read_to_string(&path).expect("record text");
        let read = Record::parse(&text).expect("a v3 record");
        assert_eq!(read.schema_version, super::SCHEMA_VERSION);
        let report = read.render();
        assert_eq!(report, record.render(), "the report regenerates");
        assert!(
            report.contains("timing (schema 3): server benchmark histograms"),
            "{report}"
        );
        assert!(report.contains("(0.399, 0.401] ms | met"), "{report}");

        for version in [1, 2, 4] {
            let other = text.replacen(
                "\"schema_version\": 3",
                &format!("\"schema_version\": {version}"),
                1,
            );
            assert!(matches!(
                Record::parse(&other),
                Err(ExperimentError::Unsupported(found)) if found == version
            ));
        }
    }

    /// Every repetition's diagnostic readings survive publication: a
    /// fresh and a re-read report agree once the raw artifacts are gone,
    /// each observation shows its declared boundary and every repetition's
    /// count, bucket bounds, and mean, a slow first repetition stays
    /// visible beside the median, an inapplicable observation says why, a
    /// repetition that read none says it was not read, and a record that
    /// declares none says diagnostics are unavailable.
    ///
    /// # Panics
    ///
    /// Panics when a diagnostic line is missing, collapsed, or differs
    /// between the fresh and re-read report.
    #[tokio::test(start_paused = true)]
    async fn reports_keep_repetition_diagnostics_after_artifacts_are_removed() {
        let root = tempfile::tempdir().expect("root");
        let artifacts = root.path().join("target/run");
        std::fs::create_dir_all(&artifacts).expect("artifacts");
        std::fs::write(artifacts.join("timings.json"), "{}").expect("raw timings");
        let mut declared = declaration();
        declared.metrics.extend(Metric::diagnostic(
            "phase.admit",
            "server: the admission transaction, every call",
        ));
        declared.metrics.extend(Metric::diagnostic(
            "phase.relay",
            "server: stream relay; streams only",
        ));
        let diagnosed = |nanos: &[u64]| {
            let mut repetition = good();
            repetition.diagnose(
                "phase.admit",
                wyrd_benchmark::Distribution::of_durations(nanos),
                true,
            );
            repetition.inapplicable("phase.relay", "a buffered call relays no stream");
            repetition
        };
        let unread = good();
        let mut workload = Scripted::always(&good());
        workload.script = [
            diagnosed(&[1]),
            diagnosed(&[9_000_000; 4]),
            diagnosed(&[20_000; 3]),
            diagnosed(&[30_000, 50_000]),
            diagnosed(&[1]),
            diagnosed(&[40_000; 2]),
            unread,
            diagnosed(&[60_000]),
        ]
        .map(Ok)
        .into_iter()
        .collect();
        let record = execute(
            declared,
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options(&artifacts),
        )
        .await;
        let console = record.render();
        let path = record.publish(root.path()).expect("publish");
        std::fs::remove_dir_all(&artifacts).expect("raw artifacts removed");
        let read = Record::read(&path).expect("read");
        assert_eq!(
            console,
            read.render(),
            "diagnostics regenerate from history alone"
        );
        let lines: Vec<&str> = console.lines().collect();
        let admit: Vec<&str> = lines
            .iter()
            .copied()
            .filter(|line| line.starts_with("      - repetition"))
            .collect();
        assert_eq!(
            admit.len(),
            12,
            "both observations of both steps, every repetition: {admit:?}"
        );
        assert!(
            admit[0].starts_with("      - repetition 1: n 4, p50 ("),
            "{admit:?}"
        );
        assert!(admit[0].contains("mean ("), "{admit:?}");
        assert!(admit[1].starts_with("      - repetition 2: n 3, p50 ("));
        assert!(admit[2].starts_with("      - repetition 3: n 2, p50 ("));
        assert!(admit[3].contains("— inapplicable: a buffered call relays no stream"));
        assert!(
            admit[7].contains("`diagnostic.phase.admit.count` was not read"),
            "{admit:?}"
        );
        assert!(
            console.contains("  - diagnostics (wall time; nearest-rank bucket bounds; rows overlap and never add or subtract):"),
            "{console}"
        );
        assert!(
            !console.contains("  - diagnostic.phase.admit.p50_us: median"),
            "diagnostics are shown per repetition, not only as a median"
        );

        let mut workload = Scripted::always(&good());
        let historical = execute(
            declaration(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options(&root.path().join("target/historical")),
        )
        .await;
        assert_eq!(
            historical
                .render()
                .matches("  - diagnostics: — unavailable: this record declares and read none")
                .count(),
            2
        );

        let mut declared = declaration();
        for (observation, boundary) in [
            (
                "phase.admit",
                "server: the admission transaction, every call",
            ),
            (
                "driver.client.http",
                "driver: one logical request, every call",
            ),
            (
                "driver.client.http.attempt",
                "driver: one attempt, retries included",
            ),
        ] {
            declared
                .metrics
                .extend(Metric::diagnostic(observation, boundary));
        }
        let observed = |server: &[u64], logical: &[u64], attempts: &[u64]| {
            let mut repetition = good();
            for (observation, nanos) in [
                ("phase.admit", server),
                ("driver.client.http", logical),
                ("driver.client.http.attempt", attempts),
            ] {
                repetition.diagnose(
                    observation,
                    wyrd_benchmark::Distribution::of_durations(nanos),
                    true,
                );
            }
            repetition
        };
        let mut workload = Scripted::always(&good());
        workload.script = [
            observed(&[1], &[1], &[1]),
            observed(&[10_000; 2], &[100_000; 3], &[40_000; 4]),
            observed(&[20_000; 2], &[200_000; 5], &[90_000; 5]),
            observed(&[30_000; 2], &[300_000; 7], &[70_000; 9]),
            observed(&[1], &[1], &[1]),
            observed(&[10_000], &[100_000], &[100_000]),
            observed(&[10_000], &[100_000], &[100_000]),
            observed(&[10_000], &[100_000], &[100_000]),
        ]
        .map(Ok)
        .into_iter()
        .collect();
        let artifacts = root.path().join("target/roles");
        let mut record = execute(
            declared,
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options(&artifacts),
        )
        .await;
        record.artifacts.profile = ProfileNote {
            requested: true,
            complete: false,
            captures: vec![
                "profiles/op-1/cpu-server-7.json.gz".to_owned(),
                "profiles/op-1/cpu-driver-8.json.gz".to_owned(),
                "profiles/heap-driver-op-1.pb.gz".to_owned(),
                "profiles/verify-typescript-c1/cpu-python-9.json.gz".to_owned(),
            ],
            notes: vec!["the server heap profile was not served".to_owned()],
            mode: Some(ProfileMode::Both),
            roles: vec![Role::Server, Role::Driver, Role::Python],
            symbols: vec![Symbolized {
                capture: "profiles/op-1/cpu-server-7.json.gz".to_owned(),
                sidecar: "profiles/op-1/cpu-server-7.json.syms.json".to_owned(),
                executable: Some(Archived {
                    build_id: "0f0f".to_owned(),
                    path: "symbols/0f0f/wyrd-server".to_owned(),
                }),
                samples: 200,
                unresolved: 3,
            }],
        };
        let console = record.render();
        let path = record.publish(root.path()).expect("publish");
        std::fs::remove_dir_all(&artifacts).ok();
        assert_eq!(
            console,
            Record::read(&path).expect("read").render(),
            "roles and profile scope regenerate from history alone"
        );
        let block = |row: &str| -> Vec<String> {
            let lines: Vec<&str> = console.lines().collect();
            let start = lines
                .iter()
                .position(|line| line.starts_with(row))
                .unwrap_or_else(|| panic!("{row} is reported: {console}"));
            lines[start + 1..start + 4]
                .iter()
                .map(|line| line.trim().to_owned())
                .collect()
        };
        let counts = |row: &str| -> Vec<String> {
            block(row)
                .iter()
                .map(|line| line.split(',').next().unwrap_or_default().to_owned())
                .collect()
        };
        assert_eq!(
            counts("    - driver client.http: driver: one logical request"),
            [
                "- repetition 1: n 3",
                "- repetition 2: n 5",
                "- repetition 3: n 7"
            ],
            "every repetition's logical calls"
        );
        assert_eq!(
            counts("    - driver client.http.attempt: driver: one attempt"),
            [
                "- repetition 1: n 4",
                "- repetition 2: n 5",
                "- repetition 3: n 9"
            ],
            "attempts are counted apart from logical calls"
        );
        assert_eq!(
            counts("    - server phase.admit: server: the admission transaction"),
            [
                "- repetition 1: n 2",
                "- repetition 2: n 2",
                "- repetition 3: n 2"
            ],
            "the server's population is its own"
        );
        assert!(!console.contains("driver: — unavailable"), "{console}");
        for line in [
            "profile both of server and driver and python: incomplete (4 captures: server 1, driver 2, python 1); the server heap profile was not served",
            "- python capture: profiles/verify-typescript-c1/cpu-python-9.json.gz",
            "- driver capture: profiles/op-1/cpu-driver-8.json.gz",
            "- server capture: profiles/op-1/cpu-server-7.json.gz",
            "- symbols: profiles/op-1/cpu-server-7.json.syms.json: 3 of 200 executable samples unresolved; build 0f0f kept at symbols/0f0f/wyrd-server",
            "neither is aligned to a repetition window",
        ] {
            assert!(console.contains(line), "{line}: {console}");
        }

        let mut old = historical;
        old.artifacts.profile = serde_json::from_value(json!({
            "requested": true,
            "complete": true,
            "captures": ["profiles/profile-7.json.gz"],
            "notes": [],
        }))
        .expect("an old profile note");
        let report = old.render();
        assert!(
            report.contains("profile of server (recorded before profile modes and roles): complete (1 captures: server 1, driver 0)"),
            "{report}"
        );
        assert_eq!(
            report
                .matches("    - driver: — unavailable: this record holds no driver observation for this step")
                .count(),
            0,
            "a record with no diagnostics says so once, not per role"
        );
    }

    /// V8: a persisted multi-criterion record renders the exact common
    /// header, values with units, spread, reasons, and every verdict, and
    /// re-reading it after its raw artifacts are deleted renders the same
    /// report. A full run's target miss reads back VALID with its observed
    /// value and exit 0, as does a met target, and a busy host INVALID with
    /// exit 2, which keeps the miss in its record.
    ///
    /// # Panics
    ///
    /// Panics when the header, a cell, a read-back verdict or exit code, or
    /// the regenerated report differs.
    #[tokio::test(start_paused = true)]
    async fn reports_regenerate_from_persisted_records() {
        let root = tempfile::tempdir().expect("root");
        let artifacts = root.path().join("target/run");
        std::fs::create_dir_all(&artifacts).expect("artifacts");
        std::fs::write(artifacts.join("samples.jsonl"), "{}").expect("raw samples");
        let options = options(&artifacts);
        let mut slow = good();
        slow.set("p99_ms", Reading::exact(0.7).with_samples(2_000));
        slow.set("client_p99_ms", Reading::exact(0.9));
        let mut workload = Scripted::always(&good());
        workload.script = std::iter::repeat_n(Ok(good()), 4)
            .chain(std::iter::repeat_n(Ok(slow.clone()), 4))
            .collect();
        let failed = execute(
            declaration(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        let mut workload = Scripted::always(&good());
        let passed = execute(
            declaration(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        let mut workload = Scripted::always(&good());
        let invalid = execute(
            declaration(),
            &mut workload,
            [Some(loaded(10.0)), Some(quiet())],
            &options,
        )
        .await;
        let mut workload = Scripted::always(&good());
        workload.script = std::iter::repeat_n(Ok(good()), 4)
            .chain(std::iter::repeat_n(Ok(slow.clone()), 4))
            .collect();
        let failed_on_busy_host = execute(
            declaration(),
            &mut workload,
            [Some(quiet()), Some(loaded(10.0))],
            &options,
        )
        .await;

        for (record, verdict, exit) in [
            (&failed, Verdict::Valid, 0),
            (&passed, Verdict::Valid, 0),
            (&invalid, Verdict::Invalid, 2),
            (&failed_on_busy_host, Verdict::Invalid, 2),
        ] {
            std::fs::create_dir_all(&artifacts).expect("artifacts");
            let console = record.render();
            let path = record.publish(root.path()).expect("publish");
            std::fs::remove_dir_all(&artifacts).ok();
            let read = Record::read(&path).expect("read");
            assert_eq!((read.verdict, read.exit_code()), (verdict, exit));
            assert_eq!(
                console,
                read.render(),
                "report regenerates from history alone"
            );
            assert!(
                console.lines().any(|line| line == REPORT_HEADER),
                "exact header"
            );
            assert!(console.contains(&format!("verdict: {} (exit {exit})", verdict.label())));
        }
        let missed = |record: &Record| {
            record.steps[1]
                .criteria
                .iter()
                .find(|judgment| judgment.criterion == "overhead-slo")
                .map(|judgment| (judgment.outcome, judgment.observed.value))
        };
        assert_eq!(missed(&failed), Some((Some(Outcome::Missed), Some(0.7))));
        assert_eq!(
            missed(&failed_on_busy_host).map(|(_, observed)| observed),
            Some(Some(0.7)),
            "the miss is kept under the invalidity"
        );
        let report = failed.render();
        assert!(
            report.contains("| op-64 | clients=64 | 100 ops/s; 500 ops/cpu-s | — / — / 0.7 (client — / — / 0.9) |"),
            "{report}"
        );
        assert!(report.contains("2 s / 0.2 cores"));
        assert!(report.contains("512 MiB (sampled 10 ms)"));
        assert!(
            report.contains(
                "overhead-slo: p99_ms < 0.5 ms | 0.7 ms spread 0 | missed: median 0.7 misses < 0.5 |"
            ),
            "{report}"
        );
        assert!(!report.contains("criterion."), "{report}");
        assert!(invalid.render().contains("- host.busy"));
        assert!(
            invalid.render().contains("— the run admitted no workload")
                || invalid.render().contains("| INVALID |")
        );
    }

    /// A repetition kept before the opening-memory reading existed still
    /// reads, with no opening reading.
    ///
    /// # Panics
    ///
    /// Panics when the older repetition is refused or invents a reading.
    #[test]
    fn repetitions_without_opening_memory_still_read() {
        let mut older = serde_json::to_value(good()).expect("serializes");
        older
            .as_object_mut()
            .expect("an object")
            .remove("opening_memory_bytes");
        let read: Repetition = serde_json::from_value(older).expect("an older repetition reads");
        assert_eq!(read.opening_memory_bytes, None);
        assert_eq!(read.peak_memory_bytes, good().peak_memory_bytes);
    }

    /// V9: absent, failing, incomplete, and successful profilers leave the
    /// measured verdict and evidence identical, each records a diagnostic
    /// note or capture, and a profile never stands in for missing CPU.
    ///
    /// # Panics
    ///
    /// Panics when profiling changes judgment or its note is wrong.
    #[tokio::test(start_paused = true)]
    async fn profiling_is_optional_diagnostic_evidence() {
        let root = tempfile::tempdir().expect("root");
        let saving = profiler(root.path(), "saving-profiler", Sidecar::Resolved, 0);
        let silent = root.path().join("silent-profiler");
        std::fs::write(
            &silent,
            "#!/bin/sh\ntrap 'exit 0' INT\nwhile :; do sleep 0.02; done\n",
        )
        .expect("script");
        let mut permissions = std::fs::metadata(&silent).expect("metadata").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(&silent, permissions).expect("chmod");
        let profiled = |program: Option<PathBuf>| Options {
            artifacts: root.path().join("target"),
            profile: program.map(|program| Profiling {
                mode: ProfileMode::Cpu,
                program,
                symbols: root.path().join("symbols"),
            }),
            interrupt: CancellationToken::new(),
            setup_spent: Duration::ZERO,
        };
        let run_with = |options: Options, repetition: Repetition| async move {
            let mut workload = Scripted::always(&repetition);
            workload.settle = Some(Duration::from_millis(500));
            execute(
                declaration(),
                &mut workload,
                [Some(quiet()), Some(quiet())],
                &options,
            )
            .await
        };
        let unprofiled = run_with(profiled(None), good()).await;
        assert!(!unprofiled.artifacts.profile.requested);
        for (program, complete, note) in [
            (root.path().join("absent-samply"), false, "did not start"),
            (PathBuf::from("false"), false, "ended early"),
            (silent.clone(), false, "left no"),
            (saving.clone(), true, ""),
        ] {
            let record = run_with(profiled(Some(program)), good()).await;
            assert_eq!(
                record.verdict, unprofiled.verdict,
                "profiling never changes a verdict"
            );
            assert_eq!(record.steps[1].summary, unprofiled.steps[1].summary);
            let profile = &record.artifacts.profile;
            assert!(profile.requested);
            assert_eq!(profile.complete, complete, "{profile:?}");
            if complete {
                assert_eq!(profile.captures.len(), 2, "one per measured step");
                assert!(record.render().contains(
                    "profile cpu of server and driver: complete (2 captures: server 2, driver 0)"
                ));
            } else {
                assert!(
                    profile.notes.iter().any(|n| n.contains(note)),
                    "{note}: {profile:?}"
                );
                assert!(
                    record
                        .render()
                        .contains("profile cpu of server and driver: incomplete")
                );
            }
        }
        let mut lost = good();
        lost.server_cpu_seconds = None;
        let record = run_with(profiled(Some(saving)), lost).await;
        assert_eq!(
            record.verdict,
            Verdict::Invalid,
            "a profile never supplies the CPU denominator"
        );
    }

    /// V9: a capture slow to save neither postpones the drain nor changes
    /// the budget judgment or verdict, and is still kept; a genuine overrun
    /// is INVALID with and without a profile.
    ///
    /// # Panics
    ///
    /// Panics when saving delays the drain or decides the budget.
    #[tokio::test(start_paused = true)]
    async fn profile_saving_never_delays_drain_or_budget() {
        let root = tempfile::tempdir().expect("root");
        let slow = profiler(root.path(), "slow-profiler", Sidecar::Resolved, 1);
        let artifacts = root.path().join("target");
        let capture = artifacts
            .join("profiles")
            .join("op-64")
            .join(format!("cpu-server-{}.json.gz", std::process::id()));
        let run_with = |program: Option<PathBuf>, stall: Duration| {
            let (artifacts, capture) = (artifacts.clone(), capture.clone());
            let symbols = artifacts.join("symbols");
            async move {
                let mut workload = Scripted::always(&good());
                workload.settle = Some(Duration::from_millis(500));
                workload.stall = stall;
                workload.watch = Some(capture);
                let options = Options {
                    artifacts,
                    profile: program.map(|program| Profiling {
                        mode: ProfileMode::Both,
                        program,
                        symbols: symbols.clone(),
                    }),
                    interrupt: CancellationToken::new(),
                    setup_spent: Duration::ZERO,
                };
                let record = execute(
                    declaration(),
                    &mut workload,
                    [Some(quiet()), Some(quiet())],
                    &options,
                )
                .await;
                let saved = workload.seen.lock().expect("seen").saved_at_drain;
                (record, saved)
            }
        };
        let (unprofiled, _) = run_with(None, Duration::ZERO).await;
        let (profiled, saved) = run_with(Some(slow.clone()), Duration::ZERO).await;
        assert_eq!(saved, Some(false), "the drain began before saving ended");
        assert!(
            profiled.artifacts.profile.complete,
            "{:?}",
            profiled.artifacts.profile
        );
        assert_eq!(profiled.verdict, unprofiled.verdict);
        assert_eq!(profiled.elapsed_seconds, unprofiled.elapsed_seconds);
        assert_eq!(validity(&profiled), validity(&unprofiled));
        for profiler in [None, Some(slow)] {
            let (overrun, _) = run_with(profiler, Duration::from_secs(250)).await;
            assert!(
                codes(&overrun).contains(&"budget.exceeded"),
                "{:?}",
                overrun.reasons
            );
            assert_eq!(overrun.verdict, Verdict::Invalid);
        }
    }

    /// Runs the two-step declaration profiled in `mode`, or unprofiled,
    /// writing artifacts under `artifacts`.
    async fn profiled_run(artifacts: &Path, program: &Path, mode: Option<ProfileMode>) -> Record {
        let mut workload = Scripted::always(&good());
        workload.settle = Some(Duration::from_millis(500));
        let options = Options {
            artifacts: artifacts.to_path_buf(),
            profile: mode.map(|mode| Profiling {
                mode,
                program: program.to_path_buf(),
                symbols: artifacts.join("symbols"),
            }),
            interrupt: CancellationToken::new(),
            setup_spent: Duration::ZERO,
        };
        execute(
            declaration(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await
    }

    /// The files a run left under its `profiles/` directory, relative to it.
    fn profile_files(artifacts: &Path) -> Vec<String> {
        let profiles = artifacts.join("profiles");
        let mut files = Vec::new();
        let mut pending = vec![profiles.clone()];
        while let Some(directory) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                } else if let Ok(relative) = path.strip_prefix(&profiles) {
                    files.push(relative.display().to_string());
                }
            }
        }
        files.sort();
        files
    }

    /// Each mode starts only its collectors: no `--profile` starts neither,
    /// `cpu` runs a CPU capture per measured step and no heap sampler,
    /// `heap` starts no CPU profiler process and captures the driver's heap
    /// after each step, and `both` does both; none changes the verdict.
    ///
    /// # Panics
    ///
    /// Panics when a mode starts a collector it did not ask for or misses
    /// one it did.
    #[tokio::test(start_paused = true)]
    async fn profile_modes_start_only_the_requested_collectors() {
        let root = tempfile::tempdir().expect("root");
        let program = profiler(root.path(), "saving-profiler", Sidecar::Resolved, 0);
        let pid = std::process::id();
        let cpu = [
            format!("op-1/cpu-server-{pid}.json.gz"),
            format!("op-1/cpu-server-{pid}.json.syms.json"),
            format!("op-1/samply-server-{pid}.log"),
            format!("op-64/cpu-server-{pid}.json.gz"),
            format!("op-64/cpu-server-{pid}.json.syms.json"),
            format!("op-64/samply-server-{pid}.log"),
        ];
        let heap = ["heap-driver-op-1.pb.gz", "heap-driver-op-64.pb.gz"].map(str::to_owned);
        let unprofiled = profiled_run(&root.path().join("none"), &program, None).await;
        assert!(!unprofiled.artifacts.profile.requested);
        assert!(profile_files(&root.path().join("none")).is_empty());
        for (mode, expected) in [
            (ProfileMode::Cpu, cpu.to_vec()),
            (ProfileMode::Heap, heap.to_vec()),
            (ProfileMode::Both, [cpu.to_vec(), heap.to_vec()].concat()),
        ] {
            let artifacts = root.path().join(mode.name());
            let record = profiled_run(&artifacts, &program, Some(mode)).await;
            let files = profile_files(&artifacts);
            let mut expected = expected.clone();
            expected.sort();
            assert_eq!(files, expected, "{mode:?}");
            let note = &record.artifacts.profile;
            assert!(note.complete, "{mode:?}: {note:?}");
            assert_eq!(note.mode, Some(mode));
            assert_eq!(record.verdict, unprofiled.verdict, "{mode:?}");
        }
    }

    /// A profiled record keeps its mode, both process roles, each capture
    /// under its measured step, and the declaration's executable identity;
    /// a record that predates modes and roles reads back as the server-only
    /// profile it was.
    ///
    /// # Panics
    ///
    /// Panics when a manifest field is lost or an old record misreads.
    #[tokio::test(start_paused = true)]
    async fn profile_manifests_preserve_executable_and_window_identity() {
        let root = tempfile::tempdir().expect("root");
        let program = profiler(root.path(), "saving-profiler", Sidecar::Resolved, 0);
        let record = profiled_run(root.path(), &program, Some(ProfileMode::Both)).await;
        let read: Record =
            serde_json::from_str(&serde_json::to_string(&record).expect("json")).expect("record");
        let note = &read.artifacts.profile;
        assert_eq!(note.mode, Some(ProfileMode::Both));
        assert_eq!(note.roles, [Role::Server, Role::Driver]);
        for step in ["op-1", "op-64"] {
            assert!(
                note.captures
                    .iter()
                    .any(|capture| capture.contains(&format!("/{step}/cpu-server-"))),
                "{step}: {note:?}"
            );
            assert!(
                note.captures
                    .iter()
                    .any(|capture| capture.ends_with(&format!("heap-driver-{step}.pb.gz"))),
                "{step}: {note:?}"
            );
        }
        assert_eq!(
            read.build,
            declaration().build,
            "executable identity is kept"
        );
        let old: ProfileNote = serde_json::from_value(json!({
            "requested": true,
            "complete": true,
            "captures": ["profiles/profile-1.json.gz"],
            "notes": [],
        }))
        .expect("an old note");
        assert_eq!((old.mode, old.roles.len()), (None, 0));
    }

    /// The representative declaration narrowed to its `op-64` case on the
    /// L1 case clock, as a sweep's case process declares one case.
    fn case_declaration() -> Declaration {
        let mut declaration = declaration();
        declaration.steps.retain(|step| step.id == "op-64");
        declaration
            .criteria
            .retain(|criterion| criterion.steps.iter().any(|step| step == "op-64"));
        for criterion in &mut declaration.criteria {
            criterion.steps = vec!["op-64".to_owned()];
        }
        declaration.timing = Level::case_timing();
        declaration
    }

    /// A window of `ops` operations over `seconds` on `cpu` server
    /// CPU-seconds, with a p99 read from `latencies` nanoseconds.
    fn window(ops: f64, seconds: f64, cpu: Option<f64>, latencies: &[u64]) -> Repetition {
        let mut repetition = Repetition {
            seconds,
            server_cpu_seconds: cpu,
            ..good()
        };
        repetition.set("ops", Reading::exact(ops));
        let distribution =
            wyrd_benchmark::Distribution::of_durations(latencies).expect("durations");
        repetition.set(
            "p99_ms",
            Reading::quantile(&distribution, 0.99, crate::capacity::NANOS_PER_MS),
        );
        repetition
    }

    /// `count` latencies of `nanos` each.
    fn samples(count: usize, nanos: u64) -> Vec<u64> {
        vec![nanos; count]
    }

    /// A case's aggregate divides summed work by summed seconds and CPU,
    /// never averaging window ratios, pools the windows' histograms for its
    /// percentile with their combined sample count and bucket bounds, and
    /// leaves a reading missing, never zero, when a window lacks CPU, work
    /// or a kept histogram.
    #[test]
    fn case_aggregates_sum_windows_and_pool_histograms() {
        let metrics = declaration().metrics;
        let windows = [
            window(100.0, 10.0, Some(1.0), &samples(1_000, 1_000_000)),
            window(1_000.0, 30.0, Some(4.0), &samples(10, 100_000_000)),
            window(10.0, 20.0, Some(0.5), &samples(1_000, 2_000_000)),
        ];
        let aggregate = Repetition::aggregate(&metrics, &windows);
        assert_eq!(aggregate.seconds, 60.0);
        assert_eq!(aggregate.server_cpu_seconds, Some(5.5));
        assert_eq!(aggregate.reading("ops").value, Some(1_110.0));
        let per_cpu = aggregate.reading("ops_per_cpu_s").value.expect("per cpu");
        assert!((per_cpu - 1_110.0 / 5.5).abs() < 1e-9, "{per_cpu}");
        let mean_of_ratios = (100.0 / 1.0 + 1_000.0 / 4.0 + 10.0 / 0.5) / 3.0;
        assert!(
            (per_cpu - mean_of_ratios).abs() > 10.0,
            "never an average of ratios"
        );
        let pooled = aggregate.reading("p99_ms");
        let mut all = samples(1_000, 1_000_000);
        all.extend(samples(10, 100_000_000));
        all.extend(samples(1_000, 2_000_000));
        let expected = Reading::quantile(
            &wyrd_benchmark::Distribution::of_durations(&all).expect("all"),
            0.99,
            crate::capacity::NANOS_PER_MS,
        );
        assert_eq!(pooled, expected);
        assert_eq!(pooled.samples, Some(2_010));
        assert_eq!(pooled.rank, Some(1_990));
        assert!(
            pooled.value.is_some_and(|value| value < 3.0),
            "pooled, not the mean of window p99s: {pooled:?}"
        );
        let mut lost = windows.clone();
        lost[1].server_cpu_seconds = None;
        let aggregate = Repetition::aggregate(&metrics, &lost);
        assert_eq!(aggregate.server_cpu_seconds, None);
        assert!(!aggregate.reading("ops_per_cpu_s").is_present());
        assert!(aggregate.reading("ops_per_cpu_s").reason.is_some());
        let mut unread = windows.clone();
        unread[2].set("ops", Reading::missing("the counter reset"));
        assert!(
            Repetition::aggregate(&metrics, &unread)
                .reading("ops")
                .reason
                .is_some_and(
                    |reason| reason.contains("window 3") && reason.contains("the counter reset")
                )
        );
        let mut bare = windows;
        bare[0].set("p99_ms", Reading::exact(1.0).with_samples(1_000));
        let unpooled = Repetition::aggregate(&metrics, &bare).reading("p99_ms");
        assert!(!unpooled.is_present());
        assert_eq!(
            unpooled.reason.as_deref(),
            Some("window 1 kept no histogram to pool")
        );
    }

    /// A workload whose every call admits for its window, then drains for
    /// `drain`, recording when each call started and returned.
    struct Draining {
        /// The repetition each call returns.
        repetition: Repetition,
        /// The paused-clock drain after each window.
        drain: Duration,
        /// How long prepare takes.
        setup: Duration,
        /// Each call's start and return.
        calls: Arc<Mutex<Vec<(tokio::time::Instant, tokio::time::Instant)>>>,
    }

    #[async_trait]
    impl Workload for Draining {
        async fn prepare(&mut self) -> Result<(), ExperimentError> {
            tokio::time::sleep(self.setup).await;
            Ok(())
        }

        fn processes(&self) -> Vec<u32> {
            vec![std::process::id()]
        }

        async fn measure(
            &mut self,
            _: &Step,
            _: Phase,
            window: Duration,
        ) -> Result<Repetition, ExperimentError> {
            let opened = tokio::time::Instant::now();
            tokio::time::sleep(window + self.drain).await;
            self.calls
                .lock()
                .expect("calls")
                .push((opened, tokio::time::Instant::now()));
            Ok(self.repetition.clone())
        }

        async fn finish(&mut self) -> Result<(), ExperimentError> {
            Ok(())
        }

        async fn outstanding(&mut self) -> Result<Option<Value>, ExperimentError> {
            Ok(Some(json!({ "staged_members": 0 })))
        }
    }

    /// Runs the case declaration over a [`Draining`] workload that drains
    /// `drain` after each window and prepares in `setup`, `spent` setup
    /// already used, and returns the record and each call's span.
    async fn case_run(
        drain: Duration,
        setup: Duration,
        spent: Duration,
    ) -> (Record, Vec<(tokio::time::Instant, tokio::time::Instant)>) {
        let directory = tempfile::tempdir().expect("artifacts");
        let calls = Arc::default();
        let mut workload = Draining {
            repetition: window(1_000.0, 30.0, Some(3.0), &samples(2_000, 300_000)),
            drain,
            setup,
            calls: Arc::clone(&calls),
        };
        let options = Options {
            setup_spent: spent,
            ..options(directory.path())
        };
        let mut samples = VecDeque::from([Some(quiet()), Some(quiet())]);
        let record = run(
            case_declaration().seal(),
            &mut workload,
            move |_| std::future::ready(samples.pop_front().flatten()),
            &options,
        )
        .await;
        let calls = calls.lock().expect("calls").clone();
        (record, calls)
    }

    /// On the case clock a case warms up once, measures three fixed 30 s
    /// windows whatever window its step declares, drains each window before
    /// the next opens, charges every drain to the one shared drain
    /// allowance, keeps its owned processes and outstanding work, and is
    /// INVALID when a drain or its setup overruns an allowance, with the
    /// phase named; nothing extends an allowance.
    #[tokio::test(start_paused = true)]
    async fn case_clock_charges_shared_allowances_and_invalidates_overruns() {
        let (record, calls) = case_run(
            Duration::from_secs(5),
            Duration::from_secs(10),
            Duration::from_secs(20),
        )
        .await;
        assert_eq!(record.verdict, Verdict::Valid, "{:?}", record.reasons);
        assert_eq!(calls.len(), 4, "a warmup and three windows");
        for pair in calls.windows(2) {
            assert!(
                pair[1].0 >= pair[0].1,
                "a window opens only after the last drained"
            );
        }
        let lifecycle = record.lifecycle.as_ref().expect("a case lifecycle");
        assert_eq!(lifecycle.allowances, super::Allowances::L1);
        assert_eq!(lifecycle.allowances.total(), 590);
        assert!(
            (lifecycle.setup_seconds - 30.0).abs() < 0.01,
            "{}",
            lifecycle.setup_seconds
        );
        assert!((lifecycle.warmup_seconds - 30.0).abs() < 0.01);
        assert!((lifecycle.measurement_seconds - 90.0).abs() < 0.01);
        assert!(
            (lifecycle.drain_seconds - 20.0).abs() < 0.01,
            "{}",
            lifecycle.drain_seconds
        );
        assert_eq!(
            lifecycle
                .windows
                .iter()
                .map(|window| window.phase.as_str())
                .collect::<Vec<_>>(),
            ["warmup", "repetition-1", "repetition-2", "repetition-3"]
        );
        assert!(
            lifecycle
                .windows
                .iter()
                .all(|window| (window.drained_seconds - 5.0).abs() < 0.01)
        );
        assert_eq!(lifecycle.owned.len(), 1);
        assert_eq!(lifecycle.owned[0].pid, std::process::id());
        assert!(lifecycle.owned[0].alive());
        assert_eq!(lifecycle.outstanding, Some(json!({ "staged_members": 0 })));
        assert_eq!(record.steps[0].timing.repetition_seconds, 30);

        let (overrun, calls) =
            case_run(Duration::from_secs(40), Duration::ZERO, Duration::ZERO).await;
        assert_eq!(overrun.verdict, Verdict::Invalid);
        assert_eq!(
            calls.len(),
            3,
            "the fourth call ran out of shared drain allowance"
        );
        assert!(
            codes(&overrun).contains(&"phase.deadline"),
            "{:?}",
            overrun.reasons
        );
        assert!(
            overrun
                .reasons
                .iter()
                .any(|reason| reason.message.contains("drain and capture allowance")),
            "{:?}",
            overrun.reasons
        );
        let drained = overrun.lifecycle.as_ref().expect("lifecycle").drain_seconds;
        assert!(drained >= 150.0 && drained < 151.0, "{drained}");

        let (late, calls) = case_run(
            Duration::ZERO,
            Duration::from_secs(30),
            Duration::from_secs(220),
        )
        .await;
        assert_eq!(late.verdict, Verdict::Invalid);
        assert!(calls.is_empty(), "nothing is measured after setup overran");
        assert!(
            late.reasons
                .iter()
                .any(|reason| reason.code == "phase.deadline" && reason.message.contains("setup"))
        );
        let setup = late.lifecycle.as_ref().expect("lifecycle").setup_seconds;
        assert!(
            (setup - 240.0).abs() < 0.01,
            "the deadline never extends setup: {setup}"
        );

        let (spent, _) = case_run(Duration::ZERO, Duration::ZERO, Duration::from_secs(250)).await;
        assert_eq!(spent.verdict, Verdict::Invalid);
        assert!(
            spent
                .reasons
                .iter()
                .any(|reason| reason.code == "phase.overrun"
                    && reason.message.contains("setup took 250.0s")),
            "{:?}",
            spent.reasons
        );
    }

    /// A case's targets judge the same pooled aggregate its summary shows:
    /// two fast windows and one slow one miss the p99 target although the
    /// median window meets it, and the record stays VALID; a window under
    /// its sample floor or failing a correctness check is INVALID instead.
    #[tokio::test(start_paused = true)]
    async fn case_targets_judge_aggregates_apart_from_validity() {
        let fast = window(1_000.0, 30.0, Some(3.0), &samples(2_000, 300_000));
        let slow = window(1_000.0, 30.0, Some(3.0), &samples(2_000, 900_000));
        let judged = |windows: Vec<Repetition>| async move {
            let directory = tempfile::tempdir().expect("artifacts");
            let mut script: VecDeque<Result<Repetition, String>> =
                std::iter::once(Ok(windows[0].clone()))
                    .chain(windows.into_iter().map(Ok))
                    .collect();
            script.push_back(Err("exhausted".to_owned()));
            let mut workload = Scripted {
                script,
                ..Scripted::always(&good())
            };
            execute(
                case_declaration(),
                &mut workload,
                [Some(quiet()), Some(quiet())],
                &options(directory.path()),
            )
            .await
        };
        let missed = judged(vec![fast.clone(), fast.clone(), slow.clone()]).await;
        assert_eq!(missed.verdict, Verdict::Valid, "{:?}", missed.reasons);
        let step = &missed.steps[0];
        let slo = step
            .criteria
            .iter()
            .find(|judgment| judgment.criterion == "overhead-slo")
            .expect("judged");
        assert_eq!(slo.outcome, Some(Outcome::Missed), "{slo:?}");
        assert_eq!(
            slo.observed,
            step.aggregate
                .as_ref()
                .expect("aggregate")
                .reading("p99_ms")
                .bare()
        );
        assert_eq!(slo.observed.samples, Some(6_000));
        assert!(
            slo.reason
                .as_deref()
                .is_some_and(|reason| reason.starts_with("aggregate "))
        );
        assert!(
            step.summary["p99_ms"]
                .median
                .value
                .is_some_and(|value| value < 0.5),
            "the median window meets it"
        );
        assert!(
            slo.values.iter().all(|value| value.pool.is_none()),
            "judgments repeat no histogram"
        );
        assert!(
            step.repetitions
                .iter()
                .all(|window| window.reading("p99_ms").pool.is_some()),
            "windows keep theirs"
        );

        let met = judged(vec![fast.clone(), fast.clone(), fast.clone()]).await;
        assert_eq!(
            met.steps[0]
                .criteria
                .iter()
                .find(|j| j.criterion == "overhead-slo")
                .and_then(|j| j.outcome),
            Some(Outcome::Met)
        );

        let short = window(1_000.0, 30.0, Some(3.0), &samples(500, 300_000));
        let floor = judged(vec![fast.clone(), short, fast.clone()]).await;
        assert_eq!(floor.verdict, Verdict::Invalid);
        assert!(codes(&floor).contains(&"criterion.invalid"));

        let mut wrong = fast.clone();
        wrong.set("errors", Reading::count(1));
        let integrity = judged(vec![fast.clone(), wrong, fast]).await;
        assert_eq!(integrity.verdict, Verdict::Invalid);
        assert!(
            integrity
                .reasons
                .iter()
                .any(|reason| reason.criterion.as_deref() == Some("no-errors"))
        );
    }

    /// Records outside the case protocol drop the kept window histograms
    /// and carry no lifecycle, and a case-protocol record is refused by the
    /// schema-3 comparison reader.
    #[tokio::test(start_paused = true)]
    async fn legacy_records_keep_their_protocol_boundary() {
        let directory = tempfile::tempdir().expect("artifacts");
        let fast = window(1_000.0, 30.0, Some(3.0), &samples(2_000, 300_000));
        let mut workload = Scripted::always(&fast);
        let legacy = execute(
            declaration(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options(directory.path()),
        )
        .await;
        assert_eq!(legacy.lifecycle, None);
        assert!(legacy.steps.iter().all(|step| step.aggregate.is_none()));
        assert!(
            legacy
                .steps
                .iter()
                .flat_map(|step| &step.repetitions)
                .all(|window| window
                    .metrics
                    .values()
                    .all(|reading| reading.pool.is_none()))
        );
        assert_eq!(
            Record::parse(&serde_json::to_string(&legacy).expect("json")).expect("reads"),
            legacy
        );
        let (case, _) = case_run(Duration::ZERO, Duration::ZERO, Duration::ZERO).await;
        let reread = Record::parse(&serde_json::to_string(&case).expect("json")).expect("reads");
        assert_eq!(reread, case);
        assert!(
            legacy
                .compare(&case)
                .expect_err("refused")
                .contains("L1 case-protocol")
        );
        assert!(case.compare(&case).is_err());
    }
}
