//! Portable bounded assignment for sealed fragments.

use super::fragment::SealedScanFragment;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;
use wyrd_spec::vala::api::{NodeId, OracleCapabilitiesV1};

/// Closed validation failure for portable worker assignment.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentError {
    /// The configured remote-worker bound exceeds the protocol's 63-worker ceiling.
    #[error("Oracle assignment supports at most 63 remote workers")]
    WorkerLimit,
}

/// Eligible Oracle worker and its immutable capability advertisement.
#[derive(Debug, Clone)]
pub struct OracleNode {
    /// Stable node identity.
    pub node_id: NodeId,
    /// Advertised worker capabilities.
    pub capabilities: OracleCapabilitiesV1,
}

/// Versioned consistent-hash assignment algorithm.
#[derive(Debug, Clone, Copy, Default)]
pub struct PortableAssignmentV1;

impl PortableAssignmentV1 {
    /// Assigns fragments deterministically to a bounded worker subset.
    ///
    /// # Errors
    ///
    /// Returns [`AssignmentError::WorkerLimit`] rather than silently changing a
    /// configuration that requests more than 63 remote workers.
    pub fn assign(
        &self,
        fragments: &[SealedScanFragment],
        eligible: &BTreeMap<NodeId, OracleNode>,
        leader: NodeId,
        max_workers: usize,
    ) -> Result<HashMap<NodeId, Vec<SealedScanFragment>>, AssignmentError> {
        if max_workers > 63 {
            return Err(AssignmentError::WorkerLimit);
        }
        let worker_count = max_workers.min(fragments.len());
        let mut nodes: Vec<NodeId> = eligible
            .keys()
            .copied()
            .filter(|id| *id != leader)
            .collect();
        nodes.sort_by_key(|id| id.as_uuid());
        nodes.truncate(worker_count);
        let mut selected = vec![leader];
        selected.extend(nodes);
        let mut result = HashMap::new();
        for fragment in fragments {
            let mut hash = Sha256::new();
            hash.update(b"wyrd.oracle.assignment.v1\0");
            hash.update(fragment.binding.as_bytes());
            hash.update(fragment.fragment_id.as_bytes());
            let digest = hash.finalize();
            let bucket = u64::from_le_bytes([
                digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6],
                digest[7],
            ]);
            let selected_len = u64::try_from(selected.len()).unwrap_or(1);
            let index = usize::try_from(bucket % selected_len).unwrap_or_default();
            result
                .entry(selected[index])
                .or_insert_with(Vec::new)
                .push(fragment.clone());
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oracle::fragment::{SealedScanFile, SealedSourceTier};

    /// Builds one closed fragment with a stable identity.
    fn fragment(id: &str) -> SealedScanFragment {
        SealedScanFragment {
            fragment_id: id.to_owned(),
            binding: "binding".to_owned(),
            tier: SealedSourceTier::HotSealed,
            pinned_digest: "manifest".to_owned(),
            files: vec![SealedScanFile {
                location: "binding/a".to_owned(),
                row_groups: Vec::new(),
                size_bytes: 1,
                estimated_rows: 1,
            }],
            projection: Vec::new(),
            predicates: Vec::new(),
            schema_fingerprint: "schema".to_owned(),
            estimated_rows: 1,
            estimated_bytes: 1,
            deadline_unix_ms: i64::MAX,
        }
    }

    /// Stable eligibility order produces one node-keyed assignment and bounded fanout.
    #[test]
    fn oracle_assignment_is_stable_and_never_uses_more_nodes_than_fragments() {
        let leader = NodeId::new(uuid::Uuid::from_u128(1));
        let worker = NodeId::new(uuid::Uuid::from_u128(2));
        let capabilities = OracleCapabilitiesV1 {
            peer_protocol_version: 1,
            storage_protocol_version: 1,
            cpu_cores: 1.0,
            memory_budget_bytes: 2 * 1024 * 1024 * 1024,
            cpu_cores_per_slot: 1.0,
            memory_bytes_per_slot: 2 * 1024 * 1024 * 1024,
            raw_slots: 1,
            usable_slots: 1,
            supported_classes: vec![wyrd_spec::vala::api::QueryClass::Interactive],
            max_workers_per_query: 1,
        };
        let eligible = BTreeMap::from([
            (
                leader,
                OracleNode {
                    node_id: leader,
                    capabilities: capabilities.clone(),
                },
            ),
            (
                worker,
                OracleNode {
                    node_id: worker,
                    capabilities,
                },
            ),
        ]);
        let fragments = vec![fragment("one"), fragment("two")];
        let first = PortableAssignmentV1
            .assign(&fragments, &eligible, leader, 63)
            .expect("protocol worker bound is valid");
        let second = PortableAssignmentV1
            .assign(&fragments, &eligible, leader, 63)
            .expect("protocol worker bound is valid");
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        assert_eq!(
            first
                .get(&leader)
                .expect("leader vector")
                .iter()
                .map(|fragment| fragment.fragment_id.as_str())
                .collect::<Vec<_>>(),
            vec!["two"]
        );
        assert_eq!(
            first
                .get(&worker)
                .expect("worker vector")
                .iter()
                .map(|fragment| fragment.fragment_id.as_str())
                .collect::<Vec<_>>(),
            vec!["one"]
        );
    }

    /// Assignment rejects an out-of-range worker bound instead of silently clamping it.
    #[test]
    fn oracle_assignment_rejects_more_than_sixty_three_workers() {
        let leader = NodeId::new(uuid::Uuid::from_u128(1));
        assert_eq!(
            PortableAssignmentV1.assign(&[fragment("one")], &BTreeMap::new(), leader, 64),
            Err(AssignmentError::WorkerLimit)
        );
    }
}
