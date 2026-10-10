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
//! The whole command, setup through exit, lives inside one absolute 30-minute
//! deadline (REQ-171) that `mise run bench:capacity` fixes before its first
//! setup action and hands this binary in [`STARTED_ENV`] and
//! [`DEADLINE_ENV`]: a run that has not finished by then stops, cleans up
//! within the time it reserved, and reports what it measured as a failure.
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
use std::time::{Duration, SystemTime};

use clap::Parser;
use secrecy::ExposeSecret as _;
use tokio::time::{Instant, Interval};
use wyrd_testing::bifrost::peer_ca::BifrostPeerCa;
use wyrd_testing::release_server::{CPUS, Envelope, LocalServer, MEMORY_BYTES, STOP_GRACE};

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

/// Unix second at which `mise run bench:capacity` began, before its first
/// setup action; the report's durations count from it.
const STARTED_ENV: &str = "WYRD_CAPACITY_STARTED";

/// Unix second by which the whole command must have exited (REQ-171).
const DEADLINE_ENV: &str = "WYRD_CAPACITY_DEADLINE";

/// What the lifetime keeps after the replicas stop: writing the report, the
/// Postgres wrapper's teardown, and the command's exit.
const EXIT_RESERVE: Duration = Duration::from_mins(1);

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
    /// Cluster administrator URL, exported by the Postgres wrapper; the
    /// queue backlog is read across every tenant as this superuser.
    #[arg(long, env = "WYRD_TEST_DATABASE_ADMIN_URL", hide_env_values = true)]
    database_admin_url: String,
}

/// One absolute command lifetime split into a measuring part and the
/// cleanup it reserves, so the command ends by its deadline however its
/// work behaves.
#[derive(Debug, Clone, Copy)]
struct Lifetime {
    /// When the command began.
    started: Instant,
    /// When the command must have exited.
    deadline: Instant,
    /// When measuring must stop: client shutdown, replica stops, and exit
    /// remain.
    measure_until: Instant,
    /// When client shutdown must stop: replica stops and exit remain.
    clients_until: Instant,
}

impl Lifetime {
    /// The lifetime `mise run bench:capacity` fixed in [`STARTED_ENV`] and
    /// [`DEADLINE_ENV`], reserving [`CLIENT_SHUTDOWN`], each replica's
    /// [`REPLICA_STOP`], and [`EXIT_RESERVE`] at its end.
    ///
    /// # Errors
    ///
    /// Returns an error when either variable is unset or not a Unix second,
    /// which means the binary is not running through mise.
    fn from_command() -> Result<Self> {
        let at = |name: &str| -> Result<Instant> {
            let seconds: u64 = std::env::var(name)
                .map_err(|_| format!("{name} is unset; run through mise run bench:capacity"))?
                .parse()?;
            Ok(instant_at(
                SystemTime::UNIX_EPOCH + Duration::from_secs(seconds),
            ))
        };
        Ok(Self::new(
            at(STARTED_ENV)?,
            at(DEADLINE_ENV)?,
            CLIENT_SHUTDOWN,
            REPLICA_STOP
                .saturating_mul(u32::from(REPLICAS))
                .saturating_add(EXIT_RESERVE),
        ))
    }

    /// A lifetime from `started` to `deadline`, reserving `client_shutdown`
    /// and then `finish` at its end. A reserve longer than the lifetime
    /// leaves no time before it, never time before `started`.
    fn new(
        started: Instant,
        deadline: Instant,
        client_shutdown: Duration,
        finish: Duration,
    ) -> Self {
        let clients_until = deadline.checked_sub(finish).unwrap_or(started).max(started);
        Self {
            started,
            deadline,
            measure_until: clients_until
                .checked_sub(client_shutdown)
                .unwrap_or(started)
                .max(started),
            clients_until,
        }
    }

    /// Whole seconds from start to deadline.
    fn limit_seconds(&self) -> u64 {
        self.deadline
            .saturating_duration_since(self.started)
            .as_secs()
    }

    /// Runs `work` until [`Lifetime::measure_until`]; returns why it did not
    /// finish, or `None` when it did.
    ///
    /// # Cancellation
    ///
    /// See [`Lifetime::bounded`]; whatever `work` recorded in its owner
    /// before the deadline stays there.
    async fn measure(&self, work: impl Future<Output = Result<()>>) -> Option<String> {
        self.bounded(self.measure_until, work, "measuring")
            .await
            .err()
    }

    /// Runs cleanup `work` until [`Lifetime::clients_until`]; returns why it
    /// did not finish, or `None` when it did.
    ///
    /// # Cancellation
    ///
    /// See [`Lifetime::bounded`]; cleanup `work` left unfinished at the
    /// deadline falls to the owners' `Drop`.
    async fn shut_down(&self, work: impl Future<Output = Result<()>>) -> Option<String> {
        self.bounded(self.clients_until, work, "client shutdown")
            .await
            .err()
    }

    /// Runs `work` until `deadline` and returns its output, or why it did
    /// not finish, naming `phase`.
    ///
    /// # Errors
    ///
    /// Returns `work`'s own failure, or a message that `phase` passed its
    /// share of the limit.
    ///
    /// # Cancellation
    ///
    /// The one place the benchmark cancels work. Cancellation is
    /// cooperative: at `deadline` `work` is dropped at its next yield, and
    /// work that blocks without yielding is not interrupted, so every child
    /// process it waits on must be polled, as [`LocalServer`] does. Nothing
    /// is rolled back: effects `work` already produced (database rows, store
    /// objects, records in its owner) stay as their owners left them, and
    /// processes or clients it started are stopped by their owners' `Drop`
    /// as the future drops. Whether a retry is safe depends on the
    /// interrupted workflow, whose own documentation says so.
    async fn bounded<T>(
        &self,
        deadline: Instant,
        work: impl Future<Output = Result<T>>,
        phase: &str,
    ) -> std::result::Result<T, String> {
        match tokio::time::timeout_at(deadline, work).await {
            Ok(Ok(output)) => Ok(output),
            Ok(Err(error)) => Err(format!("{phase} failed: {error}")),
            Err(_) => Err(format!(
                "{phase} passed its share of the command's {} s limit",
                self.limit_seconds()
            )),
        }
    }
}

/// The monotonic instant of wall-clock time `at`, which may be past.
fn instant_at(at: SystemTime) -> Instant {
    let now = Instant::now();
    match at.duration_since(SystemTime::now()) {
        Ok(ahead) => now + ahead,
        Err(behind) => now.checked_sub(behind.duration()).unwrap_or(now),
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
    /// Prepares the local fixtures under `lifetime`: output directory,
    /// scratch directory, server binary, judge provider, peer certificates,
    /// and signing key.
    ///
    /// # Errors
    ///
    /// Returns a file, binary, certificate, key, or judge start failure.
    ///
    /// # Cancellation
    ///
    /// Only local files and the judge listener exist yet; dropping the
    /// future removes the scratch directory and stops the judge.
    async fn prepare(cli: Cli, lifetime: Lifetime) -> Result<Self> {
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
        let mut failure = Box::pin(lifetime.measure(self.measure())).await;
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
                "limit_seconds": lifetime.limit_seconds(),
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
        let first = LocalServer::start(&self.binary, &TENANTS, &self.env(0), Envelope::POD).await?;
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
            queue: Queue::connect(&self.cli.database_admin_url).await?,
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
    /// Not cancelled by [`Benchmark::run`]; see [`Deployment::stop_replicas`].
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
        let shutdown = deployment.stop_replicas(&self.output).await;
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
    let cli = Cli::parse();
    let result = match Lifetime::from_command() {
        Ok(lifetime) => match lifetime
            .bounded(
                lifetime.measure_until,
                Benchmark::prepare(cli, lifetime),
                "preparation",
            )
            .await
        {
            Ok(benchmark) => Box::pin(benchmark.run()).await,
            Err(error) => Err(error.into()),
        },
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
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::{Path, PathBuf};
    use std::process::{Command, ExitStatus, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use clap::Parser as _;
    use tokio::time::Instant;

    use super::{Benchmark, Cli, Envelope, Lifetime, LocalServer};

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
        let started = Instant::now();
        let lifetime = Lifetime::new(
            started,
            started + Duration::from_mins(1),
            Duration::from_secs(5),
            Duration::from_secs(10),
        );
        let stopped = lifetime
            .measure(std::future::pending::<super::Result<()>>())
            .await;
        assert!(stopped.is_some_and(|failure| failure.contains("60 s limit")));
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

    /// A stand-in `wyrd-server` for the stalled-setup proof: `migrate`
    /// succeeds, serving answers `/readyz` from a static file server and
    /// exits cleanly two seconds after `SIGTERM`, and `setup` logs to stderr
    /// and never exits. Each invocation records its pid, the serve its
    /// working directory, and each `setup` its tenant in `scratch`.
    const STALLING_SERVER: &str = r#"#!/bin/sh
case "$1" in
  migrate) exit 0 ;;
  setup)
    echo "$3" >> "SCRATCH/setups"
    echo $$ > "SCRATCH/setup.pid"
    echo "setup stalled" >&2
    exec sleep 600 ;;
  *)
    echo $$ > "SCRATCH/serve.pid"
    pwd > "SCRATCH/serve.cwd"
    touch readyz
    exec python3 -c 'import http.server as h, signal, sys, time
signal.signal(signal.SIGTERM, lambda *_: (time.sleep(2), sys.exit(0)))
h.ThreadingHTTPServer(("127.0.0.1", 8080), h.SimpleHTTPRequestHandler).serve_forever()' ;;
esac
"#;

    /// Writes `program` into `path` with `SCRATCH` replaced by `scratch` and
    /// makes it executable.
    ///
    /// # Panics
    ///
    /// Panics when the file cannot be written.
    fn stand_in(path: &Path, program: &str, scratch: &Path) {
        std::fs::write(
            path,
            program.replace("SCRATCH", &scratch.display().to_string()),
        )
        .expect("write the stand-in");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .expect("make the stand-in executable");
    }

    /// Whether process `pid` is gone, reaped rather than left a zombie.
    fn reaped(scratch: &Path, file: &str) -> bool {
        let pid =
            std::fs::read_to_string(scratch.join(file)).expect("the stand-in recorded its pid");
        !Path::new(&format!("/proc/{}", pid.trim())).exists()
    }

    /// A tenant `setup` that never exits, with the first replica already
    /// serving, cannot hold the benchmark past its deadline: the run stops at
    /// its measuring share, kills and reaps the `setup` and the replica, keeps
    /// the replica's log and the `setup`'s stderr, starts no later `setup` or
    /// step, and writes a failed report, all before the deadline.
    ///
    /// # Panics
    ///
    /// Panics when the run outlives its deadline, a child survives, a later
    /// phase starts, a diagnostic is lost, or the report does not fail.
    #[tokio::test]
    #[ignore = "starts a stand-in server in a systemd user scope on port 8080; needs a delegating systemd user manager and python3"]
    async fn a_stalled_tenant_setup_stops_the_run_by_its_deadline() {
        let scratch = tempfile::tempdir().expect("scratch directory");
        let server = scratch.path().join("wyrd-server");
        stand_in(&server, STALLING_SERVER, scratch.path());
        let cli = Cli::parse_from([
            "capacity",
            "--server-binary",
            &server.display().to_string(),
            "--storage-url",
            "file:///unused",
            "--storage-endpoint-url",
            "http://127.0.0.1:1",
            "--database-admin-url",
            "postgres://unused",
        ]);
        let started = Instant::now();
        let deadline = started + Duration::from_secs(12);
        let lifetime = Lifetime::new(
            started,
            deadline,
            Duration::from_secs(1),
            Duration::from_secs(1),
        );
        let mut benchmark = Benchmark::prepare(cli, lifetime).await.expect("prepare");
        benchmark.output = scratch.path().join("capacity");
        std::fs::create_dir_all(&benchmark.output).expect("output directory");

        let passed = Box::pin(benchmark.run())
            .await
            .expect("the failed report is written");

        assert!(!passed, "a stopped run fails its verdict");
        assert!(Instant::now() < deadline, "the run ended by its deadline");
        assert!(
            reaped(scratch.path(), "setup.pid"),
            "the stalled setup was reaped"
        );
        assert!(
            reaped(scratch.path(), "serve.pid"),
            "the replica was reaped"
        );
        assert_eq!(
            std::fs::read_to_string(scratch.path().join("setups")).expect("setup ran"),
            "t0\n",
            "no later setup started"
        );
        let root = std::fs::read_to_string(scratch.path().join("serve.cwd")).expect("served");
        let root = Path::new(root.trim());
        assert!(
            root.join("server.log").is_file(),
            "the replica's log is kept"
        );
        assert!(
            std::fs::read_to_string(root.join("setup-t0.stderr"))
                .expect("the setup's stderr is kept")
                .contains("setup stalled")
        );
        let report: serde_json::Value = serde_json::from_slice(
            &std::fs::read(scratch.path().join("capacity/report.json")).expect("report written"),
        )
        .expect("report is JSON");
        assert_eq!(report["passed"], false);
        assert!(
            report["report"]["failure"]
                .as_str()
                .is_some_and(|failure| failure.starts_with("measuring passed its share"))
        );
        assert_eq!(
            report["report"]["records"],
            serde_json::json!([]),
            "no step started"
        );
        assert!(report["report"]["setup_seconds"].is_null());
        std::fs::remove_dir_all(root).expect("remove the kept replica directory");
    }

    /// A replica that takes two seconds to exit after `SIGTERM` stops off
    /// the test runtime's only worker through the benchmark's own cleanup:
    /// [`Benchmark::clean_up`] shuts the (empty) client set down and hands the
    /// deployment's replicas to [`super::Deployment::stop_replicas`]. A
    /// heartbeat task on that worker keeps ticking while the replica stops,
    /// and the stop still waits for the clean exit, reaps the process, keeps
    /// its log, and reports its time in the replica's ordinal slot.
    ///
    /// # Panics
    ///
    /// Panics when the heartbeat stalls, the cleanup fails or is too quick,
    /// the replica survives, or its log is lost.
    #[tokio::test]
    #[ignore = "starts a stand-in server in a systemd user scope on port 8080; needs a delegating systemd user manager and python3"]
    async fn a_slow_replica_stop_leaves_the_runtime_free() {
        let scratch = tempfile::tempdir().expect("scratch directory");
        let server = scratch.path().join("wyrd-server");
        stand_in(&server, STALLING_SERVER, scratch.path());
        let cli = Cli::parse_from([
            "capacity",
            "--server-binary",
            &server.display().to_string(),
            "--storage-url",
            "file:///unused",
            "--storage-endpoint-url",
            "http://127.0.0.1:1",
            "--database-admin-url",
            "postgres://unused",
        ]);
        let started = Instant::now();
        let lifetime = Lifetime::new(
            started,
            started + Duration::from_mins(1),
            Duration::from_secs(5),
            Duration::from_secs(10),
        );
        let mut benchmark = Benchmark::prepare(cli, lifetime).await.expect("prepare");
        benchmark.output = scratch.path().join("capacity");
        std::fs::create_dir_all(&benchmark.output).expect("output directory");
        let replica = LocalServer::start(&server, &[], &[], Envelope::POD)
            .await
            .expect("the stand-in serves");
        benchmark.deployment = Some(super::Deployment {
            replicas: vec![replica],
            tenants: Vec::new(),
            clients: Vec::new(),
            queue: super::Queue::unconnected().expect("lazy pool"),
            judge: Arc::clone(&benchmark.judge),
            permits: super::permits(),
            profiles: None,
            binary: serde_json::Value::Null,
        });

        let beats = Arc::new(AtomicUsize::new(0));
        let heartbeat = tokio::spawn({
            let beats = Arc::clone(&beats);
            async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    beats.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        let (failure, shutdown) = benchmark.clean_up().await;
        let beats = beats.load(Ordering::Relaxed);
        heartbeat.abort();

        assert_eq!(failure, None, "the empty client set shut down");
        assert!(
            beats >= 10,
            "the heartbeat ticked {beats} times during the stop"
        );
        assert!(
            matches!(shutdown.as_slice(), [Ok(seconds)] if *seconds >= 2.0),
            "the stop waited for the clean exit: {shutdown:?}"
        );
        assert!(
            reaped(scratch.path(), "serve.pid"),
            "the replica was reaped"
        );
        assert!(
            benchmark.output.join("server-0.log").is_file(),
            "the replica's log is kept"
        );
    }

    /// A stand-in `docker` or `cargo` for the task proofs. It logs its name
    /// and first argument; when that argument is `HANG` it ignores `SIGTERM`
    /// and starts a descendant that ignores it too, records its pid, and
    /// sleeps, then waits for it, so only a `SIGKILL` stops either.
    const TASK_STAND_IN: &str = r#"#!/bin/sh
echo "NAME $1" >> SCRATCH/calls
if [ "$1" = "HANG" ]; then
  trap '' TERM
  sh -c 'echo $$ > SCRATCH/descendant.pid; exec sleep 600' &
  wait
fi
"#;

    /// A stand-in Postgres wrapper: it logs its start, runs its command, and
    /// logs its teardown on exit, including after `SIGTERM`, as the real
    /// wrapper tears its database down.
    const WRAPPER_STAND_IN: &str = r#"#!/usr/bin/env bash
echo wrapper >> SCRATCH/calls
trap 'echo teardown >> SCRATCH/calls' EXIT
trap 'exit 143' TERM
shift
"$@"
"#;

    /// Runs the `bench:capacity` script exactly as `mise.toml` defines it,
    /// except for a `limit`-second deadline, from a scratch directory with
    /// stand-ins for `docker`, `cargo`, and the Postgres wrapper; the
    /// `docker` one hangs on `docker_hang` and the `cargo` one on
    /// `cargo_hang`. Returns the scratch directory, the task's exit status,
    /// and how long it took. `HOME` points at the scratch directory so the
    /// wrapped login shell keeps the stand-ins first on `PATH`.
    ///
    /// # Panics
    ///
    /// Panics when the task or a stand-in cannot be written or run.
    fn run_task(
        limit: u64,
        docker_hang: &str,
        cargo_hang: &str,
    ) -> (tempfile::TempDir, ExitStatus, Duration) {
        let mise =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../mise.toml"))
                .expect("read mise.toml");
        let task = &mise[mise
            .find("[tasks.\"bench:capacity\"]")
            .expect("the task exists")..];
        let open = "run = '''\n";
        let script = &task[task.find(open).expect("the task runs a script") + open.len()..];
        let script = &script[..script.find("\n'''").expect("the script ends")];
        assert_eq!(script.matches("30 * 60").count(), 1, "one 30-minute limit");
        let script = script.replace("30 * 60", &limit.to_string());

        let scratch = tempfile::tempdir().expect("scratch directory");
        let bin = scratch.path().join("bin");
        let wrapper = scratch.path().join("scripts/postgres");
        std::fs::create_dir_all(&bin).expect("stand-in directory");
        std::fs::create_dir_all(&wrapper).expect("wrapper directory");
        let program =
            |name: &str, hang: &str| TASK_STAND_IN.replace("NAME", name).replace("HANG", hang);
        stand_in(
            &bin.join("docker"),
            &program("docker", docker_hang),
            scratch.path(),
        );
        stand_in(
            &bin.join("cargo"),
            &program("cargo", cargo_hang),
            scratch.path(),
        );
        stand_in(
            &wrapper.join("with-test-postgres.sh"),
            WRAPPER_STAND_IN,
            scratch.path(),
        );
        let path = std::env::var_os("PATH").unwrap_or_default();
        let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&path)))
            .expect("PATH joins");
        let stderr = std::fs::File::create(scratch.path().join("stderr")).expect("stderr file");
        let started = std::time::Instant::now();
        let status = Command::new("sh")
            .args(["-o", "errexit", "-c", &script])
            .current_dir(scratch.path())
            .env("PATH", path)
            .env("HOME", scratch.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(stderr)
            .status()
            .expect("run the task");
        (scratch, status, started.elapsed())
    }

    /// Whether the process whose pid `scratch/file` holds no longer runs:
    /// gone, or a zombie its new parent has yet to reap.
    fn stopped(scratch: &Path, file: &str) -> bool {
        let pid =
            std::fs::read_to_string(scratch.join(file)).expect("the stand-in recorded its pid");
        std::fs::read_to_string(PathBuf::from(format!("/proc/{}/stat", pid.trim()))).map_or(
            true,
            |stat| {
                stat.rsplit(')')
                    .next()
                    .is_some_and(|rest| rest.trim_start().starts_with('Z'))
            },
        )
    }

    /// The calls the task stand-ins logged, in order.
    fn calls(scratch: &Path) -> String {
        std::fs::read_to_string(scratch.join("calls")).expect("the stand-ins logged")
    }

    /// A setup step before the Postgres wrapper whose descendant ignores
    /// `SIGTERM` cannot hold the task past its deadline: its whole process
    /// group is killed within its share, the task fails, and no later step
    /// starts.
    ///
    /// # Panics
    ///
    /// Panics when the task succeeds, outlives its deadline, leaves the
    /// descendant running, or starts a later step.
    #[test]
    #[ignore = "runs the bench:capacity task script with stand-ins for about 7 s"]
    fn a_hung_setup_step_is_killed_with_its_descendants_by_the_deadline() {
        let (scratch, status, took) = run_task(12, "compose", "-");

        assert!(!status.success(), "the task fails");
        assert!(took < Duration::from_secs(12), "the task took {took:?}");
        assert!(
            stopped(scratch.path(), "descendant.pid"),
            "the descendant was killed"
        );
        assert_eq!(calls(scratch.path()), "docker compose\n", "no later step");
    }

    /// The benchmark run inside the Postgres wrapper, whose descendant
    /// ignores `SIGTERM`, cannot hold the task past its deadline: its whole
    /// process group is killed within its share, the wrapper still tears
    /// down, and the task fails.
    ///
    /// # Panics
    ///
    /// Panics when the task succeeds, outlives its deadline, leaves the
    /// descendant running, or skips a step or the teardown.
    #[test]
    #[ignore = "runs the bench:capacity task script with stand-ins for about 30 s"]
    fn a_hung_benchmark_run_is_killed_with_its_descendants_by_the_deadline() {
        let (scratch, status, took) = run_task(40, "-", "run");

        assert!(!status.success(), "the task fails");
        assert!(took < Duration::from_secs(40), "the task took {took:?}");
        assert!(
            stopped(scratch.path(), "descendant.pid"),
            "the descendant was killed"
        );
        assert_eq!(
            calls(scratch.path()),
            "docker compose\ndocker compose\nwrapper\ncargo build\ncargo run\nteardown\n"
        );
    }
}
