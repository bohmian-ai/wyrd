//! Replica-local circuit breaking for failing deployments.
//!
//! Health is process-local runtime state, never a deployment resource field:
//! consecutive failures move a deployment to the back of plan order on this
//! replica for a cooldown and never mutate tenant configuration. A cooling
//! deployment is still attempted when no healthy candidate answers, so health
//! reorders attempts and never refuses a call by itself.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::time::Instant;
use wyrd_spec::DataTenantId;
use wyrd_spec::ids::ProviderDeploymentName;

/// Default time a tripped deployment stays behind healthy candidates.
const DEFAULT_COOLDOWN: Duration = Duration::from_secs(30);

/// Default consecutive failures that trip a deployment, matching Envoy's
/// `consecutive_5xx` outlier-detection default.
const DEFAULT_THRESHOLD: u32 = 5;

/// Breaker state of one deployment that has failed since its last success.
#[derive(Debug, Clone, Copy)]
struct Breaker {
    /// Failures since the last success or lapsed cooldown.
    failures: u32,
    /// Instant the cooldown lapses, set once `failures` reaches the threshold.
    cooling_until: Option<Instant>,
}

/// Replica-local breaker map keyed by tenant and deployment.
///
/// Entries exist only for deployments that failed since their last success
/// and are dropped by a success or a lapsed cooldown, so the map is bounded by
/// configured deployments.
#[derive(Debug)]
pub struct DeploymentHealth {
    /// Consecutive failures that start a cooldown.
    threshold: u32,
    /// How long a tripped deployment stays behind healthy candidates.
    cooldown: Duration,
    /// Breaker state per failing deployment.
    breakers: Mutex<HashMap<(DataTenantId, ProviderDeploymentName), Breaker>>,
}

impl Default for DeploymentHealth {
    /// Builds an empty breaker map with the default threshold and cooldown.
    fn default() -> Self {
        Self::new(DEFAULT_THRESHOLD, DEFAULT_COOLDOWN)
    }
}

impl DeploymentHealth {
    /// Builds an empty breaker map that trips after `threshold` consecutive
    /// failures for `cooldown`; a zero threshold trips on the first failure.
    #[must_use]
    pub fn new(threshold: u32, cooldown: Duration) -> Self {
        Self {
            threshold: threshold.max(1),
            cooldown,
            breakers: Mutex::new(HashMap::new()),
        }
    }

    /// True unless `deployment` is cooling down; a lapsed cooldown drops the
    /// entry so the deployment starts again from zero failures.
    #[must_use]
    pub fn is_healthy(&self, tenant: DataTenantId, deployment: &ProviderDeploymentName) -> bool {
        let mut breakers = self.lock();
        let key = (tenant, deployment.clone());
        match breakers.get(&key).and_then(|breaker| breaker.cooling_until) {
            Some(until) if Instant::now() < until => false,
            Some(_) => {
                breakers.remove(&key);
                true
            }
            None => true,
        }
    }

    /// Counts one failure of `deployment` and starts its cooldown once the
    /// consecutive count reaches the threshold.
    pub fn record_failure(&self, tenant: DataTenantId, deployment: &ProviderDeploymentName) {
        let mut breakers = self.lock();
        let breaker = breakers
            .entry((tenant, deployment.clone()))
            .or_insert(Breaker {
                failures: 0,
                cooling_until: None,
            });
        breaker.failures = breaker.failures.saturating_add(1);
        if breaker.failures >= self.threshold {
            breaker.cooling_until = Some(Instant::now() + self.cooldown);
        }
    }

    /// Clears `deployment`'s failures and any cooldown after it answered.
    pub fn record_success(&self, tenant: DataTenantId, deployment: &ProviderDeploymentName) {
        let mut breakers = self.lock();
        if !breakers.is_empty() {
            breakers.remove(&(tenant, deployment.clone()));
        }
    }

    /// Locks the map, recovering from a poisoned lock since entries are
    /// independent per deployment.
    fn lock(&self) -> MutexGuard<'_, HashMap<(DataTenantId, ProviderDeploymentName), Breaker>> {
        self.breakers.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deployment name used by every breaker test.
    fn name() -> ProviderDeploymentName {
        ProviderDeploymentName::new("dep").expect("name")
    }

    /// Failures below the threshold leave the deployment healthy, a success
    /// resets the count, the threshold trips it, and the cooldown lapses.
    ///
    /// # Panics
    /// Panics when the breaker trips early, ignores a success, or never lapses.
    #[tokio::test(start_paused = true)]
    async fn consecutive_failures_trip_and_success_resets() {
        let health = DeploymentHealth::new(3, Duration::from_secs(30));
        let tenant = DataTenantId::SYSTEM_OWNER;
        health.record_failure(tenant, &name());
        health.record_failure(tenant, &name());
        health.record_success(tenant, &name());
        health.record_failure(tenant, &name());
        health.record_failure(tenant, &name());
        assert!(
            health.is_healthy(tenant, &name()),
            "the success reset the count"
        );
        health.record_failure(tenant, &name());
        assert!(
            !health.is_healthy(tenant, &name()),
            "three in a row trip it"
        );
        tokio::time::advance(Duration::from_secs(30)).await;
        assert!(health.is_healthy(tenant, &name()), "the cooldown lapsed");
        health.record_failure(tenant, &name());
        assert!(
            health.is_healthy(tenant, &name()),
            "a lapse restarts the count"
        );
    }
}
