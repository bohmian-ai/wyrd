//! Optional stage measurements for real Scribe workload runs.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use num_traits::ToPrimitive;

use crate::scribe::admission::AdmissionSnapshot;
use crate::scribe::geometry::ContentionCategory;
use crate::scribe::material_plan::IngestMaterialPlan;
use crate::scribe::memory::MEMORY_CATEGORY_COUNT;
use crate::scribe::seal_key::SealKey;

/// Declares the closed, compile-time Scribe hot-path effect registry.
macro_rules! scribe_effects {
    ($(#[$meta:meta] $variant:ident => ($stage:literal, $decision:literal, $terminal:expr, $delta:expr)),+ $(,)?) => {
        /// Every production Scribe lifecycle effect accepted by [`ScribeTelemetry`].
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub(crate) enum ScribeEffect {
            $(#[$meta] $variant),+
        }

        impl ScribeEffect {
            /// Complete registry in stable metric-index order.
            pub(crate) const ALL: [Self; scribe_effects!(@count $($variant),+)] = [
                $(Self::$variant),+
            ];

            /// Returns the stable registry index for this effect.
            pub(crate) fn index(self) -> usize {
                Self::ALL
                    .iter()
                    .position(|candidate| *candidate == self)
                    .expect("invariant: every Scribe effect is in the closed registry")
            }

            /// Returns the closed lifecycle stage label.
            pub(crate) const fn stage(self) -> &'static str {
                match self { $(Self::$variant => $stage),+ }
            }

            /// Returns the closed decision label.
            pub(crate) const fn decision(self) -> &'static str {
                match self { $(Self::$variant => $decision),+ }
            }

            /// Reports whether the effect terminates one active lifecycle.
            pub(crate) const fn is_terminal(self) -> bool {
                match self { $(Self::$variant => $terminal),+ }
            }

            /// Returns the effect's active-gauge movement.
            pub(crate) const fn active_delta(self) -> i8 {
                match self { $(Self::$variant => $delta),+ }
            }
        }
    };
    (@count $($variant:ident),+) => { <[()]>::len(&[$(scribe_effects!(@one $variant)),+]) };
    (@one $variant:ident) => { () };
}

scribe_effects! {
    /// Global admission attempt entered.
    AdmissionStarted => ("admission", "started", false, 1),
    /// Global admission attempt settled.
    AdmissionSettled => ("admission", "settled", true, -1),
    /// Tenant/table fair share changed.
    ShareDecided => ("admission", "share_decided", true, 0),
    /// Bounded demand entered or advanced in fairness order.
    DemandQueued => ("admission", "demand_queued", false, 0),
    /// Bounded demand left fairness order.
    DemandSettled => ("admission", "demand_settled", true, 0),
    /// One request was routed to its recorded shard.
    RouteDecided => ("route", "shard_selected", true, 0),
    /// One shard job entered a bounded queue.
    QueueEntered => ("queue", "entered", false, 1),
    /// One queued shard job began service.
    ScheduleStarted => ("schedule", "started", false, 0),
    /// One shard job reached a terminal outcome.
    ScheduleSettled => ("schedule", "settled", true, -1),
    /// One physical WAL append began.
    WalAppendStarted => ("wal_append", "started", false, 1),
    /// One physical WAL append completed.
    WalAppendSettled => ("wal_append", "settled", true, -1),
    /// One WAL fsync began.
    WalFsyncStarted => ("wal_fsync", "started", false, 1),
    /// One WAL fsync completed.
    WalFsyncSettled => ("wal_fsync", "settled", true, -1),
    /// Committed WAL coverage retired.
    WalRetired => ("wal_retire", "retired", true, 0),
    /// Startup WAL replay began.
    WalReplayStarted => ("wal_replay", "started", false, 1),
    /// Startup WAL replay completed or failed closed.
    WalReplaySettled => ("wal_replay", "settled", true, -1),
    /// An active generation rotated.
    GenerationRotated => ("rotation", "rotated", true, 0),
    /// An active generation froze immutably.
    GenerationFrozen => ("freeze", "frozen", true, 0),
    /// A sorted run became durable.
    RunDurable => ("run", "durable", true, 0),
    /// A staged manifest became durable.
    ManifestDurable => ("manifest", "durable", true, 0),
    /// Query authority moved to its next source.
    SourceTransitioned => ("source", "transitioned", true, 0),
    /// A staged member entered the ready set.
    MemberReady => ("ready", "member_ready", true, 0),
    /// An assembler claim began.
    ClaimStarted => ("claim", "started", false, 1),
    /// An assembler claim terminated.
    ClaimSettled => ("claim", "settled", true, -1),
    /// A merge pass completed.
    MergePassCompleted => ("merge", "pass_completed", true, 0),
    /// A Parquet row group flushed.
    RowGroupFlushed => ("row_group", "flushed", true, 0),
    /// A target-sized object closed.
    TargetObjectClosed => ("object_close", "target", true, 0),
    /// A terminal residue object closed.
    ResidueObjectClosed => ("object_close", "residue", true, 0),
    /// One object upload began.
    UploadStarted => ("upload", "started", false, 1),
    /// One object upload terminated.
    UploadSettled => ("upload", "settled", true, -1),
    /// One publication manifest became durable.
    PublicationManifestDurable => ("publication_manifest", "durable", true, 0),
    /// One fenced file-list transaction began.
    FileListCommitStarted => ("file_list", "started", false, 1),
    /// One fenced file-list transaction terminated.
    FileListCommitSettled => ("file_list", "settled", true, -1),
    /// Lease-drained cleanup began.
    CleanupStarted => ("cleanup", "started", false, 1),
    /// Lease-drained cleanup terminated.
    CleanupSettled => ("cleanup", "settled", true, -1),
    /// Cancellation began deterministic ownership settlement.
    CancellationStarted => ("cancellation", "started", false, 1),
    /// Cancellation settlement terminated.
    CancellationSettled => ("cancellation", "settled", true, -1),
    /// Process drain began.
    DrainStarted => ("drain", "started", false, 1),
    /// Process drain terminated.
    DrainSettled => ("drain", "settled", true, -1),
    /// Terminal resource accounting reconciled.
    ResourceSettled => ("settlement", "resource_settled", true, 0),
}

/// Closed terminal outcomes accepted by the top-level hot-path registry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ScribeEffectOutcome {
    /// The activity has acquired ownership and begun.
    Started,
    /// Work entered a bounded queue.
    Queued,
    /// A closed decision was observed without opening an activity.
    #[default]
    Observed,
    /// The activity completed successfully.
    Success,
    /// The activity failed while retaining recoverable ownership.
    Failed,
    /// Cancellation terminated the activity.
    Cancelled,
}

impl ScribeEffectOutcome {
    /// Returns the fixed-cardinality metric label.
    const fn label(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Queued => "queued",
            Self::Observed => "observed",
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Closed reasons accepted by the top-level hot-path registry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ScribeEffectReason {
    /// No narrower reason applies.
    #[default]
    None,
    /// Normal owner progress.
    Routine,
    /// Capacity was available immediately.
    CapacityAvailable,
    /// A fair tenant/table group owns the operation.
    FairGroup,
    /// Work left its bounded queue.
    Dequeued,
    /// Routing preserved the recorded shard.
    RecordedShard,
    /// A durable filesystem boundary completed.
    Fsynced,
    /// A durable manifest advanced.
    Advanced,
    /// A claim finished assembly.
    ClaimAssembled,
    /// The publication manifest is durable.
    ManifestDurable,
    /// Publication committed.
    Committed,
    /// Publication is durable and cleanup may start.
    Published,
    /// Physical evidence was revalidated.
    Verified,
    /// Upload failed closed.
    UploadRefused,
    /// Ownership remains available for retry or reconciliation.
    Retained,
    /// Terminal cleanup retired the owned artifacts.
    Retired,
    /// The generation committed and permits WAL retirement.
    GenerationCommitted,
    /// A sealed Parquet footer owns the observation.
    SealedFooter,
    /// Startup recovery owns the operation.
    Startup,
    /// Graceful shutdown owns the operation.
    Shutdown,
    /// The shutdown deadline elapsed.
    Deadline,
    /// Retained async owners were aborted.
    OwnersAborted,
    /// The process drained normally.
    Drained,
    /// A dropped future cancelled its owner.
    FutureDropped,
    /// Size triggered generation rotation.
    Size,
    /// Age triggered generation rotation.
    Age,
    /// Pressure triggered generation rotation.
    Pressure,
    /// An explicit request triggered generation rotation.
    Explicit,
    /// The configured target released an assembly claim.
    Target,
    /// A physical object reached its configured target.
    TargetReached,
    /// A claim exhausted its members below the physical target.
    ClaimExhausted,
    /// Maximum dwell released an assembly claim.
    Dwell,
    /// A closed partition released an assembly claim.
    PartitionClosed,
    /// Drain released an assembly claim.
    Drain,
    /// Recovery resumed an assembly claim.
    Recovery,
    /// A bounded contention decision was informational.
    ContentionInfo,
    /// A bounded contention decision reported pressure.
    ContentionWarn,
    /// A contention invariant failed.
    ContentionError,
}

impl ScribeEffectReason {
    /// Returns the fixed-cardinality metric label.
    const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Routine => "routine",
            Self::CapacityAvailable => "capacity_available",
            Self::FairGroup => "fair_group",
            Self::Dequeued => "dequeued",
            Self::RecordedShard => "recorded_shard",
            Self::Fsynced => "fsynced",
            Self::Advanced => "advanced",
            Self::ClaimAssembled => "claim_assembled",
            Self::ManifestDurable => "manifest_durable",
            Self::Committed => "committed",
            Self::Published => "published",
            Self::Verified => "verified",
            Self::UploadRefused => "upload_refused",
            Self::Retained => "retained",
            Self::Retired => "retired",
            Self::GenerationCommitted => "generation_committed",
            Self::SealedFooter => "sealed_footer",
            Self::Startup => "startup",
            Self::Shutdown => "shutdown",
            Self::Deadline => "deadline",
            Self::OwnersAborted => "owners_aborted",
            Self::Drained => "drained",
            Self::FutureDropped => "future_dropped",
            Self::Size => "size",
            Self::Age => "age",
            Self::Pressure => "pressure",
            Self::Explicit => "explicit",
            Self::Target => "target",
            Self::TargetReached => "target_reached",
            Self::ClaimExhausted => "claim_exhausted",
            Self::Dwell => "dwell",
            Self::PartitionClosed => "partition_closed",
            Self::Drain => "drain",
            Self::Recovery => "recovery",
            Self::ContentionInfo => "contention_info",
            Self::ContentionWarn => "contention_warn",
            Self::ContentionError => "contention_error",
        }
    }
}

/// Bounded numeric facts shared by the top-level hot-path registry.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ScribeEffectFacts {
    /// Rows affected by the transition.
    pub(crate) rows: u64,
    /// Bytes affected by the transition.
    pub(crate) bytes: u64,
    /// Objects affected by the transition.
    pub(crate) artifacts: u64,
    /// Closed outcome label.
    pub(crate) outcome: ScribeEffectOutcome,
    /// Closed reason label.
    pub(crate) reason: ScribeEffectReason,
}

/// RAII owner for one registered active Scribe lifecycle.
pub(crate) struct ScribeEffectGuard {
    /// Shared production telemetry owner.
    telemetry: Arc<ScribeTelemetry>,
    /// Required terminal paired with the already-emitted start.
    terminal: ScribeEffect,
    /// Bounded numeric facts retained through cancellation.
    facts: ScribeEffectFacts,
    /// Whether an explicit terminal already emitted.
    settled: bool,
}

/// Immutable top-level production telemetry totals for evidence capture.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScribeHotPathSnapshot {
    /// Counts keyed by the registry's stable `stage.decision` identity.
    pub effects: BTreeMap<String, u64>,
    /// Rows accumulated per registered effect identity.
    pub rows: BTreeMap<String, u64>,
    /// Bytes accumulated per registered effect identity.
    pub bytes: BTreeMap<String, u64>,
    /// Artifacts accumulated per registered effect identity.
    pub artifacts: BTreeMap<String, u64>,
    /// Current sum of every registered active-gauge movement.
    pub active: i64,
}

impl ScribeEffectGuard {
    /// Emits the activity's explicit terminal exactly once.
    pub(crate) fn settle(
        mut self,
        outcome: ScribeEffectOutcome,
        reason: ScribeEffectReason,
        artifacts: u64,
    ) {
        self.facts.outcome = outcome;
        self.facts.reason = reason;
        self.facts.artifacts = artifacts;
        self.telemetry.record_effect(self.terminal, self.facts);
        self.settled = true;
    }
}

impl Drop for ScribeEffectGuard {
    /// Balances a dropped future as cancellation without performing IO.
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        self.facts.outcome = ScribeEffectOutcome::Cancelled;
        self.facts.reason = ScribeEffectReason::FutureDropped;
        self.telemetry.record_effect(self.terminal, self.facts);
    }
}

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
/// This is the closed, compile-time enumerable registry that
/// [`ScribeTelemetry`] publishes and that the contention ledger and admission
/// controller emit through. Production code names a variant; it never invents a
/// stage, decision, or metric label at a call site. The registry is closed on
/// purpose: the operator vocabulary for admission fairness has to be stable
/// enough to alert on, and a free-form log string at one call site is exactly
/// how that vocabulary rots.
///
/// [`ContentionEffect::ALL`] is the inventory, and it is production state, not
/// a test fixture: [`ScribeTelemetry`] indexes its per-effect counters by
/// position in this list, so an entry missing from `ALL` cannot be counted and
/// an entry present in it with no emitter shows a permanent zero. Every entry
/// has one production emitter and every production transition maps to one
/// entry; the closure test in the admission module proves both directions by
/// driving the public surfaces and comparing what they emitted against `ALL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ContentionEffect {
    /// A request entered pod-global admission and holds an active transition.
    AdmissionAttempted,
    /// A request left pod-global admission and released its active transition.
    AdmissionSettled,
    /// A table's lifecycle vector cell was installed.
    ActivationInstalled,
    /// Activation was refused because the pod is at its derived table ceiling.
    ActivationRefused,
    /// An installed cell was removed again because its first charge refused.
    ActivationRolledBack,
    /// A max-min fair level was recomputed and bound the charging owner's growth.
    ShareRecomputed,
    /// A charge took idle capacity beyond the caller's own lifecycle quantum.
    CapacityBorrowed,
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
    /// A queued contender advanced but still holds less than its full quantum.
    DemandPartiallyServed,
    /// A contender received its complete category quantum and left the queue.
    DemandRetired,
    /// A contender withdrew every record it held.
    DemandCancelled,
    /// A demand record was dropped because the identity it spoke for is gone.
    DemandInvalidated,
    /// A rolled-back transition returned a demand record to its earned position.
    DemandRestored,
    /// A demand record stopped conferring priority because its TTL elapsed.
    DemandExpired,
    /// An empty table cell was retired by terminal settlement.
    TableSettled,
    /// A tenant cell was retired because its final table settled.
    TenantSettled,
    /// Terminal settlement left a table installed because it still holds capacity.
    SettlementDeferred,
    /// A reservation returned every resource it owned at every level.
    ResourceSettled,
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
    /// Projects the detailed contention decision into the top-level registry.
    const fn top_effect(self) -> ScribeEffect {
        match self {
            Self::AdmissionAttempted => ScribeEffect::AdmissionStarted,
            Self::AdmissionSettled => ScribeEffect::AdmissionSettled,
            Self::ShareRecomputed
            | Self::CapacityBorrowed
            | Self::ChargeCommitted
            | Self::ChargeRefused
            | Self::IncumbentBlocked
            | Self::ChargeReleased
            | Self::OverReleaseRefused
            | Self::ResizeGrown
            | Self::ResizeShrunk
            | Self::ResizeRefused => ScribeEffect::ShareDecided,
            Self::DemandEnqueued
            | Self::DemandRefreshed
            | Self::DemandPartiallyServed
            | Self::DemandRestored => ScribeEffect::DemandQueued,
            Self::DemandDropped
            | Self::DemandRetired
            | Self::DemandCancelled
            | Self::DemandInvalidated
            | Self::DemandExpired => ScribeEffect::DemandSettled,
            Self::ActivationInstalled
            | Self::ActivationRefused
            | Self::ActivationRolledBack
            | Self::TableSettled
            | Self::TenantSettled
            | Self::SettlementDeferred
            | Self::ResourceSettled
            | Self::InvariantFailure => ScribeEffect::ResourceSettled,
        }
    }

    /// The complete registry inventory, in lifecycle order.
    ///
    /// Indexed by [`Self::index`], so the order here is the order of
    /// [`ScribeTelemetry`]'s per-effect counters. Appending is safe; reordering
    /// silently re-labels historical counters.
    pub(crate) const ALL: [Self; 29] = [
        Self::AdmissionAttempted,
        Self::AdmissionSettled,
        Self::ActivationInstalled,
        Self::ActivationRefused,
        Self::ActivationRolledBack,
        Self::ShareRecomputed,
        Self::CapacityBorrowed,
        Self::ChargeCommitted,
        Self::ChargeRefused,
        Self::IncumbentBlocked,
        Self::ChargeReleased,
        Self::OverReleaseRefused,
        Self::DemandEnqueued,
        Self::DemandRefreshed,
        Self::DemandDropped,
        Self::DemandPartiallyServed,
        Self::DemandRetired,
        Self::DemandCancelled,
        Self::DemandInvalidated,
        Self::DemandRestored,
        Self::DemandExpired,
        Self::TableSettled,
        Self::TenantSettled,
        Self::SettlementDeferred,
        Self::ResourceSettled,
        Self::ResizeGrown,
        Self::ResizeShrunk,
        Self::ResizeRefused,
        Self::InvariantFailure,
    ];

    /// Returns this effect's position in [`Self::ALL`].
    ///
    /// The match is exhaustive, so a new variant cannot be added without being
    /// given a position here, and the registry test proves the positions are a
    /// bijection with `ALL`.
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::AdmissionAttempted => 0,
            Self::AdmissionSettled => 1,
            Self::ActivationInstalled => 2,
            Self::ActivationRefused => 3,
            Self::ActivationRolledBack => 4,
            Self::ShareRecomputed => 5,
            Self::CapacityBorrowed => 6,
            Self::ChargeCommitted => 7,
            Self::ChargeRefused => 8,
            Self::IncumbentBlocked => 9,
            Self::ChargeReleased => 10,
            Self::OverReleaseRefused => 11,
            Self::DemandEnqueued => 12,
            Self::DemandRefreshed => 13,
            Self::DemandDropped => 14,
            Self::DemandPartiallyServed => 15,
            Self::DemandRetired => 16,
            Self::DemandCancelled => 17,
            Self::DemandInvalidated => 18,
            Self::DemandRestored => 19,
            Self::DemandExpired => 20,
            Self::TableSettled => 21,
            Self::TenantSettled => 22,
            Self::SettlementDeferred => 23,
            Self::ResourceSettled => 24,
            Self::ResizeGrown => 25,
            Self::ResizeShrunk => 26,
            Self::ResizeRefused => 27,
            Self::InvariantFailure => 28,
        }
    }

    /// Returns the closed lifecycle stage this effect belongs to.
    ///
    /// Stages group the effects an operator reasons about together, so a
    /// dashboard can aggregate "everything that happened to the demand queue"
    /// without enumerating each decision.
    pub(crate) const fn stage(self) -> &'static str {
        match self {
            Self::AdmissionAttempted | Self::AdmissionSettled => "admission",
            Self::ActivationInstalled | Self::ActivationRefused | Self::ActivationRolledBack => {
                "activation"
            }
            Self::ShareRecomputed
            | Self::CapacityBorrowed
            | Self::ChargeCommitted
            | Self::ChargeRefused
            | Self::IncumbentBlocked
            | Self::ChargeReleased
            | Self::OverReleaseRefused => "charge",
            Self::DemandEnqueued
            | Self::DemandRefreshed
            | Self::DemandDropped
            | Self::DemandPartiallyServed
            | Self::DemandRetired
            | Self::DemandCancelled
            | Self::DemandInvalidated
            | Self::DemandRestored
            | Self::DemandExpired => "demand",
            Self::TableSettled
            | Self::TenantSettled
            | Self::SettlementDeferred
            | Self::ResourceSettled => "settlement",
            Self::ResizeGrown | Self::ResizeShrunk | Self::ResizeRefused => "resize",
            Self::InvariantFailure => "invariant",
        }
    }

    /// Returns the closed decision this effect records within its stage.
    pub(crate) const fn decision(self) -> &'static str {
        match self {
            Self::AdmissionAttempted => "attempted",
            Self::AdmissionSettled => "settled",
            Self::ActivationInstalled => "installed",
            Self::ActivationRefused => "ceiling_refused",
            Self::ActivationRolledBack => "rolled_back",
            Self::ShareRecomputed => "share_recomputed",
            Self::CapacityBorrowed => "borrowed",
            Self::ChargeCommitted => "committed",
            Self::ChargeRefused => "refused",
            Self::IncumbentBlocked => "contender_priority",
            Self::ChargeReleased => "released",
            Self::OverReleaseRefused => "over_release",
            Self::DemandEnqueued => "enqueued",
            Self::DemandRefreshed => "refreshed",
            Self::DemandDropped => "queue_full",
            Self::DemandPartiallyServed => "partially_served",
            Self::DemandRetired => "served",
            Self::DemandCancelled => "cancelled",
            Self::DemandInvalidated => "invalidated",
            Self::DemandRestored => "restored",
            Self::DemandExpired => "expired",
            Self::TableSettled => "table_retired",
            Self::TenantSettled => "tenant_retired",
            Self::SettlementDeferred => "nonempty",
            Self::ResourceSettled => "resources_returned",
            Self::ResizeGrown => "grown",
            Self::ResizeShrunk => "shrunk",
            Self::ResizeRefused => "delta_refused",
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

    /// Reports whether this effect opens an active transition.
    ///
    /// Starts and terminals must balance for the pod to be drained, so exactly
    /// one effect opens a transition and exactly one closes it.
    pub(crate) const fn is_start(self) -> bool {
        matches!(self, Self::AdmissionAttempted)
    }

    /// Reports whether this effect closes an active transition.
    pub(crate) const fn is_terminal(self) -> bool {
        matches!(self, Self::AdmissionSettled)
    }

    /// Reports whether this effect installs one complete lifecycle vector.
    pub(crate) const fn installs_vector(self) -> bool {
        matches!(self, Self::ActivationInstalled)
    }

    /// Reports whether this effect releases one complete lifecycle vector.
    ///
    /// A rolled-back install releases the vector it briefly held, so it counts
    /// here alongside the two settlement outcomes that retire a table.
    pub(crate) const fn releases_vector(self) -> bool {
        matches!(
            self,
            Self::ActivationRolledBack | Self::TableSettled | Self::TenantSettled
        )
    }

    /// Reports whether this effect is a transition of the bounded demand queue.
    pub(crate) const fn is_demand_transition(self) -> bool {
        matches!(
            self,
            Self::DemandEnqueued
                | Self::DemandRefreshed
                | Self::DemandDropped
                | Self::DemandPartiallyServed
                | Self::DemandRetired
                | Self::DemandCancelled
                | Self::DemandInvalidated
                | Self::DemandRestored
                | Self::DemandExpired
        )
    }
}

/// Bounded, identity-free facts attached to one contention lifecycle effect.
///
/// Every field here is a scalar or a closed enum the operator vocabulary
/// already covers. Tenant, table, request, batch, generation, path, row, SQL,
/// credential, and token values are deliberately absent: this struct is what
/// reaches the metric label set, and admitting one workload identity into it
/// would make the metric cardinality track request cardinality.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ContentionFacts {
    /// Governed category, or `None` when the effect spans every category.
    pub(crate) category: Option<ContentionCategory>,
    /// Bound the decision was measured against, when one applies.
    pub(crate) ceiling: usize,
    /// Amount the caller asked for.
    pub(crate) requested: usize,
    /// Amount the owner held before the transition.
    pub(crate) held_before: usize,
    /// Amount the owner holds after the transition.
    pub(crate) held_after: usize,
    /// Pod-wide committed amount in this category after the transition.
    pub(crate) pod_committed: usize,
    /// Distinct unserved contenders holding capacity back from the caller.
    pub(crate) reserved_contenders: usize,
    /// Live bounded demand records after the transition.
    pub(crate) demand_records: usize,
    /// Active table cells after the transition.
    pub(crate) active_tables: usize,
    /// Active tenant cells after the transition.
    pub(crate) active_tenants: usize,
}

impl ContentionFacts {
    /// Returns the closed metric label naming this effect's resource category.
    fn category_label(self) -> &'static str {
        self.category.map_or("all", ContentionCategory::label)
    }
}

/// Reconcilable totals one [`ScribeTelemetry`] has published since startup.
///
/// Exposed so a caller can prove the published metrics agree with the ledger's
/// own state rather than inferring a successful stage from a counter alone:
/// installed minus released vectors is the active table count, the demand gauge
/// is the live record count, and starts minus terminals is the number of
/// admission transitions still in flight.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScribeTelemetrySnapshot {
    /// Emission count per registry entry, indexed by [`ContentionEffect::index`].
    counts: [u64; ContentionEffect::ALL.len()],
    /// Admission transitions opened.
    starts: u64,
    /// Admission transitions closed.
    terminals: u64,
    /// Complete lifecycle vectors installed.
    vectors_installed: u64,
    /// Complete lifecycle vectors released.
    vectors_released: u64,
    /// Bounded demand-queue transitions of every kind.
    demand_transitions: u64,
}

impl ScribeTelemetrySnapshot {
    /// Returns how many times one registry entry was emitted.
    pub(crate) const fn count(&self, effect: ContentionEffect) -> u64 {
        self.counts[effect.index()]
    }

    /// Returns admission transitions opened since startup.
    pub const fn starts(&self) -> u64 {
        self.starts
    }

    /// Returns admission transitions closed since startup.
    pub const fn terminals(&self) -> u64 {
        self.terminals
    }

    /// Returns admission transitions still in flight.
    ///
    /// A drained pod publishes zero here; any other value names outstanding
    /// work, never an accounting leak on its own.
    pub const fn active_transitions(&self) -> u64 {
        self.starts.saturating_sub(self.terminals)
    }

    /// Returns lifecycle vectors installed and not yet released.
    ///
    /// Reconciles against the ledger's own active table count.
    pub const fn live_vectors(&self) -> u64 {
        self.vectors_installed.saturating_sub(self.vectors_released)
    }

    /// Returns every bounded demand-queue transition published so far.
    pub const fn demand_transitions(&self) -> u64 {
        self.demand_transitions
    }
}

/// Every staged-member and claim lifecycle effect Scribe can emit.
///
/// This is the second closed registry [`ScribeTelemetry`] publishes, and it
/// covers the durability half of the pod: a generation becoming a durable
/// staged member, the authority handover that makes those runs the live-tail
/// source, the claim that gathers members into one published object, and the
/// retirement that removes the runs the object replaced.
///
/// It is deliberately separate from [`ContentionEffect`] rather than folded
/// into it. Contention effects answer "who was allowed to own capacity"; these
/// answer "where do these rows live now". Their facts have no fields in common,
/// and merging them would give every emission a half-empty payload and an
/// operator vocabulary that means two different things per label.
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
    /// Projects the detailed staging decision into the top-level registry.
    const fn top_effect(self) -> ScribeEffect {
        match self {
            Self::MemberStaged => ScribeEffect::MemberReady,
            Self::SourceTransitioned => ScribeEffect::SourceTransitioned,
            Self::ClaimTaken => ScribeEffect::ClaimStarted,
            Self::ClaimAssembled => ScribeEffect::MergePassCompleted,
            Self::ClaimPublished => ScribeEffect::PublicationManifestDurable,
            Self::MemberRetired => ScribeEffect::ResourceSettled,
            Self::ClaimSettled | Self::ClaimFailed => ScribeEffect::ClaimSettled,
            Self::StagingRestored => ScribeEffect::ResourceSettled,
        }
    }

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
    pub(crate) cause: Option<crate::scribe::assembly::ClaimCause>,
}

impl StagingFacts {
    /// Returns the closed cause label, or the no-cause placeholder.
    const fn cause_label(&self) -> &'static str {
        match self.cause {
            Some(cause) => cause.label(),
            None => "none",
        }
    }

    /// Projects the closed claim cause into the top-level reason vocabulary.
    const fn effect_reason(&self) -> ScribeEffectReason {
        match self.cause {
            Some(crate::scribe::assembly::ClaimCause::Target) => ScribeEffectReason::Target,
            Some(crate::scribe::assembly::ClaimCause::Dwell) => ScribeEffectReason::Dwell,
            Some(crate::scribe::assembly::ClaimCause::PartitionClosed) => {
                ScribeEffectReason::PartitionClosed
            }
            Some(crate::scribe::assembly::ClaimCause::Pressure) => ScribeEffectReason::Pressure,
            Some(crate::scribe::assembly::ClaimCause::Drain) => ScribeEffectReason::Drain,
            Some(crate::scribe::assembly::ClaimCause::Recovery) => ScribeEffectReason::Recovery,
            None => ScribeEffectReason::Routine,
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

/// The pod's single production observation owner for Scribe admission fairness.
///
/// One instance lives beside the contention ledger it observes, and admission,
/// contention, and settlement code call its inherent methods rather than
/// scattering spans, log strings, or metric descriptors of their own. It owns
/// the reconcilable totals above, which is what lets a test prove a published
/// metric came from a real state transition instead of accepting a counter as
/// evidence of one.
///
/// Counters are `u64` under one lock rather than atomics because every emission
/// already happens inside a ledger transition that holds a lock, and one
/// consistent snapshot is worth more here than uncontended increments.
#[derive(Debug)]
pub(crate) struct ScribeTelemetry {
    /// Reconcilable totals published since startup.
    totals: Mutex<ScribeTelemetrySnapshot>,
    /// Reconcilable staged and claim totals published since startup.
    staging: Mutex<ScribeStagingSnapshot>,
    /// Complete hot-path effect counts in [`ScribeEffect::ALL`] order.
    effects: Mutex<[u64; ScribeEffect::ALL.len()]>,
    /// Row totals in [`ScribeEffect::ALL`] order.
    effect_rows: Mutex<[u64; ScribeEffect::ALL.len()]>,
    /// Byte totals in [`ScribeEffect::ALL`] order.
    effect_bytes: Mutex<[u64; ScribeEffect::ALL.len()]>,
    /// Artifact totals in [`ScribeEffect::ALL`] order.
    effect_artifacts: Mutex<[u64; ScribeEffect::ALL.len()]>,
    /// Sum of all registered active-gauge movements.
    active_effects: Mutex<i64>,
}

impl Default for ScribeTelemetry {
    /// Builds an owner whose totals start at zero.
    fn default() -> Self {
        Self {
            totals: Mutex::new(ScribeTelemetrySnapshot {
                counts: [0; ContentionEffect::ALL.len()],
                starts: 0,
                terminals: 0,
                vectors_installed: 0,
                vectors_released: 0,
                demand_transitions: 0,
            }),
            staging: Mutex::new(ScribeStagingSnapshot::default()),
            effects: Mutex::new([0; ScribeEffect::ALL.len()]),
            effect_rows: Mutex::new([0; ScribeEffect::ALL.len()]),
            effect_bytes: Mutex::new([0; ScribeEffect::ALL.len()]),
            effect_artifacts: Mutex::new([0; ScribeEffect::ALL.len()]),
            active_effects: Mutex::new(0),
        }
    }
}

impl ScribeTelemetry {
    /// Opens one RAII-balanced registered activity.
    pub(crate) fn start_effect(
        self: &Arc<Self>,
        start: ScribeEffect,
        terminal: ScribeEffect,
        facts: ScribeEffectFacts,
    ) -> ScribeEffectGuard {
        debug_assert_eq!(start.active_delta(), 1);
        debug_assert_eq!(terminal.active_delta(), -1);
        self.record_effect(start, facts);
        ScribeEffectGuard {
            telemetry: Arc::clone(self),
            terminal,
            facts,
            settled: false,
        }
    }

    /// Publishes one registered production hot-path effect.
    ///
    /// Only registry-owned stage and decision strings become labels. Workload
    /// identities stay on the caller's enclosing trace span.
    pub(crate) fn record_effect(&self, effect: ScribeEffect, facts: ScribeEffectFacts) {
        metrics::counter!(
            "bifrost_scribe_effects_total",
            "stage" => effect.stage(),
            "decision" => effect.decision(),
            "outcome" => facts.outcome.label(),
            "reason" => facts.reason.label(),
        )
        .increment(1);
        if let Ok(mut counts) = self.effects.lock() {
            counts[effect.index()] = counts[effect.index()].saturating_add(1);
        }
        if let Ok(mut rows) = self.effect_rows.lock() {
            rows[effect.index()] = rows[effect.index()].saturating_add(facts.rows);
        }
        if let Ok(mut bytes) = self.effect_bytes.lock() {
            bytes[effect.index()] = bytes[effect.index()].saturating_add(facts.bytes);
        }
        if let Ok(mut artifacts) = self.effect_artifacts.lock() {
            artifacts[effect.index()] = artifacts[effect.index()].saturating_add(facts.artifacts);
        }
        let active = if let Ok(mut active) = self.active_effects.lock() {
            *active = active.saturating_add(i64::from(effect.active_delta()));
            *active
        } else {
            0
        };
        metrics::gauge!("bifrost_scribe_effects_active").set(active as f64);
        tracing::info!(
            stage = effect.stage(),
            decision = effect.decision(),
            outcome = facts.outcome.label(),
            reason = facts.reason.label(),
            rows = facts.rows,
            bytes = facts.bytes,
            artifacts = facts.artifacts,
            active,
            terminal = effect.is_terminal(),
            "Scribe hot-path lifecycle"
        );
    }

    /// Returns the exact emission count for one registered effect.
    #[cfg(test)]
    pub(crate) fn effect_count(&self, effect: ScribeEffect) -> u64 {
        self.effects
            .lock()
            .map_or(0, |counts| counts[effect.index()])
    }

    /// Returns the current top-level active lifecycle balance.
    pub(crate) fn active_effects(&self) -> i64 {
        self.active_effects.lock().map_or(0, |active| *active)
    }

    /// Captures every registered effect, including zero-count entries.
    pub(crate) fn hot_path_snapshot(&self) -> ScribeHotPathSnapshot {
        let counts = self
            .effects
            .lock()
            .map_or_else(|_| [0; ScribeEffect::ALL.len()], |counts| *counts);
        let rows = self
            .effect_rows
            .lock()
            .map_or_else(|_| [0; ScribeEffect::ALL.len()], |rows| *rows);
        let bytes = self
            .effect_bytes
            .lock()
            .map_or_else(|_| [0; ScribeEffect::ALL.len()], |bytes| *bytes);
        let artifacts = self
            .effect_artifacts
            .lock()
            .map_or_else(|_| [0; ScribeEffect::ALL.len()], |artifacts| *artifacts);
        let keyed = |values: [u64; ScribeEffect::ALL.len()]| {
            ScribeEffect::ALL
                .into_iter()
                .map(|effect| {
                    (
                        format!("{}.{}", effect.stage(), effect.decision()),
                        values[effect.index()],
                    )
                })
                .collect()
        };
        ScribeHotPathSnapshot {
            effects: keyed(counts),
            rows: keyed(rows),
            bytes: keyed(bytes),
            artifacts: keyed(artifacts),
            active: self.active_effects(),
        }
    }

    /// Publishes one production effect as metrics, a trace event, and totals.
    ///
    /// The counter and gauge label sets are built from the effect's own closed
    /// vocabulary plus the closed resource category; nothing a caller passes can
    /// widen them. The correlated event carries the bounded numeric facts a
    /// maintainer needs to reconstruct one attempt's ordering, while scrubbed
    /// tenant and table context stays on the caller's own `tracing` span.
    ///
    /// A poisoned totals lock is not allowed to fail a production transition:
    /// the metrics and the event are still published and only the reconcilable
    /// totals stop advancing, because losing observation is strictly better than
    /// refusing admitted work.
    pub(crate) fn record(&self, effect: ContentionEffect, facts: ContentionFacts) {
        self.record_effect(
            effect.top_effect(),
            ScribeEffectFacts {
                bytes: u64::try_from(facts.requested).unwrap_or(u64::MAX),
                outcome: ScribeEffectOutcome::Observed,
                reason: match effect.severity() {
                    "warn" => ScribeEffectReason::ContentionWarn,
                    "error" => ScribeEffectReason::ContentionError,
                    _ => ScribeEffectReason::ContentionInfo,
                },
                ..ScribeEffectFacts::default()
            },
        );
        let category = facts.category_label();
        metrics::counter!(
            "bifrost_scribe_contention_effects_total",
            "stage" => effect.stage(),
            "decision" => effect.decision(),
            "severity" => effect.severity(),
            "category" => category,
        )
        .increment(1);
        if effect.is_demand_transition() {
            metrics::counter!(
                "bifrost_scribe_contention_demand_transitions_total",
                "decision" => effect.decision(),
            )
            .increment(1);
        }
        if effect.installs_vector() {
            metrics::counter!("bifrost_scribe_contention_vectors_installed_total").increment(1);
        }
        if effect.releases_vector() {
            metrics::counter!("bifrost_scribe_contention_vectors_released_total").increment(1);
        }
        if facts.category.is_some() {
            metrics::gauge!(
                "bifrost_scribe_contention_committed",
                "category" => category,
            )
            .set(gauge_value(facts.pod_committed));
        }
        metrics::gauge!("bifrost_scribe_contention_active_tables")
            .set(gauge_value(facts.active_tables));
        metrics::gauge!("bifrost_scribe_contention_active_tenants")
            .set(gauge_value(facts.active_tenants));
        metrics::gauge!("bifrost_scribe_contention_demand_records")
            .set(gauge_value(facts.demand_records));
        let totals = self.accumulate(effect);
        let active = totals.active_transitions();
        metrics::gauge!("bifrost_scribe_contention_active_transitions")
            .set(active.to_f64().unwrap_or(f64::MAX));
        metrics::gauge!("bifrost_scribe_contention_live_vectors")
            .set(totals.live_vectors().to_f64().unwrap_or(f64::MAX));
        metrics::gauge!("bifrost_scribe_contention_admissions_opened")
            .set(totals.starts().to_f64().unwrap_or(f64::MAX));
        metrics::gauge!("bifrost_scribe_contention_admissions_closed")
            .set(totals.terminals().to_f64().unwrap_or(f64::MAX));
        tracing::info!(
            stage = effect.stage(),
            decision = effect.decision(),
            severity = effect.severity(),
            category,
            ceiling = facts.ceiling,
            requested = facts.requested,
            held_before = facts.held_before,
            held_after = facts.held_after,
            pod_committed = facts.pod_committed,
            reserved_contenders = facts.reserved_contenders,
            demand_records = facts.demand_records,
            active_tables = facts.active_tables,
            active_tenants = facts.active_tenants,
            active_transitions = active,
            live_vectors = totals.live_vectors(),
            demand_transitions = totals.demand_transitions(),
            effect_total = totals.count(effect),
            "Scribe contention lifecycle"
        );
    }

    /// Advances the reconcilable totals and returns the view they now hold.
    ///
    /// The advance and the read happen under one acquisition so the published
    /// reconciliation metrics describe one consistent moment rather than two.
    /// A poisoned lock yields an all-zero view, which stops the totals advancing
    /// without failing the transition that was being observed.
    fn accumulate(&self, effect: ContentionEffect) -> ScribeTelemetrySnapshot {
        let Ok(mut totals) = self.totals.lock() else {
            return ScribeTelemetrySnapshot::default();
        };
        totals.counts[effect.index()] = totals.counts[effect.index()].saturating_add(1);
        if effect.is_start() {
            totals.starts = totals.starts.saturating_add(1);
        }
        if effect.is_terminal() {
            totals.terminals = totals.terminals.saturating_add(1);
        }
        if effect.installs_vector() {
            totals.vectors_installed = totals.vectors_installed.saturating_add(1);
        }
        if effect.releases_vector() {
            totals.vectors_released = totals.vectors_released.saturating_add(1);
        }
        if effect.is_demand_transition() {
            totals.demand_transitions = totals.demand_transitions.saturating_add(1);
        }
        *totals
    }

    /// Publishes one staged or claim lifecycle effect and its totals.
    ///
    /// The label set is built from the effect's own closed vocabulary plus the
    /// closed claim cause; nothing a caller passes can widen it. As with the
    /// contention registry, a poisoned totals lock stops the totals advancing
    /// but never fails the durable transition being observed.
    pub(crate) fn record_staging(&self, effect: StagingEffect, facts: StagingFacts) {
        self.record_effect(
            effect.top_effect(),
            ScribeEffectFacts {
                bytes: facts.bytes,
                artifacts: u64::try_from(facts.artifacts).unwrap_or(u64::MAX),
                outcome: if matches!(effect, StagingEffect::ClaimFailed) {
                    ScribeEffectOutcome::Failed
                } else if matches!(effect, StagingEffect::ClaimTaken) {
                    ScribeEffectOutcome::Started
                } else {
                    ScribeEffectOutcome::Success
                },
                reason: facts.effect_reason(),
                ..ScribeEffectFacts::default()
            },
        );
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

    /// Returns one consistent view of every reconcilable total.
    ///
    /// # Panics
    ///
    /// Panics when the totals lock is poisoned, which can only happen if a
    /// previous accumulation panicked inside this module.
    pub(crate) fn snapshot(&self) -> ScribeTelemetrySnapshot {
        *self.totals.lock().expect("Scribe telemetry totals lock")
    }
}

/// Converts one bounded count to the `f64` a gauge takes.
///
/// Saturating at `f64::MAX` rather than wrapping keeps an impossible count
/// visibly impossible instead of silently small.
fn gauge_value(count: usize) -> f64 {
    count.to_f64().unwrap_or(f64::MAX)
}

#[cfg(test)]
/// Registry-shape proofs for the closed contention effect vocabulary.
mod contention_registry_tests {
    use std::sync::Arc;

    use super::{
        ContentionEffect, ContentionFacts, ScribeEffect, ScribeEffectFacts, ScribeEffectOutcome,
        ScribeEffectReason, ScribeTelemetry,
    };
    use crate::scribe::geometry::ContentionCategory;
    use crate::scribe::telemetry::producer_lifecycle_tests::EventCaptureSubscriber;

    /// The registry's inventory is a bijection with its own index space.
    ///
    /// [`ScribeTelemetry`] indexes its per-effect counters by
    /// [`ContentionEffect::index`], so an entry missing from
    /// [`ContentionEffect::ALL`] would be counted into a slot nothing reads and
    /// a duplicated index would merge two decisions into one counter. The index
    /// match is exhaustive, so this pairing is what makes adding a variant
    /// impossible to get half-right.
    ///
    /// # Panics
    ///
    /// Panics when an index is out of range, duplicated, or unclaimed.
    #[test]
    fn scribe_contention_registry_inventory_is_complete() {
        let mut claimed = [false; ContentionEffect::ALL.len()];
        for effect in ContentionEffect::ALL {
            let index = effect.index();
            assert!(
                index < ContentionEffect::ALL.len(),
                "{effect:?} indexes past the registry"
            );
            assert!(!claimed[index], "{effect:?} duplicates index {index}");
            claimed[index] = true;
        }
        assert!(
            claimed.iter().all(|slot| *slot),
            "an index in the registry's counter space has no entry"
        );
    }

    /// The registry covers every effect this remediation is required to publish.
    ///
    /// Named rather than counted: a count would still pass if one required
    /// decision were dropped and an unrelated one added. Each entry here is an
    /// obligation from the remediation packet's telemetry contract.
    ///
    /// # Panics
    ///
    /// Panics when a required effect is absent from the registry.
    #[test]
    fn scribe_contention_registry_covers_every_required_effect() {
        for required in [
            ContentionEffect::AdmissionAttempted,
            ContentionEffect::AdmissionSettled,
            ContentionEffect::ActivationInstalled,
            ContentionEffect::ActivationRolledBack,
            ContentionEffect::ShareRecomputed,
            ContentionEffect::CapacityBorrowed,
            ContentionEffect::ChargeCommitted,
            ContentionEffect::ChargeRefused,
            ContentionEffect::ChargeReleased,
            ContentionEffect::IncumbentBlocked,
            ContentionEffect::DemandEnqueued,
            ContentionEffect::DemandRefreshed,
            ContentionEffect::DemandPartiallyServed,
            ContentionEffect::DemandRetired,
            ContentionEffect::DemandExpired,
            ContentionEffect::DemandCancelled,
            ContentionEffect::DemandInvalidated,
            ContentionEffect::DemandDropped,
            ContentionEffect::TableSettled,
            ContentionEffect::TenantSettled,
            ContentionEffect::ResourceSettled,
            ContentionEffect::ResizeGrown,
            ContentionEffect::ResizeShrunk,
            ContentionEffect::ResizeRefused,
            ContentionEffect::InvariantFailure,
        ] {
            assert!(
                ContentionEffect::ALL.contains(&required),
                "{required:?} is required by the telemetry contract but absent"
            );
        }
    }

    /// The registry's vocabularies are closed, complete, and free of duplicates.
    ///
    /// A duplicate stage/decision pair would silently merge two distinct
    /// decisions into one operator time series.
    ///
    /// # Panics
    ///
    /// Panics when an entry publishes an empty or duplicated label, or a
    /// severity outside the closed operator vocabulary.
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

    /// Exactly one effect opens an active transition and one closes it.
    ///
    /// Starts and terminals have to balance for a drained pod to publish zero
    /// active transitions. Two openers or two closers would make that balance
    /// depend on which path a request happened to take.
    ///
    /// # Panics
    ///
    /// Panics when the start, terminal, install, or release sets are not the
    /// single fixed entries the reconciliation depends on.
    #[test]
    fn scribe_contention_transitions_have_one_opener_and_one_closer() {
        let starts: Vec<_> = ContentionEffect::ALL
            .into_iter()
            .filter(|effect| effect.is_start())
            .collect();
        let terminals: Vec<_> = ContentionEffect::ALL
            .into_iter()
            .filter(|effect| effect.is_terminal())
            .collect();
        assert_eq!(starts, vec![ContentionEffect::AdmissionAttempted]);
        assert_eq!(terminals, vec![ContentionEffect::AdmissionSettled]);

        let installs: Vec<_> = ContentionEffect::ALL
            .into_iter()
            .filter(|effect| effect.installs_vector())
            .collect();
        assert_eq!(installs, vec![ContentionEffect::ActivationInstalled]);
        let releases: Vec<_> = ContentionEffect::ALL
            .into_iter()
            .filter(|effect| effect.releases_vector())
            .collect();
        assert_eq!(
            releases,
            vec![
                ContentionEffect::ActivationRolledBack,
                ContentionEffect::TableSettled,
                ContentionEffect::TenantSettled,
            ]
        );
    }

    /// Steady-state fairness decisions stay inside the bounded `info` severity.
    ///
    /// Borrowing, retryable pressure, and turnover are what a work-conserving
    /// ledger does constantly. Emitting them above `info` would make normal
    /// operation indistinguishable from a fault.
    ///
    /// # Panics
    ///
    /// Panics when a steady-state decision escalates, or when queue saturation
    /// and accounting corruption do not carry their fixed severities.
    #[test]
    fn scribe_contention_severity_is_bounded_to_real_faults() {
        for effect in [
            ContentionEffect::AdmissionAttempted,
            ContentionEffect::AdmissionSettled,
            ContentionEffect::ActivationInstalled,
            ContentionEffect::ActivationRefused,
            ContentionEffect::ActivationRolledBack,
            ContentionEffect::ShareRecomputed,
            ContentionEffect::CapacityBorrowed,
            ContentionEffect::ChargeCommitted,
            ContentionEffect::ChargeRefused,
            ContentionEffect::IncumbentBlocked,
            ContentionEffect::ChargeReleased,
            ContentionEffect::DemandEnqueued,
            ContentionEffect::DemandPartiallyServed,
            ContentionEffect::DemandRetired,
            ContentionEffect::DemandRestored,
            ContentionEffect::TableSettled,
            ContentionEffect::TenantSettled,
            ContentionEffect::ResourceSettled,
        ] {
            assert_eq!(effect.severity(), "info", "{effect:?} escalated");
        }
        assert_eq!(ContentionEffect::DemandDropped.severity(), "warn");
        assert_eq!(ContentionEffect::OverReleaseRefused.severity(), "error");
        assert_eq!(ContentionEffect::InvariantFailure.severity(), "error");
    }

    /// The owner's totals count exactly what it published, per entry.
    ///
    /// This is what makes a reconciliation assertion meaningful elsewhere: the
    /// totals are not a parallel bookkeeping of what the code intended, they are
    /// a tally of the emissions that actually happened.
    ///
    /// # Panics
    ///
    /// Panics when a published effect is not counted, or when the derived
    /// start/terminal, vector, and demand totals disagree with the emissions.
    #[test]
    fn scribe_telemetry_totals_tally_what_it_published() {
        let telemetry = ScribeTelemetry::default();
        for effect in ContentionEffect::ALL {
            telemetry.record(
                effect,
                ContentionFacts {
                    category: Some(ContentionCategory::AdmissionBytes),
                    ..ContentionFacts::default()
                },
            );
        }
        let snapshot = telemetry.snapshot();
        for effect in ContentionEffect::ALL {
            assert_eq!(snapshot.count(effect), 1, "{effect:?} was not counted");
        }
        assert_eq!(snapshot.starts(), 1);
        assert_eq!(snapshot.terminals(), 1);
        assert_eq!(snapshot.active_transitions(), 0);
        assert_eq!(snapshot.live_vectors(), 0, "one install, three releases");
        assert_eq!(
            snapshot.demand_transitions(),
            ContentionEffect::ALL
                .into_iter()
                .filter(|effect| effect.is_demand_transition())
                .count() as u64
        );
    }

    /// AC22/AC21 unit owner: the Scribe observation registry is closed and its
    /// transitions balance over a complete production lifecycle.
    ///
    /// Closed means an operator's label space cannot widen at runtime: every
    /// entry occupies a distinct index in the counter space, publishes a
    /// non-empty stage and decision, no two entries collapse onto the same
    /// stage/decision series, and severity comes from a fixed three-value
    /// vocabulary. Balanced means the counters reconcile rather than drift:
    /// exactly one entry opens an active transition and one closes it, and a
    /// pod driven through activation, charge, release, and terminal settlement
    /// publishes zero active transitions and zero live vectors at the end.
    ///
    /// The lifecycle half runs against the real ledger rather than direct
    /// emissions, because the property that matters is that the production
    /// paths emit in balanced pairs, not that the counters can be balanced.
    ///
    /// # Panics
    ///
    /// Panics when an entry duplicates an index or a label pair, publishes an
    /// unknown severity, when the opener/closer sets are not singletons, or
    /// when a drained ledger still reports live transitions or vectors.
    #[test]
    fn scribe_observation_registry_is_closed_and_balanced() {
        let mut top_claimed = vec![false; ScribeEffect::ALL.len()];
        let mut top_pairs = Vec::new();
        for effect in ScribeEffect::ALL {
            let index = effect.index();
            assert!(!top_claimed[index], "{effect:?} duplicates index {index}");
            top_claimed[index] = true;
            assert!(!effect.stage().is_empty(), "{effect:?} has no stage");
            assert!(!effect.decision().is_empty(), "{effect:?} has no decision");
            assert!(
                !top_pairs.contains(&(effect.stage(), effect.decision())),
                "{effect:?} collapses onto an existing top-level series"
            );
            top_pairs.push((effect.stage(), effect.decision()));
        }
        assert!(top_claimed.into_iter().all(|claimed| claimed));
        assert_eq!(
            ScribeEffect::ALL
                .into_iter()
                .map(ScribeEffect::active_delta)
                .map(i64::from)
                .sum::<i64>(),
            0,
            "the complete registry must define balanced active movements"
        );

        // Closed: index space, labels, and severity vocabulary.
        let mut claimed = [false; ContentionEffect::ALL.len()];
        let mut pairs: Vec<(&'static str, &'static str)> = Vec::new();
        for effect in ContentionEffect::ALL {
            let index = effect.index();
            assert!(
                index < ContentionEffect::ALL.len(),
                "{effect:?} indexes past the registry"
            );
            assert!(!claimed[index], "{effect:?} duplicates index {index}");
            claimed[index] = true;
            assert!(!effect.stage().is_empty(), "{effect:?} has no stage");
            assert!(!effect.decision().is_empty(), "{effect:?} has no decision");
            assert!(
                matches!(effect.severity(), "info" | "warn" | "error"),
                "{effect:?} publishes a severity outside the closed vocabulary"
            );
            assert!(
                !pairs.contains(&(effect.stage(), effect.decision())),
                "{effect:?} collapses onto an existing operator series"
            );
            pairs.push((effect.stage(), effect.decision()));
        }
        assert!(
            claimed.iter().all(|slot| *slot),
            "an index in the registry's counter space has no entry"
        );

        // Balanced: exactly one opener and one closer define the reconciliation.
        assert_eq!(
            ContentionEffect::ALL
                .into_iter()
                .filter(|effect| effect.is_start())
                .collect::<Vec<_>>(),
            vec![ContentionEffect::AdmissionAttempted]
        );
        assert_eq!(
            ContentionEffect::ALL
                .into_iter()
                .filter(|effect| effect.is_terminal())
                .collect::<Vec<_>>(),
            vec![ContentionEffect::AdmissionSettled]
        );

        // Balanced over a real lifecycle: a pod driven from activation through
        // terminal settlement publishes no live transition and no live vector.
        let admission = crate::scribe::admission::AdmissionController::with_config(
            crate::scribe::admission::AdmissionConfig::default(),
        )
        .expect("the default geometry fits the default budget");
        let ledger = admission.contention();
        let mut key_bytes = [31_u8; 16];
        key_bytes[6] = 0x70 | (key_bytes[6] & 0x0f);
        key_bytes[8] = 0x80 | (key_bytes[8] & 0x3f);
        let owner = crate::scribe::contention::ContentionKey::new(
            wyrd_spec::ids::DataTenantId::new(uuid::Uuid::from_bytes(key_bytes))
                .expect("UUIDv7 test tenant"),
            crate::catalog::TableRef::new(
                crate::namespaces::BifrostNamespace::Datasets,
                "observed",
            ),
        );
        for _ in 0..8 {
            admission
                .try_reserve_for_cell(&owner, "wyrd.observed", 4_096)
                .expect("the fixture pod admits a small request")
                .release()
                .expect("terminal release settles the cell");
        }
        let totals = ledger.telemetry_totals();
        assert_eq!(totals.starts(), 8, "every admitted request opens once");
        assert_eq!(totals.terminals(), 8, "every opened transition closes once");
        assert_eq!(
            totals.active_transitions(),
            0,
            "a drained pod publishes no live transition"
        );
        assert_eq!(
            totals.live_vectors(),
            0,
            "a drained pod holds no installed lifecycle vector"
        );
        assert_eq!(ledger.active_cells().expect("ledger readable"), 0);
        assert_eq!(ledger.telemetry().active_effects(), 0);
        assert_eq!(
            ledger
                .telemetry()
                .effect_count(ScribeEffect::AdmissionStarted),
            8
        );
        assert_eq!(
            ledger
                .telemetry()
                .effect_count(ScribeEffect::AdmissionSettled),
            8
        );
    }

    /// An effect with no category publishes the closed `all` label, not an identity.
    ///
    /// Guards the emitter itself: the label set is built from the effect's own
    /// closed vocabulary plus a closed [`ContentionCategory`], and nothing a
    /// caller passes can widen it.
    ///
    /// # Panics
    ///
    /// Panics when emitting an effect panics, which would make the production
    /// call sites fallible.
    #[test]
    fn recording_an_effect_publishes_only_closed_labels() {
        let telemetry = ScribeTelemetry::default();
        for category in ContentionCategory::ALL.map(Some).into_iter().chain([None]) {
            telemetry.record(
                ContentionEffect::ChargeCommitted,
                ContentionFacts {
                    category,
                    ..ContentionFacts::default()
                },
            );
        }
        assert_eq!(
            telemetry
                .snapshot()
                .count(ContentionEffect::ChargeCommitted),
            u64::try_from(ContentionCategory::ALL.len() + 1).expect("small count")
        );
    }

    /// The concrete metric recorder and trace subscriber observe only bounded labels.
    ///
    /// # Panics
    ///
    /// Panics when a workload identity reaches the metric key or when the trace
    /// event omits the registered lifecycle vocabulary.
    #[test]
    fn top_level_effect_recorder_and_trace_are_cardinality_closed() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let subscriber = EventCaptureSubscriber::default();
        let events = Arc::clone(&subscriber.events);
        let telemetry = ScribeTelemetry::default();
        metrics::with_local_recorder(&recorder, || {
            tracing::subscriber::with_default(subscriber, || {
                telemetry.record_effect(
                    ScribeEffect::RouteDecided,
                    ScribeEffectFacts {
                        rows: 3,
                        bytes: 4096,
                        artifacts: 1,
                        outcome: ScribeEffectOutcome::Success,
                        reason: ScribeEffectReason::RecordedShard,
                    },
                );
            });
        });

        let snapshot = recorder.snapshot();
        let key = snapshot
            .counters
            .keys()
            .find(|key| key.starts_with("bifrost_scribe_effects_total{"))
            .expect("top-level effect counter");
        for expected in [
            "stage=\"route\"",
            "decision=\"shard_selected\"",
            "outcome=\"success\"",
            "reason=\"recorded_shard\"",
        ] {
            assert!(key.contains(expected), "missing {expected} from {key}");
        }
        for forbidden in [
            "tenant",
            "table",
            "request",
            "batch",
            "member",
            "generation",
            "claim",
            "object",
            "node",
            "sql",
            "path",
        ] {
            assert!(
                !key.contains(forbidden),
                "identity label `{forbidden}` in {key}"
            );
        }

        let events = events.lock().expect("trace capture");
        let event = events.last().expect("top-level lifecycle trace event");
        let names = event
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        for required in ["stage", "decision", "outcome", "reason", "rows", "bytes"] {
            assert!(
                names.contains(&required),
                "trace omitted `{required}`: {event:?}"
            );
        }
        for forbidden in ["sql", "path", "credential", "token"] {
            assert!(!names.contains(&forbidden), "trace exposed `{forbidden}`");
        }
    }
}
