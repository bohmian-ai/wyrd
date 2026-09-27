//! Opt-in single-pod Bifrost query capacity benchmark.
//!
//! Run through `mise run bench:bifrost:query-capacity` on a Linux host with a
//! systemd user manager that delegates the `cpu` and `memory` controllers.
//! Launches the sibling `bifrost_peer_test_node` binary (or
//! `WYRD_BENCH_NODE_BINARY`) as one local pod in its own 4-CPU/8-GiB systemd
//! user scope, then runs the four offered-rate / held-live-stream rows and
//! writes the report under `WYRD_BENCH_OUTPUT_DIR`.

use std::path::PathBuf;
use std::process::ExitCode;

use wyrd_testing::load::capacity::{BenchmarkSettings, CapacityError, QueryCapacityBenchmark};

/// Runs setup, then every combination, and reports the first fatal failure.
///
/// # Errors
///
/// Returns the settings, setup, or run failure that stopped the benchmark.
async fn benchmark() -> Result<(), CapacityError> {
    let settings = BenchmarkSettings::from_env()?;
    let binary = match std::env::var_os("WYRD_BENCH_NODE_BINARY") {
        Some(binary) => PathBuf::from(binary),
        None => std::env::current_exe()
            .map_err(|error| CapacityError::Output(error.to_string()))?
            .with_file_name("bifrost_peer_test_node"),
    };
    QueryCapacityBenchmark::setup(&binary, settings)
        .await?
        .run()
        .await?;
    Ok(())
}

/// Runs the benchmark on a multi-thread runtime, which the driver needs for
/// its blocking pod control calls, and exits nonzero on failure.
#[tokio::main]
async fn main() -> ExitCode {
    match benchmark().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bifrost query capacity benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}
