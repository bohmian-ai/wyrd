//! Opt-in Bifrost OLAP capacity benchmark.
//!
//! Seeds `vala.datasets.events` through the public write API into one local
//! pod limited to 4 CPUs and 8 GiB by its own systemd user scope, then runs
//! the named read cases — selective and small-aggregate concurrency sweeps,
//! medium and table aggregates, the broad window, the full scan, reads during
//! writes, a full Oracle queue, and a remote-live window — through the public
//! Rust client, and reports every row as PASS, FAIL, or INVALID with its
//! reason. `WYRD_BENCH_HEAVY_SCAN=1` runs the separate 100-million-row scan
//! qualification instead. It runs only through its own `mise` command.
//!
//! [`workload`] holds the fixture, statements, answers, and targets,
//! [`schedule`] the client drivers, and [`run`] the visible sequence and the
//! report over [`crate::bifrost::process_cluster::BifrostProcessCluster`].

pub mod run;
pub mod schedule;
pub mod workload;

pub use run::{BenchmarkSettings, CapacityError, QueryCapacityBenchmark, Report};
pub use schedule::{
    ClosedLoopDriver, FixedRateDriver, FixedRateRun, ProbeResult, ShortQueryOutcome,
    ShortQuerySample,
};
