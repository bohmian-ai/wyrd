//! Immutable, role-fenced participant selection for one Oracle query attempt.

use std::time::Duration;

use chrono::{DateTime, Utc};
use num_traits::ToPrimitive as _;
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use wyrd_spec::vala::api::{
    ClusterCapabilities, ClusterRole, ClusterRoleLease, NodeId, QueryClass, QueryId,
};

use crate::cluster::ClusterSnapshot;

/// A participant retained by one query attempt with its exact role incarnation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

/// One class-neutral frozen membership and source cut for a query attempt.
///
/// A leader must freeze membership before it knows the query's class, because
/// the class is derived from a physical root that can only be built against the
/// frozen cut. This value is that intermediate: the exact same participants,
/// leader, and deadline the signed cut will carry, validated for readiness,
/// fences, and role capability, but not yet bound to a class.
///
/// [`Self::finalize`] consumes it with the root-derived class. No topology or
/// source refresh happens between the two calls, so the signed cut names the
/// membership the root was planned against.
#[derive(Debug, Clone)]
pub struct OracleQueryAttemptRoster {
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

/// Reduces the eligible remote roster to a bounded, deterministic selection.
///
/// `oracles` arrives sorted and deduplicated by stable node identity. A remote
/// only ever runs an Analytical fragment, so one that does not advertise
/// [`QueryClass::Analytical`] is dropped before selection: rotating onto a
/// replica that cannot admit the work would fail a query a capable replica
/// could have served. The leader is exempt — it is the pod deciding, not a
/// participant being chosen.
///
/// The starting position rotates by the attempt identity, so two concurrent
/// queries on the same cut spread across different replicas while one query
/// always selects the same replicas on every node that recomputes the cut. The
/// leader's own advertised `max_workers_per_query` bounds only the remotes, and
/// zero means local execution.
///
/// The result stays in the original node-identity order so every downstream
/// digest, exchange assignment, and dispatch sees one canonical sequence.
fn select_bounded_workers(
    oracles: &mut Vec<OracleQueryParticipant>,
    leader: &OracleQueryParticipant,
    attempt_id: QueryId,
) {
    oracles.retain(|participant| {
        participant.node_id == leader.node_id || supports_class(participant, QueryClass::Analytical)
    });
    let ClusterCapabilities::OracleV1(capabilities) = &leader.capabilities else {
        return;
    };
    let max_workers = usize::try_from(capabilities.max_workers_per_query).unwrap_or(usize::MAX);
    let eligible = oracles.len().saturating_sub(1);
    let selected = max_workers.min(eligible);
    if selected == eligible {
        return;
    }
    let remotes: Vec<usize> = (0..oracles.len())
        .filter(|index| oracles[*index].node_id != leader.node_id)
        .collect();
    let mut keep = vec![false; oracles.len()];
    if !remotes.is_empty() {
        let rotation =
            usize::try_from(attempt_id.as_uuid().as_u128() % remotes.len() as u128).unwrap_or(0);
        for step in 0..selected {
            keep[remotes[(rotation + step) % remotes.len()]] = true;
        }
    }
    let mut index = 0;
    oracles.retain(|participant| {
        let retained = participant.node_id == leader.node_id || keep[index];
        index += 1;
        retained
    });
}

impl OracleQueryAttemptRoster {
    /// Freezes one fresh snapshot into class-neutral attempt membership.
    ///
    /// The leader is chosen before any class exists, so class compatibility is
    /// not checked here at all. Participants are validated for readiness,
    /// fences, and role capability now, and for the derived class in
    /// [`Self::finalize`]. Selecting a leader that can serve every derivable
    /// class is the caller's concern, upstream of this freeze.
    ///
    /// # Errors
    ///
    /// Returns [`OracleQueryAttemptCutError`] for stale or duplicate membership,
    /// incomplete role fences, incompatible capabilities, an unavailable leader,
    /// or an elapsed deadline.
    pub fn freeze(
        snapshot: &ClusterSnapshot,
        attempt_id: QueryId,
        leader_node_id: NodeId,
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
        // The snapshot projects leases into a map keyed by `(node_id, role)`, so
        // a disagreeing repeat is already collapsed to one arbitrary winner by
        // the time participants are built and cannot be detected downstream.
        // Consult the flag the projection recorded instead: routing a query off
        // a cut that may have selected a stale fence is exactly the split-brain
        // case this refusal exists to prevent.
        if snapshot.has_conflicting_roles() {
            return Err(OracleQueryAttemptCutError::DuplicateRole);
        }
        let mut oracles = Self::participants(snapshot.live_oracles())?;
        let mut scribes = Self::participants(snapshot.live_scribes())?;
        Self::sort_and_deduplicate(&mut oracles)?;
        Self::sort_and_deduplicate(&mut scribes)?;
        let leader = oracles
            .iter()
            .find(|participant| participant.node_id == leader_node_id)
            .cloned()
            .ok_or(OracleQueryAttemptCutError::LeaderUnavailable)?;
        // Bound the fan-out here, once, before the roster is observed by
        // planning, dispatch, reservation, or the signed digest — planning is
        // what decides whether the root distributes, so a replica selected
        // later could never have been planned against. An unselected replica is
        // therefore absent from the cut entirely and cannot receive a request or
        // affect the result.
        select_bounded_workers(&mut oracles, &leader, attempt_id);
        Ok(Self {
            observed_at: snapshot.observed_at(),
            attempt_id,
            oracles,
            scribes,
            leader,
            deadline,
        })
    }

    /// Signs the frozen roster into the immutable cut for one derived class.
    ///
    /// Consumes the roster so no caller can finalize the same membership twice
    /// under two classes.
    ///
    /// # Errors
    ///
    /// Returns [`OracleQueryAttemptCutError::IncompatibleCapability`] when a
    /// frozen participant does not advertise `query_class`.
    pub fn finalize(
        self,
        query_class: QueryClass,
    ) -> Result<OracleQueryAttemptCut, OracleQueryAttemptCutError> {
        let Self {
            observed_at,
            attempt_id,
            oracles,
            scribes,
            leader,
            deadline,
        } = self;
        for participant in oracles.iter().chain(scribes.iter()) {
            if !supports_class(participant, query_class) {
                return Err(OracleQueryAttemptCutError::IncompatibleCapability);
            }
        }
        // Recorded here rather than at the class-neutral freeze: only Analytical
        // work dispatches to a remote, so an Interactive attempt must report the
        // zero workers it actually selected.
        metrics::histogram!("bifrost_oracle_selected_workers").record(
            match query_class {
                QueryClass::Analytical => oracles.len().saturating_sub(1),
                QueryClass::Interactive => 0,
            }
            .to_f64()
            .unwrap_or(f64::MAX),
        );
        Ok(OracleQueryAttemptCut {
            observed_at,
            attempt_id,
            oracles,
            scribes,
            leader,
            deadline,
        })
    }

    /// Returns the selected Oracle participants, leader included.
    #[must_use]
    pub fn oracles(&self) -> &[OracleQueryParticipant] {
        &self.oracles
    }

    /// Returns the frozen ready Scribe participants.
    #[must_use]
    pub fn scribes(&self) -> &[OracleQueryParticipant] {
        &self.scribes
    }

    /// Returns the request-local leader frozen before any class existed.
    #[must_use]
    pub const fn leader(&self) -> &OracleQueryParticipant {
        &self.leader
    }

    /// Returns the stable attempt identity this roster was frozen for.
    #[must_use]
    pub const fn attempt_id(&self) -> QueryId {
        self.attempt_id
    }

    /// Returns the absolute wall-clock deadline frozen with this roster.
    #[must_use]
    pub const fn deadline(&self) -> DateTime<Utc> {
        self.deadline
    }

    /// Returns this frozen membership's canonical digest, before any class.
    ///
    /// Identical to the digest [`OracleQueryAttemptCut::fingerprint`] renders
    /// for the cut this roster finalizes into, so an observation taken at the
    /// physical build can be compared with the terminal attempt's own cut.
    ///
    /// # Panics
    ///
    /// Panics only when a participant collection exceeds the fixed-width
    /// canonical length encoding.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        canonical_fingerprint(
            self.attempt_id,
            self.observed_at,
            self.deadline,
            &self.leader,
            &self.oracles,
            &self.scribes,
        )
    }

    /// Validates and projects one role-filtered participant slice.
    ///
    /// # Errors
    ///
    /// Returns [`OracleQueryAttemptCutError`] when readiness, fencing, or role
    /// capability is invalid. Class compatibility is checked in
    /// [`Self::finalize`], once the root has decided the class.
    fn participants(
        leases: Vec<&ClusterRoleLease>,
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

    /// Sorts stable role identities, collapses exact duplicates, and rejects conflicting fences.
    fn sort_and_deduplicate(
        participants: &mut Vec<OracleQueryParticipant>,
    ) -> Result<(), OracleQueryAttemptCutError> {
        participants.sort_by_key(|participant| {
            (
                participant.node_id,
                match participant.role {
                    ClusterRole::Oracle => 0_u8,
                    ClusterRole::Scribe => 1_u8,
                },
                participant.fencing_token,
                participant.endpoint.clone(),
            )
        });
        let mut deduplicated = Vec::<OracleQueryParticipant>::with_capacity(participants.len());
        for participant in participants.drain(..) {
            if let Some(previous) = deduplicated.last()
                && previous.node_id == participant.node_id
                && previous.role == participant.role
            {
                if previous != &participant {
                    return Err(OracleQueryAttemptCutError::DuplicateRole);
                }
                continue;
            }
            deduplicated.push(participant);
        }
        *participants = deduplicated;
        Ok(())
    }
}

/// Reports whether one frozen participant advertises `query_class`.
///
/// Only an Oracle capability document constrains class; a Scribe participant
/// serves the live tail for either class and is always compatible.
fn supports_class(participant: &OracleQueryParticipant, query_class: QueryClass) -> bool {
    match &participant.capabilities {
        ClusterCapabilities::OracleV1(capabilities) => {
            capabilities.supported_classes.contains(&query_class)
        }
        ClusterCapabilities::ScribeV1(_) => true,
    }
}

/// Renders one frozen membership into its canonical execution-cut digest.
///
/// Shared by the pre-class roster and the finalized cut so both sides of one
/// attempt encode the same facts the same way. Query class is deliberately
/// absent: the digest identifies the membership a build ran against, which is
/// decided before the root exists.
///
/// # Panics
///
/// Panics only when a participant collection exceeds the fixed-width canonical
/// length encoding.
fn canonical_fingerprint(
    attempt_id: QueryId,
    observed_at: DateTime<Utc>,
    deadline: DateTime<Utc>,
    leader: &OracleQueryParticipant,
    oracles: &[OracleQueryParticipant],
    scribes: &[OracleQueryParticipant],
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"wyrd.oracle.participant-cut.v1\0");
    digest.update(attempt_id.as_uuid().as_bytes());
    OracleQueryAttemptCut::hash_timestamp(&mut digest, observed_at);
    OracleQueryAttemptCut::hash_timestamp(&mut digest, deadline);
    digest.update(b"leader\0");
    OracleQueryAttemptCut::hash_participant(&mut digest, leader);
    digest.update(b"oracles\0");
    OracleQueryAttemptCut::hash_len(&mut digest, oracles.len());
    for participant in oracles {
        OracleQueryAttemptCut::hash_participant(&mut digest, participant);
    }
    digest.update(b"scribes\0");
    OracleQueryAttemptCut::hash_len(&mut digest, scribes.len());
    for participant in scribes {
        OracleQueryAttemptCut::hash_participant(&mut digest, participant);
    }
    format!("sha256:{:x}", digest.finalize())
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
        OracleQueryAttemptRoster::freeze(
            snapshot,
            attempt_id,
            leader_node_id,
            deadline,
            now,
            freshness,
        )?
        .finalize(query_class)
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

    /// Returns a stable digest of every execution-relevant immutable cut fact.
    ///
    /// # Panics
    ///
    /// Panics only when an in-process participant or capability collection
    /// exceeds the fixed-width canonical digest encoding.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        canonical_fingerprint(
            self.attempt_id,
            self.observed_at,
            self.deadline,
            &self.leader,
            &self.oracles,
            &self.scribes,
        )
    }

    /// Adds one full-resolution UTC timestamp to the cut digest.
    fn hash_timestamp(digest: &mut Sha256, timestamp: DateTime<Utc>) {
        digest.update(timestamp.timestamp().to_be_bytes());
        digest.update(timestamp.timestamp_subsec_nanos().to_be_bytes());
    }

    /// Adds one exact participant and its validated capability document.
    ///
    /// # Panics
    ///
    /// Panics only when a validated capability advertises more query classes
    /// than the canonical u32 collection length can represent.
    fn hash_participant(digest: &mut Sha256, participant: &OracleQueryParticipant) {
        digest.update(participant.node_id.as_uuid().as_bytes());
        Self::hash_bytes(digest, participant.endpoint.as_bytes());
        digest.update(match participant.role {
            ClusterRole::Scribe => [0],
            ClusterRole::Oracle => [1],
        });
        digest.update(participant.fencing_token.to_be_bytes());
        match &participant.capabilities {
            ClusterCapabilities::ScribeV1(value) => {
                digest.update([0]);
                digest.update(value.tail_protocol_version.to_be_bytes());
            }
            ClusterCapabilities::OracleV1(value) => {
                digest.update([1]);
                digest.update(value.storage_protocol_version.to_be_bytes());
                digest.update(value.cpu_cores.to_bits().to_be_bytes());
                digest.update(value.memory_budget_bytes.to_be_bytes());
                digest.update(value.cpu_cores_per_slot.to_bits().to_be_bytes());
                digest.update(value.memory_bytes_per_slot.to_be_bytes());
                digest.update(value.raw_slots.to_be_bytes());
                digest.update(value.usable_slots.to_be_bytes());
                digest.update(
                    u32::try_from(value.supported_classes.len())
                        .expect("invariant: capability class count fits in u32")
                        .to_be_bytes(),
                );
                for class in &value.supported_classes {
                    digest.update(match class {
                        QueryClass::Interactive => [0],
                        QueryClass::Analytical => [1],
                    });
                }
                digest.update(value.max_workers_per_query.to_be_bytes());
            }
        }
    }

    /// Adds length-delimited bytes to the cut digest.
    ///
    /// # Panics
    ///
    /// Panics only when the byte slice length cannot fit the canonical u64
    /// collection length.
    fn hash_bytes(digest: &mut Sha256, value: &[u8]) {
        Self::hash_len(digest, value.len());
        digest.update(value);
    }

    /// Adds one collection length to the canonical cut digest.
    ///
    /// # Panics
    ///
    /// Panics only when the collection length cannot fit the canonical u64
    /// collection length.
    fn hash_len(digest: &mut Sha256, value: usize) {
        digest.update(
            u64::try_from(value)
                .expect("invariant: participant collection length fits in u64")
                .to_be_bytes(),
        );
    }

    /// Returns the exact number of participants frozen into this cut.
    ///
    /// # Panics
    ///
    /// Panics only if a process constructs a participant slice larger than the
    /// public `u32` protocol counter can represent.
    #[must_use]
    pub fn participant_count(&self) -> u32 {
        u32::try_from(self.oracles.len() + self.scribes.len())
            .expect("invariant: participant cut length fits in u32")
    }

    /// Looks up an exact Oracle role incarnation without refreshing membership.
    #[must_use]
    pub fn oracle(&self, node_id: NodeId, fencing_token: u64) -> Option<&OracleQueryParticipant> {
        self.oracles.iter().find(|participant| {
            participant.node_id == node_id && participant.fencing_token == fencing_token
        })
    }
}

/// Focused invariants for immutable participant selection and fingerprinting.
#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use wyrd_spec::vala::api::{ClusterNodeKey, OracleCapabilitiesV1, ScribeCapabilitiesV1};

    /// Builds one ready role lease with a valid capability document.
    pub(crate) fn lease(node: u128, role: ClusterRole, fence: u64) -> ClusterRoleLease {
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
                    storage_protocol_version: 1,
                    cpu_cores: 1.0,
                    memory_budget_bytes: 1024,
                    cpu_cores_per_slot: 1.0,
                    memory_bytes_per_slot: 1024,
                    raw_slots: 1,
                    usable_slots: 1,
                    supported_classes: vec![QueryClass::Interactive, QueryClass::Analytical],
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

    /// Freezes one Analytical-capable three-Oracle cut led by `leader`.
    ///
    /// The Analytical leader path projects its remote participants straight out
    /// of the frozen cut, so a leasing test needs a cut whose Oracle set is both
    /// Analytical-capable and larger than the leader alone.
    ///
    /// # Panics
    ///
    /// Panics when the fixture snapshot does not freeze, which would make every
    /// assertion built on the returned cut vacuous.
    pub(crate) fn analytical_cut(
        now: DateTime<Utc>,
        leader: u128,
        deadline: DateTime<Utc>,
    ) -> OracleQueryAttemptCut {
        let analytical = |node: u128, fence: u64| {
            let mut role = lease(node, ClusterRole::Oracle, fence);
            // Mutually authenticated by construction: an Analytical participant
            // cut refuses any endpoint that is not `https`.
            role.address = format!("https://node-{node}.invalid/");
            if let ClusterCapabilities::OracleV1(capabilities) = &mut role.capabilities {
                capabilities.supported_classes = vec![QueryClass::Analytical];
                capabilities.max_workers_per_query = 4;
            }
            role
        };
        let snapshot = ClusterSnapshot::observed(
            vec![analytical(leader, 7), analytical(3, 8), analytical(4, 9)],
            now,
        );
        OracleQueryAttemptCut::try_from_snapshot(
            &snapshot,
            QueryId::new(uuid::Uuid::now_v7()),
            NodeId::new(uuid::Uuid::from_u128(leader)),
            QueryClass::Analytical,
            deadline,
            now,
            Duration::from_secs(15),
        )
        .expect("the fixture snapshot freezes one analytical cut")
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

    /// A stale snapshot is refused, and repeated membership collapses or conflicts.
    ///
    /// Duplicate handling mirrors the ported design, which sorts participants,
    /// collapses repeats, and re-sorts for stable ordinals rather than failing:
    /// a registry that lists one node twice is a registry anomaly, not a query
    /// fault, and the collapsed cut is the same set either way. Wyrd tightens
    /// only the case the ported design cannot see, because it collapses on
    /// address alone: two leases sharing `(node_id, role)` but disagreeing on
    /// fencing token or endpoint are a split-brain or stale-lease signal, and
    /// silently keeping whichever sorted first could admit the stale one. That
    /// case fails closed with [`OracleQueryAttemptCutError::DuplicateRole`].
    ///
    /// # Panics
    ///
    /// Panics when a stale snapshot is accepted, when an exact repeat does not
    /// collapse to a single participant, or when a conflicting fence is
    /// admitted.
    #[test]
    fn cut_rejects_stale_snapshot_collapses_repeats_and_refuses_conflicting_fence() {
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
        // An exact repeat is a registry anomaly: collapse it and serve the query.
        let repeated = ClusterSnapshot::observed(vec![member.clone(), member.clone()], now);
        let cut = OracleQueryAttemptCut::try_from_snapshot(
            &repeated,
            QueryId::new(uuid::Uuid::now_v7()),
            member.key.node_id,
            QueryClass::Interactive,
            now + chrono::Duration::seconds(1),
            now,
            Duration::from_secs(15),
        )
        .expect("an exact repeat collapses rather than failing the cut");
        assert_eq!(
            cut.oracles().len(),
            1,
            "the repeated lease must collapse to one participant so no node is assigned twice"
        );
        assert_eq!(cut.oracles()[0].node_id, member.key.node_id);

        // A disagreeing fence for the same node and role is a safety signal.
        let conflicting = lease(1, ClusterRole::Oracle, 2);
        assert_eq!(conflicting.key, member.key, "same node and role by fixture");
        let split_brain = ClusterSnapshot::observed(vec![member.clone(), conflicting], now);
        let result = OracleQueryAttemptCut::try_from_snapshot(
            &split_brain,
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

    /// Every stored execution fact changes the participant-cut fingerprint.
    ///
    /// # Panics
    ///
    /// Panics when the valid fixture cannot be built or any immutable fact is
    /// omitted from the digest.
    #[test]
    pub(in crate::oracle) fn fingerprint_binds_every_immutable_participant_cut_fact() {
        let now = Utc::now();
        let snapshot = ClusterSnapshot::observed(
            vec![
                lease(1, ClusterRole::Oracle, 7),
                lease(2, ClusterRole::Oracle, 8),
                lease(3, ClusterRole::Scribe, 9),
            ],
            now,
        );
        let cut = OracleQueryAttemptCut::try_from_snapshot(
            &snapshot,
            QueryId::new(uuid::Uuid::from_u128(11)),
            NodeId::new(uuid::Uuid::from_u128(1)),
            QueryClass::Interactive,
            now + chrono::Duration::seconds(5),
            now,
            Duration::from_secs(15),
        )
        .expect("fresh compatible cut");
        assert_cut_mutation(&cut, "attempt_id", |changed| {
            changed.attempt_id = QueryId::new(uuid::Uuid::from_u128(12));
        });
        assert_cut_mutation(&cut, "observed_at", |changed| {
            changed.observed_at += chrono::Duration::nanoseconds(1);
        });
        assert_cut_mutation(&cut, "deadline", |changed| {
            changed.deadline += chrono::Duration::nanoseconds(1);
        });
        assert_oracle_participant_fields(&cut, "leader", |changed| &mut changed.leader);
        assert_oracle_participant_fields(&cut, "oracle", |changed| &mut changed.oracles[1]);
        assert_scribe_participant_fields(&cut, "scribe", |changed| &mut changed.scribes[0]);
        assert_cut_mutation(&cut, "oracle_membership", |changed| {
            changed.oracles.pop();
        });
        assert_cut_mutation(&cut, "scribe_membership", |changed| {
            changed.scribes.pop();
        });
        assert_cut_mutation(&cut, "oracle_order", |changed| {
            changed.oracles.swap(0, 1);
        });
    }

    /// Asserts that one isolated cut mutation changes its canonical fingerprint.
    ///
    /// # Panics
    ///
    /// Panics when the named mutation does not change the fingerprint.
    fn assert_cut_mutation(
        cut: &OracleQueryAttemptCut,
        field: &str,
        mutate: impl FnOnce(&mut OracleQueryAttemptCut),
    ) {
        let expected = cut.fingerprint();
        let mut changed = cut.clone();
        mutate(&mut changed);
        assert_ne!(changed.fingerprint(), expected, "{field}");
    }

    /// Proves every field of one Oracle participant is independently fingerprinted.
    ///
    /// # Panics
    ///
    /// Panics when an Oracle field is omitted from the fingerprint or the
    /// fixture no longer carries Oracle capabilities.
    fn assert_oracle_participant_fields(
        cut: &OracleQueryAttemptCut,
        prefix: &str,
        select: fn(&mut OracleQueryAttemptCut) -> &mut OracleQueryParticipant,
    ) {
        assert_cut_mutation(cut, &format!("{prefix}.node_id"), |changed| {
            select(changed).node_id = NodeId::new(uuid::Uuid::from_u128(40));
        });
        assert_cut_mutation(cut, &format!("{prefix}.endpoint"), |changed| {
            select(changed).endpoint.push_str("/changed");
        });
        assert_cut_mutation(cut, &format!("{prefix}.role"), |changed| {
            select(changed).role = ClusterRole::Scribe;
        });
        assert_cut_mutation(cut, &format!("{prefix}.fencing_token"), |changed| {
            select(changed).fencing_token += 1;
        });
        for field in 0..10 {
            assert_cut_mutation(cut, &format!("{prefix}.capabilities.{field}"), |changed| {
                let participant = select(changed);
                if field == 9 {
                    participant.capabilities =
                        ClusterCapabilities::ScribeV1(ScribeCapabilitiesV1 {
                            tail_protocol_version: 1,
                        });
                    return;
                }
                let ClusterCapabilities::OracleV1(value) = &mut participant.capabilities else {
                    panic!("invariant: Oracle fingerprint fixture has Oracle capabilities");
                };
                match field {
                    0 => value.storage_protocol_version += 1,
                    1 => value.cpu_cores += 1.0,
                    2 => value.memory_budget_bytes += 1,
                    3 => value.cpu_cores_per_slot += 1.0,
                    4 => value.memory_bytes_per_slot += 1,
                    5 => value.raw_slots += 1,
                    6 => value.usable_slots += 1,
                    7 => value.supported_classes.push(QueryClass::Analytical),
                    8 => value.max_workers_per_query += 1,
                    _ => unreachable!("bounded Oracle capability fixture"),
                }
            });
        }
    }

    /// Proves every field of one Scribe participant is independently fingerprinted.
    ///
    /// # Panics
    ///
    /// Panics when a Scribe field is omitted from the fingerprint or the
    /// fixture no longer carries Scribe capabilities.
    fn assert_scribe_participant_fields(
        cut: &OracleQueryAttemptCut,
        prefix: &str,
        select: fn(&mut OracleQueryAttemptCut) -> &mut OracleQueryParticipant,
    ) {
        assert_cut_mutation(cut, &format!("{prefix}.node_id"), |changed| {
            select(changed).node_id = NodeId::new(uuid::Uuid::from_u128(41));
        });
        assert_cut_mutation(cut, &format!("{prefix}.endpoint"), |changed| {
            select(changed).endpoint.push_str("/changed");
        });
        assert_cut_mutation(cut, &format!("{prefix}.role"), |changed| {
            select(changed).role = ClusterRole::Oracle;
        });
        assert_cut_mutation(cut, &format!("{prefix}.fencing_token"), |changed| {
            select(changed).fencing_token += 1;
        });
        assert_cut_mutation(cut, &format!("{prefix}.capabilities.variant"), |changed| {
            select(changed).capabilities = ClusterCapabilities::OracleV1(OracleCapabilitiesV1 {
                storage_protocol_version: 1,
                cpu_cores: 1.0,
                memory_budget_bytes: 1024,
                cpu_cores_per_slot: 1.0,
                memory_bytes_per_slot: 1024,
                raw_slots: 1,
                usable_slots: 1,
                supported_classes: vec![QueryClass::Interactive],
                max_workers_per_query: 1,
            });
        });
        assert_cut_mutation(
            cut,
            &format!("{prefix}.capabilities.tail_protocol_version"),
            |changed| {
                let ClusterCapabilities::ScribeV1(value) = &mut select(changed).capabilities else {
                    panic!("invariant: Scribe fingerprint fixture has Scribe capabilities");
                };
                value.tail_protocol_version += 1;
            },
        );
    }
}
