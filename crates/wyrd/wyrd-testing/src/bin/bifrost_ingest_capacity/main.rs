//! Bifrost client ingest capacity benchmark (AC-041, Scenario A5).
//!
//! An operator starts one release `wyrd-server` the way the
//! local-development guide says (`migrate`, serve, `setup`) over the RustFS
//! emulator. The tenant administrator registers a Service whose one
//! component is a Model and issues its key; the application hydrates the
//! bundle and emits Drift observations of [`step::FEATURES`] numeric
//! features through one `WyrdState` at a fixed open-loop rate.
//!
//! Steps, each through a fresh `WyrdState` that drains before the next:
//! - `sustained`: 500 obs/s with the default queue; its AC-041 checks decide
//!   the run (zero `QUEUE_FULL`, client bytes not growing, drain within 1 s,
//!   exactly 100 durable rows per admitted observation);
//! - `headroom`: 1,000 obs/s, reported only;
//! - `ack-delay`: 500 obs/s with every server→client byte delayed 50 ms by a
//!   loopback proxy, reported only;
//! - `max-in-flight-N`: the sustained step per `--max-in-flight` value,
//!   reporting the smallest that passes.
//!
//! The report in `target/bifrost-ingest-capacity/` carries achieved rate,
//! refusals, client bytes, drain time, durable rows, batch count and mean
//! size, Gate write latency, and client and server CPU per row. Run through
//! `mise run bench:bifrost:ingest-capacity`. Exits nonzero when any gating
//! check fails.

mod fixture;
mod proxy;
mod step;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use wyrd_client::QueueConfig;
use wyrd_testing::capacity::{install_tracing, release_binary};
use wyrd_testing::release_server::{CPUS, Envelope, LocalServer, MEMORY_BYTES};

use fixture::Tenant;
use step::{Bench, Plan, Record};

/// Error type of every benchmark step: the binary only reports it.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// The AC-041 rate, observations per second.
const SUSTAINED_RATE: f64 = 500.0;

/// The reported headroom rate, observations per second.
const HEADROOM_RATE: f64 = 1_000.0;

/// The reported injected acknowledgement delay.
const ACK_DELAY: Duration = Duration::from_millis(50);

/// Command-line settings.
#[derive(Debug, Parser)]
#[command(about = "Measures sustained Bifrost client ingest through one WyrdState")]
struct Cli {
    /// Arrival window of every step, seconds.
    #[arg(long, default_value_t = 60.0)]
    seconds: f64,
    /// `max_in_flight` values to rerun the sustained step with.
    #[arg(long, value_delimiter = ',', default_value = "1,2,4,8")]
    max_in_flight: Vec<usize>,
    /// The `wyrd-server` binary; defaults to the one built beside this one.
    #[arg(long)]
    server_binary: Option<PathBuf>,
    /// Object store the server publishes into.
    #[arg(long, env = "WYRD_STORAGE_URL")]
    storage_url: String,
    /// S3-compatible endpoint of that store.
    #[arg(long, env = "WYRD_STORAGE_ENDPOINT_URL")]
    storage_endpoint_url: String,
}

/// The benchmark: start, provision, run every step, report.
///
/// Returns whether every gating check passed.
///
/// # Errors
///
/// Returns a server, provisioning, step, or report failure that stopped the
/// run.
async fn benchmark(cli: Cli) -> Result<bool> {
    let output = PathBuf::from("target/bifrost-ingest-capacity");
    std::fs::create_dir_all(&output)?;
    let work = tempfile::Builder::new()
        .prefix("wyrd-ingest-capacity-")
        .tempdir()?;
    let binary = match cli.server_binary {
        Some(binary) => std::fs::canonicalize(binary)?,
        None => release_binary()?,
    };
    let env = [
        ("WYRD_STORAGE_URL", cli.storage_url.as_str()),
        (
            "WYRD_STORAGE_ENDPOINT_URL",
            cli.storage_endpoint_url.as_str(),
        ),
    ];
    let server = LocalServer::start(&binary, &["m0"], &env, Envelope::POD).await?;
    let setup = server
        .tenants()
        .first()
        .ok_or("setup provisioned no tenant")?;
    let tenant = Tenant::provision(setup, &work.path().join("tenant")).await?;
    let bench = Bench {
        server: &server,
        tenant: &tenant,
    };

    let default_in_flight = QueueConfig::default().max_in_flight;
    let plan = |name: String, rate, max_in_flight, ack_delay, gating| Plan {
        name,
        rate,
        seconds: cli.seconds,
        max_in_flight,
        ack_delay,
        gating,
    };
    let mut plans = vec![
        plan(
            "sustained".to_owned(),
            SUSTAINED_RATE,
            default_in_flight,
            None,
            true,
        ),
        plan(
            "headroom".to_owned(),
            HEADROOM_RATE,
            default_in_flight,
            None,
            false,
        ),
        plan(
            "ack-delay".to_owned(),
            SUSTAINED_RATE,
            default_in_flight,
            Some(ACK_DELAY),
            false,
        ),
    ];
    for max_in_flight in cli.max_in_flight.iter().copied() {
        plans.push(plan(
            format!("max-in-flight-{max_in_flight}"),
            SUSTAINED_RATE,
            max_in_flight,
            None,
            false,
        ));
    }
    let mut records = Vec::new();
    for plan in &plans {
        let record = bench.run(plan).await?;
        eprintln!("{}", line(&record));
        records.push(record);
    }
    let shutdown = server
        .stop(&output.join("server.log"))
        .map_err(|error| error.to_string());

    let smallest_passing = records
        .iter()
        .filter(|record| record.name.starts_with("max-in-flight-") && record.failures.is_empty())
        .map(|record| record.max_in_flight)
        .min();
    let passed = records
        .iter()
        .filter(|record| record.gating)
        .all(|record| record.failures.is_empty())
        && shutdown.is_ok();
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "passed": passed,
            "envelope": { "cpus": CPUS, "memory_bytes": MEMORY_BYTES, "seconds": cli.seconds },
            "default_max_in_flight": default_in_flight,
            "smallest_passing_max_in_flight": smallest_passing,
            "shutdown_seconds": shutdown,
            "steps": records,
        }))?,
    )?;
    let rendered = render(&records, smallest_passing, passed);
    std::fs::write(output.join("report.txt"), &rendered)?;
    println!("{rendered}");
    Ok(passed)
}

/// One step as a report line.
fn line(record: &Record) -> String {
    let optional = |value: Option<f64>, scale: f64| {
        value.map_or_else(|| "-".to_owned(), |value| format!("{:.1}", value * scale))
    };
    format!(
        "{:<18} {:>6.0} {:>8.1} {:>4} {:>8} {:>10} {:>12.0} {:>7.3} {:>9}/{:<9} {:>7} {:>8} {:>8} {:>7} {:>7} {:>7} {:>7}  {}",
        record.name,
        record.rate,
        record.achieved_rate,
        record.max_in_flight,
        record.refusals,
        record.max_client_bytes,
        record.client_bytes_slope,
        record.drain_seconds,
        record.durable_rows,
        record.expected_rows,
        record.uneven_records,
        record.batches,
        optional(record.mean_batch_rows, 1.0),
        optional(record.send_p50_seconds, 1e3),
        optional(record.send_p99_seconds, 1e3),
        optional(record.client_cpu_us_per_row, 1.0),
        optional(record.server_cpu_us_per_row, 1.0),
        if record.failures.is_empty() {
            "PASS".to_owned()
        } else if record.gating {
            format!("FAIL: {}", record.failures.join("; "))
        } else {
            format!("report: {}", record.failures.join("; "))
        },
    )
}

/// The rendered text report.
fn render(records: &[Record], smallest_passing: Option<usize>, passed: bool) -> String {
    let mut rendered = format!(
        "{:<18} {:>6} {:>8} {:>4} {:>8} {:>10} {:>12} {:>7} {:>19} {:>7} {:>8} {:>8} {:>7} {:>7} {:>7} {:>7}  verdict\n",
        "step",
        "obs/s",
        "achieved",
        "mif",
        "refused",
        "max_bytes",
        "bytes_slope",
        "drain_s",
        "rows/expected",
        "uneven",
        "batches",
        "rows/b",
        "p50_ms",
        "p99_ms",
        "cli_us",
        "srv_us"
    );
    for record in records {
        rendered.push_str(&line(record));
        rendered.push('\n');
    }
    rendered.push_str(&format!(
        "smallest passing max_in_flight: {}\noverall: {}\n",
        smallest_passing.map_or_else(|| "none".to_owned(), |value| value.to_string()),
        if passed { "PASS" } else { "FAIL" }
    ));
    rendered
}

/// Runs the benchmark and exits nonzero on any failed gating check or error.
#[tokio::main]
async fn main() -> ExitCode {
    install_tracing();
    match benchmark(Cli::parse()).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("bifrost ingest capacity benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}
