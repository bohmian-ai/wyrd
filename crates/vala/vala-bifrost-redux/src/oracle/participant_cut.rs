//! Immutable, role-fenced participant selection for one Oracle query attempt.

use std::time::Duration;

use chrono::{DateTime, Utc};
use thiserror::Error;
use wyrd_spec::vala::api::{
    ClusterCapabilities, ClusterRole, ClusterRoleLease, NodeId, QueryClass, QueryId,
};

use crate::cluster::ClusterSnapshot;

/// A participant retained by one query attempt with its exact role incarnation.
#[derive(Debug, Clone, PartialEq)]
pub struct OracleQueryParticipant {
    /// Physical node identity.
    pub node_id: NodeId,
    /// Private endpoint selected from the snapshot.
    pub endpoint: String,
    /// Exact role represented by this participant.
    pub role: ClusterRole,
    /// Exact role-incarnation fence.
    pub fencing_token: u64,
    /// Validated capability document used for the attempt.
    pub capabilities: ClusterCapabilities,
}

/// Immutable membership and deadline used by every stage of one query attempt.
#[derive(Debug, Clone)]
pub struct OracleQueryAttemptCut {
    /// Source snapshot observation time.
    observed_at: DateTime<Utc>,
    /// Stable attempt identity.
    attempt_id: QueryId,
    /// Ready Oracle participants sorted by node identity.
    oracles: Vec<OracleQueryParticipant>,
    /// Ready Scribe participants sorted by node identity.
    scribes: Vec<OracleQueryParticipant>,
    /// Request-local leader retained from the Oracle set.
    leader: OracleQueryParticipant,
    /// Absolute wall-clock query deadline.
    deadline: DateTime<Utc>,
}

/// Participant-cut validation failure.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum OracleQueryAttemptCutError {
    /// The snapshot is older than the accepted role-liveness window.
    #[error("cluster snapshot is stale")]
    StaleSnapshot,
    /// The snapshot contains a duplicate `(node, role)` identity.
    #[error("cluster snapshot contains duplicate role identities")]
    DuplicateRole,
    /// A participant is not ready or lacks a complete role fence.
    #[error("cluster participant is not ready and role-fenced")]
    IncompleteRole,
    /// A capability document does not match the participant role or query class.
    #[error("cluster participant capability is incompatible")]
    IncompatibleCapability,
    /// The request-local leader is absent from the ready Oracle set.
    #[error("request-local leader is not a ready Oracle participant")]
    LeaderUnavailable,
    /// The absolute deadline has already elapsed.
    #[error("query attempt deadline has elapsed")]
    DeadlineElapsed,
}

impl OracleQueryAttemptCut {
    /// Freezes one fresh snapshot into the exact participants for a query attempt.
    ///
    /// # Errors
    ///
    /// Returns [`OracleQueryAttemptCutError`] for stale or duplicate membership,
    /// incomplete role fences, incompatible capabilities, an unavailable leader,
    /// or an elapsed deadline.
    pub fn try_from_snapshot(
        snapshot: &ClusterSnapshot,
        attempt_id: QueryId,
        leader_node_id: NodeId,
        query_class: QueryClass,
        deadline: DateTime<Utc>,
        now: DateTime<Utc>,
        freshness: Duration,
    ) -> Result<Self, OracleQueryAttemptCutError> {
        if deadline <= now {
            return Err(OracleQueryAttemptCutError::DeadlineElapsed);
        }
        let maximum_age = chrono::Duration::from_std(freshness)
            .map_err(|_| OracleQueryAttemptCutError::StaleSnapshot)?;
        if snapshot.observed_at() > now || now - snapshot.observed_at() > maximum_age {
            return Err(OracleQueryAttemptCutError::StaleSnapshot);
        }
        if snapshot.has_duplicate_roles() {
            return Err(OracleQueryAttemptCutError::DuplicateRole);
        }

        let mut oracles = Self::participants(snapshot.live_oracles(), query_class)?;
        let mut scribes = Self::participants(snapshot.live_scribes(), query_class)?;
        oracles.sort_by_key(|participant| participant.node_id);
        scribes.sort_by_key(|participant| participant.node_id);
        let leader = oracles
            .iter()
            .find(|participant| participant.node_id == leader_node_id)
            .cloned()
            .ok_or(OracleQueryAttemptCutError::LeaderUnavailable)?;
        Ok(Self {
            observed_at: snapshot.observed_at(),
            attempt_id,
            oracles,
            scribes,
            leader,
            deadline,
        })
    }

    /// Validates and projects one role-filtered participant slice.
    ///
    /// # Errors
    ///
    /// Returns [`OracleQueryAttemptCutError`] when readiness, fencing, role
    /// capability, or query-class compatibility is invalid.
    fn participants(
        leases: Vec<&ClusterRoleLease>,
        query_class: QueryClass,
    ) -> Result<Vec<OracleQueryParticipant>, OracleQueryAttemptCutError> {
        leases
            .into_iter()
            .map(|lease| {
                if !lease.ready || lease.fencing_token == 0 || lease.address.trim().is_empty() {
                    return Err(OracleQueryAttemptCutError::IncompleteRole);
                }
                lease
                    .capabilities
                    .validate_for_role(lease.key.role)
                    .map_err(|_| OracleQueryAttemptCutError::IncompatibleCapability)?;
                if let ClusterCapabilities::OracleV1(capabilities) = &lease.capabilities
                    && !capabilities.supported_classes.contains(&query_class)
                {
                    return Err(OracleQueryAttemptCutError::IncompatibleCapability);
                }
                Ok(OracleQueryParticipant {
                    node_id: lease.key.node_id,
                    endpoint: lease.address.clone(),
                    role: lease.key.role,
                    fencing_token: lease.fencing_token,
                    capabilities: lease.capabilities.clone(),
                })
            })
            .collect()
    }

    /// Returns the source snapshot observation time.
    #[must_use]
    pub const fn observed_at(&self) -> DateTime<Utc> {
        self.observed_at
    }

    /// Returns the stable attempt identity.
    #[must_use]
    pub const fn attempt_id(&self) -> QueryId {
        self.attempt_id
    }

    /// Returns all exact ready Oracle participants.
    #[must_use]
    pub fn oracles(&self) -> &[OracleQueryParticipant] {
        &self.oracles
    }

    /// Returns all exact ready Scribe participants.
    #[must_use]
    pub fn scribes(&self) -> &[OracleQueryParticipant] {
        &self.scribes
    }

    /// Returns the request-local Oracle leader.
    #[must_use]
    pub const fn leader(&self) -> &OracleQueryParticipant {
        &self.leader
    }

    /// Returns the immutable absolute query deadline.
    #[must_use]
    pub const fn deadline(&self) -> DateTime<Utc> {
        self.deadline
    }

    /// Looks up an exact Oracle role incarnation without refreshing membership.
    #[must_use]
    pub fn oracle(&self, node_id: NodeId, fencing_token: u64) -> Option<&OracleQueryParticipant> {
        self.oracles.iter().find(|participant| {
            participant.node_id == node_id && participant.fencing_token == fencing_token
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_spec::vala::api::{ClusterNodeKey, OracleCapabilitiesV1, ScribeCapabilitiesV1};

    /// Builds one ready role lease with a valid capability document.
    fn lease(node: u128, role: ClusterRole, fence: u64) -> ClusterRoleLease {
        let now = Utc::now();
        ClusterRoleLease {
            key: ClusterNodeKey {
                node_id: NodeId::new(uuid::Uuid::from_u128(node)),
                role,
            },
            address: format!("http://node-{node}"),
            fencing_token: fence,
            capability_version: 1,
            capabilities: match role {
                ClusterRole::Oracle => ClusterCapabilities::OracleV1(OracleCapabilitiesV1 {
                    peer_protocol_version: 1,
                    storage_protocol_version: 1,
                    cpu_cores: 1.0,
                    memory_budget_bytes: 1024,
                    cpu_cores_per_slot: 1.0,
                    memory_bytes_per_slot: 1024,
                    raw_slots: 1,
                    usable_slots: 1,
                    supported_classes: vec![QueryClass::Interactive],
                    max_workers_per_query: 1,
                }),
                ClusterRole::Scribe => ClusterCapabilities::ScribeV1(ScribeCapabilitiesV1 {
                    tail_protocol_version: 1,
                }),
            },
            ready: true,
            started_at: now,
            heartbeat_at: now,
        }
    }

    /// One snapshot becomes one deterministic Oracle and Scribe participant cut.
    ///
    /// # Panics
    ///
    /// Panics when the valid fixture does not construct the expected cut.
    #[test]
    fn cut_is_sorted_and_retains_exact_leader_fence() {
        let now = Utc::now();
        let snapshot = ClusterSnapshot::observed(
            vec![
                lease(2, ClusterRole::Oracle, 7),
                lease(3, ClusterRole::Scribe, 8),
                lease(1, ClusterRole::Oracle, 9),
            ],
            now,
        );
        let cut = OracleQueryAttemptCut::try_from_snapshot(
            &snapshot,
            QueryId::new(uuid::Uuid::now_v7()),
            NodeId::new(uuid::Uuid::from_u128(2)),
            QueryClass::Interactive,
            now + chrono::Duration::seconds(5),
            now,
            Duration::from_secs(15),
        )
        .expect("fresh compatible cut");
        assert_eq!(
            cut.oracles()[0].node_id,
            NodeId::new(uuid::Uuid::from_u128(1))
        );
        assert_eq!(cut.leader().fencing_token, 7);
        assert_eq!(cut.scribes().len(), 1);
    }

    /// Stale and duplicate membership fail rather than silently changing a cut.
    ///
    /// # Panics
    ///
    /// Panics when either invalid fixture is unexpectedly accepted.
    #[test]
    fn cut_rejects_stale_and_duplicate_membership() {
        let now = Utc::now();
        let member = lease(1, ClusterRole::Oracle, 1);
        let stale =
            ClusterSnapshot::observed(vec![member.clone()], now - chrono::Duration::seconds(16));
        let result = OracleQueryAttemptCut::try_from_snapshot(
            &stale,
            QueryId::new(uuid::Uuid::now_v7()),
            member.key.node_id,
            QueryClass::Interactive,
            now + chrono::Duration::seconds(1),
            now,
            Duration::from_secs(15),
        );
        assert!(matches!(
            result,
            Err(OracleQueryAttemptCutError::StaleSnapshot)
        ));
        let duplicate = ClusterSnapshot::observed(vec![member.clone(), member.clone()], now);
        let result = OracleQueryAttemptCut::try_from_snapshot(
            &duplicate,
            QueryId::new(uuid::Uuid::now_v7()),
            member.key.node_id,
            QueryClass::Interactive,
            now + chrono::Duration::seconds(1),
            now,
            Duration::from_secs(15),
        );
        assert!(matches!(
            result,
            Err(OracleQueryAttemptCutError::DuplicateRole)
        ));
    }
}
