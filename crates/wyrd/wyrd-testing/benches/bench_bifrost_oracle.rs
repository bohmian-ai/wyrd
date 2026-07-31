//! Canonical Oracle benchmark adapter.

use wyrd_bench::{BifrostLane, BifrostScenario};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let scenario = BifrostScenario::from_process(BifrostLane::Oracle)?;
    if std::env::var_os("WYRD_BIFROST_CALIBRATION").is_some() {
        wyrd_testing::bifrost::bench_oracle::calibrate(scenario).await
    } else {
        wyrd_testing::bifrost::bench_oracle::run(scenario).await
    }
}
