use std::time::{Duration, Instant};
use uuid::Uuid;
use wyrd_spec::DataTenantId;

use vala_sql::OperatorPool;

use super::error::ForgeError;

pub fn forge_lease_key(tenant: DataTenantId, logical_namespace: &str, table_name: &str) -> String {
    format!("forge:table:{tenant}:{logical_namespace}:{table_name}")
}

#[derive(Debug, Clone)]
pub struct ForgeLease {
    pub lease_key: String,
    pub owner: Uuid,
    pub fencing_token: i64,
    ttl: Duration,
    confirmed_at: Instant,
}

impl ForgeLease {
    pub async fn acquire(
        operator_pool: &OperatorPool,
        lease_key: String,
        owner: Uuid,
        ttl: Duration,
    ) -> Result<Option<Self>, ForgeError> {
        let seconds = i64::try_from(ttl.as_secs()).map_err(|_| ForgeError::InvalidConfig {
            detail: "lease TTL is too large".to_owned(),
        })?;
        let fencing_token = vala_sql::queries::maintenance_leases::try_acquire_lease(
            operator_pool,
            &lease_key,
            owner,
            seconds,
        )
        .await
        .map_err(ForgeError::Lease)?;
        Ok(fencing_token.map(|fencing_token| Self {
            lease_key,
            owner,
            fencing_token,
            ttl,
            confirmed_at: Instant::now(),
        }))
    }

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
            self.confirmed_at = Instant::now();
        }
        Ok(renewed)
    }

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

    pub fn remaining(&self) -> Duration {
        self.ttl.saturating_sub(self.confirmed_at.elapsed())
    }

    pub fn commit_window_fits(&self, required: Duration) -> bool {
        self.remaining() > required
    }

    pub fn renewal_interval(&self) -> Duration {
        self.ttl
            .checked_div(4)
            .unwrap_or_else(|| Duration::from_millis(1))
            .max(Duration::from_millis(1))
    }

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
