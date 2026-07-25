//! Canonical Scribe benchmark adapter.

use wyrd_bench::{BifrostLane, BifrostScenario};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let scenario = BifrostScenario::from_process(BifrostLane::Scribe)?;
    wyrd_testing::bifrost::bench_scribe::run(scenario).await
}
