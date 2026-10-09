//! `mise run bench:bifrost:reads`: how fast Bifrost answers a read, as three
//! like-for-like comparisons. Each section stores one set of data, runs the
//! same SQL on both sides, asserts both sides return identical rows, and only
//! then times them (see `report` for how).
//!
//! 1. `hot`: native `DataFusion` vs Wyrd over the same hot Parquet file. Native
//!    is `register_parquet`; Wyrd is the production Oracle provider reading the
//!    file as unpublished data. Measures Wyrd's overhead on recent data.
//! 2. `published`: native `DataFusion` vs Wyrd over the same Iceberg snapshot.
//!    Native is iceberg-datafusion's `IcebergStaticTableProvider`; Wyrd is the
//!    production Oracle provider reading the snapshot through the published
//!    path. Measures the speed of published data.
//! 3. `variant`: the same Variant objects written unshredded and shredded by
//!    Bifrost's own writer, each read by plain `DataFusion`, the Wyrd hot read
//!    and the Wyrd published read, end to end and planning only, with the
//!    cases of `DataFusion`'s `parquet_variant_query` benchmark (see
//!    `variant`).
//!
//! Both sides of every pair use the Oracle session configuration
//! (`OracleSessionShape`): same target partitions, same Parquet options. The
//! only difference is the table provider and Wyrd's installed rules and
//! functions. `-- hot`, `-- published` or `-- variant` runs one section.
//!
//! [`GLOBAL`] installs the server's allocator.

mod events;
mod report;
mod variant;

use std::fmt::Write as _;

use vala_bifrost_redux::bench_support::OracleBenchTables;

/// mimalloc, the allocator `wyrd-server` installs, so allocation costs match
/// production.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Builds every fixture and runs each requested section, printing one table
/// per section.
fn main() {
    // Cargo passes `--bench`; a bare word names a section.
    let sections: Vec<String> = std::env::args()
        .skip(1)
        .filter(|arg| !arg.starts_with('-'))
        .collect();
    let wants = |name: &str| sections.is_empty() || sections.iter().any(|arg| arg == name);
    let runtime = tokio::runtime::Runtime::new().expect("Tokio runtime");
    let dir = tempfile::tempdir().expect("fixture directory");
    let tables = OracleBenchTables::new(dir.path());
    let mut reports = Vec::new();
    if wants("hot") || wants("published") {
        let events = events::write(&tables, dir.path());
        if wants("hot") {
            reports.push(runtime.block_on(events.hot(&tables)));
        }
        if wants("published") {
            reports.push(runtime.block_on(events.published(&tables)));
        }
    }
    let variant = wants("variant").then(|| runtime.block_on(variant::run(&tables, dir.path())));
    for report in &reports {
        report.print();
    }
    if let Some(variant) = variant {
        variant.print();
    }
}

/// `bytes` of xorshift hex seeded by `seed` and `row`, so every value differs
/// and nothing collapses to a dictionary.
fn text(seed: &str, row: usize, bytes: usize) -> String {
    let mut state = seed
        .bytes()
        .fold(row as u64 ^ 0x9E37_79B9_7F4A_7C15, |hash, byte| {
            hash.rotate_left(5) ^ u64::from(byte)
        })
        | 1;
    let mut value = String::with_capacity(bytes);
    while value.len() < bytes {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        write!(value, "{state:016x}").expect("writing to a String cannot fail");
    }
    value
}
