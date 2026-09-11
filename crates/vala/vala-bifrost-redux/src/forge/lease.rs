//! Fenced leases for one Forge tenant/table maintenance scope.
//!
//! Lease ownership is coordinated through the operator pool. The fencing token
//! is carried into tenant transactions so an expired or superseded owner cannot
//! commit a durable state transition after another Forge process takes over.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use uuid::Uuid;
use wyrd_spec::DataTenantId;

use vala_sql::OperatorPool;
use vala_sql::TenantConn;

use super::error::ForgeError;

/// Build the shared lease namespace for all maintenance stages of a table.
pub fn forge_lease_key(tenant: DataTenantId, logical_namespace: &str, table_name: &str) -> String {
    format!("forge:table:{tenant}:{logical_namespace}:{table_name}")
}

#[derive(Debug, Clone)]
/// A Forge table lease and its database fencing token.
pub struct ForgeLease {
    /// Stable tenant/table lease namespace.
    pub lease_key: String,
    /// UUID of the process that owns the lease.
    pub owner: Uuid,
    /// Monotonic token asserted by every durable transition.
    pub fencing_token: i64,
    /// Whether this acquisition replaced an expired different owner.
    takeover: bool,
    ttl: Duration,
    /// Shared monotonic origin used by every clone of this lease generation.
    clock_started: Instant,
    /// Shared nanosecond offset of the most recent successful renewal.
    confirmed_at_nanos: Arc<AtomicU64>,
}

impl ForgeLease {
    /// Try to acquire a lease, returning `None` when another owner holds it.
    ///
    /// # Errors
    ///
    /// Returns a lease error when the operator query fails or the TTL cannot
    /// be represented by the database interval.
    pub async fn acquire(
        operator_pool: &OperatorPool,
        lease_key: String,
        owner: Uuid,
        ttl: Duration,
    ) -> Result<Option<Self>, ForgeError> {
        let seconds = i64::try_from(ttl.as_secs()).map_err(|_| ForgeError::InvalidConfig {
            detail: "lease TTL is too large".to_owned(),
        })?;
        let acquisition = vala_sql::queries::maintenance_leases::try_acquire_lease(
            operator_pool,
            &lease_key,
            owner,
            seconds,
        )
        .await
        .map_err(ForgeError::Lease)?;
        Ok(acquisition.map(|acquisition| {
            let clock_started = Instant::now();
            Self {
                lease_key,
                owner,
                fencing_token: acquisition.fencing_token,
                takeover: acquisition.takeover,
                ttl,
                clock_started,
                confirmed_at_nanos: Arc::new(AtomicU64::new(0)),
            }
        }))
    }

    /// Reports whether acquisition replaced an expired row owned by another owner.
    #[must_use]
    pub const fn takeover(&self) -> bool {
        self.takeover
    }

    /// Renew this lease and refresh its local confirmation time.
    ///
    /// Returns `false` when the owner or fencing token no longer matches.
    pub async fn renew(&mut self, operator_pool: &OperatorPool) -> Result<bool, ForgeError> {
        let seconds = i64::try_from(self.ttl.as_secs()).map_err(|_| ForgeError::InvalidConfig {
            detail: "lease TTL is too large".to_owned(),
        })?;
        let renewed = vala_sql::queries::maintenance_leases::renew_lease_fenced(
            operator_pool,
            &self.lease_key,
            self.owner,
            self.fencing_token,
            seconds,
        )
        .await
        .map_err(ForgeError::Lease)?;
        if renewed {
            self.confirmed_at_nanos
                .store(self.elapsed_nanos(), Ordering::Release);
        }
        Ok(renewed)
    }

    /// Release this lease only if the owner and fencing token still match.
    pub async fn release(&self, operator_pool: &OperatorPool) -> Result<bool, ForgeError> {
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            operator_pool,
            &self.lease_key,
            self.owner,
            self.fencing_token,
        )
        .await
        .map_err(ForgeError::Lease)
    }

    /// Return the locally estimated time remaining before the lease expires.
    pub fn remaining(&self) -> Duration {
        let confirmed_at = self.confirmed_at_nanos.load(Ordering::Acquire);
        self.ttl.saturating_sub(Duration::from_nanos(
            self.elapsed_nanos().saturating_sub(confirmed_at),
        ))
    }

    /// Check whether the remaining lease covers a required commit window.
    pub fn commit_window_fits(&self, required: Duration) -> bool {
        self.remaining() > required
    }

    /// Renew the lease and fail closed if its fence cannot be confirmed.
    pub async fn require_fence(&mut self, operator_pool: &OperatorPool) -> Result<(), ForgeError> {
        if self.remaining().is_zero() {
            return Err(ForgeError::FenceLost {
                lease_key: self.lease_key.clone(),
            });
        }
        if !self.renew(operator_pool).await? {
            return Err(ForgeError::FenceLost {
                lease_key: self.lease_key.clone(),
            });
        }
        Ok(())
    }

    /// Assert this lease inside a tenant transaction immediately before commit.
    ///
    /// A lost lease is returned as [`ForgeError::FenceLost`], while other SQL
    /// failures retain their original error classification.
    pub async fn assert_transaction_fence(
        &self,
        conn: &mut TenantConn<'_>,
    ) -> Result<(), ForgeError> {
        match vala_sql::queries::maintenance_leases::assert_fence(
            conn,
            &self.lease_key,
            self.owner,
            self.fencing_token,
        )
        .await
        {
            Ok(()) => Ok(()),
            Err(vala_sql::SqlError::InvariantViolation { .. }) => Err(ForgeError::FenceLost {
                lease_key: self.lease_key.clone(),
            }),
            Err(error) => Err(ForgeError::Sql(error)),
        }
    }

    /// Returns the saturated monotonic nanosecond offset for shared renewal state.
    fn elapsed_nanos(&self) -> u64 {
        u64::try_from(self.clock_started.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lease_key_is_shared_by_all_table_maintenance_concerns() {
        let tenant = DataTenantId::new_v7();
        let key = forge_lease_key(tenant, "vala.traces", "spans");
        assert_eq!(key, format!("forge:table:{tenant}:vala.traces:spans"));
        assert_eq!(key, forge_lease_key(tenant, "vala.traces", "spans"));
    }
}
