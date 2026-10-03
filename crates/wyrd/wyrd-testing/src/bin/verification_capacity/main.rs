//! Verification capacity benchmark (REQ-171, AC-040).
//!
//! An operator starts release `wyrd-server` replicas the way the
//! local-development guide says (`migrate`, serve, `setup`), in peer mode
//! over one shared S3-compatible store, as the kind deployment runs them,
//! with the server's own sampled traces exported to a local OTLP collector
//! and its LLM provider pointed at a local TLS judge that answers after a
//! fixed delay. Tenant administrators register the AC-040 reference
//! workloads; the driver then offers open-loop load through the public
//! client in fixed steps.
//!
//! Each replica count runs a warmup, then a ladder per path: queued, direct,
//! each kind alone and all kinds mixed, then queued and direct together.
//! A ladder stops at its first unsustainable step. A fairness step follows:
//! one noisy tenant at the top step beside a quiet tenant and every
//! background tenant. Every step drains before the next. The report in
//! `target/verification-capacity/` reconciles every request, checks
//! coexistence, cross-replica claims, and fairness, and applies the AC-040
//! overhead objective. Missing evidence never passes.
//!
//! Run through `mise run bench:verification:capacity`; `-- --profile` adds
//! per-step `perf` captures of a frame-pointer build, as diagnostic evidence
//! only. Exits nonzero when any check fails.

mod collector;
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
use wyrd_testing::capacity::{binary_identity, install_tracing, release_binary};
use wyrd_testing::release_server::{CPUS, LocalServer, MEMORY_BYTES};

use collector::Collector;
use evidence::Queue;
use fixture::{Kind, Tenant};
use judge::Judge;
use load::{MAX_IN_FLIGHT, Mode, TenantClients};
use report::{Report, sustainable};
use step::{Deployment, Plan, Record, lanes, permits};

/// Error type of every benchmark step: the binary only reports it.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Fraction of its own traces the server samples and exports.
const SAMPLE_RATIO: &str = "0.05";

/// Queued arrivals per second of each background tenant in the fairness
/// step.
const BACKGROUND_RATE: f64 = 0.5;

/// Peer certificate name every replica presents and dials.
const PEER_NAME: &str = "wyrd-peer";

/// Command-line settings.
#[derive(Debug, Parser)]
#[command(about = "Measures verification capacity of release wyrd-server replicas")]
struct Cli {
    /// Offered executions per second per path, across every kind, in ladder
    /// order.
    #[arg(long, value_delimiter = ',', default_value = "10,25,50,100,200")]
    steps: Vec<f64>,
    /// Arrival window of every step, seconds.
    #[arg(long, default_value_t = 30.0)]
    step_seconds: f64,
    /// Replica counts to measure, in order.
    #[arg(long, value_delimiter = ',', default_value = "1,2")]
    replicas: Vec<u16>,
    /// Background tenants in the fairness step.
    #[arg(long, default_value_t = 70)]
    background: usize,
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

/// The benchmark: start, provision, run every replica count, report.
///
/// Returns whether every check passed.
///
/// # Errors
///
/// Returns a server, client, or evidence failure that stopped the run.
async fn benchmark(cli: Cli) -> Result<bool> {
    let started = std::time::Instant::now();
    let output = PathBuf::from("target/verification-capacity");
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
    let collector = Arc::new(Collector::start().await?);
    let judge = Arc::new(Judge::start(&work.path().join("judge")).await?);
    let peer = BifrostPeerCa::generate(PEER_NAME)?;
    let max_replicas = cli.replicas.iter().copied().max().unwrap_or(1);
    let peer_dirs = (0..max_replicas)
        .map(|ordinal| {
            Ok(peer
                .materialize(&work.path().join("peer"), &format!("replica-{ordinal}"))?
                .dir
                .display()
                .to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    let ca_file = judge.ca_file().display().to_string();
    let env = |ordinal: u16| -> Vec<(&str, &str)> {
        vec![
            ("WYRD_OTLP_ENDPOINT", collector.trace_endpoint()),
            ("WYRD_OTLP_PROTOCOL", "grpc"),
            ("WYRD_OTLP_SAMPLE_RATIO", SAMPLE_RATIO),
            ("OPENAI_API_KEY", "verification-capacity"),
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
        ]
    };

    let slugs: Vec<String> = ["m0".to_owned(), "m1".to_owned()]
        .into_iter()
        .chain((0..cli.background).map(|index| format!("bg{index}")))
        .collect();
    let slug_refs: Vec<&str> = slugs.iter().map(String::as_str).collect();
    let first = LocalServer::start(&binary, &slug_refs, &env(0)).await?;
    let mut tenants = Vec::new();
    for (index, setup) in first.tenants().iter().enumerate() {
        tenants.push(Tenant::provision(setup, &work.path().join("tenants"), index < 2).await?);
    }
    let setup_seconds = started.elapsed().as_secs_f64();

    let mut deployment = Deployment {
        replicas: vec![first],
        tenants,
        clients: Vec::new(),
        queue: Queue::connect().await?,
        collector: Arc::clone(&collector),
        judge: Arc::clone(&judge),
        permits: permits(),
        profiles: cli.profile.then(|| profiles.clone()),
        binary: identity(&binary, cli.profile)?,
    };
    let mut records = Vec::new();
    for count in cli.replicas.iter().copied() {
        for clients in deployment.clients.drain(..) {
            clients.shutdown().await?;
        }
        while deployment.replicas.len() < usize::from(count) {
            let ordinal = u16::try_from(deployment.replicas.len())?;
            let replica = deployment.replicas[0]
                .start_replica(&binary, ordinal, &env(ordinal))
                .await?;
            deployment.replicas.push(replica);
        }
        let urls: Vec<String> = deployment
            .replicas
            .iter()
            .take(usize::from(count))
            .map(LocalServer::url)
            .collect();
        for tenant in &deployment.tenants {
            deployment
                .clients
                .push(Arc::new(TenantClients::connect(tenant, &urls, true).await?));
        }
        ladders(
            &deployment,
            &cli.steps,
            cli.step_seconds,
            cli.profile,
            &mut records,
        )
        .await?;
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
    let mut forbidden = Vec::new();
    for tenant in &deployment.tenants {
        forbidden.push(tenant.service_key.expose_secret().to_owned());
        forbidden.push(tenant.admin_key.expose_secret().to_owned());
    }
    let tenant_ids: Vec<String> = deployment
        .tenants
        .iter()
        .map(|tenant| tenant.tenant_id.clone())
        .collect();
    let report = Report {
        profiled: cli.profile,
        setup_seconds,
        total_seconds: started.elapsed().as_secs_f64(),
        envelope: serde_json::json!({
            "cpus": CPUS,
            "memory_bytes": MEMORY_BYTES,
            "steps": cli.steps,
            "step_seconds": cli.step_seconds,
            "replicas": cli.replicas,
            "background_tenants": cli.background,
            "max_in_flight": MAX_IN_FLIGHT,
            "sample_ratio": SAMPLE_RATIO,
            "binary": deployment.binary,
        }),
        records,
        traces: collector.finish(&tenant_ids, &forbidden),
        shutdown,
    };
    let rendered = report.write_to(&output, if cli.profile { "-profile" } else { "" })?;
    println!("{rendered}");
    Ok(report.passed())
}

/// Runs the warmup, every path's ladder, the combined ladder, and the
/// fairness step against `deployment`, appending each step's record.
///
/// # Errors
///
/// Returns the first step failure.
async fn ladders(
    deployment: &Deployment,
    steps: &[f64],
    seconds: f64,
    profiled: bool,
    records: &mut Vec<Record>,
) -> Result<()> {
    let per_kind = |rate: f64| rate / Kind::ALL.len() as f64;
    let lowest = steps.first().copied().unwrap_or(10.0);
    let highest = steps.last().copied().unwrap_or(lowest);
    let plan = |path, case: &str, rate, lanes| Plan {
        path,
        case: case.to_owned(),
        rate,
        seconds,
        lanes,
        profiled,
    };
    let mut warmup = lanes(0, &Kind::ALL, Mode::Queued, per_kind(lowest));
    warmup.extend(lanes(0, &Kind::ALL, Mode::Direct, per_kind(lowest)));
    records.push(
        deployment
            .run(plan("warmup", "all", lowest, warmup))
            .await?,
    );

    for mode in [Mode::Queued, Mode::Direct] {
        for kind in Kind::ALL {
            for rate in steps.iter().copied() {
                let record = deployment
                    .run(plan(
                        mode.label(),
                        kind.label(),
                        rate,
                        lanes(0, &[kind], mode, rate),
                    ))
                    .await?;
                let stop = !sustainable(&record);
                records.push(record);
                if stop {
                    break;
                }
            }
        }
        for rate in steps.iter().copied() {
            let record = deployment
                .run(plan(
                    mode.label(),
                    "mixed",
                    rate,
                    lanes(0, &Kind::ALL, mode, per_kind(rate)),
                ))
                .await?;
            let stop = !sustainable(&record);
            records.push(record);
            if stop {
                break;
            }
        }
    }
    for rate in steps.iter().copied() {
        let mut both = lanes(0, &Kind::ALL, Mode::Queued, per_kind(rate));
        both.extend(lanes(0, &Kind::ALL, Mode::Direct, per_kind(rate)));
        let record = deployment
            .run(plan("combined", "mixed", rate, both))
            .await?;
        let stop = !sustainable(&record);
        records.push(record);
        if stop {
            break;
        }
    }

    let mut fairness = lanes(0, &Kind::ALL, Mode::Queued, per_kind(highest));
    fairness.extend(lanes(1, &[Kind::Assertion], Mode::Queued, per_kind(lowest)));
    for tenant in 2..deployment.tenants.len() {
        fairness.extend(lanes(
            tenant,
            &[Kind::Assertion],
            Mode::Queued,
            BACKGROUND_RATE,
        ));
    }
    records.push(
        deployment
            .run(plan("fairness", "mixed", highest, fairness))
            .await?,
    );
    Ok(())
}

/// The identity of the measured `binary`, as [`binary_identity`] records it,
/// and whether it is the profiling build.
///
/// # Errors
///
/// Returns the metadata failure.
fn identity(binary: &Path, profiled: bool) -> Result<serde_json::Value> {
    let mut identity = binary_identity(binary)?;
    identity["profiling_build"] = serde_json::Value::Bool(profiled);
    Ok(identity)
}

/// Runs the benchmark and exits nonzero on any failed check or error.
#[tokio::main]
async fn main() -> ExitCode {
    install_tracing();
    match benchmark(Cli::parse()).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("verification capacity benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}
