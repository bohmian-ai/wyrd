//! Opt-in Forge capacity benchmark. Run only through
//! `mise run bench:bifrost:forge-capacity`.
//!
//! Every Wyrd process together stays within 8 CPU / 16 GiB on a 16-CPU host:
//! the leader runs in a 1 CPU / 2 GiB systemd scope and each dedicated
//! compactor in a 1.5 CPU / 3 GiB scope, so the 4-worker step uses
//! 7 CPU / 14 GiB. The host's load is sampled first; a run that starts with
//! fewer than 8 free CPUs is discarded, not measured.
//!
//! 1. Leader decision latency, in-process: a 10,000 tracked / 1,000 due
//!    schedule driven open-loop by 32 simulated compactors at the production
//!    pull rate, 10x, then increasing multiples until the p99 bends.
//! 2. Leader decision latency, live: one release `wyrd-server` leader
//!    (`WYRD_TARGET=server`) in peer mode over the storage emulators' RustFS
//!    bucket and the wrapper's Postgres, started through the local-development
//!    journey. While at least 128 tables receive continuous public writes and
//!    no compactor runs, the benchmark plays the 32-compactor fleet against
//!    the leader's peer route at the production rate and 10x, reading the
//!    leader's `bifrost_forge_leader_decision_seconds`.
//! 3. Fleet throughput: 1, 2, then 4 dedicated `WYRD_TARGET=forge-worker`
//!    replicas compact those tables at the default compaction type and file
//!    target with a realistic snapshot-count trigger. Each worker count is
//!    measured over repeated windows after a warmup.
//!
//! Writes `report.json`, `report.txt`, and every server log to
//! `target/bifrost-forge-capacity/`; exits 1 on any failed check and 2 when
//! the host was too loaded for the run to count.

mod dependencies;
mod deployment;
mod fleet;
mod host;
mod probe;
mod report;
mod schedule;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use wyrd_testing::bifrost::peer_ca::BifrostPeerCa;
use wyrd_testing::capacity::{binary_identity, install_tracing, release_binary};
use wyrd_testing::release_server::{Envelope, LocalServer};

use dependencies::Dependencies;
use deployment::{Deployment, LogCursor};
use fleet::Fleet;
use host::{Host, HostLoad};
use probe::LeaderProbe;
use report::{Evidence, Report, ResourcePlans, Settings};
use schedule::RateSweep;

/// Error type of the benchmark.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Peer certificate name every process presents and dials.
const PEER_NAME: &str = "wyrd-peer";

/// Report directory, relative to the workspace root mise runs from.
const OUTPUT: &str = "target/bifrost-forge-capacity";

/// The leader's CPU quota, percent of one CPU.
const LEADER_CPU_PERCENT: u64 = 100;

/// Each dedicated compactor's CPU quota, percent of one CPU.
const WORKER_CPU_PERCENT: u64 = 150;

/// Server log filter: info, plus the Forge leader's per-pull and the
/// worker's per-pull debug lines the full-answer gate reads.
const SERVER_LOG: &str =
    "info,vala_bifrost_redux::forge::leader=debug,vala_bifrost_redux::forge::worker=debug";

/// How long the host's load is sampled before and after a run.
const LOAD_SAMPLE: Duration = Duration::from_secs(5);

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
    /// Seconds each worker count runs before its first window, and the
    /// writes run before the live probe.
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
    /// Promotion commits that make a table due. Ten seals per compaction
    /// keeps production's ratio: at the default ten-minute seal a table
    /// compacts about every hundred minutes, merging about ten staged files.
    #[arg(long, default_value_t = 10)]
    trigger_snapshot_count: usize,
    /// Interval between two writes to one table, milliseconds.
    #[arg(long, default_value_t = 1_000)]
    write_interval_ms: u64,
    /// Rows per write.
    #[arg(long, default_value_t = 64)]
    rows_per_write: usize,
    /// The leader scope's memory, GiB.
    #[arg(long, default_value_t = 2)]
    leader_memory_gib: u64,
    /// Each compactor scope's memory, GiB.
    #[arg(long, default_value_t = 3)]
    worker_memory_gib: u64,
    /// Seconds each live probe step runs; at least the leader summary's
    /// 60 s rolling window, so its quantiles describe only that step.
    #[arg(long, default_value_t = 60.0)]
    probe_seconds: f64,
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

/// How a run ended.
enum Outcome {
    /// Measured; whether every check passed.
    Measured(bool),
    /// The host was too loaded to measure.
    Discarded,
}

/// The benchmark: the host-load preflight, the in-process sweep, the live
/// probe, the worker ladder, then the report.
///
/// # Errors
///
/// Returns a scheduling, server, client, or evidence failure that stopped
/// the run.
async fn benchmark(cli: Cli) -> Result<Outcome> {
    if cli.tables < 128 {
        return Err("the Forge capacity workload needs at least 128 tables".into());
    }
    let host = Host::read();
    let output = PathBuf::from(OUTPUT);
    std::fs::create_dir_all(&output)?;
    let load_before = HostLoad::sample(host.cpus, LOAD_SAMPLE).await;
    eprintln!("host load before: {}", load_before.describe());
    if !load_before.qualifies() {
        let note = format!("DISCARDED: host load {}\n", load_before.describe());
        std::fs::write(output.join("discarded.txt"), &note)?;
        eprint!("{note}");
        return Ok(Outcome::Discarded);
    }
    let leader_envelope = Envelope {
        cpu_percent: LEADER_CPU_PERCENT,
        memory_bytes: cli.leader_memory_gib << 30,
    };
    let worker_envelope = Envelope {
        cpu_percent: WORKER_CPU_PERCENT,
        memory_bytes: cli.worker_memory_gib << 30,
    };
    let binary = match &cli.server_binary {
        Some(binary) => std::fs::canonicalize(binary)?,
        None => release_binary()?,
    };

    let sweep = RateSweep {
        tables: 10_000,
        due: 1_000,
        production_seconds: 60.0,
        tenfold_seconds: 30.0,
        step_seconds: 10.0,
    }
    .run()?;

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
    let probe_identity = peer.materialize(&work.path().join("peer"), "probe")?;
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
            ("WYRD_LOG", SERVER_LOG),
        ]
    };
    let mut leader_env = shared(0);
    leader_env.extend([
        ("WYRD_TARGET", "server"),
        ("WYRD_MAX_FILE_RETENTION_TIME", seal.as_str()),
    ]);
    let leader = LocalServer::start(&binary, &["forge"], &leader_env, leader_envelope).await?;
    let fleet = Fleet::provision(&leader, cli.tables, cli.trigger_snapshot_count).await?;
    let dependencies = Dependencies::connect().await?;
    let probe = LeaderProbe::new(dependencies.owner(), &probe_identity)?;
    let mut deployment = Deployment {
        leader_log: LogCursor::at_end(&leader.log_path()),
        leader,
        workers: Vec::new(),
        dependencies,
    };
    let writers = fleet.write(
        Duration::from_millis(cli.write_interval_ms),
        cli.rows_per_write,
    );

    let mut probes = Vec::new();
    let mut windows = Vec::new();
    let mut failure = None;
    tokio::time::sleep(Duration::from_secs_f64(cli.warmup_seconds)).await;
    for multiplier in [1.0, 10.0] {
        match probe
            .step(&deployment.leader, multiplier, cli.probe_seconds)
            .await
        {
            Ok(step) => {
                eprintln!(
                    "live probe {multiplier}x: {:.2} pulls/s, leader p99 commit/pull/report {:?}/{:?}/{:?} µs",
                    step.achieved_pulls_per_second,
                    step.leader.commit.us.p99,
                    step.leader.pull.us.p99,
                    step.leader.report.us.p99
                );
                probes.push(step);
            }
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }
    for count in cli.workers.iter().copied() {
        if failure.is_some() {
            break;
        }
        if let Err(error) = deployment
            .grow_to(&binary, count, worker_envelope, shared)
            .await
        {
            failure = Some(error);
            break;
        }
        tokio::time::sleep(Duration::from_secs_f64(cli.warmup_seconds)).await;
        for _ in 0..cli.windows {
            match deployment.measure(&writers, cli.window_seconds).await {
                Ok(window) => {
                    eprintln!(
                        "{count} worker(s): {:.2} rewrites/s, leader CPU {:.1}%, pulls {} ({} short, {} short while due)",
                        window.rewrites_per_second,
                        window.leader_cpu_share * 100.0,
                        window.pulls.pulls,
                        window.pulls.short,
                        window.pulls.short_while_due
                    );
                    windows.push(window);
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
    }
    writers.stop().await;
    let worker_count = deployment.workers.len();
    stop(deployment, &output);
    if let Some(error) = failure {
        return Err(error);
    }
    let load_after = HostLoad::sample(host.cpus, LOAD_SAMPLE).await;

    let report = Report::new(
        Evidence {
            host,
            load_before,
            load_after,
            binary: binary_identity(&binary)?,
            settings: Settings {
                tables: fleet.table_count(),
                tenants: 1,
                seal_seconds: cli.seal_seconds,
                write_interval_ms: cli.write_interval_ms,
                rows_per_write: cli.rows_per_write,
                trigger_snapshot_count: cli.trigger_snapshot_count,
                leader_envelope: (leader_envelope.cpu_percent, leader_envelope.memory_bytes),
                worker_envelope: (worker_envelope.cpu_percent, worker_envelope.memory_bytes),
                warmup_seconds: cli.warmup_seconds,
                window_seconds: cli.window_seconds,
                windows: cli.windows,
            },
            resource_plans: resource_plans(&output, worker_count),
            sweep,
            probe: probes,
            windows,
        },
        &cli.workers,
    );
    report.write(&output)?;
    print!("{}", report.text());
    Ok(Outcome::Measured(report.passed()))
}

/// Reads what each process resolved from its scope out of the logs
/// [`stop`] copied into `output`.
fn resource_plans(output: &Path, workers: usize) -> ResourcePlans {
    let leader = std::fs::read_to_string(output.join("leader.log")).unwrap_or_default();
    let oracle = "Oracle admission capacity resolved";
    ResourcePlans {
        leader_effective_cpu: ResourcePlans::field(&leader, oracle, "effective_cpu"),
        leader_managed_memory_bytes: ResourcePlans::field(&leader, oracle, "managed_memory_bytes"),
        worker_max_task_parallelism: (1..=workers)
            .map(|ordinal| {
                let log = std::fs::read_to_string(output.join(format!("worker-{ordinal}.log")))
                    .unwrap_or_default();
                ResourcePlans::field(&log, "Forge worker started", "max_task_parallelism")
            })
            .collect(),
    }
}

/// Stops every process cleanly and keeps its log, ANSI styling removed,
/// beside the report; a stop failure is printed, not fatal, so the evidence
/// still lands.
fn stop(deployment: Deployment, output: &Path) {
    let keep = |server: LocalServer, name: String| {
        let log = output.join(name);
        if let Err(error) = server.stop(&log) {
            eprintln!("{} did not stop cleanly: {error}", log.display());
        }
        let mut cursor = LogCursor::at_start(&log);
        if let Ok(plain) = cursor.read_new() {
            let _ = std::fs::write(&log, plain);
        }
    };
    for worker in deployment.workers {
        let name = format!("worker-{}.log", worker.ordinal());
        keep(worker, name);
    }
    keep(deployment.leader, "leader.log".to_owned());
}

/// Runs the benchmark; exits 1 on any failed check or error and 2 when the
/// host was too loaded to measure.
#[tokio::main]
async fn main() -> ExitCode {
    install_tracing();
    match benchmark(Cli::parse()).await {
        Ok(Outcome::Measured(true)) => ExitCode::SUCCESS,
        Ok(Outcome::Measured(false)) => ExitCode::FAILURE,
        Ok(Outcome::Discarded) => ExitCode::from(2),
        Err(error) => {
            eprintln!("Forge capacity benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}
