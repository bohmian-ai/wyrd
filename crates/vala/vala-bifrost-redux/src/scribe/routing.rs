//! Canonical pod-local Scribe shard routing.

use blake3::Hasher;
use uuid::Uuid;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::TableRef;

/// The fixed Scribe shard count for every pod.
pub const SCRIBE_SHARD_COUNT: usize = 16;

/// Routes one append batch to its owning shard lane.
///
/// The batch id is included in the routing key so that a hot (tenant, table)
/// pair spreads its write load across all sixteen shard lanes while a client
/// retry carrying the same `batch_id` deterministically returns to the lane
/// that holds its in-memory (`synced_not_inserted`) and WAL dedup state —
/// exactly-once is lane-local by construction.
///
/// Correctness is gateway-independent: the fenced-publication `AppendSliceId`
/// uniqueness check is the sole authoritative cross-pod exactly-once mechanism.
/// Scribe storage identities (`node_id`, `writer_epoch`, `shard_id`) prevent
/// cross-pod publication collisions without any gateway routing contract.
/// Gateway batch-identity affinity is an optional efficiency optimization that
/// reduces duplicate buffering and WAL work; it must not be described as a
/// correctness requirement.
///
/// The input bytes hashed are:
/// 1. Authenticated tenant UUID bytes.
/// 2. Canonical table FQN bytes. No caller may hash an unverified table string.
/// 3. The `batch_id` UUID bytes.
#[must_use]
pub fn shard_for(tenant: DataTenantId, table: &TableRef, batch_id: Uuid) -> usize {
    let mut hasher = Hasher::new();
    hasher.update(tenant.as_uuid().as_bytes());
    hasher.update(table.fqn().as_bytes());
    hasher.update(batch_id.as_bytes());
    usize::from(hasher.finalize().as_bytes()[0]) & (SCRIBE_SHARD_COUNT - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::BifrostNamespace;

    /// AC22/AC25 unit owner: routing is exactly an equality function of
    /// `(tenant, canonical table, batch_id)`, a retried batch returns to the
    /// lane holding its dedup state, and deterministic distinct batch ids for
    /// one hot (tenant, table) pair can reach every one of the sixteen lanes.
    ///
    /// The three clauses are the complete production contract for
    /// [`shard_for`]: nothing outside the routing key may influence the lane,
    /// no observation may change it, and the fixed topology must not be
    /// effectively narrower than sixteen.
    #[test]
    fn scribe_routing_equality_retry_and_all_lanes_are_exact() {
        let tenant = DataTenantId::new(
            Uuid::parse_str("0192f000-0000-7000-8000-00000000a1a1").expect("fixed UUID is valid"),
        )
        .expect("fixed tenant UUID is a valid DataTenantId");
        let other_tenant = DataTenantId::new(
            Uuid::parse_str("0192f000-0000-7000-8000-00000000b2b2").expect("fixed UUID is valid"),
        )
        .expect("fixed tenant UUID is a valid DataTenantId");
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        let other_table = TableRef::new(BifrostNamespace::Bifrost, "events_other");
        let namespace_twin = TableRef::new(BifrostNamespace::Audit, "events");
        let batch_id =
            Uuid::parse_str("12345678-1234-5678-1234-567812345678").expect("fixed UUID is valid");

        // Equality: the same three inputs always select the same lane, and the
        // lane is inside the fixed topology.
        let route = shard_for(tenant, &table, batch_id);
        assert!(
            route < SCRIBE_SHARD_COUNT,
            "routing must stay inside the fixed sixteen-lane topology, got {route}"
        );
        for _ in 0..64 {
            assert_eq!(
                shard_for(tenant, &table, batch_id),
                route,
                "routing must be a pure equality function of its key"
            );
        }

        // Retry: replaying the recorded batch id after unrelated traffic still
        // returns to the lane that owns its `synced_not_inserted` and WAL
        // dedup state.
        for offset in 0..256_u128 {
            let _ = shard_for(other_tenant, &other_table, Uuid::from_u128(offset));
        }
        assert_eq!(
            shard_for(tenant, &table, batch_id),
            route,
            "a retried batch must land on the shard that recorded it"
        );

        // Every component of the key participates: changing the tenant, the
        // table name, or the namespace must be able to move the lane, and none
        // of the three may be silently ignored.
        for (component, probe) in [
            (
                "table name",
                Box::new(|batch: Uuid| shard_for(tenant, &other_table, batch))
                    as Box<dyn Fn(Uuid) -> usize>,
            ),
            (
                "table namespace",
                Box::new(|batch: Uuid| shard_for(tenant, &namespace_twin, batch)),
            ),
            (
                "tenant",
                Box::new(|batch: Uuid| shard_for(other_tenant, &table, batch)),
            ),
        ] {
            let diverged = (0..256_u128).any(|ordinal| {
                let batch = Uuid::from_u128(ordinal);
                probe(batch) != shard_for(tenant, &table, batch)
            });
            assert!(
                diverged,
                "{component} must feed the routing key: varying it never changed a lane"
            );
        }

        let mut reached = [false; SCRIBE_SHARD_COUNT];
        for ordinal in 0..4_096_u128 {
            reached[shard_for(tenant, &table, Uuid::from_u128(ordinal))] = true;
        }
        let unreached: Vec<usize> = (0..SCRIBE_SHARD_COUNT)
            .filter(|lane| !reached[*lane])
            .collect();
        assert!(
            unreached.is_empty(),
            "a hot table must be able to reach all sixteen lanes; unreached: {unreached:?}"
        );
    }
}
