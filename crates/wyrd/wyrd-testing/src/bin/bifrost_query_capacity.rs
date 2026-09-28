//! Opt-in single-pod Bifrost query capacity benchmark.
//!
//! Run through `mise run bench:bifrost:query-capacity` on a Linux host with a
//! systemd user manager that delegates the `cpu` and `memory` controllers.
//! Launches the sibling `bifrost_peer_test_node` binary (or
//! `WYRD_BENCH_NODE_BINARY`) as one local pod in its own 4-CPU/8-GiB systemd
//! user scope, runs the four offered-rate / held-live-stream rows and the
//! write measurement, then relaunches with the live fixture on a second
//! Scribe pod for the remote rows. Reports land under `WYRD_BENCH_OUTPUT_DIR`,
//! the remote run's under its `remote-scribe` directory. Exits nonzero when
//! any required row or read-back fails.

use std::path::PathBuf;
use std::process::ExitCode;

use wyrd_testing::load::capacity::{
    BenchmarkSettings, CapacityError, QueryCapacityBenchmark, ScribePlacement,
};

/// Runs setup and every combination for one Scribe placement.
///
/// # Errors
///
/// Returns the setup or run failure, including an unmet requirement.
async fn placement(
    binary: &std::path::Path,
    settings: BenchmarkSettings,
    placement: ScribePlacement,
) -> Result<(), CapacityError> {
    QueryCapacityBenchmark::setup(binary, settings, placement)
        .await?
        .run()
        .await
        .map(drop)
}

/// Runs the colocated placement, then the remote-Scribe placement even when
/// the first failed, and reports every failure.
///
/// # Errors
///
/// Returns the settings failure, or every placement's failure joined.
async fn benchmark() -> Result<(), CapacityError> {
    let settings = BenchmarkSettings::from_env()?;
    let binary = match std::env::var_os("WYRD_BENCH_NODE_BINARY") {
        Some(binary) => PathBuf::from(binary),
        None => std::env::current_exe()
            .map_err(|error| CapacityError::Output(error.to_string()))?
            .with_file_name("bifrost_peer_test_node"),
    };
    let remote = BenchmarkSettings {
        output: settings.output.join("remote-scribe"),
        ..settings.clone()
    };
    let failures = [
        placement(&binary, settings, ScribePlacement::Colocated).await,
        placement(&binary, remote, ScribePlacement::Remote).await,
    ]
    .into_iter()
    .filter_map(Result::err)
    .map(|error| error.to_string())
    .collect::<Vec<_>>();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(CapacityError::Requirement(failures.join("; ")))
    }
}

/// Installs the driver's stderr log subscriber when `RUST_LOG` asks for one.
///
/// The pod child logs its own view under the same variable; this makes the
/// driver's client-side failures, such as each failed short query's error,
/// readable alongside it.
fn install_tracing() {
    let Ok(filter) = std::env::var("RUST_LOG") else {
        return;
    };
    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
            .with_writer(std::io::stderr)
            .finish(),
    );
}

/// Runs the benchmark on a multi-thread runtime, which the driver needs for
/// its blocking pod control calls, and exits nonzero on failure.
#[tokio::main]
async fn main() -> ExitCode {
    install_tracing();
    match benchmark().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bifrost query capacity benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}
