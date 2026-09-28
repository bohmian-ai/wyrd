//! Opt-in single-pod Bifrost query capacity benchmark.
//!
//! Measures how many fixed short reads per second one mixed Bifrost pod, a
//! local process limited to 4 CPUs and 8 GiB by its own systemd user scope,
//! answers through the public Rust client, alone and with live streams holding
//! Interactive slot units, with the live fixture on its own Scribe and on a
//! second Scribe pod; then measures the durable public write rate and reads
//! every acknowledged batch back. It runs only through its own `mise` command,
//! and fails when a query row misses its requirement or an acknowledged batch
//! does not read back. The write rate has no approved target and is reported
//! only.
//!
//! [`workload`] holds the fixture and SQL, [`schedule`] the open-loop
//! fixed-rate driver, and [`run`] the setup, warmup, measurement, and drain
//! steps over [`crate::bifrost::process_cluster::BifrostProcessCluster`].

pub mod run;
pub mod schedule;
pub mod workload;

pub use run::{
    BenchmarkSettings, CapacityError, CombinationReport, QueryCapacityBenchmark, ScribePlacement,
    WriteReport,
};
pub use schedule::{
    FixedRateDriver, FixedRateRun, ProbeResult, ShortQueryOutcome, ShortQuerySample,
};
