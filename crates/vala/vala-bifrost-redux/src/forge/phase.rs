//! The Forge activation boundary for the current implementation phase.
//!
//! Forge's maintenance graph — snapshot expiry, manifest rewrite, expired-file
//! cleanup, orphan protection, live reconciliation — is implemented, tested,
//! and statically reachable from the scheduler and the worker. What this phase
//! withholds is not the machinery but its *activation*: the point at which a
//! due state is allowed to become a new durable effect against a real catalog
//! or object store.
//!
//! Concentrating that decision in one predicate keeps the property auditable.
//! A boundary spread across scheduler branches, worker match arms, and config
//! defaults is a boundary nobody can prove closed; a boundary spread across
//! deletions is not a boundary at all, because reopening it means rewriting
//! the engine rather than flipping a decision. Both call sites below consult
//! this function and nothing else, so widening the phase is one edit here plus
//! the tests that pin it.
//!
//! The predicate is deliberately not derived from table configuration. A
//! tenant that enables snapshot expiry on its table states a retention policy;
//! it does not grant this phase permission to execute one. Configuration is
//! read *inside* the boundary, never as a way around it.

use vala_sql::row_types::forge_tasks::ForgeTaskStrategy;

/// Reports whether this phase permits `strategy` to begin a new durable effect.
///
/// Scribe promotion and the small-file live rewrite may cross. Every other
/// closed strategy is a retained owner awaiting its own activation task: its
/// planning, execution, recovery, and evidence code stays compiled and
/// reachable, and only its admission is refused.
///
/// Callers must apply this before the first catalog or object-store effect of
/// the strategy, not after. The scheduler applies it to candidate admission so
/// a due state never becomes a durable task; the worker applies it during
/// pre-effect payload validation so an injected claim that bypassed the
/// scheduler is refused before it can touch a catalog, an object store, or a
/// durable transition.
pub(super) const fn admits_new_effect(strategy: ForgeTaskStrategy) -> bool {
    match strategy {
        ForgeTaskStrategy::ScribePromotion | ForgeTaskStrategy::SmallFiles => true,
        ForgeTaskStrategy::FullIdentity
        | ForgeTaskStrategy::ManifestRewrite
        | ForgeTaskStrategy::SnapshotExpiry
        | ForgeTaskStrategy::ExpiredCleanup
        | ForgeTaskStrategy::OrphanCleanup => false,
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
    fn forge_phase_admits_promotion_and_live_rewrite_only() {
        assert!(admits_new_effect(ForgeTaskStrategy::ScribePromotion));
        assert!(admits_new_effect(ForgeTaskStrategy::SmallFiles));
        assert!(!admits_new_effect(ForgeTaskStrategy::FullIdentity));
        assert!(!admits_new_effect(ForgeTaskStrategy::ManifestRewrite));
        assert!(!admits_new_effect(ForgeTaskStrategy::SnapshotExpiry));
        assert!(!admits_new_effect(ForgeTaskStrategy::ExpiredCleanup));
        assert!(!admits_new_effect(ForgeTaskStrategy::OrphanCleanup));
    }
}
