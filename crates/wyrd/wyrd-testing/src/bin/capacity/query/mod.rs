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
//! - **L1, `isolated`** ([`isolated`]): the standard 10M-row events fixture
//!   published as Parquet, then closed-loop point lookups at 1/4/16/64
//!   clients, small aggregates and full scans at 1/64, and write sweeps at
//!   1/4/16/64 clients for native Arrow writes, OTLP spans, and OTLP logs,
//!   each repetition a full window of fresh identities.
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
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Value, json};
use wyrd_client::config::ClientConfig;
use wyrd_client::{Bifrost, GlobalConfig, WyrdClient};
use wyrd_testing::capacity::{Collector, Selection, Smoke, Window, request_trace};
use wyrd_testing::experiment::{
    Baselines, Criterion, Declaration, Derive, Direction, Engine, Level, Metric, Op, Reason,
    Record, Status, Step, Verdict,
};
use wyrd_testing::release_server::{LocalServer, SERVER_URL};

use crate::levels::{Context, Plan};

/// Error type of the server and client steps a workload runs.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Closed-loop client counts of the point and write sweeps.
pub const SWEEP: [u64; 4] = [1, 4, 16, 64];

/// Client counts of the small-aggregate and full-scan pairs.
pub const PAIR: [u64; 2] = [1, 64];

/// Rows the L1 native and OTLP write fixtures spread over their day; a
/// window's rows continue past it, so every repetition writes fresh rows.
pub const WRITE_FIXTURE_ROWS: i64 = 2_000_000;

/// Strict point-lookup p99 objective, milliseconds; equality fails.
pub const POINT_P99_MS: f64 = 5.0;

/// Native write peak sanity floor, rows per second.
pub const NATIVE_PEAK_FLOOR: f64 = 200_000.0;

/// OTLP write peak sanity floor, encoded wire MiB per server CPU-second.
pub const OTLP_PEAK_FLOOR: f64 = 30.0;

/// The L1 experiment id.
pub const ISOLATED: &str = "isolated";

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

/// The three ingest paths an experiment writes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOp {
    /// Native Arrow writes into `vala.datasets.events_ingest`.
    Native,
    /// OTLP spans into `vala.traces.spans`.
    Spans,
    /// OTLP bulky Kubernetes logs into `vala.logs.records`.
    Logs,
}

impl WriteOp {
    /// Every path, in L1 sweep order.
    pub const ALL: [Self; 3] = [Self::Native, Self::Spans, Self::Logs];

    /// The table this path writes, as `vala.<segment>.<name>`.
    #[must_use]
    pub const fn table(self) -> &'static str {
        match self {
            Self::Native => workload::INGEST_TABLE,
            Self::Spans => "vala.traces.spans",
            Self::Logs => "vala.logs.records",
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

    /// The baselines entry prefix and fixture this path compares against.
    const fn baseline(self) -> (&'static str, &'static str) {
        match self {
            Self::Native => ("native-writes.write", "native-2m"),
            Self::Spans => ("otlp-spans-writes.otlp-spans", "spans-2m"),
            Self::Logs => ("otlp-logs-writes.otlp-logs", "logs-2m"),
        }
    }
}

/// The L1 read operations and the case each runs.
const READS: [(&str, workload::Case, &[u64]); 3] = [
    ("point", workload::Case::Selective, &SWEEP),
    ("small-aggregate", workload::Case::SmallAggregate, &PAIR),
    ("full-scan", workload::Case::FullScan, &PAIR),
];

/// The read case an `op` step parameter names.
fn read_case(name: &str) -> Option<workload::Case> {
    READS
        .iter()
        .find(|(op, ..)| *op == name)
        .map(|(_, case, _)| *case)
}

/// Plans the Bifrost experiment at the context's level: refuses unknown
/// flags, declares it, and builds the workload, which starts the server only
/// once the run admits it.
///
/// # Errors
///
/// Returns a message for an unknown flag or an invalid `--server-cores`.
pub fn plan(context: &Context<'_>, base: Declaration) -> std::result::Result<Plan, String> {
    let valued: &[&str] = match context.level {
        Level::One => &[],
        Level::Two => &["--server-cores"],
    };
    context.known(valued, &[])?;
    let declaration = declare(context.level, context.arguments, base, context.root)?;
    let logs = context
        .root
        .join("target/benchmarks")
        .join(Engine::Bifrost.name())
        .join(&declaration.experiment);
    let workload: Box<dyn wyrd_testing::experiment::Workload> = match context.level {
        Level::One => Box::new(isolated::Isolated::new(logs)),
        Level::Two => Box::new(competing::Competing::new(logs)),
    };
    Ok(Plan {
        declaration,
        workload,
    })
}

/// The Bifrost declaration at `level` over `base`, reading `--server-cores`
/// from `arguments` (program name first), the baselines under `root`, and,
/// at L2, the L1 records under `root/benchmarks/bifrost/isolated/`. Pure
/// otherwise: it starts nothing.
///
/// A missing or unusable calibration is a declaration refusal
/// (`calibration.missing`), never a substituted rate.
///
/// # Errors
///
/// Returns a message for a `--server-cores` that is not a positive number.
pub fn declare(
    level: Level,
    arguments: &[String],
    base: Declaration,
    root: &Path,
) -> std::result::Result<Declaration, String> {
    let baselines = Baselines::load(root, Engine::Bifrost);
    let mut declaration = match level {
        Level::One => isolated_declaration(base, baselines.as_ref().ok()),
        Level::Two => {
            let cores = server_cores(arguments)?;
            let records = l1_records(root);
            competing_declaration(base, cores, &records)
        }
    };
    let note = match &baselines {
        Ok(baselines) => format!("{} entries loaded", baselines.baseline.len()),
        Err(error) => format!("unavailable: {error}"),
    };
    declaration
        .controls
        .insert("baselines".to_owned(), Value::String(note));
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

/// Every readable record under `root/benchmarks/bifrost/isolated/`; an
/// unreadable one is skipped, since calibration only needs a valid one.
fn l1_records(root: &Path) -> Vec<Record> {
    let directory = root
        .join("benchmarks")
        .join(Engine::Bifrost.name())
        .join(ISOLATED);
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

/// The fixture control every L1 record declares, which a calibrating L2
/// must match.
fn l1_fixture() -> Value {
    json!({
        "read": "standard-10m-parquet: 10,000,000 events rows in vala.datasets.events, published as Parquet before any read step",
        "native": format!("native-2m: events rows into vala.datasets.events_ingest, {WRITE_FIXTURE_ROWS} rows per day, fresh ids every window"),
        "spans": format!("spans-2m: GenAI/HTTP spans into vala.traces.spans, {WRITE_FIXTURE_ROWS} spans per day, fresh ids every window"),
        "logs": format!("logs-2m: bulky Kubernetes logs into vala.logs.records, {WRITE_FIXTURE_ROWS} records per day, fresh ids every window"),
    })
}

/// The limits control both levels declare.
fn limits() -> Value {
    json!(
        "server defaults on the whole host except WYRD_MAX_FILE_RETENTION_TIME=30; no resource quota"
    )
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
            "uncompressed Arrow bytes Scribe inserted into memtables (`bifrost_scribe_memtable_bytes_inserted_total`), 2^20 bytes; write steps only",
        ),
        Metric::raw(
            "wire_bytes",
            "B",
            "encoded OTLP protobuf bytes of acknowledged exports, before framing and compression; OTLP steps only",
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
const SERVER_TIME: &str = "server handler entry until the complete query result is handed off (`bifrost.query`), Gate's durable write acknowledgement (`bifrost.gate.write`), or the OTLP export response (`bifrost.otlp.traces`, `bifrost.otlp.logs`): successful requests' exact `wyrd.timing` nanoseconds, nearest rank";

/// The boundary of the diagnostic client round trip, which never judges.
const CLIENT_TIME: &str =
    "diagnostic only: client send until its terminal or durable acknowledgement, never judged";

/// What `server_failures` counts.
const SERVER_FAILURES: &str = "server timings at the step's boundary whose outcome is not `success`, kept with their durations in timings.jsonl and never judged";

/// The L1 `isolated` declaration: the read sweeps, then the write sweeps,
/// their derived scaling and peak steps, the exactness, error, SLO, and
/// sanity-floor criteria, and every baseline comparison.
fn isolated_declaration(mut base: Declaration, baselines: Option<&Baselines>) -> Declaration {
    ISOLATED.clone_into(&mut base.experiment);
    base.question = String::from(
        "With 10M events rows published as Parquet, does a point lookup keep p99 strictly under 5 ms at 64 closed-loop clients with every answer exact, what does each read and write path cost the server per CPU-second at 1/4/16/64 clients, and do native and OTLP write peaks clear their sanity floors?",
    );
    base.variables.insert("clients".to_owned(), json!(SWEEP));
    base.controls.insert("fixture".to_owned(), l1_fixture());
    base.controls.insert("limits".to_owned(), limits());
    base.controls.insert(
        "workload".to_owned(),
        json!("closed-loop public-client clients, each sending its next request once the last answers: Bifrost SQL point lookups, 100k-row aggregates, and full scans checked against exact answers; native `write_batch` of 16,384 rows; OTLP exports of 512 spans or log records with the client's exchanged bearer"),
    );
    base.metrics = isolated_metrics();
    let mut reads = Vec::new();
    for (op, _, clients) in READS {
        for clients in clients {
            let id = format!("{op}-c{clients}");
            base.steps
                .push(Step::measured(&id, json!({ "op": op, "clients": clients })));
            reads.push(id);
        }
    }
    let mut writes = Vec::new();
    for op in WriteOp::ALL {
        for clients in SWEEP {
            let id = format!("{}-c{clients}", op.name());
            base.steps.push(Step::measured(
                &id,
                json!({ "op": op.name(), "clients": clients }),
            ));
            writes.push(id);
        }
    }
    for (op, ..) in READS {
        base.steps.push(Step::scaling(
            &format!("{op}-scaling"),
            &format!("{op}-c1"),
            &format!("{op}-c64"),
            64.0,
            "rate",
        ));
    }
    for op in WriteOp::ALL {
        let sweep: Vec<String> = SWEEP
            .iter()
            .map(|clients| format!("{}-c{clients}", op.name()))
            .collect();
        base.steps.push(Step::peak(
            &format!("{}-peak", op.name()),
            &sweep,
            "rows_per_s",
        ));
    }
    let measured: Vec<String> = reads.iter().chain(&writes).cloned().collect();
    base.criteria = vec![
        Criterion::every("exact-answers", "wrong_results", &reads, Op::Le, 0.0),
        Criterion::every("no-errors", "errors", &measured, Op::Le, 0.0),
        Criterion::limit(
            "point-p99-slo",
            "p99_ms",
            &["point-c64".to_owned()],
            Op::Lt,
            POINT_P99_MS,
        )
        .percentile(99),
        Criterion::limit(
            "native-peak-floor",
            "rows_per_s",
            &["native-write-peak".to_owned()],
            Op::Ge,
            NATIVE_PEAK_FLOOR,
        ),
        Criterion::limit(
            "otlp-spans-peak-floor",
            "wire_mib_per_cpu_s",
            &["otlp-spans-peak".to_owned()],
            Op::Ge,
            OTLP_PEAK_FLOOR,
        ),
        Criterion::limit(
            "otlp-logs-peak-floor",
            "wire_mib_per_cpu_s",
            &["otlp-logs-peak".to_owned()],
            Op::Ge,
            OTLP_PEAK_FLOOR,
        ),
    ];
    let baselines = baselines.cloned().unwrap_or_default();
    let host = base.host.fingerprint.clone();
    let mut compare = |metric: &str, step: &str, entry: String, fixture: &str| {
        if let Some(metric) = base.metrics.iter().find(|declared| declared.id == metric) {
            let criterion = baselines.criterion(
                &format!("baseline.{entry}"),
                metric,
                step,
                &entry,
                &host,
                fixture,
            );
            base.criteria.push(criterion);
        }
    };
    let read_baselines: [(&str, &str, &[u64]); 3] = [
        ("point", "selective", &SWEEP),
        ("small-aggregate", "small-aggregate", &PAIR),
        ("full-scan", "full-scan", &[1]),
    ];
    for (op, case, clients) in read_baselines {
        for clients in clients {
            let step = format!("{op}-c{clients}");
            let entry = format!("standard-parquet.{case}.c{clients}");
            compare(
                "queries_per_cpu_s",
                &step,
                format!("{entry}.queries-per-cpu-s"),
                "standard-10m-parquet",
            );
            compare(
                "cpu_ms_per_query",
                &step,
                format!("{entry}.cpu-us-per-query"),
                "standard-10m-parquet",
            );
        }
    }
    for op in WriteOp::ALL {
        let (prefix, fixture) = op.baseline();
        for clients in SWEEP {
            let step = format!("{}-c{clients}", op.name());
            let entry = format!("{prefix}.c{clients}");
            compare(
                "rows_per_cpu_s",
                &step,
                format!("{entry}.rows-per-cpu-s"),
                fixture,
            );
            compare(
                "cpu_us_per_row",
                &step,
                format!("{entry}.cpu-us-per-row"),
                fixture,
            );
            compare(
                "mib_per_cpu_s",
                &step,
                format!("{entry}.mb-per-cpu-s"),
                fixture,
            );
            if op != WriteOp::Native {
                compare(
                    "wire_mib_per_cpu_s",
                    &step,
                    format!("{entry}.otlp-wire-mib-per-cpu-s"),
                    fixture,
                );
            }
        }
    }
    base
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
/// Bifrost level-1 `isolated` record from the same host fingerprint and
/// build profile, flags, and compiler, with the current L1 fixture and
/// limits controls, no validity reason, and a measured non-INVALID sweep step
/// of every write path. Each path's rate is the highest median
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
                && record.experiment == ISOLATED
                && record.host.fingerprint == base.host.fingerprint
                && ["profile", "rustflags", "compiler"]
                    .iter()
                    .all(|key| build(&record.build, key) == build(&base.build, key))
                && record.controls.values.get("fixture") == Some(&l1_fixture())
                && record.controls.values.get("limits") == Some(&limits())
                && record.validity.reasons.is_empty()
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
                "no compatible valid L1 `isolated` record measures every write path: {} records read, {} compatible; run `mise run bench:l1:bifrost` on this host and build first",
                records.len(),
                compatible.len()
            )
        })
}

/// The boundary of every judged L2 query time.
const QUERY_TIME: &str = "server: handler entry until the complete query result is handed off (`bifrost.query`), the query's own observation by trace id; exact nanoseconds, nearest rank";

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
                "server: handler entry until the durable write acknowledgement (`bifrost.gate.write`) or OTLP export response (`bifrost.otlp.traces`, `bifrost.otlp.logs`), every successful write that ended in the phase, the client-queue lanes' included; exact nanoseconds, nearest rank{after}"
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
        Criterion::every(
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
/// ingest rate calibrated from the newest compatible record in `records`.
fn competing_declaration(mut base: Declaration, cores: f64, records: &[Record]) -> Declaration {
    let step = vec![COMPETING.to_owned()];
    COMPETING.clone_into(&mut base.experiment);
    base.question = String::from(
        "With 100M events rows published as Parquet and Forge compaction on, does open-loop mixed ingest at 70% of calibrated capacity beside mixed queries offered open loop at 12/s with at most 32 in flight keep point p99 under 5 ms, dashboard p95 under 100/300 ms, heavy scan p95 under 2 s, server write acknowledgements p99 within 100 ms, with every answer exact, every client-queue row durable exactly once, a bounded backlog, a declared queue-full refusal only under overload, and the SLOs restored within 30 s after it?",
    );
    let calibration = calibrate(records, &base);
    let rate = match &calibration {
        Ok(calibration) => calibration.initial_rate(cores),
        Err(message) => {
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
            Ok(calibration) => json!({
                "record": calibration.record,
                "commit": calibration.commit,
                "rows_per_cpu_s": { "spans": calibration.rows_per_cpu_s[0], "logs": calibration.rows_per_cpu_s[1], "native": calibration.rows_per_cpu_s[2] },
                "mixed_rows_per_cpu_s": calibration.mixed_capacity(),
                "headroom": HEADROOM,
            }),
            Err(message) => json!({ "missing": message }),
        },
    );
    base.controls
        .insert("ingest_rows_per_s".to_owned(), json!(rate));
    base.controls.insert(
        "fixture".to_owned(),
        json!({
            "read": "heavy-100m-parquet: 100,000,000 events rows in vala.datasets.events, published as Parquet before the step",
            "ingest": l1_fixture(),
            "client_queue": "one Service tenant emitting Drift observations of 100 features (100 rows each) through `WyrdState`",
        }),
    );
    base.controls.insert("limits".to_owned(), limits());
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
        Metric::raw("retained_mib", "MiB", "retained Arrow MiB the server inserted while the window was open (`bifrost_scribe_memtable_bytes_inserted_total`), including client-queue lanes and server-internal rows"),
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
            Criterion::every(
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

/// Starts the release server with every row published as Parquet and
/// every span exported to `collector`.
///
/// # Errors
///
/// Returns the binary lookup or start failure.
async fn start(collector: &Collector) -> Result<LocalServer> {
    let binary = wyrd_testing::capacity::release_binary()?;
    let tracing = collector.server_env();
    let variables: Vec<(&str, &str)> = PARQUET_ENV
        .into_iter()
        .chain(tracing.iter().map(|(name, value)| (*name, value.as_str())))
        .collect();
    Ok(LocalServer::start(&binary, &["bench"], &variables).await?)
}

/// One write path's sender and the cursor every later window continues.
pub type Lane = (WriteOp, run::Sender, Arc<AtomicI64>);

/// Requests the release smoke sends through each write path.
const SMOKE_WRITES: i64 = 2;

/// Queries the release smoke may send before the smoked native rows answer
/// exactly, [`SMOKE_POLL`] apart.
const SMOKE_QUERIES: u64 = 50;

/// How long the smoke waits for its rows to publish as Parquet: a write far
/// below Scribe's size triggers waits out the declared
/// `WYRD_MAX_FILE_RETENTION_TIME` of 30 s twice, once before its generation
/// rotates and once before its staged members assemble, plus 30 s for the
/// flush and publication.
const SMOKE_SETTLE: Duration = Duration::from_secs(90);

/// How long the release smoke waits between unanswered queries.
const SMOKE_POLL: Duration = Duration::from_millis(100);

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

/// The Bifrost release smoke over the started `server`: [`SMOKE_WRITES`]
/// requests through every lane, advancing its cursor so later windows write
/// fresh identities, a wait of at most [`SMOKE_SETTLE`] for their Parquet
/// publication, since only physical scans count scanned rows, then exact
/// `count(*)` queries over the smoked native
/// rows until one answers, all inside one window. Every request must reach
/// `collector` as exactly one successful timing at its boundary, each write
/// matched by the trace id it carried, with positive server CPU and inserted
/// memtable bytes, and every answered query's own executed-plan scan on its
/// trace, reporting positive rows and its bytes.
///
/// # Errors
///
/// Returns the scrape failure, or the [`Smoke`] message naming every missing
/// piece of evidence.
async fn smoke(
    server: &LocalServer,
    collector: &Collector,
    client: &WyrdClient,
    lanes: &[Lane],
) -> std::result::Result<String, String> {
    let mut smoke = Smoke::new("bifrost");
    let from = SystemTime::now();
    let (window, before) = Window::open(server, collector, Selection::RESOURCES)
        .await
        .map_err(|error| error.to_string())?;
    let mut expected = Vec::new();
    let mut traces = Vec::new();
    let mut native_rows = 0;
    for (op, sender, cursor) in lanes {
        let chunk = sender.request_rows();
        for _ in 0..SMOKE_WRITES {
            let first = cursor.fetch_add(chunk, Ordering::Relaxed);
            let trace = request_trace();
            let sent = sender.send(first, first + chunk, &trace).await;
            traces.push(trace.trace_id);
            match sent {
                Ok(wire) if *op != WriteOp::Native => {
                    smoke.positive(&format!("{} wire bytes", op.name()), wire as f64);
                }
                Ok(_) => native_rows += chunk,
                Err(code) => {
                    smoke.missing(format!("a `{}` write was refused with `{code}`", op.name()));
                }
            }
        }
        expected.push((op.boundary(), SMOKE_WRITES.cast_unsigned()));
    }
    if let Err(error) = Residence::settle(server, SMOKE_SETTLE).await {
        smoke.missing(format!(
            "the smoked rows were not published as Parquet: {error}"
        ));
    }
    let queries = Bifrost::query_only(client);
    let sql = format!("SELECT count(*) FROM {}", workload::INGEST_TABLE);
    let mut asked = 0;
    let mut answered = false;
    while !answered && asked < SMOKE_QUERIES {
        asked += 1;
        answered = run::ask(&queries, sql.clone(), &vec![vec![native_rows]])
            .await
            .is_ok();
        if !answered {
            tokio::time::sleep(SMOKE_POLL).await;
        }
    }
    if !answered {
        smoke.missing(format!(
            "none of {asked} `count(*)` queries answered the {native_rows} acknowledged native rows exactly"
        ));
    }
    expected.push(("bifrost.query", asked));
    let (usage, after) = window
        .close(server)
        .await
        .map_err(|error| error.to_string())?;
    smoke.usage(&usage);
    let delta = |family: &str| after.sum(family, &[]) - before.sum(family, &[]);
    smoke.positive(isolated::INSERTED, delta(isolated::INSERTED));
    let traces: Vec<&str> = traces.iter().map(String::as_str).collect();
    let population = collector.smoke("smoke", from, &expected, &traces).await;
    let boundaries: Vec<(&str, &str, bool)> = expected
        .iter()
        .map(|(boundary, _)| (*boundary, isolated::SUCCESS, false))
        .collect();
    smoke.timings(&population, &boundaries);
    // `count(*)` projects no column, so its scan reads no column bytes:
    // rows must be positive, bytes only reported.
    let [rows, bytes] = population.scanned_per_query(isolated::SUCCESS);
    for reading in [&rows, &bytes] {
        if reading.value.is_none() {
            smoke.missing(reading.reason.clone().unwrap_or_default());
        }
    }
    if let Some(rows) = rows.value {
        smoke.positive("scanned rows per query", rows);
    }
    smoke.finish()
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

/// Counts the Parquet objects under the `file://` store rooted at `root`.
fn parquet_files(root: &Path) -> u64 {
    parquet(root).len() as u64
}

/// Sums the sizes of `table`'s Parquet objects under the `file://` store
/// rooted at `root`, whose keys hold `/<segment>/<name>/` after the tenant.
pub fn parquet_bytes(root: &Path, table: &str) -> u64 {
    let segment = format!("/{}/", table.trim_start_matches("vala.").replace('.', "/"));
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
