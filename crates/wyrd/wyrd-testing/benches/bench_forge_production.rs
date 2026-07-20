//! Actual public Forge-stage benchmark over a real Postgres/Iceberg fixture.

use std::time::Instant;

use vala_bifrost_redux::forge::run_maintenance_tick;
use wyrd_bench::{SloGate, SloMeasurement};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::seed_forge_group;

#[tokio::main]
async fn main() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(std::time::Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge benchmark server");
    let fixture = seed_forge_group(&server, "forge_benchmark_rows").await;
    let start = Instant::now();
    let mut completed = 0_usize;
    for sequence in 0..8_i64 {
        fixture.append_forge_file(sequence * 2).await;
        fixture.append_forge_file(sequence * 2 + 1).await;
        run_maintenance_tick(&fixture.context)
            .await
            .expect("production Forge benchmark tick");
        completed += 1;
    }
    let elapsed = start.elapsed();
    let ticks_per_sec = completed as f64 / elapsed.as_secs_f64();
    println!("forge_production_ticks_per_sec={ticks_per_sec:.2}");
    println!("forge_production_ticks={completed}");
    SloGate::from_toml(include_str!("../../../../benches/thresholds.toml"))
        .expect("Forge production threshold must parse")
        .check(&SloMeasurement {
            group: "bifrost.forge_production".to_owned(),
            metric: "ticks_per_sec".to_owned(),
            value: ticks_per_sec,
        })
        .expect("Forge production SLO regressed");
    server.shutdown().await.expect("benchmark server shutdown");
}
