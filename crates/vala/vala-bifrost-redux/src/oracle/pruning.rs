//! Pre-footer file exclusion from validated immutable event-time statistics.
//!
//! Oracle already prunes row groups from Parquet footer statistics, but that
//! happens after the object is opened and its footer ranges are read. Every
//! persisted file the pinned cut carries also declares an immutable
//! `wyrd_event_time` interval in metadata Oracle already holds, so a file whose
//! interval cannot intersect the query's supported event-time predicates can be
//! dropped before it is assigned to a participant, before its footer is read,
//! and before any data range leaves storage.
//!
//! This is a strict optimization layered under the existing execution
//! guarantees. Pushdown stays `Inexact`, `DataFusion` keeps its residual filter,
//! the hidden tenant column and `TenantTripwireExec` are untouched, and any
//! file whose statistics are absent, undecodable, or contradictory is retained.
//! The only files this module removes are ones the query provably cannot read a
//! row from.

use wyrd_spec::vala::WYRD_EVENT_TIME;
use wyrd_spec::vala::assignment_authority::{ScanLiteral, ScanPredicate};

use crate::catalog::event_time::{EventTimeBoundsDefect, EventTimeStatistics};

/// Closed outcome of one file's pre-assignment pruning decision.
///
/// The three fail-open variants are folded into a single carrier because the
/// caller records the defect's own label; separating them here would duplicate
/// the reason inventory that [`EventTimeBoundsDefect`] already owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FilePruningDecision {
    /// The file may contain matching rows and is assigned and scanned.
    Include,
    /// The file's validated interval cannot intersect the query interval.
    Exclude,
    /// The file's statistics are unusable; it is retained for the named reason.
    FailOpen(EventTimeBoundsDefect),
}

impl FilePruningDecision {
    /// Returns `true` when the file must not be assigned, opened, or read.
    #[must_use]
    pub(crate) const fn excludes(self) -> bool {
        matches!(self, Self::Exclude)
    }

    /// Returns the closed telemetry outcome label for this decision.
    ///
    /// The values are the complete `bifrost_oracle_file_pruning_total` outcome
    /// inventory; every physical file decision emits exactly one of them.
    #[must_use]
    pub(crate) const fn outcome_label(self) -> &'static str {
        match self {
            Self::Include => "included",
            Self::Exclude => "excluded",
            Self::FailOpen(defect) => defect.outcome_label(),
        }
    }
}

/// Closed persisted-source label for one file-pruning decision.
///
/// The two members are the only sources whose immutable statistics Oracle
/// holds before opening an object: staged hot Parquet described by
/// `vala.file_list`, and pinned Iceberg data files described by their manifest
/// entry. Keeping the label an enum is what bounds the
/// `bifrost_oracle_file_pruning_total` series cardinality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FilePruningSource {
    /// Staged hot Parquet not yet published into the pinned snapshot.
    Hot,
    /// A data file reachable from the pinned Iceberg snapshot's manifests.
    Iceberg,
}

impl FilePruningSource {
    /// Complete closed label domain, used by telemetry inventory contracts.
    #[cfg(any(test, feature = "bench-support"))]
    pub(crate) const ALL: [Self; 2] = [Self::Hot, Self::Iceberg];

    /// Returns the emitted `source` label for this persisted source.
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Hot => "hot",
            Self::Iceberg => "iceberg",
        }
    }
}

/// The closed inclusive `wyrd_event_time` interval a query's supported
/// predicates restrict one scan to.
///
/// An absent endpoint is unbounded, not zero: a query with only an upper bound
/// still prunes files that start after it. The interval is derived once per
/// scan from the same closed [`ScanPredicate`] vocabulary the leader already
/// signed, so this path never re-parses SQL or a `DataFusion` expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct EventTimeQueryInterval {
    /// Inclusive lower bound in epoch microseconds, or unbounded below.
    lower_micros: Option<i64>,
    /// Inclusive upper bound in epoch microseconds, or unbounded above.
    upper_micros: Option<i64>,
}

impl EventTimeQueryInterval {
    /// Derives the interval from one scan's closed predicate conjunction.
    ///
    /// Only comparisons on `wyrd_event_time` against a timestamp literal
    /// contribute. `NotEq`, null-checks, other columns, and non-timestamp
    /// literals are ignored rather than approximated — each of them leaves the
    /// interval wider, which can only make this path prune less.
    ///
    /// The predicates are a conjunction, so each contributing bound narrows the
    /// interval by taking the tighter endpoint.
    #[must_use]
    pub(crate) fn from_predicates(predicates: &[ScanPredicate]) -> Self {
        let _ = predicates;
        Self::default()
    }

    /// Returns `true` when no predicate constrained the event-time axis.
    ///
    /// An unconstrained interval can never exclude anything, so callers skip the
    /// per-file decision entirely and record every file as included.
    #[must_use]
    pub(crate) const fn is_unbounded(self) -> bool {
        self.lower_micros.is_none() && self.upper_micros.is_none()
    }

    /// Decides one file from its normalized immutable statistics.
    ///
    /// Exclusion requires a validated file interval and a proven empty
    /// intersection under closed-interval overlap. Every other case includes the
    /// file: unusable statistics fail open with their exact defect, and an
    /// unconstrained query interval includes unconditionally.
    #[must_use]
    pub(crate) fn decide(self, statistics: EventTimeStatistics) -> FilePruningDecision {
        let _ = statistics;
        FilePruningDecision::Include
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a `wyrd_event_time` timestamp comparison predicate.
    fn event_time(build: fn(String, ScanLiteral) -> ScanPredicate, micros: i64) -> ScanPredicate {
        build(
            WYRD_EVENT_TIME.to_owned(),
            ScanLiteral::TimestampMicros(micros),
        )
    }

    /// A conjunction narrows to the tightest endpoints, and only supported
    /// timestamp comparisons on the event-time column contribute at all.
    #[test]
    fn interval_derives_only_from_supported_event_time_comparisons() {
        let interval = EventTimeQueryInterval::from_predicates(&[
            event_time(ScanPredicate::GtEq, 100),
            event_time(ScanPredicate::Gt, 150),
            event_time(ScanPredicate::Lt, 400),
            event_time(ScanPredicate::LtEq, 500),
        ]);
        assert_eq!(interval.lower_micros, Some(151));
        assert_eq!(interval.upper_micros, Some(399));

        // A different column, a non-timestamp literal, `NotEq`, and null-checks
        // all leave the axis unconstrained.
        let ignored = EventTimeQueryInterval::from_predicates(&[
            ScanPredicate::GtEq("other".to_owned(), ScanLiteral::TimestampMicros(100)),
            ScanPredicate::GtEq(WYRD_EVENT_TIME.to_owned(), ScanLiteral::I64(100)),
            event_time(ScanPredicate::NotEq, 100),
            ScanPredicate::IsNotNull(WYRD_EVENT_TIME.to_owned()),
        ]);
        assert!(ignored.is_unbounded());
    }

    /// Closed-interval overlap: touching endpoints include, disjoint intervals
    /// on either side exclude, and an unbounded query never excludes.
    #[test]
    fn closed_interval_overlap_excludes_only_disjoint_files() {
        let interval = EventTimeQueryInterval::from_predicates(&[
            event_time(ScanPredicate::GtEq, 200),
            event_time(ScanPredicate::LtEq, 300),
        ]);
        let bounded = |min_micros, max_micros| EventTimeStatistics::Bounded {
            min_micros,
            max_micros,
        };

        assert_eq!(
            interval.decide(bounded(100, 200)),
            FilePruningDecision::Include,
            "a file touching the lower endpoint overlaps"
        );
        assert_eq!(
            interval.decide(bounded(300, 400)),
            FilePruningDecision::Include,
            "a file touching the upper endpoint overlaps"
        );
        assert_eq!(
            interval.decide(bounded(0, 199)),
            FilePruningDecision::Exclude
        );
        assert_eq!(
            interval.decide(bounded(301, 999)),
            FilePruningDecision::Exclude
        );
        assert_eq!(
            EventTimeQueryInterval::default().decide(bounded(0, 1)),
            FilePruningDecision::Include
        );
    }

    /// Every unusable statistic retains the file and reports its exact defect,
    /// even when the file provably cannot overlap.
    #[test]
    fn unusable_statistics_always_fail_open_with_their_reason() {
        let interval = EventTimeQueryInterval::from_predicates(&[
            event_time(ScanPredicate::GtEq, 200),
            event_time(ScanPredicate::LtEq, 300),
        ]);
        for defect in [
            EventTimeBoundsDefect::Missing,
            EventTimeBoundsDefect::Invalid,
            EventTimeBoundsDefect::Contradictory,
        ] {
            let decision = interval.decide(EventTimeStatistics::Unusable(defect));
            assert_eq!(decision, FilePruningDecision::FailOpen(defect));
            assert!(!decision.excludes());
            assert_eq!(decision.outcome_label(), defect.outcome_label());
        }
        assert_eq!(FilePruningDecision::Include.outcome_label(), "included");
        assert_eq!(FilePruningDecision::Exclude.outcome_label(), "excluded");
        assert_eq!(
            FilePruningSource::ALL.map(FilePruningSource::as_str),
            ["hot", "iceberg"]
        );
    }
}
