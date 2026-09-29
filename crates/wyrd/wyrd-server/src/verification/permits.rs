//! Global and per-tenant Verifier execution capacity.
//!
//! The runner takes one [`VerifierPermit`] before it claims a run and holds it
//! for the whole execution and settlement, so no more than the global ceiling
//! runs in this process and no tenant holds more than its share. Permits count
//! capacity only; the durable queue remains the sole record of which runs exist.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use wyrd_spec::DataTenantId;

/// Capacity held by one in-flight Verifier execution.
///
/// Dropping it returns both the tenant and the global slot.
#[derive(Debug)]
pub struct VerifierPermit {
    /// The tenant's slot.
    _tenant: OwnedSemaphorePermit,
    /// The process-wide slot.
    _global: OwnedSemaphorePermit,
}

/// Owner of the process-wide and per-tenant Verifier ceilings.
#[derive(Debug)]
pub struct VerifierPermits {
    /// Process-wide ceiling.
    global: Arc<Semaphore>,
    /// Global capacity, kept to report active work.
    global_capacity: usize,
    /// Ceiling per tenant.
    per_tenant: usize,
    /// One semaphore per tenant that currently holds or is taking a permit.
    tenants: Mutex<HashMap<DataTenantId, Arc<Semaphore>>>,
}

impl VerifierPermits {
    /// Build ceilings of `global` executions in total and `per_tenant` per tenant.
    #[must_use]
    pub fn new(global: usize, per_tenant: usize) -> Self {
        Self {
            global: Arc::new(Semaphore::new(global)),
            global_capacity: global,
            per_tenant,
            tenants: Mutex::new(HashMap::new()),
        }
    }

    /// Take one slot for `tenant` without waiting.
    ///
    /// Takes the tenant slot first, so a saturated tenant never holds a
    /// global slot another tenant could use. Returns `None` when either
    /// ceiling is reached. Idle tenants are pruned on every call, so the map
    /// holds only tenants with in-flight work.
    ///
    /// # Panics
    /// Panics when the tenant map lock was poisoned by a panic while held,
    /// which no code path inside the lock can cause.
    pub fn try_acquire(&self, tenant: DataTenantId) -> Option<VerifierPermit> {
        let mut tenants = self
            .tenants
            .lock()
            .expect("invariant: the permit map lock is never held across a panic");
        tenants.retain(|_, semaphore| semaphore.available_permits() < self.per_tenant);
        let semaphore = tenants
            .entry(tenant)
            .or_insert_with(|| Arc::new(Semaphore::new(self.per_tenant)));
        let tenant_permit = Arc::clone(semaphore).try_acquire_owned().ok()?;
        let global = Arc::clone(&self.global).try_acquire_owned().ok()?;
        Some(VerifierPermit {
            _tenant: tenant_permit,
            _global: global,
        })
    }

    /// Whether every global slot is taken.
    #[must_use]
    pub fn saturated(&self) -> bool {
        self.global.available_permits() == 0
    }

    /// Executions currently holding a permit.
    #[must_use]
    pub fn active(&self) -> usize {
        self.global_capacity - self.global.available_permits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tenant stops at its own ceiling while another still gets capacity,
    /// the global ceiling bounds both, and dropping a permit frees its slots.
    ///
    /// # Panics
    /// Panics when a ceiling is not enforced or a released slot is not reusable.
    #[test]
    fn tenant_and_global_ceilings_bound_acquisition() {
        let permits = VerifierPermits::new(6, 4);
        let busy = DataTenantId::new_v7();
        let other = DataTenantId::new_v7();
        let mut held: Vec<_> = (0..4)
            .map(|_| permits.try_acquire(busy).expect("busy tenant slot"))
            .collect();
        assert!(permits.try_acquire(busy).is_none(), "tenant ceiling holds");
        held.push(permits.try_acquire(other).expect("other tenant progresses"));
        held.push(permits.try_acquire(other).expect("other tenant progresses"));
        assert!(permits.saturated());
        assert!(
            permits.try_acquire(DataTenantId::new_v7()).is_none(),
            "global ceiling holds"
        );
        assert_eq!(permits.active(), 6);
        held.truncate(3);
        assert_eq!(permits.active(), 3);
        assert!(
            permits.try_acquire(busy).is_some(),
            "a released slot is reusable"
        );
    }
}
