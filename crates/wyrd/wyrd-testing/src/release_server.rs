//! One release `wyrd-server`, started the way the local-development guide
//! (`docs/src/content/docs/self-hosting/local-development.svx`) tells an
//! operator to, for the opt-in benchmark binaries.
//!
//! The operator steps are: the database owner runs `wyrd-server migrate`;
//! `wyrd-server` serves with the environment the Postgres wrapper exported;
//! `wyrd-server setup --tenant` prints each tenant's admin credential, the
//! first run also disclosing the platform credential the later runs present.
//! The benchmarks add only what the guide leaves to the operator:
//! `WYRD_STORAGE_URL`, which has no default, pointing at a `file://`
//! directory; a working directory so the server's `.wyrd/` state lands in a
//! temporary root; a systemd scope that gives the process the
//! 8-CPU/16-GiB pod envelope; and any extra environment a benchmark names.
//! A further replica joins the same Postgres and store on its own ports, as
//! the configuration guide's peer mode describes.
//! The wrapper's test-fixture `WYRD_DB_MAX_CONNECTIONS` cap is removed, so
//! the pool runs at the server's default. Every other setting is the server's
//! default.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read as _, Seek as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use secrecy::SecretString;

/// The server's default public HTTP address, as the guide exports it in
/// `WYRD_SERVER_URL`.
pub const SERVER_URL: &str = "http://127.0.0.1:8080";

/// The server's default public HTTP port, which replica 0 binds.
const HTTP_PORT: u16 = 8080;

/// The server's default Bifrost peer port, which replica 0 binds.
const PEER_PORT: u16 = 50052;

/// Port offset between consecutive replicas' listeners. The metrics
/// listener follows the HTTP port by the server's default of one.
const REPLICA_PORT_STRIDE: u16 = 10;

/// Base of a joined replica's gRPC port. Replica 0 keeps the server defaults,
/// 50051 for gRPC and [`PEER_PORT`] for peers. Those lie in
/// Linux's ephemeral range (32768-60999); replica 0 binds them before any
/// load, but a replica joining mid-run would race outbound connections that
/// already took those local ports, so joined replicas listen below the range.
const JOINED_GRPC_PORT: u16 = 30051;

/// Base of a joined replica's Bifrost peer port; see [`JOINED_GRPC_PORT`].
const JOINED_PEER_PORT: u16 = 30052;

/// CPUs of quota in the pod envelope.
pub const CPUS: u64 = 8;

/// Memory limit of the pod envelope, with no swap.
pub const MEMORY_BYTES: u64 = 16 << 30;

/// How long the server may take to report ready on `/readyz`.
const READY_TIMEOUT: Duration = Duration::from_secs(120);

/// How long the server may take to exit after `SIGTERM`: the kind pods'
/// `terminationGracePeriodSeconds`, 10 s beyond the server's default 35 s
/// shutdown drain budget. A benchmark that bounds its own lifetime reserves
/// this much per replica for [`LocalServer::stop`].
pub const STOP_GRACE: Duration = Duration::from_secs(45);

/// Why starting, observing, or stopping the release server failed.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseServerError {
    /// A process, file, or cgroup operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A `/readyz` or `/metrics` request failed.
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    /// A cgroup file held a non-integer value.
    #[error(transparent)]
    Parse(#[from] std::num::ParseIntError),
    /// The server or an operator subcommand misbehaved.
    #[error("{0}")]
    Server(String),
}

impl From<&str> for ReleaseServerError {
    /// Wraps a static failure description.
    fn from(message: &str) -> Self {
        Self::Server(message.to_owned())
    }
}

impl From<String> for ReleaseServerError {
    /// Wraps a formatted failure description.
    fn from(message: String) -> Self {
        Self::Server(message)
    }
}

/// Result of every release-server operation.
pub type Result<T> = std::result::Result<T, ReleaseServerError>;

/// One tenant `wyrd-server setup` provisioned.
pub struct SetupTenant {
    /// The tenant slug passed to `setup --tenant`.
    pub slug: String,
    /// The data tenant id `setup` printed.
    pub tenant_id: String,
    /// The tenant's admin credential, as the guide exports it in `WYRD_API_KEY`.
    pub api_key: SecretString,
}

/// A running `wyrd-server` and the tenants its `setup` runs provisioned.
///
/// Dropping it kills the process; [`LocalServer::stop`] is the clean exit.
pub struct LocalServer {
    /// Working directory holding the store, `.wyrd/` state, and the log.
    root: tempfile::TempDir,
    /// The serving process until it is reaped.
    child: Option<Child>,
    /// The process's cgroup-v2 directory, where its envelope is enforced.
    cgroup: PathBuf,
    /// Tenants in the order `setup` provisioned them; empty on a joined
    /// replica.
    tenants: Vec<SetupTenant>,
    /// Replica ordinal: 0 for the server [`LocalServer::start`] set up,
    /// which binds the default ports, and its listener offset otherwise.
    ordinal: u16,
}

impl LocalServer {
    /// Migrates and serves `binary` with `env` added, then runs `setup` once
    /// per slug in `tenants`, returning once `/readyz` answers 200, the
    /// cgroup enforces the envelope, and every tenant's credential was
    /// printed.
    ///
    /// The first `setup` initializes the platform root and discloses its
    /// credential; later runs present it as `WYRD_PLATFORM_CREDENTIAL`, as
    /// the guide tells an operator adding a tenant to do.
    ///
    /// # Errors
    ///
    /// Returns an error when `WYRD_TEST_DATABASE_ADMIN_URL` is unset (the
    /// benchmark is not running under the Postgres wrapper), `migrate` or a
    /// `setup` fails or prints no credential, the server exits or never
    /// becomes ready, or its cgroup does not enforce the
    /// [`CPUS`]/[`MEMORY_BYTES`] envelope.
    ///
    /// # Cancellation
    ///
    /// Every child is owned while this future is pending, so a caller's
    /// deadline drops it at its next yield without waiting for a child: a
    /// `migrate` or `setup` still running is killed and reaped with its
    /// stderr printed, and the serving replica is killed
    /// and reaped with its working directory, `server.log`, and each
    /// `setup`'s stderr file kept. Rows `migrate` and the finished `setup`s
    /// wrote stay in the database; a retry starts from a fresh database.
    pub async fn start(binary: &Path, tenants: &[&str], env: &[(&str, &str)]) -> Result<Self> {
        let owner_url = std::env::var("WYRD_TEST_DATABASE_ADMIN_URL")
            .map_err(|_| "WYRD_TEST_DATABASE_ADMIN_URL is unset; run through mise")?;
        let root = tempfile::Builder::new().prefix("wyrd-bench-").tempdir()?;
        let storage = root.path().join("storage");
        std::fs::create_dir_all(&storage)?;
        let storage_url = format!("file://{}", storage.display());
        let workdir = root.path().to_path_buf();
        OperatorRun::spawn(
            operator(binary, &workdir, &storage_url, 0, env)
                .arg("migrate")
                .env("WYRD_DATABASE_URL", owner_url),
            &workdir.join("migrate.stderr"),
        )?
        .finish()
        .await?;
        let mut server = Self::serve(binary, root, &storage_url, 0, env).await?;

        let operator = |program: &Path| operator(program, &workdir, &storage_url, 0, env);

        let mut platform: Option<String> = None;
        for slug in tenants {
            let mut setup = operator(binary);
            setup.args(["setup", "--tenant", slug]);
            if let Some(credential) = &platform {
                setup.env("WYRD_PLATFORM_CREDENTIAL", credential);
            }
            let printed =
                OperatorRun::spawn(&mut setup, &workdir.join(format!("setup-{slug}.stderr")))?
                    .finish()
                    .await?;
            let field = |name: &str| {
                printed
                    .lines()
                    .find_map(|line| line.strip_prefix(name))
                    .map(|value| value.trim().to_owned())
                    .ok_or_else(|| format!("`wyrd-server setup --tenant {slug}` printed no {name}"))
            };
            if platform.is_none() {
                platform = printed
                    .lines()
                    .find(|line| line.starts_with("wyrd_global_"))
                    .map(str::to_owned);
            }
            server.tenants.push(SetupTenant {
                slug: (*slug).to_owned(),
                tenant_id: field("tenant_id: ")?,
                api_key: SecretString::from(field("admin_credential: ")?),
            });
        }
        Ok(server)
    }

    /// Starts replica `ordinal` of this deployment: `binary` serving the same
    /// Postgres and store from its own working directory and envelope, with
    /// every listener offset by `ordinal` strides from its base: HTTP from
    /// the server default, gRPC and peer from [`JOINED_GRPC_PORT`] and
    /// [`JOINED_PEER_PORT`]. Nothing is migrated or set
    /// up; the replica serves the tenants this server provisioned.
    ///
    /// Several replicas need peer mode: give every replica, including this
    /// one, `WYRD_PEER_TLS_DIR` in `env`; its peer address is derived here.
    ///
    /// # Errors
    ///
    /// Returns an error when the process exits or never becomes ready, or its
    /// cgroup does not enforce the envelope.
    pub async fn start_replica(
        &self,
        binary: &Path,
        ordinal: u16,
        env: &[(&str, &str)],
    ) -> Result<Self> {
        let root = tempfile::Builder::new().prefix("wyrd-bench-").tempdir()?;
        let storage_url = format!("file://{}", self.storage_dir().display());
        Self::serve(binary, root, &storage_url, ordinal, env).await
    }

    /// Serves `binary` as replica `ordinal` from `root` in its envelope and
    /// waits until it is ready.
    ///
    /// # Errors
    ///
    /// Returns an error when the process cannot spawn, exits, never becomes
    /// ready, or its cgroup does not enforce the envelope.
    async fn serve(
        binary: &Path,
        root: tempfile::TempDir,
        storage_url: &str,
        ordinal: u16,
        env: &[(&str, &str)],
    ) -> Result<Self> {
        let log = File::create(root.path().join("server.log"))?;
        let child = operator(
            Path::new("systemd-run"),
            root.path(),
            storage_url,
            ordinal,
            env,
        )
        .args(["--user", "--scope", "--quiet", "--collect"])
        .arg(format!("--property=CPUQuota={}%", CPUS * 100))
        .arg(format!("--property=MemoryMax={MEMORY_BYTES}"))
        .arg("--property=MemorySwapMax=0")
        .arg("--")
        .arg(binary)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()?;
        let mut server = Self {
            cgroup: PathBuf::new(),
            child: Some(child),
            root,
            tenants: Vec::new(),
            ordinal,
        };
        server.await_ready().await?;
        server.cgroup = server.find_cgroup()?;
        server.confirm_envelope()?;
        Ok(server)
    }

    /// This replica's public HTTP base URL; [`SERVER_URL`] for replica 0.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port(HTTP_PORT))
    }

    /// This replica's ordinal.
    pub fn ordinal(&self) -> u16 {
        self.ordinal
    }

    /// PID of the process `systemd-run --scope` executes in place, so the
    /// serving `wyrd-server` itself; `None` once it was stopped.
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    /// `base` offset by this replica's ordinal.
    fn port(&self, base: u16) -> u16 {
        replica_port(base, self.ordinal)
    }

    /// The first tenant's admin credential, as the guide exports it in
    /// `WYRD_API_KEY`.
    ///
    /// # Panics
    ///
    /// Panics when the server was started with no tenant.
    pub fn api_key(&self) -> &SecretString {
        &self
            .tenants
            .first()
            .expect("the server was started with at least one tenant")
            .api_key
    }

    /// Every tenant `setup` provisioned, in order.
    pub fn tenants(&self) -> &[SetupTenant] {
        &self.tenants
    }

    /// The `file://` store the server publishes into.
    pub fn storage_dir(&self) -> PathBuf {
        self.root.path().join("storage")
    }

    /// Scrapes `/metrics` and parses the Prometheus text exposition.
    ///
    /// # Errors
    ///
    /// Returns an error when the endpoint does not answer 200.
    pub async fn metrics(&self) -> Result<Metrics> {
        let url = format!("http://127.0.0.1:{}/metrics", self.port(HTTP_PORT) + 1);
        let response = reqwest::get(url).await?.error_for_status()?;
        Ok(Metrics::parse(&response.text().await?))
    }

    /// Reads one `key value` line of a flat-keyed cgroup file such as
    /// `cpu.stat` or `memory.events`; zero when absent.
    pub fn cgroup_stat(&self, file: &str, key: &str) -> u64 {
        std::fs::read_to_string(self.cgroup.join(file))
            .unwrap_or_default()
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(' '))
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0)
    }

    /// Starts a fresh memory-peak measurement for this cgroup.
    ///
    /// Writing to `memory.peak` resets the peak seen through that file
    /// descriptor to current usage, so each measurement window gets its own
    /// peak.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be opened or reset.
    pub fn memory_peak(&self) -> Result<MemoryPeak> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.cgroup.join("memory.peak"))?;
        file.write_all(b"reset\n")?;
        Ok(MemoryPeak(file))
    }

    /// Sends `SIGTERM`, as `Ctrl-C` in the guide does, waits for a clean
    /// exit, and copies the server log to `log`. Returns the seconds the exit
    /// took.
    ///
    /// # Errors
    ///
    /// Returns an error when the process exits unsuccessfully or outlives
    /// [`STOP_GRACE`] (it is then killed), or the log cannot be copied.
    pub fn stop(mut self, log: &Path) -> Result<f64> {
        let started = Instant::now();
        let result = self.terminate();
        std::fs::copy(self.root.path().join("server.log"), log)?;
        result.map(|()| started.elapsed().as_secs_f64())
    }

    /// Signals the process and reaps it.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsuccessful exit or a kill after the grace.
    fn terminate(&mut self) -> Result<()> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        Command::new("kill")
            .args(["-s", "TERM", &child.id().to_string()])
            .status()?;
        let deadline = Instant::now() + STOP_GRACE;
        while Instant::now() < deadline {
            if let Some(status) = child.try_wait()? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("wyrd-server exited with {status}").into())
                };
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        child.kill()?;
        child.wait()?;
        Err(format!("wyrd-server did not exit within {}s", STOP_GRACE.as_secs()).into())
    }

    /// Polls `/readyz` until it answers 200, failing if the process exits.
    ///
    /// # Errors
    ///
    /// Returns an error when the process exits, carrying the last lines of
    /// its log, or the timeout passes, carrying the last `/readyz` body,
    /// which names each unready check.
    async fn await_ready(&mut self) -> Result<()> {
        let deadline = Instant::now() + READY_TIMEOUT;
        let mut last = String::new();
        while Instant::now() < deadline {
            if let Some(child) = self.child.as_mut()
                && let Some(status) = child.try_wait()?
            {
                let log = std::fs::read_to_string(self.root.path().join("server.log"))
                    .unwrap_or_default();
                let tail: Vec<&str> = log.lines().rev().take(20).collect();
                return Err(format!(
                    "wyrd-server exited during boot with {status}; log tail:\n{}",
                    tail.into_iter().rev().collect::<Vec<_>>().join("\n")
                )
                .into());
            }
            if let Ok(response) = reqwest::get(format!("{}/readyz", self.url())).await {
                if response.status().is_success() {
                    return Ok(());
                }
                last = response.text().await.unwrap_or_default();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(format!("wyrd-server never reported ready; last /readyz: {last}").into())
    }

    /// Finds the process's cgroup-v2 directory from `/proc/<pid>/cgroup`.
    ///
    /// # Errors
    ///
    /// Returns an error when the process is gone or has no unified cgroup.
    fn find_cgroup(&self) -> Result<PathBuf> {
        let pid = self.child.as_ref().ok_or("wyrd-server exited")?.id();
        let membership = std::fs::read_to_string(format!("/proc/{pid}/cgroup"))?;
        let path = membership
            .lines()
            .find_map(|line| line.strip_prefix("0::"))
            .ok_or("wyrd-server has no unified cgroup")?;
        Ok(Path::new("/sys/fs/cgroup").join(path.trim_start_matches('/')))
    }

    /// Confirms `cpu.max` allows exactly the envelope's CPUs and `memory.max`
    /// equals its bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when either differs, which means the systemd user
    /// manager does not delegate the `cpu` and `memory` controllers.
    fn confirm_envelope(&self) -> Result<()> {
        let read = |file: &str| std::fs::read_to_string(self.cgroup.join(file)).unwrap_or_default();
        let (cpu, memory) = (read("cpu.max"), read("memory.max"));
        let mut fields = cpu
            .split_whitespace()
            .map(|field| field.parse::<u64>().ok());
        let cpus = match (fields.next().flatten(), fields.next().flatten()) {
            (Some(quota), Some(period)) if period > 0 => quota / period,
            _ => 0,
        };
        if cpus == CPUS && memory.trim() == MEMORY_BYTES.to_string() {
            return Ok(());
        }
        Err(format!(
            "cgroup {} enforces cpu.max `{}` memory.max `{}`, not {CPUS} CPUs and {MEMORY_BYTES} bytes",
            self.cgroup.display(),
            cpu.trim(),
            memory.trim()
        )
        .into())
    }
}

impl Drop for LocalServer {
    /// Kills and reaps the server when [`LocalServer::stop`] was not reached.
    /// That only happens when the caller failed, so the working directory
    /// and its `server.log` are kept and their path printed for diagnosis.
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            self.root.disable_cleanup(true);
            eprintln!(
                "replica {} stopped abnormally; its log is kept at {}",
                self.ordinal,
                self.root.path().join("server.log").display()
            );
        }
    }
}

/// One open `memory.peak` descriptor whose peak was reset at open.
pub struct MemoryPeak(File);

impl MemoryPeak {
    /// Peak cgroup memory in bytes since this descriptor was reset.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read or parsed.
    pub fn read(mut self) -> Result<u64> {
        let mut contents = String::new();
        self.0.rewind()?;
        self.0.read_to_string(&mut contents)?;
        Ok(contents.trim().parse()?)
    }
}

/// One `/metrics` scrape: every series and its value.
pub struct Metrics(BTreeMap<String, f64>);

impl Metrics {
    /// Parses a Prometheus text exposition, skipping comments. Scrapes come
    /// from [`LocalServer::metrics`]; tests of scrape consumers build one
    /// from literal text.
    pub fn parse(exposition: &str) -> Self {
        Self(
            exposition
                .lines()
                .filter(|line| !line.starts_with('#'))
                .filter_map(|line| {
                    let (series, value) = line.rsplit_once(' ')?;
                    Some((series.to_owned(), value.parse().ok()?))
                })
                .collect(),
        )
    }

    /// Sums every series of `family` whose labels contain each of `labels`;
    /// an absent family reads as zero.
    pub fn sum(&self, family: &str, labels: &[&str]) -> f64 {
        self.0
            .iter()
            .filter(|(series, _)| {
                series.strip_prefix(family).is_some_and(|rest| {
                    (rest.is_empty() || rest.starts_with('{'))
                        && labels.iter().all(|label| rest.contains(label))
                })
            })
            .map(|(_, value)| value)
            .sum()
    }

    /// The cumulative `le` buckets of histogram `family`, summed across every
    /// series whose labels contain each of `labels`, in ascending bound
    /// order; `+Inf` reads as infinity.
    pub fn buckets(&self, family: &str, labels: &[&str]) -> Vec<(f64, f64)> {
        let mut buckets: BTreeMap<u64, (f64, f64)> = BTreeMap::new();
        let prefix = format!("{family}_bucket{{");
        for (series, value) in &self.0 {
            let Some(rest) = series.strip_prefix(&prefix) else {
                continue;
            };
            if !labels.iter().all(|label| rest.contains(label)) {
                continue;
            }
            let Some(bound) = rest
                .split("le=\"")
                .nth(1)
                .and_then(|tail| tail.split('"').next())
                .and_then(|bound| match bound {
                    "+Inf" => Some(f64::INFINITY),
                    finite => finite.parse::<f64>().ok(),
                })
            else {
                continue;
            };
            buckets.entry(bound.to_bits()).or_insert((bound, 0.0)).1 += value;
        }
        let mut sorted: Vec<(f64, f64)> = buckets.into_values().collect();
        sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
        sorted
    }
}

/// `base` offset by `ordinal` replica strides.
fn replica_port(base: u16, ordinal: u16) -> u16 {
    base + ordinal * REPLICA_PORT_STRIDE
}

/// A command running `program` as replica `ordinal` would: in `workdir`,
/// publishing to `storage_url`, at the server's default pool size, on the
/// replica's listeners, with `env` added. Peer mode derives the peer
/// listener and advertised address when `env` names `WYRD_PEER_TLS_DIR`.
fn operator(
    program: &Path,
    workdir: &Path,
    storage_url: &str,
    ordinal: u16,
    env: &[(&str, &str)],
) -> Command {
    let mut command = Command::new(program);
    command
        .current_dir(workdir)
        .env("WYRD_STORAGE_URL", storage_url)
        .env_remove("WYRD_DB_MAX_CONNECTIONS")
        .envs(env.iter().copied());
    if ordinal > 0 {
        command
            .env(
                "WYRD_SERVER_BIND",
                format!("127.0.0.1:{}", replica_port(HTTP_PORT, ordinal)),
            )
            .env(
                "WYRD_GRPC_BIND",
                format!("127.0.0.1:{}", replica_port(JOINED_GRPC_PORT, ordinal)),
            );
    }
    if env.iter().any(|(name, _)| *name == "WYRD_PEER_TLS_DIR") {
        let base = if ordinal == 0 {
            PEER_PORT
        } else {
            JOINED_PEER_PORT
        };
        let peer = format!("127.0.0.1:{}", replica_port(base, ordinal));
        command
            .env("WYRD_BIFROST_PEER_BIND_ADDR", &peer)
            .env("WYRD_PEER_ADDRESS", &peer);
    }
    command
}

/// One operator subcommand (`migrate`, `setup`) the harness owns until it
/// exits, so a caller's deadline can stop it instead of waiting on it.
///
/// Stdout goes to an unlinked file, since `setup` prints credentials, and
/// stderr to a file the caller names, kept for diagnosis; neither can fill a
/// pipe and stall the child.
struct OperatorRun {
    /// The running child until it is reaped.
    child: Option<Child>,
    /// The program and arguments, for failure messages; never the
    /// environment, which carries credentials.
    command: String,
    /// The child's stdout.
    stdout: File,
    /// Where the child's stderr lands.
    stderr: PathBuf,
}

impl OperatorRun {
    /// Spawns `command` with stdout to an unlinked file and stderr to
    /// `stderr`.
    ///
    /// # Errors
    ///
    /// Returns an error when either file cannot be created or the command
    /// cannot spawn.
    fn spawn(command: &mut Command, stderr: &Path) -> Result<Self> {
        let stdout = tempfile::tempfile()?;
        let child = command
            .stdin(Stdio::null())
            .stdout(stdout.try_clone()?)
            .stderr(File::create(stderr)?)
            .spawn()?;
        Ok(Self {
            child: Some(child),
            command: std::iter::once(command.get_program())
                .chain(command.get_args())
                .map(|part| part.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" "),
            stdout,
            stderr: stderr.to_path_buf(),
        })
    }

    /// Waits for the child to exit, polling so the future yields, and
    /// returns its stdout.
    ///
    /// # Errors
    ///
    /// Returns an error carrying its stderr when it exits unsuccessfully, or
    /// an IO error waiting on it or reading its output.
    ///
    /// # Cancellation
    ///
    /// Dropping the future at a yield drops `self`, whose `Drop` kills and
    /// reaps the child; whatever it already wrote stays written.
    async fn finish(mut self) -> Result<String> {
        loop {
            let child = self.child.as_mut().ok_or("operator command was reaped")?;
            if let Some(status) = child.try_wait()? {
                self.child = None;
                if !status.success() {
                    return Err(format!(
                        "{} exited with {status}: {}",
                        self.command,
                        std::fs::read_to_string(&self.stderr).unwrap_or_default()
                    )
                    .into());
                }
                let mut printed = String::new();
                self.stdout.rewind()?;
                self.stdout.read_to_string(&mut printed)?;
                return Ok(printed);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

impl Drop for OperatorRun {
    /// Kills and reaps a child that [`OperatorRun::finish`] did not see exit,
    /// which only happens when its caller stopped waiting, and prints its
    /// stderr so the stop is diagnosable.
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            eprintln!(
                "{} stopped before it exited; its stderr ({}):\n{}",
                self.command,
                self.stderr.display(),
                std::fs::read_to_string(&self.stderr).unwrap_or_default()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Metrics;

    /// Series of one family sum across label sets; other families and
    /// same-prefix families do not leak in.
    ///
    /// # Panics
    ///
    /// Panics when the sum includes the wrong series.
    #[test]
    fn metrics_sum_selects_family_and_labels() {
        let metrics = Metrics::parse(
            "# HELP x\noracle_queries_queued{class=\"a\"} 3\noracle_queries_queued{class=\"b\"} 4\n\
             oracle_queries_queued_total 100\nslots{kind=\"limit\"} 8\nslots{kind=\"used\"} 2\n",
        );
        assert_eq!(metrics.sum("oracle_queries_queued", &[]), 7.0);
        assert_eq!(metrics.sum("slots", &["kind=\"limit\""]), 8.0);
        assert_eq!(metrics.sum("absent", &[]), 0.0);
    }

    /// Histogram buckets sum across matching series in bound order, `+Inf`
    /// last, and other label sets stay out.
    ///
    /// # Panics
    ///
    /// Panics when a bucket is missing, misordered, or misattributed.
    #[test]
    fn metrics_buckets_merge_matching_series_in_bound_order() {
        let metrics = Metrics::parse(
            "w_bucket{origin=\"a\",le=\"+Inf\"} 5\nw_bucket{origin=\"a\",le=\"0.5\"} 2\n\
             w_bucket{origin=\"b\",le=\"0.5\"} 1\nw_bucket{origin=\"b\",le=\"+Inf\"} 1\n\
             w_bucket{origin=\"c\",le=\"0.5\"} 9\nw_count{origin=\"a\"} 5\n",
        );
        assert_eq!(
            metrics.buckets("w", &["origin=\"a\""]),
            vec![(0.5, 2.0), (f64::INFINITY, 5.0)]
        );
        let ab: Vec<(f64, f64)> = metrics.buckets("w", &[]).into_iter().collect();
        assert_eq!(ab, vec![(0.5, 12.0), (f64::INFINITY, 6.0)]);
    }
}
