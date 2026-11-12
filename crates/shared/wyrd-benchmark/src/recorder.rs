//! The standard [`metrics`] recorder of a benchmark server.
//!
//! [`install`](crate::install) makes [`BenchmarkRecorder`] the process's only
//! `metrics` recorder, so every existing `metrics::counter!`, `gauge!`, and
//! `histogram!` observation lands in the same registry as the explicit
//! request timings. Series are keyed by kind, name, and sorted labels; there
//! are at most [`MAX_METRIC_SERIES`], and a registration that would open
//! another is counted in `dropped`. Each series has its own lock, and a
//! repeated registration shares the series' existing handle, so an update
//! waits for nothing but another update of the same series.
//!
//! Every histogram records fixed-point billionths of its observations
//! ([`HistogramUnit::per_unit`]): a `_seconds` histogram's are nanoseconds
//! in the timing range, and any other's are billionths of its value in the
//! [`HIGHEST_METRIC`](crate::HIGHEST_METRIC) range, so a whole count, a
//! fractional ratio, and a decimal cost all keep their value to a billionth
//! and [`SIGNIFICANT_DIGITS`](crate::SIGNIFICANT_DIGITS). A value either
//! cannot hold (negative, non-finite, or out of range) is counted
//! `rejected`, never clamped.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use metrics::{
    Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, KeyName, Metadata, Recorder,
    SharedString, Unit,
};
use num_traits::ToPrimitive as _;
use serde::{Deserialize, Serialize};

use crate::{Buckets, REGISTRY, durations, locked, metric_values, read, record_into, write};

/// The most standard metric series one process keeps.
pub const MAX_METRIC_SERIES: usize = 1024;

/// A standard metric's kind, which indexes [`Metrics`] key slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A monotonic counter.
    Counter = 0,
    /// A gauge.
    Gauge = 1,
    /// A histogram.
    Histogram = 2,
}

/// What a standard histogram's integer values count: both units are
/// [`per_unit`](Self::per_unit) billionths of the original observation,
/// in different ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistogramUnit {
    /// Nanoseconds of a `_seconds` histogram, in the timing range.
    Nanoseconds,
    /// Billionths of any other histogram's values, in the
    /// [`HIGHEST_METRIC`](crate::HIGHEST_METRIC) range: a ratio of 0.25 is
    /// 250,000,000.
    Billionths,
}

impl HistogramUnit {
    /// Recorded integers per original unit: divide a bucket bound by it to
    /// read seconds or the metric's own value.
    #[must_use]
    pub const fn per_unit(self) -> u64 {
        1_000_000_000
    }

    /// A new empty histogram of this unit's range.
    fn empty(self) -> hdrhistogram::Histogram<u64> {
        match self {
            Self::Nanoseconds => durations(),
            Self::Billionths => metric_values(),
        }
    }
}

/// One standard metric series' cumulative state.
pub(crate) struct Metric {
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

    /// Records one histogram observation as the nearest billionth; a
    /// negative, non-finite, or out-of-range value is rejected.
    fn observe(&mut self, observed: f64) {
        let Value::Histogram { values, .. } = &mut self.value else {
            return;
        };
        let scaled = (observed * 1e9).round();
        let recorded = (scaled >= 0.0)
            .then(|| scaled.to_u64())
            .flatten()
            .is_some_and(|scaled| record_into(values, scaled));
        if !recorded {
            self.rejected += 1;
        }
    }

    /// This series in a snapshot, named `name` with `labels`.
    fn snapshot(&self, name: &str, labels: &BTreeMap<String, String>) -> MetricSnapshot {
        MetricSnapshot {
            name: name.to_owned(),
            labels: labels.clone(),
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

/// One standard metric series, its state behind its own lock: the handle
/// every registration of its key shares.
pub(crate) struct MetricSeries {
    /// The metric family name.
    name: String,
    /// Its labels, sorted by name.
    labels: BTreeMap<String, String>,
    /// Its kind.
    kind: Kind,
    /// Its cumulative state.
    metric: Mutex<Metric>,
}

impl MetricSeries {
    /// Applies `update` to this series under its own lock.
    fn update(&self, update: impl FnOnce(&mut Metric)) {
        update(&mut locked(&self.metric));
    }
}

/// The process's standard metric series and each registered key's series.
#[derive(Default)]
pub(crate) struct Metrics {
    /// Every series, in registration order.
    series: Vec<Arc<MetricSeries>>,
    /// Each registered key's series index, per [`Kind`].
    keys: HashMap<Key, [Option<usize>; 3]>,
}

impl Metrics {
    /// `key`'s registered `kind` series, if any.
    fn find(&self, kind: Kind, key: &Key) -> Option<Arc<MetricSeries>> {
        let index = self.keys.get(key)?[kind as usize]?;
        self.series.get(index).cloned()
    }

    /// `key`'s `kind` series, registering it on first sight; `None`, counted
    /// in `dropped`, once [`MAX_METRIC_SERIES`] are kept.
    fn register(
        &mut self,
        kind: Kind,
        key: &Key,
        dropped: &AtomicU64,
    ) -> Option<Arc<MetricSeries>> {
        if let Some(series) = self.find(kind, key) {
            return Some(series);
        }
        let name = key.name().to_owned();
        let labels: BTreeMap<String, String> = key
            .labels()
            .map(|label| (label.key().to_owned(), label.value().to_owned()))
            .collect();
        // ponytail: a linear scan on a key's first sight only; label-order
        // variants of one series then share its index.
        let existing = self.series.iter().position(|series| {
            series.kind == kind && series.name == name && series.labels == labels
        });
        let index = match existing {
            Some(index) => index,
            None if self.series.len() >= MAX_METRIC_SERIES => {
                dropped.fetch_add(1, Ordering::Relaxed);
                return None;
            }
            None => {
                let value = match kind {
                    Kind::Counter => Value::Counter(0),
                    Kind::Gauge => Value::Gauge {
                        value: 0.0,
                        peak: 0.0,
                    },
                    Kind::Histogram => {
                        let unit = if name.ends_with("_seconds") {
                            HistogramUnit::Nanoseconds
                        } else {
                            HistogramUnit::Billionths
                        };
                        Value::Histogram {
                            unit,
                            values: unit.empty(),
                        }
                    }
                };
                self.series.push(Arc::new(MetricSeries {
                    name,
                    labels,
                    kind,
                    metric: Mutex::new(Metric { rejected: 0, value }),
                }));
                self.series.len() - 1
            }
        };
        self.keys.entry(key.clone()).or_default()[kind as usize] = Some(index);
        self.series.get(index).cloned()
    }

    /// Every standard metric series' cumulative snapshot, each read under
    /// its own lock after `metrics` is released.
    pub(crate) fn snapshots(metrics: &std::sync::RwLock<Self>) -> Vec<MetricSnapshot> {
        let series = read(metrics).series.clone();
        series
            .iter()
            .map(|series| locked(&series.metric).snapshot(&series.name, &series.labels))
            .collect()
    }
}

/// The process's standard `metrics` recorder: every series lives in the
/// benchmark registry beside the request timings.
#[derive(Debug, Clone, Copy, Default)]
pub struct BenchmarkRecorder;

impl CounterFn for MetricSeries {
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

impl GaugeFn for MetricSeries {
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

impl HistogramFn for MetricSeries {
    /// Records `value` in the series' unit.
    fn record(&self, value: f64) {
        self.update(|metric| metric.observe(value));
    }
}

impl BenchmarkRecorder {
    /// The shared handle of `key`'s `kind` series, or `None` when the bound
    /// refused it. A registered key needs only a shared read.
    fn handle(kind: Kind, key: &Key) -> Option<Arc<MetricSeries>> {
        let registry = &*REGISTRY;
        if let Some(series) = read(&registry.metrics).find(kind, key) {
            return Some(series);
        }
        write(&registry.metrics).register(kind, key, &registry.dropped)
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
                IntervalValue::Histogram {
                    unit: *unit,
                    distribution: crate::interval_of(buckets, prior, || unit.empty())?,
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};
    use std::time::{Duration, Instant};

    /// The registered counter series `name`.
    fn registered(name: &'static str) -> Arc<super::MetricSeries> {
        crate::read(&crate::REGISTRY.metrics)
            .find(super::Kind::Counter, &metrics::Key::from_static_name(name))
            .expect("a registered counter")
    }

    /// While one registered series is held mid-update, an unrelated series
    /// still registers and records, and so does a grouped request timing,
    /// even while a snapshot waits on the held series; the held update and
    /// the snapshot then finish, and each total is exact.
    ///
    /// # Panics
    ///
    /// Panics when an unrelated update waits for the held series or for the
    /// snapshot, or a total is wrong.
    #[test]
    fn unrelated_series_record_while_one_series_is_held() {
        crate::install().expect("the only recorder");
        let before = crate::snapshot();
        metrics::counter!("bench_held_total").increment(1);
        let series = registered("bench_held_total");
        let shared = Arc::strong_count(&series);
        let held = crate::locked(&series.metric);
        let (done, finished) = mpsc::channel();
        // The timeouts only guard a deadlock; they are not latency bounds.
        let deadline = Instant::now() + Duration::from_secs(10);
        let progressed = std::thread::scope(|threads| {
            let snapshot = threads.spawn(crate::snapshot);
            // The snapshot has released the registry once it holds its own
            // reference to every series, and then waits on the held one.
            while Arc::strong_count(&series) == shared && Instant::now() < deadline {
                std::thread::yield_now();
            }
            let reached = Arc::strong_count(&series) > shared;
            threads.spawn(move || {
                metrics::counter!("bench_free_total").increment(2);
                metrics::histogram!("bench_free_seconds").record(0.001);
                crate::record_timing_parts(
                    &tracing::Span::none(),
                    "bench.free",
                    "success",
                    Duration::from_micros(2),
                    None,
                    &[("bench.free.part", Duration::from_micros(1))],
                );
                let _ = done.send(());
            });
            let progressed = finished.recv_timeout(Duration::from_secs(10)).is_ok();
            drop(held);
            let snapshot = snapshot.join().expect("the snapshot finishes");
            (reached, progressed, snapshot)
        });
        let (reached, progressed, during) = progressed;
        assert!(reached, "the snapshot never reached the held series");
        assert!(progressed, "an unrelated series waited for the held one");
        assert!(
            during.since(&before).is_ok(),
            "the waiting snapshot is whole"
        );
        metrics::counter!("bench_held_total").increment(1);
        let interval = crate::snapshot().since(&before).expect("one process");
        assert_eq!(interval.counter("bench_held_total", &[]), Some(2));
        assert_eq!(interval.counter("bench_free_total", &[]), Some(2));
        assert_eq!(
            interval
                .histogram("bench_free_seconds", &[])
                .map(|values| values.len()),
            Ok(1)
        );
        for boundary in ["bench.free", "bench.free.part"] {
            assert_eq!(
                interval
                    .series
                    .iter()
                    .find(|series| series.boundary == boundary)
                    .map(|series| series.count),
                Some(1),
                "{boundary}"
            );
        }
    }
}
