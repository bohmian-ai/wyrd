//! The one claim loop the Verifier runner and the Operator worker share.
//!
//! Both capabilities drain a durable, tenant-scoped, leased queue the same
//! way: pull work on demand. A round reads the due tenants once, then claims
//! round-robin across them, each tenant's fair share of the free capacity per
//! pass in one statement and one committed transaction that races shutdown,
//! repeating passes until the free capacity is spent or every tenant has
//! nothing left; a burst in one tenant is therefore claimed in one round up to
//! capacity, in as few statements as its share allows. There is no tenant
//! limit. A round that runs out of capacity rather than work carries its
//! remaining due tenants into the next round, which claims from them without
//! reading the due tenants again; the loop reads them afresh once no carried
//! tenant remains or the carried list is a poll interval old, so a saturated
//! loop pays for the cross-tenant read at most once per poll interval and
//! discovers a newly due tenant within one.
//! Every claim takes one permit of the loop's capacity before claiming and
//! holds it until the item finishes, so a loop claims nothing while its
//! capacity is spent; the Verifier runner shares its capacity with group
//! snapshot handoff. Operator delivery also takes delivery permits before
//! claiming. A loop that claimed something claims again at once; an idle loop
//! wakes on its optional local wake signal (the Verifier runner's is notified
//! when this process queues new submissions), when an item finishes, or after
//! the poll interval, which remains only the fallback for work queued by other
//! replicas. An item refused
//! by a full shared resource pauses claiming until a running item finishes.
//! On shutdown admit no further claim, give in-flight work the drain grace,
//! then cancel and let it release its lease. [`ClaimLoop`] owns that
//! mechanism; each capability implements [`LeasedWork`] to supply only its
//! due-tenant list, claim, process-and-settle, and late-claim release steps.

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Notify, Semaphore};
use tokio::task::{JoinError, JoinSet};
use tokio_util::sync::CancellationToken;
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::verifier_executions::Settlement;
use wyrd_sql::{SqlError, TenantConn, WyrdPostgres};

#[cfg(feature = "test-support")]
use super::CapabilityCrash;
use super::RuntimeLimits;
use super::health::RuntimeCapability;
use super::permits::OperatorPermits;

/// The capability-specific steps of one leased queue.
///
/// Implemented by the Verifier runner and the Operator worker; the claim
/// loop owns durable claims, the stop race, draining, and reaping.
pub(super) trait LeasedWork: Send + Sync + 'static {
    /// One claimed item and its lease.
    type Claim: Send + 'static;

    /// Health slot, crash switch, and log label of this capability.
    const CAPABILITY: RuntimeCapability;
    /// Gauge the loop sets to this capability's in-flight items, or `None`
    /// when the capability accounts for its own activity.
    const ACTIVE_GAUGE: Option<&'static str>;

    /// Every tenant with claimable work, most overdue first.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the cross-tenant read fails.
    fn due_tenants(&self) -> impl Future<Output = Result<Vec<DataTenantId>, SqlError>> + Send;

    /// Claim up to `limit` of the tenant's next items inside `conn`'s
    /// uncommitted transaction; never more than `limit`, and nothing when the
    /// tenant has nothing claimable.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the claim fails.
    fn claim(
        &self,
        conn: &mut TenantConn<'_>,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<Self::Claim>, SqlError>> + Send;

    /// Execute a claimed item and apply its single fenced settlement.
    ///
    /// `abandon` cancels the item past the drain grace; a retryable failure
    /// observed after `stop` should be released rather than charged.
    /// `spawned_at` is the process-monotonic instant the loop spawned the
    /// item, so its first poll can measure task-start delay. Resolves to
    /// `true` when a full shared resource refused the item, which the loop
    /// answers by claiming nothing new until a running item finishes.
    fn process(
        self: Arc<Self>,
        tenant: DataTenantId,
        claim: Self::Claim,
        stop: CancellationToken,
        abandon: CancellationToken,
        spawned_at: Instant,
    ) -> impl Future<Output = bool> + Send;

    /// Release an item whose claim committed after shutdown began, with its
    /// attempt refunded and without executing it; failures are logged and
    /// the lease expires into a reclaim.
    fn release_late(
        &self,
        tenant: DataTenantId,
        claim: &Self::Claim,
    ) -> impl Future<Output = ()> + Send;
}

/// Owner of durable claims, draining, and reaping for one leased queue.
pub(super) struct ClaimLoop {
    /// Wyrd Postgres owner that opens every tenant-scoped claim transaction.
    postgres: WyrdPostgres,
    /// Operator delivery capacity; Verifier execution has no permit gate.
    permits: Option<OperatorPermits>,
    /// Active-work capacity in this process; each running item holds one
    /// permit.
    capacity: Arc<Semaphore>,
    /// Idle fallback wait between empty claim rounds, for work this process
    /// is not woken for.
    poll_interval: Duration,
    /// Local signal that new work was queued, which ends an idle wait early;
    /// `None` when the capability relies on the poll interval alone.
    wake: Option<Arc<Notify>>,
    /// How long shutdown waits for in-flight items before cancelling them.
    drain_grace: Duration,
    /// Test-only crash switch checked every loop turn.
    #[cfg(feature = "test-support")]
    crash: Option<CapabilityCrash>,
}

impl ClaimLoop {
    /// Build a loop claiming through `postgres` under `permits` and `limits`,
    /// each running item holding one permit of `capacity`; an idle loop also
    /// wakes when `wake` is notified.
    pub(super) fn new(
        postgres: WyrdPostgres,
        permits: Option<OperatorPermits>,
        capacity: Arc<Semaphore>,
        wake: Option<Arc<Notify>>,
        limits: &RuntimeLimits,
    ) -> Self {
        Self {
            postgres,
            permits,
            capacity,
            poll_interval: limits.poll_interval,
            wake,
            drain_grace: limits.drain_grace,
            #[cfg(feature = "test-support")]
            crash: None,
        }
    }

    /// Let `crash` panic this loop at its next turn.
    #[cfg(feature = "test-support")]
    pub(super) fn arm_crash(&mut self, crash: CapabilityCrash) {
        self.crash = Some(crash);
    }

    /// Claim and process `work` until `stop` is cancelled, then drain.
    ///
    /// Each turn runs one claim round up to the free capacity and spawns what
    /// it claimed; Operator work holds its delivery permit. While no capacity
    /// permit is free no round is claimed. A round that claimed something is
    /// followed at once by the next. An empty round waits the poll interval,
    /// until an item finishes, or until the wake signal is notified; the
    /// signal stores one notification while nobody waits, so a wake landing
    /// mid-round is not lost. Once an item reports a shared-resource refusal, rounds
    /// are skipped until a running item finishes (or none is running). On
    /// `stop` no further claim is admitted (an uncommitted
    /// claim rolls back and a claim committed after `stop` is released
    /// unexecuted), items spawned before `stop` get the drain grace to settle,
    /// and any still running are then cancelled through `abandon` and release
    /// their leases before this returns.
    ///
    /// # Panics
    /// Panics under `test-support` when a test armed a crash for
    /// `W::CAPABILITY`; in-flight items are aborted with the task and keep
    /// their leases until expiry.
    pub(super) async fn run<W: LeasedWork>(&self, work: &Arc<W>, stop: CancellationToken) {
        let abandon = CancellationToken::new();
        let mut spawned = JoinSet::new();
        let mut paused = false;
        let mut carried = Carried::default();
        while !stop.is_cancelled() {
            #[cfg(feature = "test-support")]
            if let Some(crash) = &self.crash {
                crash.check(W::CAPABILITY);
            }
            paused &= !spawned.is_empty();
            let claimed = if paused || self.capacity.available_permits() == 0 {
                0
            } else {
                match self
                    .claim_round(work, &mut carried, &stop, &abandon, &mut spawned)
                    .await
                {
                    Ok(claimed) => claimed,
                    Err(error) => {
                        tracing::warn!(capability = W::CAPABILITY.as_str(), %error, "claim round failed");
                        0
                    }
                }
            };
            while let Some(finished) = spawned.try_join_next() {
                paused = reap::<W>(finished);
            }
            self.record_active::<W>(spawned.len());
            if claimed > 0 {
                continue;
            }
            tokio::select! {
                () = stop.cancelled() => {}
                () = tokio::time::sleep(self.poll_interval) => {}
                () = Self::woken(self.wake.as_deref()) => {}
                Some(finished) = spawned.join_next(), if !spawned.is_empty() => {
                    paused = reap::<W>(finished);
                }
            }
        }
        let drained = tokio::time::timeout(self.drain_grace, async {
            while let Some(finished) = spawned.join_next().await {
                reap::<W>(finished);
            }
        })
        .await;
        if drained.is_err() {
            tracing::warn!(
                capability = W::CAPABILITY.as_str(),
                in_flight = spawned.len(),
                "drain grace elapsed; releasing in-flight work"
            );
            abandon.cancel();
            while let Some(finished) = spawned.join_next().await {
                reap::<W>(finished);
            }
        }
        self.record_active::<W>(spawned.len());
    }

    /// Claim up to the free capacity, round-robin across the due tenants.
    ///
    /// The round claims from the tenants `carried` from the previous round
    /// while they are fresh (see [`Carried::take`]); otherwise it reads the
    /// due tenants once. With several due, the first pass claims
    /// one item per tenant, so every tenant starts work before any tenant's
    /// batch. Each later pass, or the only one when a single tenant is due,
    /// visits every remaining tenant and takes its fair share of the capacity
    /// still free, `ceil(free / tenants left in the pass)` permits, before
    /// claiming that many items in one statement; permits a short claim
    /// leaves unused are returned at once. Only tenants that filled their
    /// share stay for the next pass, so a single bursting tenant takes all
    /// free capacity in one statement. The
    /// round ends once no capacity permit is free, delivery permits are
    /// saturated, `stop` fires, or no tenant remains. A round that runs out of
    /// capacity or delivery permits carries the tenant it stopped at, its
    /// unvisited tenants, then those that filled their share this pass, into
    /// `carried` in that order, so the next round resumes the rotation where
    /// this one stopped; a tenant whose claim came back short or failed is
    /// not carried, and a round ending with no tenant left carries none.
    /// Only Operator work takes
    /// delivery permits, one per item, before the claim and drops unused ones;
    /// a tenant without a free delivery permit leaves the round. Claims
    /// committed after `stop` fired are released at once instead of being
    /// spawned. Returns the number of items claimed and spawned. A tenant
    /// whose claim fails is logged and leaves the round, so one tenant's
    /// failure never starves the rest; its rolled-back claim consumes no
    /// attempt.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the due-tenant list fails.
    async fn claim_round<W: LeasedWork>(
        &self,
        work: &Arc<W>,
        carried: &mut Carried,
        stop: &CancellationToken,
        abandon: &CancellationToken,
        spawned: &mut JoinSet<bool>,
    ) -> Result<usize, SqlError> {
        if self
            .permits
            .as_ref()
            .is_some_and(OperatorPermits::saturated)
        {
            return Ok(0);
        }
        let (tenants, scanned) = if let Some(tenants) = carried.take(self.poll_interval) {
            (tenants, false)
        } else {
            let tenants = tokio::select! {
                biased;
                () = stop.cancelled() => return Ok(0),
                tenants = work.due_tenants() => tenants?,
            };
            carried.scanned_at = Some(Instant::now());
            (tenants, true)
        };
        let due = tenants.len();
        let mut claimed = 0;
        let mut pending = tenants;
        let mut first_pass = due > 1;
        'round: while !pending.is_empty() {
            let mut next = Vec::with_capacity(pending.len());
            let tenants_in_pass = pending.len();
            let mut pass = pending.into_iter().enumerate();
            while let Some((visited, tenant)) = pass.next() {
                if stop.is_cancelled()
                    || self
                        .permits
                        .as_ref()
                        .is_some_and(OperatorPermits::saturated)
                {
                    carried.keep(tenant, pass, next);
                    break 'round;
                }
                let share = if first_pass {
                    1
                } else {
                    self.capacity
                        .available_permits()
                        .div_ceil(tenants_in_pass - visited)
                };
                let mut slots = Vec::with_capacity(share);
                while slots.len() < share {
                    let Ok(slot) = Arc::clone(&self.capacity).try_acquire_owned() else {
                        break;
                    };
                    slots.push(slot);
                }
                if slots.is_empty() {
                    carried.keep(tenant, pass, next);
                    break 'round;
                }
                let mut delivery = Vec::new();
                if let Some(permits) = &self.permits {
                    while delivery.len() < slots.len() {
                        let Some(permit) = permits.try_acquire(tenant) else {
                            break;
                        };
                        delivery.push(permit);
                    }
                    slots.truncate(delivery.len());
                    if slots.is_empty() {
                        continue;
                    }
                }
                let limit = u32::try_from(slots.len()).unwrap_or(u32::MAX);
                let claims = match self.claim(work.as_ref(), tenant, limit, stop).await {
                    Ok(claims) => claims,
                    Err(error) => {
                        tracing::warn!(
                            capability = W::CAPABILITY.as_str(),
                            tenant = %tenant,
                            %error,
                            "claim failed; continuing with the next tenant"
                        );
                        continue;
                    }
                };
                if stop.is_cancelled() {
                    for claim in &claims {
                        work.release_late(tenant, claim).await;
                    }
                    break 'round;
                }
                if claims.len() == slots.len() {
                    next.push(tenant);
                }
                let mut delivery = delivery.into_iter();
                for (claim, slot) in claims.into_iter().zip(slots) {
                    claimed += 1;
                    let permit = delivery.next();
                    let process = Arc::clone(work).process(
                        tenant,
                        claim,
                        stop.clone(),
                        abandon.clone(),
                        Instant::now(),
                    );
                    spawned.spawn(async move {
                        let _held = (slot, permit);
                        process.await
                    });
                }
            }
            pending = next;
            first_pass = false;
        }
        tracing::debug!(
            capability = W::CAPABILITY.as_str(),
            due,
            scanned,
            claimed,
            "claim round"
        );
        Ok(claimed)
    }

    /// Claim up to `limit` of `tenant`'s next items in one committed
    /// transaction.
    ///
    /// Opening the transaction and the claim race `stop`; when `stop` wins
    /// the uncommitted transaction is dropped, which rolls it back. The
    /// commit is not raced, so committed claims are always returned and the
    /// caller must release them if `stop` fired meanwhile.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the transaction or claim fails.
    async fn claim<W: LeasedWork>(
        &self,
        work: &W,
        tenant: DataTenantId,
        limit: u32,
        stop: &CancellationToken,
    ) -> Result<Vec<W::Claim>, SqlError> {
        let uncommitted = async {
            let mut conn = self.postgres.tenant_conn(tenant).await?;
            let claims = work.claim(&mut conn, limit).await?;
            Ok::<_, SqlError>((conn, claims))
        };
        let (conn, claims) = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(Vec::new()),
            claimed = uncommitted => claimed?,
        };
        conn.commit().await?;
        Ok(claims)
    }

    /// Resolve when `wake` is notified, or never when the loop has no wake
    /// signal, so the idle wait falls back to the poll interval.
    async fn woken(wake: Option<&Notify>) {
        match wake {
            Some(wake) => wake.notified().await,
            None => std::future::pending().await,
        }
    }

    /// Publish the in-flight gauge of `W`.
    fn record_active<W: LeasedWork>(&self, active: usize) {
        if let Some(gauge) = W::ACTIVE_GAUGE {
            metrics::gauge!(gauge).set(f64::from(u32::try_from(active).unwrap_or(u32::MAX)));
        }
    }
}

/// The due tenants one claim loop carries from a round that ran out of
/// capacity into the next, and when it last read the due tenants.
///
/// Carrying lets a saturated loop keep claiming from tenants it already
/// knows have work without repeating the cross-tenant due read each time a
/// slot frees. The list holds only tenants the last read returned whose
/// every claim since has filled its share; a claim that came back short
/// drops its tenant, and an empty list forces a read. The list goes stale a
/// poll interval after that read, so a tenant that became due meanwhile is
/// discovered within one poll interval.
#[derive(Debug, Default)]
struct Carried {
    /// Tenants to claim from next, in rotation order.
    tenants: Vec<DataTenantId>,
    /// When the loop last read the due tenants; `None` before the first read.
    scanned_at: Option<Instant>,
}

impl Carried {
    /// Take the carried tenants for the next round, or `None` when the round
    /// must read the due tenants: nothing is carried, or the last read is at
    /// least `stale_after` old. Either way the carried list is left empty.
    fn take(&mut self, stale_after: Duration) -> Option<Vec<DataTenantId>> {
        let tenants = std::mem::take(&mut self.tenants);
        let fresh = self
            .scanned_at
            .is_some_and(|scanned_at| scanned_at.elapsed() < stale_after);
        (fresh && !tenants.is_empty()).then_some(tenants)
    }

    /// Carry `stopped_at`, then the rest of the pass, then the tenants that
    /// already filled their share this pass, replacing anything carried.
    fn keep(
        &mut self,
        stopped_at: DataTenantId,
        rest: impl Iterator<Item = (usize, DataTenantId)>,
        filled: Vec<DataTenantId>,
    ) {
        self.tenants = std::iter::once(stopped_at)
            .chain(rest.map(|(_, tenant)| tenant))
            .chain(filled)
            .collect();
    }
}

/// The outcome label both claim capabilities log and count for a settlement.
///
/// Returns `label` when a fenced run or dispatch settlement applied, otherwise
/// `stale_lease`, because a lost lease means another claimant owns the row.
pub(super) const fn settled(settlement: Settlement, label: &'static str) -> &'static str {
    match settlement {
        Settlement::Applied => label,
        Settlement::StaleLease => "stale_lease",
    }
}

/// Whether a finished item was refused by a full shared resource; an item
/// that panicked is logged, and its lease expires into a reclaim.
fn reap<W: LeasedWork>(finished: Result<bool, JoinError>) -> bool {
    match finished {
        Ok(refused) => refused,
        Err(error) => {
            if error.is_panic() {
                tracing::error!(capability = W::CAPABILITY.as_str(), %error, "leased work panicked; its lease will expire");
            }
            false
        }
    }
}

/// Unit tests of the carried due-tenant list.
#[cfg(test)]
mod tests {
    use super::*;

    /// A loop that has never read the due tenants carries nothing to claim.
    #[test]
    fn nothing_is_carried_before_the_first_read() {
        let mut carried = Carried::default();
        assert_eq!(carried.take(Duration::from_secs(60)), None);
    }

    /// Carried tenants resume the rotation where the round stopped, then
    /// the tenants that filled their share, and are taken while fresh.
    #[test]
    fn a_fresh_carry_resumes_the_rotation() {
        let [a, b, c, d] = std::array::from_fn(|_| DataTenantId::new_v7());
        let mut carried = Carried {
            tenants: Vec::new(),
            scanned_at: Some(Instant::now()),
        };
        carried.keep(b, [(2, c), (3, d)].into_iter(), vec![a]);
        assert_eq!(
            carried.take(Duration::from_secs(60)),
            Some(vec![b, c, d, a])
        );
        assert_eq!(
            carried.take(Duration::from_secs(60)),
            None,
            "taking empties the carry, so an unrefilled carry forces a read"
        );
    }

    /// A carry a poll interval past its read forces a fresh read and is
    /// discarded.
    #[test]
    fn a_stale_carry_forces_a_read() {
        let mut carried = Carried {
            tenants: vec![DataTenantId::new_v7()],
            scanned_at: Some(Instant::now()),
        };
        assert_eq!(carried.take(Duration::ZERO), None);
        assert!(carried.tenants.is_empty());
    }
}
