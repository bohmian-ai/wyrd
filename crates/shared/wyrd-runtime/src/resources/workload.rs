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
    /// Woken when attributed bytes fall or the operation settles.
    pub(crate) changed: Arc<Notify>,
}

impl OperationShared {
    /// Adds `bytes` to this operation's attribution.
    pub(crate) fn add(&self, bytes: u64) {
        self.bytes.fetch_add(bytes, Ordering::AcqRel);
    }

    /// Removes `bytes` from this operation's attribution and wakes waiters.
    pub(crate) fn remove(&self, bytes: u64) {
        if bytes == 0 {
            return;
        }
        let previous = self.bytes.fetch_sub(bytes, Ordering::AcqRel);
        debug_assert!(previous >= bytes, "attribution underflow");
        self.changed.notify_waiters();
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
}

impl Operation {
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

impl Drop for Operation {
    fn drop(&mut self) {
        self.shared.live.store(false, Ordering::Release);
        self.shared.changed.notify_waiters();
    }
}

/// The registry behind every clone of one [`WorkloadManager`].
#[derive(Debug, Default)]
struct ManagerInner {
    /// Next operation id.
    next: AtomicU64,
    /// Every operation that may still hold attributed bytes.
    operations: Mutex<HashMap<OperationId, Weak<OperationShared>>>,
    /// Woken when usage falls, an operation settles, or pressure changes.
    changed: Arc<Notify>,
}

/// The one process workload manager: operation attribution today.
///
/// Cloning shares the manager.
#[derive(Debug, Clone, Default)]
pub struct WorkloadManager {
    /// The shared registry.
    inner: Arc<ManagerInner>,
}

impl WorkloadManager {
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
            changed: Arc::clone(&self.inner.changed),
        });
        let mut operations = self
            .inner
            .operations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        operations.retain(|_, operation| operation.strong_count() > 0);
        operations.insert(id, Arc::downgrade(&shared));
        Operation { shared }
    }

    /// Root bytes currently attributed to `workload`'s operations.
    #[must_use]
    pub fn workload_bytes(&self, workload: Workload) -> u64 {
        self.live_operations()
            .iter()
            .filter(|operation| operation.context.workload == workload)
            .map(|operation| operation.bytes.load(Ordering::Acquire))
            .sum()
    }

    /// Every operation that may still hold attributed bytes.
    fn live_operations(&self) -> Vec<Arc<OperationShared>> {
        self.inner
            .operations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .filter_map(Weak::upgrade)
            .collect()
    }
}
