//! Fair hosted admission over the process memory ledger.
//!
//! Every hosted grant — a Python session's peak envelope — is promised from
//! the one process memory ledger Bifrost, recording, and request buffering
//! also charge. There is no fixed hosted share, waiter count, or worker
//! count: a request runs as soon as its bytes fit. When they do not, it waits
//! in its tenant's FIFO line, and lines are served round robin, so a tenant
//! that keeps refilling cannot bypass another eligible tenant, and a head
//! whose bytes do not fit never blocks another tenant whose bytes do.
//!
//! A waiter's retained bytes (the inputs it holds while waiting) are charged
//! before it queues; when even those do not fit, it is refused at once
//! rather than queued. A grant is one atomic charge, never a partial one, and
//! a waiter that is cancelled or expires leaves nothing behind.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, Weak};

use tokio::sync::oneshot;
use tokio::time::Instant;
use wyrd_runtime::resources::{ByteCharge, ByteLedger, LedgerRefusal};
use wyrd_spec::DataTenantId;

use crate::app::metrics::{
    VERIFICATION_HOSTED_BYTES, VERIFICATION_HOSTED_OPERATIONS, VERIFICATION_HOSTED_REFUSALS_TOTAL,
    VERIFICATION_HOSTED_WAIT_SECONDS,
};

/// Why a hosted request was not admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AdmissionRefusal {
    /// Process memory cannot hold the request's bytes right now, and the
    /// request may not wait for them: its retained waiting bytes do not fit,
    /// or its grant can never fit the whole ledger.
    #[error("process memory refused the hosted request: {0}")]
    Memory(LedgerRefusal),
    /// The request's deadline passed while it waited.
    #[error("process memory did not free before the request's deadline")]
    Expired,
}

impl AdmissionRefusal {
    /// The bounded `reason` label of the refusal.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Memory(_) => "memory",
            Self::Expired => "expired",
        }
    }

    /// Count the refusal under its reason and return it.
    fn counted(self) -> Self {
        metrics::counter!(VERIFICATION_HOSTED_REFUSALS_TOTAL, "reason" => self.reason())
            .increment(1);
        self
    }
}

/// Fair, work-conserving admission of hosted grants.
///
/// Cloning shares the admission. Construct inside a Tokio runtime: a
/// background task serves waiters whenever the process ledger releases bytes,
/// and ends once the last clone drops.
#[derive(Debug, Clone)]
pub struct HostedAdmission {
    /// The shared ledger and queue; the serving task holds only a weak
    /// reference, so dropping the last clone ends it.
    inner: Arc<Inner>,
}

/// The state every [`HostedAdmission`] clone and its serving task share.
#[derive(Debug)]
struct Inner {
    /// The process memory ledger every grant is promised from.
    memory: ByteLedger,
    /// Waiting requests by tenant.
    queue: Mutex<Queue>,
}

/// Waiting requests grouped per tenant and served round-robin, so one
/// tenant's backlog never starves another's; `waiting_bytes` is always the
/// sum of every queued waiter's bytes.
#[derive(Debug, Default)]
struct Queue {
    /// Tenants with waiters, in service order.
    rotation: VecDeque<DataTenantId>,
    /// Each tenant's waiters, oldest first.
    lines: HashMap<DataTenantId, VecDeque<Waiter>>,
    /// The next waiter's identity.
    next: u64,
    /// Bytes every waiter has asked for.
    waiting_bytes: u64,
}

/// One queued grant request; it holds no charge until it is served.
#[derive(Debug)]
struct Waiter {
    /// Identity for cancellation.
    id: u64,
    /// Bytes the grant charges.
    bytes: u64,
    /// Where the grant goes.
    grant: oneshot::Sender<ByteCharge>,
}

impl Waiter {
    /// Account for the waiter joining a line holding `waiting_bytes`.
    fn joined(&self, waiting_bytes: &mut u64) {
        *waiting_bytes += self.bytes;
        metrics::gauge!(VERIFICATION_HOSTED_OPERATIONS, "state" => "waiting").increment(1.0);
        metrics::gauge!(VERIFICATION_HOSTED_BYTES, "state" => "waiting")
            .increment(self.bytes as f64);
    }

    /// Account for the waiter leaving its line, granted, cancelled, or
    /// expired; the exact inverse of [`joined`](Self::joined).
    fn left(&self, waiting_bytes: &mut u64) {
        *waiting_bytes -= self.bytes;
        metrics::gauge!(VERIFICATION_HOSTED_OPERATIONS, "state" => "waiting").decrement(1.0);
        metrics::gauge!(VERIFICATION_HOSTED_BYTES, "state" => "waiting")
            .decrement(self.bytes as f64);
    }
}

impl HostedAdmission {
    /// Admission over the process `memory` ledger.
    ///
    /// # Panics
    /// Panics when called outside a Tokio runtime, because it spawns the
    /// task that serves waiters as the ledger releases bytes.
    #[must_use]
    pub fn new(memory: ByteLedger) -> Self {
        let inner = Arc::new(Inner {
            memory: memory.clone(),
            queue: Mutex::default(),
        });
        tokio::spawn(serve(memory, Arc::downgrade(&inner)));
        Self { inner }
    }

    /// The process memory ledger grants are promised from.
    #[must_use]
    pub fn memory(&self) -> &ByteLedger {
        &self.inner.memory
    }

    /// Charge the `bytes` a request retains while it waits.
    ///
    /// # Errors
    /// Returns [`AdmissionRefusal::Memory`] when they do not fit; the request
    /// is refused, never queued.
    pub fn hold(&self, bytes: u64) -> Result<ByteCharge, AdmissionRefusal> {
        self.inner
            .memory
            .try_charge(bytes)
            .map_err(|refusal| AdmissionRefusal::Memory(refusal).counted())
    }

    /// Wait in `tenant`'s line for `bytes`, returning one charge holding
    /// them plus the request's `held` waiting bytes.
    ///
    /// A request runs at once when nobody waits and its bytes fit; otherwise
    /// it queues behind its tenant's earlier requests.
    ///
    /// # Errors
    /// Returns [`AdmissionRefusal::Memory`] when `bytes` plus `held` exceed
    /// the whole ledger and so can never fit, and [`AdmissionRefusal::Expired`] when
    /// `deadline` passes first; either way `held` is released.
    pub async fn acquire(
        &self,
        tenant: DataTenantId,
        mut held: ByteCharge,
        bytes: u64,
        deadline: Instant,
    ) -> Result<ByteCharge, AdmissionRefusal> {
        let memory = &self.inner.memory;
        let total = bytes.saturating_add(held.bytes());
        if total > memory.limit() {
            return Err(AdmissionRefusal::Memory(LedgerRefusal {
                requested: total,
                available: memory.available(),
                limit: memory.limit(),
            })
            .counted());
        }
        let (grant, granted) = oneshot::channel();
        let id = {
            let mut queue = self.inner.lock();
            if queue.rotation.is_empty()
                && let Ok(charge) = memory.try_charge(bytes)
            {
                held.merge(charge);
                return Ok(held);
            }
            queue.push(tenant, bytes, grant)
        };
        let waiting = Waiting {
            inner: &self.inner,
            tenant,
            id,
        };
        self.inner.dispatch();
        let queued = Instant::now();
        let charge = tokio::time::timeout_at(deadline, granted).await;
        drop(waiting);
        let (outcome, granted) = match charge {
            Ok(Ok(charge)) => {
                held.merge(charge);
                ("granted", Ok(held))
            }
            Ok(Err(_)) | Err(_) => ("expired", Err(AdmissionRefusal::Expired.counted())),
        };
        metrics::histogram!(VERIFICATION_HOSTED_WAIT_SECONDS, "outcome" => outcome)
            .record(queued.elapsed().as_secs_f64());
        granted
    }

    /// Requests waiting now, and the bytes they ask for.
    #[must_use]
    pub fn waiting(&self) -> (usize, u64) {
        let queue = self.inner.lock();
        (
            queue.lines.values().map(VecDeque::len).sum(),
            queue.waiting_bytes,
        )
    }
}

impl Inner {
    /// The queue, recovered after a panic elsewhere: every critical section
    /// leaves it consistent.
    fn lock(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Grant every waiter whose bytes fit, round robin across tenants and
    /// FIFO within each.
    ///
    /// Each pass offers every tenant's head once. A tenant served in a pass
    /// moves behind those that were not, so a head that did not fit is
    /// offered first next time and a refilling tenant waits its turn. Passes
    /// repeat until one serves nobody.
    fn dispatch(&self) {
        let mut queue = self.lock();
        loop {
            let mut unserved = VecDeque::with_capacity(queue.rotation.len());
            let mut served = VecDeque::new();
            while let Some(tenant) = queue.rotation.pop_front() {
                match queue.serve_head(tenant, &self.memory) {
                    Served::Granted { more: true } => served.push_back(tenant),
                    Served::Granted { more: false } | Served::Empty => {}
                    Served::DoesNotFit => unserved.push_back(tenant),
                }
            }
            let progressed = !served.is_empty();
            unserved.extend(served);
            queue.rotation = unserved;
            if !progressed {
                return;
            }
        }
    }
}

/// What offering one tenant's head did.
enum Served {
    /// The head was granted; `more` waiters remain in the line.
    Granted { more: bool },
    /// The head's bytes do not fit.
    DoesNotFit,
    /// The line had no live waiter left.
    Empty,
}

impl Queue {
    /// Queue `bytes` for `tenant`, returning the waiter's identity.
    fn push(
        &mut self,
        tenant: DataTenantId,
        bytes: u64,
        grant: oneshot::Sender<ByteCharge>,
    ) -> u64 {
        let id = self.next;
        self.next += 1;
        let line = self.lines.entry(tenant).or_default();
        if line.is_empty() {
            self.rotation.push_back(tenant);
        }
        let waiter = Waiter { id, bytes, grant };
        waiter.joined(&mut self.waiting_bytes);
        line.push_back(waiter);
        id
    }

    /// Offer `tenant`'s oldest live waiter its bytes.
    fn serve_head(&mut self, tenant: DataTenantId, memory: &ByteLedger) -> Served {
        let Some(line) = self.lines.get_mut(&tenant) else {
            return Served::Empty;
        };
        while line.front().is_some_and(|waiter| waiter.grant.is_closed()) {
            if let Some(gone) = line.pop_front() {
                gone.left(&mut self.waiting_bytes);
            }
        }
        let Some(head) = line.front() else {
            self.lines.remove(&tenant);
            return Served::Empty;
        };
        let Ok(charge) = memory.try_charge(head.bytes) else {
            return Served::DoesNotFit;
        };
        let Some(head) = line.pop_front() else {
            return Served::Empty;
        };
        head.left(&mut self.waiting_bytes);
        // A waiter that left after the check drops the charge with the send.
        drop(head.grant.send(charge));
        let more = !line.is_empty();
        if !more {
            self.lines.remove(&tenant);
        }
        Served::Granted { more }
    }

    /// Remove waiter `id` from `tenant`'s line, when it is still there.
    fn remove(&mut self, tenant: DataTenantId, id: u64) {
        let Some(line) = self.lines.get_mut(&tenant) else {
            return;
        };
        if let Some(index) = line.iter().position(|waiter| waiter.id == id)
            && let Some(gone) = line.remove(index)
        {
            gone.left(&mut self.waiting_bytes);
        }
        if line.is_empty() {
            self.lines.remove(&tenant);
            self.rotation.retain(|queued| *queued != tenant);
        }
    }
}

/// A queued request; dropping it leaves the line.
struct Waiting<'a> {
    /// The admission whose queue holds the waiter.
    inner: &'a Inner,
    /// The tenant line the waiter is in.
    tenant: DataTenantId,
    /// The waiter's identity in that line, used to remove exactly it.
    id: u64,
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.inner.lock().remove(self.tenant, self.id);
    }
}

/// Serve waiters each time the process ledger releases bytes, until the
/// admission is gone.
async fn serve(memory: ByteLedger, inner: Weak<Inner>) {
    loop {
        let released = memory.changed();
        let mut released = std::pin::pin!(released);
        released.as_mut().enable();
        match inner.upgrade() {
            Some(inner) => inner.dispatch(),
            None => return,
        }
        released.await;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::task::JoinHandle;

    use super::*;

    /// A deadline no test reaches.
    fn far() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    /// Queue a request for `bytes` in `tenant`'s line, holding nothing while
    /// it waits.
    fn wait_for(
        admission: &HostedAdmission,
        tenant: DataTenantId,
        bytes: u64,
    ) -> JoinHandle<Result<ByteCharge, AdmissionRefusal>> {
        let admission = admission.clone();
        let held = admission.memory().empty_charge();
        tokio::spawn(async move { admission.acquire(tenant, held, bytes, far()).await })
    }

    /// Wait until `admission` has `count` waiters.
    async fn queued(admission: &HostedAdmission, count: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while admission.waiting().0 != count {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the waiters queue");
    }

    /// Tenant A queues two requests and then tenant B one. As capacity frees
    /// one request at a time, B is served between A's two, and each waiter's
    /// retained bytes stay charged until it is granted.
    #[tokio::test]
    async fn admission_rotates_tenants_and_accounts_waiting_bytes() {
        let memory = ByteLedger::new(100);
        let admission = HostedAdmission::new(memory.clone());
        let mut busy = memory.try_charge(95).expect("other work holds 95 bytes");
        let (a, b) = (DataTenantId::new_v7(), DataTenantId::new_v7());
        let (granted, mut order) = tokio::sync::mpsc::unbounded_channel();
        for (waiters, (tenant, name)) in [(a, "a1"), (a, "a2"), (b, "b1")].into_iter().enumerate() {
            let held = admission.hold(1).expect("one waiting byte fits");
            let waiter = admission.clone();
            let granted = granted.clone();
            tokio::spawn(async move {
                let charge = waiter.acquire(tenant, held, 5, far()).await;
                granted.send((name, charge)).expect("the test listens");
            });
            queued(&admission, waiters + 1).await;
        }
        assert_eq!(memory.used(), 98, "three waiting bytes are charged");

        let mut served = Vec::new();
        for _ in 0..3 {
            busy.shrink(5);
            let (name, charge) = order.recv().await.expect("a waiter is granted");
            assert_eq!(charge.map(|charge| charge.bytes()), Ok(6));
            served.push(name);
        }

        assert_eq!(served, ["a1", "b1", "a2"]);
        assert_eq!(memory.used(), 80, "granted charges drop with their tasks");
    }

    /// When even a request's retained waiting bytes do not fit, it is
    /// refused at once instead of queued.
    #[tokio::test]
    async fn waiting_bytes_that_do_not_fit_are_refused() {
        let memory = ByteLedger::new(100);
        let admission = HostedAdmission::new(memory.clone());
        let _busy = memory.try_charge(98).expect("other work holds 98 bytes");

        let refused = admission.hold(3).map(|charge| charge.bytes());

        assert_eq!(
            refused,
            Err(AdmissionRefusal::Memory(LedgerRefusal {
                requested: 3,
                available: 2,
                limit: 100
            }))
        );
        assert_eq!(admission.waiting(), (0, 0));
    }

    /// A lone request may take the whole idle ledger; one whose bytes, with
    /// or without its waiting bytes, exceed the ledger can never fit and is
    /// refused instead of waiting forever.
    #[tokio::test]
    async fn a_lone_request_borrows_the_idle_ledger() {
        let memory = ByteLedger::new(100);
        let admission = HostedAdmission::new(memory.clone());
        let tenant = DataTenantId::new_v7();

        let whole = admission
            .acquire(tenant, memory.empty_charge(), 100, far())
            .await
            .expect("an idle ledger lends everything");
        assert_eq!(whole.bytes(), 100);
        drop(whole);

        let oversized = admission
            .acquire(tenant, memory.empty_charge(), 101, far())
            .await
            .map(|charge| charge.bytes());
        assert!(matches!(oversized, Err(AdmissionRefusal::Memory(_))));

        let held = admission.hold(1).expect("one waiting byte fits");
        let with_held = tokio::time::timeout(
            Duration::from_secs(1),
            admission.acquire(tenant, held, 100, far()),
        )
        .await
        .expect("a request that can never fit is refused at once")
        .map(|charge| charge.bytes());
        assert!(matches!(with_held, Err(AdmissionRefusal::Memory(_))));
        assert_eq!(memory.used(), 0);
    }

    /// Tenant A's 40-byte head cannot fit, so tenant B's 10-byte request
    /// runs past it, and A stays queued.
    #[tokio::test]
    async fn a_non_fitting_head_does_not_block_another_tenant() {
        let memory = ByteLedger::new(100);
        let admission = HostedAdmission::new(memory.clone());
        let _busy = memory.try_charge(80).expect("other work holds 80 bytes");
        let big = wait_for(&admission, DataTenantId::new_v7(), 40);
        queued(&admission, 1).await;

        let small = admission
            .acquire(DataTenantId::new_v7(), memory.empty_charge(), 10, far())
            .await
            .expect("tenant B runs past A's head");

        assert_eq!(small.bytes(), 10);
        assert_eq!(admission.waiting(), (1, 40), "A still waits");
        assert!(!big.is_finished());
    }

    /// A cancelled waiter leaves the line and releases its waiting bytes.
    #[tokio::test]
    async fn cancelled_waiters_leave_no_bytes() {
        let memory = ByteLedger::new(100);
        let admission = HostedAdmission::new(memory.clone());
        let _busy = memory.try_charge(80).expect("other work holds 80 bytes");
        let held = admission.hold(5).expect("five waiting bytes fit");
        let waiter = {
            let admission = admission.clone();
            tokio::spawn(async move {
                admission
                    .acquire(DataTenantId::new_v7(), held, 40, far())
                    .await
            })
        };
        queued(&admission, 1).await;
        assert_eq!(memory.used(), 85);

        waiter.abort();
        assert!(waiter.await.is_err(), "the waiter is cancelled");

        assert_eq!(admission.waiting(), (0, 0));
        assert_eq!(memory.used(), 80);
    }

    /// Every queue transition (wait, cancel, grant, expire) keeps the waiting
    /// count and bytes balanced, each refusal is counted once under its
    /// reason, and each queued wait is timed under its outcome.
    ///
    /// # Panics
    /// Panics when a rendered sample is missing or a request is not served
    /// as arranged.
    #[tokio::test]
    async fn admission_transitions_balance_their_telemetry() {
        let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let expect = |samples: &[String]| {
            let rendered = handle.render();
            for sample in samples {
                assert!(
                    rendered.lines().any(|line| line == sample),
                    "{sample} missing from {rendered}"
                );
            }
        };
        let waiting = |count: u64, bytes: u64| {
            [
                format!("wyrd_verification_hosted_operations{{state=\"waiting\"}} {count}"),
                format!("wyrd_verification_hosted_bytes{{state=\"waiting\"}} {bytes}"),
            ]
        };
        let memory = ByteLedger::new(100);
        let admission = HostedAdmission::new(memory.clone());
        let mut busy = memory.try_charge(80).expect("other work holds 80 bytes");
        let tenant = DataTenantId::new_v7();

        assert!(admission.hold(30).is_err(), "30 waiting bytes do not fit");
        let granted = wait_for(&admission, tenant, 40);
        queued(&admission, 1).await;
        let cancelled = wait_for(&admission, DataTenantId::new_v7(), 30);
        queued(&admission, 2).await;
        expect(&waiting(2, 70));

        cancelled.abort();
        assert!(cancelled.await.is_err(), "the waiter is cancelled");
        expect(&waiting(1, 40));

        busy.shrink(40);
        let charge = granted
            .await
            .expect("the waiter task completes")
            .expect("the freed bytes are granted");
        expect(&waiting(0, 0));

        let expired = admission
            .acquire(
                tenant,
                memory.empty_charge(),
                40,
                Instant::now() + Duration::from_millis(20),
            )
            .await
            .map(|charge| charge.bytes());
        assert_eq!(expired, Err(AdmissionRefusal::Expired));
        drop(charge);
        expect(&waiting(0, 0));
        expect(&[
            "wyrd_verification_hosted_refusals_total{reason=\"memory\"} 1".to_owned(),
            "wyrd_verification_hosted_refusals_total{reason=\"expired\"} 1".to_owned(),
            "wyrd_verification_hosted_wait_seconds_count{outcome=\"granted\"} 1".to_owned(),
            "wyrd_verification_hosted_wait_seconds_count{outcome=\"expired\"} 1".to_owned(),
        ]);
    }

    /// A waiter whose deadline passes is refused as expired and releases its
    /// waiting bytes.
    #[tokio::test]
    async fn expired_waiters_release_their_waiting_bytes() {
        let memory = ByteLedger::new(100);
        let admission = HostedAdmission::new(memory.clone());
        let _busy = memory.try_charge(80).expect("other work holds 80 bytes");
        let held = admission.hold(2).expect("two waiting bytes fit");

        let expired = admission
            .acquire(
                DataTenantId::new_v7(),
                held,
                40,
                Instant::now() + Duration::from_millis(20),
            )
            .await
            .map(|charge| charge.bytes());

        assert_eq!(expired, Err(AdmissionRefusal::Expired));
        assert_eq!(admission.waiting(), (0, 0));
        assert_eq!(memory.used(), 80);
    }
}
