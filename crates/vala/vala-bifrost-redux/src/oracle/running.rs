//! Tenant-scoped, in-memory ownership for active Oracle query lifecycles.
//!
//! The registry contains only control-plane lifecycle facts. Query text,
//! parameters, Arrow batches, and result rows stay with execution owners.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use tokio_util::sync::CancellationToken;
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    CancelRunningQueryResponse, QueryClass, QueryTerminalOutcome, RunningQueryLifecycleState,
    RunningQueryProgress, RunningQuerySummary,
};

use super::participant_cut::OracleQueryAttemptCut;

/// Immutable facts and shared cancellation control retained for one active query.
#[derive(Clone)]
pub struct RunningQueryEntry {
    /// Tenant that owns the request and scopes every registry lookup.
    tenant_id: DataTenantId,
    /// Public request identity; it is the sole live-control identifier.
    request_id: RequestId,
    /// Server-derived execution class.
    query_class: QueryClass,
    /// Wall-clock time at which local admission completed.
    started_at: DateTime<Utc>,
    /// Immutable selected membership, role fences, and deadline.
    participant_cut: Arc<OracleQueryAttemptCut>,
    /// Cooperative cancellation signal shared with execution owners.
    cancellation: CancellationToken,
}

/// Exactly-once terminal settlement removed from the active-query registry.
///
/// The value binds the immutable admitted entry to the terminal classification
/// chosen by the lifecycle owner. It is returned to that owner for resource,
/// audit, and follower settlement after the active state has been removed.
#[derive(Debug)]
pub struct RunningQuerySettlement {
    /// Immutable admitted entry whose active lifecycle ended.
    entry: RunningQueryEntry,
    /// Final classification fixed at the registry removal boundary.
    outcome: QueryTerminalOutcome,
}

impl RunningQuerySettlement {
    /// Returns the immutable admitted entry associated with this settlement.
    #[must_use]
    pub const fn entry(&self) -> &RunningQueryEntry {
        &self.entry
    }

    /// Consumes the settlement and returns its immutable admitted entry.
    #[must_use]
    pub fn into_entry(self) -> RunningQueryEntry {
        self.entry
    }

    /// Returns the terminal classification fixed by this settlement.
    #[must_use]
    pub const fn outcome(&self) -> QueryTerminalOutcome {
        self.outcome
    }
}

impl std::fmt::Debug for RunningQueryEntry {
    /// Formats lifecycle identity without exposing any execution payload.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RunningQueryEntry")
            .field("tenant_id", &self.tenant_id)
            .field("request_id", &self.request_id)
            .field("query_class", &self.query_class)
            .field("started_at", &self.started_at)
            .field("cut_fingerprint", &self.participant_cut.fingerprint())
            .finish_non_exhaustive()
    }
}

impl RunningQueryEntry {
    /// Constructs immutable lifecycle facts for one locally admitted request.
    #[must_use]
    pub fn new(
        tenant_id: DataTenantId,
        request_id: RequestId,
        query_class: QueryClass,
        started_at: DateTime<Utc>,
        participant_cut: OracleQueryAttemptCut,
    ) -> Self {
        Self {
            tenant_id,
            request_id,
            query_class,
            started_at,
            participant_cut: Arc::new(participant_cut),
            cancellation: CancellationToken::new(),
        }
    }

    /// Constructs lifecycle facts using the admitted query's exact cancellation token.
    #[must_use]
    pub fn with_cancellation(
        tenant_id: DataTenantId,
        request_id: RequestId,
        query_class: QueryClass,
        started_at: DateTime<Utc>,
        participant_cut: OracleQueryAttemptCut,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            tenant_id,
            request_id,
            query_class,
            started_at,
            participant_cut: Arc::new(participant_cut),
            cancellation,
        }
    }

    /// Returns the tenant that owns this entry.
    #[must_use]
    pub const fn tenant_id(&self) -> DataTenantId {
        self.tenant_id
    }

    /// Returns the public request identity for this lifecycle.
    #[must_use]
    pub fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    /// Returns the immutable participant cut selected at admission.
    #[must_use]
    pub fn participant_cut(&self) -> &OracleQueryAttemptCut {
        &self.participant_cut
    }

    /// Returns a clone of the cooperative cancellation signal.
    #[must_use]
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }
}

/// Mutable lifecycle facts held under the registry's single synchronization boundary.
struct RunningQueryState {
    /// Immutable identity and execution control facts.
    entry: RunningQueryEntry,
    /// Current active lifecycle state.
    lifecycle: RunningQueryLifecycleState,
    /// Number of participants that have reported completion.
    completed_participants: u32,
    /// Whether an idempotent cancellation request has been accepted.
    cancellation_requested: bool,
    /// Terminal classification fixed immediately before exactly-once removal.
    terminal_outcome: Option<QueryTerminalOutcome>,
}

impl RunningQueryState {
    /// Projects the state into its SQL-free public summary.
    fn summary(&self) -> RunningQuerySummary {
        RunningQuerySummary {
            request_id: self.entry.request_id.clone(),
            query_class: self.entry.query_class,
            started_at: self.entry.started_at,
            deadline: self.entry.participant_cut.deadline(),
            state: self.lifecycle,
            progress: RunningQueryProgress {
                completed_participants: self.completed_participants,
                total_participants: self.entry.participant_cut.participant_count(),
            },
            cancellation_requested: self.cancellation_requested,
        }
    }
}

/// In-memory, tenant-scoped owner of every currently running Oracle query.
///
/// One mutex protects insertion, cancellation, progress, and terminal removal
/// so concurrent terminal and cancel operations have deterministic idempotent
/// outcomes. Entries are removed at terminal completion; this is not a durable
/// query-job registry.
#[derive(Default)]
pub struct RunningQueryRegistry {
    /// Active states keyed by their tenant-qualified public request identity.
    entries: Mutex<HashMap<(DataTenantId, RequestId), RunningQueryState>>,
}

impl std::fmt::Debug for RunningQueryRegistry {
    /// Formats only the current entry count and never locks caller-visible data.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        formatter
            .debug_struct("RunningQueryRegistry")
            .field("entry_count", &entries.len())
            .finish()
    }
}

impl RunningQueryRegistry {
    /// Creates an empty active-query registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts one admitted query unless its tenant-qualified request already exists.
    ///
    /// Returns `true` only when this call installed the entry. Existing entries
    /// are never replaced, preserving their immutable participant cut.
    pub fn insert(&self, entry: RunningQueryEntry) -> bool {
        let key = (entry.tenant_id, entry.request_id.clone());
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if entries.contains_key(&key) {
            return false;
        }
        entries.insert(
            key,
            RunningQueryState {
                entry,
                lifecycle: RunningQueryLifecycleState::Admitted,
                completed_participants: 0,
                cancellation_requested: false,
                terminal_outcome: None,
            },
        );
        true
    }

    /// Returns one tenant-visible active query entry without exposing another tenant.
    #[must_use]
    pub fn get(
        &self,
        tenant_id: DataTenantId,
        request_id: &RequestId,
    ) -> Option<RunningQueryEntry> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&(tenant_id, request_id.clone()))
            .map(|state| state.entry.clone())
    }

    /// Lists active query summaries for exactly one tenant in request-ID order.
    #[must_use]
    pub fn list(&self, tenant_id: DataTenantId) -> Vec<RunningQuerySummary> {
        let mut summaries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .filter(|((entry_tenant, _), _)| *entry_tenant == tenant_id)
            .map(|(_, state)| state.summary())
            .collect::<Vec<_>>();
        summaries.sort_by(|left, right| left.request_id.as_str().cmp(right.request_id.as_str()));
        summaries
    }

    /// Marks one tenant-visible query for cancellation exactly once.
    ///
    /// Returns `None` when the request is absent in the named tenant, avoiding
    /// a cross-tenant existence signal. Repeated calls return `Some` with
    /// `cancellation_started` set to `false`.
    pub fn cancel(
        &self,
        tenant_id: DataTenantId,
        request_id: &RequestId,
    ) -> Option<CancelRunningQueryResponse> {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let state = entries.get_mut(&(tenant_id, request_id.clone()))?;
        let cancellation_started = !state.cancellation_requested;
        if cancellation_started {
            state.cancellation_requested = true;
            state.lifecycle = RunningQueryLifecycleState::Cancelling;
            state.entry.cancellation.cancel();
        }
        Some(CancelRunningQueryResponse {
            request_id: request_id.clone(),
            cancellation_started,
        })
    }

    /// Updates aggregate participant progress for one active tenant-qualified query.
    ///
    /// Returns `false` when the entry is absent or the supplied progress exceeds
    /// the immutable participant count.
    pub fn update_progress(
        &self,
        tenant_id: DataTenantId,
        request_id: &RequestId,
        completed_participants: u32,
    ) -> bool {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(state) = entries.get_mut(&(tenant_id, request_id.clone())) else {
            return false;
        };
        if completed_participants < state.completed_participants
            || completed_participants > state.entry.participant_cut.participant_count()
        {
            return false;
        }
        state.completed_participants = completed_participants;
        if !state.cancellation_requested {
            state.lifecycle = RunningQueryLifecycleState::Running;
        }
        true
    }

    /// Settles and removes one terminal entry for its exact tenant-qualified identity.
    ///
    /// The registry records the classification while holding the same lock used
    /// by cancellation and progress, then returns the removed state exactly
    /// once. Later terminal or cancellation calls are indistinguishable from an
    /// absent entry and cannot alter the chosen outcome.
    pub fn settle_terminal(
        &self,
        tenant_id: DataTenantId,
        request_id: &RequestId,
        outcome: QueryTerminalOutcome,
    ) -> Option<RunningQuerySettlement> {
        let mut state = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&(tenant_id, request_id.clone()))?;
        state.terminal_outcome = Some(outcome);
        state
            .terminal_outcome
            .map(|outcome| RunningQuerySettlement {
                entry: state.entry,
                outcome,
            })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use chrono::Utc;
    use wyrd_spec::vala::api::{
        ClusterCapabilities, ClusterNodeKey, ClusterRole, ClusterRoleLease, NodeId,
        OracleCapabilitiesV1, ScribeCapabilitiesV1,
    };

    use super::*;
    use crate::cluster::ClusterSnapshot;
    use crate::oracle::participant_cut::tests::fingerprint_binds_every_immutable_participant_cut_fact;

    /// Builds one ready lease suitable for a deterministic participant cut.
    fn lease(node: u128, role: ClusterRole) -> ClusterRoleLease {
        let now = Utc::now();
        ClusterRoleLease {
            key: ClusterNodeKey {
                node_id: NodeId::new(uuid::Uuid::from_u128(node)),
                role,
            },
            address: format!("http://node-{node}"),
            fencing_token: 1,
            capability_version: 1,
            capabilities: match role {
                ClusterRole::Oracle => ClusterCapabilities::OracleV1(OracleCapabilitiesV1 {
                    peer_protocol_version: 1,
                    storage_protocol_version: 1,
                    cpu_cores: 1.0,
                    memory_budget_bytes: 1024,
                    cpu_cores_per_slot: 1.0,
                    memory_bytes_per_slot: 1024,
                    raw_slots: 1,
                    usable_slots: 1,
                    supported_classes: vec![QueryClass::Interactive],
                    max_workers_per_query: 1,
                }),
                ClusterRole::Scribe => ClusterCapabilities::ScribeV1(ScribeCapabilitiesV1 {
                    tail_protocol_version: 1,
                }),
            },
            ready: true,
            started_at: now,
            heartbeat_at: now,
        }
    }

    /// Creates one entry with an Oracle and Scribe participant.
    fn entry(tenant_id: DataTenantId, request_id: RequestId) -> RunningQueryEntry {
        let now = Utc::now();
        let oracle = lease(1, ClusterRole::Oracle);
        let cut = OracleQueryAttemptCut::try_from_snapshot(
            &ClusterSnapshot::observed(vec![oracle.clone(), lease(2, ClusterRole::Scribe)], now),
            wyrd_spec::vala::api::QueryId::new(uuid::Uuid::now_v7()),
            oracle.key.node_id,
            QueryClass::Interactive,
            now + chrono::Duration::seconds(10),
            now,
            std::time::Duration::from_secs(15),
        )
        .expect("invariant: valid registry fixture constructs a cut");
        RunningQueryEntry::new(tenant_id, request_id, QueryClass::Interactive, now, cut)
    }

    /// Cancellation and failed settlement retain the exact admitted cut without refresh.
    #[test]
    fn immutable_cut_survives_cancel_and_failed_settlement() {
        let registry = RunningQueryRegistry::new();
        let tenant_id = DataTenantId::new_v7();
        let request_id = RequestId::now_v7();
        let admitted = entry(tenant_id, request_id.clone());
        let fingerprint = admitted.participant_cut().fingerprint();
        let deadline = admitted.participant_cut().deadline();
        let participants = admitted.participant_cut().participant_count();
        assert!(registry.insert(admitted));

        let cancelled = registry
            .cancel(tenant_id, &request_id)
            .expect("the admitted request remains cancellable");
        assert!(cancelled.cancellation_started);
        let cancelling = registry
            .get(tenant_id, &request_id)
            .expect("cancellation retains the running owner until settlement");
        assert_eq!(cancelling.participant_cut().fingerprint(), fingerprint);
        assert_eq!(cancelling.participant_cut().deadline(), deadline);
        assert_eq!(
            cancelling.participant_cut().participant_count(),
            participants
        );

        let settled = registry
            .settle_terminal(tenant_id, &request_id, QueryTerminalOutcome::Failed)
            .expect("failed recovery settles the same retained owner once");
        assert_eq!(settled.entry().participant_cut().fingerprint(), fingerprint);
        assert_eq!(settled.entry().participant_cut().deadline(), deadline);
        assert_eq!(
            settled.entry().participant_cut().participant_count(),
            participants
        );
        assert_eq!(settled.outcome(), QueryTerminalOutcome::Failed);
        assert!(registry.get(tenant_id, &request_id).is_none());
    }

    /// Proves every tenant-scoped lifecycle transition and terminal race invariant.
    #[test]
    fn tenant_scoped_registry_lifecycle_is_race_safe() {
        fingerprint_binds_every_immutable_participant_cut_fact();
        let registry = Arc::new(RunningQueryRegistry::new());
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let request_id = RequestId::now_v7();
        let second_request_id = RequestId::now_v7();
        let admitted = entry(tenant_a, request_id.clone());
        let cancellation = admitted.cancellation_token();
        assert!(registry.insert(admitted.clone()));
        assert!(!registry.insert(admitted));
        assert_eq!(
            registry
                .get(tenant_a, &request_id)
                .map(|entry| entry.request_id().clone()),
            Some(request_id.clone())
        );
        assert!(registry.insert(entry(tenant_a, second_request_id.clone())));
        assert!(registry.get(tenant_b, &request_id).is_none());
        assert!(registry.list(tenant_b).is_empty());
        assert!(registry.cancel(tenant_b, &request_id).is_none());
        assert!(
            registry
                .settle_terminal(tenant_b, &request_id, QueryTerminalOutcome::Failed)
                .is_none()
        );

        let listed = registry.list(tenant_a);
        assert_eq!(listed.len(), 2);
        assert!(listed[0].request_id.as_str() < listed[1].request_id.as_str());
        assert!(!cancellation.is_cancelled());
        assert!(!registry.update_progress(tenant_a, &request_id, 3));
        assert!(registry.update_progress(tenant_a, &request_id, 1));
        assert!(!registry.update_progress(tenant_a, &request_id, 0));
        let progressed = registry
            .list(tenant_a)
            .into_iter()
            .find(|summary| summary.request_id == request_id)
            .expect("invariant: progressed request remains active");
        assert_eq!(progressed.state, RunningQueryLifecycleState::Running);
        assert_eq!(progressed.progress.completed_participants, 1);
        assert_eq!(progressed.progress.total_participants, 2);

        let first_cancel = registry
            .cancel(tenant_a, &request_id)
            .expect("invariant: same-tenant request remains active");
        assert!(first_cancel.cancellation_started);
        assert!(cancellation.is_cancelled());
        let repeated_cancel = registry
            .cancel(tenant_a, &request_id)
            .expect("invariant: cancelling request remains active");
        assert!(!repeated_cancel.cancellation_started);
        let cancelling = registry
            .list(tenant_a)
            .into_iter()
            .find(|summary| summary.request_id == request_id)
            .expect("invariant: cancelling request remains listed");
        assert_eq!(cancelling.state, RunningQueryLifecycleState::Cancelling);
        assert!(cancelling.cancellation_requested);

        let settled = registry
            .settle_terminal(tenant_a, &request_id, QueryTerminalOutcome::Degraded)
            .expect("invariant: active request settles exactly once");
        assert_eq!(settled.entry().request_id(), &request_id);
        assert_eq!(settled.outcome(), QueryTerminalOutcome::Degraded);
        assert!(
            registry
                .settle_terminal(tenant_a, &request_id, QueryTerminalOutcome::Success)
                .is_none()
        );
        assert!(registry.get(tenant_a, &request_id).is_none());

        let race_request_id = RequestId::now_v7();
        let race_entry = entry(tenant_a, race_request_id.clone());
        let race_cancellation = race_entry.cancellation_token();
        assert!(registry.insert(race_entry));

        let barrier = Arc::new(Barrier::new(3));
        let cancel_registry = Arc::clone(&registry);
        let cancel_request = race_request_id.clone();
        let cancel_barrier = Arc::clone(&barrier);
        let cancel = thread::spawn(move || {
            cancel_barrier.wait();
            cancel_registry.cancel(tenant_a, &cancel_request)
        });
        let terminal_registry = Arc::clone(&registry);
        let terminal_request = race_request_id.clone();
        let terminal_barrier = Arc::clone(&barrier);
        let terminal = thread::spawn(move || {
            terminal_barrier.wait();
            terminal_registry.settle_terminal(
                tenant_a,
                &terminal_request,
                QueryTerminalOutcome::Failed,
            )
        });
        barrier.wait();

        let cancellation = cancel.join().expect("invariant: cancel worker joins");
        let settlement = terminal.join().expect("invariant: terminal worker joins");
        let settlement = settlement.expect("invariant: terminal worker owns settlement");
        assert_eq!(settlement.outcome(), QueryTerminalOutcome::Failed);
        assert_eq!(settlement.entry().request_id(), &race_request_id);
        assert_eq!(
            cancellation
                .as_ref()
                .map(|response| response.cancellation_started),
            if race_cancellation.is_cancelled() {
                Some(true)
            } else {
                None
            }
        );
        assert!(registry.get(tenant_a, &race_request_id).is_none());
        assert!(registry.cancel(tenant_a, &race_request_id).is_none());
        assert!(
            registry
                .settle_terminal(tenant_a, &race_request_id, QueryTerminalOutcome::Success,)
                .is_none()
        );
        assert_eq!(registry.list(tenant_a).len(), 1);
        assert_eq!(registry.list(tenant_a)[0].request_id, second_request_id);
    }
}
