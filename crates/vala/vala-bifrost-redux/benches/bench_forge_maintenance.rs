use std::collections::BTreeSet;
use std::time::Instant;

use vala_bifrost_redux::forge::ForgeConfig;
use wyrd_bench::{SloGate, SloMeasurement};

fn main() {
    const OBJECT_COUNT: usize = 10_000;
    let config = ForgeConfig::default();
    let mut live = BTreeSet::new();
    let mut paths = Vec::with_capacity(OBJECT_COUNT);
    for index in 0..OBJECT_COUNT {
        let path = format!("tenants/t/traces/spans/data/{index}.parquet");
        if index % 2 == 0 {
            live.insert(path.clone());
        }
        paths.push(path);
    }
    let start = Instant::now();
    let mut candidates = 0_usize;
    let mut selected = 0_usize;
    for _ in 0..100 {
        selected += OBJECT_COUNT.saturating_sub(config.retain_last);
        candidates += paths.iter().filter(|path| !live.contains(*path)).count();
    }
    let elapsed = start.elapsed();
    let mib_per_sec = (f64::from(u32::try_from(OBJECT_COUNT).expect("object count fits")) * 100.0
        / (1024.0 * 1024.0))
        / elapsed.as_secs_f64();
    println!("forge_maintenance_mib_per_sec={mib_per_sec:.2}");
    println!("forge_maintenance_selected={selected}");
    println!("forge_maintenance_candidates={candidates}");
    let gate = SloGate::from_toml(include_str!("../../../../benches/thresholds.toml"))
        .expect("Forge maintenance threshold must parse");
    gate.check(&SloMeasurement {
        group: "bifrost.forge_maintenance".to_owned(),
        metric: "mib_per_sec".to_owned(),
        value: mib_per_sec,
    })
    .expect("Forge maintenance SLO regressed");
}
