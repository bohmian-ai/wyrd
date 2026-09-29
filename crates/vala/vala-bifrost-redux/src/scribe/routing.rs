//! Canonical pod-local Scribe shard routing.

use blake3::Hasher;
use uuid::Uuid;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::TableRef;

/// Lane count test WAL framing helpers use to stamp segment shard ids.
///
/// Production routes with the running shard count; replay maps any recorded
/// id onto that count, so a helper's lane only needs to be a valid id.
#[cfg(any(test, feature = "test-support"))]
pub const TEST_SHARD_COUNT: usize = 16;

/// Routes one append batch to its owning shard lane among `shard_count`.
///
/// The batch id is included in the routing key so that a hot (tenant, table)
/// pair spreads its write load across every shard lane while a client
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
///
/// The lane is the little-endian first eight digest bytes modulo
/// `shard_count`, so a power-of-two count keeps the low bits of the first
/// byte. A lane is recomputed, never stored, so a count change only moves
/// future batches.
///
/// # Panics
///
/// Panics when `shard_count` is zero; [`crate::scribe::geometry::ScribeGeometry`]
/// refuses a zero count before any shard exists.
#[must_use]
pub fn shard_for(
    tenant: DataTenantId,
    table: &TableRef,
    batch_id: Uuid,
    shard_count: usize,
) -> usize {
    let mut hasher = Hasher::new();
    hasher.update(tenant.as_uuid().as_bytes());
    hasher.update(table.fqn().as_bytes());
    hasher.update(batch_id.as_bytes());
    let digest = hasher.finalize();
    let prefix = u64::from_le_bytes(
        digest.as_bytes()[..8]
            .try_into()
            .expect("a blake3 digest is longer than eight bytes"),
    );
    let lanes = u64::try_from(shard_count).expect("a shard count fits in u64");
    usize::try_from(prefix % lanes).expect("a lane below the shard count fits in usize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::BifrostNamespace;

    /// AC22/AC25 unit owner: routing is exactly an equality function of
    /// `(tenant, canonical table, batch_id)`, a retried batch returns to the
    /// lane holding its dedup state, and deterministic distinct batch ids for
    /// one hot (tenant, table) pair can reach every lane of a sixteen-lane pod.
    ///
    /// The three clauses are the complete production contract for
    /// [`shard_for`]: nothing outside the routing key may influence the lane,
    /// no observation may change it, and the topology must not be effectively
    /// narrower than its configured count.
    #[test]
    fn scribe_routing_equality_retry_and_all_lanes_are_exact() {
        /// Lane count the reachability clause is proven against.
        const LANES: usize = 16;
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
        let route = shard_for(tenant, &table, batch_id, LANES);
        assert!(
            route < LANES,
            "routing must stay inside the configured topology, got {route}"
        );
        for _ in 0..64 {
            assert_eq!(
                shard_for(tenant, &table, batch_id, LANES),
                route,
                "routing must be a pure equality function of its key"
            );
        }

        // Retry: replaying the recorded batch id after unrelated traffic still
        // returns to the lane that owns its `synced_not_inserted` and WAL
        // dedup state.
        for offset in 0..256_u128 {
            let _ = shard_for(other_tenant, &other_table, Uuid::from_u128(offset), LANES);
        }
        assert_eq!(
            shard_for(tenant, &table, batch_id, LANES),
            route,
            "a retried batch must land on the shard that recorded it"
        );

        // Every component of the key participates: changing the tenant, the
        // table name, or the namespace must be able to move the lane, and none
        // of the three may be silently ignored.
        for (component, probe) in [
            (
                "table name",
                Box::new(|batch: Uuid| shard_for(tenant, &other_table, batch, LANES))
                    as Box<dyn Fn(Uuid) -> usize>,
            ),
            (
                "table namespace",
                Box::new(|batch: Uuid| shard_for(tenant, &namespace_twin, batch, LANES)),
            ),
            (
                "tenant",
                Box::new(|batch: Uuid| shard_for(other_tenant, &table, batch, LANES)),
            ),
        ] {
            let diverged = (0..256_u128).any(|ordinal| {
                let batch = Uuid::from_u128(ordinal);
                probe(batch) != shard_for(tenant, &table, batch, LANES)
            });
            assert!(
                diverged,
                "{component} must feed the routing key: varying it never changed a lane"
            );
        }

        let mut reached = [false; LANES];
        for ordinal in 0..4_096_u128 {
            reached[shard_for(tenant, &table, Uuid::from_u128(ordinal), LANES)] = true;
        }
        let unreached: Vec<usize> = (0..LANES).filter(|lane| !reached[*lane]).collect();
        assert!(
            unreached.is_empty(),
            "a hot table must be able to reach all sixteen lanes; unreached: {unreached:?}"
        );
    }
}
