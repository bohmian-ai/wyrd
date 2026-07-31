//! Forge owns background maintenance for Bifrost's staged and Iceberg data.
//!
//! A scheduler leases one tenant/table at a time, then runs reconciliation,
//! compaction, snapshot expiry, and orphan garbage collection under the same
//! fencing boundary. The stages use the audit outbox and durable file metadata
//! to recover work after a process or catalog failure.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Duration;

use iceberg::Catalog;

pub(crate) mod binpack;
mod clock;
pub(crate) mod compact;
mod discovery;
pub(crate) mod error;
pub(crate) mod expire;
pub(crate) mod lease;
mod live_reconcile;
mod live_replace;
mod metrics;
pub(crate) mod orphan_gc;
mod path;
pub(crate) mod rewrite;
pub(crate) mod right_size;
mod scheduler;

pub use clock::ForgeClock;
#[cfg(feature = "test-support")]
pub use clock::ForgeClockControl;
pub use compact::{ForgeConfig, ForgeObjectStore, ForgeTickOutcome};
pub use error::ForgeError;
pub use rewrite::ForgeRewriteRuntime;
#[cfg(feature = "test-support")]
pub use rewrite::deterministic_output_path_for_test;

#[cfg(feature = "test-support")]
pub use lease::{ForgeLease, forge_lease_key};
#[cfg(feature = "test-support")]
pub use live_reconcile::LiveReconciliationTestOutcome;
#[cfg(feature = "test-support")]
pub use live_replace::IcebergRewriteDisposition;
#[cfg(feature = "test-support")]
pub use orphan_gc::current_gc_gate_for_test;
#[cfg(feature = "test-support")]
pub use right_size::{IcebergCandidateFile, IcebergRewriteGroup, IcebergTablePlan};

use rewrite::ForgeRewritePipeline;

/// Construction-time dependency graph for one Forge maintenance handle.
pub struct ForgeBuildConfig {
    /// SQL handle used by tenant-scoped durable Forge transitions.
    pub vala: vala_sql::ValaPostgres,
    /// Cross-tenant operator pool used by discovery and table leases.
    pub operator_pool: vala_sql::OperatorPool,
    /// Iceberg catalog used by maintenance operations.
    pub catalog: Arc<dyn Catalog>,
    /// Raw staging operator retained for table-owned producer fixtures.
    pub staging: Arc<opendal::Operator>,
    /// Object-store capability used by rewrites and garbage collection.
    pub object_store: Arc<dyn ForgeObjectStore>,
    /// Shared `DataFusion` runtime provisioned by the hosting process.
    pub rewrite_runtime: ForgeRewriteRuntime,
    /// Bounded advisory Scribe wake-up inbox.
    pub hints: crate::maintenance::StagingFileInbox,
    /// Validated maintenance and rewrite limits.
    pub config: ForgeConfig,
    /// Delay between complete periodic maintenance ticks.
    pub maintenance_interval: Duration,
    /// Concrete wall clock captured once by each Forge work batch.
    pub clock: ForgeClock,
}

/// The single stateful owner for all Forge maintenance workflows.
pub struct Forge {
    /// Immutable dependencies shared by narrow internal maintenance futures.
    core: Arc<ForgeCore>,
    /// Bounded inbox held only while receiving or draining advisory hints.
    hints: tokio::sync::Mutex<crate::maintenance::StagingFileInbox>,
    /// Serializes periodic, hinted, and explicit maintenance execution.
    tick: tokio::sync::Mutex<()>,
    /// Rejects a second directly supervised scheduler loop.
    running: AtomicBool,
    /// Process-local starting offset for complete periodic table passes.
    periodic_cursor: AtomicU64,
    /// Process-local terminal identities that suppress identical unsafe work.
    terminal_work: tokio::sync::Mutex<compact::ForgeTerminalWorkRegistry>,
}

/// Immutable dependency graph shared by one Forge owner.
pub(crate) struct ForgeCore {
    /// Vala SQL handle used by tenant-scoped transitions.
    vala: vala_sql::ValaPostgres,
    /// Operator pool used by discovery and lease operations.
    operator_pool: vala_sql::OperatorPool,
    /// Iceberg catalog used to load tables and commit maintenance actions.
    catalog: Arc<dyn Catalog>,
    /// Raw staging operator retained for the established Forge composition.
    staging: Arc<opendal::Operator>,
    /// Narrow object-store seam used by rewrite and garbage-collection IO.
    object_store: Arc<dyn ForgeObjectStore>,
    /// Streaming and spillable rewrite pipeline.
    rewrite: ForgeRewritePipeline,
    /// Validated maintenance and rewrite limits.
    config: ForgeConfig,
    /// Delay between periodic scheduler ticks.
    maintenance_interval: Duration,
    /// Wall clock shared by periodic and hinted maintenance batches.
    clock: ForgeClock,
    /// Fixed-cardinality operational metric handles registered at construction.
    metrics: metrics::ForgeMetrics,
}

impl Forge {
    /// Construct one Forge owner after validating its complete dependency graph.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when any Forge limit is unsafe,
    /// the maintenance interval is zero, or the runtime and configured spill
    /// ceilings differ.
    pub fn new(build: ForgeBuildConfig) -> Result<Self, ForgeError> {
        if build.maintenance_interval.is_zero() {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge scheduler interval must be positive".to_owned(),
            });
        }
        build.config.validate()?;
        if build.rewrite_runtime.spill_limit_bytes() != build.config.spill_limit_bytes {
            return Err(ForgeError::InvalidConfig {
                detail: "rewrite runtime spill limit must match Forge config".to_owned(),
            });
        }
        let rewrite = ForgeRewritePipeline::new(
            build.rewrite_runtime,
            Arc::clone(&build.staging),
            Arc::clone(&build.catalog),
            Arc::clone(&build.object_store),
            build.config.max_concurrent_reads,
        )?;
        let core = ForgeCore {
            vala: build.vala,
            operator_pool: build.operator_pool,
            catalog: build.catalog,
            staging: build.staging,
            object_store: build.object_store,
            rewrite,
            config: build.config,
            maintenance_interval: build.maintenance_interval,
            clock: build.clock,
            metrics: metrics::ForgeMetrics::new(),
        };
        debug_assert!(core.rewrite.uses_staging(&core.staging));
        Ok(Self {
            core: Arc::new(core),
            hints: tokio::sync::Mutex::new(build.hints),
            tick: tokio::sync::Mutex::new(()),
            running: AtomicBool::new(false),
            periodic_cursor: AtomicU64::new(0),
            terminal_work: tokio::sync::Mutex::new(compact::ForgeTerminalWorkRegistry::default()),
        })
    }

    /// Returns this Forge clock for test-only fixture reconstruction.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn clock_for_test(&self) -> ForgeClock {
        self.core.clock.clone()
    }
}
