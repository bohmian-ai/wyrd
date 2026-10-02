//! The one claim loop the Verifier runner and the Operator worker share.
//!
//! Both capabilities drain a durable, tenant-scoped, leased queue the same
//! way: claim one item per due tenant each round in its own committed
//! transaction that races shutdown, then spawn the claimed item. Only Operator
//! delivery takes execution permits before claiming. On shutdown admit no
//! further claim, give in-flight work the drain grace, then cancel and let it
//! release its lease. [`ClaimLoop`] owns that mechanism; each capability
//! implements [`LeasedWork`] to supply only its due-tenant list, claim,
//! process-and-settle, and late-claim release steps.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::task::{JoinError, JoinSet};
use tokio_util::sync::CancellationToken;
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::verifier_runs::Settlement;
use wyrd_sql::{SqlError, TenantConn, WyrdPostgres};

#[cfg(feature = "test-support")]
use super::CapabilityCrash;
use super::RuntimeLimits;
use super::health::RuntimeCapability;
use super::permits::OperatorPermits;

/// Tenants examined per claim round, most overdue first.
const TENANTS_PER_ROUND: i64 = 64;

/// The capability-specific steps of one leased queue.
///
/// Implemented by the Verifier runner and the Operator worker; the claim
/// loop owns durable claims, the stop race, draining, and reaping.
pub(super) trait LeasedWork: Send + Sync + 'static {
    /// One claimed item and its lease.
    type Claim: Send + 'static;

    /// Health slot, crash switch, and log label of this capability.
    const CAPABILITY: RuntimeCapability;
    /// Gauge publishing this capability's in-flight items.
    const ACTIVE_GAUGE: &'static str;

    /// Up to `limit` tenants with claimable work, most overdue first.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the cross-tenant read fails.
    fn due_tenants(
        &self,
        limit: i64,
    ) -> impl Future<Output = Result<Vec<DataTenantId>, SqlError>> + Send;

    /// Claim the tenant's next item inside `conn`'s uncommitted transaction.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the claim fails.
    fn claim(
        &self,
        conn: &mut TenantConn<'_>,
    ) -> impl Future<Output = Result<Option<Self::Claim>, SqlError>> + Send;

    /// Execute a claimed item and apply its single fenced settlement.
    ///
    /// `abandon` cancels the item past the drain grace; a retryable failure
    /// observed after `stop` should be released rather than charged.
    fn process(
        self: Arc<Self>,
        tenant: DataTenantId,
        claim: Self::Claim,
        stop: CancellationToken,
        abandon: CancellationToken,
    ) -> impl Future<Output = ()> + Send;

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
    /// Idle wait between empty claim rounds.
    poll_interval: Duration,
    /// How long shutdown waits for in-flight items before cancelling them.
    drain_grace: Duration,
    /// Test-only crash switch checked every loop turn.
    #[cfg(feature = "test-support")]
    crash: Option<CapabilityCrash>,
}

impl ClaimLoop {
    /// Build a loop claiming through `postgres` under `permits` and `limits`.
    pub(super) fn new(
        postgres: WyrdPostgres,
        permits: Option<OperatorPermits>,
        limits: &RuntimeLimits,
    ) -> Self {
        Self {
            postgres,
            permits,
            poll_interval: limits.poll_interval,
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
    /// Each turn claims one item per due tenant and spawns it; Operator work
    /// holds its delivery permit. An empty round waits the poll interval or until an
    /// item finishes. On `stop` no further claim is admitted (an uncommitted
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
        while !stop.is_cancelled() {
            #[cfg(feature = "test-support")]
            if let Some(crash) = &self.crash {
                crash.check(W::CAPABILITY);
            }
            let claimed = match self.claim_round(work, &stop, &abandon, &mut spawned).await {
                Ok(claimed) => claimed,
                Err(error) => {
                    tracing::warn!(capability = W::CAPABILITY.as_str(), %error, "claim round failed");
                    0
                }
            };
            while let Some(finished) = spawned.try_join_next() {
                reap::<W>(finished);
            }
            self.record_active::<W>(spawned.len());
            if claimed > 0 {
                continue;
            }
            tokio::select! {
                () = stop.cancelled() => {}
                () = tokio::time::sleep(self.poll_interval) => {}
                Some(finished) = spawned.join_next(), if !spawned.is_empty() => reap::<W>(finished),
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

    /// Claim at most one item for each due tenant.
    ///
    /// Only Operator work takes delivery permits before the claim and drops
    /// them unused when the tenant has nothing claimable. A claim committed after `stop`
    /// fired is released at once instead of being spawned. Returns the number
    /// of items claimed and spawned.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the due-tenant list or a claim fails; items
    /// claimed earlier in the round are already spawned.
    async fn claim_round<W: LeasedWork>(
        &self,
        work: &Arc<W>,
        stop: &CancellationToken,
        abandon: &CancellationToken,
        spawned: &mut JoinSet<()>,
    ) -> Result<usize, SqlError> {
        if self
            .permits
            .as_ref()
            .is_some_and(OperatorPermits::saturated)
        {
            return Ok(0);
        }
        let tenants = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(0),
            tenants = work.due_tenants(TENANTS_PER_ROUND) => tenants?,
        };
        let mut claimed = 0;
        for tenant in tenants {
            if stop.is_cancelled()
                || self
                    .permits
                    .as_ref()
                    .is_some_and(OperatorPermits::saturated)
            {
                break;
            }
            let permit = if let Some(permits) = &self.permits {
                let Some(permit) = permits.try_acquire(tenant) else {
                    continue;
                };
                Some(permit)
            } else {
                None
            };
            let Some(claim) = self.claim(work.as_ref(), tenant, stop).await? else {
                continue;
            };
            if stop.is_cancelled() {
                work.release_late(tenant, &claim).await;
                break;
            }
            claimed += 1;
            let process = Arc::clone(work).process(tenant, claim, stop.clone(), abandon.clone());
            spawned.spawn(async move {
                let _permit = permit;
                process.await;
            });
        }
        Ok(claimed)
    }

    /// Claim `tenant`'s next item in its own committed transaction.
    ///
    /// Opening the transaction and the claim race `stop`; when `stop` wins
    /// the uncommitted transaction is dropped, which rolls it back. The
    /// commit is not raced, so a committed claim is always returned and the
    /// caller must release it if `stop` fired meanwhile.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the transaction or claim fails.
    async fn claim<W: LeasedWork>(
        &self,
        work: &W,
        tenant: DataTenantId,
        stop: &CancellationToken,
    ) -> Result<Option<W::Claim>, SqlError> {
        let uncommitted = async {
            let mut conn = self.postgres.tenant_conn(tenant).await?;
            let claim = work.claim(&mut conn).await?;
            Ok::<_, SqlError>((conn, claim))
        };
        let (conn, claim) = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(None),
            claimed = uncommitted => claimed?,
        };
        conn.commit().await?;
        Ok(claim)
    }

    /// Publish the in-flight gauge of `W`.
    fn record_active<W: LeasedWork>(&self, active: usize) {
        metrics::gauge!(W::ACTIVE_GAUGE).set(f64::from(u32::try_from(active).unwrap_or(u32::MAX)));
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

/// Log a spawned item that panicked; its lease expires into a reclaim.
fn reap<W: LeasedWork>(finished: Result<(), JoinError>) {
    if let Err(error) = finished
        && error.is_panic()
    {
        tracing::error!(capability = W::CAPABILITY.as_str(), %error, "leased work panicked; its lease will expire");
    }
}
