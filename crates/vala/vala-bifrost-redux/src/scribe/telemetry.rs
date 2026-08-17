//! Optional stage measurements for real Scribe workload runs.

use std::sync::{Arc, Mutex};

use crate::scribe::admission::AdmissionSnapshot;
use crate::scribe::material_plan::IngestMaterialPlan;
use crate::scribe::memory::MEMORY_CATEGORY_COUNT;
use crate::scribe::seal_key::SealKey;

/// Bounded cumulative and live ownership facts for Scribe ingress roots.
///
/// The snapshot contains pod-global scalars only. It deliberately carries no
/// tenant, table, request, or batch identity, so inspection remains bounded as
/// request cardinality grows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScribeIngressLifecycleSnapshot {
    /// Logical ingress attempts that entered Scribe processing.
    pub attempts: u64,
    /// Immutable material plans completed before admission.
    pub plans: u64,
    /// Complete root bytes described by those plans.
    pub planned_bytes: usize,
    /// Source descriptors represented by completed plans.
    pub planned_sources: u64,
    /// Event-day descriptors represented by completed plans.
    pub planned_event_days: u64,
    /// Rows represented by completed plans.
    pub planned_rows: u64,
    /// Root reservations successfully established.
    pub reservations: u64,
    /// Root bytes successfully reserved.
    pub reserved_bytes: usize,
    /// Current slices materialized under admitted roots.
    pub materializations: u64,
    /// Arrow, IPC, and bounded audit bytes observed for materialized slices.
    pub materialized_bytes: usize,
    /// Root owners transferred into fixed shard lanes.
    pub shard_transfers: u64,
    /// Root bytes transferred into fixed shard lanes.
    pub shard_transferred_bytes: usize,
    /// Current material payloads transferred into WAL ownership.
    pub transfers: u64,
    /// Current material bytes transferred into WAL ownership.
    pub transferred_bytes: usize,
    /// Admitted root owners terminally released.
    pub releases: u64,
    /// Admitted root bytes terminally released.
    pub released_bytes: usize,
    /// Attempts currently owned by any Scribe stage.
    pub active_attempts: usize,
    /// Root owners currently reserved.
    pub active_reservations: usize,
    /// Root bytes currently reserved.
    pub active_reserved_bytes: usize,
    /// Materialized slices currently retained by active roots.
    pub active_materializations: usize,
    /// Materialized bytes currently retained by active roots.
    pub active_materialized_bytes: usize,
    /// Root owners currently transferred to shard lanes.
    pub active_shard_transfers: usize,
    /// Root bytes currently transferred to shard lanes.
    pub active_shard_transferred_bytes: usize,
    /// Attempts that reached durable public acknowledgement.
    pub succeeded: u64,
    /// Attempts that terminated through a stable refusal.
    pub refused: u64,
    /// Attempts dropped by cancellation or shutdown before a terminal result.
    pub cancelled: u64,
    /// Attempts whose owner unwound through a panic.
    pub panicked: u64,
    /// Attempts explicitly settled by shutdown refusal.
    pub shutdown: u64,
}

/// Fixed-size pod-global owner for ingress lifecycle inspection.
#[derive(Debug, Default)]
pub(crate) struct ScribeIngressLifecycle {
    /// Scalar lifecycle ledger shared by move-only request owners.
    state: Mutex<ScribeIngressLifecycleSnapshot>,
}

impl ScribeIngressLifecycle {
    /// Starts one move-only ingress observation owner.
    pub(crate) fn begin(self: &Arc<Self>) -> ScribeIngressLifecycleOwner {
        self.with_state(|state| {
            state.attempts = state.attempts.saturating_add(1);
            state.active_attempts = state.active_attempts.saturating_add(1);
        });
        ScribeIngressLifecycleOwner {
            lifecycle: Arc::clone(self),
            reserved_bytes: 0,
            materialized_bytes: 0,
            materializations: 0,
            shard_transferred_bytes: 0,
            terminal: None,
        }
    }

    /// Returns one coherent scalar snapshot without exposing the mutable ledger.
    #[must_use]
    pub(crate) fn snapshot(&self) -> ScribeIngressLifecycleSnapshot {
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Applies one bounded ledger transition, recovering inspection after poison.
    fn with_state(&self, update: impl FnOnce(&mut ScribeIngressLifecycleSnapshot)) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        update(&mut state);
    }
}

/// Terminal classification applied exactly once when an ingress owner settles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IngressTerminal {
    /// Durable acknowledgement completed.
    Succeeded,
    /// A stable request or runtime refusal completed.
    Refused,
    /// Shutdown refused or cancelled the request.
    Shutdown,
}

/// Move-only lifecycle evidence retained beside the admitted Scribe root.
///
/// Dropping the owner always releases its live byte and cardinality gauges.
/// An unclassified normal drop is cancellation; a drop during unwinding is a
/// panic. Explicit success, refusal, and shutdown settlement override that
/// fallback classification.
#[derive(Debug)]
pub(crate) struct ScribeIngressLifecycleOwner {
    /// Pod-global bounded lifecycle ledger.
    lifecycle: Arc<ScribeIngressLifecycle>,
    /// Exact root bytes reserved by this attempt, or zero before admission.
    reserved_bytes: usize,
    /// Materialized bytes accumulated by current-slice production.
    materialized_bytes: usize,
    /// Materialized slice count accumulated by current-slice production.
    materializations: usize,
    /// Exact root bytes transferred to a shard, or zero before transfer.
    shard_transferred_bytes: usize,
    /// Explicit terminal result, if the production owner reached one.
    terminal: Option<IngressTerminal>,
}

impl ScribeIngressLifecycleOwner {
    /// Records immutable pre-admission plan bytes and bounded cardinalities.
    pub(crate) fn planned(&mut self, plan: &IngestMaterialPlan) {
        self.lifecycle.with_state(|state| {
            state.plans = state.plans.saturating_add(1);
            state.planned_bytes = state.planned_bytes.saturating_add(plan.root_bytes);
            state.planned_sources = state
                .planned_sources
                .saturating_add(u64::try_from(plan.source_count).unwrap_or(u64::MAX));
            state.planned_event_days = state
                .planned_event_days
                .saturating_add(u64::try_from(plan.event_day_count).unwrap_or(u64::MAX));
            state.planned_rows = state
                .planned_rows
                .saturating_add(u64::try_from(plan.rows).unwrap_or(u64::MAX));
        });
    }

    /// Records one exact root reservation.
    pub(crate) fn reserved(&mut self, bytes: usize) {
        debug_assert_eq!(self.reserved_bytes, 0, "one root reservation per ingress");
        self.reserved_bytes = bytes;
        self.lifecycle.with_state(|state| {
            state.reservations = state.reservations.saturating_add(1);
            state.reserved_bytes = state.reserved_bytes.saturating_add(bytes);
            state.active_reservations = state.active_reservations.saturating_add(1);
            state.active_reserved_bytes = state.active_reserved_bytes.saturating_add(bytes);
        });
    }

    /// Records one newly materialized current slice under this root.
    pub(crate) fn materialized(&mut self, bytes: usize) {
        self.materialized_bytes = self.materialized_bytes.saturating_add(bytes);
        self.materializations = self.materializations.saturating_add(1);
        self.lifecycle.with_state(|state| {
            state.materializations = state.materializations.saturating_add(1);
            state.materialized_bytes = state.materialized_bytes.saturating_add(bytes);
            state.active_materializations = state.active_materializations.saturating_add(1);
            state.active_materialized_bytes = state.active_materialized_bytes.saturating_add(bytes);
        });
    }

    /// Records transfer of the exact root into one fixed shard lane.
    pub(crate) fn shard_transferred(&mut self) {
        debug_assert_ne!(
            self.reserved_bytes, 0,
            "transfer requires one admitted root"
        );
        debug_assert_eq!(
            self.shard_transferred_bytes, 0,
            "one shard transfer per ingress"
        );
        self.shard_transferred_bytes = self.reserved_bytes;
        self.lifecycle.with_state(|state| {
            state.shard_transfers = state.shard_transfers.saturating_add(1);
            state.shard_transferred_bytes = state
                .shard_transferred_bytes
                .saturating_add(self.reserved_bytes);
            state.active_shard_transfers = state.active_shard_transfers.saturating_add(1);
            state.active_shard_transferred_bytes = state
                .active_shard_transferred_bytes
                .saturating_add(self.reserved_bytes);
        });
    }

    /// Reverts a shard transfer when the bounded mailbox refuses it.
    pub(crate) fn revert_shard_transfer(&mut self) {
        let bytes = std::mem::take(&mut self.shard_transferred_bytes);
        if bytes == 0 {
            return;
        }
        self.lifecycle.with_state(|state| {
            state.shard_transfers = state.shard_transfers.saturating_sub(1);
            state.shard_transferred_bytes = state.shard_transferred_bytes.saturating_sub(bytes);
            state.active_shard_transfers = state.active_shard_transfers.saturating_sub(1);
            state.active_shard_transferred_bytes =
                state.active_shard_transferred_bytes.saturating_sub(bytes);
        });
    }

    /// Transfers one current material payload into WAL ownership.
    pub(crate) fn transferred_to_wal(&mut self, bytes: usize) {
        self.released_materialization(bytes);
        self.lifecycle.with_state(|state| {
            state.transfers = state.transfers.saturating_add(1);
            state.transferred_bytes = state.transferred_bytes.saturating_add(bytes);
        });
    }

    /// Releases one current-only material payload without transferring it to WAL.
    pub(crate) fn released_materialization(&mut self, bytes: usize) {
        self.materialized_bytes = self.materialized_bytes.saturating_sub(bytes);
        self.materializations = self.materializations.saturating_sub(1);
        self.lifecycle.with_state(|state| {
            state.active_materializations = state.active_materializations.saturating_sub(1);
            state.active_materialized_bytes = state.active_materialized_bytes.saturating_sub(bytes);
        });
    }

    /// Marks durable acknowledgement as the terminal attempt result.
    pub(crate) fn succeed(&mut self) {
        self.terminal = Some(IngressTerminal::Succeeded);
    }

    /// Marks a stable refusal as the terminal attempt result.
    pub(crate) fn refuse(&mut self) {
        self.terminal = Some(IngressTerminal::Refused);
    }

    /// Marks shutdown refusal or cancellation as the terminal attempt result.
    pub(crate) fn settle_shutdown(&mut self) {
        self.terminal = Some(IngressTerminal::Shutdown);
    }
}

impl Drop for ScribeIngressLifecycleOwner {
    /// Releases every live scalar and records exactly one terminal outcome.
    fn drop(&mut self) {
        let terminal = self.terminal.unwrap_or_else(|| {
            if std::thread::panicking() {
                return IngressTerminal::Refused;
            }
            IngressTerminal::Shutdown
        });
        let panicked = self.terminal.is_none() && std::thread::panicking();
        let cancelled = self.terminal.is_none() && !panicked;
        self.lifecycle.with_state(|state| {
            state.active_attempts = state.active_attempts.saturating_sub(1);
            if self.reserved_bytes != 0 {
                state.releases = state.releases.saturating_add(1);
                state.released_bytes = state.released_bytes.saturating_add(self.reserved_bytes);
                state.active_reservations = state.active_reservations.saturating_sub(1);
                state.active_reserved_bytes = state
                    .active_reserved_bytes
                    .saturating_sub(self.reserved_bytes);
            }
            state.active_materializations = state
                .active_materializations
                .saturating_sub(self.materializations);
            state.active_materialized_bytes = state
                .active_materialized_bytes
                .saturating_sub(self.materialized_bytes);
            if self.shard_transferred_bytes != 0 {
                state.active_shard_transfers = state.active_shard_transfers.saturating_sub(1);
                state.active_shard_transferred_bytes = state
                    .active_shard_transferred_bytes
                    .saturating_sub(self.shard_transferred_bytes);
            }
            if panicked {
                state.panicked = state.panicked.saturating_add(1);
            } else if cancelled {
                state.cancelled = state.cancelled.saturating_add(1);
            } else {
                match terminal {
                    IngressTerminal::Succeeded => {
                        state.succeeded = state.succeeded.saturating_add(1);
                    }
                    IngressTerminal::Refused => {
                        state.refused = state.refused.saturating_add(1);
                    }
                    IngressTerminal::Shutdown => {
                        state.shutdown = state.shutdown.saturating_add(1);
                    }
                }
            }
        });
    }
}

/// Memory ownership for one bounded tenant/table/day bucket in inspection data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScribeBucketMemorySnapshot {
    /// Exact bucket identity. This is an inspection payload, not a metric label.
    pub seal_key: SealKey,
    /// Writable Arrow bytes.
    pub writable_bytes: usize,
    /// Immutable Arrow bytes.
    pub immutable_bytes: usize,
}

/// Complete bounded setup snapshot used by Bifrost test harnesses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScribeInspectionSnapshot {
    /// Number of fixed shard owner tasks.
    pub shard_task_count: usize,
    /// Number of fixed shard command channels.
    pub shard_channel_count: usize,
    /// Number of currently open shard WAL streams.
    pub open_wal_stream_count: usize,
    /// Accepted append commands waiting in shard scheduling.
    pub queued_items: usize,
    /// Writable bucket count.
    pub writable_bucket_count: usize,
    /// Immutable bucket count.
    pub immutable_bucket_count: usize,
    /// Category totals in [`MemoryCategory`](crate::scribe::memory::MemoryCategory) order.
    pub memory_by_category: [usize; MEMORY_CATEGORY_COUNT],
    /// Memory totals distributed across the fixed shard owners.
    pub memory_by_shard: [usize; crate::scribe::routing::SCRIBE_SHARD_COUNT],
    /// Exact bucket-level memory ownership.
    pub memory_by_bucket: Vec<ScribeBucketMemorySnapshot>,
    /// Sum of all governor category totals.
    pub total_accounted_memory: usize,
    /// Parent Bifrost bytes currently reserved across all roles.
    pub parent_used_memory: usize,
    /// Parent Bifrost memory ceiling.
    pub parent_memory_limit: usize,
    /// Scribe child bytes currently reserved.
    pub scribe_used_memory: usize,
    /// Scribe child memory ceiling.
    pub scribe_memory_limit: usize,
    /// Current ingress bytes charged against the ingress ceiling.
    ///
    /// Equals `scribe_used_memory`; ingress admission charges the whole Scribe
    /// child total against the (smaller) ingress ceiling. Exposed so harnesses
    /// can observe the D83 pressure-seal watermark decision — occupancy against
    /// [`Self::ingress_memory_limit`] and the high/low-water marks below —
    /// without recomputing the runtime pressure config.
    pub ingress_used_memory: usize,
    /// Ingress reservation ceiling (`limit_bytes - persistence_headroom`).
    pub ingress_memory_limit: usize,
    /// High-water byte threshold that triggers a coordinated pressure seal.
    pub ingress_high_water_memory: usize,
    /// Low-water byte target that pressure sealing drains toward.
    pub ingress_low_water_memory: usize,
    /// WAL bytes retained on disk.
    pub wal_disk_bytes: u64,
    /// Bounded pod-global ingress ownership lifecycle observations.
    pub ingress_lifecycle: ScribeIngressLifecycleSnapshot,
}

/// Point-in-time health and queue metrics for the fixed shard owners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShardHealthSnapshot {
    /// Fixed shard owner tasks.
    pub shard_tasks: usize,
    /// Fixed bounded shard command channels.
    pub shard_channels: usize,
    /// Items currently accepted by the shard runtime.
    pub pending_items: usize,
    /// Terminal shard owner errors.
    pub terminal_errors: usize,
}

/// Aggregate queue metrics for runtime dashboards.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecutorSnapshot {
    /// Operations currently queued or executing on this lane.
    pub depth: usize,
    /// Application queue capacity for this lane.
    pub capacity: usize,
    /// Number of submissions that encountered lane saturation.
    pub saturation_events: u64,
    /// Work items completed successfully.
    pub completed: u64,
    /// Work items that returned an error.
    pub failed: u64,
    /// Work items terminated by a worker panic.
    pub panicked: u64,
}

/// Point-in-time queue, admission, execution-lane, and shard health telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScribeRuntimeSnapshot {
    /// Pod-global admission counters and limits.
    pub admission: AdmissionSnapshot,
    /// Aggregate execution-lane queue metrics.
    pub executor: ExecutorSnapshot,
    /// Pre-ACK decode and projection lane.
    pub ingress: ExecutorSnapshot,
    /// Pre-ACK preprocessing and reconstruction lane.
    pub persistence: ExecutorSnapshot,
    /// WAL filesystem lane.
    pub wal_io: ExecutorSnapshot,
    /// Fixed shard owner topology and health metrics.
    pub shards: ShardHealthSnapshot,
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::Arc;

    use super::ScribeIngressLifecycle;
    use crate::scribe::material_plan::{
        IngestMaterialPlan, IngestPath, MAX_EVENT_DAYS, MAX_SOURCE_PLANS, SourceMaterialPlan,
    };

    /// Builds one exact scalar plan used to exercise lifecycle accounting.
    fn material_plan(root_bytes: usize) -> IngestMaterialPlan {
        IngestMaterialPlan {
            path: IngestPath::Native,
            request_bytes: 16,
            planner_bytes: 8,
            name_bytes: 4,
            aligned_copy_bytes: 0,
            native_schema_start: 0,
            native_schema_end: 0,
            native_schema_material_bytes: 0,
            native_metadata_scratch_bytes: 0,
            sources: [SourceMaterialPlan::default(); MAX_SOURCE_PLANS],
            source_count: 2,
            rows: 7,
            event_days: [0; MAX_EVENT_DAYS],
            event_day_count: 1,
            current_material_bytes: 32,
            active_output_bytes: 24,
            durable_metadata_bytes: 8,
            wal_workspace_bytes: 8,
            root_bytes,
        }
    }

    /// Success records every byte boundary and leaves no live owner cardinality.
    #[test]
    fn success_settles_planned_reserved_materialized_transferred_and_released() {
        let lifecycle = Arc::new(ScribeIngressLifecycle::default());
        {
            let mut owner = lifecycle.begin();
            owner.planned(&material_plan(128));
            owner.reserved(128);
            owner.shard_transferred();
            owner.materialized(48);
            owner.transferred_to_wal(48);
            owner.succeed();
        }
        let snapshot = lifecycle.snapshot();
        assert_eq!(snapshot.attempts, 1);
        assert_eq!(snapshot.plans, 1);
        assert_eq!(snapshot.planned_bytes, 128);
        assert_eq!(snapshot.planned_sources, 2);
        assert_eq!(snapshot.planned_event_days, 1);
        assert_eq!(snapshot.planned_rows, 7);
        assert_eq!(snapshot.reserved_bytes, 128);
        assert_eq!(snapshot.materialized_bytes, 48);
        assert_eq!(snapshot.shard_transferred_bytes, 128);
        assert_eq!(snapshot.transferred_bytes, 48);
        assert_eq!(snapshot.released_bytes, 128);
        assert_eq!(snapshot.succeeded, 1);
        assert_eq!(snapshot.active_attempts, 0);
        assert_eq!(snapshot.active_reservations, 0);
        assert_eq!(snapshot.active_reserved_bytes, 0);
        assert_eq!(snapshot.active_materializations, 0);
        assert_eq!(snapshot.active_materialized_bytes, 0);
        assert_eq!(snapshot.active_shard_transfers, 0);
        assert_eq!(snapshot.active_shard_transferred_bytes, 0);
    }

    /// Refusal, cancellation, and panic each release their exact admitted root.
    #[test]
    fn refusal_cancellation_and_panic_settle_without_double_release() {
        let lifecycle = Arc::new(ScribeIngressLifecycle::default());
        {
            let mut refused = lifecycle.begin();
            refused.reserved(10);
            refused.refuse();
        }
        {
            let mut cancelled = lifecycle.begin();
            cancelled.reserved(20);
        }
        let panic_lifecycle = Arc::clone(&lifecycle);
        let result = catch_unwind(AssertUnwindSafe(move || {
            let mut panicked = panic_lifecycle.begin();
            panicked.reserved(30);
            panic!("deterministic lifecycle unwind");
        }));
        assert!(result.is_err());

        let snapshot = lifecycle.snapshot();
        assert_eq!(snapshot.refused, 1);
        assert_eq!(snapshot.cancelled, 1);
        assert_eq!(snapshot.panicked, 1);
        assert_eq!(snapshot.reservations, 3);
        assert_eq!(snapshot.releases, 3);
        assert_eq!(snapshot.reserved_bytes, 60);
        assert_eq!(snapshot.released_bytes, 60);
        assert_eq!(snapshot.active_attempts, 0);
        assert_eq!(snapshot.active_reserved_bytes, 0);
    }

    /// A refused first attempt and successful retry settle independently, and
    /// shutdown has its own terminal cardinality.
    #[test]
    fn retry_and_shutdown_preserve_attempt_cardinality_and_zero_live_state() {
        let lifecycle = Arc::new(ScribeIngressLifecycle::default());
        {
            let mut first = lifecycle.begin();
            first.reserved(64);
            first.refuse();
        }
        {
            let mut retry = lifecycle.begin();
            retry.reserved(64);
            retry.shard_transferred();
            retry.succeed();
        }
        {
            let mut shutdown = lifecycle.begin();
            shutdown.reserved(32);
            shutdown.settle_shutdown();
        }

        let snapshot = lifecycle.snapshot();
        assert_eq!(snapshot.attempts, 3);
        assert_eq!(snapshot.refused, 1);
        assert_eq!(snapshot.succeeded, 1);
        assert_eq!(snapshot.shutdown, 1);
        assert_eq!(snapshot.reservations, 3);
        assert_eq!(snapshot.releases, 3);
        assert_eq!(snapshot.reserved_bytes, 160);
        assert_eq!(snapshot.released_bytes, 160);
        assert_eq!(snapshot.active_attempts, 0);
        assert_eq!(snapshot.active_reservations, 0);
        assert_eq!(snapshot.active_shard_transfers, 0);
    }

    /// Each WAL transfer clears the sole live material payload before the next slice.
    #[test]
    fn multi_slice_transfer_preserves_current_only_materialization() {
        let lifecycle = Arc::new(ScribeIngressLifecycle::default());
        let mut owner = lifecycle.begin();
        owner.reserved(128);
        owner.shard_transferred();

        owner.materialized(24);
        assert_eq!(lifecycle.snapshot().active_materializations, 1);
        assert_eq!(lifecycle.snapshot().active_materialized_bytes, 24);
        owner.transferred_to_wal(24);
        assert_eq!(lifecycle.snapshot().active_materializations, 0);
        assert_eq!(lifecycle.snapshot().active_materialized_bytes, 0);

        owner.materialized(32);
        assert_eq!(lifecycle.snapshot().active_materializations, 1);
        assert_eq!(lifecycle.snapshot().active_materialized_bytes, 32);
        owner.transferred_to_wal(32);
        owner.succeed();
        drop(owner);

        let snapshot = lifecycle.snapshot();
        assert_eq!(snapshot.materializations, 2);
        assert_eq!(snapshot.materialized_bytes, 56);
        assert_eq!(snapshot.transfers, 2);
        assert_eq!(snapshot.transferred_bytes, 56);
        assert_eq!(snapshot.active_materializations, 0);
        assert_eq!(snapshot.active_materialized_bytes, 0);
        assert_eq!(snapshot.releases, 1);
    }
}
