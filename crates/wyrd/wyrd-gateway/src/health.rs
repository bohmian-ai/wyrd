//! Temporary removal of failing deployments from selection.
//!
//! Health is process-local runtime state, never a deployment resource field:
//! a failure removes the deployment from new attempts on this replica for a
//! cooldown and never mutates tenant configuration.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::time::Instant;
use wyrd_spec::DataTenantId;
use wyrd_spec::ids::ProviderDeploymentName;

/// Default time a failed deployment stays out of selection.
const DEFAULT_COOLDOWN: Duration = Duration::from_secs(30);

/// Replica-local cooldown map keyed by tenant and deployment.
///
/// Entries exist only for deployments that failed and are dropped once their
/// cooldown lapses, so the map is bounded by configured deployments.
#[derive(Debug)]
pub struct DeploymentHealth {
    /// How long a failure removes a deployment.
    cooldown: Duration,
    /// Instant each unhealthy deployment becomes eligible again.
    unhealthy: Mutex<HashMap<(DataTenantId, ProviderDeploymentName), Instant>>,
}

impl Default for DeploymentHealth {
    /// Builds an empty health map with the default cooldown.
    fn default() -> Self {
        Self::new(DEFAULT_COOLDOWN)
    }
}

impl DeploymentHealth {
    /// Builds an empty health map with `cooldown`.
    #[must_use]
    pub fn new(cooldown: Duration) -> Self {
        Self {
            cooldown,
            unhealthy: Mutex::new(HashMap::new()),
        }
    }

    /// True unless `deployment` failed within the cooldown; a lapsed entry is
    /// removed.
    #[must_use]
    pub fn is_healthy(&self, tenant: DataTenantId, deployment: &ProviderDeploymentName) -> bool {
        let mut unhealthy = self.lock();
        let key = (tenant, deployment.clone());
        match unhealthy.get(&key) {
            Some(until) if Instant::now() < *until => false,
            Some(_) => {
                unhealthy.remove(&key);
                true
            }
            None => true,
        }
    }

    /// Removes `deployment` from selection for the cooldown.
    pub fn mark_unhealthy(&self, tenant: DataTenantId, deployment: &ProviderDeploymentName) {
        self.lock()
            .insert((tenant, deployment.clone()), Instant::now() + self.cooldown);
    }

    /// Locks the map, recovering from a poisoned lock since entries are
    /// independent timestamps.
    fn lock(&self) -> MutexGuard<'_, HashMap<(DataTenantId, ProviderDeploymentName), Instant>> {
        self.unhealthy
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}
