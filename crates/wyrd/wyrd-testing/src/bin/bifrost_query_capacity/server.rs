//! One release `wyrd-server`, started the way the local-development guide
//! (`docs/src/content/docs/self-hosting/local-development.svx`) tells an
//! operator to.
//!
//! The operator steps are: the database owner runs `wyrd-server migrate`;
//! `wyrd-server` serves with the environment the Postgres wrapper exported;
//! `wyrd-server setup --tenant` prints the first tenant's admin credential.
//! The benchmark adds only what the guide leaves to the operator:
//! `WYRD_STORAGE_URL`, which has no default, pointing at a `file://`
//! directory; a working directory so the server's `.wyrd/` state lands in a
//! temporary root; and a systemd scope that gives the process the
//! 4-CPU/8-GiB pod envelope. Every other setting is the server's default.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read as _, Seek as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use secrecy::SecretString;

use crate::Result;

/// The server's default public HTTP address, as the guide exports it in
/// `WYRD_SERVER_URL`.
pub const SERVER_URL: &str = "http://127.0.0.1:8080";

/// The server's default metrics listener: loopback on the HTTP port plus one.
const METRICS_URL: &str = "http://127.0.0.1:8081/metrics";

/// Tenant `setup` creates for the benchmark application.
const TENANT: &str = "bench";

/// CPUs of quota in the pod envelope.
pub const CPUS: u64 = 8;

/// Memory limit of the pod envelope, with no swap.
pub const MEMORY_BYTES: u64 = 16 << 30;

/// How long the server may take to report ready on `/readyz`.
const READY_TIMEOUT: Duration = Duration::from_secs(120);

/// How long the server may take to exit after `SIGTERM`: the kind pods'
/// `terminationGracePeriodSeconds`, 10 s beyond the server's default 35 s
/// shutdown drain budget.
const STOP_GRACE: Duration = Duration::from_secs(45);

/// A running `wyrd-server` and the credential its `setup` printed.
///
/// Dropping it kills the process; [`LocalServer::stop`] is the clean exit.
pub struct LocalServer {
    /// Working directory holding the store, `.wyrd/` state, and the log.
    root: tempfile::TempDir,
    /// The serving process until it is reaped.
    child: Option<Child>,
    /// The process's cgroup-v2 directory, where its envelope is enforced.
    cgroup: PathBuf,
    /// Admin credential of the benchmark tenant.
    api_key: SecretString,
}

impl LocalServer {
    /// Migrates, serves, and sets up `binary`, returning once `/readyz`
    /// answers 200, the cgroup enforces the envelope, and `setup` printed the
    /// tenant's credential.
    ///
    /// # Errors
    ///
    /// Returns an error when `WYRD_TEST_DATABASE_ADMIN_URL` is unset (the
    /// benchmark is not running under the Postgres wrapper), `migrate` or
    /// `setup` fails, the server exits or never becomes ready, or its cgroup
    /// does not enforce the [`CPUS`]/[`MEMORY_BYTES`] envelope.
    pub async fn start(binary: &Path) -> Result<Self> {
        let owner_url = std::env::var("WYRD_TEST_DATABASE_ADMIN_URL")
            .map_err(|_| "WYRD_TEST_DATABASE_ADMIN_URL is unset; run through mise")?;
        let root = tempfile::Builder::new().prefix("wyrd-bench-").tempdir()?;
        let storage = root.path().join("storage");
        std::fs::create_dir_all(&storage)?;
        let storage_url = format!("file://{}", storage.display());
        let workdir = root.path().to_path_buf();
        let operator = |program: &Path| {
            let mut command = Command::new(program);
            command
                .current_dir(&workdir)
                .env("WYRD_STORAGE_URL", &storage_url);
            command
        };

        run(operator(binary)
            .arg("migrate")
            .env("WYRD_DATABASE_URL", owner_url))?;

        let log = File::create(root.path().join("server.log"))?;
        let child = operator(Path::new("systemd-run"))
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
            api_key: SecretString::from(String::new()),
        };
        server.await_ready().await?;
        server.cgroup = server.find_cgroup()?;
        server.confirm_envelope()?;

        let setup = run(operator(binary).args(["setup", "--tenant", TENANT]))?;
        let key = setup
            .lines()
            .find_map(|line| line.strip_prefix("admin_credential: "))
            .ok_or("`wyrd-server setup` printed no admin_credential")?;
        server.api_key = SecretString::from(key.trim().to_owned());
        Ok(server)
    }

    /// The admin credential `setup` printed, as the guide exports it in
    /// `WYRD_API_KEY`.
    pub fn api_key(&self) -> &SecretString {
        &self.api_key
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
        let response = reqwest::get(METRICS_URL).await?.error_for_status()?;
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
    /// Returns an error when the process exits or the timeout passes.
    async fn await_ready(&mut self) -> Result<()> {
        let deadline = Instant::now() + READY_TIMEOUT;
        while Instant::now() < deadline {
            if let Some(child) = self.child.as_mut()
                && let Some(status) = child.try_wait()?
            {
                return Err(format!("wyrd-server exited during boot with {status}").into());
            }
            if reqwest::get(format!("{SERVER_URL}/readyz"))
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
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
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
}
