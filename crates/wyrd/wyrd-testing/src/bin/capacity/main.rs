//! Wyrd's one server capacity benchmark (REQ-171, AC-040, AC-041).
//!
//! An operator starts release `wyrd-server` replicas the way the
//! local-development guide says (`migrate`, serve, `setup`), in peer mode
//! over Postgres and one shared S3-compatible store, as the kind deployment
//! runs them, with the LLM provider pointed at a local TLS judge that
//! answers after a fixed delay. Four identical tenants register the AC-040
//! reference workloads; the driver then offers one production mix open-loop
//! through the public Rust client: direct and queued verification, Scribe
//! ingest through `WyrdState`, and Oracle queries (see [`load::mix`]).
//!
//! The sequence follows the Google SRE load-test shape:
//! 1. warmup at the first level on one replica, not judged;
//! 2. ramp through every level on one replica, stopping at the first step
//!    that misses an SLO; the highest passing level is the knee `K`;
//! 3. sustained at `K` on one replica;
//! 4. sustained at `K` on two replicas;
//! 5. scale-out at `2K` on two replicas.
//!
//! Every step is judged against the golden-signal SLOs in [`report`], and
//! the run passes when the one-replica sustained step and both two-replica
//! steps pass. The report in `target/capacity/` has one row per step with
//! PASS/FAIL cells, then one row per operation per step.
//!
//! Run through `mise run bench:capacity`; `-- --profile` adds per-step,
//! per-replica `perf` captures of a frame-pointer build, as diagnostic
//! evidence. Exits nonzero when the verdict fails.

mod evidence;
mod fixture;
mod judge;
mod load;
mod profile;
mod report;
mod step;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use secrecy::ExposeSecret as _;
use wyrd_testing::bifrost::peer_ca::BifrostPeerCa;
use wyrd_testing::release_server::{CPUS, LocalServer, MEMORY_BYTES};

use evidence::Queue;
use fixture::Tenant;
use judge::Judge;
use load::{MAX_IN_FLIGHT, TenantClients};
use report::{Report, step_row};
use step::{Deployment, Plan, permits};

/// Error type of every benchmark step: the binary only reports it.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// The four identical tenants `setup` provisions (REQ-171).
const TENANTS: [&str; 4] = ["t0", "t1", "t2", "t3"];

/// Replicas of the two-replica steps.
const REPLICAS: u16 = 2;

/// Peer certificate name every replica presents and dials.
const PEER_NAME: &str = "wyrd-peer";

/// Spacing between tenants whose clients authenticate. Each replica limits
/// `/auth/*` to a burst of 20 then 10 requests per second per peer IP, and
/// every tenant here comes from one loopback address; one tenant makes up to
/// four such requests per replica, so this spacing stays under the refill
/// rate instead of tripping the limiter.
const AUTH_SPACING: std::time::Duration = std::time::Duration::from_millis(400);

/// Command-line settings. The defaults are the REQ-171 run; shorter windows
/// are for smoke runs only.
#[derive(Debug, Parser)]
#[command(about = "Measures the capacity of release wyrd-server replicas")]
struct Cli {
    /// Ramp levels `L`, verification executions per second, in order; the
    /// warmup runs at the first.
    #[arg(long, value_delimiter = ',', default_value = "50,100,200,400")]
    levels: Vec<f64>,
    /// Arrival window of the warmup, seconds.
    #[arg(long, default_value_t = 30.0)]
    warmup_seconds: f64,
    /// Arrival window of each ramp step, seconds.
    #[arg(long, default_value_t = 60.0)]
    ramp_seconds: f64,
    /// Arrival window of each sustained and scale-out step, seconds.
    #[arg(long, default_value_t = 180.0)]
    sustained_seconds: f64,
    /// Capture every step with `perf`; the report is then diagnostic.
    #[arg(long)]
    profile: bool,
    /// The `wyrd-server` binary, resolved to an absolute path because each
    /// replica runs from its own working directory; defaults to the one built
    /// beside this one.
    #[arg(long)]
    server_binary: Option<PathBuf>,
    /// Shared object store every replica serves, as peer mode requires.
    #[arg(long, env = "WYRD_STORAGE_URL")]
    storage_url: String,
    /// S3-compatible endpoint of that store.
    #[arg(long, env = "WYRD_STORAGE_ENDPOINT_URL")]
    storage_endpoint_url: String,
}

/// The benchmark: start, provision, run the REQ-171 sequence, report.
///
/// Returns whether the verdict passed.
///
/// # Errors
///
/// Returns a server, client, or evidence failure that stopped the run.
async fn benchmark(cli: Cli) -> Result<bool> {
    let started = std::time::Instant::now();
    let output = PathBuf::from("target/capacity");
    std::fs::create_dir_all(&output)?;
    let profiles = output.join("profiles");
    if cli.profile && profiles.exists() {
        std::fs::remove_dir_all(&profiles)?;
    }
    let work = tempfile::Builder::new()
        .prefix("wyrd-capacity-")
        .tempdir()?;
    let binary = match cli.server_binary {
        Some(binary) => std::fs::canonicalize(binary)?,
        None => release_binary()?,
    };
    let judge = Arc::new(Judge::start(&work.path().join("judge")).await?);
    let peer = BifrostPeerCa::generate(PEER_NAME)?;
    let peer_dirs = (0..REPLICAS)
        .map(|ordinal| {
            Ok(peer
                .materialize(&work.path().join("peer"), &format!("replica-{ordinal}"))?
                .dir
                .display()
                .to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    let ca_file = judge.ca_file().display().to_string();
    // One workload signing key for every replica, as the kind deployment
    // mounts one secret into every pod: a token one replica mints must verify
    // on every other replica.
    let signing_key = work.path().join("signing.pem");
    std::fs::write(
        &signing_key,
        wyrd_auth_issue::IssuingKey::generate_ephemeral_pem()?.expose_secret(),
    )?;
    let signing_key = signing_key.display().to_string();
    let env = |ordinal: u16| -> Vec<(&str, &str)> {
        vec![
            ("OPENAI_API_KEY", "capacity"),
            ("OPENAI_BASE_URL", judge.base_url()),
            ("SSL_CERT_FILE", ca_file.as_str()),
            ("WYRD_STORAGE_URL", cli.storage_url.as_str()),
            (
                "WYRD_STORAGE_ENDPOINT_URL",
                cli.storage_endpoint_url.as_str(),
            ),
            (
                "WYRD_PEER_TLS_DIR",
                peer_dirs[usize::from(ordinal)].as_str(),
            ),
            // Every replica is `all`, the recommended first way to scale out.
            ("WYRD_SIGNING_KEY_FILE", signing_key.as_str()),
        ]
    };

    let first = LocalServer::start(&binary, &TENANTS, &env(0)).await?;
    let mut auth_pace = tokio::time::interval(AUTH_SPACING);
    auth_pace.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut tenants = Vec::new();
    for setup in first.tenants() {
        auth_pace.tick().await;
        tenants.push(Tenant::provision(setup, &work.path().join("tenants")).await?);
    }
    let setup_seconds = started.elapsed().as_secs_f64();

    let mut deployment = Deployment {
        replicas: vec![first],
        tenants,
        clients: Vec::new(),
        queue: Queue::connect().await?,
        judge: Arc::clone(&judge),
        permits: permits(),
        profiles: cli.profile.then(|| profiles.clone()),
        binary: identity(&binary, cli.profile)?,
    };
    let plan = |name, level, seconds, judged| Plan {
        name,
        level,
        seconds,
        judged,
        sample_floor: false,
    };
    let lowest = cli
        .levels
        .first()
        .copied()
        .ok_or("--levels names no level")?;
    let mut records = Vec::new();
    connect(&mut deployment, &mut auth_pace).await?;
    records.push(
        deployment
            .run(plan("warmup", lowest, cli.warmup_seconds, false))
            .await?,
    );
    let mut knee = None;
    for level in &cli.levels {
        let record = deployment
            .run(plan("ramp", *level, cli.ramp_seconds, true))
            .await?;
        let passed = step_row(&record).passed();
        records.push(record);
        if !passed {
            break;
        }
        knee = Some(*level);
    }
    if let Some(knee) = knee {
        records.push(
            deployment
                .run(Plan {
                    sample_floor: true,
                    ..plan("sustained", knee, cli.sustained_seconds, true)
                })
                .await?,
        );
        while deployment.replicas.len() < usize::from(REPLICAS) {
            let ordinal = u16::try_from(deployment.replicas.len())?;
            let replica = deployment.replicas[0]
                .start_replica(&binary, ordinal, &env(ordinal))
                .await?;
            deployment.replicas.push(replica);
        }
        connect(&mut deployment, &mut auth_pace).await?;
        records.push(
            deployment
                .run(plan("sustained", knee, cli.sustained_seconds, true))
                .await?,
        );
        records.push(
            deployment
                .run(plan("scale-out", 2.0 * knee, cli.sustained_seconds, true))
                .await?,
        );
    }
    for clients in deployment.clients.drain(..) {
        clients.shutdown().await?;
    }

    let mut shutdown = Vec::new();
    for replica in deployment.replicas.drain(..).rev() {
        let log = output.join(format!("server-{}.log", replica.ordinal()));
        shutdown.push(replica.stop(&log).map_err(|error| error.to_string()));
    }
    shutdown.reverse();
    let report = Report {
        profiled: cli.profile,
        setup_seconds,
        total_seconds: started.elapsed().as_secs_f64(),
        envelope: serde_json::json!({
            "cpus": CPUS,
            "memory_bytes": MEMORY_BYTES,
            "tenants": TENANTS.len(),
            "levels": cli.levels,
            "warmup_seconds": cli.warmup_seconds,
            "ramp_seconds": cli.ramp_seconds,
            "sustained_seconds": cli.sustained_seconds,
            "replicas": REPLICAS,
            "driver_permits": MAX_IN_FLIGHT,
            "binary": deployment.binary,
        }),
        records,
        knee,
        shutdown,
    };
    let rendered = report.write_to(&output, if cli.profile { "-profile" } else { "" })?;
    println!("{rendered}");
    Ok(report.passed())
}

/// Replaces every tenant's clients with ones connected to every replica of
/// `deployment`, one tenant per `pace` tick so the replicas' per-IP `/auth/*`
/// limit is never tripped.
///
/// # Errors
///
/// Returns a client shutdown or connect failure.
async fn connect(deployment: &mut Deployment, pace: &mut tokio::time::Interval) -> Result<()> {
    for clients in deployment.clients.drain(..) {
        clients.shutdown().await?;
    }
    let urls: Vec<String> = deployment.replicas.iter().map(LocalServer::url).collect();
    for tenant in &deployment.tenants {
        pace.tick().await;
        deployment
            .clients
            .push(Arc::new(TenantClients::connect(tenant, &urls).await?));
    }
    Ok(())
}

/// The identity of the measured `binary`: path, size, and modification
/// time, and whether it is the profiling build.
///
/// # Errors
///
/// Returns the metadata failure.
fn identity(binary: &Path, profiled: bool) -> Result<serde_json::Value> {
    let metadata = std::fs::metadata(binary)?;
    let modified: chrono::DateTime<chrono::Utc> = metadata.modified()?.into();
    Ok(serde_json::json!({
        "path": binary.display().to_string(),
        "bytes": metadata.len(),
        "modified": modified,
        "profiling_build": profiled,
    }))
}

/// The release `wyrd-server` built beside this binary.
///
/// # Errors
///
/// Returns an error when it has not been built.
fn release_binary() -> Result<PathBuf> {
    let binary = std::env::current_exe()?.with_file_name("wyrd-server");
    if binary.is_file() {
        Ok(binary)
    } else {
        Err(format!("{} is not built; run through mise", binary.display()).into())
    }
}

/// Installs a stderr log subscriber when `WYRD_LOG`, else `RUST_LOG`, is set,
/// so client-side failures read alongside the server logs.
fn install_tracing() {
    let Ok(filter) = std::env::var("WYRD_LOG").or_else(|_| std::env::var("RUST_LOG")) else {
        return;
    };
    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
            .with_writer(std::io::stderr)
            .finish(),
    );
}

/// Runs the benchmark and exits nonzero on a failed verdict or error.
#[tokio::main]
async fn main() -> ExitCode {
    install_tracing();
    match benchmark(Cli::parse()).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("capacity benchmark failed: {error}\n{error:?}");
            ExitCode::FAILURE
        }
    }
}
