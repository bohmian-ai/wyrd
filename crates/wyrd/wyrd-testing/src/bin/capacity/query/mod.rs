//! The Bifrost engine's experiments, run through `mise run bench:l1:bifrost`
//! and `mise run bench:l2:bifrost`.
//!
//! Both read as the production journey they measure. One release
//! `wyrd-server` starts through the local-development guide (`migrate`,
//! serve, `setup`) on the whole host; an application then uses the public
//! Rust client exactly as the guide's `WYRD_SERVER_URL` and `WYRD_API_KEY`
//! configure it. Every query is checked against its exact answer and every
//! write resolves at its durable acknowledgement. Evidence comes from client
//! timings, `/metrics`, and the OS's readings of the server process.
//!
//! - **L1** ([`isolated`]), three experiments at 1 and 64 closed-loop
//!   clients ([`Experiment`]): `writes-parquet` sweeps native Arrow, OTLP
//!   spans, and OTLP logs writes under the Parquet Scribe settings, each
//!   repetition a full window of fresh identities; `hot` reads 10M events and 1M spans held in memtables;
//!   `parquet` reads them published as Parquet, with span Bloom, shredding,
//!   and pruning checks and each write path's storage ratio.
//! - **L2, `competing`** ([`competing`]): the heavy 100M-row fixture with
//!   Forge compaction on, one stateful step whose every repetition runs open
//!   loop mixed ingest at a rate calibrated from the newest compatible L1
//!   record, mixed queries open loop at 12/s with at most 32 in flight, and
//!   three client-queue lanes, through capacity → overload → recovery.

mod competing;
mod isolated;
mod logs;
mod run;
mod telemetry;
mod workload;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use wyrd_client::config::ClientConfig;
use wyrd_client::{GlobalConfig, WyrdClient};
use wyrd_testing::experiment::{
    Criterion, Declaration, Derive, Direction, Engine, Level, Metric, Mode, Op, Reason, Record,
    Status, Step, Verdict, WINDOW_SECONDS,
};
use wyrd_testing::release_server::{LocalServer, SERVER_URL};

use crate::levels::{Context, Plan};

/// Error type of the server and client steps a workload runs.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Closed-loop client counts of every Bifrost L1 sweep: unloaded latency
/// and cost at 1, the SLO at 64.
pub const PAIR: [u64; 2] = [1, 64];

/// The one-client step of a read measured only unloaded.
const ONE: [u64; 1] = [1];

/// The fixture sizes a Bifrost plan loads: the declared full sizes, or a
/// smoke's smaller sizes of the same shape, which still write every path
/// and table, publish, scan, and settle as the full run does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Scale {
    /// Rows of the standard events fixture the read experiments load.
    pub standard: i64,
    /// Rows of the L2 heavy events fixture.
    pub heavy: i64,
    /// Spans the `hot` and `parquet` read fixtures load through OTLP.
    pub spans: i64,
    /// Rows `parquet` loads into each native and logs storage sample.
    pub storage: i64,
}

impl Scale {
    /// The full experiment's fixtures.
    pub const FULL: Self = Self {
        standard: workload::STANDARD_ROWS,
        heavy: workload::HEAVY_ROWS,
        spans: 1_000_000,
        storage: 200_000,
    };

    /// A smoke's fixtures: many ingest requests and aggregate variants each,
    /// so loading, publication, and pruning keep their shape.
    pub const SMOKE: Self = Self {
        standard: 640_000,
        heavy: 1_280_000,
        spans: 64_000,
        storage: 32_768,
    };

    /// [`Self::SMOKE`] for a smoke, [`Self::FULL`] otherwise.
    #[must_use]
    pub const fn of(smoke: bool) -> Self {
        if smoke { Self::SMOKE } else { Self::FULL }
    }
}

/// The fixed open-loop ingest rate a Bifrost L2 smoke declares, rows per
/// second, in place of a calibration a smoke must never take or give.
const SMOKE_INGEST_ROWS_PER_S: f64 = 20_000.0;

/// Rows the L1 native and OTLP write fixtures spread over their day; a
/// window's rows continue past it, so every repetition writes fresh rows.
pub const WRITE_FIXTURE_ROWS: i64 = 2_000_000;

/// Strict point-lookup p99 objective, milliseconds; equality misses.
pub const POINT_P99_MS: f64 = 5.0;

/// Native write peak sanity floor, rows per second.
pub const NATIVE_PEAK_FLOOR: f64 = 200_000.0;

/// OTLP write peak sanity floor, encoded wire MiB per server CPU-second.
pub const OTLP_PEAK_FLOOR: f64 = 30.0;

/// The `writes-parquet` budget, seconds: its measured windows, the
/// publication each path waits for before the next, and its drain take
/// about 311 s, over the L1 300 s every other experiment keeps.
const WRITES_BUDGET_SECONDS: u64 = 420;

/// The L2 experiment and step id.
pub const COMPETING: &str = "competing";

/// Fraction of calibrated mixed capacity the L2 open-loop ingest offers.
pub const HEADROOM: f64 = 0.7;

/// Declared server cores the L2 calibration scales by, unless
/// `--server-cores` names another.
pub const SERVER_CORES: f64 = 8.0;

/// The L2 ingest split by rows, in [`WriteOp`] order of spans, logs, native.
pub const SPLIT: [(WriteOp, f64); 3] = [
    (WriteOp::Spans, 0.5),
    (WriteOp::Logs, 0.3),
    (WriteOp::Native, 0.2),
];

/// Scribe's operator settings that publish every row as Parquet: default
/// sizes, so writes roll production-sized objects, and a 30 s retention so
/// the residue publishes.
const PARQUET_ENV: [(&str, &str); 1] = [("WYRD_MAX_FILE_RETENTION_TIME", "30")];

/// Scribe's operator settings that keep every row in a memtable: 16,384 MB
/// memtable and on-disk rotation sizes and a 24-hour retention, so nothing
/// stages or publishes. Only the `hot` reads use them, over a fixture
/// loaded once that never grows; a write workload under them would hold
/// every row it writes in memory.
const HOT_ENV: [(&str, &str); 3] = [
    ("WYRD_MAX_FILE_SIZE_IN_MEMORY", "16384"),
    ("WYRD_MAX_FILE_SIZE_ON_DISK", "16384"),
    ("WYRD_MAX_FILE_RETENTION_TIME", "86400"),
];

/// The three Bifrost L1 experiments, each its own record, verdict, and
/// L1 budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Experiment {
    /// Native, OTLP spans, and OTLP logs writes under the `parquet` Scribe
    /// settings, whose windows
    /// pay for the staging and publication they cause; nothing waits for
    /// publication inside a window.
    WritesParquet,
    /// Reads while every row stays in a memtable.
    Hot,
    /// Reads after every row is published as Parquet.
    Parquet,
}

impl Experiment {
    /// Every experiment, in the order `l1 bifrost` runs them.
    pub const ALL: [Self; 3] = [Self::WritesParquet, Self::Hot, Self::Parquet];

    /// The experiment id: its command word and record directory.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::WritesParquet => "writes-parquet",
            Self::Hot => "hot",
            Self::Parquet => "parquet",
        }
    }

    /// The experiment `name` selects.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|experiment| experiment.name() == name)
    }

    /// Whether the experiment measures writes rather than reads.
    #[must_use]
    pub const fn writes(self) -> bool {
        matches!(self, Self::WritesParquet)
    }

    /// The Scribe settings the server runs with.
    const fn env(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::WritesParquet | Self::Parquet => &PARQUET_ENV,
            Self::Hot => &HOT_ENV,
        }
    }

    /// The read steps: each op, what it asks, and its client counts.
    const fn reads(self) -> &'static [(&'static str, Read, &'static [u64])] {
        match self {
            Self::WritesParquet => &[],
            Self::Hot => &HOT_READS,
            Self::Parquet => &PARQUET_READS,
        }
    }
}

/// What a read step asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Read {
    /// An events-fixture case.
    Events(workload::Case),
    /// A span `trace_id` lookup, read through its Bloom filter.
    Trace,
    /// A span filter on a shredded attribute key.
    Shredded,
    /// A span filter on a long-tail key below the shredding threshold.
    LongTail,
}

impl Read {
    /// The closed benchmark case its queries carry.
    #[must_use]
    pub const fn case(self) -> &'static str {
        match self {
            Self::Events(case) => case.label(),
            Self::Trace => "trace-lookup",
            Self::Shredded => "shredded-filter",
            Self::LongTail => "long-tail-filter",
        }
    }
}

/// The `hot` read steps.
const HOT_READS: [(&str, Read, &[u64]); 3] = [
    ("point", Read::Events(workload::Case::Selective), &PAIR),
    (
        "small-aggregate",
        Read::Events(workload::Case::SmallAggregate),
        &ONE,
    ),
    ("span-shredded", Read::Shredded, &ONE),
];

/// The `parquet` read steps.
const PARQUET_READS: [(&str, Read, &[u64]); 6] = [
    ("point", Read::Events(workload::Case::Selective), &PAIR),
    (
        "small-aggregate",
        Read::Events(workload::Case::SmallAggregate),
        &ONE,
    ),
    ("full-scan", Read::Events(workload::Case::FullScan), &ONE),
    ("span-trace", Read::Trace, &PAIR),
    ("span-shredded", Read::Shredded, &ONE),
    ("span-long-tail", Read::LongTail, &ONE),
];

/// The three ingest paths an experiment writes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOp {
    /// Native Arrow writes into `bifrost.datasets.events_ingest`.
    Native,
    /// OTLP spans into `bifrost.traces.spans`.
    Spans,
    /// OTLP bulky Kubernetes logs into `bifrost.logs.records`.
    Logs,
}

impl WriteOp {
    /// Every path, in L1 sweep order: native last, so no later path's
    /// warmup waits for its far larger backlog to publish.
    pub const ALL: [Self; 3] = [Self::Spans, Self::Logs, Self::Native];

    /// The table this path writes, as `vala.<segment>.<name>`.
    #[must_use]
    pub const fn table(self) -> &'static str {
        match self {
            Self::Native => workload::INGEST_TABLE,
            Self::Spans => telemetry::TABLE,
            Self::Logs => "bifrost.logs.records",
        }
    }

    /// The step-id prefix and `op` parameter of this path.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Native => "native-write",
            Self::Spans => "otlp-spans",
            Self::Logs => "otlp-logs",
        }
    }

    /// The path an `op` step parameter names.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|op| op.name() == name)
    }

    /// The server timing boundary of this path's requests: Gate's write
    /// acknowledgement or the OTLP export's response handoff.
    #[must_use]
    pub const fn boundary(self) -> &'static str {
        match self {
            Self::Native => "bifrost.gate.write",
            Self::Spans => "bifrost.otlp.traces",
            Self::Logs => "bifrost.otlp.logs",
        }
    }
}

/// The read an `op` step parameter names.
fn read_op(name: &str) -> Option<Read> {
    PARQUET_READS
        .iter()
        .chain(&HOT_READS)
        .find(|(op, ..)| *op == name)
        .map(|(_, read, _)| *read)
}

/// The L1 experiment `arguments[1]` names (program name first).
///
/// # Errors
///
/// Returns a message when it is absent or unknown.
pub fn experiment(arguments: &[String]) -> std::result::Result<Experiment, String> {
    arguments
        .get(1)
        .and_then(|name| Experiment::parse(name))
        .ok_or_else(|| {
            format!(
                "l1 bifrost takes one experiment of {:?} before its flags",
                Experiment::ALL.map(Experiment::name)
            )
        })
}

/// Plans the Bifrost experiment at the context's level, at L1 the one its
/// first argument names: refuses unknown flags, declares it, and builds the
/// workload, which starts the server only once the run admits it.
///
/// # Errors
///
/// Returns a message for a missing or unknown L1 experiment, an unknown
/// flag, or an invalid `--server-cores`.
pub fn plan(context: &Context<'_>, base: Declaration) -> std::result::Result<Plan, String> {
    let (experiment, flags) = match context.level {
        Level::One => {
            let experiment = experiment(context.arguments)?;
            let flags: Vec<String> = context
                .arguments
                .iter()
                .take(1)
                .chain(context.arguments.iter().skip(2))
                .cloned()
                .collect();
            (Some(experiment), flags)
        }
        Level::Two => (None, context.arguments.to_vec()),
    };
    let valued: &[&str] = match context.level {
        Level::One => &[],
        Level::Two => &["--server-cores"],
    };
    Context {
        arguments: &flags,
        ..*context
    }
    .known(valued, &[])?;
    let mut declaration = declare(experiment, &flags, base, context.root)?;
    let logs = context.output(Engine::Bifrost, &declaration.experiment);
    let scale = Scale::of(context.smoke());
    if context.smoke()
        && let Some(Value::Object(fixture)) = declaration.controls.get_mut("fixture")
    {
        fixture.insert("smoke_rows".to_owned(), json!(scale));
    }
    let workload: Box<dyn wyrd_testing::experiment::Workload> = match experiment {
        Some(experiment) => Box::new(isolated::Isolated::new(logs, experiment, scale)),
        None => Box::new(competing::Competing::new(logs, scale)),
    };
    Ok(Plan {
        declaration,
        workload,
    })
}

/// The Bifrost declaration over `base`: the L1 `experiment`, or L2 when
/// `None`, reading `--server-cores` and `--smoke` from `arguments` (program
/// name first), and, at L2, the L1 records under
/// `root/benchmarks/bifrost/writes-parquet/`. Pure otherwise: it starts
/// nothing.
///
/// A missing or unusable calibration is a declaration refusal
/// (`calibration.missing`), never a substituted rate; a smoke declares
/// [`SMOKE_INGEST_ROWS_PER_S`] and reads no calibration.
///
/// # Errors
///
/// Returns a message for a `--server-cores` that is not a positive number.
pub fn declare(
    experiment: Option<Experiment>,
    arguments: &[String],
    base: Declaration,
    root: &Path,
) -> std::result::Result<Declaration, String> {
    let declaration = if let Some(experiment) = experiment {
        l1_declaration(experiment, base)
    } else {
        let cores = server_cores(arguments)?;
        let records = if arguments.iter().any(|argument| argument == "--smoke") {
            None
        } else {
            Some(l1_records(root))
        };
        competing_declaration(base, cores, records.as_deref())
    };
    Ok(declaration)
}

/// The `--server-cores` value, or [`SERVER_CORES`].
///
/// # Errors
///
/// Returns a message when it is not a positive number.
fn server_cores(arguments: &[String]) -> std::result::Result<f64, String> {
    let Some(position) = arguments
        .iter()
        .position(|argument| argument == "--server-cores")
    else {
        return Ok(SERVER_CORES);
    };
    arguments
        .get(position + 1)
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|cores| cores.is_finite() && *cores > 0.0)
        .ok_or_else(|| "--server-cores takes a positive number of cores".to_owned())
}

/// Every readable record under `root/benchmarks/bifrost/writes-parquet/`;
/// an unreadable one is skipped, since calibration only needs a valid one.
fn l1_records(root: &Path) -> Vec<Record> {
    let directory = root
        .join("benchmarks")
        .join(Engine::Bifrost.name())
        .join(Experiment::WritesParquet.name());
    std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .filter_map(|path| Record::read(&path).ok())
        .collect()
}

/// The fixture control every write record declares, which a calibrating
/// L2 must match.
fn write_fixture() -> Value {
    json!({
        "native": format!("native-2m: events rows into bifrost.datasets.events_ingest, {WRITE_FIXTURE_ROWS} rows per day, fresh ids every window"),
        "spans": format!("spans-2m: GenAI/HTTP spans into bifrost.traces.spans, {WRITE_FIXTURE_ROWS} spans per day, fresh ids every window"),
        "logs": format!("logs-2m: bulky Kubernetes logs into bifrost.logs.records, {WRITE_FIXTURE_ROWS} records per day, fresh ids every window"),
    })
}

/// The fixture control of a read experiment: the events fixture and the
/// spans loaded before it, as the `tier` holds them.
fn read_fixture(tier: &str) -> Value {
    json!({
        "read": format!("standard-10m-{tier}: 10,000,000 events rows in bifrost.datasets.events, loaded before any read step"),
        "spans": format!("spans-1m-{tier}: {} spans of the spans-2m generator into bifrost.traces.spans, loaded before any read step", Scale::FULL.spans),
    })
}

/// The limits control of a server run with Scribe settings `settings`.
fn limits(settings: &[(&str, &str)]) -> Value {
    let settings: Vec<String> = settings
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    json!(format!(
        "server defaults on the whole host except {}; no resource quota",
        settings.join(", ")
    ))
}

/// The metrics every L1 step reads, raw or derived.
fn isolated_metrics() -> Vec<Metric> {
    let derived = |id: &str, unit: &str, boundary: &str, direction, derive| {
        Metric::derived(id, unit, boundary, direction, derive)
    };
    let work = |id: &str, scale: f64| (id.to_owned(), scale);
    let per_cpu = |(work, scale): (String, f64)| Derive::PerCpu { work, scale };
    let per_vcpu = |(work, scale): (String, f64)| Derive::PerVcpu { work, scale };
    let cpu_per = |(work, scale): (String, f64)| Derive::CpuPer { work, scale };
    let rate = |(work, scale): (String, f64)| Derive::Rate { work, scale };
    vec![
        Metric::cores(),
        Metric::raw(
            "requests",
            "requests",
            "successful requests in the window: answered queries or acknowledged write requests",
        ),
        Metric::raw("errors", "errors", "failed requests of every stable code"),
        Metric::raw("server_failures", "requests", SERVER_FAILURES),
        Metric::raw(
            "wrong_results",
            "queries",
            "answered queries whose rows differ from the exact answer",
        ),
        Metric::raw("p50_ms", "ms", SERVER_TIME),
        Metric::raw("p95_ms", "ms", SERVER_TIME),
        Metric::raw("p99_ms", "ms", SERVER_TIME),
        Metric::raw("client_p50_ms", "ms", CLIENT_TIME),
        Metric::raw("client_p95_ms", "ms", CLIENT_TIME),
        Metric::raw("client_p99_ms", "ms", CLIENT_TIME),
        Metric::raw(
            "queries",
            "queries",
            "exactly answered queries in the window; read steps only",
        ),
        Metric::raw(
            "scanned_rows_per_query",
            "rows/query",
            "mean rows each successful query's own executed-plan file scans output (`bifrost.oracle.stream` `rows_scanned` on its trace); read steps only",
        ),
        Metric::raw(
            "scanned_bytes_per_query",
            "B/query",
            "mean bytes each successful query's own executed plan scanned (`bifrost.oracle.stream` `bytes_scanned` on its trace); read steps only",
        ),
        Metric::raw(
            "rows",
            "rows",
            "durably acknowledged rows in the window; write steps only",
        ),
        Metric::raw(
            "retained_mib",
            "MiB",
            "uncompressed Arrow bytes Scribe inserted into memtables (`bifrost_scribe_memtable_bytes_inserted_total`) between the window open and its measured stop, the CPU interval, 2^20 bytes; write steps only",
        ),
        Metric::raw(
            "wire_bytes",
            "B",
            "encoded OTLP protobuf bytes of acknowledged exports, before framing and compression; OTLP steps only",
        ),
        Metric::raw(
            "published_files",
            "files",
            "Parquet files Scribe published (`bifrost_scribe_publication_files_total`) between the window open and its measured stop, the CPU interval: the publication the window's server CPU paid for, zero when none ran; write steps only",
        ),
        Metric::raw(
            "published_mib",
            "MiB",
            "Parquet MiB Scribe published (`bifrost_scribe_publication_bytes_total`) between the window open and its measured stop, 2^20 bytes; write steps only",
        ),
        Metric::raw(
            "memtable_backlog_mib",
            "MiB",
            "Arrow MiB still in active and immutable memtables (`bifrost_scribe_active_memtable_bytes`, `bifrost_scribe_immutable_memtable_bytes`) at the measured stop, 2^20 bytes; write steps only",
        ),
        Metric::raw(
            "staged_backlog_mib",
            "MiB",
            "staged MiB not yet published (`bifrost_scribe_staging_live_bytes`) at the measured stop, 2^20 bytes; write steps only",
        ),
        Metric::raw(
            "visibility_p99_ms",
            "ms",
            "acknowledgement until the rows are queryable",
        ),
        Metric::raw(
            "storage_ratio",
            "ratio",
            "the path's table growth in published Parquet bytes per retained Arrow byte inserted while one 200,000-row sample of the path loaded before the windows, settled with the read fixture; the inserted bytes include server-internal rows written meanwhile, and WAL, staging, and catalog bytes are excluded; write steps only",
        ),
        Metric::raw("scaling", "ratio", "Q_N/(N × Q_1) of `rate`"),
        Metric::raw("base_rate", "requests/s", "the one-client step's `rate`"),
        Metric::raw("scaled_rate", "requests/s", "the 64-client step's `rate`"),
        derived(
            "rate",
            "requests/s",
            "successful requests per wall second",
            Direction::Higher,
            rate(work("requests", 1.0)),
        ),
        derived(
            "queries_per_s_per_vcpu",
            "queries/s/vCPU",
            "(W/T)/P: exactly answered queries per wall second per provisioned vCPU",
            Direction::Higher,
            per_vcpu(work("queries", 1.0)),
        ),
        derived(
            "queries_per_cpu_s",
            "queries/cpu-s",
            "W/C: exactly answered queries per server CPU-second used",
            Direction::Higher,
            per_cpu(work("queries", 1.0)),
        ),
        derived(
            "cpu_ms_per_query",
            "ms/query",
            "server CPU per exactly answered query",
            Direction::Lower,
            cpu_per(work("queries", 1e3)),
        ),
        derived(
            "rows_per_s",
            "rows/s",
            "durably acknowledged rows per wall second",
            Direction::Higher,
            rate(work("rows", 1.0)),
        ),
        derived(
            "rows_per_s_per_vcpu",
            "rows/s/vCPU",
            "(W/T)/P: durably acknowledged rows per wall second per provisioned vCPU",
            Direction::Higher,
            per_vcpu(work("rows", 1.0)),
        ),
        derived(
            "rows_per_cpu_s",
            "rows/cpu-s",
            "W/C: durably acknowledged rows per server CPU-second used",
            Direction::Higher,
            per_cpu(work("rows", 1.0)),
        ),
        derived(
            "cpu_us_per_row",
            "µs/row",
            "server CPU per durably acknowledged row",
            Direction::Lower,
            cpu_per(work("rows", 1e6)),
        ),
        derived(
            "memory_per_mib_s",
            "B/(MiB/s)",
            "sampled peak server resident bytes per retained Arrow MiB/s achieved; a sampled peak, not an exact OS high-water mark",
            Direction::Lower,
            Derive::MemoryPer {
                work: "retained_mib".to_owned(),
            },
        ),
        derived(
            "mib_per_s_per_vcpu",
            "MiB/s/vCPU",
            "(W/T)/P: uncompressed retained Arrow MiB per wall second per provisioned vCPU",
            Direction::Higher,
            per_vcpu(work("retained_mib", 1.0)),
        ),
        derived(
            "mib_per_cpu_s",
            "MiB/cpu-s",
            "W/C: uncompressed retained Arrow MiB per server CPU-second used",
            Direction::Higher,
            per_cpu(work("retained_mib", 1.0)),
        ),
        derived(
            "wire_mib_per_s_per_vcpu",
            "MiB/s/vCPU",
            "(W/T)/P: encoded OTLP wire MiB per wall second per provisioned vCPU",
            Direction::Higher,
            per_vcpu(work("wire_bytes", 1.0 / 1_048_576.0)),
        ),
        derived(
            "wire_mib_per_cpu_s",
            "MiB/cpu-s",
            "W/C: encoded OTLP wire MiB per server CPU-second used",
            Direction::Higher,
            per_cpu(work("wire_bytes", 1.0 / 1_048_576.0)),
        ),
    ]
}

/// The boundary of every judged Bifrost request time.
const SERVER_TIME: &str = "server handler entry until the complete query result is handed off (`bifrost.query`), Gate's durable write acknowledgement (`bifrost.gate.write`), or the OTLP export response (`bifrost.otlp.traces`, `bifrost.otlp.logs`): successful requests' nearest-rank bucket bounds of the server's timing histograms";

/// The boundary of the diagnostic client round trip, which never judges.
const CLIENT_TIME: &str =
    "diagnostic only: client send until its terminal or durable acknowledgement, never judged";

/// What `server_failures` counts.
const SERVER_FAILURES: &str = "server timings at the step's boundary whose outcome is not `success`, counted in their own outcome series and never judged";

/// The settled-state facts the `parquet` experiment reads once after its
/// settlement, outside every window, and reports on the step each
/// describes: `(step, metric, unit, description)`.
pub const SETTLED: [(&str, &str, &str, &str); 5] = [
    (
        "span-shredded-c1",
        "span_shredding_faults",
        "count",
        "1 when the published span files' `attributes` Variant does not shred exactly the keys on at least 10% of spans, or no file shredded; 0 otherwise; read once after settlement",
    ),
    (
        "span-trace-c1",
        "span_window_pruned",
        "count",
        "1 when a one-hour `start_time_unix_nano` window scanned fewer files or row groups (`oracle_query_files_scanned_total`, `oracle_query_row_groups_scanned_total`) than the whole span table; read once after settlement",
    ),
    (
        "full-scan-c1",
        "storage_ratio.otlp-spans",
        "ratio",
        STORAGE_RATIO,
    ),
    (
        "full-scan-c1",
        "storage_ratio.otlp-logs",
        "ratio",
        STORAGE_RATIO,
    ),
    (
        "full-scan-c1",
        "storage_ratio.native-write",
        "ratio",
        STORAGE_RATIO,
    ),
];

/// What each `storage_ratio.<path>` reading is.
const STORAGE_RATIO: &str = "the path's table growth in published Parquet bytes per retained Arrow byte inserted while its sample loaded alone: 200,000 rows for native and logs, the 1M-row span fixture for spans, all settled together; the inserted bytes include server-internal rows written meanwhile, and WAL, staging, and catalog bytes are excluded";

/// The L1 declaration of `experiment`: its read steps and their scaling,
/// or the write sweeps and their peaks, the exactness, error, SLO,
/// sanity-floor, scan-fact, and settled-check criteria, within the
/// experiment's own budget: [`WRITES_BUDGET_SECONDS`] for `writes-parquet`,
/// the L1 budget otherwise.
fn l1_declaration(experiment: Experiment, mut base: Declaration) -> Declaration {
    experiment.name().clone_into(&mut base.experiment);
    if experiment.writes() {
        base.timing.budget_seconds = WRITES_BUDGET_SECONDS;
    }
    base.question = String::from(match experiment {
        Experiment::WritesParquet => {
            "Under the Parquet Scribe settings, whose windows pay for the staging and publication they cause, what does each native, OTLP spans, and OTLP logs write cost the server per CPU-second and per provisioned vCPU at 1 and 64 closed-loop clients up to its durable acknowledgement, with no error, how much publication ran and what backlog remained, and do the native and OTLP write peaks clear their sanity floors?"
        }
        Experiment::Hot => {
            "With 10M events rows and 1M spans held in memtables, does a point lookup keep p99 strictly under 5 ms at 64 closed-loop clients with every answer exact, and what does each hot read cost the server per CPU-second?"
        }
        Experiment::Parquet => {
            "With 10M events rows and 1M spans published as Parquet, does a point lookup keep p99 strictly under 5 ms at 64 closed-loop clients with every answer exact, what does each read cost per CPU-second and scan per query, do the span files shred exactly the keys over the threshold, does a span-time window prune, and what does each write path store per retained byte?"
        }
    });
    base.variables.insert("clients".to_owned(), json!(PAIR));
    base.controls.insert(
        "fixture".to_owned(),
        match experiment {
            Experiment::WritesParquet => write_fixture(),
            Experiment::Hot => read_fixture("hot"),
            Experiment::Parquet => read_fixture("parquet"),
        },
    );
    base.controls
        .insert("limits".to_owned(), limits(experiment.env()));
    base.controls.insert(
        "workload".to_owned(),
        json!(match experiment {
            Experiment::WritesParquet => "closed-loop public-client clients, each sending its next request once the last is acknowledged: native `write_batch` of 16,384 rows; OTLP exports of 512 spans or log records with the client's exchanged bearer",
            Experiment::Hot | Experiment::Parquet => "closed-loop public-client clients, each sending its next query once the last answers, every answer checked exactly: Bifrost SQL events point lookups, 100k-row aggregates, and full scans; span `trace_id` lookups and `attributes ->>` filters on a shredded and a long-tail key",
        }),
    );
    base.metrics = isolated_metrics();
    if experiment == Experiment::Parquet {
        base.metrics.extend(
            SETTLED
                .iter()
                .map(|(_, metric, unit, description)| Metric::raw(metric, unit, description)),
        );
    }
    let mut reads = Vec::new();
    for (op, _, clients) in experiment.reads() {
        for clients in *clients {
            let id = format!("{op}-c{clients}");
            base.steps.push(windowed(
                experiment,
                &id,
                json!({ "op": op, "clients": clients }),
            ));
            reads.push(id);
        }
    }
    for (op, _, clients) in experiment.reads() {
        if *clients == PAIR {
            base.steps.push(Step::scaling(
                &format!("{op}-scaling"),
                &format!("{op}-c1"),
                &format!("{op}-c64"),
                64.0,
                "rate",
            ));
        }
    }
    let mut writes = Vec::new();
    if experiment.writes() {
        for op in WriteOp::ALL {
            let sweep: Vec<String> = PAIR
                .iter()
                .map(|clients| format!("{}-c{clients}", op.name()))
                .collect();
            for (id, clients) in sweep.iter().zip(PAIR) {
                base.steps.push(windowed(
                    experiment,
                    id,
                    json!({ "op": op.name(), "clients": clients }),
                ));
            }
            base.steps.push(Step::peak(
                &format!("{}-peak", op.name()),
                &sweep,
                "rows_per_s",
            ));
            writes.extend(sweep);
        }
    }
    let measured: Vec<String> = reads.iter().chain(&writes).cloned().collect();
    base.criteria = vec![Criterion::every(
        "no-errors",
        "errors",
        &measured,
        Op::Le,
        0.0,
    )];
    if experiment.writes() {
        for (id, metric, step, floor) in [
            (
                "native-peak-floor",
                "rows_per_s",
                "native-write-peak",
                NATIVE_PEAK_FLOOR,
            ),
            (
                "otlp-spans-peak-floor",
                "wire_mib_per_cpu_s",
                "otlp-spans-peak",
                OTLP_PEAK_FLOOR,
            ),
            (
                "otlp-logs-peak-floor",
                "wire_mib_per_cpu_s",
                "otlp-logs-peak",
                OTLP_PEAK_FLOOR,
            ),
        ] {
            base.criteria.push(Criterion::limit(
                id,
                metric,
                &[step.to_owned()],
                Op::Ge,
                floor,
            ));
        }
    } else {
        base.criteria.extend([
            Criterion::every("exact-answers", "wrong_results", &reads, Op::Le, 0.0),
            Criterion::limit(
                "point-p99-slo",
                "p99_ms",
                &["point-c64".to_owned()],
                Op::Lt,
                POINT_P99_MS,
            )
            .percentile(99),
        ]);
    }
    if experiment == Experiment::Parquet {
        // A settled query must report its own scan; zero is a fact, a
        // missing reading is INVALID evidence.
        for metric in ["scanned_rows_per_query", "scanned_bytes_per_query"] {
            base.criteria.push(Criterion::every(
                &format!("scan-facts.{metric}"),
                metric,
                &reads,
                Op::Ge,
                0.0,
            ));
        }
        base.criteria.extend([
            Criterion::every(
                "span-shredding",
                "span_shredding_faults",
                &["span-shredded-c1".to_owned()],
                Op::Le,
                0.0,
            ),
            Criterion::every(
                "span-pruning",
                "span_window_pruned",
                &["span-trace-c1".to_owned()],
                Op::Ge,
                1.0,
            ),
        ]);
    }
    base
}

/// The measured step `id` of `experiment` with `params`, declaring the
/// 10 s window of a step whose 5 s L1 window would leave under 1.25 times
/// its latency percentile's sample floor at TASK-005's observed rates:
/// Parquet c1 aggregates (~930 per 5 s against 1,000) and span trace
/// lookups (~930), c1 native and OTLP spans writes (~150 and ~190 against
/// 200), and c64 native writes (~1,090 against 1,000).
fn windowed(experiment: Experiment, id: &str, mut params: Value) -> Step {
    let long = match experiment {
        Experiment::Parquet => matches!(id, "small-aggregate-c1" | "span-trace-c1"),
        Experiment::WritesParquet => {
            matches!(id, "native-write-c1" | "otlp-spans-c1" | "native-write-c64")
        }
        Experiment::Hot => false,
    };
    if long {
        params[WINDOW_SECONDS] = json!(10);
    }
    Step::measured(id, params)
}

/// The per-signal rows per server CPU-second an L2 rate is calibrated from,
/// and the L1 record it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    /// The L1 record's start, RFC 3339, naming it.
    pub record: String,
    /// The L1 record's measured source commit.
    pub commit: String,
    /// Peak median rows per server CPU-second of each path, in [`SPLIT`]
    /// order of spans, logs, native.
    pub rows_per_cpu_s: [f64; 3],
}

impl Calibration {
    /// Rows per server CPU-second of the [`SPLIT`] mix: the harmonic
    /// combination `1 / Σ(f_i / r_i)`.
    #[must_use]
    pub fn mixed_capacity(&self) -> f64 {
        1.0 / SPLIT
            .iter()
            .zip(self.rows_per_cpu_s)
            .map(|((_, share), rate)| share / rate)
            .sum::<f64>()
    }

    /// The L2 open-loop ingest rate, rows per second:
    /// [`HEADROOM`] × [`Self::mixed_capacity`] × `cores`.
    #[must_use]
    pub fn initial_rate(&self, cores: f64) -> f64 {
        HEADROOM * self.mixed_capacity() * cores
    }
}

/// The calibration the newest compatible valid L1 record gives `base`: a
/// Bifrost level-1 `writes-parquet` record from the same host fingerprint and
/// build profile, flags, and compiler, with the current L1 fixture and
/// limits controls, no validity reason, full (never smoke) mode wherever
/// the record now lives, and a measured non-INVALID sweep step of every
/// write path. Each path's rate is the highest median
/// `rows_per_cpu_s` over its sweep.
///
/// # Errors
///
/// Returns a message naming why no record qualifies.
pub fn calibrate(
    records: &[Record],
    base: &Declaration,
) -> std::result::Result<Calibration, String> {
    let build = |value: &Value, key: &str| value.get(key).cloned();
    let mut compatible: Vec<&Record> = records
        .iter()
        .filter(|record| {
            record.engine == Engine::Bifrost
                && record.level == Level::One
                && record.experiment == Experiment::WritesParquet.name()
                && record.host.fingerprint == base.host.fingerprint
                && ["profile", "rustflags", "compiler"]
                    .iter()
                    .all(|key| build(&record.build, key) == build(&base.build, key))
                && record.controls.values.get("fixture") == Some(&write_fixture())
                && record.controls.values.get("limits") == Some(&limits(&PARQUET_ENV))
                && record.validity.reasons.is_empty()
                && record.mode == Mode::Full
        })
        .collect();
    compatible.sort_by_key(|record| std::cmp::Reverse(record.started_at));
    let rate = |record: &Record, op: WriteOp| {
        record
            .steps
            .iter()
            .filter(|step| {
                step.status == Status::Measured
                    && step.verdict != Verdict::Invalid
                    && step.params.get("op").and_then(Value::as_str) == Some(op.name())
            })
            .filter_map(|step| step.summary.get("rows_per_cpu_s"))
            .filter_map(|summary| {
                summary
                    .median
                    .value
                    .filter(|_| summary.median.above.is_none())
            })
            .filter(|rate| rate.is_finite() && *rate > 0.0)
            .max_by(f64::total_cmp)
    };
    compatible
        .iter()
        .find_map(|record| {
            let [spans, logs, native] = SPLIT.map(|(op, _)| rate(record, op));
            Some(Calibration {
                record: record.started_at.to_rfc3339(),
                commit: record.commit.clone(),
                rows_per_cpu_s: [spans?, logs?, native?],
            })
        })
        .ok_or_else(|| {
            format!(
                "no compatible valid L1 `writes-parquet` record measures every write path: {} records read, {} compatible; run `mise run bench:l1:bifrost:writes:parquet` (or `bench:l1:bifrost:writes`) on this host and build first",
                records.len(),
                compatible.len()
            )
        })
}

/// The boundary of every judged L2 query time.
const QUERY_TIME: &str = "server: handler entry until the complete query result is handed off (`bifrost.query`), each query class's own case; nearest-rank bucket bounds of the server's timing histograms";

/// The L2 phase metrics of `phase`, with their criteria on the one step.
fn phase_metrics(phase: &str, step: &[String]) -> (Vec<Metric>, Vec<Criterion>) {
    let id = |name: &str| format!("{phase}.{name}");
    let after = if phase == "recovery" {
        ", read only from 30 s after overload ends, so meeting it means the SLOs were restored within 30 s"
    } else {
        ""
    };
    let metrics = vec![
        Metric::raw(
            &id("point_p99_ms"),
            "ms",
            &format!("{QUERY_TIME}: point lookups{after}"),
        ),
        Metric::raw(
            &id("small_p95_ms"),
            "ms",
            &format!("{QUERY_TIME}: dashboard 100k-row aggregates{after}"),
        ),
        Metric::raw(
            &id("million_p95_ms"),
            "ms",
            &format!("{QUERY_TIME}: dashboard 1M-row aggregates{after}"),
        ),
        Metric::raw(
            &id("table_p95_ms"),
            "ms",
            &format!("{QUERY_TIME}: whole-table aggregates{after}; reported, not judged"),
        ),
        Metric::raw(
            &id("scan_p95_ms"),
            "ms",
            &format!("{QUERY_TIME}: heavy full scans{after}"),
        ),
        Metric::raw(
            &id("write_ack_p99_ms"),
            "ms",
            &format!(
                "server: handler entry until the durable write acknowledgement (`bifrost.gate.write`) or OTLP export response (`bifrost.otlp.traces`, `bifrost.otlp.logs`), every successful write that ended in the phase, the client-queue lanes' included; nearest-rank bucket bounds of the server's timing histograms{after}"
            ),
        ),
        Metric::raw(
            &id("visibility_max_ms"),
            "ms",
            &format!(
                "diagnostic only, client: slowest native acknowledgement until its last row is queryable, censored at the 30 s poll limit; never judged{after}"
            ),
        ),
        Metric::raw(
            &id("backlog_growth_s"),
            "s",
            "growth of unacknowledged offered rows over the phase, in seconds of offered ingest",
        ),
        Metric::raw(
            &id("queries"),
            "queries",
            "exactly answered queries in the phase",
        ),
        Metric::raw(
            &id("ingest_rows"),
            "rows",
            "durably acknowledged ingest rows in the phase",
        ),
    ];
    let criteria = vec![
        Criterion::limit(
            &id("point-p99-slo"),
            &id("point_p99_ms"),
            step,
            Op::Lt,
            POINT_P99_MS,
        )
        .percentile(99),
        Criterion::limit(
            &id("small-p95-slo"),
            &id("small_p95_ms"),
            step,
            Op::Lt,
            100.0,
        )
        .percentile(95),
        Criterion::limit(
            &id("million-p95-slo"),
            &id("million_p95_ms"),
            step,
            Op::Lt,
            300.0,
        )
        .percentile(95),
        Criterion::limit(
            &id("scan-p95-slo"),
            &id("scan_p95_ms"),
            step,
            Op::Lt,
            2_000.0,
        )
        .percentile(95),
        Criterion::limit(
            &id("write-ack-p99-slo"),
            &id("write_ack_p99_ms"),
            step,
            Op::Le,
            100.0,
        )
        .percentile(99),
        Criterion::limit(
            &id("backlog-bounded"),
            &id("backlog_growth_s"),
            step,
            Op::Le,
            1.0,
        ),
    ];
    (metrics, criteria)
}

/// The L2 `competing` declaration at `cores` declared server cores, its
/// ingest rate calibrated from the newest compatible record in `records`,
/// or a smoke's [`SMOKE_INGEST_ROWS_PER_S`] when `records` is `None`.
fn competing_declaration(
    mut base: Declaration,
    cores: f64,
    records: Option<&[Record]>,
) -> Declaration {
    let step = vec![COMPETING.to_owned()];
    COMPETING.clone_into(&mut base.experiment);
    base.question = String::from(
        "With 100M events rows published as Parquet and Forge compaction on, does open-loop mixed ingest at 70% of calibrated capacity beside mixed queries offered open loop at 12/s with at most 32 in flight keep point p99 under 5 ms, dashboard p95 under 100/300 ms, heavy scan p95 under 2 s, server write acknowledgements p99 within 100 ms, with every answer exact, every client-queue row durable exactly once, a bounded backlog, a declared queue-full refusal only under overload, and the SLOs restored within 30 s after it?",
    );
    let calibration = records.map(|records| calibrate(records, &base));
    let rate = match &calibration {
        None => SMOKE_INGEST_ROWS_PER_S,
        Some(Ok(calibration)) => calibration.initial_rate(cores),
        Some(Err(message)) => {
            base.refusals
                .push(Reason::new("calibration.missing", message.clone()));
            0.0
        }
    };
    base.variables
        .insert("ingest_rows_per_s".to_owned(), json!(rate));
    base.controls
        .insert("server_cores".to_owned(), json!(cores));
    base.controls.insert(
        "calibration".to_owned(),
        match &calibration {
            None => json!({ "smoke": "a fixed declared rate; a smoke neither takes nor gives a calibration" }),
            Some(Ok(calibration)) => json!({
                "record": calibration.record,
                "commit": calibration.commit,
                "rows_per_cpu_s": { "spans": calibration.rows_per_cpu_s[0], "logs": calibration.rows_per_cpu_s[1], "native": calibration.rows_per_cpu_s[2] },
                "mixed_rows_per_cpu_s": calibration.mixed_capacity(),
                "headroom": HEADROOM,
            }),
            Some(Err(message)) => json!({ "missing": message }),
        },
    );
    base.controls
        .insert("ingest_rows_per_s".to_owned(), json!(rate));
    base.controls.insert(
        "fixture".to_owned(),
        json!({
            "read": "heavy-100m-parquet: 100,000,000 events rows in bifrost.datasets.events, published as Parquet before the step",
            "ingest": write_fixture(),
            "client_queue": "one Service tenant emitting Drift observations of 100 features (100 rows each) through `WyrdState`",
        }),
    );
    base.controls
        .insert("limits".to_owned(), limits(&PARQUET_ENV));
    base.controls.insert(
        "workload".to_owned(),
        json!({
            "ingest": "open loop by rows: 50% OTLP spans, 30% OTLP logs, 20% native writes; Forge compaction enabled (trigger 2 snapshots) on every ingest table",
            "queries": "open loop at 12 queries/s independent of answers, at most 32 in flight (a due query finding 32 unanswered is missed): 60% point lookup, 25% dashboard (15% 100k-row, 10% 1M-row aggregate), 10% whole-table aggregate, 5% full scan",
            "dashboard_split": { "small": 0.6, "million": 0.4 },
            "client_queue": "three lanes, each a fresh WyrdState per repetition: headroom 1,000 obs/s, ack-delay 500 obs/s behind a 50 ms acknowledgement proxy, in-flight 500 obs/s with max_in_flight 1",
            "phases": "capacity 5/12 of the window; overload fills Oracle's 1,000 queue places behind held slots while ingest continues, then one more query; recovery to the window's end",
            "admission_refusal": run::QUEUE_FULL,
        }),
    );
    base.steps = vec![Step::measured(
        COMPETING,
        json!({ "op": "competing", "query_clients": competing::QUERY_CLIENTS, "query_rate": competing::QUERY_RATE, "ingest_rows_per_s": rate }),
    )];
    let mut metrics = Vec::new();
    let mut criteria = Vec::new();
    for phase in ["capacity", "recovery"] {
        let (phase_metrics, phase_criteria) = phase_metrics(phase, &step);
        metrics.extend(phase_metrics);
        criteria.extend(phase_criteria);
    }
    metrics.extend([
        Metric::raw("overload.queued", "queries", "highest `oracle_queries_queued` while filling"),
        Metric::raw("overload.overflow_refused", "count", "1 when the query sent into the full queue was refused with the queue-full code"),
        Metric::raw("overload.queue_drain_s", "s", "disconnecting the waiters until the queue is empty"),
        Metric::raw("overload.queue_full", "refusals", "queue-full refusals mixed queries saw from overload start until 30 s after it"),
        Metric::raw("errors", "errors", "failed requests of every code, except queue-full during overload and its 30 s grace"),
        Metric::raw("wrong_results", "queries", "answered queries whose rows differ from the exact answer"),
        Metric::raw("drain_s", "s", "window end until every in-flight ingest request is acknowledged"),
        Metric::raw("queries", "queries", "exactly answered mixed queries in the window"),
        Metric::raw("table_aggregates", "queries", "exactly answered whole-table aggregates in the window"),
        Metric::raw("ingest_rows", "rows", "durably acknowledged ingest rows in the window"),
        Metric::raw("queries_after_window", "queries", "exactly answered mixed queries in flight at the window end, answered after it and outside every work-per-CPU metric"),
        Metric::raw("scanned_rows_per_query", "rows/query", "mean rows each in-window mixed query's own executed-plan file scans output (`bifrost.oracle.stream` `rows_scanned` on its trace)"),
        Metric::raw("scanned_bytes_per_query", "B/query", "mean bytes each in-window mixed query's own executed plan scanned (`bifrost.oracle.stream` `bytes_scanned` on its trace)"),
        Metric::raw("retained_mib", "MiB", "retained Arrow MiB the server inserted from the window open to its measured stop, the CPU interval, never during the drain (`bifrost_scribe_memtable_bytes_inserted_total`), including client-queue lanes and server-internal rows"),
        Metric::raw("otlp_wire_mib", "MiB", "encoded OTLP protobuf MiB of ingest acknowledged before the window end, before framing and compression"),
        Metric::raw("ingest_rows_after_window", "rows", "ingest rows in flight at the window end, acknowledged during the drain and outside every work-per-CPU metric"),
        Metric::raw("p50_ms", "ms", "server: capacity-phase point lookup handler entry until its result handoff (`bifrost.query`), exact nanoseconds (12 queries/s, at most 32 in flight, not the L1 64-client objective)"),
        Metric::raw("p95_ms", "ms", "server: capacity-phase point lookup handler entry until its result handoff (`bifrost.query`), exact nanoseconds (12 queries/s, at most 32 in flight)"),
        Metric::raw("p99_ms", "ms", "server: capacity-phase point lookup handler entry until its result handoff (`bifrost.query`), exact nanoseconds (12 queries/s, at most 32 in flight)"),
        Metric::derived("rate", "queries/s", "exactly answered mixed queries per wall second", Direction::Higher, Derive::Rate { work: "queries".to_owned(), scale: 1.0 }),
        Metric::derived("queries_per_cpu_s", "queries/cpu-s", "mixed queries per server CPU-second; the denominator also pays for ingest", Direction::Higher, Derive::PerCpu { work: "queries".to_owned(), scale: 1.0 }),
        Metric::derived("table_aggregates_per_cpu_s", "queries/cpu-s", "whole-table aggregates per server CPU-second; the denominator also pays for everything else", Direction::Higher, Derive::PerCpu { work: "table_aggregates".to_owned(), scale: 1.0 }),
        Metric::derived("ingest_rows_per_cpu_s", "rows/cpu-s", "ingest rows per server CPU-second; the denominator also pays for queries", Direction::Higher, Derive::PerCpu { work: "ingest_rows".to_owned(), scale: 1.0 }),
        Metric::derived("queries_per_s_per_vcpu", "queries/s/vCPU", "mixed queries per wall second per provisioned logical CPU, (W/T)/P", Direction::Higher, Derive::PerVcpu { work: "queries".to_owned(), scale: 1.0 }),
        Metric::derived("mib_per_s_per_vcpu", "MiB/s/vCPU", "(W/T)/P: retained Arrow MiB per wall second per provisioned logical CPU", Direction::Higher, Derive::PerVcpu { work: "retained_mib".to_owned(), scale: 1.0 }),
        Metric::derived("mib_per_cpu_s", "MiB/cpu-s", "W/C: retained Arrow MiB per server CPU-second; the denominator also pays for queries", Direction::Higher, Derive::PerCpu { work: "retained_mib".to_owned(), scale: 1.0 }),
        Metric::derived("wire_mib_per_s_per_vcpu", "MiB/s/vCPU", "(W/T)/P: encoded OTLP wire MiB per wall second per provisioned logical CPU", Direction::Higher, Derive::PerVcpu { work: "otlp_wire_mib".to_owned(), scale: 1.0 }),
        Metric::derived("wire_mib_per_cpu_s", "MiB/cpu-s", "W/C: encoded OTLP wire MiB per server CPU-second; the denominator also pays for native ingest and queries", Direction::Higher, Derive::PerCpu { work: "otlp_wire_mib".to_owned(), scale: 1.0 }),
        Metric::derived("cpu_ms_per_query", "cpu-ms/query", "server CPU per mixed query, C/W; the numerator also pays for ingest", Direction::Lower, Derive::CpuPer { work: "queries".to_owned(), scale: 1e3 }),
        Metric::derived("ingest_rows_per_s_per_vcpu", "rows/s/vCPU", "ingest rows per wall second per provisioned logical CPU, (W/T)/P", Direction::Higher, Derive::PerVcpu { work: "ingest_rows".to_owned(), scale: 1.0 }),
        Metric::derived("cpu_us_per_ingest_row", "cpu-µs/row", "server CPU per ingest row, C/W; the numerator also pays for queries", Direction::Lower, Derive::CpuPer { work: "ingest_rows".to_owned(), scale: 1e6 }),
    ]);
    criteria.extend([
        Criterion::every(
            "overload.queue-filled",
            "overload.queued",
            &step,
            Op::Ge,
            f64::from(u32::try_from(run::QUEUE_PLACES).unwrap_or(u32::MAX)),
        ),
        Criterion::every(
            "overload.overflow-refused",
            "overload.overflow_refused",
            &step,
            Op::Ge,
            1.0,
        ),
        Criterion::every(
            "overload.queue-drained",
            "overload.queue_drain_s",
            &step,
            Op::Le,
            60.0,
        ),
        Criterion::every("no-errors", "errors", &step, Op::Le, 0.0),
        Criterion::every("exact-answers", "wrong_results", &step, Op::Le, 0.0),
        Criterion::every("drained", "drain_s", &step, Op::Le, 60.0),
    ]);
    let message_bytes = wyrd_client::QueueConfig::default().max_message_bytes;
    for lane in crate::client_queue::LANES {
        let id = |name: &str| format!("queue.{}.{name}", lane.name);
        metrics.extend([
            Metric::raw(&id("refusals"), "observations", "client queue-full refusals outside overload and its 30 s grace"),
            Metric::raw(&id("overload_refusals"), "observations", "client queue-full refusals during overload and its 30 s grace"),
            Metric::raw(&id("row_mismatch"), "rows", "|durable − expected| rows under the lane's run_id, plus record_ids without exactly 100 rows"),
            Metric::raw(&id("bytes_growth"), "B", "least-squares growth of client-owned bytes over the last 10 s of emission"),
            Metric::raw(&id("drain_s"), "s", "`flush` once emission stops"),
            Metric::raw(&id("achieved_rate"), "observations/s", "admitted observations per second of emission"),
        ]);
        criteria.extend([
            Criterion::every(&id("no-refusals"), &id("refusals"), &step, Op::Le, 0.0),
            Criterion::every(&id("exactly-once"), &id("row_mismatch"), &step, Op::Le, 0.0),
            Criterion::limit(
                &id("bytes-bounded"),
                &id("bytes_growth"),
                &step,
                Op::Le,
                message_bytes as f64,
            ),
            Criterion::every(&id("drained"), &id("drain_s"), &step, Op::Le, 60.0),
        ]);
    }
    base.metrics = metrics;
    base.criteria = criteria;
    base
}

/// A tenant-scoped public client of the local server, presenting the
/// server's setup credential as the guide's `WYRD_API_KEY`.
///
/// # Errors
///
/// Returns the client configuration failure.
fn client(server: &LocalServer) -> Result<WyrdClient> {
    Ok(WyrdClient::with_config(ClientConfig {
        credential: Some(server.api_key().clone()),
        ..ClientConfig::from_global_with_overrides(&GlobalConfig::default(), Some(SERVER_URL), None)
    })?)
}

/// Starts the release server with Scribe settings `settings`.
///
/// # Errors
///
/// Returns the binary lookup or start failure.
async fn start(settings: &[(&str, &str)]) -> Result<LocalServer> {
    let binary = wyrd_testing::capacity::release_binary()?;
    Ok(LocalServer::start(&binary, &["bench"], settings).await?)
}

/// One write path's sender and the cursor every later window continues.
pub type Lane = (WriteOp, run::Sender, Arc<AtomicI64>);

/// Every write path of `ops`, in that order, over the public `client`, each
/// cursor at zero.
///
/// # Errors
///
/// Returns the native writer's connection or registration failure, or the
/// OTLP connection failure.
async fn lanes(client: &WyrdClient, ops: &[WriteOp]) -> Result<Vec<Lane>> {
    let otlp = run::Otlp::connect(client).await?;
    let mut lanes = Vec::new();
    for op in ops {
        let sender = match op {
            WriteOp::Native => {
                run::Sender::native(
                    client,
                    workload::INGEST_TABLE,
                    workload::Fixture {
                        rows: WRITE_FIXTURE_ROWS,
                    },
                )
                .await?
            }
            WriteOp::Spans => run::Sender::Spans(
                otlp.clone(),
                telemetry::Telemetry {
                    spans: WRITE_FIXTURE_ROWS,
                },
            ),
            WriteOp::Logs => run::Sender::Logs(
                otlp.clone(),
                logs::Logs {
                    records: WRITE_FIXTURE_ROWS,
                },
            ),
        };
        lanes.push((*op, sender, Arc::new(AtomicI64::new(0))));
    }
    Ok(lanes)
}

/// Where Scribe holds rows, from one `/metrics` scrape and the store.
#[derive(Debug)]
struct Residence {
    /// Arrow bytes in active and immutable memtables.
    memtable_bytes: f64,
    /// Durable staged members not yet published.
    staged_members: f64,
    /// Claims taken but not yet published.
    outstanding_claims: f64,
    /// Published Parquet objects in the store.
    published_files: u64,
}

impl Residence {
    /// Reads the server's memtable, staging, and claim gauges and counts the
    /// store's Parquet objects.
    ///
    /// # Errors
    ///
    /// Returns the scrape failure.
    async fn read(server: &LocalServer) -> Result<Self> {
        let metrics = server.metrics().await?;
        Ok(Self {
            memtable_bytes: metrics.sum("bifrost_scribe_active_memtable_bytes", &[])
                + metrics.sum("bifrost_scribe_immutable_memtable_bytes", &[]),
            staged_members: metrics.sum("bifrost_scribe_staging_live_members", &[]),
            outstanding_claims: metrics.sum("bifrost_scribe_staging_outstanding_claims", &[]),
            published_files: parquet_files(&server.storage_dir()),
        })
    }

    /// Why a row left its memtable, or `None` while every row is held in
    /// one: nothing staged, claimed, or published.
    fn left_memtable(&self) -> Option<String> {
        (self.staged_members > 0.0 || self.outstanding_claims > 0.0 || self.published_files > 0)
            .then(|| {
                format!(
                    "rows left the memtable: {} staged, {} claims, {} published files",
                    self.staged_members, self.outstanding_claims, self.published_files
                )
            })
    }

    /// Whether every row is published: nothing in a memtable, staged, or
    /// claimed, and at least one Parquet object in the store.
    fn all_published(&self) -> bool {
        self.memtable_bytes == 0.0
            && self.staged_members == 0.0
            && self.outstanding_claims == 0.0
            && self.published_files > 0
    }

    /// Waits until every written row is published as Parquet, polling every
    /// second for at most `timeout`.
    ///
    /// # Errors
    ///
    /// Returns a scrape failure, or a message saying where the rows still
    /// were when `timeout` passed.
    async fn settle(server: &LocalServer, timeout: Duration) -> Result<()> {
        let started = Instant::now();
        loop {
            let residence = Self::read(server).await?;
            if residence.all_published() {
                tracing::info!(
                    seconds = started.elapsed().as_secs_f64(),
                    "fixture published"
                );
                return Ok(());
            }
            if started.elapsed() >= timeout {
                return Err(format!(
                    "not all published after {}s: memtable {:.0} MiB, {} staged, {} claims, {} published files",
                    timeout.as_secs(),
                    residence.memtable_bytes / f64::from(1 << 20),
                    residence.staged_members,
                    residence.outstanding_claims,
                    residence.published_files
                )
                .into());
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

/// The published Parquet files of `table` under the `file://` store rooted
/// at `root`.
pub fn table_files(root: &Path, table: &str) -> Vec<std::path::PathBuf> {
    let segment = segment(table);
    parquet(root)
        .into_iter()
        .map(|(path, _)| path)
        .filter(|path| path.to_string_lossy().contains(&segment))
        .collect()
}

/// The `/<segment>/<name>/` key part of `table`'s objects: the name without
/// its `bifrost` catalog, which object keys do not carry.
fn segment(table: &str) -> String {
    let name = table.strip_prefix("bifrost.").unwrap_or(table);
    format!("/{}/", name.replace('.', "/"))
}

/// Counts the Parquet objects under the `file://` store rooted at `root`.
fn parquet_files(root: &Path) -> u64 {
    parquet(root).len() as u64
}

/// Sums the sizes of `table`'s Parquet objects under the `file://` store
/// rooted at `root`, whose keys hold `/<segment>/<name>/` after the tenant.
pub fn parquet_bytes(root: &Path, table: &str) -> u64 {
    let segment = segment(table);
    parquet(root)
        .iter()
        .filter(|(path, _)| path.to_string_lossy().contains(&segment))
        .map(|(_, bytes)| bytes)
        .sum()
}

/// Every Parquet object under the `file://` store rooted at `root`, with
/// its size.
fn parquet(root: &Path) -> Vec<(std::path::PathBuf, u64)> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for path in std::fs::read_dir(&directory)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
        {
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "parquet")
            {
                let bytes = std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
                found.push((path, bytes));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A table's stored bytes sum only its own Parquet objects, never a
    /// sibling table's or a non-Parquet object.
    ///
    /// # Panics
    ///
    /// Panics when another object is counted.
    #[test]
    fn parquet_bytes_count_only_the_tables_own_objects() {
        let root = tempfile::tempdir().expect("store");
        for (key, bytes) in [
            ("tenants/t/traces/spans/data/a.parquet", 3),
            ("tenants/t/traces/spans/data/b.parquet", 4),
            ("tenants/t/traces/spans/metadata/v1.json", 50),
            ("tenants/t/traces/spans_old/data/c.parquet", 100),
            ("tenants/t/logs/records/data/d.parquet", 200),
        ] {
            let path = root.path().join(key);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            std::fs::write(path, vec![0; bytes]).expect("object");
        }
        assert_eq!(parquet_bytes(root.path(), WriteOp::Spans.table()), 7);
        assert_eq!(parquet_bytes(root.path(), WriteOp::Native.table()), 0);
    }

    /// The harmonic mix weights each path by its share of rows, and the
    /// initial rate is 70% of it per declared core.
    ///
    /// # Panics
    ///
    /// Panics when the arithmetic drifts.
    #[test]
    fn calibration_mixes_harmonically() {
        let calibration = Calibration {
            record: String::new(),
            commit: String::new(),
            rows_per_cpu_s: [100_000.0, 30_000.0, 400_000.0],
        };
        let mixed = 1.0 / (0.5 / 100_000.0 + 0.3 / 30_000.0 + 0.2 / 400_000.0);
        assert!((calibration.mixed_capacity() - mixed).abs() < 1e-6);
        assert!((calibration.initial_rate(8.0) - 0.7 * mixed * 8.0).abs() < 1e-6);
        let equal = Calibration {
            rows_per_cpu_s: [1_000.0; 3],
            ..calibration
        };
        assert!((equal.mixed_capacity() - 1_000.0).abs() < 1e-9);
    }

    /// `--server-cores` defaults to eight and refuses a non-positive value.
    ///
    /// # Panics
    ///
    /// Panics when the flag is misread.
    #[test]
    fn server_cores_defaults_and_validates() {
        let arguments = |rest: &[&str]| -> Vec<String> {
            std::iter::once("capacity")
                .chain(rest.iter().copied())
                .map(str::to_owned)
                .collect()
        };
        assert_eq!(server_cores(&arguments(&[])), Ok(SERVER_CORES));
        assert_eq!(server_cores(&arguments(&["--server-cores", "4"])), Ok(4.0));
        assert!(server_cores(&arguments(&["--server-cores", "0"])).is_err());
        assert!(server_cores(&arguments(&["--server-cores"])).is_err());
    }
}
