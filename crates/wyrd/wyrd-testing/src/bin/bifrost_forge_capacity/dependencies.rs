//! Postgres and object-store utilization over one measurement window.
//!
//! A worker ladder that stops scaling may be bounded by a saturated
//! dependency rather than by the leader, so every window records both shared
//! dependencies beside the servers' own CPU. Postgres reports its committed
//! transactions and block traffic from `pg_stat_database` and its sampled
//! active client backends from `pg_stat_activity`. Both Postgres and the
//! RustFS store run as containers in the Docker VM, so each one's CPU is its
//! container cgroup's `cpu.stat usage_usec`, read through `docker exec`.

use std::process::Command;

use serde::Serialize;
use sqlx::PgPool;

use crate::Result;

/// The storage emulators' RustFS container the mise task brings up.
const RUSTFS_CONTAINER: &str = "wyrd-storage-rustfs";

/// Cumulative dependency counters at one instant.
#[derive(Debug, Clone, Copy, Default)]
pub struct DependencyReading {
    /// `xact_commit + xact_rollback` of the benchmark database.
    transactions: i64,
    /// `blks_read` of the benchmark database.
    blocks_read: i64,
    /// `tup_inserted + tup_updated + tup_deleted` of the benchmark database.
    tuples_written: i64,
    /// Postgres container CPU microseconds, when its container was found.
    postgres_cpu_us: Option<u64>,
    /// RustFS container CPU microseconds, when readable.
    rustfs_cpu_us: Option<u64>,
}

/// Dependency utilization over one window.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DependencyUsage {
    /// Postgres transactions per second.
    pub postgres_transactions_per_second: f64,
    /// Postgres blocks read from disk per second.
    pub postgres_blocks_read_per_second: f64,
    /// Postgres tuples written per second.
    pub postgres_tuples_written_per_second: f64,
    /// Mean active Postgres client backends over the window's samples.
    pub postgres_active_backends: f64,
    /// Postgres container CPU cores, when measured.
    pub postgres_cores: Option<f64>,
    /// RustFS container CPU cores, when measured.
    pub rustfs_cores: Option<f64>,
    /// CPUs of the Docker VM both containers share.
    pub docker_vm_cpus: Option<f64>,
}

/// The two shared dependencies the deployment serves from.
pub struct Dependencies {
    /// Owner pool over the benchmark database.
    owner: PgPool,
    /// The Postgres container publishing the benchmark database's port.
    postgres_container: Option<String>,
    /// CPUs the Docker VM reports.
    docker_vm_cpus: Option<f64>,
}

impl Dependencies {
    /// Connects as the database owner the Postgres wrapper exports and finds
    /// the container publishing its port.
    ///
    /// # Errors
    ///
    /// Returns a missing or malformed `WYRD_TEST_DATABASE_ADMIN_URL` or a
    /// connection failure.
    pub async fn connect() -> Result<Self> {
        let url = std::env::var("WYRD_TEST_DATABASE_ADMIN_URL")?;
        let port = url::Url::parse(&url)?.port().unwrap_or(5432);
        let postgres_container = docker(&[
            "ps",
            "--filter",
            &format!("publish={port}"),
            "--format",
            "{{.Names}}",
        ])
        .and_then(|names| names.lines().next().map(str::to_owned));
        let docker_vm_cpus = docker(&["info", "--format", "{{.NCPU}}"])
            .and_then(|cpus| cpus.trim().parse::<f64>().ok());
        Ok(Self {
            owner: PgPool::connect(&url).await?,
            postgres_container,
            docker_vm_cpus,
        })
    }

    /// Reads every cumulative counter now.
    ///
    /// # Errors
    ///
    /// Returns the statistics query failure.
    pub async fn read(&self) -> Result<DependencyReading> {
        let (transactions, blocks_read, tuples_written): (i64, i64, i64) = sqlx::query_as(
            "SELECT (xact_commit + xact_rollback)::bigint, blks_read::bigint, \
                    (tup_inserted + tup_updated + tup_deleted)::bigint \
               FROM pg_stat_database WHERE datname = current_database()",
        )
        .fetch_one(&self.owner)
        .await?;
        Ok(DependencyReading {
            transactions,
            blocks_read,
            tuples_written,
            postgres_cpu_us: self
                .postgres_container
                .as_deref()
                .and_then(container_cpu_us),
            rustfs_cpu_us: container_cpu_us(RUSTFS_CONTAINER),
        })
    }

    /// Active client backends other than this query's own.
    ///
    /// # Errors
    ///
    /// Returns the activity query failure.
    pub async fn active_backends(&self) -> Result<u64> {
        let active: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pg_stat_activity \
              WHERE state = 'active' AND backend_type = 'client backend' \
                AND pid <> pg_backend_pid()",
        )
        .fetch_one(&self.owner)
        .await?;
        Ok(u64::try_from(active)?)
    }

    /// Utilization between two readings `seconds` apart, with the active
    /// backend `samples` taken between them.
    pub fn usage(
        &self,
        before: DependencyReading,
        after: DependencyReading,
        samples: &[u64],
        seconds: f64,
    ) -> DependencyUsage {
        let seconds = seconds.max(f64::EPSILON);
        let rate = |from: i64, to: i64| (to - from) as f64 / seconds;
        let cores = |from: Option<u64>, to: Option<u64>| {
            Some(to?.saturating_sub(from?) as f64 / 1e6 / seconds)
        };
        DependencyUsage {
            postgres_transactions_per_second: rate(before.transactions, after.transactions),
            postgres_blocks_read_per_second: rate(before.blocks_read, after.blocks_read),
            postgres_tuples_written_per_second: rate(before.tuples_written, after.tuples_written),
            postgres_active_backends: if samples.is_empty() {
                0.0
            } else {
                samples.iter().sum::<u64>() as f64 / samples.len() as f64
            },
            postgres_cores: cores(before.postgres_cpu_us, after.postgres_cpu_us),
            rustfs_cores: cores(before.rustfs_cpu_us, after.rustfs_cpu_us),
            docker_vm_cpus: self.docker_vm_cpus,
        }
    }
}

/// `usage_usec` of a container's own cgroup, or `None` when it cannot be
/// read.
fn container_cpu_us(container: &str) -> Option<u64> {
    docker(&["exec", container, "cat", "/sys/fs/cgroup/cpu.stat"])?
        .lines()
        .find_map(|line| line.strip_prefix("usage_usec "))
        .and_then(|value| value.trim().parse().ok())
}

/// Runs `docker` with `args` and returns its stdout, or `None` when it fails.
fn docker(args: &[&str]) -> Option<String> {
    let output = Command::new("docker").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}
