//! Process-wide memory, scratch and CPU ownership shared by every capability.
//!
//! One [`ProcessResources`] exists per process. Its [`ByteLedger`]s record
//! the process memory and disposable scratch that owners actually retain:
//! Bifrost, hosted verification, recording, gateway buffering and the outbox
//! all charge the same two ledgers, so no capability holds an idle share and
//! no byte is counted twice. Outside Bifrost's declared workspace
//! reservations, a substantial buffer is charged when it is materialized or
//! retained, never for predicted future copies; the charge moves with the
//! buffer and returns only once its owner settles it.
//!
//! The [`WorkloadManager`] attributes charges to operations, samples memory
//! pressure across the parent and its managed workers, pauses new work above
//! its soft share under contention, and admits major CPU phases to the shared
//! [`CpuLanes`]. Each capability still decides how it reacts to a refusal
//! (spill, wait, or fail cooperatively) and keeps its own local limits.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::Notify;
use tokio::sync::futures::Notified;

mod lanes;
mod workload;

pub use lanes::{CpuLanes, LaneLease, Laned};
use workload::OperationShared;
pub use workload::{
    CommitObservation, Operation, OperationId, Pressure, PressureState, WorkContext, Workload,
    WorkloadManager,
};

/// A charge that does not fit the ledger's remaining bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{requested} bytes exceed the {available} bytes left of a {limit}-byte process budget")]
pub struct LedgerRefusal {
    /// Bytes the refused charge asked for.
    pub requested: u64,
    /// Bytes still unpromised when it was refused.
    pub available: u64,
    /// The ledger's whole budget.
    pub limit: u64,
}

/// One process budget of bytes and the bytes currently promised from it.
///
/// Cloning shares the ledger. Every charge is a [`ByteCharge`] that returns
/// its bytes when dropped and then wakes [`Self::changed`] waiters.
#[derive(Debug, Clone)]
pub struct ByteLedger {
    /// The shared budget; every clone and every charge points here.
    inner: Arc<LedgerInner>,
}

/// The state behind every clone of one [`ByteLedger`].
///
/// `used` may exceed `limit` only through infallible charges; fallible
/// charges never take it past `limit`.
#[derive(Debug)]
struct LedgerInner {
    /// The whole budget in bytes, fixed at construction.
    limit: u64,
    /// Bytes promised by live charges, including infallible overshoot.
    used: AtomicU64,
    /// Woken on every release so refused callers can retry.
    changed: Notify,
}

impl ByteLedger {
    /// A ledger promising at most `limit` bytes.
    #[must_use]
    pub fn new(limit: u64) -> Self {
        Self {
            inner: Arc::new(LedgerInner {
                limit,
                used: AtomicU64::new(0),
                changed: Notify::new(),
            }),
        }
    }

    /// The whole budget.
    #[must_use]
    pub fn limit(&self) -> u64 {
        self.inner.limit
    }

    /// Bytes currently promised, including infallible overshoot.
    #[must_use]
    pub fn used(&self) -> u64 {
        self.inner.used.load(Ordering::Acquire)
    }

    /// Bytes a fallible charge could still take right now.
    #[must_use]
    pub fn available(&self) -> u64 {
        self.inner.limit.saturating_sub(self.used())
    }

    /// Whether `other` is this same ledger.
    #[must_use]
    pub fn same_ledger(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// An empty charge that can grow later.
    #[must_use]
    pub fn empty_charge(&self) -> ByteCharge {
        ByteCharge {
            ledger: self.clone(),
            bytes: 0,
            owner: None,
        }
    }

    /// Promise `bytes` when they fit, all or nothing.
    ///
    /// # Errors
    /// Returns [`LedgerRefusal`] without changing the ledger when `bytes`
    /// exceed what remains unpromised.
    pub fn try_charge(&self, bytes: u64) -> Result<ByteCharge, LedgerRefusal> {
        self.try_add(bytes)?;
        Ok(ByteCharge {
            ledger: self.clone(),
            bytes,
            owner: None,
        })
    }

    /// Record `bytes` the process already holds and cannot refuse, such as
    /// mandatory engine growth, even past the limit; later fallible charges
    /// refuse sooner until it is returned.
    #[must_use]
    pub fn charge_infallible(&self, bytes: u64) -> ByteCharge {
        self.inner.used.fetch_add(bytes, Ordering::AcqRel);
        ByteCharge {
            ledger: self.clone(),
            bytes,
            owner: None,
        }
    }

    /// Resolves after the next release. Create it before retrying a refused
    /// charge so a release between the refusal and the wait is not missed.
    pub fn changed(&self) -> Notified<'_> {
        self.inner.changed.notified()
    }

    /// Adds `bytes` when they fit the limit, atomically and all or nothing.
    ///
    /// # Errors
    /// Returns [`LedgerRefusal`], leaving `used` unchanged, when `bytes`
    /// would overflow or take `used` past the limit.
    fn try_add(&self, bytes: u64) -> Result<(), LedgerRefusal> {
        let limit = self.inner.limit;
        self.inner
            .used
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|next| *next <= limit)
            })
            .map(drop)
            .map_err(|used| LedgerRefusal {
                requested: bytes,
                available: limit.saturating_sub(used),
                limit,
            })
    }

    /// Returns `bytes` and wakes waiters.
    fn release(&self, bytes: u64) {
        if bytes == 0 {
            return;
        }
        let previous = self.inner.used.fetch_sub(bytes, Ordering::AcqRel);
        debug_assert!(previous >= bytes, "ledger release underflow");
        self.inner.changed.notify_waiters();
    }
}

/// Bytes promised from one [`ByteLedger`], returned when dropped.
#[derive(Debug)]
pub struct ByteCharge {
    /// The ledger these bytes are promised from and returned to.
    ledger: ByteLedger,
    /// Bytes this charge currently owns; dropped charges return exactly this.
    bytes: u64,
    /// The operation these bytes are attributed to, when one has custody.
    owner: Option<Arc<OperationShared>>,
}

impl ByteCharge {
    /// Bytes this charge holds.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    /// The ledger this charge is promised from.
    #[must_use]
    pub const fn ledger(&self) -> &ByteLedger {
        &self.ledger
    }

    /// Grow by `bytes` when they fit, all or nothing.
    ///
    /// # Errors
    /// Returns [`LedgerRefusal`] without changing the charge on refusal.
    pub fn try_grow(&mut self, bytes: u64) -> Result<(), LedgerRefusal> {
        self.ledger.try_add(bytes)?;
        self.bytes += bytes;
        if let Some(owner) = &self.owner {
            owner.add(bytes);
        }
        Ok(())
    }

    /// Grow by bytes the process already holds and cannot refuse.
    pub fn grow_infallible(&mut self, bytes: u64) {
        self.ledger.inner.used.fetch_add(bytes, Ordering::AcqRel);
        self.bytes += bytes;
        if let Some(owner) = &self.owner {
            owner.add(bytes);
        }
    }

    /// Return up to `bytes`, never more than this charge holds.
    pub fn shrink(&mut self, bytes: u64) {
        let bytes = bytes.min(self.bytes);
        self.bytes -= bytes;
        if let Some(owner) = &self.owner {
            owner.remove(bytes);
        }
        self.ledger.release(bytes);
    }

    /// Move `bytes` of this charge into a new one, transferring ownership
    /// without charging the ledger again; `None` when it holds fewer.
    #[must_use]
    pub fn split(&mut self, bytes: u64) -> Option<Self> {
        self.bytes = self.bytes.checked_sub(bytes)?;
        Some(Self {
            ledger: self.ledger.clone(),
            bytes,
            owner: self.owner.clone(),
        })
    }

    /// Attribute this charge's bytes to `operation`, moving them from any
    /// previous owner without charging the ledger again.
    pub fn attribute(&mut self, operation: &Operation) {
        if let Some(previous) = self.owner.take() {
            previous.remove(self.bytes);
        }
        operation.shared.add(self.bytes);
        self.owner = Some(Arc::clone(&operation.shared));
    }

    /// The operation this charge is attributed to, if any.
    #[must_use]
    pub fn operation(&self) -> Option<OperationId> {
        self.owner.as_ref().map(|owner| owner.context.operation)
    }

    /// Take over `other`'s bytes without charging the ledger again.
    ///
    /// # Panics
    /// Panics when `other` is promised from a different ledger, which would
    /// return its bytes to the wrong budget.
    pub fn merge(&mut self, mut other: Self) {
        assert!(
            self.ledger.same_ledger(&other.ledger),
            "a charge merges only into its own ledger"
        );
        let bytes = std::mem::take(&mut other.bytes);
        if let Some(owner) = other.owner.take() {
            owner.remove(bytes);
        }
        if let Some(owner) = &self.owner {
            owner.add(bytes);
        }
        self.bytes += bytes;
    }
}

impl Drop for ByteCharge {
    fn drop(&mut self) {
        if let Some(owner) = &self.owner {
            owner.remove(self.bytes);
        }
        self.ledger.release(self.bytes);
    }
}

/// The process plan every capability composes from: effective CPU, one
/// memory ledger, and one disposable-scratch ledger for the data-root
/// filesystem.
#[derive(Debug, Clone)]
pub struct ProcessResources {
    /// CPUs the process may run on, resolved once at startup.
    effective_cpu: NonZeroUsize,
    /// The one process memory ledger every capability charges.
    memory: ByteLedger,
    /// The one disposable-scratch ledger for the data-root filesystem.
    scratch: ByteLedger,
    /// The one workload manager sharing memory and CPU among workloads.
    workloads: WorkloadManager,
}

impl ProcessResources {
    /// The plan over already-resolved budgets, with no headroom outside the
    /// managed memory budget.
    #[must_use]
    pub fn new(effective_cpu: NonZeroUsize, memory_bytes: u64, scratch_bytes: u64) -> Self {
        Self::with_ceiling(effective_cpu, memory_bytes, memory_bytes, scratch_bytes)
    }

    /// The plan for a process ceiling of `ceiling_bytes` (`P`, covering the
    /// parent and its managed workers) of which `memory_bytes` (`M = P - H`)
    /// is the managed budget; headroom `H` is excluded, never charged.
    #[must_use]
    pub fn with_ceiling(
        effective_cpu: NonZeroUsize,
        ceiling_bytes: u64,
        memory_bytes: u64,
        scratch_bytes: u64,
    ) -> Self {
        let memory = ByteLedger::new(memory_bytes);
        Self {
            effective_cpu,
            workloads: WorkloadManager::new(memory.clone(), ceiling_bytes, effective_cpu.get()),
            memory,
            scratch: ByteLedger::new(scratch_bytes),
        }
    }

    /// CPUs the process may run on: the number of managed CPU lanes.
    #[must_use]
    pub const fn effective_cpu(&self) -> NonZeroUsize {
        self.effective_cpu
    }

    /// The process memory ledger.
    #[must_use]
    pub const fn memory(&self) -> &ByteLedger {
        &self.memory
    }

    /// The disposable-scratch ledger of the data-root filesystem.
    #[must_use]
    pub const fn scratch(&self) -> &ByteLedger {
        &self.scratch
    }

    /// The workload manager every capability shares.
    #[must_use]
    pub const fn workloads(&self) -> &WorkloadManager {
        &self.workloads
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One capability may take the whole budget while others idle, a second
    /// borrows what the first returns, transfers never charge twice, and a
    /// refusal changes nothing.
    ///
    /// # Panics
    /// Panics when a charge is double counted, refused while it fits, or
    /// accepted past the limit.
    #[test]
    fn process_grants_follow_use_without_double_promises() {
        let ledger = ByteLedger::new(100);
        let mut bifrost = ledger
            .try_charge(100)
            .expect("an idle process lends everything");
        assert_eq!(
            ledger.try_charge(1).expect_err("nothing is left"),
            LedgerRefusal {
                requested: 1,
                available: 0,
                limit: 100
            }
        );
        bifrost.shrink(60);
        let hosted = ledger
            .try_charge(60)
            .expect("returned bytes are borrowable");
        assert_eq!(ledger.used(), 100);
        let mut moved = bifrost.split(40).expect("the charge holds 40");
        assert_eq!(ledger.used(), 100, "a transfer is not a second charge");
        moved.merge(bifrost);
        assert_eq!(moved.bytes(), 40);
        assert!(moved.try_grow(1).is_err());
        assert_eq!(moved.bytes(), 40, "a refused grow changes nothing");
        drop(hosted);
        moved.try_grow(60).expect("the hosted bytes came back");
        let overshoot = ledger.charge_infallible(5);
        assert_eq!(ledger.used(), 105);
        assert_eq!(ledger.available(), 0);
        drop((moved, overshoot));
        assert_eq!(ledger.used(), 0);
    }

    /// Custody follows the physical buffer: moving a charge to another
    /// operation (an outbox, say) changes attribution but never the root,
    /// and a buffer shared by many readers is one charge until the last
    /// reference drops.
    ///
    /// # Panics
    /// Panics when attribution double counts, is lost on transfer, or root
    /// usage changes on a transfer or a refusal.
    #[test]
    fn attribution_follows_custody_and_counts_shared_buffers_once() {
        let process = ProcessResources::new(NonZeroUsize::MIN, 1_000, 0);
        let ledger = process.memory();
        let workloads = process.workloads();
        let verification = workloads.begin(Workload::Verification, None, None);
        let outbox = workloads.begin(Workload::Bifrost, None, None);

        let mut retained = ledger.try_charge(100).expect("fits");
        retained.attribute(&verification);
        assert_eq!(verification.bytes(), 100);
        assert_eq!(retained.operation(), Some(verification.context().operation));

        let shared = Arc::new(retained);
        let readers = [
            Arc::clone(&shared),
            Arc::clone(&shared),
            Arc::clone(&shared),
        ];
        assert_eq!(ledger.used(), 100, "readers are not copies");
        assert_eq!(workloads.workload_bytes(Workload::Verification), 100);

        assert!(ledger.try_charge(901).is_err());
        assert_eq!(verification.bytes(), 100, "a refusal attributes nothing");

        drop(readers);
        let mut retained = Arc::into_inner(shared).expect("the last reference");
        let mut published = retained.split(40).expect("holds 40");
        published.attribute(&outbox);
        assert_eq!(ledger.used(), 100, "handoff is not a second root charge");
        assert_eq!(verification.bytes(), 60);
        assert_eq!(outbox.bytes(), 40);

        retained.grow_infallible(10);
        retained.shrink(30);
        assert_eq!(verification.bytes(), 40);
        published.merge(retained);
        assert_eq!(outbox.bytes(), 80, "a merge carries custody with it");
        assert_eq!(verification.bytes(), 0);

        drop(verification);
        assert_eq!(workloads.workload_bytes(Workload::Bifrost), 80);
        drop(published);
        assert_eq!(outbox.bytes(), 0);
        assert_eq!(ledger.used(), 0);
    }

    /// A waiter registered before retrying sees a release that happens
    /// between its refusal and its wait.
    ///
    /// # Panics
    /// Panics when the release is missed.
    #[tokio::test]
    async fn release_wakes_a_registered_waiter() {
        let ledger = ByteLedger::new(1);
        let held = ledger.try_charge(1).expect("fits");
        let changed = ledger.changed();
        assert!(ledger.try_charge(1).is_err());
        drop(held);
        tokio::time::timeout(std::time::Duration::from_secs(5), changed)
            .await
            .expect("the release wakes the waiter");
        drop(ledger.try_charge(1).expect("the byte is free again"));
    }
}
