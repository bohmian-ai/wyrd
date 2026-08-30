//! Normalized immutable `wyrd_event_time` statistics for one persisted file.
//!
//! Both persisted sources declare an event-time interval for every object they
//! produce: a pinned Iceberg manifest entry carries it as typed `DataFile`
//! lower/upper bounds, and an unresolved `vala.file_list` row carries it as two
//! durable timestamp columns. Oracle uses that interval to exclude a provably
//! non-overlapping file before it opens a footer, so the two sources must
//! normalize into one value with one closed set of failure reasons.
//!
//! Normalization is deliberately fail-open. A file whose declared bounds are
//! absent, undecodable, or self-contradictory is retained and scanned; only a
//! validated, ordered interval may ever exclude anything. Pruning that guessed
//! would drop rows, which is a correctness failure, whereas pruning that
//! declines costs one file read.

use chrono::{DateTime, Utc};

/// Closed reason one file's declared event-time bounds cannot prune.
///
/// Each variant is a distinct production signal: the three failure classes have
/// different operational causes (a writer that never recorded statistics, a
/// manifest whose literal does not decode as a UTC timestamp, and an interval
/// whose lower bound follows its upper bound), and collapsing them would hide
/// which one a deployment is actually hitting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventTimeBoundsDefect {
    /// One or both bounds were absent from the source's statistics.
    Missing,
    /// A bound was present but did not decode as a UTC microsecond timestamp
    /// of the expected Iceberg type.
    Invalid,
    /// Both bounds decoded, but the lower bound follows the upper bound.
    Contradictory,
}

impl EventTimeBoundsDefect {
    /// Returns the closed telemetry outcome label for this defect.
    ///
    /// The values are the fail-open members of the Oracle file-pruning outcome
    /// inventory and must stay byte-identical to the emitted metric labels.
    #[must_use]
    pub const fn outcome_label(self) -> &'static str {
        match self {
            Self::Missing => "fail_open_missing_bounds",
            Self::Invalid => "fail_open_invalid_bounds",
            Self::Contradictory => "fail_open_contradictory_bounds",
        }
    }
}

/// One persisted file's normalized event-time statistics.
///
/// Making the usable and unusable cases separate variants means a caller can
/// never read a half-present interval: either both endpoints are known and
/// ordered, or the file carries the exact reason it must be retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventTimeStatistics {
    /// A validated inclusive interval in epoch microseconds, `min <= max`.
    Bounded {
        /// Inclusive lower bound in epoch microseconds.
        min_micros: i64,
        /// Inclusive upper bound in epoch microseconds, never below `min_micros`.
        max_micros: i64,
    },
    /// The declared bounds cannot prune; the file is always retained.
    Unusable(EventTimeBoundsDefect),
}

impl EventTimeStatistics {
    /// Normalizes one already-decoded microsecond pair.
    ///
    /// Absent endpoints (including exactly one present) normalize to
    /// [`EventTimeBoundsDefect::Missing`]; a reversed interval normalizes to
    /// [`EventTimeBoundsDefect::Contradictory`]. Decode failures are the
    /// caller's to classify because only the caller knows whether a literal was
    /// the wrong Iceberg type or out of Chrono's representable range.
    #[must_use]
    pub fn normalize(min_micros: Option<i64>, max_micros: Option<i64>) -> Self {
        match (min_micros, max_micros) {
            (Some(min_micros), Some(max_micros)) if min_micros <= max_micros => Self::Bounded {
                min_micros,
                max_micros,
            },
            (Some(_), Some(_)) => Self::Unusable(EventTimeBoundsDefect::Contradictory),
            _ => Self::Unusable(EventTimeBoundsDefect::Missing),
        }
    }

    /// Normalizes one durable `vala.file_list` timestamp pair.
    ///
    /// A timestamp outside the microsecond domain is [`EventTimeBoundsDefect::Invalid`]
    /// rather than missing: the row did record evidence, it simply cannot be
    /// represented, and conflating the two would hide a writer producing
    /// unusable statistics.
    #[must_use]
    pub fn from_catalog_timestamps(
        min_event_time: Option<DateTime<Utc>>,
        max_event_time: Option<DateTime<Utc>>,
    ) -> Self {
        match (
            representable_micros(min_event_time),
            representable_micros(max_event_time),
        ) {
            (Ok(min_micros), Ok(max_micros)) => Self::normalize(min_micros, max_micros),
            _ => Self::Unusable(EventTimeBoundsDefect::Invalid),
        }
    }

    /// Borrows the validated interval, or `None` when the file must be retained.
    #[must_use]
    pub const fn interval(self) -> Option<(i64, i64)> {
        match self {
            Self::Bounded {
                min_micros,
                max_micros,
            } => Some((min_micros, max_micros)),
            Self::Unusable(_) => None,
        }
    }

    /// Returns the fail-open defect, or `None` for a usable interval.
    #[must_use]
    pub const fn defect(self) -> Option<EventTimeBoundsDefect> {
        match self {
            Self::Bounded { .. } => None,
            Self::Unusable(defect) => Some(defect),
        }
    }

    /// Projects the interval into the optional microsecond pair carried by a
    /// signed follower descriptor.
    ///
    /// Unusable statistics project to the absent pair, which is the descriptor's
    /// only representation of "retain this file"; a half-present pair is
    /// rejected by descriptor validation and therefore never produced here.
    #[must_use]
    pub const fn descriptor_pair(self) -> (Option<i64>, Option<i64>) {
        match self {
            Self::Bounded {
                min_micros,
                max_micros,
            } => (Some(min_micros), Some(max_micros)),
            Self::Unusable(_) => (None, None),
        }
    }
}

/// Converts one optional timestamp into epoch microseconds, rejecting a value
/// outside the representable microsecond domain.
///
/// `chrono` saturates rather than failing on out-of-range microsecond
/// conversion, so the result is round-tripped instead of trusted: a saturated
/// bound would silently widen or narrow the interval a reader prunes with.
///
/// # Errors
/// Returns `Err(())` when the timestamp does not round-trip through its
/// microsecond representation.
fn representable_micros(value: Option<DateTime<Utc>>) -> Result<Option<i64>, ()> {
    let Some(value) = value else {
        return Ok(None);
    };
    let micros = value.timestamp_micros();
    match DateTime::from_timestamp_micros(micros) {
        Some(round_trip) if round_trip == value => Ok(Some(micros)),
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both endpoints present and ordered are the only shape that may prune;
    /// every other shape carries its exact fail-open reason.
    #[test]
    fn normalization_is_fail_open_with_closed_reasons() {
        assert_eq!(
            EventTimeStatistics::normalize(Some(10), Some(20)),
            EventTimeStatistics::Bounded {
                min_micros: 10,
                max_micros: 20
            }
        );
        assert_eq!(
            EventTimeStatistics::normalize(Some(7), Some(7)),
            EventTimeStatistics::Bounded {
                min_micros: 7,
                max_micros: 7
            }
        );
        assert_eq!(
            EventTimeStatistics::normalize(Some(20), Some(10)),
            EventTimeStatistics::Unusable(EventTimeBoundsDefect::Contradictory)
        );
        assert_eq!(
            EventTimeStatistics::normalize(Some(10), None),
            EventTimeStatistics::Unusable(EventTimeBoundsDefect::Missing)
        );
        assert_eq!(
            EventTimeStatistics::normalize(None, None),
            EventTimeStatistics::Unusable(EventTimeBoundsDefect::Missing)
        );
    }

    /// A durable timestamp pair projects to the same normalized value, and an
    /// unusable interval always projects to the absent descriptor pair.
    #[test]
    fn catalog_timestamps_project_into_the_descriptor_pair() {
        let lower = DateTime::from_timestamp_micros(1_787_493_600_000_000).expect("representable");
        let upper = DateTime::from_timestamp_micros(1_787_497_200_000_000).expect("representable");
        let bounded = EventTimeStatistics::from_catalog_timestamps(Some(lower), Some(upper));
        assert_eq!(
            bounded.descriptor_pair(),
            (Some(1_787_493_600_000_000), Some(1_787_497_200_000_000))
        );
        assert_eq!(bounded.defect(), None);

        let reversed = EventTimeStatistics::from_catalog_timestamps(Some(upper), Some(lower));
        assert_eq!(reversed.descriptor_pair(), (None, None));
        assert_eq!(
            reversed.defect(),
            Some(EventTimeBoundsDefect::Contradictory)
        );
    }

    /// The three defects keep distinct closed labels so an operator can tell a
    /// writer that records nothing from one that records something unusable.
    #[test]
    fn defect_labels_are_distinct_and_closed() {
        assert_eq!(
            EventTimeBoundsDefect::Missing.outcome_label(),
            "fail_open_missing_bounds"
        );
        assert_eq!(
            EventTimeBoundsDefect::Invalid.outcome_label(),
            "fail_open_invalid_bounds"
        );
        assert_eq!(
            EventTimeBoundsDefect::Contradictory.outcome_label(),
            "fail_open_contradictory_bounds"
        );
    }
}
