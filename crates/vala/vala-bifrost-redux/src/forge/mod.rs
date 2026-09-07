//! Forge owns background maintenance for Bifrost's staged and Iceberg data.
//!
//! A scheduler leases one tenant/table at a time, then runs reconciliation,
//! compaction, snapshot expiry, and orphan garbage collection under the same
//! fencing boundary. The stages use the audit outbox and durable file metadata
//! to recover work after a process or catalog failure.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8};
use std::time::Duration;

use iceberg::Catalog;

mod clock;
pub(crate) mod compact;
pub(crate) mod error;
pub(crate) mod expire;
mod expiry_gates;
mod expiry_policy;
mod identity;
pub(crate) mod lease;
mod live_reconcile;
mod live_replace;
pub(crate) mod managed;
mod metrics;
pub(crate) mod orphan_gc;
mod path;
mod phase;
mod planner;
mod planning_scheduler;
mod protection_roots;
pub(crate) mod publication;
mod reader_protection;
mod scheduler;
pub(crate) mod scribe_promotion;
mod worker;

pub use clock::ForgeClock;
#[cfg(feature = "test-support")]
pub use clock::ForgeClockControl;
pub use compact::{ForgeConfig, ForgeObjectPages, ForgeObjectStore};
pub use error::ForgeError;
pub use managed::{
    ForgeRewriteEvidence, ForgeRewriteOutcome, ForgeUnsettledOutput, RewriteHandoff,
};
pub use metrics::ForgeTelemetry;
pub use planner::{ForgePlanCandidate, PlannedForgeTask};
pub use planning_scheduler::{ForgeScheduleOutcome, ForgeScheduler};
#[cfg(feature = "test-support")]
pub use scheduler::ForgeSchedulerTrigger;
#[cfg(feature = "test-support")]
pub use worker::{ForgeLifecycleEvent, ForgeWorkerCompletionObserver};
pub use worker::{ForgeWorker, ForgeWorkerConfig};

#[cfg(feature = "test-support")]
pub use expiry_gates::ExpiryTestControls;
#[cfg(feature = "test-support")]
pub use lease::{ForgeLease, forge_lease_key};
#[cfg(feature = "test-support")]
pub use live_reconcile::LiveReconciliationTestOutcome;
#[cfg(feature = "test-support")]
pub use orphan_gc::{OrphanGcReport, current_gc_gate_for_test};
#[cfg(feature = "test-support")]
pub use worker::ForgeRewriteEvidenceRecord;

/// Role is not routable and may still become ready.
const FORGE_ROLE_UNREADY: u8 = 0;
/// Role currently holds the authority it advertises.
const FORGE_ROLE_READY: u8 = 1;
/// Role is shutting down; readiness can never be republished.
const FORGE_ROLE_CLOSED: u8 = 2;

/// One process-owned readiness bit for a selected Forge role.
///
/// The server owns the bit and publishes it on `/readyz`; the Vala owner that
/// actually performs the role's work sets it. Keeping it a shared handle rather
/// than a return value means readiness reflects the loop's current state, not
/// the last value the server happened to poll. Cleared before, never after, the
/// failure that makes the role unusable, so routing closes ahead of authority
/// loss.
///
/// The state is a three-valued atomic rather than a boolean because shutdown
/// must close routing synchronously, ahead of cancellation, while the role's
/// loop is still running and may still be mid-`publish`. `closed` is terminal,
/// so the close cannot be undone by a later publish that raced it — the atomic,
/// not loop timing, is what keeps the bit down.
#[derive(Debug, Clone, Default)]
pub struct ForgeRoleReadiness(Arc<AtomicU8>);

impl ForgeRoleReadiness {
    /// Creates a readiness bit no health surface observes.
    ///
    /// Used by fixtures and by any caller driving a Forge loop outside the
    /// server, so the loops need no optional readiness parameter.
    #[must_use]
    pub fn detached() -> Self {
        Self::default()
    }

    /// Publishes this role's current readiness.
    ///
    /// `true` promotes only an open, unready role; `false` demotes only a ready
    /// one. Either way a closed role stays closed, so a loop that publishes
    /// after shutdown began cannot reopen routing.
    pub fn publish(&self, ready: bool) {
        let (from, to) = if ready {
            (FORGE_ROLE_UNREADY, FORGE_ROLE_READY)
        } else {
            (FORGE_ROLE_READY, FORGE_ROLE_UNREADY)
        };
        let _ = self.0.compare_exchange(
            from,
            to,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        );
    }

    /// Closes this role's routing permanently.
    ///
    /// Called by the process shutdown owner before the shared Forge token is
    /// cancelled, so `/readyz` stops advertising the role before its loops are
    /// told to stop rather than after they happen to notice.
    pub fn close(&self) {
        self.0
            .store(FORGE_ROLE_CLOSED, std::sync::atomic::Ordering::Release);
    }

    /// Reads the currently published readiness.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Acquire) == FORGE_ROLE_READY
    }
}

/// Construction-time dependency graph for one Forge maintenance handle.
pub struct ForgeBuildConfig {
    /// Immutable root resource plan this process booted with.
    ///
    /// Forge reads the plan rather than holding a live root lease: its only
    /// dynamic memory accounting is the worker-local compaction queue, charged
    /// against `forge_compaction_memory_limit_bytes`.
    pub resource_plan: crate::resources::ResourcePlan,
    /// SQL handle used by tenant-scoped durable Forge transitions.
    pub vala: vala_sql::ValaPostgres,
    /// Cross-tenant operator pool used by discovery and table leases.
    pub operator_pool: vala_sql::OperatorPool,
    /// Iceberg catalog used by maintenance operations.
    pub catalog: Arc<dyn Catalog>,
    /// Raw staging operator retained for table-owned producer fixtures.
    pub staging: Arc<opendal::Operator>,
    /// Whether that staging operator advertises native `list_with_start_after`.
    ///
    /// Read once, from the concrete operator, before it is erased behind
    /// [`ForgeObjectStore`]. Orphan collection resumes a bounded listing by
    /// cursor, so a worker whose backend cannot do that natively must never
    /// register, recover, publish ready, or claim.
    pub staging_lists_by_cursor: bool,
    /// Object-store capability used by rewrites and garbage collection.
    pub object_store: Arc<dyn ForgeObjectStore>,
    /// Bounded advisory Scribe wake-up inbox.
    pub hints: crate::maintenance::StagingFileInbox,
    /// Validated maintenance and rewrite limits.
    pub config: ForgeConfig,
    /// Delay between complete periodic maintenance ticks.
    pub maintenance_interval: Duration,
    /// Concrete wall clock captured once by each Forge work batch.
    pub clock: ForgeClock,
    /// Optional test-only observer of successful supervised task completion.
    #[cfg(feature = "test-support")]
    pub completion_observer: Option<ForgeWorkerCompletionObserver>,
    /// Optional test-only trigger that wakes this owner’s supervised scheduler loop.
    #[cfg(feature = "test-support")]
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
    /// Immutable root resource plan this process booted with.
    resource_plan: crate::resources::ResourcePlan,
    /// Vala SQL handle used by tenant-scoped transitions.
    vala: vala_sql::ValaPostgres,
    /// Operator pool used by discovery and lease operations.
    operator_pool: vala_sql::OperatorPool,
    /// Iceberg catalog used to load tables and commit maintenance actions.
    catalog: Arc<dyn Catalog>,
    /// Raw staging operator retained for the established Forge composition.
    staging: Arc<opendal::Operator>,
    /// Whether the staging operator natively resumes a listing from a cursor.
    staging_lists_by_cursor: bool,
    /// Narrow object-store seam used by rewrite and garbage-collection IO.
    object_store: Arc<dyn ForgeObjectStore>,
    /// Validated maintenance and rewrite limits.
    config: ForgeConfig,
    /// Delay between periodic scheduler ticks.
    maintenance_interval: Duration,
    /// Wall clock shared by periodic and hinted maintenance batches.
    clock: ForgeClock,
    /// Optional observer notified only after a supervised worker returns success.
    #[cfg(feature = "test-support")]
    completion_observer: Option<ForgeWorkerCompletionObserver>,
    /// Optional trigger/observer retained only by test-built supervised schedulers.
    #[cfg(feature = "test-support")]
    scheduler_trigger: Option<ForgeSchedulerTrigger>,
    /// Fixed-cardinality operational metric handles registered at construction.
    telemetry: Arc<ForgeTelemetry>,
    /// Deterministic expiration boundaries used only by integration tests.
    #[cfg(feature = "test-support")]
    expiry_controls: expiry_gates::ExpiryTestControls,
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
        let core = ForgeCore {
            resource_plan: build.resource_plan,
            vala: build.vala,
            operator_pool: build.operator_pool,
            catalog: build.catalog,
            staging: build.staging,
            staging_lists_by_cursor: build.staging_lists_by_cursor,
            object_store: build.object_store,
            config: build.config,
            maintenance_interval: build.maintenance_interval,
            clock: build.clock,
            #[cfg(feature = "test-support")]
            completion_observer: build.completion_observer,
            #[cfg(feature = "test-support")]
            scheduler_trigger: build.scheduler_trigger,
            telemetry: build.telemetry,
            #[cfg(feature = "test-support")]
            expiry_controls: expiry_gates::ExpiryTestControls::default(),
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
    /// The plan is the same immutable calculation the worker admits against,
    /// so a test can assert the composed Forge budget without gaining the
    /// ability to construct a sibling governor or a raw pool.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn resource_plan_for_test(&self) -> crate::resources::ResourcePlan {
        self.core.resource_plan
    }

    /// Returns deterministic controls for the expiry commit boundaries.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn expiry_controls_for_test(&self) -> ExpiryTestControls {
        self.core.expiry_controls.clone()
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

/// Unit coverage for Forge's process-owned role readiness handle.
#[cfg(test)]
mod tests {
    use super::ForgeRoleReadiness;

    /// Closing a role's readiness is terminal for every later publish.
    ///
    /// Shutdown closes routing before it cancels the shared Forge token, so a
    /// role loop that is still running may publish afterwards. Neither value
    /// may reopen the bit, or `/readyz` would advertise a role the process is
    /// already tearing down.
    ///
    /// # Panics
    ///
    /// Panics when a closed handle reports ready again.
    #[test]
    fn role_readiness_close_is_terminal() {
        let readiness = ForgeRoleReadiness::detached();
        assert!(!readiness.is_ready(), "a fresh role is not ready");
        readiness.publish(true);
        assert!(readiness.is_ready(), "an open role publishes ready");

        readiness.close();
        assert!(!readiness.is_ready(), "closing clears routing");
        for republish in [false, true] {
            readiness.publish(republish);
            assert!(
                !readiness.is_ready(),
                "publish({republish}) after close must not reopen routing"
            );
        }
    }
}
