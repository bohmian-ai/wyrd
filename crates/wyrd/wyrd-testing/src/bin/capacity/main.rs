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
//! The whole command, setup through cleanup, lives inside one [`LIMIT`]
//! (REQ-171): a run that has not finished by then stops, cleans up within
//! the time it reserved, and reports what it measured as a failure.
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

use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use secrecy::ExposeSecret as _;
use tokio::time::{Instant, Interval};
use wyrd_testing::bifrost::peer_ca::BifrostPeerCa;
use wyrd_testing::release_server::{CPUS, LocalServer, MEMORY_BYTES, STOP_GRACE};

use evidence::Queue;
use fixture::Tenant;
use judge::Judge;
use load::{MAX_IN_FLIGHT, TenantClients};
use report::{Report, step_row};
use step::{Deployment, Plan, Record, StepKind, permits};

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
const AUTH_SPACING: Duration = Duration::from_millis(400);

/// The whole command's lifetime, setup through cleanup (REQ-171).
const LIMIT: Duration = Duration::from_secs(30 * 60);

/// How long cleanup lets the tenants' clients flush and stop before
/// dropping them.
const CLIENT_SHUTDOWN: Duration = Duration::from_secs(30);

/// How long one replica's clean stop may take: [`STOP_GRACE`] plus the kill,
/// reap, and log copy after it.
const REPLICA_STOP: Duration = STOP_GRACE.saturating_add(Duration::from_secs(5));

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

/// One absolute benchmark lifetime split into a measuring part and the
/// cleanup it reserves, so the command ends by its deadline however its
/// work behaves.
#[derive(Debug, Clone, Copy)]
struct Lifetime {
    /// When the benchmark began.
    started: Instant,
    /// When measuring must stop: client shutdown and replica stops remain.
    measure_until: Instant,
    /// When client shutdown must stop: replica stops remain.
    clients_until: Instant,
}

impl Lifetime {
    /// A lifetime of `limit` from now, reserving `client_shutdown` and then
    /// `replica_stops` at its end.
    fn new(limit: Duration, client_shutdown: Duration, replica_stops: Duration) -> Self {
        let started = Instant::now();
        let clients_until = started + limit.saturating_sub(replica_stops);
        Self {
            started,
            measure_until: clients_until
                .checked_sub(client_shutdown)
                .unwrap_or(started),
            clients_until,
        }
    }

    /// Runs `work` until [`Lifetime::measure_until`]; returns why it did not
    /// finish, or `None` when it did.
    ///
    /// # Cancellation
    ///
    /// At the deadline `work` is dropped at its next await; whatever it
    /// recorded in its owner before then stays there.
    async fn measure(&self, work: impl Future<Output = Result<()>>) -> Option<String> {
        Self::bounded(self.measure_until, work, "measuring").await
    }

    /// Runs cleanup `work` until [`Lifetime::clients_until`]; returns why it
    /// did not finish, or `None` when it did.
    ///
    /// # Cancellation
    ///
    /// At the deadline `work` is dropped, leaving its remaining cleanup to
    /// the owners' `Drop`.
    async fn shut_down(&self, work: impl Future<Output = Result<()>>) -> Option<String> {
        Self::bounded(self.clients_until, work, "client shutdown").await
    }

    /// Runs `work` until `deadline`, naming `phase` in the failure.
    async fn bounded(
        deadline: Instant,
        work: impl Future<Output = Result<()>>,
        phase: &str,
    ) -> Option<String> {
        match tokio::time::timeout_at(deadline, work).await {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(format!("{phase} failed: {error}")),
            Err(_) => Some(format!(
                "{phase} passed its share of the {} minute limit",
                LIMIT.as_secs() / 60
            )),
        }
    }
}

/// The benchmark run: its settings, lifetime, local fixtures, the
/// deployment once started, and the evidence measured so far. It owns the
/// setup-to-report lifecycle, so an interrupted run still cleans up and
/// reports what it holds.
struct Benchmark {
    /// Command-line settings.
    cli: Cli,
    /// The deadline every phase runs under.
    lifetime: Lifetime,
    /// Where the report, logs, and profiles land.
    output: PathBuf,
    /// Scratch directory of the judge CA, peer certificates, signing key,
    /// and tenant bundles; removed when the benchmark drops.
    work: tempfile::TempDir,
    /// The measured `wyrd-server`.
    binary: PathBuf,
    /// The judge provider every replica calls.
    judge: Arc<Judge>,
    /// The judge CA file every replica trusts.
    ca_file: String,
    /// Each replica's peer TLS directory, by ordinal.
    peer_dirs: Vec<String>,
    /// The workload signing key file every replica shares.
    signing_key: String,
    /// Paces tenant authentication under the replicas' `/auth/*` limit.
    auth_pace: Interval,
    /// The deployment, once the first replica and tenants are up.
    deployment: Option<Deployment>,
    /// Every finished step, in order.
    records: Vec<Record>,
    /// The highest passing ramp level.
    knee: Option<f64>,
    /// Seconds setup took, once it finished.
    setup_seconds: Option<f64>,
}

impl Benchmark {
    /// Prepares the local fixtures: output directory, scratch directory,
    /// server binary, judge provider, peer certificates, and signing key.
    /// The lifetime starts here, before any server starts.
    ///
    /// # Errors
    ///
    /// Returns a file, binary, certificate, key, or judge start failure.
    ///
    /// # Cancellation
    ///
    /// Only local files and the judge listener exist yet; dropping the
    /// future removes the scratch directory and stops the judge.
    async fn prepare(cli: Cli) -> Result<Self> {
        let lifetime = Lifetime::new(
            LIMIT,
            CLIENT_SHUTDOWN,
            REPLICA_STOP.saturating_mul(u32::from(REPLICAS)),
        );
        let output = PathBuf::from("target/capacity");
        std::fs::create_dir_all(&output)?;
        let profiles = output.join("profiles");
        if cli.profile && profiles.exists() {
            std::fs::remove_dir_all(&profiles)?;
        }
        let work = tempfile::Builder::new()
            .prefix("wyrd-capacity-")
            .tempdir()?;
        let binary = match &cli.server_binary {
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
        // One workload signing key for every replica, as the kind deployment
        // mounts one secret into every pod: a token one replica mints must
        // verify on every other replica.
        let signing_key = work.path().join("signing.pem");
        std::fs::write(
            &signing_key,
            wyrd_auth_issue::IssuingKey::generate_ephemeral_pem()?.expose_secret(),
        )?;
        let mut auth_pace = tokio::time::interval(AUTH_SPACING);
        auth_pace.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Ok(Self {
            ca_file: judge.ca_file().display().to_string(),
            signing_key: signing_key.display().to_string(),
            cli,
            lifetime,
            output,
            work,
            binary,
            judge,
            peer_dirs,
            auth_pace,
            deployment: None,
            records: Vec::new(),
            knee: None,
            setup_seconds: None,
        })
    }

    /// Measures inside the lifetime, cleans up, writes the report, and
    /// returns whether the verdict passed. A failure or the deadline stops
    /// measuring; the report then records why and fails.
    ///
    /// # Errors
    ///
    /// Returns the report encoding or write failure.
    ///
    /// # Cancellation
    ///
    /// Not cancelled by the binary. Dropping it drops the deployment, whose
    /// replicas are killed with their logs kept and whose clients drop their
    /// queued observations; durable rows from finished requests remain.
    async fn run(mut self) -> Result<bool> {
        let lifetime = self.lifetime;
        let mut failure = lifetime.measure(self.measure()).await;
        let (stopped, shutdown) = self.clean_up().await;
        if failure.is_none() {
            failure = stopped;
        }
        let report = Report {
            profiled: self.cli.profile,
            setup_seconds: self.setup_seconds,
            total_seconds: lifetime.started.elapsed().as_secs_f64(),
            failure,
            envelope: serde_json::json!({
                "cpus": CPUS,
                "memory_bytes": MEMORY_BYTES,
                "tenants": TENANTS.len(),
                "levels": self.cli.levels,
                "warmup_seconds": self.cli.warmup_seconds,
                "ramp_seconds": self.cli.ramp_seconds,
                "sustained_seconds": self.cli.sustained_seconds,
                "replicas": REPLICAS,
                "driver_permits": MAX_IN_FLIGHT,
                "limit_seconds": LIMIT.as_secs(),
                "binary": identity(&self.binary, self.cli.profile)?,
            }),
            records: std::mem::take(&mut self.records),
            knee: self.knee,
            shutdown,
        };
        let rendered =
            report.write_to(&self.output, if self.cli.profile { "-profile" } else { "" })?;
        println!("{rendered}");
        Ok(report.passed())
    }

    /// The REQ-171 sequence: setup, warmup, the ramp to the knee, sustained
    /// on one replica, the second replica, sustained on two, and scale-out.
    ///
    /// # Errors
    ///
    /// Returns the first setup, connect, or step failure.
    ///
    /// # Cancellation
    ///
    /// [`Benchmark::run`] drops it at the deadline. Finished steps stay in
    /// [`Benchmark::records`] and are reported; the interrupted step is
    /// discarded (see [`Deployment::run`]). Started replicas stay in the
    /// deployment for cleanup.
    async fn measure(&mut self) -> Result<()> {
        self.provision().await?;
        self.reconnect().await?;
        let lowest = self
            .cli
            .levels
            .first()
            .copied()
            .ok_or("--levels names no level")?;
        self.step(Plan::new(StepKind::Warmup, lowest, self.cli.warmup_seconds))
            .await?;
        for level in self.cli.levels.clone() {
            if !self
                .step(Plan::new(StepKind::Ramp, level, self.cli.ramp_seconds))
                .await?
            {
                break;
            }
            self.knee = Some(level);
        }
        let Some(knee) = self.knee else {
            return Ok(());
        };
        let sustained = self.cli.sustained_seconds;
        self.step(Plan {
            sample_floor: true,
            ..Plan::new(StepKind::Sustained, knee, sustained)
        })
        .await?;
        self.scale_out().await?;
        self.reconnect().await?;
        self.step(Plan::new(StepKind::Sustained, knee, sustained))
            .await?;
        self.step(Plan::new(StepKind::ScaleOut, 2.0 * knee, sustained))
            .await?;
        Ok(())
    }

    /// Starts the first replica, runs `setup` for every tenant, provisions
    /// each tenant's workloads one auth tick apart, and builds the
    /// deployment.
    ///
    /// # Errors
    ///
    /// Returns a server start, provisioning, or database connect failure.
    ///
    /// # Cancellation
    ///
    /// The first replica is killed with its log kept when the future drops
    /// before the deployment holds it. Tenants, Cards, fitted baselines, and
    /// seeded observations already written stay in the database and store;
    /// a retry starts a fresh database through the Postgres wrapper.
    async fn provision(&mut self) -> Result<()> {
        let first = LocalServer::start(&self.binary, &TENANTS, &self.env(0)).await?;
        let mut tenants = Vec::new();
        for setup in first.tenants() {
            self.auth_pace.tick().await;
            tenants.push(Tenant::provision(setup, &self.work.path().join("tenants")).await?);
        }
        self.setup_seconds = Some(self.lifetime.started.elapsed().as_secs_f64());
        self.deployment = Some(Deployment {
            replicas: vec![first],
            tenants,
            clients: Vec::new(),
            queue: Queue::connect().await?,
            judge: Arc::clone(&self.judge),
            permits: permits(),
            profiles: self.cli.profile.then(|| self.output.join("profiles")),
            binary: identity(&self.binary, self.cli.profile)?,
        });
        Ok(())
    }

    /// Joins replicas until the deployment has [`REPLICAS`].
    ///
    /// # Errors
    ///
    /// Returns a missing deployment or a replica start failure.
    ///
    /// # Cancellation
    ///
    /// A joined replica is in the deployment and stopped by cleanup; one
    /// still starting is killed with its log kept.
    async fn scale_out(&mut self) -> Result<()> {
        loop {
            let ordinal = u16::try_from(self.deployment()?.replicas.len())?;
            if ordinal >= REPLICAS {
                return Ok(());
            }
            let replica = self.deployment()?.replicas[0]
                .start_replica(&self.binary, ordinal, &self.env(ordinal))
                .await?;
            self.deployment_mut()?.replicas.push(replica);
        }
    }

    /// Replaces every tenant's clients with ones connected to every replica
    /// of the deployment, one tenant per auth tick so the replicas' per-IP
    /// `/auth/*` limit is never tripped. The deployment's clients then match
    /// its replicas and tenants.
    ///
    /// # Errors
    ///
    /// Returns a missing deployment or a client shutdown or connect failure.
    ///
    /// # Cancellation
    ///
    /// Old clients already shut down are gone; new ones already connected
    /// are in the deployment and shut down by cleanup. A partial client set
    /// is never measured, since a step only follows a finished reconnect.
    async fn reconnect(&mut self) -> Result<()> {
        let deployment = self.deployment.as_mut().ok_or("no deployment to connect")?;
        for clients in deployment.clients.drain(..) {
            clients.shutdown().await?;
        }
        let urls: Vec<String> = deployment.replicas.iter().map(LocalServer::url).collect();
        for tenant in &deployment.tenants {
            self.auth_pace.tick().await;
            deployment
                .clients
                .push(Arc::new(TenantClients::connect(tenant, &urls).await?));
        }
        Ok(())
    }

    /// Runs `plan`, keeps its record, and returns whether its row passed.
    ///
    /// # Errors
    ///
    /// Returns a missing deployment or the step's failure.
    ///
    /// # Cancellation
    ///
    /// See [`Deployment::run`]: an interrupted step leaves no record.
    async fn step(&mut self, plan: Plan) -> Result<bool> {
        let record = self.deployment()?.run(plan).await?;
        let passed = step_row(&record).passed();
        self.records.push(record);
        Ok(passed)
    }

    /// Shuts the clients down within the lifetime's client reserve, then
    /// stops every replica newest first, copying each log into the output.
    /// Returns a client shutdown failure, if any, and each replica's stop in
    /// ordinal order.
    ///
    /// # Cancellation
    ///
    /// Not cancelled by [`Benchmark::run`]; each replica stop is synchronous
    /// and bounded by [`STOP_GRACE`].
    async fn clean_up(&mut self) -> (Option<String>, Vec<std::result::Result<f64, String>>) {
        let Some(mut deployment) = self.deployment.take() else {
            return (None, Vec::new());
        };
        let clients = std::mem::take(&mut deployment.clients);
        let failure = self
            .lifetime
            .shut_down(async {
                for clients in clients {
                    clients.shutdown().await?;
                }
                Ok(())
            })
            .await;
        let mut shutdown = Vec::new();
        for replica in deployment.replicas.drain(..).rev() {
            let log = self
                .output
                .join(format!("server-{}.log", replica.ordinal()));
            shutdown.push(replica.stop(&log).map_err(|error| error.to_string()));
        }
        shutdown.reverse();
        (failure, shutdown)
    }

    /// The environment of replica `ordinal`: the judge provider and its CA,
    /// the shared store, its peer certificates, and the shared signing key.
    /// Every replica is `all`, the recommended first way to scale out.
    fn env(&self, ordinal: u16) -> Vec<(&str, &str)> {
        vec![
            ("OPENAI_API_KEY", "capacity"),
            ("OPENAI_BASE_URL", self.judge.base_url()),
            ("SSL_CERT_FILE", self.ca_file.as_str()),
            ("WYRD_STORAGE_URL", self.cli.storage_url.as_str()),
            (
                "WYRD_STORAGE_ENDPOINT_URL",
                self.cli.storage_endpoint_url.as_str(),
            ),
            (
                "WYRD_PEER_TLS_DIR",
                self.peer_dirs
                    .get(usize::from(ordinal))
                    .map_or("", String::as_str),
            ),
            ("WYRD_SIGNING_KEY_FILE", self.signing_key.as_str()),
        ]
    }

    /// The deployment.
    ///
    /// # Errors
    ///
    /// Returns an error before setup built it.
    fn deployment(&self) -> Result<&Deployment> {
        Ok(self.deployment.as_ref().ok_or("no deployment yet")?)
    }

    /// The deployment, mutably.
    ///
    /// # Errors
    ///
    /// Returns an error before setup built it.
    fn deployment_mut(&mut self) -> Result<&mut Deployment> {
        Ok(self.deployment.as_mut().ok_or("no deployment yet")?)
    }
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
    let result = match Benchmark::prepare(Cli::parse()).await {
        Ok(benchmark) => benchmark.run().await,
        Err(error) => Err(error),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("capacity benchmark failed: {error}\n{error:?}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::Lifetime;

    /// A benchmark that cannot finish fails when its measuring share ends,
    /// and a cleanup that cannot finish stops at its own reserve, so both
    /// end before the lifetime's deadline; finished and failed work report
    /// accordingly.
    ///
    /// # Panics
    ///
    /// Panics when a phase outlives its bound or reports wrongly.
    #[tokio::test(start_paused = true)]
    async fn an_unfinished_benchmark_fails_and_cleans_up_by_its_deadline() {
        let lifetime = Lifetime::new(
            Duration::from_secs(60),
            Duration::from_secs(5),
            Duration::from_secs(10),
        );
        let started = Instant::now();
        let stopped = lifetime
            .measure(std::future::pending::<super::Result<()>>())
            .await;
        assert!(stopped.is_some_and(|failure| failure.contains("30 minute limit")));
        assert_eq!(started.elapsed(), Duration::from_secs(45));
        let cleanup = lifetime
            .shut_down(std::future::pending::<super::Result<()>>())
            .await;
        assert!(cleanup.is_some_and(|failure| failure.starts_with("client shutdown")));
        assert_eq!(started.elapsed(), Duration::from_secs(50));
        assert_eq!(lifetime.measure(async { Ok(()) }).await, None);
        assert!(
            lifetime
                .measure(async { Err("setup broke".into()) })
                .await
                .is_some_and(|failure| failure.contains("setup broke"))
        );
    }
}
