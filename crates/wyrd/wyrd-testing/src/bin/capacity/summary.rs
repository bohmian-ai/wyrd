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
    /// confirmed gone before the teardown deadline; gone only later is
    /// `false` with nothing `surviving`.
    pub confirmed: bool,
    /// Seconds of teardown, from when it began (the case's announcement,
    /// the cutoff, an interrupt or the case process ending) to the end of
    /// confirmation.
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
    /// repetition order, as records kept before these local diagnostics
    /// moved to gzipped files under the case's artifacts directory. Read
    /// only, so those records still render in full; never written.
    #[serde(default, skip_serializing)]
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
    /// Puts the window snapshots back onto the engine record's
    /// repetitions, so its full report lists every window series: an older
    /// record's embedded [`Self::timings`], else the local diagnostics its
    /// case kept under `target/`. Without either, the record still renders
    /// every measurement, only without the per-window series.
    fn hydrate(&mut self) {
        let Some(record) = self.record.as_mut() else {
            return;
        };
        if self.timings.is_empty() {
            record.load_timings();
            return;
        }
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
    /// The work unit's noun, as in "no <noun> completed".
    noun: &'static str,
    /// The Server CPU per unit cell's unit and its multiplier from
    /// CPU-seconds per unit: milliseconds per unit, or per 1,000 rows for
    /// writes, whose per-row cost would round away at three decimals.
    cpu: (&'static str, f64),
    /// The pooled latency reading.
    latency: &'static str,
    /// The latency label, naming the reading in a shared Headline latency
    /// column; a category whose columns name their readings shows none.
    label: &'static str,
    /// Whether the row has an OTLP input column reading.
    input: Input,
}

/// A case's Input column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Input {
    /// Not a write category: no column.
    None,
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
                cpu: ("ms/verification", 1e3),
                latency: "overhead_p99_ms",
                label: "server overhead p99",
                input: Input::None,
            },
            (Category::Gateway, _) => Self {
                work: "requests",
                unit: "requests/s",
                noun: "request",
                cpu: ("ms/request", 1e3),
                latency: "overhead_p99_ms",
                label: "added overhead p99",
                input: Input::None,
            },
            (Category::BifrostWrites, _) => Self {
                work: "rows",
                unit: "rows/s",
                noun: "row",
                cpu: ("ms/1k rows", 1e6),
                latency: "p99_ms",
                label: "durable ACK p99",
                input: if case.starts_with("native-write") {
                    Input::Unmeasured
                } else {
                    Input::Otlp
                },
            },
            (Category::BifrostReads, family) if family.starts_with("ui-sql") => Self {
                work: "requests",
                unit: "pages/s",
                noun: "page",
                cpu: ("ms/page", 1e3),
                latency: "client_p99_ms",
                label: "client page p99",
                input: Input::None,
            },
            (Category::BifrostReads, _) if case.starts_with("full-scan") => Self {
                work: "queries",
                unit: "queries/s",
                noun: "query",
                cpu: ("ms/query", 1e3),
                latency: "p95_ms",
                label: "server query p95",
                input: Input::None,
            },
            (Category::BifrostReads, _) => Self {
                work: "queries",
                unit: "queries/s",
                noun: "query",
                cpu: ("ms/query", 1e3),
                latency: "p99_ms",
                label: "server query p99",
                input: Input::None,
            },
        }
    }
}

/// The performance columns of a category's table, in order, after Case,
/// Tier (reads only) and Status: [`cells`] fills exactly these. Verification
/// shows its server overhead and client round trip in their own columns;
/// every other category names its one latency inside the Headline latency
/// cell.
fn columns(category: Option<Category>) -> &'static [&'static str] {
    match category {
        Some(Category::Verification) => &[
            "Throughput",
            "Server CPU per unit",
            "Server overhead p99",
            "Client round trip p99",
            "Target",
        ],
        Some(Category::BifrostWrites) => &[
            "Throughput",
            "Server CPU per unit",
            "Headline latency",
            "Input",
            "Target",
        ],
        Some(Category::Gateway | Category::BifrostReads) | None => &[
            "Throughput",
            "Server CPU per unit",
            "Headline latency",
            "Target",
        ],
    }
}

/// The storage tier a Bifrost read case queries, from its registry
/// identity: `hot` cases and UI pages name `memtable` or `staged` in
/// their id, and everything else reads published Parquet.
fn tier(family: &str, case: &str) -> &'static str {
    if family == "parquet" {
        "published"
    } else if case.contains("memtable") {
        "memtable"
    } else if case.contains("staged") {
        "staged"
    } else {
        "published"
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
    let width = columns(Some(category)).len();
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
            cpu.clone().map(|cpu| cpu / work * headline.cpu.1)
        } else {
            Err(format!("no {} completed", headline.noun))
        }
    });
    let latency = interval(&aggregate.reading(headline.latency));
    let mut row = vec![
        fixed(throughput, 1, headline.unit),
        fixed(per_unit, 3, headline.cpu.0),
    ];
    if category == Category::Verification {
        row.extend([latency, interval(&aggregate.reading("client_p99_ms"))]);
    } else {
        row.push(format!("{}: {latency}", headline.label));
    }
    match headline.input {
        Input::None => {}
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
/// lacks, then the category's definitions ([`glossary`]).
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
    let reads = category == Some(Category::BifrostReads);
    let mut out = format!(
        "L1 {} sweep\n\nCoverage: {coverage}\nMode: {mode}\nSweep: {path}\n\n",
        sweep.category
    );
    let mut header = vec!["Case"];
    if reads {
        header.push("Tier");
    }
    header.push("Status");
    header.extend(columns(category));
    header.push("Record");
    let _ = writeln!(
        out,
        "| {} |\n|{}",
        header.join(" | "),
        "---|".repeat(header.len())
    );
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
        let performance =
            performance.unwrap_or_else(|| vec!["—".to_owned(); columns(category).len()]);
        let _ = write!(out, "| {} |", cell(&entry.selector));
        if reads {
            let _ = write!(out, " {} |", tier(&entry.family, &entry.case));
        }
        let _ = write!(out, " {} |", cell(&status));
        for value in performance {
            let _ = write!(out, " {} |", cell(&value));
        }
        let _ = writeln!(out, " {} |", cell(&record_cell));
    }
    if let Some(category) = category {
        let derived = derived(category, sweep, &valid);
        if !derived.is_empty() {
            out.push_str("\nDerived readings:\n\n");
            for line in derived {
                let _ = writeln!(out, "- {line}");
            }
        }
    }
    out.push_str(&glossary(category));
    out
}

/// The definitions printed under every sweep table: each column, and each
/// latency and derived reading the category shows, with where its
/// interval starts and stops, so a pasted table reads on its own.
fn glossary(category: Option<Category>) -> String {
    let mut lines: Vec<&str> = vec![
        "- **Case**: the exact selector. Its suffix is the load: `c<N>` is N closed-loop clients, each sending its next request only once the previous one is answered; `r<N>` is an open-loop schedule of N requests per second at 64 clients.",
    ];
    if category == Some(Category::BifrostReads) {
        lines.push(
            "- **Tier**: where the queried rows live: `memtable` (in server memory, not yet flushed), `staged` (flushed to fsynced local staged runs, not yet published) or `published` (Parquet in the table's object storage).",
        );
    }
    lines.extend([
        "- **Status**: `VALID` only when every measured window met the validity rules, every phase fit its allowance and cleanup was confirmed. Do not interpret the numbers of an `INVALID` or `NOT RUN` row.",
        "- **Throughput**: successful work units, in the unit shown, divided by the summed seconds of the case's three measured windows; warmup and drain are excluded. Higher is better.",
        "- **Server CPU per unit**: CPU time (user and system) of the server process over the measured windows, divided by the same work units, in milliseconds per unit (per 1,000 rows for writes). It includes background work the server did in those windows, so it is the server's whole cost per unit, not one code path. Lower is better.",
        "- **Latency cells**: one percentile of the case's latency, pooled over its three windows. `(a, b] ms` is the histogram bucket that holds it: above a and at most b, so read b as the value; a single value is exact. `n` is the number of samples pooled; a p99 needs at least 1,000 and a p95 at least 200, or the reading is unavailable.",
    ]);
    match category {
        Some(Category::Verification) => lines.extend([
            "- **Server overhead p99**: per direct verification, the server's time from the request's HTTP arrival until the execution finished (`verification.direct`), less the time spent inside its Verifier blocks: what Wyrd adds around the Verifier's own work. Network and client time are excluded.",
            "- **Client round trip p99**: per direct verification, the caller's time from immediately before the SDK's public `verify` call until its decoded result returns: SDK serialization, transport, the whole server execution including the Verifier, and decoding. Compare the `rust`, `python` and `typescript` `r3000` cases here: they run at one shared rate, so this is where each SDK's cost shows.",
        ]),
        Some(Category::Gateway) => lines.push(
            "- **Headline latency**, *added overhead p99*: per model call, the server's time from handler entry until the answer is handed off (a buffered response, or a stream's first bytes), less the time the call waited on the model provider: what the gateway adds on top of the provider. Network and client time are excluded.",
        ),
        Some(Category::BifrostWrites) => lines.extend([
            "- **Headline latency**, *durable ACK p99*: per write request, the server's time from its gRPC handler entry until the rows are durably acknowledged (Gate's acknowledgement for native writes, the export response for OTLP): when the rows are safe to call stored. Network and client time are excluded.",
            "- **Input**: OTLP writes only: encoded OTLP bytes accepted per second, and per server CPU-second. Native writes do not measure input bytes.",
        ]),
        Some(Category::BifrostReads) => lines.extend([
            "- **Headline latency**, by the label in the cell:",
            "  - *server query p99*: per query, the server's time from the request's arrival at its outermost HTTP middleware until the query's final result frame is handed to the response body: authentication, planning, execution and encoding. Network transfer and client decoding are excluded.",
            "  - *server query p95*: the same boundary at p95, for `full-scan`, whose windows hold too few queries for a p99.",
            "  - *client page p99* (UI SQL): per UI page (traces, verification, logs or metrics), the driver's time from sending the page's first statement until its last answer arrived; independent statements run concurrently, then those that use their results. A page is several queries, and the network and client are included, so it is larger than one query's latency.",
            "- **Variant reads**: both count, over the same published spans, the spans matching one `attributes` key. `span-shredded` filters `region`, a key on at least 10% of spans that the server stores in its own typed column; `span-long-tail` filters a `custom.*` key on about 0.1% of spans, which stays in the Variant's encoded remainder and is decoded per row. Their difference is the cost of reading a key from the remainder instead of its own column, for two different keys.",
        ]),
        None => {}
    }
    lines.extend([
        "- **Target**: `none`: earlier performance targets do not apply to L1 cases.",
        "- **Record**: the case record beside the sweep record; `capacity report <record>` shows its full evidence.",
    ]);
    match category {
        Some(Category::BifrostWrites) => lines.push(
            "- **`<path>-peak`**: the higher throughput of a write path's `c1` and `c64` cases, shown only when the sweep ran both.",
        ),
        Some(Category::BifrostReads) => lines.push(
            "- **`<lookup>-scaling`**: `Q64 / (64 × Q1)`, the `c64` case's throughput over 64 times the `c1` case's: 1.0 means 64 clients answer 64 times what one client does; lower means contention. Shown only when the sweep ran both cases.",
        ),
        _ => {}
    }
    format!("\nDefinitions:\n\n{}\n", lines.join("\n"))
}

/// The Bifrost readings derived from two cases of the sweep, for each one
/// whose two inputs the sweep both selected: each write path's peak
/// acknowledged rows/s in a writes sweep, and each lookup's 64-client rate
/// scaling in a reads sweep; other categories derive none. An input
/// that is not VALID or has no reading makes the reading incomplete and
/// names it; a reading the selection cannot fill is not printed, and
/// nothing derived ever starts a case.
fn derived(category: Category, sweep: &SweepRecord, valid: &[CaseRecord]) -> Vec<String> {
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
        ids.iter().all(|id| {
            sweep
                .cases
                .iter()
                .any(|entry| entry.family == family && entry.case == *id)
        })
    };
    let mut lines = Vec::new();
    let peaks: &[&str] = if category == Category::BifrostWrites {
        &["otlp-spans", "otlp-spans-1k", "otlp-logs", "native-write"]
    } else {
        &[]
    };
    for op in peaks {
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
    let scalings: &[(&str, &str, &str, &str)] = if category == Category::BifrostReads {
        &[
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
        ]
    } else {
        &[]
    };
    for &(family, name, base, scaled) in scalings {
        if !selected(family, &[base, scaled]) {
            continue;
        }
        let pair = (
            rate(family, base, "queries"),
            rate(family, scaled, "queries"),
        );
        lines.push(match pair {
            (Ok(q1), Ok(q64)) if q1 > 0.0 => format!(
                "{name}: {:.3} (Q64 / (64 × Q1)) from `{scaled}` ({q64:.1} queries/s) and `{base}` ({q1:.1} queries/s)",
                q64 / (64.0 * q1)
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
        let readings: Vec<String> = columns(Some(category))
            .iter()
            .zip(cells(category, case))
            .map(|(column, value)| match *column {
                "Headline latency" => value,
                "Server CPU per unit" => format!("server CPU {value}"),
                column => format!("{} {value}", column.to_lowercase()),
            })
            .collect();
        let _ = writeln!(out, "\nCase aggregate: {}", readings.join("; "));
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
