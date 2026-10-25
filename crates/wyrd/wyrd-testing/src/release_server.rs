//! One release `wyrd-server`, started the way the local-development guide
//! (`docs/src/content/docs/self-hosting/local-development.svx`) tells an
//! operator to, for the opt-in benchmark binaries.
//!
//! The operator steps are: `wyrd-server migrate` runs as the platform login;
//! `wyrd-server` serves with the environment the Postgres wrapper exported;
//! `wyrd-server setup --tenant` prints each tenant's admin credential, the
//! first run also disclosing the platform credential the later runs present.
//! The benchmarks add only what the guide leaves to the operator:
//! `WYRD_STORAGE_URL`, which has no default, pointing at a `file://`
//! directory; a working directory so the server's `.wyrd/` state lands in a
//! temporary root; and any extra environment a benchmark names. The server
//! is a plain child process with the whole [`Host`]'s CPU and memory on every
//! supported OS: no OS scope, quota, or limit bounds it, and its CPU and
//! memory are read through `sysinfo` by PID.
//! The wrapper's test-fixture `WYRD_DB_MAX_CONNECTIONS` and
//! `WYRD_DB_MAX_CONNECTIONS_VALA` caps are removed, so
//! the pools run at the server's defaults. Every other setting is the server's
//! default.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read as _, Seek as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub(crate) mod os;

pub use os::{HostLoad, PEAK_SAMPLE_INTERVAL_MS, PeakSampler};

use secrecy::SecretString;
use tempfile::TempDir;

/// The server's default public HTTP address, as the guide exports it in
/// `WYRD_SERVER_URL`.
pub const SERVER_URL: &str = "http://127.0.0.1:8080";

/// The server's default metrics listener address, one port above HTTP.
const METRICS_PORT: u16 = 8081;

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
    /// Operating system version, or `unknown` when the OS reports none.
    pub os_version: String,
    /// CPU architecture, as [`std::env::consts::ARCH`] names it.
    pub arch: &'static str,
}

impl Host {
    /// Reads the running machine's CPU model, CPU count, memory, OS, and
    /// architecture.
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
            os_version: os::os_version(),
            arch: std::env::consts::ARCH,
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

/// How long the server may take to report ready on `/readyz`.
const READY_TIMEOUT: Duration = Duration::from_mins(2);

/// How long the server may take to exit after `SIGTERM`: the kind pods'
/// `terminationGracePeriodSeconds`, 10 s beyond the server's default 35 s
/// shutdown drain budget. A benchmark that bounds its own lifetime reserves
/// this much per replica for [`LocalServer::stop`].
pub const STOP_GRACE: Duration = Duration::from_secs(45);

/// Why starting, observing, or stopping the release server failed.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseServerError {
    /// A process, file, or OS probe operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A `/readyz` or `/metrics` request failed.
    #[error(transparent)]
    Http(#[from] reqwest::Error),
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
    /// The machine the process runs on.
    host: Host,
    /// Tenants in the order `setup` provisioned them.
    tenants: Vec<SetupTenant>,
}

impl LocalServer {
    /// Migrates `binary` with the database URLs the Postgres wrapper exported
    /// and serves it as a plain child with `env` added, then runs `setup` once
    /// per slug in `tenants`, returning once `/readyz` answers 200 and every
    /// tenant's credential was printed.
    ///
    /// The first `setup` initializes the platform root and discloses its
    /// credential; later runs present it as `WYRD_PLATFORM_CREDENTIAL`, as
    /// the guide tells an operator adding a tenant to do.
    ///
    /// # Errors
    ///
    /// Returns an error when `migrate` or a
    /// `setup` fails or prints no credential, the server exits or never
    /// becomes ready, or the host cannot be read.
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
        let root = tempfile::Builder::new().prefix("wyrd-bench-").tempdir()?;
        let storage = root.path().join("storage");
        std::fs::create_dir_all(&storage)?;
        let storage_url = format!("file://{}", storage.display());
        let workdir = root.path().to_path_buf();
        OperatorRun::spawn(
            operator(binary, &workdir, &storage_url, env).arg("migrate"),
            &workdir.join("migrate.stderr"),
        )?
        .finish()
        .await?;
        let mut server = Self::serve(binary, root, &storage_url, env).await?;

        let operator = |program: &Path| operator(program, &workdir, &storage_url, env);

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

    /// Serves `binary` from `root` as a plain child and waits until it is
    /// ready.
    ///
    /// The process-wide rustls provider is installed first, because the
    /// readiness and metrics probes build reqwest clients, which carry no
    /// provider of their own.
    ///
    /// # Errors
    ///
    /// Returns an error when the rustls provider cannot be installed, the
    /// host cannot be read, or the process cannot spawn, exits, or never
    /// becomes ready.
    async fn serve(
        binary: &Path,
        root: TempDir,
        storage_url: &str,
        env: &[(&str, &str)],
    ) -> Result<Self> {
        wyrd_tls::install_crypto_provider().map_err(|error| error.to_string())?;
        let host = Host::detect()?;
        let log = File::create(root.path().join("server.log"))?;
        let child = operator(binary, root.path(), storage_url, env)
            .env("WYRD_METRICS_BENCHMARK_SNAPSHOT", "true")
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()?;
        let mut server = Self {
            host,
            child: Some(child),
            root,
            tenants: Vec::new(),
        };
        server.await_ready().await?;
        Ok(server)
    }

    /// The machine the server runs on.
    pub fn host(&self) -> &Host {
        &self.host
    }

    /// The process's log, which [`LocalServer::stop`] later copies out; it
    /// grows while the process runs.
    pub fn log_path(&self) -> PathBuf {
        self.root.path().join("server.log")
    }

    /// PID of the serving `wyrd-server`; `None` once it was stopped.
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
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
        let url = format!("http://127.0.0.1:{METRICS_PORT}/metrics");
        let response = reqwest::get(url).await?.error_for_status()?;
        Ok(Metrics::parse(&response.text().await?))
    }

    /// Reads the server's cumulative benchmark timing snapshot from the
    /// loopback metrics listener, which [`Self::serve`] enables.
    ///
    /// # Errors
    ///
    /// Returns an error when the endpoint does not answer 200 with a
    /// snapshot.
    pub async fn timings(&self) -> Result<wyrd_telemetry::timing::TimingSnapshot> {
        let url = format!("http://127.0.0.1:{METRICS_PORT}/benchmark/timings");
        Ok(reqwest::get(url).await?.error_for_status()?.json().await?)
    }

    /// Microseconds of user plus system CPU the server process has used so
    /// far, or `None` once it is gone or unreadable, so a lost process never
    /// reads as idle.
    pub fn cpu_usec(&self) -> Option<u64> {
        self.pid().and_then(os::process_cpu_usec)
    }

    /// Starts sampling the server's resident memory for one window, or
    /// `None` once it was stopped.
    pub fn memory_peak(&self) -> Option<PeakSampler> {
        self.pid().map(PeakSampler::start)
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
            if let Ok(response) = reqwest::get(format!("{SERVER_URL}/readyz")).await {
                if response.status().is_success() {
                    return Ok(());
                }
                last = response.text().await.unwrap_or_default();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(format!("wyrd-server never reported ready; last probe body: {last}").into())
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
                "wyrd-server stopped abnormally; its log is kept at {}",
                self.root.path().join("server.log").display()
            );
        }
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

/// A command running `program` as the operator would: in `workdir`,
/// publishing to `storage_url`, at the server's default pool size, with `env`
/// added.
fn operator(program: &Path, workdir: &Path, storage_url: &str, env: &[(&str, &str)]) -> Command {
    let mut command = Command::new(program);
    command
        .current_dir(workdir)
        .env("WYRD_STORAGE_URL", storage_url)
        .env_remove("WYRD_DB_MAX_CONNECTIONS")
        .env_remove("WYRD_DB_MAX_CONNECTIONS_VALA")
        .envs(env.iter().copied());
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
