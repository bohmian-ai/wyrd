//! Forge owns background maintenance for Bifrost's staged and Iceberg data.
//!
//! A scheduler leases one tenant/table at a time, then runs reconciliation,
//! compaction, snapshot expiry, and orphan garbage collection under the same
//! fencing boundary. The stages use the audit outbox and durable file metadata
//! to recover work after a process or catalog failure.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use iceberg::Catalog;

mod clock;
pub(crate) mod compact;
pub(crate) mod error;
pub(crate) mod expire;
mod identity;
pub(crate) mod lease;
mod live_reconcile;
mod live_replace;
mod maintenance;
mod metrics;
pub(crate) mod orphan_gc;
mod path;
mod planner;
mod planning_scheduler;
pub(crate) mod rewrite;
mod scheduler;
mod worker;

pub use clock::ForgeClock;
#[cfg(feature = "test-support")]
pub use clock::ForgeClockControl;
pub use compact::{ForgeConfig, ForgeObjectPages, ForgeObjectStore, ForgeTickOutcome};
pub use error::ForgeError;
pub use metrics::ForgeTelemetry;
pub use planner::{
    ForgeCapacity, ForgeEnvelopeSizer, ForgePlanCandidate, ForgePlanCapacity, ForgePlanner,
    PlannedForgeTask,
};
pub use planning_scheduler::{ForgeScheduleOutcome, ForgeScheduler};
pub use rewrite::ForgeRewriteRuntime;
pub use scheduler::ForgeSchedulerTrigger;
pub use worker::{
    ForgeLifecycleEvent, ForgeWorker, ForgeWorkerCompletionObserver, ForgeWorkerConfig,
};

#[cfg(feature = "test-support")]
pub use lease::{ForgeLease, forge_lease_key};
#[cfg(feature = "test-support")]
pub use live_reconcile::LiveReconciliationTestOutcome;
#[cfg(feature = "test-support")]
pub use maintenance::MaintenanceTestControls;
#[cfg(feature = "test-support")]
pub use orphan_gc::{OrphanGcReport, current_gc_gate_for_test};

/// Construction-time dependency graph for one Forge maintenance handle.
pub struct ForgeBuildConfig {
    /// Narrow Forge capability issued by the one production composition.
    pub resources: crate::resources::ForgeResources,
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
    /// Pod-local base beneath which each leased rewrite owns scratch.
    pub rewrite_spill_root: std::path::PathBuf,
    /// Bounded advisory Scribe wake-up inbox.
    pub hints: crate::maintenance::StagingFileInbox,
    /// Validated maintenance and rewrite limits.
    pub config: ForgeConfig,
    /// Delay between complete periodic maintenance ticks.
    pub maintenance_interval: Duration,
    /// Concrete wall clock captured once by each Forge work batch.
    pub clock: ForgeClock,
    /// Optional test-only observer of successful supervised task completion.
    pub completion_observer: Option<ForgeWorkerCompletionObserver>,
    /// Optional test-only trigger that wakes this owner’s supervised scheduler loop.
    pub scheduler_trigger: Option<ForgeSchedulerTrigger>,
    /// Fixed-cardinality production telemetry injected by process composition.
    pub telemetry: Arc<ForgeTelemetry>,
}

/// The single stateful owner for all Forge maintenance workflows.
pub struct Forge {
    /// Immutable dependencies shared by narrow internal maintenance futures.
    core: Arc<ForgeCore>,
    /// Bounded inbox held only while receiving or draining advisory hints.
    hints: tokio::sync::Mutex<crate::maintenance::StagingFileInbox>,
    /// Rejects a second directly supervised scheduler loop.
    running: AtomicBool,
}

/// Immutable dependency graph shared by one Forge owner.
pub(crate) struct ForgeCore {
    /// Narrow Forge capability used for exact rewrite resource leases.
    resources: crate::resources::ForgeResources,
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
    /// Pod-local base for attempt-owned disposable scratch directories.
    rewrite_spill_root: std::path::PathBuf,
    /// Validated maintenance and rewrite limits.
    config: ForgeConfig,
    /// Delay between periodic scheduler ticks.
    maintenance_interval: Duration,
    /// Wall clock shared by periodic and hinted maintenance batches.
    clock: ForgeClock,
    /// Optional observer notified only after a supervised worker returns success.
    completion_observer: Option<ForgeWorkerCompletionObserver>,
    /// Optional trigger/observer retained only by test-built supervised schedulers.
    scheduler_trigger: Option<ForgeSchedulerTrigger>,
    /// Fixed-cardinality operational metric handles registered at construction.
    telemetry: Arc<ForgeTelemetry>,
    /// Deterministic maintenance boundaries used only by integration tests.
    #[cfg(feature = "test-support")]
    maintenance_controls: maintenance::MaintenanceTestControls,
    /// Owner-bound one-shot failure after maintenance evidence becomes Prepared.
    #[cfg(feature = "test-support")]
    fail_after_maintenance_prepared: AtomicBool,
}

impl Forge {
    /// Construct one Forge owner after validating its complete dependency graph.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when any Forge limit is unsafe,
    /// or the maintenance interval is zero.
    pub fn new(build: ForgeBuildConfig) -> Result<Self, ForgeError> {
        if build.maintenance_interval.is_zero() {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge scheduler interval must be positive".to_owned(),
            });
        }
        build.config.validate()?;
        ForgeRewriteRuntime::prepare_root(&build.rewrite_spill_root)?;
        let core = ForgeCore {
            resources: build.resources,
            vala: build.vala,
            operator_pool: build.operator_pool,
            catalog: build.catalog,
            staging: build.staging,
            object_store: build.object_store,
            rewrite_spill_root: build.rewrite_spill_root,
            config: build.config,
            maintenance_interval: build.maintenance_interval,
            clock: build.clock,
            completion_observer: build.completion_observer,
            scheduler_trigger: build.scheduler_trigger,
            telemetry: build.telemetry,
            #[cfg(feature = "test-support")]
            maintenance_controls: maintenance::MaintenanceTestControls::default(),
            #[cfg(feature = "test-support")]
            fail_after_maintenance_prepared: AtomicBool::new(false),
        };
        Ok(Self {
            core: Arc::new(core),
            hints: tokio::sync::Mutex::new(build.hints),
            running: AtomicBool::new(false),
        })
    }

    /// Returns this Forge clock for test-only fixture reconstruction.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn clock_for_test(&self) -> ForgeClock {
        self.core.clock.clone()
    }

    /// Returns this Forge's narrow resource capability for lifecycle assertions.
    ///
    /// The capability observes the same process root the worker leases from, so
    /// a test can inspect baselines and root identity without gaining the
    /// ability to construct a sibling governor or a raw pool.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn resources_for_test(&self) -> crate::resources::ForgeResources {
        self.core.resources.clone()
    }

    /// Returns deterministic controls for manifest and expiry commit boundaries.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn maintenance_controls_for_test(&self) -> MaintenanceTestControls {
        self.core.maintenance_controls.clone()
    }
}

#[cfg(test)]
/// Verifies that Forge can consume the owned fork's maintenance contract types.
mod iceberg_maintenance_contract_tests {
    use iceberg::io::FileIO;
    use iceberg::spec::TableMetadata;
    use iceberg::table::Table;
    use iceberg::transaction::{
        CleanupTraversalLimits, ExpiredFileSet, ManifestRewriteLimits, ManifestRewriteOutcome,
        ManifestRewriteResult, ManifestRewriteSelection, expired_files_between, rewrite_manifests,
    };
    use iceberg::{Catalog, Result};

    /// Type-checks both maintenance futures without performing metadata or catalog IO.
    async fn assert_owned_fork_function_signatures(
        catalog: &dyn Catalog,
        table: &Table,
        metadata: (&FileIO, &TableMetadata, &TableMetadata),
        rewrite: (ManifestRewriteSelection, ManifestRewriteLimits),
        cleanup_limits: CleanupTraversalLimits,
    ) {
        let (file_io, before, after) = metadata;
        let (rewrite_selection, rewrite_limits) = rewrite;
        let _: Result<ManifestRewriteResult> =
            rewrite_manifests(catalog, table, rewrite_selection, rewrite_limits).await;
        let _: Result<ExpiredFileSet> =
            expired_files_between(file_io, before, after, cleanup_limits).await;
    }

    /// Pins the narrow owned-fork types consumed by later Forge maintenance orchestration.
    #[test]
    fn owned_fork_maintenance_types_are_consumable() {
        let cleanup = CleanupTraversalLimits {
            max_items: 8,
            max_bytes: 1_024,
        };
        let rewrite = ManifestRewriteLimits {
            max_manifests: 2,
            max_entries: 8,
            max_bytes: 1_024,
        };
        let selection = ManifestRewriteSelection {
            manifest_paths: vec!["metadata/manifest.avro".to_owned()],
        };
        let expired = ExpiredFileSet::default();

        assert_eq!(cleanup.max_items, 8);
        assert_eq!(rewrite.max_manifests, 2);
        assert_eq!(selection.manifest_paths.len(), 1);
        assert!(expired.data_files.is_empty());
        assert_eq!(ManifestRewriteOutcome::NoOp, ManifestRewriteOutcome::NoOp);
        let _ = assert_owned_fork_function_signatures;
    }
}
