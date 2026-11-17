//! Who owns process memory, and how competing workloads share it.
//!
//! Every substantial retained buffer is a [`super::ByteCharge`] on the one
//! process ledger; attributing it to an [`Operation`] records which workload
//! and tenant currently has custody of it. Attribution is bookkeeping over the
//! same root bytes: moving a charge between operations never charges the
//! ledger again, and a buffer shared by many readers is one charge.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use wyrd_spec::ids::DataTenantId;

tokio::task_local! {
    /// The operation the running task's work belongs to, set by
    /// [`Operation::scope`]; admissions inside it join it.
    static CURRENT: Arc<OperationShared>;
}

/// The outer workloads that share one server process, with equal weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Workload {
    /// Standalone gateway (model and tool) calls.
    Gateway,
    /// Direct and queued verification, including its nested gateway calls.
    Verification,
    /// Bifrost ingest, query and maintenance.
    Bifrost,
}

impl Workload {
    /// Every workload, in a fixed order.
    pub const ALL: [Self; 3] = [Self::Gateway, Self::Verification, Self::Bifrost];

    /// The fixed index of this workload in [`Self::ALL`].
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Gateway => 0,
            Self::Verification => 1,
            Self::Bifrost => 2,
        }
    }

    /// The closed metric label of this workload.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Gateway => "gateway",
            Self::Verification => "verification",
            Self::Bifrost => "bifrost",
        }
    }
}

/// Process-local identity of one admitted operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OperationId(u64);

/// The resource identity work carries through nested calls: the workload and
/// tenant that initiated it, and the operation that owns its custody.
///
/// A nested call made on behalf of an operation reuses its context, so a
/// verification-originated gateway call stays verification work for sharing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkContext {
    /// The workload that initiated the work.
    pub workload: Workload,
    /// The initiating tenant, when the work is attributable to one.
    pub tenant: Option<DataTenantId>,
    /// The operation owning the work's custody.
    pub operation: OperationId,
}

/// The shared state of one operation, reachable from its handle and from
/// every charge attributed to it.
#[derive(Debug)]
pub(crate) struct OperationShared {
    /// The operation's resource identity.
    pub(crate) context: WorkContext,
    /// Root bytes currently attributed to this operation.
    pub(crate) bytes: AtomicU64,
    /// Whether the operation handle is still live (the work is unsettled).
    pub(crate) live: AtomicBool,
    /// Cancels the operation when it is chosen as a pressure victim; `None`
    /// for work that may never be cancelled, such as acknowledged durable
    /// work or mandatory settlement.
    pub(crate) cancel: Option<CancellationToken>,
    /// The registry this operation is listed in and reports usage to.
    pub(crate) registry: Arc<Registry>,
}

/// Every operation that may hold attributed bytes, and their usage totals.
#[derive(Debug, Default)]
pub(crate) struct Registry {
    /// Operations by id, each removed when its last reference drops.
    operations: Mutex<HashMap<OperationId, Weak<OperationShared>>>,
    /// Attributed bytes per workload, in [`Workload::ALL`] order.
    usage: [AtomicU64; 3],
    /// Woken when usage falls, an operation settles, or pressure changes.
    changed: Notify,
}

impl OperationShared {
    /// Adds `bytes` to this operation's attribution.
    pub(crate) fn add(&self, bytes: u64) {
        self.bytes.fetch_add(bytes, Ordering::AcqRel);
        self.registry.usage[self.context.workload.index()].fetch_add(bytes, Ordering::AcqRel);
    }

    /// Removes `bytes` from this operation's attribution and wakes waiters.
    pub(crate) fn remove(&self, bytes: u64) {
        if bytes == 0 {
            return;
        }
        let previous = self.bytes.fetch_sub(bytes, Ordering::AcqRel);
        debug_assert!(previous >= bytes, "attribution underflow");
        self.registry.usage[self.context.workload.index()].fetch_sub(bytes, Ordering::AcqRel);
        self.registry.changed.notify_waiters();
    }
}

impl Drop for OperationShared {
    fn drop(&mut self) {
        self.registry
            .operations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.context.operation);
    }
}

/// One admitted unit of work that may own memory.
///
/// Dropping the handle settles the operation: it no longer counts as demand
/// or as a cancellation candidate, although charges still attributed to it
/// keep counting until they drop or move to another operation.
#[derive(Debug)]
pub struct Operation {
    /// State shared with the manager registry and attributed charges.
    pub(crate) shared: Arc<OperationShared>,
    /// Whether this handle joined an enclosing operation; dropping a joined
    /// handle leaves that operation's settlement to its own handle.
    joined: bool,
}

impl Operation {
    /// Runs `work` as part of this operation: every
    /// [`WorkloadManager::admit`] inside it, on the same manager, joins this
    /// operation instead of starting another. Its bytes then count here, and
    /// choosing this operation as a pressure victim cancels it as a whole.
    pub async fn scope<F: Future>(&self, work: F) -> F::Output {
        CURRENT.scope(Arc::clone(&self.shared), work).await
    }

    /// The pressure cancellation of the operation whose
    /// [`Self::scope`] the running task is in, when that operation is
    /// cancellable; `None` outside any scope.
    ///
    /// Work inside the scope that must settle on its own terms, rather than
    /// being dropped from outside, watches this token and stops itself.
    #[must_use]
    pub fn current_cancellation() -> Option<CancellationToken> {
        CURRENT
            .try_with(|shared| shared.cancel.clone())
            .ok()
            .flatten()
    }

    /// The resource identity of this operation.
    #[must_use]
    pub fn context(&self) -> WorkContext {
        self.shared.context
    }

    /// Root bytes currently attributed to this operation.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.shared.bytes.load(Ordering::Acquire)
    }
}

/// `used / limit`, treating an empty limit as fully used.
fn ratio(used: u64, limit: u64) -> f64 {
    if limit == 0 {
        return if used == 0 { 0.0 } else { f64::INFINITY };
    }
    used as f64 / limit as f64
}

/// Advances the contention state machine for one sample at `q`.
fn next_state(control: &mut Control, q: f64) -> PressureState {
    let previous = control.pressure.state;
    if q >= 1.0 {
        control.recovered_samples = 0;
        return PressureState::Critical;
    }
    if q >= CONTENDED || previous == PressureState::Normal {
        control.recovered_samples = 0;
        return if q >= CONTENDED {
            PressureState::Contended
        } else {
            PressureState::Normal
        };
    }
    if q <= RECOVERED {
        control.recovered_samples += 1;
        if control.recovered_samples >= RECOVERY_SAMPLES {
            control.recovered_samples = 0;
            return PressureState::Normal;
        }
    } else {
        control.recovered_samples = 0;
    }
    PressureState::Contended
}

/// Divides `total` equally among demanding workloads, redistributing what a
/// local cap below the equal share leaves unused.
fn water_fill(total: u64, demand: [bool; 3], caps: [Option<u64>; 3]) -> [Option<u64>; 3] {
    let mut targets = [None; 3];
    let mut open: Vec<usize> = (0..3).filter(|index| demand[*index]).collect();
    let mut remaining = total;
    loop {
        let Some(count) = u64::try_from(open.len()).ok().filter(|count| *count > 0) else {
            return targets;
        };
        let share = remaining / count;
        let capped: Vec<usize> = open
            .iter()
            .copied()
            .filter(|index| caps[*index].is_some_and(|cap| cap < share))
            .collect();
        if capped.is_empty() {
            for index in open {
                targets[index] = Some(share);
            }
            return targets;
        }
        for index in capped {
            let cap = caps[index].unwrap_or(share);
            targets[index] = Some(cap);
            remaining = remaining.saturating_sub(cap);
            open.retain(|open| *open != index);
        }
    }
}

/// The one cancellable operation critical pressure cancels next: the largest
/// attributable operation of the workload furthest above its target, else
/// the largest cancellable attributable operation overall. Uncancellable,
/// already cancelled, settled and empty operations are never chosen.
fn select_victim(
    control: &Control,
    usage: [u64; 3],
    operations: &[Arc<OperationShared>],
) -> Option<Arc<OperationShared>> {
    let eligible: Vec<&Arc<OperationShared>> = operations
        .iter()
        .filter(|operation| {
            operation.live.load(Ordering::Acquire)
                && operation.bytes.load(Ordering::Acquire) > 0
                && operation
                    .cancel
                    .as_ref()
                    .is_some_and(|cancel| !cancel.is_cancelled())
        })
        .collect();
    let furthest = Workload::ALL
        .into_iter()
        .filter_map(|workload| {
            let index = workload.index();
            let excess = usage[index].checked_sub(control.targets[index]?)?;
            (excess > 0
                && eligible
                    .iter()
                    .any(|operation| operation.context.workload == workload))
            .then_some((excess, workload))
        })
        .max_by_key(|(excess, _)| *excess)
        .map(|(_, workload)| workload);
    eligible
        .into_iter()
        .filter(|operation| furthest.is_none_or(|workload| operation.context.workload == workload))
        .max_by_key(|operation| operation.bytes.load(Ordering::Acquire))
        .cloned()
}

/// Publishes one sample's pressure and per-workload attribution gauges.
fn record(pressure: &Pressure, usage: [u64; 3]) {
    metrics::gauge!("wyrd_workload_ledger_bytes").set(pressure.ledger_bytes as f64);
    metrics::gauge!("wyrd_workload_committed_bytes").set(pressure.committed_bytes as f64);
    metrics::gauge!("wyrd_workload_pressure_ratio").set(pressure.ratio);
    for workload in Workload::ALL {
        metrics::gauge!("wyrd_workload_attributed_bytes", "workload" => workload.label())
            .set(usage[workload.index()] as f64);
    }
}

/// One managed worker's latest allocator commitment, kept until dropped.
///
/// A worker whose samples stop keeps its last value: losing visibility
/// never frees usage. Drop it only on confirmed process exit.
#[derive(Debug)]
pub struct CommitObservation {
    /// The latest commitment, read by every sample.
    bytes: Arc<AtomicU64>,
}

impl CommitObservation {
    /// Records the worker's latest commitment.
    pub fn set(&self, bytes: u64) {
        self.bytes.store(bytes, Ordering::Release);
    }

    /// The latest recorded commitment.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Acquire)
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        if self.joined {
            return;
        }
        self.shared.live.store(false, Ordering::Release);
        self.shared.registry.changed.notify_waiters();
    }
}

/// Where the process stands against its memory budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PressureState {
    /// Below the contention threshold: unused capacity is borrowable.
    #[default]
    Normal,
    /// Contended: work above its soft target pauses.
    Contended,
    /// At or past the budget: no new work starts.
    Critical,
}

/// One pressure observation.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pressure {
    /// Root ledger usage `L`.
    pub ledger_bytes: u64,
    /// Sampled allocator commitment `A` of the parent plus managed workers.
    pub committed_bytes: u64,
    /// `max(L / M, A / P)`.
    pub ratio: f64,
    /// The state the controller is in after this observation.
    pub state: PressureState,
}

/// The registry behind every clone of one [`WorkloadManager`].
#[derive(Debug)]
struct ManagerInner {
    /// The process memory ledger whose usage is `L` and limit is `M`.
    memory: super::ByteLedger,
    /// The process ceiling `P` covering the parent and managed workers.
    ceiling: u64,
    /// Next operation id.
    next: AtomicU64,
    /// Every operation that may still hold attributed bytes.
    registry: Arc<Registry>,
    /// The managed CPU lanes.
    lanes: super::CpuLanes,
    /// The latest commitment of every live managed worker.
    commitments: Mutex<Vec<Weak<AtomicU64>>>,
    /// Pressure state, soft targets, local caps and reclamation hooks.
    control: Mutex<Control>,
}

/// A reclamation hook run when pressure turns critical.
type Reclaim = Arc<dyn Fn() + Send + Sync>;

/// How often the process samples pressure: the controller's clock.
pub const SAMPLE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// Contention entry threshold of `q` and the admission operating target as a
/// fraction of `M`.
const CONTENDED: f64 = 0.95;
/// Contention ends after [`RECOVERY_SAMPLES`] consecutive samples at or below
/// this `q`.
const RECOVERED: f64 = 0.90;
/// Consecutive recovered samples that end contention.
const RECOVERY_SAMPLES: u8 = 2;

/// The controller state the latest sample produced.
#[derive(Default)]
struct Control {
    /// The latest observation.
    pressure: Pressure,
    /// Consecutive samples at or below [`RECOVERED`] while contended.
    recovered_samples: u8,
    /// Consecutive critical samples; the first reclaims, later ones cancel.
    critical_samples: u32,
    /// Whether allocator commitment, not ledger usage, drives contention.
    allocator_led: bool,
    /// Soft target of each demanding workload under contention.
    targets: [Option<u64>; 3],
    /// Equal soft target of each demanding tenant within a workload.
    tenant_targets: [Option<u64>; 3],
    /// Existing local ceilings that bound a workload's share.
    local_caps: [Option<u64>; 3],
    /// Hooks reclaiming supported idle memory.
    reclaim: Vec<Reclaim>,
}

impl std::fmt::Debug for Control {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Control")
            .field("pressure", &self.pressure)
            .field("targets", &self.targets)
            .field("local_caps", &self.local_caps)
            .finish_non_exhaustive()
    }
}

/// The one process workload manager: operation attribution today.
///
/// Cloning shares the manager.
#[derive(Debug, Clone)]
pub struct WorkloadManager {
    /// The shared registry.
    inner: Arc<ManagerInner>,
}

impl WorkloadManager {
    /// A manager over `memory` (usage `L`, budget `M`) inside a process
    /// ceiling of `ceiling` bytes (`P`, at least `M`), with `cpu` lanes.
    pub(crate) fn new(memory: super::ByteLedger, ceiling: u64, cpu: usize) -> Self {
        Self {
            inner: Arc::new(ManagerInner {
                lanes: super::CpuLanes::new(cpu),
                ceiling: ceiling.max(memory.limit()),
                memory,
                next: AtomicU64::new(0),
                registry: Arc::default(),
                commitments: Mutex::default(),
                control: Mutex::default(),
            }),
        }
    }

    /// Registers a new operation of `workload` for `tenant`.
    ///
    /// `cancel` makes the operation a pressure-victim candidate; pass `None`
    /// for work that must not be cancelled.
    #[must_use]
    pub fn begin(
        &self,
        workload: Workload,
        tenant: Option<DataTenantId>,
        cancel: Option<CancellationToken>,
    ) -> Operation {
        let id = OperationId(self.inner.next.fetch_add(1, Ordering::Relaxed));
        let shared = Arc::new(OperationShared {
            context: WorkContext {
                workload,
                tenant,
                operation: id,
            },
            bytes: AtomicU64::new(0),
            live: AtomicBool::new(true),
            cancel,
            registry: Arc::clone(&self.inner.registry),
        });
        self.inner
            .registry
            .operations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, Arc::downgrade(&shared));
        Operation {
            shared,
            joined: false,
        }
    }

    /// Root bytes currently attributed to `workload`'s operations.
    #[must_use]
    pub fn workload_bytes(&self, workload: Workload) -> u64 {
        self.inner.registry.usage[workload.index()].load(Ordering::Acquire)
    }

    /// The managed CPU lanes every workload's major CPU phases share.
    #[must_use]
    pub fn lanes(&self) -> &super::CpuLanes {
        &self.inner.lanes
    }

    /// Bounds `workload`'s soft target by an existing local ceiling.
    pub fn set_local_cap(&self, workload: Workload, bytes: u64) {
        self.control().local_caps[workload.index()] = Some(bytes);
    }

    /// Registers a reclamation hook run when pressure turns critical, such as
    /// idle-worker eviction. Hooks must not block.
    pub fn on_reclaim(&self, reclaim: impl Fn() + Send + Sync + 'static) {
        self.control().reclaim.push(Arc::new(reclaim));
    }

    /// A slot reporting one managed worker's allocator commitment.
    #[must_use]
    pub fn observe_commitment(&self) -> CommitObservation {
        let bytes = Arc::new(AtomicU64::new(0));
        let mut commitments = self
            .inner
            .commitments
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        commitments.retain(|slot| slot.strong_count() > 0);
        commitments.push(Arc::downgrade(&bytes));
        CommitObservation { bytes }
    }

    /// Takes one pressure sample given the parent's allocator commitment.
    ///
    /// This is the controller's only clock: contention starts at
    /// `q >= 0.95`, critical pressure at `q >= 1`, and contention ends after
    /// two consecutive samples at `q <= 0.90`. The first critical sample
    /// runs the reclamation hooks; each later one cancels at most one victim.
    /// Without a sampler the controller stays [`PressureState::Normal`].
    pub fn sample(&self, parent_committed: u64) -> Pressure {
        let ledger_bytes = self.inner.memory.used();
        let committed_bytes = self.worker_commitment().saturating_add(parent_committed);
        let ledger_ratio = ratio(ledger_bytes, self.inner.memory.limit());
        let committed_ratio = ratio(committed_bytes, self.inner.ceiling);
        let pressure_ratio = ledger_ratio.max(committed_ratio);
        let usage = Workload::ALL.map(|workload| self.workload_bytes(workload));
        let (pressure, reclaim, victim) = {
            let mut control = self.control();
            let state = next_state(&mut control, pressure_ratio);
            let operations = if state == PressureState::Normal {
                Vec::new()
            } else {
                self.live_operations()
            };
            control.allocator_led = committed_ratio >= CONTENDED && ledger_ratio < CONTENDED;
            control.pressure = Pressure {
                ledger_bytes,
                committed_bytes,
                ratio: pressure_ratio,
                state,
            };
            self.retarget(&mut control, &operations);
            let mut reclaim = Vec::new();
            let mut victim = None;
            if state == PressureState::Critical {
                if control.critical_samples == 0 {
                    reclaim.clone_from(&control.reclaim);
                } else {
                    victim = select_victim(&control, usage, &operations);
                }
                control.critical_samples += 1;
            } else {
                control.critical_samples = 0;
            }
            (control.pressure, reclaim, victim)
        };
        for hook in reclaim {
            hook();
        }
        if let Some(victim) = victim.and_then(|victim| victim.cancel.clone()) {
            victim.cancel();
        }
        record(&pressure, usage);
        self.inner.registry.changed.notify_waiters();
        pressure
    }

    /// Samples pressure every [`SAMPLE_INTERVAL`] with the parent commitment
    /// `parent_committed` reads, until the returned future is dropped.
    ///
    /// The process's one sampler: the binary that can observe its allocator
    /// supplies the reading, and an embedder that cannot passes zero, leaving
    /// the ledger as the only observation.
    pub async fn sample_every(&self, parent_committed: impl Fn() -> u64) {
        let mut ticks = tokio::time::interval(SAMPLE_INTERVAL);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticks.tick().await;
            self.sample(parent_committed());
        }
    }

    /// The latest pressure observation.
    #[must_use]
    pub fn pressure(&self) -> Pressure {
        self.control().pressure
    }

    /// The soft target of `workload` under the latest observation; `None`
    /// outside contention or when the workload was not demanding.
    #[must_use]
    pub fn target(&self, workload: Workload) -> Option<u64> {
        self.control().targets[workload.index()]
    }

    /// Admits a new operation once pressure allows it to start.
    ///
    /// The operation registers as demand immediately, then waits while its
    /// workload or tenant is above its soft target under contention, while
    /// unattributed allocator pressure pauses all growth, or while pressure
    /// is critical. Admission grants no bytes: owners still charge what they
    /// materialize. Callers bound the wait with their own deadline; dropping
    /// the future withdraws the demand.
    /// An operation that has to wait logs that once, at debug level.
    ///
    /// Inside another operation's [`Operation::scope`] on this manager, the
    /// admission joins that operation instead: it waits by the enclosing
    /// operation's workload and tenant, and `workload`, `tenant` and `cancel`
    /// are ignored, since cancelling the enclosing operation stops this work
    /// with it.
    pub async fn admit(
        &self,
        workload: Workload,
        tenant: Option<DataTenantId>,
        cancel: Option<CancellationToken>,
    ) -> Operation {
        let enclosing = CURRENT
            .try_with(Arc::clone)
            .ok()
            .filter(|shared| Arc::ptr_eq(&shared.registry, &self.inner.registry));
        let operation = match enclosing {
            Some(shared) => Operation {
                shared,
                joined: true,
            },
            None => self.begin(workload, tenant, cancel),
        };
        let WorkContext {
            workload, tenant, ..
        } = operation.context();
        let mut waited = false;
        loop {
            let mut changed = std::pin::pin!(self.inner.registry.changed.notified());
            changed.as_mut().enable();
            if !self.paused(workload, tenant) {
                return operation;
            }
            if !waited {
                waited = true;
                tracing::debug!(
                    ?workload,
                    ?tenant,
                    "new work waits for process pressure to admit it"
                );
            }
            changed.await;
        }
    }

    /// Whether new work of `workload` for `tenant` must wait now.
    fn paused(&self, workload: Workload, tenant: Option<DataTenantId>) -> bool {
        let (state, allocator_led, target, tenant_target) = {
            let control = self.control();
            (
                control.pressure.state,
                control.allocator_led,
                control.targets[workload.index()],
                control.tenant_targets[workload.index()],
            )
        };
        match state {
            PressureState::Normal => false,
            PressureState::Critical => true,
            PressureState::Contended if allocator_led => true,
            PressureState::Contended => {
                target.is_some_and(|target| self.workload_bytes(workload) > target)
                    || tenant.zip(tenant_target).is_some_and(|(tenant, target)| {
                        self.tenant_bytes(workload, tenant) > target
                    })
            }
        }
    }

    /// Recomputes soft targets for the demanding workloads and tenants.
    ///
    /// Outside contention there are no targets. Under contention the
    /// `0.95 * M` operating target is divided equally among demanding
    /// workloads, with a share bounded by a local ceiling redistributed to the
    /// others; each workload's target is divided equally among its demanding
    /// attributed tenants. A workload or tenant demands while it has a live
    /// operation or attributed bytes.
    fn retarget(&self, control: &mut Control, operations: &[Arc<OperationShared>]) {
        control.targets = [None; 3];
        control.tenant_targets = [None; 3];
        if control.pressure.state == PressureState::Normal {
            return;
        }
        let demanding = |operation: &&Arc<OperationShared>| {
            operation.live.load(Ordering::Acquire) || operation.bytes.load(Ordering::Acquire) > 0
        };
        let mut tenants: [Vec<DataTenantId>; 3] = Default::default();
        let mut demand = [false; 3];
        for operation in operations.iter().filter(demanding) {
            let index = operation.context.workload.index();
            demand[index] = true;
            if let Some(tenant) = operation.context.tenant
                && !tenants[index].contains(&tenant)
            {
                tenants[index].push(tenant);
            }
        }
        let limit = self.inner.memory.limit();
        let total = limit - limit / 20;
        control.targets = water_fill(total, demand, control.local_caps);
        for (index, target) in control.targets.iter().enumerate() {
            let count = tenants[index].len() as u64;
            control.tenant_targets[index] = target
                .filter(|_| count > 0)
                .map(|target| target / count.max(1));
        }
    }

    /// The summed latest commitment of every live managed worker.
    fn worker_commitment(&self) -> u64 {
        self.inner
            .commitments
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter_map(Weak::upgrade)
            .map(|slot| slot.load(Ordering::Acquire))
            .sum()
    }

    /// The controller state, recovered from a poisoned lock because every
    /// write leaves it consistent.
    fn control(&self) -> std::sync::MutexGuard<'_, Control> {
        self.inner
            .control
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Root bytes attributed to `tenant`'s operations of `workload`.
    ///
    /// Scans the registry, so it is consulted only under contention.
    fn tenant_bytes(&self, workload: Workload, tenant: DataTenantId) -> u64 {
        self.live_operations()
            .iter()
            .filter(|operation| {
                operation.context.workload == workload && operation.context.tenant == Some(tenant)
            })
            .map(|operation| operation.bytes.load(Ordering::Acquire))
            .sum()
    }

    /// Every operation that may still hold attributed bytes.
    fn live_operations(&self) -> Vec<Arc<OperationShared>> {
        self.inner
            .registry
            .operations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .filter_map(Weak::upgrade)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::pin;
    use std::sync::atomic::AtomicUsize;
    use std::task::{Context, Poll, Waker};

    use super::*;
    use crate::resources::ByteLedger;

    /// `M = 1000` managed bytes inside a `P = 1100` ceiling (`H = 100`).
    fn manager() -> (ByteLedger, WorkloadManager) {
        let memory = ByteLedger::new(1_000);
        (memory.clone(), WorkloadManager::new(memory, 1_100, 1))
    }

    /// A fresh tenant for attribution tests.
    fn tenant() -> DataTenantId {
        DataTenantId::new_v7()
    }

    /// Polls `future` once without a runtime.
    fn poll_once<F: Future>(future: std::pin::Pin<&mut F>) -> Poll<F::Output> {
        future.poll(&mut Context::from_waker(Waker::noop()))
    }

    /// Whether an admission of `workload` for `tenant` would start now.
    fn starts_now(
        workloads: &WorkloadManager,
        workload: Workload,
        tenant: Option<DataTenantId>,
    ) -> bool {
        let admission = pin!(workloads.admit(workload, tenant, None));
        poll_once(admission).is_ready()
    }

    /// Charges `bytes` attributed to `operation`.
    fn hold(memory: &ByteLedger, operation: &Operation, bytes: u64) -> super::super::ByteCharge {
        let mut charge = memory.charge_infallible(bytes);
        charge.attribute(operation);
        charge
    }

    /// One workload borrows idle capacity with no charged floor; at q=0.95 a
    /// borrower above its share pauses while a new contender starts, the
    /// contender changes the soft targets, q=1 stops all new work, and only
    /// two samples at q<=0.90 end contention.
    ///
    /// # Panics
    /// Panics when borrowing is refused, a share is charged, a borrower is
    /// not paused, or contention ends early.
    #[test]
    fn borrowing_contention_and_recovery_follow_samples() {
        let (memory, workloads) = manager();
        let gateway = workloads.begin(Workload::Gateway, None, None);
        let mut held = hold(&memory, &gateway, 900);
        assert_eq!(workloads.sample(0).state, PressureState::Normal);
        assert!(
            starts_now(&workloads, Workload::Gateway, None),
            "idle capacity is borrowable"
        );
        assert_eq!(memory.used(), 900, "no entitlement is charged");

        held.grow_infallible(50);
        let pressure = workloads.sample(0);
        assert_eq!(pressure.state, PressureState::Contended, "q=0.95 contends");
        held.grow_infallible(10);
        assert_eq!(workloads.target(Workload::Gateway), Some(950));
        assert!(
            !starts_now(&workloads, Workload::Gateway, None),
            "at its target the borrower pauses"
        );
        let verification = workloads.begin(Workload::Verification, None, None);
        assert!(
            starts_now(&workloads, Workload::Verification, None),
            "a below-target contender starts"
        );
        workloads.sample(0);
        assert_eq!(workloads.target(Workload::Gateway), Some(475));
        assert_eq!(workloads.target(Workload::Verification), Some(475));
        assert_eq!(
            workloads.target(Workload::Bifrost),
            None,
            "an absent workload has no share"
        );

        held.grow_infallible(40);
        assert_eq!(
            workloads.sample(0).state,
            PressureState::Critical,
            "q=1 is critical"
        );
        assert!(
            !starts_now(&workloads, Workload::Verification, None),
            "critical stops all new work"
        );

        held.shrink(100);
        assert_eq!(
            workloads.sample(0).state,
            PressureState::Contended,
            "q=0.90 once"
        );
        assert_eq!(
            workloads.sample(0).state,
            PressureState::Normal,
            "q=0.90 twice: the boundary is inclusive"
        );
        assert!(starts_now(&workloads, Workload::Gateway, None));
        drop((held, gateway, verification));
    }

    /// A local ceiling smaller than an equal share is redistributed, and
    /// demanding tenants of one workload split its target equally.
    ///
    /// A tenant held back above its share does not hold back another
    /// tenant's eligible work.
    ///
    /// # Panics
    /// Panics when the capped share is not redistributed, a tenant above its
    /// share is admitted, or its waiting admission blocks another tenant.
    #[test]
    fn local_caps_redistribute_and_tenants_share_equally() {
        let (memory, workloads) = manager();
        workloads.set_local_cap(Workload::Bifrost, 100);
        let bifrost = workloads.begin(Workload::Bifrost, None, None);
        let gateway = workloads.begin(Workload::Gateway, None, None);
        let (heavy_tenant, light_tenant) = (Some(tenant()), Some(tenant()));
        let heavy = workloads.begin(Workload::Verification, heavy_tenant, None);
        let light = workloads.begin(Workload::Verification, light_tenant, None);
        let held = [
            hold(&memory, &bifrost, 100),
            hold(&memory, &gateway, 400),
            hold(&memory, &heavy, 300),
            hold(&memory, &light, 60),
            memory.charge_infallible(100),
        ];
        assert_eq!(workloads.sample(0).state, PressureState::Contended);
        assert_eq!(workloads.target(Workload::Bifrost), Some(100));
        assert_eq!(workloads.target(Workload::Gateway), Some(425));
        assert_eq!(workloads.target(Workload::Verification), Some(425));
        assert!(
            !starts_now(&workloads, Workload::Verification, heavy_tenant),
            "212 share, 300 held"
        );
        let mut paused = pin!(workloads.admit(Workload::Verification, heavy_tenant, None));
        assert!(poll_once(paused.as_mut()).is_pending());
        assert!(
            starts_now(&workloads, Workload::Verification, light_tenant),
            "212 share, 60 held, and the paused heavy tenant does not hold it back"
        );
        assert!(
            starts_now(&workloads, Workload::Gateway, None),
            "below its target"
        );
        drop((held, bifrost, gateway, heavy, light));
    }

    /// Allocator commitment no ledger owner explains pauses all new growth,
    /// even below-target work, and is compared with the ledger rather than
    /// added to it. A worker's last commitment remains until it is dropped.
    ///
    /// # Panics
    /// Panics when unattributed pressure is bypassed, observations are
    /// added, or a silent worker's usage disappears.
    #[test]
    fn untracked_allocator_pressure_pauses_all_growth() {
        let (memory, workloads) = manager();
        let gateway = workloads.begin(Workload::Gateway, None, None);
        let held = hold(&memory, &gateway, 100);
        let worker = workloads.observe_commitment();
        worker.set(550);
        let pressure = workloads.sample(500);
        assert_eq!(pressure.ledger_bytes, 100);
        assert_eq!(pressure.committed_bytes, 1_050);
        assert!(
            (pressure.ratio - 1_050.0 / 1_100.0).abs() < 1e-9,
            "max, not a sum"
        );
        assert_eq!(pressure.state, PressureState::Contended);
        assert!(
            !starts_now(&workloads, Workload::Verification, None),
            "no bypass below target"
        );
        assert_eq!(
            workloads.sample(500).committed_bytes,
            1_050,
            "a silent worker keeps usage"
        );
        drop(worker);
        assert_eq!(
            workloads.sample(500).committed_bytes,
            500,
            "confirmed exit releases it"
        );
        drop((held, gateway));
    }

    /// Work admitted inside an operation's scope joins it: nested calls
    /// share its identity, their bytes count as its bytes, dropping a joined
    /// handle leaves it unsettled, and critical pressure cancels the
    /// enclosing operation as a whole rather than one nested call.
    ///
    /// # Panics
    /// Panics when a nested admission starts its own operation, its bytes
    /// count elsewhere, a joined handle settles the enclosing operation, or
    /// the victim is not the enclosing operation.
    #[tokio::test]
    async fn nested_admissions_join_the_enclosing_operation() {
        let (memory, workloads) = manager();
        let (execution_cancel, nested_cancel, standalone_cancel) = (
            CancellationToken::new(),
            CancellationToken::new(),
            CancellationToken::new(),
        );
        let tenant = tenant();
        let execution = workloads.begin(
            Workload::Verification,
            Some(tenant),
            Some(execution_cancel.clone()),
        );
        let standalone = workloads.begin(Workload::Gateway, None, Some(standalone_cancel.clone()));
        let _standalone_bytes = hold(&memory, &standalone, 250);
        let nested_bytes = execution
            .scope(async {
                let mut held = Vec::new();
                for _ in 0..2 {
                    let nested = workloads
                        .admit(Workload::Gateway, None, Some(nested_cancel.clone()))
                        .await;
                    assert_eq!(nested.context(), execution.context());
                    held.push(hold(&memory, &nested, 200));
                }
                held
            })
            .await;
        assert_eq!(
            execution.bytes(),
            400,
            "nested bytes count as the execution's"
        );
        assert_eq!(workloads.workload_bytes(Workload::Verification), 400);
        assert_eq!(workloads.workload_bytes(Workload::Gateway), 250);
        assert!(
            starts_now(&workloads, Workload::Gateway, None),
            "outside the scope, admission starts its own operation"
        );

        let _commitment = {
            let observation = workloads.observe_commitment();
            observation.set(u64::MAX);
            observation
        };
        workloads.sample(0);
        workloads.sample(0);
        assert!(
            execution_cancel.is_cancelled(),
            "the execution is cancelled as a whole"
        );
        assert!(!nested_cancel.is_cancelled() && !standalone_cancel.is_cancelled());
        drop(nested_bytes);
        assert_eq!(execution.bytes(), 0);
    }

    /// Critical pressure first reclaims, then cancels one cancellable victim
    /// per sample: the largest operation of the workload furthest above its
    /// target. Uncancellable acknowledged or mandatory work is never chosen.
    ///
    /// # Panics
    /// Panics when a victim precedes reclamation, more than one is chosen
    /// per sample, or ineligible work is chosen.
    #[test]
    fn critical_pressure_reclaims_then_cancels_one_victim_per_sample() {
        let (memory, workloads) = manager();
        let reclaimed = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&reclaimed);
        workloads.on_reclaim(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        let (big, small, other) = (
            CancellationToken::new(),
            CancellationToken::new(),
            CancellationToken::new(),
        );
        let acknowledged = workloads.begin(Workload::Verification, None, None);
        let largest = workloads.begin(Workload::Verification, None, Some(big.clone()));
        let smaller = workloads.begin(Workload::Verification, None, Some(small.clone()));
        let gateway = workloads.begin(Workload::Gateway, None, Some(other.clone()));
        let held = [
            hold(&memory, &acknowledged, 450),
            hold(&memory, &largest, 300),
            hold(&memory, &smaller, 150),
            hold(&memory, &gateway, 120),
        ];

        assert_eq!(workloads.sample(0).state, PressureState::Critical);
        assert_eq!(
            reclaimed.load(Ordering::Relaxed),
            1,
            "reclamation comes first"
        );
        assert!(!big.is_cancelled() && !small.is_cancelled() && !other.is_cancelled());

        workloads.sample(0);
        assert!(big.is_cancelled(), "the largest above-target operation");
        assert!(
            !small.is_cancelled() && !other.is_cancelled(),
            "one victim per sample"
        );

        workloads.sample(0);
        assert!(small.is_cancelled(), "reassessed on the next sample");
        assert!(!other.is_cancelled(), "gateway is below its target");
        drop((held, acknowledged, largest, smaller, gateway));
    }

    /// With every workload at or below its target, the victim is the largest
    /// cancellable attributable operation of any workload; a larger
    /// uncancellable one is never chosen.
    ///
    /// # Panics
    /// Panics when another operation is selected or none is.
    #[test]
    fn at_target_workloads_yield_the_largest_cancellable_victim() {
        let (memory, workloads) = manager();
        let verification =
            workloads.begin(Workload::Verification, None, Some(CancellationToken::new()));
        let gateway = workloads.begin(Workload::Gateway, None, Some(CancellationToken::new()));
        let durable = workloads.begin(Workload::Bifrost, None, None);
        let held = [
            hold(&memory, &verification, 200),
            hold(&memory, &gateway, 300),
            hold(&memory, &durable, 400),
        ];
        let mut control = Control::default();
        let mut usage = [0; 3];
        for (workload, used, target) in [
            (Workload::Verification, 200, 300),
            (Workload::Gateway, 300, 300),
            (Workload::Bifrost, 400, 400),
        ] {
            usage[workload.index()] = used;
            control.targets[workload.index()] = Some(target);
        }

        let victim = select_victim(&control, usage, &workloads.live_operations())
            .expect("a cancellable operation is selected");
        assert!(
            Arc::ptr_eq(&victim, &gateway.shared),
            "the largest cancellable operation: {:?}",
            victim.context
        );
        drop((held, verification, gateway, durable));
    }

    /// Laned work waiting for admission holds no CPU lane, so the lane runs
    /// other work meanwhile.
    ///
    /// # Panics
    /// Panics when the waiting work holds the lane or other work cannot run.
    #[tokio::test]
    async fn admission_waits_hold_no_cpu_lane() {
        let (_memory, workloads) = manager();
        let worker = workloads.observe_commitment();
        worker.set(u64::MAX);
        assert_eq!(workloads.sample(0).state, PressureState::Critical);
        let lanes = workloads.lanes();
        let verification = workloads.begin(Workload::Verification, None, None);
        let mut waiting = pin!(lanes.laned(
            verification.context(),
            workloads.admit(Workload::Verification, None, None),
        ));
        assert!(poll_once(waiting.as_mut()).is_pending());
        assert_eq!(lanes.free(), 1, "waiting for admission holds no lane");
        let gateway = workloads.begin(Workload::Gateway, None, None);
        let ran = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            lanes.laned(gateway.context(), async { lanes.free() }),
        )
        .await
        .expect("the free lane runs other work");
        assert_eq!(ran, 0, "the other work holds the one lane");
        drop((worker, verification, gateway));
    }

    /// A paused admission starts once pressure recovers, and a dropped one
    /// withdraws its demand.
    ///
    /// # Panics
    /// Panics when a paused admission never starts, or a dropped one still
    /// counts as demand.
    #[tokio::test]
    async fn paused_admission_starts_after_recovery() {
        let (memory, workloads) = manager();
        let gateway = workloads.begin(Workload::Gateway, None, None);
        let mut held = hold(&memory, &gateway, 1_000);
        workloads.sample(0);
        let mut admission = pin!(workloads.admit(Workload::Gateway, None, None));
        assert!(poll_once(admission.as_mut()).is_pending());
        {
            let mut waiting = pin!(workloads.admit(Workload::Verification, None, None));
            assert!(poll_once(waiting.as_mut()).is_pending());
            workloads.sample(0);
            assert!(
                workloads.target(Workload::Verification).is_some(),
                "a waiting admission is demand"
            );
        }
        workloads.sample(0);
        assert_eq!(
            workloads.target(Workload::Verification),
            None,
            "a dropped admission withdraws its demand"
        );
        held.shrink(500);
        workloads.sample(0);
        workloads.sample(0);
        tokio::time::timeout(std::time::Duration::from_secs(5), admission)
            .await
            .expect("recovery admits the paused operation");
        drop((held, gateway));
    }
}
