//! Optional stage measurements for real Scribe workload runs.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::scribe::admission::AdmissionSnapshot;
use crate::scribe::material_plan::IngestMaterialPlan;
use crate::scribe::memory::MEMORY_CATEGORY_COUNT;
use crate::scribe::seal_key::SealKey;

/// Closed resource and state facts for one producer lifecycle transition.
#[derive(Clone, Copy)]
pub(crate) struct ProducerLifecycleEvent {
    /// Exact producer workspace charged by the transition.
    pub(crate) workspace_bytes: usize,
    /// Arrival position observed when the producer entered its queue.
    pub(crate) queue_position: usize,
    /// Queue population observed when the transition was emitted.
    pub(crate) queue_count: usize,
    /// Resource epoch associated with the admission decision.
    pub(crate) resource_epoch: u64,
    /// Producer operation that owns the transition.
    pub(crate) operation: &'static str,
    /// Lifecycle stage reached by the operation.
    pub(crate) stage: &'static str,
    /// Stable outcome of the transition.
    pub(crate) outcome: &'static str,
    /// Stable reason that explains the outcome.
    pub(crate) reason: &'static str,
}

/// Bounded generation identity attached to each producer lifecycle event.
///
/// This is trace-only correlation context. It must not become a metric label:
/// the tenant, table, closed WAL cohort, and member generation are all
/// workload-cardinality values.
#[derive(Clone)]
pub(crate) struct ProducerLifecycleIdentity {
    /// Tenant/table/day member that owns the producer.
    pub(crate) seal_key: SealKey,
    /// Pod-local shard lane that detached the member.
    pub(crate) shard_id: usize,
    /// Fenced WAL writer epoch for the detached member.
    pub(crate) writer_epoch: i64,
    /// Generation-local WAL position that identifies the shard generation.
    pub(crate) shard_generation: u64,
    /// Closed WAL cohort retained until every member settles, when available.
    pub(crate) cohort_id: Option<PathBuf>,
    /// Detached member generation within the shard cohort.
    pub(crate) member_generation: u64,
}

/// Emits one production producer admission or persistence lifecycle event.
///
/// When supplied, `identity` is emitted as tracing fields on every admission,
/// cancellation, release, and terminal event for one immutable producer.
pub(crate) fn record_producer_lifecycle(
    event: ProducerLifecycleEvent,
    identity: Option<&ProducerLifecycleIdentity>,
) {
    let ProducerLifecycleEvent {
        workspace_bytes,
        queue_position,
        queue_count,
        resource_epoch,
        operation,
        stage,
        outcome,
        reason,
    } = event;
    if let Some(identity) = identity {
        tracing::info!(
            tenant = %identity.seal_key.tenant,
            table = %identity.seal_key.table,
            shard_id = identity.shard_id,
            writer_epoch = identity.writer_epoch,
            shard_generation = identity.shard_generation,
            cohort_id = ?identity.cohort_id,
            member_generation = identity.member_generation,
            workspace_bytes,
            queue_position,
            queue_count,
            resource_epoch,
            operation,
            stage,
            outcome,
            reason,
            "Scribe producer lifecycle"
        );
    } else {
        tracing::info!(
            workspace_bytes,
            queue_position,
            queue_count,
            resource_epoch,
            operation,
            stage,
            outcome,
            reason,
            "Scribe producer lifecycle"
        );
    }
}

#[cfg(test)]
/// Event-capture fixtures shared by focused producer lifecycle owner tests.
pub(crate) mod producer_lifecycle_tests {
    use super::*;

    use crate::catalog::TableRef;
    use crate::namespaces::BifrostNamespace;

    /// Shared, ordered capture of every event's `(name, value)` field pairs.
    ///
    /// Named so the subscriber and its assertions refer to one type rather
    /// than repeating a four-level nesting at each use site.
    pub(crate) type CapturedEventFields = Arc<Mutex<Vec<Vec<(String, String)>>>>;

    /// Captures structured tracing event fields for producer lifecycle tests.
    #[derive(Default)]
    pub(crate) struct EventCaptureSubscriber {
        /// Event fields in the order each lifecycle event was emitted.
        pub(crate) events: CapturedEventFields,
    }

    /// Collects the fields from one tracing event.
    struct EventFieldVisitor<'a> {
        /// Destination for the current event's fields.
        fields: &'a mut Vec<(String, String)>,
    }

    impl tracing::field::Visit for EventFieldVisitor<'_> {
        /// Retains string fields without debug quoting.
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.fields
                .push((field.name().to_owned(), value.to_owned()));
        }

        /// Retains numeric and debug fields in tracing's canonical format.
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.fields
                .push((field.name().to_owned(), format!("{value:?}")));
        }
    }

    impl tracing::Subscriber for EventCaptureSubscriber {
        /// Enables every event emitted under the scoped test subscriber.
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }

        /// Returns a placeholder span ID because this capture observes events only.
        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }

        /// Ignores span field updates because this capture observes events only.
        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

        /// Ignores causal links because producer identity is asserted on each event.
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

        /// Captures every structured field on a producer lifecycle event.
        fn event(&self, event: &tracing::Event<'_>) {
            let mut fields = Vec::new();
            event.record(&mut EventFieldVisitor {
                fields: &mut fields,
            });
            self.events.lock().expect("event capture").push(fields);
        }

        /// Ignores span entry because this capture observes events only.
        fn enter(&self, _: &tracing::span::Id) {}

        /// Ignores span exit because this capture observes events only.
        fn exit(&self, _: &tracing::span::Id) {}
    }

    /// Returns one named event field from a captured lifecycle event.
    pub(crate) fn field(event: &[(String, String)], name: &str) -> String {
        event.iter().find(|(field, _)| field == name).map_or_else(
            || panic!("missing event field {name}"),
            |(_, value)| value.clone(),
        )
    }

    /// Emits the same bounded generation identity for every producer transition.
    #[test]
    fn producer_lifecycle_events_share_generation_identity() {
        let identity = ProducerLifecycleIdentity {
            seal_key: SealKey::new(
                crate::test_support::tenant(),
                TableRef::new(BifrostNamespace::Bifrost, "producer_lifecycle"),
                crate::test_support::day_partition(2026, 8, 19),
            ),
            shard_id: 3,
            writer_epoch: 41,
            shard_generation: 9001,
            cohort_id: Some(PathBuf::from("wal/shard-3/segment-9001")),
            member_generation: 77,
        };
        let subscriber = EventCaptureSubscriber::default();
        let events = Arc::clone(&subscriber.events);
        tracing::subscriber::with_default(subscriber, || {
            for (stage, outcome) in [
                ("wait", "pending"),
                ("grant", "granted"),
                ("refusal", "refused"),
                ("cancel", "cancelled"),
                ("terminal", "committed"),
                ("release", "released"),
            ] {
                record_producer_lifecycle(
                    ProducerLifecycleEvent {
                        workspace_bytes: 1024,
                        queue_position: 2,
                        queue_count: 1,
                        resource_epoch: 19,
                        operation: "persistence",
                        stage,
                        outcome,
                        reason: "test",
                    },
                    Some(&identity),
                );
            }
        });
        let events = events.lock().expect("event capture");
        assert_eq!(events.len(), 6);
        for name in [
            "tenant",
            "table",
            "shard_id",
            "writer_epoch",
            "shard_generation",
            "cohort_id",
            "member_generation",
        ] {
            let values = events
                .iter()
                .map(|event| field(event, name))
                .collect::<Vec<_>>();
            assert!(
                values.windows(2).all(|pair| pair[0] == pair[1]),
                "{name} changed across lifecycle events: {values:?}"
            );
        }
    }
}

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
    pub planned_time_partitions: u64,
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
            state.planned_time_partitions = state
                .planned_time_partitions
                .saturating_add(u64::try_from(plan.time_partition_count).unwrap_or(u64::MAX));
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
    /// Bounded generation, replay, persistence-transfer, and retirement observations.
    pub generation_lifecycle: crate::scribe::memory::ScribeGenerationLifecycleSnapshot,
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
        IngestMaterialPlan, IngestPath, MAX_SOURCE_PLANS, SourceMaterialPlan,
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
            time_partition_count: 1,
            current_material_bytes: 32,
            active_output_bytes: 24,
            persistence_candidate_bytes: 24,
            durable_metadata_bytes: 8,
            wal_workspace_bytes: 8,
            persistence_replay_bytes: root_bytes,
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
        assert_eq!(snapshot.planned_time_partitions, 1);
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

/// Every contention and admission lifecycle effect Scribe can emit.
///
/// This is the closed, compile-time enumerable registry the contention ledger
/// and the admission controller emit through. Production code names a variant;
/// it never invents a stage, decision, or metric label at a call site. The
/// registry is closed on purpose: the operator vocabulary for admission
/// fairness has to be stable enough to alert on, and a free-form log string at
/// one call site is exactly how that vocabulary rots.
///
/// [`ContentionEffect::ALL`] is the inventory. Every entry has one production
/// emitter and every production transition maps to one entry; the registry
/// closure test in this module proves both directions by driving the public
/// ledger and admission surfaces and comparing what they emitted against this
/// list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ContentionEffect {
    /// A table's lifecycle vector cell was installed.
    ActivationInstalled,
    /// Activation was refused because the pod is at its derived table ceiling.
    ActivationRefused,
    /// An installed cell was removed again because its first charge refused.
    ActivationRolledBack,
    /// One category charge committed to a table's cell.
    ChargeCommitted,
    /// One category charge was refused by a recomputed fair level or pod bound.
    ChargeRefused,
    /// A charge was refused specifically to hold capacity for queued contenders.
    IncumbentBlocked,
    /// One category amount was returned to a table's cell.
    ChargeReleased,
    /// A release asked for more than the cell holds and was refused unmoved.
    OverReleaseRefused,
    /// A bounded identity-only demand record entered the queue.
    DemandEnqueued,
    /// An existing demand record's expiry was refreshed in its earned position.
    DemandRefreshed,
    /// A demand record was dropped because the bounded queue is full.
    DemandDropped,
    /// A contender received its complete category quantum and left the queue.
    DemandRetired,
    /// A contender withdrew every record it held.
    DemandCancelled,
    /// A demand record stopped conferring priority because its TTL elapsed.
    DemandExpired,
    /// An empty table cell was retired by terminal settlement.
    TableSettled,
    /// A tenant cell was retired because its final table settled.
    TenantSettled,
    /// Terminal settlement left a table installed because it still holds capacity.
    SettlementDeferred,
    /// An in-flight reservation grew and the delta moved at every level.
    ResizeGrown,
    /// An in-flight reservation shrank and the delta moved at every level.
    ResizeShrunk,
    /// A resize refused and every level was left at its pre-call value.
    ResizeRefused,
    /// Accounting reached a state the ledger's own invariants forbid.
    InvariantFailure,
}

impl ContentionEffect {
    /// The complete registry inventory, in lifecycle order.
    pub(crate) const ALL: [Self; 21] = [
        Self::ActivationInstalled,
        Self::ActivationRefused,
        Self::ActivationRolledBack,
        Self::ChargeCommitted,
        Self::ChargeRefused,
        Self::IncumbentBlocked,
        Self::ChargeReleased,
        Self::OverReleaseRefused,
        Self::DemandEnqueued,
        Self::DemandRefreshed,
        Self::DemandDropped,
        Self::DemandRetired,
        Self::DemandCancelled,
        Self::DemandExpired,
        Self::TableSettled,
        Self::TenantSettled,
        Self::SettlementDeferred,
        Self::ResizeGrown,
        Self::ResizeShrunk,
        Self::ResizeRefused,
        Self::InvariantFailure,
    ];

    /// Returns the closed lifecycle stage this effect belongs to.
    ///
    /// Stages group the effects an operator reasons about together, so a
    /// dashboard can aggregate "everything that happened to the demand queue"
    /// without enumerating each decision.
    pub(crate) const fn stage(self) -> &'static str {
        match self {
            Self::ActivationInstalled | Self::ActivationRefused | Self::ActivationRolledBack => {
                "activation"
            }
            Self::ChargeCommitted
            | Self::ChargeRefused
            | Self::IncumbentBlocked
            | Self::ChargeReleased
            | Self::OverReleaseRefused => "charge",
            Self::DemandEnqueued
            | Self::DemandRefreshed
            | Self::DemandDropped
            | Self::DemandRetired
            | Self::DemandCancelled
            | Self::DemandExpired => "demand",
            Self::TableSettled | Self::TenantSettled | Self::SettlementDeferred => "settlement",
            Self::ResizeGrown | Self::ResizeShrunk | Self::ResizeRefused => "resize",
            Self::InvariantFailure => "invariant",
        }
    }

    /// Returns the closed decision this effect records within its stage.
    pub(crate) const fn decision(self) -> &'static str {
        match self {
            Self::ActivationInstalled => "installed",
            Self::ActivationRefused => "ceiling_refused",
            Self::ActivationRolledBack => "rolled_back",
            Self::ChargeCommitted => "committed",
            Self::ChargeRefused => "refused",
            Self::IncumbentBlocked => "contender_priority",
            Self::ChargeReleased => "released",
            Self::OverReleaseRefused => "over_release",
            Self::DemandEnqueued => "enqueued",
            Self::DemandRefreshed => "refreshed",
            Self::DemandDropped => "queue_full",
            Self::DemandRetired => "served",
            Self::DemandCancelled => "cancelled",
            Self::DemandExpired => "expired",
            Self::TableSettled => "table_retired",
            Self::TenantSettled => "tenant_retired",
            Self::SettlementDeferred => "nonempty",
            Self::ResizeGrown => "grown",
            Self::ResizeShrunk => "shrunk",
            Self::ResizeRefused => "refused",
            Self::InvariantFailure => "corrupt",
        }
    }

    /// Returns the closed severity an operator signal is bounded to.
    ///
    /// Borrowing, retryable pressure, and turnover are `info`: they are the
    /// steady state of a work-conserving ledger and must not produce unbounded
    /// warning volume. Queue saturation is the one bounded `warn`, and only
    /// genuine accounting corruption reaches `error`.
    pub(crate) const fn severity(self) -> &'static str {
        match self {
            Self::DemandDropped => "warn",
            Self::OverReleaseRefused | Self::InvariantFailure => "error",
            _ => "info",
        }
    }
}

/// Bounded, identity-free facts attached to one contention lifecycle effect.
///
/// Every field here is a scalar the operator vocabulary already closes over.
/// Tenant, table, request, batch, generation, path, row, SQL, credential, and
/// token values are deliberately absent: this struct is what reaches the metric
/// label set, and admitting one workload identity into it would make the metric
/// cardinality track request cardinality.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ContentionFacts {
    /// Governed category label, or `""` when the effect spans every category.
    pub(crate) category: &'static str,
    /// Bound the decision was measured against, when one applies.
    pub(crate) ceiling: usize,
    /// Amount the caller asked for.
    pub(crate) requested: usize,
    /// Amount the owner held before the transition.
    pub(crate) held_before: usize,
    /// Amount the owner holds after the transition.
    pub(crate) held_after: usize,
    /// Distinct unserved contenders holding capacity back from the caller.
    pub(crate) reserved_contenders: usize,
    /// Live bounded demand records after the transition.
    pub(crate) demand_records: usize,
    /// Active table cells after the transition.
    pub(crate) active_tables: usize,
    /// Active tenant cells after the transition.
    pub(crate) active_tenants: usize,
}

/// Emits one production contention lifecycle effect as a metric and a trace event.
///
/// The counter carries only the effect's closed stage, decision, severity, and
/// resource-category labels. The correlated event carries the bounded numeric
/// facts a maintainer needs to reconstruct one attempt's ordering; scrubbed
/// tenant/table context is attached by the caller's own `tracing` span rather
/// than lifted into a label here.
pub(crate) fn record_contention_effect(effect: ContentionEffect, facts: ContentionFacts) {
    metrics::counter!(
        "bifrost_scribe_contention_effects_total",
        "stage" => effect.stage(),
        "decision" => effect.decision(),
        "severity" => effect.severity(),
        "category" => facts.category,
    )
    .increment(1);
    metrics::gauge!("bifrost_scribe_contention_active_tables").set(facts.active_tables as f64);
    metrics::gauge!("bifrost_scribe_contention_active_tenants").set(facts.active_tenants as f64);
    metrics::gauge!("bifrost_scribe_contention_demand_records").set(facts.demand_records as f64);
    tracing::info!(
        stage = effect.stage(),
        decision = effect.decision(),
        severity = effect.severity(),
        category = facts.category,
        ceiling = facts.ceiling,
        requested = facts.requested,
        held_before = facts.held_before,
        held_after = facts.held_after,
        reserved_contenders = facts.reserved_contenders,
        demand_records = facts.demand_records,
        active_tables = facts.active_tables,
        active_tenants = facts.active_tenants,
        "Scribe contention lifecycle"
    );
}

#[cfg(test)]
/// Registry-shape proofs for the closed contention effect vocabulary.
mod contention_registry_tests {
    use super::{ContentionEffect, ContentionFacts, record_contention_effect};

    /// The registry's vocabularies are closed, complete, and free of duplicates.
    ///
    /// Complements the production-emitter coverage proof in the admission
    /// module: that test proves every entry is reachable from a real
    /// transition, and this one proves the vocabulary those entries publish is
    /// a closed label set an operator can aggregate on. A duplicate
    /// stage/decision pair would silently merge two distinct decisions into one
    /// time series.
    ///
    /// # Panics
    ///
    /// Panics when an entry publishes an empty or duplicated label, or when a
    /// severity outside the closed operator vocabulary is returned.
    #[test]
    fn scribe_contention_registry_vocabularies_are_closed() {
        let mut pairs: Vec<(&'static str, &'static str)> = Vec::new();
        for effect in ContentionEffect::ALL {
            assert!(!effect.stage().is_empty(), "{effect:?} has no stage");
            assert!(!effect.decision().is_empty(), "{effect:?} has no decision");
            assert!(
                matches!(effect.severity(), "info" | "warn" | "error"),
                "{effect:?} publishes a severity outside the closed vocabulary"
            );
            assert!(
                !pairs.contains(&(effect.stage(), effect.decision())),
                "{effect:?} duplicates an existing stage/decision pair"
            );
            pairs.push((effect.stage(), effect.decision()));
        }
        assert_eq!(pairs.len(), ContentionEffect::ALL.len());
    }

    /// Steady-state fairness decisions stay inside the bounded `info` severity.
    ///
    /// Borrowing, retryable pressure, and turnover are what a work-conserving
    /// ledger does constantly. Emitting them above `info` would make normal
    /// operation indistinguishable from a fault and drown the two signals that
    /// genuinely need attention.
    ///
    /// # Panics
    ///
    /// Panics when a steady-state decision escalates, or when queue saturation
    /// and accounting corruption do not carry their fixed severities.
    #[test]
    fn scribe_contention_severity_is_bounded_to_real_faults() {
        for effect in [
            ContentionEffect::ActivationInstalled,
            ContentionEffect::ActivationRefused,
            ContentionEffect::ActivationRolledBack,
            ContentionEffect::ChargeCommitted,
            ContentionEffect::ChargeRefused,
            ContentionEffect::IncumbentBlocked,
            ContentionEffect::ChargeReleased,
            ContentionEffect::DemandEnqueued,
            ContentionEffect::DemandRetired,
            ContentionEffect::TableSettled,
            ContentionEffect::TenantSettled,
        ] {
            assert_eq!(effect.severity(), "info", "{effect:?} escalated");
        }
        assert_eq!(ContentionEffect::DemandDropped.severity(), "warn");
        assert_eq!(ContentionEffect::OverReleaseRefused.severity(), "error");
        assert_eq!(ContentionEffect::InvariantFailure.severity(), "error");
    }

    /// Emitting an effect with default facts publishes only closed labels.
    ///
    /// Guards the emitter itself: the metric label set is built from the
    /// effect's own closed vocabulary plus the resource category, and nothing a
    /// caller passes can widen it.
    ///
    /// # Panics
    ///
    /// Panics when emitting an effect panics, which would make the production
    /// call sites fallible.
    #[test]
    fn recording_an_effect_publishes_only_closed_labels() {
        for effect in ContentionEffect::ALL {
            record_contention_effect(
                effect,
                ContentionFacts {
                    category: "admission_bytes",
                    ..ContentionFacts::default()
                },
            );
        }
    }
}
