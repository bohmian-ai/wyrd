//! Per-unit execution profiles: Bifrost's analog of `ClickHouse`
//! `ProfileEvents`.
//!
//! A profile follows one unit of work, an Oracle query or a Scribe write
//! request, and is emitted exactly once when that unit settles: global
//! counters for fleet-wide Pareto analysis and one DEBUG event for the single
//! unit. Each owner times its own steps; this module owns only the value type
//! and the emission, so every profile reads the same way in `/metrics`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use datafusion::physical_plan::metrics::{MetricValue, MetricsSet};

/// One profile event's accumulated value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProfileValue {
    /// Summed time, in nanoseconds.
    Nanoseconds(u64),
    /// Summed count.
    Count(u64),
}

impl ProfileValue {
    /// Classifies one `DataFusion` metric, keeping only additive times and counts.
    fn of(value: &MetricValue) -> Option<Self> {
        let amount = u64::try_from(value.as_usize()).unwrap_or(u64::MAX);
        match value {
            MetricValue::ElapsedCompute(_) | MetricValue::Time { .. } => {
                Some(Self::Nanoseconds(amount))
            }
            MetricValue::OutputRows(_)
            | MetricValue::OutputBytes(_)
            | MetricValue::OutputBatches(_)
            | MetricValue::SpillCount(_)
            | MetricValue::SpilledBytes(_)
            | MetricValue::SpilledRows(_)
            | MetricValue::Count { .. } => Some(Self::Count(amount)),
            _ => None,
        }
    }

    /// Adds every additive metric of one operator under `<operator>.<metric>`.
    pub(crate) fn fold(operator: &str, metrics: &MetricsSet, events: &mut BTreeMap<String, Self>) {
        for metric in metrics.iter() {
            let value = metric.value();
            let Some(addition) = Self::of(value) else {
                continue;
            };
            events
                .entry(format!("{operator}.{}", value.name()))
                .and_modify(|current| *current = current.plus(addition))
                .or_insert(addition);
        }
    }

    /// Sums two values of the same kind, saturating; a kind clash keeps `self`.
    const fn plus(self, other: Self) -> Self {
        match (self, other) {
            (Self::Nanoseconds(left), Self::Nanoseconds(right)) => {
                Self::Nanoseconds(left.saturating_add(right))
            }
            (Self::Count(left), Self::Count(right)) => Self::Count(left.saturating_add(right)),
            (kept, _) => kept,
        }
    }
}

/// The metric names one kind of profile is emitted under.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProfileFamily {
    /// Counter of nanosecond events, labelled by `event`.
    nanoseconds: &'static str,
    /// Counter of count events, labelled by `event`.
    counts: &'static str,
    /// Counter of emitted profiles, the divisor that turns a scrape delta
    /// into a mean per unit.
    profiles: &'static str,
    /// Message of the per-unit DEBUG event.
    message: &'static str,
}

impl ProfileFamily {
    /// Oracle query profiles, one per settled query attempt.
    pub(crate) const ORACLE_QUERY: Self = Self {
        nanoseconds: "oracle_query_profile_nanoseconds_total",
        counts: "oracle_query_profile_count_total",
        profiles: "oracle_query_profiles_total",
        message: "Oracle query profile",
    };

    /// Scribe write profiles, one per settled write request.
    pub(crate) const SCRIBE_WRITE: Self = Self {
        nanoseconds: "scribe_write_profile_nanoseconds_total",
        counts: "scribe_write_profile_count_total",
        profiles: "scribe_write_profiles_total",
        message: "Scribe write profile",
    };

    /// Emits one settled unit's `events`: each adds to this family's
    /// `event`-labelled counter of its kind, the profile counter increments
    /// once, and one DEBUG event renders the unit's events.
    pub(crate) fn emit(self, events: &BTreeMap<String, ProfileValue>) {
        let mut rendered = String::new();
        for (event, value) in events {
            let (family, amount, unit) = match *value {
                ProfileValue::Nanoseconds(amount) => (self.nanoseconds, amount, "ns"),
                ProfileValue::Count(amount) => (self.counts, amount, ""),
            };
            metrics::counter!(family, "event" => event.clone()).increment(amount);
            let _ = write!(rendered, "{event}={amount}{unit} ");
        }
        metrics::counter!(self.profiles).increment(1);
        tracing::debug!(profile = rendered.trim_end(), "{}", self.message);
    }
}
