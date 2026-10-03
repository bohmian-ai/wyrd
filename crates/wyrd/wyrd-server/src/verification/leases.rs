//! Lease renewal for in-flight Verifier runs.
//!
//! While a run executes or writes its result, [`LeaseRenewal`] keeps its lease
//! on the PostgreSQL clock: every tick renews all of one tenant's in-flight
//! leases in one statement, extending each once a third of it has passed. A
//! renewal that finds a run's token gone cancels that run's work at once, and
//! a run whose lease could not be renewed for a whole lease length is
//! cancelled too, because its lease may have expired. Renewal is an engine
//! mechanic: it evaluates no permission and writes no audit.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::verifier_runs::{LeaseToken, VerifierRunQueue};
use wyrd_sql::{SqlError, WyrdPostgres};

/// One held lease: the work's cancellation and when it was last known held.
struct Held {
    /// Cancelled when the lease is lost.
    lost: CancellationToken,
    /// When the claim or the latest renewal confirmed the lease.
    confirmed: Instant,
}

/// Owner of every lease this process's runner holds, and their renewal.
pub(super) struct LeaseRenewal {
    /// Wyrd Postgres owner that opens each tenant's renewal transaction.
    postgres: WyrdPostgres,
    /// Queue owning the renewal statement.
    queue: VerifierRunQueue,
    /// Lease length every claim and renewal grants.
    lease: Duration,
    /// Held leases by tenant, behind one short synchronous lock.
    held: Mutex<HashMap<DataTenantId, HashMap<LeaseToken, Held>>>,
}

/// A lease registered for renewal until this guard drops.
pub(super) struct HeldLease {
    /// The renewal owner the lease is removed from on drop.
    renewal: Arc<LeaseRenewal>,
    /// Tenant of the run.
    tenant: DataTenantId,
    /// The claim's fencing token.
    token: LeaseToken,
    /// Cancelled when the lease is lost.
    lost: CancellationToken,
}

impl HeldLease {
    /// The token cancelled when this lease is lost or its work abandoned.
    pub(super) fn lost(&self) -> &CancellationToken {
        &self.lost
    }
}

impl Drop for HeldLease {
    /// Stop renewing the lease.
    ///
    /// # Panics
    /// Panics when the held-lease lock is poisoned.
    fn drop(&mut self) {
        let mut held = self.renewal.held.lock().expect("held lease lock");
        if let Some(tenant) = held.get_mut(&self.tenant) {
            tenant.remove(&self.token);
            if tenant.is_empty() {
                held.remove(&self.tenant);
            }
        }
    }
}

impl LeaseRenewal {
    /// Build a renewal owner granting `lease` through `postgres`.
    pub(super) fn new(postgres: WyrdPostgres, queue: VerifierRunQueue, lease: Duration) -> Self {
        Self {
            postgres,
            queue,
            lease,
            held: Mutex::default(),
        }
    }

    /// Register the lease `token` of `tenant`, just claimed, for renewal.
    ///
    /// The returned lease's `lost` token is a child of `abandon`, so work
    /// watching it stops when the lease is lost or the drain abandons it.
    ///
    /// # Panics
    /// Panics when the held-lease lock is poisoned.
    pub(super) fn hold(
        self: &Arc<Self>,
        tenant: DataTenantId,
        token: LeaseToken,
        abandon: &CancellationToken,
    ) -> HeldLease {
        let lost = abandon.child_token();
        self.held
            .lock()
            .expect("held lease lock")
            .entry(tenant)
            .or_default()
            .insert(
                token,
                Held {
                    lost: lost.clone(),
                    confirmed: Instant::now(),
                },
            );
        HeldLease {
            renewal: Arc::clone(self),
            tenant,
            token,
            lost,
        }
    }

    /// Renew held leases every sixth of a lease until `stop` is cancelled.
    ///
    /// Ticking at a sixth of the lease renews each lease after a third and
    /// before half of it has passed.
    pub(super) async fn run(&self, stop: CancellationToken) {
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                () = tokio::time::sleep(self.lease / 6) => self.renew().await,
            }
        }
    }

    /// Renew every tenant's held leases, one statement per tenant.
    ///
    /// A token the statement does not return was reclaimed or settled, and
    /// its work is cancelled. A tenant whose renewal fails keeps its leases
    /// until they have gone unconfirmed for a whole lease length, after which
    /// their work is cancelled because the lease may have expired.
    ///
    /// # Panics
    /// Panics when the held-lease lock is poisoned.
    async fn renew(&self) {
        let tenants: Vec<(DataTenantId, Vec<LeaseToken>)> = self
            .held
            .lock()
            .expect("held lease lock")
            .iter()
            .map(|(tenant, held)| (*tenant, held.keys().copied().collect()))
            .collect();
        for (tenant, tokens) in tenants {
            let renewed = self.renew_tenant(tenant, &tokens).await;
            let mut held = self.held.lock().expect("held lease lock");
            let Some(leases) = held.get_mut(&tenant) else {
                continue;
            };
            match renewed {
                Ok(kept) => {
                    for token in &tokens {
                        let Some(lease) = leases.get_mut(token) else {
                            continue;
                        };
                        if kept.contains(token) {
                            lease.confirmed = Instant::now();
                        } else {
                            tracing::warn!(tenant_id = %tenant, "a held Verifier run lease was taken; cancelling its work");
                            lease.lost.cancel();
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(tenant_id = %tenant, %error, "Verifier lease renewal failed");
                    for lease in leases.values() {
                        if lease.confirmed.elapsed() >= self.lease {
                            lease.lost.cancel();
                        }
                    }
                }
            }
        }
    }

    /// Renew `tokens` of `tenant` in one committed statement, returning the
    /// tokens that still hold their runs.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the connection, statement, or commit fails.
    async fn renew_tenant(
        &self,
        tenant: DataTenantId,
        tokens: &[LeaseToken],
    ) -> Result<Vec<LeaseToken>, SqlError> {
        let lease = chrono::Duration::from_std(self.lease)
            .unwrap_or_else(|_| chrono::Duration::minutes(10));
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        let kept = self.queue.renew(&mut conn, tokens, lease).await?;
        conn.commit().await?;
        Ok(kept)
    }
}
