//! The one decision that says which Iceberg snapshots may be expired.
//!
//! Snapshot expiry is the first Forge protocol whose mistake is unrecoverable:
//! a snapshot removed while something still needs it cannot be put back. The
//! protections that keep that from happening do not live in one place in the
//! table — refs come from Iceberg, open attempts and existing claims come
//! from Postgres, and the lineage a no-progress check
//! depends on is a property on the branch head. This module is where they are
//! composed into a single decision, so that adding a protection means adding a
//! root here rather than another guard somewhere along the call path.
//!
//! Active Oracle readers are not a root here: any active table read refuses
//! the whole expiration at preparation, under the table's maintenance
//! authority, so selection never reasons about which snapshots a reader needs.

use std::collections::HashSet;

use vala_sql::row_types::forge_tasks::SnapshotWatermark;

use super::error::ForgeError;
use super::expire::{SnapshotSummary, select_expirable_snapshots, validate_watermarks};
use super::live_reconcile::DestructiveMaintenance;

/// Every authority outside the Iceberg table that can protect a snapshot.
///
/// Iceberg can answer which snapshots its refs still need. It cannot answer
/// whether a Forge attempt is mid-flight, whether another expiration already
/// owns a snapshot, or whether the branch head's rewrite lineage
/// still has to be readable for the next no-progress check. Those answers come
/// from Postgres and from the head snapshot's own properties, and they are
/// gathered here so the decision below sees all of them or none.
pub(super) struct SnapshotProtectionRoots {
    /// Base snapshots held by open Forge attempts on this table.
    ///
    /// Each protects itself and every retained snapshot between it and a
    /// head, so the attempt's commit-time conflict validation can still walk
    /// from the head back to its base.
    pub(super) attempt_watermarks: Vec<SnapshotWatermark>,
    /// Base snapshot the branch head's rewrite lineage still references.
    ///
    /// Convergence refuses a rewrite that would make no progress, and it
    /// decides that by reading this snapshot back. Expiring it would not lose
    /// data, but it would make every later rewrite of this table unprovable.
    pub(super) lineage_snapshot_id: Option<i64>,
    /// Snapshots an unresolved snapshot-expiration claim already owns.
    ///
    /// A claim is not a read protection: nothing needs the snapshot, another
    /// prepared operation is already responsible for removing it. Selecting it
    /// again would prepare two operations for the same deletion, so the claim
    /// removes it from this pass rather than refusing the pass.
    pub(super) claimed_snapshot_ids: Vec<i64>,
    /// Fail-closed permission derived from unreconciled durable operations.
    pub(super) destructive_maintenance: DestructiveMaintenance,
}

/// What one expiry pass may do to this table right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SnapshotExpiryDecision {
    /// Exactly these snapshots are eligible.
    Expire {
        /// Ascending snapshot identifiers, exactly as selected.
        snapshot_ids: Vec<i64>,
    },
    /// Nothing may be expired, and the reason is not a failure.
    NoOp(SnapshotExpiryNoOp),
}

/// Why an expiry pass legitimately did nothing.
///
/// These are distinct from an error because both are correct outcomes that a
/// scheduler must be able to repeat forever without escalating, and distinct
/// from each other because only one of them clears on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SnapshotExpiryNoOp {
    /// A durable operation is open or its outcome is still uncertain.
    UnreconciledWork,
    /// Every retained snapshot is protected by some root.
    NothingEligible,
}

/// One table's complete snapshot-expiry decision inputs.
///
/// Held by borrow for the length of a single decision: the whole point is that
/// the catalog metadata and the durable roots were read for this pass and are
/// evaluated together, so nothing here outlives the fence that made it true.
pub(super) struct SnapshotExpiryPolicy<'inputs> {
    /// Every snapshot the table currently retains.
    pub(super) snapshots: &'inputs [SnapshotSummary],
    /// The table's current snapshot, when it has one.
    pub(super) current_snapshot_id: Option<i64>,
    /// Snapshot identifiers at the head of every named Iceberg ref.
    pub(super) ref_heads: &'inputs [i64],
    /// Non-catalog protection gathered for this pass.
    pub(super) roots: &'inputs SnapshotProtectionRoots,
    /// Bound on ancestry traversal, so a malformed graph cannot spin.
    pub(super) traversal_limit: usize,
}

impl SnapshotExpiryPolicy<'_> {
    /// Decides which snapshots this pass may expire, or why it may not.
    ///
    /// Order is deliberate. Unreconciled work refuses the protocol outright
    /// rather than narrowing the selection, because a smaller selection from
    /// incomplete evidence is still a selection from incomplete evidence. The
    /// watermarks are then corroborated against Iceberg before they protect
    /// anything, so a watermark the table cannot account for fails the pass
    /// instead of silently protecting nothing. There is no age or depth
    /// retention: every replaced snapshot no root holds is selected.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::SnapshotExpiry`] when a watermark names a snapshot
    /// the table no longer retains, is detached from the approved heads,
    /// disagrees with Iceberg on its timestamp, or when the ancestry graph is
    /// malformed or exceeds [`Self::traversal_limit`].
    pub(super) fn decide(&self) -> Result<SnapshotExpiryDecision, ForgeError> {
        if self.roots.destructive_maintenance == DestructiveMaintenance::Blocked {
            return Ok(SnapshotExpiryDecision::NoOp(
                SnapshotExpiryNoOp::UnreconciledWork,
            ));
        }
        validate_watermarks(
            self.snapshots,
            self.current_snapshot_id,
            self.ref_heads,
            &self.roots.attempt_watermarks,
            self.traversal_limit,
        )?;
        let held = self.watermark_chains();
        let mut snapshot_ids =
            select_expirable_snapshots(self.snapshots, self.current_snapshot_id, self.ref_heads);
        snapshot_ids.retain(|id| {
            !held.contains(id)
                && Some(*id) != self.roots.lineage_snapshot_id
                && !self.roots.claimed_snapshot_ids.contains(id)
        });
        Ok(if snapshot_ids.is_empty() {
            SnapshotExpiryDecision::NoOp(SnapshotExpiryNoOp::NothingEligible)
        } else {
            SnapshotExpiryDecision::Expire { snapshot_ids }
        })
    }

    /// Returns every snapshot on a head's ancestry from the head down to and
    /// including an attempt watermark.
    ///
    /// Runs after [`validate_watermarks`] proved the graph acyclic, bounded,
    /// and every watermark reachable, so each walk ends at a root or at a
    /// parent an earlier expiration removed.
    fn watermark_chains(&self) -> HashSet<i64> {
        let parents = self
            .snapshots
            .iter()
            .map(|snapshot| (snapshot.id, snapshot.parent_id))
            .collect::<std::collections::HashMap<_, _>>();
        let watermarks = self
            .roots
            .attempt_watermarks
            .iter()
            .map(|watermark| watermark.snapshot_id)
            .collect::<HashSet<_>>();
        let mut held = HashSet::new();
        for head in self
            .ref_heads
            .iter()
            .copied()
            .chain(self.current_snapshot_id)
        {
            let mut path = Vec::new();
            let mut cursor = Some(head);
            while let Some(id) = cursor.filter(|id| parents.contains_key(id)) {
                path.push(id);
                if watermarks.contains(&id) {
                    held.extend(path.iter().copied());
                }
                cursor = parents.get(&id).copied().flatten();
            }
        }
        held
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A linear four-snapshot history: 10 -> 20 -> 30 -> 40.
    fn history() -> Vec<SnapshotSummary> {
        vec![
            SnapshotSummary {
                id: 10,
                parent_id: None,
                timestamp_ms: 1_000,
            },
            SnapshotSummary {
                id: 20,
                parent_id: Some(10),
                timestamp_ms: 2_000,
            },
            SnapshotSummary {
                id: 30,
                parent_id: Some(20),
                timestamp_ms: 3_000,
            },
            SnapshotSummary {
                id: 40,
                parent_id: Some(30),
                timestamp_ms: 4_000,
            },
        ]
    }

    /// Roots with nothing protecting anything beyond the table's own refs.
    fn bare_roots() -> SnapshotProtectionRoots {
        SnapshotProtectionRoots {
            attempt_watermarks: Vec::new(),
            lineage_snapshot_id: None,
            claimed_snapshot_ids: Vec::new(),
            destructive_maintenance: DestructiveMaintenance::Allowed,
        }
    }

    /// Builds the policy under test with one shared geometry.
    fn decide(
        ref_heads: &[i64],
        roots: &SnapshotProtectionRoots,
    ) -> Result<SnapshotExpiryDecision, crate::forge::error::ForgeError> {
        decide_from(Some(40), ref_heads, roots)
    }

    /// Builds the policy under test with an explicit current snapshot.
    ///
    /// Removing the current snapshot is its own root: a table with no current
    /// snapshot protects nothing through it, which is only visible when the
    /// geometry can be decided both ways.
    fn decide_from(
        current_snapshot_id: Option<i64>,
        ref_heads: &[i64],
        roots: &SnapshotProtectionRoots,
    ) -> Result<SnapshotExpiryDecision, crate::forge::error::ForgeError> {
        SnapshotExpiryPolicy {
            snapshots: &history(),
            current_snapshot_id,
            ref_heads,
            roots,
            traversal_limit: 16,
        }
        .decide()
    }

    /// Returns the exact snapshots a decision would expire.
    fn expired(decision: &SnapshotExpiryDecision) -> &[i64] {
        match decision {
            SnapshotExpiryDecision::Expire { snapshot_ids, .. } => snapshot_ids,
            SnapshotExpiryDecision::NoOp(_) => &[],
        }
    }

    /// One protection root, and the snapshot it alone keeps from expiring.
    struct RootCase {
        /// What this case proves, quoted back in every failure.
        what: &'static str,
        /// Snapshots this root must hold, and the bare policy must release.
        protected: Vec<i64>,
        /// Iceberg refs for this case.
        ref_heads: Vec<i64>,
        /// Non-catalog roots for this case.
        roots: SnapshotProtectionRoots,
    }

    /// Each root, in isolation, with nothing else covering for it.
    fn root_cases() -> Vec<RootCase> {
        vec![
            RootCase {
                what: "an active ref head",
                protected: vec![20],
                ref_heads: vec![20],
                roots: bare_roots(),
            },
            RootCase {
                what: "a watermark held by an open Forge attempt",
                protected: vec![20, 30],
                ref_heads: Vec::new(),
                roots: SnapshotProtectionRoots {
                    attempt_watermarks: vec![SnapshotWatermark {
                        snapshot_id: 20,
                        timestamp_ms: 2_000,
                    }],
                    ..bare_roots()
                },
            },
            RootCase {
                what: "an unresolved expiration claim on the same table",
                protected: vec![20],
                ref_heads: Vec::new(),
                roots: SnapshotProtectionRoots {
                    claimed_snapshot_ids: vec![20],
                    ..bare_roots()
                },
            },
            RootCase {
                what: "two attempt watermarks on one lineage",
                protected: vec![10, 20, 30],
                ref_heads: Vec::new(),
                roots: SnapshotProtectionRoots {
                    attempt_watermarks: vec![
                        SnapshotWatermark {
                            snapshot_id: 10,
                            timestamp_ms: 1_000,
                        },
                        SnapshotWatermark {
                            snapshot_id: 20,
                            timestamp_ms: 2_000,
                        },
                    ],
                    ..bare_roots()
                },
            },
            RootCase {
                what: "the rewrite lineage the branch head still references",
                protected: vec![10],
                ref_heads: Vec::new(),
                roots: SnapshotProtectionRoots {
                    lineage_snapshot_id: Some(10),
                    ..bare_roots()
                },
            },
        ]
    }

    /// Every protection root independently keeps a snapshot from expiring.
    ///
    /// Each case is written as a pair: the same geometry decided with the root
    /// present and with that one root removed. Asserting only the protected
    /// direction would pass for a policy that refuses everything, and asserting
    /// only the eligible direction would pass for a policy that protects
    /// nothing, so the case is the difference between the two. The roots are
    /// deliberately not composed: a policy that protects a snapshot because
    /// some *other* root happened to cover it is not proof that this root
    /// works, and it is exactly how a protection is silently lost.
    ///
    /// This inventory carries no Oracle reader case because an active table
    /// read refuses the whole expiration at preparation rather than protecting
    /// individual snapshots, and no live-tail case because a v1 live-tail lease
    /// names no Forge-collectable object.
    #[test]
    fn snapshot_expiry_root_mutation_matrix() {
        // Baseline: with no protection beyond the current head, every replaced
        // snapshot is eligible however recent it is.
        let baseline = decide(&[], &bare_roots()).expect("bare policy decides");
        assert_eq!(
            expired(&baseline),
            [10, 20, 30],
            "only the current head is protected by default"
        );

        for case in root_cases() {
            let RootCase {
                what,
                protected,
                ref_heads,
                roots,
            } = case;
            let with_root = decide(&ref_heads, &roots)
                .unwrap_or_else(|error| panic!("{what} decides: {error}"));
            for snapshot in &protected {
                assert!(
                    !expired(&with_root).contains(snapshot),
                    "{what}: snapshot {snapshot} must not be eligible while the root holds"
                );
            }

            let without_root = decide(&[], &bare_roots())
                .unwrap_or_else(|error| panic!("{what} decides without its root: {error}"));
            for snapshot in &protected {
                assert!(
                    expired(&without_root).contains(snapshot),
                    "{what}: snapshot {snapshot} is only protected by that root, \
                     so removing it must make the snapshot eligible"
                );
            }
        }

        // The current snapshot is a root in its own right: without one, the
        // head stops being protected at all.
        let headless = decide_from(None, &[], &bare_roots())
            .expect("a table with no current snapshot decides");
        assert_eq!(
            expired(&headless),
            [10, 20, 30, 40],
            "removing the current snapshot removes only what it alone protected"
        );

        // Unreconciled durable work is a refusal, not a smaller selection.
        let blocked = decide(
            &[],
            &SnapshotProtectionRoots {
                destructive_maintenance: DestructiveMaintenance::Blocked,
                ..bare_roots()
            },
        )
        .expect("blocked policy decides");
        assert_eq!(
            blocked,
            SnapshotExpiryDecision::NoOp(SnapshotExpiryNoOp::UnreconciledWork),
            "an open or uncertain attempt refuses the whole protocol"
        );

        assert_uncorroborated_watermarks_fail_closed();
    }

    /// Proves a watermark Iceberg cannot corroborate fails the pass instead of
    /// being quietly dropped from the protected set.
    ///
    /// # Panics
    ///
    /// Panics when either uncorroborated watermark is tolerated.
    fn assert_uncorroborated_watermarks_fail_closed() {
        for (case, watermark) in [
            (
                "a watermark on a snapshot the table no longer retains",
                SnapshotWatermark {
                    snapshot_id: 99,
                    timestamp_ms: 2_000,
                },
            ),
            (
                "a watermark whose timestamp disagrees with Iceberg",
                SnapshotWatermark {
                    snapshot_id: 20,
                    timestamp_ms: 2_001,
                },
            ),
        ] {
            assert!(
                decide(
                    &[],
                    &SnapshotProtectionRoots {
                        attempt_watermarks: vec![watermark],
                        ..bare_roots()
                    },
                )
                .is_err(),
                "{case} must fail closed"
            );
        }
    }
}
