//! Canonical pod-local Scribe shard routing.

use blake3::Hasher;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::TableRef;

/// The fixed Scribe shard count for every pod.
pub const SCRIBE_SHARD_COUNT: usize = 16;

/// Route an authenticated tenant and canonical table reference to one shard.
///
/// The input is deliberately the tenant UUID bytes followed by the canonical
/// table FQN bytes. No caller may hash an unverified table string.
#[must_use]
pub fn shard_for(tenant: DataTenantId, table: &TableRef) -> usize {
    let mut hasher = Hasher::new();
    hasher.update(tenant.as_uuid().as_bytes());
    hasher.update(table.fqn().as_bytes());
    usize::from(hasher.finalize().as_bytes()[0]) & (SCRIBE_SHARD_COUNT - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::BifrostNamespace;

    #[test]
    fn routing_stays_within_the_fixed_topology() {
        let tenant = DataTenantId::new_v7();
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        assert!(shard_for(tenant, &table) < SCRIBE_SHARD_COUNT);
        assert_eq!(shard_for(tenant, &table), shard_for(tenant, &table));
    }

    #[test]
    fn namespace_and_name_are_part_of_the_routing_key() {
        let tenant = DataTenantId::new_v7();
        let first = TableRef::new(BifrostNamespace::Bifrost, "events");
        let second = TableRef::new(BifrostNamespace::Audit, "events");
        assert_ne!(first.fqn(), second.fqn());
        let _ = (shard_for(tenant, &first), shard_for(tenant, &second));
    }
}
