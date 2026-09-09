use vala_sql::row_types::forge_tasks::ForgeTaskStrategy;

/// Reports whether this phase permits `strategy` to begin a new durable effect.
///
/// Every closed strategy is now a live production route with its own retained
/// owner, so the boundary admits all of them. It is retained rather than
/// deleted because it is the one place where a strategy that has no reachable
/// production owner would be refused, and it stays the pre-effect gate both
/// the scheduler and the worker apply.
///
/// Callers must apply this before the first catalog or object-store effect of
/// the strategy, not after. The scheduler applies it to candidate admission so
/// a due state never becomes a durable task; the worker applies it during
/// pre-effect payload validation so an injected claim that bypassed the
/// scheduler is refused before it can touch a catalog, an object store, or a
/// durable transition.
pub(super) const fn admits_new_effect(strategy: ForgeTaskStrategy) -> bool {
    match strategy {
        ForgeTaskStrategy::ScribePromotion
        | ForgeTaskStrategy::SmallFiles
        | ForgeTaskStrategy::SnapshotExpiry
        | ForgeTaskStrategy::ExpiredCleanup
        | ForgeTaskStrategy::OrphanCleanup => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the exact closed set this phase admits.
    ///
    /// The assertion is written strategy by strategy rather than as a
    /// `matches!` over the admitted arm so that adding a strategy to
    /// `ForgeTaskStrategy` fails this test rather than silently defaulting the
    /// new strategy to refused or admitted.
    #[test]
    fn forge_phase_admits_every_production_route() {
        assert!(admits_new_effect(ForgeTaskStrategy::ScribePromotion));
        assert!(admits_new_effect(ForgeTaskStrategy::SmallFiles));
        assert!(admits_new_effect(ForgeTaskStrategy::SnapshotExpiry));
        assert!(admits_new_effect(ForgeTaskStrategy::ExpiredCleanup));
        assert!(admits_new_effect(ForgeTaskStrategy::OrphanCleanup));
    }
}
