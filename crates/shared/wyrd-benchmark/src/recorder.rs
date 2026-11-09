//! The standard [`metrics`] recorder of a benchmark server.
//!
//! [`install`](crate::install) makes [`BenchmarkRecorder`] the process's only
//! `metrics` recorder, so every existing `metrics::counter!`, `gauge!`, and
//! `histogram!` observation lands in the same registry, under the same lock,
//! as the explicit request timings. Series are keyed by kind, name, and
//! sorted labels; there are at most [`MAX_METRIC_SERIES`], and a
//! registration that would open another is counted in `dropped`.
//!
//! A histogram whose name ends in `_seconds` records integer nanoseconds in
//! the timing range; any other histogram records non-negative integer values
//! exactly in the scan-fact range. A value either cannot hold (negative,
//! fractional, non-finite, or out of range) is counted `rejected`, never
//! rounded or clamped.

use std::collections::BTreeMap;

use metrics::{
    Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, KeyName, Metadata, Recorder,
    SharedString, Unit,
};
use num_traits::ToPrimitive as _;
use serde::{Deserialize, Serialize};

use crate::{Buckets, Registry, durations, facts, lock, record_into};

/// The most standard metric series one process keeps.
pub const MAX_METRIC_SERIES: usize = 1024;

/// A standard metric's kind, which indexes [`Registry::metric_keys`] slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A monotonic counter.
    Counter = 0,
    /// A gauge.
    Gauge = 1,
    /// A histogram.
    Histogram = 2,
}

/// What a standard histogram's integer values count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistogramUnit {
    /// Nanoseconds of a `_seconds` histogram, in the timing range.
    Nanoseconds,
    /// The recorded values themselves, in the scan-fact range.
    Value,
}

/// One standard metric series' cumulative state.
pub(crate) struct Metric {
    /// The metric family name.
    name: String,
    /// Its labels, sorted by name.
    labels: BTreeMap<String, String>,
    /// Updates refused as unrepresentable.
    rejected: u64,
    /// The kind-specific value.
    value: Value,
}

/// A standard metric's kind-specific cumulative value.
enum Value {
    /// A counter's total.
    Counter(u64),
    /// A gauge's current value and the highest it held since registration.
    Gauge {
        /// Current value.
        value: f64,
        /// Highest value since registration, starting at zero.
        peak: f64,
    },
    /// A histogram's observations.
    Histogram {
        /// What its values count.
        unit: HistogramUnit,
        /// Recorded values.
        values: hdrhistogram::Histogram<u64>,
    },
}

impl Metric {
    /// Applies `update` to a gauge, keeping its peak; a non-finite result is
    /// rejected.
    fn gauge(&mut self, update: impl FnOnce(f64) -> f64) {
        if let Value::Gauge { value, peak } = &mut self.value {
            let next = update(*value);
            if next.is_finite() {
                *value = next;
                *peak = peak.max(next);
            } else {
                self.rejected += 1;
            }
        }
    }

    /// Records one histogram observation in its unit.
    fn observe(&mut self, observed: f64) {
        let Value::Histogram { unit, values } = &mut self.value else {
            return;
        };
        let integer = match unit {
            HistogramUnit::Nanoseconds => (observed * 1e9).round(),
            HistogramUnit::Value => observed,
        };
        let recorded = (integer >= 0.0 && integer.fract() == 0.0)
            .then(|| integer.to_u64())
            .flatten()
            .is_some_and(|integer| record_into(values, integer));
        if !recorded {
            self.rejected += 1;
        }
    }

    /// This series in a snapshot.
    fn snapshot(&self) -> MetricSnapshot {
        MetricSnapshot {
            name: self.name.clone(),
            labels: self.labels.clone(),
            rejected: self.rejected,
            value: match &self.value {
                Value::Counter(total) => MetricValue::Counter { total: *total },
                Value::Gauge { value, peak } => MetricValue::Gauge {
                    value: *value,
                    peak: *peak,
                },
                Value::Histogram { unit, values } => MetricValue::Histogram {
                    unit: *unit,
                    count: values.len(),
                    buckets: Buckets::of(values),
                },
            },
        }
    }
}

impl Registry {
    /// The index of `key`'s `kind` series, registering it on first sight;
    /// `None`, counted dropped, once [`MAX_METRIC_SERIES`] are kept.
    pub(crate) fn metric(&mut self, kind: Kind, key: &Key) -> Option<usize> {
        if let Some(index) = self
            .metric_keys
            .get(key)
            .and_then(|slots| slots[kind as usize])
        {
            return Some(index);
        }
        let name = key.name().to_owned();
        let labels: BTreeMap<String, String> = key
            .labels()
            .map(|label| (label.key().to_owned(), label.value().to_owned()))
            .collect();
        // ponytail: a linear scan on a key's first sight only; label-order
        // variants of one series then share its index.
        let existing = self.metrics.iter().position(|metric| {
            metric.name == name
                && metric.labels == labels
                && matches!(
                    (&metric.value, kind),
                    (Value::Counter(_), Kind::Counter)
                        | (Value::Gauge { .. }, Kind::Gauge)
                        | (Value::Histogram { .. }, Kind::Histogram)
                )
        });
        let index = match existing {
            Some(index) => index,
            None if self.metrics.len() >= MAX_METRIC_SERIES => {
                self.dropped += 1;
                return None;
            }
            None => {
                let value = match kind {
                    Kind::Counter => Value::Counter(0),
                    Kind::Gauge => Value::Gauge {
                        value: 0.0,
                        peak: 0.0,
                    },
                    Kind::Histogram if name.ends_with("_seconds") => Value::Histogram {
                        unit: HistogramUnit::Nanoseconds,
                        values: durations(),
                    },
                    Kind::Histogram => Value::Histogram {
                        unit: HistogramUnit::Value,
                        values: facts(),
                    },
                };
                self.metrics.push(Metric {
                    name,
                    labels,
                    rejected: 0,
                    value,
                });
                self.metrics.len() - 1
            }
        };
        self.metric_keys.entry(key.clone()).or_default()[kind as usize] = Some(index);
        Some(index)
    }

    /// Every standard metric series' cumulative snapshot.
    pub(crate) fn metric_snapshots(&self) -> Vec<MetricSnapshot> {
        self.metrics.iter().map(Metric::snapshot).collect()
    }
}

/// The process's standard `metrics` recorder: every series lives in the
/// benchmark registry beside the request timings.
#[derive(Debug, Clone, Copy, Default)]
pub struct BenchmarkRecorder;

/// A registered series' handle: its index in the registry.
struct Handle(usize);

impl Handle {
    /// Applies `update` to this series under the registry lock.
    fn update(&self, update: impl FnOnce(&mut Metric)) {
        if let Some(metric) = lock().metrics.get_mut(self.0) {
            update(metric);
        }
    }
}

impl CounterFn for Handle {
    /// Adds `value`, saturating.
    fn increment(&self, value: u64) {
        self.update(|metric| {
            if let Value::Counter(total) = &mut metric.value {
                *total = total.saturating_add(value);
            }
        });
    }

    /// Raises the total to `value`; a counter never decreases.
    fn absolute(&self, value: u64) {
        self.update(|metric| {
            if let Value::Counter(total) = &mut metric.value {
                *total = (*total).max(value);
            }
        });
    }
}

impl GaugeFn for Handle {
    /// Adds `value`.
    fn increment(&self, value: f64) {
        self.update(|metric| metric.gauge(|current| current + value));
    }

    /// Subtracts `value`.
    fn decrement(&self, value: f64) {
        self.update(|metric| metric.gauge(|current| current - value));
    }

    /// Sets `value`.
    fn set(&self, value: f64) {
        self.update(|metric| metric.gauge(|_| value));
    }
}

impl HistogramFn for Handle {
    /// Records `value` in the series' unit.
    fn record(&self, value: f64) {
        self.update(|metric| metric.observe(value));
    }
}

impl BenchmarkRecorder {
    /// The handle of `key`'s `kind` series, or `None` when the bound refused it.
    fn handle(kind: Kind, key: &Key) -> Option<std::sync::Arc<Handle>> {
        lock()
            .metric(kind, key)
            .map(|index| std::sync::Arc::new(Handle(index)))
    }
}

impl Recorder for BenchmarkRecorder {
    /// Descriptions are a production exporter concern.
    fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    /// Descriptions are a production exporter concern.
    fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    /// Descriptions are a production exporter concern.
    fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    /// Registers `key`'s counter series.
    fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
        Self::handle(Kind::Counter, key).map_or_else(Counter::noop, Counter::from_arc)
    }

    /// Registers `key`'s gauge series.
    fn register_gauge(&self, key: &Key, _: &Metadata<'_>) -> Gauge {
        Self::handle(Kind::Gauge, key).map_or_else(Gauge::noop, Gauge::from_arc)
    }

    /// Registers `key`'s histogram series.
    fn register_histogram(&self, key: &Key, _: &Metadata<'_>) -> Histogram {
        Self::handle(Kind::Histogram, key).map_or_else(Histogram::noop, Histogram::from_arc)
    }
}

/// One standard metric series in a snapshot, cumulative since registration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricSnapshot {
    /// The metric family name.
    pub name: String,
    /// Its labels.
    pub labels: BTreeMap<String, String>,
    /// Updates refused as unrepresentable.
    pub rejected: u64,
    /// The kind-specific value.
    pub value: MetricValue,
}

/// A standard metric series' cumulative value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MetricValue {
    /// A counter.
    Counter {
        /// Its total.
        total: u64,
    },
    /// A gauge.
    Gauge {
        /// Its value at the snapshot.
        value: f64,
        /// The highest value it held since registration, starting at zero.
        peak: f64,
    },
    /// A histogram.
    Histogram {
        /// What its values count.
        unit: HistogramUnit,
        /// Observations recorded.
        count: u64,
        /// Its nonzero buckets.
        buckets: Buckets,
    },
}

impl MetricSnapshot {
    /// Whether this series is `name` and carries every `(label, value)` of
    /// `labels`.
    #[must_use]
    pub fn matches(&self, name: &str, labels: &[(&str, &str)]) -> bool {
        self.name == name
            && labels
                .iter()
                .all(|(label, value)| self.labels.get(*label).is_some_and(|held| held == value))
    }
}

/// One standard metric series over an interval.
#[derive(Debug, Clone)]
pub struct IntervalMetric {
    /// The metric family name.
    pub name: String,
    /// Its labels.
    pub labels: BTreeMap<String, String>,
    /// Updates refused as unrepresentable in the interval.
    pub rejected: u64,
    /// The kind-specific interval value.
    pub value: IntervalValue,
}

/// A standard metric series' value over an interval.
#[derive(Debug, Clone)]
pub enum IntervalValue {
    /// A counter's increase.
    Counter(u64),
    /// A gauge, which no difference describes: its value at the later
    /// snapshot and the highest it held since registration.
    Gauge {
        /// Value at the later snapshot.
        value: f64,
        /// Highest value since registration, starting at zero.
        peak: f64,
    },
    /// A histogram's observations in the interval.
    Histogram {
        /// What its values count.
        unit: HistogramUnit,
        /// The interval's observations.
        distribution: crate::Distribution,
    },
}

impl IntervalMetric {
    /// Every series of `later` since `earlier`; a series new in `later`
    /// counts from zero.
    ///
    /// # Errors
    ///
    /// Returns a reset message when a series vanished, changed kind or unit,
    /// or any count or bucket went backwards.
    pub(crate) fn between_all(
        later: &[MetricSnapshot],
        earlier: &[MetricSnapshot],
    ) -> Result<Vec<Self>, String> {
        let same = |a: &MetricSnapshot, b: &MetricSnapshot| {
            a.name == b.name
                && a.labels == b.labels
                && std::mem::discriminant(&a.value) == std::mem::discriminant(&b.value)
        };
        if let Some(gone) = earlier
            .iter()
            .find(|prior| !later.iter().any(|series| same(series, prior)))
        {
            return Err(format!(
                "metric `{}` vanished: the recorder was reset",
                gone.name
            ));
        }
        later
            .iter()
            .map(|series| Self::between(series, earlier.iter().find(|prior| same(series, prior))))
            .collect()
    }

    /// `later` since `earlier`, `earlier` absent counting from zero.
    ///
    /// # Errors
    ///
    /// Returns a reset message when a count or bucket went backwards or a
    /// histogram's unit changed, or a range message when a bucket cannot
    /// rebuild.
    fn between(later: &MetricSnapshot, earlier: Option<&MetricSnapshot>) -> Result<Self, String> {
        let what = format!("metric `{}`", later.name);
        let rejected = crate::counted(
            later.rejected,
            earlier.map_or(0, |prior| prior.rejected),
            &what,
        )?;
        let value = match (&later.value, earlier.map(|prior| &prior.value)) {
            (MetricValue::Counter { total }, prior) => {
                let prior = match prior {
                    Some(MetricValue::Counter { total }) => *total,
                    _ => 0,
                };
                IntervalValue::Counter(crate::counted(*total, prior, &what)?)
            }
            (MetricValue::Gauge { value, peak }, _) => IntervalValue::Gauge {
                value: *value,
                peak: *peak,
            },
            (MetricValue::Histogram { unit, buckets, .. }, prior) => {
                let prior = match prior {
                    Some(MetricValue::Histogram {
                        unit: prior_unit,
                        buckets,
                        ..
                    }) if prior_unit == unit => Some(buckets),
                    Some(_) => return Err(format!("{what} changed unit: the recorder was reset")),
                    None => None,
                };
                let empty = match unit {
                    HistogramUnit::Nanoseconds => durations,
                    HistogramUnit::Value => facts,
                };
                IntervalValue::Histogram {
                    unit: *unit,
                    distribution: crate::interval_of(buckets, prior, empty)?,
                }
            }
        };
        Ok(Self {
            name: later.name.clone(),
            labels: later.labels.clone(),
            rejected,
            value,
        })
    }

    /// Whether this series is `name` and carries every `(label, value)` of
    /// `labels`.
    #[must_use]
    pub fn matches(&self, name: &str, labels: &[(&str, &str)]) -> bool {
        self.name == name
            && labels
                .iter()
                .all(|(label, value)| self.labels.get(*label).is_some_and(|held| held == value))
    }
}

impl IntervalMetric {
    /// The summed increase of every `name` counter series of `metrics`
    /// carrying `labels`; `None` when no such series was registered, which
    /// is missing evidence, not zero.
    #[must_use]
    pub fn counter(metrics: &[Self], name: &str, labels: &[(&str, &str)]) -> Option<u64> {
        metrics
            .iter()
            .filter(|metric| metric.matches(name, labels))
            .filter_map(|metric| match metric.value {
                IntervalValue::Counter(increase) => Some(increase),
                _ => None,
            })
            .reduce(u64::saturating_add)
    }

    /// The merged interval observations of every `name` histogram series of
    /// `metrics` carrying `labels`.
    ///
    /// # Errors
    ///
    /// Returns why the selection is not evidence: no such series was
    /// registered, or a selected series refused an unrepresentable value.
    pub fn histogram(
        metrics: &[Self],
        name: &str,
        labels: &[(&str, &str)],
    ) -> Result<crate::Distribution, String> {
        let mut merged: Option<crate::Distribution> = None;
        for metric in metrics.iter().filter(|metric| metric.matches(name, labels)) {
            let IntervalValue::Histogram { distribution, .. } = &metric.value else {
                continue;
            };
            if metric.rejected > 0 {
                return Err(format!(
                    "histogram `{name}` refused {} unrepresentable values",
                    metric.rejected
                ));
            }
            match &mut merged {
                Some(merged) => merged.add(distribution)?,
                None => merged = Some(distribution.clone()),
            }
        }
        merged.ok_or_else(|| format!("no `{name}` histogram {labels:?} was recorded"))
    }
}

impl crate::BenchmarkInterval {
    /// [`IntervalMetric::counter`] over this interval's metrics.
    #[must_use]
    pub fn counter(&self, name: &str, labels: &[(&str, &str)]) -> Option<u64> {
        IntervalMetric::counter(&self.metrics, name, labels)
    }

    /// [`IntervalMetric::histogram`] over this interval's metrics.
    ///
    /// # Errors
    ///
    /// Returns [`IntervalMetric::histogram`]'s refusal.
    pub fn histogram(
        &self,
        name: &str,
        labels: &[(&str, &str)],
    ) -> Result<crate::Distribution, String> {
        IntervalMetric::histogram(&self.metrics, name, labels)
    }
}
