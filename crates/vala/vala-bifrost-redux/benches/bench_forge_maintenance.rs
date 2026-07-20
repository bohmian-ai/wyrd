use std::time::{Duration, Instant};

use opendal::raw::Timestamp;
use vala_bifrost_redux::forge::expire::{SnapshotSummary, select_expirable_snapshots};
use vala_bifrost_redux::forge::{ProtectedLiveSet, is_gc_candidate};
use wyrd_bench::{SloGate, SloMeasurement};

fn main() {
    const OBJECT_COUNT: usize = 10_000;
    const SNAPSHOT_COUNT: usize = 256;
    let snapshots = (0..SNAPSHOT_COUNT)
        .map(|index| SnapshotSummary {
            id: i64::try_from(index + 1).expect("snapshot count fits"),
            parent_id: index
                .checked_sub(1)
                .map(|parent| i64::try_from(parent + 1).expect("snapshot count fits")),
            timestamp_ms: i64::try_from(index).expect("snapshot count fits"),
        })
        .collect::<Vec<_>>();
    let mut live = ProtectedLiveSet::default();
    let mut paths = Vec::with_capacity(OBJECT_COUNT);
    for index in 0..OBJECT_COUNT {
        let path = format!("tenants/t/traces/spans/data/{index}.parquet");
        if index % 2 == 0 {
            live.insert(path.clone());
        }
        paths.push(path);
    }
    let now = Timestamp::from_millisecond(48 * 60 * 60 * 1_000).expect("timestamp");
    let old = Timestamp::from_millisecond(0).expect("timestamp");
    let start = Instant::now();
    let mut candidates = 0_usize;
    let mut selected = 0_usize;
    for _ in 0..100 {
        selected += select_expirable_snapshots(&snapshots, Some(256), &[], 200, 8).len();
        candidates += paths
            .iter()
            .filter(|path| is_gc_candidate(path, &live, Some(old), now, Duration::from_hours(24)))
            .count();
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
