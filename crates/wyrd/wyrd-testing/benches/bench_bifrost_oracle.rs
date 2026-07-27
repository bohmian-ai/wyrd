//! Canonical Oracle benchmark adapter.

use wyrd_bench::{BifrostLane, BifrostScenario};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let scenario = BifrostScenario::from_process(BifrostLane::Oracle)?;
    wyrd_testing::bifrost::bench_oracle::run(scenario).await
}
