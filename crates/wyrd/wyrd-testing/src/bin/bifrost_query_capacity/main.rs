//! Bifrost query capacity benchmark.
//!
//! Reads as the production journey it measures. An operator starts one
//! release `wyrd-server` through the local-development guide (`migrate`,
//! serve, `setup`) on the whole host, Linux or macOS; the report title carries
//! the host fingerprint, so compare only reports from matching hosts. An
//! application then uses the public Rust client exactly as the guide's
//! `WYRD_SERVER_URL` and `WYRD_API_KEY` configure it: it writes the events
//! fixture, checks every answer, and runs each read step. All evidence comes
//! from client timings, `/metrics`, and the OS's readings of the server
//! process.
//!
//! Each process runs one [`Tier`], named by `--tier hot|parquet`: the same
//! read cases with the rows in the memtable or published as Parquet, so the
//! two reports compare the read paths like for like. A tier needs a fresh
//! database, so the mise task runs each tier under its own Postgres.
//!
//! `--writes` runs only the write sweep instead: the standard fixture's rows
//! written again per step at each of [`WRITE_SWEEP`] writers into one table on
//! one Scribe shard, each step profiled, reported in retained Arrow MB/s, MB/s
//! per server core, and rows/s (`mise run bench:bifrost:write-capacity`).
//!
//! Run through `mise run bench:bifrost:query-capacity` (10 million rows, both
//! tiers) or `mise run bench:bifrost:query-capacity -- --heavy` (100 million
//! rows, Parquet only, broad window and full scan only). Every query window
//! of the server is recorded with `perf` (a frame-pointer build) under
//! `<tier>/profiles/`. Each tier's report, raw
//! latencies (`samples.jsonl`), and server log land in
//! `target/bifrost-query-capacity/{standard,heavy}/{hot,parquet}/`; each row
//! prints as it is judged. Exits nonzero when any row fails.

mod report;
mod run;
mod workload;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;
use wyrd_client::config::ClientConfig;
use wyrd_client::{GlobalConfig, WyrdClient};

use report::Report;
use run::Bench;
use workload::{Case, Fixture, TABLE};
use wyrd_testing::capacity::{install_tracing, release_binary};
use wyrd_testing::release_server::{LocalServer, Placement, SERVER_URL};

/// Error type of every benchmark step: the binary only reports it.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Concurrency points of the selective and small-aggregate sweeps.
const SWEEP: [usize; 6] = [1, 4, 8, 16, 32, 64];

/// Writer counts of the `--writes` sweep, one step each.
const WRITE_SWEEP: [usize; 4] = [1, 4, 16, 64];

/// Read clients beside the writer in the reads-while-writing steps.
///
/// Reads and writes share the node's CPUs, so the step runs few enough
/// readers to leave the writer room; at 16 the reads alone saturate the node.
const LOADED_CLIENTS: usize = 4;

/// Seconds a generation or staged member may age in the Parquet tier before
/// it rotates or publishes, as the server's environment takes it; the write's
/// residue publishes within about twice this.
const PARQUET_RETENTION_SECONDS: &str = "30";

/// How long the Parquet tier may take to publish everything after the write:
/// ten retentions, which covers one rotation, one dwell, and the claims'
/// merge and upload with room to spare.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Where the events table's rows are while the read cases run.
///
/// Both tiers run the same release binary, fixture, and cases; only Scribe's
/// operator settings differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tier {
    /// Every row stays in the memtable: rotation sizes and age are raised
    /// past anything the run writes or lasts, so nothing stages or publishes.
    Hot,
    /// Every row is published Parquet: default sizes, so the write rolls
    /// production-sized objects, and a short retention publishes the residue.
    Parquet,
}

impl Tier {
    /// Parses the `--tier` value.
    ///
    /// # Errors
    ///
    /// Returns an error naming the accepted values when `name` is neither.
    fn parse(name: &str) -> Result<Self> {
        match name {
            "hot" => Ok(Self::Hot),
            "parquet" => Ok(Self::Parquet),
            other => Err(format!("--tier must be hot or parquet, got `{other}`").into()),
        }
    }

    /// Lower-case name, used in the report title and output directory.
    const fn name(self) -> &'static str {
        match self {
            Self::Hot => "hot",
            Self::Parquet => "parquet",
        }
    }

    /// Scribe operator settings that put the rows in this tier.
    const fn env(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Hot => &[
                ("WYRD_MAX_FILE_SIZE_IN_MEMORY", "16384"),
                ("WYRD_MAX_FILE_SIZE_ON_DISK", "16384"),
                ("WYRD_MAX_FILE_RETENTION_TIME", "86400"),
            ],
            Self::Parquet => &[("WYRD_MAX_FILE_RETENTION_TIME", PARQUET_RETENTION_SECONDS)],
        }
    }
}

/// Runs one tier of the fixture and returns whether every row passed.
///
/// The heavy fixture runs only Parquet: its 100M rows are far past what a
/// production memtable holds.
///
/// # Errors
///
/// Returns an error for a heavy hot run, or the server, client, or evidence
/// failure that stopped the tier.
async fn benchmark(heavy: bool, writes: bool, tier: Tier) -> Result<bool> {
    let (fixture, mode) = if writes {
        (Fixture::standard(), "writes")
    } else if heavy {
        if tier == Tier::Hot {
            return Err("--heavy runs only --tier parquet".into());
        }
        (Fixture::heavy(), "heavy")
    } else {
        (Fixture::standard(), "standard")
    };
    let output = PathBuf::from("target/bifrost-query-capacity")
        .join(mode)
        .join(tier.name());
    if output.exists() {
        std::fs::remove_dir_all(&output)?;
    }
    std::fs::create_dir_all(&output)?;
    if writes {
        return write_sweep(fixture, tier, &output).await;
    }
    journey(fixture, heavy, tier, &output).await
}

/// The `--writes` journey: start the server, write the fixture's rows once
/// per [`WRITE_SWEEP`] step into disjoint id ranges of one table, stop.
///
/// Returns whether every step passed.
///
/// # Errors
///
/// Returns a server, client, capture, or evidence failure that stopped it.
async fn write_sweep(fixture: Fixture, tier: Tier, output: &Path) -> Result<bool> {
    let server =
        LocalServer::start(&release_binary()?, &["bench"], tier.env(), Placement::Host).await?;
    let client = WyrdClient::with_config(ClientConfig {
        credential: Some(server.api_key().clone()),
        ..ClientConfig::from_global_with_overrides(&GlobalConfig::default(), Some(SERVER_URL), None)
    })?;
    let bench = Bench::new(&server, &client, fixture, &[], output.join("profiles"));
    let label = format!("{} rows per step, {}", tier.name(), server.describe());
    let mut report = Report::new(fixture, label, server.memory_limit_bytes());
    let mut first = 0;
    for writers in WRITE_SWEEP {
        let written = bench
            .write_step(TABLE, first..first + fixture.rows, writers)
            .await?;
        let (files, bytes) = parquet_files(&server.storage_dir());
        report.write(&format!("write x{writers}"), &written, files, bytes);
        first += fixture.rows;
    }
    report.shutdown(&server.stop(&output.join("server.log")).map_err(Into::into));
    report.write_to(output)?;
    Ok(report.passed())
}

/// One tier's journey: start the server, write, put the rows in `tier`,
/// check answers, measure, confirm a hot tier held, stop.
///
/// Returns whether every row passed.
///
/// # Errors
///
/// Returns a server, client, or evidence failure that stopped the journey.
async fn journey(fixture: Fixture, heavy: bool, tier: Tier, output: &Path) -> Result<bool> {
    let cases: &[Case] = if heavy {
        &[Case::BroadWindow, Case::FullScan]
    } else {
        &[
            Case::Selective,
            Case::SmallAggregate,
            Case::MillionAggregate,
            Case::TableAggregate,
            Case::BroadWindow,
            Case::FullScan,
        ]
    };
    let server =
        LocalServer::start(&release_binary()?, &["bench"], tier.env(), Placement::Host).await?;
    let client = WyrdClient::with_config(ClientConfig {
        credential: Some(server.api_key().clone()),
        ..ClientConfig::from_global_with_overrides(&GlobalConfig::default(), Some(SERVER_URL), None)
    })?;
    let bench = Bench::new(&server, &client, fixture, cases, output.join("profiles"));
    let label = format!("{} rows, {}", tier.name(), server.describe());
    let mut report = Report::new(fixture, label, server.memory_limit_bytes());

    let written = bench
        .write(
            TABLE,
            0..fixture.rows,
            run::WRITERS,
            CancellationToken::new(),
        )
        .await?;
    let (files, bytes) = parquet_files(&server.storage_dir());
    report.write("write events", &written, files, bytes);
    report.tier("rows in tier", settle(&server, tier).await?);
    report.answers(&bench.check_answers(cases).await?);

    if !heavy {
        for case in [Case::Selective, Case::SmallAggregate] {
            let mut points = Vec::new();
            for clients in SWEEP {
                points.push(bench.run(case, clients).await?);
            }
            report.sweep(case, &points);
        }
        report.query(
            Case::MillionAggregate,
            &bench.run(Case::MillionAggregate, 8).await?,
        );
        report.query(
            Case::TableAggregate,
            &bench.run(Case::TableAggregate, 1).await?,
        );
    }
    report.query(Case::BroadWindow, &bench.run(Case::BroadWindow, 1).await?);
    report.query(Case::FullScan, &bench.run(Case::FullScan, 1).await?);
    if tier == Tier::Hot {
        report.tier("rows still hot", Residence::hot(&server).await?);
    }

    if !heavy {
        for case in [Case::SmallAggregate, Case::MillionAggregate] {
            let (alone, loaded, written) = bench.reads_while_writing(case, LOADED_CLIENTS).await?;
            report.loaded(case, &alone, &loaded);
            report.write(&format!("write during {}", case.name()), &written, 0, 0);
        }
        report.overload(bench.overload_queue().await?);
    }

    report.shutdown(&server.stop(&output.join("server.log")).map_err(Into::into));
    report.write_to(output)?;
    Ok(report.passed())
}

/// Where Scribe holds rows, from one `/metrics` scrape and the store.
#[derive(Debug)]
struct Residence {
    /// Arrow bytes in active and immutable memtables.
    memtable_bytes: f64,
    /// Durable staged members not yet published.
    staged_members: f64,
    /// Claims taken but not yet published.
    outstanding_claims: f64,
    /// Published Parquet objects in the store.
    published_files: u64,
}

impl Residence {
    /// Reads the server's memtable, staging, and claim gauges and counts the
    /// store's Parquet objects.
    ///
    /// # Errors
    ///
    /// Returns the scrape failure.
    async fn read(server: &LocalServer) -> Result<Self> {
        let metrics = server.metrics().await?;
        Ok(Self {
            memtable_bytes: metrics.sum("bifrost_scribe_active_memtable_bytes", &[])
                + metrics.sum("bifrost_scribe_immutable_memtable_bytes", &[]),
            staged_members: metrics.sum("bifrost_scribe_staging_live_members", &[]),
            outstanding_claims: metrics.sum("bifrost_scribe_staging_outstanding_claims", &[]),
            published_files: parquet_files(&server.storage_dir()).0,
        })
    }

    /// Whether every row is published: nothing in a memtable, staged, or
    /// claimed, and at least one Parquet object in the store.
    fn all_published(&self) -> bool {
        self.memtable_bytes == 0.0
            && self.staged_members == 0.0
            && self.outstanding_claims == 0.0
            && self.published_files > 0
    }

    /// Whether every row is in a memtable: nothing staged, claimed, or
    /// published.
    fn all_hot(&self) -> bool {
        self.staged_members == 0.0 && self.outstanding_claims == 0.0 && self.published_files == 0
    }

    /// Checks that every row is in a memtable.
    ///
    /// # Errors
    ///
    /// Returns the scrape failure; a row that left the memtable is an `Err`
    /// outcome, not an error.
    async fn hot(server: &LocalServer) -> Result<std::result::Result<String, String>> {
        let residence = Self::read(server).await?;
        Ok(if residence.all_hot() {
            Ok(residence.describe())
        } else {
            Err(format!("rows left the memtable: {}", residence.describe()))
        })
    }

    /// One-line summary for a report row.
    fn describe(&self) -> String {
        format!(
            "memtable {:.0} MiB, {} staged, {} claims, {} published files",
            self.memtable_bytes / f64::from(1 << 20),
            self.staged_members,
            self.outstanding_claims,
            self.published_files
        )
    }
}

/// Puts the written rows in `tier` and says where they are.
///
/// Hot is checked once: its settings leave nothing to wait for. Parquet polls
/// every second until everything is published or [`SETTLE_TIMEOUT`] passes.
///
/// # Errors
///
/// Returns a scrape failure; a tier that did not hold is an `Err` outcome.
async fn settle(server: &LocalServer, tier: Tier) -> Result<std::result::Result<String, String>> {
    if tier == Tier::Hot {
        return Residence::hot(server).await;
    }
    let started = Instant::now();
    loop {
        let residence = Residence::read(server).await?;
        if residence.all_published() {
            return Ok(Ok(format!(
                "published in {:.0}s: {}",
                started.elapsed().as_secs_f64(),
                residence.describe()
            )));
        }
        if started.elapsed() >= SETTLE_TIMEOUT {
            return Ok(Err(format!(
                "not all published after {}s: {}",
                SETTLE_TIMEOUT.as_secs(),
                residence.describe()
            )));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Counts the Parquet objects and bytes under the `file://` store.
fn parquet_files(root: &Path) -> (u64, u64) {
    let (mut files, mut bytes) = (0, 0);
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "parquet")
            {
                files += 1;
                bytes += entry.metadata().map_or(0, |metadata| metadata.len());
            }
        }
    }
    (files, bytes)
}

/// Runs the benchmark and exits nonzero on any failed row or error.
#[tokio::main]
async fn main() -> ExitCode {
    install_tracing();
    let arguments: Vec<String> = std::env::args().collect();
    let heavy = arguments.iter().any(|argument| argument == "--heavy");
    let writes = arguments.iter().any(|argument| argument == "--writes");
    let tier = arguments
        .iter()
        .position(|argument| argument == "--tier")
        .and_then(|index| arguments.get(index + 1))
        .map_or(Err("--tier hot|parquet is required".into()), |name| {
            Tier::parse(name)
        });
    let outcome = match tier {
        Ok(tier) => benchmark(heavy, writes, tier).await,
        Err(error) => Err(error),
    };
    match outcome {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("bifrost query capacity benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}
