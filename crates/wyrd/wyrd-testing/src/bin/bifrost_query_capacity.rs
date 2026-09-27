//! Opt-in single-pod Bifrost query capacity benchmark.
//!
//! Run through `mise run bench:bifrost:query-capacity` on a dedicated Linux
//! runner with a container runtime; never on a shared workstation. Launches the
//! sibling `bifrost_peer_test_node` binary (or `WYRD_BENCH_NODE_BINARY`) as one
//! containerized pod, then runs every offered-rate / live-occupancy row and
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
