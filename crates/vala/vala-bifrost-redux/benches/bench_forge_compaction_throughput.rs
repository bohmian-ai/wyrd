use std::cmp::Reverse;
use std::time::Instant;

use vala_bifrost_redux::forge::ForgeConfig;

struct Candidate {
    sequence: usize,
    size: u64,
}

fn main() {
    const FILE_COUNT: usize = 10_000;
    const ITERATIONS: usize = 100;
    const TARGET_BYTES: u64 = 512 * 1024 * 1024;
    let config = ForgeConfig::default();
    let files = (0..FILE_COUNT)
        .map(|sequence| Candidate {
            sequence,
            size: config.target_bin_bytes / 16,
        })
        .collect::<Vec<_>>();
    let start = Instant::now();
    let mut bins = 0_usize;
    for _ in 0..ITERATIONS {
        let mut sorted = files
            .iter()
            .map(|file| (Reverse(file.sequence), file.size))
            .collect::<Vec<_>>();
        sorted.sort_unstable();
        bins += sorted
            .chunks(256)
            .filter(|chunk| chunk.iter().map(|(_, size)| *size).sum::<u64>() <= TARGET_BYTES)
            .count();
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
