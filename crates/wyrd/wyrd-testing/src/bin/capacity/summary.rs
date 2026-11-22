//! The L1 case protocol's persisted records and their deterministic
//! reports.
//!
//! Each attempted case keeps one [`CaseRecord`]: its identity, the engine
//! [`Record`] of exactly that case (windows, aggregate, criteria, lifecycle
//! and profile evidence), and its cleanup evidence. A sweep keeps one
//! [`SweepRecord`] beside its case records: the invocation, mode, ordered
//! selection, and each case's outcome with its record reference or the
//! reason it has none. [`render_sweep`] draws the category table from those
//! saved files alone, so `capacity report <sweep.json>` reproduces the table
//! the sweep printed, and [`render_case`] draws a case's full report.
//!
//! Both records carry [`PROTOCOL_VERSION`] and no `schema_version`, so a
//! reader of the earlier schema-3 experiment records refuses them instead of
//! reading new evidence as old.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use wyrd_testing::experiment::{
    Over, Reading, Record, Repetition, Status, TimingEvidence, Verdict,
};
use wyrd_testing::profile::ProfileMode;

use crate::cases::{Case, Category};

/// The protocol id a case record carries.
pub const CASE_PROTOCOL: &str = "wyrd-l1-case";

/// The protocol id a sweep record carries.
pub const SWEEP_PROTOCOL: &str = "wyrd-l1-sweep";

/// The L1 case protocol version both records carry.
pub const PROTOCOL_VERSION: u64 = 1;

/// The file a sweep keeps its record in, beside its case records.
pub const SWEEP_FILE: &str = "sweep.json";

/// How a sweep measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SweepMode {
    /// The standardized performance clock.
    Standard,
    /// The standard clock with profilers attached: readings marked
    /// diagnostic.
    Diagnostic,
    /// The harness smoke: short windows and small fixtures, judging
    /// workflow and evidence integrity only.
    Smoke,
}

impl SweepMode {
    /// The mode's label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Diagnostic => "diagnostic",
            Self::Smoke => "smoke",
        }
    }
}

/// What a case's teardown proved about the resources it owned.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cleanup {
    /// The PostgreSQL Compose project the case owned: its ownership id.
    pub project: String,
    /// The case's own temporary root, holding its server storage and WAL.
    pub storage_root: String,
    /// Whether every owned process, container, volume and storage root was
    /// confirmed gone within the teardown allowance.
    pub confirmed: bool,
    /// Seconds the supervisor spent stopping and confirming, after the case
    /// process kept its record.
    pub seconds: f64,
    /// What was found before the case started: it must be nothing.
    pub preexisting: Vec<String>,
    /// What still existed when confirmation ended.
    pub surviving: Vec<String>,
}

/// One attempted case's full record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseRecord {
    /// [`CASE_PROTOCOL`].
    pub protocol: String,
    /// [`PROTOCOL_VERSION`].
    pub protocol_version: u64,
    /// The category command.
    pub category: String,
    /// The case's family.
    pub family: String,
    /// The case id within its family.
    pub case: String,
    /// The exact selector.
    pub selector: String,
    /// The sweep mode.
    pub mode: SweepMode,
    /// The profile mode, when profiled.
    pub profile: Option<ProfileMode>,
    /// When the case clock started.
    pub started_at: DateTime<Utc>,
    /// The engine record of exactly this case, or `None` when the case
    /// process kept none.
    pub record: Option<Record>,
    /// Each measured repetition's window snapshots by step id, in
    /// repetition order: the evidence the case process kept in its
    /// temporary artifacts, which the engine record never carries.
    /// Reading the case record puts them back on its repetitions.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub timings: BTreeMap<String, Vec<Vec<TimingEvidence>>>,
    /// Teardown evidence.
    pub cleanup: Cleanup,
    /// VALID only when the engine record is VALID, every phase fit its
    /// allowance, and cleanup was confirmed.
    pub verdict: Verdict,
    /// Every reason the case is INVALID.
    pub reasons: Vec<String>,
}

impl CaseRecord {
    /// Moves each measured repetition's window snapshots out of `record`,
    /// by step id, for [`Self::timings`].
    #[must_use]
    pub fn timings_of(record: &mut Record) -> BTreeMap<String, Vec<Vec<TimingEvidence>>> {
        record
            .steps
            .iter_mut()
            .filter(|step| {
                step.repetitions
                    .iter()
                    .any(|repetition| !repetition.timings.is_empty())
            })
            .map(|step| {
                let windows = step
                    .repetitions
                    .iter_mut()
                    .map(|repetition| std::mem::take(&mut repetition.timings))
                    .collect();
                (step.id.clone(), windows)
            })
            .collect()
    }

    /// Moves the saved [`Self::timings`] back onto the engine record's
    /// repetitions, so its full report lists every window series.
    fn hydrate(&mut self) {
        let Some(record) = self.record.as_mut() else {
            return;
        };
        for step in &mut record.steps {
            let Some(windows) = self.timings.remove(&step.id) else {
                continue;
            };
            for (repetition, timings) in step.repetitions.iter_mut().zip(windows) {
                repetition.timings = timings;
            }
        }
    }

    /// Sets the engine record's persistence time from the entry naming the
    /// case file at `path` in the sweep record beside it: keeping a case
    /// record is timed only once it is written, so its sweep entry holds
    /// that time. It stays unavailable without such an entry.
    fn persisted(&mut self, path: &Path) {
        let name = path.file_name().and_then(|name| name.to_str());
        let sweep = path.parent().map(|directory| directory.join(SWEEP_FILE));
        if let (Some(name), Some(sweep)) = (name, sweep)
            && let Ok(text) = std::fs::read_to_string(sweep)
            && let Ok(sweep) = serde_json::from_str::<SweepRecord>(&text)
            && let Some(entry) = sweep
                .cases
                .iter()
                .find(|entry| entry.record.as_deref() == Some(name))
            && let Some(lifecycle) = self
                .record
                .as_mut()
                .and_then(|record| record.lifecycle.as_mut())
        {
            lifecycle.persist_seconds = entry.persist_seconds;
        }
    }
}

/// A case's outcome in its sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    /// Measured what it claims.
    Valid,
    /// Attempted, and its measurement or evidence is invalid.
    Invalid,
    /// Selected, never started.
    NotRun,
}

impl CaseStatus {
    /// The table label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Valid => "VALID",
            Self::Invalid => "INVALID",
            Self::NotRun => "NOT RUN",
        }
    }
}

/// One selected case of a sweep.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SweepEntry {
    /// The exact selector.
    pub selector: String,
    /// The family.
    pub family: String,
    /// The case id.
    pub case: String,
    /// The outcome.
    pub status: CaseStatus,
    /// The case record's file name beside the sweep record, or `None` when
    /// it was never started or could not be kept.
    pub record: Option<String>,
    /// Why it is INVALID or NOT RUN.
    pub reasons: Vec<String>,
    /// Seconds keeping the case record took, measured once it was written
    /// and so kept here rather than in the case record itself; `None` when
    /// it was never kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persist_seconds: Option<f64>,
}

/// One sweep's record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SweepRecord {
    /// [`SWEEP_PROTOCOL`].
    pub protocol: String,
    /// [`PROTOCOL_VERSION`].
    pub protocol_version: u64,
    /// The category.
    pub category: String,
    /// The invocation, program name first.
    pub invocation: Vec<String>,
    /// The mode.
    pub mode: SweepMode,
    /// The profile mode, when profiled.
    pub profile: Option<ProfileMode>,
    /// Whether `--cases` chose the selection, rather than the routine
    /// sweep.
    pub explicit: bool,
    /// When the sweep started.
    pub started_at: DateTime<Utc>,
    /// When its last case ended.
    pub finished_at: DateTime<Utc>,
    /// Every selected case, in run order.
    pub cases: Vec<SweepEntry>,
}

impl SweepRecord {
    /// 0 when every selected case is VALID, else 2.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        if self
            .cases
            .iter()
            .all(|entry| entry.status == CaseStatus::Valid)
        {
            0
        } else {
            2
        }
    }
}

/// A saved L1 protocol file.
#[derive(Debug)]
pub enum Saved {
    /// A sweep record.
    Sweep(SweepRecord),
    /// A case record.
    Case(Box<CaseRecord>),
}

/// Reads `text` as an L1 protocol record, or `None` when it is no protocol
/// record, such as an earlier schema-3 experiment record.
///
/// # Errors
///
/// Returns a message for a protocol record of another version or shape.
pub fn parse(text: &str) -> Result<Option<Saved>, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let protocol = value.get("protocol").and_then(serde_json::Value::as_str);
    if protocol.is_none() {
        return Ok(None);
    }
    match value
        .get("protocol_version")
        .and_then(serde_json::Value::as_u64)
    {
        Some(PROTOCOL_VERSION) => {}
        other => {
            return Err(format!(
                "L1 protocol version {other:?} is not supported; this reader reads {PROTOCOL_VERSION}"
            ));
        }
    }
    match protocol {
        Some(SWEEP_PROTOCOL) => serde_json::from_value(value)
            .map(|sweep| Some(Saved::Sweep(sweep)))
            .map_err(|error| format!("malformed sweep record: {error}")),
        Some(CASE_PROTOCOL) => serde_json::from_value::<CaseRecord>(value)
            .map(|mut case| {
                case.hydrate();
                Some(Saved::Case(Box::new(case)))
            })
            .map_err(|error| format!("malformed case record: {error}")),
        Some(other) => Err(format!("unknown record protocol `{other}`")),
        None => Ok(None),
    }
}

/// What a category's table reads from a case's aggregate.
struct Headline {
    /// The raw work reading summed over the windows.
    work: &'static str,
    /// The throughput unit.
    unit: &'static str,
    /// The CPU unit noun, as in `µs/<noun>`.
    noun: &'static str,
    /// The pooled latency reading.
    latency: &'static str,
    /// The latency label.
    label: &'static str,
    /// Whether the row has an OTLP input column reading.
    input: Input,
}

/// A case's Input column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Input {
    /// Not a Bifrost category: no column.
    None,
    /// A Bifrost case without ingest input: inapplicable.
    Inapplicable,
    /// A native write: input bytes are not measured.
    Unmeasured,
    /// An OTLP write: uncompressed protobuf input.
    Otlp,
}

impl Headline {
    /// The readings `case`'s row shows, from the summary design.
    fn of(category: Category, family: &str, case: &str) -> Self {
        match (category, family) {
            (Category::Verification, _) => Self {
                work: "verifications",
                unit: "verifications/s",
                noun: "verification",
                latency: "overhead_p99_ms",
                label: "server overhead p99",
                input: Input::None,
            },
            (Category::Gateway, _) => Self {
                work: "requests",
                unit: "requests/s",
                noun: "request",
                latency: "overhead_p99_ms",
                label: "added overhead p99",
                input: Input::None,
            },
            (Category::Bifrost, "writes-parquet") => Self {
                work: "rows",
                unit: "rows/s",
                noun: "row",
                latency: "p99_ms",
                label: "durable ACK p99",
                input: if case.starts_with("native-write") {
                    Input::Unmeasured
                } else {
                    Input::Otlp
                },
            },
            (Category::Bifrost, family) if family.starts_with("ui-sql") => Self {
                work: "requests",
                unit: "pages/s",
                noun: "page",
                latency: "client_p99_ms",
                label: "client page p99",
                input: Input::Inapplicable,
            },
            (Category::Bifrost, _) if case.starts_with("full-scan") => Self {
                work: "queries",
                unit: "queries/s",
                noun: "query",
                latency: "p95_ms",
                label: "server query p95",
                input: Input::Inapplicable,
            },
            (Category::Bifrost, _) => Self {
                work: "queries",
                unit: "queries/s",
                noun: "query",
                latency: "p99_ms",
                label: "server query p99",
                input: Input::Inapplicable,
            },
        }
    }
}

/// Bytes per MiB.
const MIB: f64 = 1_048_576.0;

/// An exact value with fixed `decimals` and `unit`, or why it is
/// unavailable; never a zero for a missing reading.
fn fixed(value: Result<f64, String>, decimals: usize, unit: &str) -> String {
    match value {
        Ok(value) => format!("{value:.decimals$} {unit}"),
        Err(reason) => format!("unavailable: {reason}"),
    }
}

/// The exact value of `reading`, or why there is none.
fn exact(reading: &Reading) -> Result<f64, String> {
    match (reading.value, reading.above) {
        (Some(value), None) => Ok(value),
        _ => Err(reading
            .reason
            .clone()
            .unwrap_or_else(|| "not exact".to_owned())),
    }
}

/// A percentile as its bucket interval in ms with three decimals and its
/// sample count, or why it is unavailable.
fn interval(reading: &Reading) -> String {
    let samples = reading
        .samples
        .map_or_else(String::new, |samples| format!(", n={samples}"));
    match (reading.above, reading.value) {
        (Some(above), Some(value)) => format!("({above:.3}, {value:.3}] ms{samples}"),
        (Some(above), None) => format!("> {above:.3} ms{samples}"),
        (None, Some(value)) => format!("{value:.3} ms{samples}"),
        (None, None) => format!(
            "unavailable: {}",
            reading.reason.as_deref().unwrap_or("not measured")
        ),
    }
}

/// The aggregate of a case record's one measured step.
fn aggregate(record: &Record) -> Option<&Repetition> {
    record
        .steps
        .iter()
        .find(|step| step.status == Status::Measured)
        .and_then(|step| step.aggregate.as_ref())
}

/// The performance cells of a VALID case's row: throughput, CPU per unit,
/// headline latency, input when the category has the column, and target.
fn cells(category: Category, case: &CaseRecord) -> Vec<String> {
    let headline = Headline::of(category, &case.family, &case.case);
    let width = if headline.input == Input::None { 4 } else { 5 };
    let Some(record) = &case.record else {
        return vec!["unavailable: no engine record".to_owned(); width];
    };
    let Some(aggregate) = aggregate(record) else {
        return vec!["unavailable: no case aggregate".to_owned(); width];
    };
    let work = exact(&aggregate.reading(headline.work));
    let cpu = aggregate
        .server_cpu_seconds
        .ok_or_else(|| "server CPU-seconds are unavailable".to_owned());
    let seconds = Some(aggregate.seconds)
        .filter(|seconds| *seconds > 0.0)
        .ok_or_else(|| "no measured seconds".to_owned());
    let throughput = work
        .clone()
        .and_then(|work| seconds.clone().map(|seconds| work / seconds));
    let per_unit = work.clone().and_then(|work| {
        if work > 0.0 {
            cpu.clone().map(|cpu| cpu / work * 1e6)
        } else {
            Err(format!("no {} completed", headline.noun))
        }
    });
    let mut row = vec![
        fixed(throughput, 1, headline.unit),
        fixed(per_unit, 2, &format!("µs/{}", headline.noun)),
        format!(
            "{}: {}",
            headline.label,
            interval(&aggregate.reading(headline.latency))
        ),
    ];
    match headline.input {
        Input::None => {}
        Input::Inapplicable => row.push("n/a".to_owned()),
        Input::Unmeasured => {
            row.push("unavailable: native input bytes are not measured".to_owned());
        }
        Input::Otlp => {
            let mib = exact(&aggregate.reading("wire_bytes")).map(|bytes| bytes / MIB);
            let rate = mib
                .clone()
                .and_then(|mib| seconds.clone().map(|seconds| mib / seconds));
            let efficiency = mib.and_then(|mib| {
                cpu.clone().and_then(|cpu| {
                    if cpu > 0.0 {
                        Ok(mib / cpu)
                    } else {
                        Err("server CPU-seconds were zero".to_owned())
                    }
                })
            });
            row.push(format!(
                "{}; {}",
                fixed(rate, 2, "MiB/s"),
                fixed(efficiency, 2, "MiB/CPU-s")
            ));
        }
    }
    row.push(target(record));
    row
}

/// The Target cell: each applicable performance target of the case's step,
/// judged on its aggregate, as `id: outcome`; `none` when it has none.
/// Previous performance targets are inapplicable to L1 cases, so a case
/// declaring one still reads `none` rather than implying it applies.
fn target(record: &Record) -> String {
    let targets: Vec<String> = record
        .steps
        .iter()
        .filter(|step| step.status == Status::Measured)
        .flat_map(|step| &step.criteria)
        .filter(|judgment| {
            judgment.applicable
                && record.criteria.criteria.iter().any(|criterion| {
                    criterion.id == judgment.criterion && criterion.rule.over == Over::Median
                })
        })
        .filter_map(|judgment| {
            judgment
                .outcome
                .map(|outcome| format!("{}: {}", judgment.criterion, outcome.label()))
        })
        .collect();
    if targets.is_empty() {
        "none".to_owned()
    } else {
        targets.join("; ")
    }
}

/// Escapes a table cell.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

/// The sweep's Markdown summary: coverage, mode and `path`, then one row
/// per selected case in saved order. `read` loads a case record by its
/// file name beside the sweep. Invalid and unstarted rows give their reason
/// and withhold performance cells; a profiled sweep marks every reading
/// diagnostic. Derived Bifrost readings follow, each naming the inputs it
/// lacks.
#[must_use]
pub fn render_sweep(
    sweep: &SweepRecord,
    path: &str,
    read: &dyn Fn(&str) -> Result<CaseRecord, String>,
) -> String {
    let category = Category::parse(&sweep.category);
    let routine = category.map_or(0, |category| category.routine().len());
    let coverage = if sweep.explicit {
        format!("targeted selection of {} cases", sweep.cases.len())
    } else {
        format!(
            "routine sweep, {} of {routine} routine cases",
            sweep.cases.len()
        )
    };
    let mode = match (sweep.mode, sweep.profile) {
        (SweepMode::Diagnostic, Some(profile)) => {
            format!(
                "diagnostic (profile {}; readings are diagnostic)",
                profile.name()
            )
        }
        (mode, _) => mode.label().to_owned(),
    };
    let input = category == Some(Category::Bifrost);
    let mut out = format!(
        "L1 {} sweep\n\nCoverage: {coverage}\nMode: {mode}\nSweep: {path}\n\n",
        sweep.category
    );
    out.push_str("| Case | Status | Throughput | Server CPU per unit | Headline latency |");
    if input {
        out.push_str(" Input |");
    }
    out.push_str(" Target | Record |\n|---|---|---|---|---|");
    if input {
        out.push_str("---|");
    }
    out.push_str("---|---|\n");
    let mut valid = Vec::new();
    for entry in &sweep.cases {
        let read = entry.record.as_deref().map(read);
        let record_cell = match (&entry.record, &read) {
            (Some(name), Some(Ok(_))) => name.clone(),
            (Some(name), Some(Err(error))) => format!("{name} (unreadable: {error})"),
            _ => "unavailable".to_owned(),
        };
        let reasons = entry.reasons.join("; ");
        let (status, performance) = match (entry.status, &read) {
            (CaseStatus::Valid, Some(Ok(case))) => {
                let label = if sweep.mode == SweepMode::Diagnostic {
                    "VALID (diagnostic)".to_owned()
                } else {
                    "VALID".to_owned()
                };
                valid.push(case.clone());
                (label, category.map(|category| cells(category, case)))
            }
            (CaseStatus::Valid, _) => ("INVALID: the case record cannot be read".to_owned(), None),
            (status, _) => (format!("{}: {reasons}", status.label()), None),
        };
        let width = if input { 5 } else { 4 };
        let performance = performance.unwrap_or_else(|| vec!["—".to_owned(); width]);
        let _ = write!(out, "| {} | {} |", cell(&entry.selector), cell(&status));
        for value in performance {
            let _ = write!(out, " {} |", cell(&value));
        }
        let _ = writeln!(out, " {} |", cell(&record_cell));
    }
    if category == Some(Category::Bifrost) {
        let derived = derived(sweep, &valid);
        if !derived.is_empty() {
            out.push_str("\nDerived readings:\n\n");
            for line in derived {
                let _ = writeln!(out, "- {line}");
            }
        }
    }
    out
}

/// The Bifrost readings derived from two cases of the sweep, for each one
/// whose inputs the sweep selected at least in part: each write path's
/// peak acknowledged rows/s and each lookup's 64-client rate scaling. A
/// missing, unselected or invalid input makes the reading incomplete and
/// names it; nothing derived ever starts a case.
fn derived(sweep: &SweepRecord, valid: &[CaseRecord]) -> Vec<String> {
    let rate = |family: &str, id: &str, work: &str| -> Result<f64, String> {
        let case = valid
            .iter()
            .find(|case| case.family == family && case.case == id)
            .ok_or_else(|| {
                if sweep
                    .cases
                    .iter()
                    .any(|entry| entry.family == family && entry.case == id)
                {
                    format!("`{id}` is not VALID")
                } else {
                    format!("`{id}` was not selected")
                }
            })?;
        let aggregate = case
            .record
            .as_ref()
            .and_then(aggregate)
            .ok_or_else(|| format!("`{id}` has no aggregate"))?;
        exact(&aggregate.reading(work)).map(|work| work / aggregate.seconds)
    };
    let selected = |family: &str, ids: &[&str]| {
        sweep
            .cases
            .iter()
            .any(|entry| entry.family == family && ids.contains(&entry.case.as_str()))
    };
    let mut lines = Vec::new();
    for op in ["otlp-spans", "otlp-spans-1k", "otlp-logs", "native-write"] {
        let ids = [format!("{op}-c1"), format!("{op}-c64")];
        let names = [ids[0].as_str(), ids[1].as_str()];
        if !selected("writes-parquet", &names) {
            continue;
        }
        let readings: Vec<Result<f64, String>> = names
            .iter()
            .map(|id| rate("writes-parquet", id, "rows"))
            .collect();
        let line = match readings
            .iter()
            .cloned()
            .collect::<Result<Vec<f64>, String>>()
        {
            Ok(values) => {
                let (index, best) = values.iter().copied().enumerate().fold(
                    (0, f64::MIN),
                    |best, (index, value)| {
                        if value > best.1 { (index, value) } else { best }
                    },
                );
                format!("{op}-peak: {best:.1} rows/s from `{}`", names[index])
            }
            Err(_) => format!(
                "{op}-peak: incomplete: {}",
                readings
                    .iter()
                    .filter_map(|reading| reading.clone().err())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        };
        lines.push(line);
    }
    for (family, name, base, scaled) in [
        (
            "hot",
            "point-memtable-scaling",
            "point-memtable-c1",
            "point-memtable-c64",
        ),
        ("parquet", "point-scaling", "point-c1", "point-c64"),
        (
            "parquet",
            "span-trace-scaling",
            "span-trace-c1",
            "span-trace-c64",
        ),
    ] {
        if !selected(family, &[base, scaled]) {
            continue;
        }
        let pair = (
            rate(family, base, "queries"),
            rate(family, scaled, "queries"),
        );
        lines.push(match pair {
            (Ok(base), Ok(scaled)) if base > 0.0 => format!(
                "{name}: {:.3} (Q64 / (64 × Q1)) from `{scaled}` and `{base}`",
                scaled / (64.0 * base)
            ),
            (base, scaled) => format!(
                "{name}: incomplete: {}",
                [base.err(), scaled.err()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        });
    }
    lines
}

/// One case's full report: identity, verdict and reasons, phase timings
/// against their allowances, ownership and cleanup evidence, the windows
/// and their aggregate, profile references, then the engine record's
/// report.
#[must_use]
pub fn render_case(case: &CaseRecord) -> String {
    let mut out = format!(
        "L1 case {} ({} / {}), protocol {} v{}\nMode: {}{}\nStarted: {}\nVerdict: {}\n",
        case.selector,
        case.category,
        case.family,
        case.protocol,
        case.protocol_version,
        case.mode.label(),
        case.profile.map_or_else(String::new, |profile| format!(
            " (profile {}; readings are diagnostic)",
            profile.name()
        )),
        case.started_at.to_rfc3339(),
        case.verdict.label(),
    );
    for reason in &case.reasons {
        let _ = writeln!(out, "- {reason}");
    }
    let cleanup = &case.cleanup;
    let _ = write!(
        out,
        "\nOwnership: PostgreSQL project `{}`, storage root `{}`\nCleanup: {} in {:.1}s\n",
        cleanup.project,
        cleanup.storage_root,
        if cleanup.confirmed {
            "confirmed"
        } else {
            "NOT confirmed"
        },
        cleanup.seconds
    );
    for (what, items) in [
        ("preexisting", &cleanup.preexisting),
        ("surviving", &cleanup.surviving),
    ] {
        for item in items {
            let _ = writeln!(out, "- {what}: {item}");
        }
    }
    let Some(record) = &case.record else {
        out.push_str("\nNo engine record was kept.\n");
        return out;
    };
    if let Some(lifecycle) = &record.lifecycle {
        let a = lifecycle.allowances;
        out.push_str("\n| Phase | Seconds | Allowance |\n|---|---:|---:|\n");
        for (phase, used, allowed) in [
            ("setup", Some(lifecycle.setup_seconds), a.setup_seconds),
            ("warmup", Some(lifecycle.warmup_seconds), a.warmup_seconds),
            (
                "measurement",
                Some(lifecycle.measurement_seconds),
                a.measurement_seconds,
            ),
            (
                "drain and capture",
                Some(lifecycle.drain_seconds),
                a.drain_seconds,
            ),
            (
                "teardown",
                Some(lifecycle.teardown_seconds),
                a.teardown_seconds,
            ),
            (
                "persist and report",
                lifecycle.persist_seconds,
                a.persist_seconds,
            ),
        ] {
            let used = used.map_or_else(|| "unavailable".to_owned(), |used| format!("{used:.1}"));
            let _ = writeln!(out, "| {phase} | {used} | {allowed} |");
        }
        out.push_str("\n| Window | Admitted s | Drained s |\n|---|---:|---:|\n");
        for window in &lifecycle.windows {
            let _ = writeln!(
                out,
                "| {} | {:.3} | {:.3} |",
                window.phase, window.admitted_seconds, window.drained_seconds
            );
        }
        out.push_str("\nOwned processes:\n");
        for owned in &lifecycle.owned {
            let _ = writeln!(
                out,
                "- {} pid {} started {}",
                owned.role.name(),
                owned.pid,
                owned
                    .started_unix
                    .map_or_else(|| "unknown".to_owned(), |started| started.to_string())
            );
        }
        let _ = writeln!(
            out,
            "\nOutstanding work before teardown: {}",
            lifecycle.outstanding.as_ref().map_or_else(
                || "not read by this workload".to_owned(),
                ToString::to_string
            )
        );
    }
    if let Some(category) = Category::parse(&case.category) {
        let headline = Headline::of(category, &case.family, &case.case);
        let cells = cells(category, case);
        let _ = write!(
            out,
            "\nCase aggregate: throughput {}; server CPU {}; {}",
            cells[0], cells[1], cells[2]
        );
        if headline.input != Input::None {
            let _ = write!(out, "; input {}", cells[3]);
        }
        let _ = writeln!(out, "; target {}", cells[cells.len() - 1]);
    }
    let profile = &record.artifacts.profile;
    if profile.requested {
        out.push_str("\nProfile captures:\n");
        for capture in &profile.captures {
            let _ = writeln!(
                out,
                "- {}",
                serde_json::to_string(capture).unwrap_or_default()
            );
        }
        for note in &profile.notes {
            let _ = writeln!(out, "- note: {note}");
        }
    }
    out.push('\n');
    out.push_str(&record.render());
    out
}

/// Reads a sweep record at `path` and its case records beside it, and
/// renders its table.
///
/// # Errors
///
/// Returns a message when the sweep record cannot be read.
pub fn report(path: &Path) -> Result<Option<String>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let directory = path.parent().unwrap_or(Path::new("."));
    Ok(match parse(&text)? {
        None => None,
        Some(Saved::Case(mut case)) => {
            case.persisted(path);
            Some(render_case(&case))
        }
        Some(Saved::Sweep(sweep)) => {
            Some(render_sweep(&sweep, &path.display().to_string(), &|name| {
                read_case(&directory.join(name))
            }))
        }
    })
}

/// Reads the case record at `path`, with its engine record's persistence
/// time from the sweep record beside it.
///
/// # Errors
///
/// Returns a message when it is unreadable or not a case record.
pub fn read_case(path: &Path) -> Result<CaseRecord, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let Some(Saved::Case(mut case)) = parse(&text)? else {
        return Err("not an L1 case record".to_owned());
    };
    case.persisted(path);
    Ok(*case)
}

/// The sweep entry of `case` from its persisted outcome: the case record's
/// verdict and reasons, made INVALID with the reason when `kept` failed
/// (no reference) or took longer than `allowance` seconds (reference
/// kept).
#[must_use]
pub fn entry(
    case: &Case,
    record: &CaseRecord,
    kept: Result<(String, f64), String>,
    allowance: u64,
) -> SweepEntry {
    let mut reasons = record.reasons.clone();
    let mut status = match record.verdict {
        Verdict::Valid => CaseStatus::Valid,
        Verdict::Invalid => CaseStatus::Invalid,
    };
    let mut persisted = None;
    let reference = match kept {
        Ok((name, seconds)) => {
            persisted = Some(seconds);
            if seconds > allowance as f64 {
                status = CaseStatus::Invalid;
                reasons.push(format!(
                    "persisting the case record took {seconds:.1}s, over its {allowance}s allowance"
                ));
            }
            Some(name)
        }
        Err(error) => {
            status = CaseStatus::Invalid;
            reasons.push(format!("the case record could not be kept: {error}"));
            None
        }
    };
    SweepEntry {
        selector: case.selector(),
        family: case.family.to_owned(),
        case: case.id.to_owned(),
        status,
        record: reference,
        reasons,
        persist_seconds: persisted,
    }
}
