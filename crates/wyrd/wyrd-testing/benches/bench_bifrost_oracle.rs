//! Oracle read benchmark binary.
//!
//! Parses the shared tiered-runner flag surface and dispatches the resolved
//! workload through its Oracle family runner: single-topology query, distributed
//! weak-scaling, tenant-matrix fairness, or the mixed Scribe/Oracle/Forge
//! diagonal. An ingest workload routed here is rejected before a cluster starts.

use wyrd_testing::bifrost::bench_families::run_family;
use wyrd_testing::bifrost::bench_runner::{RunnerFamily, RunnerInvocation};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let invocation = RunnerInvocation::parse(std::env::args().skip(1))?;
    let report = run_family(
        &invocation,
        &[
            RunnerFamily::Query,
            RunnerFamily::Distributed,
            RunnerFamily::Fairness,
            RunnerFamily::Mixed,
        ],
    )
    .await?;
    println!("{}", report.display());
    Ok(())
}
