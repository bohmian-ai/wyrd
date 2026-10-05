//! Opt-in Forge capacity benchmark. Run only through
//! `mise run bench:bifrost:forge-capacity`.
//!
//! Every Wyrd process together stays within 8 CPU / 16 GiB: the leader runs
//! in a 1 CPU / 4 GiB systemd scope and each of at most three dedicated
//! compactors in a 7/3 CPU / 4 GiB scope, so the 3-worker step uses
//! 8 CPU / 16 GiB. 4 GiB is the Wyrd process floor. The host's load is
//! sampled first; a run that starts with fewer than 8 free CPUs is
//! discarded, not measured.
//!
//! 1. Leader decision latency, in-process: a 10,000 tracked / 1,000 due
//!    schedule driven open-loop by 32 simulated compactors at the production
//!    pull rate, 10x, then increasing multiples until the p99 bends.
//! 2. Backlog fill: one release `wyrd-server` leader (`WYRD_TARGET=server`)
//!    in peer mode over the storage emulators' RustFS bucket and the
//!    wrapper's Postgres, started through the local-development journey.
//!    For each fleet size the benchmark registers its own fresh set of at
//!    least 128 tables and writes them through the public client and Scribe,
//!    with no compactor running, until every table holds the backlog's
//!    snapshots: at least two promotions, so at least two staged files per
//!    table and a two-file filter passes. Each table opts into compaction at
//!    the default type and file target with a snapshot-count trigger equal to
//!    the backlog's snapshots, so the whole set is due when the fill ends.
//! 3. Leader decision latency, live: while the first fill's writers still
//!    run and no compactor runs, the benchmark plays the 32-compactor fleet
//!    against the leader's peer route at the production rate and 10x,
//!    reading the leader's `bifrost_forge_leader_decision_seconds`.
//! 4. Backlog drain: the writers stop and 1, 2, then 3 fresh dedicated
//!    `WYRD_TARGET=forge-worker` replicas drain their own equal backlog until
//!    every table was rewritten or the drain limit passed, measuring rewrites
//!    and bytes per second and every pull the leader answered short while a
//!    table stayed due.
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
use deployment::{Deployment, LogCursor, keep_log};
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

/// Each dedicated compactor's CPU quota, percent of one CPU: 7/3 CPU, so
/// three compactors and the leader fill 8 CPUs.
const WORKER_CPU_PERCENT: u64 = 233;

/// Server log filter: info, plus the Forge leader's per-pull and the
/// worker's per-pull debug lines the full-answer gate reads.
const SERVER_LOG: &str =
    "info,vala_bifrost_redux::forge::leader=debug,vala_bifrost_redux::forge::worker=debug";

/// How long the host's load is sampled before and after a run.
const LOAD_SAMPLE: Duration = Duration::from_secs(5);

/// Command-line settings.
#[derive(Debug, Parser)]
#[command(about = "Measures Forge leader decisions and compaction backlog drain")]
struct Cli {
    /// Independent tables in each fleet size's backlog; at least 128.
    #[arg(long, default_value_t = 128)]
    tables: usize,
    /// Dedicated worker counts to measure, in order; at most 3 keeps the
    /// 8 CPU / 16 GiB envelope.
    #[arg(long, value_delimiter = ',', default_value = "1,2,3")]
    workers: Vec<usize>,
    /// Snapshots — promotions, each appending staged files — every table
    /// holds before its fleet drains; also each table's snapshot-count
    /// trigger, so the filled backlog is due. At least 2, so a table holds
    /// at least two staged files to merge.
    #[arg(long, default_value_t = 2)]
    backlog_snapshots: usize,
    /// Longest one backlog fill may take, seconds.
    #[arg(long, default_value_t = 600)]
    fill_timeout_seconds: u64,
    /// Longest one drain may run, seconds; a drain still short of its
    /// backlog then reports what it rewrote.
    #[arg(long, default_value_t = 300.0)]
    drain_limit_seconds: f64,
    /// Scribe seal age, seconds (`WYRD_MAX_FILE_RETENTION_TIME`).
    #[arg(long, default_value_t = 10)]
    seal_seconds: u64,
    /// Interval between two writes to one table, milliseconds.
    #[arg(long, default_value_t = 1_000)]
    write_interval_ms: u64,
    /// Rows per write.
    #[arg(long, default_value_t = 64)]
    rows_per_write: usize,
    /// The leader scope's memory, GiB; the Wyrd process floor is 4.
    #[arg(long, default_value_t = 4)]
    leader_memory_gib: u64,
    /// Each compactor scope's memory, GiB; the Wyrd process floor is 4.
    #[arg(long, default_value_t = 4)]
    worker_memory_gib: u64,
    /// Seconds each live probe step runs; at least the leader summary's
    /// 60 s rolling window, so its quantiles describe only that step.
    #[arg(long, default_value_t = 60.0)]
    probe_seconds: f64,
    /// `WYRD_LOG` every server process runs with; it must keep the Forge
    /// leader's and worker's debug pull lines.
    #[arg(long, default_value = SERVER_LOG)]
    server_log: String,
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

/// The benchmark: the host-load preflight, the in-process sweep, then per
/// fleet size a backlog fill (the first carrying the live probe) and its
/// drain, then the report.
///
/// # Errors
///
/// Returns invalid settings, or a scheduling, server, client, or evidence
/// failure that stopped the run.
async fn benchmark(cli: Cli) -> Result<Outcome> {
    if cli.tables < 128 {
        return Err("the Forge capacity workload needs at least 128 tables".into());
    }
    if cli.backlog_snapshots < 2 {
        return Err("a backlog needs at least two snapshots per table".into());
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
            ("WYRD_LOG", cli.server_log.as_str()),
        ]
    };
    let mut leader_env = shared(0);
    leader_env.extend([
        ("WYRD_TARGET", "server"),
        ("WYRD_MAX_FILE_RETENTION_TIME", seal.as_str()),
    ]);
    let leader = LocalServer::start(&binary, &["forge"], &leader_env, leader_envelope).await?;
    let dependencies = Dependencies::connect().await?;
    let probe = LeaderProbe::new(dependencies.owner(), &probe_identity)?;
    let mut deployment = Deployment {
        leader_log: LogCursor::at_end(&leader.log_path()),
        leader,
        workers: Vec::new(),
        dependencies,
    };

    let mut probes = Vec::new();
    let mut fills = Vec::new();
    let mut drains = Vec::new();
    let mut failure = None;
    for (index, count) in cli.workers.iter().copied().enumerate() {
        let step = index + 1;
        let result: Result<()> = async {
            let fleet =
                Fleet::provision(&deployment.leader, step, cli.tables, cli.backlog_snapshots)
                    .await?;
            let (fill, writers) = fleet
                .fill(
                    &deployment.leader,
                    count,
                    (
                        Duration::from_millis(cli.write_interval_ms),
                        cli.rows_per_write,
                    ),
                    cli.backlog_snapshots,
                    Duration::from_secs(cli.fill_timeout_seconds),
                )
                .await?;
            eprintln!(
                "backlog for {count} worker(s): {:.0} s, promotions {:.2}/s",
                fill.seconds, fill.promotion_commits_per_second
            );
            fills.push(fill);
            if probes.is_empty() {
                for multiplier in [1.0, 10.0] {
                    let probed = match probe
                        .step(&deployment.leader, multiplier, cli.probe_seconds)
                        .await
                    {
                        Ok(probed) => probed,
                        Err(error) => {
                            writers.stop().await;
                            return Err(error);
                        }
                    };
                    eprintln!(
                        "live probe {multiplier}x: {:.2} pulls/s, leader p99 commit/pull/report {:?}/{:?}/{:?} µs",
                        probed.achieved_pulls_per_second,
                        probed.leader.commit.us.p99,
                        probed.leader.pull.us.p99,
                        probed.leader.report.us.p99
                    );
                    probes.push(probed);
                }
            }
            writers.stop().await;
            let drain = deployment
                .drain(
                    &binary,
                    count,
                    worker_envelope,
                    shared,
                    u64::try_from(fleet.table_count())?,
                    cli.drain_limit_seconds,
                )
                .await;
            deployment.stop_workers(&output, step);
            let drain = drain?;
            eprintln!(
                "{count} worker(s): {} of {} tables in {:.0} s, {:.2} rewrites/s, {:.0} KiB/s in, \
                 leader CPU {:.1}%, pulls {} ({} short, {} short while due)",
                drain.rewrites,
                drain.backlog_tables,
                drain.seconds,
                drain.rewrites_per_second,
                drain.input_bytes_per_second / 1024.0,
                drain.leader_cpu_share * 100.0,
                drain.pulls.pulls,
                drain.pulls.short,
                drain.pulls.short_while_due
            );
            drains.push(drain);
            Ok(())
        }
        .await;
        if let Err(error) = result {
            failure = Some(error);
            break;
        }
    }
    keep_log(deployment.leader, &output.join("leader.log"));
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
                tables: cli.tables,
                tenants: 1,
                seal_seconds: cli.seal_seconds,
                write_interval_ms: cli.write_interval_ms,
                rows_per_write: cli.rows_per_write,
                trigger_snapshot_count: cli.backlog_snapshots,
                leader_envelope: (leader_envelope.cpu_percent, leader_envelope.memory_bytes),
                worker_envelope: (worker_envelope.cpu_percent, worker_envelope.memory_bytes),
                backlog_snapshots: cli.backlog_snapshots,
                drain_limit_seconds: cli.drain_limit_seconds,
            },
            resource_plans: resource_plans(&output, &cli.workers),
            sweep,
            probe: probes,
            fills,
            drains,
        },
        &cli.workers,
    );
    report.write(&output)?;
    print!("{}", report.text());
    Ok(Outcome::Measured(report.passed()))
}

/// Reads what each process resolved from its scope out of the logs
/// [`keep_log`] copied into `output`: the leader's, then each step's workers
/// in ladder order.
fn resource_plans(output: &Path, ladder: &[usize]) -> ResourcePlans {
    let leader = std::fs::read_to_string(output.join("leader.log")).unwrap_or_default();
    let oracle = "Oracle admission capacity resolved";
    ResourcePlans {
        leader_effective_cpu: ResourcePlans::field(&leader, oracle, "effective_cpu"),
        leader_managed_memory_bytes: ResourcePlans::field(&leader, oracle, "managed_memory_bytes"),
        worker_max_task_parallelism: ladder
            .iter()
            .enumerate()
            .flat_map(|(index, count)| (1..=*count).map(move |ordinal| (index + 1, ordinal)))
            .map(|(step, ordinal)| {
                let log =
                    std::fs::read_to_string(output.join(format!("worker-{step}-{ordinal}.log")))
                        .unwrap_or_default();
                ResourcePlans::field(&log, "Forge worker started", "max_task_parallelism")
            })
            .collect(),
    }
}

/// Runs the benchmark; exits 1 on any failed check or error and 2 when the
/// host was too loaded to measure.
///
/// # Panics
///
/// Panics if `#[tokio::main]` cannot build the Tokio runtime.
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
