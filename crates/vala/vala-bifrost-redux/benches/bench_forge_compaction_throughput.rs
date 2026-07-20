use std::time::Instant;

use chrono::DateTime;
use uuid::Uuid;
use vala_bifrost_redux::forge::binpack::{CandidateFile, stable_pack};

fn main() {
    const FILE_COUNT: usize = 10_000;
    const ITERATIONS: usize = 100;
    const TARGET_BYTES: u64 = 512 * 1024 * 1024;
    let files = (0..FILE_COUNT)
        .map(|index| CandidateFile {
            id: Uuid::from_u128(u128::try_from(index + 1).expect("index fits in UUID")),
            path: format!("staging/{index}.parquet"),
            size: 32 * 1024 * 1024,
            min_event_time: DateTime::from_timestamp(i64::try_from(index).expect("index fits"), 0)
                .expect("timestamp"),
            max_event_time: DateTime::from_timestamp(
                i64::try_from(index + 1).expect("index fits"),
                0,
            )
            .expect("timestamp"),
        })
        .collect::<Vec<_>>();
    let start = Instant::now();
    let mut bins = 0_usize;
    for _ in 0..ITERATIONS {
        bins += stable_pack(files.clone(), TARGET_BYTES, 256).len();
    }
    let elapsed = start.elapsed();
    let input_mib =
        (f64::from(u32::try_from(FILE_COUNT).expect("file count fits in u32")) * 32.0) / 1024.0;
    let mib_per_sec = input_mib
        * f64::from(u32::try_from(ITERATIONS).expect("iteration count fits in u32"))
        / elapsed.as_secs_f64();
    println!("forge_compaction_throughput_mib_per_sec={mib_per_sec:.2}");
    println!("forge_compaction_bins={bins}");
    assert!(
        mib_per_sec > 100.0,
        "Forge bin packing throughput regressed: {mib_per_sec:.2} MiB/s"
    );
}
