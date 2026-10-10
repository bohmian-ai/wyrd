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
//! temporary root; a [`Placement`] that runs the process on the whole
//! [`Host`] (Linux or macOS) or in a systemd scope enforcing an [`Envelope`]
//! (Linux, for multi-replica benchmarks whose replicas each get their own
//! resources); and any extra environment a benchmark names.
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

pub(crate) mod os;

use secrecy::SecretString;
use tempfile::TempDir;

/// The server's default public HTTP address, as the guide exports it in
/// `WYRD_SERVER_URL`.
pub const SERVER_URL: &str = "http://127.0.0.1:8080";

/// The server's default public HTTP port, which replica 0 binds.
const HTTP_PORT: u16 = 8080;

/// The server's default gRPC port, which replica 0 binds.
const GRPC_PORT: u16 = 50051;

/// The server's default Bifrost peer port, which replica 0 binds.
const PEER_PORT: u16 = 50052;

/// Port offset between consecutive replicas' listeners. The metrics
/// listener follows the HTTP port by the server's default of one.
const REPLICA_PORT_STRIDE: u16 = 10;

/// CPUs of quota in the pod envelope.
pub const CPUS: u64 = 8;

/// Memory limit of the pod envelope, with no swap.
pub const MEMORY_BYTES: u64 = 16 << 30;

/// Where one server process runs and what bounds it.
///
/// A single-server benchmark runs on [`Placement::Host`], so it measures
/// whatever machine runs it, Linux or macOS, and labels its report with that
/// [`Host`]. A multi-replica benchmark gives each process a
/// [`Placement::Scoped`] envelope so replicas cannot borrow each other's CPU
/// or memory; that needs a delegating systemd user manager, so Linux only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// A plain child process with the whole host's CPU and memory.
    Host,
    /// A systemd user scope enforcing this envelope.
    Scoped(Envelope),
}

/// The machine a benchmark ran on, which labels its report: results are
/// comparable only between reports whose hosts match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// CPU model name, or `unknown` when the OS reports none.
    pub cpu_model: String,
    /// CPUs this process may run on.
    pub cpus: usize,
    /// Physical memory in bytes.
    pub memory_bytes: u64,
    /// Operating system, as [`std::env::consts::OS`] names it.
    pub os: &'static str,
}

impl Host {
    /// Reads the running machine's CPU model, CPU count, and memory.
    ///
    /// # Errors
    ///
    /// Returns an error when the CPU count or physical memory cannot be read.
    pub fn detect() -> Result<Self> {
        Ok(Self {
            cpu_model: os::cpu_model(),
            cpus: std::thread::available_parallelism()?.get(),
            memory_bytes: os::memory_bytes()?,
            os: std::env::consts::OS,
        })
    }

    /// One-line fingerprint such as `AMD Ryzen 9 9950X, 32 CPU / 61 GiB, linux`.
    pub fn describe(&self) -> String {
        format!(
            "{}, {} CPU / {} GiB, {}",
            self.cpu_model,
            self.cpus,
            self.memory_bytes >> 30,
            self.os
        )
    }
}

/// The CPU quota and memory limit one process's systemd scope enforces.
///
/// The server sizes itself from its cgroup — the CPU quota rounded down to
/// whole CPUs, at least one, and `memory.max` — so a benchmark that gives a
/// process a smaller envelope measures it as a pod of that size would run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Envelope {
    /// CPU quota in percent of one CPU, as systemd's `CPUQuota` takes it:
    /// 150 is one and a half CPUs.
    pub cpu_percent: u64,
    /// Memory limit in bytes, with no swap.
    pub memory_bytes: u64,
}

impl Envelope {
    /// The pod envelope every benchmark runs in unless it names another:
    /// [`CPUS`] CPUs and [`MEMORY_BYTES`].
    pub const POD: Self = Self {
        cpu_percent: CPUS * 100,
        memory_bytes: MEMORY_BYTES,
    };

    /// The CPU quota in CPUs, possibly fractional.
    pub fn cpus(self) -> f64 {
        self.cpu_percent as f64 / 100.0
    }
}

/// How long the server may take to report ready on `/readyz`.
const READY_TIMEOUT: Duration = Duration::from_secs(120);

/// How long the server may take to exit after `SIGTERM`: the kind pods'
/// `terminationGracePeriodSeconds`, 10 s beyond the server's default 35 s
/// shutdown drain budget.
const STOP_GRACE: Duration = Duration::from_secs(45);

/// Why starting, observing, or stopping the release server failed.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseServerError {
    /// A process, file, cgroup, or OS probe operation failed.
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
    root: TempDir,
    /// The serving process until it is reaped.
    child: Option<Child>,
    /// Where the process's CPU and memory are read from.
    meter: Meter,
    /// The machine the process runs on.
    host: Host,
    /// Tenants in the order `setup` provisioned them; empty on a joined
    /// replica.
    tenants: Vec<SetupTenant>,
    /// Replica ordinal: 0 for the server [`LocalServer::start`] set up,
    /// which binds the default ports, and its listener offset otherwise.
    ordinal: u16,
    /// What the process runs, which decides its readiness probe.
    role: Role,
    /// Where the process runs and what bounds it.
    placement: Placement,
}

/// Where a [`LocalServer`]'s CPU and memory are read from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Meter {
    /// The scope's cgroup-v2 directory: the whole scope's usage.
    Cgroup(PathBuf),
    /// The server process itself, read from the OS by PID.
    Process(u32),
}

/// What a [`LocalServer`] process runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// The default target: the public API and every serving subsystem, ready
    /// once `/readyz` answers.
    Serving,
    /// `WYRD_TARGET=forge-worker`: Forge work only, no API and no peer
    /// listener, ready once its metrics listener answers.
    ForgeWorker,
}

impl LocalServer {
    /// Migrates and serves `binary` with `env` added at `placement`, then
    /// runs `setup` once per slug in `tenants`, returning once `/readyz`
    /// answers 200, a scoped cgroup enforces its envelope, and every tenant's
    /// credential was printed.
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
    /// becomes ready, the host cannot be read, or a scoped cgroup does not
    /// enforce its envelope.
    pub async fn start(
        binary: &Path,
        tenants: &[&str],
        env: &[(&str, &str)],
        placement: Placement,
    ) -> Result<Self> {
        let owner_url = std::env::var("WYRD_TEST_DATABASE_ADMIN_URL")
            .map_err(|_| "WYRD_TEST_DATABASE_ADMIN_URL is unset; run through mise")?;
        let root = tempfile::Builder::new().prefix("wyrd-bench-").tempdir()?;
        let storage = root.path().join("storage");
        std::fs::create_dir_all(&storage)?;
        let storage_url = format!("file://{}", storage.display());
        let workdir = root.path().to_path_buf();
        run(
            operator(binary, &workdir, &storage_url, 0, Role::Serving, env)
                .arg("migrate")
                .env("WYRD_DATABASE_URL", owner_url),
        )?;
        let mut server =
            Self::serve(binary, root, &storage_url, 0, Role::Serving, env, placement).await?;

        let operator =
            |program: &Path| operator(program, &workdir, &storage_url, 0, Role::Serving, env);

        let mut platform: Option<String> = None;
        for slug in tenants {
            let mut setup = operator(binary);
            setup.args(["setup", "--tenant", slug]);
            if let Some(credential) = &platform {
                setup.env("WYRD_PLATFORM_CREDENTIAL", credential);
            }
            let printed = run(&mut setup)?;
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
    /// Postgres and store from its own working directory at this server's
    /// placement, with every listener offset by `ordinal` strides. Nothing is
    /// migrated or set up; the replica serves the tenants this server
    /// provisioned.
    ///
    /// Several replicas need peer mode: give every replica, including this
    /// one, `WYRD_PEER_TLS_DIR` in `env`; its peer address is derived here.
    ///
    /// # Errors
    ///
    /// Returns an error when the process exits or never becomes ready, or a
    /// scoped cgroup does not enforce its envelope.
    pub async fn start_replica(
        &self,
        binary: &Path,
        ordinal: u16,
        env: &[(&str, &str)],
    ) -> Result<Self> {
        let root = tempfile::Builder::new().prefix("wyrd-bench-").tempdir()?;
        let storage_url = format!("file://{}", self.storage_dir().display());
        Self::serve(
            binary,
            root,
            &storage_url,
            ordinal,
            Role::Serving,
            env,
            self.placement,
        )
        .await
    }

    /// Starts a dedicated Forge worker of this deployment as replica
    /// `ordinal`: `binary` with `WYRD_TARGET=forge-worker`, the same Postgres
    /// and store, its own working directory, a scope of `envelope`, and its
    /// metrics listener offset by `ordinal` strides.
    ///
    /// A Forge worker serves no API and opens no peer listener, so it is
    /// ready once `/metrics` answers. It pulls compaction from the elected
    /// Forge leader over the peer route, which needs `WYRD_PEER_TLS_DIR` in
    /// `env`; no peer address is derived for it.
    ///
    /// # Errors
    ///
    /// Returns an error when the process exits or never becomes ready, or its
    /// cgroup does not enforce the envelope.
    pub async fn start_forge_worker(
        &self,
        binary: &Path,
        ordinal: u16,
        env: &[(&str, &str)],
        envelope: Envelope,
    ) -> Result<Self> {
        let root = tempfile::Builder::new().prefix("wyrd-bench-").tempdir()?;
        let storage_url = format!("file://{}", self.storage_dir().display());
        Self::serve(
            binary,
            root,
            &storage_url,
            ordinal,
            Role::ForgeWorker,
            env,
            Placement::Scoped(envelope),
        )
        .await
    }

    /// Serves `binary` as replica `ordinal` in `role` from `root` at
    /// `placement` and waits until it is ready.
    ///
    /// On the host, `binary` is spawned directly and metered by PID. Scoped,
    /// it is spawned through `systemd-run --scope`, which execs it in place,
    /// and metered by the scope's cgroup once that is confirmed to enforce
    /// the envelope.
    ///
    /// # Errors
    ///
    /// Returns an error when the host cannot be read, the process cannot
    /// spawn, exits, or never becomes ready, or a scoped cgroup does not
    /// enforce its envelope.
    async fn serve(
        binary: &Path,
        root: TempDir,
        storage_url: &str,
        ordinal: u16,
        role: Role,
        env: &[(&str, &str)],
        placement: Placement,
    ) -> Result<Self> {
        let host = Host::detect()?;
        let log = File::create(root.path().join("server.log"))?;
        let mut command = match placement {
            Placement::Host => operator(binary, root.path(), storage_url, ordinal, role, env),
            Placement::Scoped(envelope) => {
                let mut command = operator(
                    Path::new("systemd-run"),
                    root.path(),
                    storage_url,
                    ordinal,
                    role,
                    env,
                );
                command
                    .args(["--user", "--scope", "--quiet", "--collect"])
                    .arg(format!("--property=CPUQuota={}%", envelope.cpu_percent))
                    .arg(format!("--property=MemoryMax={}", envelope.memory_bytes))
                    .arg("--property=MemorySwapMax=0")
                    .arg("--")
                    .arg(binary);
                command
            }
        };
        let child = command
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()?;
        let mut server = Self {
            meter: Meter::Process(child.id()),
            host,
            child: Some(child),
            root,
            tenants: Vec::new(),
            ordinal,
            role,
            placement,
        };
        server.await_ready().await?;
        if let Placement::Scoped(envelope) = placement {
            server.meter = Meter::Cgroup(server.find_cgroup()?);
            server.confirm_envelope(envelope)?;
        }
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

    /// CPUs this process may use: its envelope's quota when scoped, else the
    /// host's.
    pub fn cpus(&self) -> f64 {
        match self.placement {
            Placement::Host => self.host.cpus as f64,
            Placement::Scoped(envelope) => envelope.cpus(),
        }
    }

    /// Memory this process may use: its envelope's limit when scoped, else
    /// the host's physical memory.
    pub fn memory_limit_bytes(&self) -> u64 {
        match self.placement {
            Placement::Host => self.host.memory_bytes,
            Placement::Scoped(envelope) => envelope.memory_bytes,
        }
    }

    /// One-line label for a report: the host fingerprint, prefixed by the
    /// envelope when the process is scoped.
    pub fn describe(&self) -> String {
        match self.placement {
            Placement::Host => self.host.describe(),
            Placement::Scoped(envelope) => format!(
                "{} CPU / {} GiB scope on {}",
                envelope.cpus(),
                envelope.memory_bytes >> 30,
                self.host.describe()
            ),
        }
    }

    /// The process's log, which [`LocalServer::stop`] later copies out; it
    /// grows while the process runs.
    pub fn log_path(&self) -> PathBuf {
        self.root.path().join("server.log")
    }

    /// PID of the serving `wyrd-server` itself (`systemd-run --scope` execs
    /// it in place); `None` once it was stopped.
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

    /// Microseconds of CPU the server has used so far: the scope's
    /// `cpu.stat` `usage_usec` when scoped, else the process's user plus
    /// system time; zero when unreadable.
    pub fn cpu_usec(&self) -> u64 {
        match &self.meter {
            Meter::Cgroup(cgroup) => cgroup_stat(cgroup, "cpu.stat", "usage_usec"),
            Meter::Process(pid) => os::process_cpu_usec(*pid),
        }
    }

    /// OOM kills the scope has seen so far. Always zero on the host, where an
    /// OOM kill ends the server and so fails the benchmark directly.
    pub fn oom_kills(&self) -> u64 {
        match &self.meter {
            Meter::Cgroup(cgroup) => cgroup_stat(cgroup, "memory.events", "oom_kill"),
            Meter::Process(_) => 0,
        }
    }

    /// Starts a fresh memory-peak measurement window.
    ///
    /// Scoped, writing to the cgroup's `memory.peak` resets the peak seen
    /// through that descriptor. On a Linux host, the process's peak resident
    /// set is reset instead. macOS keeps only a process's lifetime peak, so
    /// there a window's peak is the peak since the server started.
    ///
    /// # Errors
    ///
    /// Returns an error when the peak cannot be reset.
    pub fn memory_peak(&self) -> Result<MemoryPeak> {
        match &self.meter {
            Meter::Cgroup(cgroup) => {
                let mut file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(cgroup.join("memory.peak"))?;
                file.write_all(b"reset\n")?;
                Ok(MemoryPeak::Cgroup(file))
            }
            Meter::Process(pid) => {
                os::reset_peak_memory(*pid)?;
                Ok(MemoryPeak::Process(*pid))
            }
        }
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

    /// Polls the role's readiness endpoint until it answers 200, failing if
    /// the process exits: `/readyz` for a serving replica and `/metrics` for
    /// a Forge worker, which serves nothing else.
    ///
    /// # Errors
    ///
    /// Returns an error when the process exits, carrying the last lines of
    /// its log, or the timeout passes.
    async fn await_ready(&mut self) -> Result<()> {
        let deadline = Instant::now() + READY_TIMEOUT;
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
            let probe = match self.role {
                Role::Serving => format!("{}/readyz", self.url()),
                Role::ForgeWorker => {
                    format!("http://127.0.0.1:{}/metrics", self.port(HTTP_PORT) + 1)
                }
            };
            if reqwest::get(probe)
                .await
                .is_ok_and(|response| response.status().is_success())
            {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err("wyrd-server never reported ready".into())
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

    /// Confirms the scope's `cpu.max` allows exactly `envelope`'s CPU
    /// percent and its `memory.max` equals its bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when the server is not metered by its cgroup, or
    /// either limit differs, which means the systemd user manager does not
    /// delegate the `cpu` and `memory` controllers.
    fn confirm_envelope(&self, envelope: Envelope) -> Result<()> {
        let Meter::Cgroup(cgroup) = &self.meter else {
            return Err("a scoped wyrd-server is not metered by its cgroup".into());
        };
        let read = |file: &str| std::fs::read_to_string(cgroup.join(file)).unwrap_or_default();
        let (cpu, memory) = (read("cpu.max"), read("memory.max"));
        let mut fields = cpu
            .split_whitespace()
            .map(|field| field.parse::<u64>().ok());
        let cpu_percent = match (fields.next().flatten(), fields.next().flatten()) {
            (Some(quota), Some(period)) if period > 0 => quota * 100 / period,
            _ => 0,
        };
        let Envelope {
            cpu_percent: expected_cpu,
            memory_bytes,
        } = envelope;
        if cpu_percent == expected_cpu && memory.trim() == memory_bytes.to_string() {
            return Ok(());
        }
        Err(format!(
            "cgroup {} enforces cpu.max `{}` memory.max `{}`, not {expected_cpu}% CPU and {memory_bytes} bytes",
            cgroup.display(),
            cpu.trim(),
            memory.trim()
        )
        .into())
    }
}

impl Drop for LocalServer {
    /// Kills and reaps the server when [`LocalServer::stop`] was not reached.
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// One memory-peak window, reset when [`LocalServer::memory_peak`] opened it.
pub enum MemoryPeak {
    /// An open cgroup `memory.peak` descriptor.
    Cgroup(File),
    /// The server process, read from the OS by PID.
    Process(u32),
}

impl MemoryPeak {
    /// Peak memory in bytes since the window opened: the scope's peak when
    /// scoped, else the process's peak resident set (lifetime peak on
    /// macOS).
    ///
    /// # Errors
    ///
    /// Returns an error when the peak cannot be read or parsed.
    pub fn read(self) -> Result<u64> {
        match self {
            Self::Cgroup(mut file) => {
                let mut contents = String::new();
                file.rewind()?;
                file.read_to_string(&mut contents)?;
                Ok(contents.trim().parse()?)
            }
            Self::Process(pid) => Ok(os::peak_memory_bytes(pid)?),
        }
    }
}

/// Reads one `key value` line of a flat-keyed cgroup file such as
/// `cpu.stat` or `memory.events`; zero when absent.
fn cgroup_stat(cgroup: &Path, file: &str, key: &str) -> u64 {
    std::fs::read_to_string(cgroup.join(file))
        .unwrap_or_default()
        .lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(' '))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

/// One `/metrics` scrape: every series and its value.
pub struct Metrics(BTreeMap<String, f64>);

impl Metrics {
    /// Parses a Prometheus text exposition, skipping comments.
    fn parse(exposition: &str) -> Self {
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

    /// Every series of `family`, summed by the value of its `label`, so one
    /// scrape lists a labeled counter family without naming its members.
    pub fn by_label(&self, family: &str, label: &str) -> BTreeMap<String, f64> {
        let prefix = format!("{family}{{");
        let key = format!("{label}=\"");
        let mut values = BTreeMap::new();
        for (series, value) in &self.0 {
            let Some(value_of) = series
                .strip_prefix(&prefix)
                .and_then(|rest| rest.split(&key).nth(1))
                .and_then(|tail| tail.split('"').next())
            else {
                continue;
            };
            *values.entry(value_of.to_owned()).or_default() += value;
        }
        values
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

/// A command running `program` as replica `ordinal` in `role` would: in
/// `workdir`, publishing to `storage_url`, at the server's default pool size,
/// on the replica's listeners, with `env` added. Peer mode derives a serving
/// replica's peer listener and advertised address when `env` names
/// `WYRD_PEER_TLS_DIR`; a Forge worker only dials, so it gets neither.
fn operator(
    program: &Path,
    workdir: &Path,
    storage_url: &str,
    ordinal: u16,
    role: Role,
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
                format!("127.0.0.1:{}", replica_port(GRPC_PORT, ordinal)),
            );
    }
    if role == Role::ForgeWorker {
        command.env("WYRD_TARGET", "forge-worker");
    } else if env.iter().any(|(name, _)| *name == "WYRD_PEER_TLS_DIR") {
        let peer = format!("127.0.0.1:{}", replica_port(PEER_PORT, ordinal));
        command
            .env("WYRD_BIFROST_PEER_BIND_ADDR", &peer)
            .env("WYRD_PEER_ADDRESS", &peer);
    }
    command
}

/// Runs an operator subcommand to completion and returns its stdout.
///
/// # Errors
///
/// Returns an error with its stderr when it cannot run or exits
/// unsuccessfully.
fn run(command: &mut Command) -> Result<String> {
    let output = command.output()?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(format!(
        "{command:?} exited with {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    )
    .into())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;

    use super::{Metrics, Role, operator};

    /// A serving replica in peer mode derives its peer listener and address;
    /// a Forge worker given the same TLS directory only dials, so it gets
    /// its target and neither peer setting.
    ///
    /// # Panics
    ///
    /// Panics when a role's environment is wrong.
    #[test]
    fn forge_worker_dials_without_a_peer_listener() {
        let env = [("WYRD_PEER_TLS_DIR", "/peer")];
        let read = |role: Role| {
            let command = operator(
                Path::new("wyrd-server"),
                Path::new("/"),
                "file:///s",
                2,
                role,
                &env,
            );
            let value = |name: &str| {
                command
                    .get_envs()
                    .find(|(key, _)| *key == OsStr::new(name))
                    .and_then(|(_, value)| value)
                    .map(|value| value.to_string_lossy().into_owned())
            };
            (
                value("WYRD_TARGET"),
                value("WYRD_PEER_ADDRESS"),
                value("WYRD_BIFROST_PEER_BIND_ADDR"),
            )
        };
        let peer = Some("127.0.0.1:50072".to_owned());
        assert_eq!(read(Role::Serving), (None, peer.clone(), peer));
        assert_eq!(
            read(Role::ForgeWorker),
            (Some("forge-worker".to_owned()), None, None)
        );
    }

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

    /// A labeled family groups by the label's value; a same-prefix family
    /// stays out.
    ///
    /// # Panics
    ///
    /// Panics when a value is grouped under the wrong label value.
    #[test]
    fn metrics_by_label_groups_one_family() {
        let metrics = Metrics::parse(
            "p_total{event=\"Oracle.cut_read\"} 3\np_total{event=\"FilterExec.output_rows\",x=\"1\"} 4\n\
             p_total{event=\"FilterExec.output_rows\",x=\"2\"} 1\np_total_extra{event=\"z\"} 9\n",
        );
        let grouped = metrics.by_label("p_total", "event");
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped.get("Oracle.cut_read"), Some(&3.0));
        assert_eq!(grouped.get("FilterExec.output_rows"), Some(&5.0));
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
