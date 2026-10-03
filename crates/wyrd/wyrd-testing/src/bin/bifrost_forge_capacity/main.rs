//! Opt-in Forge capacity benchmark. Run only through
//! `mise run bench:bifrost:forge-capacity`.
//!
//! Two measurements, reported together and gated separately:
//!
//! 1. The leader decision, in-process: a fixed 10,000 tracked-table / 1,000
//!    due-table schedule pulled by 32 concurrent threads, repeated, for the
//!    p99 warm commit-state update and p99 pull selection.
//! 2. Physical compaction throughput, live: one release `wyrd-server` leader
//!    (`WYRD_TARGET=server`) in peer mode over the storage emulators' RustFS
//!    bucket and the wrapper's Postgres, started through the local-development
//!    journey, plus 1, 2, then 4 dedicated `WYRD_TARGET=forge-worker`
//!    replicas, each in its own 8-CPU/16-GiB systemd scope. At least 128
//!    tables, opted into compaction on every commit, receive continuous public
//!    writes; Scribe seals each at the configured age, the leader promotes
//!    and notifies, and the workers pull and rewrite. After a warmup, each
//!    worker count is measured over repeated windows.
//!
//! Writes `report.json`, `report.txt`, and every server log to
//! `target/bifrost-forge-capacity/`, and exits nonzero on any failed check.

mod dependencies;
mod deployment;
mod fleet;
mod report;
mod schedule;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use wyrd_testing::bifrost::peer_ca::BifrostPeerCa;
use wyrd_testing::capacity::{binary_identity, install_tracing, release_binary};
use wyrd_testing::release_server::LocalServer;

use dependencies::Dependencies;
use deployment::Deployment;
use fleet::Fleet;
use report::{Host, Report, Scheduling, Settings};
use schedule::SchedulingCase;

/// Error type of the benchmark.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Peer certificate name every process presents and dials.
const PEER_NAME: &str = "wyrd-peer";

/// Report directory, relative to the workspace root mise runs from.
const OUTPUT: &str = "target/bifrost-forge-capacity";

/// Command-line settings.
#[derive(Debug, Parser)]
#[command(about = "Measures Forge leader decisions and compaction throughput")]
struct Cli {
    /// Independent tables the live workload compacts; at least 128.
    #[arg(long, default_value_t = 128)]
    tables: usize,
    /// Dedicated worker counts to measure, in order.
    #[arg(long, value_delimiter = ',', default_value = "1,2,4")]
    workers: Vec<usize>,
    /// Seconds each worker count runs before its first window.
    #[arg(long, default_value_t = 60.0)]
    warmup_seconds: f64,
    /// Length of one measured window, seconds.
    #[arg(long, default_value_t = 60.0)]
    window_seconds: f64,
    /// Windows measured per worker count.
    #[arg(long, default_value_t = 3)]
    windows: usize,
    /// Scribe seal age, seconds (`WYRD_MAX_FILE_RETENTION_TIME`).
    #[arg(long, default_value_t = 10)]
    seal_seconds: u64,
    /// Interval between two writes to one table, milliseconds.
    #[arg(long, default_value_t = 1_000)]
    write_interval_ms: u64,
    /// Rows per write.
    #[arg(long, default_value_t = 64)]
    rows_per_write: usize,
    /// Repetitions of the in-process scheduling case.
    #[arg(long, default_value_t = 5)]
    schedule_repeats: usize,
    /// The `wyrd-server` binary; defaults to the one built beside this one.
    #[arg(long)]
    server_binary: Option<PathBuf>,
    /// Shared object store every process serves, as peer mode requires.
    #[arg(long, env = "WYRD_STORAGE_URL")]
    storage_url: String,
    /// S3-compatible endpoint of that store.
    #[arg(long, env = "WYRD_STORAGE_ENDPOINT_URL")]
    storage_endpoint_url: String,
}

/// The benchmark: the scheduling case, then the live worker ladder, then the
/// report.
///
/// Returns whether every check passed.
///
/// # Errors
///
/// Returns a scheduling, server, client, or evidence failure that stopped
/// the run.
async fn benchmark(cli: Cli) -> Result<bool> {
    if cli.tables < 128 {
        return Err("the Forge capacity workload needs at least 128 tables".into());
    }
    let host = Host::read();
    let output = PathBuf::from(OUTPUT);
    std::fs::create_dir_all(&output)?;
    let binary = match &cli.server_binary {
        Some(binary) => std::fs::canonicalize(binary)?,
        None => release_binary()?,
    };

    let case = SchedulingCase {
        tables: 10_000,
        due: 1_000,
        pullers: 32,
    };
    let runs = (0..cli.schedule_repeats)
        .map(|_| case.run())
        .collect::<Result<Vec<_>>>()?;
    let scheduling = Scheduling::new(case, runs);

    let work = tempfile::Builder::new()
        .prefix("wyrd-forge-capacity-")
        .tempdir()?;
    let peer = BifrostPeerCa::generate(PEER_NAME)?;
    let max_workers = cli.workers.iter().copied().max().unwrap_or(1);
    let peer_dirs = (0..=max_workers)
        .map(|ordinal| {
            Ok(peer
                .materialize(&work.path().join("peer"), &format!("replica-{ordinal}"))?
                .dir
                .display()
                .to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    let seal = cli.seal_seconds.to_string();
    let shared = |ordinal: u16| -> Vec<(&str, &str)> {
        vec![
            ("WYRD_STORAGE_URL", cli.storage_url.as_str()),
            (
                "WYRD_STORAGE_ENDPOINT_URL",
                cli.storage_endpoint_url.as_str(),
            ),
            (
                "WYRD_PEER_TLS_DIR",
                peer_dirs[usize::from(ordinal)].as_str(),
            ),
        ]
    };
    let mut leader_env = shared(0);
    leader_env.extend([
        ("WYRD_TARGET", "server"),
        ("WYRD_MAX_FILE_RETENTION_TIME", seal.as_str()),
    ]);
    let leader = LocalServer::start(&binary, &["forge"], &leader_env).await?;
    let fleet = Fleet::provision(&leader, cli.tables).await?;
    let mut deployment = Deployment {
        leader,
        workers: Vec::new(),
        dependencies: Dependencies::connect().await?,
    };
    let writers = fleet.write(
        Duration::from_millis(cli.write_interval_ms),
        cli.rows_per_write,
    );

    let mut windows = Vec::new();
    let mut failure = None;
    for count in cli.workers.iter().copied() {
        if let Err(error) = deployment.grow_to(&binary, count, shared).await {
            failure = Some(error);
            break;
        }
        tokio::time::sleep(Duration::from_secs_f64(cli.warmup_seconds)).await;
        for _ in 0..cli.windows {
            match deployment.measure(&writers, cli.window_seconds).await {
                Ok(window) => {
                    eprintln!(
                        "{count} worker(s): {:.2} rewrites/s, leader CPU {:.1}%",
                        window.rewrites_per_second,
                        window.leader_cpu_share * 100.0
                    );
                    windows.push(window);
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        if failure.is_some() {
            break;
        }
    }
    writers.stop().await;
    stop(deployment, &output);
    if let Some(error) = failure {
        return Err(error);
    }

    let report = Report::new(
        host,
        binary_identity(&binary)?,
        Settings {
            tables: fleet.table_count(),
            tenants: 1,
            seal_seconds: cli.seal_seconds,
            write_interval_ms: cli.write_interval_ms,
            rows_per_write: cli.rows_per_write,
            warmup_seconds: cli.warmup_seconds,
            window_seconds: cli.window_seconds,
            windows: cli.windows,
        },
        scheduling,
        windows,
        &cli.workers,
    );
    report.write(&output)?;
    print!("{}", report.text());
    Ok(report.passed())
}

/// Stops every process cleanly and keeps its log beside the report; a stop
/// failure is printed, not fatal, so the evidence still lands.
fn stop(deployment: Deployment, output: &Path) {
    for worker in deployment.workers {
        let log = output.join(format!("worker-{}.log", worker.ordinal()));
        if let Err(error) = worker.stop(&log) {
            eprintln!("worker {} did not stop cleanly: {error}", log.display());
        }
    }
    if let Err(error) = deployment.leader.stop(&output.join("leader.log")) {
        eprintln!("the leader did not stop cleanly: {error}");
    }
}

/// Runs the benchmark and exits nonzero on any failed check or error.
#[tokio::main]
async fn main() -> ExitCode {
    install_tracing();
    match benchmark(Cli::parse()).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("Forge capacity benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}
