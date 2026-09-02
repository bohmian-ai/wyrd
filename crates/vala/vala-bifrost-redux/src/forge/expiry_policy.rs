//! The one decision that says which Iceberg snapshots may be expired.
//!
//! Snapshot expiry is the first Forge protocol whose mistake is unrecoverable:
//! a snapshot removed while something still needs it cannot be put back. The
//! protections that keep that from happening do not live in one place in the
//! table — refs and retained ancestry come from Iceberg, open attempts and
//! pinned readers come from Postgres, and the lineage a no-progress check
//! depends on is a property on the branch head. This module is where they are
//! composed into a single decision, so that adding a protection means adding a
//! root here rather than another guard somewhere along the call path.

use vala_sql::row_types::forge_tasks::SnapshotWatermark;

use super::error::ForgeError;
use super::expire::{SnapshotSummary, select_expirable_snapshots, validate_watermarks};
use super::live_reconcile::DestructiveMaintenance;

/// Every authority outside the Iceberg table that can protect a snapshot.
///
/// Iceberg can answer which snapshots its refs and their retained ancestry
/// still need. It cannot answer whether a Forge attempt is mid-flight, whether
/// a reader has pinned a cut, or whether the branch head's rewrite lineage
/// still has to be readable for the next no-progress check. Those answers come
/// from Postgres and from the head snapshot's own properties, and they are
/// gathered here so the decision below sees all of them or none.
pub(super) struct SnapshotProtectionRoots {
    /// Base snapshots held by open Forge attempts on this table.
    pub(super) attempt_watermarks: Vec<SnapshotWatermark>,
    /// Snapshots held by pinned Oracle cuts.
    pub(super) reader_watermarks: Vec<SnapshotWatermark>,
    /// Base snapshot the branch head's rewrite lineage still references.
    ///
    /// Convergence refuses a rewrite that would make no progress, and it
    /// decides that by reading this snapshot back. Expiring it would not lose
    /// data, but it would make every later rewrite of this table unprovable.
    pub(super) lineage_snapshot_id: Option<i64>,
    /// Fail-closed permission derived from unreconciled durable operations.
    pub(super) destructive_maintenance: DestructiveMaintenance,
}

/// What one expiry pass may do to this table right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SnapshotExpiryDecision {
    /// Exactly these snapshots are eligible, under this proven cutoff.
    Expire {
        /// Ascending snapshot identifiers, exactly as selected.
        snapshot_ids: Vec<i64>,
        /// The cutoff every selected snapshot was proven older than.
        cutoff_ms: i64,
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
    /// Every retained snapshot is protected or younger than the cutoff.
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
    /// Cutoff derived from configured retention and the pass's captured clock.
    pub(super) retention_cutoff_ms: i64,
    /// Minimum ancestry depth retained behind every head.
    pub(super) retain_last: usize,
    /// Bound on ancestry traversal, so a malformed graph cannot spin.
    pub(super) traversal_limit: usize,
}

impl SnapshotExpiryPolicy<'_> {
    /// Decides which snapshots this pass may expire, or why it may not.
    ///
    /// Order is deliberate. Unreconciled work refuses the protocol outright
    /// rather than narrowing the selection, because a smaller selection from
    /// incomplete evidence is still a selection from incomplete evidence. The
    /// watermarks are then corroborated against Iceberg before they are allowed
    /// to lower the cutoff, so a watermark the table cannot account for fails
    /// the pass instead of silently protecting nothing.
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
        let watermarks = self
            .roots
            .attempt_watermarks
            .iter()
            .chain(self.roots.reader_watermarks.iter())
            .copied()
            .collect::<Vec<_>>();
        let cutoff_ms = validate_watermarks(
            self.snapshots,
            self.current_snapshot_id,
            self.ref_heads,
            &watermarks,
            self.retention_cutoff_ms,
            self.traversal_limit,
        )?;
        let mut snapshot_ids = select_expirable_snapshots(
            self.snapshots,
            self.current_snapshot_id,
            self.ref_heads,
            cutoff_ms,
            self.retain_last,
        );
        if let Some(lineage) = self.roots.lineage_snapshot_id {
            snapshot_ids.retain(|id| *id != lineage);
        }
        Ok(if snapshot_ids.is_empty() {
            SnapshotExpiryDecision::NoOp(SnapshotExpiryNoOp::NothingEligible)
        } else {
            SnapshotExpiryDecision::Expire {
                snapshot_ids,
                cutoff_ms,
            }
        })
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
            reader_watermarks: Vec::new(),
            lineage_snapshot_id: None,
            destructive_maintenance: DestructiveMaintenance::Allowed,
        }
    }

    /// Builds the policy under test with one shared geometry.
    fn decide(
        ref_heads: &[i64],
        roots: &SnapshotProtectionRoots,
        retain_last: usize,
    ) -> Result<SnapshotExpiryDecision, crate::forge::error::ForgeError> {
        SnapshotExpiryPolicy {
            snapshots: &history(),
            current_snapshot_id: Some(40),
            ref_heads,
            roots,
            retention_cutoff_ms: 3_500,
            retain_last,
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
        /// Ancestry depth retained behind every head.
        retain_last: usize,
    }

    /// Each root, in isolation, with nothing else covering for it.
    fn root_cases() -> Vec<RootCase> {
        vec![
            RootCase {
                what: "an active ref head",
                protected: vec![20],
                ref_heads: vec![20],
                roots: bare_roots(),
                retain_last: 1,
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
                retain_last: 1,
            },
            RootCase {
                what: "a watermark held by a pinned Oracle cut",
                protected: vec![20, 30],
                ref_heads: Vec::new(),
                roots: SnapshotProtectionRoots {
                    reader_watermarks: vec![SnapshotWatermark {
                        snapshot_id: 20,
                        timestamp_ms: 2_000,
                    }],
                    ..bare_roots()
                },
                retain_last: 1,
            },
            RootCase {
                what: "the rewrite lineage the branch head still references",
                protected: vec![10],
                ref_heads: Vec::new(),
                roots: SnapshotProtectionRoots {
                    lineage_snapshot_id: Some(10),
                    ..bare_roots()
                },
                retain_last: 1,
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
    /// This inventory covers pinned Oracle cuts and carries no live-tail case. That
    /// is a recorded open conflict, not an omission: see
    /// `changes/active/forge-live-tail-authority-conflict.md`. Until it is
    /// resolved, no case here may be widened to stand for both roots at once.
    #[test]
    fn forge_snapshot_expiry_policy_matrix() {
        // Baseline: with no protection beyond the current head and one retained
        // ancestor, everything strictly older than the cutoff is eligible.
        let baseline = decide(&[], &bare_roots(), 1).expect("bare policy decides");
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
                retain_last,
            } = case;
            let with_root = decide(&ref_heads, &roots, retain_last)
                .unwrap_or_else(|error| panic!("{what} decides: {error}"));
            for snapshot in &protected {
                assert!(
                    !expired(&with_root).contains(snapshot),
                    "{what}: snapshot {snapshot} must not be eligible while the root holds"
                );
            }

            let without_root = decide(&[], &bare_roots(), 1)
                .unwrap_or_else(|error| panic!("{what} decides without its root: {error}"));
            for snapshot in &protected {
                assert!(
                    expired(&without_root).contains(snapshot),
                    "{what}: snapshot {snapshot} is only protected by that root, \
                     so removing it must make the snapshot eligible"
                );
            }
        }

        // The age floor is a floor, not a preference: nothing at or after the
        // cutoff is eligible however little else protects it.
        assert!(
            !expired(&baseline).contains(&40),
            "a snapshot at or after the retention cutoff is never eligible"
        );

        // The count floor protects retained ancestry independently of age.
        let count_floor = decide(&[], &bare_roots(), 3).expect("count floor decides");
        assert_eq!(
            expired(&count_floor),
            [10],
            "retain_last protects the head's ancestry regardless of timestamps"
        );

        // Unreconciled durable work is a refusal, not a smaller selection.
        let blocked = decide(
            &[],
            &SnapshotProtectionRoots {
                destructive_maintenance: DestructiveMaintenance::Blocked,
                ..bare_roots()
            },
            1,
        )
        .expect("blocked policy decides");
        assert_eq!(
            blocked,
            SnapshotExpiryDecision::NoOp(SnapshotExpiryNoOp::UnreconciledWork),
            "an open or uncertain attempt refuses the whole protocol"
        );

        // A watermark that Iceberg cannot corroborate fails closed rather than
        // being dropped from the protected set.
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
                        reader_watermarks: vec![watermark],
                        ..bare_roots()
                    },
                    1,
                )
                .is_err(),
                "{case} must fail closed"
            );
        }
    }
}
