//! Bounded claim-driven execution for durable Forge tasks.
//!
//! The same owner runs in the embedded `all` topology and in dedicated worker
//! processes. `PostgreSQL` claims assign compute, while the table-scoped
//! [`ForgeLease`] remains the only publication fence.

use std::path::Path;
use std::str::FromStr;
#[cfg(feature = "test-support")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iceberg::spec::TableMetadata;
use iceberg::table::Table;
use sha2::{Digest, Sha256};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::queries::forge_tasks::{ForgeClaimLimits, ForgeTasks};
use vala_sql::row_types::forge_operations::ForgeOperationFamily;
use vala_sql::row_types::forge_tasks::{
    ForgeClaimStrategy, ForgeCleanupCandidate, ForgeCleanupCategory, ForgeCleanupPath,
    ForgePreparedTaskClaim, ForgeTask, ForgeTaskClaim, ForgeTaskEvidence, ForgeTaskState,
    ForgeTaskStrategy, ForgeTaskTableIdentity, ForgeTaskTransition, MAINTENANCE_STRATEGIES,
    SnapshotWatermark, TaskProgressEffect,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

use super::error::{ForgeCapacityFailurePhase, ForgeError, ForgeFailureClass};
use super::expire::{PendingExpiryTerminal, derive_recovered_files, table_resource_for_key};
use super::identity::task_table_binding;
use super::lease::{ForgeLease, forge_lease_key};
use super::live_replace::{
    IcebergRewriteDisposition, TaskLiveRewriteRequest, TaskLiveRewriteResult,
};
use super::maintenance::{ForgeMaintenance, ForgeMaintenanceResult};
use super::metrics::{
    ForgeAttemptResource, ForgeCapacityRefusalPhase, ForgeCleanupKind, ForgeConflictKind,
    ForgeDemandTransitionResult, ForgeLeaseResult, ForgeMetricStage, ForgeProgressEffect,
    ForgeTaskMetricStrategy, ForgeTaskTerminalResult,
};
use super::path::catalog_path_to_object_key;
use super::rewrite::ForgeRewritePipeline;
use super::{Forge, ForgeCapacity};
use crate::catalog::TenantTableBinding;

/// Maximum number of attempt-consuming failures before audited terminalization.
const ATTEMPT_BOUND: u32 = 5;

/// Returns whether the next classified failure must terminalize at the worker boundary.
#[must_use]
const fn failure_is_terminal(class: ForgeFailureClass, completed_attempts: u32) -> bool {
    matches!(
        class,
        ForgeFailureClass::DataRefusal
            | ForgeFailureClass::CapacityRefused
            | ForgeFailureClass::InternalInvariant
    ) || completed_attempts.saturating_add(1) >= ATTEMPT_BOUND
}

/// Exact external effect returned by strategy dispatch.
/// Borrowed authority and exact payload for one dispatched task attempt.
///
/// The attempt's rewrite pipeline, publication fence, and cancellation token
/// all belong to the worker's fenced execution, so dispatch borrows them
/// together instead of threading each through the strategy match.
struct ForgeDispatchRequest<'a> {
    /// Durable claim whose persisted plan is authoritative.
    claim: &'a ForgeTaskClaim,
    /// Attempt generation fencing this execution.
    attempt: Uuid,
    /// Tenant/table identity the claim must belong to.
    binding: &'a TenantTableBinding,
    /// Mutable publication fence retained through every external effect.
    lease: &'a mut ForgeLease,
    /// Loaded table at the attempt's observed snapshot.
    table: Table,
    /// Cancellation token this strategy observes before any committed effect.
    stop: &'a CancellationToken,
    /// Attempt-local pipeline bound to the retained operation lease.
    rewrite: &'a ForgeRewritePipeline,
}

enum ForgeDispatchResult {
    /// Ordinary publication whose evidence is not yet Prepared.
    Committed(Table),
    /// Ordered maintenance with exact post-expiry candidates.
    Maintenance(Box<ForgeMaintenanceResult>),
}

/// Durable task state accompanying exact committed evidence.
enum ForgeExecutionEvidenceState {
    /// A normal strategy execution returned evidence while its task is Running.
    Fresh,
    /// An accepted external commit was discovered while its task is Running.
    RecoveredCommit,
    /// Maintenance already persisted the task's Prepared evidence transaction.
    Prepared,
}

/// Exact durable ownership required to advance a Prepared cleanup cursor.
struct CleanupAttempt<'a> {
    /// Durable task identity.
    task_id: Uuid,
    /// Tenant transaction boundary.
    tenant: DataTenantId,
    /// Attempt generation retained across takeover.
    attempt: Uuid,
    /// Physical table binding used for path validation.
    binding: &'a TenantTableBinding,
}

/// Converts fork categories into sorted, table-bound durable cleanup evidence.
///
/// # Errors
///
/// Returns path-binding or SQL validation failures for any unsafe candidate.
fn cleanup_candidates(
    result: &ForgeMaintenanceResult,
    binding: &TenantTableBinding,
    staging: &opendal::Operator,
    identity: &ForgeTaskTableIdentity,
) -> Result<Vec<ForgeCleanupCandidate>, ForgeError> {
    let categories = [
        (ForgeCleanupCategory::Data, &result.expired_files.data_files),
        (
            ForgeCleanupCategory::Data,
            &result.expired_files.delete_files,
        ),
        (ForgeCleanupCategory::Data, &result.expired_files.statistics),
        (
            ForgeCleanupCategory::Manifest,
            &result.expired_files.manifests,
        ),
        (
            ForgeCleanupCategory::Manifest,
            &result.expired_files.manifest_lists,
        ),
        (
            ForgeCleanupCategory::Metadata,
            &result.expired_files.metadata_logs,
        ),
    ];
    let mut candidates = categories
        .into_iter()
        .flat_map(|(category, paths)| paths.iter().map(move |path| (category, path)))
        .map(|(category, path)| {
            let key = catalog_path_to_object_key(
                result.table.metadata().location(),
                binding,
                staging,
                path,
            )?;
            Ok(ForgeCleanupCandidate {
                category,
                table: identity.clone(),
                path: ForgeCleanupPath::new(key).map_err(ForgeError::Sql)?,
            })
        })
        .collect::<Result<Vec<_>, ForgeError>>()?;
    candidates.sort();
    candidates.dedup();
    Ok(candidates)
}

/// Fixed process-local bounds for one Forge worker pool.
#[derive(Debug, Clone, Copy)]
pub struct ForgeWorkerConfig {
    /// Number of task executors spawned by this process.
    ///
    /// Controls parallelism only: how many claim executors this worker runs
    /// (see the executor spawn loop in [`ForgeWorker::run`]). It no longer
    /// determines the per-tenant admission cap; that value now lives in
    /// [`Self::per_tenant_active_cap`] so parallelism can scale without
    /// silently widening what one tenant may consume.
    pub worker_concurrency: usize,
    /// Maximum active tasks one tenant may hold concurrently (the D78 fairness
    /// bound, `max_active_per_tenant` in the fair claim).
    ///
    /// Previously aliased to `worker_concurrency`; it is now an independent
    /// value. The server resolves it to `worker_concurrency` when an operator
    /// leaves it unset, preserving today's behavior, but it may be set higher
    /// or lower to tune tenant fairness separately from executor parallelism.
    pub per_tenant_active_cap: usize,
}

/// Observes successful durable task completion from supervised worker roles.
///
/// The observer is an optional test-support seam. It records only after the
/// production worker's claim execution returns success, so it cannot affect
/// scheduling, claim ownership, publication, or terminal transitions.
#[derive(Clone, Default)]
pub struct ForgeWorkerCompletionObserver {
    /// Ordered typed lifecycle events recorded by production scheduler and worker owners.
    lifecycle_events: Arc<Mutex<Vec<ForgeLifecycleEvent>>>,
    /// Number of supervised execution attempts observed after their durable path returned.
    attempts: Arc<AtomicUsize>,
    /// Errors returned by supervised execution attempts in observation order.
    returned_errors: Arc<Mutex<Vec<String>>>,
    /// Number of successful task executions observed after their durable path returned.
    completed: Arc<AtomicUsize>,
    /// Stable production worker identities that completed each observed task.
    completed_workers: Arc<Mutex<Vec<Uuid>>>,
    /// Durable task identities completed in the same order as observed workers.
    completed_tasks: Arc<Mutex<Vec<Uuid>>>,
    /// Strategies completed in the same order as the observed workers.
    completed_strategies: Arc<Mutex<Vec<ForgeClaimStrategy>>>,
    /// Wakeup used by deterministic role-topology journeys.
    ready: Arc<tokio::sync::Notify>,
    /// Unit-test seam around the notification-registration boundary.
    #[cfg(test)]
    registration_gate: Arc<Mutex<Option<CompletionObserverRegistrationGate>>>,
    /// Integration-test barrier that holds supervised workers after their first claim.
    #[cfg(feature = "test-support")]
    claim_gate: Arc<Mutex<Option<Arc<CompletionObserverClaimGate>>>>,
    /// One-shot supervised-worker loss seam after a durable claim.
    #[cfg(feature = "test-support")]
    abandon_next_claim: Arc<AtomicBool>,
    /// Whether the one-shot loss seam has stopped a supervised worker.
    #[cfg(feature = "test-support")]
    abandoned_claim: Arc<AtomicBool>,
    /// Exact durable identity of the worker claim stopped by the loss seam.
    #[cfg(feature = "test-support")]
    abandoned_identity: Arc<Mutex<Option<(Uuid, Uuid, Uuid)>>>,
    /// Wakeup for topology tests waiting for the durable claim to be abandoned.
    #[cfg(feature = "test-support")]
    abandoned_ready: Arc<tokio::sync::Notify>,
    /// One-shot passive barrier after the next supervised attempt returns.
    #[cfg(feature = "test-support")]
    pause_after_next_attempt: Arc<AtomicBool>,
    /// Whether a returned attempt is currently held at the passive barrier.
    #[cfg(feature = "test-support")]
    attempt_paused: Arc<AtomicBool>,
    /// Wakeup for tests waiting until the returned attempt is held.
    #[cfg(feature = "test-support")]
    attempt_pause_ready: Arc<tokio::sync::Notify>,
    /// Release for the one returned attempt held by the passive barrier.
    #[cfg(feature = "test-support")]
    attempt_pause_release: Arc<tokio::sync::Notify>,
}

/// Typed causal evidence from the production Forge scheduler and worker owners.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForgeLifecycleEvent {
    /// A fenced scheduler atomically enqueued the eligible inputs.
    Planned {
        /// Durable task identity assigned by the acknowledged enqueue transaction.
        task_id: Uuid,
        /// Tenant whose durable demand was acknowledged.
        tenant: DataTenantId,
        /// Physical table identity used by the scheduler.
        table: String,
        /// Sorted eligible published input identities carried by the durable plan.
        inputs: Vec<String>,
    },
    /// A production worker durably claimed one task.
    Claimed {
        /// Durable task identity claimed from `PostgreSQL`.
        task_id: Uuid,
        /// Production worker owner written into the claim.
        worker_id: Uuid,
    },
    /// The production rewrite path returned exact committed evidence.
    Rewritten {
        /// Durable task whose production dispatch returned rewrite evidence.
        task_id: Uuid,
        /// Exact planned input cardinality consumed by the rewrite.
        input_count: usize,
    },
    /// Catalog evidence names the committed replacement snapshot.
    CatalogCommitted {
        /// Durable task associated with the catalog evidence.
        task_id: Uuid,
        /// Committed Iceberg snapshot named by the production evidence.
        snapshot_id: i64,
    },
    /// The production worker returned after the durable terminal transition.
    Terminal {
        /// Durable task whose terminal path returned successfully.
        task_id: Uuid,
        /// Production worker that completed the terminal transition.
        worker_id: Uuid,
    },
}

/// Test-only barriers that pin the observer's race-sensitive wait boundary.
#[cfg(test)]
#[derive(Clone)]
struct CompletionObserverRegistrationGate {
    /// Signals that a waiter has registered its notification future.
    registered: Arc<tokio::sync::Barrier>,
    /// Holds that waiter until the test records the racing completion.
    release: Arc<tokio::sync::Barrier>,
}

/// Test-only coordination state that lets every role claim independent work before execution.
#[cfg(feature = "test-support")]
struct CompletionObserverClaimGate {
    /// Number of claims the topology test must observe before releasing execution.
    expected: usize,
    /// Number of supervised workers paused after a successful durable claim.
    claimed: AtomicUsize,
    /// Signals that all expected workers have independently claimed work.
    ready: tokio::sync::Notify,
    /// Releases all paused workers to execute their own claimed tasks.
    release: tokio::sync::Notify,
}

impl ForgeWorkerCompletionObserver {
    /// Create an empty completion observer for one shared Forge worker topology.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the number of successful worker executions observed so far.
    #[must_use]
    pub fn completed(&self) -> usize {
        self.completed.load(Ordering::Acquire)
    }

    /// Return typed lifecycle events in production-owner publication order.
    #[must_use]
    pub fn lifecycle_events(&self) -> Vec<ForgeLifecycleEvent> {
        self.lifecycle_events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Wait until the typed lifecycle stream satisfies `predicate`.
    pub async fn wait_for_lifecycle(
        &self,
        predicate: impl Fn(&[ForgeLifecycleEvent]) -> bool,
    ) -> Vec<ForgeLifecycleEvent> {
        loop {
            let notified = self.ready.notified();
            let events = self.lifecycle_events();
            if predicate(&events) {
                return events;
            }
            notified.await;
        }
    }

    /// Record one scheduler- or worker-owned lifecycle transition.
    pub(crate) fn record_lifecycle(&self, event: ForgeLifecycleEvent) {
        self.lifecycle_events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(event);
        self.ready.notify_waiters();
    }

    /// Return the number of supervised execution attempts that have returned.
    ///
    /// Unlike [`Self::completed`], this includes failed and cancelled attempts.
    /// It is passive lifecycle evidence for deterministic failure-boundary tests.
    #[must_use]
    pub fn attempts(&self) -> usize {
        self.attempts.load(Ordering::Acquire)
    }

    /// Return errors observed after supervised execution attempts returned.
    ///
    /// Successful attempts do not add an entry, so this remains passive
    /// diagnostic evidence rather than a second completion ledger.
    #[must_use]
    pub fn returned_errors(&self) -> Vec<String> {
        self.returned_errors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Return the stable worker identities that completed observed tasks.
    ///
    /// The sequence preserves completion order and may contain repeated owners
    /// when a single worker completes multiple durable tasks.
    #[must_use]
    pub fn completed_workers(&self) -> Vec<Uuid> {
        self.completed_workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Return durable task identities in supervised completion order.
    #[must_use]
    pub fn completed_tasks(&self) -> Vec<Uuid> {
        self.completed_tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Return completed task strategies in completion order.
    #[must_use]
    pub fn completed_strategies(&self) -> Vec<ForgeClaimStrategy> {
        self.completed_strategies
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Wait until at least `expected` successful executions have completed.
    ///
    /// Callers own any timeout because a test's expected durable work count is
    /// specific to its fixture. The atomic read precedes notification waiting,
    /// and the notification registration precedes the read, preventing a
    /// completion that races at either boundary from being lost.
    pub async fn wait_for_at_least(&self, expected: usize) {
        loop {
            let notified = self.ready.notified();
            #[cfg(test)]
            self.pause_after_registration_for_test().await;
            if self.completed() >= expected {
                return;
            }
            notified.await;
        }
    }

    /// Wait until at least `expected` supervised execution attempts have returned.
    ///
    /// Callers own any timeout. Notification registration precedes the atomic
    /// read so an attempt return cannot be lost at the wait boundary.
    pub async fn wait_for_attempts_at_least(&self, expected: usize) {
        loop {
            let notified = self.ready.notified();
            if self.attempts() >= expected {
                return;
            }
            notified.await;
        }
    }

    /// Hold the next supervised attempt after its production execution returns.
    ///
    /// The barrier is passive: claim, execution, durable transitions, and the
    /// returned result are unchanged. Tests use it only to stop the worker
    /// before its next claim after observing a failure boundary.
    #[cfg(feature = "test-support")]
    pub fn hold_after_next_attempt_for_test(&self) {
        self.attempt_paused.store(false, Ordering::Release);
        self.pause_after_next_attempt.store(true, Ordering::Release);
    }

    /// Wait until the armed returned-attempt barrier is holding the worker.
    #[cfg(feature = "test-support")]
    pub async fn wait_for_held_attempt_for_test(&self) {
        loop {
            let notified = self.attempt_pause_ready.notified();
            if self.attempt_paused.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    /// Release the worker held after its returned production attempt.
    #[cfg(feature = "test-support")]
    pub fn release_held_attempt_for_test(&self) {
        self.attempt_pause_release.notify_one();
    }

    /// Wait until successful tasks have been completed by `expected` distinct workers.
    ///
    /// Callers own any timeout because the task count and role topology are
    /// fixture-specific. The wait uses the same pre-registered notification
    /// pattern as [`Self::wait_for_at_least`], so a completion cannot be lost
    /// between the identity check and sleeping.
    pub async fn wait_for_distinct_workers_at_least(&self, expected: usize) {
        loop {
            let notified = self.ready.notified();
            if self
                .completed_workers()
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                >= expected
            {
                return;
            }
            notified.await;
        }
    }

    /// Wait until `expected` completed tasks use an exact strategy.
    ///
    /// The count advances only after the worker's terminal durable path
    /// returns, making this a causal publication gate for role-topology tests.
    pub async fn wait_for_strategy_at_least(&self, strategy: ForgeTaskStrategy, expected: usize) {
        loop {
            let notified = self.ready.notified();
            if self
                .completed_strategies()
                .into_iter()
                .filter(|completed| *completed == ForgeClaimStrategy::Known(strategy))
                .count()
                >= expected
            {
                return;
            }
            notified.await;
        }
    }

    /// Hold supervised workers after a durable claim until `expected` roles participate.
    ///
    /// This is a test-support-only deterministic topology seam. It observes a
    /// claim that the production SQL path has already persisted, then delays
    /// only the test fixture's execution timing; it does not change claim
    /// fairness, ownership, task state, or publication behavior.
    #[cfg(feature = "test-support")]
    pub fn hold_after_claims_for_test(&self, expected: usize) {
        *self
            .claim_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some(Arc::new(CompletionObserverClaimGate {
                expected,
                claimed: AtomicUsize::new(0),
                ready: tokio::sync::Notify::new(),
                release: tokio::sync::Notify::new(),
            }));
    }

    /// Wait until the configured number of supervised workers have claimed work.
    ///
    /// # Panics
    ///
    /// Panics when no claim gate was configured by
    /// [`Self::hold_after_claims_for_test`].
    #[cfg(feature = "test-support")]
    pub async fn wait_for_claims_for_test(&self) {
        let gate = self
            .claim_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .expect("claim gate must be configured before waiting");
        loop {
            let notified = gate.ready.notified();
            if gate.claimed.load(Ordering::Acquire) >= gate.expected {
                return;
            }
            notified.await;
        }
    }

    /// Release every supervised worker paused by the configured claim gate.
    ///
    /// # Panics
    ///
    /// Panics when no claim gate was configured by
    /// [`Self::hold_after_claims_for_test`].
    #[cfg(feature = "test-support")]
    pub fn release_claims_for_test(&self) {
        let gate = self
            .claim_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .expect("claim gate must be configured before release");
        gate.release.notify_waiters();
    }

    /// Make one supervised worker stop after it durably claims work.
    ///
    /// The persisted claim is deliberately not changed. Another real worker
    /// must reclaim it after the normal lease expiry, exercising production
    /// durable recovery instead of a test-only execution path.
    #[cfg(feature = "test-support")]
    pub fn abandon_next_claim_for_test(&self) {
        self.abandoned_claim.store(false, Ordering::Release);
        *self
            .abandoned_identity
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        self.abandon_next_claim.store(true, Ordering::Release);
    }

    /// Wait until a supervised worker has stopped after its durable claim.
    #[cfg(feature = "test-support")]
    pub async fn wait_for_abandoned_claim_for_test(&self) {
        while !self.abandoned_claim.load(Ordering::Acquire) {
            self.abandoned_ready.notified().await;
        }
    }

    /// Return the task, attempt, and owner identity of the stopped worker claim.
    ///
    /// # Panics
    ///
    /// Panics when the loss seam has not stopped a worker claim.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn abandoned_claim_identity_for_test(&self) -> (Uuid, Uuid, Uuid) {
        self.abandoned_identity
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .expect("abandoned worker claim identity must be recorded")
    }

    /// Record one successful production worker execution after its durable path returns.
    pub(crate) fn record(&self, worker: Uuid, task_id: Uuid, strategy: ForgeClaimStrategy) {
        self.completed_workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(worker);
        self.completed_tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(task_id);
        self.completed_strategies
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(strategy);
        self.completed.fetch_add(1, Ordering::AcqRel);
        self.ready.notify_waiters();
    }

    /// Record that one supervised execution path returned to its worker loop.
    fn record_attempt(&self, error: Option<&ForgeError>) {
        if let Some(error) = error {
            self.returned_errors
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(error.to_string());
        }
        self.attempts.fetch_add(1, Ordering::AcqRel);
        self.ready.notify_waiters();
    }

    /// Pause once after an armed supervised attempt has fully returned.
    #[cfg(feature = "test-support")]
    async fn pause_after_attempt_for_test(&self) {
        if !self.pause_after_next_attempt.swap(false, Ordering::AcqRel) {
            return;
        }
        self.attempt_paused.store(true, Ordering::Release);
        self.attempt_pause_ready.notify_waiters();
        self.attempt_pause_release.notified().await;
        self.attempt_paused.store(false, Ordering::Release);
    }

    /// Observe one persisted claim and pause execution until all configured roles participate.
    #[cfg(feature = "test-support")]
    async fn pause_after_claim_for_test(&self) {
        let gate = self
            .claim_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let Some(gate) = gate else {
            return;
        };
        let claimed = gate.claimed.fetch_add(1, Ordering::AcqRel) + 1;
        if claimed >= gate.expected {
            gate.ready.notify_waiters();
        }
        loop {
            let notified = gate.release.notified();
            if gate.claimed.load(Ordering::Acquire) >= gate.expected {
                return;
            }
            notified.await;
        }
    }

    /// Consume the one-shot abandoned-claim seam after the durable claim is visible.
    #[cfg(feature = "test-support")]
    fn abandon_claim_for_test(&self, claim: &ForgeTaskClaim, owner: Uuid) -> bool {
        if self.abandon_next_claim.swap(false, Ordering::AcqRel) {
            let attempt = claim
                .attempt_id
                .expect("persisted abandoned Forge claim must carry an attempt");
            *self
                .abandoned_identity
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some((claim.task_id, attempt, owner));
            self.abandoned_claim.store(true, Ordering::Release);
            self.abandoned_ready.notify_waiters();
            true
        } else {
            false
        }
    }

    /// Pause a unit-test waiter after notification registration and before its read.
    #[cfg(test)]
    async fn pause_after_registration_for_test(&self) {
        let gate = self
            .registration_gate
            .lock()
            .expect("completion-observer registration gate lock must not be poisoned")
            .clone();
        if let Some(gate) = gate {
            gate.registered.wait().await;
            gate.release.wait().await;
        }
    }

    /// Install deterministic barriers for one unit-test registration race.
    #[cfg(test)]
    fn install_registration_gate_for_test(
        &self,
        registered: Arc<tokio::sync::Barrier>,
        release: Arc<tokio::sync::Barrier>,
    ) {
        *self
            .registration_gate
            .lock()
            .expect("completion-observer registration gate lock must not be poisoned") =
            Some(CompletionObserverRegistrationGate {
                registered,
                release,
            });
    }
}

impl Default for ForgeWorkerConfig {
    /// Uses one executor and a matching single-task per-tenant cap so embedded
    /// deployments share dedicated semantics.
    fn default() -> Self {
        Self {
            worker_concurrency: 1,
            per_tenant_active_cap: 1,
        }
    }
}

impl ForgeWorkerConfig {
    /// Validates the fixed worker set before any executor is spawned.
    ///
    /// # Errors
    ///
    /// Returns invalid configuration when the executor count or the per-tenant
    /// active cap is zero; a zero cap would admit no task for any tenant.
    pub fn validate(self) -> Result<Self, ForgeError> {
        if self.worker_concurrency == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge worker concurrency must be positive".to_owned(),
            });
        }
        if self.per_tenant_active_cap == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge per-tenant active cap must be positive".to_owned(),
            });
        }
        Ok(self)
    }
}

/// Stable pod-local scratch-volume identity used for quarantine deferral.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScratchVolumeIdentity(String);

impl ScratchVolumeIdentity {
    /// Derives identity from the canonical root and Unix device number.
    ///
    /// # Errors
    /// Returns typed scratch IO when canonicalization or metadata inspection fails.
    fn from_root(root: &Path) -> Result<Self, ForgeError> {
        use std::os::unix::fs::MetadataExt;
        let canonical = root.canonicalize().map_err(|error| ForgeError::ScratchIo {
            kind: error.kind(),
            detail: format!("canonicalize {}: {error}", root.display()),
        })?;
        let metadata = canonical
            .metadata()
            .map_err(|error| ForgeError::ScratchIo {
                kind: error.kind(),
                detail: format!("inspect {}: {error}", canonical.display()),
            })?;
        Ok(Self(format!("{}:{}", canonical.display(), metadata.dev())))
    }

    /// Returns the durable closed identity spelling.
    #[must_use]
    fn as_str(&self) -> &str {
        &self.0
    }
}

/// Claim-driven Forge executor shared by embedded and dedicated topologies.
#[derive(Clone)]
pub struct ForgeWorker {
    /// Complete Forge dependency owner used by every execution slot.
    forge: Arc<Forge>,
    /// Durable lifecycle query owner.
    tasks: ForgeTasks,
    /// Stable worker process identity used by every claim in this pool.
    owner: Uuid,
    /// Fixed process-local concurrency.
    config: ForgeWorkerConfig,
    /// Optional observer that receives only completed supervised executions.
    completion_observer: Option<ForgeWorkerCompletionObserver>,
    /// Complete capacity declaration passed to atomic `PostgreSQL` admission.
    capacity: ForgeCapacity,
    /// One-shot crash boundary after task evidence commits and before expiry closes.
    #[cfg(feature = "test-support")]
    fail_after_maintenance_prepared: Arc<AtomicBool>,
}

impl ForgeWorker {
    /// Constructs one bounded worker over an existing Forge dependency graph.
    ///
    /// # Errors
    ///
    /// Returns invalid configuration when worker or derived claim limits are
    /// zero or cannot fit their durable integer domains.
    pub fn new(
        forge: Arc<Forge>,
        config: ForgeWorkerConfig,
        owner: Uuid,
    ) -> Result<Self, ForgeError> {
        let config = config.validate()?;
        let limits = &forge.core.config;
        let capacity = ForgeCapacity::try_from(limits)?;
        Ok(Self {
            completion_observer: forge.core.completion_observer.clone(),
            tasks: ForgeTasks::new(forge.core.operator_pool.clone()),
            forge,
            owner,
            config,
            capacity,
            #[cfg(feature = "test-support")]
            fail_after_maintenance_prepared: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Arms one failure after maintenance task Prepared commits but before expiry terminal audit.
    #[cfg(feature = "test-support")]
    pub fn fail_after_maintenance_prepared_for_test(&self) {
        self.fail_after_maintenance_prepared
            .store(true, Ordering::Release);
    }

    /// Returns this worker's stable claim-ownership identity.
    ///
    /// A test constructs a sibling worker with this identity so it can execute a
    /// claim this worker already took — the durable claim transitions fence on
    /// `claimed_by`/`attempt_id`, so a differently-owned worker would be
    /// rejected. It is the minimal seam that lets a boundary (for example a
    /// tightened Iceberg retry timeout carried only by the sibling's Forge)
    /// apply to execution without perturbing the setup that produced the claim.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn owner_for_test(&self) -> Uuid {
        self.owner
    }

    /// Runs the fixed worker set until shutdown stops claims and drains slots.
    ///
    /// Each slot claims at most one task at a time. Cancellation stops new
    /// claims immediately; active operations observe the same token at their
    /// established safe boundaries before the pool joins every slot.
    ///
    /// # Errors
    ///
    /// Returns a slot panic or an unexpected slot-level configuration failure.
    pub async fn run(self, shutdown: CancellationToken) -> Result<(), ForgeError> {
        let volume = self.scratch_volume_identity()?;
        if let Err(error) = self.probe_scratch() {
            self.tasks
                .quarantine_worker(self.owner, volume.as_str())
                .await
                .map_err(ForgeError::Sql)?;
            self.forge.core.telemetry.record_quarantine_state(true);
            tracing::error!(worker=%self.owner, error=%error, "Forge worker quarantined by startup scratch probe");
            return Ok(());
        }
        self.tasks
            .register_healthy_worker(self.owner, volume.as_str())
            .await
            .map_err(ForgeError::Sql)?;
        self.forge.core.telemetry.record_quarantine_state(false);
        let mut slots = JoinSet::new();
        for index in 0..self.config.worker_concurrency {
            let worker = self.clone();
            let stop = shutdown.clone();
            // Slot zero is the reserved maintenance slot: it always attempts a
            // maintenance-strategy claim before falling back to any strategy, so
            // a ready maintenance task can never be starved by a compaction
            // backlog occupying every other slot. With a single-slot worker that
            // one slot carries the reservation.
            let reserved_maintenance = index == 0;
            slots.spawn(async move { worker.run_slot(stop, reserved_maintenance).await });
        }
        while let Some(result) = slots.join_next().await {
            result.map_err(|error| ForgeError::Invariant {
                detail: format!("Forge worker slot panicked: {error}"),
            })??;
        }
        Ok(())
    }

    /// Returns the stable identity of this worker's configured scratch volume.
    ///
    /// # Errors
    /// Returns typed scratch IO when the root cannot be canonicalized or inspected.
    fn scratch_volume_identity(&self) -> Result<ScratchVolumeIdentity, ForgeError> {
        ScratchVolumeIdentity::from_root(&self.forge.core.rewrite_spill_root)
    }

    /// Probes scratch writability through create, fsync, and exact-file delete.
    ///
    /// # Errors
    /// Returns typed scratch IO for any local filesystem failure.
    fn probe_scratch(&self) -> Result<(), ForgeError> {
        probe_scratch_root(&self.forge.core.rewrite_spill_root, self.owner)
    }

    /// Deletes only scratch directories owned by one exact durable attempt.
    ///
    /// # Errors
    /// Returns typed scratch IO when directory inspection or deletion fails.
    fn cleanup_attempt_scratch(&self, task_id: Uuid, attempt_id: Uuid) -> Result<(), ForgeError> {
        cleanup_attempt_scratch_root(&self.forge.core.rewrite_spill_root, task_id, attempt_id)
    }

    /// Reclaims expired durable attempts and removes only their exact scratch prefixes.
    ///
    /// # Errors
    /// Returns SQL or typed scratch errors; durable reclaim may precede cleanup.
    async fn reclaim_expired_attempts(&self, cap: u32) -> Result<Vec<(Uuid, Uuid)>, ForgeError> {
        let reclaimed = self
            .tasks
            .reclaim_expired_attempts(cap)
            .await
            .map_err(ForgeError::Sql)?;
        for (task_id, attempt_id) in &reclaimed {
            self.cleanup_attempt_scratch(*task_id, *attempt_id)?;
        }
        Ok(reclaimed)
    }

    /// Reclaims expired attempts through the production worker owner in tests.
    ///
    /// # Errors
    ///
    /// Returns the production SQL or exact scratch-cleanup failure.
    #[cfg(feature = "test-support")]
    pub async fn reclaim_expired_attempts_for_test(
        &self,
        cap: u32,
    ) -> Result<Vec<(Uuid, Uuid)>, ForgeError> {
        self.reclaim_expired_attempts(cap).await
    }

    /// Claims and executes work serially for one bounded pool slot.
    ///
    /// This slot owns pre-execution release, while [`Self::execute_claim`] owns
    /// in-execution settlement before terminal telemetry. Two windows drain a
    /// just-claimed task to `retryable` through
    /// [`Self::release_cancelled_claim`] so a clean shutdown drives
    /// `forge_active_claims` to zero: the pre-execution window, when shutdown is
    /// observed after a claim is taken but before execution begins; and the
    /// in-execution window, when [`Self::execute_claim`] returns
    /// [`ForgeError::Shutdown`] because a pre-effect checkpoint fired before any
    /// durable side effect. Both cases performed no durable work, so a successor
    /// reclaims the released task losslessly. A post-effect cancellation instead
    /// returns [`ForgeError::ShutdownRetained`] and is retained here for
    /// evidence-based and lease-expiry recovery, never released. Settling
    /// before telemetry ensures the task span reflects authoritative SQL state.
    ///
    /// # Errors
    ///
    /// Returns only configuration errors that make further claims unsafe.
    /// Individual task failures remain durable and are logged for takeover; a
    /// shutdown-release database failure is likewise logged and the claim is
    /// retained for lease-expiry recovery rather than failing the slot.
    ///
    /// When `reserved_maintenance` is set this slot attempts a maintenance-only
    /// claim before any unfiltered claim on every iteration, reserving its
    /// capacity for maintenance whenever such work is ready.
    async fn run_slot(
        &self,
        shutdown: CancellationToken,
        reserved_maintenance: bool,
    ) -> Result<(), ForgeError> {
        let claim_limits = self.claim_limits()?;
        loop {
            if shutdown.is_cancelled() {
                return Ok(());
            }
            self.reclaim_expired_attempts(claim_limits.max_active_per_tenant)
                .await?;
            if let Some(prepared) = self
                .tasks
                .claim_prepared_for_reconciliation(self.owner, claim_limits.lease_seconds)
                .await
                .map_err(ForgeError::Sql)?
            {
                let task_id = prepared.task.task_id;
                let strategy = ForgeClaimStrategy::Known(prepared.task.strategy);
                if let Some(observer) = &self.completion_observer {
                    observer.record_lifecycle(ForgeLifecycleEvent::Claimed {
                        task_id,
                        worker_id: self.owner,
                    });
                }
                let result = self.reconcile_prepared(prepared, &shutdown).await;
                self.record_attempt(result.as_ref().err());
                #[cfg(feature = "test-support")]
                self.pause_after_attempt_for_test().await;
                match result {
                    Ok(()) => self.record_completion(task_id, strategy),
                    Err(error) => {
                        tracing::warn!(worker = %self.owner, error = %error, "Prepared Forge task reconciliation stopped; exact evidence retained");
                    }
                }
                continue;
            }
            let claim = self
                .claim_next(claim_limits, reserved_maintenance)
                .await
                .map_err(ForgeError::Sql)?;
            let Some(claim) = claim else {
                tokio::select! {
                    () = shutdown.cancelled() => return Ok(()),
                    () = tokio::time::sleep(Duration::from_millis(250)) => {}
                }
                continue;
            };
            let task_id = claim.task_id;
            if let Some(observer) = &self.completion_observer {
                observer.record_lifecycle(ForgeLifecycleEvent::Claimed {
                    task_id,
                    worker_id: self.owner,
                });
            }
            #[cfg(feature = "test-support")]
            if let Some(observer) = &self.completion_observer {
                observer.pause_after_claim_for_test().await;
                if observer.abandon_claim_for_test(&claim, self.owner) {
                    return Ok(());
                }
            }
            if shutdown.is_cancelled() {
                if let Some(attempt) = claim.attempt_id
                    && let Err(error) = self.release_cancelled_claim(task_id, attempt).await
                {
                    tracing::warn!(worker = %self.owner, task_id = %task_id, error = %error, "Forge claim shutdown release failed; durable state retained for lease recovery");
                }
                return Ok(());
            }
            let strategy = claim.strategy.clone();
            let result = self.execute_claim(claim, &shutdown).await;
            self.record_attempt(result.as_ref().err());
            #[cfg(feature = "test-support")]
            self.pause_after_attempt_for_test().await;
            if result.is_ok() {
                self.record_completion(task_id, strategy);
            }
        }
    }

    /// Publish one observer event after a successful worker execution.
    ///
    /// This is deliberately called after the full durable execution path
    /// returns, so test observation cannot acknowledge or alter a task.
    fn record_completion(&self, task_id: Uuid, strategy: ForgeClaimStrategy) {
        if let Some(observer) = &self.completion_observer {
            observer.record_lifecycle(ForgeLifecycleEvent::Terminal {
                task_id,
                worker_id: self.owner,
            });
            observer.record(self.owner, task_id, strategy);
        }
    }

    /// Publish one passive observer event after any supervised attempt returns.
    fn record_attempt(&self, error: Option<&ForgeError>) {
        if let Some(observer) = &self.completion_observer {
            observer.record_attempt(error);
        }
    }

    /// Apply the observer's one-shot passive returned-attempt barrier.
    #[cfg(feature = "test-support")]
    async fn pause_after_attempt_for_test(&self) {
        if let Some(observer) = &self.completion_observer {
            observer.pause_after_attempt_for_test().await;
        }
    }

    /// Claims and executes at most one task for deterministic integration fixtures.
    ///
    /// Prepared recovery retains priority over ordinary claims exactly as it
    /// does in the supervised worker loop.
    ///
    /// # Errors
    ///
    /// Returns claim, validation, execution, reconciliation, or lifecycle
    /// failures instead of logging them for a later supervised retry.
    #[cfg(feature = "test-support")]
    pub async fn execute_one_for_test(
        &self,
        shutdown: &CancellationToken,
    ) -> Result<bool, ForgeError> {
        let limits = self.claim_limits()?;
        self.reclaim_expired_attempts(limits.max_active_per_tenant)
            .await?;
        if let Some(prepared) = self
            .tasks
            .claim_prepared_for_reconciliation(self.owner, limits.lease_seconds)
            .await
            .map_err(ForgeError::Sql)?
        {
            self.reconcile_prepared(prepared, shutdown).await?;
            return Ok(true);
        }
        let claim = self
            .tasks
            .claim_fair(self.owner, limits, None)
            .await
            .map_err(ForgeError::Sql)?;
        let Some(claim) = claim else {
            return Ok(false);
        };
        self.execute_claim(claim, shutdown).await?;
        Ok(true)
    }

    /// Claims one ordinary task with this worker's production owner and limits.
    ///
    /// This narrow seam lets integration tests drive [`Self::execute_claim`]
    /// directly while preserving the same SQL admission transaction used by
    /// the supervised loop.
    ///
    /// # Errors
    ///
    /// Returns configuration, SQL, or persisted-row decoding failures.
    #[cfg(feature = "test-support")]
    pub async fn claim_for_test(&self) -> Result<Option<ForgeTaskClaim>, ForgeError> {
        let limits = self.claim_limits()?;
        self.tasks
            .claim_fair(self.owner, limits, None)
            .await
            .map_err(ForgeError::Sql)
    }

    /// Drives one reserved-slot claim attempt for deterministic fixtures.
    ///
    /// Mirrors [`Self::claim_next`] so an integration test can prove that a
    /// reserved maintenance slot claims a ready maintenance task ahead of a
    /// compaction backlog, then falls back to any strategy when no maintenance
    /// work is ready.
    ///
    /// # Errors
    ///
    /// Returns configuration, SQL, or persisted-row decoding failures.
    #[cfg(feature = "test-support")]
    pub async fn claim_next_for_test(
        &self,
        reserved_maintenance: bool,
    ) -> Result<Option<ForgeTaskClaim>, ForgeError> {
        let limits = self.claim_limits()?;
        self.claim_next(limits, reserved_maintenance)
            .await
            .map_err(ForgeError::Sql)
    }

    /// Invokes the pre-effect shutdown release directly for one planted claim.
    ///
    /// This narrow seam lets an integration test assert the retain-on-advance
    /// property of [`Self::release_cancelled_claim`] without reconstructing the
    /// non-deterministic window between a maintenance `prepared()` commit and
    /// the worker's next cancellation checkpoint. Releasing an already-advanced
    /// claim — in particular a `prepared` row past the pre-effect guard, owned
    /// by this worker and attempt — must be benign: the retry matches no row,
    /// so the method returns `Ok(())` and the durable `prepared` state is
    /// retained for reconciliation rather than surfacing a release error.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Sql`] for a genuine database failure and
    /// [`ForgeError::InvalidConfig`] when the release timestamp cannot be read.
    /// A benign no-match against an already-advanced claim is not an error and
    /// returns `Ok(())`.
    #[cfg(feature = "test-support")]
    pub async fn release_cancelled_claim_for_test(
        &self,
        task_id: Uuid,
        attempt: Uuid,
    ) -> Result<(), ForgeError> {
        self.release_cancelled_claim(task_id, attempt).await
    }

    /// Releases a cooperatively cancelled claim to `retryable` before its
    /// [`ForgeError::Shutdown`] surfaces, draining `forge_active_claims` to zero
    /// at a clean shutdown instead of retaining the claim for lease-expiry
    /// recovery.
    ///
    /// Cooperative cancellation is an event the worker observes with no
    /// pre-effect durable work performed, so releasing the claim is lossless: a
    /// successor reclaims the `retryable` task immediately. This is deliberately
    /// distinct from crash recovery, where cancellation is never observed and
    /// the claim is retained for `reclaim_expired` after its lease TTL.
    ///
    /// The release is guarded to pre-effect ownership. [`ForgeTasks::retry`]
    /// matches only `state IN ('claimed','running')` for this attempt and owner,
    /// so a claim that already advanced past that guard — expiry-reclaimed by
    /// another worker, or transitioned to `prepared` between the caller's
    /// shutdown observation and this call — does not match. That benign no-match
    /// surfaces as [`vala_sql::SqlError::Conflict`] and is treated as
    /// claim-already-advanced: the method returns `Ok(())` and the caller falls
    /// through to its existing retain-for-recovery path, which is the correct
    /// outcome for a post-effect (`prepared`) claim. This is why a `prepared`
    /// claim under cancellation is observed as clean retention, never a release
    /// error.
    ///
    /// Only the durable claim state changes; the caller still returns
    /// [`ForgeError::Shutdown`], preserving the codebase-wide "shutdown stops
    /// work" signal for both slot exit and in-flight cancellation.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Sql`] for a genuine database failure (connection,
    /// pool, or statement error) and [`ForgeError::InvalidConfig`] when the
    /// release timestamp cannot be read. A benign no-match is not an error and
    /// returns `Ok(())`.
    async fn release_cancelled_claim(
        &self,
        task_id: Uuid,
        attempt: Uuid,
    ) -> Result<(), ForgeError> {
        let ready_at = self.forge.core.clock.now()?;
        match self
            .tasks
            .retry(task_id, attempt, self.owner, ready_at)
            .await
        {
            // Success releases the claim; a benign `Conflict` means the claim
            // already advanced past the pre-effect guard and stays retained.
            Ok(()) | Err(vala_sql::SqlError::Conflict { .. }) => Ok(()),
            Err(error) => Err(ForgeError::Sql(error)),
        }
    }

    /// Applies the supervised slot's single in-execution cooperative-shutdown
    /// settlement to a failed [`Self::execute_claim`] result.
    ///
    /// Executes one planted claim through the production settlement path.
    ///
    /// This narrow seam lets integration tests prove pre-effect retry and
    /// post-effect retention without adding a second settlement after
    /// [`Self::execute_claim`].
    ///
    /// # Errors
    ///
    /// Returns the underlying [`Self::execute_claim`] result unchanged: a
    /// pre-effect cancellation still surfaces [`ForgeError::Shutdown`], and a
    /// post-effect cancellation surfaces [`ForgeError::ShutdownRetained`].
    #[cfg(feature = "test-support")]
    pub async fn execute_and_settle_claim_for_test(
        &self,
        claim: ForgeTaskClaim,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        self.execute_claim(claim, shutdown).await
    }

    /// Executes one exact claimed task through validation, table fencing,
    /// bounded rewrite, exact evidence persistence, and terminal audit.
    ///
    /// Reserved, maintenance, and malformed payloads terminalize before catalog
    /// load, lease acquisition, or object IO. Supported tasks acquire the table
    /// lease before any catalog or object-store effect.
    ///
    /// # Errors
    ///
    /// Returns validation, SQL, catalog, object-store, rewrite, fencing,
    /// cancellation, evidence, or audit errors. Uncertain external outcomes
    /// remain nonterminal for lease-based recovery.
    pub async fn execute_claim(
        &self,
        claim: ForgeTaskClaim,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let task = &claim;
        if claim.execution_tenant_id != task.data_tenant_id {
            return Err(ForgeError::Invariant {
                detail: "Forge task tenant differs from claim execution context".to_owned(),
            });
        }
        let attempt = task.attempt_id.ok_or_else(|| ForgeError::Invariant {
            detail: "claimed Forge task has no attempt generation".to_owned(),
        })?;
        if task.claimed_by != Some(self.owner) || task.state != ForgeTaskState::Claimed {
            return Err(ForgeError::Invariant {
                detail: "Forge worker received a claim owned by another attempt".to_owned(),
            });
        }
        if task.estimates.envelope.is_none() {
            let result = self.cancel_superseded(task).await;
            self.forge
                .core
                .telemetry
                .record_legacy_supersession(result.is_ok());
            return result;
        }
        let binding = task_table_binding(
            task.data_tenant_id,
            claim.execution_tenant_id,
            &task.table_ref,
        )?;
        let stage = match Self::validate_payload(task) {
            Ok(validated) => validated,
            Err(error) => {
                self.fail_before_effect(task, attempt, error.to_string())
                    .await?;
                return Ok(());
            }
        };
        let lease_key = forge_lease_key(
            task.data_tenant_id,
            &binding.logical_namespace,
            &binding.table_name,
        );
        let Some(mut lease) = ForgeLease::acquire(
            &self.forge.core.operator_pool,
            lease_key,
            self.owner,
            self.forge.core.config.lease_ttl,
        )
        .await?
        else {
            self.forge
                .core
                .telemetry
                .record_lease(ForgeLeaseResult::Contention);
            self.forge
                .core
                .telemetry
                .record_conflict(ForgeConflictKind::LeaseContention);
            return Err(ForgeError::FenceLost {
                lease_key: format!("forge:table:{}:{}", task.data_tenant_id, binding.table_ref),
            });
        };
        if lease.takeover() {
            self.forge
                .core
                .telemetry
                .record_lease(ForgeLeaseResult::Takeover);
        }
        let started = Instant::now();
        let task_span = tracing::info_span!(
            "bifrost.forge.task.execute",
            strategy = task.strategy.as_str(),
            result = tracing::field::Empty,
            role = "forge_worker",
            task_id = %task.task_id,
            attempt_id = %attempt,
        );
        let result = tracing::Instrument::instrument(
            self.execute_fenced(task, attempt, &binding, &mut lease, shutdown),
            task_span.clone(),
        )
        .await;
        let elapsed = started.elapsed();
        if let Err(error) = &result {
            tracing::warn!(
                task_id = %task.task_id,
                attempt_id = %attempt,
                failure_class = error.failure_class().as_str(),
                error = %error,
                "Forge task execution failed"
            );
        }
        if let Err(error) = &result
            && let Err(settlement) = self.settle_execution_failure(task, attempt, error).await
        {
            tracing::error!(task_id=%task.task_id, error=%settlement, "Forge failure settlement failed; claim retained for expiry recovery");
        }
        self.record_task_execution_telemetry(task, &task_span, elapsed)
            .await;
        self.forge
            .core
            .telemetry
            .record_stage(stage, elapsed, result.is_err());
        if matches!(result, Err(ForgeError::FenceLost { .. })) {
            self.forge
                .core
                .telemetry
                .record_lease(ForgeLeaseResult::FenceLost);
            self.forge
                .core
                .telemetry
                .record_conflict(ForgeConflictKind::FenceLost);
        }
        if let Err(error) = lease.release(&self.forge.core.operator_pool).await {
            tracing::warn!(task_id = %task.task_id, error = %error, "Forge table lease release failed");
        }
        result
    }

    /// Records the task-duration observation from the durable task lifecycle state.
    ///
    /// The worker never infers a terminal metric result from the Rust return
    /// value: successful execution can durably cancel a superseded task, and
    /// an error can leave the task retryable. Nonterminal states intentionally
    /// produce no terminal duration observation.
    async fn record_task_execution_telemetry(
        &self,
        task: &ForgeTaskClaim,
        span: &tracing::Span,
        elapsed: Duration,
    ) {
        let Ok(Some(task_result)) = self.durable_task_metric_result(task).await else {
            return;
        };
        span.record("result", task_result.as_str());
        let strategy = match task.strategy {
            ForgeClaimStrategy::Known(strategy) => ForgeTaskMetricStrategy::try_from(strategy)
                .expect("invariant: validated Forge execution strategy has a task metric label"),
            ForgeClaimStrategy::Unknown(_) => {
                unreachable!("invariant: unknown Forge strategy is rejected before execution")
            }
        };
        self.forge
            .core
            .telemetry
            .record_task_terminal(strategy, task_result, elapsed);
    }

    /// Reads the authoritative post-attempt state and maps it into telemetry.
    ///
    /// # Errors
    ///
    /// Returns tenant-connection, SQL, or durable-state parsing errors. The
    /// caller treats an observation failure as diagnostic-only because the
    /// underlying task transition has already committed independently.
    async fn durable_task_metric_result(
        &self,
        task: &ForgeTaskClaim,
    ) -> Result<Option<ForgeTaskTerminalResult>, ForgeError> {
        let mut conn = self
            .forge
            .core
            .vala
            .tenant_conn(task.data_tenant_id)
            .await
            .map_err(ForgeError::Sql)?;
        let state: Option<String> = sqlx::query_scalar(
            "SELECT state FROM vala.forge_tasks WHERE task_id = $1 AND data_tenant_id = $2",
        )
        .bind(task.task_id)
        .bind(task.data_tenant_id.as_uuid())
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(vala_sql::SqlError::from)
        .map_err(ForgeError::Sql)?;
        let Some(state) = state else {
            return Ok(None);
        };
        let state = ForgeTaskState::from_str(&state).map_err(ForgeError::Sql)?;
        Ok(ForgeTaskTerminalResult::try_from(state).ok())
    }

    /// Reconciles one taken-over Prepared attempt from its exact stored evidence.
    ///
    /// Recovery never repeats the rewrite or reconstructs a serialized table.
    /// It verifies the committed snapshot remains retained and hashes the exact
    /// metadata object named by durable evidence before terminalizing.
    ///
    /// # Errors
    ///
    /// Returns malformed identity/evidence, lease, catalog, object-read,
    /// heartbeat, digest mismatch, audit, or fencing failures. Any failure
    /// retains the Prepared row for another bounded takeover.
    async fn reconcile_prepared(
        &self,
        claim: ForgePreparedTaskClaim,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let task = &claim.task;
        if claim.execution_tenant_id != task.data_tenant_id {
            return Err(ForgeError::Invariant {
                detail: "Prepared task tenant differs from claim execution context".to_owned(),
            });
        }
        let attempt = task.attempt_id.ok_or_else(|| ForgeError::Invariant {
            detail: "Prepared Forge task has no attempt generation".to_owned(),
        })?;
        if task.state != ForgeTaskState::Prepared || task.claimed_by != Some(self.owner) {
            return Err(ForgeError::Invariant {
                detail: "Prepared Forge reconciliation ownership changed".to_owned(),
            });
        }
        let binding = task_table_binding(
            task.data_tenant_id,
            claim.execution_tenant_id,
            &task.table_ref,
        )?;
        let evidence = task
            .evidence
            .as_ref()
            .ok_or_else(|| ForgeError::Reconciliation {
                detail: "Prepared Forge task has no committed evidence".to_owned(),
            })?;
        let lease_key = forge_lease_key(
            task.data_tenant_id,
            &binding.logical_namespace,
            &binding.table_name,
        );
        let Some(mut lease) = ForgeLease::acquire(
            &self.forge.core.operator_pool,
            lease_key,
            self.owner,
            self.forge.core.config.lease_ttl,
        )
        .await?
        else {
            return Err(ForgeError::FenceLost {
                lease_key: format!("forge:table:{}:{}", task.data_tenant_id, binding.table_ref),
            });
        };
        // Prepared-effect recovery runs post-commit idempotent cleanup behind a
        // durable cursor; graceful shutdown cooperatively drains it, so both
        // heartbeat tokens are the same shutdown-sensitive `operation_stop`
        // (this path has no shutdown-decoupled in-flight commit to protect).
        let operation_stop = shutdown.child_token();
        let heartbeat = self.spawn_authority_heartbeat(
            task.task_id,
            attempt,
            lease.clone(),
            operation_stop.clone(),
            operation_stop.clone(),
        )?;
        let progressed = evidence.committed_snapshot_id != Some(task.base_snapshot_id)
            || evidence.deleted_candidate_count > 0;
        let reconciliation = self
            .resume_prepared_effect(
                task,
                attempt,
                &binding,
                &mut lease,
                evidence,
                &operation_stop,
            )
            .await;
        operation_stop.cancel();
        let heartbeat_result = heartbeat.await.map_err(|error| ForgeError::Invariant {
            detail: format!("Forge reconciliation heartbeat panicked: {error}"),
        })?;
        let result = async {
            heartbeat_result?;
            reconciliation?;
            self.tasks
                .heartbeat(
                    task.task_id,
                    attempt,
                    self.owner,
                    self.claim_limits()?.lease_seconds,
                )
                .await
                .map_err(ForgeError::Sql)?;
            lease.require_fence(&self.forge.core.operator_pool).await?;
            self.persist_terminal_success(
                task.task_id,
                task.data_tenant_id,
                &task.table_ref,
                attempt,
                &lease,
                task_progress_effect(
                    &ForgeClaimStrategy::Known(task.strategy),
                    task.base_snapshot_id,
                    &task.plan.parameters,
                    progressed,
                ),
            )
            .await
        }
        .await;
        if let Err(error) = lease.release(&self.forge.core.operator_pool).await {
            tracing::warn!(task_id = %task.task_id, error = %error, "Forge reconciliation lease release failed");
        }
        result
    }

    /// Verifies and resumes every external effect owned by one Prepared task.
    ///
    /// # Errors
    ///
    /// Returns evidence, cleanup, orphan, fencing, catalog, or cancellation failures.
    async fn resume_prepared_effect(
        &self,
        task: &ForgeTask,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        evidence: &ForgeTaskEvidence,
        stop: &CancellationToken,
    ) -> Result<(), ForgeError> {
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let table = self.forge.load_table(&binding.table_ident()).await?;
        self.verify_committed_evidence(binding, &table, evidence)
            .await?;
        if task.strategy == ForgeTaskStrategy::StagingFold {
            let snapshot_id =
                evidence
                    .committed_snapshot_id
                    .ok_or_else(|| ForgeError::Reconciliation {
                        detail: "recovered staging task has no committed snapshot".to_owned(),
                    })?;
            self.forge
                .stamp_recovered_staging_task(lease, binding, &task.plan.inputs, snapshot_id)
                .await?;
        }
        let expiry_terminals = if task.strategy == ForgeTaskStrategy::SnapshotExpiry {
            let key = super::compact::ForgeTableKey {
                tenant: task.data_tenant_id,
                table_ref: binding.table_ref.clone(),
            };
            self.matching_prepared_expiry(&key, binding, &task.table_ref, &table, evidence)
                .await?
        } else {
            Vec::new()
        };
        self.resume_expired_cleanup(
            CleanupAttempt {
                task_id: task.task_id,
                tenant: task.data_tenant_id,
                attempt,
                binding,
            },
            lease,
            evidence,
            stop,
        )
        .await?;
        if task.strategy == ForgeTaskStrategy::SnapshotExpiry {
            let key = super::compact::ForgeTableKey {
                tenant: task.data_tenant_id,
                table_ref: binding.table_ref.clone(),
            };
            self.forge
                .finalize_expiry_terminals(lease, task.data_tenant_id, expiry_terminals)
                .await?;
            ForgeMaintenance::new(Arc::clone(&self.forge))
                .collect_never_published(lease, &key, binding, stop)
                .await?;
        }
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        Ok(())
    }

    /// Finds the exact open expiry operation whose candidates match task evidence.
    ///
    /// # Errors
    ///
    /// Returns SQL, catalog, traversal, evidence, or audit failures, and fails
    /// closed when more than one open operation matches the durable task.
    async fn matching_prepared_expiry(
        &self,
        key: &super::compact::ForgeTableKey,
        binding: &TenantTableBinding,
        identity: &ForgeTaskTableIdentity,
        table: &Table,
        evidence: &ForgeTaskEvidence,
    ) -> Result<Vec<PendingExpiryTerminal>, ForgeError> {
        let resource = table_resource_for_key(key);
        let mut conn = self
            .forge
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let page = ForgeOperations::new(&resource, ForgeOperationFamily::SnapshotExpire)
            .map_err(ForgeError::Sql)?
            .list_open(
                &mut conn,
                self.forge.core.config.max_open_operations_per_table,
            )
            .await
            .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        if page.overflowed {
            return Err(ForgeError::Reconciliation {
                detail: "open expiry operations exceeded the recovery bound".to_owned(),
            });
        }
        let open_count = page.operations.len();
        if open_count == 0 {
            return Ok(Vec::new());
        }
        let mut matching = Vec::new();
        for row in page.operations {
            let wyrd_spec::vala::api::AuditDetail::ForgeSnapshotExpire {
                base_metadata_location,
                selected_snapshot_ids,
                ..
            } = &row.prepared_detail
            else {
                continue;
            };
            if selected_snapshot_ids
                .iter()
                .any(|id| table.metadata().snapshot_by_id(*id).is_some())
            {
                continue;
            }
            let expired = derive_recovered_files(
                table,
                base_metadata_location.as_str(),
                &self.forge.core.config,
            )
            .await?;
            let candidate_result = ForgeMaintenanceResult {
                table: table.clone(),
                expired_files: expired,
                expiry_terminals: Vec::new(),
            };
            if cleanup_candidates(
                &candidate_result,
                binding,
                &self.forge.core.staging,
                identity,
            )? == evidence.cleanup_candidates
            {
                matching.push(PendingExpiryTerminal {
                    detail: row.prepared_detail,
                    recovered: true,
                });
            }
        }
        if matching.len() != 1 {
            return Err(ForgeError::Reconciliation {
                detail: format!(
                    "expected exactly one of {open_count} open expiry operations to match task evidence; matched {}",
                    matching.len()
                ),
            });
        }
        Ok(matching)
    }

    /// Validates the closed T13 strategy and payload contract without IO.
    ///
    /// # Errors
    ///
    /// Returns an invariant error for an unknown or reserved strategy,
    /// maintenance strategy, malformed parameters, or an empty exact input set.
    fn validate_payload(task: &ForgeTaskClaim) -> Result<ForgeMetricStage, ForgeError> {
        if task.plan.inputs.is_empty() {
            return Err(ForgeError::Invariant {
                detail: "Forge task payload has no exact inputs".to_owned(),
            });
        }
        let (expected_kind, stage) = match &task.strategy {
            ForgeClaimStrategy::Known(ForgeTaskStrategy::StagingFold) => {
                ("staging_fold", ForgeMetricStage::StagingFold)
            }
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles) => {
                ("live_rewrite", ForgeMetricStage::IcebergRewrite)
            }
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry) => {
                ("maintenance", ForgeMetricStage::SnapshotExpiry)
            }
            ForgeClaimStrategy::Known(
                ForgeTaskStrategy::FullIdentity
                | ForgeTaskStrategy::ManifestRewrite
                | ForgeTaskStrategy::ExpiredCleanup
                | ForgeTaskStrategy::OrphanCleanup,
            )
            | ForgeClaimStrategy::Unknown(_) => {
                return Err(ForgeError::Invariant {
                    detail: format!("unsupported Forge task strategy {}", task.strategy.as_str()),
                });
            }
        };
        let parameters = task
            .plan
            .parameters
            .as_object()
            .ok_or_else(|| ForgeError::Invariant {
                detail: "Forge task parameters must be an object".to_owned(),
            })?;
        let valid_parameters = parameters.get("kind").and_then(serde_json::Value::as_str)
            == Some(expected_kind)
            && match task.strategy {
                ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry) => {
                    parameters.len() == 2
                        && parameters
                            .get("trigger_commit_count")
                            .and_then(serde_json::Value::as_u64)
                            .is_some()
                }
                _ => parameters.len() == 1,
            };
        if !valid_parameters {
            return Err(ForgeError::Invariant {
                detail: "Forge task parameters do not match the strategy contract".to_owned(),
            });
        }
        Ok(stage)
    }

    /// Runs one supported task after acquiring the publication lease.
    ///
    /// # Cancellation
    ///
    /// Two cancellation tokens scope this operation. `operation_stop` is a
    /// child of the caller's shutdown token; non-maintenance strategies observe
    /// it, so graceful shutdown cooperatively cancels them and they release
    /// cleanly. `authority_stop` is a fresh token that shutdown never reaches;
    /// the heartbeat cancels it (alongside `operation_stop`) only on genuine
    /// authority loss — claim-heartbeat failure or lease-renew loss. Maintenance
    /// strategies observe `authority_stop`, so a graceful shutdown does not
    /// cancel an in-flight Iceberg maintenance operation: it completes through
    /// to its own outcome, bounded by `iceberg_total_retry_timeout` and, past
    /// the server drain window, by the supervisor `abort_all` plus lease-expiry
    /// recovery (the accepted fallback, not asserted against). Maintenance
    /// cancellation therefore means fence loss only, which conservatively
    /// retains a possibly-committed effect for successor recovery.
    ///
    /// A cooperative cancellation observed before the durable effect (dispatch
    /// stopped mid-rewrite before its catalog commit, or a maintenance task
    /// stopped before its `prepared()` boundary) propagates unchanged as
    /// [`ForgeError::Shutdown`]; the caller [`Self::run_slot`] owns the single
    /// release seam and drains the claim to `retryable` through
    /// [`Self::release_cancelled_claim`] for a clean shutdown. A cancellation
    /// observed after the durable effect — the fresh catalog commit or recovered
    /// committed snapshot below, whose task row is still `running` — instead
    /// returns [`ForgeError::ShutdownRetained`] so `run_slot` retains it for
    /// evidence-based recovery rather than releasing it.
    ///
    /// # Errors
    ///
    /// Returns catalog, stale-snapshot, lifecycle, heartbeat, rewrite, evidence,
    /// object-store, fence, cancellation, or audit failures.
    /// Acquires the durable plan's exact memory and scratch before rewrite IO.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Capacity`] when the claim's estimates fail this
    /// worker's validated ceilings, the platform cannot represent the planned
    /// memory, or the global resource owner cannot grant both counters, and
    /// [`ForgeError::Invariant`] when the attempt runtime loses its leased pool.
    fn acquire_rewrite_resources(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
    ) -> Result<super::rewrite::ForgeAttemptResources, ForgeError> {
        let request =
            crate::resources::ForgeRewriteRequest::from_claim(&claim.estimates, self.capacity)?;
        super::rewrite::ForgeAttemptResources::acquire(
            &self.forge.core.resources,
            request,
            binding,
            &self.forge.core.rewrite_spill_root,
            claim.task_id,
            attempt,
            Arc::clone(&self.forge.core.telemetry),
        )
    }

    async fn execute_fenced(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let resources = self.acquire_rewrite_resources(claim, attempt, binding)?;
        let rewrite = resources.pipeline(&self.forge.core.rewrite);
        let table = self.forge.load_table(&binding.table_ident()).await?;
        let base_matches = Self::base_snapshot_matches(&table, claim.base_snapshot_id);
        let committed_recovery = if base_matches {
            None
        } else {
            self.find_retained_task_evidence(binding, &table, claim.task_id)
                .await?
        };
        let maintenance_recovery = matches!(
            claim.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry)
        );
        if !base_matches && committed_recovery.is_none() && !maintenance_recovery {
            self.forge
                .core
                .telemetry
                .record_conflict(ForgeConflictKind::SnapshotChanged);
            self.cancel_superseded(claim).await?;
            return Ok(());
        }
        let watermark = Self::execution_watermark(&table, claim, committed_recovery.as_ref())?;
        self.begin_attempt(claim, attempt, watermark).await?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        // `operation_stop` is a shutdown-sensitive child token: graceful shutdown
        // (or the post-effect drain below) cancels it, and non-maintenance
        // strategies observe it so cooperative cancel with clean release stays
        // their drain behavior. `authority_stop` is a fresh token that shutdown
        // never reaches; only genuine authority loss (claim-heartbeat failure or
        // lease-renew loss, propagated by the heartbeat) cancels it. Maintenance
        // strategies observe `authority_stop` so an in-flight Iceberg maintenance
        // operation completes through a graceful shutdown — bounded by
        // `iceberg_total_retry_timeout` and, past the drain window, by the
        // supervisor `abort_all` plus lease-expiry recovery — while still
        // conservatively retaining a possibly-committed effect on fence loss.
        let operation_stop = shutdown.child_token();
        let authority_stop = CancellationToken::new();
        let heartbeat = self.spawn_authority_heartbeat(
            claim.task_id,
            attempt,
            lease.clone(),
            operation_stop.clone(),
            authority_stop.clone(),
        )?;
        let dispatch_stop = if maintenance_recovery {
            &authority_stop
        } else {
            &operation_stop
        };
        let execution = match committed_recovery {
            Some(evidence) => Ok((evidence, ForgeExecutionEvidenceState::RecoveredCommit)),
            None => {
                match self
                    .dispatch_claim(ForgeDispatchRequest {
                        claim,
                        attempt,
                        binding,
                        lease,
                        table,
                        stop: dispatch_stop,
                        rewrite: &rewrite,
                    })
                    .await
                {
                    Ok(ForgeDispatchResult::Committed(committed)) => self
                        .committed_evidence(binding, &committed)
                        .await
                        .map(|evidence| (evidence, ForgeExecutionEvidenceState::Fresh)),
                    Ok(ForgeDispatchResult::Maintenance(result)) => self
                        .complete_maintenance(
                            claim,
                            attempt,
                            binding,
                            lease,
                            *result,
                            dispatch_stop,
                        )
                        .await
                        .map(|evidence| (evidence, ForgeExecutionEvidenceState::Prepared)),
                    Err(error) => Err(error),
                }
            }
        };
        // A pre-effect cancellation surfaces as `execution == Err(Shutdown)`
        // (dispatch stopped mid-rewrite before its catalog commit, or a
        // maintenance task stopped before its `prepared()` boundary) and
        // propagates unchanged as `ForgeError::Shutdown`, which the single
        // `run_slot` release seam drains through `release_cancelled_claim`. The
        // post-effect cancellation at the check below instead has a durable
        // effect already committed while its task row is still `running`, so it
        // propagates as `ForgeError::ShutdownRetained` to keep `run_slot` from
        // matching and releasing it; it stays retained for evidence-based and
        // lease-expiry recovery.
        let completion = async {
            let evidence = execution?;
            if operation_stop.is_cancelled() {
                return Err(ForgeError::ShutdownRetained);
            }
            Ok(evidence)
        }
        .await;
        operation_stop.cancel();
        let heartbeat_result = heartbeat.await.map_err(|error| ForgeError::Invariant {
            detail: format!("Forge claim heartbeat panicked: {error}"),
        })?;
        heartbeat_result?;
        let (evidence, state) = completion?;
        self.record_rewrite_evidence(claim, &evidence);
        self.finish_claim_execution(claim, attempt, lease, &evidence, state)
            .await
    }

    /// Publishes the rewrite and catalog-commit observer events for a completed
    /// execution.
    ///
    /// This runs only after the durable completion path produced evidence, so
    /// test observation cannot acknowledge or alter a task before its effect is
    /// committed. The catalog-committed event is emitted only when the evidence
    /// carries a committed snapshot.
    fn record_rewrite_evidence(&self, claim: &ForgeTaskClaim, evidence: &ForgeTaskEvidence) {
        let Some(observer) = &self.completion_observer else {
            return;
        };
        observer.record_lifecycle(ForgeLifecycleEvent::Rewritten {
            task_id: claim.task_id,
            input_count: claim.plan.inputs.len(),
        });
        if let Some(snapshot_id) = evidence.committed_snapshot_id {
            observer.record_lifecycle(ForgeLifecycleEvent::CatalogCommitted {
                task_id: claim.task_id,
                snapshot_id,
            });
        }
    }

    /// Refreshes final authority and commits the appropriate success transition.
    ///
    /// # Errors
    ///
    /// Returns heartbeat, fencing, evidence, audit, SQL, or replan failures.
    async fn finish_claim_execution(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        lease: &mut ForgeLease,
        evidence: &ForgeTaskEvidence,
        state: ForgeExecutionEvidenceState,
    ) -> Result<(), ForgeError> {
        self.tasks
            .heartbeat(
                claim.task_id,
                attempt,
                self.owner,
                self.claim_limits()?.lease_seconds,
            )
            .await
            .map_err(ForgeError::Sql)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        if matches!(state, ForgeExecutionEvidenceState::RecoveredCommit)
            && matches!(
                claim.strategy,
                ForgeClaimStrategy::Known(ForgeTaskStrategy::StagingFold)
            )
        {
            let snapshot_id =
                evidence
                    .committed_snapshot_id
                    .ok_or_else(|| ForgeError::Reconciliation {
                        detail: "recovered staging task has no committed snapshot".to_owned(),
                    })?;
            let binding = task_table_binding(
                claim.data_tenant_id,
                claim.execution_tenant_id,
                &claim.table_ref,
            )?;
            self.forge
                .stamp_recovered_staging_task(lease, &binding, &claim.plan.inputs, snapshot_id)
                .await?;
        }
        match state {
            ForgeExecutionEvidenceState::Fresh | ForgeExecutionEvidenceState::RecoveredCommit => {
                self.persist_success(claim, attempt, lease, evidence).await
            }
            ForgeExecutionEvidenceState::Prepared => {
                self.persist_terminal_success(
                    claim.task_id,
                    claim.data_tenant_id,
                    &claim.table_ref,
                    attempt,
                    lease,
                    task_progress_effect(
                        &claim.strategy,
                        claim.base_snapshot_id,
                        &claim.plan.parameters,
                        evidence.committed_snapshot_id != Some(claim.base_snapshot_id)
                            || evidence.deleted_candidate_count > 0,
                    ),
                )
                .await
            }
        }
    }

    /// Selects the retained snapshot protected while one claimed task runs.
    ///
    /// Normal execution protects the exact planning base. Recovery protects
    /// the task-tagged commit only when the original base has already expired.
    ///
    /// # Errors
    ///
    /// Returns reconciliation failure when neither the planning base nor the
    /// exact committed recovery snapshot remains retained.
    fn execution_watermark(
        table: &Table,
        claim: &ForgeTaskClaim,
        committed_recovery: Option<&ForgeTaskEvidence>,
    ) -> Result<SnapshotWatermark, ForgeError> {
        if let Some(base) = table.metadata().snapshot_by_id(claim.base_snapshot_id) {
            return Ok(SnapshotWatermark {
                snapshot_id: claim.base_snapshot_id,
                timestamp_ms: base.timestamp_ms(),
            });
        }
        if claim.base_snapshot_id == 0 && table.metadata().current_snapshot_id().is_none() {
            return Ok(SnapshotWatermark {
                snapshot_id: 0,
                timestamp_ms: 0,
            });
        }
        if matches!(
            claim.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry)
        ) && let Some(current) = table.metadata().current_snapshot()
        {
            return Ok(SnapshotWatermark {
                snapshot_id: current.snapshot_id(),
                timestamp_ms: current.timestamp_ms(),
            });
        }
        let evidence = committed_recovery.ok_or_else(|| ForgeError::Reconciliation {
            detail: "Forge task base snapshot is no longer retained".to_owned(),
        })?;
        let snapshot_id =
            evidence
                .committed_snapshot_id
                .ok_or_else(|| ForgeError::Reconciliation {
                    detail: "committed Forge recovery has no snapshot evidence".to_owned(),
                })?;
        let committed = table
            .metadata()
            .snapshot_by_id(snapshot_id)
            .ok_or_else(|| ForgeError::Reconciliation {
                detail: "committed Forge recovery snapshot is no longer retained".to_owned(),
            })?;
        Ok(SnapshotWatermark {
            snapshot_id: committed.snapshot_id(),
            timestamp_ms: committed.timestamp_ms(),
        })
    }

    /// Dispatches one validated task to its exact existing rewrite owner.
    ///
    /// # Errors
    ///
    /// Returns stale-snapshot, exact-input, rewrite, catalog, fencing, or
    /// unsupported-strategy failures.
    async fn dispatch_claim(
        &self,
        request: ForgeDispatchRequest<'_>,
    ) -> Result<ForgeDispatchResult, ForgeError> {
        let ForgeDispatchRequest {
            claim,
            attempt,
            binding,
            lease,
            table,
            stop,
            rewrite,
        } = request;
        if matches!(
            claim.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::StagingFold)
        ) && let Some(recovered) = self
            .forge
            .recover_interrupted_staging_task(lease, binding, &claim.plan.inputs, &table)
            .await?
        {
            return Ok(ForgeDispatchResult::Committed(recovered));
        }
        if Self::current_snapshot_matches_task(&table, claim.task_id) {
            return Ok(ForgeDispatchResult::Committed(table));
        }
        if !Self::base_snapshot_matches(&table, claim.base_snapshot_id)
            && !matches!(
                claim.strategy,
                ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry)
            )
        {
            return Err(ForgeError::Reconciliation {
                detail: "Forge task base snapshot changed before execution".to_owned(),
            });
        }
        match &claim.strategy {
            ForgeClaimStrategy::Known(ForgeTaskStrategy::StagingFold) => self
                .forge
                .execute_staging_task(super::compact::StagingTaskRequest {
                    rewrite,
                    lease,
                    binding,
                    inputs: &claim.plan.inputs,
                    task_id: claim.task_id,
                    attempt_id: attempt,
                    stop,
                })
                .await
                .map(ForgeDispatchResult::Committed),
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles) => {
                self.dispatch_small_files(ForgeDispatchRequest {
                    claim,
                    attempt,
                    binding,
                    lease,
                    table,
                    stop,
                    rewrite,
                })
                .await
            }
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry) => {
                let key = super::compact::ForgeTableKey {
                    tenant: claim.data_tenant_id,
                    table_ref: binding.table_ref.clone(),
                };
                self.forge
                    .reconcile_live_replacements(
                        lease,
                        &key,
                        binding,
                        stop,
                        self.forge.core.clock.now()?,
                    )
                    .await?;
                ForgeMaintenance::new(Arc::clone(&self.forge))
                    .execute(lease, &key, binding, table, &claim.plan.inputs, stop)
                    .await
                    .map(Box::new)
                    .map(ForgeDispatchResult::Maintenance)
            }
            _ => Err(ForgeError::Invariant {
                detail: "unsupported task passed pre-effect validation".to_owned(),
            }),
        }
    }

    /// Marks one claimed task running and extends its claim lease once.
    ///
    /// Both transitions happen before any external effect, so a worker that
    /// dies immediately after them leaves an ordinary expired claim the
    /// bounded reclaim path recovers.
    ///
    /// # Errors
    ///
    /// Returns configuration failures from the worker's claim limits and SQL
    /// failures from the durable start or heartbeat transition.
    async fn begin_attempt(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        watermark: vala_sql::row_types::forge_tasks::SnapshotWatermark,
    ) -> Result<(), ForgeError> {
        self.tasks
            .start(claim.task_id, attempt, self.owner, watermark)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .heartbeat(
                claim.task_id,
                attempt,
                self.owner,
                self.claim_limits()?.lease_seconds,
            )
            .await
            .map_err(ForgeError::Sql)
    }

    /// Executes one exact small-file task against its rediscovered live group.
    ///
    /// The persisted input list is authoritative: the group is rediscovered
    /// from the loaded snapshot and rewritten under the same attempt fence and
    /// operation lease. A non-committing disposition is an invariant failure
    /// for an exact task, because the caller already proved the base snapshot
    /// still matches.
    ///
    /// # Errors
    ///
    /// Returns discovery, rewrite, catalog, fencing, or reconciliation
    /// failures, including a disposition that performed no committed work.
    async fn dispatch_small_files(
        &self,
        request: ForgeDispatchRequest<'_>,
    ) -> Result<ForgeDispatchResult, ForgeError> {
        let ForgeDispatchRequest {
            claim,
            attempt,
            binding,
            lease,
            table,
            stop,
            rewrite,
        } = request;
        let group = self
            .forge
            .discover_exact_live_group(binding, &table, &claim.plan.inputs)
            .await?;
        let TaskLiveRewriteResult {
            disposition,
            committed_table,
        } = self
            .forge
            .replace_live_group_for_task(TaskLiveRewriteRequest {
                rewrite,
                lease,
                binding,
                table: &table,
                base_snapshot_id: claim.base_snapshot_id,
                group: &group,
                task_id: claim.task_id,
                attempt_id: attempt,
                stop,
            })
            .await?;
        match (disposition, committed_table) {
            (IcebergRewriteDisposition::Committed { spill_bytes, .. }, Some(table)) => {
                self.forge
                    .core
                    .telemetry
                    .record_task_spill(ForgeTaskMetricStrategy::SmallFiles, spill_bytes);
                Ok(ForgeDispatchResult::Committed(table))
            }
            (IcebergRewriteDisposition::NoWork | IcebergRewriteDisposition::SnapshotChanged, _) => {
                Err(ForgeError::Reconciliation {
                    detail: "exact small-file task did not commit".to_owned(),
                })
            }
            (IcebergRewriteDisposition::Committed { .. }, None) => {
                Err(ForgeError::Reconciliation {
                    detail: "committed rewrite lost its returned table".to_owned(),
                })
            }
        }
    }

    /// Persists exact cleanup evidence, deletes it with a durable cursor, then runs orphan GC.
    ///
    /// Its production span carries only closed strategy/result/role/kind fields
    /// and the scrubbed durable task and attempt UUIDs.
    ///
    /// Candidate derivation has already completed from before/after Iceberg
    /// metadata. This method records the complete ordered set before the first
    /// delete, advances the cursor after every idempotent object result, and
    /// only then enters the disjoint never-published generation collector.
    ///
    /// # Errors
    ///
    /// Returns path binding, evidence, SQL, audit, fencing, object-store, or
    /// cancellation failures. A failure retains Prepared evidence and its last
    /// committed cursor for deterministic takeover.
    async fn complete_maintenance(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        result: ForgeMaintenanceResult,
        stop: &CancellationToken,
    ) -> Result<ForgeTaskEvidence, ForgeError> {
        let started = Instant::now();
        let span = tracing::info_span!(
            "bifrost.forge.cleanup",
            kind = "expired",
            strategy = claim.strategy.as_str(),
            result = tracing::field::Empty,
            role = "forge_worker",
            task_id = %claim.task_id,
            attempt_id = %attempt,
        );
        let result = tracing::Instrument::instrument(
            self.complete_maintenance_inner(claim, attempt, binding, lease, result, stop),
            span.clone(),
        )
        .await;
        span.record(
            "result",
            if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
        );
        if result.is_ok() {
            self.record_expired_cleanup_completion(started);
        }
        result
    }

    /// Persist and drain exact cleanup evidence after maintenance commits.
    ///
    /// # Errors
    ///
    /// Returns path binding, evidence, SQL, audit, fencing, object-store, or
    /// cancellation failures while retaining the last durable cleanup cursor.
    ///
    /// # Cancellation
    ///
    /// Cancellation stops before the next durable or object-store effect.
    /// A delete racing cancellation may already be accepted; its idempotent
    /// candidate remains behind the durable cursor for successor replay.
    async fn complete_maintenance_inner(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        result: ForgeMaintenanceResult,
        stop: &CancellationToken,
    ) -> Result<ForgeTaskEvidence, ForgeError> {
        require_running(stop)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let candidates =
            cleanup_candidates(&result, binding, &self.forge.core.staging, &claim.table_ref)?;
        let mut evidence = self.committed_evidence(binding, &result.table).await?;
        evidence.cleanup_candidates = candidates;
        evidence.validate(false).map_err(ForgeError::Sql)?;

        require_running(stop)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let mut prepared = self
            .forge
            .core
            .vala
            .tenant_conn(claim.data_tenant_id)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .prepared(
                &mut prepared,
                claim.task_id,
                attempt,
                self.owner,
                &evidence,
                &task_event(
                    claim.task_id,
                    ForgeTaskState::Prepared,
                    "exact expired-file cleanup evidence persisted",
                ),
            )
            .await
            .map_err(ForgeError::Sql)?;
        lease.assert_transaction_fence(&mut prepared).await?;
        prepared.commit().await.map_err(ForgeError::Sql)?;
        #[cfg(feature = "test-support")]
        if self
            .fail_after_maintenance_prepared
            .swap(false, Ordering::AcqRel)
        {
            return Err(ForgeError::Reconciliation {
                detail: "injected crash after maintenance task evidence commit".to_owned(),
            });
        }
        require_running(stop)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        self.forge
            .finalize_expiry_terminals(lease, claim.data_tenant_id, result.expiry_terminals)
            .await?;

        let cleanup_attempt = CleanupAttempt {
            task_id: claim.task_id,
            tenant: claim.data_tenant_id,
            attempt,
            binding,
        };
        for (index, candidate) in evidence.cleanup_candidates.iter().enumerate() {
            evidence.deleted_candidate_count = self
                .delete_cleanup_candidate(&cleanup_attempt, lease, candidate, index, stop)
                .await?;
        }
        let key = super::compact::ForgeTableKey {
            tenant: claim.data_tenant_id,
            table_ref: binding.table_ref.clone(),
        };
        ForgeMaintenance::new(Arc::clone(&self.forge))
            .collect_never_published(lease, &key, binding, stop)
            .await?;
        Ok(evidence)
    }

    /// Deletes one table-bound cleanup candidate and commits its successor cursor.
    ///
    /// A missing object counts as an idempotent successful deletion. The cursor
    /// advances only after the object-store result is accepted and the lease
    /// fence is rechecked inside the tenant transaction.
    ///
    /// # Errors
    ///
    /// Returns cancellation, path-binding, object-store, cursor conversion,
    /// fencing, or durable SQL failures. A failed cursor commit leaves the
    /// candidate replayable by the current or successor lease owner.
    async fn delete_cleanup_candidate(
        &self,
        attempt: &CleanupAttempt<'_>,
        lease: &mut ForgeLease,
        candidate: &ForgeCleanupCandidate,
        index: usize,
        stop: &CancellationToken,
    ) -> Result<u32, ForgeError> {
        require_running(stop)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let path = attempt
            .binding
            .validate_object_path(candidate.path.as_str())
            .ok_or_else(|| ForgeError::Reconciliation {
                detail: "expired cleanup candidate escaped table binding".to_owned(),
            })?;
        let deletion = self.forge.core.object_store.delete(&path);
        tokio::pin!(deletion);
        let deletion = tokio::select! {
            result = &mut deletion => result,
            () = stop.cancelled() => return Err(ForgeError::Shutdown),
        };
        match deletion {
            Ok(()) => {}
            Err(error) if error.kind() == opendal::ErrorKind::NotFound => {}
            Err(error) => return Err(ForgeError::ObjectDelete(error)),
        }
        let expected = u32::try_from(index).map_err(|_| ForgeError::Invariant {
            detail: "expired cleanup cursor exceeds u32".to_owned(),
        })?;
        let next = expected
            .checked_add(1)
            .ok_or_else(|| ForgeError::Invariant {
                detail: "expired cleanup cursor overflowed".to_owned(),
            })?;
        require_running(stop)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let mut cursor = self
            .forge
            .core
            .vala
            .tenant_conn(attempt.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .advance_cleanup_cursor(
                &mut cursor,
                attempt.task_id,
                attempt.attempt,
                self.owner,
                expected,
                next,
            )
            .await
            .map_err(ForgeError::Sql)?;
        lease.assert_transaction_fence(&mut cursor).await?;
        cursor.commit().await.map_err(ForgeError::Sql)?;
        Ok(next)
    }

    /// Record the completed durable expired-cleanup obligation at its owner boundary.
    fn record_expired_cleanup_completion(&self, started: Instant) {
        self.forge
            .core
            .telemetry
            .record_cleanup(ForgeCleanupKind::Expired, started.elapsed());
    }

    /// Resumes the undeleted suffix of exact Prepared cleanup evidence.
    ///
    /// # Errors
    ///
    /// Returns malformed cursor, binding, fencing, object-store, SQL, or
    /// cancellation failures. Each successful object result and cursor update
    /// is idempotent, so another owner resumes at the first uncommitted index.
    ///
    /// # Cancellation
    ///
    /// Cancellation stops before the next delete or cursor transition. A
    /// remotely accepted delete without a cursor advance is replayed safely.
    async fn resume_expired_cleanup(
        &self,
        cleanup: CleanupAttempt<'_>,
        lease: &mut ForgeLease,
        evidence: &ForgeTaskEvidence,
        stop: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let start = usize::try_from(evidence.deleted_candidate_count).map_err(|_| {
            ForgeError::Invariant {
                detail: "Prepared cleanup cursor exceeds usize".to_owned(),
            }
        })?;
        for (index, candidate) in evidence.cleanup_candidates.iter().enumerate().skip(start) {
            if stop.is_cancelled() {
                return Err(ForgeError::Shutdown);
            }
            lease.require_fence(&self.forge.core.operator_pool).await?;
            let path = cleanup
                .binding
                .validate_object_path(candidate.path.as_str())
                .ok_or_else(|| ForgeError::Reconciliation {
                    detail: "Prepared cleanup candidate escaped table binding".to_owned(),
                })?;
            let deletion = self.forge.core.object_store.delete(&path);
            tokio::pin!(deletion);
            let deletion = tokio::select! {
                result = &mut deletion => result,
                () = stop.cancelled() => return Err(ForgeError::Shutdown),
            };
            match deletion {
                Ok(()) => {}
                Err(error) if error.kind() == opendal::ErrorKind::NotFound => {}
                Err(error) => return Err(ForgeError::ObjectDelete(error)),
            }
            let expected = u32::try_from(index).map_err(|_| ForgeError::Invariant {
                detail: "Prepared cleanup cursor exceeds u32".to_owned(),
            })?;
            let next = expected
                .checked_add(1)
                .ok_or_else(|| ForgeError::Invariant {
                    detail: "Prepared cleanup cursor overflowed".to_owned(),
                })?;
            require_running(stop)?;
            lease.require_fence(&self.forge.core.operator_pool).await?;
            let mut conn = self
                .forge
                .core
                .vala
                .tenant_conn(cleanup.tenant)
                .await
                .map_err(ForgeError::Sql)?;
            self.tasks
                .advance_cleanup_cursor(
                    &mut conn,
                    cleanup.task_id,
                    cleanup.attempt,
                    self.owner,
                    expected,
                    next,
                )
                .await
                .map_err(ForgeError::Sql)?;
            lease.assert_transaction_fence(&mut conn).await?;
            conn.commit().await.map_err(ForgeError::Sql)?;
        }
        Ok(())
    }

    /// Starts coordinated task-claim and table-lease renewal for one operation.
    ///
    /// Either authority loss cancels BOTH the shutdown-sensitive
    /// `operation_stop` and the authority-only `authority_stop` tokens, so a
    /// maintenance strategy observing `authority_stop` still aborts on genuine
    /// fence loss even though graceful shutdown never reaches that token. The
    /// task heartbeat also renews a large-lane reservation when the claim owns
    /// one, while the cloned table lease retains the same publication fence
    /// token. The renewal loop exits cleanly when `operation_stop` is cancelled
    /// (graceful shutdown or the caller's post-effect drain).
    ///
    /// # Errors
    ///
    /// Returns invalid configuration when the claim TTL cannot fit seconds.
    fn spawn_authority_heartbeat(
        &self,
        task_id: Uuid,
        attempt: Uuid,
        mut lease: ForgeLease,
        operation_stop: CancellationToken,
        authority_stop: CancellationToken,
    ) -> Result<tokio::task::JoinHandle<Result<(), ForgeError>>, ForgeError> {
        let lease_seconds =
            u32::try_from(self.forge.core.config.lease_ttl.as_secs()).map_err(|_| {
                ForgeError::InvalidConfig {
                    detail: "Forge claim TTL exceeds u32 seconds".to_owned(),
                }
            })?;
        let interval =
            (self.forge.core.config.lease_ttl / 3).min(self.forge.core.maintenance_interval);
        let tasks = self.tasks.clone();
        let operator_pool = self.forge.core.operator_pool.clone();
        let owner = self.owner;
        Ok(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ticker.tick().await;
            loop {
                tokio::select! {
                    biased;
                    () = operation_stop.cancelled() => return Ok(()),
                    _ = ticker.tick() => {
                        if let Err(error) = tasks.heartbeat(task_id, attempt, owner, lease_seconds).await {
                            operation_stop.cancel();
                            authority_stop.cancel();
                            return Err(ForgeError::Sql(error));
                        }
                        match lease.renew(&operator_pool).await {
                            Ok(true) => {}
                            Ok(false) => {
                                operation_stop.cancel();
                                authority_stop.cancel();
                                return Err(ForgeError::FenceLost {
                                    lease_key: lease.lease_key.clone(),
                                });
                            }
                            Err(error) => {
                                operation_stop.cancel();
                                authority_stop.cancel();
                                return Err(error);
                            }
                        }
                    }
                }
            }
        }))
    }

    /// Derives exact raw-byte metadata evidence from the returned table.
    ///
    /// # Errors
    ///
    /// Returns missing-location, path-binding, object read, or snapshot errors.
    /// Read/hash failure leaves the task Running for recovery and never stores a
    /// fabricated digest.
    async fn committed_evidence(
        &self,
        binding: &TenantTableBinding,
        table: &Table,
    ) -> Result<ForgeTaskEvidence, ForgeError> {
        let snapshot_id =
            table
                .metadata()
                .current_snapshot_id()
                .ok_or_else(|| ForgeError::Reconciliation {
                    detail: "committed Forge table has no current snapshot".to_owned(),
                })?;
        let location = table
            .metadata_location_result()
            .map_err(ForgeError::Catalog)?
            .to_owned();
        let object_path = catalog_path_to_object_key(
            table.metadata().location(),
            binding,
            &self.forge.core.staging,
            &location,
        )?;
        let raw = self
            .forge
            .core
            .object_store
            .read(&object_path)
            .await
            .map_err(ForgeError::ObjectStore)?;
        let digest = format!("sha256:{}", hex::encode(Sha256::digest(raw.to_bytes())));
        let evidence = ForgeTaskEvidence {
            version: 1,
            committed_snapshot_id: Some(snapshot_id),
            committed_metadata_location: Some(location),
            committed_metadata_digest: Some(digest),
            cleanup_candidates: Vec::new(),
            deleted_candidate_count: 0,
        };
        evidence.validate(false).map_err(ForgeError::Sql)?;
        Ok(evidence)
    }

    /// Locates the earliest retained metadata object whose current snapshot was
    /// committed by one exact Forge task.
    ///
    /// The metadata log is searched oldest-to-newest before the current
    /// location so a later property-only catalog commit cannot replace the
    /// task's original raw-byte evidence. Search is bounded by the configured
    /// retained-snapshot ceiling and every candidate is table-path validated.
    ///
    /// # Errors
    ///
    /// Returns catalog, object-read, path-binding, malformed metadata, or
    /// retention errors. A failure leaves the claim nonterminal and never
    /// fabricates evidence.
    async fn find_retained_task_evidence(
        &self,
        binding: &TenantTableBinding,
        table: &Table,
        task_id: Uuid,
    ) -> Result<Option<ForgeTaskEvidence>, ForgeError> {
        let mut locations = table
            .metadata()
            .metadata_log()
            .iter()
            .map(|entry| entry.metadata_file.clone())
            .collect::<Vec<_>>();
        locations.push(
            table
                .metadata_location_result()
                .map_err(ForgeError::Catalog)?
                .to_owned(),
        );
        let max_locations = self
            .forge
            .core
            .config
            .max_retained_snapshots_per_table
            .checked_add(1)
            .ok_or_else(|| ForgeError::InvalidConfig {
                detail: "Forge retained metadata search limit overflowed".to_owned(),
            })?;
        if locations.len() > max_locations {
            return Err(ForgeError::Reconciliation {
                detail: format!(
                    "retained Forge metadata search has {} locations above limit {max_locations}",
                    locations.len()
                ),
            });
        }
        for location in locations {
            let object_path = catalog_path_to_object_key(
                table.metadata().location(),
                binding,
                &self.forge.core.staging,
                &location,
            )?;
            let raw = self
                .forge
                .core
                .object_store
                .read(&object_path)
                .await
                .map_err(ForgeError::ObjectStore)?;
            let metadata = TableMetadata::read_from(table.file_io(), &location)
                .await
                .map_err(ForgeError::Catalog)?;
            if metadata.location() != table.metadata().location() {
                return Err(ForgeError::Reconciliation {
                    detail: "retained Forge metadata belongs to another table".to_owned(),
                });
            }
            let Some(snapshot) = metadata.current_snapshot() else {
                continue;
            };
            if snapshot
                .summary()
                .additional_properties
                .get("forge.task_id")
                != Some(&task_id.to_string())
            {
                continue;
            }
            if table
                .metadata()
                .snapshot_by_id(snapshot.snapshot_id())
                .is_none()
            {
                return Err(ForgeError::Reconciliation {
                    detail: "committed Forge recovery snapshot is no longer retained".to_owned(),
                });
            }
            let evidence = ForgeTaskEvidence {
                version: 1,
                committed_snapshot_id: Some(snapshot.snapshot_id()),
                committed_metadata_location: Some(location),
                committed_metadata_digest: Some(format!(
                    "sha256:{}",
                    hex::encode(Sha256::digest(raw.to_bytes()))
                )),
                cleanup_candidates: Vec::new(),
                deleted_candidate_count: 0,
            };
            evidence.validate(false).map_err(ForgeError::Sql)?;
            return Ok(Some(evidence));
        }
        Ok(None)
    }

    /// Verifies that Prepared evidence still names exact durable commit bytes.
    ///
    /// # Errors
    ///
    /// Returns reconciliation, catalog-path, object-read, or digest failures.
    async fn verify_committed_evidence(
        &self,
        binding: &TenantTableBinding,
        table: &Table,
        evidence: &ForgeTaskEvidence,
    ) -> Result<(), ForgeError> {
        evidence.validate(true).map_err(ForgeError::Sql)?;
        let snapshot_id =
            evidence
                .committed_snapshot_id
                .ok_or_else(|| ForgeError::Reconciliation {
                    detail: "Prepared evidence has no committed snapshot".to_owned(),
                })?;
        if table.metadata().snapshot_by_id(snapshot_id).is_none() {
            return Err(ForgeError::Reconciliation {
                detail: "Prepared evidence snapshot is no longer retained".to_owned(),
            });
        }
        let location = evidence
            .committed_metadata_location
            .as_deref()
            .ok_or_else(|| ForgeError::Reconciliation {
                detail: "Prepared evidence has no committed metadata location".to_owned(),
            })?;
        let expected = evidence
            .committed_metadata_digest
            .as_deref()
            .ok_or_else(|| ForgeError::Reconciliation {
                detail: "Prepared evidence has no committed metadata digest".to_owned(),
            })?;
        let object_path = catalog_path_to_object_key(
            table.metadata().location(),
            binding,
            &self.forge.core.staging,
            location,
        )?;
        let raw = self
            .forge
            .core
            .object_store
            .read(&object_path)
            .await
            .map_err(ForgeError::ObjectStore)?;
        let actual = format!("sha256:{}", hex::encode(Sha256::digest(raw.to_bytes())));
        if actual != expected {
            return Err(ForgeError::Reconciliation {
                detail: "Prepared metadata digest does not match exact durable bytes".to_owned(),
            });
        }
        Ok(())
    }

    /// Persists Prepared evidence and Succeeded as two audited tenant transitions.
    ///
    /// # Errors
    ///
    /// Returns tenant connection, task transition, audit, fence, or commit errors.
    async fn persist_success(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        lease: &ForgeLease,
        evidence: &ForgeTaskEvidence,
    ) -> Result<(), ForgeError> {
        let mut prepared = self
            .forge
            .core
            .vala
            .tenant_conn(claim.data_tenant_id)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .prepared(
                &mut prepared,
                claim.task_id,
                attempt,
                self.owner,
                evidence,
                &task_event(
                    claim.task_id,
                    ForgeTaskState::Prepared,
                    "exact committed Iceberg metadata evidence persisted",
                ),
            )
            .await
            .map_err(ForgeError::Sql)?;
        lease.assert_transaction_fence(&mut prepared).await?;
        prepared.commit().await.map_err(ForgeError::Sql)?;

        let mut terminal = self
            .forge
            .core
            .vala
            .tenant_conn(claim.data_tenant_id)
            .await
            .map_err(ForgeError::Sql)?;
        let progress_effect = task_progress_effect(
            &claim.strategy,
            claim.base_snapshot_id,
            &claim.plan.parameters,
            evidence.committed_snapshot_id != Some(claim.base_snapshot_id)
                || evidence.deleted_candidate_count > 0,
        );
        self.tasks
            .terminal_and_request_replan(
                &mut terminal,
                ForgeTaskTransition {
                    task_id: claim.task_id,
                    attempt_id: attempt,
                    owner: self.owner,
                    expected: ForgeTaskState::Prepared,
                    next: ForgeTaskState::Succeeded,
                },
                &claim.table_ref,
                progress_effect,
                &task_event(
                    claim.task_id,
                    ForgeTaskState::Succeeded,
                    "exact Forge task completed",
                ),
            )
            .await
            .map_err(ForgeError::Sql)?;
        lease.assert_transaction_fence(&mut terminal).await?;
        terminal.commit().await.map_err(ForgeError::Sql)?;
        self.forge
            .core
            .telemetry
            .record_demand_transition(ForgeDemandTransitionResult::Continued);
        self.forge.core.telemetry.record_progress_effect(
            if matches!(progress_effect, TaskProgressEffect::Progressed) {
                ForgeProgressEffect::Changed
            } else {
                ForgeProgressEffect::AcknowledgedNoop
            },
        );
        Ok(())
    }

    /// Persists only the terminal transition for already-verified Prepared work.
    ///
    /// # Errors
    ///
    /// Returns tenant connection, exact lifecycle, audit, fence, or commit errors.
    async fn persist_terminal_success(
        &self,
        task_id: Uuid,
        tenant: DataTenantId,
        table_ref: &ForgeTaskTableIdentity,
        attempt: Uuid,
        lease: &ForgeLease,
        progress_effect: TaskProgressEffect,
    ) -> Result<(), ForgeError> {
        let mut terminal = self
            .forge
            .core
            .vala
            .tenant_conn(tenant)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .terminal_and_request_replan(
                &mut terminal,
                ForgeTaskTransition {
                    task_id,
                    attempt_id: attempt,
                    owner: self.owner,
                    expected: ForgeTaskState::Prepared,
                    next: ForgeTaskState::Succeeded,
                },
                table_ref,
                progress_effect,
                &task_event(
                    task_id,
                    ForgeTaskState::Succeeded,
                    "exact Prepared Forge evidence reconciled",
                ),
            )
            .await
            .map_err(ForgeError::Sql)?;
        lease.assert_transaction_fence(&mut terminal).await?;
        terminal.commit().await.map_err(ForgeError::Sql)?;
        self.forge
            .core
            .telemetry
            .record_demand_transition(ForgeDemandTransitionResult::Continued);
        self.forge.core.telemetry.record_progress_effect(
            if matches!(progress_effect, TaskProgressEffect::Progressed) {
                ForgeProgressEffect::Changed
            } else {
                ForgeProgressEffect::AcknowledgedNoop
            },
        );
        Ok(())
    }

    /// Terminally audits a malformed or unsupported claim before external effects.
    ///
    /// # Errors
    ///
    /// Returns tenant transaction, lifecycle, or audit errors.
    async fn fail_before_effect(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        detail: String,
    ) -> Result<(), ForgeError> {
        let mut conn = self
            .forge
            .core
            .vala
            .tenant_conn(claim.data_tenant_id)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .terminal(
                &mut conn,
                ForgeTaskTransition {
                    task_id: claim.task_id,
                    attempt_id: attempt,
                    owner: self.owner,
                    expected: ForgeTaskState::Claimed,
                    next: ForgeTaskState::Failed,
                },
                &task_event(claim.task_id, ForgeTaskState::Failed, &detail),
            )
            .await
            .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        self.request_replan(claim.data_tenant_id, &claim.table_ref)
            .await
    }

    /// Applies the closed bounded-retry policy at the audited worker boundary.
    ///
    /// # Errors
    /// Returns SQL or audit errors; failure retains the fenced claim for reclaim.
    async fn settle_execution_failure(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        error: &ForgeError,
    ) -> Result<(), ForgeError> {
        match error {
            ForgeError::Shutdown => self.release_cancelled_claim(claim.task_id, attempt).await,
            ForgeError::Capacity { .. } => {
                self.record_capacity_refusal(claim.task_id, attempt).await?;
                self.forge
                    .core
                    .telemetry
                    .record_capacity_refusal(ForgeCapacityRefusalPhase::Admission);
                Ok(())
            }
            ForgeError::ShutdownRetained => Ok(()),
            _ => {
                let class = error.failure_class();
                self.forge.core.telemetry.record_failure_class(class);
                let execution_envelope_resource = if error.capacity_failure_phase()
                    == Some(ForgeCapacityFailurePhase::Execution)
                {
                    Some(match error {
                        ForgeError::ExecutionEnvelopeExceeded {
                            resource: "scratch",
                            ..
                        }
                        | ForgeError::SpillLimitExceeded { .. } => ForgeAttemptResource::Scratch,
                        _ => ForgeAttemptResource::Memory,
                    })
                } else {
                    None
                };
                let volume = if class == ForgeFailureClass::StorageHealth {
                    let volume = self.scratch_volume_identity()?;
                    self.tasks
                        .quarantine_worker(self.owner, volume.as_str())
                        .await
                        .map_err(ForgeError::Sql)?;
                    self.forge.core.telemetry.record_quarantine_state(true);
                    Some(volume)
                } else {
                    None
                };
                let attempts = self
                    .tasks
                    .attempt_count(claim.task_id)
                    .await
                    .map_err(ForgeError::Sql)?;
                if failure_is_terminal(class, attempts) {
                    self.terminal_failure(
                        claim,
                        attempt,
                        class,
                        volume.as_ref().map(ScratchVolumeIdentity::as_str),
                        error.to_string(),
                    )
                    .await?;
                    self.forge.core.telemetry.record_terminal_poison(class);
                } else {
                    self.tasks
                        .retry_failure(
                            claim.task_id,
                            attempt,
                            self.owner,
                            class.as_str(),
                            volume.as_ref().map(ScratchVolumeIdentity::as_str),
                        )
                        .await
                        .map(|_| ())
                        .map_err(ForgeError::Sql)?;
                    self.forge.core.telemetry.record_retry(class);
                }
                if let Some(resource) = execution_envelope_resource {
                    self.forge
                        .core
                        .telemetry
                        .record_capacity_refusal(ForgeCapacityRefusalPhase::Execution);
                    self.forge
                        .core
                        .telemetry
                        .record_execution_envelope_failure(resource);
                }
                Ok(())
            }
        }
    }

    /// Persists a non-attempt-consuming capacity refusal.
    ///
    /// # Errors
    /// Returns SQL errors when exact attempt release fails.
    async fn record_capacity_refusal(
        &self,
        task_id: Uuid,
        attempt: Uuid,
    ) -> Result<(), ForgeError> {
        self.tasks
            .release_capacity_refused(task_id, attempt, self.owner)
            .await
            .map_err(ForgeError::Sql)
    }

    /// Persists a typed terminal failure and audit record in one transaction.
    ///
    /// # Errors
    /// Returns tenant, qualification, transition, audit, or commit failures.
    async fn terminal_failure(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        class: ForgeFailureClass,
        failed_volume_identity: Option<&str>,
        detail: String,
    ) -> Result<(), ForgeError> {
        let mut conn = self
            .forge
            .core
            .vala
            .tenant_conn(claim.data_tenant_id)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .qualify_terminal_failure(
                &mut conn,
                claim.task_id,
                class.as_str(),
                failed_volume_identity,
            )
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .terminal(
                &mut conn,
                ForgeTaskTransition {
                    task_id: claim.task_id,
                    attempt_id: attempt,
                    owner: self.owner,
                    expected: ForgeTaskState::Running,
                    next: ForgeTaskState::Failed,
                },
                &task_event(claim.task_id, ForgeTaskState::Failed, &detail),
            )
            .await
            .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)
    }

    /// Atomically cancels a superseded claim and creates its successor demand.
    ///
    /// # Errors
    ///
    /// Returns tenant transaction, exact lifecycle, audit, lane-release, demand,
    /// or commit errors. Rollback preserves the original claim for repair.
    async fn cancel_superseded(&self, claim: &ForgeTaskClaim) -> Result<(), ForgeError> {
        let mut conn = self
            .forge
            .core
            .vala
            .tenant_conn(claim.data_tenant_id)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .cancel_superseded(
                &mut conn,
                claim,
                &task_event(
                    claim.task_id,
                    ForgeTaskState::Cancelled,
                    "base_snapshot_superseded",
                ),
            )
            .await
            .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)
    }

    /// Re-enqueues authoritative planning after one terminal table mutation.
    ///
    /// # Errors
    ///
    /// Returns SQL errors when the durable periodic demand cannot be advanced.
    async fn request_replan(
        &self,
        data_tenant_id: DataTenantId,
        table_ref: &ForgeTaskTableIdentity,
    ) -> Result<(), ForgeError> {
        let table = ForgeTaskTableIdentity::new(
            table_ref.catalog.clone(),
            table_ref.namespace.clone(),
            table_ref.table.clone(),
        )
        .map_err(ForgeError::Sql)?;
        self.tasks
            .upsert_periodic(data_tenant_id, &table)
            .await
            .map(|_| ())
            .map_err(ForgeError::Sql)
    }

    /// Reports whether the loaded current snapshot is the exact task commit.
    fn current_snapshot_matches_task(table: &Table, task_id: Uuid) -> bool {
        table.metadata().current_snapshot().is_some_and(|snapshot| {
            snapshot
                .summary()
                .additional_properties
                .get("forge.task_id")
                == Some(&task_id.to_string())
        })
    }

    /// Reports whether one table still exposes the task's exact planning base.
    fn base_snapshot_matches(table: &Table, base_snapshot_id: i64) -> bool {
        table.metadata().current_snapshot_id()
            == (base_snapshot_id != 0).then_some(base_snapshot_id)
    }

    /// Claims the next task for one slot, honoring its maintenance reservation.
    ///
    /// A reserved maintenance slot first attempts a claim restricted to
    /// [`MAINTENANCE_STRATEGIES`]; when no maintenance task is ready it falls
    /// back to an unfiltered claim so the slot still performs compaction rather
    /// than idling. A non-reserved slot claims across every strategy directly.
    /// Both paths share the same fair-claim transaction, so tenancy, lane, and
    /// size admission are identical.
    ///
    /// # Errors
    ///
    /// Returns the underlying fair-claim SQL errors.
    async fn claim_next(
        &self,
        limits: ForgeClaimLimits,
        reserved_maintenance: bool,
    ) -> Result<Option<ForgeTaskClaim>, vala_sql::SqlError> {
        let volume = self.scratch_volume_identity().map_err(|error| {
            vala_sql::SqlError::InvariantViolation {
                detail: error.to_string(),
            }
        })?;
        if reserved_maintenance
            && let Some(claim) = self
                .tasks
                .claim_fair_for_volume(
                    self.owner,
                    limits,
                    Some(MAINTENANCE_STRATEGIES),
                    Some(volume.as_str()),
                )
                .await?
        {
            return Ok(Some(claim));
        }
        self.tasks
            .claim_fair_for_volume(self.owner, limits, None, Some(volume.as_str()))
            .await
    }

    /// Builds the positive atomic claim limits from Forge capacity.
    ///
    /// # Errors
    ///
    /// Returns invalid configuration when the claim TTL exceeds `u32`.
    fn claim_limits(&self) -> Result<ForgeClaimLimits, ForgeError> {
        let governor =
            self.forge
                .core
                .resources
                .snapshot()
                .map_err(|error| ForgeError::Capacity {
                    detail: error.to_string(),
                })?;
        Ok(ForgeClaimLimits {
            max_active_per_tenant: u32::try_from(self.config.per_tenant_active_cap)
                .unwrap_or(u32::MAX),
            lease_seconds: u32::try_from(self.forge.core.config.lease_ttl.as_secs()).map_err(
                |_| ForgeError::InvalidConfig {
                    detail: "Forge claim TTL exceeds u32 seconds".to_owned(),
                },
            )?,
            max_files: self.capacity.max_files,
            max_bytes: self.capacity.max_bytes,
            max_parallelism: self
                .capacity
                .max_parallelism
                .min(u16::try_from(governor.plan.effective_cpu).unwrap_or(u16::MAX)),
            max_memory_bytes: self
                .capacity
                .max_memory_bytes
                .min(u64::try_from(governor.plan.elastic_memory_bytes).unwrap_or(u64::MAX)),
            max_spill_bytes: self
                .capacity
                .max_spill_bytes
                .min(governor.plan.scratch_limit_bytes),
            max_large_task_bytes: self.capacity.max_large_task_bytes,
        })
    }
}

/// Probes one scratch root through create, fsync, and exact-file deletion.
///
/// # Errors
/// Returns typed scratch IO for any local filesystem failure.
fn probe_scratch_root(root: &Path, owner: Uuid) -> Result<(), ForgeError> {
    let path = root.join(format!("forge-probe-{owner}"));
    let file = std::fs::File::create(&path).map_err(|error| ForgeError::ScratchIo {
        kind: error.kind(),
        detail: format!("create {}: {error}", path.display()),
    })?;
    file.sync_all().map_err(|error| ForgeError::ScratchIo {
        kind: error.kind(),
        detail: format!("fsync {}: {error}", path.display()),
    })?;
    drop(file);
    std::fs::remove_file(&path).map_err(|error| ForgeError::ScratchIo {
        kind: error.kind(),
        detail: format!("delete {}: {error}", path.display()),
    })
}

/// Deletes scratch directories belonging to one exact task attempt.
///
/// Names outside the attempt-scoped prefix are deliberately preserved because
/// filesystem listings are not an ownership authority.
///
/// # Errors
/// Returns typed scratch IO when the root cannot be listed or an owned
/// directory cannot be removed.
fn cleanup_attempt_scratch_root(
    root: &Path,
    task_id: Uuid,
    attempt_id: Uuid,
) -> Result<(), ForgeError> {
    let prefix = format!("forge-runtime-{task_id}-{attempt_id}-");
    let entries = std::fs::read_dir(root).map_err(|error| ForgeError::ScratchIo {
        kind: error.kind(),
        detail: format!("list exact attempt root {}: {error}", root.display()),
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| ForgeError::ScratchIo {
            kind: error.kind(),
            detail: format!("read exact attempt entry: {error}"),
        })?;
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            std::fs::remove_dir_all(entry.path()).map_err(|error| ForgeError::ScratchIo {
                kind: error.kind(),
                detail: format!(
                    "delete exact attempt scratch {}: {error}",
                    entry.path().display()
                ),
            })?;
        }
    }
    Ok(())
}

/// Rejects a new worker maintenance effect after shared authority is cancelled.
///
/// # Errors
///
/// Returns [`ForgeError::Shutdown`] after claim loss, table-lease loss, or process shutdown.
fn require_running(stop: &CancellationToken) -> Result<(), ForgeError> {
    if stop.is_cancelled() {
        Err(ForgeError::Shutdown)
    } else {
        Ok(())
    }
}

/// Builds one internal task lifecycle audit envelope.
fn task_event(task_id: Uuid, state: ForgeTaskState, reason: &str) -> AuditEvent {
    AuditEvent::new(
        RequestId::now_v7(),
        None,
        format!("forge.task.{}", state.as_str()),
        format!("forge-task:{task_id}"),
        None,
        PrincipalId::new(Uuid::nil()),
        PrincipalKindTag::Service,
        AuthMethod::Internal,
        "bifrost:forge".to_owned(),
        AuditDecision::Allow,
        AuditResult::Success,
        reason.to_owned(),
    )
}

/// Maps execution evidence onto the durable planning consequence for settlement.
#[must_use]
fn task_progress_effect(
    strategy: &ForgeClaimStrategy,
    base_snapshot_id: i64,
    parameters: &serde_json::Value,
    progressed: bool,
) -> TaskProgressEffect {
    if progressed || strategy != &ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry) {
        return TaskProgressEffect::Progressed;
    }
    TaskProgressEffect::NoOpAcknowledged {
        snapshot_id: base_snapshot_id,
        commit_count: parameters
            .get("trigger_commit_count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Retryable classes terminalize on the fifth failure while data refusal is immediate.
    #[test]
    fn failure_terminalization_obeys_attempt_bound() {
        assert!(failure_is_terminal(ForgeFailureClass::DataRefusal, 0));
        assert!(!failure_is_terminal(
            ForgeFailureClass::TransientObjectStore,
            ATTEMPT_BOUND - 2
        ));
        assert!(failure_is_terminal(
            ForgeFailureClass::TransientObjectStore,
            ATTEMPT_BOUND - 1
        ));
        assert!(failure_is_terminal(
            ForgeFailureClass::StorageHealth,
            ATTEMPT_BOUND - 1
        ));
    }

    /// Scratch probing proves a writable root and leaves no probe artifact.
    #[test]
    fn scratch_probe_accepts_writable_root() {
        let root = tempfile::tempdir().expect("writable scratch root");
        let owner = Uuid::now_v7();

        probe_scratch_root(root.path(), owner).expect("writable scratch probe");

        assert!(!root.path().join(format!("forge-probe-{owner}")).exists());
    }

    /// Scratch probing returns typed local IO when the configured root is not a directory.
    #[test]
    fn scratch_probe_rejects_unwritable_root() {
        let parent = tempfile::tempdir().expect("scratch probe parent");
        let root = parent.path().join("not-a-directory");
        std::fs::File::create(&root).expect("scratch root file");

        assert!(matches!(
            probe_scratch_root(&root, Uuid::now_v7()),
            Err(ForgeError::ScratchIo { .. })
        ));
    }

    /// Exact-attempt cleanup removes owned directories and preserves every peer.
    #[test]
    fn scratch_cleanup_is_exactly_attempt_scoped() {
        let root = tempfile::tempdir().expect("scratch root");
        let task_id = Uuid::now_v7();
        let attempt_id = Uuid::now_v7();
        let other_attempt = Uuid::now_v7();
        let owned = root
            .path()
            .join(format!("forge-runtime-{task_id}-{attempt_id}-owned"));
        let peer_attempt = root
            .path()
            .join(format!("forge-runtime-{task_id}-{other_attempt}-peer"));
        let foreign = root.path().join("caller-owned-directory");
        std::fs::create_dir(&owned).expect("owned attempt directory");
        std::fs::create_dir(&peer_attempt).expect("peer attempt directory");
        std::fs::create_dir(&foreign).expect("foreign directory");

        cleanup_attempt_scratch_root(root.path(), task_id, attempt_id)
            .expect("exact attempt cleanup");

        assert!(!owned.exists(), "the exact attempt directory is removed");
        assert!(peer_attempt.exists(), "a sibling attempt is preserved");
        assert!(foreign.exists(), "an unscoped directory is preserved");
    }

    /// Raw metadata evidence hashes exact bytes and uses lowercase encoding.
    #[test]
    fn raw_metadata_digest_contract_is_exact() {
        let raw = br#"{"format-version":2,"current-snapshot-id":7}"#;
        assert_eq!(
            format!("sha256:{}", hex::encode(Sha256::digest(raw))),
            "sha256:6467490277052fc1bcd80842434ed2ea67de45224d5adb4a6c50b7fc5a0ce727"
        );
    }

    /// Worker concurrency and the per-tenant cap are hard positive invariants.
    #[test]
    fn worker_concurrency_must_be_positive() {
        assert!(
            ForgeWorkerConfig {
                worker_concurrency: 0,
                per_tenant_active_cap: 1,
            }
            .validate()
            .is_err()
        );
        assert!(
            ForgeWorkerConfig {
                worker_concurrency: 1,
                per_tenant_active_cap: 0,
            }
            .validate()
            .is_err()
        );
        let defaults = ForgeWorkerConfig::default();
        assert_eq!(defaults.worker_concurrency, 1);
        assert_eq!(defaults.per_tenant_active_cap, 1);
    }

    /// The per-tenant cap is stored and read independently of executor count.
    ///
    /// Proves the fields are decoupled at the config layer: a config may carry
    /// a per-tenant cap that differs from `worker_concurrency` in either
    /// direction and still validate, which is the whole point of separating the
    /// D78 fairness bound from parallelism.
    #[test]
    fn per_tenant_active_cap_is_independent_of_worker_concurrency() {
        let wider = ForgeWorkerConfig {
            worker_concurrency: 2,
            per_tenant_active_cap: 8,
        }
        .validate()
        .expect("a per-tenant cap above the executor count is legal");
        assert_eq!(wider.per_tenant_active_cap, 8);
        assert_eq!(wider.worker_concurrency, 2);

        let narrower = ForgeWorkerConfig {
            worker_concurrency: 8,
            per_tenant_active_cap: 1,
        }
        .validate()
        .expect("a per-tenant cap below the executor count is legal");
        assert_eq!(narrower.per_tenant_active_cap, 1);
    }

    /// Completion observation retains an event recorded before the waiter starts.
    #[tokio::test]
    async fn completion_observer_preserves_early_completion() {
        let observer = ForgeWorkerCompletionObserver::new();
        observer.record(
            Uuid::now_v7(),
            Uuid::now_v7(),
            ForgeClaimStrategy::Known(ForgeTaskStrategy::StagingFold),
        );
        observer.wait_for_at_least(1).await;
        assert_eq!(observer.completed(), 1);
    }

    /// A completion between waiter registration and its counter read wakes the waiter.
    #[tokio::test]
    async fn completion_observer_preserves_registration_race() {
        let observer = ForgeWorkerCompletionObserver::new();
        let registered = Arc::new(tokio::sync::Barrier::new(2));
        let release = Arc::new(tokio::sync::Barrier::new(2));
        observer.install_registration_gate_for_test(Arc::clone(&registered), Arc::clone(&release));
        let observer_for_waiter = observer.clone();
        let waiter = tokio::spawn(async move {
            observer_for_waiter.wait_for_at_least(1).await;
        });
        registered.wait().await;
        observer.record(
            Uuid::now_v7(),
            Uuid::now_v7(),
            ForgeClaimStrategy::Known(ForgeTaskStrategy::StagingFold),
        );
        release.wait().await;
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("registered completion waiter wakes")
            .expect("completion waiter task joins");
        assert_eq!(observer.completed(), 1);
    }
}
