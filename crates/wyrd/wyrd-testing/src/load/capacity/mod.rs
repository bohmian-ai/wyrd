//! Opt-in single-pod Bifrost query capacity benchmark.
//!
//! Measures how many fixed short reads per second one mixed Bifrost pod,
//! limited to 4 CPUs and 8 GiB, answers through the public Rust client, alone
//! and with live streams holding Interactive slot units. It is a performance
//! benchmark, not a correctness journey: it runs only through its own `mise`
//! command on a dedicated runner and asserts no threshold.
//!
//! [`workload`] holds the fixture and SQL, [`schedule`] the open-loop
//! fixed-rate driver, and [`run`] the setup, warmup, measurement, and drain
//! steps over [`crate::bifrost::process_cluster::BifrostProcessCluster`].

pub mod run;
pub mod schedule;
pub mod workload;

pub use run::{BenchmarkSettings, CapacityError, CombinationReport, QueryCapacityBenchmark};
pub use schedule::{
    FixedRateDriver, FixedRateRun, ProbeResult, ShortQueryOutcome, ShortQuerySample,
};
