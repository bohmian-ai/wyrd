//! Opt-in Bifrost OLAP capacity benchmark.
//!
//! Run through `mise run bench:bifrost:query-capacity` on a Linux host with a
//! systemd user manager that delegates the `cpu` and `memory` controllers.
//! Launches the sibling `bifrost_peer_test_node` binary (or
//! `WYRD_BENCH_NODE_BINARY`) as local pods in their own 4-CPU/8-GiB systemd
//! user scopes and runs the standard suite, or with `WYRD_BENCH_HEAVY_SCAN=1`
//! the 100-million-row heavy-scan qualification. Reports land under
//! `WYRD_BENCH_OUTPUT_DIR` (default `target/bifrost-query-capacity`) in
//! `standard/` or `heavy/`. Exits nonzero when any row is not PASS.

use std::process::ExitCode;

use wyrd_testing::load::capacity::{BenchmarkSettings, CapacityError, QueryCapacityBenchmark};

/// Runs the mode the settings select.
///
/// # Errors
///
/// Returns the settings, setup, or requirement failure.
async fn benchmark() -> Result<(), CapacityError> {
    let settings = BenchmarkSettings::from_env()?;
    if settings.heavy {
        QueryCapacityBenchmark::heavy(&settings).await.map(drop)
    } else {
        QueryCapacityBenchmark::standard(&settings).await.map(drop)
    }
}

/// Installs the driver's stderr log subscriber when `RUST_LOG` asks for one.
///
/// The pod child logs its own view under the same variable; this makes the
/// driver's client-side failures, such as each failed query's error, readable
/// alongside it.
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
