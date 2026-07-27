//! Bounded standard-metrics recorder used by benchmark processes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hdrhistogram::Histogram;
use metrics::{Counter, CounterFn, Gauge, GaugeFn, Histogram as MetricHistogram, HistogramFn};
use metrics::{Key, KeyName, Metadata, Recorder, SharedString, Unit};
use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};

const MAX_SERIES: usize = 4096;
const HISTOGRAM_SCALE: f64 = 1_000_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SeriesKind {
    Counter,
    Gauge,
    Histogram,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SeriesKey {
    key: Key,
    kind: SeriesKind,
}

#[derive(Debug)]
enum SeriesValue {
    Counter(Arc<AtomicU64>),
    Gauge {
        value: Arc<AtomicU64>,
        peak: Arc<AtomicU64>,
    },
    Histogram(Arc<Mutex<Histogram<u64>>>),
}

#[derive(Debug, Default)]
struct RegistryState {
    series: HashMap<SeriesKey, SeriesValue>,
}

/// A standard `metrics::Recorder` with bounded series cardinality and HDR data.
#[derive(Debug, Clone)]
pub struct BenchmarkRecorder {
    state: Arc<Mutex<RegistryState>>,
    series_limit_exceeded: Arc<AtomicU64>,
}

/// One histogram summary captured from a benchmark recorder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistogramSnapshot {
    pub count: u64,
    pub p50: u64,
    pub p95: u64,
    pub p99: u64,
    pub max: u64,
}

/// Detached metric data used to build machine-readable benchmark reports.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkMetricSnapshot {
    pub counters: HashMap<String, u64>,
    pub gauges: HashMap<String, f64>,
    pub gauge_peaks: HashMap<String, f64>,
    pub histograms: HashMap<String, HistogramSnapshot>,
    pub series: usize,
    pub series_limit_exceeded: bool,
}

impl BenchmarkMetricSnapshot {
    /// Return whether any recorded series belongs to a metric family.
    #[must_use]
    pub fn contains_family(&self, family: &str) -> bool {
        self.counters
            .keys()
            .chain(self.gauges.keys())
            .chain(self.gauge_peaks.keys())
            .chain(self.histograms.keys())
            .any(|key| key == family || key.starts_with(&format!("{family}{{")))
    }
}

impl BenchmarkRecorder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install this recorder as the process-wide standard metrics recorder.
    pub fn install(self) -> Result<Arc<Self>, metrics::SetRecorderError<Arc<Self>>> {
        let recorder = Arc::new(self);
        metrics::set_global_recorder(Arc::clone(&recorder))?;
        Ok(recorder)
    }

    /// Capture all currently registered values without exposing mutable state.
    #[must_use]
    pub fn snapshot(&self) -> BenchmarkMetricSnapshot {
        let Ok(state) = self.state.lock() else {
            return BenchmarkMetricSnapshot::default();
        };
        let mut snapshot = BenchmarkMetricSnapshot {
            series: state.series.len(),
            series_limit_exceeded: self.series_limit_exceeded.load(Ordering::Relaxed) != 0,
            ..BenchmarkMetricSnapshot::default()
        };
        for (series, value) in &state.series {
            let name = metric_key_name(&series.key);
            match value {
                SeriesValue::Counter(value) => {
                    snapshot
                        .counters
                        .insert(name, value.load(Ordering::Relaxed));
                }
                SeriesValue::Gauge { value, peak } => {
                    snapshot
                        .gauges
                        .insert(name.clone(), f64::from_bits(value.load(Ordering::Relaxed)));
                    snapshot
                        .gauge_peaks
                        .insert(name, f64::from_bits(peak.load(Ordering::Relaxed)));
                }
                SeriesValue::Histogram(value) => {
                    let Ok(histogram) = value.lock() else {
                        continue;
                    };
                    snapshot.histograms.insert(
                        name,
                        HistogramSnapshot {
                            count: histogram.len(),
                            p50: histogram.value_at_quantile(0.50),
                            p95: histogram.value_at_quantile(0.95),
                            p99: histogram.value_at_quantile(0.99),
                            max: histogram.max(),
                        },
                    );
                }
            }
        }
        snapshot
    }

    /// Return an error when a required production metric was not observed.
    pub fn require_metrics(&self, names: &[&str]) -> Result<(), String> {
        let snapshot = self.snapshot();
        if snapshot.series_limit_exceeded {
            return Err(format!("benchmark metric series exceeded {MAX_SERIES}"));
        }
        let observed = |name: &str| {
            let matches = |candidate: &String| {
                *candidate == name || candidate.starts_with(&format!("{name}{{"))
            };
            snapshot.counters.keys().any(matches)
                || snapshot.gauges.keys().any(matches)
                || snapshot.histograms.keys().any(matches)
        };
        let missing = names
            .iter()
            .filter(|name| !observed(name))
            .copied()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            Ok(())
        } else {
            let mut observed_names = snapshot
                .counters
                .keys()
                .chain(snapshot.gauges.keys())
                .chain(snapshot.histograms.keys())
                .cloned()
                .collect::<Vec<_>>();
            observed_names.sort();
            Err(format!(
                "required benchmark metrics were not observed: {missing:?}; observed: {observed_names:?}"
            ))
        }
    }

    /// Register required metric families before a benchmark starts.
    ///
    /// Registration does not record a synthetic observation. It makes zero-valued
    /// counters, gauges, and empty histograms visible in the artifact so a report
    /// can distinguish an instrumented-but-unused failure path from missing
    /// instrumentation.
    pub fn register_metric_families(&self, names: &[String]) {
        for name in names {
            let kind = if name.ends_with("_seconds") {
                SeriesKind::Histogram
            } else if matches!(
                name.as_str(),
                "bifrost_scribe_inflight_items"
                    | "bifrost_scribe_inflight_bytes"
                    | "bifrost_scribe_lane_queued"
                    | "bifrost_scribe_lane_active"
                    | "bifrost_scribe_shard_pending"
            ) {
                SeriesKind::Gauge
            } else {
                SeriesKind::Counter
            };
            let key = Key::from_name(name.clone());
            let _ = self.register(&key, kind);
        }
    }

    /// Clear interval counters and histograms while retaining registered series.
    pub fn reset_interval(&self) {
        let Ok(state) = self.state.lock() else { return };
        for value in state.series.values() {
            match value {
                SeriesValue::Counter(value) => value.store(0, Ordering::Relaxed),
                SeriesValue::Gauge { value, peak } => {
                    let _ = value;
                    peak.store(0_f64.to_bits(), Ordering::Relaxed);
                }
                SeriesValue::Histogram(value) => {
                    if let Ok(mut histogram) = value.lock() {
                        histogram.reset();
                    }
                }
            }
        }
    }

    fn register(&self, key: &Key, kind: SeriesKind) -> Option<SeriesValue> {
        let mut state = self.state.lock().ok()?;
        let series_key = SeriesKey {
            key: key.clone(),
            kind,
        };
        if let Some(value) = state.series.get(&series_key) {
            return Some(match value {
                SeriesValue::Counter(value) => SeriesValue::Counter(Arc::clone(value)),
                SeriesValue::Gauge { value, peak } => SeriesValue::Gauge {
                    value: Arc::clone(value),
                    peak: Arc::clone(peak),
                },
                SeriesValue::Histogram(value) => SeriesValue::Histogram(Arc::clone(value)),
            });
        }
        if state.series.len() >= MAX_SERIES {
            self.series_limit_exceeded.store(1, Ordering::Relaxed);
            return None;
        }
        let value = match kind {
            SeriesKind::Counter => SeriesValue::Counter(Arc::new(AtomicU64::new(0))),
            SeriesKind::Gauge => {
                let value = Arc::new(AtomicU64::new(0_f64.to_bits()));
                SeriesValue::Gauge {
                    value: Arc::clone(&value),
                    peak: Arc::new(AtomicU64::new(0_f64.to_bits())),
                }
            }
            SeriesKind::Histogram => SeriesValue::Histogram(Arc::new(Mutex::new(
                Histogram::new_with_bounds(1, u64::MAX / 2, 3)
                    .expect("HDR histogram bounds are valid"),
            ))),
        };
        let result = match &value {
            SeriesValue::Counter(value) => SeriesValue::Counter(Arc::clone(value)),
            SeriesValue::Gauge { value, peak } => SeriesValue::Gauge {
                value: Arc::clone(value),
                peak: Arc::clone(peak),
            },
            SeriesValue::Histogram(value) => SeriesValue::Histogram(Arc::clone(value)),
        };
        state.series.insert(series_key, value);
        Some(result)
    }
}

fn metric_key_name(key: &Key) -> String {
    let mut name = key.name().to_owned();
    let mut labels = key.labels().collect::<Vec<_>>();
    labels.sort_by(|left, right| {
        left.key()
            .cmp(right.key())
            .then(left.value().cmp(right.value()))
    });
    let mut first = true;
    for label in labels {
        if first {
            name.push('{');
            first = false;
        } else {
            name.push(',');
        }
        name.push_str(label.key());
        name.push_str("=\"");
        name.push_str(label.value());
        name.push('"');
    }
    if !first {
        name.push('}');
    }
    name
}

impl Default for BenchmarkRecorder {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(RegistryState::default())),
            series_limit_exceeded: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl Recorder for BenchmarkRecorder {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}
    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}
    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        match self.register(key, SeriesKind::Counter) {
            Some(SeriesValue::Counter(value)) => Counter::from_arc(Arc::new(CounterValue(value))),
            _ => Counter::noop(),
        }
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        match self.register(key, SeriesKind::Gauge) {
            Some(SeriesValue::Gauge { value, peak }) => {
                Gauge::from_arc(Arc::new(GaugeValue { value, peak }))
            }
            _ => Gauge::noop(),
        }
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> MetricHistogram {
        match self.register(key, SeriesKind::Histogram) {
            Some(SeriesValue::Histogram(value)) => MetricHistogram::from_arc(Arc::new(
                HistogramValue(value, key.name().to_string().ends_with("_seconds")),
            )),
            _ => MetricHistogram::noop(),
        }
    }
}

#[derive(Debug)]
struct CounterValue(Arc<AtomicU64>);

impl CounterFn for CounterValue {
    fn increment(&self, value: u64) {
        self.0.fetch_add(value, Ordering::Relaxed);
    }

    fn absolute(&self, value: u64) {
        self.0.fetch_max(value, Ordering::Relaxed);
    }
}

#[derive(Debug)]
struct GaugeValue {
    value: Arc<AtomicU64>,
    peak: Arc<AtomicU64>,
}

impl GaugeFn for GaugeValue {
    fn increment(&self, value: f64) {
        self.update(|current| current + value);
    }

    fn decrement(&self, value: f64) {
        self.update(|current| current - value);
    }

    fn set(&self, value: f64) {
        self.value.store(value.to_bits(), Ordering::Relaxed);
        update_peak(&self.peak, value);
    }
}

impl GaugeValue {
    fn update(&self, update: impl Fn(f64) -> f64) {
        let mut current = self.value.load(Ordering::Relaxed);
        loop {
            let next_value = update(f64::from_bits(current));
            let next = next_value.to_bits();
            match self.value.compare_exchange_weak(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    update_peak(&self.peak, next_value);
                    return;
                }
                Err(actual) => current = actual,
            }
        }
    }
}

fn update_peak(peak: &AtomicU64, value: f64) {
    if !value.is_finite() {
        return;
    }
    let mut current = peak.load(Ordering::Relaxed);
    loop {
        if f64::from_bits(current) >= value {
            return;
        }
        match peak.compare_exchange_weak(
            current,
            value.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

#[derive(Debug)]
struct HistogramValue(Arc<Mutex<Histogram<u64>>>, bool);

impl HistogramFn for HistogramValue {
    fn record(&self, value: f64) {
        if value.is_finite() && value > 0.0 {
            let scale = if self.1 { HISTOGRAM_SCALE } else { 1.0 };
            let value = (value * scale).to_u64().unwrap_or(u64::MAX / 2);
            if let Ok(mut histogram) = self.0.lock() {
                let _ = histogram.record(value.max(1));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use metrics::{Key, Level, Metadata, Recorder};

    use super::*;

    fn metadata() -> Metadata<'static> {
        Metadata::new("wyrd-bench-test", Level::INFO, Some(module_path!()))
    }

    #[test]
    fn recorder_accumulates_counter_gauge_peak_and_histogram() {
        let recorder = BenchmarkRecorder::new();
        let metadata = metadata();
        let counter = recorder.register_counter(&Key::from_name("frames_total"), &metadata);
        counter.increment(3);
        counter.increment(2);
        let gauge = recorder.register_gauge(&Key::from_name("queue_depth"), &metadata);
        gauge.set(2.0);
        gauge.set(7.0);
        gauge.set(1.0);
        let histogram = recorder.register_histogram(&Key::from_name("ack_seconds"), &metadata);
        for value in 1..=100 {
            histogram.record(f64::from(value) / 1_000_000.0);
        }

        let snapshot = recorder.snapshot();
        assert_eq!(snapshot.counters["frames_total"], 5);
        assert_eq!(snapshot.gauges["queue_depth"].to_bits(), 1.0_f64.to_bits());
        assert_eq!(
            snapshot.gauge_peaks["queue_depth"].to_bits(),
            7.0_f64.to_bits()
        );
        assert_eq!(snapshot.histograms["ack_seconds"].count, 100);
        assert!(snapshot.histograms["ack_seconds"].p99 >= 99);
    }

    #[test]
    fn interval_reset_preserves_series_and_resets_values_and_peaks() {
        let recorder = BenchmarkRecorder::new();
        let metadata = metadata();
        let gauge = recorder.register_gauge(&Key::from_name("queue_depth"), &metadata);
        gauge.set(9.0);
        recorder.reset_interval();
        gauge.set(3.0);
        let snapshot = recorder.snapshot();
        assert_eq!(snapshot.series, 1);
        assert_eq!(snapshot.gauges["queue_depth"].to_bits(), 3.0_f64.to_bits());
        assert_eq!(
            snapshot.gauge_peaks["queue_depth"].to_bits(),
            3.0_f64.to_bits()
        );
    }

    #[test]
    fn normalized_labels_share_one_series() {
        let recorder = BenchmarkRecorder::new();
        let metadata = metadata();
        let first = Key::from_parts(
            "frames_total",
            vec![
                metrics::Label::new("status", "accepted"),
                metrics::Label::new("lane", "ingress_cpu"),
            ],
        );
        let second = Key::from_parts(
            "frames_total",
            vec![
                metrics::Label::new("lane", "ingress_cpu"),
                metrics::Label::new("status", "accepted"),
            ],
        );
        let counter = recorder.register_counter(&first, &metadata);
        counter.increment(1);
        let counter = recorder.register_counter(&second, &metadata);
        counter.increment(1);
        assert_eq!(recorder.snapshot().series, 1);
        assert_eq!(
            recorder.snapshot().counters.values().copied().sum::<u64>(),
            2
        );
    }

    #[test]
    fn missing_required_metric_and_series_overflow_fail_closed() {
        let recorder = BenchmarkRecorder::new();
        assert!(recorder.require_metrics(&["missing_metric"]).is_err());
        let metadata = metadata();
        for index in 0..=MAX_SERIES {
            let key = Key::from_name(format!("series_{index}"));
            let _ = recorder.register_counter(&key, &metadata);
        }
        assert!(recorder.snapshot().series_limit_exceeded);
        assert!(recorder.require_metrics(&[]).is_err());
    }
}
