//! IO-free projection matching.
//!
//! [`match_projections`] takes a list of [`ProjectionCandidate`]s (loaded from
//! `vala.olap_projections` via [`list_by_source`](vala_sql::queries::olap_catalog::list_by_source))
//! and a [`MatchPlan`] describing the source-scan context, and returns the
//! candidates that are eligible for read-time substitution.
//!
//! Freshness rules (from §9 slice-04 contract):
//! - **`LookupSet`**: exact freshness — `refresh_epoch == source_refresh_epoch`
//!   AND `built_for_snapshot_id == current_source_snapshot_id`.
//! - **Rollup / MV**: bounded staleness — `commit_lag <= max_allowed_lag` (the
//!   per-deployment staleness knob). Never a numeric snapshot-id comparison.
//!
//! F14 schema-fingerprint guard: the candidate's `source_schema_fingerprint`
//! must match the current source table fingerprint byte-for-byte. This is a
//! source-vs-source comparison only — the projection output schema is not
//! checked here.
//!
//! This module is **IO-free**: no Postgres, no Tokio, no `DataFusion`. All
//! inputs are plain values; callers load them before calling.

use crate::types::SchemaFingerprint;

/// The projection kind that determines which freshness rule applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionKind {
    /// Aggregated Rollup view: bounded-staleness via commit-lag knob.
    Rollup,
    /// Materialized View: bounded-staleness via commit-lag knob.
    Mv,
    /// `LookupSet` index: exact freshness (epoch + snapshot-id identity match).
    LookupSet,
}

impl ProjectionKind {
    /// Parse from the `projection_kind` DB string.
    ///
    /// Returns `None` for unknown values (treated as non-substitutable).
    #[must_use]
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "rollup" => Some(Self::Rollup),
            "mv" => Some(Self::Mv),
            "lookup_set" => Some(Self::LookupSet),
            _ => None,
        }
    }
}

/// A loaded projection candidate, ready for matching.
///
/// Constructed from a [`vala_sql::row_types::olap_catalog::ProjectionCandidateRow`]
/// after the calling layer fetches it from Postgres.
#[derive(Debug, Clone)]
pub struct ProjectionCandidate {
    /// Opaque 16-byte projection identifier.
    pub projection_uid: Vec<u8>,
    /// Parsed projection kind.
    pub kind: ProjectionKind,
    /// Fully-qualified projection name for tracing / error reporting.
    pub fqn: String,
    /// Refresh epoch at last successful refresh.
    pub refresh_epoch: i64,
    /// Source table's refresh epoch at last refresh.
    pub source_refresh_epoch: i64,
    /// Iceberg snapshot id the projection was built for (identity anchor).
    /// `None` means the projection has never been successfully refreshed.
    pub built_for_snapshot_id: Option<i64>,
    /// Commit-lag since last refresh (Rollup/MV staleness knob input).
    pub commit_lag: i64,
    /// F14 guard: source schema fingerprint at last refresh (32 bytes).
    pub source_schema_fingerprint: SchemaFingerprint,
}

/// Context describing the source table at the time of a query plan.
///
/// All values are loaded before calling [`match_projections`]; the function
/// itself performs no IO.
#[derive(Debug, Clone)]
pub struct MatchPlan {
    /// Current refresh epoch of the source table.
    pub source_refresh_epoch: i64,
    /// Current Iceberg snapshot id of the source table.
    /// `None` means the table has never been committed.
    pub current_snapshot_id: Option<i64>,
    /// Current schema fingerprint of the source table (F14 guard input).
    pub source_schema_fingerprint: SchemaFingerprint,
    /// Maximum allowed commit-lag for Rollup/MV substitution (staleness knob).
    /// A value of `0` means only exact-lag (no staleness) is allowed.
    pub max_rollup_commit_lag: i64,
}

/// A projection candidate that passed all freshness and fingerprint checks.
#[derive(Debug, Clone)]
pub struct MatchedProjection {
    /// The candidate that passed.
    pub candidate: ProjectionCandidate,
}

/// Select which candidates are substitutable for the described source scan.
///
/// Returns only candidates that satisfy:
/// 1. F14 source-schema-fingerprint guard (source-vs-source only).
/// 2. Kind-specific freshness rule:
///    - `LookupSet`: exact epoch match + snapshot-id identity match.
///    - Rollup/MV: `commit_lag <= max_rollup_commit_lag`.
///
/// Candidates that have never been refreshed (`built_for_snapshot_id = None`
/// for `LookupSet`; any kind with a zero/default epoch before first refresh)
/// are rejected.
///
/// This function is pure and IO-free.
#[must_use]
pub fn match_projections(
    candidates: &[ProjectionCandidate],
    plan: &MatchPlan,
) -> Vec<MatchedProjection> {
    candidates
        .iter()
        .filter(|c| is_substitutable(c, plan))
        .map(|c| MatchedProjection {
            candidate: c.clone(),
        })
        .collect()
}

/// Returns `true` when `candidate` passes the F14 fingerprint guard and the
/// kind-specific freshness rule.
fn is_substitutable(candidate: &ProjectionCandidate, plan: &MatchPlan) -> bool {
    // F14: source schema fingerprint must match byte-for-byte.
    // This is source-vs-source only — never projection output schema.
    if candidate.source_schema_fingerprint != plan.source_schema_fingerprint {
        tracing::debug!(
            fqn = %candidate.fqn,
            "projection matcher: F14 fingerprint mismatch — skipping candidate"
        );
        return false;
    }

    match candidate.kind {
        ProjectionKind::LookupSet => {
            // Exact freshness: epoch must match AND snapshot identity must match.
            let epoch_ok = candidate.refresh_epoch == candidate.source_refresh_epoch
                && candidate.refresh_epoch == plan.source_refresh_epoch;

            let snapshot_ok = match (candidate.built_for_snapshot_id, plan.current_snapshot_id) {
                (Some(built), Some(current)) => built == current,
                // A LookupSet that has never been built is never substitutable.
                _ => false,
            };

            if !epoch_ok {
                tracing::debug!(
                    fqn = %candidate.fqn,
                    candidate_epoch = candidate.refresh_epoch,
                    source_epoch = plan.source_refresh_epoch,
                    "projection matcher: LookupSet epoch mismatch"
                );
            }
            if !snapshot_ok {
                tracing::debug!(
                    fqn = %candidate.fqn,
                    built_for = ?candidate.built_for_snapshot_id,
                    current = ?plan.current_snapshot_id,
                    "projection matcher: LookupSet snapshot-id mismatch"
                );
            }
            epoch_ok && snapshot_ok
        }

        ProjectionKind::Rollup | ProjectionKind::Mv => {
            // Bounded staleness: commit_lag must not exceed the configured knob.
            // Never use numeric snapshot-id comparison (RW-F02).
            let lag_ok = candidate.commit_lag <= plan.max_rollup_commit_lag;

            if !lag_ok {
                tracing::debug!(
                    fqn = %candidate.fqn,
                    commit_lag = candidate.commit_lag,
                    max_allowed = plan.max_rollup_commit_lag,
                    "projection matcher: Rollup/MV commit-lag exceeds knob"
                );
            }
            lag_ok
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(byte: u8) -> SchemaFingerprint {
        SchemaFingerprint([byte; 32])
    }

    fn base_candidate(kind: ProjectionKind) -> ProjectionCandidate {
        ProjectionCandidate {
            projection_uid: vec![0u8; 16],
            kind,
            fqn: "test.proj".to_string(),
            refresh_epoch: 3,
            source_refresh_epoch: 3,
            built_for_snapshot_id: Some(42),
            commit_lag: 0,
            source_schema_fingerprint: fingerprint(1),
        }
    }

    fn base_plan() -> MatchPlan {
        MatchPlan {
            source_refresh_epoch: 3,
            current_snapshot_id: Some(42),
            source_schema_fingerprint: fingerprint(1),
            max_rollup_commit_lag: 5,
        }
    }

    // ── LookupSet exact freshness ─────────────────────────────────────────────

    #[test]
    fn lookup_set_fresh_is_matched() {
        let candidates = vec![base_candidate(ProjectionKind::LookupSet)];
        let matched = match_projections(&candidates, &base_plan());
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].candidate.fqn, "test.proj");
    }

    #[test]
    fn lookup_set_stale_epoch_is_rejected() {
        let mut c = base_candidate(ProjectionKind::LookupSet);
        c.refresh_epoch = 2; // behind
        c.source_refresh_epoch = 2;
        let matched = match_projections(&[c], &base_plan());
        assert!(matched.is_empty(), "stale epoch must not be substituted");
    }

    #[test]
    fn lookup_set_stale_snapshot_is_rejected() {
        let mut c = base_candidate(ProjectionKind::LookupSet);
        c.built_for_snapshot_id = Some(41); // built for older snapshot
        let matched = match_projections(&[c], &base_plan());
        assert!(
            matched.is_empty(),
            "snapshot-id mismatch must not be substituted"
        );
    }

    #[test]
    fn lookup_set_never_built_is_rejected() {
        let mut c = base_candidate(ProjectionKind::LookupSet);
        c.built_for_snapshot_id = None;
        let matched = match_projections(&[c], &base_plan());
        assert!(
            matched.is_empty(),
            "never-built LookupSet must not be substituted"
        );
    }

    // ── Rollup / MV bounded staleness ─────────────────────────────────────────

    #[test]
    fn rollup_within_lag_knob_is_matched() {
        let mut c = base_candidate(ProjectionKind::Rollup);
        c.commit_lag = 3; // within max_rollup_commit_lag = 5
        let matched = match_projections(&[c], &base_plan());
        assert_eq!(matched.len(), 1);
    }

    #[test]
    fn rollup_exceeds_lag_knob_is_rejected() {
        let mut c = base_candidate(ProjectionKind::Rollup);
        c.commit_lag = 10; // exceeds max_rollup_commit_lag = 5
        let matched = match_projections(&[c], &base_plan());
        assert!(
            matched.is_empty(),
            "rollup exceeding lag knob must not be substituted"
        );
    }

    #[test]
    fn mv_within_lag_knob_is_matched() {
        let c = base_candidate(ProjectionKind::Mv);
        let matched = match_projections(&[c], &base_plan());
        assert_eq!(matched.len(), 1);
    }

    // ── F14 schema-fingerprint guard ──────────────────────────────────────────

    #[test]
    fn fingerprint_mismatch_rejects_all_kinds() {
        for kind in [
            ProjectionKind::LookupSet,
            ProjectionKind::Rollup,
            ProjectionKind::Mv,
        ] {
            let mut c = base_candidate(kind);
            c.source_schema_fingerprint = fingerprint(99); // different fingerprint
            let matched = match_projections(&[c], &base_plan());
            assert!(
                matched.is_empty(),
                "F14 fingerprint mismatch must reject {kind:?}"
            );
        }
    }

    // ── substitutes a scan onto a fresh projection (behavior gate) ────────────

    /// Behavior gate: a fresh `LookupSet` is substituted; a stale one is rejected.
    /// This is the primary gate named in the build ledger.
    #[test]
    fn substitutes_scan_onto_fresh_projection_rejects_stale() {
        let fresh = base_candidate(ProjectionKind::LookupSet);
        let mut stale = base_candidate(ProjectionKind::LookupSet);
        stale.fqn = "test.stale".to_string();
        stale.built_for_snapshot_id = Some(1); // wrong snapshot

        let matched = match_projections(&[fresh, stale], &base_plan());
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].candidate.fqn, "test.proj");
    }
}
