//! Composition of the orphan-collection protected union.
//!
//! Never-published orphan collection is the one Forge protocol that deletes an
//! object no catalog snapshot names, so its safety rests entirely on having
//! assembled every authority that can still need that object. Assembling the
//! union is kept separate from loading its evidence: each root class arrives as
//! a named field, so a class that stops contributing is a visible change here
//! rather than a silently shorter list inside an IO path.

use super::error::ForgeError;
use super::orphan_gc::ProtectedLiveSet;

/// Every authority that can still need an object no catalog snapshot names.
///
/// The catalog reachability set arrives already traversed because building it
/// requires manifest IO. Everything else is durable evidence the loader has
/// already read, so composition itself is pure and the union's completeness is
/// decided in one visible place.
pub(super) struct OrphanProtectionRoots {
    /// Objects reachable from retained Iceberg metadata.
    pub(super) catalog: ProtectedLiveSet,
    /// Snapshot ids the catalog traversal actually visited.
    pub(super) traversed_snapshot_ids: Vec<i64>,
    /// Committed Scribe objects with no exact promotion evidence yet.
    pub(super) hot_unpromoted: Vec<String>,
    /// Outputs a staged, prepared, open, commit-uncertain, or reconciling
    /// attempt produced or may still produce.
    pub(super) open_outputs: Vec<String>,
    /// Snapshots a pinned Oracle cut still depends on.
    pub(super) pinned_snapshot_ids: Vec<i64>,
    /// Whether any open or unreconciled operation forbids destructive work.
    pub(super) blocked: bool,
}

/// The assembled union and destructive gate one collection pass may use.
pub(super) struct ComposedProtection {
    /// Union of every protecting authority.
    pub(super) live_set: ProtectedLiveSet,
    /// Whether destructive work is forbidden outright.
    pub(super) blocked: bool,
}

impl OrphanProtectionRoots {
    /// Folds every root class into one union.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Reconciliation`] when a pinned snapshot is not
    /// among the traversed snapshots. A reader depends on that snapshot's
    /// objects, and the union cannot name them, so the pass must refuse rather
    /// than delete objects it never had the chance to protect.
    pub(super) fn compose(self) -> Result<ComposedProtection, ForgeError> {
        for pinned in &self.pinned_snapshot_ids {
            if !self.traversed_snapshot_ids.contains(pinned) {
                return Err(ForgeError::Reconciliation {
                    detail: format!(
                        "snapshot {pinned} is pinned by a reader but absent from the traversed catalog history"
                    ),
                });
            }
        }
        let mut live_set = self.catalog;
        live_set.extend_validated(self.hot_unpromoted);
        live_set.extend_validated(self.open_outputs);
        Ok(ComposedProtection {
            live_set,
            blocked: self.blocked,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The union protects every non-catalog authority, and fails closed
    /// when a pinned snapshot is no longer traversable.
    ///
    /// Each case drops exactly one root class and requires the object that
    /// class alone protected to become eligible, so a class that quietly stops
    /// contributing cannot pass as a complete union.
    ///
    /// This inventory covers pinned Oracle cuts and carries no live-tail case
    /// because a v1 live-tail lease names no Forge-collectable object and so
    /// contributes no independent Forge GC root. No case here may be widened to
    /// stand for both roots at once.
    #[test]
    fn forge_orphan_protection_includes_all_noncatalog_authority() {
        let catalog_path = "t/spans/data/forge/catalog-00000.parquet";
        let hot_path = "t/spans/data/pod-a-01JHOT.parquet";
        let open_output = "t/spans/data/forge/open-00000.parquet";
        let stranded = "t/spans/data/forge/stranded-00000.parquet";
        let mut catalog = ProtectedLiveSet::default();
        catalog.insert(catalog_path);

        let roots = || OrphanProtectionRoots {
            catalog: {
                let mut set = ProtectedLiveSet::default();
                set.insert(catalog_path);
                set
            },
            traversed_snapshot_ids: vec![10, 20],
            hot_unpromoted: vec![hot_path.to_owned()],
            open_outputs: vec![open_output.to_owned()],
            pinned_snapshot_ids: vec![20],
            blocked: false,
        };

        let complete = roots().compose().expect("complete roots compose");
        assert!(complete.live_set.contains(catalog_path));
        assert!(complete.live_set.contains(hot_path));
        assert!(complete.live_set.contains(open_output));
        assert!(
            !complete.live_set.contains(stranded),
            "an object no authority names stays collectable"
        );
        assert!(!complete.blocked);

        let mut without_hot = roots();
        without_hot.hot_unpromoted.clear();
        assert!(
            !without_hot
                .compose()
                .expect("compose")
                .live_set
                .contains(hot_path),
            "hot unpromoted Scribe objects are protected only by their own root"
        );

        let mut without_open = roots();
        without_open.open_outputs.clear();
        assert!(
            !without_open
                .compose()
                .expect("compose")
                .live_set
                .contains(open_output),
            "staged, prepared, open, possible, and uncertain outputs are protected only by their own root"
        );

        let mut unpinned = roots();
        unpinned.pinned_snapshot_ids = vec![30];
        assert!(
            unpinned.compose().is_err(),
            "an Oracle cut on a snapshot the traversal never saw must fail closed"
        );

        let mut blocked = roots();
        blocked.blocked = true;
        assert!(blocked.compose().expect("compose").blocked);
        assert!(catalog.contains(catalog_path));
    }
}
