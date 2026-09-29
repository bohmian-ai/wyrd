//! Optional stage measurements for real Scribe workload runs.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use num_traits::ToPrimitive;

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
            state.planned_bytes = state
                .planned_bytes
                .saturating_add(plan.held_material_bytes());
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
    /// Memory totals distributed across the shard owners, indexed by shard.
    pub memory_by_shard: Vec<usize>,
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

    /// Builds one exact scalar plan whose held material is `held_bytes`.
    fn material_plan(held_bytes: usize) -> IngestMaterialPlan {
        let mut sources = [SourceMaterialPlan::default(); MAX_SOURCE_PLANS];
        sources[0].body_bytes = held_bytes;
        IngestMaterialPlan {
            path: IngestPath::Otlp,
            request_bytes: 16,
            aligned_copy_bytes: 0,
            native_schema_start: 0,
            native_schema_end: 0,
            native_schema_material_bytes: 0,
            native_metadata_scratch_bytes: 0,
            sources,
            source_count: 2,
            rows: 7,
            time_partition_count: 1,
            current_material_bytes: 32,
            active_output_bytes: 24,
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

/// Every staged-member and claim lifecycle effect Scribe can emit.
///
/// This is the closed registry [`ScribeTelemetry`] publishes. It covers the
/// durability half of the pod: a generation becoming a durable
/// staged member, the authority handover that makes those runs the live-tail
/// source, the claim that gathers members into one published object, and the
/// retirement that removes the runs the object replaced.
///
/// [`StagingEffect::ALL`] is the inventory and is production state: totals are
/// indexed by position in it, so an entry missing from `ALL` cannot be counted
/// and an entry with no production emitter shows a permanent zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum StagingEffect {
    /// A frozen generation became a durable, query-registered staged member.
    MemberStaged,
    /// One generation's authority moved forward to its next durable holder.
    SourceTransitioned,
    /// The assembler released one key's ready members as a claim.
    ClaimTaken,
    /// A claim's members merged into its sealed objects.
    ClaimAssembled,
    /// A claim's objects committed through the fenced `file_list` transaction.
    ClaimPublished,
    /// A published member's staged runs were removed after its readers drained.
    MemberRetired,
    /// A claim released its staged bytes and left the outstanding set.
    ClaimSettled,
    /// A claim could not publish and its members stayed durable and staged.
    ClaimFailed,
    /// Startup rebuilt the ready and claim indexes from durable evidence.
    StagingRestored,
}

impl StagingEffect {
    /// The complete registry inventory, in lifecycle order.
    ///
    /// Indexed by [`Self::index`], so the order here is the order of the
    /// per-effect staging totals. Appending is safe; reordering silently
    /// re-labels historical counters.
    pub(crate) const ALL: [Self; 9] = [
        Self::MemberStaged,
        Self::SourceTransitioned,
        Self::ClaimTaken,
        Self::ClaimAssembled,
        Self::ClaimPublished,
        Self::MemberRetired,
        Self::ClaimSettled,
        Self::ClaimFailed,
        Self::StagingRestored,
    ];

    /// Returns this effect's fixed position in [`Self::ALL`].
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::MemberStaged => 0,
            Self::SourceTransitioned => 1,
            Self::ClaimTaken => 2,
            Self::ClaimAssembled => 3,
            Self::ClaimPublished => 4,
            Self::MemberRetired => 5,
            Self::ClaimSettled => 6,
            Self::ClaimFailed => 7,
            Self::StagingRestored => 8,
        }
    }

    /// Returns the closed lifecycle stage this effect belongs to.
    pub(crate) const fn stage(self) -> &'static str {
        match self {
            Self::MemberStaged | Self::SourceTransitioned => "staged_source",
            Self::ClaimTaken | Self::ClaimAssembled => "assembly",
            Self::ClaimPublished => "publication",
            Self::MemberRetired | Self::ClaimSettled | Self::ClaimFailed => "settlement",
            Self::StagingRestored => "recovery",
        }
    }

    /// Returns the closed decision this effect records within its stage.
    pub(crate) const fn decision(self) -> &'static str {
        match self {
            Self::MemberStaged => "durable",
            Self::SourceTransitioned => "authority_moved",
            Self::ClaimTaken => "claimed",
            Self::ClaimAssembled => "merged",
            Self::ClaimPublished => "committed",
            Self::MemberRetired => "runs_removed",
            Self::ClaimSettled => "bytes_released",
            Self::ClaimFailed => "retained_staged",
            Self::StagingRestored => "reconciled",
        }
    }

    /// Returns the closed severity an operator signal is bounded to.
    ///
    /// A failed claim is `warn` and not `error`: its members stay durable and
    /// staged, so the outcome is a retry, not lost rows.
    pub(crate) const fn severity(self) -> &'static str {
        match self {
            Self::ClaimFailed => "warn",
            _ => "info",
        }
    }

    /// Reports whether this effect makes one more member durably staged.
    pub(crate) const fn stages_member(self) -> bool {
        matches!(self, Self::MemberStaged)
    }

    /// Reports whether this effect removes one staged member's runs.
    pub(crate) const fn retires_member(self) -> bool {
        matches!(self, Self::MemberRetired)
    }

    /// Reports whether this effect opens one outstanding claim.
    pub(crate) const fn opens_claim(self) -> bool {
        matches!(self, Self::ClaimTaken)
    }

    /// Reports whether this effect closes one outstanding claim.
    ///
    /// A failed claim closes its outstanding transition too: its members return
    /// to the ready index and a later claim takes them again, so counting it as
    /// still outstanding would make a healthy pod look permanently backed up.
    pub(crate) const fn closes_claim(self) -> bool {
        matches!(self, Self::ClaimSettled | Self::ClaimFailed)
    }
}

/// Bounded numeric facts describing one staged or claim lifecycle effect.
///
/// Every field is a count or a byte total. Tenant, table, node, and member
/// identities stay on the caller's own `tracing` span, and no path, row, or
/// query text ever reaches here.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StagingFacts {
    /// Staged members the effect concerns.
    pub(crate) members: usize,
    /// Encoded staged bytes the effect concerns.
    pub(crate) bytes: u64,
    /// Sealed objects the effect produced, when it produced any.
    pub(crate) artifacts: usize,
    /// Cause ordinal the assembler released a claim under, when one applies.
    pub(crate) cause: Option<&'static str>,
}

impl StagingFacts {
    /// Returns the closed cause label, or the no-cause placeholder.
    const fn cause_label(&self) -> &'static str {
        match self.cause {
            Some(cause) => cause,
            None => "none",
        }
    }
}

/// Reconcilable staged and claim totals one [`ScribeTelemetry`] has published.
///
/// Exposed so a caller can prove the published metrics agree with the durable
/// state rather than inferring a stage from a counter: staged minus retired is
/// the live member count, and claims taken minus claims closed is the number of
/// publications still in flight.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScribeStagingSnapshot {
    /// Emission count per registry entry, indexed by [`StagingEffect::index`].
    counts: [u64; StagingEffect::ALL.len()],
    /// Members made durable since startup.
    members_staged: u64,
    /// Members whose runs were removed since startup.
    members_retired: u64,
    /// Claims released by the assembler since startup.
    claims_taken: u64,
    /// Claims settled or failed since startup.
    claims_closed: u64,
}

impl ScribeStagingSnapshot {
    /// Returns how many times one registry entry was emitted.
    pub(crate) const fn count(&self, effect: StagingEffect) -> u64 {
        self.counts[effect.index()]
    }

    /// Returns staged members whose runs have not been retired.
    ///
    /// Reconciles against the staged namespace's own member count.
    pub const fn live_members(&self) -> u64 {
        self.members_staged.saturating_sub(self.members_retired)
    }

    /// Returns claims taken that have neither settled nor failed.
    ///
    /// A drained pod publishes zero here.
    pub const fn outstanding_claims(&self) -> u64 {
        self.claims_taken.saturating_sub(self.claims_closed)
    }
}

/// The pod's single production observation owner for Scribe staged lifecycle.
///
/// Persistence code calls its inherent methods rather than scattering spans,
/// log strings, or metric descriptors of its own. It owns the reconcilable
/// totals above, which is what lets a test prove a published metric came from
/// a real state transition instead of accepting a counter as evidence of one.
#[derive(Debug, Default)]
pub(crate) struct ScribeTelemetry {
    /// Reconcilable staged and claim totals published since startup.
    staging: Mutex<ScribeStagingSnapshot>,
}

impl ScribeTelemetry {
    /// Publishes one staged or claim lifecycle effect and its totals.
    ///
    /// The label set is built from the effect's own closed vocabulary plus the
    /// closed claim cause; nothing a caller passes can widen it. A poisoned
    /// totals lock stops the totals advancing but never fails the durable
    /// transition being observed.
    pub(crate) fn record_staging(&self, effect: StagingEffect, facts: StagingFacts) {
        metrics::counter!(
            "bifrost_scribe_staging_effects_total",
            "stage" => effect.stage(),
            "decision" => effect.decision(),
            "severity" => effect.severity(),
            "cause" => facts.cause_label(),
        )
        .increment(1);
        let totals = self.accumulate_staging(effect, &facts);
        metrics::gauge!("bifrost_scribe_staging_live_members")
            .set(totals.live_members().to_f64().unwrap_or(f64::MAX));
        metrics::gauge!("bifrost_scribe_staging_outstanding_claims")
            .set(totals.outstanding_claims().to_f64().unwrap_or(f64::MAX));
        tracing::info!(
            stage = effect.stage(),
            decision = effect.decision(),
            severity = effect.severity(),
            cause = facts.cause_label(),
            members = facts.members,
            bytes = facts.bytes,
            artifacts = facts.artifacts,
            live_members = totals.live_members(),
            outstanding_claims = totals.outstanding_claims(),
            effect_total = totals.count(effect),
            "Scribe staged lifecycle"
        );
    }

    /// Advances the staged totals and returns the view they now hold.
    ///
    /// A poisoned lock yields an all-zero view, which stops the totals
    /// advancing without failing the durable transition being observed.
    fn accumulate_staging(
        &self,
        effect: StagingEffect,
        facts: &StagingFacts,
    ) -> ScribeStagingSnapshot {
        let Ok(mut totals) = self.staging.lock() else {
            return ScribeStagingSnapshot::default();
        };
        totals.counts[effect.index()] = totals.counts[effect.index()].saturating_add(1);
        let members = facts.members as u64;
        if effect.stages_member() {
            totals.members_staged = totals.members_staged.saturating_add(members.max(1));
        }
        if effect.retires_member() {
            totals.members_retired = totals.members_retired.saturating_add(members.max(1));
        }
        if effect.opens_claim() {
            totals.claims_taken = totals.claims_taken.saturating_add(1);
        }
        if effect.closes_claim() {
            totals.claims_closed = totals.claims_closed.saturating_add(1);
        }
        *totals
    }

    /// Returns one consistent view of every reconcilable staged total.
    ///
    /// # Panics
    ///
    /// Panics when the staged totals lock is poisoned, which can only happen if
    /// a previous accumulation panicked inside this module.
    pub(crate) fn staging_snapshot(&self) -> ScribeStagingSnapshot {
        *self.staging.lock().expect("Scribe staging totals lock")
    }
}

#[cfg(test)]
/// Registry-shape proofs for the closed staging vocabulary.
mod staging_registry_tests {
    use super::{ScribeTelemetry, StagingEffect, StagingFacts};

    /// The staging registry is closed and its drained lifecycle balances.
    ///
    /// # Panics
    ///
    /// Panics when an entry duplicates an index or a label pair, publishes an
    /// unknown severity, when a predicate selects other than the transitions
    /// it names, or when a drained lifecycle still reports live members or
    /// claims.
    #[test]
    fn scribe_staging_registry_is_closed_and_balanced() {
        assert_staging_registry_is_closed_and_selective();
        assert_staged_lifecycle_balances();
    }

    /// Asserts the staging registry is a closed label space with exact predicates.
    ///
    /// The staging registry is the second closed label space and is held to the
    /// same rules as the contention one: an entry that shared an index would
    /// overwrite another entry's counter, and one that shared a stage/decision
    /// pair would fuse two lifecycle transitions into a single operator series.
    /// Its predicates are what move the reconcilable totals, so each must
    /// select exactly the entries whose durable transition it names — a widened
    /// predicate would count one member or claim twice, a narrowed one would
    /// strand it as permanently live.
    ///
    /// # Panics
    ///
    /// Panics when an index is not a bijection over the registry, when a stage
    /// or decision is empty or collapsed onto another entry, when a severity is
    /// outside the operator vocabulary, or when a predicate selects other than
    /// exactly the transitions it names.
    fn assert_staging_registry_is_closed_and_selective() {
        // The staging registry is the second closed label space and is held to
        // the same rules: an entry that shared an index would overwrite another
        // entry's counter, and one that shared a stage/decision pair would fuse
        // two lifecycle transitions into a single operator series.
        let mut staging_claimed = [false; StagingEffect::ALL.len()];
        let mut staging_pairs: Vec<(&'static str, &'static str)> = Vec::new();
        for effect in StagingEffect::ALL {
            let index = effect.index();
            assert!(
                index < StagingEffect::ALL.len(),
                "{effect:?} indexes past the staging registry"
            );
            assert!(
                !staging_claimed[index],
                "{effect:?} duplicates staging index {index}"
            );
            staging_claimed[index] = true;
            assert!(!effect.stage().is_empty(), "{effect:?} has no stage");
            assert!(!effect.decision().is_empty(), "{effect:?} has no decision");
            assert!(
                matches!(effect.severity(), "info" | "warn" | "error"),
                "{effect:?} publishes a severity outside the closed vocabulary"
            );
            assert!(
                !staging_pairs.contains(&(effect.stage(), effect.decision())),
                "{effect:?} collapses onto an existing staging series"
            );
            staging_pairs.push((effect.stage(), effect.decision()));
        }
        assert!(
            staging_claimed.iter().all(|slot| *slot),
            "an index in the staging registry's counter space has no entry"
        );

        // The staging predicates are what move the reconcilable totals, so each
        // must select exactly the entries whose durable transition it names. A
        // widened predicate would count one member or claim twice; a narrowed
        // one would strand it as permanently live.
        let staging_selecting = |predicate: fn(StagingEffect) -> bool| {
            StagingEffect::ALL
                .into_iter()
                .filter(|effect| predicate(*effect))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            staging_selecting(StagingEffect::stages_member),
            vec![StagingEffect::MemberStaged]
        );
        assert_eq!(
            staging_selecting(StagingEffect::retires_member),
            vec![StagingEffect::MemberRetired]
        );
        assert_eq!(
            staging_selecting(StagingEffect::opens_claim),
            vec![StagingEffect::ClaimTaken]
        );
        assert_eq!(
            staging_selecting(StagingEffect::closes_claim),
            vec![StagingEffect::ClaimSettled, StagingEffect::ClaimFailed],
            "a failed claim closes its outstanding transition alongside a settled one"
        );
    }

    /// Asserts a complete staged lifecycle leaves nothing live or outstanding.
    ///
    /// Three members become durable, one claim gathers and publishes them,
    /// their runs are retired and the claim settles. Live members and
    /// outstanding claims are derived differences, so a stage that counted a
    /// member twice or never retired it would leave a permanent nonzero here
    /// rather than a transient one. A claim that could not publish closes the
    /// same way: its members stay durable and staged for a later claim, so
    /// leaving it outstanding would make a recovering pod look permanently
    /// backed up.
    ///
    /// # Panics
    ///
    /// Panics when a completed lifecycle leaves a live member or an outstanding
    /// claim, or when any transition is observed other than exactly once.
    fn assert_staged_lifecycle_balances() {
        // Balanced over the staged lifecycle: three members become durable, one
        // claim gathers and publishes them, their runs are retired and the claim
        // settles. Live members and outstanding claims are derived differences,
        // so a stage that counted a member twice or never retired it would leave
        // a permanent nonzero here rather than a transient one.
        let staged = ScribeTelemetry::default();
        let members = 3;
        staged.record_staging(
            StagingEffect::MemberStaged,
            StagingFacts {
                members,
                bytes: 3_072,
                ..StagingFacts::default()
            },
        );
        assert_eq!(
            staged.staging_snapshot().live_members(),
            members as u64,
            "a staged member is live until its runs are removed"
        );
        for effect in [
            StagingEffect::SourceTransitioned,
            StagingEffect::ClaimTaken,
            StagingEffect::ClaimAssembled,
            StagingEffect::ClaimPublished,
        ] {
            staged.record_staging(
                effect,
                StagingFacts {
                    members,
                    bytes: 3_072,
                    artifacts: 1,
                    cause: Some("target"),
                },
            );
        }
        assert_eq!(
            staged.staging_snapshot().outstanding_claims(),
            1,
            "a taken claim is outstanding until it settles or fails"
        );
        staged.record_staging(
            StagingEffect::MemberRetired,
            StagingFacts {
                members,
                bytes: 3_072,
                ..StagingFacts::default()
            },
        );
        staged.record_staging(
            StagingEffect::ClaimSettled,
            StagingFacts {
                members,
                bytes: 3_072,
                cause: Some("target"),
                ..StagingFacts::default()
            },
        );
        let settled = staged.staging_snapshot();
        assert_eq!(
            settled.live_members(),
            0,
            "a fully retired lineage leaves no live staged member"
        );
        assert_eq!(
            settled.outstanding_claims(),
            0,
            "a settled claim leaves no outstanding publication"
        );
        for effect in [
            StagingEffect::MemberStaged,
            StagingEffect::ClaimTaken,
            StagingEffect::ClaimPublished,
            StagingEffect::ClaimSettled,
        ] {
            assert_eq!(settled.count(effect), 1, "{effect:?} was not counted once");
        }

        // A claim that could not publish also closes: its members stay durable
        // and staged for a later claim, so leaving it outstanding would make a
        // recovering pod look permanently backed up.
        let failing = ScribeTelemetry::default();
        failing.record_staging(StagingEffect::ClaimTaken, StagingFacts::default());
        failing.record_staging(StagingEffect::ClaimFailed, StagingFacts::default());
        assert_eq!(
            failing.staging_snapshot().outstanding_claims(),
            0,
            "a failed claim closes its outstanding transition"
        );
        assert_eq!(
            failing.staging_snapshot().live_members(),
            0,
            "a failed claim retires nothing and stages nothing"
        );
    }
}
