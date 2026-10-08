//! The one claim loop the Verifier runner and the Operator worker share.
//!
//! Both capabilities drain a durable, tenant-scoped, leased queue the same
//! way: claim one item per due tenant each round in its own committed
//! transaction that races shutdown, then spawn the claimed item. A round
//! considers every tenant with claimable work; there is no tenant limit. Only
//! Operator delivery takes execution permits before claiming. An item refused
//! by a full shared resource pauses claiming until a running item finishes.
//! On shutdown admit no further claim, give in-flight work the drain grace,
//! then cancel and let it release its lease. [`ClaimLoop`] owns that
//! mechanism; each capability implements [`LeasedWork`] to supply only its
//! due-tenant list, claim, process-and-settle, and late-claim release steps.

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    /// item finishes. Once an item reports a shared-resource refusal, rounds
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
        while !stop.is_cancelled() {
            #[cfg(feature = "test-support")]
            if let Some(crash) = &self.crash {
                crash.check(W::CAPABILITY);
            }
            paused &= !spawned.is_empty();
            let claimed = if paused {
                0
            } else {
                match self.claim_round(work, &stop, &abandon, &mut spawned).await {
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

    /// Claim at most one item for each due tenant.
    ///
    /// Only Operator work takes delivery permits before the claim and drops
    /// them unused when the tenant has nothing claimable. A claim committed after `stop`
    /// fired is released at once instead of being spawned. Returns the number
    /// of items claimed and spawned. A tenant whose claim fails is logged and
    /// skipped, so one tenant's failure never starves the rest of the round;
    /// its rolled-back claim consumes no attempt.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the due-tenant list fails.
    async fn claim_round<W: LeasedWork>(
        &self,
        work: &Arc<W>,
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
        let tenants = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(0),
            tenants = work.due_tenants() => tenants?,
        };
        let due = tenants.len();
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
            let claim = match self.claim(work.as_ref(), tenant, stop).await {
                Ok(Some(claim)) => claim,
                Ok(None) => continue,
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
                work.release_late(tenant, &claim).await;
                break;
            }
            claimed += 1;
            let process = Arc::clone(work).process(
                tenant,
                claim,
                stop.clone(),
                abandon.clone(),
                Instant::now(),
            );
            spawned.spawn(async move {
                let _permit = permit;
                process.await
            });
        }
        tracing::debug!(
            capability = W::CAPABILITY.as_str(),
            due,
            claimed,
            "claim round"
        );
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
        if let Some(gauge) = W::ACTIVE_GAUGE {
            metrics::gauge!(gauge).set(f64::from(u32::try_from(active).unwrap_or(u32::MAX)));
        }
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
