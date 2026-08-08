//! Scribe ingest benchmark binary.
//!
//! Parses the shared tiered-runner flag surface and dispatches the resolved
//! workload through the ingest family runner. Only ingest workloads are served
//! here; a workload that classifies as any Oracle family is rejected before a
//! cluster starts so the Scribe and Oracle lanes stay disjoint.

use wyrd_testing::bifrost::bench_families::run_family;
use wyrd_testing::bifrost::bench_runner::{RunnerFamily, RunnerInvocation};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let invocation = RunnerInvocation::parse(std::env::args().skip(1))?;
    let report = run_family(&invocation, &[RunnerFamily::Ingest]).await?;
    println!("{}", report.display());
    Ok(())
}
