//! Bounded claim-driven execution for durable Forge tasks.
//!
//! The same owner runs in the embedded `all` topology and in dedicated worker
//! processes. `PostgreSQL` claims assign compute, while the table-scoped
//! [`ForgeLease`] remains the only publication fence.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::Mutex;
#[cfg(feature = "test-support")]
use std::sync::atomic::AtomicBool;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::metrics::ForgeActiveTask;
use iceberg::spec::TableMetadata;
use iceberg::table::Table;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::queries::forge_tasks::{ForgeClaimLimits, ForgeTasks};
use vala_sql::row_types::forge_operations::ForgeExpirationAuthority;
use vala_sql::row_types::forge_operations::ForgeOperationFamily;
use vala_sql::row_types::forge_tasks::{
    ExpiredCleanupCandidateRequest, ExpiredCleanupOutcome, ExpiredCleanupPayload,
    FORGE_TASK_PAYLOAD_VERSION, ForgeClaimStrategy, ForgeCleanupCandidate, ForgePreparedTaskClaim,
    ForgeTask, ForgeTaskClaim, ForgeTaskEvidence, ForgeTaskRowEvidence, ForgeTaskState,
    ForgeTaskStrategy, ForgeTaskTableIdentity, ForgeTaskTransition, MAINTENANCE_STRATEGIES,
    SnapshotWatermark, TaskProgressEffect,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeIcebergRewritePhase,
    ForgeScribePromotionPhase, StoragePath,
};

use super::compact::ForgeGroupKey;
use super::error::{ForgeError, ForgeFailureClass};
use super::expire::ExpiryTaskAuthority;
use super::identity::task_table_binding;
use super::lease::{ForgeLease, forge_lease_key};
use super::metrics::{ForgeTaskResult, ForgeTelemetry};
use super::orphan_gc::{ExpiredCleanupExemption, GcEligibility, ObjectEvidence};
use super::path::catalog_path_to_object_key;
use super::scribe_promotion::{
    ForgePromotionCommit, ForgePromotionSettlement, ScribePromotionPlan,
};
use super::{Forge, ForgeRoleReadiness};
use crate::catalog::TenantTableBinding;
use crate::catalog::layout::forge_data_location;

/// Parameter discriminator every durable small-file rewrite task carries.
///
/// The scheduler writes it and pre-effect validation requires it, so a durable
/// row whose parameters describe a different workflow cannot be executed as a
/// rewrite even if its strategy column says otherwise.
pub(super) const LIVE_REWRITE_PARAMETER_KIND: &str = "live_rewrite";

/// Everything one rewrite publication holds constant across its attempts.
///
/// The derivation, the commit, and the three audit transitions all read the
/// same identity, group, partition, policy, and deadline. Owning them here is
/// what keeps a retried attempt provably bound to the same operation as the
/// first one: nothing in the retry loop can recompute a deadline, re-derive a
/// partition, or open a second operation identity.
struct RewritePublication<'publication> {
    /// Durable task this publication is settling.
    claim: &'publication ForgeTaskClaim,
    /// Tenant-qualified table the publication commits against.
    binding: &'publication TenantTableBinding,
    /// Attempt-scoped identity every published snapshot and audit row carries.
    identity: super::publication::RewriteCommitIdentity,
    /// Audit group and time partition the transitions are recorded under.
    key: ForgeGroupKey,
    /// Partition spec the base table declared when the attempt started.
    partition_spec_id: i32,
    /// Target file size the table's own property declared.
    target_file_size_bytes: u64,
    /// Schema identity the plan was authorized against.
    planned_schema_id: i32,
    /// One commit budget, captured before the first attempt and never renewed.
    deadline: super::publication::RewritePublicationDeadline,
}

impl RewritePublication<'_> {
    /// Builds the durable audit detail for one transition of this publication.
    ///
    /// Every field but the phase and the committed snapshot is fixed by the
    /// publication, so a Prepared row and the terminal row that settles it
    /// describe the same operation over the same objects — which is exactly
    /// what recovery compares them for.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when a catalog file path in the
    /// request is not a representable storage path.
    fn audit(
        &self,
        phase: ForgeIcebergRewritePhase,
        committed_snapshot_id: Option<i64>,
        request: &super::publication::RewriteCommitRequest,
    ) -> Result<AuditDetail, ForgeError> {
        Ok(AuditDetail::ForgeIcebergRewrite {
            operation_id: self.identity.operation_id,
            phase,
            group: self.identity.group.clone(),
            base_snapshot_id: request.base_snapshot_id,
            committed_snapshot_id,
            partition_spec_id: self.partition_spec_id,
            time_partition: self.key.partition.to_wire(),
            target_file_size_bytes: self.target_file_size_bytes,
            input_paths: ForgeWorker::rewrite_audit_paths(
                request
                    .removed_data_files
                    .iter()
                    .chain(&request.removed_delete_files),
            )?,
            output_paths: ForgeWorker::rewrite_audit_paths(request.added_data_files.iter())?,
        })
    }
}

/// Renders the durable operation name for one live-rewrite audit transition.
///
/// The family prefix and the phase suffix are the two halves the durable
/// `vala.forge_operation_state` row is keyed by, so composing them here keeps
/// every rewrite transition on one spelling.
fn rewrite_operation(phase: ForgeIcebergRewritePhase) -> String {
    format!(
        "{}.{}",
        ForgeOperationFamily::IcebergRewrite.operation_prefix(),
        match phase {
            ForgeIcebergRewritePhase::Prepared => "prepared",
            ForgeIcebergRewritePhase::Committed => "committed",
            ForgeIcebergRewritePhase::Recovered => "recovered",
            ForgeIcebergRewritePhase::Reset => "reset",
        }
    )
}

/// Maximum number of attempt-consuming failures before audited terminalization.
const ATTEMPT_BOUND: u32 = 5;

/// Returns whether the next classified failure must terminalize at the worker boundary.
#[must_use]
pub(super) const fn failure_is_terminal(class: ForgeFailureClass, completed_attempts: u32) -> bool {
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
/// The attempt's publication fence and cancellation token both belong to the
/// worker's fenced execution, so dispatch borrows them together instead of
/// threading each through the strategy match.
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
}

/// Validated snapshot-expiration intent encoded by one durable task plan.
///
/// Snapshot expiration is the only remaining plan-encoded maintenance effect,
/// so the intent carries one flag: whether this exact task owns a retention
/// pass, or was planned solely to reconcile an open live rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ForgeSnapshotExpiryIntent {
    /// Whether the task owns one snapshot-retention pass.
    snapshot_expiry_due: bool,
}

impl ForgeSnapshotExpiryIntent {
    /// Decodes the canonical snapshot-expiry plan.
    ///
    /// Canonical rows carry exactly the four fields the scheduler writes: the
    /// maintenance kind, its trigger commit count, and the two due flags. Any
    /// other shape is refused, because no other shape was ever produced and a
    /// fallback decoder would be an invented compatibility route.
    fn parse(strategy: &ForgeClaimStrategy, parameters: &Map<String, Value>) -> Option<Self> {
        if !matches!(
            strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry)
        ) {
            return None;
        }
        if parameters.get("kind").and_then(Value::as_str) != Some("maintenance")
            || parameters
                .get("trigger_commit_count")
                .and_then(Value::as_u64)
                .is_none()
        {
            return None;
        }
        if parameters.len() != 4 {
            return None;
        }
        let snapshot_expiry_due = parameters.get("snapshot_expiry_due")?.as_bool()?;
        let reconciliation_due = parameters.get("reconciliation_due")?.as_bool()?;
        (snapshot_expiry_due || reconciliation_due).then_some(Self {
            snapshot_expiry_due,
        })
    }
}

/// Exact result one dispatched snapshot-expiration pass returns.
///
/// The pass reloads the table after its own commits so the completion path
/// derives evidence from current metadata, and reports whether the expiration
/// already settled its own task atomically.
struct ForgeSnapshotExpiryResult {
    /// Current table after the expiration's metadata commits.
    table: Table,
    /// Final task evidence an atomic snapshot-expiration settlement stored.
    ///
    /// `Some` means the expiration already moved its task to `Succeeded` with
    /// exact cleanup candidates and advanced planning demand, so the worker
    /// owes the task no further terminal transition and performs no delete.
    expiry_evidence: Option<ForgeTaskEvidence>,
}

/// Exact file and byte volume one committed Forge effect moved.
///
/// Reported only once the durable settlement for that effect commits, so no
/// counter claims throughput the durable record does not hold.
#[derive(Debug, Clone, Copy)]
struct ForgeCommittedVolume {
    /// Logical input files the effect consumed.
    input_files: u64,
    /// Logical input bytes the effect consumed.
    input_bytes: u64,
    /// Published output files the effect produced.
    output_files: u64,
    /// Published output bytes the effect produced.
    output_bytes: u64,
}

impl ForgeCommittedVolume {
    /// Adds another committed effect's volume to this one.
    ///
    /// Saturating because a counter that wrapped would report less throughput
    /// than the durable record holds, and one attempt's plans are summed before
    /// anything is emitted.
    fn saturating_add(self, other: Self) -> Self {
        Self {
            input_files: self.input_files.saturating_add(other.input_files),
            input_bytes: self.input_bytes.saturating_add(other.input_bytes),
            output_files: self.output_files.saturating_add(other.output_files),
            output_bytes: self.output_bytes.saturating_add(other.output_bytes),
        }
    }
}

/// One committed publication and the exact volume its commit measured.
///
/// Boxed into its dispatch variant because the loaded table dominates every
/// other outcome's size, and an inline copy would make each dispatch result as
/// large as its largest arm.
struct ForgeCommittedPublication {
    /// Table as it stands after the publication's own commit.
    table: Table,
    /// Exact file and byte volume, when the committing owner measured it.
    volume: Option<ForgeCommittedVolume>,
}

/// The three things every plan of one rewrite attempt publishes against.
///
/// They are grouped because they are attempt-scoped, not plan-scoped: the
/// managed rewrite owns the one execution context, observer, and cancellation
/// token every plan runner shares; the table is the single metadata read every
/// plan's publication derives its geometry from; and the planning evidence is
/// the one selection the whole attempt was admitted on. Passing them
/// individually alongside the per-plan arguments would obscure exactly that
/// split.
struct ForgeRewriteAttempt {
    /// Managed core seam this attempt's plans execute through.
    ///
    /// Owned behind an `Arc` rather than borrowed because every plan runs on
    /// the worker's compaction executor, outside the frame that planned it.
    rewrite: Arc<super::managed::ForgeManagedRewrite>,
    /// Table as planning read it, used for geometry and publication authority.
    table: Table,
    /// Planning evidence every plan of this attempt records under its operation.
    evidence: super::managed::ForgeRewriteEvidence,
    /// One absolute publication budget every plan of this attempt shares.
    ///
    /// Attempt-scoped, not plan-scoped: the budget bounds how long this claim
    /// may keep asking the catalog to accept work planned against one table
    /// read, and a per-plan budget would let an attempt with many plans keep
    /// submitting for a multiple of the configured window. A plan that finds
    /// the budget spent refuses before it prepares anything, which is ordinary
    /// planning debt the next attempt replans.
    deadline: super::publication::RewritePublicationDeadline,
}

/// One claimed ownership episode's observation frame.
///
/// Held from the moment a claim is taken until its single passive observation,
/// so an attempt that suspends on the worker-wide plan queue keeps the same
/// span, gauge, and claim-time instant an inline one would.
struct OpenClaim {
    /// The durable claim this episode owns.
    claim: ForgeTaskClaim,
    /// Claim-time instant every attempt-duration metric measures from.
    started: Instant,
    /// Active-task gauge guard, dropped when the episode is observed.
    active: Option<ForgeActiveTask>,
    /// Execution span every phase of this episode records into.
    span: tracing::Span,
}

/// One admitted compaction attempt suspended on the worker-wide plan queue.
///
/// Everything the attempt still needs to settle lives here: its observation
/// frame, its fence, its fenced tokens and heartbeat, the managed context its
/// plans publish through, and the per-plan results collected so far. Holding it
/// beside the queue rather than inside a call frame is what lets one worker
/// keep several attempts in flight against one parallelism budget.
struct ForgeAttemptState {
    /// Observation frame this attempt closes when its last plan drains.
    open: OpenClaim,
    /// Durable attempt generation this episode writes under.
    attempt: Uuid,
    /// Tenant-scoped table this attempt publishes to.
    binding: TenantTableBinding,
    /// Validated execution stage, used by failure settlement.
    stage: ForgeExecutionStage,
    /// Table fence every plan of this attempt publishes under.
    lease: ForgeLease,
    /// Cancellation seams and heartbeat this attempt opened.
    fenced: FencedAttempt,
    /// Managed context and publication budget every plan shares.
    shared: Arc<ForgeRewriteAttempt>,
    /// Plans the queue refused, with the reason, in offer order.
    refusals: Vec<(usize, super::managed::queue::ForgePushResult)>,
    /// Per-plan results collected in completion order.
    outcomes: Vec<(usize, Result<ForgeDispatchResult, ForgeError>)>,
    /// Plans admitted to the queue that have not started yet.
    queued: usize,
    /// Plans started on the executor that have not been joined yet.
    running: usize,
}

impl ForgeAttemptState {
    /// Returns whether every admitted plan of this attempt has been joined.
    fn drained(&self) -> bool {
        self.queued == 0 && self.running == 0
    }

    /// Returns whether this attempt is drained but cannot yet be settled.
    ///
    /// A retained attempt still owns its claim, fence, heartbeat, and exact
    /// operation identities. It is neither failed nor retried: only proof about
    /// those exact operations can decide it.
    fn retained(&self) -> bool {
        self.drained() && ForgeWorker::retains_unknown_acceptance(self)
    }
}

/// The worker's compaction admission state: one FIFO, one map, one channel.
///
/// The three are one owner because they describe one fact together — which of
/// this worker's plans are waiting, which are running, and which attempt each
/// belongs to — and separating them would let the parallelism budget disagree
/// with what is actually in flight.
///
/// A drained attempt whose operations are unresolved stays in `attempts` rather
/// than moving to a second container: retention is a property of an attempt's
/// own indexed outcomes, so a parallel list would be a second place for the
/// same fact to be wrong.
struct ForgeAttemptPool {
    /// Worker-wide FIFO holding every attempt's unstarted plans in offer order.
    queue: super::managed::queue::ForgeCompactionQueue,
    /// Suspended attempts, keyed by the task each plan belongs to.
    attempts: HashMap<Uuid, ForgeAttemptState>,
    /// Sender every spawned runner sends its one keyed completion through.
    completion_tx: tokio::sync::mpsc::UnboundedSender<ForgePlanCompletion>,
    /// The worker's single completion receiver, polled by its one event loop.
    completion_rx: tokio::sync::mpsc::UnboundedReceiver<ForgePlanCompletion>,
}

impl ForgeAttemptPool {
    /// Builds an empty pool bounded by one worker's configured budgets.
    fn new(config: &ForgeWorkerConfig) -> Self {
        let (completion_tx, completion_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            queue: super::managed::queue::ForgeCompactionQueue::new(
                config.max_task_parallelism,
                config.pending_task_parallelism,
                config.compaction_memory_budget_bytes,
            ),
            attempts: HashMap::new(),
            completion_tx,
            completion_rx,
        }
    }

    /// Returns whether this worker has no suspended attempt left to settle.
    fn is_idle(&self) -> bool {
        self.attempts.is_empty()
    }

    /// Returns whether any attempt still has a plan waiting or running.
    fn has_plans_in_flight(&self) -> bool {
        self.attempts.values().any(|state| !state.drained())
    }

    /// Returns whether any locally held attempt is unresolved.
    ///
    /// Derived from the attempts' own indexed outcomes, so it is exactly the
    /// set that [`ForgeWorker::reconcile_retained_attempts`] will visit.
    fn has_local_ambiguity(&self) -> bool {
        self.attempts.values().any(ForgeAttemptState::retained)
    }

    /// Returns every unresolved attempt's task id, in ascending order.
    fn retained_task_ids(&self) -> Vec<Uuid> {
        let mut retained: Vec<Uuid> = self
            .attempts
            .iter()
            .filter(|(_, state)| state.retained())
            .map(|(task_id, _)| *task_id)
            .collect();
        retained.sort_unstable();
        retained
    }

    /// Offers one attempt's planner-ordered plans and suspends it on the queue.
    ///
    /// The offer happens here rather than in the attempt's own frame because
    /// the budget the plans compete for belongs to the worker, not the attempt.
    ///
    /// Returns the attempt unchanged when the queue admitted none of its plans:
    /// nothing will ever complete for it, so it must settle now rather than
    /// wait for a plan that was never started.
    fn admit(
        &mut self,
        worker: &ForgeWorker,
        mut state: ForgeAttemptState,
        plans: Vec<super::managed::ForgePlannedRewrite>,
    ) -> Option<ForgeAttemptState> {
        let task_id = state.open.claim.task_id;
        let planned = plans.len();
        // One concrete runner is built per plan before the plan is offered, so
        // the queue owns the thing that will execute rather than a description
        // of it, and popping is what hands execution its owner.
        let admissions = plans
            .into_iter()
            .map(|plan| {
                (
                    super::managed::queue::ForgePlanAdmission {
                        task_id,
                        plan_index: plan.plan_index,
                        required_parallelism: plan.required_parallelism,
                        memory_reservation_bytes: plan.memory_reservation_bytes,
                    },
                    Some(ForgeCompactionPlanRunner::new(worker, &state, plan)),
                )
            })
            .collect::<Vec<_>>();
        let before = self.queue.waiting_plan_count();
        state.refusals = ForgeWorker::offer_planned_rewrites(&mut self.queue, admissions);
        state.queued = self.queue.waiting_plan_count() - before;
        tracing::debug!(
            task_id = %task_id,
            planned,
            queued = state.queued,
            waiting_parallelism = self.queue.waiting_parallelism_sum(),
            refused = state.refusals.len(),
            "Forge admitted one attempt's compaction plans"
        );
        if state.queued == 0 {
            return Some(state);
        }
        self.attempts.insert(task_id, state);
        None
    }
}

/// One compaction attempt whose plans still have to enter the worker pool.
struct AdmittedRewrite {
    /// Durable attempt generation this episode writes under.
    attempt: Uuid,
    /// Tenant-scoped table this attempt publishes to.
    binding: TenantTableBinding,
    /// Validated execution stage, used by failure settlement.
    stage: ForgeExecutionStage,
    /// Table fence every plan of this attempt publishes under.
    lease: ForgeLease,
    /// Cancellation seams and heartbeat this attempt opened.
    fenced: FencedAttempt,
    /// Managed context and publication budget every plan shares.
    shared: Arc<ForgeRewriteAttempt>,
    /// Planner-ordered plans still to be offered to the queue.
    plans: Vec<super::managed::ForgePlannedRewrite>,
}

/// What starting one claimed attempt produced.
enum AttemptStart {
    /// The attempt settled inside its own frame; carries whether an effect landed.
    Settled(bool),
    /// The attempt planned compaction work the worker pool must admit.
    Planned(Box<AdmittedRewrite>),
}

/// What opening one attempt's fenced frame produced for its caller.
enum OpenedFrame {
    /// The attempt ran to a settled or failed outcome inside this frame.
    Complete(bool),
    /// The attempt's compaction plans must be admitted to the worker pool.
    Planned(
        Arc<ForgeRewriteAttempt>,
        Vec<super::managed::ForgePlannedRewrite>,
        FencedAttempt,
    ),
}

/// What planning one compaction attempt selected.
enum RewriteAdmission {
    /// Planning selected nothing, so the table is already compact.
    SelfSettled,
    /// Plans to admit, with the managed context every one of them shares.
    Planned(
        Arc<ForgeRewriteAttempt>,
        Vec<super::managed::ForgePlannedRewrite>,
    ),
}

/// What opening one claim episode produced.
enum ClaimStep {
    /// The episode settled and observed itself inside its own frame.
    Closed(Result<bool, ForgeError>),
    /// The episode's plans entered the worker-wide pool; settlement waits.
    Admitted,
}

/// The fenced frame an attempt holds between opening and settlement.
///
/// Kept as one value so an attempt that suspends across the worker-wide plan
/// queue carries its cancellation seams and heartbeat forward intact instead of
/// threading four fields through the resumption path.
struct FencedAttempt {
    /// Shutdown-sensitive token every non-maintenance dispatch observes.
    ///
    /// Cancelling it is also how settlement drains a post-effect attempt.
    operation_stop: CancellationToken,
    /// Authority-loss token the heartbeat cancels; shutdown never reaches it.
    authority_stop: CancellationToken,
    /// The claim heartbeat keeping this attempt's ownership provable.
    heartbeat: tokio::task::JoinHandle<Result<(), ForgeError>>,
    /// Whether this strategy observes authority loss instead of shutdown.
    maintenance_recovery: bool,
}

impl FencedAttempt {
    /// Returns the token this frame's dispatch must observe.
    ///
    /// Maintenance completes through a graceful shutdown and stops only on
    /// genuine authority loss; every other strategy drains cooperatively.
    fn dispatch_stop(&self) -> &CancellationToken {
        if self.maintenance_recovery {
            &self.authority_stop
        } else {
            &self.operation_stop
        }
    }
}

/// What opening an attempt's fenced frame produced.
enum FencedStart {
    /// The base moved past this plan, so the claim was cancelled as superseded.
    Superseded,
    /// A prior attempt's commit was proven from retained evidence.
    ///
    /// Boxed because committed evidence is far larger than the other variants.
    Recovered(Box<ForgeTaskEvidence>, FencedAttempt),
    /// The frame is open and this attempt's strategy must still be dispatched.
    Open(Table, FencedAttempt),
}

enum ForgeDispatchResult {
    /// Ordinary publication whose evidence is not yet Prepared.
    ///
    /// The volume is present only when the committing owner measured exact
    /// file and byte counts for this effect.
    Committed(Box<ForgeCommittedPublication>),
    /// One snapshot-expiration pass with its exact post-expiry candidates.
    SnapshotExpiry(Box<ForgeSnapshotExpiryResult>),
    /// A fully drained expired-cleanup candidate set with its final evidence.
    Cleaned(Box<ForgeTaskEvidence>),
    /// One submitted commit whose acceptance this attempt could not learn.
    ///
    /// Not a failure and not a success: the operation is open, its outputs may
    /// already be live, and only proof about this exact operation can close it.
    /// The owner therefore retains it rather than settling the task.
    AcceptanceUnknown(Box<ForgeUnknownAcceptance>),
    /// A dispatch that already wrote its own durable task transition and owes
    /// the worker no further one.
    ///
    /// Two dispatches settle themselves. A bounded orphan-cleanup pass writes
    /// either a cursor checkpoint plus a non-consuming release or the audited
    /// terminal success that exhaustion earns. A compaction attempt whose
    /// planning selected nothing writes the audited no-op success directly,
    /// because it holds no operation and produced no evidence for the ordinary
    /// Prepared path to reconcile.
    SelfSettled,
}

/// Everything a retained attempt needs to settle one unresolved operation.
///
/// The exact UUID is what reconciliation asks about, the typed error is what
/// the plan settles with if the operation is proven absent, and the volume is
/// what it reports if the operation is proven live — none of which can be
/// re-derived once the attempt's frame is gone.
struct ForgeUnknownAcceptance {
    /// Prepared operation whose acceptance is unknown.
    operation_id: Uuid,
    /// Original typed failure this plan returned with.
    error: ForgeError,
    /// Exact request volume this plan submitted.
    volume: ForgeCommittedVolume,
}

/// Durable task state accompanying exact committed evidence.
enum ForgeExecutionEvidenceState {
    /// A normal strategy execution returned evidence while its task is Running.
    Fresh,
    /// An accepted external commit was discovered while its task is Running.
    RecoveredCommit,
    /// Maintenance already persisted the task's Prepared evidence transaction.
    Prepared,
    /// An atomic snapshot-expiration settlement already moved the task to
    /// `Succeeded`, stored its exact cleanup candidates, and advanced planning
    /// demand, so this worker owes it no further terminal transition.
    Settled,
}

/// Exact durable ownership required to advance a Prepared cleanup cursor.
/// Durable position an expired-cleanup drain restarts from.
///
/// A fresh dispatch starts at zero with nothing prepared. A takeover starts at
/// the durable frontier, and `prepared` records whether that frontier's
/// candidate already committed a preparation whose external result is unproven
/// — in which case the drain must settle that candidate rather than prepare it
/// a second time.
#[derive(Debug, Clone, Copy, Default)]
struct CleanupResume {
    /// Candidates already durably settled as advanced.
    frontier: u32,
    /// Whether the candidate at `frontier` is already prepared.
    prepared: bool,
}

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

/// Readiness and stop bookkeeping owned by one running worker's event loop.
///
/// A worker no longer has sibling slots, so this holds only the two handles the
/// loop's own fatal path must reach: the role readiness it must retract and the
/// child token that stops any in-flight compaction runner it spawned.
#[derive(Clone)]
struct ForgeWorkerLoopHandles {
    /// Existing role handle; the loop is the only readiness decider.
    readiness: ForgeRoleReadiness,
    /// Child token cancelling admitted runners without touching process shutdown.
    stop: CancellationToken,
}

/// Compaction memory budget used by embedded fixtures and defaults.
///
/// Production always overrides this from the node's immutable resource plan.
/// The value only has to be large enough that a small fixture table's single
/// plan is admitted rather than refused as too large for the whole worker.
const DEFAULT_COMPACTION_MEMORY_BUDGET_BYTES: usize = 1 << 30;

/// Fixed process-local bounds for one Forge worker.
///
/// One worker runs exactly one event loop and one compaction-admission queue.
/// Local execution concurrency is therefore not an executor count: it is the
/// parallelism and estimated-memory budget the queue admits plans against,
/// resolved once from the immutable node resource plan.
#[derive(Debug, Clone, Copy)]
pub struct ForgeWorkerConfig {
    /// Maximum active tasks one tenant may hold concurrently (the D78 fairness
    /// bound, `max_active_per_tenant` in the fair claim).
    ///
    /// This bounds durable claims only. It is deliberately independent of local
    /// execution parallelism, because one claimed compaction task fans out into
    /// as many concurrent plan runners as the queue admits.
    pub per_tenant_active_cap: usize,
    /// Estimated heap this worker charges against concurrently running plans.
    ///
    /// Waiting plans are uncharged, so a large queued plan cannot starve
    /// smaller running ones. Must be positive.
    pub compaction_memory_budget_bytes: usize,
    /// Maximum parallelism summed across concurrently running plans.
    pub max_task_parallelism: u32,
    /// Maximum parallelism summed across waiting plans.
    ///
    /// Must be at least [`Self::max_task_parallelism`], otherwise the queue
    /// could not hold one maximally parallel plan.
    pub pending_task_parallelism: u32,
}

/// One admitted rewrite attempt's publication evidence, keyed by its identities.
///
/// Compiled only under `test-support`. The evidence itself is the immutable set
/// managed execution produced; the three identities are the ones the published
/// snapshot carries in its `forge.task_id`, `forge.attempt_id`, and
/// `forge.operation_id` properties, which is what lets a journey correlate a
/// landed snapshot to the exact attempt that derived it.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeRewriteEvidenceRecord {
    /// Durable task the attempt was claimed for.
    pub task_id: Uuid,
    /// Attempt identity the evidence was produced under.
    pub attempt_id: Uuid,
    /// Operation identity publication will Prepare under.
    pub operation_id: Uuid,
    /// Exact planning evidence the attempt produced.
    pub evidence: crate::forge::managed::ForgeRewriteEvidence,
}

/// Observes successful durable task completion from supervised worker roles.
///
/// The observer is an optional test-support seam. It records only after the
/// production worker's claim execution returns success, so it cannot affect
/// scheduling, claim ownership, publication, or terminal transitions.
#[derive(Clone, Default)]
#[cfg(feature = "test-support")]
pub struct ForgeWorkerCompletionObserver {
    /// Ordered typed lifecycle events recorded by production scheduler and worker owners.
    lifecycle_events: Arc<Mutex<Vec<ForgeLifecycleEvent>>>,
    /// Number of supervised execution attempts observed after their durable path returned.
    attempts: Arc<AtomicUsize>,
    /// Errors returned by supervised execution attempts in observation order.
    returned_errors: Arc<Mutex<Vec<String>>>,
    /// Typed unsettled-output evidence carried by each returned error.
    ///
    /// Index-aligned with [`Self::returned_errors`]. An entry is `None` when
    /// the returned failure was not a [`ForgeError::RewriteUnsettled`], and
    /// otherwise holds the attempt-global possible-output set exactly as the
    /// wrapper carried it. Retaining it typed is what lets a refusal scenario
    /// assert the exact object identities a failed attempt left behind instead
    /// of matching the wrapper's rendered text.
    ///
    /// Compiled only under `test-support`: retaining the wrapper's outputs is
    /// diagnostic evidence for refusal scenarios, so a default production build
    /// neither holds the vector nor clones an unsettled attempt's paths into it.
    #[cfg(feature = "test-support")]
    returned_unsettled: Arc<Mutex<Vec<Option<Vec<crate::forge::managed::ForgeUnsettledOutput>>>>>,
    /// Publication evidence each admitted rewrite attempt produced, in order.
    ///
    /// Compiled only under `test-support`. Recorded at the production dispatch
    /// boundary once managed execution has produced the immutable evidence and
    /// before publication consumes it, so a journey can compare a landed
    /// snapshot's fingerprint properties against the attempt's own values
    /// without recomputing a fingerprint or reading a persisted column that
    /// does not carry them.
    #[cfg(feature = "test-support")]
    rewrite_evidence: Arc<Mutex<Vec<ForgeRewriteEvidenceRecord>>>,
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
    /// One-shot passive barrier after the next managed rewrite handoff exists.
    #[cfg(feature = "test-support")]
    pause_after_next_handoff: Arc<AtomicBool>,
    /// Whether a completed handoff is currently held at the passive barrier.
    #[cfg(feature = "test-support")]
    handoff_paused: Arc<AtomicBool>,
    /// Wakeup for tests waiting until the completed handoff is held.
    #[cfg(feature = "test-support")]
    handoff_pause_ready: Arc<tokio::sync::Notify>,
    /// Release for the one completed handoff held by the passive barrier.
    #[cfg(feature = "test-support")]
    handoff_pause_release: Arc<tokio::sync::Notify>,
    /// One-shot injected failure of the next healthy-worker registration.
    #[cfg(feature = "test-support")]
    fail_next_registration: Arc<AtomicBool>,
    /// One-shot injected failure of the next recovery-predicate query.
    #[cfg(feature = "test-support")]
    fail_next_recovery_predicate: Arc<AtomicBool>,
    /// One-shot injected failure of the next worker-owned table-lease release.
    #[cfg(feature = "test-support")]
    fail_next_lease_release: Arc<AtomicBool>,
    /// One-shot pause before releasing the worker-owned table lease.
    #[cfg(feature = "test-support")]
    hold_next_release: Arc<AtomicBool>,
    /// Whether the lease release is parked after durable settlement.
    #[cfg(feature = "test-support")]
    release_paused: Arc<AtomicBool>,
    /// Wakes a test when settlement has reached lease release.
    #[cfg(feature = "test-support")]
    release_pause_ready: Arc<tokio::sync::Notify>,
    /// Lets the parked release continue through its real production boundary.
    #[cfg(feature = "test-support")]
    release_pause_resume: Arc<tokio::sync::Notify>,
    /// Count of slot joins fully processed by the production parent.
    #[cfg(feature = "test-support")]
    joined_slots: Arc<AtomicUsize>,
    /// Notifies tests that a secondary result has reached parent error selection.
    #[cfg(feature = "test-support")]
    joined_slot_ready: Arc<tokio::sync::Notify>,
    /// One-shot pause before post-fatal durable telemetry observation.
    #[cfg(feature = "test-support")]
    hold_next_fatal_observation: Arc<AtomicBool>,
    /// Whether the fatal slot is waiting to observe its durable result.
    #[cfg(feature = "test-support")]
    fatal_observation_paused: Arc<AtomicBool>,
    /// Notifies tests that fatal observation has reached its gate.
    #[cfg(feature = "test-support")]
    fatal_observation_ready: Arc<tokio::sync::Notify>,
    /// Releases the original fatal slot to finish required observation.
    #[cfg(feature = "test-support")]
    fatal_observation_resume: Arc<tokio::sync::Notify>,
    /// One-shot injected failure of the next cancelled-claim release.
    #[cfg(feature = "test-support")]
    fail_next_cancelled_claim_release: Arc<AtomicBool>,
    /// One-shot passive barrier after a dispatch returns durable settlement.
    #[cfg(feature = "test-support")]
    pause_after_next_settlement: Arc<AtomicBool>,
    /// Whether a durably settled dispatch is currently held at that barrier.
    #[cfg(feature = "test-support")]
    settlement_paused: Arc<AtomicBool>,
    /// Wakeup for tests waiting until the settled dispatch is held.
    #[cfg(feature = "test-support")]
    settlement_pause_ready: Arc<tokio::sync::Notify>,
    /// Release for the one settled dispatch held by the barrier.
    #[cfg(feature = "test-support")]
    settlement_pause_release: Arc<tokio::sync::Notify>,
    /// Extra trigger that makes the running claim heartbeat take one real beat.
    ///
    /// Part of the same settlement barrier: while a settled dispatch is held,
    /// a test uses this to drive the already-running heartbeat through its
    /// genuine post-settlement conflict instead of waiting on wall-clock ticks.
    #[cfg(feature = "test-support")]
    heartbeat_beat_now: Arc<tokio::sync::Notify>,
}

/// Typed causal evidence from the production Forge scheduler and worker owners.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(feature = "test-support")]
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
    /// Whether the test has released the barrier.
    ///
    /// The paused workers wait on this rather than on the claim count. A
    /// rendezvous of `expected` workers reports itself through `ready`, but the
    /// pause itself ends only when the test says so, so a single supervised
    /// worker can be held after its durable claim exactly like a pair can.
    released: AtomicBool,
}

#[cfg(feature = "test-support")]
impl ForgeWorkerCompletionObserver {
    /// Create an empty completion observer for one shared Forge worker topology.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Arm a one-shot failure of the next healthy-worker registration.
    ///
    /// A worker that cannot record itself healthy has no durable identity to
    /// claim under, so the startup boundary must be reachable in a test without
    /// a production configuration field. Consumed by the first registration
    /// that observes it, which is what lets the same worker respawn and recover.
    pub fn fail_next_registration(&self) {
        self.fail_next_registration.store(true, Ordering::Release);
    }

    /// Consume the armed one-shot registration failure, if any.
    #[must_use]
    fn take_registration_failure(&self) -> bool {
        self.fail_next_registration.swap(false, Ordering::AcqRel)
    }

    /// Arm a one-shot failure of the next recovery-predicate query.
    ///
    /// The predicate is the last thing standing between recovery and published
    /// readiness, so a failure there must leave the worker unready rather than
    /// optimistically ready.
    pub fn fail_next_recovery_predicate(&self) {
        self.fail_next_recovery_predicate
            .store(true, Ordering::Release);
    }

    /// Consume the armed one-shot recovery-predicate failure, if any.
    #[must_use]
    fn take_recovery_predicate_failure(&self) -> bool {
        self.fail_next_recovery_predicate
            .swap(false, Ordering::AcqRel)
    }

    /// Arm a one-shot failure of the next worker-owned table-lease release.
    ///
    /// A retained table fence lets this owner's next claim run against a table
    /// it can no longer prove it owns, so the release boundary is fatal and
    /// must be injectable at both worker-owned release sites.
    pub fn fail_next_lease_release(&self) {
        self.fail_next_lease_release.store(true, Ordering::Release);
    }

    /// Reports whether the one-shot lease-release failure is still armed.
    ///
    /// A test that injects this failure into a path that already failed for
    /// another reason cannot tell from the returned error whether the release
    /// site was reached at all. Reading the disarmed flag proves it was.
    #[must_use]
    pub fn lease_release_failure_armed(&self) -> bool {
        self.fail_next_lease_release.load(Ordering::Acquire)
    }

    /// Consume the armed one-shot lease-release failure, if any.
    #[must_use]
    fn take_lease_release_failure(&self) -> bool {
        self.fail_next_lease_release.swap(false, Ordering::AcqRel)
    }

    /// Arm a one-shot failure of the next cancelled-claim release.
    ///
    /// A shutdown that cannot drain its own claim leaves durable state this
    /// owner still holds, so it must report the failure rather than exit clean.
    pub fn fail_next_cancelled_claim_release(&self) {
        self.fail_next_cancelled_claim_release
            .store(true, Ordering::Release);
    }

    /// Consume the armed one-shot cancelled-claim-release failure, if any.
    #[must_use]
    fn take_cancelled_claim_release_failure(&self) -> bool {
        self.fail_next_cancelled_claim_release
            .swap(false, Ordering::AcqRel)
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

    /// Return the typed unsettled-output evidence each returned error carried.
    ///
    /// Index-aligned with [`Self::returned_errors`]: entry `i` is `Some` only
    /// when that attempt ended as [`ForgeError::RewriteUnsettled`], in which
    /// case it is the exact possible-output set the wrapper preserved. Passive
    /// diagnostic evidence; recording it cannot affect any durable transition.
    ///
    /// Available only under `test-support`, alongside the field it reads: the
    /// default production surface of this observer does not expose it.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn returned_unsettled_outputs(
        &self,
    ) -> Vec<Option<Vec<crate::forge::managed::ForgeUnsettledOutput>>> {
        self.returned_unsettled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Record one admitted rewrite attempt's publication evidence.
    ///
    /// Passive: the record is appended after the evidence already exists and
    /// is never read by any durable path, so it cannot affect scheduling,
    /// publication, or settlement.
    #[cfg(feature = "test-support")]
    pub(crate) fn record_rewrite_evidence_for_test(&self, record: ForgeRewriteEvidenceRecord) {
        self.rewrite_evidence
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(record);
        self.ready.notify_waiters();
    }

    /// Return the publication evidence every admitted rewrite attempt produced.
    ///
    /// Ordered by observation. A journey selects the entry whose identities
    /// match the snapshot it is inspecting rather than assuming a position,
    /// because one pod legitimately runs rewrites for several tables.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn rewrite_evidence_for_test(&self) -> Vec<ForgeRewriteEvidenceRecord> {
        self.rewrite_evidence
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

    /// Hold the next managed rewrite once its handoff and outputs exist.
    ///
    /// The barrier is passive and sits after managed execution produced the
    /// five-field handoff and before publication reacquires authoritative
    /// metadata. It never manufactures a handoff, changes a verdict, skips IO,
    /// publishes, or settles: a held rewrite resumes into exactly the same
    /// production decisions it would have taken without the barrier. Tests use
    /// it to mutate real publication authority through its own owner while the
    /// rewrite is provably past execution and provably before validation.
    #[cfg(feature = "test-support")]
    pub fn hold_after_next_rewrite_handoff_for_test(&self) {
        self.handoff_paused.store(false, Ordering::Release);
        self.pause_after_next_handoff.store(true, Ordering::Release);
    }

    /// Wait until the armed post-handoff barrier is holding one rewrite.
    #[cfg(feature = "test-support")]
    pub async fn wait_for_held_rewrite_handoff_for_test(&self) {
        loop {
            let notified = self.handoff_pause_ready.notified();
            if self.handoff_paused.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    /// Release the one rewrite held after its managed handoff.
    #[cfg(feature = "test-support")]
    pub fn release_held_rewrite_handoff_for_test(&self) {
        self.handoff_pause_release.notify_one();
    }

    /// Hold the next dispatch that returned durable settlement.
    ///
    /// The barrier is passive and sits immediately after the dispatch returned
    /// [`ForgeExecutionEvidenceState::Settled`] and before the worker reads
    /// shutdown or joins the claim heartbeat. The task and its operation are
    /// already terminal when it runs, so holding there changes only fixture
    /// timing: it lets a test make a concurrent shutdown or an obsolete
    /// heartbeat conflict provably precede the worker's own interpretation.
    #[cfg(feature = "test-support")]
    pub fn hold_after_next_durable_settlement_for_test(&self) {
        self.settlement_paused.store(false, Ordering::Release);
        self.pause_after_next_settlement
            .store(true, Ordering::Release);
    }

    /// Wait until the armed post-settlement barrier is holding one dispatch.
    #[cfg(feature = "test-support")]
    pub async fn wait_for_held_durable_settlement_for_test(&self) {
        loop {
            let notified = self.settlement_pause_ready.notified();
            if self.settlement_paused.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    /// Release the one dispatch held after its durable settlement.
    #[cfg(feature = "test-support")]
    pub fn release_held_durable_settlement_for_test(&self) {
        self.settlement_pause_release.notify_one();
    }

    /// Make the running claim heartbeat take one immediate real beat.
    ///
    /// The permit is stored, so the beat happens on the heartbeat's next select
    /// pass whether or not it is already parked, and it takes priority over
    /// cancellation. The beat itself is the production statement: against an
    /// already-settled task it returns the genuine obsolete-claim conflict.
    #[cfg(feature = "test-support")]
    pub fn beat_claim_heartbeat_now_for_test(&self) {
        self.heartbeat_beat_now.notify_one();
    }

    /// Shares the extra heartbeat trigger with the spawned heartbeat task.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn heartbeat_beat_signal(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.heartbeat_beat_now)
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

    /// Waits until the parent has processed the requested number of slot joins.
    /// Callers bound this diagnostic wait and cancel spawned work on timeout.
    pub async fn wait_for_joined_slots_for_test(&self, expected: usize) {
        loop {
            let notified = self.joined_slot_ready.notified();
            if self.joined_slots.load(Ordering::Acquire) >= expected {
                return;
            }
            notified.await;
        }
    }

    /// Holds the next fatal result immediately before durable telemetry observation.
    pub fn hold_before_fatal_observation_for_test(&self) {
        self.fatal_observation_paused
            .store(false, Ordering::Release);
        self.hold_next_fatal_observation
            .store(true, Ordering::Release);
    }

    /// Waits for the fatal observation gate; callers bound and cancel their work.
    pub async fn wait_for_fatal_observation_for_test(&self) {
        loop {
            let notified = self.fatal_observation_ready.notified();
            if self.fatal_observation_paused.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    /// Releases fatal observation without changing its durable result or error.
    pub fn release_fatal_observation_for_test(&self) {
        self.fatal_observation_resume.notify_one();
    }

    /// Parks one fatal slot until the test permits its telemetry read to proceed.
    async fn pause_fatal_observation_for_test(&self) {
        if self
            .hold_next_fatal_observation
            .swap(false, Ordering::AcqRel)
        {
            self.fatal_observation_paused.store(true, Ordering::Release);
            self.fatal_observation_ready.notify_waiters();
            self.fatal_observation_resume.notified().await;
        }
    }

    /// Parks the next table-lease release after execution and durable settlement.
    pub fn hold_before_next_lease_release_for_test(&self) {
        self.release_paused.store(false, Ordering::Release);
        self.hold_next_release.store(true, Ordering::Release);
    }

    /// Waits until the armed release reaches the production worker boundary.
    ///
    /// Callers own a bounded diagnostic timeout and cancellation of spawned work.
    pub async fn wait_for_held_lease_release_for_test(&self) {
        loop {
            let notified = self.release_pause_ready.notified();
            if self.release_paused.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    /// Releases the parked table-lease operation without changing its result.
    pub fn release_held_lease_release_for_test(&self) {
        self.release_pause_resume.notify_one();
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
                released: AtomicBool::new(false),
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
        gate.released.store(true, Ordering::Release);
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
            // Test-support only: the typed retention below is the sole place
            // an unsettled attempt's output paths are cloned, so a default
            // production build performs none of that work.
            #[cfg(feature = "test-support")]
            self.returned_unsettled
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(match error {
                    ForgeError::RewriteUnsettled {
                        possible_outputs, ..
                    } => Some(possible_outputs.clone()),
                    _ => None,
                });
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

    /// Pause once after an armed managed rewrite handoff exists.
    #[cfg(feature = "test-support")]
    async fn pause_after_handoff_for_test(&self) {
        if !self.pause_after_next_handoff.swap(false, Ordering::AcqRel) {
            return;
        }
        self.handoff_paused.store(true, Ordering::Release);
        self.handoff_pause_ready.notify_waiters();
        self.handoff_pause_release.notified().await;
        self.handoff_paused.store(false, Ordering::Release);
    }

    /// Pause once after an armed dispatch returned durable settlement.
    #[cfg(feature = "test-support")]
    async fn pause_after_settlement_for_test(&self) {
        if !self
            .pause_after_next_settlement
            .swap(false, Ordering::AcqRel)
        {
            return;
        }
        self.settlement_paused.store(true, Ordering::Release);
        self.settlement_pause_ready.notify_waiters();
        self.settlement_pause_release.notified().await;
        self.settlement_paused.store(false, Ordering::Release);
    }

    /// Observe one persisted claim and pause until the test releases the barrier.
    ///
    /// The claim itself is already durable when this runs, so the pause changes
    /// only the fixture's execution timing, never claim fairness, ownership,
    /// task state, or publication behavior.
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
            if gate.released.load(Ordering::Acquire) {
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
            per_tenant_active_cap: 1,
            compaction_memory_budget_bytes: DEFAULT_COMPACTION_MEMORY_BUDGET_BYTES,
            max_task_parallelism: 3,
            pending_task_parallelism: 12,
        }
    }
}

impl ForgeWorkerConfig {
    /// Validates the fixed worker bounds before the event loop starts.
    ///
    /// # Errors
    ///
    /// Returns invalid configuration when the per-tenant active cap is zero (no
    /// task would ever be claimable), when the compaction memory budget is zero
    /// (no plan could ever run), when running parallelism is zero, or when the
    /// waiting budget cannot hold one maximally parallel plan.
    pub fn validate(self) -> Result<Self, ForgeError> {
        if self.per_tenant_active_cap == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge per-tenant active cap must be positive".to_owned(),
            });
        }
        if self.compaction_memory_budget_bytes == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge compaction memory budget must be positive".to_owned(),
            });
        }
        if self.max_task_parallelism == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge compaction running parallelism must be positive".to_owned(),
            });
        }
        if self.pending_task_parallelism < self.max_task_parallelism {
            return Err(ForgeError::InvalidConfig {
                detail: format!(
                    "Forge waiting parallelism {} cannot hold one maximally parallel plan of {}",
                    self.pending_task_parallelism, self.max_task_parallelism
                ),
            });
        }
        Ok(self)
    }
}

/// Join set of one worker's in-flight plan runners.
///
/// Each entry reports the durable task it belongs to, its planner ordinal, and
/// the plan's own publication result, because the worker admits plans from
/// several tasks at once and they complete in whatever order their objects and
/// commits allow.
/// One finished plan's exact queue key and the result it returned.
///
/// This is the only thing a spawned runner ever sends back. It carries the key
/// rather than the attempt so the event loop can release exactly the
/// reservation that plan held before it stores the outcome, which is what makes
/// a double release detectable rather than silently double-crediting a budget.
struct ForgePlanCompletion {
    /// Exact `(task_id, plan_index)` reservation this completion releases.
    key: super::managed::queue::ForgePlanKey,
    /// The plan's own typed publication result.
    outcome: Result<ForgeDispatchResult, ForgeError>,
}

/// Fenced execution state awaiting durable failure settlement and lease release.
struct ClaimExecutionOutcome<'task> {
    /// Durable task claim being settled.
    task: &'task ForgeTaskClaim,
    /// Exact claim attempt identity.
    attempt: Uuid,
    /// Closed metric stage derived from the validated payload.
    stage: ForgeExecutionStage,
    /// Table fence held through settlement.
    lease: ForgeLease,
    /// Fenced execution result: whether the requested effect settled.
    result: Result<bool, ForgeError>,
}

/// Classifies whether prepared evidence represents progress beyond its base.
fn prepared_effect_progressed(evidence: &ForgeTaskEvidence, base_snapshot_id: i64) -> bool {
    evidence.committed_snapshot_id != Some(base_snapshot_id) || evidence.deleted_candidate_count > 0
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
    #[cfg(feature = "test-support")]
    completion_observer: Option<ForgeWorkerCompletionObserver>,
    /// Readiness and stop bookkeeping, present only while the event loop runs.
    loop_handles: Option<ForgeWorkerLoopHandles>,
    /// Executor admitted compaction runners are spawned on.
    ///
    /// `None` on a direct fixture or an embedded deployment with no dedicated
    /// executor, in which case runners share the ambient server runtime. Only
    /// a [`tokio::runtime::Handle`] ever reaches this worker; the `Runtime`
    /// itself is owned by the server composition that outlives supervision.
    compaction_runtime: Option<tokio::runtime::Handle>,
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
        Ok(Self {
            #[cfg(feature = "test-support")]
            completion_observer: forge.core.completion_observer.clone(),
            tasks: ForgeTasks::new(forge.core.operator_pool.clone()),
            forge,
            owner,
            config,
            loop_handles: None,
            compaction_runtime: None,
        })
    }

    /// Selects the executor admitted compaction runners are spawned on.
    ///
    /// Composition hands the worker a [`tokio::runtime::Handle`] for the
    /// server-owned dedicated compaction runtime. Only admitted plan runners
    /// use it; claims, maintenance, reconciliation, and settlement stay on the
    /// ambient runtime, so a saturated rewrite cannot starve the loop that must
    /// settle it. A worker without one runs runners on the ambient runtime,
    /// which is what an embedded deployment and every direct fixture want.
    #[must_use]
    pub fn on_compaction_runtime(mut self, handle: tokio::runtime::Handle) -> Self {
        self.compaction_runtime = Some(handle);
        self
    }

    /// Closes readiness and stops in-flight runners without yielding.
    ///
    /// Direct execution and startup recovery run outside the event loop and
    /// therefore have no controls to close.
    fn close_after_fatal(&self) {
        let Some(controls) = &self.loop_handles else {
            return;
        };
        controls.readiness.publish(false);
        controls.stop.cancel();
    }

    /// Arms one failure after maintenance task Prepared commits but before expiry terminal audit.
    #[cfg(feature = "test-support")]
    pub fn fail_after_maintenance_prepared_for_test(&self) {
        self.forge
            .core
            .fail_after_maintenance_prepared
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
    /// Drains this worker's recoverable work before it can become ready.
    ///
    /// This is everything that must succeed before a worker may advertise
    /// itself: a durable healthy-worker registration, and a complete recovery
    /// drain of every `Prepared` attempt and lapsed claim it already owns.
    /// Readiness is published by the caller only when this returns `true`.
    ///
    /// # Errors
    ///
    /// Returns any settlement, reconciliation, audit, catalog, object-store, or
    /// SQL failure the recovery drain returns.
    ///
    /// Returns `Ok(false)` when shutdown was requested during recovery, so the
    /// caller returns without ever publishing readiness.
    async fn start_and_drain(&self, shutdown: &CancellationToken) -> Result<bool, ForgeError> {
        // Orphan collection resumes its bounded listing from a cursor, and
        // emulating that cursor by refiltering would relist every earlier page
        // on every attempt. A worker whose actual staging backend cannot resume
        // natively therefore cannot keep the anti-starvation guarantee its
        // cleanup authority depends on, so it refuses before recovery,
        // readiness, or any claim.
        if !self.forge.core.staging_lists_by_cursor {
            return Err(ForgeError::InvalidConfig {
                detail:
                    "Forge worker staging backend does not support native list_with_start_after"
                        .to_owned(),
            });
        }
        #[cfg(feature = "test-support")]
        if let Some(observer) = &self.completion_observer
            && observer.take_registration_failure()
        {
            return Err(ForgeError::Sql(vala_sql::SqlError::Conflict {
                detail: "injected Forge worker registration failure".to_owned(),
            }));
        }
        tracing::info!(worker = %self.owner, "Forge worker started");
        // Readiness is recovery-gated. A Prepared attempt or a lapsed claim is
        // durable evidence a reader can already observe, so this worker
        // resolves all of it before advertising itself and taking new work.
        self.drain_recoverable_work(shutdown).await?;
        Ok(!shutdown.is_cancelled())
    }

    /// Runs the one worker event loop until shutdown stops claims and drains.
    ///
    /// A worker is a single mutable loop, not a pool: local compaction
    /// concurrency comes from its admission queue, so there is nothing to fan
    /// out into slots and no cross-slot error selection to make. Cancellation
    /// stops new claims immediately; admitted work observes the same token at
    /// its established safe boundaries before the loop returns.
    ///
    /// # Errors
    ///
    /// Returns the first failure the loop could not settle. Readiness is
    /// retracted before returning, so a worker that stopped — cleanly, by
    /// refusal, or by failure — never keeps advertising itself.
    ///
    /// # Cancellation
    ///
    /// Cancellation retracts readiness, stops admitting new work, and drains
    /// what is already admitted before returning `Ok(())`.
    pub async fn run(
        mut self,
        shutdown: CancellationToken,
        readiness: ForgeRoleReadiness,
    ) -> Result<(), ForgeError> {
        // Cleared on every exit, so a worker that stopped — cleanly, by
        // quarantine, or by failure — never keeps advertising itself.
        let _readiness = ForgeWorkerReadinessGuard(readiness.clone());
        if !self.start_and_drain(&shutdown).await? {
            return Ok(());
        }
        readiness.publish(true);
        let stop = shutdown.child_token();
        self.loop_handles = Some(ForgeWorkerLoopHandles {
            readiness: readiness.clone(),
            stop: stop.clone(),
        });
        // Boxed because the loop's per-claim execution arms are large enough
        // that inlining the whole state machine here overflows a default task
        // stack in debug builds.
        let outcome = Box::pin(self.run_event_loop(stop)).await;
        if let Err(error) = outcome {
            readiness.publish(false);
            tracing::error!(worker = %self.owner, error = %error, "Forge worker stopped after it could not settle its work");
            return Err(error);
        }
        // Pairs with the start event so an operator can tell a worker that
        // drained cleanly from one that vanished.
        tracing::info!(worker = %self.owner, "Forge worker stopped");
        Ok(())
    }

    /// Resolves every durable attempt left unsettled before this worker starts.
    ///
    /// Reclaims lapsed claims and reconciles Prepared attempts until the durable
    /// predicate reports nothing recoverable. Ready and retryable demand is not
    /// recovery — the slots claim it once they start — so this terminates.
    ///
    /// # Errors
    ///
    /// Returns the SQL, reconciliation, settlement, or audit failures
    /// recovery itself raises. Each leaves durable state this owner cannot
    /// account for, so the worker must not become ready.
    ///
    /// # Cancellation
    ///
    /// Cancellation returns early with recovery incomplete and readiness
    /// unpublished, leaving the remaining attempts for a later owner.
    async fn drain_recoverable_work(&self, shutdown: &CancellationToken) -> Result<(), ForgeError> {
        let claim_limits = self.claim_limits()?;
        while !shutdown.is_cancelled() {
            // Reclaim is paged, so a single call can leave lapsed attempts
            // behind. Draining until one page comes back short is what makes
            // the predicate below an answer about the whole queue rather than
            // about one page of it.
            loop {
                if shutdown.is_cancelled() {
                    return Ok(());
                }
                let reclaimed = self
                    .reclaim_expired_attempts(claim_limits.max_active_per_tenant)
                    .await?;
                if reclaimed.len() < claim_limits.max_active_per_tenant as usize {
                    break;
                }
            }
            if shutdown.is_cancelled() {
                return Ok(());
            }
            let reconciled = self.reconcile_one_prepared(claim_limits, shutdown).await?;
            if shutdown.is_cancelled() {
                return Ok(());
            }
            let cleaned = self
                .execute_one_recoverable_cleanup(claim_limits, shutdown)
                .await?;
            #[cfg(feature = "test-support")]
            if let Some(observer) = &self.completion_observer
                && observer.take_recovery_predicate_failure()
            {
                return Err(ForgeError::Sql(vala_sql::SqlError::Conflict {
                    detail: "injected Forge worker registration failure".to_owned(),
                }));
            }
            if !self
                .tasks
                .has_recoverable_work(self.owner)
                .await
                .map_err(ForgeError::Sql)?
            {
                return Ok(());
            }
            if !reconciled && !cleaned {
                // Something is unresolved that this pass could not claim: a
                // deferred cursor waiting on its own timing gates, or an
                // attempt this owner holds whose lease has not lapsed yet. The
                // wait is cancellation-aware so shutdown never has to outlast
                // a lease, and it replaces what would otherwise be a hot spin
                // against Postgres before any slot exists.
                tokio::select! {
                    () = shutdown.cancelled() => return Ok(()),
                    () = tokio::time::sleep(Duration::from_millis(250)) => {}
                }
            }
        }
        Ok(())
    }

    /// Claims and settles one currently eligible recoverable cleanup cursor.
    ///
    /// An unowned cleanup row carrying evidence is residue: an earlier attempt
    /// deleted objects and checkpointed a scan it never finished. Recovery runs
    /// it through the ordinary execution and settlement path, so a durably
    /// settled non-success is progress rather than a recovery failure and the
    /// caller simply asks the predicate again.
    ///
    /// Returns whether a cursor was claimed at all, which is what tells the
    /// drain loop apart from a pass that made no progress and must wait.
    ///
    /// # Errors
    ///
    /// Returns the SQL, execution, settlement, or audit failure the attempt
    /// raised. Each leaves durable state this owner cannot account for, so the
    /// worker must not become ready.
    ///
    /// # Cancellation
    ///
    /// Cancellation is observed at [`Self::execute_claim`]'s existing safe
    /// boundaries and leaves the cursor durable for a later owner.
    async fn execute_one_recoverable_cleanup(
        &self,
        claim_limits: ForgeClaimLimits,
        shutdown: &CancellationToken,
    ) -> Result<bool, ForgeError> {
        let Some(claim) = self
            .tasks
            .claim_recoverable_cleanup(self.owner, claim_limits)
            .await
            .map_err(ForgeError::Sql)?
        else {
            return Ok(false);
        };
        let task_id = claim.task_id;
        let strategy = claim.strategy.clone();
        #[cfg(feature = "test-support")]
        if let Some(observer) = &self.completion_observer {
            observer.record_lifecycle(ForgeLifecycleEvent::Claimed {
                task_id,
                worker_id: self.owner,
            });
        }
        tracing::info!(
            worker = %self.owner,
            task_id = %task_id,
            strategy = ?strategy,
            "Forge recovery claimed an unfinished cleanup cursor"
        );
        // Either healthy boolean is progress: a settled refusal or retry is a
        // durable result, and the predicate decides whether more remains.
        // Boxed for the same reason the event loop is: execution nests deeply, and
        // holding that whole state machine inline inside the startup drain
        // pushes the composed server future past rustc's layout-query budget.
        let result = Box::pin(self.execute_claim(claim, shutdown)).await;
        if result? {
            self.record_completion(task_id, strategy);
        }
        Ok(true)
    }

    /// Releases one worker-owned table fence through the single fault seam.
    ///
    /// Both worker-owned release sites route here so an injected release
    /// failure exercises exactly the boundary production uses. The injection
    /// precedes the real release, so the fence stays held and the durable state
    /// matches what a genuine release failure would leave behind.
    ///
    /// # Errors
    ///
    /// Returns the lease layer's release failure, or the injected typed SQL
    /// conflict when a test armed the one-shot seam.
    async fn release_table_lease(&self, lease: &mut ForgeLease) -> Result<bool, ForgeError> {
        #[cfg(feature = "test-support")]
        if let Some(observer) = &self.completion_observer
            && observer.hold_next_release.swap(false, Ordering::AcqRel)
        {
            observer.release_paused.store(true, Ordering::Release);
            observer.release_pause_ready.notify_waiters();
            observer.release_pause_resume.notified().await;
            observer.release_paused.store(false, Ordering::Release);
        }
        #[cfg(feature = "test-support")]
        if let Some(observer) = &self.completion_observer
            && observer.take_lease_release_failure()
        {
            return Err(ForgeError::Sql(vala_sql::SqlError::Conflict {
                detail: "injected Forge table lease release failure".to_owned(),
            }));
        }
        lease.release(&self.forge.core.operator_pool).await
    }

    /// Reclaims expired durable attempts.
    ///
    /// # Errors
    /// Returns SQL errors from the bounded operator reclaim.
    async fn reclaim_expired_attempts(&self, cap: u32) -> Result<Vec<(Uuid, Uuid)>, ForgeError> {
        self.tasks
            .reclaim_expired_attempts(cap)
            .await
            .map_err(ForgeError::Sql)
    }

    /// Reclaims expired attempts through the production worker owner in tests.
    ///
    /// # Errors
    ///
    /// Returns the production SQL failure.
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
    /// Returns only failures that make a further claim by this owner unsafe:
    /// configuration, durable settlement, audit, SQL, reconciliation, claim, and
    /// cancellation-release failures. Each leaves durable state that no longer
    /// describes what this owner did, so the loop stops rather than claiming
    /// again. An execution failure this attempt durably settled is not one of
    /// them: it stays a healthy exit that permits the next claim.
    async fn run_event_loop(&mut self, shutdown: CancellationToken) -> Result<(), ForgeError> {
        let claim_limits = self.claim_limits()?;
        // One worker owns exactly one maintenance execution position, and it is
        // outside the compaction queue: maintenance strategies are tried first
        // on every pass so a ready snapshot expiry is never starved behind
        // compaction backlog.
        let reserved_maintenance = true;
        // One FIFO, one attempt map, and one completion channel for the whole
        // worker. Constructing them here rather than per attempt is what makes
        // the parallelism and memory budgets describe this worker's real load:
        // two tasks claimed a moment apart compete for the same room, in the
        // order their plans were offered.
        let mut pool = ForgeAttemptPool::new(&self.config);
        // The one delayed cadence this loop waits on. `Delay` is what keeps a
        // process that was blocked past several periods from firing a burst of
        // catalog reconciliation passes when it resumes.
        let period = self.forge.core.maintenance_interval;
        let mut maintenance =
            tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let stranded = self.start_fitting_plans(&mut pool, &shutdown);
            for state in stranded {
                self.settle_or_retain_attempt(&mut pool, state, &shutdown)
                    .await?;
            }
            if shutdown.is_cancelled() {
                // A stopped worker still owns the plans it started: their
                // publications are durable and their attempts hold leases and
                // operations only this owner can settle.
                if pool.is_idle() {
                    return Ok(());
                }
                if pool.has_plans_in_flight() {
                    Box::pin(self.await_loop_event(&mut pool, &mut maintenance, &shutdown)).await?;
                    continue;
                }
                // Nothing is in flight, so every attempt still held is one this
                // owner cannot account for. A stopping worker cannot wait for
                // an operation to become provable and must not guess: it hands
                // the Prepared authority to its successor untouched.
                for task_id in pool.retained_task_ids() {
                    if let Some(state) = pool.attempts.remove(&task_id) {
                        self.hand_off_retained_attempt(state).await;
                    }
                }
                continue;
            }
            if pool.has_local_ambiguity() {
                // An owner that cannot say what its own operation did has no
                // business taking more of them: it stops claiming and retracts
                // readiness, while the plans it already admitted keep running
                // and settling normally.
                self.publish_readiness(false);
                Box::pin(self.await_loop_event(&mut pool, &mut maintenance, &shutdown)).await?;
                continue;
            }
            self.reclaim_expired_attempts(claim_limits.max_active_per_tenant)
                .await?;
            // Checked immediately before each durable claim so a stop signal
            // lands before this loop takes new work.
            if shutdown.is_cancelled() {
                continue;
            }
            if self.reconcile_one_prepared(claim_limits, &shutdown).await? {
                continue;
            }
            if shutdown.is_cancelled() {
                continue;
            }
            // Completions that are already waiting are taken before any new
            // authority is acquired, so the budget a finished plan freed is the
            // budget the pull calculation below sees.
            while let Ok(completion) = pool.completion_rx.try_recv() {
                if let Some(state) = self.record_plan_completion(&mut pool, completion)? {
                    self.settle_or_retain_attempt(&mut pool, state, &shutdown)
                        .await?;
                }
            }
            if pool.has_local_ambiguity() {
                continue;
            }
            self.publish_readiness(true);
            if (self.config.max_task_parallelism - pool.queue.running_parallelism_sum()).min(4) == 0
            {
                // No running parallelism remains, so this turn acquires no
                // authority at all. Waiting plans keep their pending
                // reservation; it does not authorize a pull.
                Box::pin(self.await_loop_event(&mut pool, &mut maintenance, &shutdown)).await?;
                continue;
            }
            let Some(claimed) = self
                .pull_claimed_tasks(&mut pool, claim_limits, reserved_maintenance, &shutdown)
                .await?
            else {
                return Ok(());
            };
            if claimed > 0 {
                continue;
            }
            if pool.is_idle() {
                tokio::select! {
                    () = shutdown.cancelled() => return Ok(()),
                    () = tokio::time::sleep(Duration::from_millis(250)) => {}
                }
                continue;
            }
            // Nothing to claim while this worker still holds work: waiting on a
            // running plan is the idle wait, because polling for new work on a
            // timer would delay the settlement that frees its budget.
            Box::pin(self.await_loop_event(&mut pool, &mut maintenance, &shutdown)).await?;
        }
    }

    /// Claims and admits tasks for one pull turn, up to this worker's free room.
    ///
    /// The pull budget is recomputed after every claim's plans are offered, so
    /// a task that filled the queue ends the turn instead of letting the next
    /// claim overcommit the same parallelism. Claims are taken one at a time
    /// through the fair SQL selection, which is what keeps two workers from
    /// splitting a tenant's backlog unevenly.
    ///
    /// Returns the number of tasks claimed, or `None` when a stop signal or a
    /// test-support abandonment ended this worker's loop.
    ///
    /// # Errors
    ///
    /// Returns the SQL, planning, settlement, audit, and lease failures the
    /// claim and its episode raise.
    async fn pull_claimed_tasks(
        &self,
        pool: &mut ForgeAttemptPool,
        claim_limits: ForgeClaimLimits,
        reserved_maintenance: bool,
        shutdown: &CancellationToken,
    ) -> Result<Option<u32>, ForgeError> {
redacted
        // the parallelism this worker still has free, and never more than four
        // tasks.
        let mut pending_pull_task_count =
            (self.config.max_task_parallelism - pool.queue.running_parallelism_sum()).min(4);
        let mut claimed = 0_u32;
        while pending_pull_task_count > 0 {
            let claim = self
                .claim_next(claim_limits, reserved_maintenance)
                .await
                .map_err(ForgeError::Sql)?;
            let Some(claim) = claim else {
                break;
            };
            claimed += 1;
            let started = Instant::now();
            let active = Self::metric_strategy(&claim.strategy)
                .map(|strategy| self.forge.core.telemetry.active_task(strategy));
            let task_id = claim.task_id;
            #[cfg(feature = "test-support")]
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
                    return Ok(None);
                }
            }
            if shutdown.is_cancelled() {
                self.release_claim_at_shutdown(&claim, started).await?;
                return Ok(None);
            }
            let strategy = claim.strategy.clone();
            // One INFO per claimed task, not per file or per row: Forge tasks are
            // coarse, so this stays bounded by compaction throughput and gives an
            // operator the claim/settle pair that shows whether work is moving.
            tracing::info!(
                worker = %self.owner,
                task_id = %task_id,
                strategy = ?strategy,
                "Forge task claimed"
            );
            let open = Self::open_claim_episode(claim, started, active);
            match self.begin_claim_episode(open, shutdown, pool).await {
                // The attempt's plans are on the queue; it settles when they drain.
                ClaimStep::Admitted => {}
                ClaimStep::Closed(result) => {
                    self.record_settled_claim(task_id, strategy, started, result)
                        .await?;
                }
            }
            // Recomputed after this task's plans were offered, so the next
            // claim of the same turn sees the room they took.
            pending_pull_task_count =
                (self.config.max_task_parallelism - pool.queue.running_parallelism_sum()).min(4);
        }
        Ok(Some(claimed))
    }

    /// Waits for the next plan completion, maintenance tick, or stop signal.
    ///
    /// This is the loop's only wait once it holds work. A completion is
    /// recorded and, when it drained its attempt, settled or retained; a tick
    /// runs one complete exact-operation reconciliation pass over every
    /// retained attempt; a stop signal returns so the caller re-reads it.
    ///
    /// # Errors
    ///
    /// Returns the invariant, reconciliation, settlement, audit, SQL, and
    /// lease-release failures the recorded completion or the pass raises.
    async fn await_loop_event(
        &self,
        pool: &mut ForgeAttemptPool,
        maintenance: &mut tokio::time::Interval,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let mut ticked = false;
        let completion = tokio::select! {
            () = shutdown.cancelled() => None,
            completion = pool.completion_rx.recv() => completion,
            _ = maintenance.tick() => {
                ticked = true;
                None
            }
        };
        if let Some(completion) = completion {
            if let Some(state) = self.record_plan_completion(pool, completion)? {
                self.settle_or_retain_attempt(pool, state, shutdown).await?;
            }
        } else if ticked {
            self.reconcile_retained_attempts(pool, shutdown).await?;
        }
        Ok(())
    }

    /// Leaves one unresolved attempt's Prepared authority for its successor.
    ///
    /// A stopping owner writes nothing durable for an attempt it cannot
    /// account for: no retry, no failure, no success, no planning demand, and
    /// no terminal audit. It closes only what is local — the heartbeat and the
    /// table fence — and leaves the task `Running` with its operation
    /// `Prepared`, which is exactly the state a lost process leaves and the one
    /// the table-wide reconciliation owner takes over from.
    ///
    /// A local closure failure is logged rather than returned: this worker is
    /// already stopping, nothing durable depends on the closure, and a
    /// retained fence lapses on its own TTL.
    async fn hand_off_retained_attempt(&self, state: ForgeAttemptState) {
        let ForgeAttemptState {
            open,
            mut lease,
            fenced,
            shared,
            ..
        } = state;
        let task_id = open.claim.task_id;
        let FencedAttempt {
            operation_stop,
            heartbeat,
            ..
        } = fenced;
        operation_stop.cancel();
        match heartbeat.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::debug!(
                worker = %self.owner, task_id = %task_id, error = %error,
                "Forge claim heartbeat ended before its retained attempt was handed off"
            ),
            Err(error) => tracing::warn!(
                worker = %self.owner, task_id = %task_id, error = %error,
                "Forge claim heartbeat did not join at shutdown"
            ),
        }
        if let Err(error) = self.release_table_lease(&mut lease).await {
            tracing::warn!(
                worker = %self.owner, task_id = %task_id, error = %error,
                "Forge table lease release failed at shutdown; the fence lapses on its TTL"
            );
        }
        // The one final snapshot of what this attempt may have written is read
        // before its shared context is dropped, so a successor's operator has
        // the object names even though nothing durable is settled here.
        tracing::warn!(
            worker = %self.owner,
            task_id = %task_id,
            possible_outputs = shared.rewrite.possible_outputs().len(),
            "Forge shutdown retained an unresolved attempt for table-wide takeover"
        );
        drop(shared);
        drop(open);
    }

    /// Publishes this loop's readiness, when it is running under a role handle.
    ///
    /// The loop is the only readiness decider, so retracting the bit while an
    /// operation is unresolved is what stops a gateway routing new work to an
    /// owner that has stopped claiming.
    fn publish_readiness(&self, ready: bool) {
        if let Some(handles) = &self.loop_handles {
            handles.readiness.publish(ready);
        }
    }

    /// Drains one claim this owner took as shutdown was signalled.
    ///
    /// The claim is released back to `retryable` so a stopping worker leaves no
    /// task owned by a process that will never run it, and the episode is still
    /// observed so its duration is not lost.
    ///
    /// # Errors
    ///
    /// Returns the release failure, which leaves the claim for lease-expiry
    /// recovery and stops this owner.
    async fn release_claim_at_shutdown(
        &self,
        claim: &ForgeTaskClaim,
        started: Instant,
    ) -> Result<(), ForgeError> {
        let task_id = claim.task_id;
        let released = if let Some(attempt) = claim.attempt_id {
            self.release_cancelled_claim(task_id, attempt).await
        } else {
            Ok(())
        };
        if let Err(error) = &released {
            self.close_after_fatal();
            tracing::error!(worker = %self.owner, task_id = %task_id, error = %error,
                "Forge claim shutdown release failed; durable state retained for lease recovery");
        }
        self.record_task_execution_telemetry(
            claim.data_tenant_id,
            task_id,
            Self::metric_strategy(&claim.strategy),
            &tracing::Span::none(),
            started,
        )
        .await;
        released
    }

    /// Retains one drained attempt whose operations are unresolved, or settles it.
    ///
    /// The reducer runs first, as a predicate: any plan this attempt is known
    /// to have published decides the task immediately and leaves its ambiguous
    /// siblings' Prepared rows to the table-wide reconciliation owner. Only an
    /// attempt with no known success and at least one unresolved operation is
    /// retained, because for that one nothing durable can be written yet — a
    /// failure would be a guess and a retry would republish an effect that may
    /// already be live.
    ///
    /// # Errors
    ///
    /// Returns the settlement, audit, SQL, or lease-release failure that makes
    /// a further claim by this owner unsafe.
    async fn settle_or_retain_attempt(
        &self,
        pool: &mut ForgeAttemptPool,
        state: ForgeAttemptState,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        if !Self::retains_unknown_acceptance(&state) {
            return self.settle_pooled_attempt(state, shutdown).await;
        }
        tracing::warn!(
            worker = %self.owner,
            task_id = %state.open.claim.task_id,
            unresolved = Self::unknown_operations(&state).len(),
            "Forge task retained: an operation's acceptance is unknown and no plan is known to have published"
        );
        pool.attempts.insert(state.open.claim.task_id, state);
        Ok(())
    }

    /// Returns whether this drained attempt must be retained rather than settled.
    fn retains_unknown_acceptance(state: &ForgeAttemptState) -> bool {
        let known_success = state.outcomes.iter().any(|(_, outcome)| {
            matches!(outcome, Ok(result) if !matches!(result, ForgeDispatchResult::AcceptanceUnknown(_)))
        });
        !known_success && !Self::unknown_operations(state).is_empty()
    }

    /// Returns this attempt's unresolved operations, in plan-index order.
    fn unknown_operations(state: &ForgeAttemptState) -> Vec<(usize, Uuid)> {
        let mut unresolved: Vec<(usize, Uuid)> = state
            .outcomes
            .iter()
            .filter_map(|(plan_index, outcome)| match outcome {
                Ok(ForgeDispatchResult::AcceptanceUnknown(unknown)) => {
                    Some((*plan_index, unknown.operation_id))
                }
                _ => None,
            })
            .collect();
        unresolved.sort_unstable();
        unresolved
    }

    /// Reconciles every retained attempt's unresolved operations once.
    ///
    /// One pass visits every retained attempt in task order and every one of
    /// that attempt's unresolved operations, so a worker holding several
    /// ambiguous attempts resolves all of them on one delayed cadence rather
    /// than one per tick. An attempt leaves the pool only when it is decided:
    /// the first operation proven live settles the task successfully and
    /// leaves any other unresolved row to the table-wide owner, and an attempt
    /// whose every operation is proven absent settles with the failure its
    /// plans returned.
    ///
    /// # Errors
    ///
    /// Returns the SQL, catalog, storage, fence, and settlement failures
    /// reconciliation and settlement raise. Each leaves the retained attempt
    /// durable for a later owner.
    async fn reconcile_retained_attempts(
        &self,
        pool: &mut ForgeAttemptPool,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        for task_id in pool.retained_task_ids() {
            let Some(state) = pool.attempts.get_mut(&task_id) else {
                continue;
            };
            if !self.reconcile_retained_attempt(state).await? {
                continue;
            }
            // The reducer runs once, after this attempt's pass completed, and
            // only because that pass changed at least one phase.
            if let Some(state) = pool.attempts.remove(&task_id) {
                self.settle_pooled_attempt(state, shutdown).await?;
            }
        }
        Ok(())
    }

    /// Reconciles one retained attempt's unresolved operations, in plan order.
    ///
    /// Each pass walks the ordered unresolved list to completion — no early
    /// return on the first change — and asks the durable operation row and the
    /// retained table evidence what each one did, updating the stored outcome
    /// in place. Nothing is resubmitted, and an operation still unproven is
    /// left exactly as it was.
    ///
    /// Returns whether this pass changed a phase and left the attempt decided,
    /// which is the only condition under which its caller reduces and settles.
    ///
    /// # Errors
    ///
    /// Returns the SQL, catalog, storage, fence, and clock failures exact
    /// reconciliation raises, and [`ForgeError::Invariant`] when the recovered
    /// table can no longer be loaded.
    async fn reconcile_retained_attempt(
        &self,
        state: &mut ForgeAttemptState,
    ) -> Result<bool, ForgeError> {
        let key = super::compact::ForgeTableKey {
            tenant: state.open.claim.data_tenant_id,
            table_ref: state.binding.table_ref.clone(),
        };
        let now = self.forge.core.clock.now()?;
        let stop = state.fenced.dispatch_stop().clone();
        let mut changed = false;
        for (plan_index, operation_id) in Self::unknown_operations(state) {
            let settlement = self
                .forge
                .reconcile_exact_live_operation(
                    &mut state.lease,
                    &key,
                    &state.binding,
                    &stop,
                    now,
                    operation_id,
                )
                .await?;
            match settlement {
                super::live_reconcile::ForgeLiveSettlement::Prepared => {}
                super::live_reconcile::ForgeLiveSettlement::Recovered(_) => {
                    // The retained request volume is the exact one this plan
                    // submitted, so recovery reports it rather than the
                    // reconciler emitting a second copy of the same effect.
                    let table = self.forge.load_table(&state.binding.table_ident()).await?;
                    let unknown = Self::take_retained_unknown(state, plan_index);
                    Self::replace_plan_outcome(
                        state,
                        plan_index,
                        Ok(ForgeDispatchResult::Committed(Box::new(
                            ForgeCommittedPublication {
                                table,
                                volume: Some(unknown.volume),
                            },
                        ))),
                    );
                    changed = true;
                }
                super::live_reconcile::ForgeLiveSettlement::Reset => {
                    let unknown = Self::take_retained_unknown(state, plan_index);
                    Self::replace_plan_outcome(state, plan_index, Err(unknown.error));
                    changed = true;
                }
            }
        }
        Ok(changed && !Self::retains_unknown_acceptance(state))
    }

    /// Removes and returns one unresolved plan's retained payload.
    ///
    /// The slot is left holding a placeholder its caller overwrites in the same
    /// step, which is what lets the exact operation identity, typed error, and
    /// request volume move out of the attempt without cloning any of them.
    ///
    /// # Panics
    ///
    /// Panics when `plan_index` is not an unresolved plan of `state`, which
    /// every caller has already established.
    fn take_retained_unknown(
        state: &mut ForgeAttemptState,
        plan_index: usize,
    ) -> ForgeUnknownAcceptance {
        let slot = state
            .outcomes
            .iter_mut()
            .find(|(index, _)| *index == plan_index)
            .expect("the caller selected a plan this attempt holds");
        match std::mem::replace(&mut slot.1, Err(ForgeError::Shutdown)) {
            Ok(ForgeDispatchResult::AcceptanceUnknown(unknown)) => *unknown,
            _ => unreachable!("the caller selected an unresolved plan"),
        }
    }

    /// Replaces one plan's retained outcome with its settled result.
    fn replace_plan_outcome(
        state: &mut ForgeAttemptState,
        plan_index: usize,
        settled: Result<ForgeDispatchResult, ForgeError>,
    ) {
        if let Some(slot) = state
            .outcomes
            .iter_mut()
            .find(|(index, _)| *index == plan_index)
        {
            slot.1 = settled;
        }
    }

    /// Settles one drained attempt and records the episode it completes.
    ///
    /// # Errors
    ///
    /// Returns the settlement, audit, SQL, or lease-release failure that makes
    /// a further claim by this owner unsafe.
    async fn settle_pooled_attempt(
        &self,
        state: ForgeAttemptState,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let task_id = state.open.claim.task_id;
        let strategy = state.open.claim.strategy.clone();
        let started = state.open.started;
        let result = self.finish_admitted_attempt(state, shutdown).await;
        self.record_settled_claim(task_id, strategy, started, result)
            .await
    }

    /// Records one settled episode and decides whether this owner keeps going.
    ///
    /// Only a settled effect is a completion; a superseded envelope, a
    /// terminalized payload, and a settled execution failure are all healthy
    /// exits that permit the next claim.
    ///
    /// # Errors
    ///
    /// Returns `result`'s failure unchanged, which stops this owner.
    async fn record_settled_claim(
        &self,
        task_id: Uuid,
        strategy: ForgeClaimStrategy,
        started: Instant,
        result: Result<bool, ForgeError>,
    ) -> Result<(), ForgeError> {
        tracing::info!(
            worker = %self.owner,
            task_id = %task_id,
            strategy = ?strategy,
            outcome = if result.is_ok() { "committed" } else { "failed" },
            elapsed_ms = started.elapsed().as_millis(),
            "Forge task settled"
        );
        #[cfg(feature = "test-support")]
        self.pause_after_attempt_for_test().await;
        match result {
            Ok(true) => self.record_completion(task_id, strategy),
            Ok(false) => {}
            Err(error) => return Err(error),
        }
        Ok(())
    }

    /// Reconciles one Prepared attempt this owner may claim, if any exists.
    ///
    /// Returns whether an attempt was reconciled, so the caller's slot loop can
    /// retry recovery before taking any ordinary claim. Recovery outranks new
    /// work: a Prepared attempt already produced durable evidence a reader can
    /// observe, so it must be resolved before this owner starts another effect.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Sql`] when the bounded recovery claim fails. A
    /// reconciliation or release failure retains exact evidence and stops the
    /// slot before any later claim.
    ///
    /// # Cancellation
    ///
    /// A cancelled reconciliation leaves the attempt durable for a later owner.
    async fn reconcile_one_prepared(
        &self,
        claim_limits: ForgeClaimLimits,
        shutdown: &CancellationToken,
    ) -> Result<bool, ForgeError> {
        let Some(prepared) = self
            .tasks
            .claim_prepared_for_reconciliation(self.owner, claim_limits.lease_seconds)
            .await
            .map_err(ForgeError::Sql)?
        else {
            return Ok(false);
        };
        let task_id = prepared.task.task_id;
        let strategy = ForgeClaimStrategy::Known(prepared.task.strategy);
        let result = self.reconcile_prepared(prepared, shutdown).await;
        #[cfg(feature = "test-support")]
        self.pause_after_attempt_for_test().await;
        match result {
            Ok(()) => self.record_completion(task_id, strategy),
            Err(error) => {
                tracing::error!(worker = %self.owner, error = %error, "Prepared Forge task reconciliation stopped; exact evidence retained");
                // A Prepared attempt already produced durable evidence a reader
                // can observe. Continuing past an unresolved one would let this
                // owner start another effect over unreconciled state.
                return Err(error);
            }
        }
        Ok(true)
    }

    /// Maps one claimed strategy onto its durable work type, if this build knows it.
    ///
    /// A strategy this build does not know has no `task_type`, so it is
    /// observed by the durable task row and the refusal path, not by a metric.
    fn metric_strategy(strategy: &ForgeClaimStrategy) -> Option<ForgeTaskStrategy> {
        match strategy {
            ForgeClaimStrategy::Known(known) => Some(*known),
            ForgeClaimStrategy::Unknown(_) => None,
        }
    }

    /// Publish one observer event after a successful worker execution.
    ///
    /// This is deliberately called after the full durable execution path
    /// returns, so test observation cannot acknowledge or alter a task.
    #[cfg(not(feature = "test-support"))]
    fn record_completion(&self, _task_id: Uuid, _strategy: ForgeClaimStrategy) {}

    /// Publish one observer event after a successful worker execution.
    #[cfg(feature = "test-support")]
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
    #[cfg(not(feature = "test-support"))]
    fn record_attempt(&self, _error: Option<&ForgeError>) {}

    /// Publish one passive observer event after any supervised attempt returns.
    #[cfg(feature = "test-support")]
    fn record_attempt(&self, error: Option<&ForgeError>) {
        if let Some(observer) = &self.completion_observer {
            observer.record_attempt(error);
        }
    }

    /// Publish the passive publication-evidence event for one admitted rewrite.
    /// Apply the observer's one-shot passive returned-attempt barrier.
    #[cfg(feature = "test-support")]
    async fn pause_after_attempt_for_test(&self) {
        if let Some(observer) = &self.completion_observer {
            observer.pause_after_attempt_for_test().await;
        }
    }

    /// Apply the observer's one-shot passive post-settlement barrier.
    ///
    /// Called only after a dispatch returned durable settlement and before the
    /// worker reads shutdown or joins the heartbeat. Without an armed observer
    /// it is a no-op, so no production decision depends on it.
    #[cfg(feature = "test-support")]
    async fn pause_after_settlement_for_test(&self) {
        if let Some(observer) = &self.completion_observer {
            observer.pause_after_settlement_for_test().await;
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

    /// Executes one already-claimed snapshot-expiry task through the real
    /// fenced path while phase activation is still owned by a later task.
    ///
    /// Everything a production attempt does is reused: the same
    /// [`Self::validate_payload_contract`] gate, the same table binding, the
    /// same table lease, [`Self::execute_fenced`], and the real
    /// [`Self::finish_claim_execution`] result. Only
    /// [`super::phase::admits_new_effect`] is bypassed, because snapshot
    /// expiration is not activated for production routing yet and refusing the
    /// claim here would prove nothing about its settlement.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the claim is not an
    /// exact snapshot-expiry task owned by this worker's attempt, when it fails
    /// the production payload contract,
    /// [`ForgeError::FenceLost`] when the table lease is held elsewhere, and
    /// every catalog, SQL, object-store, fencing, or audit failure the fenced
    /// execution itself raises.
    #[cfg(feature = "test-support")]
    pub async fn execute_snapshot_expiry_claim_for_test(
        &self,
        claim: ForgeTaskClaim,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        self.execute_maintenance_claim_for_test(ForgeTaskStrategy::SnapshotExpiry, claim, shutdown)
            .await
    }

    /// Executes one already-claimed expired-cleanup task through the real
    /// fenced path while phase activation is still owned by a later task.
    ///
    /// # Errors
    ///
    /// Returns the same failures as
    /// [`Self::execute_snapshot_expiry_claim_for_test`].
    #[cfg(feature = "test-support")]
    pub async fn execute_expired_cleanup_claim_for_test(
        &self,
        claim: ForgeTaskClaim,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        self.execute_maintenance_claim_for_test(ForgeTaskStrategy::ExpiredCleanup, claim, shutdown)
            .await
    }

    /// Executes one already-claimed orphan-cleanup task through the real
    /// fenced path while phase activation is still owned by a later task.
    ///
    /// # Errors
    ///
    /// Returns the same failures as
    /// [`Self::execute_snapshot_expiry_claim_for_test`].
    #[cfg(feature = "test-support")]
    pub async fn execute_orphan_cleanup_claim_for_test(
        &self,
        claim: ForgeTaskClaim,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        self.execute_maintenance_claim_for_test(ForgeTaskStrategy::OrphanCleanup, claim, shutdown)
            .await
    }

    /// Shared body of the phase-bypassing maintenance test entrypoints.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the claim is not an exact task of
    /// `expected` owned by this worker's attempt or fails the production
    /// payload contract, [`ForgeError::FenceLost`] when the table lease is held
    /// elsewhere, and every failure the fenced execution itself raises.
    #[cfg(feature = "test-support")]
    async fn execute_maintenance_claim_for_test(
        &self,
        expected: ForgeTaskStrategy,
        claim: ForgeTaskClaim,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let started = Instant::now();
        let _active = Self::metric_strategy(&claim.strategy)
            .map(|strategy| self.forge.core.telemetry.active_task(strategy));
        let result = self
            .execute_maintenance_claim_attempt_for_test(expected, &claim, shutdown)
            .await;
        self.record_task_execution_telemetry(
            claim.data_tenant_id,
            claim.task_id,
            Self::metric_strategy(&claim.strategy),
            &tracing::Span::none(),
            started,
        )
        .await;
        result
    }

    /// Runs the established maintenance adapter under its caller's ownership guard.
    ///
    /// # Errors
    /// Preserves the adapter's validation, fenced execution, and release semantics.
    #[cfg(feature = "test-support")]
    async fn execute_maintenance_claim_attempt_for_test(
        &self,
        expected: ForgeTaskStrategy,
        claim: &ForgeTaskClaim,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        if claim.strategy != ForgeClaimStrategy::Known(expected) {
            return Err(ForgeError::Invariant {
                detail: "the maintenance test entrypoint accepts only its own strategy".to_owned(),
            });
        }
        let attempt = claim.attempt_id.ok_or_else(|| ForgeError::Invariant {
            detail: "claimed Forge task has no attempt generation".to_owned(),
        })?;
        if claim.claimed_by != Some(self.owner) || claim.state != ForgeTaskState::Claimed {
            return Err(ForgeError::Invariant {
                detail: "Forge worker received a claim owned by another attempt".to_owned(),
            });
        }
        Self::validate_payload_contract(claim)?;
        let binding = task_table_binding(
            claim.data_tenant_id,
            claim.execution_tenant_id,
            &claim.table_ref,
        )?;
        let lease_key = forge_lease_key(
            claim.data_tenant_id,
            &binding.logical_namespace,
            &binding.table_name,
        );
        let mut lease = ForgeLease::acquire(
            &self.forge.core.operator_pool,
            lease_key,
            self.owner,
            self.forge.core.config.lease_ttl,
        )
        .await?
        .ok_or_else(|| ForgeError::FenceLost {
            lease_key: format!("forge:table:{}:{}", claim.data_tenant_id, binding.table_ref),
        })?;
        let result = self
            .open_rewrite_frame(claim, attempt, &binding, &mut lease, shutdown)
            .await;
        if let Err(error) = lease.release(&self.forge.core.operator_pool).await {
            tracing::warn!(task_id = %claim.task_id, error = %error, "Forge expiry test lease release failed");
        }
        result.map(|_| ())
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
        #[cfg(feature = "test-support")]
        if let Some(observer) = &self.completion_observer
            && observer.take_cancelled_claim_release_failure()
        {
            return Err(ForgeError::Sql(vala_sql::SqlError::Conflict {
                detail: "injected Forge cancelled claim release failure".to_owned(),
            }));
        }
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
        self.execute_claim(claim, shutdown).await.map(|_| ())
    }

    /// Executes one exact claimed task through validation, table fencing,
    /// bounded rewrite, exact evidence persistence, and terminal audit.
    ///
    /// Reserved, maintenance, and malformed payloads terminalize before catalog
    /// load, lease acquisition, or object IO. Supported tasks acquire the table
    /// lease before any catalog or object-store effect.
    ///
    /// Returns whether the requested strategy effect settled successfully. A
    /// superseded legacy envelope, a terminalized malformed payload, a
    /// superseded base snapshot, and an execution failure this attempt durably
    /// settled all return `Ok(false)`: nothing a reader can observe was
    /// produced, but this owner may take another claim. The durable task row
    /// remains the authority for what happened; the boolean is private control
    /// flow, not a second state taxonomy.
    ///
    /// # Errors
    ///
    /// Returns only failures that make a further claim by this owner unsafe:
    /// durable settlement, audit, SQL, claim, reconciliation, and table-lease
    /// release failures. An execution failure whose settlement and lease
    /// release both commit is reported as `Ok(false)` instead, and its error is
    /// still observed once by the attempt seam.
    pub async fn execute_claim(
        &self,
        claim: ForgeTaskClaim,
        shutdown: &CancellationToken,
    ) -> Result<bool, ForgeError> {
        let started = Instant::now();
        let active = Self::metric_strategy(&claim.strategy)
            .map(|strategy| self.forge.core.telemetry.active_task(strategy));
        let open = Self::open_claim_episode(claim, started, active);
        // One claim of its own still runs through the pool, because the pool is
        // the only implementation of plan admission and drain. A single-attempt
        // pool simply never has a sibling to interleave with.
        let mut pool = ForgeAttemptPool::new(&self.config);
        match self.begin_claim_episode(open, shutdown, &mut pool).await {
            ClaimStep::Closed(result) => result,
            ClaimStep::Admitted => self.drain_pool(&mut pool, shutdown).await,
        }
    }

    /// Drains every attempt suspended in one pool, reporting the last result.
    ///
    /// Used by the single-claim entry points, whose pool holds exactly one
    /// attempt; the event loop drains incrementally instead so it can keep
    /// claiming while plans run.
    ///
    /// # Errors
    ///
    /// Returns the first settlement failure that makes further work unsafe.
    async fn drain_pool(
        &self,
        pool: &mut ForgeAttemptPool,
        shutdown: &CancellationToken,
    ) -> Result<bool, ForgeError> {
        let mut settled = false;
        loop {
            for state in self.start_fitting_plans(pool, shutdown) {
                settled = self.finish_admitted_attempt(state, shutdown).await?;
            }
            if !pool.has_plans_in_flight() {
                return Ok(settled);
            }
            let Some(completion) = pool.completion_rx.recv().await else {
                return Ok(settled);
            };
            if let Some(state) = self.record_plan_completion(pool, completion)? {
                settled = self.finish_admitted_attempt(state, shutdown).await?;
            }
        }
    }

    /// Opens one ownership episode's observation frame.
    ///
    /// The claim-time instant and the active-task gauge are taken before any
    /// validation, so an episode that suspends on the worker-wide plan queue
    /// measures and reports exactly what an inline one does.
    fn open_claim_episode(
        claim: ForgeTaskClaim,
        started: Instant,
        active: Option<ForgeActiveTask>,
    ) -> OpenClaim {
        let span = tracing::info_span!(
            "bifrost.forge.task.execute",
            strategy = claim.strategy.as_str(),
            result = tracing::field::Empty,
            role = "forge_worker",
            task_id = %claim.task_id,
            attempt_id = tracing::field::Empty,
        );
        if let Some(attempt) = claim.attempt_id {
            span.record("attempt_id", tracing::field::display(attempt));
        }
        OpenClaim {
            claim,
            started,
            active,
            span,
        }
    }

    /// Starts one claimed episode, suspending it when it plans compaction work.
    ///
    /// Returns [`ClaimStep::Admitted`] once the attempt's plans are in the
    /// worker pool; the episode is then settled by
    /// [`Self::finish_admitted_attempt`] when its last plan drains. Every other
    /// strategy and every short-circuit settles and observes itself here, so
    /// both paths reach exactly one [`Self::close_claim_episode`].
    async fn begin_claim_episode(
        &self,
        open: OpenClaim,
        shutdown: &CancellationToken,
        pool: &mut ForgeAttemptPool,
    ) -> ClaimStep {
        // Exactly one passive observation per attempt. An execution failure the
        // attempt durably settled is reported at close even though the attempt
        // itself returns a healthy boolean, so the failure stays observable
        // without turning a settled outcome into a slot-fatal error.
        let mut settled_failure = None;
        let started = tracing::Instrument::instrument(
            self.begin_claim_attempt(&open.claim, shutdown, &mut settled_failure),
            open.span.clone(),
        )
        .await;
        match started {
            Ok(AttemptStart::Planned(admitted)) => {
                let AdmittedRewrite {
                    attempt,
                    binding,
                    stage,
                    lease,
                    fenced,
                    shared,
                    plans,
                } = *admitted;
                let refused = pool.admit(
                    self,
                    ForgeAttemptState {
                        open,
                        attempt,
                        binding,
                        stage,
                        lease,
                        fenced,
                        shared,
                        refusals: Vec::new(),
                        outcomes: Vec::new(),
                        queued: 0,
                        running: 0,
                    },
                    plans,
                );
                match refused {
                    Some(state) => {
                        ClaimStep::Closed(self.finish_admitted_attempt(state, shutdown).await)
                    }
                    None => ClaimStep::Admitted,
                }
            }
            Ok(AttemptStart::Settled(settled)) => ClaimStep::Closed(
                self.close_claim_episode(open, settled_failure, Ok(settled))
                    .await,
            ),
            Err(error) => ClaimStep::Closed(
                self.close_claim_episode(open, settled_failure, Err(error))
                    .await,
            ),
        }
    }

    /// Observes one finished episode exactly once and releases its frame.
    ///
    /// Both the inline and the pooled path end here, so an attempt is observed
    /// once no matter which one settled it. Telemetry is read from the durable
    /// row before the active-task gauge is released, so a reader never sees a
    /// task counted as active after its result was recorded.
    ///
    /// # Errors
    ///
    /// Returns `outcome` unchanged; observation is diagnostic-only.
    async fn close_claim_episode(
        &self,
        open: OpenClaim,
        settled_failure: Option<ForgeError>,
        outcome: Result<bool, ForgeError>,
    ) -> Result<bool, ForgeError> {
        let OpenClaim {
            claim,
            started,
            active,
            span,
        } = open;
        if outcome.is_err() {
            self.close_after_fatal();
        }
        #[cfg(feature = "test-support")]
        if outcome.is_err()
            && let Some(observer) = &self.completion_observer
        {
            observer.pause_fatal_observation_for_test().await;
        }
        self.record_task_execution_telemetry(
            claim.data_tenant_id,
            claim.task_id,
            Self::metric_strategy(&claim.strategy),
            &span,
            started,
        )
        .await;
        drop(span);
        self.record_attempt(settled_failure.as_ref().or(outcome.as_ref().err()));
        drop(active);
        outcome
    }

    /// Runs one claimed attempt, reporting a durably settled execution failure.
    ///
    /// `settled_failure` receives the execution error whenever this attempt
    /// settled it durably and therefore returns a healthy boolean.
    ///
    /// # Errors
    ///
    /// Returns every failure [`Self::execute_claim`] documents.
    async fn begin_claim_attempt(
        &self,
        claim: &ForgeTaskClaim,
        shutdown: &CancellationToken,
        settled_failure: &mut Option<ForgeError>,
    ) -> Result<AttemptStart, ForgeError> {
        let task = claim;
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
                // Terminalizing a malformed payload is a healthy worker outcome
                // that produced no effect.
                return Ok(AttemptStart::Settled(false));
            }
        };
        // An orphan-cleanup prefix is a delete authority, and a well-shaped one
        // belonging to a sibling table would still be a cross-table delete. The
        // dispatch's catalog-derived comparison catches metadata drift but only
        // after a lease and a catalog load; the binding already names this
        // task's own table, so the mismatch is refused here, before any lease,
        // catalog, listing, stat, or delete happens at all.
        if matches!(stage, ForgeExecutionStage::OrphanGc) {
            let prefix = claim
                .plan
                .orphan_cleanup_prefix(true)
                .map_err(ForgeError::Sql)?;
            let owned = super::orphan_gc::forge_data_prefix(&binding);
            if prefix != owned {
                return Err(ForgeError::Invariant {
                    detail: format!(
                        "orphan cleanup plan prefix {prefix} is not this task's table Forge root {owned}"
                    ),
                });
            }
        }
        let mut lease = self
            .acquire_table_lease(task.data_tenant_id, &binding)
            .await?;
        if lease.takeover() {
            tracing::info!(task_id = %task.task_id, "Forge worker took over an expired table fence");
        }
        let opened = self
            .open_rewrite_frame(task, attempt, &binding, &mut lease, shutdown)
            .await;
        let result = match opened {
            Ok(OpenedFrame::Planned(shared, plans, fenced)) => {
                return Ok(AttemptStart::Planned(Box::new(AdmittedRewrite {
                    attempt,
                    binding,
                    stage,
                    lease,
                    fenced,
                    shared,
                    plans,
                })));
            }
            Ok(OpenedFrame::Complete(settled)) => Ok(settled),
            Err(error) => Err(error),
        };
        self.settle_claim_execution(
            ClaimExecutionOutcome {
                task,
                attempt,
                stage,
                lease,
                result,
            },
            settled_failure,
        )
        .await
        .map(AttemptStart::Settled)
    }

    /// Settles failure and releases one task's table lease before outer observation.
    ///
    /// Returns whether the requested effect settled. An execution failure whose
    /// durable settlement and lease release both commit is a healthy outcome
    /// reported through `settled_failure` rather than through the return value.
    ///
    /// # Errors
    ///
    /// Returns the settlement failure when the durable row could not be made to
    /// describe this attempt, or the lease-release failure when the table fence
    /// could not be given up. Either leaves this owner unable to prove what it
    /// did, so it must stop claiming.
    async fn settle_claim_execution(
        &self,
        outcome: ClaimExecutionOutcome<'_>,
        settled_failure: &mut Option<ForgeError>,
    ) -> Result<bool, ForgeError> {
        let ClaimExecutionOutcome {
            task,
            attempt,
            stage,
            mut lease,
            result,
        } = outcome;
        if let Err(error) = &result {
            tracing::warn!(
                task_id = %task.task_id,
                attempt_id = %attempt,
                failure_class = error.failure_class().as_str(),
                error = %error,
                "Forge task execution failed"
            );
        }
        // An execution failure is only a healthy worker outcome once its
        // durable settlement commits. A settlement failure means the durable
        // row no longer describes what this owner did, so it is preserved and
        // propagated after the remaining bookkeeping rather than logged away.
        let mut fatal = None;
        if let Err(error) = &result
            && let Err(settlement) = self.settle_execution_failure(task, attempt, error).await
        {
            tracing::error!(task_id=%task.task_id, error=%settlement, "Forge failure settlement failed; claim retained for expiry recovery");
            self.close_after_fatal();
            fatal = Some(settlement);
        }
        tracing::debug!(
            task_id = %task.task_id,
            stage = ?stage,
            failed = result.is_err(),
            "Forge task execution stage returned"
        );
        if let Err(error) = self.release_table_lease(&mut lease).await {
            self.close_after_fatal();
            tracing::warn!(task_id = %task.task_id, error = %error, "Forge table lease release failed");
            // A retained table fence would let this owner's next claim run
            // against a table it can no longer prove it owns.
            fatal.get_or_insert(error);
        }
        if let Some(error) = fatal {
            return Err(error);
        }
        // The failure committed to durable state, so the caller reports it as
        // an observed attempt failure rather than as a reason to stop claiming.
        if let Err(error) = result {
            *settled_failure = Some(error);
            return Ok(false);
        }
        // The durable row is the authority for what happened; the boolean only
        // reports whether the requested effect settled successfully.
        Ok(result.unwrap_or(false))
    }

    /// Records one completed ownership episode from the durable task row.
    ///
    /// The worker never infers the result from the Rust return value:
    /// successful execution can durably cancel a superseded task, and an error
    /// can leave the task retryable. The durable `(state, failure_class)` pair
    /// is the authority, and a nonterminal or unread state emits nothing.
    async fn record_task_execution_telemetry(
        &self,
        tenant: DataTenantId,
        task_id: Uuid,
        strategy: Option<ForgeTaskStrategy>,
        span: &tracing::Span,
        started: Instant,
    ) {
        let Ok(Some(observation)) = self.durable_task_observation(tenant, task_id).await else {
            return;
        };
        let Some(result) = durable_task_result(observation) else {
            return;
        };
        span.record("result", result.as_str());
        let Some(task_type) = strategy else {
            return;
        };
        ForgeTelemetry::record_task_attempt(task_type, result, started.elapsed());
    }

    /// Reads the authoritative post-attempt state and durable failure class.
    ///
    /// # Errors
    ///
    /// Returns tenant-connection, SQL, or durable-state parsing errors. The
    /// caller treats an observation failure as diagnostic-only because the
    /// underlying task transition has already committed independently.
    async fn durable_task_observation(
        &self,
        tenant: DataTenantId,
        task_id: Uuid,
    ) -> Result<Option<(ForgeTaskState, Option<ForgeFailureClass>)>, ForgeError> {
        let mut conn = self
            .forge
            .core
            .vala
            .tenant_conn(tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let row: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT state, failure_class FROM vala.forge_tasks WHERE task_id = $1 AND data_tenant_id = $2",
        )
        .bind(task_id)
        .bind(tenant.as_uuid())
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(vala_sql::SqlError::from)
        .map_err(ForgeError::Sql)?;
        let Some((state, failure_class)) = row else {
            return Ok(None);
        };
        let state = ForgeTaskState::from_str(&state).map_err(ForgeError::Sql)?;
        let failure_class = failure_class
            .map(|class| ForgeFailureClass::from_sql(&class))
            .transpose()
            .map_err(ForgeError::Sql)?;
        Ok(Some((state, failure_class)))
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
    /// Validates one Prepared claim's tenancy, attempt, and ownership, and
    /// resolves the table binding reconciliation acts on.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the claim's execution tenant
    /// differs from the task's data tenant, the task carries no attempt
    /// generation, or the task is no longer Prepared under this owner.
    fn prepared_claim_context(
        &self,
        claim: &ForgePreparedTaskClaim,
    ) -> Result<(Uuid, TenantTableBinding), ForgeError> {
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
        Ok((attempt, binding))
    }

    /// Observes one Prepared ownership episode from claim through reconciliation.
    ///
    /// The ordinary active guard is shared with recovery; no synthetic ordinary
    /// claim is needed. The guard drops before the caller's post-attempt barrier.
    ///
    /// # Errors
    /// Returns the original reconciliation/release error; telemetry never replaces it.
    ///
    /// # Cancellation
    /// Retains exact Prepared evidence when recovery cannot finish under shutdown.
    async fn reconcile_prepared(
        &self,
        claim: ForgePreparedTaskClaim,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let started = Instant::now();
        let _active = self.forge.core.telemetry.active_task(claim.task.strategy);
        let span = tracing::info_span!("bifrost.forge.task.execute",
            strategy = claim.task.strategy.as_str(), result = tracing::field::Empty,
            role = "forge_worker", task_id = %claim.task.task_id, attempt_id = tracing::field::Empty);
        if let Some(attempt) = claim.task.attempt_id {
            span.record("attempt_id", tracing::field::display(attempt));
        }
        #[cfg(feature = "test-support")]
        if let Some(observer) = &self.completion_observer {
            observer.record_lifecycle(ForgeLifecycleEvent::Claimed {
                task_id: claim.task.task_id,
                worker_id: self.owner,
            });
            observer.pause_after_claim_for_test().await;
        }
        let result = tracing::Instrument::instrument(
            self.reconcile_prepared_attempt(&claim, shutdown),
            span.clone(),
        )
        .await;
        if result.is_err() {
            self.close_after_fatal();
        }
        #[cfg(feature = "test-support")]
        if result.is_err()
            && let Some(observer) = &self.completion_observer
        {
            observer.pause_fatal_observation_for_test().await;
        }
        self.record_task_execution_telemetry(
            claim.task.data_tenant_id,
            claim.task.task_id,
            Some(claim.task.strategy),
            &span,
            started,
        )
        .await;
        drop(span);
        self.record_attempt(result.as_ref().err());
        result
    }

    /// Takes this table's exclusive Forge lease, or reports the fence as lost.
    ///
    /// An attempt cannot recover or reconcile a table without the same
    /// table-level exclusivity its original owner held: another owner may
    /// already be mid-recovery on the very evidence this pass would act on. A
    /// refused acquisition is therefore reported as a lost fence rather than
    /// waited on, leaving the durable state for whoever does hold it.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::FenceLost`] when another owner holds the lease,
    /// and the SQL failures the acquisition raises.
    async fn acquire_table_lease(
        &self,
        tenant: DataTenantId,
        binding: &TenantTableBinding,
    ) -> Result<ForgeLease, ForgeError> {
        let lease_key = forge_lease_key(tenant, &binding.logical_namespace, &binding.table_name);
        ForgeLease::acquire(
            &self.forge.core.operator_pool,
            lease_key,
            self.owner,
            self.forge.core.config.lease_ttl,
        )
        .await?
        .ok_or_else(|| ForgeError::FenceLost {
            lease_key: format!("forge:table:{tenant}:{}", binding.table_ref),
        })
    }

    /// Validates and reconciles a borrowed Prepared claim while its caller owns telemetry.
    ///
    /// # Errors
    /// Returns identity, evidence, heartbeat, settlement, and release failures,
    /// preserving reconciliation failure ahead of a secondary release failure.
    ///
    /// # Cancellation
    /// Cancellation retains durable evidence for a later fenced owner.
    async fn reconcile_prepared_attempt(
        &self,
        claim: &ForgePreparedTaskClaim,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let (attempt, binding) = self.prepared_claim_context(claim)?;
        let task = &claim.task;
        let evidence = task
            .evidence
            .as_ref()
            .and_then(vala_sql::row_types::forge_tasks::ForgeTaskRowEvidence::publication)
            .ok_or_else(|| ForgeError::Reconciliation {
                detail: "Prepared Forge task has no committed evidence".to_owned(),
            })?;
        let mut lease = self
            .acquire_table_lease(task.data_tenant_id, &binding)
            .await?;
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
        let progressed = prepared_effect_progressed(evidence, task.base_snapshot_id);
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
        if reconciliation.is_err() {
            self.close_after_fatal();
        }
        operation_stop.cancel();
        let heartbeat_result = heartbeat
            .await
            .map_err(|error| ForgeError::Invariant {
                detail: format!("Forge reconciliation heartbeat panicked: {error}"),
            })
            .and_then(std::convert::identity);
        if heartbeat_result.is_err() {
            self.close_after_fatal();
        }
        let result = async {
            heartbeat_result?;
            if reconciliation? {
                return Ok(());
            }
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
                ForgeTaskTransition {
                    task_id: task.task_id,
                    attempt_id: attempt,
                    owner: self.owner,
                    expected: ForgeTaskState::Prepared,
                    next: ForgeTaskState::Succeeded,
                },
                task.data_tenant_id,
                &task.table_ref,
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
        // Matches the precedence `settle_claim_execution` already implements: a
        // retained table fence would let this owner's next claim run against a
        // table it can no longer prove it owns, so a release-only failure is
        // fatal too. When reconciliation already failed, that error stays
        // primary and the release failure is only a secondary diagnostic.
        if result.is_err() {
            self.close_after_fatal();
        }
        self.release_prepared_lease(task.task_id, &mut lease, result)
            .await
    }

    /// Releases Prepared ownership while preserving an earlier reconciliation error.
    /// The caller closes known failures before entering cleanup; a release-only
    /// failure closes the same run controls immediately after release returns.
    ///
    /// # Errors
    /// Returns reconciliation failure ahead of release failure when both occur.
    ///
    /// # Cancellation
    /// The caller retains its ownership guard until release and observation finish.
    async fn release_prepared_lease(
        &self,
        task_id: Uuid,
        lease: &mut ForgeLease,
        result: Result<(), ForgeError>,
    ) -> Result<(), ForgeError> {
        let release = self.release_table_lease(lease).await;
        if release.is_err() {
            self.close_after_fatal();
        }
        match (result, release) {
            (Err(reconciliation), Err(release)) => {
                tracing::warn!(task_id = %task_id, error = %release, "Forge reconciliation lease release also failed");
                Err(reconciliation)
            }
            (Err(reconciliation), Ok(_)) => Err(reconciliation),
            (Ok(()), Err(release)) => {
                tracing::error!(task_id = %task_id, error = %release, "Forge reconciliation committed but its table lease could not be released");
                Err(release)
            }
            (Ok(()), Ok(_)) => Ok(()),
        }
    }

    /// Verifies and resumes every external effect owned by one Prepared task.
    ///
    /// Returns whether the resumed effect already settled its own task. A
    /// prepared snapshot expiration owns that transition: takeover preserves
    /// `attempt_id`, replaces the current owner, acquires a new live table
    /// fence, and then settles or retains the operation its own claim rows
    /// name. Every other strategy leaves its terminal transition to the caller.
    ///
    /// # Errors
    ///
    /// Returns evidence, cleanup, orphan, fencing, catalog, or cancellation
    /// failures, and [`ForgeError::Reconciliation`] when a prepared expiration
    /// still cannot be settled.
    async fn resume_prepared_effect(
        &self,
        task: &ForgeTask,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        evidence: &ForgeTaskEvidence,
        stop: &CancellationToken,
    ) -> Result<bool, ForgeError> {
        lease.require_fence(&self.forge.core.operator_pool).await?;
        if task.strategy == ForgeTaskStrategy::SnapshotExpiry {
            let key = super::compact::ForgeTableKey {
                tenant: task.data_tenant_id,
                table_ref: binding.table_ref.clone(),
            };
            let settled = self
                .forge
                .run_snapshot_expiry_for_table(
                    lease,
                    &key,
                    binding,
                    &ExpiryTaskAuthority {
                        task: task.task_id,
                        attempt,
                        worker: self.owner,
                    },
                    self.forge.core.clock.now()?,
                    stop,
                )
                .await?
                .settled_evidence
                .is_some();
            if !settled {
                return Err(ForgeError::Reconciliation {
                    detail: "prepared snapshot expiration could not be settled or released"
                        .to_owned(),
                });
            }
            if stop.is_cancelled() {
                return Err(ForgeError::Shutdown);
            }
            return Ok(true);
        }
        let table = self.forge.load_table(&binding.table_ident()).await?;
        self.verify_committed_evidence(binding, &table, evidence)
            .await?;
        if task.strategy == ForgeTaskStrategy::ExpiredCleanup {
            self.resume_expired_cleanup(
                CleanupAttempt {
                    task_id: task.task_id,
                    tenant: task.data_tenant_id,
                    attempt,
                    binding,
                },
                lease,
                &task.plan,
                &table,
                evidence,
                stop,
            )
            .await?;
        }
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        Ok(false)
    }

    /// Validates the closed strategy and payload contract without IO.
    ///
    /// This is the phase-independent half of the worker's pre-effect gate: the
    /// exact input set, the closed strategy, and that strategy's own parameter
    /// contract. Each retained strategy keeps its contract here even while the
    /// activation boundary refuses it, because a strategy whose contract stops
    /// being checked is a strategy whose contract has quietly rotted by the
    /// time its own activation task arrives. It is separate from
    /// [`Self::validate_payload`] so the snapshot-expiry test entrypoint, which
    /// exists only because that one strategy is not phase-activated yet, still
    /// enforces every contract production enforces.
    ///
    /// # Errors
    ///
    /// Returns an invariant error for an unknown or reserved strategy,
    /// malformed parameters, or an empty exact input set, and the SQL refusal
    /// of a cleanup plan whose candidates do not all belong to the task table.
    fn validate_payload_contract(task: &ForgeTaskClaim) -> Result<ForgeExecutionStage, ForgeError> {
        // Expired cleanup is the one strategy whose exact work is its
        // parameters rather than its inputs: it deletes objects no snapshot
        // reaches, so an input file set would be meaningless and an empty one
        // is the contract.
        let cleanup = matches!(
            task.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::ExpiredCleanup)
        );
        if task.plan.inputs.is_empty() != cleanup {
            return Err(ForgeError::Invariant {
                detail: if cleanup {
                    "Forge expired cleanup carries inputs its payload contract forbids".to_owned()
                } else {
                    "Forge task payload carries no exact inputs".to_owned()
                },
            });
        }
        if cleanup {
            // The copied plan is authoritative for what this task deletes, so
            // the consumer re-binds it to the table the task row is filed under
            // before any lease, catalog, stat, or delete can happen.
            task.plan
                .expired_cleanup_payload(ForgeTaskStrategy::ExpiredCleanup, true)
                .and_then(|payload| payload.validate_for_table(&task.table_ref))
                .map_err(ForgeError::Sql)?;
            return Ok(ForgeExecutionStage::ExpiredCleanup);
        }
        if matches!(
            task.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::OrphanCleanup)
        ) {
            // Orphan cleanup's single input is a scan prefix, not a file set,
            // and its parameters carry the immutable cutoff. Both shapes are
            // re-validated here; the prefix is bound to this task's own table
            // in the dispatch, where the catalog location is available.
            task.plan
                .orphan_cleanup_payload(ForgeTaskStrategy::OrphanCleanup, true)
                .map_err(ForgeError::Sql)?;
            task.plan
                .orphan_cleanup_prefix(true)
                .map_err(ForgeError::Sql)?;
            return Ok(ForgeExecutionStage::OrphanGc);
        }
        let (expected_kind, stage) = match &task.strategy {
            ForgeClaimStrategy::Known(ForgeTaskStrategy::ScribePromotion) => (
                super::scribe_promotion::SCRIBE_PROMOTION_PARAMETER_KIND,
                ForgeExecutionStage::ScribePromotion,
            ),
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles) => (
                LIVE_REWRITE_PARAMETER_KIND,
                ForgeExecutionStage::IcebergRewrite,
            ),
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry) => {
                ("maintenance", ForgeExecutionStage::SnapshotExpiry)
            }
            ForgeClaimStrategy::Known(
                ForgeTaskStrategy::ExpiredCleanup | ForgeTaskStrategy::OrphanCleanup,
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
        let valid_parameters = match task.strategy {
            ForgeClaimStrategy::Known(ForgeTaskStrategy::ScribePromotion) => {
                super::scribe_promotion::ScribePromotionPlan::from_parameters(parameters).is_ok()
            }
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry) => {
                ForgeSnapshotExpiryIntent::parse(&task.strategy, parameters).is_some()
            }
            _ => {
                parameters.get("kind").and_then(Value::as_str) == Some(expected_kind)
                    && parameters.len() == 1
            }
        };
        if !valid_parameters {
            return Err(ForgeError::Invariant {
                detail: "Forge task parameters do not match the strategy contract".to_owned(),
            });
        }
        Ok(stage)
    }

    /// Validates the payload contract and then this phase's activation.
    ///
    /// This is the worker's pre-effect gate. It runs before the publication
    /// lease, the table load, and every dispatch arm, so a claim it refuses has
    /// touched no catalog, no object store, and no durable transition.
    ///
    /// The phase boundary is the last check rather than the first: a claim is
    /// refused for being malformed before it is refused for being early, so
    /// widening the phase later cannot turn a contract violation into a
    /// silently accepted task. A claim that reached durable state without
    /// passing the scheduler's admission gate therefore still cannot produce a
    /// catalog or object-store effect.
    ///
    /// # Errors
    ///
    /// Returns every [`Self::validate_payload_contract`] invariant, and an
    /// invariant error for a strategy this implementation phase has not
    /// activated.
    fn validate_payload(task: &ForgeTaskClaim) -> Result<ForgeExecutionStage, ForgeError> {
        let stage = Self::validate_payload_contract(task)?;
        if !matches!(
            task.strategy,
            ForgeClaimStrategy::Known(strategy) if super::phase::admits_new_effect(strategy)
        ) {
            return Err(ForgeError::Invariant {
                detail: format!(
                    "Forge task strategy {} is not activated in this phase",
                    task.strategy.as_str()
                ),
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
    /// [`ForgeError::Shutdown`]; the caller [`Self::run_event_loop`] owns the single
    /// release seam and drains the claim to `retryable` through
    /// [`Self::release_cancelled_claim`] for a clean shutdown. A cancellation
    /// observed after the durable effect — the fresh catalog commit or recovered
    /// committed snapshot below, whose task row is still `running` — instead
    /// returns [`ForgeError::ShutdownRetained`] so the event loop retains it for
    /// evidence-based recovery rather than releasing it.
    ///
    /// Returns whether the requested effect settled. A base snapshot that moved
    /// past this plan cancels the task as superseded and returns `false`: the
    /// row is settled, but no effect a reader can observe was produced.
    ///
    /// # Errors
    ///
    /// Returns catalog, stale-snapshot, lifecycle, heartbeat, rewrite, evidence,
    /// object-store, fence, cancellation, or audit failures.
    /// Dispatches one fresh claim and reduces its result to durable evidence.
    ///
    /// Splits the strategy-specific dispatch and evidence reduction out of the
    /// fenced attempt so the fence, heartbeat, and cancellation seams stay one
    /// readable sequence.
    ///
    /// # Errors
    ///
    /// Returns whatever the dispatched strategy returns, including shutdown,
    /// reconciliation, catalog, and SQL errors.
    async fn dispatch_evidence(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        table: Table,
        stop: &CancellationToken,
    ) -> Result<
        (
            ForgeTaskEvidence,
            ForgeExecutionEvidenceState,
            Option<ForgeCommittedVolume>,
        ),
        ForgeError,
    > {
        let result = self
            .dispatch_claim(ForgeDispatchRequest {
                claim,
                attempt,
                binding,
                lease,
                table,
                stop,
            })
            .await?;
        self.reduce_dispatch_result(claim, attempt, binding, lease, result, stop)
            .await
    }

    /// Reduces one dispatched result to the evidence its attempt settles with.
    ///
    /// Split out of [`Self::dispatch_evidence`] because a compaction attempt
    /// whose plans ran on the worker-wide pool produces its result outside that
    /// call and must still reduce it through exactly the same mapping.
    ///
    /// # Errors
    ///
    /// Returns the catalog, object-store, SQL, and audit failures evidence
    /// collection and snapshot-expiry completion raise.
    async fn reduce_dispatch_result(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        result: ForgeDispatchResult,
        stop: &CancellationToken,
    ) -> Result<
        (
            ForgeTaskEvidence,
            ForgeExecutionEvidenceState,
            Option<ForgeCommittedVolume>,
        ),
        ForgeError,
    > {
        match result {
            ForgeDispatchResult::Committed(publication) => self
                .committed_evidence(binding, &publication.table)
                .await
                .map(|evidence| {
                    (
                        evidence,
                        ForgeExecutionEvidenceState::Fresh,
                        publication.volume,
                    )
                }),
            ForgeDispatchResult::Cleaned(evidence) => {
                Ok((*evidence, ForgeExecutionEvidenceState::Prepared, None))
            }
            ForgeDispatchResult::AcceptanceUnknown(unknown) => Err(ForgeError::Invariant {
                detail: format!(
                    "Forge settled operation {} while its acceptance was unknown",
                    unknown.operation_id
                ),
            }),
            ForgeDispatchResult::SelfSettled => Ok(Self::self_settled_evidence()),
            ForgeDispatchResult::SnapshotExpiry(result) => self
                .complete_snapshot_expiry(claim, attempt, binding, lease, *result, stop)
                .await
                .map(|(evidence, state)| (evidence, state, None)),
        }
    }

    async fn open_rewrite_frame(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        shutdown: &CancellationToken,
    ) -> Result<OpenedFrame, ForgeError> {
        let (execution, fenced) = match self
            .prepare_fenced_attempt(claim, attempt, binding, lease, shutdown)
            .await?
        {
            FencedStart::Superseded => return Ok(OpenedFrame::Complete(false)),
            FencedStart::Recovered(evidence, fenced) => (
                Ok((
                    *evidence,
                    ForgeExecutionEvidenceState::RecoveredCommit,
                    None,
                )),
                fenced,
            ),
            // Compaction is the one strategy that suspends: its plans compete
            // for a budget that belongs to the worker, not to this attempt, so
            // admission and execution happen outside this frame.
            FencedStart::Open(_, fenced)
                if matches!(
                    claim.strategy,
                    ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles)
                ) =>
            {
                match self
                    .plan_rewrite_attempt(claim, attempt, binding, lease, fenced.dispatch_stop())
                    .await
                {
                    Ok(RewriteAdmission::Planned(shared, plans)) => {
                        return Ok(OpenedFrame::Planned(shared, plans, fenced));
                    }
                    Ok(RewriteAdmission::SelfSettled) => {
                        (Ok(Self::self_settled_evidence()), fenced)
                    }
                    Err(error) => (Err(error), fenced),
                }
            }
            FencedStart::Open(table, fenced) => {
                let execution = self
                    .dispatch_evidence(
                        claim,
                        attempt,
                        binding,
                        lease,
                        table,
                        fenced.dispatch_stop(),
                    )
                    .await;
                (execution, fenced)
            }
        };
        self.settle_fenced_attempt(claim, attempt, binding, lease, fenced, execution, shutdown)
            .await
            .map(OpenedFrame::Complete)
    }

    /// Returns the evidence an attempt that produced no effect settles with.
    ///
    /// Planning that selected nothing already wrote its own durable
    /// acknowledgement, so this evidence names no snapshot and no candidate.
    fn self_settled_evidence() -> (
        ForgeTaskEvidence,
        ForgeExecutionEvidenceState,
        Option<ForgeCommittedVolume>,
    ) {
        (
            ForgeTaskEvidence {
                version: FORGE_TASK_PAYLOAD_VERSION,
                committed_snapshot_id: None,
                committed_metadata_location: None,
                committed_metadata_digest: None,
                cleanup_candidates: Vec::new(),
                deleted_candidate_count: 0,
                prepared_candidate_index: None,
            },
            ForgeExecutionEvidenceState::Settled,
            None,
        )
    }

    /// Opens one attempt's fenced frame up to the point its strategy dispatches.
    ///
    /// Everything here is the part of an attempt that must happen before any
    /// effect: the fence check, the base-snapshot decision, the durable attempt
    /// begin, and the heartbeat that keeps ownership provable while the effect
    /// runs. It is separated from settlement so a compaction attempt can hold
    /// this frame open across the worker-wide plan queue instead of owning its
    /// own dispatch inline.
    ///
    /// # Errors
    ///
    /// Returns the fence, catalog, watermark, lifecycle, and heartbeat-spawn
    /// failures the opening sequence raises. A superseded base cancels the task
    /// durably and reports [`FencedStart::Superseded`] rather than an error.
    async fn prepare_fenced_attempt(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        shutdown: &CancellationToken,
    ) -> Result<FencedStart, ForgeError> {
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let table = self.forge.load_table(&binding.table_ident()).await?;
        let base_matches = Self::base_snapshot_matches(&table, claim.base_snapshot_id);
        let committed_recovery = if base_matches {
            None
        } else {
            Self::find_retained_task_evidence(
                &self.forge,
                binding,
                &table,
                "forge.task_id",
                claim.task_id,
            )
            .await?
        };
        let maintenance_recovery = matches!(
            claim.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry)
        );
        if !base_matches
            && committed_recovery.is_none()
            && !Self::tolerates_base_drift(&claim.strategy)
        {
            self.cancel_superseded(claim).await?;
            // The requested effect never ran, so this is a healthy exit that
            // recorded no successful completion.
            return Ok(FencedStart::Superseded);
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
        let fenced = FencedAttempt {
            operation_stop,
            authority_stop,
            heartbeat,
            maintenance_recovery,
        };
        Ok(match committed_recovery {
            Some(evidence) => FencedStart::Recovered(Box::new(evidence), fenced),
            None => FencedStart::Open(table, fenced),
        })
    }

    /// Settles one fenced attempt from the outcome its dispatch produced.
    ///
    /// Owns the whole post-effect sequence: the cancellation classification
    /// that decides whether a stopped attempt is released or retained, the
    /// heartbeat join, evidence settlement, and the durable finish. It is
    /// separated from [`Self::prepare_fenced_attempt`] so a compaction attempt
    /// whose plans ran on the worker-wide queue settles through exactly the
    /// same seam as an inline one.
    ///
    /// Returns whether the requested effect settled.
    ///
    /// # Errors
    ///
    /// Returns the dispatch failure, [`ForgeError::ShutdownRetained`] for a
    /// cancellation observed after a durable effect, the heartbeat's authority
    /// failure, and the evidence, audit, SQL, and fence failures settlement
    /// raises.
    ///
    /// # Panics
    ///
    /// Does not panic; a heartbeat panic is reported as
    /// [`ForgeError::Invariant`].
    #[expect(
        clippy::too_many_arguments,
        reason = "settlement needs the whole attempt frame: claim, generation, binding, fence, fenced tokens, dispatch outcome, and shutdown"
    )]
    async fn settle_fenced_attempt(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        fenced: FencedAttempt,
        execution: Result<
            (
                ForgeTaskEvidence,
                ForgeExecutionEvidenceState,
                Option<ForgeCommittedVolume>,
            ),
            ForgeError,
        >,
        shutdown: &CancellationToken,
    ) -> Result<bool, ForgeError> {
        let FencedAttempt {
            operation_stop,
            authority_stop: _,
            heartbeat,
            maintenance_recovery,
        } = fenced;
        // Test-only barrier: a durably settled snapshot expiration is held
        // here, after its atomic task and operation settlement committed and
        // before this worker reads shutdown or joins the heartbeat.
        #[cfg(feature = "test-support")]
        if maintenance_recovery
            && matches!(execution, Ok((_, ForgeExecutionEvidenceState::Settled, _)))
        {
            self.pause_after_settlement_for_test().await;
        }
        // A pre-effect cancellation surfaces as `execution == Err(Shutdown)`
        // (dispatch stopped mid-rewrite before its catalog commit, or a
        // maintenance task stopped before its `prepared()` boundary) and
        // propagates unchanged as `ForgeError::Shutdown`, which the single
        // the event loop release seam drains through `release_cancelled_claim`. The
        // post-effect cancellation at the check below instead has a durable
        // effect already committed while its task row is still `running`, so it
        // propagates as `ForgeError::ShutdownRetained` to keep the event loop from
        // matching and releasing it; it stays retained for evidence-based and
        // lease-expiry recovery.
        //
        // Durable settlement is the exception to both. `Settled` means the
        // atomic task-and-operation transition has already committed, so a
        // shutdown that raced it and a now-obsolete heartbeat conflict can
        // neither undo it nor be retried into a different outcome: reporting
        // either as this attempt's result would contradict durable state. The
        // heartbeat is still cancelled and joined so no task is detached.
        let settled_dispatch =
            matches!(execution, Ok((_, ForgeExecutionEvidenceState::Settled, _)));
        let completion = match execution {
            Ok(evidence) if settled_dispatch || !operation_stop.is_cancelled() => Ok(evidence),
            Ok(_) => Err(ForgeError::ShutdownRetained),
            Err(error) => Err(error),
        };
        operation_stop.cancel();
        let heartbeat_result = heartbeat.await.map_err(|error| ForgeError::Invariant {
            detail: format!("Forge claim heartbeat panicked: {error}"),
        })?;
        if !settled_dispatch {
            heartbeat_result?;
        }
        let (evidence, state, volume) = completion?;
        self.settle_promotion_evidence(claim, binding, lease, &evidence, &state)
            .await?;
        self.settle_rewrite_recovery(claim, binding, lease, &state, shutdown)
            .await?;
        self.record_rewrite_evidence(claim, &evidence);
        let settled = matches!(state, ForgeExecutionEvidenceState::Settled);
        self.finish_claim_execution(claim, attempt, lease, &evidence, state)
            .await?;
        if !settled {
            Self::record_committed_volume(claim, volume);
        }
        Ok(true)
    }

    /// Settles a rewrite whose commit was discovered rather than observed.
    ///
    /// A rewrite that lost its answer and then proved it landed never reaches
    /// its own dispatcher again, so the Prepared operation it left open would
    /// stay open forever. Routing that one case back through live
    /// reconciliation settles it from the retained snapshot — the same evidence
    /// and the same writer a takeover would have used — instead of adding a
    /// second terminal audit writer that could disagree with it.
    ///
    /// Every other strategy and every other evidence state is a no-op: their
    /// terminal transition was already written by the owner that produced it.
    ///
    /// # Errors
    ///
    /// Returns the fence, SQL, catalog, and object-store failures live
    /// reconciliation raises, and [`ForgeError::Reconciliation`] when the
    /// operation cannot be settled from retained evidence.
    async fn settle_rewrite_recovery(
        &self,
        claim: &ForgeTaskClaim,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        state: &ForgeExecutionEvidenceState,
        stop: &CancellationToken,
    ) -> Result<(), ForgeError> {
        if !matches!(
            claim.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles)
        ) || !matches!(state, ForgeExecutionEvidenceState::RecoveredCommit)
        {
            return Ok(());
        }
        let outcome = self
            .forge
            .reconcile_live_replacements(
                lease,
                &super::compact::ForgeTableKey {
                    tenant: claim.data_tenant_id,
                    table_ref: binding.table_ref.clone(),
                },
                binding,
                stop,
                self.forge.core.clock.now()?,
            )
            .await?;
        if outcome.pending > 0 || outcome.unresolved > 0 {
            return Err(ForgeError::Reconciliation {
                detail: format!(
                    "a recovered rewrite left {} pending and {} unresolved live operations",
                    outcome.pending, outcome.unresolved
                ),
            });
        }
        Ok(())
    }

    /// Settles one promotion's publication and terminal audit for a finished attempt.
    ///
    /// Promotion's SQL publication and terminal audit settle together, under
    /// the same fence, for every path that produced evidence: a fresh append, a
    /// recovered commit discovered on a retained snapshot, and a restarted
    /// attempt that finds its own settlement already durable. Non-promotion
    /// strategies own no publication columns, so the call is a no-op for them.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the persisted promotion
    /// parameters no longer decode, and the fence and SQL failures raised by
    /// the settlement transaction.
    async fn settle_promotion_evidence(
        &self,
        claim: &ForgeTaskClaim,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        evidence: &ForgeTaskEvidence,
        state: &ForgeExecutionEvidenceState,
    ) -> Result<(), ForgeError> {
        if !matches!(
            claim.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::ScribePromotion)
        ) {
            return Ok(());
        }
        let plan = Self::promotion_plan(claim)?;
        self.forge
            .settle_promotion(
                lease,
                binding,
                ForgePromotionSettlement {
                    plan: &plan,
                    phase: match state {
                        ForgeExecutionEvidenceState::RecoveredCommit => {
                            ForgeScribePromotionPhase::Recovered
                        }
                        ForgeExecutionEvidenceState::Fresh
                        | ForgeExecutionEvidenceState::Prepared
                        | ForgeExecutionEvidenceState::Settled => {
                            ForgeScribePromotionPhase::Committed
                        }
                    },
                    operation_id: Self::promotion_operation_id(claim),
                    base_snapshot_id: claim.base_snapshot_id,
                    committed_snapshot_id: evidence.committed_snapshot_id,
                },
            )
            .await
    }

    /// Publishes the rewrite and catalog-commit observer events for a completed
    /// execution.
    ///
    /// This runs only after the durable completion path produced evidence, so
    /// test observation cannot acknowledge or alter a task before its effect is
    /// committed. The catalog-committed event is emitted only when the evidence
    /// carries a committed snapshot.
    #[cfg(not(feature = "test-support"))]
    fn record_rewrite_evidence(&self, _claim: &ForgeTaskClaim, _evidence: &ForgeTaskEvidence) {}

    /// Publishes the rewrite and catalog-commit observer events.
    #[cfg(feature = "test-support")]
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

    /// Reports the logical volume one Scribe promotion published.
    ///
    /// The claimed task's persisted estimates are the exact promotion demand:
    /// the promotion moves those already-written objects into the table
    /// unchanged. They are reported as both logical input and published output
    /// because the same objects entered and left the effect; nothing here
    /// claims Forge rewrote the bytes, and no storage metadata is reread.
    fn promotion_volume(claim: &ForgeTaskClaim) -> ForgeCommittedVolume {
        ForgeCommittedVolume {
            input_files: u64::from(claim.estimates.files),
            input_bytes: claim.estimates.bytes,
            output_files: u64::from(claim.estimates.files),
            output_bytes: claim.estimates.bytes,
        }
    }

    /// Counts committed file and byte throughput after durable settlement.
    ///
    /// Called only once the settlement transition for this attempt commits, and
    /// never for a state another owner already settled. A strategy this build
    /// does not know carries no `task_type`.
    fn record_committed_volume(claim: &ForgeTaskClaim, volume: Option<ForgeCommittedVolume>) {
        let (Some(volume), Some(task_type)) = (volume, Self::metric_strategy(&claim.strategy))
        else {
            return;
        };
        ForgeTelemetry::record_input(task_type, volume.input_files, volume.input_bytes);
        ForgeTelemetry::record_output(task_type, volume.output_files, volume.output_bytes);
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
        // Atomic expiration settlement already wrote the terminal task
        // transition, cleared ownership, removed the claims, advanced planning
        // demand, and appended both terminal audits in one transaction. The row
        // no longer names this attempt, so heartbeating it would report a
        // conflict after durable success. The worker performs no second
        // transition and no further authority refresh at all.
        if matches!(state, ForgeExecutionEvidenceState::Settled) {
            return Ok(());
        }
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
        match state {
            ForgeExecutionEvidenceState::Fresh | ForgeExecutionEvidenceState::RecoveredCommit => {
                self.persist_success(claim, attempt, lease, evidence).await
            }
            ForgeExecutionEvidenceState::Settled => Ok(()),
            ForgeExecutionEvidenceState::Prepared => {
                self.persist_terminal_success(
                    ForgeTaskTransition {
                        task_id: claim.task_id,
                        attempt_id: attempt,
                        owner: self.owner,
                        expected: ForgeTaskState::Prepared,
                        next: ForgeTaskState::Succeeded,
                    },
                    claim.data_tenant_id,
                    &claim.table_ref,
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

    /// Reports whether a strategy stays valid when the table has moved past
    /// its planning base.
    ///
    /// Maintenance and expired cleanup are defined against the table's own
    /// history rather than against one exact input set, so an intervening
    /// writer does not supersede them: refusing them on base drift would make
    /// them unrunnable on any table that is still being written to.
    fn tolerates_base_drift(strategy: &ForgeClaimStrategy) -> bool {
        matches!(
            strategy,
            ForgeClaimStrategy::Known(
                ForgeTaskStrategy::SnapshotExpiry
                    | ForgeTaskStrategy::ExpiredCleanup
                    | ForgeTaskStrategy::OrphanCleanup
            )
        )
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
        if Self::tolerates_base_drift(&claim.strategy)
            && let Some(current) = table.metadata().current_snapshot()
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

    /// Runs one claimed snapshot-expiry task through its retained owner.
    ///
    /// The plan's own validated parameters decide whether retention is due, so
    /// a claim cannot widen its scope at execution time. Live replacements are
    /// reconciled first: expiry that ran against unreconciled replacements
    /// could retire a snapshot still referenced by an in-flight rewrite. The
    /// reconciliation clock reading is taken inside this call so the
    /// reconciliation window is measured from execution, not from claim. The
    /// pass performs no deletion of its own: the exact candidates a successful
    /// expiration commits are the durable handoff an independently claimed
    /// expired-cleanup task consumes.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the validated plan parameters are
    /// not an object or the expiry intent cannot be decoded,
    /// [`ForgeError::FenceLost`] when the table lease no longer covers the
    /// commit window, [`ForgeError::Shutdown`] when authority is lost before an
    /// effect, and propagates reconciliation, catalog, SQL, audit, and clock
    /// errors from the retained expiration owner.
    ///
    /// # Cancellation
    ///
    /// Cancellation stops before the next stage. Cancellation racing the expiry
    /// commit leaves the Prepared operation open for evidence-based recovery.
    async fn dispatch_snapshot_expiry(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        stop: &CancellationToken,
    ) -> Result<ForgeDispatchResult, ForgeError> {
        let intent = ForgeSnapshotExpiryIntent::parse(
            &claim.strategy,
            claim
                .plan
                .parameters
                .as_object()
                .ok_or_else(|| ForgeError::Invariant {
                    detail: "validated Forge snapshot-expiry parameters lost their object shape"
                        .to_owned(),
                })?,
        )
        .ok_or_else(|| ForgeError::Invariant {
            detail: "validated Forge snapshot-expiry intent could not be decoded".to_owned(),
        })?;
        let key = super::compact::ForgeTableKey {
            tenant: claim.data_tenant_id,
            table_ref: binding.table_ref.clone(),
        };
        self.forge
            .reconcile_live_replacements(lease, &key, binding, stop, self.forge.core.clock.now()?)
            .await?;
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        lease.require_fence(&self.forge.core.operator_pool).await?;
        if !lease.commit_window_fits(self.forge.core.config.commit_window()) {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        #[cfg(feature = "test-support")]
        if self
            .forge
            .core
            .expiry_controls
            .pause_expiry_submission(stop)
            .await
        {
            return Err(ForgeError::Reconciliation {
                detail:
                    "Iceberg expiry lifecycle was cancelled before its first catalog submission"
                        .to_owned(),
            });
        }
        let expiry_evidence =
            if intent.snapshot_expiry_due && self.forge.core.config.snapshot_expiry_enabled {
                self.forge
                    .run_snapshot_expiry_for_table(
                        lease,
                        &key,
                        binding,
                        &ExpiryTaskAuthority {
                            task: claim.task_id,
                            attempt,
                            worker: self.owner,
                        },
                        self.forge.core.clock.now()?,
                        stop,
                    )
                    .await?
                    .settled_evidence
            } else {
                None
            };
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let table = self.forge.load_table(&binding.table_ident()).await?;
        Ok(ForgeDispatchResult::SnapshotExpiry(Box::new(
            ForgeSnapshotExpiryResult {
                table,
                expiry_evidence,
            },
        )))
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
        } = request;
        if Self::current_snapshot_matches_task(&table, claim.task_id) {
            return Ok(ForgeDispatchResult::Committed(Box::new(
                ForgeCommittedPublication {
                    table,
                    volume: None,
                },
            )));
        }
        if !Self::base_snapshot_matches(&table, claim.base_snapshot_id)
            && !Self::tolerates_base_drift(&claim.strategy)
        {
            return Err(ForgeError::Reconciliation {
                detail: "Forge task base snapshot changed before execution".to_owned(),
            });
        }
        match &claim.strategy {
            ForgeClaimStrategy::Known(ForgeTaskStrategy::ScribePromotion) => {
                self.dispatch_scribe_promotion(claim, attempt, binding, lease, &table, stop)
                    .await
            }
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry) => {
                self.dispatch_snapshot_expiry(claim, attempt, binding, lease, stop)
                    .await
            }
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles) => {
                Err(ForgeError::Invariant {
                    detail: "Forge compaction dispatches through the worker plan pool".to_owned(),
                })
            }
            ForgeClaimStrategy::Known(ForgeTaskStrategy::ExpiredCleanup) => {
                self.dispatch_expired_cleanup(claim, attempt, binding, lease, &table, stop)
                    .await
            }
            ForgeClaimStrategy::Known(ForgeTaskStrategy::OrphanCleanup) => {
                self.dispatch_orphan_cleanup(claim, attempt, binding, lease, &table, stop)
                    .await
            }
            _ => Err(ForgeError::Invariant {
                detail: "unsupported task passed pre-effect validation".to_owned(),
            }),
        }
    }

    /// Executes, prepares, and publishes one managed live rewrite.
    ///
    /// The ordering is the whole safety argument. The managed core runs first
    /// and publishes nothing, so the objects exist in storage while belonging
    /// to no snapshot and no reader can observe them. Only then is the exact
    /// replacement derived — from the immutable base this attempt read, never
    /// from the core's own applied-delete evidence — and only then is the
    /// Prepared audit written, before any catalog effect, naming the exact
    /// inputs and outputs a successor must reconcile.
    ///
    /// A catalog refusal that is not retryable is a *definite* conflict: the
    /// catalog answered, so the replacement certainly did not land. Exactly one
    /// retry is permitted against it, and only after reloading the table and
    /// re-deriving the base and the request against it — replaying the same
    /// replacement against a stale base is what would delete files a concurrent
    /// writer has already replaced. The deadline is captured once, before the
    /// first attempt, so a slow first attempt cannot buy the retry more time
    /// than the original commit budget allowed. Every other failure, including
    /// an uncertain one, is returned unchanged so the Prepared operation stays
    /// open for evidence-based recovery.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Shutdown`] when the managed attempt drains before
    /// producing a publishable handoff, [`ForgeError::Reconciliation`] when the
    /// core made no progress, when the touched files do not share one time
    /// partition, or when acceptance becomes unknown,
    /// [`ForgeError::InvalidConfig`] when the configured Iceberg retry budget
    /// is not representable as a deadline, and the capacity, audit, fence,
    /// object-store, and catalog failures raised by execution, preparation, and
    /// the commit.
    async fn plan_rewrite_attempt(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        stop: &CancellationToken,
    ) -> Result<RewriteAdmission, ForgeError> {
        self.rewrite_settlement_barrier(claim, binding, lease, stop)
            .await?;
        let rewrite = self
            .forge
            .managed_rewrite(binding, claim.task_id, attempt, stop)?;
        let super::managed::ForgePlannedAttempt {
            table,
            evidence,
            plans,
        } = rewrite.plan().await?;
        if plans.is_empty() {
            self.acknowledge_compact_table(claim, attempt, &table, lease)
                .await?;
            return Ok(RewriteAdmission::SelfSettled);
        }
        Ok(RewriteAdmission::Planned(
            Arc::new(ForgeRewriteAttempt {
                rewrite: Arc::new(rewrite),
                table,
                evidence,
                deadline: super::publication::RewritePublicationDeadline::new(
                    self.forge.core.clock.now()?,
                    self.forge.core.config.iceberg_total_retry_timeout,
                )?,
            }),
            plans,
        ))
    }

    /// Starts every queued head the worker's running budgets currently admit.
    ///
    /// Only the FIFO head is ever considered, so a plan that does not fit blocks
    /// the ones behind it rather than being skipped: that head-of-line behavior
    /// is what makes the queue's start order the planner's order across every
    /// attempt this worker holds, not just within one of them.
    ///
    /// Popping moves the plan's concrete runner out of the queue and into the
    /// future scheduled on the compaction executor, so exactly one owner holds
    /// it at every moment and the future captures nothing else of this worker.
    ///
    /// A drained worker starts nothing and instead drops the plans it has not
    /// started. That is not a lost outcome: an unstarted plan wrote nothing,
    /// holds no operation, and is ordinary planning debt the next attempt
    /// replans from the same table. The plans that did run are kept, because
    /// their publications are durable and their failures carry objects only
    /// their attempt can still name.
    ///
    /// Returns the attempts a drain or a missing runner left with nothing to
    /// join, so the caller settles them instead of waiting for a plan that will
    /// never start.
    fn start_fitting_plans(
        &self,
        pool: &mut ForgeAttemptPool,
        shutdown: &CancellationToken,
    ) -> Vec<ForgeAttemptState> {
        if shutdown.is_cancelled() {
            return Self::drop_waiting_plans(pool);
        }
        let completion_tx = pool.completion_tx.clone();
        let mut without_runner = Vec::new();
        while let Some(popped) = pool.queue.pop() {
            let key = (popped.admission.task_id, popped.admission.plan_index);
            let Some(state) = pool.attempts.get_mut(&key.0) else {
                pool.queue.finish_running(key);
                continue;
            };
            state.queued = state.queued.saturating_sub(1);
            let Some(runner) = popped.runner else {
                // Upstream finishes the exact reservation and schedules
                // nothing. The attempt still owes an outcome for this index, so
                // the invariant violation is recorded against it directly.
                state.outcomes.push((
                    key.1,
                    Err(ForgeError::Invariant {
                        detail: format!("Forge admitted plan {} carried no runner", key.1),
                    }),
                ));
                without_runner.push(key.0);
                pool.queue.finish_running(key);
                continue;
            };
            state.running += 1;
            self.spawn_plan_runner(runner, key, completion_tx.clone());
        }
        let mut stranded = Vec::new();
        for task_id in without_runner {
            if pool
                .attempts
                .get(&task_id)
                .is_some_and(ForgeAttemptState::drained)
                && let Some(state) = pool.attempts.remove(&task_id)
            {
                stranded.push(state);
            }
        }
        stranded
    }

    /// Drops every unstarted plan this worker holds for a graceful drain.
    ///
    /// Returns the attempts that are left with nothing in flight and are not
    /// retained; a retained attempt stays in the pool for the shutdown handoff.
    fn drop_waiting_plans(pool: &mut ForgeAttemptPool) -> Vec<ForgeAttemptState> {
        let suspended: Vec<Uuid> = pool.attempts.keys().copied().collect();
        let mut stranded = Vec::new();
        for task_id in suspended {
            let dropped = pool.queue.cancel_waiting_task(task_id);
            let Some(state) = pool.attempts.get_mut(&task_id) else {
                continue;
            };
            if dropped > 0 {
                state.queued = state.queued.saturating_sub(dropped);
                tracing::debug!(
                    task_id = %task_id,
                    dropped,
                    running_parallelism = pool.queue.running_parallelism_sum(),
                    running_memory_reservation_bytes = pool.queue.running_memory_reservation_bytes(),
                    "Forge dropped the plans a drained attempt had not started"
                );
            }
            if state.drained()
                && !state.retained()
                && let Some(state) = pool.attempts.remove(&task_id)
            {
                stranded.push(state);
            }
        }
        stranded
    }

    /// Records one keyed plan completion and reports the attempt it drained.
    ///
    /// The exact queue reservation is released before the outcome is stored, so
    /// a completion that names a key this worker does not hold is refused
    /// rather than double-crediting a budget. An unknown acceptance retracts
    /// readiness in this same transition, before any classifier reads it.
    ///
    /// Returns `None` when the finished plan has siblings this worker is still
    /// running for the same attempt.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the completion's key was already
    /// released, or when it names an attempt this worker is not holding: either
    /// way the owner can no longer prove what it published.
    fn record_plan_completion(
        &self,
        pool: &mut ForgeAttemptPool,
        completion: ForgePlanCompletion,
    ) -> Result<Option<ForgeAttemptState>, ForgeError> {
        let ForgePlanCompletion { key, outcome } = completion;
        if !pool.queue.finish_running(key) {
            return Err(ForgeError::Invariant {
                detail: format!(
                    "Forge released plan {} of task {} more than once",
                    key.1, key.0
                ),
            });
        }
        if matches!(outcome, Ok(ForgeDispatchResult::AcceptanceUnknown(_))) {
            self.publish_readiness(false);
        }
        let Some(state) = pool.attempts.get_mut(&key.0) else {
            return Err(ForgeError::Invariant {
                detail: format!("Forge joined plan {} of an attempt it does not hold", key.1),
            });
        };
        state.running = state.running.saturating_sub(1);
        state.outcomes.push((key.1, outcome));
        if state.drained() {
            return Ok(pool.attempts.remove(&key.0));
        }
        Ok(None)
    }

    /// Settles one attempt whose every admitted plan has drained.
    ///
    /// Reduces the per-plan results to one task result and then walks exactly
    /// the seams an inline attempt walks — evidence reduction, fenced
    /// settlement, claim settlement, and one passive observation — so a pooled
    /// attempt is indistinguishable from an inline one in durable state.
    ///
    /// The attempt-shared managed context is snapshotted exactly once here,
    /// after the last plan completion arrived and before anything is reduced,
    /// and is dropped only after settlement, the heartbeat join, the lease
    /// release, and the episode closure. A running sibling can still add to
    /// that ledger, so an earlier snapshot would name fewer objects than the
    /// attempt actually wrote and a later drop would race the ledger's readers.
    ///
    /// Returns whether the requested effect settled.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Shutdown`] when a drain stopped every plan before
    /// any ran, and otherwise the settlement, audit, SQL, and lease-release
    /// failures that make a further claim by this owner unsafe.
    async fn finish_admitted_attempt(
        &self,
        state: ForgeAttemptState,
        shutdown: &CancellationToken,
    ) -> Result<bool, ForgeError> {
        let ForgeAttemptState {
            open,
            attempt,
            binding,
            stage,
            mut lease,
            fenced,
            shared,
            refusals,
            outcomes,
            ..
        } = state;
        let possible_outputs = shared.rewrite.possible_outputs();
        let execution = if outcomes.is_empty() {
            Err(Self::attach_attempt_outputs(
                ForgeError::Shutdown,
                possible_outputs,
            ))
        } else {
            match Self::reduce_plan_outcomes(refusals, outcomes, possible_outputs) {
                Ok(result) => {
                    self.reduce_dispatch_result(
                        &open.claim,
                        attempt,
                        &binding,
                        &mut lease,
                        result,
                        fenced.dispatch_stop(),
                    )
                    .await
                }
                Err(error) => Err(error),
            }
        };
        let result = self
            .settle_fenced_attempt(
                &open.claim,
                attempt,
                &binding,
                &mut lease,
                fenced,
                execution,
                shutdown,
            )
            .await;
        let mut settled_failure = None;
        let settled = self
            .settle_claim_execution(
                ClaimExecutionOutcome {
                    task: &open.claim,
                    attempt,
                    stage,
                    lease,
                    result,
                },
                &mut settled_failure,
            )
            .await;
        let closed = self
            .close_claim_episode(open, settled_failure, settled)
            .await;
        drop(shared);
        closed
    }

    /// Schedules one popped runner and returns its keyed completion to the loop.
    ///
    /// The future owns the runner and one sender clone and nothing else: it
    /// never captures this worker, so a plan cannot reach claims, readiness,
    /// recovery, or settlement while it executes.
    fn spawn_plan_runner(
        &self,
        runner: ForgeCompactionPlanRunner,
        key: super::managed::queue::ForgePlanKey,
        completion_tx: tokio::sync::mpsc::UnboundedSender<ForgePlanCompletion>,
    ) {
        let future = async move {
            let outcome = runner.compact().await;
            if completion_tx
                .send(ForgePlanCompletion { key, outcome })
                .is_err()
            {
                tracing::warn!(
                    task_id = %key.0,
                    plan_index = key.1,
                    "Forge plan completion could not be delivered to its worker"
                );
            }
        };
        match &self.compaction_runtime {
            Some(handle) => {
                handle.spawn(future);
            }
            None => {
                tokio::spawn(future);
            }
        }
    }

    /// Offers one attempt's planner-ordered plans to the worker-wide queue.
    ///
    /// The pass is ordered because the queue is a FIFO: index `n` must occupy a
    /// position ahead of index `n + 1`, so offering out of order would let a
    /// later plan start before an earlier one. A pending-capacity refusal ends
    /// the pass rather than skipping to the next plan, because the budget the
    /// refused plan did not fit is worker-wide: continuing would let a smaller
    /// later sibling take the waiting slot the blocked earlier head needs, and
    /// that is exactly the head-of-line bypass the queue exists to prevent. The
    /// unoffered remainder is ordinary planning debt the next attempt replans.
    ///
    /// Every other refusal is recorded and the pass continues, because those
    /// describe the individual plan (an invalid estimate, a duplicate key)
    /// rather than the worker's remaining room.
    ///
    /// Returns each refused plan's index and reason, in offer order.
    fn offer_planned_rewrites(
        queue: &mut super::managed::queue::ForgeCompactionQueue,
        plans: impl IntoIterator<
            Item = (
                super::managed::queue::ForgePlanAdmission,
                Option<ForgeCompactionPlanRunner>,
            ),
        >,
    ) -> Vec<(usize, super::managed::queue::ForgePushResult)> {
        let mut refusals = Vec::new();
        for (admission, runner) in plans {
            let plan_index = admission.plan_index;
            match queue.push(admission, runner) {
                super::managed::queue::ForgePushResult::Added => {}
                refusal @ super::managed::queue::ForgePushResult::RejectedCapacity => {
                    refusals.push((plan_index, refusal));
                    break;
                }
                refusal => {
                    refusals.push((plan_index, refusal));
                }
            }
        }
        refusals
    }

    /// Settles an attempt whose planning selected nothing as a success.
    ///
    /// Planning that selects nothing means the table is already compact, not
    /// that this attempt failed. Reporting a failure would consume the task's
    /// attempt budget and terminalize an idle table after five passes, so the
    /// no-op is acknowledged instead: the task succeeds and its planning demand
    /// records the exact snapshot and commit backlog the acknowledgement was
    /// made against, which is what keeps the next demand honest.
    ///
    /// # Errors
    ///
    /// Returns the audit, fence, and SQL failures the terminal transition
    /// raises; the caller has already refused to publish anything.
    async fn acknowledge_compact_table(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        table: &Table,
        lease: &ForgeLease,
    ) -> Result<(), ForgeError> {
        let metadata = table.metadata();
        self.persist_terminal_success(
            ForgeTaskTransition {
                task_id: claim.task_id,
                attempt_id: attempt,
                owner: self.owner,
                expected: ForgeTaskState::Running,
                next: ForgeTaskState::Succeeded,
            },
            claim.data_tenant_id,
            &claim.table_ref,
            lease,
            TaskProgressEffect::NoOpAcknowledged {
                snapshot_id: metadata.current_snapshot_id().unwrap_or(0),
                commit_count: u64::try_from(
                    metadata
                        .snapshots()
                        .count()
                        .saturating_sub(self.forge.core.config.retain_last),
                )
                .unwrap_or(u64::MAX),
            },
        )
        .await
    }

    /// Reduces one attempt's per-plan refusals and outcomes to one task result.
    ///
redacted
    /// fall-through: any single published plan makes the task successful no
    /// matter how its siblings ended, because the commit is durable and the
    /// unfinished siblings remain ordinary planning debt for the next attempt.
    /// Only when nothing published does a failure decide the task, and then it
    /// is the *lowest plan index*'s failure — never the first to arrive, and
    /// never a severity ranking — so the same plan set always settles the same
    /// way regardless of completion order.
    ///
    /// A refusal decides only when no plan was admitted at all. The lowest
    /// refused index supplies it, which is what keeps a capacity refusal
    /// non-consuming while an invalid parallelism or duplicate key at a lower
    /// index is still reported as the invariant violation it is.
    ///
    /// # Errors
    ///
    /// Returns the deciding plan's exact failure unchanged,
    /// [`ForgeError::Capacity`] when the whole attempt was refused for
    /// capacity, and [`ForgeError::Invariant`] for a queue refusal that is an
    /// invariant violation or for an attempt that produced neither.
    fn reduce_plan_outcomes(
        mut refusals: Vec<(usize, super::managed::queue::ForgePushResult)>,
        outcomes: Vec<(usize, Result<ForgeDispatchResult, ForgeError>)>,
        possible_outputs: Vec<super::managed::ForgeUnsettledOutput>,
    ) -> Result<ForgeDispatchResult, ForgeError> {
        let mut published = None;
        let mut committed_volume: Option<ForgeCommittedVolume> = None;
        let mut failures: Vec<(usize, ForgeError)> = Vec::new();
        for (plan_index, outcome) in outcomes {
            match outcome {
                // Reached only for a sibling of a known success, which decides
                // the task on its own; the unresolved row is left to the
                // table-wide reconciliation owner.
                Ok(ForgeDispatchResult::AcceptanceUnknown(unknown)) => {
                    failures.push((plan_index, unknown.error));
                }
                Ok(result) => {
                    // Every plan of one attempt commits its own operation, so
                    // the task's throughput is the sum of them all. Retaining
                    // only the surviving publication's own measurement would
                    // report one plan's volume for the whole attempt.
                    if let ForgeDispatchResult::Committed(publication) = &result
                        && let Some(volume) = publication.volume
                    {
                        committed_volume = Some(
                            committed_volume
                                .map_or(volume, |running| running.saturating_add(volume)),
                        );
                    }
                    published = Some(result);
                }
                Err(error) => failures.push((plan_index, error)),
            }
        }
        if let Some(mut result) = published {
            if let ForgeDispatchResult::Committed(publication) = &mut result {
                publication.volume = committed_volume;
            }
            return Ok(result);
        }
        failures.sort_by_key(|(plan_index, _)| *plan_index);
        if !failures.is_empty() {
            return Err(Self::unsettled_across_plans(failures, possible_outputs));
        }
        refusals.sort_by_key(|(plan_index, _)| *plan_index);
        match refusals.into_iter().next() {
            Some((
                plan_index,
                super::managed::queue::ForgePushResult::RejectedCapacity
                | super::managed::queue::ForgePushResult::RejectedTooLarge,
            )) => Err(ForgeError::Capacity {
                detail: format!(
                    "Forge admitted no plan of this attempt; plan {plan_index} did not fit"
                ),
            }),
            Some((plan_index, refusal)) => Err(ForgeError::Invariant {
                detail: format!("Forge admission refused plan {plan_index}: {refusal:?}"),
            }),
            None => Err(ForgeError::Invariant {
                detail: "Forge planning produced plans that were neither admitted nor refused"
                    .to_owned(),
            }),
        }
    }

    /// Reports one attempt's failure while keeping every plan's loose objects.
    ///
    /// The deciding failure is the lowest plan index's, which is what makes an
    /// attempt settle the same way regardless of completion order. Its own
    /// possible outputs are not enough, though: a sibling plan that also failed
    /// wrote objects that only this attempt can still name, and dropping them
    /// with its error would leave objects no reclaiming caller ever hears
    /// about. So the returned error carries `possible_outputs` — the one
    /// attempt-global snapshot the caller took after every plan drained.
    ///
    /// # Panics
    ///
    /// Panics when `failures` is empty, which its one caller has already
    /// excluded.
    fn unsettled_across_plans(
        failures: Vec<(usize, ForgeError)>,
        possible_outputs: Vec<super::managed::ForgeUnsettledOutput>,
    ) -> ForgeError {
        let (_, deciding) = failures
            .into_iter()
            .next()
            .expect("the caller reduces at least one failure");
        Self::attach_attempt_outputs(deciding, possible_outputs)
    }

    /// Carries one attempt's final possible-output snapshot out with a failure.
    ///
    /// Returns `failure` unchanged when the attempt can have produced nothing,
    /// so [`ForgeError::RewriteUnsettled`] only ever appears when it names
    /// objects a caller has to reclaim. An already-wrapped failure has its
    /// per-plan set replaced by the attempt-global one, which is its superset.
    fn attach_attempt_outputs(
        failure: ForgeError,
        possible_outputs: Vec<super::managed::ForgeUnsettledOutput>,
    ) -> ForgeError {
        if possible_outputs.is_empty() {
            return failure;
        }
        let source = match failure {
            ForgeError::RewriteUnsettled { source, .. } => source,
            other => Box::new(other),
        };
        ForgeError::RewriteUnsettled {
            source,
            possible_outputs,
        }
    }

    /// Settles every live replacement this table still owes, before any effect.
    ///
    /// The ordering is what makes ambiguity block fresh work rather than
    /// accumulate it: a rewrite planned while an earlier one may or may not
    /// have landed would be planned against a live set nobody can describe yet.
    /// Reconciliation settles a landed effect as Recovered and a provably
    /// absent one as Reset; anything it cannot settle keeps this attempt out of
    /// the catalog and the object store entirely.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Reconciliation`] when a live operation remains
    /// pending or unresolved, and the clock, fence, audit, and catalog failures
    /// reconciliation itself raises.
    async fn rewrite_settlement_barrier(
        &self,
        claim: &ForgeTaskClaim,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        stop: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let reconciled = self
            .forge
            .reconcile_live_replacements(
                lease,
                &super::compact::ForgeTableKey {
                    tenant: claim.data_tenant_id,
                    table_ref: binding.table_ref.clone(),
                },
                binding,
                stop,
                self.forge.core.clock.now()?,
            )
            .await?;
        if reconciled.pending > 0 || reconciled.unresolved > 0 {
            return Err(ForgeError::Reconciliation {
                detail: format!(
                    "{} pending and {} unresolved live operations block a fresh rewrite",
                    reconciled.pending, reconciled.unresolved
                ),
            });
        }
        Ok(())
    }
}

/// One admitted plan's complete execution and publication owner.
///
/// The queue owns one of these per plan; popping moves it into the future
/// scheduled on the compaction executor, and consuming it is the whole of that
/// plan's work. It therefore owns exactly what one plan needs and nothing the
/// worker keeps: no queue, no readiness, no worker configuration, no runtime,
/// no heartbeat, no task settlement, and no recovery or maintenance state. That
/// boundary is what lets the worker stay retained for supervision while its
/// plans run without any of them being able to reach it.
pub(crate) struct ForgeCompactionPlanRunner {
    /// Durable task this plan belongs to; the first half of its queue key.
    task_id: Uuid,
    /// Planner ordinal of this plan; the second half of its queue key.
    plan_index: usize,
    /// The real plan this runner executes, taken exactly once by
    /// [`Self::compact`].
    plan: Option<super::managed::ForgePlannedRewrite>,
    /// Operation identity minted before admission and used by this plan alone.
    ///
    /// A durable operation's Prepared detail is immutable and names exactly the
    /// inputs and outputs one commit replaces. Sibling plans of the same
    /// attempt replace different inputs, so one shared operation could only
    /// describe one of them truthfully.
    operation_id: Uuid,
    /// Shared Forge dependency graph this plan publishes through.
    forge: Arc<Forge>,
    /// Stable identity of the worker that admitted this plan.
    owner: Uuid,
    /// Durable claim this plan publishes under.
    claim: ForgeTaskClaim,
    /// Tenant-scoped table this plan publishes to.
    binding: TenantTableBinding,
    /// This attempt's table fence, cloned per plan.
    lease: ForgeLease,
    /// Managed context, table, evidence, and publication budget the attempt
    /// shares with every one of its plans.
    shared: Arc<ForgeRewriteAttempt>,
    /// The attempt's cancellation seam; this plan drains when it is cancelled.
    stop: CancellationToken,
    /// Passive test evidence sink; never consulted by a production decision.
    #[cfg(feature = "test-support")]
    completion_observer: Option<ForgeWorkerCompletionObserver>,
}

impl std::fmt::Debug for ForgeCompactionPlanRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForgeCompactionPlanRunner")
            .field("task_id", &self.task_id)
            .field("plan_index", &self.plan_index)
            .field("operation_id", &self.operation_id)
            .finish_non_exhaustive()
    }
}

impl ForgeCompactionPlanRunner {
    /// Builds one plan's runner from its worker and its suspended attempt.
    ///
    /// Everything is captured by value or by `Arc` here, before the plan is
    /// offered, so nothing the runner holds can later be invalidated by the
    /// worker moving on to another claim.
    fn new(
        worker: &ForgeWorker,
        state: &ForgeAttemptState,
        plan: super::managed::ForgePlannedRewrite,
    ) -> Self {
        Self {
            task_id: state.open.claim.task_id,
            plan_index: plan.plan_index,
            plan: Some(plan),
            operation_id: Uuid::now_v7(),
            forge: Arc::clone(&worker.forge),
            owner: worker.owner,
            claim: state.open.claim.clone(),
            binding: state.binding.clone(),
            lease: state.lease.clone(),
            shared: Arc::clone(&state.shared),
            stop: state.fenced.operation_stop.clone(),
            #[cfg(feature = "test-support")]
            completion_observer: worker.completion_observer.clone(),
        }
    }

    /// Rewrites this plan and publishes it under its own operation.
    ///
    /// This is the whole of one plan's work, and it consumes the runner so it
    /// can happen only once. The publication authority, deadline, and retry
    /// budget are the single-plan ones: a sibling's refusal or failure neither
    /// cancels nor weakens it. The attempt identity stays shared, because it is
    /// what every object this attempt wrote is named for and what the
    /// unsettled-output ledger is keyed by.
    ///
    /// # Errors
    ///
    /// Returns whatever managed execution and publication raise for this plan
    /// alone: [`ForgeError::Shutdown`] when the attempt drained,
    /// [`ForgeError::RewriteUnsettled`] when objects exist that no commit
    /// names, [`ForgeError::Catalog`] or [`ForgeError::Reconciliation`] when
    /// the catalog refused or its answer was lost, and the clock, audit,
    /// fence, and object-store failures the boundary raises.
    ///
    /// # Panics
    ///
    /// Panics when the plan was already taken, which consuming `self` prevents.
    async fn compact(mut self) -> Result<ForgeDispatchResult, ForgeError> {
        let plan = self
            .plan
            .take()
            .expect("a runner is consumed once and holds its plan until then");
        // A per-plan clone of the attempt's fence: every clone shares one
        // renewal record, so this is a mutable handle onto the same lease
        // rather than a second acquisition.
        let mut lease = self.lease.clone();
        tracing::debug!(
            worker = %self.owner,
            task_id = %self.task_id,
            plan_index = self.plan_index,
            operation_id = %self.operation_id,
            "Forge compaction plan started from the queue"
        );
        let handoff = self
            .shared
            .rewrite
            .rewrite_plan(plan, &self.shared.table)
            .await?;
        let metadata = self.shared.table.metadata();
        let context = RewritePublication {
            claim: &self.claim,
            binding: &self.binding,
            identity: super::publication::RewriteCommitIdentity {
                task_id: self.task_id,
                attempt_id: self.shared.rewrite.attempt_id(),
                operation_id: self.operation_id,
                group: ForgeGroupKey::table_audit_resource(
                    self.binding.tenant,
                    &self.binding.table_ref,
                ),
                plan_hash: super::planner::plan_hash(&self.claim.plan)?,
                evidence: self.shared.evidence.clone(),
            },
            key: ForgeGroupKey {
                tenant: self.binding.tenant,
                table_ref: self.binding.table_ref.clone(),
                partition: super::publication::rewrite_group_partition(
                    metadata.default_partition_spec(),
                    handoff.output_data_files.iter(),
                )?,
            },
            partition_spec_id: metadata.default_partition_spec_id(),
            target_file_size_bytes: super::managed::policy::declared_target_file_size_bytes(
                metadata,
            )?,
            planned_schema_id: metadata.current_schema_id(),
            deadline: self.shared.deadline,
        };
        #[cfg(feature = "test-support")]
        self.record_rewrite_evidence_for_test(&context.identity);
        let outcome = self
            .publish_rewrite(&context, &handoff, &mut lease, &self.stop)
            .await;
        if let Err(error) = &outcome {
            tracing::warn!(
                worker = %self.owner,
                task_id = %self.task_id,
                plan_index = self.plan_index,
                operation_id = %self.operation_id,
                error = %error,
                "Forge compaction plan failed"
            );
        }
        outcome
    }

    /// Records this plan's immutable evidence for a passive test observer.
    ///
    /// Called once managed execution has produced the attempt's evidence and
    /// before publication consumes it, which is the only point where the
    /// evidence and all three durable identities are known together.
    /// Test-support only and side-effect free.
    #[cfg(feature = "test-support")]
    fn record_rewrite_evidence_for_test(
        &self,
        identity: &super::publication::RewriteCommitIdentity,
    ) {
        if let Some(observer) = &self.completion_observer {
            observer.record_rewrite_evidence_for_test(ForgeRewriteEvidenceRecord {
                task_id: identity.task_id,
                attempt_id: identity.attempt_id,
                operation_id: identity.operation_id,
                evidence: identity.evidence.clone(),
            });
        }
    }

    /// Apply the observer's one-shot passive post-handoff barrier.
    ///
    /// Called by publication after managed execution produced the handoff and
    /// before authoritative metadata is reacquired. Without an armed observer
    /// it is a no-op, so no production decision depends on it.
    #[cfg(feature = "test-support")]
    async fn pause_after_handoff_for_test(&self) {
        if let Some(observer) = &self.completion_observer {
            observer.pause_after_handoff_for_test().await;
        }
    }

    /// Derives, prepares, and commits one handoff, retrying at most once.
    ///
    /// The first thing this does is load the table again. Managed
    /// execution ran while the table stayed open to every other writer, so the
    /// `Table` this attempt planned against is evidence of what was intended
    /// and never authority to publish: a branch move, a policy change, a
    /// replaced input, or a new delete that arrived during execution must be
    /// observed *before* the Prepared audit and before any catalog mutation, or
    /// a refusal would arrive after the effects it was supposed to prevent.
    /// The claimed base is then resolved out of that fresh metadata by exact
    /// snapshot identity, so a moved head cannot be silently reinterpreted as
    /// the plan's base.
    ///
    /// Each pass re-derives the replacement from the base it is about to commit
    /// against, because replaying a request derived against a stale base is
    /// what would delete files a concurrent writer has already replaced. The
    /// Prepared audit is written exactly once, only behind a complete
    /// `Proceed`, naming the inputs and outputs a successor must reconcile.
    ///
    /// A refusal reached before any catalog call is definite non-acceptance: it
    /// makes no `update_table` call, closes an already-Prepared operation once
    /// as Reset rather than stranding it, and carries the objects the managed
    /// core produced out with it as unsettled evidence so nothing is left that
    /// no attempt can name.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::RewriteUnsettled`] wrapping the refusal when
    /// authority is refused or no call was submitted, [`ForgeError::Catalog`]
    /// or [`ForgeError::Reconciliation`] when the catalog refused or its answer
    /// was lost, and the reload, derivation, audit, fence, and clock failures
    /// the boundary raises.
    /// Reacquires authoritative metadata and refuses before any effect.
    ///
    /// Managed execution ran while the table stayed open to every other
    /// writer, so the `Table` this attempt planned against is evidence of what
    /// was intended and never authority to publish. A branch move, a policy
    /// change, a replaced input, or a new delete that arrived during execution
    /// must be observed here — before one manifest is read for the derivation,
    /// before the Prepared audit, and before any catalog mutation — or a
    /// refusal would arrive after the effects it was supposed to prevent.
    ///
    /// A refusal reached here is definite non-acceptance: no `update_table`
    /// call was made, so the operation closes rather than stranding a successor
    /// with a commit that never happened, and the objects the managed core
    /// produced leave with it as unsettled evidence.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Catalog`] when the reload fails, and
    /// [`ForgeError::RewriteUnsettled`] wrapping the refusal when authority is
    /// refused, alongside the audit, fence, and clock failures that boundary
    /// raises.
    async fn acquire_publication_authority(
        &self,
        context: &RewritePublication<'_>,
        handoff: &super::managed::RewriteHandoff,
        lease: &mut ForgeLease,
        stop: &CancellationToken,
    ) -> Result<Table, ForgeError> {
        #[cfg(feature = "test-support")]
        self.pause_after_handoff_for_test().await;
        let current = self
            .forge
            .core
            .catalog
            .load_table(&context.binding.table_ident())
            .await
            .map_err(ForgeError::Catalog)?;
        if let Some(refusal) = self
            .rewrite_publication_refusal(context, &current, handoff, lease, stop)
            .await?
        {
            return Err(self
                .abandon_unsubmitted_rewrite(
                    context,
                    None,
                    handoff,
                    ForgeError::Reconciliation {
                        detail: format!(
                            "Forge rewrite publication refused before commit: {refusal:?}"
                        ),
                    },
                    lease,
                )
                .await);
        }
        Ok(current)
    }

    /// Derives one pass's replacement and records the Prepared audit once.
    ///
    /// Each pass re-derives the replacement from the base it is about to commit
    /// against, because replaying a request derived against a stale base is
    /// what would delete files a concurrent writer has already replaced. The
    /// claimed base is resolved out of `current` by exact snapshot identity, so
    /// a moved head cannot be silently reinterpreted as the plan's base.
    ///
    /// The file-level authorities live in the derivation, so a derivation
    /// refusal is a publication refusal and settles like one: no catalog call
    /// was made, and the managed outputs leave as unsettled evidence. The
    /// Prepared audit is written exactly once — when `prepared` is `None` —
    /// naming the inputs and outputs a successor must reconcile.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::RewriteUnsettled`] wrapping the derivation refusal
    /// when the fresh base cannot produce this replacement, and the audit and
    /// SQL failures the Prepared transition raises.
    async fn prepare_rewrite_request(
        &self,
        context: &RewritePublication<'_>,
        handoff: &super::managed::RewriteHandoff,
        current: &Table,
        prepared: Option<&super::publication::RewriteCommitRequest>,
        lease: &mut ForgeLease,
    ) -> Result<super::publication::RewriteCommitRequest, ForgeError> {
        let derived = async {
            let base = self
                .forge
                .rewrite_base_at(current, context.claim.base_snapshot_id)
                .await?;
            super::publication::RewriteCommitRequest::derive(
                super::publication::RewriteCommitInputs {
                    handoff,
                    base: &base,
                    selected_inputs: &context.claim.plan.inputs,
                    identity: &context.identity,
                },
            )
        }
        .await;
        let request = match derived {
            Ok(request) => request,
            Err(error) => {
                return Err(self
                    .abandon_unsubmitted_rewrite(context, prepared, handoff, error, lease)
                    .await);
            }
        };
        if prepared.is_none() {
            self.forge
                .append_live_audit(
                    lease,
                    &context.key,
                    &rewrite_operation(ForgeIcebergRewritePhase::Prepared),
                    context.audit(ForgeIcebergRewritePhase::Prepared, None, &request)?,
                )
                .await?;
        }
        Ok(request)
    }

    /// Publishes one immutable managed handoff and settles its operation.
    ///
    /// This is the sole boundary between managed execution and the catalog. It
    /// receives outputs that already exist in object storage and nothing that
    /// references them, and it leaves behind either a live replacement whose
    /// operation is settled, or a refusal whose operation is settled, or —
    /// exactly once, for one honest reason — an open Prepared operation a
    /// successor must reconcile from retained evidence.
    ///
    /// The order is load-decide-prepare-commit-settle, and each step exists
    /// because the table stayed open to every other writer while managed
    /// execution ran:
    ///
    /// 1. [`Self::acquire_publication_authority`] reloads the table *after* the
    ///    handoff and decides every knowable authority — lease and fence,
    ///    cancellation, the absolute deadline, branch, base, ancestry, and the
    ///    planned schema/spec/sort policy — against that fresh metadata. The
    ///    pre-execution table is evidence, never authority, so a refusal here
    ///    arrives before any Prepared row and before any catalog mutation.
    /// 2. [`Self::prepare_rewrite_request`] re-derives the replacement against
    ///    the base it is about to commit against, which is what carries the
    ///    file-level authorities (selected inputs still live, delete scope), and
    ///    writes the Prepared audit exactly once — on the first pass only —
    ///    naming the inputs and outputs a successor would reconcile.
    /// 3. The commit is submitted under `context.deadline`, one absolute budget
    ///    shared by every pass. A definite conflict — the catalog *answered*,
    ///    so nothing landed — buys a revalidated retry against the reloaded
    ///    table: the fixed schedule permits three of them, delayed 1s, 2s and
    ///    4s, and each one only while that same deadline still has budget for
    ///    the delay plus a call. No retry renews the deadline, and the schedule
    ///    is deliberately not configurable. Sibling plans of the same attempt
    ///    publish concurrently and independently while this one retries.
    /// 4. A committed submission settles the operation terminally through
    ///    [`Self::settle_committed_rewrite`], which writes the SQL settlement
    ///    and the terminal audit in the same transition.
    ///
    /// The distinction the return value carries is the point of the whole
    /// method. *Definitely unsubmitted* — a refusal, a not-submitted transport
    /// failure, or a definite conflict with the schedule spent — closes the
    /// operation here as Reset, because asking a successor to reconcile a
    /// commit that never started would block that table's fresh work forever.
    /// *Submitted but acceptance unknown* claims nothing: the operation is left
    /// Prepared and the error propagates, so the successor that takes the table
    /// over settles it as Recovered or Reset from the catalog's own snapshots.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Catalog`] when the post-handoff metadata load
    /// fails; [`ForgeError::Reconciliation`] wrapping the refusal when
    /// [`Self::rewrite_publication_refusal`] denies authority;
    /// [`ForgeError::RewriteUnsettled`] when the fresh base cannot re-derive
    /// the replacement or when a definite non-acceptance closes the operation,
    /// carrying the possible outputs as unsettled evidence;
    /// [`ForgeError::Shutdown`] or [`ForgeError::ShutdownRetained`] for a
    /// cancelled attempt, returned bare so the event loop can release the claim;
    /// [`ForgeError::Timeout`] when the absolute deadline expires with a call
    /// in flight; [`ForgeError::Invariant`] when a revalidated retry has no
    /// reloaded table; and the lease, fence, audit, and SQL failures raised by
    /// the authority decision, the Prepared transition, and the terminal
    /// settlement. An audit or settlement failure replaces the originating
    /// reason, because a transition that was not recorded is the more severe
    /// fact.
    ///
    /// # Cancellation and partial progress
    ///
    /// Cancellation observed before submission has zero effect: no Prepared
    /// row, no catalog mutation, and the live cut is exactly as it was.
    /// Cancellation observed while a call is in flight is *not* an answer — the
    /// commit may already have been accepted — so the operation stays Prepared
    /// and the managed outputs stay in object storage, unreferenced, until a
    /// successor settles that operation or the objects are reclaimed as
    /// orphans. Every returned error carries the attempt's possible output
    /// identities for that reason.
    ///
    /// A caller must therefore never read an error as "the catalog rejected the
    /// commit". Only a Reset settlement means that. An error alone means only
    /// that this attempt did not learn of an acceptance.
    async fn publish_rewrite(
        &self,
        context: &RewritePublication<'_>,
        handoff: &super::managed::RewriteHandoff,
        lease: &mut ForgeLease,
        stop: &CancellationToken,
    ) -> Result<ForgeDispatchResult, ForgeError> {
        let mut current = self
            .acquire_publication_authority(context, handoff, lease, stop)
            .await?;
        // Retains the request each pass derived, which is both the Prepared
        // marker and the exact detail a terminal transition must describe.
        let mut prepared: Option<super::publication::RewriteCommitRequest> = None;
        let mut retries_spent = 0_u32;
        loop {
            let request = self
                .prepare_rewrite_request(context, handoff, &current, prepared.as_ref(), lease)
                .await?;
            let request: &super::publication::RewriteCommitRequest = prepared.insert(request);
            let (acceptance, conflict) = match self
                .forge
                .commit_rewrite(
                    lease,
                    super::publication::ForgeRewriteCommit {
                        table: &current,
                        request,
                        deadline: context.deadline,
                    },
                    stop,
                )
                .await?
            {
                super::publication::RewriteSubmission::Committed(committed) => {
                    return self
                        .settle_committed_rewrite(context, request, *committed, lease)
                        .await;
                }
                super::publication::RewriteSubmission::NotSubmitted(error) => {
                    // Knowledge, not an outcome: no call started, so the
                    // operation closes here instead of waiting for a successor
                    // to reconcile a commit that never happened.
                    return Err(self
                        .abandon_unsubmitted_rewrite(context, Some(request), handoff, error, lease)
                        .await);
                }
                super::publication::RewriteSubmission::DefiniteConflict(error) => (
                    super::publication::RewriteAcceptance::DefiniteConflict,
                    error,
                ),
                super::publication::RewriteSubmission::AcceptanceUnknown(error) => {
                    (super::publication::RewriteAcceptance::Ambiguous, error)
                }
            };
            // The delay comes before the revalidation, not after it: a plan
            // that reloaded metadata and then slept would resubmit against a
            // picture of the table that is already as old as the backoff.
            // Ambiguity never reaches here, because no delay makes an unknown
            // commit safe to repeat.
            if matches!(
                acceptance,
                super::publication::RewriteAcceptance::DefiniteConflict
            ) {
                tracing::debug!(
                    task_id = %context.claim.task_id,
                    error = %conflict,
                    retries_spent,
                    "re-deriving one Forge rewrite after a definite catalog conflict"
                );
                // Every way that wait can fail is definite non-acceptance, so
                // each closes the operation here.
                if let Some(stopped) = self.conflict_backoff(context, retries_spent, stop).await? {
                    let reason = match stopped {
                        super::publication::RewriteRetryStop::Cancelled => ForgeError::Shutdown,
                        super::publication::RewriteRetryStop::Exhausted
                        | super::publication::RewriteRetryStop::DeadlineTruncated => conflict,
                    };
                    return Err(self
                        .abandon_unsubmitted_rewrite(context, Some(request), handoff, reason, lease)
                        .await);
                }
            }
            let (action, reloaded_after_conflict) = self
                .rewrite_follow_up(
                    context,
                    acceptance,
                    request,
                    handoff,
                    retries_spent,
                    lease,
                    stop,
                )
                .await?;
            match action {
                super::publication::RewriteConflictAction::ReconcileWithoutRecommit => {
                    return Self::reconcile_without_recommit(
                        context, acceptance, request, conflict,
                    );
                }
                super::publication::RewriteConflictAction::ResetDefinitelyUncommitted => {
                    // Certain non-acceptance, so this operation is closed here
                    // rather than left open for a successor to reconcile a
                    // commit that never happened. The outputs stay unreferenced
                    // and are reclaimed as orphans.
                    return Err(self
                        .abandon_unsubmitted_rewrite(
                            context,
                            Some(request),
                            handoff,
                            conflict,
                            lease,
                        )
                        .await);
                }
                super::publication::RewriteConflictAction::RevalidateAndRecommit => {}
            }
            retries_spent = retries_spent.saturating_add(1);
            current = reloaded_after_conflict.ok_or_else(|| ForgeError::Invariant {
                detail: "a revalidated Forge rewrite retry has no reloaded table".to_owned(),
            })?;
        }
    }

    /// Reports one publication that must reconcile rather than commit again.
    ///
    /// Ambiguity is the one non-success this owner may not settle: the
    /// operation is open, its outputs may be live, and this attempt holds the
    /// only copy of the identity, error, and volume a later proof needs. Every
    /// other reconcile-without-recommit outcome is a definite refusal whose
    /// effect was already found on the table, which its caller settles as the
    /// failure it is.
    ///
    /// # Errors
    ///
    /// Returns `conflict` unchanged for a definite refusal.
    fn reconcile_without_recommit(
        context: &RewritePublication<'_>,
        acceptance: super::publication::RewriteAcceptance,
        request: &super::publication::RewriteCommitRequest,
        conflict: ForgeError,
    ) -> Result<ForgeDispatchResult, ForgeError> {
        if matches!(acceptance, super::publication::RewriteAcceptance::Ambiguous) {
            return Ok(ForgeDispatchResult::AcceptanceUnknown(Box::new(
                ForgeUnknownAcceptance {
                    operation_id: context.identity.operation_id,
                    error: conflict,
                    volume: rewrite_volume(request),
                },
            )));
        }
        Err(conflict)
    }

    /// Waits the definite-conflict backoff owed before the next revalidated
    /// retry.
    ///
    /// The wait belongs before the revalidation, not after it: a plan that
    /// reloaded metadata and then slept would resubmit against a picture of the
    /// table that is already as old as the backoff.
    ///
    /// Returns the stop that ended the wait, or `None` when the retry may
    /// proceed. Every stop is definite non-acceptance, so the caller closes the
    /// operation rather than resubmitting.
    ///
    /// # Errors
    ///
    /// Returns the clock failure the deadline comparison raises.
    async fn conflict_backoff(
        &self,
        context: &RewritePublication<'_>,
        retries_spent: u32,
        stop: &CancellationToken,
    ) -> Result<Option<super::publication::RewriteRetryStop>, ForgeError> {
        Ok(super::publication::RewriteConflictSchedule::wait(
            retries_spent,
            context.deadline,
            self.forge.core.clock.now()?,
            stop,
        )
        .await
        .err())
    }

    /// Reports the one authority a fresh publication pass fails, if any.
    ///
    /// Split out so the refusal is decided against `current` — metadata loaded
    /// after managed execution — with no derived request in hand yet: an
    /// authority that has already lapsed must refuse before this attempt spends
    /// manifest reads deriving a replacement it may not publish, and long
    /// before the Prepared audit. The file-level dimensions stay with
    /// [`super::publication::RewriteCommitRequest::derive`], which owns them.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Lease`] or [`ForgeError::Sql`] when the lease
    /// cannot be renewed, the clock failures the deadline comparison raises,
    /// and the catalog failures the current-head live-set read raises.
    async fn rewrite_publication_refusal(
        &self,
        context: &RewritePublication<'_>,
        current: &Table,
        handoff: &super::managed::RewriteHandoff,
        lease: &mut ForgeLease,
        stop: &CancellationToken,
    ) -> Result<Option<super::publication::RewriteRefusal>, ForgeError> {
        let authority = self
            .rewrite_authority(
                lease,
                current,
                context,
                context.claim.base_snapshot_id,
                handoff,
                stop,
            )
            .await?;
        Ok(match authority.decide() {
            super::publication::RewriteCommitDecision::Proceed => None,
            super::publication::RewriteCommitDecision::Refuse(refusal) => Some(refusal),
        })
    }

    /// Closes one publication that provably made no catalog call.
    ///
    /// Two things have to happen together and neither is optional. An operation
    /// that reached Prepared is settled once as Reset, because leaving it open
    /// would ask a successor to reconcile a commit that never started. And the
    /// objects the managed core already wrote travel out on the returned error
    /// as unsettled evidence, because this attempt is the last thing that can
    /// name them; they stay unreferenced and are reclaimed as orphans.
    ///
    /// Cancellation is the one refusal that is returned bare. The event loop
    /// matches [`ForgeError::Shutdown`] exactly to release a cleanly cancelled
    /// claim, and a wrapper would silently turn that release into a retention.
    ///
    /// Returns the error to propagate, which is the audit failure instead when
    /// the Reset transition could not be recorded.
    async fn abandon_unsubmitted_rewrite(
        &self,
        context: &RewritePublication<'_>,
        prepared: Option<&super::publication::RewriteCommitRequest>,
        handoff: &super::managed::RewriteHandoff,
        reason: ForgeError,
        lease: &mut ForgeLease,
    ) -> ForgeError {
        if let Some(request) = prepared {
            let reset = match context.audit(ForgeIcebergRewritePhase::Reset, None, request) {
                Ok(detail) => detail,
                Err(error) => return error,
            };
            if let Err(error) = self
                .forge
                .append_live_audit(
                    lease,
                    &context.key,
                    &rewrite_operation(ForgeIcebergRewritePhase::Reset),
                    reset,
                )
                .await
            {
                return error;
            }
        }
        if matches!(
            reason,
            ForgeError::Shutdown
                | ForgeError::ShutdownRetained
                | ForgeError::RewriteUnsettled { .. }
        ) {
            return reason;
        }
        ForgeError::RewriteUnsettled {
            source: Box::new(reason),
            possible_outputs: Self::unsettled_rewrite_outputs(handoff),
        }
    }

    /// Projects one handoff's outputs into the unsettled-object evidence shape.
    ///
    /// The core returned a handoff, so every object it names was written and
    /// closed; `settled` is therefore true and the ordinal is the handoff's own
    /// order, which is the only attempt-global ordering this owner can honestly
    /// report.
    fn unsettled_rewrite_outputs(
        handoff: &super::managed::RewriteHandoff,
    ) -> Vec<super::managed::ForgeUnsettledOutput> {
        handoff
            .output_data_files
            .iter()
            .zip(0_u64..)
            .map(
                |(file, logical_ordinal)| super::managed::ForgeUnsettledOutput {
                    logical_ordinal,
                    path: file.file_path().to_owned(),
                    settled: true,
                },
            )
            .collect()
    }

    /// Records the terminal audit for a rewrite the catalog accepted.
    ///
    /// The committed snapshot identity is read off the table the commit
    /// returned rather than predicted, so the audit row names the snapshot a
    /// successor would find if it had to reconcile this operation instead.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the request cannot render its
    /// audit paths, and the fence and SQL failures the audit append raises.
    async fn settle_committed_rewrite(
        &self,
        context: &RewritePublication<'_>,
        request: &super::publication::RewriteCommitRequest,
        committed: Table,
        lease: &mut ForgeLease,
    ) -> Result<ForgeDispatchResult, ForgeError> {
        let committed_snapshot_id = committed
            .metadata()
            .current_snapshot()
            .map(|snapshot| snapshot.snapshot_id());
        self.forge
            .append_live_audit(
                lease,
                &context.key,
                &rewrite_operation(ForgeIcebergRewritePhase::Committed),
                context.audit(
                    ForgeIcebergRewritePhase::Committed,
                    committed_snapshot_id,
                    request,
                )?,
            )
            .await?;
        Ok(ForgeDispatchResult::Committed(Box::new(
            ForgeCommittedPublication {
                table: committed,
                volume: Some(rewrite_volume(request)),
            },
        )))
    }

    /// Decides what one non-success catalog outcome permits next.
    ///
    /// A refusal is only *certain non-acceptance* if this operation's effect is
    /// absent from the table. A lost response looks identical from here — the
    /// replacement landed, the answer did not — and a resubmission would then
    /// delete files the landed snapshot already replaced. Reading the task
    /// identity back off the table separates the two, so an already-landed
    /// rewrite is never reset and never re-committed: the operation stays open
    /// and a successor settles it from that same evidence. Ambiguity is never
    /// given that chance; it reconciles without loading anything, because no
    /// observation made after a lost answer can make resubmitting it safe.
    ///
    /// Returns the action together with the reloaded table a revalidated retry
    /// must derive against, which is `None` for every terminal action.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Reconciliation`] when the refusal arrived after
    /// this task's own effect landed, and the catalog, object-store, clock, and
    /// fence failures the reload and the authority read raise.
    #[expect(
        clippy::too_many_arguments,
        reason = "a follow-up decision needs the whole publication frame: context, acceptance, request, handoff, retry count, fence, and cancellation"
    )]
    async fn rewrite_follow_up(
        &self,
        context: &RewritePublication<'_>,
        acceptance: super::publication::RewriteAcceptance,
        request: &super::publication::RewriteCommitRequest,
        handoff: &super::managed::RewriteHandoff,
        retries_spent: u32,
        lease: &mut ForgeLease,
        stop: &CancellationToken,
    ) -> Result<(super::publication::RewriteConflictAction, Option<Table>), ForgeError> {
        match acceptance {
            super::publication::RewriteAcceptance::Ambiguous => Ok((
                acceptance.next_action(
                    retries_spent,
                    false,
                    super::publication::RewriteCommitDecision::Proceed,
                ),
                None,
            )),
            super::publication::RewriteAcceptance::DefiniteConflict => {
                let refreshed = self
                    .forge
                    .core
                    .catalog
                    .load_table(&context.binding.table_ident())
                    .await
                    .map_err(ForgeError::Catalog)?;
                if ForgeWorker::find_retained_task_evidence(
                    &self.forge,
                    context.binding,
                    &refreshed,
                    "forge.operation_id",
                    context.identity.operation_id,
                )
                .await?
                .is_some()
                {
                    return Err(ForgeError::Reconciliation {
                        detail: "Forge rewrite commit was refused after its own effect landed"
                            .to_owned(),
                    });
                }
                let authority = self
                    .rewrite_authority(
                        lease,
                        &refreshed,
                        context,
                        request.base_snapshot_id,
                        handoff,
                        stop,
                    )
                    .await?;
                let action = acceptance.next_action(
                    retries_spent,
                    context.deadline.passed(self.forge.core.clock.now()?),
                    authority.decide(),
                );
                Ok((action, Some(refreshed)))
            }
        }
    }

    /// Collects every knowable authority for one publication at the boundary.
    ///
    /// Nothing here decides anything: each field is answered by the owner that
    /// actually knows it — the lease, the clock, the loaded table — and the
    /// ordering and completeness of the verdict belong to
    /// [`super::publication::RewriteCommitAuthority::decide`]. Reading them all
    /// before any of them is acted on is what keeps a refusal free of partial
    /// effects.
    ///
    /// `inputs_all_live` is answered against the *current head*, not against
    /// the planning base. A retained planning snapshot lets a disjoint plan
    /// compose on a moving head, but it never permits replacing an input a
    /// concurrent writer already removed: doing so would resurrect the rows the
    /// earlier replacement retired. The head's live data set is therefore read
    /// through the same manifest reader publication already owns, and the
    /// authority holds only when every rewritten path is still in it.
    ///
    /// `delete_scope_safe` is recorded as held because
    /// [`super::publication::RewriteCommitRequest::derive`] is its owner and
    /// refuses otherwise: the caller derives against the planning base right
    /// after this call and settles that refusal the same way, so the two halves
    /// of the decision cover every dimension between them.
    ///
    /// The base is named by the durable claim rather than read off a derived
    /// request, so this can run against freshly loaded metadata.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Sql`] when the lease cannot be renewed against the
    /// operator pool, the clock failures the deadline comparison raises, and
    /// the catalog and manifest failures the current-head live-set read raises.
    async fn rewrite_authority(
        &self,
        lease: &mut ForgeLease,
        table: &Table,
        context: &RewritePublication<'_>,
        base_snapshot_id: i64,
        handoff: &super::managed::RewriteHandoff,
        stop: &CancellationToken,
    ) -> Result<super::publication::RewriteCommitAuthority, ForgeError> {
        let inputs_all_live = self
            .current_head_holds_inputs(table, &handoff.rewritten_data_files)
            .await?;
        let metadata = table.metadata();
        Ok(super::publication::RewriteCommitAuthority {
            fence: super::publication::RewriteFenceAuthority {
                lease_held: lease.renew(&self.forge.core.operator_pool).await?,
                fence_held: !lease.remaining().is_zero(),
                commit_window_fits: lease
                    .commit_window_fits(self.forge.core.config.commit_window()),
            },
            attempt: super::publication::RewriteAttemptAuthority {
                cancelled: stop.is_cancelled(),
                deadline_passed: context.deadline.passed(self.forge.core.clock.now()?),
            },
            table: super::publication::RewriteTableAuthority {
                base_is_retained: metadata.snapshot_by_id(base_snapshot_id).is_some(),
                schema_unchanged: context.planned_schema_id == metadata.current_schema_id(),
            },
            files: super::publication::RewriteFileAuthority {
                inputs_all_live,
                delete_scope_safe: true,
            },
        })
    }

    /// Reports whether every rewritten input is still live on the current head.
    ///
    /// The head's own live file set is read through
    /// [`super::Forge::rewrite_base_at`], the one manifest-reading owner, so
    /// this check never duplicates manifest traversal. A table with no current
    /// snapshot holds nothing, which refuses any plan that names an input.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Catalog`] when the manifest list or a manifest
    /// cannot be read, and the invariant failures the base assembly raises.
    async fn current_head_holds_inputs(
        &self,
        table: &Table,
        rewritten_data_files: &[String],
    ) -> Result<bool, ForgeError> {
        if rewritten_data_files.is_empty() {
            return Ok(true);
        }
        let Some(head) = table.metadata().current_snapshot_id() else {
            return Ok(false);
        };
        let live = self.forge.rewrite_base_at(table, head).await?;
        Ok(live.holds_all_data(rewritten_data_files))
    }
}

impl ForgeWorker {
    /// Converts catalog file paths into the audit row's storage paths.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when a catalog path is not a valid
    /// storage path, which would leave an audit row that cannot name the object
    /// it settled.
    fn rewrite_audit_paths<'file>(
        files: impl IntoIterator<Item = &'file iceberg::spec::DataFile>,
    ) -> Result<Vec<StoragePath>, ForgeError> {
        files
            .into_iter()
            .map(|file| {
                StoragePath::new(file.file_path()).map_err(|error| ForgeError::Invariant {
                    detail: format!("rewrite file path is not a storage path: {error}"),
                })
            })
            .collect()
    }

    /// Prepares, revalidates, and fast-appends one Scribe promotion group.
    ///
    /// The Prepared audit transition is written before any catalog effect, so a
    /// worker that dies during the append leaves durable evidence naming the
    /// exact operation and file set a successor must reconcile. Revalidation
    /// then runs against the objects that currently exist, and only the
    /// writer's own `DataFile` values reach the catalog.
    ///
    /// A catalog refusal that is not retryable is a *definite* conflict: the
    /// catalog answered, so the append certainly did not land. Exactly one
    /// retry is permitted against it, and only after reloading the table and
    /// revalidating the durable demand again — replaying the same plan against
    /// a stale base is what would promote a file set that no longer exists.
    /// The deadline is captured once, before the first attempt, so a slow
    /// first attempt cannot buy the retry more time than the original commit
    /// budget allowed. Every other failure, including an uncertain one, is
    /// returned unchanged for evidence-based recovery.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the persisted parameters do not
    /// decode into an exact plan, [`ForgeError::InvalidConfig`] when the
    /// configured Iceberg retry budget is not representable as a deadline, and
    /// the audit, fence, object-store, and catalog failures raised by
    /// preparation, revalidation, and the commit.
    async fn dispatch_scribe_promotion(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        table: &Table,
        stop: &CancellationToken,
    ) -> Result<ForgeDispatchResult, ForgeError> {
        let plan = Self::promotion_plan(claim)?;
        let operation_id = Self::promotion_operation_id(claim);
        self.forge
            .settle_promotion(
                lease,
                binding,
                ForgePromotionSettlement {
                    plan: &plan,
                    phase: ForgeScribePromotionPhase::Prepared,
                    operation_id,
                    base_snapshot_id: claim.base_snapshot_id,
                    committed_snapshot_id: None,
                },
            )
            .await?;
        let deadline = self.forge.core.clock.now()?
            + chrono::Duration::from_std(self.forge.core.config.iceberg_total_retry_timeout)
                .map_err(|_| ForgeError::InvalidConfig {
                    detail: "Forge Iceberg retry timeout is not representable".to_owned(),
                })?;
        let mut reloaded: Option<Table> = None;
        let mut retried = false;
        loop {
            let base = reloaded.as_ref().unwrap_or(table);
            let data_files = self
                .forge
                .revalidate_promotion(binding, &plan, base)
                .await?;
            let conflict = match self
                .forge
                .commit_promotion(
                    lease,
                    ForgePromotionCommit {
                        table: base,
                        data_files,
                        task_id: claim.task_id,
                        attempt_id: attempt,
                        operation_id,
                    },
                    stop,
                )
                .await
            {
                Ok(committed) => {
                    return Ok(ForgeDispatchResult::Committed(Box::new(
                        ForgeCommittedPublication {
                            table: committed,
                            volume: Some(Self::promotion_volume(claim)),
                        },
                    )));
                }
                Err(ForgeError::Catalog(error)) if !error.retryable() => error,
                Err(error) => return Err(error),
            };
            let reloaded_after_conflict = self
                .forge
                .core
                .catalog
                .load_table(&binding.table_ident())
                .await
                .map_err(ForgeError::Catalog)?;
            // A refusal is only *certain non-acceptance* if this operation's
            // effect is absent from the table. A lost response looks identical
            // from here — the commit landed, the answer did not — and the
            // catalog's own duplicate check then refuses the resubmission of
            // files it already references. Reading the operation identity back
            // off the table separates the two, so an already-landed promotion
            // is never reset and never re-appended: the operation stays open
            // and a successor settles it from that same evidence.
            if ForgeWorker::find_retained_task_evidence(
                &self.forge,
                binding,
                &reloaded_after_conflict,
                "forge.operation_id",
                operation_id,
            )
            .await?
            .is_some()
            {
                return Err(ForgeError::Reconciliation {
                    detail: "Scribe promotion commit was refused after its own effect landed"
                        .to_owned(),
                });
            }
            if retried || self.forge.core.clock.now()? >= deadline {
                // A definite conflict is certain non-acceptance, so this
                // operation is closed here rather than left open for a
                // successor to reconcile a commit that never happened. No row
                // is settled: `committed_snapshot_id` stays `None`.
                self.reset_promotion_operation(claim, binding, lease, &plan, operation_id)
                    .await?;
                return Err(ForgeError::Catalog(conflict));
            }
            tracing::debug!(
                task_id = %claim.task_id,
                error = %conflict,
                "revalidating one Scribe promotion after a definite catalog conflict"
            );
            retried = true;
            reloaded = Some(reloaded_after_conflict);
        }
    }

    /// Closes one promotion operation as `Reset` after certain non-acceptance.
    ///
    /// Reached only once the catalog has definitely refused and the table has
    /// been read back without this operation's effect, so the commit certainly
    /// did not land. Closing here is what keeps a successor from reconciling a
    /// commit that never happened; `committed_snapshot_id` stays `None`
    /// because no snapshot was settled.
    ///
    /// # Errors
    ///
    /// Returns the SQL, audit, and fence failures the promotion settlement
    /// boundary raises.
    async fn reset_promotion_operation(
        &self,
        claim: &ForgeTaskClaim,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        plan: &ScribePromotionPlan,
        operation_id: Uuid,
    ) -> Result<(), ForgeError> {
        self.forge
            .settle_promotion(
                lease,
                binding,
                ForgePromotionSettlement {
                    plan,
                    phase: ForgeScribePromotionPhase::Reset,
                    operation_id,
                    base_snapshot_id: claim.base_snapshot_id,
                    committed_snapshot_id: None,
                },
            )
            .await
    }

    /// Decodes one promotion claim's persisted plan.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the parameters are not an object
    /// or do not decode into an exact, digest-consistent promotion plan.
    fn promotion_plan(claim: &ForgeTaskClaim) -> Result<ScribePromotionPlan, ForgeError> {
        let parameters =
            claim
                .plan
                .parameters
                .as_object()
                .ok_or_else(|| ForgeError::Invariant {
                    detail: "Forge task parameters must be an object".to_owned(),
                })?;
        ScribePromotionPlan::from_parameters(parameters)
    }

    /// Returns the stable operation identity for one promotion task.
    ///
    /// The durable task identity *is* the operation identity. A task is
    /// enqueued once per exact file set and is retried in place rather than
    /// re-planned, so binding the operation to it makes the identity survive
    /// every retry, restart, and takeover without a second durable column to
    /// keep consistent.
    const fn promotion_operation_id(claim: &ForgeTaskClaim) -> Uuid {
        claim.task_id
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

    /// Persists exact cleanup evidence, deletes it with a durable cursor, then runs orphan GC.
    ///
    /// Its production span carries only closed strategy/result/role/kind fields
    /// and the scrubbed durable task and attempt UUIDs.
    ///
    /// Candidate derivation has already completed from before/after Iceberg
    /// metadata. This method records the complete ordered set as durable
    /// Prepared evidence and deletes nothing: the exact candidates are the
    /// handoff an independently claimed expired-cleanup task consumes.
    ///
    /// # Errors
    ///
    /// Returns path binding, evidence, SQL, audit, fencing, object-store, or
    /// cancellation failures. A failure retains Prepared evidence and its last
    /// committed cursor for deterministic takeover.
    async fn complete_snapshot_expiry(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        result: ForgeSnapshotExpiryResult,
        stop: &CancellationToken,
    ) -> Result<(ForgeTaskEvidence, ForgeExecutionEvidenceState), ForgeError> {
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
            self.complete_snapshot_expiry_inner(claim, attempt, binding, lease, result, stop),
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
            Self::record_expired_cleanup_completion(started);
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
    async fn complete_snapshot_expiry_inner(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        result: ForgeSnapshotExpiryResult,
        stop: &CancellationToken,
    ) -> Result<(ForgeTaskEvidence, ForgeExecutionEvidenceState), ForgeError> {
        require_running(stop)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        // A snapshot expiration settled its own task, operation, claims, audits,
        // and planning demand in one transaction and deliberately deleted
        // nothing: its exact candidates are the handoff to separate cleanup.
        if let Some(evidence) = result.expiry_evidence {
            return Ok((evidence, ForgeExecutionEvidenceState::Settled));
        }
        let evidence = self.committed_evidence(binding, &result.table).await?;
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
            .forge
            .core
            .fail_after_maintenance_prepared
            .swap(false, Ordering::AcqRel)
        {
            return Err(ForgeError::Reconciliation {
                detail: "injected crash after maintenance task evidence commit".to_owned(),
            });
        }
        require_running(stop)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        Ok((evidence, ForgeExecutionEvidenceState::Prepared))
    }

    /// Runs one claimed expired-cleanup task to its drained frontier.
    ///
    /// The immutable payload is the whole work: it names the succeeded
    /// expiration this task consumes, the committed identity that expiration
    /// left behind, and the exact ordered candidates that commit made
    /// unreachable. Nothing here is re-derived from the catalog, so a task that
    /// runs much later deletes exactly the objects its handoff named or nothing
    /// at all.
    ///
    /// # Errors
    ///
    /// Returns payload-decoding, protection, path-binding, object-store,
    /// fencing, audit, SQL, or cancellation failures. Every failure leaves the
    /// durable cursor authoritative, so the exact candidate is replayed by this
    /// or a successor attempt.
    /// Runs one bounded orphan-collection pass for an already-claimed task.
    ///
    /// The task row's tenant and table plus its immutable prefix and cutoff are
    /// the complete scan identity, so the first thing this does is re-bind that
    /// prefix to the table's *current* Forge recipe root. A plan naming another
    /// table's prefix, or a stale root, is refused before the lease is used and
    /// before a single object is listed.
    ///
    /// Everything destructive stays with the retained owner: listing,
    /// protection, the sorted batch, its `OrphanGc` prepare/delete/reconcile
    /// protocol, and crash recovery. What this adds is the durable scan
    /// position. After the selected batch settles, a partial pass checkpoints
    /// the last completely processed key and releases the task without
    /// consuming failure budget, which is what keeps a leading run of protected
    /// objects from starving the pages behind it. An exhausted prefix instead
    /// clears the cursor and takes the audited terminal transition.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Sql`] when the plan is not the closed orphan
    /// contract or a durable transition is refused, [`ForgeError::Invariant`]
    /// when the plan prefix is not this table's current Forge recipe root, and
    /// every lease, catalog, protection, object-store, audit, and cancellation
    /// failure the retained collection owner raises.
    ///
    /// # Cancellation
    ///
    /// Cancellation observed by the retained owner leaves any prepared batch
    /// open and does not advance the cursor; the task stays claimable by
    /// lease-expiry reclaim with its durable position unchanged.
    async fn dispatch_orphan_cleanup(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        table: &Table,
        stop: &CancellationToken,
    ) -> Result<ForgeDispatchResult, ForgeError> {
        let payload = claim
            .plan
            .orphan_cleanup_payload(ForgeTaskStrategy::OrphanCleanup, true)
            .map_err(ForgeError::Sql)?;
        let prefix = claim
            .plan
            .orphan_cleanup_prefix(true)
            .map_err(ForgeError::Sql)?;
        let expected = catalog_path_to_object_key(
            table.metadata().location(),
            binding,
            &self.forge.core.staging,
            &forge_data_location(table.metadata().location()),
        )?;
        if prefix != expected {
            return Err(ForgeError::Invariant {
                detail: format!(
                    "orphan cleanup plan prefix {prefix} is not this table's Forge root {expected}"
                ),
            });
        }
        let start_after = claim
            .evidence
            .as_ref()
            .and_then(ForgeTaskRowEvidence::orphan_scan)
            .map(|cursor| cursor.start_after.clone());
        let key = super::compact::ForgeTableKey {
            tenant: claim.data_tenant_id,
            table_ref: binding.table_ref.clone(),
        };
        // The retained collector owns the scan; this route owns its
        // observation. Both the stage duration and the deleted/retained/partial
        // accounting are recorded on the failure path too, so a failed bounded
        // pass still moves its stage-failure series before the error propagates.
        let started = Instant::now();
        let result = self
            .forge
            .run_orphan_gc_for_table(
                lease,
                &key,
                binding,
                super::orphan_gc::OrphanGcScan {
                    now: self.forge.core.clock.now()?,
                    age_cutoff_ms: Some(payload.age_cutoff_ms),
                    start_after: start_after.as_deref(),
                    task_id: Some(claim.task_id),
                },
                stop,
            )
            .await;
        let elapsed = started.elapsed();
        let outcome = result?;
        tracing::debug!(
            task_id = %claim.task_id,
            elapsed_seconds = elapsed.as_secs_f64(),
            "Forge orphan collection scan returned"
        );
        let authority = ForgeExpirationAuthority {
            task_id: claim.task_id,
            attempt_id: attempt,
            worker_id: self.owner,
            lease_key: lease.lease_key.clone(),
            lease_fencing_token: lease.fencing_token,
        };
        let claim_table = self.forge.expiry_claim_table(&key, table).await?;
        if outcome.partial {
            if let Some(frontier) = outcome.frontier.as_deref() {
                self.tasks
                    .checkpoint_orphan_cleanup_cursor(
                        claim.data_tenant_id,
                        &authority,
                        &claim_table,
                        frontier,
                    )
                    .await
                    .map_err(ForgeError::Sql)?;
            }
            // A bounded pass is progress, not a failure: the same task returns
            // to the pool with its position intact and its budget untouched.
            self.tasks
                .retry(claim.task_id, attempt, self.owner, chrono::Utc::now())
                .await
                .map_err(ForgeError::Sql)?;
        } else {
            self.tasks
                .complete_orphan_cleanup(
                    claim.data_tenant_id,
                    &authority,
                    &claim_table,
                    &task_event(
                        claim.task_id,
                        ForgeTaskState::Succeeded,
                        "orphan cleanup prefix exhausted",
                    ),
                )
                .await
                .map_err(ForgeError::Sql)?;
        }
        Ok(ForgeDispatchResult::SelfSettled)
    }

    async fn dispatch_expired_cleanup(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        table: &Table,
        stop: &CancellationToken,
    ) -> Result<ForgeDispatchResult, ForgeError> {
        let payload = claim
            .plan
            .expired_cleanup_payload(ForgeTaskStrategy::ExpiredCleanup, true)
            .map_err(ForgeError::Sql)?;
        let cleanup = CleanupAttempt {
            task_id: claim.task_id,
            tenant: claim.data_tenant_id,
            attempt,
            binding,
        };
        let evidence = self
            .drain_expired_cleanup(
                &cleanup,
                lease,
                &payload,
                table,
                CleanupResume::default(),
                stop,
            )
            .await?;
        Ok(ForgeDispatchResult::Cleaned(Box::new(evidence)))
    }

    /// Deletes every remaining candidate through the two-phase protocol.
    ///
    /// Each candidate is handled alone and in order: prepare durably, close
    /// Postgres, take a fresh reachability proof, submit the delete, then
    /// settle. Postgres is never open across the object-store call, and the
    /// frontier advances only for a confirmed deletion or a proven absence, so
    /// a refusal or an uncertain acceptance retains the exact candidate for
    /// replay instead of skipping it.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Reconciliation`] when a candidate is refused or
    /// its acceptance is unknown, plus protection, path-binding, object-store,
    /// fencing, audit, SQL, or cancellation failures.
    ///
    /// # Cancellation
    ///
    /// Cancellation observed before a delete is submitted stops the drain with
    /// the frontier intact. Cancellation racing a submitted delete is recorded
    /// as uncertainty, which retains the candidate rather than advancing past
    /// an object whose fate is unknown.
    async fn drain_expired_cleanup(
        &self,
        attempt: &CleanupAttempt<'_>,
        lease: &mut ForgeLease,
        payload: &ExpiredCleanupPayload,
        table: &Table,
        resume: CleanupResume,
        stop: &CancellationToken,
    ) -> Result<ForgeTaskEvidence, ForgeError> {
        let started = Instant::now();
        let total =
            u32::try_from(payload.cleanup_candidates.len()).map_err(|_| ForgeError::Invariant {
                detail: "expired cleanup candidate set exceeds u32".to_owned(),
            })?;
        let authority = ForgeExpirationAuthority {
            task_id: attempt.task_id,
            attempt_id: attempt.attempt,
            worker_id: self.owner,
            lease_key: lease.lease_key.clone(),
            lease_fencing_token: lease.fencing_token,
        };
        let key = super::compact::ForgeTableKey {
            tenant: attempt.tenant,
            table_ref: attempt.binding.table_ref.clone(),
        };
        let table = self.forge.expiry_claim_table(&key, table).await?;
        let mut prepared = resume.prepared;
        let mut index = resume.frontier;
        while index < total {
            let candidate = payload
                .cleanup_candidates
                .get(usize::try_from(index).unwrap_or(usize::MAX))
                .ok_or_else(|| ForgeError::Invariant {
                    detail: "expired cleanup cursor named an absent candidate".to_owned(),
                })?;
            if !prepared {
                require_running(stop)?;
                lease.require_fence(&self.forge.core.operator_pool).await?;
                let event =
                    cleanup_event(attempt.task_id, "forge.expired_cleanup.candidate_prepared");
                self.tasks
                    .prepare_expired_cleanup_candidate(
                        attempt.tenant,
                        ExpiredCleanupCandidateRequest {
                            authority: &authority,
                            table: &table,
                            index,
                            candidate,
                            event: &event,
                        },
                    )
                    .await
                    .map_err(ForgeError::Sql)?;
            }
            prepared = false;
            let outcome = self
                .attempt_cleanup_delete(attempt, lease, index, candidate, stop)
                .await?;
            let event = cleanup_event(attempt.task_id, outcome.audit_operation());
            self.tasks
                .settle_expired_cleanup_candidate(
                    attempt.tenant,
                    ExpiredCleanupCandidateRequest {
                        authority: &authority,
                        table: &table,
                        index,
                        candidate,
                        event: &event,
                    },
                    outcome,
                )
                .await
                .map_err(ForgeError::Sql)?;
            if !outcome.advances() {
                return Err(ForgeError::Reconciliation {
                    detail: format!(
                        "expired cleanup retained candidate {index} for replay after {}",
                        outcome.audit_operation()
                    ),
                });
            }
            index = index.saturating_add(1);
        }
        Self::record_expired_cleanup_completion(started);
        Ok(ForgeTaskEvidence {
            version: FORGE_TASK_PAYLOAD_VERSION,
            committed_snapshot_id: Some(payload.committed_snapshot_id),
            committed_metadata_location: Some(payload.committed_metadata_location.clone()),
            committed_metadata_digest: Some(payload.committed_metadata_digest.clone()),
            cleanup_candidates: payload.cleanup_candidates.clone(),
            deleted_candidate_count: total,
            prepared_candidate_index: None,
        })
    }

    /// Proves one prepared candidate is still safe to delete right now.
    ///
    /// No Postgres transaction is alive here: the preparation committed and
    /// closed before this runs, so the object-store call cannot hold a database
    /// resource. The protection proof exempts exactly this task, attempt,
    /// cursor index, and candidate, so the drain's own prepared row stops
    /// protecting the object it is about to delete while every other
    /// unresolved preparation still does. `Ok(None)` reports an absence proven
    /// by a fresh stat, the only non-deleting outcome permitted to advance the
    /// cursor; `Ok(Some(path))` is the bound path the caller may delete.
    ///
    /// # Errors
    ///
    /// Returns clock, protection, object-metadata, refreshed-eligibility,
    /// path-binding, cancellation, and fencing failures. Every one of them
    /// happens strictly before a deletion is constructed, so the caller settles
    /// them all as [`ExpiredCleanupOutcome::Refused`] rather than propagating.
    async fn prove_cleanup_candidate(
        &self,
        attempt: &CleanupAttempt<'_>,
        lease: &mut ForgeLease,
        index: u32,
        candidate: &ForgeCleanupCandidate,
        stop: &CancellationToken,
    ) -> Result<Option<String>, ForgeError> {
        let key = super::compact::ForgeTableKey {
            tenant: attempt.tenant,
            table_ref: attempt.binding.table_ref.clone(),
        };
        let protection = self
            .forge
            .load_expired_cleanup_protection(
                &key,
                attempt.binding,
                self.forge.core.clock.now()?,
                ExpiredCleanupExemption {
                    task_id: attempt.task_id,
                    attempt_id: attempt.attempt,
                    index,
                    candidate,
                },
                stop,
            )
            .await?;
        let path = candidate.path.as_str();
        let metadata = match self.forge.core.object_store.stat(path).await {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == opendal::ErrorKind::NotFound => None,
            Err(error) => return Err(ForgeError::ObjectDelete(error)),
        };
        let evidence = metadata
            .as_ref()
            .map_or(ObjectEvidence::Missing, ObjectEvidence::Present);
        match protection.expired_cleanup_eligibility(attempt.binding, path, evidence) {
            GcEligibility::Eligible => {}
            GcEligibility::Missing => return Ok(None),
            refusal => {
                return Err(ForgeError::Reconciliation {
                    detail: format!(
                        "refreshed protection refused a prepared expired-cleanup candidate: {refusal:?}"
                    ),
                });
            }
        }
        let path = attempt.binding.validate_object_path(path).ok_or_else(|| {
            ForgeError::Reconciliation {
                detail: "expired cleanup candidate escaped table binding".to_owned(),
            }
        })?;
        // The last two proofs before the irreversible effect: still running,
        // and still the fenced publication owner.
        require_running(stop)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        Ok(Some(path))
    }

    /// Takes one candidate's fresh proof and submits its deletion.
    ///
    /// No Postgres transaction is alive here: the preparation committed and
    /// closed before this runs, so the object-store call cannot hold a database
    /// resource. The protection proof exempts exactly this task, attempt,
    /// cursor index, and candidate, so the drain's own prepared row stops
    /// protecting the object it is about to delete while every other
    /// unresolved preparation still does.
    ///
    /// # Errors
    ///
    /// Never fails for a pre-submission cause: every clock, protection,
    /// object-metadata, eligibility, path-binding, cancellation, and fencing
    /// failure observed before the deletion is constructed is normalized to
    /// [`ExpiredCleanupOutcome::Refused`] so it reaches the settlement and
    /// audit path with the candidate untouched and still prepared.
    async fn attempt_cleanup_delete(
        &self,
        attempt: &CleanupAttempt<'_>,
        lease: &mut ForgeLease,
        index: u32,
        candidate: &ForgeCleanupCandidate,
        stop: &CancellationToken,
    ) -> Result<ExpiredCleanupOutcome, ForgeError> {
        let path = match self
            .prove_cleanup_candidate(attempt, lease, index, candidate, stop)
            .await
        {
            Ok(Some(path)) => path,
            Ok(None) => return Ok(ExpiredCleanupOutcome::Missing),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    task_id = %attempt.task_id,
                    attempt_id = %attempt.attempt,
                    index,
                    "expired cleanup refused a prepared candidate before submitting its delete"
                );
                return Ok(ExpiredCleanupOutcome::Refused);
            }
        };
        let deletion = self.forge.core.object_store.delete(&path);
        tokio::pin!(deletion);
        let deletion = tokio::select! {
            result = &mut deletion => result,
            () = stop.cancelled() => return Ok(ExpiredCleanupOutcome::Uncertain),
        };
        match deletion {
            Ok(()) => {
                // Counted at the delete boundary, after the fresh `Present`
                // proof, `Eligible` classification, and fence renewal that
                // `prove_cleanup_candidate` performed. Candidate settlement can
                // still fail; recovery then observes the object already absent
                // and adds nothing, so this object is counted exactly once.
                ForgeTelemetry::record_deleted_objects(ForgeTaskStrategy::ExpiredCleanup, 1);
                Ok(ExpiredCleanupOutcome::Deleted)
            }
            Err(error) if error.kind() == opendal::ErrorKind::NotFound => {
                Ok(ExpiredCleanupOutcome::Missing)
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    task_id = %attempt.task_id,
                    index,
                    "expired cleanup deletion acceptance is unknown"
                );
                Ok(ExpiredCleanupOutcome::Uncertain)
            }
        }
    }

    /// Reports the completed durable expired-cleanup obligation at its owner boundary.
    fn record_expired_cleanup_completion(started: Instant) {
        tracing::debug!(
            elapsed_seconds = started.elapsed().as_secs_f64(),
            "Forge expired cleanup obligation completed"
        );
    }

    /// Resumes one taken-over expired-cleanup task from its durable frontier.
    ///
    /// Takeover retains the attempt generation, so the candidate a previous
    /// owner prepared is still this attempt's to settle: it is not re-prepared,
    /// its fresh proof and delete are simply retried, and the same acceptance
    /// rules decide whether the frontier moves.
    ///
    /// # Errors
    ///
    /// Returns payload-decoding, cursor, protection, object-store, fencing,
    /// audit, SQL, or cancellation failures from the shared drain owner.
    async fn resume_expired_cleanup(
        &self,
        cleanup: CleanupAttempt<'_>,
        lease: &mut ForgeLease,
        plan: &vala_sql::row_types::forge_tasks::ForgeTaskPlan,
        table: &Table,
        evidence: &ForgeTaskEvidence,
        stop: &CancellationToken,
    ) -> Result<ForgeTaskEvidence, ForgeError> {
        let payload = plan
            .expired_cleanup_payload(ForgeTaskStrategy::ExpiredCleanup, true)
            .map_err(ForgeError::Sql)?;
        let resume = CleanupResume {
            frontier: evidence.deleted_candidate_count,
            prepared: evidence.prepared_candidate_index.is_some(),
        };
        self.drain_expired_cleanup(&cleanup, lease, &payload, table, resume, stop)
            .await
    }

    /// Starts coordinated task-claim and table-lease renewal for one operation.
    ///
    /// Either authority loss cancels BOTH the shutdown-sensitive
    /// `operation_stop` and the authority-only `authority_stop` tokens, so a
    /// maintenance strategy observing `authority_stop` still aborts on genuine
    /// fence loss even though graceful shutdown never reaches that token. The
    /// task heartbeat also renews the claim's durable reservation, while the
    /// cloned table lease retains the same publication fence
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
        // A production build never resolves this trigger: nothing outside the
        // test-support observer holds a handle to it, so the extra select arm
        // is inert and the loop keeps its interval-only behavior.
        #[cfg(feature = "test-support")]
        let beat_now = self.completion_observer.as_ref().map_or_else(
            || Arc::new(tokio::sync::Notify::new()),
            ForgeWorkerCompletionObserver::heartbeat_beat_signal,
        );
        #[cfg(not(feature = "test-support"))]
        let beat_now = Arc::new(tokio::sync::Notify::new());
        Ok(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ticker.tick().await;
            loop {
                let beat = tokio::select! {
                    biased;
                    () = beat_now.notified() => true,
                    () = operation_stop.cancelled() => false,
                    _ = ticker.tick() => true,
                };
                if !beat {
                    return Ok(());
                }
                if let Err(error) = tasks
                    .heartbeat(task_id, attempt, owner, lease_seconds)
                    .await
                {
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
        self.forge.committed_evidence(binding, table).await
    }
}

impl Forge {
    /// Reads the exact committed snapshot, metadata location, and raw-byte digest.
    ///
    /// Every strategy that produces a durable Iceberg commit records the same
    /// three facts, so the owner of the shared object store and staging
    /// operator owns the read rather than each caller repeating it.
    ///
    /// # Errors
    ///
    /// Returns missing-location, path-binding, object read, or snapshot errors.
    /// Read/hash failure leaves the task Running for recovery and never stores a
    /// fabricated digest.
    pub(super) async fn committed_evidence(
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
            &self.core.staging,
            &location,
        )?;
        let raw = self
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
            prepared_candidate_index: None,
        };
        evidence.validate(false).map_err(ForgeError::Sql)?;
        Ok(evidence)
    }
}

impl ForgeWorker {
    /// Locates the earliest retained metadata object whose current snapshot
    /// carries `identity` under the snapshot property `key`.
    ///
    /// The key is a parameter because the two questions this answers are not
    /// the same question. Task recovery asks whether *this task* left an
    /// effect, and has no operation to name yet. A publication asks whether
    /// *this exact operation* landed, and must not be answered by a sibling
    /// plan of the same task that published its own snapshot — which is
    /// precisely what per-plan publication makes possible.
    ///
    /// The metadata log is searched oldest-to-newest before the current
    /// location so a later property-only catalog commit cannot replace the
    /// original raw-byte evidence. Search is bounded by the configured
    /// retained-snapshot ceiling and every candidate is table-path validated.
    ///
    /// # Errors
    ///
    /// Returns catalog, object-read, path-binding, malformed metadata, or
    /// retention errors. A failure leaves the claim nonterminal and never
    /// fabricates evidence.
    async fn find_retained_task_evidence(
        forge: &Forge,
        binding: &TenantTableBinding,
        table: &Table,
        key: &str,
        identity: Uuid,
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
        let max_locations = forge
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
                &forge.core.staging,
                &location,
            )?;
            let raw = forge
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
            if snapshot.summary().additional_properties.get(key) != Some(&identity.to_string()) {
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
                prepared_candidate_index: None,
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
        tracing::debug!(
            progressed = matches!(progress_effect, TaskProgressEffect::Progressed),
            "Forge task settled its durable progress effect"
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
        transition: ForgeTaskTransition,
        tenant: DataTenantId,
        table_ref: &ForgeTaskTableIdentity,
        lease: &ForgeLease,
        progress_effect: TaskProgressEffect,
    ) -> Result<(), ForgeError> {
        let task_id = transition.task_id;
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
                transition,
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
        tracing::debug!(
            progressed = matches!(progress_effect, TaskProgressEffect::Progressed),
            "Forge task settled its durable progress effect"
        );
        Ok(())
    }

    /// Terminally audits a malformed or unsupported claim before external effects.
    ///
    /// Known payloads acquire data-refusal qualification in this same Claimed to
    /// Failed transaction. The committed failure is counted before replan IO.
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
        if Self::metric_strategy(&claim.strategy).is_some() {
            self.tasks
                .qualify_terminal_failure(
                    &mut conn,
                    claim.task_id,
                    ForgeFailureClass::DataRefusal.as_str(),
                )
                .await
                .map_err(ForgeError::Sql)?;
        }
        conn.commit().await.map_err(ForgeError::Sql)?;
        Self::record_settled_failure(claim, ForgeFailureClass::DataRefusal);
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
                Self::record_settled_failure(claim, ForgeFailureClass::CapacityRefused);
                Ok(())
            }
            ForgeError::ShutdownRetained => Ok(()),
            _ => {
                let class = error.failure_class();
                let attempts = self
                    .tasks
                    .attempt_count(claim.task_id)
                    .await
                    .map_err(ForgeError::Sql)?;
                if failure_is_terminal(class, attempts) {
                    self.terminal_failure(claim, attempt, class, error.to_string())
                        .await?;
                } else {
                    self.tasks
                        .retry_failure(claim.task_id, attempt, self.owner, class.as_str())
                        .await
                        .map(|_| ())
                        .map_err(ForgeError::Sql)?;
                }
                // Emitted after the retry or terminal transaction commits, so
                // the counter never claims a failure the durable row does not
                // hold, and exactly once per settled attempt.
                Self::record_settled_failure(claim, class);
                Ok(())
            }
        }
    }

    /// Counts one settled durable failure under its exact class.
    ///
    /// Called only after the corresponding retry, refusal, or terminal-failure
    /// transaction commits. A claim whose strategy this build does not know
    /// carries no `task_type`, so it is observed by the durable row instead.
    fn record_settled_failure(claim: &ForgeTaskClaim, class: ForgeFailureClass) {
        if let Some(task_type) = Self::metric_strategy(&claim.strategy) {
            ForgeTelemetry::record_task_failure(task_type, class);
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
            .qualify_terminal_failure(&mut conn, claim.task_id, class.as_str())
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
    /// Returns tenant transaction, exact lifecycle, audit, release, demand,
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
    /// Both paths share the same fair-claim transaction, so tenancy admission
    /// is identical.
    ///
    /// # Errors
    ///
    /// Returns the underlying fair-claim SQL errors.
    async fn claim_next(
        &self,
        limits: ForgeClaimLimits,
        reserved_maintenance: bool,
    ) -> Result<Option<ForgeTaskClaim>, vala_sql::SqlError> {
        if reserved_maintenance
            && let Some(claim) = self
                .tasks
                .claim_fair(self.owner, limits, Some(MAINTENANCE_STRATEGIES))
                .await?
        {
            return Ok(Some(claim));
        }
        if self
            .loop_handles
            .as_ref()
            .is_some_and(|controls| controls.stop.is_cancelled())
        {
            return Ok(None);
        }
        self.tasks.claim_fair(self.owner, limits, None).await
    }

    /// Builds the positive atomic claim limits used by every fair claim.
    ///
    /// # Errors
    ///
    /// Returns invalid configuration when the claim TTL exceeds `u32`.
    fn claim_limits(&self) -> Result<ForgeClaimLimits, ForgeError> {
        Ok(ForgeClaimLimits {
            max_active_per_tenant: u32::try_from(self.config.per_tenant_active_cap)
                .unwrap_or(u32::MAX),
            lease_seconds: u32::try_from(self.forge.core.config.lease_ttl.as_secs()).map_err(
                |_| ForgeError::InvalidConfig {
                    detail: "Forge claim TTL exceeds u32 seconds".to_owned(),
                },
            )?,
        })
    }
}

#[cfg(feature = "test-support")]
impl Forge {
    /// Arms one supervised failure after maintenance reaches durable Prepared state.
    pub fn fail_after_maintenance_prepared_for_test(&self) {
        self.core
            .fail_after_maintenance_prepared
            .store(true, Ordering::Release);
    }
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
/// Builds the tenant-outbox event for one expired-cleanup candidate transition.
///
/// The operation string is the only thing that varies, and the durable owner
/// re-validates it against the transition it is about to perform, so a
/// mislabelled event is refused rather than recorded.
pub(super) fn cleanup_event(task_id: Uuid, operation: &str) -> AuditEvent {
    AuditEvent::new(
        RequestId::now_v7(),
        None,
        operation.to_owned(),
        format!("forge-task:{task_id}"),
        None,
        PrincipalId::new(Uuid::nil()),
        PrincipalKindTag::Service,
        AuthMethod::Internal,
        "bifrost:forge".to_owned(),
        AuditDecision::Allow,
        AuditResult::Success,
        "expired cleanup candidate transition".to_owned(),
    )
}

pub(super) fn task_event(task_id: Uuid, state: ForgeTaskState, reason: &str) -> AuditEvent {
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

/// Clears the Forge worker readiness bit when its supervised loop stops.
///
/// Held for the whole `run` body so quarantine, registration failure, a slot
/// failure, and an unwind all clear readiness on the same path.
struct ForgeWorkerReadinessGuard(super::ForgeRoleReadiness);

impl Drop for ForgeWorkerReadinessGuard {
    fn drop(&mut self) {
        self.0.publish(false);
    }
}

#[cfg(test)]
mod tests {
    use vala_sql::row_types::forge_tasks::{
        FORGE_TASK_PAYLOAD_VERSION, ForgeTaskEstimates, ForgeTaskPlan, ForgeTaskTableIdentity,
    };

    use super::*;

    /// The per-plan reducer applies exactly the documented priority.
    ///
    /// Success outranks every sibling outcome, the lowest plan index decides an
    /// unpublished attempt regardless of completion order, and a refusal
    /// decides only when nothing was admitted — with the lowest refused index
    /// separating a non-consuming capacity refusal from an invariant violation.
    ///
    /// # Panics
    ///
    /// Panics when any of those priorities is not the one applied.
    #[test]
    fn plan_reduction_applies_the_documented_priority() {
        use super::super::managed::queue::ForgePushResult;

        let published = ForgeWorker::reduce_plan_outcomes(
            vec![(2, ForgePushResult::RejectedCapacity)],
            vec![
                (
                    0,
                    Err(ForgeError::Invariant {
                        detail: "lower sibling failed".to_owned(),
                    }),
                ),
                (1, Ok(ForgeDispatchResult::SelfSettled)),
            ],
            Vec::new(),
        );
        assert!(
            matches!(published, Ok(ForgeDispatchResult::SelfSettled)),
            "one publication makes the task successful whatever its siblings did"
        );

        let failed = ForgeWorker::reduce_plan_outcomes(
            Vec::new(),
            vec![
                (
                    3,
                    Err(ForgeError::Invariant {
                        detail: "arrived first".to_owned(),
                    }),
                ),
                (
                    1,
                    Err(ForgeError::Capacity {
                        detail: "lowest index".to_owned(),
                    }),
                ),
            ],
            Vec::new(),
        );
        assert!(
            matches!(failed, Err(ForgeError::Capacity { .. })),
            "the lowest plan index decides, never the first completion to arrive"
        );

        let refused = ForgeWorker::reduce_plan_outcomes(
            vec![
                (0, ForgePushResult::RejectedCapacity),
                (1, ForgePushResult::RejectedDuplicate),
            ],
            Vec::new(),
            Vec::new(),
        );
        assert!(
            matches!(refused, Err(ForgeError::Capacity { .. })),
            "a capacity refusal below every invariant refusal stays non-consuming"
        );

        let invariant = ForgeWorker::reduce_plan_outcomes(
            vec![
                (0, ForgePushResult::RejectedInvalidParallelism),
                (1, ForgePushResult::RejectedCapacity),
            ],
            Vec::new(),
            Vec::new(),
        );
        assert!(
            matches!(invariant, Err(ForgeError::Invariant { .. })),
            "an invariant refusal below the capacity one is still reported as one"
        );
    }

    /// A pending-capacity refusal ends the offer pass instead of skipping ahead.
    ///
    /// The waiting budget is worker-wide, so the plan that did not fit it is
    /// blocking a position, not failing on its own terms. Offering the next
    /// plan anyway would let a smaller later sibling take the slot the blocked
    /// head needs and start ahead of it, which is the head-of-line bypass the
    /// FIFO exists to prevent.
    ///
    /// # Panics
    ///
    /// Panics when a plan after the refused one is offered, or when the refusal
    /// is not recorded against exactly the plan that did not fit.
    #[test]
    fn pending_capacity_refusal_stops_offer_pass() {
        use super::super::managed::queue::{
            ForgeCompactionQueue, ForgePlanAdmission, ForgePushResult,
        };

        let task_id = Uuid::now_v7();
        let admission = |plan_index: usize, required_parallelism: u32| ForgePlanAdmission {
            task_id,
            plan_index,
            required_parallelism,
            memory_reservation_bytes: 1,
        };
        let mut queue = ForgeCompactionQueue::new(8, 12, 1 << 20);
        let refusals = ForgeWorker::offer_planned_rewrites(
            &mut queue,
            [
                (admission(0, 8), None),
                (admission(1, 8), None),
                (admission(2, 4), None),
            ],
        );

        assert_eq!(
            refusals,
            vec![(1, ForgePushResult::RejectedCapacity)],
            "only the plan that did not fit is refused, and no later plan is offered"
        );
        assert_eq!(
            queue.waiting_parallelism_sum(),
            8,
            "the plan behind the refused one never took the blocked head's slot"
        );
    }

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

    /// Raw metadata evidence hashes exact bytes and uses lowercase encoding.
    #[test]
    fn raw_metadata_digest_contract_is_exact() {
        let raw = br#"{"format-version":2,"current-snapshot-id":7}"#;
        assert_eq!(
            format!("sha256:{}", hex::encode(Sha256::digest(raw))),
            "sha256:6467490277052fc1bcd80842434ed2ea67de45224d5adb4a6c50b7fc5a0ce727"
        );
    }

    /// Every worker bound is a hard invariant resolved once at composition.
    ///
    /// Each one, violated, makes the worker unable to do the thing it exists
    /// for: a zero tenant cap claims nothing, a zero memory budget admits no
    /// plan, zero running parallelism runs no plan, and a waiting budget below
    /// running parallelism cannot even hold one maximally parallel plan.
    ///
    /// # Panics
    ///
    /// Panics when any of those is accepted or when the defaults change.
    #[test]
    fn worker_admission_bounds_must_be_positive_and_ordered() {
        let base = ForgeWorkerConfig::default();
        for invalid in [
            ForgeWorkerConfig {
                per_tenant_active_cap: 0,
                ..base
            },
            ForgeWorkerConfig {
                compaction_memory_budget_bytes: 0,
                ..base
            },
            ForgeWorkerConfig {
                max_task_parallelism: 0,
                ..base
            },
            ForgeWorkerConfig {
                max_task_parallelism: 8,
                pending_task_parallelism: 4,
                ..base
            },
        ] {
            assert!(
                invalid.validate().is_err(),
                "an unusable worker bound must be refused: {invalid:?}"
            );
        }
        assert_eq!(base.per_tenant_active_cap, 1);
        assert!(base.validate().is_ok(), "the compiled defaults are usable");
    }

    /// Snapshot-expiry intent decodes only the canonical four-field plan.
    ///
    /// The canonical row carries the scheduler's due flags. No two-field row
    /// was ever shipped, so accepting one would be an invented compatibility
    /// route, and a row filed under any other strategy must not decode because
    /// the intent is what authorizes the retention pass.
    #[test]
    fn snapshot_expiry_intent_routes_canonical_rows_exactly() {
        let canonical = serde_json::json!({
            "kind": "maintenance",
            "trigger_commit_count": 0,
            "snapshot_expiry_due": true,
            "reconciliation_due": false,
        });
        assert_eq!(
            ForgeSnapshotExpiryIntent::parse(
                &ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry),
                canonical.as_object().expect("canonical parameters"),
            ),
            Some(ForgeSnapshotExpiryIntent {
                snapshot_expiry_due: true,
            })
        );

        let reconciliation_only = serde_json::json!({
            "kind": "maintenance",
            "trigger_commit_count": 0,
            "snapshot_expiry_due": false,
            "reconciliation_due": true,
        });
        assert_eq!(
            ForgeSnapshotExpiryIntent::parse(
                &ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry),
                reconciliation_only
                    .as_object()
                    .expect("reconciliation parameters"),
            ),
            Some(ForgeSnapshotExpiryIntent {
                snapshot_expiry_due: false,
            })
        );

        let two_field = serde_json::json!({
            "kind": "maintenance",
            "trigger_commit_count": 7,
        });
        assert_eq!(
            ForgeSnapshotExpiryIntent::parse(
                &ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry),
                two_field.as_object().expect("two-field expiry parameters"),
            ),
            None,
            "an unshipped two-field payload carries no retention authority"
        );
        assert!(
            ForgeSnapshotExpiryIntent::parse(
                &ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles),
                canonical.as_object().expect("canonical parameters"),
            )
            .is_none(),
            "only a snapshot-expiry row may carry retention authority"
        );
    }

    /// Builds one claimed task carrying `strategy` and its own valid payload.
    ///
    /// Every field is the shape the claim transaction would have produced, so a
    /// refusal this returns comes from the validation under test rather than
    /// from a malformed fixture.
    fn claimed_task(strategy: ForgeTaskStrategy, parameters: Value) -> ForgeTaskClaim {
        let tenant = DataTenantId::new(Uuid::now_v7()).expect("a fixture tenant identity");
        ForgeTaskClaim {
            execution_tenant_id: tenant,
            task_id: Uuid::now_v7(),
            data_tenant_id: tenant,
            table_ref: ForgeTaskTableIdentity {
                catalog: "bifrost".to_owned(),
                namespace: "observations".to_owned(),
                table: "activation".to_owned(),
            },
            strategy: ForgeClaimStrategy::Known(strategy),
            base_snapshot_id: 1,
            plan: ForgeTaskPlan {
                version: FORGE_TASK_PAYLOAD_VERSION,
                inputs: vec!["data/one.parquet".to_owned()],
                parameters,
            },
            estimates: ForgeTaskEstimates { files: 1, bytes: 1 },
            state: ForgeTaskState::Claimed,
            attempt_id: Some(Uuid::now_v7()),
            claimed_by: Some(Uuid::now_v7()),
            claim_expires_at: None,
            watermark: None,
            evidence: None,
            attempt_count: 0,
            failure_class: None,
            next_eligible_at: chrono::Utc::now(),
            ready_at: chrono::Utc::now(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// The activation boundary admits every production route's exact payload.
    ///
    /// Each closed strategy now has a reachable production owner, so the gate's
    /// job is no longer refusal but payload fidelity: a row whose parameters do
    /// not match its strategy contract is still refused before any catalog,
    /// object store, or durable effect, and that refusal must come from the
    /// payload contract rather than from activation.
    #[test]
    fn forge_activation_boundary_admits_every_production_payload() {
        ForgeWorker::validate_payload(&claimed_task(
            ForgeTaskStrategy::SnapshotExpiry,
            serde_json::json!({
                "kind": "maintenance",
                "trigger_commit_count": 0,
                "snapshot_expiry_due": true,
                "reconciliation_due": false,
            }),
        ))
        .expect("snapshot expiration is an activated production route");

        ForgeWorker::validate_payload(&claimed_task(
            ForgeTaskStrategy::SmallFiles,
            serde_json::json!({ "kind": LIVE_REWRITE_PARAMETER_KIND }),
        ))
        .expect("live rewrite is an activated production route");
        let mislabelled = ForgeWorker::validate_payload(&claimed_task(
            ForgeTaskStrategy::SmallFiles,
            serde_json::json!({ "kind": "maintenance" }),
        ))
        .expect_err("a rewrite row describing another workflow is still refused");
        assert!(
            !mislabelled
                .to_string()
                .contains("not activated in this phase"),
            "the refusal must come from the payload, not the activation boundary, \
             saw {mislabelled}"
        );
    }

    /// The per-tenant cap is independent of local compaction parallelism.
    ///
    /// The D78 fairness bound governs durable claims; the queue's parallelism
    /// governs how many plans of one claim run at once. Proving they validate
    /// in either relative direction is what keeps a deployment from silently
    /// re-coupling them the way the deleted executor count did.
    ///
    /// # Panics
    ///
    /// Panics when either combination is refused or read back changed.
    #[test]
    fn per_tenant_active_cap_is_independent_of_compaction_parallelism() {
        let base = ForgeWorkerConfig::default();
        let wider = ForgeWorkerConfig {
            per_tenant_active_cap: 8,
            max_task_parallelism: 2,
            pending_task_parallelism: 8,
            ..base
        }
        .validate()
        .expect("a per-tenant cap above running parallelism is legal");
        assert_eq!(wider.per_tenant_active_cap, 8);
        assert_eq!(wider.max_task_parallelism, 2);

        let narrower = ForgeWorkerConfig {
            per_tenant_active_cap: 1,
            max_task_parallelism: 8,
            pending_task_parallelism: 32,
            ..base
        }
        .validate()
        .expect("a per-tenant cap below running parallelism is legal");
        assert_eq!(narrower.per_tenant_active_cap, 1);
    }

    /// Completion observation retains an event recorded before the waiter starts.
    #[cfg(feature = "test-support")]
    #[tokio::test]
    async fn completion_observer_preserves_early_completion() {
        let observer = ForgeWorkerCompletionObserver::new();
        observer.record(
            Uuid::now_v7(),
            Uuid::now_v7(),
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles),
        );
        observer.wait_for_at_least(1).await;
        assert_eq!(observer.completed(), 1);
    }

    /// A completion between waiter registration and its counter read wakes the waiter.
    #[cfg(feature = "test-support")]
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
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles),
        );
        release.wait().await;
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("registered completion waiter wakes")
            .expect("completion waiter task joins");
        assert_eq!(observer.completed(), 1);
    }
}

/// Maps one durable `(state, failure_class)` pair onto its public result.
///
/// `Prepared` is the uncertainty handoff: work was externally accepted and its
/// evidence is retained for recovery. A refusal is separated from an ordinary
/// retry or terminal failure by the durable class, because an operator paging
/// on failures needs capacity and data refusals to read differently. States a
/// worker never settles into produce no observation at all.
fn durable_task_result(
    observation: (ForgeTaskState, Option<ForgeFailureClass>),
) -> Option<ForgeTaskResult> {
    let (state, class) = observation;
    match state {
        ForgeTaskState::Succeeded => Some(ForgeTaskResult::Succeeded),
        ForgeTaskState::Cancelled => Some(ForgeTaskResult::Cancelled),
        ForgeTaskState::Prepared => Some(ForgeTaskResult::Uncertain),
        ForgeTaskState::Retryable => Some(match class {
            Some(ForgeFailureClass::CapacityRefused) => ForgeTaskResult::Refused,
            _ => ForgeTaskResult::Retry,
        }),
        ForgeTaskState::Failed => Some(match class {
            Some(ForgeFailureClass::DataRefusal | ForgeFailureClass::CapacityRefused) => {
                ForgeTaskResult::Refused
            }
            _ => ForgeTaskResult::Failed,
        }),
        ForgeTaskState::Ready | ForgeTaskState::Claimed | ForgeTaskState::Running => None,
    }
}

/// Closed execution-stage inventory one validated Forge payload routes to.
///
/// The stage names which owner runs the claim and appears in that attempt's
/// structured trace. It is protocol detail, not a public metric label: the
/// public catalog partitions work by durable `task_type` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ForgeExecutionStage {
    /// Promote already-published Scribe hot objects into the table unchanged.
    ScribePromotion,
    /// Replace one current-snapshot group.
    IcebergRewrite,
    /// Reconcile and expire old snapshots.
    SnapshotExpiry,
    /// Delete the exact objects one committed expiration made unreachable.
    ExpiredCleanup,
    /// Reconcile and remove proven orphan objects.
    OrphanGc,
}

/// Reports the exact volume one accepted small-file rewrite moved.
///
/// The counts come from the same cross-checked descriptor lists the commit
/// submitted and the snapshot properties record, so a later recovery reading
/// those properties reports the identical numbers.
fn rewrite_volume(request: &super::publication::RewriteCommitRequest) -> ForgeCommittedVolume {
    ForgeCommittedVolume {
        input_files: request.removed_data_files.len() as u64,
        input_bytes: request
            .removed_data_files
            .iter()
            .map(iceberg::spec::DataFile::file_size_in_bytes)
            .sum(),
        output_files: request.added_data_files.len() as u64,
        output_bytes: request
            .added_data_files
            .iter()
            .map(iceberg::spec::DataFile::file_size_in_bytes)
            .sum(),
    }
}
