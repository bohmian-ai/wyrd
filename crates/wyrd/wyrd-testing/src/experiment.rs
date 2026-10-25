//! Benchmarks as declared experiments: one workflow from declaration to a
//! persisted, regenerable record, shared by every engine.
//!
//! A run follows one order. A [`Declaration`] fixes the question, level,
//! sweep, controls, metric units and boundaries, criteria, baseline
//! snapshots, validity thresholds, and budget, and is sealed before any
//! workload runs. [`run`] then checks quiet-host eligibility, prepares the
//! [`Workload`], discards each step's warmup, measures three repetitions,
//! samples the host again, and judges every applicable criterion into a
//! [`Record`]. The record is published append-only under
//! `benchmarks/<engine>/<experiment>/` and every report is rendered from it.
//! A [`Mode::Smoke`] runs that same workflow at reduced declared controls
//! and keeps its record under [`SMOKE_DIRECTORY`].
//!
//! Judgment has three outcomes. PASS means valid evidence met every
//! criterion; FAIL means valid evidence missed one; INVALID means the run did
//! not measure what it claims. INVALID beats FAIL beats PASS, with exit codes
//! 2, 1, and 0, and a valid failure stays visible beside an invalidity
//! reason. Numeric criteria judge the median repetition; correctness criteria
//! hold in every repetition. A missing, reset, or unresolved reading is
//! INVALID evidence, never a favorable zero.
//!
//! Engines supply only their [`Workload`] (production setup and client
//! calls) and declarations; this module owns lifecycle, validity, judgment,
//! history, and reporting. Profiling through [`crate::profile`] is optional
//! diagnostic evidence that never changes a verdict.

use std::collections::BTreeMap;
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

use crate::profile::{ProfileNote, Profiler};
use crate::release_server::{Host, HostLoad};

/// The persisted record version this module writes: server timings from
/// bounded histogram snapshots, percentiles as bucket bounds.
pub const SCHEMA_VERSION: u64 = 2;

/// The earlier record version this module still reads and reports with its
/// original meaning: exact raw timing spans from a collector process.
pub const LEGACY_SCHEMA_VERSION: u64 = 1;

/// Repetitions every measured step keeps.
pub const REPETITIONS: u8 = 3;

/// Where smoke records and evidence live, under the repository root:
/// local build output, never committed history.
pub const SMOKE_DIRECTORY: &str = "target/benchmarks/smoke";

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
    #[error("record is not a well-formed v1 or v2 record: {0}")]
    Malformed(String),
    /// The record names a schema version this reader does not interpret.
    #[error("record schema_version {0} is not supported; this reader reads versions 1 and 2")]
    Unsupported(u64),
    /// A baselines file could not be read or parsed.
    #[error("baselines {path} could not be read: {message}")]
    Baselines {
        /// The baselines file.
        path: PathBuf,
        /// What was wrong with it.
        message: String,
    },
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
    /// Fast isolated work: 5 s warmup, 3 × 10 s, at most 15 minutes.
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
                repetition_seconds: 10,
                repetitions: REPETITIONS,
                budget_seconds: 15 * 60,
                minimum_seconds: 0,
            },
            Self::Two => Timing {
                warmup_seconds: 120,
                repetition_seconds: 360,
                repetitions: REPETITIONS,
                budget_seconds: 30 * 60,
                minimum_seconds: 20 * 60,
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
            },
            Self::Two => Timing {
                warmup_seconds: 3,
                repetition_seconds: 36,
                repetitions: 1,
                budget_seconds: 15 * 60,
                minimum_seconds: 0,
            },
        }
    }
}

/// Whether a run is a full experiment or a smoke of the same workflow at
/// reduced declared controls.
///
/// A smoke runs the same declaration, fixtures, steps, phases,
/// measurement, judgment, and record as a full run with a smaller fixture
/// and [`Level::smoke_timing`]. It is functional evidence only: it never
/// qualifies performance or serves as a reference, its records stay under
/// `target/benchmarks/smoke/`, and percentile floors and full-run
/// reference comparisons are explicitly inapplicable to it. A record
/// without a mode is a full run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// The full declared experiment.
    #[default]
    Full,
    /// The reduced functional smoke.
    Smoke,
}

/// The automatic smoke a full run executed in its own process, fresh
/// database, and clock before preparing any full-size fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prepass {
    /// The smoke's command, as argument values.
    pub command: Vec<String>,
    /// The smoke's declared budget, seconds.
    pub budget_seconds: u64,
    /// Wall seconds it took, outside the full run's clock.
    pub elapsed_seconds: f64,
    /// Its exit code; only 0 admits the full run.
    pub exit_code: u8,
    /// Where its record and timing evidence live.
    pub records: String,
}

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
}

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
            driver_cores: 8.0,
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
    /// Which way the metric improves, when it is a regression candidate.
    pub direction: Option<Direction>,
    /// How the metric is derived, or `None` for a raw workload reading.
    pub derive: Option<Derive>,
}

impl Metric {
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

/// The baseline value a criterion compares against, fixed at declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Baseline entry id in `baselines.toml`.
    pub id: String,
    /// The entry's value in its own unit.
    pub value: f64,
    /// The entry's unit.
    pub unit: String,
    /// The value converted to the metric's unit.
    pub converted: f64,
    /// The source report.
    pub source_report: String,
    /// The source commit.
    pub source_commit: String,
    /// The source host fingerprint.
    pub host: String,
    /// The source fixture.
    pub fixture: String,
    /// The original operands the value was derived from.
    pub operands: String,
}

/// How a criterion judges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Rule {
    /// An absolute limit.
    Limit {
        /// The comparison.
        op: Op,
        /// The limit, in the metric's unit.
        limit: f64,
        /// Which repetitions it judges.
        over: Over,
    },
    /// The noise-aware regression rule against an eligible same-host
    /// baseline.
    Regression {
        /// The baseline fixed at declaration.
        baseline: Snapshot,
    },
    /// A comparison against an eligible baseline from another host or
    /// fixture, recorded with both fingerprints; it never judges.
    Advisory {
        /// The baseline fixed at declaration.
        baseline: Snapshot,
        /// Why it cannot judge.
        reason: String,
    },
    /// A declared baseline comparison whose reference is missing, excluded,
    /// or unit-incompatible; it judges INVALID.
    Unusable {
        /// The baseline entry id named.
        baseline: String,
        /// Why it cannot be used.
        reason: String,
    },
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
            rule: Rule::Limit {
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
            rule: Rule::Limit {
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
        match &self.rule {
            Rule::Limit { op, limit, over } => format!(
                "{} {} {} {unit}{}",
                self.metric,
                op.symbol(),
                number(*limit),
                if *over == Over::Every {
                    " every rep"
                } else {
                    ""
                }
            ),
            Rule::Regression { baseline } => {
                format!(
                    "{} vs {} {unit} ({})",
                    self.metric,
                    number(baseline.converted),
                    baseline.id
                )
            }
            Rule::Advisory { baseline, .. } => {
                format!(
                    "{} advisory vs {} {unit}",
                    self.metric,
                    number(baseline.converted)
                )
            }
            Rule::Unusable { baseline, .. } => format!("{} vs {baseline}", self.metric),
        }
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
    /// The one-line fingerprint baselines are matched by.
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
    /// The smoke a full run passed first, or `None` for a smoke.
    pub prepass: Option<Prepass>,
}

/// Controls every declaration names.
const REQUIRED_CONTROLS: [&str; 3] = ["fixture", "limits", "workload"];

impl Declaration {
    /// The reasons this declaration cannot admit a workload: missing
    /// question, steps, controls, metrics, or source; criteria naming
    /// undeclared metrics or steps; a dirty source without override; a
    /// failed smoke prepass; and declaration-time refusals.
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
        if self.source.dirty && !self.source.dirty_override {
            refuse(
                "source.dirty",
                "tracked files differ from HEAD; commit them or declare --allow-dirty".to_owned(),
            );
        }
        if let Some(prepass) = self
            .prepass
            .as_ref()
            .filter(|prepass| prepass.exit_code != 0)
        {
            refuse(
                "prepass.failed",
                format!(
                    "the smoke prepass `{}` exited {}; its evidence is under {}",
                    prepass.command.join(" "),
                    prepass.exit_code,
                    prepass.records
                ),
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
    /// Stable code, such as `host.busy` or `criterion.failed`.
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

    /// Whether this reason invalidates rather than fails.
    fn invalidates(&self) -> bool {
        self.code != "criterion.failed"
    }
}

/// PASS, FAIL, or INVALID, ordered by precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Verdict {
    /// Valid evidence met every criterion.
    Pass,
    /// Valid evidence missed at least one criterion.
    Fail,
    /// The run did not measure what it claims.
    Invalid,
}

impl Verdict {
    /// The process exit code: 0, 1, or 2.
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Fail => 1,
            Self::Invalid => 2,
        }
    }

    /// The verdict's report label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Invalid => "INVALID",
        }
    }

    /// The precedence-combined verdict of several exit codes, as the L1
    /// aggregate combines engines: any 2 wins, then any 1, else 0.
    #[must_use]
    pub fn combine(codes: &[u8]) -> u8 {
        codes.iter().copied().max().unwrap_or(0).min(2)
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

    /// Nearest-rank percentile `q` of a histogram `distribution`, each value
    /// divided by `per`, as the [`Self::bounded`] interval of its bucket,
    /// with the sample count and rank; missing without samples.
    #[must_use]
    pub fn quantile(distribution: &wyrd_telemetry::timing::Distribution, q: f64, per: f64) -> Self {
        distribution.quantile(q).map_or_else(
            || Self::missing("no successful sample"),
            |bucket| Self {
                rank: Some(bucket.rank),
                ..Self::bounded(bucket.lowest as f64, bucket.highest as f64, per)
                    .with_samples(distribution.len())
            },
        )
    }

    /// Nearest-rank percentile `q` of exact raw `samples`, each divided by
    /// `per` (1e6 for nanoseconds to milliseconds), with the sample count;
    /// missing without samples. Dividing keeps a boundary exact: 500,000 ns
    /// is exactly 0.5 ms.
    #[must_use]
    pub fn percentile(samples: &[u64], q: f64, per: f64) -> Self {
        crate::capacity::nearest_rank(samples, q, per).map_or_else(
            || Self::missing("no successful sample"),
            |value| Self::exact(value).with_samples(samples.len() as u64),
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
    /// The memory sampling interval, milliseconds.
    pub memory_interval_ms: u64,
    /// Driver CPU-seconds, or `None` when unreadable.
    pub driver_cpu_seconds: Option<f64>,
    /// CPU-seconds of the timing collector helper process a schema-1
    /// record kept, in neither server nor driver CPU; never measured since
    /// server timing moved into the server's own histograms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collector_cpu_seconds: Option<f64>,
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

/// One server timing cohort's reconciliation: the observations a window
/// owed at one boundary, outcome, case, and phase, and those it recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reconciled {
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

/// The full server timing evidence of one window: the cumulative snapshots
/// it differences, whose buckets rebuild every judged distribution, and
/// its reconciliation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimingEvidence {
    /// The window's name within its repetition, such as `measured`.
    pub window: String,
    /// The snapshot at the window's open.
    pub before: wyrd_telemetry::timing::TimingSnapshot,
    /// The snapshot once the window's cohort drained.
    pub after: wyrd_telemetry::timing::TimingSnapshot,
    /// The window's expected and recorded observations.
    pub reconciliation: Vec<Reconciled>,
}

impl Repetition {
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
    /// [`wyrd_telemetry::timing::INVALID`], which the server counts.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Warmup => "warmup",
            Self::Repetition(0) => "repetition-1",
            Self::Repetition(1) => "repetition-2",
            Self::Repetition(2) => "repetition-3",
            Self::Repetition(_) => wyrd_telemetry::timing::INVALID,
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
}

/// Where a run keeps raw artifacts and whether it profiles.
#[derive(Debug, Clone)]
pub struct Options {
    /// Raw artifacts directory under `target/`.
    pub artifacts: PathBuf,
    /// The profiler to run with `--profile`, or `None` without it.
    pub profiler: Option<PathBuf>,
    /// Cancelled when the run is interrupted.
    pub interrupt: CancellationToken,
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
    /// The regression operands, when a baseline rule judged.
    pub operands: Option<Value>,
    /// PASS, FAIL, or INVALID; `None` for an advisory comparison or an
    /// inapplicable criterion.
    pub verdict: Option<Verdict>,
    /// Why it failed, was invalid, is advisory, or is inapplicable.
    pub reason: Option<String>,
    /// Whether the criterion could be judged at the run's controls: false
    /// only in a smoke, for a percentile under its sample floor or a
    /// full-run reference comparison. Records without it were judged.
    #[serde(default = "judged")]
    pub applicable: bool,
}

/// The applicability of a judgment read from a record that predates it.
const fn judged() -> bool {
    true
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
    /// Each metric's median, minimum, maximum, and spread.
    pub summary: BTreeMap<String, Summary>,
    /// Each applicable criterion's outcome.
    pub criteria: Vec<Judgment>,
    /// The step verdict.
    pub verdict: Verdict,
    /// Why the step failed or is invalid.
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

/// The criteria a record keeps: metric definitions and criteria with their
/// baseline snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CriteriaRecord {
    /// Every declared metric.
    pub metrics: Vec<Metric>,
    /// Every declared criterion.
    pub criteria: Vec<Criterion>,
}

/// One persisted experiment record: schema version [`SCHEMA_VERSION`] when
/// written, or a [`LEGACY_SCHEMA_VERSION`] record read back unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    /// [`SCHEMA_VERSION`], or [`LEGACY_SCHEMA_VERSION`] for a legacy read.
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
    /// Metric definitions and criteria with baseline snapshots.
    pub criteria: CriteriaRecord,
    /// Every declared step.
    pub steps: Vec<StepRecord>,
    /// Validity observations.
    pub validity: ValidityRecord,
    /// The overall verdict.
    pub verdict: Verdict,
    /// Every reason, run- and step-level.
    pub reasons: Vec<Reason>,
    /// Optional local artifacts.
    pub artifacts: Artifacts,
    /// Full or smoke; a record without one is a full run.
    #[serde(default)]
    pub mode: Mode,
    /// The smoke prepass a full run passed before its clock started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepass: Option<Prepass>,
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
        match bounded(deadline, &interrupt, workload.prepare()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                stopped = Some(Reason::new(
                    "workload.failed",
                    format!("preparation failed: {error}"),
                ));
            }
            Err(reason) => stopped = Some(reason),
        }
        let profiler = match (&options.profiler, stopped.is_none()) {
            (Some(program), true) => Profiler::start(
                program,
                &workload.processes(),
                &options.artifacts.join("profiles"),
            ),
            _ => Profiler::disabled(),
        };
        for step in &declaration.steps {
            if stopped.is_some()
                || step.kind != StepKind::Measured
                || !declaration.selected(&step.id)
            {
                continue;
            }
            let (repetitions, reasons) = measured.entry(step.id.clone()).or_default();
            let window = Duration::from_secs(timing.repetition_seconds);
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
                match bounded(deadline, &interrupt, workload.measure(step, phase, length)).await {
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
        }
        let saving = tokio::task::spawn_blocking(move || profiler.finish());
        match tokio::time::timeout(FINISH_GRACE, workload.finish()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => validity.push(Reason::new(
                "workload.failed",
                format!("drain or stop failed: {error}"),
            )),
            Err(_) => validity.push(Reason::new(
                "budget.exceeded",
                format!(
                    "drain and stop did not finish within {}s",
                    FINISH_GRACE.as_secs()
                ),
            )),
        }
        if let Some(reason) = stopped.take() {
            validity.push(reason);
        }
        host_after = host(sample).await;
        validity.extend(host_reasons(
            host_after.as_ref(),
            &declaration.validity,
            "after",
        ));
        worked = Some(clock.elapsed());
        profile = saving.await.unwrap_or_else(|_| ProfileNote {
            requested: true,
            notes: vec!["the profiler thread panicked".to_owned()],
            ..ProfileNote::default()
        });
    }
    let elapsed = worked.unwrap_or_else(|| clock.elapsed());
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
    let steps = judge(&declaration, measured, admitted);
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
    let verdict = steps
        .iter()
        .map(|step| step.verdict)
        .chain(reasons.iter().map(|reason| {
            if reason.invalidates() {
                Verdict::Invalid
            } else {
                Verdict::Fail
            }
        }))
        .max()
        .unwrap_or(Verdict::Invalid);
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
        prepass: declaration.prepass.clone(),
    }
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
    let timing = declaration.timing;
    for (repetitions, _) in measured.values_mut() {
        for repetition in repetitions.iter_mut() {
            repetition.derive(&declaration.metrics);
        }
    }
    let mut records: Vec<StepRecord> = Vec::new();
    for step in &declaration.steps {
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
                    repetition_reasons(repetition, &declaration.validity)
                        .into_iter()
                        .map(|reason| Reason {
                            message: format!("repetition {}: {}", index + 1, reason.message),
                            ..reason.at(&step.id)
                        }),
                );
            }
        }
        let summary = summarize(declaration, &repetitions);
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
                    &declaration.validity,
                    timing.repetitions,
                    declaration.mode,
                );
                if let Some(verdict) = judgment.verdict.filter(|verdict| *verdict != Verdict::Pass)
                {
                    let code = if verdict == Verdict::Fail {
                        "criterion.failed"
                    } else {
                        "criterion.invalid"
                    };
                    reasons.push(
                        Reason::new(code, judgment.reason.clone().unwrap_or_default())
                            .at(&step.id)
                            .of(&criterion.id),
                    );
                }
                criteria.push(judgment);
            }
        }
        let verdict = match status {
            Status::Omitted => Verdict::Pass,
            _ => reasons
                .iter()
                .map(|reason| {
                    if reason.invalidates() {
                        Verdict::Invalid
                    } else {
                        Verdict::Fail
                    }
                })
                .max()
                .unwrap_or(Verdict::Pass),
        };
        records.push(StepRecord {
            id: step.id.clone(),
            params,
            status,
            timing,
            repetitions,
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
/// missing or zero CPU, driver and collector saturation, and open-loop misses or
/// lateness.
fn repetition_reasons(repetition: &Repetition, validity: &Validity) -> Vec<Reason> {
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
        Some(driver) if repetition.seconds > 0.0 => {
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

/// Judges one criterion over a step's repetitions. In a smoke, a
/// full-run reference comparison, and a percentile whose every reading is
/// present but under its sample floor, are inapplicable with the observed
/// counts; a missing reading stays INVALID in both modes.
fn judge_criterion(
    criterion: &Criterion,
    repetitions: &[Repetition],
    validity: &Validity,
    expected: u8,
    mode: Mode,
) -> Judgment {
    let values: Vec<Reading> = repetitions
        .iter()
        .map(|rep| rep.reading(&criterion.metric))
        .collect();
    let mut judgment = Judgment {
        criterion: criterion.id.clone(),
        observed: Reading::median(&values),
        values: values.clone(),
        operands: None,
        verdict: Some(Verdict::Invalid),
        reason: None,
        applicable: true,
    };
    let inapplicable = |mut judgment: Judgment, reason: String| {
        judgment.verdict = None;
        judgment.applicable = false;
        judgment.reason = Some(reason);
        judgment
    };
    if mode == Mode::Smoke
        && matches!(
            criterion.rule,
            Rule::Regression { .. } | Rule::Advisory { .. } | Rule::Unusable { .. }
        )
    {
        return inapplicable(
            judgment,
            "not judgeable at smoke size: a full-run reference does not apply to the reduced smoke controls".to_owned(),
        );
    }
    if let Rule::Advisory { baseline, reason } = &criterion.rule {
        judgment.verdict = None;
        judgment.reason = Some(reason.clone());
        judgment.operands = regression_operands(&values, baseline.converted);
        return judgment;
    }
    if let Rule::Unusable { baseline, reason } = &criterion.rule {
        judgment.reason = Some(format!("baseline `{baseline}` is unusable: {reason}"));
        return judgment;
    }
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
    match &criterion.rule {
        Rule::Limit { op, limit, over } => {
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
                match op.judge(lo, open, hi, *limit) {
                    Some(true) => {}
                    Some(false) => {
                        if *over == Over::Every {
                            judgment.observed = (*reading).clone();
                            judgment.reason = Some(format!(
                                "repetition {} read {}, missing {} {}",
                                index + 1,
                                reading.cell(""),
                                op.symbol(),
                                number(*limit)
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
                            number(*limit)
                        ));
                        break;
                    }
                }
            }
            match outcome {
                Some(true) => judgment.verdict = Some(Verdict::Pass),
                Some(false) => {
                    judgment.verdict = Some(Verdict::Fail);
                    if judgment.reason.is_none() {
                        judgment.reason = Some(format!(
                            "median {} misses {} {}",
                            judgment.observed.cell(""),
                            op.symbol(),
                            number(*limit)
                        ));
                    }
                }
                None => {}
            }
        }
        Rule::Regression { baseline } => {
            let Some(direction) = criterion_direction(criterion, baseline) else {
                judgment.reason = Some("the metric declares no direction".to_owned());
                return judgment;
            };
            let Some(exact) = values
                .iter()
                .map(Reading::exact_value)
                .collect::<Option<Vec<f64>>>()
            else {
                judgment.reason = Some("a regression needs exact values".to_owned());
                return judgment;
            };
            let (failed, operands) = regression(&exact, baseline.converted, direction);
            judgment.operands = Some(operands);
            judgment.verdict = Some(if failed { Verdict::Fail } else { Verdict::Pass });
            if failed {
                judgment.reason = Some(format!(
                    "regressed against {} = {}",
                    baseline.id,
                    number(baseline.converted)
                ));
            }
        }
        Rule::Advisory { .. } | Rule::Unusable { .. } => {}
    }
    judgment
}

/// The direction stored with a regression criterion's snapshot unit:
/// per-CPU units improve upward, per-operation CPU downward.
fn criterion_direction(criterion: &Criterion, baseline: &Snapshot) -> Option<Direction> {
    let _ = criterion;
    if baseline.unit.contains("/cpu-s") {
        Some(Direction::Higher)
    } else if baseline.unit.starts_with("µs/") || baseline.unit.starts_with("ms/") {
        Some(Direction::Lower)
    } else {
        None
    }
}

/// The approved noise-aware regression rule over three values against
/// baseline `b`: higher-is-better fails only when `hi < 0.90b` and
/// `b - m > s`; lower-is-better only when `lo > 1.15b` and `m - b > s`.
/// Equality at either boundary passes. Returns whether it failed and the
/// operands.
#[must_use]
pub fn regression(values: &[f64], b: f64, direction: Direction) -> (bool, Value) {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let (lo, hi) = (
        sorted.first().copied().unwrap_or(f64::NAN),
        sorted.last().copied().unwrap_or(f64::NAN),
    );
    let m = sorted.get(sorted.len() / 2).copied().unwrap_or(f64::NAN);
    let s = hi - lo;
    let (percent, beyond, failed) = match direction {
        Direction::Higher => {
            let limit = b * 90.0 / 100.0;
            (hi < limit, b - m > s, hi < limit && b - m > s)
        }
        Direction::Lower => {
            let limit = b * 115.0 / 100.0;
            (lo > limit, m - b > s, lo > limit && m - b > s)
        }
    };
    let operands = serde_json::json!({
        "baseline": b, "median": m, "low": lo, "high": hi, "spread": s,
        "percentage_limit_missed": percent, "beyond_spread": beyond,
    });
    (failed, operands)
}

/// Advisory operands: the same comparison, never judged.
fn regression_operands(values: &[Reading], b: f64) -> Option<Value> {
    let exact: Option<Vec<f64>> = values.iter().map(Reading::exact_value).collect();
    exact
        .filter(|values| !values.is_empty())
        .map(|values| regression(&values, b, Direction::Higher).1)
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

    /// Publishes the record under `root/benchmarks/<engine>/<experiment>/`,
    /// or a smoke's under `root/target/benchmarks/smoke/<engine>/<experiment>/`
    /// outside committed history: written to a temporary file in that
    /// directory, synced, then linked to the first free collision-numbered
    /// name, so history is never overwritten and a partial file never
    /// appears complete.
    ///
    /// # Errors
    ///
    /// Returns [`ExperimentError::Persist`] when the directory, write, sync,
    /// or rename fails.
    pub fn publish(&self, root: &Path) -> Result<PathBuf, ExperimentError> {
        let history = match self.mode {
            Mode::Full => root.join("benchmarks"),
            Mode::Smoke => root.join(SMOKE_DIRECTORY),
        };
        let directory = history.join(self.engine.name()).join(&self.experiment);
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

    /// Reads a record file.
    ///
    /// # Errors
    ///
    /// Returns a read failure, [`ExperimentError::Unsupported`] for another
    /// schema version, or [`ExperimentError::Malformed`].
    pub fn read(path: &Path) -> Result<Self, ExperimentError> {
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
    /// Returns [`ExperimentError::Unsupported`] for a version other than 1
    /// or 2 and [`ExperimentError::Malformed`] for anything not a record of
    /// its version.
    pub fn parse(text: &str) -> Result<Self, ExperimentError> {
        let value: Value = serde_json::from_str(text)
            .map_err(|error| ExperimentError::Malformed(error.to_string()))?;
        match value.get("schema_version").and_then(Value::as_u64) {
            Some(SCHEMA_VERSION | LEGACY_SCHEMA_VERSION) => {}
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
            out.push_str("SMOKE: functional pipeline evidence at reduced controls, never a performance qualification or reference.\n\n");
        }
        if let Some(prepass) = &self.prepass {
            let _ = writeln!(
                out,
                "smoke prepass: exit {} in {:.1}s of its {}s budget, outside this run's clock; evidence under {}\n",
                prepass.exit_code, prepass.elapsed_seconds, prepass.budget_seconds, prepass.records
            );
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
                let verdict = match judgment.verdict {
                    Some(verdict) => match &judgment.reason {
                        Some(reason) if verdict != Verdict::Pass => {
                            format!("{}: {reason}", verdict.label())
                        }
                        _ => verdict.label().to_owned(),
                    },
                    None if !judgment.applicable => format!(
                        "— inapplicable: {}",
                        judgment.reason.as_deref().unwrap_or("not judgeable")
                    ),
                    None => format!(
                        "— advisory: {}",
                        judgment.reason.as_deref().unwrap_or("not judged")
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
            for (id, summary) in &step.summary {
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
        let profile = &self.artifacts.profile;
        if profile.requested {
            let _ = writeln!(
                out,
                "\nprofile: {} ({} captures){}",
                if profile.complete {
                    "complete"
                } else {
                    "incomplete"
                },
                profile.captures.len(),
                if profile.notes.is_empty() {
                    String::new()
                } else {
                    format!("; {}", profile.notes.join("; "))
                }
            );
        }
        let _ = writeln!(
            out,
            "\nverdict: {} (exit {})",
            self.verdict.label(),
            self.exit_code()
        );
        out
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
                        Some(Derive::PerVcpu { .. } | Derive::PerCpu { .. })
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
        let latency = ["p50_ms", "p95_ms", "p99_ms"]
            .iter()
            .map(|id| {
                median(id).map_or_else(
                    || "—".to_owned(),
                    |reading| reading.cell("").trim().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        let latency = if latency.iter().all(|cell| cell == "—") {
            "— no request latency for this step".to_owned()
        } else {
            latency.join(" / ")
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
                let cpu = |seconds: Option<f64>| {
                    seconds.map_or_else(
                        || "— unavailable".to_owned(),
                        |cpu| format!("{} s", number(cpu)),
                    )
                };
                match rep.collector_cpu_seconds {
                    Some(collector) => format!(
                        "{} (collector {} s)",
                        cpu(rep.driver_cpu_seconds),
                        number(collector)
                    ),
                    None => cpu(rep.driver_cpu_seconds),
                }
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
pub const REPORT_HEADER: &str = "| step | params | rate | p50/p95/p99 ms | errors | server CPU | peak mem | driver CPU | criterion | observed | verdict |";

/// Whether a baselines entry may judge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Eligibility {
    /// Clean, established source with measured server CPU.
    Eligible,
    /// Excluded; its reason says why.
    Excluded,
}

/// One historical reference in `benchmarks/<engine>/baselines.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineEntry {
    /// Stable id a criterion names.
    pub id: String,
    /// Eligibility.
    pub status: Eligibility,
    /// Why the entry is excluded or limited.
    pub reason: String,
    /// The historical experiment and step.
    pub step: String,
    /// Client count.
    pub clients: Option<u64>,
    /// The metric compared.
    pub metric: String,
    /// The value's unit.
    pub unit: String,
    /// Which way the metric improves.
    pub direction: Direction,
    /// The reference value.
    pub value: f64,
    /// The original printed operands.
    pub operands: String,
    /// The fixture the source measured.
    pub fixture: String,
    /// The source report.
    pub source_report: String,
    /// The source commit.
    pub source_commit: String,
    /// The source host fingerprint.
    pub host: String,
    /// Known build and limits context.
    pub build: String,
    /// Known limitations.
    pub limitations: String,
}

/// One engine's baselines file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Baselines {
    /// Every entry, eligible or excluded.
    #[serde(default)]
    pub baseline: Vec<BaselineEntry>,
}

impl Baselines {
    /// Reads `benchmarks/<engine>/baselines.toml` under `root`.
    ///
    /// # Errors
    ///
    /// Returns [`ExperimentError::Baselines`] when the file is unreadable or
    /// malformed.
    pub fn load(root: &Path, engine: Engine) -> Result<Self, ExperimentError> {
        let path = root
            .join("benchmarks")
            .join(engine.name())
            .join("baselines.toml");
        let text = std::fs::read_to_string(&path).map_err(|error| ExperimentError::Baselines {
            path: path.clone(),
            message: error.to_string(),
        })?;
        Self::parse(&text).map_err(|message| ExperimentError::Baselines { path, message })
    }

    /// Parses a baselines file.
    ///
    /// # Errors
    ///
    /// Returns the TOML parse error's message.
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|error| error.to_string())
    }

    /// Declares a baseline criterion `id` on `metric` for `step` against
    /// entry `baseline`, chosen before measurement: an eligible entry from
    /// the same host fingerprint and fixture judges; an eligible entry from
    /// another host or fixture is advisory with both fingerprints; a
    /// missing, excluded, or unit-incompatible entry is unusable and judges
    /// INVALID.
    #[must_use]
    pub fn criterion(
        &self,
        id: &str,
        metric: &Metric,
        step: &str,
        baseline: &str,
        host: &str,
        fixture: &str,
    ) -> Criterion {
        let rule = match self.baseline.iter().find(|entry| entry.id == baseline) {
            None => Rule::Unusable {
                baseline: baseline.to_owned(),
                reason: "no such entry in baselines.toml".to_owned(),
            },
            Some(entry) if entry.status == Eligibility::Excluded => Rule::Unusable {
                baseline: baseline.to_owned(),
                reason: format!("excluded: {}", entry.reason),
            },
            Some(entry) => match convert(entry.value, &entry.unit, &metric.unit) {
                None => Rule::Unusable {
                    baseline: baseline.to_owned(),
                    reason: format!("unit {} cannot convert to {}", entry.unit, metric.unit),
                },
                Some(converted) => {
                    let snapshot = Snapshot {
                        id: entry.id.clone(),
                        value: entry.value,
                        unit: entry.unit.clone(),
                        converted,
                        source_report: entry.source_report.clone(),
                        source_commit: entry.source_commit.clone(),
                        host: entry.host.clone(),
                        fixture: entry.fixture.clone(),
                        operands: entry.operands.clone(),
                    };
                    if entry.host != host {
                        Rule::Advisory {
                            baseline: snapshot,
                            reason: format!(
                                "cross-host: baseline host `{}`, this host `{host}`",
                                entry.host
                            ),
                        }
                    } else if entry.fixture != fixture {
                        Rule::Advisory {
                            baseline: snapshot,
                            reason: format!(
                                "fixture differs: baseline `{}`, this run `{fixture}`",
                                entry.fixture
                            ),
                        }
                    } else {
                        Rule::Regression { baseline: snapshot }
                    }
                }
            },
        };
        Criterion {
            id: id.to_owned(),
            metric: metric.id.clone(),
            steps: vec![step.to_owned()],
            rule,
            percentile: None,
        }
    }
}

/// Converts `value` from unit `from` to unit `to` when they are equal or
/// differ only by the numerator's time prefix (`µs/x` and `ms/x`) or byte
/// unit (`MB/x`, `MiB/x`, `GB/x`, with MB = 10^6, MiB = 2^20, GB = 10^9
/// bytes), so a historical decimal MB/cpu-s value converts to MiB/cpu-s by
/// × 10^6 / 2^20. Time and byte units never convert into each other.
#[must_use]
pub fn convert(value: f64, from: &str, to: &str) -> Option<f64> {
    if from == to {
        return Some(value);
    }
    let split = |unit: &str| {
        unit.split_once('/')
            .map(|(head, tail)| (head.to_owned(), tail.to_owned()))
    };
    let ((from_head, from_tail), (to_head, to_tail)) = (split(from)?, split(to)?);
    if from_tail != to_tail {
        return None;
    }
    let factor = |head: &str| match head {
        "s" => Some((1.0, "time")),
        "ms" => Some((1e-3, "time")),
        "µs" | "us" => Some((1e-6, "time")),
        "MB" => Some((1e6, "bytes")),
        "MiB" => Some((f64::from(1_u32 << 20), "bytes")),
        "GB" => Some((1e9, "bytes")),
        _ => None,
    };
    let ((from, from_kind), (to, to_kind)) = (factor(&from_head)?, factor(&to_head)?);
    (from_kind == to_kind).then(|| value * from / to)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, VecDeque};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;
    use serde_json::json;
    use tokio_util::sync::CancellationToken;

    use super::{
        Arrivals, Baselines, Criterion, Declaration, Derive, Direction, Eligibility, Engine,
        ExperimentError, HostRecord, Invocation, Level, Metric, Mode, Op, Options, Phase, Prepass,
        REPORT_HEADER, Reading, Record, Repetition, Rule, Source, Status, Step, Validity, Verdict,
        Workload, convert, regression, run,
    };
    use crate::capacity::{Histogram, Population, UNLABELLED};
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
            prepass: None,
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
            collector_cpu_seconds: Some(0.5),
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
        /// Real time the first measurement blocks, so an external profiler
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
            if let Some(settle) = self.settle.take() {
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
            profiler: None,
            interrupt: CancellationToken::new(),
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
        assert_eq!(valid.verdict, Verdict::Pass, "{:?}", valid.reasons);
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
            Verdict::Pass,
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
                Verdict::Pass,
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
                Verdict::Pass,
            ),
            (
                case(|r| r.set("p99_ms", Reading::exact(0.3).with_samples(999))),
                "criterion.invalid",
                Verdict::Invalid,
            ),
            (
                case(|r| r.set("p99_ms", Reading::exact(0.3).with_samples(1_000))),
                "",
                Verdict::Pass,
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
            record.steps[1].verdict,
            Verdict::Fail,
            "the valid miss stays visible"
        );
        assert!(
            codes(&record).contains(&"reading.missing")
                && codes(&record).contains(&"criterion.failed")
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
        assert_eq!(calls[1].2, Duration::from_secs(10));
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
        assert_eq!(record.verdict, Verdict::Pass);
        assert!(
            (record.elapsed_seconds - 2.0 * 35.0).abs() < 1e-6,
            "two steps of 5 s + 3 × 10 s"
        );

        let mut overrun = Scripted::always(&good());
        overrun.stall = Duration::from_secs(250);
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
        assert!(record.elapsed_seconds >= 900.0 && record.elapsed_seconds < 900.0 + 120.0);
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
            Verdict::Pass,
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
    /// reasons, never zero; host core metadata is never a denominator; units
    /// convert MB to MiB and time prefixes but never across kinds. Server
    /// timings recorded into the benchmark histograms and differenced
    /// between two snapshots give nearest-rank residual buckets, so p99s of
    /// 499,000, 500,000, and 501,000 ns judge PASS, ambiguous, FAIL under a
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

        assert_eq!(
            convert(1588.281, "µs/query", "ms/query").map(|v| (v * 1e6).round()),
            Some(1_588_281.0)
        );
        assert!(convert(2.0, "ms/query", "µs/query").is_some_and(|v| (v - 2000.0).abs() < 1e-9));
        assert!(
            convert(1.0, "MB/cpu-s", "MiB/cpu-s")
                .is_some_and(|v| (v - 1e6 / 1_048_576.0).abs() < 1e-12),
            "a historical MB baseline converts to MiB"
        );
        assert_eq!(
            convert(1.0, "MB/cpu-s", "ms/cpu-s"),
            None,
            "bytes never become time"
        );
        assert_eq!(convert(1.0, "µs/row", "µs/query"), None);

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

        wyrd_telemetry::timing::enable();
        let verifier = 2_000_000;
        let window = |requests: &[(u64, Option<u64>)], expected: u64| {
            let before = wyrd_telemetry::timing::snapshot();
            let request = tracing::Span::none();
            for (elapsed, excluded) in requests {
                wyrd_telemetry::record_timing(
                    &request,
                    "verification.direct",
                    "completed",
                    std::time::Duration::from_nanos(*elapsed),
                    excluded.map(std::time::Duration::from_nanos),
                );
            }
            let interval = wyrd_telemetry::timing::snapshot()
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
            let before = wyrd_telemetry::timing::snapshot();
            wyrd_telemetry::record_timing(
                &tracing::Span::none(),
                "verification.direct",
                "completed",
                std::time::Duration::ZERO,
                Some(std::time::Duration::ZERO),
            );
            wyrd_telemetry::timing::snapshot()
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
        overflowed[0].0 = wyrd_telemetry::timing::HIGHEST_NS * 2;
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

    /// V5: both regression directions, percentage and spread equality, the
    /// approved examples, strict SLO equality, noise never excusing an SLO
    /// miss, and correctness failing in only one repetition.
    ///
    /// # Panics
    ///
    /// Panics when a comparison or criterion judges against the rule.
    #[tokio::test(start_paused = true)]
    async fn criteria_distinguish_regression_noise_and_invalidity() {
        let fails = |values: [f64; 3], direction| regression(&values, 100.0, direction).0;
        assert!(
            fails([75.0, 80.0, 85.0], Direction::Higher),
            "the approved failing example"
        );
        assert!(
            !fails([70.0, 85.0, 100.0], Direction::Higher),
            "the approved passing example"
        );
        assert!(
            !fails([89.0, 89.0, 90.0], Direction::Higher),
            "hi at exactly 90% of b passes"
        );
        assert!(
            fails([80.0, 85.0, 89.0], Direction::Higher),
            "hi under 90% and b-m 15 beyond s 9"
        );
        assert!(
            !fails([69.0, 80.0, 89.0], Direction::Higher),
            "b-m equal to s passes"
        );
        assert!(fails([120.0, 125.0, 130.0], Direction::Lower));
        assert!(
            !fails([115.0, 125.0, 130.0], Direction::Lower),
            "lo at exactly 115% of b passes"
        );
        assert!(
            fails([116.0, 126.0, 126.0], Direction::Lower),
            "lo over 115% and m-b 26 beyond s 10"
        );
        assert!(
            !fails([116.0, 126.0, 142.0], Direction::Lower),
            "m-b equal to s passes"
        );
        let (_, operands) = regression(&[75.0, 80.0, 85.0], 100.0, Direction::Higher);
        assert_eq!(operands["median"], 80.0);
        assert_eq!(operands["spread"], 10.0);
        assert_eq!(operands["percentage_limit_missed"], true);
        assert_eq!(operands["beyond_spread"], true);

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
            record.verdict,
            Verdict::Fail,
            "p99 equal to 500 µs fails < 500 µs"
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
            record.verdict,
            Verdict::Fail,
            "spread never excuses an SLO miss"
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
            record.verdict,
            Verdict::Fail,
            "an error in one repetition fails correctness"
        );
        assert_eq!(record.steps[1].criteria[1].observed.value, Some(1.0));
    }

    /// The baselines this module's tests read: one eligible same-host entry
    /// in µs, one eligible cross-host entry, and one excluded entry.
    const BASELINES: &str = r#"
[[baseline]]
id = "point-1-cpu"
status = "eligible"
reason = "Eligible fallback"
step = "selective sweep"
clients = 1
metric = "cpu_ms_per_query"
unit = "µs/query"
direction = "lower"
value = 2000.0
operands = "rate 500, cores 1.0"
fixture = "standard-10m"
source_report = "read-standard-parquet.md"
source_commit = "cfe21dc90"
host = "test cpu, 32 CPU / 16 GiB, linux"
build = "release"
limitations = "single run"

[[baseline]]
id = "point-1-qps"
status = "eligible"
reason = "Eligible fallback"
step = "selective sweep"
clients = 1
metric = "queries_per_cpu_s"
unit = "queries/cpu-s"
direction = "higher"
value = 500.0
operands = "rate 500, cores 1.0"
fixture = "standard-10m"
source_report = "read-standard-parquet.md"
source_commit = "cfe21dc90"
host = "AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux"
build = "release"
limitations = "single run"

[[baseline]]
id = "direct-1"
status = "excluded"
reason = "historical clean state unestablished"
step = "1 direct verify 1"
clients = 1
metric = "ops_per_cpu_s"
unit = "verifications/cpu-s"
direction = "higher"
value = 4122.034
operands = "rate 4864, cores 1.18"
fixture = "synthetic"
source_report = "verification-capacity/report.md"
source_commit = "unknown"
host = "AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux"
build = "release"
limitations = "excluded"
"#;

    /// V6: selection happens at declaration: an eligible same-host entry
    /// judges after unit conversion, a cross-host entry is advisory with
    /// both fingerprints and never fails the run, and missing, excluded, and
    /// unit-incompatible entries are INVALID; the record keeps every
    /// snapshot.
    ///
    /// # Panics
    ///
    /// Panics when a selection, conversion, or recorded snapshot is wrong.
    #[tokio::test(start_paused = true)]
    async fn baseline_selection_preserves_eligibility() {
        let baselines = Baselines::parse(BASELINES).expect("baselines parse");
        let host = declaration().host.fingerprint;
        let cpu = Metric::derived(
            "cpu_ms_per_query",
            "ms/query",
            "C/W",
            Direction::Lower,
            Derive::CpuPer {
                work: "ops".to_owned(),
                scale: 1e3,
            },
        );
        let qps = Metric::derived(
            "queries_per_cpu_s",
            "queries/cpu-s",
            "W/C",
            Direction::Higher,
            Derive::PerCpu {
                work: "ops".to_owned(),
                scale: 1.0,
            },
        );
        let judging = baselines.criterion(
            "cpu-regression",
            &cpu,
            "op-1",
            "point-1-cpu",
            &host,
            "standard-10m",
        );
        let Rule::Regression { baseline } = &judging.rule else {
            panic!("{:?}", judging.rule)
        };
        assert!(
            (baseline.converted - 2.0).abs() < 1e-12,
            "2000 µs/query is 2 ms/query"
        );
        assert_eq!(
            (baseline.value, baseline.unit.as_str()),
            (2000.0, "µs/query"),
            "original operands kept"
        );
        let advisory = baselines.criterion(
            "qps-advisory",
            &qps,
            "op-1",
            "point-1-qps",
            &host,
            "standard-10m",
        );
        let Rule::Advisory { reason, .. } = &advisory.rule else {
            panic!("{:?}", advisory.rule)
        };
        assert!(
            reason.contains("AMD Ryzen 9 9950X") && reason.contains(&host),
            "both fingerprints"
        );
        let other_fixture =
            baselines.criterion("x", &cpu, "op-1", "point-1-cpu", &host, "writes-2m");
        assert!(matches!(other_fixture.rule, Rule::Advisory { .. }));
        let excluded =
            baselines.criterion("excluded", &qps, "op-1", "direct-1", &host, "synthetic");
        assert!(
            matches!(&excluded.rule, Rule::Unusable { reason, .. } if reason.contains("unestablished"))
        );
        let missing = baselines.criterion("missing", &qps, "op-1", "absent", &host, "synthetic");
        assert!(matches!(missing.rule, Rule::Unusable { .. }));
        let incompatible =
            baselines.criterion("units", &cpu, "op-1", "point-1-qps", &host, "standard-10m");
        assert!(
            matches!(&incompatible.rule, Rule::Unusable { reason, .. } if reason.contains("cannot convert"))
        );

        let directory = tempfile::tempdir().expect("artifacts");
        let options = options(directory.path());
        let mut declared = declaration();
        declared.metrics.extend([cpu, qps]);
        declared.criteria.extend([judging, advisory]);
        let mut workload = Scripted::always(&good());
        let record = execute(
            declared.clone(),
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(record.verdict, Verdict::Pass, "{:?}", record.reasons);
        let judged = &record.steps[0].criteria;
        assert_eq!(
            judged[1].verdict,
            Some(Verdict::Pass),
            "2 CPU-s per 1,000 ops is 2 ms/query"
        );
        assert!(judged[1].operands.is_some());
        assert_eq!(judged[2].verdict, None, "advisory never judges");
        assert_eq!(
            record.criteria.criteria, declared.criteria,
            "snapshots recorded as declared"
        );

        declared.criteria.push(excluded);
        let mut workload = Scripted::always(&good());
        let record = execute(
            declared,
            &mut workload,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(
            record.verdict,
            Verdict::Invalid,
            "a declared unusable baseline is INVALID"
        );
        assert!(codes(&record).contains(&"criterion.invalid"));

        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let committed = |engine| Baselines::load(&repository, engine).expect("committed baselines");
        let bifrost = committed(Engine::Bifrost);
        let eligible = |baselines: &Baselines| {
            baselines
                .baseline
                .iter()
                .filter(|entry| entry.status == Eligibility::Eligible)
                .count()
        };
        assert_eq!((bifrost.baseline.len(), eligible(&bifrost)), (247, 145));
        let selective = bifrost
            .baseline
            .iter()
            .find(|entry| entry.id == "standard-parquet.selective.c64.cpu-us-per-query")
            .expect("selective entry");
        assert_eq!(selective.status, Eligibility::Eligible);
        assert!((selective.value - 3_416.199).abs() < 1e-9);
        assert_eq!(selective.source_commit, "cfe21dc90");
        assert!(bifrost.baseline.iter().all(
            |entry| entry.status == Eligibility::Excluded || entry.source_commit == "cfe21dc90"
        ));
        for engine in [Engine::Verification, Engine::Gateway] {
            let baselines = committed(engine);
            assert!(!baselines.baseline.is_empty());
            assert_eq!(
                eligible(&baselines),
                0,
                "no {} baseline is eligible",
                engine.name()
            );
        }
    }

    /// V7: records round-trip, publish append-only with collision suffixes
    /// and no temporary residue, fail honestly when the directory cannot be
    /// written, refuse unsupported versions and malformed input, keep nulls
    /// for missing values, store the dirty digest, and never carry a secret
    /// configuration value.
    ///
    /// # Panics
    ///
    /// Panics when history is overwritten, a failure succeeds, a reader is
    /// permissive, or a sentinel leaks.
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
        assert!(first.starts_with(root.path().join("benchmarks/verification/direct-verify")));
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
        let future = text.replacen("\"schema_version\": 2", "\"schema_version\": 3", 1);
        assert!(matches!(
            Record::parse(&future),
            Err(ExperimentError::Unsupported(3))
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
    /// short warmup and one repetition each: a present percentile under its
    /// floor is inapplicable with its count, while an empty population and
    /// zero CPU stay INVALID; its record round-trips `mode: smoke` under
    /// `target/benchmarks/smoke/` and a failed write is still a persistence
    /// error. A full run refuses before preparation when its prepass failed
    /// and keeps a passing prepass beside its own clock; a record without a
    /// mode reads as full.
    ///
    /// # Panics
    ///
    /// Panics when a smoke skips or repeats work, waives an integrity check,
    /// judges a short percentile, lands in history, or a failed prepass
    /// admits preparation.
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
        assert_eq!(record.verdict, Verdict::Pass, "{:?}", record.reasons);
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
        assert!(!slo.applicable && slo.verdict.is_none());
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

        let prepass = |exit_code| Prepass {
            command: vec![
                "l1".to_owned(),
                "verification".to_owned(),
                "--smoke".to_owned(),
            ],
            budget_seconds: 600,
            elapsed_seconds: 42.0,
            exit_code,
            records: "target/benchmarks/smoke/verification/direct-verify".to_owned(),
        };
        let mut failed = declaration();
        failed.prepass = Some(prepass(2));
        let mut unprepared = Scripted::always(&good());
        let refused = execute(
            failed,
            &mut unprepared,
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(validity(&refused), ["prepass.failed"]);
        {
            let seen = unprepared.seen.lock().expect("seen");
            assert!(!seen.prepared && seen.calls.is_empty());
        }
        let mut passed = declaration();
        passed.prepass = Some(prepass(0));
        let admitted = execute(
            passed,
            &mut Scripted::always(&good()),
            [Some(quiet()), Some(quiet())],
            &options,
        )
        .await;
        assert_eq!(admitted.verdict, Verdict::Pass, "{:?}", admitted.reasons);
        assert_eq!(admitted.prepass, Some(prepass(0)));
        assert!(admitted.render().contains("smoke prepass: exit 0 in 42.0s"));

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
        use wyrd_telemetry::timing;

        timing::enable();
        let record = |micros: u64| {
            wyrd_telemetry::record_timing(
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
        let window = |after: &timing::TimingSnapshot| {
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
        use wyrd_telemetry::timing;

        timing::enable();
        let before = timing::snapshot();
        for (micros, outcome) in [(100, "success"), (300, "success"), (900, "error")] {
            wyrd_telemetry::record_timing(
                &tracing::Span::none(),
                "bifrost.query",
                outcome,
                Duration::from_micros(micros),
                None,
            );
        }
        let after = timing::snapshot();
        let judged = [("p50_ms", 0.5), ("p95_ms", 0.95), ("p99_ms", 0.99)];
        let reduce = |before: &timing::TimingSnapshot, after: timing::TimingSnapshot| {
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
            assert_eq!(rebuilt.reading(id), reading, "{id}");
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

    /// A written record is version 2 and names its histogram method and
    /// bucket bounds in the regenerated report; a legacy version-1 record
    /// reads and reports with its original raw-span method and collector
    /// CPU unchanged; any other version is refused.
    ///
    /// # Panics
    ///
    /// Panics when a version, method, bound, or legacy cell is lost.
    #[tokio::test(start_paused = true)]
    async fn histogram_records_preserve_method_and_legacy_reports() {
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
        bounded.collector_cpu_seconds = None;
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
        let read = Record::parse(&text).expect("a v2 record");
        assert_eq!(read.schema_version, super::SCHEMA_VERSION);
        let report = read.render();
        assert_eq!(report, record.render(), "the report regenerates");
        assert!(
            report.contains("timing (schema 2): server benchmark histograms"),
            "{report}"
        );
        assert!(report.contains("(0.399, 0.401] ms | PASS"), "{report}");
        assert!(!report.contains("collector"), "{report}");

        let mut legacy: serde_json::Value = serde_json::from_str(&text).expect("record json");
        legacy["schema_version"] = json!(super::LEGACY_SCHEMA_VERSION);
        legacy["controls"]["values"]["timing"]["method"] = json!("server wyrd.timing spans");
        for repetition in legacy["steps"][0]["repetitions"]
            .as_array_mut()
            .expect("repetitions")
        {
            repetition["collector_cpu_seconds"] = json!(0.5);
        }
        let legacy = Record::parse(&legacy.to_string()).expect("a legacy record");
        assert_eq!(legacy.schema_version, super::LEGACY_SCHEMA_VERSION);
        let report = legacy.render();
        assert!(
            report.contains("timing (schema 1): server wyrd.timing spans"),
            "{report}"
        );
        assert!(report.contains("(collector 0.5 s)"), "{report}");
        let reread = Record::parse(&serde_json::to_string(&legacy).expect("legacy json"))
            .expect("a legacy round trip");
        assert_eq!(reread, legacy, "a legacy record is never rewritten");
        let future = text.replacen("\"schema_version\": 2", "\"schema_version\": 3", 1);
        assert!(matches!(
            Record::parse(&future),
            Err(ExperimentError::Unsupported(3))
        ));
    }

    /// V8: a persisted multi-criterion record renders the exact common
    /// header, values with units, spread, reasons, and every verdict, and
    /// re-reading it after its raw artifacts are deleted renders the same
    /// report.
    ///
    /// # Panics
    ///
    /// Panics when the header, a cell, or the regenerated report differs.
    #[tokio::test(start_paused = true)]
    async fn reports_regenerate_from_persisted_records() {
        let root = tempfile::tempdir().expect("root");
        let artifacts = root.path().join("target/run");
        std::fs::create_dir_all(&artifacts).expect("artifacts");
        std::fs::write(artifacts.join("samples.jsonl"), "{}").expect("raw samples");
        let options = options(&artifacts);
        let mut slow = good();
        slow.set("p99_ms", Reading::exact(0.7).with_samples(2_000));
        let mut workload = Scripted::always(&good());
        workload.script = std::iter::repeat_n(Ok(good()), 4)
            .chain(std::iter::repeat_n(Ok(slow), 4))
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

        for (record, label) in [(&failed, "FAIL"), (&passed, "PASS"), (&invalid, "INVALID")] {
            let console = record.render();
            let path = record.publish(root.path()).expect("publish");
            std::fs::remove_dir_all(&artifacts).ok();
            let regenerated = Record::read(&path).expect("read").render();
            assert_eq!(
                console, regenerated,
                "report regenerates from history alone"
            );
            assert!(
                console.lines().any(|line| line == REPORT_HEADER),
                "exact header"
            );
            assert!(console.contains(&format!("verdict: {label} (exit {})", record.exit_code())));
        }
        let report = failed.render();
        assert!(
            report.contains("| op-64 | clients=64 | 100 ops/s; 500 ops/cpu-s | — / — / 0.7 |"),
            "{report}"
        );
        assert!(report.contains("2 s / 0.2 cores"));
        assert!(report.contains("512 MiB (sampled 10 ms)"));
        assert!(
            report.contains(
                "overhead-slo: p99_ms < 0.5 ms | 0.7 ms spread 0 | FAIL: median 0.7 misses < 0.5 |"
            ),
            "{report}"
        );
        assert!(report.contains("- criterion.failed op-64/overhead-slo"));
        assert!(invalid.render().contains("- host.busy"));
        assert!(
            invalid.render().contains("— the run admitted no workload")
                || invalid.render().contains("| INVALID |")
        );
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
        let saving = root.path().join("saving-profiler");
        std::fs::write(
            &saving,
            "#!/bin/sh\nout=''\nwhile [ $# -gt 0 ]; do [ \"$1\" = --output ] && out=\"$2\"; shift; done\n\
             trap 'echo profile > \"$out\"; exit 0' INT\nwhile :; do sleep 0.02; done\n",
        )
        .expect("script");
        let silent = root.path().join("silent-profiler");
        std::fs::write(
            &silent,
            "#!/bin/sh\ntrap 'exit 0' INT\nwhile :; do sleep 0.02; done\n",
        )
        .expect("script");
        for script in [&saving, &silent] {
            let mut permissions = std::fs::metadata(script).expect("metadata").permissions();
            std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
            std::fs::set_permissions(script, permissions).expect("chmod");
        }
        let profiled = |program: Option<PathBuf>| Options {
            artifacts: root.path().join("target"),
            profiler: program,
            interrupt: CancellationToken::new(),
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
                assert_eq!(profile.captures.len(), 1);
                assert!(record.render().contains("profile: complete (1 captures)"));
            } else {
                assert!(
                    profile.notes.iter().any(|n| n.contains(note)),
                    "{note}: {profile:?}"
                );
                assert!(record.render().contains("profile: incomplete"));
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
        let slow = root.path().join("slow-profiler");
        std::fs::write(
            &slow,
            "#!/bin/sh\nout=''\nwhile [ $# -gt 0 ]; do [ \"$1\" = --output ] && out=\"$2\"; shift; done\n\
             trap 'sleep 1; echo profile > \"$out\"; exit 0' INT\nwhile :; do sleep 0.02; done\n",
        )
        .expect("script");
        let mut permissions = std::fs::metadata(&slow).expect("metadata").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(&slow, permissions).expect("chmod");
        let artifacts = root.path().join("target");
        let capture = artifacts
            .join("profiles")
            .join(format!("profile-{}.json.gz", std::process::id()));
        let run_with = |profiler: Option<PathBuf>, stall: Duration| {
            let (artifacts, capture) = (artifacts.clone(), capture.clone());
            async move {
                let mut workload = Scripted::always(&good());
                workload.settle = Some(Duration::from_millis(500));
                workload.stall = stall;
                workload.watch = Some(capture);
                let options = Options {
                    artifacts,
                    profiler,
                    interrupt: CancellationToken::new(),
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
}
