//! Process-local admission, retention, and lookup of accepted Workflow runs.
//!
//! [`WorkflowRuns`] is the one owner of this process's run state. A single
//! short-held lock guards the run table; nothing under it awaits or performs
//! IO. A create first [`WorkflowRuns::admit`]s its scoped idempotency key,
//! which either replays an accepted run, joins a matching preparation, or
//! installs a [`Reservation`] holding one tenant and one global active slot
//! and a token of the tracker shutdown drains, so no reservation is ever
//! visible without tracked ownership.
//! The reservation's tracked preparation then either
//! [`Reservation::accept`]s the run, transferring that slot to the queued run
//! and its key, or releases both exactly once by failing or being dropped.
//! The executor reports transitions and its terminal snapshot through the
//! returned [`AcceptedRun`]. Terminal runs are retained for a fixed 24 hours
//! within the configured retained-run ceilings.

use std::collections::HashMap;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use blake3::Hash;
#[cfg(feature = "test-support")]
use tokio::sync::Notify;
use tokio::sync::watch::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time::Instant as TokioInstant;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tokio_util::task::task_tracker::TaskTrackerToken;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::card::workflow::WorkflowRun;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{IdempotencyKey, WorkflowRunId};

use crate::config::ServerWorkflowConfig;

/// How long a terminal run stays retrievable.
const RETENTION: Duration = Duration::from_hours(24);

/// Outcome a preparation publishes to its creator and matching waiters: the
/// queued snapshot on acceptance, or the structured preparation failure.
pub(crate) type PreparationOutcome = Result<WorkflowRun, WyrdError>;

/// Idempotency scope of one create: verified tenant, effective principal,
/// and the caller's key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RunKey {
    /// Verified tenant of the caller.
    pub(crate) tenant: DataTenantId,
    /// Effective principal of the caller.
    pub(crate) principal: PrincipalId,
    /// Caller-chosen `Idempotency-Key`.
    pub(crate) key: IdempotencyKey,
}

/// What [`WorkflowRuns::admit`] decided for one create.
pub(crate) enum Admission {
    /// The key names an accepted run with the same request; its current
    /// snapshot is the replay answer.
    Replay(Box<WorkflowRun>),
    /// A preparation of the same request is in flight; its outcome is the
    /// answer.
    Wait(Receiver<Option<PreparationOutcome>>),
    /// This create owns a new preparation.
    Reserved(Reservation),
}

/// State of one scoped idempotency key.
enum KeyState {
    /// A preparation owns the key and one active slot.
    Preparing {
        /// BLAKE3 hash of the canonical request.
        hash: Hash,
        /// Outcome the preparation publishes.
        outcome: Receiver<Option<PreparationOutcome>>,
    },
    /// The key names an accepted run.
    Accepted {
        /// BLAKE3 hash of the canonical request.
        hash: Hash,
        /// The accepted run.
        run_id: WorkflowRunId,
    },
}

/// One accepted run.
struct RunEntry {
    /// The key that created the run; its tenant and principal own the run.
    key: RunKey,
    /// The current complete snapshot, replaced whole on each transition.
    snapshot: Sender<WorkflowRun>,
    /// Cancels the run's preparation, execution, and tool owners.
    cancel: CancellationToken,
    /// When the terminal snapshot was committed; `None` while active.
    terminal_at: Option<Instant>,
}

/// Everything the run-state lock guards.
#[derive(Default)]
struct RunTable {
    /// Accepted runs by id.
    runs: HashMap<WorkflowRunId, RunEntry>,
    /// Scoped idempotency keys.
    keys: HashMap<RunKey, KeyState>,
    /// Preparing and active runs across every tenant.
    active: usize,
    /// Preparing and active runs by tenant.
    active_by_tenant: HashMap<DataTenantId, usize>,
    /// Test-only offset added to the retention clock.
    #[cfg(feature = "test-support")]
    clock_offset: Duration,
}

impl RunTable {
    /// The retention clock.
    fn now(&self) -> Instant {
        let now = Instant::now();
        #[cfg(feature = "test-support")]
        let now = now + self.clock_offset;
        now
    }

    /// Release one active slot of `tenant`.
    fn release(&mut self, tenant: DataTenantId) {
        self.active = self.active.saturating_sub(1);
        if let Some(count) = self.active_by_tenant.get_mut(&tenant) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.active_by_tenant.remove(&tenant);
            }
        }
    }

    /// Remove one run and the key that created it.
    fn evict(&mut self, run_id: WorkflowRunId) {
        if let Some(entry) = self.runs.remove(&run_id) {
            self.keys.remove(&entry.key);
        }
    }

    /// The oldest terminal run, of `tenant` when one is named.
    fn oldest_terminal(&self, tenant: Option<DataTenantId>) -> Option<WorkflowRunId> {
        self.runs
            .iter()
            .filter(|(_, entry)| tenant.is_none_or(|tenant| entry.key.tenant == tenant))
            .filter_map(|(run_id, entry)| entry.terminal_at.map(|at| (at, *run_id)))
            .min()
            .map(|(_, run_id)| run_id)
    }

    /// Terminal runs, of `tenant` when one is named.
    fn terminal_count(&self, tenant: Option<DataTenantId>) -> usize {
        self.runs
            .values()
            .filter(|entry| tenant.is_none_or(|tenant| entry.key.tenant == tenant))
            .filter(|entry| entry.terminal_at.is_some())
            .count()
    }

    /// Expire terminal runs past retention, then evict the oldest terminal
    /// runs of each tenant over `per_tenant` and globally over `global`.
    ///
    /// Active runs are never eligible.
    fn sweep(&mut self, per_tenant: usize, global: usize) {
        let now = self.now();
        let expired: Vec<WorkflowRunId> = self
            .runs
            .iter()
            .filter(|(_, entry)| entry.terminal_at.is_some_and(|at| now >= at + RETENTION))
            .map(|(run_id, _)| *run_id)
            .collect();
        for run_id in expired {
            self.evict(run_id);
        }
        let mut tenants: Vec<DataTenantId> =
            self.runs.values().map(|entry| entry.key.tenant).collect();
        tenants.sort_unstable();
        tenants.dedup();
        for tenant in tenants {
            while self.terminal_count(Some(tenant)) > per_tenant {
                let Some(run_id) = self.oldest_terminal(Some(tenant)) else {
                    break;
                };
                self.evict(run_id);
            }
        }
        while self.terminal_count(None) > global {
            let Some(run_id) = self.oldest_terminal(None) else {
                break;
            };
            self.evict(run_id);
        }
    }

    /// The run `run_id` when `tenant` and `principal` own it.
    fn owned(
        &self,
        tenant: DataTenantId,
        principal: PrincipalId,
        run_id: WorkflowRunId,
    ) -> Result<&RunEntry, WyrdError> {
        self.runs
            .get(&run_id)
            .filter(|entry| entry.key.tenant == tenant && entry.key.principal == principal)
            .ok_or_else(run_not_found)
    }
}

/// This process's accepted Workflow runs and their admission.
///
/// Built once per server from [`ServerWorkflowConfig`] and shared through
/// application state. Every preparation and executor runs on its tracker,
/// so shutdown can close admission, signal, and drain them.
pub struct WorkflowRuns {
    /// Server Workflow bounds and tenant-assigned bindings.
    config: ServerWorkflowConfig,
    /// Cancelled on shutdown; the parent of every run's token.
    admission: CancellationToken,
    /// Owns every preparation and executor task.
    tasks: TaskTracker,
    /// The run table.
    table: Mutex<RunTable>,
    /// Holds an armed preparation at its next gate pass.
    #[cfg(feature = "test-support")]
    preparation_gate: PreparationGate,
}

impl WorkflowRuns {
    /// Build an empty run owner whose admission closes with `shutdown`.
    #[must_use]
    pub fn new(config: ServerWorkflowConfig, shutdown: &CancellationToken) -> Self {
        Self {
            config,
            admission: shutdown.child_token(),
            tasks: TaskTracker::new(),
            table: Mutex::default(),
            #[cfg(feature = "test-support")]
            preparation_gate: PreparationGate::default(),
        }
    }

    /// Server Workflow bounds and bindings.
    pub(crate) fn config(&self) -> &ServerWorkflowConfig {
        &self.config
    }

    /// Lock the run table, recovering it from a poisoned lock.
    ///
    /// No code under the lock panics by design, and every mutation leaves the
    /// table consistent before it can unwind, so the guarded value stays valid.
    fn table(&self) -> MutexGuard<'_, RunTable> {
        self.table.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Decide one create for `key` with canonical request `hash`.
    ///
    /// Under the lock: expire retained runs; replay an accepted run with the
    /// same hash; join a preparation with the same hash; otherwise, while
    /// admission is open and the global and tenant active ceilings have room,
    /// install a reservation owning one slot of each and a token of the task
    /// tracker. [`Self::drain`] closes admission under the same lock, so either
    /// the reservation's token keeps the drain waiting or the create sees
    /// admission closed and installs nothing. The returned reservation's
    /// cancellation token is a child of admission, so shutdown cancels the
    /// preparation.
    ///
    /// # Errors
    /// Returns `WYRD_WORKFLOW_409_IDEMPOTENCY_CONFLICT` when the key names a
    /// different request, `WYRD_WORKFLOW_503_RUN_UNAVAILABLE` after shutdown
    /// began, and `WYRD_WORKFLOW_429_RUN_CAPACITY` when an active ceiling is
    /// full.
    pub(crate) fn admit(self: &Arc<Self>, key: RunKey, hash: Hash) -> Result<Admission, WyrdError> {
        let mut table = self.table();
        table.sweep(
            self.config.max_retained_per_tenant,
            self.config.max_retained_global,
        );
        match table.keys.get(&key) {
            Some(KeyState::Accepted {
                hash: accepted,
                run_id,
            }) if *accepted == hash => {
                let snapshot = table
                    .runs
                    .get(run_id)
                    .map(|entry| entry.snapshot.borrow().clone())
                    .ok_or_else(run_not_found)?;
                return Ok(Admission::Replay(Box::new(snapshot)));
            }
            Some(KeyState::Preparing {
                hash: preparing,
                outcome,
            }) if *preparing == hash => {
                #[cfg(feature = "test-support")]
                self.preparation_gate
                    .joined
                    .send_modify(|joined| *joined += 1);
                return Ok(Admission::Wait(outcome.clone()));
            }
            Some(_) => return Err(idempotency_conflict()),
            None => {}
        }
        if self.admission.is_cancelled() {
            return Err(run_unavailable());
        }
        let tenant_active = table
            .active_by_tenant
            .get(&key.tenant)
            .copied()
            .unwrap_or(0);
        if table.active >= self.config.max_active_global
            || tenant_active >= self.config.max_active_per_tenant
        {
            return Err(WyrdError::WorkflowRunCapacity {
                message: "the server is running its maximum number of Workflow runs".to_owned(),
                details: serde_json::json!({
                    "max_active_global": self.config.max_active_global,
                    "max_active_per_tenant": self.config.max_active_per_tenant,
                }),
            });
        }
        table.active += 1;
        *table.active_by_tenant.entry(key.tenant).or_insert(0) += 1;
        let (outcome, receiver) = watch::channel(None);
        table.keys.insert(
            key.clone(),
            KeyState::Preparing {
                hash,
                outcome: receiver,
            },
        );
        Ok(Admission::Reserved(Reservation {
            runs: Arc::clone(self),
            key,
            hash,
            outcome,
            cancel: self.admission.child_token(),
            _tracked: self.tasks.token(),
            settled: false,
        }))
    }

    /// Run `task` on the tracker that shutdown drains.
    pub(crate) fn spawn<F>(&self, task: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.tasks.spawn(task)
    }

    /// Run the CPU-bound `work` on the blocking pool, tracked by the same
    /// tracker shutdown drains.
    ///
    /// Dropping the returned handle detaches nothing: the closure stays
    /// counted until it returns, so a clean drain waits for it even after its
    /// caller was cancelled.
    pub(crate) fn spawn_blocking<F, T>(&self, work: F) -> JoinHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        self.tasks.spawn_blocking(work)
    }

    /// The current snapshot of a run `tenant` and `principal` own.
    ///
    /// # Errors
    /// Returns `WYRD_WORKFLOW_404_RUN_NOT_FOUND` for an unknown, foreign,
    /// expired, or evicted run.
    pub(crate) fn get(
        &self,
        tenant: DataTenantId,
        principal: PrincipalId,
        run_id: WorkflowRunId,
    ) -> Result<WorkflowRun, WyrdError> {
        let mut table = self.table();
        table.sweep(
            self.config.max_retained_per_tenant,
            self.config.max_retained_global,
        );
        Ok(table
            .owned(tenant, principal, run_id)?
            .snapshot
            .borrow()
            .clone())
    }

    /// Signal cancellation of a run `tenant` and `principal` own and return a
    /// receiver of its snapshots.
    ///
    /// The signal is recorded synchronously, so dropping the caller's wait
    /// does not revoke it. Cancelling a terminal run changes nothing.
    ///
    /// # Errors
    /// Returns `WYRD_WORKFLOW_404_RUN_NOT_FOUND` for an unknown, foreign,
    /// expired, or evicted run.
    pub(crate) fn cancel(
        &self,
        tenant: DataTenantId,
        principal: PrincipalId,
        run_id: WorkflowRunId,
    ) -> Result<Receiver<WorkflowRun>, WyrdError> {
        let mut table = self.table();
        table.sweep(
            self.config.max_retained_per_tenant,
            self.config.max_retained_global,
        );
        let entry = table.owned(tenant, principal, run_id)?;
        entry.cancel.cancel();
        Ok(entry.snapshot.subscribe())
    }

    /// Close admission, signal every preparation and run, and wait for their
    /// tracked tasks and reservations until `deadline`.
    ///
    /// Admission closes under the run-table lock, so a concurrent
    /// [`Self::admit`] either already holds its tracker token or refuses.
    /// Returns whether every task finished; the remaining tasks are left to
    /// process exit.
    pub async fn drain(&self, deadline: TokioInstant) -> bool {
        {
            let _table = self.table();
            self.admission.cancel();
        }
        self.tasks.close();
        tokio::time::timeout_at(deadline, self.tasks.wait())
            .await
            .is_ok()
    }

    /// Advance the retention clock by `offset`.
    #[cfg(feature = "test-support")]
    pub fn advance_clock_for_test(&self, offset: Duration) {
        self.table().clock_offset += offset;
    }

    /// The stored snapshot of `run_id`, whoever owns it; `None` once the run
    /// is evicted or expired.
    ///
    /// Readable after shutdown, since the table outlives its admission.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn snapshot_for_test(&self, run_id: WorkflowRunId) -> Option<WorkflowRun> {
        self.table()
            .runs
            .get(&run_id)
            .map(|entry| entry.snapshot.borrow().clone())
    }

    /// How many creates have joined an in-flight preparation as waiters
    /// since this owner was built.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn preparation_joins_for_test(&self) -> usize {
        *self.preparation_gate.joined.borrow()
    }

    /// Wait until `count` creates in total have joined an in-flight
    /// preparation as waiters, as [`Self::preparation_joins_for_test`]
    /// counts them.
    #[cfg(feature = "test-support")]
    pub async fn wait_preparation_joins_for_test(&self, count: usize) {
        let _ = self
            .preparation_gate
            .joined
            .subscribe()
            .wait_for(|joined| *joined >= count)
            .await;
    }

    /// Stop at the next preparation gate, until
    /// [`Self::release_preparation_for_test`].
    ///
    /// A preparation passes the gate twice: before it reads its graph, and
    /// after its run is accepted but before the run starts executing. Each
    /// armed stall stops at whichever pass comes next and is consumed there.
    #[cfg(feature = "test-support")]
    pub fn stall_next_preparation_for_test(&self) {
        self.preparation_gate.armed.store(true, Ordering::SeqCst);
    }

    /// Wait until the preparation armed by
    /// [`Self::stall_next_preparation_for_test`] has stopped.
    #[cfg(feature = "test-support")]
    pub async fn wait_preparation_stall_for_test(&self) {
        self.preparation_gate.reached.notified().await;
    }

    /// Let the stopped preparation continue.
    #[cfg(feature = "test-support")]
    pub fn release_preparation_for_test(&self) {
        self.preparation_gate.released.notify_one();
    }

    /// Stop here when a stall is armed, consuming it, until released.
    ///
    /// The preparation awaits this under its run token, so shutdown still
    /// cancels a stopped preparation.
    #[cfg(feature = "test-support")]
    pub(crate) async fn pass_preparation_gate_for_test(&self) {
        let gate = &self.preparation_gate;
        if gate.armed.swap(false, Ordering::SeqCst) {
            gate.reached.notify_one();
            gate.released.notified().await;
        }
    }
}

/// A one-shot stop for the next preparation.
///
/// Both notifications store a permit when nobody waits yet, so neither side
/// can miss the other.
#[cfg(feature = "test-support")]
#[derive(Default)]
struct PreparationGate {
    /// Whether the next preparation stops.
    armed: AtomicBool,
    /// Notified once a preparation has stopped.
    reached: Notify,
    /// Notified to let the stopped preparation continue.
    released: Notify,
    /// Creates that joined an in-flight preparation as waiters so far.
    joined: Sender<usize>,
}

/// One in-flight preparation's ownership of its key and active slot.
///
/// Exactly one of [`Self::accept`], [`Self::fail`], or drop settles it, and
/// every path wakes the creator and all matching waiters.
pub(crate) struct Reservation {
    /// The owner of the table this reservation is installed in.
    runs: Arc<WorkflowRuns>,
    /// The reserved key.
    key: RunKey,
    /// BLAKE3 hash of the canonical request.
    hash: Hash,
    /// Publishes the preparation outcome.
    outcome: Sender<Option<PreparationOutcome>>,
    /// Cancels the preparation and, once accepted, the run.
    cancel: CancellationToken,
    /// Keeps [`WorkflowRuns::drain`] waiting until the reservation settles,
    /// covering the interval before its preparation task is spawned.
    _tracked: TaskTrackerToken,
    /// Whether a settling path already ran.
    settled: bool,
}

impl Reservation {
    /// The preparation's and future run's cancellation token.
    pub(crate) fn cancel_token(&self) -> &CancellationToken {
        &self.cancel
    }

    /// A receiver of this preparation's outcome.
    pub(crate) fn outcome(&self) -> Receiver<Option<PreparationOutcome>> {
        self.outcome.subscribe()
    }

    /// Accept the run whose queued snapshot is `snapshot`.
    ///
    /// Under the lock, while admission is open, replaces the reservation with
    /// the accepted run and its key, transferring the active slot, and then
    /// publishes the snapshot to the creator and every waiter.
    ///
    /// # Errors
    /// Returns `WYRD_WORKFLOW_503_RUN_UNAVAILABLE` after shutdown began; the
    /// reservation is then released and the waiters are woken with it.
    pub(crate) fn accept(mut self, snapshot: WorkflowRun) -> Result<AcceptedRun, WyrdError> {
        let runs = Arc::clone(&self.runs);
        let mut table = runs.table();
        if runs.admission.is_cancelled() {
            drop(table);
            self.fail(run_unavailable());
            return Err(run_unavailable());
        }
        let run_id = snapshot.run_id;
        let (sender, _) = watch::channel(snapshot.clone());
        table.keys.insert(
            self.key.clone(),
            KeyState::Accepted {
                hash: self.hash,
                run_id,
            },
        );
        table.runs.insert(
            run_id,
            RunEntry {
                key: self.key.clone(),
                snapshot: sender.clone(),
                cancel: self.cancel.clone(),
                terminal_at: None,
            },
        );
        drop(table);
        self.settled = true;
        self.outcome.send_replace(Some(Ok(snapshot)));
        Ok(AcceptedRun {
            runs: Arc::clone(&self.runs),
            run_id,
            tenant: self.key.tenant,
            snapshot: sender,
        })
    }

    /// Release the reservation and publish `error` to the creator and every
    /// waiter. Nothing about the failure is retained.
    pub(crate) fn fail(mut self, error: WyrdError) {
        self.release(error);
    }

    /// Remove the key and slot once and publish `error`.
    fn release(&mut self, error: WyrdError) {
        if self.settled {
            return;
        }
        self.settled = true;
        {
            let mut table = self.runs.table();
            table.keys.remove(&self.key);
            table.release(self.key.tenant);
        }
        self.outcome.send_replace(Some(Err(error)));
    }
}

impl Drop for Reservation {
    /// Release a preparation that ended without accepting or failing, such
    /// as one cancelled by shutdown.
    fn drop(&mut self) {
        self.release(run_unavailable());
    }
}

/// The executor's handle to one accepted run's stored state.
pub(crate) struct AcceptedRun {
    /// The owner of the table the run is stored in.
    runs: Arc<WorkflowRuns>,
    /// The run.
    run_id: WorkflowRunId,
    /// The run's tenant, whose active slot it holds.
    tenant: DataTenantId,
    /// The stored snapshot.
    snapshot: Sender<WorkflowRun>,
}

impl AcceptedRun {
    /// Replace the stored snapshot with the non-terminal `run`.
    ///
    /// A terminal `run` is ignored: only [`Self::finish`] commits a terminal
    /// snapshot, after the run's tool owners have drained.
    pub(crate) fn observe(&self, run: &WorkflowRun) {
        if run.status.is_terminal() {
            return;
        }
        self.snapshot.send_if_modified(|stored| {
            if stored.status.is_terminal() {
                return false;
            }
            stored.clone_from(run);
            true
        });
    }

    /// Commit the terminal `run`, release the active slot, and apply
    /// retention.
    pub(crate) fn finish(self, run: WorkflowRun) {
        let mut table = self.runs.table();
        self.snapshot.send_if_modified(|stored| {
            if stored.status.is_terminal() {
                return false;
            }
            *stored = run;
            true
        });
        let now = table.now();
        if let Some(entry) = table.runs.get_mut(&self.run_id) {
            entry.terminal_at = Some(now);
        }
        table.release(self.tenant);
        table.sweep(
            self.runs.config.max_retained_per_tenant,
            self.runs.config.max_retained_global,
        );
    }
}

/// The common answer for every run the caller cannot see.
pub(crate) fn run_not_found() -> WyrdError {
    WyrdError::WorkflowRunNotFound {
        message: "no Workflow run with that id is visible to the caller".to_owned(),
        details: serde_json::json!({}),
    }
}

/// The answer once shutdown has closed admission.
pub(crate) fn run_unavailable() -> WyrdError {
    WyrdError::WorkflowRunUnavailable {
        message: "the server is shutting down and accepts no Workflow runs".to_owned(),
        details: serde_json::json!({}),
    }
}

/// The answer for a key reused with a different request.
fn idempotency_conflict() -> WyrdError {
    WyrdError::WorkflowIdempotencyConflict {
        message: "the Idempotency-Key was already used with a different request".to_owned(),
        details: serde_json::json!({}),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::ids::IdempotencyKey;

    use super::{Admission, RunKey, WorkflowRuns, run_unavailable};
    use crate::config::ServerWorkflowConfig;

    /// A fresh run owner whose admission closes with its own shutdown token.
    fn runs() -> Arc<WorkflowRuns> {
        Arc::new(WorkflowRuns::new(
            ServerWorkflowConfig::default(),
            &CancellationToken::new(),
        ))
    }

    /// A fresh key of one fresh tenant and principal.
    fn key() -> RunKey {
        RunKey {
            tenant: DataTenantId::new_v7(),
            principal: PrincipalId::new(uuid::Uuid::now_v7()),
            key: IdempotencyKey::new(uuid::Uuid::now_v7().to_string())
                .expect("a UUID is an opaque key"),
        }
    }

    /// A reservation whose preparation task was not spawned yet keeps drain
    /// waiting; once drain began, admission installs nothing; and dropping
    /// the reservation releases its slot, wakes its waiter with the
    /// unavailable answer exactly once, and lets drain finish.
    ///
    /// # Panics
    /// Panics if drain completes while the reservation is outstanding, a
    /// shutdown-losing create installs a key, or the release is not observed.
    #[tokio::test(start_paused = true)]
    async fn drain_waits_for_a_reservation_before_its_task_exists() {
        let runs = runs();
        let hash = blake3::hash(b"request");
        let Ok(Admission::Reserved(reservation)) = runs.admit(key(), hash) else {
            panic!("an open owner reserves a new key");
        };
        let mut outcome = reservation.outcome();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(50);
        assert!(
            !runs.drain(deadline).await,
            "drain waits for the reservation"
        );

        let refused = runs
            .admit(key(), hash)
            .err()
            .expect("admission is closed once drain began");
        assert_eq!(refused.code(), run_unavailable().code());
        assert_eq!(runs.table().keys.len(), 1);

        drop(reservation);
        let published = outcome
            .wait_for(Option::is_some)
            .await
            .expect("the reservation publishes before it closes")
            .clone()
            .expect("a published outcome is present");
        assert_eq!(
            published
                .expect_err("a released reservation is unavailable")
                .code(),
            run_unavailable().code()
        );
        {
            let table = runs.table();
            assert!(table.keys.is_empty());
            assert_eq!(table.active, 0);
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        assert!(runs.drain(deadline).await, "drain finishes once released");
    }

    /// Blocking preparation work stays counted after the task awaiting it is
    /// cancelled, so a clean drain waits until the closure returns.
    ///
    /// # Panics
    /// Panics if drain finishes while the blocking closure still runs, or does
    /// not finish once it returned.
    #[tokio::test]
    async fn drain_waits_for_blocking_work_its_caller_abandoned() {
        let runs = runs();
        let (release, held) = std::sync::mpsc::channel::<()>();
        let (started, running) = tokio::sync::oneshot::channel::<()>();
        let cancel = runs.admission.child_token();
        let owner = Arc::clone(&runs);
        runs.spawn(async move {
            let work = owner.spawn_blocking(move || {
                started.send(()).expect("the test awaits the start");
                held.recv().expect("the test releases the work");
            });
            tokio::select! {
                () = cancel.cancelled() => {}
                _ = work => {}
            }
        });
        running.await.expect("the blocking work starts");

        let deadline = tokio::time::Instant::now() + Duration::from_millis(100);
        assert!(
            !runs.drain(deadline).await,
            "drain waits for the abandoned blocking work"
        );
        release.send(()).expect("the blocking work still waits");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        assert!(runs.drain(deadline).await, "drain finishes once it returns");
    }
}
