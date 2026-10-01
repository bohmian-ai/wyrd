//! Bifrost query capacity benchmark.
//!
//! Reads as the production journey it measures. An operator starts one
//! release `wyrd-server` through the local-development guide (`migrate`,
//! serve, `setup`) in a 4-CPU/8-GiB systemd scope. An application then uses
//! the public Rust client exactly as the guide's `WYRD_SERVER_URL` and
//! `WYRD_API_KEY` configure it: it writes the events fixture, checks every
//! answer, and runs each read step. All evidence comes from client timings,
//! `/metrics`, and the server's cgroup.
//!
//! Run through `mise run bench:bifrost:query-capacity` (10 million rows) or
//! `mise run bench:bifrost:query-capacity -- --heavy` (100 million rows,
//! broad window and full scan only). The report, raw latencies (`samples.jsonl`), and server
//! log land in `target/bifrost-query-capacity/{standard,heavy}/`; each row
//! prints as it is judged. Exits nonzero when any row fails.

mod report;
mod run;
mod server;
mod workload;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tokio_util::sync::CancellationToken;
use wyrd_client::config::ClientConfig;
use wyrd_client::{GlobalConfig, WyrdClient};

use report::Report;
use run::Bench;
use server::{LocalServer, SERVER_URL};
use workload::{Case, Fixture, TABLE};

/// Error type of every benchmark step: the binary only reports it.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Concurrency points of the selective and small-aggregate sweeps.
const SWEEP: [usize; 6] = [1, 4, 8, 16, 32, 64];

/// Read clients beside the writer in the reads-while-writing steps.
///
/// Reads and writes share the node's 4 CPUs, so the step runs few enough
/// readers to leave the writer room; at 8 the reads alone saturate the node.
const LOADED_CLIENTS: usize = 2;

/// The journey: start the server, write, check answers, measure, stop.
///
/// Returns whether every row passed.
///
/// # Errors
///
/// Returns a server, client, or evidence failure that stopped the journey.
async fn benchmark(heavy: bool) -> Result<bool> {
    let (fixture, mode) = if heavy {
        (Fixture::heavy(), "heavy")
    } else {
        (Fixture::standard(), "standard")
    };
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
    let output = PathBuf::from("target/bifrost-query-capacity").join(mode);
    if output.exists() {
        std::fs::remove_dir_all(&output)?;
    }
    std::fs::create_dir_all(&output)?;

    let server = LocalServer::start(&release_binary()?).await?;
    let client = WyrdClient::with_config(ClientConfig {
        credential: Some(server.api_key().clone()),
        ..ClientConfig::from_global_with_overrides(&GlobalConfig::default(), Some(SERVER_URL), None)
    })?;
    let bench = Bench::new(&server, &client, fixture, cases);
    let mut report = Report::new(fixture);

    let written = bench
        .write(TABLE, fixture.rows, CancellationToken::new())
        .await?;
    let (files, bytes) = parquet_files(&server.storage_dir());
    report.write("write events", &written, files, bytes);
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

    if !heavy {
        for case in [Case::SmallAggregate, Case::MillionAggregate] {
            let (alone, loaded, written) = bench.reads_while_writing(case, LOADED_CLIENTS).await?;
            report.loaded(case, &alone, &loaded);
            report.write(&format!("write during {}", case.name()), &written, 0, 0);
        }
        report.overload(bench.overload_queue().await?);
    }

    report.shutdown(&server.stop(&output.join("server.log")));
    report.write_to(&output)?;
    Ok(report.passed())
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

/// Installs a stderr log subscriber when `WYRD_LOG`, else `RUST_LOG`, is set,
/// so client-side failures read alongside the server log.
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

/// Runs the benchmark and exits nonzero on any failed row or error.
#[tokio::main]
async fn main() -> ExitCode {
    install_tracing();
    let heavy = std::env::args().any(|argument| argument == "--heavy");
    match benchmark(heavy).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("bifrost query capacity benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}
