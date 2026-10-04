//! Composition of the orphan-collection protected union.
//!
//! Never-published orphan collection is the one Forge protocol that deletes an
//! object no catalog snapshot names, so its safety rests entirely on having
//! assembled every authority that can still need that object. Assembling the
//! union is kept separate from loading its evidence: each root class arrives as
//! a named field, so a class that stops contributing is a visible change here
//! rather than a silently shorter list inside an IO path.

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
    /// Committed Scribe objects with no exact promotion evidence yet.
    pub(super) hot_unpromoted: Vec<String>,
    /// Outputs a staged, prepared, open, commit-uncertain, or reconciling
    /// attempt produced or may still produce.
    pub(super) open_outputs: Vec<String>,
    /// Whether an open or unreconciled operation, or an active Oracle table
    /// read, forbids destructive work.
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
    /// Active Oracle readers contribute no paths: an active table read blocks
    /// destructive work for the whole table through [`Self::blocked`].
    pub(super) fn compose(self) -> ComposedProtection {
        let mut live_set = self.catalog;
        live_set.extend_validated(self.hot_unpromoted);
        live_set.extend_validated(self.open_outputs);
        ComposedProtection {
            live_set,
            blocked: self.blocked,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The union protects every non-catalog authority, and a blocked root set
    /// stays blocked.
    ///
    /// Each case drops exactly one root class and requires the object that
    /// class alone protected to become eligible, so a class that quietly stops
    /// contributing cannot pass as a complete union.
    ///
    /// This inventory carries no live-tail case because a v1 live-tail lease
    /// names no Forge-collectable object and so contributes no independent
    /// Forge GC root. Active Oracle reads reach the union only as `blocked`.
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
            hot_unpromoted: vec![hot_path.to_owned()],
            open_outputs: vec![open_output.to_owned()],
            blocked: false,
        };

        let complete = roots().compose();
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
            !without_hot.compose().live_set.contains(hot_path),
            "hot unpromoted Scribe objects are protected only by their own root"
        );

        let mut without_open = roots();
        without_open.open_outputs.clear();
        assert!(
            !without_open.compose().live_set.contains(open_output),
            "staged, prepared, open, possible, and uncertain outputs are protected only by their own root"
        );

        let mut blocked = roots();
        blocked.blocked = true;
        assert!(blocked.compose().blocked);
        assert!(catalog.contains(catalog_path));
    }
}
